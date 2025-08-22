//! Error types for the Lighthouse compatibility layer
//!
//! This module provides comprehensive error handling for all aspects of the
//! Lighthouse v4 to v5 migration process, including network errors, version
//! incompatibilities, migration failures, and rollback scenarios.

use std::time::Duration;
use thiserror::Error;

/// Result type for compatibility layer operations
pub type CompatResult<T> = Result<T, CompatError>;

/// Comprehensive error types for the compatibility layer
#[derive(Error, Debug, Clone)]
pub enum CompatError {
    /// Version-related errors
    #[error("Version incompatible: expected {expected}, got {actual}")]
    VersionIncompatible { expected: String, actual: String },
    
    #[error("Unsupported version: {version}")]
    UnsupportedVersion { version: String },
    
    /// Type conversion errors
    #[error("Type conversion failed: {from_type} -> {to_type}: {reason}")]
    TypeConversion { 
        from_type: String, 
        to_type: String, 
        reason: String 
    },
    
    #[error("Incompatible feature: {feature} not supported in {version}")]
    IncompatibleFeature { feature: String, version: String },
    
    /// Client connection errors
    #[error("Connection failed: {endpoint} - {reason}")]
    Connection { endpoint: String, reason: String },
    
    #[error("Authentication failed: {method} - {reason}")]
    Authentication { method: String, reason: String },
    
    #[error("Timeout: {operation} exceeded {timeout:?}")]
    Timeout { operation: String, timeout: Duration },
    
    /// API errors
    #[error("API error: {method} {endpoint} - {status}: {message}")]
    Api { 
        method: String, 
        endpoint: String, 
        status: u16, 
        message: String 
    },
    
    #[error("Engine API error: {operation} - {details}")]
    EngineApi { operation: String, details: String },
    
    /// Migration errors
    #[error("Migration failed: {phase} - {reason}")]
    MigrationFailed { phase: String, reason: String },
    
    #[error("Rollback failed: {reason}")]
    RollbackFailed { reason: String },
    
    #[error("Health check failed: {check_type} - {reason}")]
    HealthCheckFailed { check_type: String, reason: String },
    
    #[error("Migration timeout: {phase} exceeded {timeout:?}")]
    MigrationTimeout { phase: String, timeout: Duration },
    
    /// A/B testing errors
    #[error("A/B test error: {test_name} - {reason}")]
    ABTestError { test_name: String, reason: String },
    
    #[error("Invalid test configuration: {parameter} - {reason}")]
    InvalidTestConfig { parameter: String, reason: String },
    
    #[error("Statistical analysis failed: {test_name} - {reason}")]
    StatisticalAnalysis { test_name: String, reason: String },
    
    /// Configuration errors
    #[error("Configuration error: {parameter} - {reason}")]
    Configuration { parameter: String, reason: String },
    
    #[error("Invalid migration mode: {mode}")]
    InvalidMigrationMode { mode: String },
    
    /// Serialization errors
    #[error("Serialization error: {format} - {reason}")]
    Serialization { format: String, reason: String },
    
    #[error("Deserialization error: {format} - {data_type} - {reason}")]
    Deserialization { 
        format: String, 
        data_type: String, 
        reason: String 
    },
    
    /// State management errors
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },
    
    #[error("State corruption detected: {component}")]
    StateCorruption { component: String },
    
    /// Resource errors
    #[error("Resource not found: {resource_type} {identifier}")]
    ResourceNotFound { resource_type: String, identifier: String },
    
    #[error("Resource exhausted: {resource} - {limit}")]
    ResourceExhausted { resource: String, limit: String },
    
    /// Security errors
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },
    
    #[error("Rate limit exceeded: {limit} requests per {window:?}")]
    RateLimitExceeded { limit: u32, window: Duration },
    
    /// Consensus errors
    #[error("Consensus mismatch: v4={v4_result:?}, v5={v5_result:?}")]
    ConsensusMismatch { v4_result: String, v5_result: String },
    
    #[error("Fork detected during migration: {fork_info}")]
    ForkDetected { fork_info: String },
    
    /// Performance errors
    #[error("Performance degradation: {metric} exceeded threshold {threshold}")]
    PerformanceDegradation { metric: String, threshold: String },
    
    #[error("Memory limit exceeded: {used} > {limit}")]
    MemoryLimitExceeded { used: String, limit: String },
    
    /// Initialization errors
    #[error("Initialization failed: {reason}")]
    Initialization { reason: String },
    
    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },
    
    /// Internal errors
    #[error("Internal error: {message}")]
    Internal { message: String },
    
    #[error("Unrecoverable error: {reason}")]
    Unrecoverable { reason: String },
}

impl CompatError {
    /// Check if the error is recoverable through retry
    pub fn is_recoverable(&self) -> bool {
        match self {
            // Recoverable errors
            Self::Connection { .. } => true,
            Self::Timeout { .. } => true,
            Self::Api { status, .. } => *status >= 500,
            Self::ServiceUnavailable { .. } => true,
            Self::RateLimitExceeded { .. } => true,
            Self::HealthCheckFailed { .. } => true,
            Self::PerformanceDegradation { .. } => true,
            Self::ResourceExhausted { .. } => true,
            
            // Non-recoverable errors
            Self::VersionIncompatible { .. } => false,
            Self::UnsupportedVersion { .. } => false,
            Self::Configuration { .. } => false,
            Self::PermissionDenied { .. } => false,
            Self::StateCorruption { .. } => false,
            Self::Unrecoverable { .. } => false,
            Self::InvalidMigrationMode { .. } => false,
            
            // Context-dependent
            Self::TypeConversion { .. } => false,
            Self::MigrationFailed { .. } => false,
            Self::RollbackFailed { .. } => false,
            Self::ConsensusMismatch { .. } => true, // May resolve with retry
            Self::ForkDetected { .. } => false,
            
            _ => true, // Default to recoverable
        }
    }
    
    /// Check if the error should trigger an automatic retry
    pub fn should_retry(&self) -> bool {
        match self {
            Self::Connection { .. } => true,
            Self::Timeout { .. } => true,
            Self::Api { status, .. } => *status >= 500,
            Self::ServiceUnavailable { .. } => true,
            Self::HealthCheckFailed { .. } => true,
            
            // Don't retry rate limits immediately
            Self::RateLimitExceeded { .. } => false,
            
            _ => false,
        }
    }
    
    /// Get the severity level of the error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            // Critical errors requiring immediate attention
            Self::StateCorruption { .. } => ErrorSeverity::Critical,
            Self::Unrecoverable { .. } => ErrorSeverity::Critical,
            Self::RollbackFailed { .. } => ErrorSeverity::Critical,
            Self::ForkDetected { .. } => ErrorSeverity::Critical,
            Self::ConsensusMismatch { .. } => ErrorSeverity::Critical,
            
            // High severity errors
            Self::MigrationFailed { .. } => ErrorSeverity::High,
            Self::VersionIncompatible { .. } => ErrorSeverity::High,
            Self::UnsupportedVersion { .. } => ErrorSeverity::High,
            Self::PermissionDenied { .. } => ErrorSeverity::High,
            Self::MemoryLimitExceeded { .. } => ErrorSeverity::High,
            
            // Medium severity errors
            Self::TypeConversion { .. } => ErrorSeverity::Medium,
            Self::Configuration { .. } => ErrorSeverity::Medium,
            Self::HealthCheckFailed { .. } => ErrorSeverity::Medium,
            Self::PerformanceDegradation { .. } => ErrorSeverity::Medium,
            Self::ABTestError { .. } => ErrorSeverity::Medium,
            Self::EngineApi { .. } => ErrorSeverity::Medium,
            
            // Low severity errors
            Self::Connection { .. } => ErrorSeverity::Low,
            Self::Timeout { .. } => ErrorSeverity::Low,
            Self::Api { .. } => ErrorSeverity::Low,
            Self::ServiceUnavailable { .. } => ErrorSeverity::Low,
            Self::RateLimitExceeded { .. } => ErrorSeverity::Low,
            
            _ => ErrorSeverity::Medium,
        }
    }
    
    /// Get the error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            Self::VersionIncompatible { .. } | Self::UnsupportedVersion { .. } => "version",
            Self::TypeConversion { .. } | Self::IncompatibleFeature { .. } => "conversion",
            Self::Connection { .. } | Self::Authentication { .. } => "connection",
            Self::Api { .. } | Self::EngineApi { .. } => "api",
            Self::MigrationFailed { .. } | Self::MigrationTimeout { .. } => "migration",
            Self::RollbackFailed { .. } => "rollback",
            Self::HealthCheckFailed { .. } => "health",
            Self::ABTestError { .. } | Self::StatisticalAnalysis { .. } => "ab_test",
            Self::Configuration { .. } | Self::InvalidMigrationMode { .. } => "config",
            Self::Serialization { .. } | Self::Deserialization { .. } => "serialization",
            Self::InvalidStateTransition { .. } | Self::StateCorruption { .. } => "state",
            Self::ResourceNotFound { .. } | Self::ResourceExhausted { .. } => "resource",
            Self::PermissionDenied { .. } | Self::RateLimitExceeded { .. } => "security",
            Self::ConsensusMismatch { .. } | Self::ForkDetected { .. } => "consensus",
            Self::PerformanceDegradation { .. } | Self::MemoryLimitExceeded { .. } => "performance",
            _ => "general",
        }
    }
    
    /// Check if the error should trigger a rollback
    pub fn should_rollback(&self) -> bool {
        match self {
            Self::StateCorruption { .. } => true,
            Self::ConsensusMismatch { .. } => true,
            Self::ForkDetected { .. } => true,
            Self::MemoryLimitExceeded { .. } => true,
            Self::Unrecoverable { .. } => true,
            Self::PerformanceDegradation { .. } => true, // Configurable threshold
            _ => false,
        }
    }
    
    /// Get a user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            Self::MigrationFailed { phase, .. } => {
                format!("Migration failed during {}, rolling back to safe state", phase)
            }
            Self::VersionIncompatible { expected, actual } => {
                format!("Version mismatch: system expects {} but found {}", expected, actual)
            }
            Self::Connection { endpoint, .. } => {
                format!("Cannot connect to service at {}", endpoint)
            }
            Self::HealthCheckFailed { check_type, .. } => {
                format!("System health check failed: {}", check_type)
            }
            _ => self.to_string(),
        }
    }
}

/// Error severity levels for alerting and monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low impact - logged but no immediate action needed
    Low,
    /// Medium impact - requires attention but not urgent
    Medium,
    /// High impact - requires prompt attention
    High,
    /// Critical impact - requires immediate action
    Critical,
}

impl ErrorSeverity {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
    
    /// Check if this severity level should trigger alerts
    pub fn should_alert(&self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

// Standard library error conversions
impl From<std::io::Error> for CompatError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::ResourceNotFound {
                resource_type: "file".to_string(),
                identifier: "unknown".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied {
                operation: "file_access".to_string(),
            },
            std::io::ErrorKind::TimedOut => Self::Timeout {
                operation: "io".to_string(),
                timeout: Duration::from_secs(30),
            },
            _ => Self::Internal {
                message: format!("IO error: {}", err),
            },
        }
    }
}

impl From<serde_json::Error> for CompatError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization {
            format: "json".to_string(),
            reason: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for CompatError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout {
                operation: "http_request".to_string(),
                timeout: Duration::from_secs(30),
            }
        } else if err.is_connect() {
            Self::Connection {
                endpoint: err.url().map(|u| u.to_string()).unwrap_or_default(),
                reason: "Connection failed".to_string(),
            }
        } else if err.is_status() {
            Self::Api {
                method: "unknown".to_string(),
                endpoint: err.url().map(|u| u.to_string()).unwrap_or_default(),
                status: err.status().map(|s| s.as_u16()).unwrap_or(0),
                message: err.to_string(),
            }
        } else {
            Self::Internal {
                message: format!("HTTP error: {}", err),
            }
        }
    }
}

impl From<tokio::time::error::Elapsed> for CompatError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Self::Timeout {
            operation: "task".to_string(),
            timeout: Duration::from_secs(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_severity() {
        let critical_err = CompatError::StateCorruption {
            component: "test".to_string(),
        };
        assert_eq!(critical_err.severity(), ErrorSeverity::Critical);
        assert!(critical_err.severity().should_alert());
    }
    
    #[test]
    fn test_error_recoverability() {
        let timeout_err = CompatError::Timeout {
            operation: "test".to_string(),
            timeout: Duration::from_secs(5),
        };
        assert!(timeout_err.is_recoverable());
        assert!(timeout_err.should_retry());
        
        let version_err = CompatError::VersionIncompatible {
            expected: "v4".to_string(),
            actual: "v3".to_string(),
        };
        assert!(!version_err.is_recoverable());
        assert!(!version_err.should_retry());
    }
    
    #[test]
    fn test_error_category() {
        let conn_err = CompatError::Connection {
            endpoint: "test".to_string(),
            reason: "test".to_string(),
        };
        assert_eq!(conn_err.category(), "connection");
        
        let migration_err = CompatError::MigrationFailed {
            phase: "test".to_string(),
            reason: "test".to_string(),
        };
        assert_eq!(migration_err.category(), "migration");
    }
    
    #[test]
    fn test_rollback_triggers() {
        let corruption_err = CompatError::StateCorruption {
            component: "test".to_string(),
        };
        assert!(corruption_err.should_rollback());
        
        let timeout_err = CompatError::Timeout {
            operation: "test".to_string(),
            timeout: Duration::from_secs(5),
        };
        assert!(!timeout_err.should_rollback());
    }
}