# Implementation Plan: Slot Worker to Tendermint Timing Migration

## Overview

This document provides a comprehensive implementation guide for replacing the Aura-based slot worker timing mechanism with Tendermint consensus timing. The fundamental change is from fixed time-slot based block production to height+round based consensus-driven block production.

**Estimated Effort**: 3-5 days
**Dependencies**:
- `02_STATE_MACHINE.md`
- `04_CHAINACTOR_HANDLERS.md`
- `05_NETWORK_LAYER.md`
- `06_WAL.md`
- `08_TIMEOUT_MANAGEMENT.md`
- `09_SYNC_ACTOR.md`
- `17_GOVERNANCE.md`
**Files to Modify**:
- `app/src/actors_v2/slot_worker.rs` → Replace entirely
- `app/src/aura.rs` → Partially remove/adapt
- `app/src/actors_v2/chain/actor.rs`
**Files to Create**:
- `app/src/actors_v2/tendermint_driver.rs`

---

## Cross-Document Type References

| Type | Source Document | Usage in This Document |
|------|-----------------|----------------------|
| `TendermintState` | `02_STATE_MACHINE.md` | Shared consensus state with ChainActor |
| `TendermintStep` | `02_STATE_MACHINE.md` | Step enumeration for timeout scheduling |
| `Vote`, `VoteType` | `02_STATE_MACHINE.md` | Vote creation and casting |
| `Proposal` | `02_STATE_MACHINE.md` | Proposal creation when proposer |
| `LastCommit` | `02_STATE_MACHINE.md` | Embedded commit for next block proposal |
| `ValidatorSet`, `ValidatorId` | `02_STATE_MACHINE.md` | Proposer selection and voting |
| `TimeoutScheduler`, `TimeoutConfig` | `08_TIMEOUT_MANAGEMENT.md` | Adaptive timeout calculation |
| `WalWriter`, `WalEntry` | `06_WAL.md` | Crash recovery for driver state |
| `ConsensusMessage` | `05_NETWORK_LAYER.md` | Network message types |
| `SyncStatus` | `09_SYNC_ACTOR.md` | Coordination with sync actor |
| `GovernanceUpdate` | `17_GOVERNANCE.md` | Validator set changes |

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

    /// WAL writer for crash recovery
    wal_writer: Option<Arc<Mutex<WalWriter>>>,

    /// Whether consensus is paused (during sync)
    is_paused: bool,

    /// Last committed block's commit (for next proposal)
    last_commit: Option<LastCommit>,

    /// Lock state for Tendermint safety
    locked_round: Option<u32>,
    locked_value: Option<BlockHash>,

    /// Valid value from POL (Proof of Lock)
    valid_round: Option<u32>,
    valid_value: Option<BlockHash>,
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
    Committed { height: u64, last_commit: LastCommit },

    /// Pause consensus (during sync)
    Pause,

    /// Resume consensus after sync
    Resume { height: u64 },

    /// Update validator set (from governance)
    UpdateValidatorSet {
        validator_set: Arc<ValidatorSet>,
        activation_height: u64,
    },

    /// Recover state from WAL after crash
    RecoverFromWal { entries: Vec<WalEntry> },

    /// Stop the driver
    Stop,
}

/// Error types for TendermintDriver
#[derive(Debug, Clone, thiserror::Error)]
pub enum DriverError {
    #[error("ChainActor not reachable: {0}")]
    ChainActorUnreachable(String),

    #[error("Invalid driver state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Timeout scheduler error: {0}")]
    TimeoutSchedulerError(String),

    #[error("WAL write failed: {0}")]
    WalWriteFailed(String),

    #[error("Driver is paused during sync")]
    PausedDuringSync,

    #[error("Missing last commit for height {0}")]
    MissingLastCommit(u64),

    #[error("Validator set update failed: {0}")]
    ValidatorSetUpdateFailed(String),

    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
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

### 3.6 Lock and POL (Proof of Lock) Handling

```rust
impl TendermintDriver {
    /// Update lock state when we see 2/3+ prevotes for a block
    ///
    /// Tendermint safety rule: Once locked on a value, we can only
    /// unlock if we see a POL (Proof of Lock) for a different value
    /// at a higher round.
    fn update_lock(&mut self, round: u32, block_hash: BlockHash) {
        // Lock on this value
        self.locked_round = Some(round);
        self.locked_value = Some(block_hash);

        // Also update valid value
        self.valid_round = Some(round);
        self.valid_value = Some(block_hash);

        tracing::info!(
            round = round,
            block_hash = %block_hash,
            "Locked on block"
        );

        TENDERMINT_LOCKED_ROUNDS.inc();
    }

    /// Check if we can vote for a block given our lock state
    ///
    /// Returns true if:
    /// - We are not locked, OR
    /// - The block matches our locked value, OR
    /// - We have a valid POL for this block at a round >= locked_round
    fn can_vote_for(&self, block_hash: &BlockHash, pol_round: Option<u32>) -> bool {
        match (&self.locked_value, &self.locked_round) {
            (None, _) => true,  // Not locked
            (Some(locked), _) if locked == block_hash => true,  // Same value
            (_, Some(lr)) => {
                // Check if POL round >= locked round
                pol_round.map(|pr| pr >= *lr).unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Get the block to propose (considering lock state)
    ///
    /// If locked, we MUST re-propose the locked value.
    /// Otherwise, we can propose a new block.
    fn get_proposal_value(&self) -> Option<BlockHash> {
        // If we're locked, must re-propose locked value
        if let Some(locked) = &self.locked_value {
            tracing::debug!(
                locked_round = ?self.locked_round,
                "Re-proposing locked value"
            );
            return Some(locked.clone());
        }

        // If we have a valid value, prefer it
        if let Some(valid) = &self.valid_value {
            tracing::debug!(
                valid_round = ?self.valid_round,
                "Proposing valid value from POL"
            );
            return Some(valid.clone());
        }

        // Build new block
        None
    }

    /// Clear lock state on height advancement
    fn clear_lock_state(&mut self) {
        self.locked_round = None;
        self.locked_value = None;
        self.valid_round = None;
        self.valid_value = None;
    }
}
```

### 3.7 LastCommit Management

```rust
impl TendermintDriver {
    /// Store the LastCommit for the next block proposal
    ///
    /// In Tendermint's embedded design, Block N+1's last_commit field
    /// contains the commit (2/3+ precommits) that proved Block N.
    fn store_last_commit(&mut self, last_commit: LastCommit) {
        tracing::debug!(
            height = last_commit.height,
            signatures = last_commit.signatures.len(),
            "Storing LastCommit for next proposal"
        );
        self.last_commit = Some(last_commit);
    }

    /// Get LastCommit for including in the next proposal
    ///
    /// Returns None for genesis (height 1) since there's no prior block.
    fn get_last_commit_for_proposal(&self, height: u64) -> Result<Option<LastCommit>, DriverError> {
        if height == 1 {
            // Genesis block has no prior commit
            return Ok(None);
        }

        match &self.last_commit {
            Some(lc) if lc.height == height - 1 => Ok(Some(lc.clone())),
            Some(lc) => {
                tracing::warn!(
                    expected = height - 1,
                    actual = lc.height,
                    "LastCommit height mismatch"
                );
                Err(DriverError::MissingLastCommit(height - 1))
            }
            None => {
                tracing::error!(height = height - 1, "Missing LastCommit");
                Err(DriverError::MissingLastCommit(height - 1))
            }
        }
    }
}
```

### 3.8 WAL Recovery Integration

```rust
impl TendermintDriver {
    /// Recover driver state from WAL after crash
    ///
    /// Called during startup if WAL entries exist.
    /// Restores: height, round, step, lock state, last_commit.
    pub fn recover_from_wal(&mut self, entries: Vec<WalEntry>, ctx: &mut Context<Self>) {
        if entries.is_empty() {
            tracing::info!("No WAL entries to recover");
            return;
        }

        tracing::info!(
            entries = entries.len(),
            "Recovering driver state from WAL"
        );

        let mut recovered_height = 0u64;
        let mut recovered_round = 0u32;
        let mut recovered_step = TendermintStep::Propose;

        for entry in entries {
            match entry {
                WalEntry::RoundStarted { height, round } => {
                    recovered_height = height;
                    recovered_round = round;
                    recovered_step = TendermintStep::Propose;
                    // Clear locks on new height
                    if round == 0 {
                        self.clear_lock_state();
                    }
                }
                WalEntry::VoteSent { height, round, vote_type, .. } => {
                    if height == recovered_height && round == recovered_round {
                        recovered_step = match vote_type {
                            VoteType::Prevote => TendermintStep::Prevote,
                            VoteType::Precommit => TendermintStep::Precommit,
                        };
                    }
                }
                WalEntry::Locked { round, block_hash } => {
                    self.locked_round = Some(round);
                    self.locked_value = Some(block_hash);
                }
                WalEntry::Committed { height, last_commit } => {
                    // If we see a commit, advance past it
                    recovered_height = height + 1;
                    recovered_round = 0;
                    recovered_step = TendermintStep::Propose;
                    self.last_commit = Some(last_commit);
                    self.clear_lock_state();
                }
                _ => {}
            }
        }

        tracing::info!(
            height = recovered_height,
            round = recovered_round,
            step = ?recovered_step,
            locked = self.locked_value.is_some(),
            "WAL recovery complete"
        );

        // Update shared state
        {
            let mut state = self.state.blocking_write();
            state.height = recovered_height;
            state.round = recovered_round;
            state.step = recovered_step.clone();
        }

        // Resume consensus from recovered state
        if recovered_step == TendermintStep::Propose && self.is_proposer(recovered_height, recovered_round) {
            self.trigger_propose(recovered_height, recovered_round, ctx);
        }
        self.schedule_timeout(ctx);

        TENDERMINT_WAL_RECOVERIES.inc();
    }

    /// Write driver state change to WAL
    fn write_to_wal(&self, entry: WalEntry) {
        if let Some(wal) = &self.wal_writer {
            if let Ok(mut writer) = wal.lock() {
                if let Err(e) = writer.write_entry(&entry) {
                    tracing::error!(error = %e, "Failed to write WAL entry");
                }
            }
        }
    }
}
```

### 3.9 Sync Coordination

```rust
impl TendermintDriver {
    /// Pause consensus during sync
    ///
    /// Called by SyncActor when node is behind and syncing.
    /// Driver stops proposing/voting but maintains state.
    fn pause_consensus(&mut self, ctx: &mut Context<Self>) {
        if self.is_paused {
            return;
        }

        tracing::info!("Pausing consensus for sync");
        self.is_paused = true;

        // Cancel pending timeout
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        TENDERMINT_DRIVER_PAUSED.set(1);
    }

    /// Resume consensus after sync completion
    ///
    /// Called by SyncActor when node has caught up.
    /// Driver resumes from the specified height.
    fn resume_consensus(&mut self, height: u64, ctx: &mut Context<Self>) {
        if !self.is_paused {
            tracing::warn!("Resume called but not paused");
            return;
        }

        tracing::info!(height = height, "Resuming consensus after sync");
        self.is_paused = false;

        // Clear any stale lock state from before sync
        self.clear_lock_state();

        // Start fresh at the new height
        self.start_height(height, ctx);

        TENDERMINT_DRIVER_PAUSED.set(0);
    }

    /// Check if driver should process consensus messages
    fn should_process_consensus(&self) -> bool {
        !self.is_paused
    }
}
```

### 3.10 ValidatorSet Change Handling

```rust
impl TendermintDriver {
    /// Update validator set from governance
    ///
    /// Validator set changes take effect at activation_height.
    /// This is typically H+2 where H is the height where governance
    /// approved the change.
    fn update_validator_set(
        &mut self,
        new_set: Arc<ValidatorSet>,
        activation_height: u64,
    ) {
        let current_height = self.state.blocking_read().height;

        if activation_height <= current_height {
            // Immediate activation
            tracing::info!(
                activation_height = activation_height,
                new_validators = new_set.len(),
                "Activating new validator set immediately"
            );
            self.validator_set = new_set;

            // Check if we're still a validator
            self.check_validator_status();
        } else {
            // Schedule for future activation
            tracing::info!(
                activation_height = activation_height,
                current_height = current_height,
                new_validators = new_set.len(),
                "Scheduling validator set update for height {}",
                activation_height
            );

            // Store pending update (applied in start_height when we reach activation_height)
            // This would be stored in a pending_validator_updates field
        }
    }

    /// Check if we're still a validator after set change
    fn check_validator_status(&mut self) {
        if let Some(our_id) = &self.validator_id {
            if !self.validator_set.contains(our_id) {
                tracing::warn!(
                    validator_id = ?our_id,
                    "We are no longer in the validator set - switching to observer mode"
                );
                self.validator_id = None;
            }
        }
    }

    /// Get proposer accounting for pending validator set changes
    pub fn get_proposer_for_height(&self, height: u64, round: u32) -> ValidatorId {
        // Check if there's a pending validator set for this height
        // If so, use that set for proposer selection
        let validator_set = self.get_validator_set_for_height(height);
        let index = ((height + round as u64) % validator_set.len() as u64) as usize;
        validator_set.get_by_index(index)
    }

    fn get_validator_set_for_height(&self, _height: u64) -> &ValidatorSet {
        // TODO: Check pending_validator_updates for this height
        &self.validator_set
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

        // Flush WAL
        if let Some(wal) = &self.wal_writer {
            if let Ok(mut writer) = wal.lock() {
                let _ = writer.flush();
            }
        }

        tracing::info!("TendermintDriver stopping");
        Running::Stop
    }
}

impl TendermintDriver {
    /// Graceful shutdown with coordination
    ///
    /// Notifies ChainActor that driver is stopping, flushes WAL,
    /// and cancels pending operations.
    fn graceful_shutdown(&mut self, ctx: &mut Context<Self>) {
        tracing::info!("Initiating graceful shutdown");

        // Notify ChainActor
        let chain_actor = self.chain_actor.clone();
        ctx.spawn(async move {
            let _ = chain_actor.send(ChainMessage::TendermintDriverStopping).await;
        }.into_actor(self));

        // Flush WAL before stopping
        if let Some(wal) = &self.wal_writer {
            if let Ok(mut writer) = wal.lock() {
                if let Err(e) = writer.flush() {
                    tracing::error!(error = %e, "Failed to flush WAL during shutdown");
                }
            }
        }

        // Cancel pending timeout
        if let Some(handle) = self.pending_timeout.take() {
            ctx.cancel_future(handle);
        }

        ctx.stop();
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
                if self.should_process_consensus() {
                    self.start_height(height, ctx);
                }
            }

            TendermintDriverMessage::Timeout { height, round, step } => {
                if self.should_process_consensus() {
                    self.handle_timeout(height, round, step, ctx);
                }
            }

            TendermintDriverMessage::NextRound { height, round } => {
                if self.should_process_consensus() {
                    self.advance_round(height, round, ctx);
                }
            }

            TendermintDriverMessage::Committed { height, last_commit } => {
                // Store LastCommit for next proposal
                self.store_last_commit(last_commit);

                // Write commit to WAL
                self.write_to_wal(WalEntry::Committed {
                    height,
                    last_commit: self.last_commit.clone().unwrap(),
                });

                if self.should_process_consensus() {
                    self.on_commit(height, ctx);
                }
            }

            TendermintDriverMessage::Pause => {
                self.pause_consensus(ctx);
            }

            TendermintDriverMessage::Resume { height } => {
                self.resume_consensus(height, ctx);
            }

            TendermintDriverMessage::UpdateValidatorSet { validator_set, activation_height } => {
                self.update_validator_set(validator_set, activation_height);
            }

            TendermintDriverMessage::RecoverFromWal { entries } => {
                self.recover_from_wal(entries, ctx);
            }

            TendermintDriverMessage::Stop => {
                self.graceful_shutdown(ctx);
            }
        }
    }
}
```

---

## 5. Network Layer Integration

### 5.1 Receiving Consensus Messages

```rust
/// Handler for consensus messages from NetworkActor
impl Handler<NetworkConsensusMessage> for TendermintDriver {
    type Result = ();

    fn handle(&mut self, msg: NetworkConsensusMessage, ctx: &mut Context<Self>) {
        if !self.should_process_consensus() {
            return;
        }

        match msg {
            NetworkConsensusMessage::Proposal { proposal, from_peer } => {
                self.handle_proposal(proposal, from_peer, ctx);
            }
            NetworkConsensusMessage::Vote { vote, from_peer } => {
                self.handle_vote(vote, from_peer, ctx);
            }
        }
    }
}

impl TendermintDriver {
    /// Handle incoming proposal from network
    fn handle_proposal(
        &mut self,
        proposal: Proposal,
        from_peer: PeerId,
        ctx: &mut Context<Self>,
    ) {
        let state = self.state.blocking_read();

        // Validate proposal is for current height/round
        if proposal.height != state.height || proposal.round != state.round {
            tracing::trace!(
                proposal_height = proposal.height,
                proposal_round = proposal.round,
                our_height = state.height,
                our_round = state.round,
                "Ignoring proposal for different height/round"
            );
            return;
        }

        // Verify proposer
        let expected_proposer = self.get_proposer(proposal.height, proposal.round);
        if proposal.proposer_id != expected_proposer {
            tracing::warn!(
                expected = ?expected_proposer,
                actual = ?proposal.proposer_id,
                "Invalid proposer for height/round"
            );
            // Report to peer scoring
            self.report_peer_violation(from_peer, PeerViolation::InvalidProposer);
            return;
        }

        drop(state);

        // Forward to ChainActor for processing
        let chain_actor = self.chain_actor.clone();
        ctx.spawn(async move {
            let _ = chain_actor.send(ChainMessage::HandleProposal { proposal }).await;
        }.into_actor(self));
    }

    /// Handle incoming vote from network
    fn handle_vote(
        &mut self,
        vote: Vote,
        from_peer: PeerId,
        ctx: &mut Context<Self>,
    ) {
        // Forward to ChainActor for vote aggregation
        let chain_actor = self.chain_actor.clone();
        ctx.spawn(async move {
            let _ = chain_actor.send(ChainMessage::HandleVote { vote }).await;
        }.into_actor(self));
    }

    /// Report peer violation for scoring
    fn report_peer_violation(&self, peer_id: PeerId, violation: PeerViolation) {
        // Would send to NetworkActor for peer scoring
        tracing::warn!(peer = %peer_id, violation = ?violation, "Peer violation");
    }
}
```

### 5.2 Broadcasting Consensus Messages

```rust
impl TendermintDriver {
    /// Trigger proposal creation and broadcast
    fn trigger_propose(&self, height: u64, round: u32, ctx: &mut Context<Self>) {
        tracing::info!(
            height = height,
            round = round,
            "We are proposer - triggering block proposal"
        );

        // Check if we have a locked value to re-propose
        let locked_value = self.get_proposal_value();

        // Get LastCommit for this proposal
        let last_commit = match self.get_last_commit_for_proposal(height) {
            Ok(lc) => lc,
            Err(e) => {
                tracing::error!(error = %e, "Cannot propose without LastCommit");
                return;
            }
        };

        let chain_actor = self.chain_actor.clone();

        ctx.spawn(async move {
            let result = chain_actor.send(ChainMessage::TendermintPropose {
                height,
                round,
                locked_value,
                last_commit,
            }).await;

            match result {
                Ok(Ok(_)) => {
                    tracing::debug!(height = height, round = round, "Proposal created");
                    TENDERMINT_PROPOSALS_CREATED.inc();
                }
                Ok(Err(e)) => {
                    tracing::error!(error = ?e, "Failed to create proposal");
                }
                Err(e) => {
                    tracing::error!(error = %e, "ChainActor mailbox error");
                }
            }
        }.into_actor(self));
    }
}
```

---

## 6. Observer Mode

### 6.1 Observer-Specific Behavior

```rust
impl TendermintDriver {
    /// Check if running in observer mode
    pub fn is_observer(&self) -> bool {
        self.validator_id.is_none()
    }

    /// Observer startup - different from validator startup
    fn start_observer(&mut self, ctx: &mut Context<Self>) {
        tracing::info!("Starting TendermintDriver in observer mode");

        // Observers don't propose or vote, but track consensus state
        // to know when blocks are finalized

        // Query ChainActor for current height
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
                    tracing::error!("Observer: Failed to get chain status on startup");
                }
            }
        }.into_actor(self));
    }

    /// Observer handles commits but doesn't participate in voting
    fn observer_on_commit(&mut self, height: u64, last_commit: LastCommit, ctx: &mut Context<Self>) {
        tracing::debug!(height = height, "Observer: Block committed");

        // Store for tracking (even though observers don't propose)
        self.store_last_commit(last_commit);

        // Update state to track consensus progress
        {
            let mut state = self.state.blocking_write();
            state.height = height + 1;
            state.round = 0;
            state.step = TendermintStep::Propose;
        }

        // Observers still schedule timeouts to detect stuck consensus
        self.schedule_timeout(ctx);

        TENDERMINT_HEIGHT.set(height as i64 + 1);
    }

    /// Observer timeout handling - just advances tracking state
    fn observer_handle_timeout(&mut self, height: u64, round: u32, step: TendermintStep, ctx: &mut Context<Self>) {
        // Verify timeout is still relevant
        let state = self.state.blocking_read();
        if state.height != height || state.round != round || state.step != step {
            return;
        }
        drop(state);

        tracing::debug!(
            height = height,
            round = round,
            step = ?step,
            "Observer: Timeout - consensus may be stuck"
        );

        // Observers just track state, don't send NIL votes
        match step {
            TendermintStep::Precommit => {
                // Observer notes round advancement
                self.advance_round(height, round + 1, ctx);
            }
            _ => {
                // Just reschedule for next step
                let mut state = self.state.blocking_write();
                state.step = match step {
                    TendermintStep::Propose => TendermintStep::Prevote,
                    TendermintStep::Prevote => TendermintStep::Precommit,
                    _ => TendermintStep::Propose,
                };
                drop(state);
                self.schedule_timeout(ctx);
            }
        }
    }
}
```

---

## 7. Aura Migration

### 7.1 Functions to Keep

```rust
// KEEP: Used for block timestamps
pub fn duration_now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
}
```

### 7.2 Functions to Remove

```rust
// REMOVE: No longer needed with Tendermint

/// Slot calculation (replaced by height+round)
pub fn slot_from_timestamp(timestamp_ms: u64, slot_duration_ms: u64) -> u64 { ... }

/// Slot author (replaced by get_proposer)
pub fn slot_author(slot: u64, authorities: &[PublicKey]) -> Option<(u64, &PublicKey)> { ... }

/// Sleep calculation (replaced by timeout scheduling)
pub fn time_until_next_slot(slot_duration: Duration) -> Duration { ... }
```

### 7.3 Functions to Adapt

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

## 8. ChainActor Integration

### 8.1 New Messages for Tendermint Driver

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

### 8.2 ChainActor Handler Additions

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

## 9. Startup and Initialization

### 9.1 Current Initialization (Aura)

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

### 9.2 New Initialization (Tendermint)

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

## 10. Configuration Changes

### 10.1 Remove Aura Config

```rust
// REMOVE from config
pub struct AuraConfig {
    pub slot_duration: Duration,  // No longer needed
}
```

### 10.2 Add Tendermint Config

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

## 11. Metrics

### 11.1 Remove Aura Metrics

```rust
// REMOVE:
pub static AURA_CURRENT_SLOT: Gauge = ...;
pub static AURA_PRODUCED_BLOCKS: CounterVec = ...;
pub static AURA_SLOT_CLAIM_TOTALS: CounterVec = ...;
```

### 11.2 Add Tendermint Metrics

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

    /// Proposals created by this node
    pub static ref TENDERMINT_PROPOSALS_CREATED: IntCounter = IntCounter::new(
        "tendermint_proposals_created_total",
        "Total proposals created by this node"
    ).unwrap();

    /// NIL votes sent
    pub static ref TENDERMINT_NIL_VOTES: IntCounterVec = IntCounterVec::new(
        Opts::new("tendermint_nil_votes_total", "NIL votes sent"),
        &["vote_type"]  // "prevote" or "precommit"
    ).unwrap();

    /// Rounds where we were locked
    pub static ref TENDERMINT_LOCKED_ROUNDS: IntCounter = IntCounter::new(
        "tendermint_locked_rounds_total",
        "Total rounds where we locked on a value"
    ).unwrap();

    /// Driver state (0=stopped, 1=running, 2=paused)
    pub static ref TENDERMINT_DRIVER_STATE: IntGauge = IntGauge::new(
        "tendermint_driver_state",
        "Driver state: 0=stopped, 1=running, 2=paused"
    ).unwrap();

    /// Whether driver is paused for sync
    pub static ref TENDERMINT_DRIVER_PAUSED: IntGauge = IntGauge::new(
        "tendermint_driver_paused",
        "Whether driver is paused (1) or running (0)"
    ).unwrap();

    /// WAL recovery events
    pub static ref TENDERMINT_WAL_RECOVERIES: IntCounter = IntCounter::new(
        "tendermint_wal_recoveries_total",
        "Total WAL recovery events"
    ).unwrap();

    /// Validator set updates
    pub static ref TENDERMINT_VALIDATOR_SET_UPDATES: IntCounter = IntCounter::new(
        "tendermint_validator_set_updates_total",
        "Total validator set updates"
    ).unwrap();

    /// Current validator count
    pub static ref TENDERMINT_VALIDATOR_COUNT: IntGauge = IntGauge::new(
        "tendermint_validator_count",
        "Current number of validators"
    ).unwrap();
}
```

---

## 12. Testing Strategy

### 12.1 Unit Tests

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

### 12.2 Integration Tests

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

## 13. Migration Checklist

### Core Driver Implementation
- [ ] Create `tendermint_driver.rs` with `TendermintDriver` struct
- [ ] Implement `DriverError` enum with all error variants
- [ ] Implement proposer selection: `get_proposer(height, round)`
- [ ] Implement timeout scheduling with cancellation
- [ ] Implement height advancement on commit
- [ ] Implement round advancement on timeout
- [ ] Add `TendermintDriverMessage` enum with all message types

### Lock and Safety Mechanisms
- [ ] Implement `update_lock()` for POL handling
- [ ] Implement `can_vote_for()` lock checking
- [ ] Implement `get_proposal_value()` for locked value re-proposal
- [ ] Implement `clear_lock_state()` on height advancement

### LastCommit Management
- [ ] Implement `store_last_commit()` for next proposal
- [ ] Implement `get_last_commit_for_proposal()` with height validation
- [ ] Handle genesis case (height 1 has no prior commit)

### WAL Integration
- [ ] Implement `recover_from_wal()` for crash recovery
- [ ] Implement `write_to_wal()` for state changes
- [ ] Add `WalEntry` variants for driver state
- [ ] Test WAL recovery scenarios

### Sync Coordination
- [ ] Implement `pause_consensus()` for sync mode
- [ ] Implement `resume_consensus()` after sync
- [ ] Implement `should_process_consensus()` check
- [ ] Add `Pause`/`Resume` message handlers

### ValidatorSet Changes
- [ ] Implement `update_validator_set()` for governance updates
- [ ] Implement `check_validator_status()` for membership changes
- [ ] Implement `get_validator_set_for_height()` for pending updates
- [ ] Handle transition from validator to observer

### Network Layer Integration
- [ ] Implement `NetworkConsensusMessage` handler
- [ ] Implement `handle_proposal()` with validation
- [ ] Implement `handle_vote()` forwarding
- [ ] Implement `report_peer_violation()` for scoring

### Observer Mode
- [ ] Implement `is_observer()` check
- [ ] Implement `start_observer()` startup logic
- [ ] Implement `observer_on_commit()` state tracking
- [ ] Implement `observer_handle_timeout()` without voting

### Actor Implementation
- [ ] Implement `Actor` trait for `TendermintDriver`
- [ ] Implement all message handlers in `Handler<TendermintDriverMessage>`
- [ ] Implement `graceful_shutdown()` with ChainActor notification
- [ ] Implement `stopping()` with WAL flush

### ChainActor Integration
- [ ] Add new messages to `ChainMessage` enum
- [ ] Add `TendermintDriverStopping` message
- [ ] Add handler for `TendermintPropose` (with locked_value, last_commit)
- [ ] Add handler for `TendermintCastVote`
- [ ] Add handler for `TendermintEvent`
- [ ] Add handler for `HandleProposal` from network
- [ ] Add handler for `HandleVote` from network

### Startup and Configuration
- [ ] Update startup code to create `TendermintDriver`
- [ ] Add `TendermintTimingConfig` to configuration
- [ ] Wire WAL writer to driver
- [ ] Wire driver address to ChainActor

### Cleanup
- [ ] Remove `AuraSlotWorkerV2` (or feature flag)
- [ ] Remove unused `aura.rs` functions
- [ ] Remove Aura metrics
- [ ] Remove Aura configuration

### Metrics
- [ ] Add `TENDERMINT_HEIGHT` gauge
- [ ] Add `TENDERMINT_ROUND` gauge
- [ ] Add `TENDERMINT_STEP` gauge
- [ ] Add `TENDERMINT_TIMEOUTS` counter
- [ ] Add `TENDERMINT_ROUNDS_PER_HEIGHT` histogram
- [ ] Add `TENDERMINT_BLOCK_TIME` histogram
- [ ] Add `TENDERMINT_PROPOSALS_CREATED` counter
- [ ] Add `TENDERMINT_NIL_VOTES` counter
- [ ] Add `TENDERMINT_LOCKED_ROUNDS` counter
- [ ] Add `TENDERMINT_DRIVER_STATE` gauge
- [ ] Add `TENDERMINT_DRIVER_PAUSED` gauge
- [ ] Add `TENDERMINT_WAL_RECOVERIES` counter
- [ ] Add `TENDERMINT_VALIDATOR_SET_UPDATES` counter
- [ ] Add `TENDERMINT_VALIDATOR_COUNT` gauge

### Testing
- [ ] Write unit tests for proposer selection
- [ ] Write unit tests for timeout calculation
- [ ] Write unit tests for lock/POL handling
- [ ] Write unit tests for WAL recovery
- [ ] Write unit tests for sync pause/resume
- [ ] Write unit tests for validator set updates
- [ ] Write unit tests for observer mode
- [ ] Write integration tests for full consensus round
- [ ] Write integration tests for proposer timeout and round advance
- [ ] Write integration tests for network message handling

---

*Implementation Plan Version: 2.0*
*Last Updated: February 2026*
