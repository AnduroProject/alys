use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::convert::Infallible;
use std::net::SocketAddr;

use lazy_static::lazy_static;
use prometheus::{
    register_gauge, register_histogram, register_histogram_vec, register_int_counter,
    register_int_counter_vec, register_int_gauge, Encoder, Gauge, Histogram, HistogramVec,
    IntCounter, IntCounterVec, IntGauge, TextEncoder,
};

lazy_static! {
    /// This counter tracks the total number of times we've produced an Aura block.
    pub static ref AURA_PRODUCED_BLOCKS: IntCounterVec = register_int_counter_vec!(
        "aura_produced_blocks_total",
        "Total number of blocks produced by the node via the Aura consensus",
        &["status"]
    ).unwrap();

    /// This gauge tracks the current slot for Aura.
    pub static ref AURA_CURRENT_SLOT: Gauge = register_gauge!(
        "aura_current_slot",
        "Tracks the current slot number processed by the node"
    ).unwrap();

    /// Gauge for the number of successful author retrievals, labeled by authority index.
    pub static ref AURA_SLOT_AUTHOR_RETRIEVALS: IntCounterVec = register_int_counter_vec!(
        "aura_slot_author_retrievals",
        "Number of slot author retrievals",
        &["status", "authority_index"]
    ).unwrap();

    pub static ref AURA_LATEST_SLOT_AUTHOR: Gauge = register_gauge!(
        "aura_latest_slot_author",
        "The index of the latest slot author"
    ).unwrap();

    pub static ref AURA_VERIFY_SIGNED_BLOCK: IntCounterVec = register_int_counter_vec!(
        "aura_verify_signed_block_total",
        "Number of times the proposed block is verified",
        &["status"]
    ).unwrap();

    /// Gauge for the number of successful slot claims.
    pub static ref AURA_SLOT_CLAIM_TOTALS: IntCounterVec = register_int_counter_vec!(
        "aura_slot_claim_totals",
        "Number of slot claims",
        &["status"]
    ).unwrap();

    /// Counter for the number of times `create_aux_block` is called.
    pub static ref AUXPOW_CREATE_BLOCK_CALLS: IntCounterVec = register_int_counter_vec!(
        "create_aux_block_calls_total",
        "Total number of times the create_aux_block method is called",
        &["status"]
    ).unwrap();

    /// Counter for the number of times `submit_aux_block` is called, labeled by status, chain ID, and block height.
    pub static ref AUXPOW_SUBMIT_BLOCK_CALLS: IntCounterVec = register_int_counter_vec!(
        "auxpow_submit_block_calls_total",
        "Total number of times the submit_aux_block method is called",
        &["status"]
    ).unwrap();

    /// Histogram for the number of hashes processed during aux block creation.
    pub static ref AUXPOW_HASHES_PROCESSED: Histogram = register_histogram!(
        "auxpow_hashes_processed",
        "Histogram of the number of hashes processed during aux block creation"
    ).unwrap();

    pub static ref CHAIN_PEGIN_TOTALS: IntCounterVec = register_int_counter_vec!(
        "chain_pegin_totals",
        "Total number of peg-in operations labeled by operation type (add, remove, process)",
        &["status"]
    ).unwrap();

    pub static ref CHAIN_TOTAL_PEGIN_AMOUNT: IntGauge = register_int_gauge!(
        "chain_total_pegin_amount",
        "Total amount of peg-ins processed"
    ).unwrap();

    pub static ref CHAIN_PROCESS_BLOCK_ATTEMPTS: IntCounter = register_int_counter!(
        "chain_process_block_attempts_total",
        "Number of times process_block is invoked"
    ).unwrap();

    pub static ref CHAIN_PROCESS_BLOCK_TOTALS: IntCounterVec = register_int_counter_vec!(
        "chain_process_block_totals",
        "Total number of blocks processed, labeled by status",
        &["status", "reason"]
    ).unwrap();

    pub static ref CHAIN_NETWORK_GOSSIP_TOTALS: IntCounterVec = register_int_counter_vec!(
        "chain_network_gossip_totals",
        "Total number of network gossip messages, labeled by message type",
        &["message_type", "status"]
    ).unwrap();

    pub static ref CHAIN_BTC_BLOCK_MONITOR_TOTALS: IntCounterVec = register_int_counter_vec!(
        "chain_btc_block_monitor_totals",
        "Total number of BTC block monitor messages, labeled by message type",
        &["block_height", "status"]
    ).unwrap();

    pub static ref CHAIN_DISCOVERED_PEERS: Gauge = register_gauge!(
        "chain_discovered_peers",
        "Number of discovered peers"
    ).unwrap();

    pub static ref CHAIN_SYNCING_OPERATION_TOTALS: IntCounterVec = register_int_counter_vec!(
        "chain_syncing_operation_totals",
        "Total number of syncing operations, labeled by operation type (add, remove)",
        &["start_height", "status"]
    ).unwrap();

    pub static ref CHAIN_BLOCK_PRODUCTION_TOTALS: IntCounterVec = register_int_counter_vec!(
        "chain_block_production_totals",
        "Total number of blocks produced, labeled by status",
        &["message_type", "status"]
    ).unwrap();

    pub static ref ENGINE_BUILD_BLOCK_CALLS: IntCounterVec = register_int_counter_vec!(
        "engine_build_block_calls_total",
        "Number of times build_block is invoked",
        &["status", "reason"]
    ).unwrap();

    pub static ref ENGINE_COMMIT_BLOCK_CALLS: IntCounterVec = register_int_counter_vec!(
        "engine_commit_block_calls_total",
        "Number of times commit_block is invoked",
        &["status"]
    ).unwrap();

    /// Counter for the number of JSON-RPC requests, labeled by method and status.
    pub static ref RPC_REQUESTS: IntCounterVec = register_int_counter_vec!(
        "rpc_requests_total",
        "Total number of JSON-RPC requests",
        &["method", "status"]
    ).unwrap();

    /// Histogram for the time taken to process JSON-RPC requests, labeled by method.
    pub static ref RPC_REQUEST_DURATION: HistogramVec = register_histogram_vec!(
        "rpc_request_duration_seconds",
        "Histogram of the time taken to process JSON-RPC requests",
        &["method"]
    ).unwrap();

}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            // Gather the metrics from the Prometheus registry
            let metric_families = prometheus::gather();
            // Encode them to text format
            let encoder = TextEncoder::new();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer).unwrap();
            Ok(Response::new(Body::from(buffer)))
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
