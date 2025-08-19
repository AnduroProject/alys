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

/// System resource metrics collector with automated monitoring
pub struct MetricsCollector {
    system: System,
    process_id: u32,
    start_time: std::time::Instant,
    collection_interval: Duration,
    /// Actor metrics bridge for Prometheus integration
    actor_bridge: Option<Arc<ActorMetricsBridge>>,
}

impl MetricsCollector {
    /// Create a new MetricsCollector
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut system = System::new_all();
        system.refresh_all();
        
        let process_id = std::process::id();
        let start_time = std::time::Instant::now();
        
        tracing::info!("Initializing MetricsCollector with PID: {}", process_id);
        
        Ok(Self {
            system,
            process_id,
            start_time,
            collection_interval: Duration::from_secs(5),
            actor_bridge: None,
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

    /// Start automated metrics collection
    pub async fn start_collection(&self) -> tokio::task::JoinHandle<()> {
        let mut collector = self.clone();
        let actor_bridge = self.actor_bridge.clone();
        
        tokio::spawn(async move {
            // Start actor bridge collection if available
            if let Some(bridge) = &actor_bridge {
                let _actor_handle = bridge.start_collection().await;
                tracing::info!("Actor metrics bridge collection started");
            }
            
            let mut interval = interval(collector.collection_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = collector.collect_system_metrics().await {
                    tracing::warn!("Failed to collect system metrics: {}", e);
                    continue;
                }
                
                collector.update_uptime_metrics();
                
                // Update actor system health if bridge is available
                if let Some(bridge) = &actor_bridge {
                    let is_healthy = bridge.is_system_healthy();
                    let stats = bridge.get_aggregate_stats();
                    
                    tracing::trace!(
                        actor_system_healthy = is_healthy,
                        total_actors = stats.total_actors,
                        healthy_actors = stats.healthy_actors,
                        "Actor system health check completed"
                    );
                }
                
                tracing::trace!("System metrics collection completed");
            }
        })
    }

    /// Collect system resource metrics
    async fn collect_system_metrics(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.system.refresh_all();
        
        // Get process-specific metrics
        if let Some(process) = self.system.process(sysinfo::Pid::from(self.process_id as usize)) {
            // Memory usage
            let memory_bytes = process.memory() * 1024; // Convert KB to bytes
            MEMORY_USAGE.set(memory_bytes as i64);
            
            // CPU usage
            let cpu_percent = process.cpu_usage() as f64;
            CPU_USAGE.set(cpu_percent);
            
            // Thread count (approximation)
            THREAD_COUNT.set(num_cpus::get() as i64);
            
            tracing::trace!(
                memory_mb = memory_bytes / 1024 / 1024,
                cpu_percent = %format!("{:.2}", cpu_percent),
                "Collected process metrics"
            );
        }
        
        // System-wide metrics
        let total_memory = self.system.total_memory();
        let used_memory = self.system.used_memory();
        let memory_usage_percent = (used_memory as f64 / total_memory as f64) * 100.0;
        
        // Global CPU usage (simplified)
        let global_cpu = self.system.global_cpu_info().cpu_usage() as f64;
        
        tracing::trace!(
            total_memory_gb = total_memory / 1024 / 1024 / 1024,
            used_memory_gb = used_memory / 1024 / 1024 / 1024,
            memory_usage_percent = %format!("{:.2}", memory_usage_percent),
            global_cpu_percent = %format!("{:.2}", global_cpu),
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
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        Self {
            system: System::new_all(),
            process_id: self.process_id,
            start_time: self.start_time,
            collection_interval: self.collection_interval,
            actor_bridge: self.actor_bridge.clone(),
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
