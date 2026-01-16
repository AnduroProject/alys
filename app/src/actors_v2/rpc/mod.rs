//! RpcActor V2 - JSON-RPC 1.0 Server
//!
//! Exposes createauxblock and submitauxblock endpoints for mining pool integration

pub mod actor;
pub mod config;
pub mod error;
pub mod handlers;
pub mod messages;

pub use actor::RpcActor;
pub use config::RpcConfig;
pub use error::RpcError;
pub use messages::{GetRpcStatus, RpcStatus, StartRpcServer, StopRpcServer};
