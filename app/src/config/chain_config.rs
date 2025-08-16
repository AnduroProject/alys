//! Chain and consensus configuration

use super::*;
use crate::types::blockchain::ChainId;
use std::time::Duration;

/// Chain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub chain_id: ChainId,
    pub genesis_file: String,
    pub data_dir: String,
    pub slot_duration: Duration,
    pub max_blocks_without_pow: u64,
    pub authorities: Vec<String>,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_id: ChainId::Testnet,
            genesis_file: "./config/genesis.json".to_string(),
            data_dir: "./data/chain".to_string(),
            slot_duration: Duration::from_secs(2),
            max_blocks_without_pow: 10,
            authorities: Vec::new(),
        }
    }
}

impl Validate for ChainConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.authorities.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "chain.authorities".to_string(),
                reason: "At least one authority must be configured".to_string(),
            });
        }
        Ok(())
    }
}