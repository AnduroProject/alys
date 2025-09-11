//! Mining domain RPC methods
//! 
//! Handles auxiliary proof-of-work methods that interact with the AuxPowActor

use std::sync::Arc;
use std::str::FromStr;
use hyper::{Body, Response, StatusCode};
use tracing::{info, warn, error, debug};
use serde_json::json;
use ethereum_types::Address as EvmAddress;
use bitcoin::consensus::Decodable;

use crate::actors::auxpow::messages::*;
use crate::metrics::{RPC_REQUESTS, RPC_REQUEST_DURATION};
use super::{JsonRpcRequest, JsonRpcResponse, RpcError, UnifiedRpcContext};

/// Handle all mining-related RPC methods
pub async fn handle_mining_method(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let timer = RPC_REQUEST_DURATION
        .with_label_values(&[&req.method])
        .start_timer();

    let response = match req.method.as_str() {
        "createauxblock" => handle_create_aux_block(req, context).await,
        "submitauxblock" => handle_submit_aux_block(req, context).await,
        "getauxblock" => handle_get_aux_block(req, context).await,
        "getmininginfo" => handle_get_mining_info(req, context).await,
        "setgenerate" => handle_set_generate(req, context).await,
        "getqueuedpow" => handle_get_queued_pow(req, context).await,
        _ => {
            // Should not reach here due to routing in main handler
            error_response(req.id, RpcError::method_not_found())
        }
    };

    timer.observe_duration();
    response
}

/// Handle createauxblock RPC method
async fn handle_create_aux_block(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["createauxblock", "called"])
        .inc();

    let address = match extract_address_param(&req) {
        Ok(addr) => addr,
        Err(error) => {
            RPC_REQUESTS
                .with_label_values(&["createauxblock", "invalid_params"])
                .inc();
            return error_response(req.id, error);
        }
    };

    debug!("RPC createauxblock: address={:?}", address);

    let create_msg = CreateAuxBlock { address };
    
    match context.auxpow_actor.send(create_msg).await {
        Ok(Ok(aux_block)) => {
            RPC_REQUESTS
                .with_label_values(&["createauxblock", "success"])
                .inc();
            info!(
                block_hash = %aux_block.hash,
                chain_id = aux_block.chain_id,
                height = aux_block.height,
                "Created aux block for mining"
            );
            success_response(req.id, json!(aux_block))
        }
        Ok(Err(auxpow_error)) => {
            let status = match auxpow_error {
                _ if matches!(auxpow_error, crate::actors::auxpow::error::AuxPowError::ChainSyncing) => "chain_syncing",
                _ => "auxpow_error"
            };
            RPC_REQUESTS
                .with_label_values(&["createauxblock", status])
                .inc();
            error!("AuxPowActor error creating aux block: {:?}", auxpow_error);
            error_response(req.id, RpcError::from(auxpow_error))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["createauxblock", "actor_unavailable"])
                .inc();
            error!("Failed to send message to AuxPowActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("AuxPowActor"))
        }
    }
}

/// Handle submitauxblock RPC method
async fn handle_submit_aux_block(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["submitauxblock", "called"])
        .inc();

    let (hash, auxpow) = match extract_submit_params(&req) {
        Ok(params) => params,
        Err(error) => {
            RPC_REQUESTS
                .with_label_values(&["submitauxblock", "invalid_params"])
                .inc();
            return error_response(req.id, error);
        }
    };

    debug!("RPC submitauxblock: hash={:?}", hash);

    let submit_msg = SubmitAuxBlock { hash, auxpow };
    
    match context.auxpow_actor.send(submit_msg).await {
        Ok(Ok(_)) => {
            RPC_REQUESTS
                .with_label_values(&["submitauxblock", "success"])
                .inc();
            info!(block_hash = %hash, "AuxPow submission accepted");
            success_response(req.id, json!(true))
        }
        Ok(Err(auxpow_error)) => {
            RPC_REQUESTS
                .with_label_values(&["submitauxblock", "rejected"])
                .inc();
            warn!(block_hash = %hash, error = ?auxpow_error, "AuxPow submission rejected");
            // Bitcoin RPC returns false on failure, not error
            success_response(req.id, json!(false))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["submitauxblock", "actor_unavailable"])
                .inc();
            error!("Failed to send message to AuxPowActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("AuxPowActor"))
        }
    }
}

/// Handle getauxblock RPC method
async fn handle_get_aux_block(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getauxblock", "called"])
        .inc();

    // Use zero address as default for template requests
    let create_msg = CreateAuxBlock { address: EvmAddress::zero() };
    
    match context.auxpow_actor.send(create_msg).await {
        Ok(Ok(aux_block)) => {
            RPC_REQUESTS
                .with_label_values(&["getauxblock", "success"])
                .inc();
            debug!("Generated aux block template");
            success_response(req.id, json!(aux_block))
        }
        Ok(Err(auxpow_error)) => {
            match auxpow_error {
                crate::actors::auxpow::error::AuxPowError::ChainSyncing => {
                    RPC_REQUESTS
                        .with_label_values(&["getauxblock", "chain_syncing"])
                        .inc();
                    debug!("No aux block available - chain syncing");
                    success_response(req.id, json!(null))
                }
                _ => {
                    RPC_REQUESTS
                        .with_label_values(&["getauxblock", "auxpow_error"])
                        .inc();
                    error_response(req.id, RpcError::from(auxpow_error))
                }
            }
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getauxblock", "actor_unavailable"])
                .inc();
            error!("Failed to send message to AuxPowActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("AuxPowActor"))
        }
    }
}

/// Handle getmininginfo RPC method
async fn handle_get_mining_info(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getmininginfo", "called"])
        .inc();

    let get_status_msg = GetMiningStatus;
    
    match context.auxpow_actor.send(get_status_msg).await {
        Ok(status) => {
            RPC_REQUESTS
                .with_label_values(&["getmininginfo", "success"])
                .inc();
            
            let mining_info = json!({
                "mining": status.mining_enabled,
                "blocks": status.total_blocks_mined,
                "currentblocksize": 0,  // Not applicable to auxiliary mining
                "currentblocktx": 0,    // Not applicable to auxiliary mining
                "difficulty": 1.0,      // Would need difficulty manager integration
                "errors": "",
                "pooledtx": status.current_work_count,
                "testnet": false,       // Would be determined from chain config
                "chain": "alys",
                "generate": status.mining_enabled,
                "genproclimit": 1,
                "hashespersec": 0.0     // Would need hash rate calculation
            });
            
            success_response(req.id, mining_info)
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getmininginfo", "actor_unavailable"])
                .inc();
            error!("Failed to send message to AuxPowActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("AuxPowActor"))
        }
    }
}

/// Handle setgenerate RPC method
async fn handle_set_generate(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["setgenerate", "called"])
        .inc();

    let generate = match extract_generate_param(&req) {
        Ok(gen) => gen,
        Err(error) => {
            RPC_REQUESTS
                .with_label_values(&["setgenerate", "invalid_params"])
                .inc();
            return error_response(req.id, error);
        }
    };

    info!("RPC setgenerate called: generate={}", generate);

    let set_enabled_msg = SetMiningEnabled {
        enabled: generate,
        mining_address: None,  // Keep current address
    };
    
    match context.auxpow_actor.send(set_enabled_msg).await {
        Ok(Ok(_)) => {
            RPC_REQUESTS
                .with_label_values(&["setgenerate", "success"])
                .inc();
            success_response(req.id, json!(generate))
        }
        Ok(Err(auxpow_error)) => {
            RPC_REQUESTS
                .with_label_values(&["setgenerate", "auxpow_error"])
                .inc();
            error_response(req.id, RpcError::from(auxpow_error))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["setgenerate", "actor_unavailable"])
                .inc();
            error!("Failed to send message to AuxPowActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("AuxPowActor"))
        }
    }
}

/// Handle getqueuedpow RPC method
async fn handle_get_queued_pow(
    req: JsonRpcRequest,
    context: &Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    RPC_REQUESTS
        .with_label_values(&["getqueuedpow", "called"])
        .inc();

    let get_queued_msg = GetQueuedAuxpow;
    
    match context.auxpow_actor.send(get_queued_msg).await {
        Ok(Some(queued_pow)) => {
            RPC_REQUESTS
                .with_label_values(&["getqueuedpow", "success"])
                .inc();
            success_response(req.id, json!(queued_pow))
        }
        Ok(None) => {
            RPC_REQUESTS
                .with_label_values(&["getqueuedpow", "no_data"])
                .inc();
            success_response(req.id, json!(null))
        }
        Err(mailbox_error) => {
            RPC_REQUESTS
                .with_label_values(&["getqueuedpow", "actor_unavailable"])
                .inc();
            error!("Failed to send message to AuxPowActor: {}", mailbox_error);
            error_response(req.id, RpcError::service_unavailable("AuxPowActor"))
        }
    }
}

// Parameter extraction helpers

/// Extract EVM address parameter from request
fn extract_address_param(req: &JsonRpcRequest) -> Result<EvmAddress, RpcError> {
    let params = req.params.as_ref().ok_or_else(|| RpcError::invalid_params())?;
    
    let address_str = params.as_str().ok_or_else(|| RpcError::invalid_params())?;
    
    address_str.parse::<EvmAddress>()
        .map_err(|_| RpcError::invalid_params())
}

/// Extract submitauxblock parameters (hash and auxpow hex)
fn extract_submit_params(req: &JsonRpcRequest) -> Result<(bitcoin::BlockHash, crate::auxpow::AuxPow), RpcError> {
    let params = req.params.as_ref().ok_or_else(|| RpcError::invalid_params())?;
    
    let params_array = params.as_array().ok_or_else(|| RpcError::invalid_params())?;
    
    if params_array.len() != 2 {
        return Err(RpcError::invalid_params());
    }
    
    let hash_str = params_array[0].as_str().ok_or_else(|| RpcError::invalid_params())?;
    let auxpow_str = params_array[1].as_str().ok_or_else(|| RpcError::invalid_params())?;
    
    // Parse block hash
    let hash = bitcoin::BlockHash::from_str(hash_str)
        .map_err(|_| RpcError::invalid_params())?;
    
    // Parse auxpow hex data
    let auxpow_bytes = hex::decode(auxpow_str)
        .map_err(|_| RpcError::invalid_params())?;
    
    // Deserialize auxpow structure
    let auxpow = crate::auxpow::AuxPow::consensus_decode(&mut auxpow_bytes.as_slice())
        .map_err(|_| RpcError::invalid_params())?;
    
    Ok((hash, auxpow))
}

/// Extract generate parameter from setgenerate request
fn extract_generate_param(req: &JsonRpcRequest) -> Result<bool, RpcError> {
    let params = req.params.as_ref().ok_or_else(|| RpcError::invalid_params())?;
    
    params.as_bool().ok_or_else(|| RpcError::invalid_params())
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
        -32606 => StatusCode::SERVICE_UNAVAILABLE, // Service unavailable
        -32607 => StatusCode::SERVICE_UNAVAILABLE, // Mining disabled
        -32608 => StatusCode::SERVICE_UNAVAILABLE, // Chain syncing
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