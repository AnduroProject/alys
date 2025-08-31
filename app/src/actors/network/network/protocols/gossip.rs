//! Gossipsub Protocol Implementation
//! 
//! Federation-aware gossipsub protocol for efficient block and transaction
//! propagation with deduplication, validation, and priority routing.

use libp2p::{
    gossipsub::{
        self, Gossipsub, GossipsubEvent, GossipsubConfigBuilder, MessageAuthenticity,
        ValidationMode, MessageId, TopicHash, Topic, GossipsubMessage,
    },
    identity::Keypair,
    PeerId,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use sha2::{Sha256, Digest};

/// Alys-specific gossipsub configuration and management
pub struct AlysGossipsub {
    /// Core gossipsub behaviour
    gossipsub: Gossipsub,
    /// Topic subscriptions with metadata
    subscriptions: HashMap<TopicHash, TopicInfo>,
    /// Message cache for deduplication
    message_cache: HashMap<MessageId, CachedMessage>,
    /// Federation peer priorities
    federation_peers: HashSet<PeerId>,
    /// Message validation rules
    validation_config: ValidationConfig,
    /// Performance metrics
    metrics: GossipMetrics,
}

impl AlysGossipsub {
    /// Create a new Alys gossipsub instance
    pub fn new(
        keypair: &Keypair, 
        federation_peers: HashSet<PeerId>,
        validation_config: ValidationConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Configure gossipsub for Alys blockchain requirements
        let gossipsub_config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_millis(700)) // Faster than default for blockchain
            .validation_mode(ValidationMode::Strict)
            .message_id_fn(alys_message_id_fn) // Custom message ID for deduplication
            .max_transmit_size(1024 * 1024) // 1MB max for large blocks
            .duplicate_cache_time(Duration::from_secs(60))
            .history_length(6) // Keep 6 rounds of history
            .history_gossip(3) // Gossip to 3 peers per round
            .mesh_n(8) // Target 8 peers in mesh
            .mesh_n_low(4) // Min 4 peers in mesh
            .mesh_n_high(12) // Max 12 peers in mesh
            .mesh_outbound_min(2) // At least 2 outbound connections
            .flood_publish(false) // Use mesh, not flood
            .build()
            .map_err(|e| format!("Failed to build gossipsub config: {}", e))?;

        let mut gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        ).map_err(|e| format!("Failed to create gossipsub: {}", e))?;

        // Subscribe to essential Alys topics
        let default_topics = vec![
            "alys/blocks/v1",
            "alys/transactions/v1", 
            "alys/discovery/v1",
        ];

        let mut subscriptions = HashMap::new();
        for topic_str in default_topics {
            let topic = Topic::new(topic_str);
            let topic_hash = topic.hash();
            
            gossipsub.subscribe(&topic)
                .map_err(|e| format!("Failed to subscribe to {}: {}", topic_str, e))?;
            
            subscriptions.insert(topic_hash, TopicInfo {
                topic: topic_str.to_string(),
                subscribed_at: Instant::now(),
                message_count: 0,
                last_message: None,
                priority: if topic_str.contains("blocks") { MessagePriority::High } else { MessagePriority::Normal },
            });
        }

        // Subscribe to federation topics if we have federation peers
        if !federation_peers.is_empty() {
            let federation_topics = vec![
                "alys/federation/consensus/v1",
                "alys/federation/blocks/v1",
                "alys/federation/emergency/v1",
            ];

            for topic_str in federation_topics {
                let topic = Topic::new(topic_str);
                let topic_hash = topic.hash();
                
                gossipsub.subscribe(&topic)
                    .map_err(|e| format!("Failed to subscribe to federation topic {}: {}", topic_str, e))?;
                
                subscriptions.insert(topic_hash, TopicInfo {
                    topic: topic_str.to_string(),
                    subscribed_at: Instant::now(),
                    message_count: 0,
                    last_message: None,
                    priority: MessagePriority::Critical, // Federation messages are critical
                });
            }
        }

        Ok(Self {
            gossipsub,
            subscriptions,
            message_cache: HashMap::new(),
            federation_peers,
            validation_config,
            metrics: GossipMetrics::default(),
        })
    }

    /// Publish a message to a topic with priority handling
    pub fn publish(
        &mut self, 
        topic: &str, 
        data: Vec<u8>,
        priority: MessagePriority,
    ) -> Result<MessageId, libp2p::gossipsub::PublishError> {
        let topic = Topic::new(topic);
        let topic_hash = topic.hash();

        // Apply message validation before publishing
        if !self.validate_outgoing_message(topic.as_str(), &data, priority) {
            return Err(libp2p::gossipsub::PublishError::InsufficientPeers);
        }

        // Publish the message
        let message_id = self.gossipsub.publish(topic, data.clone())?;

        // Cache the message for deduplication and metrics
        self.cache_message(message_id, data, topic_hash, priority);
        
        // Update metrics
        self.metrics.messages_published += 1;
        self.metrics.bytes_published += data.len() as u64;

        // Update topic info
        if let Some(topic_info) = self.subscriptions.get_mut(&topic_hash) {
            topic_info.message_count += 1;
            topic_info.last_message = Some(Instant::now());
        }

        Ok(message_id)
    }

    /// Subscribe to a new topic
    pub fn subscribe(&mut self, topic: &str) -> Result<bool, libp2p::gossipsub::SubscriptionError> {
        let topic_obj = Topic::new(topic);
        let topic_hash = topic_obj.hash();

        let result = self.gossipsub.subscribe(&topic_obj)?;

        if result {
            // Determine priority based on topic
            let priority = match topic {
                t if t.contains("federation") => MessagePriority::Critical,
                t if t.contains("blocks") => MessagePriority::High,
                t if t.contains("emergency") => MessagePriority::Critical,
                _ => MessagePriority::Normal,
            };

            self.subscriptions.insert(topic_hash, TopicInfo {
                topic: topic.to_string(),
                subscribed_at: Instant::now(),
                message_count: 0,
                last_message: None,
                priority,
            });

            tracing::info!("Subscribed to gossipsub topic: {} (priority: {:?})", topic, priority);
        }

        Ok(result)
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &str) -> Result<bool, libp2p::gossipsub::PublishError> {
        let topic_obj = Topic::new(topic);
        let topic_hash = topic_obj.hash();

        let result = self.gossipsub.unsubscribe(&topic_obj);
        
        if result.is_ok() {
            self.subscriptions.remove(&topic_hash);
            tracing::info!("Unsubscribed from gossipsub topic: {}", topic);
        }

        result.map(|_| true)
    }

    /// Process incoming gossipsub event
    pub fn handle_event(&mut self, event: GossipsubEvent) -> Vec<AlysGossipEvent> {
        let mut alys_events = Vec::new();

        match event {
            GossipsubEvent::Message { 
                propagation_source, 
                message_id, 
                message 
            } => {
                // Update metrics
                self.metrics.messages_received += 1;
                self.metrics.bytes_received += message.data.len() as u64;

                // Check for duplicates
                if self.is_duplicate_message(&message_id) {
                    self.metrics.duplicate_messages += 1;
                    return alys_events; // Skip duplicates
                }

                // Get topic info and priority
                let topic_info = self.subscriptions.get(&message.topic).cloned();
                let priority = topic_info.as_ref()
                    .map(|info| info.priority)
                    .unwrap_or(MessagePriority::Normal);

                // Validate the message
                let validation_result = self.validate_incoming_message(&message, &propagation_source);
                
                if validation_result.is_valid {
                    // Cache the valid message
                    self.cache_message(message_id, message.data.clone(), message.topic, priority);

                    // Update topic statistics
                    if let Some(topic_info) = self.subscriptions.get_mut(&message.topic) {
                        topic_info.message_count += 1;
                        topic_info.last_message = Some(Instant::now());
                    }

                    // Create Alys-specific event
                    alys_events.push(AlysGossipEvent::MessageReceived {
                        message_id,
                        topic: message.topic,
                        data: message.data,
                        source: propagation_source,
                        priority,
                        validation_time: validation_result.processing_time,
                        is_federation_message: self.federation_peers.contains(&propagation_source),
                    });
                } else {
                    tracing::warn!(
                        "Invalid message {} from {}: {}",
                        message_id, propagation_source, validation_result.reason
                    );
                    self.metrics.invalid_messages += 1;
                }
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                tracing::debug!("Peer {} subscribed to topic {:?}", peer_id, topic);
                alys_events.push(AlysGossipEvent::PeerSubscribed { peer_id, topic });
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                tracing::debug!("Peer {} unsubscribed from topic {:?}", peer_id, topic);
                alys_events.push(AlysGossipEvent::PeerUnsubscribed { peer_id, topic });
            }
            GossipsubEvent::GossipsubNotSupported { peer_id } => {
                tracing::warn!("Peer {} does not support gossipsub", peer_id);
                alys_events.push(AlysGossipEvent::ProtocolNotSupported { peer_id });
            }
        }

        alys_events
    }

    /// Add a federation peer for priority handling
    pub fn add_federation_peer(&mut self, peer_id: PeerId) {
        self.federation_peers.insert(peer_id);
        tracing::info!("Added federation peer: {}", peer_id);
    }

    /// Remove a federation peer
    pub fn remove_federation_peer(&mut self, peer_id: &PeerId) {
        self.federation_peers.remove(peer_id);
        tracing::info!("Removed federation peer: {}", peer_id);
    }

    /// Get current gossipsub metrics
    pub fn metrics(&self) -> &GossipMetrics {
        &self.metrics
    }

    /// Clean up old cached messages
    pub fn cleanup_cache(&mut self) {
        let now = Instant::now();
        let cache_ttl = Duration::from_secs(300); // 5 minutes

        self.message_cache.retain(|_, cached_msg| {
            now.duration_since(cached_msg.received_at) < cache_ttl
        });
    }

    // Private helper methods
    
    fn validate_outgoing_message(&self, topic: &str, data: &[u8], priority: MessagePriority) -> bool {
        // Size limits based on priority
        let max_size = match priority {
            MessagePriority::Critical => 2 * 1024 * 1024, // 2MB for critical federation messages
            MessagePriority::High => 1024 * 1024, // 1MB for blocks
            MessagePriority::Normal => 256 * 1024, // 256KB for transactions
        };

        if data.len() > max_size {
            tracing::warn!(
                "Message too large for topic {}: {} bytes > {} bytes",
                topic, data.len(), max_size
            );
            return false;
        }

        // Topic-specific validation
        match topic {
            t if t.contains("blocks") => self.validate_block_message(data),
            t if t.contains("transactions") => self.validate_transaction_message(data),
            t if t.contains("federation") => self.validate_federation_message(data),
            _ => true, // Allow other messages
        }
    }

    fn validate_incoming_message(&self, message: &GossipsubMessage, source: &PeerId) -> ValidationResult {
        let start_time = Instant::now();
        
        // Basic validation
        if message.data.is_empty() {
            return ValidationResult {
                is_valid: false,
                reason: "Empty message".to_string(),
                processing_time: start_time.elapsed(),
            };
        }

        // Federation peer messages get expedited validation
        if self.federation_peers.contains(source) {
            return ValidationResult {
                is_valid: true,
                reason: "Federation peer - trusted".to_string(),
                processing_time: start_time.elapsed(),
            };
        }

        // Apply validation rules based on configuration
        let is_valid = match &self.validation_config.mode {
            ValidationMode::Strict => self.strict_message_validation(&message.data),
            ValidationMode::Permissive => self.permissive_message_validation(&message.data),
            _ => true,
        };

        ValidationResult {
            is_valid,
            reason: if is_valid { "Valid".to_string() } else { "Failed validation".to_string() },
            processing_time: start_time.elapsed(),
        }
    }

    fn validate_block_message(&self, data: &[u8]) -> bool {
        // Basic block message validation
        data.len() >= 32 && data.len() <= 1024 * 1024 // Between 32 bytes and 1MB
    }

    fn validate_transaction_message(&self, data: &[u8]) -> bool {
        // Basic transaction message validation
        data.len() >= 20 && data.len() <= 256 * 1024 // Between 20 bytes and 256KB
    }

    fn validate_federation_message(&self, data: &[u8]) -> bool {
        // Federation messages have more flexible size requirements
        data.len() >= 8 && data.len() <= 2 * 1024 * 1024 // Between 8 bytes and 2MB
    }

    fn strict_message_validation(&self, _data: &[u8]) -> bool {
        // Implement strict validation rules
        // Would include signature verification, format validation, etc.
        true // Placeholder
    }

    fn permissive_message_validation(&self, _data: &[u8]) -> bool {
        // Implement permissive validation rules
        true // Placeholder
    }

    fn is_duplicate_message(&self, message_id: &MessageId) -> bool {
        self.message_cache.contains_key(message_id)
    }

    fn cache_message(&mut self, message_id: MessageId, data: Vec<u8>, topic: TopicHash, priority: MessagePriority) {
        self.message_cache.insert(message_id, CachedMessage {
            data,
            topic,
            priority,
            received_at: Instant::now(),
        });
    }
}

/// Custom message ID function for Alys gossipsub
fn alys_message_id_fn(message: &GossipsubMessage) -> MessageId {
    let mut hasher = Sha256::new();
    hasher.update(&message.data);
    hasher.update(message.topic.as_str().as_bytes());
    
    MessageId::from(hasher.finalize().as_slice())
}

// Supporting types and structures

#[derive(Debug, Clone)]
pub struct TopicInfo {
    pub topic: String,
    pub subscribed_at: Instant,
    pub message_count: u64,
    pub last_message: Option<Instant>,
    pub priority: MessagePriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagePriority {
    Normal,
    High,      // For blocks
    Critical,  // For federation messages
}

#[derive(Debug)]
pub struct CachedMessage {
    pub data: Vec<u8>,
    pub topic: TopicHash,
    pub priority: MessagePriority,
    pub received_at: Instant,
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub reason: String,
    pub processing_time: Duration,
}

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub mode: ValidationMode,
    pub max_message_size: usize,
    pub allow_empty_messages: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            mode: ValidationMode::Strict,
            max_message_size: 1024 * 1024, // 1MB
            allow_empty_messages: false,
        }
    }
}

#[derive(Default)]
pub struct GossipMetrics {
    pub messages_published: u64,
    pub messages_received: u64,
    pub bytes_published: u64,
    pub bytes_received: u64,
    pub duplicate_messages: u64,
    pub invalid_messages: u64,
}

#[derive(Debug)]
pub enum AlysGossipEvent {
    MessageReceived {
        message_id: MessageId,
        topic: TopicHash,
        data: Vec<u8>,
        source: PeerId,
        priority: MessagePriority,
        validation_time: Duration,
        is_federation_message: bool,
    },
    PeerSubscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
    PeerUnsubscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
    ProtocolNotSupported {
        peer_id: PeerId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn test_alys_gossipsub_creation() {
        let keypair = Keypair::generate_ed25519();
        let federation_peers = HashSet::new();
        let validation_config = ValidationConfig::default();

        let gossipsub = AlysGossipsub::new(&keypair, federation_peers, validation_config);
        assert!(gossipsub.is_ok());
    }

    #[test]
    fn test_message_validation() {
        let keypair = Keypair::generate_ed25519();
        let federation_peers = HashSet::new();
        let validation_config = ValidationConfig::default();
        let gossipsub = AlysGossipsub::new(&keypair, federation_peers, validation_config).unwrap();

        // Test block message validation
        let valid_block = vec![0u8; 1000]; // 1KB block
        assert!(gossipsub.validate_block_message(&valid_block));

        let invalid_block = vec![0u8; 10]; // Too small
        assert!(!gossipsub.validate_block_message(&invalid_block));
    }

    #[test]
    fn test_custom_message_id() {
        use libp2p::gossipsub::{Topic, TopicHash};
        
        let topic = Topic::new("test");
        let message = GossipsubMessage {
            source: None,
            data: b"test message".to_vec(),
            sequence_number: None,
            topic: topic.hash(),
        };

        let id1 = alys_message_id_fn(&message);
        let id2 = alys_message_id_fn(&message);
        
        // Same message should produce same ID
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_priority_assignment() {
        let keypair = Keypair::generate_ed25519();
        let federation_peers = HashSet::new();
        let validation_config = ValidationConfig::default();
        let mut gossipsub = AlysGossipsub::new(&keypair, federation_peers, validation_config).unwrap();

        // Test subscription with priority assignment
        assert!(gossipsub.subscribe("alys/blocks/v1").unwrap());
        assert!(gossipsub.subscribe("alys/federation/consensus/v1").unwrap());

        let blocks_topic_hash = Topic::new("alys/blocks/v1").hash();
        let federation_topic_hash = Topic::new("alys/federation/consensus/v1").hash();

        assert_eq!(gossipsub.subscriptions[&blocks_topic_hash].priority, MessagePriority::High);
        assert_eq!(gossipsub.subscriptions[&federation_topic_hash].priority, MessagePriority::Critical);
    }
}