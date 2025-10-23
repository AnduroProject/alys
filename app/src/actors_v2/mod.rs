//! V2 Actor System
//!
//! This module contains the V2 actor implementations that use pure Actix
//! without the custom actor_system crate dependency.

pub mod chain;
pub mod common;
pub mod engine;
pub mod network;
pub mod rpc;
pub mod slot_worker;
pub mod storage;

pub mod testing;

// Export modules with v2 suffix to avoid collision with V1
pub use chain as chain_v2;
pub use network as network_v2;
pub use rpc as rpc_v2;
