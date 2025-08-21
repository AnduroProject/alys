use thiserror::Error;
use actix::MailboxError;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("Insufficient confirmations: got {got}, required {required}")]
    InsufficientConfirmations { got: u32, required: u32 },

    #[error("Invalid deposit address: expected {expected}, got {got}")]
    InvalidDepositAddress { expected: String, got: String },

    #[error("No EVM address found in OP_RETURN output")]
    NoEvmAddress,

    #[error("Invalid EVM address format: {0}")]
    InvalidEvmAddress(String),

    #[error("Invalid Bitcoin address: {0}")]
    InvalidAddress(String),

    #[error("Amount too large: {amount}, maximum allowed: {max}")]
    AmountTooLarge { amount: u64, max: u64 },

    #[error("Insufficient funds: needed {needed}, available {available}")]
    InsufficientFunds { needed: u64, available: u64 },

    #[error("Operation not found: {0}")]
    OperationNotFound(String),

    #[error("Invalid witness index: {index}, transaction has {count} inputs")]
    InvalidWitnessIndex { index: usize, count: usize },

    #[error("Bitcoin broadcast failed: {0}")]
    BroadcastFailed(String),

    #[error("UTXO selection failed: {0}")]
    UtxoSelectionFailed(String),

    #[error("Transaction building failed: {0}")]
    TransactionBuildingFailed(String),

    #[error("Bitcoin RPC error: {0}")]
    BitcoinRpcError(String),

    #[error("Governance communication error: {0}")]
    GovernanceError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Timeout error: operation timed out after {seconds} seconds")]
    TimeoutError { seconds: u64 },

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Actor mailbox error: {0}")]
    MailboxError(#[from] MailboxError),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Operation already exists: {0}")]
    OperationAlreadyExists(String),

    #[error("Invalid operation state: current={current}, attempted={attempted}")]
    InvalidOperationState { current: String, attempted: String },

    #[error("Maximum retries exceeded: {max_retries}")]
    MaxRetriesExceeded { max_retries: u32 },

    #[error("Fee estimation failed: {0}")]
    FeeEstimationFailed(String),

    #[error("Script validation failed: {0}")]
    ScriptValidationFailed(String),

    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    #[error("Federation update failed: {0}")]
    FederationUpdateFailed(String),
}

impl BridgeError {
    /// Returns true if the error is recoverable and the operation can be retried
    pub fn is_recoverable(&self) -> bool {
        match self {
            BridgeError::NetworkError(_) => true,
            BridgeError::BitcoinRpcError(_) => true,
            BridgeError::GovernanceError(_) => true,
            BridgeError::TimeoutError { .. } => true,
            BridgeError::DatabaseError(_) => true,
            BridgeError::BroadcastFailed(_) => true,
            BridgeError::FeeEstimationFailed(_) => true,
            _ => false,
        }
    }

    /// Returns the error severity level for monitoring and alerting
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            BridgeError::InternalError(_) => ErrorSeverity::Critical,
            BridgeError::DatabaseError(_) => ErrorSeverity::Critical,
            BridgeError::ConfigurationError(_) => ErrorSeverity::Critical,
            BridgeError::FederationUpdateFailed(_) => ErrorSeverity::Critical,
            
            BridgeError::BitcoinRpcError(_) => ErrorSeverity::High,
            BridgeError::GovernanceError(_) => ErrorSeverity::High,
            BridgeError::BroadcastFailed(_) => ErrorSeverity::High,
            BridgeError::InsufficientFunds { .. } => ErrorSeverity::High,
            
            BridgeError::NetworkError(_) => ErrorSeverity::Medium,
            BridgeError::TimeoutError { .. } => ErrorSeverity::Medium,
            BridgeError::UtxoSelectionFailed(_) => ErrorSeverity::Medium,
            BridgeError::FeeEstimationFailed(_) => ErrorSeverity::Medium,
            
            _ => ErrorSeverity::Low,
        }
    }

    /// Returns a user-friendly error message for display purposes
    pub fn user_message(&self) -> &str {
        match self {
            BridgeError::InsufficientConfirmations { .. } => "Transaction needs more confirmations",
            BridgeError::InvalidDepositAddress { .. } => "Invalid deposit address",
            BridgeError::AmountTooLarge { .. } => "Amount exceeds maximum limit",
            BridgeError::InsufficientFunds { .. } => "Insufficient funds for transaction",
            BridgeError::NetworkError(_) => "Network connection error",
            BridgeError::TimeoutError { .. } => "Operation timed out",
            _ => "Internal processing error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ErrorSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorSeverity::Low => "low",
            ErrorSeverity::Medium => "medium", 
            ErrorSeverity::High => "high",
            ErrorSeverity::Critical => "critical",
        }
    }
}