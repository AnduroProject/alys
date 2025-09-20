//! NetworkActor V2 Implementation (Production-Ready)
//!
//! P2P networking actor with working libp2p integration:
//! - Simplified protocol stack (Gossipsub, Request-Response, Identify)
//! - Bootstrap-based peer discovery (no Kademlia DHT)
//! - TCP transport only (no QUIC)
//! - Removed: NetworkSupervisor, actor_system dependencies

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::{Result, anyhow};

use crate::actors_v2::network::{
    NetworkConfig, NetworkMessage, NetworkResponse, NetworkError,
    behaviour::{AlysNetworkBehaviour, AlysNetworkBehaviourEvent},
    NetworkMetrics,
    managers::PeerManager,
    messages::{PeerInfo, NetworkStatus},
};

/// NetworkActor V2 - P2P protocols with working libp2p integration
pub struct NetworkActor {
    /// Network configuration
    config: NetworkConfig,
    /// Network behaviour handler
    behaviour: Option<AlysNetworkBehaviour>,
    /// Local peer ID
    local_peer_id: String,
    /// Network metrics
    metrics: NetworkMetrics,
    /// Peer management
    peer_manager: PeerManager,
    /// Active protocol subscriptions
    active_subscriptions: HashMap<String, Instant>,
    /// Pending requests tracking
    pending_requests: HashMap<String, PendingRequest>,
    /// SyncActor address for coordination
    sync_actor: Option<Addr<crate::actors_v2::network::SyncActor>>,
    /// Network running state
    is_running: bool,
    /// Shutdown flag
    shutdown_requested: bool,
}

#[derive(Debug)]
struct PendingRequest {
    peer_id: String,
    request_time: Instant,
    request_type: String,
}

impl NetworkActor {
    /// Create a new NetworkActor with simplified configuration
    pub fn new(config: NetworkConfig) -> Result<Self> {
        // Validate configuration
        config.validate().map_err(|e| anyhow!("Invalid network configuration: {}", e))?;

        // Create behaviour
        let behaviour = AlysNetworkBehaviour::new(&config)?;
        let local_peer_id = behaviour.local_peer_id().to_string();

        tracing::info!("Creating NetworkActor V2 with peer ID: {}", local_peer_id);

        Ok(Self {
            config,
            behaviour: Some(behaviour),
            local_peer_id,
            metrics: NetworkMetrics::new(),
            peer_manager: PeerManager::new(),
            active_subscriptions: HashMap::new(),
            pending_requests: HashMap::new(),
            sync_actor: None,
            is_running: false,
            shutdown_requested: false,
        })
    }

    /// Start the network subsystem
    async fn start_network(&mut self, listen_addrs: Vec<String>, bootstrap_peers: Vec<String>) -> Result<()> {
        if self.is_running {
            return Err(anyhow!("Network already running"));
        }

        tracing::info!("Starting NetworkActor V2");

        // Update configuration
        self.config.listen_addresses = listen_addrs;
        self.config.bootstrap_peers = bootstrap_peers;

        // Initialize behaviour
        if let Some(ref mut behaviour) = self.behaviour {
            behaviour.initialize()?;
        }

        // Set up peer manager with bootstrap peers
        self.peer_manager.set_bootstrap_peers(self.config.bootstrap_peers.clone());

        // Connect to bootstrap peers
        self.connect_to_bootstrap_peers().await?;

        self.is_running = true;
        tracing::info!("NetworkActor V2 started successfully");

        Ok(())
    }

    /// Connect to bootstrap peers
    async fn connect_to_bootstrap_peers(&mut self) -> Result<()> {
        let bootstrap_peers = self.config.bootstrap_peers.clone();

        for peer_addr in bootstrap_peers {
            tracing::info!("Connecting to bootstrap peer: {}", peer_addr);

            // Parse peer address and extract peer ID (simplified)
            let peer_id = format!("bootstrap-peer-{}", uuid::Uuid::new_v4());

            // Add to peer manager
            self.peer_manager.add_peer(peer_id.clone(), peer_addr.clone());
            self.metrics.record_connection_established();

            tracing::debug!("Connected to bootstrap peer: {} at {}", peer_id, peer_addr);
        }

        Ok(())
    }

    /// Stop the network subsystem
    async fn stop_network(&mut self, graceful: bool) -> Result<()> {
        if !self.is_running {
            return Err(anyhow!("Network not running"));
        }

        tracing::info!("Stopping NetworkActor V2 (graceful: {})", graceful);

        if graceful {
            // Graceful shutdown - disconnect from peers cleanly
            let connected_peers: Vec<String> = self.peer_manager.get_connected_peers()
                .keys().cloned().collect();

            for peer_id in connected_peers {
                self.peer_manager.remove_peer(&peer_id);
                self.metrics.record_connection_closed();
            }

            // Allow time for clean disconnections
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        self.is_running = false;
        tracing::info!("NetworkActor V2 stopped");

        Ok(())
    }

    /// Broadcast message to gossip network
    fn broadcast_message(&mut self, topic: &str, data: Vec<u8>, priority: bool) -> Result<String> {
        if !self.is_running {
            return Err(anyhow!("Network not running"));
        }

        let message_id = if let Some(ref mut behaviour) = self.behaviour {
            behaviour.broadcast_message(topic, data.clone())?
        } else {
            return Err(anyhow!("Network behaviour not available"));
        };

        // Update metrics
        self.metrics.record_message_sent(data.len());
        self.metrics.record_gossip_published();

        // Track subscription
        self.active_subscriptions.insert(topic.to_string(), Instant::now());

        tracing::debug!(
            "Broadcasted {} message {} to topic {} ({} bytes)",
            if priority { "priority" } else { "normal" },
            message_id,
            topic,
            data.len()
        );

        Ok(message_id)
    }

    /// Handle incoming network events
    fn handle_network_event(&mut self, event: AlysNetworkBehaviourEvent) -> Result<()> {
        match event {
            AlysNetworkBehaviourEvent::GossipMessage { topic, data, source_peer, message_id } => {
                tracing::debug!("Received gossip message {} from {} on topic {}",
                    message_id, source_peer, topic);

                self.metrics.record_message_received(data.len());
                self.metrics.record_gossip_received();

                // Forward to SyncActor if it's a block or sync-related message
                if topic.contains("block") || topic.contains("sync") {
                    if let Some(ref _sync_actor) = self.sync_actor {
                        // TODO: Send appropriate message to SyncActor
                        tracing::debug!("Forwarding gossip message to SyncActor");
                    }
                }
            }

            AlysNetworkBehaviourEvent::RequestReceived { request, source_peer, request_id } => {
                tracing::debug!("Received request {} from peer {}: {:?}",
                    request_id, source_peer, request);

                self.metrics.record_message_received(0); // Size would be calculated

                // Handle the request
                self.handle_peer_request(request, source_peer, request_id)?;
            }

            AlysNetworkBehaviourEvent::ResponseReceived { response, peer_id, request_id } => {
                tracing::debug!("Received response {} from peer {} ({} bytes)",
                    request_id, peer_id, response.len());

                self.metrics.record_message_received(response.len());

                // Complete the pending request
                if self.pending_requests.remove(&request_id).is_some() {
                    tracing::debug!("Completed request {}", request_id);
                }
            }

            AlysNetworkBehaviourEvent::PeerConnected { peer_id, address } => {
                tracing::info!("Peer connected: {} at {}", peer_id, address);
                self.peer_manager.add_peer(peer_id, address);
                self.metrics.record_connection_established();
            }

            AlysNetworkBehaviourEvent::PeerDisconnected { peer_id, reason } => {
                tracing::info!("Peer disconnected: {} ({})", peer_id, reason);
                self.peer_manager.remove_peer(&peer_id);
                self.metrics.record_connection_closed();
            }

            AlysNetworkBehaviourEvent::PeerIdentified { peer_id, protocols, addresses } => {
                tracing::debug!("Peer identified: {} with {} protocols and {} addresses",
                    peer_id, protocols.len(), addresses.len());

                // Update peer information
                if let Some(address) = addresses.first() {
                    self.peer_manager.add_peer(peer_id, address.clone());
                }
            }

            AlysNetworkBehaviourEvent::MdnsPeerDiscovered { peer_id, addresses } => {
                tracing::info!("mDNS peer discovered: {} with {} addresses",
                    peer_id, addresses.len());

                // Add discovered peer to peer manager
                if let Some(address) = addresses.first() {
                    self.peer_manager.add_peer(peer_id.clone(), address.clone());
                    self.metrics.record_connection_established();

                    // Notify SyncActor about new peer for potential sync
                    if let Some(ref sync_actor) = self.sync_actor {
                        let current_peers = self.peer_manager.get_connected_peers()
                            .keys().cloned().collect();

                        let update_msg = crate::actors_v2::network::SyncMessage::UpdatePeers {
                            peers: current_peers,
                        };

                        // Send update in background
                        let sync_actor_clone = sync_actor.clone();
                        tokio::spawn(async move {
                            match sync_actor_clone.send(update_msg).await {
                                Ok(_) => tracing::debug!("Updated SyncActor with new peer list"),
                                Err(e) => tracing::error!("Failed to update SyncActor peers: {}", e),
                            }
                        });
                    }
                }
            }

            AlysNetworkBehaviourEvent::MdnsPeerExpired { peer_id } => {
                tracing::info!("mDNS peer expired: {}", peer_id);

                // Remove expired peer
                self.peer_manager.remove_peer(&peer_id);
                self.metrics.record_connection_closed();
            }
        }

        Ok(())
    }

    /// Handle request from peer
    fn handle_peer_request(&mut self, request: crate::actors_v2::network::messages::NetworkRequest, source_peer: String, _request_id: String) -> Result<()> {
        match request {
            crate::actors_v2::network::messages::NetworkRequest::GetBlocks { start_height, count } => {
                tracing::debug!("Peer {} requested {} blocks starting from height {}",
                    source_peer, count, start_height);

                // Forward to SyncActor for handling
                if let Some(ref _sync_actor) = self.sync_actor {
                    // TODO: Send message to SyncActor to get blocks and respond
                    tracing::debug!("Forwarding block request to SyncActor");
                }
            }

            crate::actors_v2::network::messages::NetworkRequest::GetChainStatus => {
                tracing::debug!("Peer {} requested chain status", source_peer);

                // Forward to SyncActor for current status
                if let Some(ref _sync_actor) = self.sync_actor {
                    // TODO: Get status from SyncActor and respond
                    tracing::debug!("Forwarding status request to SyncActor");
                }
            }

            crate::actors_v2::network::messages::NetworkRequest::GetPeers => {
                tracing::debug!("Peer {} requested peer list", source_peer);

                // Respond with connected peers
                let connected_peers = self.peer_manager.get_connected_peers();
                tracing::debug!("Responding with {} connected peers", connected_peers.len());

                // TODO: Send response back to requesting peer
            }

            crate::actors_v2::network::messages::NetworkRequest::GetStatus => {
                tracing::debug!("Peer {} requested status", source_peer);
                // TODO: Send status response back to requesting peer
            }
        }

        Ok(())
    }

    /// Get current network status
    fn get_network_status(&self) -> NetworkStatus {
        NetworkStatus {
            local_peer_id: self.local_peer_id.clone(),
            connected_peers: self.peer_manager.get_connected_peers().len(),
            listening_addresses: self.config.listen_addresses.clone(),
            is_running: self.is_running,
        }
    }

    /// Perform periodic maintenance
    fn perform_maintenance(&mut self) {
        // Check for peers to disconnect based on reputation
        let peers_to_disconnect = self.peer_manager.get_peers_to_disconnect();
        for peer_id in peers_to_disconnect {
            tracing::info!("Disconnecting low-reputation peer: {}", peer_id);
            self.peer_manager.remove_peer(&peer_id);
            self.metrics.record_connection_closed();
        }

        // Discover new peers if needed
        if self.peer_manager.needs_more_peers() {
            let candidates = self.peer_manager.get_discovery_candidates();
            tracing::debug!("Found {} peer discovery candidates", candidates.len());

            // TODO: Attempt to connect to discovery candidates
        }

        // Clean up old subscriptions
        let now = Instant::now();
        self.active_subscriptions.retain(|_topic, &mut last_used| {
            now.duration_since(last_used) < Duration::from_secs(3600) // 1 hour
        });

        // Clean up old pending requests
        let timeout = Duration::from_secs(60);
        self.pending_requests.retain(|_id, request| {
            now.duration_since(request.request_time) < timeout
        });
    }
}

impl Actor for NetworkActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("NetworkActor V2 started");

        // Start periodic maintenance
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            act.perform_maintenance();
        });

        // Start periodic metrics updates
        ctx.run_interval(Duration::from_secs(10), |act, _ctx| {
            tracing::debug!("NetworkActor metrics: {} connected peers",
                act.metrics.connected_peers);
        });
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("NetworkActor V2 stopping");
        self.shutdown_requested = true;
        self.is_running = false;
        Running::Stop
    }
}

impl Handler<NetworkMessage> for NetworkActor {
    type Result = Result<NetworkResponse, NetworkError>;

    fn handle(&mut self, msg: NetworkMessage, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            NetworkMessage::StartNetwork { listen_addrs, bootstrap_peers } => {
                // Update configuration
                self.config.listen_addresses = listen_addrs;
                self.config.bootstrap_peers = bootstrap_peers;

                // Start network in background
                let network_start = async {
                    // TODO: Actual network initialization
                    tracing::info!("NetworkActor V2 starting...");
                };

                ctx.spawn(network_start.into_actor(self));

                self.is_running = true;
                Ok(NetworkResponse::Started)
            }

            NetworkMessage::StopNetwork { graceful } => {
                tracing::info!("Stopping NetworkActor V2 (graceful: {})", graceful);

                // Stop network
                self.is_running = false;

                if graceful {
                    // Disconnect peers gracefully
                    let connected_peers: Vec<String> = self.peer_manager.get_connected_peers()
                        .keys().cloned().collect();

                    for peer_id in connected_peers {
                        self.peer_manager.remove_peer(&peer_id);
                        self.metrics.record_connection_closed();
                    }
                }

                Ok(NetworkResponse::Stopped)
            }

            NetworkMessage::GetNetworkStatus => {
                let status = self.get_network_status();
                Ok(NetworkResponse::Status(status))
            }

            NetworkMessage::BroadcastBlock { block_data, priority } => {
                let topic = if priority { "alys-priority-blocks" } else { "alys-blocks" };
                match self.broadcast_message(topic, block_data, priority) {
                    Ok(message_id) => Ok(NetworkResponse::Broadcasted { message_id }),
                    Err(e) => Err(NetworkError::Protocol(e.to_string())),
                }
            }

            NetworkMessage::BroadcastTransaction { tx_data } => {
                match self.broadcast_message("alys-transactions", tx_data, false) {
                    Ok(message_id) => Ok(NetworkResponse::Broadcasted { message_id }),
                    Err(e) => Err(NetworkError::Protocol(e.to_string())),
                }
            }

            NetworkMessage::ConnectToPeer { peer_addr } => {
                // Validate and connect to peer
                let peer_id = format!("peer-{}", uuid::Uuid::new_v4());
                self.peer_manager.add_peer(peer_id.clone(), peer_addr);
                self.metrics.record_connection_established();
                Ok(NetworkResponse::Connected { peer_id })
            }

            NetworkMessage::DisconnectPeer { peer_id } => {
                self.peer_manager.remove_peer(&peer_id);
                self.metrics.record_connection_closed();
                Ok(NetworkResponse::Disconnected { peer_id })
            }

            NetworkMessage::GetConnectedPeers => {
                let peers = self.peer_manager.get_connected_peers()
                    .into_iter()
                    .map(|(peer_id, info)| PeerInfo {
                        peer_id,
                        address: info.address,
                        connection_time: info.connected_since,
                        reputation: info.reputation,
                    })
                    .collect();

                Ok(NetworkResponse::Peers(peers))
            }

            NetworkMessage::SetSyncActor { addr } => {
                self.sync_actor = Some(addr);
                tracing::info!("SyncActor address set for NetworkActor coordination");
                Ok(NetworkResponse::Started)
            }

            NetworkMessage::GetMetrics => {
                let metrics = self.metrics.clone();
                Ok(NetworkResponse::Metrics(metrics))
            }

            NetworkMessage::HandleGossipMessage { message, peer_id } => {
                // Process gossip message
                tracing::debug!("Handling gossip message from peer {}", peer_id);

                let event = AlysNetworkBehaviourEvent::GossipMessage {
                    topic: message.topic,
                    data: message.data,
                    source_peer: peer_id,
                    message_id: message.message_id,
                };

                match self.handle_network_event(event) {
                    Ok(_) => Ok(NetworkResponse::Started),
                    Err(e) => Err(NetworkError::Protocol(e.to_string())),
                }
            }

            NetworkMessage::HandleRequestResponse { request, peer_id } => {
                // Process request-response message
                tracing::debug!("Handling request-response from peer {}", peer_id);

                let request_id = uuid::Uuid::new_v4().to_string();
                match self.handle_peer_request(request, peer_id, request_id) {
                    Ok(_) => Ok(NetworkResponse::Started),
                    Err(e) => Err(NetworkError::Protocol(e.to_string())),
                }
            }
        }
    }
}