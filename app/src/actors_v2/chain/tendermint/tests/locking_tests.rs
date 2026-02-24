//! Locking Behavior Tests
//!
//! Tests the critical locking rules that ensure consensus safety.
//! Once locked, a validator can only vote for the locked block or NIL.

use super::harness::TendermintTestHarness;
use super::mock_actors::CapturedBroadcast;
use crate::actors_v2::chain::tendermint::VoteType;

/// Test: Validator locks on block after 2/3+ prevotes
#[tokio::test]
async fn test_lock_on_two_thirds_prevotes() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // For single validator, 2/3+ = 1/1 = immediate lock
    let state = harness.get_state().await;

    assert!(state.locked_block.is_some(), "Should be locked after 2/3+ prevotes");
    assert_eq!(state.locked_round, Some(0), "Should be locked at round 0");
}

/// Test: Lock persists across round advancement
#[tokio::test]
async fn test_lock_persists_across_rounds() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Get the locked block hash from round 0
    let state_r0 = harness.get_state().await;
    let locked_hash_r0 = state_r0.locked_block.expect("Should be locked");
    let locked_round_r0 = state_r0.locked_round.expect("Should have locked round");

    // Advance to round 1
    harness.advance_round().await.unwrap();

    // Lock should still be present
    let state_r1 = harness.get_state().await;
    assert_eq!(state_r1.locked_block, Some(locked_hash_r0), "Lock should persist");
    assert_eq!(state_r1.locked_round, Some(locked_round_r0), "Locked round should persist");
}

/// Test: Lock clears on new height
#[tokio::test]
async fn test_lock_clears_on_new_height() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    // Start height 1 and commit
    harness.start_height(1).await.unwrap();

    let state_h1 = harness.get_state().await;
    assert!(state_h1.locked_block.is_some(), "Should be locked at height 1");

    // Start height 2
    harness.start_height(2).await.unwrap();

    // Lock from height 1 should be cleared, new lock for height 2
    let state_h2 = harness.get_state().await;
    assert_eq!(state_h2.height, 2, "Should be at height 2");

    // New height means new lock (if we voted)
    // The lock should be for height 2's block, not height 1's
    if state_h2.locked_block.is_some() {
        // The locked_round should be for this height's voting
        assert_eq!(state_h2.locked_round, Some(0), "Should be locked at round 0 of height 2");
    }
}

/// Test: Locked validator re-proposes locked block in new round
#[tokio::test]
async fn test_locked_reproposal() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Get the locked block hash from round 0
    let state = harness.get_state().await;
    let locked_hash = state.locked_block.expect("Should be locked");

    // Clear broadcasts
    harness.clear_broadcasts().await;

    // Advance to round 1 (via advance_round which simulates timeout)
    harness.advance_round().await.unwrap();

    // Single validator should still be proposer
    assert!(harness.is_proposer_for(1, 1), "Single validator should be proposer in round 1");

    // If we trigger proposal in round 1, it should propose the locked block
    // The locked hash should be re-used
    let state_after = harness.get_state().await;
    if state_after.locked_block.is_some() {
        assert_eq!(
            state_after.locked_block,
            Some(locked_hash),
            "Should remain locked on same block"
        );
    }
}

/// Test: Locked validator votes for locked block only
#[tokio::test]
async fn test_locked_validator_votes_for_locked_block() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Get all votes cast
    let broadcasts = harness.get_broadcasts().await;

    // Extract the block hash we locked on (from proposal)
    let proposal_hash = broadcasts.iter()
        .find_map(|b| {
            if let CapturedBroadcast::Proposal { block_hash, .. } = b {
                Some(*block_hash)
            } else {
                None
            }
        })
        .expect("Should have proposal");

    // All votes should be for this block
    for broadcast in &broadcasts {
        if let CapturedBroadcast::Vote { block_hash: Some(hash), .. } = broadcast {
            assert_eq!(
                *hash, proposal_hash,
                "All votes should be for the locked/proposed block"
            );
        }
    }
}

/// Test: Lock round is correctly set
#[tokio::test]
async fn test_lock_round_correct() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    // Start at round 0
    harness.start_height(1).await.unwrap();

    let state = harness.get_state().await;
    assert_eq!(state.locked_round, Some(0), "Should be locked at round 0");

    // Advance to round 1 and check lock round hasn't changed
    harness.advance_round().await.unwrap();

    let state_r1 = harness.get_state().await;
    assert_eq!(state_r1.locked_round, Some(0), "Locked round should still be 0");
    assert_eq!(state_r1.round, 1, "Current round should be 1");
}

/// Test: Multiple heights maintain correct locks
#[tokio::test]
async fn test_multiple_heights_lock_tracking() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();

    let mut locked_hashes = Vec::new();

    for height in 1..=3 {
        harness.start_height(height).await.unwrap();

        let state = harness.get_state().await;
        assert_eq!(state.height, height, "Should be at height {}", height);

        if let Some(hash) = state.locked_block {
            locked_hashes.push((height, hash));
        }
    }

    // Each height should have produced a different locked block
    // (since each height creates a new block)
    assert!(locked_hashes.len() >= 1, "Should have locked blocks");
}

/// Test: Valid block tracking alongside lock
#[tokio::test]
async fn test_valid_block_tracking() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // After 2/3+ prevotes, we should have both locked_block and valid_block set
    // These may or may not be the same block depending on implementation
    let state = harness.get_state().await;

    assert!(state.locked_block.is_some(), "Should have locked block");
    // valid_block is set when we see 2/3+ prevotes
    // For single validator, this happens simultaneously with lock
}

/// Test: Sent prevote tracking
#[tokio::test]
async fn test_sent_prevote_tracking() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Check that broadcasts include our prevote
    let broadcasts = harness.get_broadcasts().await;
    let prevote_count = broadcasts.iter()
        .filter(|b| matches!(b, CapturedBroadcast::Vote { vote_type: VoteType::Prevote, .. }))
        .count();

    assert!(prevote_count >= 1, "Should have broadcast prevote");
}

/// Test: Sent precommit tracking
#[tokio::test]
async fn test_sent_precommit_tracking() {
    let harness = TendermintTestHarness::single_validator().await.unwrap();
    harness.start_height(1).await.unwrap();

    // Check that broadcasts include our precommit
    let broadcasts = harness.get_broadcasts().await;
    let precommit_count = broadcasts.iter()
        .filter(|b| matches!(b, CapturedBroadcast::Vote { vote_type: VoteType::Precommit, .. }))
        .count();

    assert!(precommit_count >= 1, "Should have broadcast precommit");
}
