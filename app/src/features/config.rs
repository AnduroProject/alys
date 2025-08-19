//! Configuration loading and validation for feature flags
//!
//! This module handles loading feature flag configuration from TOML files,
//! validating the configuration, and providing type-safe access to flag definitions.

use super::types::*;
use super::{FeatureFlagResult, FeatureFlagError};
use crate::config::{ConfigError, Environment, Validate};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use tracing::{info, warn, debug};

/// Configuration file loader for feature flags
pub struct FeatureFlagConfigLoader {
    /// Whether to validate configuration on load
    validate_on_load: bool,
}

impl FeatureFlagConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self {
            validate_on_load: true,
        }
    }
    
    /// Create loader with custom validation setting
    pub fn with_validation(validate_on_load: bool) -> Self {
        Self {
            validate_on_load,
        }
    }
    
    /// Load configuration from a TOML file
    pub fn load_from_file<P: AsRef<Path>>(&self, path: P) -> FeatureFlagResult<FeatureFlagCollection> {
        let path = path.as_ref();
        
        debug!("Loading feature flag configuration from {}", path.display());
        
        let content = fs::read_to_string(path)
            .map_err(|e| FeatureFlagError::IoError {
                operation: format!("reading config file {}", path.display()),
                error: e.to_string(),
            })?;
        
        self.parse_toml_content(&content)
    }
    
    /// Load configuration from environment variables
    pub fn load_from_env(&self, prefix: &str) -> FeatureFlagResult<FeatureFlagCollection> {
        let mut collection = FeatureFlagCollection::new();
        
        // Load global settings from environment
        if let Ok(cache_ttl) = std::env::var(format!("{}_CACHE_TTL_SECONDS", prefix)) {
            if let Ok(ttl) = cache_ttl.parse::<u64>() {
                collection.global_settings.cache_ttl_seconds = ttl;
            }
        }
        
        if let Ok(env_str) = std::env::var(format!("{}_ENVIRONMENT", prefix)) {
            if let Ok(env) = env_str.parse::<Environment>() {
                collection.default_environment = env;
            }
        }
        
        // Load individual flags from environment
        // Format: ALYS_FLAG_<FLAG_NAME>_ENABLED=true
        for (key, value) in std::env::vars() {
            if let Some(flag_name) = self.parse_env_flag_name(&key, prefix) {
                let mut flag = collection.flags.entry(flag_name.clone())
                    .or_insert_with(|| FeatureFlag::new(flag_name, false));
                
                self.apply_env_setting(&key, &value, flag, prefix);
            }
        }
        
        if self.validate_on_load {
            collection.validate()
                .map_err(|e| FeatureFlagError::ValidationError {
                    flag: "configuration".to_string(),
                    reason: e.to_string(),
                })?;
        }
        
        info!("Loaded {} feature flags from environment", collection.flags.len());
        Ok(collection)
    }
    
    /// Parse TOML content into feature flag collection
    pub fn parse_toml_content(&self, content: &str) -> FeatureFlagResult<FeatureFlagCollection> {
        let raw_config: RawFeatureFlagConfig = toml::from_str(content)
            .map_err(|e| FeatureFlagError::SerializationError {
                reason: format!("TOML parse error: {}", e),
            })?;
        
        let collection = self.convert_raw_config(raw_config)?;
        
        if self.validate_on_load {
            collection.validate()
                .map_err(|e| FeatureFlagError::ValidationError {
                    flag: "configuration".to_string(),
                    reason: e.to_string(),
                })?;
        }
        
        info!("Loaded {} feature flags from TOML", collection.flags.len());
        Ok(collection)
    }
    
    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(
        &self, 
        collection: &FeatureFlagCollection, 
        path: P
    ) -> FeatureFlagResult<()> {
        let path = path.as_ref();
        
        debug!("Saving feature flag configuration to {}", path.display());
        
        let raw_config = self.convert_to_raw_config(collection)?;
        let toml_content = toml::to_string_pretty(&raw_config)
            .map_err(|e| FeatureFlagError::SerializationError {
                reason: format!("TOML serialization error: {}", e),
            })?;
        
        fs::write(path, toml_content)
            .map_err(|e| FeatureFlagError::IoError {
                operation: format!("writing config file {}", path.display()),
                error: e.to_string(),
            })?;
        
        info!("Saved feature flag configuration to {}", path.display());
        Ok(())
    }
    
    /// Create a default configuration file
    pub fn create_default_config<P: AsRef<Path>>(path: P) -> FeatureFlagResult<()> {
        let default_config = Self::default_config();
        let loader = Self::new();
        loader.save_to_file(&default_config, path)
    }
    
    /// Get default configuration
    pub fn default_config() -> FeatureFlagCollection {
        let mut collection = FeatureFlagCollection::new();
        
        // Add some example flags
        collection.add_flag(
            FeatureFlag::disabled("actor_system".to_string())
                .with_description("Enable actor-based architecture".to_string())
                .with_metadata("risk".to_string(), "high".to_string())
                .with_metadata("owner".to_string(), "platform-team".to_string())
        );
        
        collection.add_flag(
            FeatureFlag::disabled("improved_sync".to_string())
                .with_percentage(0)
                .with_description("Use improved sync algorithm".to_string())
                .with_metadata("risk".to_string(), "medium".to_string())
                .with_targets(FeatureTargets::new().with_environments(vec![Environment::Testing]))
        );
        
        collection.add_flag(
            FeatureFlag::enabled("parallel_validation".to_string())
                .with_percentage(100)
                .with_description("Enable parallel block validation".to_string())
                .with_metadata("risk".to_string(), "low".to_string())
        );
        
        collection
    }
    
    // Private helper methods
    
    fn convert_raw_config(&self, raw: RawFeatureFlagConfig) -> FeatureFlagResult<FeatureFlagCollection> {
        let mut collection = FeatureFlagCollection::new();
        
        collection.version = raw.version.unwrap_or_else(|| "1.0".to_string());
        collection.default_environment = raw.default_environment.unwrap_or(Environment::Development);
        collection.global_settings = raw.global_settings.unwrap_or_default();
        
        if let Some(flags) = raw.flags {
            for (name, raw_flag) in flags {
                let flag = self.convert_raw_flag(name, raw_flag)?;
                collection.add_flag(flag);
            }
        }
        
        Ok(collection)
    }
    
    fn convert_raw_flag(&self, name: String, raw: RawFeatureFlag) -> FeatureFlagResult<FeatureFlag> {
        let mut flag = FeatureFlag::new(name, raw.enabled);
        
        flag.rollout_percentage = raw.rollout_percentage;
        flag.targets = raw.targets;
        flag.conditions = raw.conditions;
        flag.metadata = raw.metadata.unwrap_or_default();
        flag.description = raw.description;
        
        // Handle timestamps
        flag.created_at = raw.created_at.unwrap_or_else(Utc::now);
        flag.updated_at = raw.updated_at.unwrap_or_else(Utc::now);
        flag.updated_by = raw.updated_by.unwrap_or_else(|| "system".to_string());
        
        Ok(flag)
    }
    
    fn convert_to_raw_config(&self, collection: &FeatureFlagCollection) -> FeatureFlagResult<RawFeatureFlagConfig> {
        let mut raw_flags = HashMap::new();
        
        for (name, flag) in &collection.flags {
            raw_flags.insert(name.clone(), self.convert_to_raw_flag(flag));
        }
        
        Ok(RawFeatureFlagConfig {
            version: Some(collection.version.clone()),
            default_environment: Some(collection.default_environment),
            global_settings: Some(collection.global_settings.clone()),
            flags: Some(raw_flags),
        })
    }
    
    fn convert_to_raw_flag(&self, flag: &FeatureFlag) -> RawFeatureFlag {
        RawFeatureFlag {
            enabled: flag.enabled,
            rollout_percentage: flag.rollout_percentage,
            targets: flag.targets.clone(),
            conditions: flag.conditions.clone(),
            metadata: if flag.metadata.is_empty() { None } else { Some(flag.metadata.clone()) },
            created_at: Some(flag.created_at),
            updated_at: Some(flag.updated_at),
            updated_by: Some(flag.updated_by.clone()),
            description: flag.description.clone(),
        }
    }
    
    fn parse_env_flag_name(&self, env_key: &str, prefix: &str) -> Option<String> {
        let expected_prefix = format!("{}_FLAG_", prefix.to_uppercase());
        if env_key.starts_with(&expected_prefix) {
            let remainder = &env_key[expected_prefix.len()..];
            if let Some(underscore_pos) = remainder.find('_') {
                let flag_name = &remainder[..underscore_pos];
                return Some(flag_name.to_lowercase());
            }
        }
        None
    }
    
    fn apply_env_setting(&self, env_key: &str, env_value: &str, flag: &mut FeatureFlag, prefix: &str) {
        let flag_prefix = format!("{}_FLAG_{}_", prefix.to_uppercase(), flag.name.to_uppercase());
        
        if let Some(setting) = env_key.strip_prefix(&flag_prefix) {
            match setting {
                "ENABLED" => {
                    flag.enabled = env_value.parse().unwrap_or(false);
                }
                "ROLLOUT_PERCENTAGE" => {
                    flag.rollout_percentage = env_value.parse().ok();
                }
                "DESCRIPTION" => {
                    flag.description = Some(env_value.to_string());
                }
                _ if setting.starts_with("META_") => {
                    let meta_key = setting.strip_prefix("META_").unwrap().to_lowercase();
                    flag.metadata.insert(meta_key, env_value.to_string());
                }
                _ => {
                    warn!("Unknown environment setting for flag '{}': {}", flag.name, setting);
                }
            }
        }
    }
}

impl Default for FeatureFlagConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw configuration structure for TOML deserialization
#[derive(Debug, Serialize, Deserialize)]
struct RawFeatureFlagConfig {
    version: Option<String>,
    default_environment: Option<Environment>,
    global_settings: Option<FeatureFlagGlobalSettings>,
    flags: Option<HashMap<String, RawFeatureFlag>>,
}

/// Raw feature flag for TOML deserialization
#[derive(Debug, Serialize, Deserialize)]
struct RawFeatureFlag {
    enabled: bool,
    rollout_percentage: Option<u8>,
    targets: Option<FeatureTargets>,
    conditions: Option<Vec<FeatureCondition>>,
    metadata: Option<HashMap<String, String>>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    updated_by: Option<String>,
    description: Option<String>,
}

/// Validation implementation for feature flag collection
impl Validate for FeatureFlagCollection {
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate version
        if self.version.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "version".to_string(),
                reason: "Version cannot be empty".to_string(),
            });
        }
        
        // Validate each flag
        for (name, flag) in &self.flags {
            if let Err(e) = flag.validate() {
                return Err(ConfigError::ValidationError {
                    field: format!("flags.{}", name),
                    reason: e.to_string(),
                });
            }
        }
        
        // Validate global settings
        self.global_settings.validate()?;
        
        Ok(())
    }
}

/// Validation implementation for feature flags
impl Validate for FeatureFlag {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "name".to_string(),
                reason: "Feature flag name cannot be empty".to_string(),
            });
        }
        
        if let Some(percentage) = self.rollout_percentage {
            if percentage > 100 {
                return Err(ConfigError::ValidationError {
                    field: "rollout_percentage".to_string(),
                    reason: "Rollout percentage cannot exceed 100".to_string(),
                });
            }
        }
        
        // Validate conditions
        if let Some(conditions) = &self.conditions {
            for (i, condition) in conditions.iter().enumerate() {
                if let Err(e) = Self::validate_condition(condition) {
                    return Err(ConfigError::ValidationError {
                        field: format!("conditions[{}]", i),
                        reason: e,
                    });
                }
            }
        }
        
        // Validate targets
        if let Some(targets) = &self.targets {
            targets.validate()?;
        }
        
        Ok(())
    }
}

impl FeatureFlag {
    fn validate_condition(condition: &FeatureCondition) -> Result<(), String> {
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
            FeatureCondition::NodeHealth { max_cpu_usage_percent, .. } => {
                if let Some(cpu) = max_cpu_usage_percent {
                    if *cpu > 100 {
                        return Err("CPU usage percentage cannot exceed 100".to_string());
                    }
                }
            }
            _ => {} // Other conditions are valid by construction
        }
        Ok(())
    }
}

/// Validation implementation for feature targets
impl Validate for FeatureTargets {
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate IP ranges if present
        if let Some(ip_ranges) = &self.ip_ranges {
            for (i, range) in ip_ranges.iter().enumerate() {
                if range.parse::<ipnetwork::IpNetwork>().is_err() {
                    return Err(ConfigError::ValidationError {
                        field: format!("ip_ranges[{}]", i),
                        reason: format!("Invalid IP range format: {}", range),
                    });
                }
            }
        }
        
        Ok(())
    }
}

/// Validation implementation for global settings
impl Validate for FeatureFlagGlobalSettings {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.cache_ttl_seconds == 0 {
            return Err(ConfigError::ValidationError {
                field: "cache_ttl_seconds".to_string(),
                reason: "Cache TTL must be greater than 0".to_string(),
            });
        }
        
        if self.max_evaluation_time_ms == 0 {
            return Err(ConfigError::ValidationError {
                field: "max_evaluation_time_ms".to_string(),
                reason: "Max evaluation time must be greater than 0".to_string(),
            });
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    #[test]
    fn test_load_valid_config() {
        let toml_content = r#"
version = "1.0"
default_environment = "development"

[global_settings]
cache_ttl_seconds = 5
enable_audit_log = true
enable_metrics = true
max_evaluation_time_ms = 1

[flags.test_flag]
enabled = true
rollout_percentage = 50
description = "Test flag"
created_at = "2024-01-01T00:00:00Z"
updated_at = "2024-01-01T00:00:00Z"
updated_by = "test"

[flags.test_flag.metadata]
owner = "test-team"
risk = "low"
        "#;
        
        let loader = FeatureFlagConfigLoader::new();
        let config = loader.parse_toml_content(toml_content).unwrap();
        
        assert_eq!(config.version, "1.0");
        assert_eq!(config.default_environment, Environment::Development);
        assert_eq!(config.flags.len(), 1);
        
        let flag = config.get_flag("test_flag").unwrap();
        assert!(flag.enabled);
        assert_eq!(flag.rollout_percentage, Some(50));
        assert_eq!(flag.description, Some("Test flag".to_string()));
        assert_eq!(flag.metadata.get("owner"), Some(&"test-team".to_string()));
    }
    
    #[test]
    fn test_validation() {
        let mut collection = FeatureFlagCollection::new();
        
        // Add invalid flag (empty name)
        let mut invalid_flag = FeatureFlag::new("".to_string(), true);
        invalid_flag.rollout_percentage = Some(150); // Invalid percentage
        
        collection.add_flag(invalid_flag);
        
        assert!(collection.validate().is_err());
    }
    
    #[test]
    fn test_save_and_load_roundtrip() {
        let original = FeatureFlagConfigLoader::default_config();
        let loader = FeatureFlagConfigLoader::new();
        
        let mut temp_file = NamedTempFile::new().unwrap();
        loader.save_to_file(&original, temp_file.path()).unwrap();
        
        let loaded = loader.load_from_file(temp_file.path()).unwrap();
        
        assert_eq!(original.version, loaded.version);
        assert_eq!(original.flags.len(), loaded.flags.len());
    }
}