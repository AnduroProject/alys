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
pub use crate::store::BlockRef;
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
#[derive(Debug, Message)]
#[rtype(result = "Result<AuxPowHeader, crate::actors_v2::chain::ChainError>")]
pub struct SubmitAuxBlock {
    /// Aggregate hash from createauxblock response
    pub aggregate_hash: BitcoinBlockHash,
    /// Completed AuxPoW proof
    pub auxpow: AuxPow,
    /// Correlation ID for distributed tracing
    pub correlation_id: Uuid,
}
