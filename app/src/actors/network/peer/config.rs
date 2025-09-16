//! PeerActor Configuration
//! 
//! Configuration structures for peer management, scoring, and discovery.

use std::time::Duration;
use std::net::IpAddr;
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};

/// Main configuration for PeerActor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Maximum number of concurrent peer connections
    pub max_peers: u32,
    /// Maximum number of inbound connections
    pub max_inbound_peers: u32,
    /// Maximum number of outbound connections
    pub max_outbound_peers: u32,
    /// Connection timeout duration
    pub connection_timeout: Duration,
    /// Health check interval for peer monitoring
    pub health_check_interval: Duration,
    /// Peer scoring configuration
    pub scoring_config: ScoringConfig,
    /// Peer discovery configuration
    pub discovery_config: PeerDiscoveryConfig,
    /// Bootstrap peers for initial connections
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Reserved peers that should always be connected
    pub reserved_peers: Vec<Multiaddr>,
    /// Banned peer addresses
    pub banned_peers: Vec<IpAddr>,
    /// Enable federation peer prioritization
    pub federation_priority: bool,
    /// Minimum peers required for normal operation
    pub min_peers: u32,
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            max_peers: 1000,
            max_inbound_peers: 500,
            max_outbound_peers: 500,
            connection_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(10),
            scoring_config: ScoringConfig::default(),
            discovery_config: PeerDiscoveryConfig::default(),
            bootstrap_peers: Vec::new(),
            reserved_peers: Vec::new(),
            banned_peers: Vec::new(),
            federation_priority: true,
            min_peers: 10,
        }
    }
}

impl PeerConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_peers == 0 {
            return Err("max_peers cannot be zero".to_string());
        }
        
        if self.max_inbound_peers + self.max_outbound_peers > self.max_peers {
            return Err("Sum of max_inbound_peers and max_outbound_peers cannot exceed max_peers".to_string());
        }
        
        if self.min_peers > self.max_peers {
            return Err("min_peers cannot exceed max_peers".to_string());
        }
        
        if self.connection_timeout.is_zero() {
            return Err("connection_timeout cannot be zero".to_string());
        }
        
        self.scoring_config.validate()?;
        self.discovery_config.validate()?;
        
        Ok(())
    }
}

/// Configuration for peer scoring system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Enable peer scoring
    pub enabled: bool,
    /// Base score for new peers
    pub base_score: f64,
    /// Maximum score a peer can achieve
    pub max_score: f64,
    /// Minimum score before peer is banned
    pub min_score: f64,
    /// Score decay rate per hour
    pub decay_rate: f64,
    /// Bonus score for federation peers
    pub federation_bonus: f64,
    /// Penalty for failed connections
    pub connection_failure_penalty: f64,
    /// Penalty for protocol violations
    pub protocol_violation_penalty: f64,
    /// Bonus for successful message routing
    pub message_success_bonus: f64,
    /// Weight for latency in scoring (lower is better)
    pub latency_weight: f64,
    /// Weight for uptime in scoring
    pub uptime_weight: f64,
    /// Scoring update interval
    pub update_interval: Duration,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_score: 0.0,
            max_score: 100.0,
            min_score: -50.0,
            decay_rate: 0.1,
            federation_bonus: 20.0,
            connection_failure_penalty: 5.0,
            protocol_violation_penalty: 10.0,
            message_success_bonus: 1.0,
            latency_weight: 0.3,
            uptime_weight: 0.4,
            update_interval: Duration::from_secs(60),
        }
    }
}

impl ScoringConfig {
    /// Validate scoring configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_score <= self.min_score {
            return Err("max_score must be greater than min_score".to_string());
        }
        
        if self.decay_rate < 0.0 || self.decay_rate > 1.0 {
            return Err("decay_rate must be between 0.0 and 1.0".to_string());
        }
        
        if self.update_interval.is_zero() {
            return Err("update_interval cannot be zero".to_string());
        }
        
        Ok(())
    }
}

/// Configuration for peer discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDiscoveryConfig {
    /// Enable peer discovery
    pub enabled: bool,
    /// Enable mDNS for local peer discovery
    pub enable_mdns: bool,
    /// Enable Kademlia DHT for peer discovery
    pub enable_kademlia: bool,
    /// Discovery interval
    pub discovery_interval: Duration,
    /// Maximum discovered peers to track
    pub max_discovered_peers: u32,
    /// Time to keep discovered peer info
    pub peer_info_ttl: Duration,
    /// Bootstrap nodes for DHT
    pub bootstrap_nodes: Vec<Multiaddr>,
    /// Discovery query timeout
    pub query_timeout: Duration,
    /// Number of closest peers to query
    pub closest_peers_to_query: u32,
}

impl Default for PeerDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_mdns: true,
            enable_kademlia: true,
            discovery_interval: Duration::from_secs(30),
            max_discovered_peers: 10000,
            peer_info_ttl: Duration::from_secs(3600), // 1 hour
            bootstrap_nodes: Vec::new(),
            query_timeout: Duration::from_secs(10),
            closest_peers_to_query: 20,
        }
    }
}

impl PeerDiscoveryConfig {
    /// Validate discovery configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_discovered_peers == 0 {
            return Err("max_discovered_peers cannot be zero".to_string());
        }
        
        if self.discovery_interval.is_zero() {
            return Err("discovery_interval cannot be zero".to_string());
        }
        
        if self.peer_info_ttl.is_zero() {
            return Err("peer_info_ttl cannot be zero".to_string());
        }
        
        if self.query_timeout.is_zero() {
            return Err("query_timeout cannot be zero".to_string());
        }
        
        if self.closest_peers_to_query == 0 {
            return Err("closest_peers_to_query cannot be zero".to_string());
        }
        
        Ok(())
    }
}