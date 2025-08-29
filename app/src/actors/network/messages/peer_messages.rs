//! PeerActor Message Protocol
//! 
//! Defines all messages for peer management operations including connection
//! establishment, peer scoring, and discovery coordination.

use actix::{Message, Result as ActorResult};
use serde::{Deserialize, Serialize};
use libp2p::{PeerId, Multiaddr};
use crate::actors::network::messages::{NetworkMessage, NetworkResult};

/// Connect to a specific peer
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<ConnectionResponse>>")]
pub struct ConnectToPeer {
    pub peer_id: Option<PeerId>,
    pub address: Multiaddr,
    pub priority: ConnectionPriority,
    pub timeout_ms: u64,
}

impl NetworkMessage for ConnectToPeer {}

/// Disconnect from a specific peer
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct DisconnectFromPeer {
    pub peer_id: PeerId,
    pub reason: String,
    pub graceful: bool,
}

impl NetworkMessage for DisconnectFromPeer {}

/// Get peer connection status
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<PeerStatus>>")]
pub struct GetPeerStatus {
    pub peer_id: Option<PeerId>, // None = all peers
}

impl NetworkMessage for GetPeerStatus {}

/// Update peer performance score
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct UpdatePeerScore {
    pub peer_id: PeerId,
    pub score_update: ScoreUpdate,
}

impl NetworkMessage for UpdatePeerScore {}

/// Get best peers for a specific operation
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<Vec<PeerInfo>>>")]
pub struct GetBestPeers {
    pub count: u32,
    pub operation_type: OperationType,
    pub exclude_peers: Vec<PeerId>,
}

impl NetworkMessage for GetBestPeers {}

/// Start peer discovery
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<DiscoveryResponse>>")]
pub struct StartDiscovery {
    pub discovery_type: DiscoveryType,
    pub target_peer_count: Option<u32>,
}

impl NetworkMessage for StartDiscovery {}

/// Stop peer discovery
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<NetworkResult<()>>")]
pub struct StopDiscovery {
    pub discovery_type: DiscoveryType,
}

impl NetworkMessage for StopDiscovery {}

/// Connection priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ConnectionPriority {
    Critical,  // Federation peers
    High,      // Bootstrap and seed peers
    Normal,    // Regular discovered peers
    Low,       // Background discovery
}

/// Peer performance score update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreUpdate {
    pub latency_ms: Option<u64>,
    pub throughput_bytes_sec: Option<u64>,
    pub success_rate: Option<f64>, // 0.0 to 1.0
    pub protocol_violation: bool,
    pub byzantine_behavior: bool,
}

/// Operation types for peer selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    BlockSync,
    Transaction,
    Federation,
    Discovery,
}

/// Peer discovery types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryType {
    MDNS,
    Kademlia,
    Bootstrap,
    All,
}

/// Connection establishment response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionResponse {
    pub peer_id: PeerId,
    pub connected: bool,
    pub connection_time_ms: u64,
    pub protocols: Vec<String>,
    pub error_message: Option<String>,
}

/// Comprehensive peer status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatus {
    pub peers: Vec<PeerInfo>,
    pub total_peers: u32,
    pub federation_peers: u32,
    pub connection_stats: ConnectionStats,
}

/// Individual peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
    pub connection_status: ConnectionStatus,
    pub protocols: Vec<String>,
    pub peer_type: PeerType,
    pub score: PeerScore,
    pub connection_time: Option<std::time::SystemTime>,
    pub last_seen: std::time::SystemTime,
    pub statistics: PeerStatistics,
}

/// Peer connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed,
    Banned,
}

/// Peer classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerType {
    Federation,     // Consensus authority
    Miner,         // Mining pool or solo miner
    Regular,       // Standard node
    Bootstrap,     // Bootstrap/seed node
    Unknown,       // Classification pending
}

/// Peer performance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerScore {
    pub overall_score: f64,      // 0.0 to 100.0
    pub latency_score: f64,      // Lower is better
    pub throughput_score: f64,   // Higher is better
    pub reliability_score: f64,  // Higher is better
    pub federation_bonus: f64,   // Additional score for federation peers
    pub last_updated: std::time::SystemTime,
}

/// Peer performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatistics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub average_latency_ms: f64,
    pub success_rate: f64,
    pub last_activity: std::time::SystemTime,
    pub connection_uptime: std::time::Duration,
}

/// Overall connection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub active_connections: u32,
    pub pending_connections: u32,
    pub failed_connections: u32,
    pub total_bandwidth_in: u64,
    pub total_bandwidth_out: u64,
    pub average_connection_time_ms: f64,
}

/// Discovery operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    pub discovery_id: String,
    pub discovery_type: DiscoveryType,
    pub started_at: std::time::SystemTime,
    pub initial_peer_count: u32,
}

// Peer management events
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct PeerDiscovered {
    pub peer_id: PeerId,
    pub address: Multiaddr,
    pub discovery_method: DiscoveryType,
}

impl NetworkMessage for PeerDiscovered {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct PeerBanned {
    pub peer_id: PeerId,
    pub reason: String,
    pub duration: std::time::Duration,
}

impl NetworkMessage for PeerBanned {}

#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "ActorResult<()>")]
pub struct PeerReputationChanged {
    pub peer_id: PeerId,
    pub old_score: f64,
    pub new_score: f64,
    pub reason: String,
}

impl NetworkMessage for PeerReputationChanged {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_priority_ordering() {
        let priorities = vec![
            ConnectionPriority::Critical,
            ConnectionPriority::High,
            ConnectionPriority::Normal,
            ConnectionPriority::Low,
        ];
        
        // Test that we can create and compare priorities
        assert_ne!(priorities[0], priorities[1]);
        assert_ne!(priorities[1], priorities[2]);
    }

    #[test]
    fn peer_score_calculation() {
        let score = PeerScore {
            overall_score: 85.0,
            latency_score: 20.0,     // Lower is better
            throughput_score: 95.0,  // Higher is better
            reliability_score: 90.0, // Higher is better
            federation_bonus: 10.0,  // Bonus for federation peers
            last_updated: std::time::SystemTime::now(),
        };
        
        assert_eq!(score.overall_score, 85.0);
        assert_eq!(score.federation_bonus, 10.0);
    }

    #[test]
    fn peer_type_classification() {
        let peer_info = PeerInfo {
            peer_id: PeerId::random(),
            addresses: vec![],
            connection_status: ConnectionStatus::Connected,
            protocols: vec!["sync".to_string()],
            peer_type: PeerType::Federation,
            score: PeerScore {
                overall_score: 100.0,
                latency_score: 10.0,
                throughput_score: 100.0,
                reliability_score: 100.0,
                federation_bonus: 20.0,
                last_updated: std::time::SystemTime::now(),
            },
            connection_time: Some(std::time::SystemTime::now()),
            last_seen: std::time::SystemTime::now(),
            statistics: PeerStatistics {
                messages_sent: 100,
                messages_received: 150,
                bytes_sent: 50000,
                bytes_received: 75000,
                average_latency_ms: 25.0,
                success_rate: 0.98,
                last_activity: std::time::SystemTime::now(),
                connection_uptime: std::time::Duration::from_secs(3600),
            },
        };
        
        matches!(peer_info.peer_type, PeerType::Federation);
        assert_eq!(peer_info.score.federation_bonus, 20.0);
    }
}