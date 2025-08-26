//! Storage Actor Message Handlers
//!
//! This module contains all message handlers for the Storage Actor,
//! organized by functional area for maintainability and clarity.

pub mod block_handlers;
pub mod state_handlers;
pub mod maintenance_handlers;
pub mod query_handlers;

// Re-export handler-specific message types
pub use block_handlers::{GetBlockRangeMessage, BlockExistsMessage};