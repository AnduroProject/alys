//! Bridge domain RPC methods
//! 
//! Handles federation and bridge-related methods

use std::sync::Arc;
use hyper::{Body, Response, StatusCode};
use tracing::debug;
use serde_json::json;

use crate::metrics::{RPC_REQUESTS, RPC_REQUEST_DURATION};
use super::{JsonRpcRequest, JsonRpcResponse, RpcError, UnifiedRpcContext};

/// Handle all bridge-related RPC methods
pub async fn handle_bridge_method(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let timer = RPC_REQUEST_DURATION
        .with_label_values(&[&req.method])
        .start_timer();

    let response = match req.method.as_str() {
        "getfederationaddress" => handle_get_federation_address(req, context).await,
        "getdepositaddress" => handle_get_deposit_address(req, context).await,
        _ => {
            // Should not reach here due to routing in main handler
            error_response(req.id, RpcError::method_not_found())
        }
    };

    timer.observe_duration();
    response
}

/// Handle getfederationaddress RPC method
async fn handle_get_federation_address(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getfederationaddress", "called"])
        .inc();

    debug!("RPC getfederationaddress called");

    RPC_REQUESTS
        .with_label_values(&["getfederationaddress", "success"])
        .inc();

    success_response(req.id, json!(context.federation_address.to_string()))
}

/// Handle getdepositaddress RPC method (alias for getfederationaddress)
async fn handle_get_deposit_address(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getdepositaddress", "called"])
        .inc();

    debug!("RPC getdepositaddress called (alias for getfederationaddress)");

    RPC_REQUESTS
        .with_label_values(&["getdepositaddress", "success"])
        .inc();

    success_response(req.id, json!(context.federation_address.to_string()))
}

// Response helpers

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