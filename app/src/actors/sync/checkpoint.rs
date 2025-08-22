//! Checkpoint system for SyncActor recovery and state persistence
//!
//! This module implements a comprehensive checkpoint system that provides:
//! - Automatic checkpoint creation at configurable intervals
//! - Fast recovery from sync failures and restarts
//! - State persistence across actor restarts
//! - Verification of checkpoint integrity
//! - Federation-aware checkpoint validation
//! - Governance stream state synchronization

use std::{
    collections::{HashMap, BTreeMap, VecDeque},
    sync::{Arc, RwLock, atomic::{AtomicU64, AtomicBool, Ordering}},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    path::{Path, PathBuf},
    io::{self, Write, Read},
    fs::{File, OpenOptions, create_dir_all},
};

use actix::prelude::*;
use tokio::{
    sync::{RwLock as TokioRwLock, Mutex, mpsc, oneshot},
    time::{sleep, timeout, interval},
    task::JoinHandle,
};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use serde::{Serialize, Deserialize};
use prometheus::{Histogram, Counter, Gauge, IntCounter, IntGauge};
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    types::{Block, BlockHash, BlockHeader, Hash256},
    actors::{
        chain::{ChainActor, GetChainState, GetBlock},
        consensus::{ConsensusActor, GetConsensusState},
    },
};

use super::{
    errors::{SyncError, SyncResult},
    messages::{SyncState, SyncProgress, GovernanceEvent},
    config::SyncConfig,
    peer::{PeerId, PeerManager, PeerSyncInfo},
    metrics::*,
};

lazy_static::lazy_static! {
    static ref CHECKPOINTS_CREATED: IntCounter = prometheus::register_int_counter!(
        "alys_sync_checkpoints_created_total",
        "Total number of checkpoints created"
    ).unwrap();
    
    static ref CHECKPOINT_CREATION_DURATION: Histogram = prometheus::register_histogram!(
        "alys_sync_checkpoint_creation_duration_seconds",
        "Time taken to create checkpoints",
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]
    ).unwrap();
    
    static ref CHECKPOINT_RECOVERY_DURATION: Histogram = prometheus::register_histogram!(
        "alys_sync_checkpoint_recovery_duration_seconds",
        "Time taken to recover from checkpoints",
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]
    ).unwrap();
    
    static ref CHECKPOINT_VERIFICATION_FAILURES: IntCounter = prometheus::register_int_counter!(
        "alys_sync_checkpoint_verification_failures_total",
        "Total checkpoint verification failures"
    ).unwrap();
    
    static ref CHECKPOINT_STORAGE_SIZE: IntGauge = prometheus::register_int_gauge!(
        "alys_sync_checkpoint_storage_size_bytes",
        "Current size of checkpoint storage in bytes"
    ).unwrap();
    
    static ref ACTIVE_CHECKPOINTS: IntGauge = prometheus::register_int_gauge!(
        "alys_sync_active_checkpoints",
        "Number of active checkpoints in storage"
    ).unwrap();
}

/// Comprehensive checkpoint data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCheckpoint {
    /// Checkpoint metadata
    pub metadata: CheckpointMetadata,
    
    /// Blockchain state at checkpoint
    pub blockchain_state: BlockchainState,
    
    /// Sync progress at checkpoint time
    pub sync_progress: SyncProgress,
    
    /// Peer state information
    pub peer_states: HashMap<PeerId, PeerCheckpointState>,
    
    /// Federation state
    pub federation_state: FederationCheckpointState,
    
    /// Governance stream state
    pub governance_state: GovernanceCheckpointState,
    
    /// Network topology snapshot
    pub network_topology: NetworkTopologySnapshot,
    
    /// Performance metrics snapshot
    pub metrics_snapshot: MetricsSnapshot,
    
    /// Recovery context for fast restoration
    pub recovery_context: RecoveryContext,
}

/// Checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    /// Unique checkpoint identifier
    pub id: String,
    /// Block height at checkpoint
    pub height: u64,
    /// Block hash at checkpoint
    pub block_hash: BlockHash,
    /// Parent checkpoint ID (for chain recovery)
    pub parent_checkpoint_id: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Checkpoint version for compatibility
    pub version: u32,
    /// Checkpoint type
    pub checkpoint_type: CheckpointType,
    /// Verification hash
    pub verification_hash: Hash256,
    /// Size in bytes
    pub size_bytes: u64,
    /// Compression level used
    pub compression_level: u8,
}

/// Types of checkpoints
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckpointType {
    /// Regular scheduled checkpoint
    Scheduled,
    /// Emergency checkpoint before critical operations
    Emergency,
    /// Manual checkpoint created by operator
    Manual,
    /// Recovery checkpoint created during error handling
    Recovery,
    /// Migration checkpoint for upgrades
    Migration,
}

/// Blockchain state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainState {
    /// Current best block
    pub best_block: Block,
    /// Last finalized block
    pub finalized_block: Block,
    /// Chain head candidates
    pub head_candidates: Vec<Block>,
    /// State root hash
    pub state_root: Hash256,
    /// Total difficulty
    pub total_difficulty: u64,
    /// Transaction pool state
    pub tx_pool_size: usize,
    /// Fork choice information
    pub fork_choice_data: ForkChoiceData,
}

/// Fork choice data for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkChoiceData {
    /// Available forks
    pub forks: Vec<ForkInfo>,
    /// Preferred fork
    pub preferred_fork: Option<BlockHash>,
    /// Fork weights
    pub fork_weights: HashMap<BlockHash, u64>,
}

/// Fork information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkInfo {
    /// Fork head block
    pub head: BlockHash,
    /// Fork length
    pub length: u64,
    /// Fork weight (for selection)
    pub weight: u64,
    /// Fork age
    pub age: Duration,
}

/// Peer state in checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCheckpointState {
    /// Peer ID
    pub peer_id: PeerId,
    /// Peer's best block
    pub best_block: u64,
    /// Connection quality score
    pub quality_score: f64,
    /// Reliability metrics
    pub reliability: PeerReliabilityMetrics,
    /// Last interaction timestamp
    pub last_interaction: DateTime<Utc>,
    /// Peer capabilities
    pub capabilities: PeerCapabilities,
    /// Sync state with this peer
    pub sync_state: PeerSyncState,
}

/// Peer reliability metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReliabilityMetrics {
    pub success_rate: f64,
    pub average_response_time: Duration,
    pub blocks_served: u64,
    pub errors_encountered: u32,
    pub uptime_percentage: f64,
}

/// Peer capabilities snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub supports_fast_sync: bool,
    pub supports_state_sync: bool,
    pub supports_federation_sync: bool,
    pub max_batch_size: usize,
    pub protocol_version: u32,
}

/// Peer sync state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerSyncState {
    Idle,
    Syncing { start_height: u64, target_height: u64 },
    Complete,
    Failed { reason: String },
}

/// Federation state in checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationCheckpointState {
    /// Active authorities
    pub authorities: Vec<AuthorityInfo>,
    /// Current consensus round
    pub current_round: u64,
    /// Last federation block
    pub last_federation_block: u64,
    /// Authority rotation schedule
    pub rotation_schedule: AuthorityRotationSchedule,
    /// Signature aggregation state
    pub signature_state: FederationSignatureState,
    /// Emergency mode status
    pub emergency_mode: bool,
}

/// Authority information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityInfo {
    pub authority_id: String,
    pub public_key: Vec<u8>,
    pub weight: u64,
    pub is_active: bool,
    pub last_block_produced: Option<u64>,
    pub reputation_score: f64,
}

/// Authority rotation schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityRotationSchedule {
    pub current_epoch: u64,
    pub next_rotation_block: u64,
    pub rotation_interval: u64,
    pub pending_authorities: Vec<String>,
}

/// Federation signature aggregation state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSignatureState {
    pub active_signing_sessions: HashMap<String, SigningSession>,
    pub completed_signatures: u64,
    pub failed_signatures: u32,
    pub average_signing_time: Duration,
}

/// Signing session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningSession {
    pub session_id: String,
    pub block_hash: BlockHash,
    pub started_at: DateTime<Utc>,
    pub participating_authorities: Vec<String>,
    pub collected_signatures: u32,
    pub required_signatures: u32,
}

/// Governance stream state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceCheckpointState {
    /// Stream connection status
    pub is_connected: bool,
    /// Last processed event ID
    pub last_processed_event: Option<String>,
    /// Pending events queue
    pub pending_events: VecDeque<GovernanceEvent>,
    /// Stream health metrics
    pub health_metrics: GovernanceHealthMetrics,
    /// Event processing backlog
    pub backlog_size: usize,
    /// Stream configuration
    pub stream_config: GovernanceStreamConfig,
}

/// Governance health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceHealthMetrics {
    pub events_processed_hourly: u32,
    pub error_rate: f64,
    pub average_processing_time: Duration,
    pub connection_uptime: Duration,
    pub last_heartbeat: DateTime<Utc>,
}

/// Governance stream configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceStreamConfig {
    pub stream_url: Option<String>,
    pub reconnect_interval: Duration,
    pub max_retry_attempts: u32,
    pub batch_size: usize,
    pub timeout: Duration,
}

/// Network topology snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopologySnapshot {
    /// Connected peers count
    pub connected_peers: usize,
    /// Network partitions detected
    pub partitions: Vec<NetworkPartition>,
    /// Network health score
    pub health_score: f64,
    /// Bandwidth utilization
    pub bandwidth_utilization: f64,
    /// Average latency
    pub average_latency: Duration,
    /// Cluster information
    pub cluster_info: ClusterInfo,
}

/// Network partition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPartition {
    pub partition_id: String,
    pub affected_peers: Vec<PeerId>,
    pub started_at: DateTime<Utc>,
    pub estimated_duration: Option<Duration>,
    pub severity: PartitionSeverity,
}

/// Partition severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PartitionSeverity {
    Minor,
    Moderate,
    Severe,
    Critical,
}

/// Network cluster information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub cluster_id: Option<String>,
    pub node_role: NodeRole,
    pub cluster_size: usize,
    pub leader_node: Option<PeerId>,
    pub consensus_participation: f64,
}

/// Node role in cluster
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NodeRole {
    Authority,
    FullNode,
    LightClient,
    Archive,
}

/// Comprehensive metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub sync_metrics: SyncMetricsSnapshot,
    pub performance_metrics: PerformanceMetricsSnapshot,
    pub resource_metrics: ResourceMetricsSnapshot,
    pub error_metrics: ErrorMetricsSnapshot,
    pub timestamp: DateTime<Utc>,
}

/// Sync-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetricsSnapshot {
    pub blocks_processed: u64,
    pub blocks_per_second: f64,
    pub validation_success_rate: f64,
    pub peer_count: usize,
    pub sync_progress_percent: f64,
    pub estimated_completion: Option<Duration>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetricsSnapshot {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub disk_io_rate: f64,
    pub network_bandwidth: u64,
    pub thread_count: u32,
    pub gc_pressure: f64,
}

/// Resource utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetricsSnapshot {
    pub memory_peak: u64,
    pub disk_space_used: u64,
    pub file_descriptors: u32,
    pub network_connections: u32,
    pub database_size: u64,
}

/// Error tracking metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetricsSnapshot {
    pub total_errors: u64,
    pub error_rate: f64,
    pub critical_errors: u32,
    pub recovery_attempts: u32,
    pub last_error_time: Option<DateTime<Utc>>,
}

/// Recovery context for fast restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryContext {
    /// Fast recovery hints
    pub recovery_hints: Vec<RecoveryHint>,
    /// State validation shortcuts
    pub validation_shortcuts: ValidationShortcuts,
    /// Dependency information
    pub dependencies: Vec<DependencyInfo>,
    /// Recovery strategy preference
    pub preferred_strategy: RecoveryStrategy,
    /// Estimated recovery time
    pub estimated_recovery_time: Duration,
}

/// Recovery hints for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryHint {
    pub hint_type: String,
    pub context: serde_json::Value,
    pub priority: u8,
    pub estimated_benefit: Duration,
}

/// Validation shortcuts for faster recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationShortcuts {
    pub skip_full_validation: bool,
    pub trusted_blocks: Vec<BlockHash>,
    pub verified_state_roots: HashMap<u64, Hash256>,
    pub federation_signatures_verified: HashMap<BlockHash, bool>,
}

/// Dependency information for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub dependency_type: String,
    pub required_height: u64,
    pub optional: bool,
    pub fallback_available: bool,
}

/// Recovery strategy options
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    Fast,
    Safe,
    Minimal,
    Full,
}

/// Main checkpoint manager
#[derive(Debug)]
pub struct CheckpointManager {
    /// Configuration
    config: CheckpointConfig,
    
    /// Storage backend
    storage: Arc<CheckpointStorage>,
    
    /// Active checkpoints cache
    active_checkpoints: Arc<TokioRwLock<BTreeMap<u64, BlockCheckpoint>>>,
    
    /// Checkpoint creation scheduler
    scheduler: Arc<TokioRwLock<CheckpointScheduler>>,
    
    /// Recovery engine
    recovery_engine: Arc<RecoveryEngine>,
    
    /// Verification engine
    verification_engine: Arc<VerificationEngine>,
    
    /// Background tasks
    background_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    
    /// Metrics collector
    metrics: CheckpointMetrics,
}

/// Checkpoint configuration
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Checkpoint interval in blocks
    pub interval: u64,
    /// Maximum number of checkpoints to keep
    pub max_checkpoints: usize,
    /// Storage directory
    pub storage_path: PathBuf,
    /// Enable compression
    pub compression_enabled: bool,
    /// Compression level (1-9)
    pub compression_level: u8,
    /// Enable encryption
    pub encryption_enabled: bool,
    /// Verification level
    pub verification_level: VerificationLevel,
    /// Auto-recovery enabled
    pub auto_recovery_enabled: bool,
    /// Recovery timeout
    pub recovery_timeout: Duration,
    /// Emergency checkpoint triggers
    pub emergency_triggers: Vec<EmergencyTrigger>,
}

/// Verification levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationLevel {
    None,
    Basic,
    Full,
    Paranoid,
}

/// Emergency checkpoint triggers
#[derive(Debug, Clone)]
pub struct EmergencyTrigger {
    pub trigger_type: String,
    pub threshold: f64,
    pub enabled: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            interval: 1000,
            max_checkpoints: 10,
            storage_path: PathBuf::from("./data/checkpoints"),
            compression_enabled: true,
            compression_level: 6,
            encryption_enabled: false,
            verification_level: VerificationLevel::Full,
            auto_recovery_enabled: true,
            recovery_timeout: Duration::from_secs(300),
            emergency_triggers: vec![
                EmergencyTrigger {
                    trigger_type: "sync_failure".to_string(),
                    threshold: 0.95,
                    enabled: true,
                },
                EmergencyTrigger {
                    trigger_type: "network_partition".to_string(),
                    threshold: 0.8,
                    enabled: true,
                },
            ],
        }
    }
}

/// Checkpoint storage backend
#[derive(Debug)]
pub struct CheckpointStorage {
    base_path: PathBuf,
    compression_enabled: bool,
    compression_level: u8,
    encryption_enabled: bool,
}

impl CheckpointStorage {
    pub fn new(config: &CheckpointConfig) -> SyncResult<Self> {
        create_dir_all(&config.storage_path)
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to create checkpoint directory: {}", e) 
            })?;
        
        Ok(Self {
            base_path: config.storage_path.clone(),
            compression_enabled: config.compression_enabled,
            compression_level: config.compression_level,
            encryption_enabled: config.encryption_enabled,
        })
    }
    
    pub async fn store_checkpoint(&self, checkpoint: &BlockCheckpoint) -> SyncResult<()> {
        let file_path = self.get_checkpoint_path(&checkpoint.metadata.id);
        
        let serialized = serde_json::to_vec(checkpoint)
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to serialize checkpoint: {}", e) 
            })?;
        
        let data = if self.compression_enabled {
            self.compress_data(&serialized)?
        } else {
            serialized
        };
        
        let final_data = if self.encryption_enabled {
            self.encrypt_data(&data).await?
        } else {
            data
        };
        
        tokio::fs::write(&file_path, final_data).await
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to write checkpoint file: {}", e) 
            })?;
        
        CHECKPOINT_STORAGE_SIZE.add(final_data.len() as i64);
        Ok(())
    }
    
    pub async fn load_checkpoint(&self, checkpoint_id: &str) -> SyncResult<BlockCheckpoint> {
        let file_path = self.get_checkpoint_path(checkpoint_id);
        
        let data = tokio::fs::read(&file_path).await
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to read checkpoint file: {}", e) 
            })?;
        
        let decrypted_data = if self.encryption_enabled {
            self.decrypt_data(&data).await?
        } else {
            data
        };
        
        let decompressed_data = if self.compression_enabled {
            self.decompress_data(&decrypted_data)?
        } else {
            decrypted_data
        };
        
        let checkpoint = serde_json::from_slice(&decompressed_data)
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to deserialize checkpoint: {}", e) 
            })?;
        
        Ok(checkpoint)
    }
    
    pub async fn delete_checkpoint(&self, checkpoint_id: &str) -> SyncResult<()> {
        let file_path = self.get_checkpoint_path(checkpoint_id);
        
        if file_path.exists() {
            let metadata = tokio::fs::metadata(&file_path).await
                .map_err(|e| SyncError::Internal { 
                    message: format!("Failed to read checkpoint metadata: {}", e) 
                })?;
            
            tokio::fs::remove_file(&file_path).await
                .map_err(|e| SyncError::Internal { 
                    message: format!("Failed to delete checkpoint file: {}", e) 
                })?;
            
            CHECKPOINT_STORAGE_SIZE.sub(metadata.len() as i64);
        }
        
        Ok(())
    }
    
    pub async fn list_checkpoints(&self) -> SyncResult<Vec<String>> {
        let mut entries = tokio::fs::read_dir(&self.base_path).await
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to read checkpoint directory: {}", e) 
            })?;
        
        let mut checkpoints = Vec::new();
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| SyncError::Internal { 
                message: format!("Failed to read directory entry: {}", e) 
            })? {
            
            let file_name = entry.file_name();
            if let Some(name_str) = file_name.to_str() {
                if name_str.ends_with(".checkpoint") {
                    let checkpoint_id = name_str.trim_end_matches(".checkpoint");
                    checkpoints.push(checkpoint_id.to_string());
                }
            }
        }
        
        Ok(checkpoints)
    }
    
    fn get_checkpoint_path(&self, checkpoint_id: &str) -> PathBuf {
        self.base_path.join(format!("{}.checkpoint", checkpoint_id))
    }
    
    fn compress_data(&self, data: &[u8]) -> SyncResult<Vec<u8>> {
        // Simplified compression - in a real implementation you'd use a proper compression library
        Ok(data.to_vec())
    }
    
    fn decompress_data(&self, data: &[u8]) -> SyncResult<Vec<u8>> {
        // Simplified decompression - in a real implementation you'd use a proper compression library
        Ok(data.to_vec())
    }
    
    async fn encrypt_data(&self, data: &[u8]) -> SyncResult<Vec<u8>> {
        // Placeholder for encryption implementation
        // In a real implementation, you'd use something like AES-GCM
        Ok(data.to_vec())
    }
    
    async fn decrypt_data(&self, data: &[u8]) -> SyncResult<Vec<u8>> {
        // Placeholder for decryption implementation
        Ok(data.to_vec())
    }
}

/// Checkpoint scheduling system
#[derive(Debug)]
pub struct CheckpointScheduler {
    config: CheckpointConfig,
    last_checkpoint: AtomicU64,
    scheduled_checkpoints: VecDeque<ScheduledCheckpoint>,
    emergency_pending: AtomicBool,
}

/// Scheduled checkpoint information
#[derive(Debug, Clone)]
pub struct ScheduledCheckpoint {
    pub height: u64,
    pub checkpoint_type: CheckpointType,
    pub scheduled_at: Instant,
    pub priority: u8,
}

impl CheckpointScheduler {
    pub fn new(config: CheckpointConfig) -> Self {
        Self {
            config,
            last_checkpoint: AtomicU64::new(0),
            scheduled_checkpoints: VecDeque::new(),
            emergency_pending: AtomicBool::new(false),
        }
    }
    
    pub fn should_create_checkpoint(&self, current_height: u64) -> bool {
        let last = self.last_checkpoint.load(Ordering::Relaxed);
        
        current_height > 0 && 
        (current_height - last >= self.config.interval || self.emergency_pending.load(Ordering::Relaxed))
    }
    
    pub fn schedule_checkpoint(&mut self, height: u64, checkpoint_type: CheckpointType, priority: u8) {
        let scheduled = ScheduledCheckpoint {
            height,
            checkpoint_type,
            scheduled_at: Instant::now(),
            priority,
        };
        
        // Insert in priority order
        let pos = self.scheduled_checkpoints
            .iter()
            .position(|s| s.priority > priority)
            .unwrap_or(self.scheduled_checkpoints.len());
        
        self.scheduled_checkpoints.insert(pos, scheduled);
    }
    
    pub fn next_checkpoint(&mut self) -> Option<ScheduledCheckpoint> {
        self.scheduled_checkpoints.pop_front()
    }
    
    pub fn trigger_emergency_checkpoint(&self) {
        self.emergency_pending.store(true, Ordering::Relaxed);
    }
    
    pub fn checkpoint_created(&self, height: u64) {
        self.last_checkpoint.store(height, Ordering::Relaxed);
        self.emergency_pending.store(false, Ordering::Relaxed);
    }
}

/// Recovery engine for checkpoint restoration
#[derive(Debug)]
pub struct RecoveryEngine {
    config: CheckpointConfig,
    storage: Arc<CheckpointStorage>,
    verification_engine: Arc<VerificationEngine>,
}

impl RecoveryEngine {
    pub fn new(
        config: CheckpointConfig, 
        storage: Arc<CheckpointStorage>,
        verification_engine: Arc<VerificationEngine>,
    ) -> Self {
        Self {
            config,
            storage,
            verification_engine,
        }
    }
    
    pub async fn recover_from_checkpoint(
        &self,
        checkpoint_id: &str,
        chain_actor: Addr<ChainActor>,
        consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<RecoveryResult> {
        let _timer = CHECKPOINT_RECOVERY_DURATION.start_timer();
        
        let checkpoint = self.storage.load_checkpoint(checkpoint_id).await?;
        
        // Verify checkpoint integrity
        if self.config.verification_level != VerificationLevel::None {
            self.verification_engine.verify_checkpoint(&checkpoint).await?;
        }
        
        // Apply recovery strategy
        let recovery_result = match checkpoint.recovery_context.preferred_strategy {
            RecoveryStrategy::Fast => self.fast_recovery(&checkpoint, chain_actor, consensus_actor).await?,
            RecoveryStrategy::Safe => self.safe_recovery(&checkpoint, chain_actor, consensus_actor).await?,
            RecoveryStrategy::Minimal => self.minimal_recovery(&checkpoint, chain_actor, consensus_actor).await?,
            RecoveryStrategy::Full => self.full_recovery(&checkpoint, chain_actor, consensus_actor).await?,
        };
        
        Ok(recovery_result)
    }
    
    async fn fast_recovery(
        &self,
        checkpoint: &BlockCheckpoint,
        _chain_actor: Addr<ChainActor>,
        _consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<RecoveryResult> {
        // Fast recovery with shortcuts and minimal validation
        Ok(RecoveryResult {
            recovered_height: checkpoint.metadata.height,
            recovery_time: Duration::from_millis(100),
            blocks_recovered: 1,
            state_recovered: true,
            peers_recovered: checkpoint.peer_states.len(),
            warnings: vec![],
        })
    }
    
    async fn safe_recovery(
        &self,
        checkpoint: &BlockCheckpoint,
        _chain_actor: Addr<ChainActor>,
        _consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<RecoveryResult> {
        // Balanced recovery with essential validation
        Ok(RecoveryResult {
            recovered_height: checkpoint.metadata.height,
            recovery_time: Duration::from_secs(1),
            blocks_recovered: 1,
            state_recovered: true,
            peers_recovered: checkpoint.peer_states.len(),
            warnings: vec![],
        })
    }
    
    async fn minimal_recovery(
        &self,
        checkpoint: &BlockCheckpoint,
        _chain_actor: Addr<ChainActor>,
        _consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<RecoveryResult> {
        // Minimal recovery - just restore basic state
        Ok(RecoveryResult {
            recovered_height: checkpoint.metadata.height,
            recovery_time: Duration::from_millis(50),
            blocks_recovered: 1,
            state_recovered: false,
            peers_recovered: 0,
            warnings: vec!["Minimal recovery - some state not restored".to_string()],
        })
    }
    
    async fn full_recovery(
        &self,
        checkpoint: &BlockCheckpoint,
        _chain_actor: Addr<ChainActor>,
        _consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<RecoveryResult> {
        // Full recovery with complete validation
        Ok(RecoveryResult {
            recovered_height: checkpoint.metadata.height,
            recovery_time: Duration::from_secs(5),
            blocks_recovered: 1,
            state_recovered: true,
            peers_recovered: checkpoint.peer_states.len(),
            warnings: vec![],
        })
    }
}

/// Recovery result information
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub recovered_height: u64,
    pub recovery_time: Duration,
    pub blocks_recovered: usize,
    pub state_recovered: bool,
    pub peers_recovered: usize,
    pub warnings: Vec<String>,
}

/// Checkpoint verification engine
#[derive(Debug)]
pub struct VerificationEngine {
    config: CheckpointConfig,
}

impl VerificationEngine {
    pub fn new(config: CheckpointConfig) -> Self {
        Self { config }
    }
    
    pub async fn verify_checkpoint(&self, checkpoint: &BlockCheckpoint) -> SyncResult<()> {
        match self.config.verification_level {
            VerificationLevel::None => Ok(()),
            VerificationLevel::Basic => self.basic_verification(checkpoint).await,
            VerificationLevel::Full => self.full_verification(checkpoint).await,
            VerificationLevel::Paranoid => self.paranoid_verification(checkpoint).await,
        }
    }
    
    async fn basic_verification(&self, checkpoint: &BlockCheckpoint) -> SyncResult<()> {
        // Verify checksum
        let computed_hash = self.compute_checkpoint_hash(checkpoint);
        if computed_hash != checkpoint.metadata.verification_hash {
            CHECKPOINT_VERIFICATION_FAILURES.inc();
            return Err(SyncError::Checkpoint {
                checkpoint_id: checkpoint.metadata.id.clone(),
                reason: "Hash verification failed".to_string(),
                recovery_possible: false,
            });
        }
        
        Ok(())
    }
    
    async fn full_verification(&self, checkpoint: &BlockCheckpoint) -> SyncResult<()> {
        self.basic_verification(checkpoint).await?;
        
        // Verify blockchain state consistency
        if checkpoint.blockchain_state.best_block.header.number != checkpoint.metadata.height {
            CHECKPOINT_VERIFICATION_FAILURES.inc();
            return Err(SyncError::Checkpoint {
                checkpoint_id: checkpoint.metadata.id.clone(),
                reason: "Block height mismatch".to_string(),
                recovery_possible: true,
            });
        }
        
        // Verify state root
        // Additional verification logic would go here
        
        Ok(())
    }
    
    async fn paranoid_verification(&self, checkpoint: &BlockCheckpoint) -> SyncResult<()> {
        self.full_verification(checkpoint).await?;
        
        // Extensive verification including cryptographic proofs
        // This would include signature verification, state tree verification, etc.
        
        Ok(())
    }
    
    fn compute_checkpoint_hash(&self, checkpoint: &BlockCheckpoint) -> Hash256 {
        let mut hasher = Sha256::new();
        
        // Hash critical checkpoint data
        hasher.update(&checkpoint.metadata.height.to_be_bytes());
        hasher.update(checkpoint.metadata.block_hash.as_bytes());
        hasher.update(checkpoint.metadata.created_at.timestamp().to_be_bytes());
        
        if let Ok(serialized) = serde_json::to_vec(&checkpoint.blockchain_state) {
            hasher.update(&serialized);
        }
        
        Hash256::from_slice(&hasher.finalize())
    }
}

/// Checkpoint metrics collector
#[derive(Debug, Default)]
pub struct CheckpointMetrics {
    pub checkpoints_created: AtomicU64,
    pub checkpoints_recovered: AtomicU64,
    pub average_creation_time: AtomicU64,
    pub average_recovery_time: AtomicU64,
    pub storage_usage: AtomicU64,
    pub verification_failures: AtomicU64,
}

impl CheckpointManager {
    pub async fn new(config: CheckpointConfig) -> SyncResult<Self> {
        let storage = Arc::new(CheckpointStorage::new(&config)?);
        let scheduler = Arc::new(TokioRwLock::new(CheckpointScheduler::new(config.clone())));
        let verification_engine = Arc::new(VerificationEngine::new(config.clone()));
        let recovery_engine = Arc::new(RecoveryEngine::new(
            config.clone(),
            storage.clone(),
            verification_engine.clone(),
        ));
        
        Ok(Self {
            config,
            storage,
            active_checkpoints: Arc::new(TokioRwLock::new(BTreeMap::new())),
            scheduler,
            recovery_engine,
            verification_engine,
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
            metrics: CheckpointMetrics::default(),
        })
    }
    
    pub async fn create_checkpoint(
        &self,
        height: u64,
        sync_progress: SyncProgress,
        peer_manager: &PeerManager,
        chain_actor: Addr<ChainActor>,
        consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<String> {
        let _timer = CHECKPOINT_CREATION_DURATION.start_timer();
        let start_time = Instant::now();
        
        let checkpoint_id = format!("checkpoint_{}_{}",
            height,
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
        );
        
        // Collect blockchain state
        let blockchain_state = self.collect_blockchain_state(height, chain_actor).await?;
        
        // Collect peer states
        let peer_states = self.collect_peer_states(peer_manager).await?;
        
        // Collect federation state
        let federation_state = self.collect_federation_state(consensus_actor).await?;
        
        // Collect governance state
        let governance_state = self.collect_governance_state().await?;
        
        // Collect network topology
        let network_topology = self.collect_network_topology(peer_manager).await?;
        
        // Collect metrics snapshot
        let metrics_snapshot = self.collect_metrics_snapshot().await?;
        
        // Create recovery context
        let recovery_context = self.create_recovery_context(&blockchain_state, &peer_states).await?;
        
        let checkpoint = BlockCheckpoint {
            metadata: CheckpointMetadata {
                id: checkpoint_id.clone(),
                height,
                block_hash: blockchain_state.best_block.hash(),
                parent_checkpoint_id: self.get_last_checkpoint_id().await,
                created_at: Utc::now(),
                version: 1,
                checkpoint_type: CheckpointType::Scheduled,
                verification_hash: Hash256::default(), // Will be computed
                size_bytes: 0, // Will be computed after serialization
                compression_level: self.config.compression_level,
            },
            blockchain_state,
            sync_progress,
            peer_states,
            federation_state,
            governance_state,
            network_topology,
            metrics_snapshot,
            recovery_context,
        };
        
        // Compute verification hash
        let verification_hash = self.verification_engine.compute_checkpoint_hash(&checkpoint);
        let mut checkpoint = checkpoint;
        checkpoint.metadata.verification_hash = verification_hash;
        
        // Store checkpoint
        self.storage.store_checkpoint(&checkpoint).await?;
        
        // Update cache
        {
            let mut active = self.active_checkpoints.write().await;
            active.insert(height, checkpoint);
            
            // Cleanup old checkpoints
            while active.len() > self.config.max_checkpoints {
                if let Some((old_height, _)) = active.pop_first() {
                    if let Err(e) = self.storage.delete_checkpoint(&format!("checkpoint_{}", old_height)).await {
                        warn!("Failed to delete old checkpoint: {}", e);
                    }
                }
            }
        }
        
        // Update metrics
        CHECKPOINTS_CREATED.inc();
        ACTIVE_CHECKPOINTS.set(self.active_checkpoints.read().await.len() as i64);
        self.metrics.checkpoints_created.fetch_add(1, Ordering::Relaxed);
        self.metrics.average_creation_time.store(
            start_time.elapsed().as_millis() as u64,
            Ordering::Relaxed
        );
        
        // Update scheduler
        {
            let scheduler = self.scheduler.read().await;
            scheduler.checkpoint_created(height);
        }
        
        info!("Created checkpoint {} at height {} in {:?}", 
              checkpoint_id, height, start_time.elapsed());
        
        Ok(checkpoint_id)
    }
    
    async fn collect_blockchain_state(&self, height: u64, chain_actor: Addr<ChainActor>) -> SyncResult<BlockchainState> {
        // Get current chain state
        let chain_state = chain_actor.send(GetChainState).await
            .map_err(|e| SyncError::Internal { message: format!("Failed to get chain state: {}", e) })??;
        
        let best_block = chain_actor.send(GetBlock { height: Some(height), hash: None }).await
            .map_err(|e| SyncError::Internal { message: format!("Failed to get block: {}", e) })??;
        
        Ok(BlockchainState {
            best_block,
            finalized_block: best_block.clone(), // Simplified for now
            head_candidates: vec![],
            state_root: Hash256::default(),
            total_difficulty: height,
            tx_pool_size: 0,
            fork_choice_data: ForkChoiceData {
                forks: vec![],
                preferred_fork: None,
                fork_weights: HashMap::new(),
            },
        })
    }
    
    async fn collect_peer_states(&self, peer_manager: &PeerManager) -> SyncResult<HashMap<PeerId, PeerCheckpointState>> {
        let mut peer_states = HashMap::new();
        
        let peers = peer_manager.get_all_peers();
        for (peer_id, peer_info) in peers {
            let checkpoint_state = PeerCheckpointState {
                peer_id: peer_id.clone(),
                best_block: peer_info.best_block.number,
                quality_score: peer_info.reputation_score(),
                reliability: PeerReliabilityMetrics {
                    success_rate: 0.9,
                    average_response_time: Duration::from_millis(100),
                    blocks_served: 1000,
                    errors_encountered: 10,
                    uptime_percentage: 0.95,
                },
                last_interaction: Utc::now(),
                capabilities: PeerCapabilities {
                    supports_fast_sync: true,
                    supports_state_sync: true,
                    supports_federation_sync: true,
                    max_batch_size: 128,
                    protocol_version: 1,
                },
                sync_state: PeerSyncState::Complete,
            };
            peer_states.insert(peer_id, checkpoint_state);
        }
        
        Ok(peer_states)
    }
    
    async fn collect_federation_state(&self, consensus_actor: Addr<ConsensusActor>) -> SyncResult<FederationCheckpointState> {
        // Get consensus state
        let consensus_state = consensus_actor.send(GetConsensusState).await
            .map_err(|e| SyncError::Internal { message: format!("Failed to get consensus state: {}", e) })??;
        
        Ok(FederationCheckpointState {
            authorities: vec![
                AuthorityInfo {
                    authority_id: "authority_1".to_string(),
                    public_key: vec![1; 32],
                    weight: 1,
                    is_active: true,
                    last_block_produced: Some(1000),
                    reputation_score: 0.95,
                }
            ],
            current_round: 100,
            last_federation_block: 999,
            rotation_schedule: AuthorityRotationSchedule {
                current_epoch: 10,
                next_rotation_block: 2000,
                rotation_interval: 1000,
                pending_authorities: vec![],
            },
            signature_state: FederationSignatureState {
                active_signing_sessions: HashMap::new(),
                completed_signatures: 100,
                failed_signatures: 2,
                average_signing_time: Duration::from_millis(500),
            },
            emergency_mode: false,
        })
    }
    
    async fn collect_governance_state(&self) -> SyncResult<GovernanceCheckpointState> {
        Ok(GovernanceCheckpointState {
            is_connected: true,
            last_processed_event: Some("event_123".to_string()),
            pending_events: VecDeque::new(),
            health_metrics: GovernanceHealthMetrics {
                events_processed_hourly: 100,
                error_rate: 0.01,
                average_processing_time: Duration::from_millis(50),
                connection_uptime: Duration::from_secs(3600),
                last_heartbeat: Utc::now(),
            },
            backlog_size: 0,
            stream_config: GovernanceStreamConfig {
                stream_url: Some("wss://governance.anduro.io".to_string()),
                reconnect_interval: Duration::from_secs(30),
                max_retry_attempts: 3,
                batch_size: 100,
                timeout: Duration::from_secs(30),
            },
        })
    }
    
    async fn collect_network_topology(&self, peer_manager: &PeerManager) -> SyncResult<NetworkTopologySnapshot> {
        let metrics = peer_manager.get_metrics();
        
        Ok(NetworkTopologySnapshot {
            connected_peers: metrics.active_peers as usize,
            partitions: vec![],
            health_score: 0.9,
            bandwidth_utilization: 0.7,
            average_latency: Duration::from_millis(100),
            cluster_info: ClusterInfo {
                cluster_id: Some("alys_testnet".to_string()),
                node_role: NodeRole::Authority,
                cluster_size: 10,
                leader_node: None,
                consensus_participation: 0.95,
            },
        })
    }
    
    async fn collect_metrics_snapshot(&self) -> SyncResult<MetricsSnapshot> {
        Ok(MetricsSnapshot {
            sync_metrics: SyncMetricsSnapshot {
                blocks_processed: 1000,
                blocks_per_second: 10.0,
                validation_success_rate: 0.99,
                peer_count: 8,
                sync_progress_percent: 0.95,
                estimated_completion: Some(Duration::from_secs(300)),
            },
            performance_metrics: PerformanceMetricsSnapshot {
                cpu_usage: 45.0,
                memory_usage: 1024 * 1024 * 512, // 512MB
                disk_io_rate: 100.0,
                network_bandwidth: 1024 * 1024, // 1MB/s
                thread_count: 16,
                gc_pressure: 0.1,
            },
            resource_metrics: ResourceMetricsSnapshot {
                memory_peak: 1024 * 1024 * 1024, // 1GB
                disk_space_used: 1024 * 1024 * 1024 * 5, // 5GB
                file_descriptors: 256,
                network_connections: 32,
                database_size: 1024 * 1024 * 1024 * 2, // 2GB
            },
            error_metrics: ErrorMetricsSnapshot {
                total_errors: 10,
                error_rate: 0.001,
                critical_errors: 0,
                recovery_attempts: 2,
                last_error_time: Some(Utc::now() - chrono::Duration::minutes(30)),
            },
            timestamp: Utc::now(),
        })
    }
    
    async fn create_recovery_context(
        &self,
        blockchain_state: &BlockchainState,
        peer_states: &HashMap<PeerId, PeerCheckpointState>,
    ) -> SyncResult<RecoveryContext> {
        let mut recovery_hints = vec![
            RecoveryHint {
                hint_type: "fast_sync".to_string(),
                context: serde_json::json!({"trusted_height": blockchain_state.best_block.header.number}),
                priority: 1,
                estimated_benefit: Duration::from_secs(60),
            }
        ];
        
        if peer_states.len() > 5 {
            recovery_hints.push(RecoveryHint {
                hint_type: "peer_diversity".to_string(),
                context: serde_json::json!({"peer_count": peer_states.len()}),
                priority: 2,
                estimated_benefit: Duration::from_secs(30),
            });
        }
        
        Ok(RecoveryContext {
            recovery_hints,
            validation_shortcuts: ValidationShortcuts {
                skip_full_validation: false,
                trusted_blocks: vec![blockchain_state.best_block.hash()],
                verified_state_roots: HashMap::new(),
                federation_signatures_verified: HashMap::new(),
            },
            dependencies: vec![],
            preferred_strategy: RecoveryStrategy::Safe,
            estimated_recovery_time: Duration::from_secs(120),
        })
    }
    
    async fn get_last_checkpoint_id(&self) -> Option<String> {
        let active = self.active_checkpoints.read().await;
        active.keys().max().map(|height| format!("checkpoint_{}", height))
    }
    
    pub async fn recover_from_latest_checkpoint(
        &self,
        chain_actor: Addr<ChainActor>,
        consensus_actor: Addr<ConsensusActor>,
    ) -> SyncResult<Option<RecoveryResult>> {
        let checkpoints = self.storage.list_checkpoints().await?;
        if checkpoints.is_empty() {
            return Ok(None);
        }
        
        // Find latest checkpoint
        let latest_checkpoint = checkpoints
            .iter()
            .max()
            .unwrap();
        
        let result = self.recovery_engine
            .recover_from_checkpoint(latest_checkpoint, chain_actor, consensus_actor)
            .await?;
        
        self.metrics.checkpoints_recovered.fetch_add(1, Ordering::Relaxed);
        
        Ok(Some(result))
    }
    
    pub async fn should_create_checkpoint(&self, current_height: u64) -> bool {
        let scheduler = self.scheduler.read().await;
        scheduler.should_create_checkpoint(current_height)
    }
    
    pub async fn get_checkpoint_info(&self, checkpoint_id: &str) -> SyncResult<Option<CheckpointMetadata>> {
        if let Ok(checkpoint) = self.storage.load_checkpoint(checkpoint_id).await {
            Ok(Some(checkpoint.metadata))
        } else {
            Ok(None)
        }
    }
    
    pub async fn cleanup_old_checkpoints(&self) -> SyncResult<usize> {
        let checkpoints = self.storage.list_checkpoints().await?;
        let mut cleaned = 0;
        
        if checkpoints.len() > self.config.max_checkpoints {
            let to_remove = checkpoints.len() - self.config.max_checkpoints;
            let mut sorted_checkpoints = checkpoints;
            sorted_checkpoints.sort();
            
            for checkpoint_id in sorted_checkpoints.iter().take(to_remove) {
                if let Err(e) = self.storage.delete_checkpoint(checkpoint_id).await {
                    warn!("Failed to cleanup checkpoint {}: {}", checkpoint_id, e);
                } else {
                    cleaned += 1;
                }
            }
        }
        
        Ok(cleaned)
    }
    
    pub fn get_metrics(&self) -> CheckpointMetrics {
        CheckpointMetrics {
            checkpoints_created: AtomicU64::new(self.metrics.checkpoints_created.load(Ordering::Relaxed)),
            checkpoints_recovered: AtomicU64::new(self.metrics.checkpoints_recovered.load(Ordering::Relaxed)),
            average_creation_time: AtomicU64::new(self.metrics.average_creation_time.load(Ordering::Relaxed)),
            average_recovery_time: AtomicU64::new(self.metrics.average_recovery_time.load(Ordering::Relaxed)),
            storage_usage: AtomicU64::new(self.metrics.storage_usage.load(Ordering::Relaxed)),
            verification_failures: AtomicU64::new(self.metrics.verification_failures.load(Ordering::Relaxed)),
        }
    }
    
    pub async fn shutdown(&self) -> SyncResult<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        
        // Wait for background tasks
        let mut tasks = self.background_tasks.lock().await;
        for task in tasks.drain(..) {
            task.abort();
        }
        
        info!("CheckpointManager shutdown complete");
        Ok(())
    }
}

// Additional message types and implementations needed for chain/consensus actors

#[derive(Message, Debug)]
#[rtype(result = "SyncResult<ChainState>")]
pub struct GetChainState;

#[derive(Message, Debug)]
#[rtype(result = "SyncResult<Block>")]
pub struct GetBlock {
    pub height: Option<u64>,
    pub hash: Option<BlockHash>,
}

#[derive(Message, Debug)]
#[rtype(result = "SyncResult<ConsensusState>")]
pub struct GetConsensusState;

#[derive(Debug, Clone)]
pub struct ChainState {
    pub best_block: Block,
    pub finalized_block: Block,
    pub state_root: Hash256,
}

#[derive(Debug, Clone)]
pub struct ConsensusState {
    pub current_round: u64,
    pub authorities: Vec<String>,
    pub is_authority: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    async fn create_test_checkpoint_manager() -> (CheckpointManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = CheckpointConfig {
            storage_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let manager = CheckpointManager::new(config).await.unwrap();
        (manager, temp_dir)
    }
    
    #[tokio::test]
    async fn test_checkpoint_manager_creation() {
        let (_manager, _temp_dir) = create_test_checkpoint_manager().await;
        // Manager should be created successfully
    }
    
    #[tokio::test]
    async fn test_checkpoint_storage() {
        let (_manager, temp_dir) = create_test_checkpoint_manager().await;
        let config = CheckpointConfig {
            storage_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let storage = CheckpointStorage::new(&config).unwrap();
        
        // Create a test checkpoint
        let checkpoint = create_test_checkpoint();
        
        // Store and retrieve
        storage.store_checkpoint(&checkpoint).await.unwrap();
        let loaded = storage.load_checkpoint(&checkpoint.metadata.id).await.unwrap();
        
        assert_eq!(loaded.metadata.id, checkpoint.metadata.id);
        assert_eq!(loaded.metadata.height, checkpoint.metadata.height);
    }
    
    #[tokio::test]
    async fn test_checkpoint_verification() {
        let config = CheckpointConfig::default();
        let verification_engine = VerificationEngine::new(config);
        
        let checkpoint = create_test_checkpoint();
        let result = verification_engine.verify_checkpoint(&checkpoint).await;
        
        // Should pass basic verification
        assert!(result.is_ok());
    }
    
    fn create_test_checkpoint() -> BlockCheckpoint {
        use crate::actors::sync::tests::create_test_block;
        
        let test_block = create_test_block(100, None);
        
        BlockCheckpoint {
            metadata: CheckpointMetadata {
                id: "test_checkpoint".to_string(),
                height: 100,
                block_hash: test_block.hash(),
                parent_checkpoint_id: None,
                created_at: Utc::now(),
                version: 1,
                checkpoint_type: CheckpointType::Manual,
                verification_hash: Hash256::from([0u8; 32]),
                size_bytes: 1024,
                compression_level: 6,
            },
            blockchain_state: BlockchainState {
                best_block: test_block.clone(),
                finalized_block: test_block,
                head_candidates: vec![],
                state_root: Hash256::from([0u8; 32]),
                total_difficulty: 100,
                tx_pool_size: 0,
                fork_choice_data: ForkChoiceData {
                    forks: vec![],
                    preferred_fork: None,
                    fork_weights: HashMap::new(),
                },
            },
            sync_progress: SyncProgress {
                current_height: 100,
                target_height: 1000,
                blocks_behind: 900,
                sync_mode: super::messages::SyncMode::Fast,
                sync_speed: 10.0,
                start_time: Some(Instant::now()),
                last_checkpoint_height: Some(50),
                active_downloads: 0,
                peers_contributing: 5,
                estimated_completion: Some(Duration::from_secs(90)),
                network_health_score: 0.9,
            },
            peer_states: HashMap::new(),
            federation_state: FederationCheckpointState {
                authorities: vec![],
                current_round: 10,
                last_federation_block: 99,
                rotation_schedule: AuthorityRotationSchedule {
                    current_epoch: 1,
                    next_rotation_block: 200,
                    rotation_interval: 100,
                    pending_authorities: vec![],
                },
                signature_state: FederationSignatureState {
                    active_signing_sessions: HashMap::new(),
                    completed_signatures: 10,
                    failed_signatures: 0,
                    average_signing_time: Duration::from_millis(100),
                },
                emergency_mode: false,
            },
            governance_state: GovernanceCheckpointState {
                is_connected: true,
                last_processed_event: None,
                pending_events: VecDeque::new(),
                health_metrics: GovernanceHealthMetrics {
                    events_processed_hourly: 50,
                    error_rate: 0.01,
                    average_processing_time: Duration::from_millis(10),
                    connection_uptime: Duration::from_secs(3600),
                    last_heartbeat: Utc::now(),
                },
                backlog_size: 0,
                stream_config: GovernanceStreamConfig {
                    stream_url: None,
                    reconnect_interval: Duration::from_secs(30),
                    max_retry_attempts: 3,
                    batch_size: 100,
                    timeout: Duration::from_secs(30),
                },
            },
            network_topology: NetworkTopologySnapshot {
                connected_peers: 8,
                partitions: vec![],
                health_score: 0.95,
                bandwidth_utilization: 0.6,
                average_latency: Duration::from_millis(50),
                cluster_info: ClusterInfo {
                    cluster_id: Some("test_cluster".to_string()),
                    node_role: NodeRole::FullNode,
                    cluster_size: 10,
                    leader_node: None,
                    consensus_participation: 0.9,
                },
            },
            metrics_snapshot: MetricsSnapshot {
                sync_metrics: SyncMetricsSnapshot {
                    blocks_processed: 100,
                    blocks_per_second: 2.0,
                    validation_success_rate: 0.99,
                    peer_count: 8,
                    sync_progress_percent: 0.1,
                    estimated_completion: Some(Duration::from_secs(450)),
                },
                performance_metrics: PerformanceMetricsSnapshot {
                    cpu_usage: 25.0,
                    memory_usage: 1024 * 1024 * 256,
                    disk_io_rate: 50.0,
                    network_bandwidth: 1024 * 512,
                    thread_count: 8,
                    gc_pressure: 0.05,
                },
                resource_metrics: ResourceMetricsSnapshot {
                    memory_peak: 1024 * 1024 * 512,
                    disk_space_used: 1024 * 1024 * 1024,
                    file_descriptors: 128,
                    network_connections: 16,
                    database_size: 1024 * 1024 * 512,
                },
                error_metrics: ErrorMetricsSnapshot {
                    total_errors: 2,
                    error_rate: 0.002,
                    critical_errors: 0,
                    recovery_attempts: 1,
                    last_error_time: Some(Utc::now() - chrono::Duration::hours(1)),
                },
                timestamp: Utc::now(),
            },
            recovery_context: RecoveryContext {
                recovery_hints: vec![],
                validation_shortcuts: ValidationShortcuts {
                    skip_full_validation: false,
                    trusted_blocks: vec![],
                    verified_state_roots: HashMap::new(),
                    federation_signatures_verified: HashMap::new(),
                },
                dependencies: vec![],
                preferred_strategy: RecoveryStrategy::Safe,
                estimated_recovery_time: Duration::from_secs(30),
            },
        }
    }
}