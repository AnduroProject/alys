//! Blockchain-aware actor system extensions
//!
//! This module provides blockchain-specific extensions to the core actor framework,
//! supporting the Alys V2 merged mining sidechain with federated PoA consensus,
//! 2-second block timing, and governance integration.

use crate::{
    actor::{AlysActor, ActorRegistration},
    lifecycle::LifecycleAware,
    supervisor::{RestartStrategy, EscalationStrategy},
    error::{ActorError, ActorResult},
    metrics::ActorMetrics,
};
use actix::{Actor, Addr, Context, Message};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tracing::{info, warn, error};

/// Blockchain timing constraints for the Alys sidechain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainTimingConstraints {
    /// Block production interval (2 seconds for Alys)
    pub block_interval: Duration,
    /// Maximum allowed consensus operation latency
    pub max_consensus_latency: Duration,
    /// Federation coordination timeout
    pub federation_timeout: Duration,
    /// AuxPoW submission window
    pub auxpow_window: Duration,
}

impl Default for BlockchainTimingConstraints {
    fn default() -> Self {
        Self {
            block_interval: Duration::from_secs(2),
            max_consensus_latency: Duration::from_millis(100),
            federation_timeout: Duration::from_millis(500),
            auxpow_window: Duration::from_secs(600), // 10 minutes
        }
    }
}

/// Federation configuration for consensus operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Current federation members
    pub members: Vec<String>,
    /// Signature threshold (e.g., 3 of 5)
    pub threshold: usize,
    /// Federation health check interval
    pub health_interval: Duration,
    /// Minimum healthy members for operation
    pub min_healthy: usize,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            members: Vec::new(),
            threshold: 3,
            health_interval: Duration::from_secs(30),
            min_healthy: 3,
        }
    }
}

/// Actor priority levels for blockchain operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BlockchainActorPriority {
    /// Critical consensus operations (ChainActor, EngineActor)
    Consensus = 0,
    /// High priority bridge operations (BridgeActor, StreamActor)  
    Bridge = 1,
    /// Normal network operations (SyncActor, NetworkActor)
    Network = 2,
    /// Background services (StorageActor, MetricsActor)
    Background = 3,
}

/// Enhanced actor trait with blockchain-specific capabilities
#[async_trait]
pub trait BlockchainAwareActor: AlysActor {
    /// Get blockchain timing constraints for this actor
    fn timing_constraints(&self) -> BlockchainTimingConstraints {
        BlockchainTimingConstraints::default()
    }
    
    /// Get federation configuration if this actor participates in federation
    fn federation_config(&self) -> Option<FederationConfig> {
        None
    }
    
    /// Get blockchain-specific priority level
    fn blockchain_priority(&self) -> BlockchainActorPriority {
        BlockchainActorPriority::Background
    }
    
    /// Check if actor is critical for consensus operations
    fn is_consensus_critical(&self) -> bool {
        self.blockchain_priority() == BlockchainActorPriority::Consensus
    }
    
    /// Handle blockchain-specific events (block production, finalization, etc.)
    async fn handle_blockchain_event(&mut self, event: BlockchainEvent) -> ActorResult<()> {
        match event {
            BlockchainEvent::BlockProduced { height, hash } => {
                info!(
                    actor_type = LifecycleAware::actor_type(self),
                    height = height,
                    hash = ?hash,
                    "Block produced event received"
                );
                Ok(())
            }
            BlockchainEvent::BlockFinalized { height, hash } => {
                info!(
                    actor_type = LifecycleAware::actor_type(self),
                    height = height, 
                    hash = ?hash,
                    "Block finalized event received"
                );
                Ok(())
            }
            BlockchainEvent::FederationChange { members, threshold } => {
                info!(
                    actor_type = LifecycleAware::actor_type(self),
                    members = ?members,
                    threshold = threshold,
                    "Federation change event received"
                );
                Ok(())
            }
            BlockchainEvent::ConsensusFailure { reason } => {
                error!(
                    actor_type = LifecycleAware::actor_type(self),
                    reason = %reason,
                    "Consensus failure event received"
                );
                Ok(())
            }
        }
    }
    
    /// Validate that actor can operate under current blockchain conditions
    async fn validate_blockchain_readiness(&self) -> ActorResult<BlockchainReadiness> {
        Ok(BlockchainReadiness {
            can_produce_blocks: true,
            can_validate_blocks: true,
            federation_healthy: true,
            sync_status: SyncStatus::Synced,
            last_validated: SystemTime::now(),
        })
    }
}

/// Blockchain events that actors can subscribe to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockchainEvent {
    /// New block has been produced
    BlockProduced { height: u64, hash: [u8; 32] },
    /// Block has been finalized via AuxPoW
    BlockFinalized { height: u64, hash: [u8; 32] },
    /// Federation membership has changed
    FederationChange { members: Vec<String>, threshold: usize },
    /// Consensus operation failed
    ConsensusFailure { reason: String },
}

impl Message for BlockchainEvent {
    type Result = ActorResult<()>;
}

/// Blockchain readiness status for an actor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainReadiness {
    /// Can participate in block production
    pub can_produce_blocks: bool,
    /// Can validate incoming blocks
    pub can_validate_blocks: bool,
    /// Federation is healthy enough for operations
    pub federation_healthy: bool,
    /// Current sync status
    pub sync_status: SyncStatus,
    /// Last validation timestamp
    pub last_validated: SystemTime,
}

/// Synchronization status for blockchain operations
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Not synced, cannot produce blocks
    NotSynced,
    /// Syncing in progress
    Syncing { progress: f64 },
    /// Synced enough for block production (99.5%+)
    SyncedForProduction,
    /// Fully synced
    Synced,
}

/// Enhanced restart strategy for blockchain-aware actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainRestartStrategy {
    /// Base restart strategy
    pub base_strategy: RestartStrategy,
    /// Align restart timing to block boundaries
    pub align_to_blocks: bool,
    /// Respect consensus timing constraints
    pub respect_consensus: bool,
    /// Maximum restart time for consensus-critical actors
    pub max_consensus_downtime: Duration,
    /// Federation health requirements during restart
    pub federation_requirements: Option<FederationHealthRequirement>,
}

impl Default for BlockchainRestartStrategy {
    fn default() -> Self {
        Self {
            base_strategy: RestartStrategy::default(),
            align_to_blocks: true,
            respect_consensus: true,
            max_consensus_downtime: Duration::from_millis(500),
            federation_requirements: None,
        }
    }
}

impl BlockchainRestartStrategy {
    /// Calculate restart delay with blockchain-specific adjustments
    pub fn calculate_blockchain_delay(
        &self, 
        attempt: u32, 
        timing_constraints: &BlockchainTimingConstraints
    ) -> Option<Duration> {
        let mut base_delay = self.base_strategy.calculate_delay(attempt)?;
        
        // Align to block boundaries if requested
        if self.align_to_blocks {
            base_delay = self.align_to_block_boundary(base_delay, timing_constraints);
        }
        
        // Respect consensus timing constraints
        if self.respect_consensus {
            base_delay = base_delay.min(self.max_consensus_downtime);
        }
        
        Some(base_delay)
    }
    
    fn align_to_block_boundary(
        &self, 
        delay: Duration, 
        constraints: &BlockchainTimingConstraints
    ) -> Duration {
        let block_time_ms = constraints.block_interval.as_millis() as u64;
        let delay_ms = delay.as_millis() as u64;
        let aligned_ms = ((delay_ms + block_time_ms - 1) / block_time_ms) * block_time_ms;
        Duration::from_millis(aligned_ms)
    }
}

/// Federation health requirements for actor operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationHealthRequirement {
    /// Minimum number of healthy federation members required
    pub min_healthy_members: usize,
    /// Maximum time to wait for federation health
    pub max_wait_time: Duration,
    /// Whether to proceed with degraded federation
    pub allow_degraded_operation: bool,
}

/// Enhanced actor registration with blockchain-specific metadata
#[derive(Debug)]
pub struct BlockchainActorRegistration {
    /// Base actor registration
    pub base: ActorRegistration,
    /// Blockchain-specific priority
    pub blockchain_priority: BlockchainActorPriority,
    /// Timing constraints for this actor
    pub timing_constraints: BlockchainTimingConstraints,
    /// Federation configuration (if applicable)
    pub federation_config: Option<FederationConfig>,
    /// Last blockchain readiness check
    pub last_readiness_check: Option<(SystemTime, BlockchainReadiness)>,
    /// Blockchain event subscriptions
    pub event_subscriptions: Vec<BlockchainEventType>,
}

/// Types of blockchain events actors can subscribe to
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockchainEventType {
    BlockProduction,
    BlockFinalization,
    FederationChanges,
    ConsensusFailures,
    SyncStatusChanges,
}

/// Message for subscribing to blockchain events
#[derive(Debug, Clone, Message)]
#[rtype(result = "ActorResult<()>")]
pub struct SubscribeToBlockchainEvents {
    /// Actor address to send events to
    pub subscriber: actix::Recipient<BlockchainEvent>,
    /// Event types to subscribe to
    pub event_types: Vec<BlockchainEventType>,
}

/// Message for updating blockchain readiness status
#[derive(Debug, Clone, Message)]
#[rtype(result = "ActorResult<BlockchainReadiness>")]
pub struct CheckBlockchainReadiness;

/// Blockchain-aware supervision context
#[derive(Debug, Clone)]
pub struct BlockchainSupervisionContext {
    /// Timing constraints for the supervised actor
    pub timing_constraints: BlockchainTimingConstraints,
    /// Federation requirements
    pub federation_requirements: Option<FederationHealthRequirement>,
    /// Last consensus health check
    pub last_consensus_check: Option<SystemTime>,
    /// Current blockchain readiness
    pub blockchain_readiness: Option<BlockchainReadiness>,
}

/// Factory for creating blockchain-aware actors
pub struct BlockchainActorFactory;

impl BlockchainActorFactory {
    /// Create a blockchain-aware actor with enhanced supervision
    pub async fn create_blockchain_actor<A>(
        id: String,
        config: A::Config,
        blockchain_config: BlockchainActorConfig,
    ) -> ActorResult<Addr<A>>
    where
        A: BlockchainAwareActor + Actor<Context = Context<A>> + 'static,
    {
        let actor = A::new(config).map_err(|e| e.into())?;
        let addr = actor.start();
        
        info!(
            actor_id = %id,
            actor_type = %std::any::type_name::<A>(),
            priority = ?blockchain_config.priority,
            "Blockchain-aware actor created"
        );
        
        Ok(addr)
    }
}

/// Configuration for blockchain-aware actors
#[derive(Debug, Clone)]
pub struct BlockchainActorConfig {
    /// Blockchain-specific priority
    pub priority: BlockchainActorPriority,
    /// Timing constraints
    pub timing_constraints: BlockchainTimingConstraints,
    /// Federation configuration
    pub federation_config: Option<FederationConfig>,
    /// Event subscriptions
    pub event_subscriptions: Vec<BlockchainEventType>,
    /// Restart strategy
    pub restart_strategy: BlockchainRestartStrategy,
}

impl Default for BlockchainActorConfig {
    fn default() -> Self {
        Self {
            priority: BlockchainActorPriority::Background,
            timing_constraints: BlockchainTimingConstraints::default(),
            federation_config: None,
            event_subscriptions: Vec::new(),
            restart_strategy: BlockchainRestartStrategy::default(),
        }
    }
}

// Convenience functions for common blockchain actor patterns

/// Create a consensus-critical actor with appropriate configuration
pub async fn create_consensus_actor<A>(
    id: String,
    config: A::Config,
) -> ActorResult<Addr<A>>
where
    A: BlockchainAwareActor + Actor<Context = Context<A>> + 'static,
{
    let blockchain_config = BlockchainActorConfig {
        priority: BlockchainActorPriority::Consensus,
        timing_constraints: BlockchainTimingConstraints::default(),
        event_subscriptions: vec![
            BlockchainEventType::BlockProduction,
            BlockchainEventType::BlockFinalization,
            BlockchainEventType::ConsensusFailures,
        ],
        restart_strategy: BlockchainRestartStrategy {
            max_consensus_downtime: Duration::from_millis(100),
            ..Default::default()
        },
        ..Default::default()
    };
    
    BlockchainActorFactory::create_blockchain_actor(id, config, blockchain_config).await
}

/// Create a federation-aware actor with appropriate configuration
pub async fn create_federation_actor<A>(
    id: String,
    config: A::Config,
    federation_config: FederationConfig,
) -> ActorResult<Addr<A>>
where
    A: BlockchainAwareActor + Actor<Context = Context<A>> + 'static,
{
    let blockchain_config = BlockchainActorConfig {
        priority: BlockchainActorPriority::Bridge,
        federation_config: Some(federation_config),
        event_subscriptions: vec![
            BlockchainEventType::FederationChanges,
            BlockchainEventType::BlockFinalization,
        ],
        restart_strategy: BlockchainRestartStrategy {
            federation_requirements: Some(FederationHealthRequirement {
                min_healthy_members: 3,
                max_wait_time: Duration::from_secs(30),
                allow_degraded_operation: false,
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    
    BlockchainActorFactory::create_blockchain_actor(id, config, blockchain_config).await
}