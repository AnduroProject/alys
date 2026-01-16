//! Fork choice rule implementation for Alys V2
//!
//! Implements longest chain rule with timestamp tiebreaker for resolving forks.
//! This module provides the logic to decide which competing chain should become
//! canonical when the network experiences a temporary fork.

use crate::actors_v2::common::serialization::calculate_block_hash;
use crate::block::SignedConsensusBlock;
use ethereum_types::H256;
use lighthouse_wrapper::types::MainnetEthSpec;

/// Fork choice decision
#[derive(Debug, Clone, PartialEq)]
pub enum ForkChoice {
    /// Keep current canonical block (reject new block)
    KeepCurrent,

    /// Reorganize to new block (new block wins)
    Reorganize { new_tip: H256, rollback_to: u64 },

    /// Chains are equal, apply tiebreaker (returns winner hash)
    Tiebreak { winner: H256 },
}

/// Compare two competing blocks at the same height and determine canonical chain
///
/// Uses the following rules in order:
/// 1. **Longest chain rule**: In a full implementation, would traverse back to find
///    chain lengths from a common ancestor. The longer chain wins.
/// 2. **Timestamp tiebreaker**: If chains are equal length, the block with the
///    earliest timestamp wins (incentivizes timely block production).
/// 3. **Hash tiebreaker**: If timestamps are identical, the block with the
///    lower hash value wins (provides deterministic resolution).
///
/// # Arguments
/// * `current_block` - The currently canonical block at this height
/// * `new_block` - The competing block received from the network
///
/// # Returns
/// A `ForkChoice` indicating which block should be canonical
///
pub fn compare_blocks(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
) -> ForkChoice {
    let current_height = current_block.message.execution_payload.block_number;
    let new_height = new_block.message.execution_payload.block_number;

    // Sanity check: blocks should be at the same height
    if current_height != new_height {
        tracing::error!(
            current = current_height,
            new = new_height,
            "compare_blocks called with different heights - keeping current"
        );
        return ForkChoice::KeepCurrent;
    }

    // For blocks at the same height, apply tiebreaker rules
    // In a full implementation with chain depth tracking, we would:
    // 1. Traverse both chains back to find common ancestor
    // 2. Count chain lengths from ancestor to tips
    // 3. Choose the longer chain
    //
    // For now, we use a simplified approach based on timestamps
    // since we're primarily dealing with 2-node regtest scenarios

    apply_tiebreaker(current_block, new_block)
}

/// Apply tiebreaker rule for competing blocks at the same height
///
/// Tiebreaker rules (in order):
/// 1. **Earliest timestamp wins** - Incentivizes validators to produce blocks promptly
/// 2. **Lowest hash wins** - Provides deterministic resolution if timestamps are identical
///
/// # Arguments
/// * `block_a` - First competing block (currently canonical)
/// * `block_b` - Second competing block (newly received)
///
/// # Returns
/// A `ForkChoice::Tiebreak` with the winning block's hash
///
fn apply_tiebreaker(
    block_a: &SignedConsensusBlock<MainnetEthSpec>,
    block_b: &SignedConsensusBlock<MainnetEthSpec>,
) -> ForkChoice {
    let timestamp_a = block_a.message.execution_payload.timestamp;
    let timestamp_b = block_b.message.execution_payload.timestamp;

    let hash_a = calculate_block_hash(block_a);
    let hash_b = calculate_block_hash(block_b);

    let winner = if timestamp_a < timestamp_b {
        // Block A has earlier timestamp - keep current
        tracing::info!(
            timestamp_a = timestamp_a,
            timestamp_b = timestamp_b,
            hash_a = %hash_a,
            hash_b = %hash_b,
            "Tiebreaker: current block wins (earlier timestamp)"
        );
        hash_a
    } else if timestamp_b < timestamp_a {
        // Block B has earlier timestamp - switch to new
        tracing::info!(
            timestamp_a = timestamp_a,
            timestamp_b = timestamp_b,
            hash_a = %hash_a,
            hash_b = %hash_b,
            "Tiebreaker: new block wins (earlier timestamp)"
        );
        hash_b
    } else {
        // Exact timestamp tie, use hash comparison (deterministic)
        if hash_a < hash_b {
            tracing::info!(
                timestamp = timestamp_a,
                hash_a = %hash_a,
                hash_b = %hash_b,
                "Tiebreaker: current block wins (lower hash)"
            );
            hash_a
        } else {
            tracing::info!(
                timestamp = timestamp_a,
                hash_a = %hash_a,
                hash_b = %hash_b,
                "Tiebreaker: new block wins (lower hash)"
            );
            hash_b
        }
    };

    ForkChoice::Tiebreak { winner }
}

/// Find common ancestor height between two blocks (simplified)
///
/// In a full implementation, this would traverse both chains back
/// to find the actual common ancestor block by comparing parent hashes.
///
/// Simplified version for 2-node regtest: Assumes the common ancestor
/// is at height - 1 (i.e., the fork occurred at the current height).
///
/// # Arguments
/// * `block_a` - First block
/// * `block_b` - Second block
///
/// # Returns
/// The height of the common ancestor (simplified: height - 1)
///
pub fn find_common_ancestor(
    block_a: &SignedConsensusBlock<MainnetEthSpec>,
    block_b: &SignedConsensusBlock<MainnetEthSpec>,
) -> u64 {
    let height_a = block_a.message.execution_payload.block_number;
    let height_b = block_b.message.execution_payload.block_number;

    // For blocks at same height, common ancestor is at height - 1
    // In a full implementation, would traverse parent_hash chains
    std::cmp::min(height_a, height_b).saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aura::Authority;
    use crate::block::ConsensusBlock;
    use lighthouse_wrapper::bls::Keypair;

    fn create_test_block(timestamp: u64, slot: u64) -> SignedConsensusBlock<MainnetEthSpec> {
        let mut block = ConsensusBlock::default();
        block.execution_payload.timestamp = timestamp;
        block.slot = slot;

        let keypair = Keypair::random();
        let authority = Authority {
            signer: keypair,
            index: 0,
        };

        block.sign_block(&authority)
    }

    #[test]
    fn test_tiebreaker_earlier_timestamp_wins() {
        let early_block = create_test_block(1000, 1);
        let late_block = create_test_block(2000, 1);

        let choice = compare_blocks(&early_block, &late_block);

        match choice {
            ForkChoice::Tiebreak { winner } => {
                let early_hash = calculate_block_hash(&early_block);
                assert_eq!(winner, early_hash, "Earlier timestamp should win");
            }
            _ => panic!("Expected Tiebreak decision"),
        }
    }

    #[test]
    fn test_tiebreaker_hash_comparison_when_timestamps_equal() {
        // Create two blocks with identical timestamps
        let block_a = create_test_block(1000, 1);
        let block_b = create_test_block(1000, 1);

        let choice = compare_blocks(&block_a, &block_b);

        match choice {
            ForkChoice::Tiebreak { winner } => {
                let hash_a = calculate_block_hash(&block_a);
                let hash_b = calculate_block_hash(&block_b);

                // Winner should be the one with lower hash
                let expected_winner = if hash_a < hash_b { hash_a } else { hash_b };
                assert_eq!(
                    winner, expected_winner,
                    "Lower hash should win when timestamps equal"
                );
            }
            _ => panic!("Expected Tiebreak decision"),
        }
    }

    #[test]
    fn test_common_ancestor_calculation() {
        let block_a = create_test_block(1000, 10);
        let block_b = create_test_block(1001, 10);

        // Both at height 0 (default), so common ancestor should be 0 (saturating_sub)
        let ancestor = find_common_ancestor(&block_a, &block_b);
        assert_eq!(
            ancestor, 0,
            "Common ancestor for height 0 blocks should be 0"
        );
    }
}
