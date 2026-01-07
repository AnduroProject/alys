//! NetworkActor V2 Message System
//!
//! Split message system for two-actor architecture:
//! - NetworkMessage: P2P protocol operations
//! - SyncMessage: Blockchain synchronization operations
//!
//! Removed from V1: Complex supervision messages, actor_system dependencies

use actix::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Re-export common types (these would be defined elsewhere in the codebase)
pub type PeerId = String; // Simplified for now
pub type Block = Vec<u8>; // Simplified for now
pub type Transaction = Vec<u8>; // Simplified for now

/// NetworkActor messages - P2P protocols only
#[derive(Debug, Message)]
#[rtype(result = "Result<NetworkResponse, NetworkError>")]
pub enum NetworkMessage {
    /// Start networking subsystem
    StartNetwork {
        listen_addrs: Vec<String>,
        bootstrap_peers: Vec<String>,
    },
    /// Stop networking subsystem
    StopNetwork { graceful: bool },
    /// Get current network status (Phase 4: Enhanced with readiness check)
    GetNetworkStatus,
    /// Broadcast block to network (Phase 1/4: Production-ready with correlation tracking)
    BroadcastBlock { block_data: Vec<u8>, priority: bool },
    /// Broadcast transaction to network
    BroadcastTransaction { tx_data: Vec<u8> },
    /// Broadcast AuxPoW header for mining coordination (Phase 4: Task 4.2.1)
    BroadcastAuxPow {
        auxpow_data: Vec<u8>,
        correlation_id: Option<Uuid>,
    },
    /// Handle completed AuxPoW from miner (Phase 4: Integration Point 3a)
    HandleCompletedAuxPow {
        auxpow_data: Vec<u8>,
        peer_id: String,
        correlation_id: Option<Uuid>,
    },
    /// Request blocks from peers (Phase 4: Enhanced sync support)
    RequestBlocks {
        start_height: u64,
        count: u32,
        correlation_id: Option<Uuid>,
    },
    /// Handle block response from peer (Phase 4: Task 2.6)
    HandleBlockResponse {
        blocks: Vec<Block>,
        request_id: Uuid,
        peer_id: String,
        correlation_id: Option<Uuid>,
    },
    /// Connect to specific peer
    ConnectToPeer { peer_addr: String },
    /// Disconnect from peer
    DisconnectPeer { peer_id: PeerId },
    /// Get connected peers
    GetConnectedPeers,
    /// Handle incoming gossip message
    HandleGossipMessage {
        message: GossipMessage,
        peer_id: PeerId,
    },
    /// Handle request-response message
    HandleRequestResponse {
        request: NetworkRequest,
        peer_id: PeerId,
    },
    /// Set SyncActor address for coordination
    SetSyncActor {
        addr: Addr<crate::actors_v2::network::SyncActor>,
    },
    /// Set ChainActor address for AuxPoW forwarding (Phase 4: Integration Point 3b)
    SetChainActor {
        addr: Addr<crate::actors_v2::chain::ChainActor>,
    },
    /// Set StorageActor address for block request handling
    SetStorageActor {
        addr: Addr<crate::actors_v2::storage::StorageActor>,
    },
    /// Get network metrics
    GetMetrics,
    /// Health check for production monitoring (Phase 4: Task 4.3.1)
    HealthCheck { correlation_id: Option<Uuid> },
    /// Cleanup timed-out requests (Phase 4: Task 7)
    CleanupTimeouts,
    /// Query connected peers for their chain heights (for sync)
    /// Sends GetChainStatus to all connected peers and reports results to SyncActor
    QueryPeerHeights,
}

/// SyncActor messages - blockchain sync only
#[derive(Debug, Message)]
#[rtype(result = "Result<SyncResponse, SyncError>")]
pub enum SyncMessage {
    /// Start synchronization
    StartSync {
        start_height: u64,
        target_height: Option<u64>, // None means discover from network
    },
    /// Stop synchronization
    StopSync,
    /// Get sync status
    GetSyncStatus,
    /// Request blocks from network
    RequestBlocks {
        start_height: u64,
        count: u32,
        peer_id: Option<PeerId>,
    },
    /// Handle new block from network
    HandleNewBlock { block: Block, peer_id: PeerId },
    /// Handle block response from peer (blocks received via request-response protocol)
    HandleBlockResponse {
        blocks: Vec<Block>,
        request_id: String,
        peer_id: PeerId,
    },
    /// Set NetworkActor address for coordination
    SetNetworkActor {
        addr: Addr<crate::actors_v2::network::NetworkActor>,
    },
    /// Set ChainActor address for block forwarding
    SetChainActor {
        addr: Addr<crate::actors_v2::chain::ChainActor>,
    },
    /// Update available peers for sync
    UpdatePeers { peers: Vec<PeerId> },
    /// Get sync metrics
    GetMetrics,
    /// Query network peers for consensus chain height
    QueryNetworkHeight,
    /// Report peer heights received from network queries
    /// NetworkActor sends this after querying peers for their chain status
    ReportPeerHeights {
        /// Map of peer_id -> (height, head_hash)
        peer_heights: Vec<(PeerId, u64, [u8; 32])>,
    },
    /// Load checkpoint on startup (Phase 5)
    LoadCheckpoint,
    /// Save checkpoint during sync (Phase 5)
    SaveCheckpoint,
    /// Clear checkpoint after sync completion (Phase 5)
    ClearCheckpoint,

    // Network height monitoring messages (Active Height Monitoring feature)
    /// Force refresh of network height from peers (used after reconnection)
    /// Triggers immediate QueryPeerHeights to NetworkActor
    RefreshNetworkHeight,
    /// Force re-sync (emergency recovery, e.g., after repeated PayloadIdUnavailable errors)
    ForceResync { reason: String },
}

/// NetworkActor response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkResponse {
    Started,
    Stopped,
    /// Network status with readiness information (Phase 4: Enhanced)
    Status(NetworkStatus),
    /// Block broadcast confirmation with timing (Phase 4: Enhanced monitoring)
    BlockBroadcasted {
        peer_count: usize,
        broadcast_time: std::time::Duration,
    },
    /// Generic broadcast confirmation
    Broadcasted {
        message_id: String,
    },
    /// AuxPoW broadcast confirmation (Phase 4: Task 4.2.1)
    AuxPowBroadcasted {
        peer_count: usize,
    },
    /// Block request sent confirmation (Phase 4)
    BlocksRequested {
        peer_count: usize,
        request_id: Uuid,
    },
    Connected {
        peer_id: PeerId,
    },
    Disconnected {
        peer_id: PeerId,
    },
    Peers(Vec<PeerInfo>),
    Metrics(crate::actors_v2::network::NetworkMetrics),
    /// Health check response (Phase 4: Task 4.3.1)
    Healthy {
        is_healthy: bool,
        connected_peers: usize,
        issues: Vec<String>,
    },
}

/// SyncActor response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncResponse {
    Started,
    Stopped,
    Status(SyncStatus),
    BlocksRequested { request_id: String },
    BlockProcessed { block_height: u64 },
    Metrics(crate::actors_v2::network::SyncMetrics),
    /// Network height from peer consensus
    NetworkHeight { height: u64 },
    /// Already synced response
    AlreadySynced,
}

/// Network status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub local_peer_id: PeerId,
    pub connected_peers: usize,
    pub listening_addresses: Vec<String>,
    pub is_running: bool,
    pub chain_height: u64,  // Current blockchain height from ChainActor
}

/// Sync status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub current_height: u64,
    pub target_height: u64,
    pub is_syncing: bool,
    pub sync_peers: Vec<PeerId>,
    pub pending_requests: usize,
}

/// Peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub address: String,
    pub connection_time: std::time::SystemTime,
    pub reputation: f64,
}

/// Gossip message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    pub topic: String,
    pub data: Vec<u8>,
    pub message_id: String,
}

/// Network request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkRequest {
    GetBlocks { start_height: u64, count: u32 },
    GetChainStatus,
    GetPeers,
    GetStatus,
}

/// Network error types
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Network not started")]
    NotStarted,
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Sync error types
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Sync not started")]
    NotStarted,
    #[error("No peers available")]
    NoPeers,
    #[error("Block validation failed: {0}")]
    BlockValidation(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Network query failed: {0}")]
    NetworkQuery(String),
    #[error("Insufficient peers for consensus: {0}")]
    InsufficientPeers(String),
    #[error("ChainActor not set")]
    ChainActorNotSet,
    #[error("NetworkActor not set")]
    NetworkActorNotSet,
    #[error("Block validation failed: {0}")]
    ValidationFailed(String),
    #[error("Peer request timeout")]
    RequestTimeout,
    #[error("Invalid block response: {0}")]
    InvalidResponse(String),
    #[error("Sync failed: {0}")]
    SyncFailed(String),
    #[error("Actor mailbox error: {0}")]
    MailboxError(String),
}

// Conversion from actix MailboxError
impl From<actix::MailboxError> for SyncError {
    fn from(e: actix::MailboxError) -> Self {
        SyncError::MailboxError(e.to_string())
    }
}
