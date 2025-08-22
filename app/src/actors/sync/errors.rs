//! Comprehensive error types for SyncActor operations
//!
//! This module defines all error types that can occur during synchronization operations,
//! including network errors, consensus failures, governance stream issues, and 
//! federation-specific error conditions in the Alys sidechain architecture.

use thiserror::Error;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use crate::types::*;

/// Result type for sync operations
pub type SyncResult<T> = Result<T, SyncError>;

/// Comprehensive error types for SyncActor operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum SyncError {
    /// Configuration errors
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    /// Network-related errors
    #[error("Network error: {message}, peer: {peer_id:?}")]
    Network { 
        message: String, 
        peer_id: Option<PeerId>,
        recoverable: bool,
    },

    /// Peer management errors
    #[error("Peer error: {message}, peer: {peer_id}")]
    Peer { 
        message: String, 
        peer_id: PeerId,
        peer_score: f64,
    },

    /// Block validation errors
    #[error("Block validation failed: {block_hash}, reason: {reason}")]
    BlockValidation { 
        block_hash: BlockHash, 
        reason: String,
        block_height: u64,
    },

    /// Consensus-related errors specific to Alys federated PoA
    #[error("Consensus error: {message}, slot: {slot:?}")]
    Consensus { 
        message: String, 
        slot: Option<u64>,
        federation_signature_missing: bool,
    },

    /// Governance stream errors for Anduro integration
    #[error("Governance stream error: {message}, stream_id: {stream_id:?}")]
    GovernanceStream { 
        message: String, 
        stream_id: Option<String>,
        retry_after: Option<Duration>,
    },

    /// Federation-specific errors
    #[error("Federation error: {message}, node_id: {node_id:?}")]
    Federation { 
        message: String, 
        node_id: Option<String>,
        authority_count: u32,
    },

    /// Merged mining and auxiliary PoW errors
    #[error("Mining error: {message}, height: {height:?}")]
    Mining { 
        message: String, 
        height: Option<u64>,
        blocks_without_pow: u64,
    },

    /// Checkpoint system errors
    #[error("Checkpoint error: {checkpoint_id}, reason: {reason}")]
    Checkpoint { 
        checkpoint_id: String, 
        reason: String,
        recovery_possible: bool,
    },

    /// Storage and persistence errors
    #[error("Storage error: {operation}, reason: {reason}")]
    Storage { 
        operation: String, 
        reason: String,
        disk_space_available: Option<u64>,
    },

    /// Resource exhaustion errors
    #[error("Resource exhausted: {resource}, limit: {limit}, current: {current}")]
    ResourceExhausted { 
        resource: String, 
        limit: u64, 
        current: u64,
        recovery_strategy: Option<String>,
    },

    /// Timeout errors with context
    #[error("Timeout: {operation}, duration: {timeout:?}, context: {context:?}")]
    Timeout { 
        operation: String, 
        timeout: Duration,
        context: Option<String>,
    },

    /// Actor system errors
    #[error("Actor system error: {message}, actor_id: {actor_id:?}")]
    ActorSystem { 
        message: String, 
        actor_id: Option<String>,
        supervision_strategy: Option<String>,
    },

    /// Sync state transition errors
    #[error("Invalid state transition: from {from:?} to {to:?}, reason: {reason}")]
    InvalidStateTransition { 
        from: String, 
        to: String, 
        reason: String,
    },

    /// Protocol version mismatch errors
    #[error("Protocol mismatch: local {local_version}, peer {peer_version}, peer_id: {peer_id}")]
    ProtocolMismatch { 
        local_version: u32, 
        peer_version: u32, 
        peer_id: PeerId,
    },

    /// Serialization/deserialization errors
    #[error("Serialization error: {message}, data_type: {data_type}")]
    Serialization { 
        message: String, 
        data_type: String,
    },

    /// Cryptographic errors (signatures, hashes, etc.)
    #[error("Cryptographic error: {message}, operation: {operation}")]
    Cryptographic { 
        message: String, 
        operation: String,
    },

    /// Network partition detection and recovery errors
    #[error("Network partition: {message}, isolated_peers: {isolated_peers}, duration: {duration:?}")]
    NetworkPartition { 
        message: String, 
        isolated_peers: Vec<PeerId>,
        duration: Duration,
        recovery_strategy: PartitionRecoveryStrategy,
    },

    /// Performance degradation errors
    #[error("Performance degraded: {metric} below threshold, current: {current}, threshold: {threshold}")]
    Performance { 
        metric: String, 
        current: f64, 
        threshold: f64,
        impact_assessment: String,
    },

    /// Security-related errors
    #[error("Security violation: {message}, severity: {severity}, source: {source:?}")]
    Security { 
        message: String, 
        severity: SecuritySeverity,
        source: Option<PeerId>,
        mitigation_applied: bool,
    },

    /// Rate limiting errors
    #[error("Rate limit exceeded: {operation}, current_rate: {current_rate}, limit: {limit}")]
    RateLimited { 
        operation: String, 
        current_rate: f64, 
        limit: f64,
        reset_time: SystemTime,
    },

    /// Generic internal errors
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Security severity levels for sync operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Network partition recovery strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionRecoveryStrategy {
    /// Wait for connectivity to be restored
    WaitForRecovery,
    /// Attempt to reconnect to known peers
    ReconnectPeers,
    /// Use checkpoint recovery
    CheckpointRecovery,
    /// Fallback to governance stream
    GovernanceStreamFallback,
    /// Manual intervention required
    ManualIntervention,
}

impl SyncError {
    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            SyncError::Network { recoverable, .. } => *recoverable,
            SyncError::Checkpoint { recovery_possible, .. } => *recovery_possible,
            SyncError::GovernanceStream { retry_after, .. } => retry_after.is_some(),
            SyncError::ResourceExhausted { recovery_strategy, .. } => recovery_strategy.is_some(),
            SyncError::Timeout { .. } => true, // Timeouts are usually recoverable
            SyncError::Performance { .. } => true, // Performance issues can often be mitigated
            SyncError::RateLimited { .. } => true, // Rate limits are temporary
            SyncError::NetworkPartition { .. } => true, // Partitions can be recovered from
            
            // Non-recoverable errors
            SyncError::Configuration { .. } => false,
            SyncError::InvalidStateTransition { .. } => false,
            SyncError::ProtocolMismatch { .. } => false,
            SyncError::Cryptographic { .. } => false,
            SyncError::Security { severity: SecuritySeverity::Critical, .. } => false,
            
            // Other errors are potentially recoverable depending on context
            _ => true,
        }
    }

    /// Get the retry delay for recoverable errors
    pub fn retry_delay(&self) -> Option<Duration> {
        match self {
            SyncError::GovernanceStream { retry_after, .. } => *retry_after,
            SyncError::RateLimited { reset_time, .. } => {
                reset_time.duration_since(SystemTime::now()).ok()
            }
            SyncError::Network { .. } => Some(Duration::from_secs(5)),
            SyncError::Peer { .. } => Some(Duration::from_secs(30)),
            SyncError::Timeout { .. } => Some(Duration::from_secs(10)),
            SyncError::Performance { .. } => Some(Duration::from_secs(60)),
            SyncError::NetworkPartition { .. } => Some(Duration::from_secs(120)),
            _ => None,
        }
    }

    /// Get the error severity for monitoring and alerting
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            SyncError::Security { severity: SecuritySeverity::Critical, .. } => ErrorSeverity::Critical,
            SyncError::Configuration { .. } => ErrorSeverity::Critical,
            SyncError::InvalidStateTransition { .. } => ErrorSeverity::Critical,
            SyncError::Cryptographic { .. } => ErrorSeverity::Critical,
            
            SyncError::Federation { .. } => ErrorSeverity::High,
            SyncError::Consensus { .. } => ErrorSeverity::High,
            SyncError::Mining { blocks_without_pow, .. } if *blocks_without_pow > 5000 => ErrorSeverity::High,
            SyncError::NetworkPartition { .. } => ErrorSeverity::High,
            SyncError::Security { severity: SecuritySeverity::High, .. } => ErrorSeverity::High,
            
            SyncError::BlockValidation { .. } => ErrorSeverity::Medium,
            SyncError::Checkpoint { .. } => ErrorSeverity::Medium,
            SyncError::GovernanceStream { .. } => ErrorSeverity::Medium,
            SyncError::Storage { .. } => ErrorSeverity::Medium,
            SyncError::ResourceExhausted { .. } => ErrorSeverity::Medium,
            SyncError::Performance { .. } => ErrorSeverity::Medium,
            SyncError::Security { severity: SecuritySeverity::Medium, .. } => ErrorSeverity::Medium,
            
            _ => ErrorSeverity::Low,
        }
    }

    /// Convert error to a format suitable for metrics and monitoring
    pub fn to_metric_labels(&self) -> std::collections::HashMap<String, String> {
        let mut labels = std::collections::HashMap::new();
        
        labels.insert("error_type".to_string(), self.error_type());
        labels.insert("severity".to_string(), format!("{:?}", self.severity()));
        labels.insert("recoverable".to_string(), self.is_recoverable().to_string());
        
        // Add specific context based on error type
        match self {
            SyncError::Network { peer_id, .. } => {
                if let Some(peer) = peer_id {
                    labels.insert("peer_id".to_string(), peer.to_string());
                }
            }
            SyncError::Consensus { slot, federation_signature_missing, .. } => {
                if let Some(s) = slot {
                    labels.insert("slot".to_string(), s.to_string());
                }
                labels.insert("federation_signature_missing".to_string(), federation_signature_missing.to_string());
            }
            SyncError::Mining { blocks_without_pow, .. } => {
                labels.insert("blocks_without_pow".to_string(), blocks_without_pow.to_string());
            }
            SyncError::Federation { authority_count, .. } => {
                labels.insert("authority_count".to_string(), authority_count.to_string());
            }
            _ => {}
        }
        
        labels
    }

    /// Get the error type as a string for categorization
    pub fn error_type(&self) -> String {
        match self {
            SyncError::Configuration { .. } => "configuration",
            SyncError::Network { .. } => "network",
            SyncError::Peer { .. } => "peer",
            SyncError::BlockValidation { .. } => "block_validation",
            SyncError::Consensus { .. } => "consensus",
            SyncError::GovernanceStream { .. } => "governance_stream",
            SyncError::Federation { .. } => "federation",
            SyncError::Mining { .. } => "mining",
            SyncError::Checkpoint { .. } => "checkpoint",
            SyncError::Storage { .. } => "storage",
            SyncError::ResourceExhausted { .. } => "resource_exhausted",
            SyncError::Timeout { .. } => "timeout",
            SyncError::ActorSystem { .. } => "actor_system",
            SyncError::InvalidStateTransition { .. } => "invalid_state_transition",
            SyncError::ProtocolMismatch { .. } => "protocol_mismatch",
            SyncError::Serialization { .. } => "serialization",
            SyncError::Cryptographic { .. } => "cryptographic",
            SyncError::NetworkPartition { .. } => "network_partition",
            SyncError::Performance { .. } => "performance",
            SyncError::Security { .. } => "security",
            SyncError::RateLimited { .. } => "rate_limited",
            SyncError::Internal { .. } => "internal",
        }.to_string()
    }
}

/// Error severity levels for monitoring and alerting
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Error context for enhanced debugging and monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub error_id: String,
    pub timestamp: SystemTime,
    pub actor_id: Option<String>,
    pub operation: String,
    pub attempt_count: u32,
    pub correlation_id: Option<String>,
    pub additional_metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(operation: String) -> Self {
        Self {
            error_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            actor_id: None,
            operation,
            attempt_count: 1,
            correlation_id: None,
            additional_metadata: std::collections::HashMap::new(),
        }
    }

    /// Add metadata to the error context
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.additional_metadata.insert(key, value);
        self
    }

    /// Set the actor ID for the error context
    pub fn with_actor_id(mut self, actor_id: String) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Set the correlation ID for tracing related operations
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Increment the attempt count for retry scenarios
    pub fn increment_attempt(mut self) -> Self {
        self.attempt_count += 1;
        self
    }
}

/// Error aggregation for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncErrorBatch {
    pub errors: Vec<(SyncError, ErrorContext)>,
    pub success_count: usize,
    pub failure_count: usize,
    pub critical_failures: usize,
}

impl SyncErrorBatch {
    /// Create a new empty error batch
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            success_count: 0,
            failure_count: 0,
            critical_failures: 0,
        }
    }

    /// Add an error to the batch
    pub fn add_error(&mut self, error: SyncError, context: ErrorContext) {
        if error.severity() == ErrorSeverity::Critical {
            self.critical_failures += 1;
        }
        self.failure_count += 1;
        self.errors.push((error, context));
    }

    /// Add a success to the batch
    pub fn add_success(&mut self) {
        self.success_count += 1;
    }

    /// Check if the batch has any critical failures
    pub fn has_critical_failures(&self) -> bool {
        self.critical_failures > 0
    }

    /// Get the overall success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            1.0
        } else {
            self.success_count as f64 / total as f64
        }
    }

    /// Get errors grouped by type
    pub fn errors_by_type(&self) -> std::collections::HashMap<String, Vec<&SyncError>> {
        let mut grouped = std::collections::HashMap::new();
        
        for (error, _) in &self.errors {
            let error_type = error.error_type();
            grouped.entry(error_type).or_insert_with(Vec::new).push(error);
        }
        
        grouped
    }
}

use std::time::SystemTime;