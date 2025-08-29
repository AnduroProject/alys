//! SyncActor Message Protocol
//! 
//! Defines all messages for blockchain synchronization operations including
//! block requests, sync status, and production eligibility checks.

use actix::{Message, Result as ActorResult};
use serde::{Deserialize, Serialize};
use ethereum_types::H256;
use crate::actors::network::messages::{NetworkMessage, NetworkResult};

/// Sync operation modes with different performance characteristics
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SyncMode {
    /// Fast sync with parallel validation (default)
    Fast,
    /// Full validation sync for highest security
    Full,
    /// Checkpoint-based recovery sync
    Recovery,
    /// Federation-only sync for consensus nodes
    Federation,
}

impl Default for SyncMode {
    fn default() -> Self {
        SyncMode::Fast
    }
}

/// Start blockchain synchronization
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<SyncResponse>>")]
pub struct StartSync {
    pub from_height: Option<u64>,
    pub target_height: Option<u64>,
    pub sync_mode: SyncMode,
    pub priority_peers: Vec<String>, // Peer IDs for preferred sync sources
}

impl NetworkMessage for StartSync {}

/// Stop ongoing synchronization
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct StopSync {
    pub force: bool, // Force stop even if in critical sync phase
}

impl NetworkMessage for StopSync {}

/// Check if node can produce blocks (99.5% threshold)
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<bool>>")]
pub struct CanProduceBlocks;

impl NetworkMessage for CanProduceBlocks {}

/// Get current synchronization status
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<SyncStatus>>")]
pub struct GetSyncStatus;

impl NetworkMessage for GetSyncStatus {}

/// Request specific blocks from peers
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<BlocksResponse>>")]
pub struct RequestBlocks {
    pub start_height: u64,
    pub count: u32,
    pub preferred_peers: Vec<String>,
}

impl NetworkMessage for RequestBlocks {}

/// Create a synchronization checkpoint
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<CheckpointResponse>>")]
pub struct CreateCheckpoint {
    pub height: Option<u64>, // None = current height
    pub compression: bool,
}

impl NetworkMessage for CreateCheckpoint {}

/// Restore from a synchronization checkpoint
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<RestoreResponse>>")]
pub struct RestoreCheckpoint {
    pub checkpoint_id: String,
    pub verify_integrity: bool,
}

impl NetworkMessage for RestoreCheckpoint {}

/// Sync operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub operation_id: String,
    pub started_at: std::time::SystemTime,
    pub mode: SyncMode,
    pub initial_height: u64,
    pub target_height: Option<u64>,
}

/// Detailed synchronization status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_height: u64,
    pub target_height: Option<u64>,
    pub sync_progress: f64, // 0.0 to 1.0
    pub blocks_per_second: f64,
    pub eta_seconds: Option<u64>,
    pub connected_peers: u32,
    pub active_downloads: u32,
    pub validation_queue_size: u32,
    pub can_produce_blocks: bool, // True if >= 99.5% synced
    pub last_block_hash: Option<H256>,
    pub sync_mode: SyncMode,
    pub checkpoint_info: Option<CheckpointInfo>,
}

/// Block request response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocksResponse {
    pub blocks: Vec<BlockData>,
    pub more_available: bool,
    pub source_peers: Vec<String>,
}

/// Simplified block data for sync operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData {
    pub height: u64,
    pub hash: H256,
    pub parent_hash: H256,
    pub timestamp: u64,
    pub data: Vec<u8>, // Serialized block
    pub signature: Option<Vec<u8>>, // Federation signature if applicable
}

/// Checkpoint creation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointResponse {
    pub checkpoint_id: String,
    pub height: u64,
    pub created_at: std::time::SystemTime,
    pub compressed: bool,
    pub size_bytes: u64,
}

/// Checkpoint restoration response  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResponse {
    pub restored_height: u64,
    pub restored_at: std::time::SystemTime,
    pub verified: bool,
    pub blocks_restored: u64,
}

/// Checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub last_checkpoint_height: u64,
    pub last_checkpoint_time: std::time::SystemTime,
    pub available_checkpoints: u32,
    pub next_checkpoint_eta: Option<std::time::Duration>,
}

// Internal sync events for actor coordination
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct SyncProgressUpdate {
    pub current_height: u64,
    pub progress: f64,
    pub blocks_per_second: f64,
}

impl NetworkMessage for SyncProgressUpdate {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct SyncCompleted {
    pub final_height: u64,
    pub total_blocks: u64,
    pub duration: std::time::Duration,
    pub average_bps: f64,
}

impl NetworkMessage for SyncCompleted {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct SyncError {
    pub error: String,
    pub height: Option<u64>,
    pub recoverable: bool,
}

impl NetworkMessage for SyncError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_status_production_threshold() {
        let mut status = SyncStatus {
            is_syncing: true,
            current_height: 995,
            target_height: Some(1000),
            sync_progress: 0.995,
            blocks_per_second: 250.0,
            eta_seconds: Some(2),
            connected_peers: 10,
            active_downloads: 4,
            validation_queue_size: 100,
            can_produce_blocks: true,
            last_block_hash: None,
            sync_mode: SyncMode::Fast,
            checkpoint_info: None,
        };

        // At 99.5% should allow production
        assert!(status.can_produce_blocks);
        
        // Below threshold should not allow production
        status.sync_progress = 0.994;
        status.can_produce_blocks = false;
        assert!(!status.can_produce_blocks);
    }

    #[test]
    fn sync_modes() {
        assert_eq!(SyncMode::default(), SyncMode::Fast);
        
        let start_msg = StartSync {
            from_height: None,
            target_height: None,
            sync_mode: SyncMode::Federation,
            priority_peers: vec!["peer1".to_string()],
        };
        
        assert_eq!(start_msg.sync_mode, SyncMode::Federation);
        assert_eq!(start_msg.priority_peers.len(), 1);
    }
}