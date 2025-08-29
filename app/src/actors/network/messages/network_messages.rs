//! NetworkActor Message Protocol
//! 
//! Defines all messages for P2P networking operations including peer discovery,
//! message broadcasting, and protocol management.

use actix::{Message, Result as ActorResult};
use serde::{Deserialize, Serialize};
use libp2p::{PeerId, Multiaddr};
use crate::actors::network::messages::{NetworkMessage, NetworkResult};

/// Start the networking subsystem
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<NetworkStartResponse>>")]
pub struct StartNetwork {
    pub listen_addresses: Vec<Multiaddr>,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub enable_mdns: bool,
}

impl NetworkMessage for StartNetwork {}

/// Stop the networking subsystem
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct StopNetwork {
    pub graceful: bool,
}

impl NetworkMessage for StopNetwork {}

/// Get current network status
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<NetworkStatus>>")]
pub struct GetNetworkStatus;

impl NetworkMessage for GetNetworkStatus {}

/// Broadcast a block to the network
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<BroadcastResponse>>")]
pub struct BroadcastBlock {
    pub block_data: Vec<u8>,
    pub block_height: u64,
    pub block_hash: String,
    pub priority: bool, // True for federation blocks
}

impl NetworkMessage for BroadcastBlock {}

/// Broadcast a transaction to the network
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<BroadcastResponse>>")]
pub struct BroadcastTransaction {
    pub tx_data: Vec<u8>,
    pub tx_hash: String,
}

impl NetworkMessage for BroadcastTransaction {}

/// Subscribe to network gossip topics
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct SubscribeToTopic {
    pub topic: GossipTopic,
}

impl NetworkMessage for SubscribeToTopic {}

/// Unsubscribe from network gossip topics
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct UnsubscribeFromTopic {
    pub topic: GossipTopic,
}

impl NetworkMessage for UnsubscribeFromTopic {}

/// Request specific data from a peer
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<RequestResponse>>")]
pub struct SendRequest {
    pub peer_id: PeerId,
    pub request_data: Vec<u8>,
    pub timeout_ms: u64,
}

impl NetworkMessage for SendRequest {}

/// Gossip topic enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipTopic {
    Blocks,
    Transactions,
    FederationMessages,
    Discovery,
    Custom(String),
}

impl std::fmt::Display for GossipTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GossipTopic::Blocks => write!(f, "blocks"),
            GossipTopic::Transactions => write!(f, "transactions"),
            GossipTopic::FederationMessages => write!(f, "federation"),
            GossipTopic::Discovery => write!(f, "discovery"),
            GossipTopic::Custom(topic) => write!(f, "{}", topic),
        }
    }
}

/// Network startup response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStartResponse {
    pub local_peer_id: PeerId,
    pub listening_addresses: Vec<Multiaddr>,
    pub protocols: Vec<String>,
    pub started_at: std::time::SystemTime,
}

/// Current network status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub is_active: bool,
    pub local_peer_id: PeerId,
    pub listening_addresses: Vec<Multiaddr>,
    pub connected_peers: u32,
    pub pending_connections: u32,
    pub total_bandwidth_in: u64,  // Bytes
    pub total_bandwidth_out: u64, // Bytes
    pub active_protocols: Vec<String>,
    pub gossip_topics: Vec<GossipTopic>,
    pub discovery_status: DiscoveryStatus,
}

/// Peer discovery status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryStatus {
    pub mdns_enabled: bool,
    pub kad_routing_table_size: u32,
    pub bootstrap_peers_connected: u32,
    pub total_discovered_peers: u32,
}

/// Broadcast operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResponse {
    pub message_id: String,
    pub peers_reached: u32,
    pub propagation_started_at: std::time::SystemTime,
}

/// Request-response operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestResponse {
    pub response_data: Vec<u8>,
    pub peer_id: PeerId,
    pub duration_ms: u64,
}

// Network events for inter-actor communication
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct PeerConnected {
    pub peer_id: PeerId,
    pub address: Multiaddr,
    pub protocols: Vec<String>,
    pub is_federation_peer: bool,
}

impl NetworkMessage for PeerConnected {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct PeerDisconnected {
    pub peer_id: PeerId,
    pub reason: String,
}

impl NetworkMessage for PeerDisconnected {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct MessageReceived {
    pub from_peer: PeerId,
    pub topic: GossipTopic,
    pub data: Vec<u8>,
    pub received_at: std::time::SystemTime,
}

impl NetworkMessage for MessageReceived {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct NetworkEvent {
    pub event_type: NetworkEventType,
    pub timestamp: std::time::SystemTime,
    pub details: String,
}

impl NetworkMessage for NetworkEvent {}

/// Types of network events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEventType {
    BootstrapCompleted,
    PartitionDetected,
    PartitionRecovered,
    ProtocolUpgrade,
    BandwidthLimitExceeded,
    SecurityViolation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gossip_topic_display() {
        assert_eq!(GossipTopic::Blocks.to_string(), "blocks");
        assert_eq!(GossipTopic::Transactions.to_string(), "transactions");
        assert_eq!(GossipTopic::FederationMessages.to_string(), "federation");
        assert_eq!(GossipTopic::Discovery.to_string(), "discovery");
        assert_eq!(GossipTopic::Custom("test".to_string()).to_string(), "test");
    }

    #[test]
    fn network_message_creation() {
        let start_msg = StartNetwork {
            listen_addresses: vec![],
            bootstrap_peers: vec![],
            enable_mdns: true,
        };
        
        assert!(start_msg.enable_mdns);
        assert_eq!(start_msg.listen_addresses.len(), 0);
    }

    #[test]
    fn broadcast_message_priority() {
        let block_msg = BroadcastBlock {
            block_data: vec![1, 2, 3],
            block_height: 100,
            block_hash: "test_hash".to_string(),
            priority: true,
        };
        
        assert!(block_msg.priority);
        assert_eq!(block_msg.block_height, 100);
    }
}