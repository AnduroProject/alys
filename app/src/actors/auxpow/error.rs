//! Error types for V2 AuxPow system
//!
//! Provides complete error coverage matching legacy AuxPowMiner errors

use std::fmt;
use thiserror::Error;

/// AuxPow operation errors with exact legacy parity
#[derive(Error, Debug, Clone)]
pub enum AuxPowError {
    /// Chain is currently syncing (legacy: Error::ChainSyncing)
    #[error("Chain is currently syncing")]
    ChainSyncing,
    
    /// Failed to retrieve required hashes
    #[error("Hash retrieval error")]
    HashRetrievalError,
    
    /// Submitted AuxPow for unknown block hash
    #[error("Submitted AuxPow for unknown block")]
    UnknownBlock,
    
    /// Last block not found in chain
    #[error("Last block not found")]
    LastBlockNotFound,
    
    /// Proof of work validation failed
    #[error("POW is not valid")]
    InvalidPow,
    
    /// AuxPow structure validation failed  
    #[error("AuxPow is not valid")]
    InvalidAuxpow,
    
    /// Communication with ChainActor failed
    #[error("Chain actor communication error")]
    ChainCommunicationError,
    
    /// General chain operation error
    #[error("Chain operation error")]
    ChainError,
    
    /// Difficulty calculation failed
    #[error("Difficulty calculation error: {0}")]
    DifficultyCalculationError(String),
    
    /// Mining is disabled
    #[error("Mining is disabled")]
    MiningDisabled,
    
    /// Invalid mining address format
    #[error("Invalid mining address: {0}")]
    InvalidMiningAddress(String),
}

/// Difficulty management errors
#[derive(Error, Debug, Clone)]
pub enum DifficultyError {
    /// Consensus parameter validation failed
    #[error("Invalid consensus parameters: {0}")]
    InvalidConsensusParams(String),
    
    /// History storage operation failed
    #[error("History storage error: {0}")]
    HistoryStorageError(String),
    
    /// Difficulty calculation overflow
    #[error("Difficulty calculation overflow")]
    CalculationOverflow,
    
    /// Invalid height for retargeting
    #[error("Invalid retarget height: {0}")]
    InvalidRetargetHeight(u64),
    
    /// Storage actor communication failed
    #[error("Storage communication error")]
    StorageCommunicationError,
}

/// Convenience type for AuxPow results
pub type AuxPowResult<T> = Result<T, AuxPowError>;

/// Convenience type for difficulty results
pub type DifficultyResult<T> = Result<T, DifficultyError>;