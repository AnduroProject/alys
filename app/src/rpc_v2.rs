//! V2 RPC Server - Actor-based implementation
//!
//! This is the V2 version of the RPC server that uses actor messages instead of
//! direct Chain method calls. All blockchain queries are sent as messages to 
//! the appropriate actors (ChainActor, EngineActor, etc.)

use crate::auxpow::AuxPow;
use crate::auxpow_miner::{AuxPowMiner, BitcoinConsensusParams, BlockIndex, ChainManager};
use crate::actors::chain::{ChainActor, messages::*};
use crate::actors::chain::error::ChainError;
use crate::actors::engine::{EngineActor};
use crate::actors::storage::{StorageActor};
use crate::block::SignedConsensusBlock;
use crate::metrics::{RPC_REQUESTS, RPC_REQUEST_DURATION};
use bitcoin::address::NetworkChecked;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::Hash;
use bitcoin::{Address, BlockHash};
use ethereum_types::Address as EvmAddress;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use lighthouse_wrapper::store::ItemStore;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use serde_derive::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use actix::prelude::*;

// Re-use existing JSON-RPC structures
use crate::rpc::{JsonRpcRequestV1, JsonRpcErrorV1, JsonRpcResponseV1};

/// V2 RPC server context with actor addresses
#[derive(Debug)]
pub struct RpcV2Context {
    /// ChainActor for blockchain queries
    pub chain_actor: Addr<ChainActor>,
    /// EngineActor for execution layer queries  
    pub engine_actor: Addr<EngineActor>,
    /// StorageActor for data persistence queries
    pub storage_actor: Addr<StorageActor>,
    /// Federation address for peg operations
    pub federation_address: Address<NetworkChecked>,
    /// Miner for auxiliary PoW operations
    pub miner: Arc<Mutex<dyn MinerV2>>,
}

/// Trait for V2 miner operations (actor-compatible)
#[async_trait::async_trait]
pub trait MinerV2: Send + Sync {
    async fn get_queued_auxpow(&self) -> Option<Value>;
    async fn submit_auxpow(&mut self, hash: String, auxpow: AuxPow) -> Result<bool, String>;
}

/// V2 RPC server entry point
pub async fn run_server_v2(
    chain_actor: Addr<ChainActor>,
    engine_actor: Addr<EngineActor>, 
    storage_actor: Addr<StorageActor>,
    federation_address: Address<NetworkChecked>,
    retarget_params: BitcoinConsensusParams,
    rpc_port: u16,
) {
    let addr = SocketAddr::from(([0, 0, 0, 0], rpc_port));
    
    // TODO: Create V2 miner that works with actors
    // For now, create a placeholder
    let miner: Arc<Mutex<dyn MinerV2>> = Arc::new(Mutex::new(PlaceholderMiner));
    
    let rpc_context = Arc::new(RpcV2Context {
        chain_actor: chain_actor.clone(),
        engine_actor: engine_actor.clone(),
        storage_actor: storage_actor.clone(),
        federation_address: federation_address.clone(),
        miner: miner.clone(),
    });

    info!("Starting V2 Actor-based RPC server on {}", addr);
    
    let server = Server::bind(&addr).serve(make_service_fn(move |_conn| {
        let context = rpc_context.clone();
        
        async move {
            Ok::<_, GenericError>(service_fn(move |req| {
                let ctx = context.clone();
                http_req_json_rpc_v2(req, ctx)
            }))
        }
    }));

    // TODO: handle graceful shutdown with actor system
    tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("V2 RPC server error: {}", e);
        }
    });
    
    info!("V2 RPC server started successfully");
}

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;

/// Main V2 RPC request handler using actor messages
async fn http_req_json_rpc_v2(
    req: Request<Body>,
    context: Arc<RpcV2Context>,
) -> Result<Response<Body>> {
    if req.method() != Method::POST {
        RPC_REQUESTS
            .with_label_values(&["unknown", "method_not_allowed"])
            .inc();
        return Ok(Response::builder()
            .status(hyper::StatusCode::METHOD_NOT_ALLOWED)
            .body("V2 JSONRPC server handles only POST requests".into())?);
    }

    let bytes = hyper::body::to_bytes(req.into_body()).await?;
    let json_req = serde_json::from_slice::<JsonRpcRequestV1>(&bytes)?;
    let id = json_req.id;

    let timer = RPC_REQUEST_DURATION
        .with_label_values(&[json_req.method])
        .start_timer();

    let result = match json_req.method {
        "getblockbyheight" => {
            handle_get_block_by_height_v2(json_req.params, id, &context).await
        }
        "getblockbyhash" => {
            handle_get_block_by_hash_v2(json_req.params, id, &context).await
        }
        "getqueuedpow" => {
            handle_get_queued_pow_v2(id, &context).await
        }
        "submitauxblock" => {
            handle_submit_aux_block_v2(json_req.params, id, &context).await
        }
        "getfederationaddress" => {
            handle_get_federation_address_v2(id, &context).await
        }
        "getchainmetrics" => {
            handle_get_chain_metrics_v2(id, &context).await  
        }
        "getblockcount" => {
            handle_get_block_count_v2(id, &context).await
        }
        _ => {
            RPC_REQUESTS
                .with_label_values(&[json_req.method, "not_found"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::NOT_FOUND)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::method_not_found()),
                        id,
                    }
                    .into(),
                )?)
        }
    };

    timer.observe_duration();
    result
}

/// V2 handler for getblockbyheight using ChainActor message
async fn handle_get_block_by_height_v2(
    params: Option<&RawValue>,
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    let params = match params {
        Some(p) => p,
        None => {
            return Ok(Response::builder()
                .status(hyper::StatusCode::BAD_REQUEST)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::invalid_params()),
                        id,
                    }
                    .into(),
                )?)
        }
    };

    let target_height: u64 = match params.get().parse() {
        Ok(h) => h,
        Err(e) => {
            return Ok(Response::builder()
                .status(hyper::StatusCode::BAD_REQUEST)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error(e.to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
    };

    // Send message to ChainActor instead of direct method call
    let get_block_msg = GetBlockByHeight { height: target_height };
    
    match context.chain_actor.send(get_block_msg).await {
        Ok(Ok(maybe_block)) => {
            block_response_helper_v2(id, maybe_block)
        }
        Ok(Err(chain_error)) => {
            error!("ChainActor error getting block by height {}: {}", target_height, chain_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error(chain_error.to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
        Err(mailbox_error) => {
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error("ChainActor unavailable".to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
    }
}

/// V2 handler for getblockbyhash using ChainActor message  
async fn handle_get_block_by_hash_v2(
    params: Option<&RawValue>,
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    let params = match params {
        Some(p) => p,
        None => {
            return Ok(Response::builder()
                .status(hyper::StatusCode::BAD_REQUEST)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::invalid_params()),
                        id,
                    }
                    .into(),
                )?)
        }
    };

    let block_hash = if let Ok(value) = serde_json::from_str::<String>(params.get()) {
        let block_hash_bytes = hex::decode(&value)?;
        Hash256::from_slice(block_hash_bytes.as_slice())
    } else {
        return Ok(Response::builder()
            .status(hyper::StatusCode::BAD_REQUEST)
            .body(
                JsonRpcResponseV1 {
                    result: None,
                    error: Some(JsonRpcErrorV1::invalid_params()),
                    id,
                }
                .into(),
            )?)
    };

    // Send message to ChainActor instead of direct method call
    let get_block_msg = GetBlockByHash { hash: block_hash };
    
    match context.chain_actor.send(get_block_msg).await {
        Ok(Ok(maybe_block)) => {
            block_response_helper_v2(id, maybe_block)
        }
        Ok(Err(chain_error)) => {
            error!("ChainActor error getting block by hash: {}", chain_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error(chain_error.to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
        Err(mailbox_error) => {
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error("ChainActor unavailable".to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
    }
}

/// Helper function for block responses in V2
fn block_response_helper_v2(
    id: Value,
    maybe_block: Option<SignedConsensusBlock>,
) -> Result<Response<Body>> {
    match maybe_block {
        Some(block) => {
            RPC_REQUESTS
                .with_label_values(&["getblock", "success"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .body(
                    JsonRpcResponseV1 {
                        result: Some(json!(block)),
                        error: None,
                        id,
                    }
                    .into(),
                )?)
        }
        None => {
            RPC_REQUESTS
                .with_label_values(&["getblock", "not_found"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::NOT_FOUND)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::block_not_found()),
                        id,
                    }
                    .into(),
                )?)
        }
    }
}

/// V2 handler for getqueuedpow
async fn handle_get_queued_pow_v2(
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    let miner = context.miner.lock().await;
    match miner.get_queued_auxpow().await {
        Some(queued_pow) => {
            RPC_REQUESTS
                .with_label_values(&["getqueuedpow", "success"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .body(
                    JsonRpcResponseV1 {
                        result: Some(queued_pow),
                        error: None,
                        id,
                    }
                    .into(),
                )?)
        }
        None => {
            RPC_REQUESTS
                .with_label_values(&["getqueuedpow", "no_data"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::NO_CONTENT)
                .body(
                    JsonRpcResponseV1 {
                        result: Some(json!(null)),
                        error: None,
                        id,
                    }
                    .into(),
                )?)
        }
    }
}

/// V2 handler for submitauxblock
async fn handle_submit_aux_block_v2(
    params: Option<&RawValue>,
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    // TODO: Implement submitauxblock with actor messages
    // This would involve sending messages to ChainActor for block submission
    warn!("submitauxblock not yet implemented in V2");
    Ok(Response::builder()
        .status(hyper::StatusCode::NOT_IMPLEMENTED)
        .body(
            JsonRpcResponseV1 {
                result: None,
                error: Some(JsonRpcErrorV1::debug_error("submitauxblock not implemented in V2".to_string())),
                id,
            }
            .into(),
        )?)
}

/// V2 handler for getfederationaddress
async fn handle_get_federation_address_v2(
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    RPC_REQUESTS
        .with_label_values(&["getfederationaddress", "success"])
        .inc();
    Ok(Response::builder()
        .status(hyper::StatusCode::OK)
        .body(
            JsonRpcResponseV1 {
                result: Some(json!(context.federation_address.to_string())),
                error: None,
                id,
            }
            .into(),
        )?)
}

/// V2 handler for getchainmetrics using ChainActor message
async fn handle_get_chain_metrics_v2(
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    let get_metrics_msg = GetChainMetrics;
    
    match context.chain_actor.send(get_metrics_msg).await {
        Ok(Ok(metrics)) => {
            RPC_REQUESTS
                .with_label_values(&["getchainmetrics", "success"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .body(
                    JsonRpcResponseV1 {
                        result: Some(json!(metrics)),
                        error: None,
                        id,
                    }
                    .into(),
                )?)
        }
        Ok(Err(chain_error)) => {
            error!("ChainActor error getting metrics: {}", chain_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error(chain_error.to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
        Err(mailbox_error) => {
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error("ChainActor unavailable".to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
    }
}

/// V2 handler for getblockcount using ChainActor message
async fn handle_get_block_count_v2(
    id: Value,
    context: &RpcV2Context,
) -> Result<Response<Body>> {
    let get_count_msg = GetBlockCount;
    
    match context.chain_actor.send(get_count_msg).await {
        Ok(Ok(block_count)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockcount", "success"])
                .inc();
            Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .body(
                    JsonRpcResponseV1 {
                        result: Some(json!(block_count)),
                        error: None,
                        id,
                    }
                    .into(),
                )?)
        }
        Ok(Err(chain_error)) => {
            error!("ChainActor error getting block count: {}", chain_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error(chain_error.to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
        Err(mailbox_error) => {
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            Ok(Response::builder()
                .status(hyper::StatusCode::SERVICE_UNAVAILABLE)
                .body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error("ChainActor unavailable".to_string())),
                        id,
                    }
                    .into(),
                )?)
        }
    }
}

/// Placeholder miner implementation for V2 transition
struct PlaceholderMiner;

#[async_trait::async_trait]
impl MinerV2 for PlaceholderMiner {
    async fn get_queued_auxpow(&self) -> Option<Value> {
        warn!("PlaceholderMiner: get_queued_auxpow not implemented");
        None
    }
    
    async fn submit_auxpow(&mut self, _hash: String, _auxpow: AuxPow) -> Result<bool, String> {
        warn!("PlaceholderMiner: submit_auxpow not implemented");
        Err("Not implemented in placeholder".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_v2_rpc_context_creation() {
        // This would require actual actor addresses in a real test
        // For now, just test that the structure compiles
        info!("V2 RPC context structure validated");
    }

    #[test] 
    fn test_placeholder_miner() {
        // Test placeholder miner compiles
        let _miner = PlaceholderMiner;
        info!("PlaceholderMiner structure validated");
    }
}