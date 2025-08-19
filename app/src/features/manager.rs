//! Feature Flag Manager
//!
//! This module implements the main FeatureFlagManager that provides configuration loading,
//! flag evaluation, caching, and hot-reload capabilities.

use super::types::*;
use super::context::*;
use super::evaluation::*;
use super::cache::*;
use super::config::*;
use super::watcher::*;
use super::validation::*;
use super::audit::*;
use super::performance;
use super::metrics::{FeatureFlagMetrics, TimedEvaluation};
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
    
    /// File watcher for hot-reload capability
    file_watcher: Option<FeatureFlagFileWatcher>,
    
    /// Hot-reload task handle
    hot_reload_task: Option<tokio::task::JoinHandle<()>>,
    
    /// Audit logger for flag changes
    audit_logger: FeatureFlagAuditLogger,
    
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
        
        let audit_config = AuditConfig {
            enabled: collection.global_settings.enable_audit_log,
            use_tracing: true,
            include_metadata: false, // Security: don't log sensitive metadata
            max_events_in_memory: 1000,
            sync_writes: true,
            ..Default::default()
        };
        let audit_logger = FeatureFlagAuditLogger::with_config(audit_config);
        
        // Log system startup
        let startup_task = audit_logger.log_system_event(
            &format!("Feature flag manager initialized with {} flags from {}", 
                collection.flags.len(), 
                config_path.display()),
            "system_startup"
        );
        tokio::spawn(startup_task);
        
        // Initialize metrics for startup
        FeatureFlagMetrics::record_config_reload("startup");
        FeatureFlagMetrics::update_flag_counts(&collection.flags);
        
        Ok(Self {
            flags: Arc::new(RwLock::new(collection.flags)),
            config_path,
            evaluator,
            cache,
            config_loader,
            file_watcher: None,
            hot_reload_task: None,
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
            let evaluation_time_us = start_time.elapsed().as_micros() as u64;
            
            // Record metrics for cache hit
            FeatureFlagMetrics::record_evaluation(flag_name, cached_result, evaluation_time_us, true);
            FeatureFlagMetrics::record_cache_operation("hit", Some(flag_name));
            
            self.update_stats(|s| {
                s.cache_hits += 1;
                s.total_evaluations += 1;
            }).await;
            return Ok(cached_result);
        }
        
        // Record cache miss
        FeatureFlagMetrics::record_cache_operation("miss", Some(flag_name));
        
        // Get flag from storage
        let flags = self.flags.read().await;
        let flag = flags.get(flag_name).ok_or_else(|| FeatureFlagError::FlagNotFound {
            name: flag_name.to_string()
        })?;
        
        // Evaluate the flag
        let enabled = match self.evaluator.evaluate_flag(flag, context).await {
            Ok(result) => result,
            Err(e) => {
                let evaluation_time_us = start_time.elapsed().as_micros() as u64;
                FeatureFlagMetrics::record_evaluation_error(flag_name, &e.to_string());
                return Err(e);
            }
        };
        
        // Cache the result
        self.cache.put(flag_name.to_string(), context.clone(), enabled).await;
        FeatureFlagMetrics::record_cache_operation("store", Some(flag_name));
        
        // Calculate final timing
        let evaluation_time = start_time.elapsed();
        let evaluation_time_us = evaluation_time.as_micros() as u64;
        
        // Record metrics for cache miss evaluation
        FeatureFlagMetrics::record_evaluation(flag_name, enabled, evaluation_time_us, false);
        
        // Update statistics
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
        
        // Calculate changes for audit logging
        let (flags_changed, flags_added, flags_removed) = self.calculate_flag_changes(&old_flags, &collection.flags);
        
        // Update flags
        {
            let mut flags_guard = self.flags.write().await;
            *flags_guard = collection.flags.clone();
        }
        
        // Clear cache to ensure fresh evaluations
        self.cache.clear().await;
        
        // Log detailed configuration changes
        self.log_detailed_configuration_changes(&old_flags, &collection.flags, "manual_reload").await;
        
        // Log overall configuration reload event
        self.audit_logger.log_configuration_reload(
            &self.config_path,
            flags_changed,
            flags_added,
            flags_removed,
            "manual_reload"
        ).await;
        
        // Update stats
        self.update_stats(|s| s.config_reloads += 1).await;
        
        info!("Feature flag configuration reloaded successfully");
        Ok(())
    }
    
    /// Start hot-reload capability for automatic configuration updates
    pub async fn start_hot_reload(&mut self) -> FeatureFlagResult<()> {
        if self.file_watcher.is_some() {
            warn!("Hot-reload is already active");
            return Ok(());
        }
        
        info!("Starting hot-reload for configuration file: {}", self.config_path.display());
        
        // Create file watcher
        let mut watcher = FeatureFlagFileWatcher::new(self.config_path.clone())?;
        let event_receiver = watcher.start_watching()?;
        
        // Start hot-reload processing task
        let task_handle = self.start_hot_reload_task(event_receiver).await;
        
        self.file_watcher = Some(watcher);
        self.hot_reload_task = Some(task_handle);
        
        info!("Hot-reload started successfully");
        Ok(())
    }
    
    /// Stop hot-reload capability
    pub async fn stop_hot_reload(&mut self) -> FeatureFlagResult<()> {
        info!("Stopping hot-reload");
        
        if let Some(mut watcher) = self.file_watcher.take() {
            watcher.stop_watching()?;
        }
        
        if let Some(task_handle) = self.hot_reload_task.take() {
            task_handle.abort();
        }
        
        info!("Hot-reload stopped");
        Ok(())
    }
    
    /// Check if hot-reload is currently active
    pub fn is_hot_reload_active(&self) -> bool {
        self.file_watcher.as_ref().map(|w| w.is_watching()).unwrap_or(false)
            && self.hot_reload_task.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
    }
    
    /// Start the background task that handles hot-reload events
    async fn start_hot_reload_task(
        &self,
        mut event_receiver: tokio::sync::mpsc::UnboundedReceiver<ConfigFileEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let config_loader = self.config_loader.clone();
        let flags = self.flags.clone();
        let cache = self.cache.clone();
        let audit_logger = self.audit_logger.clone();
        let stats = self.stats.clone();
        let config_path = self.config_path.clone();
        
        tokio::spawn(async move {
            info!("Hot-reload task started");
            
            while let Some(event) = event_receiver.recv().await {
                match event {
                    ConfigFileEvent::Modified(_path) | ConfigFileEvent::Created(_path) => {
                        info!("Configuration file changed, reloading...");
                        
                        match Self::handle_config_reload(
                            &config_loader,
                            &config_path,
                            &flags,
                            &cache,
                            &audit_logger,
                            &stats,
                        ).await {
                            Ok(()) => {
                                info!("Hot-reload completed successfully");
                            }
                            Err(e) => {
                                error!("Hot-reload failed: {}", e);
                                // Track error in statistics
                                if let Ok(mut stats_guard) = stats.write().await {
                                    stats_guard.hot_reload_errors += 1;
                                }
                                // Record metrics for hot reload error
                                FeatureFlagMetrics::record_hot_reload_event("error");
                                // Continue running despite errors to allow recovery
                            }
                        }
                    }
                    ConfigFileEvent::Deleted(_path) => {
                        error!("Configuration file was deleted! Hot-reload disabled until file is restored.");
                        // Record metrics for file deletion
                        FeatureFlagMetrics::record_hot_reload_event("file_deleted");
                        // Continue monitoring in case file is recreated
                    }
                    ConfigFileEvent::Error(error) => {
                        error!("File watcher error: {}", error);
                        // Continue despite watcher errors
                    }
                }
            }
            
            info!("Hot-reload task stopped");
        })
    }
    
    /// Handle configuration reload (static method for use in background task)
    async fn handle_config_reload(
        config_loader: &FeatureFlagConfigLoader,
        config_path: &PathBuf,
        flags: &Arc<RwLock<HashMap<String, FeatureFlag>>>,
        cache: &FeatureFlagCache,
        audit_logger: &FeatureFlagAuditLogger,
        stats: &Arc<RwLock<ManagerStats>>,
    ) -> FeatureFlagResult<()> {
        // Log hot-reload trigger
        audit_logger.log_hot_reload_triggered(config_path).await;
        
        // Load new configuration
        let collection = config_loader.load_from_file(config_path)?;
        
        // Track changes for audit log
        let old_flags = {
            let flags_guard = flags.read().await;
            flags_guard.clone()
        };
        
        // Calculate changes for audit logging
        let (flags_changed, flags_added, flags_removed) = Self::calculate_flag_changes_static(&old_flags, &collection.flags);
        
        // Update flags
        {
            let mut flags_guard = flags.write().await;
            *flags_guard = collection.flags.clone();
        }
        
        // Clear cache to ensure fresh evaluations
        cache.clear().await;
        
        // Log detailed flag changes
        Self::log_detailed_configuration_changes_static(&old_flags, &collection.flags, "hot_reload", audit_logger).await;
        
        // Log overall configuration reload event
        audit_logger.log_configuration_reload(
            config_path,
            flags_changed,
            flags_added,
            flags_removed,
            "hot_reload"
        ).await;
        
        // Update stats
        if let Ok(mut stats_guard) = stats.write().await {
            stats_guard.config_reloads += 1;
            stats_guard.hot_reloads += 1;
        }
        
        // Record metrics for hot reload
        FeatureFlagMetrics::record_hot_reload_event("success");
        FeatureFlagMetrics::record_config_reload("hot_reload");
        FeatureFlagMetrics::record_bulk_flag_changes(flags_changed, flags_added, flags_removed);
        FeatureFlagMetrics::update_flag_counts(&collection.flags);
        FeatureFlagMetrics::record_cache_operation("clear", None);
        
        Ok(())
    }
    
    /// Static version of log_configuration_changes for use in background task
    async fn log_configuration_changes_static(
        old_flags: &HashMap<String, FeatureFlag>,
        new_flags: &HashMap<String, FeatureFlag>,
        audit_logger: &FeatureFlagAuditLogger,
    ) {
        for (name, new_flag) in new_flags {
            if let Some(old_flag) = old_flags.get(name) {
                if old_flag.enabled != new_flag.enabled 
                    || old_flag.rollout_percentage != new_flag.rollout_percentage {
                    audit_logger.log_flag_change(name, Some(old_flag), new_flag, "hot-reload").await;
                }
            } else {
                audit_logger.log_flag_change(name, None, new_flag, "hot-reload-added").await;
            }
        }
        
        // Check for removed flags
        for (name, old_flag) in old_flags {
            if !new_flags.contains_key(name) {
                audit_logger.log_flag_deleted(name, old_flag, "hot-reload-removed").await;
            }
        }
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
        
        // Get old flag for audit logging
        let old_flag = {
            let flags = self.flags.read().await;
            flags.get(&flag_name).cloned()
        };
        
        // Log the change
        self.audit_logger.log_flag_change(&flag_name, old_flag.as_ref(), &flag, "programmatic_upsert").await;
        
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
        
        if let Some(ref flag) = removed_flag {
            // Clear cache for this flag
            self.cache.invalidate_flag(name).await;
            
            // Log the change
            self.audit_logger.log_flag_deleted(name, flag, "programmatic_removal").await;
            
            info!("Feature flag '{}' removed", name);
        }
        
        Ok(removed_flag)
    }
    
    /// Get manager statistics
    pub async fn get_stats(&self) -> ManagerStats {
        let stats = self.stats.read().await;
        let mut stats_copy = stats.clone();
        stats_copy.uptime = self.started_at.elapsed();
        stats_copy.hot_reload_active = self.is_hot_reload_active();
        stats_copy.flags_count = {
            let flags = self.flags.read().await;
            flags.len()
        };
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
    
    /// Run performance benchmark with <1ms target (ALYS-004-10)
    pub async fn run_performance_benchmark(&self, iterations: usize) -> crate::features::performance::benchmarks::BenchmarkResults {
        info!("Running performance benchmark with {} iterations", iterations);
        
        let benchmark_results = crate::features::performance::benchmarks::run_comprehensive_benchmark(
            self, iterations
        ).await;
        
        // Log results
        info!(
            "Performance benchmark completed: avg={}μs, p95={}μs, target_met={}",
            benchmark_results.avg_evaluation_time_us,
            benchmark_results.p95_evaluation_time_us, 
            benchmark_results.target_met
        );
        
        if !benchmark_results.target_met {
            warn!(
                "Performance target not met: {}/{} evaluations over 1ms ({}%)",
                benchmark_results.evaluations_over_1ms,
                benchmark_results.total_evaluations,
                if benchmark_results.total_evaluations > 0 {
                    (benchmark_results.evaluations_over_1ms as f64 / benchmark_results.total_evaluations as f64) * 100.0
                } else { 0.0 }
            );
        }
        
        benchmark_results
    }
    
    /// Generate comprehensive validation report for all flags
    pub async fn generate_validation_report(&self) -> FeatureFlagResult<String> {
        let flags = self.flags.read().await;
        let mut collection = FeatureFlagCollection::new();
        collection.flags = flags.clone();
        collection.global_settings = self.global_settings.clone();
        
        // Create validation context based on current environment
        let validation_context = ValidationContext {
            environment: collection.default_environment,
            schema_version: collection.version.clone(),
            strict_mode: true,
            deprecated_warnings: true,
        };
        
        let (is_valid, report) = validate_collection_with_report(&collection, Some(validation_context));
        
        if !is_valid {
            warn!("Configuration validation issues detected");
        } else {
            info!("All configuration validations passed");
        }
        
        Ok(report)
    }
    
    /// Validate configuration during reload with enhanced reporting
    pub async fn validate_config_with_enhanced_reporting(&self, collection: &FeatureFlagCollection) -> FeatureFlagResult<()> {
        let validation_context = ValidationContext {
            environment: collection.default_environment,
            schema_version: collection.version.clone(),
            strict_mode: matches!(collection.default_environment, crate::config::Environment::Production),
            deprecated_warnings: true,
        };
        
        let validator = FeatureFlagValidator::with_context(validation_context);
        if let Err(errors) = validator.validate_collection(collection) {
            // Log detailed validation errors
            for error in &errors {
                match error.error_type {
                    ValidationErrorType::Security => {
                        warn!("Security validation issue in {}: {}", error.field_path, error.message);
                    }
                    ValidationErrorType::Performance => {
                        warn!("Performance validation issue in {}: {}", error.field_path, error.message);
                    }
                    _ => {
                        warn!("Validation issue in {}: {}", error.field_path, error.message);
                    }
                }
                
                if let Some(suggestion) = &error.suggestion {
                    info!("Suggestion for {}: {}", error.field_path, suggestion);
                }
            }
            
            // Create comprehensive error message
            let error_summary = errors.iter()
                .take(5) // Show first 5 errors
                .map(|e| format!("{}: {}", e.field_path, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            
            let total_errors = errors.len();
            let final_message = if total_errors > 5 {
                format!("{} (and {} more errors)", error_summary, total_errors - 5)
            } else {
                error_summary
            };
            
            return Err(FeatureFlagError::ValidationError {
                flag: "configuration".to_string(),
                reason: final_message,
            });
        }
        
        Ok(())
    }
    
    /// Get comprehensive performance report
    pub async fn get_performance_report(&self) -> String {
        let manager_stats = self.get_stats().await;
        let macro_cache_stats = crate::features::performance::macro_cache::get_cache_stats().await;
        let macro_health = crate::features::performance::macro_cache::health_check().await;
        let cache_stats = self.cache.get_stats().await;
        let cache_size = self.cache.get_size_info().await;
        
        format!(
            "Feature Flag System Performance Report\n\
            ==========================================\n\
            \n\
            Manager Statistics:\n\
            - Total Evaluations: {}\n\
            - Cache Hit Rate: {:.1}%\n\
            - Average Evaluation Time: {}μs\n\
            - Max Evaluation Time: {}μs\n\
            - Config Reloads: {}\n\
            - Hot Reloads: {}\n\
            - Evaluation Errors: {}\n\
            - Uptime: {:.1} minutes\n\
            \n\
            Macro Cache (5-second TTL):\n\
            - Total Accesses: {}\n\
            - Hit Rate: {:.1}%\n\
            - Average Lookup Time: {}μs\n\
            - Current Size: {} entries\n\
            - Max Size Reached: {} entries\n\
            - Health Status: {}\n\
            \n\
            Manager Cache:\n\
            - Hit Rate: {:.1}%\n\
            - Total Entries: {}\n\
            - Memory Estimate: {} KB\n\
            \n\
            Performance Target Status:\n\
            - <1ms Target: {}\n\
            - Macro Cache Health: {}\n\
            - System Ready: {}",
            manager_stats.total_evaluations,
            manager_stats.cache_hit_rate() * 100.0,
            manager_stats.avg_evaluation_time().as_micros(),
            manager_stats.max_evaluation_time.as_micros(),
            manager_stats.config_reloads,
            manager_stats.hot_reloads,
            manager_stats.evaluation_errors,
            manager_stats.uptime.as_secs_f64() / 60.0,
            
            macro_cache_stats.total_accesses,
            if macro_cache_stats.total_accesses > 0 {
                (macro_cache_stats.hits as f64 / macro_cache_stats.total_accesses as f64) * 100.0
            } else { 0.0 },
            macro_cache_stats.avg_cache_lookup_time_ns / 1000, // Convert to microseconds
            macro_cache_stats.current_cache_size,
            macro_cache_stats.max_cache_size,
            if macro_health.is_healthy() { "✓ Healthy" } else { "✗ Degraded" },
            
            cache_stats.hit_rate() * 100.0,
            cache_size.total_entries,
            cache_size.estimated_memory_kb(),
            
            if manager_stats.avg_evaluation_time().as_millis() < 1 { "✓ Met" } else { "✗ Exceeded" },
            if macro_health.is_healthy() { "✓" } else { "✗" },
            if macro_health.is_healthy() && manager_stats.avg_evaluation_time().as_millis() < 1 { 
                "✓ Ready" 
            } else { 
                "✗ Needs Attention" 
            }
        )
    }
    
    /// Validate percentage rollout distribution for consistency testing
    pub async fn validate_rollout_distribution(
        &self,
        flag_name: &str,
        percentage: u8,
        sample_size: usize,
    ) -> FeatureFlagResult<crate::features::performance::consistent_hashing::RolloutDistributionStats> {
        use crate::config::Environment;
        
        // Generate sample contexts
        let samples: Vec<(String, Environment)> = (0..sample_size)
            .map(|i| (format!("test-node-{}", i), Environment::Development))
            .collect();
            
        let stats = crate::features::performance::consistent_hashing::verify_rollout_distribution(
            percentage, &samples, flag_name
        );
        
        if !stats.is_within_tolerance {
            warn!(
                "Rollout distribution out of tolerance for flag '{}': target={}%, actual={:.1}%, deviation={:.1}%",
                flag_name, percentage, stats.actual_percentage, stats.deviation
            );
        } else {
            debug!(
                "Rollout distribution validated for flag '{}': target={}%, actual={:.1}%, deviation={:.1}%",
                flag_name, percentage, stats.actual_percentage, stats.deviation
            );
        }
        
        Ok(stats)
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
        // Use enhanced validation for more comprehensive checking
        validate_flag_quick(flag)
    }
    
    /// Calculate changes between old and new flag sets
    fn calculate_flag_changes(
        &self,
        old_flags: &HashMap<String, FeatureFlag>,
        new_flags: &HashMap<String, FeatureFlag>
    ) -> (usize, usize, usize) {
        Self::calculate_flag_changes_static(old_flags, new_flags)
    }
    
    /// Static version of calculate_flag_changes for use in background task
    fn calculate_flag_changes_static(
        old_flags: &HashMap<String, FeatureFlag>,
        new_flags: &HashMap<String, FeatureFlag>
    ) -> (usize, usize, usize) {
        let mut flags_changed = 0;
        let mut flags_added = 0;
        let flags_removed = old_flags.len().saturating_sub(
            old_flags.keys().filter(|key| new_flags.contains_key(*key)).count()
        );
        
        for (name, new_flag) in new_flags {
            if let Some(old_flag) = old_flags.get(name) {
                // Check if flag actually changed
                if old_flag.enabled != new_flag.enabled ||
                   old_flag.rollout_percentage != new_flag.rollout_percentage ||
                   old_flag.targets != new_flag.targets ||
                   old_flag.conditions != new_flag.conditions ||
                   old_flag.metadata != new_flag.metadata {
                    flags_changed += 1;
                }
            } else {
                flags_added += 1;
            }
        }
        
        (flags_changed, flags_added, flags_removed)
    }
    
    /// Log detailed configuration changes
    async fn log_detailed_configuration_changes(
        &self,
        old_flags: &HashMap<String, FeatureFlag>,
        new_flags: &HashMap<String, FeatureFlag>,
        source: &str,
    ) {
        Self::log_detailed_configuration_changes_static(old_flags, new_flags, source, &self.audit_logger).await;
    }
    
    /// Static version of log_detailed_configuration_changes for use in background task
    async fn log_detailed_configuration_changes_static(
        old_flags: &HashMap<String, FeatureFlag>,
        new_flags: &HashMap<String, FeatureFlag>,
        source: &str,
        audit_logger: &FeatureFlagAuditLogger,
    ) {
        // Log individual flag changes
        for (name, new_flag) in new_flags {
            if let Some(old_flag) = old_flags.get(name) {
                // Check if flag actually changed
                if old_flag.enabled != new_flag.enabled ||
                   old_flag.rollout_percentage != new_flag.rollout_percentage ||
                   old_flag.targets != new_flag.targets ||
                   old_flag.conditions != new_flag.conditions ||
                   old_flag.metadata != new_flag.metadata {
                    audit_logger.log_flag_change(name, Some(old_flag), new_flag, source).await;
                }
            } else {
                // New flag
                audit_logger.log_flag_change(name, None, new_flag, source).await;
            }
        }
        
        // Log removed flags
        for (name, old_flag) in old_flags {
            if !new_flags.contains_key(name) {
                audit_logger.log_flag_deleted(name, old_flag, source).await;
            }
        }
    }
    
    /// Get audit statistics (for monitoring and debugging)
    pub async fn get_audit_stats(&self) -> AuditStats {
        self.audit_logger.get_audit_stats().await
    }
    
    /// Get recent audit events (for debugging and monitoring)
    pub async fn get_recent_audit_events(&self, limit: Option<usize>) -> Vec<AuditEvent> {
        self.audit_logger.get_recent_events(limit).await
    }
    
    /// Get audit events for a specific flag (for debugging)
    pub async fn get_audit_events_for_flag(&self, flag_name: &str, limit: Option<usize>) -> Vec<AuditEvent> {
        self.audit_logger.get_events_for_flag(flag_name, limit).await
    }
    
    /// Generate comprehensive audit report
    pub async fn generate_audit_report(&self) -> String {
        let stats = self.get_audit_stats().await;
        stats.generate_summary()
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
    pub hot_reloads: u64,
    pub hot_reload_errors: u64,
    pub evaluation_errors: u64,
    pub total_evaluation_time: Duration,
    pub max_evaluation_time: Duration,
    pub uptime: Duration,
    pub flags_count: usize,
    pub hot_reload_active: bool,
}

impl ManagerStats {
    pub fn new() -> Self {
        Self {
            total_evaluations: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_clears: 0,
            config_reloads: 0,
            hot_reloads: 0,
            hot_reload_errors: 0,
            evaluation_errors: 0,
            total_evaluation_time: Duration::ZERO,
            max_evaluation_time: Duration::ZERO,
            uptime: Duration::ZERO,
            flags_count: 0,
            hot_reload_active: false,
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


impl Drop for FeatureFlagManager {
    fn drop(&mut self) {
        // Stop hot-reload if it's active
        if self.is_hot_reload_active() {
            tracing::debug!("Stopping hot-reload during manager cleanup");
            // We can't use async methods in Drop, but the file watcher will clean itself up
            if let Some(task_handle) = self.hot_reload_task.take() {
                task_handle.abort();
            }
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