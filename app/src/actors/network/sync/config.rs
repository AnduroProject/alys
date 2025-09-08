//! Comprehensive configuration for SyncActor operations
//!
//! This module provides detailed configuration options for all aspects of the
//! SyncActor including performance tuning, security settings, federation parameters,
//! governance stream integration, and network optimization for Alys V2 architecture.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::Duration;
use crate::types::*;
use super::errors::*;

/// Main configuration for SyncActor with comprehensive tuning options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Core synchronization parameters
    pub core: CoreSyncConfig,
    
    /// Performance optimization settings
    pub performance: PerformanceConfig,
    
    /// Security and validation settings
    pub security: SecurityConfig,
    
    /// Network and peer management configuration
    pub network: NetworkConfig,
    
    /// Checkpoint system configuration
    pub checkpoint: CheckpointConfig,
    
    /// Federation-specific settings for Alys PoA
    pub federation: FederationConfig,
    
    /// Governance stream integration settings
    pub governance: GovernanceConfig,
    
    /// Mining and auxiliary PoW settings
    pub mining: MiningConfig,
    
    /// Monitoring and metrics configuration
    pub monitoring: MonitoringConfig,
    
    /// Emergency response configuration
    pub emergency: EmergencyConfig,
}

/// Core synchronization parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreSyncConfig {
    /// Checkpoint creation interval (blocks)
    pub checkpoint_interval: u64,
    
    /// Maximum number of checkpoints to retain
    pub max_checkpoints: usize,
    
    /// Minimum batch size for block downloads
    pub batch_size_min: usize,
    
    /// Maximum batch size for block downloads
    pub batch_size_max: usize,
    
    /// Number of parallel download workers
    pub parallel_downloads: usize,
    
    /// Number of validation workers
    pub validation_workers: usize,
    
    /// Block production threshold (99.5% = 0.995)
    pub production_threshold: f64,
    
    /// Minimum peer score threshold for inclusion
    pub peer_score_threshold: f64,
    
    /// Request timeout for individual operations
    pub request_timeout: Duration,
    
    /// Sync lookahead distance (blocks)
    pub sync_lookahead: u64,
    
    /// Maximum sync age before restart
    pub max_sync_age: Duration,
}

/// Performance optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable SIMD optimizations for hash calculations
    pub enable_simd_optimization: bool,
    
    /// Memory pool size for block buffering
    pub memory_pool_size: usize,
    
    /// Target sync speed (blocks per second)
    pub target_sync_speed: f64,
    
    /// Maximum memory usage (bytes)
    pub max_memory_usage: u64,
    
    /// CPU utilization target (0.0 to 1.0)
    pub cpu_utilization_target: f64,
    
    /// Network bandwidth limit (bytes/sec)
    pub network_bandwidth_limit: Option<u64>,
    
    /// Disk I/O optimization settings
    pub disk_io: DiskIOConfig,
    
    /// Adaptive batching configuration
    pub adaptive_batching: AdaptiveBatchingConfig,
    
    /// Parallel processing tuning
    pub parallel_processing: ParallelProcessingConfig,
    
    /// Cache optimization settings
    pub cache: CacheConfig,
}

/// Disk I/O optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIOConfig {
    /// Enable memory-mapped I/O
    pub enable_mmap: bool,
    
    /// Enable io_uring for Linux systems
    pub enable_io_uring: bool,
    
    /// Buffer size for I/O operations
    pub buffer_size: usize,
    
    /// Enable write-ahead logging optimization
    pub enable_wal_optimization: bool,
    
    /// Compression level for stored data
    pub compression_level: u8,
    
    /// Enable async I/O
    pub enable_async_io: bool,
}

/// Adaptive batching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveBatchingConfig {
    /// Enable adaptive batch sizing
    pub enabled: bool,
    
    /// Latency weight in batch size calculation
    pub latency_weight: f64,
    
    /// Bandwidth weight in batch size calculation
    pub bandwidth_weight: f64,
    
    /// Peer count weight in batch size calculation
    pub peer_count_weight: f64,
    
    /// Memory pressure weight in batch size calculation
    pub memory_pressure_weight: f64,
    
    /// Batch size adjustment frequency
    pub adjustment_interval: Duration,
    
    /// Maximum batch size increase per adjustment
    pub max_increase_per_adjustment: f64,
    
    /// Maximum batch size decrease per adjustment
    pub max_decrease_per_adjustment: f64,
}

/// Parallel processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelProcessingConfig {
    /// Maximum parallel validation workers
    pub max_validation_workers: usize,
    
    /// Maximum parallel download workers
    pub max_download_workers: usize,
    
    /// Work stealing enabled between workers
    pub work_stealing_enabled: bool,
    
    /// Worker affinity to CPU cores
    pub cpu_affinity_enabled: bool,
    
    /// Preferred CPU cores for workers
    pub preferred_cpu_cores: Vec<usize>,
    
    /// Worker queue size
    pub worker_queue_size: usize,
    
    /// Load balancing strategy
    pub load_balancing_strategy: LoadBalancingStrategy,
}

/// Load balancing strategies for worker allocation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    LeastLoaded,
    Random,
    CpuAffinity,
    Custom,
}

/// Cache optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Block cache size (number of blocks)
    pub block_cache_size: usize,
    
    /// Header cache size (number of headers)
    pub header_cache_size: usize,
    
    /// State cache size (bytes)
    pub state_cache_size: u64,
    
    /// Peer info cache size
    pub peer_cache_size: usize,
    
    /// Cache eviction strategy
    pub eviction_strategy: CacheEvictionStrategy,
    
    /// Cache compression enabled
    pub compression_enabled: bool,
    
    /// Cache persistence to disk
    pub persistent_cache: bool,
}

/// Cache eviction strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CacheEvictionStrategy {
    LRU,  // Least Recently Used
    LFU,  // Least Frequently Used
    FIFO, // First In, First Out
    Random,
    TTL,  // Time To Live
}

/// Security and validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable Byzantine fault tolerance
    pub byzantine_fault_tolerance: bool,
    
    /// Maximum Byzantine nodes tolerated (f in 3f+1)
    pub max_byzantine_nodes: u32,
    
    /// Enable signature verification caching
    pub signature_cache_enabled: bool,
    
    /// Signature cache size
    pub signature_cache_size: usize,
    
    /// Enable peer reputation tracking
    pub peer_reputation_enabled: bool,
    
    /// Peer blacklist configuration
    pub peer_blacklist: PeerBlacklistConfig,
    
    /// Rate limiting configuration
    pub rate_limiting: RateLimitingConfig,
    
    /// Security event detection
    pub security_monitoring: SecurityMonitoringConfig,
}

/// Peer blacklist configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerBlacklistConfig {
    /// Enable automatic blacklisting
    pub enabled: bool,
    
    /// Error threshold for blacklisting
    pub error_threshold: u32,
    
    /// Blacklist duration
    pub blacklist_duration: Duration,
    
    /// Maximum blacklist size
    pub max_blacklist_size: usize,
    
    /// Automatic removal after good behavior
    pub auto_remove_after_good_behavior: bool,
    
    /// Good behavior threshold for removal
    pub good_behavior_threshold: u32,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Enable rate limiting
    pub enabled: bool,
    
    /// Requests per second limit per peer
    pub requests_per_second_per_peer: f64,
    
    /// Burst allowance
    pub burst_allowance: u32,
    
    /// Rate limit window size
    pub window_size: Duration,
    
    /// Penalty for rate limit violations
    pub violation_penalty: Duration,
}

/// Security monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMonitoringConfig {
    /// Enable anomaly detection
    pub anomaly_detection_enabled: bool,
    
    /// Anomaly detection sensitivity (0.0 to 1.0)
    pub anomaly_sensitivity: f64,
    
    /// Enable attack pattern recognition
    pub attack_pattern_recognition: bool,
    
    /// Security event notification threshold
    pub notification_threshold: SecuritySeverity,
    
    /// Automatic mitigation enabled
    pub auto_mitigation_enabled: bool,
}

/// Network and peer management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Minimum required peers for sync
    pub min_peers: usize,
    
    /// Target number of peers to maintain
    pub target_peers: usize,
    
    /// Maximum number of peers to track
    pub max_peers: usize,
    
    /// Peer discovery configuration
    pub peer_discovery: PeerDiscoveryConfig,
    
    /// Connection management settings
    pub connection_management: ConnectionManagementConfig,
    
    /// Network health monitoring
    pub health_monitoring: NetworkHealthConfig,
    
    /// Partition detection and recovery
    pub partition_recovery: PartitionRecoveryConfig,
}

/// Peer discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDiscoveryConfig {
    /// Enable automatic peer discovery
    pub enabled: bool,
    
    /// Discovery interval
    pub discovery_interval: Duration,
    
    /// Bootstrap peers
    pub bootstrap_peers: Vec<String>,
    
    /// Discovery timeout
    pub discovery_timeout: Duration,
    
    /// Maximum discovery attempts
    pub max_discovery_attempts: u32,
}

/// Connection management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionManagementConfig {
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Keep-alive interval
    pub keep_alive_interval: Duration,
    
    /// Maximum connection retries
    pub max_connection_retries: u32,
    
    /// Retry backoff multiplier
    pub retry_backoff_multiplier: f64,
    
    /// Connection pool size
    pub connection_pool_size: usize,
}

/// Network health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHealthConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    
    /// Latency threshold for healthy peers (milliseconds)
    pub latency_threshold_ms: u64,
    
    /// Bandwidth threshold for healthy peers (bytes/sec)
    pub bandwidth_threshold_bps: u64,
    
    /// Reliability threshold (0.0 to 1.0)
    pub reliability_threshold: f64,
    
    /// Network partition detection timeout
    pub partition_detection_timeout: Duration,
}

/// Partition recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionRecoveryConfig {
    /// Enable automatic partition recovery
    pub enabled: bool,
    
    /// Recovery strategy
    pub default_strategy: PartitionRecoveryStrategy,
    
    /// Recovery timeout
    pub recovery_timeout: Duration,
    
    /// Maximum recovery attempts
    pub max_recovery_attempts: u32,
    
    /// Recovery backoff interval
    pub recovery_backoff: Duration,
}

/// Checkpoint system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Enable checkpoint system
    pub enabled: bool,
    
    /// Checkpoint creation interval (blocks)
    pub creation_interval: u64,
    
    /// Maximum checkpoints to retain
    pub max_retained: usize,
    
    /// Checkpoint verification timeout
    pub verification_timeout: Duration,
    
    /// Checkpoint storage configuration
    pub storage: CheckpointStorageConfig,
    
    /// Checkpoint compression settings
    pub compression: CheckpointCompressionConfig,
    
    /// Checkpoint validation rules
    pub validation: CheckpointValidationConfig,
}

/// Checkpoint storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointStorageConfig {
    /// Storage backend type
    pub backend: CheckpointStorageBackend,
    
    /// Storage directory path
    pub storage_path: String,
    
    /// Enable atomic writes
    pub atomic_writes: bool,
    
    /// Enable write-ahead logging
    pub wal_enabled: bool,
    
    /// Sync to disk frequency
    pub sync_frequency: Duration,
}

/// Checkpoint storage backend options
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckpointStorageBackend {
    File,
    LevelDB,
    RocksDB,
    InMemory,
}

/// Checkpoint compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointCompressionConfig {
    /// Enable compression
    pub enabled: bool,
    
    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,
    
    /// Compression level (1-9)
    pub level: u8,
    
    /// Minimum size threshold for compression
    pub min_size_threshold: u64,
}

/// Compression algorithms
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    Gzip,
    Zstd,
    Lz4,
    Snappy,
}

/// Checkpoint validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointValidationConfig {
    /// Enable checksum validation
    pub checksum_validation: bool,
    
    /// Enable signature validation
    pub signature_validation: bool,
    
    /// Enable state root validation
    pub state_root_validation: bool,
    
    /// Validation timeout
    pub validation_timeout: Duration,
    
    /// Retry failed validations
    pub retry_failed_validations: bool,
    
    /// Maximum validation retries
    pub max_validation_retries: u32,
}

/// Federation-specific configuration for Alys PoA consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Federation member count
    pub member_count: u32,
    
    /// Required signature threshold
    pub signature_threshold: u32,
    
    /// Aura slot duration (milliseconds)
    pub slot_duration_ms: u64,
    
    /// Maximum slots without block production
    pub max_empty_slots: u32,
    
    /// Enable federation signature caching
    pub signature_caching: bool,
    
    /// Federation health monitoring
    pub health_monitoring: FederationHealthConfig,
    
    /// Authority rotation settings
    pub authority_rotation: AuthorityRotationConfig,
}

/// Federation health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationHealthConfig {
    /// Health check interval
    pub check_interval: Duration,
    
    /// Minimum online authorities required
    pub min_online_authorities: u32,
    
    /// Authority response timeout
    pub authority_timeout: Duration,
    
    /// Enable automatic authority replacement
    pub auto_authority_replacement: bool,
}

/// Authority rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityRotationConfig {
    /// Enable authority rotation
    pub enabled: bool,
    
    /// Rotation interval (blocks)
    pub rotation_interval: u64,
    
    /// Rotation strategy
    pub rotation_strategy: RotationStrategy,
    
    /// Advance notice for rotation (blocks)
    pub rotation_notice_blocks: u64,
}

/// Authority rotation strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RotationStrategy {
    RoundRobin,
    Performance,
    Random,
    Manual,
}

/// Governance stream integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Enable governance stream integration
    pub enabled: bool,
    
    /// Governance stream endpoint
    pub stream_endpoint: String,
    
    /// Stream connection timeout
    pub connection_timeout: Duration,
    
    /// Event processing configuration
    pub event_processing: GovernanceEventConfig,
    
    /// Stream health monitoring
    pub health_monitoring: GovernanceHealthConfig,
    
    /// Event buffer configuration
    pub event_buffer: EventBufferConfig,
}

/// Governance event processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEventConfig {
    /// Event processing timeout
    pub processing_timeout: Duration,
    
    /// Maximum event queue size
    pub max_queue_size: usize,
    
    /// Event priority mapping
    pub priority_mapping: HashMap<String, GovernanceEventPriority>,
    
    /// Enable event validation
    pub event_validation: bool,
    
    /// Event retention duration
    pub retention_duration: Duration,
}

/// Governance stream health monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceHealthConfig {
    /// Health check interval
    pub check_interval: Duration,
    
    /// Connection health timeout
    pub connection_health_timeout: Duration,
    
    /// Event processing health threshold
    pub processing_health_threshold: Duration,
    
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
}

/// Event buffer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBufferConfig {
    /// Buffer size for governance events
    pub buffer_size: usize,
    
    /// Buffer overflow strategy
    pub overflow_strategy: BufferOverflowStrategy,
    
    /// Enable persistent buffering
    pub persistent_buffer: bool,
    
    /// Buffer flush interval
    pub flush_interval: Duration,
}

/// Buffer overflow strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BufferOverflowStrategy {
    DropOldest,
    DropNewest,
    Block,
    Expand,
}

/// Mining and auxiliary PoW configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningConfig {
    /// Enable merged mining integration
    pub merged_mining_enabled: bool,
    
    /// Maximum blocks without PoW before halt
    pub max_blocks_without_pow: u64,
    
    /// Block bundle size for merged mining
    pub block_bundle_size: u32,
    
    /// Mining timeout configuration
    pub mining_timeout: MiningTimeoutConfig,
    
    /// Auxiliary PoW validation settings
    pub auxpow_validation: AuxPowValidationConfig,
    
    /// Mining performance monitoring
    pub performance_monitoring: MiningPerformanceConfig,
}

/// Mining timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningTimeoutConfig {
    /// Timeout warning threshold (blocks)
    pub warning_threshold: u64,
    
    /// Timeout critical threshold (blocks)
    pub critical_threshold: u64,
    
    /// Timeout emergency threshold (blocks)
    pub emergency_threshold: u64,
    
    /// Enable automatic mining fallback
    pub auto_fallback_enabled: bool,
    
    /// Fallback mining difficulty
    pub fallback_difficulty: Option<u64>,
}

/// Auxiliary PoW validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxPowValidationConfig {
    /// Enable strict AuxPoW validation
    pub strict_validation: bool,
    
    /// Enable merkle root validation
    pub merkle_root_validation: bool,
    
    /// Enable chain work validation
    pub chain_work_validation: bool,
    
    /// Validation timeout
    pub validation_timeout: Duration,
    
    /// Enable validation result caching
    pub validation_caching: bool,
}

/// Mining performance monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningPerformanceConfig {
    /// Monitor mining latency
    pub monitor_latency: bool,
    
    /// Monitor mining throughput
    pub monitor_throughput: bool,
    
    /// Monitor miner connectivity
    pub monitor_connectivity: bool,
    
    /// Performance alert thresholds
    pub alert_thresholds: MiningAlertThresholds,
}

/// Mining performance alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningAlertThresholds {
    /// Maximum acceptable mining latency
    pub max_mining_latency: Duration,
    
    /// Minimum acceptable mining throughput
    pub min_mining_throughput: f64,
    
    /// Maximum acceptable blocks without PoW
    pub max_blocks_without_pow: u64,
}

/// Monitoring and metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable detailed metrics collection
    pub detailed_metrics: bool,
    
    /// Metrics collection interval
    pub collection_interval: Duration,
    
    /// Metrics retention duration
    pub retention_duration: Duration,
    
    /// Enable performance profiling
    pub performance_profiling: bool,
    
    /// Profiling sample rate (0.0 to 1.0)
    pub profiling_sample_rate: f64,
    
    /// Enable health checks
    pub health_checks: HealthCheckConfig,
    
    /// Alert configuration
    pub alerting: AlertConfig,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Enable health checks
    pub enabled: bool,
    
    /// Health check interval
    pub check_interval: Duration,
    
    /// Health check timeout
    pub check_timeout: Duration,
    
    /// Health metrics to track
    pub tracked_metrics: Vec<String>,
    
    /// Health threshold configuration
    pub thresholds: HealthThresholds,
}

/// Health threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthThresholds {
    /// Memory usage threshold (percentage)
    pub memory_usage_percent: f64,
    
    /// CPU usage threshold (percentage)
    pub cpu_usage_percent: f64,
    
    /// Disk usage threshold (percentage)
    pub disk_usage_percent: f64,
    
    /// Network latency threshold
    pub network_latency_ms: u64,
    
    /// Error rate threshold (percentage)
    pub error_rate_percent: f64,
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable alerting
    pub enabled: bool,
    
    /// Alert channels configuration
    pub channels: Vec<AlertChannel>,
    
    /// Alert rate limiting
    pub rate_limiting: AlertRateLimiting,
    
    /// Alert severity levels
    pub severity_config: AlertSeverityConfig,
}

/// Alert channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertChannel {
    /// Channel type
    pub channel_type: AlertChannelType,
    
    /// Channel configuration
    pub config: HashMap<String, String>,
    
    /// Minimum severity for this channel
    pub min_severity: ErrorSeverity,
    
    /// Enable this channel
    pub enabled: bool,
}

/// Alert channel types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertChannelType {
    Log,
    Webhook,
    Email,
    Slack,
    Discord,
    Prometheus,
}

/// Alert rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRateLimiting {
    /// Enable rate limiting for alerts
    pub enabled: bool,
    
    /// Maximum alerts per hour
    pub max_alerts_per_hour: u32,
    
    /// Burst allowance
    pub burst_allowance: u32,
    
    /// Cooldown period for repeated alerts
    pub cooldown_period: Duration,
}

/// Alert severity configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSeverityConfig {
    /// Threshold for low severity alerts
    pub low_threshold: f64,
    
    /// Threshold for medium severity alerts
    pub medium_threshold: f64,
    
    /// Threshold for high severity alerts
    pub high_threshold: f64,
    
    /// Threshold for critical severity alerts
    pub critical_threshold: f64,
}

/// Emergency response configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyConfig {
    /// Enable emergency response system
    pub enabled: bool,
    
    /// Emergency detection thresholds
    pub detection_thresholds: EmergencyThresholds,
    
    /// Emergency response actions
    pub response_actions: EmergencyResponseActions,
    
    /// Emergency escalation configuration
    pub escalation: EmergencyEscalationConfig,
}

/// Emergency detection thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyThresholds {
    /// Critical error rate threshold
    pub critical_error_rate: f64,
    
    /// Federation offline threshold
    pub federation_offline_threshold: f64,
    
    /// Mining timeout threshold (blocks)
    pub mining_timeout_threshold: u64,
    
    /// Network partition threshold (duration)
    pub network_partition_threshold: Duration,
    
    /// Governance stream offline threshold
    pub governance_offline_threshold: Duration,
}

/// Emergency response actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyResponseActions {
    /// Enable automatic emergency mode
    pub auto_emergency_mode: bool,
    
    /// Enable automatic checkpoint creation
    pub auto_checkpoint_creation: bool,
    
    /// Enable automatic peer blacklisting
    pub auto_peer_blacklisting: bool,
    
    /// Enable automatic governance fallback
    pub auto_governance_fallback: bool,
    
    /// Enable automatic performance optimization
    pub auto_performance_optimization: bool,
}

/// Emergency escalation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyEscalationConfig {
    /// Enable escalation
    pub enabled: bool,
    
    /// Escalation levels
    pub escalation_levels: Vec<EscalationLevel>,
    
    /// Escalation timeout
    pub escalation_timeout: Duration,
    
    /// Maximum escalation level
    pub max_escalation_level: u32,
}

/// Emergency escalation level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLevel {
    /// Level number
    pub level: u32,
    
    /// Level name
    pub name: String,
    
    /// Actions to take at this level
    pub actions: Vec<EscalationAction>,
    
    /// Time to wait before escalating
    pub escalation_delay: Duration,
}

/// Emergency escalation actions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EscalationAction {
    Alert,
    CreateCheckpoint,
    PauseSync,
    RestartComponents,
    ActivateEmergencyMode,
    NotifyOperators,
    ShutdownGracefully,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            core: CoreSyncConfig::default(),
            performance: PerformanceConfig::default(),
            security: SecurityConfig::default(),
            network: NetworkConfig::default(),
            checkpoint: CheckpointConfig::default(),
            federation: FederationConfig::default(),
            governance: GovernanceConfig::default(),
            mining: MiningConfig::default(),
            monitoring: MonitoringConfig::default(),
            emergency: EmergencyConfig::default(),
        }
    }
}

impl Default for CoreSyncConfig {
    fn default() -> Self {
        Self {
            checkpoint_interval: 1000,
            max_checkpoints: 10,
            batch_size_min: 32,
            batch_size_max: 512,
            parallel_downloads: 8,
            validation_workers: 4,
            production_threshold: 0.995, // 99.5%
            peer_score_threshold: 0.7,
            request_timeout: Duration::from_secs(30),
            sync_lookahead: 100,
            max_sync_age: Duration::from_hours(1),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_simd_optimization: true,
            memory_pool_size: 10000,
            target_sync_speed: 100.0,
            max_memory_usage: 2 * 1024 * 1024 * 1024, // 2GB
            cpu_utilization_target: 0.8,
            network_bandwidth_limit: None,
            disk_io: DiskIOConfig::default(),
            adaptive_batching: AdaptiveBatchingConfig::default(),
            parallel_processing: ParallelProcessingConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

impl Default for DiskIOConfig {
    fn default() -> Self {
        Self {
            enable_mmap: true,
            enable_io_uring: cfg!(target_os = "linux"),
            buffer_size: 64 * 1024, // 64KB
            enable_wal_optimization: true,
            compression_level: 6,
            enable_async_io: true,
        }
    }
}

impl Default for AdaptiveBatchingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            latency_weight: 0.3,
            bandwidth_weight: 0.4,
            peer_count_weight: 0.2,
            memory_pressure_weight: 0.1,
            adjustment_interval: Duration::from_secs(30),
            max_increase_per_adjustment: 0.5,
            max_decrease_per_adjustment: 0.3,
        }
    }
}

impl Default for ParallelProcessingConfig {
    fn default() -> Self {
        Self {
            max_validation_workers: num_cpus::get(),
            max_download_workers: 16,
            work_stealing_enabled: true,
            cpu_affinity_enabled: false,
            preferred_cpu_cores: Vec::new(),
            worker_queue_size: 1000,
            load_balancing_strategy: LoadBalancingStrategy::LeastLoaded,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            block_cache_size: 1000,
            header_cache_size: 10000,
            state_cache_size: 100 * 1024 * 1024, // 100MB
            peer_cache_size: 1000,
            eviction_strategy: CacheEvictionStrategy::LRU,
            compression_enabled: true,
            persistent_cache: false,
        }
    }
}

// Implement defaults for other config structures...
// (Additional default implementations follow similar patterns)

impl SyncConfig {
    /// Create a development configuration with relaxed settings
    pub fn development() -> Self {
        let mut config = Self::default();
        
        // Relax performance requirements for development
        config.performance.target_sync_speed = 50.0;
        config.performance.max_memory_usage = 1024 * 1024 * 1024; // 1GB
        
        // Reduce security for faster development iteration
        config.security.signature_cache_enabled = true;
        config.security.peer_reputation_enabled = false;
        
        // Reduce checkpoint frequency for development
        config.checkpoint.creation_interval = 100;
        config.checkpoint.max_retained = 5;
        
        // Enable detailed monitoring for debugging
        config.monitoring.detailed_metrics = true;
        config.monitoring.performance_profiling = true;
        
        config
    }
    
    /// Create a production configuration with strict security
    pub fn production() -> Self {
        let mut config = Self::default();
        
        // Strict security settings
        config.security.byzantine_fault_tolerance = true;
        config.security.peer_reputation_enabled = true;
        config.security.signature_cache_enabled = true;
        
        // Conservative performance settings
        config.performance.target_sync_speed = 200.0;
        config.performance.cpu_utilization_target = 0.6;
        
        // Frequent checkpoints for production reliability
        config.checkpoint.creation_interval = 500;
        config.checkpoint.max_retained = 20;
        
        // Comprehensive monitoring
        config.monitoring.detailed_metrics = true;
        config.monitoring.health_checks.enabled = true;
        config.monitoring.alerting.enabled = true;
        
        // Enable emergency response
        config.emergency.enabled = true;
        
        config
    }
    
    /// Create a testnet configuration balancing performance and reliability
    pub fn testnet() -> Self {
        let mut config = Self::default();
        
        // Moderate security settings
        config.security.peer_reputation_enabled = true;
        config.security.rate_limiting.enabled = true;
        
        // Balanced performance settings
        config.performance.target_sync_speed = 150.0;
        
        // Regular checkpoints
        config.checkpoint.creation_interval = 1000;
        
        // Enable monitoring without overwhelming detail
        config.monitoring.detailed_metrics = false;
        config.monitoring.health_checks.enabled = true;
        
        config
    }
    
    /// Validate configuration for consistency and feasibility
    pub fn validate(&self) -> SyncResult<()> {
        // Validate core configuration
        if self.core.batch_size_min > self.core.batch_size_max {
            return Err(SyncError::Configuration {
                message: "batch_size_min cannot be greater than batch_size_max".to_string(),
            });
        }
        
        if self.core.production_threshold < 0.0 || self.core.production_threshold > 1.0 {
            return Err(SyncError::Configuration {
                message: "production_threshold must be between 0.0 and 1.0".to_string(),
            });
        }
        
        if self.core.validation_workers == 0 {
            return Err(SyncError::Configuration {
                message: "validation_workers must be greater than 0".to_string(),
            });
        }
        
        // Validate federation configuration
        if self.federation.member_count == 0 {
            return Err(SyncError::Configuration {
                message: "federation.member_count must be greater than 0".to_string(),
            });
        }
        
        if self.federation.signature_threshold > self.federation.member_count {
            return Err(SyncError::Configuration {
                message: "federation.signature_threshold cannot exceed member_count".to_string(),
            });
        }
        
        // Validate performance configuration
        if self.performance.max_memory_usage == 0 {
            return Err(SyncError::Configuration {
                message: "performance.max_memory_usage must be greater than 0".to_string(),
            });
        }
        
        if self.performance.cpu_utilization_target > 1.0 {
            return Err(SyncError::Configuration {
                message: "performance.cpu_utilization_target cannot exceed 1.0".to_string(),
            });
        }
        
        // Validate network configuration
        if self.network.min_peers > self.network.max_peers {
            return Err(SyncError::Configuration {
                message: "network.min_peers cannot exceed max_peers".to_string(),
            });
        }
        
        Ok(())
    }
}

use super::messages::*;
use num_cpus;