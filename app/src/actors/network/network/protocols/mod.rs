//! Network Protocol Implementations
//! 
//! Core libp2p protocol implementations for the Alys blockchain network:
//! - Gossipsub for block/transaction propagation
//! - Kademlia DHT + mDNS for peer discovery
//! - Request-Response for block downloads and sync coordination

pub mod gossip;
pub mod discovery;
pub mod request_response;

pub use gossip::{AlysGossipsub, AlysGossipEvent, GossipMetrics, MessagePriority};
pub use discovery::{AlysDiscovery, AlysDiscoveryEvent, DiscoveryConfig, DiscoveredPeer, DiscoverySource};
pub use request_response::{
    AlysRequestResponse, AlysRequestResponseEvent, AlysRequest, AlysResponse, 
    AlysRequestType, FederationMessageType, BlockInfo
};