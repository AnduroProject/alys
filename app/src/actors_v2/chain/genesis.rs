//! Genesis block creation for V2 consensus layer
//!
//! This module handles creating the genesis block (height 0) by querying
//! the execution layer for block #0 and wrapping it in a ConsensusBlock.
//!
//! The genesis block serves as the common foundation that all validator nodes
//! share, ensuring consensus starts from the same state.
//!
//! ## Tendermint Genesis Configuration
//!
//! For Tendermint consensus, genesis includes:
//! - Initial validator set with public keys and voting powers
//! - Consensus parameters (timeouts, max validators)
//! - Governance authority (multisig for parameter changes)
//! - Bridge configuration (peg-in compensation, thresholds)

use crate::actors_v2::chain::tendermint::params::ChainParams;
use crate::actors_v2::chain::tendermint::pegin::PegInCompensation;
use crate::actors_v2::chain::tendermint::types::{ValidatorSet, VotingPower};
use crate::actors_v2::chain::ChainError;
use crate::actors_v2::engine::EngineActor;
use crate::block::SignedConsensusBlock;
use crate::spec::ChainSpec;
use actix::Addr;
use ethereum_types::Address;
use lighthouse_wrapper::bls::PublicKey;
use lighthouse_wrapper::types::MainnetEthSpec;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{debug, info};

/// Create genesis block by querying execution layer for block #0
///
/// This function:
/// 1. Queries the execution layer (Reth/Geth) for block #0
/// 2. Wraps the execution payload in a ConsensusBlock structure
/// 3. Returns a genesis block ready for storage
///
/// # Genesis Block Properties
/// - Height: 0
/// - Slot: 0
/// - Parent hash: 0x0000...0000 (genesis has no parent)
/// - Execution payload: Retrieved from execution layer block #0
/// - Signature: Empty (genesis is not signed by any authority)
///
/// # Determinism
/// All nodes using the same genesis.json will produce identical genesis blocks
/// because the execution layer's block #0 is deterministically generated from
/// the genesis.json configuration.
///
/// # Arguments
/// * `engine_actor` - Address of the EngineActor for querying execution layer
/// * `chain_spec` - Chain specification (authorities, slot duration, etc.)
///
/// # Returns
/// - `Ok(SignedConsensusBlock)` - Genesis block ready for storage
/// - `Err(ChainError)` - If execution layer query fails
///
/// # Errors
/// Returns `ChainError::Engine` if:
/// - Cannot communicate with EngineActor
/// - Execution layer doesn't have block #0
/// - Execution payload is invalid
///
pub async fn create_genesis_block(
    engine_actor: &Addr<EngineActor>,
    chain_spec: ChainSpec,
) -> Result<SignedConsensusBlock<MainnetEthSpec>, ChainError> {
    info!("Creating genesis block from execution layer");

    // Query execution layer for block #0
    // We use the GetPayloadByTag message which accepts "0x0" or "earliest"
    let get_genesis_msg = crate::actors_v2::engine::messages::EngineMessage::GetPayloadByTag {
        block_tag: "0x0".to_string(), // Query block #0 (genesis)
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    debug!("Querying execution layer for block #0");

    let execution_payload = match engine_actor.send(get_genesis_msg).await {
        Ok(Ok(crate::actors_v2::engine::messages::EngineResponse::PayloadByTag { payload })) => {
            // Extract the Capella payload
            match payload {
                lighthouse_wrapper::types::ExecutionPayload::Capella(capella_payload) => {
                    info!(
                        block_number = capella_payload.block_number,
                        block_hash = %capella_payload.block_hash,
                        "Retrieved execution layer block #0"
                    );
                    capella_payload
                }
                _ => {
                    return Err(ChainError::Engine(
                        "Expected Capella execution payload for genesis".to_string(),
                    ));
                }
            }
        }
        Ok(Ok(_)) => {
            return Err(ChainError::Engine(
                "Unexpected response type from EngineActor".to_string(),
            ));
        }
        Ok(Err(e)) => {
            return Err(ChainError::Engine(format!(
                "Execution layer failed to provide block #0: {}",
                e
            )));
        }
        Err(e) => {
            return Err(ChainError::NetworkError(format!(
                "Failed to communicate with EngineActor: {}",
                e
            )));
        }
    };

    // Validate that we actually got block #0
    if execution_payload.block_number != 0 {
        return Err(ChainError::InvalidBlock(format!(
            "Expected block #0 from execution layer, got block #{}",
            execution_payload.block_number
        )));
    }

    // Wrap execution payload in a ConsensusBlock
    let genesis = SignedConsensusBlock::genesis(chain_spec, execution_payload);

    let genesis_hash = genesis.canonical_root();
    let genesis_exec_hash = genesis.message.execution_payload.block_hash;

    info!(
        consensus_hash = %genesis_hash,
        execution_hash = %genesis_exec_hash,
        "Genesis block created successfully"
    );

    Ok(genesis)
}

/// Check if genesis block exists in storage
///
/// Helper function to determine if genesis has already been initialized.
/// Used during ChainActor startup to decide whether to create genesis.
///
/// # Arguments
/// * `storage_actor` - Address of the StorageActor
///
/// # Returns
/// - `Ok(true)` - Genesis block exists in storage
/// - `Ok(false)` - Genesis block does not exist
/// - `Err(ChainError)` - Communication or query error
///
pub async fn genesis_exists(
    storage_actor: &Addr<crate::actors_v2::storage::StorageActor>,
) -> Result<bool, ChainError> {
    let get_genesis_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
        height: 0,
        correlation_id: Some(uuid::Uuid::new_v4()),
    };

    match storage_actor.send(get_genesis_msg).await {
        Ok(Ok(Some(_))) => Ok(true),
        Ok(Ok(None)) => Ok(false),
        Ok(Err(e)) => Err(ChainError::Storage(format!(
            "Failed to query genesis from storage: {}",
            e
        ))),
        Err(e) => Err(ChainError::NetworkError(format!(
            "Failed to communicate with StorageActor: {}",
            e
        ))),
    }
}

// =============================================================================
// Tendermint Genesis Configuration
// =============================================================================

/// Genesis configuration for Tendermint consensus.
///
/// This structure contains all initial parameters needed to bootstrap
/// a Tendermint-based chain. It is loaded from a genesis JSON file.
///
/// # Example JSON
///
/// ```json
/// {
///   "chain_id": "alys-mainnet",
///   "validators": [
///     {
///       "public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb",
///       "voting_power": 1
///     }
///   ],
///   "consensus_params": { ... },
///   "governance_authority": "0x...",
///   "bridge_config": { ... },
///   "pegin_compensation": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    /// Chain identifier (e.g., "alys-mainnet", "alys-testnet")
    pub chain_id: String,

    /// Initial genesis timestamp (Unix timestamp)
    pub genesis_time: u64,

    /// Initial validator set configuration
    pub validators: Vec<GenesisValidator>,

    /// Consensus parameters (timeouts, limits)
    pub consensus_params: TendermintConsensusParams,

    /// Governance authority address (multisig for parameter changes)
    pub governance_authority: Address,

    /// Bridge configuration
    pub bridge_config: BridgeConfig,

    /// Peg-in compensation parameters
    pub pegin_compensation: PegInCompensation,

    /// Optional: app state hash from execution layer genesis
    #[serde(default)]
    pub app_hash: Option<String>,
}

/// Validator configuration for genesis.
///
/// Each validator is specified by their BLS public key and initial voting power.
/// For Alys with equal voting, all validators typically have power = 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisValidator {
    /// BLS12-381 public key (hex encoded with 0x prefix)
    pub public_key: String,

    /// Initial voting power (typically 1 for equal voting)
    #[serde(default = "default_voting_power")]
    pub voting_power: VotingPower,

    /// Optional: human-readable moniker for the validator
    #[serde(default)]
    pub moniker: Option<String>,
}

fn default_voting_power() -> VotingPower {
    1
}

/// Tendermint consensus parameters for genesis.
///
/// These control the timing and limits of the consensus protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendermintConsensusParams {
    /// Proposal timeout in milliseconds
    #[serde(default = "default_propose_timeout")]
    pub propose_timeout_ms: u64,

    /// Prevote timeout in milliseconds
    #[serde(default = "default_prevote_timeout")]
    pub prevote_timeout_ms: u64,

    /// Precommit timeout in milliseconds
    #[serde(default = "default_precommit_timeout")]
    pub precommit_timeout_ms: u64,

    /// Timeout increment per round in milliseconds
    #[serde(default = "default_timeout_delta")]
    pub timeout_delta_ms: u64,

    /// Maximum number of validators
    #[serde(default = "default_max_validators")]
    pub max_validators: u32,
}

fn default_propose_timeout() -> u64 {
    3000
}
fn default_prevote_timeout() -> u64 {
    1000
}
fn default_precommit_timeout() -> u64 {
    1000
}
fn default_timeout_delta() -> u64 {
    500
}
fn default_max_validators() -> u32 {
    15
}

impl Default for TendermintConsensusParams {
    fn default() -> Self {
        Self {
            propose_timeout_ms: default_propose_timeout(),
            prevote_timeout_ms: default_prevote_timeout(),
            precommit_timeout_ms: default_precommit_timeout(),
            timeout_delta_ms: default_timeout_delta(),
            max_validators: default_max_validators(),
        }
    }
}

/// Bridge configuration for genesis.
///
/// Controls Bitcoin deposit/withdrawal parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Required Bitcoin confirmations for peg-ins
    #[serde(default = "default_btc_confirmations")]
    pub btc_confirmations: u32,

    /// Minimum peg-in/out amount in satoshis
    #[serde(default = "default_min_peg_amount")]
    pub min_peg_amount: u64,

    /// Maximum peg-in/out amount in satoshis
    #[serde(default = "default_max_peg_amount")]
    pub max_peg_amount: u64,

    /// Required federation signatures for peg-outs (e.g., 11 of 15)
    #[serde(default = "default_federation_threshold")]
    pub federation_threshold: u32,
}

fn default_btc_confirmations() -> u32 {
    6
}
fn default_min_peg_amount() -> u64 {
    10_000
}
fn default_max_peg_amount() -> u64 {
    100_000_000
}
fn default_federation_threshold() -> u32 {
    11
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            btc_confirmations: default_btc_confirmations(),
            min_peg_amount: default_min_peg_amount(),
            max_peg_amount: default_max_peg_amount(),
            federation_threshold: default_federation_threshold(),
        }
    }
}

/// Errors that can occur during genesis configuration processing.
#[derive(Debug, Clone, thiserror::Error)]
pub enum GenesisError {
    #[error("Invalid public key at index {index}: {error}")]
    InvalidPublicKey { index: usize, error: String },

    #[error("Invalid validator count: {count} (minimum {minimum}, maximum {maximum})")]
    InvalidValidatorCount {
        count: usize,
        minimum: usize,
        maximum: usize,
    },

    #[error("Duplicate validator public key at index {index}")]
    DuplicateValidator { index: usize },

    #[error("Invalid voting power at index {index}: must be > 0")]
    InvalidVotingPower { index: usize },

    #[error("Total voting power overflow")]
    VotingPowerOverflow,

    #[error("Invalid consensus parameter: {param} = {value} ({reason})")]
    InvalidConsensusParam {
        param: String,
        value: String,
        reason: String,
    },

    #[error("JSON parse error: {0}")]
    JsonParseError(String),
}

impl GenesisConfig {
    /// Load genesis configuration from JSON string.
    pub fn from_json(json: &str) -> Result<Self, GenesisError> {
        serde_json::from_str(json).map_err(|e| GenesisError::JsonParseError(e.to_string()))
    }

    /// Validate the genesis configuration.
    ///
    /// Checks:
    /// - Validator count within limits (4 minimum for BFT, max_validators ceiling)
    /// - All public keys are valid BLS12-381 keys
    /// - No duplicate validators
    /// - All voting powers are positive
    /// - Consensus parameters are within reasonable bounds
    pub fn validate(&self) -> Result<(), GenesisError> {
        // Validate validator count
        let min_validators = 4; // BFT requires at least 4 validators for 1 Byzantine fault
        let max_validators = self.consensus_params.max_validators as usize;

        if self.validators.len() < min_validators || self.validators.len() > max_validators {
            return Err(GenesisError::InvalidValidatorCount {
                count: self.validators.len(),
                minimum: min_validators,
                maximum: max_validators,
            });
        }

        // Validate each validator
        let mut seen_pubkeys = std::collections::HashSet::new();

        for (index, validator) in self.validators.iter().enumerate() {
            // Parse public key
            let pk_str = validator.public_key.strip_prefix("0x").unwrap_or(&validator.public_key);
            if PublicKey::from_str(&format!("0x{}", pk_str)).is_err() {
                return Err(GenesisError::InvalidPublicKey {
                    index,
                    error: "invalid BLS12-381 public key".to_string(),
                });
            }

            // Check for duplicates
            if !seen_pubkeys.insert(&validator.public_key) {
                return Err(GenesisError::DuplicateValidator { index });
            }

            // Validate voting power
            if validator.voting_power == 0 {
                return Err(GenesisError::InvalidVotingPower { index });
            }
        }

        // Validate consensus parameters
        self.validate_consensus_params()?;

        Ok(())
    }

    /// Validate consensus parameters.
    fn validate_consensus_params(&self) -> Result<(), GenesisError> {
        let params = &self.consensus_params;

        // Timeouts must be positive
        if params.propose_timeout_ms == 0 {
            return Err(GenesisError::InvalidConsensusParam {
                param: "propose_timeout_ms".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
            });
        }

        if params.prevote_timeout_ms == 0 {
            return Err(GenesisError::InvalidConsensusParam {
                param: "prevote_timeout_ms".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
            });
        }

        if params.precommit_timeout_ms == 0 {
            return Err(GenesisError::InvalidConsensusParam {
                param: "precommit_timeout_ms".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
            });
        }

        // Max validators must allow BFT (minimum 4)
        if params.max_validators < 4 {
            return Err(GenesisError::InvalidConsensusParam {
                param: "max_validators".to_string(),
                value: params.max_validators.to_string(),
                reason: "BFT requires at least 4 validators".to_string(),
            });
        }

        Ok(())
    }

    /// Convert genesis validators to ValidatorSet.
    ///
    /// This parses the public keys and creates a ValidatorSet ready
    /// for use by the consensus engine.
    pub fn to_validator_set(&self) -> Result<ValidatorSet, GenesisError> {
        let mut public_keys = Vec::with_capacity(self.validators.len());
        let mut powers = Vec::with_capacity(self.validators.len());

        for (index, validator) in self.validators.iter().enumerate() {
            let pk_str = validator.public_key.strip_prefix("0x").unwrap_or(&validator.public_key);
            let public_key = PublicKey::from_str(&format!("0x{}", pk_str)).map_err(|e| {
                GenesisError::InvalidPublicKey {
                    index,
                    error: format!("{:?}", e),
                }
            })?;

            public_keys.push(public_key);
            powers.push(validator.voting_power);
        }

        Ok(ValidatorSet::with_powers(public_keys, powers))
    }

    /// Convert genesis configuration to ChainParams.
    ///
    /// Creates the initial chain parameters from genesis values.
    pub fn to_chain_params(&self) -> ChainParams {
        ChainParams {
            pegin_compensation: self.pegin_compensation.clone(),
            btc_confirmations: self.bridge_config.btc_confirmations,
            min_peg_amount: self.bridge_config.min_peg_amount,
            max_peg_amount: self.bridge_config.max_peg_amount,
            federation_threshold: self.bridge_config.federation_threshold,
            propose_timeout_ms: self.consensus_params.propose_timeout_ms,
            prevote_timeout_ms: self.consensus_params.prevote_timeout_ms,
            precommit_timeout_ms: self.consensus_params.precommit_timeout_ms,
            timeout_delta_ms: self.consensus_params.timeout_delta_ms,
            max_validators: self.consensus_params.max_validators,
            // Use defaults for checkpoint config
            min_checkpoint_interval: 100,
            target_checkpoint_interval: 500,
            max_blocks_without_pow: 50_000,
            // Use defaults for fee schedule
            base_fee_floor: 1,
            base_fee_ceiling: 1000,
            // Emergency controls default to off
            chain_paused: false,
            pegins_paused: false,
            pegouts_paused: false,
        }
    }

    /// Create a minimal testnet genesis configuration.
    ///
    /// Useful for integration tests and local development.
    pub fn testnet(validators: Vec<PublicKey>) -> Self {
        Self {
            chain_id: "alys-testnet".to_string(),
            genesis_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            validators: validators
                .into_iter()
                .map(|pk| GenesisValidator {
                    public_key: format!("{:?}", pk),
                    voting_power: 1,
                    moniker: None,
                })
                .collect(),
            consensus_params: TendermintConsensusParams::default(),
            governance_authority: Address::zero(),
            bridge_config: BridgeConfig::default(),
            pegin_compensation: PegInCompensation::default(),
            app_hash: None,
        }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, GenesisError> {
        serde_json::to_string_pretty(self).map_err(|e| GenesisError::JsonParseError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aura::Authority;
    use lighthouse_wrapper::bls::Keypair;

    #[test]
    fn test_genesis_has_zero_height() {
        use crate::block::ConsensusBlock;

        let block = ConsensusBlock::default();
        let keypair = Keypair::random();
        let authority = Authority {
            signer: keypair.clone(),
            index: 0,
        };

        // Create a signed block with default values
        let signed_block = block.sign_block(&authority);

        // Default ConsensusBlock should have height 0
        assert_eq!(
            signed_block.message.execution_payload.block_number, 0,
            "Default block should have height 0"
        );
    }

    #[test]
    fn test_genesis_has_zero_parent_hash() {
        use crate::block::ConsensusBlock;
        use ethereum_types::H256;

        let block = ConsensusBlock::default();
        let keypair = Keypair::random();
        let authority = Authority {
            signer: keypair.clone(),
            index: 0,
        };

        let signed_block = block.sign_block(&authority);

        // Genesis parent hash should be zero
        assert_eq!(
            signed_block.message.parent_hash,
            H256::zero(),
            "Genesis block should have zero parent hash"
        );
    }

    // ==========================================================================
    // Tendermint Genesis Configuration Tests
    // ==========================================================================

    #[allow(dead_code)]
    fn test_pubkey() -> String {
        "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb".to_string()
    }

    #[allow(dead_code)]
    fn test_pubkey_2() -> String {
        "0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e".to_string()
    }

    fn valid_genesis_json() -> String {
        r#"{
            "chain_id": "alys-testnet",
            "genesis_time": 1700000000,
            "validators": [
                {"public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", "voting_power": 1},
                {"public_key": "0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e", "voting_power": 1},
                {"public_key": "0xb0e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc", "voting_power": 1},
                {"public_key": "0xb928f3beb93519eecf0145da903b40a4c97dca00b21f12ac0df3be9116ef2ef27b2ae6bcd4c5bc2d54ef5a70627efcb7", "voting_power": 1}
            ],
            "consensus_params": {
                "propose_timeout_ms": 3000,
                "prevote_timeout_ms": 1000,
                "precommit_timeout_ms": 1000,
                "timeout_delta_ms": 500,
                "max_validators": 15
            },
            "governance_authority": "0x0000000000000000000000000000000000000001",
            "bridge_config": {
                "btc_confirmations": 6,
                "min_peg_amount": 10000,
                "max_peg_amount": 100000000,
                "federation_threshold": 3
            },
            "pegin_compensation": {
                "miner_fee_bps": 50,
                "min_fee_satoshi": 1000,
                "max_fee_satoshi": 10000000
            }
        }"#
        .to_string()
    }

    #[test]
    fn test_genesis_config_parse() {
        let json = valid_genesis_json();
        let config = GenesisConfig::from_json(&json).expect("should parse");

        assert_eq!(config.chain_id, "alys-testnet");
        assert_eq!(config.validators.len(), 4);
        assert_eq!(config.consensus_params.propose_timeout_ms, 3000);
        assert_eq!(config.bridge_config.btc_confirmations, 6);
    }

    #[test]
    fn test_genesis_config_validate() {
        let json = valid_genesis_json();
        let config = GenesisConfig::from_json(&json).expect("should parse");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_genesis_config_too_few_validators() {
        let json = r#"{
            "chain_id": "test",
            "genesis_time": 0,
            "validators": [
                {"public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", "voting_power": 1}
            ],
            "consensus_params": {"max_validators": 15},
            "governance_authority": "0x0000000000000000000000000000000000000001",
            "bridge_config": {},
            "pegin_compensation": {"miner_fee_bps": 50, "min_fee_satoshi": 1000, "max_fee_satoshi": 10000000}
        }"#;

        let config = GenesisConfig::from_json(json).expect("should parse");
        let result = config.validate();
        assert!(matches!(result, Err(GenesisError::InvalidValidatorCount { .. })));
    }

    #[test]
    fn test_genesis_config_duplicate_validator() {
        let json = r#"{
            "chain_id": "test",
            "genesis_time": 0,
            "validators": [
                {"public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", "voting_power": 1},
                {"public_key": "0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e", "voting_power": 1},
                {"public_key": "0xb0e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc", "voting_power": 1},
                {"public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", "voting_power": 1}
            ],
            "consensus_params": {"max_validators": 15},
            "governance_authority": "0x0000000000000000000000000000000000000001",
            "bridge_config": {},
            "pegin_compensation": {"miner_fee_bps": 50, "min_fee_satoshi": 1000, "max_fee_satoshi": 10000000}
        }"#;

        let config = GenesisConfig::from_json(json).expect("should parse");
        let result = config.validate();
        assert!(matches!(result, Err(GenesisError::DuplicateValidator { .. })));
    }

    #[test]
    fn test_genesis_config_zero_voting_power() {
        let json = r#"{
            "chain_id": "test",
            "genesis_time": 0,
            "validators": [
                {"public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", "voting_power": 0},
                {"public_key": "0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e", "voting_power": 1},
                {"public_key": "0xb0e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc", "voting_power": 1},
                {"public_key": "0xb928f3beb93519eecf0145da903b40a4c97dca00b21f12ac0df3be9116ef2ef27b2ae6bcd4c5bc2d54ef5a70627efcb7", "voting_power": 1}
            ],
            "consensus_params": {"max_validators": 15},
            "governance_authority": "0x0000000000000000000000000000000000000001",
            "bridge_config": {},
            "pegin_compensation": {"miner_fee_bps": 50, "min_fee_satoshi": 1000, "max_fee_satoshi": 10000000}
        }"#;

        let config = GenesisConfig::from_json(json).expect("should parse");
        let result = config.validate();
        assert!(matches!(result, Err(GenesisError::InvalidVotingPower { index: 0 })));
    }

    #[test]
    fn test_genesis_to_validator_set() {
        let json = valid_genesis_json();
        let config = GenesisConfig::from_json(&json).expect("should parse");
        let validator_set = config.to_validator_set().expect("should convert");

        assert_eq!(validator_set.len(), 4);
        assert_eq!(validator_set.total_power(), 4);
        // With 4 validators, threshold should be 3 (floor(4*2/3)+1)
        assert_eq!(validator_set.two_thirds_threshold(), 3);
    }

    #[test]
    fn test_genesis_to_chain_params() {
        let json = valid_genesis_json();
        let config = GenesisConfig::from_json(&json).expect("should parse");
        let params = config.to_chain_params();

        assert_eq!(params.propose_timeout_ms, 3000);
        assert_eq!(params.btc_confirmations, 6);
        assert_eq!(params.pegin_compensation.miner_fee_bps, 50);
        assert_eq!(params.max_validators, 15);
    }

    #[test]
    fn test_genesis_testnet_helper() {
        let keypairs: Vec<_> = (0..4).map(|_| Keypair::random()).collect();
        let pubkeys: Vec<_> = keypairs.iter().map(|kp| kp.pk.clone()).collect();

        let genesis = GenesisConfig::testnet(pubkeys);

        assert_eq!(genesis.chain_id, "alys-testnet");
        assert_eq!(genesis.validators.len(), 4);
        assert!(genesis.validate().is_ok());
    }

    #[test]
    fn test_genesis_json_roundtrip() {
        let json = valid_genesis_json();
        let config = GenesisConfig::from_json(&json).expect("should parse");
        let serialized = config.to_json().expect("should serialize");
        let config2 = GenesisConfig::from_json(&serialized).expect("should parse again");

        assert_eq!(config.chain_id, config2.chain_id);
        assert_eq!(config.validators.len(), config2.validators.len());
    }

    #[test]
    fn test_consensus_params_defaults() {
        let params = TendermintConsensusParams::default();
        assert_eq!(params.propose_timeout_ms, 3000);
        assert_eq!(params.prevote_timeout_ms, 1000);
        assert_eq!(params.precommit_timeout_ms, 1000);
        assert_eq!(params.timeout_delta_ms, 500);
        assert_eq!(params.max_validators, 15);
    }

    #[test]
    fn test_bridge_config_defaults() {
        let config = BridgeConfig::default();
        assert_eq!(config.btc_confirmations, 6);
        assert_eq!(config.min_peg_amount, 10_000);
        assert_eq!(config.max_peg_amount, 100_000_000);
        assert_eq!(config.federation_threshold, 11);
    }

    #[test]
    fn test_genesis_config_invalid_consensus_params() {
        let json = r#"{
            "chain_id": "test",
            "genesis_time": 0,
            "validators": [
                {"public_key": "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb", "voting_power": 1},
                {"public_key": "0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e", "voting_power": 1},
                {"public_key": "0xb0e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc", "voting_power": 1},
                {"public_key": "0xb928f3beb93519eecf0145da903b40a4c97dca00b21f12ac0df3be9116ef2ef27b2ae6bcd4c5bc2d54ef5a70627efcb7", "voting_power": 1}
            ],
            "consensus_params": {
                "propose_timeout_ms": 0,
                "max_validators": 15
            },
            "governance_authority": "0x0000000000000000000000000000000000000001",
            "bridge_config": {},
            "pegin_compensation": {"miner_fee_bps": 50, "min_fee_satoshi": 1000, "max_fee_satoshi": 10000000}
        }"#;

        let config = GenesisConfig::from_json(json).expect("should parse");
        let result = config.validate();
        assert!(matches!(result, Err(GenesisError::InvalidConsensusParam { .. })));
    }
}
