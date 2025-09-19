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
    StopNetwork {
        graceful: bool,
    },
    /// Get current network status
    GetNetworkStatus,
    /// Broadcast block to network
    BroadcastBlock {
        block_data: Vec<u8>,
        priority: bool,
    },
    /// Broadcast transaction to network
    BroadcastTransaction {
        tx_data: Vec<u8>,
    },
    /// Connect to specific peer
    ConnectToPeer {
        peer_addr: String,
    },
    /// Disconnect from peer
    DisconnectPeer {
        peer_id: PeerId,
    },
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
    /// Get network metrics
    GetMetrics,
}

/// SyncActor messages - blockchain sync only
#[derive(Debug, Message)]
#[rtype(result = "Result<SyncResponse, SyncError>")]
pub enum SyncMessage {
    /// Start synchronization
    StartSync,
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
    HandleNewBlock {
        block: Block,
        peer_id: PeerId,
    },
    /// Handle block response
    HandleBlockResponse {
        blocks: Vec<Block>,
        request_id: String,
    },
    /// Set NetworkActor address for coordination
    SetNetworkActor {
        addr: Addr<crate::actors_v2::network::NetworkActor>,
    },
    /// Set StorageActor address for coordination
    SetStorageActor {
        addr: Addr<crate::actors_v2::storage::StorageActor>,
    },
    /// Update available peers for sync
    UpdatePeers {
        peers: Vec<PeerId>,
    },
    /// Get sync metrics
    GetMetrics,
}

/// NetworkActor response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkResponse {
    Started,
    Stopped,
    Status(NetworkStatus),
    Broadcasted { message_id: String },
    Connected { peer_id: PeerId },
    Disconnected { peer_id: PeerId },
    Peers(Vec<PeerInfo>),
    Metrics(crate::actors_v2::network::NetworkMetrics),
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
}

/// Network status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub local_peer_id: PeerId,
    pub connected_peers: usize,
    pub listening_addresses: Vec<String>,
    pub is_running: bool,
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
    GetBlocks {
        start_height: u64,
        count: u32,
    },
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
}