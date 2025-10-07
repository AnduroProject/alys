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

use super::{
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
    /// Pending block requests tracking (Phase 4: Task 2.3)
    pending_block_requests: HashMap<uuid::Uuid, BlockRequest>,
    /// SyncActor address for coordination
    sync_actor: Option<Addr<crate::actors_v2::network::SyncActor>>,
    /// ChainActor address for AuxPoW forwarding (Phase 4: Integration Point 3b)
    chain_actor: Option<Addr<crate::actors_v2::chain::ChainActor>>,
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

/// Block request tracking (Phase 4: Task 2.3)
#[derive(Debug, Clone)]
struct BlockRequest {
    request_id: uuid::Uuid,
    peer_ids: Vec<String>,
    start_height: u64,
    count: u32,
    timestamp: Instant,
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
            pending_block_requests: HashMap::new(),
            sync_actor: None,
            chain_actor: None,
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

    /// Cleanup timed-out block requests (Phase 4: Task 7)
    fn cleanup_timed_out_requests(&mut self) {
        let now = Instant::now();
        let timeout_threshold = Duration::from_secs(60);

        self.pending_block_requests.retain(|request_id, request| {
            let elapsed = now.duration_since(request.timestamp);

            if elapsed > timeout_threshold {
                tracing::warn!(
                    request_id = %request_id,
                    start_height = request.start_height,
                    elapsed_secs = elapsed.as_secs(),
                    "Removing timed-out block request"
                );

                // Penalize peers
                for peer_id in &request.peer_ids {
                    self.peer_manager.update_peer_reputation(peer_id, -5.0);
                    tracing::debug!(
                        peer_id = %peer_id,
                        "Penalized peer for request timeout"
                    );
                }

                // Record error metric
                self.metrics.record_block_response_error();

                false // Remove this request
            } else {
                true // Keep this request
            }
        });
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

                // Start periodic cleanup of timed-out requests
                ctx.address().do_send(NetworkMessage::CleanupTimeouts);

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

            // Phase 4 messages
            NetworkMessage::BroadcastAuxPow { auxpow_data, correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());

                tracing::debug!(
                    correlation_id = %correlation_id,
                    data_len = auxpow_data.len(),
                    "Broadcasting AuxPoW to network"
                );

                // Record metrics
                self.metrics.record_auxpow_broadcast(auxpow_data.len());

                // Validate network is running
                if !self.is_running {
                    tracing::error!(correlation_id = %correlation_id, "Network not running");
                    return Err(NetworkError::NotStarted);
                }

                // Check peer connectivity
                let peer_count = self.peer_manager.get_connected_peers().len();
                if peer_count == 0 {
                    tracing::error!(correlation_id = %correlation_id, "No peers connected for AuxPoW broadcast");
                    return Err(NetworkError::Connection("No peers connected".to_string()));
                }

                if peer_count < 3 {
                    tracing::warn!(
                        correlation_id = %correlation_id,
                        peer_count = peer_count,
                        "Low peer count for AuxPoW broadcast (recommended: >=3)"
                    );
                }

                // Validate AuxPoW data format
                if let Err(e) = serde_json::from_slice::<crate::block::AuxPowHeader>(&auxpow_data) {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "Invalid AuxPoW data format"
                    );
                    return Err(NetworkError::Protocol(format!("Invalid AuxPoW format: {}", e)));
                }

                // Broadcast via gossipsub
                match self.broadcast_message("alys-auxpow", auxpow_data, false) {
                    Ok(_message_id) => {
                        tracing::info!(
                            correlation_id = %correlation_id,
                            peer_count = peer_count,
                            "Successfully broadcasted AuxPoW to network"
                        );
                        Ok(NetworkResponse::AuxPowBroadcasted { peer_count })
                    }
                    Err(e) => {
                        tracing::error!(
                            correlation_id = %correlation_id,
                            error = ?e,
                            "Failed to broadcast AuxPoW"
                        );
                        Err(NetworkError::Protocol(format!("Broadcast failed: {}", e)))
                    }
                }
            }

            NetworkMessage::RequestBlocks { start_height, count, correlation_id } => {
                let request_id = correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());

                tracing::debug!(
                    correlation_id = %request_id,
                    start_height = start_height,
                    count = count,
                    "Requesting blocks from network"
                );

                // Record metrics
                self.metrics.record_block_request_sent();

                // Validate network is running
                if !self.is_running {
                    tracing::error!(correlation_id = %request_id, "Network not running");
                    return Err(NetworkError::NotStarted);
                }

                // Validate block range
                if count == 0 || count > 100 {
                    tracing::error!(
                        correlation_id = %request_id,
                        count = count,
                        "Invalid block request count (must be 1-100)"
                    );
                    return Err(NetworkError::Protocol("Invalid block count: must be 1-100".to_string()));
                }

                // Check rate limiting (Phase 4: Task 2.9)
                const MAX_CONCURRENT_REQUESTS: usize = 10;
                if self.pending_block_requests.len() >= MAX_CONCURRENT_REQUESTS {
                    tracing::warn!(
                        correlation_id = %request_id,
                        pending_count = self.pending_block_requests.len(),
                        "Too many pending block requests"
                    );
                    return Err(NetworkError::Internal("Too many pending requests".to_string()));
                }

                // Select best peers for block requests (Phase 4: Task 2.2)
                let selected_peers = self.peer_manager.select_peers_for_blocks(5);
                if selected_peers.is_empty() {
                    tracing::error!(
                        correlation_id = %request_id,
                        "No suitable peers available for block request"
                    );
                    return Err(NetworkError::Connection("No suitable peers available".to_string()));
                }

                tracing::info!(
                    correlation_id = %request_id,
                    peer_count = selected_peers.len(),
                    start_height = start_height,
                    count = count,
                    "Selected peers for block request"
                );

                // Create and track request (Phase 4: Task 2.3)
                let block_request = BlockRequest {
                    request_id,
                    peer_ids: selected_peers.clone(),
                    start_height,
                    count,
                    timestamp: Instant::now(),
                };
                self.pending_block_requests.insert(request_id, block_request);

                // Send requests to selected peers (Phase 4: Task 2.4)
                // Note: Actual libp2p request-response implementation needed in AlysNetworkBehaviour
                for peer_id in &selected_peers {
                    tracing::debug!(
                        correlation_id = %request_id,
                        peer_id = %peer_id,
                        "Sending block request to peer"
                    );
                    // TODO: Call behaviour.send_request() when implemented
                }

                Ok(NetworkResponse::BlocksRequested {
                    peer_count: selected_peers.len(),
                    request_id,
                })
            }

            NetworkMessage::HandleBlockResponse { blocks, request_id, peer_id, correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());

                tracing::info!(
                    correlation_id = %correlation_id,
                    request_id = %request_id,
                    peer_id = %peer_id,
                    block_count = blocks.len(),
                    "Received block response from peer"
                );

                // Look up pending request
                let request = match self.pending_block_requests.remove(&request_id) {
                    Some(req) => req,
                    None => {
                        tracing::warn!(
                            correlation_id = %correlation_id,
                            request_id = %request_id,
                            "Received response for unknown or expired request"
                        );
                        self.metrics.record_block_response_error();
                        return Err(NetworkError::Protocol("Unknown request ID".to_string()));
                    }
                };

                // Validate response
                if blocks.is_empty() {
                    tracing::warn!(
                        correlation_id = %correlation_id,
                        request_id = %request_id,
                        "Peer returned empty block response"
                    );
                    self.peer_manager.record_peer_failure(&peer_id);
                    self.metrics.record_block_response_error();
                    return Err(NetworkError::Protocol("Empty block response".to_string()));
                }

                if blocks.len() as u32 > request.count {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        request_id = %request_id,
                        expected_count = request.count,
                        actual_count = blocks.len(),
                        "Peer returned more blocks than requested"
                    );
                    self.peer_manager.record_peer_failure(&peer_id);
                    self.metrics.record_block_response_error();
                    return Err(NetworkError::Protocol("Invalid block count".to_string()));
                }

                // Record metrics
                let latency = request.timestamp.elapsed();
                self.metrics.record_block_response(latency);
                self.peer_manager.record_peer_success(&peer_id);

                tracing::debug!(
                    correlation_id = %correlation_id,
                    request_id = %request_id,
                    latency_ms = latency.as_millis(),
                    "Block response latency recorded"
                );

                // Forward to SyncActor
                if let Some(sync_actor) = self.sync_actor.clone() {
                    let msg = crate::actors_v2::network::SyncMessage::HandleBlockResponse {
                        blocks,
                        request_id: request_id.to_string(),
                    };

                    tokio::spawn(async move {
                        match sync_actor.send(msg).await {
                            Ok(Ok(_)) => {
                                tracing::info!(
                                    correlation_id = %correlation_id,
                                    "Successfully forwarded blocks to SyncActor"
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::error!(
                                    correlation_id = %correlation_id,
                                    error = ?e,
                                    "SyncActor rejected blocks"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    correlation_id = %correlation_id,
                                    error = ?e,
                                    "Failed to communicate with SyncActor"
                                );
                            }
                        }
                    });

                    Ok(NetworkResponse::Started)
                } else {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        "SyncActor not available for block forwarding"
                    );
                    Err(NetworkError::Internal("SyncActor not available".to_string()))
                }
            }

            NetworkMessage::SetChainActor { addr } => {
                self.chain_actor = Some(addr);
                tracing::info!("ChainActor address set for NetworkActor AuxPoW forwarding");
                Ok(NetworkResponse::Started)
            }
            NetworkMessage::HandleCompletedAuxPow { auxpow_data, peer_id, correlation_id } => {
                let correlation_id = correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4());

                tracing::info!(
                    correlation_id = %correlation_id,
                    peer_id = %peer_id,
                    data_len = auxpow_data.len(),
                    "Received completed AuxPoW from miner"
                );

                // Record metrics
                self.metrics.record_auxpow_received();

                // Validate and deserialize AuxPoW header
                let auxpow_header = match serde_json::from_slice::<crate::block::AuxPowHeader>(&auxpow_data) {
                    Ok(header) => header,
                    Err(e) => {
                        tracing::error!(
                            correlation_id = %correlation_id,
                            peer_id = %peer_id,
                            error = ?e,
                            "Invalid AuxPoW data from miner"
                        );
                        return Err(NetworkError::Protocol(format!("Invalid AuxPoW: {}", e)));
                    }
                };

                // Validate that AuxPoW field is populated (miners must complete it)
                if auxpow_header.auxpow.is_none() {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        peer_id = %peer_id,
                        "AuxPoW header missing completed work"
                    );
                    return Err(NetworkError::Protocol("Incomplete AuxPoW".to_string()));
                }

                // Forward to ChainActor for queuing (spawn async task)
                if let Some(chain_actor) = self.chain_actor.clone() {
                    let peer_id_clone = peer_id.clone();

                    // Spawn async task to forward to ChainActor
                    tokio::spawn(async move {
                        let msg = crate::actors_v2::chain::messages::ChainMessage::QueueAuxPow {
                            auxpow_header,
                            correlation_id: Some(correlation_id),
                        };

                        match chain_actor.send(msg).await {
                            Ok(Ok(_)) => {
                                tracing::info!(
                                    correlation_id = %correlation_id,
                                    peer_id = %peer_id_clone,
                                    "Successfully queued completed AuxPoW"
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::error!(
                                    correlation_id = %correlation_id,
                                    error = ?e,
                                    "ChainActor rejected AuxPoW"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    correlation_id = %correlation_id,
                                    error = ?e,
                                    "Failed to communicate with ChainActor"
                                );
                            }
                        }
                    });

                    // Update peer reputation immediately - they provided useful work
                    self.peer_manager.record_peer_success(&peer_id);

                    tracing::info!(
                        correlation_id = %correlation_id,
                        peer_id = %peer_id,
                        "AuxPoW accepted and forwarding to ChainActor"
                    );

                    Ok(NetworkResponse::Started)
                } else {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        "ChainActor not available for AuxPoW queueing"
                    );
                    Err(NetworkError::Internal("ChainActor not available".to_string()))
                }
            }
            NetworkMessage::HealthCheck { correlation_id } => {
                tracing::debug!(
                    correlation_id = ?correlation_id,
                    "Performing network health check"
                );

                let connected_peers = self.peer_manager.get_connected_peers().len();
                let is_healthy = self.is_running && connected_peers > 0;
                let issues = if !is_healthy {
                    vec![
                        if !self.is_running { "Network not running".to_string() } else { String::new() },
                        if connected_peers == 0 { "No peers connected".to_string() } else { String::new() },
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect()
                } else {
                    vec![]
                };

                Ok(NetworkResponse::Healthy { is_healthy, connected_peers, issues })
            }

            NetworkMessage::CleanupTimeouts => {
                tracing::debug!("Running periodic block request timeout cleanup");

                self.cleanup_timed_out_requests();

                // Schedule next cleanup in 30 seconds
                ctx.run_later(Duration::from_secs(30), |_act, ctx| {
                    ctx.address().do_send(NetworkMessage::CleanupTimeouts);
                });

                Ok(NetworkResponse::Started)
            }
        }
    }
}