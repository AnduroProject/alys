//! Federation system error types

use thiserror::Error;

/// Result type for federation operations
pub type FederationResult<T> = Result<T, FederationError>;

/// Federation system errors
#[derive(Debug, Error, Clone)]
pub enum FederationError {
    /// Governance-related errors
    #[error("Governance error: {message}")]
    Governance { message: String },
    
    /// Key management errors
    #[error("Key management error: {operation} - {reason}")]
    KeyManagement { operation: String, reason: String },
    
    /// Signature errors
    #[error("Signature error: {signature_type} - {reason}")]
    Signature { signature_type: String, reason: String },
    
    /// Bitcoin integration errors
    #[error("Bitcoin error: {operation} - {reason}")]
    Bitcoin { operation: String, reason: String },
    
    /// UTXO management errors
    #[error("UTXO error: {utxo_id} - {reason}")]
    Utxo { utxo_id: String, reason: String },
    
    /// Transaction building errors
    #[error("Transaction error: {tx_type} - {reason}")]
    Transaction { tx_type: String, reason: String },
    
    /// Bridge operation errors
    #[error("Bridge error: {operation} - {reason}")]
    Bridge { operation: String, reason: String },
    
    /// Protocol errors
    #[error("Protocol error: {protocol} version {version} - {reason}")]
    Protocol { protocol: String, version: String, reason: String },
    
    /// Network communication errors
    #[error("Network error: {peer_id} - {reason}")]
    Network { peer_id: String, reason: String },
    
    /// Consensus errors
    #[error("Consensus error: {reason}")]
    Consensus { reason: String },
    
    /// Configuration errors
    #[error("Configuration error: {parameter} - {reason}")]
    Configuration { parameter: String, reason: String },
    
    /// Insufficient signatures
    #[error("Insufficient signatures: need {required}, have {collected}")]
    InsufficientSignatures { required: usize, collected: usize },
    
    /// Invalid threshold
    #[error("Invalid threshold: {threshold} of {total} members")]
    InvalidThreshold { threshold: usize, total: usize },
    
    /// Member not found
    #[error("Federation member not found: {member_id}")]
    MemberNotFound { member_id: String },
    
    /// Duplicate member
    #[error("Duplicate federation member: {member_id}")]
    DuplicateMember { member_id: String },
    
    /// Timeout errors
    #[error("Operation timed out: {operation} after {timeout:?}")]
    Timeout { operation: String, timeout: std::time::Duration },
    
    /// Serialization errors
    #[error("Serialization error: {format} - {reason}")]
    Serialization { format: String, reason: String },
    
    /// Storage errors
    #[error("Storage error: {operation} - {reason}")]
    Storage { operation: String, reason: String },
    
    /// Invalid state
    #[error("Invalid state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },
    
    /// Permission denied
    #[error("Permission denied: {operation} by {actor}")]
    PermissionDenied { operation: String, actor: String },
    
    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
    
    /// Version mismatch
    #[error("Version mismatch: local={local}, remote={remote}")]
    VersionMismatch { local: String, remote: String },
    
    /// Emergency mode
    #[error("Federation in emergency mode: {reason}")]
    EmergencyMode { reason: String },
    
    /// Internal error
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl FederationError {
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            FederationError::Network { .. } => true,
            FederationError::Timeout { .. } => true,
            FederationError::InsufficientSignatures { .. } => true,
            FederationError::ResourceExhausted { .. } => true,
            FederationError::Consensus { .. } => true,
            
            FederationError::Configuration { .. } => false,
            FederationError::KeyManagement { .. } => false,
            FederationError::InvalidThreshold { .. } => false,
            FederationError::PermissionDenied { .. } => false,
            FederationError::EmergencyMode { .. } => false,
            FederationError::Internal { .. } => false,
            
            _ => true, // Most errors are potentially recoverable
        }
    }
    
    /// Check if error should trigger emergency mode
    pub fn triggers_emergency(&self) -> bool {
        match self {
            FederationError::KeyManagement { .. } => true,
            FederationError::Internal { .. } => true,
            FederationError::Storage { .. } => true,
            _ => false,
        }
    }
    
    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            FederationError::EmergencyMode { .. } => ErrorSeverity::Critical,
            FederationError::Internal { .. } => ErrorSeverity::Critical,
            FederationError::KeyManagement { .. } => ErrorSeverity::Critical,
            
            FederationError::Bridge { .. } => ErrorSeverity::High,
            FederationError::Bitcoin { .. } => ErrorSeverity::High,
            FederationError::InvalidThreshold { .. } => ErrorSeverity::High,
            FederationError::Storage { .. } => ErrorSeverity::High,
            
            FederationError::Signature { .. } => ErrorSeverity::Medium,
            FederationError::Transaction { .. } => ErrorSeverity::Medium,
            FederationError::Utxo { .. } => ErrorSeverity::Medium,
            FederationError::Protocol { .. } => ErrorSeverity::Medium,
            FederationError::InsufficientSignatures { .. } => ErrorSeverity::Medium,
            FederationError::Consensus { .. } => ErrorSeverity::Medium,
            
            FederationError::Network { .. } => ErrorSeverity::Low,
            FederationError::Timeout { .. } => ErrorSeverity::Low,
            FederationError::Serialization { .. } => ErrorSeverity::Low,
            FederationError::VersionMismatch { .. } => ErrorSeverity::Low,
            
            _ => ErrorSeverity::Medium,
        }
    }
    
    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            FederationError::Governance { .. } => "governance",
            FederationError::KeyManagement { .. } => "keys",
            FederationError::Signature { .. } => "signatures",
            FederationError::Bitcoin { .. } => "bitcoin",
            FederationError::Utxo { .. } => "utxo",
            FederationError::Transaction { .. } => "transactions",
            FederationError::Bridge { .. } => "bridge",
            FederationError::Protocol { .. } => "protocol",
            FederationError::Network { .. } => "network",
            FederationError::Consensus { .. } => "consensus",
            FederationError::Configuration { .. } => "config",
            FederationError::Storage { .. } => "storage",
            FederationError::PermissionDenied { .. } => "permissions",
            FederationError::EmergencyMode { .. } => "emergency",
            _ => "general",
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
impl From<std::io::Error> for FederationError {
    fn from(err: std::io::Error) -> Self {
        FederationError::Storage {
            operation: "io".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for FederationError {
    fn from(err: serde_json::Error) -> Self {
        FederationError::Serialization {
            format: "json".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<bitcoin::consensus::encode::Error> for FederationError {
    fn from(err: bitcoin::consensus::encode::Error) -> Self {
        FederationError::Bitcoin {
            operation: "serialization".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<secp256k1::Error> for FederationError {
    fn from(err: secp256k1::Error) -> Self {
        FederationError::Signature {
            signature_type: "secp256k1".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<tonic::Status> for FederationError {
    fn from(err: tonic::Status) -> Self {
        FederationError::Network {
            peer_id: "unknown".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<tokio::time::error::Elapsed> for FederationError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        FederationError::Timeout {
            operation: "unknown".to_string(),
            timeout: std::time::Duration::from_secs(0),
        }
    }
}