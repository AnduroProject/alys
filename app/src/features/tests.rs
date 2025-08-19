//! Comprehensive unit tests for the feature flag system
//!
//! This module contains tests for all core components of the feature flag system,
//! including evaluation logic, targeting, caching, and configuration loading.

// Phase 4 (Logging & Metrics) Integration Tests
#[cfg(test)]
mod phase4_tests;

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::super::types::*;
    use super::super::context::*;
    use super::super::evaluation::*;
    use super::super::manager::*;
    use super::super::cache::*;
    use super::super::config::*;
    use crate::config::Environment;
    
    use std::collections::HashMap;
    use tempfile::NamedTempFile;
    use tokio::time::Duration;
    use chrono::{Utc, TimeZone};
    use std::io::Write;

    // Test data structures

    fn create_test_context() -> EvaluationContext {
        EvaluationContext::new("test-node-1".to_string(), Environment::Development)
            .with_chain_state(1500, 0.95)
            .with_custom_attribute("region".to_string(), "us-west".to_string())
    }

    fn create_test_context_with_validator() -> EvaluationContext {
        create_test_context()
            .with_validator_key("validator-key-123".to_string())
    }

    // Basic Feature Flag Tests

    #[test]
    fn test_feature_flag_creation() {
        let flag = FeatureFlag::enabled("test_feature".to_string())
            .with_description("Test feature flag".to_string())
            .with_metadata("owner".to_string(), "test-team".to_string());

        assert_eq!(flag.name, "test_feature");
        assert!(flag.enabled);
        assert_eq!(flag.description, Some("Test feature flag".to_string()));
        assert_eq!(flag.metadata.get("owner"), Some(&"test-team".to_string()));
    }

    #[test]
    fn test_feature_flag_with_percentage() {
        let flag = FeatureFlag::with_percentage("test_feature".to_string(), true, 75);
        
        assert_eq!(flag.name, "test_feature");
        assert!(flag.enabled);
        assert_eq!(flag.rollout_percentage, Some(75));
    }

    #[test]
    fn test_feature_targets() {
        let targets = FeatureTargets::new()
            .with_node_ids(vec!["node-1".to_string(), "node-2".to_string()])
            .with_environments(vec![Environment::Testing, Environment::Development])
            .with_custom_attributes({
                let mut attrs = HashMap::new();
                attrs.insert("team".to_string(), "platform".to_string());
                attrs
            });

        assert_eq!(targets.node_ids.as_ref().unwrap().len(), 2);
        assert_eq!(targets.environments.as_ref().unwrap().len(), 2);
        assert!(targets.custom_attributes.is_some());
    }

    #[test]
    fn test_feature_conditions() {
        let conditions = vec![
            FeatureCondition::ChainHeightAbove(1000),
            FeatureCondition::SyncProgressAbove(0.9),
            FeatureCondition::After(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
        ];

        let flag = FeatureFlag::enabled("test_feature".to_string())
            .with_conditions(conditions);

        assert!(flag.conditions.is_some());
        assert_eq!(flag.conditions.as_ref().unwrap().len(), 3);
    }

    // Evaluation Context Tests

    #[test]
    fn test_evaluation_context_creation() {
        let context = create_test_context();
        
        assert_eq!(context.node_id, "test-node-1");
        assert_eq!(context.environment, Environment::Development);
        assert_eq!(context.chain_height, 1500);
        assert_eq!(context.sync_progress, 0.95);
        assert!(context.custom_attributes.contains_key("region"));
    }

    #[test]
    fn test_evaluation_context_hashing() {
        let context1 = create_test_context();
        let context2 = create_test_context();
        let context3 = EvaluationContext::new("different-node".to_string(), Environment::Development);

        // Same contexts should have same hash
        assert_eq!(context1.hash(), context2.hash());
        
        // Different contexts should have different hashes (very likely)
        assert_ne!(context1.hash(), context3.hash());
    }

    #[test]
    fn test_stable_id_generation() {
        let context_without_validator = create_test_context();
        let context_with_validator = create_test_context_with_validator();

        assert_eq!(context_without_validator.stable_id(), "test-node-1");
        assert_eq!(context_with_validator.stable_id(), "test-node-1:validator-key-123");
    }

    // Evaluation Logic Tests

    #[tokio::test]
    async fn test_basic_flag_evaluation() {
        let evaluator = FeatureFlagEvaluator::new();
        let context = create_test_context();

        // Test enabled flag
        let enabled_flag = FeatureFlag::enabled("test_enabled".to_string());
        let result = evaluator.evaluate_flag(&enabled_flag, &context).await.unwrap();
        assert!(result);

        // Test disabled flag
        let disabled_flag = FeatureFlag::disabled("test_disabled".to_string());
        let result = evaluator.evaluate_flag(&disabled_flag, &context).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_percentage_rollout_evaluation() {
        let evaluator = FeatureFlagEvaluator::new();
        
        // Test 0% rollout
        let zero_percent_flag = FeatureFlag::with_percentage("test_0".to_string(), true, 0);
        let context = create_test_context();
        let result = evaluator.evaluate_flag(&zero_percent_flag, &context).await.unwrap();
        assert!(!result);

        // Test 100% rollout
        let hundred_percent_flag = FeatureFlag::with_percentage("test_100".to_string(), true, 100);
        let result = evaluator.evaluate_flag(&hundred_percent_flag, &context).await.unwrap();
        assert!(result);

        // Test percentage distribution
        let fifty_percent_flag = FeatureFlag::with_percentage("test_50".to_string(), true, 50);
        let mut enabled_count = 0;
        
        for i in 0..1000 {
            let test_context = EvaluationContext::new(format!("node-{}", i), Environment::Development);
            if evaluator.evaluate_flag(&fifty_percent_flag, &test_context).await.unwrap() {
                enabled_count += 1;
            }
        }

        // Should be approximately 50% (allowing for variance)
        assert!(enabled_count > 400 && enabled_count < 600, "Got {} enabled out of 1000", enabled_count);
    }

    #[tokio::test]
    async fn test_condition_evaluation() {
        let evaluator = FeatureFlagEvaluator::new();
        let context = create_test_context(); // chain_height = 1500, sync_progress = 0.95

        // Test chain height condition (should pass)
        let chain_height_flag = FeatureFlag::enabled("test_chain_height".to_string())
            .with_conditions(vec![FeatureCondition::ChainHeightAbove(1000)]);
        let result = evaluator.evaluate_flag(&chain_height_flag, &context).await.unwrap();
        assert!(result);

        // Test chain height condition (should fail)
        let chain_height_flag_fail = FeatureFlag::enabled("test_chain_height_fail".to_string())
            .with_conditions(vec![FeatureCondition::ChainHeightAbove(2000)]);
        let result = evaluator.evaluate_flag(&chain_height_flag_fail, &context).await.unwrap();
        assert!(!result);

        // Test sync progress condition (should pass)
        let sync_progress_flag = FeatureFlag::enabled("test_sync_progress".to_string())
            .with_conditions(vec![FeatureCondition::SyncProgressAbove(0.8)]);
        let result = evaluator.evaluate_flag(&sync_progress_flag, &context).await.unwrap();
        assert!(result);

        // Test multiple conditions (all must pass)
        let multi_condition_flag = FeatureFlag::enabled("test_multi".to_string())
            .with_conditions(vec![
                FeatureCondition::ChainHeightAbove(1000),
                FeatureCondition::SyncProgressAbove(0.9),
            ]);
        let result = evaluator.evaluate_flag(&multi_condition_flag, &context).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_targeting_evaluation() {
        let evaluator = FeatureFlagEvaluator::new();
        let context = create_test_context(); // node_id = "test-node-1"

        // Test node ID targeting (should match)
        let node_targeting_flag = FeatureFlag::enabled("test_node_targeting".to_string())
            .with_targets(FeatureTargets::new().with_node_ids(vec!["test-node-1".to_string()]));
        let result = evaluator.evaluate_flag(&node_targeting_flag, &context).await.unwrap();
        assert!(result);

        // Test node ID targeting (should not match)
        let node_targeting_flag_fail = FeatureFlag::enabled("test_node_targeting_fail".to_string())
            .with_targets(FeatureTargets::new().with_node_ids(vec!["other-node".to_string()]));
        let result = evaluator.evaluate_flag(&node_targeting_flag_fail, &context).await.unwrap();
        assert!(!result);

        // Test environment targeting
        let env_targeting_flag = FeatureFlag::enabled("test_env_targeting".to_string())
            .with_targets(FeatureTargets::new().with_environments(vec![Environment::Development]));
        let result = evaluator.evaluate_flag(&env_targeting_flag, &context).await.unwrap();
        assert!(result);

        // Test custom attribute targeting
        let custom_targeting_flag = FeatureFlag::enabled("test_custom_targeting".to_string())
            .with_targets(FeatureTargets::new().with_custom_attributes({
                let mut attrs = HashMap::new();
                attrs.insert("region".to_string(), "us-west".to_string());
                attrs
            }));
        let result = evaluator.evaluate_flag(&custom_targeting_flag, &context).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_time_window_condition() {
        let evaluator = FeatureFlagEvaluator::new();
        let mut context = create_test_context();
        
        // Set evaluation time to 10 AM UTC
        let test_time = Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap();
        context.evaluation_time = test_time;

        // Test time window that includes 10 AM (9-11)
        let time_window_flag = FeatureFlag::enabled("test_time_window".to_string())
            .with_conditions(vec![FeatureCondition::TimeWindow { start_hour: 9, end_hour: 11 }]);
        let result = evaluator.evaluate_flag(&time_window_flag, &context).await.unwrap();
        assert!(result);

        // Test time window that excludes 10 AM (12-14)
        let time_window_flag_fail = FeatureFlag::enabled("test_time_window_fail".to_string())
            .with_conditions(vec![FeatureCondition::TimeWindow { start_hour: 12, end_hour: 14 }]);
        let result = evaluator.evaluate_flag(&time_window_flag_fail, &context).await.unwrap();
        assert!(!result);
    }

    // Cache Tests

    #[tokio::test]
    async fn test_cache_basic_operations() {
        let cache = FeatureFlagCache::new(60); // 60 second TTL
        let context = create_test_context();

        // Test cache miss
        assert!(cache.get("test_flag", &context).await.is_none());

        // Test cache put and hit
        cache.put("test_flag".to_string(), context.clone(), true).await;
        assert_eq!(cache.get("test_flag", &context).await, Some(true));

        // Test different context (should be separate cache entry)
        let different_context = EvaluationContext::new("different-node".to_string(), Environment::Development);
        assert!(cache.get("test_flag", &different_context).await.is_none());

        // Test cache stats
        let stats = cache.get_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.insertions, 1);
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = FeatureFlagCache::new(1); // 1 second TTL
        let context = create_test_context();

        // Insert and verify
        cache.put("test_flag".to_string(), context.clone(), true).await;
        assert_eq!(cache.get("test_flag", &context).await, Some(true));

        // Wait for expiration and verify
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(cache.get("test_flag", &context).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = FeatureFlagCache::new(60);
        let context = create_test_context();

        // Insert entry
        cache.put("test_flag".to_string(), context.clone(), true).await;
        assert_eq!(cache.get("test_flag", &context).await, Some(true));

        // Invalidate and verify
        cache.invalidate_flag("test_flag").await;
        assert!(cache.get("test_flag", &context).await.is_none());
    }

    // Configuration Tests

    #[test]
    fn test_config_loader_toml_parsing() {
        let toml_content = r#"
version = "1.0"
default_environment = "development"

[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true
enable_metrics = true
max_evaluation_time_ms = 1

[flags.test_enabled]
enabled = true
rollout_percentage = 75
description = "Test enabled flag"
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"

[flags.test_enabled.metadata]
owner = "test-team"
risk = "low"

[flags.test_enabled.targets]
node_ids = ["node-1", "node-2"]
environments = ["development"]

[flags.test_disabled]
enabled = false
description = "Test disabled flag"
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"
        "#;

        let loader = FeatureFlagConfigLoader::new();
        let config = loader.parse_toml_content(toml_content).unwrap();

        assert_eq!(config.version, "1.0");
        assert_eq!(config.default_environment, Environment::Development);
        assert_eq!(config.flags.len(), 2);

        // Test enabled flag
        let enabled_flag = config.get_flag("test_enabled").unwrap();
        assert!(enabled_flag.enabled);
        assert_eq!(enabled_flag.rollout_percentage, Some(75));
        assert_eq!(enabled_flag.description, Some("Test enabled flag".to_string()));
        assert_eq!(enabled_flag.metadata.get("owner"), Some(&"test-team".to_string()));

        // Test targeting
        let targets = enabled_flag.targets.as_ref().unwrap();
        assert_eq!(targets.node_ids.as_ref().unwrap().len(), 2);
        assert_eq!(targets.environments.as_ref().unwrap().len(), 1);

        // Test disabled flag
        let disabled_flag = config.get_flag("test_disabled").unwrap();
        assert!(!disabled_flag.enabled);
    }

    #[test]
    fn test_config_validation() {
        let mut collection = FeatureFlagCollection::new();
        
        // Add valid flag
        collection.add_flag(FeatureFlag::enabled("valid_flag".to_string()));
        assert!(collection.validate().is_ok());
        
        // Add invalid flag
        let mut invalid_flag = FeatureFlag::new("".to_string(), true); // Empty name
        invalid_flag.rollout_percentage = Some(150); // Invalid percentage
        collection.add_flag(invalid_flag);
        
        assert!(collection.validate().is_err());
    }

    // Manager Integration Tests

    #[tokio::test]
    async fn test_manager_basic_functionality() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();

        // Test enabled flag
        assert!(manager.is_enabled("test_enabled", &context).await);
        
        // Test disabled flag
        assert!(!manager.is_enabled("test_disabled", &context).await);
        
        // Test non-existent flag (should default to false)
        assert!(!manager.is_enabled("non_existent", &context).await);
    }

    #[tokio::test]
    async fn test_manager_cache_behavior() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();

        // First evaluation (cache miss)
        let _result1 = manager.is_enabled("test_enabled", &context).await;
        
        // Second evaluation (cache hit)
        let _result2 = manager.is_enabled("test_enabled", &context).await;

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_evaluations, 2);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert!(stats.cache_hit_rate() > 0.0);
    }

    #[tokio::test]
    async fn test_manager_flag_management() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();

        // Add new flag
        let new_flag = FeatureFlag::enabled("dynamic_flag".to_string());
        manager.upsert_flag(new_flag).await.unwrap();
        
        // Verify it's enabled
        assert!(manager.is_enabled("dynamic_flag", &context).await);
        
        // Remove flag
        let removed = manager.remove_flag("dynamic_flag").await.unwrap();
        assert!(removed.is_some());
        
        // Verify it's no longer enabled
        assert!(!manager.is_enabled("dynamic_flag", &context).await);
    }

    #[tokio::test]
    async fn test_detailed_evaluation() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();

        let result = manager.evaluate_detailed("test_enabled", &context).await.unwrap();
        assert!(result.enabled);
        assert!(matches!(result.reason, EvaluationReason::Enabled));
        assert_eq!(result.flag_name, "test_enabled");
        assert!(result.evaluation_time_us > 0);
    }

    #[tokio::test]
    async fn test_config_reload() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();

        // Initial state
        assert!(manager.is_enabled("test_enabled", &context).await);

        // Modify config file (flip the enabled flag)
        let modified_config = r#"
version = "1.0"
default_environment = "development"

[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true
enable_metrics = true
max_evaluation_time_ms = 1

[flags.test_enabled]
enabled = false
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"

[flags.test_disabled]
enabled = false
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"
        "#;

        std::fs::write(temp_file.path(), modified_config).unwrap();
        manager.reload_config().await.unwrap();

        // Should now be disabled
        assert!(!manager.is_enabled("test_enabled", &context).await);
    }

    // Helper Functions

    fn create_test_config_file() -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
version = "1.0"
default_environment = "development"

[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true
enable_metrics = true
max_evaluation_time_ms = 1

[flags.test_enabled]
enabled = true
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"

[flags.test_disabled]
enabled = false
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"
        "#;
        
        write!(temp_file, "{}", config_content).unwrap();
        temp_file
    }

    // Performance Tests

    #[tokio::test]
    async fn test_evaluation_performance() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();

        // Warm up cache
        let _ = manager.is_enabled("test_enabled", &context).await;

        // Measure cached evaluation performance
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = manager.is_enabled("test_enabled", &context).await;
        }
        let elapsed = start.elapsed();

        // Should be very fast with caching (< 1ms per evaluation)
        let avg_time_us = elapsed.as_micros() / 1000;
        println!("Average cached evaluation time: {}μs", avg_time_us);
        assert!(avg_time_us < 1000, "Cached evaluation too slow: {}μs", avg_time_us);
    }

    #[tokio::test]
    async fn test_percentage_consistency() {
        let flag = FeatureFlag::with_percentage("consistency_test".to_string(), true, 50);
        let evaluator = FeatureFlagEvaluator::new();
        
        // Same context should always give same result
        let context = create_test_context();
        let result1 = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        let result2 = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        let result3 = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        
        assert_eq!(result1, result2);
        assert_eq!(result2, result3);
        
        println!("Consistent result for context: {}", result1);
    }

    // Phase 3: Performance & Caching Tests

    #[tokio::test]
    async fn test_macro_cache_functionality() {
        use super::super::performance::macro_cache;
        
        // Clear cache to start fresh
        macro_cache::clear_cache().await;
        
        let context = create_test_context();
        
        // Test cache miss
        let result = macro_cache::fast_cache_lookup("test_macro_flag", &context).await;
        assert!(result.is_none());
        
        // Test cache store and hit
        macro_cache::fast_cache_store("test_macro_flag", &context, true, 100).await;
        let result = macro_cache::fast_cache_lookup("test_macro_flag", &context).await;
        assert_eq!(result, Some(true));
        
        // Test cache statistics
        let stats = macro_cache::get_cache_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.total_accesses, 2);
        assert!(stats.hit_rate() > 0.0);
    }
    
    #[tokio::test]
    async fn test_macro_cache_expiration() {
        use super::super::performance::macro_cache;
        
        macro_cache::clear_cache().await;
        
        let context = create_test_context();
        
        // Store in cache
        macro_cache::fast_cache_store("test_expiry", &context, true, 50).await;
        assert_eq!(macro_cache::fast_cache_lookup("test_expiry", &context).await, Some(true));
        
        // Wait for expiration (5 seconds + buffer)
        tokio::time::sleep(Duration::from_secs(6)).await;
        
        // Should be expired
        assert!(macro_cache::fast_cache_lookup("test_expiry", &context).await.is_none());
    }
    
    #[tokio::test]
    async fn test_macro_cache_context_sensitivity() {
        use super::super::performance::macro_cache;
        
        macro_cache::clear_cache().await;
        
        let context1 = create_test_context();
        let context2 = EvaluationContext::new("different-node".to_string(), Environment::Development);
        
        // Store for context1
        macro_cache::fast_cache_store("test_context", &context1, true, 100).await;
        
        // Should hit for same context
        assert_eq!(macro_cache::fast_cache_lookup("test_context", &context1).await, Some(true));
        
        // Should miss for different context
        assert!(macro_cache::fast_cache_lookup("test_context", &context2).await.is_none());
        
        // Store for context2
        macro_cache::fast_cache_store("test_context", &context2, false, 100).await;
        
        // Both contexts should have their own cached values
        assert_eq!(macro_cache::fast_cache_lookup("test_context", &context1).await, Some(true));
        assert_eq!(macro_cache::fast_cache_lookup("test_context", &context2).await, Some(false));
    }
    
    #[tokio::test]
    async fn test_consistent_hashing_rollout() {
        use super::super::performance::consistent_hashing;
        
        let context = create_test_context();
        
        // Test edge cases
        assert!(!consistent_hashing::evaluate_consistent_percentage(0, &context, "test_flag"));
        assert!(consistent_hashing::evaluate_consistent_percentage(100, &context, "test_flag"));
        
        // Test consistency - same inputs should always give same result
        let result1 = consistent_hashing::evaluate_consistent_percentage(50, &context, "test_flag");
        let result2 = consistent_hashing::evaluate_consistent_percentage(50, &context, "test_flag");
        let result3 = consistent_hashing::evaluate_consistent_percentage(50, &context, "test_flag");
        
        assert_eq!(result1, result2);
        assert_eq!(result2, result3);
        
        // Different flag names should potentially give different results
        let result_diff_flag = consistent_hashing::evaluate_consistent_percentage(50, &context, "different_flag");
        // Note: This might be the same by chance, but we're testing the deterministic nature
        
        println!("Consistent results: {} (same flag), {} (different flag)", result1, result_diff_flag);
    }
    
    #[tokio::test]
    async fn test_rollout_distribution_validation() {
        use super::super::performance::consistent_hashing;
        
        // Test 25% rollout with 1000 samples
        let samples: Vec<(String, Environment)> = (0..1000)
            .map(|i| (format!("test-node-{}", i), Environment::Development))
            .collect();
            
        let stats = consistent_hashing::verify_rollout_distribution(25, &samples, "test_flag");
        
        assert_eq!(stats.target_percentage, 25);
        assert_eq!(stats.sample_size, 1000);
        assert!(stats.is_within_tolerance, "Distribution deviation too high: {:.1}%", stats.deviation);
        assert!(stats.actual_percentage > 20.0 && stats.actual_percentage < 30.0, 
               "Actual percentage {} outside reasonable range", stats.actual_percentage);
        
        println!("Rollout distribution: target={}%, actual={:.1}%, deviation={:.1}%", 
                stats.target_percentage, stats.actual_percentage, stats.deviation);
    }
    
    #[tokio::test]
    async fn test_performance_benchmarking() {
        use super::super::performance::benchmarks;
        
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        
        // Run a small benchmark
        let results = manager.run_performance_benchmark(100).await;
        
        assert_eq!(results.total_evaluations, 100);
        assert!(results.avg_evaluation_time_us > 0);
        assert!(results.max_evaluation_time_us >= results.avg_evaluation_time_us);
        assert!(results.p95_evaluation_time_us >= results.avg_evaluation_time_us);
        assert!(results.p99_evaluation_time_us >= results.p95_evaluation_time_us);
        
        // Performance targets
        assert!(results.avg_evaluation_time_us < 10000, 
               "Average evaluation time too high: {}μs", results.avg_evaluation_time_us);
        
        println!("Benchmark results: avg={}μs, p95={}μs, target_met={}", 
                results.avg_evaluation_time_us, results.p95_evaluation_time_us, results.target_met);
    }
    
    #[tokio::test]
    async fn test_enhanced_feature_enabled_macro() {
        let temp_file = create_test_config_file();
        
        // Initialize global feature flags for macro testing
        crate::features::init_feature_flags(temp_file.path().to_str().unwrap()).unwrap();
        
        // Initialize context provider (normally done in app startup)
        let context = create_test_context();
        crate::features::set_evaluation_context_provider(Box::new(move || {
            let ctx = context.clone();
            Box::pin(async move { Ok(ctx) })
        })).unwrap();
        
        // Test macro with automatic context
        let result1 = feature_enabled!("test_enabled").await;
        assert!(result1);
        
        // Test with cache hit (second call should be much faster)
        let start = std::time::Instant::now();
        let result2 = feature_enabled!("test_enabled").await;
        let elapsed = start.elapsed();
        
        assert!(result2);
        assert_eq!(result1, result2);
        
        // Should be very fast due to macro caching
        assert!(elapsed.as_micros() < 1000, "Macro cache too slow: {}μs", elapsed.as_micros());
        
        println!("Macro cache lookup time: {}μs", elapsed.as_micros());
    }
    
    #[tokio::test] 
    async fn test_macro_cache_health_monitoring() {
        use super::super::performance::macro_cache;
        
        macro_cache::clear_cache().await;
        
        // Perform some operations to generate statistics
        let context = create_test_context();
        for i in 0..10 {
            let flag_name = format!("test_flag_{}", i % 3); // Create some hits and misses
            
            // Cache miss
            let _ = macro_cache::fast_cache_lookup(&flag_name, &context).await;
            
            // Cache store
            macro_cache::fast_cache_store(&flag_name, &context, i % 2 == 0, 100).await;
            
            // Cache hit
            let _ = macro_cache::fast_cache_lookup(&flag_name, &context).await;
        }
        
        // Check health status
        let health = macro_cache::health_check().await;
        println!("Macro cache health: {:?}", health);
        
        // Get detailed statistics
        let stats = macro_cache::get_cache_stats().await;
        println!("Cache stats: hits={}, misses={}, hit_rate={:.1}%", 
                stats.hits, stats.misses, stats.hits as f64 / stats.total_accesses as f64 * 100.0);
        
        assert!(stats.total_accesses > 0);
        assert!(stats.hits > 0);
        assert!(stats.misses > 0);
    }
    
    #[tokio::test]
    async fn test_performance_maintenance_task() {
        use super::super::performance;
        
        // Start background maintenance (short interval for testing)
        let maintenance = performance::PerformanceMaintenance::new(1, 1000);
        let task_handle = maintenance.start();
        
        // Let it run for a short time
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Stop the task
        task_handle.abort();
        
        // Task should have run at least once
        let stats = performance::macro_cache::get_cache_stats().await;
        println!("Maintenance task stats: cleanups={}", stats.cleanups);
    }
    
    #[tokio::test]
    async fn test_manager_performance_report() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();
        
        // Perform some evaluations to generate statistics
        for _ in 0..10 {
            let _ = manager.is_enabled("test_enabled", &context).await;
        }
        
        // Generate performance report
        let report = manager.get_performance_report().await;
        
        assert!(report.contains("Feature Flag System Performance Report"));
        assert!(report.contains("Manager Statistics"));
        assert!(report.contains("Macro Cache"));
        assert!(report.contains("Performance Target Status"));
        
        println!("Performance Report:\n{}", report);
    }
    
    #[tokio::test]
    async fn test_rollout_distribution_validation_manager() {
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        
        // Add a flag with 30% rollout
        let flag = FeatureFlag::with_percentage("rollout_test".to_string(), true, 30);
        manager.upsert_flag(flag).await.unwrap();
        
        // Validate rollout distribution
        let stats = manager.validate_rollout_distribution("rollout_test", 30, 1000).await.unwrap();
        
        assert_eq!(stats.target_percentage, 30);
        assert_eq!(stats.sample_size, 1000);
        assert!(stats.is_within_tolerance, 
               "Rollout distribution validation failed: deviation={:.1}%", stats.deviation);
        
        println!("Manager rollout validation: target={}%, actual={:.1}%, tolerance_met={}", 
                stats.target_percentage, stats.actual_percentage, stats.is_within_tolerance);
    }
    
    #[tokio::test]
    async fn test_macro_cache_memory_protection() {
        use super::super::performance::macro_cache;
        
        macro_cache::clear_cache().await;
        
        let context = create_test_context();
        
        // Fill cache beyond normal limits (testing memory protection)
        for i in 0..50 {
            let flag_name = format!("memory_test_flag_{}", i);
            macro_cache::fast_cache_store(&flag_name, &context, i % 2 == 0, 100).await;
        }
        
        // Check cache size and statistics
        let stats = macro_cache::get_cache_stats().await;
        println!("Memory protection test: cache_size={}, max_size={}", 
                stats.current_cache_size, stats.max_cache_size);
        
        // Cache should be managing memory appropriately
        assert!(stats.current_cache_size > 0);
        assert!(stats.max_cache_size >= stats.current_cache_size);
        
        // Trigger cleanup
        let cleaned = macro_cache::cleanup_expired().await;
        println!("Cleanup removed {} expired entries", cleaned);
    }
    
    // Integration test for complete Phase 3 workflow
    #[tokio::test]
    async fn test_phase3_integration_workflow() {
        // Initialize system
        let temp_file = create_test_config_file();
        let manager = FeatureFlagManager::new(temp_file.path().to_path_buf()).unwrap();
        let context = create_test_context();
        
        // Clear macro cache for clean test
        crate::features::performance::macro_cache::clear_cache().await;
        
        // Test Phase 3 features integration:
        
        // 1. Enhanced macro with 5-second caching (ALYS-004-08)
        macro_cache::fast_cache_store("integration_test", &context, true, 150).await;
        let cached_result = macro_cache::fast_cache_lookup("integration_test", &context).await;
        assert_eq!(cached_result, Some(true));
        
        // 2. Consistent hashing for rollouts (ALYS-004-09)
        let rollout_result1 = crate::features::performance::consistent_hashing::evaluate_consistent_percentage(
            75, &context, "integration_rollout"
        );
        let rollout_result2 = crate::features::performance::consistent_hashing::evaluate_consistent_percentage(
            75, &context, "integration_rollout"
        );
        assert_eq!(rollout_result1, rollout_result2); // Consistency guarantee
        
        // 3. Performance benchmarking (ALYS-004-10)
        let benchmark = manager.run_performance_benchmark(50).await;
        assert!(benchmark.total_evaluations > 0);
        assert!(benchmark.avg_evaluation_time_us < 5000); // Should be well under 5ms
        
        // 4. Comprehensive performance report
        let report = manager.get_performance_report().await;
        assert!(report.contains("Feature Flag System Performance Report"));
        
        // 5. Rollout distribution validation
        let distribution = manager.validate_rollout_distribution("integration_rollout", 75, 500).await.unwrap();
        assert!(distribution.is_within_tolerance);
        
        println!("Phase 3 integration test completed successfully");
        println!("Benchmark average: {}μs", benchmark.avg_evaluation_time_us);
        println!("Distribution accuracy: {:.1}% (target: {}%)", 
                distribution.actual_percentage, distribution.target_percentage);
    }
}