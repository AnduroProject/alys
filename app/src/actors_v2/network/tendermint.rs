//! Tendermint network layer for consensus message handling.
//!
//! This module provides:
//! - Wire protocol for Tendermint messages (MessagePack serialization)
//! - Message deduplication (LRU cache)
//! - Rate limiting per validator
//! - Proposal equivocation detection

use crate::actors_v2::chain::tendermint::{
    Height, Round, TendermintMessage, ValidatorId, VoteType,
};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};
use tiny_keccak::{Hasher, Keccak};

/// Configuration for Tendermint network layer
#[derive(Debug, Clone)]
pub struct TendermintNetworkConfig {
    /// Maximum messages in dedup cache
    pub dedup_cache_size: usize,
    /// Time-to-live for dedup entries
    pub dedup_ttl: Duration,
    /// Maximum proposals per validator per (height, round)
    pub max_proposals_per_slot: u32,
    /// Maximum votes per validator per (height, round)
    pub max_votes_per_slot: u32,
}

impl Default for TendermintNetworkConfig {
    fn default() -> Self {
        Self {
            dedup_cache_size: 10_000,
            dedup_ttl: Duration::from_secs(120), // 2 minutes
            max_proposals_per_slot: 1,
            max_votes_per_slot: 1, // 1 prevote + 1 precommit per validator per slot
        }
    }
}

/// Wire message format for Tendermint consensus messages.
///
/// Uses MessagePack serialization (already used in the codebase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendermintWireMessage {
    /// Protocol version
    pub version: u8,
    /// Message type discriminator
    pub msg_type: TendermintWireType,
    /// Serialized payload (MessagePack)
    pub payload: Vec<u8>,
    /// Sender identifier (peer ID or validator ID as string)
    pub sender: String,
    /// Unique message identifier for deduplication
    pub message_id: [u8; 32],
}

/// Wire message type discriminator
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TendermintWireType {
    Proposal = 0,
    Vote = 1,
    Timeout = 2,
    Evidence = 3,
    NewRound = 4,
    BlockRequest = 5,
    BlockResponse = 6,
}

impl TendermintWireMessage {
    /// Current protocol version
    pub const PROTOCOL_VERSION: u8 = 1;

    /// Serialize a TendermintMessage to wire format
    pub fn from_message(msg: &TendermintMessage, sender: String) -> Result<Self, WireError> {
        let (msg_type, payload) = match msg {
            TendermintMessage::Proposal(p) => {
                let payload = rmp_serde::to_vec(p).map_err(WireError::Serialization)?;
                (TendermintWireType::Proposal, payload)
            }
            TendermintMessage::Vote(v) => {
                let payload = rmp_serde::to_vec(v).map_err(WireError::Serialization)?;
                (TendermintWireType::Vote, payload)
            }
            TendermintMessage::Timeout(t) => {
                let payload = rmp_serde::to_vec(t).map_err(WireError::Serialization)?;
                (TendermintWireType::Timeout, payload)
            }
            TendermintMessage::Evidence(e) => {
                let payload = rmp_serde::to_vec(e).map_err(WireError::Serialization)?;
                (TendermintWireType::Evidence, payload)
            }
            TendermintMessage::NewRound {
                height,
                round,
                highest_known_round,
            } => {
                let payload =
                    rmp_serde::to_vec(&(*height, *round, *highest_known_round)).map_err(WireError::Serialization)?;
                (TendermintWireType::NewRound, payload)
            }
            TendermintMessage::BlockRequest { height } => {
                let payload = rmp_serde::to_vec(height).map_err(WireError::Serialization)?;
                (TendermintWireType::BlockRequest, payload)
            }
            TendermintMessage::BlockResponse { block, commit } => {
                let payload =
                    rmp_serde::to_vec(&(block, commit)).map_err(WireError::Serialization)?;
                (TendermintWireType::BlockResponse, payload)
            }
        };

        // Generate message ID from content
        let message_id = Self::compute_message_id(&payload, &sender, msg_type);

        Ok(Self {
            version: Self::PROTOCOL_VERSION,
            msg_type,
            payload,
            sender,
            message_id,
        })
    }

    /// Deserialize from wire format to TendermintMessage
    pub fn to_message(&self) -> Result<TendermintMessage, WireError> {
        if self.version != Self::PROTOCOL_VERSION {
            return Err(WireError::UnsupportedVersion(self.version));
        }

        match self.msg_type {
            TendermintWireType::Proposal => {
                let proposal = rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::Proposal(proposal))
            }
            TendermintWireType::Vote => {
                let vote = rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::Vote(vote))
            }
            TendermintWireType::Timeout => {
                let timeout = rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::Timeout(timeout))
            }
            TendermintWireType::Evidence => {
                let evidence = rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::Evidence(evidence))
            }
            TendermintWireType::NewRound => {
                let (height, round, highest_known_round): (Height, Round, Round) =
                    rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::NewRound {
                    height,
                    round,
                    highest_known_round,
                })
            }
            TendermintWireType::BlockRequest => {
                let height: Height =
                    rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::BlockRequest { height })
            }
            TendermintWireType::BlockResponse => {
                let (block, commit) =
                    rmp_serde::from_slice(&self.payload).map_err(WireError::Deserialization)?;
                Ok(TendermintMessage::BlockResponse { block, commit })
            }
        }
    }

    /// Serialize to bytes for network transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, WireError> {
        rmp_serde::to_vec(self).map_err(WireError::Serialization)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WireError> {
        rmp_serde::from_slice(bytes).map_err(WireError::Deserialization)
    }

    /// Compute unique message ID for deduplication
    fn compute_message_id(payload: &[u8], sender: &str, msg_type: TendermintWireType) -> [u8; 32] {
        let mut hasher = Keccak::v256();
        hasher.update(&[msg_type as u8]);
        hasher.update(sender.as_bytes());
        hasher.update(payload);
        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        output
    }
}

/// Wire protocol errors
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(#[from] rmp_serde::decode::Error),
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
}

/// Message deduplication cache.
///
/// Two-layer approach:
/// 1. Generic: LRU cache by message_id
/// 2. Proposal-specific: (height, round, proposer) -> block_hash for equivocation detection
pub struct MessageDeduplicator {
    /// Generic dedup cache: message_id -> timestamp
    seen_messages: LruCache<[u8; 32], Instant>,
    /// Proposal tracking for equivocation detection: (height, round, proposer) -> block_hash
    proposals: HashMap<(Height, Round, ValidatorId), [u8; 32]>,
    /// TTL for cache entries
    ttl: Duration,
}

impl MessageDeduplicator {
    /// Create a new deduplicator with given capacity and TTL
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            seen_messages: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
            proposals: HashMap::new(),
            ttl,
        }
    }

    /// Check if a message has been seen before.
    ///
    /// Returns `None` if the message is new.
    /// Returns `Some(DuplicateReason)` if it's a duplicate.
    pub fn check_and_insert(&mut self, msg: &TendermintWireMessage) -> Option<DuplicateReason> {
        let now = Instant::now();

        // Check generic dedup cache first
        if let Some(&seen_at) = self.seen_messages.get(&msg.message_id) {
            if now.duration_since(seen_at) < self.ttl {
                return Some(DuplicateReason::AlreadySeen);
            }
        }

        // For proposals, check equivocation
        if msg.msg_type == TendermintWireType::Proposal {
            if let Ok(TendermintMessage::Proposal(proposal)) = msg.to_message() {
                let key = (proposal.height, proposal.round, proposal.proposer);
                let block_hash = proposal.block_hash();

                if let Some(&existing_hash) = self.proposals.get(&key) {
                    if existing_hash != block_hash.0 {
                        return Some(DuplicateReason::ProposalEquivocation {
                            height: proposal.height,
                            round: proposal.round,
                            proposer: proposal.proposer,
                        });
                    }
                    // Same proposal, it's a duplicate
                    return Some(DuplicateReason::DuplicateProposal);
                }

                // New proposal, record it
                self.proposals.insert(key, block_hash.0);
            }
        }

        // Insert into generic cache
        self.seen_messages.put(msg.message_id, now);

        None
    }

    /// Prune old proposal entries.
    ///
    /// Call periodically or when height advances significantly.
    pub fn prune_old_proposals(&mut self, min_height: Height) {
        self.proposals.retain(|(h, _, _), _| *h >= min_height);
    }

    /// Get cache statistics
    pub fn stats(&self) -> DedupStats {
        DedupStats {
            messages_cached: self.seen_messages.len(),
            proposals_tracked: self.proposals.len(),
        }
    }
}

/// Reason why a message was identified as duplicate
#[derive(Debug, Clone)]
pub enum DuplicateReason {
    /// Message ID already seen within TTL
    AlreadySeen,
    /// Same proposal (height, round, proposer, block_hash)
    DuplicateProposal,
    /// Different proposal for same (height, round, proposer) - EQUIVOCATION!
    ProposalEquivocation {
        height: Height,
        round: Round,
        proposer: ValidatorId,
    },
}

/// Deduplication statistics
#[derive(Debug, Clone)]
pub struct DedupStats {
    pub messages_cached: usize,
    pub proposals_tracked: usize,
}

/// Rate limiter for Tendermint messages per validator.
///
/// Enforces:
/// - 1 proposal per (height, round) per proposer
/// - 1 prevote + 1 precommit per (height, round) per validator
pub struct RateLimiter {
    /// Proposal counts: (height, round, validator) -> count
    proposal_counts: HashMap<(Height, Round, ValidatorId), u32>,
    /// Prevote counts: (height, round, validator) -> count
    prevote_counts: HashMap<(Height, Round, ValidatorId), u32>,
    /// Precommit counts: (height, round, validator) -> count
    precommit_counts: HashMap<(Height, Round, ValidatorId), u32>,
    /// Max proposals per slot
    max_proposals: u32,
    /// Max votes per slot
    max_votes: u32,
}

impl RateLimiter {
    /// Create a new rate limiter with default limits
    pub fn new(max_proposals: u32, max_votes: u32) -> Self {
        Self {
            proposal_counts: HashMap::new(),
            prevote_counts: HashMap::new(),
            precommit_counts: HashMap::new(),
            max_proposals,
            max_votes,
        }
    }

    /// Check if a message should be rate-limited.
    ///
    /// Returns `None` if allowed.
    /// Returns `Some(RateLimitReason)` if rate-limited.
    pub fn check_and_count(&mut self, msg: &TendermintMessage) -> Option<RateLimitReason> {
        match msg {
            TendermintMessage::Proposal(p) => {
                let key = (p.height, p.round, p.proposer);
                let count = self.proposal_counts.entry(key).or_insert(0);
                if *count >= self.max_proposals {
                    return Some(RateLimitReason::TooManyProposals {
                        height: p.height,
                        round: p.round,
                        validator: p.proposer,
                    });
                }
                *count += 1;
            }
            TendermintMessage::Vote(v) => {
                let key = (v.height, v.round, v.validator);
                let counts = match v.vote_type {
                    VoteType::Prevote => &mut self.prevote_counts,
                    VoteType::Precommit => &mut self.precommit_counts,
                };
                let count = counts.entry(key).or_insert(0);
                if *count >= self.max_votes {
                    return Some(RateLimitReason::TooManyVotes {
                        height: v.height,
                        round: v.round,
                        validator: v.validator,
                        vote_type: v.vote_type,
                    });
                }
                *count += 1;
            }
            // Other message types are not rate-limited at this level
            _ => {}
        }
        None
    }

    /// Prune old entries when height advances
    pub fn prune_old_entries(&mut self, min_height: Height) {
        self.proposal_counts.retain(|(h, _, _), _| *h >= min_height);
        self.prevote_counts.retain(|(h, _, _), _| *h >= min_height);
        self.precommit_counts.retain(|(h, _, _), _| *h >= min_height);
    }
}

/// Reason why a message was rate-limited
#[derive(Debug, Clone)]
pub enum RateLimitReason {
    TooManyProposals {
        height: Height,
        round: Round,
        validator: ValidatorId,
    },
    TooManyVotes {
        height: Height,
        round: Round,
        validator: ValidatorId,
        vote_type: VoteType,
    },
}

/// Tendermint network handler combining dedup and rate limiting.
pub struct TendermintNetworkHandler {
    deduplicator: MessageDeduplicator,
    rate_limiter: RateLimiter,
    config: TendermintNetworkConfig,
}

impl TendermintNetworkHandler {
    /// Create a new handler with default config
    pub fn new() -> Self {
        Self::with_config(TendermintNetworkConfig::default())
    }

    /// Create a new handler with custom config
    pub fn with_config(config: TendermintNetworkConfig) -> Self {
        Self {
            deduplicator: MessageDeduplicator::new(config.dedup_cache_size, config.dedup_ttl),
            rate_limiter: RateLimiter::new(config.max_proposals_per_slot, config.max_votes_per_slot),
            config,
        }
    }

    /// Process an incoming wire message.
    ///
    /// Returns the deserialized message if it passes all checks.
    pub fn process_incoming(
        &mut self,
        wire_msg: &TendermintWireMessage,
    ) -> Result<TendermintMessage, ProcessError> {
        // Check deduplication first
        if let Some(reason) = self.deduplicator.check_and_insert(wire_msg) {
            return Err(ProcessError::Duplicate(reason));
        }

        // Deserialize
        let msg = wire_msg.to_message()?;

        // Check rate limiting
        if let Some(reason) = self.rate_limiter.check_and_count(&msg) {
            return Err(ProcessError::RateLimited(reason));
        }

        Ok(msg)
    }

    /// Prepare a message for outgoing transmission.
    pub fn prepare_outgoing(
        &self,
        msg: &TendermintMessage,
        sender: String,
    ) -> Result<TendermintWireMessage, WireError> {
        TendermintWireMessage::from_message(msg, sender)
    }

    /// Prune old state when height advances
    pub fn on_height_advance(&mut self, new_height: Height) {
        // Keep 10 blocks of history for late messages
        let min_height = new_height.saturating_sub(10);
        self.deduplicator.prune_old_proposals(min_height);
        self.rate_limiter.prune_old_entries(min_height);
    }

    /// Get deduplication statistics
    pub fn dedup_stats(&self) -> DedupStats {
        self.deduplicator.stats()
    }
}

impl Default for TendermintNetworkHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Error when processing incoming messages
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("Duplicate message: {0:?}")]
    Duplicate(DuplicateReason),
    #[error("Rate limited: {0:?}")]
    RateLimited(RateLimitReason),
    #[error("Wire error: {0}")]
    Wire(#[from] WireError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors_v2::chain::tendermint::{Vote, VoteType};
    use lighthouse_wrapper::bls::Signature;

    fn create_test_vote(height: Height, round: Round, validator: u8) -> TendermintMessage {
        TendermintMessage::Vote(Vote {
            height,
            round,
            vote_type: VoteType::Prevote,
            block_hash: Some(ethereum_types::H256::zero()),
            validator: ValidatorId::new(validator),
            timestamp: 0,
            signature: Signature::empty(),
        })
    }

    #[test]
    fn test_wire_message_roundtrip() {
        let msg = create_test_vote(100, 0, 1);
        let wire = TendermintWireMessage::from_message(&msg, "test-peer".to_string()).unwrap();
        let bytes = wire.to_bytes().unwrap();
        let wire2 = TendermintWireMessage::from_bytes(&bytes).unwrap();
        let msg2 = wire2.to_message().unwrap();

        match (&msg, &msg2) {
            (TendermintMessage::Vote(v1), TendermintMessage::Vote(v2)) => {
                assert_eq!(v1.height, v2.height);
                assert_eq!(v1.round, v2.round);
                assert_eq!(v1.validator, v2.validator);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_deduplicator() {
        let mut dedup = MessageDeduplicator::new(100, Duration::from_secs(60));
        let msg = create_test_vote(100, 0, 1);
        let wire = TendermintWireMessage::from_message(&msg, "test".to_string()).unwrap();

        // First time should pass
        assert!(dedup.check_and_insert(&wire).is_none());

        // Second time should be duplicate
        assert!(matches!(
            dedup.check_and_insert(&wire),
            Some(DuplicateReason::AlreadySeen)
        ));
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(1, 1);
        let msg = create_test_vote(100, 0, 1);

        // First vote should pass
        assert!(limiter.check_and_count(&msg).is_none());

        // Second vote from same validator at same (height, round) should be limited
        let reason = limiter.check_and_count(&msg);
        assert!(matches!(reason, Some(RateLimitReason::TooManyVotes { .. })));
    }

    #[test]
    fn test_handler_integration() {
        let mut handler = TendermintNetworkHandler::new();
        let msg = create_test_vote(100, 0, 1);
        let wire = handler.prepare_outgoing(&msg, "test".to_string()).unwrap();

        // First processing should succeed
        let result = handler.process_incoming(&wire);
        assert!(result.is_ok());

        // Second processing should be duplicate
        let result = handler.process_incoming(&wire);
        assert!(matches!(result, Err(ProcessError::Duplicate(_))));
    }
}
