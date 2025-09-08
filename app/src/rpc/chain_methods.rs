//! Chain domain RPC methods
//! 
//! Handles blockchain query methods that interact with the ChainActor

use std::sync::Arc;
use hyper::{Body, Response, StatusCode};
use tracing::{error, debug};
use serde_json::json;
use lighthouse_facade::Hash256;

use crate::actors::chain::messages::*;
use crate::metrics::{RPC_REQUESTS, RPC_REQUEST_DURATION};
use super::{JsonRpcRequest, JsonRpcResponse, RpcError, UnifiedRpcContext};

/// Handle all chain-related RPC methods
pub async fn handle_chain_method(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let timer = RPC_REQUEST_DURATION
        .with_label_values(&[&req.method])
        .start_timer();

    let response = match req.method.as_str() {
        "getblockbyheight" => handle_get_block_by_height(req, context).await,
        "getblockbyhash" => handle_get_block_by_hash(req, context).await,
        "getblockcount" => handle_get_block_count(req, context).await,
        "getchainmetrics" => handle_get_chain_metrics(req, context).await,
        _ => {
            // Should not reach here due to routing in main handler
            error_response(req.id, RpcError::method_not_found())
        }
    };

    timer.observe_duration();
    response
}

/// Handle getblockbyheight RPC method
async fn handle_get_block_by_height(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getblockbyheight", "called"])
        .inc();

    let height = match extract_height_param(&req) {
        Ok(h) => h,
        Err(error) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyheight", "invalid_params"])
                .inc();
            return error_response(req.id, error);
        }
    };

    debug!("RPC getblockbyheight: height={}", height);

    let get_block_msg = GetBlockByHeight { height };
    
    match context.chain_actor.send(get_block_msg).await {
        Ok(Ok(Some(block))) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyheight", "success"])
                .inc();
            success_response(req.id, json!(block))
        }
        Ok(Ok(None)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyheight", "not_found"])
                .inc();
            error_response(req.id, RpcError::block_not_found())
        }
        Ok(Err(chain_error)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyheight", "chain_error"])
                .inc();
            error!("ChainActor error getting block by height {}: {:?}", height, chain_error);
            error_response(req.id, RpcError::from(chain_error))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyheight", "actor_unavailable"])
                .inc();
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("ChainActor"))
        }
    }
}

/// Handle getblockbyhash RPC method
async fn handle_get_block_by_hash(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getblockbyhash", "called"])
        .inc();

    let block_hash = match extract_hash_param(&req) {
        Ok(h) => h,
        Err(error) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyhash", "invalid_params"])
                .inc();
            return error_response(req.id, error);
        }
    };

    debug!("RPC getblockbyhash: hash={:?}", block_hash);

    let get_block_msg = GetBlockByHash { hash: block_hash };
    
    match context.chain_actor.send(get_block_msg).await {
        Ok(Ok(Some(block))) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyhash", "success"])
                .inc();
            success_response(req.id, json!(block))
        }
        Ok(Ok(None)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyhash", "not_found"])
                .inc();
            error_response(req.id, RpcError::block_not_found())
        }
        Ok(Err(chain_error)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyhash", "chain_error"])
                .inc();
            error!("ChainActor error getting block by hash: {:?}", chain_error);
            error_response(req.id, RpcError::from(chain_error))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getblockbyhash", "actor_unavailable"])
                .inc();
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("ChainActor"))
        }
    }
}

/// Handle getblockcount RPC method
async fn handle_get_block_count(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getblockcount", "called"])
        .inc();

    let get_count_msg = GetBlockCount;
    
    match context.chain_actor.send(get_count_msg).await {
        Ok(Ok(block_count)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockcount", "success"])
                .inc();
            success_response(req.id, json!(block_count))
        }
        Ok(Err(chain_error)) => {
            RPC_REQUESTS
                .with_label_values(&["getblockcount", "chain_error"])
                .inc();
            error!("ChainActor error getting block count: {:?}", chain_error);
            error_response(req.id, RpcError::from(chain_error))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getblockcount", "actor_unavailable"])
                .inc();
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("ChainActor"))
        }
    }
}

/// Handle getchainmetrics RPC method
async fn handle_get_chain_metrics(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getchainmetrics", "called"])
        .inc();

    let get_metrics_msg = GetChainMetrics;
    
    match context.chain_actor.send(get_metrics_msg).await {
        Ok(Ok(metrics)) => {
            RPC_REQUESTS
                .with_label_values(&["getchainmetrics", "success"])
                .inc();
            success_response(req.id, json!(metrics))
        }
        Ok(Err(chain_error)) => {
            RPC_REQUESTS
                .with_label_values(&["getchainmetrics", "chain_error"])
                .inc();
            error!("ChainActor error getting metrics: {:?}", chain_error);
            error_response(req.id, RpcError::from(chain_error))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getchainmetrics", "actor_unavailable"])
                .inc();
            error!("Failed to send message to ChainActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("ChainActor"))
        }
    }
}

/// Extract height parameter from request
fn extract_height_param(req: &JsonRpcRequest) -> Result<u64, RpcError> {
    let params = req.params.as_ref().ok_or_else(|| RpcError::invalid_params())?;
    
    // Handle both string and number parameters
    if let Some(height_num) = params.as_u64() {
        Ok(height_num)
    } else if let Some(height_str) = params.as_str() {
        height_str.parse::<u64>()
            .map_err(|_| RpcError::invalid_params())
    } else {
        Err(RpcError::invalid_params())
    }
}

/// Extract hash parameter from request
fn extract_hash_param(req: &JsonRpcRequest) -> Result<Hash256, RpcError> {
    let params = req.params.as_ref().ok_or_else(|| RpcError::invalid_params())?;
    
    let hash_str = params.as_str().ok_or_else(|| RpcError::invalid_params())?;
    
    let hash_bytes = hex::decode(hash_str)
        .map_err(|_| RpcError::invalid_params())?;
    
    if hash_bytes.len() != 32 {
        return Err(RpcError::invalid_params());
    }
    
    Ok(Hash256::from_slice(&hash_bytes))
}

/// Create a success response
fn success_response(id: serde_json::Value, result: serde_json::Value) 
    -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> 
{
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(
            JsonRpcResponse {
                result: Some(result),
                error: None,
                id,
            }
            .into(),
        )?)
}

/// Create an error response
fn error_response(id: serde_json::Value, error: RpcError) 
    -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>>
{
    let status = match error.code {
        -32600 => StatusCode::BAD_REQUEST,  // Invalid Request
        -32601 => StatusCode::NOT_FOUND,    // Method not found  
        -32602 => StatusCode::BAD_REQUEST,  // Invalid params
        -32604 => StatusCode::NOT_FOUND,    // Block not found
        -32606 => StatusCode::SERVICE_UNAVAILABLE, // Service unavailable
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    Ok(Response::builder()
        .status(status)
        .body(
            JsonRpcResponse {
                result: None,
                error: Some(error),
                id,
            }
            .into(),
        )?)
}