//! Network P2P communication messages

use crate::types::*;
use actix::prelude::*;

/// Message to connect to a peer
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct ConnectToPeerMessage {
    pub multiaddr: String,
}

/// Message to disconnect from a peer
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct DisconnectFromPeerMessage {
    pub peer_id: PeerId,
    pub reason: String,
}

/// Message to publish data to a topic
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct PublishMessage {
    pub topic: String,
    pub data: Vec<u8>,
}

/// Message to subscribe to a topic
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct SubscribeToTopicMessage {
    pub topic: String,
}

/// Message to unsubscribe from a topic
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct UnsubscribeFromTopicMessage {
    pub topic: String,
}

/// Message to send direct message to a peer
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct SendDirectMessage {
    pub peer_id: PeerId,
    pub protocol: String,
    pub data: Vec<u8>,
}

/// Message to handle incoming gossipsub message
#[derive(Message)]
#[rtype(result = "()")]
pub struct IncomingGossipMessage {
    pub topic: String,
    pub peer_id: PeerId,
    pub data: Vec<u8>,
}

/// Message to handle incoming direct message
#[derive(Message)]
#[rtype(result = "()")]
pub struct IncomingDirectMessage {
    pub peer_id: PeerId,
    pub protocol: String,
    pub data: Vec<u8>,
}

/// Message to handle peer connection event
#[derive(Message)]
#[rtype(result = "()")]
pub struct PeerConnectedMessage {
    pub peer_id: PeerId,
    pub multiaddr: String,
    pub direction: ConnectionDirection,
}

/// Message to handle peer disconnection event
#[derive(Message)]
#[rtype(result = "()")]
pub struct PeerDisconnectedMessage {
    pub peer_id: PeerId,
    pub reason: String,
}

/// Message to get network status
#[derive(Message)]
#[rtype(result = "NetworkStatus")]
pub struct GetNetworkStatusMessage;

/// Message to get connected peers
#[derive(Message)]
#[rtype(result = "Vec<PeerConnection>")]
pub struct GetPeersMessage;

/// Message to ban a peer
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct BanPeerMessage {
    pub peer_id: PeerId,
    pub duration: std::time::Duration,
    pub reason: String,
}

/// Message to update peer reputation
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdatePeerReputationMessage {
    pub peer_id: PeerId,
    pub delta: i32,
    pub reason: String,
}

/// Message to discover new peers
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct DiscoverPeersMessage {
    pub count: usize,
}

/// Message to handle DHT query
#[derive(Message)]
#[rtype(result = "Result<DhtQueryResult, NetworkError>")]
pub struct DhtQueryMessage {
    pub query_type: DhtQueryType,
    pub key: Vec<u8>,
}

/// Message to handle DHT put operation
#[derive(Message)]
#[rtype(result = "Result<(), NetworkError>")]
pub struct DhtPutMessage {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub ttl: std::time::Duration,
}

/// Connection direction
#[derive(Debug, Clone)]
pub enum ConnectionDirection {
    Inbound,
    Outbound,
}

/// Network status information
#[derive(Debug, Clone)]
pub struct NetworkStatus {
    pub local_peer_id: PeerId,
    pub listen_addresses: Vec<String>,
    pub connected_peers: usize,
    pub banned_peers: usize,
    pub subscribed_topics: Vec<String>,
    pub network_stats: NetworkStats,
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connections_established: u64,
    pub connections_dropped: u64,
}

/// Peer connection information
#[derive(Debug, Clone)]
pub struct PeerConnection {
    pub peer_id: PeerId,
    pub multiaddr: String,
    pub direction: ConnectionDirection,
    pub connected_at: std::time::SystemTime,
    pub protocols: Vec<String>,
    pub reputation: PeerReputation,
}

/// Peer reputation data
#[derive(Debug, Clone)]
pub struct PeerReputation {
    pub score: i32,
    pub last_interaction: std::time::SystemTime,
    pub successful_interactions: u64,
    pub failed_interactions: u64,
    pub violations: Vec<ReputationViolation>,
}

/// Reputation violation types
#[derive(Debug, Clone)]
pub struct ReputationViolation {
    pub violation_type: ViolationType,
    pub timestamp: std::time::SystemTime,
    pub severity: u8,
    pub description: String,
}

/// Types of reputation violations
#[derive(Debug, Clone)]
pub enum ViolationType {
    InvalidMessage,
    Spam,
    BadBehavior,
    ProtocolViolation,
    Timeout,
    Disconnect,
}

/// DHT query types
#[derive(Debug, Clone)]
pub enum DhtQueryType {
    GetValue,
    GetProviders,
    FindPeer,
    GetClosestPeers,
}

/// DHT query result
#[derive(Debug, Clone)]
pub struct DhtQueryResult {
    pub query_type: DhtQueryType,
    pub key: Vec<u8>,
    pub result: DhtResult,
}

/// DHT operation results
#[derive(Debug, Clone)]
pub enum DhtResult {
    Value(Vec<u8>),
    Providers(Vec<PeerId>),
    Peer(PeerRecord),
    Peers(Vec<PeerId>),
    NotFound,
}

/// Peer record from DHT
#[derive(Debug, Clone)]
pub struct PeerRecord {
    pub peer_id: PeerId,
    pub addresses: Vec<String>,
    pub protocols: Vec<String>,
}

/// Message routing information
#[derive(Debug, Clone)]
pub struct MessageRoute {
    pub source: PeerId,
    pub destination: Option<PeerId>,
    pub topic: Option<String>,
    pub hop_count: u8,
    pub timestamp: std::time::SystemTime,
}

/// Network event types
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerConnected {
        peer_id: PeerId,
        multiaddr: String,
    },
    PeerDisconnected {
        peer_id: PeerId,
        reason: String,
    },
    MessageReceived {
        topic: String,
        peer_id: PeerId,
        data: Vec<u8>,
    },
    SubscriptionChanged {
        topic: String,
        subscribed: bool,
    },
    DhtEvent {
        event_type: String,
        data: Vec<u8>,
    },
}