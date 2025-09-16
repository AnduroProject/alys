//! V2 AuxPow Actor System
//!
//! This module implements the complete V2 replacement for the legacy AuxPowMiner
//! with 100% functional parity. The system consists of specialized actors that
//! handle Bitcoin merged mining operations through message passing.

pub mod actor;
pub mod difficulty;
pub mod messages;
pub mod config;
pub mod error;
pub mod metrics;
pub mod rpc;
pub mod types;

#[cfg(test)]
pub mod tests;

// Re-export main types
pub use actor::AuxPowActor;
pub use difficulty::DifficultyManager;
pub use messages::*;
pub use config::*;
pub use error::*;
pub use metrics::*;
pub use rpc::*;
pub use types::*;