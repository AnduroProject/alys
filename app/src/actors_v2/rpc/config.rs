use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

/// RPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// RPC server bind address
    pub bind_address: SocketAddr,

    /// Request timeout
    pub request_timeout: Duration,

    /// Enable request logging
    pub enable_logging: bool,

    /// Enable Prometheus metrics
    pub enable_metrics: bool,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:3001".parse().expect("Valid socket address"),
            request_timeout: Duration::from_secs(30),
            enable_logging: true,
            enable_metrics: true,
        }
    }
}

impl RpcConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.bind_address.port() == 0 {
            return Err("Invalid port number".to_string());
        }
        if self.request_timeout.is_zero() {
            return Err("Request timeout must be greater than zero".to_string());
        }
        Ok(())
    }
}
