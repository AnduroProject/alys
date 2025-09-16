//! Bridge and peg operations configuration

use super::*;
use std::time::Duration;

/// Bridge configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub enabled: bool,
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: Option<String>,
    pub bitcoin_rpc_password: Option<String>,
    pub bridge_contract_address: String,
    pub min_confirmations_pegin: u32,
    pub min_confirmations_pegout: u32,
    pub federation_threshold: u32,
    pub monitoring_interval: Duration,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bitcoin_rpc_url: "http://localhost:8332".to_string(),
            bitcoin_rpc_user: None,
            bitcoin_rpc_password: None,
            bridge_contract_address: "0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB".to_string(),
            min_confirmations_pegin: 6,
            min_confirmations_pegout: 3,
            federation_threshold: 2,
            monitoring_interval: Duration::from_secs(30),
        }
    }
}

impl Validate for BridgeConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.federation_threshold == 0 {
            return Err(ConfigError::ValidationError {
                field: "bridge.federation_threshold".to_string(),
                reason: "Federation threshold must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}