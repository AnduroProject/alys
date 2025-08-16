//! Synchronization engine error types

use thiserror::Error;

/// Result type for sync operations
pub type SyncResult<T> = Result<T, SyncError>;

/// Synchronization engine errors
#[derive(Debug, Error, Clone)]
pub enum SyncError {
    /// Network-related errors
    #[error("Network error: {message}")]
    Network { message: String },
    
    /// Peer-related errors
    #[error("Peer error {peer_id}: {message}")]
    Peer { peer_id: String, message: String },
    
    /// Block validation errors
    #[error("Block validation failed for {block_hash}: {reason}")]
    BlockValidation { block_hash: String, reason: String },
    
    /// State verification errors
    #[error("State verification failed: {reason}")]
    StateVerification { reason: String },
    
    /// Download errors
    #[error("Download failed: {reason}")]
    DownloadFailed { reason: String },
    
    /// Storage errors
    #[error("Storage error: {operation} - {reason}")]
    Storage { operation: String, reason: String },
    
    /// Protocol errors
    #[error("Protocol error: {protocol} - {reason}")]
    Protocol { protocol: String, reason: String },
    
    /// Sync timeout
    #[error("Sync operation timed out: {operation} after {timeout:?}")]
    Timeout { operation: String, timeout: std::time::Duration },
    
    /// Invalid configuration
    #[error("Invalid configuration: {parameter} - {reason}")]
    InvalidConfig { parameter: String, reason: String },
    
    /// Insufficient peers
    #[error("Insufficient peers: need {required}, have {available}")]
    InsufficientPeers { required: usize, available: usize },
    
    /// Checkpoint verification failed
    #[error("Checkpoint verification failed: {checkpoint} - {reason}")]
    CheckpointFailed { checkpoint: String, reason: String },
    
    /// Fork detection
    #[error("Fork detected at block {block_number}: local={local_hash}, peer={peer_hash}")]
    ForkDetected { 
        block_number: u64, 
        local_hash: String, 
        peer_hash: String 
    },
    
    /// Sync already in progress
    #[error("Sync already in progress: {sync_type}")]
    SyncInProgress { sync_type: String },
    
    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
    
    /// Internal error
    #[error("Internal error: {message}")]
    Internal { message: String },
    
    /// Aborted by user
    #[error("Sync aborted: {reason}")]
    Aborted { reason: String },
    
    /// Consensus error
    #[error("Consensus error: {reason}")]
    Consensus { reason: String },
    
    /// Serialization error
    #[error("Serialization error: {reason}")]
    Serialization { reason: String },
    
    /// Database corruption
    #[error("Database corruption detected: {details}")]
    DatabaseCorruption { details: String },
    
    /// Version mismatch
    #[error("Version mismatch: local={local_version}, peer={peer_version}")]
    VersionMismatch { local_version: String, peer_version: String },
}

impl SyncError {
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            SyncError::Network { .. } => true,
            SyncError::Peer { .. } => true,
            SyncError::DownloadFailed { .. } => true,
            SyncError::Timeout { .. } => true,
            SyncError::InsufficientPeers { .. } => true,
            SyncError::ResourceExhausted { .. } => true,
            SyncError::SyncInProgress { .. } => true,
            SyncError::Aborted { .. } => true,
            
            SyncError::BlockValidation { .. } => false,
            SyncError::StateVerification { .. } => false,
            SyncError::Storage { .. } => false,
            SyncError::Protocol { .. } => false,
            SyncError::InvalidConfig { .. } => false,
            SyncError::CheckpointFailed { .. } => false,
            SyncError::ForkDetected { .. } => false,
            SyncError::Internal { .. } => false,
            SyncError::Consensus { .. } => false,
            SyncError::Serialization { .. } => false,
            SyncError::DatabaseCorruption { .. } => false,
            SyncError::VersionMismatch { .. } => false,
        }
    }
    
    /// Check if error should trigger peer penalty
    pub fn should_penalize_peer(&self) -> bool {
        match self {
            SyncError::BlockValidation { .. } => true,
            SyncError::StateVerification { .. } => true,
            SyncError::Protocol { .. } => true,
            SyncError::VersionMismatch { .. } => true,
            _ => false,
        }
    }
    
    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            SyncError::DatabaseCorruption { .. } => ErrorSeverity::Critical,
            SyncError::Internal { .. } => ErrorSeverity::Critical,
            SyncError::InvalidConfig { .. } => ErrorSeverity::Critical,
            
            SyncError::BlockValidation { .. } => ErrorSeverity::High,
            SyncError::StateVerification { .. } => ErrorSeverity::High,
            SyncError::CheckpointFailed { .. } => ErrorSeverity::High,
            SyncError::ForkDetected { .. } => ErrorSeverity::High,
            SyncError::Storage { .. } => ErrorSeverity::High,
            
            SyncError::Network { .. } => ErrorSeverity::Medium,
            SyncError::Peer { .. } => ErrorSeverity::Medium,
            SyncError::DownloadFailed { .. } => ErrorSeverity::Medium,
            SyncError::Protocol { .. } => ErrorSeverity::Medium,
            SyncError::InsufficientPeers { .. } => ErrorSeverity::Medium,
            SyncError::Consensus { .. } => ErrorSeverity::Medium,
            SyncError::VersionMismatch { .. } => ErrorSeverity::Medium,
            
            SyncError::Timeout { .. } => ErrorSeverity::Low,
            SyncError::SyncInProgress { .. } => ErrorSeverity::Low,
            SyncError::ResourceExhausted { .. } => ErrorSeverity::Low,
            SyncError::Aborted { .. } => ErrorSeverity::Low,
            SyncError::Serialization { .. } => ErrorSeverity::Low,
        }
    }
    
    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            SyncError::Network { .. } => "network",
            SyncError::Peer { .. } => "peer",
            SyncError::BlockValidation { .. } => "validation",
            SyncError::StateVerification { .. } => "state",
            SyncError::DownloadFailed { .. } => "download",
            SyncError::Storage { .. } => "storage",
            SyncError::Protocol { .. } => "protocol",
            SyncError::Timeout { .. } => "timeout",
            SyncError::InvalidConfig { .. } => "config",
            SyncError::InsufficientPeers { .. } => "peers",
            SyncError::CheckpointFailed { .. } => "checkpoint",
            SyncError::ForkDetected { .. } => "fork",
            SyncError::SyncInProgress { .. } => "sync",
            SyncError::ResourceExhausted { .. } => "resources",
            SyncError::Internal { .. } => "internal",
            SyncError::Aborted { .. } => "abort",
            SyncError::Consensus { .. } => "consensus",
            SyncError::Serialization { .. } => "serialization",
            SyncError::DatabaseCorruption { .. } => "database",
            SyncError::VersionMismatch { .. } => "version",
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low impact error
    Low,
    /// Medium impact error
    Medium,
    /// High impact error
    High,
    /// Critical system error
    Critical,
}

// Convert from common error types
impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::Storage {
            operation: "io".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for SyncError {
    fn from(err: serde_json::Error) -> Self {
        SyncError::Serialization {
            reason: err.to_string(),
        }
    }
}

impl From<tokio::time::error::Elapsed> for SyncError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        SyncError::Timeout {
            operation: "unknown".to_string(),
            timeout: std::time::Duration::from_secs(0),
        }
    }
}