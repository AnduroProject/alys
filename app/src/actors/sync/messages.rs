//! Comprehensive message protocol for SyncActor
//!
//! This module defines all message types for inter-actor communication in the
//! Alys synchronization system, supporting federated PoA consensus, merged mining,
//! governance stream integration, and checkpoint-based recovery.

use actix::prelude::*;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use crate::types::*;
use super::errors::*;
use super::peer::*;

/// Primary sync control messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct StartSync {
    /// Starting height for synchronization (None = auto-detect)
    pub from_height: Option<u64>,
    /// Target height for synchronization (None = auto-detect from peers)
    pub target_height: Option<u64>,
    /// Recovery checkpoint if available
    pub checkpoint: Option<BlockCheckpoint>,
    /// Sync mode preference
    pub sync_mode: SyncMode,
    /// Priority level for this sync operation
    pub priority: SyncPriority,
    /// Correlation ID for tracing related operations
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct PauseSync {
    /// Reason for pausing synchronization
    pub reason: String,
    /// Whether the sync can be resumed later
    pub can_resume: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct ResumeSync {
    /// Optional target height override
    pub target_height: Option<u64>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct StopSync {
    /// Reason for stopping synchronization
    pub reason: String,
    /// Whether to perform graceful shutdown
    pub graceful: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Sync status and monitoring messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<SyncStatus>")]
pub struct GetSyncStatus {
    /// Include detailed progress information
    pub include_details: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<SyncProgress>")]
pub struct GetSyncProgress {
    /// Include peer information
    pub include_peers: bool,
    /// Include performance metrics
    pub include_metrics: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<bool>")]
pub struct CanProduceBlocks {
    /// Minimum sync threshold to check against (default: 99.5%)
    pub threshold: Option<f64>,
    /// Consider governance stream health
    pub check_governance_health: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Block processing messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<ProcessingResult>")]
pub struct ProcessBlockBatch {
    /// Blocks to process in parallel
    pub blocks: Vec<SignedConsensusBlock>,
    /// Source peer for performance tracking
    pub from_peer: PeerId,
    /// Processing priority
    pub priority: ProcessingPriority,
    /// Validation requirements
    pub validation_level: ValidationLevel,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<ValidationResult>")]
pub struct ValidateBlock {
    /// Block to validate
    pub block: SignedConsensusBlock,
    /// Validation requirements
    pub validation_level: ValidationLevel,
    /// Federation signature requirements
    pub require_federation_signature: bool,
    /// Check against governance stream events
    pub check_governance_events: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Peer management messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct PeerDiscovered {
    /// Newly discovered peer
    pub peer_id: PeerId,
    /// Peer's reported best block height
    pub reported_height: u64,
    /// Peer's protocol version
    pub protocol_version: String,
    /// Peer capabilities
    pub capabilities: PeerCapabilities,
    /// Initial connection quality assessment
    pub connection_quality: ConnectionQuality,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct PeerDisconnected {
    /// Disconnected peer
    pub peer_id: PeerId,
    /// Reason for disconnection
    pub reason: String,
    /// Whether the disconnection was expected
    pub expected: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct UpdatePeerScore {
    /// Peer to update
    pub peer_id: PeerId,
    /// New score components
    pub performance_update: PeerPerformanceUpdate,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Checkpoint management messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<BlockCheckpoint>")]
pub struct CreateCheckpoint {
    /// Height to create checkpoint at (None = current height)
    pub height: Option<u64>,
    /// Whether to verify the checkpoint after creation
    pub verify: bool,
    /// Additional metadata for the checkpoint
    pub metadata: Option<HashMap<String, String>>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct RecoverFromCheckpoint {
    /// Checkpoint to recover from
    pub checkpoint: BlockCheckpoint,
    /// Whether to verify checkpoint integrity first
    pub verify_integrity: bool,
    /// Fallback strategy if recovery fails
    pub fallback_strategy: CheckpointRecoveryStrategy,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<Vec<BlockCheckpoint>>")]
pub struct ListCheckpoints {
    /// Maximum number of checkpoints to return
    pub limit: Option<usize>,
    /// Include checkpoint verification status
    pub include_verification: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Network monitoring and health messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<NetworkHealth>")]
pub struct GetNetworkHealth {
    /// Include detailed peer information
    pub include_peer_details: bool,
    /// Include partition detection results
    pub include_partition_info: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct NetworkPartitionDetected {
    /// Isolated peers during partition
    pub isolated_peers: Vec<PeerId>,
    /// Partition start time
    pub partition_start: Instant,
    /// Estimated duration of partition
    pub estimated_duration: Option<Duration>,
    /// Recovery strategy to apply
    pub recovery_strategy: PartitionRecoveryStrategy,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct NetworkPartitionResolved {
    /// Partition duration
    pub partition_duration: Duration,
    /// Recovered peers
    pub recovered_peers: Vec<PeerId>,
    /// Sync state after recovery
    pub post_recovery_status: SyncState,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Governance stream integration messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct GovernanceEventReceived {
    /// Governance event from Anduro stream
    pub event: GovernanceEvent,
    /// Event processing priority
    pub priority: GovernanceEventPriority,
    /// Expected processing deadline
    pub deadline: Option<Instant>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<GovernanceStreamHealth>")]
pub struct GetGovernanceStreamHealth {
    /// Include event processing statistics
    pub include_stats: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Performance monitoring and optimization messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<PerformanceMetrics>")]
pub struct GetPerformanceMetrics {
    /// Time range for metrics collection
    pub time_range: Option<Duration>,
    /// Include detailed breakdown by operation type
    pub include_breakdown: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct OptimizePerformance {
    /// Target performance improvement areas
    pub optimization_targets: Vec<OptimizationTarget>,
    /// Performance constraints
    pub constraints: PerformanceConstraints,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Internal coordination messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct SyncStateChanged {
    /// Previous sync state
    pub previous_state: SyncState,
    /// New sync state
    pub new_state: SyncState,
    /// Reason for state change
    pub reason: String,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct MetricsUpdate {
    /// Updated metrics
    pub metrics: SyncMetricsSnapshot,
    /// Update timestamp
    pub timestamp: Instant,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Supporting enums and structures

/// Synchronization modes for different scenarios
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncMode {
    /// Full synchronization from genesis
    Full,
    /// Fast sync using checkpoints and parallel downloads
    Fast,
    /// Optimistic sync assuming honest majority
    Optimistic,
    /// Catch-up sync for recent blocks only
    CatchUp,
    /// Emergency sync with governance stream priority
    Emergency,
}

/// Sync operation priorities
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncPriority {
    Low,
    Normal,
    High,
    Critical,
    Emergency,
}

/// Block processing priorities
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessingPriority {
    Background,
    Normal,
    High,
    RealTime,
}

/// Validation levels for block verification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationLevel {
    /// Basic structure and signature validation
    Basic,
    /// Full validation including state transitions
    Full,
    /// Extended validation with governance event checks
    Extended,
    /// Paranoid validation with all possible checks
    Paranoid,
}

/// Governance event priorities for Anduro stream processing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum GovernanceEventPriority {
    Informational,
    Normal,
    Important,
    Critical,
    Emergency,
}

/// Performance optimization targets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationTarget {
    /// Optimize block download throughput
    DownloadThroughput,
    /// Optimize validation performance
    ValidationSpeed,
    /// Optimize memory usage
    MemoryUsage,
    /// Optimize network utilization
    NetworkUtilization,
    /// Optimize peer selection algorithms
    PeerSelection,
    /// Optimize checkpoint operations
    CheckpointOperations,
}

/// Performance constraints for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConstraints {
    /// Maximum memory usage (bytes)
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU usage (percentage)
    pub max_cpu_percent: Option<u8>,
    /// Maximum network bandwidth (bytes/sec)
    pub max_network_bps: Option<u64>,
    /// Target sync speed (blocks/sec)
    pub target_sync_speed: Option<f64>,
    /// Maximum validation latency
    pub max_validation_latency: Option<Duration>,
}

/// Sync state enumeration for detailed state tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncState {
    /// Sync actor is idle and waiting for commands
    Idle,
    
    /// Discovering peers and network topology
    Discovering { 
        started_at: Instant, 
        attempts: u32,
        min_peers_required: usize,
    },
    
    /// Downloading block headers for fast sync
    DownloadingHeaders { 
        start: u64, 
        current: u64, 
        target: u64,
        batch_size: usize,
        peers_used: Vec<PeerId>,
    },
    
    /// Downloading full blocks with parallel processing
    DownloadingBlocks { 
        start: u64, 
        current: u64, 
        target: u64, 
        batch_size: usize,
        parallel_workers: usize,
        throughput_bps: f64,
    },
    
    /// Catching up with recent blocks near chain head
    CatchingUp { 
        blocks_behind: u64, 
        sync_speed: f64,
        governance_events_pending: u32,
        can_produce_threshold: f64,
    },
    
    /// Fully synchronized and following chain head
    Synced { 
        last_check: Instant,
        blocks_produced_while_synced: u64,
        governance_stream_healthy: bool,
    },
    
    /// Sync failed with recovery information
    Failed { 
        reason: String, 
        last_good_height: u64, 
        recovery_attempts: u32,
        recovery_strategy: Option<FailureRecoveryStrategy>,
        can_retry: bool,
    },
    
    /// Sync paused (can be resumed)
    Paused {
        paused_at: Instant,
        reason: String,
        last_progress: u64,
        can_resume: bool,
    },
    
    /// Emergency mode due to critical issues
    Emergency {
        issue: EmergencyIssue,
        started_at: Instant,
        mitigation_applied: bool,
    },
}

/// Emergency issues that trigger emergency sync mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmergencyIssue {
    /// Governance stream disconnected
    GovernanceStreamDown,
    /// Federation majority offline
    FederationMajorityOffline,
    /// Mining timeout approaching critical threshold
    MiningTimeoutCritical,
    /// Critical consensus failure
    ConsensusCriticalFailure,
    /// Severe network partition
    SevereNetworkPartition,
}

/// Failure recovery strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureRecoveryStrategy {
    /// Retry with same parameters
    Retry,
    /// Retry with reduced batch size
    RetryReducedBatch,
    /// Use checkpoint recovery
    CheckpointRecovery,
    /// Switch to emergency mode
    EmergencyMode,
    /// Fallback to governance stream
    GovernanceStreamFallback,
    /// Manual intervention required
    ManualIntervention,
}

/// Comprehensive sync status with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    /// Current sync state
    pub state: SyncState,
    /// Current blockchain height
    pub current_height: u64,
    /// Target blockchain height
    pub target_height: u64,
    /// Sync progress percentage (0.0 to 1.0)
    pub progress: f64,
    /// Current sync speed (blocks per second)
    pub blocks_per_second: f64,
    /// Number of connected peers
    pub peers_connected: usize,
    /// Estimated time to completion
    pub estimated_completion: Option<Duration>,
    /// Whether block production is allowed
    pub can_produce_blocks: bool,
    /// Governance stream health status
    pub governance_stream_healthy: bool,
    /// Federation health status
    pub federation_healthy: bool,
    /// Mining health status (blocks without PoW)
    pub mining_healthy: bool,
    /// Last successful checkpoint
    pub last_checkpoint: Option<u64>,
    /// Performance metrics snapshot
    pub performance: PerformanceSnapshot,
}

/// Detailed sync progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    /// Basic status information
    pub status: SyncStatus,
    /// Detailed peer information
    pub peer_details: Option<Vec<PeerSyncInfo>>,
    /// Active download operations
    pub active_downloads: Vec<DownloadOperation>,
    /// Recent validation results
    pub recent_validations: Vec<ValidationSummary>,
    /// Network health assessment
    pub network_health: NetworkHealth,
    /// Resource utilization
    pub resource_usage: ResourceUsage,
    /// Recent error summary
    pub recent_errors: Vec<ErrorSummary>,
}

/// Performance snapshot for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Network bandwidth utilization (bytes/sec)
    pub network_bandwidth: u64,
    /// Disk I/O rate (ops/sec)
    pub disk_io_rate: f64,
    /// Current throughput (blocks/sec)
    pub throughput: f64,
    /// Average latency for operations
    pub avg_latency: Duration,
}

/// Active download operation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadOperation {
    /// Block being downloaded
    pub block_height: u64,
    /// Source peer
    pub peer_id: PeerId,
    /// Download start time
    pub started_at: Instant,
    /// Current progress (bytes downloaded)
    pub bytes_downloaded: u64,
    /// Total expected bytes
    pub total_bytes: Option<u64>,
    /// Download speed (bytes/sec)
    pub download_speed: f64,
}

/// Validation operation summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationSummary {
    /// Block that was validated
    pub block_height: u64,
    /// Validation result
    pub result: bool,
    /// Validation time
    pub validation_time: Duration,
    /// Validation level used
    pub validation_level: ValidationLevel,
    /// Error message if validation failed
    pub error_message: Option<String>,
}

/// Error summary for recent errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    /// Error type
    pub error_type: String,
    /// Error count in recent time window
    pub count: u32,
    /// Most recent error message
    pub last_message: String,
    /// First occurrence time
    pub first_occurrence: Instant,
    /// Last occurrence time
    pub last_occurrence: Instant,
    /// Error severity
    pub severity: ErrorSeverity,
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Current memory usage (bytes)
    pub memory_current: u64,
    /// Peak memory usage (bytes)
    pub memory_peak: u64,
    /// CPU usage percentage
    pub cpu_percent: f64,
    /// File descriptor count
    pub file_descriptors: u32,
    /// Network connections count
    pub network_connections: u32,
    /// Disk space usage (bytes)
    pub disk_usage: u64,
}

/// Network health assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHealth {
    /// Overall health score (0.0 to 1.0)
    pub health_score: f64,
    /// Connected peer count
    pub connected_peers: usize,
    /// Reliable peer count
    pub reliable_peers: usize,
    /// Network partition detected
    pub partition_detected: bool,
    /// Average peer latency
    pub avg_peer_latency: Duration,
    /// Network bandwidth utilization
    pub bandwidth_utilization: f64,
    /// Consensus network health (federation)
    pub consensus_network_healthy: bool,
}

/// Governance stream health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceStreamHealth {
    /// Stream connection status
    pub connected: bool,
    /// Events processed in last hour
    pub events_processed_hourly: u32,
    /// Events pending processing
    pub events_pending: u32,
    /// Last successful event timestamp
    pub last_event_time: Option<Instant>,
    /// Stream latency
    pub stream_latency: Option<Duration>,
    /// Error rate percentage
    pub error_rate: f64,
}

/// Performance metrics for sync operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Sync throughput (blocks/sec)
    pub sync_throughput: f64,
    /// Validation throughput (blocks/sec)
    pub validation_throughput: f64,
    /// Download throughput (bytes/sec)
    pub download_throughput: f64,
    /// Average operation latencies
    pub operation_latencies: HashMap<String, Duration>,
    /// Resource efficiency scores
    pub efficiency_scores: HashMap<String, f64>,
    /// Performance bottlenecks identified
    pub bottlenecks: Vec<PerformanceBottleneck>,
}

/// Performance bottleneck identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBottleneck {
    /// Bottleneck component
    pub component: String,
    /// Impact severity
    pub severity: BottleneckSeverity,
    /// Description of the bottleneck
    pub description: String,
    /// Suggested optimization
    pub suggested_optimization: Option<String>,
    /// Estimated performance improvement
    pub estimated_improvement: Option<f64>,
}

/// Bottleneck severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum BottleneckSeverity {
    Minor,
    Moderate,
    Significant,
    Critical,
}

/// Block processing result with comprehensive information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    /// Number of blocks successfully processed
    pub processed: usize,
    /// Number of blocks that failed processing
    pub failed: usize,
    /// Successfully validated blocks ready for import
    pub validated_blocks: Vec<SignedConsensusBlock>,
    /// Failed validation results with reasons
    pub validation_failures: Vec<ValidationFailure>,
    /// Processing performance metrics
    pub processing_metrics: ProcessingMetrics,
    /// Federation signature verification results
    pub federation_signatures_verified: usize,
    /// Governance event compliance status
    pub governance_compliance: bool,
}

/// Validation failure information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFailure {
    /// Block that failed validation
    pub block_hash: BlockHash,
    /// Block height
    pub block_height: u64,
    /// Failure reason
    pub reason: String,
    /// Validation level at which failure occurred
    pub validation_level: ValidationLevel,
    /// Whether failure is due to federation signature issues
    pub federation_signature_issue: bool,
    /// Whether failure is due to governance compliance
    pub governance_compliance_issue: bool,
}

/// Processing performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMetrics {
    /// Total processing time
    pub total_time: Duration,
    /// Average processing time per block
    pub avg_time_per_block: Duration,
    /// Peak memory usage during processing
    pub peak_memory_usage: u64,
    /// Parallel efficiency (0.0 to 1.0)
    pub parallel_efficiency: f64,
    /// Validation worker utilization
    pub worker_utilization: Vec<f64>,
}

/// Validation result with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// Block that was validated
    pub block_hash: BlockHash,
    /// Validation time
    pub validation_time: Duration,
    /// Validation level used
    pub validation_level: ValidationLevel,
    /// Error message if validation failed
    pub error_message: Option<String>,
    /// Federation signature verification result
    pub federation_signature_valid: bool,
    /// Governance compliance check result
    pub governance_compliant: bool,
    /// Additional validation context
    pub validation_context: ValidationContext,
}

/// Additional context for block validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationContext {
    /// Validator worker ID that performed validation
    pub worker_id: usize,
    /// Validation timestamp
    pub timestamp: Instant,
    /// Parent block validation status
    pub parent_valid: bool,
    /// State root verification result
    pub state_root_valid: bool,
    /// Transaction validation results
    pub transaction_validations: Vec<TransactionValidationResult>,
    /// Consensus-specific validation results
    pub consensus_validations: ConsensusValidationResult,
}

/// Transaction validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionValidationResult {
    /// Transaction hash
    pub tx_hash: TransactionHash,
    /// Validation result
    pub valid: bool,
    /// Error message if invalid
    pub error: Option<String>,
    /// Gas usage validation
    pub gas_valid: bool,
    /// Signature validation
    pub signature_valid: bool,
}

/// Consensus-specific validation results for Alys PoA
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusValidationResult {
    /// Aura slot validation
    pub slot_valid: bool,
    /// Producer authorization validation
    pub producer_authorized: bool,
    /// Federation signature validation
    pub federation_signature_valid: bool,
    /// Block timing validation (2-second slots)
    pub timing_valid: bool,
    /// Parent block hash validation
    pub parent_hash_valid: bool,
    /// Difficulty adjustment validation
    pub difficulty_valid: bool,
}

/// Peer performance update information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPerformanceUpdate {
    /// Response time for recent operations
    pub response_time: Duration,
    /// Blocks successfully served
    pub blocks_served: u64,
    /// Errors encountered
    pub error_count: u32,
    /// Bandwidth measurement
    pub bandwidth_measurement: f64,
    /// Reliability score update
    pub reliability_update: f64,
    /// Timestamp of update
    pub timestamp: Instant,
}

/// Governance event from Anduro stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEvent {
    /// Event ID
    pub event_id: String,
    /// Event type
    pub event_type: String,
    /// Event payload
    pub payload: serde_json::Value,
    /// Event timestamp
    pub timestamp: SystemTime,
    /// Processing deadline
    pub deadline: Option<Instant>,
    /// Event priority
    pub priority: GovernanceEventPriority,
    /// Related block height (if applicable)
    pub block_height: Option<u64>,
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetricsSnapshot {
    /// Snapshot timestamp
    pub timestamp: Instant,
    /// Blocks processed since last snapshot
    pub blocks_processed: u64,
    /// Processing rate (blocks/sec)
    pub processing_rate: f64,
    /// Error count since last snapshot
    pub error_count: u32,
    /// Memory usage
    pub memory_usage: u64,
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Network utilization
    pub network_utilization: f64,
    /// Peer count
    pub peer_count: usize,
    /// Governance events processed
    pub governance_events_processed: u32,
}

impl Default for SyncMode {
    fn default() -> Self {
        SyncMode::Fast
    }
}

impl Default for SyncPriority {
    fn default() -> Self {
        SyncPriority::Normal
    }
}

impl Default for ProcessingPriority {
    fn default() -> Self {
        ProcessingPriority::Normal
    }
}

impl Default for ValidationLevel {
    fn default() -> Self {
        ValidationLevel::Full
    }
}

impl Default for GovernanceEventPriority {
    fn default() -> Self {
        GovernanceEventPriority::Normal
    }
}

impl SyncState {
    /// Check if sync is actively processing blocks
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SyncState::Discovering { .. } |
            SyncState::DownloadingHeaders { .. } |
            SyncState::DownloadingBlocks { .. } |
            SyncState::CatchingUp { .. }
        )
    }

    /// Check if sync is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SyncState::Synced { .. } |
            SyncState::Failed { can_retry: false, .. }
        )
    }

    /// Check if sync can be resumed from current state
    pub fn can_resume(&self) -> bool {
        matches!(
            self,
            SyncState::Paused { can_resume: true, .. } |
            SyncState::Failed { can_retry: true, .. }
        )
    }

    /// Get progress percentage for states that support it
    pub fn progress(&self) -> Option<f64> {
        match self {
            SyncState::DownloadingHeaders { current, target, .. } |
            SyncState::DownloadingBlocks { current, target, .. } => {
                if *target > 0 {
                    Some(*current as f64 / *target as f64)
                } else {
                    None
                }
            }
            SyncState::CatchingUp { blocks_behind, .. } => {
                // Inverse progress based on how close we are to being caught up
                if *blocks_behind <= 1000 {
                    Some(1.0 - (*blocks_behind as f64 / 1000.0))
                } else {
                    Some(0.0)
                }
            }
            SyncState::Synced { .. } => Some(1.0),
            _ => None,
        }
    }
}

/// Block processing messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<Vec<ValidationResult>>")]
pub struct ProcessBlocks {
    /// Blocks to process and validate
    pub blocks: Vec<Block>,
    /// Source peer that provided the blocks
    pub source_peer: Option<PeerId>,
    /// Batch processing configuration
    pub batch_config: Option<BatchConfig>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Batch processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Processing timeout
    pub timeout: Duration,
    /// Validation mode for the batch
    pub validation_mode: ValidationMode,
    /// Priority for the batch
    pub priority: ValidationPriority,
}

/// Validation result message
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct ValidationResult {
    /// Block hash that was validated
    pub block_hash: BlockHash,
    /// Whether validation passed
    pub is_valid: bool,
    /// Error if validation failed
    pub error: Option<SyncError>,
    /// Time taken for validation
    pub validation_time: Duration,
    /// Worker ID that performed validation
    pub worker_id: Option<usize>,
}

/// Batch processing result
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct BatchResult {
    /// Batch ID
    pub batch_id: u64,
    /// Individual validation results
    pub results: Vec<ValidationResult>,
    /// Batch processing metrics
    pub metrics: BatchMetrics,
    /// Source peer for the batch
    pub source_peer: Option<PeerId>,
}

/// Batch processing metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMetrics {
    /// Total processing time
    pub total_time: Duration,
    /// Number of blocks processed
    pub blocks_processed: usize,
    /// Number of validation failures
    pub validation_failures: usize,
    /// Average validation time per block
    pub avg_validation_time: Duration,
    /// Peak memory usage
    pub peak_memory_usage: u64,
}

/// Validation mode enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationMode {
    /// Complete validation including state
    Full,
    /// Header and signature validation only
    HeaderOnly,
    /// Optimized for sync performance
    FastSync,
    /// Checkpoint validation
    Checkpoint,
}

/// Validation priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationPriority {
    /// Critical consensus blocks
    Emergency = 0,
    /// Federation blocks  
    High = 1,
    /// Regular sync blocks
    Normal = 2,
    /// Background verification
    Low = 3,
}

impl Default for ValidationMode {
    fn default() -> Self {
        ValidationMode::Full
    }
}

impl Default for ValidationPriority {
    fn default() -> Self {
        ValidationPriority::Normal
    }
}

/// Checkpoint management messages
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<String>")]
pub struct CreateCheckpoint {
    /// Height to create checkpoint at (None = current height)
    pub height: Option<u64>,
    /// Checkpoint type
    pub checkpoint_type: CheckpointType,
    /// Force checkpoint creation even if not scheduled
    pub force: bool,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<Option<RecoveryResult>>")]
pub struct RecoverFromCheckpoint {
    /// Specific checkpoint ID to recover from (None = latest)
    pub checkpoint_id: Option<String>,
    /// Recovery strategy to use
    pub strategy: Option<RecoveryStrategy>,
    /// Skip verification during recovery
    pub skip_verification: bool,
    /// Maximum recovery time allowed
    pub timeout: Option<Duration>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<Vec<CheckpointInfo>>")]
pub struct ListCheckpoints {
    /// Maximum number of checkpoints to return
    pub limit: Option<usize>,
    /// Include detailed checkpoint information
    pub include_details: bool,
    /// Filter by checkpoint type
    pub filter_type: Option<CheckpointType>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<()>")]
pub struct DeleteCheckpoint {
    /// Checkpoint ID to delete
    pub checkpoint_id: String,
    /// Force deletion even if checkpoint is referenced
    pub force: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

#[derive(Message, Debug, Clone, Serialize, Deserialize)]
#[rtype(result = "SyncResult<CheckpointStatus>")]
pub struct GetCheckpointStatus {
    /// Include storage statistics
    pub include_storage_stats: bool,
    /// Include recovery capabilities
    pub include_recovery_info: bool,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
}

/// Checkpoint-related types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Fast recovery with minimal validation
    Fast,
    /// Balanced recovery with essential validation
    Safe,
    /// Minimal recovery - basic state only
    Minimal,
    /// Complete recovery with full validation
    Full,
}

/// Checkpoint information summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    /// Checkpoint identifier
    pub id: String,
    /// Block height
    pub height: u64,
    /// Block hash
    pub block_hash: BlockHash,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Checkpoint type
    pub checkpoint_type: CheckpointType,
    /// Size in bytes
    pub size_bytes: u64,
    /// Verification status
    pub verified: bool,
    /// Recovery time estimate
    pub recovery_estimate: Duration,
}

/// Checkpoint system status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointStatus {
    /// Number of active checkpoints
    pub active_checkpoints: usize,
    /// Total storage used
    pub storage_used_bytes: u64,
    /// Last checkpoint created
    pub last_checkpoint: Option<CheckpointInfo>,
    /// Next scheduled checkpoint height
    pub next_scheduled_height: Option<u64>,
    /// Recovery capabilities
    pub recovery_available: bool,
    /// Storage health
    pub storage_healthy: bool,
    /// Recent checkpoint operations
    pub recent_operations: Vec<CheckpointOperation>,
}

/// Checkpoint operation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointOperation {
    /// Operation type
    pub operation: String,
    /// Checkpoint ID involved
    pub checkpoint_id: String,
    /// Operation timestamp
    pub timestamp: DateTime<Utc>,
    /// Operation result
    pub success: bool,
    /// Duration of operation
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
}

impl Default for CheckpointType {
    fn default() -> Self {
        CheckpointType::Scheduled
    }
}

impl Default for RecoveryStrategy {
    fn default() -> Self {
        RecoveryStrategy::Safe
    }
}

use chrono::{DateTime, Utc};