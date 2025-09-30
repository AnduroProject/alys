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

    #[error("Invalid chain state: {0}")]
    InvalidState(String),

    #[error("Bridge operation failed: {0}")]
    Bridge(String),

    #[error("Engine operation failed: {0}")]
    Engine(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<eyre::Error> for ChainError {
    fn from(err: eyre::Error) -> Self {
        ChainError::Internal(err.to_string())
    }
}