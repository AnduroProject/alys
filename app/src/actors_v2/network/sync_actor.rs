//! SyncActor V2 Implementation (Production-Ready)
//!
//! Blockchain synchronization actor with simplified logic.
//! Extracted from V1 SyncActor (13,333 lines -> ~2,000-3,000 lines).
//!
//! Removed: Complex state machines, actor_system dependencies, supervision
//! Simplified: Linear sync states, direct NetworkActor coordination

use actix::prelude::*;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant, SystemTime};

use super::{
    messages::{Block, NetworkMessage, PeerId, SyncStatus},
    metrics::update_prometheus_sync_state,
    tendermint_sync::{TendermintSyncConfig, TendermintSyncValidator},
    SyncConfig, SyncError, SyncMessage, SyncMetrics, SyncResponse,
};
use crate::actors_v2::storage::{StorageActor, messages::GetChainHeadMessage};

/// Simplified sync states (linear progression)
#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    Stopped,
    Starting,
    DiscoveringPeers,
    /// Querying connected peers for their chain height before deciding sync strategy.
    /// This state ensures we don't prematurely conclude "already synced" before
    /// actually discovering what height the network is at.
    QueryingNetworkHeight,
    RequestingBlocks,
    ProcessingBlocks,
    Synced,
    Error(String),
}

/// Block request tracking
#[derive(Debug, Clone)]
struct BlockRequestInfo {
    request_id: String,
    start_height: u64,
    count: u32,
    peer_id: PeerId,
    requested_at: SystemTime,
}

/// Timestamped peer height observation for freshness tracking
/// Used by Active Network Height Monitoring to filter stale data
#[derive(Debug, Clone)]
pub struct PeerHeightObservation {
    pub peer_id: String,
    pub height: u64,
    pub observed_at: Instant,
}

/// Mutable state extracted for Arc<RwLock<T>> wrapping
///
/// This struct contains all mutable state that needs to be shared between
/// synchronous message handlers and asynchronous workflow methods.
///
/// Refactor Context: Phase 1, Task 1.1 - Arc<RwLock<T>> Pattern
/// See: SYNCACTOR_ARC_REFACTOR_PLAN.md
struct SyncActorState {
    /// Current sync state
    sync_state: SyncState,
    /// Current blockchain height
    current_height: u64,
    /// Target height to sync to
    target_height: u64,
    /// Sync metrics
    metrics: SyncMetrics,
    /// Block processing queue
    block_queue: VecDeque<(Block, PeerId)>,
    /// Active block requests
    active_requests: HashMap<String, BlockRequestInfo>,
    /// Available sync peers
    sync_peers: Vec<PeerId>,
    /// Peer selection index (round-robin)
    peer_selection_index: usize,
    /// Running state
    is_running: bool,
    /// Shutdown flag
    shutdown_requested: bool,
    /// Timestamp when current sync_state was entered (for bootstrap detection)
    state_entered_at: SystemTime,
    /// Total time spent in DiscoveringPeers state (accumulated across attempts)
    discovery_time_accumulated: Duration,
    /// Collected peer heights during QueryingNetworkHeight state
    /// Used to calculate mode (consensus) height from peer responses
    observed_peer_heights: Vec<u64>,

    // Network height monitoring state (Active Height Monitoring feature)
    /// Timestamped peer height observations for freshness filtering
    peer_height_observations: Vec<PeerHeightObservation>,
    /// Last sync completion time (for cooldown enforcement)
    last_sync_completed_at: Option<Instant>,
    /// Consecutive checks showing node is behind (for hysteresis)
    consecutive_behind_checks: u32,
    /// Consecutive queries with no peer responses (stale detection)
    consecutive_no_response_queries: u32,
}

impl SyncActorState {
    /// Create new state with default values
    fn new() -> Self {
        Self {
            sync_state: SyncState::Stopped,
            current_height: 0,
            target_height: 0,
            metrics: SyncMetrics::new(),
            block_queue: VecDeque::new(),
            active_requests: HashMap::new(),
            sync_peers: Vec::new(),
            peer_selection_index: 0,
            is_running: false,
            shutdown_requested: false,
            state_entered_at: SystemTime::now(),
            discovery_time_accumulated: Duration::ZERO,
            observed_peer_heights: Vec::new(),

            // Network height monitoring initialization
            peer_height_observations: Vec::new(),
            last_sync_completed_at: None,
            consecutive_behind_checks: 0,
            consecutive_no_response_queries: 0,
        }
    }

    /// Bootstrap detection timeout (30 seconds for regtest)
    const BOOTSTRAP_DETECTION_TIMEOUT: Duration = Duration::from_secs(30);

    /// Get current sync status (no async needed)
    fn get_sync_status(&self) -> SyncStatus {
        let is_syncing = self.determine_sync_state();

        SyncStatus {
            current_height: self.current_height,
            target_height: self.target_height,
            is_syncing,
            sync_peers: self.sync_peers.clone(),
            pending_requests: self.active_requests.len(),
        }
    }

    /// Determine if we're in active sync or bootstrap mode
    fn determine_sync_state(&self) -> bool {
        const SYNC_THRESHOLD: u64 = 2;

        // Case 1: Target height known → Simple comparison
        if self.target_height > 0 {
            return self.current_height + SYNC_THRESHOLD < self.target_height;
        }

        // Case 2: Target unknown → State-based logic with bootstrap detection
        match self.sync_state {
            SyncState::Stopped | SyncState::Synced => false,
            SyncState::DiscoveringPeers => {
                // Check if we've timed out (bootstrap mode)
                let total_discovery_time = self.discovery_time_accumulated
                    + self.state_entered_at.elapsed().unwrap_or(Duration::ZERO);

                if total_discovery_time > Self::BOOTSTRAP_DETECTION_TIMEOUT {
                    // Genesis node with no peers: Allow block production
                    if self.current_height == 0 && self.sync_peers.is_empty() {
                        tracing::info!(
                            discovery_time_secs = total_discovery_time.as_secs(),
                            "Bootstrap timeout reached (genesis, no peers) - allowing block production"
                        );
                        false
                    } else {
                        // Non-genesis or has peers: Continue sync attempts
                        tracing::warn!(
                            current_height = self.current_height,
                            peer_count = self.sync_peers.len(),
                            discovery_time_secs = total_discovery_time.as_secs(),
                            "Timeout in peer discovery - continuing sync"
                        );
                        true
                    }
                } else {
                    // Still discovering, block production should wait
                    true
                }
            }
            SyncState::Starting
            | SyncState::QueryingNetworkHeight
            | SyncState::RequestingBlocks
            | SyncState::ProcessingBlocks
            | SyncState::Error(_) => true,
        }
    }

    /// Select next peer (round-robin)
    fn select_sync_peer(&mut self) -> PeerId {
        if self.sync_peers.is_empty() {
            return "no_peers".to_string();
        }

        let peer = self.sync_peers[self.peer_selection_index].clone();
        self.peer_selection_index =
            (self.peer_selection_index + 1) % self.sync_peers.len();

        peer
    }

    /// Handle request timeouts
    fn handle_timeouts(&mut self, timeout: Duration) {
        let mut timed_out_requests = Vec::new();
        let now = SystemTime::now();

        for (request_id, request_info) in &self.active_requests {
            if let Ok(elapsed) = now.duration_since(request_info.requested_at) {
                if elapsed > timeout {
                    timed_out_requests.push(request_id.clone());
                }
            }
        }

        for request_id in timed_out_requests {
            if let Some(request_info) = self.active_requests.remove(&request_id) {
                tracing::warn!(
                    request_id = %request_id,
                    peer_id = %request_info.peer_id,
                    elapsed_secs = ?now.duration_since(request_info.requested_at),
                    "Block request timed out"
                );

                self.metrics.record_network_error();
            }
        }
    }

    /// Transition to new state with timestamp tracking
    fn transition_to_state(&mut self, new_state: SyncState) {
        // Calculate time spent in previous state
        let time_in_previous_state = self.state_entered_at.elapsed().unwrap_or(Duration::ZERO);

        // Accumulate discovery time before transitioning out of DiscoveringPeers
        if self.sync_state == SyncState::DiscoveringPeers {
            self.discovery_time_accumulated += time_in_previous_state;

            tracing::debug!(
                discovery_time_secs = self.discovery_time_accumulated.as_secs(),
                "Accumulated discovery time"
            );
        }

        // Reset accumulated time when entering DiscoveringPeers from a different state
        if new_state == SyncState::DiscoveringPeers
            && self.sync_state != SyncState::DiscoveringPeers
        {
            self.discovery_time_accumulated = Duration::ZERO;
            tracing::debug!("Reset discovery time for new discovery cycle");
        }

        // Transition to new state
        let old_state = std::mem::replace(&mut self.sync_state, new_state.clone());
        self.state_entered_at = SystemTime::now();

        // Enhanced logging with full sync context
        tracing::info!(
            "╔══════════════════════════════════════════════════════════════════╗"
        );
        tracing::info!(
            "║ SYNC STATE TRANSITION: {:?} → {:?}",
            old_state,
            self.sync_state
        );
        tracing::info!(
            "║ Current Height: {} | Target Height: {} | Peers: {} | Active Requests: {}",
            self.current_height,
            self.target_height,
            self.sync_peers.len(),
            self.active_requests.len()
        );
        tracing::info!(
            "║ Time in previous state: {:.2}s | Block queue: {}",
            time_in_previous_state.as_secs_f64(),
            self.block_queue.len()
        );
        tracing::info!(
            "╚══════════════════════════════════════════════════════════════════╝"
        );

        // Update Prometheus metrics for state transition
        update_prometheus_sync_state(&self.sync_state);
    }
}

/// Simplified sync actor - blockchain sync only (refactored with Arc<RwLock<State>>)
///
/// Refactor Context: Phase 1, Task 1.2 - Actor struct with shared state
/// See: SYNCACTOR_ARC_REFACTOR_PLAN.md
pub struct SyncActor {
    /// Shared mutable state (wrapped for sync access - Actix actors are single-threaded)
    state: std::sync::Arc<std::sync::RwLock<SyncActorState>>,

    /// Immutable configuration (no lock needed)
    config: SyncConfig,

    /// Actor addresses for coordination (set once, never mutated directly)
    network_actor: Option<Addr<crate::actors_v2::network::NetworkActor>>,
    chain_actor: Option<Addr<crate::actors_v2::chain::ChainActor>>,
    storage_actor: Option<Addr<StorageActor>>,

    /// Tendermint sync validator for commit verification (Phase 3)
    tendermint_validator: Option<std::sync::Arc<std::sync::RwLock<TendermintSyncValidator>>>,
}

impl SyncActor {
    /// Create new SyncActor with Arc<RwLock<State>> pattern
    ///
    /// Refactor Context: Phase 1, Task 1.3 - Updated constructor
    pub fn new(config: SyncConfig) -> Result<Self> {
        tracing::info!("Creating SyncActor V2 with Arc<RwLock<State>> pattern");

        config
            .validate()
            .map_err(|e| anyhow!("Invalid sync configuration: {}", e))?;

        // Initialize Tendermint validator if enabled
        // Uses deferred initialization - validator set loaded from storage later
        let tendermint_validator = if config.tendermint_enabled {
            let tm_config = TendermintSyncConfig {
                verify_commits: config.verify_commits,
                max_batch_size: config.max_blocks_per_request,
                allow_untrusted_sync: false,
                chain_id: config.chain_id.to_string(), // Issue 1.2: domain separation
            };
            tracing::info!("Tendermint sync validation enabled (verify_commits={})", config.verify_commits);
            Some(std::sync::Arc::new(std::sync::RwLock::new(
                TendermintSyncValidator::new_deferred(tm_config)
            )))
        } else {
            tracing::info!("Tendermint sync validation disabled (legacy mode)");
            None
        };

        Ok(Self {
            state: std::sync::Arc::new(std::sync::RwLock::new(SyncActorState::new())),
            config,
            network_actor: None,
            chain_actor: None,
            storage_actor: None,
            tendermint_validator,
        })
    }

    /// Get the TendermintSyncValidator reference for sharing with ChainActor.
    ///
    /// Issue 4.2 Step 4.2.6: This allows ChainActor to receive governance notifications
    /// and forward them to the sync validator, ensuring validator set tracking stays
    /// synchronized between consensus and sync validation.
    pub fn tendermint_validator(&self) -> Option<std::sync::Arc<std::sync::RwLock<TendermintSyncValidator>>> {
        self.tendermint_validator.clone()
    }

    /// Calculate the mode (most common value) from a list of heights
    /// Returns the highest value if there are ties (conservative approach)
    /// Returns 0 if the list is empty
    fn calculate_mode(heights: &[u64]) -> u64 {
        if heights.is_empty() {
            return 0;
        }

        // Count occurrences of each height
        let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for &height in heights {
            *counts.entry(height).or_insert(0) += 1;
        }

        // Find the maximum count
        let max_count = counts.values().max().copied().unwrap_or(0);

        // Among heights with the max count, pick the highest (conservative)
        // This handles ties by choosing the higher height
        counts
            .into_iter()
            .filter(|(_, count)| *count == max_count)
            .map(|(height, _)| height)
            .max()
            .unwrap_or(0)
    }

    /// Calculate median height from peer observations (robust to outliers)
    /// Used by Active Network Height Monitoring for Byzantine-resistant consensus
    ///
    /// Returns None if:
    /// - Insufficient fresh observations (< min_quorum)
    /// - All observations are stale (older than max_age)
    ///
    /// The median is preferred over mode/max because:
    /// - Single malicious peer cannot skew result (unlike max)
    /// - More robust with varied peer heights (unlike mode which needs agreement)
    fn calculate_median_height(
        observations: &[PeerHeightObservation],
        max_age: Duration,
        min_quorum: usize,
    ) -> Option<u64> {
        let now = Instant::now();

        // Filter to fresh observations only
        let fresh_heights: Vec<u64> = observations
            .iter()
            .filter(|obs| now.duration_since(obs.observed_at) < max_age)
            .map(|obs| obs.height)
            .collect();

        // Require minimum quorum for Byzantine resistance
        if fresh_heights.len() < min_quorum {
            tracing::trace!(
                total_observations = observations.len(),
                fresh_observations = fresh_heights.len(),
                min_quorum = min_quorum,
                "Insufficient fresh peer heights for median calculation"
            );
            return None;
        }

        // Calculate median
        let mut sorted = fresh_heights;
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];

        tracing::trace!(
            observation_count = sorted.len(),
            median_height = median,
            min_height = sorted.first().copied().unwrap_or(0),
            max_height = sorted.last().copied().unwrap_or(0),
            "Calculated median network height"
        );

        Some(median)
    }
}

// Phase 4: All unused workflow methods deleted (947 lines removed)
// These 18 methods were never called in production code - all functionality
// reimplemented inline in Handler<SyncMessage> using ctx.spawn() pattern.
// See SYNCACTOR_FUNCTIONAL_VERIFICATION.md for detailed analysis.
// Git history preserves the original implementations if needed for reference.

impl Actor for SyncActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("SyncActor V2 started (Arc<RwLock> pattern)");

        // Initialize current_height from StorageActor on startup
        // This replaces the old checkpoint system - we just query the authoritative source
        let state = std::sync::Arc::clone(&self.state);
        let storage_actor = self.storage_actor.clone();
        tokio::spawn(async move {
            let storage_height = if let Some(storage) = storage_actor.as_ref() {
                match storage.send(GetChainHeadMessage { correlation_id: None }).await {
                    Ok(Ok(Some(head))) => {
                        tracing::info!(
                            storage_height = head.number,
                            "Initialized SyncActor from StorageActor chain head"
                        );
                        head.number
                    }
                    Ok(Ok(None)) => {
                        tracing::debug!("No chain head in storage - starting fresh");
                        0
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "Failed to query StorageActor - starting fresh");
                        0
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "StorageActor mailbox error - starting fresh");
                        0
                    }
                }
            } else {
                tracing::debug!("No StorageActor configured - starting fresh");
                0
            };

            let mut s = state.write().unwrap();
            s.current_height = storage_height;
            s.target_height = 0; // Will be discovered via peer queries
            s.transition_to_state(SyncState::Stopped);
            s.is_running = false;
        });

        // Periodic timeout checking
        ctx.run_interval(Duration::from_secs(10), |act, _ctx| {
            let state = std::sync::Arc::clone(&act.state);
            let timeout = act.config.sync_timeout;

            tokio::spawn(async move {
                let mut s = state.write().unwrap();
                s.handle_timeouts(timeout);
            });
        });

        // Periodic sync progress updates
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            let state = std::sync::Arc::clone(&act.state);

            tokio::spawn(async move {
                let s = state.read().unwrap();
                if s.is_running {
                    let progress = s.metrics.get_sync_progress();
                    tracing::debug!(
                        "Sync progress: {:.1}% ({}/{})",
                        progress * 100.0,
                        s.current_height,
                        s.target_height
                    );
                }
            });
        });

        // Network height query: Poll for network height when in QueryingNetworkHeight state
        // This handler does TWO things:
        // 1. Queries NetworkActor to send GetChainStatus to peers (actual peer height discovery)
        // 2. Falls back to checking local ChainActor (for gossipsub-delivered blocks)
        ctx.run_interval(Duration::from_secs(2), |act, _ctx| {
            let state = std::sync::Arc::clone(&act.state);
            let chain_actor = act.chain_actor.clone();
            let network_actor = act.network_actor.clone();

            tokio::spawn(async move {
                // Check if we're in QueryingNetworkHeight state
                let (should_query, current_height, time_in_state) = {
                    let s = state.read().unwrap();
                    let in_querying_state = s.is_running
                        && s.sync_state == SyncState::QueryingNetworkHeight;
                    let elapsed = s.state_entered_at.elapsed().unwrap_or(Duration::ZERO);
                    (in_querying_state, s.current_height, elapsed)
                };

                if !should_query {
                    return;
                }

                // PRIMARY: Query NetworkActor to send GetChainStatus requests to all peers
                // The responses will come back via ReportPeerHeights message
                if let Some(network_actor) = network_actor {
                    tracing::debug!("Sending QueryPeerHeights to NetworkActor for peer height discovery");
                    if let Err(e) = network_actor
                        .send(crate::actors_v2::network::NetworkMessage::QueryPeerHeights)
                        .await
                    {
                        tracing::warn!(
                            error = ?e,
                            "Failed to send QueryPeerHeights to NetworkActor"
                        );
                    }
                    // Note: The actual height response comes via ReportPeerHeights message
                    // which is handled separately and will trigger state transition
                }

                // FALLBACK: Also check local ChainActor for blocks received via gossipsub
                // This catches blocks that arrived and were successfully imported,
                // as well as blocks that were received but cached as orphans
                if let Some(chain_actor) = chain_actor {
                    match chain_actor
                        .send(crate::actors_v2::chain::messages::ChainMessage::GetChainStatus)
                        .await
                    {
                        Ok(Ok(crate::actors_v2::chain::messages::ChainResponse::ChainStatus(status))) => {
                            // Use the higher of: imported height OR observed height (from orphan cache)
                            // This catches blocks that arrived via gossipsub but couldn't be imported
                            // because their parents were missing (they're cached as orphans)
                            let imported_height = status.height;
                            let observed_height = status.observed_height;
                            let network_height = std::cmp::max(imported_height, observed_height);

                            // If we know of higher blocks (imported or observed), we know network height
                            if network_height > current_height {
                                let mut s = state.write().unwrap();
                                s.target_height = network_height;
                                tracing::info!(
                                    current_height = s.current_height,
                                    imported_height = imported_height,
                                    observed_height = observed_height,
                                    target_height = network_height,
                                    orphan_count = status.orphan_count,
                                    "Network height discovered via local chain status (includes orphan blocks) - transitioning to RequestingBlocks"
                                );
                                s.transition_to_state(SyncState::RequestingBlocks);
                                return;
                            }
                        }
                        _ => {
                            // Error or unexpected response - log and continue
                            tracing::debug!("Failed to query ChainActor for chain status during height discovery");
                        }
                    }
                }

                // Timeout after 10 seconds of querying: If no higher chain discovered, we're synced
                // This handles the case where we're starting a fresh network or are the first node
                const NETWORK_HEIGHT_QUERY_TIMEOUT: Duration = Duration::from_secs(10);

                if time_in_state > NETWORK_HEIGHT_QUERY_TIMEOUT {
                    let mut s = state.write().unwrap();
                    let height = s.current_height;
                    let observations = s.observed_peer_heights.len();

                    tracing::info!(
                        current_height = height,
                        query_duration_secs = time_in_state.as_secs(),
                        peer_responses = observations,
                        "Network height query timeout - no higher chain discovered, completing sync"
                    );

                    // Clear collected peer heights
                    s.observed_peer_heights.clear();

                    s.transition_to_state(SyncState::Synced);
                    s.is_running = false;
                    s.last_sync_completed_at = Some(Instant::now());
                    s.metrics.record_sync_complete(height);
                }
            });
        });

        // Sync loop: Periodic block requesting during active sync
        ctx.run_interval(Duration::from_secs(2), |act, ctx| {
            let state = std::sync::Arc::clone(&act.state);
            let addr = ctx.address();
            let max_concurrent = act.config.max_concurrent_requests;
            let max_per_request = act.config.max_blocks_per_request;

            tokio::spawn(async move {
                // Check if we should request blocks
                let should_request = {
                    let s = state.read().unwrap();

                    // Only request if:
                    // 1. Sync is running
                    // 2. We're in RequestingBlocks state
                    // 3. We have peers
                    // 4. We're behind target
                    // 5. We're under max concurrent requests
                    s.is_running
                        && s.sync_state == SyncState::RequestingBlocks
                        && !s.sync_peers.is_empty()
                        && s.current_height < s.target_height
                        && s.active_requests.len() < max_concurrent
                };

                if !should_request {
                    return;
                }

                // Calculate request parameters
                let (start_height, count) = {
                    let s = state.read().unwrap();
                    let start_height = s.current_height + 1;
                    let remaining = s.target_height.saturating_sub(s.current_height);
                    let count = remaining.min(max_per_request as u64) as u32;
                    (start_height, count)
                };

                if count == 0 {
                    return;
                }

                tracing::debug!(
                    start_height = start_height,
                    count = count,
                    "Sync loop triggering block request"
                );

                if let Err(e) = addr
                    .send(SyncMessage::RequestBlocks {
                        start_height,
                        count,
                        peer_id: None, // Auto-select peer via round-robin
                    })
                    .await
                {
                    tracing::error!(
                        error = %e,
                        "Sync loop failed to send RequestBlocks"
                    );
                }
            });
        });

        // Sync completion detection: Check if we've reached target
        ctx.run_interval(Duration::from_secs(5), |act, _ctx| {
            let state = std::sync::Arc::clone(&act.state);

            tokio::spawn(async move {
                let mut s = state.write().unwrap();

                // Check if sync is complete
                const SYNC_THRESHOLD: u64 = 2;

                // Sync is complete when:
                // 1. Sync is running and in active sync state (RequestingBlocks or ProcessingBlocks)
                // 2. No pending requests or blocks
                // 3. We have a known target (target_height > 0) AND we've reached it
                // NOTE: target_height == 0 case is now handled by QueryingNetworkHeight state
                let in_sync_state = s.is_running
                    && (s.sync_state == SyncState::RequestingBlocks
                        || s.sync_state == SyncState::ProcessingBlocks);

                let no_pending_work = s.active_requests.is_empty() && s.block_queue.is_empty();

                // Only complete when we have a known target and reached it
                // (target_height == 0 means height not yet discovered - handled by QueryingNetworkHeight)
                let reached_target = s.target_height > 0
                    && s.current_height + SYNC_THRESHOLD >= s.target_height;

                let is_complete = in_sync_state && no_pending_work && reached_target;

                if is_complete {
                    let current = s.current_height;
                    let target = s.target_height;

                    tracing::info!("┌─────────────────────────────────────────────────────────────────┐");
                    tracing::info!("│ ✅ SYNC LIFECYCLE: Sync Complete!                              │");
                    tracing::info!("│ Final Height: {} | Target Height: {} | Synced!", current, target);
                    tracing::info!("└─────────────────────────────────────────────────────────────────┘");

                    s.transition_to_state(SyncState::Synced);
                    s.is_running = false;
                    s.last_sync_completed_at = Some(Instant::now());
                    s.metrics.record_sync_complete(current);
                }
            });
        });

        // ========================================================================
        // ACTIVE NETWORK HEIGHT MONITORING (Layer 1)
        // ========================================================================
        // This interval runs ALWAYS (even when synced) to keep target_height fresh.
        // Unlike other intervals that only run during active sync, this monitors
        // for the node falling behind the network after sync completes.
        let poll_interval_secs = self.config.peer_height_poll_interval_secs;
        let poll_interval = Duration::from_secs(poll_interval_secs);

        ctx.run_interval(poll_interval, |act, _ctx| {
            let state = std::sync::Arc::clone(&act.state);
            let network_actor = act.network_actor.clone();

            tokio::spawn(async move {
                // Only poll when synced or stopped (not during active sync to avoid interference)
                let should_poll = {
                    let s = state.read().unwrap();
                    matches!(s.sync_state, SyncState::Synced | SyncState::Stopped)
                        && !s.sync_peers.is_empty()
                };

                if !should_poll {
                    return;
                }

                // Query peers for their current height via NetworkActor
                if let Some(network) = network_actor {
                    tracing::trace!("Active height monitoring: querying peer heights");
                    if let Err(e) = network.send(NetworkMessage::QueryPeerHeights).await {
                        tracing::debug!(
                            error = %e,
                            "Failed to query peer heights during active monitoring"
                        );
                    }
                    // Responses arrive via ReportPeerHeights message
                }
            });
        });

        tracing::info!(
            poll_interval_secs = poll_interval_secs,
            "Active network height monitoring started (runs when synced)"
        );
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("SyncActor V2 stopping");

        // Update state synchronously using std::sync::RwLock
        if let Ok(mut s) = self.state.write() {
            s.shutdown_requested = true;
            s.sync_state = SyncState::Stopped;
            s.is_running = false;
        }

        Running::Stop
    }
}

impl Handler<SyncMessage> for SyncActor {
    type Result = Result<SyncResponse, SyncError>;

    fn handle(&mut self, msg: SyncMessage, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            SyncMessage::StartSync {
                start_height,
                target_height,
            } => {
                // Phase 2: Refactored to spawn async workflow via ctx.spawn()
                // This is THE critical fix - StartSync now triggers actual sync workflow

                tracing::info!("┌─────────────────────────────────────────────────────────────────┐");
                tracing::info!("│ 🔄 SYNC LIFECYCLE: StartSync received                          │");
                tracing::info!("│ Start Height: {} | Target Height: {:?}", start_height, target_height);
                tracing::info!("└─────────────────────────────────────────────────────────────────┘");

                // Clone Arc for workflow execution
                let state = std::sync::Arc::clone(&self.state);
                let network_actor = self.network_actor.clone();
                let chain_actor = self.chain_actor.clone();

                // Schedule async workflow (non-blocking)
                ctx.spawn(
                    async move {
                        // Acquire write lock to validate and update state
                        let mut s = state.write().unwrap();

                        // Validate state
                        if s.sync_state != SyncState::Stopped && s.sync_state != SyncState::Synced {
                            tracing::warn!(
                                state = ?s.sync_state,
                                "Sync already running, ignoring StartSync"
                            );
                            return;
                        }

                        // Update state
                        s.current_height = start_height;
                        s.target_height = target_height.unwrap_or(0);
                        s.is_running = true;
                        s.transition_to_state(SyncState::Starting);

                        tracing::info!(
                            current_height = s.current_height,
                            target_height = s.target_height,
                            "Starting blockchain synchronization"
                        );

                        // Check if already synced
                        const SYNC_THRESHOLD: u64 = 2;
                        if s.target_height > 0
                            && s.current_height + SYNC_THRESHOLD >= s.target_height
                        {
                            tracing::info!(
                                current_height = s.current_height,
                                target_height = s.target_height,
                                "Already synced (within threshold)"
                            );
                            s.transition_to_state(SyncState::Synced);
                            s.is_running = false;
                            s.last_sync_completed_at = Some(Instant::now());
                            return;
                        }

                        // Mark as discovering if we need to find target
                        if s.target_height == 0 {
                            s.transition_to_state(SyncState::DiscoveringPeers);
                        }

                        // Release lock before querying NetworkActor
                        drop(s);

                        // Query NetworkActor for connected peers
                        if let Some(network_actor) = network_actor {
                            let state_clone = state.clone();

                            tokio::spawn(async move {
                                match network_actor
                                    .send(crate::actors_v2::network::NetworkMessage::GetConnectedPeers)
                                    .await
                                {
                                    Ok(Ok(crate::actors_v2::network::NetworkResponse::Peers(peer_list))) => {
                                        let mut s = state_clone.write().unwrap();
                                        s.sync_peers = peer_list.into_iter()
                                            .map(|p| p.peer_id)
                                            .collect();
                                        s.peer_selection_index = 0;

                                        tracing::info!(
                                            peer_count = s.sync_peers.len(),
                                            "Retrieved peers from NetworkActor"
                                        );

                                        // Transition based on peer availability and sync status
                                        if !s.sync_peers.is_empty() {
                                            const SYNC_THRESHOLD: u64 = 2;

                                            if s.target_height == 0 {
                                                // target_height=0 means we haven't queried the network yet
                                                // Transition to QueryingNetworkHeight to actually discover network height
                                                tracing::info!(
                                                    current_height = s.current_height,
                                                    peer_count = s.sync_peers.len(),
                                                    "Peers found - querying network height before deciding sync strategy"
                                                );
                                                s.transition_to_state(SyncState::QueryingNetworkHeight);
                                            } else if s.current_height + SYNC_THRESHOLD >= s.target_height {
                                                // We have a known target and we're already within threshold
                                                let height = s.current_height;
                                                tracing::info!(
                                                    current_height = height,
                                                    target_height = s.target_height,
                                                    "Already synced (within threshold of known target)"
                                                );
                                                s.transition_to_state(SyncState::Synced);
                                                s.is_running = false;
                                                s.last_sync_completed_at = Some(Instant::now());
                                                s.metrics.record_sync_complete(height);
                                            } else {
                                                // We have a known target and we're behind it
                                                s.transition_to_state(SyncState::RequestingBlocks);
                                                tracing::info!(
                                                    current_height = s.current_height,
                                                    target_height = s.target_height,
                                                    "Peers available, behind target - transitioning to RequestingBlocks"
                                                );
                                            }
                                        } else {
                                            tracing::info!("No peers yet - staying in DiscoveringPeers");
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        tracing::error!(
                                            error = ?e,
                                            "Failed to get connected peers from NetworkActor"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            error = ?e,
                                            "Failed to communicate with NetworkActor"
                                        );
                                    }
                                    _ => {
                                        tracing::warn!("Unexpected response from NetworkActor.GetConnectedPeers");
                                    }
                                }
                            });
                        } else {
                            tracing::warn!("NetworkActor not set - cannot discover peers");
                        }
                    }
                    .into_actor(self)
                );

                // Return immediately (non-blocking response)
                Ok(SyncResponse::Started)
            }

            SyncMessage::StopSync => {
                tracing::info!("┌─────────────────────────────────────────────────────────────────┐");
                tracing::info!("│ 🛑 SYNC LIFECYCLE: StopSync received                           │");
                tracing::info!("└─────────────────────────────────────────────────────────────────┘");

                let state = std::sync::Arc::clone(&self.state);

                ctx.spawn(
                    async move {
                        let mut s = state.write().unwrap();
                        let previous_state = s.sync_state.clone();
                        let final_height = s.current_height;
                        let target = s.target_height;

                        s.sync_state = SyncState::Stopped;
                        s.metrics.stop_sync();
                        s.is_running = false;

                        tracing::info!(
                            "│ Sync stopped - Previous state: {:?} | Final height: {} | Target was: {}",
                            previous_state,
                            final_height,
                            target
                        );
                    }
                    .into_actor(self),
                );

                Ok(SyncResponse::Stopped)
            }

            SyncMessage::GetSyncStatus => {
                // Read-only access using std::sync::RwLock
                let status = self
                    .state
                    .read()
                    .map(|s| s.get_sync_status())
                    .map_err(|_| SyncError::Internal("Failed to acquire read lock".to_string()))?;

                Ok(SyncResponse::Status(status))
            }

            SyncMessage::RequestBlocks {
                start_height,
                count,
                peer_id,
            } => {
                tracing::info!(
                    "📥 SYNC: RequestBlocks - start_height={} count={} peer={:?}",
                    start_height,
                    count,
                    peer_id
                );

                let state = std::sync::Arc::clone(&self.state);
                let network_actor = self.network_actor.clone();

                // Use std::sync::RwLock for synchronous access
                let (is_running, target_peer, request_uuid, request_id) = {
                    let mut s = self
                        .state
                        .write()
                        .map_err(|_| SyncError::Internal("Failed to acquire write lock".to_string()))?;

                    if !s.is_running {
                        (false, String::new(), uuid::Uuid::nil(), String::new())
                    } else {
                        let target_peer = peer_id.unwrap_or_else(|| s.select_sync_peer());
                        // CRITICAL: Create both UUID and String versions for correlation
                        let request_uuid = uuid::Uuid::new_v4();
                        let request_id = request_uuid.to_string();

                        let request_info = BlockRequestInfo {
                            request_id: request_id.clone(),
                            start_height,
                            count,
                            peer_id: target_peer.clone(),
                            requested_at: SystemTime::now(),
                        };

                        s.active_requests.insert(request_id.clone(), request_info);
                        s.metrics.record_block_request(&target_peer);

                        (true, target_peer, request_uuid, request_id)
                    }
                };

                if !is_running {
                    return Err(SyncError::NotStarted);
                }

                tracing::info!(
                    request_id = %request_id,
                    peer_id = %target_peer,
                    start_height = start_height,
                    count = count,
                    "Sending block request to NetworkActor"
                );

                // CORRECTED: Actually call NetworkActor to fetch blocks
                // CRITICAL: Pass request_uuid via correlation_id so IDs match
                if let Some(network_actor) = network_actor {
                    let request_id_clone = request_id.clone();
                    let state_clone = std::sync::Arc::clone(&state);

                    tokio::spawn(async move {
                        match network_actor
                            .send(crate::actors_v2::network::NetworkMessage::RequestBlocks {
                                start_height,
                                count,
                                correlation_id: Some(request_uuid),  // ✅ CRITICAL FIX: Pass our UUID
                            })
                            .await
                        {
                            Ok(Ok(_response)) => {
                                tracing::info!(
                                    request_id = %request_id_clone,
                                    "NetworkActor accepted block request"
                                );
                                // NetworkActor will use our correlation_id when forwarding blocks
                                // So HandleBlockResponse will receive matching request_id
                            }
                            Ok(Err(e)) => {
                                tracing::error!(
                                    request_id = %request_id_clone,
                                    error = ?e,
                                    "NetworkActor rejected block request"
                                );

                                // Remove failed request from active_requests
                                let mut s = state_clone.write().unwrap();
                                s.active_requests.remove(&request_id_clone);
                                s.metrics.record_network_error();
                            }
                            Err(e) => {
                                tracing::error!(
                                    request_id = %request_id_clone,
                                    error = ?e,
                                    "Failed to communicate with NetworkActor"
                                );

                                // Remove failed request from active_requests
                                let mut s = state_clone.write().unwrap();
                                s.active_requests.remove(&request_id_clone);
                                s.metrics.record_network_error();
                            }
                        }
                    });
                } else {
                    tracing::error!("NetworkActor not set - cannot request blocks");
                    return Err(SyncError::NotStarted);
                }

                Ok(SyncResponse::BlocksRequested { request_id })
            }

            SyncMessage::HandleNewBlock { block, peer_id } => {
                let state = std::sync::Arc::clone(&self.state);
                let chain_actor = self.chain_actor.clone();

                ctx.spawn(
                    async move {
                        // Queue block
                        {
                            let mut s = state.write().unwrap();
                            s.block_queue.push_back((block, peer_id.clone()));

                            tracing::debug!(
                                "Queued new block from peer {} (queue size: {})",
                                peer_id,
                                s.block_queue.len()
                            );
                        }

                        // Process the queued block immediately if we have ChainActor
                        if let Some(chain_actor) = chain_actor {
                            // Get the block we just queued
                            let block_to_process = {
                                let mut s = state.write().unwrap();
                                s.block_queue.pop_front()
                            };

                            if let Some((block_bytes, peer_id)) = block_to_process {
                                // Deserialize block from MessagePack format
                                match crate::actors_v2::common::serialization::deserialize_block_from_network(&block_bytes) {
                                    Ok(block) => {
                                        tracing::debug!(
                                            height = block.message.execution_payload.block_number,
                                            peer = peer_id,
                                            "Processing new block"
                                        );

                                        if let Err(e) = chain_actor
                                            .send(crate::actors_v2::chain::messages::ChainMessage::ImportBlock {
                                                block: block.clone(),
                                                source: crate::actors_v2::chain::messages::BlockSource::Network(peer_id.clone()),
                                                peer_id: Some(peer_id.clone()),
                                            })
                                            .await
                                        {
                                            tracing::error!(
                                                height = block.message.execution_payload.block_number,
                                                error = %e,
                                                "Failed to import new block"
                                            );

                                            let mut s = state.write().unwrap();
                                            s.metrics.record_network_error();
                                        } else {
                                            // Update height after successful import
                                            let mut s = state.write().unwrap();
                                            let block_height = block.message.execution_payload.block_number;
                                            if block_height > s.current_height {
                                                s.current_height = block_height;
                                            }
                                            s.metrics.record_block_processed(block_height, Duration::from_millis(0));
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            peer = peer_id,
                                            error = %e,
                                            "Failed to deserialize block from network"
                                        );
                                        let mut s = state.write().unwrap();
                                        s.metrics.record_network_error();
                                    }
                                }
                            }
                        } else {
                            tracing::warn!("ChainActor not set, block queued but not processed");
                        }
                    }
                    .into_actor(self),
                );

                Ok(SyncResponse::BlockProcessed { block_height: 0 })
            }

            SyncMessage::HandleBlockResponse { blocks, request_id, peer_id } => {
                tracing::info!(
                    "📦 SYNC: HandleBlockResponse - {} blocks received from peer {} (request_id={})",
                    blocks.len(),
                    peer_id,
                    request_id
                );

                let state = std::sync::Arc::clone(&self.state);
                let chain_actor = self.chain_actor.clone();
                let peer_id_clone = peer_id.clone();
                let tendermint_validator = self.tendermint_validator.clone();

                ctx.spawn(
                    async move {
                        // Update state with received blocks
                        {
                            let mut s = state.write().unwrap();

                            // Find and complete the request (if tracked)
                            // Note: request_id format may vary, try both formats
                            let request_found = s.active_requests.remove(&request_id).is_some();

                            if request_found {
                                tracing::debug!(
                                    request_id = %request_id,
                                    "Found and removed matching request from active_requests"
                                );
                            } else {
                                tracing::debug!(
                                    request_id = %request_id,
                                    active_requests = ?s.active_requests.keys().collect::<Vec<_>>(),
                                    "Request not found in active_requests (may have been cleaned up)"
                                );
                            }

                            s.metrics.record_block_response(blocks.len() as u32);

                            // Queue blocks for processing
                            for block in blocks.clone() {
                                s.block_queue.push_back((block, peer_id_clone.clone()));
                            }

                            tracing::info!(
                                block_count = blocks.len(),
                                queue_size = s.block_queue.len(),
                                "Queued blocks for import processing"
                            );
                        }

                        // Process queued blocks if we have a ChainActor
                        if let Some(chain_actor) = chain_actor {
                            loop {
                                // Get next block from queue
                                let next_block = {
                                    let mut s = state.write().unwrap();
                                    s.block_queue.pop_front()
                                };

                                match next_block {
                                    Some((block_bytes, peer_id)) => {
                                        // Deserialize block from MessagePack format
                                        match crate::actors_v2::common::serialization::deserialize_block_from_network(&block_bytes) {
                                            Ok(block) => {
                                                let block_height = block.message.execution_payload.block_number;

                                                tracing::info!(
                                                    "⛓️  SYNC: Processing block #{} from queue (peer={})",
                                                    block_height,
                                                    peer_id
                                                );

                                                // Tendermint commit verification (Phase 3)
                                                if let Some(ref validator) = tendermint_validator {
                                                    match validator.write() {
                                                        Ok(mut v) => {
                                                            if let Err(e) = v.validate_sync_block(&block, block_height) {
                                                                tracing::error!(
                                                                    height = block_height,
                                                                    error = %e,
                                                                    peer = %peer_id,
                                                                    "Block failed Tendermint commit verification - rejecting"
                                                                );
                                                                let mut s = state.write().unwrap();
                                                                s.metrics.record_network_error();
                                                                continue; // Skip this block but process others
                                                            }
                                                            tracing::debug!(
                                                                height = block_height,
                                                                "Block passed Tendermint commit verification"
                                                            );
                                                        }
                                                        Err(e) => {
                                                            tracing::error!(
                                                                error = %e,
                                                                "Failed to acquire Tendermint validator lock"
                                                            );
                                                            continue;
                                                        }
                                                    }
                                                }

                                                if let Err(e) = chain_actor
                                                    .send(crate::actors_v2::chain::messages::ChainMessage::ImportBlock {
                                                        block: block.clone(),
                                                        source: crate::actors_v2::chain::messages::BlockSource::Sync,
                                                        peer_id: Some(peer_id.clone()),
                                                    })
                                                    .await
                                                {
                                                    tracing::error!(
                                                        height = block_height,
                                                        error = %e,
                                                        "Failed to send block to ChainActor"
                                                    );

                                                    // Record error in metrics
                                                    let mut s = state.write().unwrap();
                                                    s.metrics.record_network_error();
                                                    break;
                                                }

                                                // Update current height after successful import
                                                {
                                                    let mut s = state.write().unwrap();
                                                    let old_height = s.current_height;
                                                    if block_height > s.current_height {
                                                        s.current_height = block_height;
                                                        tracing::info!(
                                                            "📈 SYNC: Height updated {} → {} (target: {}, remaining: {})",
                                                            old_height,
                                                            block_height,
                                                            s.target_height,
                                                            s.target_height.saturating_sub(block_height)
                                                        );
                                                    }
                                                    s.metrics.record_block_processed(block_height, Duration::from_millis(0));
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    peer = peer_id,
                                                    error = %e,
                                                    "Failed to deserialize block from sync response"
                                                );

                                                let mut s = state.write().unwrap();
                                                s.metrics.record_network_error();
                                                // Continue processing other blocks despite this error
                                            }
                                        }
                                    }
                                    None => {
                                        // Queue is empty
                                        tracing::trace!("Block queue empty, processing complete");
                                        break;
                                    }
                                }
                            }
                        } else {
                            tracing::warn!("ChainActor not set, cannot process blocks");
                        }
                    }
                    .into_actor(self),
                );

                Ok(SyncResponse::BlockProcessed { block_height: 0 })
            }

            SyncMessage::SetNetworkActor { addr } => {
                self.network_actor = Some(addr);
                tracing::info!("NetworkActor address set for SyncActor coordination");
                Ok(SyncResponse::Started)
            }

            SyncMessage::SetChainActor { addr } => {
                self.chain_actor = Some(addr);
                tracing::info!("ChainActor address set for SyncActor coordination");
                Ok(SyncResponse::Started)
            }

            SyncMessage::SetStorageActor { addr } => {
                self.storage_actor = Some(addr);
                tracing::info!("StorageActor address set for SyncActor height queries");
                Ok(SyncResponse::Started)
            }

            SyncMessage::UpdatePeers { peers } => {
                let peer_count = peers.len();
                tracing::info!(
                    "👥 SYNC: UpdatePeers received - {} peers",
                    peer_count
                );

                let state = std::sync::Arc::clone(&self.state);

                ctx.spawn(
                    async move {
                        let mut s = state.write().unwrap();

                        let previous_count = s.sync_peers.len();
                        s.sync_peers = peers;
                        s.peer_selection_index = 0;

                        if previous_count != s.sync_peers.len() {
                            tracing::info!(
                                "👥 SYNC: Peer count changed {} → {}",
                                previous_count,
                                s.sync_peers.len()
                            );
                        }

                        // Reset bootstrap timer when peers first appear
                        if previous_count == 0 && s.sync_peers.len() > 0 {
                            tracing::info!(
                                peer_count = s.sync_peers.len(),
                                "First peers discovered - resetting bootstrap detection timer"
                            );

                            s.discovery_time_accumulated = Duration::ZERO;
                            s.state_entered_at = SystemTime::now();

                            // Transition based on sync status
                            if s.is_running && s.sync_state == SyncState::DiscoveringPeers {
                                const SYNC_THRESHOLD: u64 = 2;

                                if s.target_height == 0 {
                                    // target_height=0 means we haven't queried the network yet
                                    // Transition to QueryingNetworkHeight to discover actual network height
                                    tracing::info!(
                                        current_height = s.current_height,
                                        peer_count = s.sync_peers.len(),
                                        "First peers discovered - querying network height"
                                    );
                                    s.transition_to_state(SyncState::QueryingNetworkHeight);
                                } else if s.current_height + SYNC_THRESHOLD >= s.target_height {
                                    // We have a known target and we're within threshold
                                    let height = s.current_height;
                                    tracing::info!(
                                        current_height = height,
                                        target_height = s.target_height,
                                        peer_count = s.sync_peers.len(),
                                        "Already synced (within threshold of known target)"
                                    );
                                    s.transition_to_state(SyncState::Synced);
                                    s.is_running = false;
                                    s.last_sync_completed_at = Some(Instant::now());
                                    s.metrics.record_sync_complete(height);
                                } else {
                                    // We have a known target and we're behind it
                                    s.transition_to_state(SyncState::RequestingBlocks);
                                    tracing::info!(
                                        current_height = s.current_height,
                                        target_height = s.target_height,
                                        peer_count = s.sync_peers.len(),
                                        "Peers discovered, behind target - transitioning to RequestingBlocks"
                                    );
                                }
                            }
                        }

                        // If we lost all peers while syncing, go back to discovering
                        if s.sync_peers.is_empty() && s.is_running {
                            if s.sync_state == SyncState::RequestingBlocks
                                || s.sync_state == SyncState::ProcessingBlocks {
                                s.transition_to_state(SyncState::DiscoveringPeers);

                                tracing::warn!("All peers lost - returning to DiscoveringPeers");
                            }
                        }
                    }
                    .into_actor(self),
                );

                Ok(SyncResponse::Started)
            }

            SyncMessage::GetMetrics => {
                // Read-only access using std::sync::RwLock
                let metrics = self
                    .state
                    .read()
                    .map(|s| s.metrics.clone())
                    .map_err(|_| SyncError::Internal("Failed to acquire read lock".to_string()))?;

                Ok(SyncResponse::Metrics(metrics))
            }

            SyncMessage::QueryNetworkHeight => {
                tracing::debug!("Querying network for chain height");

                // Read-only access using std::sync::RwLock
                let target_height = self
                    .state
                    .read()
                    .map(|s| s.target_height)
                    .map_err(|_| SyncError::Internal("Failed to acquire read lock".to_string()))?;

                if target_height > 0 {
                    Ok(SyncResponse::NetworkHeight {
                        height: target_height,
                    })
                } else {
                    Err(SyncError::Internal(
                        "Network height not yet discovered".to_string(),
                    ))
                }
            }

            SyncMessage::ReportPeerHeights { peer_heights } => {
                // Handle peer height reports from NetworkActor
                // This is called when ChainStatusResponse messages arrive from peers
                //
                // ACTIVE HEIGHT MONITORING: This handler now processes heights in ALL states:
                // - QueryingNetworkHeight: Original behavior (initial sync discovery)
                // - Synced/Stopped: NEW - Detects when node falls behind network

                if peer_heights.is_empty() {
                    // Track consecutive queries with no response for stale detection
                    let mut s = self.state.write().unwrap();
                    s.consecutive_no_response_queries += 1;

                    // Threshold for stale detection (configurable via STALE_DETECTION_THRESHOLD)
                    const STALE_DETECTION_THRESHOLD: u32 = 3;

                    // After threshold queries (90 seconds at 30s intervals) with no responses,
                    // network_height is likely stale
                    if s.consecutive_no_response_queries >= STALE_DETECTION_THRESHOLD
                        && matches!(s.sync_state, SyncState::Synced | SyncState::Stopped)
                    {
                        tracing::warn!(
                            consecutive_no_responses = s.consecutive_no_response_queries,
                            threshold = STALE_DETECTION_THRESHOLD,
                            "⚠️ STALE NETWORK HEIGHT: No peer responses for {} queries - V2 peers may be disconnected",
                            s.consecutive_no_response_queries
                        );

                        // Reset counter after triggering health check to prevent spam
                        // Next trigger will require another STALE_DETECTION_THRESHOLD queries
                        s.consecutive_no_response_queries = 0;

                        // Signal to NetworkActor to check V2 peer health
                        if let Some(network) = self.network_actor.clone() {
                            drop(s); // Release lock before async
                            tokio::spawn(async move {
                                if let Err(e) = network.send(NetworkMessage::CheckV2PeerHealth).await {
                                    tracing::warn!(error = %e, "Failed to trigger V2 peer health check");
                                }
                            });
                        }
                    } else {
                        tracing::debug!(
                            consecutive_no_responses = s.consecutive_no_response_queries,
                            threshold = STALE_DETECTION_THRESHOLD,
                            "Empty peer heights report - tracking for stale detection"
                        );
                    }
                    return Ok(SyncResponse::Started);
                }

                tracing::debug!(
                    "🔍 SYNC: ReportPeerHeights - received {} peer height reports",
                    peer_heights.len()
                );

                // Log each peer's height for debugging
                for (peer_id, height, head_hash) in &peer_heights {
                    tracing::trace!(
                        "   └─ Peer {} reports height {} (hash: {:?})",
                        peer_id,
                        height,
                        &head_hash[..4]
                    );
                }

                let mut s = self.state.write().unwrap();
                let current_state = s.sync_state.clone();
                let now = Instant::now();

                // Reset stale detection counter when we receive valid responses
                s.consecutive_no_response_queries = 0;

                // Store timestamped observations for freshness filtering
                for (peer_id, height, _) in &peer_heights {
                    s.peer_height_observations.push(PeerHeightObservation {
                        peer_id: peer_id.clone(),
                        height: *height,
                        observed_at: now,
                    });

                    // Also maintain legacy observed_peer_heights for QueryingNetworkHeight state
                    if current_state == SyncState::QueryingNetworkHeight {
                        s.observed_peer_heights.push(*height);
                    }
                }

                // Prune stale observations (older than max_age)
                let max_age = Duration::from_secs(self.config.peer_height_max_age_secs);
                s.peer_height_observations.retain(|obs| now.duration_since(obs.observed_at) < max_age);

                // Handle based on current sync state
                match current_state {
                    SyncState::QueryingNetworkHeight => {
                        // Original behavior: Use mode for initial sync discovery
                        let consensus_height = Self::calculate_mode(&s.observed_peer_heights);

                        tracing::info!(
                            observations = s.observed_peer_heights.len(),
                            consensus_height = consensus_height,
                            "Calculated consensus network height from peer responses"
                        );

                        let current_height = s.current_height;

                        if consensus_height > current_height {
                            tracing::info!(
                                current_height = current_height,
                                consensus_height = consensus_height,
                                delta = consensus_height - current_height,
                                peer_count = s.observed_peer_heights.len(),
                                "Discovered higher chain from peer consensus (mode)!"
                            );

                            s.target_height = consensus_height;
                            s.observed_peer_heights.clear();

                            tracing::info!(
                                target_height = consensus_height,
                                "Transitioning from QueryingNetworkHeight to RequestingBlocks"
                            );
                            s.transition_to_state(SyncState::RequestingBlocks);

                            Ok(SyncResponse::NetworkHeight {
                                height: consensus_height,
                            })
                        } else {
                            tracing::debug!(
                                current_height = current_height,
                                consensus_height = consensus_height,
                                "Waiting for more responses or timeout"
                            );
                            Ok(SyncResponse::AlreadySynced)
                        }
                    }

                    SyncState::Synced | SyncState::Stopped => {
                        // ACTIVE HEIGHT MONITORING: Check if we've fallen behind while synced
                        // Use median for Byzantine resistance (single bad peer can't skew result)
                        let network_height = match Self::calculate_median_height(
                            &s.peer_height_observations,
                            max_age,
                            self.config.min_peer_quorum,
                        ) {
                            Some(h) => h,
                            None => {
                                tracing::trace!("Insufficient fresh peer heights for monitoring");
                                return Ok(SyncResponse::Started);
                            }
                        };

                        // Always update target_height if peers report higher
                        if network_height > s.target_height {
                            s.target_height = network_height;
                        }

                        // Extract state needed for async storage query
                        let sync_actor_height = s.current_height; // Stale fallback
                        let last_sync_completed_at = s.last_sync_completed_at;
                        let consecutive_behind_checks = s.consecutive_behind_checks;
                        let resync_threshold = self.config.resync_threshold;
                        let sync_cooldown_secs = self.config.sync_cooldown_secs;
                        let state = std::sync::Arc::clone(&self.state);
                        let storage_actor = self.storage_actor.clone();

                        // Drop state lock before spawning async task
                        drop(s);

                        // Spawn async task to query StorageActor and evaluate gap
                        // This is necessary because the handler is synchronous but we need async I/O
                        ctx.spawn(
                            async move {
                                // Query StorageActor for authoritative chain height
                                // This ensures we use the actual imported height, not SyncActor's stale tracking
                                let storage_height = if let Some(storage) = storage_actor {
                                    match storage.send(GetChainHeadMessage { correlation_id: None }).await {
                                        Ok(Ok(Some(head))) => {
                                            tracing::trace!(
                                                storage_height = head.number,
                                                sync_actor_height = sync_actor_height,
                                                "Using StorageActor height for gap calculation"
                                            );
                                            head.number
                                        }
                                        Ok(Ok(None)) => {
                                            tracing::debug!("No chain head in storage, using SyncActor height");
                                            sync_actor_height
                                        }
                                        Ok(Err(e)) => {
                                            tracing::warn!(error = ?e, "StorageActor error, using SyncActor height");
                                            sync_actor_height
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, "StorageActor mailbox error, using SyncActor height");
                                            sync_actor_height
                                        }
                                    }
                                } else {
                                    tracing::debug!("No StorageActor configured, using SyncActor height");
                                    sync_actor_height
                                };

                                // Re-acquire state lock for updates
                                let mut s = state.write().unwrap();

                                // Calculate gap using authoritative storage height
                                let gap = network_height.saturating_sub(storage_height);

                                if gap > resync_threshold {
                                    s.consecutive_behind_checks = consecutive_behind_checks + 1;

                                    // Check cooldown (don't trigger re-sync too soon after last sync)
                                    let cooldown_elapsed = last_sync_completed_at
                                        .map(|t| t.elapsed() > Duration::from_secs(sync_cooldown_secs))
                                        .unwrap_or(true);

                                    // Require 2 consecutive checks showing gap AND cooldown elapsed
                                    // This prevents thrashing from transient network conditions
                                    if s.consecutive_behind_checks >= 2 && cooldown_elapsed {
                                        tracing::warn!(
                                            storage_height = storage_height,
                                            network_height = network_height,
                                            gap = gap,
                                            consecutive_checks = s.consecutive_behind_checks,
                                            "🚨 ACTIVE MONITORING: Fell behind network - triggering re-sync"
                                        );

                                        // Reset state and trigger re-sync
                                        s.consecutive_behind_checks = 0;
                                        s.is_running = true;
                                        s.transition_to_state(SyncState::RequestingBlocks);
                                    } else {
                                        tracing::debug!(
                                            storage_height = storage_height,
                                            gap = gap,
                                            consecutive_checks = s.consecutive_behind_checks,
                                            cooldown_elapsed = cooldown_elapsed,
                                            "Behind network but waiting for confirmation before re-sync"
                                        );
                                    }
                                } else {
                                    // Gap is acceptable - reset consecutive check counter
                                    if consecutive_behind_checks > 0 {
                                        tracing::trace!(
                                            storage_height = storage_height,
                                            network_height = network_height,
                                            "Gap reduced below threshold - resetting consecutive check counter"
                                        );
                                    }
                                    s.consecutive_behind_checks = 0;
                                }
                            }
                            .into_actor(self),
                        );

                        Ok(SyncResponse::Started)
                    }

                    _ => {
                        // During active sync (RequestingBlocks, ProcessingBlocks, etc.)
                        // Don't interfere with ongoing sync operations
                        tracing::trace!(
                            state = ?current_state,
                            "Storing peer heights but not processing during active sync"
                        );
                        Ok(SyncResponse::Started)
                    }
                }
            }

            // ========================================================================
            // ACTIVE NETWORK HEIGHT MONITORING - New Message Handlers
            // ========================================================================

            SyncMessage::RefreshNetworkHeight => {
                // Force immediate peer height query (used after reconnection)
                let network_actor = self.network_actor.clone();

                if let Some(network) = network_actor {
                    tracing::debug!("RefreshNetworkHeight: Forcing immediate peer height query");
                    tokio::spawn(async move {
                        if let Err(e) = network.send(NetworkMessage::QueryPeerHeights).await {
                            tracing::warn!(
                                error = %e,
                                "Failed to query peer heights for refresh"
                            );
                        }
                    });
                    Ok(SyncResponse::Started)
                } else {
                    Err(SyncError::NetworkActorNotSet)
                }
            }

            SyncMessage::ForceResync { reason } => {
                // Emergency re-sync trigger (e.g., after repeated PayloadIdUnavailable errors)
                let mut s = self.state.write().unwrap();

                // Don't force resync if already actively syncing
                if s.is_running && !matches!(s.sync_state, SyncState::Synced | SyncState::Stopped) {
                    tracing::debug!(
                        reason = %reason,
                        current_state = ?s.sync_state,
                        "ForceResync ignored - sync already in progress"
                    );
                    return Ok(SyncResponse::Started);
                }

                tracing::warn!(
                    reason = %reason,
                    current_height = s.current_height,
                    target_height = s.target_height,
                    "🚨 FORCE RE-SYNC triggered"
                );

                // Reset monitoring state
                s.consecutive_behind_checks = 0;
                s.peer_height_observations.clear();

                // Reset target_height so QueryingNetworkHeight will re-discover network height
                s.target_height = 0;

                // Start sync - transition to QueryingNetworkHeight (has 2s polling interval)
                // instead of DiscoveringPeers (has no polling interval and would get stuck)
                s.is_running = true;
                s.transition_to_state(SyncState::QueryingNetworkHeight);

                Ok(SyncResponse::Started)
            }

            SyncMessage::UpdateCurrentHeight { height } => {
                // Update current_height to stay in sync with StorageActor
                // Called by ChainActor after any successful block import (sync, gossipsub, production)
                let mut s = self.state.write().unwrap();

                // Only update if the new height is greater (blocks should be imported in order)
                if height > s.current_height {
                    tracing::trace!(
                        previous_height = s.current_height,
                        new_height = height,
                        "Updating current_height from block import notification"
                    );
                    s.current_height = height;

                    // Also update target_height if we've exceeded it (can happen via gossipsub)
                    if height > s.target_height {
                        tracing::debug!(
                            previous_target = s.target_height,
                            new_target = height,
                            "Updating target_height from block import (exceeded via gossipsub)"
                        );
                        s.target_height = height;
                    }
                }

                Ok(SyncResponse::Started)
            }
        }
    }
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    fn create_test_actor() -> SyncActor {
        let config = SyncConfig {
            max_blocks_per_request: 100,
            sync_timeout: Duration::from_secs(30),
            max_concurrent_requests: 5,
            block_validation_timeout: Duration::from_secs(10),
            max_sync_peers: 10,
            data_dir: std::path::PathBuf::from("/tmp/test"),
            ..Default::default()
        };
        SyncActor::new(config).unwrap()
    }

    #[tokio::test]
    async fn test_bootstrap_detection_genesis_no_peers_timeout() {
        let actor = create_test_actor();

        // Setup: Genesis state, no peers
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 0;
            s.target_height = 0;
            s.sync_peers = vec![];
            s.transition_to_state(SyncState::DiscoveringPeers);
        }

        // Before timeout: should be syncing (returns true)
        {
            let s = actor.state.read().unwrap();
            assert_eq!(s.determine_sync_state(), true);
        }

        // After timeout: should NOT be syncing (bootstrap mode, returns false)
        {
            let mut s = actor.state.write().unwrap();
            s.discovery_time_accumulated = Duration::from_secs(31);
            assert_eq!(s.determine_sync_state(), false);
        }
    }

    #[tokio::test]
    async fn test_bootstrap_detection_not_at_genesis() {
        let actor = create_test_actor();

        // Setup: NOT at genesis, no peers, timeout reached
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 10; // Not genesis
            s.target_height = 0;
            s.sync_peers = vec![];
            s.discovery_time_accumulated = Duration::from_secs(31);
            s.transition_to_state(SyncState::DiscoveringPeers);
        }

        // Should still be syncing (not genesis - prevents forks)
        {
            let s = actor.state.read().unwrap();
            assert_eq!(s.determine_sync_state(), true);
        }
    }

    #[tokio::test]
    async fn test_bootstrap_detection_has_peers() {
        let actor = create_test_actor();

        // Setup: Genesis, HAS peers, timeout reached
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 0;
            s.target_height = 0;
            s.sync_peers = vec!["peer1".to_string()]; // Has peer
            s.discovery_time_accumulated = Duration::from_secs(31);
            s.transition_to_state(SyncState::DiscoveringPeers);
        }

        // Should still be syncing (has peers to sync from)
        {
            let s = actor.state.read().unwrap();
            assert_eq!(s.determine_sync_state(), true);
        }
    }

    #[tokio::test]
    async fn test_bootstrap_detection_before_timeout() {
        let actor = create_test_actor();

        // Setup: Genesis, no peers, BEFORE timeout
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 0;
            s.target_height = 0;
            s.sync_peers = vec![];
            s.discovery_time_accumulated = Duration::from_secs(15); // Half timeout
            s.transition_to_state(SyncState::DiscoveringPeers);
        }

        // Should still be syncing (timeout not reached)
        {
            let s = actor.state.read().unwrap();
            assert_eq!(s.determine_sync_state(), true);
        }
    }

    #[tokio::test]
    async fn test_discovery_time_accumulation() {
        let actor = create_test_actor();

        // Simulate multiple discovery attempts
        {
            let mut s = actor.state.write().unwrap();
            s.transition_to_state(SyncState::DiscoveringPeers);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        {
            let mut s = actor.state.write().unwrap();
            s.transition_to_state(SyncState::RequestingBlocks);
            let accumulated = s.discovery_time_accumulated;
            assert!(accumulated >= Duration::from_millis(90));
            assert!(accumulated <= Duration::from_millis(200));
        }

        // Re-enter discovery - time should reset
        {
            let mut s = actor.state.write().unwrap();
            s.transition_to_state(SyncState::DiscoveringPeers);
            assert_eq!(s.discovery_time_accumulated, Duration::ZERO);
        }
    }

    #[tokio::test]
    async fn test_determine_sync_state_known_target() {
        let actor = create_test_actor();

        // Case: Target known, behind
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 10;
            s.target_height = 20;
            assert_eq!(s.determine_sync_state(), true); // Syncing
        }

        // Case: Target known, caught up
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 19;
            s.target_height = 20;
            assert_eq!(s.determine_sync_state(), false); // Not syncing (within threshold)
        }
    }

    #[tokio::test]
    async fn test_determine_sync_state_unknown_target_bootstrap() {
        let actor = create_test_actor();

        // Case: Unknown target, genesis, no peers, timeout
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 0;
            s.target_height = 0;
            s.sync_peers = vec![];
            s.transition_to_state(SyncState::DiscoveringPeers);
            s.discovery_time_accumulated = Duration::from_secs(31);
        }

        {
            let s = actor.state.read().unwrap();
            assert_eq!(s.determine_sync_state(), false); // Bootstrap mode
        }
    }

    #[tokio::test]
    async fn test_get_sync_status_uses_bootstrap_detection() {
        let actor = create_test_actor();

        // Setup bootstrap scenario
        {
            let mut s = actor.state.write().unwrap();
            s.current_height = 0;
            s.target_height = 0;
            s.sync_peers = vec![];
            s.transition_to_state(SyncState::DiscoveringPeers);
            s.discovery_time_accumulated = Duration::from_secs(31);
        }

        {
            let s = actor.state.read().unwrap();
            let status = s.get_sync_status();
            assert_eq!(status.is_syncing, false); // Bootstrap mode active
            assert_eq!(status.current_height, 0);
            assert_eq!(status.target_height, 0);
        }
    }
}
