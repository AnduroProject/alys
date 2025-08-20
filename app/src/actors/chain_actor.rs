//! ChainActor implementation for ALYS-007
//!
//! This module implements the ChainActor that replaces the monolithic Chain struct with a 
//! message-driven actor system. The actor handles consensus operations, block production, 
//! validation, finalization, and chain reorganization while maintaining state isolation
//! and eliminating shared mutable state patterns.
//!
//! ## Architecture
//!
//! The ChainActor follows the Alys V2 actor foundation system patterns:
//! - **State Isolation**: All chain state owned by the actor, no Arc<RwLock<>>
//! - **Message-Driven**: All operations via Actix messages with correlation IDs
//! - **Supervision**: Integrated with actor supervision system for fault tolerance
//! - **Performance**: <500ms block production, <100ms block import targets
//! - **Monitoring**: Comprehensive metrics and distributed tracing
//!
//! ## Consensus Integration
//!
//! - **Aura PoA**: Slot-based block production with federation signatures
//! - **AuxPoW**: Bitcoin merged mining for block finalization
//! - **Hybrid Model**: Fast federated block production + secure PoW finalization
//! - **Peg Operations**: Two-way peg integration for Bitcoin bridge
//!
//! ## Migration Support
//!
//! The actor supports gradual migration from legacy Chain struct through:
//! - Parallel execution modes during transition
//! - Backward compatibility adapters
//! - Feature flag controlled rollout
//! - Zero-consensus-disruption migration

use crate::messages::chain_messages::*;
use crate::types::*;
use crate::actors::foundation::*;
use crate::features::{FeatureFlagManager, FeatureFlag};
use crate::integration::*;

use actix::prelude::*;
use std::collections::{HashMap, VecDeque, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::{interval, timeout};
use tracing::*;
use uuid::Uuid;

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

/// Configuration for ChainActor behavior and performance
#[derive(Debug, Clone)]
pub struct ChainActorConfig {
    /// Slot duration for Aura consensus (default 2 seconds)
    pub slot_duration: Duration,
    
    /// Maximum blocks without PoW before halting
    pub max_blocks_without_pow: u64,
    
    /// Maximum reorg depth allowed
    pub max_reorg_depth: u32,
    
    /// Whether this node is a validator
    pub is_validator: bool,
    
    /// Authority key for block signing
    pub authority_key: Option<SecretKey>,
    
    /// Block production timeout
    pub production_timeout: Duration,
    
    /// Block import timeout
    pub import_timeout: Duration,
    
    /// Validation cache size
    pub validation_cache_size: usize,
    
    /// Maximum pending blocks
    pub max_pending_blocks: usize,
    
    /// Performance targets
    pub performance_targets: PerformanceTargets,
    
    /// Actor supervision configuration
    pub supervision_config: SupervisionConfig,
}

/// Performance targets for monitoring and optimization
#[derive(Debug, Clone)]
pub struct PerformanceTargets {
    /// Maximum block production time (default 500ms)
    pub max_production_time_ms: u64,
    
    /// Maximum block import time (default 100ms)
    pub max_import_time_ms: u64,
    
    /// Maximum validation time (default 50ms)
    pub max_validation_time_ms: u64,
    
    /// Target blocks per second
    pub target_blocks_per_second: f64,
    
    /// Maximum memory usage (MB)
    pub max_memory_mb: u64,
}

/// Current chain state managed by the actor
#[derive(Debug)]
pub struct ChainState {
    /// Current chain head
    pub head: Option<BlockRef>,
    
    /// Finalized block (confirmed with PoW)
    pub finalized: Option<BlockRef>,
    
    /// Genesis block reference
    pub genesis: BlockRef,
    
    /// Current block height
    pub height: u64,
    
    /// Total difficulty accumulator
    pub total_difficulty: U256,
    
    /// Pending PoW header awaiting finalization
    pub pending_pow: Option<AuxPowHeader>,
    
    /// Fork choice tracking
    pub fork_choice: ForkChoiceState,
    
    /// Recent block timing for performance monitoring
    pub recent_timings: VecDeque<BlockTiming>,
}

/// Information about pending blocks being processed
#[derive(Debug, Clone)]
pub struct PendingBlockInfo {
    /// The block being processed
    pub block: SignedConsensusBlock,
    
    /// When the block was received
    pub received_at: Instant,
    
    /// Current processing status
    pub status: ProcessingStatus,
    
    /// Validation attempts made
    pub validation_attempts: u32,
    
    /// Source of the block
    pub source: BlockSource,
    
    /// Priority for processing
    pub priority: BlockProcessingPriority,
    
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
    
    /// Dependencies that must be satisfied first
    pub dependencies: Vec<Hash256>,
}

/// Block processing status tracking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingStatus {
    /// Just received, waiting to start
    Queued,
    
    /// Currently validating
    Validating { started_at: Instant },
    
    /// Validation complete, waiting for dependencies
    ValidatedPending { dependencies: Vec<Hash256> },
    
    /// Ready for import
    ReadyForImport,
    
    /// Currently importing
    Importing { started_at: Instant },
    
    /// Import completed successfully
    Imported { completed_at: Instant },
    
    /// Processing failed
    Failed { reason: String, failed_at: Instant },
    
    /// Timed out during processing
    TimedOut { timeout_at: Instant },
}

/// Block candidate for production
#[derive(Debug, Clone)]
pub struct BlockCandidate {
    /// Slot this candidate is for
    pub slot: u64,
    
    /// Execution payload built
    pub execution_payload: ExecutionPayload,
    
    /// Peg-in operations to include
    pub pegins: Vec<(bitcoin::Txid, bitcoin::BlockHash)>,
    
    /// Peg-out proposal (if any)
    pub pegout_proposal: Option<bitcoin::Transaction>,
    
    /// When the candidate was created
    pub created_at: Instant,
    
    /// Priority for production
    pub priority: BlockProcessingPriority,
}

/// Federation state and configuration
#[derive(Debug)]
pub struct FederationState {
    /// Current federation version
    pub version: u32,
    
    /// Active federation members
    pub members: Vec<FederationMember>,
    
    /// Signature threshold
    pub threshold: usize,
    
    /// Pending configuration changes
    pub pending_changes: Vec<PendingFederationChange>,
    
    /// Recent signature performance
    pub signature_performance: SignaturePerformanceTracker,
}

/// Pending federation configuration change
#[derive(Debug)]
pub struct PendingFederationChange {
    /// New configuration
    pub new_config: FederationConfig,
    
    /// Effective block height
    pub effective_height: u64,
    
    /// Migration strategy
    pub migration_strategy: FederationMigrationStrategy,
    
    /// When the change was proposed
    pub proposed_at: SystemTime,
}

/// Federation configuration
#[derive(Debug, Clone)]
pub struct FederationConfig {
    pub version: u32,
    pub members: Vec<FederationMember>,
    pub threshold: usize,
}

/// Signature performance tracking for federation
#[derive(Debug)]
pub struct SignaturePerformanceTracker {
    /// Recent signature times by member
    pub member_signature_times: HashMap<Address, VecDeque<Duration>>,
    
    /// Average signature collection time
    pub avg_collection_time: Duration,
    
    /// Success rate tracking
    pub success_rates: HashMap<Address, f64>,
}

/// Auxiliary PoW state for Bitcoin merged mining
#[derive(Debug)]
pub struct AuxPowState {
    /// Current difficulty target
    pub current_target: U256,
    
    /// Height of last finalized PoW block
    pub last_pow_height: u64,
    
    /// Active miners tracking
    pub active_miners: HashSet<String>,
    
    /// Recent PoW submission performance
    pub pow_performance: PoWPerformanceTracker,
    
    /// Pending AuxPoW submissions
    pub pending_submissions: HashMap<Hash256, PendingAuxPow>,
}

/// Performance tracking for PoW operations
#[derive(Debug)]
pub struct PoWPerformanceTracker {
    /// Recent PoW validation times
    pub validation_times: VecDeque<Duration>,
    
    /// Network hash rate estimate
    pub estimated_hashrate: f64,
    
    /// Average time between PoW blocks
    pub avg_pow_interval: Duration,
    
    /// PoW submission success rate
    pub success_rate: f64,
}

/// Pending auxiliary PoW submission
#[derive(Debug)]
pub struct PendingAuxPow {
    /// The AuxPoW data
    pub auxpow: AuxPow,
    
    /// Target range for finalization
    pub target_range: (Hash256, Hash256),
    
    /// Miner information
    pub miner: String,
    
    /// Submission timestamp
    pub submitted_at: Instant,
    
    /// Validation attempts
    pub attempts: u32,
}

/// Block subscriber for notifications
#[derive(Debug)]
pub struct BlockSubscriber {
    /// Actor to receive notifications
    pub recipient: Recipient<BlockNotification>,
    
    /// Event types subscribed to
    pub event_types: HashSet<BlockEventType>,
    
    /// Filter criteria
    pub filter: Option<NotificationFilter>,
    
    /// Subscription start time
    pub subscribed_at: SystemTime,
    
    /// Messages sent counter
    pub messages_sent: u64,
}

/// Actor performance metrics
#[derive(Debug)]
pub struct ChainActorMetrics {
    /// Blocks produced by this actor
    pub blocks_produced: u64,
    
    /// Blocks imported successfully
    pub blocks_imported: u64,
    
    /// Blocks that failed validation
    pub validation_failures: u64,
    
    /// Chain reorganizations performed
    pub reorganizations: u32,
    
    /// Average block production time
    pub avg_production_time: MovingAverage,
    
    /// Average block import time
    pub avg_import_time: MovingAverage,
    
    /// Average validation time
    pub avg_validation_time: MovingAverage,
    
    /// Peak memory usage
    pub peak_memory_bytes: u64,
    
    /// Current queue depths
    pub queue_depths: QueueDepthTracker,
    
    /// Error counters
    pub error_counters: ErrorCounters,
    
    /// Performance violations
    pub performance_violations: PerformanceViolationTracker,
}

/// Moving average calculation
#[derive(Debug)]
pub struct MovingAverage {
    values: VecDeque<f64>,
    window_size: usize,
    sum: f64,
}

/// Queue depth tracking for performance monitoring
#[derive(Debug)]
pub struct QueueDepthTracker {
    pub pending_blocks: usize,
    pub block_candidates: usize,
    pub validation_queue: usize,
    pub notification_queue: usize,
}

/// Error counters for monitoring
#[derive(Debug)]
pub struct ErrorCounters {
    pub validation_errors: u64,
    pub import_errors: u64,
    pub production_errors: u64,
    pub network_errors: u64,
    pub auxpow_errors: u64,
    pub peg_operation_errors: u64,
}

/// Performance violation tracking
#[derive(Debug)]
pub struct PerformanceViolationTracker {
    pub production_timeouts: u32,
    pub import_timeouts: u32,
    pub validation_timeouts: u32,
    pub memory_violations: u32,
    pub last_violation_at: Option<Instant>,
}

/// Addresses of other actors for integration
#[derive(Debug)]
pub struct ActorAddresses {
    /// Engine actor for execution layer
    pub engine: Addr<EngineActor>,
    
    /// Bridge actor for peg operations
    pub bridge: Addr<BridgeActor>,
    
    /// Storage actor for persistence
    pub storage: Addr<StorageActor>,
    
    /// Network actor for P2P communication
    pub network: Addr<NetworkActor>,
    
    /// Sync actor for chain synchronization
    pub sync: Option<Addr<SyncActor>>,
    
    /// Root supervisor for health monitoring
    pub supervisor: Addr<RootSupervisor>,
}

/// Validation result cache for performance
#[derive(Debug)]
pub struct ValidationCache {
    /// Cache of recent validation results
    cache: HashMap<Hash256, CachedValidation>,
    
    /// Maximum cache size
    max_size: usize,
    
    /// Cache hit/miss statistics
    hits: u64,
    misses: u64,
}

/// Cached validation result
#[derive(Debug, Clone)]
pub struct CachedValidation {
    /// Validation result
    result: bool,
    
    /// Validation errors (if any)
    errors: Vec<ValidationError>,
    
    /// When cached
    cached_at: Instant,
    
    /// Cache expiry time
    expires_at: Instant,
}

/// Actor health monitoring state
#[derive(Debug)]
pub struct ActorHealthMonitor {
    /// Last health check time
    last_health_check: Instant,
    
    /// Health check interval
    health_check_interval: Duration,
    
    /// Health status
    status: ActorHealthStatus,
    
    /// Recent health scores
    recent_scores: VecDeque<u8>,
}

/// Block production state tracking
#[derive(Debug)]
pub struct BlockProductionState {
    /// Whether production is currently paused
    paused: bool,
    
    /// Reason for pause (if any)
    pause_reason: Option<String>,
    
    /// When pause ends (if scheduled)
    pause_until: Option<Instant>,
    
    /// Current slot being produced
    current_slot: Option<u64>,
    
    /// Production start time
    production_started: Option<Instant>,
    
    /// Recent production performance
    recent_production_times: VecDeque<Duration>,
}

/// Network broadcast tracking
#[derive(Debug)]
pub struct BroadcastTracker {
    /// Recent broadcast results
    recent_broadcasts: VecDeque<BroadcastMetrics>,
    
    /// Failed peer tracking
    failed_peers: HashMap<PeerId, FailedPeerInfo>,
    
    /// Broadcast success rate
    success_rate: f64,
}

/// Broadcast performance metrics
#[derive(Debug)]
pub struct BroadcastMetrics {
    /// Block hash broadcast
    block_hash: Hash256,
    
    /// Number of peers reached
    peers_reached: u32,
    
    /// Successful sends
    successful_sends: u32,
    
    /// Broadcast time
    broadcast_time: Duration,
    
    /// Timestamp
    timestamp: Instant,
}

/// Failed peer information
#[derive(Debug)]
pub struct FailedPeerInfo {
    /// Consecutive failures
    consecutive_failures: u32,
    
    /// Last failure time
    last_failure: Instant,
    
    /// Failure reasons
    failure_reasons: VecDeque<String>,
}

/// Fork choice state for managing chain forks
#[derive(Debug)]
pub struct ForkChoiceState {
    /// Known chain tips
    tips: HashMap<Hash256, ChainTip>,
    
    /// Current canonical tip
    canonical_tip: Hash256,
    
    /// Fork tracking
    active_forks: HashMap<Hash256, ForkInfo>,
}

/// Information about a chain tip
#[derive(Debug)]
pub struct ChainTip {
    /// Block reference
    block_ref: BlockRef,
    
    /// Total difficulty
    total_difficulty: U256,
    
    /// When this tip was last updated
    last_updated: Instant,
}

/// Information about an active fork
#[derive(Debug)]
pub struct ForkInfo {
    /// Fork point (common ancestor)
    fork_point: BlockRef,
    
    /// Current tip of this fork
    current_tip: BlockRef,
    
    /// Number of blocks in this fork
    length: u32,
    
    /// When fork was detected
    detected_at: Instant,
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
        self.metrics.queue_depths.pending_blocks = self.pending_blocks.len();
        self.metrics.queue_depths.block_candidates = self.block_candidates.len();
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        info!(
            blocks_produced = self.metrics.blocks_produced,
            blocks_imported = self.metrics.blocks_imported,
            "ChainActor stopping gracefully"
        );
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
        
        let chain_state = ChainState {
            head: None,
            finalized: None,
            genesis: genesis.clone(),
            height: 0,
            total_difficulty: U256::zero(),
            pending_pow: None,
            fork_choice: ForkChoiceState {
                tips: HashMap::new(),
                canonical_tip: genesis.hash,
                active_forks: HashMap::new(),
            },
            recent_timings: VecDeque::with_capacity(100),
        };

        let federation = FederationState {
            version: 0,
            members: Vec::new(),
            threshold: 0,
            pending_changes: Vec::new(),
            signature_performance: SignaturePerformanceTracker {
                member_signature_times: HashMap::new(),
                avg_collection_time: Duration::from_millis(100),
                success_rates: HashMap::new(),
            },
        };

        let auxpow_state = AuxPowState {
            current_target: U256::from(1u64) << 235, // Default target
            last_pow_height: 0,
            active_miners: HashSet::new(),
            pow_performance: PoWPerformanceTracker {
                validation_times: VecDeque::with_capacity(50),
                estimated_hashrate: 0.0,
                avg_pow_interval: Duration::from_secs(600), // 10 minutes default
                success_rate: 0.0,
            },
            pending_submissions: HashMap::new(),
        };

        let metrics = ChainActorMetrics {
            blocks_produced: 0,
            blocks_imported: 0,
            validation_failures: 0,
            reorganizations: 0,
            avg_production_time: MovingAverage::new(50),
            avg_import_time: MovingAverage::new(100),
            avg_validation_time: MovingAverage::new(100),
            peak_memory_bytes: 0,
            queue_depths: QueueDepthTracker {
                pending_blocks: 0,
                block_candidates: 0,
                validation_queue: 0,
                notification_queue: 0,
            },
            error_counters: ErrorCounters {
                validation_errors: 0,
                import_errors: 0,
                production_errors: 0,
                network_errors: 0,
                auxpow_errors: 0,
                peg_operation_errors: 0,
            },
            performance_violations: PerformanceViolationTracker {
                production_timeouts: 0,
                import_timeouts: 0,
                validation_timeouts: 0,
                memory_violations: 0,
                last_violation_at: None,
            },
        };

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
            validation_cache: ValidationCache {
                cache: HashMap::new(),
                max_size: config.validation_cache_size,
                hits: 0,
                misses: 0,
            },
            health_monitor: ActorHealthMonitor {
                last_health_check: Instant::now(),
                health_check_interval: Duration::from_secs(30),
                status: ActorHealthStatus {
                    active_actors: 1,
                    failed_actors: 0,
                    queue_depths: HashMap::new(),
                    system_health: 100,
                    supervision_active: true,
                },
                recent_scores: VecDeque::with_capacity(10),
            },
            trace_context: TraceContext::default(),
            production_state: BlockProductionState {
                paused: false,
                pause_reason: None,
                pause_until: None,
                current_slot: None,
                production_started: None,
                recent_production_times: VecDeque::with_capacity(20),
            },
            broadcast_tracker: BroadcastTracker {
                recent_broadcasts: VecDeque::with_capacity(50),
                failed_peers: HashMap::new(),
                success_rate: 1.0,
            },
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
                        act.metrics.error_counters.auxpow_errors += 1;
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
        let queue_size = self.pending_blocks.len();
        let avg_production = self.metrics.avg_production_time.current();
        let avg_import = self.metrics.avg_import_time.current();
        
        info!(
            blocks_produced = self.metrics.blocks_produced,
            blocks_imported = self.metrics.blocks_imported,
            queue_size = queue_size,
            avg_production_ms = avg_production,
            avg_import_ms = avg_import,
            validation_failures = self.metrics.validation_failures,
            "ChainActor performance metrics"
        );

        // Update queue depth tracking
        self.metrics.queue_depths.pending_blocks = self.pending_blocks.len();
        self.metrics.queue_depths.block_candidates = self.block_candidates.len();
        
        // Check for performance violations
        self.check_performance_violations();
    }

    /// Check for performance violations
    fn check_performance_violations(&mut self) {
        let targets = &self.config.performance_targets;
        
        if self.metrics.avg_production_time.current() > targets.max_production_time_ms as f64 {
            self.metrics.performance_violations.production_timeouts += 1;
            warn!("Block production time exceeded target");
        }
        
        if self.metrics.avg_import_time.current() > targets.max_import_time_ms as f64 {
            self.metrics.performance_violations.import_timeouts += 1;
            warn!("Block import time exceeded target");
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
        if self.metrics.avg_production_time.current() > self.config.performance_targets.max_production_time_ms as f64 {
            score = score.saturating_sub(15);
        }

        if self.metrics.avg_import_time.current() > self.config.performance_targets.max_import_time_ms as f64 {
            score = score.saturating_sub(15);
        }

        // Check error rates
        let recent_errors = self.metrics.error_counters.validation_errors + 
                           self.metrics.error_counters.import_errors;
        if recent_errors > 10 {
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

// Message handler implementations will be added in subsequent parts
// This includes handlers for ImportBlock, ProduceBlock, GetChainStatus, etc.

impl MovingAverage {
    pub fn new(window_size: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(window_size),
            window_size,
            sum: 0.0,
        }
    }

    pub fn add(&mut self, value: f64) {
        if self.values.len() >= self.window_size {
            if let Some(old_value) = self.values.pop_front() {
                self.sum -= old_value;
            }
        }
        
        self.values.push_back(value);
        self.sum += value;
    }

    pub fn current(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f64
        }
    }
}

impl Default for ChainActorConfig {
    fn default() -> Self {
        Self {
            slot_duration: Duration::from_secs(2),
            max_blocks_without_pow: 10,
            max_reorg_depth: 32,
            is_validator: false,
            authority_key: None,
            production_timeout: Duration::from_millis(500),
            import_timeout: Duration::from_millis(100),
            validation_cache_size: 1000,
            max_pending_blocks: 100,
            performance_targets: PerformanceTargets {
                max_production_time_ms: 500,
                max_import_time_ms: 100,
                max_validation_time_ms: 50,
                target_blocks_per_second: 0.5, // 2 second blocks
                max_memory_mb: 512,
            },
            supervision_config: SupervisionConfig::default(),
        }
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self {
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            baggage: HashMap::new(),
            sampled: false,
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

// Placeholder actor types for integration
pub struct EngineActor;
pub struct BridgeActor;
pub struct StorageActor;
pub struct NetworkActor;
pub struct SyncActor;
pub struct RootSupervisor;

impl Actor for EngineActor { type Context = Context<Self>; }
impl Actor for BridgeActor { type Context = Context<Self>; }
impl Actor for StorageActor { type Context = Context<Self>; }
impl Actor for NetworkActor { type Context = Context<Self>; }
impl Actor for SyncActor { type Context = Context<Self>; }
impl Actor for RootSupervisor { type Context = Context<Self>; }