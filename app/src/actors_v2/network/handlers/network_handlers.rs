//! NetworkActor V2 Message Handlers
//!
//! Handles P2P protocol operations for NetworkActor.
//! Removed complex supervision and actor_system patterns.

use anyhow::{Result, anyhow};

use crate::actors_v2::network::{
    NetworkMessage, NetworkResponse, NetworkError,
    messages::{PeerInfo, NetworkStatus},
};

/// NetworkActor message handling utilities
pub struct NetworkMessageHandlers;

impl NetworkMessageHandlers {
    /// Handle network startup
    pub fn handle_start_network(
        listen_addrs: Vec<String>,
        bootstrap_peers: Vec<String>,
    ) -> Result<NetworkResponse> {
        // Validation
        if listen_addrs.is_empty() {
            return Err(anyhow!("At least one listen address required"));
        }

        // In real implementation, this would initialize libp2p swarm
        tracing::info!("Starting network with {} listen addresses and {} bootstrap peers",
            listen_addrs.len(), bootstrap_peers.len());

        Ok(NetworkResponse::Started)
    }

    /// Handle graceful network shutdown
    pub fn handle_stop_network(graceful: bool) -> Result<NetworkResponse> {
        if graceful {
            tracing::info!("Gracefully stopping network");
            // In real implementation, this would drain connections
        } else {
            tracing::info!("Forcefully stopping network");
        }

        Ok(NetworkResponse::Stopped)
    }

    /// Validate peer address format
    pub fn validate_peer_address(addr: &str) -> Result<()> {
        if addr.is_empty() {
            return Err(anyhow!("Peer address cannot be empty"));
        }

        // Basic multiaddr validation
        if !addr.starts_with('/') {
            return Err(anyhow!("Invalid multiaddr format: {}", addr));
        }

        Ok(())
    }

    /// Validate gossip message
    pub fn validate_gossip_message(topic: &str, data: &[u8]) -> Result<()> {
        if topic.is_empty() {
            return Err(anyhow!("Topic cannot be empty"));
        }

        if data.is_empty() {
            return Err(anyhow!("Message data cannot be empty"));
        }

        if data.len() > 10 * 1024 * 1024 { // 10MB limit
            return Err(anyhow!("Message too large: {} bytes", data.len()));
        }

        Ok(())
    }

    /// Create network status response
    pub fn create_status_response(
        local_peer_id: String,
        connected_peers: usize,
        listening_addresses: Vec<String>,
        is_running: bool,
    ) -> NetworkStatus {
        NetworkStatus {
            local_peer_id,
            connected_peers,
            listening_addresses,
            is_running,
        }
    }

    /// Handle connection attempt result
    pub fn handle_connection_result(peer_id: String, success: bool) -> Result<NetworkResponse> {
        if success {
            tracing::info!("Successfully connected to peer: {}", peer_id);
            Ok(NetworkResponse::Connected { peer_id })
        } else {
            tracing::warn!("Failed to connect to peer: {}", peer_id);
            Err(anyhow!("Connection failed"))
        }
    }

    /// Handle disconnection
    pub fn handle_disconnection(peer_id: String) -> Result<NetworkResponse> {
        tracing::info!("Disconnected from peer: {}", peer_id);
        Ok(NetworkResponse::Disconnected { peer_id })
    }

    /// Convert internal peer info to response format
    pub fn convert_peer_info(
        internal_peers: Vec<(String, crate::actors_v2::network::managers::peer_manager::PeerInfo)>
    ) -> Vec<PeerInfo> {
        internal_peers.into_iter()
            .map(|(peer_id, info)| PeerInfo {
                peer_id,
                address: info.address,
                connection_time: info.connected_since,
                reputation: info.reputation,
            })
            .collect()
    }

    /// Validate broadcast request
    pub fn validate_broadcast_request(data: &[u8], priority: bool) -> Result<()> {
        if data.is_empty() {
            return Err(anyhow!("Broadcast data cannot be empty"));
        }

        let max_size = if priority {
            50 * 1024 * 1024 // 50MB for priority messages (blocks)
        } else {
            10 * 1024 * 1024 // 10MB for regular messages
        };

        if data.len() > max_size {
            return Err(anyhow!("Broadcast message too large: {} bytes", data.len()));
        }

        Ok(())
    }

    /// Generate message ID for broadcast
    pub fn generate_message_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}