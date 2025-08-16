//! Sync engine configuration

use super::*;
use std::time::Duration;

/// Sync engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Enable sync engine
    pub enabled: bool,
    
    /// Parallel download settings
    pub parallel_downloads: ParallelDownloadConfig,
    
    /// Checkpoint settings
    pub checkpoints: CheckpointConfig,
    
    /// Sync timeouts
    pub timeouts: SyncTimeouts,
    
    /// Performance settings
    pub performance: SyncPerformanceConfig,
}

/// Parallel download configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelDownloadConfig {
    /// Maximum concurrent downloads
    pub max_concurrent: usize,
    
    /// Blocks per download batch
    pub batch_size: usize,
    
    /// Download timeout per batch
    pub batch_timeout: Duration,
    
    /// Maximum retries per batch
    pub max_retries: u32,
    
    /// Retry delay
    pub retry_delay: Duration,
}

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Checkpoint interval in blocks
    pub interval: u64,
    
    /// Enable checkpoint validation
    pub validation: bool,
    
    /// Checkpoint storage path
    pub storage_path: String,
    
    /// Maximum checkpoints to keep
    pub max_checkpoints: u32,
}

/// Sync timeouts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTimeouts {
    /// Initial sync timeout
    pub initial_sync: Duration,
    
    /// Block request timeout
    pub block_request: Duration,
    
    /// Peer response timeout
    pub peer_response: Duration,
    
    /// Sync completion timeout
    pub completion: Duration,
}

/// Sync performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPerformanceConfig {
    /// Memory buffer size in MB
    pub buffer_size_mb: u64,
    
    /// Enable compression
    pub compression: bool,
    
    /// Enable parallel validation
    pub parallel_validation: bool,
    
    /// Validation thread count
    pub validation_threads: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            parallel_downloads: ParallelDownloadConfig::default(),
            checkpoints: CheckpointConfig::default(),
            timeouts: SyncTimeouts::default(),
            performance: SyncPerformanceConfig::default(),
        }
    }
}

impl Default for ParallelDownloadConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 8,
            batch_size: 100,
            batch_timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
        }
    }
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            interval: 1000,
            validation: true,
            storage_path: "./checkpoints".to_string(),
            max_checkpoints: 10,
        }
    }
}

impl Default for SyncTimeouts {
    fn default() -> Self {
        Self {
            initial_sync: Duration::from_secs(600),
            block_request: Duration::from_secs(10),
            peer_response: Duration::from_secs(30),
            completion: Duration::from_secs(120),
        }
    }
}

impl Default for SyncPerformanceConfig {
    fn default() -> Self {
        Self {
            buffer_size_mb: 128,
            compression: true,
            parallel_validation: true,
            validation_threads: num_cpus::get(),
        }
    }
}

impl Validate for SyncConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.parallel_downloads.max_concurrent == 0 {
            return Err(ConfigError::ValidationError {
                field: "sync.parallel_downloads.max_concurrent".to_string(),
                reason: "Max concurrent downloads must be greater than 0".to_string(),
            });
        }
        
        if self.parallel_downloads.batch_size == 0 {
            return Err(ConfigError::ValidationError {
                field: "sync.parallel_downloads.batch_size".to_string(),
                reason: "Batch size must be greater than 0".to_string(),
            });
        }
        
        Ok(())
    }
}