//! Lighthouse wrapper error types

use thiserror::Error;

/// Result type for Lighthouse operations
pub type LighthouseResult<T> = Result<T, LighthouseError>;

/// Lighthouse wrapper errors
#[derive(Debug, Error, Clone)]
pub enum LighthouseError {
    /// Connection errors
    #[error("Connection error: {endpoint} - {reason}")]
    Connection { endpoint: String, reason: String },
    
    /// API errors
    #[error("API error: {method} {endpoint} - {status} {reason}")]
    Api { method: String, endpoint: String, status: u16, reason: String },
    
    /// BLS signature errors
    #[error("BLS error: {operation} - {reason}")]
    Bls { operation: String, reason: String },
    
    /// Beacon chain errors
    #[error("Beacon chain error: {reason}")]
    BeaconChain { reason: String },
    
    /// Validator errors
    #[error("Validator error: {validator_id} - {reason}")]
    Validator { validator_id: String, reason: String },
    
    /// Synchronization errors
    #[error("Sync error: {sync_type} - {reason}")]
    Sync { sync_type: String, reason: String },
    
    /// Configuration errors
    #[error("Configuration error: {parameter} - {reason}")]
    Configuration { parameter: String, reason: String },
    
    /// Timeout errors
    #[error("Operation timed out: {operation} after {timeout:?}")]
    Timeout { operation: String, timeout: std::time::Duration },
    
    /// Serialization errors
    #[error("Serialization error: {format} - {reason}")]
    Serialization { format: String, reason: String },
    
    /// Key management errors
    #[error("Key management error: {key_type} - {reason}")]
    KeyManagement { key_type: String, reason: String },
    
    /// Network errors
    #[error("Network error: {reason}")]
    Network { reason: String },
    
    /// Version incompatibility
    #[error("Version incompatible: expected {expected}, got {actual}")]
    VersionIncompatible { expected: String, actual: String },
    
    /// Service unavailable
    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },
    
    /// Invalid state
    #[error("Invalid state: {expected} -> {actual}")]
    InvalidState { expected: String, actual: String },
    
    /// Resource not found
    #[error("Resource not found: {resource_type} {identifier}")]
    ResourceNotFound { resource_type: String, identifier: String },
    
    /// Permission denied
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },
    
    /// Rate limit exceeded
    #[error("Rate limit exceeded: {limit} requests per {window:?}")]
    RateLimitExceeded { limit: u32, window: std::time::Duration },
    
    /// Internal error
    #[error("Internal error: {message}")]
    Internal { message: String },
    
    /// Lighthouse not ready
    #[error("Lighthouse not ready: {reason}")]
    NotReady { reason: String },
    
    /// Consensus failure
    #[error("Consensus failure: {reason}")]
    ConsensusFailure { reason: String },
    
    /// Fork detected
    #[error("Fork detected: {fork_info}")]
    ForkDetected { fork_info: String },
}

impl LighthouseError {
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            LighthouseError::Connection { .. } => true,
            LighthouseError::Network { .. } => true,
            LighthouseError::Timeout { .. } => true,
            LighthouseError::ServiceUnavailable { .. } => true,
            LighthouseError::RateLimitExceeded { .. } => true,
            LighthouseError::NotReady { .. } => true,
            LighthouseError::Sync { .. } => true,
            
            LighthouseError::Configuration { .. } => false,
            LighthouseError::VersionIncompatible { .. } => false,
            LighthouseError::PermissionDenied { .. } => false,
            LighthouseError::KeyManagement { .. } => false,
            LighthouseError::Internal { .. } => false,
            
            _ => true, // Most errors are potentially recoverable
        }
    }
    
    /// Check if error should trigger retry
    pub fn should_retry(&self) -> bool {
        match self {
            LighthouseError::Connection { .. } => true,
            LighthouseError::Network { .. } => true,
            LighthouseError::Timeout { .. } => true,
            LighthouseError::ServiceUnavailable { .. } => true,
            LighthouseError::NotReady { .. } => true,
            
            // API errors depend on status code
            LighthouseError::Api { status, .. } => *status >= 500,
            
            _ => false,
        }
    }
    
    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            LighthouseError::Internal { .. } => ErrorSeverity::Critical,
            LighthouseError::ConsensusFailure { .. } => ErrorSeverity::Critical,
            LighthouseError::KeyManagement { .. } => ErrorSeverity::Critical,
            
            LighthouseError::BeaconChain { .. } => ErrorSeverity::High,
            LighthouseError::ForkDetected { .. } => ErrorSeverity::High,
            LighthouseError::VersionIncompatible { .. } => ErrorSeverity::High,
            
            LighthouseError::Validator { .. } => ErrorSeverity::Medium,
            LighthouseError::Sync { .. } => ErrorSeverity::Medium,
            LighthouseError::Bls { .. } => ErrorSeverity::Medium,
            LighthouseError::Configuration { .. } => ErrorSeverity::Medium,
            
            LighthouseError::Connection { .. } => ErrorSeverity::Low,
            LighthouseError::Network { .. } => ErrorSeverity::Low,
            LighthouseError::Api { .. } => ErrorSeverity::Low,
            LighthouseError::Timeout { .. } => ErrorSeverity::Low,
            LighthouseError::ServiceUnavailable { .. } => ErrorSeverity::Low,
            LighthouseError::NotReady { .. } => ErrorSeverity::Low,
            
            _ => ErrorSeverity::Medium,
        }
    }
    
    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            LighthouseError::Connection { .. } => "connection",
            LighthouseError::Api { .. } => "api",
            LighthouseError::Bls { .. } => "bls",
            LighthouseError::BeaconChain { .. } => "beacon_chain",
            LighthouseError::Validator { .. } => "validator",
            LighthouseError::Sync { .. } => "sync",
            LighthouseError::Configuration { .. } => "config",
            LighthouseError::Network { .. } => "network",
            LighthouseError::KeyManagement { .. } => "keys",
            LighthouseError::ConsensusFailure { .. } => "consensus",
            LighthouseError::ForkDetected { .. } => "fork",
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
impl From<reqwest::Error> for LighthouseError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            LighthouseError::Timeout {
                operation: "http_request".to_string(),
                timeout: std::time::Duration::from_secs(30),
            }
        } else if err.is_connect() {
            LighthouseError::Connection {
                endpoint: err.url().map(|u| u.to_string()).unwrap_or_default(),
                reason: "Connection failed".to_string(),
            }
        } else {
            LighthouseError::Network {
                reason: err.to_string(),
            }
        }
    }
}

impl From<serde_json::Error> for LighthouseError {
    fn from(err: serde_json::Error) -> Self {
        LighthouseError::Serialization {
            format: "json".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<std::io::Error> for LighthouseError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => LighthouseError::ResourceNotFound {
                resource_type: "file".to_string(),
                identifier: "unknown".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => LighthouseError::PermissionDenied {
                operation: "file_access".to_string(),
            },
            std::io::ErrorKind::TimedOut => LighthouseError::Timeout {
                operation: "io".to_string(),
                timeout: std::time::Duration::from_secs(30),
            },
            _ => LighthouseError::Internal {
                message: format!("IO error: {}", err),
            },
        }
    }
}

impl From<tokio::time::error::Elapsed> for LighthouseError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        LighthouseError::Timeout {
            operation: "task".to_string(),
            timeout: std::time::Duration::from_secs(0),
        }
    }
}