//! Storage for messages received for future rounds.
//!
//! When a node receives messages for rounds it hasn't reached yet,
//! they are stored here and applied when the node advances to that round.
//! This module tracks VOTING POWER, not just message counts.
//!
//! # Specification Reference
//!
//! Per the [Consensus Message Types](https://github.com/tendermint/spec/blob/master/spec/p2p/messages/consensus.md):
//!
//! > "A node NODE_A that is ahead of NODE_B can send NODE_B prevotes or precommits
//! > for NODE_B's current (or future) round to enable it to progress forward."
//!
//! # Safety Properties
//!
//! 1. **Bounded Storage**: MAX_FUTURE_ROUNDS prevents memory exhaustion
//! 2. **Vote Deduplication**: HashMap by ValidatorId prevents double-counting
//! 3. **Power Tracking**: Tracks voting power, not message counts

use super::messages::{Proposal, Vote};
use super::round_sync::{analyze_future_round_votes, two_thirds_threshold, FutureRoundAction};
use super::types::{BlockHash, Round, ValidatorId, ValidatorSet, VoteType, VotingPower};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Maximum number of future rounds to store (prevents memory exhaustion)
///
/// 20 rounds is generous - even at 1 second per round, this is 20 seconds
/// of messages, far more than typical network delays.
const MAX_FUTURE_ROUNDS: u32 = 20;

// ═══════════════════════════════════════════════════════════════════════════
// FUTURE ROUND VOTES
// ═══════════════════════════════════════════════════════════════════════════

/// Votes collected for a specific future round.
///
/// Tracks both individual votes (for later replay) and aggregate voting power
/// (for threshold detection).
#[derive(Debug)]
pub struct FutureRoundVotes {
    /// Round number
    pub round: Round,

    /// Prevotes indexed by validator ID (prevents duplicates)
    prevotes: HashMap<ValidatorId, Vote>,

    /// Precommits indexed by validator ID
    precommits: HashMap<ValidatorId, Vote>,

    /// Total prevote voting power collected
    prevote_power: VotingPower,

    /// Total precommit voting power collected
    precommit_power: VotingPower,

    /// Precommit power per block hash (for commit detection)
    precommit_power_by_block: HashMap<BlockHash, VotingPower>,

    /// Precommit power for NIL
    precommit_nil_power: VotingPower,

    /// Prevote power per block hash (for PoLC detection)
    prevote_power_by_block: HashMap<BlockHash, VotingPower>,

    /// Reference to validator set for power lookups
    validator_set: Arc<ValidatorSet>,
}

impl FutureRoundVotes {
    /// Create a new vote collection for a specific round
    pub fn new(round: Round, validator_set: Arc<ValidatorSet>) -> Self {
        Self {
            round,
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            prevote_power: 0,
            precommit_power: 0,
            precommit_power_by_block: HashMap::new(),
            precommit_nil_power: 0,
            prevote_power_by_block: HashMap::new(),
            validator_set,
        }
    }

    /// Add a vote, returns the action to take (if threshold reached)
    ///
    /// # Note
    ///
    /// The vote should be pre-validated (signature verified) before calling this method.
    pub fn add_vote(&mut self, vote: Vote) -> FutureRoundAction {
        // Get validator's voting power
        let power = self
            .validator_set
            .get_power(&vote.validator)
            .unwrap_or(0);

        match vote.vote_type {
            VoteType::Prevote => {
                // Only count if not already received from this validator
                if self.prevotes.insert(vote.validator, vote.clone()).is_none() {
                    self.prevote_power += power;

                    if let Some(block_hash) = vote.block_hash {
                        *self.prevote_power_by_block.entry(block_hash).or_insert(0) += power;
                    }
                }
            }
            VoteType::Precommit => {
                if self.precommits.insert(vote.validator, vote.clone()).is_none() {
                    self.precommit_power += power;

                    match vote.block_hash {
                        Some(block_hash) => {
                            *self.precommit_power_by_block.entry(block_hash).or_insert(0) += power;
                        }
                        None => {
                            self.precommit_nil_power += power;
                        }
                    }
                }
            }
        }

        // Check if we've reached any threshold
        let max_block_precommit = self
            .precommit_power_by_block
            .iter()
            .max_by_key(|(_, &power)| power)
            .map(|(hash, &power)| (*hash, power));

        analyze_future_round_votes(
            self.round,
            self.prevote_power,
            self.precommit_power,
            max_block_precommit,
            self.precommit_nil_power,
            self.validator_set.total_power(),
        )
    }

    /// Check if we have a PoLC (Proof of Lock Change) for a specific block
    pub fn has_polc_for_block(&self, block_hash: &BlockHash) -> bool {
        let threshold = two_thirds_threshold(self.validator_set.total_power());
        self.prevote_power_by_block
            .get(block_hash)
            .copied()
            .unwrap_or(0)
            >= threshold
    }

    /// Get all prevotes
    pub fn prevotes(&self) -> impl Iterator<Item = &Vote> {
        self.prevotes.values()
    }

    /// Get all precommits
    pub fn precommits(&self) -> impl Iterator<Item = &Vote> {
        self.precommits.values()
    }

    /// Get total prevote power
    pub fn prevote_power(&self) -> VotingPower {
        self.prevote_power
    }

    /// Get total precommit power
    pub fn precommit_power(&self) -> VotingPower {
        self.precommit_power
    }

    /// Get the block with highest prevote power (if any)
    pub fn highest_prevote_block(&self) -> Option<(BlockHash, VotingPower)> {
        self.prevote_power_by_block
            .iter()
            .max_by_key(|(_, &power)| power)
            .map(|(hash, &power)| (*hash, power))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FUTURE MESSAGE STORE
// ═══════════════════════════════════════════════════════════════════════════

/// Storage for votes and proposals from future rounds.
///
/// This is the main interface for managing future round messages. It handles:
/// - Vote storage with automatic threshold detection
/// - Proposal storage (one per round)
/// - Cleanup when advancing rounds
/// - PoLC verification for unlocking
#[derive(Debug)]
pub struct FutureMessageStore {
    /// Proposals indexed by round
    proposals: HashMap<Round, Proposal>,

    /// Votes indexed by round
    votes: HashMap<Round, FutureRoundVotes>,

    /// Current round (messages for rounds <= this are not stored)
    current_round: Round,

    /// Current validator set
    validator_set: Arc<ValidatorSet>,
}

impl FutureMessageStore {
    /// Create a new store with the given validator set
    pub fn new(validator_set: Arc<ValidatorSet>) -> Self {
        Self {
            proposals: HashMap::new(),
            votes: HashMap::new(),
            current_round: 0,
            validator_set,
        }
    }

    /// Update validator set (e.g., at height change)
    ///
    /// # Note
    ///
    /// Existing future votes retain their original validator set reference.
    /// This is intentional - votes were valid under the old set.
    pub fn update_validator_set(&mut self, validator_set: Arc<ValidatorSet>) {
        self.validator_set = validator_set;
    }

    /// Update current round and clean up old messages
    pub fn advance_to_round(&mut self, round: Round) {
        self.current_round = round;

        // Clean up messages from rounds we've passed
        self.proposals.retain(|&r, _| r > round);
        self.votes.retain(|&r, _| r > round);

        // Clean up messages too far in the future
        let max_round = round.saturating_add(MAX_FUTURE_ROUNDS);
        self.proposals.retain(|&r, _| r <= max_round);
        self.votes.retain(|&r, _| r <= max_round);
    }

    /// Reset the store for a new height
    pub fn reset(&mut self, validator_set: Arc<ValidatorSet>) {
        self.proposals.clear();
        self.votes.clear();
        self.current_round = 0;
        self.validator_set = validator_set;
    }

    /// Store a vote for a future round (vote must be pre-validated!)
    ///
    /// Returns the action to take if a threshold is reached.
    pub fn store_vote(&mut self, vote: Vote) -> FutureRoundAction {
        if vote.round <= self.current_round {
            return FutureRoundAction::NoAction;
        }

        if vote.round > self.current_round.saturating_add(MAX_FUTURE_ROUNDS) {
            debug!(
                vote_round = vote.round,
                current_round = self.current_round,
                max_future = MAX_FUTURE_ROUNDS,
                "Rejecting vote too far in future"
            );
            return FutureRoundAction::NoAction;
        }

        let round_votes = self
            .votes
            .entry(vote.round)
            .or_insert_with(|| FutureRoundVotes::new(vote.round, self.validator_set.clone()));

        round_votes.add_vote(vote)
    }

    /// Store a proposal for a future round
    ///
    /// Returns true if the proposal was stored (first valid one for this round).
    pub fn store_proposal(&mut self, proposal: Proposal) -> bool {
        if proposal.round <= self.current_round {
            return false;
        }

        if proposal.round > self.current_round.saturating_add(MAX_FUTURE_ROUNDS) {
            return false;
        }

        // Only store one proposal per round (first valid one)
        if !self.proposals.contains_key(&proposal.round) {
            self.proposals.insert(proposal.round, proposal);
            true
        } else {
            false
        }
    }

    /// Get stored proposal for a specific round (removes it from store)
    pub fn take_proposal(&mut self, round: Round) -> Option<Proposal> {
        self.proposals.remove(&round)
    }

    /// Get stored proposal for a specific round (without removing)
    pub fn get_proposal(&self, round: Round) -> Option<&Proposal> {
        self.proposals.get(&round)
    }

    /// Get stored votes for a specific round (removes them from store)
    pub fn take_votes(&mut self, round: Round) -> Option<FutureRoundVotes> {
        self.votes.remove(&round)
    }

    /// Get stored votes for a specific round (without removing)
    pub fn get_votes(&self, round: Round) -> Option<&FutureRoundVotes> {
        self.votes.get(&round)
    }

    /// Check if any intermediate round has a PoLC for a different block.
    ///
    /// This is used for unlocking during round skip. Per Tendermint spec,
    /// a node can unlock from block A if it sees a PoLC for block B in
    /// an intermediate round between the lock round and current round.
    ///
    /// # Arguments
    ///
    /// * `locked_round` - The round at which the node locked on locked_block
    /// * `locked_block` - The block hash the node is currently locked on
    /// * `target_round` - The round we're advancing to
    ///
    /// # Returns
    ///
    /// If found, returns `Some((polc_round, polc_block))` - the round where
    /// the PoLC was found and the block it was for.
    pub fn find_polc_for_unlock(
        &self,
        locked_round: Round,
        locked_block: &BlockHash,
        target_round: Round,
    ) -> Option<(Round, BlockHash)> {
        for round in (locked_round + 1)..target_round {
            if let Some(round_votes) = self.votes.get(&round) {
                // Check each block hash for PoLC
                for (block_hash, &power) in &round_votes.prevote_power_by_block {
                    if block_hash != locked_block {
                        let threshold = two_thirds_threshold(round_votes.validator_set.total_power());
                        if power >= threshold {
                            return Some((round, *block_hash));
                        }
                    }
                }
            }
        }
        None
    }

    /// Get the number of stored proposals
    pub fn proposal_count(&self) -> usize {
        self.proposals.len()
    }

    /// Get the number of rounds with stored votes
    pub fn vote_round_count(&self) -> usize {
        self.votes.len()
    }

    /// Check if there are any stored future messages
    pub fn is_empty(&self) -> bool {
        self.proposals.is_empty() && self.votes.is_empty()
    }

    /// Get the current round
    pub fn current_round(&self) -> Round {
        self.current_round
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_types::H256;
    use lighthouse_wrapper::bls::{Keypair, PublicKey, Signature as BLSSignature};
    use std::str::FromStr;

    fn test_hash(byte: u8) -> BlockHash {
        let mut bytes = [0u8; 32];
        bytes[0] = byte;
        H256::from_slice(&bytes)
    }

    fn mock_pubkey() -> PublicKey {
        PublicKey::from_str(
            "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
        ).expect("valid test public key")
    }

    fn create_test_validator_set(count: usize) -> Arc<ValidatorSet> {
        let validators: Vec<PublicKey> = (0..count).map(|_| mock_pubkey()).collect();
        Arc::new(ValidatorSet::with_equal_power(validators))
    }

    fn create_test_vote(round: Round, validator_idx: u8, block_hash: Option<BlockHash>, vote_type: VoteType) -> Vote {
        Vote {
            height: 1,
            round,
            vote_type,
            block_hash,
            validator: ValidatorId::new(validator_idx),
            timestamp: 0,
            signature: BLSSignature::empty(),
        }
    }

    #[test]
    fn test_future_round_votes_basic() {
        let validator_set = create_test_validator_set(4);
        let mut votes = FutureRoundVotes::new(5, validator_set);

        // Add a prevote
        let vote = create_test_vote(5, 0, Some(test_hash(1)), VoteType::Prevote);
        let action = votes.add_vote(vote);

        // With only 1 of 4, should be NoAction
        assert_eq!(action, FutureRoundAction::NoAction);
        assert_eq!(votes.prevote_power(), 1);
    }

    #[test]
    fn test_future_round_votes_threshold_detection() {
        let validator_set = create_test_validator_set(4);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add 3 prevotes (threshold is 3 for 4 validators)
        for i in 0..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Prevote);
            let action = votes.add_vote(vote);

            if i < 2 {
                assert_eq!(action, FutureRoundAction::NoAction);
            } else {
                assert!(matches!(action, FutureRoundAction::AdvanceToPrevote { round: 5, .. }));
            }
        }
    }

    #[test]
    fn test_future_round_votes_commit_detection() {
        let validator_set = create_test_validator_set(4);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add 3 precommits for a block
        for i in 0..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Precommit);
            let action = votes.add_vote(vote);

            if i < 2 {
                assert_eq!(action, FutureRoundAction::NoAction);
            } else {
                assert!(matches!(
                    action,
                    FutureRoundAction::CommitBlock {
                        round: 5,
                        block_hash: _
                    }
                ));
            }
        }
    }

    #[test]
    fn test_future_round_votes_nil_precommits() {
        let validator_set = create_test_validator_set(4);
        let mut votes = FutureRoundVotes::new(5, validator_set);

        // Add 3 NIL precommits
        for i in 0..3 {
            let vote = create_test_vote(5, i, None, VoteType::Precommit);
            let action = votes.add_vote(vote);

            if i < 2 {
                assert_eq!(action, FutureRoundAction::NoAction);
            } else {
                assert!(matches!(
                    action,
                    FutureRoundAction::AdvanceToNextRound {
                        nil_round: 5,
                        target_round: 6
                    }
                ));
            }
        }
    }

    #[test]
    fn test_future_round_votes_duplicate_prevention() {
        let validator_set = create_test_validator_set(4);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add same vote twice
        let vote = create_test_vote(5, 0, Some(block_hash), VoteType::Prevote);
        votes.add_vote(vote.clone());
        votes.add_vote(vote);

        // Should only count once
        assert_eq!(votes.prevote_power(), 1);
    }

    #[test]
    fn test_future_message_store_basic() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        assert!(store.is_empty());
        assert_eq!(store.current_round(), 0);
    }

    #[test]
    fn test_future_message_store_vote_storage() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Store vote for round 5
        let vote = create_test_vote(5, 0, Some(test_hash(1)), VoteType::Prevote);
        let action = store.store_vote(vote);

        assert_eq!(action, FutureRoundAction::NoAction);
        assert_eq!(store.vote_round_count(), 1);
    }

    #[test]
    fn test_future_message_store_rejects_past_votes() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Advance to round 5
        store.advance_to_round(5);

        // Try to store vote for round 3 (past)
        let vote = create_test_vote(3, 0, Some(test_hash(1)), VoteType::Prevote);
        let action = store.store_vote(vote);

        assert_eq!(action, FutureRoundAction::NoAction);
        assert!(store.is_empty());
    }

    #[test]
    fn test_future_message_store_rejects_far_future_votes() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Try to store vote for round 100 (too far in future)
        let vote = create_test_vote(100, 0, Some(test_hash(1)), VoteType::Prevote);
        let action = store.store_vote(vote);

        assert_eq!(action, FutureRoundAction::NoAction);
        assert!(store.is_empty());
    }

    #[test]
    fn test_future_message_store_cleanup_on_advance() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Store votes for rounds 2, 5, 8
        for round in [2, 5, 8] {
            let vote = create_test_vote(round, 0, Some(test_hash(1)), VoteType::Prevote);
            store.store_vote(vote);
        }

        assert_eq!(store.vote_round_count(), 3);

        // Advance to round 5
        store.advance_to_round(5);

        // Rounds 2 and 5 should be removed
        assert_eq!(store.vote_round_count(), 1);
        assert!(store.get_votes(2).is_none());
        assert!(store.get_votes(5).is_none());
        assert!(store.get_votes(8).is_some());
    }

    #[test]
    fn test_polc_for_unlock() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        let locked_block = test_hash(1);
        let unlock_block = test_hash(2);

        // We're "locked" on block 1 at round 2
        // Store 2/3+ prevotes for block 2 in round 4
        for i in 0..3 {
            let vote = create_test_vote(4, i, Some(unlock_block), VoteType::Prevote);
            store.store_vote(vote);
        }

        // Should find PoLC for unlock_block in round 4
        let polc = store.find_polc_for_unlock(2, &locked_block, 6);
        assert!(polc.is_some());
        let (polc_round, polc_block) = polc.unwrap();
        assert_eq!(polc_round, 4);
        assert_eq!(polc_block, unlock_block);
    }

    #[test]
    fn test_polc_for_unlock_same_block_not_counted() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        let locked_block = test_hash(1);

        // Store 2/3+ prevotes for the SAME locked block
        for i in 0..3 {
            let vote = create_test_vote(4, i, Some(locked_block), VoteType::Prevote);
            store.store_vote(vote);
        }

        // Should NOT find PoLC (same block as locked)
        let polc = store.find_polc_for_unlock(2, &locked_block, 6);
        assert!(polc.is_none());
    }

    #[test]
    fn test_has_polc_for_block() {
        let validator_set = create_test_validator_set(4);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add 2 prevotes (not enough)
        for i in 0..2 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Prevote);
            votes.add_vote(vote);
        }
        assert!(!votes.has_polc_for_block(&block_hash));

        // Add third prevote (now enough)
        let vote = create_test_vote(5, 2, Some(block_hash), VoteType::Prevote);
        votes.add_vote(vote);
        assert!(votes.has_polc_for_block(&block_hash));
    }
}
