//! Chain State Management
//!
//! All chain state structures and related implementations for the ChainActor.
//! This module contains the complete state model including chain state, federation state,
//! auxiliary proof-of-work state, and all supporting structures.

use std::collections::{HashMap, VecDeque, HashSet};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;
use actix::prelude::*;

// Import types from other modules
use crate::types::*;
use crate::messages::chain_messages::*;

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
    
    /// Finalization manager for AuxPoW
    pub finalization_manager: super::handlers::auxpow_handlers::FinalizationManager,
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
    pub last_health_check: Instant,
    
    /// Health check interval
    pub health_check_interval: Duration,
    
    /// Health status
    pub status: ActorHealthStatus,
    
    /// Recent health scores
    pub recent_scores: VecDeque<u8>,
}

/// Block production state tracking
#[derive(Debug)]
pub struct BlockProductionState {
    /// Whether production is currently paused
    pub paused: bool,
    
    /// Reason for pause (if any)
    pub pause_reason: Option<String>,
    
    /// When pause ends (if scheduled)
    pub pause_until: Option<Instant>,
    
    /// When pause started (for tracking)
    pub paused_at: Option<SystemTime>,
    
    /// When pause should be automatically lifted
    pub resume_at: Option<SystemTime>,
    
    /// Current slot being produced
    pub current_slot: Option<u64>,
    
    /// Production start time
    pub production_started: Option<Instant>,
    
    /// Recent production performance
    pub recent_production_times: VecDeque<Duration>,
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
    
    /// Advanced reorganization manager
    pub reorg_manager: ReorganizationManager,
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

/// Advanced reorganization management system
#[derive(Debug)]
pub struct ReorganizationManager {
    /// State trees for different heights
    state_at_height: BTreeMap<u64, ChainSnapshot>,
    
    /// Orphan blocks awaiting parent connection
    orphan_pool: HashMap<Hash256, SignedConsensusBlock>,
    
    /// Block index for fast lookups
    block_index: HashMap<Hash256, BlockMetadata>,
    
    /// Chain metrics for reorganization tracking
    chain_metrics: ChainStateMetrics,
    
    /// Configuration parameters
    config: StateManagerConfig,
}

/// Snapshot of chain state at a specific height
#[derive(Debug, Clone)]
pub struct ChainSnapshot {
    /// Block at this height
    pub block: BlockRef,
    
    /// State root hash
    pub state_root: Hash256,
    
    /// Execution state summary
    pub execution_state: ExecutionState,
    
    /// Federation state at this height
    pub federation_state: FederationState,
    
    /// Finalization status
    pub finalization_status: FinalizationStatus,
}

/// Metadata about a block for efficient lookups
#[derive(Debug, Clone)]
pub struct BlockMetadata {
    /// Block height
    pub height: u64,
    
    /// Parent block hash
    pub parent: Hash256,
    
    /// Child blocks
    pub children: Vec<Hash256>,
    
    /// Total difficulty at this block
    pub difficulty: U256,
    
    /// Block timestamp
    pub timestamp: Duration,
    
    /// Whether this block is finalized
    pub is_finalized: bool,
    
    /// Whether this block is on canonical chain
    pub is_canonical: bool,
    
    /// Number of confirmations
    pub confirmations: u64,
}

/// Finalization status of a block
#[derive(Debug, Clone, PartialEq)]
pub enum FinalizationStatus {
    /// Not yet finalized
    Unfinalized,
    
    /// Pending finalization with AuxPoW
    PendingFinalization(AuxPowHeader),
    
    /// Fully finalized
    Finalized(AuxPowHeader),
}

/// Configuration for state management
#[derive(Debug, Clone)]
pub struct StateManagerConfig {
    /// Maximum number of orphan blocks to keep
    pub max_orphan_blocks: usize,
    
    /// Maximum size of state cache
    pub state_cache_size: usize,
    
    /// Maximum allowed reorganization depth
    pub max_reorg_depth: u64,
    
    /// Interval for state snapshots
    pub snapshot_interval: u64,
    
    /// Time to retain non-canonical branches
    pub branch_retention_time: Duration,
}

impl Default for StateManagerConfig {
    fn default() -> Self {
        Self {
            max_orphan_blocks: 1000,
            state_cache_size: 5000,
            max_reorg_depth: 64,
            snapshot_interval: 10,
            branch_retention_time: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Chain state metrics for monitoring
#[derive(Debug)]
pub struct ChainStateMetrics {
    /// Number of reorganizations
    pub reorgs: u64,
    
    /// Average reorganization depth
    pub avg_reorg_depth: f64,
    
    /// Maximum reorganization depth seen
    pub max_reorg_depth: u64,
    
    /// Current finalized height
    pub finalized_height: u64,
    
    /// Orphan blocks currently held
    pub orphan_blocks: usize,
    
    /// Cache hit rate
    pub cache_hit_rate: f64,
}

impl Default for ChainStateMetrics {
    fn default() -> Self {
        Self {
            reorgs: 0,
            avg_reorg_depth: 0.0,
            max_reorg_depth: 0,
            finalized_height: 0,
            orphan_blocks: 0,
            cache_hit_rate: 0.0,
        }
    }
}

/// Results of adding a block to the state manager
#[derive(Debug)]
pub enum AddBlockResult {
    /// Block extended the canonical chain
    ExtendedChain,
    
    /// Block created a new fork
    CreatedFork,
    
    /// Block was orphaned (parent not found)
    Orphaned,
    
    /// Block already exists
    AlreadyExists,
}

/// Result of a reorganization operation
#[derive(Debug)]
pub struct ReorgResult {
    /// Hash of the old chain tip
    pub old_tip: Hash256,
    
    /// Hash of the new chain tip
    pub new_tip: Hash256,
    
    /// Depth of the reorganization
    pub reorg_depth: u64,
    
    /// Number of blocks reverted
    pub blocks_reverted: u64,
    
    /// Number of blocks applied
    pub blocks_applied: u64,
    
    /// Common ancestor block
    pub common_ancestor: Hash256,
}

/// Processed block result
#[derive(Debug)]
pub struct ProcessedBlock {
    /// Block hash
    pub hash: Hash256,
    
    /// Processing result
    pub result: ProcessBlockResult,
}

/// Result of processing a block
#[derive(Debug)]
pub enum ProcessBlockResult {
    /// Block was accepted
    Accepted,
    
    /// Block was rejected with error
    Rejected(ChainError),
}

// Implementation methods for state structures
impl ChainState {
    /// Create a new chain state with genesis block
    pub fn new(genesis: BlockRef) -> Self {
        Self {
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
                reorg_manager: ReorganizationManager::new(StateManagerConfig::default(), genesis.clone()),
            },
            recent_timings: VecDeque::with_capacity(100),
        }
    }
    
    /// Check if the chain is synced
    pub fn is_synced(&self) -> bool {
        // Implementation would check sync status
        true // Placeholder
    }
    
    /// Get the head block number
    pub fn head_block_number(&self) -> u64 {
        self.height
    }
    
    /// Get sync progress (0.0 to 1.0)
    pub fn sync_progress(&self) -> f64 {
        // Implementation would calculate sync progress
        1.0 // Placeholder
    }
    
    /// Get finalized height
    pub fn finalized_height(&self) -> u64 {
        self.finalized.as_ref().map_or(0, |f| f.number)
    }
    
    /// Set finalized height
    pub fn set_finalized_height(&mut self, height: u64) {
        // Implementation would update finalized state
    }
}

impl FederationState {
    /// Create a new federation state
    pub fn new(config: Option<FederationConfig>) -> Self {
        let (members, threshold, version) = if let Some(cfg) = config {
            (cfg.members, cfg.threshold, cfg.version)
        } else {
            (Vec::new(), 0, 0)
        };
        
        Self {
            version,
            members,
            threshold,
            pending_changes: Vec::new(),
            signature_performance: SignaturePerformanceTracker {
                member_signature_times: HashMap::new(),
                avg_collection_time: Duration::from_millis(100),
                success_rates: HashMap::new(),
            },
        }
    }
    
    /// Check if federation is healthy
    pub fn is_healthy(&self) -> bool {
        self.healthy_members() >= self.threshold
    }
    
    /// Count healthy members
    pub fn healthy_members(&self) -> usize {
        // Implementation would check member health
        self.members.len() // Placeholder
    }
}

impl AuxPowState {
    /// Create a new auxiliary PoW state
    pub fn new() -> Self {
        use super::handlers::auxpow_handlers::FinalizationConfig;
        
        Self {
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
            finalization_manager: super::handlers::auxpow_handlers::FinalizationManager::new(
                FinalizationConfig::default()
            ),
        }
    }
}

impl ValidationCache {
    /// Create a new validation cache with the given size
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            max_size,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64
        }
    }
}

impl ActorHealthMonitor {
    /// Create a new health monitor for the given actor
    pub fn new(actor_name: String) -> Self {
        Self {
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
        }
    }
}

impl Default for BlockProductionState {
    fn default() -> Self {
        Self {
            paused: false,
            pause_reason: None,
            pause_until: None,
            current_slot: None,
            production_started: None,
            recent_production_times: VecDeque::with_capacity(20),
        }
    }
}

impl Default for BroadcastTracker {
    fn default() -> Self {
        Self {
            recent_broadcasts: VecDeque::with_capacity(50),
            failed_peers: HashMap::new(),
            success_rate: 1.0,
        }
    }
}

// Placeholder actor types - these should be imported from other modules
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

impl ActorAddresses {
    /// Create a new set of actor addresses (placeholder implementation)
    pub fn new() -> Self {
        // This would be properly initialized with real actor addresses
        todo!("ActorAddresses::new not yet implemented")
    }
}

impl ReorganizationManager {
    /// Create a new reorganization manager with genesis block
    pub fn new(config: StateManagerConfig, genesis: BlockRef) -> Self {
        let mut state_manager = Self {
            state_at_height: BTreeMap::new(),
            orphan_pool: HashMap::new(),
            block_index: HashMap::new(),
            chain_metrics: ChainStateMetrics::default(),
            config,
        };

        // Initialize with genesis
        let genesis_snapshot = ChainSnapshot {
            block: genesis.clone(),
            state_root: Hash256::zero(), // Would be actual state root
            execution_state: ExecutionState::default(),
            federation_state: FederationState::new(None),
            finalization_status: FinalizationStatus::Finalized(AuxPowHeader::default()),
        };

        state_manager.state_at_height.insert(0, genesis_snapshot);
        state_manager.block_index.insert(genesis.hash, BlockMetadata {
            height: 0,
            parent: Hash256::zero(),
            children: vec![],
            difficulty: U256::zero(),
            timestamp: genesis.timestamp,
            is_finalized: true,
            is_canonical: true,
            confirmations: 0,
        });

        state_manager
    }

    /// Add a block to the chain state
    pub fn add_block(&mut self, block: SignedConsensusBlock) -> Result<AddBlockResult, ChainError> {
        let block_hash = block.message.hash();
        let parent_hash = block.message.parent_hash;

        // Check if we already have this block
        if self.block_index.contains_key(&block_hash) {
            return Ok(AddBlockResult::AlreadyExists);
        }

        // Check if parent exists
        if let Some(parent_metadata) = self.block_index.get_mut(&parent_hash) {
            // Parent exists, add to chain
            parent_metadata.children.push(block_hash);
            
            let height = parent_metadata.height + 1;
            
            // Add block metadata
            self.block_index.insert(block_hash, BlockMetadata {
                height,
                parent: parent_hash,
                children: vec![],
                difficulty: block.message.difficulty(),
                timestamp: block.message.timestamp,
                is_finalized: false,
                is_canonical: self.is_extending_canonical_chain(&parent_hash),
                confirmations: 0,
            });

            // Create state snapshot
            let snapshot = self.create_snapshot_from_parent(&block, parent_hash)?;
            self.state_at_height.insert(height, snapshot);

            // Update chain tip if canonical
            if self.is_extending_canonical_chain(&parent_hash) {
                self.update_canonical_chain(block_hash, height)?;
                Ok(AddBlockResult::ExtendedChain)
            } else {
                Ok(AddBlockResult::CreatedFork)
            }
        } else {
            // Parent doesn't exist, add to orphan pool
            if self.orphan_pool.len() >= self.config.max_orphan_blocks {
                // Remove oldest orphan
                if let Some((oldest_hash, _)) = self.orphan_pool.iter().next() {
                    let oldest_hash = *oldest_hash;
                    self.orphan_pool.remove(&oldest_hash);
                }
            }
            
            self.orphan_pool.insert(block_hash, block);
            self.chain_metrics.orphan_blocks = self.orphan_pool.len();
            Ok(AddBlockResult::Orphaned)
        }
    }

    /// Reorganize chain to the specified block
    pub fn reorganize_to_block(
        &mut self,
        target_block_hash: Hash256,
    ) -> Result<ReorgResult, ChainError> {
        let target_metadata = self.block_index.get(&target_block_hash)
            .ok_or(ChainError::BlockNotFound)?;

        let current_tip = self.get_canonical_tip()?;
        
        // Find common ancestor
        let common_ancestor = self.find_common_ancestor(
            target_block_hash,
            current_tip.block.hash,
        )?;

        let reorg_depth = current_tip.block.number - common_ancestor.height;
        if reorg_depth > self.config.max_reorg_depth {
            return Err(ChainError::ReorgTooDeep);
        }

        // Check finalization constraints
        if let Some(snapshot) = self.state_at_height.get(&common_ancestor.height) {
            if snapshot.finalization_status != FinalizationStatus::Unfinalized {
                return Err(ChainError::ReorgPastFinalized);
            }
        }

        // Build new canonical chain
        let new_chain = self.build_chain_to_block(target_block_hash, common_ancestor.block.hash)?;
        
        // Update canonical flags
        self.update_canonical_flags(&new_chain)?;

        // Update state snapshots
        self.rebuild_state_from_ancestor(&common_ancestor, &new_chain)?;

        // Update metrics
        self.chain_metrics.reorgs += 1;
        let total_reorgs = self.chain_metrics.reorgs as f64;
        self.chain_metrics.avg_reorg_depth = 
            (self.chain_metrics.avg_reorg_depth * (total_reorgs - 1.0) + reorg_depth as f64) / total_reorgs;
        
        if reorg_depth > self.chain_metrics.max_reorg_depth {
            self.chain_metrics.max_reorg_depth = reorg_depth;
        }

        Ok(ReorgResult {
            old_tip: current_tip.block.hash,
            new_tip: target_block_hash,
            reorg_depth,
            blocks_reverted: reorg_depth,
            blocks_applied: new_chain.len() as u64,
            common_ancestor: common_ancestor.block.hash,
        })
    }

    /// Finalize blocks up to the specified height
    pub fn finalize_up_to_height(&mut self, height: u64, pow_header: AuxPowHeader) -> Result<(), ChainError> {
        // Find all blocks up to height in canonical chain
        let mut blocks_to_finalize = vec![];
        
        for (h, snapshot) in self.state_at_height.range(..=height) {
            if let Some(metadata) = self.block_index.get(&snapshot.block.hash) {
                if metadata.is_canonical && !metadata.is_finalized {
                    blocks_to_finalize.push(*h);
                }
            }
        }

        // Mark blocks as finalized
        for h in blocks_to_finalize {
            if let Some(snapshot) = self.state_at_height.get_mut(&h) {
                snapshot.finalization_status = FinalizationStatus::Finalized(pow_header.clone());
                
                if let Some(metadata) = self.block_index.get_mut(&snapshot.block.hash) {
                    metadata.is_finalized = true;
                }
            }
        }

        // Prune old non-canonical branches
        self.prune_non_canonical_branches(height)?;

        self.chain_metrics.finalized_height = height;
        
        Ok(())
    }

    /// Process orphan blocks that may now have parents
    pub fn process_orphan_blocks(&mut self) -> Result<Vec<ProcessedBlock>, ChainError> {
        let mut processed = Vec::new();
        let mut retry_queue = VecDeque::new();

        // Move all orphans to retry queue
        for (hash, block) in self.orphan_pool.drain() {
            retry_queue.push_back((hash, block));
        }

        // Process retry queue until no progress
        let mut made_progress = true;
        while made_progress && !retry_queue.is_empty() {
            made_progress = false;
            let queue_size = retry_queue.len();
            
            for _ in 0..queue_size {
                if let Some((hash, block)) = retry_queue.pop_front() {
                    match self.add_block(block.clone()) {
                        Ok(AddBlockResult::ExtendedChain) | Ok(AddBlockResult::CreatedFork) => {
                            processed.push(ProcessedBlock {
                                hash,
                                result: ProcessBlockResult::Accepted,
                            });
                            made_progress = true;
                        }
                        Ok(AddBlockResult::Orphaned) => {
                            retry_queue.push_back((hash, block));
                        }
                        Ok(AddBlockResult::AlreadyExists) => {
                            // Skip, already processed
                            made_progress = true;
                        }
                        Err(e) => {
                            processed.push(ProcessedBlock {
                                hash,
                                result: ProcessBlockResult::Rejected(e),
                            });
                        }
                    }
                }
            }
        }

        // Put unprocessed blocks back in orphan pool
        for (hash, block) in retry_queue {
            self.orphan_pool.insert(hash, block);
        }
        
        self.chain_metrics.orphan_blocks = self.orphan_pool.len();

        Ok(processed)
    }

    // Helper methods
    fn is_extending_canonical_chain(&self, parent_hash: &Hash256) -> bool {
        if let Some(parent_metadata) = self.block_index.get(parent_hash) {
            parent_metadata.is_canonical
        } else {
            false
        }
    }

    fn create_snapshot_from_parent(
        &self,
        block: &SignedConsensusBlock,
        parent_hash: Hash256,
    ) -> Result<ChainSnapshot, ChainError> {
        // Get parent snapshot
        let parent_metadata = self.block_index.get(&parent_hash)
            .ok_or(ChainError::ParentNotFound)?;
        
        let parent_snapshot = self.state_at_height.get(&parent_metadata.height)
            .ok_or(ChainError::ParentStateNotFound)?;

        // Apply block transitions (simplified)
        let block_ref = BlockRef {
            hash: block.message.hash(),
            number: parent_metadata.height + 1,
            timestamp: block.message.timestamp,
        };

        Ok(ChainSnapshot {
            block: block_ref,
            state_root: block.message.state_root(),
            execution_state: parent_snapshot.execution_state.clone(),
            federation_state: parent_snapshot.federation_state.clone(),
            finalization_status: FinalizationStatus::Unfinalized,
        })
    }

    fn get_canonical_tip(&self) -> Result<ChainSnapshot, ChainError> {
        let max_height = self.state_at_height.keys().max()
            .copied()
            .unwrap_or(0);
        
        self.state_at_height.get(&max_height)
            .cloned()
            .ok_or(ChainError::NoCanonicalTip)
    }

    fn find_common_ancestor(
        &self,
        block_a: Hash256,
        block_b: Hash256,
    ) -> Result<ChainSnapshot, ChainError> {
        // Implementation would trace back from both blocks to find common ancestor
        // For now, return genesis as placeholder
        self.state_at_height.get(&0)
            .cloned()
            .ok_or(ChainError::NoCommonAncestor)
    }

    fn build_chain_to_block(
        &self,
        target: Hash256,
        ancestor: Hash256,
    ) -> Result<Vec<Hash256>, ChainError> {
        // Implementation would build chain from ancestor to target
        // For now, return empty chain
        Ok(vec![])
    }

    fn update_canonical_flags(&mut self, _chain: &[Hash256]) -> Result<(), ChainError> {
        // Implementation would update canonical flags for the new chain
        Ok(())
    }

    fn rebuild_state_from_ancestor(
        &mut self,
        _ancestor: &ChainSnapshot,
        _new_chain: &[Hash256],
    ) -> Result<(), ChainError> {
        // Implementation would rebuild state snapshots for the new chain
        Ok(())
    }

    fn update_canonical_chain(&mut self, _block_hash: Hash256, _height: u64) -> Result<(), ChainError> {
        // Implementation would update canonical chain tracking
        Ok(())
    }

    fn prune_non_canonical_branches(&mut self, finalized_height: u64) -> Result<(), ChainError> {
        let blocks_to_remove: Vec<Hash256> = self.block_index
            .iter()
            .filter(|(_, metadata)| {
                metadata.height <= finalized_height && !metadata.is_canonical
            })
            .map(|(hash, _)| *hash)
            .collect();

        for hash in blocks_to_remove {
            if let Some(metadata) = self.block_index.remove(&hash) {
                self.state_at_height.remove(&metadata.height);
            }
        }

        // Cleanup orphan pool of old blocks
        let orphans_to_remove: Vec<Hash256> = self.orphan_pool
            .iter()
            .filter(|(_, block)| block.message.height() <= finalized_height)
            .map(|(hash, _)| *hash)
            .collect();

        for hash in orphans_to_remove {
            self.orphan_pool.remove(&hash);
        }

        self.chain_metrics.orphan_blocks = self.orphan_pool.len();
        Ok(())
    }
}