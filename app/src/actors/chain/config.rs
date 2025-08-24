//! Chain Actor Configuration
//!
//! Configuration structures, defaults, and validation for the ChainActor.
//! This module contains all configuration-related types and provides
//! sensible defaults for different deployment environments.

use std::time::Duration;
use actor_system::SupervisionConfig;
use super::state::FederationConfig;

/// Configuration for ChainActor behavior and performance
#[derive(Debug, Clone)]
pub struct ChainActorConfig {
    /// Slot duration for Aura consensus (default 2 seconds)
    pub slot_duration: Duration,
    
    /// Maximum blocks without PoW before halting
    pub max_blocks_without_pow: u64,
    
    /// Maximum reorg depth allowed
    pub max_reorg_depth: u32,
    
    /// Whether this node is a validator
    pub is_validator: bool,
    
    /// Authority key for block signing
    pub authority_key: Option<SecretKey>,
    
    /// Block production timeout
    pub production_timeout: Duration,
    
    /// Block import timeout
    pub import_timeout: Duration,
    
    /// Validation cache size
    pub validation_cache_size: usize,
    
    /// Maximum pending blocks
    pub max_pending_blocks: usize,
    
    /// Performance targets
    pub performance_targets: PerformanceTargets,
    
    /// Actor supervision configuration
    pub supervision_config: SupervisionConfig,
    
    /// Federation configuration (if this node is part of federation)
    pub federation_config: Option<FederationConfig>,
}

/// Performance targets for monitoring and optimization
#[derive(Debug, Clone)]
pub struct PerformanceTargets {
    /// Maximum block production time (default 500ms)
    pub max_production_time_ms: u64,
    
    /// Maximum block import time (default 100ms)
    pub max_import_time_ms: u64,
    
    /// Maximum validation time (default 50ms)
    pub max_validation_time_ms: u64,
    
    /// Target blocks per second
    pub target_blocks_per_second: f64,
    
    /// Maximum memory usage (MB)
    pub max_memory_mb: u64,
}

/// Environment-specific configuration presets
#[derive(Debug, Clone)]
pub enum ConfigPreset {
    /// Development configuration with relaxed constraints
    Development,
    /// Testnet configuration with moderate constraints
    Testnet,
    /// Production configuration with strict constraints
    Production,
    /// High-performance configuration for powerful hardware
    HighPerformance,
}

impl ChainActorConfig {
    /// Create a new configuration with the given preset
    pub fn from_preset(preset: ConfigPreset) -> Self {
        match preset {
            ConfigPreset::Development => Self::development(),
            ConfigPreset::Testnet => Self::testnet(),
            ConfigPreset::Production => Self::production(),
            ConfigPreset::HighPerformance => Self::high_performance(),
        }
    }
    
    /// Development configuration with relaxed timeouts
    pub fn development() -> Self {
        Self {
            production_timeout: Duration::from_secs(2),
            import_timeout: Duration::from_millis(500),
            max_pending_blocks: 200,
            performance_targets: PerformanceTargets {
                max_production_time_ms: 1000,
                max_import_time_ms: 300,
                max_validation_time_ms: 150,
                target_blocks_per_second: 0.5,
                max_memory_mb: 1024,
            },
            federation_config: None,
            ..Default::default()
        }
    }
    
    /// Testnet configuration with moderate constraints
    pub fn testnet() -> Self {
        Self {
            production_timeout: Duration::from_millis(800),
            import_timeout: Duration::from_millis(200),
            max_pending_blocks: 150,
            performance_targets: PerformanceTargets {
                max_production_time_ms: 700,
                max_import_time_ms: 150,
                max_validation_time_ms: 80,
                target_blocks_per_second: 0.5,
                max_memory_mb: 768,
            },
            federation_config: None,
            ..Default::default()
        }
    }
    
    /// Production configuration with strict constraints
    pub fn production() -> Self {
        Default::default()
    }
    
    /// High-performance configuration for powerful hardware
    pub fn high_performance() -> Self {
        Self {
            production_timeout: Duration::from_millis(300),
            import_timeout: Duration::from_millis(50),
            max_pending_blocks: 50,
            validation_cache_size: 2000,
            performance_targets: PerformanceTargets {
                max_production_time_ms: 250,
                max_import_time_ms: 50,
                max_validation_time_ms: 25,
                target_blocks_per_second: 1.0,
                max_memory_mb: 256,
            },
            federation_config: None,
            ..Default::default()
        }
    }
    
    /// Validate the configuration for consistency and safety
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.slot_duration.as_millis() == 0 {
            return Err(ConfigError::InvalidSlotDuration);
        }
        
        if self.max_blocks_without_pow == 0 {
            return Err(ConfigError::InvalidMaxBlocksWithoutPow);
        }
        
        if self.max_reorg_depth == 0 {
            return Err(ConfigError::InvalidMaxReorgDepth);
        }
        
        if self.validation_cache_size == 0 {
            return Err(ConfigError::InvalidCacheSize);
        }
        
        if self.max_pending_blocks == 0 {
            return Err(ConfigError::InvalidMaxPendingBlocks);
        }
        
        // Validate performance targets
        self.performance_targets.validate()?;
        
        Ok(())
    }
}

impl PerformanceTargets {
    /// Validate performance targets for consistency
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_production_time_ms == 0 {
            return Err(ConfigError::InvalidPerformanceTarget("max_production_time_ms cannot be 0".to_string()));
        }
        
        if self.max_import_time_ms == 0 {
            return Err(ConfigError::InvalidPerformanceTarget("max_import_time_ms cannot be 0".to_string()));
        }
        
        if self.max_validation_time_ms == 0 {
            return Err(ConfigError::InvalidPerformanceTarget("max_validation_time_ms cannot be 0".to_string()));
        }
        
        if self.target_blocks_per_second <= 0.0 {
            return Err(ConfigError::InvalidPerformanceTarget("target_blocks_per_second must be positive".to_string()));
        }
        
        if self.max_memory_mb == 0 {
            return Err(ConfigError::InvalidPerformanceTarget("max_memory_mb cannot be 0".to_string()));
        }
        
        Ok(())
    }
}

impl Default for ChainActorConfig {
    fn default() -> Self {
        Self {
            slot_duration: Duration::from_secs(2),
            max_blocks_without_pow: 10,
            max_reorg_depth: 32,
            is_validator: false,
            authority_key: None,
            production_timeout: Duration::from_millis(500),
            import_timeout: Duration::from_millis(100),
            validation_cache_size: 1000,
            max_pending_blocks: 100,
            performance_targets: PerformanceTargets {
                max_production_time_ms: 500,
                max_import_time_ms: 100,
                max_validation_time_ms: 50,
                target_blocks_per_second: 0.5, // 2 second blocks
                max_memory_mb: 512,
            },
            supervision_config: SupervisionConfig::default(),
            federation_config: None,
        }
    }
}

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid slot duration: must be greater than 0")]
    InvalidSlotDuration,
    #[error("Invalid max blocks without PoW: must be greater than 0")]
    InvalidMaxBlocksWithoutPow,
    #[error("Invalid max reorg depth: must be greater than 0")]
    InvalidMaxReorgDepth,
    #[error("Invalid cache size: must be greater than 0")]
    InvalidCacheSize,
    #[error("Invalid max pending blocks: must be greater than 0")]
    InvalidMaxPendingBlocks,
    #[error("Invalid performance target: {0}")]
    InvalidPerformanceTarget(String),
}

// Temporary placeholder for SecretKey until we import the proper type
use crate::types::SecretKey;