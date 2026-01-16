# Analysis: Block Production with Aura PoA in V0, V1, and V2

## Overview

This document analyzes how block production is triggered and handled across three versions of the Alys blockchain implementation, focusing on the Aura (Authority Round) Proof-of-Authority consensus mechanism.

---

## V0 Architecture (Current Production System)

### Components
- **`AuraSlotWorker`** - External timing loop (`app/src/aura.rs:178-281`)
- **`Chain::produce_block()`** - Monolithic block production (`app/src/chain.rs:437-700+`)
- **Shared `Aura` instance** - Used for validation

### Flow

```
1. App.rs starts AuraSlotWorker with Arc<Chain> (line 570-577)
2. SlotWorker runs infinite loop:
   - next_slot() → waits until slot boundary using futures_timer::Delay
   - Calculates current slot from timestamp
   - claim_slot() → checks if we're the authority
   - on_slot() → calls chain.produce_block(slot, timestamp)
3. Chain::produce_block() handles EVERYTHING:
   - Sync check
   - Parent block retrieval
   - Execution payload validation/rollback
   - AuxPoW/pegout handling
   - Engine.build_block()
   - Peg-in filling
   - Pegout creation
   - Block signing with Aura keypair
   - Block storage + network broadcast
```

### Code Reference: AuraSlotWorker

```rust
// app/src/aura.rs:224-242
async fn on_slot(&self, slot: u64) -> Option<Result<(), Error>> {
    AURA_CURRENT_SLOT.set(slot as f64);

    let _ = self.claim_slot(slot, &self.authorities[..])?;
    debug!("My turn");

    let res = self.chain.produce_block(slot, duration_now()).await;
    match res {
        Ok(_) => {
            AURA_PRODUCED_BLOCKS.with_label_values(&["success"]).inc();
            Some(Ok(()))
        }
        Err(e) => {
            error!("Failed to produce block: {:?}", e);
            AURA_PRODUCED_BLOCKS.with_label_values(&["error"]).inc();
            Some(Err(e))
        }
    }
}

// app/src/aura.rs:271-280
pub async fn start_slot_worker(&mut self) {
    loop {
        let slot_info = self.next_slot().await;
        if self.maybe_signer.is_some() {
            let _ = self.on_slot(slot_info).await;
        } else {
            // nothing to do
        }
    }
}
```

### Characteristics

**Strengths:**
- ✅ **Simple, proven, working** - production system in active use
- ✅ **Clear ownership** - AuraSlotWorker owns timing, Chain owns logic
- ✅ **Deterministic scheduling** - slot calculation based on genesis timestamp
- ✅ **Precise timing** - futures_timer::Delay aligns to slot boundaries

**Weaknesses:**
- ❌ **Monolithic** - 300+ line produce_block function with 10+ concerns
- ❌ **Tight coupling** - Chain directly calls Engine, Storage, Network
- ❌ **Hard to test** - Arc<Chain> required for slot worker
- ❌ **No actor isolation** - all operations in single thread context

---

## V1 Architecture (Failed Refactor)

### Components
- **`ChainActor`** with `ctx.run_interval()` timer (`actors/chain/actor.rs:174-192`)
- **`AuraConsensusManager`** - Complex state tracking (`actors/chain/handlers/consensus_handlers.rs:77-304`)
- **`ProduceBlock` message handler** (`actors/chain/handlers/block_handlers.rs:325-420, 863-920`)

### Flow

```
1. ChainActor::started() → start_block_production_timer()
2. Actix interval timer (runs every slot_duration):
   - Calculate current slot from SystemTime
   - ctx.notify(ProduceBlock::new(slot, now))
3. Handler<ProduceBlock>:
   - Check should_produce_block() → authority check
   - Check production_state.paused
   - Get parent from chain_state.head
   - build_execution_payload() → calls EngineActor
   - Create ConsensusBlock
   - Sign and store
```

### Code Reference: Interval Timer

```rust
// actors/chain/actor.rs:174-192
fn start_block_production_timer(&self, ctx: &mut Context<Self>) {
    let slot_duration = self.config.slot_duration;

    ctx.run_interval(slot_duration, move |act, ctx| {
        if act.production_state.paused {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let slot = now.as_secs() / slot_duration.as_secs();

        // Send produce block message to ourselves
        let msg = ProduceBlock::new(slot, now);
        ctx.notify(msg);
    });
}
```

### Code Reference: ProduceBlock Handler

```rust
// actors/chain/handlers/block_handlers.rs:325-365
pub async fn handle_produce_block(&mut self, msg: ProduceBlock)
    -> Result<SignedConsensusBlock, ChainError>
{
    let start_time = Instant::now();

    info!(
        slot = msg.slot,
        timestamp = ?msg.timestamp,
        force = msg.force,
        "Producing block"
    );

    // Check if we should produce for this slot
    if !msg.force && !self.should_produce_block(msg.slot) {
        return Err(ChainError::NotOurSlot {
            slot: msg.slot,
            reason: "This slot is not assigned to us".to_string()
        });
    }

    // Check if block production is paused
    if self.production_state.paused && !msg.force {
        return Err(ChainError::ProductionPaused {
            reason: self.production_state.pause_reason.clone()
                .unwrap_or_else(|| "Unknown reason".to_string()),
        });
    }

    // Get parent block
    let parent = self.chain_state.head.as_ref()
        .ok_or(ChainError::NoParentBlock)?;

    // Build execution payload
    let execution_payload = self.build_execution_payload(
        &parent.hash,
        msg.slot,
        msg.timestamp
    ).await?;

    // Create consensus block with all required fields
    let consensus_block = ConsensusBlock {
        parent_hash: parent.hash,
        slot: msg.slot,
        auxpow_header: None, // Will be set during finalization
        // ... additional fields
    };

    // Sign and process...
}
```

### Characteristics

**Strengths:**
- ✅ **Actor-based** - proper Actix message handling
- ✅ **Separation of concerns** - consensus logic in dedicated manager
- ✅ **Pausable** - production_state allows graceful pause/resume
- ✅ **Testable** - can send ProduceBlock messages in tests

**Weaknesses:**
- ❌ **Over-engineered** - AuraConsensusManager with 300+ lines, complex slot scheduling
- ❌ **Timing issues** - run_interval() may drift, not aligned to slot boundaries
- ❌ **Never worked** - V1 was abandoned before completion
- ❌ **Tight actor coupling** - ChainActor directly calls multiple child actors

---

## V2 Current State

### What Exists ✅
- **`ChainMessage::ProduceBlock`** defined (`actors_v2/chain/messages.rs:24-28`)
- **ChainActor with actor dependencies wired** - StorageActor, NetworkActor, EngineActor, SyncActor all connected
- **Complete ProduceBlock handler** (`actors_v2/chain/handlers.rs:53-354`) with full 10-step pipeline:
  1. Validator and sync validation
  2. Parent block retrieval via StorageActor
  3. Withdrawal collection with fee calculation
  4. AddBalance conversion for pegins
  5. **Execution payload building via EngineActor** ✅ (lines 165-197)
  6. Consensus block creation
  7. AuxPoW incorporation
  8. Block storage via StorageActor
  9. Fee storage
  10. Network broadcast via NetworkActor
- **Full block production pipeline** - All actor integrations working

### What's Missing ❌
- **Slot timing mechanism for V2** - No V2-specific slot worker
  - V0's `AuraSlotWorker` exists and runs (app.rs:570-577)
  - But it only triggers V0's `Chain::produce_block()`, not V2's `ChainActor`
  - Need: V2 slot worker that sends `ChainMessage::ProduceBlock` to ChainActor
- **V2 slot worker instantiation in app.rs** - Not started alongside V2 actors

### Summary
**Implementation Status: 80% Complete**
- ✅ Block production logic fully implemented
- ✅ All actor integrations working
- ❌ Just needs slot timing trigger to be operational

---

## Proposed V2 Architecture

### Design Principles

1. **Learn from V0's simplicity** - proven timing and slot calculation
2. **Avoid V1's complexity** - no over-engineered state managers
3. **Actor-based boundaries** - proper message passing for testability
4. **Incremental migration** - can run alongside V0

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│ app.rs (V0 territory - shared startup context)              │
│                                                              │
│  AuraSlotWorkerV2::new(                                     │
│    slot_duration,                                            │
│    authorities,                                              │
│    maybe_signer,                                             │
│    chain_actor_addr: Addr<ChainActor>  ← Key difference!    │
│  ).start_slot_worker() // spawns task                       │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼ Sends ChainMessage::ProduceBlock
┌─────────────────────────────────────────────────────────────┐
│ ChainActor (actors_v2/chain/actor.rs)                       │
│                                                              │
│  Handler<ChainMessage>:                                     │
│    ProduceBlock { slot, timestamp } →                       │
│      1. Check sync status (via state.is_synced())           │
│      2. Validate slot ownership (claim_slot logic)          │
│      3. Get parent block (via storage_actor)                │
│      4. Collect peg-ins/pegouts from state                  │
│      5. Send EngineMessage::BuildPayload → EngineActor      │
│      6. Create ConsensusBlock + sign with Aura keypair      │
│      7. Send StorageMessage::StoreBlock → StorageActor      │
│      8. Send NetworkMessage::BroadcastBlock → NetworkActor  │
│      9. Update state + metrics                               │
│      10. Return ChainResponse::BlockProduced                 │
└─────────────────────────────────────────────────────────────┘
         │                    │                      │
         ▼                    ▼                      ▼
  EngineActor          StorageActor           NetworkActor
```

---

## Implementation Plan

> **Note:** As of the current V2 state, Phase 2 (ProduceBlock handler) is already complete! Only Phase 1 (slot worker) and Phase 3 (wiring) remain.

### Phase 1: Create Slot Worker (New File) ⚠️ NOT YET IMPLEMENTED

**File: `app/src/actors_v2/slot_worker.rs`**

```rust
//! Aura Slot Worker V2
//!
//! Simplified slot timing loop that sends messages to ChainActor.
//! Based on V0's proven AuraSlotWorker but adapted for actor model.

use actix::prelude::*;
use futures_timer::Delay;
use lighthouse_wrapper::bls::{Keypair, PublicKey};
use std::time::Duration;
use tracing::*;

use crate::actors_v2::chain::{ChainActor, ChainMessage, ChainResponse};
use crate::aura::{duration_now, time_until_next_slot, slot_from_timestamp, slot_author};

pub struct AuraSlotWorkerV2 {
    last_slot: u64,
    slot_duration: Duration,
    authorities: Vec<PublicKey>,
    maybe_signer: Option<Keypair>,
    chain_actor: Addr<ChainActor>,
}

impl AuraSlotWorkerV2 {
    pub fn new(
        slot_duration: Duration,
        authorities: Vec<PublicKey>,
        maybe_signer: Option<Keypair>,
        chain_actor: Addr<ChainActor>,
    ) -> Self {
        Self {
            last_slot: 0,
            slot_duration,
            authorities,
            maybe_signer,
            chain_actor,
        }
    }

    /// Check if this node is the authority for the given slot
    fn claim_slot(&self, slot: u64) -> bool {
        let expected_author = slot_author(slot, &self.authorities);
        expected_author
            .map(|(_, pk)| {
                self.maybe_signer
                    .as_ref()
                    .map(|signer| signer.pk.eq(pk))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Handle slot tick - send message to ChainActor if we're the authority
    async fn on_slot(&self, slot: u64) {
        if !self.claim_slot(slot) {
            return; // Not our slot
        }

        debug!(slot = slot, "Our slot - requesting block production");

        let msg = ChainMessage::ProduceBlock {
            slot,
            timestamp: duration_now(),
        };

        match self.chain_actor.send(msg).await {
            Ok(Ok(ChainResponse::BlockProduced { block, duration })) => {
                info!(
                    slot = slot,
                    block_hash = ?block.message.execution_payload.block_hash,
                    duration_ms = duration.as_millis(),
                    "Block produced successfully"
                );
            }
            Ok(Err(e)) => {
                error!(slot = slot, error = ?e, "Failed to produce block");
            }
            Err(e) => {
                error!(slot = slot, error = ?e, "ChainActor mailbox error");
            }
            _ => {}
        }
    }

    /// Wait for next slot boundary
    async fn next_slot(&mut self) -> u64 {
        loop {
            let wait_dur = time_until_next_slot(self.slot_duration);
            Delay::new(wait_dur).await;

            let slot = slot_from_timestamp(
                duration_now().as_millis() as u64,
                self.slot_duration.as_millis() as u64,
            );

            if slot > self.last_slot {
                self.last_slot = slot;
                break slot;
            }
        }
    }

    /// Start the slot worker loop
    pub async fn start_slot_worker(mut self) {
        info!("Starting Aura slot worker V2");

        loop {
            let slot = self.next_slot().await;

            if self.maybe_signer.is_some() {
                self.on_slot(slot).await;
            }
            // Non-validators just track slots for metrics
        }
    }
}
```

### Phase 2: Implement ProduceBlock Handler ✅ ALREADY COMPLETE

**File: `app/src/actors_v2/chain/handlers.rs:53-354`**

**Status:** Fully implemented with complete 10-step pipeline including:
- Validator and sync precondition checks
- Parent block retrieval via StorageActor
- Withdrawal collection with real fee calculation
- Execution payload building via EngineActor (lines 165-197)
- Consensus block creation with AuxPoW incorporation
- Block storage and fee tracking
- Network broadcast

**Implementation Reference:**
```rust
// See actual implementation at:
// app/src/actors_v2/chain/handlers.rs:53-354
//
// Key features:
// - Complete actor integration (Storage, Engine, Network)
// - Proper error handling and logging with correlation IDs
// - Real withdrawal/fee calculation
// - AuxPoW incorporation support
// - Comprehensive metrics tracking
```

**No action needed for this phase - already complete!**

### Phase 3: Wire Up in app.rs

```rust
// In app.rs execute() function, after V2 actor initialization:

// Start V2 Aura slot worker (if validator)
if v2_is_validator && !v2_not_validator {
    info!("Starting V2 Aura slot worker...");

    let chain_actor_addr_clone = chain_actor_addr.clone();
    tokio::spawn(async move {
        crate::actors_v2::slot_worker::AuraSlotWorkerV2::new(
            Duration::from_millis(v2_slot_duration),
            v2_authorities,
            v2_maybe_aura_signer,
            chain_actor_addr_clone,
        )
        .start_slot_worker()
        .await;
    });
}
```

---

## Key Design Decisions

### 1. Slot Worker Placement
**Decision:** Keep in `app/src` shared code, not in `actors_v2/`

**Rationale:**
- Timing loops are infrastructure, not business logic
- Precedent: V0 has `aura.rs` at app level
- Slot calculation is shared between V0 and V2

### 2. Message-Based Triggering
**Decision:** SlotWorker sends `ChainMessage::ProduceBlock`

**Rationale:**
- Testable - can inject messages in tests
- Loosely coupled - SlotWorker doesn't know ChainActor internals
- Async-friendly - proper Actix message handling

**Alternative Rejected:** Actix interval timer (V1 approach)
- Has drift issues over time
- Not aligned to slot boundaries
- Less precise than futures_timer::Delay

### 3. No AuraConsensusManager
**Decision:** Keep authority checks in SlotWorker

**Rationale:**
- Slot claiming is < 10 lines of code
- Doesn't need separate 300-line manager
- V1's manager was over-engineered
- YAGNI (You Aren't Gonna Need It)

### 4. Reuse V0 Timing Logic
**Decision:** Use `duration_now()`, `time_until_next_slot()`, `slot_from_timestamp()`

**Rationale:**
- Proven in production for months
- No need to reinvent working code
- Keep in `aura.rs`, expose as public utilities

### 5. Actor Boundaries
**Decision:** Clear separation of concerns

**Architecture:**
- **ChainActor** - Orchestrates block production
- **EngineActor** - Builds execution payloads
- **StorageActor** - Persists blocks
- **NetworkActor** - Broadcasts to peers
- **SlotWorker** - Timing and slot claiming only

**Rationale:**
- Each actor has single responsibility
- No direct cross-actor calls (all via messages)
- Testable in isolation

---

## Testing Strategy

### Unit Tests

```rust
#[actix::test]
async fn test_produce_block_message() {
    let (chain_actor, storage_actor, engine_actor) = setup_test_actors();

    let msg = ChainMessage::ProduceBlock {
        slot: 100,
        timestamp: Duration::from_secs(1000),
    };

    let response = chain_actor.send(msg).await.unwrap();

    assert!(matches!(response, Ok(ChainResponse::BlockProduced { .. })));
}

#[actix::test]
async fn test_produce_block_not_synced() {
    let chain_actor = setup_unsynced_chain_actor();

    let msg = ChainMessage::ProduceBlock {
        slot: 100,
        timestamp: Duration::from_secs(1000),
    };

    let response = chain_actor.send(msg).await.unwrap();

    assert!(matches!(response, Err(ChainError::NotSynced)));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_slot_worker_produces_at_boundaries() {
    // Start slot worker with 2-second slots
    // Verify blocks produced at t=0, t=2, t=4, etc.
    // Tolerance: ±100ms
}

#[tokio::test]
async fn test_slot_worker_claims_only_our_slots() {
    // Federation with 3 validators
    // We are validator #1
    // Verify we only produce at slots 1, 4, 7, 10, etc.
}
```

---

## Migration Path

### Phase 1: Implementation
- Implement `slot_worker.rs` (no breaking changes to V0)
- Implement `handle_produce_block()` in ChainActor
- Wire up in `app.rs` behind feature flag

### Phase 2: Testing
- Run V2 block production in dev mode alongside V0
- Monitor logs for block production events
- Verify no interference between V0/V2

### Phase 3: Validation
- Verify V2 produces blocks at correct slot boundaries
- Check block structure matches V0 format
- Validate signatures and state updates

### Phase 4: Metrics Comparison
- Compare V2/V0 block production latency
- Measure memory/CPU usage delta
- Verify no performance regression

---

## Comparison Summary

| Aspect | V0 | V1 | V2 (Current) |
|--------|----|----|---------------|
| **Timing** | futures_timer::Delay ✅ | Actix interval ❌ | ⚠️ Not yet wired (design ready) |
| **Slot Calculation** | Proven algorithm ✅ | Same as V0 ✅ | Will reuse V0 ✅ |
| **Architecture** | Monolithic ❌ | Actor-based ✅ | **Actor-based ✅ (Implemented)** |
| **Complexity** | Simple ✅ | Over-engineered ❌ | **Simple ✅ (Implemented)** |
| **Block Production Pipeline** | Monolithic 300+ lines ❌ | Actor-based ✅ | **10-step actor pipeline ✅ (Implemented)** |
| **Testability** | Hard to test ❌ | Message-based ✅ | **Message-based ✅ (Implemented)** |
| **Handler Implementation** | In Chain struct ❌ | In ChainActor ✅ | **In ChainActor ✅ (Complete)** |
| **Actor Integration** | Direct calls ❌ | Message passing ✅ | **Message passing ✅ (Complete)** |
| **Status** | Production ✅ | Abandoned ❌ | **80% Complete 🚧** |

---

## Conclusion

### Current Implementation Status

**V2 Block Production: 80% Complete**

**What's Working (Already Implemented):**
- ✅ Complete ProduceBlock message handler with 10-step pipeline
- ✅ Full actor integration (Storage, Engine, Network)
- ✅ Withdrawal collection and fee calculation
- ✅ AuxPoW incorporation support
- ✅ Proper error handling and logging
- ✅ All V2 actors instantiated and wired in app.rs

**What's Missing (Final 20%):**
- ❌ V2 slot worker to trigger block production
- ❌ Wiring slot worker to ChainActor in app.rs

**Effort to Complete:**
- ~100 lines of code for AuraSlotWorkerV2
- ~15 lines in app.rs to start the worker
- Estimated: 1-2 hours of work

---

### Architecture Achievement

**V2 successfully adopts the best of both worlds:**

From V0:
- ✅ Simple, deterministic slot calculation (to be reused)
- ✅ futures_timer for precise slot boundaries (to be reused)
- ✅ Proven timing logic (ready to adapt)

From V1:
- ✅ **Message-based architecture** (fully implemented)
- ✅ **Testability via Actix messages** (working)
- ✅ **Actor isolation** (complete)

Rejecting:
- ❌ V0's monolithic block production (✅ avoided)
- ❌ V1's complex state managers (✅ avoided)
- ❌ V1's run_interval timing (✅ avoided)

**Result:** V2 has achieved a simple, testable, actor-based block production system that maintains V0's reliability principles while enabling modularity. Only the timing trigger remains to be implemented.

---

### Next Steps to Complete V2 Block Production

**Required Work (1-2 hours):**

1. **Create `app/src/actors_v2/slot_worker.rs`** (~100 lines)
   - Copy V0's `AuraSlotWorker` structure
   - Replace `Arc<Chain>` with `Addr<ChainActor>`
   - Change `chain.produce_block()` call to `ChainMessage::ProduceBlock` send
   - Keep all timing logic identical to V0

2. **Wire up in `app/src/app.rs`** (~15 lines)
   - Add after line 536 (after "V2 Actor System fully initialized")
   - Start V2 slot worker if validator
   - Pass `chain_actor_addr` clone to worker

3. **Test End-to-End**
   - Run in dev mode
   - Verify blocks produced at slot boundaries
   - Check logs for correlation IDs
   - Validate all 10 pipeline steps execute

**Acceptance Criteria:**
- [ ] V2 produces blocks at correct slot boundaries (±100ms tolerance)
- [ ] Blocks stored via StorageActor successfully
- [ ] Blocks broadcast via NetworkActor
- [ ] No interference with V0 block production
- [ ] All correlation IDs logged for traceability
- [ ] Metrics show successful block production

**Post-Completion:**
- V2 block production will be fully operational
- Can run alongside V0 for validation
