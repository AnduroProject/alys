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
    AuxPow, // Phase 4: AuxPoW mining coordination
    // Tendermint consensus topics
    TendermintProposals,
    TendermintVotes,
    TendermintTimeouts,
    TendermintEvidence,
    TendermintNewRound,
}

impl GossipTopic {
    /// Convert to libp2p topic
    pub fn to_topic(&self) -> IdentTopic {
        match self {
            GossipTopic::Blocks => IdentTopic::new("alys-blocks"),
            GossipTopic::Transactions => IdentTopic::new("alys-transactions"),
            GossipTopic::PeerAnnouncements => IdentTopic::new("alys-peers"),
            GossipTopic::AuxPow => IdentTopic::new("alys-auxpow"),
            // Tendermint topics
            GossipTopic::TendermintProposals => IdentTopic::new("alys-tendermint-proposals"),
            GossipTopic::TendermintVotes => IdentTopic::new("alys-tendermint-votes"),
            GossipTopic::TendermintTimeouts => IdentTopic::new("alys-tendermint-timeouts"),
            GossipTopic::TendermintEvidence => IdentTopic::new("alys-tendermint-evidence"),
            GossipTopic::TendermintNewRound => IdentTopic::new("alys-tendermint-newround"),
        }
    }

    /// Get topic string
    pub fn as_str(&self) -> &'static str {
        match self {
            GossipTopic::Blocks => "alys-blocks",
            GossipTopic::Transactions => "alys-transactions",
            GossipTopic::PeerAnnouncements => "alys-peers",
            GossipTopic::AuxPow => "alys-auxpow",
            // Tendermint topics
            GossipTopic::TendermintProposals => "alys-tendermint-proposals",
            GossipTopic::TendermintVotes => "alys-tendermint-votes",
            GossipTopic::TendermintTimeouts => "alys-tendermint-timeouts",
            GossipTopic::TendermintEvidence => "alys-tendermint-evidence",
            GossipTopic::TendermintNewRound => "alys-tendermint-newround",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "alys-blocks" => Some(GossipTopic::Blocks),
            "alys-transactions" => Some(GossipTopic::Transactions),
            "alys-peers" => Some(GossipTopic::PeerAnnouncements),
            "alys-auxpow" => Some(GossipTopic::AuxPow),
            // Tendermint topics
            "alys-tendermint-proposals" => Some(GossipTopic::TendermintProposals),
            "alys-tendermint-votes" => Some(GossipTopic::TendermintVotes),
            "alys-tendermint-timeouts" => Some(GossipTopic::TendermintTimeouts),
            "alys-tendermint-evidence" => Some(GossipTopic::TendermintEvidence),
            "alys-tendermint-newround" => Some(GossipTopic::TendermintNewRound),
            _ => None,
        }
    }

    /// Get all essential topics (non-Tendermint)
    pub fn all_topics() -> Vec<Self> {
        vec![
            GossipTopic::Blocks,
            GossipTopic::Transactions,
            GossipTopic::PeerAnnouncements,
            GossipTopic::AuxPow,
        ]
    }

    /// Get all Tendermint consensus topics
    pub fn tendermint_topics() -> Vec<Self> {
        vec![
            GossipTopic::TendermintProposals,
            GossipTopic::TendermintVotes,
            GossipTopic::TendermintTimeouts,
            GossipTopic::TendermintEvidence,
            GossipTopic::TendermintNewRound,
        ]
    }

    /// Get all topics including Tendermint
    pub fn all_topics_with_tendermint() -> Vec<Self> {
        let mut topics = Self::all_topics();
        topics.extend(Self::tendermint_topics());
        topics
    }

    /// Check if this is a Tendermint consensus topic
    pub fn is_tendermint(&self) -> bool {
        matches!(
            self,
            GossipTopic::TendermintProposals
                | GossipTopic::TendermintVotes
                | GossipTopic::TendermintTimeouts
                | GossipTopic::TendermintEvidence
                | GossipTopic::TendermintNewRound
        )
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