//! Synchronization Protocol Extensions
//!
//! This module provides sync-specific types and utilities that extend
//! the existing request_response protocol.
//!
//! Note: The base ChainStatus and BlockRange protocols are already implemented
//! in the request_response module as BlockRequest::GetChainStatus and
//! BlockRequest::GetBlocks. This module provides sync state management
//! and automatic catchup logic.

use ssz_derive::{Decode as DecodeDeriv, Encode as EncodeDeriv};

/// Maximum blocks that can be requested in a single BlockRange request
pub const MAX_BLOCKS_PER_REQUEST: u64 = 128;

/// Re-export from request_response for convenience
pub use super::request_response::{
    BlockRangeRequest, BlockRequest, BlockResponse, ChainStatusResponse,
};

// ============================================================================
// Sync State Management Types
// ============================================================================

/// Sync state for tracking synchronization progress
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    /// Node is fully synced with the network
    Synced,

    /// Node is actively syncing
    Syncing {
        /// Current height we have
        current_height: u64,
        /// Target height from best peer
        target_height: u64,
        /// Peer we're syncing from
        sync_peer: Option<String>,
    },

    /// Not synced, waiting to discover peers
    NotSynced,

    /// Sync failed with error
    Failed {
        /// Error message
        error: String,
        /// Whether we can retry
        can_retry: bool,
    },
}

impl SyncState {
    /// Check if we're currently synced
    pub fn is_synced(&self) -> bool {
        matches!(self, SyncState::Synced)
    }

    /// Check if we're actively syncing
    pub fn is_syncing(&self) -> bool {
        matches!(self, SyncState::Syncing { .. })
    }

    /// Get sync progress as a percentage (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        match self {
            SyncState::Synced => 1.0,
            SyncState::Syncing {
                current_height,
                target_height,
                ..
            } => {
                if *target_height == 0 {
                    0.0
                } else {
                    (*current_height as f64) / (*target_height as f64)
                }
            }
            SyncState::NotSynced | SyncState::Failed { .. } => 0.0,
        }
    }
}

/// Peer sync information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerSyncInfo {
    /// Peer's reported chain height
    pub height: u64,

    /// Peer's head hash
    pub head_hash: [u8; 32],

    /// Peer's genesis hash (for compatibility check)
    pub genesis_hash: Option<[u8; 32]>,

    /// Whether peer reports as synced
    pub is_synced: bool,

    /// Last time we queried this peer
    pub last_updated: std::time::Instant,
}

impl PeerSyncInfo {
    /// Check if this peer is compatible with our chain
    pub fn is_compatible(&self, our_genesis: &[u8; 32]) -> bool {
        match self.genesis_hash {
            Some(ref genesis) => genesis == our_genesis,
            None => true, // Assume compatible if no genesis provided
        }
    }

    /// Check if this peer info is stale (> 30 seconds old)
    pub fn is_stale(&self) -> bool {
        self.last_updated.elapsed().as_secs() > 30
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_state_is_synced() {
        let synced = SyncState::Synced;
        assert!(synced.is_synced());
        assert!(!synced.is_syncing());
        assert_eq!(synced.progress(), 1.0);
    }

    #[test]
    fn test_sync_state_syncing_progress() {
        let syncing = SyncState::Syncing {
            current_height: 50,
            target_height: 100,
            sync_peer: Some("peer1".to_string()),
        };
        assert!(!syncing.is_synced());
        assert!(syncing.is_syncing());
        assert_eq!(syncing.progress(), 0.5);
    }

    #[test]
    fn test_peer_sync_info_compatibility() {
        let our_genesis = [1u8; 32];
        let compatible_peer = PeerSyncInfo {
            height: 100,
            head_hash: [2u8; 32],
            genesis_hash: Some(our_genesis),
            is_synced: true,
            last_updated: std::time::Instant::now(),
        };
        assert!(compatible_peer.is_compatible(&our_genesis));

        let incompatible_peer = PeerSyncInfo {
            height: 100,
            head_hash: [2u8; 32],
            genesis_hash: Some([99u8; 32]),
            is_synced: true,
            last_updated: std::time::Instant::now(),
        };
        assert!(!incompatible_peer.is_compatible(&our_genesis));
    }

    #[test]
    fn test_peer_sync_info_staleness() {
        use std::time::{Duration, Instant};

        let fresh_peer = PeerSyncInfo {
            height: 100,
            head_hash: [2u8; 32],
            genesis_hash: None,
            is_synced: true,
            last_updated: Instant::now(),
        };
        assert!(!fresh_peer.is_stale());

        let stale_peer = PeerSyncInfo {
            height: 100,
            head_hash: [2u8; 32],
            genesis_hash: None,
            is_synced: true,
            last_updated: Instant::now() - Duration::from_secs(35),
        };
        assert!(stale_peer.is_stale());
    }
}
