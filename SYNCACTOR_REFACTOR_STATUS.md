# SyncActor Arc<RwLock<T>> Refactor - Current Status

**Date**: 2025-11-13
**Status**: Phase 1 Complete ✅
**Branch**: feature/v2-network

---

## ✅ Phase 1: State Refactoring (COMPLETE)

### Task 1.1: SyncActorState Struct Created ✅

**File**: `app/src/actors_v2/network/sync_actor.rs` (Lines 42-230)

Successfully created `SyncActorState` struct containing all 12 mutable fields:
- `sync_state: SyncState`
- `current_height: u64`
- `target_height: u64`
- `metrics: SyncMetrics`
- `block_queue: VecDeque<(Block, PeerId)>`
- `active_requests: HashMap<String, BlockRequestInfo>`
- `sync_peers: Vec<PeerId>`
- `peer_selection_index: usize`
- `is_running: bool`
- `shutdown_requested: bool`
- `state_entered_at: SystemTime`
- `discovery_time_accumulated: Duration`

**Helper Methods Implemented**:
- `new()` - Constructor with default values
- `get_sync_status()` - Returns SyncStatus
- `determine_sync_state()` - Bootstrap detection logic
- `select_sync_peer()` - Round-robin peer selection
- `handle_timeouts()` - Request timeout management
- `transition_to_state()` - State transitions with timing

### Task 1.2: SyncActor Struct Refactored ✅

**File**: `app/src/actors_v2/network/sync_actor.rs` (Lines 232-246)

Successfully refactored to use Arc<RwLock> pattern:

```rust
pub struct SyncActor {
    /// Shared mutable state (wrapped for async access)
    state: std::sync::Arc<tokio::sync::RwLock<SyncActorState>>,

    /// Immutable configuration (no lock needed)
    config: SyncConfig,

    /// Actor addresses for coordination (set once, never mutated directly)
    network_actor: Option<Addr<NetworkActor>>,
    chain_actor: Option<Addr<ChainActor>>,
}
```

### Task 1.3: Constructor Updated ✅

**File**: `app/src/actors_v2/network/sync_actor.rs` (Lines 248-265)

Updated `SyncActor::new()` to initialize with Arc<RwLock<SyncActorState>>:

```rust
pub fn new(config: SyncConfig) -> Result<Self> {
    tracing::info!("Creating SyncActor V2 with Arc<RwLock<State>> pattern");

    Ok(Self {
        state: std::sync::Arc::new(tokio::sync::RwLock::new(SyncActorState::new())),
        config,
        network_actor: None,
        chain_actor: None,
    })
}
```

### Task 1.4: Actor Lifecycle Updated ✅

**File**: `app/src/actors_v2/network/sync_actor.rs` (Lines 1396-1477)

Updated `Actor::started()` and `Actor::stopping()` to use Arc<RwLock> pattern:

**started() changes**:
- Periodic timeout checking now spawns async task with state clone
- Progress updates read state through RwLock
- Checkpoint saving checks state through RwLock

**stopping() changes**:
- Uses `tokio::task::block_in_place()` for synchronous state update
- Properly acquires write lock to update shutdown flags

---

## 🔄 Remaining Work

### Phase 2: Handler Refactoring (NOT STARTED)

**Estimated Time**: 2-3 hours
**Status**: File currently does NOT compile - has 50+ errors

**Required Changes**:
1. Update `Handler<SyncMessage>` implementation (~200 lines, starts at line 1479)
2. Refactor all handlers to use `ctx.spawn()` for async workflows
3. Update handlers to access state through Arc<RwLock>

**Key Handlers to Refactor**:
- `StartSync` - Must spawn async workflow instead of setting state variables
- `StopSync` - Update to use state lock
- `GetSyncStatus` - Read state through lock (can use block_in_place)
- `HandleBlockResponse` - Queue blocks and spawn processing workflow
- `HandleNewBlock` - Queue and potentially trigger processing
- `LoadCheckpoint` - Spawn async workflow
- `SaveCheckpoint` - Spawn async workflow
- `ClearCheckpoint` - Already async-safe
- `SetNetworkActor` / `SetChainActor` - Simple, no state access
- `UpdatePeers` - Update state through lock
- `GetMetrics` - Read metrics through lock
- `QueryNetworkHeight` - Read state through lock

### Phase 3: Workflow Refactoring (NOT STARTED)

**Estimated Time**: 3-4 hours

**Required Changes**:
Convert all async workflow methods from instance methods (`&mut self`) to static methods that accept `Arc<RwLock<State>>`.

**Methods to Convert** (~30 methods total):
1. `start_sync()` → `start_sync_workflow(state, network_actor, chain_actor)`
2. `initialize_height()` → `initialize_height_workflow(state, chain_actor)`
3. `discover_sync_peers()` → `discover_sync_peers_workflow(state, network_actor)`
4. `start_block_requests()` → `start_block_requests_workflow(state, network_actor, chain_actor)`
5. `discover_target_height()` → `discover_target_height_workflow(state, network_actor, chain_actor)`
6. `query_peer_heights()` → Static method
7. `create_block_requests()` → Static method
8. `process_block()` → `process_single_block(state, chain_actor, block, peer_id)`
9. `process_blocks_parallel()` → `process_blocks_parallel_workflow(state, chain_actor)`
10. `process_block_queue_optimized()` → `process_block_queue_workflow(state, chain_actor)`
11. `load_checkpoint()` → `load_checkpoint_workflow(state, data_dir)`
12. `save_checkpoint()` → `save_checkpoint_workflow(state, data_dir)`
13. `clear_checkpoint()` - Already static-safe
14. ... and ~17 more helper methods

**Pattern for Conversion**:

```rust
// BEFORE:
async fn start_sync(&mut self) -> Result<()> {
    if self.sync_state != SyncState::Stopped {
        return Err(anyhow!("Sync already running"));
    }
    self.transition_to_state(SyncState::Starting);
    self.initialize_height().await?;
    // ...
}

// AFTER:
async fn start_sync_workflow(
    state: Arc<RwLock<SyncActorState>>,
    network_actor: Option<Addr<NetworkActor>>,
    chain_actor: Option<Addr<ChainActor>>,
) -> Result<()> {
    // Check state
    {
        let s = state.read().await;
        if s.sync_state != SyncState::Stopped {
            return Err(anyhow!("Sync already running"));
        }
    }

    // Update state
    {
        let mut s = state.write().await;
        s.transition_to_state(SyncState::Starting);
    }

    Self::initialize_height_workflow(state.clone(), chain_actor).await?;
    // ...
}
```

### Phase 4: Testing & Validation (NOT STARTED)

**Estimated Time**: 1-2 hours

**Test Plan**:
1. Verify compilation (no errors, no warnings)
2. Run unit tests: `cargo test sync_tests`
3. Run integration tests: `cargo test sync_coordination`
4. Manual testing:
   - Fresh node sync
   - Block queue processing
   - Checkpoint resume

---

## Current File Status

**File**: `app/src/actors_v2/network/sync_actor.rs`
**Total Lines**: ~2068 (grown from 1878 after Phase 1)
**Compile Status**: ❌ DOES NOT COMPILE
**Estimated Errors**: 50+ field access errors

**Sample Errors**:
```
error[E0609]: no field `sync_state` on type `&mut SyncActor`
error[E0609]: no field `current_height` on type `&mut SyncActor`
error[E0609]: no field `is_running` on type `&mut SyncActor`
```

**Why it doesn't compile**:
- All workflow methods still use `self.field` instead of `state.field`
- All handlers still access fields directly instead of through lock
- ~30 methods need refactoring

---

## Critical Files Created

1. **`SYNCACTOR_ARC_REFACTOR_PLAN.md`** (1,700 lines)
   - Complete implementation plan
   - Code examples for all refactoring patterns
   - Risk analysis and mitigation
   - Testing strategy

2. **`SYNCACTOR_ARC_REFACTOR_PROGRESS.md`**
   - Initial progress tracking
   - Option A vs Option B analysis
   - Decision point documentation

3. **`SYNCACTOR_REFACTOR_STATUS.md`** (this file)
   - Current implementation status
   - Detailed phase completion tracking
   - Remaining work breakdown

---

## Next Steps

### Immediate (Recommended)

**Decision Point**: The refactoring is at a critical juncture. You have two options:

#### Option A: Continue Full Refactor (Recommended)
- **Time**: 5-7 more hours (Phase 2: 2-3hrs, Phase 3: 3-4hrs, Phase 4: 1-2hrs)
- **Result**: Complete, production-ready solution
- **Process**:
  1. Start Phase 2: Refactor handlers (tomorrow's work)
  2. Then Phase 3: Convert workflows to static methods
  3. Finally Phase 4: Test and validate

#### Option B: Rollback and Reassess
- **Time**: Immediate
- **Command**: `git checkout app/src/actors_v2/network/sync_actor.rs`
- **Result**: Back to working (but broken sync) state
- **Reason**: If timeline is too aggressive, rollback and schedule properly

### For Continuing (Option A)

**Phase 2 Starting Point**:

File: `app/src/actors_v2/network/sync_actor.rs`
Line: 1479 (`impl Handler<SyncMessage> for SyncActor`)

**First Handler to Refactor**: `StartSync` (lines 1483-1519)

**Pattern to Follow** (from plan):
```rust
SyncMessage::StartSync { start_height, target_height } => {
    tracing::info!(
        start_height = start_height,
        target_height = ?target_height,
        "Received StartSync message"
    );

    // Clone Arc for workflow execution
    let state = std::sync::Arc::clone(&self.state);
    let network_actor = self.network_actor.clone();
    let chain_actor = self.chain_actor.clone();

    // Schedule async workflow
    ctx.spawn(
        async move {
            // Validate and update state
            {
                let mut s = state.write().await;
                if s.sync_state != SyncState::Stopped && s.sync_state != SyncState::Synced {
                    tracing::warn!(state = ?s.sync_state, "Sync already running");
                    return;
                }

                s.current_height = start_height;
                s.target_height = target_height.unwrap_or(0);
                s.is_running = true;
                s.transition_to_state(SyncState::Starting);
            }

            // Execute workflow (after converting to static in Phase 3)
            if let Err(e) = Self::start_sync_workflow(
                state,
                network_actor,
                chain_actor
            ).await {
                tracing::error!("Sync workflow failed: {}", e);
            }
        }
        .into_actor(self)
    );

    // Return immediately
    Ok(SyncResponse::Started)
}
```

---

## Success Criteria

### Phase 1 ✅
- [x] SyncActorState struct created with all fields
- [x] SyncActor refactored to use Arc<RwLock<State>>
- [x] Constructor updated
- [x] Actor lifecycle updated

### Phase 2 (Pending)
- [ ] All handlers use ctx.spawn() for async work
- [ ] State access through locks only
- [ ] No direct field access in handlers
- [ ] Compilation succeeds with handler changes

### Phase 3 (Pending)
- [ ] All workflows converted to static methods
- [ ] Workflows accept Arc<RwLock<State>> parameter
- [ ] No `&mut self` in workflow signatures
- [ ] Compilation succeeds completely

### Phase 4 (Pending)
- [ ] All tests pass
- [ ] Fresh node syncs successfully
- [ ] Blocks process automatically
- [ ] Checkpoints work correctly

---

## Commit Message (When Ready)

```
feat(sync): Phase 1 - Arc<RwLock> state refactoring

Refactors SyncActor to use Arc<RwLock<SyncActorState>> pattern,
enabling async workflows to be called from sync message handlers.

Changes:
- Extract mutable state to SyncActorState struct
- Wrap state in Arc<RwLock<T>> for shared async access
- Update SyncActor struct to hold Arc<RwLock<State>>
- Refactor Actor lifecycle (started/stopping) to use locks
- Add helper methods to SyncActorState (get_status, select_peer, etc.)

Status: Phase 1 complete, does not compile yet (expected)
Next: Phase 2 - Refactor message handlers
Related: SYNCACTOR_ARC_REFACTOR_PLAN.md

Part of solving sync issues:
- Fresh nodes never synchronize
- Blocks queued but never imported
- Checkpoint loading broken
```

---

## Rollback Instructions (If Needed)

If you need to rollback Phase 1:

```bash
# Restore sync_actor.rs to original state
git checkout app/src/actors_v2/network/sync_actor.rs

# Keep documentation for future reference
git add SYNCACTOR_ARC_REFACTOR_PLAN.md
git add SYNCACTOR_ARC_REFACTOR_PROGRESS.md
git add SYNCACTOR_REFACTOR_STATUS.md
git commit -m "docs: Add Arc<RwLock> refactor plan for SyncActor"
```

---

## Time Investment Summary

- **Phase 1 (Complete)**: 2 hours ✅
- **Phase 2 (Pending)**: 2-3 hours
- **Phase 3 (Pending)**: 3-4 hours
- **Phase 4 (Pending)**: 1-2 hours
- **Total Estimated**: 8-11 hours
- **Time Invested**: 2 hours
- **Time Remaining**: 6-9 hours

---

## Recommendation

**Proceed with Phase 2 tomorrow** (fresh start, better focus for complex handler refactoring).

The groundwork is solid. Phase 1 creates the foundation. Phases 2-3 are systematic but require concentration. Breaking between phases is natural and recommended for quality.

**Why continue?**
- Phase 1 investment (2 hours) shouldn't be wasted
- The plan is solid and well-documented
- The sync issues are critical and blocking production
- This is the cleanest solution (vs workarounds)

**End Result**: Production-ready SyncActor that actually works.
