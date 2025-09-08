//! Unified RPC Server for Alys V2 Actor System
//!
//! This module provides a consolidated RPC interface that routes requests to
//! appropriate actors based on the method domain:
//! - Chain methods -> ChainActor
//! - Mining methods -> AuxPowActor  
//! - Bridge methods -> BridgeActor
//!
//! This replaces the previous fragmented RPC implementations with a single,
//! maintainable RPC server.

use std::net::SocketAddr;
use std::sync::Arc;
use actix::prelude::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use tracing::{info, error};

use crate::actors::{
    auxpow::AuxPowActor,
    bridge::BridgeActor,
    chain::ChainActor,
    engine::EngineActor,
    storage::StorageActor,
};
use bitcoin::address::NetworkChecked;
use bitcoin::Address;

mod types;
mod error;
mod chain_methods;
mod mining_methods;
mod bridge_methods;

pub use types::*;
pub use error::*;

/// Unified RPC server context containing all necessary actor addresses
#[derive(Clone)]
pub struct UnifiedRpcContext {
    /// ChainActor for blockchain queries
    pub chain_actor: Addr<ChainActor>,
    /// EngineActor for execution layer queries  
    pub engine_actor: Addr<EngineActor>,
    /// StorageActor for data persistence queries
    pub storage_actor: Addr<StorageActor>,
    /// AuxPowActor for mining operations
    pub auxpow_actor: Addr<AuxPowActor>,
    /// BridgeActor for federation operations
    pub bridge_actor: Addr<BridgeActor>,
    /// Federation address for peg operations
    pub federation_address: Address<NetworkChecked>,
}

/// Main entry point for the unified RPC server
pub async fn run_unified_rpc_server(
    chain_actor: Addr<ChainActor>,
    engine_actor: Addr<EngineActor>, 
    storage_actor: Addr<StorageActor>,
    auxpow_actor: Addr<AuxPowActor>,
    bridge_actor: Addr<BridgeActor>,
    federation_address: Address<NetworkChecked>,
    rpc_port: u16,
) {
    let addr = SocketAddr::from(([0, 0, 0, 0], rpc_port));
    
    let rpc_context = Arc::new(UnifiedRpcContext {
        chain_actor: chain_actor.clone(),
        engine_actor: engine_actor.clone(),
        storage_actor: storage_actor.clone(),
        auxpow_actor: auxpow_actor.clone(),
        bridge_actor: bridge_actor.clone(),
        federation_address: federation_address.clone(),
    });

    info!("Starting Unified RPC server on {}", addr);
    
    let server = Server::bind(&addr).serve(make_service_fn(move |_conn| {
        let context = rpc_context.clone();
        
        async move {
            Ok::<_, GenericError>(service_fn(move |req| {
                let ctx = context.clone();
                handle_rpc_request(req, ctx)
            }))
        }
    }));

    // TODO: handle graceful shutdown with actor system
    tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("Unified RPC server error: {}", e);
        }
    });
    
    info!("Unified RPC server started successfully");
}

/// Main request handler that routes methods to appropriate domain handlers
async fn handle_rpc_request(
    req: Request<Body>,
    context: Arc<UnifiedRpcContext>,
) -> Result<Response<Body>, GenericError> {
    if req.method() != Method::POST {
        return Ok(Response::builder()
            .status(hyper::StatusCode::METHOD_NOT_ALLOWED)
            .body("Unified RPC server handles only POST requests".into())?);
    }

    let bytes = hyper::body::to_bytes(req.into_body()).await?;
    let json_req = serde_json::from_slice::<JsonRpcRequest>(&bytes)?;
    let id = json_req.id.clone();

    // Route to appropriate domain handler based on method
    let response = match json_req.method.as_str() {
        // Chain domain methods
        "getblockbyheight" | "getblockbyhash" | "getblockcount" | "getchainmetrics" => {
            chain_methods::handle_chain_method(json_req, &context).await
        }
        
        // Mining domain methods  
        "createauxblock" | "submitauxblock" | "getauxblock" | "getmininginfo" | "setgenerate" | "getqueuedpow" => {
            mining_methods::handle_mining_method(json_req, &context).await
        }
        
        // Bridge domain methods
        "getfederationaddress" | "getdepositaddress" => {
            bridge_methods::handle_bridge_method(json_req, &context).await
        }
        
        _ => {
            // Method not found
            Ok(Response::builder()
                .status(hyper::StatusCode::NOT_FOUND)
                .body(
                    JsonRpcResponse {
                        result: None,
                        error: Some(RpcError::method_not_found()),
                        id,
                    }
                    .into(),
                )?)
        }
    };

    response
}

type GenericError = Box<dyn std::error::Error + Send + Sync>;