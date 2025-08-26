//! Core ChainActor Implementation
//!
//! This module contains the main ChainActor struct and its core implementation
//! including Actor trait implementations, startup/shutdown logic, and timers.
//! The ChainActor manages blockchain consensus, block production, and chain state.

use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use tracing::*;
use actix::prelude::*;

// Import from our organized modules
use super::{
    config::ChainActorConfig,
    state::*,
    messages::*,
    metrics::ChainActorMetrics,
};

// Import types from the broader application
use crate::types::*;
use crate::features::{FeatureFlagManager, FeatureFlag};
use crate::integration::*;

// Enhanced actor system integration
use actor_system::prelude::*;
use actor_system::{
    BlockchainAwareActor, BlockchainActorPriority, BlockchainTimingConstraints,
    BlockchainEvent, BlockchainReadiness, SyncStatus, FederationConfig as ActorFederationConfig
};

/// ChainActor that manages blockchain consensus, block production, and chain state
/// 
/// This actor implements the core blockchain functionality using the actor model
/// to replace shared mutable state patterns with message-driven operations.
/// It integrates with the Alys V2 actor foundation system for supervision,
/// health monitoring, and graceful shutdown.
#[derive(Debug)]
pub struct ChainActor {
    /// Actor configuration
    pub config: ChainActorConfig,
    
    /// Current chain state (owned by actor, no sharing)
    pub chain_state: ChainState,
    
    /// Pending blocks awaiting processing or validation
    pub pending_blocks: HashMap<Hash256, PendingBlockInfo>,
    
    /// Block candidate queue for production
    pub block_candidates: VecDeque<BlockCandidate>,
    
    /// Federation configuration and state
    pub federation: FederationState,
    
    /// Auxiliary PoW state for Bitcoin merged mining
    pub auxpow_state: AuxPowState,
    
    /// Subscriber management for block notifications
    pub subscribers: HashMap<Uuid, BlockSubscriber>,
    
    /// Performance metrics and monitoring
    pub metrics: ChainActorMetrics,
    
    /// Feature flag manager for gradual rollout
    pub feature_flags: Arc<FeatureFlagManager>,
    
    /// Integration with other actors
    pub actor_addresses: ActorAddresses,
    
    /// Validation result cache
    pub validation_cache: ValidationCache,
    
    /// Actor health monitoring
    pub health_monitor: ActorHealthMonitor,
    
    /// Distributed tracing context
    pub trace_context: TraceContext,
    
    /// Block production state
    pub production_state: BlockProductionState,
    
    /// Network broadcast tracking
    pub broadcast_tracker: BroadcastTracker,
}

impl Actor for ChainActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            actor_id = %ctx.address().recipient::<ImportBlock>(),
            "ChainActor started with head at height {}",
            self.chain_state.height
        );

        // Start periodic block production if we're a validator
        if self.config.is_validator {
            self.start_block_production_timer(ctx);
        }

        // Start finalization checker
        self.start_finalization_checker(ctx);

        // Start metrics reporting
        self.start_metrics_reporting(ctx);
        
        // Start health monitoring for supervision
        self.start_health_monitoring(ctx);

        // Register with supervisor
        self.register_with_supervisor(ctx);

        // Update metrics
        self.metrics.update_queue_depths(
            self.pending_blocks.len(),
            self.block_candidates.len(),
            0, // validation queue
            0, // notification queue
        );
        
        // Record actor startup
        self.metrics.record_actor_started();
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        info!(
            blocks_produced = self.metrics.blocks_produced,
            blocks_imported = self.metrics.blocks_imported,
            "ChainActor stopping gracefully"
        );
        
        // Record actor shutdown
        self.metrics.record_actor_stopped();
        
        Running::Stop
    }
}

impl ChainActor {
    /// Create a new ChainActor with the given configuration
    pub fn new(
        config: ChainActorConfig,
        actor_addresses: ActorAddresses,
        feature_flags: Arc<FeatureFlagManager>,
    ) -> Result<Self, ChainError> {
        let genesis = BlockRef::genesis(Hash256::zero());
        
        // Initialize chain state
        let chain_state = ChainState::new(genesis.clone());
        
        // Initialize federation state
        let federation_config = config.federation_config.clone();
        let federation = FederationState::new(federation_config);
        
        // Initialize auxiliary PoW state
        let auxpow_state = AuxPowState::new();
        
        // Initialize metrics
        let mut metrics = ChainActorMetrics::new();
        
        // Initialize validation cache
        let validation_cache = ValidationCache::new(config.validation_cache_size);
        
        // Initialize health monitor
        let health_monitor = ActorHealthMonitor::new("ChainActor".to_string());
        
        Ok(Self {
            config,
            chain_state,
            pending_blocks: HashMap::new(),
            block_candidates: VecDeque::new(),
            federation,
            auxpow_state,
            subscribers: HashMap::new(),
            metrics,
            feature_flags,
            actor_addresses,
            validation_cache,
            health_monitor,
            trace_context: TraceContext::default(),
            production_state: BlockProductionState::default(),
            broadcast_tracker: BroadcastTracker::default(),
        })
    }
    
    /// Start the block production timer for validator nodes
    fn start_block_production_timer(&self, ctx: &mut Context<Self>) {
        let slot_duration = self.config.slot_duration;
        
        ctx.run_interval(slot_duration, move |act, ctx| {
            if act.production_state.paused {
                return;
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            
            let slot = now.as_secs() / slot_duration.as_secs();
            
            // Send produce block message to ourselves
            let msg = ProduceBlock::new(slot, now);
            ctx.notify(msg);
        });
    }

    /// Start the finalization checker timer
    fn start_finalization_checker(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(10), |act, ctx| {
            ctx.spawn(
                async move {
                    act.check_finalization().await
                }
                .into_actor(act)
                .map(|result, act, _| {
                    if let Err(e) = result {
                        error!("Finalization check failed: {}", e);
                        act.metrics.record_consensus_failure();
                    }
                })
            );
        });
    }

    /// Start metrics reporting timer
    fn start_metrics_reporting(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(60), |act, _| {
            act.report_metrics();
        });
    }

    /// Start health monitoring timer
    fn start_health_monitoring(&self, ctx: &mut Context<Self>) {
        let interval = self.health_monitor.health_check_interval;
        
        ctx.run_interval(interval, |act, ctx| {
            act.perform_health_check(ctx);
        });
    }

    /// Register with the root supervisor
    fn register_with_supervisor(&self, ctx: &mut Context<Self>) {
        let supervisor = &self.actor_addresses.supervisor;
        let self_addr = ctx.address();
        
        info!(
            actor_name = "ChainActor",
            health_check_interval = ?self.health_monitor.health_check_interval,
            "Registering ChainActor with supervision system"
        );
        
        // Register with supervisor for health monitoring and lifecycle management
        supervisor.do_send(RegisterActor {
            name: "ChainActor".to_string(),
            address: self_addr.clone().recipient(),
            health_check_interval: self.health_monitor.health_check_interval,
        });

        // TODO: Add additional supervision metadata like:
        // - Actor priority (Critical for ChainActor)
        // - Restart strategy (Immediate restart on failure)
        // - Escalation rules (Notify operator on repeated failures)
        // - Dependency actors (Engine, Storage, Network, Bridge actors)
        // - Performance thresholds for supervision alerts
        
        debug!("ChainActor successfully registered with supervision system");
    }

    /// Calculate the current slot based on system time
    fn calculate_current_slot(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        now.as_secs() / self.config.slot_duration.as_secs()
    }

    /// Check if this node should produce a block for the given slot
    pub fn should_produce_block(&self, slot: u64) -> bool {
        // Placeholder implementation - in real system would check authority schedule
        if !self.config.is_validator {
            return false;
        }

        if self.production_state.paused {
            return false;
        }

        // Simple round-robin for demo - real implementation would use proper authority rotation
        if self.federation.members.is_empty() {
            return false;
        }
        
        let authority_index = slot % self.federation.members.len() as u64;
        
        // Check if we are the designated authority for this slot
        if let Some(authority_key) = &self.config.authority_key {
            if let Some(member) = self.federation.members.get(authority_index as usize) {
                return member.public_key == authority_key.public_key();
            }
        }

        false
    }

    /// Check for blocks that need finalization
    async fn check_finalization(&mut self) -> Result<(), ChainError> {
        if let Some(pow_header) = &self.chain_state.pending_pow {
            let pow_height = pow_header.height;
            
            // Check if PoW confirms our current head
            if self.chain_state.height >= pow_height {
                info!(
                    pow_height = pow_height,
                    current_height = self.chain_state.height,
                    "Finalizing blocks with AuxPoW"
                );
                
                // Update finalized block
                self.chain_state.finalized = self.chain_state.head.clone();
                
                // Clear pending PoW
                self.chain_state.pending_pow = None;
                
                // Notify subscribers
                self.notify_finalization(pow_height).await?;
                
                return Ok(());
            }
        }

        // Check if we need to halt due to no PoW
        if let Some(finalized) = &self.chain_state.finalized {
            let blocks_since_finalized = self.chain_state.height - finalized.number;
            if blocks_since_finalized > self.config.max_blocks_without_pow {
                warn!(
                    blocks_since_finalized = blocks_since_finalized,
                    max_allowed = self.config.max_blocks_without_pow,
                    "Halting block production due to lack of PoW"
                );
                
                self.production_state.paused = true;
                self.production_state.pause_reason = Some(
                    "No auxiliary proof-of-work received within timeout".to_string()
                );
            }
        }

        Ok(())
    }

    /// Notify subscribers about block finalization
    async fn notify_finalization(&self, finalized_height: u64) -> Result<(), ChainError> {
        // Implementation would notify all subscribers about finalization
        debug!(finalized_height = finalized_height, "Notifying finalization");
        Ok(())
    }

    /// Report performance metrics
    fn report_metrics(&mut self) {
        let snapshot = self.metrics.snapshot();
        
        info!(
            blocks_produced = snapshot.blocks_produced,
            blocks_imported = snapshot.blocks_imported,
            queue_size = snapshot.queue_depths.pending_blocks,
            avg_production_ms = snapshot.avg_production_time_ms,
            avg_import_ms = snapshot.avg_import_time_ms,
            total_errors = snapshot.total_errors,
            "ChainActor performance metrics"
        );

        // Update queue depth tracking
        self.metrics.update_queue_depths(
            self.pending_blocks.len(),
            self.block_candidates.len(),
            0, // validation queue
            0, // notification queue
        );
        
        // Check for performance violations
        self.check_performance_violations();
    }

    /// Check for performance violations
    fn check_performance_violations(&mut self) {
        let targets = &self.config.performance_targets;
        let snapshot = self.metrics.snapshot();
        
        if snapshot.avg_production_time_ms > targets.max_production_time_ms as f64 {
            warn!("Block production time exceeded target: {:.2}ms > {}ms",
                snapshot.avg_production_time_ms, targets.max_production_time_ms);
        }
        
        if snapshot.avg_import_time_ms > targets.max_import_time_ms as f64 {
            warn!("Block import time exceeded target: {:.2}ms > {}ms",
                snapshot.avg_import_time_ms, targets.max_import_time_ms);
        }
    }

    /// Perform health check
    fn perform_health_check(&mut self, _ctx: &mut Context<Self>) {
        let now = Instant::now();
        let mut score = 100u8;

        // Check queue depths
        if self.pending_blocks.len() > self.config.max_pending_blocks {
            score = score.saturating_sub(20);
        }

        // Check recent performance
        let snapshot = self.metrics.snapshot();
        if snapshot.avg_production_time_ms > self.config.performance_targets.max_production_time_ms as f64 {
            score = score.saturating_sub(15);
        }

        if snapshot.avg_import_time_ms > self.config.performance_targets.max_import_time_ms as f64 {
            score = score.saturating_sub(15);
        }

        // Check error rates
        if snapshot.total_errors > 10 {
            score = score.saturating_sub(25);
        }

        // Update health status
        self.health_monitor.status.system_health = score;
        self.health_monitor.recent_scores.push_back(score);
        if self.health_monitor.recent_scores.len() > 10 {
            self.health_monitor.recent_scores.pop_front();
        }
        
        self.health_monitor.last_health_check = now;

        if score < 50 {
            warn!(health_score = score, "ChainActor health degraded");
        }
    }
}

/// Message for actor registration with supervisor
#[derive(Message)]
#[rtype(result = "()")]
struct RegisterActor {
    name: String,
    address: Recipient<HealthCheck>,
    health_check_interval: Duration,
}

/// Health check message for supervision
#[derive(Message)]
#[rtype(result = "HealthCheckResult")]
struct HealthCheck;

/// Health check result
#[derive(Debug)]
struct HealthCheckResult {
    healthy: bool,
    score: u8,
    details: String,
}

// Additional message handlers for remaining ChainActor operations
impl ChainActor {
    /// Handle request for blocks in a specific range
    pub async fn handle_get_blocks_by_range(&mut self, msg: GetBlocksByRange) -> Result<Vec<SignedConsensusBlock>, ChainError> {
        debug!(
            start_height = msg.start_height,
            count = msg.count,
            include_body = msg.include_body,
            "Retrieving blocks by range"
        );

        let mut blocks = Vec::new();
        let end_height = msg.start_height + msg.count as u64;
        let actual_end = std::cmp::min(end_height, self.chain_state.height + 1);

        for height in msg.start_height..actual_end {
            // In real implementation, would fetch from storage
            if let Some(block) = self.get_block_by_height(height).await? {
                blocks.push(block);
                
                // Check response size limit
                if let Some(max_size) = msg.max_response_size {
                    let estimated_size = blocks.len() * 1000; // Rough estimate
                    if estimated_size >= max_size {
                        break;
                    }
                }
            }
        }

        debug!(
            blocks_returned = blocks.len(),
            requested_count = msg.count,
            "Retrieved blocks by range"
        );

        Ok(blocks)
    }

    /// Handle block broadcast request
    pub async fn handle_broadcast_block(&mut self, msg: BroadcastBlock) -> Result<BroadcastResult, ChainError> {
        let start_time = Instant::now();
        let block_hash = msg.block.message.hash();

        info!(
            block_hash = %block_hash,
            priority = ?msg.priority,
            exclude_peers = msg.exclude_peers.len(),
            "Broadcasting block to network"
        );

        // Update broadcast tracker
        self.broadcast_tracker.add_broadcast(
            block_hash,
            msg.priority,
            msg.exclude_peers.clone(),
            start_time,
        );

        // In real implementation, would use network actor to broadcast
        let result = self.perform_block_broadcast(&msg).await?;

        // Record metrics
        let broadcast_time = start_time.elapsed();
        self.metrics.record_block_broadcast(broadcast_time, result.successful_sends > 0);

        info!(
            block_hash = %block_hash,
            peers_reached = result.peers_reached,
            successful_sends = result.successful_sends,
            broadcast_time_ms = broadcast_time.as_millis(),
            "Block broadcast completed"
        );

        Ok(result)
    }

    /// Handle block subscription request
    pub async fn handle_subscribe_blocks(&mut self, msg: SubscribeBlocks) -> Result<(), ChainError> {
        let subscription_id = Uuid::new_v4();
        
        info!(
            subscription_id = %subscription_id,
            event_types = ?msg.event_types,
            "Adding block subscription"
        );

        let subscriber = BlockSubscriber {
            recipient: msg.subscriber,
            event_types: msg.event_types.into_iter().collect(),
            filter: msg.filter,
            subscribed_at: SystemTime::now(),
            messages_sent: 0,
        };

        self.subscribers.insert(subscription_id, subscriber);

        debug!(
            total_subscribers = self.subscribers.len(),
            "Block subscription added"
        );

        Ok(())
    }

    /// Handle chain metrics request
    pub async fn handle_get_chain_metrics(&mut self, msg: GetChainMetrics) -> Result<ChainMetrics, ChainError> {
        debug!(
            include_details = msg.include_details,
            time_window = ?msg.time_window,
            "Retrieving chain metrics"
        );

        let metrics = self.calculate_chain_metrics(msg.time_window).await?;

        if msg.include_details {
            debug!(
                blocks_produced = metrics.blocks_produced,
                blocks_imported = metrics.blocks_imported,
                avg_production_time = metrics.avg_production_time_ms,
                "Detailed chain metrics calculated"
            );
        }

        Ok(metrics)
    }

    /// Handle chain state query
    pub async fn handle_query_chain_state(&mut self, msg: QueryChainState) -> Result<ChainStateQuery, ChainError> {
        let start_time = Instant::now();
        
        // Determine target block
        let target_block = if let Some(hash) = msg.block_hash {
            self.get_block_by_hash(hash).await?
        } else if let Some(height) = msg.block_height {
            self.get_block_by_height(height).await?
        } else {
            self.chain_state.head.clone()
                .and_then(|head| Some(SignedConsensusBlock::from_block_ref(&head)))
        };

        let block_ref = target_block
            .as_ref()
            .map(BlockRef::from_block)
            .ok_or(ChainError::BlockNotFound)?;

        // Collect requested state information
        let mut state_info = std::collections::HashMap::new();
        
        for info_type in msg.include_info {
            let value = self.get_state_info(&target_block, info_type).await?;
            state_info.insert(info_type, value);
        }

        let processing_time = start_time.elapsed().as_millis() as u64;

        debug!(
            block_hash = %block_ref.hash,
            block_height = block_ref.number,
            info_types = state_info.len(),
            processing_time_ms = processing_time,
            "Chain state query completed"
        );

        Ok(ChainStateQuery {
            block_ref,
            state_info,
            processing_time_ms: processing_time,
        })
    }

    // Helper methods for the handlers

    async fn get_block_by_height(&self, height: u64) -> Result<Option<SignedConsensusBlock>, ChainError> {
        // Implementation would fetch from storage actor
        debug!(height = height, "Fetching block by height");
        Ok(None) // Placeholder
    }

    async fn get_block_by_hash(&self, hash: Hash256) -> Result<Option<SignedConsensusBlock>, ChainError> {
        // Implementation would fetch from storage actor
        debug!(hash = %hash, "Fetching block by hash");
        Ok(None) // Placeholder
    }

    async fn perform_block_broadcast(&mut self, msg: &BroadcastBlock) -> Result<BroadcastResult, ChainError> {
        // Implementation would use network actor to broadcast to peers
        // For now, return simulated success
        Ok(BroadcastResult {
            peers_reached: 10,
            successful_sends: 9,
            failed_sends: 1,
            avg_response_time_ms: Some(50),
            failed_peers: vec![], // Would contain actual failed peer IDs
        })
    }

    async fn calculate_chain_metrics(&self, _time_window: Option<Duration>) -> Result<ChainMetrics, ChainError> {
        let snapshot = self.metrics.snapshot();
        
        Ok(ChainMetrics {
            blocks_produced: snapshot.blocks_produced,
            blocks_imported: snapshot.blocks_imported,
            avg_production_time_ms: snapshot.avg_production_time_ms,
            avg_import_time_ms: snapshot.avg_import_time_ms,
            reorg_count: 0, // Would track from reorg manager
            avg_reorg_depth: 0.0,
            pegins_processed: 0, // Would get from peg manager
            pegouts_processed: 0,
            total_peg_value_sats: 0,
            validation_failures: snapshot.total_errors,
            broadcast_success_rate: 95.0, // Would calculate from broadcast tracker
            memory_stats: MemoryStats::default(),
        })
    }

    async fn get_state_info(
        &self, 
        _target_block: &Option<SignedConsensusBlock>, 
        info_type: StateInfoType
    ) -> Result<serde_json::Value, ChainError> {
        // Implementation would extract specific state information
        match info_type {
            StateInfoType::Header => Ok(serde_json::json!({"type": "header"})),
            StateInfoType::Transactions => Ok(serde_json::json!({"tx_count": 0})),
            StateInfoType::PegOperations => Ok(serde_json::json!({"pegins": 0, "pegouts": 0})),
            StateInfoType::Validation => Ok(serde_json::json!({"is_valid": true})),
            StateInfoType::Network => Ok(serde_json::json!({"peers": 0})),
        }
    }
}

/// Handler implementations for the additional Actix messages
impl Handler<GetBlocksByRange> for ChainActor {
    type Result = ResponseActFuture<Self, Result<Vec<SignedConsensusBlock>, ChainError>>;

    fn handle(&mut self, msg: GetBlocksByRange, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_get_blocks_by_range(msg).await
        }.into_actor(self))
    }
}

impl Handler<BroadcastBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<BroadcastResult, ChainError>>;

    fn handle(&mut self, msg: BroadcastBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_broadcast_block(msg).await
        }.into_actor(self))
    }
}

impl Handler<SubscribeBlocks> for ChainActor {
    type Result = ResponseActFuture<Self, Result<(), ChainError>>;

    fn handle(&mut self, msg: SubscribeBlocks, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_subscribe_blocks(msg).await
        }.into_actor(self))
    }
}

impl Handler<GetChainMetrics> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ChainMetrics, ChainError>>;

    fn handle(&mut self, msg: GetChainMetrics, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_get_chain_metrics(msg).await
        }.into_actor(self))
    }
}

impl Handler<QueryChainState> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ChainStateQuery, ChainError>>;

    fn handle(&mut self, msg: QueryChainState, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            self.handle_query_chain_state(msg).await
        }.into_actor(self))
    }
}

/// Handler for health check requests from the supervision system
impl Handler<HealthCheck> for ChainActor {
    type Result = HealthCheckResult;

    fn handle(&mut self, _msg: HealthCheck, ctx: &mut Context<Self>) -> Self::Result {
        // Perform comprehensive health check
        self.perform_health_check(ctx);
        
        // Get the latest health score
        let score = self.health_monitor.recent_scores.back().cloned().unwrap_or(0);
        let healthy = score >= 50; // Consider healthy if score is 50 or above
        
        let details = format!(
            "Chain height: {}, pending blocks: {}, health score: {}",
            self.chain_state.height,
            self.pending_blocks.len(),
            score
        );

        debug!(
            health_score = score,
            healthy = healthy,
            chain_height = self.chain_state.height,
            pending_blocks = self.pending_blocks.len(),
            "Health check completed"
        );
        
        HealthCheckResult {
            healthy,
            score,
            details,
        }
    }
}