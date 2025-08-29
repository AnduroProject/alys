//! SyncActor State Management
//! 
//! Manages synchronization state including progress tracking, block production
//! eligibility, and coordination with federation timing constraints.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime};
use ethereum_types::H256;
use crate::actors::network::messages::SyncMode;

/// Complete synchronization state
#[derive(Debug, Clone)]
pub struct SyncState {
    /// Current sync progress information
    pub progress: SyncProgress,
    /// Active sync operations
    pub active_operations: HashMap<String, SyncOperation>,
    /// Performance metrics
    pub metrics: SyncMetrics,
    /// Peer coordination state
    pub peer_state: PeerSyncState,
    /// Federation timing state
    pub federation_state: FederationSyncState,
    /// Checkpoint management state
    pub checkpoint_state: CheckpointState,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            progress: SyncProgress::default(),
            active_operations: HashMap::new(),
            metrics: SyncMetrics::default(),
            peer_state: PeerSyncState::default(),
            federation_state: FederationSyncState::default(),
            checkpoint_state: CheckpointState::default(),
        }
    }
}

/// Sync progress tracking with granular states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    /// Current blockchain height
    pub current_height: u64,
    /// Target height for synchronization
    pub target_height: Option<u64>,
    /// Sync progress as percentage (0.0 to 1.0)
    pub progress_percent: f64,
    /// Current sync status
    pub status: SyncStatus,
    /// Sync mode being used
    pub mode: SyncMode,
    /// Whether block production is allowed (99.5% threshold)
    pub can_produce_blocks: bool,
    /// Last successful block sync
    pub last_block_sync: Option<SystemTime>,
    /// Estimated time to completion
    pub eta: Option<Duration>,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            current_height: 0,
            target_height: None,
            progress_percent: 0.0,
            status: SyncStatus::Idle,
            mode: SyncMode::default(),
            can_produce_blocks: false,
            last_block_sync: None,
            eta: None,
        }
    }
}

/// Sync status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Not currently syncing
    Idle,
    /// Discovering peers for sync
    Discovery,
    /// Downloading blocks from peers
    Downloading,
    /// Validating downloaded blocks
    Validating,
    /// Applying blocks to chain state
    Applying,
    /// Sync completed successfully
    Completed,
    /// Sync failed with error
    Failed,
    /// Recovering from checkpoint
    Recovery,
    /// Emergency sync mode
    Emergency,
}

impl SyncStatus {
    /// Check if sync is actively running
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SyncStatus::Discovery
                | SyncStatus::Downloading
                | SyncStatus::Validating
                | SyncStatus::Applying
                | SyncStatus::Recovery
        )
    }

    /// Check if sync is in error state
    pub fn is_error(&self) -> bool {
        matches!(self, SyncStatus::Failed)
    }
}

/// Individual sync operation tracking
#[derive(Debug, Clone)]
pub struct SyncOperation {
    pub operation_id: String,
    pub start_height: u64,
    pub end_height: u64,
    pub mode: SyncMode,
    pub started_at: Instant,
    pub progress: f64,
    pub assigned_peers: Vec<String>,
    pub blocks_downloaded: u64,
    pub blocks_validated: u64,
    pub blocks_applied: u64,
    pub status: SyncStatus,
    pub error_count: u32,
}

/// Sync performance metrics
#[derive(Debug, Clone, Default)]
pub struct SyncMetrics {
    /// Total blocks synced in current session
    pub total_blocks_synced: u64,
    /// Current blocks per second rate
    pub current_bps: f64,
    /// Average blocks per second over session
    pub average_bps: f64,
    /// Peak blocks per second achieved
    pub peak_bps: f64,
    /// Total download bandwidth used
    pub total_bandwidth_bytes: u64,
    /// Current download rate
    pub current_download_rate: f64,
    /// Validation performance metrics
    pub validation_metrics: ValidationMetrics,
    /// Recent performance samples for averaging
    pub recent_samples: VecDeque<PerformanceSample>,
}

/// Block validation performance metrics
#[derive(Debug, Clone, Default)]
pub struct ValidationMetrics {
    /// Total blocks validated
    pub blocks_validated: u64,
    /// Average validation time per block (ms)
    pub avg_validation_time_ms: f64,
    /// SIMD acceleration usage percentage
    pub simd_usage_percent: f64,
    /// Parallel worker utilization
    pub worker_utilization: f64,
    /// Validation errors encountered
    pub validation_errors: u64,
}

/// Performance sample for metrics averaging
#[derive(Debug, Clone)]
pub struct PerformanceSample {
    pub timestamp: Instant,
    pub blocks_per_second: f64,
    pub validation_time_ms: f64,
    pub download_rate: f64,
}

/// Peer synchronization coordination
#[derive(Debug, Clone, Default)]
pub struct PeerSyncState {
    /// Peers currently being used for sync
    pub active_sync_peers: HashMap<String, PeerSyncInfo>,
    /// Peer performance rankings
    pub peer_rankings: Vec<PeerRanking>,
    /// Blacklisted peers (temporary)
    pub blacklisted_peers: HashMap<String, BlacklistInfo>,
    /// Download assignments per peer
    pub peer_assignments: HashMap<String, Vec<BlockRange>>,
}

/// Per-peer sync information
#[derive(Debug, Clone)]
pub struct PeerSyncInfo {
    pub peer_id: String,
    pub height: u64,
    pub blocks_per_second: f64,
    pub reliability_score: f64,
    pub last_activity: Instant,
    pub assigned_ranges: Vec<BlockRange>,
    pub completed_ranges: Vec<BlockRange>,
    pub error_count: u32,
}

/// Peer performance ranking for sync selection
#[derive(Debug, Clone)]
pub struct PeerRanking {
    pub peer_id: String,
    pub composite_score: f64,
    pub latency_ms: f64,
    pub throughput_score: f64,
    pub reliability_score: f64,
    pub is_federation_peer: bool,
}

/// Block range assignment
#[derive(Debug, Clone)]
pub struct BlockRange {
    pub start_height: u64,
    pub end_height: u64,
    pub assigned_at: Instant,
    pub priority: u8, // 0 = highest priority
}

/// Peer blacklist information
#[derive(Debug, Clone)]
pub struct BlacklistInfo {
    pub blacklisted_at: Instant,
    pub duration: Duration,
    pub reason: String,
    pub strike_count: u32,
}

/// Federation timing coordination state
#[derive(Debug, Clone, Default)]
pub struct FederationSyncState {
    /// Last federation block seen
    pub last_federation_block: Option<u64>,
    /// Federation block production timeline
    pub production_schedule: VecDeque<ProductionSlot>,
    /// Current slot information
    pub current_slot: Option<SlotInfo>,
    /// Timing constraint violations
    pub timing_violations: u32,
    /// Emergency mode status
    pub emergency_mode: bool,
}

/// Block production slot information
#[derive(Debug, Clone)]
pub struct ProductionSlot {
    pub slot_number: u64,
    pub expected_time: SystemTime,
    pub authority: String,
    pub status: SlotStatus,
}

/// Slot status tracking
#[derive(Debug, Clone, Copy)]
pub enum SlotStatus {
    Pending,
    Produced,
    Missed,
    Finalized,
}

/// Current slot information
#[derive(Debug, Clone)]
pub struct SlotInfo {
    pub slot_number: u64,
    pub slot_start: SystemTime,
    pub slot_duration: Duration,
    pub authority: String,
    pub can_produce: bool,
}

/// Checkpoint management state
#[derive(Debug, Clone, Default)]
pub struct CheckpointState {
    /// Available checkpoints
    pub available_checkpoints: Vec<CheckpointInfo>,
    /// Currently active checkpoint operation
    pub active_checkpoint: Option<CheckpointOperation>,
    /// Checkpoint creation schedule
    pub next_checkpoint_height: Option<u64>,
    /// Last checkpoint creation time
    pub last_checkpoint_time: Option<SystemTime>,
}

/// Checkpoint metadata
#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    pub checkpoint_id: String,
    pub height: u64,
    pub state_root: H256,
    pub created_at: SystemTime,
    pub size_bytes: u64,
    pub compressed: bool,
}

/// Active checkpoint operation
#[derive(Debug, Clone)]
pub struct CheckpointOperation {
    pub operation_id: String,
    pub operation_type: CheckpointOperationType,
    pub started_at: Instant,
    pub progress: f64,
    pub checkpoint_id: String,
}

/// Types of checkpoint operations
#[derive(Debug, Clone)]
pub enum CheckpointOperationType {
    Create,
    Restore,
    Verify,
}

impl SyncState {
    /// Update sync progress and check production eligibility
    pub fn update_progress(&mut self, current_height: u64, target_height: Option<u64>) {
        self.progress.current_height = current_height;
        self.progress.target_height = target_height;

        // Calculate progress percentage
        if let Some(target) = target_height {
            if target > 0 {
                self.progress.progress_percent = current_height as f64 / target as f64;
            }
        }

        // Check 99.5% production threshold
        self.progress.can_produce_blocks = self.progress.progress_percent >= 0.995;

        // Update last sync time
        self.progress.last_block_sync = Some(SystemTime::now());
    }

    /// Add performance sample and update metrics
    pub fn add_performance_sample(&mut self, blocks_per_second: f64, validation_time_ms: f64) {
        let sample = PerformanceSample {
            timestamp: Instant::now(),
            blocks_per_second,
            validation_time_ms,
            download_rate: 0.0, // To be updated separately
        };

        self.metrics.recent_samples.push_back(sample);

        // Keep only recent samples (last 100)
        while self.metrics.recent_samples.len() > 100 {
            self.metrics.recent_samples.pop_front();
        }

        // Update current and average metrics
        self.metrics.current_bps = blocks_per_second;
        self.metrics.peak_bps = self.metrics.peak_bps.max(blocks_per_second);

        if !self.metrics.recent_samples.is_empty() {
            self.metrics.average_bps = self.metrics.recent_samples.iter()
                .map(|s| s.blocks_per_second)
                .sum::<f64>() / self.metrics.recent_samples.len() as f64;
        }
    }

    /// Check if sync is meeting performance targets
    pub fn is_meeting_targets(&self) -> bool {
        // Target: 250+ blocks/sec for fast sync
        self.metrics.current_bps >= 250.0
    }

    /// Get sync health status
    pub fn health_status(&self) -> SyncHealthStatus {
        if self.progress.status.is_error() {
            return SyncHealthStatus::Unhealthy;
        }

        if self.is_meeting_targets() && self.progress.status.is_active() {
            SyncHealthStatus::Healthy
        } else if self.progress.status.is_active() {
            SyncHealthStatus::Degraded
        } else {
            SyncHealthStatus::Idle
        }
    }
}

/// Sync health status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Idle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_progress_calculation() {
        let mut state = SyncState::default();
        
        // Test progress calculation
        state.update_progress(950, Some(1000));
        assert_eq!(state.progress.progress_percent, 0.95);
        assert!(!state.progress.can_produce_blocks); // Below 99.5%
        
        // Test production threshold
        state.update_progress(995, Some(1000));
        assert_eq!(state.progress.progress_percent, 0.995);
        assert!(state.progress.can_produce_blocks); // At 99.5%
    }

    #[test]
    fn performance_metrics() {
        let mut state = SyncState::default();
        
        // Add performance samples
        state.add_performance_sample(200.0, 10.0);
        state.add_performance_sample(300.0, 8.0);
        state.add_performance_sample(250.0, 12.0);
        
        assert_eq!(state.metrics.current_bps, 250.0);
        assert_eq!(state.metrics.peak_bps, 300.0);
        assert!((state.metrics.average_bps - 250.0).abs() < 0.1);
    }

    #[test]
    fn sync_status_checks() {
        assert!(SyncStatus::Downloading.is_active());
        assert!(SyncStatus::Validating.is_active());
        assert!(!SyncStatus::Idle.is_active());
        assert!(!SyncStatus::Completed.is_active());
        
        assert!(SyncStatus::Failed.is_error());
        assert!(!SyncStatus::Completed.is_error());
    }

    #[test]
    fn health_status() {
        let mut state = SyncState::default();
        
        // Idle state
        assert_eq!(state.health_status(), SyncHealthStatus::Idle);
        
        // Active but slow
        state.progress.status = SyncStatus::Downloading;
        state.metrics.current_bps = 100.0; // Below target
        assert_eq!(state.health_status(), SyncHealthStatus::Degraded);
        
        // Active and fast
        state.metrics.current_bps = 300.0; // Above target
        assert_eq!(state.health_status(), SyncHealthStatus::Healthy);
        
        // Error state
        state.progress.status = SyncStatus::Failed;
        assert_eq!(state.health_status(), SyncHealthStatus::Unhealthy);
    }
}