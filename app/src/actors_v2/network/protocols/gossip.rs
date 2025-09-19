//! Gossip Protocol V2
//!
//! Simplified gossipsub implementation for NetworkActor V2.
//! TCP transport only, essential topics for block/transaction broadcasting.

use libp2p::gossipsub::{Topic, IdentTopic};
use serde::{Serialize, Deserialize};

/// Essential gossip topics for V2 system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GossipTopic {
    Blocks,
    Transactions,
    PeerAnnouncements,
}

impl GossipTopic {
    /// Convert to libp2p topic
    pub fn to_topic(&self) -> IdentTopic {
        match self {
            GossipTopic::Blocks => IdentTopic::new("alys-blocks"),
            GossipTopic::Transactions => IdentTopic::new("alys-transactions"),
            GossipTopic::PeerAnnouncements => IdentTopic::new("alys-peers"),
        }
    }

    /// Get topic string
    pub fn as_str(&self) -> &'static str {
        match self {
            GossipTopic::Blocks => "alys-blocks",
            GossipTopic::Transactions => "alys-transactions",
            GossipTopic::PeerAnnouncements => "alys-peers",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "alys-blocks" => Some(GossipTopic::Blocks),
            "alys-transactions" => Some(GossipTopic::Transactions),
            "alys-peers" => Some(GossipTopic::PeerAnnouncements),
            _ => None,
        }
    }

    /// Get all essential topics
    pub fn all_topics() -> Vec<Self> {
        vec![
            GossipTopic::Blocks,
            GossipTopic::Transactions,
            GossipTopic::PeerAnnouncements,
        ]
    }
}

/// Gossip message wrapper for V2 system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessageV2 {
    pub topic: GossipTopic,
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub message_id: String,
}

impl GossipMessageV2 {
    pub fn new(topic: GossipTopic, data: Vec<u8>) -> Self {
        Self {
            topic,
            data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            message_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Validate message size and content
    pub fn is_valid(&self) -> bool {
        // Basic validation
        !self.data.is_empty() && self.data.len() <= 10 * 1024 * 1024 // 10MB max
    }

    /// Get message age in seconds
    pub fn age_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp)
    }
}