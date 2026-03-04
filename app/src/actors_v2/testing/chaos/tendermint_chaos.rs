//! Tendermint Consensus Chaos Testing Framework
//!
//! This module provides chaos injection and testing utilities specific to
//! Tendermint BFT consensus. It enables testing of:
//!
//! - Vote manipulation (delays, drops, corruption)
//! - Proposal manipulation
//! - Equivocation scenarios
//! - Locking and unlocking rules
//! - Timeout behavior
//! - Evidence generation
//!
//! # Architecture
//!
//! The framework uses mock validators and a mock network to test consensus
//! logic in isolation without requiring real network I/O.
//!
//! ```text
//! TendermintTestHarness
//! ├── MockValidator[] - Simulated validators with controlled behavior
//! ├── MockNetwork - Message routing with chaos injection
//! └── TendermintChaosInjector - Chaos scenario controller
//! ```

use crate::actors_v2::chain::tendermint::{
    BlockHash, EquivocationEvidence, EquivocationType, Height, Proposal, Round,
    TendermintStep, ValidatorId, Vote, VoteType,
};
use ethereum_types::H256;
use lighthouse_wrapper::bls::Signature as BLSSignature;
use std::collections::HashMap;
use std::time::Duration;

/// Tendermint consensus chaos scenarios
#[derive(Debug, Clone)]
pub enum TendermintChaosScenario {
    // ═══════════════════════════════════════════════════════════════════════
    // VOTE & PROPOSAL MANIPULATION
    // ═══════════════════════════════════════════════════════════════════════
    /// Drop a percentage of votes (test vote collection resilience)
    VoteLoss { drop_rate: f64 },

    /// Delay vote propagation (test timeout thresholds)
    VoteDelay { delay_ms: u64 },

    /// Delay proposal propagation (test NIL voting on timeout)
    ProposalDelay { delay_ms: u64 },

    /// Drop proposals entirely (test round advancement)
    DropProposals { drop_rate: f64 },

    /// Corrupt vote signatures (should be rejected, not counted)
    CorruptVotes { corruption_rate: f64 },

    // ═══════════════════════════════════════════════════════════════════════
    // CONSENSUS STATE MANIPULATION (for testing safety)
    // ═══════════════════════════════════════════════════════════════════════
    /// Simulate equivocating validator (test evidence generation)
    InjectEquivocation { validator_id: u8 },

    /// Force timeout on specific step (test round progression)
    ForceTimeout { step: TendermintStep },

    /// Inject conflicting POL claims (test POL validation)
    InvalidPOL { claimed_round: u32 },

    /// Send votes for future rounds (test future message buffering)
    FutureRoundVotes { rounds_ahead: u32 },

    /// Send votes for past rounds (should be ignored)
    PastRoundVotes { rounds_behind: u32 },

    // ═══════════════════════════════════════════════════════════════════════
    // LOCKING SCENARIOS
    // ═══════════════════════════════════════════════════════════════════════
    /// Prevent lock by dropping votes (test lock threshold)
    PreventLock { target_block: H256 },

    /// Inject conflicting prevotes to split vote (test no-majority behavior)
    SplitPrevotes,

    // ═══════════════════════════════════════════════════════════════════════
    // EXTERNAL DEPENDENCIES
    // ═══════════════════════════════════════════════════════════════════════
    /// Simulate slow execution layer (test EL timeout handling)
    SlowExecution { delay_ms: u64 },

    /// Simulate execution layer returning invalid payload
    InvalidExecutionPayload,
}

/// Controlled voting behavior for mock validators
#[derive(Debug, Clone, Default)]
pub enum VoteBehavior {
    /// Vote honestly according to protocol
    #[default]
    Honest,
    /// Always vote NIL
    AlwaysNil,
    /// Equivocate (send conflicting votes)
    Equivocate,
    /// Vote with delay
    Delayed { delay_ms: u64 },
    /// Don't vote (simulate crash)
    Silent,
    /// Vote for random block (byzantine)
    Byzantine,
}

/// Mock validator for testing
#[derive(Debug)]
pub struct MockValidator {
    /// Validator ID
    pub id: ValidatorId,
    /// Controlled voting behavior
    pub vote_behavior: VoteBehavior,
    /// Whether this validator is online
    pub is_online: bool,
    /// Network latency for this validator (ms)
    pub latency_ms: u64,
    /// Votes cast by this validator
    pub votes_cast: Vec<Vote>,
}

impl MockValidator {
    /// Create a new honest mock validator
    pub fn new(id: ValidatorId) -> Self {
        Self {
            id,
            vote_behavior: VoteBehavior::Honest,
            is_online: true,
            latency_ms: 0,
            votes_cast: Vec::new(),
        }
    }

    /// Create a validator with specific behavior
    pub fn with_behavior(id: ValidatorId, behavior: VoteBehavior) -> Self {
        Self {
            id,
            vote_behavior: behavior,
            is_online: true,
            latency_ms: 0,
            votes_cast: Vec::new(),
        }
    }

    /// Set the validator offline
    pub fn set_offline(&mut self) {
        self.is_online = false;
    }

    /// Set the validator online
    pub fn set_online(&mut self) {
        self.is_online = true;
    }

    /// Set network latency
    pub fn set_latency(&mut self, latency_ms: u64) {
        self.latency_ms = latency_ms;
    }
}

/// Chaos injector for Tendermint consensus
#[derive(Debug, Default)]
pub struct TendermintChaosInjector {
    /// Active chaos scenarios
    active_scenarios: Vec<TendermintChaosScenario>,
    /// Injection statistics
    pub stats: ChaosInjectionStats,
}

/// Statistics for chaos injection
#[derive(Debug, Default, Clone)]
pub struct ChaosInjectionStats {
    pub votes_dropped: u64,
    pub votes_delayed: u64,
    pub proposals_dropped: u64,
    pub proposals_delayed: u64,
    pub votes_corrupted: u64,
    pub equivocations_injected: u64,
    pub timeouts_forced: u64,
}

impl TendermintChaosInjector {
    /// Create a new chaos injector
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a chaos scenario
    pub fn add_scenario(&mut self, scenario: TendermintChaosScenario) {
        self.active_scenarios.push(scenario);
    }

    /// Clear all scenarios
    pub fn clear_scenarios(&mut self) {
        self.active_scenarios.clear();
    }

    /// Check if a vote should be dropped
    pub fn should_drop_vote(&mut self, _vote: &Vote) -> bool {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::VoteLoss { drop_rate } = scenario {
                if rand::random::<f64>() < *drop_rate {
                    self.stats.votes_dropped += 1;
                    return true;
                }
            }
        }
        false
    }

    /// Get vote delay if applicable
    pub fn get_vote_delay(&self) -> Option<Duration> {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::VoteDelay { delay_ms } = scenario {
                return Some(Duration::from_millis(*delay_ms));
            }
        }
        None
    }

    /// Check if a proposal should be dropped
    pub fn should_drop_proposal(&mut self, _proposal: &Proposal) -> bool {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::DropProposals { drop_rate } = scenario {
                if rand::random::<f64>() < *drop_rate {
                    self.stats.proposals_dropped += 1;
                    return true;
                }
            }
        }
        false
    }

    /// Get proposal delay if applicable
    pub fn get_proposal_delay(&self) -> Option<Duration> {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::ProposalDelay { delay_ms } = scenario {
                return Some(Duration::from_millis(*delay_ms));
            }
        }
        None
    }

    /// Check if a vote should be corrupted
    pub fn should_corrupt_vote(&mut self, _vote: &Vote) -> bool {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::CorruptVotes { corruption_rate } = scenario {
                if rand::random::<f64>() < *corruption_rate {
                    self.stats.votes_corrupted += 1;
                    return true;
                }
            }
        }
        false
    }

    /// Get the validator ID that should equivocate
    pub fn get_equivocating_validator(&self) -> Option<u8> {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::InjectEquivocation { validator_id } = scenario {
                return Some(*validator_id);
            }
        }
        None
    }

    /// Check if timeout should be forced for a step
    pub fn should_force_timeout(&self, step: TendermintStep) -> bool {
        for scenario in &self.active_scenarios {
            if let TendermintChaosScenario::ForceTimeout { step: target_step } = scenario {
                if *target_step == step {
                    return true;
                }
            }
        }
        false
    }
}

/// Test harness for Tendermint consensus chaos testing
pub struct TendermintTestHarness {
    /// Mock validators with controlled behavior
    pub validators: Vec<MockValidator>,
    /// Number of validators
    pub validator_count: usize,
    /// Chaos injector
    pub chaos: TendermintChaosInjector,
    /// Generated evidence during tests
    pub evidence: Vec<EquivocationEvidence>,
    /// Committed blocks: height -> block_hash
    pub committed_blocks: HashMap<Height, BlockHash>,
}

impl TendermintTestHarness {
    /// Create harness with n validators
    pub fn new(num_validators: usize) -> Self {
        let validators = (0..num_validators)
            .map(|i| MockValidator::new(ValidatorId::new(i as u8)))
            .collect();

        Self {
            validators,
            validator_count: num_validators,
            chaos: TendermintChaosInjector::new(),
            evidence: Vec::new(),
            committed_blocks: HashMap::new(),
        }
    }

    /// Inject chaos scenario
    pub fn inject_chaos(&mut self, scenario: TendermintChaosScenario) {
        self.chaos.add_scenario(scenario);
    }

    /// Clear all chaos scenarios
    pub fn clear_chaos(&mut self) {
        self.chaos.clear_scenarios();
    }

    /// Set validator behavior
    pub fn set_validator_behavior(&mut self, validator_idx: usize, behavior: VoteBehavior) {
        if validator_idx < self.validators.len() {
            self.validators[validator_idx].vote_behavior = behavior;
        }
    }

    /// Set validator offline
    pub fn set_validator_offline(&mut self, validator_idx: usize) {
        if validator_idx < self.validators.len() {
            self.validators[validator_idx].set_offline();
        }
    }

    /// Set validator online
    pub fn set_validator_online(&mut self, validator_idx: usize) {
        if validator_idx < self.validators.len() {
            self.validators[validator_idx].set_online();
        }
    }

    /// Get count of online validators
    pub fn online_validator_count(&self) -> usize {
        self.validators.iter().filter(|v| v.is_online).count()
    }

    /// Calculate quorum size (>2/3)
    pub fn quorum_size(&self) -> usize {
        (self.validator_count * 2 / 3) + 1
    }

    /// Check if we have quorum
    pub fn has_quorum(&self) -> bool {
        self.online_validator_count() >= self.quorum_size()
    }

    /// Record evidence
    pub fn add_evidence(&mut self, evidence: EquivocationEvidence) {
        self.evidence.push(evidence);
    }

    /// Check if any evidence was generated
    pub fn has_evidence(&self) -> bool {
        !self.evidence.is_empty()
    }

    /// Get evidence count
    pub fn evidence_count(&self) -> usize {
        self.evidence.len()
    }

    /// Assert no equivocation evidence generated
    pub fn assert_no_evidence(&self) {
        assert!(
            self.evidence.is_empty(),
            "Expected no evidence, but found {} instances",
            self.evidence.len()
        );
    }

    /// Assert evidence exists for specific validator
    pub fn assert_evidence_for(&self, validator_id: ValidatorId) {
        assert!(
            self.evidence.iter().any(|e| e.culprit == validator_id),
            "Expected evidence for validator {:?}, but none found",
            validator_id
        );
    }

    /// Record a committed block
    pub fn record_commit(&mut self, height: Height, block_hash: BlockHash) {
        self.committed_blocks.insert(height, block_hash);
    }

    /// Check if height was committed
    pub fn is_committed(&self, height: Height) -> bool {
        self.committed_blocks.contains_key(&height)
    }

    /// Get committed block hash for height
    pub fn get_committed_hash(&self, height: Height) -> Option<&BlockHash> {
        self.committed_blocks.get(&height)
    }

    /// Simulate vote collection with chaos injection
    pub fn simulate_votes(
        &mut self,
        height: Height,
        round: Round,
        vote_type: VoteType,
        block_hash: Option<BlockHash>,
    ) -> Vec<Vote> {
        let mut votes = Vec::new();

        for validator in &self.validators {
            if !validator.is_online {
                continue;
            }

            // Get current timestamp
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // Check behavior
            match &validator.vote_behavior {
                VoteBehavior::Silent => continue,
                VoteBehavior::AlwaysNil => {
                    let vote = Vote {
                        height,
                        round,
                        vote_type,
                        block_hash: None,
                        validator: validator.id,
                        timestamp,
                        signature: BLSSignature::empty(), // Mock signature
                    };
                    if !self.chaos.should_drop_vote(&vote) {
                        votes.push(vote);
                    }
                }
                VoteBehavior::Honest => {
                    let vote = Vote {
                        height,
                        round,
                        vote_type,
                        block_hash,
                        validator: validator.id,
                        timestamp,
                        signature: BLSSignature::empty(),
                    };
                    if !self.chaos.should_drop_vote(&vote) {
                        votes.push(vote);
                    }
                }
                VoteBehavior::Byzantine => {
                    // Vote for random block
                    let random_hash = H256::random();
                    let vote = Vote {
                        height,
                        round,
                        vote_type,
                        block_hash: Some(random_hash),
                        validator: validator.id,
                        timestamp,
                        signature: BLSSignature::empty(),
                    };
                    votes.push(vote);
                }
                VoteBehavior::Equivocate => {
                    // Cast two conflicting votes - generates evidence
                    let vote1 = Vote {
                        height,
                        round,
                        vote_type,
                        block_hash,
                        validator: validator.id,
                        timestamp,
                        signature: BLSSignature::empty(),
                    };
                    let vote2 = Vote {
                        height,
                        round,
                        vote_type,
                        block_hash: Some(H256::random()),
                        validator: validator.id,
                        timestamp,
                        signature: BLSSignature::empty(),
                    };
                    votes.push(vote1.clone());
                    votes.push(vote2.clone());

                    // Record evidence
                    let evidence = EquivocationEvidence {
                        kind: if vote_type == VoteType::Prevote {
                            EquivocationType::DoublePrevote
                        } else {
                            EquivocationType::DoublePrecommit
                        },
                        culprit: validator.id,
                        height,
                        round,
                        vote_a: vote1,
                        vote_b: vote2,
                    };
                    self.evidence.push(evidence);
                }
                VoteBehavior::Delayed { delay_ms: _ } => {
                    // For testing purposes, delayed votes still arrive
                    let vote = Vote {
                        height,
                        round,
                        vote_type,
                        block_hash,
                        validator: validator.id,
                        timestamp,
                        signature: BLSSignature::empty(),
                    };
                    votes.push(vote);
                }
            }
        }

        votes
    }

    /// Check if votes meet quorum threshold
    pub fn has_vote_quorum(&self, votes: &[Vote], target_hash: Option<BlockHash>) -> bool {
        let matching_votes = votes
            .iter()
            .filter(|v| v.block_hash == target_hash)
            .count();
        matching_votes >= self.quorum_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_creation() {
        let harness = TendermintTestHarness::new(3);
        assert_eq!(harness.validator_count, 3);
        assert_eq!(harness.validators.len(), 3);
        assert_eq!(harness.quorum_size(), 3); // 2/3 of 3 + 1 = 3
    }

    #[test]
    fn test_quorum_calculations() {
        // n=3: quorum = 3 (100%)
        let h3 = TendermintTestHarness::new(3);
        assert_eq!(h3.quorum_size(), 3);

        // n=4: quorum = 3 (75%)
        let h4 = TendermintTestHarness::new(4);
        assert_eq!(h4.quorum_size(), 3);

        // n=5: quorum = 4 (80%)
        let h5 = TendermintTestHarness::new(5);
        assert_eq!(h5.quorum_size(), 4);
    }

    #[test]
    fn test_validator_offline() {
        let mut harness = TendermintTestHarness::new(3);
        assert!(harness.has_quorum());

        harness.set_validator_offline(0);
        assert!(!harness.has_quorum()); // n=3 requires all 3
    }

    #[test]
    fn test_chaos_injection() {
        let mut harness = TendermintTestHarness::new(3);
        harness.inject_chaos(TendermintChaosScenario::VoteDelay { delay_ms: 500 });

        assert!(harness.chaos.get_vote_delay().is_some());
        assert_eq!(harness.chaos.get_vote_delay().unwrap().as_millis(), 500);
    }

    #[test]
    fn test_honest_voting() {
        let mut harness = TendermintTestHarness::new(3);
        let block_hash = H256::from_slice(&[1u8; 32]);

        let votes =
            harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

        assert_eq!(votes.len(), 3);
        assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
    }

    #[test]
    fn test_equivocation_detection() {
        let mut harness = TendermintTestHarness::new(3);
        harness.set_validator_behavior(1, VoteBehavior::Equivocate);

        let block_hash = H256::from_slice(&[1u8; 32]);
        let _votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

        assert!(harness.has_evidence());
        assert_eq!(harness.evidence_count(), 1);
    }

    #[test]
    fn test_silent_validator_no_vote() {
        let mut harness = TendermintTestHarness::new(3);
        harness.set_validator_behavior(0, VoteBehavior::Silent);

        let block_hash = H256::from_slice(&[1u8; 32]);
        let votes =
            harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

        assert_eq!(votes.len(), 2); // Only 2 validators voted
        assert!(!harness.has_vote_quorum(&votes, Some(block_hash))); // Need 3 for n=3
    }

    #[test]
    fn test_nil_voting() {
        let mut harness = TendermintTestHarness::new(3);
        harness.set_validator_behavior(0, VoteBehavior::AlwaysNil);
        harness.set_validator_behavior(1, VoteBehavior::AlwaysNil);
        harness.set_validator_behavior(2, VoteBehavior::AlwaysNil);

        let block_hash = H256::from_slice(&[1u8; 32]);
        let votes =
            harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

        // All votes should be NIL
        assert!(votes.iter().all(|v| v.block_hash.is_none()));
        // Should have quorum for NIL
        assert!(harness.has_vote_quorum(&votes, None));
    }
}
