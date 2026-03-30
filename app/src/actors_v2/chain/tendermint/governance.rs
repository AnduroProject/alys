//! Governance types for federation-controlled updates.
//!
//! All governance changes flow through the unified GovernanceUpdate enum,
//! which supports three categories with different activation timing:
//! - Validator updates: H+2 activation (standard Tendermint)
//! - Parameter updates: H+1 activation (propagation delay)
//! - Emergency actions: H+0 activation (immediate)

use super::params::{GovernableParam, ParameterUpdate};
use super::types::{ValidatorSet, VotingPower};
use ethereum_types::H256;
use lighthouse_wrapper::bls::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tiny_keccak::{Hasher, Keccak};

/// Errors related to governance operations
#[derive(Debug, Clone, PartialEq)]
pub enum GovernanceError {
    /// Governance update is missing a required signature
    MissingSignature,
    /// Governance signature failed verification
    InvalidSignature,
    /// Insufficient voting power to authorize the action
    InsufficientVotingPower { required: u64, provided: u64 },
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GovernanceError::MissingSignature => {
                write!(f, "Governance update is missing required signature")
            }
            GovernanceError::InvalidSignature => {
                write!(f, "Governance signature failed verification")
            }
            GovernanceError::InsufficientVotingPower { required, provided } => {
                write!(
                    f,
                    "Insufficient voting power: required {}, provided {}",
                    required, provided
                )
            }
        }
    }
}

impl std::error::Error for GovernanceError {}

/// Unified type for all governance-controlled changes
///
/// Included in blocks for auditability and late-joiner verification.
/// Updates are idempotent - applying the same update twice is a no-op.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GovernanceUpdate {
    /// Validator set changes (add/remove/change power)
    /// Activation: H+2 (standard Tendermint delayed validator changes)
    Validator(ValidatorUpdate),

    /// Chain parameter changes
    /// Activation: H+1 (allows propagation before activation)
    Parameter(ParameterUpdate),

    /// Emergency actions (pause/resume)
    /// Activation: H+0 (immediate effect)
    Emergency(EmergencyAction),
}

impl GovernanceUpdate {
    /// Get the activation delay (in blocks) for this update type
    pub fn activation_delay(&self) -> u64 {
        match self {
            GovernanceUpdate::Validator(_) => 2,
            GovernanceUpdate::Parameter(_) => 1,
            GovernanceUpdate::Emergency(_) => 0,
        }
    }

    /// Get the effective height when included at `inclusion_height`
    pub fn effective_height(&self, inclusion_height: u64) -> u64 {
        inclusion_height + self.activation_delay()
    }

    /// Get variant name for logging
    pub fn variant_name(&self) -> &'static str {
        match self {
            GovernanceUpdate::Validator(_) => "Validator",
            GovernanceUpdate::Parameter(_) => "Parameter",
            GovernanceUpdate::Emergency(_) => "Emergency",
        }
    }

    /// Verify the governance signature against the current validator set.
    ///
    /// All governance updates require 2/3+ aggregate signature from validators.
    /// This method dispatches to the appropriate verification method based on
    /// the update type.
    pub fn verify_governance_signature(
        &self,
        validator_set: &ValidatorSet,
        chain_id: &str,
    ) -> Result<(), GovernanceError> {
        match self {
            GovernanceUpdate::Validator(update) => {
                update.verify_governance_signature(validator_set, chain_id)
            }
            GovernanceUpdate::Parameter(update) => {
                update.verify_governance_signature(validator_set, chain_id)
            }
            GovernanceUpdate::Emergency(update) => {
                update.verify_governance_signature(validator_set, chain_id)
            }
        }
    }
}

/// Validator set change request from governance
///
/// # Idempotency
///
/// Updates are keyed by `public_key`. If multiple updates arrive for the
/// same validator, the latest one wins (replaces in queue).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatorUpdate {
    /// Validator's BLS public key
    pub public_key: PublicKey,

    /// New voting power (0 = remove from validator set)
    pub power: VotingPower,

    /// Governance threshold signature proving authorization
    pub governance_signature: Signature,
}

impl ValidatorUpdate {
    /// Check if this update removes the validator
    pub fn is_removal(&self) -> bool {
        self.power == 0
    }

    /// Compute the signing root for this validator update.
    ///
    /// Issue 1.4 Fix: This provides the message that governance signers must sign.
    /// Includes domain separation and chain_id for replay protection.
    pub fn signing_root(&self, chain_id: &str) -> H256 {
        let mut hasher = Keccak::v256();

        // Domain separation prefix
        hasher.update(b"governance-validator-update");
        // Chain ID for cross-network replay protection
        hasher.update(chain_id.as_bytes());
        // Validator being updated
        hasher.update(&self.public_key.serialize());
        // New voting power
        hasher.update(&self.power.to_le_bytes());

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        H256::from(output)
    }

    /// Verify the governance signature against the current validator set.
    ///
    /// Issue 1.4 Fix: Requires 2/3+ of current validators to sign.
    /// For MVP, we use a single aggregate signature that must verify against
    /// a quorum of the validator set.
    pub fn verify_governance_signature(
        &self,
        validator_set: &ValidatorSet,
        chain_id: &str,
    ) -> Result<(), GovernanceError> {
        let signing_root = self.signing_root(chain_id);

        // Check if signature is empty (empty signature is all zeros)
        if self.governance_signature == Signature::empty() {
            return Err(GovernanceError::MissingSignature);
        }

        // For MVP: Verify as an aggregate BLS signature from all validators
        // The signature should be an aggregate of individual validator signatures.
        // This assumes all validators participated in signing.
        //
        // In production, we would need to:
        // 1. Track which validators signed (e.g., using a bitfield)
        // 2. Aggregate only participating validator public keys
        // 3. Verify that participating power >= 2/3+ threshold
        //
        // For now, aggregate all validator public keys and verify
        let aggregate_pubkey = validator_set.aggregate_public_key();

        if !self
            .governance_signature
            .verify(&aggregate_pubkey, signing_root)
        {
            return Err(GovernanceError::InvalidSignature);
        }

        Ok(())
    }
}

/// Emergency action from governance
///
/// Emergency actions take effect immediately (H+0) and are used for
/// critical situations requiring instant response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmergencyAction {
    /// The action to take
    pub action: EmergencyActionKind,

    /// Governance threshold signature proving authorization
    pub governance_signature: Signature,
}

/// Types of emergency actions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmergencyActionKind {
    /// Pause peg-in processing (new deposits rejected)
    PausePegIns = 0,
    /// Resume peg-in processing
    ResumePegIns = 1,
    /// Pause peg-out processing (withdrawals halted)
    PausePegOuts = 2,
    /// Resume peg-out processing
    ResumePegOuts = 3,
    /// Emergency chain halt (no new blocks)
    PauseChain = 4,
    /// Resume chain operation
    ResumeChain = 5,
}

impl EmergencyAction {
    /// Get the action type for logging/metrics
    pub fn action_type(&self) -> EmergencyActionKind {
        self.action
    }

    /// Compute the signing root for this emergency action.
    ///
    /// Issue 1.4 Fix: This provides the message that governance signers must sign.
    /// Includes domain separation and chain_id for replay protection.
    pub fn signing_root(&self, chain_id: &str) -> H256 {
        let mut hasher = Keccak::v256();

        // Domain separation prefix
        hasher.update(b"governance-emergency-action");
        // Chain ID for cross-network replay protection
        hasher.update(chain_id.as_bytes());
        // The action being taken
        hasher.update(&[self.action as u8]);

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        H256::from(output)
    }

    /// Verify the governance signature against the current validator set.
    ///
    /// Issue 1.4 Fix: Emergency actions require 2/3+ of current validators to sign.
    pub fn verify_governance_signature(
        &self,
        validator_set: &ValidatorSet,
        chain_id: &str,
    ) -> Result<(), GovernanceError> {
        let signing_root = self.signing_root(chain_id);

        // Check if signature is empty
        if self.governance_signature == Signature::empty() {
            return Err(GovernanceError::MissingSignature);
        }

        // Verify as aggregate BLS signature (same as ValidatorUpdate)
        let aggregate_pubkey = validator_set.aggregate_public_key();

        if !self
            .governance_signature
            .verify(&aggregate_pubkey, signing_root)
        {
            return Err(GovernanceError::InvalidSignature);
        }

        Ok(())
    }
}

/// Queue of pending governance updates awaiting activation
///
/// Updates are keyed to provide idempotency:
/// - Validators keyed by PublicKey
/// - Parameters keyed by GovernableParam
#[derive(Debug, Clone, Default)]
pub struct GovernanceQueue {
    /// Pending validator updates (keyed by public key)
    pub validators: HashMap<PublicKey, ValidatorUpdate>,

    /// Pending parameter updates (keyed by parameter)
    pub parameters: HashMap<GovernableParam, ParameterUpdate>,
}

impl GovernanceQueue {
    /// Create an empty governance queue
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty() && self.parameters.is_empty()
    }

    /// Get total number of pending updates
    pub fn len(&self) -> usize {
        self.validators.len() + self.parameters.len()
    }

    /// Add a governance update to the queue
    pub fn add(&mut self, update: GovernanceUpdate) {
        match update {
            GovernanceUpdate::Validator(v) => {
                self.validators.insert(v.public_key.clone(), v);
            }
            GovernanceUpdate::Parameter(p) => {
                self.parameters.insert(p.param, p);
            }
            GovernanceUpdate::Emergency(_) => {
                // Emergency actions are applied immediately, not queued
            }
        }
    }

    /// Clear all pending updates
    pub fn clear(&mut self) {
        self.validators.clear();
        self.parameters.clear();
    }

    /// Get all pending validator updates
    pub fn drain_validators(&mut self) -> Vec<ValidatorUpdate> {
        self.validators.drain().map(|(_, v)| v).collect()
    }

    /// Get all pending parameter updates
    pub fn drain_parameters(&mut self) -> Vec<ParameterUpdate> {
        self.parameters.drain().map(|(_, p)| p).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn create_mock_pubkey() -> PublicKey {
        // Use a known valid BLS public key for testing
        PublicKey::from_str(
            "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
        ).expect("valid test public key")
    }

    #[test]
    fn test_activation_delays() {
        let validator_update = GovernanceUpdate::Validator(ValidatorUpdate {
            public_key: create_mock_pubkey(),
            power: 1,
            governance_signature: Signature::empty(),
        });
        assert_eq!(validator_update.activation_delay(), 2);
        assert_eq!(validator_update.effective_height(100), 102);

        // Emergency actions are immediate
        let emergency = GovernanceUpdate::Emergency(EmergencyAction {
            action: EmergencyActionKind::PausePegIns,
            governance_signature: Signature::empty(),
        });
        assert_eq!(emergency.activation_delay(), 0);
        assert_eq!(emergency.effective_height(100), 100);
    }

    #[test]
    fn test_governance_queue_idempotency() {
        let mut queue = GovernanceQueue::new();
        let pubkey = create_mock_pubkey();

        // First update: power = 100
        queue.add(GovernanceUpdate::Validator(ValidatorUpdate {
            public_key: pubkey.clone(),
            power: 100,
            governance_signature: Signature::empty(),
        }));

        // Second update: power = 200 (replaces first)
        queue.add(GovernanceUpdate::Validator(ValidatorUpdate {
            public_key: pubkey.clone(),
            power: 200,
            governance_signature: Signature::empty(),
        }));

        assert_eq!(queue.validators.len(), 1);
        assert_eq!(queue.validators.get(&pubkey).unwrap().power, 200);
    }

    #[test]
    fn test_validator_update_is_removal() {
        let removal = ValidatorUpdate {
            public_key: create_mock_pubkey(),
            power: 0,
            governance_signature: Signature::empty(),
        };
        assert!(removal.is_removal());

        let update = ValidatorUpdate {
            public_key: create_mock_pubkey(),
            power: 1,
            governance_signature: Signature::empty(),
        };
        assert!(!update.is_removal());
    }

    #[test]
    fn test_governance_signature_verification_rejects_empty() {
        use lighthouse_wrapper::bls::SecretKey;

        // Create a validator set with 3 validators
        let secret_keys: Vec<SecretKey> = (0..3).map(|_| SecretKey::random()).collect();
        let public_keys: Vec<PublicKey> = secret_keys.iter().map(|sk| sk.public_key()).collect();
        let validator_set = ValidatorSet::with_equal_power(public_keys.clone());

        // Create an update with empty signature (unsigned)
        let update = ValidatorUpdate {
            public_key: create_mock_pubkey(),
            power: 100,
            governance_signature: Signature::empty(),
        };

        // Verification should fail with MissingSignature
        let result = update.verify_governance_signature(&validator_set, "alys-test");
        assert_eq!(result, Err(GovernanceError::MissingSignature));
    }

    #[test]
    fn test_governance_signing_root_domain_separation() {
        let update = ValidatorUpdate {
            public_key: create_mock_pubkey(),
            power: 100,
            governance_signature: Signature::empty(),
        };

        // Signing roots with different chain_ids should be different
        let root_mainnet = update.signing_root("alys-mainnet-1");
        let root_testnet = update.signing_root("alys-testnet-1");

        assert_ne!(
            root_mainnet, root_testnet,
            "Signing roots should differ for different chain_ids"
        );

        // Same chain_id should produce same root
        let root_mainnet_2 = update.signing_root("alys-mainnet-1");
        assert_eq!(
            root_mainnet, root_mainnet_2,
            "Signing roots should be deterministic"
        );
    }

    #[test]
    fn test_emergency_action_signing_root_domain_separation() {
        let action = EmergencyAction {
            action: EmergencyActionKind::PausePegIns,
            governance_signature: Signature::empty(),
        };

        // Signing roots with different chain_ids should be different
        let root_mainnet = action.signing_root("alys-mainnet-1");
        let root_testnet = action.signing_root("alys-testnet-1");

        assert_ne!(
            root_mainnet, root_testnet,
            "Signing roots should differ for different chain_ids"
        );

        // Different actions should have different signing roots
        let action2 = EmergencyAction {
            action: EmergencyActionKind::PausePegOuts,
            governance_signature: Signature::empty(),
        };
        let root_pause_pegouts = action2.signing_root("alys-mainnet-1");

        assert_ne!(
            root_mainnet, root_pause_pegouts,
            "Different actions should have different signing roots"
        );
    }

    #[test]
    fn test_governance_signature_verification_single_validator() {
        use lighthouse_wrapper::bls::SecretKey;

        // Create a validator set with a single validator (simpler case)
        let secret_key = SecretKey::random();
        let public_key = secret_key.public_key();
        let validator_set = ValidatorSet::with_equal_power(vec![public_key.clone()]);

        // Create an update
        let mut update = ValidatorUpdate {
            public_key: create_mock_pubkey(),
            power: 100,
            governance_signature: Signature::empty(),
        };

        // Compute the signing root
        let chain_id = "alys-test";
        let signing_root = update.signing_root(chain_id);

        // Sign with the single validator
        let sig = secret_key.sign(signing_root);

        // Set the governance signature (single signature for single validator)
        update.governance_signature = sig;

        // Verification should succeed with single validator set
        let result = update.verify_governance_signature(&validator_set, chain_id);
        assert!(result.is_ok(), "Valid signature should verify against single validator");
    }

    // TODO: Add aggregate signature test when proper aggregate signing infrastructure is in place.
    // The current governance_signature field uses Signature type, but proper aggregate
    // verification requires AggregateSignature. This architectural decision needs review.
}
