//! Anduro Governance Stream Actor for bi-directional gRPC communication
//!
//! This module implements the StreamActor responsible for establishing and maintaining 
//! persistent bi-directional streaming communication with Anduro Governance nodes. 
//! The actor handles message routing, connection resilience, buffering during 
//! disconnections, and serves as the gateway for all governance operations including 
//! signature requests and federation updates.
//!
//! # Architecture
//!
//! The StreamActor follows the Alys V2 actor-based architecture patterns and integrates 
//! with the governance system for:
//! - Signature request/response coordination
//! - Federation membership updates
//! - Consensus coordination
//! - Emergency governance actions
//! - Health monitoring and status reporting
//!
//! # Protocol Design
//!
//! The stream communication uses gRPC bidirectional streaming with authentication
//! via Bearer tokens. Messages are protobuf-encoded and support various governance
//! operations including signature requests, federation updates, and consensus coordination.

pub mod actor;
pub mod config;
pub mod error;
pub mod messages;
pub mod protocol;
pub mod reconnect;
pub mod types;

#[cfg(test)]
pub mod tests;

// Re-export commonly used types
pub use actor::StreamActor;
pub use config::StreamConfig;
pub use error::StreamError;
pub use messages::*;
pub use protocol::GovernanceProtocol;
pub use reconnect::ExponentialBackoff;
pub use types::*;

/// Stream actor system version for protocol compatibility
pub const STREAM_PROTOCOL_VERSION: &str = "v1.0.0";

/// Default configuration values
pub const DEFAULT_MAX_GOVERNANCE_CONNECTIONS: usize = 10;
pub const DEFAULT_BUFFER_SIZE: usize = 1000;
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub const DEFAULT_CONNECTION_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_RECONNECT_INITIAL_DELAY_MS: u64 = 1000;
pub const DEFAULT_RECONNECT_MAX_DELAY_SECS: u64 = 300;
pub const DEFAULT_RECONNECT_MULTIPLIER: f64 = 2.0;
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_MAX_PENDING_REQUESTS: usize = 100;
pub const DEFAULT_MESSAGE_TTL_SECS: u64 = 3600;