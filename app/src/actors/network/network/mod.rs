//! NetworkActor Module
//! 
//! P2P protocol management with libp2p integration for gossipsub, Kademlia DHT,
//! and mDNS discovery with federation-aware message routing.

pub mod actor;
pub mod config;
pub mod behaviour;
pub mod protocols;
pub mod handlers;

#[cfg(test)]
pub mod tests;

pub use actor::NetworkActor;
pub use config::{NetworkConfig, GossipConfig, DiscoveryConfig, TransportConfig};