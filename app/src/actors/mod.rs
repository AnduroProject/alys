//! Actor system implementations for Alys V2 architecture
//! 
//! This module contains all actor implementations that replace the shared mutable state
//! patterns from the V1 architecture. Each actor manages its own state independently
//! and communicates through message passing via the actor_system crate.
//!
//! ## Architecture
//!
//! The actor system is organized into focused modules:
//! - **chain/**: ChainActor for consensus, block production, and validation
//! - **storage/**: StorageActor for persistent data operations  
//! - **bridge/**: Bridge actors for two-way peg operations with Bitcoin
//! - **engine/**: EngineActor for execution layer integration (Geth/Reth)
//! - **network/**: Network actors for P2P networking, sync, and peer management
//! - **bridge/actors/stream**: Consolidated StreamActor for governance communication
//!
//! All actors use the actor_system crate for supervision, lifecycle management,
//! message handling, and metrics collection.

pub mod chain; // Organized chain actor module
pub mod engine; // Organized engine actor module
pub mod bridge; // Bridge actor system
pub mod network; // Network actor system (SyncActor, NetworkActor, PeerActor)
pub mod sync; // Sync actor system
pub mod storage; // Organized storage actor module
pub mod supervisor; // Root supervisor for all actors
pub mod shared; // Shared structures like ActorAddresses

#[cfg(test)]
pub mod tests; // V2 Actor system integration tests

pub use chain::*; // Import from organized module
pub use engine::*; // Import from organized engine module  
pub use network::*; // New network actor system
pub use storage::*; // Import from organized storage module
pub use bridge::actors::stream::*; // Export new consolidated StreamActor
pub use supervisor::*; // Export supervisor
pub use shared::*; // Export shared structures