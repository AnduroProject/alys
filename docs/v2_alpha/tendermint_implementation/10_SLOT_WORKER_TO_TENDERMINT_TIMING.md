# Implementation Plan: Slot Worker to Tendermint Timing Migration

## Overview

This document provides a comprehensive implementation guide for replacing the Aura-based slot worker timing mechanism with Tendermint consensus timing. The fundamental change is from fixed time-slot based block production to height+round based consensus-driven block production.

**Estimated Effort**: 3-5 days
**Dependencies**:
- `02_STATE_MACHINE.md`
- `04_CHAINACTOR_HANDLERS.md`
- `08_TIMEOUT_MANAGEMENT.md`
**Files to Modify**:
- `app/src/actors_v2/slot_worker.rs` → Replace entirely
- `app/src/aura.rs` → Partially remove/adapt
- `app/src/actors_v2/chain/actor.rs`
**Files to Create**:
- `app/src/actors_v2/tendermint_driver.rs`

---

## 1. Conceptual Change

### 1.1 Current (Aura Slot Worker)

```
TIME-BASED BLOCK PRODUCTION:

  t=0      t=6s     t=12s    t=18s    t=24s
   │        │        │        │        │
   ▼        ▼        ▼        ▼        ▼
 ┌────┐  ┌────┐  ┌────┐  ┌────┐  ┌────┐
 │Slot│  │Slot│  │Slot│  │Slot│  │Slot│
 │ 0  │  │ 1  │  │ 2  │  │ 3  │  │ 4  │
 └────┘  └────┘  └────┘  └────┘  └────┘
   │        │        │        │        │
   ▼        ▼        ▼        ▼        ▼
  V0       V1       V2       V0       V1    (round-robin)
```

**Key Characteristics**:
- Fixed 6-second slots
- Proposer = `slot % num_validators`
- Time-based triggering (wall clock)
- No consensus confirmation before next slot
- Blocks may be orphaned if network is slow

### 1.2 New (Tendermint Timing)

```
CONSENSUS-BASED BLOCK PRODUCTION:

  Height 100                    Height 101
     │                              │
     ▼                              ▼
 ┌────────────────────────┐    ┌────────────────────────┐
 │ Round 0                │    │ Round 0                │
 │ ┌──────┐ ┌──────┐     │    │ ┌──────┐ ┌──────┐     │
 │ │Propose│→│Prevote│→...│    │ │Propose│→│Prevote│→...│
 │ └──────┘ └──────┘     │    │ └──────┘ └──────┘     │
 │    V0        ✓        │    │    V1        ✓        │
 └────────────────────────┘    └────────────────────────┘
           │                              │
           ▼                              ▼
     COMMIT (finalized)           COMMIT (finalized)
```

**Key Characteristics**:
- Height+Round based (not time slots)
- Proposer = `(height + round) % num_validators`
- Event-driven (consensus completion triggers next height)
- Block finalized before advancing
- No orphans possible

---

## 2. Current Slot Worker Analysis

### 2.1 File: `slot_worker.rs` (247 lines)

```rust
// CURRENT STRUCTURE:
pub struct AuraSlotWorkerV2 {
    last_slot: u64,
    slot_duration: Duration,        // Fixed 6 seconds
    authorities: Vec<PublicKey>,
    maybe_signer: Option<Keypair>,
    chain_actor: Addr<ChainActor>,
}

impl AuraSlotWorkerV2 {
    fn claim_slot(&self, slot: u64) -> bool {
        // slot % num_authorities == our_index
    }

    async fn on_slot(&self, slot: u64) {
        // Send ChainMessage::ProduceBlock
    }

    async fn next_slot(&mut self) -> u64 {
        // Wait for wall clock to reach next slot boundary
    }

    pub async fn start_slot_worker(mut self) {
        loop {
            let slot = self.next_slot().await;
            self.on_slot(slot).await;
        }
    }
}
```

### 2.2 Dependencies on `aura.rs`

| Function | Purpose | Tendermint Equivalent |
|----------|---------|----------------------|
| `slot_from_timestamp()` | Calculate current slot | Not needed (event-driven) |
| `slot_author()` | Round-robin selection | `get_proposer(height, round)` |
| `time_until_next_slot()` | Sleep calculation | Timeout scheduling |
| `duration_now()` | Current timestamp | Keep for block timestamps |

---

## 3. New Tendermint Driver

### 3.1 Core Structure

```rust
// NEW FILE: app/src/actors_v2/tendermint_driver.rs

use actix::prelude::*;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::actors_v2::chain::{ChainActor, ChainMessage};
use crate::actors_v2::chain::tendermint::{
    TendermintState, TendermintStep, TimeoutScheduler, ValidatorId,
};

/// Tendermint Consensus Driver
///
/// Replaces AuraSlotWorkerV2. Instead of time-based slots, this driver:
/// 1. Listens for consensus events (new height, timeout, votes)
/// 2. Triggers appropriate actions (propose, prevote, precommit)
/// 3. Advances height only after commit
pub struct TendermintDriver {
    /// Our validator identity (None if observer)
    validator_id: Option<ValidatorId>,

    /// Validator set for proposer selection
    validator_set: Arc<ValidatorSet>,

    /// Timeout configuration
    timeout_scheduler: TimeoutScheduler,

    /// Address of ChainActor
    chain_actor: Addr<ChainActor>,

    /// Current consensus state (shared with ChainActor)
    state: Arc<RwLock<TendermintState>>,

    /// Pending timeout handle (for cancellation)
    pending_timeout: Option<SpawnHandle>,
}

/// Messages to the Tendermint Driver
#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum TendermintDriverMessage {
    /// Start consensus at a new height
    NewHeight { height: u64 },

    /// Timeout expired for current step
    Timeout { height: u64, round: u32, step: TendermintStep },

    /// Move to next round (after timeout or 2/3+ NIL)
    NextRound { height: u64, round: u32 },

    /// Block committed, advance to next height
    Committed { height: u64 },

    /// Stop the driver
    Stop,
}
```

### 3.2 Proposer Selection (Replaces `slot_author`)

```rust
impl TendermintDriver {
    /// Get proposer for a given height and round
    ///
    /// Proposer selection: (height + round) % num_validators
    /// This ensures:
    /// - Different proposer each height (fair distribution)
    /// - Round advancement rotates proposer (liveness)
    pub fn get_proposer(&self, height: u64, round: u32) -> ValidatorId {
        let index = ((height + round as u64) % self.validator_set.len() as u64) as usize;
        self.validator_set.get_by_index(index)
    }

    /// Check if we are the proposer for this height/round
    pub fn is_proposer(&self, height: u64, round: u32) -> bool {
        match &self.validator_id {
            Some(our_id) => self.get_proposer(height, round) == *our_id,
            None => false,  // Observers never propose
        }
    }
}
```

### 3.3 Timeout Scheduling (Replaces `next_slot`)

```rust
impl TendermintDriver {
    /// Schedule timeout for current step
    ///
    /// Unlike Aura's fixed slot boundaries, Tendermint uses adaptive timeouts
    /// that increase with round number (exponential backoff).
    fn schedule_timeout(&mut self, ctx: &mut Context<Self>) {
        // Cancel any pending timeout
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        let state = self.state.blocking_read();
        let height = state.height;
        let round = state.round;
        let step = state.step.clone();
        drop(state);

        // Calculate timeout based on step and round
        let timeout = match step {
            TendermintStep::Propose => self.timeout_scheduler.propose_timeout(round),
            TendermintStep::Prevote => self.timeout_scheduler.prevote_timeout(round),
            TendermintStep::Precommit => self.timeout_scheduler.precommit_timeout(round),
            TendermintStep::Commit => return,  // No timeout during commit
        };

        tracing::debug!(
            height = height,
            round = round,
            step = ?step,
            timeout_ms = timeout.as_millis(),
            "Scheduling timeout"
        );

        // Schedule timeout message
        let handle = ctx.notify_later(
            TendermintDriverMessage::Timeout { height, round, step },
            timeout,
        );
        self.pending_timeout = Some(handle);
    }

    /// Handle timeout expiration
    fn handle_timeout(
        &mut self,
        height: u64,
        round: u32,
        step: TendermintStep,
        ctx: &mut Context<Self>,
    ) {
        // Verify timeout is still relevant
        let state = self.state.blocking_read();
        if state.height != height || state.round != round || state.step != step {
            tracing::trace!(
                "Ignoring stale timeout for h={} r={} s={:?}",
                height, round, step
            );
            return;
        }
        drop(state);

        tracing::warn!(
            height = height,
            round = round,
            step = ?step,
            "Timeout expired"
        );

        // Take action based on step
        match step {
            TendermintStep::Propose => {
                // Proposer failed - send NIL prevote
                self.send_nil_prevote(height, round, ctx);
            }
            TendermintStep::Prevote => {
                // Didn't get 2/3+ prevotes - send NIL precommit
                self.send_nil_precommit(height, round, ctx);
            }
            TendermintStep::Precommit => {
                // Didn't get 2/3+ precommits - advance round
                self.advance_round(height, round + 1, ctx);
            }
            TendermintStep::Commit => {
                unreachable!("No timeout during commit");
            }
        }
    }
}
```

### 3.4 Height Advancement (Event-Driven)

```rust
impl TendermintDriver {
    /// Start consensus for a new height
    ///
    /// Called after:
    /// 1. Block committed at previous height
    /// 2. Startup (after loading chain head from storage)
    /// 3. Sync completion
    pub fn start_height(&mut self, height: u64, ctx: &mut Context<Self>) {
        tracing::info!(height = height, "Starting consensus for new height");

        // Reset state for new height
        {
            let mut state = self.state.blocking_write();
            state.new_round(height, 0);
        }

        // If we're the proposer, trigger proposal
        if self.is_proposer(height, 0) {
            self.trigger_propose(height, 0, ctx);
        }

        // Schedule propose timeout
        self.schedule_timeout(ctx);
    }

    /// Advance to next round within same height
    fn advance_round(&mut self, height: u64, round: u32, ctx: &mut Context<Self>) {
        tracing::info!(
            height = height,
            round = round,
            "Advancing to round {}", round
        );

        // Update state
        {
            let mut state = self.state.blocking_write();
            state.new_round(height, round);
        }

        // If we're the proposer for this round, trigger proposal
        if self.is_proposer(height, round) {
            self.trigger_propose(height, round, ctx);
        }

        // Schedule propose timeout
        self.schedule_timeout(ctx);
    }

    /// Called when block is committed
    fn on_commit(&mut self, height: u64, ctx: &mut Context<Self>) {
        tracing::info!(height = height, "Block committed, advancing to next height");

        // Cancel pending timeouts
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        // Start next height immediately (event-driven, no waiting)
        self.start_height(height + 1, ctx);
    }
}
```

### 3.5 Interaction with ChainActor

```rust
impl TendermintDriver {
    /// Trigger proposal creation (we are the proposer)
    fn trigger_propose(&self, height: u64, round: u32, ctx: &mut Context<Self>) {
        tracing::info!(
            height = height,
            round = round,
            "We are proposer - triggering block proposal"
        );

        let chain_actor = self.chain_actor.clone();

        // Spawn async task to request block production
        ctx.spawn(async move {
            let result = chain_actor.send(ChainMessage::TendermintPropose {
                height,
                round,
            }).await;

            match result {
                Ok(Ok(_)) => {
                    tracing::debug!(height = height, round = round, "Proposal created");
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        height = height,
                        round = round,
                        error = ?e,
                        "Failed to create proposal"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        height = height,
                        round = round,
                        error = %e,
                        "ChainActor mailbox error"
                    );
                }
            }
        }.into_actor(self));
    }

    /// Send NIL prevote (proposal timeout or invalid proposal)
    fn send_nil_prevote(&self, height: u64, round: u32, ctx: &mut Context<Self>) {
        if self.validator_id.is_none() {
            return;  // Observers don't vote
        }

        tracing::debug!(height = height, round = round, "Sending NIL prevote");

        let chain_actor = self.chain_actor.clone();

        ctx.spawn(async move {
            let _ = chain_actor.send(ChainMessage::TendermintCastVote {
                height,
                round,
                vote_type: VoteType::Prevote,
                block_hash: None,  // NIL
            }).await;
        }.into_actor(self));
    }

    /// Send NIL precommit (prevote timeout)
    fn send_nil_precommit(&self, height: u64, round: u32, ctx: &mut Context<Self>) {
        if self.validator_id.is_none() {
            return;
        }

        tracing::debug!(height = height, round = round, "Sending NIL precommit");

        let chain_actor = self.chain_actor.clone();

        ctx.spawn(async move {
            let _ = chain_actor.send(ChainMessage::TendermintCastVote {
                height,
                round,
                vote_type: VoteType::Precommit,
                block_hash: None,  // NIL
            }).await;
        }.into_actor(self));
    }
}
```

---

## 4. Actor Implementation

### 4.1 Actor Trait

```rust
impl Actor for TendermintDriver {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mode = if self.validator_id.is_some() {
            "validator"
        } else {
            "observer"
        };

        tracing::info!(
            mode = mode,
            validators = self.validator_set.len(),
            "TendermintDriver started"
        );

        // Query ChainActor for current height and start consensus
        let chain_actor = self.chain_actor.clone();
        let addr = ctx.address();

        ctx.spawn(async move {
            match chain_actor.send(ChainMessage::GetChainStatus).await {
                Ok(Ok(ChainResponse::ChainStatus(status))) => {
                    let next_height = status.height + 1;
                    let _ = addr.send(TendermintDriverMessage::NewHeight {
                        height: next_height,
                    }).await;
                }
                _ => {
                    tracing::error!("Failed to get chain status on startup");
                }
            }
        }.into_actor(self));
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // Cancel pending timeouts
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }
        tracing::info!("TendermintDriver stopping");
        Running::Stop
    }
}
```

### 4.2 Message Handler

```rust
impl Handler<TendermintDriverMessage> for TendermintDriver {
    type Result = ();

    fn handle(&mut self, msg: TendermintDriverMessage, ctx: &mut Context<Self>) {
        match msg {
            TendermintDriverMessage::NewHeight { height } => {
                self.start_height(height, ctx);
            }

            TendermintDriverMessage::Timeout { height, round, step } => {
                self.handle_timeout(height, round, step, ctx);
            }

            TendermintDriverMessage::NextRound { height, round } => {
                self.advance_round(height, round, ctx);
            }

            TendermintDriverMessage::Committed { height } => {
                self.on_commit(height, ctx);
            }

            TendermintDriverMessage::Stop => {
                ctx.stop();
            }
        }
    }
}
```

---

## 5. Migration from `aura.rs`

### 5.1 Functions to Keep

```rust
// KEEP: Used for block timestamps
pub fn duration_now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
}
```

### 5.2 Functions to Remove

```rust
// REMOVE: No longer needed with Tendermint

/// Slot calculation (replaced by height+round)
pub fn slot_from_timestamp(timestamp_ms: u64, slot_duration_ms: u64) -> u64 { ... }

/// Slot author (replaced by get_proposer)
pub fn slot_author(slot: u64, authorities: &[PublicKey]) -> Option<(u64, &PublicKey)> { ... }

/// Sleep calculation (replaced by timeout scheduling)
pub fn time_until_next_slot(slot_duration: Duration) -> Duration { ... }
```

### 5.3 Functions to Adapt

```rust
// ADAPT: Signature verification still needed

/// Keep but rename: verify_block_signature -> verify_tendermint_signature
pub fn check_signed_by_author(block: &SignedConsensusBlock, ...) -> Result<(), AuraError> {
    // Adapt for Tendermint commit verification
}

/// Keep: Threshold calculation (same 2/3+)
pub fn majority_approved(&self, block: &SignedConsensusBlock) -> Result<bool, AuraError> {
    let required_signatures = ((self.authorities.len() * 2) + 2) / 3;
    // ...
}
```

---

## 6. ChainActor Integration

### 6.1 New Messages for Tendermint Driver

```rust
// In chain/messages.rs

pub enum ChainMessage {
    // ... existing messages ...

    /// Tendermint: Request to propose a block (we are proposer)
    TendermintPropose {
        height: u64,
        round: u32,
    },

    /// Tendermint: Cast a vote (prevote or precommit)
    TendermintCastVote {
        height: u64,
        round: u32,
        vote_type: VoteType,
        block_hash: Option<BlockHash>,
    },

    /// Tendermint: Notify driver of consensus events
    TendermintEvent(TendermintEvent),
}

#[derive(Debug, Clone)]
pub enum TendermintEvent {
    /// 2/3+ prevotes received for a block
    PrevoteQuorum { height: u64, round: u32, block_hash: Option<BlockHash> },

    /// 2/3+ precommits received - block committed
    Committed { height: u64, block_hash: BlockHash },

    /// Need to advance round (2/3+ NIL or timeout)
    AdvanceRound { height: u64, next_round: u32 },
}
```

### 6.2 ChainActor Handler Additions

```rust
// In chain/handlers.rs

impl Handler<ChainMessage> for ChainActor {
    // ...

    ChainMessage::TendermintPropose { height, round } => {
        // 1. Build block using existing block building logic
        // 2. Create Proposal message
        // 3. Sign and broadcast
        // 4. Cast our own prevote for the proposal

        // See 04_CHAINACTOR_HANDLERS.md for full implementation
    }

    ChainMessage::TendermintCastVote { height, round, vote_type, block_hash } => {
        // 1. Create Vote message
        // 2. Write to WAL (safety)
        // 3. Sign and broadcast
        // 4. Add to local vote set

        // See 04_CHAINACTOR_HANDLERS.md for full implementation
    }

    ChainMessage::TendermintEvent(event) => {
        // Forward to TendermintDriver
        if let Some(driver) = &self.tendermint_driver {
            match event {
                TendermintEvent::Committed { height, .. } => {
                    let _ = driver.send(TendermintDriverMessage::Committed { height });
                }
                TendermintEvent::AdvanceRound { height, next_round } => {
                    let _ = driver.send(TendermintDriverMessage::NextRound {
                        height,
                        round: next_round,
                    });
                }
                _ => {}
            }
        }
    }
}
```

---

## 7. Startup and Initialization

### 7.1 Current Initialization (Aura)

```rust
// CURRENT: In main.rs or actor system startup

let slot_worker = AuraSlotWorkerV2::new(
    Duration::from_secs(6),  // Fixed slot duration
    authorities.clone(),
    maybe_signer,
    chain_actor.clone(),
);

// Start in background
tokio::spawn(slot_worker.start_slot_worker());
```

### 7.2 New Initialization (Tendermint)

```rust
// NEW: In main.rs or actor system startup

let timeout_config = TimeoutConfig {
    propose_timeout: Duration::from_secs(3),
    prevote_timeout: Duration::from_secs(1),
    precommit_timeout: Duration::from_secs(1),
    timeout_delta: Duration::from_millis(500),
};

let driver = TendermintDriver::new(
    validator_id,
    validator_set.clone(),
    TimeoutScheduler::new(timeout_config),
    chain_actor.clone(),
    tendermint_state.clone(),
);

// Start as Actix actor
let driver_addr = driver.start();

// Store address in ChainActor for event forwarding
chain_actor.send(ChainMessage::SetTendermintDriver { addr: driver_addr }).await?;
```

---

## 8. Configuration Changes

### 8.1 Remove Aura Config

```rust
// REMOVE from config
pub struct AuraConfig {
    pub slot_duration: Duration,  // No longer needed
}
```

### 8.2 Add Tendermint Config

```rust
// ADD: Tendermint timing configuration
pub struct TendermintTimingConfig {
    /// Base timeout for propose step
    pub propose_timeout: Duration,

    /// Base timeout for prevote step
    pub prevote_timeout: Duration,

    /// Base timeout for precommit step
    pub precommit_timeout: Duration,

    /// Timeout increase per round (exponential backoff)
    pub timeout_delta: Duration,

    /// Maximum rounds before alerting
    pub max_rounds_alert: u32,
}

impl Default for TendermintTimingConfig {
    fn default() -> Self {
        Self {
            propose_timeout: Duration::from_secs(3),
            prevote_timeout: Duration::from_secs(1),
            precommit_timeout: Duration::from_secs(1),
            timeout_delta: Duration::from_millis(500),
            max_rounds_alert: 10,
        }
    }
}
```

---

## 9. Metrics Migration

### 9.1 Remove Aura Metrics

```rust
// REMOVE:
pub static AURA_CURRENT_SLOT: Gauge = ...;
pub static AURA_PRODUCED_BLOCKS: CounterVec = ...;
pub static AURA_SLOT_CLAIM_TOTALS: CounterVec = ...;
```

### 9.2 Add Tendermint Metrics

```rust
lazy_static! {
    /// Current consensus height
    pub static ref TENDERMINT_HEIGHT: IntGauge = IntGauge::new(
        "tendermint_height",
        "Current consensus height"
    ).unwrap();

    /// Current round within height
    pub static ref TENDERMINT_ROUND: IntGauge = IntGauge::new(
        "tendermint_round",
        "Current round within height"
    ).unwrap();

    /// Current step (0=Propose, 1=Prevote, 2=Precommit, 3=Commit)
    pub static ref TENDERMINT_STEP: IntGauge = IntGauge::new(
        "tendermint_step",
        "Current consensus step"
    ).unwrap();

    /// Timeout events by step
    pub static ref TENDERMINT_TIMEOUTS: IntCounterVec = IntCounterVec::new(
        Opts::new("tendermint_timeouts_total", "Timeout events"),
        &["step"]
    ).unwrap();

    /// Rounds per height histogram
    pub static ref TENDERMINT_ROUNDS_PER_HEIGHT: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "tendermint_rounds_per_height",
            "Number of rounds needed per block"
        ).buckets(vec![1.0, 2.0, 3.0, 5.0, 10.0, 20.0])
    ).unwrap();

    /// Block time (from height start to commit)
    pub static ref TENDERMINT_BLOCK_TIME: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "tendermint_block_time_seconds",
            "Time to finalize each block"
        )
    ).unwrap();
}
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposer_selection_round_robin() {
        let validator_set = create_test_validator_set(4);
        let driver = create_test_driver(validator_set);

        // Height 0: proposer = (0 + 0) % 4 = 0
        assert_eq!(driver.get_proposer(0, 0), ValidatorId(0));

        // Height 1: proposer = (1 + 0) % 4 = 1
        assert_eq!(driver.get_proposer(1, 0), ValidatorId(1));

        // Height 0, Round 1: proposer = (0 + 1) % 4 = 1
        assert_eq!(driver.get_proposer(0, 1), ValidatorId(1));

        // Height 3, Round 2: proposer = (3 + 2) % 4 = 1
        assert_eq!(driver.get_proposer(3, 2), ValidatorId(1));
    }

    #[test]
    fn test_timeout_calculation_exponential_backoff() {
        let config = TimeoutConfig {
            propose_timeout: Duration::from_secs(3),
            timeout_delta: Duration::from_millis(500),
            ..Default::default()
        };
        let scheduler = TimeoutScheduler::new(config);

        // Round 0: 3000ms
        assert_eq!(scheduler.propose_timeout(0), Duration::from_millis(3000));

        // Round 1: 3000 + 500 = 3500ms
        assert_eq!(scheduler.propose_timeout(1), Duration::from_millis(3500));

        // Round 5: 3000 + 5*500 = 5500ms
        assert_eq!(scheduler.propose_timeout(5), Duration::from_millis(5500));
    }

    #[tokio::test]
    async fn test_timeout_cancellation_on_commit() {
        let (driver, chain_actor) = setup_test_driver().await;

        // Start height
        driver.send(TendermintDriverMessage::NewHeight { height: 1 }).await.unwrap();

        // Verify timeout is scheduled
        assert!(driver_has_pending_timeout(&driver));

        // Commit
        driver.send(TendermintDriverMessage::Committed { height: 1 }).await.unwrap();

        // Verify old timeout cancelled, new one for height 2
        let state = get_driver_state(&driver);
        assert_eq!(state.height, 2);
    }
}
```

### 10.2 Integration Tests

```rust
#[tokio::test]
async fn test_full_consensus_round() {
    // Setup 4-node test network with TendermintDriver
    let nodes = setup_tendermint_testnet(4).await;

    // Trigger height 1
    for node in &nodes {
        node.driver.send(TendermintDriverMessage::NewHeight { height: 1 }).await.unwrap();
    }

    // Wait for block finalization
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify all nodes committed height 1
    for node in &nodes {
        let status = node.chain_actor.send(ChainMessage::GetChainStatus).await.unwrap();
        assert_eq!(status.height, 1);
    }
}

#[tokio::test]
async fn test_proposer_timeout_round_advance() {
    let nodes = setup_tendermint_testnet(4).await;

    // Kill node 0 (proposer for height 1, round 0)
    nodes[0].driver.send(TendermintDriverMessage::Stop).await.unwrap();

    // Start height 1
    for node in &nodes[1..] {
        node.driver.send(TendermintDriverMessage::NewHeight { height: 1 }).await.unwrap();
    }

    // Wait for timeout and round advance
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify nodes advanced to round 1 and committed
    for node in &nodes[1..] {
        let state = node.get_tendermint_state();
        assert!(state.round >= 1 || state.height > 1);
    }
}
```

---

## 11. Migration Checklist

- [ ] Create `tendermint_driver.rs` with `TendermintDriver` struct
- [ ] Implement proposer selection: `get_proposer(height, round)`
- [ ] Implement timeout scheduling with cancellation
- [ ] Implement height advancement on commit
- [ ] Implement round advancement on timeout
- [ ] Add `TendermintDriverMessage` enum
- [ ] Implement Actor trait for `TendermintDriver`
- [ ] Add new messages to `ChainMessage` enum
- [ ] Add handler for `TendermintPropose` in ChainActor
- [ ] Add handler for `TendermintCastVote` in ChainActor
- [ ] Add handler for `TendermintEvent` in ChainActor
- [ ] Update startup code to create `TendermintDriver`
- [ ] Remove `AuraSlotWorkerV2` (or feature flag)
- [ ] Remove unused `aura.rs` functions
- [ ] Update metrics (remove Aura, add Tendermint)
- [ ] Update configuration (remove slot duration, add timeouts)
- [ ] Write unit tests for proposer selection
- [ ] Write unit tests for timeout calculation
- [ ] Write integration tests for consensus round

---

*Implementation Plan Version: 1.0*
*Last Updated: January 2026*
