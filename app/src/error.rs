use crate::aura::AuraError;
use bridge::Error as FederationError;
use std::time::SystemTimeError;

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
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
