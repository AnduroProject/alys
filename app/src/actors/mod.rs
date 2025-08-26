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
//! - **engine_actor**: Execution layer integration (Geth/Reth)
//! - **bridge_actor**: Two-way peg bridge operations
//! - **network_actor**: P2P networking and peer management
//! - **sync_actor**: Blockchain synchronization
//! - **stream_actor**: Real-time data streaming
//! - **governance_stream**: Governance node communication

pub mod foundation;
pub mod supervisor;
pub mod chain; // Organized chain actor module
pub mod engine_actor;
pub mod bridge_actor;
pub mod sync_actor;
pub mod network_actor;
pub mod stream_actor;
pub mod storage; // Organized storage actor module
pub mod governance_stream;

pub use foundation::*;
pub use supervisor::*;
pub use chain::*; // Import from organized module
pub use engine_actor::*;
pub use bridge_actor::*;
pub use sync_actor::*;
pub use network_actor::*;
pub use stream_actor::*;
pub use storage::*; // Import from organized storage module
pub use governance_stream::*;