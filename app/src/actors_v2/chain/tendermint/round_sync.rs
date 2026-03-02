//! Round synchronization and advancement logic.
//!
//! This module implements the Tendermint round advancement rules that allow
//! nodes to catch up when they receive messages from higher rounds:
//!
//! - 2/3+ prevotes from round R+x → goto Prevote(H, R+x)
//! - 2/3+ precommits for block from round R+x → COMMIT block
//! - 2/3+ precommits for NIL from round R+x → goto round R+x+1
//!
//! # Specification Reference
//!
//! Per the [Byzantine Consensus Algorithm](https://github.com/tendermint/tendermint/wiki/Byzantine-Consensus-Algorithm):
//!
//! > "After any +2/3 prevotes received at (H,R+x). --> goto Prevote(H,R+x)"
//! > "After any +2/3 precommits received at (H,R+x). --> goto Precommit(H,R+x)"
//!
//! **Important**: The threshold is **more than 2/3 voting power** (not f+1 messages).
//! The f+1 optimization was [proposed but rejected](https://github.com/tendermint/tendermint/issues/1496)
//! due to security concerns about Byzantine actors forcing inappropriate round advancement.

use super::types::{BlockHash, Height, Round, VotingPower};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════
// PENDING COMMIT STATE
// ═══════════════════════════════════════════════════════════════════════════

/// State for a commit that is pending block retrieval.
///
/// This handles the edge case where we receive 2/3+ precommits for a block
/// from a future round, but we never received the proposal containing the block.
/// This can happen due to network partitions or message loss.
///
/// # Workflow
///
/// 1. Receive 2/3+ precommits for block hash X
/// 2. Don't have block X locally → create PendingCommit
/// 3. Request block from peers
/// 4. When block arrives → verify hash matches → finalize commit
/// 5. If timeout → retry (up to max retries)
#[derive(Debug, Clone)]
pub struct PendingCommit {
    /// Hash of the block we need to commit
    pub block_hash: BlockHash,

    /// Round the commit is for
    pub round: Round,

    /// Correlation ID for logging/tracing
    pub correlation_id: Uuid,

    /// When the request was initiated
    pub requested_at: Instant,

    /// Number of retry attempts
    pub retry_count: u32,
}

impl PendingCommit {
    /// Create a new pending commit state
    pub fn new(block_hash: BlockHash, round: Round, correlation_id: Uuid) -> Self {
        Self {
            block_hash,
            round,
            correlation_id,
            requested_at: Instant::now(),
            retry_count: 0,
        }
    }

    /// Check if the pending commit has exceeded maximum wait time
    pub fn has_exceeded_timeout(&self, max_duration: std::time::Duration) -> bool {
        self.requested_at.elapsed() > max_duration
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOCK REQUEST/RESPONSE MESSAGES
// ═══════════════════════════════════════════════════════════════════════════

/// Request for a specific block from peers.
///
/// Sent when we need to commit a block but don't have the block data
/// (only received precommits proving the block is committed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRequest {
    /// Hash of the requested block
    pub block_hash: BlockHash,

    /// Height the block should be at
    pub height: Height,

    /// Correlation ID for request tracking
    pub correlation_id: Uuid,
}

/// Response containing a requested block.
///
/// This is a placeholder type - the actual block type will be
/// ConsensusBlock<MainnetEthSpec> from the crate::block module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockResponse {
    /// Hash of the block (for verification)
    pub block_hash: BlockHash,

    /// Height of the block
    pub height: Height,

    /// Original request correlation ID
    pub correlation_id: Uuid,
    // Note: The actual block data is passed separately via the existing
    // TendermintMessage::BlockResponse variant
}

// ═══════════════════════════════════════════════════════════════════════════
// FUTURE ROUND ACTION TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// Result of analyzing votes from a future round.
///
/// This enum determines what action to take when receiving messages
/// from rounds higher than our current round.
#[derive(Debug, Clone, PartialEq)]
pub enum FutureRoundAction {
    /// Not enough votes yet - continue waiting
    NoAction,

    /// Received 2/3+ prevotes - advance to Prevote step of that round
    AdvanceToPrevote {
        round: Round,
        /// Block hash that has 2/3+ prevotes (if any - for PoLC tracking)
        polka_block: Option<BlockHash>,
    },

    /// Received 2/3+ precommits for a specific block - COMMIT immediately
    CommitBlock { round: Round, block_hash: BlockHash },

    /// Received 2/3+ precommits for NIL - advance to next round
    AdvanceToNextRound {
        /// The round where we saw 2/3+ NIL precommits
        nil_round: Round,
        /// The round we should start (nil_round + 1)
        target_round: Round,
    },

    /// Received 2/3+ precommits (mixed or for block) - advance to Precommit step
    AdvanceToPrecommit {
        round: Round,
        /// Block hash that has 2/3+ precommits (if any, None means NIL)
        block_hash: Option<BlockHash>,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// THRESHOLD CALCULATIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Calculate 2/3+ threshold for voting power.
///
/// Tendermint requires MORE than 2/3 (strictly greater), not >= 2/3.
/// This is critical for safety - with exactly 2/3, a single Byzantine
/// validator could create two conflicting commits.
///
/// # Formula
///
/// `threshold = floor(total_power * 2 / 3) + 1`
///
/// # Examples
///
/// - `total_power=3`: threshold = 2+1 = 3 (need all 3)
/// - `total_power=4`: threshold = 2+1 = 3 (need 3 of 4)
/// - `total_power=10`: threshold = 6+1 = 7 (need 7 of 10)
/// - `total_power=100`: threshold = 66+1 = 67 (need 67 of 100)
#[inline]
pub fn two_thirds_threshold(total_power: VotingPower) -> VotingPower {
    (total_power * 2 / 3) + 1
}

/// Check if the given power meets the 2/3+ threshold
#[inline]
pub fn has_two_thirds(power: VotingPower, total_power: VotingPower) -> bool {
    power >= two_thirds_threshold(total_power)
}

// ═══════════════════════════════════════════════════════════════════════════
// VOTE ANALYSIS
// ═══════════════════════════════════════════════════════════════════════════

/// Analyze future round votes to determine required action.
///
/// This function implements the priority ordering for Tendermint actions:
///
/// 1. **Commit** (highest): 2/3+ precommits for a specific block
/// 2. **Next Round**: 2/3+ precommits for NIL
/// 3. **Precommit Step**: 2/3+ total precommits (mixed)
/// 4. **Prevote Step**: 2/3+ prevotes
///
/// # Arguments
///
/// * `round` - The future round being analyzed
/// * `prevote_power` - Total voting power of prevotes received
/// * `precommit_power` - Total voting power of precommits received
/// * `precommit_block_power` - Block with highest precommit power (if any)
/// * `precommit_nil_power` - Voting power of NIL precommits
/// * `total_power` - Total voting power of the validator set
///
/// # Returns
///
/// The appropriate `FutureRoundAction` based on the vote tallies.
pub fn analyze_future_round_votes(
    round: Round,
    prevote_power: VotingPower,
    precommit_power: VotingPower,
    precommit_block_power: Option<(BlockHash, VotingPower)>,
    precommit_nil_power: VotingPower,
    total_power: VotingPower,
) -> FutureRoundAction {
    let threshold = two_thirds_threshold(total_power);

    // Priority 1: Check for 2/3+ precommits on a specific block → COMMIT
    // This is the highest priority because it means the block is already decided
    if let Some((block_hash, power)) = precommit_block_power {
        if power >= threshold {
            return FutureRoundAction::CommitBlock { round, block_hash };
        }
    }

    // Priority 2: Check for 2/3+ precommits for NIL → advance to next round
    // NIL precommits mean the network decided to skip this round
    if precommit_nil_power >= threshold {
        return FutureRoundAction::AdvanceToNextRound {
            nil_round: round,
            target_round: round.saturating_add(1),
        };
    }

    // Priority 3: Check for 2/3+ total precommits → advance to Precommit step
    // This handles the rare case where we have 2/3+ precommits but split between
    // different blocks (shouldn't happen in normal operation, but handle it)
    if precommit_power >= threshold {
        return FutureRoundAction::AdvanceToPrecommit {
            round,
            block_hash: precommit_block_power.map(|(h, _)| h),
        };
    }

    // Priority 4: Check for 2/3+ prevotes → advance to Prevote step
    if prevote_power >= threshold {
        return FutureRoundAction::AdvanceToPrevote {
            round,
            polka_block: None, // Will be filled in by caller if available
        };
    }

    FutureRoundAction::NoAction
}

// ═══════════════════════════════════════════════════════════════════════════
// UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_types::H256;

    fn test_hash(byte: u8) -> BlockHash {
        let mut bytes = [0u8; 32];
        bytes[0] = byte;
        H256::from_slice(&bytes)
    }

    #[test]
    fn test_two_thirds_threshold() {
        // n=3: need > 2 = need 3 (all validators must agree)
        assert_eq!(two_thirds_threshold(3), 3);

        // n=4: need > 2.67 = need 3
        assert_eq!(two_thirds_threshold(4), 3);

        // n=10: need > 6.67 = need 7
        assert_eq!(two_thirds_threshold(10), 7);

        // n=15 (standard validator set): need > 10 = need 11
        assert_eq!(two_thirds_threshold(15), 11);

        // n=100: need > 66.67 = need 67
        assert_eq!(two_thirds_threshold(100), 67);
    }

    #[test]
    fn test_has_two_thirds() {
        // With total power 15
        assert!(!has_two_thirds(10, 15)); // 10 < 11
        assert!(has_two_thirds(11, 15)); // 11 >= 11
        assert!(has_two_thirds(15, 15)); // 15 >= 11

        // Edge case: 0 total power
        assert!(has_two_thirds(1, 0)); // threshold = 1
    }

    #[test]
    fn test_commit_takes_priority() {
        let block_hash = test_hash(1);
        let action = analyze_future_round_votes(
            5,                             // round
            100,                           // prevote_power (2/3+)
            80,                            // precommit_power (2/3+)
            Some((block_hash, 70)),        // 2/3+ precommit for block
            10,                            // precommit_nil_power
            100,                           // total_power
        );

        // Commit should take priority over other actions
        assert!(matches!(
            action,
            FutureRoundAction::CommitBlock {
                round: 5,
                block_hash: _
            }
        ));
    }

    #[test]
    fn test_nil_precommits_advance_to_next_round() {
        let action = analyze_future_round_votes(
            5,   // round
            50,  // prevote_power
            70,  // precommit_power
            None, // no block precommits
            70,  // precommit_nil_power (2/3+)
            100, // total_power
        );

        // Should advance to round 6, not stay in round 5
        assert!(matches!(
            action,
            FutureRoundAction::AdvanceToNextRound {
                nil_round: 5,
                target_round: 6
            }
        ));
    }

    #[test]
    fn test_precommit_power_advances_to_precommit_step() {
        let action = analyze_future_round_votes(
            5,   // round
            50,  // prevote_power (not 2/3+)
            70,  // precommit_power (2/3+)
            None, // no single block has majority
            30,  // precommit_nil_power (not 2/3+)
            100, // total_power
        );

        // Should advance to Precommit step
        assert!(matches!(
            action,
            FutureRoundAction::AdvanceToPrecommit {
                round: 5,
                block_hash: None
            }
        ));
    }

    #[test]
    fn test_prevotes_advance_to_prevote_step() {
        let action = analyze_future_round_votes(
            5,   // round
            70,  // prevote_power (2/3+)
            30,  // precommit_power (not 2/3+)
            None,
            0,
            100, // total_power
        );

        // Should advance to Prevote step of round 5
        assert!(matches!(
            action,
            FutureRoundAction::AdvanceToPrevote { round: 5, .. }
        ));
    }

    #[test]
    fn test_insufficient_votes_no_action() {
        let action = analyze_future_round_votes(
            5,   // round
            50,  // prevote_power (not 2/3+)
            30,  // precommit_power (not 2/3+)
            None,
            30,  // precommit_nil_power (not 2/3+)
            100, // total_power
        );

        assert_eq!(action, FutureRoundAction::NoAction);
    }

    #[test]
    fn test_pending_commit_creation() {
        let block_hash = test_hash(42);
        let correlation_id = Uuid::new_v4();
        let pending = PendingCommit::new(block_hash, 5, correlation_id);

        assert_eq!(pending.block_hash, block_hash);
        assert_eq!(pending.round, 5);
        assert_eq!(pending.correlation_id, correlation_id);
        assert_eq!(pending.retry_count, 0);
    }

    #[test]
    fn test_pending_commit_timeout() {
        let pending = PendingCommit::new(test_hash(1), 5, Uuid::new_v4());

        // Immediately after creation, should not be timed out
        assert!(!pending.has_exceeded_timeout(std::time::Duration::from_secs(10)));

        // With a zero duration, should be timed out
        assert!(pending.has_exceeded_timeout(std::time::Duration::ZERO));
    }

    #[test]
    fn test_edge_case_small_validator_sets() {
        // 2 validators: need 2/3 + 1 = 2 (both must agree)
        assert_eq!(two_thirds_threshold(2), 2);

        // 1 validator: need 0 + 1 = 1 (single validator always succeeds)
        assert_eq!(two_thirds_threshold(1), 1);
    }
}
