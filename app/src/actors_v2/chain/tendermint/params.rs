//! Chain parameters that can be modified by governance.
//!
//! All governable parameters are enumerated here with their constraints.
//! See `17_GOVERNANCE_PARAMETERS.md` for complete documentation.

use super::pegin::PegInCompensation;
use lighthouse_wrapper::bls::Signature;
use lighthouse_wrapper::types::Hash256;
use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};

/// Enumeration of all governable parameters
///
/// Organized into ranges by category:
/// - 100-199: Peg-in compensation
/// - 200-299: Bridge configuration
/// - 300-399: Checkpoint configuration
/// - 400-499: Consensus parameters
/// - 500-599: Fee schedule
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum GovernableParam {
    // Peg-in compensation (100-199)
    /// Miner fee in basis points (50 = 0.5%)
    MinerFeeBps = 100,
    /// Minimum fee in satoshis (floor for small peg-ins)
    MinFeeSatoshi = 101,
    /// Maximum fee in satoshis (cap for large peg-ins)
    MaxFeeSatoshi = 102,

    // Bridge configuration (200-299)
    /// Required Bitcoin confirmations for peg-ins
    BtcConfirmations = 200,
    /// Minimum peg-in/out amount in satoshis
    MinPegAmount = 201,
    /// Maximum peg-in/out amount in satoshis
    MaxPegAmount = 202,
    /// Required federation signatures for peg-outs
    FederationThreshold = 203,

    // Checkpoint configuration (300-399)
    /// Minimum blocks between checkpoints
    MinCheckpointInterval = 300,
    /// Target checkpoint frequency
    TargetCheckpointInterval = 301,
    /// Liveness gate: max blocks without AuxPoW
    MaxBlocksWithoutPow = 304,

    // Consensus parameters (400-499)
    /// Proposal timeout in milliseconds
    ProposeTimeoutMs = 400,
    /// Prevote timeout in milliseconds
    PrevoteTimeoutMs = 401,
    /// Precommit timeout in milliseconds
    PrecommitTimeoutMs = 402,
    /// Timeout increase per round in milliseconds
    TimeoutDeltaMs = 403,
    /// Maximum validator count
    MaxValidators = 404,

    // Fee schedule (500-599)
    /// Minimum EVM base fee in gwei
    BaseFeeFloor = 500,
    /// Maximum EVM base fee in gwei
    BaseFeeCeiling = 501,
}

impl GovernableParam {
    /// Convert to bytes for storage key
    pub fn to_bytes(&self) -> [u8; 2] {
        (*self as u16).to_be_bytes()
    }

    /// Parse from bytes
    pub fn from_bytes(bytes: [u8; 2]) -> Option<Self> {
        let value = u16::from_be_bytes(bytes);
        Self::try_from_u16(value)
    }

    /// Try to convert from u16
    pub fn try_from_u16(value: u16) -> Option<Self> {
        match value {
            100 => Some(Self::MinerFeeBps),
            101 => Some(Self::MinFeeSatoshi),
            102 => Some(Self::MaxFeeSatoshi),
            200 => Some(Self::BtcConfirmations),
            201 => Some(Self::MinPegAmount),
            202 => Some(Self::MaxPegAmount),
            203 => Some(Self::FederationThreshold),
            300 => Some(Self::MinCheckpointInterval),
            301 => Some(Self::TargetCheckpointInterval),
            304 => Some(Self::MaxBlocksWithoutPow),
            400 => Some(Self::ProposeTimeoutMs),
            401 => Some(Self::PrevoteTimeoutMs),
            402 => Some(Self::PrecommitTimeoutMs),
            403 => Some(Self::TimeoutDeltaMs),
            404 => Some(Self::MaxValidators),
            500 => Some(Self::BaseFeeFloor),
            501 => Some(Self::BaseFeeCeiling),
            _ => None,
        }
    }

    /// Get the category name for this parameter
    pub fn category(&self) -> &'static str {
        match *self as u16 {
            100..=199 => "peg-in-compensation",
            200..=299 => "bridge-config",
            300..=399 => "checkpoint-config",
            400..=499 => "consensus-params",
            500..=599 => "fee-schedule",
            _ => "unknown",
        }
    }
}

/// Parameter value types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ParameterValue {
    U64(u64),
    U32(u32),
    Bool(bool),
    Bytes(Vec<u8>),
}

/// A parameter update from governance
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParameterUpdate {
    /// Which parameter to update
    pub param: GovernableParam,

    /// New value
    pub value: ParameterValue,

    /// Governance threshold signature
    pub governance_signature: Signature,
}

impl ParameterUpdate {
    /// Validate the parameter value against constraints
    pub fn validate(&self) -> Result<(), ParameterError> {
        match self.param {
            GovernableParam::MinerFeeBps => {
                if let ParameterValue::U64(v) = &self.value {
                    if *v > 10000 {
                        return Err(ParameterError::OutOfRange {
                            param: self.param,
                            value: format!("{}", v),
                            constraint: "0-10000".to_string(),
                        });
                    }
                }
            }
            GovernableParam::BtcConfirmations => {
                if let ParameterValue::U32(v) = &self.value {
                    if *v < 1 || *v > 100 {
                        return Err(ParameterError::OutOfRange {
                            param: self.param,
                            value: format!("{}", v),
                            constraint: "1-100".to_string(),
                        });
                    }
                }
            }
            GovernableParam::MaxValidators => {
                if let ParameterValue::U32(v) = &self.value {
                    if *v < 4 {
                        return Err(ParameterError::OutOfRange {
                            param: self.param,
                            value: format!("{}", v),
                            constraint: "4+ (BFT minimum)".to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Current chain parameter state
///
/// Holds all governable parameters with their current values.
/// Initialized from genesis and updated via governance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainParams {
    // Peg-in compensation
    pub pegin_compensation: PegInCompensation,

    // Bridge config
    pub btc_confirmations: u32,
    pub min_peg_amount: u64,
    pub max_peg_amount: u64,
    pub federation_threshold: u32,

    // Consensus params
    pub propose_timeout_ms: u64,
    pub prevote_timeout_ms: u64,
    pub precommit_timeout_ms: u64,
    pub timeout_delta_ms: u64,
    pub max_validators: u32,

    // Checkpoint config
    pub min_checkpoint_interval: u64,
    pub target_checkpoint_interval: u64,
    pub max_blocks_without_pow: u64,

    // Fee schedule
    pub base_fee_floor: u64,
    pub base_fee_ceiling: u64,

    // Emergency controls
    pub chain_paused: bool,
    pub pegins_paused: bool,
    pub pegouts_paused: bool,
}

impl ChainParams {
    /// Compute a hash of the current parameter state
    ///
    /// Used in block headers for light client verification.
    pub fn compute_hash(&self) -> Hash256 {
        let serialized =
            bincode::serialize(self).expect("ChainParams serialization should not fail");

        let mut hasher = Keccak::v256();
        hasher.update(&serialized);

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        Hash256::from_slice(&output)
    }

    /// Apply a parameter update
    pub fn apply_update(&mut self, update: &ParameterUpdate) -> Result<(), ParameterError> {
        update.validate()?;

        match update.param {
            GovernableParam::MinerFeeBps => {
                if let ParameterValue::U64(v) = update.value {
                    self.pegin_compensation.miner_fee_bps = v;
                }
            }
            GovernableParam::MinFeeSatoshi => {
                if let ParameterValue::U64(v) = update.value {
                    self.pegin_compensation.min_fee_satoshi = v;
                }
            }
            GovernableParam::MaxFeeSatoshi => {
                if let ParameterValue::U64(v) = update.value {
                    self.pegin_compensation.max_fee_satoshi = v;
                }
            }
            GovernableParam::BtcConfirmations => {
                if let ParameterValue::U32(v) = update.value {
                    self.btc_confirmations = v;
                }
            }
            GovernableParam::MinPegAmount => {
                if let ParameterValue::U64(v) = update.value {
                    self.min_peg_amount = v;
                }
            }
            GovernableParam::MaxPegAmount => {
                if let ParameterValue::U64(v) = update.value {
                    self.max_peg_amount = v;
                }
            }
            GovernableParam::FederationThreshold => {
                if let ParameterValue::U32(v) = update.value {
                    self.federation_threshold = v;
                }
            }
            GovernableParam::ProposeTimeoutMs => {
                if let ParameterValue::U64(v) = update.value {
                    self.propose_timeout_ms = v;
                }
            }
            GovernableParam::PrevoteTimeoutMs => {
                if let ParameterValue::U64(v) = update.value {
                    self.prevote_timeout_ms = v;
                }
            }
            GovernableParam::PrecommitTimeoutMs => {
                if let ParameterValue::U64(v) = update.value {
                    self.precommit_timeout_ms = v;
                }
            }
            GovernableParam::TimeoutDeltaMs => {
                if let ParameterValue::U64(v) = update.value {
                    self.timeout_delta_ms = v;
                }
            }
            GovernableParam::MaxValidators => {
                if let ParameterValue::U32(v) = update.value {
                    self.max_validators = v;
                }
            }
            GovernableParam::MinCheckpointInterval => {
                if let ParameterValue::U64(v) = update.value {
                    self.min_checkpoint_interval = v;
                }
            }
            GovernableParam::TargetCheckpointInterval => {
                if let ParameterValue::U64(v) = update.value {
                    self.target_checkpoint_interval = v;
                }
            }
            GovernableParam::MaxBlocksWithoutPow => {
                if let ParameterValue::U64(v) = update.value {
                    self.max_blocks_without_pow = v;
                }
            }
            GovernableParam::BaseFeeFloor => {
                if let ParameterValue::U64(v) = update.value {
                    self.base_fee_floor = v;
                }
            }
            GovernableParam::BaseFeeCeiling => {
                if let ParameterValue::U64(v) = update.value {
                    self.base_fee_ceiling = v;
                }
            }
        }

        Ok(())
    }
}

impl Default for ChainParams {
    fn default() -> Self {
        Self {
            pegin_compensation: PegInCompensation::default(),
            btc_confirmations: 6,
            min_peg_amount: 10_000,
            max_peg_amount: 100_000_000,
            federation_threshold: 11,
            propose_timeout_ms: 3000,
            prevote_timeout_ms: 1000,
            precommit_timeout_ms: 1000,
            timeout_delta_ms: 500,
            max_validators: 15,
            min_checkpoint_interval: 100,
            target_checkpoint_interval: 500,
            max_blocks_without_pow: 50_000,
            base_fee_floor: 1,
            base_fee_ceiling: 1000,
            chain_paused: false,
            pegins_paused: false,
            pegouts_paused: false,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParameterError {
    #[error("Parameter {param:?} value {value} out of range: {constraint}")]
    OutOfRange {
        param: GovernableParam,
        value: String,
        constraint: String,
    },

    #[error("Invalid parameter value type for {param:?}")]
    InvalidType { param: GovernableParam },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governable_param_category() {
        assert_eq!(GovernableParam::MinerFeeBps.category(), "peg-in-compensation");
        assert_eq!(
            GovernableParam::BtcConfirmations.category(),
            "bridge-config"
        );
        assert_eq!(
            GovernableParam::ProposeTimeoutMs.category(),
            "consensus-params"
        );
    }

    #[test]
    fn test_governable_param_roundtrip() {
        let param = GovernableParam::MaxValidators;
        let bytes = param.to_bytes();
        let recovered = GovernableParam::from_bytes(bytes);
        assert_eq!(recovered, Some(param));
    }

    #[test]
    fn test_parameter_validation() {
        // Valid miner fee
        let valid = ParameterUpdate {
            param: GovernableParam::MinerFeeBps,
            value: ParameterValue::U64(50),
            governance_signature: Signature::empty(),
        };
        assert!(valid.validate().is_ok());

        // Invalid miner fee (> 100%)
        let invalid = ParameterUpdate {
            param: GovernableParam::MinerFeeBps,
            value: ParameterValue::U64(15000),
            governance_signature: Signature::empty(),
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_chain_params_hash_changes() {
        let params1 = ChainParams::default();
        let mut params2 = ChainParams::default();
        params2.btc_confirmations = 10;

        assert_ne!(params1.compute_hash(), params2.compute_hash());
    }
}
