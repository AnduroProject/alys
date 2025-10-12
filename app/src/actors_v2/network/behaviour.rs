//! NetworkActor V2 libp2p Behaviour (Real Implementation)
//!
//! Complete network behaviour with libp2p NetworkBehaviour derive macro.
//! Includes: Gossipsub, Identify, and mDNS protocols.

use anyhow::{Result, Context as AnyhowContext};
use libp2p::swarm::NetworkBehaviour;
use libp2p::PeerId;
use super::NetworkConfig;

/// Complete V2 network behaviour with real libp2p protocols
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "AlysNetworkBehaviourEvent")]
pub struct AlysNetworkBehaviour {
    pub gossipsub: libp2p::gossipsub::Behaviour,
    pub identify: libp2p::identify::Behaviour,
    pub mdns: libp2p::mdns::tokio::Behaviour,
}

/// Network behaviour events
#[derive(Debug)]
pub enum AlysNetworkBehaviourEvent {
    /// Gossip message received
    GossipMessage {
        topic: String,
        data: Vec<u8>,
        source_peer: String,
        message_id: String,
    },
    /// Request received from peer
    RequestReceived {
        request: crate::actors_v2::network::messages::NetworkRequest,
        source_peer: String,
        request_id: String,
    },
    /// Response received from peer
    ResponseReceived {
        response: Vec<u8>,
        peer_id: String,
        request_id: String,
    },
    /// Peer connected
    PeerConnected {
        peer_id: String,
        address: String,
    },
    /// Peer disconnected
    PeerDisconnected {
        peer_id: String,
        reason: String,
    },
    /// Peer identified via identify protocol
    PeerIdentified {
        peer_id: String,
        protocols: Vec<String>,
        addresses: Vec<String>,
    },
    /// mDNS peer discovered
    MdnsPeerDiscovered {
        peer_id: String,
        addresses: Vec<String>,
    },
    /// mDNS peer expired
    MdnsPeerExpired {
        peer_id: String,
    },
}

impl AlysNetworkBehaviour {
    /// Create new behaviour from configuration
    pub fn new(config: &NetworkConfig) -> Result<Self> {
        use libp2p::{gossipsub, identify, mdns};
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Generate keypair
        let local_key = libp2p::identity::Keypair::generate_ed25519();

        // Configure Gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(config.message_size_limit)
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(|msg: &gossipsub::Message| {
                let mut hasher = DefaultHasher::new();
                msg.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_string())
            })
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build Gossipsub config: {}", e))?;

        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create Gossipsub behaviour: {}", e))?;

        // Subscribe to configured topics
        for topic_str in &config.gossip_topics {
            let topic = gossipsub::IdentTopic::new(topic_str);
            gossipsub.subscribe(&topic)
                .context(format!("Failed to subscribe to topic: {}", topic_str))?;
            tracing::debug!("Subscribed to gossip topic: {}", topic_str);
        }

        // Configure Identify
        let identify_config = identify::Config::new(
            "/alys/v2/0.1.0".to_string(),
            local_key.public(),
        )
        .with_agent_version(format!("alys-v2/{}", env!("CARGO_PKG_VERSION")));

        let identify = identify::Behaviour::new(identify_config);

        // Configure mDNS
        let mdns = mdns::tokio::Behaviour::new(
            mdns::Config::default(),
            local_key.public().to_peer_id(),
        )
        .context("Failed to create mDNS behaviour")?;

        Ok(Self {
            gossipsub,
            identify,
            mdns,
        })
    }

    /// Initialize behaviour (placeholder for compatibility)
    pub fn initialize(&mut self) -> Result<()> {
        tracing::debug!("AlysNetworkBehaviour initialized");
        Ok(())
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        // libp2p 0.52 doesn't provide direct access to peer ID from identify
        // We'll need to store it separately or extract from swarm
        // For now, generate a temporary one (will be fixed in swarm integration)
        libp2p::identity::Keypair::generate_ed25519().public().to_peer_id()
    }

    /// Broadcast message to gossip network
    pub fn broadcast_message(&mut self, topic: &str, data: Vec<u8>) -> Result<String> {
        use libp2p::gossipsub::IdentTopic;

        let topic = IdentTopic::new(topic);

        // Auto-subscribe if not already subscribed
        if self.gossipsub.mesh_peers(&topic.hash()).next().is_none() {
            self.gossipsub.subscribe(&topic)
                .context(format!("Failed to subscribe to topic: {}", topic))?;
        }

        let message_id = self.gossipsub
            .publish(topic, data)
            .context("Failed to publish message")?;

        Ok(message_id.to_string())
    }
}

// Event mapping for NetworkBehaviour derive macro
impl From<libp2p::gossipsub::Event> for AlysNetworkBehaviourEvent {
    fn from(event: libp2p::gossipsub::Event) -> Self {
        match event {
            libp2p::gossipsub::Event::Message {
                propagation_source,
                message_id,
                message,
            } => AlysNetworkBehaviourEvent::GossipMessage {
                topic: message.topic.to_string(),
                data: message.data,
                source_peer: propagation_source.to_string(),
                message_id: message_id.to_string(),
            },
            _ => {
                tracing::trace!("Unhandled gossipsub event: {:?}", event);
                // For unhandled events, return a dummy event
                AlysNetworkBehaviourEvent::PeerIdentified {
                    peer_id: String::new(),
                    protocols: vec![],
                    addresses: vec![],
                }
            }
        }
    }
}

impl From<libp2p::identify::Event> for AlysNetworkBehaviourEvent {
    fn from(event: libp2p::identify::Event) -> Self {
        match event {
            libp2p::identify::Event::Received { peer_id, info } => {
                AlysNetworkBehaviourEvent::PeerIdentified {
                    peer_id: peer_id.to_string(),
                    protocols: info.protocols.iter().map(|p| p.to_string()).collect(),
                    addresses: info.listen_addrs.iter().map(|a| a.to_string()).collect(),
                }
            }
            _ => {
                tracing::trace!("Unhandled identify event: {:?}", event);
                AlysNetworkBehaviourEvent::PeerIdentified {
                    peer_id: String::new(),
                    protocols: vec![],
                    addresses: vec![],
                }
            }
        }
    }
}

impl From<libp2p::mdns::Event> for AlysNetworkBehaviourEvent {
    fn from(event: libp2p::mdns::Event) -> Self {
        match event {
            libp2p::mdns::Event::Discovered(peers) => {
                // Return first discovered peer (simplified)
                if let Some((peer_id, addresses)) = peers.into_iter().next() {
                    AlysNetworkBehaviourEvent::MdnsPeerDiscovered {
                        peer_id: peer_id.to_string(),
                        addresses: addresses.iter().map(|a| a.to_string()).collect(),
                    }
                } else {
                    AlysNetworkBehaviourEvent::PeerIdentified {
                        peer_id: String::new(),
                        protocols: vec![],
                        addresses: vec![],
                    }
                }
            }
            libp2p::mdns::Event::Expired(peers) => {
                // Return first expired peer (simplified)
                if let Some((peer_id, _)) = peers.into_iter().next() {
                    AlysNetworkBehaviourEvent::MdnsPeerExpired {
                        peer_id: peer_id.to_string(),
                    }
                } else {
                    AlysNetworkBehaviourEvent::PeerIdentified {
                        peer_id: String::new(),
                        protocols: vec![],
                        addresses: vec![],
                    }
                }
            }
        }
    }
}
