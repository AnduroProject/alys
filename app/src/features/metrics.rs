//! Metrics integration for the feature flag system
//!
//! This module provides comprehensive metrics collection for the feature flag system,
//! integrating with the existing Prometheus metrics infrastructure to track flag usage,
//! evaluation performance, cache operations, and operational events.

use super::types::*;
use super::audit::{AuditEvent, AuditEventType};
use crate::metrics::{
    FF_EVALUATIONS_TOTAL, FF_EVALUATION_DURATION, FF_CACHE_OPERATIONS_TOTAL,
    FF_ACTIVE_FLAGS, FF_ENABLED_FLAGS, FF_HOT_RELOAD_EVENTS_TOTAL,
    FF_CONFIG_RELOADS_TOTAL, FF_AUDIT_EVENTS_TOTAL, FF_FLAG_CHANGES_TOTAL,
    FF_VALIDATION_ERRORS_TOTAL, FF_MACRO_CACHE_HITS, FF_CONTEXT_BUILDS_TOTAL,
};

use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, warn};

/// Metrics collector for feature flag operations
pub struct FeatureFlagMetrics;

impl FeatureFlagMetrics {
    /// Record a feature flag evaluation
    pub fn record_evaluation(
        flag_name: &str,
        result: bool,
        evaluation_time_us: u64,
        cache_hit: bool,
    ) {
        let status = "success";
        let result_str = if result { "enabled" } else { "disabled" };
        let cache_status = if cache_hit { "hit" } else { "miss" };
        
        // Record evaluation counter
        FF_EVALUATIONS_TOTAL
            .with_label_values(&[flag_name, status, result_str])
            .inc();
        
        // Record evaluation duration (convert microseconds to seconds)
        let duration_seconds = evaluation_time_us as f64 / 1_000_000.0;
        FF_EVALUATION_DURATION
            .with_label_values(&[flag_name, cache_status])
            .observe(duration_seconds);
        
        debug!(
            flag_name = flag_name,
            result = result,
            evaluation_time_us = evaluation_time_us,
            cache_hit = cache_hit,
            "Recorded feature flag evaluation metrics"
        );
    }
    
    /// Record a failed feature flag evaluation
    pub fn record_evaluation_error(flag_name: &str, error: &str) {
        FF_EVALUATIONS_TOTAL
            .with_label_values(&[flag_name, "error", "false"])
            .inc();
        
        debug!(
            flag_name = flag_name,
            error = error,
            "Recorded feature flag evaluation error"
        );
    }
    
    /// Record cache operations
    pub fn record_cache_operation(operation: &str, flag_name: Option<&str>) {
        let flag = flag_name.unwrap_or("all");
        FF_CACHE_OPERATIONS_TOTAL
            .with_label_values(&[operation, flag])
            .inc();
    }
    
    /// Record macro cache hit
    pub fn record_macro_cache_hit(flag_name: &str) {
        FF_MACRO_CACHE_HITS
            .with_label_values(&[flag_name])
            .inc();
        
        Self::record_cache_operation("macro_hit", Some(flag_name));
    }
    
    /// Update flag count metrics
    pub fn update_flag_counts(flags: &HashMap<String, FeatureFlag>) {
        let total_flags = flags.len() as i64;
        let enabled_flags = flags.values().filter(|flag| flag.enabled).count() as i64;
        
        FF_ACTIVE_FLAGS.set(total_flags);
        FF_ENABLED_FLAGS.set(enabled_flags);
        
        debug!(
            total_flags = total_flags,
            enabled_flags = enabled_flags,
            "Updated feature flag count metrics"
        );
    }
    
    /// Record hot reload event
    pub fn record_hot_reload_event(status: &str) {
        FF_HOT_RELOAD_EVENTS_TOTAL
            .with_label_values(&[status])
            .inc();
        
        debug!(status = status, "Recorded hot reload event");
    }
    
    /// Record configuration reload
    pub fn record_config_reload(source: &str) {
        FF_CONFIG_RELOADS_TOTAL
            .with_label_values(&[source])
            .inc();
        
        debug!(source = source, "Recorded configuration reload");
    }
    
    /// Record audit event
    pub fn record_audit_event(event: &AuditEvent) {
        let event_type = Self::audit_event_type_to_string(&event.event_type);
        
        FF_AUDIT_EVENTS_TOTAL
            .with_label_values(&[&event_type])
            .inc();
        
        // Also record specific flag changes
        if let Some(flag_name) = &event.flag_name {
            let change_type = Self::get_change_type_from_audit_event(event);
            FF_FLAG_CHANGES_TOTAL
                .with_label_values(&[flag_name, &change_type])
                .inc();
        }
        
        debug!(
            event_type = event_type,
            flag_name = event.flag_name.as_deref().unwrap_or("none"),
            "Recorded audit event metrics"
        );
    }
    
    /// Record validation error
    pub fn record_validation_error(error_type: &str, flag_name: Option<&str>) {
        let flag = flag_name.unwrap_or("unknown");
        FF_VALIDATION_ERRORS_TOTAL
            .with_label_values(&[error_type, flag])
            .inc();
        
        warn!(
            error_type = error_type,
            flag_name = flag,
            "Recorded validation error"
        );
    }
    
    /// Record context build operation
    pub fn record_context_build(success: bool) {
        let status = if success { "success" } else { "error" };
        FF_CONTEXT_BUILDS_TOTAL
            .with_label_values(&[status])
            .inc();
    }
    
    /// Record bulk flag changes from configuration reload
    pub fn record_bulk_flag_changes(
        flags_changed: usize,
        flags_added: usize,
        flags_removed: usize,
    ) {
        debug!(
            flags_changed = flags_changed,
            flags_added = flags_added,
            flags_removed = flags_removed,
            "Recorded bulk flag changes"
        );
        
        // Individual flag changes will be recorded separately via audit events
        // This is just for monitoring bulk operations
    }
    
    // Helper methods
    
    fn audit_event_type_to_string(event_type: &AuditEventType) -> String {
        match event_type {
            AuditEventType::FlagToggled => "flag_toggled",
            AuditEventType::RolloutPercentageChanged => "rollout_changed",
            AuditEventType::TargetingChanged => "targeting_changed",
            AuditEventType::ConditionsChanged => "conditions_changed",
            AuditEventType::FlagCreated => "flag_created",
            AuditEventType::FlagDeleted => "flag_deleted",
            AuditEventType::MetadataChanged => "metadata_changed",
            AuditEventType::ConfigurationReloaded => "config_reloaded",
            AuditEventType::HotReloadTriggered => "hot_reload_triggered",
            AuditEventType::ValidationError => "validation_error",
            AuditEventType::SystemEvent => "system_event",
        }.to_string()
    }
    
    fn get_change_type_from_audit_event(event: &AuditEvent) -> String {
        match event.event_type {
            AuditEventType::FlagToggled => {
                if let Some(new_value) = &event.new_value {
                    if new_value.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                } else {
                    "toggled"
                }
            },
            AuditEventType::RolloutPercentageChanged => "rollout",
            AuditEventType::TargetingChanged => "targeting",
            AuditEventType::ConditionsChanged => "conditions",
            AuditEventType::FlagCreated => "created",
            AuditEventType::FlagDeleted => "deleted",
            AuditEventType::MetadataChanged => "metadata",
            _ => "other",
        }.to_string()
    }
}

/// Timed evaluation wrapper for automatic metrics collection
pub struct TimedEvaluation {
    flag_name: String,
    start_time: Instant,
}

impl TimedEvaluation {
    /// Start timing a flag evaluation
    pub fn start(flag_name: &str) -> Self {
        Self {
            flag_name: flag_name.to_string(),
            start_time: Instant::now(),
        }
    }
    
    /// Complete the evaluation and record metrics
    pub fn complete(self, result: bool, cache_hit: bool) {
        let evaluation_time_us = self.start_time.elapsed().as_micros() as u64;
        FeatureFlagMetrics::record_evaluation(&self.flag_name, result, evaluation_time_us, cache_hit);
    }
    
    /// Complete the evaluation with an error and record metrics
    pub fn complete_with_error(self, error: &str) {
        FeatureFlagMetrics::record_evaluation_error(&self.flag_name, error);
    }
}

/// Convenience macro for timing flag evaluations with automatic metrics collection
#[macro_export]
macro_rules! timed_flag_evaluation {
    ($flag_name:expr, $evaluation:expr) => {{
        let timer = $crate::features::metrics::TimedEvaluation::start($flag_name);
        match $evaluation {
            Ok(result) => {
                timer.complete(result, false); // Assume cache miss for manual evaluations
                Ok(result)
            }
            Err(e) => {
                timer.complete_with_error(&e.to_string());
                Err(e)
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audit_event_type_conversion() {
        assert_eq!(
            FeatureFlagMetrics::audit_event_type_to_string(&AuditEventType::FlagToggled),
            "flag_toggled"
        );
        assert_eq!(
            FeatureFlagMetrics::audit_event_type_to_string(&AuditEventType::ConfigurationReloaded),
            "config_reloaded"
        );
    }
    
    #[test] 
    fn test_change_type_from_audit_event() {
        use super::super::audit::{AuditEvent, AuditFlagState};
        use chrono::Utc;
        use std::collections::HashMap;
        
        let event = AuditEvent {
            event_id: "test".to_string(),
            timestamp: Utc::now(),
            event_type: AuditEventType::FlagToggled,
            flag_name: Some("test_flag".to_string()),
            old_value: None,
            new_value: Some(AuditFlagState {
                enabled: true,
                rollout_percentage: None,
                has_targeting: false,
                has_conditions: false,
                metadata_keys: vec![],
            }),
            source: "test".to_string(),
            changed_by: Some("test".to_string()),
            details: HashMap::new(),
            environment: None,
            config_file: None,
        };
        
        assert_eq!(
            FeatureFlagMetrics::get_change_type_from_audit_event(&event),
            "enabled"
        );
    }
    
    #[tokio::test]
    async fn test_timed_evaluation() {
        let timer = TimedEvaluation::start("test_flag");
        
        // Simulate some work
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        
        timer.complete(true, false);
        // Metrics should be recorded (this is tested via integration tests)
    }
}