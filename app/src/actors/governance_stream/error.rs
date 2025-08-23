//! Stream error types and error handling for governance communication
//!
//! This module defines comprehensive error types for the governance stream actor,
//! including connection errors, protocol errors, authentication failures, and
//! recovery strategies. All errors follow the Alys V2 error handling patterns
//! using thiserror for consistent error representation.

use crate::types::*;
use thiserror::Error;
use std::time::{Duration, SystemTime};

/// Primary error type for governance stream operations
#[derive(Error, Debug, Clone)]
pub enum StreamError {
    /// Connection-related errors
    #[error("Connection error: {source}")]
    Connection {
        #[from]
        source: ConnectionError,
    },

    /// Protocol-level errors
    #[error("Protocol error: {source}")]
    Protocol {
        #[from]
        source: ProtocolError,
    },

    /// Authentication and authorization errors
    #[error("Authentication error: {source}")]
    Authentication {
        #[from]
        source: AuthenticationError,
    },

    /// Message handling errors
    #[error("Message error: {source}")]
    Message {
        #[from]
        source: MessageError,
    },

    /// Configuration errors
    #[error("Configuration error: {source}")]
    Configuration {
        #[from]
        source: ConfigurationError,
    },

    /// Governance-specific errors
    #[error("Governance error: {source}")]
    Governance {
        #[from]
        source: GovernanceError,
    },

    /// Resource exhaustion errors
    #[error("Resource error: {source}")]
    Resource {
        #[from]
        source: ResourceError,
    },

    /// System-level errors
    #[error("System error: {source}")]
    System {
        #[from]
        source: SystemError,
    },
}

/// Connection-related error types
#[derive(Error, Debug, Clone)]
pub enum ConnectionError {
    /// Failed to establish initial connection
    #[error("Failed to connect to governance endpoint {endpoint}: {reason}")]
    ConnectionFailed { endpoint: String, reason: String },

    /// Connection timed out
    #[error("Connection timeout after {timeout:?} to {endpoint}")]
    ConnectionTimeout { endpoint: String, timeout: Duration },

    /// Connection was rejected by the server
    #[error("Connection rejected by {endpoint}: {reason}")]
    ConnectionRejected { endpoint: String, reason: String },

    /// Connection lost unexpectedly
    #[error("Connection lost to {endpoint}: {reason}")]
    ConnectionLost { endpoint: String, reason: String },

    /// Too many concurrent connections
    #[error("Maximum connections ({max}) exceeded")]
    TooManyConnections { max: usize },

    /// Connection is in invalid state for operation
    #[error("Invalid connection state for operation: {current_state}")]
    InvalidState { current_state: String },

    /// Network-level connectivity issues
    #[error("Network error: {details}")]
    NetworkError { details: String },

    /// DNS resolution failed
    #[error("DNS resolution failed for {hostname}: {reason}")]
    DnsResolutionFailed { hostname: String, reason: String },

    /// TLS/SSL errors
    #[error("TLS error: {details}")]
    TlsError { details: String },

    /// Connection pool exhaustion
    #[error("Connection pool exhausted (pool_size: {pool_size})")]
    PoolExhausted { pool_size: usize },
}

/// Protocol-level error types
#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    /// Unsupported protocol version
    #[error("Unsupported protocol version: {version} (supported: {supported_versions:?})")]
    UnsupportedVersion { version: String, supported_versions: Vec<String> },

    /// Invalid message format
    #[error("Invalid message format: {reason}")]
    InvalidMessageFormat { reason: String },

    /// Message serialization failed
    #[error("Message serialization failed: {message_type} - {reason}")]
    SerializationFailed { message_type: String, reason: String },

    /// Message deserialization failed
    #[error("Message deserialization failed: {reason}")]
    DeserializationFailed { reason: String },

    /// Message validation failed
    #[error("Message validation failed: {validation_error}")]
    ValidationFailed { validation_error: String },

    /// Unsupported message type
    #[error("Unsupported message type: {message_type}")]
    UnsupportedMessageType { message_type: String },

    /// Protocol handshake failed
    #[error("Protocol handshake failed: {reason}")]
    HandshakeFailed { reason: String },

    /// Compression/decompression error
    #[error("Compression error: {details}")]
    CompressionError { details: String },

    /// Stream corruption detected
    #[error("Stream corruption detected: {details}")]
    StreamCorruption { details: String },

    /// Message ordering violation
    #[error("Message ordering violation: expected seq {expected}, got {actual}")]
    OrderingViolation { expected: u64, actual: u64 },
}

/// Authentication and authorization errors
#[derive(Error, Debug, Clone)]
pub enum AuthenticationError {
    /// Authentication failed
    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    /// Invalid credentials provided
    #[error("Invalid credentials: {credential_type}")]
    InvalidCredentials { credential_type: String },

    /// Token has expired
    #[error("Token expired at {expired_at:?}")]
    TokenExpired { expired_at: SystemTime },

    /// Insufficient permissions for operation
    #[error("Insufficient permissions for operation: {operation}")]
    InsufficientPermissions { operation: String },

    /// Token refresh failed
    #[error("Token refresh failed: {reason}")]
    TokenRefreshFailed { reason: String },

    /// Authentication challenge failed
    #[error("Authentication challenge failed: {challenge_type}")]
    ChallengeFailed { challenge_type: String },

    /// Certificate validation failed
    #[error("Certificate validation failed: {reason}")]
    CertificateValidationFailed { reason: String },

    /// Authorization header missing
    #[error("Authorization header missing")]
    MissingAuthorizationHeader,

    /// Invalid token format
    #[error("Invalid token format: {format_error}")]
    InvalidTokenFormat { format_error: String },

    /// Authentication method not supported
    #[error("Authentication method not supported: {method}")]
    UnsupportedAuthMethod { method: String },
}

/// Message handling errors
#[derive(Error, Debug, Clone)]
pub enum MessageError {
    /// Message buffer overflow
    #[error("Message buffer overflow (capacity: {capacity})")]
    BufferOverflow { capacity: usize },

    /// Message queue full
    #[error("Message queue full (size: {size})")]
    QueueFull { size: usize },

    /// Message send failed
    #[error("Failed to send message: {reason}")]
    SendFailed { reason: String },

    /// Message receive failed
    #[error("Failed to receive message: {reason}")]
    ReceiveFailed { reason: String },

    /// Message timeout
    #[error("Message timeout after {timeout:?}")]
    MessageTimeout { timeout: Duration },

    /// Message TTL expired
    #[error("Message TTL expired (age: {age:?}, ttl: {ttl:?})")]
    TtlExpired { age: Duration, ttl: Duration },

    /// Duplicate message detected
    #[error("Duplicate message detected: {message_id}")]
    DuplicateMessage { message_id: String },

    /// Message correlation failed
    #[error("Message correlation failed: {correlation_id}")]
    CorrelationFailed { correlation_id: String },

    /// Message routing failed
    #[error("Message routing failed: {destination}")]
    RoutingFailed { destination: String },

    /// Message priority violation
    #[error("Message priority violation: {details}")]
    PriorityViolation { details: String },
}

/// Configuration error types
#[derive(Error, Debug, Clone)]
pub enum ConfigurationError {
    /// Invalid configuration parameter
    #[error("Invalid configuration parameter: {parameter} - {reason}")]
    InvalidParameter { parameter: String, reason: String },

    /// Missing required configuration
    #[error("Missing required configuration: {config_key}")]
    MissingRequired { config_key: String },

    /// Configuration validation failed
    #[error("Configuration validation failed: {validation_errors:?}")]
    ValidationFailed { validation_errors: Vec<String> },

    /// Configuration file not found
    #[error("Configuration file not found: {file_path}")]
    FileNotFound { file_path: String },

    /// Configuration parse error
    #[error("Configuration parse error: {parse_error}")]
    ParseError { parse_error: String },

    /// Incompatible configuration version
    #[error("Incompatible configuration version: {version}")]
    IncompatibleVersion { version: String },

    /// Configuration lock failed
    #[error("Configuration lock failed: {reason}")]
    LockFailed { reason: String },

    /// Configuration update conflict
    #[error("Configuration update conflict: {conflict_details}")]
    UpdateConflict { conflict_details: String },
}

/// Governance-specific error types
#[derive(Error, Debug, Clone)]
pub enum GovernanceError {
    /// Governance node unavailable
    #[error("Governance node unavailable: {node_id}")]
    NodeUnavailable { node_id: String },

    /// Signature request failed
    #[error("Signature request failed: {request_id} - {reason}")]
    SignatureRequestFailed { request_id: String, reason: String },

    /// Signature collection timeout
    #[error("Signature collection timeout for request: {request_id} (timeout: {timeout:?})")]
    SignatureTimeout { request_id: String, timeout: Duration },

    /// Insufficient signatures collected
    #[error("Insufficient signatures: {collected}/{required} for request {request_id}")]
    InsufficientSignatures { request_id: String, collected: usize, required: usize },

    /// Federation update failed
    #[error("Federation update failed: {reason}")]
    FederationUpdateFailed { reason: String },

    /// Proposal submission failed
    #[error("Proposal submission failed: {proposal_id} - {reason}")]
    ProposalSubmissionFailed { proposal_id: String, reason: String },

    /// Governance consensus failed
    #[error("Governance consensus failed: {consensus_round}")]
    ConsensusFailed { consensus_round: u64 },

    /// Emergency action rejected
    #[error("Emergency action rejected: {action_type} - {reason}")]
    EmergencyActionRejected { action_type: String, reason: String },

    /// Quorum not reached
    #[error("Quorum not reached: {current_votes}/{required_votes}")]
    QuorumNotReached { current_votes: u32, required_votes: u32 },

    /// Governance node conflict
    #[error("Governance node conflict: {conflict_details}")]
    NodeConflict { conflict_details: String },
}

/// Resource exhaustion errors
#[derive(Error, Debug, Clone)]
pub enum ResourceError {
    /// Memory allocation failed
    #[error("Memory allocation failed: {requested_bytes} bytes")]
    MemoryExhausted { requested_bytes: u64 },

    /// CPU resources exhausted
    #[error("CPU resources exhausted: {cpu_usage}%")]
    CpuExhausted { cpu_usage: f64 },

    /// Network bandwidth exhausted
    #[error("Network bandwidth exhausted: {current_usage}/{limit} bytes/sec")]
    BandwidthExhausted { current_usage: u64, limit: u64 },

    /// File descriptor limit reached
    #[error("File descriptor limit reached: {current}/{limit}")]
    FileDescriptorLimit { current: u32, limit: u32 },

    /// Thread pool exhausted
    #[error("Thread pool exhausted: {active_threads}/{max_threads}")]
    ThreadPoolExhausted { active_threads: usize, max_threads: usize },

    /// Disk space exhausted
    #[error("Disk space exhausted: {available_bytes} bytes available")]
    DiskSpaceExhausted { available_bytes: u64 },

    /// Resource timeout
    #[error("Resource acquisition timeout: {resource_type} after {timeout:?}")]
    ResourceTimeout { resource_type: String, timeout: Duration },

    /// Resource lock contention
    #[error("Resource lock contention: {resource_id}")]
    LockContention { resource_id: String },
}

/// System-level errors
#[derive(Error, Debug, Clone)]
pub enum SystemError {
    /// I/O operation failed
    #[error("I/O error: {operation} - {reason}")]
    IoError { operation: String, reason: String },

    /// System call failed
    #[error("System call failed: {syscall} - {error_code}")]
    SystemCallFailed { syscall: String, error_code: i32 },

    /// Process signal received
    #[error("Process signal received: {signal}")]
    SignalReceived { signal: String },

    /// System shutdown initiated
    #[error("System shutdown initiated: {reason}")]
    ShutdownInitiated { reason: String },

    /// Service unavailable
    #[error("Service unavailable: {service_name} - {reason}")]
    ServiceUnavailable { service_name: String, reason: String },

    /// Database error
    #[error("Database error: {operation} - {details}")]
    DatabaseError { operation: String, details: String },

    /// External service error
    #[error("External service error: {service} - {error}")]
    ExternalServiceError { service: String, error: String },

    /// Backup operation failed
    #[error("Backup operation failed: {backup_type} - {reason}")]
    BackupFailed { backup_type: String, reason: String },

    /// Recovery operation failed
    #[error("Recovery operation failed: {recovery_type} - {reason}")]
    RecoveryFailed { recovery_type: String, reason: String },

    /// Health check failed
    #[error("Health check failed: {check_name} - {details}")]
    HealthCheckFailed { check_name: String, details: String },
}

/// Error context for enhanced debugging and monitoring
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Operation that caused the error
    pub operation: String,
    /// Connection ID if applicable
    pub connection_id: Option<String>,
    /// Request ID if applicable
    pub request_id: Option<String>,
    /// Governance node ID if applicable
    pub node_id: Option<String>,
    /// Timestamp when error occurred
    pub timestamp: SystemTime,
    /// Additional context metadata
    pub metadata: std::collections::HashMap<String, String>,
    /// Error correlation ID for distributed tracing
    pub correlation_id: Option<uuid::Uuid>,
    /// Stack trace if available
    pub stack_trace: Option<String>,
}

/// Error recovery strategy
#[derive(Debug, Clone)]
pub enum ErrorRecoveryStrategy {
    /// No recovery possible, operation failed permanently
    NoRecovery,
    /// Retry operation with same parameters
    Retry { 
        max_attempts: u32, 
        delay: Duration,
        backoff_multiplier: Option<f64>,
    },
    /// Retry with different parameters or endpoints
    RetryWithAlternatives {
        alternatives: Vec<String>,
        max_attempts_per_alternative: u32,
    },
    /// Fallback to alternative method
    Fallback {
        fallback_method: String,
        parameters: std::collections::HashMap<String, String>,
    },
    /// Escalate to higher-level handler
    Escalate {
        escalation_target: String,
        escalation_data: std::collections::HashMap<String, String>,
    },
    /// Graceful degradation
    GracefulDegradation {
        degraded_service_level: String,
        impact_description: String,
    },
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Low impact, operation can continue
    Low = 0,
    /// Medium impact, some functionality affected
    Medium = 1,
    /// High impact, significant functionality affected
    High = 2,
    /// Critical impact, service severely degraded
    Critical = 3,
    /// Fatal error, service must be stopped
    Fatal = 4,
}

impl StreamError {
    /// Create error with context
    pub fn with_context(mut self, context: ErrorContext) -> EnhancedStreamError {
        EnhancedStreamError {
            error: self,
            context,
            recovery_strategy: self.default_recovery_strategy(),
            severity: self.default_severity(),
        }
    }

    /// Get default recovery strategy for this error type
    pub fn default_recovery_strategy(&self) -> ErrorRecoveryStrategy {
        match self {
            StreamError::Connection { source } => {
                match source {
                    ConnectionError::ConnectionFailed { .. } 
                    | ConnectionError::ConnectionTimeout { .. }
                    | ConnectionError::NetworkError { .. } => {
                        ErrorRecoveryStrategy::Retry {
                            max_attempts: 5,
                            delay: Duration::from_secs(1),
                            backoff_multiplier: Some(2.0),
                        }
                    }
                    ConnectionError::TooManyConnections { .. } => {
                        ErrorRecoveryStrategy::GracefulDegradation {
                            degraded_service_level: "reduced_connections".to_string(),
                            impact_description: "Some connections may be queued".to_string(),
                        }
                    }
                    _ => ErrorRecoveryStrategy::NoRecovery,
                }
            }
            StreamError::Authentication { .. } => {
                ErrorRecoveryStrategy::Fallback {
                    fallback_method: "token_refresh".to_string(),
                    parameters: std::collections::HashMap::new(),
                }
            }
            StreamError::Message { source } => {
                match source {
                    MessageError::MessageTimeout { .. } => {
                        ErrorRecoveryStrategy::Retry {
                            max_attempts: 3,
                            delay: Duration::from_millis(500),
                            backoff_multiplier: Some(1.5),
                        }
                    }
                    _ => ErrorRecoveryStrategy::NoRecovery,
                }
            }
            _ => ErrorRecoveryStrategy::NoRecovery,
        }
    }

    /// Get default severity for this error type
    pub fn default_severity(&self) -> ErrorSeverity {
        match self {
            StreamError::Connection { .. } => ErrorSeverity::High,
            StreamError::Authentication { .. } => ErrorSeverity::High,
            StreamError::Protocol { .. } => ErrorSeverity::Medium,
            StreamError::Message { .. } => ErrorSeverity::Medium,
            StreamError::Configuration { .. } => ErrorSeverity::Critical,
            StreamError::Governance { .. } => ErrorSeverity::High,
            StreamError::Resource { .. } => ErrorSeverity::Critical,
            StreamError::System { .. } => ErrorSeverity::Fatal,
        }
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        !matches!(self.default_recovery_strategy(), ErrorRecoveryStrategy::NoRecovery)
    }

    /// Check if error requires immediate attention
    pub fn requires_immediate_attention(&self) -> bool {
        self.default_severity() >= ErrorSeverity::Critical
    }
}

/// Enhanced error with context and recovery information
#[derive(Debug, Clone)]
pub struct EnhancedStreamError {
    /// The original error
    pub error: StreamError,
    /// Error context
    pub context: ErrorContext,
    /// Recovery strategy
    pub recovery_strategy: ErrorRecoveryStrategy,
    /// Error severity
    pub severity: ErrorSeverity,
}

impl std::fmt::Display for EnhancedStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} (operation: {}, severity: {:?})",
            self.context.timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            self.error,
            self.context.operation,
            self.severity
        )
    }
}

impl std::error::Error for EnhancedStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl ErrorContext {
    /// Create new error context
    pub fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            connection_id: None,
            request_id: None,
            node_id: None,
            timestamp: SystemTime::now(),
            metadata: std::collections::HashMap::new(),
            correlation_id: None,
            stack_trace: None,
        }
    }

    /// Add connection ID to context
    pub fn with_connection_id(mut self, connection_id: String) -> Self {
        self.connection_id = Some(connection_id);
        self
    }

    /// Add request ID to context
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Add governance node ID to context
    pub fn with_node_id(mut self, node_id: String) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Add metadata to context
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Add correlation ID for distributed tracing
    pub fn with_correlation_id(mut self, correlation_id: uuid::Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}

impl Default for ErrorSeverity {
    fn default() -> Self {
        ErrorSeverity::Medium
    }
}

/// Result type alias for stream operations
pub type StreamResult<T> = Result<T, StreamError>;

/// Enhanced result type with error context
pub type EnhancedStreamResult<T> = Result<T, EnhancedStreamError>;

/// Convenience macro for creating errors with context
#[macro_export]
macro_rules! stream_error {
    ($error:expr, $operation:expr) => {
        $error.with_context(ErrorContext::new($operation))
    };
    ($error:expr, $operation:expr, $connection_id:expr) => {
        $error.with_context(
            ErrorContext::new($operation)
                .with_connection_id($connection_id.to_string())
        )
    };
    ($error:expr, $operation:expr, $connection_id:expr, $request_id:expr) => {
        $error.with_context(
            ErrorContext::new($operation)
                .with_connection_id($connection_id.to_string())
                .with_request_id($request_id.to_string())
        )
    };
}

/// Type alias for connection management errors
pub type ConnectionResult<T> = Result<T, ConnectionError>;

/// Type alias for protocol errors  
pub type ProtocolResult<T> = Result<T, ProtocolError>;

/// Type alias for authentication errors
pub type AuthenticationResult<T> = Result<T, AuthenticationError>;