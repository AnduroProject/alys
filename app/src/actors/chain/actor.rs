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
    config: ChainActorConfig,
    
    /// Current chain state (owned by actor, no sharing)
    chain_state: ChainState,
    
    /// Pending blocks awaiting processing or validation
    pending_blocks: HashMap<Hash256, PendingBlockInfo>,
    
    /// Block candidate queue for production
    block_candidates: VecDeque<BlockCandidate>,
    
    /// Federation configuration and state
    federation: FederationState,
    
    /// Auxiliary PoW state for Bitcoin merged mining
    auxpow_state: AuxPowState,
    
    /// Subscriber management for block notifications
    subscribers: HashMap<Uuid, BlockSubscriber>,
    
    /// Performance metrics and monitoring
    metrics: ChainActorMetrics,
    
    /// Feature flag manager for gradual rollout
    feature_flags: Arc<FeatureFlagManager>,
    
    /// Integration with other actors
    actor_addresses: ActorAddresses,
    
    /// Validation result cache
    validation_cache: ValidationCache,
    
    /// Actor health monitoring
    health_monitor: ActorHealthMonitor,
    
    /// Distributed tracing context
    trace_context: TraceContext,
    
    /// Block production state
    production_state: BlockProductionState,
    
    /// Network broadcast tracking
    broadcast_tracker: BroadcastTracker,
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
        
        supervisor.do_send(RegisterActor {
            name: "ChainActor".to_string(),
            address: self_addr.clone().recipient(),
            health_check_interval: self.health_monitor.health_check_interval,
        });
    }

    /// Calculate the current slot based on system time
    fn calculate_current_slot(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        now.as_secs() / self.config.slot_duration.as_secs()
    }

    /// Check if this node should produce a block for the given slot
    fn should_produce_block(&self, slot: u64) -> bool {
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