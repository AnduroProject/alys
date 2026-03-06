//! ChainActor V2 Testing Framework
//!
//! Comprehensive testing infrastructure for ChainActor

pub mod fixtures;
pub mod integration;
pub mod unit;

pub use fixtures::*;

use ethereum_types::Address;
use tempfile::TempDir;

use crate::actors_v2::chain::{ChainConfig, ChainError};
use crate::auxpow_miner::BitcoinConsensusParams;
use crate::engine::Engine;
use bridge::{BitcoinSignatureCollector, BitcoinSigner, Bridge};

pub(crate) type BitcoinWallet = bridge::UtxoManager<bridge::Tree>;

/// Comprehensive ChainActor test harness with all required components
///
/// Note: With Tendermint-only consensus, Aura is no longer needed.
/// Block finality is proven via last_commit with 2/3+ validator signatures.
pub struct ChainTestHarness {
    pub temp_dir: TempDir,
    pub config: ChainConfig,

    // Core blockchain components
    pub engine: Engine,
    pub federation: Vec<Address>,

    // Bridge and Bitcoin components
    pub bridge: Bridge,
    pub bitcoin_wallet: BitcoinWallet,
    pub bitcoin_signature_collector: BitcoinSignatureCollector,
    pub maybe_bitcoin_signer: Option<BitcoinSigner>,
    pub retarget_params: BitcoinConsensusParams,
}

impl ChainTestHarness {
    /// Create new test harness with all components
    ///
    /// Note: With Tendermint-only consensus, Aura is no longer needed.
    pub async fn new() -> Result<Self, ChainTestError> {
        let temp_dir = TempDir::new().map_err(|e| ChainTestError::Setup(e.to_string()))?;
        let config = ChainConfig::default();

        // Create mock components for testing
        let engine = Self::create_mock_engine()?;
        let federation = Self::create_mock_federation();
        let bridge = Self::create_mock_bridge()?;
        let bitcoin_wallet = Self::create_mock_bitcoin_wallet()?;
        let bitcoin_signature_collector = Self::create_mock_signature_collector()?;
        let maybe_bitcoin_signer = Self::create_mock_bitcoin_signer();
        let retarget_params = BitcoinConsensusParams::default();

        Ok(Self {
            temp_dir,
            config,
            engine,
            federation,
            bridge,
            bitcoin_wallet,
            bitcoin_signature_collector,
            maybe_bitcoin_signer,
            retarget_params,
        })
    }

    /// Setup with custom configuration
    pub async fn with_config(config: ChainConfig) -> Result<Self, ChainTestError> {
        let mut harness = Self::new().await?;
        harness.config = config;
        Ok(harness)
    }

    /// Setup validator configuration
    pub async fn validator() -> Result<Self, ChainTestError> {
        let mut config = ChainConfig::default();
        config.is_validator = true;
        config.enable_auxpow = true;
        config.enable_peg_operations = true;
        config.federation = vec![
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            Address::from_low_u64_be(3),
        ];
        config.max_blocks_without_pow = 100;
        Self::with_config(config).await
    }

    /// Setup non-validator configuration
    pub async fn non_validator() -> Result<Self, ChainTestError> {
        let mut config = ChainConfig::default();
        config.is_validator = false;
        config.enable_auxpow = true;
        config.enable_peg_operations = false;
        Self::with_config(config).await
    }

    /// Setup follower configuration (alias for non_validator)
    pub async fn follower() -> Result<Self, ChainTestError> {
        Self::non_validator().await
    }

    /// Verify configuration is valid
    pub async fn verify_config(&self) -> Result<(), ChainTestError> {
        self.config
            .validate()
            .map_err(|e| ChainTestError::Configuration(e.to_string()))?;
        Ok(())
    }

    /// Create ChainState consuming the harness components
    /// This avoids the need to clone non-Clone types
    ///
    /// Note: With Tendermint-only consensus, Aura is no longer needed.
    pub fn into_chain_state(
        self,
        is_validator: bool,
        max_blocks_without_pow: u64,
        head: Option<crate::actors_v2::storage::actor::BlockRef>,
    ) -> crate::actors_v2::chain::ChainState {
        use crate::actors_v2::chain::ChainState;
        ChainState::new(
            self.federation,
            self.bridge,
            self.bitcoin_wallet,
            self.bitcoin_signature_collector,
            self.maybe_bitcoin_signer,
            self.retarget_params,
            is_validator,
            max_blocks_without_pow,
            head,
        )
    }

    // Mock component creation methods

    fn create_mock_engine() -> Result<Engine, ChainTestError> {
        // Create a mock Engine for testing with mock RPC endpoints
        use lighthouse_wrapper::execution_layer::HttpJsonRpc;
        use lighthouse_wrapper::sensitive_url::SensitiveUrl;
        let mock_url_api = SensitiveUrl::parse("http://127.0.0.1:8545")
            .map_err(|e| ChainTestError::Setup(format!("Failed to parse URL: {}", e)))?;
        let mock_url_execution = SensitiveUrl::parse("http://127.0.0.1:8551")
            .map_err(|e| ChainTestError::Setup(format!("Failed to parse URL: {}", e)))?;
        let mock_api = HttpJsonRpc::new(mock_url_api, None)
            .map_err(|e| ChainTestError::Setup(format!("Failed to create HttpJsonRpc: {:?}", e)))?;
        let mock_execution_api = HttpJsonRpc::new(mock_url_execution, None)
            .map_err(|e| ChainTestError::Setup(format!("Failed to create HttpJsonRpc: {:?}", e)))?;
        Ok(Engine::new(mock_api, mock_execution_api))
    }

    // Note: create_mock_aura() removed - Tendermint-only consensus doesn't use Aura

    fn create_mock_federation() -> Vec<Address> {
        vec![
            Address::from_low_u64_be(1),
            Address::from_low_u64_be(2),
            Address::from_low_u64_be(3),
        ]
    }

    fn create_mock_bridge() -> Result<Bridge, ChainTestError> {
        // Create a mock Bridge for testing
        use bitcoin::Address as BitcoinAddress;
        use bridge::BitcoinCore;
        use std::str::FromStr;

        let mock_bitcoin_addr =
            BitcoinAddress::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                .map_err(|e| {
                    ChainTestError::Setup(format!("Failed to parse mock bitcoin address: {}", e))
                })?
                .assume_checked();

        // Create a mock BitcoinCore for testing
        let mock_bitcoin_core = BitcoinCore::new("http://127.0.0.1:8332", "user", "pass");

        Ok(Bridge::new(
            mock_bitcoin_core,
            vec![mock_bitcoin_addr],
            6, // required confirmations
        ))
    }

    fn create_mock_bitcoin_wallet() -> Result<BitcoinWallet, ChainTestError> {
        // Create a mock Bitcoin wallet for testing
        use bitcoin::secp256k1::PublicKey;
        use bitcoin::Network;
        use bridge::Federation;
        use tempfile::tempdir;

        // Create mock Bitcoin PublicKey (different from lighthouse PublicKey)
        // Using a known valid secp256k1 public key
        let mock_pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let mock_pubkey_bytes = hex::decode(mock_pubkey_hex).map_err(|e| {
            ChainTestError::Setup(format!("Failed to decode mock bitcoin pubkey hex: {:?}", e))
        })?;
        let mock_pubkey = PublicKey::from_slice(&mock_pubkey_bytes).map_err(|e| {
            ChainTestError::Setup(format!("Failed to create mock bitcoin pubkey: {}", e))
        })?;

        let federation = Federation::new(
            vec![mock_pubkey],
            1,                // threshold
            Network::Regtest, // Use regtest network for testing
        );

        // Create a temporary database for testing
        let temp_dir = tempdir()
            .map_err(|e| ChainTestError::Setup(format!("Failed to create temp dir: {}", e)))?;
        let db_path = temp_dir.path().join("test_wallet");

        BitcoinWallet::new(db_path.to_str().unwrap(), federation)
            .map_err(|e| ChainTestError::Setup(format!("Failed to create wallet: {:?}", e)))
    }

    fn create_mock_signature_collector() -> Result<BitcoinSignatureCollector, ChainTestError> {
        // Create a mock signature collector for testing
        use bitcoin::secp256k1::PublicKey;
        use bitcoin::Network;
        use bridge::Federation;

        // Create mock Bitcoin PublicKey
        // Using the same known valid secp256k1 public key
        let mock_pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let mock_pubkey_bytes = hex::decode(mock_pubkey_hex).map_err(|e| {
            ChainTestError::Setup(format!("Failed to decode mock bitcoin pubkey hex: {:?}", e))
        })?;
        let mock_pubkey = PublicKey::from_slice(&mock_pubkey_bytes).map_err(|e| {
            ChainTestError::Setup(format!("Failed to create mock bitcoin pubkey: {}", e))
        })?;

        let federation = Federation::new(
            vec![mock_pubkey],
            1,                // threshold
            Network::Regtest, // Use regtest network for testing
        );

        Ok(BitcoinSignatureCollector::new(federation))
    }

    fn create_mock_bitcoin_signer() -> Option<BitcoinSigner> {
        // Create an optional mock Bitcoin signer for testing
        use bridge::BitcoinSecretKey;

        // Mock private key for testing (this is a dummy key, not secure)
        let mock_secret_key = BitcoinSecretKey::from_slice(&[0x01; 32]).ok()?;

        Some(BitcoinSigner::new(mock_secret_key))
    }
}

/// ChainActor test errors
#[derive(Debug, thiserror::Error)]
pub enum ChainTestError {
    #[error("Setup error: {0}")]
    Setup(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("State inconsistency: {0}")]
    StateInconsistency(String),

    #[error("Block operation error: {0}")]
    BlockOperation(String),

    #[error("AuxPoW operation error: {0}")]
    AuxPowOperation(String),

    #[error("Peg operation error: {0}")]
    PegOperation(String),

    #[error("Message not implemented: {0}")]
    MessageNotImplemented(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Chain error: {0}")]
    Chain(#[from] ChainError),
}
