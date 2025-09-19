//! SyncActor V2 Message Handlers
//!
//! Handles blockchain synchronization operations for SyncActor.
//! Simplified from V1's complex state machine handling.

use anyhow::{Result, anyhow};

use crate::actors_v2::network::{
    SyncMessage, SyncResponse, SyncError,
    messages::{SyncStatus, PeerId, Block},
};

/// SyncActor message handling utilities
pub struct SyncMessageHandlers;

impl SyncMessageHandlers {
    /// Validate sync configuration on startup
    pub fn validate_sync_startup(
        current_height: u64,
        target_height: u64,
        available_peers: &[PeerId],
    ) -> Result<()> {
        if available_peers.is_empty() {
            return Err(anyhow!("No peers available for sync"));
        }

        if target_height <= current_height {
            return Err(anyhow!(
                "Target height ({}) must be greater than current height ({})",
                target_height,
                current_height
            ));
        }

        Ok(())
    }

    /// Create sync status response
    pub fn create_sync_status(
        current_height: u64,
        target_height: u64,
        is_syncing: bool,
        sync_peers: Vec<PeerId>,
        pending_requests: usize,
    ) -> SyncStatus {
        SyncStatus {
            current_height,
            target_height,
            is_syncing,
            sync_peers,
            pending_requests,
        }
    }

    /// Validate block request parameters
    pub fn validate_block_request(start_height: u64, count: u32, peer_id: Option<&PeerId>) -> Result<()> {
        if count == 0 {
            return Err(anyhow!("Block count must be greater than 0"));
        }

        if count > 1000 {
            return Err(anyhow!("Block count too large: {} (max 1000)", count));
        }

        if let Some(peer) = peer_id {
            if peer.is_empty() {
                return Err(anyhow!("Peer ID cannot be empty"));
            }
        }

        Ok(())
    }

    /// Validate incoming block
    pub fn validate_incoming_block(block: &Block, peer_id: &PeerId) -> Result<()> {
        if block.is_empty() {
            return Err(anyhow!("Block data cannot be empty"));
        }

        if peer_id.is_empty() {
            return Err(anyhow!("Source peer ID cannot be empty"));
        }

        // Basic size validation
        if block.len() > 50 * 1024 * 1024 { // 50MB max block size
            return Err(anyhow!("Block too large: {} bytes", block.len()));
        }

        // Additional validation would go here in real implementation
        // - Block header validation
        // - Signature verification
        // - Merkle root validation
        // - etc.

        Ok(())
    }

    /// Validate block response from network
    pub fn validate_block_response(blocks: &[Block], request_id: &str) -> Result<()> {
        if request_id.is_empty() {
            return Err(anyhow!("Request ID cannot be empty"));
        }

        if blocks.is_empty() {
            return Err(anyhow!("Block response cannot be empty"));
        }

        if blocks.len() > 1000 {
            return Err(anyhow!("Too many blocks in response: {}", blocks.len()));
        }

        // Validate each block
        for (i, block) in blocks.iter().enumerate() {
            if block.is_empty() {
                return Err(anyhow!("Block {} in response is empty", i));
            }

            if block.len() > 50 * 1024 * 1024 {
                return Err(anyhow!("Block {} too large: {} bytes", i, block.len()));
            }
        }

        Ok(())
    }

    /// Calculate sync progress percentage
    pub fn calculate_sync_progress(current_height: u64, target_height: u64) -> f64 {
        if target_height == 0 {
            return 0.0;
        }

        let progress = current_height as f64 / target_height as f64;
        progress.min(1.0).max(0.0)
    }

    /// Estimate time remaining for sync
    pub fn estimate_sync_time_remaining(
        current_height: u64,
        target_height: u64,
        blocks_per_second: f64,
    ) -> Option<std::time::Duration> {
        if blocks_per_second <= 0.0 || target_height <= current_height {
            return None;
        }

        let blocks_remaining = target_height - current_height;
        let seconds_remaining = blocks_remaining as f64 / blocks_per_second;

        Some(std::time::Duration::from_secs_f64(seconds_remaining))
    }

    /// Handle sync completion
    pub fn handle_sync_completion(final_height: u64) -> Result<SyncResponse> {
        tracing::info!("Blockchain sync completed at height {}", final_height);

        Ok(SyncResponse::Status(SyncStatus {
            current_height: final_height,
            target_height: final_height,
            is_syncing: false,
            sync_peers: Vec::new(),
            pending_requests: 0,
        }))
    }

    /// Handle sync error
    pub fn handle_sync_error(error: &str, current_state: &str) -> SyncError {
        tracing::error!("Sync error in state '{}': {}", current_state, error);

        SyncError::Internal(format!("Sync failed in state '{}': {}", current_state, error))
    }

    /// Validate peer list update
    pub fn validate_peer_update(peers: &[PeerId]) -> Result<()> {
        if peers.is_empty() {
            return Err(anyhow!("Peer list cannot be empty"));
        }

        if peers.len() > 1000 {
            return Err(anyhow!("Too many peers: {} (max 1000)", peers.len()));
        }

        // Check for duplicate peers
        let mut unique_peers = std::collections::HashSet::new();
        for peer in peers {
            if peer.is_empty() {
                return Err(anyhow!("Peer ID cannot be empty"));
            }

            if !unique_peers.insert(peer) {
                return Err(anyhow!("Duplicate peer ID: {}", peer));
            }
        }

        Ok(())
    }

    /// Generate block processed response
    pub fn create_block_processed_response(block_height: u64) -> SyncResponse {
        SyncResponse::BlockProcessed { block_height }
    }

    /// Generate blocks requested response
    pub fn create_blocks_requested_response() -> SyncResponse {
        let request_id = uuid::Uuid::new_v4().to_string();
        SyncResponse::BlocksRequested { request_id }
    }
}