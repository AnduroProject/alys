//! Chain consensus and blockchain messages for ALYS-007 ChainActor implementation
//!
//! This module defines the comprehensive message protocol for the ChainActor that replaces
//! the monolithic Chain struct with a message-driven actor system. The protocol supports
//! block production, import, validation, finalization, and chain reorganization operations
//! while maintaining compatibility with Alys sidechain consensus requirements.
//!
//! ## Message Categories
//!
//! - **Block Production**: ProduceBlock, BuildExecutionPayload
//! - **Block Import**: ImportBlock, ValidateBlock, CommitBlock
//! - **Chain State**: GetChainStatus, GetBlocksByRange, UpdateFederation
//! - **Finalization**: FinalizeBlocks, ProcessAuxPoW
//! - **Reorganization**: ReorgChain, RevertToHeight
//! - **Peg Operations**: ProcessPegIns, ProcessPegOuts
//! - **Network**: BroadcastBlock, HandlePeerBlock
//!
//! All messages support distributed tracing, correlation IDs, and actor supervision patterns.

use crate::types::*;
use actix::prelude::*;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Message to import a block into the chain with comprehensive validation
/// This is the primary message for processing incoming blocks from peers or local production
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<ImportBlockResult, ChainError>")]
pub struct ImportBlock {
    /// The signed consensus block to import
    pub block: SignedConsensusBlock,
    /// Whether to broadcast the block after successful import
    pub broadcast: bool,
    /// Priority for processing this block
    pub priority: BlockProcessingPriority,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
    /// Source of the block (peer, mining, sync, etc.)
    pub source: BlockSource,
}

/// Result of block import operation with detailed validation information
#[derive(Debug, Clone)]
pub struct ImportBlockResult {
    /// Whether the block was successfully imported
    pub imported: bool,
    /// The block reference if imported
    pub block_ref: Option<BlockRef>,
    /// Whether a reorganization was triggered
    pub triggered_reorg: bool,
    /// Number of blocks reverted (if reorg occurred)
    pub blocks_reverted: u32,
    /// Validation result details
    pub validation_result: ValidationResult,
    /// Processing metrics
    pub processing_metrics: BlockProcessingMetrics,
}

/// Enhanced block processing metrics for performance monitoring
#[derive(Debug, Clone, Default)]
pub struct BlockProcessingMetrics {
    /// Total time from receive to import completion
    pub total_time_ms: u64,
    /// Time spent in validation
    pub validation_time_ms: u64,
    /// Time spent in execution
    pub execution_time_ms: u64,
    /// Time spent in storage operations
    pub storage_time_ms: u64,
    /// Queue time before processing started
    pub queue_time_ms: u64,
    /// Memory usage during processing
    pub memory_usage_bytes: Option<u64>,
}

/// Message to produce a new block at the specified slot
/// Only processed if this node is the slot authority and conditions are met
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<SignedConsensusBlock, ChainError>")]
pub struct ProduceBlock {
    /// Aura slot for block production
    pub slot: u64,
    /// Block timestamp (must align with slot timing)
    pub timestamp: Duration,
    /// Force production even if not our slot (for testing)
    pub force: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get blocks within a specified range
/// Supports pagination and filtering for chain synchronization
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<SignedConsensusBlock>, ChainError>")]
pub struct GetBlocksByRange {
    /// Starting block height (inclusive)
    pub start_height: u64,
    /// Number of blocks to retrieve
    pub count: usize,
    /// Whether to include full block data or just headers
    pub include_body: bool,
    /// Maximum allowed response size in bytes
    pub max_response_size: Option<usize>,
}

/// Message to get the current comprehensive chain status
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<ChainStatus, ChainError>")]
pub struct GetChainStatus {
    /// Include detailed metrics in response
    pub include_metrics: bool,
    /// Include peer sync status
    pub include_sync_info: bool,
}

/// Message to update the federation configuration
/// Supports hot-reload of federation membership and thresholds
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), ChainError>")]
pub struct UpdateFederation {
    /// New federation version
    pub version: u32,
    /// Updated federation members with their public keys
    pub members: Vec<FederationMember>,
    /// New signature threshold
    pub threshold: usize,
    /// Effective block height for the change
    pub effective_height: u64,
    /// Migration strategy for the update
    pub migration_strategy: FederationMigrationStrategy,
}

/// Federation member information
#[derive(Debug, Clone)]
pub struct FederationMember {
    /// Member's public key for signature verification
    pub public_key: PublicKey,
    /// Member's address
    pub address: Address,
    /// Member's weight in consensus (for weighted voting)
    pub weight: u32,
    /// Whether this member is currently active
    pub active: bool,
}

/// Strategy for migrating federation configuration
#[derive(Debug, Clone)]
pub enum FederationMigrationStrategy {
    /// Immediate switch at specified height
    Immediate,
    /// Gradual transition over specified blocks
    Gradual { transition_blocks: u32 },
    /// Parallel operation with both federations
    Parallel { overlap_blocks: u32 },
}

/// Message to finalize blocks up to a specified height using AuxPoW
/// This confirms blocks with Bitcoin merged mining proof-of-work
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<FinalizationResult, ChainError>")]
pub struct FinalizeBlocks {
    /// AuxPoW header providing proof-of-work
    pub pow_header: AuxPowHeader,
    /// Target height to finalize (inclusive)
    pub target_height: u64,
    /// Whether to halt block production if finalization fails
    pub halt_on_failure: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Result of finalization operation
#[derive(Debug, Clone)]
pub struct FinalizationResult {
    /// Height that was actually finalized
    pub finalized_height: u64,
    /// Hash of the finalized block
    pub finalized_hash: Hash256,
    /// Number of blocks finalized in this operation
    pub blocks_finalized: u32,
    /// Whether proof-of-work was valid
    pub pow_valid: bool,
    /// Finalization processing time
    pub processing_time_ms: u64,
}

/// Message to validate a block without importing it
/// Used for pre-validation of blocks before adding to candidate pool
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<bool, ChainError>")]
pub struct ValidateBlock {
    /// The signed consensus block to validate
    pub block: SignedConsensusBlock,
    /// Validation level to perform
    pub validation_level: ValidationLevel,
    /// Whether to cache validation results
    pub cache_result: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Levels of block validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    /// Basic structural validation only
    Basic,
    /// Full validation including state transitions
    Full,
    /// Signature validation only
    SignatureOnly,
    /// Consensus rules validation
    ConsensusOnly,
}

/// Message to handle a chain reorganization
/// Reverts the current chain and applies a new canonical chain
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<ReorgResult, ChainError>")]
pub struct ReorgChain {
    /// The new canonical head
    pub new_head: Hash256,
    /// The blocks that form the new canonical chain
    pub blocks: Vec<SignedConsensusBlock>,
    /// Maximum allowed reorg depth
    pub max_depth: Option<u32>,
    /// Whether to force the reorg even if not heavier
    pub force: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Result of reorganization operation
#[derive(Debug, Clone)]
pub struct ReorgResult {
    /// Whether the reorganization was successful
    pub success: bool,
    /// The common ancestor block
    pub common_ancestor: BlockRef,
    /// Number of blocks reverted
    pub blocks_reverted: u32,
    /// Number of blocks applied
    pub blocks_applied: u32,
    /// The new chain head
    pub new_head: BlockRef,
    /// Processing time for the reorg
    pub processing_time_ms: u64,
    /// Whether any peg operations were affected
    pub peg_operations_affected: bool,
}

/// Message to process pending peg-in operations
/// Converts Bitcoin deposits into Alys sidechain tokens
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<PegInResult, ChainError>")]
pub struct ProcessPegIns {
    /// Pending peg-in transactions to process
    pub peg_ins: Vec<PendingPegIn>,
    /// Block height to process for
    pub target_height: u64,
    /// Maximum number of peg-ins to process
    pub max_pegins: Option<usize>,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Pending peg-in transaction
#[derive(Debug, Clone)]
pub struct PendingPegIn {
    /// Bitcoin transaction ID
    pub bitcoin_txid: bitcoin::Txid,
    /// Bitcoin block hash containing the transaction
    pub bitcoin_block_hash: bitcoin::BlockHash,
    /// EVM address to receive tokens
    pub evm_address: Address,
    /// Amount in satoshis
    pub amount_sats: u64,
    /// Number of confirmations
    pub confirmations: u32,
    /// Index of the relevant output
    pub output_index: u32,
}

/// Result of peg-in processing
#[derive(Debug, Clone)]
pub struct PegInResult {
    /// Number of peg-ins successfully processed
    pub processed: u32,
    /// Number of peg-ins that failed
    pub failed: u32,
    /// Total amount processed (in wei)
    pub total_amount_wei: U256,
    /// Processing details for each peg-in
    pub details: Vec<PegInDetail>,
}

/// Details of individual peg-in processing
#[derive(Debug, Clone)]
pub struct PegInDetail {
    /// The Bitcoin transaction ID
    pub bitcoin_txid: bitcoin::Txid,
    /// Whether processing was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Amount processed (in wei)
    pub amount_wei: U256,
    /// EVM transaction hash if successful
    pub evm_tx_hash: Option<H256>,
}

/// Message to process peg-out operations
/// Burns sidechain tokens and initiates Bitcoin withdrawals
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<PegOutResult, ChainError>")]
pub struct ProcessPegOuts {
    /// Pending peg-out requests to process
    pub peg_outs: Vec<PendingPegOut>,
    /// Federation signatures collected
    pub signatures: Vec<FederationSignature>,
    /// Whether to create the Bitcoin transaction
    pub create_btc_tx: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Pending peg-out request
#[derive(Debug, Clone)]
pub struct PendingPegOut {
    /// EVM transaction hash that burned tokens
    pub burn_tx_hash: H256,
    /// Bitcoin address to send to
    pub bitcoin_address: String,
    /// Amount to send (in satoshis)
    pub amount_sats: u64,
    /// Fee for the transaction
    pub fee_sats: u64,
    /// Block number of the burn transaction
    pub burn_block_number: u64,
}

/// Federation signature for peg-out operations
#[derive(Debug, Clone)]
pub struct FederationSignature {
    /// Member's public key
    pub public_key: PublicKey,
    /// Signature bytes
    pub signature: Signature,
    /// Index of the signer in the federation
    pub signer_index: u8,
}

/// Result of peg-out processing
#[derive(Debug, Clone)]
pub struct PegOutResult {
    /// Number of peg-outs successfully processed
    pub processed: u32,
    /// Bitcoin transaction created (if any)
    pub bitcoin_tx: Option<bitcoin::Transaction>,
    /// Total amount sent (in satoshis)
    pub total_amount_sats: u64,
    /// Processing details for each peg-out
    pub details: Vec<PegOutDetail>,
}

/// Details of individual peg-out processing
#[derive(Debug, Clone)]
pub struct PegOutDetail {
    /// The burn transaction hash
    pub burn_tx_hash: H256,
    /// Whether processing was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Bitcoin transaction output index
    pub output_index: Option<u32>,
}

/// Message to broadcast a block to the network
/// Used after successful block production or import
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<BroadcastResult, ChainError>")]
pub struct BroadcastBlock {
    /// The block to broadcast
    pub block: SignedConsensusBlock,
    /// Priority for broadcast
    pub priority: BroadcastPriority,
    /// Exclude specific peers from broadcast
    pub exclude_peers: Vec<PeerId>,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Priority levels for block broadcasting
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BroadcastPriority {
    /// Low priority background broadcast
    Low,
    /// Normal priority broadcast
    Normal,
    /// High priority broadcast (new head)
    High,
    /// Critical broadcast (emergency)
    Critical,
}

/// Result of block broadcast operation
#[derive(Debug, Clone)]
pub struct BroadcastResult {
    /// Number of peers the block was sent to
    pub peers_reached: u32,
    /// Number of successful sends
    pub successful_sends: u32,
    /// Number of failed sends
    pub failed_sends: u32,
    /// Average response time from peers
    pub avg_response_time_ms: Option<u64>,
    /// List of peers that failed to receive
    pub failed_peers: Vec<PeerId>,
}

/// Message to register for block notifications
/// Allows other actors to subscribe to chain events
#[derive(Message, Debug)]
#[rtype(result = "Result<(), ChainError>")]
pub struct SubscribeBlocks {
    /// Actor to receive block notifications
    pub subscriber: Recipient<BlockNotification>,
    /// Types of events to subscribe to
    pub event_types: Vec<BlockEventType>,
    /// Filter criteria for notifications
    pub filter: Option<NotificationFilter>,
}

/// Types of block events available for subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockEventType {
    /// New block imported
    BlockImported,
    /// Block finalized
    BlockFinalized,
    /// Chain reorganization
    ChainReorg,
    /// Block validation failed
    ValidationFailed,
    /// New block produced locally
    BlockProduced,
}

/// Filter criteria for block notifications
#[derive(Debug, Clone)]
pub struct NotificationFilter {
    /// Only notify for blocks above this height
    pub min_height: Option<u64>,
    /// Only notify for blocks with specific attributes
    pub has_auxpow: Option<bool>,
    /// Only notify for blocks with peg operations
    pub has_peg_ops: Option<bool>,
}

/// Block notification sent to subscribers
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct BlockNotification {
    /// The block that triggered the notification
    pub block: SignedConsensusBlock,
    /// Type of event that occurred
    pub event_type: BlockEventType,
    /// Whether this block is part of the canonical chain
    pub is_canonical: bool,
    /// Additional event context
    pub context: NotificationContext,
}

/// Additional context for block notifications
#[derive(Debug, Clone, Default)]
pub struct NotificationContext {
    /// Whether this was a reorg operation
    pub is_reorg: bool,
    /// Depth of reorganization (if applicable)
    pub reorg_depth: Option<u32>,
    /// Processing metrics
    pub processing_time_ms: Option<u64>,
    /// Source of the block
    pub source: Option<BlockSource>,
}

/// Message to handle auxiliary PoW submission from Bitcoin miners
/// Processes merged mining proofs for block finalization
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<AuxPowResult, ChainError>")]
pub struct ProcessAuxPow {
    /// The auxiliary proof-of-work to process
    pub aux_pow: AuxPow,
    /// Target block range for finalization
    pub target_range: (Hash256, Hash256),
    /// Difficulty bits for validation
    pub bits: u32,
    /// Chain ID for isolation
    pub chain_id: u32,
    /// Miner's fee recipient address
    pub fee_recipient: Address,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<Uuid>,
}

/// Result of auxiliary PoW processing
#[derive(Debug, Clone)]
pub struct AuxPowResult {
    /// Whether the AuxPoW was valid
    pub valid: bool,
    /// Difficulty target that was met
    pub difficulty_met: Option<U256>,
    /// Range of blocks finalized
    pub finalized_range: Option<(u64, u64)>,
    /// Processing time
    pub processing_time_ms: u64,
    /// Error details if invalid
    pub error_details: Option<String>,
}

/// Message to pause block production
/// Used during maintenance or emergency situations
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), ChainError>")]
pub struct PauseBlockProduction {
    /// Reason for pausing
    pub reason: String,
    /// Duration to pause (None = indefinite)
    pub duration: Option<Duration>,
    /// Whether to finish current block first
    pub finish_current: bool,
    /// Authority requesting the pause
    pub authority: Option<Address>,
}

/// Message to resume block production
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), ChainError>")]
pub struct ResumeBlockProduction {
    /// Authority requesting the resume
    pub authority: Option<Address>,
    /// Force resume even if conditions not met
    pub force: bool,
}

/// Message to get performance metrics
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<ChainMetrics, ChainError>")]
pub struct GetChainMetrics {
    /// Include detailed breakdown
    pub include_details: bool,
    /// Time window for metrics (None = all time)
    pub time_window: Option<Duration>,
}

/// Comprehensive chain performance metrics
#[derive(Debug, Clone, Default)]
pub struct ChainMetrics {
    /// Total blocks produced by this node
    pub blocks_produced: u64,
    /// Total blocks imported
    pub blocks_imported: u64,
    /// Average block production time
    pub avg_production_time_ms: f64,
    /// Average block import time
    pub avg_import_time_ms: f64,
    /// Number of reorganizations
    pub reorg_count: u32,
    /// Average reorg depth
    pub avg_reorg_depth: f64,
    /// Peg-in operations processed
    pub pegins_processed: u64,
    /// Peg-out operations processed
    pub pegouts_processed: u64,
    /// Total value transferred in peg operations
    pub total_peg_value_sats: u64,
    /// Validation failures
    pub validation_failures: u64,
    /// Network broadcast success rate
    pub broadcast_success_rate: f64,
    /// Memory usage statistics
    pub memory_stats: MemoryStats,
}

/// Memory usage statistics
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// Current memory usage in bytes
    pub current_bytes: u64,
    /// Peak memory usage
    pub peak_bytes: u64,
    /// Memory allocated for pending blocks
    pub pending_blocks_bytes: u64,
    /// Memory allocated for validation cache
    pub validation_cache_bytes: u64,
}

/// Message to query chain state at a specific height or hash
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<ChainStateQuery, ChainError>")]
pub struct QueryChainState {
    /// Block hash to query (if None, use latest)
    pub block_hash: Option<Hash256>,
    /// Block height to query (if hash not provided)
    pub block_height: Option<u64>,
    /// Types of state information to include
    pub include_info: Vec<StateInfoType>,
}

/// Types of chain state information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateInfoType {
    /// Basic block header information
    Header,
    /// Transaction count and gas usage
    Transactions,
    /// Peg operation details
    PegOperations,
    /// Validation status
    Validation,
    /// Network propagation info
    Network,
}

/// Chain state query result
#[derive(Debug, Clone)]
pub struct ChainStateQuery {
    /// Block reference
    pub block_ref: BlockRef,
    /// Requested state information
    pub state_info: std::collections::HashMap<StateInfoType, serde_json::Value>,
    /// Query processing time
    pub processing_time_ms: u64,
}

/// Source of a block with enhanced context information
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockSource {
    /// Block produced locally by this node
    Local,
    /// Block received from a specific peer
    Peer {
        /// Peer identifier
        peer_id: PeerId,
        /// Peer's reported chain height
        peer_height: Option<u64>,
    },
    /// Block received during sync operation
    Sync {
        /// Sync session identifier
        sync_id: String,
        /// Batch number in sync operation
        batch_number: Option<u32>,
    },
    /// Block from mining operation (auxiliary PoW)
    Mining {
        /// Miner identifier
        miner_id: Option<String>,
        /// Mining pool information
        pool_info: Option<String>,
    },
    /// Block loaded from storage during startup
    Storage,
    /// Block received via RPC
    Rpc {
        /// Client identifier
        client_id: Option<String>,
    },
    /// Block for testing purposes
    Test,
}

/// Comprehensive block validation result with detailed analysis
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Overall validation status
    pub is_valid: bool,
    /// Detailed validation errors
    pub errors: Vec<ValidationError>,
    /// Gas consumed during validation
    pub gas_used: u64,
    /// Resulting state root
    pub state_root: Hash256,
    /// Validation performance metrics
    pub validation_metrics: ValidationMetrics,
    /// Checkpoints passed during validation
    pub checkpoints: Vec<String>,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

/// Validation performance metrics
#[derive(Debug, Clone, Default)]
pub struct ValidationMetrics {
    /// Total validation time
    pub total_time_ms: u64,
    /// Time for structural validation
    pub structural_time_ms: u64,
    /// Time for signature validation
    pub signature_time_ms: u64,
    /// Time for state transition validation
    pub state_time_ms: u64,
    /// Time for consensus rule validation
    pub consensus_time_ms: u64,
    /// Memory usage during validation
    pub memory_used_bytes: u64,
}

/// Detailed block validation errors with context
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Parent block hash doesn't match expected
    InvalidParentHash {
        expected: Hash256,
        actual: Hash256,
    },
    /// Block timestamp is invalid
    InvalidTimestamp {
        timestamp: u64,
        reason: TimestampError,
    },
    /// Invalid transactions in block
    InvalidTransactions {
        tx_hashes: Vec<H256>,
        reasons: Vec<String>,
    },
    /// State root mismatch after execution
    InvalidStateRoot {
        expected: Hash256,
        computed: Hash256,
    },
    /// Gas usage doesn't match header
    InvalidGasUsed {
        expected: u64,
        actual: u64,
    },
    /// Signature validation failed
    InvalidSignature {
        signer: Option<Address>,
        reason: String,
    },
    /// Consensus rule violation
    ConsensusError {
        rule: String,
        message: String,
    },
    /// Slot validation error
    InvalidSlot {
        slot: u64,
        expected_producer: Address,
        actual_producer: Address,
    },
    /// Auxiliary PoW validation failed
    InvalidAuxPoW {
        reason: String,
        details: Option<String>,
    },
    /// Peg operation validation failed
    InvalidPegOperations {
        pegin_errors: Vec<String>,
        pegout_errors: Vec<String>,
    },
    /// Block too far in future
    BlockTooFuture {
        block_time: u64,
        current_time: u64,
        max_drift: u64,
    },
    /// Block too old
    BlockTooOld {
        block_height: u64,
        current_height: u64,
        max_age: u32,
    },
}

/// Timestamp validation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimestampError {
    /// Timestamp is too far in the future
    TooFuture { max_drift_seconds: u64 },
    /// Timestamp is before parent block
    BeforeParent { parent_timestamp: u64 },
    /// Timestamp doesn't align with slot
    SlotMismatch { expected: u64, actual: u64 },
}

/// Comprehensive current chain status with detailed metrics
#[derive(Debug, Clone)]
pub struct ChainStatus {
    /// Current chain head
    pub head: Option<BlockRef>,
    /// Highest block number
    pub best_block_number: u64,
    /// Hash of the best block
    pub best_block_hash: Hash256,
    /// Finalized block information
    pub finalized: Option<BlockRef>,
    /// Sync status with peer information
    pub sync_status: SyncStatus,
    /// Validator status and next duties
    pub validator_status: ValidatorStatus,
    /// Proof-of-Work status and metrics
    pub pow_status: PoWStatus,
    /// Federation status
    pub federation_status: FederationStatus,
    /// Peg operation status
    pub peg_status: PegOperationStatus,
    /// Performance metrics
    pub performance: ChainPerformanceStatus,
    /// Network status
    pub network_status: NetworkStatus,
    /// Actor system health
    pub actor_health: ActorHealthStatus,
}

/// Federation status information
#[derive(Debug, Clone)]
pub struct FederationStatus {
    /// Current federation version
    pub version: u32,
    /// Number of active federation members
    pub active_members: usize,
    /// Signature threshold
    pub threshold: usize,
    /// Whether federation is ready for operations
    pub ready: bool,
    /// Pending configuration changes
    pub pending_changes: Vec<String>,
}

/// Peg operation status
#[derive(Debug, Clone)]
pub struct PegOperationStatus {
    /// Pending peg-ins
    pub pending_pegins: u32,
    /// Pending peg-outs
    pub pending_pegouts: u32,
    /// Total value locked (in sats)
    pub total_value_locked: u64,
    /// Recent peg operation success rate
    pub success_rate: f64,
    /// Average processing time
    pub avg_processing_time_ms: u64,
}

/// Chain performance status
#[derive(Debug, Clone)]
pub struct ChainPerformanceStatus {
    /// Average block time
    pub avg_block_time_ms: u64,
    /// Current blocks per second
    pub blocks_per_second: f64,
    /// Transaction throughput
    pub transactions_per_second: f64,
    /// Memory usage
    pub memory_usage_mb: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
}

/// Network connectivity status
#[derive(Debug, Clone)]
pub struct NetworkStatus {
    /// Number of connected peers
    pub connected_peers: usize,
    /// Inbound connections
    pub inbound_connections: usize,
    /// Outbound connections
    pub outbound_connections: usize,
    /// Average peer block height
    pub avg_peer_height: Option<u64>,
    /// Network health score (0-100)
    pub health_score: u8,
}

/// Actor system health status
#[derive(Debug, Clone)]
pub struct ActorHealthStatus {
    /// Number of active actors
    pub active_actors: u32,
    /// Failed actors requiring restart
    pub failed_actors: u32,
    /// Actor message queue depths
    pub queue_depths: std::collections::HashMap<String, u32>,
    /// Overall system health (0-100)
    pub system_health: u8,
    /// Actor supervision status
    pub supervision_active: bool,
}

/// Enhanced validator status with detailed information
#[derive(Debug, Clone)]
pub enum ValidatorStatus {
    /// Node is not configured as a validator
    NotValidator,
    /// Node is a validator with detailed status
    Validator {
        /// Validator's address
        address: Address,
        /// Whether validator is currently active
        is_active: bool,
        /// Next assigned slot (if any)
        next_slot: Option<u64>,
        /// Time until next slot
        next_slot_in_ms: Option<u64>,
        /// Recent block production performance
        recent_performance: ValidatorPerformance,
        /// Validator weight in consensus
        weight: u32,
    },
    /// Validator is temporarily paused
    Paused {
        /// Reason for pause
        reason: String,
        /// When pause ends (if known)
        resume_at: Option<SystemTime>,
    },
    /// Validator is being migrated
    Migrating {
        /// Current migration phase
        phase: String,
        /// Progress percentage
        progress: u8,
    },
}

/// Validator performance metrics
#[derive(Debug, Clone, Default)]
pub struct ValidatorPerformance {
    /// Blocks produced in recent window
    pub blocks_produced: u32,
    /// Blocks missed in recent window
    pub blocks_missed: u32,
    /// Success rate percentage
    pub success_rate: f64,
    /// Average block production time
    pub avg_production_time_ms: u64,
    /// Recent uptime percentage
    pub uptime_percent: f64,
}

/// Enhanced Proof of Work status with mining metrics
#[derive(Debug, Clone)]
pub enum PoWStatus {
    /// AuxPoW is disabled
    Disabled,
    /// Waiting for proof-of-work
    Waiting {
        /// Height of last PoW block
        last_pow_block: u64,
        /// Blocks produced since last PoW
        blocks_since_pow: u64,
        /// Maximum blocks allowed without PoW
        timeout_blocks: u64,
        /// Time remaining before halt
        time_until_halt_ms: Option<u64>,
    },
    /// PoW is active with mining
    Active {
        /// Current difficulty target
        current_target: U256,
        /// Estimated network hash rate
        hash_rate: f64,
        /// Number of active miners
        active_miners: u32,
        /// Recent blocks with valid PoW
        recent_pow_blocks: u32,
        /// Average time between PoW blocks
        avg_pow_interval_ms: u64,
    },
    /// Emergency halt due to no PoW
    Halted {
        /// Reason for halt
        reason: String,
        /// When halt started
        halted_at: SystemTime,
        /// Blocks waiting for PoW
        pending_blocks: u32,
    },
}

/// Synchronization status
#[derive(Debug, Clone)]
pub enum SyncStatus {
    /// Fully synchronized with network
    Synced,
    /// Currently syncing blocks
    Syncing {
        /// Current block height
        current: u64,
        /// Target block height
        target: u64,
        /// Sync progress percentage
        progress: f64,
        /// Estimated time remaining
        eta_ms: Option<u64>,
    },
    /// Sync failed
    Failed {
        /// Failure reason
        reason: String,
        /// Last successful block
        last_block: u64,
    },
    /// Not connected to network
    Disconnected,
}

// Helper implementations for message construction and validation

impl ImportBlock {
    /// Create a new import block message with default values
    pub fn new(block: SignedConsensusBlock, source: BlockSource) -> Self {
        Self {
            block,
            broadcast: true,
            priority: BlockProcessingPriority::Normal,
            correlation_id: Some(Uuid::new_v4()),
            source,
        }
    }

    /// Create import block message for high priority processing
    pub fn high_priority(block: SignedConsensusBlock, source: BlockSource) -> Self {
        Self {
            block,
            broadcast: true,
            priority: BlockProcessingPriority::High,
            correlation_id: Some(Uuid::new_v4()),
            source,
        }
    }

    /// Create import block message without broadcasting
    pub fn no_broadcast(block: SignedConsensusBlock, source: BlockSource) -> Self {
        Self {
            block,
            broadcast: false,
            priority: BlockProcessingPriority::Normal,
            correlation_id: Some(Uuid::new_v4()),
            source,
        }
    }
}

impl ProduceBlock {
    /// Create a new produce block message
    pub fn new(slot: u64, timestamp: Duration) -> Self {
        Self {
            slot,
            timestamp,
            force: false,
            correlation_id: Some(Uuid::new_v4()),
        }
    }

    /// Create forced block production (for testing)
    pub fn forced(slot: u64, timestamp: Duration) -> Self {
        Self {
            slot,
            timestamp,
            force: true,
            correlation_id: Some(Uuid::new_v4()),
        }
    }
}

impl GetChainStatus {
    /// Create basic chain status request
    pub fn basic() -> Self {
        Self {
            include_metrics: false,
            include_sync_info: false,
        }
    }

    /// Create detailed chain status request
    pub fn detailed() -> Self {
        Self {
            include_metrics: true,
            include_sync_info: true,
        }
    }
}

impl BroadcastBlock {
    /// Create normal priority broadcast
    pub fn normal(block: SignedConsensusBlock) -> Self {
        Self {
            block,
            priority: BroadcastPriority::Normal,
            exclude_peers: Vec::new(),
            correlation_id: Some(Uuid::new_v4()),
        }
    }

    /// Create high priority broadcast
    pub fn high_priority(block: SignedConsensusBlock) -> Self {
        Self {
            block,
            priority: BroadcastPriority::High,
            exclude_peers: Vec::new(),
            correlation_id: Some(Uuid::new_v4()),
        }
    }
}

impl Default for ChainStatus {
    fn default() -> Self {
        Self {
            head: None,
            best_block_number: 0,
            best_block_hash: Hash256::zero(),
            finalized: None,
            sync_status: SyncStatus::Disconnected,
            validator_status: ValidatorStatus::NotValidator,
            pow_status: PoWStatus::Disabled,
            federation_status: FederationStatus {
                version: 0,
                active_members: 0,
                threshold: 0,
                ready: false,
                pending_changes: Vec::new(),
            },
            peg_status: PegOperationStatus {
                pending_pegins: 0,
                pending_pegouts: 0,
                total_value_locked: 0,
                success_rate: 0.0,
                avg_processing_time_ms: 0,
            },
            performance: ChainPerformanceStatus {
                avg_block_time_ms: 2000, // 2 second default
                blocks_per_second: 0.0,
                transactions_per_second: 0.0,
                memory_usage_mb: 0,
                cpu_usage_percent: 0.0,
            },
            network_status: NetworkStatus {
                connected_peers: 0,
                inbound_connections: 0,
                outbound_connections: 0,
                avg_peer_height: None,
                health_score: 0,
            },
            actor_health: ActorHealthStatus {
                active_actors: 0,
                failed_actors: 0,
                queue_depths: std::collections::HashMap::new(),
                system_health: 0,
                supervision_active: false,
            },
        }
    }
}