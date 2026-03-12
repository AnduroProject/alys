//! Peer Manager V2
//!
//! Simplified peer management replacing V1 PeerActor (2,655 lines -> ~500-800 lines).
//! Removed: Kademlia DHT, complex supervision, actor_system dependencies
//! Added: Bootstrap-based discovery, basic reputation system
//! Phase 4: Advanced reputation tracking, violation management, DOS protection

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use super::super::messages::PeerId;
use super::super::metrics::update_prometheus_peer_reputations;

/// Default Instant value for deserialization
fn default_instant() -> Instant {
    Instant::now()
}

/// Phase 4: Peer violation types for reputation tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Violation {
    /// Peer sent invalid or malformed message
    InvalidMessage {
        #[serde(skip, default = "default_instant")]
        timestamp: Instant,
    },
    /// Peer exceeded message rate limit
    ExcessiveRate { messages_per_second: u64 },
    /// Peer sent malformed protocol data
    MalformedProtocol { details: String },
    /// Peer was unresponsive or timed out
    UnresponsivePeer { timeout_count: u32 },
    /// Peer sent oversized message
    OversizedMessage { size_bytes: usize },
    /// Peer sent invalid or malformed data (Phase 1: Block reception)
    InvalidData { reason: String },
}

/// Simplified peer information with Phase 4 enhancements
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
    // Phase 4: Advanced reputation tracking
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub violations: Vec<Violation>,
    #[serde(skip, default = "default_instant")]
    pub last_activity: Instant,
    // V2 protocol capability tracking
    /// Whether peer supports V2 block protocol (/alys/block/1.0.0)
    #[serde(default)]
    pub supports_v2_protocol: bool,
    /// All protocols this peer supports
    #[serde(default)]
    pub protocols: Vec<String>,
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
            // Phase 4: Initialize new fields
            bytes_sent: 0,
            bytes_received: 0,
            violations: Vec::new(),
            last_activity: Instant::now(),
            // V2 protocol capability (unknown until identify)
            supports_v2_protocol: false,
            protocols: Vec::new(),
        }
    }

    /// Phase 4: Get connection duration
    pub fn connection_duration(&self) -> Duration {
        self.connected_since
            .elapsed()
            .unwrap_or(Duration::from_secs(0))
    }

    /// Phase 4: Record violation
    pub fn add_violation(&mut self, violation: Violation) {
        self.violations.push(violation);
        self.last_activity = Instant::now();
    }

    /// Phase 4: Get recent violations (last hour)
    pub fn recent_violations_count(&self) -> usize {
        let one_hour_ago = Instant::now() - Duration::from_secs(3600);
        self.violations
            .iter()
            .filter(|v| match v {
                Violation::InvalidMessage { timestamp } => *timestamp > one_hour_ago,
                Violation::ExcessiveRate { .. } => true, // Always count rate violations
                Violation::MalformedProtocol { .. } => true,
                Violation::UnresponsivePeer { .. } => true,
                Violation::OversizedMessage { .. } => true,
                Violation::InvalidData { .. } => true, // Phase 1: Always count invalid data
            })
            .count()
    }

    /// Phase 4: Record bytes sent/received
    pub fn record_bytes(&mut self, sent: u64, received: u64) {
        self.bytes_sent += sent;
        self.bytes_received += received;
        self.last_activity = Instant::now();
    }

    /// Update reputation based on interaction
    pub fn update_reputation(&mut self, delta: f64) {
        self.reputation = (self.reputation + delta).max(0.0).min(100.0);
        self.last_seen = SystemTime::now();
        self.last_activity = Instant::now();
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

    /// Phase 4: Check if peer should be disconnected based on reputation and violations
    pub fn should_disconnect(&self) -> bool {
        self.reputation < 10.0 || self.success_rate() < 0.3 || self.recent_violations_count() > 10
    }

    /// Phase 4: Check if peer should be banned (stricter than disconnect)
    pub fn should_be_banned(&self) -> bool {
        self.reputation < -50.0 || self.recent_violations_count() > 20
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

    /// Add a new peer connection with a known good address (from outgoing/Dialer connections)
    pub fn add_peer(&mut self, peer_id: PeerId, address: String) {
        // Check if peer already exists - if so, just update the address
        if self.connected_peers.contains_key(&peer_id) {
            self.update_peer_address(&peer_id, address);
            return;
        }

        let peer_info = PeerInfo::new(peer_id.clone(), address);

        tracing::info!("Added peer connection: {}", peer_id);

        self.connected_peers
            .insert(peer_id.clone(), peer_info.clone());
        self.known_peers.insert(peer_id, peer_info);

        // Update Prometheus per-peer reputation metrics
        self.update_prometheus_metrics();
    }

    /// Track an incoming peer connection without overwriting existing address.
    /// For incoming (Listener) connections, the send_back_addr is an ephemeral port
    /// that cannot be used for reconnection. We only use it as a placeholder if
    /// we have no existing address for this peer.
    pub fn add_peer_incoming(&mut self, peer_id: PeerId, send_back_addr: String) {
        // If peer already connected, just update activity timestamp
        if let Some(peer_info) = self.connected_peers.get_mut(&peer_id) {
            peer_info.last_seen = std::time::SystemTime::now();
            peer_info.last_activity = std::time::Instant::now();
            tracing::debug!(
                peer_id = %peer_id,
                "Incoming connection from already-connected peer - keeping existing address"
            );
            return;
        }

        // Check if we have this peer in known_peers with a good address
        if let Some(known_peer) = self.known_peers.get(&peer_id) {
            // Reuse the known address (likely from a previous outgoing connection)
            let mut peer_info = known_peer.clone();
            peer_info.last_seen = std::time::SystemTime::now();
            peer_info.last_activity = std::time::Instant::now();
            tracing::info!(
                peer_id = %peer_id,
                address = %peer_info.address,
                "Incoming connection - using known address for reconnection"
            );
            self.connected_peers.insert(peer_id, peer_info);
        } else {
            // New peer we've never seen - use send_back_addr as placeholder
            // (reconnection may fail, but at least we track the peer)
            let peer_info = PeerInfo::new(peer_id.clone(), send_back_addr);
            tracing::info!(
                peer_id = %peer_id,
                "Added incoming peer connection (address may be ephemeral)"
            );
            self.connected_peers.insert(peer_id.clone(), peer_info.clone());
            self.known_peers.insert(peer_id, peer_info);
        }

        // Update Prometheus per-peer reputation metrics
        self.update_prometheus_metrics();
    }

    /// Update peer's address without resetting other fields
    /// This is called when PeerIdentified provides a potentially updated address
    pub fn update_peer_address(&mut self, peer_id: &PeerId, address: String) {
        // Update in connected_peers
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            if peer_info.address != address {
                tracing::debug!(
                    peer_id = %peer_id,
                    old_address = %peer_info.address,
                    new_address = %address,
                    "Updated peer address"
                );
                peer_info.address = address.clone();
                peer_info.last_seen = SystemTime::now();
                peer_info.last_activity = Instant::now();
            }
        }

        // Also update in known_peers for future reconnection
        if let Some(known_peer) = self.known_peers.get_mut(peer_id) {
            known_peer.address = address;
        }
    }

    /// Remove peer connection
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.connected_peers.remove(peer_id) {
            tracing::info!("Removed peer connection: {}", peer_id);

            // Keep in known_peers for potential reconnection
            self.known_peers.insert(peer_id.clone(), peer_info);

            // Update Prometheus per-peer reputation metrics (removes disconnected peer)
            self.update_prometheus_metrics();
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

    /// Update peer reputation (legacy method - kept for compatibility)
    pub fn update_peer_reputation(&mut self, peer_id: &PeerId, delta: f64) {
        self.update_reputation(peer_id, delta, "legacy_update");
    }

    /// Phase 4: Update peer reputation with decay, delta, reason and logging
    pub fn update_reputation(&mut self, peer_id: &PeerId, delta: f64, reason: &str) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            let old_reputation = peer_info.reputation;

            // Apply decay: reputation naturally trends toward neutral (50.0) over time
            let decay_factor = 0.01; // 1% decay toward neutral per update
            let decayed = peer_info.reputation + (50.0 - peer_info.reputation) * decay_factor;

            // Apply delta
            peer_info.reputation = (decayed + delta).max(-100.0).min(100.0);
            peer_info.last_seen = SystemTime::now();
            peer_info.last_activity = Instant::now();

            // Log significant changes
            if (old_reputation - peer_info.reputation).abs() > 5.0 || delta.abs() > 10.0 {
                tracing::warn!(
                    peer_id = %peer_id,
                    old_reputation = old_reputation,
                    new_reputation = peer_info.reputation,
                    delta = delta,
                    reason = reason,
                    "Significant reputation change"
                );
            } else {
                tracing::debug!(
                    peer_id = %peer_id,
                    reputation = peer_info.reputation,
                    delta = delta,
                    reason = reason,
                    "Updated peer reputation"
                );
            }

            // Also update in known_peers
            if let Some(known_peer) = self.known_peers.get_mut(peer_id) {
                known_peer.reputation = peer_info.reputation;
            }
        }

        // Update Prometheus per-peer reputation metrics
        self.update_prometheus_metrics();
    }

    /// Phase 4: Get peers below reputation threshold (for disconnection)
    pub fn get_low_reputation_peers(&self, threshold: f64) -> Vec<String> {
        self.connected_peers
            .values()
            .filter(|peer| peer.reputation < threshold)
            .map(|peer| peer.peer_id.clone())
            .collect()
    }

    /// Phase 4: Check if peer should be banned
    pub fn should_ban_peer(&self, peer_id: &str) -> bool {
        if let Some(peer_info) = self.connected_peers.get(peer_id) {
            peer_info.should_be_banned()
        } else {
            false
        }
    }

    /// Phase 4: Get average reputation across all connected peers
    pub fn get_average_reputation(&self) -> f64 {
        if self.connected_peers.is_empty() {
            return 50.0; // Neutral if no peers
        }

        let sum: f64 = self.connected_peers.values().map(|p| p.reputation).sum();

        sum / self.connected_peers.len() as f64
    }

    /// Phase 4: Add violation to peer
    pub fn add_peer_violation(&mut self, peer_id: &PeerId, violation: Violation) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            // Determine reputation penalty based on violation type
            let penalty = match &violation {
                Violation::InvalidMessage { .. } => -5.0,
                Violation::ExcessiveRate { .. } => -10.0,
                Violation::MalformedProtocol { .. } => -8.0,
                Violation::UnresponsivePeer { .. } => -3.0,
                Violation::OversizedMessage { .. } => -7.0,
                Violation::InvalidData { .. } => -5.0, // Phase 1: Penalty for invalid block data
            };

            peer_info.add_violation(violation.clone());

            let reason = format!("violation: {:?}", violation);
            self.update_reputation(peer_id, penalty, &reason);
        }
    }

    /// Record successful request to peer
    pub fn record_peer_success(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            peer_info.record_success();
            tracing::debug!(
                "Recorded success for peer {}: reputation = {:.1}",
                peer_id,
                peer_info.reputation
            );
        }
    }

    /// Record failed request to peer
    pub fn record_peer_failure(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            peer_info.record_failure();
            tracing::debug!(
                "Recorded failure for peer {}: reputation = {:.1}",
                peer_id,
                peer_info.reputation
            );
        }
    }

    /// Get best peers for requests (by reputation)
    pub fn get_best_peers(&self, count: usize) -> Vec<PeerId> {
        let mut peers: Vec<_> = self.connected_peers.values().collect();
        peers.sort_by(|a, b| {
            b.reputation
                .partial_cmp(&a.reputation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        peers
            .into_iter()
            .take(count)
            .map(|p| p.peer_id.clone())
            .collect()
    }

    /// Select best peers for block requests (Phase 4: Task 2.2)
    /// Criteria relaxed to allow new peers: reputation >= 50.0, success_rate >= 0.5
    /// Falls back to best available peers if no peers meet criteria
    pub fn select_peers_for_blocks(&self, count: usize) -> Vec<PeerId> {
        // First, try peers meeting baseline criteria
        // Note: New peers start with reputation=50.0 and success_rate=0.5,
        // so we use >= to include them (they need a chance to prove themselves)
        let mut suitable_peers: Vec<_> = self
            .connected_peers
            .values()
            .filter(|peer| peer.reputation >= 50.0 && peer.success_rate() >= 0.5)
            .collect();

        // If no peers meet baseline criteria, fall back to best available peers
        // This prevents "No suitable peers" errors when all peers are new or recovering
        if suitable_peers.is_empty() {
            tracing::debug!(
                connected_peers = self.connected_peers.len(),
                "No peers meet baseline criteria for block requests - using best available"
            );
            return self.get_best_peers(count);
        }

        // Sort by reputation descending (prefer proven peers)
        suitable_peers.sort_by(|a, b| {
            b.reputation
                .partial_cmp(&a.reputation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        suitable_peers
            .into_iter()
            .take(count)
            .map(|p| p.peer_id.clone())
            .collect()
    }

    /// Get peers that should be disconnected
    ///
    /// IMPORTANT: Never returns ALL connected peers to prevent network isolation.
    /// This protects against the "reputation death spiral" where temporary sync failures
    /// cause all peers to be penalized and disconnected, leaving the node unable to
    /// recover. At least one peer is always kept connected.
    pub fn get_peers_to_disconnect(&self) -> Vec<PeerId> {
        let candidates: Vec<_> = self
            .connected_peers
            .values()
            .filter(|peer| peer.should_disconnect())
            .collect();

        // Safety: Never disconnect ALL peers - always keep at least 1 connected
        // This prevents complete network isolation from reputation penalties
        let total_connected = self.connected_peers.len();
        let max_to_disconnect = if total_connected > 1 {
            total_connected - 1
        } else {
            0 // Never disconnect the last peer
        };

        if candidates.len() > max_to_disconnect {
            tracing::warn!(
                total_candidates = candidates.len(),
                max_allowed = max_to_disconnect,
                total_connected = total_connected,
                "Limiting peer disconnections to prevent network isolation"
            );
        }

        // Sort by reputation (lowest first) and take only up to max_to_disconnect
        let mut sorted_candidates = candidates;
        sorted_candidates.sort_by(|a, b| {
            a.reputation
                .partial_cmp(&b.reputation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sorted_candidates
            .into_iter()
            .take(max_to_disconnect)
            .map(|p| p.peer_id.clone())
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
        self.known_peers
            .values()
            .filter(|peer| !self.connected_peers.contains_key(&peer.peer_id))
            .filter(|peer| peer.reputation > 20.0) // Only try peers with decent reputation
            .map(|peer| peer.address.clone())
            .collect()
    }

    /// Get connection statistics
    pub fn get_connection_stats(&self) -> PeerConnectionStats {
        let total_connected = self.connected_peers.len();
        let avg_reputation = if total_connected > 0 {
            self.connected_peers
                .values()
                .map(|p| p.reputation)
                .sum::<f64>()
                / total_connected as f64
        } else {
            0.0
        };

        let high_reputation_count = self
            .connected_peers
            .values()
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

    /// Update Prometheus metrics with current per-peer reputation scores
    /// This exports individual peer reputations for Grafana dashboards
    pub fn update_prometheus_metrics(&self) {
        let peer_reputations: Vec<(String, f64)> = self
            .connected_peers
            .iter()
            .map(|(peer_id, info)| (peer_id.clone(), info.reputation))
            .collect();

        update_prometheus_peer_reputations(&peer_reputations);

        tracing::trace!(
            peer_count = peer_reputations.len(),
            "Updated Prometheus per-peer reputation metrics"
        );
    }

    // ==================== V2 Protocol Capability Tracking ====================

    /// V2 block protocol identifier
    const V2_BLOCK_PROTOCOL: &'static str = "/alys/block/1.0.0";

    /// Update peer's protocol capabilities after identify exchange
    /// Returns true if peer supports V2 block protocol
    pub fn update_peer_protocols(&mut self, peer_id: &PeerId, protocols: Vec<String>) -> bool {
        let supports_v2 = protocols.iter().any(|p| p == Self::V2_BLOCK_PROTOCOL);

        // Update connected peer
        if let Some(peer_info) = self.connected_peers.get_mut(peer_id) {
            peer_info.protocols = protocols.clone();
            peer_info.supports_v2_protocol = supports_v2;
            peer_info.last_seen = SystemTime::now();
            peer_info.last_activity = Instant::now();

            if supports_v2 {
                tracing::info!(
                    peer_id = %peer_id,
                    "Peer identified as V2-capable (supports {})",
                    Self::V2_BLOCK_PROTOCOL
                );
            } else {
                tracing::debug!(
                    peer_id = %peer_id,
                    protocol_count = protocols.len(),
                    "Peer identified as V0 only (no V2 block protocol)"
                );
            }
        }

        // Also update known_peers for future reconnection
        if let Some(known_peer) = self.known_peers.get_mut(peer_id) {
            known_peer.protocols = protocols;
            known_peer.supports_v2_protocol = supports_v2;
        }

        supports_v2
    }

    /// Check if we have at least one connected V2-capable peer
    pub fn has_connected_v2_peer(&self) -> bool {
        self.connected_peers
            .values()
            .any(|p| p.supports_v2_protocol)
    }

    /// Get count of connected V2-capable peers
    pub fn connected_v2_peer_count(&self) -> usize {
        self.connected_peers
            .values()
            .filter(|p| p.supports_v2_protocol)
            .count()
    }

    /// Get connected V2-capable peers (for block requests)
    pub fn get_connected_v2_peers(&self) -> Vec<&PeerInfo> {
        self.connected_peers
            .values()
            .filter(|p| p.supports_v2_protocol)
            .collect()
    }

    /// Get disconnected V2-capable peers for reconnection attempts
    /// Returns peers that are known to support V2 but are not currently connected
    pub fn get_disconnected_v2_peers(&self) -> Vec<&PeerInfo> {
        self.known_peers
            .values()
            .filter(|peer| {
                peer.supports_v2_protocol
                    && !self.connected_peers.contains_key(&peer.peer_id)
                    && peer.reputation > 20.0 // Only try peers with decent reputation
            })
            .collect()
    }

    /// Get addresses of disconnected V2 peers for reconnection
    pub fn get_v2_reconnection_candidates(&self) -> Vec<(String, String)> {
        self.get_disconnected_v2_peers()
            .into_iter()
            .map(|p| (p.peer_id.clone(), p.address.clone()))
            .collect()
    }

    /// Check if disconnecting peer is V2-capable (called BEFORE remove_peer)
    /// Returns true if peer supports V2 protocol
    pub fn is_v2_peer(&self, peer_id: &PeerId) -> bool {
        // Check connected_peers first (peer is still there before remove_peer is called)
        if let Some(peer_info) = self.connected_peers.get(peer_id) {
            return peer_info.supports_v2_protocol;
        }
        // Fallback to known_peers
        if let Some(peer_info) = self.known_peers.get(peer_id) {
            return peer_info.supports_v2_protocol;
        }
        false
    }

    /// Get peer IDs of connected V2-capable peers (TM-B2)
    /// Returns Vec of peer_id strings for use in periodic mesh health checks
    pub fn get_v2_peer_ids(&self) -> Vec<String> {
        self.connected_peers
            .iter()
            .filter(|(_, p)| p.supports_v2_protocol)
            .map(|(peer_id, _)| peer_id.clone())
            .collect()
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
