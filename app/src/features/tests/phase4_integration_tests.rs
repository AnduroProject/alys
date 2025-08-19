//! Integration tests for Phase 4: Basic Logging & Metrics Integration
//! 
//! This module provides comprehensive tests for ALYS-004-11 and ALYS-004-12:
//! - Audit logging for flag changes detected through file watcher
//! - Metrics integration for flag usage tracking and evaluation performance monitoring

use super::super::*;
use crate::metrics::{
    FF_EVALUATIONS_TOTAL, FF_EVALUATION_DURATION, FF_CACHE_OPERATIONS_TOTAL,
    FF_ACTIVE_FLAGS, FF_ENABLED_FLAGS, FF_HOT_RELOAD_EVENTS_TOTAL,
    FF_CONFIG_RELOADS_TOTAL, FF_AUDIT_EVENTS_TOTAL, FF_FLAG_CHANGES_TOTAL,
    FF_MACRO_CACHE_HITS,
};

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tempfile::{NamedTempFile, TempDir};
use tokio::time::timeout;
use prometheus::{Encoder, TextEncoder};

/// Test suite for audit logging functionality (ALYS-004-11)
mod audit_logging_tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_logger_initialization() {
        let config = AuditConfig {
            enabled: true,
            use_tracing: true,
            include_metadata: false,
            max_events_in_memory: 100,
            sync_writes: false,
            ..Default::default()
        };
        
        let logger = FeatureFlagAuditLogger::with_config(config);
        let stats = logger.get_audit_stats().await;
        
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.unique_flags_changed, 0);
        assert!(!stats.session_id.is_empty());
    }

    #[tokio::test]
    async fn test_comprehensive_flag_change_audit() {
        let logger = FeatureFlagAuditLogger::new();
        
        // Test flag creation
        let new_flag = FeatureFlag::new("test_flag".to_string(), true);
        logger.log_flag_change("test_flag", None, &new_flag, "test_source").await;
        
        // Test flag modification
        let mut modified_flag = new_flag.clone();
        modified_flag.enabled = false;
        modified_flag.rollout_percentage = Some(50);
        
        logger.log_flag_change("test_flag", Some(&new_flag), &modified_flag, "test_source").await;
        
        // Test flag deletion
        logger.log_flag_deleted("test_flag", &modified_flag, "test_source").await;
        
        // Verify audit events
        let events = logger.get_recent_events(Some(10)).await;
        assert_eq!(events.len(), 3);
        
        // Check event types
        assert_eq!(events[0].event_type, AuditEventType::FlagCreated);
        assert_eq!(events[1].event_type, AuditEventType::FlagToggled); // enabled changed first
        assert_eq!(events[2].event_type, AuditEventType::FlagDeleted);
        
        // Verify flag-specific events
        let flag_events = logger.get_events_for_flag("test_flag", None).await;
        assert_eq!(flag_events.len(), 3);
    }

    #[tokio::test] 
    async fn test_configuration_reload_audit() {
        let logger = FeatureFlagAuditLogger::new();
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        logger.log_configuration_reload(&config_path, 5, 2, 1, "hot_reload").await;
        
        let events = logger.get_recent_events(Some(1)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, AuditEventType::ConfigurationReloaded);
        assert_eq!(events[0].details.get("flags_changed"), Some(&"5".to_string()));
        assert_eq!(events[0].details.get("flags_added"), Some(&"2".to_string()));
        assert_eq!(events[0].details.get("flags_removed"), Some(&"1".to_string()));
    }

    #[tokio::test]
    async fn test_hot_reload_trigger_audit() {
        let logger = FeatureFlagAuditLogger::new();
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        
        logger.log_hot_reload_triggered(&config_path).await;
        
        let events = logger.get_recent_events(Some(1)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, AuditEventType::HotReloadTriggered);
        assert_eq!(events[0].source, "file_watcher");
    }

    #[tokio::test]
    async fn test_validation_error_audit() {
        let logger = FeatureFlagAuditLogger::new();
        
        logger.log_validation_error(
            "Invalid rollout percentage: 150",
            None,
            Some("invalid_flag")
        ).await;
        
        let events = logger.get_recent_events(Some(1)).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, AuditEventType::ValidationError);
        assert_eq!(events[0].flag_name, Some("invalid_flag".to_string()));
    }

    #[tokio::test]
    async fn test_audit_file_logging() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();
        
        let config = AuditConfig {
            enabled: true,
            log_file: Some(temp_path.clone()),
            use_tracing: false,
            include_metadata: false,
            max_events_in_memory: 10,
            sync_writes: true,
        };
        
        let logger = FeatureFlagAuditLogger::with_config(config);
        let flag = FeatureFlag::new("file_test_flag".to_string(), true);
        
        logger.log_flag_change("file_test_flag", None, &flag, "file_test").await;
        
        // Wait for file write
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let contents = tokio::fs::read_to_string(&temp_path).await.unwrap();
        assert!(contents.contains("file_test_flag"));
        assert!(contents.contains("FlagCreated"));
        assert!(contents.contains("file_test"));
    }

    #[tokio::test]
    async fn test_audit_statistics_generation() {
        let logger = FeatureFlagAuditLogger::new();
        let flag1 = FeatureFlag::new("flag1".to_string(), true);
        let flag2 = FeatureFlag::new("flag2".to_string(), false);
        
        // Generate various audit events
        logger.log_flag_change("flag1", None, &flag1, "test").await;
        logger.log_flag_change("flag2", None, &flag2, "test").await;
        logger.log_system_event("Test system event", "test").await;
        
        let stats = logger.get_audit_stats().await;
        assert_eq!(stats.total_events, 3);
        assert_eq!(stats.unique_flags_changed, 2);
        assert!(stats.event_type_counts.contains_key(&AuditEventType::FlagCreated));
        assert!(stats.event_type_counts.contains_key(&AuditEventType::SystemEvent));
        assert_eq!(stats.event_type_counts[&AuditEventType::FlagCreated], 2);
        assert_eq!(stats.event_type_counts[&AuditEventType::SystemEvent], 1);
        
        // Test summary generation
        let summary = stats.generate_summary();
        assert!(summary.contains("Total Events: 3"));
        assert!(summary.contains("Unique Flags Changed: 2"));
        assert!(summary.contains("FlagCreated: 2"));
        assert!(summary.contains("SystemEvent: 1"));
    }

    #[tokio::test]
    async fn test_sensitive_metadata_filtering() {
        let logger = FeatureFlagAuditLogger::with_config(AuditConfig {
            include_metadata: true,
            ..Default::default()
        });
        
        let mut flag = FeatureFlag::new("metadata_test".to_string(), true);
        flag.metadata.insert("owner".to_string(), "test_team".to_string());
        flag.metadata.insert("api_secret".to_string(), "secret_value".to_string());
        flag.metadata.insert("password".to_string(), "secure_password".to_string());
        
        logger.log_flag_change("metadata_test", None, &flag, "test").await;
        
        let events = logger.get_recent_events(Some(1)).await;
        let event = &events[0];
        
        // Should include non-sensitive metadata
        assert!(event.details.contains_key("metadata.owner"));
        // Should exclude sensitive metadata
        assert!(!event.details.contains_key("metadata.api_secret"));
        assert!(!event.details.contains_key("metadata.password"));
    }
}

/// Test suite for metrics integration functionality (ALYS-004-12)
mod metrics_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_flag_evaluation_metrics() {
        // Record successful evaluation
        FeatureFlagMetrics::record_evaluation("test_flag", true, 1500, false);
        FeatureFlagMetrics::record_evaluation("test_flag", false, 800, true);
        
        // Record evaluation error
        FeatureFlagMetrics::record_evaluation_error("error_flag", "Flag not found");
        
        // Verify metrics were recorded (check via Prometheus registry)
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let evaluations_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_evaluations_total")
            .expect("Evaluations metric not found");
            
        // Should have at least 3 samples (2 successful + 1 error)
        let mut total_samples = 0;
        for metric in evaluations_metric.get_metric() {
            total_samples += metric.get_counter().get_value() as u32;
        }
        assert!(total_samples >= 3);
    }

    #[tokio::test]
    async fn test_cache_operation_metrics() {
        FeatureFlagMetrics::record_cache_operation("hit", Some("cached_flag"));
        FeatureFlagMetrics::record_cache_operation("miss", Some("uncached_flag"));
        FeatureFlagMetrics::record_cache_operation("store", Some("new_flag"));
        FeatureFlagMetrics::record_cache_operation("clear", None);
        
        // Verify cache metrics
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let cache_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_cache_operations_total")
            .expect("Cache operations metric not found");
            
        assert!(cache_metric.get_metric().len() >= 4); // At least 4 different operations
    }

    #[tokio::test]
    async fn test_flag_count_metrics() {
        let mut flags = HashMap::new();
        flags.insert("flag1".to_string(), FeatureFlag::new("flag1".to_string(), true));
        flags.insert("flag2".to_string(), FeatureFlag::new("flag2".to_string(), false));
        flags.insert("flag3".to_string(), FeatureFlag::new("flag3".to_string(), true));
        
        FeatureFlagMetrics::update_flag_counts(&flags);
        
        // Check active flags count
        assert_eq!(FF_ACTIVE_FLAGS.get() as usize, 3);
        // Check enabled flags count  
        assert_eq!(FF_ENABLED_FLAGS.get() as usize, 2);
    }

    #[tokio::test]
    async fn test_hot_reload_metrics() {
        FeatureFlagMetrics::record_hot_reload_event("success");
        FeatureFlagMetrics::record_hot_reload_event("error");
        FeatureFlagMetrics::record_hot_reload_event("file_deleted");
        
        // Verify hot reload metrics recorded
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let hot_reload_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_hot_reload_events_total")
            .expect("Hot reload metric not found");
            
        assert_eq!(hot_reload_metric.get_metric().len(), 3); // 3 different statuses
    }

    #[tokio::test]
    async fn test_configuration_reload_metrics() {
        FeatureFlagMetrics::record_config_reload("startup");
        FeatureFlagMetrics::record_config_reload("hot_reload");
        FeatureFlagMetrics::record_config_reload("manual");
        
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let config_reload_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_config_reloads_total")
            .expect("Config reload metric not found");
            
        assert_eq!(config_reload_metric.get_metric().len(), 3); // 3 different sources
    }

    #[tokio::test]
    async fn test_audit_event_metrics_integration() {
        let flag = FeatureFlag::new("audit_metrics_test".to_string(), true);
        
        // Create audit event
        let event = AuditEvent {
            event_id: "test_event".to_string(),
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::FlagToggled,
            flag_name: Some("audit_metrics_test".to_string()),
            old_value: None,
            new_value: Some(AuditFlagState::from(&flag)),
            source: "test".to_string(),
            changed_by: Some("test_user".to_string()),
            details: HashMap::new(),
            environment: None,
            config_file: None,
        };
        
        // Record audit event (should also trigger metrics)
        FeatureFlagMetrics::record_audit_event(&event);
        
        // Verify both audit and flag change metrics were recorded
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        
        let audit_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_audit_events_total")
            .expect("Audit events metric not found");
            
        let flag_change_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_changes_total")
            .expect("Flag changes metric not found");
            
        // Should have recorded audit event
        assert!(!audit_metric.get_metric().is_empty());
        // Should have recorded flag change
        assert!(!flag_change_metric.get_metric().is_empty());
    }

    #[tokio::test]
    async fn test_macro_cache_metrics() {
        FeatureFlagMetrics::record_macro_cache_hit("macro_flag_1");
        FeatureFlagMetrics::record_macro_cache_hit("macro_flag_2");
        FeatureFlagMetrics::record_macro_cache_hit("macro_flag_1"); // Second hit
        
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let macro_cache_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_macro_cache_hits_total")
            .expect("Macro cache hits metric not found");
            
        // Should have metrics for both flags
        assert_eq!(macro_cache_metric.get_metric().len(), 2);
    }

    #[tokio::test]
    async fn test_validation_error_metrics() {
        FeatureFlagMetrics::record_validation_error("invalid_percentage", Some("bad_flag"));
        FeatureFlagMetrics::record_validation_error("missing_field", Some("incomplete_flag"));
        FeatureFlagMetrics::record_validation_error("schema_error", None);
        
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let validation_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_validation_errors_total")
            .expect("Validation errors metric not found");
            
        assert_eq!(validation_metric.get_metric().len(), 3); // 3 different errors
    }

    #[tokio::test]
    async fn test_context_build_metrics() {
        FeatureFlagMetrics::record_context_build(true);
        FeatureFlagMetrics::record_context_build(true);
        FeatureFlagMetrics::record_context_build(false); // error case
        
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let context_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_context_builds_total")
            .expect("Context builds metric not found");
            
        assert_eq!(context_metric.get_metric().len(), 2); // success and error
    }
}

/// Integration tests combining audit logging and metrics
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_evaluation_with_audit_and_metrics() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("test_config.toml");
        
        // Create test configuration
        let config_content = r#"
[global_settings]
cache_ttl_seconds = 300
enable_audit_log = true

[global_settings.percentage_rollout]
hash_algorithm = "sha256"

[[flags]]
name = "integration_test_flag"
enabled = true
rollout_percentage = 100

[flags.metadata]
owner = "integration_tests"
"#;
        
        tokio::fs::write(&config_file, config_content).await.unwrap();
        
        // Create manager and evaluate flag
        let manager = FeatureFlagManager::new(config_file.clone()).unwrap();
        let context = EvaluationContext::new()
            .with_user_id("test_user".to_string())
            .with_environment(Environment::Development);
        
        // Perform evaluations to generate audit logs and metrics
        let result1 = manager.is_enabled("integration_test_flag", &context).await;
        let result2 = manager.is_enabled("integration_test_flag", &context).await; // Should hit cache
        let result3 = manager.is_enabled("nonexistent_flag", &context).await; // Should be false
        
        assert!(result1);
        assert!(result2);
        assert!(!result3);
        
        // Verify audit events were created
        let stats = manager.get_stats().await;
        assert!(stats.total_evaluations >= 3);
        assert!(stats.cache_hits >= 1); // Second evaluation should hit cache
        
        // Verify metrics were recorded 
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let evaluations_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_evaluations_total");
        assert!(evaluations_metric.is_some());
    }

    #[tokio::test]
    async fn test_hot_reload_audit_and_metrics_integration() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("hot_reload_test.toml");
        
        // Initial configuration
        let initial_config = r#"
[global_settings]
cache_ttl_seconds = 300
enable_audit_log = true

[[flags]]
name = "hot_reload_flag"
enabled = true
"#;
        
        tokio::fs::write(&config_file, initial_config).await.unwrap();
        
        let mut manager = FeatureFlagManager::new(config_file.clone()).unwrap();
        manager.start_hot_reload().await.unwrap();
        
        // Wait for hot reload to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Modify configuration
        let updated_config = r#"
[global_settings]
cache_ttl_seconds = 300
enable_audit_log = true

[[flags]]
name = "hot_reload_flag"
enabled = false

[[flags]]
name = "new_flag"
enabled = true
"#;
        
        tokio::fs::write(&config_file, updated_config).await.unwrap();
        
        // Wait for hot reload to trigger
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Verify flags were updated
        let flags = manager.list_flags().await;
        assert!(flags.contains(&"hot_reload_flag".to_string()));
        assert!(flags.contains(&"new_flag".to_string()));
        
        // Verify stats updated
        let stats = manager.get_stats().await;
        assert!(stats.hot_reloads >= 1);
        
        manager.stop_hot_reload().await.unwrap();
    }

    #[tokio::test] 
    async fn test_performance_metrics_collection() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("performance_test.toml");
        
        let config_content = r#"
[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true

[[flags]]
name = "perf_flag_1"
enabled = true

[[flags]]
name = "perf_flag_2"  
enabled = false
"#;
        
        tokio::fs::write(&config_file, config_content).await.unwrap();
        
        let manager = FeatureFlagManager::new(config_file).unwrap();
        let context = EvaluationContext::new().with_user_id("perf_test".to_string());
        
        // Perform many evaluations to test performance metrics
        let start = Instant::now();
        let mut results = Vec::new();
        
        for i in 0..100 {
            let flag_name = if i % 2 == 0 { "perf_flag_1" } else { "perf_flag_2" };
            let result = manager.is_enabled(flag_name, &context).await;
            results.push(result);
        }
        
        let total_time = start.elapsed();
        
        // Verify performance is reasonable (should be much less than 1ms per evaluation on average)
        assert!(total_time.as_millis() < 100, "Performance test took too long: {}ms", total_time.as_millis());
        
        // Verify metrics captured the evaluations
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let duration_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_evaluation_duration_seconds");
        assert!(duration_metric.is_some());
        
        // Check that at least some evaluations were recorded
        let evaluations_metric = metric_families.iter()
            .find(|mf| mf.get_name() == "alys_feature_flag_evaluations_total")
            .unwrap();
            
        let mut total_evaluations = 0;
        for metric in evaluations_metric.get_metric() {
            total_evaluations += metric.get_counter().get_value() as u32;
        }
        assert!(total_evaluations >= 100);
    }

    #[tokio::test]
    async fn test_metrics_endpoint_integration() {
        // Generate some metrics
        FeatureFlagMetrics::record_evaluation("endpoint_test", true, 1000, false);
        FeatureFlagMetrics::record_hot_reload_event("success");
        FeatureFlagMetrics::record_config_reload("test");
        
        // Gather metrics (simulating /metrics endpoint)
        let metric_families = crate::metrics::ALYS_REGISTRY.gather();
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        
        let metrics_output = String::from_utf8(buffer).unwrap();
        
        // Verify feature flag metrics are present in output
        assert!(metrics_output.contains("alys_feature_flag_evaluations_total"));
        assert!(metrics_output.contains("alys_feature_flag_hot_reload_events_total"));
        assert!(metrics_output.contains("alys_feature_flag_config_reloads_total"));
        assert!(metrics_output.contains("endpoint_test"));
    }
}

/// Performance benchmarks for logging and metrics overhead
mod performance_benchmarks {
    use super::*;

    #[tokio::test]
    async fn bench_audit_logging_overhead() {
        let logger = FeatureFlagAuditLogger::with_config(AuditConfig {
            use_tracing: false, // Disable tracing for pure audit performance
            log_file: None, // Disable file logging
            ..Default::default()
        });
        
        let flag = FeatureFlag::new("bench_flag".to_string(), true);
        let iterations = 1000;
        
        let start = Instant::now();
        
        for i in 0..iterations {
            logger.log_flag_change(&format!("bench_flag_{}", i), None, &flag, "benchmark").await;
        }
        
        let elapsed = start.elapsed();
        let avg_time_us = elapsed.as_micros() / iterations;
        
        println!("Audit logging benchmark: {} iterations in {:?}, avg: {}μs per audit", 
                 iterations, elapsed, avg_time_us);
        
        // Should be reasonable performance (less than 100μs per audit log on average)
        assert!(avg_time_us < 100, "Audit logging too slow: {}μs per audit", avg_time_us);
    }

    #[tokio::test]
    async fn bench_metrics_collection_overhead() {
        let iterations = 10000;
        let start = Instant::now();
        
        for i in 0..iterations {
            FeatureFlagMetrics::record_evaluation(&format!("bench_flag_{}", i % 100), true, 1000, false);
        }
        
        let elapsed = start.elapsed();
        let avg_time_ns = elapsed.as_nanos() / iterations;
        
        println!("Metrics collection benchmark: {} iterations in {:?}, avg: {}ns per metric", 
                 iterations, elapsed, avg_time_ns);
        
        // Should be very fast (less than 10μs per metric on average)
        assert!(avg_time_ns < 10_000, "Metrics collection too slow: {}ns per metric", avg_time_ns);
    }

    #[tokio::test]
    async fn bench_integrated_audit_and_metrics() {
        let logger = FeatureFlagAuditLogger::with_config(AuditConfig {
            use_tracing: false,
            log_file: None,
            ..Default::default()
        });
        
        let flag = FeatureFlag::new("integrated_bench".to_string(), true);
        let iterations = 1000;
        
        let start = Instant::now();
        
        for i in 0..iterations {
            // Both audit logging and metrics (as would happen in real usage)
            logger.log_flag_change("integrated_bench", None, &flag, "benchmark").await;
            FeatureFlagMetrics::record_evaluation("integrated_bench", true, 1500, false);
        }
        
        let elapsed = start.elapsed();
        let avg_time_us = elapsed.as_micros() / iterations;
        
        println!("Integrated audit+metrics benchmark: {} iterations in {:?}, avg: {}μs per operation", 
                 iterations, elapsed, avg_time_us);
        
        // Should still be reasonable with both systems (less than 150μs per operation)  
        assert!(avg_time_us < 150, "Integrated logging+metrics too slow: {}μs per operation", avg_time_us);
    }
}

#[cfg(test)]
mod test_utilities {
    use super::*;

    /// Helper to create a temporary configuration file for testing
    pub async fn create_test_config(flags: &[(&str, bool)]) -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("test_config.toml");
        
        let mut config_content = String::from(r#"
[global_settings]
cache_ttl_seconds = 300
enable_audit_log = true

[global_settings.percentage_rollout]
hash_algorithm = "sha256"

"#);
        
        for (name, enabled) in flags {
            config_content.push_str(&format!(r#"
[[flags]]
name = "{}"
enabled = {}

[flags.metadata]
test = "true"
"#, name, enabled));
        }
        
        tokio::fs::write(&config_file, config_content).await.unwrap();
        (temp_dir, config_file)
    }
    
    /// Helper to reset metrics between tests (for isolated testing)
    pub fn reset_test_metrics() {
        // Note: In a real implementation, you might want to create 
        // separate registries for testing to avoid cross-test pollution
        FF_ACTIVE_FLAGS.set(0);
        FF_ENABLED_FLAGS.set(0);
    }
}