//! NetworkActor V2 Module
//!
//! Two-Actor P2P networking system with simplified protocols:
//! - NetworkActor: P2P protocols (Gossipsub, Request-Response, Identify, mDNS)
//! - SyncActor: Blockchain synchronization logic
//!
//! Removed from V1: NetworkSupervisor, Kademlia DHT, QUIC, actor_system dependencies
//! Retained: mDNS for local network discovery (essential for local development)

pub mod behaviour;
pub mod config;
pub mod handlers;
pub mod managers;
pub mod messages;
pub mod metrics;
pub mod network_actor;
pub mod protocols;
pub mod rpc;
pub mod swarm_factory;
pub mod sync_actor;
pub mod tendermint;
pub mod tendermint_sync;

pub use behaviour::AlysNetworkBehaviour;
pub use config::{NetworkConfig, SyncConfig};
pub use messages::{
    NetworkError, NetworkMessage, NetworkResponse, SyncError, SyncMessage, SyncResponse,
};
pub use metrics::{NetworkMetrics, SyncMetrics};
pub use network_actor::NetworkActor;
pub use rpc::{NetworkRpcHandler, NetworkRpcRequest, NetworkRpcResponse, NetworkSubsystem};
pub use sync_actor::SyncActor;
pub use tendermint::{
    TendermintNetworkConfig, TendermintNetworkHandler, TendermintWireMessage, TendermintWireType,
};
pub use tendermint_sync::{
    PersistableValidatorSetTracker, TendermintSyncConfig, TendermintSyncError, TendermintSyncState,
    TendermintSyncValidator, TrustedCheckpoint, ValidatorSetTracker,
};
