//! Bridge Supervisor Module
//! 
//! Comprehensive bridge system for Bitcoin <-> Alys peg operations.
//! Contains specialized actors for different aspects of bridge operations:
//! - BridgeActor: Coordination and orchestration
//! - PegInActor: Bitcoin deposit processing 
//! - PegOutActor: Bitcoin withdrawal processing
//! - StreamActor: Governance communication

pub mod messages;
pub mod actors;
pub mod shared;
pub mod supervision;
pub mod integration;
pub mod metrics;
pub mod config;

#[cfg(test)]
pub mod tests;

pub use actors::bridge::BridgeActor;
pub use actors::pegin::PegInActor;
pub use actors::pegout::PegOutActor;
pub use actors::stream::StreamActor;
pub use supervision::BridgeSupervisor;
pub use config::BridgeSystemConfig;
pub use messages::*;