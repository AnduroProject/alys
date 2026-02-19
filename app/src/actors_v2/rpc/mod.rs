//! RpcActor V2 - JSON-RPC 1.0 Server
//!
//! Exposes mining endpoints (createauxblock, submitauxblock) and
//! Tendermint consensus queries (tendermint_*).

pub mod actor;
pub mod config;
pub mod error;
pub mod handlers;
pub mod messages;
pub mod tendermint_types;

pub use actor::RpcActor;
pub use config::RpcConfig;
pub use error::RpcError;
pub use messages::{GetRpcStatus, RpcStatus, StartRpcServer, StopRpcServer};
pub use tendermint_types::*;
