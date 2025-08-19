//! Audit logging system for feature flag changes
//!
//! This module provides comprehensive audit logging for the feature flag system,
//! tracking all flag changes, configuration updates, and operational events
//! for compliance, debugging, and monitoring purposes.

use super::types::*;
use super::{FeatureFlagResult, FeatureFlagError};
use super::metrics::FeatureFlagMetrics;
use crate::config::Environment;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Audit event types for different kinds of flag system events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditEventType {
    /// Flag was enabled or disabled
    FlagToggled,
    /// Flag rollout percentage changed
    RolloutPercentageChanged,
    /// Flag targeting rules changed
    TargetingChanged,
    /// Flag conditions changed
    ConditionsChanged,
    /// New flag was created
    FlagCreated,
    /// Existing flag was deleted
    FlagDeleted,
    /// Flag metadata was updated
    MetadataChanged,
    /// Configuration file was reloaded
    ConfigurationReloaded,
    /// Hot-reload event occurred
    HotReloadTriggered,
    /// Validation error occurred
    ValidationError,
    /// System startup/shutdown
    SystemEvent,
}

/// Detailed audit event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event identifier
    pub event_id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Type of audit event
    pub event_type: AuditEventType,
    /// Name of the flag that changed
    pub flag_name: Option<String>,
    /// Previous state of the flag (if applicable)
    pub old_value: Option<AuditFlagState>,
    /// New state of the flag (if applicable)
    pub new_value: Option<AuditFlagState>,
    /// Source of the change (file, api, system)
    pub source: String,
    /// User or system component that made the change
    pub changed_by: Option<String>,
    /// Additional context or details
    pub details: HashMap<String, String>,
    /// Environment where the change occurred
    pub environment: Option<Environment>,
    /// Configuration file path (if applicable)
    pub config_file: Option<PathBuf>,
}

/// Simplified flag state for audit logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFlagState {
    pub enabled: bool,
    pub rollout_percentage: Option<u8>,
    pub has_targeting: bool,
    pub has_conditions: bool,
    pub metadata_keys: Vec<String>,
}

impl From<&FeatureFlag> for AuditFlagState {
    fn from(flag: &FeatureFlag) -> Self {
        Self {
            enabled: flag.enabled,
            rollout_percentage: flag.rollout_percentage,
            has_targeting: flag.targets.is_some(),
            has_conditions: flag.conditions.is_some(),
            metadata_keys: flag.metadata.keys().cloned().collect(),
        }
    }
}

/// Configuration for the audit logging system
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Whether audit logging is enabled
    pub enabled: bool,
    /// Path to the audit log file
    pub log_file: Option<PathBuf>,
    /// Whether to log to structured tracing output
    pub use_tracing: bool,
    /// Whether to include sensitive metadata in logs
    pub include_metadata: bool,
    /// Maximum number of events to keep in memory
    pub max_events_in_memory: usize,
    /// Whether to sync writes to disk immediately
    pub sync_writes: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_file: None, // Default to tracing only
            use_tracing: true,
            include_metadata: false, // Don't log potentially sensitive metadata by default
            max_events_in_memory: 1000,
            sync_writes: true,
        }
    }
}

/// High-performance audit logging system for feature flags
pub struct FeatureFlagAuditLogger {
    /// Audit configuration
    config: AuditConfig,
    /// In-memory event buffer for fast access
    events: Arc<RwLock<Vec<AuditEvent>>>,
    /// Current session ID for grouping related events
    session_id: String,
}

impl FeatureFlagAuditLogger {
    /// Create a new audit logger with default configuration
    pub fn new() -> Self {
        Self::with_config(AuditConfig::default())
    }
    
    /// Create audit logger with custom configuration
    pub fn with_config(config: AuditConfig) -> Self {
        let session_id = Self::generate_session_id();
        
        if config.enabled {
            info!(
                audit = true,
                session_id = %session_id,
                "Feature flag audit logging enabled"
            );
        }
        
        Self {
            config,
            events: Arc::new(RwLock::new(Vec::new())),
            session_id,
        }
    }
    
    /// Log a flag change event
    pub async fn log_flag_change(
        &self,
        flag_name: &str,
        old_flag: Option<&FeatureFlag>,
        new_flag: &FeatureFlag,
        source: &str,
    ) {
        if !self.config.enabled {
            return;
        }
        
        let event_type = if old_flag.is_none() {
            AuditEventType::FlagCreated
        } else if let Some(old) = old_flag {
            // Determine what changed
            if old.enabled != new_flag.enabled {
                AuditEventType::FlagToggled
            } else if old.rollout_percentage != new_flag.rollout_percentage {
                AuditEventType::RolloutPercentageChanged
            } else if old.targets != new_flag.targets {
                AuditEventType::TargetingChanged
            } else if old.conditions != new_flag.conditions {
                AuditEventType::ConditionsChanged
            } else {
                AuditEventType::MetadataChanged
            }
        } else {
            AuditEventType::MetadataChanged
        };
        
        let mut details = HashMap::new();
        
        // Add specific change details
        if let Some(old) = old_flag {
            if old.enabled != new_flag.enabled {
                details.insert("change_type".to_string(), "enabled_status".to_string());
                details.insert("old_enabled".to_string(), old.enabled.to_string());
                details.insert("new_enabled".to_string(), new_flag.enabled.to_string());
            }
            
            if old.rollout_percentage != new_flag.rollout_percentage {
                details.insert("change_type".to_string(), "rollout_percentage".to_string());
                details.insert("old_percentage".to_string(), 
                    old.rollout_percentage.map(|p| p.to_string()).unwrap_or_else(|| "none".to_string()));
                details.insert("new_percentage".to_string(),
                    new_flag.rollout_percentage.map(|p| p.to_string()).unwrap_or_else(|| "none".to_string()));
            }
        } else {
            details.insert("change_type".to_string(), "flag_created".to_string());
        }
        
        // Add metadata information if configured
        if self.config.include_metadata {
            if let Some(description) = &new_flag.description {
                details.insert("description".to_string(), description.clone());
            }
            
            for (key, value) in &new_flag.metadata {
                if !Self::is_sensitive_metadata_key(key) {
                    details.insert(format!("metadata.{}", key), value.clone());
                }
            }
        }
        
        let event = AuditEvent {
            event_id: Self::generate_event_id(),
            timestamp: Utc::now(),
            event_type,
            flag_name: Some(flag_name.to_string()),
            old_value: old_flag.map(AuditFlagState::from),
            new_value: Some(AuditFlagState::from(new_flag)),
            source: source.to_string(),
            changed_by: Some(new_flag.updated_by.clone()),
            details,
            environment: None, // Will be set by manager if available
            config_file: None, // Will be set by manager if available
        };
        
        self.record_event(event).await;
    }
    
    /// Log flag deletion
    pub async fn log_flag_deleted(&self, flag_name: &str, deleted_flag: &FeatureFlag, source: &str) {
        if !self.config.enabled {
            return;
        }
        
        let mut details = HashMap::new();
        details.insert("change_type".to_string(), "flag_deleted".to_string());
        details.insert("was_enabled".to_string(), deleted_flag.enabled.to_string());
        
        let event = AuditEvent {
            event_id: Self::generate_event_id(),
            timestamp: Utc::now(),
            event_type: AuditEventType::FlagDeleted,
            flag_name: Some(flag_name.to_string()),
            old_value: Some(AuditFlagState::from(deleted_flag)),
            new_value: None,
            source: source.to_string(),
            changed_by: Some(deleted_flag.updated_by.clone()),
            details,
            environment: None,
            config_file: None,
        };
        
        self.record_event(event).await;
    }
    
    /// Log configuration reload event
    pub async fn log_configuration_reload(
        &self,
        config_file: &PathBuf,
        flags_changed: usize,
        flags_added: usize,
        flags_removed: usize,
        source: &str,
    ) {
        if !self.config.enabled {
            return;
        }
        
        let mut details = HashMap::new();
        details.insert("flags_changed".to_string(), flags_changed.to_string());
        details.insert("flags_added".to_string(), flags_added.to_string());
        details.insert("flags_removed".to_string(), flags_removed.to_string());
        details.insert("total_flags".to_string(), (flags_changed + flags_added).to_string());
        
        let event = AuditEvent {
            event_id: Self::generate_event_id(),
            timestamp: Utc::now(),
            event_type: AuditEventType::ConfigurationReloaded,
            flag_name: None,
            old_value: None,
            new_value: None,
            source: source.to_string(),
            changed_by: Some("system".to_string()),
            details,
            environment: None,
            config_file: Some(config_file.clone()),
        };
        
        self.record_event(event).await;
    }
    
    /// Log hot-reload trigger event
    pub async fn log_hot_reload_triggered(&self, config_file: &PathBuf) {
        if !self.config.enabled {
            return;
        }
        
        let mut details = HashMap::new();
        details.insert("trigger".to_string(), "file_watcher".to_string());
        details.insert("config_file".to_string(), config_file.display().to_string());
        
        let event = AuditEvent {
            event_id: Self::generate_event_id(),
            timestamp: Utc::now(),
            event_type: AuditEventType::HotReloadTriggered,
            flag_name: None,
            old_value: None,
            new_value: None,
            source: "file_watcher".to_string(),
            changed_by: Some("system".to_string()),
            details,
            environment: None,
            config_file: Some(config_file.clone()),
        };
        
        self.record_event(event).await;
    }
    
    /// Log validation error
    pub async fn log_validation_error(
        &self,
        error_message: &str,
        config_file: Option<&PathBuf>,
        flag_name: Option<&str>,
    ) {
        if !self.config.enabled {
            return;
        }
        
        let mut details = HashMap::new();
        details.insert("error_message".to_string(), error_message.to_string());
        details.insert("validation_failed".to_string(), "true".to_string());
        
        let event = AuditEvent {
            event_id: Self::generate_event_id(),
            timestamp: Utc::now(),
            event_type: AuditEventType::ValidationError,
            flag_name: flag_name.map(|s| s.to_string()),
            old_value: None,
            new_value: None,
            source: "validation_system".to_string(),
            changed_by: Some("system".to_string()),
            details,
            environment: None,
            config_file: config_file.cloned(),
        };
        
        self.record_event(event).await;
    }
    
    /// Log system event (startup, shutdown, etc.)
    pub async fn log_system_event(&self, event_description: &str, source: &str) {
        if !self.config.enabled {
            return;
        }
        
        let mut details = HashMap::new();
        details.insert("event_description".to_string(), event_description.to_string());
        details.insert("session_id".to_string(), self.session_id.clone());
        
        let event = AuditEvent {
            event_id: Self::generate_event_id(),
            timestamp: Utc::now(),
            event_type: AuditEventType::SystemEvent,
            flag_name: None,
            old_value: None,
            new_value: None,
            source: source.to_string(),
            changed_by: Some("system".to_string()),
            details,
            environment: None,
            config_file: None,
        };
        
        self.record_event(event).await;
    }
    
    /// Get recent audit events (for debugging/monitoring)
    pub async fn get_recent_events(&self, limit: Option<usize>) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        let limit = limit.unwrap_or(100).min(events.len());
        events[events.len().saturating_sub(limit)..].to_vec()
    }
    
    /// Get events for a specific flag
    pub async fn get_events_for_flag(&self, flag_name: &str, limit: Option<usize>) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|event| {
                event.flag_name.as_ref().map(|name| name == flag_name).unwrap_or(false)
            })
            .cloned()
            .collect();
            
        let limit = limit.unwrap_or(50).min(filtered.len());
        filtered[filtered.len().saturating_sub(limit)..].to_vec()
    }
    
    /// Get audit statistics
    pub async fn get_audit_stats(&self) -> AuditStats {
        let events = self.events.read().await;
        
        let mut event_type_counts = HashMap::new();
        let mut flags_changed = std::collections::HashSet::new();
        let total_events = events.len();
        
        let oldest_event = events.first().map(|e| e.timestamp);
        let newest_event = events.last().map(|e| e.timestamp);
        
        for event in events.iter() {
            *event_type_counts.entry(event.event_type.clone()).or_insert(0) += 1;
            
            if let Some(flag_name) = &event.flag_name {
                flags_changed.insert(flag_name.clone());
            }
        }
        
        AuditStats {
            total_events,
            unique_flags_changed: flags_changed.len(),
            event_type_counts,
            oldest_event,
            newest_event,
            session_id: self.session_id.clone(),
        }
    }
    
    // Private helper methods
    
    async fn record_event(&self, mut event: AuditEvent) {
        // Log to tracing if enabled
        if self.config.use_tracing {
            self.log_event_to_tracing(&event);
        }
        
        // Write to file if configured
        if let Some(log_file) = &self.config.log_file {
            if let Err(e) = self.write_event_to_file(&event, log_file).await {
                error!(
                    audit = true,
                    error = %e,
                    "Failed to write audit event to file"
                );
            }
        }
        
        // Store in memory buffer
        let mut events = self.events.write().await;
        events.push(event.clone());
        
        // Record metrics for this audit event
        FeatureFlagMetrics::record_audit_event(&event);
        
        // Trim buffer if it exceeds max size
        if events.len() > self.config.max_events_in_memory {
            let excess = events.len() - self.config.max_events_in_memory;
            events.drain(0..excess);
        }
    }
    
    fn log_event_to_tracing(&self, event: &AuditEvent) {
        match event.event_type {
            AuditEventType::FlagToggled | AuditEventType::RolloutPercentageChanged => {
                info!(
                    audit = true,
                    event_id = %event.event_id,
                    event_type = ?event.event_type,
                    flag_name = event.flag_name.as_deref().unwrap_or("unknown"),
                    source = %event.source,
                    changed_by = event.changed_by.as_deref().unwrap_or("unknown"),
                    timestamp = %event.timestamp,
                    "Feature flag changed"
                );
            }
            AuditEventType::ValidationError => {
                warn!(
                    audit = true,
                    event_id = %event.event_id,
                    flag_name = event.flag_name.as_deref().unwrap_or("none"),
                    error = event.details.get("error_message").unwrap_or(&"unknown error".to_string()),
                    "Feature flag validation error"
                );
            }
            AuditEventType::ConfigurationReloaded | AuditEventType::HotReloadTriggered => {
                info!(
                    audit = true,
                    event_id = %event.event_id,
                    event_type = ?event.event_type,
                    source = %event.source,
                    config_file = event.config_file.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "none".to_string()),
                    "Feature flag configuration event"
                );
            }
            _ => {
                debug!(
                    audit = true,
                    event_id = %event.event_id,
                    event_type = ?event.event_type,
                    flag_name = event.flag_name.as_deref().unwrap_or("none"),
                    source = %event.source,
                    "Feature flag audit event"
                );
            }
        }
    }
    
    async fn write_event_to_file(&self, event: &AuditEvent, log_file: &PathBuf) -> Result<(), std::io::Error> {
        let json_line = serde_json::to_string(event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .await?;
            
        file.write_all(json_line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        
        if self.config.sync_writes {
            file.sync_all().await?;
        }
        
        Ok(())
    }
    
    fn generate_event_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("audit_{}", timestamp)
    }
    
    fn generate_session_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("session_{}", timestamp)
    }
    
    fn is_sensitive_metadata_key(key: &str) -> bool {
        let sensitive_keys = [
            "password", "secret", "key", "token", "credential", "auth",
            "private", "confidential", "sensitive"
        ];
        
        let key_lower = key.to_lowercase();
        sensitive_keys.iter().any(|&sensitive| key_lower.contains(sensitive))
    }
}

impl Default for FeatureFlagAuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Audit statistics for monitoring and reporting
#[derive(Debug, Clone, Serialize)]
pub struct AuditStats {
    pub total_events: usize,
    pub unique_flags_changed: usize,
    pub event_type_counts: HashMap<AuditEventType, usize>,
    pub oldest_event: Option<DateTime<Utc>>,
    pub newest_event: Option<DateTime<Utc>>,
    pub session_id: String,
}

impl AuditStats {
    /// Generate a human-readable summary report
    pub fn generate_summary(&self) -> String {
        let mut summary = String::new();
        summary.push_str("Feature Flag Audit Summary\n");
        summary.push_str("==========================\n\n");
        
        summary.push_str(&format!("Total Events: {}\n", self.total_events));
        summary.push_str(&format!("Unique Flags Changed: {}\n", self.unique_flags_changed));
        summary.push_str(&format!("Session ID: {}\n\n", self.session_id));
        
        if let (Some(oldest), Some(newest)) = (&self.oldest_event, &self.newest_event) {
            summary.push_str(&format!("Time Range: {} to {}\n", oldest, newest));
            let duration = newest.signed_duration_since(*oldest);
            summary.push_str(&format!("Duration: {} hours\n\n", duration.num_hours()));
        }
        
        summary.push_str("Event Types:\n");
        let mut sorted_events: Vec<_> = self.event_type_counts.iter().collect();
        sorted_events.sort_by(|a, b| b.1.cmp(a.1));
        
        for (event_type, count) in sorted_events {
            summary.push_str(&format!("  {:?}: {}\n", event_type, count));
        }
        
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    #[tokio::test]
    async fn test_audit_logger_creation() {
        let logger = FeatureFlagAuditLogger::new();
        assert!(logger.config.enabled);
        assert!(logger.config.use_tracing);
    }
    
    #[tokio::test]
    async fn test_flag_change_logging() {
        let logger = FeatureFlagAuditLogger::new();
        let flag = FeatureFlag::new("test_flag".to_string(), true);
        
        logger.log_flag_change("test_flag", None, &flag, "test").await;
        
        let events = logger.get_recent_events(Some(1)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, AuditEventType::FlagCreated);
        assert_eq!(events[0].flag_name, Some("test_flag".to_string()));
    }
    
    #[tokio::test]
    async fn test_configuration_reload_logging() {
        let logger = FeatureFlagAuditLogger::new();
        let config_path = PathBuf::from("test.toml");
        
        logger.log_configuration_reload(&config_path, 2, 1, 0, "file_watcher").await;
        
        let events = logger.get_recent_events(Some(1)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, AuditEventType::ConfigurationReloaded);
        assert_eq!(events[0].details.get("flags_changed"), Some(&"2".to_string()));
    }
    
    #[tokio::test] 
    async fn test_audit_stats() {
        let logger = FeatureFlagAuditLogger::new();
        let flag = FeatureFlag::new("test_flag".to_string(), true);
        
        logger.log_flag_change("test_flag", None, &flag, "test").await;
        logger.log_system_event("System started", "startup").await;
        
        let stats = logger.get_audit_stats().await;
        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.unique_flags_changed, 1);
        assert!(stats.event_type_counts.contains_key(&AuditEventType::FlagCreated));
        assert!(stats.event_type_counts.contains_key(&AuditEventType::SystemEvent));
    }
    
    #[tokio::test]
    async fn test_events_for_flag() {
        let logger = FeatureFlagAuditLogger::new();
        let flag1 = FeatureFlag::new("flag1".to_string(), true);
        let flag2 = FeatureFlag::new("flag2".to_string(), false);
        
        logger.log_flag_change("flag1", None, &flag1, "test").await;
        logger.log_flag_change("flag2", None, &flag2, "test").await;
        
        let flag1_events = logger.get_events_for_flag("flag1", None).await;
        assert_eq!(flag1_events.len(), 1);
        assert_eq!(flag1_events[0].flag_name, Some("flag1".to_string()));
        
        let flag2_events = logger.get_events_for_flag("flag2", None).await;
        assert_eq!(flag2_events.len(), 1);
        assert_eq!(flag2_events[0].flag_name, Some("flag2".to_string()));
    }
    
    #[tokio::test]
    async fn test_file_logging() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();
        
        let config = AuditConfig {
            enabled: true,
            log_file: Some(temp_path.clone()),
            use_tracing: false,
            include_metadata: false,
            max_events_in_memory: 100,
            sync_writes: true,
        };
        
        let logger = FeatureFlagAuditLogger::with_config(config);
        let flag = FeatureFlag::new("test_flag".to_string(), true);
        
        logger.log_flag_change("test_flag", None, &flag, "test").await;
        
        // Wait a bit for the file write to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        let contents = tokio::fs::read_to_string(&temp_path).await.unwrap();
        assert!(contents.contains("test_flag"));
        assert!(contents.contains("FlagCreated"));
    }
    
    #[test]
    fn test_sensitive_metadata_detection() {
        assert!(FeatureFlagAuditLogger::is_sensitive_metadata_key("password"));
        assert!(FeatureFlagAuditLogger::is_sensitive_metadata_key("api_secret"));
        assert!(FeatureFlagAuditLogger::is_sensitive_metadata_key("private_key"));
        assert!(!FeatureFlagAuditLogger::is_sensitive_metadata_key("owner"));
        assert!(!FeatureFlagAuditLogger::is_sensitive_metadata_key("description"));
    }
}