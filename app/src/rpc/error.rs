//! Unified RPC error handling

use crate::types::errors::{ChainError, AlysError as AuxPowError};
use super::RpcError;

/// Convert ChainActor errors to RPC errors
impl From<ChainError> for RpcError {
    fn from(error: ChainError) -> Self {
        match error {
            ChainError::BlockNotFound => RpcError::block_not_found(),
            ChainError::InvalidHeight(_) => RpcError::invalid_params(),
            ChainError::StorageError(_) => RpcError::internal_error(),
            _ => RpcError::debug_error(format!("Chain error: {:?}", error)),
        }
    }
}

/// Convert AuxPowActor errors to RPC errors  
impl From<AuxPowError> for RpcError {
    fn from(error: AuxPowError) -> Self {
        match error {
            AuxPowError::ChainSyncing => RpcError::chain_syncing(),
            AuxPowError::MiningDisabled => RpcError::mining_disabled(),
            AuxPowError::InvalidPow => RpcError::invalid_params(),
            AuxPowError::UnknownBlock => RpcError::block_not_found(),
            _ => RpcError::debug_error(format!("Mining error: {:?}", error)),
        }
    }
}

/// Convert actix mailbox errors to RPC errors
impl From<actix::MailboxError> for RpcError {
    fn from(error: actix::MailboxError) -> Self {
        RpcError::service_unavailable(&format!("Actor: {}", error))
    }
}