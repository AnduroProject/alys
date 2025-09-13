//! Configuration types for V2 AuxPow system
//!
//! Provides configuration structures for AuxPowActor and DifficultyManager

use std::time::Duration;
use ethereum_types::Address as EvmAddress;
use bitcoin::{BlockHash, CompactTarget, Target};
use bitcoin::consensus::Encodable;
use bitcoin::consensus::Decodable;
use bitcoin::string::FromHexStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error as _;
use serde::ser::Error as _;
use crate::types::blockchain::AuxPowHeader;
use crate::actors::auxpow::types::AuxPow;
use eyre::Result;

// Serialization helpers for AuxBlock (migrated from legacy auxpow_miner.rs)
fn compact_target_to_hex<S>(bits: &CompactTarget, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&format!("{:x}", bits.to_consensus()))
}

fn compact_target_from_hex<'de, D>(deserializer: D) -> Result<CompactTarget, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    CompactTarget::from_hex_str_no_prefix(s).map_err(D::Error::custom)
}

fn block_hash_to_consensus_hex<S>(block_hash: &BlockHash, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut encoded_block_hash = Vec::new();
    block_hash
        .consensus_encode(&mut encoded_block_hash)
        .map_err(S::Error::custom)?;
    let stringified_auxpow = hex::encode(encoded_block_hash);

    s.serialize_str(&stringified_auxpow)
}

fn block_hash_from_consensus_hex<'de, D>(deserializer: D) -> Result<BlockHash, D::Error>
where
    D: Deserializer<'de>,
{
    let blockhash_str: &str = Deserialize::deserialize(deserializer)?;
    // Note: BlockHash::from_slice results in opposite endianness from BlockHash::from_str
    let blockhash_bytes = hex::decode(blockhash_str).map_err(D::Error::custom)?;
    BlockHash::consensus_decode(&mut blockhash_bytes.as_slice()).map_err(D::Error::custom)
}

/// AuxBlock structure for merged mining (migrated from legacy auxpow_miner.rs)
#[derive(Debug, Serialize, Deserialize)]
pub struct AuxBlock {
    #[serde(serialize_with = "block_hash_to_consensus_hex")]
    #[serde(deserialize_with = "block_hash_from_consensus_hex")]
    pub hash: BlockHash,
    #[serde(rename = "chainid")]
    pub chain_id: u32,
    #[serde(rename = "previousblockhash")]
    #[serde(serialize_with = "block_hash_to_consensus_hex")]
    #[serde(deserialize_with = "block_hash_from_consensus_hex")]
    pub previous_block_hash: BlockHash,
    #[serde(rename = "coinbasevalue")]
    pub coinbase_value: u64,
    #[serde(serialize_with = "compact_target_to_hex")]
    #[serde(deserialize_with = "compact_target_from_hex")]
    pub bits: CompactTarget,
    pub height: u64,
    pub _target: Target,
}

/// BlockIndex trait for mining operations (migrated from legacy auxpow_miner.rs)
pub trait BlockIndex {
    fn block_hash(&self) -> BlockHash;
    fn block_time(&self) -> u64;
    fn bits(&self) -> u32;
    fn chain_id(&self) -> u32;
    fn height(&self) -> u64;
}

/// Bitcoin consensus parameters (migrated from legacy auxpow_miner.rs)
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BitcoinConsensusParams {
    /// The proof of work limit of the bitcoin network
    pub pow_limit: u32,
    /// The proof of work lower limit
    pub pow_lower_limit: u32,
    /// The targeted timespan between difficulty adjustments
    pub pow_target_timespan: u64,
    /// The targeted interval between blocks
    pub pow_target_spacing: u64,
    /// Whether this chain supports proof of work retargeting or not
    pub pow_no_retargeting: bool,
    /// The maximum range of adjustment for the proof of work represented as a whole number percentage (e.g. 20 == 20%)
    pub max_pow_adjustment: u8,
}

impl BitcoinConsensusParams {
    #[allow(unused)]
    const BITCOIN_MAINNET: Self = Self {
        // https://github.com/rust-bitcoin/rust-bitcoin/blob/67793d04c302bd494519b20b44b260ec3ff8a2f1/bitcoin/src/pow.rs#L124C9-L124C90
        pow_limit: 486604799,
        pow_lower_limit: 439495319,
        pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
        pow_target_spacing: 10 * 60,            // ten minutes
        pow_no_retargeting: false,
        max_pow_adjustment: 20,
    };

    pub fn difficulty_adjustment_interval(&self) -> u64 {
        self.pow_target_timespan / self.pow_target_spacing
    }
}

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