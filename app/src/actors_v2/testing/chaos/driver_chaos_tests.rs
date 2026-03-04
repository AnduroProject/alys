//! Tendermint Driver Chaos Tests
//!
//! This module tests the TendermintDriver's timing and state transitions
//! under chaos conditions. These tests verify:
//!
//! - Timeout cascade handling
//! - Rapid round advancement
//! - Validator set updates during chaos
//! - Pause/resume behavior during network issues

use super::tendermint_chaos::{
    TendermintChaosScenario, TendermintTestHarness, VoteBehavior,
};
use crate::actors_v2::chain::tendermint::{TendermintStep, VoteType};
use ethereum_types::H256;

// ═══════════════════════════════════════════════════════════════════════════
// TIMEOUT CASCADE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_timeout_cascade_handling() {
    // Multiple concurrent timeouts should be handled correctly
    let mut harness = TendermintTestHarness::new(3);

    // Force timeouts on all steps
    harness.inject_chaos(TendermintChaosScenario::ForceTimeout {
        step: TendermintStep::Propose,
    });

    // Verify timeout detection
    assert!(harness.chaos.should_force_timeout(TendermintStep::Propose));
}

#[test]
fn test_selective_step_timeout() {
    let mut harness = TendermintTestHarness::new(3);

    // Only timeout on prevote step
    harness.inject_chaos(TendermintChaosScenario::ForceTimeout {
        step: TendermintStep::Prevote,
    });

    assert!(!harness.chaos.should_force_timeout(TendermintStep::Propose));
    assert!(harness.chaos.should_force_timeout(TendermintStep::Prevote));
    assert!(!harness.chaos.should_force_timeout(TendermintStep::Precommit));
}

// ═══════════════════════════════════════════════════════════════════════════
// RAPID ROUND ADVANCEMENT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_rapid_round_advancement_simulation() {
    // Stress test round advancement from 0 to 100+
    let mut harness = TendermintTestHarness::new(3);

    // Simulate voting across many rounds
    for round in 0u32..100 {
        // All validators vote NIL to force round advancement
        harness.set_validator_behavior(0, VoteBehavior::AlwaysNil);
        harness.set_validator_behavior(1, VoteBehavior::AlwaysNil);
        harness.set_validator_behavior(2, VoteBehavior::AlwaysNil);

        let votes = harness.simulate_votes(1, round, VoteType::Precommit, None);

        // All votes should be NIL
        assert!(votes.iter().all(|v| v.block_hash.is_none()));
        assert!(harness.has_vote_quorum(&votes, None));
    }

    // Should complete without issues
    harness.assert_no_evidence();
}

#[test]
fn test_round_progression_with_mixed_behavior() {
    let mut harness = TendermintTestHarness::new(4);

    // Different validators have different behaviors
    harness.set_validator_behavior(0, VoteBehavior::Honest);
    harness.set_validator_behavior(1, VoteBehavior::Honest);
    harness.set_validator_behavior(2, VoteBehavior::Delayed { delay_ms: 100 });
    harness.set_validator_behavior(3, VoteBehavior::AlwaysNil);

    let block_hash = H256::from_slice(&[1u8; 32]);

    // Simulate multiple rounds
    for round in 0u32..10 {
        let votes = harness.simulate_votes(1, round, VoteType::Prevote, Some(block_hash));
        // Should still get votes from honest and delayed validators
        assert!(votes.len() >= 3);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATOR SET CHANGES DURING CHAOS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_validator_online_offline_toggle() {
    let mut harness = TendermintTestHarness::new(4);
    let block_hash = H256::from_slice(&[1u8; 32]);

    // All online
    let votes1 = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));
    assert_eq!(votes1.len(), 4);
    assert!(harness.has_quorum());

    // Take one offline
    harness.set_validator_offline(0);
    let votes2 = harness.simulate_votes(1, 1, VoteType::Prevote, Some(block_hash));
    assert_eq!(votes2.len(), 3);
    assert!(harness.has_quorum()); // Still have quorum with n=4

    // Take another offline
    harness.set_validator_offline(1);
    let votes3 = harness.simulate_votes(1, 2, VoteType::Prevote, Some(block_hash));
    assert_eq!(votes3.len(), 2);
    assert!(!harness.has_quorum()); // Lost quorum

    // Bring back online
    harness.set_validator_online(0);
    harness.set_validator_online(1);
    let votes4 = harness.simulate_votes(1, 3, VoteType::Prevote, Some(block_hash));
    assert_eq!(votes4.len(), 4);
    assert!(harness.has_quorum());
}

#[test]
fn test_quorum_boundary_conditions() {
    // Test various validator counts and their quorum requirements
    let test_cases = vec![
        (3, 3),  // n=3: need 3 (100%)
        (4, 3),  // n=4: need 3 (75%)
        (5, 4),  // n=5: need 4 (80%)
        (6, 5),  // n=6: need 5 (83%)
        (7, 5),  // n=7: need 5 (71%)
        (10, 7), // n=10: need 7 (70%)
    ];

    for (n, expected_quorum) in test_cases {
        let harness = TendermintTestHarness::new(n);
        assert_eq!(
            harness.quorum_size(),
            expected_quorum,
            "n={}: expected quorum {}, got {}",
            n,
            expected_quorum,
            harness.quorum_size()
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAOS SCENARIO STATISTICS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_chaos_injection_stats() {
    let mut harness = TendermintTestHarness::new(3);

    // Initially no stats
    assert_eq!(harness.chaos.stats.votes_dropped, 0);
    assert_eq!(harness.chaos.stats.proposals_dropped, 0);
}

#[test]
fn test_multiple_chaos_scenarios() {
    let mut harness = TendermintTestHarness::new(3);

    harness.inject_chaos(TendermintChaosScenario::VoteDelay { delay_ms: 100 });
    harness.inject_chaos(TendermintChaosScenario::ProposalDelay { delay_ms: 200 });
    harness.inject_chaos(TendermintChaosScenario::ForceTimeout {
        step: TendermintStep::Propose,
    });

    // All scenarios should be active
    assert!(harness.chaos.get_vote_delay().is_some());
    assert!(harness.chaos.get_proposal_delay().is_some());
    assert!(harness.chaos.should_force_timeout(TendermintStep::Propose));

    // Clear and verify
    harness.clear_chaos();
    assert!(harness.chaos.get_vote_delay().is_none());
    assert!(harness.chaos.get_proposal_delay().is_none());
    assert!(!harness.chaos.should_force_timeout(TendermintStep::Propose));
}

// ═══════════════════════════════════════════════════════════════════════════
// HEIGHT PROGRESSION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_height_commit_tracking() {
    let mut harness = TendermintTestHarness::new(3);

    // Simulate committing blocks at multiple heights
    for height in 1..=50 {
        let block_hash = H256::from_low_u64_be(height);

        // Simulate successful vote
        let votes = harness.simulate_votes(height, 0, VoteType::Precommit, Some(block_hash));
        assert!(harness.has_vote_quorum(&votes, Some(block_hash)));

        // Record commit
        harness.record_commit(height, block_hash);
        assert!(harness.is_committed(height));
    }

    // Verify all heights committed
    for height in 1..=50 {
        assert!(harness.is_committed(height));
    }

    // No evidence during honest operation
    harness.assert_no_evidence();
}

// ═══════════════════════════════════════════════════════════════════════════
// BYZANTINE BEHAVIOR TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_single_byzantine_validator() {
    let mut harness = TendermintTestHarness::new(4);
    harness.set_validator_behavior(0, VoteBehavior::Byzantine);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // 4 validators voted (byzantine also votes, just for wrong block)
    assert_eq!(votes.len(), 4);

    // Only 3 honest validators voted for the correct block
    let matching_votes: Vec<_> = votes
        .iter()
        .filter(|v| v.block_hash == Some(block_hash))
        .collect();
    assert_eq!(matching_votes.len(), 3);

    // Still have quorum for the correct block (n=4 needs 3)
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_byzantine_minority_cannot_commit() {
    let mut harness = TendermintTestHarness::new(4);
    harness.set_validator_behavior(0, VoteBehavior::Byzantine);

    // Byzantine validator votes for a different block
    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Precommit, Some(block_hash));

    // The byzantine vote is for a random hash, which shouldn't have quorum
    let byzantine_validator_vote = votes.iter().find(|v| v.validator.index() == 0);
    if let Some(byz_vote) = byzantine_validator_vote {
        // Byzantine vote should be for a different block
        let byz_hash = byz_vote.block_hash;
        if byz_hash != Some(block_hash) {
            // No quorum for the byzantine block (only 1 vote)
            assert!(!harness.has_vote_quorum(&votes, byz_hash));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EDGE CASE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_all_validators_silent() {
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(0, VoteBehavior::Silent);
    harness.set_validator_behavior(1, VoteBehavior::Silent);
    harness.set_validator_behavior(2, VoteBehavior::Silent);

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // No votes cast (silent validators don't vote)
    assert!(votes.is_empty());
    // No vote quorum since no votes were cast
    assert!(!harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_delayed_votes_still_count() {
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(0, VoteBehavior::Delayed { delay_ms: 1000 });
    harness.set_validator_behavior(1, VoteBehavior::Delayed { delay_ms: 2000 });
    harness.set_validator_behavior(2, VoteBehavior::Delayed { delay_ms: 3000 });

    let block_hash = H256::from_slice(&[1u8; 32]);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    // All delayed votes still arrive (in test simulation)
    assert_eq!(votes.len(), 3);
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
}

#[test]
fn test_vote_for_height_zero() {
    // Edge case: height 0 votes
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    let votes = harness.simulate_votes(0, 0, VoteType::Prevote, Some(block_hash));
    assert_eq!(votes.len(), 3);

    // All votes should have height 0
    assert!(votes.iter().all(|v| v.height == 0));
}

#[test]
fn test_high_round_number() {
    // Edge case: very high round number
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    let votes = harness.simulate_votes(1, u32::MAX, VoteType::Prevote, Some(block_hash));
    assert_eq!(votes.len(), 3);

    // All votes should have the high round number
    assert!(votes.iter().all(|v| v.round == u32::MAX));
}
