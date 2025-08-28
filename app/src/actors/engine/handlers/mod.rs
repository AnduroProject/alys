//! Engine Actor Message Handlers
//!
//! This module organizes all message handlers for the EngineActor into functional categories:
//! - Payload handlers: Building and executing payloads
//! - Forkchoice handlers: Managing execution layer head/finalized state
//! - Sync handlers: Engine synchronization status
//! - Client handlers: Execution client lifecycle and health

pub mod payload_handlers;
pub mod forkchoice_handlers;
pub mod sync_handlers;
pub mod client_handlers;

// Re-export handler implementations
pub use payload_handlers::*;
pub use forkchoice_handlers::*;
pub use sync_handlers::*;
pub use client_handlers::*;