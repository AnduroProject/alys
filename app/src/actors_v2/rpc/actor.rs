use actix::{Actor, ActorFutureExt, Addr, Context, Handler, WrapFuture};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::sync::RwLock;

use super::config::RpcConfig;
use crate::metrics::{RPC_REQUESTS, RPC_REQUEST_DURATION};
use super::error::{JsonRpcError, RpcError};
use super::handlers::{CreateAuxBlockHandler, SubmitAuxBlockHandler};
use super::messages::{GetRpcStatus, RpcStatus, StartRpcServer, StopRpcServer};
use crate::actors_v2::chain::ChainActor;

/// JSON-RPC 1.0 request structure (Bitcoin-compatible)
#[derive(Debug, Clone, serde::Deserialize)]
struct JsonRpcRequest {
    pub method: String,
    #[serde(default)]
    pub params: Vec<Value>,
    pub id: Option<Value>,
}

/// JSON-RPC 1.0 response structure (Bitcoin-compatible)
#[derive(Debug, Clone, serde::Serialize)]
struct JsonRpcResponse {
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
    pub id: Option<Value>,
}

/// RPC server state (shared across handlers)
#[derive(Clone)]
struct RpcServerState {
    chain_actor: Addr<ChainActor>,
    config: RpcConfig,
    metrics: Arc<RwLock<RpcMetrics>>,
}

/// RPC metrics
#[derive(Debug, Default)]
struct RpcMetrics {
    requests_handled: u64,
    errors_count: u64,
    start_time: Option<SystemTime>,
}

/// RpcActor manages JSON-RPC server lifecycle
pub struct RpcActor {
    config: RpcConfig,
    chain_actor: Addr<ChainActor>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
    metrics: Arc<RwLock<RpcMetrics>>,
    start_time: Option<SystemTime>,
}

impl RpcActor {
    /// Create new RpcActor
    pub fn new(config: RpcConfig, chain_actor: Addr<ChainActor>) -> Self {
        Self {
            config,
            chain_actor,
            server_handle: None,
            metrics: Arc::new(RwLock::new(RpcMetrics::default())),
            start_time: None,
        }
    }

    /// Handle HTTP request
    async fn handle_http_request(
        req: Request<Body>,
        state: RpcServerState,
    ) -> Result<Response<Body>, Infallible> {
        // Start timing for Prometheus metrics
        let request_start = Instant::now();

        // Only accept POST requests
        if req.method() != Method::POST {
            RPC_REQUESTS
                .with_label_values(&["unknown", "error"])
                .inc();
            return Ok(Self::error_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "Method not allowed",
                None,
            ));
        }

        // Read request body
        let body_bytes = match hyper::body::to_bytes(req.into_body()).await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to read request body");
                return Ok(Self::error_response(
                    StatusCode::BAD_REQUEST,
                    "Failed to read request body",
                    None,
                ));
            }
        };

        // Parse JSON-RPC request
        let rpc_request: JsonRpcRequest = match serde_json::from_slice(&body_bytes) {
            Ok(req) => req,
            Err(e) => {
                tracing::error!(error = ?e, "Invalid JSON-RPC request");
                state.metrics.write().await.errors_count += 1;
                RPC_REQUESTS
                    .with_label_values(&["parse_error", "error"])
                    .inc();
                return Ok(Self::json_rpc_error_response(
                    RpcError::InvalidRequest("Invalid JSON".to_string()),
                    None,
                ));
            }
        };

        tracing::debug!(
            method = %rpc_request.method,
            params_count = rpc_request.params.len(),
            "RPC request received"
        );

        // Route to appropriate handler
        let method_name = rpc_request.method.clone();
        let result = Self::route_request(rpc_request.clone(), state.clone()).await;

        // Record request duration for Prometheus
        let duration = request_start.elapsed();
        RPC_REQUEST_DURATION
            .with_label_values(&[&method_name])
            .observe(duration.as_secs_f64());

        // Update metrics
        {
            let mut metrics = state.metrics.write().await;
            metrics.requests_handled += 1;
            if result.is_err() {
                metrics.errors_count += 1;
            }
        }

        // Build response and record Prometheus status
        let response = match result {
            Ok(value) => {
                RPC_REQUESTS
                    .with_label_values(&[&method_name, "success"])
                    .inc();
                JsonRpcResponse {
                    result: Some(value),
                    error: None,
                    id: rpc_request.id,
                }
            }
            Err(e) => {
                RPC_REQUESTS
                    .with_label_values(&[&method_name, "error"])
                    .inc();
                tracing::warn!(
                    method = %rpc_request.method,
                    error = ?e,
                    "RPC request failed"
                );
                JsonRpcResponse {
                    result: None,
                    error: Some(e.to_json_rpc_error()),
                    id: rpc_request.id,
                }
            }
        };

        Ok(Self::json_response(response))
    }

    /// Route request to appropriate handler
    async fn route_request(req: JsonRpcRequest, state: RpcServerState) -> Result<Value, RpcError> {
        match req.method.as_str() {
            "createauxblock" => CreateAuxBlockHandler::handle(req.params, state.chain_actor).await,
            "submitauxblock" => SubmitAuxBlockHandler::handle(req.params, state.chain_actor).await,
            _ => Err(RpcError::MethodNotFound(req.method)),
        }
    }

    /// Create JSON response
    fn json_response(data: JsonRpcResponse) -> Response<Body> {
        let body = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    /// Create error response
    fn error_response(status: StatusCode, message: &str, id: Option<Value>) -> Response<Body> {
        let response = JsonRpcResponse {
            result: None,
            error: Some(JsonRpcError {
                code: -32603,
                message: message.to_string(),
            }),
            id,
        };
        let body = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    /// Create JSON-RPC error response
    fn json_rpc_error_response(error: RpcError, id: Option<Value>) -> Response<Body> {
        let response = JsonRpcResponse {
            result: None,
            error: Some(error.to_json_rpc_error()),
            id,
        };
        Self::json_response(response)
    }
}

impl Actor for RpcActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("RpcActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("RpcActor stopped");
    }
}

// Message handlers

impl Handler<StartRpcServer> for RpcActor {
    type Result = actix::ResponseActFuture<Self, Result<(), RpcError>>;

    fn handle(&mut self, _msg: StartRpcServer, _ctx: &mut Self::Context) -> Self::Result {
        if self.server_handle.is_some() {
            return Box::pin(
                async { Err(RpcError::Internal("Server already running".to_string())) }
                    .into_actor(self),
            );
        }

        if let Err(e) = self.config.validate() {
            return Box::pin(async move { Err(RpcError::Internal(e)) }.into_actor(self));
        }

        let addr = self.config.bind_address;
        let state = RpcServerState {
            chain_actor: self.chain_actor.clone(),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
        };

        // Spawn server in background
        let make_svc = make_service_fn(move |_conn| {
            let state = state.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    RpcActor::handle_http_request(req, state.clone())
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);
        let handle = tokio::spawn(async move {
            if let Err(e) = server.await {
                tracing::error!(error = ?e, "RPC server error");
            }
        });

        self.server_handle = Some(handle);
        self.start_time = Some(SystemTime::now());
        self.metrics = Arc::new(RwLock::new(RpcMetrics {
            requests_handled: 0,
            errors_count: 0,
            start_time: Some(SystemTime::now()),
        }));

        tracing::info!(address = %addr, "RPC server started");

        Box::pin(async { Ok(()) }.into_actor(self))
    }
}

impl Handler<StopRpcServer> for RpcActor {
    type Result = Result<(), RpcError>;

    fn handle(&mut self, _msg: StopRpcServer, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
            self.start_time = None;
            tracing::info!("RPC server stopped");
            Ok(())
        } else {
            Err(RpcError::ServerNotRunning)
        }
    }
}

impl Handler<GetRpcStatus> for RpcActor {
    type Result = actix::ResponseActFuture<Self, RpcStatus>;

    fn handle(&mut self, _msg: GetRpcStatus, _ctx: &mut Self::Context) -> Self::Result {
        let uptime_secs = self
            .start_time
            .and_then(|start| SystemTime::now().duration_since(start).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let metrics = self.metrics.clone();
        let running = self.server_handle.is_some();
        let port = self.config.bind_address.port();

        let fut = async move {
            let metrics = metrics.read().await;
            RpcStatus {
                running,
                port,
                requests_handled: metrics.requests_handled,
                errors_count: metrics.errors_count,
                uptime_secs,
            }
        };

        Box::pin(fut.into_actor(self).map(|result, _actor, _ctx| result))
    }
}
