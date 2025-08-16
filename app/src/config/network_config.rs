//! Network and P2P configuration

use super::*;
use std::net::SocketAddr;
use std::time::Duration;

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub listen_addr: SocketAddr,
    pub external_addr: Option<SocketAddr>,
    pub bootnodes: Vec<String>,
    pub max_peers: usize,
    pub connection_timeout: Duration,
    pub discovery: DiscoveryConfig,
}

/// Discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub mdns: bool,
    pub kademlia: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:30303".parse().unwrap(),
            external_addr: None,
            bootnodes: Vec::new(),
            max_peers: 50,
            connection_timeout: Duration::from_secs(10),
            discovery: DiscoveryConfig::default(),
        }
    }
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mdns: true,
            kademlia: true,
        }
    }
}

impl Validate for NetworkConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_peers == 0 {
            return Err(ConfigError::ValidationError {
                field: "network.max_peers".to_string(),
                reason: "Max peers must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}