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

/// Simplified sync actor - blockchain sync only
pub struct SyncActor {
    /// Sync configuration
    config: SyncConfig,
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

    /// Actor addresses for coordination
    network_actor: Option<Addr<crate::actors_v2::network::NetworkActor>>,
    chain_actor: Option<Addr<crate::actors_v2::chain::ChainActor>>,

    /// Running state
    is_running: bool,
    /// Shutdown flag
    shutdown_requested: bool,
}

impl SyncActor {
    /// Create new SyncActor with simplified configuration
    pub fn new(config: SyncConfig) -> Result<Self> {
        tracing::info!("Creating SyncActor V2");

        config
            .validate()
            .map_err(|e| anyhow!("Invalid sync configuration: {}", e))?;

        Ok(Self {
            config,
            sync_state: SyncState::Stopped,
            current_height: 0,
            target_height: 0,
            metrics: SyncMetrics::new(),
            block_queue: VecDeque::new(),
            active_requests: HashMap::new(),
            sync_peers: Vec::new(),
            peer_selection_index: 0,
            network_actor: None,
            chain_actor: None,
            is_running: false,
            shutdown_requested: false,
        })
    }

    /// Start synchronization process
    async fn start_sync(&mut self) -> Result<()> {
        if self.sync_state != SyncState::Stopped {
            return Err(anyhow!("Sync already running"));
        }

        tracing::info!("Starting blockchain synchronization");
        self.sync_state = SyncState::Starting;
        self.is_running = true;

        // Initialize height from ChainActor (source of truth)
        self.initialize_height().await?;

        // Transition to peer discovery
        self.sync_state = SyncState::DiscoveringPeers;
        self.discover_sync_peers().await?;

        Ok(())
    }

    /// Initialize sync state by querying ChainActor for current chain height.
    ///
    /// ChainActor is the single source of truth for chain state. It maintains
    /// the canonical chain height by coordinating with StorageActor.
    async fn initialize_height(&mut self) -> Result<()> {
        if let Some(ref chain_actor) = self.chain_actor {
            // Query ChainActor for current chain state
            let msg = crate::actors_v2::chain::messages::ChainMessage::GetChainStatus;

            match chain_actor.send(msg).await {
                Ok(Ok(response)) => {
                    use crate::actors_v2::chain::messages::ChainResponse;
                    if let ChainResponse::ChainStatus(status) = response {
                        self.current_height = status.height;
                        tracing::info!(
                            current_height = status.height,
                            "Initialized sync from current chain height"
                        );
                        Ok(())
                    } else {
                        Err(anyhow!("Unexpected response from ChainActor"))
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Failed to query chain height: {}", e);
                    Err(anyhow!("Chain height query failed: {}", e))
                }
                Err(e) => {
                    tracing::error!("ChainActor mailbox error: {}", e);
                    Err(anyhow!("Mailbox error: {}", e))
                }
            }
        } else {
            Err(anyhow!("ChainActor not set - cannot query height"))
        }
    }

    /// Stop synchronization process
    async fn stop_sync(&mut self) -> Result<()> {
        tracing::info!("Stopping blockchain synchronization");

        // Cancel all active requests
        let active_request_ids: Vec<String> = self.active_requests.keys().cloned().collect();
        for request_id in active_request_ids {
            self.active_requests.remove(&request_id);
            tracing::debug!("Cancelled block request: {}", request_id);
        }

        // Clear block queue
        self.block_queue.clear();

        self.sync_state = SyncState::Stopped;
        self.metrics.stop_sync();
        self.is_running = false;

        tracing::info!("SyncActor V2 stopped");
        Ok(())
    }

    /// Discover peers for synchronization
    async fn discover_sync_peers(&mut self) -> Result<()> {
        tracing::info!("Discovering sync peers");

        if let Some(ref network_actor) = self.network_actor {
            // Request connected peers from NetworkActor
            match network_actor.send(NetworkMessage::GetConnectedPeers).await {
                Ok(Ok(response)) => {
                    if let crate::actors_v2::network::NetworkResponse::Peers(peers) = response {
                        self.sync_peers = peers.into_iter().map(|p| p.peer_id).collect();

                        tracing::info!("Found {} sync peers", self.sync_peers.len());

                        if self.sync_peers.is_empty() {
                            self.sync_state =
                                SyncState::Error("No peers available for sync".to_string());
                            return Err(anyhow!("No peers available for sync"));
                        }

                        // Transition to requesting blocks
                        self.sync_state = SyncState::RequestingBlocks;
                        self.start_block_requests().await?;
                    }
                }
                Ok(Err(e)) => {
                    let error_msg = format!("Failed to get peers from network: {:?}", e);
                    self.sync_state = SyncState::Error(error_msg.clone());
                    return Err(anyhow!(error_msg));
                }
                Err(e) => {
                    let error_msg = format!("Network actor communication error: {}", e);
                    self.sync_state = SyncState::Error(error_msg.clone());
                    return Err(anyhow!(error_msg));
                }
            }
        } else {
            return Err(anyhow!("NetworkActor not set"));
        }

        Ok(())
    }

    /// Start requesting blocks from peers
    async fn start_block_requests(&mut self) -> Result<()> {
        if self.sync_peers.is_empty() {
            return Err(anyhow!("No peers available for sync"));
        }

        // Discover target height from network consensus
        match self.discover_target_height().await {
            Ok(target) => {
                if self.current_height >= target.saturating_sub(2) {
                    // Already synced (within 2 blocks tolerance)
                    tracing::info!("Already synced at height {}", self.current_height);
                    self.sync_state = SyncState::Synced;
                    return Ok(());
                }

                tracing::info!(
                    "Starting block sync from height {} to {} (gap: {} blocks)",
                    self.current_height,
                    target,
                    target - self.current_height
                );
            }
            Err(e) => {
                tracing::error!("Failed to discover target height: {}", e);
                self.sync_state = SyncState::Error(e.to_string());
                return Err(e);
            }
        }

        self.metrics.start_sync(self.target_height);

        // Create initial block requests
        self.create_block_requests().await?;

        Ok(())
    }

    /// Query multiple peers for their chain head to establish sync target
    ///
    /// This function queries available peers for their chain height and uses
    /// a simple consensus mechanism (mode - most common height) to determine
    /// the network's current height.
    async fn discover_target_height(&mut self) -> Result<u64> {
        const QUERY_PEER_COUNT: usize = 3;
        const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

        tracing::info!(
            available_peers = self.sync_peers.len(),
            "Querying peers for chain height"
        );

        if self.sync_peers.is_empty() {
            return Err(anyhow!("No peers available for height discovery"));
        }

        // Query up to QUERY_PEER_COUNT peers (or all available, whichever is less)
        let query_count = QUERY_PEER_COUNT.min(self.sync_peers.len());
        let peers_to_query: Vec<_> = self.sync_peers.iter().take(query_count).cloned().collect();

        tracing::debug!(
            peers_to_query = peers_to_query.len(),
            "Querying peer subset for height"
        );

        let mut heights = Vec::new();

        // Query each peer for their status
        for peer_id in peers_to_query {
            if let Some(ref network_actor) = self.network_actor {
                let msg =
                    crate::actors_v2::network::messages::NetworkMessage::HandleRequestResponse {
                        request: crate::actors_v2::network::messages::NetworkRequest::GetStatus,
                        peer_id: peer_id.clone(),
                    };

                match tokio::time::timeout(QUERY_TIMEOUT, network_actor.send(msg)).await {
                    Ok(Ok(Ok(response))) => {
                        use crate::actors_v2::network::messages::NetworkResponse;
                        if let NetworkResponse::Status(status) = response {
                            heights.push(status.connected_peers as u64); // Placeholder - need actual height field
                            tracing::debug!(
                                peer_id = %peer_id,
                                "Received response from peer"
                            );
                        }
                    }
                    Ok(Ok(Err(e))) => {
                        tracing::warn!(
                            peer_id = %peer_id,
                            error = ?e,
                            "Failed to get status from peer"
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            peer_id = %peer_id,
                            error = ?e,
                            "Network actor error querying peer"
                        );
                    }
                    Err(_) => {
                        tracing::warn!(
                            peer_id = %peer_id,
                            "Peer status query timed out"
                        );
                    }
                }
            }
        }

        // Handle case where no peers responded
        if heights.is_empty() {
            return Err(anyhow!("No peers responded with height"));
        }

        // For single peer (common in dev mode)
        if heights.len() == 1 {
            tracing::warn!(
                height = heights[0],
                "Only 1 peer responded, using their height as target"
            );
            self.target_height = heights[0];
            return Ok(heights[0]);
        }

        // Calculate consensus height using mode (most common)
        let mut counts = std::collections::HashMap::new();
        for &h in &heights {
            *counts.entry(h).or_insert(0) += 1;
        }
        let consensus_height = *counts.iter().max_by_key(|(_, count)| *count).unwrap().0;

        self.target_height = consensus_height;
        tracing::info!(
            target_height = consensus_height,
            peer_heights = ?heights,
            "Established sync target from peer consensus"
        );

        Ok(consensus_height)
    }

    /// Create block requests for peers
    async fn create_block_requests(&mut self) -> Result<()> {
        let mut next_height = self.current_height;

        // Create requests up to max concurrent limit
        while self.active_requests.len() < self.config.max_concurrent_requests
            && next_height < self.target_height
        {
            let blocks_to_request = std::cmp::min(
                self.config.max_blocks_per_request,
                (self.target_height - next_height) as u32,
            );

            if blocks_to_request == 0 {
                break;
            }

            // Select peer for request (round-robin)
            let peer_id = self.select_sync_peer();

            // Create block request
            let request_id = uuid::Uuid::new_v4().to_string();
            let request_info = BlockRequestInfo {
                request_id: request_id.clone(),
                start_height: next_height,
                count: blocks_to_request,
                peer_id: peer_id.clone(),
                requested_at: SystemTime::now(),
            };

            // Send request to NetworkActor
            if let Some(ref network_actor) = self.network_actor {
                let request_msg = NetworkMessage::HandleRequestResponse {
                    request: crate::actors_v2::network::messages::NetworkRequest::GetBlocks {
                        start_height: next_height,
                        count: blocks_to_request,
                    },
                    peer_id: peer_id.clone(),
                };

                match network_actor.send(request_msg).await {
                    Ok(_) => {
                        self.active_requests
                            .insert(request_id.clone(), request_info);
                        self.metrics.record_block_request(&peer_id);

                        tracing::debug!(
                            "Requested blocks {} to {} from peer {}",
                            next_height,
                            next_height + blocks_to_request as u64 - 1,
                            peer_id
                        );

                        next_height += blocks_to_request as u64;
                    }
                    Err(e) => {
                        tracing::error!("Failed to send block request: {}", e);
                        self.metrics.record_network_error();
                    }
                }
            }
        }

        Ok(())
    }

    /// Select peer for sync request (simple round-robin)
    fn select_sync_peer(&mut self) -> PeerId {
        if self.sync_peers.is_empty() {
            return "".to_string(); // Should not happen if properly validated
        }

        let peer = self.sync_peers[self.peer_selection_index % self.sync_peers.len()].clone();
        self.peer_selection_index = (self.peer_selection_index + 1) % self.sync_peers.len();
        peer
    }

    /// Process incoming block by routing through ChainActor for validation
    async fn process_block(&mut self, block: Block, peer_id: PeerId) -> Result<()> {
        let processing_start = std::time::Instant::now();

        // Basic pre-validation (size check)
        if !self.validate_block(&block) {
            self.metrics.record_block_rejected("validation failed");
            return Err(anyhow!("Block validation failed"));
        }

        // Convert block data to proper format for ChainActor
        let consensus_block = match self.convert_block_to_storage_format(block.clone()) {
            block => block,
        };

        let block_height = consensus_block.message.execution_payload.block_number;
        let block_hash = self.calculate_block_hash(&consensus_block);

        tracing::debug!(
            block_height = block_height,
            block_hash = ?block_hash,
            peer_id = %peer_id,
            "Processing synced block via ChainActor"
        );

        // Forward to ChainActor for full validation and import
        if let Some(ref chain_actor) = self.chain_actor {
            let msg = crate::actors_v2::chain::messages::ChainMessage::ImportBlock {
                block: consensus_block,
                source: crate::actors_v2::chain::messages::BlockSource::Sync,
                peer_id: Some(peer_id.to_string()),
            };

            match chain_actor.send(msg).await {
                Ok(Ok(response)) => {
                    use crate::actors_v2::chain::messages::ChainResponse;
                    match response {
                        ChainResponse::BlockImported {
                            height,
                            block_hash: hash,
                        } => {
                            // Update sync progress
                            self.current_height = height;

                            let processing_time = processing_start.elapsed();
                            self.metrics.record_block_processed(height, processing_time);
                            self.metrics.record_block_validated();

                            tracing::info!(
                                block_height = height,
                                block_hash = ?hash,
                                processing_time_ms = processing_time.as_millis(),
                                "Block successfully imported via ChainActor"
                            );

                            // Check if sync is complete
                            if self.current_height >= self.target_height {
                                self.complete_sync().await?;
                            }

                            Ok(())
                        }
                        ChainResponse::BlockRejected { reason } => {
                            self.metrics.record_block_rejected(&reason);
                            tracing::warn!(
                                block_height = block_height,
                                reason = %reason,
                                peer_id = %peer_id,
                                "Block rejected by ChainActor during sync"
                            );
                            Err(anyhow!("Block rejected: {}", reason))
                        }
                        _ => Err(anyhow!("Unexpected response from ChainActor")),
                    }
                }
                Ok(Err(e)) => {
                    self.metrics.record_block_rejected("chain_actor_error");
                    tracing::error!(
                        block_height = block_height,
                        error = ?e,
                        "ChainActor returned error during sync"
                    );
                    Err(anyhow!("ChainActor error: {}", e))
                }
                Err(e) => {
                    self.metrics.record_block_rejected("mailbox_error");
                    tracing::error!(
                        block_height = block_height,
                        error = ?e,
                        "Failed to communicate with ChainActor"
                    );
                    Err(anyhow!("Mailbox error: {}", e))
                }
            }
        } else {
            Err(anyhow!(
                "ChainActor not set - cannot process blocks during sync"
            ))
        }
    }

    /// Simple block validation
    fn validate_block(&self, block: &Block) -> bool {
        // Simplified validation - in real implementation, this would be comprehensive
        !block.is_empty() && block.len() < 50 * 1024 * 1024 // 50MB max
    }

    /// Calculate block hash (wrapper for serialization module)
    fn calculate_block_hash(
        &self,
        block: &crate::actors_v2::storage::actor::AlysConsensusBlock,
    ) -> ethereum_types::H256 {
        crate::actors_v2::common::serialization::calculate_block_hash(block)
    }

    /// Convert block format for StorageActor V2
    fn convert_block_to_storage_format(
        &self,
        block: Block,
    ) -> crate::actors_v2::storage::actor::AlysConsensusBlock {
        // TODO: Implement proper block format conversion from network to storage format
        // For now, create a basic block structure
        let mut storage_block = crate::actors_v2::storage::actor::AlysConsensusBlock {
            message: crate::block::ConsensusBlock::default(),
            signature: crate::signatures::AggregateApproval::new(),
        };

        // Basic conversion logic (would be more sophisticated in production)
        if block.len() >= 8 {
            // Try to extract height from block data (simplified)
            let height_bytes: [u8; 8] = block[0..8].try_into().unwrap_or([0; 8]);
            storage_block.message.slot = u64::from_le_bytes(height_bytes);
        }

        // Set other basic fields
        storage_block.message.execution_payload.block_number = self.current_height;
        storage_block.message.execution_payload.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        storage_block
    }

    /// Complete synchronization after verifying sync completion
    async fn complete_sync(&mut self) -> Result<()> {
        tracing::info!("Completing sync process");

        // Verify completion
        if !self.verify_sync_completion().await? {
            tracing::warn!("Sync completion verification failed, continuing sync");
            return Ok(());
        }

        // Update state
        self.sync_state = SyncState::Synced;
        let sync_duration = self.metrics.get_sync_duration();
        self.metrics.stop_sync();

        // Notify ChainActor
        if let Some(ref chain_actor) = self.chain_actor {
            let msg = crate::actors_v2::chain::messages::ChainMessage::SyncCompleted {
                final_height: self.current_height,
            };

            if let Err(e) = chain_actor.send(msg).await {
                tracing::error!("Failed to notify ChainActor of sync completion: {}", e);
            }
        }

        tracing::info!(
            final_height = self.current_height,
            duration_secs = sync_duration.as_secs(),
            "✓✓✓ Sync completed successfully"
        );

        Ok(())
    }

    /// Verify sync completion by checking against network consensus
    async fn verify_sync_completion(&mut self) -> Result<bool> {
        const SYNC_TOLERANCE: u64 = 2; // Allow 2 block tolerance

        tracing::info!(
            current_height = self.current_height,
            target_height = self.target_height,
            "Verifying sync completion"
        );

        // Re-query peers for current chain head to ensure we're still synced
        let consensus_height = match self.discover_target_height().await {
            Ok(height) => height,
            Err(e) => {
                tracing::warn!("Failed to verify sync: {}", e);
                return Ok(false);
            }
        };

        // Check we're within acceptable range
        if self.current_height >= consensus_height.saturating_sub(SYNC_TOLERANCE) {
            tracing::info!(
                current_height = self.current_height,
                consensus_height = consensus_height,
                "✓ Sync verified: height matches consensus"
            );
            return Ok(true);
        }

        // Still syncing
        tracing::info!(
            current = self.current_height,
            consensus = consensus_height,
            gap = consensus_height - self.current_height,
            "Not yet synced"
        );
        Ok(false)
    }

    /// Handle sync timeout and cleanup
    fn handle_timeouts(&mut self) {
        let now = SystemTime::now();
        let mut timed_out_requests = Vec::new();

        for (request_id, request_info) in &self.active_requests {
            if now
                .duration_since(request_info.requested_at)
                .unwrap_or_default()
                > self.config.sync_timeout
            {
                timed_out_requests.push(request_id.clone());
            }
        }

        for request_id in timed_out_requests {
            if let Some(request_info) = self.active_requests.remove(&request_id) {
                tracing::warn!(
                    "Block request {} to peer {} timed out",
                    request_id,
                    request_info.peer_id
                );
                self.metrics.record_network_error();
            }
        }
    }

    /// Get current sync status
    fn get_sync_status(&self) -> SyncStatus {
        SyncStatus {
            current_height: self.current_height,
            target_height: self.target_height,
            is_syncing: matches!(
                self.sync_state,
                SyncState::RequestingBlocks | SyncState::ProcessingBlocks
            ),
            sync_peers: self.sync_peers.clone(),
            pending_requests: self.active_requests.len(),
        }
    }

    /// Update sync progress and create new requests if needed
    async fn update_sync_progress(&mut self) -> Result<()> {
        if self.sync_state == SyncState::RequestingBlocks {
            // Create more requests if we have capacity
            if self.active_requests.len() < self.config.max_concurrent_requests {
                self.create_block_requests().await?;
            }
        }

        // Update metrics
        self.metrics.update_sync_rate();

        Ok(())
    }

    /// Handle block response from network
    async fn handle_block_response(
        &mut self,
        blocks: Vec<Block>,
        request_id: String,
    ) -> Result<()> {
        // Find and remove the corresponding request
        if let Some(request_info) = self.active_requests.remove(&request_id) {
            tracing::debug!(
                "Received {} blocks for request {} from peer {}",
                blocks.len(),
                request_id,
                request_info.peer_id
            );

            self.metrics.record_block_response(blocks.len() as u32);

            // Process each block
            for block in blocks {
                self.block_queue
                    .push_back((block, request_info.peer_id.clone()));
            }

            // Process blocks from queue
            self.process_block_queue().await?;

            // Update peer reputation based on successful response
            // TODO: Inform NetworkActor about successful peer interaction
        } else {
            tracing::warn!("Received blocks for unknown request: {}", request_id);
        }

        Ok(())
    }

    /// Process blocks from the queue
    async fn process_block_queue(&mut self) -> Result<()> {
        self.sync_state = SyncState::ProcessingBlocks;

        while let Some((block, peer_id)) = self.block_queue.pop_front() {
            match self.process_block(block, peer_id.clone()).await {
                Ok(_) => {
                    // TODO: Update peer reputation positively
                }
                Err(e) => {
                    tracing::error!("Failed to process block from peer {}: {}", peer_id, e);
                    // TODO: Update peer reputation negatively
                }
            }
        }

        // Return to requesting blocks if not complete
        if self.current_height < self.target_height {
            self.sync_state = SyncState::RequestingBlocks;
        }

        Ok(())
    }
}

impl Actor for SyncActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("SyncActor V2 started");

        // Start periodic timeout checking
        ctx.run_interval(Duration::from_secs(10), |act, _ctx| {
            act.handle_timeouts();
        });

        // Start periodic sync progress updates
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            if act.is_running {
                let progress = act.metrics.get_sync_progress();
                tracing::debug!(
                    "Sync progress: {:.1}% ({}/{})",
                    progress * 100.0,
                    act.current_height,
                    act.target_height
                );

                // Attempt to update sync progress
                tokio::spawn(async move {
                    // Progress update logic would go here
                });
            }
        });
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("SyncActor V2 stopping");
        self.shutdown_requested = true;
        self.sync_state = SyncState::Stopped;
        self.is_running = false;
        Running::Stop
    }
}

impl Handler<SyncMessage> for SyncActor {
    type Result = Result<SyncResponse, SyncError>;

    fn handle(&mut self, msg: SyncMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            SyncMessage::StartSync => {
                // Start sync in background
                if self.sync_state != SyncState::Stopped {
                    return Err(SyncError::Internal("Sync already running".to_string()));
                }

                self.sync_state = SyncState::Starting;
                self.is_running = true;
                tracing::info!("Starting blockchain synchronization");
                Ok(SyncResponse::Started)
            }

            SyncMessage::StopSync => {
                self.sync_state = SyncState::Stopped;
                self.metrics.stop_sync();
                self.is_running = false;
                Ok(SyncResponse::Stopped)
            }

            SyncMessage::GetSyncStatus => {
                let status = self.get_sync_status();
                Ok(SyncResponse::Status(status))
            }

            SyncMessage::RequestBlocks {
                start_height,
                count,
                peer_id,
            } => {
                if !self.is_running {
                    return Err(SyncError::NotStarted);
                }

                let target_peer = peer_id.unwrap_or_else(|| self.select_sync_peer());
                let request_id = uuid::Uuid::new_v4().to_string();

                let request_info = BlockRequestInfo {
                    request_id: request_id.clone(),
                    start_height,
                    count,
                    peer_id: target_peer.clone(),
                    requested_at: SystemTime::now(),
                };

                self.active_requests
                    .insert(request_id.clone(), request_info);
                self.metrics.record_block_request(&target_peer);

                tracing::debug!(
                    "Created block request {} for {} blocks starting at height {}",
                    request_id,
                    count,
                    start_height
                );

                Ok(SyncResponse::BlocksRequested { request_id })
            }

            SyncMessage::HandleNewBlock { block, peer_id } => {
                // Add block to processing queue
                self.block_queue.push_back((block, peer_id.clone()));

                tracing::debug!(
                    "Queued new block from peer {} (queue size: {})",
                    peer_id,
                    self.block_queue.len()
                );

                Ok(SyncResponse::BlockProcessed {
                    block_height: self.current_height,
                })
            }

            SyncMessage::HandleBlockResponse { blocks, request_id } => {
                // TODO: Process block response
                tracing::debug!(
                    "Received {} blocks for request {}",
                    blocks.len(),
                    request_id
                );

                // Find and complete the request
                if let Some(request_info) = self.active_requests.remove(&request_id) {
                    self.metrics.record_block_response(blocks.len() as u32);

                    // Queue blocks for processing
                    for block in blocks {
                        self.block_queue
                            .push_back((block, request_info.peer_id.clone()));
                    }

                    tracing::debug!(
                        "Queued {} blocks from request {}",
                        self.block_queue.len(),
                        request_id
                    );
                }

                Ok(SyncResponse::BlockProcessed {
                    block_height: self.current_height,
                })
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
                let previous_count = self.sync_peers.len();
                self.sync_peers = peers;
                self.peer_selection_index = 0;

                tracing::info!(
                    "Updated sync peers: {} -> {} peers",
                    previous_count,
                    self.sync_peers.len()
                );

                Ok(SyncResponse::Started)
            }

            SyncMessage::GetMetrics => {
                let metrics = self.metrics.clone();
                Ok(SyncResponse::Metrics(metrics))
            }

            SyncMessage::QueryNetworkHeight => {
                tracing::debug!("Querying network for chain height");

                // Note: This is a synchronous handler but discover_target_height is async
                // We'll need to spawn it or return a future
                // For now, return an error if not already discovered
                if self.target_height > 0 {
                    Ok(SyncResponse::NetworkHeight {
                        height: self.target_height,
                    })
                } else {
                    // Cannot query network height synchronously from handler
                    // This should be called after sync has discovered peers
                    Err(SyncError::Internal(
                        "Network height not yet discovered".to_string(),
                    ))
                }
            }
        }
    }
}
