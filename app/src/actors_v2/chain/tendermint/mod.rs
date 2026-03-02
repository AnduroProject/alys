//! Tendermint Consensus Implementation for Alys V2
//!
//! This module implements Tendermint-style two-phase BFT consensus,
//! providing instant finality for the Alys blockchain.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    TENDERMINT MODULE                        │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │  types.rs ──► messages.rs ──► state_machine.rs             │
//! │      │            │                  │                      │
//! │      └────────────┼──────────────────┘                      │
//! │                   ▼                                         │
//! │             vote_set.rs ◄── timeout.rs                      │
//! │                   │                                         │
//! │                   ▼                                         │
//! │               wal.rs                                        │
//! │                   │                                         │
//! │                   ▼                                         │
//! │           evidence.rs ── proposer.rs                        │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Safety Properties
//!
//! 1. **No Double Voting**: WAL ensures votes are recorded before broadcast
//! 2. **Locking Rules**: Once locked, only vote for locked block or NIL
//! 3. **Instant Finality**: 2/3+ precommits = irreversible commit
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::actors_v2::chain::tendermint::{
//!     TendermintState,
//!     Proposal,
//!     Vote,
//!     VoteType,
//! };
//! ```

// Core types used throughout the module
pub mod types;
pub use types::*;

// Protocol messages
pub mod messages;
pub use messages::*;

// Governance types (GovernanceUpdate, ValidatorUpdate, etc.)
pub mod governance;
pub use governance::*;

// Chain parameters (ChainParams, GovernableParam)
pub mod params;
pub use params::*;

// Peg-in types (PegInInfo, PegInCompensation)
pub mod pegin;
pub use pegin::*;

// State machine
pub mod state_machine;
pub use state_machine::{ConsensusAction, ConsensusEvent, StateSummary, TendermintState};

// Vote collection
pub mod vote_set;
pub use vote_set::{VoteError, VoteSet, VoteSetSummary};

// Timeout management
pub mod timeout;
pub use timeout::{TimeoutConfig, TimeoutError, TimeoutEvent, TimeoutScheduler};

// Write-ahead log
pub mod wal;
pub use wal::{ConsensusWAL, RecoveredState, WALConfig, WALEntry, WALError};

// Validation module
pub mod validation;
pub use validation::{
    check_for_equivocation, validate_last_commit, verify_commit, verify_future_proposal,
    verify_proposal, verify_vote, TendermintValidationError,
};

// Round synchronization (future round handling)
pub mod round_sync;
pub use round_sync::{
    analyze_future_round_votes, has_two_thirds, two_thirds_threshold, BlockRequest,
    BlockResponse, FutureRoundAction, PendingCommit,
};

// Future message storage
pub mod future_messages;
pub use future_messages::{FutureMessageStore, FutureRoundVotes};

/// Re-export common types for convenience
pub mod prelude {
    pub use super::future_messages::{FutureMessageStore, FutureRoundVotes};
    pub use super::governance::*;
    pub use super::messages::*;
    pub use super::params::*;
    pub use super::pegin::*;
    pub use super::round_sync::{FutureRoundAction, PendingCommit};
    pub use super::state_machine::{ConsensusAction, ConsensusEvent, TendermintState};
    pub use super::timeout::{TimeoutConfig, TimeoutScheduler};
    pub use super::types::*;
    pub use super::validation::{
        check_for_equivocation, validate_last_commit, verify_commit, verify_future_proposal,
        verify_proposal, verify_vote, TendermintValidationError,
    };
    pub use super::vote_set::VoteSet;
    pub use super::wal::ConsensusWAL;
}

// Integration tests
#[cfg(test)]
pub mod tests;
