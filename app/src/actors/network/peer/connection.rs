//! Peer Connection Manager
//! 
//! Manages peer connections, connection pools, and connection lifecycle
//! with federation prioritization and load balancing.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use libp2p::{PeerId, Multiaddr};
use libp2p::swarm::{ConnectionId, ConnectedPoint};
use tokio::sync::mpsc;

use actor_system::error::ActorResult;
use crate::actors::network::peer::config::PeerConfig;

/// Connection manager for peer connections
#[derive(Debug)]
pub struct ConnectionManager {
    /// Configuration
    config: PeerConfig,
    /// Active connections
    connections: HashMap<PeerId, ConnectionInfo>,
    /// Pending outbound connections
    pending_outbound: HashMap<PeerId, PendingConnection>,
    /// Connection pools by priority
    connection_pools: ConnectionPools,
    /// Connection statistics
    stats: ConnectionStats,
    /// Connection events channel
    event_sender: Option<mpsc::UnboundedSender<ConnectionEvent>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(config: &PeerConfig) -> ActorResult<Self> {
        Ok(Self {
            config: config.clone(),
            connections: HashMap::new(),
            pending_outbound: HashMap::new(),
            connection_pools: ConnectionPools::new(config),
            stats: ConnectionStats::default(),
            event_sender: None,
        })
    }

    /// Set event channel for connection notifications
    pub fn set_event_channel(&mut self, sender: mpsc::UnboundedSender<ConnectionEvent>) {
        self.event_sender = Some(sender);
    }

    /// Add a new connection
    pub fn add_connection(
        &mut self, 
        peer_id: PeerId, 
        endpoint: ConnectedPoint,
        is_federation: bool,
        protocols: Vec<String>
    ) -> ActorResult<()> {
        // Check connection limits
        if self.connections.len() >= self.config.max_peers as usize {
            return Err(actor_system::ActorError::ConfigurationError {
                reason: "Maximum peer connections reached".to_string(),
            });
        }

        let connection_type = match endpoint {
            ConnectedPoint::Dialer { .. } => ConnectionType::Outbound,
            ConnectedPoint::Listener { .. } => ConnectionType::Inbound,
        };

        let connection_info = ConnectionInfo {
            peer_id,
            endpoint,
            connection_type: connection_type.clone(),
            established_at: Instant::now(),
            last_activity: Instant::now(),
            is_federation,
            supported_protocols: protocols,
            bytes_sent: 0,
            bytes_received: 0,
            messages_sent: 0,
            messages_received: 0,
            status: ConnectionStatus::Active,
        };

        // Check specific connection type limits
        match connection_type {
            ConnectionType::Inbound => {
                if self.count_inbound_connections() >= self.config.max_inbound_peers as usize {
                    return Err(actor_system::ActorError::ConfigurationError {
                        reason: "Maximum inbound connections reached".to_string(),
                    });
                }
            },
            ConnectionType::Outbound => {
                if self.count_outbound_connections() >= self.config.max_outbound_peers as usize {
                    return Err(actor_system::ActorError::ConfigurationError {
                        reason: "Maximum outbound connections reached".to_string(),
                    });
                }
            },
        }

        // Add to appropriate pool
        self.connection_pools.add_connection(&connection_info)?;
        
        // Remove from pending if it was a pending outbound connection
        self.pending_outbound.remove(&peer_id);

        // Update statistics
        self.stats.total_connections += 1;
        match connection_type {
            ConnectionType::Inbound => self.stats.inbound_connections += 1,
            ConnectionType::Outbound => self.stats.outbound_connections += 1,
        }

        if is_federation {
            self.stats.federation_connections += 1;
        }

        // Store connection
        self.connections.insert(peer_id, connection_info);

        // Send event
        self.send_event(ConnectionEvent::Connected {
            peer_id,
            connection_type,
            is_federation,
        });

        tracing::info!(
            "Connection established with peer {} ({:?}, federation: {})",
            peer_id, connection_type, is_federation
        );

        Ok(())
    }

    /// Remove a connection
    pub fn remove_connection(&mut self, peer_id: &PeerId, reason: DisconnectionReason) -> Option<ConnectionInfo> {
        if let Some(connection_info) = self.connections.remove(peer_id) {
            // Remove from pools
            self.connection_pools.remove_connection(&connection_info);

            // Update statistics
            self.stats.total_connections = self.stats.total_connections.saturating_sub(1);
            match connection_info.connection_type {
                ConnectionType::Inbound => {
                    self.stats.inbound_connections = self.stats.inbound_connections.saturating_sub(1);
                },
                ConnectionType::Outbound => {
                    self.stats.outbound_connections = self.stats.outbound_connections.saturating_sub(1);
                },
            }

            if connection_info.is_federation {
                self.stats.federation_connections = self.stats.federation_connections.saturating_sub(1);
            }

            // Send event
            self.send_event(ConnectionEvent::Disconnected {
                peer_id: *peer_id,
                reason: reason.clone(),
                duration: connection_info.established_at.elapsed(),
            });

            tracing::info!(
                "Connection removed for peer {} (reason: {:?}, duration: {:?})",
                peer_id, reason, connection_info.established_at.elapsed()
            );

            Some(connection_info)
        } else {
            None
        }
    }

    /// Get connection info
    pub fn get_connection(&self, peer_id: &PeerId) -> Option<&ConnectionInfo> {
        self.connections.get(peer_id)
    }

    /// Get mutable connection info
    pub fn get_connection_mut(&mut self, peer_id: &PeerId) -> Option<&mut ConnectionInfo> {
        self.connections.get_mut(peer_id)
    }

    /// Check if peer is connected
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        self.connections.contains_key(peer_id)
    }

    /// Get all connected peers
    pub fn get_connected_peers(&self) -> Vec<PeerId> {
        self.connections.keys().copied().collect()
    }

    /// Get federation peers
    pub fn get_federation_peers(&self) -> Vec<PeerId> {
        self.connections.iter()
            .filter(|(_, info)| info.is_federation)
            .map(|(&peer_id, _)| peer_id)
            .collect()
    }

    /// Get best peers for communication (highest priority)
    pub fn get_best_peers(&self, limit: usize) -> Vec<PeerId> {
        self.connection_pools.get_best_peers(limit)
    }

    /// Initiate outbound connection
    pub fn initiate_connection(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) -> ActorResult<()> {
        // Check if already connected or pending
        if self.connections.contains_key(&peer_id) {
            return Err(actor_system::ActorError::ConfigurationError {
                reason: "Peer already connected".to_string(),
            });
        }

        if self.pending_outbound.contains_key(&peer_id) {
            return Err(actor_system::ActorError::ConfigurationError {
                reason: "Connection already pending".to_string(),
            });
        }

        // Check outbound connection limit
        if self.count_outbound_connections() + self.pending_outbound.len() >= self.config.max_outbound_peers as usize {
            return Err(actor_system::ActorError::ConfigurationError {
                reason: "Maximum outbound connections reached".to_string(),
            });
        }

        let pending_connection = PendingConnection {
            peer_id,
            addresses,
            initiated_at: Instant::now(),
            timeout: self.config.connection_timeout,
            retry_count: 0,
        };

        self.pending_outbound.insert(peer_id, pending_connection);

        tracing::debug!("Initiating connection to peer {}", peer_id);
        Ok(())
    }

    /// Cancel pending connection
    pub fn cancel_pending_connection(&mut self, peer_id: &PeerId) -> bool {
        if let Some(_) = self.pending_outbound.remove(peer_id) {
            tracing::debug!("Cancelled pending connection to peer {}", peer_id);
            true
        } else {
            false
        }
    }

    /// Get pending connections
    pub fn get_pending_connections(&self) -> Vec<&PendingConnection> {
        self.pending_outbound.values().collect()
    }

    /// Update connection activity
    pub fn update_activity(&mut self, peer_id: &PeerId, bytes_sent: u64, bytes_received: u64) {
        if let Some(connection) = self.connections.get_mut(peer_id) {
            connection.last_activity = Instant::now();
            connection.bytes_sent += bytes_sent;
            connection.bytes_received += bytes_received;
        }
    }

    /// Update message counts
    pub fn update_message_counts(&mut self, peer_id: &PeerId, messages_sent: u32, messages_received: u32) {
        if let Some(connection) = self.connections.get_mut(peer_id) {
            connection.messages_sent += messages_sent;
            connection.messages_received += messages_received;
        }
    }

    /// Check for idle connections that should be closed
    pub fn check_idle_connections(&mut self, max_idle_time: Duration) -> Vec<PeerId> {
        let now = Instant::now();
        let mut idle_peers = Vec::new();

        for (&peer_id, connection) in &self.connections {
            if now.duration_since(connection.last_activity) > max_idle_time {
                // Don't close federation or reserved connections due to idle timeout
                if !connection.is_federation {
                    idle_peers.push(peer_id);
                }
            }
        }

        idle_peers
    }

    /// Check for timed-out pending connections
    pub fn check_pending_timeouts(&mut self) -> Vec<PeerId> {
        let now = Instant::now();
        let mut timed_out = Vec::new();

        for (&peer_id, pending) in &self.pending_outbound {
            if now.duration_since(pending.initiated_at) > pending.timeout {
                timed_out.push(peer_id);
            }
        }

        // Remove timed out connections
        for peer_id in &timed_out {
            self.pending_outbound.remove(peer_id);
        }

        timed_out
    }

    /// Get connection statistics
    pub fn get_stats(&self) -> &ConnectionStats {
        &self.stats
    }

    /// Count inbound connections
    fn count_inbound_connections(&self) -> usize {
        self.connections.values()
            .filter(|conn| matches!(conn.connection_type, ConnectionType::Inbound))
            .count()
    }

    /// Count outbound connections
    fn count_outbound_connections(&self) -> usize {
        self.connections.values()
            .filter(|conn| matches!(conn.connection_type, ConnectionType::Outbound))
            .count()
    }

    /// Send connection event
    fn send_event(&self, event: ConnectionEvent) {
        if let Some(sender) = &self.event_sender {
            if let Err(_) = sender.send(event) {
                tracing::warn!("Failed to send connection event");
            }
        }
    }
}

/// Information about an active connection
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Peer ID
    pub peer_id: PeerId,
    /// Connection endpoint information
    pub endpoint: ConnectedPoint,
    /// Connection type (inbound/outbound)
    pub connection_type: ConnectionType,
    /// When connection was established
    pub established_at: Instant,
    /// Last activity timestamp
    pub last_activity: Instant,
    /// Whether this is a federation peer
    pub is_federation: bool,
    /// Supported protocols
    pub supported_protocols: Vec<String>,
    /// Bytes sent to this peer
    pub bytes_sent: u64,
    /// Bytes received from this peer
    pub bytes_received: u64,
    /// Messages sent to this peer
    pub messages_sent: u32,
    /// Messages received from this peer
    pub messages_received: u32,
    /// Connection status
    pub status: ConnectionStatus,
}

/// Pending outbound connection
#[derive(Debug, Clone)]
pub struct PendingConnection {
    /// Target peer ID
    pub peer_id: PeerId,
    /// Addresses to try
    pub addresses: Vec<Multiaddr>,
    /// When connection attempt was initiated
    pub initiated_at: Instant,
    /// Connection timeout
    pub timeout: Duration,
    /// Number of retry attempts
    pub retry_count: u32,
}

/// Connection type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    Inbound,
    Outbound,
}

/// Connection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Active,
    Idle,
    Closing,
}

/// Connection pools organized by priority
#[derive(Debug)]
struct ConnectionPools {
    /// High priority connections (federation members)
    high_priority: VecDeque<PeerId>,
    /// Normal priority connections
    normal_priority: VecDeque<PeerId>,
    /// Low priority connections
    low_priority: VecDeque<PeerId>,
}

impl ConnectionPools {
    fn new(_config: &PeerConfig) -> Self {
        Self {
            high_priority: VecDeque::new(),
            normal_priority: VecDeque::new(),
            low_priority: VecDeque::new(),
        }
    }

    fn add_connection(&mut self, connection: &ConnectionInfo) -> ActorResult<()> {
        let pool = if connection.is_federation {
            &mut self.high_priority
        } else {
            &mut self.normal_priority
        };

        pool.push_back(connection.peer_id);
        Ok(())
    }

    fn remove_connection(&mut self, connection: &ConnectionInfo) {
        let pool = if connection.is_federation {
            &mut self.high_priority
        } else {
            &mut self.normal_priority
        };

        if let Some(pos) = pool.iter().position(|&x| x == connection.peer_id) {
            pool.remove(pos);
        }
    }

    fn get_best_peers(&self, limit: usize) -> Vec<PeerId> {
        let mut result = Vec::new();
        
        // First, take from high priority
        let high_count = limit.min(self.high_priority.len());
        result.extend(self.high_priority.iter().take(high_count).copied());
        
        // Then from normal priority
        if result.len() < limit {
            let remaining = limit - result.len();
            let normal_count = remaining.min(self.normal_priority.len());
            result.extend(self.normal_priority.iter().take(normal_count).copied());
        }
        
        // Finally from low priority if needed
        if result.len() < limit {
            let remaining = limit - result.len();
            let low_count = remaining.min(self.low_priority.len());
            result.extend(self.low_priority.iter().take(low_count).copied());
        }
        
        result
    }
}

/// Connection statistics
#[derive(Debug, Default)]
pub struct ConnectionStats {
    /// Total active connections
    pub total_connections: u32,
    /// Number of inbound connections
    pub inbound_connections: u32,
    /// Number of outbound connections
    pub outbound_connections: u32,
    /// Number of federation connections
    pub federation_connections: u32,
    /// Total bytes sent across all connections
    pub total_bytes_sent: u64,
    /// Total bytes received across all connections
    pub total_bytes_received: u64,
}

/// Disconnection reasons
#[derive(Debug, Clone)]
pub enum DisconnectionReason {
    /// Graceful close by peer
    PeerClosed,
    /// Connection error
    ConnectionError(String),
    /// Timeout
    Timeout,
    /// Banned peer
    Banned,
    /// Local shutdown
    LocalShutdown,
    /// Connection limit reached
    LimitReached,
    /// Protocol error
    ProtocolError,
}

/// Connection events
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// New connection established
    Connected {
        peer_id: PeerId,
        connection_type: ConnectionType,
        is_federation: bool,
    },
    /// Connection lost
    Disconnected {
        peer_id: PeerId,
        reason: DisconnectionReason,
        duration: Duration,
    },
    /// Connection attempt failed
    ConnectionFailed {
        peer_id: PeerId,
        error: String,
    },
}