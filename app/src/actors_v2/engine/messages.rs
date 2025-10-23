//! EngineActor V2 Message Protocol
//!
//! Comprehensive message system for execution layer coordination

use actix::prelude::*;
use std::time::Duration;
use uuid::Uuid;

// Re-export types from lighthouse_wrapper and crate
use crate::engine::{AddBalance, ConsensusAmount};
use ethereum_types::H256;
use lighthouse_wrapper::types::{ExecutionBlockHash, ExecutionPayload, MainnetEthSpec, Withdrawal};

use super::EngineError;

/// Engine operation messages
#[derive(Message)]
#[rtype(result = "Result<EngineResponse, EngineError>")]
pub enum EngineMessage {
    /// Build execution payload for block production
    BuildPayload {
        timestamp: Duration,
        parent_hash: Option<ExecutionBlockHash>,
        add_balances: Vec<AddBalance>,
        correlation_id: Option<Uuid>,
    },

    /// Validate execution payload from network
    ValidatePayload {
        payload: ExecutionPayload<MainnetEthSpec>,
        correlation_id: Option<Uuid>,
    },

    /// Commit finalized block to execution layer
    CommitBlock {
        execution_payload: ExecutionPayload<MainnetEthSpec>,
        correlation_id: Option<Uuid>,
    },

    /// Get latest execution block info
    GetLatestBlock { correlation_id: Option<Uuid> },

    /// Update finalized block hash
    SetFinalized {
        block_hash: ExecutionBlockHash,
        correlation_id: Option<Uuid>,
    },

    /// Update fork choice in execution layer
    UpdateForkChoice {
        head_hash: ExecutionBlockHash,
        safe_hash: ExecutionBlockHash,
        finalized_hash: ExecutionBlockHash,
        correlation_id: Option<Uuid>,
    },

    /// Get block with transactions by hash
    GetBlockWithTransactions {
        block_hash: ExecutionBlockHash,
        correlation_id: Option<Uuid>,
    },

    /// Get transaction receipt
    GetTransactionReceipt {
        transaction_hash: H256,
        correlation_id: Option<Uuid>,
    },

    /// Get engine status
    GetStatus { correlation_id: Option<Uuid> },

    /// Shutdown engine gracefully
    Shutdown {
        graceful: bool,
        correlation_id: Option<Uuid>,
    },
}

impl std::fmt::Debug for EngineMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuildPayload {
                timestamp,
                parent_hash,
                add_balances,
                correlation_id,
            } => f
                .debug_struct("BuildPayload")
                .field("timestamp", timestamp)
                .field("parent_hash", parent_hash)
                .field("add_balances_count", &add_balances.len())
                .field("correlation_id", correlation_id)
                .finish(),
            Self::ValidatePayload {
                payload,
                correlation_id,
            } => f
                .debug_struct("ValidatePayload")
                .field("payload", payload)
                .field("correlation_id", correlation_id)
                .finish(),
            Self::CommitBlock {
                execution_payload,
                correlation_id,
            } => f
                .debug_struct("CommitBlock")
                .field("execution_payload", execution_payload)
                .field("correlation_id", correlation_id)
                .finish(),
            Self::GetLatestBlock { correlation_id } => f
                .debug_struct("GetLatestBlock")
                .field("correlation_id", correlation_id)
                .finish(),
            Self::SetFinalized {
                block_hash,
                correlation_id,
            } => f
                .debug_struct("SetFinalized")
                .field("block_hash", block_hash)
                .field("correlation_id", correlation_id)
                .finish(),
            Self::UpdateForkChoice {
                head_hash,
                safe_hash,
                finalized_hash,
                correlation_id,
            } => f
                .debug_struct("UpdateForkChoice")
                .field("head_hash", head_hash)
                .field("safe_hash", safe_hash)
                .field("finalized_hash", finalized_hash)
                .field("correlation_id", correlation_id)
                .finish(),
            Self::GetBlockWithTransactions {
                block_hash,
                correlation_id,
            } => f
                .debug_struct("GetBlockWithTransactions")
                .field("block_hash", block_hash)
                .field("correlation_id", correlation_id)
                .finish(),
            Self::GetTransactionReceipt {
                transaction_hash,
                correlation_id,
            } => f
                .debug_struct("GetTransactionReceipt")
                .field("transaction_hash", transaction_hash)
                .field("correlation_id", correlation_id)
                .finish(),
            Self::GetStatus { correlation_id } => f
                .debug_struct("GetStatus")
                .field("correlation_id", correlation_id)
                .finish(),
            Self::Shutdown {
                graceful,
                correlation_id,
            } => f
                .debug_struct("Shutdown")
                .field("graceful", graceful)
                .field("correlation_id", correlation_id)
                .finish(),
        }
    }
}

/// Engine response types
#[derive(Debug, Clone)]
pub enum EngineResponse {
    PayloadBuilt {
        payload: ExecutionPayload<MainnetEthSpec>,
        build_time: Duration,
    },
    PayloadValid {
        is_valid: bool,
        validation_time: Duration,
    },
    BlockCommitted {
        block_hash: ExecutionBlockHash,
        commit_time: Duration,
    },
    LatestBlock {
        hash: ExecutionBlockHash,
        number: u64,
    },
    FinalizedUpdated {
        block_hash: ExecutionBlockHash,
    },
    ForkChoiceUpdated {
        success: bool,
    },
    BlockWithTransactions {
        block: Option<ethers_core::types::Block<ethers_core::types::Transaction>>,
    },
    TransactionReceipt {
        receipt: Option<ethers_core::types::TransactionReceipt>,
    },
    Status {
        is_ready: bool,
        finalized_block: Option<ExecutionBlockHash>,
        head_block: Option<ExecutionBlockHash>,
    },
    ShutdownComplete,
}

/// Helper functions for creating common messages
impl EngineMessage {
    /// Create BuildPayload message for block production
    pub fn build_payload_for_production(
        timestamp: Duration,
        parent_hash: Option<ExecutionBlockHash>,
        withdrawals: Vec<Withdrawal>,
    ) -> Self {
        let add_balances = withdrawals
            .into_iter()
            .map(|w| AddBalance::from((w.address, ConsensusAmount(w.amount))))
            .collect();

        Self::BuildPayload {
            timestamp,
            parent_hash,
            add_balances,
            correlation_id: Some(Uuid::new_v4()),
        }
    }

    /// Create ValidatePayload message for import validation
    pub fn validate_payload_for_import(payload: ExecutionPayload<MainnetEthSpec>) -> Self {
        Self::ValidatePayload {
            payload,
            correlation_id: Some(Uuid::new_v4()),
        }
    }

    /// Create CommitBlock message for finalization
    pub fn commit_block_for_finalization(payload: ExecutionPayload<MainnetEthSpec>) -> Self {
        Self::CommitBlock {
            execution_payload: payload,
            correlation_id: Some(Uuid::new_v4()),
        }
    }
}
