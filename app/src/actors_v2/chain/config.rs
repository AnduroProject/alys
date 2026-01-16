//! ChainActor V2 Configuration
//!
//! Simplified configuration without complex supervision or actor_system dependencies

use ethereum_types::Address;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// ChainActor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    /// Whether this node is a validator (can produce blocks)
    pub is_validator: bool,

    /// Validator fee recipient address (for block production rewards)
    pub validator_address: Option<Address>,

    /// Federation member addresses
    pub federation: Vec<Address>,

    /// Maximum blocks to produce without AuxPoW
    pub max_blocks_without_pow: u64,

    /// Block production timeout
    pub block_production_timeout: Duration,

    /// Block validation timeout
    pub block_validation_timeout: Duration,

    /// Enable AuxPoW processing
    pub enable_auxpow: bool,

    /// Enable peg operations
    pub enable_peg_operations: bool,

    /// Bitcoin consensus parameters for difficulty retargeting
    pub retarget_params: Option<BitcoinConsensusParams>,

    /// Block hash cache size
    pub block_hash_cache_size: Option<usize>,

    /// Chain ID for AuxPoW validation
    ///
    /// Default: 1337 (Alys mainnet)
    /// Testnet should use different value to prevent replay attacks
    pub chain_id: u32,
}

/// Bitcoin consensus parameters (simplified from auxpow_miner)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinConsensusParams {
    pub target_spacing: Duration,
    pub target_timespan: Duration,
    pub retarget_interval: u32,
    pub max_target: u32,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            is_validator: false,
            validator_address: None,
            federation: Vec::new(),
            max_blocks_without_pow: 100,
            block_production_timeout: Duration::from_secs(30),
            block_validation_timeout: Duration::from_secs(10),
            enable_auxpow: true,
            enable_peg_operations: true,
            retarget_params: Some(BitcoinConsensusParams::default()),
            block_hash_cache_size: Some(1000),
            chain_id: 1337, // Alys mainnet
        }
    }
}

impl Default for BitcoinConsensusParams {
    fn default() -> Self {
        Self {
            target_spacing: Duration::from_secs(600),      // 10 minutes
            target_timespan: Duration::from_secs(1209600), // 2 weeks
            retarget_interval: 2016,
            max_target: 0x1d00ffff,
        }
    }
}

impl ChainConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), crate::actors_v2::chain::ChainError> {
        if self.max_blocks_without_pow == 0 {
            return Err(crate::actors_v2::chain::ChainError::Configuration(
                "max_blocks_without_pow must be greater than 0".to_string(),
            ));
        }

        if self.block_production_timeout.is_zero() {
            return Err(crate::actors_v2::chain::ChainError::Configuration(
                "block_production_timeout must be greater than 0".to_string(),
            ));
        }

        if self.block_validation_timeout.is_zero() {
            return Err(crate::actors_v2::chain::ChainError::Configuration(
                "block_validation_timeout must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}
