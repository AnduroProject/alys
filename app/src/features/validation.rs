//! Enhanced configuration validation for feature flags
//!
//! This module provides comprehensive schema validation, error reporting,
//! and configuration integrity checking for the feature flag system.

use super::types::*;
use super::{FeatureFlagResult, FeatureFlagError};
use crate::config::{ConfigError, Environment};

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use chrono::{DateTime, Utc, Duration};
use regex::Regex;
use tracing::{warn, debug};
use once_cell::sync::Lazy;

/// Enhanced validation error with detailed context
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field_path: String,
    pub error_type: ValidationErrorType,
    pub message: String,
    pub suggestion: Option<String>,
    pub value: Option<String>,
}

/// Types of validation errors
#[derive(Debug, Clone)]
pub enum ValidationErrorType {
    Required,
    InvalidFormat,
    InvalidValue,
    OutOfRange,
    Inconsistent,
    Deprecated,
    Security,
    Performance,
}

/// Validation result with multiple errors
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validation context for environment-specific rules
#[derive(Debug, Clone)]
pub struct ValidationContext {
    pub environment: Environment,
    pub schema_version: String,
    pub strict_mode: bool,
    pub deprecated_warnings: bool,
}

impl Default for ValidationContext {
    fn default() -> Self {
        Self {
            environment: Environment::Development,
            schema_version: "1.0".to_string(),
            strict_mode: true,
            deprecated_warnings: true,
        }
    }
}

/// Enhanced configuration validator
pub struct FeatureFlagValidator {
    context: ValidationContext,
}

impl FeatureFlagValidator {
    /// Create a new validator with default context
    pub fn new() -> Self {
        Self {
            context: ValidationContext::default(),
        }
    }
    
    /// Create validator with specific context
    pub fn with_context(context: ValidationContext) -> Self {
        Self { context }
    }
    
    /// Validate complete feature flag collection
    pub fn validate_collection(&self, collection: &FeatureFlagCollection) -> ValidationResult {
        let mut errors = Vec::new();
        
        // Validate collection structure
        self.validate_collection_structure(collection, &mut errors);
        
        // Validate global settings
        self.validate_global_settings(&collection.global_settings, &mut errors);
        
        // Validate each flag
        for (name, flag) in &collection.flags {
            self.validate_flag_with_context(name, flag, collection, &mut errors);
        }
        
        // Cross-flag validation
        self.validate_flag_consistency(&collection.flags, &mut errors);
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Validate individual feature flag
    pub fn validate_flag(&self, flag: &FeatureFlag) -> ValidationResult {
        let mut errors = Vec::new();
        self.validate_flag_structure(flag, &mut errors);
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Generate validation report
    pub fn generate_report(&self, errors: &[ValidationError]) -> String {
        let mut report = String::new();
        
        report.push_str("Feature Flag Configuration Validation Report\n");
        report.push_str("==============================================\n\n");
        
        if errors.is_empty() {
            report.push_str("✅ All validations passed successfully!\n");
            return report;
        }
        
        // Group errors by type
        let mut error_groups: HashMap<ValidationErrorType, Vec<&ValidationError>> = HashMap::new();
        for error in errors {
            error_groups.entry(error.error_type.clone()).or_default().push(error);
        }
        
        let mut total_errors = 0;
        for (error_type, group_errors) in error_groups {
            report.push_str(&format!("{} ({} issues):\n", 
                self.format_error_type(&error_type), group_errors.len()));
            total_errors += group_errors.len();
            
            for error in group_errors {
                report.push_str(&format!("  ❌ {}: {}\n", error.field_path, error.message));
                if let Some(suggestion) = &error.suggestion {
                    report.push_str(&format!("     💡 Suggestion: {}\n", suggestion));
                }
            }
            report.push('\n');
        }
        
        report.push_str(&format!("Total Issues: {}\n", total_errors));
        report
    }
    
    // Private validation methods
    
    fn validate_collection_structure(&self, collection: &FeatureFlagCollection, errors: &mut Vec<ValidationError>) {
        // Version validation
        if collection.version.is_empty() {
            errors.push(ValidationError {
                field_path: "version".to_string(),
                error_type: ValidationErrorType::Required,
                message: "Configuration version is required".to_string(),
                suggestion: Some("Add version = \"1.0\" to your configuration".to_string()),
                value: None,
            });
        } else if !self.is_valid_version(&collection.version) {
            errors.push(ValidationError {
                field_path: "version".to_string(),
                error_type: ValidationErrorType::InvalidFormat,
                message: format!("Invalid version format: {}", collection.version),
                suggestion: Some("Use semantic versioning format (e.g., \"1.0.0\")".to_string()),
                value: Some(collection.version.clone()),
            });
        }
        
        // Environment validation
        if matches!(self.context.environment, Environment::Production) && collection.flags.is_empty() {
            errors.push(ValidationError {
                field_path: "flags".to_string(),
                error_type: ValidationErrorType::InvalidValue,
                message: "Production configuration cannot have zero flags".to_string(),
                suggestion: Some("Add at least one feature flag or use a development configuration".to_string()),
                value: None,
            });
        }
    }
    
    fn validate_global_settings(&self, settings: &FeatureFlagGlobalSettings, errors: &mut Vec<ValidationError>) {
        // Cache TTL validation
        if settings.cache_ttl_seconds == 0 {
            errors.push(ValidationError {
                field_path: "global_settings.cache_ttl_seconds".to_string(),
                error_type: ValidationErrorType::InvalidValue,
                message: "Cache TTL must be greater than 0".to_string(),
                suggestion: Some("Set cache_ttl_seconds to at least 1 (recommended: 5-300)".to_string()),
                value: Some("0".to_string()),
            });
        } else if settings.cache_ttl_seconds > 3600 {
            errors.push(ValidationError {
                field_path: "global_settings.cache_ttl_seconds".to_string(),
                error_type: ValidationErrorType::Performance,
                message: "Cache TTL exceeds recommended maximum (1 hour)".to_string(),
                suggestion: Some("Consider reducing cache_ttl_seconds to improve responsiveness".to_string()),
                value: Some(settings.cache_ttl_seconds.to_string()),
            });
        }
        
        // Evaluation timeout validation
        if settings.max_evaluation_time_ms == 0 {
            errors.push(ValidationError {
                field_path: "global_settings.max_evaluation_time_ms".to_string(),
                error_type: ValidationErrorType::InvalidValue,
                message: "Max evaluation time must be greater than 0".to_string(),
                suggestion: Some("Set max_evaluation_time_ms to at least 1ms".to_string()),
                value: Some("0".to_string()),
            });
        } else if settings.max_evaluation_time_ms > 100 {
            errors.push(ValidationError {
                field_path: "global_settings.max_evaluation_time_ms".to_string(),
                error_type: ValidationErrorType::Performance,
                message: "Max evaluation time exceeds performance target (100ms)".to_string(),
                suggestion: Some("Set max_evaluation_time_ms to 1-10ms for optimal performance".to_string()),
                value: Some(settings.max_evaluation_time_ms.to_string()),
            });
        }
    }
    
    fn validate_flag_with_context(
        &self, 
        name: &str, 
        flag: &FeatureFlag, 
        collection: &FeatureFlagCollection,
        errors: &mut Vec<ValidationError>
    ) {
        let base_path = format!("flags.{}", name);
        
        // Basic structure validation
        self.validate_flag_structure_with_path(flag, &base_path, errors);
        
        // Context-specific validation
        self.validate_flag_for_environment(flag, &base_path, errors);
        self.validate_flag_metadata_requirements(flag, &base_path, errors);
        self.validate_flag_security(flag, &base_path, errors);
    }
    
    fn validate_flag_structure(&self, flag: &FeatureFlag, errors: &mut Vec<ValidationError>) {
        self.validate_flag_structure_with_path(flag, "flag", errors);
    }
    
    fn validate_flag_structure_with_path(&self, flag: &FeatureFlag, base_path: &str, errors: &mut Vec<ValidationError>) {
        // Name validation
        if flag.name.is_empty() {
            errors.push(ValidationError {
                field_path: format!("{}.name", base_path),
                error_type: ValidationErrorType::Required,
                message: "Feature flag name cannot be empty".to_string(),
                suggestion: Some("Provide a descriptive name using lowercase letters, numbers, and underscores".to_string()),
                value: None,
            });
        } else if !self.is_valid_flag_name(&flag.name) {
            errors.push(ValidationError {
                field_path: format!("{}.name", base_path),
                error_type: ValidationErrorType::InvalidFormat,
                message: format!("Invalid flag name format: {}", flag.name),
                suggestion: Some("Use lowercase letters, numbers, and underscores only (e.g., 'my_feature_flag')".to_string()),
                value: Some(flag.name.clone()),
            });
        }
        
        // Rollout percentage validation
        if let Some(percentage) = flag.rollout_percentage {
            if percentage > 100 {
                errors.push(ValidationError {
                    field_path: format!("{}.rollout_percentage", base_path),
                    error_type: ValidationErrorType::OutOfRange,
                    message: format!("Rollout percentage cannot exceed 100: {}", percentage),
                    suggestion: Some("Set rollout_percentage between 0 and 100".to_string()),
                    value: Some(percentage.to_string()),
                });
            }
        }
        
        // Conditions validation
        if let Some(conditions) = &flag.conditions {
            for (i, condition) in conditions.iter().enumerate() {
                self.validate_condition(condition, &format!("{}.conditions[{}]", base_path, i), errors);
            }
        }
        
        // Targets validation
        if let Some(targets) = &flag.targets {
            self.validate_targets(targets, &format!("{}.targets", base_path), errors);
        }
        
        // Timestamp validation
        self.validate_timestamps(flag, base_path, errors);
    }
    
    fn validate_condition(&self, condition: &FeatureCondition, path: &str, errors: &mut Vec<ValidationError>) {
        match condition {
            FeatureCondition::SyncProgressAbove(p) | FeatureCondition::SyncProgressBelow(p) => {
                if *p < 0.0 || *p > 1.0 {
                    errors.push(ValidationError {
                        field_path: path.to_string(),
                        error_type: ValidationErrorType::OutOfRange,
                        message: format!("Sync progress must be between 0.0 and 1.0, got: {}", p),
                        suggestion: Some("Use a decimal value between 0.0 (0%) and 1.0 (100%)".to_string()),
                        value: Some(p.to_string()),
                    });
                }
            }
            FeatureCondition::TimeWindow { start_hour, end_hour } => {
                if *start_hour > 23 {
                    errors.push(ValidationError {
                        field_path: format!("{}.start_hour", path),
                        error_type: ValidationErrorType::OutOfRange,
                        message: format!("Start hour must be 0-23, got: {}", start_hour),
                        suggestion: Some("Use 24-hour format (0-23)".to_string()),
                        value: Some(start_hour.to_string()),
                    });
                }
                if *end_hour > 23 {
                    errors.push(ValidationError {
                        field_path: format!("{}.end_hour", path),
                        error_type: ValidationErrorType::OutOfRange,
                        message: format!("End hour must be 0-23, got: {}", end_hour),
                        suggestion: Some("Use 24-hour format (0-23)".to_string()),
                        value: Some(end_hour.to_string()),
                    });
                }
            }
            FeatureCondition::NodeHealth { max_cpu_usage_percent, min_memory_mb, .. } => {
                if let Some(cpu) = max_cpu_usage_percent {
                    if *cpu > 100 {
                        errors.push(ValidationError {
                            field_path: format!("{}.max_cpu_usage_percent", path),
                            error_type: ValidationErrorType::OutOfRange,
                            message: format!("CPU usage percentage cannot exceed 100: {}", cpu),
                            suggestion: Some("Set max_cpu_usage_percent between 0 and 100".to_string()),
                            value: Some(cpu.to_string()),
                        });
                    }
                }
                
                if let Some(memory) = min_memory_mb {
                    if *memory == 0 {
                        errors.push(ValidationError {
                            field_path: format!("{}.min_memory_mb", path),
                            error_type: ValidationErrorType::InvalidValue,
                            message: "Minimum memory cannot be 0".to_string(),
                            suggestion: Some("Set min_memory_mb to a positive value (e.g., 512)".to_string()),
                            value: Some("0".to_string()),
                        });
                    } else if *memory > 128 * 1024 { // 128GB
                        errors.push(ValidationError {
                            field_path: format!("{}.min_memory_mb", path),
                            error_type: ValidationErrorType::OutOfRange,
                            message: format!("Minimum memory requirement seems excessive: {}MB", memory),
                            suggestion: Some("Consider a more reasonable memory requirement".to_string()),
                            value: Some(memory.to_string()),
                        });
                    }
                }
            }
            _ => {} // Other conditions are structurally valid
        }
    }
    
    fn validate_targets(&self, targets: &FeatureTargets, path: &str, errors: &mut Vec<ValidationError>) {
        // IP ranges validation
        if let Some(ip_ranges) = &targets.ip_ranges {
            for (i, range) in ip_ranges.iter().enumerate() {
                if range.parse::<ipnetwork::IpNetwork>().is_err() {
                    errors.push(ValidationError {
                        field_path: format!("{}.ip_ranges[{}]", path, i),
                        error_type: ValidationErrorType::InvalidFormat,
                        message: format!("Invalid IP range format: {}", range),
                        suggestion: Some("Use CIDR notation (e.g., '192.168.1.0/24' or '10.0.0.1/32')".to_string()),
                        value: Some(range.clone()),
                    });
                }
            }
        }
        
        // Node IDs validation
        if let Some(node_ids) = &targets.node_ids {
            for (i, node_id) in node_ids.iter().enumerate() {
                if node_id.is_empty() {
                    errors.push(ValidationError {
                        field_path: format!("{}.node_ids[{}]", path, i),
                        error_type: ValidationErrorType::InvalidValue,
                        message: "Node ID cannot be empty".to_string(),
                        suggestion: Some("Remove empty node IDs or provide valid identifiers".to_string()),
                        value: None,
                    });
                }
            }
        }
        
        // Validator keys validation
        if let Some(validator_keys) = &targets.validator_keys {
            for (i, key) in validator_keys.iter().enumerate() {
                if key.len() != 64 && key.len() != 96 { // Common key lengths
                    errors.push(ValidationError {
                        field_path: format!("{}.validator_keys[{}]", path, i),
                        error_type: ValidationErrorType::InvalidFormat,
                        message: format!("Validator key has unexpected length: {}", key.len()),
                        suggestion: Some("Ensure validator key is a valid hex string (32 or 48 bytes)".to_string()),
                        value: Some(format!("{}...", &key[..8.min(key.len())])),
                    });
                }
            }
        }
    }
    
    fn validate_timestamps(&self, flag: &FeatureFlag, base_path: &str, errors: &mut Vec<ValidationError>) {
        let now = Utc::now();
        
        // Check if created_at is in the future
        if flag.created_at > now {
            errors.push(ValidationError {
                field_path: format!("{}.created_at", base_path),
                error_type: ValidationErrorType::InvalidValue,
                message: "Created timestamp cannot be in the future".to_string(),
                suggestion: Some("Set created_at to current time or earlier".to_string()),
                value: Some(flag.created_at.to_string()),
            });
        }
        
        // Check if updated_at is before created_at
        if flag.updated_at < flag.created_at {
            errors.push(ValidationError {
                field_path: format!("{}.updated_at", base_path),
                error_type: ValidationErrorType::Inconsistent,
                message: "Updated timestamp cannot be before created timestamp".to_string(),
                suggestion: Some("Set updated_at to be equal to or after created_at".to_string()),
                value: Some(format!("updated: {}, created: {}", flag.updated_at, flag.created_at)),
            });
        }
        
        // Warn about old flags
        let six_months_ago = now - Duration::days(180);
        if flag.updated_at < six_months_ago && self.context.deprecated_warnings {
            errors.push(ValidationError {
                field_path: format!("{}.updated_at", base_path),
                error_type: ValidationErrorType::Deprecated,
                message: "Flag hasn't been updated in over 6 months".to_string(),
                suggestion: Some("Review if this flag is still needed and update metadata".to_string()),
                value: Some(flag.updated_at.to_string()),
            });
        }
    }
    
    fn validate_flag_for_environment(&self, flag: &FeatureFlag, base_path: &str, errors: &mut Vec<ValidationError>) {
        match self.context.environment {
            Environment::Production => {
                // Production-specific validations
                if flag.rollout_percentage.is_none() && flag.enabled {
                    errors.push(ValidationError {
                        field_path: format!("{}.rollout_percentage", base_path),
                        error_type: ValidationErrorType::Security,
                        message: "Production flags should specify rollout percentage".to_string(),
                        suggestion: Some("Add rollout_percentage to control blast radius".to_string()),
                        value: None,
                    });
                }
                
                if flag.description.is_none() {
                    errors.push(ValidationError {
                        field_path: format!("{}.description", base_path),
                        error_type: ValidationErrorType::Required,
                        message: "Production flags must have descriptions".to_string(),
                        suggestion: Some("Add description explaining the flag's purpose".to_string()),
                        value: None,
                    });
                }
            }
            Environment::Development => {
                // Development-specific validations
                if let Some(percentage) = flag.rollout_percentage {
                    if percentage > 50 && flag.metadata.get("experimental").is_some() {
                        errors.push(ValidationError {
                            field_path: format!("{}.rollout_percentage", base_path),
                            error_type: ValidationErrorType::Security,
                            message: "Experimental flags should have limited rollout in development".to_string(),
                            suggestion: Some("Keep experimental flags under 50% rollout".to_string()),
                            value: Some(percentage.to_string()),
                        });
                    }
                }
            }
            _ => {} // Other environments have default validation
        }
    }
    
    fn validate_flag_metadata_requirements(&self, flag: &FeatureFlag, base_path: &str, errors: &mut Vec<ValidationError>) {
        // Required metadata fields
        let required_fields = match self.context.environment {
            Environment::Production => vec!["owner", "risk"],
            Environment::Testing => vec!["owner"],
            _ => vec![],
        };
        
        for field in required_fields {
            if !flag.metadata.contains_key(field) {
                errors.push(ValidationError {
                    field_path: format!("{}.metadata.{}", base_path, field),
                    error_type: ValidationErrorType::Required,
                    message: format!("Required metadata field missing: {}", field),
                    suggestion: Some(format!("Add {} = \"...\" to flag metadata", field)),
                    value: None,
                });
            }
        }
        
        // Validate risk level
        if let Some(risk) = flag.metadata.get("risk") {
            if !["low", "medium", "high", "critical"].contains(&risk.as_str()) {
                errors.push(ValidationError {
                    field_path: format!("{}.metadata.risk", base_path),
                    error_type: ValidationErrorType::InvalidValue,
                    message: format!("Invalid risk level: {}", risk),
                    suggestion: Some("Use one of: low, medium, high, critical".to_string()),
                    value: Some(risk.clone()),
                });
            }
        }
    }
    
    fn validate_flag_security(&self, flag: &FeatureFlag, base_path: &str, errors: &mut Vec<ValidationError>) {
        // Check for potential security issues
        if let Some(description) = &flag.description {
            if description.to_lowercase().contains("password") || description.to_lowercase().contains("secret") {
                errors.push(ValidationError {
                    field_path: format!("{}.description", base_path),
                    error_type: ValidationErrorType::Security,
                    message: "Description may contain sensitive information".to_string(),
                    suggestion: Some("Avoid referencing credentials in flag descriptions".to_string()),
                    value: None,
                });
            }
        }
        
        // Check metadata for sensitive info
        for (key, value) in &flag.metadata {
            if key.to_lowercase().contains("password") || value.to_lowercase().contains("secret") {
                errors.push(ValidationError {
                    field_path: format!("{}.metadata.{}", base_path, key),
                    error_type: ValidationErrorType::Security,
                    message: "Metadata may contain sensitive information".to_string(),
                    suggestion: Some("Remove sensitive data from flag metadata".to_string()),
                    value: None,
                });
            }
        }
    }
    
    fn validate_flag_consistency(&self, flags: &HashMap<String, FeatureFlag>, errors: &mut Vec<ValidationError>) {
        // Check for naming conflicts
        let mut name_groups: HashMap<String, Vec<String>> = HashMap::new();
        for name in flags.keys() {
            let normalized = name.replace('_', "-");
            name_groups.entry(normalized).or_default().push(name.clone());
        }
        
        for (normalized, names) in name_groups {
            if names.len() > 1 {
                for name in names {
                    errors.push(ValidationError {
                        field_path: format!("flags.{}", name),
                        error_type: ValidationErrorType::Inconsistent,
                        message: format!("Potential naming conflict with similar flags: {}", normalized),
                        suggestion: Some("Use consistent naming conventions to avoid confusion".to_string()),
                        value: Some(name),
                    });
                }
            }
        }
        
        // Check for dependency cycles in conditions
        // This would require more complex analysis of condition dependencies
    }
    
    // Helper methods
    
    fn is_valid_version(&self, version: &str) -> bool {
        static VERSION_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\d+)\.(\d+)(\.\d+)?(-[\w\d\-\.]+)?$").unwrap()
        });
        VERSION_REGEX.is_match(version)
    }
    
    fn is_valid_flag_name(&self, name: &str) -> bool {
        static FLAG_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^[a-z][a-z0-9_]*[a-z0-9]$|^[a-z]$").unwrap()
        });
        FLAG_NAME_REGEX.is_match(name)
    }
    
    fn format_error_type(&self, error_type: &ValidationErrorType) -> &'static str {
        match error_type {
            ValidationErrorType::Required => "Required Fields",
            ValidationErrorType::InvalidFormat => "Format Errors", 
            ValidationErrorType::InvalidValue => "Invalid Values",
            ValidationErrorType::OutOfRange => "Range Errors",
            ValidationErrorType::Inconsistent => "Consistency Issues",
            ValidationErrorType::Deprecated => "Deprecation Warnings",
            ValidationErrorType::Security => "Security Concerns",
            ValidationErrorType::Performance => "Performance Warnings",
        }
    }
}

impl Default for FeatureFlagValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick validation function for use in manager
pub fn validate_flag_quick(flag: &FeatureFlag) -> Result<(), String> {
    let validator = FeatureFlagValidator::new();
    match validator.validate_flag(flag) {
        Ok(()) => Ok(()),
        Err(errors) => {
            let first_error = errors.first().unwrap();
            Err(format!("{}: {}", first_error.field_path, first_error.message))
        }
    }
}

/// Validate collection and return formatted report
pub fn validate_collection_with_report(
    collection: &FeatureFlagCollection,
    context: Option<ValidationContext>
) -> (bool, String) {
    let validator = if let Some(ctx) = context {
        FeatureFlagValidator::with_context(ctx)
    } else {
        FeatureFlagValidator::new()
    };
    
    match validator.validate_collection(collection) {
        Ok(()) => (true, validator.generate_report(&[])),
        Err(errors) => (false, validator.generate_report(&errors)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_flag_name_validation() {
        let validator = FeatureFlagValidator::new();
        
        assert!(validator.is_valid_flag_name("test_flag"));
        assert!(validator.is_valid_flag_name("a"));
        assert!(validator.is_valid_flag_name("my_feature_v2"));
        
        assert!(!validator.is_valid_flag_name("Test_Flag")); // Capital letters
        assert!(!validator.is_valid_flag_name("test-flag")); // Hyphens
        assert!(!validator.is_valid_flag_name("test flag")); // Spaces
        assert!(!validator.is_valid_flag_name("_test")); // Starting with underscore
        assert!(!validator.is_valid_flag_name("test_")); // Ending with underscore
        assert!(!validator.is_valid_flag_name("")); // Empty
    }
    
    #[test]
    fn test_version_validation() {
        let validator = FeatureFlagValidator::new();
        
        assert!(validator.is_valid_version("1.0"));
        assert!(validator.is_valid_version("1.0.0"));
        assert!(validator.is_valid_version("2.1.3"));
        assert!(validator.is_valid_version("1.0.0-beta"));
        
        assert!(!validator.is_valid_version("1"));
        assert!(!validator.is_valid_version("v1.0"));
        assert!(!validator.is_valid_version("1.0."));
        assert!(!validator.is_valid_version(""));
    }
    
    #[test]
    fn test_enhanced_validation() {
        let validator = FeatureFlagValidator::new();
        
        let mut flag = FeatureFlag::new("invalid name".to_string(), true);
        flag.rollout_percentage = Some(150); // Invalid
        
        let result = validator.validate_flag(&flag);
        assert!(result.is_err());
        
        let errors = result.unwrap_err();
        assert!(errors.len() >= 2); // At least name and percentage errors
    }
    
    #[test]
    fn test_validation_report() {
        let validator = FeatureFlagValidator::new();
        
        let errors = vec![
            ValidationError {
                field_path: "flags.test.name".to_string(),
                error_type: ValidationErrorType::InvalidFormat,
                message: "Invalid flag name format".to_string(),
                suggestion: Some("Use lowercase with underscores".to_string()),
                value: Some("Test Name".to_string()),
            }
        ];
        
        let report = validator.generate_report(&errors);
        assert!(report.contains("Format Errors"));
        assert!(report.contains("Invalid flag name format"));
        assert!(report.contains("Suggestion:"));
    }
}