//! Single Validator Consensus Cycle Tests
//!
//! Tests the happy path where a single validator completes consensus cycles.
//! Single validator is always the proposer and reaches threshold immediately.

use std::time::Duration;

use super::harness::TendermintTestHarness;
use super::mock_actors::CapturedBroadcast;
use crate::actors_v2::chain::tendermint::{TendermintStep, VoteType};

/// Test: Complete consensus cycle for single validator
///
/// Verifies the happy path: propose -> prevote -> precommit -> commit
#[tokio::test]
async fn test_single_validator_full_cycle() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    // Start height 1
    harness.start_height(1).await.unwrap();

    // Verify proposal was created and broadcast
    let broadcasts = harness.get_broadcasts().await;
    assert!(
        broadcasts.iter().any(|b| matches!(b, CapturedBroadcast::Proposal { height: 1, .. })),
        "Should broadcast proposal"
    );

    // Verify prevote was cast for own proposal
    assert!(
        broadcasts.iter().any(|b| matches!(
            b,
            CapturedBroadcast::Vote { vote_type: VoteType::Prevote, block_hash: Some(_), .. }
        )),
        "Should broadcast prevote for block"
    );

    // Verify precommit was cast after 2/3+ prevotes (own vote)
    assert!(
        broadcasts.iter().any(|b| matches!(
            b,
            CapturedBroadcast::Vote { vote_type: VoteType::Precommit, block_hash: Some(_), .. }
        )),
        "Should broadcast precommit for block"
    );

    // Verify commit was broadcast
    assert!(
        broadcasts.iter().any(|b| matches!(b, CapturedBroadcast::Commit { height: 1, .. })),
        "Should broadcast commit"
    );

    // Verify state shows commit step
    let state = harness.get_state().await;
    assert_eq!(state.step, TendermintStep::Commit, "Should be in commit step");
    assert_eq!(state.height, 1, "Should still be at height 1");
}

/// Test: Produce multiple blocks in sequence
#[tokio::test]
async fn test_single_validator_produces_10_blocks() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    for height in 1..=10 {
        harness.start_height(height).await.unwrap();
        harness.wait_for_commit(height, Duration::from_secs(5)).await.unwrap();
    }

    let state = harness.get_state().await;
    // After committing height 10, we're still at height 10 (state advances on new_height call)
    assert_eq!(state.height, 10, "Should be at height 10");
    assert_eq!(state.step, TendermintStep::Commit, "Should be committed");
}

/// Test: Block hash consistency across consensus phases
#[tokio::test]
async fn test_block_hash_consistency() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    let broadcasts = harness.get_broadcasts().await;

    // Extract block hash from proposal
    let proposal_hash = broadcasts.iter()
        .find_map(|b| {
            if let CapturedBroadcast::Proposal { block_hash, .. } = b {
                Some(*block_hash)
            } else {
                None
            }
        })
        .expect("Should have proposal");

    // Extract block hash from prevote
    let prevote_hash = broadcasts.iter()
        .find_map(|b| {
            if let CapturedBroadcast::Vote { vote_type: VoteType::Prevote, block_hash: Some(h), .. } = b {
                Some(*h)
            } else {
                None
            }
        })
        .expect("Should have prevote");

    // Extract block hash from precommit
    let precommit_hash = broadcasts.iter()
        .find_map(|b| {
            if let CapturedBroadcast::Vote { vote_type: VoteType::Precommit, block_hash: Some(h), .. } = b {
                Some(*h)
            } else {
                None
            }
        })
        .expect("Should have precommit");

    // Extract block hash from commit
    let commit_hash = broadcasts.iter()
        .find_map(|b| {
            if let CapturedBroadcast::Commit { block_hash, .. } = b {
                Some(*block_hash)
            } else {
                None
            }
        })
        .expect("Should have commit");

    // All hashes must match
    assert_eq!(proposal_hash, prevote_hash, "Proposal and prevote hash should match");
    assert_eq!(prevote_hash, precommit_hash, "Prevote and precommit hash should match");
    assert_eq!(precommit_hash, commit_hash, "Precommit and commit hash should match");
}

/// Test: Broadcasts are in correct order
#[tokio::test]
async fn test_broadcast_order() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    let broadcasts = harness.get_broadcasts().await;

    // Find indices of each broadcast type
    let proposal_idx = broadcasts.iter()
        .position(|b| matches!(b, CapturedBroadcast::Proposal { .. }))
        .expect("Should have proposal");

    let prevote_idx = broadcasts.iter()
        .position(|b| matches!(b, CapturedBroadcast::Vote { vote_type: VoteType::Prevote, .. }))
        .expect("Should have prevote");

    let precommit_idx = broadcasts.iter()
        .position(|b| matches!(b, CapturedBroadcast::Vote { vote_type: VoteType::Precommit, .. }))
        .expect("Should have precommit");

    let commit_idx = broadcasts.iter()
        .position(|b| matches!(b, CapturedBroadcast::Commit { .. }))
        .expect("Should have commit");

    // Verify order: proposal -> prevote -> precommit -> commit
    assert!(proposal_idx < prevote_idx, "Proposal should come before prevote");
    assert!(prevote_idx < precommit_idx, "Prevote should come before precommit");
    assert!(precommit_idx < commit_idx, "Precommit should come before commit");
}

/// Test: State transitions through correct steps
#[tokio::test]
async fn test_single_validator_state_transitions() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    // Initial state
    let initial_state = harness.get_state().await;
    assert_eq!(initial_state.height, 0, "Should start at height 0");
    assert_eq!(initial_state.step, TendermintStep::Propose, "Should start in propose step");

    // After starting height
    harness.start_height(1).await.unwrap();

    let final_state = harness.get_state().await;
    assert_eq!(final_state.height, 1, "Should be at height 1");
    assert_eq!(final_state.step, TendermintStep::Commit, "Should be in commit step");
    assert!(final_state.locked_block.is_some(), "Should have locked block");
    assert_eq!(final_state.locked_round, Some(0), "Should be locked at round 0");
}

/// Test: Single validator is always proposer
#[tokio::test]
async fn test_single_validator_always_proposer() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    // Check proposer for various heights and rounds
    for height in 1..=5 {
        for round in 0..=3 {
            assert!(
                harness.is_proposer_for(height, round),
                "Single validator should always be proposer (h={}, r={})",
                height,
                round
            );
        }
    }
}
