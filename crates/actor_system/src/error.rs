//! Error types for the actor system

use std::fmt;
use thiserror::Error;

/// Result type for actor operations
pub type ActorResult<T> = Result<T, ActorError>;

/// Actor system error types
#[derive(Debug, Error, Clone)]
pub enum ActorError {
    /// Actor not found in registry
    #[error("Actor not found: {name}")]
    ActorNotFound { name: String },
    
    /// Actor failed to start
    #[error("Actor startup failed: {actor_type} - {reason}")]
    StartupFailed { actor_type: String, reason: String },
    
    /// Actor failed to stop cleanly
    #[error("Actor shutdown failed: {actor_type} - {reason}")]
    ShutdownFailed { actor_type: String, reason: String },
    
    /// Message delivery failed
    #[error("Message delivery failed from {from} to {to}: {reason}")]
    MessageDeliveryFailed { from: String, to: String, reason: String },
    
    /// Message handling failed
    #[error("Message handling failed: {message_type} - {reason}")]
    MessageHandlingFailed { message_type: String, reason: String },
    
    /// Actor supervision failed
    #[error("Supervision failed for {actor_name}: {reason}")]
    SupervisionFailed { actor_name: String, reason: String },
    
    /// Actor restart failed
    #[error("Actor restart failed: {actor_name} - {reason}")]
    RestartFailed { actor_name: String, reason: String },
    
    /// System resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
    
    /// Configuration error
    #[error("Configuration error: {parameter} - {reason}")]
    ConfigurationError { parameter: String, reason: String },
    
    /// Permission denied
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },
    
    /// Invalid state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },
    
    /// Timeout occurred
    #[error("Operation timed out: {operation} after {timeout:?}")]
    Timeout { operation: String, timeout: std::time::Duration },
    
    /// Deadlock detected
    #[error("Deadlock detected in actor chain: {actors:?}")]
    DeadlockDetected { actors: Vec<String> },
    
    /// Actor mailbox full
    #[error("Mailbox full for actor {actor_name}: {current_size}/{max_size}")]
    MailboxFull { actor_name: String, current_size: usize, max_size: usize },
    
    /// Serialization error
    #[error("Serialization failed: {reason}")]
    SerializationFailed { reason: String },
    
    /// Deserialization error
    #[error("Deserialization failed: {reason}")]
    DeserializationFailed { reason: String },
    
    /// Network error
    #[error("Network error: {reason}")]
    NetworkError { reason: String },
    
    /// Storage error
    #[error("Storage error: {reason}")]
    StorageError { reason: String },
    
    /// Critical system failure
    #[error("Critical system failure: {reason}")]
    SystemFailure { reason: String },
    
    /// Internal error (should not happen in production)
    #[error("Internal error: {reason}")]
    Internal { reason: String },
    
    /// External dependency error
    #[error("External dependency error: {service} - {reason}")]
    ExternalDependency { service: String, reason: String },
    
    /// Rate limit exceeded
    #[error("Rate limit exceeded: {limit} requests per {window:?}")]
    RateLimitExceeded { limit: u32, window: std::time::Duration },
    
    /// Custom error with context
    #[error("Custom error: {message}")]
    Custom { message: String },
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low impact, system continues normally
    Minor,
    /// Medium impact, might affect performance
    Moderate,
    /// High impact, requires attention
    Major,
    /// System-threatening, requires immediate action
    Critical,
    /// System failure, emergency shutdown required
    Fatal,
}

/// Error context for better debugging
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub actor_name: String,
    pub actor_type: String,
    pub message_type: Option<String>,
    pub timestamp: std::time::SystemTime,
    pub severity: ErrorSeverity,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ErrorContext {
    /// Create new error context
    pub fn new(actor_name: String, actor_type: String) -> Self {
        Self {
            actor_name,
            actor_type,
            message_type: None,
            timestamp: std::time::SystemTime::now(),
            severity: ErrorSeverity::Moderate,
            metadata: std::collections::HashMap::new(),
        }
    }
    
    /// Set message type
    pub fn with_message_type(mut self, message_type: String) -> Self {
        self.message_type = Some(message_type);
        self
    }
    
    /// Set severity
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// Add multiple metadata entries
    pub fn with_metadata_map(mut self, metadata: std::collections::HashMap<String, String>) -> Self {
        self.metadata.extend(metadata);
        self
    }
}

impl ActorError {
    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ActorError::SystemFailure { .. } => ErrorSeverity::Fatal,
            ActorError::DeadlockDetected { .. } => ErrorSeverity::Critical,
            ActorError::ResourceExhausted { .. } => ErrorSeverity::Critical,
            ActorError::StartupFailed { .. } => ErrorSeverity::Major,
            ActorError::ShutdownFailed { .. } => ErrorSeverity::Major,
            ActorError::SupervisionFailed { .. } => ErrorSeverity::Major,
            ActorError::RestartFailed { .. } => ErrorSeverity::Major,
            ActorError::MessageDeliveryFailed { .. } => ErrorSeverity::Moderate,
            ActorError::MessageHandlingFailed { .. } => ErrorSeverity::Moderate,
            ActorError::MailboxFull { .. } => ErrorSeverity::Moderate,
            ActorError::Timeout { .. } => ErrorSeverity::Moderate,
            ActorError::InvalidStateTransition { .. } => ErrorSeverity::Moderate,
            ActorError::ConfigurationError { .. } => ErrorSeverity::Major,
            ActorError::PermissionDenied { .. } => ErrorSeverity::Moderate,
            ActorError::SerializationFailed { .. } => ErrorSeverity::Minor,
            ActorError::DeserializationFailed { .. } => ErrorSeverity::Minor,
            ActorError::NetworkError { .. } => ErrorSeverity::Moderate,
            ActorError::StorageError { .. } => ErrorSeverity::Major,
            ActorError::ExternalDependency { .. } => ErrorSeverity::Moderate,
            ActorError::RateLimitExceeded { .. } => ErrorSeverity::Minor,
            ActorError::ActorNotFound { .. } => ErrorSeverity::Minor,
            ActorError::Internal { .. } => ErrorSeverity::Critical,
            ActorError::Custom { .. } => ErrorSeverity::Moderate,
        }
    }
    
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Fatal | ErrorSeverity::Critical => false,
            _ => true,
        }
    }
    
    /// Check if error should trigger actor restart
    pub fn should_restart_actor(&self) -> bool {
        match self {
            ActorError::MessageHandlingFailed { .. } => true,
            ActorError::InvalidStateTransition { .. } => true,
            ActorError::Internal { .. } => true,
            _ => false,
        }
    }
    
    /// Check if error should escalate to supervisor
    pub fn should_escalate(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Critical | ErrorSeverity::Fatal => true,
            ErrorSeverity::Major => true,
            _ => false,
        }
    }
    
    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            ActorError::ActorNotFound { .. } => "actor_lifecycle",
            ActorError::StartupFailed { .. } => "actor_lifecycle",
            ActorError::ShutdownFailed { .. } => "actor_lifecycle",
            ActorError::RestartFailed { .. } => "actor_lifecycle",
            ActorError::MessageDeliveryFailed { .. } => "messaging",
            ActorError::MessageHandlingFailed { .. } => "messaging",
            ActorError::MailboxFull { .. } => "messaging",
            ActorError::SupervisionFailed { .. } => "supervision",
            ActorError::ResourceExhausted { .. } => "resources",
            ActorError::ConfigurationError { .. } => "configuration",
            ActorError::PermissionDenied { .. } => "security",
            ActorError::InvalidStateTransition { .. } => "state_management",
            ActorError::Timeout { .. } => "performance",
            ActorError::DeadlockDetected { .. } => "deadlock",
            ActorError::SerializationFailed { .. } => "serialization",
            ActorError::DeserializationFailed { .. } => "serialization",
            ActorError::NetworkError { .. } => "network",
            ActorError::StorageError { .. } => "storage",
            ActorError::SystemFailure { .. } => "system",
            ActorError::Internal { .. } => "internal",
            ActorError::ExternalDependency { .. } => "external",
            ActorError::RateLimitExceeded { .. } => "rate_limiting",
            ActorError::Custom { .. } => "custom",
        }
    }
}

/// Conversion from common error types
impl From<tokio::time::error::Elapsed> for ActorError {
    fn from(err: tokio::time::error::Elapsed) -> Self {
        ActorError::Timeout {
            operation: "tokio_timeout".to_string(),
            timeout: std::time::Duration::from_millis(0), // Unknown timeout duration
        }
    }
}

impl From<serde_json::Error> for ActorError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_io() {
            ActorError::SerializationFailed {
                reason: format!("JSON I/O error: {}", err),
            }
        } else if err.is_syntax() {
            ActorError::DeserializationFailed {
                reason: format!("JSON syntax error: {}", err),
            }
        } else {
            ActorError::SerializationFailed {
                reason: format!("JSON error: {}", err),
            }
        }
    }
}

impl From<std::io::Error> for ActorError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => ActorError::ActorNotFound {
                name: "unknown".to_string(),
            },
            std::io::ErrorKind::PermissionDenied => ActorError::PermissionDenied {
                operation: "io_operation".to_string(),
            },
            std::io::ErrorKind::TimedOut => ActorError::Timeout {
                operation: "io_operation".to_string(),
                timeout: std::time::Duration::from_millis(0),
            },
            _ => ActorError::SystemFailure {
                reason: format!("I/O error: {}", err),
            },
        }
    }
}

/// Error reporting and metrics
pub struct ErrorReporter {
    error_counts: dashmap::DashMap<String, std::sync::atomic::AtomicU64>,
}

impl ErrorReporter {
    /// Create new error reporter
    pub fn new() -> Self {
        Self {
            error_counts: dashmap::DashMap::new(),
        }
    }
    
    /// Report an error
    pub fn report_error(&self, error: &ActorError, context: Option<&ErrorContext>) {
        let category = error.category();
        
        // Increment error count
        let counter = self.error_counts
            .entry(category.to_string())
            .or_insert_with(|| std::sync::atomic::AtomicU64::new(0));
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Log error
        match error.severity() {
            ErrorSeverity::Fatal => {
                tracing::error!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "FATAL error occurred"
                );
            }
            ErrorSeverity::Critical => {
                tracing::error!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "CRITICAL error occurred"
                );
            }
            ErrorSeverity::Major => {
                tracing::error!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "MAJOR error occurred"
                );
            }
            ErrorSeverity::Moderate => {
                tracing::warn!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "MODERATE error occurred"
                );
            }
            ErrorSeverity::Minor => {
                tracing::debug!(
                    error = %error,
                    category = category,
                    context = ?context,
                    "MINOR error occurred"
                );
            }
        }
    }
    
    /// Get error counts by category
    pub fn get_error_counts(&self) -> std::collections::HashMap<String, u64> {
        self.error_counts
            .iter()
            .map(|entry| {
                let key = entry.key().clone();
                let value = entry.value().load(std::sync::atomic::Ordering::Relaxed);
                (key, value)
            })
            .collect()
    }
    
    /// Reset error counts
    pub fn reset_counts(&self) {
        for mut entry in self.error_counts.iter_mut() {
            entry.value_mut().store(0, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl Default for ErrorReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Global error reporter instance
static ERROR_REPORTER: once_cell::sync::Lazy<ErrorReporter> = 
    once_cell::sync::Lazy::new(ErrorReporter::new);

/// Report error globally
pub fn report_error(error: &ActorError, context: Option<&ErrorContext>) {
    ERROR_REPORTER.report_error(error, context);
}

/// Get global error counts
pub fn get_global_error_counts() -> std::collections::HashMap<String, u64> {
    ERROR_REPORTER.get_error_counts()
}

/// Reset global error counts
pub fn reset_global_error_counts() {
    ERROR_REPORTER.reset_counts();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_severity() {
        let error = ActorError::SystemFailure { reason: "test".to_string() };
        assert_eq!(error.severity(), ErrorSeverity::Fatal);
        assert!(!error.is_recoverable());
        assert!(error.should_escalate());
    }
    
    #[test]
    fn test_error_context() {
        let context = ErrorContext::new("test_actor".to_string(), "TestActor".to_string())
            .with_message_type("TestMessage".to_string())
            .with_severity(ErrorSeverity::Major)
            .with_metadata("key".to_string(), "value".to_string());
        
        assert_eq!(context.actor_name, "test_actor");
        assert_eq!(context.message_type, Some("TestMessage".to_string()));
        assert_eq!(context.severity, ErrorSeverity::Major);
        assert_eq!(context.metadata.get("key"), Some(&"value".to_string()));
    }
    
    #[test]
    fn test_error_reporter() {
        let reporter = ErrorReporter::new();
        let error = ActorError::MessageHandlingFailed {
            message_type: "test".to_string(),
            reason: "test".to_string(),
        };
        
        reporter.report_error(&error, None);
        let counts = reporter.get_error_counts();
        assert_eq!(counts.get("messaging"), Some(&1));
    }
}