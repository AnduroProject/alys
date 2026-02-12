//! Governance types for federation-controlled updates.
//!
//! All governance changes flow through the unified GovernanceUpdate enum,
//! which supports three categories with different activation timing:
//! - Validator updates: H+2 activation (standard Tendermint)
//! - Parameter updates: H+1 activation (propagation delay)
//! - Emergency actions: H+0 activation (immediate)

use super::params::{GovernableParam, ParameterUpdate};
use super::types::VotingPower;
use lighthouse_wrapper::bls::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}
