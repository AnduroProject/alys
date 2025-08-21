//! Actor System Configuration - ALYS-006-01 Implementation
//! 
//! Enhanced configuration for the Alys V2 actor system with comprehensive
//! supervision settings, mailbox capacity management, restart strategies,
//! metrics collection, and blockchain-specific parameters.

use crate::actors::foundation::restart_strategy::RestartStrategy as AlysRestartStrategy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

/// Comprehensive actor system configuration for Alys V2 sidechain
/// 
/// This configuration extends the base actor system with blockchain-specific
/// features required for the merged mining Bitcoin sidechain architecture.
/// It includes supervision policies, mailbox management, restart strategies,
/// metrics collection, and integration with the Alys consensus system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorSystemConfig {
    /// System identification
    pub system_name: String,
    pub system_version: String,
    
    /// Core supervision settings
    pub enable_supervision: bool,
    pub supervision_tree_depth: usize,
    pub default_restart_strategy: AlysRestartStrategy,
    
    /// Mailbox and message handling
    pub default_mailbox_capacity: usize,
    pub high_priority_mailbox_capacity: usize,
    pub message_timeout: Duration,
    pub mailbox_overflow_strategy: MailboxOverflowStrategy,
    
    /// System lifecycle management
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub graceful_shutdown_enabled: bool,
    
    /// Health monitoring and metrics
    pub health_check_enabled: bool,
    pub health_check_interval: Duration,
    pub metrics_enabled: bool,
    pub metrics_collection_interval: Duration,
    
    /// Blockchain-specific settings
    pub blockchain_integration: BlockchainIntegrationConfig,
    
    /// Actor-specific configurations
    pub actor_configs: HashMap<String, ActorSpecificConfig>,
    
    /// Performance and resource management
    pub performance_config: PerformanceConfig,
    
    /// Feature flags for gradual migration
    pub feature_flags: HashMap<String, bool>,
}

/// Mailbox overflow handling strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxOverflowStrategy {
    /// Drop new messages when mailbox is full
    DropNew,
    /// Drop oldest messages to make room for new ones
    DropOld,
    /// Apply backpressure to senders
    Backpressure,
    /// Increase mailbox capacity dynamically
    DynamicResize,
    /// Fail the actor when mailbox overflows
    FailActor,
}

/// Blockchain-specific integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainIntegrationConfig {
    /// Block production interval (2 seconds for Alys)
    pub block_interval: Duration,
    /// Consensus coordination timeout
    pub consensus_timeout: Duration,
    /// Peg-in confirmation requirements
    pub peg_in_confirmations: u32,
    /// AuxPow mining coordination settings
    pub auxpow_coordination: AuxPowConfig,
    /// Federation signature requirements
    pub federation_config: FederationConfig,
}

/// AuxPow mining coordination configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxPowConfig {
    /// Mining work distribution timeout
    pub work_distribution_timeout: Duration,
    /// Block bundle finalization timeout
    pub bundle_finalization_timeout: Duration,
    /// Maximum blocks without PoW before halt
    pub max_blocks_without_pow: u32,
}

/// Federation consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// BLS signature aggregation timeout
    pub signature_timeout: Duration,
    /// Federation member count
    pub member_count: usize,
    /// Consensus threshold (majority + 1)
    pub consensus_threshold: usize,
}

/// Actor-specific configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorSpecificConfig {
    /// Override restart strategy for this actor type
    pub restart_strategy: Option<AlysRestartStrategy>,
    /// Override mailbox capacity
    pub mailbox_capacity: Option<usize>,
    /// Actor priority level
    pub priority: ActorPriority,
    /// Dependencies on other actors
    pub dependencies: Vec<String>,
    /// Custom health check settings
    pub health_check_config: Option<HealthCheckConfig>,
}

/// Actor priority levels for supervision ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ActorPriority {
    /// Critical system actors (ChainActor, EngineActor)
    Critical = 5,
    /// High priority core actors (BridgeActor, AuxPowMinerActor)
    High = 4,
    /// Normal priority supporting actors
    Normal = 3,
    /// Low priority utility actors
    Low = 2,
    /// Background actors (HealthMonitor, MetricsCollector)
    Background = 1,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check interval for this actor
    pub interval: Duration,
    /// Response timeout for health checks
    pub timeout: Duration,
    /// Number of failed checks before marking unhealthy
    pub failure_threshold: u32,
    /// Enable detailed health reporting
    pub detailed_reporting: bool,
}

/// Performance and resource management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum memory usage per actor (bytes)
    pub max_memory_per_actor: u64,
    /// Maximum CPU usage threshold
    pub max_cpu_usage: f64,
    /// Enable performance monitoring
    pub performance_monitoring: bool,
    /// Performance metrics collection interval
    pub metrics_interval: Duration,
    /// Enable automatic resource optimization
    pub auto_optimization: bool,
}

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Invalid mailbox capacity: {0}")]
    InvalidMailboxCapacity(usize),
    #[error("Invalid timeout value: {0:?}")]
    InvalidTimeout(Duration),
    #[error("Invalid actor priority: {0}")]
    InvalidActorPriority(String),
    #[error("Blockchain configuration error: {0}")]
    BlockchainConfig(String),
    #[error("Feature flag error: {0}")]
    FeatureFlag(String),
    #[error("Validation error: {0}")]
    Validation(String),
}

impl Default for ActorSystemConfig {
    fn default() -> Self {
        Self {
            system_name: "alys-v2-actor-system".to_string(),
            system_version: "2.0.0".to_string(),
            
            // Supervision settings optimized for sidechain operations
            enable_supervision: true,
            supervision_tree_depth: 5,
            default_restart_strategy: AlysRestartStrategy::default(),
            
            // Mailbox settings for high-throughput blockchain operations
            default_mailbox_capacity: 10000,
            high_priority_mailbox_capacity: 50000,
            message_timeout: Duration::from_secs(30),
            mailbox_overflow_strategy: MailboxOverflowStrategy::DynamicResize,
            
            // Lifecycle management with blockchain timing considerations
            startup_timeout: Duration::from_secs(60),
            shutdown_timeout: Duration::from_secs(120),
            graceful_shutdown_enabled: true,
            
            // Health monitoring for sidechain reliability
            health_check_enabled: true,
            health_check_interval: Duration::from_secs(10),
            metrics_enabled: true,
            metrics_collection_interval: Duration::from_secs(5),
            
            // Blockchain-specific configuration
            blockchain_integration: BlockchainIntegrationConfig::default(),
            
            // Default actor configurations
            actor_configs: Self::default_actor_configs(),
            
            // Performance settings for blockchain workloads
            performance_config: PerformanceConfig::default(),
            
            // Feature flags for migration control
            feature_flags: Self::default_feature_flags(),
        }
    }
}

impl Default for BlockchainIntegrationConfig {
    fn default() -> Self {
        Self {
            block_interval: Duration::from_secs(2), // 2-second Alys block time
            consensus_timeout: Duration::from_secs(10),
            peg_in_confirmations: 6, // Bitcoin confirmations
            auxpow_coordination: AuxPowConfig::default(),
            federation_config: FederationConfig::default(),
        }
    }
}

impl Default for AuxPowConfig {
    fn default() -> Self {
        Self {
            work_distribution_timeout: Duration::from_secs(30),
            bundle_finalization_timeout: Duration::from_secs(60),
            max_blocks_without_pow: 10, // Halt after 20 seconds without PoW
        }
    }
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            signature_timeout: Duration::from_secs(5),
            member_count: 5, // Default federation size
            consensus_threshold: 3, // Majority + 1 for 5 members
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_memory_per_actor: 1024 * 1024 * 1024, // 1GB per actor
            max_cpu_usage: 80.0, // 80% CPU threshold
            performance_monitoring: true,
            metrics_interval: Duration::from_secs(10),
            auto_optimization: false, // Disabled by default for predictability
        }
    }
}

impl ActorSystemConfig {
    /// Create development configuration with relaxed timeouts
    pub fn development() -> Self {
        let mut config = Self::default();
        
        // Relaxed timeouts for development
        config.startup_timeout = Duration::from_secs(30);
        config.shutdown_timeout = Duration::from_secs(60);
        config.message_timeout = Duration::from_secs(60);
        
        // Increased logging and monitoring
        config.health_check_interval = Duration::from_secs(5);
        config.metrics_collection_interval = Duration::from_secs(1);
        
        // Smaller mailboxes for development
        config.default_mailbox_capacity = 1000;
        config.high_priority_mailbox_capacity = 5000;
        
        // Enable all features for development
        config.feature_flags.insert("actor_system".to_string(), true);
        config.feature_flags.insert("enhanced_logging".to_string(), true);
        config.feature_flags.insert("development_mode".to_string(), true);
        
        config
    }
    
    /// Create production configuration with strict timeouts
    pub fn production() -> Self {
        let mut config = Self::default();
        
        // Strict production timeouts
        config.startup_timeout = Duration::from_secs(120);
        config.shutdown_timeout = Duration::from_secs(180);
        config.message_timeout = Duration::from_secs(10);
        
        // Optimized for production load
        config.default_mailbox_capacity = 50000;
        config.high_priority_mailbox_capacity = 100000;
        config.mailbox_overflow_strategy = MailboxOverflowStrategy::Backpressure;
        
        // Production health monitoring
        config.health_check_interval = Duration::from_secs(30);
        config.metrics_collection_interval = Duration::from_secs(10);
        
        // Performance optimization enabled
        config.performance_config.auto_optimization = true;
        
        // Conservative feature flags for production
        config.feature_flags.insert("actor_system".to_string(), true);
        config.feature_flags.insert("enhanced_logging".to_string(), false);
        config.feature_flags.insert("development_mode".to_string(), false);
        
        config
    }
    
    /// Get configuration for a specific actor type
    pub fn get_actor_config(&self, actor_type: &str) -> Option<&ActorSpecificConfig> {
        self.actor_configs.get(actor_type)
    }
    
    /// Set configuration for a specific actor type
    pub fn set_actor_config(&mut self, actor_type: String, config: ActorSpecificConfig) {
        self.actor_configs.insert(actor_type, config);
    }
    
    /// Validate configuration for consistency and correctness
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate mailbox capacities
        if self.default_mailbox_capacity == 0 {
            return Err(ConfigError::InvalidMailboxCapacity(self.default_mailbox_capacity));
        }
        
        if self.high_priority_mailbox_capacity < self.default_mailbox_capacity {
            return Err(ConfigError::Validation(
                "High priority mailbox capacity must be >= default capacity".to_string()
            ));
        }
        
        // Validate timeouts
        if self.startup_timeout.is_zero() {
            return Err(ConfigError::InvalidTimeout(self.startup_timeout));
        }
        
        if self.shutdown_timeout.is_zero() {
            return Err(ConfigError::InvalidTimeout(self.shutdown_timeout));
        }
        
        // Validate blockchain configuration
        self.validate_blockchain_config()?;
        
        // Validate actor configurations
        for (actor_type, config) in &self.actor_configs {
            self.validate_actor_config(actor_type, config)?;
        }
        
        Ok(())
    }
    
    /// Validate blockchain-specific configuration
    fn validate_blockchain_config(&self) -> Result<(), ConfigError> {
        let config = &self.blockchain_integration;
        
        // Validate block interval (should be close to 2 seconds for Alys)
        if config.block_interval < Duration::from_millis(1000) 
            || config.block_interval > Duration::from_secs(10) {
            return Err(ConfigError::BlockchainConfig(
                format!("Block interval {:?} outside acceptable range (1s-10s)", config.block_interval)
            ));
        }
        
        // Validate federation configuration
        if config.federation_config.consensus_threshold > config.federation_config.member_count {
            return Err(ConfigError::BlockchainConfig(
                "Consensus threshold cannot exceed member count".to_string()
            ));
        }
        
        // Validate peg-in confirmations (should be reasonable for Bitcoin)
        if config.peg_in_confirmations == 0 || config.peg_in_confirmations > 144 {
            return Err(ConfigError::BlockchainConfig(
                format!("Invalid peg-in confirmations: {}", config.peg_in_confirmations)
            ));
        }
        
        Ok(())
    }
    
    /// Validate actor-specific configuration
    fn validate_actor_config(&self, actor_type: &str, config: &ActorSpecificConfig) -> Result<(), ConfigError> {
        // Validate dependencies don't create cycles
        if config.dependencies.contains(&actor_type.to_string()) {
            return Err(ConfigError::Validation(
                format!("Actor {} cannot depend on itself", actor_type)
            ));
        }
        
        // Validate health check configuration if present
        if let Some(health_config) = &config.health_check_config {
            if health_config.interval.is_zero() {
                return Err(ConfigError::InvalidTimeout(health_config.interval));
            }
            
            if health_config.failure_threshold == 0 {
                return Err(ConfigError::Validation(
                    "Health check failure threshold must be > 0".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    /// Default actor configurations for Alys blockchain components
    fn default_actor_configs() -> HashMap<String, ActorSpecificConfig> {
        let mut configs = HashMap::new();
        
        // Critical blockchain actors
        configs.insert("ChainActor".to_string(), ActorSpecificConfig {
            restart_strategy: Some(AlysRestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(30),
                multiplier: 2.0,
                max_restarts: Some(10),
            }),
            mailbox_capacity: Some(100000),
            priority: ActorPriority::Critical,
            dependencies: vec!["EngineActor".to_string(), "StorageActor".to_string()],
            health_check_config: Some(HealthCheckConfig {
                interval: Duration::from_secs(5),
                timeout: Duration::from_secs(2),
                failure_threshold: 3,
                detailed_reporting: true,
            }),
        });
        
        configs.insert("EngineActor".to_string(), ActorSpecificConfig {
            restart_strategy: Some(AlysRestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(50),
                max_delay: Duration::from_secs(15),
                multiplier: 1.5,
                max_restarts: Some(15),
            }),
            mailbox_capacity: Some(50000),
            priority: ActorPriority::Critical,
            dependencies: vec!["StorageActor".to_string()],
            health_check_config: Some(HealthCheckConfig {
                interval: Duration::from_secs(3),
                timeout: Duration::from_secs(1),
                failure_threshold: 5,
                detailed_reporting: true,
            }),
        });
        
        // High priority actors
        configs.insert("BridgeActor".to_string(), ActorSpecificConfig {
            restart_strategy: Some(AlysRestartStrategy::FixedDelay {
                delay: Duration::from_secs(1),
                max_restarts: Some(20),
            }),
            mailbox_capacity: Some(25000),
            priority: ActorPriority::High,
            dependencies: vec!["ChainActor".to_string()],
            health_check_config: Some(HealthCheckConfig {
                interval: Duration::from_secs(10),
                timeout: Duration::from_secs(5),
                failure_threshold: 2,
                detailed_reporting: true,
            }),
        });
        
        configs.insert("AuxPowMinerActor".to_string(), ActorSpecificConfig {
            restart_strategy: Some(AlysRestartStrategy::Always),
            mailbox_capacity: Some(15000),
            priority: ActorPriority::High,
            dependencies: vec!["ChainActor".to_string()],
            health_check_config: Some(HealthCheckConfig {
                interval: Duration::from_secs(15),
                timeout: Duration::from_secs(3),
                failure_threshold: 3,
                detailed_reporting: false,
            }),
        });
        
        // Background actors
        configs.insert("HealthMonitor".to_string(), ActorSpecificConfig {
            restart_strategy: Some(AlysRestartStrategy::Always),
            mailbox_capacity: Some(1000),
            priority: ActorPriority::Background,
            dependencies: vec![],
            health_check_config: None, // Health monitor doesn't need health checks
        });
        
        configs
    }
    
    /// Default feature flags for gradual migration
    fn default_feature_flags() -> HashMap<String, bool> {
        let mut flags = HashMap::new();
        
        // Core system flags
        flags.insert("actor_system".to_string(), true);
        flags.insert("supervision_enabled".to_string(), true);
        flags.insert("health_monitoring".to_string(), true);
        flags.insert("metrics_collection".to_string(), true);
        
        // Migration flags
        flags.insert("legacy_compatibility".to_string(), true);
        flags.insert("gradual_migration".to_string(), true);
        
        // Development and debugging
        flags.insert("enhanced_logging".to_string(), false);
        flags.insert("development_mode".to_string(), false);
        flags.insert("debug_supervision".to_string(), false);
        
        // Performance features
        flags.insert("auto_optimization".to_string(), false);
        flags.insert("performance_monitoring".to_string(), true);
        
        flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config_validation() {
        let config = ActorSystemConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_development_config() {
        let config = ActorSystemConfig::development();
        assert!(config.validate().is_ok());
        assert!(config.feature_flags["development_mode"]);
        assert_eq!(config.default_mailbox_capacity, 1000);
    }
    
    #[test]
    fn test_production_config() {
        let config = ActorSystemConfig::production();
        assert!(config.validate().is_ok());
        assert!(!config.feature_flags["development_mode"]);
        assert_eq!(config.default_mailbox_capacity, 50000);
    }
    
    #[test]
    fn test_invalid_mailbox_capacity() {
        let mut config = ActorSystemConfig::default();
        config.default_mailbox_capacity = 0;
        
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::InvalidMailboxCapacity(0)
        ));
    }
    
    #[test]
    fn test_invalid_blockchain_config() {
        let mut config = ActorSystemConfig::default();
        config.blockchain_integration.block_interval = Duration::from_millis(100);
        
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::BlockchainConfig(_)
        ));
    }
    
    #[test]
    fn test_actor_config_retrieval() {
        let config = ActorSystemConfig::default();
        
        let chain_config = config.get_actor_config("ChainActor");
        assert!(chain_config.is_some());
        assert_eq!(chain_config.unwrap().priority, ActorPriority::Critical);
        
        let unknown_config = config.get_actor_config("UnknownActor");
        assert!(unknown_config.is_none());
    }
    
    #[test]
    fn test_federation_config_validation() {
        let mut config = ActorSystemConfig::default();
        config.blockchain_integration.federation_config.consensus_threshold = 10;
        config.blockchain_integration.federation_config.member_count = 5;
        
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::BlockchainConfig(_)
        ));
    }
}