//! NetworkActor V2 Module
//!
//! Two-Actor P2P networking system with simplified protocols:
//! - NetworkActor: P2P protocols (Gossipsub, Request-Response)
//! - SyncActor: Blockchain synchronization logic
//!
//! Removed from V1: NetworkSupervisor, Kademlia DHT, mDNS, QUIC, actor_system dependencies

pub mod network_actor;
pub mod sync_actor;
pub mod config;
pub mod messages;
pub mod behaviour;
pub mod swarm_factory;
pub mod metrics;
pub mod managers;
pub mod protocols;
pub mod handlers;
pub mod rpc;

pub use network_actor::NetworkActor;
pub use sync_actor::SyncActor;
pub use config::{NetworkConfig, SyncConfig};
pub use messages::{NetworkMessage, SyncMessage, NetworkResponse, SyncResponse, NetworkError, SyncError};
pub use behaviour::AlysNetworkBehaviour;
pub use metrics::{NetworkMetrics, SyncMetrics};
pub use rpc::{NetworkRpcHandler, NetworkSubsystem, NetworkRpcRequest, NetworkRpcResponse};