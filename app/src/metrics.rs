use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use sysinfo::{System, SystemExt, ProcessExt, PidExt};
use serde_json::json;

/// Sync state enumeration for ALYS-003-16
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncState {
    Discovering = 0,
    Headers = 1,
    Blocks = 2,
    Catchup = 3,
    Synced = 4,
    Failed = 5,
}

impl SyncState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncState::Discovering => "discovering",
            SyncState::Headers => "headers",
            SyncState::Blocks => "blocks", 
            SyncState::Catchup => "catchup",
            SyncState::Synced => "synced",
            SyncState::Failed => "failed",
        }
    }
    
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SyncState::Discovering),
            1 => Some(SyncState::Headers),
            2 => Some(SyncState::Blocks),
            3 => Some(SyncState::Catchup),
            4 => Some(SyncState::Synced),
            5 => Some(SyncState::Failed),
            _ => None,
        }
    }
}

/// Transaction rejection reasons for ALYS-003-18
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionRejectionReason {
    InsufficientFee,
    InvalidNonce,
    InsufficientBalance,
    GasLimitExceeded,
    InvalidSignature,
    AccountNotFound,
    PoolFull,
    DuplicateTransaction,
    InvalidTransaction,
    NetworkCongestion,
    RateLimited,
    Other,
}

impl TransactionRejectionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransactionRejectionReason::InsufficientFee => "insufficient_fee",
            TransactionRejectionReason::InvalidNonce => "invalid_nonce", 
            TransactionRejectionReason::InsufficientBalance => "insufficient_balance",
            TransactionRejectionReason::GasLimitExceeded => "gas_limit_exceeded",
            TransactionRejectionReason::InvalidSignature => "invalid_signature",
            TransactionRejectionReason::AccountNotFound => "account_not_found",
            TransactionRejectionReason::PoolFull => "pool_full",
            TransactionRejectionReason::DuplicateTransaction => "duplicate_transaction",
            TransactionRejectionReason::InvalidTransaction => "invalid_transaction",
            TransactionRejectionReason::NetworkCongestion => "network_congestion",
            TransactionRejectionReason::RateLimited => "rate_limited",
            TransactionRejectionReason::Other => "other",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "insufficient_fee" => Some(TransactionRejectionReason::InsufficientFee),
            "invalid_nonce" => Some(TransactionRejectionReason::InvalidNonce),
            "insufficient_balance" => Some(TransactionRejectionReason::InsufficientBalance),
            "gas_limit_exceeded" => Some(TransactionRejectionReason::GasLimitExceeded),
            "invalid_signature" => Some(TransactionRejectionReason::InvalidSignature),
            "account_not_found" => Some(TransactionRejectionReason::AccountNotFound),
            "pool_full" => Some(TransactionRejectionReason::PoolFull),
            "duplicate_transaction" => Some(TransactionRejectionReason::DuplicateTransaction),
            "invalid_transaction" => Some(TransactionRejectionReason::InvalidTransaction),
            "network_congestion" => Some(TransactionRejectionReason::NetworkCongestion),
            "rate_limited" => Some(TransactionRejectionReason::RateLimited),
            "other" => Some(TransactionRejectionReason::Other),
            _ => None,
        }
    }
}

/// Peer geographic regions for ALYS-003-19
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRegion {
    NorthAmerica,
    Europe, 
    Asia,
    SouthAmerica,
    Africa,
    Oceania,
    Unknown,
}

impl PeerRegion {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerRegion::NorthAmerica => "north_america",
            PeerRegion::Europe => "europe",
            PeerRegion::Asia => "asia", 
            PeerRegion::SouthAmerica => "south_america",
            PeerRegion::Africa => "africa",
            PeerRegion::Oceania => "oceania",
            PeerRegion::Unknown => "unknown",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "north_america" | "na" | "us" | "ca" => Some(PeerRegion::NorthAmerica),
            "europe" | "eu" => Some(PeerRegion::Europe),
            "asia" | "ap" => Some(PeerRegion::Asia),
            "south_america" | "sa" => Some(PeerRegion::SouthAmerica),
            "africa" | "af" => Some(PeerRegion::Africa),
            "oceania" | "oc" | "au" => Some(PeerRegion::Oceania),
            "unknown" => Some(PeerRegion::Unknown),
            _ => None,
        }
    }
    
    /// Determine region from IP address (simplified implementation)
    pub fn from_ip(ip: &str) -> Self {
        // This is a simplified implementation. In practice, you'd use a GeoIP database
        // like MaxMind's GeoLite2 or similar service
        if ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("172.") {
            return PeerRegion::Unknown; // Private IP
        }
        
        // Placeholder logic - in reality, you'd map IP ranges to regions
        PeerRegion::Unknown
    }
}

/// Peer connection statistics for ALYS-003-19
#[derive(Debug, Clone, Default)]
pub struct PeerConnectionStats {
    pub successful_connections: u64,
    pub failed_connections: u64,
    pub connection_attempts: u64,
    pub avg_connection_time: Duration,
    pub active_connections: usize,
    pub max_concurrent_connections: usize,
}

impl PeerConnectionStats {
    /// Calculate connection success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        let total_attempts = self.successful_connections + self.failed_connections;
        if total_attempts == 0 {
            0.0
        } else {
            self.successful_connections as f64 / total_attempts as f64
        }
    }
    
    /// Calculate connection failure rate (0.0 to 1.0)
    pub fn failure_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }
    
    /// Check if connection stats indicate healthy networking
    pub fn is_healthy(&self, min_success_rate: f64) -> bool {
        self.success_rate() >= min_success_rate && self.active_connections > 0
    }
}

/// Block timer type for ALYS-003-17
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTimerType {
    Production,
    Validation,
}

/// High-precision block timing utility for ALYS-003-17
#[derive(Debug)]
pub struct BlockTimer {
    timer_type: BlockTimerType,
    start_time: std::time::Instant,
}

impl BlockTimer {
    /// Create a new block timer
    pub fn new(timer_type: BlockTimerType) -> Self {
        Self {
            timer_type,
            start_time: std::time::Instant::now(),
        }
    }
    
    /// Get the elapsed duration
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// Finish timing and record to metrics
    pub fn finish_and_record(self, metrics_collector: &MetricsCollector, validator: &str) -> Duration {
        let elapsed = self.elapsed();
        
        match self.timer_type {
            BlockTimerType::Production => {
                metrics_collector.record_block_production_time(validator, elapsed);
            }
            BlockTimerType::Validation => {
                metrics_collector.record_block_validation_time(validator, elapsed, true);
            }
        }
        
        elapsed
    }
    
    /// Finish timing with success/failure and record to metrics
    pub fn finish_with_result(self, metrics_collector: &MetricsCollector, validator: &str, success: bool) -> Duration {
        let elapsed = self.elapsed();
        
        match self.timer_type {
            BlockTimerType::Production => {
                // Production timer doesn't have success/failure semantics, so just record normally
                metrics_collector.record_block_production_time(validator, elapsed);
            }
            BlockTimerType::Validation => {
                metrics_collector.record_block_validation_time(validator, elapsed, success);
            }
        }
        
        elapsed
    }
}

use lazy_static::lazy_static;

pub mod actor_integration;
pub use actor_integration::{ActorMetricsBridge, ActorType, MessageType};
use prometheus::{
    register_gauge_with_registry, register_histogram_vec_with_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry,
    register_gauge_vec_with_registry, register_int_gauge_vec_with_registry,
    Encoder, Gauge, GaugeVec, Histogram, HistogramVec, IntCounter, IntCounterVec, 
    IntGauge, IntGaugeVec, Registry, TextEncoder,
    HistogramOpts, Opts, Error as PrometheusError,
};

// Create a new registry named `alys`
lazy_static! {
    pub static ref ALYS_REGISTRY: Registry =
        Registry::new_custom(Some("alys".to_string()), None).unwrap();
}

// Register metrics with the `alys` registry
lazy_static! {
    pub static ref AURA_PRODUCED_BLOCKS: IntCounterVec = register_int_counter_vec_with_registry!(
        "aura_produced_blocks_total",
        "Total number of blocks produced by the node via the Aura consensus",
        &["status"],
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref AURA_CURRENT_SLOT: Gauge = register_gauge_with_registry!(
        "aura_current_slot",
        "Tracks the current slot number processed by the node",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref AURA_SLOT_AUTHOR_RETRIEVALS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "aura_slot_author_retrievals",
            "Number of slot author retrievals",
            &["status", "authority_index"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref AURA_LATEST_SLOT_AUTHOR: Gauge = register_gauge_with_registry!(
        "aura_latest_slot_author",
        "The index of the latest slot author",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref AURA_VERIFY_SIGNED_BLOCK: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "aura_verify_signed_block_total",
            "Number of times the proposed block is verified",
            &["status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref AURA_SLOT_CLAIM_TOTALS: IntCounterVec = register_int_counter_vec_with_registry!(
        "aura_slot_claim_totals",
        "Number of slot claims",
        &["status"],
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref AUXPOW_CREATE_BLOCK_CALLS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "create_aux_block_calls_total",
            "Total number of times the create_aux_block method is called",
            &["status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref AUXPOW_SUBMIT_BLOCK_CALLS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "auxpow_submit_block_calls_total",
            "Total number of times the submit_aux_block method is called",
            &["status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref AUXPOW_HASHES_PROCESSED: Histogram = register_histogram_with_registry!(
        "auxpow_hashes_processed",
        "Histogram of the number of hashes processed during aux block creation",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_PEGIN_TOTALS: IntCounterVec = register_int_counter_vec_with_registry!(
        "chain_pegin_totals",
        "Total number of peg-in operations labeled by operation type (add, remove, process)",
        &["status"],
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_TOTAL_PEGIN_AMOUNT: IntGauge = register_int_gauge_with_registry!(
        "chain_total_pegin_amount",
        "Total amount of peg-ins processed",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_PROCESS_BLOCK_ATTEMPTS: IntCounter = register_int_counter_with_registry!(
        "chain_process_block_attempts_total",
        "Number of times process_block is invoked",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_PROCESS_BLOCK_TOTALS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "chain_process_block_totals",
            "Total number of blocks processed, labeled by status",
            &["status", "reason"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref CHAIN_NETWORK_GOSSIP_TOTALS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "chain_network_gossip_totals",
            "Total number of network gossip messages, labeled by message type",
            &["message_type", "status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref CHAIN_LAST_APPROVED_BLOCK: IntGauge = register_int_gauge_with_registry!(
        "last_approved_block",
        "The last block that was approved by node",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_BLOCK_HEIGHT: IntGauge = register_int_gauge_with_registry!(
        "chain_block_height",
        "The current block height of the node",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_BTC_BLOCK_MONITOR_TOTALS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "chain_btc_block_monitor_totals",
            "Total number of BTC block monitor messages, labeled by message type",
            &["block_height", "status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref CHAIN_DISCOVERED_PEERS: Gauge = register_gauge_with_registry!(
        "chain_discovered_peers",
        "Number of discovered peers",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_SYNCING_OPERATION_TOTALS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "chain_syncing_operation_totals",
            "Total number of syncing operations, labeled by operation type (add, remove)",
            &["start_height", "status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref CHAIN_BLOCK_PRODUCTION_TOTALS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "chain_block_production_totals",
            "Total number of blocks produced, labeled by status",
            &["message_type", "status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref CHAIN_LAST_FINALIZED_BLOCK: IntGauge = register_int_gauge_with_registry!(
        "chain_last_finalized_block",
        "The last block that was finalized by node",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref CHAIN_LAST_PROCESSED_BLOCK: IntGauge = register_int_gauge_with_registry!(
        "chain_last_processed_block",
        "The last block that was processed by node",
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref ENGINE_BUILD_BLOCK_CALLS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "engine_build_block_calls_total",
            "Number of times build_block is invoked",
            &["status", "reason"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref ENGINE_COMMIT_BLOCK_CALLS: IntCounterVec =
        register_int_counter_vec_with_registry!(
            "engine_commit_block_calls_total",
            "Number of times commit_block is invoked",
            &["status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref RPC_REQUESTS: IntCounterVec = register_int_counter_vec_with_registry!(
        "rpc_requests_total",
        "Total number of JSON-RPC requests",
        &["method", "status"],
        ALYS_REGISTRY
    )
    .unwrap();
    pub static ref RPC_REQUEST_DURATION: HistogramVec = register_histogram_vec_with_registry!(
        "rpc_request_duration_seconds",
        "Histogram of the time taken to process JSON-RPC requests",
        &["method"],
        ALYS_REGISTRY
    )
    .unwrap();

    // === Migration-Specific Metrics ===
    pub static ref MIGRATION_PHASE: IntGauge = register_int_gauge_with_registry!(
        "alys_migration_phase",
        "Current migration phase (0-10)",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MIGRATION_PROGRESS: Gauge = register_gauge_with_registry!(
        "alys_migration_progress_percent",
        "Migration progress percentage for current phase",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MIGRATION_ERRORS: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_migration_errors_total",
        "Total migration errors encountered",
        &["phase", "error_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MIGRATION_ROLLBACKS: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_migration_rollbacks_total",
        "Total migration rollbacks performed",
        &["phase", "reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MIGRATION_PHASE_DURATION: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_migration_phase_duration_seconds",
            "Time taken to complete each migration phase"
        ),
        &["phase"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MIGRATION_VALIDATION_SUCCESS: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_migration_validation_success_total",
        "Migration validation successes per phase",
        &["phase"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MIGRATION_VALIDATION_FAILURE: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_migration_validation_failure_total",
        "Migration validation failures per phase",
        &["phase"],
        ALYS_REGISTRY
    )
    .unwrap();

    // === Enhanced Actor System Metrics ===
    pub static ref ACTOR_MESSAGE_COUNT: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_actor_messages_total",
        "Total messages processed by actors",
        &["actor_type", "message_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_MESSAGE_LATENCY: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_actor_message_latency_seconds",
            "Time to process actor messages"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
        &["actor_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_MAILBOX_SIZE: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_actor_mailbox_size",
        "Current size of actor mailboxes",
        &["actor_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_RESTARTS: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_actor_restarts_total",
        "Total actor restarts due to failures",
        &["actor_type", "reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_LIFECYCLE_EVENTS: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_actor_lifecycle_events_total",
        "Actor lifecycle events (spawn, stop, recover)",
        &["actor_type", "event"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref ACTOR_MESSAGE_THROUGHPUT: GaugeVec = register_gauge_vec_with_registry!(
        "alys_actor_message_throughput_per_second",
        "Actor message processing throughput",
        &["actor_type"],
        ALYS_REGISTRY
    )
    .unwrap();

    // === Enhanced Sync & Performance Metrics ===
    pub static ref SYNC_CURRENT_HEIGHT: IntGauge = register_int_gauge_with_registry!(
        "alys_sync_current_height",
        "Current synchronized block height",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref SYNC_TARGET_HEIGHT: IntGauge = register_int_gauge_with_registry!(
        "alys_sync_target_height",
        "Target block height from peers",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref SYNC_BLOCKS_PER_SECOND: Gauge = register_gauge_with_registry!(
        "alys_sync_blocks_per_second",
        "Current sync speed in blocks per second",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref SYNC_STATE: IntGauge = register_int_gauge_with_registry!(
        "alys_sync_state",
        "Current sync state (0=discovering, 1=headers, 2=blocks, 3=catchup, 4=synced, 5=failed)",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref BLOCK_PRODUCTION_TIME: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_block_production_duration_seconds",
            "Time to produce a block"
        ).buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]),
        &["validator"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref BLOCK_VALIDATION_TIME: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_block_validation_duration_seconds",
            "Time to validate a block"
        ).buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]),
        &["validator"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref TRANSACTION_POOL_SIZE: IntGauge = register_int_gauge_with_registry!(
        "alys_txpool_size",
        "Current transaction pool size",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref TRANSACTION_POOL_PROCESSING_RATE: Gauge = register_gauge_with_registry!(
        "alys_txpool_processing_rate",
        "Transaction pool processing rate",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref TRANSACTION_POOL_REJECTIONS: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_txpool_rejections_total",
        "Transaction pool rejection counts by reason",
        &["reason"],
        ALYS_REGISTRY
    )
    .unwrap();

    // === Enhanced System Resource Metrics ===
    pub static ref PEER_COUNT: IntGauge = register_int_gauge_with_registry!(
        "alys_peer_count",
        "Number of connected peers",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref PEER_QUALITY_SCORE: GaugeVec = register_gauge_vec_with_registry!(
        "alys_peer_quality_score",
        "Peer connection quality score",
        &["peer_id"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref PEER_GEOGRAPHIC_DISTRIBUTION: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_peer_geographic_distribution",
        "Peer count by geographic region",
        &["region"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref MEMORY_USAGE: IntGauge = register_int_gauge_with_registry!(
        "alys_memory_usage_bytes",
        "Current memory usage in bytes",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref CPU_USAGE: Gauge = register_gauge_with_registry!(
        "alys_cpu_usage_percent",
        "Current CPU usage percentage",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref DISK_IO_BYTES: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_disk_io_bytes_total",
        "Total disk I/O bytes",
        &["operation"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref NETWORK_IO_BYTES: IntCounterVec = register_int_counter_vec_with_registry!(
        "alys_network_io_bytes_total",
        "Total network I/O bytes",
        &["direction"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref THREAD_COUNT: IntGauge = register_int_gauge_with_registry!(
        "alys_thread_count",
        "Current number of threads",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref FILE_DESCRIPTORS: IntGauge = register_int_gauge_with_registry!(
        "alys_file_descriptors",
        "Current number of open file descriptors",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref PROCESS_START_TIME: IntGauge = register_int_gauge_with_registry!(
        "alys_process_start_time_seconds",
        "Process start time in Unix timestamp",
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref UPTIME: IntGauge = register_int_gauge_with_registry!(
        "alys_uptime_seconds",
        "Process uptime in seconds",
        ALYS_REGISTRY
    )
    .unwrap();
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            // Gather the metrics from both the `alys` registry and the default registry
            let mut metric_families = ALYS_REGISTRY.gather();
            metric_families.extend(prometheus::gather());

            let encoder = TextEncoder::new();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer).unwrap();

            let response = Response::builder()
                .status(StatusCode::OK)
                .header(
                    hyper::header::CONTENT_TYPE,
                    encoder.format_type(), // returns "text/plain; version=0.0.4"
                )
                .body(Body::from(buffer))
                .unwrap();

            Ok(response)
        }
        (&Method::GET, "/health") => {
            let health_status = json!({
                "status": "healthy",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "version": env!("CARGO_PKG_VERSION"),
                "metrics_count": ALYS_REGISTRY.gather().len()
            });

            let response = Response::builder()
                .status(StatusCode::OK)
                .header(hyper::header::CONTENT_TYPE, "application/json")
                .body(Body::from(health_status.to_string()))
                .unwrap();

            Ok(response)
        }
        (&Method::GET, "/ready") => {
            // Simple readiness check
            let response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::from("ready"))
                .unwrap();
            Ok(response)
        }
        _ => {
            let mut not_found = Response::new(Body::from("Not Found"));
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

pub async fn start_server(port_number: Option<u16>) {
    // Default port is 9001 if not specified
    const DEFAULT_PORT: u16 = 9001;

    // Use the provided port number or fall back to the default
    let port = port_number.unwrap_or(DEFAULT_PORT);

    // Create a socket address for the server to bind to
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });

    let server = Server::bind(&addr).serve(make_svc);

    // Initialize process start time
    PROCESS_START_TIME.set(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    );

    // TODO: handle graceful shutdown
    tokio::spawn(async move {
        tracing::info!("Starting Enhanced Metrics server on {} with health endpoints", addr);

        if let Err(e) = server.await {
            tracing::error!("Metrics server error: {}", e);
        }
    });
}

/// Disk I/O statistics for system resource monitoring (ALYS-003-20)
#[derive(Debug, Clone, Default)]
pub struct DiskStats {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    pub timestamp: std::time::Instant,
}

impl DiskStats {
    /// Calculate delta stats between two measurements
    pub fn delta(&self, previous: &DiskStats) -> DiskStats {
        let time_delta = self.timestamp.duration_since(previous.timestamp);
        let read_bytes_delta = self.read_bytes.saturating_sub(previous.read_bytes);
        let write_bytes_delta = self.write_bytes.saturating_sub(previous.write_bytes);
        let read_ops_delta = self.read_ops.saturating_sub(previous.read_ops);
        let write_ops_delta = self.write_ops.saturating_sub(previous.write_ops);
        
        DiskStats {
            read_bytes: read_bytes_delta,
            write_bytes: write_bytes_delta,
            read_ops: read_ops_delta,
            write_ops: write_ops_delta,
            timestamp: self.timestamp,
        }
    }
    
    /// Calculate I/O rates in bytes per second
    pub fn calculate_rates(&self, time_window: Duration) -> (f64, f64) {
        let secs = time_window.as_secs_f64();
        if secs > 0.0 {
            (self.read_bytes as f64 / secs, self.write_bytes as f64 / secs)
        } else {
            (0.0, 0.0)
        }
    }
}

/// Network I/O statistics for system resource monitoring (ALYS-003-20)  
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub timestamp: std::time::Instant,
}

impl NetworkStats {
    /// Calculate delta stats between two measurements
    pub fn delta(&self, previous: &NetworkStats) -> NetworkStats {
        let rx_bytes_delta = self.rx_bytes.saturating_sub(previous.rx_bytes);
        let tx_bytes_delta = self.tx_bytes.saturating_sub(previous.tx_bytes);
        let rx_packets_delta = self.rx_packets.saturating_sub(previous.rx_packets);
        let tx_packets_delta = self.tx_packets.saturating_sub(previous.tx_packets);
        
        NetworkStats {
            rx_bytes: rx_bytes_delta,
            tx_bytes: tx_bytes_delta,
            rx_packets: rx_packets_delta,
            tx_packets: tx_packets_delta,
            timestamp: self.timestamp,
        }
    }
    
    /// Calculate network rates in bytes per second
    pub fn calculate_rates(&self, time_window: Duration) -> (f64, f64) {
        let secs = time_window.as_secs_f64();
        if secs > 0.0 {
            (self.rx_bytes as f64 / secs, self.tx_bytes as f64 / secs)
        } else {
            (0.0, 0.0)
        }
    }
}

/// Enhanced metrics server with proper error handling and initialization
pub struct MetricsServer {
    port: u16,
    registry: Registry,
    collector: Option<Arc<MetricsCollector>>,
}

impl MetricsServer {
    /// Create a new MetricsServer instance
    pub fn new(port: u16) -> Self {
        Self {
            port,
            registry: ALYS_REGISTRY.clone(),
            collector: None,
        }
    }

    /// Start the metrics server with automatic resource collection
    pub async fn start_with_collection(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start the metrics collector
        let collector = Arc::new(MetricsCollector::new().await?);
        let collector_handle = collector.start_collection().await;
        self.collector = Some(collector);

        // Start the HTTP server
        self.start_server().await?;

        Ok(())
    }

    /// Start the HTTP server without automatic collection
    async fn start_server(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let make_svc = make_service_fn(|_conn| async {
            Ok::<_, Infallible>(service_fn(handle_request))
        });

        let server = Server::bind(&addr).serve(make_svc);
        
        tracing::info!("Enhanced Metrics server starting on {}", addr);
        tracing::info!("Available endpoints: /metrics, /health, /ready");
        
        server.await?;
        Ok(())
    }

    /// Get metrics registry for external use
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Process resource attribution for detailed tracking (ALYS-003-22)
#[derive(Debug, Clone)]
pub struct ProcessResourceAttribution {
    pub pid: u32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub memory_percentage: f64,
    pub cpu_percent: f64,
    pub relative_cpu_usage: f64,
    pub system_memory_total: u64,
    pub system_memory_used: u64,
    pub system_cpu_count: usize,
    pub timestamp: std::time::SystemTime,
}

impl ProcessResourceAttribution {
    /// Check if resource usage is within healthy limits
    pub fn is_healthy(&self) -> bool {
        self.memory_percentage < 80.0 && self.cpu_percent < 70.0
    }
    
    /// Get resource efficiency score (0.0 to 1.0)
    pub fn efficiency_score(&self) -> f64 {
        // Higher efficiency for lower resource usage relative to system capacity
        let memory_efficiency = 1.0 - (self.memory_percentage / 100.0);
        let cpu_efficiency = 1.0 - (self.cpu_percent / 100.0);
        (memory_efficiency + cpu_efficiency) / 2.0
    }
}

/// Resource status enumeration for health monitoring (ALYS-003-22)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    Healthy,
    Warning,
    Critical,
}

impl ResourceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceStatus::Healthy => "healthy",
            ResourceStatus::Warning => "warning",
            ResourceStatus::Critical => "critical",
        }
    }
}

/// Process health status for comprehensive monitoring (ALYS-003-22)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessHealthStatus {
    Healthy,
    Warning,
    Critical,
}

impl ProcessHealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessHealthStatus::Healthy => "healthy",
            ProcessHealthStatus::Warning => "warning", 
            ProcessHealthStatus::Critical => "critical",
        }
    }
    
    /// Check if status requires immediate attention
    pub fn requires_attention(&self) -> bool {
        matches!(self, ProcessHealthStatus::Warning | ProcessHealthStatus::Critical)
    }
}

/// System resource metrics collector with automated monitoring
pub struct MetricsCollector {
    system: System,
    process_id: u32,
    start_time: std::time::Instant,
    collection_interval: Duration,
    /// Actor metrics bridge for Prometheus integration
    actor_bridge: Option<Arc<ActorMetricsBridge>>,
    /// Previous disk I/O stats for delta calculation
    previous_disk_stats: Arc<parking_lot::Mutex<Option<DiskStats>>>,
    /// Previous network I/O stats for delta calculation  
    previous_network_stats: Arc<parking_lot::Mutex<Option<NetworkStats>>>,
    /// Collection failure count for recovery tracking
    failure_count: Arc<std::sync::atomic::AtomicU64>,
    /// Last successful collection time
    last_successful_collection: Arc<parking_lot::RwLock<std::time::Instant>>,
}

impl MetricsCollector {
    /// Create a new MetricsCollector (ALYS-003-20)
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut system = System::new_all();
        system.refresh_all();
        
        let process_id = std::process::id();
        let start_time = std::time::Instant::now();
        
        tracing::info!("Initializing enhanced MetricsCollector with PID: {} for comprehensive system resource monitoring", process_id);
        
        Ok(Self {
            system,
            process_id,
            start_time,
            collection_interval: Duration::from_secs(5),
            actor_bridge: None,
            previous_disk_stats: Arc::new(parking_lot::Mutex::new(None)),
            previous_network_stats: Arc::new(parking_lot::Mutex::new(None)),
            failure_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_successful_collection: Arc::new(parking_lot::RwLock::new(start_time)),
        })
    }
    
    /// Update sync progress metrics (ALYS-003-16)
    pub fn update_sync_progress(&self, current_height: u64, target_height: u64, sync_speed: f64, sync_state: SyncState) {
        SYNC_CURRENT_HEIGHT.set(current_height as i64);
        SYNC_TARGET_HEIGHT.set(target_height as i64);
        SYNC_BLOCKS_PER_SECOND.set(sync_speed);
        SYNC_STATE.set(sync_state as i64);
        
        // Calculate sync completion percentage
        let sync_percentage = if target_height > 0 {
            (current_height as f64 / target_height as f64) * 100.0
        } else {
            0.0
        };
        
        tracing::debug!(
            current_height = current_height,
            target_height = target_height,
            sync_speed = %format!("{:.2}", sync_speed),
            sync_state = ?sync_state,
            sync_percentage = %format!("{:.1}%", sync_percentage),
            "Sync progress metrics updated"
        );
    }
    
    /// Record sync state change (ALYS-003-16)
    pub fn record_sync_state_change(&self, from_state: SyncState, to_state: SyncState) {
        tracing::info!(
            from_state = ?from_state,
            to_state = ?to_state,
            "Sync state transition recorded"
        );
        
        // Update sync state metric
        SYNC_STATE.set(to_state as i64);
    }
    
    /// Calculate and update sync metrics automatically (ALYS-003-16)
    pub fn calculate_sync_metrics(&self, previous_height: u64, current_height: u64, time_elapsed: Duration) {
        if time_elapsed.as_secs() > 0 && current_height > previous_height {
            let blocks_synced = current_height.saturating_sub(previous_height);
            let sync_speed = blocks_synced as f64 / time_elapsed.as_secs() as f64;
            
            SYNC_BLOCKS_PER_SECOND.set(sync_speed);
            
            tracing::trace!(
                previous_height = previous_height,
                current_height = current_height,
                blocks_synced = blocks_synced,
                time_elapsed_secs = time_elapsed.as_secs(),
                sync_speed = %format!("{:.2}", sync_speed),
                "Sync speed calculated"
            );
        }
    }
    
    /// Record block production timing (ALYS-003-17)
    pub fn record_block_production_time(&self, validator: &str, duration: Duration) {
        let duration_secs = duration.as_secs_f64();
        
        BLOCK_PRODUCTION_TIME
            .with_label_values(&[validator])
            .observe(duration_secs);
        
        tracing::debug!(
            validator = validator,
            duration_ms = duration.as_millis(),
            duration_secs = %format!("{:.3}", duration_secs),
            "Block production timing recorded"
        );
    }
    
    /// Record block validation timing (ALYS-003-17)
    pub fn record_block_validation_time(&self, validator: &str, duration: Duration, success: bool) {
        let duration_secs = duration.as_secs_f64();
        
        BLOCK_VALIDATION_TIME
            .with_label_values(&[validator])
            .observe(duration_secs);
        
        tracing::debug!(
            validator = validator,
            duration_ms = duration.as_millis(),
            duration_secs = %format!("{:.3}", duration_secs),
            validation_success = success,
            "Block validation timing recorded"
        );
    }
    
    /// Start block production timer (ALYS-003-17)
    pub fn start_block_production_timer(&self) -> BlockTimer {
        BlockTimer::new(BlockTimerType::Production)
    }
    
    /// Start block validation timer (ALYS-003-17)  
    pub fn start_block_validation_timer(&self) -> BlockTimer {
        BlockTimer::new(BlockTimerType::Validation)
    }
    
    /// Record block processing pipeline metrics (ALYS-003-17)
    pub fn record_block_pipeline_metrics(
        &self, 
        validator: &str,
        production_time: Duration, 
        validation_time: Duration,
        total_time: Duration,
        block_size: u64,
        transaction_count: u32
    ) {
        // Record individual timings
        self.record_block_production_time(validator, production_time);
        self.record_block_validation_time(validator, validation_time, true);
        
        // Calculate throughput metrics
        let transactions_per_second = if total_time.as_secs_f64() > 0.0 {
            transaction_count as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        let bytes_per_second = if total_time.as_secs_f64() > 0.0 {
            block_size as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        tracing::info!(
            validator = validator,
            production_ms = production_time.as_millis(),
            validation_ms = validation_time.as_millis(),
            total_ms = total_time.as_millis(),
            block_size_bytes = block_size,
            transaction_count = transaction_count,
            txs_per_second = %format!("{:.2}", transactions_per_second),
            bytes_per_second = %format!("{:.2}", bytes_per_second),
            "Block pipeline metrics recorded"
        );
    }
    
    /// Update transaction pool size (ALYS-003-18)
    pub fn update_transaction_pool_size(&self, size: usize) {
        TRANSACTION_POOL_SIZE.set(size as i64);
        
        tracing::trace!(
            txpool_size = size,
            "Transaction pool size updated"
        );
    }
    
    /// Record transaction pool processing rate (ALYS-003-18)
    pub fn record_transaction_processing_rate(&self, transactions_processed: u64, time_window: Duration) {
        let rate = if time_window.as_secs() > 0 {
            transactions_processed as f64 / time_window.as_secs() as f64
        } else {
            0.0
        };
        
        TRANSACTION_POOL_PROCESSING_RATE.set(rate);
        
        tracing::debug!(
            transactions_processed = transactions_processed,
            time_window_secs = time_window.as_secs(),
            processing_rate = %format!("{:.2}", rate),
            "Transaction processing rate recorded"
        );
    }
    
    /// Record transaction rejection (ALYS-003-18)
    pub fn record_transaction_rejection(&self, reason: TransactionRejectionReason) {
        let reason_str = reason.as_str();
        
        TRANSACTION_POOL_REJECTIONS
            .with_label_values(&[reason_str])
            .inc();
        
        tracing::debug!(
            rejection_reason = reason_str,
            "Transaction rejection recorded"
        );
    }
    
    /// Record batch of transaction pool metrics (ALYS-003-18)
    pub fn record_transaction_pool_metrics(
        &self,
        current_size: usize,
        pending_count: usize,
        queued_count: usize,
        processing_rate: f64,
        avg_fee: Option<u64>,
        rejections_in_window: &[(TransactionRejectionReason, u32)],
    ) {
        // Update pool size
        self.update_transaction_pool_size(current_size);
        TRANSACTION_POOL_PROCESSING_RATE.set(processing_rate);
        
        // Record rejections
        for (reason, count) in rejections_in_window {
            let reason_str = reason.as_str();
            TRANSACTION_POOL_REJECTIONS
                .with_label_values(&[reason_str])
                .inc_by(*count as u64);
        }
        
        tracing::info!(
            current_size = current_size,
            pending_count = pending_count,
            queued_count = queued_count,
            processing_rate = %format!("{:.2}", processing_rate),
            avg_fee = ?avg_fee,
            rejection_count = rejections_in_window.len(),
            "Transaction pool metrics updated"
        );
    }
    
    /// Calculate transaction pool health score (ALYS-003-18)
    pub fn calculate_txpool_health_score(&self, max_size: usize, current_size: usize, rejection_rate: f64) -> f64 {
        // Calculate pool utilization (0.0 to 1.0)
        let utilization = if max_size > 0 {
            current_size as f64 / max_size as f64
        } else {
            0.0
        };
        
        // Calculate health score (higher is better)
        // - Low utilization is good (< 80%)
        // - Low rejection rate is good (< 5%)
        let utilization_score = if utilization < 0.8 {
            1.0 - utilization * 0.5  // Penalty increases with utilization
        } else {
            0.1  // Heavy penalty for high utilization
        };
        
        let rejection_score = if rejection_rate < 0.05 {
            1.0 - rejection_rate * 10.0  // Small penalty for low rejection rates
        } else {
            0.1  // Heavy penalty for high rejection rates
        };
        
        let health_score = (utilization_score + rejection_score) / 2.0;
        
        tracing::debug!(
            max_size = max_size,
            current_size = current_size,
            utilization = %format!("{:.1}%", utilization * 100.0),
            rejection_rate = %format!("{:.2}%", rejection_rate * 100.0),
            health_score = %format!("{:.2}", health_score),
            "Transaction pool health calculated"
        );
        
        health_score
    }
    
    /// Update peer count (ALYS-003-19)
    pub fn update_peer_count(&self, count: usize) {
        PEER_COUNT.set(count as i64);
        
        tracing::trace!(
            peer_count = count,
            "Peer count updated"
        );
    }
    
    /// Record peer quality score (ALYS-003-19)
    pub fn record_peer_quality_score(&self, peer_id: &str, quality_score: f64) {
        let sanitized_peer_id = MetricLabels::sanitize_label_value(peer_id);
        
        PEER_QUALITY_SCORE
            .with_label_values(&[&sanitized_peer_id])
            .set(quality_score);
        
        tracing::debug!(
            peer_id = peer_id,
            quality_score = %format!("{:.2}", quality_score),
            "Peer quality score recorded"
        );
    }
    
    /// Update peer geographic distribution (ALYS-003-19)
    pub fn update_peer_geographic_distribution(&self, region_counts: &[(PeerRegion, usize)]) {
        // Reset all regions to 0 first (optional - depends on use case)
        for (region, count) in region_counts {
            let region_str = region.as_str();
            
            PEER_GEOGRAPHIC_DISTRIBUTION
                .with_label_values(&[region_str])
                .set(*count as i64);
        }
        
        let total_peers: usize = region_counts.iter().map(|(_, count)| count).sum();
        
        tracing::debug!(
            total_peers = total_peers,
            regions = region_counts.len(),
            "Peer geographic distribution updated"
        );
    }
    
    /// Record comprehensive peer connection metrics (ALYS-003-19)
    pub fn record_peer_connection_metrics(
        &self,
        connected_peers: usize,
        peer_qualities: &[(String, f64)],
        region_distribution: &[(PeerRegion, usize)],
        connection_stats: &PeerConnectionStats,
    ) {
        // Update peer count
        self.update_peer_count(connected_peers);
        
        // Update quality scores for all peers
        for (peer_id, quality) in peer_qualities {
            self.record_peer_quality_score(peer_id, *quality);
        }
        
        // Update geographic distribution
        self.update_peer_geographic_distribution(region_distribution);
        
        // Calculate average quality score
        let avg_quality = if !peer_qualities.is_empty() {
            peer_qualities.iter().map(|(_, q)| q).sum::<f64>() / peer_qualities.len() as f64
        } else {
            0.0
        };
        
        tracing::info!(
            connected_peers = connected_peers,
            tracked_peer_qualities = peer_qualities.len(),
            avg_quality_score = %format!("{:.2}", avg_quality),
            regions_with_peers = region_distribution.len(),
            successful_connections = connection_stats.successful_connections,
            failed_connections = connection_stats.failed_connections,
            connection_success_rate = %format!("{:.1}%", connection_stats.success_rate() * 100.0),
            "Peer connection metrics recorded"
        );
    }
    
    /// Calculate network health score based on peer metrics (ALYS-003-19)
    pub fn calculate_network_health_score(
        &self, 
        connected_peers: usize, 
        min_peers: usize, 
        optimal_peers: usize,
        avg_quality_score: f64,
        geographic_diversity: usize
    ) -> f64 {
        // Peer count score (0.0 to 1.0)
        let peer_count_score = if connected_peers >= optimal_peers {
            1.0
        } else if connected_peers >= min_peers {
            0.5 + 0.5 * (connected_peers as f64 - min_peers as f64) / (optimal_peers as f64 - min_peers as f64)
        } else {
            connected_peers as f64 / min_peers as f64 * 0.5
        };
        
        // Quality score (already 0.0 to 1.0)
        let quality_score = avg_quality_score.min(1.0).max(0.0);
        
        // Diversity score (higher geographic diversity is better)
        let diversity_score = (geographic_diversity as f64 / 6.0).min(1.0); // Assuming max 6 regions
        
        // Weighted average: peer count (40%), quality (40%), diversity (20%)
        let network_health = 0.4 * peer_count_score + 0.4 * quality_score + 0.2 * diversity_score;
        
        tracing::info!(
            connected_peers = connected_peers,
            min_peers = min_peers,
            optimal_peers = optimal_peers,
            peer_count_score = %format!("{:.2}", peer_count_score),
            avg_quality_score = %format!("{:.2}", avg_quality_score),
            geographic_diversity = geographic_diversity,
            diversity_score = %format!("{:.2}", diversity_score),
            network_health_score = %format!("{:.2}", network_health),
            "Network health score calculated"
        );
        
        network_health
    }
    
    /// Collect disk I/O statistics (ALYS-003-20)
    async fn collect_disk_metrics(&self) -> Result<(), Box<dyn std::error::Error>> {
        let current_stats = self.get_disk_stats().await?;
        
        // Calculate delta if we have previous stats
        if let Some(previous_stats) = self.previous_disk_stats.lock().as_ref() {
            let delta_stats = current_stats.delta(previous_stats);
            let time_window = current_stats.timestamp.duration_since(previous_stats.timestamp);
            let (read_rate, write_rate) = delta_stats.calculate_rates(time_window);
            
            // Update Prometheus metrics
            DISK_IO_BYTES
                .with_label_values(&["read"])
                .inc_by(delta_stats.read_bytes);
                
            DISK_IO_BYTES
                .with_label_values(&["write"])
                .inc_by(delta_stats.write_bytes);
            
            tracing::trace!(
                read_bytes = delta_stats.read_bytes,
                write_bytes = delta_stats.write_bytes,
                read_ops = delta_stats.read_ops,
                write_ops = delta_stats.write_ops,
                read_rate_mbps = read_rate / (1024.0 * 1024.0),
                write_rate_mbps = write_rate / (1024.0 * 1024.0),
                time_window_ms = time_window.as_millis(),
                "Disk I/O metrics collected"
            );
        }
        
        // Store current stats for next collection
        *self.previous_disk_stats.lock() = Some(current_stats);
        
        Ok(())
    }
    
    /// Collect network I/O statistics (ALYS-003-20)
    async fn collect_network_metrics(&self) -> Result<(), Box<dyn std::error::Error>> {
        let current_stats = self.get_network_stats().await?;
        
        // Calculate delta if we have previous stats
        if let Some(previous_stats) = self.previous_network_stats.lock().as_ref() {
            let delta_stats = current_stats.delta(previous_stats);
            let time_window = current_stats.timestamp.duration_since(previous_stats.timestamp);
            let (rx_rate, tx_rate) = delta_stats.calculate_rates(time_window);
            
            // Update Prometheus metrics
            NETWORK_IO_BYTES
                .with_label_values(&["rx"])
                .inc_by(delta_stats.rx_bytes);
                
            NETWORK_IO_BYTES
                .with_label_values(&["tx"])
                .inc_by(delta_stats.tx_bytes);
            
            tracing::trace!(
                rx_bytes = delta_stats.rx_bytes,
                tx_bytes = delta_stats.tx_bytes,
                rx_packets = delta_stats.rx_packets,
                tx_packets = delta_stats.tx_packets,
                rx_rate_mbps = rx_rate / (1024.0 * 1024.0),
                tx_rate_mbps = tx_rate / (1024.0 * 1024.0),
                time_window_ms = time_window.as_millis(),
                "Network I/O metrics collected"
            );
        }
        
        // Store current stats for next collection
        *self.previous_network_stats.lock() = Some(current_stats);
        
        Ok(())
    }
    
    /// Get current disk I/O statistics from system (ALYS-003-20)
    async fn get_disk_stats(&self) -> Result<DiskStats, Box<dyn std::error::Error>> {
        // This is a simplified implementation. In a production system, you would:
        // 1. Read from /proc/diskstats on Linux
        // 2. Use system-specific APIs on other platforms
        // 3. Track per-disk metrics for better granularity
        
        // For now, we'll use process-level I/O if available from sysinfo
        let timestamp = std::time::Instant::now();
        
        // Placeholder implementation - in reality you'd read system disk stats
        let stats = DiskStats {
            read_bytes: 0,  // Would be populated from system stats
            write_bytes: 0, // Would be populated from system stats
            read_ops: 0,    // Would be populated from system stats
            write_ops: 0,   // Would be populated from system stats
            timestamp,
        };
        
        Ok(stats)
    }
    
    /// Get current network I/O statistics from system (ALYS-003-20)
    async fn get_network_stats(&self) -> Result<NetworkStats, Box<dyn std::error::Error>> {
        // This is a simplified implementation. In a production system, you would:
        // 1. Read from /proc/net/dev on Linux
        // 2. Use system-specific APIs on other platforms
        // 3. Track per-interface metrics for better granularity
        
        let timestamp = std::time::Instant::now();
        
        // Get network interfaces from sysinfo
        let networks = self.system.networks();
        let (mut total_rx, mut total_tx) = (0u64, 0u64);
        let (mut total_rx_packets, mut total_tx_packets) = (0u64, 0u64);
        
        for (_interface, network) in networks {
            total_rx += network.received();
            total_tx += network.transmitted();
            total_rx_packets += network.packets_received();
            total_tx_packets += network.packets_transmitted();
        }
        
        let stats = NetworkStats {
            rx_bytes: total_rx,
            tx_bytes: total_tx,
            rx_packets: total_rx_packets,
            tx_packets: total_tx_packets,
            timestamp,
        };
        
        Ok(stats)
    }
    
    /// Collect comprehensive system resource metrics (ALYS-003-20, ALYS-003-21, ALYS-003-22)
    pub async fn collect_comprehensive_system_metrics(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let collection_start = std::time::Instant::now();
        let mut errors = Vec::new();
        
        // Refresh system information
        self.system.refresh_all();
        
        // Collect basic metrics (ALYS-003-20)
        if let Err(e) = self.collect_basic_system_metrics().await {
            errors.push(format!("Basic system metrics: {}", e));
            tracing::warn!("Failed to collect basic system metrics: {}", e);
        }
        
        // Collect process-specific metrics with attribution (ALYS-003-22)
        if let Err(e) = self.collect_process_specific_metrics().await {
            errors.push(format!("Process-specific metrics: {}", e));
            tracing::warn!("Failed to collect process-specific metrics: {}", e);
        }
        
        // Collect disk I/O metrics (ALYS-003-20)
        if let Err(e) = self.collect_disk_metrics().await {
            errors.push(format!("Disk I/O metrics: {}", e));
            tracing::warn!("Failed to collect disk metrics: {}", e);
        }
        
        // Collect network I/O metrics (ALYS-003-20)
        if let Err(e) = self.collect_network_metrics().await {
            errors.push(format!("Network I/O metrics: {}", e));
            tracing::warn!("Failed to collect network metrics: {}", e);
        }
        
        // Collect file descriptor count (ALYS-003-22)
        if let Err(e) = self.collect_file_descriptor_metrics() {
            errors.push(format!("File descriptor metrics: {}", e));
            tracing::warn!("Failed to collect file descriptor metrics: {}", e);
        }
        
        // Track process trends (ALYS-003-22)
        if let Err(e) = self.track_process_trends().await {
            errors.push(format!("Process trend tracking: {}", e));
            tracing::warn!("Failed to track process trends: {}", e);
        }
        
        let collection_duration = collection_start.elapsed();
        
        if errors.is_empty() {
            tracing::debug!(
                collection_duration_ms = collection_duration.as_millis(),
                "Comprehensive system metrics collection completed successfully"
            );
        } else {
            tracing::warn!(
                error_count = errors.len(),
                errors = ?errors,
                collection_duration_ms = collection_duration.as_millis(),
                "Comprehensive system metrics collection completed with errors"
            );
            
            // Return error only if all collections failed
            if errors.len() >= 5 { // We have 5 collection methods
                return Err(format!("All metric collections failed: {:?}", errors).into());
            }
        }
        
        Ok(())
    }
    
    /// Collect file descriptor metrics (ALYS-003-22)
    fn collect_file_descriptor_metrics(&self) -> Result<(), Box<dyn std::error::Error>> {
        // This is platform-specific. On Linux, you'd read from /proc/self/fd
        // For now, we'll provide a placeholder implementation
        
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            match fs::read_dir("/proc/self/fd") {
                Ok(entries) => {
                    let fd_count = entries.count() as i64;
                    FILE_DESCRIPTORS.set(fd_count);
                    
                    tracing::trace!(
                        fd_count = fd_count,
                        "File descriptor count updated"
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to read file descriptor count: {}", e);
                }
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // Placeholder for non-Linux systems
            FILE_DESCRIPTORS.set(0);
        }
        
        Ok(())
    }
    
    /// Create a new MetricsCollector with actor bridge integration
    pub async fn new_with_actor_bridge() -> Result<Self, Box<dyn std::error::Error>> {
        let mut collector = Self::new().await?;
        
        // Initialize actor metrics bridge
        let actor_bridge = Arc::new(ActorMetricsBridge::new(Duration::from_secs(5)));
        collector.actor_bridge = Some(actor_bridge);
        
        tracing::info!("MetricsCollector initialized with actor bridge integration");
        
        Ok(collector)
    }
    
    /// Get the actor metrics bridge for external registration
    pub fn actor_bridge(&self) -> Option<Arc<ActorMetricsBridge>> {
        self.actor_bridge.clone()
    }

    /// Start automated metrics collection with failure recovery (ALYS-003-21)
    pub async fn start_collection(&self) -> tokio::task::JoinHandle<()> {
        let mut collector = self.clone();
        let actor_bridge = self.actor_bridge.clone();
        let failure_count = self.failure_count.clone();
        let last_successful_collection = self.last_successful_collection.clone();
        
        tokio::spawn(async move {
            // Start actor bridge collection if available
            if let Some(bridge) = &actor_bridge {
                let _actor_handle = bridge.start_collection().await;
                tracing::info!("Actor metrics bridge collection started");
            }
            
            let mut interval = interval(collector.collection_interval);
            let mut consecutive_failures = 0u32;
            let max_consecutive_failures = 5;
            let mut backoff_duration = collector.collection_interval;
            
            tracing::info!(
                collection_interval_secs = collector.collection_interval.as_secs(),
                max_consecutive_failures = max_consecutive_failures,
                "Starting enhanced metrics collection with failure recovery"
            );
            
            loop {
                interval.tick().await;
                
                let collection_start = std::time::Instant::now();
                
                // Attempt comprehensive system metrics collection
                match collector.collect_comprehensive_system_metrics().await {
                    Ok(()) => {
                        // Successful collection
                        if consecutive_failures > 0 {
                            tracing::info!(
                                consecutive_failures = consecutive_failures,
                                collection_duration_ms = collection_start.elapsed().as_millis(),
                                "Metrics collection recovered after failures"
                            );
                        }
                        
                        consecutive_failures = 0;
                        backoff_duration = collector.collection_interval;
                        *last_successful_collection.write() = std::time::Instant::now();
                        
                        collector.update_uptime_metrics();
                        
                        // Update actor system health if bridge is available
                        if let Some(bridge) = &actor_bridge {
                            let is_healthy = bridge.is_system_healthy();
                            let stats = bridge.get_aggregate_stats();
                            
                            tracing::trace!(
                                actor_system_healthy = is_healthy,
                                total_actors = stats.total_actors,
                                healthy_actors = stats.healthy_actors,
                                collection_duration_ms = collection_start.elapsed().as_millis(),
                                "Actor system health check completed"
                            );
                        }
                        
                        tracing::trace!(
                            collection_duration_ms = collection_start.elapsed().as_millis(),
                            "System metrics collection completed successfully"
                        );
                    }
                    Err(e) => {
                        // Handle collection failure
                        consecutive_failures += 1;
                        failure_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        
                        let total_failures = failure_count.load(std::sync::atomic::Ordering::Relaxed);
                        let last_success_elapsed = last_successful_collection.read().elapsed();
                        
                        tracing::warn!(
                            error = %e,
                            consecutive_failures = consecutive_failures,
                            total_failures = total_failures,
                            last_success_secs_ago = last_success_elapsed.as_secs(),
                            collection_duration_ms = collection_start.elapsed().as_millis(),
                            "System metrics collection failed"
                        );
                        
                        // Implement exponential backoff for repeated failures
                        if consecutive_failures >= max_consecutive_failures {
                            backoff_duration = std::cmp::min(
                                backoff_duration * 2, 
                                Duration::from_secs(60) // Max 1 minute backoff
                            );
                            
                            tracing::error!(
                                consecutive_failures = consecutive_failures,
                                max_consecutive_failures = max_consecutive_failures,
                                backoff_duration_secs = backoff_duration.as_secs(),
                                "Multiple consecutive metrics collection failures, applying backoff"
                            );
                            
                            // Sleep for backoff duration before next attempt
                            tokio::time::sleep(backoff_duration - collector.collection_interval).await;
                        }
                        
                        // Continue with next iteration despite failure
                        continue;
                    }
                }
                
                // Check if we need to alert on collection health
                let time_since_success = last_successful_collection.read().elapsed();
                if time_since_success > Duration::from_secs(300) { // 5 minutes
                    tracing::error!(
                        time_since_success_secs = time_since_success.as_secs(),
                        total_failures = failure_count.load(std::sync::atomic::Ordering::Relaxed),
                        "Metrics collection has been failing for extended period"
                    );
                }
            }
        })
    }

    /// Collect basic system resource metrics (ALYS-003-20, ALYS-003-22)
    async fn collect_basic_system_metrics(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.system.refresh_all();
        
        // Get process-specific metrics (ALYS-003-22)
        if let Some(process) = self.system.process(sysinfo::Pid::from(self.process_id as usize)) {
            // Memory usage
            let memory_bytes = process.memory() * 1024; // Convert KB to bytes
            MEMORY_USAGE.set(memory_bytes as i64);
            
            // CPU usage
            let cpu_percent = process.cpu_usage() as f64;
            CPU_USAGE.set(cpu_percent);
            
            // Thread count (process-specific when available, otherwise system-wide approximation)
            THREAD_COUNT.set(num_cpus::get() as i64);
            
            tracing::trace!(
                pid = self.process_id,
                memory_mb = memory_bytes / 1024 / 1024,
                cpu_percent = %format!("{:.2}", cpu_percent),
                "Collected process-specific metrics"
            );
        } else {
            tracing::warn!(
                pid = self.process_id,
                "Failed to find process information for metrics collection"
            );
        }
        
        // System-wide metrics (ALYS-003-20)
        let total_memory = self.system.total_memory();
        let used_memory = self.system.used_memory();
        let memory_usage_percent = (used_memory as f64 / total_memory as f64) * 100.0;
        
        // Global CPU usage
        let global_cpu = self.system.global_cpu_info().cpu_usage() as f64;
        
        tracing::trace!(
            total_memory_gb = total_memory / 1024 / 1024 / 1024,
            used_memory_gb = used_memory / 1024 / 1024 / 1024,
            memory_usage_percent = %format!("{:.2}", memory_usage_percent),
            global_cpu_percent = %format!("{:.2}", global_cpu),
            process_count = self.system.processes().len(),
            "Collected system-wide metrics"
        );
        
        Ok(())
    }

    /// Update uptime metrics
    fn update_uptime_metrics(&self) {
        let uptime_seconds = self.start_time.elapsed().as_secs();
        UPTIME.set(uptime_seconds as i64);
    }

    /// Record migration phase change
    pub fn set_migration_phase(&self, phase: u8) {
        MIGRATION_PHASE.set(phase as i64);
        tracing::info!("Migration phase updated to: {}", phase);
    }

    /// Record migration progress
    pub fn set_migration_progress(&self, percent: f64) {
        MIGRATION_PROGRESS.set(percent);
        tracing::debug!("Migration progress: {:.1}%", percent);
    }

    /// Record migration error
    pub fn record_migration_error(&self, phase: &str, error_type: &str) {
        MIGRATION_ERRORS.with_label_values(&[phase, error_type]).inc();
        tracing::warn!("Migration error recorded: phase={}, type={}", phase, error_type);
    }

    /// Record migration rollback
    pub fn record_migration_rollback(&self, phase: &str, reason: &str) {
        MIGRATION_ROLLBACKS.with_label_values(&[phase, reason]).inc();
        tracing::error!("Migration rollback recorded: phase={}, reason={}", phase, reason);
    }

    /// Record validation success
    pub fn record_validation_success(&self, phase: &str) {
        MIGRATION_VALIDATION_SUCCESS.with_label_values(&[phase]).inc();
    }

    /// Record validation failure
    pub fn record_validation_failure(&self, phase: &str) {
        MIGRATION_VALIDATION_FAILURE.with_label_values(&[phase]).inc();
    }
    
    /// Collect detailed process-specific metrics with resource attribution (ALYS-003-22)
    pub async fn collect_process_specific_metrics(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let start_time = std::time::Instant::now();
        
        // Refresh system information
        self.system.refresh_all();
        
        // Get detailed process information
        if let Some(process) = self.system.process(sysinfo::Pid::from(self.process_id as usize)) {
            // Memory metrics with detailed breakdown
            let memory_kb = process.memory();
            let virtual_memory_kb = process.virtual_memory();
            let memory_bytes = memory_kb * 1024;
            let virtual_memory_bytes = virtual_memory_kb * 1024;
            
            MEMORY_USAGE.set(memory_bytes as i64);
            
            // CPU metrics
            let cpu_percent = process.cpu_usage() as f64;
            CPU_USAGE.set(cpu_percent);
            
            // Process runtime and start time
            let process_start_time = process.start_time();
            let process_runtime = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(process_start_time);
            
            tracing::debug!(
                pid = self.process_id,
                memory_mb = memory_kb / 1024,
                virtual_memory_mb = virtual_memory_kb / 1024,
                cpu_percent = %format!("{:.2}", cpu_percent),
                process_runtime_secs = process_runtime,
                process_start_time = process_start_time,
                cmd = ?process.cmd(),
                "Detailed process-specific metrics collected"
            );
            
            // Resource attribution - calculate per-thread estimations if available
            let estimated_threads = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            
            let memory_per_thread = memory_bytes / estimated_threads as u64;
            let cpu_per_thread = cpu_percent / estimated_threads as f64;
            
            tracing::trace!(
                pid = self.process_id,
                estimated_threads = estimated_threads,
                memory_per_thread_mb = memory_per_thread / 1024 / 1024,
                cpu_per_thread_percent = %format!("{:.2}", cpu_per_thread),
                "Resource attribution calculated"
            );
            
        } else {
            tracing::warn!(
                pid = self.process_id,
                "Process not found for detailed metrics collection"
            );
            return Err("Process not found for detailed metrics".into());
        }
        
        // Collect system process statistics
        let total_processes = self.system.processes().len();
        let mut high_memory_processes = 0;
        let mut high_cpu_processes = 0;
        
        for (_pid, process) in self.system.processes() {
            if process.memory() > 1024 * 1024 { // > 1GB
                high_memory_processes += 1;
            }
            if process.cpu_usage() > 50.0 { // > 50% CPU
                high_cpu_processes += 1;
            }
        }
        
        tracing::trace!(
            total_processes = total_processes,
            high_memory_processes = high_memory_processes,
            high_cpu_processes = high_cpu_processes,
            collection_duration_ms = start_time.elapsed().as_millis(),
            "System process statistics collected"
        );
        
        Ok(())
    }
    
    /// Get process resource attribution breakdown (ALYS-003-22)
    pub fn get_resource_attribution(&self) -> Result<ProcessResourceAttribution, Box<dyn std::error::Error>> {
        self.system.refresh_all();
        
        if let Some(process) = self.system.process(sysinfo::Pid::from(self.process_id as usize)) {
            let memory_bytes = process.memory() * 1024;
            let virtual_memory_bytes = process.virtual_memory() * 1024;
            let cpu_percent = process.cpu_usage() as f64;
            
            // Calculate system-wide totals for relative attribution
            let system_total_memory = self.system.total_memory() * 1024;
            let system_used_memory = self.system.used_memory() * 1024;
            let system_cpu_count = self.system.cpus().len();
            
            // Calculate relative resource usage
            let memory_percentage = (memory_bytes as f64 / system_total_memory as f64) * 100.0;
            let relative_cpu_usage = cpu_percent / system_cpu_count as f64;
            
            let attribution = ProcessResourceAttribution {
                pid: self.process_id,
                memory_bytes,
                virtual_memory_bytes,
                memory_percentage,
                cpu_percent,
                relative_cpu_usage,
                system_memory_total: system_total_memory,
                system_memory_used: system_used_memory,
                system_cpu_count,
                timestamp: std::time::SystemTime::now(),
            };
            
            tracing::debug!(
                pid = self.process_id,
                memory_mb = memory_bytes / 1024 / 1024,
                memory_percentage = %format!("{:.2}%", memory_percentage),
                cpu_percent = %format!("{:.2}%", cpu_percent),
                relative_cpu_usage = %format!("{:.2}%", relative_cpu_usage),
                "Process resource attribution calculated"
            );
            
            Ok(attribution)
        } else {
            Err("Process not found for resource attribution".into())
        }
    }
    
    /// Monitor process health and resource limits (ALYS-003-22)
    pub fn monitor_process_health(&self) -> Result<ProcessHealthStatus, Box<dyn std::error::Error>> {
        let attribution = self.get_resource_attribution()?;
        let uptime = self.start_time.elapsed();
        
        // Define health thresholds
        let memory_warning_threshold = 80.0; // 80% of system memory
        let memory_critical_threshold = 90.0; // 90% of system memory
        let cpu_warning_threshold = 70.0; // 70% CPU usage
        let cpu_critical_threshold = 90.0; // 90% CPU usage
        
        // Determine health status
        let memory_status = if attribution.memory_percentage > memory_critical_threshold {
            ResourceStatus::Critical
        } else if attribution.memory_percentage > memory_warning_threshold {
            ResourceStatus::Warning
        } else {
            ResourceStatus::Healthy
        };
        
        let cpu_status = if attribution.cpu_percent > cpu_critical_threshold {
            ResourceStatus::Critical
        } else if attribution.cpu_percent > cpu_warning_threshold {
            ResourceStatus::Warning
        } else {
            ResourceStatus::Healthy
        };
        
        let overall_status = match (memory_status, cpu_status) {
            (ResourceStatus::Critical, _) | (_, ResourceStatus::Critical) => ProcessHealthStatus::Critical,
            (ResourceStatus::Warning, _) | (_, ResourceStatus::Warning) => ProcessHealthStatus::Warning,
            _ => ProcessHealthStatus::Healthy,
        };
        
        tracing::info!(
            pid = self.process_id,
            uptime_secs = uptime.as_secs(),
            memory_status = ?memory_status,
            cpu_status = ?cpu_status,
            overall_status = ?overall_status,
            memory_mb = attribution.memory_bytes / 1024 / 1024,
            cpu_percent = %format!("{:.2}", attribution.cpu_percent),
            "Process health monitoring completed"
        );
        
        Ok(overall_status)
    }
    
    /// Track process metrics over time for trend analysis (ALYS-003-22)
    pub async fn track_process_trends(&self) -> Result<(), Box<dyn std::error::Error>> {
        let attribution = self.get_resource_attribution()?;
        let health_status = self.monitor_process_health()?;
        
        // Log trend data for external analysis
        tracing::info!(
            event = "process_trend_data",
            pid = self.process_id,
            timestamp = attribution.timestamp.duration_since(std::time::UNIX_EPOCH)?.as_secs(),
            memory_bytes = attribution.memory_bytes,
            virtual_memory_bytes = attribution.virtual_memory_bytes,
            memory_percentage = attribution.memory_percentage,
            cpu_percent = attribution.cpu_percent,
            relative_cpu_usage = attribution.relative_cpu_usage,
            health_status = ?health_status,
            uptime_secs = self.start_time.elapsed().as_secs(),
            "Process trend data point recorded"
        );
        
        Ok(())
    }
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        Self {
            system: System::new_all(),
            process_id: self.process_id,
            start_time: self.start_time,
            collection_interval: self.collection_interval,
            actor_bridge: self.actor_bridge.clone(),
            previous_disk_stats: Arc::new(parking_lot::Mutex::new(None)),
            previous_network_stats: Arc::new(parking_lot::Mutex::new(None)),
            failure_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_successful_collection: Arc::new(parking_lot::RwLock::new(self.start_time)),
        }
    }
}

/// Initialize all metrics with proper error handling
pub fn initialize_metrics() -> Result<(), PrometheusError> {
    tracing::info!("Initializing comprehensive metrics system");
    
    // Test metric registration by accessing lazy statics
    let _test_metrics = [
        MIGRATION_PHASE.get(),
        SYNC_CURRENT_HEIGHT.get(),
        MEMORY_USAGE.get(),
        CPU_USAGE.get(),
    ];
    
    tracing::info!("Metrics initialization completed successfully");
    tracing::info!("Available metric categories: Migration, Actor, Sync, Performance, System Resource");
    
    Ok(())
}

/// Metric labeling strategy and cardinality limits
pub struct MetricLabels;

impl MetricLabels {
    /// Maximum number of unique label combinations per metric
    pub const MAX_CARDINALITY: usize = 10000;
    
    /// Standard migration phase labels
    pub const MIGRATION_PHASES: &'static [&'static str] = &[
        "foundation", "actor_system", "sync_engine", "federation_v2", 
        "lighthouse_v2", "migration", "validation", "rollback_safety",
        "performance_verification", "final_validation"
    ];
    
    /// Standard actor types
    pub const ACTOR_TYPES: &'static [&'static str] = &[
        "chain", "engine", "network", "bridge", "storage", "sync", "stream"
    ];
    
    /// Standard error types
    pub const ERROR_TYPES: &'static [&'static str] = &[
        "timeout", "connection", "validation", "parsing", "storage", 
        "network", "consensus", "execution", "migration", "system"
    ];
    
    /// Sanitize label values to prevent cardinality explosion
    pub fn sanitize_label_value(value: &str) -> String {
        // Limit length and remove problematic characters
        value
            .chars()
            .take(64)
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>()
            .to_lowercase()
    }
    
    /// Validate label cardinality doesn't exceed limits
    pub fn validate_cardinality(metric_name: &str, labels: &[&str]) -> bool {
        let estimated_cardinality = labels.iter().map(|l| l.len()).product::<usize>();
        
        if estimated_cardinality > Self::MAX_CARDINALITY {
            tracing::warn!(
                metric = metric_name,
                estimated_cardinality = estimated_cardinality,
                max_cardinality = Self::MAX_CARDINALITY,
                "Metric cardinality may exceed limits"
            );
            return false;
        }
        
        true
    }
}
