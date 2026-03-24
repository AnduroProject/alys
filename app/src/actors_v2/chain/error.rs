//! ChainActor V2 Error Types
//!
//! Simplified error handling without custom actor_system dependencies

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("Block production error: {0}")]
    BlockProduction(String),

    #[error("Block validation error: {0}")]
    BlockValidation(String),

    #[error("Block import error: {0}")]
    BlockImport(String),

    #[error("Invalid block: {0}")]
    InvalidBlock(String),

    #[error("AuxPoW processing error: {0}")]
    AuxPowProcessing(String),

    #[error("AuxPoW validation error: {0}")]
    AuxPowValidation(String),

    #[error("Peg operation error: {0}")]
    PegOperation(String),

    #[error("Consensus error: {0}")]
    Consensus(String),

    #[error("Storage actor error: {0}")]
    Storage(String),

    #[error("Network actor error: {0}")]
    Network(crate::actors_v2::network::NetworkError),

    #[error("Sync actor error: {0}")]
    Sync(crate::actors_v2::network::SyncError),

    #[error("Sync actor not set")]
    SyncActorNotSet,

    #[error("Storage actor not set")]
    StorageActorNotSet,

    #[error("Actor mailbox error: {0}")]
    ActorMailbox(String),

    #[error("Network communication error: {0}")]
    NetworkError(String),

    #[error("Network not available")]
    NetworkNotAvailable,

    #[error("Unexpected response type")]
    UnexpectedResponse,

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Chain not synchronized")]
    NotSynced,

    #[error("No work to do - no unfinalized blocks available")]
    NoWorkToDo,

    #[error("Invalid chain state: {0}")]
    InvalidState(String),

    #[error("Bridge operation failed: {0}")]
    Bridge(String),

    #[error("Engine operation failed: {0}")]
    Engine(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Import queue is full - cannot queue more blocks")]
    QueueFull,

    #[error("Invalid block signature: {0}")]
    InvalidSignature(String),

    #[error("Invalid parent relationship: {0}")]
    InvalidParent(String),

    #[error("Orphan block: parent not found (parent_hash={parent_hash}, block_height={block_height})")]
    OrphanBlock {
        parent_hash: ethereum_types::H256,
        block_height: u64,
    },

    #[error("Reorganization too deep: depth {depth} exceeds maximum allowed {max_allowed}")]
    ReorgTooDeep { depth: u64, max_allowed: u64 },

    #[error("Reorganization error: {0}")]
    ReorganizationError(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Governance actor not set - cannot verify peg-ins")]
    GovernanceActorNotSet,

    #[error("Governance verification failed: {0}")]
    GovernanceVerificationFailed(String),

    #[error("Peg-in verification failed for txid {txid}: {reason}")]
    PeginVerificationFailed { txid: String, reason: String },
}

impl From<eyre::Error> for ChainError {
    fn from(err: eyre::Error) -> Self {
        ChainError::Internal(err.to_string())
    }
}

impl From<crate::actors_v2::governance::GovernanceError> for ChainError {
    fn from(err: crate::actors_v2::governance::GovernanceError) -> Self {
        ChainError::GovernanceVerificationFailed(err.to_string())
    }
}
