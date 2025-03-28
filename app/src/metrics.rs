use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use std::convert::Infallible;
use std::net::SocketAddr;

use lazy_static::lazy_static;
use prometheus::{
    register_gauge, register_gauge_vec, register_histogram, register_histogram_vec,
    register_int_counter, register_int_counter_vec, register_int_gauge, Encoder, Gauge, GaugeVec,
    Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, TextEncoder,
};

lazy_static! {
    /// This counter tracks the total number of times we've produced an Aura block.
    pub static ref AURA_PRODUCED_BLOCKS: IntCounter = register_int_counter!(
        "aura_produced_blocks_total",
        "Total number of blocks produced by the node via the Aura consensus"
    ).unwrap();

    /// This gauge tracks the current slot for Aura.
    pub static ref AURA_CURRENT_SLOT: Gauge = register_gauge!(
        "aura_current_slot",
        "Tracks the current slot number processed by the node"
    ).unwrap();

    /// Counter for the number of times `claim_slot` is called.
    pub static ref AURA_CLAIM_SLOT_CALLS: IntCounter = register_int_counter!(
        "aura_claim_slot_calls_total",
        "Total number of times the claim_slot method is called"
    ).unwrap();

    /// Gauge for the number of successful author retrievals, labeled by authority index.
    pub static ref AURA_SUCCESSFUL_SLOT_AUTHOR_RETRIEVALS: GaugeVec = register_gauge_vec!(
        "aura_successful_author_retrievals",
        "Number of successful author retrievals",
        &["authority_index"]
    ).unwrap();

    /// Gauge for the number of successful slot claims.
    pub static ref AURA_SUCCESSFUL_SLOT_CLAIMS: Gauge = register_gauge!(
        "aura_successful_slot_claims",
        "Number of successful slot claims"
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
        &["status", "chain_id", "block_height"]
    ).unwrap();

    /// Histogram for the number of hashes processed during aux block creation.
    pub static ref AUXPOW_HASHES_PROCESSED: Histogram = register_histogram!(
        "auxpow_hashes_processed",
        "Histogram of the number of hashes processed during aux block creation"
    ).unwrap();

    pub static ref CHAIN_PEGINS_PROCESSED: IntCounter = register_int_counter!(
        "chain_pegins_processed_total",
        "Total number of peg-ins processed"
    ).unwrap();

    pub static ref CHAIN_PEGINS_REMOVED: IntCounter = register_int_counter!(
        "chain_pegins_removed_total",
        "Total number of peg-ins removed because they were already processed"
    ).unwrap();

    pub static ref CHAIN_PEGINS_ADDED: IntCounter = register_int_counter!(
        "chain_pegins_added_total",
        "Total number of peg-ins added to the processed list"
    ).unwrap();

    pub static ref CHAIN_TOTAL_PEGIN_AMOUNT: IntGauge = register_int_gauge!(
        "chain_total_pegin_amount",
        "Total amount of peg-ins processed"
    ).unwrap();

    pub static ref CHAIN_PROCESS_BLOCK_ATTEMPTS: IntCounter = register_int_counter!(
        "chain_process_block_attempts_total",
        "Number of times process_block is invoked"
    ).unwrap();

    pub static ref CHAIN_BLOCKS_REJECTED: IntCounterVec = register_int_counter_vec!(
        "chain_blocks_rejected_total",
        "Number of blocks rejected, labeled by reason",
        &["reason"]
    ).unwrap();

    pub static ref ENGINE_BUILD_BLOCK_CALLS: IntCounterVec = register_int_counter_vec!(
        "engine_build_block_calls_total",
        "Number of times build_block is invoked",
        &["status"]
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

    tracing::info!("Starting Metrics server on {}", addr);

    if let Err(e) = server.await {
        tracing::error!("Metrics server error: {}", e);
    }
}
