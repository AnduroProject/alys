//! Network Actor System for Alys V2
//! 
//! This module contains the complete networking subsystem consisting of three core actors:
//! - **SyncActor**: Blockchain synchronization with 99.5% threshold and parallel validation
//! - **NetworkActor**: P2P protocol management with libp2p integration  
//! - **PeerActor**: Connection management and peer scoring for 1000+ concurrent peers
//!
//! ## Architecture
//!
//! The network actors form the communication backbone of the Alys V2 system:
//! - High-performance sync (250+ blocks/sec with parallel validation)
//! - Reliable block propagation (sub-100ms gossip latency)
//! - Scalable peer management (1000+ concurrent connections)
//! - Robust fault tolerance (automatic recovery from network partitions)
//!
//! ## Key Features
//!
//! - **99.5% Sync Threshold**: Enables block production before 100% sync
//! - **libp2p Integration**: Gossipsub, Kademlia DHT, mDNS discovery
//! - **Federation Timing**: Respects 2-second Aura PoA block intervals
//! - **Checkpoint Recovery**: Resilient sync with state snapshots
//! - **SIMD Optimizations**: Hardware-accelerated validation
//! - **Network Supervision**: Fault tolerance with automatic actor restart

pub mod messages;
pub mod supervisor;
pub mod sync;
pub mod network;
pub mod peer;
pub mod transport;

#[cfg(test)]
pub mod tests;

// Re-export core types for external use
pub use messages::*;
pub use supervisor::NetworkSupervisor;
pub use sync::SyncActor;
pub use network::NetworkActor;
pub use peer::PeerActor;

// Configuration re-exports
pub use sync::SyncConfig;
pub use network::NetworkConfig;
pub use peer::PeerConfig;