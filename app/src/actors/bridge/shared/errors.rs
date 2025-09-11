//! Bridge Actor Error Types
//! 
//! Comprehensive error handling for bridge actor system operations

use std::time::Duration;
use thiserror::Error;
use serde::{Deserialize, Serialize};

/// Configuration-specific error type
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ConfigError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Configuration parse error: {message}")]
    ParseError { message: String },
    
    #[error("Configuration validation error: {0}")]
    ValidationError(String),
    
    #[error("Configuration I/O error: {message}")]
    IoError { message: String },
    
    #[error("Unsupported configuration format: {format}")]
    UnsupportedFormat { format: String },
    
    #[error("Configuration schema error: {message}")]
    SchemaError { message: String },
    
    #[error("Environment configuration error: {message}")]
    EnvironmentError { message: String },
}

/// Bridge operation errors
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum BridgeError {
    /// Connection errors
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Network communication errors  
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Authentication and authorization errors
    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    /// Simple validation errors
    #[error("Validation error: {0}")]
    SimpleValidationError(String),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Request timeout errors
    #[error("Request timeout: {request_id} (timeout: {timeout:?})")]
    RequestTimeout {
        request_id: String,
        timeout: Duration,
    },

    /// Request cancelled errors
    #[error("Request cancelled: {request_id}")]
    RequestCancelled {
        request_id: String,
    },

    /// Request not found errors
    #[error("Request not found: {request_id}")]
    RequestNotFound {
        request_id: String,
    },

    /// Invalid request errors
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Unknown request correlation errors
    #[error("Unknown request correlation: {0}")]
    UnknownRequest(String),

    /// Signature collection errors
    #[error("Signature collection failed: {request_id} - {reason}")]
    SignatureCollectionFailed {
        request_id: String,
        reason: String,
    },

    /// Insufficient signatures errors
    #[error("Insufficient signatures: {collected}/{required} for request {request_id}")]
    InsufficientSignatures {
        request_id: String,
        collected: usize,
        required: usize,
    },

    /// Federation update errors
    #[error("Federation update failed: {update_id} - {reason}")]
    FederationUpdateFailed {
        update_id: String,
        reason: String,
    },

    /// Internal actor errors
    #[error("Internal error: {0}")]
    InternalError(String),

    /// Actor system integration errors
    #[error("Actor system error: {0}")]
    ActorSystemError(String),

    /// gRPC protocol errors
    #[error("gRPC error: {0}")]
    GrpcError(String),

    /// Peg-out operation errors
    #[error("Peg-out error: {pegout_id} - {reason}")]
    PegOutError {
        pegout_id: String,
        reason: String,
    },

    /// Peg-in operation errors
    #[error("Peg-in error: {pegin_id} - {reason}")]
    PegInError {
        pegin_id: String,
        reason: String,
    },

    /// Governance communication errors
    #[error("Governance error: {0}")]
    GovernanceError(String),

    /// Resource exhaustion errors
    #[error("Resource exhausted: {resource} - {details}")]
    ResourceExhausted {
        resource: String,
        details: String,
    },

    /// Validation errors
    #[error("Validation error: {field} - {reason}")]
    ValidationError {
        field: String,
        reason: String,
    },

    /// State transition errors
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition {
        from: String,
        to: String,
    },

    /// Temporary service unavailable errors
    #[error("Service unavailable: {service} - retry after {retry_after:?}")]
    ServiceUnavailable {
        service: String,
        retry_after: Option<Duration>,
    },

    /// Rate limiting errors
    #[error("Rate limit exceeded: {limit} requests per {window:?}")]
    RateLimitExceeded {
        limit: u32,
        window: Duration,
    },
}

impl BridgeError {
    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            BridgeError::ConnectionError(_) => true,
            BridgeError::NetworkError(_) => true,
            BridgeError::RequestTimeout { .. } => true,
            BridgeError::ServiceUnavailable { .. } => true,
            BridgeError::RateLimitExceeded { .. } => true,
            BridgeError::ResourceExhausted { .. } => false,
            BridgeError::AuthenticationError(_) => false,
            BridgeError::ConfigurationError(_) => false,
            BridgeError::ValidationError { .. } => false,
            BridgeError::InvalidRequest(_) => false,
            BridgeError::RequestCancelled { .. } => false,
            _ => true, // Default to retryable for new error types
        }
    }

    /// Get retry delay suggestion
    pub fn retry_delay(&self) -> Option<Duration> {
        match self {
            BridgeError::ServiceUnavailable { retry_after, .. } => *retry_after,
            BridgeError::RateLimitExceeded { window, .. } => Some(*window),
            BridgeError::ConnectionError(_) => Some(Duration::from_secs(5)),
            BridgeError::NetworkError(_) => Some(Duration::from_secs(2)),
            BridgeError::RequestTimeout { .. } => Some(Duration::from_secs(10)),
            _ => None,
        }
    }

    /// Get error category for metrics and logging
    pub fn category(&self) -> BridgeErrorCategory {
        match self {
            BridgeError::ConnectionError(_) | 
            BridgeError::NetworkError(_) => BridgeErrorCategory::Network,
            
            BridgeError::AuthenticationError(_) => BridgeErrorCategory::Auth,
            
            BridgeError::RequestTimeout { .. } |
            BridgeError::RequestCancelled { .. } |
            BridgeError::RequestNotFound { .. } => BridgeErrorCategory::Request,
            
            BridgeError::SignatureCollectionFailed { .. } |
            BridgeError::InsufficientSignatures { .. } => BridgeErrorCategory::Signature,
            
            BridgeError::FederationUpdateFailed { .. } => BridgeErrorCategory::Federation,
            
            BridgeError::PegOutError { .. } |
            BridgeError::PegInError { .. } => BridgeErrorCategory::Bridge,
            
            BridgeError::ValidationError { .. } |
            BridgeError::InvalidRequest(_) => BridgeErrorCategory::Validation,
            
            BridgeError::ConfigurationError(_) => BridgeErrorCategory::Configuration,
            
            _ => BridgeErrorCategory::Internal,
        }
    }
}

/// Error categories for classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BridgeErrorCategory {
    Network,
    Auth,
    Request,
    Signature,
    Federation,
    Bridge,
    Validation,
    Configuration,
    Internal,
}

/// Convert from other error types
impl From<serde_json::Error> for BridgeError {
    fn from(err: serde_json::Error) -> Self {
        BridgeError::SerializationError(err.to_string())
    }
}

impl From<tonic::Status> for BridgeError {
    fn from(err: tonic::Status) -> Self {
        BridgeError::GrpcError(err.message().to_string())
    }
}

impl From<std::io::Error> for BridgeError {
    fn from(err: std::io::Error) -> Self {
        BridgeError::NetworkError(err.to_string())
    }
}

/// Migration error types for bridge actor transitions
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum MigrationError {
    #[error("Chain error during migration: {0}")]
    ChainError { message: String },
    
    #[error("Migration configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Migration state error: {0}")]
    StateError(String),
}