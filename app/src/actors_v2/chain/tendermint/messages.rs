//! Tendermint protocol messages.
//!
//! This module defines all messages exchanged between validators during
//! Tendermint consensus. Messages are designed for:
//! - Efficient serialization (serde + SSZ for network)
//! - BLS signature compatibility (using existing lighthouse_wrapper)
//! - Integration with the Alys actor message system
//!
//! # Message Flow
//!
//! ```text
//! Height H, Round R:
//!
//!   Leader                      Validators
//!     │                              │
//!     │──── Proposal(block) ────────►│
//!     │                              │
//!     │◄─── Prevote(block_hash) ─────│
//!     │                              │
//!     │     [collect 2/3+ prevotes]  │
//!     │                              │
//!     │◄─── Precommit(block_hash) ───│
//!     │                              │
//!     │     [collect 2/3+ precommits]│
//!     │                              │
//!     │          COMMITTED           │
//! ```

use super::types::*;
use crate::block::ConsensusBlock;
use lighthouse_wrapper::bls::Signature as BLSSignature;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};

/// A block proposal from the designated proposer
///
/// The proposer creates a Proposal containing:
/// - The candidate block
/// - Proof-of-Lock (if locked from a previous round)
/// - Their signature authorizing the proposal
///
/// # Proof-of-Lock (POL)
///
/// If the proposer is locked on a block from a previous round, they MUST
/// propose that block and include the `pol_round` indicating when they locked.
/// This prevents equivocation and ensures progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Block height being proposed
    pub height: Height,

    /// Round within this height
    pub round: Round,

    /// The proposed block
    pub block: ConsensusBlock<MainnetEthSpec>,

    /// Proof-of-Lock round (if proposer is locked on this block)
    ///
    /// - `None`: Proposer is not locked, free to propose any valid block
    /// - `Some(r)`: Proposer locked at round `r`, must propose locked block
    pub pol_round: Option<Round>,

    /// Validator ID of the proposer
    pub proposer: ValidatorId,

    /// BLS signature over the proposal
    ///
    /// Signs: `hash(height || round || block_hash || pol_round)`
    pub signature: BLSSignature,
}

impl Proposal {
    /// Get the hash of the proposed block
    pub fn block_hash(&self) -> BlockHash {
        // Use the existing signing_root from ConsensusBlock
        let signing_root = tree_hash::merkle_root(&rmp_serde::to_vec(&self.block).unwrap(), 0);
        BlockHash::from_slice(signing_root.as_bytes())
    }

    /// Create the signing root for this proposal
    ///
    /// The signing root is: `keccak256(height || round || block_hash || pol_round_flag || pol_round)`
    pub fn signing_root(&self) -> Hash256 {
        let mut hasher = Keccak::v256();
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.round.to_le_bytes());
        hasher.update(self.block_hash().as_bytes());

        match self.pol_round {
            Some(r) => {
                hasher.update(&[1u8]); // Has POL
                hasher.update(&r.to_le_bytes());
            }
            None => {
                hasher.update(&[0u8]); // No POL
            }
        }

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        Hash256::from_slice(&output)
    }

    /// Verify the proposal signature
    pub fn verify_signature(&self, public_key: &lighthouse_wrapper::bls::PublicKey) -> bool {
        let signing_root = self.signing_root();
        self.signature.verify(public_key, signing_root)
    }
}

/// A vote (prevote or precommit) from a validator
///
/// Votes are the core mechanism for reaching consensus:
/// - **Prevote**: "I've seen a valid proposal and am willing to commit to it"
/// - **Precommit**: "I've seen 2/3+ prevotes and am committing to this block"
///
/// # NIL Votes
///
/// A vote with `block_hash = None` is a NIL vote, indicating:
/// - No valid proposal was received in time (timeout)
/// - The validator cannot vote for the proposed block (invalid, conflicts with lock)
///
/// NIL votes are important for liveness - they allow the round to progress
/// even when no block can be committed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// Block height being voted on
    pub height: Height,

    /// Round within this height
    pub round: Round,

    /// Type of vote (Prevote or Precommit)
    pub vote_type: VoteType,

    /// Hash of the block being voted for, or None for NIL
    pub block_hash: Option<BlockHash>,

    /// Validator casting the vote
    pub validator: ValidatorId,

    /// Timestamp when the vote was cast (Unix timestamp)
    pub timestamp: u64,

    /// BLS signature over the vote
    ///
    /// Signs: `hash(vote_type || height || round || block_hash_or_nil)`
    pub signature: BLSSignature,
}

impl Vote {
    /// Check if this is a NIL vote
    pub fn is_nil(&self) -> bool {
        self.block_hash.is_none()
    }

    /// Create the signing root for this vote
    ///
    /// The signing root is: `keccak256(vote_type || height || round || block_hash_or_zeros)`
    pub fn signing_root(&self) -> Hash256 {
        let mut hasher = Keccak::v256();
        hasher.update(&[self.vote_type as u8]);
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.round.to_le_bytes());

        match &self.block_hash {
            Some(hash) => hasher.update(hash.as_bytes()),
            None => hasher.update(&[0u8; 32]), // NIL represented as zeros
        }

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        Hash256::from_slice(&output)
    }

    /// Verify the vote signature
    pub fn verify_signature(&self, public_key: &lighthouse_wrapper::bls::PublicKey) -> bool {
        let signing_root = self.signing_root();
        self.signature.verify(public_key, signing_root)
    }

    /// Create a signed vote
    pub fn new_signed(
        height: Height,
        round: Round,
        vote_type: VoteType,
        block_hash: Option<BlockHash>,
        validator: ValidatorId,
        timestamp: u64,
        keypair: &lighthouse_wrapper::bls::Keypair,
    ) -> Self {
        let mut vote = Self {
            height,
            round,
            vote_type,
            block_hash,
            validator,
            timestamp,
            signature: BLSSignature::empty(), // Placeholder
        };

        let signing_root = vote.signing_root();
        vote.signature = keypair.sk.sign(signing_root);
        vote
    }
}

/// Timeout event for progressing stuck rounds
///
/// When a validator doesn't receive expected messages within the timeout period,
/// they emit a timeout to trigger round progression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeout {
    /// Height of the timeout
    pub height: Height,

    /// Round that timed out
    pub round: Round,

    /// Step that timed out (Propose, Prevote, or Precommit)
    pub step: TendermintStep,

    /// Validator reporting the timeout
    pub validator: ValidatorId,

    /// Signature proving authenticity
    pub signature: BLSSignature,
}

impl Timeout {
    /// Create the signing root for this timeout
    pub fn signing_root(&self) -> Hash256 {
        let mut hasher = Keccak::v256();
        hasher.update(b"timeout");
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.round.to_le_bytes());
        hasher.update(&[self.step as u8]);

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        Hash256::from_slice(&output)
    }
}

/// Evidence of validator equivocation (double voting or double proposing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivocationEvidence {
    /// Type of equivocation
    pub kind: EquivocationType,

    /// Validator who equivocated
    pub culprit: ValidatorId,

    /// Height at which equivocation occurred
    pub height: Height,

    /// Round at which equivocation occurred
    pub round: Round,

    /// First vote/proposal
    pub vote_a: Vote,

    /// Conflicting vote/proposal
    pub vote_b: Vote,
}

impl EquivocationEvidence {
    /// Get the height of the equivocation
    pub fn height(&self) -> Height {
        self.height
    }

    /// Compute the evidence hash for deduplication
    pub fn evidence_hash(&self) -> [u8; 32] {
        let mut hasher = Keccak::v256();
        hasher.update(&[self.kind as u8]);
        hasher.update(&self.culprit.0.to_le_bytes());
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.round.to_le_bytes());
        // Include both vote hashes
        hasher.update(self.vote_a.signing_root().as_bytes());
        hasher.update(self.vote_b.signing_root().as_bytes());

        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        output
    }
}

/// Type of equivocation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EquivocationType {
    /// Validator signed two different prevotes for the same (height, round)
    DoublePrevote = 0,
    /// Validator signed two different precommits for the same (height, round)
    DoublePrecommit = 1,
    /// Validator proposed two different blocks for the same (height, round)
    DoubleProposal = 2,
}

/// High-level message enum for network transmission
///
/// This enum wraps all Tendermint protocol messages for unified handling
/// in the network layer.
///
/// # Network Topics
///
/// Each message type corresponds to a Gossipsub topic:
/// - `Proposal` -> `/alys/tendermint/proposals/1`
/// - `Vote` -> `/alys/tendermint/votes/1`
/// - `Timeout` -> `/alys/tendermint/timeouts/1`
/// - `Evidence` -> `/alys/tendermint/evidence/1`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TendermintMessage {
    /// Block proposal from the designated proposer
    Proposal(Proposal),

    /// Vote (prevote or precommit) from a validator
    Vote(Vote),

    /// Timeout notification
    Timeout(Timeout),

    /// Equivocation evidence (double voting/proposing)
    Evidence(EquivocationEvidence),

    /// New round notification (used for view synchronization)
    NewRound {
        height: Height,
        round: Round,
        /// Highest round for which we've seen 2/3+ any votes
        highest_known_round: Round,
    },

    /// Request for a committed block (for syncing)
    BlockRequest { height: Height },

    /// Response with a committed block and its proof
    BlockResponse {
        block: ConsensusBlock<MainnetEthSpec>,
        commit: Commit,
    },
}

impl TendermintMessage {
    /// Get the height this message pertains to
    pub fn height(&self) -> Option<Height> {
        match self {
            Self::Proposal(p) => Some(p.height),
            Self::Vote(v) => Some(v.height),
            Self::Timeout(t) => Some(t.height),
            Self::Evidence(e) => Some(e.height()),
            Self::NewRound { height, .. } => Some(*height),
            Self::BlockRequest { height } => Some(*height),
            Self::BlockResponse { block, .. } => Some(block.slot),
        }
    }

    /// Get the message type name for logging
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::Proposal(_) => "Proposal",
            Self::Vote(v) => match v.vote_type {
                VoteType::Prevote => "Prevote",
                VoteType::Precommit => "Precommit",
            },
            Self::Timeout(_) => "Timeout",
            Self::Evidence(_) => "Evidence",
            Self::NewRound { .. } => "NewRound",
            Self::BlockRequest { .. } => "BlockRequest",
            Self::BlockResponse { .. } => "BlockResponse",
        }
    }

    /// Get the Gossipsub topic for this message type
    pub fn topic(&self) -> &'static str {
        match self {
            Self::Proposal(_) => "/alys/tendermint/proposals/1",
            Self::Vote(_) => "/alys/tendermint/votes/1",
            Self::Timeout(_) => "/alys/tendermint/timeouts/1",
            Self::Evidence(_) => "/alys/tendermint/evidence/1",
            Self::NewRound { .. } => "/alys/tendermint/newround/1",
            Self::BlockRequest { .. } => "/alys/tendermint/blockreq/1",
            Self::BlockResponse { .. } => "/alys/tendermint/blockresp/1",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vote_is_nil() {
        let vote = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: None,
            validator: ValidatorId(0),
            timestamp: 0,
            signature: BLSSignature::empty(),
        };
        assert!(vote.is_nil());

        let vote_with_hash = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::zero()),
            validator: ValidatorId(0),
            timestamp: 0,
            signature: BLSSignature::empty(),
        };
        assert!(!vote_with_hash.is_nil());
    }

    #[test]
    fn test_vote_signing_root_differs_by_type() {
        let prevote = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::zero()),
            validator: ValidatorId(0),
            timestamp: 0,
            signature: BLSSignature::empty(),
        };

        let precommit = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Precommit,
            block_hash: Some(BlockHash::zero()),
            validator: ValidatorId(0),
            timestamp: 0,
            signature: BLSSignature::empty(),
        };

        assert_ne!(prevote.signing_root(), precommit.signing_root());
    }

    #[test]
    fn test_tendermint_message_height() {
        let vote_msg = TendermintMessage::Vote(Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: None,
            validator: ValidatorId(0),
            timestamp: 0,
            signature: BLSSignature::empty(),
        });
        assert_eq!(vote_msg.height(), Some(100));

        let new_round_msg = TendermintMessage::NewRound {
            height: 200,
            round: 1,
            highest_known_round: 0,
        };
        assert_eq!(new_round_msg.height(), Some(200));
    }

    #[test]
    fn test_tendermint_message_type() {
        let vote_msg = TendermintMessage::Vote(Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: None,
            validator: ValidatorId(0),
            timestamp: 0,
            signature: BLSSignature::empty(),
        });
        assert_eq!(vote_msg.message_type(), "Prevote");
    }
}
