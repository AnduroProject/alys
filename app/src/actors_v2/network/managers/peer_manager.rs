//! Peer Manager V2
//!
//! Simplified peer management replacing V1 PeerActor (2,655 lines -> ~500-800 lines).
//! Removed: Kademlia DHT, complex supervision, actor_system dependencies
//! Added: Bootstrap-based discovery, basic reputation system

use std::collections::HashMap;
use std::time::SystemTime;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};

use crate::actors_v2::network::messages::PeerId;

/// Simplified peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub address: String,
    pub connected_since: SystemTime,
    pub reputation: f64,
    pub connection_attempts: u32,
    pub successful_requests: u32,
    pub failed_requests: u32,
    pub last_seen: SystemTime,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId, address: String) -> Self {
        let now = SystemTime::now();
        Self {
            peer_id,
            address,
            connected_since: now,
            reputation: 50.0, // Start with neutral reputation
            connection_attempts: 0,
            successful_requests: 0,
            failed_requests: 0,
            last_seen: now,
        }
    }

    /// Update reputation based on interaction
    pub fn update_reputation(&mut self, delta: f64) {
        self.reputation = (self.reputation + delta).max(0.0).min(100.0);
        self.last_seen = SystemTime::now();
    }

    /// Record successful interaction
    pub fn record_success(&mut self) {
        self.successful_requests += 1;
        self.update_reputation(1.0);
    }

    /// Record failed interaction
    pub fn record_failure(&mut self) {
        self.failed_requests += 1;
        self.update_reputation(-2.0);
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_requests + self.failed_requests;
        if total == 0 {
            return 0.5; // Neutral for new peers
        }
        self.successful_requests as f64 / total as f64
    }

    /// Check if peer should be disconnected based on reputation
    pub fn should_disconnect(&self) -> bool {
        self.reputation < 10.0 || self.success_rate() < 0.3
    }
}

/// Simplified peer manager (replacing V1 PeerActor complexity)
#[derive(Debug)]
pub struct PeerManager {
    /// Currently connected peers
    connected_peers: HashMap<PeerId, PeerInfo>,
    /// Known peers (not necessarily connected)
    known_peers: HashMap<PeerId, PeerInfo>,
    /// Bootstrap peer addresses
    bootstrap_peers: Vec<String>,
    /// Maximum number of peers to maintain
    max_peers: usize,
    /// Peer discovery state
    discovery_active: bool,
}

impl PeerManager {
    /// Create new peer manager with simplified configuration
    pub fn new() -> Self {
        Self {
            connected_peers: HashMap::new(),
            known_peers: HashMap::new(),
            bootstrap_peers: Vec::new(),
            max_peers: 50, // Reasonable default
            discovery_active: false,
        }
    }

    /// Add a new peer connection
    pub fn add_peer(&mut self, peer_id: PeerId, address: String) {
        let peer_info = PeerInfo::new(peer_id.clone(), address);

        tracing::info!("Added peer connection: {}", peer_id);

        self.connected_peers.insert(peer_id.clone(), peer_info.clone());
        self.known_peers.insert(peer_id, peer_info);
    }

    /// Remove peer connection
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.connected_peers.remove(peer_id) {
            tracing::info!("Removed peer connection: {}", peer_id);

            // Keep in known_peers for potential reconnection
            self.known_peers.insert(peer_id.clone(), peer_info);
        }
    }

    /// Get connected peers
    pub fn get_connected_peers(&self) -> HashMap<PeerId, PeerInfo> {
        self.connected_peers.clone()
    }

    /// Get peer by ID
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.connected_peers.get(peer_id)
    }

    /// Update peer reputation
    pub fn update_peer_reputation(&mut self, peer_id: &PeerId, delta: f64) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            peer_info.update_reputation(delta);

            // Also update in known_peers
            if let Some(known_peer) = self.known_peers.get_mut(peer_id) {
                known_peer.update_reputation(delta);
            }
        }
    }

    /// Record successful request to peer
    pub fn record_peer_success(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            peer_info.record_success();
            tracing::debug!("Recorded success for peer {}: reputation = {:.1}",
                peer_id, peer_info.reputation);
        }
    }

    /// Record failed request to peer
    pub fn record_peer_failure(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            peer_info.record_failure();
            tracing::debug!("Recorded failure for peer {}: reputation = {:.1}",
                peer_id, peer_info.reputation);
        }
    }

    /// Get best peers for requests (by reputation)
    pub fn get_best_peers(&self, count: usize) -> Vec<PeerId> {
        let mut peers: Vec<_> = self.connected_peers.values().collect();
        peers.sort_by(|a, b| b.reputation.partial_cmp(&a.reputation).unwrap_or(std::cmp::Ordering::Equal));

        peers.into_iter()
            .take(count)
            .map(|p| p.peer_id.clone())
            .collect()
    }

    /// Get peers that should be disconnected
    pub fn get_peers_to_disconnect(&self) -> Vec<PeerId> {
        self.connected_peers.values()
            .filter(|peer| peer.should_disconnect())
            .map(|peer| peer.peer_id.clone())
            .collect()
    }

    /// Set bootstrap peers for discovery
    pub fn set_bootstrap_peers(&mut self, peers: Vec<String>) {
        self.bootstrap_peers = peers;
        tracing::info!("Set {} bootstrap peers", self.bootstrap_peers.len());
    }

    /// Get bootstrap peers for connection
    pub fn get_bootstrap_peers(&self) -> &[String] {
        &self.bootstrap_peers
    }

    /// Start peer discovery (simplified - no Kademlia)
    pub fn start_discovery(&mut self) {
        self.discovery_active = true;
        tracing::info!("Started bootstrap-based peer discovery");
    }

    /// Stop peer discovery
    pub fn stop_discovery(&mut self) {
        self.discovery_active = false;
        tracing::info!("Stopped peer discovery");
    }

    /// Check if we need more peers
    pub fn needs_more_peers(&self) -> bool {
        self.connected_peers.len() < self.max_peers / 2
    }

    /// Get discovery candidates (from known_peers not connected)
    pub fn get_discovery_candidates(&self) -> Vec<String> {
        self.known_peers.values()
            .filter(|peer| !self.connected_peers.contains_key(&peer.peer_id))
            .filter(|peer| peer.reputation > 20.0) // Only try peers with decent reputation
            .map(|peer| peer.address.clone())
            .collect()
    }

    /// Get connection statistics
    pub fn get_connection_stats(&self) -> PeerConnectionStats {
        let total_connected = self.connected_peers.len();
        let avg_reputation = if total_connected > 0 {
            self.connected_peers.values()
                .map(|p| p.reputation)
                .sum::<f64>() / total_connected as f64
        } else {
            0.0
        };

        let high_reputation_count = self.connected_peers.values()
            .filter(|p| p.reputation > 70.0)
            .count();

        PeerConnectionStats {
            total_connected,
            total_known: self.known_peers.len(),
            average_reputation: avg_reputation,
            high_reputation_peers: high_reputation_count,
            discovery_active: self.discovery_active,
        }
    }
}

/// Peer connection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConnectionStats {
    pub total_connected: usize,
    pub total_known: usize,
    pub average_reputation: f64,
    pub high_reputation_peers: usize,
    pub discovery_active: bool,
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new()
    }
}