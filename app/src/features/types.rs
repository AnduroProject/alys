//! Core feature flag data structures
//!
//! This module defines the core data structures for the feature flag system,
//! including FeatureFlag, targeting rules, conditions, and metadata.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use std::net::IpAddr;
use crate::config::Environment;

/// A feature flag definition with targeting, conditions, and rollout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    /// Unique name for the feature flag
    pub name: String,
    
    /// Whether the flag is globally enabled
    pub enabled: bool,
    
    /// Optional percentage rollout (0-100)
    pub rollout_percentage: Option<u8>,
    
    /// Targeting rules for specific users/nodes
    pub targets: Option<FeatureTargets>,
    
    /// Conditional logic for flag evaluation
    pub conditions: Option<Vec<FeatureCondition>>,
    
    /// Additional metadata for the flag
    pub metadata: HashMap<String, String>,
    
    /// When the flag was created
    pub created_at: DateTime<Utc>,
    
    /// When the flag was last updated
    pub updated_at: DateTime<Utc>,
    
    /// Who last updated the flag
    pub updated_by: String,
    
    /// Optional description of the flag's purpose
    pub description: Option<String>,
}

impl FeatureFlag {
    /// Create a new feature flag with default values
    pub fn new(name: String, enabled: bool) -> Self {
        let now = Utc::now();
        Self {
            name,
            enabled,
            rollout_percentage: None,
            targets: None,
            conditions: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            updated_by: "system".to_string(),
            description: None,
        }
    }
    
    /// Create a simple enabled flag
    pub fn enabled(name: String) -> Self {
        Self::new(name, true)
    }
    
    /// Create a simple disabled flag
    pub fn disabled(name: String) -> Self {
        Self::new(name, false)
    }
    
    /// Create a flag with percentage rollout
    pub fn with_percentage(name: String, enabled: bool, percentage: u8) -> Self {
        let mut flag = Self::new(name, enabled);
        flag.rollout_percentage = Some(percentage.min(100));
        flag
    }
    
    /// Add metadata to the flag
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// Add description to the flag
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
    
    /// Add targets to the flag
    pub fn with_targets(mut self, targets: FeatureTargets) -> Self {
        self.targets = Some(targets);
        self
    }
    
    /// Add conditions to the flag
    pub fn with_conditions(mut self, conditions: Vec<FeatureCondition>) -> Self {
        self.conditions = Some(conditions);
        self
    }
    
    /// Update the flag's modification timestamp
    pub fn touch(&mut self, updated_by: String) {
        self.updated_at = Utc::now();
        self.updated_by = updated_by;
    }
}

/// Targeting rules for feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureTargets {
    /// Specific node IDs to target
    pub node_ids: Option<Vec<String>>,
    
    /// Specific validator public keys to target
    pub validator_keys: Option<Vec<String>>,
    
    /// IP address ranges to target (CIDR notation)
    pub ip_ranges: Option<Vec<String>>,
    
    /// Environments to target
    pub environments: Option<Vec<Environment>>,
    
    /// Custom attributes for advanced targeting
    pub custom_attributes: Option<HashMap<String, String>>,
}

impl FeatureTargets {
    /// Create empty targets
    pub fn new() -> Self {
        Self {
            node_ids: None,
            validator_keys: None,
            ip_ranges: None,
            environments: None,
            custom_attributes: None,
        }
    }
    
    /// Target specific node IDs
    pub fn with_node_ids(mut self, node_ids: Vec<String>) -> Self {
        self.node_ids = Some(node_ids);
        self
    }
    
    /// Target specific validator keys
    pub fn with_validator_keys(mut self, validator_keys: Vec<String>) -> Self {
        self.validator_keys = Some(validator_keys);
        self
    }
    
    /// Target specific environments
    pub fn with_environments(mut self, environments: Vec<Environment>) -> Self {
        self.environments = Some(environments);
        self
    }
    
    /// Target specific IP ranges
    pub fn with_ip_ranges(mut self, ip_ranges: Vec<String>) -> Self {
        self.ip_ranges = Some(ip_ranges);
        self
    }
    
    /// Add custom targeting attributes
    pub fn with_custom_attributes(mut self, attributes: HashMap<String, String>) -> Self {
        self.custom_attributes = Some(attributes);
        self
    }
}

impl Default for FeatureTargets {
    fn default() -> Self {
        Self::new()
    }
}

/// Conditional logic for feature flag evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureCondition {
    /// Enable after a specific date/time
    After(DateTime<Utc>),
    
    /// Enable before a specific date/time
    Before(DateTime<Utc>),
    
    /// Enable after reaching a specific chain height
    ChainHeightAbove(u64),
    
    /// Enable below a specific chain height
    ChainHeightBelow(u64),
    
    /// Enable when sync progress is above threshold (0.0-1.0)
    SyncProgressAbove(f64),
    
    /// Enable when sync progress is below threshold (0.0-1.0)
    SyncProgressBelow(f64),
    
    /// Custom condition using a string expression
    Custom(String),
    
    /// Enable only during specific time windows
    TimeWindow {
        start_hour: u8,  // 0-23
        end_hour: u8,    // 0-23
    },
    
    /// Enable based on node health metrics
    NodeHealth {
        min_peers: Option<u32>,
        max_memory_usage_mb: Option<u64>,
        max_cpu_usage_percent: Option<u8>,
    },
}

impl FeatureCondition {
    /// Create a time-based condition (after)
    pub fn after(datetime: DateTime<Utc>) -> Self {
        FeatureCondition::After(datetime)
    }
    
    /// Create a time-based condition (before)
    pub fn before(datetime: DateTime<Utc>) -> Self {
        FeatureCondition::Before(datetime)
    }
    
    /// Create a chain height condition (above)
    pub fn chain_height_above(height: u64) -> Self {
        FeatureCondition::ChainHeightAbove(height)
    }
    
    /// Create a sync progress condition
    pub fn sync_progress_above(progress: f64) -> Self {
        FeatureCondition::SyncProgressAbove(progress.clamp(0.0, 1.0))
    }
    
    /// Create a time window condition
    pub fn time_window(start_hour: u8, end_hour: u8) -> Self {
        FeatureCondition::TimeWindow {
            start_hour: start_hour % 24,
            end_hour: end_hour % 24,
        }
    }
    
    /// Create a node health condition
    pub fn node_health(min_peers: Option<u32>, max_memory_mb: Option<u64>, max_cpu_percent: Option<u8>) -> Self {
        FeatureCondition::NodeHealth {
            min_peers,
            max_memory_usage_mb: max_memory_mb,
            max_cpu_usage_percent: max_cpu_percent,
        }
    }
    
    /// Create a custom condition
    pub fn custom(expression: String) -> Self {
        FeatureCondition::Custom(expression)
    }
}

/// Rollout strategies for feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RolloutStrategy {
    /// Simple percentage-based rollout
    Percentage(u8),
    
    /// Canary release to specific targets first, then percentage
    Canary {
        targets: FeatureTargets,
        fallback_percentage: u8,
    },
    
    /// Ring-based rollout (staged deployment)
    Ring {
        rings: Vec<RolloutRing>,
    },
    
    /// Blue-green deployment strategy
    BlueGreen {
        active_variant: String,
        variants: HashMap<String, FeatureVariant>,
    },
}

/// A single rollout ring for staged deployments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutRing {
    pub name: String,
    pub targets: FeatureTargets,
    pub percentage: u8,
    pub delay_hours: Option<u32>,
}

/// Feature variant for A/B testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVariant {
    pub name: String,
    pub percentage: u8,
    pub configuration: HashMap<String, serde_json::Value>,
}

/// Feature flag collection for bulk operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagCollection {
    /// Version of the configuration format
    pub version: String,
    
    /// Global feature flags configuration
    pub flags: HashMap<String, FeatureFlag>,
    
    /// Default environment for evaluation
    pub default_environment: Environment,
    
    /// Global settings affecting all flags
    pub global_settings: FeatureFlagGlobalSettings,
}

impl FeatureFlagCollection {
    /// Create a new empty collection
    pub fn new() -> Self {
        Self {
            version: "1.0".to_string(),
            flags: HashMap::new(),
            default_environment: Environment::Development,
            global_settings: FeatureFlagGlobalSettings::default(),
        }
    }
    
    /// Add a flag to the collection
    pub fn add_flag(&mut self, flag: FeatureFlag) {
        self.flags.insert(flag.name.clone(), flag);
    }
    
    /// Remove a flag from the collection
    pub fn remove_flag(&mut self, name: &str) -> Option<FeatureFlag> {
        self.flags.remove(name)
    }
    
    /// Get a flag by name
    pub fn get_flag(&self, name: &str) -> Option<&FeatureFlag> {
        self.flags.get(name)
    }
    
    /// Get mutable reference to a flag
    pub fn get_flag_mut(&mut self, name: &str) -> Option<&mut FeatureFlag> {
        self.flags.get_mut(name)
    }
    
    /// List all flag names
    pub fn flag_names(&self) -> Vec<&String> {
        self.flags.keys().collect()
    }
}

impl Default for FeatureFlagCollection {
    fn default() -> Self {
        Self::new()
    }
}

/// Global settings for the feature flag system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagGlobalSettings {
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    
    /// Enable audit logging
    pub enable_audit_log: bool,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
    
    /// Default rollout strategy
    pub default_rollout_strategy: Option<RolloutStrategy>,
    
    /// Performance limits
    pub max_evaluation_time_ms: u64,
}

impl Default for FeatureFlagGlobalSettings {
    fn default() -> Self {
        Self {
            cache_ttl_seconds: 5,
            enable_audit_log: true,
            enable_metrics: true,
            default_rollout_strategy: None,
            max_evaluation_time_ms: 1,
        }
    }
}