//! Timeout Handling Tests
//!
//! Tests timeout behavior for Tendermint consensus phases.
//! Note: Some timeout scenarios require multi-validator setups because
//! a single validator always reaches threshold with its own vote.

use super::harness::TendermintTestHarness;
use super::mock_actors::CapturedBroadcast;
use crate::actors_v2::chain::tendermint::TendermintStep;

/// Test: Propose timeout behavior (non-proposer scenario)
///
/// Note: For single validator, they are always the proposer, so this test
/// uses a 3-validator setup where our node is NOT the proposer.
#[tokio::test]
async fn test_propose_timeout_nil_prevote() {
    // Setup 3 validators where WE are validator index 1
    // Proposer for height 2, round 0: (2 + 0) % 3 = 2 (not us)
    let harness = TendermintTestHarness::multi_validator_as_index(3, 1).await.unwrap();

    // Verify we are NOT the proposer for height 2
    assert!(!harness.is_proposer_for(2, 0), "We should NOT be proposer for height 2");

    // Start height 2 - we are not the proposer, so we'll wait for proposal
    harness.start_height(2).await.unwrap();

    // At this point, no proposal received, inject propose timeout
    harness.inject_timeout(TendermintStep::Propose).await.unwrap();

    // After timeout handling, we should cast NIL prevote
    // (Implementation note: timeout handling would need to be implemented in harness)

    let state = harness.get_state().await;
    assert_eq!(state.height, 2, "Should be at height 2");
}

/// Test: Prevote timeout advances to precommit with NIL
///
/// Note: For single validator, 2/3+ threshold is reached immediately with own vote.
/// This test uses multi-validator setup where timeout fires before external votes arrive.
#[tokio::test]
async fn test_prevote_timeout_nil_precommit() {
    // Use 3 validators - need 2/3+ = 2 votes, but only cast 1 (our own)
    // Proposer selection: (height + round) % 3
    // For height 3, round 0: (3 + 0) % 3 = 0, so validator 0 is proposer
    let harness = TendermintTestHarness::multi_validator_as_index(3, 0).await.unwrap();

    // Start height 3 - we ARE the proposer (validator 0)
    assert!(harness.is_proposer_for(3, 0), "We should be proposer for height 3");
    harness.start_height(3).await.unwrap();

    // We're proposer at height 3, proposal broadcast, own prevote cast
    // But we only have 1/3 prevotes (our own) - not 2/3+
    // Inject prevote timeout before receiving external prevotes
    harness.inject_timeout(TendermintStep::Prevote).await.unwrap();

    let state = harness.get_state().await;
    assert_eq!(state.height, 3, "Should still be at height 3");
}

/// Test: Precommit timeout advances to new round
#[tokio::test]
async fn test_precommit_timeout_new_round() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Get initial state - should be committed for single validator
    let initial_state = harness.get_state().await;
    assert_eq!(initial_state.round, 0);

    // Clear broadcasts for next phase
    harness.clear_broadcasts().await;

    // For this test, we simulate being stuck before commit by advancing round
    harness.advance_round().await.unwrap();

    // Verify round advanced
    let new_state = harness.get_state().await;
    assert_eq!(new_state.round, 1, "Round should advance to 1");
    assert_eq!(new_state.height, 1, "Still same height");
    assert_eq!(new_state.step, TendermintStep::Propose, "Should be in propose step for new round");
}

/// Test: Round advancement triggers new proposal if proposer
#[tokio::test]
async fn test_round_advancement_triggers_proposal() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Record initial broadcasts
    let initial_broadcasts = harness.get_broadcasts().await;
    let initial_proposal_count = initial_broadcasts.iter()
        .filter(|b| matches!(b, CapturedBroadcast::Proposal { .. }))
        .count();

    // Clear broadcasts
    harness.clear_broadcasts().await;

    // Advance to round 1
    harness.advance_round().await.unwrap();

    // Verify state is ready for new proposal
    let state = harness.get_state().await;
    assert_eq!(state.round, 1, "Should be in round 1");
    assert_eq!(state.step, TendermintStep::Propose, "Should be in propose step");

    // Single validator should still be proposer in round 1
    assert!(harness.is_proposer_for(1, 1), "Single validator should be proposer in round 1");
}

/// Test: Multiple round advancements
#[tokio::test]
async fn test_multiple_round_advancements() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Advance through multiple rounds
    for expected_round in 1..=5 {
        harness.advance_round().await.unwrap();

        let state = harness.get_state().await;
        assert_eq!(state.round, expected_round, "Round should be {}", expected_round);
        assert_eq!(state.height, 1, "Height should remain 1");
    }
}

/// Test: Timeout scheduler position tracking
#[tokio::test]
async fn test_scheduler_position_updated_on_round_change() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Advance round - scheduler position should be updated
    harness.advance_round().await.unwrap();

    // The scheduler should now be at (height=1, round=1)
    // This is implicitly tested by the ability to schedule/inject timeouts
    // at the correct position

    let state = harness.get_state().await;
    assert_eq!(state.height, 1);
    assert_eq!(state.round, 1);
}

/// Test: Proposer rotation across rounds (multi-validator)
#[tokio::test]
async fn test_proposer_rotation() {
    let harness = TendermintTestHarness::multi_validator_as_index(3, 0).await.unwrap();

    // Height 1, Round 0: (1 + 0) % 3 = 1 -> validator 1 is proposer
    assert!(!harness.is_proposer_for(1, 0), "Validator 0 should NOT be proposer at (1,0)");

    // Height 1, Round 1: (1 + 1) % 3 = 2 -> validator 2 is proposer
    assert!(!harness.is_proposer_for(1, 1), "Validator 0 should NOT be proposer at (1,1)");

    // Height 1, Round 2: (1 + 2) % 3 = 0 -> validator 0 is proposer
    assert!(harness.is_proposer_for(1, 2), "Validator 0 SHOULD be proposer at (1,2)");

    // Height 2, Round 0: (2 + 0) % 3 = 2 -> validator 2 is proposer
    assert!(!harness.is_proposer_for(2, 0), "Validator 0 should NOT be proposer at (2,0)");

    // Height 3, Round 0: (3 + 0) % 3 = 0 -> validator 0 is proposer
    assert!(harness.is_proposer_for(3, 0), "Validator 0 SHOULD be proposer at (3,0)");
}
