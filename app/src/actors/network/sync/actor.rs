//! SyncActor Implementation
//! 
//! Core blockchain synchronization actor with 99.5% production threshold,
//! parallel validation, and checkpoint recovery capabilities.

use actix::{Actor, Context, Handler, Addr, AsyncContext, ResponseFuture, ResponseActFuture, ActorFutureExt, WrapFuture, ActorContext};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

use actor_system::{AlysActor, LifecycleAware, ActorResult, ActorError};
use actor_system::blockchain::{BlockchainAwareActor, BlockchainTimingConstraints, BlockchainActorPriority};

use crate::actors::network::messages::*;
use crate::actors::network::sync::*;
use crate::actors::chain::ChainActor;
use crate::actors::network::NetworkActor;
use crate::actors::network::PeerActor;

/// SyncActor for blockchain synchronization
pub struct SyncActor {
    /// Actor configuration
    config: SyncConfig,
    /// Current synchronization state
    state: SyncState,
    /// Block processing pipeline
    block_processor: BlockProcessor,
    /// Peer management for sync coordination
    peer_manager: PeerManager,
    /// Checkpoint management system
    checkpoint_manager: Option<CheckpointManager>,
    /// Performance metrics
    metrics: SyncMetrics,
    
    // Actor addresses for coordination
    chain_actor: Option<Addr<ChainActor>>,
    network_actor: Option<Addr<NetworkActor>>,
    peer_actor: Option<Addr<PeerActor>>,
    
    // Internal state
    sync_operations: HashMap<String, SyncOperation>,
    last_health_check: Instant,
    shutdown_requested: bool,
}

impl SyncActor {
    /// Create a new SyncActor with the given configuration
    pub fn new(config: SyncConfig) -> ActorResult<Self> {
        let block_processor = BlockProcessor::new(config.clone());
        let peer_manager = PeerManager::new(PeerManagerConfig::default());

        Ok(Self {
            config: config.clone(),
            state: SyncState::default(),
            block_processor,
            peer_manager,
            checkpoint_manager: None,
            metrics: SyncMetrics::default(),
            
            chain_actor: None,
            network_actor: None,
            peer_actor: None,
            
            sync_operations: HashMap::new(),
            last_health_check: Instant::now(),
            shutdown_requested: false,
        })
    }

    /// Initialize checkpoint manager
    pub async fn initialize_checkpoints(&mut self, storage_path: std::path::PathBuf) -> ActorResult<()> {
        let checkpoint_manager = CheckpointManager::new(
            storage_path,
            self.config.checkpoint_retention as u32,
            self.config.compression_enabled,
        ).await.map_err(|e| ActorError::InitializationError {
            reason: format!("Failed to initialize checkpoint manager: {:?}", e),
        })?;

        self.checkpoint_manager = Some(checkpoint_manager);
        tracing::info!("Checkpoint manager initialized");
        Ok(())
    }

    /// Set actor addresses for coordination
    pub fn set_actor_addresses(
        &mut self,
        chain_actor: Option<Addr<ChainActor>>,
        network_actor: Option<Addr<NetworkActor>>,
        peer_actor: Option<Addr<PeerActor>>,
    ) {
        self.chain_actor = chain_actor;
        self.network_actor = network_actor;
        self.peer_actor = peer_actor;
        
        tracing::debug!("Actor addresses configured for sync coordination");
    }

    /// Start sync operation with the given parameters
    async fn start_sync_operation(
        &mut self,
        from_height: Option<u64>,
        target_height: Option<u64>,
        mode: SyncMode,
        priority_peers: Vec<String>,
    ) -> NetworkResult<SyncResponse> {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = std::time::SystemTime::now();

        tracing::info!(
            "Starting sync operation {} from {:?} to {:?} in mode {:?}",
            operation_id, from_height, target_height, mode
        );

        // Update sync state
        self.state.progress.status = SyncStatus::Discovery;
        self.state.progress.mode = mode;

        // Create sync operation tracking
        let operation = SyncOperation {
            operation_id: operation_id.clone(),
            start_height: from_height.unwrap_or(0),
            end_height: target_height.unwrap_or(0),
            mode,
            started_at: Instant::now(),
            progress: 0.0,
            assigned_peers: priority_peers.clone(),
            blocks_downloaded: 0,
            blocks_validated: 0,
            blocks_applied: 0,
            status: SyncStatus::Discovery,
            error_count: 0,
        };

        self.sync_operations.insert(operation_id.clone(), operation);

        // Request peer information if needed
        if let Some(peer_actor) = &self.peer_actor {
            let get_peers_msg = GetBestPeers {
                count: self.config.max_parallel_downloads as u32,
                operation_type: OperationType::BlockSync,
                exclude_peers: vec![],
            };

            // Send async message to peer actor (fire and forget for now)
            peer_actor.do_send(get_peers_msg);
        }

        // Transition to downloading state
        self.state.progress.status = SyncStatus::Downloading;

        Ok(SyncResponse {
            operation_id,
            started_at: start_time,
            mode,
            initial_height: from_height.unwrap_or(0),
            target_height,
        })
    }

    /// Check if block production is allowed (99.5% threshold)
    fn can_produce_blocks(&self) -> bool {
        self.state.progress.can_produce_blocks && 
        self.state.progress.progress_percent >= self.config.production_threshold
    }

    /// Get current sync status with comprehensive information
    fn get_sync_status(&self) -> SyncStatus {
        let processing_queue = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.block_processor.get_queue_status().await
            })
        });

        SyncStatus {
            is_syncing: self.state.progress.status.is_active(),
            current_height: self.state.progress.current_height,
            target_height: self.state.progress.target_height,
            sync_progress: self.state.progress.progress_percent,
            blocks_per_second: self.state.metrics.current_bps,
            eta_seconds: self.state.progress.eta.map(|d| d.as_secs()),
            connected_peers: self.state.peer_state.active_sync_peers.len() as u32,
            active_downloads: processing_queue.processing_blocks as u32,
            validation_queue_size: processing_queue.queued_blocks as u32,
            can_produce_blocks: self.can_produce_blocks(),
            last_block_hash: None, // Would be populated from chain state
            sync_mode: self.state.progress.mode,
            checkpoint_info: Some(CheckpointInfo {
                last_checkpoint_height: 0, // Would be populated from checkpoint manager
                last_checkpoint_time: std::time::SystemTime::now(),
                available_checkpoints: 0,
                next_checkpoint_eta: None,
            }),
        }
    }

    /// Perform health check and maintenance
    async fn health_check(&mut self) -> ActorResult<()> {
        let now = Instant::now();
        
        // Check if health check interval has passed
        if now.duration_since(self.last_health_check) < self.config.health_check_interval {
            return Ok(());
        }

        self.last_health_check = now;

        // Check sync progress and performance
        let health_status = self.state.health_status();
        
        match health_status {
            SyncHealthStatus::Unhealthy => {
                tracing::warn!("Sync health is unhealthy, attempting recovery");
                // Attempt to recover sync operation
                if let Some(last_operation) = self.sync_operations.values().last() {
                    if last_operation.error_count < self.config.max_retries {
                        // Retry sync operation
                        self.state.progress.status = SyncStatus::Recovery;
                    }
                }
            }
            SyncHealthStatus::Degraded => {
                tracing::info!("Sync performance is degraded, optimizing");
                // Implement performance optimization logic
            }
            _ => {}
        }

        // Update metrics
        self.metrics.total_blocks_synced = self.state.progress.current_height;

        Ok(())
    }

    /// Create checkpoint if conditions are met
    async fn maybe_create_checkpoint(&mut self) -> ActorResult<()> {
        if let Some(ref mut checkpoint_manager) = self.checkpoint_manager {
            let current_height = self.state.progress.current_height;
            
            // Check if checkpoint should be created
            if current_height > 0 && current_height % self.config.checkpoint_interval == 0 {
                tracing::info!("Creating checkpoint at height {}", current_height);

                // Create chain state for checkpoint
                let chain_state = ChainState {
                    height: current_height,
                    state_root: ethereum_types::H256::random(), // Would get from chain
                    block_hashes: vec![(current_height, ethereum_types::H256::random())],
                    peer_states: HashMap::new(), // Would populate from peer manager
                    federation_state: FederationCheckpointState {
                        current_authorities: vec!["authority1".to_string()],
                        current_slot: current_height / 2,
                        last_finalized_block: current_height - 1,
                        emergency_mode: false,
                    },
                    block_count: current_height,
                    metadata: HashMap::new(),
                };

                match checkpoint_manager.create_checkpoint(current_height, chain_state).await {
                    Ok(response) => {
                        tracing::info!("Created checkpoint {} successfully", response.checkpoint_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to create checkpoint: {:?}", e);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Actor for SyncActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("SyncActor started with config: production_threshold = {}", self.config.production_threshold);

        // Schedule periodic health checks
        ctx.run_interval(self.config.health_check_interval, |actor, _ctx| {
            let health_check_future = actor.health_check();
            let actor_future = async move {
                if let Err(e) = health_check_future.await {
                    tracing::error!("Health check failed: {:?}", e);
                }
            }.into_actor(actor);
            
            ctx.spawn(actor_future);
        });

        // Schedule periodic checkpoint creation
        if self.config.checkpoint_interval > 0 {
            ctx.run_interval(Duration::from_secs(60), |actor, _ctx| {
                let checkpoint_future = actor.maybe_create_checkpoint();
                let actor_future = async move {
                    if let Err(e) = checkpoint_future.await {
                        tracing::error!("Checkpoint creation failed: {:?}", e);
                    }
                }.into_actor(actor);
                
                ctx.spawn(actor_future);
            });
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("SyncActor stopped");
    }
}

impl AlysActor for SyncActor {
    fn actor_type(&self) -> &'static str {
        "SyncActor"
    }

    fn metrics(&self) -> serde_json::Value {
        serde_json::json!({
            "current_height": self.state.progress.current_height,
            "sync_progress": self.state.progress.progress_percent,
            "can_produce_blocks": self.can_produce_blocks(),
            "blocks_per_second": self.state.metrics.current_bps,
            "active_peers": self.state.peer_state.active_sync_peers.len(),
            "sync_status": format!("{:?}", self.state.progress.status),
            "health_status": format!("{:?}", self.state.health_status()),
        })
    }
}

impl LifecycleAware for SyncActor {
    fn on_start(&mut self) -> ActorResult<()> {
        tracing::info!("SyncActor lifecycle started");
        Ok(())
    }

    fn on_stop(&mut self) -> ActorResult<()> {
        self.shutdown_requested = true;
        tracing::info!("SyncActor lifecycle stopped");
        Ok(())
    }

    fn health_check(&self) -> ActorResult<()> {
        if self.shutdown_requested {
            return Err(ActorError::ActorStopped);
        }

        let health_status = self.state.health_status();
        match health_status {
            SyncHealthStatus::Unhealthy => Err(ActorError::HealthCheckFailed {
                reason: "Sync is in unhealthy state".to_string(),
            }),
            _ => Ok(()),
        }
    }
}

impl BlockchainAwareActor for SyncActor {
    fn timing_constraints(&self) -> BlockchainTimingConstraints {
        BlockchainTimingConstraints {
            max_processing_time: self.config.request_timeout,
            federation_timeout: self.config.federation_constraints.consensus_timeout,
            emergency_timeout: self.config.federation_constraints.emergency_timeout,
        }
    }

    fn federation_config(&self) -> Option<actor_system::blockchain::FederationConfig> {
        Some(actor_system::blockchain::FederationConfig {
            consensus_threshold: 0.67, // 2/3 majority
            max_authorities: 21,
            slot_duration: self.config.aura_slot_duration,
        })
    }

    fn blockchain_priority(&self) -> BlockchainActorPriority {
        BlockchainActorPriority::High // Sync is critical for blockchain operation
    }
}

// Message Handlers

impl Handler<StartSync> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<SyncResponse>>;

    fn handle(&mut self, msg: StartSync, _ctx: &mut Context<Self>) -> Self::Result {
        let mut actor = self.clone_for_async();
        
        Box::pin(async move {
            match actor.start_sync_operation(
                msg.from_height,
                msg.target_height,
                msg.sync_mode,
                msg.priority_peers,
            ).await {
                Ok(response) => Ok(Ok(response)),
                Err(error) => Ok(Err(error)),
            }
        })
    }
}

impl Handler<StopSync> for SyncActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: StopSync, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Stopping sync operations (force: {})", msg.force);
        
        if msg.force {
            // Force stop all operations immediately
            self.sync_operations.clear();
            self.state.progress.status = SyncStatus::Idle;
        } else {
            // Graceful stop - let current operations complete
            self.state.progress.status = SyncStatus::Idle;
        }

        Ok(Ok(()))
    }
}

impl Handler<CanProduceBlocks> for SyncActor {
    type Result = NetworkActorResult<bool>;

    fn handle(&mut self, _msg: CanProduceBlocks, _ctx: &mut Context<Self>) -> Self::Result {
        let can_produce = self.can_produce_blocks();
        tracing::debug!("Block production check: {} (progress: {:.2}%)", 
            can_produce, self.state.progress.progress_percent * 100.0);
        Ok(Ok(can_produce))
    }
}

impl Handler<GetSyncStatus> for SyncActor {
    type Result = NetworkActorResult<SyncStatus>;

    fn handle(&mut self, _msg: GetSyncStatus, _ctx: &mut Context<Self>) -> Self::Result {
        let status = self.get_sync_status();
        Ok(Ok(status))
    }
}

impl Handler<CreateCheckpoint> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<CheckpointResponse>>;

    fn handle(&mut self, msg: CreateCheckpoint, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        let current_height = msg.height.unwrap_or(self.state.progress.current_height);
        
        Box::pin(async move {
            if let Some(mut checkpoint_manager) = checkpoint_manager {
                // Create minimal chain state for checkpoint
                let chain_state = ChainState {
                    height: current_height,
                    state_root: ethereum_types::H256::random(),
                    block_hashes: vec![(current_height, ethereum_types::H256::random())],
                    peer_states: HashMap::new(),
                    federation_state: FederationCheckpointState {
                        current_authorities: vec!["authority1".to_string()],
                        current_slot: current_height / 2,
                        last_finalized_block: current_height - 1,
                        emergency_mode: false,
                    },
                    block_count: current_height,
                    metadata: HashMap::new(),
                };

                match checkpoint_manager.create_checkpoint(current_height, chain_state).await {
                    Ok(response) => Ok(Ok(response)),
                    Err(error) => Ok(Err(error)),
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

impl Handler<RestoreCheckpoint> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<RestoreResponse>>;

    fn handle(&mut self, msg: RestoreCheckpoint, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        
        Box::pin(async move {
            if let Some(checkpoint_manager) = checkpoint_manager {
                match checkpoint_manager.restore_checkpoint(&msg.checkpoint_id, msg.verify_integrity).await {
                    Ok((_chain_state, restore_response)) => Ok(Ok(restore_response)),
                    Err(error) => Ok(Err(error)),
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

// Internal implementation for async operations
impl SyncActor {
    /// Clone actor state for async operations (avoiding full clone)
    fn clone_for_async(&self) -> SyncActor {
        SyncActor {
            config: self.config.clone(),
            state: self.state.clone(),
            block_processor: BlockProcessor::new(self.config.clone()), // Create new processor
            peer_manager: PeerManager::new(PeerManagerConfig::default()), // Create new manager
            checkpoint_manager: None, // Don't clone heavy checkpoint manager
            metrics: self.metrics.clone(),
            chain_actor: self.chain_actor.clone(),
            network_actor: self.network_actor.clone(),
            peer_actor: self.peer_actor.clone(),
            sync_operations: HashMap::new(), // Don't clone active operations
            last_health_check: self.last_health_check,
            shutdown_requested: self.shutdown_requested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;

    #[actix::test]
    async fn sync_actor_creation() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap();
        assert_eq!(sync_actor.actor_type(), "SyncActor");
    }

    #[actix::test]
    async fn sync_actor_lifecycle() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        assert!(sync_actor.on_start().is_ok());
        assert!(sync_actor.health_check().is_ok());
        assert!(sync_actor.on_stop().is_ok());
    }

    #[actix::test]
    async fn production_threshold_check() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        // Below threshold
        sync_actor.state.progress.progress_percent = 0.994;
        assert!(!sync_actor.can_produce_blocks());
        
        // At threshold
        sync_actor.state.progress.progress_percent = 0.995;
        sync_actor.state.progress.can_produce_blocks = true;
        assert!(sync_actor.can_produce_blocks());
    }

    #[actix::test]
    async fn sync_status_response() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap();
        
        let status = sync_actor.get_sync_status();
        assert_eq!(status.current_height, 0);
        assert!(!status.is_syncing);
        assert!(!status.can_produce_blocks);
    }
}