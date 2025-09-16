//! Synchronization and peer management messages

use crate::types::*;
use actix::prelude::*;

/// Message to add a peer for synchronization
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct AddPeerMessage {
    pub peer_info: PeerInfo,
}

/// Message to remove a peer from synchronization
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct RemovePeerMessage {
    pub peer_id: PeerId,
}

/// Message to start synchronization
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct StartSyncMessage {
    pub target_block: u64,
    pub peer_id: Option<PeerId>,
}

/// Message to stop synchronization
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct StopSyncMessage;

/// Message to get synchronization status
#[derive(Message)]
#[rtype(result = "SyncStatus")]
pub struct GetSyncStatusMessage;

/// Message to handle a downloaded block
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct BlockDownloadedMessage {
    pub block: ConsensusBlock,
    pub peer_id: PeerId,
}

/// Message to handle block download failure
#[derive(Message)]
#[rtype(result = "()")]
pub struct BlockDownloadFailedMessage {
    pub block_hash: BlockHash,
    pub peer_id: PeerId,
    pub error: String,
}

/// Message to request blocks from a peer
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct RequestBlocksMessage {
    pub peer_id: PeerId,
    pub start_block: u64,
    pub count: u64,
}

/// Message to handle peer status update
#[derive(Message)]
#[rtype(result = "()")]
pub struct PeerStatusUpdateMessage {
    pub peer_id: PeerId,
    pub status: PeerStatus,
}

/// Message to get peer information
#[derive(Message)]
#[rtype(result = "Vec<PeerInfo>")]
pub struct GetPeersMessage;

/// Message to ban a peer
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct BanPeerMessage {
    pub peer_id: PeerId,
    pub reason: String,
    pub duration: std::time::Duration,
}

/// Message to handle sync progress update
#[derive(Message)]
#[rtype(result = "()")]
pub struct SyncProgressMessage {
    pub current_block: u64,
    pub target_block: u64,
    pub progress: f64,
}

/// Peer information for synchronization
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub best_block: BlockRef,
    pub capabilities: PeerCapabilities,
    pub connection_quality: ConnectionQuality,
    pub reputation: PeerReputation,
}

/// Peer capabilities
#[derive(Debug, Clone)]
pub struct PeerCapabilities {
    pub protocol_version: u32,
    pub max_block_request_size: u64,
    pub supports_fast_sync: bool,
    pub supports_state_sync: bool,
}

/// Connection quality metrics
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    pub latency_ms: u64,
    pub bandwidth_kbps: u64,
    pub reliability_score: f64,
    pub packet_loss_rate: f64,
}

/// Peer reputation tracking
#[derive(Debug, Clone)]
pub struct PeerReputation {
    pub score: i32,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub last_interaction: std::time::SystemTime,
}

/// Current peer status
#[derive(Debug, Clone)]
pub enum PeerStatus {
    Connected {
        best_block: BlockRef,
        sync_state: PeerSyncState,
    },
    Disconnected,
    Banned {
        reason: String,
        until: std::time::SystemTime,
    },
}

/// Peer synchronization state
#[derive(Debug, Clone)]
pub enum PeerSyncState {
    Idle,
    Syncing {
        requested_blocks: std::ops::Range<u64>,
        pending_requests: u32,
    },
    UpToDate,
}

/// Synchronization status
#[derive(Debug, Clone)]
pub enum SyncStatus {
    Idle,
    Syncing {
        current_block: u64,
        target_block: u64,
        progress: f64,
        syncing_peers: Vec<PeerId>,
    },
    UpToDate,
    Stalled {
        reason: String,
        last_progress: std::time::SystemTime,
    },
}

/// Block request information
#[derive(Debug, Clone)]
pub struct BlockRequest {
    pub start_block: u64,
    pub count: u64,
    pub reverse: bool,
    pub skip: u64,
}

/// Block response from peer
#[derive(Debug, Clone)]
pub struct BlockResponse {
    pub blocks: Vec<ConsensusBlock>,
    pub peer_id: PeerId,
    pub request_id: u64,
}

/// Fast sync state request
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct RequestStateSyncMessage {
    pub state_root: Hash256,
    pub peer_id: PeerId,
}

/// State sync response
#[derive(Message)]
#[rtype(result = "Result<(), SyncError>")]
pub struct StateSyncResponseMessage {
    pub state_data: Vec<StateTrieNode>,
    pub peer_id: PeerId,
    pub is_complete: bool,
}

/// State trie node for fast sync
#[derive(Debug, Clone)]
pub struct StateTrieNode {
    pub path: Vec<u8>,
    pub value: Option<Vec<u8>>,
    pub children: Vec<Hash256>,
}

/// Sync metrics and statistics
#[derive(Debug, Clone)]
pub struct SyncMetrics {
    pub blocks_downloaded: u64,
    pub download_rate_bps: f64,
    pub active_peers: usize,
    pub failed_downloads: u64,
    pub average_download_time: std::time::Duration,
    pub estimated_completion: Option<std::time::Duration>,
}