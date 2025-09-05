//! Error types for the Lighthouse facade
//!
//! This module provides error types that abstract over the underlying
//! compatibility layer errors while providing clear, actionable error messages.

use thiserror::Error;

/// Result type for facade operations
pub type FacadeResult<T> = Result<T, FacadeError>;

/// Error types for the facade layer
#[derive(Error, Debug)]
pub enum FacadeError {
    /// Initialization error
    #[error("Facade initialization failed: {reason}")]
    Initialization { reason: String },
    
    /// Configuration error
    #[error("Invalid configuration: {parameter} - {reason}")]
    InvalidConfiguration { parameter: String, reason: String },
    
    /// Service unavailable
    #[error("Lighthouse service unavailable: {service}")]
    ServiceUnavailable { service: String },
    
    /// Engine API error
    #[error("Engine API operation failed: {operation} - {reason}")]
    EngineApi { operation: String, reason: String },
    
    /// Conversion error
    #[error("Type conversion failed: {reason}")]
    Conversion { reason: String },
    
    /// Migration error
    #[error("Migration operation failed: {reason}")]
    Migration { reason: String },
    
    /// Internal error
    #[error("Internal facade error: {reason}")]
    Internal { reason: String },
    
    /// Connection error
    #[error("Connection error: {endpoint} - {reason}")]
    Connection { endpoint: String, reason: String },
    
    /// API error
    #[error("API error: {method} {endpoint} - {status} {reason}")]
    Api { method: String, endpoint: String, status: u16, reason: String },
    
    /// Timeout error
    #[error("Operation timed out: {operation} after {timeout:?}")]
    Timeout { operation: String, timeout: std::time::Duration },
    
    /// Configuration error (alias for InvalidConfiguration)
    #[error("Configuration error: {parameter} - {reason}")]
    Configuration { parameter: String, reason: String },
    
    /// Type conversion error
    #[error("Type conversion error: {from_type} -> {to_type} - {reason}")]
    TypeConversion { from_type: String, to_type: String, reason: String },
    
    /// Incompatible feature error
    #[error("Incompatible feature: {feature} not supported in {version}")]
    IncompatibleFeature { feature: String, version: String },
    
    /// Validation error
    #[error("Validation error: {field} - {reason}")]
    ValidationError { field: String, reason: String },
    
    /// Compatibility error
    #[error("Compatibility error: {0}")]
    Compatibility(String),
}

impl FacadeError {
    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ServiceUnavailable { .. } => true,
            Self::EngineApi { .. } => true,
            Self::Connection { .. } => true,
            Self::Api { status, .. } => *status >= 500, // Server errors are recoverable
            Self::Timeout { .. } => true,
            Self::InvalidConfiguration { .. } => false,
            Self::Configuration { .. } => false,
            Self::Conversion { .. } => false,
            Self::TypeConversion { .. } => false,
            Self::IncompatibleFeature { .. } => false,
            Self::ValidationError { .. } => false,
            Self::Compatibility(_) => false,
            _ => false,
        }
    }
    
    /// Get the error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Initialization { .. } => ErrorSeverity::Critical,
            Self::InvalidConfiguration { .. } => ErrorSeverity::High,
            Self::Configuration { .. } => ErrorSeverity::High,
            Self::ServiceUnavailable { .. } => ErrorSeverity::Medium,
            Self::EngineApi { .. } => ErrorSeverity::Medium,
            Self::Connection { .. } => ErrorSeverity::Medium,
            Self::Api { status, .. } => {
                if *status >= 500 {
                    ErrorSeverity::High
                } else if *status >= 400 {
                    ErrorSeverity::Medium
                } else {
                    ErrorSeverity::Low
                }
            }
            Self::Timeout { .. } => ErrorSeverity::Medium,
            Self::Conversion { .. } => ErrorSeverity::Low,
            Self::TypeConversion { .. } => ErrorSeverity::Low,
            Self::IncompatibleFeature { .. } => ErrorSeverity::High,
            Self::ValidationError { .. } => ErrorSeverity::Medium,
            Self::Migration { .. } => ErrorSeverity::High,
            Self::Internal { .. } => ErrorSeverity::Critical,
            Self::Compatibility(_) => ErrorSeverity::Medium,
        }
    }
    
    /// Get user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            Self::ServiceUnavailable { service } => {
                format!("Lighthouse service '{}' is currently unavailable", service)
            }
            Self::InvalidConfiguration { parameter, .. } => {
                format!("Configuration parameter '{}' is invalid", parameter)
            }
            Self::EngineApi { operation, .. } => {
                format!("Engine operation '{}' failed", operation)
            }
            _ => self.to_string(),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low impact
    Low,
    /// Medium impact
    Medium,
    /// High impact
    High,
    /// Critical impact
    Critical,
}

impl ErrorSeverity {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
    
    /// Check if this severity should trigger alerts
    pub fn should_alert(&self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}