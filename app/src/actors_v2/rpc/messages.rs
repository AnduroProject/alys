use super::error::RpcError;
use actix::Message;
use serde::{Deserialize, Serialize};

/// Start RPC server
#[derive(Debug, Clone)]
pub struct StartRpcServer;

impl Message for StartRpcServer {
    type Result = Result<(), RpcError>;
}

/// Stop RPC server
#[derive(Debug, Clone)]
pub struct StopRpcServer;

impl Message for StopRpcServer {
    type Result = Result<(), RpcError>;
}

/// Get RPC server status
#[derive(Debug, Clone)]
pub struct GetRpcStatus;

impl Message for GetRpcStatus {
    type Result = RpcStatus;
}

/// RPC server status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcStatus {
    pub running: bool,
    pub port: u16,
    pub requests_handled: u64,
    pub errors_count: u64,
    pub uptime_secs: u64,
}
