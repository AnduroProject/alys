//! Actor system implementations for Alys V2 architecture
//! 
//! This module contains all actor implementations that replace the shared mutable state
//! patterns from the V1 architecture. Each actor manages its own state independently
//! and communicates through message passing.
//!
//! ## Architecture
//!
//! The actor system is organized into focused modules:
//! - **chain/**: ChainActor for consensus, block production, and validation
//! - **storage/**: StorageActor for persistent data operations  
//! - **foundation/**: Core actor system infrastructure and supervision
//! - **engine/**: EngineActor for execution layer integration (Geth/Reth)
//! - **bridge_actor**: Two-way peg bridge operations  
//! - **network/**: Network actors for P2P networking, sync, and peer management
//! - **sync_actor**: Legacy blockchain synchronization (being replaced by network/sync)
//! - **stream_actor**: Real-time data streaming
//! - **governance_stream**: Governance node communication

pub mod foundation;
pub mod supervisor;
pub mod chain; // Organized chain actor module
pub mod engine; // Organized engine actor module
pub mod bridge_actor;
pub mod sync_actor; // Legacy sync - will be deprecated
pub mod network_actor; // Legacy network - will be deprecated  
pub mod network; // New network actor system (SyncActor, NetworkActor, PeerActor)
pub mod stream_actor;
pub mod storage; // Organized storage actor module
pub mod governance_stream;

pub use foundation::*;
pub use supervisor::*;
pub use chain::*; // Import from organized module
pub use engine::*; // Import from organized engine module
pub use bridge_actor::*;
pub use sync_actor::*; // Legacy sync
pub use network_actor::*; // Legacy network
pub use network::*; // New network actor system
pub use stream_actor::*;
pub use storage::*; // Import from organized storage module
pub use governance_stream::*;