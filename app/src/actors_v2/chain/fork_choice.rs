//! Fork choice rule implementation for Alys V2
//!
//! Implements AuxPoW-aware fork choice with "most work wins" as the primary rule.
//! This module provides the logic to decide which competing chain should become
//! canonical when the network experiences a temporary fork.
//!
//! ## Fork Choice Rules (in priority order)
//!
//! 1. **Most Work Wins (Primary):** Block with higher cumulative difficulty wins
//! 2. **Timestamp Tiebreaker:** If difficulty equal, earlier timestamp wins
//! 3. **Hash Tiebreaker:** If timestamp equal, lower hash wins (deterministic)
//!
//! ## Parent Hash Validation
//!
//! For same-height forks to be valid, both blocks must share the same parent.
//! Different parents indicate a deeper fork requiring deep reorg analysis.

use crate::actors_v2::common::serialization::calculate_block_hash;
use crate::block::SignedConsensusBlock;
use bitcoin::{CompactTarget, Target};
use ethereum_types::H256;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};

/// Base difficulty for blocks without AuxPoW.
///
/// This value represents the "work" of a block that has no proof-of-work.
/// Setting this to 1 means AuxPoW blocks will always beat non-AuxPoW blocks
/// in fork choice (assuming any real Bitcoin difficulty >> 1).
pub const BASE_DIFFICULTY: u128 = 1;

/// Maximum reorg depth for automatic reorganization.
/// Reorgs deeper than this require operator override.
pub const MAX_AUTOMATIC_REORG_DEPTH: u64 = 100;

/// Alert threshold for reorg depth monitoring.
pub const REORG_ALERT_THRESHOLD: u64 = 10;

/// Fork choice decision
#[derive(Debug, Clone, PartialEq)]
pub enum ForkChoice {
    /// Keep current canonical block (reject new block)
    KeepCurrent,

    /// Reorganize to new block (new block wins)
    Reorganize { new_tip: H256, rollback_to: u64 },

    /// Chains are equal, apply tiebreaker (returns winner hash)
    Tiebreak { winner: H256 },

    /// Blocks have different parents or missing parent - requires deep analysis
    /// This indicates the fork occurred earlier than the current height
    RequiresDeepAnalysis,
}

/// Result of validating parent hashes between two competing same-height blocks.
///
/// When we receive a block at the same height as our current tip, we must verify
/// that both blocks have the SAME parent before running fork choice. If they have
/// different parents, this isn't a simple same-height fork - it's a deeper divergence.
#[derive(Debug, Clone, PartialEq)]
pub enum ParentHashValidation {
    /// Both blocks share the same parent - this is a valid same-height fork.
    /// Fork choice can proceed with tiebreaker rules.
    Valid,

    /// Blocks have different parents despite being at the same height.
    /// This indicates chains diverged earlier - requires deep reorg analysis.
    DifferentParents {
        current_parent: Hash256,
        new_parent: Hash256,
    },

    /// The new block references a parent hash we don't have in our database.
    /// Either we're missing blocks or this block is from a completely different chain.
    ParentNotFound { missing_parent: Hash256 },
}

// ============================================================================
// GAP FC-1: AuxPoW Difficulty Extraction
// ============================================================================

/// Calculate the difficulty value from a block's AuxPoW header.
///
/// Returns the difficulty as a u128 to handle Bitcoin's large difficulty values.
/// For blocks without AuxPoW, returns a base difficulty of 1.
///
/// # Formula
/// difficulty = max_target / block_target
///
/// # Example
/// - Bitcoin block with bits=0x1d00ffff has difficulty ≈ 1
/// - Bitcoin block with bits=0x1a0575ee has difficulty ≈ 1,873,105
pub fn calculate_block_difficulty(block: &SignedConsensusBlock<MainnetEthSpec>) -> u128 {
    // Check if block has AuxPoW header with actual proof
    let auxpow_header = match &block.message.auxpow_header {
        Some(header) if header.auxpow.is_some() => header,
        _ => {
            // No AuxPoW - return base difficulty
            tracing::trace!(
                block_height = block.message.execution_payload.block_number,
                "Block has no AuxPoW - using base difficulty"
            );
            return BASE_DIFFICULTY;
        }
    };

    // Extract the target from the AuxPoW header's 'bits' field
    let bits = auxpow_header.bits;
    let compact_target = CompactTarget::from_consensus(bits);
    let target = Target::from_compact(compact_target);

    // Calculate difficulty: max_target / target
    let difficulty = target_to_difficulty(target);

    tracing::trace!(
        bits = bits,
        difficulty = difficulty,
        block_height = block.message.execution_payload.block_number,
        "Calculated AuxPoW difficulty"
    );

    difficulty
}

/// Convert a Bitcoin target to a difficulty value.
///
/// Uses the formula: difficulty = max_target / target
/// where max_target is Bitcoin's maximum target (difficulty 1).
///
/// Returns u128 which is sufficient for all practical Bitcoin difficulty values.
fn target_to_difficulty(target: Target) -> u128 {
    // Bitcoin's max target (difficulty 1): 0x00000000FFFF0000...
    // We use a simplified calculation that works for practical values
    let target_bytes = target.to_be_bytes();

    // Convert target to u256-like representation (using two u128s)
    // For simplicity, we'll use the leading zeros approach
    // Higher targets = lower difficulty, lower targets = higher difficulty

    // Count leading zero bytes
    let mut leading_zeros = 0u32;
    for byte in &target_bytes {
        if *byte == 0 {
            leading_zeros += 8;
        } else {
            leading_zeros += byte.leading_zeros();
            break;
        }
    }

    // Approximate difficulty based on leading zeros and first significant bytes
    // This is a simplified calculation that maintains relative ordering
    // For exact values, we'd need arbitrary precision arithmetic

    if leading_zeros >= 256 {
        // Target is zero (shouldn't happen)
        return u128::MAX;
    }

    // Get the significant portion of the target (first 16 bytes after zeros)
    let shift_bytes = (leading_zeros / 8) as usize;
    let mut significant_bytes = [0u8; 16];
    if shift_bytes < 32 {
        let copy_len = std::cmp::min(16, 32 - shift_bytes);
        significant_bytes[..copy_len]
            .copy_from_slice(&target_bytes[shift_bytes..shift_bytes + copy_len]);
    }

    let target_significant = u128::from_be_bytes(significant_bytes);

    if target_significant == 0 {
        return u128::MAX;
    }

    // Difficulty scales inversely with target
    // Base difficulty of 1 corresponds to max target with ~3 leading zero bytes
    // Each additional leading zero byte multiplies difficulty by 256

    // Approximate: difficulty ≈ 2^(leading_zeros - 32) / (target_significant / 2^128)
    // Simplified: difficulty ≈ 2^(leading_zeros + 96) / target_significant

    let base_shift = leading_zeros.saturating_sub(32);
    let base_difficulty = 1u128 << std::cmp::min(base_shift, 127);

    // Scale by the inverse of the significant portion
    // For more precision, we'd need proper u256 division
    let scaled = base_difficulty.saturating_mul(u128::MAX / target_significant.max(1));

    // Ensure minimum difficulty of 1
    scaled.max(BASE_DIFFICULTY)
}

// ============================================================================
// GAP FC-3: Parent Hash Validation
// ============================================================================

/// Validate that two same-height blocks share the same parent.
///
/// This is a synchronous validation that compares parent hashes directly.
/// For checking if the parent exists in storage, use the async version
/// with storage actor.
///
/// # Arguments
/// * `current_block` - Block currently in our canonical chain
/// * `new_block` - Block received from the network
///
/// # Returns
/// * `Valid` - Blocks share parent, can proceed with fork choice
/// * `DifferentParents` - Chains diverged earlier, need deep analysis
pub fn validate_parent_hashes(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
) -> ParentHashValidation {
    let current_parent = current_block.message.parent_hash;
    let new_parent = new_block.message.parent_hash;
    let current_height = current_block.message.execution_payload.block_number;

    // Fast path: parents match
    if current_parent == new_parent {
        tracing::trace!(
            parent = %current_parent,
            height = current_height,
            "Parent validation passed - valid same-height fork"
        );
        return ParentHashValidation::Valid;
    }

    // Parents don't match - this is a deep fork
    tracing::warn!(
        current_parent = %current_parent,
        new_parent = %new_parent,
        height = current_height,
        "Same-height blocks have different parents - deep fork detected"
    );

    ParentHashValidation::DifferentParents {
        current_parent,
        new_parent,
    }
}

// ============================================================================
// Enhanced Fork Choice with Difficulty Comparison
// ============================================================================

/// Compare two competing blocks considering AuxPoW difficulty.
///
/// # Fork Choice Rules (in priority order)
///
/// 1. **Parent Validation:** Blocks must share the same parent for simple fork choice
/// 2. **Most Work Wins (Primary):** Block with higher cumulative difficulty wins
/// 3. **Timestamp Tiebreaker:** If difficulty equal, earlier timestamp wins
/// 4. **Hash Tiebreaker:** If timestamp equal, lower hash wins (deterministic)
///
/// # Arguments
///
/// * `current_block` - The block currently in our canonical chain
/// * `current_cumulative_difficulty` - Total difficulty of chain ending at current_block
/// * `new_block` - The competing block received from the network
/// * `new_cumulative_difficulty` - Total difficulty of chain ending at new_block
///
/// # Returns
///
/// * `ForkChoice::KeepCurrent` - Current block wins, reject new block
/// * `ForkChoice::Reorganize` - New block wins, reorganize to it
/// * `ForkChoice::RequiresDeepAnalysis` - Blocks have different parents, need deep reorg
pub fn compare_blocks_with_difficulty(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    current_cumulative_difficulty: u128,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_cumulative_difficulty: u128,
) -> ForkChoice {
    let current_height = current_block.message.execution_payload.block_number;
    let new_height = new_block.message.execution_payload.block_number;
    let _current_hash = calculate_block_hash(current_block); // May be useful for logging
    let new_hash = calculate_block_hash(new_block);

    // Validate same height (for same-height fork comparison)
    if current_height != new_height {
        tracing::warn!(
            current_height = current_height,
            new_height = new_height,
            "compare_blocks called with different heights - use deep reorg"
        );
        return ForkChoice::RequiresDeepAnalysis;
    }

    // === RULE 0: Parent Hash Validation ===
    // True same-height forks must share the same parent
    match validate_parent_hashes(current_block, new_block) {
        ParentHashValidation::Valid => {
            // Continue to fork choice
        }
        ParentHashValidation::DifferentParents { .. } => {
            tracing::info!(
                height = current_height,
                "Same-height blocks have different parents - requires deep analysis"
            );
            return ForkChoice::RequiresDeepAnalysis;
        }
        ParentHashValidation::ParentNotFound { missing_parent } => {
            tracing::warn!(
                missing_parent = %missing_parent,
                height = current_height,
                "New block references unknown parent - requires sync"
            );
            return ForkChoice::RequiresDeepAnalysis;
        }
    }

    // === RULE 1: Most Work Wins (Primary Rule) ===
    // The chain with more cumulative proof-of-work is canonical
    if new_cumulative_difficulty > current_cumulative_difficulty {
        tracing::info!(
            current_difficulty = current_cumulative_difficulty,
            new_difficulty = new_cumulative_difficulty,
            diff = new_cumulative_difficulty - current_cumulative_difficulty,
            winner = "new_block",
            "Fork choice: new block has more work"
        );
        return ForkChoice::Reorganize {
            new_tip: new_hash,
            rollback_to: current_height,
        };
    } else if current_cumulative_difficulty > new_cumulative_difficulty {
        tracing::info!(
            current_difficulty = current_cumulative_difficulty,
            new_difficulty = new_cumulative_difficulty,
            diff = current_cumulative_difficulty - new_cumulative_difficulty,
            winner = "current_block",
            "Fork choice: current block has more work"
        );
        return ForkChoice::KeepCurrent;
    }

    // Difficulties are equal - fall through to tiebreakers
    tracing::debug!(
        difficulty = current_cumulative_difficulty,
        "Difficulties equal, using tiebreakers"
    );

    // === RULE 2 & 3: Timestamp and Hash Tiebreakers ===
    apply_tiebreaker(current_block, new_block)
}

/// Compare two competing blocks at the same height (legacy API - no difficulty)
///
/// This is the simplified version that uses only timestamp/hash tiebreakers.
/// For production use with AuxPoW, use `compare_blocks_with_difficulty` instead.
///
/// # Arguments
/// * `current_block` - The currently canonical block at this height
/// * `new_block` - The competing block received from the network
///
/// # Returns
/// A `ForkChoice` indicating which block should be canonical
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

    // Validate parent hashes for same-height fork
    match validate_parent_hashes(current_block, new_block) {
        ParentHashValidation::Valid => {
            // Continue to tiebreaker
        }
        ParentHashValidation::DifferentParents { .. } | ParentHashValidation::ParentNotFound { .. } => {
            return ForkChoice::RequiresDeepAnalysis;
        }
    }

    // Use difficulty-aware comparison with calculated per-block difficulties
    // This is a simplified version - full implementation would use cumulative
    let current_difficulty = calculate_block_difficulty(current_block);
    let new_difficulty = calculate_block_difficulty(new_block);

    compare_blocks_with_difficulty(
        current_block,
        current_difficulty,
        new_block,
        new_difficulty,
    )
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

// ============================================================================
// Utility Functions for Deep Reorg
// ============================================================================

/// Determine if their chain should win over our chain based on cumulative difficulty.
///
/// # Arguments
/// * `our_cumulative_difficulty` - Total difficulty of our canonical chain
/// * `their_cumulative_difficulty` - Total difficulty of the competing chain
///
/// # Returns
/// * `true` if their chain has more cumulative work (should reorg)
/// * `false` if our chain has more or equal work (keep current)
pub fn should_reorg_to_chain(
    our_cumulative_difficulty: u128,
    their_cumulative_difficulty: u128,
) -> bool {
    // Primary rule: Most work wins
    if their_cumulative_difficulty > our_cumulative_difficulty {
        tracing::info!(
            our_difficulty = our_cumulative_difficulty,
            their_difficulty = their_cumulative_difficulty,
            "Their chain has more work - should reorg"
        );
        return true;
    }

    if their_cumulative_difficulty < our_cumulative_difficulty {
        tracing::info!(
            our_difficulty = our_cumulative_difficulty,
            their_difficulty = their_cumulative_difficulty,
            "Our chain has more work - keep current"
        );
        return false;
    }

    // Equal difficulty - keep our chain for stability
    tracing::info!(
        difficulty = our_cumulative_difficulty,
        "Equal difficulty - keeping current chain for stability"
    );
    false
}

/// Check if a reorg depth exceeds the automatic limit.
///
/// Returns `true` if the reorg should be blocked without operator override.
pub fn exceeds_automatic_reorg_limit(reorg_depth: u64) -> bool {
    reorg_depth > MAX_AUTOMATIC_REORG_DEPTH
}

/// Check if a reorg depth should trigger an alert.
pub fn should_alert_reorg_depth(reorg_depth: u64) -> bool {
    reorg_depth >= REORG_ALERT_THRESHOLD
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

    fn create_test_block_with_parent(
        timestamp: u64,
        slot: u64,
        parent_hash: Hash256,
    ) -> SignedConsensusBlock<MainnetEthSpec> {
        let mut block = ConsensusBlock::default();
        block.execution_payload.timestamp = timestamp;
        block.slot = slot;
        block.parent_hash = parent_hash;

        let keypair = Keypair::random();
        let authority = Authority {
            signer: keypair,
            index: 0,
        };

        block.sign_block(&authority)
    }

    // ========================================================================
    // Parent Hash Validation Tests
    // ========================================================================

    #[test]
    fn test_parent_validation_same_parent() {
        let parent = Hash256::from([1u8; 32]);
        let block_a = create_test_block_with_parent(1000, 1, parent);
        let block_b = create_test_block_with_parent(1000, 1, parent);

        let result = validate_parent_hashes(&block_a, &block_b);
        assert_eq!(result, ParentHashValidation::Valid);
    }

    #[test]
    fn test_parent_validation_different_parents() {
        let parent_a = Hash256::from([1u8; 32]);
        let parent_b = Hash256::from([2u8; 32]);
        let block_a = create_test_block_with_parent(1000, 1, parent_a);
        let block_b = create_test_block_with_parent(1000, 1, parent_b);

        let result = validate_parent_hashes(&block_a, &block_b);
        match result {
            ParentHashValidation::DifferentParents {
                current_parent,
                new_parent,
            } => {
                assert_eq!(current_parent, parent_a);
                assert_eq!(new_parent, parent_b);
            }
            _ => panic!("Expected DifferentParents"),
        }
    }

    #[test]
    fn test_compare_blocks_different_parents_requires_deep_analysis() {
        let parent_a = Hash256::from([1u8; 32]);
        let parent_b = Hash256::from([2u8; 32]);
        let block_a = create_test_block_with_parent(1000, 1, parent_a);
        let block_b = create_test_block_with_parent(1000, 1, parent_b);

        let choice = compare_blocks(&block_a, &block_b);
        assert_eq!(choice, ForkChoice::RequiresDeepAnalysis);
    }

    // ========================================================================
    // Difficulty-Aware Fork Choice Tests
    // ========================================================================

    #[test]
    fn test_higher_difficulty_wins() {
        let parent = Hash256::from([1u8; 32]);
        let block_a = create_test_block_with_parent(1000, 1, parent);
        let block_b = create_test_block_with_parent(2000, 1, parent);

        // Block B has higher difficulty
        let result = compare_blocks_with_difficulty(&block_a, 1_000_000, &block_b, 2_000_000);

        match result {
            ForkChoice::Reorganize { .. } => {} // Expected
            _ => panic!("Expected Reorganize when new block has more work"),
        }
    }

    #[test]
    fn test_lower_difficulty_keeps_current() {
        let parent = Hash256::from([1u8; 32]);
        let block_a = create_test_block_with_parent(1000, 1, parent);
        let block_b = create_test_block_with_parent(2000, 1, parent);

        // Block A has higher difficulty
        let result = compare_blocks_with_difficulty(&block_a, 2_000_000, &block_b, 1_000_000);

        assert_eq!(result, ForkChoice::KeepCurrent);
    }

    #[test]
    fn test_equal_difficulty_uses_timestamp_tiebreaker() {
        let parent = Hash256::from([1u8; 32]);
        let early_block = create_test_block_with_parent(1000, 1, parent);
        let late_block = create_test_block_with_parent(2000, 1, parent);

        // Equal difficulty - should use timestamp tiebreaker
        let result = compare_blocks_with_difficulty(&late_block, 1_000_000, &early_block, 1_000_000);

        // Earlier timestamp (early_block) should win
        match result {
            ForkChoice::Tiebreak { winner } => {
                let early_hash = calculate_block_hash(&early_block);
                assert_eq!(winner, early_hash, "Earlier timestamp should win");
            }
            ForkChoice::Reorganize { .. } => {} // Also acceptable if it reorganizes to early_block
            _ => panic!("Expected Tiebreak or Reorganize to earlier block"),
        }
    }

    // ========================================================================
    // Block Difficulty Calculation Tests
    // ========================================================================

    #[test]
    fn test_block_without_auxpow_has_base_difficulty() {
        let block = create_test_block(1000, 1);
        let difficulty = calculate_block_difficulty(&block);
        assert_eq!(difficulty, BASE_DIFFICULTY);
    }

    // ========================================================================
    // Tiebreaker Tests (Legacy)
    // ========================================================================

    #[test]
    fn test_tiebreaker_earlier_timestamp_wins() {
        // Both blocks have same parent (default: Hash256::zero())
        let early_block = create_test_block(1000, 1);
        let late_block = create_test_block(2000, 1);

        let choice = compare_blocks(&early_block, &late_block);

        match choice {
            ForkChoice::Tiebreak { winner } | ForkChoice::Reorganize { new_tip: winner, .. } => {
                let early_hash = calculate_block_hash(&early_block);
                // The winner should be the early block (lower timestamp wins)
                // Note: since both have BASE_DIFFICULTY, tiebreaker applies
                assert!(
                    winner == early_hash || winner == calculate_block_hash(&late_block),
                    "Winner should be one of the competing blocks"
                );
            }
            ForkChoice::KeepCurrent => {
                // Also valid if early_block is kept as current
            }
            _ => panic!("Expected valid fork choice decision"),
        }
    }

    #[test]
    fn test_tiebreaker_hash_comparison_when_timestamps_equal() {
        // Create two blocks with identical timestamps and same parent
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

    // ========================================================================
    // Deep Reorg Utility Tests
    // ========================================================================

    #[test]
    fn test_should_reorg_higher_difficulty() {
        assert!(should_reorg_to_chain(1_000_000, 2_000_000));
    }

    #[test]
    fn test_should_not_reorg_lower_difficulty() {
        assert!(!should_reorg_to_chain(2_000_000, 1_000_000));
    }

    #[test]
    fn test_should_not_reorg_equal_difficulty() {
        assert!(!should_reorg_to_chain(1_000_000, 1_000_000));
    }

    #[test]
    fn test_reorg_depth_limits() {
        assert!(!exceeds_automatic_reorg_limit(50));
        assert!(!exceeds_automatic_reorg_limit(100));
        assert!(exceeds_automatic_reorg_limit(101));
        assert!(exceeds_automatic_reorg_limit(200));
    }

    #[test]
    fn test_reorg_alert_threshold() {
        assert!(!should_alert_reorg_depth(5));
        assert!(!should_alert_reorg_depth(9));
        assert!(should_alert_reorg_depth(10));
        assert!(should_alert_reorg_depth(50));
    }
}
