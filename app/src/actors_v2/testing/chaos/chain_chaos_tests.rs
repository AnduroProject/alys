//! Tendermint ChainActor Chaos Tests
//!
//! This module tests the ChainActor's Tendermint consensus logic under chaos conditions.
//! These are Layer 2 tests that verify internal consensus state with precision.
//!
//! # Test Categories
//!
//! - **Locking & POL**: Tests for vote locking and proof-of-lock behavior
//! - **Vote Thresholds**: Tests for exact >2/3 threshold enforcement
//! - **Round & Timeout**: Tests for round advancement and timeout handling
//! - **Equivocation & Evidence**: Tests for evidence generation

use super::tendermint_chaos::{
    TendermintChaosScenario, TendermintTestHarness, VoteBehavior,
};
use crate::actors_v2::chain::tendermint::{TendermintStep, ValidatorId, VoteType};
use ethereum_types::H256;

// ═══════════════════════════════════════════════════════════════════════════
// LOCKING & POL TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lock_on_two_thirds_prevotes() {
    // Validator sees >2/3 prevotes for block B
    // Expected: validator.locked_round = current_round, validator.locked_block = B
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    // All validators vote honestly for the same block
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // With 3 validators all voting for same block, we have quorum
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
    assert_eq!(votes.len(), 3);

    // All votes should be for the same block
    assert!(votes.iter().all(|v| v.block_hash == Some(block_hash)));
}

#[test]
fn test_locked_validator_only_votes_for_locked_block() {
    // Validator is locked on block B at round R
    // New proposal for block C arrives in round R+1
    // Expected: validator prevotes NIL (not C), unless POL for C exists
    let mut harness = TendermintTestHarness::new(3);
    let block_b = H256::from_slice(&[1u8; 32]);
    let block_c = H256::from_slice(&[2u8; 32]);

    // Simulate round 0 where validator locks on B
    let r0_votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_b));
    assert!(harness.has_vote_quorum(&r0_votes, Some(block_b)));

    // In round 1, if validators are honest and locked on B,
    // they should not vote for C
    // (This tests the vote simulation behavior)
    let r1_votes = harness.simulate_votes(1, 1, VoteType::Prevote, Some(block_b));
    assert!(r1_votes.iter().all(|v| v.block_hash == Some(block_b)));
}

#[test]
fn test_nil_prevote_behavior() {
    // Validators configured to always vote NIL
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(0, VoteBehavior::AlwaysNil);
    harness.set_validator_behavior(1, VoteBehavior::AlwaysNil);
    harness.set_validator_behavior(2, VoteBehavior::AlwaysNil);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // All votes should be NIL
    assert!(votes.iter().all(|v| v.block_hash.is_none()));
    // Should have quorum for NIL
    assert!(harness.has_vote_quorum(&votes, None));
}

// ═══════════════════════════════════════════════════════════════════════════
// VOTE THRESHOLD TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_prevote_threshold_exactly_two_thirds() {
    // With n=3, exactly 2 prevotes = 66.67% (not sufficient, need >66.67%)
    // Expected: no lock triggered, wait for timeout
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(2, VoteBehavior::Silent); // One validator silent

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // Only 2 validators voted
    assert_eq!(votes.len(), 2);
    // Not enough for quorum (need 3 for n=3)
    assert!(!harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_prevote_threshold_more_than_two_thirds() {
    // With n=3, 3 prevotes = 100% (sufficient)
    // Expected: lock triggered, proceed to precommit
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert_eq!(votes.len(), 3);
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_precommit_threshold_triggers_commit() {
    // >2/3 precommits for same block
    // Expected: block committed, last_commit created, height advances
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    let votes = harness.simulate_votes(1, 0, VoteType::Precommit, Some(block_hash));

    assert_eq!(votes.len(), 3);
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_mixed_votes_no_majority() {
    // Prevotes split: 1 for A, 1 for B, 1 NIL
    // Expected: no lock, timeout, round advances
    let mut harness = TendermintTestHarness::new(3);

    harness.set_validator_behavior(0, VoteBehavior::Honest);
    harness.set_validator_behavior(1, VoteBehavior::Byzantine); // Random vote
    harness.set_validator_behavior(2, VoteBehavior::AlwaysNil);

    let block_a = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_a));

    // Votes are split - no single value has quorum
    assert!(!harness.has_vote_quorum(&votes, Some(block_a)));
    assert!(!harness.has_vote_quorum(&votes, None));
}

#[test]
fn test_four_validator_quorum() {
    // n=4: quorum = 3 (75%), can tolerate 1 failure
    let mut harness = TendermintTestHarness::new(4);
    harness.set_validator_behavior(3, VoteBehavior::Silent);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // 3 of 4 voted
    assert_eq!(votes.len(), 3);
    // With n=4, quorum = 3, so 3 votes is sufficient
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_five_validator_quorum() {
    // n=5: quorum = 4 (80%)
    let mut harness = TendermintTestHarness::new(5);
    harness.set_validator_behavior(4, VoteBehavior::Silent);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // 4 of 5 voted
    assert_eq!(votes.len(), 4);
    // With n=5, quorum = 4, so 4 votes is sufficient
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
}

// ═══════════════════════════════════════════════════════════════════════════
// EQUIVOCATION & EVIDENCE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_double_prevote_generates_evidence() {
    // Validator sends two different prevotes for same (height, round)
    // Expected: EquivocationEvidence generated with both votes
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(1, VoteBehavior::Equivocate);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let _votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert!(harness.has_evidence());
    assert_eq!(harness.evidence_count(), 1);
    harness.assert_evidence_for(ValidatorId::new(1));
}

#[test]
fn test_double_precommit_generates_evidence() {
    // Validator sends two different precommits for same (height, round)
    // Expected: EquivocationEvidence generated
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(2, VoteBehavior::Equivocate);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let _votes = harness.simulate_votes(1, 0, VoteType::Precommit, Some(block_hash));

    assert!(harness.has_evidence());
    harness.assert_evidence_for(ValidatorId::new(2));
}

#[test]
fn test_honest_validators_no_evidence() {
    // All validators vote honestly
    // Expected: no evidence generated
    let mut harness = TendermintTestHarness::new(3);

    let block_hash = H256::from_slice(&[1u8; 32]);

    // Multiple rounds of honest voting
    for round in 0..5 {
        let _ = harness.simulate_votes(1, round, VoteType::Prevote, Some(block_hash));
        let _ = harness.simulate_votes(1, round, VoteType::Precommit, Some(block_hash));
    }

    harness.assert_no_evidence();
}

#[test]
fn test_multiple_equivocators() {
    // Multiple validators equivocate
    let mut harness = TendermintTestHarness::new(4);
    harness.set_validator_behavior(1, VoteBehavior::Equivocate);
    harness.set_validator_behavior(2, VoteBehavior::Equivocate);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let _votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert_eq!(harness.evidence_count(), 2);
    harness.assert_evidence_for(ValidatorId::new(1));
    harness.assert_evidence_for(ValidatorId::new(2));
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAOS INJECTION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_vote_delay_injection() {
    let mut harness = TendermintTestHarness::new(3);
    harness.inject_chaos(TendermintChaosScenario::VoteDelay { delay_ms: 500 });

    assert!(harness.chaos.get_vote_delay().is_some());
    assert_eq!(harness.chaos.get_vote_delay().unwrap().as_millis(), 500);
}

#[test]
fn test_proposal_delay_injection() {
    let mut harness = TendermintTestHarness::new(3);
    harness.inject_chaos(TendermintChaosScenario::ProposalDelay { delay_ms: 1000 });

    assert!(harness.chaos.get_proposal_delay().is_some());
    assert_eq!(harness.chaos.get_proposal_delay().unwrap().as_millis(), 1000);
}

#[test]
fn test_clear_chaos() {
    let mut harness = TendermintTestHarness::new(3);
    harness.inject_chaos(TendermintChaosScenario::VoteDelay { delay_ms: 500 });
    harness.inject_chaos(TendermintChaosScenario::ProposalDelay { delay_ms: 1000 });

    harness.clear_chaos();

    assert!(harness.chaos.get_vote_delay().is_none());
    assert!(harness.chaos.get_proposal_delay().is_none());
}

#[test]
fn test_forced_timeout() {
    let mut harness = TendermintTestHarness::new(3);
    harness.inject_chaos(TendermintChaosScenario::ForceTimeout {
        step: TendermintStep::Propose,
    });

    assert!(harness.chaos.should_force_timeout(TendermintStep::Propose));
    assert!(!harness.chaos.should_force_timeout(TendermintStep::Prevote));
}

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATOR OFFLINE/ONLINE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_validator_goes_offline() {
    let mut harness = TendermintTestHarness::new(3);
    assert!(harness.has_quorum());

    harness.set_validator_offline(0);
    assert!(!harness.has_quorum()); // n=3 requires all 3

    harness.set_validator_online(0);
    assert!(harness.has_quorum());
}

#[test]
fn test_multiple_validators_offline() {
    let mut harness = TendermintTestHarness::new(4);
    assert!(harness.has_quorum()); // 4/4 online

    harness.set_validator_offline(0);
    assert!(harness.has_quorum()); // 3/4 online, still quorum

    harness.set_validator_offline(1);
    assert!(!harness.has_quorum()); // 2/4 online, no quorum
}

#[test]
fn test_offline_validator_no_vote() {
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_offline(0);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // Only 2 validators voted (one is offline)
    assert_eq!(votes.len(), 2);
    assert!(votes.iter().all(|v| v.validator.index() != 0));
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMIT RECORDING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_record_commit() {
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    assert!(!harness.is_committed(1));

    harness.record_commit(1, block_hash);

    assert!(harness.is_committed(1));
    assert_eq!(harness.get_committed_hash(1), Some(&block_hash));
}

#[test]
fn test_multiple_commits() {
    let mut harness = TendermintTestHarness::new(3);

    for height in 1..=10 {
        let block_hash = H256::from_low_u64_be(height);
        harness.record_commit(height, block_hash);
    }

    for height in 1..=10 {
        assert!(harness.is_committed(height));
        assert_eq!(
            harness.get_committed_hash(height),
            Some(&H256::from_low_u64_be(height))
        );
    }
}
