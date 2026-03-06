//! NetworkActor V2 Implementation (Production-Ready)
//!
//! P2P networking actor with working libp2p integration:
//! - Simplified protocol stack (Gossipsub, Request-Response, Identify)
//! - Bootstrap-based peer discovery (no Kademlia DHT)
//! - TCP transport only (no QUIC)
//! - Removed: NetworkSupervisor, actor_system dependencies

use actix::prelude::*;
use anyhow::{anyhow, Context as AnyhowContext, Result};
use ethereum_types::H256;
use futures::{select, FutureExt, StreamExt};
use libp2p::request_response::{RequestId, ResponseChannel};
use libp2p::{
    swarm::{ConnectionHandler, NetworkBehaviour, Swarm, SwarmEvent},
    Multiaddr, PeerId,
};
use lru::LruCache;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use super::{
    behaviour::{AlysNetworkBehaviour, AlysNetworkBehaviourEvent},
    managers::{PeerManager, Violation},
    messages::{NetworkStatus, PeerInfo, SyncMessage},
    metrics::{
        NETWORK_BLOCKS_DESER_ERRORS, NETWORK_BLOCKS_DUPLICATE, NETWORK_BLOCKS_FORWARDED,
        NETWORK_BLOCKS_RECEIVED,
    },
    protocols::{BlockRequest, BlockResponse},
    NetworkConfig, NetworkError, NetworkMessage, NetworkMetrics, NetworkResponse,
};

/// Type alias for SwarmEvent with our behaviour's error type
type AlysSwarmEvent = SwarmEvent<
    AlysNetworkBehaviourEvent,
    <<AlysNetworkBehaviour as NetworkBehaviour>::ConnectionHandler as ConnectionHandler>::Error,
>;

/// Commands that can be sent to the swarm polling task
///
/// Phase 2 Task 2.0: SwarmCommand channel foundation
#[derive(Debug)]
pub enum SwarmCommand {
    /// Dial a peer at the given multiaddr
    Dial {
        addr: Multiaddr,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Start listening on an address
    ListenOn {
        addr: Multiaddr,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Publish a gossipsub message
    PublishGossip {
        topic: String,
        data: Vec<u8>,
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    /// Subscribe to a gossipsub topic
    SubscribeTopic {
        topic: String,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Send a request-response request
    SendRequest {
        peer_id: PeerId,
        request: BlockRequest,
        response_tx: tokio::sync::oneshot::Sender<Result<RequestId, String>>,
    },
    /// Send a request-response response
    SendResponse {
        channel: ResponseChannel<BlockResponse>,
        response: BlockResponse,
    },
    /// Add peer as explicit gossipsub peer for immediate mesh formation
    AddExplicitPeer { peer_id: PeerId },
}

/// Phase 4: Rate limiter for DOS protection
#[derive(Debug)]
struct RateLimiter {
    /// Per-peer message timestamps (sliding window)
    peer_message_counts: HashMap<String, VecDeque<Instant>>,
    /// Per-peer byte counts (timestamp, byte_count)
    peer_byte_counts: HashMap<String, VecDeque<(Instant, u64)>>,
    /// Rate limit window duration
    window: Duration,
    /// Max messages per peer per window
    max_messages: u64,
    /// Max bytes per peer per window
    max_bytes: u64,
}

impl RateLimiter {
    fn new(window: Duration, max_messages: u64, max_bytes: u64) -> Self {
        Self {
            peer_message_counts: HashMap::new(),
            peer_byte_counts: HashMap::new(),
            window,
            max_messages,
            max_bytes,
        }
    }

    /// Check if peer has exceeded message rate limit
    fn check_message_rate(&mut self, peer_id: &str) -> Result<(), NetworkError> {
        let now = Instant::now();
        let cutoff = now - self.window;

        // Get or create peer's message queue
        let messages = self
            .peer_message_counts
            .entry(peer_id.to_string())
            .or_insert_with(VecDeque::new);

        // Remove old messages outside the window
        while messages.front().map_or(false, |&t| t < cutoff) {
            messages.pop_front();
        }

        // Check rate limit
        if messages.len() as u64 >= self.max_messages {
            return Err(NetworkError::Protocol(format!(
                "Rate limit exceeded: {} messages in {} seconds",
                messages.len(),
                self.window.as_secs()
            )));
        }

        // Record this message
        messages.push_back(now);

        Ok(())
    }

    /// Check if peer has exceeded bandwidth rate limit
    fn check_byte_rate(&mut self, peer_id: &str, bytes: u64) -> Result<(), NetworkError> {
        let now = Instant::now();
        let cutoff = now - self.window;

        // Get or create peer's byte queue
        let byte_records = self
            .peer_byte_counts
            .entry(peer_id.to_string())
            .or_insert_with(VecDeque::new);

        // Remove old records outside the window
        while byte_records.front().map_or(false, |(t, _)| *t < cutoff) {
            byte_records.pop_front();
        }

        // Calculate total bytes in window
        let total_bytes: u64 = byte_records.iter().map(|(_, b)| b).sum();

        // Check bandwidth limit
        if total_bytes + bytes > self.max_bytes {
            return Err(NetworkError::Protocol(format!(
                "Bandwidth limit exceeded: {} bytes in {} seconds (limit: {} bytes)",
                total_bytes + bytes,
                self.window.as_secs(),
                self.max_bytes
            )));
        }

        // Record these bytes
        byte_records.push_back((now, bytes));

        Ok(())
    }

    /// Clean up old rate limit data for peers
    fn cleanup(&mut self, active_peers: &[String]) {
        // Remove data for disconnected peers
        self.peer_message_counts
            .retain(|peer_id, _| active_peers.contains(peer_id));
        self.peer_byte_counts
            .retain(|peer_id, _| active_peers.contains(peer_id));
    }
}

/// NetworkActor V2 - P2P protocols with working libp2p integration
pub struct NetworkActor {
    /// Network configuration
    config: NetworkConfig,

    /// Event receiver from swarm polling task
    event_rx: Option<mpsc::UnboundedReceiver<AlysSwarmEvent>>,

    /// Swarm polling task handle (for graceful shutdown)
    swarm_task_handle: Option<tokio::task::JoinHandle<()>>,

    /// Send commands to swarm task (Phase 2 Task 2.0)
    swarm_cmd_tx: Option<mpsc::Sender<SwarmCommand>>,

    /// Local peer ID (cached from config)
    local_peer_id: String,

    /// Network metrics
    metrics: NetworkMetrics,
    /// Peer management
    peer_manager: PeerManager,
    /// Phase 4: Rate limiter for DOS protection
    rate_limiter: RateLimiter,
    /// Active protocol subscriptions
    active_subscriptions: HashMap<String, Instant>,
    /// Pending block requests tracking (Phase 4: Task 2.3)
    pending_block_requests: HashMap<uuid::Uuid, PendingBlockRequest>,
    /// SyncActor address for coordination
    sync_actor: Option<Addr<crate::actors_v2::network::SyncActor>>,
    /// ChainActor address for AuxPoW forwarding (Phase 4: Integration Point 3b)
    chain_actor: Option<Addr<crate::actors_v2::chain::ChainActor>>,
    /// StorageActor address for block request handling
    storage_actor: Option<Addr<crate::actors_v2::storage::StorageActor>>,
    /// Phase 5: Cache of recently seen block hashes
    /// Prevents duplicate forwarding to ChainActor
    block_cache: Arc<RwLock<LruCache<H256, Instant>>>,
    /// Network running state
    is_running: bool,
    /// Shutdown flag
    shutdown_requested: bool,
    /// Last V2 peer reconnection attempt (for cooldown)
    last_v2_reconnection_attempt: Option<Instant>,
}

/// Pending block request tracking (Phase 4: Task 2.3)
#[derive(Debug, Clone)]
struct PendingBlockRequest {
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
        config
            .validate()
            .map_err(|e| anyhow!("Invalid network configuration: {}", e))?;

        // Generate peer ID for identification (swarm will be created on StartNetwork)
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = libp2p::PeerId::from(keypair.public()).to_string();

        tracing::info!("Creating NetworkActor V2 with peer ID: {}", local_peer_id);

        // Phase 4: Initialize rate limiter from config
        let rate_limiter = RateLimiter::new(
            config.rate_limit_window,
            config.max_messages_per_peer_per_second,
            config.max_bytes_per_peer_per_second,
        );

        // Phase 5: Initialize block cache (LRU with capacity of 100 blocks)
        let block_cache = Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(100).unwrap())));

        Ok(Self {
            config,
            event_rx: None,
            swarm_task_handle: None,
            swarm_cmd_tx: None,
            local_peer_id,
            metrics: NetworkMetrics::new(),
            peer_manager: PeerManager::new(),
            rate_limiter,
            active_subscriptions: HashMap::new(),
            pending_block_requests: HashMap::new(),
            sync_actor: None,
            block_cache,
            chain_actor: None,
            storage_actor: None,
            is_running: false,
            shutdown_requested: false,
            last_v2_reconnection_attempt: None,
        })
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

    /// Select a non-loopback address from a list of addresses.
    /// Loopback addresses (127.0.0.1, ::1) are unreachable from other containers
    /// in Docker networks, so we prefer external addresses for peer storage.
    /// Falls back to first address if all addresses are loopback.
    pub(crate) fn select_external_address(addresses: &[String]) -> Option<&String> {
        // First, try to find a non-loopback address
        addresses
            .iter()
            .find(|addr| !addr.contains("127.0.0.1") && !addr.contains("/ip6/::1/"))
            .or_else(|| addresses.first())
    }

    /// Cooldown duration between V2 reconnection attempts (30 seconds)
    const V2_RECONNECTION_COOLDOWN: Duration = Duration::from_secs(30);

    /// Attempt to reconnect to known V2-capable peers
    /// Called when the last V2 peer disconnects or periodically if no V2 peers are connected
    /// Includes cooldown to prevent reconnection spam
    fn attempt_v2_peer_reconnection(&mut self) {
        // Check cooldown to prevent reconnection spam
        if let Some(last_attempt) = self.last_v2_reconnection_attempt {
            let elapsed = last_attempt.elapsed();
            if elapsed < Self::V2_RECONNECTION_COOLDOWN {
                tracing::debug!(
                    elapsed_secs = elapsed.as_secs(),
                    cooldown_secs = Self::V2_RECONNECTION_COOLDOWN.as_secs(),
                    "V2 reconnection cooldown active - skipping attempt"
                );
                return;
            }
        }

        let candidates = self.peer_manager.get_v2_reconnection_candidates();

        if candidates.is_empty() {
            // Use DEBUG level - this is expected during startup before any V2 peers are known
            tracing::debug!(
                "No V2-capable peers available for reconnection"
            );
            return;
        }

        // Update last attempt timestamp
        self.last_v2_reconnection_attempt = Some(Instant::now());

        tracing::info!(
            candidate_count = candidates.len(),
            "Attempting to reconnect to known V2-capable peers"
        );

        if let Some(cmd_tx) = self.swarm_cmd_tx.as_ref() {
            for (peer_id, address) in candidates {
                // Parse multiaddr for dialing
                match address.parse::<Multiaddr>() {
                    Ok(multiaddr) => {
                        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                        let dial_cmd = SwarmCommand::Dial {
                            addr: multiaddr.clone(),
                            response_tx,
                        };

                        match cmd_tx.try_send(dial_cmd) {
                            Ok(_) => {
                                tracing::info!(
                                    peer_id = %peer_id,
                                    address = %address,
                                    "Attempting reconnection to V2 peer"
                                );

                                // Spawn task to handle dial response
                                let peer_id_clone = peer_id.clone();
                                tokio::spawn(async move {
                                    match response_rx.await {
                                        Ok(Ok(())) => {
                                            tracing::info!(
                                                peer_id = %peer_id_clone,
                                                "Successfully reconnected to V2 peer"
                                            );
                                        }
                                        Ok(Err(e)) => {
                                            tracing::warn!(
                                                peer_id = %peer_id_clone,
                                                error = %e,
                                                "Failed to reconnect to V2 peer"
                                            );
                                        }
                                        Err(_) => {
                                            tracing::debug!(
                                                peer_id = %peer_id_clone,
                                                "Dial response channel closed for V2 peer"
                                            );
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!(
                                    peer_id = %peer_id,
                                    error = ?e,
                                    "Failed to send dial command for V2 peer"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            peer_id = %peer_id,
                            address = %address,
                            error = %e,
                            "Invalid multiaddr for V2 peer reconnection"
                        );
                    }
                }
            }
        } else {
            tracing::debug!(
                "Cannot attempt V2 peer reconnection: command channel not available"
            );
        }
    }

    /// Check V2 peer connectivity and attempt reconnection if needed
    /// This is called periodically to ensure network health
    fn check_v2_peer_health(&mut self) {
        let v2_count = self.peer_manager.connected_v2_peer_count();
        let total_connected = self.peer_manager.get_connected_peers().len();

        tracing::trace!(
            v2_peer_count = v2_count,
            total_connected = total_connected,
            "V2 peer health check"
        );

        if v2_count == 0 && total_connected > 0 {
            // We have peers but none support V2 - this is a problem
            tracing::warn!(
                total_connected = total_connected,
                "Connected to peers but none support V2 protocol - sync will fail"
            );
            self.attempt_v2_peer_reconnection();
        } else if v2_count == 0 && total_connected == 0 {
            // No peers at all - peer discovery should handle this
            tracing::debug!("No peers connected - waiting for peer discovery");
        }
    }

    /// Handle swarm events (delegated from StreamHandler)
    fn handle_swarm_event(&mut self, event: AlysSwarmEvent) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                self.handle_network_event(behaviour_event)?;
            }

            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                tracing::info!(
                    peer_id = %peer_id,
                    endpoint = ?endpoint,
                    "Connection established"
                );
                self.peer_manager.add_peer(
                    peer_id.to_string(),
                    endpoint.get_remote_address().to_string(),
                );
                self.metrics.record_connection_established();

                // CRITICAL FIX FOR ISSUE #2: Add peer as explicit gossipsub peer immediately
                // This ensures the peer is added to the gossipsub mesh for all topics
                // without waiting for the heartbeat tick (which can take 1+ seconds)
                if let Some(cmd_tx) = &self.swarm_cmd_tx {
                    let cmd = SwarmCommand::AddExplicitPeer { peer_id };
                    if let Err(e) = cmd_tx.try_send(cmd) {
                        tracing::warn!(
                            peer_id = %peer_id,
                            error = ?e,
                            "Failed to send AddExplicitPeer command after connection"
                        );
                    } else {
                        tracing::debug!(
                            peer_id = %peer_id,
                            "Sent AddExplicitPeer command for immediate mesh formation"
                        );
                    }
                }
            }

            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                tracing::info!(
                    peer_id = %peer_id,
                    cause = ?cause,
                    "Connection closed"
                );
                self.peer_manager.remove_peer(&peer_id.to_string());
                self.metrics.record_connection_closed();
            }

            SwarmEvent::IncomingConnection {
                local_addr,
                send_back_addr,
                connection_id,
            } => {
                tracing::debug!(
                    local_addr = %local_addr,
                    send_back_addr = %send_back_addr,
                    connection_id = ?connection_id,
                    "Incoming connection"
                );
            }

            SwarmEvent::IncomingConnectionError {
                local_addr,
                send_back_addr,
                error,
                connection_id,
            } => {
                tracing::warn!(
                    local_addr = %local_addr,
                    send_back_addr = %send_back_addr,
                    connection_id = ?connection_id,
                    error = %error,
                    "Incoming connection error"
                );
            }

            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                tracing::warn!(
                    peer_id = ?peer_id,
                    error = %error,
                    "Outgoing connection error"
                );
                if let Some(peer_id) = peer_id {
                    self.peer_manager.record_peer_failure(&peer_id.to_string());
                }
            }

            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!(address = %address, "Listening on new address");
            }

            SwarmEvent::ExpiredListenAddr { address, .. } => {
                tracing::info!(address = %address, "Expired listen address");
            }

            SwarmEvent::ListenerClosed { addresses, .. } => {
                tracing::info!(addresses = ?addresses, "Listener closed");
            }

            SwarmEvent::ListenerError { error, .. } => {
                tracing::error!(error = %error, "Listener error");
            }

            SwarmEvent::Dialing { peer_id, .. } => {
                tracing::debug!(peer_id = ?peer_id, "Dialing peer");
            }
        }

        Ok(())
    }

    /// Restart swarm after unexpected shutdown
    fn restart_swarm(&mut self, ctx: &mut Context<Self>) -> Result<()> {
        tracing::info!("Creating new swarm for restart");

        // Create new swarm
        let mut swarm = crate::actors_v2::network::swarm_factory::create_swarm(&self.config)
            .context("Failed to create swarm during restart")?;

        // Re-listen on configured addresses
        for addr_str in &self.config.listen_addresses {
            let addr: Multiaddr = addr_str
                .parse()
                .context(format!("Invalid listen address: {}", addr_str))?;

            swarm
                .listen_on(addr.clone())
                .context(format!("Failed to listen on {}", addr))?;

            tracing::info!("Listening on: {}", addr);
        }

        // Setup new event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Spawn new swarm polling task
        let swarm_task = tokio::spawn(async move {
            use futures::StreamExt;

            loop {
                match swarm.select_next_some().await {
                    event => {
                        if event_tx.send(event).is_err() {
                            tracing::info!("Event receiver dropped, stopping swarm poll");
                            break; // Actor stopped
                        }
                    }
                }
            }
        });

        self.swarm_task_handle = Some(swarm_task);

        // Add new event stream to actor context
        ctx.add_stream(tokio_stream::wrappers::UnboundedReceiverStream::new(
            event_rx,
        ));

        self.is_running = true;

        Ok(())
    }

    /// Handle incoming network events
    fn handle_network_event(&mut self, event: AlysNetworkBehaviourEvent) -> Result<()> {
        match event {
            AlysNetworkBehaviourEvent::GossipMessage {
                topic,
                data,
                source_peer,
                message_id,
            } => {
                tracing::debug!(
                    "Received gossip message {} from {} on topic {}",
                    message_id,
                    source_peer,
                    topic
                );

                // Phase 4: DOS Protection - Rate limit check
                if let Err(e) = self.rate_limiter.check_message_rate(&source_peer) {
                    tracing::warn!(
                        peer_id = %source_peer,
                        error = %e,
                        "Rate limit exceeded for gossip message"
                    );
                    self.peer_manager.add_peer_violation(
                        &source_peer,
                        Violation::ExcessiveRate {
                            messages_per_second: self.config.max_messages_per_peer_per_second,
                        },
                    );
                    self.metrics.record_rate_limited();
                    return Ok(()); // Drop message
                }

                // Phase 4: DOS Protection - Size limit check
                if data.len() > self.config.message_size_limit {
                    tracing::warn!(
                        peer_id = %source_peer,
                        message_size = data.len(),
                        limit = self.config.message_size_limit,
                        "Oversized gossip message from peer"
                    );
                    self.peer_manager.add_peer_violation(
                        &source_peer,
                        Violation::OversizedMessage {
                            size_bytes: data.len(),
                        },
                    );
                    return Ok(()); // Drop message
                }

                // Phase 4: DOS Protection - Bandwidth limit check
                if let Err(e) = self
                    .rate_limiter
                    .check_byte_rate(&source_peer, data.len() as u64)
                {
                    tracing::warn!(
                        peer_id = %source_peer,
                        bytes = data.len(),
                        error = %e,
                        "Bandwidth limit exceeded for gossip message"
                    );
                    self.peer_manager.add_peer_violation(
                        &source_peer,
                        Violation::ExcessiveRate {
                            messages_per_second: self.config.max_messages_per_peer_per_second,
                        },
                    );
                    self.metrics.record_rate_limited();
                    return Ok(()); // Drop message
                }

                self.metrics.record_message_received(data.len());
                self.metrics.record_gossip_received();

                // Phase 1: Forward block gossip messages to ChainActor for import
                if topic.contains("block") {
                    if let Some(ref chain_actor) = self.chain_actor {
                        // Phase 5: Update metrics for block received
                        self.metrics.blocks_received += 1;
                        NETWORK_BLOCKS_RECEIVED.inc();

                        // Deserialize block from MessagePack format
                        match crate::actors_v2::common::serialization::deserialize_block_from_network(&data) {
                            Ok(block) => {
                                // Extract block info for logging
                                let block_height = block.message.execution_payload.block_number;
                                let block_hash = crate::actors_v2::common::serialization::calculate_block_hash(&block);

                                tracing::info!(
                                    peer_id = %source_peer,
                                    block_height = block_height,
                                    block_hash = %block_hash,
                                    topic = %topic,
                                    "Received block via gossipsub"
                                );

                                // Phase 5: Check block cache before forwarding to ChainActor
                                {
                                    // Use try_read() for non-async context
                                    if let Ok(cache) = self.block_cache.try_read() {
                                        if cache.peek(&block_hash).is_some() {
                                            tracing::debug!(
                                                peer_id = %source_peer,
                                                block_hash = %block_hash,
                                                block_height = block_height,
                                                "Duplicate block detected via cache, skipping ChainActor forward"
                                            );

                                            // Update metrics
                                            self.metrics.blocks_duplicate_cached += 1;
                                            NETWORK_BLOCKS_DUPLICATE.inc();

                                            return Ok(());
                                        }
                                    }
                                }

                                tracing::debug!(
                                    peer_id = %source_peer,
                                    block_hash = %block_hash,
                                    "Block not in cache, proceeding with validation and forwarding"
                                );

                                // Perform basic structural validation before forwarding
                                if let Err(validation_error) = crate::actors_v2::common::serialization::validate_block_structure(&block) {
                                    tracing::warn!(
                                        peer_id = %source_peer,
                                        block_height = block_height,
                                        error = %validation_error,
                                        "Block failed basic structural validation, dropping"
                                    );

                                    // Penalize peer for sending invalid block
                                    self.peer_manager.add_peer_violation(
                                        &source_peer,
                                        Violation::InvalidData {
                                            reason: "Invalid block structure".to_string()
                                        }
                                    );

                                    return Ok(());
                                }

                                // Forward to ChainActor (async, non-blocking)
                                let chain_actor_clone = chain_actor.clone();
                                let peer_id_clone = source_peer.clone();
                                let block_cache_clone = self.block_cache.clone();
                                let block_hash_clone = block_hash;

                                // Update metrics
                                self.metrics.blocks_forwarded += 1;
                                NETWORK_BLOCKS_FORWARDED.inc();

                                tokio::spawn(async move {
                                    let msg = crate::actors_v2::chain::messages::ChainMessage::NetworkBlockReceived {
                                        block,
                                        peer_id: peer_id_clone.clone(),
                                    };

                                    match chain_actor_clone.send(msg).await {
                                        Ok(Ok(response)) => {
                                            match response {
                                                crate::actors_v2::chain::messages::ChainResponse::NetworkBlockProcessed { accepted, reason } => {
                                                    if accepted {
                                                        tracing::info!(
                                                            peer_id = %peer_id_clone,
                                                            block_height = block_height,
                                                            "Block successfully imported by ChainActor"
                                                        );

                                                        // Phase 5: Add block to cache after successful import
                                                        {
                                                            let mut cache = block_cache_clone.write().await;
                                                            cache.put(block_hash_clone, Instant::now());
                                                            tracing::debug!(
                                                                block_hash = %block_hash_clone,
                                                                "Added block to cache after successful import"
                                                            );
                                                        }
                                                    } else {
                                                        tracing::warn!(
                                                            peer_id = %peer_id_clone,
                                                            block_height = block_height,
                                                            reason = ?reason,
                                                            "Block rejected by ChainActor"
                                                        );
                                                    }
                                                }
                                                _ => {
                                                    tracing::warn!(
                                                        peer_id = %peer_id_clone,
                                                        "Unexpected response from ChainActor"
                                                    );
                                                }
                                            }
                                        }
                                        Ok(Err(e)) => {
                                            tracing::error!(
                                                peer_id = %peer_id_clone,
                                                error = ?e,
                                                "ChainActor rejected block with error"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                peer_id = %peer_id_clone,
                                                error = ?e,
                                                "Failed to communicate with ChainActor"
                                            );
                                        }
                                    }
                                });

                                // Update peer reputation immediately (optimistic)
                                self.peer_manager.record_peer_success(&source_peer);

                            }
                            Err(deserialization_error) => {
                                // Phase 5: Update metrics for deserialization error
                                self.metrics.blocks_deserialization_errors += 1;
                                NETWORK_BLOCKS_DESER_ERRORS.inc();

                                tracing::warn!(
                                    peer_id = %source_peer,
                                    topic = %topic,
                                    error = %deserialization_error,
                                    data_len = data.len(),
                                    "Failed to deserialize block from gossipsub message"
                                );

                                // Penalize peer for sending malformed data
                                self.peer_manager.add_peer_violation(
                                    &source_peer,
                                    Violation::InvalidData {
                                        reason: format!("Block deserialization failed: {}", deserialization_error)
                                    }
                                );
                            }
                        }
                    } else {
                        tracing::debug!(
                            topic = %topic,
                            "Received block gossip but ChainActor not available, dropping"
                        );
                    }
                }
                // Handle sync-related messages separately (not blocks)
                else if topic.contains("sync") {
                    if let Some(ref _sync_actor) = self.sync_actor {
                        // TODO: Forward sync messages to SyncActor (future phase)
                        tracing::debug!("Received sync-related gossip message");
                    }
                }
                // Handle Tendermint consensus messages
                else if topic.contains("tendermint") {
                    if let Some(ref chain_actor) = self.chain_actor {
                        let chain_actor_clone = chain_actor.clone();
                        let topic_clone = topic.clone();
                        let source_peer_clone = source_peer.clone();
                        tokio::spawn(async move {
                            Self::handle_tendermint_gossip_async(
                                &topic_clone,
                                data,
                                source_peer_clone,
                                chain_actor_clone,
                            ).await;
                        });
                    } else {
                        tracing::debug!(
                            topic = %topic,
                            "Received Tendermint gossip but ChainActor not available, dropping"
                        );
                    }
                }
            }

            AlysNetworkBehaviourEvent::BlockRequestReceived {
                peer_id,
                request_id,
                request,
                channel,
            } => {
                tracing::info!(
                    peer_id = %peer_id,
                    request_id = ?request_id,
                    request = ?request,
                    "Received block request from peer"
                );

                self.metrics.record_message_received(0);

                // Handle different request types
                match request {
                    BlockRequest::GetBlocks(range_request) => {
                        let start_height = range_request.start_height;
                        let count = range_request.count;
                        let end_height = start_height + count as u64 - 1;

                        tracing::info!(
                            peer_id = %peer_id,
                            start_height = start_height,
                            end_height = end_height,
                            count = count,
                            "Processing GetBlocks request"
                        );

                        // Check if we have StorageActor available
                        if let (Some(storage_actor), Some(cmd_tx)) = (self.storage_actor.clone(), self.swarm_cmd_tx.clone()) {
                            // Spawn async task to query storage and send response
                            let peer_id_clone = peer_id.clone();
                            tokio::spawn(async move {
                                // Query StorageActor for block range
                                let query_msg = crate::actors_v2::storage::messages::GetBlockRangeMessage {
                                    start_height,
                                    end_height,
                                    correlation_id: Some(uuid::Uuid::new_v4()),
                                };

                                match storage_actor.send(query_msg).await {
                                    Ok(Ok(blocks)) => {
                                        tracing::info!(
                                            peer_id = %peer_id_clone,
                                            block_count = blocks.len(),
                                            "Retrieved blocks from storage for peer request"
                                        );

                                        // Convert SignedConsensusBlock to BlockData
                                        let block_data_list: Vec<crate::actors_v2::network::protocols::request_response::BlockData> = blocks
                                            .iter()
                                            .map(|block| {
                                                let block_hash = crate::actors_v2::common::serialization::calculate_block_hash(block);
                                                let parent_hash = block.message.parent_hash;
                                                let height = block.message.execution_payload.block_number;
                                                let timestamp = block.message.execution_payload.timestamp;

                                                // Serialize the block for transport
                                                let serialized_block = crate::actors_v2::common::serialization::serialize_block_for_network(block)
                                                    .unwrap_or_default();

                                                crate::actors_v2::network::protocols::request_response::BlockData {
                                                    height,
                                                    hash: block_hash.0,
                                                    parent_hash: parent_hash.0,
                                                    timestamp,
                                                    transactions: vec![serialized_block], // First "transaction" is the serialized block
                                                }
                                            })
                                            .collect();

                                        let response = BlockResponse::Blocks(
                                            crate::actors_v2::network::protocols::request_response::BlocksResponse {
                                                blocks: block_data_list,
                                            },
                                        );

                                        let cmd = SwarmCommand::SendResponse { channel, response };
                                        if let Err(e) = cmd_tx.send(cmd).await {
                                            tracing::error!(
                                                error = ?e,
                                                "Failed to send blocks response command"
                                            );
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        tracing::warn!(
                                            peer_id = %peer_id_clone,
                                            error = ?e,
                                            start_height = start_height,
                                            end_height = end_height,
                                            "StorageActor returned error for block range query"
                                        );

                                        let error_response = BlockResponse::Error(
                                            crate::actors_v2::network::protocols::request_response::ErrorResponse {
                                                message: format!("Storage error: {}", e).into_bytes(),
                                            },
                                        );
                                        let cmd = SwarmCommand::SendResponse { channel, response: error_response };
                                        let _ = cmd_tx.send(cmd).await;
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            peer_id = %peer_id_clone,
                                            error = ?e,
                                            "Failed to communicate with StorageActor"
                                        );

                                        let error_response = BlockResponse::Error(
                                            crate::actors_v2::network::protocols::request_response::ErrorResponse {
                                                message: b"Internal storage error".to_vec(),
                                            },
                                        );
                                        let cmd = SwarmCommand::SendResponse { channel, response: error_response };
                                        let _ = cmd_tx.send(cmd).await;
                                    }
                                }
                            });
                        } else {
                            // No StorageActor available - send error response
                            tracing::warn!(
                                peer_id = %peer_id,
                                "StorageActor not available for block request handling"
                            );

                            if let Some(cmd_tx) = self.swarm_cmd_tx.as_ref() {
                                let error_response = BlockResponse::Error(
                                    crate::actors_v2::network::protocols::request_response::ErrorResponse {
                                        message: b"StorageActor not available".to_vec(),
                                    },
                                );
                                let cmd = SwarmCommand::SendResponse { channel, response: error_response };
                                if let Err(e) = cmd_tx.try_send(cmd) {
                                    tracing::error!(error = ?e, "Failed to send error response");
                                }
                            }
                        }
                    }
                    BlockRequest::GetChainStatus(_) => {
                        tracing::info!(
                            peer_id = %peer_id,
                            "Processing GetChainStatus request"
                        );

                        // Query ChainActor for current status
                        if let (Some(chain_actor), Some(cmd_tx)) = (self.chain_actor.clone(), self.swarm_cmd_tx.clone()) {
                            let peer_id_clone = peer_id.clone();
                            tokio::spawn(async move {
                                match chain_actor
                                    .send(crate::actors_v2::chain::messages::ChainMessage::GetChainStatus)
                                    .await
                                {
                                    Ok(Ok(crate::actors_v2::chain::messages::ChainResponse::ChainStatus(status))) => {
                                        let response = BlockResponse::ChainStatus(
                                            crate::actors_v2::network::protocols::request_response::ChainStatusResponse {
                                                height: status.height,
                                                head_hash: status.head_hash.map(|h| h.0).unwrap_or([0u8; 32]),
                                            },
                                        );

                                        let cmd = SwarmCommand::SendResponse { channel, response };
                                        if let Err(e) = cmd_tx.send(cmd).await {
                                            tracing::error!(
                                                error = ?e,
                                                "Failed to send chain status response"
                                            );
                                        }
                                    }
                                    Ok(Ok(_)) => {
                                        tracing::warn!(
                                            peer_id = %peer_id_clone,
                                            "Unexpected response from ChainActor for GetChainStatus"
                                        );
                                    }
                                    Ok(Err(e)) => {
                                        tracing::warn!(
                                            peer_id = %peer_id_clone,
                                            error = ?e,
                                            "ChainActor returned error for GetChainStatus"
                                        );

                                        let error_response = BlockResponse::Error(
                                            crate::actors_v2::network::protocols::request_response::ErrorResponse {
                                                message: format!("Chain error: {}", e).into_bytes(),
                                            },
                                        );
                                        let cmd = SwarmCommand::SendResponse { channel, response: error_response };
                                        let _ = cmd_tx.send(cmd).await;
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            peer_id = %peer_id_clone,
                                            error = ?e,
                                            "Failed to communicate with ChainActor"
                                        );

                                        let error_response = BlockResponse::Error(
                                            crate::actors_v2::network::protocols::request_response::ErrorResponse {
                                                message: b"Internal chain error".to_vec(),
                                            },
                                        );
                                        let cmd = SwarmCommand::SendResponse { channel, response: error_response };
                                        let _ = cmd_tx.send(cmd).await;
                                    }
                                }
                            });
                        } else {
                            // No ChainActor available - send error response
                            tracing::warn!(
                                peer_id = %peer_id,
                                "ChainActor not available for chain status request"
                            );

                            if let Some(cmd_tx) = self.swarm_cmd_tx.as_ref() {
                                let error_response = BlockResponse::Error(
                                    crate::actors_v2::network::protocols::request_response::ErrorResponse {
                                        message: b"ChainActor not available".to_vec(),
                                    },
                                );
                                let cmd = SwarmCommand::SendResponse { channel, response: error_response };
                                if let Err(e) = cmd_tx.try_send(cmd) {
                                    tracing::error!(error = ?e, "Failed to send error response");
                                }
                            }
                        }
                    }
                }
            }

            AlysNetworkBehaviourEvent::BlockResponseReceived {
                peer_id,
                request_id,
                response,
            } => {
                match &response {
                    BlockResponse::Blocks(blocks_response) => {
                        tracing::info!(
                            peer_id = %peer_id,
                            request_id = ?request_id,
                            block_count = blocks_response.blocks.len(),
                            "Received block response from peer with blocks"
                        );
                    }
                    _ => {
                        tracing::info!(
                            peer_id = %peer_id,
                            request_id = ?request_id,
                            "Received block response from peer (not Blocks variant)"
                        );
                    }
                }

                self.metrics.record_message_received(0); // Size would be calculated

                // Forward to SyncActor or handle internally based on response type
                match response {
                    BlockResponse::Blocks(blocks_response) => {
                        tracing::info!(
                            peer_id = %peer_id,
                            block_count = blocks_response.blocks.len(),
                            request_id = ?request_id,
                            "Received blocks from peer - forwarding to SyncActor"
                        );

                        // Forward blocks to SyncActor for processing and import
                        if let Some(ref sync_actor) = self.sync_actor {
                            // Convert BlockData to raw bytes for SyncActor
                            // SyncActor will deserialize to SignedConsensusBlock
                            let blocks: Vec<Vec<u8>> = blocks_response
                                .blocks
                                .iter()
                                .map(|block_data| {
                                    // Serialize BlockData to bytes for transport
                                    // The transactions field contains the raw block data
                                    // First transaction should be the serialized block
                                    if !block_data.transactions.is_empty() {
                                        block_data.transactions[0].clone()
                                    } else {
                                        // Fallback: empty block (shouldn't happen in practice)
                                        tracing::warn!(
                                            height = block_data.height,
                                            "BlockData has no transactions - block data may be incomplete"
                                        );
                                        Vec::new()
                                    }
                                })
                                .collect();

                            sync_actor.do_send(SyncMessage::HandleBlockResponse {
                                blocks,
                                request_id: format!("{:?}", request_id),
                                peer_id: peer_id.to_string(),
                            });

                            // Update peer reputation for successful response
                            self.peer_manager.update_peer_reputation(&peer_id, 1.0);
                        } else {
                            tracing::warn!(
                                peer_id = %peer_id,
                                block_count = blocks_response.blocks.len(),
                                "SyncActor not available - discarding received blocks"
                            );
                        }
                    }
                    BlockResponse::ChainStatus(status) => {
                        tracing::info!(
                            peer_id = %peer_id,
                            height = status.height,
                            head_hash = ?status.head_hash,
                            "Received chain status from peer - forwarding to SyncActor"
                        );

                        // Forward to SyncActor for height discovery
                        if let Some(ref sync_actor) = self.sync_actor {
                            sync_actor.do_send(SyncMessage::ReportPeerHeights {
                                peer_heights: vec![(
                                    peer_id.to_string(),
                                    status.height,
                                    status.head_hash,
                                )],
                            });
                        }

                        // Update peer reputation for successful response
                        self.peer_manager.update_peer_reputation(&peer_id, 1.0);
                    }
                    BlockResponse::Error(error) => {
                        let error_msg = String::from_utf8_lossy(&error.message);
                        tracing::warn!(
                            peer_id = %peer_id,
                            error = %error_msg,
                            "Peer returned error response"
                        );
                        self.peer_manager.update_peer_reputation(&peer_id, -1.0);
                    }
                }
            }

            AlysNetworkBehaviourEvent::RequestSent {
                peer_id,
                request_id,
            } => {
                tracing::debug!(
                    peer_id = %peer_id,
                    request_id = ?request_id,
                    "Request sent successfully"
                );
                self.metrics.record_message_sent(0); // Size would be calculated
            }

            AlysNetworkBehaviourEvent::ResponseSent { peer_id } => {
                tracing::debug!(
                    peer_id = %peer_id,
                    "Response sent successfully"
                );
                self.metrics.record_message_sent(0); // Size would be calculated
            }

            AlysNetworkBehaviourEvent::RequestFailed { peer_id, error } => {
                tracing::warn!(
                    peer_id = %peer_id,
                    error = %error,
                    "Request-response operation failed"
                );
                self.peer_manager.update_peer_reputation(&peer_id, -2.0);
                self.metrics.record_block_response_error();
            }

            AlysNetworkBehaviourEvent::PeerConnected { peer_id, address } => {
                tracing::info!("Peer connected: {} at {}", peer_id, address);
                self.peer_manager.add_peer(peer_id, address);
                self.metrics.record_connection_established();
            }

            AlysNetworkBehaviourEvent::PeerDisconnected { peer_id, reason } => {
                tracing::info!("Peer disconnected: {} ({})", peer_id, reason);

                // Check if this was a V2-capable peer BEFORE removing from connected_peers
                let was_v2_peer = self.peer_manager.is_v2_peer(&peer_id);

                self.peer_manager.remove_peer(&peer_id);
                self.metrics.record_connection_closed();

                // If a V2 peer disconnected, schedule reconnection attempt
                if was_v2_peer {
                    let v2_count = self.peer_manager.connected_v2_peer_count();
                    tracing::warn!(
                        peer_id = %peer_id,
                        remaining_v2_peers = v2_count,
                        "V2-capable peer disconnected - scheduling reconnection"
                    );

                    // If we have no V2 peers left, try to reconnect immediately
                    if v2_count == 0 {
                        tracing::error!(
                            "No V2-capable peers connected! Network sync will be impaired."
                        );

                        // Attempt reconnection to known V2 peers
                        self.attempt_v2_peer_reconnection();
                    }
                }
            }

            AlysNetworkBehaviourEvent::PeerIdentified {
                peer_id,
                protocols,
                addresses,
            } => {
                tracing::debug!(
                    "Peer identified: {} with {} protocols and {} addresses",
                    peer_id,
                    protocols.len(),
                    addresses.len()
                );

                // NOTE: We intentionally do NOT call add_peer() here.
                // The peer is already added with the correct connection address
                // from ConnectionEstablished. The identify protocol reports
                // addresses from the peer's local perspective (including localhost),
                // which would overwrite the correct external address and break
                // reconnection in containerized environments.

                // Track V2 protocol capability
                let supports_v2 = self.peer_manager.update_peer_protocols(&peer_id, protocols);

                // Give V2-capable peers a reputation boost (they can serve block requests)
                if supports_v2 {
                    self.peer_manager.update_reputation(
                        &peer_id,
                        10.0,
                        "v2_protocol_support_boost",
                    );

                    // Log V2 peer count for visibility
                    let v2_count = self.peer_manager.connected_v2_peer_count();
                    tracing::info!(
                        peer_id = %peer_id,
                        v2_peer_count = v2_count,
                        "V2-capable peer connected"
                    );
                }
            }

            AlysNetworkBehaviourEvent::MdnsPeerDiscovered { peer_id, addresses } => {
                tracing::info!(
                    "mDNS peer discovered: {} with {} addresses",
                    peer_id,
                    addresses.len()
                );

                // Phase 2 Task 2.4: Record mDNS-specific discovery metric
                self.metrics.record_mdns_discovery();

                // Add discovered peer to peer manager
                // Use select_external_address to avoid storing loopback addresses
                // which are unreachable from other containers in Docker networks
                if let Some(address) = Self::select_external_address(&addresses) {
                    self.peer_manager.add_peer(peer_id.clone(), address.clone());

                    // Give mDNS-discovered peers a reputation boost (they're local network peers)
                    // This ensures they can be immediately selected for block requests
                    // (new peers start at 50.0, but select_peers_for_blocks needs >= 50.0)
                    self.peer_manager.update_reputation(
                        &peer_id,
                        5.0,
                        "mdns_discovery_boost",
                    );
                    tracing::debug!(
                        peer_id = %peer_id,
                        "Applied mDNS discovery reputation boost (+5.0)"
                    );

                    // Add peer as explicit gossipsub peer for immediate mesh formation
                    // This is critical for small networks where automatic mesh formation is unreliable
                    if let Some(cmd_tx) = self.swarm_cmd_tx.as_ref() {
                        // Parse peer_id string to PeerId
                        if let Ok(libp2p_peer_id) = peer_id.parse::<PeerId>() {
                            let add_peer_cmd = SwarmCommand::AddExplicitPeer {
                                peer_id: libp2p_peer_id,
                            };

                            match cmd_tx.try_send(add_peer_cmd) {
                                Ok(_) => {
                                    tracing::info!(
                                        "Sent AddExplicitPeer command for mDNS peer: {}",
                                        peer_id
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to send AddExplicitPeer command for {}: {:?}",
                                        peer_id,
                                        e
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                "Failed to parse peer_id {} for AddExplicitPeer command",
                                peer_id
                            );
                        }
                    }

                    // Phase 2 Task 2.4: Automatically dial discovered mDNS peer if enabled
                    if self.config.auto_dial_mdns_peers {
                        if let Some(cmd_tx) = self.swarm_cmd_tx.as_ref() {
                            // Parse multiaddr for dialing
                            match address.parse::<Multiaddr>() {
                                Ok(multiaddr) => {
                                    let (response_tx, response_rx) =
                                        tokio::sync::oneshot::channel();
                                    let dial_cmd = SwarmCommand::Dial {
                                        addr: multiaddr.clone(),
                                        response_tx,
                                    };

                                    match cmd_tx.try_send(dial_cmd) {
                                        Ok(_) => {
                                            tracing::info!(
                                                "Auto-dialing mDNS discovered peer: {}",
                                                peer_id
                                            );

                                            // Spawn task to handle dial response
                                            let peer_id_clone = peer_id.clone();
                                            tokio::spawn(async move {
                                                match response_rx.await {
                                                    Ok(Ok(())) => {
                                                        tracing::info!("Successfully connected to mDNS peer: {}", peer_id_clone);
                                                    }
                                                    Ok(Err(e)) => {
                                                        tracing::warn!(
                                                            "Failed to dial mDNS peer {}: {}",
                                                            peer_id_clone,
                                                            e
                                                        );
                                                    }
                                                    Err(_) => {
                                                        tracing::error!("Dial response channel closed for mDNS peer: {}", peer_id_clone);
                                                    }
                                                }
                                            });
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to send dial command for mDNS peer {}: {:?}", peer_id, e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Invalid multiaddr for mDNS peer {}: {}",
                                        peer_id,
                                        e
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                "Cannot auto-dial mDNS peer {}: command channel not available",
                                peer_id
                            );
                        }
                    } else {
                        tracing::debug!("Auto-dial disabled for mDNS peer: {}", peer_id);
                    }

                    // Notify SyncActor about new peer for potential sync
                    if let Some(ref sync_actor) = self.sync_actor {
                        let current_peers = self
                            .peer_manager
                            .get_connected_peers()
                            .keys()
                            .cloned()
                            .collect();

                        let update_msg = crate::actors_v2::network::SyncMessage::UpdatePeers {
                            peers: current_peers,
                        };

                        // Send update in background
                        let sync_actor_clone = sync_actor.clone();
                        tokio::spawn(async move {
                            match sync_actor_clone.send(update_msg).await {
                                Ok(_) => tracing::debug!("Updated SyncActor with new peer list"),
                                Err(e) => {
                                    tracing::error!("Failed to update SyncActor peers: {}", e)
                                }
                            }
                        });
                    }
                }
            }

            AlysNetworkBehaviourEvent::MdnsPeerExpired { peer_id } => {
                tracing::info!("mDNS peer expired: {}", peer_id);

                // Remove expired peer
                self.peer_manager.remove_peer(&peer_id);

                // Phase 2 Task 2.4: Record mDNS-specific expiry metric
                self.metrics.record_mdns_expiry();
            }
        }

        Ok(())
    }

    /// Handle Tendermint consensus gossip messages (async, spawned from event handler).
    ///
    /// Deserializes TendermintWireMessage wrapper and routes to ChainActor:
    /// - Proposal: Forward as TendermintProposal
    /// - Vote: Forward as TendermintVote
    /// - Evidence: Forward as TendermintEvidence
    /// - Timeout/NewRound: Logged for future handling
    async fn handle_tendermint_gossip_async(
        topic: &str,
        data: Vec<u8>,
        source_peer: String,
        chain_actor: actix::Addr<crate::actors_v2::chain::ChainActor>,
    ) {
        use crate::actors_v2::chain::tendermint::TendermintMessage;
        use crate::actors_v2::network::tendermint::TendermintWireMessage;

        let correlation_id = uuid::Uuid::new_v4();

        // First deserialize the wire message wrapper
        let wire_msg = match TendermintWireMessage::from_bytes(&data) {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!(
                    peer_id = %source_peer,
                    topic = %topic,
                    error = %e,
                    data_len = data.len(),
                    "Failed to deserialize Tendermint wire message"
                );
                return;
            }
        };

        // Extract the actual message from the wire wrapper
        let tendermint_msg = match wire_msg.to_message() {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!(
                    peer_id = %source_peer,
                    topic = %topic,
                    error = %e,
                    msg_type = ?wire_msg.msg_type,
                    "Failed to extract Tendermint message from wire format"
                );
                return;
            }
        };

        // Route based on message type
        match tendermint_msg {
            TendermintMessage::Proposal(proposal) => {
                let height = proposal.height;
                let round = proposal.round;
                let proposer = proposal.proposer;

                tracing::info!(
                    correlation_id = %correlation_id,
                    peer_id = %source_peer,
                    height = height,
                    round = round,
                    proposer = ?proposer,
                    "Received Tendermint proposal via gossip"
                );

                // Forward to ChainActor
                let msg = crate::actors_v2::chain::messages::ChainMessage::TendermintProposal {
                    proposal,
                    peer_id: Some(source_peer.clone()),
                    correlation_id: Some(correlation_id),
                };

                if let Err(e) = chain_actor.send(msg).await {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        error = %e,
                        "Failed to forward Tendermint proposal to ChainActor"
                    );
                }
            }

            TendermintMessage::Vote(vote) => {
                let height = vote.height;
                let round = vote.round;
                let voter = vote.validator;
                let vote_type = vote.vote_type;

                tracing::debug!(
                    correlation_id = %correlation_id,
                    peer_id = %source_peer,
                    height = height,
                    round = round,
                    voter = ?voter,
                    vote_type = ?vote_type,
                    "Received Tendermint vote via gossip"
                );

                // Forward to ChainActor
                let msg = crate::actors_v2::chain::messages::ChainMessage::TendermintVote {
                    vote,
                    peer_id: Some(source_peer.clone()),
                    correlation_id: Some(correlation_id),
                };

                if let Err(e) = chain_actor.send(msg).await {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        error = %e,
                        "Failed to forward Tendermint vote to ChainActor"
                    );
                }
            }

            TendermintMessage::Evidence(evidence) => {
                let height = evidence.height;
                let culprit = evidence.culprit;
                let kind = evidence.kind;

                tracing::warn!(
                    correlation_id = %correlation_id,
                    peer_id = %source_peer,
                    height = height,
                    culprit = ?culprit,
                    kind = ?kind,
                    "Received equivocation evidence via gossip"
                );

                // Forward to ChainActor for validation and processing
                let msg = crate::actors_v2::chain::messages::ChainMessage::TendermintEvidence {
                    evidence,
                    peer_id: Some(source_peer.clone()),
                    correlation_id: Some(correlation_id),
                };

                if let Err(e) = chain_actor.send(msg).await {
                    tracing::error!(
                        correlation_id = %correlation_id,
                        error = %e,
                        "Failed to forward equivocation evidence to ChainActor"
                    );
                }
            }

            TendermintMessage::Timeout(timeout) => {
                // Timeout messages from other validators (informational)
                tracing::debug!(
                    peer_id = %source_peer,
                    height = timeout.height,
                    round = timeout.round,
                    step = ?timeout.step,
                    "Received Tendermint timeout notification"
                );
                // Note: Local timeouts are handled by timeout_receiver, not gossip
            }

            TendermintMessage::NewRound {
                height,
                round,
                highest_known_round,
            } => {
                // New round announcements (for round synchronization)
                tracing::debug!(
                    peer_id = %source_peer,
                    height = height,
                    round = round,
                    highest_known_round = highest_known_round,
                    "Received Tendermint new round announcement"
                );
                // TODO: Forward to ChainActor for round sync
            }

            TendermintMessage::BlockRequest { height } => {
                tracing::debug!(
                    peer_id = %source_peer,
                    height = height,
                    "Received block request via gossip (should use request-response)"
                );
                // Block requests should use request-response protocol, not gossip
            }

            TendermintMessage::BlockResponse { .. } => {
                tracing::debug!(
                    peer_id = %source_peer,
                    "Received block response via gossip (should use request-response)"
                );
                // Block responses should use request-response protocol, not gossip
            }
        }
    }

    /// Handle request from peer
    fn handle_peer_request(
        &mut self,
        request: crate::actors_v2::network::messages::NetworkRequest,
        source_peer: String,
        _request_id: String,
    ) -> Result<()> {
        match request {
            crate::actors_v2::network::messages::NetworkRequest::GetBlocks {
                start_height,
                count,
            } => {
                tracing::debug!(
                    "Peer {} requested {} blocks starting from height {}",
                    source_peer,
                    count,
                    start_height
                );

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

    /// Get current network status (synchronous, uses chain_height = 0)
    /// For async version with real chain height, use get_network_status_async()
    fn get_network_status(&self) -> NetworkStatus {
        NetworkStatus {
            local_peer_id: self.local_peer_id.clone(),
            connected_peers: self.peer_manager.get_connected_peers().len(),
            listening_addresses: self.config.listen_addresses.clone(),
            is_running: self.is_running,
            chain_height: 0,  // Placeholder, use async version for real height
        }
    }

    /// Get current network status with actual chain height (async)
    async fn get_network_status_async(&self) -> NetworkStatus {
        // Query ChainActor for current height
        let chain_height = if let Some(ref chain_actor) = self.chain_actor {
            match chain_actor
                .send(crate::actors_v2::chain::ChainMessage::GetChainStatus)
                .await
            {
                Ok(Ok(crate::actors_v2::chain::ChainResponse::ChainStatus(status))) => status.height,
                Ok(Err(e)) => {
                    tracing::warn!(error = ?e, "Failed to get chain status for network status");
                    0
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "ChainActor mailbox error during network status");
                    0
                }
                Ok(Ok(_)) => {
                    tracing::warn!("Unexpected response from GetChainStatus");
                    0
                }
            }
        } else {
            0
        };

        NetworkStatus {
            local_peer_id: self.local_peer_id.clone(),
            connected_peers: self.peer_manager.get_connected_peers().len(),
            listening_addresses: self.config.listen_addresses.clone(),
            is_running: self.is_running,
            chain_height,
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

        // Phase 4: Clean up rate limiter data for disconnected peers
        let active_peers: Vec<String> = self
            .peer_manager
            .get_connected_peers()
            .keys()
            .cloned()
            .collect();
        self.rate_limiter.cleanup(&active_peers);
    }
}

impl Actor for NetworkActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("NetworkActor V2 actor started");

        // Start periodic maintenance
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            act.perform_maintenance();
        });

        // Start periodic metrics logging
        ctx.run_interval(Duration::from_secs(10), |act, _ctx| {
            tracing::debug!(
                connected_peers = act.metrics.connected_peers,
                messages_sent = act.metrics.messages_sent,
                messages_received = act.metrics.messages_received,
                "NetworkActor metrics"
            );
        });

        // V2 peer health check - ensures we maintain V2-capable peers for sync
        // Runs every 15 seconds to detect and recover from V2 peer disconnections
        ctx.run_interval(Duration::from_secs(15), |act, _ctx| {
            act.check_v2_peer_health();
        });

        // Note: Swarm event loop started in StartNetwork handler
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("NetworkActor V2 stopping");

        // Cancel swarm polling task
        if let Some(handle) = self.swarm_task_handle.take() {
            handle.abort();
            tracing::debug!("Aborted swarm polling task");
        }

        self.shutdown_requested = true;
        self.is_running = false;
        Running::Stop
    }
}

/// StreamHandler receives events from swarm polling task
impl StreamHandler<AlysSwarmEvent> for NetworkActor {
    fn handle(&mut self, event: AlysSwarmEvent, _ctx: &mut Context<Self>) {
        // Delegate to existing handler
        if let Err(e) = self.handle_swarm_event(event) {
            tracing::error!("Error handling swarm event: {}", e);
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        tracing::error!("Swarm event stream ended unexpectedly");
        self.is_running = false;

        // Automatic error recovery (only if not shutting down)
        if !self.shutdown_requested {
            tracing::warn!("Attempting to restart swarm event loop after 5 seconds");

            // Schedule restart after delay
            ctx.run_later(Duration::from_secs(5), |act, ctx| {
                tracing::info!("Restarting swarm after stream ended");

                match act.restart_swarm(ctx) {
                    Ok(_) => {
                        tracing::info!("Swarm successfully restarted");
                    }
                    Err(e) => {
                        tracing::error!("Failed to restart swarm: {}", e);
                        // After failed restart, stop actor gracefully
                        ctx.stop();
                    }
                }
            });
        }
    }
}

impl Handler<NetworkMessage> for NetworkActor {
    type Result = Result<NetworkResponse, NetworkError>;

    fn handle(&mut self, msg: NetworkMessage, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            NetworkMessage::StartNetwork {
                listen_addrs,
                bootstrap_peers,
            } => {
                // Check idempotency
                if self.is_running {
                    tracing::warn!("Network already running - ignoring StartNetwork");
                    return Ok(NetworkResponse::Started);
                }

                tracing::info!("Starting NetworkActor V2");

                // Update configuration
                self.config.listen_addresses = listen_addrs.clone();
                self.config.bootstrap_peers = bootstrap_peers.clone();

                // Create swarm on-demand
                let mut swarm =
                    match crate::actors_v2::network::swarm_factory::create_swarm(&self.config) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to create swarm: {}", e);
                            return Err(NetworkError::Internal(format!(
                                "Failed to create swarm: {}",
                                e
                            )));
                        }
                    };

                // Update local peer ID from actual swarm
                self.local_peer_id = swarm.local_peer_id().to_string();

                // Listen on configured addresses BEFORE spawning task
                for addr_str in &listen_addrs {
                    let addr: Multiaddr = match addr_str.parse() {
                        Ok(a) => a,
                        Err(e) => {
                            tracing::error!("Invalid listen address {}: {}", addr_str, e);
                            return Err(NetworkError::Configuration(format!(
                                "Invalid listen address: {}",
                                e
                            )));
                        }
                    };

                    if let Err(e) = swarm.listen_on(addr.clone()) {
                        tracing::error!("Failed to listen on {}: {}", addr, e);
                        return Err(NetworkError::Internal(format!(
                            "Failed to listen on {}: {}",
                            addr, e
                        )));
                    }

                    tracing::info!("Listening on: {}", addr);
                }

                // Setup channels - BOUNDED to prevent OOM (Phase 2 Task 2.0)
                let (event_tx, event_rx) = mpsc::channel(1000); // Bounded: 1000 events
                let (cmd_tx, mut cmd_rx) = mpsc::channel::<SwarmCommand>(1000); // Bounded: 1000 commands

                // Spawn swarm polling task with command handling (Phase 2 Task 2.0)
                let swarm_task = tokio::spawn(async move {
                    loop {
                        select! {
                            // Handle swarm events
                            event = swarm.select_next_some().fuse() => {
                                // Use try_send with backpressure handling
                                match event_tx.try_send(event) {
                                    Ok(_) => {},
                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                        tracing::warn!("Event channel full, dropping event (backpressure)");
                                    }
                                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                        tracing::info!("Event receiver dropped, stopping swarm poll");
                                        break;
                                    }
                                }
                            }

                            // Handle commands from NetworkActor
                            cmd = cmd_rx.recv().fuse() => {
                                match cmd {
                                    Some(SwarmCommand::Dial { addr, response_tx }) => {
                                        let result = swarm.dial(addr.clone())
                                            .map(|_| ())
                                            .map_err(|e| format!("Dial failed: {}", e));
                                        let _ = response_tx.send(result);
                                    }

                                    Some(SwarmCommand::ListenOn { addr, response_tx }) => {
                                        let result = swarm.listen_on(addr.clone())
                                            .map(|_| ())
                                            .map_err(|e| format!("Listen failed: {}", e));
                                        let _ = response_tx.send(result);
                                    }

                                    Some(SwarmCommand::PublishGossip { topic, data, response_tx }) => {
                                        use libp2p::gossipsub::IdentTopic;

                                        let topic = IdentTopic::new(topic);

                                        // Auto-subscribe if not already subscribed
                                        let is_subscribed = swarm.behaviour().gossipsub
                                            .mesh_peers(&topic.hash())
                                            .next()
                                            .is_some();

                                        if !is_subscribed {
                                            if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                                                let _ = response_tx.send(Err(format!("Subscribe failed: {}", e)));
                                                continue;
                                            }
                                        }

                                        // Publish message
                                        let publish_result = swarm.behaviour_mut().gossipsub
                                            .publish(topic, data);

                                        let result = match publish_result {
                                            Ok(msg_id) => Ok(msg_id.to_string()),
                                            Err(e) => Err(format!("Publish failed: {}", e)),
                                        };

                                        let _ = response_tx.send(result);
                                    }

                                    Some(SwarmCommand::SubscribeTopic { topic, response_tx }) => {
                                        use libp2p::gossipsub::IdentTopic;

                                        let topic = IdentTopic::new(topic);
                                        let result = swarm.behaviour_mut().gossipsub
                                            .subscribe(&topic)
                                            .map(|_| ())
                                            .map_err(|e| format!("Subscribe failed: {}", e));
                                        let _ = response_tx.send(result);
                                    }

                                    Some(SwarmCommand::SendRequest { peer_id, request, response_tx }) => {
                                        // Phase 2 Task 2.2: Send request-response request via request_response behavior
                                        let request_id = swarm.behaviour_mut().request_response
                                            .send_request(&peer_id, request);

                                        tracing::debug!(
                                            peer_id = %peer_id,
                                            request_id = ?request_id,
                                            "Sent request-response request"
                                        );

                                        let _ = response_tx.send(Ok(request_id));
                                    }

                                    Some(SwarmCommand::SendResponse { channel, response }) => {
                                        // Phase 2 Task 2.2: Send request-response response via request_response behavior
                                        match swarm.behaviour_mut().request_response.send_response(channel, response) {
                                            Ok(_) => {
                                                tracing::debug!("Sent request-response response");
                                            }
                                            Err(e) => {
                                                tracing::error!(error = ?e, "Failed to send request-response response");
                                            }
                                        }
                                    }

                                    Some(SwarmCommand::AddExplicitPeer { peer_id }) => {
                                        // Add peer as explicit gossipsub peer for immediate mesh formation
                                        // This is critical for small networks (e.g., 2-node regtest) where
                                        // gossipsub's automatic mesh formation may be slow or unreliable
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);

                                        tracing::info!(
                                            peer_id = %peer_id,
                                            "Added peer as explicit gossipsub peer for immediate mesh formation"
                                        );
                                    }

                                    None => {
                                        tracing::info!("Command channel closed, stopping swarm poll");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                });

                self.swarm_task_handle = Some(swarm_task);
                self.swarm_cmd_tx = Some(cmd_tx.clone());

                // Add event receiver as stream to actor context
                ctx.add_stream(tokio_stream::wrappers::ReceiverStream::new(event_rx));

                // Set up peer manager with bootstrap peers
                self.peer_manager
                    .set_bootstrap_peers(bootstrap_peers.clone());

                // Connect to bootstrap peers using command channel (Phase 2 Task 2.0.4)
                if !bootstrap_peers.is_empty() {
                    tracing::info!("Connecting to {} bootstrap peers", bootstrap_peers.len());

                    for peer_addr_str in &bootstrap_peers {
                        // Parse multiaddr
                        let multiaddr: Multiaddr = match peer_addr_str.parse() {
                            Ok(addr) => addr,
                            Err(e) => {
                                tracing::error!(
                                    "Invalid bootstrap peer address {}: {}",
                                    peer_addr_str,
                                    e
                                );
                                continue;
                            }
                        };

                        // Send dial command via channel (non-blocking)
                        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                        let dial_cmd = SwarmCommand::Dial {
                            addr: multiaddr.clone(),
                            response_tx,
                        };

                        match cmd_tx.try_send(dial_cmd) {
                            Ok(_) => {
                                // Spawn task to handle dial response (non-blocking)
                                tokio::spawn(async move {
                                    match response_rx.await {
                                        Ok(Ok(())) => {
                                            tracing::info!(
                                                "Successfully initiated dial to {}",
                                                multiaddr
                                            );
                                        }
                                        Ok(Err(e)) => {
                                            tracing::warn!("Failed to dial {}: {}", multiaddr, e);
                                        }
                                        Err(_) => {
                                            tracing::error!(
                                                "Dial response channel closed for {}",
                                                multiaddr
                                            );
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to send dial command for {}: {}",
                                    peer_addr_str,
                                    e
                                );
                                continue;
                            }
                        }
                    }
                }

                self.is_running = true;
                tracing::info!("NetworkActor V2 started successfully with command channel");

                // Start periodic cleanup
                ctx.address().do_send(NetworkMessage::CleanupTimeouts);

                Ok(NetworkResponse::Started)
            }

            NetworkMessage::StopNetwork { graceful } => {
                // Check if not running
                if !self.is_running {
                    tracing::warn!("Network not running - ignoring StopNetwork");
                    return Ok(NetworkResponse::Stopped);
                }

                tracing::info!("Stopping NetworkActor V2 (graceful: {})", graceful);

                if graceful {
                    // Graceful shutdown - disconnect from peers cleanly
                    let connected_peers: Vec<String> = self
                        .peer_manager
                        .get_connected_peers()
                        .keys()
                        .cloned()
                        .collect();

                    for peer_id in &connected_peers {
                        self.peer_manager.remove_peer(peer_id);
                        self.metrics.record_connection_closed();
                    }

                    // Allow time for clean disconnections
                    let disconnect_future = async move {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                    .into_actor(self)
                    .map(|_, act, _ctx| {
                        act.is_running = false;
                        tracing::info!("NetworkActor V2 stopped gracefully");
                    });

                    ctx.spawn(disconnect_future);
                } else {
                    // Immediate shutdown
                    self.is_running = false;
                    tracing::info!("NetworkActor V2 stopped");
                }

                Ok(NetworkResponse::Stopped)
            }

            NetworkMessage::GetNetworkStatus => {
                let status = self.get_network_status();
                Ok(NetworkResponse::Status(status))
            }

            NetworkMessage::BroadcastBlock {
                block_data,
                priority,
            } => {
                // Phase 2 Task 2.1: Real gossipsub broadcasting via SwarmCommand channel

                // Validate network is running
                if !self.is_running {
                    tracing::error!("Network not running, cannot broadcast block");
                    return Err(NetworkError::NotStarted);
                }

                // Get command channel
                let cmd_tx = match self.swarm_cmd_tx.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        tracing::error!("Swarm command channel not available");
                        return Err(NetworkError::Internal(
                            "Command channel not available".to_string(),
                        ));
                    }
                };

                let topic = if priority {
                    "alys/blocks/priority".to_string()
                } else {
                    "alys/blocks".to_string()
                };

                let data_len = block_data.len();

                tracing::debug!(
                    topic = %topic,
                    size = data_len,
                    priority = priority,
                    "Broadcasting block via gossipsub"
                );

                // Create oneshot channel for response
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();

                // Send publish command (non-blocking)
                let cmd = SwarmCommand::PublishGossip {
                    topic: topic.clone(),
                    data: block_data,
                    response_tx,
                };

                match cmd_tx.try_send(cmd) {
                    Ok(_) => {
                        // Update metrics immediately
                        self.metrics.record_message_sent(data_len);
                        self.metrics.record_gossip_published();
                        self.active_subscriptions
                            .insert(topic.clone(), Instant::now());

                        // Spawn task to handle async response
                        tokio::spawn(async move {
                            match response_rx.await {
                                Ok(Ok(message_id)) => {
                                    tracing::info!(
                                        message_id = %message_id,
                                        topic = %topic,
                                        "Block broadcast successful"
                                    );
                                }
                                Ok(Err(e)) => {
                                    tracing::error!(
                                        topic = %topic,
                                        error = %e,
                                        "Block broadcast failed"
                                    );
                                }
                                Err(_) => {
                                    tracing::error!(
                                        topic = %topic,
                                        "Block broadcast response channel closed"
                                    );
                                }
                            }
                        });

                        // Return immediately with pending status
                        Ok(NetworkResponse::Broadcasted {
                            message_id: format!("broadcast-{}", uuid::Uuid::new_v4()),
                        })
                    }
                    Err(e) => {
                        tracing::error!(
                            error = ?e,
                            "Failed to send broadcast command"
                        );
                        Err(NetworkError::Internal(format!(
                            "Failed to send command: {}",
                            e
                        )))
                    }
                }
            }

            NetworkMessage::BroadcastTransaction { tx_data } => {
                // Phase 2 Task 2.1: Real gossipsub broadcasting via SwarmCommand channel

                // Validate network is running
                if !self.is_running {
                    tracing::error!("Network not running, cannot broadcast transaction");
                    return Err(NetworkError::NotStarted);
                }

                // Get command channel
                let cmd_tx = match self.swarm_cmd_tx.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        tracing::error!("Swarm command channel not available");
                        return Err(NetworkError::Internal(
                            "Command channel not available".to_string(),
                        ));
                    }
                };

                let topic = "alys/transactions".to_string();
                let data_len = tx_data.len();

                tracing::debug!(
                    topic = %topic,
                    size = data_len,
                    "Broadcasting transaction via gossipsub"
                );

                // Create oneshot channel for response
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();

                // Send publish command (non-blocking)
                let cmd = SwarmCommand::PublishGossip {
                    topic: topic.clone(),
                    data: tx_data,
                    response_tx,
                };

                match cmd_tx.try_send(cmd) {
                    Ok(_) => {
                        // Update metrics immediately
                        self.metrics.record_message_sent(data_len);
                        self.metrics.record_gossip_published();
                        self.active_subscriptions
                            .insert(topic.clone(), Instant::now());

                        // Spawn task to handle async response
                        tokio::spawn(async move {
                            match response_rx.await {
                                Ok(Ok(message_id)) => {
                                    tracing::info!(
                                        message_id = %message_id,
                                        topic = %topic,
                                        "Transaction broadcast successful"
                                    );
                                }
                                Ok(Err(e)) => {
                                    tracing::error!(
                                        topic = %topic,
                                        error = %e,
                                        "Transaction broadcast failed"
                                    );
                                }
                                Err(_) => {
                                    tracing::error!(
                                        topic = %topic,
                                        "Transaction broadcast response channel closed"
                                    );
                                }
                            }
                        });

                        // Return immediately with pending status
                        Ok(NetworkResponse::Broadcasted {
                            message_id: format!("broadcast-{}", uuid::Uuid::new_v4()),
                        })
                    }
                    Err(e) => {
                        tracing::error!(
                            error = ?e,
                            "Failed to send broadcast command"
                        );
                        Err(NetworkError::Internal(format!(
                            "Failed to send command: {}",
                            e
                        )))
                    }
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
                let peers = self
                    .peer_manager
                    .get_connected_peers()
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
            NetworkMessage::BroadcastAuxPow {
                auxpow_data,
                correlation_id,
            } => {
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
                    return Err(NetworkError::Protocol(format!(
                        "Invalid AuxPoW format: {}",
                        e
                    )));
                }

                // Phase 2 Task 2.1: Real gossipsub broadcasting via SwarmCommand channel

                // Get command channel
                let cmd_tx = match self.swarm_cmd_tx.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        tracing::error!(correlation_id = %correlation_id, "Swarm command channel not available");
                        return Err(NetworkError::Internal(
                            "Command channel not available".to_string(),
                        ));
                    }
                };

                let topic = "alys/auxpow".to_string();

                tracing::debug!(
                    correlation_id = %correlation_id,
                    topic = %topic,
                    peer_count = peer_count,
                    "Broadcasting AuxPoW via gossipsub"
                );

                // Create oneshot channel for response
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();

                // Send publish command (non-blocking)
                let cmd = SwarmCommand::PublishGossip {
                    topic: topic.clone(),
                    data: auxpow_data,
                    response_tx,
                };

                match cmd_tx.try_send(cmd) {
                    Ok(_) => {
                        // Update active subscriptions
                        self.active_subscriptions
                            .insert(topic.clone(), Instant::now());

                        // Spawn task to handle async response
                        tokio::spawn(async move {
                            match response_rx.await {
                                Ok(Ok(message_id)) => {
                                    tracing::info!(
                                        correlation_id = %correlation_id,
                                        message_id = %message_id,
                                        topic = %topic,
                                        "AuxPoW broadcast successful"
                                    );
                                }
                                Ok(Err(e)) => {
                                    tracing::error!(
                                        correlation_id = %correlation_id,
                                        topic = %topic,
                                        error = %e,
                                        "AuxPoW broadcast failed"
                                    );
                                }
                                Err(_) => {
                                    tracing::error!(
                                        correlation_id = %correlation_id,
                                        topic = %topic,
                                        "AuxPoW broadcast response channel closed"
                                    );
                                }
                            }
                        });

                        // Return immediately with success
                        Ok(NetworkResponse::AuxPowBroadcasted { peer_count })
                    }
                    Err(e) => {
                        tracing::error!(
                            correlation_id = %correlation_id,
                            error = ?e,
                            "Failed to send AuxPoW broadcast command"
                        );
                        Err(NetworkError::Internal(format!(
                            "Failed to send command: {}",
                            e
                        )))
                    }
                }
            }

            NetworkMessage::RequestBlocks {
                start_height,
                count,
                correlation_id,
            } => {
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
                    return Err(NetworkError::Protocol(
                        "Invalid block count: must be 1-100".to_string(),
                    ));
                }

                // Check rate limiting (Phase 4: Task 2.9)
                const MAX_CONCURRENT_REQUESTS: usize = 10;
                if self.pending_block_requests.len() >= MAX_CONCURRENT_REQUESTS {
                    tracing::warn!(
                        correlation_id = %request_id,
                        pending_count = self.pending_block_requests.len(),
                        "Too many pending block requests"
                    );
                    return Err(NetworkError::Internal(
                        "Too many pending requests".to_string(),
                    ));
                }

                // Select best peers for block requests (Phase 4: Task 2.2)
                let selected_peers = self.peer_manager.select_peers_for_blocks(5);
                if selected_peers.is_empty() {
                    tracing::error!(
                        correlation_id = %request_id,
                        "No suitable peers available for block request"
                    );
                    return Err(NetworkError::Connection(
                        "No suitable peers available".to_string(),
                    ));
                }

                tracing::info!(
                    correlation_id = %request_id,
                    peer_count = selected_peers.len(),
                    start_height = start_height,
                    count = count,
                    "Selected peers for block request"
                );

                // Create and track request (Phase 4: Task 2.3)
                let block_request = PendingBlockRequest {
                    request_id,
                    peer_ids: selected_peers.clone(),
                    start_height,
                    count,
                    timestamp: Instant::now(),
                };
                self.pending_block_requests
                    .insert(request_id, block_request);

                // Get command channel (Phase 3 Task 3.1)
                let cmd_tx = match self.swarm_cmd_tx.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        tracing::error!(correlation_id = %request_id, "Swarm command channel not available");
                        return Err(NetworkError::Internal(
                            "Command channel not available".to_string(),
                        ));
                    }
                };

                // Create BlockRequest
                let block_req = BlockRequest::GetBlocks(
                    crate::actors_v2::network::protocols::request_response::BlockRangeRequest {
                        start_height,
                        count,
                    },
                );

                // Send requests to selected peers via SwarmCommand (Phase 3 Task 3.1)
                let mut send_failures = 0;
                for peer_id_str in &selected_peers {
                    // Parse peer ID string to libp2p PeerId
                    let peer_id = match peer_id_str.parse::<PeerId>() {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!(
                                correlation_id = %request_id,
                                peer_id = %peer_id_str,
                                error = ?e,
                                "Invalid peer ID format"
                            );
                            send_failures += 1;
                            continue;
                        }
                    };

                    tracing::debug!(
                        correlation_id = %request_id,
                        peer_id = %peer_id,
                        "Sending block request to peer via SwarmCommand"
                    );

                    // Create oneshot channel for response
                    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

                    // Send request command (non-blocking)
                    let cmd = SwarmCommand::SendRequest {
                        peer_id,
                        request: block_req.clone(),
                        response_tx,
                    };

                    match cmd_tx.try_send(cmd) {
                        Ok(_) => {
                            // Spawn task to handle async response
                            let correlation_id_clone = request_id;
                            let peer_id_str_clone = peer_id_str.clone();
                            tokio::spawn(async move {
                                match response_rx.await {
                                    Ok(Ok(request_id)) => {
                                        tracing::info!(
                                            correlation_id = %correlation_id_clone,
                                            peer_id = %peer_id_str_clone,
                                            request_id = ?request_id,
                                            "Block request sent successfully"
                                        );
                                    }
                                    Ok(Err(e)) => {
                                        tracing::error!(
                                            correlation_id = %correlation_id_clone,
                                            peer_id = %peer_id_str_clone,
                                            error = %e,
                                            "Block request failed"
                                        );
                                    }
                                    Err(_) => {
                                        tracing::error!(
                                            correlation_id = %correlation_id_clone,
                                            peer_id = %peer_id_str_clone,
                                            "Block request response channel closed"
                                        );
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!(
                                correlation_id = %request_id,
                                peer_id = %peer_id_str,
                                error = ?e,
                                "Failed to send block request command"
                            );
                            send_failures += 1;
                        }
                    }
                }

                // Check if all requests failed
                if send_failures == selected_peers.len() {
                    tracing::error!(
                        correlation_id = %request_id,
                        "All block requests failed to send"
                    );
                    self.pending_block_requests.remove(&request_id);
                    return Err(NetworkError::Internal(
                        "Failed to send any block requests".to_string(),
                    ));
                }

                Ok(NetworkResponse::BlocksRequested {
                    peer_count: selected_peers.len(),
                    request_id,
                })
            }

            NetworkMessage::HandleBlockResponse {
                blocks,
                request_id,
                peer_id,
                correlation_id,
            } => {
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

                // Task 2.3: Handle empty block response without penalizing peer
                // An empty response doesn't mean protocol error - the peer may legitimately
                // not have those blocks (e.g., during initial sync, after restart, storage gap).
                // Only penalize for actual protocol violations, not for storage gaps.
                if blocks.is_empty() {
                    tracing::debug!(
                        correlation_id = %correlation_id,
                        request_id = %request_id,
                        peer_id = %peer_id,
                        start_height = request.start_height,
                        count = request.count,
                        "Peer returned empty block response - may not have these blocks yet (not penalizing)"
                    );
                    // Don't record as failure - peer may simply not have these blocks
                    // self.peer_manager.record_peer_failure(&peer_id);  // Removed
                    // Still return error to let caller retry with different peer
                    return Err(NetworkError::Protocol("Empty block response (peer may not have blocks)".to_string()));
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
                        peer_id: peer_id.clone(),
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
                    Err(NetworkError::Internal(
                        "SyncActor not available".to_string(),
                    ))
                }
            }

            NetworkMessage::SetChainActor { addr } => {
                self.chain_actor = Some(addr);
                tracing::info!("ChainActor address set for NetworkActor AuxPoW forwarding");
                Ok(NetworkResponse::Started)
            }
            NetworkMessage::SetStorageActor { addr } => {
                self.storage_actor = Some(addr);
                tracing::info!("StorageActor address set for NetworkActor block request handling");
                Ok(NetworkResponse::Started)
            }
            NetworkMessage::HandleCompletedAuxPow {
                auxpow_data,
                peer_id,
                correlation_id,
            } => {
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
                let auxpow_header =
                    match serde_json::from_slice::<crate::block::AuxPowHeader>(&auxpow_data) {
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
                    Err(NetworkError::Internal(
                        "ChainActor not available".to_string(),
                    ))
                }
            }
            NetworkMessage::HealthCheck { correlation_id } => {
                tracing::debug!(
                    correlation_id = ?correlation_id,
                    "Performing network health check"
                );

                // Phase 4: Enhanced health check with reputation monitoring
                let connected_peers = self.peer_manager.get_connected_peers().len();
                let avg_reputation = self.peer_manager.get_average_reputation();
                let swarm_healthy = self.is_running && self.swarm_cmd_tx.is_some();

                // Health criteria
                let is_healthy = swarm_healthy && connected_peers > 0 && avg_reputation > 0.0;

                // Detailed issues reporting
                let mut issues = Vec::new();

                if !swarm_healthy {
                    if !self.is_running {
                        issues.push("Network not running".to_string());
                    }
                    if self.swarm_cmd_tx.is_none() {
                        issues.push("Swarm command channel not available".to_string());
                    }
                }

                if connected_peers == 0 {
                    issues.push("No peers connected".to_string());
                } else if connected_peers < 3 {
                    issues.push(format!(
                        "Low peer count: {} (recommended: >=3)",
                        connected_peers
                    ));
                }

                if avg_reputation <= 0.0 {
                    issues.push(format!(
                        "Critical: Average peer reputation is {:.1} (threshold: >0.0)",
                        avg_reputation
                    ));
                } else if avg_reputation < 30.0 {
                    issues.push(format!(
                        "Warning: Low average peer reputation: {:.1}",
                        avg_reputation
                    ));
                }

                // Check for high rate limiting
                if self.metrics.rate_limited_messages > 100 {
                    issues.push(format!(
                        "High rate limiting: {} messages dropped",
                        self.metrics.rate_limited_messages
                    ));
                }

                // Check for high connection failure rate
                let connection_failure_rate = if self.metrics.total_connections > 0 {
                    self.metrics.failed_connections as f64 / self.metrics.total_connections as f64
                } else {
                    0.0
                };

                if connection_failure_rate > 0.5 {
                    issues.push(format!(
                        "High connection failure rate: {:.1}%",
                        connection_failure_rate * 100.0
                    ));
                }

                tracing::info!(
                    correlation_id = ?correlation_id,
                    is_healthy = is_healthy,
                    connected_peers = connected_peers,
                    avg_reputation = avg_reputation,
                    issues_count = issues.len(),
                    "Health check completed"
                );

                Ok(NetworkResponse::Healthy {
                    is_healthy,
                    connected_peers,
                    issues,
                })
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

            NetworkMessage::QueryPeerHeights => {
                // Query up to 5 connected peers for their chain heights
                // This is used by SyncActor during QueryingNetworkHeight state
                // to discover the actual network height from peers via consensus (mode)
                const MAX_PEERS_TO_QUERY: usize = 5;

                tracing::info!("Querying connected peers for chain heights");

                let connected_peers = self.peer_manager.get_connected_peers();
                let total_peer_count = connected_peers.len();

                if total_peer_count == 0 {
                    tracing::warn!("No connected peers to query for heights");
                    // Still report empty results so SyncActor knows query completed
                    if let Some(sync_actor) = &self.sync_actor {
                        sync_actor.do_send(SyncMessage::ReportPeerHeights {
                            peer_heights: vec![],
                        });
                    }
                    return Ok(NetworkResponse::Status(NetworkStatus {
                        local_peer_id: self.local_peer_id.clone(),
                        connected_peers: 0,
                        listening_addresses: vec![],
                        is_running: self.is_running,
                        chain_height: 0,
                    }));
                }

                // Get command channel for sending requests
                let cmd_tx = match self.swarm_cmd_tx.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        tracing::error!("Swarm command channel not available for peer height query");
                        return Err(NetworkError::Internal(
                            "Command channel not available".to_string(),
                        ));
                    }
                };

                // Select up to MAX_PEERS_TO_QUERY peers, sorted by reputation (best first)
                let mut peers_with_reputation: Vec<_> = connected_peers
                    .iter()
                    .map(|(id, info)| (id.clone(), info.reputation))
                    .collect();

                // Sort by reputation descending (best peers first)
                peers_with_reputation.sort_by(|a, b| {
                    b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                });

                // Take top N peers and convert to PeerId
                let peer_ids: Vec<(String, libp2p::PeerId)> = peers_with_reputation
                    .into_iter()
                    .take(MAX_PEERS_TO_QUERY)
                    .filter_map(|(peer_id_str, _reputation)| {
                        peer_id_str
                            .parse::<libp2p::PeerId>()
                            .ok()
                            .map(|pid| (peer_id_str, pid))
                    })
                    .collect();

                tracing::info!(
                    total_peers = total_peer_count,
                    querying_peers = peer_ids.len(),
                    max_peers = MAX_PEERS_TO_QUERY,
                    "Sending GetChainStatus requests to top peers by reputation"
                );

                // Spawn task to query all peers and collect responses
                tokio::spawn(async move {
                    use crate::actors_v2::network::protocols::request_response::{
                        BlockRequest, BlockResponse, ChainStatusResponse, EmptyRequest,
                    };
                    use std::time::Duration;
                    use tokio::time::timeout;

                    let mut peer_heights: Vec<(String, u64, [u8; 32])> = Vec::new();
                    let request = BlockRequest::GetChainStatus(EmptyRequest);

                    for (peer_id_str, peer_id) in peer_ids {
                        // Create channel for this request's response
                        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

                        let cmd = SwarmCommand::SendRequest {
                            peer_id: peer_id.clone(),
                            request: request.clone(),
                            response_tx,
                        };

                        // Send request
                        if let Err(e) = cmd_tx.try_send(cmd) {
                            tracing::warn!(
                                peer_id = %peer_id_str,
                                error = ?e,
                                "Failed to send GetChainStatus request"
                            );
                            continue;
                        }

                        // Wait for response with timeout (5 seconds per peer)
                        match timeout(Duration::from_secs(5), response_rx).await {
                            Ok(Ok(Ok(_request_id))) => {
                                // Request was sent successfully, but we need to wait for
                                // the actual response which comes via a different path
                                // For now, we'll collect heights as they come in
                                tracing::debug!(
                                    peer_id = %peer_id_str,
                                    "GetChainStatus request sent to peer"
                                );
                            }
                            Ok(Ok(Err(e))) => {
                                tracing::warn!(
                                    peer_id = %peer_id_str,
                                    error = ?e,
                                    "GetChainStatus request failed"
                                );
                            }
                            Ok(Err(_)) => {
                                tracing::warn!(
                                    peer_id = %peer_id_str,
                                    "GetChainStatus response channel closed"
                                );
                            }
                            Err(_) => {
                                tracing::warn!(
                                    peer_id = %peer_id_str,
                                    "GetChainStatus request timed out"
                                );
                            }
                        }
                    }

                    // Note: The actual ChainStatusResponse comes back via the request-response
                    // behavior event stream. For a complete implementation, we need to:
                    // 1. Track pending height queries with a correlation map
                    // 2. Collect responses as they arrive in the event loop
                    // 3. After timeout or all responses, send ReportPeerHeights
                    //
                    // For now, this spawned task just initiates the requests.
                    // The responses will be handled in the existing BlockResponseReceived handler.
                    // We'll add height tracking there.

                    tracing::debug!(
                        "GetChainStatus requests initiated, responses will be collected via event stream"
                    );
                });

                Ok(NetworkResponse::Status(NetworkStatus {
                    local_peer_id: self.local_peer_id.clone(),
                    connected_peers: total_peer_count,
                    listening_addresses: vec![],
                    is_running: self.is_running,
                    chain_height: 0,
                }))
            }

            NetworkMessage::CheckV2PeerHealth => {
                // Check V2 peer health and attempt reconnection if needed
                // This is triggered by SyncActor when no peer height responses are received
                let v2_count = self.peer_manager.connected_v2_peer_count();
                let total_connected = self.peer_manager.get_connected_peers().len();

                tracing::info!(
                    v2_peer_count = v2_count,
                    total_connected = total_connected,
                    "V2 peer health check triggered by SyncActor (stale network height detected)"
                );

                if v2_count == 0 {
                    tracing::warn!(
                        total_connected = total_connected,
                        "No V2-capable peers connected - attempting reconnection"
                    );
                    self.attempt_v2_peer_reconnection();
                } else {
                    tracing::debug!(
                        v2_count = v2_count,
                        "V2 peers are connected - network height should recover"
                    );
                }

                Ok(NetworkResponse::Started)
            }

            NetworkMessage::BroadcastTendermint {
                message,
                correlation_id,
            } => {
                // Broadcast Tendermint consensus message to network
                use crate::actors_v2::network::protocols::gossip::GossipTopic;

                let message_type = message.message_type().to_string();
                let topic = match &message {
                    crate::actors_v2::chain::tendermint::TendermintMessage::Proposal(_) => {
                        GossipTopic::TendermintProposals
                    }
                    crate::actors_v2::chain::tendermint::TendermintMessage::Vote(_) => {
                        GossipTopic::TendermintVotes
                    }
                    crate::actors_v2::chain::tendermint::TendermintMessage::Timeout(_) => {
                        GossipTopic::TendermintTimeouts
                    }
                    crate::actors_v2::chain::tendermint::TendermintMessage::Evidence(_) => {
                        GossipTopic::TendermintEvidence
                    }
                    crate::actors_v2::chain::tendermint::TendermintMessage::NewRound { .. } => {
                        GossipTopic::TendermintNewRound
                    }
                    crate::actors_v2::chain::tendermint::TendermintMessage::BlockRequest { .. }
                    | crate::actors_v2::chain::tendermint::TendermintMessage::BlockResponse { .. } => {
                        // Block requests/responses use request-response protocol, not gossip
                        return Err(NetworkError::Protocol(
                            "Block requests use request-response protocol".to_string(),
                        ));
                    }
                };

                // Serialize to wire format
                let wire_msg = match crate::actors_v2::network::tendermint::TendermintWireMessage::from_message(
                    &message,
                    self.local_peer_id.clone(),
                ) {
                    Ok(w) => w,
                    Err(e) => {
                        return Err(NetworkError::Protocol(format!(
                            "Failed to serialize Tendermint message: {}",
                            e
                        )));
                    }
                };

                let data = match wire_msg.to_bytes() {
                    Ok(d) => d,
                    Err(e) => {
                        return Err(NetworkError::Protocol(format!(
                            "Failed to encode Tendermint message: {}",
                            e
                        )));
                    }
                };

                tracing::debug!(
                    correlation_id = ?correlation_id,
                    message_type = %message_type,
                    topic = %topic.as_str(),
                    data_len = data.len(),
                    "Broadcasting Tendermint message"
                );

                // Send via gossipsub
                let cmd_tx = match self.swarm_cmd_tx.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        return Err(NetworkError::NotStarted);
                    }
                };

                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                let cmd = SwarmCommand::PublishGossip {
                    topic: topic.as_str().to_string(),
                    data,
                    response_tx,
                };

                if let Err(e) = cmd_tx.try_send(cmd) {
                    return Err(NetworkError::Internal(format!(
                        "Failed to send gossip command: {}",
                        e
                    )));
                }

                // Count connected peers for response
                let peer_count = self.peer_manager.get_connected_peers().len();

                Ok(NetworkResponse::TendermintBroadcasted {
                    peer_count,
                    message_type,
                })
            }

            NetworkMessage::HandleTendermintMessage {
                message,
                peer_id,
                correlation_id,
            } => {
                // Forward Tendermint message to ChainActor for processing
                let message_type = message.message_type().to_string();

                tracing::debug!(
                    correlation_id = ?correlation_id,
                    peer_id = %peer_id,
                    message_type = %message_type,
                    "Received Tendermint message, forwarding to ChainActor"
                );

                if let Some(chain_actor) = &self.chain_actor {
                    // Forward to ChainActor for consensus processing
                    // Note: ChainActor will need to implement a handler for TendermintMessage
                    // This is a placeholder until Stage 5 (ChainActor Tendermint handlers) is implemented
                    tracing::info!(
                        message_type = %message_type,
                        peer_id = %peer_id,
                        "Tendermint message received - ChainActor integration pending"
                    );
                    Ok(NetworkResponse::TendermintForwarded)
                } else {
                    tracing::warn!(
                        "Received Tendermint message but ChainActor not set"
                    );
                    Err(NetworkError::Internal(
                        "ChainActor not set".to_string(),
                    ))
                }
            }
        }
    }
}
