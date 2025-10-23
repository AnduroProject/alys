//! NetworkActor V2 Configuration
//!
//! Simplified configuration structures for two-actor P2P networking.
//! Removed complex V1 configurations and supervision settings.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// NetworkActor configuration - P2P protocols only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network addresses to listen on
    pub listen_addresses: Vec<String>,
    /// Bootstrap peers for initial connectivity
    pub bootstrap_peers: Vec<String>,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Gossip topics to subscribe to
    pub gossip_topics: Vec<String>,
    /// Maximum message size for gossip
    pub message_size_limit: usize,
    /// Peer discovery interval
    pub discovery_interval: Duration,
    /// Automatically dial mDNS discovered peers (Phase 2 Task 2.4)
    pub auto_dial_mdns_peers: bool,

    // Phase 4: Connection limits
    /// Maximum connections from a single IP address
    pub max_connections_per_ip: usize,
    /// Maximum inbound connections
    pub max_inbound_connections: usize,
    /// Maximum outbound connections
    pub max_outbound_connections: usize,

    // Phase 4: Rate limits
    /// Maximum messages per peer per second
    pub max_messages_per_peer_per_second: u64,
    /// Maximum bytes per peer per second
    pub max_bytes_per_peer_per_second: u64,
    /// Rate limit window duration
    pub rate_limit_window: Duration,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".to_string()],
            bootstrap_peers: vec![],
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            gossip_topics: vec![
                "alys/blocks".to_string(),          // Regular block gossip
                "alys/blocks/priority".to_string(), // Priority block gossip
                "alys/transactions".to_string(),    // Transaction gossip
                "alys/auxpow".to_string(),          // Phase 4: AuxPoW mining coordination
            ],
            message_size_limit: 1024 * 1024, // 1MB
            discovery_interval: Duration::from_secs(60),
            auto_dial_mdns_peers: true, // Phase 2 Task 2.4: Enable auto-dial for local network discovery

            // Phase 4: Connection limits (defaults)
            max_connections_per_ip: 5,
            max_inbound_connections: 500,
            max_outbound_connections: 500,

            // Phase 4: Rate limits (defaults)
            max_messages_per_peer_per_second: 100,
            max_bytes_per_peer_per_second: 1024 * 1024, // 1MB/s
            rate_limit_window: Duration::from_secs(1),
        }
    }
}

impl NetworkConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_addresses.is_empty() {
            return Err("At least one listen address must be specified".to_string());
        }

        if self.max_connections == 0 {
            return Err("Max connections must be greater than 0".to_string());
        }

        if self.message_size_limit == 0 {
            return Err("Message size limit must be greater than 0".to_string());
        }

        // Phase 4: Validate connection limits
        if self.max_connections_per_ip == 0 {
            return Err("Max connections per IP must be greater than 0".to_string());
        }

        if self.max_inbound_connections + self.max_outbound_connections > self.max_connections {
            return Err("Sum of max_inbound_connections and max_outbound_connections cannot exceed max_connections".to_string());
        }

        // Phase 4: Validate rate limits
        if self.max_messages_per_peer_per_second == 0 {
            return Err("Max messages per peer per second must be greater than 0".to_string());
        }

        if self.max_bytes_per_peer_per_second == 0 {
            return Err("Max bytes per peer per second must be greater than 0".to_string());
        }

        Ok(())
    }
}

/// SyncActor configuration - blockchain sync only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Maximum blocks to request at once
    pub max_blocks_per_request: u32,
    /// Sync request timeout
    pub sync_timeout: Duration,
    /// Number of parallel sync requests
    pub max_concurrent_requests: usize,
    /// Block validation timeout
    pub block_validation_timeout: Duration,
    /// Maximum sync peers to use
    pub max_sync_peers: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_blocks_per_request: 128,
            sync_timeout: Duration::from_secs(30),
            max_concurrent_requests: 4,
            block_validation_timeout: Duration::from_secs(10),
            max_sync_peers: 8,
        }
    }
}

impl SyncConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_blocks_per_request == 0 {
            return Err("Max blocks per request must be greater than 0".to_string());
        }

        if self.max_concurrent_requests == 0 {
            return Err("Max concurrent requests must be greater than 0".to_string());
        }

        if self.max_sync_peers == 0 {
            return Err("Max sync peers must be greater than 0".to_string());
        }

        Ok(())
    }
}
