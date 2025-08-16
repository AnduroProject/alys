//! Error types for the Alys actor system

use std::fmt;
use serde::{Deserialize, Serialize};

/// System-level errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemError {
    ActorNotFound { actor_name: String },
    ActorStartupFailed { actor_name: String, reason: String },
    ActorCommunicationFailed { from: String, to: String, reason: String },
    ConfigurationError { parameter: String, reason: String },
    ResourceExhausted { resource: String },
    ShutdownTimeout { timeout: std::time::Duration },
    InvalidState { expected: String, actual: String },
    PermissionDenied { operation: String },
}

/// Chain-related errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainError {
    // Block errors
    InvalidBlock { reason: String },
    BlockNotFound { block_hash: String },
    InvalidParentBlock { parent_hash: String },
    BlockTooOld { block_number: u64, current: u64 },
    BlockTooNew { block_number: u64, current: u64 },
    
    // Transaction errors
    InvalidTransaction { tx_hash: String, reason: String },
    TransactionNotFound { tx_hash: String },
    InsufficientBalance { address: String, required: u64, available: u64 },
    NonceError { address: String, expected: u64, got: u64 },
    GasLimitExceeded { limit: u64, required: u64 },
    
    // State errors
    StateUpdateFailed { reason: String },
    StateRootMismatch { expected: String, actual: String },
    
    // Consensus errors
    NotValidator,
    InvalidSignature,
    ConsensusFailure { reason: String },
    
    // Validation errors
    ValidationFailed { reason: String },
    ExecutionFailed { reason: String },
    
    // General errors
    NotImplemented,
    TooEarly,
    NoParentBlock,
}

/// Network-related errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkError {
    // Connection errors
    ConnectionFailed { peer_id: String, reason: String },
    PeerNotFound { peer_id: String },
    PeerNotConnected,
    TooManyConnections { limit: usize },
    ConnectionTimeout { timeout: std::time::Duration },
    
    // Message errors
    InvalidMessage { reason: String },
    MessageTooLarge { size: usize, limit: usize },
    SerializationFailed { reason: String },
    DeserializationFailed { reason: String },
    
    // Topic errors
    TopicNotFound { topic: String },
    NotSubscribed,
    SubscriptionFailed { topic: String, reason: String },
    
    // Protocol errors
    ProtocolError { protocol: String, reason: String },
    UnsupportedProtocol { protocol: String },
    
    // DHT errors
    DhtError { operation: String, reason: String },
    KeyNotFound { key: String },
    
    // Rate limiting
    RateLimited { limit: u32, retry_after: std::time::Duration },
}

/// Synchronization errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncError {
    // Peer errors
    NoPeersAvailable,
    PeerMisbehavior { peer_id: String, reason: String },
    PeerTimeout { peer_id: String },
    
    // Download errors
    DownloadFailed { item: String, reason: String },
    InvalidData { data_type: String, reason: String },
    VerificationFailed { item: String, reason: String },
    
    // State sync errors
    StateDataMissing { state_root: String },
    StateVerificationFailed { reason: String },
    
    // General sync errors
    SyncStalled { reason: String },
    SyncAborted { reason: String },
    TargetUnreachable { target_block: u64, reason: String },
}

/// Storage-related errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageError {
    // Database errors
    DatabaseConnectionFailed { path: String, reason: String },
    DatabaseCorrupted { database: String },
    DatabaseLocked { database: String },
    
    // Operation errors
    ReadFailed { key: String, reason: String },
    WriteFailed { key: String, reason: String },
    DeleteFailed { key: String, reason: String },
    
    // Batch operation errors
    BatchOperationFailed { operation_count: usize, reason: String },
    TransactionFailed { reason: String },
    
    // Space errors
    InsufficientSpace { required: u64, available: u64 },
    DiskFull,
    
    // Data integrity errors
    ChecksumMismatch { expected: String, actual: String },
    DataCorruption { item: String },
    
    // Index errors
    IndexCorrupted { index: String },
    IndexRebuildRequired { index: String },
    
    // Snapshot errors
    SnapshotFailed { reason: String },
    SnapshotNotFound { snapshot: String },
    RestoreFailed { snapshot: String, reason: String },
}

/// Streaming-related errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamError {
    // Connection errors
    ConnectionNotFound,
    TooManyConnections,
    AuthenticationFailed { reason: String },
    
    // Subscription errors
    TopicNotFound { topic: String },
    SubscriptionLimitExceeded { limit: u32 },
    InvalidFilter { reason: String },
    
    // Message errors
    MessageTooLarge { size: usize, limit: usize },
    EncodingFailed { reason: String },
    SendFailed { reason: String },
    
    // Rate limiting
    RateLimitExceeded { limit: u32 },
    
    // WebSocket errors
    WebSocketError { reason: String },
    ProtocolViolation { reason: String },
}

/// Bridge-related errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeError {
    // Bitcoin errors
    BitcoinNodeError { reason: String },
    BitcoinTransactionInvalid { tx_id: String, reason: String },
    InsufficientConfirmations { required: u32, current: u32 },
    
    // Federation errors
    FederationNotReady { reason: String },
    InsufficientSignatures { required: usize, collected: usize },
    SignatureTimeout { timeout: std::time::Duration },
    InvalidSignature { signer: String, reason: String },
    
    // Peg operation errors
    PegInFailed { bitcoin_tx: String, reason: String },
    PegOutFailed { burn_tx: String, reason: String },
    AmountTooLow,
    AmountTooHigh,
    InvalidBitcoinAddress,
    NoRelevantOutputs,
    
    // UTXO errors
    InsufficientUtxos { required: u64, available: u64 },
    UtxoSelectionFailed { reason: String },
    
    // Security errors
    ReorgDetected { depth: u32 },
    SuspiciousActivity { reason: String },
    EmergencyPause { reason: String },
    
    // Fee errors
    FeeEstimationFailed { reason: String },
    FeeTooHigh { fee: u64, limit: u64 },
}

/// Engine (execution layer) errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineError {
    // Connection errors
    ExecutionClientOffline,
    ConnectionFailed { url: String, reason: String },
    AuthenticationFailed,
    
    // Payload errors
    PayloadBuildFailed { reason: String },
    PayloadNotFound,
    InvalidPayload { reason: String },
    
    // Execution errors
    ExecutionFailed { reason: String },
    StateTransitionFailed { reason: String },
    GasEstimationFailed { reason: String },
    
    // RPC errors
    RpcError { method: String, reason: String },
    RpcTimeout { method: String, timeout: std::time::Duration },
}

/// General error wrapper that can hold any specific error type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlysError {
    System(SystemError),
    Chain(ChainError),
    Network(NetworkError),
    Sync(SyncError),
    Storage(StorageError),
    Stream(StreamError),
    Bridge(BridgeError),
    Engine(EngineError),
    
    // Generic errors
    Internal { message: String },
    Configuration { parameter: String, message: String },
    Validation { field: String, message: String },
    NotFound { item: String },
    AlreadyExists { item: String },
    Timeout { operation: String, timeout: std::time::Duration },
    Unavailable { service: String, reason: String },
}

// Implement Display trait for better error messages
impl fmt::Display for SystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SystemError::ActorNotFound { actor_name } => {
                write!(f, "Actor '{}' not found", actor_name)
            }
            SystemError::ActorStartupFailed { actor_name, reason } => {
                write!(f, "Failed to start actor '{}': {}", actor_name, reason)
            }
            SystemError::ActorCommunicationFailed { from, to, reason } => {
                write!(f, "Communication failed from '{}' to '{}': {}", from, to, reason)
            }
            SystemError::ConfigurationError { parameter, reason } => {
                write!(f, "Configuration error for '{}': {}", parameter, reason)
            }
            SystemError::ResourceExhausted { resource } => {
                write!(f, "Resource '{}' exhausted", resource)
            }
            SystemError::ShutdownTimeout { timeout } => {
                write!(f, "Shutdown timeout after {:?}", timeout)
            }
            SystemError::InvalidState { expected, actual } => {
                write!(f, "Invalid state: expected '{}', got '{}'", expected, actual)
            }
            SystemError::PermissionDenied { operation } => {
                write!(f, "Permission denied for operation '{}'", operation)
            }
        }
    }
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainError::InvalidBlock { reason } => {
                write!(f, "Invalid block: {}", reason)
            }
            ChainError::BlockNotFound { block_hash } => {
                write!(f, "Block not found: {}", block_hash)
            }
            ChainError::InvalidTransaction { tx_hash, reason } => {
                write!(f, "Invalid transaction {}: {}", tx_hash, reason)
            }
            ChainError::InsufficientBalance { address, required, available } => {
                write!(f, "Insufficient balance for {}: required {}, available {}", address, required, available)
            }
            ChainError::ValidationFailed { reason } => {
                write!(f, "Validation failed: {}", reason)
            }
            ChainError::NotValidator => {
                write!(f, "Node is not a validator")
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::ConnectionFailed { peer_id, reason } => {
                write!(f, "Connection failed to peer '{}': {}", peer_id, reason)
            }
            NetworkError::PeerNotFound { peer_id } => {
                write!(f, "Peer '{}' not found", peer_id)
            }
            NetworkError::TooManyConnections { limit } => {
                write!(f, "Too many connections (limit: {})", limit)
            }
            NetworkError::InvalidMessage { reason } => {
                write!(f, "Invalid message: {}", reason)
            }
            NetworkError::RateLimited { limit, retry_after } => {
                write!(f, "Rate limited (limit: {}, retry after: {:?})", limit, retry_after)
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::BitcoinNodeError { reason } => {
                write!(f, "Bitcoin node error: {}", reason)
            }
            BridgeError::InsufficientSignatures { required, collected } => {
                write!(f, "Insufficient signatures: need {}, have {}", required, collected)
            }
            BridgeError::PegInFailed { bitcoin_tx, reason } => {
                write!(f, "Peg-in failed for transaction {}: {}", bitcoin_tx, reason)
            }
            BridgeError::PegOutFailed { burn_tx, reason } => {
                write!(f, "Peg-out failed for burn transaction {}: {}", burn_tx, reason)
            }
            BridgeError::AmountTooLow => {
                write!(f, "Amount below minimum threshold")
            }
            BridgeError::AmountTooHigh => {
                write!(f, "Amount above maximum threshold")
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

// Implement std::error::Error trait for all error types
impl std::error::Error for SystemError {}
impl std::error::Error for ChainError {}
impl std::error::Error for NetworkError {}
impl std::error::Error for SyncError {}
impl std::error::Error for StorageError {}
impl std::error::Error for StreamError {}
impl std::error::Error for BridgeError {}
impl std::error::Error for EngineError {}
impl std::error::Error for AlysError {}

// Conversion traits for easier error handling
impl From<SystemError> for AlysError {
    fn from(err: SystemError) -> Self {
        AlysError::System(err)
    }
}

impl From<ChainError> for AlysError {
    fn from(err: ChainError) -> Self {
        AlysError::Chain(err)
    }
}

impl From<NetworkError> for AlysError {
    fn from(err: NetworkError) -> Self {
        AlysError::Network(err)
    }
}

impl From<SyncError> for AlysError {
    fn from(err: SyncError) -> Self {
        AlysError::Sync(err)
    }
}

impl From<StorageError> for AlysError {
    fn from(err: StorageError) -> Self {
        AlysError::Storage(err)
    }
}

impl From<StreamError> for AlysError {
    fn from(err: StreamError) -> Self {
        AlysError::Stream(err)
    }
}

impl From<BridgeError> for AlysError {
    fn from(err: BridgeError) -> Self {
        AlysError::Bridge(err)
    }
}

impl From<EngineError> for AlysError {
    fn from(err: EngineError) -> Self {
        AlysError::Engine(err)
    }
}

// Helper macro for creating errors with context
#[macro_export]
macro_rules! chain_error {
    ($reason:expr) => {
        ChainError::ValidationFailed { reason: $reason.to_string() }
    };
    ($variant:ident, $($field:ident: $value:expr),+ $(,)?) => {
        ChainError::$variant { $($field: $value),+ }
    };
}

#[macro_export]
macro_rules! network_error {
    ($reason:expr) => {
        NetworkError::InvalidMessage { reason: $reason.to_string() }
    };
    ($variant:ident, $($field:ident: $value:expr),+ $(,)?) => {
        NetworkError::$variant { $($field: $value),+ }
    };
}

// Result type aliases for convenience
pub type SystemResult<T> = Result<T, SystemError>;
pub type ChainResult<T> = Result<T, ChainError>;
pub type NetworkResult<T> = Result<T, NetworkError>;
pub type SyncResult<T> = Result<T, SyncError>;
pub type StorageResult<T> = Result<T, StorageError>;
pub type StreamResult<T> = Result<T, StreamError>;
pub type BridgeResult<T> = Result<T, BridgeError>;
pub type EngineResult<T> = Result<T, EngineError>;
pub type AlysResult<T> = Result<T, AlysError>;