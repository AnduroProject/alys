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
use std::time::{Duration, SystemTime};

use super::{
    messages::{Block, NetworkMessage, PeerId, SyncStatus},
    sync_checkpoint::SyncCheckpoint,
    SyncConfig, SyncError, SyncMessage, SyncMetrics, SyncResponse,
};

/// Simplified sync states (linear progression)
#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    Stopped,
    Starting,
    DiscoveringPeers,
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
        // Accumulate discovery time before transitioning out of DiscoveringPeers
        if self.sync_state == SyncState::DiscoveringPeers {
            if let Ok(elapsed) = self.state_entered_at.elapsed() {
                self.discovery_time_accumulated += elapsed;

                tracing::debug!(
                    discovery_time_secs = self.discovery_time_accumulated.as_secs(),
                    "Accumulated discovery time"
                );
            }
        }

        // Reset accumulated time when entering DiscoveringPeers from a different state
        if new_state == SyncState::DiscoveringPeers
            && self.sync_state != SyncState::DiscoveringPeers
        {
            self.discovery_time_accumulated = Duration::ZERO;
            tracing::debug!("Reset discovery time for new discovery cycle");
        }

        // Transition to new state
        let old_state = std::mem::replace(&mut self.sync_state, new_state);
        self.state_entered_at = SystemTime::now();

        tracing::info!(
            old_state = ?old_state,
            new_state = ?self.sync_state,
            "SyncActor state transition"
        );
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

        Ok(Self {
            state: std::sync::Arc::new(std::sync::RwLock::new(SyncActorState::new())),
            config,
            network_actor: None,
            chain_actor: None,
        })
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

        // Load checkpoint on startup
        let addr = ctx.address();
        tokio::spawn(async move {
            if let Err(e) = addr.send(SyncMessage::LoadCheckpoint).await {
                tracing::error!("Failed to load checkpoint: {}", e);
            }
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

        // Periodic checkpoint saving (every 30 seconds during sync)
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            let addr_clone = ctx.address();

            // Check state synchronously (RwLockReadGuard is not Send)
            let should_save = {
                let s = act.state.read().unwrap();
                s.is_running && matches!(
                    s.sync_state,
                    SyncState::RequestingBlocks | SyncState::ProcessingBlocks
                )
            };

            if should_save {
                tokio::spawn(async move {
                    if let Err(e) = addr_clone.send(SyncMessage::SaveCheckpoint).await {
                        tracing::error!("Failed to save checkpoint: {}", e);
                    }
                });
            }
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
                // 1. Sync is running and in active sync state
                // 2. No pending requests or blocks
                // 3. Either: target_height == 0 (no peer has higher chain, already synced)
                //    OR: we've reached target_height within threshold
                let in_sync_state = s.is_running
                    && (s.sync_state == SyncState::RequestingBlocks
                        || s.sync_state == SyncState::ProcessingBlocks);

                let no_pending_work = s.active_requests.is_empty() && s.block_queue.is_empty();

                let reached_target = s.target_height == 0
                    || s.current_height + SYNC_THRESHOLD >= s.target_height;

                let is_complete = in_sync_state && no_pending_work && reached_target;

                if is_complete {
                    let current = s.current_height;
                    let target = s.target_height;

                    if target == 0 {
                        tracing::info!(
                            current_height = current,
                            "Sync complete - no higher chain discovered (already synced)"
                        );
                    } else {
                        tracing::info!(
                            current_height = current,
                            target_height = target,
                            "Sync complete - reached target height"
                        );
                    }

                    s.transition_to_state(SyncState::Synced);
                    s.is_running = false;
                    s.metrics.record_sync_complete(current);

                    tracing::info!("Sync completed successfully");
                }
            });
        });
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

                tracing::info!(
                    start_height = start_height,
                    target_height = ?target_height,
                    "Received StartSync message"
                );

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
                                            // Check if we're already synced (target_height == 0 means no peer has a higher chain)
                                            const SYNC_THRESHOLD: u64 = 2;
                                            let already_synced = s.target_height == 0
                                                || s.current_height + SYNC_THRESHOLD >= s.target_height;

                                            if already_synced {
                                                let height = s.current_height;
                                                tracing::info!(
                                                    current_height = height,
                                                    target_height = s.target_height,
                                                    "Already synced (no higher chain discovered) - completing sync"
                                                );
                                                s.transition_to_state(SyncState::Synced);
                                                s.is_running = false;
                                                s.metrics.record_sync_complete(height);
                                            } else {
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
                let state = std::sync::Arc::clone(&self.state);

                ctx.spawn(
                    async move {
                        let mut s = state.write().unwrap();
                        s.sync_state = SyncState::Stopped;
                        s.metrics.stop_sync();
                        s.is_running = false;
                        tracing::info!("Sync stopped");
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

            SyncMessage::HandleBlockResponse { blocks, request_id } => {
                tracing::debug!(
                    "Received {} blocks for request {}",
                    blocks.len(),
                    request_id
                );

                let state = std::sync::Arc::clone(&self.state);
                let chain_actor = self.chain_actor.clone();

                ctx.spawn(
                    async move {
                        // Update state with received blocks
                        {
                            let mut s = state.write().unwrap();

                            // Find and complete the request
                            if let Some(request_info) = s.active_requests.remove(&request_id) {
                                s.metrics.record_block_response(blocks.len() as u32);

                                // Queue blocks for processing
                                for block in blocks.clone() {
                                    s.block_queue.push_back((block, request_info.peer_id.clone()));
                                }

                                tracing::debug!(
                                    "Queued {} blocks (queue size: {})",
                                    blocks.len(),
                                    s.block_queue.len()
                                );
                            }
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

                                                tracing::debug!(
                                                    height = block_height,
                                                    peer = peer_id,
                                                    "Processing block from queue"
                                                );

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

            SyncMessage::UpdatePeers { peers } => {
                let state = std::sync::Arc::clone(&self.state);

                ctx.spawn(
                    async move {
                        let mut s = state.write().unwrap();

                        let previous_count = s.sync_peers.len();
                        s.sync_peers = peers;
                        s.peer_selection_index = 0;

                        tracing::info!(
                            previous_count = previous_count,
                            new_count = s.sync_peers.len(),
                            "Updated sync peers"
                        );

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
                                // Check if we're already synced (target_height == 0 means no peer has a higher chain)
                                const SYNC_THRESHOLD: u64 = 2;
                                let already_synced = s.target_height == 0
                                    || s.current_height + SYNC_THRESHOLD >= s.target_height;

                                if already_synced {
                                    let height = s.current_height;
                                    tracing::info!(
                                        current_height = height,
                                        target_height = s.target_height,
                                        peer_count = s.sync_peers.len(),
                                        "Already synced (no higher chain discovered) - completing sync"
                                    );
                                    s.transition_to_state(SyncState::Synced);
                                    s.is_running = false;
                                    s.metrics.record_sync_complete(height);
                                } else {
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

            // Phase 5: Checkpoint/Resume handlers
            SyncMessage::LoadCheckpoint => {
                tracing::debug!("Loading sync checkpoint");

                let state = std::sync::Arc::clone(&self.state);
                let data_dir = self.config.data_dir.clone();

                ctx.spawn(
                    async move {
                        match SyncCheckpoint::load(&data_dir).await {
                            Ok(Some(checkpoint)) => {
                                tracing::info!(
                                    current_height = checkpoint.current_height,
                                    target_height = checkpoint.target_height,
                                    blocks_synced = checkpoint.blocks_synced,
                                    "Loaded sync checkpoint successfully"
                                );

                                // Restore state from checkpoint
                                let mut s = state.write().unwrap();

                                s.current_height = checkpoint.current_height;
                                s.target_height = checkpoint.target_height;

                                // Determine if we should resume syncing
                                const SYNC_THRESHOLD: u64 = 2;
                                let needs_sync = checkpoint.target_height > 0
                                    && checkpoint.current_height + SYNC_THRESHOLD < checkpoint.target_height;

                                if needs_sync {
                                    // Resume sync - transition to Starting
                                    s.transition_to_state(SyncState::Starting);
                                    s.is_running = true;

                                    tracing::info!(
                                        resume_from = checkpoint.current_height,
                                        target = checkpoint.target_height,
                                        remaining = checkpoint.target_height - checkpoint.current_height,
                                        "Resuming sync from checkpoint"
                                    );

                                    // Transition to DiscoveringPeers
                                    // UpdatePeers or GetConnectedPeers will populate peers
                                    s.transition_to_state(SyncState::DiscoveringPeers);
                                } else {
                                    // Sync was complete or nearly complete
                                    s.transition_to_state(SyncState::Synced);
                                    s.is_running = false;

                                    tracing::info!(
                                        current_height = checkpoint.current_height,
                                        "Checkpoint indicates sync complete"
                                    );
                                }

                                // Update metrics with checkpoint info
                                s.metrics.record_checkpoint_loaded(checkpoint.blocks_synced);
                            }
                            Ok(None) => {
                                tracing::debug!("No checkpoint file found - starting fresh");

                                // Initialize with genesis state
                                let mut s = state.write().unwrap();
                                s.current_height = 0;
                                s.target_height = 0;
                                s.transition_to_state(SyncState::Stopped);
                                s.is_running = false;
                            }
                            Err(e) => {
                                tracing::error!(
                                    error = %e,
                                    "Failed to load checkpoint - starting fresh"
                                );

                                // On error, start fresh (safe fallback)
                                let mut s = state.write().unwrap();
                                s.current_height = 0;
                                s.target_height = 0;
                                s.transition_to_state(SyncState::Stopped);
                                s.is_running = false;
                            }
                        }
                    }
                    .into_actor(self),
                );

                Ok(SyncResponse::Started)
            }

            SyncMessage::SaveCheckpoint => {
                tracing::trace!("Saving sync checkpoint");

                let state = std::sync::Arc::clone(&self.state);
                let data_dir = self.config.data_dir.clone();

                ctx.spawn(
                    async move {
                        let s = state.read().unwrap();

                        // Only save if actively syncing
                        if matches!(
                            s.sync_state,
                            SyncState::RequestingBlocks | SyncState::ProcessingBlocks
                        ) {
                            let checkpoint = SyncCheckpoint::new(
                                s.current_height,
                                s.target_height,
                                s.current_height,
                            );

                            drop(s); // Release read lock before async I/O

                            if let Err(e) = checkpoint.save(&data_dir).await {
                                tracing::error!("Failed to save checkpoint: {}", e);
                            } else {
                                tracing::trace!("Checkpoint saved successfully");
                            }
                        }
                    }
                    .into_actor(self),
                );

                Ok(SyncResponse::Started)
            }

            SyncMessage::ClearCheckpoint => {
                tracing::debug!("Clearing sync checkpoint");

                let data_dir = self.config.data_dir.clone();

                ctx.spawn(
                    async move {
                        if let Err(e) = SyncCheckpoint::delete(&data_dir).await {
                            tracing::error!("Failed to clear checkpoint: {}", e);
                        } else {
                            tracing::debug!("Checkpoint cleared successfully");
                        }
                    }
                    .into_actor(self),
                );

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
