//! V2 Actor System
//!
//! This module contains the V2 actor implementations that use pure Actix
//! without the custom actor_system crate dependency.

pub mod storage;
pub mod network;

pub mod testing;

// Export network module as network_v2 to avoid collision with V1
pub use network as network_v2;