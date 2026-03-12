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
use super::types::{BlockHash, Height, Round, ValidatorId, ValidatorSet, VoteType, VotingPower};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

/// Default maximum number of future rounds to store (prevents memory exhaustion)
///
/// 20 rounds is generous - even at 1 second per round, this is 20 seconds
/// of messages, far more than typical network delays.
const DEFAULT_MAX_FUTURE_ROUNDS: u32 = 20;

/// Extended maximum for recovery mode after restart.
///
/// When a node restarts after being offline, it may be many rounds behind.
/// During recovery mode, we accept votes from up to 100 rounds in the future
/// to allow rapid catch-up to the network's current round.
const RECOVERY_MAX_FUTURE_ROUNDS: u32 = 100;

/// Default maximum number of future heights to store proposals for.
///
/// This allows caching proposals that arrive during height transitions.
/// 5 heights is generous - typically only height H+1 proposals arrive
/// during transition from H to H+1.
const DEFAULT_MAX_FUTURE_HEIGHTS: u64 = 5;

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
        self.add_vote_with_self_power(vote, None)
    }

    /// Add a vote with self-power for threshold calculation.
    ///
    /// When a node receives future round votes from other validators, it should
    /// include its own voting power when calculating whether the threshold is met.
    /// This is because once the node advances to this round, it WILL vote
    /// (guaranteed by the Tendermint protocol).
    ///
    /// This is critical for n=3 networks where the 2/3+ threshold is 100%.
    /// Without including self-power, a restarting node at round 0 cannot advance
    /// even when it receives votes from all other validators (2/3 = 66% < 100%).
    ///
    /// # Arguments
    ///
    /// * `vote` - The vote to add (must be pre-validated)
    /// * `our_power` - Our voting power to include in threshold calculation.
    ///   Pass `None` if we're not a validator or don't want to include self-power.
    ///
    /// # Safety
    ///
    /// This is safe because:
    /// 1. We're not counting our vote twice - we haven't voted yet
    /// 2. Once we advance, we WILL vote (Tendermint protocol guarantee)
    /// 3. This only affects OUR decision to advance; other nodes decide independently
    /// 4. Byzantine nodes cannot exploit this - they can't forge our intention
    pub fn add_vote_with_self_power(
        &mut self,
        vote: Vote,
        our_power: Option<VotingPower>,
    ) -> FutureRoundAction {
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

        // Calculate effective power including our future vote.
        // We include our power because once we advance to this round,
        // we WILL vote - this is guaranteed by the Tendermint protocol.
        let self_power = our_power.unwrap_or(0);

        // Check if we've reached any threshold
        let max_block_precommit = self
            .precommit_power_by_block
            .iter()
            .max_by_key(|(_, &power)| power)
            .map(|(hash, &power)| (*hash, power));

        analyze_future_round_votes(
            self.round,
            self.prevote_power + self_power,
            self.precommit_power + self_power,
            max_block_precommit,
            self.precommit_nil_power, // NIL doesn't include our future vote
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
/// - Future height proposal storage (one per height+round, replayed at height transition)
/// - Cleanup when advancing rounds
/// - PoLC verification for unlocking
/// - Dynamic recovery mode for post-restart catch-up
#[derive(Debug)]
pub struct FutureMessageStore {
    /// Proposals indexed by round (same height)
    proposals: HashMap<Round, Proposal>,

    /// Votes indexed by round
    votes: HashMap<Round, FutureRoundVotes>,

    /// Current round (messages for rounds <= this are not stored)
    current_round: Round,

    /// Current validator set
    validator_set: Arc<ValidatorSet>,

    /// Maximum future rounds to accept (configurable for recovery mode)
    max_future_rounds: u32,

    /// Whether recovery mode is enabled (extended future round acceptance)
    recovery_mode: bool,

    // ═══════════════════════════════════════════════════════════════════════════
    // FUTURE HEIGHT PROPOSAL STORAGE
    // ═══════════════════════════════════════════════════════════════════════════

    /// Proposals for future heights, indexed by (height, round).
    ///
    /// When a proposal arrives for height H+1 while we're at height H, we store
    /// it here and replay it when we transition to height H+1.
    future_height_proposals: HashMap<(Height, Round), Proposal>,

    /// Current height (used for future height proposal storage/cleanup)
    current_height: Height,

    /// Maximum future heights to store proposals for
    max_future_heights: u64,
}

impl FutureMessageStore {
    /// Create a new store with the given validator set
    pub fn new(validator_set: Arc<ValidatorSet>) -> Self {
        Self {
            proposals: HashMap::new(),
            votes: HashMap::new(),
            current_round: 0,
            validator_set,
            max_future_rounds: DEFAULT_MAX_FUTURE_ROUNDS,
            recovery_mode: false,
            // Future height proposal storage
            future_height_proposals: HashMap::new(),
            current_height: 0,
            max_future_heights: DEFAULT_MAX_FUTURE_HEIGHTS,
        }
    }

    /// Enable recovery mode for faster round catch-up after restart.
    ///
    /// In recovery mode, the store accepts votes from up to 100 rounds in the
    /// future instead of the default 20. This allows nodes that restart after
    /// being offline to quickly catch up to the network's current round.
    ///
    /// Recovery mode should be disabled after the node has caught up (typically
    /// after 30-60 seconds or after receiving votes from the current round).
    pub fn enable_recovery_mode(&mut self) {
        if !self.recovery_mode {
            self.max_future_rounds = RECOVERY_MAX_FUTURE_ROUNDS;
            self.recovery_mode = true;
            debug!(
                max_future_rounds = RECOVERY_MAX_FUTURE_ROUNDS,
                "Recovery mode enabled - accepting extended future rounds"
            );
        }
    }

    /// Disable recovery mode and return to normal operation.
    ///
    /// This should be called after the node has caught up to the network's
    /// current round to prevent excessive memory usage.
    pub fn disable_recovery_mode(&mut self) {
        if self.recovery_mode {
            self.max_future_rounds = DEFAULT_MAX_FUTURE_ROUNDS;
            self.recovery_mode = false;
            debug!(
                max_future_rounds = DEFAULT_MAX_FUTURE_ROUNDS,
                "Recovery mode disabled - using default future round limit"
            );

            // Clean up any messages that are now too far in the future
            let max_round = self.current_round.saturating_add(DEFAULT_MAX_FUTURE_ROUNDS);
            self.proposals.retain(|&r, _| r <= max_round);
            self.votes.retain(|&r, _| r <= max_round);
        }
    }

    /// Check if recovery mode is enabled
    pub fn is_recovery_mode(&self) -> bool {
        self.recovery_mode
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
        let max_round = round.saturating_add(self.max_future_rounds);
        self.proposals.retain(|&r, _| r <= max_round);
        self.votes.retain(|&r, _| r <= max_round);
    }

    /// Reset the store for a new height
    pub fn reset(&mut self, validator_set: Arc<ValidatorSet>) {
        self.proposals.clear();
        self.votes.clear();
        self.current_round = 0;
        self.validator_set = validator_set;
        // Note: recovery_mode is preserved across height changes
        // Note: future_height_proposals is NOT cleared here - use set_current_height() instead
        // so we can replay proposals when advancing to the new height
    }

    /// Store a vote for a future round (vote must be pre-validated!)
    ///
    /// Returns the action to take if a threshold is reached.
    pub fn store_vote(&mut self, vote: Vote) -> FutureRoundAction {
        self.store_vote_with_self_power(vote, None)
    }

    /// Store a vote for a future round, including our own power for threshold calculation.
    ///
    /// This variant includes our voting power when calculating whether thresholds are met.
    /// This is critical for n=3 networks where the 2/3+ threshold is 100% (all 3 validators).
    ///
    /// # The Problem This Solves
    ///
    /// When a node restarts, it starts at round 0 while other nodes may be at round N.
    /// The restarting node receives future round votes from the other validators.
    /// Without including self-power:
    /// - n=3, threshold = 3 (100%)
    /// - Node receives 2 votes from peers
    /// - 2 < 3 → NoAction → deadlock!
    ///
    /// With self-power:
    /// - Node receives 2 votes from peers
    /// - 2 peer votes + 1 self = 3 = threshold met
    /// - Node advances to round N and votes
    /// - Consensus resumes
    ///
    /// # Arguments
    ///
    /// * `vote` - The vote to store (must be pre-validated)
    /// * `our_power` - Our voting power to include in threshold calculation.
    ///   Pass `None` for observer nodes or when self-power shouldn't be counted.
    pub fn store_vote_with_self_power(
        &mut self,
        vote: Vote,
        our_power: Option<VotingPower>,
    ) -> FutureRoundAction {
        if vote.round <= self.current_round {
            return FutureRoundAction::NoAction;
        }

        if vote.round > self.current_round.saturating_add(self.max_future_rounds) {
            debug!(
                vote_round = vote.round,
                current_round = self.current_round,
                max_future = self.max_future_rounds,
                recovery_mode = self.recovery_mode,
                "Rejecting vote too far in future"
            );
            return FutureRoundAction::NoAction;
        }

        let round_votes = self
            .votes
            .entry(vote.round)
            .or_insert_with(|| FutureRoundVotes::new(vote.round, self.validator_set.clone()));

        round_votes.add_vote_with_self_power(vote, our_power)
    }

    /// Store a proposal for a future round
    ///
    /// Returns true if the proposal was stored (first valid one for this round).
    pub fn store_proposal(&mut self, proposal: Proposal) -> bool {
        if proposal.round <= self.current_round {
            return false;
        }

        if proposal.round > self.current_round.saturating_add(self.max_future_rounds) {
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

    // ═══════════════════════════════════════════════════════════════════════════
    // FUTURE HEIGHT PROPOSAL METHODS
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set the current height and clean up old future height proposals.
    ///
    /// This should be called at height transitions to:
    /// 1. Update the current height
    /// 2. Clean up proposals for heights we've passed
    /// 3. Clean up proposals too far in the future
    pub fn set_current_height(&mut self, height: Height) {
        self.current_height = height;

        // Clean up proposals for heights we've passed
        self.future_height_proposals.retain(|(h, _), _| *h > height);

        // Clean up proposals too far in the future
        let max_height = height.saturating_add(self.max_future_heights);
        self.future_height_proposals
            .retain(|(h, _), _| *h <= max_height);
    }

    /// Get the current height
    pub fn current_height(&self) -> Height {
        self.current_height
    }

    /// Store a proposal from a future height.
    ///
    /// Proposals are stored indexed by (height, round). Only one proposal per
    /// (height, round) is stored - the first valid one.
    ///
    /// # Arguments
    ///
    /// * `proposal` - The proposal to store (should be pre-validated!)
    ///
    /// # Returns
    ///
    /// `true` if the proposal was stored, `false` if:
    /// - Height is not in the future
    /// - Height is too far in the future
    /// - A proposal already exists for this (height, round)
    pub fn store_future_height_proposal(&mut self, proposal: Proposal) -> bool {
        // Proposal must be for a future height
        if proposal.height <= self.current_height {
            return false;
        }

        // Reject proposals too far in the future
        if proposal.height > self.current_height.saturating_add(self.max_future_heights) {
            debug!(
                proposal_height = proposal.height,
                current_height = self.current_height,
                max_future = self.max_future_heights,
                "Rejecting future height proposal - too far ahead"
            );
            return false;
        }

        let key = (proposal.height, proposal.round);

        // Only store one proposal per (height, round) - first valid one wins
        if self.future_height_proposals.contains_key(&key) {
            debug!(
                height = proposal.height,
                round = proposal.round,
                "Future height proposal not stored - already have one for this (height, round)"
            );
            return false;
        }

        debug!(
            height = proposal.height,
            round = proposal.round,
            proposer = ?proposal.proposer,
            "Storing future height proposal"
        );

        self.future_height_proposals.insert(key, proposal);
        true
    }

    /// Take all stored proposals for a specific height.
    ///
    /// This removes the proposals from storage and returns them sorted by round.
    /// Call this when transitioning to a new height to replay stored proposals.
    ///
    /// # Arguments
    ///
    /// * `height` - The height to get proposals for
    ///
    /// # Returns
    ///
    /// A vector of proposals for this height, sorted by round (ascending).
    pub fn take_proposals_for_height(&mut self, height: Height) -> Vec<Proposal> {
        // Collect all proposals for this height
        let mut proposals: Vec<Proposal> = self
            .future_height_proposals
            .iter()
            .filter(|((h, _), _)| *h == height)
            .map(|(_, p)| p.clone())
            .collect();

        // Remove them from storage
        self.future_height_proposals.retain(|(h, _), _| *h != height);

        // Sort by round for deterministic replay order
        proposals.sort_by_key(|p| p.round);

        if !proposals.is_empty() {
            debug!(
                height = height,
                count = proposals.len(),
                "Retrieved stored future height proposals for replay"
            );
        }

        proposals
    }

    /// Get the number of stored future height proposals
    pub fn future_height_proposal_count(&self) -> usize {
        self.future_height_proposals.len()
    }

    /// Check if there's a stored proposal for a specific (height, round)
    pub fn has_future_height_proposal(&self, height: Height, round: Round) -> bool {
        self.future_height_proposals.contains_key(&(height, round))
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

    #[test]
    fn test_recovery_mode_extends_future_round_limit() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // By default, votes for round 50 should be rejected (default limit is 20)
        let vote = create_test_vote(50, 0, Some(test_hash(1)), VoteType::Prevote);
        let action = store.store_vote(vote);
        assert_eq!(action, FutureRoundAction::NoAction);
        assert!(store.is_empty());
        assert!(!store.is_recovery_mode());

        // Enable recovery mode
        store.enable_recovery_mode();
        assert!(store.is_recovery_mode());

        // Now votes for round 50 should be accepted (recovery limit is 100)
        let vote = create_test_vote(50, 0, Some(test_hash(1)), VoteType::Prevote);
        let action = store.store_vote(vote);
        assert_eq!(action, FutureRoundAction::NoAction); // Not enough votes for threshold
        assert_eq!(store.vote_round_count(), 1); // But vote was stored
    }

    #[test]
    fn test_recovery_mode_disable_cleans_up() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Enable recovery mode and store votes for far future rounds
        store.enable_recovery_mode();

        // Store votes at rounds 25, 50, 75 (beyond default limit of 20)
        for round in [25, 50, 75] {
            let vote = create_test_vote(round, 0, Some(test_hash(1)), VoteType::Prevote);
            store.store_vote(vote);
        }
        assert_eq!(store.vote_round_count(), 3);

        // Disable recovery mode
        store.disable_recovery_mode();
        assert!(!store.is_recovery_mode());

        // Votes beyond default limit (20) should be cleaned up
        // Round 25 is beyond limit, rounds 50 and 75 are definitely beyond
        assert_eq!(store.vote_round_count(), 0);
    }

    #[test]
    fn test_recovery_mode_preserved_across_reset() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set.clone());

        // Enable recovery mode
        store.enable_recovery_mode();
        assert!(store.is_recovery_mode());

        // Reset for new height
        store.reset(validator_set);

        // Recovery mode should be preserved
        assert!(store.is_recovery_mode());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SELF-POWER TESTS (TM-A1 chaos test fix)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_self_power_enables_n3_threshold_with_2_peer_votes() {
        // This is the TM-A1 chaos test scenario:
        // - 3 validators, threshold = 3 (100%)
        // - Node restarts at round 0, others at round N
        // - Without self-power: 2 peer votes < 3 threshold → deadlock
        // - With self-power: 2 peer votes + 1 self = 3 = threshold → advance

        let validator_set = create_test_validator_set(3);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add 2 prevotes from peer validators (not validator 0, which is us)
        for i in 1..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Prevote);
            let action = votes.add_vote_with_self_power(vote, Some(1)); // our_power = 1

            if i < 2 {
                // First peer vote: 1 peer + 1 self = 2 < 3 threshold
                assert_eq!(action, FutureRoundAction::NoAction);
            } else {
                // Second peer vote: 2 peers + 1 self = 3 = threshold met!
                assert!(matches!(action, FutureRoundAction::AdvanceToPrevote { round: 5, .. }),
                    "Expected AdvanceToPrevote, got {:?}", action);
            }
        }
    }

    #[test]
    fn test_self_power_none_behaves_like_original() {
        // When our_power is None, behavior should match the original add_vote()
        let validator_set = create_test_validator_set(3);
        let block_hash = test_hash(1);

        // Test with our_power = None
        let mut votes_none = FutureRoundVotes::new(5, validator_set.clone());
        for i in 1..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Prevote);
            let action = votes_none.add_vote_with_self_power(vote, None);

            // Without self-power: 2 votes < 3 threshold → no action
            assert_eq!(action, FutureRoundAction::NoAction);
        }

        // Compare with original add_vote()
        let mut votes_orig = FutureRoundVotes::new(5, validator_set);
        for i in 1..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Prevote);
            let action = votes_orig.add_vote(vote);
            assert_eq!(action, FutureRoundAction::NoAction);
        }
    }

    #[test]
    fn test_store_vote_with_self_power() {
        // Test the FutureMessageStore variant
        let validator_set = create_test_validator_set(3);
        let mut store = FutureMessageStore::new(validator_set);
        let block_hash = test_hash(1);

        // Store 2 votes for round 5 with self-power = 1
        let vote1 = create_test_vote(5, 1, Some(block_hash), VoteType::Prevote);
        let action1 = store.store_vote_with_self_power(vote1, Some(1));
        assert_eq!(action1, FutureRoundAction::NoAction);

        let vote2 = create_test_vote(5, 2, Some(block_hash), VoteType::Prevote);
        let action2 = store.store_vote_with_self_power(vote2, Some(1));
        // 2 peer votes + 1 self = 3 = threshold met
        assert!(matches!(action2, FutureRoundAction::AdvanceToPrevote { round: 5, .. }));
    }

    #[test]
    fn test_self_power_precommit_threshold() {
        // Test that self-power works for precommit thresholds.
        // Note: Self-power is added to total precommit_power, not per-block power.
        // This means we get AdvanceToPrecommit (Priority 3), not CommitBlock (Priority 1).
        // This is correct because we can't know which block we'll precommit until we vote.
        let validator_set = create_test_validator_set(3);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add 2 precommits from peer validators
        for i in 1..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Precommit);
            let action = votes.add_vote_with_self_power(vote, Some(1));

            if i < 2 {
                assert_eq!(action, FutureRoundAction::NoAction);
            } else {
                // 2 peers + 1 self = 3 = threshold reached
                // We get AdvanceToPrecommit because per-block power (2) < threshold,
                // but total precommit_power + self_power (3) >= threshold
                assert!(matches!(action, FutureRoundAction::AdvanceToPrecommit { round: 5, .. }),
                    "Expected AdvanceToPrecommit, got {:?}", action);
            }
        }
    }

    #[test]
    fn test_self_power_with_nil_precommits() {
        // When peers precommit NIL, self-power still enables round advancement.
        // This is correct because we should advance to participate in consensus,
        // even if the round will likely fail (we can then start the next round).
        let validator_set = create_test_validator_set(3);
        let mut votes = FutureRoundVotes::new(5, validator_set);

        // Add 2 NIL precommits from peer validators
        for i in 1..3 {
            let vote = create_test_vote(5, i, None, VoteType::Precommit); // NIL precommit
            let action = votes.add_vote_with_self_power(vote, Some(1));

            if i < 2 {
                assert_eq!(action, FutureRoundAction::NoAction);
            } else {
                // 2 peers + 1 self = 3 = threshold reached via precommit_power
                // We advance to Precommit step to participate
                assert!(matches!(action, FutureRoundAction::AdvanceToPrecommit { round: 5, .. }),
                    "Expected AdvanceToPrecommit for NIL precommits, got {:?}", action);
            }
        }
    }

    #[test]
    fn test_self_power_observer_node() {
        // Observer nodes (our_power = 0) should not affect thresholds
        let validator_set = create_test_validator_set(3);
        let mut votes = FutureRoundVotes::new(5, validator_set);
        let block_hash = test_hash(1);

        // Add 2 prevotes, but we're an observer (power = 0)
        for i in 1..3 {
            let vote = create_test_vote(5, i, Some(block_hash), VoteType::Prevote);
            let action = votes.add_vote_with_self_power(vote, Some(0));
            assert_eq!(action, FutureRoundAction::NoAction,
                "Observer nodes shouldn't reach threshold with only 2 votes");
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FUTURE HEIGHT PROPOSAL TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    fn create_test_proposal(height: Height, round: Round, proposer_idx: u8) -> Proposal {
        use crate::block::ConsensusBlock;
        use lighthouse_wrapper::types::MainnetEthSpec;

        Proposal {
            height,
            round,
            block: ConsensusBlock::<MainnetEthSpec>::default(),
            pol_round: None,
            proposer: ValidatorId::new(proposer_idx),
            signature: BLSSignature::empty(),
        }
    }

    #[test]
    fn test_store_future_height_proposal_basic() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Set current height to 10
        store.set_current_height(10);

        // Store a proposal for height 11, round 0
        let proposal = create_test_proposal(11, 0, 0);
        let stored = store.store_future_height_proposal(proposal);
        assert!(stored, "Should store future height proposal");
        assert_eq!(store.future_height_proposal_count(), 1);
    }

    #[test]
    fn test_store_future_height_proposal_rejects_current_height() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Set current height to 10
        store.set_current_height(10);

        // Try to store a proposal for current height (should be rejected)
        let proposal = create_test_proposal(10, 0, 0);
        let stored = store.store_future_height_proposal(proposal);
        assert!(!stored, "Should not store proposal for current height");
        assert_eq!(store.future_height_proposal_count(), 0);
    }

    #[test]
    fn test_store_future_height_proposal_rejects_past_height() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Set current height to 10
        store.set_current_height(10);

        // Try to store a proposal for past height (should be rejected)
        let proposal = create_test_proposal(5, 0, 0);
        let stored = store.store_future_height_proposal(proposal);
        assert!(!stored, "Should not store proposal for past height");
        assert_eq!(store.future_height_proposal_count(), 0);
    }

    #[test]
    fn test_store_future_height_proposal_rejects_too_far_future() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        // Set current height to 10, max_future_heights is 5
        store.set_current_height(10);

        // Height 16 is beyond limit (10 + 5 = 15)
        let proposal = create_test_proposal(16, 0, 0);
        let stored = store.store_future_height_proposal(proposal);
        assert!(!stored, "Should not store proposal too far in future");
        assert_eq!(store.future_height_proposal_count(), 0);

        // Height 15 is at the limit (should be accepted)
        let proposal = create_test_proposal(15, 0, 0);
        let stored = store.store_future_height_proposal(proposal);
        assert!(stored, "Should store proposal at max future height limit");
        assert_eq!(store.future_height_proposal_count(), 1);
    }

    #[test]
    fn test_store_future_height_proposal_one_per_height_round() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        store.set_current_height(10);

        // Store first proposal for (11, 0)
        let proposal1 = create_test_proposal(11, 0, 0);
        let stored1 = store.store_future_height_proposal(proposal1);
        assert!(stored1);

        // Try to store second proposal for same (11, 0) - should be rejected
        let proposal2 = create_test_proposal(11, 0, 1); // Different proposer
        let stored2 = store.store_future_height_proposal(proposal2);
        assert!(!stored2, "Should not store duplicate (height, round)");
        assert_eq!(store.future_height_proposal_count(), 1);

        // Different round (11, 1) should work
        let proposal3 = create_test_proposal(11, 1, 1);
        let stored3 = store.store_future_height_proposal(proposal3);
        assert!(stored3, "Should store proposal for different round");
        assert_eq!(store.future_height_proposal_count(), 2);
    }

    #[test]
    fn test_take_proposals_for_height() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        store.set_current_height(10);

        // Store proposals for height 11 at rounds 0, 2, 1 (out of order)
        store.store_future_height_proposal(create_test_proposal(11, 0, 0));
        store.store_future_height_proposal(create_test_proposal(11, 2, 2));
        store.store_future_height_proposal(create_test_proposal(11, 1, 1));

        // Also store one for height 12
        store.store_future_height_proposal(create_test_proposal(12, 0, 0));

        assert_eq!(store.future_height_proposal_count(), 4);

        // Take proposals for height 11
        let proposals = store.take_proposals_for_height(11);

        // Should have 3 proposals, sorted by round
        assert_eq!(proposals.len(), 3);
        assert_eq!(proposals[0].round, 0);
        assert_eq!(proposals[1].round, 1);
        assert_eq!(proposals[2].round, 2);

        // Should be removed from store
        assert_eq!(store.future_height_proposal_count(), 1);

        // Taking again should return empty
        let proposals2 = store.take_proposals_for_height(11);
        assert!(proposals2.is_empty());
    }

    #[test]
    fn test_set_current_height_cleans_up() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        store.set_current_height(10);

        // Store proposals for heights 11, 12, 13, 14, 15
        for h in 11..=15 {
            store.store_future_height_proposal(create_test_proposal(h, 0, 0));
        }
        assert_eq!(store.future_height_proposal_count(), 5);

        // Advance to height 13
        store.set_current_height(13);

        // Proposals for 11, 12, 13 should be cleaned up
        // Only 14, 15 should remain
        assert_eq!(store.future_height_proposal_count(), 2);
        assert!(!store.has_future_height_proposal(11, 0));
        assert!(!store.has_future_height_proposal(12, 0));
        assert!(!store.has_future_height_proposal(13, 0));
        assert!(store.has_future_height_proposal(14, 0));
        assert!(store.has_future_height_proposal(15, 0));
    }

    #[test]
    fn test_has_future_height_proposal() {
        let validator_set = create_test_validator_set(4);
        let mut store = FutureMessageStore::new(validator_set);

        store.set_current_height(10);

        // Initially no proposals
        assert!(!store.has_future_height_proposal(11, 0));

        // Store a proposal
        store.store_future_height_proposal(create_test_proposal(11, 0, 0));

        // Now it should exist
        assert!(store.has_future_height_proposal(11, 0));
        assert!(!store.has_future_height_proposal(11, 1)); // Different round
        assert!(!store.has_future_height_proposal(12, 0)); // Different height
    }
}
