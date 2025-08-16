//! Sync actor for blockchain synchronization
//! 
//! This actor manages the synchronization process with remote peers,
//! handles block downloading, validation, and coordination with the chain actor.

use crate::messages::sync_messages::*;
use crate::types::*;
use actix::prelude::*;
use std::collections::{HashMap, VecDeque};
use tracing::*;

/// Sync actor that manages blockchain synchronization
#[derive(Debug)]
pub struct SyncActor {
    /// Sync configuration
    config: SyncConfig,
    /// Current sync status
    sync_status: SyncStatus,
    /// Connected peers and their capabilities
    peers: HashMap<PeerId, PeerInfo>,
    /// Queue of blocks to download
    download_queue: VecDeque<BlockRequest>,
    /// Blocks currently being downloaded
    pending_downloads: HashMap<BlockHash, DownloadInfo>,
    /// Actor metrics
    metrics: SyncActorMetrics,
}

/// Configuration for the sync actor
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Maximum number of blocks to request at once
    pub max_blocks_per_request: u64,
    /// Maximum number of concurrent downloads
    pub max_concurrent_downloads: usize,
    /// Timeout for block requests
    pub request_timeout: std::time::Duration,
    /// Target number of blocks ahead to sync
    pub sync_lookahead: u64,
}

/// Current synchronization status
#[derive(Debug, Clone)]
pub enum SyncStatus {
    /// Not syncing
    Idle,
    /// Initial sync in progress
    Syncing {
        current_block: u64,
        target_block: u64,
        progress: f64,
    },
    /// Catching up with recent blocks
    CatchingUp {
        blocks_behind: u64,
    },
    /// Fully synced and following chain head
    Synced,
}

/// Information about a connected peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub best_block: BlockRef,
    pub capabilities: PeerCapabilities,
    pub connection_quality: ConnectionQuality,
    pub last_seen: std::time::Instant,
}

/// Peer capabilities for sync
#[derive(Debug, Clone)]
pub struct PeerCapabilities {
    pub protocol_version: u32,
    pub supports_fast_sync: bool,
    pub max_block_request_size: u64,
}

/// Connection quality metrics
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    pub latency_ms: u64,
    pub bandwidth_mbps: f64,
    pub reliability_score: f64,
}

/// Request for downloading a block
#[derive(Debug, Clone)]
pub struct BlockRequest {
    pub block_hash: BlockHash,
    pub block_number: u64,
    pub priority: RequestPriority,
}

/// Priority levels for block requests
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Information about an ongoing download
#[derive(Debug, Clone)]
pub struct DownloadInfo {
    pub request: BlockRequest,
    pub peer_id: PeerId,
    pub started_at: std::time::Instant,
    pub attempts: u32,
}

/// Sync actor performance metrics
#[derive(Debug, Default)]
pub struct SyncActorMetrics {
    pub blocks_downloaded: u64,
    pub download_errors: u64,
    pub average_download_time_ms: u64,
    pub bytes_downloaded: u64,
    pub sync_restarts: u64,
}

impl Actor for SyncActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Sync actor started");
        
        // Start periodic sync health checks
        ctx.run_interval(
            std::time::Duration::from_secs(10),
            |actor, _ctx| {
                actor.check_sync_health();
            }
        );
        
        // Start periodic download timeout checks
        ctx.run_interval(
            std::time::Duration::from_secs(5),
            |actor, _ctx| {
                actor.check_download_timeouts();
            }
        );
        
        // Start metrics reporting
        ctx.run_interval(
            std::time::Duration::from_secs(60),
            |actor, _ctx| {
                actor.report_metrics();
            }
        );
    }
}

impl SyncActor {
    pub fn new(config: SyncConfig) -> Self {
        Self {
            config,
            sync_status: SyncStatus::Idle,
            peers: HashMap::new(),
            download_queue: VecDeque::new(),
            pending_downloads: HashMap::new(),
            metrics: SyncActorMetrics::default(),
        }
    }

    /// Add a new peer to the sync network
    async fn add_peer(&mut self, peer_info: PeerInfo) -> Result<(), SyncError> {
        info!("Adding sync peer: {}", peer_info.peer_id);
        
        self.peers.insert(peer_info.peer_id.clone(), peer_info.clone());
        
        // Check if we need to start syncing with this peer
        self.evaluate_sync_opportunity(&peer_info).await?;
        
        Ok(())
    }

    /// Remove a peer from the sync network
    async fn remove_peer(&mut self, peer_id: &PeerId) -> Result<(), SyncError> {
        info!("Removing sync peer: {}", peer_id);
        
        self.peers.remove(peer_id);
        
        // Cancel any pending downloads from this peer
        self.cancel_peer_downloads(peer_id);
        
        Ok(())
    }

    /// Evaluate whether to start syncing with a new peer
    async fn evaluate_sync_opportunity(&mut self, peer_info: &PeerInfo) -> Result<(), SyncError> {
        // Check if peer has blocks we need
        let should_sync = match &self.sync_status {
            SyncStatus::Idle => true,
            SyncStatus::Synced => {
                // Check if peer is ahead
                peer_info.best_block.number > self.get_current_head_number()
            }
            _ => false, // Already syncing
        };

        if should_sync {
            self.start_sync_with_peer(peer_info).await?;
        }

        Ok(())
    }

    /// Start synchronization process with a peer
    async fn start_sync_with_peer(&mut self, peer_info: &PeerInfo) -> Result<(), SyncError> {
        info!("Starting sync with peer {}", peer_info.peer_id);
        
        let current_head = self.get_current_head_number();
        let target_block = peer_info.best_block.number;
        
        if target_block > current_head {
            self.sync_status = SyncStatus::Syncing {
                current_block: current_head,
                target_block,
                progress: 0.0,
            };
            
            // Queue blocks for download
            self.queue_blocks_for_download(current_head + 1, target_block).await?;
            
            // Start downloading
            self.process_download_queue().await?;
        }
        
        Ok(())
    }

    /// Queue a range of blocks for download
    async fn queue_blocks_for_download(&mut self, start: u64, end: u64) -> Result<(), SyncError> {
        info!("Queuing blocks {} to {} for download", start, end);
        
        for block_number in start..=end {
            let request = BlockRequest {
                block_hash: BlockHash::default(), // Will be resolved during download
                block_number,
                priority: RequestPriority::Normal,
            };
            
            self.download_queue.push_back(request);
        }
        
        Ok(())
    }

    /// Process the download queue
    async fn process_download_queue(&mut self) -> Result<(), SyncError> {
        while self.pending_downloads.len() < self.config.max_concurrent_downloads {
            if let Some(request) = self.download_queue.pop_front() {
                self.start_block_download(request).await?;
            } else {
                break;
            }
        }
        
        Ok(())
    }

    /// Start downloading a specific block
    async fn start_block_download(&mut self, request: BlockRequest) -> Result<(), SyncError> {
        // Select best peer for this download
        let peer_id = self.select_download_peer(&request)?;
        
        info!("Starting download of block {} from peer {}", request.block_number, peer_id);
        
        let download_info = DownloadInfo {
            request: request.clone(),
            peer_id: peer_id.clone(),
            started_at: std::time::Instant::now(),
            attempts: 1,
        };
        
        self.pending_downloads.insert(request.block_hash, download_info);
        
        // TODO: Send actual download request to peer
        // This would involve sending a message to the network actor
        
        Ok(())
    }

    /// Select the best peer for downloading a block
    fn select_download_peer(&self, _request: &BlockRequest) -> Result<PeerId, SyncError> {
        // Simple implementation: select peer with best connection quality
        self.peers
            .iter()
            .max_by(|(_, a), (_, b)| {
                a.connection_quality.reliability_score
                    .partial_cmp(&b.connection_quality.reliability_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(peer_id, _)| peer_id.clone())
            .ok_or(SyncError::NoPeersAvailable)
    }

    /// Handle a downloaded block
    async fn handle_downloaded_block(&mut self, block: ConsensusBlock) -> Result<(), SyncError> {
        let block_hash = block.hash();
        info!("Received downloaded block: {}", block_hash);
        
        // Remove from pending downloads
        if let Some(download_info) = self.pending_downloads.remove(&block_hash) {
            let download_time = download_info.started_at.elapsed();
            self.metrics.average_download_time_ms = download_time.as_millis() as u64;
            self.metrics.blocks_downloaded += 1;
        }
        
        // TODO: Send block to chain actor for processing
        // This would involve sending a ProcessBlockMessage
        
        // Update sync progress
        self.update_sync_progress().await?;
        
        // Continue processing download queue
        self.process_download_queue().await?;
        
        Ok(())
    }

    /// Update sync progress based on current state
    async fn update_sync_progress(&mut self) -> Result<(), SyncError> {
        if let SyncStatus::Syncing { current_block, target_block, .. } = &mut self.sync_status {
            let new_current = self.get_current_head_number();
            *current_block = new_current;
            
            let progress = if *target_block > 0 {
                (new_current as f64) / (*target_block as f64)
            } else {
                0.0
            };
            
            if progress >= 1.0 {
                info!("Sync completed!");
                self.sync_status = SyncStatus::Synced;
            } else {
                self.sync_status = SyncStatus::Syncing {
                    current_block: new_current,
                    target_block: *target_block,
                    progress,
                };
            }
        }
        
        Ok(())
    }

    /// Get current head block number
    fn get_current_head_number(&self) -> u64 {
        // TODO: Query chain actor for current head
        0
    }

    /// Check sync health and detect issues
    fn check_sync_health(&mut self) {
        match &self.sync_status {
            SyncStatus::Syncing { progress, .. } => {
                debug!("Sync progress: {:.2}%", progress * 100.0);
            }
            SyncStatus::CatchingUp { blocks_behind } => {
                if *blocks_behind > 100 {
                    warn!("Falling behind: {} blocks", blocks_behind);
                }
            }
            _ => {}
        }
    }

    /// Check for and handle download timeouts
    fn check_download_timeouts(&mut self) {
        let now = std::time::Instant::now();
        let mut timed_out_downloads = Vec::new();
        
        for (block_hash, download_info) in &self.pending_downloads {
            if now.duration_since(download_info.started_at) > self.config.request_timeout {
                timed_out_downloads.push(block_hash.clone());
            }
        }
        
        for block_hash in timed_out_downloads {
            self.handle_download_timeout(block_hash);
        }
    }

    /// Handle a download timeout
    fn handle_download_timeout(&mut self, block_hash: BlockHash) {
        if let Some(mut download_info) = self.pending_downloads.remove(&block_hash) {
            error!("Download timeout for block {}", block_hash);
            self.metrics.download_errors += 1;
            
            // Retry if not too many attempts
            if download_info.attempts < 3 {
                download_info.attempts += 1;
                // Re-queue for download
                self.download_queue.push_front(download_info.request);
            }
        }
    }

    /// Cancel all downloads from a specific peer
    fn cancel_peer_downloads(&mut self, peer_id: &PeerId) {
        let mut cancelled_requests = Vec::new();
        
        self.pending_downloads.retain(|_, download_info| {
            if download_info.peer_id == *peer_id {
                cancelled_requests.push(download_info.request.clone());
                false
            } else {
                true
            }
        });
        
        // Re-queue cancelled requests
        for request in cancelled_requests {
            self.download_queue.push_front(request);
        }
    }

    /// Report sync metrics
    fn report_metrics(&self) {
        info!(
            "Sync metrics: blocks_downloaded={}, errors={}, avg_download_time={}ms",
            self.metrics.blocks_downloaded,
            self.metrics.download_errors,
            self.metrics.average_download_time_ms
        );
    }
}

// Message handlers

impl Handler<AddPeerMessage> for SyncActor {
    type Result = ResponseFuture<Result<(), SyncError>>;

    fn handle(&mut self, msg: AddPeerMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Received add peer request: {}", msg.peer_info.peer_id);
            Ok(())
        })
    }
}

impl Handler<RemovePeerMessage> for SyncActor {
    type Result = ResponseFuture<Result<(), SyncError>>;

    fn handle(&mut self, msg: RemovePeerMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Received remove peer request: {}", msg.peer_id);
            Ok(())
        })
    }
}

impl Handler<StartSyncMessage> for SyncActor {
    type Result = ResponseFuture<Result<(), SyncError>>;

    fn handle(&mut self, msg: StartSyncMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Received start sync request with target block {}", msg.target_block);
            Ok(())
        })
    }
}

impl Handler<BlockDownloadedMessage> for SyncActor {
    type Result = ResponseFuture<Result<(), SyncError>>;

    fn handle(&mut self, msg: BlockDownloadedMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Received block download completion: {}", msg.block.hash());
            Ok(())
        })
    }
}