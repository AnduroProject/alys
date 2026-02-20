//! Tendermint consensus state machine.
//!
//! This module implements the core Tendermint state machine that manages
//! consensus state transitions. It enforces the safety and liveness
//! properties of the protocol.
//!
//! # State Hierarchy
//!
//! ```text
//! TendermintState
//! ├── Current Position: (height, round, step)
//! ├── Locking State: (locked_round, locked_block)
//! ├── Vote Collections: (prevotes, precommits) per round
//! ├── Proposals: received proposals for current height
//! └── Validator Set: current validators and their powers
//! ```

use super::messages::Proposal;
use super::types::*;
use super::vote_set::VoteSet;
use crate::block::ConsensusBlock;
use lighthouse_wrapper::types::MainnetEthSpec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// The core Tendermint consensus state
///
/// This structure maintains all state needed for a single validator to
/// participate in Tendermint consensus. It is designed for async-safe
/// access using `Arc<RwLock<>>` for mutable vote sets.
///
/// # Thread Safety
///
/// The state uses `Arc<RwLock<>>` for vote collections to allow concurrent
/// read access while maintaining safety. Other fields are only modified
/// through controlled state transitions.
#[derive(Debug)]
pub struct TendermintState {
    // ═══════════════════════════════════════════════════════════════════
    // POSITION STATE - Where are we in consensus?
    // ═══════════════════════════════════════════════════════════════════
    /// Current block height being decided
    pub height: Height,

    /// Current round within this height (starts at 0)
    pub round: Round,

    /// Current step within this round
    pub step: TendermintStep,

    // ═══════════════════════════════════════════════════════════════════
    // LOCKING STATE - Critical for safety
    // ═══════════════════════════════════════════════════════════════════
    /// Round at which we became locked (if any)
    ///
    /// Once locked, we can only vote for the locked block unless we
    /// receive a valid Proof-of-Lock-Change from a higher round.
    pub locked_round: Option<Round>,

    /// Block hash we are locked on (if any)
    ///
    /// Safety invariant: if locked_round is Some, locked_block must also be Some
    pub locked_block: Option<BlockHash>,

    /// The actual block content we are locked on (Issue 2.1 fix)
    ///
    /// This is needed so that a locked proposer can re-propose the locked block.
    /// Without storing the block content, we would only have the hash and couldn't
    /// create a valid proposal.
    pub locked_block_data: Option<ConsensusBlock<MainnetEthSpec>>,

    /// The round at which we last saw 2/3+ prevotes (used for unlocking)
    pub valid_round: Option<Round>,

    /// The block hash that received 2/3+ prevotes
    pub valid_block: Option<BlockHash>,

    // ═══════════════════════════════════════════════════════════════════
    // VOTE COLLECTION - Track votes from all validators
    // ═══════════════════════════════════════════════════════════════════
    /// Prevotes for the current round
    ///
    /// Uses `Arc<RwLock<>>` for async-safe concurrent access
    pub prevotes: Arc<RwLock<VoteSet>>,

    /// Precommits for the current round
    pub precommits: Arc<RwLock<VoteSet>>,

    /// Historical vote sets from previous rounds (for POL verification)
    /// Key: round number
    pub historical_prevotes: HashMap<Round, Arc<RwLock<VoteSet>>>,

    // ═══════════════════════════════════════════════════════════════════
    // PROPOSAL TRACKING
    // ═══════════════════════════════════════════════════════════════════
    /// Proposal received for current round (if any)
    pub current_proposal: Option<Proposal>,

    /// All proposals seen at this height (for evidence)
    /// Key: (round, proposer)
    pub proposals: HashMap<(Round, ValidatorId), Proposal>,

    // ═══════════════════════════════════════════════════════════════════
    // VALIDATOR CONFIGURATION
    // ═══════════════════════════════════════════════════════════════════
    /// The current validator set
    pub validator_set: Arc<ValidatorSet>,

    /// Our validator ID (if we are a validator)
    pub our_validator_id: Option<ValidatorId>,

    // ═══════════════════════════════════════════════════════════════════
    // VOTE TRACKING - Prevent double voting
    // ═══════════════════════════════════════════════════════════════════
    /// Prevotes we've sent: (round) -> block_hash
    /// Used to prevent double voting and for WAL recovery
    pub sent_prevotes: HashMap<Round, Option<BlockHash>>,

    /// Precommits we've sent: (round) -> block_hash
    pub sent_precommits: HashMap<Round, Option<BlockHash>>,
}

impl TendermintState {
    /// Create a new state for the given height
    ///
    /// # Arguments
    ///
    /// * `height` - The block height to begin consensus for
    /// * `validator_set` - The current validator set
    /// * `our_validator_id` - Our validator ID (None if not a validator)
    pub fn new(
        height: Height,
        validator_set: Arc<ValidatorSet>,
        our_validator_id: Option<ValidatorId>,
    ) -> Self {
        let prevotes = Arc::new(RwLock::new(VoteSet::new(
            height,
            0,
            VoteType::Prevote,
            validator_set.clone(),
        )));

        let precommits = Arc::new(RwLock::new(VoteSet::new(
            height,
            0,
            VoteType::Precommit,
            validator_set.clone(),
        )));

        Self {
            height,
            round: 0,
            step: TendermintStep::Propose,

            locked_round: None,
            locked_block: None,
            locked_block_data: None, // Issue 2.1: Block data stored when locking
            valid_round: None,
            valid_block: None,

            prevotes,
            precommits,
            historical_prevotes: HashMap::new(),

            current_proposal: None,
            proposals: HashMap::new(),

            validator_set,
            our_validator_id,

            sent_prevotes: HashMap::new(),
            sent_precommits: HashMap::new(),
        }
    }

    /// Start a new round within the current height
    ///
    /// This is called when:
    /// - 2/3+ precommits for NIL (no consensus this round)
    /// - Precommit timeout expired with no majority
    ///
    /// Note: Locking state is preserved across rounds!
    pub fn new_round(&mut self, round: Round) {
        debug!(
            height = self.height,
            old_round = self.round,
            new_round = round,
            "Starting new round"
        );

        // Archive current round's prevotes for POL verification
        let current_prevotes = self.prevotes.clone();
        self.historical_prevotes.insert(self.round, current_prevotes);

        self.round = round;
        self.step = TendermintStep::Propose;

        // Create new vote sets for this round
        self.prevotes = Arc::new(RwLock::new(VoteSet::new(
            self.height,
            round,
            VoteType::Prevote,
            self.validator_set.clone(),
        )));

        self.precommits = Arc::new(RwLock::new(VoteSet::new(
            self.height,
            round,
            VoteType::Precommit,
            self.validator_set.clone(),
        )));

        self.current_proposal = None;

        // Note: locked_round and locked_block are NOT reset!
        // Locking persists across rounds until height changes
    }

    /// Advance to a new height after committing a block
    ///
    /// This completely resets the state for the new height.
    /// All locking state is cleared.
    pub fn new_height(&mut self, height: Height, validator_set: Arc<ValidatorSet>) {
        info!(
            old_height = self.height,
            new_height = height,
            "Advancing to new height"
        );

        self.height = height;
        self.round = 0;
        self.step = TendermintStep::Propose;

        // Clear locking state
        self.locked_round = None;
        self.locked_block = None;
        self.locked_block_data = None; // Issue 2.1: Clear block data at new height
        self.valid_round = None;
        self.valid_block = None;

        // Clear vote tracking
        self.sent_prevotes.clear();
        self.sent_precommits.clear();

        // Clear proposals
        self.current_proposal = None;
        self.proposals.clear();

        // Clear historical votes
        self.historical_prevotes.clear();

        // Update validator set and create new vote sets
        self.validator_set = validator_set;
        self.prevotes = Arc::new(RwLock::new(VoteSet::new(
            height,
            0,
            VoteType::Prevote,
            self.validator_set.clone(),
        )));
        self.precommits = Arc::new(RwLock::new(VoteSet::new(
            height,
            0,
            VoteType::Precommit,
            self.validator_set.clone(),
        )));
    }

    /// Set the step to a new value
    ///
    /// Only allows forward progression within a round.
    pub fn set_step(&mut self, step: TendermintStep) {
        if step as u8 > self.step as u8 {
            debug!(
                height = self.height,
                round = self.round,
                old_step = %self.step,
                new_step = %step,
                "Step transition"
            );
            self.step = step;
        }
    }

    /// Get the proposer for the current round
    pub fn current_proposer(&self) -> ValidatorId {
        self.validator_set.get_proposer(self.height, self.round)
    }

    /// Check if we are the proposer for the current round
    pub fn is_proposer(&self) -> bool {
        match self.our_validator_id {
            Some(id) => id == self.current_proposer(),
            None => false,
        }
    }

    /// Check if we are a validator
    pub fn is_validator(&self) -> bool {
        self.our_validator_id.is_some()
    }

    /// Check if we have already voted prevote in the current round
    pub fn has_voted_prevote(&self) -> bool {
        self.sent_prevotes.contains_key(&self.round)
    }

    /// Check if we have already voted precommit in the current round
    pub fn has_voted_precommit(&self) -> bool {
        self.sent_precommits.contains_key(&self.round)
    }

    /// Record that we sent a prevote
    pub fn record_prevote(&mut self, block_hash: Option<BlockHash>) {
        self.sent_prevotes.insert(self.round, block_hash);
    }

    /// Record that we sent a precommit
    pub fn record_precommit(&mut self, block_hash: Option<BlockHash>) {
        self.sent_precommits.insert(self.round, block_hash);
    }

    /// Lock on a block after seeing 2/3+ prevotes
    ///
    /// This is the critical locking operation that ensures safety.
    /// Lock on a block hash (without storing block content)
    ///
    /// Note: For locked proposers to re-propose, use `lock_on_with_block` instead.
    pub fn lock_on(&mut self, round: Round, block_hash: BlockHash) {
        self.lock_on_with_block(round, block_hash, None);
    }

    /// Lock on a block with full block content (Issue 2.1 fix)
    ///
    /// Stores both the block hash and the full block content, allowing
    /// a locked proposer to re-propose the locked block in later rounds.
    pub fn lock_on_with_block(
        &mut self,
        round: Round,
        block_hash: BlockHash,
        block_data: Option<ConsensusBlock<MainnetEthSpec>>,
    ) {
        info!(
            height = self.height,
            round = round,
            block_hash = %block_hash,
            has_block_data = block_data.is_some(),
            "Locking on block"
        );
        self.locked_round = Some(round);
        self.locked_block = Some(block_hash);
        self.locked_block_data = block_data;
    }

    /// Update valid block after seeing 2/3+ prevotes
    ///
    /// Valid block is used for unlocking and proposal validity.
    pub fn set_valid(&mut self, round: Round, block_hash: BlockHash) {
        self.valid_round = Some(round);
        self.valid_block = Some(block_hash);
    }

    /// Check if we should unlock based on a Proof-of-Lock-Change
    ///
    /// We can unlock if we see 2/3+ prevotes for a different block
    /// in a round higher than our locked round.
    /// Check if we can unlock based on a Proof-of-Lock-Change claim
    ///
    /// Issue 2.4 Fix: Changed from `>` to `>=` per Tendermint paper.
    /// A POL from the same round as the lock is valid for unlocking.
    pub fn can_unlock(&self, pol_round: Round, _pol_block: BlockHash) -> bool {
        match self.locked_round {
            None => true, // Not locked, no unlock needed
            Some(locked_round) => pol_round >= locked_round, // Issue 2.4: Use >= not >
        }
    }

    /// Clear the lock (after successful unlock verification)
    pub fn unlock(&mut self) {
        debug!(
            height = self.height,
            previous_locked_round = ?self.locked_round,
            previous_locked_block = ?self.locked_block,
            "Unlocking"
        );
        self.locked_round = None;
        self.locked_block = None;
    }

    /// Get a summary of current state for logging/debugging
    pub fn summary(&self) -> StateSummary {
        StateSummary {
            height: self.height,
            round: self.round,
            step: self.step,
            locked: self.locked_block.is_some(),
            proposer: self.current_proposer(),
            is_us_proposer: self.is_proposer(),
        }
    }

    /// Determine what block hash to prevote for based on locking rules
    ///
    /// # Locking Rules (Critical for Safety)
    ///
    /// 1. If not locked: vote for the proposed block
    /// 2. If locked on the proposed block: vote for it
    /// 3. If locked on different block but proposal has valid POL: can vote for proposal
    /// 4. If locked on different block and no valid POL: vote NIL or locked block
    ///
    /// # POL Verification (Issue 1.3 Fix)
    ///
    /// When a proposal claims a POL (pol_round), we verify it against our
    /// historical_prevotes. This prevents malicious proposers from lying
    /// about having a POL to unlock validators.
    pub fn determine_prevote_target(&self, proposal: &Proposal) -> Option<BlockHash> {
        let proposal_hash = proposal.block_hash();

        match (&self.locked_block, &self.locked_round) {
            // Not locked - free to vote for the proposal
            (None, _) => Some(proposal_hash),

            // Locked on this exact block - vote for it
            (Some(locked), _) if *locked == proposal_hash => Some(proposal_hash),

            // Locked on different block - check POL for unlock
            (Some(_locked), Some(locked_round)) => {
                match proposal.pol_round {
                    // Proposal claims POL from round >= our lock
                    Some(pol_round) if pol_round >= *locked_round => {
                        // Issue 1.3 FIX: Verify POL against our historical prevotes
                        // instead of blindly trusting the proposer's claim
                        if self.verify_pol_claim(pol_round, &proposal_hash) {
                            debug!(
                                pol_round = pol_round,
                                proposal_hash = %proposal_hash,
                                locked_round = *locked_round,
                                "POL verified from historical prevotes - unlocking"
                            );
                            Some(proposal_hash)
                        } else {
                            // Proposer claims POL but our records don't support it
                            // Stay locked for safety
                            debug!(
                                pol_round = pol_round,
                                proposal_hash = %proposal_hash,
                                locked_round = *locked_round,
                                "POL claimed but not verified - staying locked"
                            );
                            None
                        }
                    }
                    // No valid POL - vote NIL (cannot vote for conflicting block)
                    _ => None,
                }
            }

            // Locked but no locked_round (shouldn't happen)
            (Some(locked), None) => Some(*locked),
        }
    }

    /// Verify a POL claim against our historical prevotes.
    ///
    /// Returns true if we have evidence of 2/3+ prevotes for the given block
    /// at the specified round.
    ///
    /// # Safety Trade-off
    ///
    /// If we were offline during pol_round, we won't have the historical data
    /// and will return false. This keeps us locked (safe but potentially slower
    /// progress). This is the correct behavior - staying locked is always safe.
    fn verify_pol_claim(&self, pol_round: Round, block_hash: &BlockHash) -> bool {
        // Check if we have archived prevotes from pol_round
        if let Some(archived_votes) = self.historical_prevotes.get(&pol_round) {
            // Use try_read to avoid blocking - if lock is contended, stay safe
            if let Ok(votes) = archived_votes.try_read() {
                return votes.has_two_thirds_for(Some(block_hash));
            }
        }

        // No historical data for this round (may have been offline)
        // or lock contention - stay locked for safety
        false
    }
}

/// Summary of state for logging
#[derive(Debug, Clone)]
pub struct StateSummary {
    pub height: Height,
    pub round: Round,
    pub step: TendermintStep,
    pub locked: bool,
    pub proposer: ValidatorId,
    pub is_us_proposer: bool,
}

impl std::fmt::Display for StateSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "H={} R={} Step={} Locked={} Proposer={} (us={})",
            self.height, self.round, self.step, self.locked, self.proposer, self.is_us_proposer
        )
    }
}

/// Events that can trigger state transitions
#[derive(Debug, Clone)]
pub enum ConsensusEvent {
    /// New proposal received from proposer
    ProposalReceived(Proposal),

    /// Vote received from a validator
    VoteReceived(super::messages::Vote),

    /// Timeout expired for current step
    Timeout(TendermintStep),

    /// 2/3+ prevotes collected for a block
    TwoThirdsPrevotes(Option<BlockHash>),

    /// 2/3+ precommits collected for a block
    TwoThirdsPrecommits(Option<BlockHash>),
}

/// Actions to take after processing an event
#[derive(Debug, Clone)]
pub enum ConsensusAction {
    /// Do nothing
    None,

    /// Broadcast our prevote
    BroadcastPrevote(Option<BlockHash>),

    /// Broadcast our precommit
    BroadcastPrecommit(Option<BlockHash>),

    /// Commit the block (consensus reached!)
    CommitBlock(BlockHash),

    /// Move to next round
    NewRound(Round),

    /// Schedule a timeout
    ScheduleTimeout(TendermintStep),
}

#[cfg(test)]
mod tests {
    use super::*;
    use lighthouse_wrapper::bls::PublicKey;
    use std::str::FromStr;

    fn create_mock_pubkey() -> PublicKey {
        // Use a known valid BLS public key for testing
        PublicKey::from_str(
            "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
        ).expect("valid test public key")
    }

    fn create_test_validator_set(count: usize) -> Arc<ValidatorSet> {
        let validators = vec![create_mock_pubkey(); count];
        Arc::new(ValidatorSet::with_equal_power(validators))
    }

    #[test]
    fn test_new_round_preserves_lock() {
        let validator_set = create_test_validator_set(4);
        let mut state = TendermintState::new(100, validator_set, Some(ValidatorId(0)));

        // Lock on a block
        let block_hash = BlockHash::repeat_byte(0xAB);
        state.lock_on(0, block_hash);

        // Advance to round 1
        state.new_round(1);

        // Lock should be preserved
        assert_eq!(state.round, 1);
        assert_eq!(state.step, TendermintStep::Propose);
        assert_eq!(state.locked_round, Some(0));
        assert_eq!(state.locked_block, Some(block_hash));
    }

    #[test]
    fn test_new_height_clears_lock() {
        let validator_set = create_test_validator_set(4);
        let mut state = TendermintState::new(100, validator_set.clone(), Some(ValidatorId(0)));

        // Lock on a block
        state.lock_on(0, BlockHash::repeat_byte(0xAB));

        // Advance to new height
        state.new_height(101, validator_set);

        // Lock should be cleared
        assert_eq!(state.height, 101);
        assert_eq!(state.round, 0);
        assert!(state.locked_round.is_none());
        assert!(state.locked_block.is_none());
    }

    #[test]
    fn test_is_proposer() {
        let validator_set = create_test_validator_set(4);

        // Validator 0 at height 0, round 0 should be proposer
        let state = TendermintState::new(0, validator_set.clone(), Some(ValidatorId(0)));
        assert!(state.is_proposer());

        // Validator 1 at height 0, round 0 should not be proposer
        let state = TendermintState::new(0, validator_set.clone(), Some(ValidatorId(1)));
        assert!(!state.is_proposer());

        // Validator 1 at height 0, round 1 should be proposer
        let mut state = TendermintState::new(0, validator_set, Some(ValidatorId(1)));
        state.new_round(1);
        assert!(state.is_proposer());
    }

    #[test]
    fn test_vote_tracking() {
        let validator_set = create_test_validator_set(4);
        let mut state = TendermintState::new(100, validator_set, Some(ValidatorId(0)));

        assert!(!state.has_voted_prevote());
        assert!(!state.has_voted_precommit());

        state.record_prevote(Some(BlockHash::zero()));
        assert!(state.has_voted_prevote());
        assert!(!state.has_voted_precommit());

        state.record_precommit(Some(BlockHash::zero()));
        assert!(state.has_voted_precommit());
    }

    #[test]
    fn test_can_unlock() {
        let validator_set = create_test_validator_set(4);
        let mut state = TendermintState::new(100, validator_set, Some(ValidatorId(0)));

        // Not locked - can always "unlock"
        assert!(state.can_unlock(0, BlockHash::zero()));

        // Lock at round 0
        state.lock_on(0, BlockHash::repeat_byte(0xAB));

        // POL from round 1 should allow unlock
        assert!(state.can_unlock(1, BlockHash::repeat_byte(0xCD)));

        // POL from round 0 should not allow unlock
        assert!(!state.can_unlock(0, BlockHash::repeat_byte(0xCD)));
    }

    #[test]
    fn test_state_summary() {
        let validator_set = create_test_validator_set(4);
        let state = TendermintState::new(100, validator_set, Some(ValidatorId(0)));

        let summary = state.summary();
        assert_eq!(summary.height, 100);
        assert_eq!(summary.round, 0);
        assert_eq!(summary.step, TendermintStep::Propose);
        assert!(!summary.locked);
        assert!(summary.is_us_proposer);
    }
}
