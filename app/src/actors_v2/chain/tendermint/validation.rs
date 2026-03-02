//! Tendermint consensus validation module.
//!
//! Provides signature verification for proposals, votes, and commits
//! in the Tendermint two-phase BFT consensus protocol.
//!
//! # Validation Functions
//!
//! - [`verify_proposal`]: Validate a proposal from the designated proposer
//! - [`verify_vote`]: Validate a vote signature and membership
//! - [`verify_commit`]: Validate 2/3+ precommit signatures for finality
//! - [`check_for_equivocation`]: Detect double-voting evidence
//!
//! # Proposer Selection
//!
//! Proposer for a given (height, round) is determined by:
//! ```text
//! proposer_index = (height + round) % validator_count
//! ```

use super::messages::{EquivocationEvidence, EquivocationType, Proposal, Vote};
use super::types::{BlockHash, Commit, BlockIDFlag, Height, Round, ValidatorId, ValidatorSet, VoteType};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during Tendermint consensus validation.
#[derive(Debug, Clone, Error)]
pub enum TendermintValidationError {
    /// Height does not match expected value
    #[error("Invalid height: expected {expected}, got {actual}")]
    InvalidHeight { expected: Height, actual: Height },

    /// Round does not match expected value
    #[error("Invalid round: expected {expected}, got {actual}")]
    InvalidRound { expected: Round, actual: Round },

    /// Wrong proposer for the given height and round
    #[error("Wrong proposer for height {height} round {round}: expected {expected}, got {actual}")]
    WrongProposer {
        expected: ValidatorId,
        actual: ValidatorId,
        height: Height,
        round: Round,
    },

    /// Validator is not in the validator set
    #[error("Unknown validator: {0}")]
    UnknownValidator(ValidatorId),

    /// Signature verification failed
    #[error("Invalid signature from {validator} on {message_type}")]
    InvalidSignature {
        validator: ValidatorId,
        message_type: String,
    },

    /// Commit block hash doesn't match expected
    #[error("Commit block mismatch: expected {expected}, got {actual}")]
    CommitBlockMismatch { expected: BlockHash, actual: BlockHash },

    /// Not enough voting power in commit
    #[error("Insufficient commit power: {committed}/{total} (need {threshold})")]
    InsufficientCommitPower {
        committed: u64,
        threshold: u64,
        total: u64,
    },

    /// Missing last_commit for non-genesis block
    #[error("Missing LastCommit for non-genesis block at height {0}")]
    MissingLastCommit(Height),

    /// LastCommit height doesn't match expected
    #[error("LastCommit height mismatch: expected {expected}, got {actual}")]
    CommitHeightMismatch { expected: Height, actual: Height },

    /// LastCommit block_hash doesn't match parent_hash
    #[error("LastCommit block_hash doesn't match parent_hash at height {0}")]
    CommitHashMismatch(Height),

    /// Validator set is empty
    #[error("Empty validator set")]
    EmptyValidatorSet,

    /// Public key not found for validator
    #[error("Public key not found for validator {0}")]
    PublicKeyNotFound(ValidatorId),
}

/// Validate a signed proposal from the designated proposer.
///
/// # Checks
///
/// 1. Height matches expected value
/// 2. Round matches expected value
/// 3. Proposer is correct for (height, round) using round-robin selection
/// 4. Proposer's BLS signature is valid
///
/// # Arguments
///
/// * `proposal` - The proposal to validate
/// * `validator_set` - The current validator set
/// * `expected_height` - The expected height for this proposal
/// * `expected_round` - The expected round for this proposal
/// * `chain_id` - Chain identifier for domain separation (Issue 1.2)
///
/// # Returns
///
/// Ok(()) if the proposal is valid, Err with details if invalid.
pub fn verify_proposal(
    proposal: &Proposal,
    validator_set: &ValidatorSet,
    expected_height: Height,
    expected_round: Round,
    chain_id: &str,
) -> Result<(), TendermintValidationError> {
    // Check height
    if proposal.height != expected_height {
        return Err(TendermintValidationError::InvalidHeight {
            expected: expected_height,
            actual: proposal.height,
        });
    }

    // Check round
    if proposal.round != expected_round {
        return Err(TendermintValidationError::InvalidRound {
            expected: expected_round,
            actual: proposal.round,
        });
    }

    // Check validator set is not empty
    if validator_set.is_empty() {
        return Err(TendermintValidationError::EmptyValidatorSet);
    }

    // Check proposer is correct for this height/round
    let expected_proposer = validator_set.get_proposer(proposal.height, proposal.round);
    if proposal.proposer != expected_proposer {
        return Err(TendermintValidationError::WrongProposer {
            expected: expected_proposer,
            actual: proposal.proposer,
            height: proposal.height,
            round: proposal.round,
        });
    }

    // Get public key for proposer
    let public_key = validator_set
        .get_public_key(&proposal.proposer)
        .map_err(|_| TendermintValidationError::PublicKeyNotFound(proposal.proposer))?;

    // Verify signature (Issue 1.2: pass chain_id for domain separation)
    if !proposal.verify_signature(public_key, chain_id) {
        return Err(TendermintValidationError::InvalidSignature {
            validator: proposal.proposer,
            message_type: "Proposal".to_string(),
        });
    }

    Ok(())
}

/// Validate a proposal from a future round for storage.
///
/// This is a relaxed validation that allows proposals for future rounds
/// to be stored and replayed when the node advances to that round.
/// Unlike `verify_proposal`, this does NOT require round to match current.
///
/// # Checks
///
/// 1. Height matches expected value (must be current height)
/// 2. Round is greater than current round (must be future)
/// 3. Proposer is correct for (height, proposal.round) using round-robin selection
/// 4. Proposer's BLS signature is valid
///
/// # Arguments
///
/// * `proposal` - The proposal to validate
/// * `validator_set` - The current validator set
/// * `expected_height` - The expected height (current height)
/// * `current_round` - The node's current round
/// * `chain_id` - Chain identifier for domain separation
///
/// # Returns
///
/// Ok(()) if the proposal is valid for storage, Err with details if invalid.
pub fn verify_future_proposal(
    proposal: &Proposal,
    validator_set: &ValidatorSet,
    expected_height: Height,
    current_round: Round,
    chain_id: &str,
) -> Result<(), TendermintValidationError> {
    // Check height - must match current height
    if proposal.height != expected_height {
        return Err(TendermintValidationError::InvalidHeight {
            expected: expected_height,
            actual: proposal.height,
        });
    }

    // Check round is in the future
    if proposal.round <= current_round {
        return Err(TendermintValidationError::InvalidRound {
            expected: current_round + 1, // Just indicate it should be > current
            actual: proposal.round,
        });
    }

    // Check validator set is not empty
    if validator_set.is_empty() {
        return Err(TendermintValidationError::EmptyValidatorSet);
    }

    // Check proposer is correct for this height and the PROPOSAL's round
    let expected_proposer = validator_set.get_proposer(proposal.height, proposal.round);
    if proposal.proposer != expected_proposer {
        return Err(TendermintValidationError::WrongProposer {
            expected: expected_proposer,
            actual: proposal.proposer,
            height: proposal.height,
            round: proposal.round,
        });
    }

    // Get public key for proposer
    let public_key = validator_set
        .get_public_key(&proposal.proposer)
        .map_err(|_| TendermintValidationError::PublicKeyNotFound(proposal.proposer))?;

    // Verify signature
    if !proposal.verify_signature(public_key, chain_id) {
        return Err(TendermintValidationError::InvalidSignature {
            validator: proposal.proposer,
            message_type: "Proposal".to_string(),
        });
    }

    Ok(())
}

/// Validate a single vote (prevote or precommit).
///
/// # Checks
///
/// 1. Height matches expected value
/// 2. Round matches expected value
/// 3. Voter is in the validator set
/// 4. Voter's BLS signature is valid over the vote signing root
///
/// # Arguments
///
/// * `vote` - The vote to validate
/// * `validator_set` - The current validator set
/// * `expected_height` - The expected height for this vote
/// * `expected_round` - The expected round for this vote
/// * `chain_id` - Chain identifier for domain separation (Issue 1.2)
///
/// # Returns
///
/// Ok(()) if the vote is valid, Err with details if invalid.
pub fn verify_vote(
    vote: &Vote,
    validator_set: &ValidatorSet,
    expected_height: Height,
    expected_round: Round,
    chain_id: &str,
) -> Result<(), TendermintValidationError> {
    // Check height
    if vote.height != expected_height {
        return Err(TendermintValidationError::InvalidHeight {
            expected: expected_height,
            actual: vote.height,
        });
    }

    // Check round
    if vote.round != expected_round {
        return Err(TendermintValidationError::InvalidRound {
            expected: expected_round,
            actual: vote.round,
        });
    }

    // Check voter is in validator set (validator index must be valid)
    if vote.validator.index() as usize >= validator_set.len() {
        return Err(TendermintValidationError::UnknownValidator(vote.validator));
    }

    // Get public key for voter
    let public_key = validator_set
        .get_public_key(&vote.validator)
        .map_err(|_| TendermintValidationError::PublicKeyNotFound(vote.validator))?;

    // Verify signature
    let message_type = match vote.vote_type {
        VoteType::Prevote => "Prevote",
        VoteType::Precommit => "Precommit",
    };

    // Issue 1.2: pass chain_id for domain separation
    if !vote.verify_signature(public_key, chain_id) {
        return Err(TendermintValidationError::InvalidSignature {
            validator: vote.validator,
            message_type: message_type.to_string(),
        });
    }

    Ok(())
}

/// Validate a commit (aggregated precommits) for block finalization.
///
/// # Checks
///
/// 1. Commit is for the claimed block hash
/// 2. Has 2/3+ voting power from valid precommit signatures
/// 3. Each precommit signature with BlockIDFlag::Commit is valid
///
/// # Arguments
///
/// * `commit` - The commit to validate
/// * `validator_set` - The validator set at the commit height
/// * `expected_block_hash` - The expected block hash being committed
/// * `chain_id` - Chain identifier for domain separation (Issue 1.2)
///
/// # Returns
///
/// Ok(()) if the commit is valid, Err with details if invalid.
pub fn verify_commit(
    commit: &Commit,
    validator_set: &ValidatorSet,
    expected_block_hash: BlockHash,
    chain_id: &str,
) -> Result<(), TendermintValidationError> {
    // Check block hash matches
    if commit.block_hash != expected_block_hash {
        return Err(TendermintValidationError::CommitBlockMismatch {
            expected: expected_block_hash,
            actual: commit.block_hash,
        });
    }

    // Check validator set is not empty
    if validator_set.is_empty() {
        return Err(TendermintValidationError::EmptyValidatorSet);
    }

    // Calculate total voting power and committed power
    let total_power = validator_set.total_power();
    let threshold = validator_set.two_thirds_threshold();
    let mut committed_power: u64 = 0;

    // Verify each commit signature
    for (idx, sig) in commit.signatures.iter().enumerate() {
        match sig.block_id_flag {
            BlockIDFlag::Commit => {
                // This validator committed to the block
                let validator_id = ValidatorId::new(idx as u8);

                // Get voting power for this validator
                let power = validator_set
                    .get_power(&validator_id)
                    .unwrap_or(0);

                // Get public key and verify signature
                if let Ok(public_key) = validator_set.get_public_key(&validator_id) {
                    // Build the signing message (same as what the validator signed)
                    // Issue 1.2: pass chain_id for domain separation
                    let signing_message = commit_sig_signing_bytes(
                        commit.height,
                        commit.round,
                        &commit.block_hash,
                        chain_id,
                    );

                    // Verify the signature if present
                    if let Some(signature) = &sig.signature {
                        if signature.verify(public_key, signing_message) {
                            committed_power += power;
                        }
                    }
                    // Note: Invalid or missing signatures are silently skipped, not added to power
                }
            }
            BlockIDFlag::Absent | BlockIDFlag::Nil => {
                // These don't count towards commit power
            }
        }
    }

    // Check threshold
    if committed_power < threshold {
        return Err(TendermintValidationError::InsufficientCommitPower {
            committed: committed_power,
            threshold,
            total: total_power,
        });
    }

    Ok(())
}

/// Detect double-vote evidence (equivocation).
///
/// Returns equivocation evidence if the same validator voted for different
/// blocks at the same (height, round, vote_type).
///
/// # Arguments
///
/// * `new_vote` - The new vote being added
/// * `existing_votes` - Map of existing votes by validator ID
///
/// # Returns
///
/// Some(EquivocationEvidence) if double-voting is detected, None otherwise.
pub fn check_for_equivocation(
    new_vote: &Vote,
    existing_votes: &HashMap<ValidatorId, Vote>,
) -> Option<EquivocationEvidence> {
    if let Some(existing) = existing_votes.get(&new_vote.validator) {
        // Check if this is the same (height, round, vote_type) but different block_hash
        if existing.height == new_vote.height
            && existing.round == new_vote.round
            && existing.vote_type == new_vote.vote_type
            && existing.block_hash != new_vote.block_hash
        {
            let kind = match new_vote.vote_type {
                VoteType::Prevote => EquivocationType::DoublePrevote,
                VoteType::Precommit => EquivocationType::DoublePrecommit,
            };
            return Some(EquivocationEvidence {
                kind,
                culprit: new_vote.validator,
                height: new_vote.height,
                round: new_vote.round,
                vote_a: existing.clone(),
                vote_b: new_vote.clone(),
            });
        }
    }
    None
}

/// Validate the last_commit field for a block.
///
/// For non-genesis blocks, validates that:
/// 1. last_commit is present
/// 2. last_commit.height == block_height - 1
/// 3. last_commit.block_hash == parent_hash
///
/// # Arguments
///
/// * `block_height` - Height of the block being validated
/// * `parent_hash` - Parent hash of the block
/// * `last_commit` - The last_commit field from the block
///
/// # Returns
///
/// Ok(()) if valid, Err with details if invalid.
pub fn validate_last_commit(
    block_height: Height,
    parent_hash: &BlockHash,
    last_commit: Option<&Commit>,
) -> Result<(), TendermintValidationError> {
    // Genesis block (height 0) doesn't need last_commit
    if block_height == 0 {
        return Ok(());
    }

    // Non-genesis blocks require last_commit
    let commit = last_commit.ok_or(TendermintValidationError::MissingLastCommit(block_height))?;

    // Verify commit height matches previous block
    let expected_commit_height = block_height - 1;
    if commit.height != expected_commit_height {
        return Err(TendermintValidationError::CommitHeightMismatch {
            expected: expected_commit_height,
            actual: commit.height,
        });
    }

    // Verify commit block_hash matches parent_hash
    if &commit.block_hash != parent_hash {
        return Err(TendermintValidationError::CommitHashMismatch(block_height));
    }

    Ok(())
}

/// Build the signing bytes for a commit signature.
///
/// This creates the canonical byte representation that validators sign
/// when creating precommit votes that become part of a Commit.
///
/// IMPORTANT: This must match Vote::signing_root() for Precommit votes.
/// The format is: keccak256(chain_id || vote_type || height || round || block_hash)
///
/// Issue 1.2: Added chain_id parameter for domain separation to prevent replay attacks.
fn commit_sig_signing_bytes(height: Height, round: Round, block_hash: &BlockHash, chain_id: &str) -> ethereum_types::H256 {
    use tiny_keccak::{Hasher, Keccak};

    let mut hasher = Keccak::v256();
    // Issue 1.2: Add chain_id for domain separation
    hasher.update(chain_id.as_bytes());
    // Issue 1.1 FIX: Include vote_type to match Vote::signing_root()
    // Commit signatures are from Precommit votes
    hasher.update(&[VoteType::Precommit as u8]);
    hasher.update(&height.to_le_bytes());
    hasher.update(&round.to_le_bytes());
    hasher.update(block_hash.as_bytes());

    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    ethereum_types::H256::from(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors_v2::chain::tendermint::messages::EquivocationType;
    use lighthouse_wrapper::bls::{Keypair, Signature};
    use std::str::FromStr;

    fn create_mock_validator_set() -> ValidatorSet {
        // Create a simple validator set with 3 validators
        let pubkeys: Vec<lighthouse_wrapper::bls::PublicKey> = (0..3)
            .map(|_| Keypair::random().pk)
            .collect();
        ValidatorSet::with_equal_power(pubkeys)
    }

    #[test]
    fn test_validate_last_commit_genesis() {
        // Genesis block (height 0) doesn't need last_commit
        let result = validate_last_commit(0, &BlockHash::zero(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_last_commit_missing() {
        // Non-genesis without last_commit should fail
        let parent_hash = BlockHash::from_low_u64_be(1);
        let result = validate_last_commit(1, &parent_hash, None);
        assert!(matches!(
            result,
            Err(TendermintValidationError::MissingLastCommit(1))
        ));
    }

    #[test]
    fn test_validate_last_commit_height_mismatch() {
        let parent_hash = BlockHash::from_low_u64_be(1);
        let commit = Commit::new(5, 0, parent_hash, vec![]); // Wrong height
        let result = validate_last_commit(2, &parent_hash, Some(&commit));
        assert!(matches!(
            result,
            Err(TendermintValidationError::CommitHeightMismatch { expected: 1, actual: 5 })
        ));
    }

    #[test]
    fn test_validate_last_commit_hash_mismatch() {
        let parent_hash = BlockHash::from_low_u64_be(1);
        let wrong_hash = BlockHash::from_low_u64_be(999);
        let commit = Commit::new(0, 0, wrong_hash, vec![]); // Wrong hash
        let result = validate_last_commit(1, &parent_hash, Some(&commit));
        assert!(matches!(
            result,
            Err(TendermintValidationError::CommitHashMismatch(1))
        ));
    }

    #[test]
    fn test_validate_last_commit_valid() {
        let parent_hash = BlockHash::from_low_u64_be(1);
        let commit = Commit::new(0, 0, parent_hash, vec![]);
        let result = validate_last_commit(1, &parent_hash, Some(&commit));
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_for_equivocation_no_existing() {
        let vote = Vote {
            height: 1,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::from_low_u64_be(1)),
            validator: ValidatorId::new(0),
            signature: Signature::empty(),
            timestamp: 0,
        };
        let existing: HashMap<ValidatorId, Vote> = HashMap::new();

        assert!(check_for_equivocation(&vote, &existing).is_none());
    }

    #[test]
    fn test_check_for_equivocation_same_vote() {
        let vote = Vote {
            height: 1,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::from_low_u64_be(1)),
            validator: ValidatorId::new(0),
            signature: Signature::empty(),
            timestamp: 0,
        };
        let mut existing: HashMap<ValidatorId, Vote> = HashMap::new();
        existing.insert(ValidatorId::new(0), vote.clone());

        // Same vote - no equivocation
        assert!(check_for_equivocation(&vote, &existing).is_none());
    }

    #[test]
    fn test_check_for_equivocation_detected() {
        let vote1 = Vote {
            height: 1,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::from_low_u64_be(1)),
            validator: ValidatorId::new(0),
            signature: Signature::empty(),
            timestamp: 0,
        };
        let vote2 = Vote {
            height: 1,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::from_low_u64_be(2)), // Different block!
            validator: ValidatorId::new(0),
            signature: Signature::empty(),
            timestamp: 0,
        };
        let mut existing: HashMap<ValidatorId, Vote> = HashMap::new();
        existing.insert(ValidatorId::new(0), vote1);

        let evidence = check_for_equivocation(&vote2, &existing);
        assert!(evidence.is_some());
        let ev = evidence.unwrap();
        assert_eq!(ev.culprit, ValidatorId::new(0));
        assert_eq!(ev.height, 1);
        assert_eq!(ev.round, 0);
        assert_eq!(ev.kind, EquivocationType::DoublePrevote);
    }

    // Test chain_id constant for Issue 1.2 domain separation tests
    const TEST_CHAIN_ID: &str = "test-chain-1337";

    #[test]
    fn test_empty_validator_set_error() {
        let empty_set = ValidatorSet::with_equal_power(vec![]);
        let commit = Commit::new(1, 0, BlockHash::from_low_u64_be(1), vec![]);

        let result = verify_commit(&commit, &empty_set, BlockHash::from_low_u64_be(1), TEST_CHAIN_ID);
        assert!(matches!(result, Err(TendermintValidationError::EmptyValidatorSet)));
    }

    #[test]
    fn test_commit_block_mismatch() {
        let validator_set = create_mock_validator_set();
        let commit = Commit::new(1, 0, BlockHash::from_low_u64_be(1), vec![]);
        let wrong_expected = BlockHash::from_low_u64_be(999);

        let result = verify_commit(&commit, &validator_set, wrong_expected, TEST_CHAIN_ID);
        assert!(matches!(
            result,
            Err(TendermintValidationError::CommitBlockMismatch { .. })
        ));
    }

    #[test]
    fn test_commit_sig_signing_bytes_matches_vote_signing_root() {
        // Issue 1.1 & 1.2 test: Verify commit_sig_signing_bytes matches Vote::signing_root
        // for Precommit votes (with chain_id domain separation)
        use crate::actors_v2::chain::tendermint::messages::Vote;

        let height = 100u64;
        let round = 5u32;
        let block_hash = BlockHash::from_low_u64_be(12345);

        // Create a precommit vote
        let vote = Vote {
            height,
            round,
            vote_type: VoteType::Precommit,
            block_hash: Some(block_hash),
            validator: ValidatorId::new(0),
            timestamp: 0,
            signature: Signature::empty(),
        };

        // Get the signing root from the vote (Issue 1.2: with chain_id)
        let vote_signing_root = vote.signing_root(TEST_CHAIN_ID);

        // Get the commit signing bytes (used in verify_commit) (Issue 1.2: with chain_id)
        let commit_signing_bytes = commit_sig_signing_bytes(height, round, &block_hash, TEST_CHAIN_ID);

        // They must match for commit verification to work
        assert_eq!(
            vote_signing_root.as_bytes(),
            commit_signing_bytes.as_bytes(),
            "commit_sig_signing_bytes must match Vote::signing_root() for Precommit"
        );
    }

    #[test]
    fn test_signing_roots_differ_by_chain_id() {
        // Issue 1.2: Verify that different chain_ids produce different signing roots
        use crate::actors_v2::chain::tendermint::messages::Vote;

        let vote = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::zero()),
            validator: ValidatorId::new(0),
            timestamp: 0,
            signature: Signature::empty(),
        };

        let root_mainnet = vote.signing_root("mainnet");
        let root_testnet = vote.signing_root("testnet");

        assert_ne!(
            root_mainnet, root_testnet,
            "Signing roots must differ for different chain_ids (domain separation)"
        );
    }
}
