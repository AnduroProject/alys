use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::convert::Infallible;
use std::net::SocketAddr;

use lazy_static::lazy_static;
use prometheus::{
    register_gauge_with_registry, register_histogram_vec_with_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry, Encoder, Gauge,
    Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, Registry, TextEncoder,
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
            &["status"],
            ALYS_REGISTRY
        )
        .unwrap();
    pub static ref CHAIN_BTC_BLOCK_MONITOR_START_HEIGHT: IntGauge =
        register_int_gauge_with_registry!(
            "chain_btc_block_monitor_start_height",
            "The start height of the BTC block monitor",
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
        _ => {
            let mut not_found = Response::new(Body::from("Not Found"));
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

pub async fn start_server() {
    let addr = SocketAddr::from(([0, 0, 0, 0], 9001));

    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });

    let server = Server::bind(&addr).serve(make_svc);

    // TODO: handle graceful shutdown
    tokio::spawn(async move {
        tracing::info!("Starting Metrics server on {}", addr);

        if let Err(e) = server.await {
            tracing::error!("Metrics server error: {}", e);
        }
    });
}
