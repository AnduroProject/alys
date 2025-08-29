//! NetworkActor Configuration
//! 
//! Configuration structures for P2P networking including libp2p protocols,
//! gossip settings, discovery parameters, and transport options.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use libp2p::Multiaddr;

/// Complete network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network addresses to listen on
    pub listen_addresses: Vec<Multiaddr>,
    /// Bootstrap peers for initial connectivity
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Gossip protocol configuration
    pub gossip_config: GossipConfig,
    /// Peer discovery configuration
    pub discovery_config: DiscoveryConfig,
    /// Transport layer configuration
    pub transport_config: TransportConfig,
    /// Federation-specific settings
    pub federation_config: FederationNetworkConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
                "/ip4/0.0.0.0/udp/0/quic-v1".parse().unwrap(),
            ],
            bootstrap_peers: vec![],
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            gossip_config: GossipConfig::default(),
            discovery_config: DiscoveryConfig::default(),
            transport_config: TransportConfig::default(),
            federation_config: FederationNetworkConfig::default(),
        }
    }
}

/// Gossipsub protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// Maximum message size for gossip
    pub max_message_size: usize,
    /// Heartbeat interval for gossip maintenance
    pub heartbeat_interval: Duration,
    /// Number of peers to gossip to per heartbeat
    pub gossip_factor: f64,
    /// History length for duplicate message detection
    pub history_length: usize,
    /// History gossip factor
    pub history_gossip: usize,
    /// Message validation mode
    pub validation_mode: ValidationMode,
    /// Enable message signing
    pub message_signing: bool,
    /// Custom topics configuration
    pub topics: TopicConfig,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            max_message_size: 65536, // 64KB
            heartbeat_interval: Duration::from_secs(1),
            gossip_factor: 0.25,
            history_length: 5,
            history_gossip: 3,
            validation_mode: ValidationMode::Strict,
            message_signing: true,
            topics: TopicConfig::default(),
        }
    }
}

/// Message validation modes
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ValidationMode {
    /// No validation (fast but insecure)
    None,
    /// Basic validation (moderate security)
    Basic,
    /// Strict validation (highest security)
    Strict,
}

/// Topic configuration for gossipsub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    /// Block propagation topic settings
    pub blocks: TopicSettings,
    /// Transaction propagation topic settings
    pub transactions: TopicSettings,
    /// Federation messages topic settings
    pub federation: TopicSettings,
    /// Discovery topic settings
    pub discovery: TopicSettings,
}

impl Default for TopicConfig {
    fn default() -> Self {
        Self {
            blocks: TopicSettings::high_priority(),
            transactions: TopicSettings::normal_priority(),
            federation: TopicSettings::critical_priority(),
            discovery: TopicSettings::low_priority(),
        }
    }
}

/// Individual topic settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicSettings {
    /// Topic priority (affects routing)
    pub priority: TopicPriority,
    /// Maximum messages per interval
    pub rate_limit: Option<u32>,
    /// Rate limiting interval
    pub rate_interval: Duration,
    /// Enable topic-specific validation
    pub custom_validation: bool,
}

impl TopicSettings {
    pub fn critical_priority() -> Self {
        Self {
            priority: TopicPriority::Critical,
            rate_limit: None, // No rate limiting for critical messages
            rate_interval: Duration::from_secs(1),
            custom_validation: true,
        }
    }

    pub fn high_priority() -> Self {
        Self {
            priority: TopicPriority::High,
            rate_limit: Some(1000),
            rate_interval: Duration::from_secs(1),
            custom_validation: true,
        }
    }

    pub fn normal_priority() -> Self {
        Self {
            priority: TopicPriority::Normal,
            rate_limit: Some(100),
            rate_interval: Duration::from_secs(1),
            custom_validation: false,
        }
    }

    pub fn low_priority() -> Self {
        Self {
            priority: TopicPriority::Low,
            rate_limit: Some(10),
            rate_interval: Duration::from_secs(1),
            custom_validation: false,
        }
    }
}

/// Topic priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TopicPriority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

/// Peer discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Enable mDNS discovery
    pub enable_mdns: bool,
    /// mDNS service name
    pub mdns_service_name: String,
    /// Enable Kademlia DHT
    pub enable_kademlia: bool,
    /// Kademlia replication factor
    pub kademlia_replication_factor: usize,
    /// DHT query timeout
    pub dht_query_timeout: Duration,
    /// Bootstrap interval
    pub bootstrap_interval: Duration,
    /// Minimum peers before stopping discovery
    pub min_peers: usize,
    /// Target number of peers
    pub target_peers: usize,
    /// Discovery protocols to use
    pub protocols: Vec<DiscoveryProtocol>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_mdns: true,
            mdns_service_name: "alys".to_string(),
            enable_kademlia: true,
            kademlia_replication_factor: 20,
            dht_query_timeout: Duration::from_secs(10),
            bootstrap_interval: Duration::from_secs(30),
            min_peers: 5,
            target_peers: 50,
            protocols: vec![
                DiscoveryProtocol::MDNS,
                DiscoveryProtocol::Kademlia,
                DiscoveryProtocol::Bootstrap,
            ],
        }
    }
}

/// Discovery protocol types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryProtocol {
    MDNS,
    Kademlia,
    Bootstrap,
    Custom(String),
}

/// Transport layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Enable TCP transport
    pub enable_tcp: bool,
    /// TCP configuration
    pub tcp_config: TcpConfig,
    /// Enable QUIC transport
    pub enable_quic: bool,
    /// QUIC configuration
    pub quic_config: QuicConfig,
    /// Security configuration
    pub security_config: SecurityConfig,
    /// Connection limits
    pub connection_limits: ConnectionLimits,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            enable_tcp: true,
            tcp_config: TcpConfig::default(),
            enable_quic: true,
            quic_config: QuicConfig::default(),
            security_config: SecurityConfig::default(),
            connection_limits: ConnectionLimits::default(),
        }
    }
}

/// TCP transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConfig {
    /// TCP keepalive interval
    pub keepalive_interval: Option<Duration>,
    /// TCP nodelay setting
    pub nodelay: bool,
    /// Socket reuse address
    pub reuse_address: bool,
    /// Send buffer size
    pub send_buffer_size: Option<usize>,
    /// Receive buffer size
    pub recv_buffer_size: Option<usize>,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            keepalive_interval: Some(Duration::from_secs(30)),
            nodelay: true,
            reuse_address: true,
            send_buffer_size: Some(65536),
            recv_buffer_size: Some(65536),
        }
    }
}

/// QUIC transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    /// Maximum idle timeout
    pub max_idle_timeout: Duration,
    /// Keep alive interval
    pub keep_alive_interval: Duration,
    /// Maximum concurrent streams
    pub max_concurrent_streams: u32,
    /// Enable 0-RTT connections
    pub enable_0rtt: bool,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            max_idle_timeout: Duration::from_secs(60),
            keep_alive_interval: Duration::from_secs(10),
            max_concurrent_streams: 100,
            enable_0rtt: false, // Disabled for security
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable TLS encryption
    pub enable_tls: bool,
    /// Require encrypted connections
    pub require_encryption: bool,
    /// Enable noise protocol
    pub enable_noise: bool,
    /// Certificate path (if using TLS)
    pub cert_path: Option<String>,
    /// Private key path (if using TLS)
    pub key_path: Option<String>,
    /// Trusted certificate authorities
    pub ca_certs: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_tls: true,
            require_encryption: true,
            enable_noise: true,
            cert_path: None,
            key_path: None,
            ca_certs: vec![],
        }
    }
}

/// Connection limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionLimits {
    /// Maximum total connections
    pub max_connections: usize,
    /// Maximum connections per peer
    pub max_connections_per_peer: usize,
    /// Maximum pending incoming connections
    pub max_pending_incoming: usize,
    /// Maximum pending outgoing connections
    pub max_pending_outgoing: usize,
    /// Connection establishment timeout
    pub establishment_timeout: Duration,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            max_connections_per_peer: 3,
            max_pending_incoming: 100,
            max_pending_outgoing: 100,
            establishment_timeout: Duration::from_secs(30),
        }
    }
}

/// Federation-specific network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationNetworkConfig {
    /// Federation peer discovery
    pub federation_discovery: bool,
    /// Federation-only topics
    pub federation_topics: Vec<String>,
    /// Priority routing for federation peers
    pub priority_routing: bool,
    /// Federation message authentication
    pub federation_auth: bool,
    /// Consensus protocol settings
    pub consensus_config: ConsensusNetworkConfig,
}

impl Default for FederationNetworkConfig {
    fn default() -> Self {
        Self {
            federation_discovery: true,
            federation_topics: vec![
                "federation_consensus".to_string(),
                "federation_blocks".to_string(),
                "federation_coordination".to_string(),
            ],
            priority_routing: true,
            federation_auth: true,
            consensus_config: ConsensusNetworkConfig::default(),
        }
    }
}

/// Consensus networking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusNetworkConfig {
    /// Consensus message timeout
    pub message_timeout: Duration,
    /// Maximum consensus message size
    pub max_message_size: usize,
    /// Consensus round timeout
    pub round_timeout: Duration,
    /// Enable fast path for consensus
    pub enable_fast_path: bool,
}

impl Default for ConsensusNetworkConfig {
    fn default() -> Self {
        Self {
            message_timeout: Duration::from_millis(500),
            max_message_size: 1024 * 1024, // 1MB for consensus messages
            round_timeout: Duration::from_secs(2),
            enable_fast_path: true,
        }
    }
}

impl NetworkConfig {
    /// Create configuration optimized for federation nodes
    pub fn federation() -> Self {
        let mut config = Self::default();
        config.max_connections = 200; // More conservative for stability
        config.gossip_config.validation_mode = ValidationMode::Strict;
        config.federation_config.federation_discovery = true;
        config.federation_config.priority_routing = true;
        config.transport_config.security_config.require_encryption = true;
        config
    }

    /// Create configuration optimized for high-performance networking
    pub fn high_performance() -> Self {
        let mut config = Self::default();
        config.max_connections = 2000;
        config.gossip_config.heartbeat_interval = Duration::from_millis(500);
        config.gossip_config.gossip_factor = 0.5; // More aggressive gossip
        config.discovery_config.target_peers = 100;
        config.transport_config.quic_config.max_concurrent_streams = 200;
        config
    }

    /// Create configuration for resource-constrained environments
    pub fn lightweight() -> Self {
        let mut config = Self::default();
        config.max_connections = 50;
        config.gossip_config.max_message_size = 32768; // 32KB
        config.gossip_config.history_length = 3;
        config.discovery_config.target_peers = 20;
        config.discovery_config.min_peers = 3;
        config.transport_config.enable_quic = false; // TCP only
        config
    }

    /// Validate configuration for consistency and security
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("max_connections must be greater than 0".to_string());
        }

        if self.listen_addresses.is_empty() {
            return Err("At least one listen address must be specified".to_string());
        }

        if self.gossip_config.max_message_size == 0 {
            return Err("Gossip max_message_size must be greater than 0".to_string());
        }

        if self.discovery_config.min_peers > self.discovery_config.target_peers {
            return Err("min_peers cannot be greater than target_peers".to_string());
        }

        if self.transport_config.connection_limits.max_connections < self.max_connections {
            return Err("Transport max_connections must be at least as large as network max_connections".to_string());
        }

        // Validate security settings
        if self.transport_config.security_config.require_encryption &&
           !self.transport_config.security_config.enable_tls &&
           !self.transport_config.security_config.enable_noise {
            return Err("Encryption is required but no encryption protocol is enabled".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validation() {
        let config = NetworkConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn federation_config_characteristics() {
        let config = NetworkConfig::federation();
        assert_eq!(config.max_connections, 200);
        assert!(matches!(config.gossip_config.validation_mode, ValidationMode::Strict));
        assert!(config.federation_config.federation_discovery);
        assert!(config.federation_config.priority_routing);
        assert!(config.transport_config.security_config.require_encryption);
    }

    #[test]
    fn high_performance_config() {
        let config = NetworkConfig::high_performance();
        assert_eq!(config.max_connections, 2000);
        assert_eq!(config.gossip_config.heartbeat_interval, Duration::from_millis(500));
        assert_eq!(config.gossip_config.gossip_factor, 0.5);
        assert_eq!(config.discovery_config.target_peers, 100);
    }

    #[test]
    fn lightweight_config() {
        let config = NetworkConfig::lightweight();
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.gossip_config.max_message_size, 32768);
        assert_eq!(config.discovery_config.target_peers, 20);
        assert!(!config.transport_config.enable_quic);
    }

    #[test]
    fn config_validation_errors() {
        let mut config = NetworkConfig::default();

        // Test max_connections validation
        config.max_connections = 0;
        assert!(config.validate().is_err());

        // Test listen_addresses validation
        config.max_connections = 100;
        config.listen_addresses.clear();
        assert!(config.validate().is_err());

        // Test discovery peer validation
        config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/0".parse().unwrap()];
        config.discovery_config.min_peers = 100;
        config.discovery_config.target_peers = 50;
        assert!(config.validate().is_err());
    }

    #[test]
    fn topic_priority_ordering() {
        assert!(TopicPriority::Critical as u8 < TopicPriority::High as u8);
        assert!(TopicPriority::High as u8 < TopicPriority::Normal as u8);
        assert!(TopicPriority::Normal as u8 < TopicPriority::Low as u8);
    }

    #[test]
    fn topic_settings_creation() {
        let critical = TopicSettings::critical_priority();
        assert!(matches!(critical.priority, TopicPriority::Critical));
        assert!(critical.rate_limit.is_none());
        assert!(critical.custom_validation);

        let normal = TopicSettings::normal_priority();
        assert!(matches!(normal.priority, TopicPriority::Normal));
        assert_eq!(normal.rate_limit, Some(100));
        assert!(!normal.custom_validation);
    }
}