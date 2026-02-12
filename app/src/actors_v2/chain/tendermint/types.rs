//! Core types for Tendermint consensus.
//!
//! This module defines the fundamental types used throughout the Tendermint
//! implementation. These types are designed to integrate with the existing
//! Alys V2 actor system.

use ethereum_types::H256;
use lighthouse_wrapper::bls::{PublicKey, Signature as BLSSignature};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Block hash type - consistent with existing Alys conventions
pub type BlockHash = H256;

/// Round identifier within a height
///
/// Rounds start at 0 and increment when:
/// - Proposer times out
/// - 2/3+ prevote NIL
/// - 2/3+ precommit NIL
pub type Round = u32;

/// Height identifier (block number)
pub type Height = u64;

/// Voting power for a validator
///
/// For Alys with equal voting power, this is typically 1 per validator.
/// The type allows for future weighted voting if needed.
pub type VotingPower = u64;

/// Validator identifier - index into the validator set (0-14 for 15 validators)
///
/// # Design Decision
/// We use u8 instead of PublicKey for efficiency in vote tracking.
/// The ValidatorSet maintains the mapping from index to public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ValidatorId(pub u8);

impl ValidatorId {
    /// Create a new validator ID
    pub fn new(index: u8) -> Self {
        Self(index)
    }

    /// Get the raw index value
    pub fn index(&self) -> u8 {
        self.0
    }
}

impl fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Validator({})", self.0)
    }
}

impl From<u8> for ValidatorId {
    fn from(index: u8) -> Self {
        Self(index)
    }
}

/// Tendermint consensus step within a round
///
/// ```text
/// ┌──────────┐     ┌──────────┐     ┌────────────┐     ┌────────┐
/// │ Propose  │ ──► │ Prevote  │ ──► │ Precommit  │ ──► │ Commit │
/// └──────────┘     └──────────┘     └────────────┘     └────────┘
///      │                │                 │                │
///      ▼                ▼                 ▼                ▼
///   timeout          timeout           timeout         done
///   ──────►          ──────►           ──────►
///  NewRound         NewRound          NewRound
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TendermintStep {
    /// Waiting for proposal from the designated proposer
    #[default]
    Propose = 0,

    /// Proposal received, voting prevote
    Prevote = 1,

    /// 2/3+ prevotes received, voting precommit
    Precommit = 2,

    /// 2/3+ precommits received, committing block
    Commit = 3,
}

impl TendermintStep {
    /// Get the next step in the sequence (does not wrap to Propose)
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Propose => Some(Self::Prevote),
            Self::Prevote => Some(Self::Precommit),
            Self::Precommit => Some(Self::Commit),
            Self::Commit => None, // Height complete
        }
    }

    /// Check if this step allows voting
    pub fn is_voting_step(&self) -> bool {
        matches!(self, Self::Prevote | Self::Precommit)
    }

    /// Convert to numeric value for metrics
    pub fn as_metric_value(&self) -> i64 {
        *self as i64
    }
}

impl fmt::Display for TendermintStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Propose => write!(f, "Propose"),
            Self::Prevote => write!(f, "Prevote"),
            Self::Precommit => write!(f, "Precommit"),
            Self::Commit => write!(f, "Commit"),
        }
    }
}

/// Vote type distinguishes between prevote and precommit phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoteType {
    /// First voting phase - signals support for a proposal
    Prevote = 0,

    /// Second voting phase - commits to the block
    Precommit = 1,
}

impl fmt::Display for VoteType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Prevote => write!(f, "Prevote"),
            Self::Precommit => write!(f, "Precommit"),
        }
    }
}

/// Commit proof - +2/3 precommit signatures proving block finality
///
/// This structure proves that a block was finalized by collecting
/// the precommit signatures from validators representing >2/3 of
/// the total voting power.
///
/// # Storage Location
///
/// Following the standard Tendermint/CometBFT pattern, this Commit is stored
/// in the **next** block's `last_commit` field:
///
/// ```text
/// Block N:
/// └── last_commit: Commit for Block N-1
///     ├── height: N-1
///     ├── round: R
///     ├── block_hash: hash(Block N-1)
///     └── signatures: [CommitSig, CommitSig, ...]
/// ```
///
/// This means `LoadBlockCommit(height)` returns `Block[height+1].last_commit`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commit {
    /// Height of the committed block (this commit is FOR this height)
    pub height: Height,

    /// Round in which the block was committed
    pub round: Round,

    /// Hash of the committed block (BlockID in Tendermint terms)
    pub block_hash: BlockHash,

    /// Individual commit signatures from validators.
    /// The array has one entry per validator in the validator set,
    /// in the same order as the validator set.
    pub signatures: Vec<CommitSig>,
}

impl Commit {
    /// Create a new commit from collected precommit signatures
    pub fn new(
        height: Height,
        round: Round,
        block_hash: BlockHash,
        signatures: Vec<CommitSig>,
    ) -> Self {
        Self {
            height,
            round,
            block_hash,
            signatures,
        }
    }

    /// Count the number of validators who committed to the block
    pub fn num_commit_signatures(&self) -> usize {
        self.signatures
            .iter()
            .filter(|sig| sig.block_id_flag == BlockIDFlag::Commit)
            .count()
    }

    /// Check if a specific validator committed to the block
    pub fn has_committed(&self, validator: ValidatorId) -> bool {
        self.signatures.iter().any(|sig| {
            sig.validator_address == Some(validator) && sig.block_id_flag == BlockIDFlag::Commit
        })
    }

    /// Verify the commit has sufficient signatures (>2/3)
    pub fn has_sufficient_signatures(&self, total_validators: usize) -> bool {
        let threshold = (total_validators * 2 / 3) + 1;
        self.num_commit_signatures() >= threshold
    }

    /// Get indices of validators who committed (for signature verification)
    pub fn get_committer_indices(&self) -> Vec<u8> {
        self.signatures
            .iter()
            .filter_map(|sig| {
                if sig.block_id_flag == BlockIDFlag::Commit {
                    sig.validator_address.map(|v| v.index())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all signatures for aggregate verification
    pub fn get_signatures_for_verification(&self) -> Vec<&BLSSignature> {
        self.signatures
            .iter()
            .filter_map(|sig| {
                if sig.block_id_flag == BlockIDFlag::Commit {
                    sig.signature.as_ref()
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Individual validator's commit signature.
///
/// Each CommitSig represents how one validator participated in the commit:
/// - Absent: Validator did not submit a precommit
/// - Commit: Validator precommitted to the block
/// - Nil: Validator precommitted nil (voted to skip)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitSig {
    /// How this validator participated in the commit
    pub block_id_flag: BlockIDFlag,

    /// Validator's ID/index (None if absent)
    pub validator_address: Option<ValidatorId>,

    /// Timestamp of the vote (Unix timestamp)
    pub timestamp: u64,

    /// BLS signature over the vote (None if absent)
    pub signature: Option<BLSSignature>,
}

impl CommitSig {
    /// Create an absent commit signature (validator didn't vote)
    pub fn absent() -> Self {
        Self {
            block_id_flag: BlockIDFlag::Absent,
            validator_address: None,
            timestamp: 0,
            signature: None,
        }
    }

    /// Create a commit signature for a validator who voted for the block
    pub fn commit(validator: ValidatorId, timestamp: u64, signature: BLSSignature) -> Self {
        Self {
            block_id_flag: BlockIDFlag::Commit,
            validator_address: Some(validator),
            timestamp,
            signature: Some(signature),
        }
    }

    /// Create a nil commit signature (validator voted nil)
    pub fn nil(validator: ValidatorId, timestamp: u64, signature: BLSSignature) -> Self {
        Self {
            block_id_flag: BlockIDFlag::Nil,
            validator_address: Some(validator),
            timestamp,
            signature: Some(signature),
        }
    }
}

/// Indicates how a validator participated in the commit
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BlockIDFlag {
    /// Validator was absent (did not submit a precommit)
    #[default]
    Absent = 0,
    /// Validator precommitted to the block
    Commit = 1,
    /// Validator precommitted nil
    Nil = 2,
}

/// Validator set configuration
///
/// Integrates with the existing Aura authorities while adding
/// voting power semantics for Tendermint.
#[derive(Debug, Clone)]
pub struct ValidatorSet {
    /// Validator public keys (indexed by ValidatorId)
    validators: Vec<PublicKey>,

    /// Voting power for each validator (same order as validators)
    powers: Vec<VotingPower>,

    /// Total voting power (cached for efficiency)
    total_power: VotingPower,
}

impl ValidatorSet {
    /// Create a new validator set from public keys with equal voting power
    ///
    /// This is the standard initialization matching the current Aura setup.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let authorities = aura.authorities.clone();
    /// let validator_set = ValidatorSet::with_equal_power(authorities);
    /// ```
    pub fn with_equal_power(validators: Vec<PublicKey>) -> Self {
        let count = validators.len();
        let powers = vec![1; count]; // Each validator has power 1
        let total_power = count as VotingPower;

        Self {
            validators,
            powers,
            total_power,
        }
    }

    /// Create a validator set with specified voting powers
    pub fn with_powers(validators: Vec<PublicKey>, powers: Vec<VotingPower>) -> Self {
        assert_eq!(
            validators.len(),
            powers.len(),
            "validators and powers must have same length"
        );
        let total_power = powers.iter().sum();

        Self {
            validators,
            powers,
            total_power,
        }
    }

    /// Get the total voting power
    pub fn total_power(&self) -> VotingPower {
        self.total_power
    }

    /// Get the voting power of a specific validator
    pub fn get_power(&self, validator: &ValidatorId) -> Result<VotingPower, ValidatorError> {
        self.powers
            .get(validator.index() as usize)
            .copied()
            .ok_or(ValidatorError::UnknownValidator(*validator))
    }

    /// Get the public key of a validator
    pub fn get_public_key(&self, validator: &ValidatorId) -> Result<&PublicKey, ValidatorError> {
        self.validators
            .get(validator.index() as usize)
            .ok_or(ValidatorError::UnknownValidator(*validator))
    }

    /// Get the number of validators
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Calculate the proposer for a given height and round
    ///
    /// Uses round-robin selection matching Aura's slot_author logic:
    /// `proposer = (height + round) % num_validators`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let validator_set = ValidatorSet::with_equal_power(authorities);
    /// let proposer = validator_set.get_proposer(100, 0);
    /// // With 15 validators: proposer = ValidatorId(100 % 15) = ValidatorId(10)
    /// ```
    pub fn get_proposer(&self, height: Height, round: Round) -> ValidatorId {
        let index = ((height as usize) + (round as usize)) % self.validators.len();
        ValidatorId::new(index as u8)
    }

    /// Calculate the threshold for 2/3+ majority
    ///
    /// For safety, we need strictly greater than 2/3:
    /// `threshold = (total_power * 2 / 3) + 1`
    pub fn two_thirds_threshold(&self) -> VotingPower {
        (self.total_power * 2 / 3) + 1
    }

    /// Check if a given power represents 2/3+ of total
    pub fn has_two_thirds(&self, power: VotingPower) -> bool {
        power >= self.two_thirds_threshold()
    }

    /// Iterator over all validators
    pub fn iter(&self) -> impl Iterator<Item = (ValidatorId, &PublicKey, VotingPower)> {
        self.validators
            .iter()
            .zip(self.powers.iter())
            .enumerate()
            .map(|(i, (pk, &power))| (ValidatorId::new(i as u8), pk, power))
    }

    /// Find validator ID by public key
    pub fn find_validator(&self, pubkey: &PublicKey) -> Option<ValidatorId> {
        self.validators
            .iter()
            .position(|pk| pk == pubkey)
            .map(|i| ValidatorId::new(i as u8))
    }

    /// Add or update a validator's voting power
    ///
    /// Used by governance to apply ValidatorUpdate changes.
    /// If the validator exists, updates their power.
    /// If new, appends to the validator set.
    pub fn upsert(&mut self, public_key: PublicKey, power: VotingPower) {
        if let Some(idx) = self.find_validator(&public_key) {
            // Update existing validator
            self.powers[idx.index() as usize] = power;
        } else {
            // Add new validator
            self.validators.push(public_key);
            self.powers.push(power);
        }
        self.recalculate_total_power();
    }

    /// Remove a validator from the set
    ///
    /// Used by governance when a validator's power is set to 0.
    /// Returns true if the validator was found and removed.
    ///
    /// # Warning
    ///
    /// Removing validators changes indices! This should only be called
    /// at height boundaries where a fresh ValidatorId mapping is established.
    pub fn remove(&mut self, public_key: &PublicKey) -> bool {
        if let Some(idx) = self.find_validator(public_key) {
            let index = idx.index() as usize;
            self.validators.remove(index);
            self.powers.remove(index);
            self.recalculate_total_power();
            true
        } else {
            false
        }
    }

    /// Recalculate total voting power after modifications
    fn recalculate_total_power(&mut self) {
        self.total_power = self.powers.iter().sum();
    }

    /// Apply a batch of validator updates atomically
    ///
    /// Updates are applied in order. Power of 0 means removal.
    /// This is the preferred method for governance changes.
    pub fn apply_updates(&mut self, updates: &[super::governance::ValidatorUpdate]) {
        for update in updates {
            if update.power == 0 {
                self.remove(&update.public_key);
            } else {
                self.upsert(update.public_key.clone(), update.power);
            }
        }
    }
}

/// Errors related to validator operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidatorError {
    #[error("Unknown validator: {0}")]
    UnknownValidator(ValidatorId),

    #[error("Invalid validator index: {0}")]
    InvalidIndex(u8),

    #[error("Validator set is empty")]
    EmptyValidatorSet,
}

/// Serializable version of ValidatorSet for storage.
///
/// Since lighthouse PublicKey doesn't implement Serialize/Deserialize,
/// this wrapper stores public keys as hex strings for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableValidatorSet {
    /// Validator public keys as hex-encoded strings
    pub validators: Vec<String>,
    /// Voting power for each validator
    pub powers: Vec<VotingPower>,
}

impl SerializableValidatorSet {
    /// Convert from ValidatorSet to SerializableValidatorSet
    pub fn from_validator_set(vs: &ValidatorSet) -> Self {
        Self {
            validators: vs
                .validators
                .iter()
                .map(|pk| format!("{:?}", pk)) // PublicKey Debug format is the hex representation
                .collect(),
            powers: vs.powers.clone(),
        }
    }

    /// Convert to ValidatorSet
    ///
    /// # Errors
    ///
    /// Returns an error if any public key fails to parse.
    pub fn to_validator_set(&self) -> Result<ValidatorSet, ValidatorError> {
        let mut validators = Vec::with_capacity(self.validators.len());

        for pk_str in &self.validators {
            // Strip the "0x" prefix if present and parse
            let hex_str = pk_str.strip_prefix("0x").unwrap_or(pk_str);
            let public_key = PublicKey::from_str(pk_str)
                .or_else(|_| PublicKey::from_str(&format!("0x{}", hex_str)))
                .map_err(|_| ValidatorError::EmptyValidatorSet)?; // Reuse error type
            validators.push(public_key);
        }

        Ok(ValidatorSet {
            validators,
            powers: self.powers.clone(),
            total_power: self.powers.iter().sum(),
        })
    }
}

impl ValidatorSet {
    /// Convert to serializable format for storage
    pub fn to_serializable(&self) -> SerializableValidatorSet {
        SerializableValidatorSet::from_validator_set(self)
    }

    /// Create from serializable format
    pub fn from_serializable(s: &SerializableValidatorSet) -> Result<Self, ValidatorError> {
        s.to_validator_set()
    }
}

// Implement Serialize for ValidatorSet via SerializableValidatorSet
impl Serialize for ValidatorSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_serializable().serialize(serializer)
    }
}

// Implement Deserialize for ValidatorSet via SerializableValidatorSet
impl<'de> Deserialize<'de> for ValidatorSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let serializable = SerializableValidatorSet::deserialize(deserializer)?;
        serializable
            .to_validator_set()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn create_mock_pubkey() -> PublicKey {
        // Use a known valid BLS public key for testing
        // This is a generator point public key commonly used in tests
        PublicKey::from_str(
            "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
        ).expect("valid test public key")
    }

    #[test]
    fn test_tendermint_step_progression() {
        assert_eq!(TendermintStep::Propose.next(), Some(TendermintStep::Prevote));
        assert_eq!(
            TendermintStep::Prevote.next(),
            Some(TendermintStep::Precommit)
        );
        assert_eq!(
            TendermintStep::Precommit.next(),
            Some(TendermintStep::Commit)
        );
        assert_eq!(TendermintStep::Commit.next(), None);
    }

    #[test]
    fn test_validator_id() {
        let id = ValidatorId::new(5);
        assert_eq!(id.index(), 5);
        assert_eq!(format!("{}", id), "Validator(5)");
    }

    #[test]
    fn test_two_thirds_threshold() {
        // 15 validators with power 1 each = total 15
        // 2/3 of 15 = 10, threshold = 11
        let validators = vec![create_mock_pubkey(); 15];
        let set = ValidatorSet::with_equal_power(validators);
        assert_eq!(set.total_power(), 15);
        assert_eq!(set.two_thirds_threshold(), 11);

        // Check threshold behavior
        assert!(!set.has_two_thirds(10));
        assert!(set.has_two_thirds(11));
        assert!(set.has_two_thirds(15));
    }

    #[test]
    fn test_proposer_selection() {
        let validators = vec![create_mock_pubkey(); 15];
        let set = ValidatorSet::with_equal_power(validators);

        // Height 0, Round 0 -> Validator 0
        assert_eq!(set.get_proposer(0, 0), ValidatorId(0));

        // Height 100, Round 0 -> Validator 10 (100 % 15)
        assert_eq!(set.get_proposer(100, 0), ValidatorId(10));

        // Height 100, Round 1 -> Validator 11 ((100 + 1) % 15)
        assert_eq!(set.get_proposer(100, 1), ValidatorId(11));

        // Round increment wraps around
        assert_eq!(set.get_proposer(14, 1), ValidatorId(0)); // (14 + 1) % 15 = 0
    }

    #[test]
    fn test_commit_sig_creation() {
        let absent = CommitSig::absent();
        assert_eq!(absent.block_id_flag, BlockIDFlag::Absent);
        assert!(absent.validator_address.is_none());
        assert!(absent.signature.is_none());
    }

    #[test]
    fn test_commit_sufficient_signatures() {
        let commit = Commit {
            height: 100,
            round: 0,
            block_hash: BlockHash::zero(),
            signatures: vec![
                CommitSig {
                    block_id_flag: BlockIDFlag::Commit,
                    validator_address: Some(ValidatorId(0)),
                    timestamp: 0,
                    signature: None,
                },
                CommitSig {
                    block_id_flag: BlockIDFlag::Commit,
                    validator_address: Some(ValidatorId(1)),
                    timestamp: 0,
                    signature: None,
                },
                CommitSig::absent(),
                CommitSig::absent(),
            ],
        };

        // 2 out of 4 is not sufficient (need 3)
        assert!(!commit.has_sufficient_signatures(4));
        // 2 out of 3 is not sufficient (need 3: floor(3*2/3)+1 = 3)
        assert!(!commit.has_sufficient_signatures(3));
        // 2 out of 2 is sufficient (need 2: floor(2*2/3)+1 = 2)
        assert!(commit.has_sufficient_signatures(2));
    }
}
