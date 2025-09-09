//! Peer Information Store
//! 
//! Manages persistent and runtime peer information including addresses,
//! reputation scores, connection history, and metadata.

use std::collections::{HashMap, BTreeMap, VecDeque};
use std::time::{Duration, Instant, SystemTime};
use std::net::IpAddr;
use libp2p::{PeerId, Multiaddr};
use serde::{Deserialize, Serialize};

use actor_system::error::ActorResult;
use crate::actors::network::peer::config::PeerConfig;

/// Persistent peer information store
#[derive(Debug)]
pub struct PeerStore {
    /// Configuration
    config: PeerConfig,
    /// Peer information by peer ID
    peers: HashMap<PeerId, PeerInfo>,
    /// Peer addresses by IP for deduplication
    ip_index: HashMap<IpAddr, Vec<PeerId>>,
    /// Recently seen peers for fast lookups
    recent_peers: VecDeque<PeerId>,
    /// Federation members cache
    federation_members: HashMap<PeerId, FederationInfo>,
    /// Statistics
    stats: StoreStats,
}

impl PeerStore {
    /// Create a new peer store
    pub fn new(config: PeerConfig) -> ActorResult<Self> {
        Ok(Self {
            config,
            peers: HashMap::new(),
            ip_index: HashMap::new(),
            recent_peers: VecDeque::new(),
            federation_members: HashMap::new(),
            stats: StoreStats::default(),
        })
    }

    /// Add or update peer information
    pub fn add_peer(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) -> ActorResult<()> {
        let now = SystemTime::now();
        
        // Check if peer exists
        if let Some(peer_info) = self.peers.get_mut(&peer_id) {
            // Update existing peer
            peer_info.last_seen = now;
            peer_info.addresses.extend(addresses);
            peer_info.addresses.sort();
            peer_info.addresses.dedup();
        } else {
            // Create new peer entry
            let peer_info = PeerInfo {
                peer_id,
                addresses: addresses.clone(),
                first_seen: now,
                last_seen: now,
                connection_count: 0,
                successful_connections: 0,
                failed_connections: 0,
                last_connection_attempt: None,
                reputation_score: self.config.scoring_config.base_score,
                is_federation: false,
                is_reserved: self.config.reserved_peers.iter().any(|addr| addresses.contains(addr)),
                is_banned: false,
                ban_until: None,
                user_agent: None,
                supported_protocols: Vec::new(),
                metadata: BTreeMap::new(),
                connection_history: ConnectionHistory::default(),
            };
            
            self.peers.insert(peer_id, peer_info);
            self.stats.total_peers += 1;
            
            // Update IP index
            for addr in &addresses {
                if let Ok(ip) = extract_ip_from_multiaddr(addr) {
                    self.ip_index.entry(ip)
                        .or_insert_with(Vec::new)
                        .push(peer_id);
                }
            }
        }
        
        // Update recent peers
        if let Some(pos) = self.recent_peers.iter().position(|&p| p == peer_id) {
            self.recent_peers.remove(pos);
        }
        self.recent_peers.push_front(peer_id);
        
        // Limit recent peers size
        if self.recent_peers.len() > 1000 {
            self.recent_peers.pop_back();
        }
        
        Ok(())
    }

    /// Get peer information
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    /// Get mutable peer information
    pub fn get_peer_mut(&mut self, peer_id: &PeerId) -> Option<&mut PeerInfo> {
        self.peers.get_mut(peer_id)
    }

    /// Remove peer from store
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Option<PeerInfo> {
        if let Some(peer_info) = self.peers.remove(peer_id) {
            // Remove from IP index
            for addr in &peer_info.addresses {
                if let Ok(ip) = extract_ip_from_multiaddr(addr) {
                    if let Some(peer_list) = self.ip_index.get_mut(&ip) {
                        peer_list.retain(|&p| p != *peer_id);
                        if peer_list.is_empty() {
                            self.ip_index.remove(&ip);
                        }
                    }
                }
            }
            
            // Remove from recent peers
            self.recent_peers.retain(|&p| p != *peer_id);
            
            self.stats.total_peers = self.stats.total_peers.saturating_sub(1);
            Some(peer_info)
        } else {
            None
        }
    }

    /// Ban a peer
    pub fn ban_peer(&mut self, peer_id: &PeerId, duration: Option<Duration>) {
        if let Some(peer_info) = self.peers.get_mut(peer_id) {
            peer_info.is_banned = true;
            peer_info.ban_until = duration.map(|d| SystemTime::now() + d);
            self.stats.banned_peers += 1;
        }
    }

    /// Unban a peer
    pub fn unban_peer(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.peers.get_mut(peer_id) {
            if peer_info.is_banned {
                peer_info.is_banned = false;
                peer_info.ban_until = None;
                self.stats.banned_peers = self.stats.banned_peers.saturating_sub(1);
            }
        }
    }

    /// Check if peer is banned
    pub fn is_banned(&self, peer_id: &PeerId) -> bool {
        if let Some(peer_info) = self.peers.get(peer_id) {
            if !peer_info.is_banned {
                return false;
            }
            
            // Check if ban has expired
            if let Some(ban_until) = peer_info.ban_until {
                SystemTime::now() < ban_until
            } else {
                true // Permanent ban
            }
        } else {
            false
        }
    }

    /// Mark peer as federation member
    pub fn mark_federation_member(&mut self, peer_id: PeerId, info: FederationInfo) {
        if let Some(peer_info) = self.peers.get_mut(&peer_id) {
            peer_info.is_federation = true;
            peer_info.reputation_score += self.config.scoring_config.federation_bonus;
        }
        self.federation_members.insert(peer_id, info);
    }

    /// Get federation members
    pub fn get_federation_members(&self) -> impl Iterator<Item = (&PeerId, &FederationInfo)> {
        self.federation_members.iter()
    }

    /// Get peers by score (highest first)
    pub fn get_peers_by_score(&self, limit: Option<usize>) -> Vec<(&PeerId, &PeerInfo)> {
        let mut peers: Vec<_> = self.peers.iter().collect();
        peers.sort_by(|a, b| b.1.reputation_score.partial_cmp(&a.1.reputation_score)
            .unwrap_or(std::cmp::Ordering::Equal));
        
        if let Some(limit) = limit {
            peers.truncate(limit);
        }
        
        peers
    }

    /// Get connected peers
    pub fn get_connected_peers(&self) -> Vec<&PeerId> {
        self.peers.iter()
            .filter(|(_, info)| info.connection_history.is_connected)
            .map(|(peer_id, _)| peer_id)
            .collect()
    }

    /// Get recent peers
    pub fn get_recent_peers(&self, limit: usize) -> Vec<PeerId> {
        self.recent_peers.iter().take(limit).copied().collect()
    }

    /// Clean up expired data
    pub fn cleanup_expired(&mut self) {
        let now = SystemTime::now();
        let ttl = self.config.discovery_config.peer_info_ttl;
        
        // Remove old peers
        let mut to_remove = Vec::new();
        for (peer_id, peer_info) in &self.peers {
            if let Ok(age) = now.duration_since(peer_info.last_seen) {
                if age > ttl && !peer_info.is_reserved && !peer_info.is_federation {
                    to_remove.push(*peer_id);
                }
            }
        }
        
        for peer_id in to_remove {
            self.remove_peer(&peer_id);
        }
        
        // Unban expired bans
        for peer_info in self.peers.values_mut() {
            if peer_info.is_banned {
                if let Some(ban_until) = peer_info.ban_until {
                    if now >= ban_until {
                        peer_info.is_banned = false;
                        peer_info.ban_until = None;
                        self.stats.banned_peers = self.stats.banned_peers.saturating_sub(1);
                    }
                }
            }
        }
    }

    /// Get store statistics
    pub fn get_stats(&self) -> &StoreStats {
        &self.stats
    }

    /// Update connection statistics
    pub fn record_connection_attempt(&mut self, peer_id: &PeerId, success: bool) {
        if let Some(peer_info) = self.peers.get_mut(peer_id) {
            peer_info.last_connection_attempt = Some(SystemTime::now());
            peer_info.connection_count += 1;
            
            if success {
                peer_info.successful_connections += 1;
                peer_info.connection_history.is_connected = true;
                peer_info.connection_history.connected_at = Some(Instant::now());
            } else {
                peer_info.failed_connections += 1;
                // Apply penalty for failed connection
                peer_info.reputation_score -= self.config.scoring_config.connection_failure_penalty;
            }
        }
    }
}

/// Information about a peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub peer_id: PeerId,
    /// Known addresses for this peer
    pub addresses: Vec<Multiaddr>,
    /// When peer was first discovered
    pub first_seen: SystemTime,
    /// When peer was last seen
    pub last_seen: SystemTime,
    /// Total connection attempts
    pub connection_count: u64,
    /// Successful connections
    pub successful_connections: u64,
    /// Failed connections
    pub failed_connections: u64,
    /// Last connection attempt time
    pub last_connection_attempt: Option<SystemTime>,
    /// Reputation score
    pub reputation_score: f64,
    /// Whether this peer is a federation member
    pub is_federation: bool,
    /// Whether this is a reserved peer
    pub is_reserved: bool,
    /// Whether this peer is banned
    pub is_banned: bool,
    /// Ban expiry time (None = permanent)
    pub ban_until: Option<SystemTime>,
    /// User agent string
    pub user_agent: Option<String>,
    /// Supported protocols
    pub supported_protocols: Vec<String>,
    /// Additional metadata
    pub metadata: BTreeMap<String, String>,
    /// Connection history
    pub connection_history: ConnectionHistory,
}

/// Connection history for a peer
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConnectionHistory {
    /// Whether currently connected
    pub is_connected: bool,
    /// When connection was established
    pub connected_at: Option<Instant>,
    /// Total connection time
    pub total_connection_time: Duration,
    /// Number of disconnections
    pub disconnection_count: u32,
    /// Last disconnect reason
    pub last_disconnect_reason: Option<String>,
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationInfo {
    /// Member public key
    pub public_key: Vec<u8>,
    /// Member role/position
    pub role: String,
    /// Authority weight in consensus
    pub authority_weight: u32,
    /// When membership was verified
    pub verified_at: SystemTime,
}

/// Store statistics
#[derive(Debug, Default)]
pub struct StoreStats {
    /// Total peers in store
    pub total_peers: u64,
    /// Number of banned peers
    pub banned_peers: u64,
    /// Number of federation peers
    pub federation_peers: u64,
    /// Number of connected peers
    pub connected_peers: u64,
}

/// Extract IP address from multiaddr
fn extract_ip_from_multiaddr(addr: &Multiaddr) -> Result<IpAddr, &'static str> {
    for component in addr.iter() {
        match component {
            libp2p::multiaddr::Protocol::Ip4(ip) => return Ok(IpAddr::V4(ip)),
            libp2p::multiaddr::Protocol::Ip6(ip) => return Ok(IpAddr::V6(ip)),
            _ => continue,
        }
    }
    Err("No IP address found in multiaddr")
}