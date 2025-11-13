# SyncActor Arc Refactor - Implementation Progress

**Date**: 2025-11-13
**Status**: Phase 1 Partially Complete
**Next Steps**: Complete Phase 1 field access refactoring

---

## Completed Work (Phase 1 - Partial)

### ✅ Task 1.1: SyncActorState Struct Created

**Location**: `sync_actor.rs:42-231`

Successfully created `SyncActorState` struct with all mutable state fields:
- All 12 mutable fields extracted from SyncActor
- Helper methods implemented:
  - `new()` - Constructor
  - `transition_to_state()` - State transitions with timing
  - `get_sync_status()` - Status queries
  - `determine_sync_state()` - Bootstrap detection logic
  - `select_sync_peer()` - Round-robin peer selection
  - `handle_timeouts()` - Request timeout handling

### ✅ Task 1.2: SyncActor Struct Refactored

**Location**: `sync_actor.rs:233-244`

Successfully refactored SyncActor to use Arc<RwLock<State>>:
```rust
pub struct SyncActor {
    state: std::sync::Arc<tokio::sync::RwLock<SyncActorState>>,
    config: SyncConfig,
    network_actor: Option<Addr<NetworkActor>>,
    chain_actor: Option<Addr<ChainActor>>,
}
```

### ✅ Task 1.3: Constructor Updated

**Location**: `sync_actor.rs:246-261`

Successfully updated `SyncActor::new()` to use Arc<RwLock> pattern.

---

## Remaining Work

### 🔄 Phase 1 Remaining: Update All Method Field Accesses

**Problem**: All existing async workflow methods still use `&mut self` and access fields directly (e.g., `self.sync_state`, `self.current_height`).

**Compiler Errors**: 50+ errors of type "no field `X` on type `&mut SyncActor`"

**Required Changes**: Every method that accesses mutable state must be refactored to either:

#### Option A: Update Instance Methods (Simpler, Less Ideal)
Keep methods as `&mut self` but access state through lock:

```rust
async fn start_sync(&mut self) -> Result<()> {
    // Before:
    // if self.sync_state != SyncState::Stopped {

    // After:
    {
        let state = self.state.read().await;
        if state.sync_state != SyncState::Stopped {
            return Err(anyhow!("Sync already running"));
        }
    }

    // Update state
    {
        let mut state = self.state.write().await;
        state.transition_to_state(SyncState::Starting);
        state.is_running = true;
    }

    self.initialize_height().await?;
    // ... rest of workflow
}
```

**Pros**:
- Minimal signature changes
- Can still access `self.config`, `self.network_actor`, `self.chain_actor` directly
- Less code churn

**Cons**:
- Workflows still require `&mut self`, can't be called from ctx.spawn() easily
- Doesn't fully solve the handler/workflow disconnect

#### Option B: Convert to Static Methods (Recommended from Plan)
Make workflows static and pass Arc<RwLock<State>> explicitly:

```rust
async fn start_sync_workflow(
    state: Arc<RwLock<SyncActorState>>,
    config: SyncConfig,
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
        s.is_running = true;
    }

    Self::initialize_height_workflow(state.clone(), chain_actor).await?;
    // ... rest of workflow
}
```

**Pros**:
- Fully decouples handlers from workflows
- Handlers can call workflows via ctx.spawn()
- Clean separation of concerns
- Matches the implementation plan exactly

**Cons**:
- More invasive changes
- Must pass config and actor addresses to every workflow
- Larger diff

---

## Estimated Effort for Completion

### If Using Option A (Instance Methods with Lock Access):
**Time**: 2-3 hours
**Changes Required**:
- ~30 methods need field access updates
- No signature changes
- Straightforward find-replace pattern

### If Using Option B (Static Workflow Methods):
**Time**: 4-6 hours (as per original plan)
**Changes Required**:
- ~30 methods need refactoring
- All method signatures change
- Must update all call sites
- More complex but cleaner result

---

## Recommended Next Steps

### Immediate Decision Point

**Question**: Should we proceed with:
1. **Option A** - Quick fix using instance methods with locks (2-3 hours)
2. **Option B** - Full static workflow refactor per plan (4-6 hours)

### If Option A (Quick Path):

1. **Create helper macro** to reduce boilerplate:
   ```rust
   macro_rules! with_state {
       ($self:expr, $state:ident => $body:block) => {{
           let $state = $self.state.read().await;
           $body
       }};
   }

   macro_rules! with_state_mut {
       ($self:expr, $state:ident => $body:block) => {{
           let mut $state = $self.state.write().await;
           $body
       }};
   }
   ```

2. **Update all methods systematically**:
   - Start with simple accessors (`get_sync_status`, `determine_sync_state`)
   - Move to state mutators (`transition_to_state`)
   - Update workflow methods last

3. **Fix Handler implementation**:
   - Update handlers to use state lock
   - Can still use ctx.spawn() with self reference

### If Option B (Full Refactor):

Follow the implementation plan in `SYNCACTOR_ARC_REFACTOR_PLAN.md`:
- Phase 1 complete (struct refactoring) ✅
- Phase 2: Handler refactoring (2-3 hours)
- Phase 3: Workflow refactoring (2-3 hours)
- Phase 4: Testing (1-2 hours)

---

## Current File State

**File**: `app/src/actors_v2/network/sync_actor.rs`
**Lines**: 1878
**Compile Status**: ❌ 50+ errors
**State**: Mid-refactor (struct updated, methods not updated)

### Duplicate Methods to Remove

The following methods exist in both `SyncActorState` impl (new) and `SyncActor` impl (old):
- Line 94 & 971: `transition_to_state()`
- Line 127 & 862: `get_sync_status()`
- Line 146 & 883: `determine_sync_state()`
- Line 193 & 597: `select_sync_peer()`
- Line 206 & 831: `handle_timeouts()`

**Action Needed**: Remove old implementations after updating all call sites.

---

## Testing Strategy (Post-Refactor)

Once refactoring complete, verify:

1. **Compilation**: No errors, no warnings
2. **Unit Tests**: Run existing sync_tests.rs
3. **Integration**: Test fresh node sync scenario
4. **Manual**: Follow test protocol from plan

---

## Decision Required

**User**: Which approach should we take?

### Option A: Quick Instance Method Fix (2-3 hours)
- ✅ Faster to implement
- ✅ Compiles and runs sooner
- ❌ Doesn't fully solve handler/workflow disconnect
- ❌ Will need refactoring again later for full solution

### Option B: Full Static Workflow Refactor (4-6 hours)
- ✅ Complete solution per plan
- ✅ Enables proper ctx.spawn() usage
- ✅ Clean architecture
- ❌ More time intensive
- ❌ Larger diff to review

**Recommendation**: Option B (full refactor) for production-ready solution, despite longer timeline.

---

## Rollback Information

If needed to rollback:
```bash
git checkout app/src/actors_v2/network/sync_actor.rs
```

Current changes are isolated to this single file, making rollback clean.

---

## Next Command (If Proceeding)

### For Option A:
```bash
# Create branch for quick fix
git checkout -b sync-actor-quick-fix

# Continue with macro-based field access updates
```

### For Option B:
```bash
# Continue on current branch
# Next: Refactor all async methods to static workflows
```
