//! SyncActor Module
//! 
//! Implements blockchain synchronization with 99.5% threshold for block production
//! and parallel validation for 250+ blocks/sec throughput.

pub mod actor;
pub mod config;
pub mod state;
pub mod processor;
pub mod checkpoint;
pub mod peer_manager;
pub mod handlers;

#[cfg(test)]
pub mod tests;

pub use actor::SyncActor;
pub use config::{SyncConfig, FederationTimingConfig, SyncMode};
pub use state::{SyncState, SyncProgress};
pub use processor::BlockProcessor;
pub use checkpoint::CheckpointManager;
pub use peer_manager::PeerManager;