//! Feature Flag Manager
//!
//! This module implements the main FeatureFlagManager that provides configuration loading,
//! flag evaluation, caching, and hot-reload capabilities.

use super::types::*;
use super::context::*;
use super::evaluation::*;
use super::cache::*;
use super::config::*;
use super::{FeatureFlagResult, FeatureFlagError};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use tracing::{info, warn, error, debug};

/// Main feature flag manager
pub struct FeatureFlagManager {
    /// Current feature flags
    flags: Arc<RwLock<HashMap<String, FeatureFlag>>>,
    
    /// Configuration file path
    config_path: PathBuf,
    
    /// Flag evaluation engine
    evaluator: FeatureFlagEvaluator,
    
    /// Evaluation cache
    cache: FeatureFlagCache,
    
    /// Configuration loader
    config_loader: FeatureFlagConfigLoader,
    
    /// File watcher for hot-reload (will be added in Phase 2)
    _file_watcher: Option<()>, // Placeholder for Phase 2
    
    /// Audit logger for flag changes
    audit_logger: AuditLogger,
    
    /// Global settings
    global_settings: FeatureFlagGlobalSettings,
    
    /// Manager start time
    started_at: Instant,
    
    /// Statistics
    stats: Arc<RwLock<ManagerStats>>,
}

impl FeatureFlagManager {
    /// Create a new feature flag manager
    pub fn new(config_path: PathBuf) -> FeatureFlagResult<Self> {
        let config_loader = FeatureFlagConfigLoader::new();
        let collection = config_loader.load_from_file(&config_path)?;
        
        let cache = FeatureFlagCache::new(collection.global_settings.cache_ttl_seconds);
        let evaluator = FeatureFlagEvaluator::with_timeout(
            collection.global_settings.max_evaluation_time_ms
        );
        
        let audit_logger = AuditLogger::new(collection.global_settings.enable_audit_log);
        
        Ok(Self {
            flags: Arc::new(RwLock::new(collection.flags)),
            config_path,
            evaluator,
            cache,
            config_loader,
            _file_watcher: None,
            audit_logger,
            global_settings: collection.global_settings,
            started_at: Instant::now(),
            stats: Arc::new(RwLock::new(ManagerStats::new())),
        })
    }
    
    /// Check if a feature flag is enabled for the given context
    pub async fn is_enabled(&self, flag_name: &str, context: &EvaluationContext) -> bool {
        match self.is_enabled_with_result(flag_name, context).await {
            Ok(enabled) => enabled,
            Err(err) => {
                error!("Failed to evaluate feature flag '{}': {}", flag_name, err);
                // Update error stats
                if let Ok(mut stats) = self.stats.write().await {
                    stats.evaluation_errors += 1;
                }
                false // Default to disabled on error
            }
        }
    }
    
    /// Check if a feature flag is enabled with detailed error handling
    pub async fn is_enabled_with_result(
        &self, 
        flag_name: &str, 
        context: &EvaluationContext
    ) -> FeatureFlagResult<bool> {
        let start_time = Instant::now();
        
        // Try cache first
        if let Some(cached_result) = self.cache.get(flag_name, context).await {
            self.update_stats(|s| {
                s.cache_hits += 1;
                s.total_evaluations += 1;
            }).await;
            return Ok(cached_result);
        }
        
        // Get flag from storage
        let flags = self.flags.read().await;
        let flag = flags.get(flag_name).ok_or_else(|| FeatureFlagError::FlagNotFound {
            name: flag_name.to_string()
        })?;
        
        // Evaluate the flag
        let enabled = self.evaluator.evaluate_flag(flag, context).await?;
        
        // Cache the result
        self.cache.put(flag_name.to_string(), context.clone(), enabled).await;
        
        // Update statistics
        let evaluation_time = start_time.elapsed();
        self.update_stats(|s| {
            s.cache_misses += 1;
            s.total_evaluations += 1;
            s.total_evaluation_time += evaluation_time;
            if evaluation_time > s.max_evaluation_time {
                s.max_evaluation_time = evaluation_time;
            }
        }).await;
        
        // Log if evaluation took too long
        if evaluation_time.as_millis() as u64 > self.global_settings.max_evaluation_time_ms {
            warn!(
                "Feature flag evaluation took {}ms for '{}', exceeding limit of {}ms",
                evaluation_time.as_millis(),
                flag_name,
                self.global_settings.max_evaluation_time_ms
            );
        }
        
        debug!("Evaluated flag '{}' = {} in {:?}", flag_name, enabled, evaluation_time);
        
        Ok(enabled)
    }
    
    /// Get detailed evaluation result
    pub async fn evaluate_detailed(
        &self,
        flag_name: &str,
        context: &EvaluationContext,
    ) -> FeatureFlagResult<EvaluationResult> {
        let flags = self.flags.read().await;
        let flag = flags.get(flag_name).ok_or_else(|| FeatureFlagError::FlagNotFound {
            name: flag_name.to_string()
        })?;
        
        let detailed_evaluator = DetailedFeatureFlagEvaluator::new();
        detailed_evaluator.evaluate_flag_detailed(flag, context).await
    }
    
    /// Reload configuration from file
    pub async fn reload_config(&self) -> FeatureFlagResult<()> {
        info!("Reloading feature flag configuration from {}", self.config_path.display());
        
        let collection = self.config_loader.load_from_file(&self.config_path)?;
        
        // Track changes for audit log
        let old_flags = {
            let flags_guard = self.flags.read().await;
            flags_guard.clone()
        };
        
        // Update flags
        {
            let mut flags_guard = self.flags.write().await;
            *flags_guard = collection.flags;
        }
        
        // Clear cache to ensure fresh evaluations
        self.cache.clear().await;
        
        // Log changes
        self.log_configuration_changes(&old_flags, &collection.flags).await;
        
        // Update stats
        self.update_stats(|s| s.config_reloads += 1).await;
        
        info!("Feature flag configuration reloaded successfully");
        Ok(())
    }
    
    /// Get all flag names
    pub async fn list_flags(&self) -> Vec<String> {
        let flags = self.flags.read().await;
        flags.keys().cloned().collect()
    }
    
    /// Get flag definition
    pub async fn get_flag(&self, name: &str) -> Option<FeatureFlag> {
        let flags = self.flags.read().await;
        flags.get(name).cloned()
    }
    
    /// Add or update a flag (for programmatic configuration)
    pub async fn upsert_flag(&self, flag: FeatureFlag) -> FeatureFlagResult<()> {
        let flag_name = flag.name.clone();
        
        // Log the change
        self.audit_logger.log_flag_change(&flag_name, "upsert", &flag).await;
        
        // Update flags
        {
            let mut flags = self.flags.write().await;
            flags.insert(flag_name.clone(), flag);
        }
        
        // Clear cache for this flag
        self.cache.invalidate_flag(&flag_name).await;
        
        info!("Feature flag '{}' updated", flag_name);
        Ok(())
    }
    
    /// Remove a flag
    pub async fn remove_flag(&self, name: &str) -> FeatureFlagResult<Option<FeatureFlag>> {
        let removed_flag = {
            let mut flags = self.flags.write().await;
            flags.remove(name)
        };
        
        if removed_flag.is_some() {
            // Clear cache for this flag
            self.cache.invalidate_flag(name).await;
            
            // Log the change
            self.audit_logger.log_flag_removal(name).await;
            
            info!("Feature flag '{}' removed", name);
        }
        
        Ok(removed_flag)
    }
    
    /// Get manager statistics
    pub async fn get_stats(&self) -> ManagerStats {
        let stats = self.stats.read().await;
        let mut stats_copy = stats.clone();
        stats_copy.uptime = self.started_at.elapsed();
        stats_copy
    }
    
    /// Clear all caches
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
        self.update_stats(|s| s.cache_clears += 1).await;
        info!("Feature flag cache cleared");
    }
    
    /// Validate all flags
    pub async fn validate_all_flags(&self) -> FeatureFlagResult<Vec<String>> {
        let flags = self.flags.read().await;
        let mut errors = Vec::new();
        
        for (name, flag) in flags.iter() {
            if let Err(err) = self.validate_flag(flag) {
                errors.push(format!("Flag '{}': {}", name, err));
            }
        }
        
        Ok(errors)
    }
    
    /// Health check for the manager
    pub async fn health_check(&self) -> FeatureFlagResult<HealthStatus> {
        let stats = self.get_stats().await;
        let validation_errors = self.validate_all_flags().await?;
        
        let status = if validation_errors.is_empty() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy(validation_errors)
        };
        
        Ok(status)
    }
    
    // Private helper methods
    
    async fn update_stats<F>(&self, updater: F) 
    where
        F: FnOnce(&mut ManagerStats),
    {
        if let Ok(mut stats) = self.stats.write().await {
            updater(&mut *stats);
        }
    }
    
    async fn log_configuration_changes(
        &self,
        old_flags: &HashMap<String, FeatureFlag>,
        new_flags: &HashMap<String, FeatureFlag>,
    ) {
        for (name, new_flag) in new_flags {
            if let Some(old_flag) = old_flags.get(name) {
                if old_flag.enabled != new_flag.enabled 
                    || old_flag.rollout_percentage != new_flag.rollout_percentage {
                    self.audit_logger.log_flag_change(name, "reload", new_flag).await;
                }
            } else {
                self.audit_logger.log_flag_change(name, "added", new_flag).await;
            }
        }
        
        // Check for removed flags
        for (name, _) in old_flags {
            if !new_flags.contains_key(name) {
                self.audit_logger.log_flag_removal(name).await;
            }
        }
    }
    
    fn validate_flag(&self, flag: &FeatureFlag) -> Result<(), String> {
        if flag.name.is_empty() {
            return Err("Flag name cannot be empty".to_string());
        }
        
        if let Some(percentage) = flag.rollout_percentage {
            if percentage > 100 {
                return Err("Rollout percentage cannot exceed 100".to_string());
            }
        }
        
        // Validate conditions
        if let Some(conditions) = &flag.conditions {
            for condition in conditions {
                match condition {
                    FeatureCondition::SyncProgressAbove(p) | FeatureCondition::SyncProgressBelow(p) => {
                        if *p < 0.0 || *p > 1.0 {
                            return Err("Sync progress must be between 0.0 and 1.0".to_string());
                        }
                    }
                    FeatureCondition::TimeWindow { start_hour, end_hour } => {
                        if *start_hour > 23 || *end_hour > 23 {
                            return Err("Hour values must be between 0 and 23".to_string());
                        }
                    }
                    _ => {} // Other conditions are valid by construction
                }
            }
        }
        
        Ok(())
    }
}

/// Manager statistics
#[derive(Debug, Clone)]
pub struct ManagerStats {
    pub total_evaluations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_clears: u64,
    pub config_reloads: u64,
    pub evaluation_errors: u64,
    pub total_evaluation_time: Duration,
    pub max_evaluation_time: Duration,
    pub uptime: Duration,
    pub flags_count: usize,
}

impl ManagerStats {
    pub fn new() -> Self {
        Self {
            total_evaluations: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_clears: 0,
            config_reloads: 0,
            evaluation_errors: 0,
            total_evaluation_time: Duration::ZERO,
            max_evaluation_time: Duration::ZERO,
            uptime: Duration::ZERO,
            flags_count: 0,
        }
    }
    
    pub fn cache_hit_rate(&self) -> f64 {
        if self.total_evaluations == 0 {
            0.0
        } else {
            self.cache_hits as f64 / self.total_evaluations as f64
        }
    }
    
    pub fn avg_evaluation_time(&self) -> Duration {
        if self.cache_misses == 0 {
            Duration::ZERO
        } else {
            self.total_evaluation_time / self.cache_misses as u32
        }
    }
}

/// Health status
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Unhealthy(Vec<String>),
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }
}

/// Audit logger for flag changes
pub struct AuditLogger {
    enabled: bool,
}

impl AuditLogger {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
    
    pub async fn log_flag_change(&self, name: &str, action: &str, flag: &FeatureFlag) {
        if self.enabled {
            info!(
                action = action,
                flag_name = name,
                enabled = flag.enabled,
                rollout_percentage = flag.rollout_percentage,
                "Feature flag change"
            );
        }
    }
    
    pub async fn log_flag_removal(&self, name: &str) {
        if self.enabled {
            info!(
                action = "removed",
                flag_name = name,
                "Feature flag removed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    use crate::config::Environment;
    
    #[tokio::test]
    async fn test_manager_basic_operations() {
        let temp_file = create_test_config().await;
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        
        let context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        
        // Test enabled flag
        assert!(manager.is_enabled("test_enabled", &context).await);
        
        // Test disabled flag  
        assert!(!manager.is_enabled("test_disabled", &context).await);
        
        // Test non-existent flag
        assert!(!manager.is_enabled("non_existent", &context).await);
    }
    
    #[tokio::test]
    async fn test_manager_stats() {
        let temp_file = create_test_config().await;
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        
        // Perform some evaluations
        let _ = manager.is_enabled("test_enabled", &context).await;
        let _ = manager.is_enabled("test_enabled", &context).await; // Should hit cache
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_evaluations, 2);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }
    
    async fn create_test_config() -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, r#"
version = "1.0"
default_environment = "development"

[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true
enable_metrics = true
max_evaluation_time_ms = 1

[flags.test_enabled]
name = "test_enabled"
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"

[flags.test_disabled]
name = "test_disabled"
enabled = false
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"
        "#).unwrap();
        temp_file
    }
}