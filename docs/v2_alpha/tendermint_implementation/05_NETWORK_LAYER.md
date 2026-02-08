# Implementation Plan: Network Layer Integration

## Overview

This document provides a comprehensive implementation guide for integrating Tendermint consensus messages with the NetworkActor. The network layer handles message broadcasting, deduplication, and routing between validators.

**Estimated Effort**: 1-2 weeks
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md`
- `04_CHAINACTOR_HANDLERS.md`
- `09_SYNC_ACTOR.md` (sync protocol messages)
**Files to Modify**:
- `app/src/actors_v2/network/messages.rs`
- `app/src/actors_v2/network/network_actor.rs`
**Files to Create**:
- `app/src/actors_v2/network/tendermint.rs`

**Cross-Document Type References**:
- `TendermintMessage`, `Proposal`, `Vote`, `EquivocationEvidence` → Defined in `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md`
- `BlockRequest`, `BlockResponse` → Defined in `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (TendermintMessage variants)
- `ChainMessage::TendermintProposal`, `ChainMessage::TendermintVote` → Defined in `04_CHAINACTOR_HANDLERS.md`
- `ValidatorId`, `BlockHash`, `Commit` → Defined in `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md`
- Sync messages (`RequestBlocks`, `BlocksResponse`) → Coordinated with `09_SYNC_ACTOR.md`

---

## 1. Network Architecture

### 1.1 Message Flow Overview

```mermaid
graph TB
    subgraph "Validator Node"
        CA[ChainActor]
        NA[NetworkActor]
        GS[Gossipsub]
    end

    subgraph "Network"
        P1[Peer 1]
        P2[Peer 2]
        P3[Peer 3]
    end

    CA -->|"broadcast_proposal()"| NA
    CA -->|"broadcast_vote()"| NA
    NA -->|"publish to topic"| GS
    GS -.->|gossip| P1
    GS -.->|gossip| P2
    GS -.->|gossip| P3

    P1 -.->|gossip| GS
    GS -->|"on_message"| NA
    NA -->|"TendermintVote"| CA
```

### 1.2 Gossipsub Topics

| Topic | Purpose | Message Type | Rate Limit |
|-------|---------|--------------|------------|
| `/alys/tendermint/proposals/1` | Block proposals | `Proposal` | 1/round/validator |
| `/alys/tendermint/votes/1` | Prevotes & Precommits | `Vote` | 2/round/validator |
| `/alys/tendermint/timeouts/1` | Timeout notifications | `Timeout` | 1/step/validator |
| `/alys/tendermint/evidence/1` | Equivocation evidence | `Evidence` | Rare |
| `/alys/tendermint/newround/1` | Round synchronization | `NewRound` | 1/round/validator |

### 1.3 Gossipsub Configuration

```rust
/// Gossipsub configuration for Tendermint consensus
pub fn tendermint_gossipsub_config() -> GossipsubConfig {
    GossipsubConfigBuilder::default()
        // Mesh parameters
        .mesh_n(8)                    // Target mesh size
        .mesh_n_low(6)                // Minimum before grafting
        .mesh_n_high(12)              // Maximum before pruning
        // Message parameters
        .max_transmit_size(1024 * 1024)  // 1MB max message size (for blocks)
        .heartbeat_interval(Duration::from_millis(700))
        .history_length(5)            // Number of heartbeats to retain
        .history_gossip(3)            // Heartbeats to gossip about
        // Validation
        .validate_messages()          // Enable message validation
        .validation_mode(ValidationMode::Strict)
        .build()
        .expect("Valid gossipsub config")
}
```

---

## 2. Message Definitions

### 2.1 NetworkMessage Extensions

```rust
// In network/messages.rs

use crate::actors_v2::chain::tendermint::{
    TendermintMessage, Proposal, Vote, EquivocationEvidence
};

/// Network message types
pub enum NetworkMessage {
    // ... existing variants ...

    // ═══════════════════════════════════════════════════════════════════
    // TENDERMINT MESSAGES
    // ═══════════════════════════════════════════════════════════════════

    /// Broadcast Tendermint message to all validators
    BroadcastTendermint {
        message: TendermintMessage,
        correlation_id: Option<Uuid>,
    },

    /// Received Tendermint message from network
    TendermintMessageReceived {
        message: TendermintMessage,
        peer_id: String,
        correlation_id: Option<Uuid>,
    },

    /// Subscribe to Tendermint topics
    SubscribeTendermintTopics {
        correlation_id: Option<Uuid>,
    },

    // ═══════════════════════════════════════════════════════════════════
    // SYNC PROTOCOL MESSAGES (See 09_SYNC_ACTOR.md)
    // ═══════════════════════════════════════════════════════════════════

    /// Query a peer for their tip height
    QueryTipHeight {
        correlation_id: Option<Uuid>,
    },

    /// Response with tip height
    TipHeightResponse {
        height: u64,
        block_hash: BlockHash,
        peer_id: PeerId,
    },

    /// Request blocks for sync (blocks include embedded last_commit)
    RequestBlocks {
        start_height: u64,
        count: u32,
        peer_id: Option<PeerId>,
        correlation_id: Option<Uuid>,
    },

    /// Response with blocks
    BlocksResponse {
        blocks: Vec<SignedConsensusBlock>,
        peer_id: PeerId,
        correlation_id: Option<Uuid>,
    },

    /// Request current commit for tip (before next block exists)
    RequestTipCommit {
        height: u64,
        block_hash: BlockHash,
        peer_id: PeerId,
    },

    /// Response with tip commit
    TipCommitResponse {
        commit: Commit,
        peer_id: PeerId,
    },

    /// Notify network layer of new height (for rate limiter cleanup)
    TendermintNewHeight {
        height: u64,
    },
}
```

### 2.2 Error Types

```rust
// In network/tendermint.rs

/// Serialization errors for Tendermint wire protocol
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("MessagePack serialization failed: {0}")]
    MsgPackEncode(#[from] rmp_serde::encode::Error),

    #[error("MessagePack deserialization failed: {0}")]
    MsgPackDecode(#[from] rmp_serde::decode::Error),

    #[error("Unsupported message type for wire protocol")]
    UnsupportedMessageType,

    #[error("Invalid wire message version: {0}")]
    InvalidVersion(u8),
}

// Add to existing NetworkError enum in network/messages.rs
impl NetworkError {
    // Additional variants needed for Tendermint:
    // Serialization(String),
    // Deserialization(String),
    // ActorMailbox(String),
    // ChainError(String),
    // RateLimited { validator: ValidatorId, message_type: String },
    // InvalidMessage(String),
}
```

### 2.3 Wire Protocol

```rust
// In network/tendermint.rs

use serde::{Deserialize, Serialize};
use super::*;

/// Wire format for Tendermint messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendermintWireMessage {
    /// Protocol version (for future compatibility)
    pub version: u8,

    /// Message type tag
    pub msg_type: TendermintWireType,

    /// Serialized message content
    pub payload: Vec<u8>,

    /// Sender peer ID (for deduplication)
    pub sender: String,

    /// Message ID (for deduplication)
    pub message_id: [u8; 32],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum TendermintWireType {
    Proposal = 0,
    Prevote = 1,
    Precommit = 2,
    NewRound = 3,
    Evidence = 4,
    Timeout = 5,
    BlockRequest = 6,
    BlockResponse = 7,
}

impl TendermintWireMessage {
    /// Create wire message from Tendermint message
    pub fn from_message(
        message: &TendermintMessage,
        sender: String,
    ) -> Result<Self, SerializationError> {
        let (msg_type, payload) = match message {
            TendermintMessage::Proposal(p) => {
                (TendermintWireType::Proposal, rmp_serde::to_vec(p)?)
            }
            TendermintMessage::Vote(v) => {
                let wire_type = match v.vote_type {
                    VoteType::Prevote => TendermintWireType::Prevote,
                    VoteType::Precommit => TendermintWireType::Precommit,
                };
                (wire_type, rmp_serde::to_vec(v)?)
            }
            TendermintMessage::Evidence(e) => {
                (TendermintWireType::Evidence, rmp_serde::to_vec(e)?)
            }
            TendermintMessage::NewRound { height, round, highest_known_round } => {
                (TendermintWireType::NewRound, rmp_serde::to_vec(&(*height, *round, *highest_known_round))?)
            }
            TendermintMessage::Timeout(t) => {
                (TendermintWireType::Timeout, rmp_serde::to_vec(t)?)
            }
            TendermintMessage::BlockRequest { height } => {
                (TendermintWireType::BlockRequest, rmp_serde::to_vec(height)?)
            }
            TendermintMessage::BlockResponse { block, commit } => {
                (TendermintWireType::BlockResponse, rmp_serde::to_vec(&(block, commit))?)
            }
        };

        // Generate message ID for deduplication
        let message_id = Self::compute_message_id(&payload, &sender);

        Ok(Self {
            version: 1,
            msg_type,
            payload,
            sender,
            message_id,
        })
    }

    /// Deserialize back to Tendermint message
    pub fn into_message(self) -> Result<TendermintMessage, SerializationError> {
        match self.msg_type {
            TendermintWireType::Proposal => {
                let proposal: Proposal = rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::Proposal(proposal))
            }
            TendermintWireType::Prevote | TendermintWireType::Precommit => {
                let vote: Vote = rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::Vote(vote))
            }
            TendermintWireType::Evidence => {
                let evidence: EquivocationEvidence = rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::Evidence(evidence))
            }
            TendermintWireType::NewRound => {
                let (height, round, highest): (u64, u32, u32) =
                    rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::NewRound {
                    height,
                    round,
                    highest_known_round: highest,
                })
            }
            TendermintWireType::Timeout => {
                let timeout: Timeout = rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::Timeout(timeout))
            }
            TendermintWireType::BlockRequest => {
                let height: u64 = rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::BlockRequest { height })
            }
            TendermintWireType::BlockResponse => {
                let (block, commit): (ConsensusBlock, Commit) =
                    rmp_serde::from_slice(&self.payload)?;
                Ok(TendermintMessage::BlockResponse { block, commit })
            }
        }
    }

    /// Compute message ID for deduplication
    fn compute_message_id(payload: &[u8], sender: &str) -> [u8; 32] {
        use tiny_keccak::{Hasher, Keccak};
        let mut hasher = Keccak::v256();
        hasher.update(payload);
        hasher.update(sender.as_bytes());
        let mut output = [0u8; 32];
        hasher.finalize(&mut output);
        output
    }
}
```

---

## 3. Message Deduplication

### 3.1 Deduplication Cache

```rust
// In network/tendermint.rs

use lru::LruCache;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

/// Cache for deduplicating Tendermint messages
///
/// Prevents processing the same message multiple times when received
/// via different gossip paths.
pub struct TendermintMessageDedup {
    /// Seen message IDs with their arrival time
    seen: LruCache<[u8; 32], Instant>,

    /// Time after which messages can be seen again
    ttl: Duration,
}

impl TendermintMessageDedup {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            seen: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
            ttl,
        }
    }

    /// Check if message is a duplicate, and mark as seen if not
    ///
    /// Returns true if this is a NEW message (not seen before)
    pub fn check_and_mark(&mut self, message_id: [u8; 32]) -> bool {
        let now = Instant::now();

        // Check if we've seen this message recently
        if let Some(seen_at) = self.seen.get(&message_id) {
            if now.duration_since(*seen_at) < self.ttl {
                return false; // Duplicate
            }
        }

        // Mark as seen
        self.seen.put(message_id, now);
        true // New message
    }

    /// Clear expired entries (called periodically)
    pub fn cleanup(&mut self) {
        let now = Instant::now();
        let expired: Vec<_> = self.seen
            .iter()
            .filter(|(_, &seen_at)| now.duration_since(seen_at) >= self.ttl)
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            self.seen.pop(&id);
        }
    }
}

// Proposal-specific deduplication (only one proposal per round per proposer)
pub struct ProposalDedup {
    /// Key: (height, round, proposer) -> proposal hash
    seen: LruCache<(u64, u32, ValidatorId), BlockHash>,
}

impl ProposalDedup {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }

    /// Check if proposal is duplicate or conflicting
    pub fn check(&mut self, proposal: &Proposal) -> ProposalDedupResult {
        let key = (proposal.height, proposal.round, proposal.proposer);

        if let Some(&existing_hash) = self.seen.get(&key) {
            if existing_hash == proposal.block_hash() {
                ProposalDedupResult::Duplicate
            } else {
                ProposalDedupResult::Conflicting { existing_hash }
            }
        } else {
            self.seen.put(key, proposal.block_hash());
            ProposalDedupResult::New
        }
    }

    /// Cleanup proposals before the given height
    pub fn cleanup_before_height(&mut self, height: u64) {
        // LRU cache handles eviction automatically, but we can proactively
        // remove old entries to free memory
        let old_keys: Vec<_> = self.seen
            .iter()
            .filter(|((h, _, _), _)| *h < height)
            .map(|(k, _)| *k)
            .collect();

        for key in old_keys {
            self.seen.pop(&key);
        }
    }
}

pub enum ProposalDedupResult {
    New,
    Duplicate,
    Conflicting { existing_hash: BlockHash },
}
```

---

## 4. NetworkActor Handler

### 4.1 Tendermint Message Handling

```rust
// In network/network_actor.rs

impl NetworkActor {
    /// Handle request to broadcast Tendermint message
    async fn handle_broadcast_tendermint(
        &mut self,
        message: TendermintMessage,
    ) -> Result<(), NetworkError> {
        // 1. Create wire message
        let wire_msg = TendermintWireMessage::from_message(
            &message,
            self.local_peer_id.to_string(),
        )?;

        // 2. Serialize for network
        let bytes = rmp_serde::to_vec(&wire_msg)
            .map_err(|e| NetworkError::Serialization(e.to_string()))?;

        // 3. Determine topic
        let topic = match &message {
            TendermintMessage::Proposal(_) => TOPIC_TENDERMINT_PROPOSALS,
            TendermintMessage::Vote(_) => TOPIC_TENDERMINT_VOTES,
            TendermintMessage::Timeout(_) => TOPIC_TENDERMINT_TIMEOUTS,
            TendermintMessage::Evidence(_) => TOPIC_TENDERMINT_EVIDENCE,
            TendermintMessage::NewRound { .. } => TOPIC_TENDERMINT_NEWROUND,
            TendermintMessage::BlockRequest { .. } |
            TendermintMessage::BlockResponse { .. } => {
                // Block sync uses request-response, not gossipsub
                return self.handle_sync_message(message).await;
            }
        };

        // 4. Publish via gossipsub
        self.publish_to_topic(topic, bytes).await?;

        debug!(
            message_type = message.message_type(),
            topic = topic,
            "Broadcast Tendermint message"
        );

        Ok(())
    }

    /// Handle incoming Tendermint message from gossipsub
    async fn on_tendermint_gossip(
        &mut self,
        data: Vec<u8>,
        peer_id: PeerId,
        topic: &str,
    ) -> Result<(), NetworkError> {
        // 1. Deserialize wire message
        let wire_msg: TendermintWireMessage = rmp_serde::from_slice(&data)
            .map_err(|e| NetworkError::Deserialization(e.to_string()))?;

        // 2. Check deduplication
        if !self.tendermint_dedup.check_and_mark(wire_msg.message_id) {
            debug!("Duplicate Tendermint message, ignoring");
            return Ok(());
        }

        // 3. Convert to Tendermint message
        let message = wire_msg.into_message()?;

        // 4. Additional validation for proposals
        if let TendermintMessage::Proposal(ref proposal) = message {
            match self.proposal_dedup.check(proposal) {
                ProposalDedupResult::New => {}
                ProposalDedupResult::Duplicate => {
                    debug!("Duplicate proposal, ignoring");
                    return Ok(());
                }
                ProposalDedupResult::Conflicting { existing_hash } => {
                    warn!(
                        proposer = %proposal.proposer,
                        height = proposal.height,
                        round = proposal.round,
                        existing = ?existing_hash,
                        new = ?proposal.block_hash(),
                        "Conflicting proposal detected - possible equivocation"
                    );
                    // Continue processing - ChainActor will create evidence
                }
            }
        }

        // 5. Apply rate limiting
        match &message {
            TendermintMessage::Proposal(p) => {
                if !self.rate_limiter.check_proposal(p) {
                    self.report_peer_misbehavior(&peer_id, MisbehaviorReason::RateLimitExceeded);
                    return Ok(());
                }
            }
            TendermintMessage::Vote(v) => {
                if !self.rate_limiter.check_vote(v) {
                    self.report_peer_misbehavior(&peer_id, MisbehaviorReason::RateLimitExceeded);
                    return Ok(());
                }
            }
            _ => {}
        }

        // 6. Forward to ChainActor
        if let Some(ref chain_actor) = self.chain_actor {
            let chain_msg = match &message {
                TendermintMessage::Proposal(p) => ChainMessage::TendermintProposal {
                    proposal: p.clone(),
                    peer_id: Some(peer_id.to_string()),
                    correlation_id: None,
                },
                TendermintMessage::Vote(v) => ChainMessage::TendermintVote {
                    vote: v.clone(),
                    peer_id: Some(peer_id.to_string()),
                    correlation_id: None,
                },
                TendermintMessage::Timeout(t) => ChainMessage::TendermintTimeout {
                    height: t.height,
                    round: t.round,
                    step: t.step,
                    correlation_id: None,
                },
                TendermintMessage::NewRound { height, round, highest_known_round } => {
                    // NewRound messages are used for view synchronization
                    // Forward to ChainActor to potentially trigger catch-up
                    ChainMessage::TendermintNewRoundHint {
                        height: *height,
                        round: *round,
                        highest_known_round: *highest_known_round,
                        peer_id: Some(peer_id.to_string()),
                        correlation_id: None,
                    }
                }
                TendermintMessage::Evidence(e) => {
                    // Handle evidence separately
                    self.handle_evidence(e.clone()).await?;
                    return Ok(());
                }
                TendermintMessage::BlockRequest { .. } |
                TendermintMessage::BlockResponse { .. } => {
                    // Sync messages handled via request-response protocol
                    return Ok(());
                }
            };

            chain_actor.send(chain_msg).await
                .map_err(|e| NetworkError::ActorMailbox(e.to_string()))?
                .map_err(|e| NetworkError::ChainError(e.to_string()))?;
        }

        Ok(())
    }
}
```

### 4.2 Topic Subscription

```rust
// In network/network_actor.rs

/// Tendermint topic names
const TOPIC_TENDERMINT_PROPOSALS: &str = "/alys/tendermint/proposals/1";
const TOPIC_TENDERMINT_VOTES: &str = "/alys/tendermint/votes/1";
const TOPIC_TENDERMINT_TIMEOUTS: &str = "/alys/tendermint/timeouts/1";
const TOPIC_TENDERMINT_EVIDENCE: &str = "/alys/tendermint/evidence/1";
const TOPIC_TENDERMINT_NEWROUND: &str = "/alys/tendermint/newround/1";

impl NetworkActor {
    /// Subscribe to all Tendermint gossipsub topics
    pub async fn subscribe_tendermint_topics(&mut self) -> Result<(), NetworkError> {
        let topics = [
            TOPIC_TENDERMINT_PROPOSALS,
            TOPIC_TENDERMINT_VOTES,
            TOPIC_TENDERMINT_TIMEOUTS,
            TOPIC_TENDERMINT_EVIDENCE,
            TOPIC_TENDERMINT_NEWROUND,
        ];

        for topic in topics {
            self.subscribe_topic(topic).await?;
            info!("Subscribed to Tendermint topic: {}", topic);
        }

        Ok(())
    }

    /// Handle messages from gossipsub based on topic
    async fn on_gossipsub_message(
        &mut self,
        peer_id: PeerId,
        topic: String,
        data: Vec<u8>,
    ) {
        match topic.as_str() {
            TOPIC_TENDERMINT_PROPOSALS |
            TOPIC_TENDERMINT_VOTES |
            TOPIC_TENDERMINT_TIMEOUTS |
            TOPIC_TENDERMINT_EVIDENCE |
            TOPIC_TENDERMINT_NEWROUND => {
                if let Err(e) = self.on_tendermint_gossip(data, peer_id, &topic).await {
                    warn!(topic, error = %e, "Error processing Tendermint gossip");
                }
            }
            // ... handle other topics (blocks, transactions) ...
            _ => {
                debug!(topic, "Unknown gossipsub topic");
            }
        }
    }

    /// Handle new height notification from ChainActor
    ///
    /// Called when consensus commits a block and advances to next height.
    /// Used to cleanup rate limiter and dedup caches.
    pub fn handle_tendermint_new_height(&mut self, height: u64) {
        self.rate_limiter.on_new_height(height);
        // Optionally cleanup old dedup entries
        self.tendermint_dedup.cleanup();
        self.proposal_dedup.cleanup_before_height(height.saturating_sub(2));
    }
}
```

---

## 5. Rate Limiting

### 5.1 Per-Validator Rate Limits

```rust
// In network/tendermint.rs

use std::collections::HashMap;

/// Rate limiter for Tendermint messages
pub struct TendermintRateLimiter {
    /// Proposals per validator: (height, round) -> count
    proposals: HashMap<ValidatorId, HashMap<(u64, u32), u32>>,

    /// Votes per validator: (height, round, vote_type) -> count
    votes: HashMap<ValidatorId, HashMap<(u64, u32, VoteType), u32>>,

    /// Maximum allowed per category
    max_proposals_per_round: u32,  // Should be 1
    max_votes_per_round: u32,       // Should be 1 per type
}

impl TendermintRateLimiter {
    pub fn new() -> Self {
        Self {
            proposals: HashMap::new(),
            votes: HashMap::new(),
            max_proposals_per_round: 1,
            max_votes_per_round: 1,
        }
    }

    /// Check and record proposal, returns false if rate limited
    pub fn check_proposal(&mut self, proposal: &Proposal) -> bool {
        let validator_proposals = self.proposals
            .entry(proposal.proposer)
            .or_default();

        let key = (proposal.height, proposal.round);
        let count = validator_proposals.entry(key).or_insert(0);

        if *count >= self.max_proposals_per_round {
            warn!(
                proposer = %proposal.proposer,
                height = proposal.height,
                round = proposal.round,
                count = *count,
                "Rate limiting proposals from validator"
            );
            return false;
        }

        *count += 1;
        true
    }

    /// Check and record vote, returns false if rate limited
    pub fn check_vote(&mut self, vote: &Vote) -> bool {
        let validator_votes = self.votes
            .entry(vote.validator)
            .or_default();

        let key = (vote.height, vote.round, vote.vote_type);
        let count = validator_votes.entry(key).or_insert(0);

        if *count >= self.max_votes_per_round {
            warn!(
                validator = %vote.validator,
                height = vote.height,
                round = vote.round,
                vote_type = ?vote.vote_type,
                count = *count,
                "Rate limiting votes from validator"
            );
            return false;
        }

        *count += 1;
        true
    }

    /// Cleanup old entries (call on new height)
    pub fn on_new_height(&mut self, new_height: u64) {
        // Remove entries for heights before new_height - 1
        for proposals in self.proposals.values_mut() {
            proposals.retain(|(h, _), _| *h >= new_height.saturating_sub(1));
        }
        for votes in self.votes.values_mut() {
            votes.retain(|(h, _, _), _| *h >= new_height.saturating_sub(1));
        }
    }
}
```

---

## 6. Evidence Handling

### 6.1 Equivocation Evidence Handler

```rust
// In network/tendermint.rs

impl NetworkActor {
    /// Handle received equivocation evidence
    ///
    /// Evidence is received when a validator is detected double-voting
    /// or double-proposing. This is forwarded to ChainActor for:
    /// 1. Verification of the evidence
    /// 2. Storage in the evidence pool
    /// 3. Inclusion in a future block for slashing
    pub async fn handle_evidence(
        &mut self,
        evidence: EquivocationEvidence,
    ) -> Result<(), NetworkError> {
        info!(
            validator = %evidence.validator(),
            height = evidence.height(),
            evidence_type = ?evidence.evidence_type(),
            "Received equivocation evidence"
        );

        // Basic validation before forwarding
        if !evidence.is_valid_format() {
            warn!("Malformed evidence received, discarding");
            return Err(NetworkError::InvalidMessage("Malformed evidence".into()));
        }

        // Check if evidence is for a recent height (not too old)
        let current_height = self.get_current_height();
        let max_evidence_age = self.config.max_evidence_age_blocks;
        if evidence.height() + max_evidence_age < current_height {
            debug!(
                evidence_height = evidence.height(),
                current_height,
                max_age = max_evidence_age,
                "Evidence too old, discarding"
            );
            return Ok(());
        }

        // Forward to ChainActor for verification and storage
        if let Some(ref chain_actor) = self.chain_actor {
            chain_actor.send(ChainMessage::TendermintEvidence {
                evidence: evidence.clone(),
                peer_id: None,
                correlation_id: None,
            }).await
                .map_err(|e| NetworkError::ActorMailbox(e.to_string()))?
                .map_err(|e| NetworkError::ChainError(e.to_string()))?;
        }

        // Broadcast to other peers (they may not have seen it)
        self.broadcast_evidence(evidence).await?;

        Ok(())
    }

    /// Broadcast evidence to the network
    async fn broadcast_evidence(
        &mut self,
        evidence: EquivocationEvidence,
    ) -> Result<(), NetworkError> {
        let wire_msg = TendermintWireMessage::from_message(
            &TendermintMessage::Evidence(evidence),
            self.local_peer_id.to_string(),
        )?;

        let bytes = rmp_serde::to_vec(&wire_msg)
            .map_err(|e| NetworkError::Serialization(e.to_string()))?;

        self.publish_to_topic(TOPIC_TENDERMINT_EVIDENCE, bytes).await
    }
}
```

---

## 7. NetworkActor State Initialization

### 7.1 Tendermint-Related Fields

```rust
// In network/network_actor.rs

/// NetworkActor with Tendermint consensus support
pub struct NetworkActor {
    // ═══════════════════════════════════════════════════════════════════
    // EXISTING FIELDS
    // ═══════════════════════════════════════════════════════════════════

    /// Local peer ID
    local_peer_id: PeerId,
    /// Swarm for libp2p networking
    swarm: Swarm<AlysNetworkBehaviour>,
    /// Configuration
    config: NetworkConfig,

    // ═══════════════════════════════════════════════════════════════════
    // ACTOR REFERENCES
    // ═══════════════════════════════════════════════════════════════════

    /// ChainActor for forwarding consensus messages
    chain_actor: Option<Addr<ChainActor>>,
    /// SyncActor for block sync coordination
    sync_actor: Option<Addr<SyncActor>>,
    /// StorageActor for block retrieval
    storage_actor: Option<Addr<StorageActor>>,

    // ═══════════════════════════════════════════════════════════════════
    // TENDERMINT-SPECIFIC FIELDS
    // ═══════════════════════════════════════════════════════════════════

    /// Message deduplication cache
    tendermint_dedup: TendermintMessageDedup,
    /// Proposal-specific deduplication
    proposal_dedup: ProposalDedup,
    /// Rate limiter for consensus messages
    rate_limiter: TendermintRateLimiter,
    /// Current consensus height (for filtering old messages)
    current_height: u64,
}

impl NetworkActor {
    /// Create a new NetworkActor with Tendermint support
    pub fn new(config: NetworkConfig, swarm: Swarm<AlysNetworkBehaviour>) -> Self {
        Self {
            local_peer_id: *swarm.local_peer_id(),
            swarm,
            config,
            chain_actor: None,
            sync_actor: None,
            storage_actor: None,
            // Initialize Tendermint components
            tendermint_dedup: TendermintMessageDedup::new(
                10_000,  // Capacity for 10k messages
                Duration::from_secs(120),  // 2 minute TTL
            ),
            proposal_dedup: ProposalDedup::new(1_000),  // 1k proposals
            rate_limiter: TendermintRateLimiter::new(),
            current_height: 0,
        }
    }

    /// Set ChainActor address (called via SetChainActor message)
    pub fn set_chain_actor(&mut self, addr: Addr<ChainActor>) {
        self.chain_actor = Some(addr);
    }

    /// Get current consensus height
    fn get_current_height(&self) -> u64 {
        self.current_height
    }
}
```

### 7.2 Handler Registration

```rust
// Add to the main handler match in network_actor.rs

impl Handler<NetworkMessage> for NetworkActor {
    type Result = ResponseFuture<Result<NetworkResponse, NetworkError>>;

    fn handle(&mut self, msg: NetworkMessage, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            // ... existing handlers ...

            NetworkMessage::BroadcastTendermint { message, correlation_id } => {
                let actor = self.clone();
                Box::pin(async move {
                    actor.handle_broadcast_tendermint(message).await?;
                    Ok(NetworkResponse::Broadcasted {
                        message_id: correlation_id.map(|u| u.to_string()).unwrap_or_default(),
                    })
                })
            }

            NetworkMessage::SubscribeTendermintTopics { correlation_id } => {
                let actor = self.clone();
                Box::pin(async move {
                    actor.subscribe_tendermint_topics().await?;
                    Ok(NetworkResponse::Started)
                })
            }

            NetworkMessage::TendermintNewHeight { height } => {
                self.handle_tendermint_new_height(height);
                Box::pin(async { Ok(NetworkResponse::Started) })
            }

            NetworkMessage::QueryTipHeight { correlation_id } => {
                // Query all connected peers for their tip height
                let actor = self.clone();
                Box::pin(async move {
                    actor.query_peer_heights().await?;
                    Ok(NetworkResponse::Started)
                })
            }

            NetworkMessage::RequestBlocks { start_height, count, peer_id, correlation_id } => {
                let actor = self.clone();
                Box::pin(async move {
                    let request_id = actor.request_blocks_from_peer(
                        start_height, count, peer_id
                    ).await?;
                    Ok(NetworkResponse::BlocksRequested {
                        peer_count: 1,
                        request_id,
                    })
                })
            }

            // ... other handlers ...
        }
    }
}
```

---

## 8. Peer Scoring

### 8.1 Gossipsub Peer Scoring

```rust
// In network/tendermint.rs

/// Configure peer scoring for Tendermint message validation
pub fn tendermint_peer_score_params() -> PeerScoreParams {
    PeerScoreParams {
        // Topic-specific scoring
        topics: hashmap! {
            TOPIC_TENDERMINT_PROPOSALS.into() => TopicScoreParams {
                // Penalize invalid proposals heavily
                invalid_message_deliveries_weight: -100.0,
                invalid_message_deliveries_decay: 0.5,
                first_message_deliveries_cap: 10.0,
                ..Default::default()
            },
            TOPIC_TENDERMINT_VOTES.into() => TopicScoreParams {
                invalid_message_deliveries_weight: -50.0,
                invalid_message_deliveries_decay: 0.7,
                first_message_deliveries_cap: 20.0,
                ..Default::default()
            },
            TOPIC_TENDERMINT_EVIDENCE.into() => TopicScoreParams {
                first_message_deliveries_weight: 10.0,
                invalid_message_deliveries_weight: -200.0,
                ..Default::default()
            },
        },
        behaviour_penalty_weight: -10.0,
        behaviour_penalty_decay: 0.9,
        ip_colocation_factor_weight: -50.0,
        ip_colocation_factor_threshold: 3,
        ..Default::default()
    }
}

#[derive(Debug)]
pub enum MisbehaviorReason {
    InvalidSignature,
    Equivocation,
    RateLimitExceeded,
    InvalidMessage,
    OldMessage,
}

impl NetworkActor {
    pub fn report_peer_misbehavior(&mut self, peer_id: &PeerId, reason: MisbehaviorReason) {
        warn!(peer = %peer_id, reason = ?reason, "Reported peer misbehavior");
        // Apply gossipsub score penalty
    }
}
```

---

## 9. Metrics

### 9.1 Network Metrics

```rust
use prometheus::{IntCounterVec, HistogramVec, Opts, Registry};

lazy_static! {
    /// Tendermint messages sent by type
    static ref TENDERMINT_MESSAGES_SENT: IntCounterVec = IntCounterVec::new(
        Opts::new("tendermint_messages_sent", "Tendermint messages broadcast"),
        &["message_type"]
    ).unwrap();

    /// Tendermint messages received by type
    static ref TENDERMINT_MESSAGES_RECEIVED: IntCounterVec = IntCounterVec::new(
        Opts::new("tendermint_messages_received", "Tendermint messages received"),
        &["message_type", "result"]  // result: new, duplicate, rate_limited
    ).unwrap();

    /// Message propagation latency
    static ref TENDERMINT_MESSAGE_LATENCY: HistogramVec = HistogramVec::new(
        prometheus::HistogramOpts::new(
            "tendermint_message_latency_seconds",
            "Time from message creation to receipt"
        ),
        &["message_type"]
    ).unwrap();
}

pub fn register_tendermint_network_metrics(registry: &Registry) {
    registry.register(Box::new(TENDERMINT_MESSAGES_SENT.clone())).ok();
    registry.register(Box::new(TENDERMINT_MESSAGES_RECEIVED.clone())).ok();
    registry.register(Box::new(TENDERMINT_MESSAGE_LATENCY.clone())).ok();
}
```

---

## 10. Complete Flow Example

### 10.1 Broadcasting a Proposal

```mermaid
sequenceDiagram
    participant CA as ChainActor
    participant NA as NetworkActor
    participant GS as Gossipsub
    participant P1 as Peer 1
    participant P2 as Peer 2

    CA->>NA: BroadcastTendermint(Proposal)
    NA->>NA: Create TendermintWireMessage
    NA->>NA: Serialize to bytes
    NA->>GS: publish(TOPIC_PROPOSALS, bytes)
    GS->>P1: gossip
    GS->>P2: gossip

    Note over P1,P2: Peers process and forward

    P1-->>NA: gossip back (ignored as duplicate)
    NA->>NA: check_and_mark() -> false
    Note over NA: Message already seen
```

### 10.2 Receiving a Vote

```mermaid
sequenceDiagram
    participant P as Peer
    participant GS as Gossipsub
    participant NA as NetworkActor
    participant DD as Dedup Cache
    participant RL as Rate Limiter
    participant CA as ChainActor

    P->>GS: gossip(vote)
    GS->>NA: on_gossipsub_message

    NA->>NA: Deserialize TendermintWireMessage
    NA->>DD: check_and_mark(message_id)
    DD-->>NA: true (new message)

    NA->>RL: check_vote(vote)
    RL-->>NA: true (not rate limited)

    NA->>NA: Convert to TendermintMessage::Vote
    NA->>CA: ChainMessage::TendermintVote

    CA->>CA: Process vote
```

---

## 11. Testing Strategy

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_message_roundtrip() {
        let vote = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::repeat_byte(0xAB)),
            validator: ValidatorId(5),
            signature: BLSSignature::empty(),
        };

        let message = TendermintMessage::Vote(vote.clone());
        let wire = TendermintWireMessage::from_message(&message, "peer1".into()).unwrap();
        let recovered = wire.into_message().unwrap();

        match recovered {
            TendermintMessage::Vote(v) => {
                assert_eq!(v.height, vote.height);
                assert_eq!(v.round, vote.round);
                assert_eq!(v.validator, vote.validator);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_deduplication() {
        let mut dedup = TendermintMessageDedup::new(100, Duration::from_secs(60));

        let message_id = [0xAB; 32];

        // First time - new
        assert!(dedup.check_and_mark(message_id));

        // Second time - duplicate
        assert!(!dedup.check_and_mark(message_id));
    }

    #[test]
    fn test_rate_limiting() {
        let mut limiter = TendermintRateLimiter::new();

        let vote1 = Vote {
            height: 100,
            round: 0,
            vote_type: VoteType::Prevote,
            block_hash: Some(BlockHash::repeat_byte(0xAB)),
            validator: ValidatorId(5),
            signature: BLSSignature::empty(),
        };

        // First vote allowed
        assert!(limiter.check_vote(&vote1));

        // Second vote for same (height, round, type) blocked
        assert!(!limiter.check_vote(&vote1));

        // Different round allowed
        let vote2 = Vote { round: 1, ..vote1.clone() };
        assert!(limiter.check_vote(&vote2));
    }
}
```

---

## 12. Checklist

### Core Consensus Message Handling
- [ ] Add `BroadcastTendermint` to `NetworkMessage`
- [ ] Create `network/tendermint.rs` module
- [ ] Implement `TendermintWireMessage` with all message types
- [ ] Implement `TendermintMessageDedup`
- [ ] Implement `ProposalDedup`
- [ ] Implement `TendermintRateLimiter`
- [ ] Add all Tendermint topic constants (proposals, votes, timeouts, evidence, newround)
- [ ] Implement `subscribe_tendermint_topics()`
- [ ] Implement `handle_broadcast_tendermint()`
- [ ] Implement `on_tendermint_gossip()`

### Error Types
- [ ] Define `SerializationError` enum
- [ ] Add Tendermint-specific variants to `NetworkError`
- [ ] Implement error conversions

### Evidence Handling
- [ ] Implement `handle_evidence()` handler
- [ ] Implement `broadcast_evidence()` function
- [ ] Add evidence age validation

### Sync Protocol Messages
- [ ] Add `QueryTipHeight` message variant
- [ ] Add `TipHeightResponse` message variant
- [ ] Add `RequestBlocks` message variant (coordinate with existing code)
- [ ] Add `BlocksResponse` message variant
- [ ] Add `RequestTipCommit` message variant
- [ ] Add `TipCommitResponse` message variant
- [ ] Implement sync message handlers

### NetworkActor State
- [ ] Add `tendermint_dedup` field to NetworkActor
- [ ] Add `proposal_dedup` field to NetworkActor
- [ ] Add `rate_limiter` field to NetworkActor
- [ ] Add `current_height` field to NetworkActor
- [ ] Implement initialization in `NetworkActor::new()`
- [ ] Add `TendermintNewHeight` message for height notifications

### Peer Scoring
- [ ] Implement `tendermint_peer_score_params()`
- [ ] Implement `report_peer_misbehavior()`
- [ ] Define `MisbehaviorReason` enum
- [ ] Configure gossipsub with peer scoring

### Gossipsub Configuration
- [ ] Implement `tendermint_gossipsub_config()`
- [ ] Configure mesh parameters
- [ ] Configure message size limits
- [ ] Enable message validation

### Metrics
- [ ] Add `TENDERMINT_MESSAGES_SENT` counter
- [ ] Add `TENDERMINT_MESSAGES_RECEIVED` counter
- [ ] Add `TENDERMINT_MESSAGE_LATENCY` histogram
- [ ] Implement `register_tendermint_network_metrics()`

### Testing
- [ ] Write unit tests for wire message roundtrip
- [ ] Write unit tests for deduplication
- [ ] Write unit tests for rate limiting
- [ ] Write unit tests for evidence handling
- [ ] Integration test with multiple peers
- [ ] Integration test for sync protocol

### Integration with Existing Code
- [ ] Coordinate with existing `RequestBlocks` in messages.rs
- [ ] Coordinate with existing `HandleBlockResponse` in messages.rs
- [ ] Ensure V0 compatibility during transition
- [ ] Update handler registration in NetworkActor

---

## 13. Integration Notes

### 13.1 Existing Code Compatibility

The current `messages.rs` already defines some sync-related messages. Tendermint integration should:

1. **Extend, don't replace**: Add Tendermint-specific messages alongside existing ones
2. **Reuse where possible**: The existing `RequestBlocks` and `HandleBlockResponse` can be reused for Tendermint sync
3. **Namespace separation**: Use `Tendermint*` prefix for consensus-specific messages

### 13.2 V0 Transition

During the migration period:
- Both V0 (Aura) and V2 (Tendermint) message types will coexist
- Topic subscription should be conditional based on consensus mode
- Rate limiting and dedup should only apply to Tendermint messages

---

*Implementation Plan Version: 2.0*
*Last Updated: February 2026*
*Changes in 2.0: Added sync protocol messages, error types, evidence handling, NetworkActor state, peer scoring, gossipsub configuration, expanded checklist*
