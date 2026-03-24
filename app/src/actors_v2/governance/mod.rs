//! Governance Client Actor
//!
//! Provides validator-side integration with the governance service via gRPC streaming.
//! Handles:
//! - Peg-in verification requests (blocking)
//! - Receiving validator set updates from governance
//! - Connection lifecycle management with automatic reconnection
//! - Heartbeat keep-alive

mod actor;
mod client;
mod config;
mod messages;

pub use actor::GovernanceClientActor;
pub use config::GovernanceConfig;
pub use messages::{
    GovernanceError, GovernanceMessage, GovernanceResponse, GovernanceUpdateReceived,
    PeginVerificationResult, VerifyPegin,
};
