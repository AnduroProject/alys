//! Configuration types for V2 AuxPow system
//!
//! Provides configuration structures for AuxPowActor and DifficultyManager

use std::time::Duration;
use ethereum_types::Address as EvmAddress;
use crate::auxpow_miner::BitcoinConsensusParams;

/// Configuration for AuxPowActor with legacy compatibility
#[derive(Debug, Clone)]
pub struct AuxPowConfig {
    /// Mining address for coinbase rewards
    pub mining_address: EvmAddress,
    /// Whether mining is enabled
    pub mining_enabled: bool,
    /// Whether to check sync status before mining
    pub sync_check_enabled: bool,
    /// How often to refresh work when no submissions
    pub work_refresh_interval: Duration,
    /// Maximum pending work items to track
    pub max_pending_work: usize,
}

impl Default for AuxPowConfig {
    fn default() -> Self {
        Self {
            mining_address: EvmAddress::zero(),
            mining_enabled: false,
            sync_check_enabled: true,
            work_refresh_interval: Duration::from_secs(30),
            max_pending_work: 100,
        }
    }
}

/// Configuration for DifficultyManager
#[derive(Debug, Clone)]
pub struct DifficultyConfig {
    /// Bitcoin consensus parameters (from chain spec)
    pub consensus_params: BitcoinConsensusParams,
    /// Number of difficulty entries to keep in history
    pub history_size: usize,
    /// Whether to enable result caching for performance
    pub enable_caching: bool,
    /// How often to cleanup expired cache entries
    pub cache_cleanup_interval: Duration,
}

impl Default for DifficultyConfig {
    fn default() -> Self {
        Self {
            consensus_params: BitcoinConsensusParams::default(),
            history_size: 2016, // Bitcoin's full difficulty adjustment window
            enable_caching: true,
            cache_cleanup_interval: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl DifficultyConfig {
    /// Create config for Bitcoin mainnet parameters
    pub fn bitcoin_mainnet() -> Self {
        Self {
            consensus_params: BitcoinConsensusParams::BITCOIN_MAINNET,
            ..Default::default()
        }
    }
    
    /// Create config for testing with faster adjustments
    pub fn test_config() -> Self {
        Self {
            consensus_params: BitcoinConsensusParams {
                pow_target_spacing: 2, // 2 seconds for Alys blocks
                pow_target_timespan: 20, // 20 seconds for testing
                max_pow_adjustment: 50, // Allow larger adjustments for testing
                ..BitcoinConsensusParams::default()
            },
            history_size: 10, // Smaller history for testing
            ..Default::default()
        }
    }
}