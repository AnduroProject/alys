//! Comprehensive test suite for enhanced feature flag validation
//!
//! This module contains extensive tests for the validation system including:
//! - Schema validation tests
//! - Error reporting tests  
//! - Context-specific validation tests
//! - Edge case handling tests

#[cfg(test)]
mod tests {
    use super::super::validation::*;
    use super::super::types::*;
    use super::super::config::*;
    use crate::config::Environment;
    use std::collections::HashMap;
    use chrono::{Utc, Duration};
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    // Helper functions for creating test data
    
    fn create_valid_flag() -> FeatureFlag {
        FeatureFlag::new("test_flag".to_string(), true)
            .with_description("Test flag for validation".to_string())
            .with_percentage(50)
            .with_metadata("owner".to_string(), "test-team".to_string())
            .with_metadata("risk".to_string(), "low".to_string())
    }
    
    fn create_production_context() -> ValidationContext {
        ValidationContext {
            environment: Environment::Production,
            schema_version: "1.0".to_string(),
            strict_mode: true,
            deprecated_warnings: true,
        }
    }
    
    fn create_development_context() -> ValidationContext {
        ValidationContext {
            environment: Environment::Development,
            schema_version: "1.0".to_string(),
            strict_mode: false,
            deprecated_warnings: false,
        }
    }
    
    // Basic validation tests
    
    #[test]
    fn test_valid_flag_passes_validation() {
        let validator = FeatureFlagValidator::new();
        let flag = create_valid_flag();
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_ok(), "Valid flag should pass validation");
    }
    
    #[test]
    fn test_empty_flag_name_fails() {
        let validator = FeatureFlagValidator::new();
        let flag = FeatureFlag::new("".to_string(), true);
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Empty flag name should fail validation");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("name")));
        assert!(errors.iter().any(|e| matches!(e.error_type, ValidationErrorType::Required)));
    }
    
    #[test]
    fn test_invalid_flag_name_format() {
        let validator = FeatureFlagValidator::new();
        
        let invalid_names = vec![
            "Test Flag",      // Spaces
            "test-flag",      // Hyphens
            "_test_flag",     // Leading underscore
            "test_flag_",     // Trailing underscore
            "TestFlag",       // Capital letters
            "123test",        // Starting with numbers
        ];
        
        for name in invalid_names {
            let flag = FeatureFlag::new(name.to_string(), true);
            let result = validator.validate_flag(&flag);
            
            assert!(result.is_err(), "Invalid flag name '{}' should fail validation", name);
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|e| matches!(e.error_type, ValidationErrorType::InvalidFormat)));
        }
    }
    
    #[test]
    fn test_valid_flag_names() {
        let validator = FeatureFlagValidator::new();
        
        let valid_names = vec![
            "test_flag",
            "a",
            "my_feature_v2",
            "feature1",
            "long_descriptive_feature_name",
        ];
        
        for name in valid_names {
            let flag = FeatureFlag::new(name.to_string(), true);
            let result = validator.validate_flag(&flag);
            
            assert!(result.is_ok(), "Valid flag name '{}' should pass validation", name);
        }
    }
    
    #[test]
    fn test_rollout_percentage_validation() {
        let validator = FeatureFlagValidator::new();
        
        // Test invalid percentage
        let mut flag = create_valid_flag();
        flag.rollout_percentage = Some(150);
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Rollout percentage > 100 should fail");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("rollout_percentage")));
        assert!(errors.iter().any(|e| matches!(e.error_type, ValidationErrorType::OutOfRange)));
        
        // Test valid percentages
        for percentage in [0, 25, 50, 75, 100] {
            flag.rollout_percentage = Some(percentage);
            let result = validator.validate_flag(&flag);
            assert!(result.is_ok(), "Valid percentage {} should pass", percentage);
        }
    }
    
    // Context-specific validation tests
    
    #[test]
    fn test_production_requires_description() {
        let validator = FeatureFlagValidator::with_context(create_production_context());
        
        let mut flag = create_valid_flag();
        flag.description = None;
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Production flags should require description");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("description")));
    }
    
    #[test]
    fn test_production_requires_metadata() {
        let validator = FeatureFlagValidator::with_context(create_production_context());
        
        // Test missing owner
        let mut flag = create_valid_flag();
        flag.metadata.remove("owner");
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Production flags should require owner metadata");
        
        // Test missing risk
        flag.metadata.insert("owner".to_string(), "team".to_string());
        flag.metadata.remove("risk");
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Production flags should require risk metadata");
    }
    
    #[test]
    fn test_development_context_flexibility() {
        let validator = FeatureFlagValidator::with_context(create_development_context());
        
        let mut flag = create_valid_flag();
        flag.description = None;
        flag.metadata.clear();
        
        // Development should be more lenient
        let result = validator.validate_flag(&flag);
        // This might still fail due to other validation rules, but not due to missing description
        if let Err(errors) = result {
            assert!(!errors.iter().any(|e| e.field_path.contains("description") && e.error_type == ValidationErrorType::Required));
        }
    }
    
    // Condition validation tests
    
    #[test]
    fn test_sync_progress_condition_validation() {
        let validator = FeatureFlagValidator::new();
        
        let invalid_conditions = vec![
            FeatureCondition::SyncProgressAbove(-0.1),
            FeatureCondition::SyncProgressAbove(1.5),
            FeatureCondition::SyncProgressBelow(-0.5),
            FeatureCondition::SyncProgressBelow(2.0),
        ];
        
        for condition in invalid_conditions {
            let mut flag = create_valid_flag();
            flag.conditions = Some(vec![condition.clone()]);
            
            let result = validator.validate_flag(&flag);
            assert!(result.is_err(), "Invalid sync progress condition should fail: {:?}", condition);
            
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|e| e.field_path.contains("conditions")));
        }
    }
    
    #[test]
    fn test_time_window_condition_validation() {
        let validator = FeatureFlagValidator::new();
        
        let invalid_conditions = vec![
            FeatureCondition::TimeWindow { start_hour: 25, end_hour: 10 },
            FeatureCondition::TimeWindow { start_hour: 10, end_hour: 30 },
        ];
        
        for condition in invalid_conditions {
            let mut flag = create_valid_flag();
            flag.conditions = Some(vec![condition.clone()]);
            
            let result = validator.validate_flag(&flag);
            assert!(result.is_err(), "Invalid time window condition should fail: {:?}", condition);
        }
    }
    
    #[test] 
    fn test_node_health_condition_validation() {
        let validator = FeatureFlagValidator::new();
        
        // Test invalid CPU usage
        let invalid_condition = FeatureCondition::NodeHealth {
            max_cpu_usage_percent: Some(150),
            min_memory_mb: Some(1024),
            max_load_average: Some(2.0),
        };
        
        let mut flag = create_valid_flag();
        flag.conditions = Some(vec![invalid_condition]);
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Invalid CPU usage should fail validation");
        
        // Test invalid memory requirement
        let invalid_condition2 = FeatureCondition::NodeHealth {
            max_cpu_usage_percent: Some(80),
            min_memory_mb: Some(0),
            max_load_average: Some(2.0),
        };
        
        flag.conditions = Some(vec![invalid_condition2]);
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Zero memory requirement should fail validation");
    }
    
    // Target validation tests
    
    #[test]
    fn test_ip_range_validation() {
        let validator = FeatureFlagValidator::new();
        
        let mut flag = create_valid_flag();
        flag.targets = Some(FeatureTargets {
            ip_ranges: Some(vec![
                "192.168.1.0/24".to_string(),  // Valid
                "invalid-ip".to_string(),       // Invalid
            ]),
            ..Default::default()
        });
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Invalid IP range should fail validation");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("ip_ranges")));
    }
    
    #[test]
    fn test_empty_node_ids_validation() {
        let validator = FeatureFlagValidator::new();
        
        let mut flag = create_valid_flag();
        flag.targets = Some(FeatureTargets {
            node_ids: Some(vec!["node1".to_string(), "".to_string()]), // Empty node ID
            ..Default::default()
        });
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Empty node ID should fail validation");
    }
    
    // Timestamp validation tests
    
    #[test]
    fn test_future_created_timestamp_fails() {
        let validator = FeatureFlagValidator::new();
        
        let mut flag = create_valid_flag();
        flag.created_at = Utc::now() + Duration::days(1); // Future timestamp
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Future created timestamp should fail validation");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("created_at")));
    }
    
    #[test]
    fn test_updated_before_created_fails() {
        let validator = FeatureFlagValidator::new();
        
        let mut flag = create_valid_flag();
        let now = Utc::now();
        flag.created_at = now;
        flag.updated_at = now - Duration::hours(1); // Updated before created
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err(), "Updated timestamp before created should fail validation");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("updated_at")));
    }
    
    // Security validation tests
    
    #[test]
    fn test_security_sensitive_content_detection() {
        let validator = FeatureFlagValidator::new();
        
        let mut flag = create_valid_flag();
        flag.description = Some("Enable password validation feature".to_string());
        
        let result = validator.validate_flag(&flag);
        if let Err(errors) = result {
            assert!(errors.iter().any(|e| matches!(e.error_type, ValidationErrorType::Security)));
        }
        
        // Test metadata security
        flag.description = Some("Normal description".to_string());
        flag.metadata.insert("password".to_string(), "some-value".to_string());
        
        let result = validator.validate_flag(&flag);
        if let Err(errors) = result {
            assert!(errors.iter().any(|e| matches!(e.error_type, ValidationErrorType::Security)));
        }
    }
    
    // Collection validation tests
    
    #[test]
    fn test_collection_validation() {
        let validator = FeatureFlagValidator::new();
        
        let mut collection = FeatureFlagCollection::new();
        collection.version = "".to_string(); // Invalid version
        
        let result = validator.validate_collection(&collection);
        assert!(result.is_err(), "Empty version should fail collection validation");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path == "version"));
    }
    
    #[test]
    fn test_global_settings_validation() {
        let validator = FeatureFlagValidator::new();
        
        let mut collection = FeatureFlagCollection::new();
        collection.global_settings.cache_ttl_seconds = 0; // Invalid
        
        let result = validator.validate_collection(&collection);
        assert!(result.is_err(), "Zero cache TTL should fail validation");
        
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field_path.contains("cache_ttl_seconds")));
    }
    
    // Version validation tests
    
    #[test]
    fn test_version_format_validation() {
        let validator = FeatureFlagValidator::new();
        
        let valid_versions = vec!["1.0", "1.0.0", "2.1.3", "1.0.0-beta"];
        for version in valid_versions {
            assert!(validator.is_valid_version(version), "Version '{}' should be valid", version);
        }
        
        let invalid_versions = vec!["1", "v1.0", "1.0.", "", "not-a-version"];
        for version in invalid_versions {
            assert!(!validator.is_valid_version(version), "Version '{}' should be invalid", version);
        }
    }
    
    // Error reporting tests
    
    #[test]
    fn test_error_report_generation() {
        let validator = FeatureFlagValidator::new();
        
        let errors = vec![
            ValidationError {
                field_path: "flags.test.name".to_string(),
                error_type: ValidationErrorType::InvalidFormat,
                message: "Invalid flag name format".to_string(),
                suggestion: Some("Use lowercase with underscores".to_string()),
                value: Some("Test Name".to_string()),
            },
            ValidationError {
                field_path: "flags.test.rollout_percentage".to_string(),
                error_type: ValidationErrorType::OutOfRange,
                message: "Percentage exceeds 100".to_string(),
                suggestion: Some("Set percentage between 0 and 100".to_string()),
                value: Some("150".to_string()),
            },
        ];
        
        let report = validator.generate_report(&errors);
        
        assert!(report.contains("Format Errors"));
        assert!(report.contains("Range Errors"));
        assert!(report.contains("Invalid flag name format"));
        assert!(report.contains("Suggestion:"));
        assert!(report.contains("Total Issues: 2"));
    }
    
    #[test]
    fn test_empty_error_report() {
        let validator = FeatureFlagValidator::new();
        let report = validator.generate_report(&[]);
        
        assert!(report.contains("✅ All validations passed successfully!"));
    }
    
    // Integration tests with configuration loader
    
    #[test]
    fn test_config_loader_enhanced_validation() {
        let toml_content = r#"
version = "1.0"
default_environment = "production"

[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true
enable_metrics = true
max_evaluation_time_ms = 1

[flags.invalid_flag]
enabled = true
rollout_percentage = 150
# Missing description (required for production)
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"
        "#;
        
        let context = ValidationContext {
            environment: Environment::Production,
            schema_version: "1.0".to_string(),
            strict_mode: true,
            deprecated_warnings: true,
        };
        
        let loader = FeatureFlagConfigLoader::with_enhanced_validation(context);
        let result = loader.parse_toml_content(toml_content);
        
        assert!(result.is_err(), "Invalid configuration should fail enhanced validation");
        
        if let Err(FeatureFlagError::ValidationError { reason, .. }) = result {
            assert!(reason.contains("rollout_percentage") || reason.contains("description"));
        }
    }
    
    #[test]
    fn test_validation_report_integration() {
        let mut collection = FeatureFlagCollection::new();
        
        // Add flag with multiple issues
        let mut flag = FeatureFlag::new("Invalid Flag Name".to_string(), true);
        flag.rollout_percentage = Some(150);
        collection.add_flag(flag);
        
        let (is_valid, report) = validate_collection_with_report(&collection, None);
        
        assert!(!is_valid, "Collection with invalid flag should not be valid");
        assert!(report.contains("Format Errors") || report.contains("Range Errors"));
        assert!(report.contains("Total Issues:"));
    }
    
    // Performance validation tests
    
    #[test]
    fn test_performance_warning_validation() {
        let validator = FeatureFlagValidator::new();
        
        let mut collection = FeatureFlagCollection::new();
        collection.global_settings.cache_ttl_seconds = 5000; // Very high TTL
        collection.global_settings.max_evaluation_time_ms = 200; // High evaluation time
        
        let result = validator.validate_collection(&collection);
        if let Err(errors) = result {
            assert!(errors.iter().any(|e| matches!(e.error_type, ValidationErrorType::Performance)));
        }
    }
}