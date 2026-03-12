//! ChainActor V2 Messages
//!
//! Essential message types (10 core messages) - simplified from V1's 25+ messages

use actix::prelude::*;
use bitcoin::{BlockHash as BitcoinBlockHash, Txid};
use ethereum_types::{Address, H256, U256};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

// Re-export types that would come from other modules
pub use crate::auxpow::AuxPow;
pub use crate::auxpow_miner::AuxBlock;
pub use crate::block::{AuxPowHeader, ConsensusBlock, SignedConsensusBlock};
pub use crate::actors_v2::storage::actor::BlockRef;
pub use bridge::PegInInfo;
pub use lighthouse_wrapper::types::MainnetEthSpec;

/// Core ChainActor messages
#[derive(Debug, Message)]
#[rtype(result = "Result<ChainResponse, crate::actors_v2::chain::ChainError>")]
pub enum ChainMessage {
    /// Produce a new block (for validators)
    ProduceBlock { slot: u64, timestamp: Duration },

    /// Import block from network/sync
    ImportBlock {
        block: SignedConsensusBlock<MainnetEthSpec>,
        source: BlockSource,
        peer_id: Option<String>,
    },

    /// Process and validate AuxPoW
    ProcessAuxPow { auxpow: AuxPow, block_hash: H256 },

    /// Queue completed AuxPoW for next block (Phase 4: Integration Point 3c)
    QueueAuxPow {
        auxpow_header: AuxPowHeader,
        correlation_id: Option<Uuid>,
    },

    /// Process peg-in operations
    ProcessPegins { pegin_infos: Vec<PegInInfo> },

    /// Process peg-out operations
    ProcessPegouts { pegout_requests: Vec<PegOutRequest> },

    /// Get current chain status
    GetChainStatus,

    /// Get block by height
    GetBlockByHeight { height: u64 },

    /// Get block by hash
    GetBlockByHash { hash: H256 },

    /// Broadcast block to network
    BroadcastBlock {
        block: SignedConsensusBlock<MainnetEthSpec>,
    },

    /// Handle block received from network
    NetworkBlockReceived {
        block: SignedConsensusBlock<MainnetEthSpec>,
        peer_id: String,
    },

    /// Sync completed notification from SyncActor
    SyncCompleted { final_height: u64 },

    /// Initialize sync state on startup (internal message)
    InitializeSyncState,

    /// Periodic sync health check (internal message)
    CheckSyncHealth,

    /// Peer connected notification
    PeerConnected { peer_id: String },

    /// Peer disconnected notification
    PeerDisconnected { peer_id: String },

    // ===== Tendermint Consensus Messages =====

    /// Start new height in Tendermint consensus
    TendermintNewHeight {
        height: u64,
        correlation_id: Option<Uuid>,
    },

    /// Trigger proposal creation (for designated proposer)
    TendermintPropose {
        height: u64,
        round: u32,
        correlation_id: Option<Uuid>,
    },

    /// Received proposal from network
    TendermintProposal {
        proposal: crate::actors_v2::chain::tendermint::Proposal,
        peer_id: Option<String>,
        correlation_id: Option<Uuid>,
    },

    /// Received vote (prevote or precommit) from network
    TendermintVote {
        vote: crate::actors_v2::chain::tendermint::Vote,
        peer_id: Option<String>,
        correlation_id: Option<Uuid>,
    },

    /// Timeout event from scheduler
    TendermintTimeout {
        height: u64,
        round: u32,
        step: crate::actors_v2::chain::tendermint::TendermintStep,
        correlation_id: Option<Uuid>,
    },

    /// Governance update to apply
    TendermintGovernanceUpdate {
        update: crate::actors_v2::chain::tendermint::GovernanceUpdate,
        correlation_id: Option<Uuid>,
    },

    /// Equivocation evidence received from network
    TendermintEvidence {
        evidence: crate::actors_v2::chain::tendermint::EquivocationEvidence,
        peer_id: Option<String>,
        correlation_id: Option<Uuid>,
    },

    /// Set TendermintDriver address for bidirectional communication
    /// Allows ChainActor to notify the driver when blocks are committed
    SetTendermintDriver {
        addr: actix::Addr<crate::actors_v2::tendermint_driver::TendermintDriver>,
    },

    /// Block request timeout for pending commit
    /// Triggered when we're waiting for a block to commit but haven't received it
    TendermintBlockRequestTimeout {
        block_hash: H256,
        correlation_id: Uuid,
    },

    /// NewRound announcement from network peer (TM-B5 Fix)
    /// Used for additional sync detection when a peer announces a round at a height
    /// we haven't reached yet.
    TendermintNewRoundAnnouncement {
        height: u64,
        round: u32,
        peer_id: Option<String>,
        correlation_id: Option<Uuid>,
    },
}

/// ChainManager interface messages (for future EngineActor/AuxPowActor coordination)
#[derive(Debug, Message)]
#[rtype(result = "Result<ChainManagerResponse, crate::actors_v2::chain::ChainError>")]
pub enum ChainManagerMessage {
    /// Check if chain is synchronized
    IsSynced,

    /// Get current chain head
    GetHead,

    /// Get aggregate hashes for mining
    GetAggregateHashes { count: u32 },

    /// Get last finalized block
    GetLastFinalizedBlock,

    /// Push validated AuxPoW for finalization
    PushAuxPow {
        auxpow: AuxPow,
        params: AuxPowParams,
    },
}

/// Block source enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockSource {
    /// Block produced locally
    Local,
    /// Block received from network peer
    Network(String),
    /// Block from sync process
    Sync,
    /// Block from RPC
    Rpc,
}

/// Peg-out request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOutRequest {
    pub recipient: bitcoin::Address<bitcoin::address::NetworkUnchecked>,
    pub amount: u64,
    pub requester: Address,
    pub nonce: U256,
}

/// AuxPoW parameters for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxPowParams {
    pub target_difficulty: U256,
    pub retarget_params: Option<crate::actors_v2::chain::config::BitcoinConsensusParams>,
}

/// ChainActor response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainResponse {
    /// Generic success response
    Success,

    /// Block produced successfully
    BlockProduced {
        block: SignedConsensusBlock<MainnetEthSpec>,
        duration: Duration,
    },

    /// Block imported successfully
    BlockImported { block_hash: H256, height: u64 },

    /// Block rejected with reason
    BlockRejected { reason: String },

    /// Block queued for import (Phase 2: import lock held)
    BlockQueued { position: usize },

    /// AuxPoW processed
    AuxPowProcessed { success: bool, finalized: bool },

    /// AuxPoW queued successfully (Phase 4: Integration Point 3c)
    AuxPowQueued { height: u64 },

    /// Peg-ins processed
    PeginsProcessed { count: usize, total_amount: U256 },

    /// Peg-outs processed
    PegoutsProcessed {
        count: usize,
        transaction_id: Option<Txid>,
    },

    /// Chain status
    ChainStatus(ChainStatus),

    /// Block data
    Block(Option<SignedConsensusBlock<MainnetEthSpec>>),

    /// Block broadcasted
    BlockBroadcasted { block_hash: H256 },

    /// Network block processed
    NetworkBlockProcessed {
        accepted: bool,
        reason: Option<String>,
    },

    // ===== Tendermint Consensus Responses =====

    /// New height initialized
    TendermintHeightStarted {
        height: u64,
        round: u32,
    },

    /// Proposal created and broadcast
    TendermintProposalCreated {
        height: u64,
        round: u32,
        block_hash: H256,
    },

    /// Proposal accepted (vote will be cast)
    TendermintProposalAccepted {
        height: u64,
        round: u32,
        block_hash: H256,
    },

    /// Vote accepted
    TendermintVoteAccepted {
        height: u64,
        round: u32,
        voter: crate::actors_v2::chain::tendermint::ValidatorId,
    },

    /// Block committed (2/3+ precommits received)
    TendermintBlockCommitted {
        height: u64,
        round: u32,
        block_hash: H256,
    },

    /// Round advanced (timeout or 2/3+ nil)
    TendermintRoundAdvanced {
        height: u64,
        new_round: u32,
    },

    /// Governance update applied
    TendermintGovernanceApplied {
        effective_height: u64,
    },

    /// Equivocation evidence processed
    TendermintEvidenceProcessed {
        culprit: crate::actors_v2::chain::tendermint::ValidatorId,
        height: u64,
    },
}

/// ChainManager response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainManagerResponse {
    /// Sync status
    Synced(bool),

    /// Chain head
    Head(SignedConsensusBlock<MainnetEthSpec>),

    /// Aggregate hashes
    AggregateHashes(Vec<BitcoinBlockHash>),

    /// Last finalized block
    LastFinalizedBlock(ConsensusBlock<MainnetEthSpec>),

    /// AuxPoW push result
    AuxPowPushed {
        accepted: bool,
        block_finalized: bool,
    },
}

/// Chain status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStatus {
    /// Current chain height
    pub height: u64,

    /// Current head block hash
    pub head_hash: Option<H256>,

    /// Sync status
    pub is_synced: bool,

    /// Validator status
    pub is_validator: bool,

    /// Network status
    pub network_connected: bool,

    /// Number of connected peers
    pub peer_count: usize,

    /// Pending peg-in count
    pub pending_pegins: usize,

    /// Last block timestamp
    pub last_block_time: Option<Duration>,

    /// AuxPoW status
    pub auxpow_enabled: bool,

    /// Blocks without AuxPoW
    pub blocks_without_pow: u64,

    /// Observed network height
    /// With Tendermint instant finality, this equals the committed height.
    /// (Legacy field maintained for API compatibility)
    pub observed_height: u64,

    /// Number of orphan blocks in cache
    /// With Tendermint instant finality, always 0 (no orphan blocks possible).
    /// (Legacy field maintained for API compatibility)
    pub orphan_count: usize,
}

/// Create AuxPoW block for mining (RPC endpoint)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<AuxBlock, crate::actors_v2::chain::ChainError>")]
pub struct CreateAuxBlock {
    /// Miner's reward address
    pub miner_address: Address,
    /// Correlation ID for distributed tracing
    pub correlation_id: Uuid,
}

/// Submit completed AuxPoW for validation and processing (RPC endpoint)
///
/// Extended per Doc 16 to accept peg-in data detected by miners.
/// Miners monitor Bitcoin for deposits and submit them along with AuxPoW.
#[derive(Debug, Message)]
#[rtype(result = "Result<SubmitAuxBlockResponse, crate::actors_v2::chain::ChainError>")]
pub struct SubmitAuxBlock {
    /// Aggregate hash from createauxblock response
    pub aggregate_hash: BitcoinBlockHash,
    /// Completed AuxPoW proof
    pub auxpow: AuxPow,
    /// Peg-in data detected by miner (Doc 16) - uses V2 PegInInfo
    pub pegins: Vec<crate::actors_v2::chain::tendermint::pegin::PegInInfo>,
    /// Miner's fee recipient address (for peg-in compensation)
    pub fee_recipient: Address,
    /// Correlation ID for distributed tracing
    pub correlation_id: Uuid,
}

/// Response for SubmitAuxBlock (per Document 16)
#[derive(Debug, Clone)]
pub struct SubmitAuxBlockResponse {
    /// The validated AuxPoW header
    pub auxpow_header: AuxPowHeader,
    /// Whether AuxPoW was accepted
    pub accepted: bool,
    /// Number of new peg-ins queued (deduplicated)
    pub pegins_queued: usize,
    /// Height of the AuxPoW header
    pub height: u64,
}

// ============================================================================
// Tendermint RPC Query Messages (Phase 4: Document 12)
// ============================================================================

/// Get current Tendermint consensus state (for RPC: tendermint_consensusState)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<TendermintStateResponse, crate::actors_v2::chain::ChainError>")]
pub struct GetTendermintState {
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for GetTendermintState
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendermintStateResponse {
    /// Current consensus height
    pub height: u64,
    /// Current consensus round
    pub round: u32,
    /// Current step: "Propose" | "Prevote" | "Precommit" | "Commit"
    pub step: String,
    /// Block hash being proposed/voted on (if any)
    pub proposal_block_hash: Option<H256>,
    /// Locked block hash
    pub locked_block_hash: Option<H256>,
    /// Locked round
    pub locked_round: Option<u32>,
    /// Valid block hash
    pub valid_block_hash: Option<H256>,
    /// Valid round
    pub valid_round: Option<u32>,
    /// Prevotes received count
    pub prevotes_count: u32,
    /// Precommits received count
    pub precommits_count: u32,
    /// Total validator count
    pub total_validators: u32,
}

/// Get validator set for a height (for RPC: tendermint_validators)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<ValidatorSetResponse, crate::actors_v2::chain::ChainError>")]
pub struct GetValidatorSet {
    /// Height to query (None = current)
    pub height: Option<u64>,
    /// Page number (1-indexed, defaults to 1)
    pub page: u32,
    /// Results per page (defaults to 30)
    pub per_page: u32,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for GetValidatorSet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSetResponse {
    /// Height at which this validator set is active
    pub height: u64,
    /// Validators in this page
    pub validators: Vec<ValidatorInfoResponse>,
    /// Number in this page
    pub count: u32,
    /// Total validators
    pub total: u32,
}

/// Validator information for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfoResponse {
    /// Validator index
    pub index: u32,
    /// Hex-encoded address/ID
    pub address: String,
    /// Public key (base64)
    pub public_key: String,
    /// Voting power
    pub voting_power: u64,
}

/// Get commit for a height (for RPC: tendermint_commit)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<CommitResponse, crate::actors_v2::chain::ChainError>")]
pub struct GetCommit {
    /// Height to query (None = latest)
    pub height: Option<u64>,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for GetCommit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResponse {
    /// Height of the commit
    pub height: u64,
    /// Round at which commit happened
    pub round: u32,
    /// Block hash that was committed
    pub block_hash: H256,
    /// Number of signatures
    pub signatures_count: u32,
    /// Whether block is canonical
    pub canonical: bool,
    /// Whether commit proof is available (false for head block until next block produced)
    pub commit_available: bool,
}

/// Get chain parameters (for RPC: tendermint_params)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<ChainParamsResponse, crate::actors_v2::chain::ChainError>")]
pub struct GetChainParams {
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for GetChainParams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainParamsResponse {
    /// Height at which these params are active
    pub height: u64,
    /// Block size limit
    pub max_block_bytes: u64,
    /// Max gas per block
    pub max_gas: i64,
    /// Evidence max age in blocks
    pub evidence_max_age_blocks: u64,
    /// Peg-in minimum satoshis
    pub pegin_minimum_satoshis: u64,
    /// Peg-in confirmation depth
    pub pegin_confirmation_depth: u32,
    /// Miner fee basis points
    pub miner_fee_bps: u64,
}

/// Get pending governance updates (for RPC: tendermint_pendingGovernanceUpdates)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<PendingGovernanceResponse, crate::actors_v2::chain::ChainError>")]
pub struct GetPendingGovernance {
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for GetPendingGovernance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingGovernanceResponse {
    /// Pending updates
    pub updates: Vec<PendingGovernanceUpdate>,
}

/// Pending governance update info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingGovernanceUpdate {
    /// Update type: "Validator" | "Parameter" | "Emergency"
    pub update_type: String,
    /// Height at which update activates
    pub activation_height: u64,
    /// Height at which update was proposed
    pub proposed_at_height: u64,
}

/// Get detected equivocation evidence (for RPC: tendermint_evidence)
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<EvidenceResponse, crate::actors_v2::chain::ChainError>")]
pub struct GetEvidence {
    /// Maximum age in blocks to include (None = all)
    pub max_age_blocks: Option<u64>,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for GetEvidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceResponse {
    /// List of detected equivocation evidence
    pub evidence: Vec<EvidenceInfo>,
    /// Total evidence count
    pub total: usize,
}

/// Evidence information for RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceInfo {
    /// Evidence type: "DoublePrevote" | "DoublePrecommit" | "DoubleProposal"
    pub evidence_type: String,
    /// Validator who equivocated (hex-encoded address)
    pub validator_address: String,
    /// Height at which equivocation occurred
    pub height: u64,
    /// Round at which equivocation occurred
    pub round: u32,
    /// Block hash of first vote
    pub vote_a_block_hash: Option<String>,
    /// Block hash of second (conflicting) vote
    pub vote_b_block_hash: Option<String>,
    /// Timestamp when evidence was detected (RFC3339 formatted)
    pub detected_at: String,
}

// ============================================================================
// Issue 3.2: Internal State Query Messages (for TendermintDriver)
// ============================================================================

/// Query current Tendermint position for timeout coordination.
///
/// This is an internal message used by TendermintDriver to query the
/// consensus position from ChainActor (the single source of truth).
/// Issue 3.2: Eliminates duplicate state between TendermintDriver and ChainActor.
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<TendermintPositionSnapshot, crate::actors_v2::chain::ChainError>")]
pub struct QueryTendermintPosition {
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Snapshot of consensus position for TendermintDriver timeout decisions.
///
/// This is a lightweight internal snapshot (vs. the full TendermintStateResponse for RPC).
/// Contains only what TendermintDriver needs for timeout coordination.
#[derive(Debug, Clone)]
pub struct TendermintPositionSnapshot {
    /// Current consensus height
    pub height: u64,
    /// Current consensus round
    pub round: u32,
    /// Current step (as enum, not string)
    pub step: crate::actors_v2::chain::tendermint::TendermintStep,
    /// Whether we are the proposer for this round
    pub is_proposer: bool,
    /// Locked round (if any)
    pub locked_round: Option<u32>,
    /// Locked block hash (if any)
    pub locked_block: Option<crate::actors_v2::chain::tendermint::BlockHash>,
}

// ============================================================================
// Issue 3.1: WAL Recovery Integration Messages
// ============================================================================

/// Apply recovered consensus state from WAL replay.
///
/// This message is sent to ChainActor at startup after WAL replay to restore
/// consensus state that was persisted before a crash. This ensures:
/// - No double-voting (sent_prevotes/sent_precommits restored)
/// - Lock state preserved across restarts
/// - Consensus resumes at correct height/round
///
/// Issue 3.1: Proper integration of WAL recovery with ChainActor's TendermintState.
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<ApplyRecoveredStateResponse, crate::actors_v2::chain::ChainError>")]
pub struct ApplyRecoveredState {
    /// Recovered state from WAL replay
    pub recovered: crate::actors_v2::chain::tendermint::wal::RecoveredState,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Response for ApplyRecoveredState
#[derive(Debug, Clone)]
pub struct ApplyRecoveredStateResponse {
    /// Whether recovery was applied (false if no state to recover)
    pub applied: bool,
    /// Height after recovery
    pub height: u64,
    /// Round after recovery
    pub round: u32,
    /// Whether a lock was restored
    pub lock_restored: bool,
    /// Number of prevotes restored
    pub prevotes_restored: usize,
    /// Number of precommits restored
    pub precommits_restored: usize,
}

// ============================================================================
// Issue 4.2: ValidatorSetTracker Integration Messages
// ============================================================================

/// Set the TendermintSyncValidator reference for governance notifications.
///
/// Issue 4.2: When governance updates change the validator set, ChainActor
/// notifies the sync validator so it can track validator set changes.
/// This enables proper commit verification during sync.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetSyncValidator {
    /// Arc<RwLock<TendermintSyncValidator>> for thread-safe access
    /// Note: Uses std::sync::RwLock to match SyncActor's validator type
    pub validator: std::sync::Arc<std::sync::RwLock<crate::actors_v2::network::tendermint_sync::TendermintSyncValidator>>,
}

impl std::fmt::Debug for SetSyncValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetSyncValidator")
            .field("validator", &"<TendermintSyncValidator>")
            .finish()
    }
}

/// Notification that a validator set change occurred.
///
/// Sent to notify the sync validator when governance updates the validator set.
/// The sync validator uses this to track which validator sets are active at which heights.
#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub struct ValidatorSetChanged {
    /// Height at which the new set becomes active
    pub activation_height: u64,
    /// The new validator set
    pub new_set: crate::actors_v2::chain::tendermint::ValidatorSet,
}
