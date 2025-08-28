//! Engine Actor Module
//!
//! This module contains the complete EngineActor implementation following the V2 actor pattern.
//! The EngineActor manages the interface to Ethereum execution clients (Geth/Reth),
//! handles payload building and execution, and coordinates with the consensus layer.
//!
//! ## Architecture
//!
//! The EngineActor is part of the Execution Layer in the V2 system architecture:
//! ```
//! ┌─────────────┐    ┌──────────────┐    ┌─────────────┐
//! │ ChainActor  │───▶│ EngineActor  │───▶│ Geth/Reth   │
//! │             │    │              │    │             │
//! │ Block Prod. │    │ EVM Interface│    │ Execution   │
//! │ Aura PoA    │    │ Block Build  │    │ Client      │
//! └─────────────┘    └──────────────┘    └─────────────┘
//! ```
//!
//! ## Key Responsibilities
//!
//! - **Payload Building**: Construct execution payloads with transactions and withdrawals
//! - **Payload Execution**: Execute payloads and validate execution results
//! - **Forkchoice Updates**: Manage execution layer head/finalized state
//! - **Client Management**: Handle execution client connectivity and health
//! - **Actor Integration**: Coordinate with ChainActor, BridgeActor, StorageActor

// Re-export public interface
pub use actor::EngineActor;
pub use config::EngineConfig;
pub use state::{ExecutionState, PayloadStatus, PendingPayload};
pub use messages::*;
pub use client::{ExecutionClient, EngineApiClient, PublicApiClient};
pub use engine::Engine;
pub use metrics::EngineActorMetrics;
pub use integration::*;

// Internal modules
pub mod actor;
pub mod config;
pub mod state;
pub mod messages;
pub mod handlers;
pub mod client;
pub mod engine;
pub mod metrics;
pub mod validation;
pub mod supervision;
pub mod integration;
pub mod tests;

// Use local types from actor.rs for now (until actor_system crate is fixed)
use actor::BlockchainActorPriority;

/// Engine actor priority in the supervision hierarchy
pub const ENGINE_ACTOR_PRIORITY: BlockchainActorPriority = BlockchainActorPriority::Consensus;

/// Engine actor restart strategy (simplified for now)
#[derive(Debug, Clone)]
pub enum RestartStrategy {
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        max_restarts: usize,
        reset_after: Duration,
    },
}

pub const ENGINE_RESTART_STRATEGY: RestartStrategy = RestartStrategy::ExponentialBackoff {
    initial_delay: Duration::from_millis(100),
    max_delay: Duration::from_secs(30),
    max_restarts: 5,
    reset_after: Duration::from_minutes(5),
};

/// Error types for the Engine actor system
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Execution client error: {0}")]
    ClientError(#[from] ClientError),
    
    #[error("Payload not found: {0}")]
    PayloadNotFound(String),
    
    #[error("Invalid payload: {0}")]
    InvalidPayload(String),
    
    #[error("Execution timeout")]
    ExecutionTimeout,
    
    #[error("Forkchoice error: {0}")]
    ForkchoiceError(String),
    
    #[error("Actor communication error: {0}")]
    ActorError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Client-specific error types
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Authentication failed")]
    AuthenticationFailed,
    
    #[error("RPC error: {0}")]
    RpcError(String),
    
    #[error("Network timeout")]
    NetworkTimeout,
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Result type for engine operations
pub type EngineResult<T> = Result<T, EngineError>;

use std::time::Duration;