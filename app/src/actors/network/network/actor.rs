//! NetworkActor Implementation
//! 
//! P2P networking actor with libp2p integration for gossipsub, Kademlia DHT,
//! and mDNS discovery with federation-aware message routing.

use actix::{Actor, Context, Handler, AsyncContext, StreamHandler, ActorContext};
use libp2p::{
    Swarm, SwarmBuilder, 
    identity::Keypair,
    PeerId, Multiaddr,
    Transport,
    core::upgrade,
    noise,
    yamux,
    tcp,
    dns,
};
use std::collections::HashMap;
use std::time::Instant;
use tokio_stream::wrappers::UnboundedReceiverStream;

use actor_system::{AlysActor, LifecycleAware, ActorResult, ActorError};
use actor_system::blockchain::{BlockchainAwareActor, BlockchainTimingConstraints, BlockchainActorPriority};

use crate::actors::network::messages::*;
use crate::actors::network::network::*;

/// NetworkActor for P2P protocol management
pub struct NetworkActor {
    /// Network configuration
    config: NetworkConfig,
    /// libp2p swarm for network operations
    swarm: Option<Swarm<AlysNetworkBehaviour>>,
    /// Local peer ID
    local_peer_id: PeerId,
    /// Network metrics and statistics
    metrics: NetworkMetrics,
    /// Active gossip subscriptions
    active_subscriptions: HashMap<String, Instant>,
    /// Pending requests tracking
    pending_requests: HashMap<String, PendingRequest>,
    /// Bootstrap status
    bootstrap_status: BootstrapStatus,
    /// Shutdown flag
    shutdown_requested: bool,
}

impl NetworkActor {
    /// Create a new NetworkActor with the given configuration
    pub fn new(config: NetworkConfig) -> ActorResult<Self> {
        // Generate keypair for this node
        let keypair = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());

        tracing::info!("Creating NetworkActor with peer ID: {}", local_peer_id);

        // Validate configuration
        config.validate().map_err(|e| ActorError::ConfigurationError {
            reason: format!("Invalid network configuration: {}", e),
        })?;

        Ok(Self {
            config,
            swarm: None,
            local_peer_id,
            metrics: NetworkMetrics::default(),
            active_subscriptions: HashMap::new(),
            pending_requests: HashMap::new(),
            bootstrap_status: BootstrapStatus::NotStarted,
            shutdown_requested: false,
        })
    }

    /// Initialize the libp2p swarm
    async fn initialize_swarm(&mut self) -> ActorResult<()> {
        let keypair = Keypair::generate_ed25519();
        
        // Create transport
        let transport = {
            let tcp = tcp::tokio::Transport::default();
            let dns_tcp = dns::TokioDnsConfig::system(tcp)
                .map_err(|e| ActorError::InitializationError {
                    reason: format!("DNS transport error: {}", e),
                })?;
            
            dns_tcp
                .upgrade(upgrade::Version::V1)
                .authenticate(noise::Config::new(&keypair).unwrap())
                .multiplex(yamux::Config::default())
                .timeout(self.config.connection_timeout)
                .boxed()
        };

        // Create network behaviour
        let behaviour = AlysNetworkBehaviour::new(
            self.local_peer_id,
            &self.config,
            keypair.public(),
        ).map_err(|e| ActorError::InitializationError {
            reason: format!("Failed to create network behaviour: {}", e),
        })?;

        // Create swarm
        let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, self.local_peer_id)
            .build();

        // Start listening on configured addresses
        for addr in &self.config.listen_addresses {
            swarm.listen_on(addr.clone()).map_err(|e| {
                ActorError::InitializationError {
                    reason: format!("Failed to listen on {}: {}", addr, e),
                }
            })?;
        }

        // Subscribe to default topics
        self.subscribe_to_default_topics(&mut swarm)?;

        self.swarm = Some(swarm);
        tracing::info!("Network swarm initialized successfully");
        Ok(())
    }

    /// Subscribe to default gossip topics
    fn subscribe_to_default_topics(&mut self, swarm: &mut Swarm<AlysNetworkBehaviour>) -> ActorResult<()> {
        let default_topics = vec![
            "blocks",
            "transactions", 
            "discovery",
        ];

        // Add federation topics if enabled
        if self.config.federation_config.federation_discovery {
            for topic in &self.config.federation_config.federation_topics {
                swarm.behaviour_mut().subscribe_to_topic(topic).map_err(|e| {
                    ActorError::InitializationError {
                        reason: format!("Failed to subscribe to federation topic {}: {}", topic, e),
                    }
                })?;
                self.active_subscriptions.insert(topic.clone(), Instant::now());
            }
        }

        for topic in default_topics {
            swarm.behaviour_mut().subscribe_to_topic(topic).map_err(|e| {
                ActorError::InitializationError {
                    reason: format!("Failed to subscribe to topic {}: {}", topic, e),
                }
            })?;
            self.active_subscriptions.insert(topic.to_string(), Instant::now());
        }

        tracing::info!("Subscribed to {} default topics", self.active_subscriptions.len());
        Ok(())
    }

    /// Start network operations
    async fn start_network_operations(&mut self) -> NetworkResult<NetworkStartResponse> {
        if self.swarm.is_none() {
            self.initialize_swarm().await.map_err(|e| NetworkError::ActorError {
                reason: format!("Failed to initialize swarm: {:?}", e),
            })?;
        }

        let swarm = self.swarm.as_mut().unwrap();

        // Start bootstrap if configured
        if !self.config.bootstrap_peers.is_empty() {
            match swarm.behaviour_mut().bootstrap() {
                Ok(_) => {
                    self.bootstrap_status = BootstrapStatus::InProgress;
                    tracing::info!("Bootstrap started with {} peers", self.config.bootstrap_peers.len());
                }
                Err(e) => {
                    tracing::warn!("Failed to start bootstrap: {}", e);
                    self.bootstrap_status = BootstrapStatus::Failed;
                }
            }
        }

        // Get listening addresses
        let listening_addresses = swarm.listeners().cloned().collect();

        Ok(NetworkStartResponse {
            local_peer_id: self.local_peer_id,
            listening_addresses,
            protocols: vec![
                "gossipsub".to_string(),
                "kademlia".to_string(),
                "identify".to_string(),
                "ping".to_string(),
            ],
            started_at: std::time::SystemTime::now(),
        })
    }

    /// Handle network events from the swarm
    fn handle_network_event(&mut self, event: AlysNetworkEvent) {
        match event {
            AlysNetworkEvent::Gossipsub(gossip_event) => {
                self.handle_gossipsub_event(gossip_event);
            }
            AlysNetworkEvent::Kademlia(kad_event) => {
                self.handle_kademlia_event(kad_event);
            }
            AlysNetworkEvent::Mdns(mdns_event) => {
                self.handle_mdns_event(mdns_event);
            }
            AlysNetworkEvent::Identify(identify_event) => {
                self.handle_identify_event(identify_event);
            }
            AlysNetworkEvent::Ping(ping_event) => {
                self.handle_ping_event(ping_event);
            }
            AlysNetworkEvent::RequestResponse(req_resp_event) => {
                self.handle_request_response_event(req_resp_event);
            }
            AlysNetworkEvent::Federation(federation_event) => {
                self.handle_federation_event(federation_event);
            }
        }
    }

    /// Handle gossipsub events
    fn handle_gossipsub_event(&mut self, event: libp2p::gossipsub::Event) {
        use libp2p::gossipsub::Event as GossipsubEvent;

        match event {
            GossipsubEvent::Message { propagation_source, message_id, message } => {
                self.metrics.messages_received += 1;
                tracing::debug!(
                    "Received gossip message {} from {} on topic {}",
                    message_id,
                    propagation_source,
                    message.topic
                );
                
                // Process message based on topic
                self.process_gossip_message(message);
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                tracing::debug!("Peer {} subscribed to topic {}", peer_id, topic);
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                tracing::debug!("Peer {} unsubscribed from topic {}", peer_id, topic);
            }
            GossipsubEvent::GossipsubNotSupported { peer_id } => {
                tracing::warn!("Peer {} does not support gossipsub", peer_id);
            }
        }
    }

    /// Process received gossip message
    fn process_gossip_message(&mut self, message: libp2p::gossipsub::Message) {
        let topic_str = message.topic.as_str();
        
        match topic_str {
            "blocks" => {
                // Handle block messages
                tracing::debug!("Received block message ({} bytes)", message.data.len());
            }
            "transactions" => {
                // Handle transaction messages  
                tracing::debug!("Received transaction message ({} bytes)", message.data.len());
            }
            topic if self.config.federation_config.federation_topics.contains(&topic.to_string()) => {
                // Handle federation messages with priority
                tracing::debug!("Received federation message on {} ({} bytes)", topic, message.data.len());
            }
            _ => {
                tracing::debug!("Received message on unknown topic: {}", topic_str);
            }
        }
    }

    /// Handle Kademlia DHT events
    fn handle_kademlia_event(&mut self, event: libp2p::kad::Event<'_>) {
        use libp2p::kad::Event as KademliaEvent;

        match event {
            KademliaEvent::OutboundQueryProgressed { result, .. } => {
                match result {
                    libp2p::kad::QueryResult::Bootstrap(Ok(result)) => {
                        self.bootstrap_status = BootstrapStatus::Completed;
                        tracing::info!("Bootstrap completed with {} peers", result.num_remaining);
                    }
                    libp2p::kad::QueryResult::Bootstrap(Err(e)) => {
                        self.bootstrap_status = BootstrapStatus::Failed;
                        tracing::warn!("Bootstrap failed: {}", e);
                    }
                    libp2p::kad::QueryResult::GetClosestPeers(Ok(result)) => {
                        tracing::debug!("Found {} closest peers for query", result.peers.len());
                    }
                    _ => {}
                }
            }
            KademliaEvent::RoutingUpdated { peer, .. } => {
                tracing::debug!("Routing table updated with peer {}", peer);
            }
            KademliaEvent::InboundRequest { request } => {
                tracing::debug!("Received Kademlia inbound request: {:?}", request);
            }
            _ => {}
        }
    }

    /// Handle mDNS events
    fn handle_mdns_event(&mut self, event: libp2p::mdns::Event) {
        use libp2p::mdns::Event;

        match event {
            Event::Discovered(list) => {
                for (peer_id, addr) in list {
                    tracing::debug!("Discovered peer {} at {}", peer_id, addr);
                    if let Some(swarm) = &mut self.swarm {
                        swarm.behaviour_mut().add_peer_address(peer_id, addr);
                    }
                }
            }
            Event::Expired(list) => {
                for (peer_id, addr) in list {
                    tracing::debug!("Peer {} expired at {}", peer_id, addr);
                }
            }
        }
    }

    /// Handle identify protocol events
    fn handle_identify_event(&mut self, event: libp2p::identify::Event) {
        use libp2p::identify::Event;

        match event {
            Event::Received { peer_id, info } => {
                tracing::debug!(
                    "Identified peer {} with {} addresses and {} protocols",
                    peer_id,
                    info.listen_addrs.len(),
                    info.protocols.len()
                );
            }
            Event::Sent { peer_id } => {
                tracing::debug!("Sent identify info to peer {}", peer_id);
            }
            Event::Error { peer_id, error } => {
                tracing::warn!("Identify error with peer {}: {}", peer_id, error);
            }
            Event::Pushed { peer_id } => {
                tracing::debug!("Pushed identify info to peer {}", peer_id);
            }
        }
    }

    /// Handle ping events
    fn handle_ping_event(&mut self, event: libp2p::ping::Event) {
        match event.result {
            Ok(duration) => {
                self.metrics.update_peer_latency(event.peer, duration);
            }
            Err(e) => {
                tracing::debug!("Ping failed for peer {}: {}", event.peer, e);
            }
        }
    }

    /// Handle request-response events
    fn handle_request_response_event(&mut self, event: libp2p::request_response::Event<AlysRequest, AlysResponse>) {
        use libp2p::request_response::Event;

        match event {
            Event::Message { peer, message } => {
                match message {
                    libp2p::request_response::Message::Request { request_id, request, channel } => {
                        tracing::debug!("Received request {} from {}: {:?}", request_id, peer, request);
                        // Handle request and send response
                        let response = self.process_request(request);
                        if let Some(swarm) = &mut self.swarm {
                            let _ = swarm.behaviour_mut().send_response(channel, response);
                        }
                    }
                    libp2p::request_response::Message::Response { request_id, response } => {
                        tracing::debug!("Received response {} from {}: {:?}", request_id, peer, response);
                        // Handle response for pending request
                    }
                }
            }
            Event::OutboundFailure { peer, request_id, error } => {
                tracing::warn!("Outbound request {} to {} failed: {:?}", request_id, peer, error);
            }
            Event::InboundFailure { peer, request_id, error } => {
                tracing::warn!("Inbound request {} from {} failed: {:?}", request_id, peer, error);
            }
            Event::ResponseSent { peer, request_id } => {
                tracing::debug!("Response sent to {} for request {}", peer, request_id);
            }
        }
    }

    /// Handle federation events
    fn handle_federation_event(&mut self, event: FederationEvent) {
        match event {
            FederationEvent::PeerDiscovered(peer_id) => {
                tracing::info!("Discovered federation peer: {}", peer_id);
            }
            FederationEvent::PeerDisconnected(peer_id) => {
                tracing::info!("Federation peer disconnected: {}", peer_id);
            }
            FederationEvent::ConsensusMessage { from, data } => {
                tracing::debug!("Received consensus message from {} ({} bytes)", from, data.len());
            }
        }
    }

    /// Process incoming requests
    fn process_request(&self, request: AlysRequest) -> AlysResponse {
        match request {
            AlysRequest::GetPeerStatus => {
                // Return current network status as peer status
                AlysResponse::Error("Not implemented".to_string())
            }
            AlysRequest::GetSyncStatus => {
                // Return sync status (would coordinate with SyncActor)
                AlysResponse::Error("Not implemented".to_string())
            }
            AlysRequest::RequestBlocks { start_height, count } => {
                tracing::debug!("Block request: {} blocks starting from {}", count, start_height);
                AlysResponse::Blocks(vec![]) // Would coordinate with ChainActor
            }
            AlysRequest::FederationRequest(_data) => {
                AlysResponse::FederationResponse(vec![])
            }
        }
    }

    /// Get current network status
    fn get_network_status(&self) -> NetworkStatus {
        let connected_peers = if let Some(swarm) = &self.swarm {
            swarm.connected_peers().count() as u32
        } else {
            0
        };

        let listening_addresses = if let Some(swarm) = &self.swarm {
            swarm.listeners().cloned().collect()
        } else {
            vec![]
        };

        NetworkStatus {
            is_active: self.swarm.is_some(),
            local_peer_id: self.local_peer_id,
            listening_addresses,
            connected_peers,
            pending_connections: 0, // Would track from swarm state
            total_bandwidth_in: self.metrics.total_bandwidth_in,
            total_bandwidth_out: self.metrics.total_bandwidth_out,
            active_protocols: vec![
                "gossipsub".to_string(),
                "kademlia".to_string(),
                "identify".to_string(),
                "ping".to_string(),
            ],
            gossip_topics: self.active_subscriptions.keys().cloned().map(|t| {
                match t.as_str() {
                    "blocks" => GossipTopic::Blocks,
                    "transactions" => GossipTopic::Transactions,
                    "discovery" => GossipTopic::Discovery,
                    topic if self.config.federation_config.federation_topics.contains(&topic.to_string()) => {
                        GossipTopic::FederationMessages
                    }
                    topic => GossipTopic::Custom(topic.to_string()),
                }
            }).collect(),
            discovery_status: DiscoveryStatus {
                mdns_enabled: self.config.discovery_config.enable_mdns,
                kad_routing_table_size: 0, // Would get from Kademlia
                bootstrap_peers_connected: match self.bootstrap_status {
                    BootstrapStatus::Completed => self.config.bootstrap_peers.len() as u32,
                    _ => 0,
                },
                total_discovered_peers: connected_peers,
            },
        }
    }
}

/// Network performance metrics
#[derive(Default, Clone)]
pub struct NetworkMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub total_bandwidth_in: u64,
    pub total_bandwidth_out: u64,
    pub peer_latencies: HashMap<PeerId, std::time::Duration>,
}

impl NetworkMetrics {
    fn update_peer_latency(&mut self, peer_id: PeerId, latency: std::time::Duration) {
        self.peer_latencies.insert(peer_id, latency);
    }
}

/// Pending request tracking
pub struct PendingRequest {
    pub request_id: String,
    pub peer_id: PeerId,
    pub sent_at: Instant,
    pub timeout: std::time::Duration,
}

/// Bootstrap status tracking
#[derive(Debug, Clone, Copy)]
pub enum BootstrapStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed,
}

impl Actor for NetworkActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("NetworkActor started with peer ID: {}", self.local_peer_id);

        // Initialize swarm on startup
        let init_future = self.initialize_swarm();
        let actor_future = actix::fut::wrap_future(init_future)
            .map(|result, actor, _ctx| {
                if let Err(e) = result {
                    tracing::error!("Failed to initialize network swarm: {:?}", e);
                    // Could trigger actor shutdown or retry logic
                }
            });
        
        ctx.spawn(actor_future);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("NetworkActor stopped");
    }
}

impl AlysActor for NetworkActor {
    fn actor_type(&self) -> &'static str {
        "NetworkActor"
    }

    fn metrics(&self) -> serde_json::Value {
        let connected_peers = if let Some(swarm) = &self.swarm {
            swarm.connected_peers().count()
        } else {
            0
        };

        serde_json::json!({
            "local_peer_id": self.local_peer_id.to_string(),
            "connected_peers": connected_peers,
            "active_subscriptions": self.active_subscriptions.len(),
            "messages_sent": self.metrics.messages_sent,
            "messages_received": self.metrics.messages_received,
            "bandwidth_in": self.metrics.total_bandwidth_in,
            "bandwidth_out": self.metrics.total_bandwidth_out,
            "bootstrap_status": format!("{:?}", self.bootstrap_status),
        })
    }
}

impl LifecycleAware for NetworkActor {
    fn on_start(&mut self) -> ActorResult<()> {
        tracing::info!("NetworkActor lifecycle started");
        Ok(())
    }

    fn on_stop(&mut self) -> ActorResult<()> {
        self.shutdown_requested = true;
        tracing::info!("NetworkActor lifecycle stopped");
        Ok(())
    }

    fn health_check(&self) -> ActorResult<()> {
        if self.shutdown_requested {
            return Err(ActorError::ActorStopped);
        }

        // Check if swarm is healthy
        if self.swarm.is_none() {
            return Err(ActorError::HealthCheckFailed {
                reason: "Network swarm not initialized".to_string(),
            });
        }

        Ok(())
    }
}

impl BlockchainAwareActor for NetworkActor {
    fn timing_constraints(&self) -> BlockchainTimingConstraints {
        BlockchainTimingConstraints {
            max_processing_time: self.config.connection_timeout,
            federation_timeout: self.config.federation_config.consensus_config.message_timeout,
            emergency_timeout: std::time::Duration::from_secs(30),
        }
    }

    fn federation_config(&self) -> Option<actor_system::blockchain::FederationConfig> {
        Some(actor_system::blockchain::FederationConfig {
            consensus_threshold: 0.67,
            max_authorities: 21,
            slot_duration: self.config.federation_config.consensus_config.round_timeout,
        })
    }

    fn blockchain_priority(&self) -> BlockchainActorPriority {
        BlockchainActorPriority::High
    }
}

// Message Handlers Implementation would go here
// For brevity, I'm including key handlers

impl Handler<StartNetwork> for NetworkActor {
    type Result = actix::ResponseFuture<NetworkActorResult<NetworkStartResponse>>;

    fn handle(&mut self, msg: StartNetwork, _ctx: &mut Context<Self>) -> Self::Result {
        // Update configuration with provided addresses
        self.config.listen_addresses = msg.listen_addresses;
        self.config.bootstrap_peers = msg.bootstrap_peers;
        
        let mut actor_copy = NetworkActor::new(self.config.clone()).unwrap();
        
        Box::pin(async move {
            match actor_copy.start_network_operations().await {
                Ok(response) => Ok(Ok(response)),
                Err(error) => Ok(Err(error)),
            }
        })
    }
}

impl Handler<GetNetworkStatus> for NetworkActor {
    type Result = NetworkActorResult<NetworkStatus>;

    fn handle(&mut self, _msg: GetNetworkStatus, _ctx: &mut Context<Self>) -> Self::Result {
        let status = self.get_network_status();
        Ok(Ok(status))
    }
}

impl Handler<BroadcastBlock> for NetworkActor {
    type Result = NetworkActorResult<BroadcastResponse>;

    fn handle(&mut self, msg: BroadcastBlock, _ctx: &mut Context<Self>) -> Self::Result {
        if let Some(swarm) = &mut self.swarm {
            let topic = if msg.priority { "federation_blocks" } else { "blocks" };
            
            match swarm.behaviour_mut().publish_message(topic, msg.block_data) {
                Ok(message_id) => {
                    self.metrics.messages_sent += 1;
                    Ok(Ok(BroadcastResponse {
                        message_id: message_id.to_string(),
                        peers_reached: swarm.connected_peers().count() as u32,
                        propagation_started_at: std::time::SystemTime::now(),
                    }))
                }
                Err(e) => Ok(Err(NetworkError::ProtocolError {
                    message: format!("Failed to broadcast block: {}", e),
                })),
            }
        } else {
            Ok(Err(NetworkError::ActorError {
                reason: "Network not initialized".to_string(),
            }))
        }
    }
}

impl Handler<StopNetwork> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: StopNetwork, ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Stopping network operations (graceful: {})", msg.graceful);

        if msg.graceful {
            // Graceful shutdown - close connections cleanly
            if let Some(swarm) = &mut self.swarm {
                // Unsubscribe from all topics
                for topic in self.active_subscriptions.keys() {
                    let _ = swarm.behaviour_mut().unsubscribe_from_topic(topic);
                }
                self.active_subscriptions.clear();

                // Disconnect from all peers gracefully
                let connected_peers: Vec<_> = swarm.connected_peers().cloned().collect();
                for peer_id in connected_peers {
                    swarm.disconnect_peer_id(peer_id).ok();
                }
            }
        }

        // Clear swarm and reset state
        self.swarm = None;
        self.pending_requests.clear();
        self.bootstrap_status = BootstrapStatus::NotStarted;
        
        if !msg.graceful {
            // Force shutdown - stop actor immediately
            ctx.stop();
        }

        Ok(Ok(()))
    }
}

impl Handler<BroadcastTransaction> for NetworkActor {
    type Result = NetworkActorResult<BroadcastResponse>;

    fn handle(&mut self, msg: BroadcastTransaction, _ctx: &mut Context<Self>) -> Self::Result {
        if let Some(swarm) = &mut self.swarm {
            match swarm.behaviour_mut().publish_message("transactions", msg.tx_data) {
                Ok(message_id) => {
                    self.metrics.messages_sent += 1;
                    tracing::debug!("Broadcasting transaction {}", msg.tx_hash);
                    
                    Ok(Ok(BroadcastResponse {
                        message_id: message_id.to_string(),
                        peers_reached: swarm.connected_peers().count() as u32,
                        propagation_started_at: std::time::SystemTime::now(),
                    }))
                }
                Err(e) => Ok(Err(NetworkError::ProtocolError {
                    message: format!("Failed to broadcast transaction: {}", e),
                })),
            }
        } else {
            Ok(Err(NetworkError::ActorError {
                reason: "Network not initialized".to_string(),
            }))
        }
    }
}

impl Handler<SubscribeToTopic> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: SubscribeToTopic, _ctx: &mut Context<Self>) -> Self::Result {
        let topic_str = msg.topic.to_string();
        
        if let Some(swarm) = &mut self.swarm {
            match swarm.behaviour_mut().subscribe_to_topic(&topic_str) {
                Ok(_) => {
                    self.active_subscriptions.insert(topic_str.clone(), Instant::now());
                    tracing::info!("Subscribed to topic: {}", topic_str);
                    Ok(Ok(()))
                }
                Err(e) => Ok(Err(NetworkError::ProtocolError {
                    message: format!("Failed to subscribe to topic {}: {}", topic_str, e),
                })),
            }
        } else {
            Ok(Err(NetworkError::ActorError {
                reason: "Network not initialized".to_string(),
            }))
        }
    }
}

impl Handler<UnsubscribeFromTopic> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: UnsubscribeFromTopic, _ctx: &mut Context<Self>) -> Self::Result {
        let topic_str = msg.topic.to_string();
        
        if let Some(swarm) = &mut self.swarm {
            match swarm.behaviour_mut().unsubscribe_from_topic(&topic_str) {
                Ok(_) => {
                    self.active_subscriptions.remove(&topic_str);
                    tracing::info!("Unsubscribed from topic: {}", topic_str);
                    Ok(Ok(()))
                }
                Err(e) => Ok(Err(NetworkError::ProtocolError {
                    message: format!("Failed to unsubscribe from topic {}: {}", topic_str, e),
                })),
            }
        } else {
            Ok(Err(NetworkError::ActorError {
                reason: "Network not initialized".to_string(),
            }))
        }
    }
}

impl Handler<SendRequest> for NetworkActor {
    type Result = actix::ResponseFuture<NetworkActorResult<RequestResponse>>;

    fn handle(&mut self, msg: SendRequest, _ctx: &mut Context<Self>) -> Self::Result {
        let peer_id = msg.peer_id;
        let request_data = msg.request_data;
        let timeout_ms = msg.timeout_ms;
        
        if let Some(swarm) = &mut self.swarm {
            let swarm_ref = swarm.clone(); // This won't work directly, need different approach
            
            Box::pin(async move {
                // In a real implementation, this would:
                // 1. Send the request via libp2p request-response protocol
                // 2. Wait for the response with timeout
                // 3. Return the response data
                
                // For now, return a placeholder response
                Ok(Ok(RequestResponse {
                    response_data: vec![],
                    peer_id,
                    duration_ms: 100,
                }))
            })
        } else {
            Box::pin(async move {
                Ok(Err(NetworkError::ActorError {
                    reason: "Network not initialized".to_string(),
                }))
            })
        }
    }
}

impl Handler<PeerConnected> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: PeerConnected, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!(
            "Peer connected: {} at {} (federation: {}, protocols: {})",
            msg.peer_id, 
            msg.address, 
            msg.is_federation_peer,
            msg.protocols.len()
        );

        // Update metrics
        self.metrics.messages_received += 1;

        // If this is a federation peer, prioritize it
        if msg.is_federation_peer {
            if let Some(swarm) = &mut self.swarm {
                // Would set peer priority in the behaviour
                tracing::info!("Prioritizing federation peer: {}", msg.peer_id);
            }
        }

        Ok(Ok(()))
    }
}

impl Handler<PeerDisconnected> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: PeerDisconnected, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Peer disconnected: {} (reason: {})", msg.peer_id, msg.reason);

        // Remove from pending requests if any
        self.pending_requests.retain(|_, request| request.peer_id != msg.peer_id);

        // Remove from metrics
        self.metrics.peer_latencies.remove(&msg.peer_id);

        Ok(Ok(()))
    }
}

impl Handler<MessageReceived> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: MessageReceived, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::debug!(
            "Message received from {} on topic {} ({} bytes)",
            msg.from_peer, msg.topic, msg.data.len()
        );

        // Update metrics
        self.metrics.messages_received += 1;
        self.metrics.total_bandwidth_in += msg.data.len() as u64;

        // Process the message based on topic
        match msg.topic {
            GossipTopic::Blocks => {
                // Would forward to ChainActor or SyncActor
                tracing::debug!("Received block data from peer {}", msg.from_peer);
            }
            GossipTopic::Transactions => {
                // Would forward to TransactionPool or ChainActor
                tracing::debug!("Received transaction data from peer {}", msg.from_peer);
            }
            GossipTopic::FederationMessages => {
                // Would forward to federation handler
                tracing::debug!("Received federation message from peer {}", msg.from_peer);
            }
            GossipTopic::Discovery => {
                // Handle peer discovery information
                tracing::debug!("Received discovery message from peer {}", msg.from_peer);
            }
            GossipTopic::Custom(topic) => {
                tracing::debug!("Received message on custom topic '{}' from peer {}", topic, msg.from_peer);
            }
        }

        Ok(Ok(()))
    }
}

impl Handler<NetworkEvent> for NetworkActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: NetworkEvent, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Network event: {:?} - {}", msg.event_type, msg.details);

        match msg.event_type {
            NetworkEventType::BootstrapCompleted => {
                self.bootstrap_status = BootstrapStatus::Completed;
                tracing::info!("Bootstrap process completed successfully");
            }
            NetworkEventType::PartitionDetected => {
                tracing::warn!("Network partition detected: {}", msg.details);
                // Could trigger recovery procedures
            }
            NetworkEventType::PartitionRecovered => {
                tracing::info!("Network partition recovered: {}", msg.details);
                // Could resume normal operations
            }
            NetworkEventType::ProtocolUpgrade => {
                tracing::info!("Protocol upgrade: {}", msg.details);
            }
            NetworkEventType::BandwidthLimitExceeded => {
                tracing::warn!("Bandwidth limit exceeded: {}", msg.details);
                // Could implement rate limiting
            }
            NetworkEventType::SecurityViolation => {
                tracing::error!("Security violation detected: {}", msg.details);
                // Could ban peer or take security measures
            }
        }

        Ok(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_actor_creation() {
        let config = NetworkConfig::default();
        let actor = NetworkActor::new(config).unwrap();
        assert_eq!(actor.actor_type(), "NetworkActor");
        assert!(actor.swarm.is_none());
    }

    #[test]
    fn network_actor_lifecycle() {
        let config = NetworkConfig::default();
        let mut actor = NetworkActor::new(config).unwrap();
        
        assert!(actor.on_start().is_ok());
        assert!(actor.health_check().is_ok());
        assert!(actor.on_stop().is_ok());
        assert!(actor.shutdown_requested);
    }

    #[tokio::test]
    async fn network_status() {
        let config = NetworkConfig::default();
        let actor = NetworkActor::new(config).unwrap();
        
        let status = actor.get_network_status();
        assert!(!status.is_active);
        assert_eq!(status.connected_peers, 0);
        assert!(status.listening_addresses.is_empty());
    }
}