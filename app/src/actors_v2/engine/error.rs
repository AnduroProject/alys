//! EngineActor Error Types
//!
//! Comprehensive error handling for execution layer operations


/// EngineActor error types
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("Engine API error: {0}")]
    EngineApi(String),

    #[error("Invalid block hash")]
    InvalidBlockHash,

    #[error("Payload ID unavailable")]
    PayloadIdUnavailable,

    #[error("Invalid execution payload: {0}")]
    InvalidExecutionPayload(String),

    #[error("Finalized block not found")]
    FinalizedBlockNotFound,

    #[error("Fork choice update failed: {0}")]
    ForkChoiceUpdateFailed(String),

    #[error("Block building failed: {0}")]
    BlockBuildingFailed(String),

    #[error("Block validation failed: {0}")]
    BlockValidationFailed(String),

    #[error("Timeout waiting for engine response")]
    Timeout,

    #[error("Engine not ready")]
    NotReady,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

impl From<crate::error::Error> for EngineError {
    fn from(error: crate::error::Error) -> Self {
        match error {
            crate::error::Error::EngineApiError(msg) => EngineError::EngineApi(msg),
            crate::error::Error::InvalidBlockHash => EngineError::InvalidBlockHash,
            crate::error::Error::PayloadIdUnavailable => EngineError::PayloadIdUnavailable,
            _ => EngineError::Internal(error.to_string()),
        }
    }
}