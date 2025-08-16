//! Consensus-related types and structures

use crate::types::*;
use serde::{Deserialize, Serialize};

/// Enhanced synchronization progress with parallel download coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    /// Current sync status
    pub status: SyncStatus,
    /// Sync strategy being used
    pub strategy: SyncStrategy,
    /// Parallel download coordination
    pub parallel_coordination: ParallelCoordination,
    /// Performance metrics
    pub performance: SyncPerformanceMetrics,
    /// Error tracking and recovery
    pub error_tracking: SyncErrorTracking,
    /// Peer management for sync
    pub peer_management: SyncPeerManagement,
    /// Checkpoints and milestones
    pub checkpoints: Vec<SyncCheckpoint>,
    /// Resource usage tracking
    pub resource_usage: SyncResourceUsage,
}

/// Synchronization status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    /// Not syncing, fully up to date
    Idle,
    /// Initial sync from genesis
    InitialSync {
        current_block: u64,
        target_block: u64,
        progress: f64,
    },
    /// Fast sync (downloading headers first)
    FastSync {
        current_header: u64,
        target_header: u64,
        current_block: u64,
        header_progress: f64,
        block_progress: f64,
    },
    /// Parallel sync with multiple workers
    ParallelSync {
        workers: Vec<SyncWorker>,
        global_progress: f64,
        coordination_mode: CoordinationMode,
    },
    /// Catching up with recent blocks
    CatchUp {
        current_block: u64,
        target_block: u64,
        behind_by: u64,
    },
    /// Up to date
    UpToDate,
    /// Sync stalled
    Stalled {
        reason: String,
        last_progress: std::time::SystemTime,
        recovery_action: Option<RecoveryAction>,
    },
    /// Sync failed
    Failed {
        error: String,
        failed_at_block: u64,
        retry_count: u32,
    },
}

/// Consensus state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusState {
    pub current_epoch: u64,
    pub current_slot: u64,
    pub finalized_epoch: u64,
    pub finalized_block: BlockRef,
    pub justified_epoch: u64,
    pub justified_block: BlockRef,
}

/// Validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: Address,
    pub public_key: PublicKey,
    pub stake: U256,
    pub is_active: bool,
    pub activation_epoch: u64,
    pub exit_epoch: Option<u64>,
}

/// Validator set for consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSet {
    pub validators: Vec<ValidatorInfo>,
    pub total_stake: U256,
    pub epoch: u64,
}

/// Attestation from validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub validator_index: u64,
    pub slot: u64,
    pub beacon_block_root: BlockHash,
    pub source_epoch: u64,
    pub target_epoch: u64,
    pub signature: Signature,
}

/// Aggregated attestations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateAttestation {
    pub attestation: Attestation,
    pub aggregation_bits: Vec<bool>,
    pub signature: Signature,
}

/// Slashing evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlashingEvidence {
    DoubleVote {
        validator_index: u64,
        vote1: Attestation,
        vote2: Attestation,
    },
    SurroundVote {
        validator_index: u64,
        surrounding: Attestation,
        surrounded: Attestation,
    },
}

/// Fork choice rule implementation
#[derive(Debug, Clone)]
pub struct ForkChoice {
    pub justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,
    pub block_scores: std::collections::HashMap<BlockHash, Score>,
    pub block_tree: BlockTree,
}

/// Checkpoint in consensus
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    pub epoch: u64,
    pub root: BlockHash,
}

/// Block score for fork choice
#[derive(Debug, Clone)]
pub struct Score {
    pub vote_weight: U256,
    pub block_hash: BlockHash,
    pub parent_score: U256,
}

/// Block tree for fork choice
#[derive(Debug, Clone)]
pub struct BlockTree {
    pub blocks: std::collections::HashMap<BlockHash, BlockNode>,
    pub genesis_hash: BlockHash,
}

/// Node in the block tree
#[derive(Debug, Clone)]
pub struct BlockNode {
    pub block_ref: BlockRef,
    pub parent_hash: BlockHash,
    pub children: Vec<BlockHash>,
    pub weight: U256,
    pub justified: bool,
    pub finalized: bool,
}

/// Consensus message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    Block(ConsensusBlock),
    Attestation(Attestation),
    AggregateAttestation(AggregateAttestation),
    SlashingProof(SlashingEvidence),
    SyncCommitteeContribution(SyncCommitteeContribution),
}

/// Sync committee contribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCommitteeContribution {
    pub slot: u64,
    pub beacon_block_root: BlockHash,
    pub subcommittee_index: u64,
    pub aggregation_bits: Vec<bool>,
    pub signature: Signature,
}

/// Consensus error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusError {
    InvalidBlock { reason: String },
    InvalidAttestation { reason: String },
    SlashableOffense { evidence: SlashingEvidence },
    ForkChoiceError { reason: String },
    InvalidSignature,
    UnknownValidator { validator_index: u64 },
    InsufficientStake,
    EpochTooOld { epoch: u64 },
    DuplicateAttestation,
}

/// Finalization status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinalizationStatus {
    Unfinalized,
    Justified {
        epoch: u64,
        checkpoint: Checkpoint,
    },
    Finalized {
        epoch: u64,
        checkpoint: Checkpoint,
        finalized_at: std::time::SystemTime,
    },
}

/// Consensus metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusMetrics {
    pub current_epoch: u64,
    pub finalized_epoch: u64,
    pub participation_rate: f64,
    pub attestation_inclusion_distance: f64,
    pub validator_count: u64,
    pub active_validator_count: u64,
    pub total_stake: U256,
    pub average_block_time: std::time::Duration,
}

/// Proof of Work related types (for auxiliary PoW)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryProofOfWork {
    pub parent_block: BlockHash,
    pub coinbase_tx: Vec<u8>,
    pub merkle_branch: Vec<Hash256>,
    pub merkle_index: u32,
    pub parent_block_header: Vec<u8>,
}

/// PoW validation result
#[derive(Debug, Clone)]
pub struct PoWValidationResult {
    pub valid: bool,
    pub target: U256,
    pub hash: Hash256,
    pub difficulty: U256,
}

impl SyncStatus {
    /// Check if currently syncing
    pub fn is_syncing(&self) -> bool {
        matches!(self, 
            SyncStatus::InitialSync { .. } | 
            SyncStatus::FastSync { .. } | 
            SyncStatus::ParallelSync { .. } |
            SyncStatus::CatchUp { .. }
        )
    }
    
    /// Get sync progress (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        match self {
            SyncStatus::InitialSync { progress, .. } => *progress,
            SyncStatus::FastSync { block_progress, .. } => *block_progress,
            SyncStatus::ParallelSync { global_progress, .. } => *global_progress,
            SyncStatus::CatchUp { current_block, target_block, .. } => {
                if *target_block > 0 {
                    (*current_block as f64) / (*target_block as f64)
                } else {
                    0.0
                }
            }
            SyncStatus::UpToDate => 1.0,
            _ => 0.0,
        }
    }
    
    /// Get estimated blocks remaining
    pub fn blocks_remaining(&self) -> Option<u64> {
        match self {
            SyncStatus::InitialSync { current_block, target_block, .. } => {
                Some(target_block.saturating_sub(*current_block))
            }
            SyncStatus::FastSync { current_block, target_header, .. } => {
                Some(target_header.saturating_sub(*current_block))
            }
            SyncStatus::CatchUp { behind_by, .. } => Some(*behind_by),
            _ => None,
        }
    }

    /// Check if sync has failed
    pub fn is_failed(&self) -> bool {
        matches!(self, SyncStatus::Failed { .. })
    }

    /// Check if sync is stalled
    pub fn is_stalled(&self) -> bool {
        matches!(self, SyncStatus::Stalled { .. })
    }

    /// Get sync status description
    pub fn description(&self) -> String {
        match self {
            SyncStatus::Idle => "Idle - no sync needed".to_string(),
            SyncStatus::InitialSync { current_block, target_block, progress } => {
                format!("Initial sync: {}/{} blocks ({:.1}%)", current_block, target_block, progress * 100.0)
            }
            SyncStatus::FastSync { current_header, target_header, header_progress, block_progress } => {
                format!("Fast sync: Headers {}/{} ({:.1}%), Blocks ({:.1}%)", 
                       current_header, target_header, header_progress * 100.0, block_progress * 100.0)
            }
            SyncStatus::ParallelSync { workers, global_progress, .. } => {
                format!("Parallel sync: {} workers, {:.1}% complete", workers.len(), global_progress * 100.0)
            }
            SyncStatus::CatchUp { behind_by, .. } => {
                format!("Catching up: {} blocks behind", behind_by)
            }
            SyncStatus::UpToDate => "Up to date".to_string(),
            SyncStatus::Stalled { reason, .. } => {
                format!("Stalled: {}", reason)
            }
            SyncStatus::Failed { error, .. } => {
                format!("Failed: {}", error)
            }
        }
    }
}

impl ValidatorSet {
    /// Create new validator set
    pub fn new(validators: Vec<ValidatorInfo>, epoch: u64) -> Self {
        let total_stake = validators
            .iter()
            .filter(|v| v.is_active)
            .map(|v| v.stake)
            .sum();
            
        Self {
            validators,
            total_stake,
            epoch,
        }
    }
    
    /// Get active validators
    pub fn active_validators(&self) -> Vec<&ValidatorInfo> {
        self.validators
            .iter()
            .filter(|v| v.is_active)
            .collect()
    }
    
    /// Get validator by index
    pub fn get_validator(&self, index: u64) -> Option<&ValidatorInfo> {
        self.validators.get(index as usize)
    }
    
    /// Check if validator exists and is active
    pub fn is_active_validator(&self, address: &Address) -> bool {
        self.validators
            .iter()
            .any(|v| v.address == *address && v.is_active)
    }
    
    /// Get validator count
    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }
    
    /// Get active validator count
    pub fn active_validator_count(&self) -> usize {
        self.validators.iter().filter(|v| v.is_active).count()
    }
}

impl ValidatorInfo {
    /// Create new validator info
    pub fn new(
        address: Address,
        public_key: PublicKey,
        stake: U256,
        activation_epoch: u64,
    ) -> Self {
        Self {
            address,
            public_key,
            stake,
            is_active: true,
            activation_epoch,
            exit_epoch: None,
        }
    }
    
    /// Check if validator is active at given epoch
    pub fn is_active_at_epoch(&self, epoch: u64) -> bool {
        self.is_active 
            && epoch >= self.activation_epoch
            && self.exit_epoch.map_or(true, |exit| epoch < exit)
    }
    
    /// Get effective balance (may be different from stake)
    pub fn effective_balance(&self) -> U256 {
        // For now, effective balance equals stake
        // In practice, this might be capped or adjusted
        self.stake
    }
}

impl Attestation {
    /// Create new attestation
    pub fn new(
        validator_index: u64,
        slot: u64,
        beacon_block_root: BlockHash,
        source_epoch: u64,
        target_epoch: u64,
    ) -> Self {
        Self {
            validator_index,
            slot,
            beacon_block_root,
            source_epoch,
            target_epoch,
            signature: [0u8; 64], // Will be filled during signing
        }
    }
    
    /// Check if attestation is slashable with another
    pub fn is_slashable_with(&self, other: &Attestation) -> bool {
        // Double vote: same target epoch, different beacon block roots
        if self.target_epoch == other.target_epoch 
            && self.beacon_block_root != other.beacon_block_root {
            return true;
        }
        
        // Surround vote: one attestation surrounds the other
        if (self.source_epoch < other.source_epoch && self.target_epoch > other.target_epoch)
            || (other.source_epoch < self.source_epoch && other.target_epoch > self.target_epoch) {
            return true;
        }
        
        false
    }
}

impl ForkChoice {
    /// Create new fork choice instance
    pub fn new(genesis_hash: BlockHash) -> Self {
        let genesis_checkpoint = Checkpoint {
            epoch: 0,
            root: genesis_hash,
        };
        
        let mut block_tree = BlockTree {
            blocks: std::collections::HashMap::new(),
            genesis_hash,
        };
        
        // Add genesis block
        block_tree.blocks.insert(genesis_hash, BlockNode {
            block_ref: BlockRef::genesis(genesis_hash),
            parent_hash: BlockHash::zero(),
            children: Vec::new(),
            weight: U256::zero(),
            justified: true,
            finalized: true,
        });
        
        Self {
            justified_checkpoint: genesis_checkpoint.clone(),
            finalized_checkpoint: genesis_checkpoint,
            block_scores: std::collections::HashMap::new(),
            block_tree,
        }
    }
    
    /// Get head block according to fork choice rule
    pub fn get_head(&self) -> BlockHash {
        // Simplified GHOST rule: choose the block with highest weight
        // among children of finalized block
        self.find_head_recursive(self.finalized_checkpoint.root)
    }
    
    /// Apply attestation to fork choice
    pub fn apply_attestation(&mut self, attestation: &Attestation) {
        // Update block weights based on attestation
        if let Some(node) = self.block_tree.blocks.get_mut(&attestation.beacon_block_root) {
            node.weight += U256::one(); // Simplified: each attestation adds 1 weight
        }
    }
    
    /// Add block to fork choice
    pub fn add_block(&mut self, block_ref: BlockRef) {
        let node = BlockNode {
            block_ref: block_ref.clone(),
            parent_hash: block_ref.parent_hash,
            children: Vec::new(),
            weight: U256::zero(),
            justified: false,
            finalized: false,
        };
        
        // Add as child to parent
        if let Some(parent) = self.block_tree.blocks.get_mut(&block_ref.parent_hash) {
            parent.children.push(block_ref.hash);
        }
        
        self.block_tree.blocks.insert(block_ref.hash, node);
    }
    
    /// Recursive head finding using GHOST rule
    fn find_head_recursive(&self, block_hash: BlockHash) -> BlockHash {
        if let Some(node) = self.block_tree.blocks.get(&block_hash) {
            if node.children.is_empty() {
                return block_hash;
            }
            
            // Find child with highest weight
            let best_child = node.children
                .iter()
                .max_by_key(|&child_hash| {
                    self.block_tree.blocks
                        .get(child_hash)
                        .map(|child| child.weight)
                        .unwrap_or(U256::zero())
                })
                .copied()
                .unwrap_or(block_hash);
                
            return self.find_head_recursive(best_child);
        }
        
        block_hash
    }
}

impl ConsensusMetrics {
    /// Create new consensus metrics
    pub fn new() -> Self {
        Self {
            current_epoch: 0,
            finalized_epoch: 0,
            participation_rate: 0.0,
            attestation_inclusion_distance: 0.0,
            validator_count: 0,
            active_validator_count: 0,
            total_stake: U256::zero(),
            average_block_time: std::time::Duration::from_secs(12),
        }
    }
    
    /// Update participation rate
    pub fn update_participation_rate(&mut self, expected: u64, actual: u64) {
        if expected > 0 {
            self.participation_rate = (actual as f64) / (expected as f64);
        }
    }
    
    /// Check if consensus is healthy
    pub fn is_healthy(&self) -> bool {
        self.participation_rate > 0.67 && // More than 2/3 participation
        self.current_epoch - self.finalized_epoch < 3 // Finality not too far behind
    }
}

impl Default for ConsensusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Sync strategy types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStrategy {
    /// Sequential block download
    Sequential {
        batch_size: u32,
        max_concurrent_requests: u32,
    },
    /// Parallel download with coordinated workers
    Parallel {
        worker_count: u32,
        chunk_size: u32,
        overlap_threshold: u32,
    },
    /// Fast sync (headers first, then bodies)
    FastSync {
        header_batch_size: u32,
        body_batch_size: u32,
        state_sync_enabled: bool,
    },
    /// Adaptive strategy based on network conditions
    Adaptive {
        initial_strategy: Box<SyncStrategy>,
        adaptation_threshold: f64,
        performance_window: std::time::Duration,
    },
}

/// Parallel download coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelCoordination {
    /// Active sync workers
    pub workers: Vec<SyncWorker>,
    /// Work distribution strategy
    pub distribution_strategy: WorkDistributionStrategy,
    /// Coordination state
    pub coordination_state: CoordinationState,
    /// Load balancing configuration
    pub load_balancing: LoadBalancingConfig,
    /// Conflict resolution
    pub conflict_resolution: ConflictResolutionStrategy,
}

/// Individual sync worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncWorker {
    /// Worker identifier
    pub worker_id: String,
    /// Assigned block range
    pub assigned_range: BlockRange,
    /// Current status
    pub status: WorkerStatus,
    /// Assigned peer for this worker
    pub peer_id: Option<PeerId>,
    /// Performance metrics
    pub performance: WorkerPerformance,
    /// Current progress
    pub progress: f64,
    /// Last activity timestamp
    pub last_activity: std::time::SystemTime,
}

/// Block range assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRange {
    /// Starting block number (inclusive)
    pub start: u64,
    /// Ending block number (inclusive)
    pub end: u64,
    /// Priority level
    pub priority: RangePriority,
    /// Retry count for this range
    pub retry_count: u32,
}

/// Worker status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkerStatus {
    /// Worker is idle
    Idle,
    /// Worker is downloading blocks
    Downloading { current_block: u64, blocks_remaining: u64 },
    /// Worker is processing downloaded blocks
    Processing { blocks_processed: u32, total_blocks: u32 },
    /// Worker encountered an error
    Error { error: String, retry_at: Option<std::time::SystemTime> },
    /// Worker completed its assignment
    Completed { blocks_downloaded: u64, duration: std::time::Duration },
}

/// Range priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum RangePriority {
    /// Low priority background sync
    Low,
    /// Normal priority sync
    Normal,
    /// High priority (recent blocks)
    High,
    /// Critical priority (tip blocks)
    Critical,
}

/// Worker performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPerformance {
    /// Download speed (blocks per second)
    pub download_speed: f64,
    /// Processing speed (blocks per second)
    pub processing_speed: f64,
    /// Error rate
    pub error_rate: f64,
    /// Average latency
    pub average_latency: std::time::Duration,
    /// Success rate percentage
    pub success_rate: f64,
}

/// Work distribution strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkDistributionStrategy {
    /// Equal ranges for all workers
    EqualDistribution,
    /// Performance-based distribution
    PerformanceBased { adjustment_factor: f64 },
    /// Priority-based distribution
    PriorityBased { critical_worker_count: u32 },
    /// Dynamic rebalancing
    Dynamic { rebalance_interval: std::time::Duration },
}

/// Coordination modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CoordinationMode {
    /// Independent workers with minimal coordination
    Independent,
    /// Coordinated with central scheduler
    Centralized,
    /// Peer-to-peer coordination between workers
    Distributed,
    /// Hybrid approach
    Hybrid,
}

/// Coordination state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationState {
    /// Global sync progress
    pub global_progress: f64,
    /// Coordination overhead metrics
    pub coordination_overhead: f64,
    /// Active coordination messages
    pub active_messages: u32,
    /// Last coordination update
    pub last_update: std::time::SystemTime,
}

/// Load balancing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancingConfig {
    /// Enable automatic load balancing
    pub enabled: bool,
    /// Rebalancing threshold (performance difference %)
    pub rebalance_threshold: f64,
    /// Minimum time between rebalances
    pub min_rebalance_interval: std::time::Duration,
    /// Maximum range size for single worker
    pub max_range_size: u64,
}

/// Conflict resolution strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolutionStrategy {
    /// First worker wins
    FirstWins,
    /// Fastest worker wins
    FastestWins,
    /// Majority consensus
    MajorityConsensus,
    /// Quality-based selection
    QualityBased,
}

/// Sync performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPerformanceMetrics {
    /// Overall sync speed (blocks per second)
    pub sync_speed: f64,
    /// Network throughput (bytes per second)
    pub network_throughput: u64,
    /// CPU utilization percentage
    pub cpu_utilization: f64,
    /// Memory usage (bytes)
    pub memory_usage: u64,
    /// Disk I/O rate (operations per second)
    pub disk_io_rate: f64,
    /// Average block processing time
    pub avg_block_processing_time: std::time::Duration,
    /// Time to sync estimate
    pub estimated_time_remaining: Option<std::time::Duration>,
}

/// Sync error tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncErrorTracking {
    /// Recent errors
    pub recent_errors: Vec<SyncError>,
    /// Error patterns detected
    pub error_patterns: Vec<ErrorPattern>,
    /// Recovery attempts
    pub recovery_attempts: Vec<RecoveryAttempt>,
    /// Error rate over time
    pub error_rate_history: Vec<(std::time::SystemTime, f64)>,
}

/// Sync error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncError {
    /// Error message
    pub error: String,
    /// Error type
    pub error_type: SyncErrorType,
    /// When error occurred
    pub timestamp: std::time::SystemTime,
    /// Affected block range
    pub affected_range: Option<BlockRange>,
    /// Associated peer
    pub peer_id: Option<PeerId>,
    /// Worker that encountered the error
    pub worker_id: Option<String>,
}

/// Types of sync errors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncErrorType {
    /// Network connectivity error
    NetworkError,
    /// Invalid block received
    InvalidBlock,
    /// Timeout error
    Timeout,
    /// Peer misbehavior
    PeerMisbehavior,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Database error
    DatabaseError,
    /// Validation error
    ValidationError,
}

/// Error pattern detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    /// Pattern type
    pub pattern_type: ErrorPatternType,
    /// Frequency of occurrence
    pub frequency: u32,
    /// Time window for pattern
    pub time_window: std::time::Duration,
    /// Suggested action
    pub suggested_action: RecoveryAction,
}

/// Types of error patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorPatternType {
    /// Repeated timeout from specific peer
    RepeatedTimeout { peer_id: PeerId },
    /// Cascading failures
    CascadingFailures,
    /// Resource exhaustion pattern
    ResourceExhaustion,
    /// Invalid block pattern
    InvalidBlockPattern,
}

/// Recovery attempt tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttempt {
    /// Recovery action taken
    pub action: RecoveryAction,
    /// When attempt was made
    pub attempted_at: std::time::SystemTime,
    /// Success of the attempt
    pub success: Option<bool>,
    /// Time taken for recovery
    pub duration: Option<std::time::Duration>,
}

/// Recovery actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Retry with same configuration
    Retry,
    /// Change sync strategy
    ChangeStrategy(SyncStrategy),
    /// Switch to different peer
    SwitchPeer,
    /// Reduce worker count
    ReduceWorkers(u32),
    /// Reset sync progress
    Reset,
    /// Pause sync temporarily
    Pause(std::time::Duration),
}

/// Peer management for sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPeerManagement {
    /// Available peers for sync
    pub available_peers: Vec<SyncPeer>,
    /// Peer selection strategy
    pub selection_strategy: PeerSelectionStrategy,
    /// Peer performance tracking
    pub peer_performance: std::collections::HashMap<PeerId, PeerPerformance>,
    /// Blacklisted peers
    pub blacklisted_peers: Vec<PeerId>,
}

/// Sync peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPeer {
    /// Peer identifier
    pub peer_id: PeerId,
    /// Peer's best block
    pub best_block: u64,
    /// Peer capabilities
    pub capabilities: PeerCapabilities,
    /// Connection quality
    pub connection_quality: ConnectionQuality,
    /// Current assignment
    pub assignment: Option<String>, // Worker ID
}

/// Peer capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    /// Maximum concurrent requests supported
    pub max_concurrent_requests: u32,
    /// Supports fast sync
    pub supports_fast_sync: bool,
    /// Maximum batch size
    pub max_batch_size: u32,
    /// Supported block ranges
    pub supported_ranges: Vec<BlockRange>,
}

/// Connection quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionQuality {
    /// Latency to peer
    pub latency: std::time::Duration,
    /// Bandwidth estimate
    pub bandwidth_estimate: u64,
    /// Reliability score (0.0 to 1.0)
    pub reliability: f64,
    /// Last measured at
    pub last_measured: std::time::SystemTime,
}

/// Peer performance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPerformance {
    /// Average response time
    pub avg_response_time: std::time::Duration,
    /// Success rate
    pub success_rate: f64,
    /// Blocks delivered
    pub blocks_delivered: u64,
    /// Errors encountered
    pub error_count: u32,
    /// Last interaction
    pub last_interaction: std::time::SystemTime,
}

/// Peer selection strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerSelectionStrategy {
    /// Random selection
    Random,
    /// Best performance first
    BestPerformance,
    /// Round-robin
    RoundRobin,
    /// Weighted selection based on performance
    WeightedPerformance,
}

/// Sync checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    /// Checkpoint block number
    pub block_number: u64,
    /// Checkpoint hash
    pub block_hash: BlockHash,
    /// When checkpoint was reached
    pub timestamp: std::time::SystemTime,
    /// Verification status
    pub verified: bool,
    /// Checkpoint type
    pub checkpoint_type: CheckpointType,
}

/// Types of sync checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointType {
    /// Regular progress checkpoint
    Progress,
    /// Milestone checkpoint (e.g., every 10k blocks)
    Milestone,
    /// Finality checkpoint
    Finality,
    /// User-defined checkpoint
    UserDefined,
}

/// Resource usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResourceUsage {
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Disk usage in bytes
    pub disk_usage: u64,
    /// Network bandwidth usage (bytes/sec)
    pub network_usage: u64,
    /// Resource usage history
    pub usage_history: Vec<ResourceSnapshot>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
}

/// Resource snapshot at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    /// Snapshot timestamp
    pub timestamp: std::time::SystemTime,
    /// CPU usage at this time
    pub cpu_usage: f64,
    /// Memory usage at this time
    pub memory_usage: u64,
    /// Network usage at this time
    pub network_usage: u64,
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum CPU usage percentage
    pub max_cpu_usage: f64,
    /// Maximum memory usage in bytes
    pub max_memory_usage: u64,
    /// Maximum network bandwidth (bytes/sec)
    pub max_network_bandwidth: u64,
    /// Maximum disk I/O rate
    pub max_disk_io_rate: f64,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            status: SyncStatus::Idle,
            strategy: SyncStrategy::default(),
            parallel_coordination: ParallelCoordination::default(),
            performance: SyncPerformanceMetrics::default(),
            error_tracking: SyncErrorTracking::default(),
            peer_management: SyncPeerManagement::default(),
            checkpoints: Vec::new(),
            resource_usage: SyncResourceUsage::default(),
        }
    }
}

impl Default for SyncStrategy {
    fn default() -> Self {
        SyncStrategy::Sequential {
            batch_size: 64,
            max_concurrent_requests: 8,
        }
    }
}

impl Default for ParallelCoordination {
    fn default() -> Self {
        Self {
            workers: Vec::new(),
            distribution_strategy: WorkDistributionStrategy::EqualDistribution,
            coordination_state: CoordinationState::default(),
            load_balancing: LoadBalancingConfig::default(),
            conflict_resolution: ConflictResolutionStrategy::FastestWins,
        }
    }
}

impl Default for CoordinationState {
    fn default() -> Self {
        Self {
            global_progress: 0.0,
            coordination_overhead: 0.0,
            active_messages: 0,
            last_update: std::time::SystemTime::now(),
        }
    }
}

impl Default for LoadBalancingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rebalance_threshold: 0.2, // 20% performance difference
            min_rebalance_interval: std::time::Duration::from_secs(30),
            max_range_size: 1000,
        }
    }
}

impl Default for SyncPerformanceMetrics {
    fn default() -> Self {
        Self {
            sync_speed: 0.0,
            network_throughput: 0,
            cpu_utilization: 0.0,
            memory_usage: 0,
            disk_io_rate: 0.0,
            avg_block_processing_time: std::time::Duration::from_millis(100),
            estimated_time_remaining: None,
        }
    }
}

impl Default for SyncErrorTracking {
    fn default() -> Self {
        Self {
            recent_errors: Vec::new(),
            error_patterns: Vec::new(),
            recovery_attempts: Vec::new(),
            error_rate_history: Vec::new(),
        }
    }
}

impl Default for SyncPeerManagement {
    fn default() -> Self {
        Self {
            available_peers: Vec::new(),
            selection_strategy: PeerSelectionStrategy::BestPerformance,
            peer_performance: std::collections::HashMap::new(),
            blacklisted_peers: Vec::new(),
        }
    }
}

impl Default for SyncResourceUsage {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0,
            disk_usage: 0,
            network_usage: 0,
            usage_history: Vec::new(),
            resource_limits: ResourceLimits::default(),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_usage: 80.0, // 80% max CPU usage
            max_memory_usage: 4 * 1024 * 1024 * 1024, // 4GB max memory
            max_network_bandwidth: 100 * 1024 * 1024, // 100MB/s max bandwidth
            max_disk_io_rate: 1000.0, // 1000 operations per second
        }
    }
}

impl SyncProgress {
    /// Create new sync progress tracker
    pub fn new(strategy: SyncStrategy) -> Self {
        Self {
            strategy,
            ..Default::default()
        }
    }

    /// Update sync status
    pub fn update_status(&mut self, status: SyncStatus) {
        self.status = status;
    }

    /// Get overall progress (0.0 to 1.0)
    pub fn overall_progress(&self) -> f64 {
        self.status.progress()
    }

    /// Add sync error
    pub fn add_error(&mut self, error: SyncError) {
        self.error_tracking.recent_errors.push(error);
        
        // Limit recent errors to last 100
        if self.error_tracking.recent_errors.len() > 100 {
            self.error_tracking.recent_errors.drain(0..50);
        }
        
        // Update error rate history
        let now = std::time::SystemTime::now();
        let error_rate = self.calculate_error_rate();
        self.error_tracking.error_rate_history.push((now, error_rate));
    }

    /// Calculate current error rate
    fn calculate_error_rate(&self) -> f64 {
        if self.error_tracking.recent_errors.is_empty() {
            return 0.0;
        }

        let now = std::time::SystemTime::now();
        let one_hour_ago = now - std::time::Duration::from_secs(3600);
        
        let recent_errors = self.error_tracking.recent_errors
            .iter()
            .filter(|e| e.timestamp >= one_hour_ago)
            .count();

        // Normalize to errors per hour
        recent_errors as f64
    }

    /// Add checkpoint
    pub fn add_checkpoint(&mut self, checkpoint: SyncCheckpoint) {
        self.checkpoints.push(checkpoint);
        
        // Keep only last 1000 checkpoints
        if self.checkpoints.len() > 1000 {
            self.checkpoints.drain(0..100);
        }
    }

    /// Get sync health assessment
    pub fn health_assessment(&self) -> SyncHealthAssessment {
        let error_rate = self.calculate_error_rate();
        let resource_health = self.assess_resource_health();
        let peer_health = self.assess_peer_health();
        
        SyncHealthAssessment {
            overall_health: if error_rate < 1.0 && resource_health && peer_health {
                SyncHealth::Healthy
            } else if error_rate < 5.0 {
                SyncHealth::Warning
            } else {
                SyncHealth::Critical
            },
            error_rate,
            resource_health_ok: resource_health,
            peer_health_ok: peer_health,
            performance_score: self.calculate_performance_score(),
        }
    }

    /// Assess resource health
    fn assess_resource_health(&self) -> bool {
        let limits = &self.resource_usage.resource_limits;
        self.resource_usage.cpu_usage < limits.max_cpu_usage &&
        self.resource_usage.memory_usage < limits.max_memory_usage &&
        self.resource_usage.network_usage < limits.max_network_bandwidth
    }

    /// Assess peer health
    fn assess_peer_health(&self) -> bool {
        !self.peer_management.available_peers.is_empty() &&
        self.peer_management.available_peers.len() > self.peer_management.blacklisted_peers.len()
    }

    /// Calculate performance score (0.0 to 1.0)
    fn calculate_performance_score(&self) -> f64 {
        let base_score = self.performance.sync_speed / 100.0; // Assume 100 blocks/sec is perfect
        let error_penalty = self.calculate_error_rate() / 10.0; // Penalize for errors
        let resource_bonus = if self.assess_resource_health() { 0.1 } else { -0.2 };
        
        (base_score - error_penalty + resource_bonus).clamp(0.0, 1.0)
    }

    /// Suggest recovery action based on current state
    pub fn suggest_recovery_action(&self) -> Option<RecoveryAction> {
        match &self.status {
            SyncStatus::Failed { retry_count, .. } if *retry_count < 3 => {
                Some(RecoveryAction::Retry)
            }
            SyncStatus::Stalled { .. } => {
                if self.parallel_coordination.workers.len() > 1 {
                    Some(RecoveryAction::ReduceWorkers(1))
                } else {
                    Some(RecoveryAction::SwitchPeer)
                }
            }
            _ if self.calculate_error_rate() > 5.0 => {
                Some(RecoveryAction::ChangeStrategy(SyncStrategy::Sequential {
                    batch_size: 32,
                    max_concurrent_requests: 4,
                }))
            }
            _ => None
        }
    }
}

impl SyncWorker {
    /// Create new sync worker
    pub fn new(worker_id: String, assigned_range: BlockRange) -> Self {
        Self {
            worker_id,
            assigned_range,
            status: WorkerStatus::Idle,
            peer_id: None,
            performance: WorkerPerformance::default(),
            progress: 0.0,
            last_activity: std::time::SystemTime::now(),
        }
    }

    /// Update worker progress
    pub fn update_progress(&mut self, current_block: u64) {
        let total_blocks = self.assigned_range.end - self.assigned_range.start + 1;
        let completed_blocks = current_block.saturating_sub(self.assigned_range.start);
        self.progress = (completed_blocks as f64) / (total_blocks as f64);
        self.last_activity = std::time::SystemTime::now();
    }

    /// Check if worker is healthy (active within threshold)
    pub fn is_healthy(&self, timeout: std::time::Duration) -> bool {
        self.last_activity.elapsed().unwrap_or_default() < timeout
    }
}

impl Default for WorkerPerformance {
    fn default() -> Self {
        Self {
            download_speed: 0.0,
            processing_speed: 0.0,
            error_rate: 0.0,
            average_latency: std::time::Duration::from_millis(100),
            success_rate: 1.0,
        }
    }
}

impl BlockRange {
    /// Create new block range
    pub fn new(start: u64, end: u64, priority: RangePriority) -> Self {
        Self {
            start,
            end,
            priority,
            retry_count: 0,
        }
    }

    /// Get range size
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }

    /// Split range into smaller chunks
    pub fn split(&self, chunk_size: u64) -> Vec<BlockRange> {
        let mut ranges = Vec::new();
        let mut current = self.start;
        
        while current <= self.end {
            let chunk_end = (current + chunk_size - 1).min(self.end);
            ranges.push(BlockRange::new(current, chunk_end, self.priority.clone()));
            current = chunk_end + 1;
        }
        
        ranges
    }

    /// Check if ranges overlap
    pub fn overlaps(&self, other: &BlockRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// Sync health assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHealthAssessment {
    /// Overall health status
    pub overall_health: SyncHealth,
    /// Current error rate
    pub error_rate: f64,
    /// Resource health OK
    pub resource_health_ok: bool,
    /// Peer health OK
    pub peer_health_ok: bool,
    /// Performance score (0.0 to 1.0)
    pub performance_score: f64,
}

/// Sync health levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncHealth {
    /// Sync is operating normally
    Healthy,
    /// Sync has some issues but is functional
    Warning,
    /// Sync has critical issues
    Critical,
}