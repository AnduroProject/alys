# Genesis Block Production Issues - V2 Analysis

**Context**: Runtime errors observed during first block production attempt in V2 actor system
**Status**: Diagnosed - Implementation pending

## Executive Summary

One critical issue identified during V2 genesis block production:

1. **CRITICAL**: `PayloadIdUnavailable` - V2 passes `Some(zero_hash)` instead of `None` to Geth, causing rejection

**Root Cause**: Semantic mismatch between V2's explicit zero hash and V0's implicit `None` for genesis blocks.

**Other Observations**:
- V0 "not synced" log is informational only - V0 and V2 operate independently (by design)

---

## Log Analysis

### Error Sequence

```
2025-10-08T13:49:08.001663Z DEBUG app::aura: app/src/aura.rs:228: My turn
2025-10-08T13:49:08.001816Z  INFO app::actors_v2::chain::handlers: app/src/actors_v2/chain/handlers.rs:79: Starting complete block production pipeline slot=439982837 timestamp_secs=1759931348 correlation_id=c4e948e0-e422-4568-aebd-fb548fbd79e6
2025-10-08T13:49:08.001969Z DEBUG app::actors_v2::storage::handlers::query_handlers: app/src/actors_v2/storage/handlers/query_handlers.rs:16: Handling GetChainHeadMessage
2025-10-08T13:49:08.002033Z  INFO produce_block{trace_id=33mnvscNJirRxDtSLb5o4U0Wf1e}: app::chain: app/src/chain.rs:452: Node is not synced, skipping block production.
2025-10-08T13:49:08.002128Z  INFO app::actors_v2::chain::handlers: app/src/actors_v2/chain/handlers.rs:107: No chain head found - producing genesis block correlation_id=c4e948e0-e422-4568-aebd-fb548fbd79e6
2025-10-08T13:49:08.002183Z DEBUG app::actors_v2::chain::withdrawals: app/src/actors_v2/chain/withdrawals.rs:37: Starting standalone withdrawal collection for block production
2025-10-08T13:49:08.002201Z DEBUG app::actors_v2::chain::withdrawals: app/src/actors_v2/chain/withdrawals.rs:102: No parent block found - returning zero fees for genesis
2025-10-08T13:49:08.002217Z  INFO app::actors_v2::chain::withdrawals: app/src/actors_v2/chain/withdrawals.rs:83: Completed standalone withdrawal collection pegin_count=0 total_pegin_amount=0 total_fee_amount=0 withdrawal_count=0
2025-10-08T13:49:08.005102Z  INFO app::actors_v2::chain::handlers: app/src/actors_v2/chain/handlers.rs:141: Successfully collected withdrawals with real fee calculation correlation_id=c4e948e0-e422-4568-aebd-fb548fbd79e6 pegin_count=0 total_pegin_amount=0 total_fee_amount=0 withdrawal_count=0
2025-10-08T13:49:08.005270Z DEBUG app::actors_v2::engine::actor: app/src/actors_v2/engine/actor.rs:309: Building execution payload correlation_id=c4e948e0-e422-4568-aebd-fb548fbd79e6 timestamp_secs=1759931348 parent_hash=Some(0x0000000000000000000000000000000000000000000000000000000000000000) balance_count=0
2025-10-08T13:49:08.007300Z ERROR app::actors_v2::engine::actor: app/src/actors_v2/engine/actor.rs:344: Failed to build execution payload correlation_id=c4e948e0-e422-4568-aebd-fb548fbd79e6 error=PayloadIdUnavailable duration_ms=2
2025-10-08T13:49:08.007402Z ERROR app::actors_v2::chain::handlers: app/src/actors_v2/chain/handlers.rs:192: Failed to build execution payload correlation_id=c4e948e0-e422-4568-aebd-fb548fbd79e6 error=BlockBuildingFailed("PayloadIdUnavailable")
2025-10-08T13:49:08.007527Z ERROR app::actors_v2::slot_worker: app/src/actors_v2/slot_worker.rs:127: Failed to produce block slot=439982837 error=Engine("Payload build failed: Block building failed: PayloadIdUnavailable")
2025-10-08T13:49:08.411907Z DEBUG sync{trace_id=33mmGtbAvEhR3It7Z1rcBAOi3xI}:wait_for_peers: app::chain: app/src/chain.rs:2212: Waiting for peers... (attempt 819)
2025-10-08T13:49:08.696178Z DEBUG app::actors_v2::network::network_actor: app/src/actors_v2/network/network_actor.rs:440: NetworkActor metrics: 0 connected peers
```

---

## Issue 1: "Node is not synced, skipping block production" (V0 Log - Not a V2 Issue)

### Root Cause

This log message is from V0, not V2. **V0 and V2 are completely separate systems with independent sync states.**

**V0 Behavior** (chain.rs:204):
```rust
sync_status: RwLock::new(SyncStatus::Synced), // assume synced, we'll find out if not
```
- Initializes with `SyncStatus::Synced` (optimistic assumption)
- Philosophy: "assume synced, we'll find out if not"
- V0 is actively syncing in the logs: `"Waiting for peers... (attempt 819)"`
- V0's sync status changed to `InProgress`, so it correctly skips block production

**V2 Behavior** (state.rs:128):
```rust
sync_status: SyncStatus::Synced,
```
- Initializes with `SyncStatus::Synced` (independent of V0)
- V2 maintains its own sync state, completely isolated from V0
- V2's NetworkActor shows: `"0 connected peers"` (expected - separate P2P network)

### Why This Is NOT a Problem

**V0 and V2 are intentionally siloed**:
- V0 runs its own P2P network, sync logic, and block production
- V2 runs its own P2P network (port offset +1000), sync logic, and block production
- No state sharing between V0 and V2 systems (by design)
- Hard cutover from V0 to V2 will happen in the future

**Current Behavior is Correct**:
- V0 is syncing (waiting for V0 peers) → skips block production ✅
- V2 is synced (no V2 peers required for genesis) → attempts block production ✅
- Both systems operate independently

### Actual Behavior Observed

Looking at the logs:
1. V2 slot worker triggers → V2 attempts block production → V2 proceeds to engine
2. V0 slot worker triggers → V0 checks sync status → V0 skips (correctly, because V0 is syncing)

**No Issue Here**: V0 and V2 are working as designed. The "not synced" message is V0's correct behavior, not a V2 problem.

---

## Issue 2: "No chain head found - producing genesis block" Flow

### Analysis

**This is CORRECT behavior and working as designed:**

1. StorageActor returns `Ok(None)` for GetChainHead (handlers.rs:106)
2. V2 detects genesis condition: `parent_hash = ExecutionBlockHash::zero()` (handlers.rs:108)
3. Withdrawal collection returns empty (no parent block for fees - correct)
4. Parent hash set to `Some(0x0000...0000)` and passed to EngineActor (handlers.rs:169)

**This flow is PERFECT** - it correctly identifies genesis and prepares for first block.

### Code Flow (handlers.rs:106-109)

```rust
Ok(None) => {
    info!(correlation_id = %correlation_id, "No chain head found - producing genesis block");
    lighthouse_wrapper::types::ExecutionBlockHash::zero()
}
```

**Status**: ✅ No fix needed - this is correct genesis detection logic

---

## Issue 3: "PayloadIdUnavailable" - The Critical Failure

### Root Cause

Geth execution engine returns `payload_id: None` from `forkchoice_updated` call.

### Technical Deep Dive

**V2 Flow** (handlers.rs:167-172):
```rust
let msg = crate::actors_v2::engine::EngineMessage::BuildPayload {
    timestamp,
    parent_hash: Some(parent_hash),  // Some(0x0000...0000) for genesis
    add_balances,
    correlation_id: Some(correlation_id),
};
```

**Engine Flow** (engine.rs:118-150):
```rust
let head = match payload_head {
    Some(head) => head,  // Uses provided hash (including zero hash)
    None => {
        let latest_block = self
            .api
            .get_block_by_number(BlockByNumberQuery::Tag(LATEST_TAG))
            .await
            .unwrap()
            .unwrap();
        latest_block.block_hash
    }
};

let forkchoice_state = ForkchoiceState {
    head_block_hash: head,  // Set to zero hash for genesis
    finalized_block_hash: finalized,
    safe_block_hash: finalized,
};

let response = self
    .api
    .forkchoice_updated(forkchoice_state, Some(payload_attributes))
    .await?;

let payload_id = response.payload_id.ok_or(Error::PayloadIdUnavailable)?;
```

**What Happens**:
1. V2 passes `Some(0x0000...0000)` as parent_hash to EngineActor
2. EngineActor calls `engine.build_block(timestamp, Some(zero_hash), add_balances)`
3. Engine sets `head_block_hash: zero_hash` in ForkchoiceState (line 133)
4. Geth's `forkchoice_updated` receives forkchoice pointing to zero hash
5. **Geth rejects this because zero hash is NOT a valid block in its database**
6. Geth returns `ForkchoiceUpdatedResponse { payload_id: None, ... }`
7. V2 fails with `PayloadIdUnavailable`

### Why V0 Doesn't Have This Problem

**V0 Genesis Flow** (chain.rs:514-519):
```rust
None => {
    debug!("No head block found, starting from genesis");
    (Hash256::zero(), None)  // Note: None for payload_head
}
```

V0 passes `payload_head: None` (not `Some(zero_hash)`) to `engine.build_block()`.

**Engine Behavior with None** (engine.rs:118-129):
```rust
let head = match payload_head {
    Some(head) => head,
    None => {
        // Fallback: Query Geth for its latest block
        let latest_block = self
            .api
            .get_block_by_number(BlockByNumberQuery::Tag(LATEST_TAG))
            .await
            .unwrap()
            .unwrap();
        latest_block.block_hash
    }
};
```

When `payload_head` is `None`:
- Engine queries Geth for its actual latest block (genesis or otherwise)
- Geth returns its real genesis block hash from database
- ForkchoiceState uses this real block hash
- Geth accepts forkchoice and returns valid payload_id

### The Semantic Difference

- **`None`**: "I don't know the parent, ask Geth what its latest block is"
- **`Some(0x00...00)`**: "Build on top of this specific block (zero hash)"

Zero hash is not in Geth's database → rejection
None triggers fallback query → success

### Why This Matters

Geth's Engine API expects `forkchoice_updated` to reference **actual blocks that exist in its database**. The zero hash is a sentinel value in our consensus layer, but it's meaningless to Geth. By passing `None`, we allow the Engine to discover Geth's actual genesis block and build on top of it.

---

## Comprehensive Resolution Plan

### Phase 1: Fix PayloadIdUnavailable (CRITICAL - Blocks All Block Production)

**Priority**: 🔴 CRITICAL
**Estimated Time**: 15 minutes
**Impact**: Unblocks all block production

#### Problem

V2 passes `Some(ExecutionBlockHash::zero())` for genesis, V0 passes `None`

#### Solution

Match V0's behavior by converting zero hash to None before calling EngineActor.

**File**: `app/src/actors_v2/chain/handlers.rs`

**Current Code** (lines 167-172):
```rust
let msg = crate::actors_v2::engine::EngineMessage::BuildPayload {
    timestamp,
    parent_hash: Some(parent_hash),
    add_balances,
    correlation_id: Some(correlation_id),
};
```

**Fixed Code**:
```rust
// Convert zero hash to None for genesis (matches V0 behavior)
let parent_hash_for_engine = if parent_hash.is_zero() {
    None
} else {
    Some(parent_hash)
};

let msg = crate::actors_v2::engine::EngineMessage::BuildPayload {
    timestamp,
    parent_hash: parent_hash_for_engine,
    add_balances,
    correlation_id: Some(correlation_id),
};
```

**Optional**: Update log message for clarity (lines 106-109):
```rust
Ok(None) => {
    info!(
        correlation_id = %correlation_id,
        "No chain head found - producing genesis block (parent_hash will be None for Engine)"
    );
    lighthouse_wrapper::types::ExecutionBlockHash::zero()
}
```

#### Testing

After fix:
```bash
# Clear databases
rm -rf ~/.alys/v2/

# Run in dev mode
cargo run -- --dev --mine

# Expected log output:
# ✅ "No chain head found - producing genesis block"
# ✅ "Building execution payload ... parent_hash=None"
# ✅ "Successfully built execution payload"
# ✅ "Block produced successfully"

# Verify no errors:
# ❌ No "PayloadIdUnavailable"
# ❌ No "Failed to build execution payload"
```

---

### Phase 2: V2 Sync Status for Genesis in Dev Mode (OPTIONAL)

**Priority**: 🟢 LOW (V2 already has correct sync status - this is optional polish)
**Estimated Time**: 15 minutes
**Impact**: Cleaner dev mode behavior (not required for functionality)

#### Analysis

V2's current sync status logic is actually **correct for genesis**:
- V2 starts with `SyncStatus::Synced` (state.rs:128)
- V2 handler checks `!self.state.is_synced()` before attempting block production (handlers.rs:60)
- Check passes → V2 proceeds with genesis block production ✅

**Why Phase 1 fix is sufficient**: Once genesis block is produced, V2 has a chain head and continues normally.

#### Optional Enhancement

For cleaner semantics in dev mode, V2 could explicitly handle "genesis with no peers" case:

**File**: `app/src/actors_v2/chain/handlers.rs`

```rust
// Around line 60
} else if !self.state.is_synced() && !self.is_genesis_mode() {
    info!("Block production requested but node is not synced");
    Box::pin(async move {
        Err(ChainError::NotSynced)
    })
}
```

Add helper method to ChainActor:
```rust
/// Check if this is genesis mode (no chain head, dev mode)
fn is_genesis_mode(&self) -> bool {
    self.state.head.is_none() && self.config.dev_mode
}
```

**Recommendation**: Skip this phase - V2's current behavior is correct. This is purely cosmetic.

---

### Phase 3: Resolve V0/V2 Slot Worker Conflict (LOW - Working but Inefficient)

**Priority**: 🟢 LOW
**Estimated Time**: 15 minutes
**Impact**: Clean up duplicate work, improve efficiency

#### Problem

Both V0 and V2 slot workers running simultaneously.

**Current Behavior** (from logs):
- V2 slot worker: Tries to produce, fails at engine
- V0 slot worker: Skips due to sync check

**Why Both Are Running**: app.rs starts both:
- V0 AuraSlotWorker started (~line 240)
- V2 AuraSlotWorkerV2 started (lines 545-562)

#### Solution

Conditional startup based on mode flag.

**File**: `app/src/app.rs`

Add configuration constant (top of file):
```rust
// V2 feature flag - set to true to use V2 actor system
const USE_V2_ACTORS: bool = true;
```

Or better yet, add CLI flag:
```rust
#[derive(Parser, Debug)]
pub struct Args {
    // ... existing fields ...

    /// Use V2 actor system instead of V0
    #[arg(long, default_value_t = false)]
    pub use_v2: bool,
}
```

Modify V0 slot worker startup (~line 240):
```rust
if !args.use_v2 && v0_is_validator && !v0_not_validator {
    info!("⏰ Starting V0 Aura slot worker...");
    tokio::spawn(async move {
        v0_aura_slot_worker.start_slot_worker().await;
    });
    info!("✓ V0 Aura slot worker started successfully");
}
```

Modify V2 slot worker startup (lines 545-562):
```rust
if args.use_v2 && v2_is_validator && !v2_not_validator {
    info!("⏰ Starting V2 Aura slot worker...");
    tokio::spawn(async move {
        crate::actors_v2::slot_worker::AuraSlotWorkerV2::new(
            Duration::from_millis(v2_slot_duration),
            v2_authorities_for_slot_worker,
            v2_maybe_aura_signer_for_slot_worker,
            chain_actor_addr_for_slot_worker,
        )
        .start_slot_worker()
        .await;
    });
    info!("✓ V2 Aura slot worker started successfully");
}
```

#### Testing

```bash
# Test V0 mode
cargo run -- --dev --mine

# Test V2 mode
cargo run -- --dev --mine --use-v2

# Verify logs show only one slot worker starting
```

---

### Phase 4: Testing Validation

#### After Phase 1 Fix (PayloadIdUnavailable)

```bash
# 1. Clear databases
rm -rf ~/.alys/v2/

# 2. Run in dev mode
cargo run -- --dev --mine

# 3. Expected logs:
✅ "No chain head found - producing genesis block"
✅ "Building execution payload ... parent_hash=None"
✅ "Successfully built execution payload"
✅ "Block produced successfully"

# 4. Verify no errors:
❌ No "PayloadIdUnavailable"
❌ No "Failed to build execution payload"
```

#### After Phase 2 Fix (Sync Status)

```bash
# Test 1: Single validator (dev mode) - should produce immediately
cargo run -- --dev --mine
# Expected: Block production starts immediately, no peer waiting

# Test 2: Multi-validator network - should wait for sync
cargo run -- --mine
# Expected: Waits for peers, syncs, then produces

# Verify correct sync semantics for both cases
```

#### After Phase 3 Fix (Slot Worker Conflict)

```bash
# Test V0 exclusive
cargo run -- --dev --mine
# Expected: Only V0 slot worker logs

# Test V2 exclusive
cargo run -- --dev --mine --use-v2
# Expected: Only V2 slot worker logs

# Verify no duplicate block production attempts
# Check metrics show single timing source
```

---

## Summary Table

| Issue | Severity | V0 Behavior | V2 Behavior | Fix Required | Files Changed |
|-------|----------|-------------|-------------|--------------|---------------|
| PayloadIdUnavailable | **🔴 CRITICAL** | Passes `None` for genesis | Passes `Some(zero_hash)` | **YES** | handlers.rs (5 lines) |
| V0 sync log message | ⚪ INFO ONLY | V0 syncing independently | V2 proceeding independently | **NO** | N/A - working as designed |
| Dual slot workers | 🟢 OPTIONAL | Both running, V0 skips | Both running, V2 proceeds | **OPTIONAL** | app.rs (conditional startup) |

**Critical Finding**: Only Issue #3 (PayloadIdUnavailable) requires a fix. Issues #1 and #2 are working as designed.

---

## Recommended Implementation Order

### Step 1: Fix PayloadIdUnavailable (15 minutes) - **REQUIRED**
- ✅ Highest impact
- ✅ Blocks all V2 block production
- ✅ Simple code change (5 lines)
- ✅ Immediately testable

### Step 2: Test Genesis Block Production (10 minutes) - **REQUIRED**
- ✅ Verify fix works
- ✅ Confirm V2 can produce genesis block
- ✅ Establishes baseline for continued development

### Step 3: Deconflict Slot Workers (15 minutes) - **OPTIONAL**
- 🔵 Nice-to-have (both work, just redundant)
- 🔵 Clean up for production
- 🔵 Adds proper mode switching with CLI flag

**Total Required Time**: ~25 minutes (Steps 1-2 only)
**Total Optional Time**: ~40 minutes (including Step 3)

---

## Key Insights

### 1. Semantic Differences Matter

The difference between `None` and `Some(zero_hash)` seems trivial but has profound implications:
- `None` = "query for latest" (discovery)
- `Some(hash)` = "use this specific block" (assertion)

For genesis, we need discovery, not assertion.

### 2. V0's Optimistic Philosophy Is Correct

V0's "assume synced, we'll find out if not" approach is exactly right for:
- Genesis block production
- Single validator networks
- Dev mode testing

V2 adopts the same philosophy - starting with `SyncStatus::Synced` is appropriate for genesis scenarios.

### 3. Actor Boundaries Expose Hidden Coupling

V0's monolithic design hid the genesis edge case inside `build_block()`. V2's actor boundaries made it explicit by forcing parent_hash to be passed as a message parameter. This is actually **good** - it exposed the implicit behavior and forced us to handle it explicitly.

### 4. V0/V2 Isolation Is By Design

V0 and V2 operate as completely separate systems:
- Separate P2P networks (V2 uses port offset +1000)
- Separate storage paths (V2 uses `/v2` subdirectory)
- Separate sync states (no coordination needed)
- Both can run simultaneously until hard cutover

This isolation is intentional and correct for the migration strategy.

---

## Next Steps

1. Implement Phase 1 fix immediately (CRITICAL - blocks all V2 block production)
2. Test genesis block production thoroughly
3. Optionally implement Phase 3 slot worker deconfliction (cosmetic improvement)

---

## References

- V0 Genesis Handling: `app/src/chain.rs:514-519`
- V0 Engine Build: `app/src/engine.rs:118-150`
- V2 Block Production: `app/src/actors_v2/chain/handlers.rs:53-354`
- V2 Engine Actor: `app/src/actors_v2/engine/actor.rs:293-357`
- V2 State Management: `app/src/actors_v2/chain/state.rs:111-144`
