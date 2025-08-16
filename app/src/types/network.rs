//! Network-related types and structures

use crate::types::*;
use serde::{Deserialize, Serialize};

/// Peer connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConnection {
    pub peer_id: PeerId,
    pub multiaddr: String,
    pub direction: ConnectionDirection,
    pub connected_at: std::time::SystemTime,
    pub protocols: Vec<String>,
    pub reputation: PeerReputation,
}

/// Direction of peer connection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionDirection {
    Inbound,
    Outbound,
}

/// Peer reputation and scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReputation {
    pub score: i32,
    pub last_interaction: std::time::SystemTime,
    pub successful_interactions: u64,
    pub failed_interactions: u64,
    pub violations: Vec<ReputationViolation>,
}

/// Reputation violation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationViolation {
    pub violation_type: ViolationType,
    pub timestamp: std::time::SystemTime,
    pub severity: u8,
    pub description: String,
}

/// Types of reputation violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    InvalidMessage,
    Spam,
    BadBehavior,
    ProtocolViolation,
    Timeout,
    Disconnect,
    Malicious,
}

/// Connection quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionQuality {
    pub latency_ms: u64,
    pub bandwidth_kbps: u64,
    pub reliability_score: f64,
    pub packet_loss_rate: f64,
}

/// Network message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    pub message_id: String,
    pub topic: String,
    pub sender: PeerId,
    pub timestamp: std::time::SystemTime,
    pub payload: MessagePayload,
    pub signature: Option<MessageSignature>,
}

/// Message payload types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    Block(ConsensusBlock),
    Transaction(Transaction),
    BlockRequest(BlockRequest),
    BlockResponse(BlockResponse),
    PeerStatus(PeerStatus),
    Ping(PingMessage),
    Pong(PongMessage),
    Custom { data: Vec<u8> },
}

/// Message signature for authenticity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSignature {
    pub signature: Signature,
    pub public_key: PublicKey,
}

/// Block request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRequest {
    pub start_block: u64,
    pub count: u64,
    pub skip: u64,
    pub reverse: bool,
}

/// Block response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockResponse {
    pub request_id: String,
    pub blocks: Vec<ConsensusBlock>,
    pub complete: bool,
}

/// Peer status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatus {
    pub best_block: BlockRef,
    pub genesis_hash: BlockHash,
    pub chain_id: u64,
    pub protocol_version: u32,
    pub client_version: String,
    pub capabilities: Vec<String>,
}

/// Ping message for connection health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingMessage {
    pub nonce: u64,
    pub timestamp: std::time::SystemTime,
}

/// Pong response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongMessage {
    pub nonce: u64,
    pub timestamp: std::time::SystemTime,
}

/// Network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub connected_peers: u32,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connections_established: u64,
    pub connections_dropped: u64,
    pub invalid_messages: u64,
}

/// Topic subscription info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicSubscription {
    pub topic: String,
    pub subscriber_count: u32,
    pub message_rate: f64,
    pub last_message: Option<std::time::SystemTime>,
}

/// Gossip message propagation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipInfo {
    pub message_id: String,
    pub origin_peer: PeerId,
    pub hop_count: u8,
    pub seen_peers: Vec<PeerId>,
    pub propagation_time: std::time::Duration,
}

/// Network discovery state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryState {
    Idle,
    Discovering {
        target_peers: usize,
        found_peers: usize,
        started_at: std::time::SystemTime,
    },
    Complete {
        peers_found: usize,
        duration: std::time::Duration,
    },
}

/// DHT (Distributed Hash Table) related types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtRecord {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub publisher: PeerId,
    pub ttl: std::time::Duration,
    pub created_at: std::time::SystemTime,
}

/// DHT query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtQueryResult {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
    pub closest_peers: Vec<PeerId>,
    pub query_duration: std::time::Duration,
}

/// Network event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    PeerConnected {
        peer_id: PeerId,
        address: String,
        direction: ConnectionDirection,
    },
    PeerDisconnected {
        peer_id: PeerId,
        reason: DisconnectionReason,
    },
    MessageReceived {
        from: PeerId,
        topic: String,
        message: NetworkMessage,
    },
    MessageSent {
        to: Option<PeerId>,
        topic: String,
        message_id: String,
    },
    TopicSubscribed {
        topic: String,
    },
    TopicUnsubscribed {
        topic: String,
    },
    ReputationUpdated {
        peer_id: PeerId,
        old_score: i32,
        new_score: i32,
    },
}

/// Reasons for peer disconnection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectionReason {
    UserInitiated,
    RemoteDisconnected,
    Timeout,
    ProtocolError { error: String },
    ReputationTooLow,
    ResourceLimits,
    NetworkError { error: String },
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub messages_per_second: u32,
    pub bytes_per_second: u64,
    pub burst_allowance: u32,
}

/// Bandwidth usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthUsage {
    pub upload_bytes_per_second: f64,
    pub download_bytes_per_second: f64,
    pub peak_upload: u64,
    pub peak_download: u64,
    pub total_uploaded: u64,
    pub total_downloaded: u64,
}

impl PeerConnection {
    /// Create a new peer connection
    pub fn new(
        peer_id: PeerId,
        multiaddr: String,
        direction: ConnectionDirection,
    ) -> Self {
        Self {
            peer_id,
            multiaddr,
            direction,
            connected_at: std::time::SystemTime::now(),
            protocols: vec!["alys/1.0.0".to_string()],
            reputation: PeerReputation::new(),
        }
    }
    
    /// Get connection duration
    pub fn connection_duration(&self) -> std::time::Duration {
        std::time::SystemTime::now()
            .duration_since(self.connected_at)
            .unwrap_or_default()
    }
    
    /// Check if peer supports a protocol
    pub fn supports_protocol(&self, protocol: &str) -> bool {
        self.protocols.iter().any(|p| p == protocol)
    }
    
    /// Update reputation score
    pub fn update_reputation(&mut self, delta: i32, reason: &str) {
        self.reputation.score += delta;
        self.reputation.last_interaction = std::time::SystemTime::now();
        
        if delta >= 0 {
            self.reputation.successful_interactions += 1;
        } else {
            self.reputation.failed_interactions += 1;
            
            // Add violation if significant negative score
            if delta < -10 {
                self.reputation.violations.push(ReputationViolation {
                    violation_type: ViolationType::BadBehavior,
                    timestamp: std::time::SystemTime::now(),
                    severity: (-delta as u8).min(255),
                    description: reason.to_string(),
                });
            }
        }
    }
    
    /// Check if peer should be banned
    pub fn should_ban(&self) -> bool {
        self.reputation.score < -100 || self.reputation.violations.len() > 10
    }
}

impl PeerReputation {
    /// Create new peer reputation
    pub fn new() -> Self {
        Self {
            score: 0,
            last_interaction: std::time::SystemTime::now(),
            successful_interactions: 0,
            failed_interactions: 0,
            violations: Vec::new(),
        }
    }
    
    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_interactions + self.failed_interactions;
        if total == 0 {
            1.0
        } else {
            self.successful_interactions as f64 / total as f64
        }
    }
    
    /// Check if peer is trustworthy
    pub fn is_trustworthy(&self) -> bool {
        self.score > 50 && self.success_rate() > 0.8
    }
    
    /// Decay reputation over time
    pub fn decay(&mut self, factor: f64) {
        self.score = ((self.score as f64) * factor) as i32;
        
        // Remove old violations (older than 1 hour)
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        self.violations.retain(|v| v.timestamp > cutoff);
    }
}

impl NetworkMessage {
    /// Create a new network message
    pub fn new(topic: String, sender: PeerId, payload: MessagePayload) -> Self {
        let message_id = format!("{}_{}", sender, std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis());
            
        Self {
            message_id,
            topic,
            sender,
            timestamp: std::time::SystemTime::now(),
            payload,
            signature: None,
        }
    }
    
    /// Get message size estimate
    pub fn size_estimate(&self) -> usize {
        match &self.payload {
            MessagePayload::Block(block) => {
                // Rough estimate based on transaction count
                1000 + block.transactions.len() * 200
            }
            MessagePayload::Transaction(_) => 200,
            MessagePayload::BlockRequest(_) => 50,
            MessagePayload::BlockResponse(resp) => {
                1000 + resp.blocks.len() * 1000
            }
            MessagePayload::PeerStatus(_) => 100,
            MessagePayload::Ping(_) => 20,
            MessagePayload::Pong(_) => 20,
            MessagePayload::Custom { data } => data.len() + 50,
        }
    }
    
    /// Check if message is expired
    pub fn is_expired(&self, ttl: std::time::Duration) -> bool {
        std::time::SystemTime::now()
            .duration_since(self.timestamp)
            .unwrap_or_default() > ttl
    }
}

impl ConnectionQuality {
    /// Create new connection quality metrics
    pub fn new() -> Self {
        Self {
            latency_ms: 0,
            bandwidth_kbps: 0,
            reliability_score: 1.0,
            packet_loss_rate: 0.0,
        }
    }
    
    /// Update latency measurement
    pub fn update_latency(&mut self, new_latency: std::time::Duration) {
        let new_latency_ms = new_latency.as_millis() as u64;
        
        // Exponential moving average
        if self.latency_ms == 0 {
            self.latency_ms = new_latency_ms;
        } else {
            self.latency_ms = (self.latency_ms * 7 + new_latency_ms) / 8;
        }
    }
    
    /// Update bandwidth measurement
    pub fn update_bandwidth(&mut self, bytes_transferred: u64, duration: std::time::Duration) {
        let kbps = (bytes_transferred * 8) / (duration.as_secs().max(1) * 1000);
        
        // Exponential moving average
        if self.bandwidth_kbps == 0 {
            self.bandwidth_kbps = kbps;
        } else {
            self.bandwidth_kbps = (self.bandwidth_kbps * 7 + kbps) / 8;
        }
    }
    
    /// Get overall connection score
    pub fn connection_score(&self) -> f64 {
        let latency_score = if self.latency_ms < 50 {
            1.0
        } else if self.latency_ms < 200 {
            0.8
        } else {
            0.5
        };
        
        let bandwidth_score = if self.bandwidth_kbps > 1000 {
            1.0
        } else if self.bandwidth_kbps > 100 {
            0.8
        } else {
            0.5
        };
        
        let loss_score = 1.0 - self.packet_loss_rate;
        
        (latency_score + bandwidth_score + loss_score + self.reliability_score) / 4.0
    }
}

impl Default for ConnectionQuality {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkStats {
    /// Create new network statistics
    pub fn new() -> Self {
        Self {
            connected_peers: 0,
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            connections_established: 0,
            connections_dropped: 0,
            invalid_messages: 0,
        }
    }
    
    /// Get message success rate
    pub fn message_success_rate(&self) -> f64 {
        let total_messages = self.messages_sent + self.messages_received;
        if total_messages == 0 {
            1.0
        } else {
            1.0 - (self.invalid_messages as f64 / total_messages as f64)
        }
    }
    
    /// Get connection stability
    pub fn connection_stability(&self) -> f64 {
        if self.connections_established == 0 {
            1.0
        } else {
            1.0 - (self.connections_dropped as f64 / self.connections_established as f64)
        }
    }
}