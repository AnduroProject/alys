use crate::aura::AuraError;
use bridge::Error as FederationError;
use std::time::SystemTimeError;
use strum::Display;
use thiserror::Error;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error, Display)]
pub enum Error {
    PayloadIdUnavailable,
    InvalidBlockHash,
    DbReadError,
    ExecutionLayerError(execution_layer::Error),
    // NOTE: error type not exported by lighthouse
    EngineApiError(String),
    TimeError(SystemTimeError),
    AuraError(AuraError),
    FederationError(FederationError),
    ExecutionHashChainIncontiguous,
    MissingParent,
    InvalidSignature,
    UnknownAuthority,
    CandidateCacheError,
    CodecError,
    InvalidPowRange,
    InvalidBlockRange,
    StorageError,
    MissingRequiredPow,
    MissingBlock,
    ProcessGenesis,
    UnknownWithdrawal,
    PegInAlreadyIncluded,
    IllegalFinalization,
    InvalidBlock,
    InvalidPow,
    ChainSyncing,
    ChainError(ChainError),
}

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("`{0}`")]
    AuxPowMiningError(AuxPowMiningError),
    /// Error retrieving a block hash from the database
    #[error("`{0}`")]
    BlockRetrievalError(BlockErrorBlockTypes),
}

#[derive(Debug, Error)]
pub enum BlockErrorBlockTypes {
    #[error("Failed to retrieve the last finalized block")]
    LastFinalized,
    #[error("Failed to retrieve the head block")]
    Head,
    #[error("Failed to retrieve the previous head block")]
    PreviousHead,
    #[error("Failed to retrieve the block with hash `{0}`")]
    GenericHash(String),
    #[error("Failed to retrieve the block with the height of `{0}`")]
    Height(u64),
    #[error("Failed to retrieve the genesis block")]
    Genesis,
    #[error("Failed to read first block in the auxPoW capture range")]
    AuxPowFirst,
    #[error("Failed to read beginning block in the auxPoW capture range")]
    AuxPowLast,
}

impl From<BlockErrorBlockTypes> for ChainError {
    fn from(e: BlockErrorBlockTypes) -> Self {
        ChainError::BlockRetrievalError(e)
    }
}

#[derive(Debug, Error)]
pub enum AuxPowMiningError {
    #[error("`{0}`")]
    UnknownError(String),
    #[error("`{0}`")]
    HashRetrievalError(BlockErrorBlockTypes),
    #[error("No work to do")]
    NoWorkToDo,
}

impl From<AuxPowMiningError> for Error {
    fn from(e: AuxPowMiningError) -> Self {
        Error::ChainError(ChainError::AuxPowMiningError(e))
    }
}

impl From<SystemTimeError> for Error {
    fn from(e: SystemTimeError) -> Self {
        Error::TimeError(e)
    }
}

impl From<AuraError> for Error {
    fn from(e: AuraError) -> Self {
        Error::AuraError(e)
    }
}

impl From<FederationError> for Error {
    fn from(e: FederationError) -> Self {
        Error::FederationError(e)
    }
}

impl From<execution_layer::Error> for Error {
    fn from(e: execution_layer::Error) -> Self {
        Error::ExecutionLayerError(e)
    }
}