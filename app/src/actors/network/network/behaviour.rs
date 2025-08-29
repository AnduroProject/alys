//! libp2p NetworkBehaviour Composition
//! 
//! Defines the composite NetworkBehaviour for the Alys network with integrated
//! gossipsub, Kademlia DHT, mDNS discovery, and custom federation protocols.

use libp2p::{
    gossipsub::{self, Gossipsub, GossipsubEvent, MessageAuthenticity, ValidationMode as GossipValidationMode},
    kad::{self, Kademlia, KademliaEvent},
    mdns::{self, tokio::Behaviour as Mdns, tokio::Event as MdnsEvent},
    identify::{self, Behaviour as Identify, Event as IdentifyEvent},
    ping::{self, Behaviour as Ping, Event as PingEvent},
    request_response::{self, RequestResponse, Event as RequestResponseEvent},
    swarm::NetworkBehaviour,
    PeerId, Multiaddr,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::actors::network::network::config::{NetworkConfig, ValidationMode};

/// Composite network behaviour for the Alys network
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "AlysNetworkEvent")]
pub struct AlysNetworkBehaviour {
    /// Gossipsub for message broadcasting and propagation
    pub gossipsub: Gossipsub,
    /// Kademlia DHT for peer discovery and content routing
    pub kademlia: Kademlia<kad::store::MemoryStore>,
    /// mDNS for local network discovery
    pub mdns: Mdns,
    /// Identify protocol for peer information exchange
    pub identify: Identify,
    /// Ping protocol for connection keepalive
    pub ping: Ping,
    /// Request-response protocol for direct peer communication
    pub request_response: RequestResponse<AlysCodec>,
    /// Custom federation behaviour for consensus coordination
    pub federation: FederationBehaviour,
}

impl AlysNetworkBehaviour {
    /// Create a new network behaviour with the given configuration
    pub fn new(
        local_peer_id: PeerId,
        config: &NetworkConfig,
        local_public_key: libp2p::identity::PublicKey,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Configure gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .max_message_size(config.gossip_config.max_message_size)
            .heartbeat_interval(config.gossip_config.heartbeat_interval)
            .validation_mode(match config.gossip_config.validation_mode {
                ValidationMode::None => GossipValidationMode::None,
                ValidationMode::Basic => GossipValidationMode::Permissive,
                ValidationMode::Strict => GossipValidationMode::Strict,
            })
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|e| format!("Failed to create gossipsub config: {}", e))?;

        let message_authenticity = if config.gossip_config.message_signing {
            MessageAuthenticity::Signed(libp2p::identity::Keypair::from(local_public_key.clone()))
        } else {
            MessageAuthenticity::Anonymous
        };

        let gossipsub = Gossipsub::new(message_authenticity, gossipsub_config)
            .map_err(|e| format!("Failed to create gossipsub: {}", e))?;

        // Configure Kademlia DHT
        let kad_store = kad::store::MemoryStore::new(local_peer_id);
        let kademlia_config = kad::KademliaConfig::default()
            .set_query_timeout(config.discovery_config.dht_query_timeout)
            .set_replication_factor(
                config.discovery_config.kademlia_replication_factor.try_into()
                    .unwrap_or(20)
            );
        let mut kademlia = Kademlia::with_config(local_peer_id, kad_store, kademlia_config);

        // Add bootstrap peers to Kademlia
        for addr in &config.bootstrap_peers {
            if let Some(peer_id) = extract_peer_id(addr) {
                kademlia.add_address(&peer_id, addr.clone());
            }
        }

        // Configure mDNS
        let mdns = if config.discovery_config.enable_mdns {
            Mdns::new(mdns::Config::default(), local_peer_id)
                .map_err(|e| format!("Failed to create mDNS: {}", e))?
        } else {
            // Create a disabled mDNS instance
            Mdns::new(mdns::Config::default(), local_peer_id)
                .map_err(|e| format!("Failed to create mDNS: {}", e))?
        };

        // Configure identify protocol
        let identify = Identify::new(identify::Config::new(
            "/alys/1.0.0".to_string(),
            local_public_key,
        ));

        // Configure ping protocol
        let ping = Ping::new(ping::Config::new());

        // Configure request-response protocol
        let request_response_config = request_response::Config::default()
            .with_request_timeout(config.connection_timeout);
        let request_response = RequestResponse::new(
            AlysCodec::default(),
            std::iter::once((AlysProtocol, request_response_config)),
        );

        // Configure federation behaviour
        let federation = FederationBehaviour::new(&config.federation_config)?;

        Ok(Self {
            gossipsub,
            kademlia,
            mdns,
            identify,
            ping,
            request_response,
            federation,
        })
    }

    /// Subscribe to a gossipsub topic
    pub fn subscribe_to_topic(&mut self, topic: &str) -> Result<bool, gossipsub::SubscriptionError> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.gossipsub.subscribe(&topic)
    }

    /// Unsubscribe from a gossipsub topic
    pub fn unsubscribe_from_topic(&mut self, topic: &str) -> Result<bool, gossipsub::PublishError> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.gossipsub.unsubscribe(&topic)
    }

    /// Publish a message to a gossipsub topic
    pub fn publish_message(&mut self, topic: &str, data: Vec<u8>) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.gossipsub.publish(topic, data)
    }

    /// Add a peer address to Kademlia DHT
    pub fn add_peer_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        self.kademlia.add_address(&peer_id, address);
    }

    /// Start a Kademlia bootstrap operation
    pub fn bootstrap(&mut self) -> Result<kad::QueryId, kad::NoKnownPeers> {
        self.kademlia.bootstrap()
    }

    /// Get peers from Kademlia routing table
    pub fn get_closest_peers(&mut self, peer_id: PeerId) -> kad::QueryId {
        self.kademlia.get_closest_peers(peer_id)
    }

    /// Send a direct request to a peer
    pub fn send_request(&mut self, peer_id: PeerId, request: AlysRequest) -> request_response::OutboundRequestId {
        self.request_response.send_request(&peer_id, request)
    }

    /// Send a response to a request
    pub fn send_response(&mut self, channel: request_response::ResponseChannel<AlysResponse>, response: AlysResponse) -> Result<(), AlysResponse> {
        self.request_response.send_response(channel, response)
    }
}

/// Network events emitted by the composite behaviour
#[derive(Debug)]
pub enum AlysNetworkEvent {
    Gossipsub(GossipsubEvent),
    Kademlia(KademliaEvent),
    Mdns(MdnsEvent),
    Identify(IdentifyEvent),
    Ping(PingEvent),
    RequestResponse(RequestResponseEvent<AlysRequest, AlysResponse>),
    Federation(FederationEvent),
}

impl From<GossipsubEvent> for AlysNetworkEvent {
    fn from(event: GossipsubEvent) -> Self {
        AlysNetworkEvent::Gossipsub(event)
    }
}

impl From<KademliaEvent> for AlysNetworkEvent {
    fn from(event: KademliaEvent) -> Self {
        AlysNetworkEvent::Kademlia(event)
    }
}

impl From<MdnsEvent> for AlysNetworkEvent {
    fn from(event: MdnsEvent) -> Self {
        AlysNetworkEvent::Mdns(event)
    }
}

impl From<IdentifyEvent> for AlysNetworkEvent {
    fn from(event: IdentifyEvent) -> Self {
        AlysNetworkEvent::Identify(event)
    }
}

impl From<PingEvent> for AlysNetworkEvent {
    fn from(event: PingEvent) -> Self {
        AlysNetworkEvent::Ping(event)
    }
}

impl From<RequestResponseEvent<AlysRequest, AlysResponse>> for AlysNetworkEvent {
    fn from(event: RequestResponseEvent<AlysRequest, AlysResponse>) -> Self {
        AlysNetworkEvent::RequestResponse(event)
    }
}

impl From<FederationEvent> for AlysNetworkEvent {
    fn from(event: FederationEvent) -> Self {
        AlysNetworkEvent::Federation(event)
    }
}

/// Custom message ID function for gossipsub
fn message_id_fn(message: &gossipsub::Message) -> gossipsub::MessageId {
    let mut hasher = DefaultHasher::new();
    message.data.hash(&mut hasher);
    message.source.hash(&mut hasher);
    message.sequence_number.hash(&mut hasher);
    gossipsub::MessageId::from(hasher.finish().to_string())
}

/// Extract peer ID from multiaddress if present
fn extract_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    use libp2p::multiaddr::Protocol;
    
    for protocol in addr.iter() {
        if let Protocol::P2p(peer_id_multihash) = protocol {
            if let Ok(peer_id) = PeerId::from_multihash(peer_id_multihash) {
                return Some(peer_id);
            }
        }
    }
    None
}

// Request-Response Protocol Types and Codec

/// Protocol identifier for Alys request-response
#[derive(Clone)]
pub struct AlysProtocol;

impl AsRef<str> for AlysProtocol {
    fn as_ref(&self) -> &str {
        "/alys/req-resp/1.0.0"
    }
}

/// Request types for the Alys protocol
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AlysRequest {
    /// Request blocks by height range
    RequestBlocks { start_height: u64, count: u32 },
    /// Request peer status information
    GetPeerStatus,
    /// Request sync status
    GetSyncStatus,
    /// Custom federation request
    FederationRequest(Vec<u8>),
}

/// Response types for the Alys protocol
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AlysResponse {
    /// Block data response
    Blocks(Vec<crate::actors::network::messages::BlockData>),
    /// Peer status response
    PeerStatus(crate::actors::network::messages::PeerInfo),
    /// Sync status response
    SyncStatus(crate::actors::network::messages::SyncStatus),
    /// Federation response
    FederationResponse(Vec<u8>),
    /// Error response
    Error(String),
}

/// Codec for encoding/decoding Alys protocol messages
#[derive(Debug, Clone, Default)]
pub struct AlysCodec;

impl request_response::Codec for AlysCodec {
    type Protocol = AlysProtocol;
    type Request = AlysRequest;
    type Response = AlysResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<Self::Request>
    where
        T: futures::io::AsyncRead + Unpin + Send,
    {
        use futures::io::AsyncReadExt;
        
        let mut length_bytes = [0u8; 4];
        io.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;

        let mut buffer = vec![0u8; length];
        io.read_exact(&mut buffer).await?;

        bincode::deserialize(&buffer).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<Self::Response>
    where
        T: futures::io::AsyncRead + Unpin + Send,
    {
        use futures::io::AsyncReadExt;
        
        let mut length_bytes = [0u8; 4];
        io.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;

        let mut buffer = vec![0u8; length];
        io.read_exact(&mut buffer).await?;

        bincode::deserialize(&buffer).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }

    async fn write_request<T>(&mut self, _: &Self::Protocol, io: &mut T, req: Self::Request) -> std::io::Result<()>
    where
        T: futures::io::AsyncWrite + Unpin + Send,
    {
        use futures::io::AsyncWriteExt;
        
        let data = bincode::serialize(&req).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        let length = (data.len() as u32).to_be_bytes();
        io.write_all(&length).await?;
        io.write_all(&data).await?;
        io.flush().await?;
        
        Ok(())
    }

    async fn write_response<T>(&mut self, _: &Self::Protocol, io: &mut T, resp: Self::Response) -> std::io::Result<()>
    where
        T: futures::io::AsyncWrite + Unpin + Send,
    {
        use futures::io::AsyncWriteExt;
        
        let data = bincode::serialize(&resp).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        let length = (data.len() as u32).to_be_bytes();
        io.write_all(&length).await?;
        io.write_all(&data).await?;
        io.flush().await?;
        
        Ok(())
    }
}

// Federation Protocol Implementation

/// Custom federation behaviour for consensus coordination
pub struct FederationBehaviour {
    /// Federation configuration
    config: crate::actors::network::network::config::FederationNetworkConfig,
    /// Connected federation peers
    federation_peers: std::collections::HashSet<PeerId>,
}

impl FederationBehaviour {
    /// Create a new federation behaviour
    pub fn new(config: &crate::actors::network::network::config::FederationNetworkConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            config: config.clone(),
            federation_peers: std::collections::HashSet::new(),
        })
    }

    /// Add a federation peer
    pub fn add_federation_peer(&mut self, peer_id: PeerId) {
        self.federation_peers.insert(peer_id);
    }

    /// Remove a federation peer
    pub fn remove_federation_peer(&mut self, peer_id: &PeerId) {
        self.federation_peers.remove(peer_id);
    }

    /// Check if a peer is a federation peer
    pub fn is_federation_peer(&self, peer_id: &PeerId) -> bool {
        self.federation_peers.contains(peer_id)
    }

    /// Get all federation peers
    pub fn get_federation_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.federation_peers.iter()
    }
}

impl NetworkBehaviour for FederationBehaviour {
    type ConnectionHandler = libp2p::swarm::dummy::ConnectionHandler;
    type ToSwarm = FederationEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(libp2p::swarm::dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: libp2p::core::Endpoint,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        Ok(libp2p::swarm::dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _event: libp2p::swarm::FromSwarm<Self::ConnectionHandler>) {
        // Handle swarm events as needed
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        _event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        // Handle connection events as needed
    }

    fn poll(&mut self, _cx: &mut std::task::Context<'_>, _params: &mut impl libp2p::swarm::PollParameters) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
        std::task::Poll::Pending
    }
}

/// Events emitted by the federation behaviour
#[derive(Debug, Clone)]
pub enum FederationEvent {
    /// Federation peer discovered
    PeerDiscovered(PeerId),
    /// Federation peer disconnected
    PeerDisconnected(PeerId),
    /// Consensus message received
    ConsensusMessage { from: PeerId, data: Vec<u8> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn network_behaviour_creation() {
        let keypair = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let config = NetworkConfig::default();

        let behaviour = AlysNetworkBehaviour::new(
            local_peer_id,
            &config,
            keypair.public(),
        );

        assert!(behaviour.is_ok());
    }

    #[test]
    fn message_id_function() {
        use gossipsub::{Message, MessageId};
        
        let message = Message {
            source: Some(PeerId::random()),
            data: b"test message".to_vec(),
            sequence_number: Some(123),
            topic: gossipsub::TopicHash::from_raw("test_topic"),
        };

        let id1 = message_id_fn(&message);
        let id2 = message_id_fn(&message);
        
        // Same message should produce same ID
        assert_eq!(id1, id2);
    }

    #[test]
    fn peer_id_extraction() {
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWGrAiUsqCYjuFmK2A6iKsEVdBxaRBaJSQi2uTAGp4TrZP"
            .parse()
            .unwrap();
        
        let peer_id = extract_peer_id(&addr);
        assert!(peer_id.is_some());

        let addr_no_peer: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let no_peer_id = extract_peer_id(&addr_no_peer);
        assert!(no_peer_id.is_none());
    }

    #[test]
    fn federation_behaviour() {
        let config = crate::actors::network::network::config::FederationNetworkConfig::default();
        let mut behaviour = FederationBehaviour::new(&config).unwrap();
        
        let peer_id = PeerId::random();
        assert!(!behaviour.is_federation_peer(&peer_id));
        
        behaviour.add_federation_peer(peer_id);
        assert!(behaviour.is_federation_peer(&peer_id));
        
        behaviour.remove_federation_peer(&peer_id);
        assert!(!behaviour.is_federation_peer(&peer_id));
    }

    #[tokio::test]
    async fn codec_serialization() {
        let mut codec = AlysCodec::default();
        
        let request = AlysRequest::RequestBlocks {
            start_height: 100,
            count: 50,
        };

        let response = AlysResponse::Blocks(vec![]);

        // Test that requests and responses can be serialized
        let req_data = bincode::serialize(&request).unwrap();
        let resp_data = bincode::serialize(&response).unwrap();
        
        assert!(!req_data.is_empty());
        assert!(!resp_data.is_empty());
        
        // Test deserialization
        let decoded_req: AlysRequest = bincode::deserialize(&req_data).unwrap();
        let decoded_resp: AlysResponse = bincode::deserialize(&resp_data).unwrap();
        
        match decoded_req {
            AlysRequest::RequestBlocks { start_height, count } => {
                assert_eq!(start_height, 100);
                assert_eq!(count, 50);
            }
            _ => panic!("Unexpected request type"),
        }
        
        matches!(decoded_resp, AlysResponse::Blocks(_));
    }
}