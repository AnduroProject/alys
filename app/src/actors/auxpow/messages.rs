//! Message definitions for V2 AuxPow system
//!
//! Provides complete message coverage for mining operations with exact
//! legacy function parity: create_aux_block, submit_aux_block, etc.

use actix::prelude::*;
use bitcoin::{BlockHash, CompactTarget};
use ethereum_types::Address as EvmAddress;
use std::time::Duration;

use crate::{
    actors::auxpow::types::AuxPow,
    actors::auxpow::config::AuxBlock,
    types::blockchain::{AuxPowHeader, ConsensusBlock, SignedConsensusBlock},
    types::errors::{ChainError, StorageError},
    types::*,
};

use super::{AuxPowError, DifficultyError};

// ============================================================================
// AuxPowActor Messages - Direct Legacy Function Ports
// ============================================================================

/// Direct port of legacy create_aux_block function
/// 
/// Creates new mining work for external miners or internal mining loop.
/// Returns AuxBlock with aggregate hash and difficulty target.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<AuxBlock, AuxPowError>")]
pub struct CreateAuxBlock {
    /// Mining address for coinbase rewards (exact legacy parameter)
    pub address: EvmAddress,
}

/// Direct port of legacy submit_aux_block function
///
/// Submits completed proof-of-work for validation and chain finalization.
/// Validates PoW and AuxPow structure before processing.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), AuxPowError>")]
pub struct SubmitAuxBlock {
    /// Block hash to submit (exact legacy parameter)
    pub hash: BlockHash,
    /// Completed AuxPow solution (exact legacy parameter)
    pub auxpow: AuxPow,
}

/// Direct port of legacy get_queued_auxpow function
///
/// Returns currently queued AuxPow header awaiting finalization.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Option<AuxPowHeader>")]
pub struct GetQueuedAuxpow;

/// Control message for mining operations
///
/// Enables/disables continuous mining loop and sets mining address.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), AuxPowError>")]
pub struct SetMiningEnabled {
    /// Whether to enable mining
    pub enabled: bool,
    /// Mining address (updates config if provided)
    pub mining_address: Option<EvmAddress>,
}

/// Get current mining status and statistics
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<MiningStatus, AuxPowError>")]
pub struct GetMiningStatus;

/// Mining status response
#[derive(Debug, Clone)]
pub struct MiningStatus {
    pub mining_enabled: bool,
    pub mining_address: EvmAddress,
    pub current_work_count: usize,
    pub last_work_time: Option<std::time::Instant>,
    pub total_blocks_mined: u64,
    pub total_submissions: u64,
    pub success_rate: f64,
}

// ============================================================================
// DifficultyManager Messages - Exact Algorithm Ports
// ============================================================================

/// Port of legacy get_next_work_required function
///
/// Calculates required difficulty target for next block based on
/// Bitcoin-compatible retargeting algorithm.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<CompactTarget, DifficultyError>")]
pub struct GetNextWorkRequired {
    /// Last block with AuxPow (exact legacy parameter)
    pub index_last: ConsensusBlock,
    /// Current chain head height (exact legacy parameter)
    pub chain_head_height: u64,
}

/// Internal calculation message for difficulty adjustment
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<CompactTarget, DifficultyError>")]
pub struct CalculateNextWorkRequired {
    /// Height difference since last AuxPow
    pub auxpow_height_difference: u32,
    /// Last difficulty bits
    pub last_bits: u32,
}

/// Update difficulty history for retargeting calculations
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), DifficultyError>")]
pub struct UpdateDifficultyHistory {
    pub height: u64,
    pub timestamp: Duration,
    pub bits: CompactTarget,
    pub auxpow_count: u32,
}

/// Get difficulty history for analysis
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<DifficultyEntry>, DifficultyError>")]
pub struct GetDifficultyHistory {
    pub limit: Option<usize>,
    pub start_height: Option<u64>,
}

/// Difficulty history entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DifficultyEntry {
    pub height: u64,
    pub timestamp: Duration,
    pub bits: CompactTarget,
    pub auxpow_count: u32,
}

/// Get current difficulty statistics
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<DifficultyStats, DifficultyError>")]
pub struct GetDifficultyStats;

/// Current difficulty statistics
#[derive(Debug, Clone)]
pub struct DifficultyStats {
    pub current_target: CompactTarget,
    pub current_difficulty: f64,
    pub last_retarget_height: u64,
    pub blocks_until_retarget: u64,
    pub estimated_next_difficulty: Option<f64>,
    pub adjustment_history: Vec<DifficultyAdjustment>,
}

/// Difficulty adjustment record
#[derive(Debug, Clone)]
pub struct DifficultyAdjustment {
    pub height: u64,
    pub old_target: CompactTarget,
    pub new_target: CompactTarget,
    pub adjustment_ratio: f64,
    pub blocks_in_period: u32,
    pub actual_timespan: Duration,
    pub target_timespan: Duration,
}

// ============================================================================
// ChainActor Extension Messages - ChainManager Trait Ports
// ============================================================================

/// Direct port of ChainManager::get_aggregate_hashes
///
/// Returns vector of block hashes for aggregate hash calculation.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<BlockHash>, ChainError>")]
pub struct GetAggregateHashes;

/// Direct port of ChainManager::get_last_finalized_block
///
/// Returns the most recent finalized consensus block.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<ConsensusBlock, ChainError>")]
pub struct GetLastFinalizedBlock;

/// Direct port of ChainManager::get_block_by_hash for mining
///
/// Retrieves specific block by hash for validation purposes.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<ConsensusBlock>, ChainError>")]
pub struct GetBlockByHashForMining {
    pub hash: BlockHash,
}

/// Direct port of ChainManager::push_auxpow
///
/// Submits validated AuxPow to chain for block finalization.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<bool, ChainError>")]
pub struct PushAuxPow {
    pub start_hash: BlockHash,
    pub end_hash: BlockHash,
    pub bits: u32,
    pub chain_id: u32,
    pub height: u64,
    pub auxpow: AuxPow,
    pub address: EvmAddress,
}

/// Direct port of ChainManager::is_synced
///
/// Checks if chain is currently synchronized for mining decisions.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<bool, ChainError>")]
pub struct IsSynced;

/// Get current chain head for height calculations
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<SignedConsensusBlock, ChainError>")]
pub struct GetHead;

// ============================================================================
// StorageActor Extension Messages - Difficulty Persistence
// ============================================================================

/// Get stored difficulty history from database
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<DifficultyEntry>, StorageError>")]
pub struct GetStoredDifficultyHistory {
    pub limit: Option<usize>,
    pub start_height: Option<u64>,
}

/// Save difficulty entry to persistent storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct SaveDifficultyEntry {
    pub entry: DifficultyEntry,
}

/// Get last retarget height from storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<u64>, StorageError>")]
pub struct GetLastRetargetHeight;

/// Save retarget height to storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct SaveRetargetHeight {
    pub height: u64,
}

// ============================================================================
// Health and Monitoring Messages
// ============================================================================

/// Health check message for supervision
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<HealthCheckResult, AuxPowError>")]
pub struct HealthCheck;

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub score: u8,
    pub details: String,
    pub last_activity: Option<std::time::Instant>,
    pub error_count: u64,
}

/// Performance metrics request
#[derive(Message, Debug, Clone)]
#[rtype(result = "PerformanceMetrics")]
pub struct GetPerformanceMetrics;

/// Performance metrics response
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub avg_create_time_ms: f64,
    pub avg_submit_time_ms: f64,
    pub avg_difficulty_calc_time_ms: f64,
    pub message_queue_depth: usize,
    pub memory_usage_bytes: Option<u64>,
    pub cache_hit_rate: f64,
}

// ============================================================================
// Message Response Trait Implementations
// ============================================================================
// Note: MessageResponse is typically implemented automatically
// via the #[rtype(result = "...")] annotation on messages