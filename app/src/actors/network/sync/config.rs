//! SyncActor Configuration
//! 
//! Configuration structures for blockchain synchronization including
//! performance tuning, federation constraints, and optimization settings.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Complete synchronization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    // Core sync settings
    pub production_threshold: f64,        // 0.995 (99.5%)
    pub max_parallel_downloads: usize,    // 16 concurrent block downloads
    pub validation_workers: usize,        // 4 validation worker threads
    pub batch_size: usize,               // 256 blocks per batch
    
    // Federation-specific timing constraints
    pub federation_constraints: FederationTimingConfig,
    pub aura_slot_duration: Duration,    // 2 seconds
    pub max_consensus_latency: Duration, // 100ms
    
    // Performance optimization
    pub simd_enabled: bool,             // Hardware-accelerated validation
    pub cache_size: usize,              // 10,000 blocks in memory
    pub memory_pool_size: usize,        // 1GB memory pool for processing
    
    // Checkpoint system
    pub checkpoint_interval: u64,       // Every 100 blocks
    pub checkpoint_retention: u64,      // Keep last 10 checkpoints
    pub compression_enabled: bool,      // Compress checkpoint data
    
    // Network and retry settings
    pub max_retries: u32,              // 3 retries per operation
    pub request_timeout: Duration,      // 30 seconds
    pub health_check_interval: Duration, // 10 seconds
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            production_threshold: 0.995, // 99.5% sync required for block production
            max_parallel_downloads: 16,
            validation_workers: num_cpus::get().min(8), // Cap at 8 workers
            batch_size: 256,
            
            federation_constraints: FederationTimingConfig::default(),
            aura_slot_duration: Duration::from_secs(2),
            max_consensus_latency: Duration::from_millis(100),
            
            simd_enabled: cfg!(feature = "simd"),
            cache_size: 10_000,
            memory_pool_size: 1024 * 1024 * 1024, // 1GB
            
            checkpoint_interval: 100,
            checkpoint_retention: 10,
            compression_enabled: true,
            
            max_retries: 3,
            request_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(10),
        }
    }
}

/// Federation timing configuration for consensus coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationTimingConfig {
    /// Maximum time to wait for federation consensus
    pub consensus_timeout: Duration,
    /// Minimum time between block production attempts  
    pub min_block_interval: Duration,
    /// Maximum time to wait for block validation
    pub validation_timeout: Duration,
    /// Grace period for network propagation
    pub propagation_grace: Duration,
    /// Emergency mode timeout (degraded operation)
    pub emergency_timeout: Duration,
}

impl Default for FederationTimingConfig {
    fn default() -> Self {
        Self {
            consensus_timeout: Duration::from_millis(500),  // 500ms for consensus
            min_block_interval: Duration::from_secs(2),     // 2-second minimum
            validation_timeout: Duration::from_millis(200), // 200ms validation
            propagation_grace: Duration::from_millis(100),  // 100ms propagation
            emergency_timeout: Duration::from_secs(30),     // 30s emergency mode
        }
    }
}

/// Sync operation modes with different characteristics
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SyncMode {
    /// Fast parallel sync with optimized validation (default)
    Fast,
    /// Full validation sync for maximum security
    Full,
    /// Checkpoint-based recovery sync
    Recovery,
    /// Federation-only sync for consensus nodes
    Federation,
    /// Emergency sync mode with reduced validation
    Emergency,
}

impl Default for SyncMode {
    fn default() -> Self {
        SyncMode::Fast
    }
}

impl SyncMode {
    /// Get validation workers for this sync mode
    pub fn validation_workers(&self, base_workers: usize) -> usize {
        match self {
            SyncMode::Fast => base_workers,
            SyncMode::Full => base_workers * 2,      // More workers for full validation
            SyncMode::Recovery => base_workers / 2,   // Fewer workers for recovery
            SyncMode::Federation => base_workers,
            SyncMode::Emergency => base_workers / 4,  // Minimal workers for emergency
        }
    }

    /// Get batch size for this sync mode
    pub fn batch_size(&self, base_batch_size: usize) -> usize {
        match self {
            SyncMode::Fast => base_batch_size,
            SyncMode::Full => base_batch_size / 2,    // Smaller batches for full validation
            SyncMode::Recovery => base_batch_size * 2, // Larger batches for recovery
            SyncMode::Federation => base_batch_size,
            SyncMode::Emergency => base_batch_size / 4, // Small batches for emergency
        }
    }

    /// Check if this mode requires full validation
    pub fn requires_full_validation(&self) -> bool {
        matches!(self, SyncMode::Full)
    }

    /// Check if this mode supports checkpoints
    pub fn supports_checkpoints(&self) -> bool {
        !matches!(self, SyncMode::Emergency)
    }
}

/// Performance optimization settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    /// Enable SIMD-optimized hash calculations
    pub simd_hashing: bool,
    /// Enable parallel signature verification
    pub parallel_signatures: bool,
    /// Enable memory pool for zero-copy operations
    pub memory_pools: bool,
    /// Enable adaptive batch sizing based on performance
    pub adaptive_batching: bool,
    /// Enable machine learning for peer selection
    pub ml_optimization: bool,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            simd_hashing: cfg!(feature = "simd"),
            parallel_signatures: true,
            memory_pools: true,
            adaptive_batching: true,
            ml_optimization: false, // Disabled by default
        }
    }
}

/// Memory management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Block cache size
    pub block_cache_size: usize,
    /// State cache size
    pub state_cache_size: usize,
    /// Memory pool block size
    pub pool_block_size: usize,
    /// Garbage collection threshold
    pub gc_threshold: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 2 * 1024 * 1024 * 1024, // 2GB
            block_cache_size: 10_000,
            state_cache_size: 5_000,
            pool_block_size: 64 * 1024, // 64KB blocks
            gc_threshold: 0.8, // GC when 80% full
        }
    }
}

impl SyncConfig {
    /// Create a configuration optimized for federation nodes
    pub fn federation() -> Self {
        let mut config = Self::default();
        config.production_threshold = 0.990; // Slightly lower threshold for federation
        config.max_parallel_downloads = 8;   // Conservative for stability
        config.validation_workers = 2;       // Lower resource usage
        config.federation_constraints.consensus_timeout = Duration::from_millis(300);
        config
    }

    /// Create a configuration optimized for high-performance sync
    pub fn high_performance() -> Self {
        let mut config = Self::default();
        config.max_parallel_downloads = 32;  // Aggressive downloading
        config.validation_workers = num_cpus::get(); // Use all cores
        config.batch_size = 512;             // Larger batches
        config.simd_enabled = true;          // Enable SIMD if available
        config.memory_pool_size = 4 * 1024 * 1024 * 1024; // 4GB memory pool
        config
    }

    /// Create a configuration optimized for resource-constrained environments
    pub fn lightweight() -> Self {
        let mut config = Self::default();
        config.max_parallel_downloads = 4;
        config.validation_workers = 1;
        config.batch_size = 64;
        config.cache_size = 1_000;
        config.memory_pool_size = 256 * 1024 * 1024; // 256MB
        config.compression_enabled = true;
        config
    }

    /// Validate configuration for consistency
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.production_threshold) {
            return Err("production_threshold must be between 0.0 and 1.0".to_string());
        }

        if self.production_threshold < 0.95 {
            return Err("production_threshold should be at least 95% for security".to_string());
        }

        if self.max_parallel_downloads == 0 {
            return Err("max_parallel_downloads must be greater than 0".to_string());
        }

        if self.validation_workers == 0 {
            return Err("validation_workers must be greater than 0".to_string());
        }

        if self.batch_size == 0 {
            return Err("batch_size must be greater than 0".to_string());
        }

        if self.checkpoint_interval == 0 {
            return Err("checkpoint_interval must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validation() {
        let config = SyncConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.production_threshold, 0.995);
        assert!(config.production_threshold >= 0.95);
    }

    #[test]
    fn federation_config() {
        let config = SyncConfig::federation();
        assert!(config.validate().is_ok());
        assert_eq!(config.production_threshold, 0.990);
        assert_eq!(config.max_parallel_downloads, 8);
        assert_eq!(config.validation_workers, 2);
    }

    #[test]
    fn sync_mode_characteristics() {
        assert_eq!(SyncMode::Fast.validation_workers(4), 4);
        assert_eq!(SyncMode::Full.validation_workers(4), 8);
        assert_eq!(SyncMode::Recovery.validation_workers(4), 2);
        assert_eq!(SyncMode::Emergency.validation_workers(4), 1);

        assert!(SyncMode::Full.requires_full_validation());
        assert!(!SyncMode::Fast.requires_full_validation());
        
        assert!(SyncMode::Fast.supports_checkpoints());
        assert!(!SyncMode::Emergency.supports_checkpoints());
    }

    #[test]
    fn config_validation_errors() {
        let mut config = SyncConfig::default();
        
        config.production_threshold = 1.5;
        assert!(config.validate().is_err());
        
        config.production_threshold = 0.90; // Too low
        assert!(config.validate().is_err());
        
        config.production_threshold = 0.995;
        config.max_parallel_downloads = 0;
        assert!(config.validate().is_err());
    }
}