# Chain Reorganization in Alys V2 - Status & Implementation Plan

**Document Version:** 2.0
**Date:** January 2026
**Status:** Living Document
**Audience:** Engineering Team
**Target Deployment:** 3+ Node Networks

---

## Executive Summary

### Target Environment: 3+ Node Networks

This document focuses on **3+ node network deployments** as the primary concern. The current implementation was developed for 2-node regtest scenarios and has significant gaps that must be addressed before deploying to multi-validator networks.

### Current State at a Glance

| Component | Status | 3+ Node Ready? | Blocker? |
|-----------|--------|----------------|----------|
| Simple (same-height) reorgs | ✅ Implemented | ⚠️ Partial | No |
| Deep (multi-block) reorgs | ❌ Stubbed | 🔴 No | **YES** |
| Fork choice rule | ✅ Timestamp + hash | 🔴 No | **YES** |
| EngineActor sync on reorg | ✅ Implemented | ✅ Yes | No |
| AuxPoW in fork choice | ❌ Not implemented | 🔴 No | **YES** |
| Cumulative difficulty tracking | ❌ Not implemented | 🔴 No | **YES** |
| Parent hash validation | ❌ Missing | 🔴 No | **YES** |
| SyncActor coordination | ❌ Not implemented | ⚠️ Partial | No |
| Non-canonical block tracking | ❌ Not implemented | ⚠️ Partial | No |

### Deployment Blockers for 3+ Node Networks

The following **MUST** be implemented before 3+ node deployment:

1. **Deep reorg support** - Network partitions and validator downtime will cause multi-block forks
2. **AuxPoW-aware fork choice** - Security model requires "most work wins" consensus
3. **Parent hash validation** - Prevents accepting blocks with invalid chain links
4. **Cumulative difficulty tracking** - Required for proper fork choice decisions

### Key Findings

1. **Simple reorgs work** but only for same-height forks with EngineActor sync
2. **Deep reorgs will fail** - 3+ node networks will encounter multi-block forks regularly
3. **AuxPoW is completely ignored** in fork choice - violates merge-mining security model
4. **Parent hash validation is missing** - could accept invalid same-height forks
5. **Current implementation is insufficient** for production multi-validator networks

### Critical Questions Requiring Team Decision

1. Should AuxPoW blocks be treated as finalized (no reorg past them)?
2. Should fork choice use "most work wins" or "earliest timestamp wins"?
3. What's the maximum reorg depth we should allow?
4. What's the timeline for 3+ node deployment?

---

## Part 1: Chain Reorganization Fundamentals

### What is a Chain Reorganization?

A reorganization occurs when a node switches from one chain to a competing chain that is considered "better" by the consensus rules.

```
Before Reorg:
Our chain:    Block 99 → Block 100a → Block 101a
                         ↑ (our canonical tip)

After Reorg:
Our chain:    Block 99 → Block 100b → Block 101b → Block 102b
                                                    ↑ (new canonical tip)
Orphaned:              → Block 100a → Block 101a (no longer canonical)
```

### Types of Reorganizations

#### Type 1: Same-Height Fork (Simple Reorg)

Two validators produce blocks at the same height simultaneously.

```
         ┌─────────┐
    ... ─┤ Block 99 ├─┬── Block 100a (Validator A)
         └─────────┘ │
                     └── Block 100b (Validator B)
```

**Characteristics:**
- Single block replacement
- **Very common in 3+ validator networks** due to simultaneous block production
- **V2 Status:** ✅ Implemented (but missing AuxPoW consideration)

#### Type 2: Multi-Block Fork (Deep Reorg)

Chains diverge for multiple blocks before one is discovered to be "better."

```
Our chain:    99 → 100a → 101a → 102a
                 ↘
Their chain:  99 → 100b → 101b → 102b → 103b (longer/heavier)
```

**Characteristics:**
- Multiple blocks rolled back and replaced
- **Expected regularly in 3+ node networks** due to:
  - Network partitions (temporary connectivity loss)
  - Validator downtime (restarts, updates, failures)
  - Sync delays (new nodes joining network)
  - Geographic latency (distributed validators)
- **V2 Status:** 🔴 **BLOCKER** - Stubbed (returns error)

---

## Part 2: Current V2 Implementation

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Block Import Flow                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   NetworkActor ──▶ ChainActor ──▶ Fork Detection                │
│                         │              │                         │
│                         │              ▼                         │
│                         │        Same Height?                    │
│                         │         /        \                     │
│                         │       Yes         No                   │
│                         │        │           │                   │
│                         │        ▼           ▼                   │
│                         │   fork_choice   Normal Import          │
│                         │   ::compare()   (or deep reorg)        │
│                         │        │                               │
│                         │        ▼                               │
│                         │   KeepCurrent? ──▶ Reject Block        │
│                         │        │                               │
│                         │   Reorganize? ──▶ Execute Reorg        │
│                         │                       │                │
│                         │                       ▼                │
│                         │              StorageActor              │
│                         │              (update chain)            │
│                         │                       │                │
│                         │                       ▼                │
│                         │              EngineActor               │
│                         │              (sync EL fork choice)     │
│                         │                       │                │
│                         │                       ▼                │
│                         │                  Metrics               │
│                         │                                        │
└─────────────────────────────────────────────────────────────────┘
```

### Component 1: Fork Choice Rule

**Location:** `app/src/actors_v2/chain/fork_choice.rs`

**Current Implementation:**

```rust
pub enum ForkChoice {
    KeepCurrent,                                    // Reject new block
    Reorganize { new_tip: H256, rollback_to: u64 }, // Accept new block
    Tiebreak { winner: H256 },                      // Resolve tie
}
```

**Decision Rules (in order):**

1. **Timestamp Tiebreaker** - Earlier timestamp wins
2. **Hash Tiebreaker** - Lower hash wins (if timestamps equal)

**What's NOT Considered:**
- ❌ AuxPoW difficulty/work
- ❌ Cumulative chain weight
- ❌ Parent hash validation
- ❌ Block validity beyond height

**Code Reference:**
```rust
// fork_choice.rs:84-136
fn apply_tiebreaker(block_a, block_b) -> ForkChoice {
    let timestamp_a = block_a.message.execution_payload.timestamp;
    let timestamp_b = block_b.message.execution_payload.timestamp;

    // Rule 1: Earlier timestamp wins
    if timestamp_a < timestamp_b {
        return ForkChoice::KeepCurrent;
    } else if timestamp_b < timestamp_a {
        return ForkChoice::Reorganize { ... };
    }

    // Rule 2: Lower hash wins (deterministic fallback)
    if hash_a < hash_b {
        ForkChoice::KeepCurrent
    } else {
        ForkChoice::Reorganize { ... }
    }
}
```

### Component 2: Reorganization Module

**Location:** `app/src/actors_v2/chain/reorganization.rs`

**Simple Reorg (Implemented):**
```rust
pub async fn reorganize_to_new_tip(
    new_tip_block: &SignedConsensusBlock,
    current_height: u64,
    storage_actor: &Addr<StorageActor>,
    correlation_id: Uuid,
) -> Result<ReorganizationResult, ChainError>
```

**Execution Steps:**
1. Validate heights match (same-height reorg only)
2. Fetch current canonical block from StorageActor
3. Calculate block hashes for logging
4. Store new block as canonical (overwrites height mapping)
5. Update chain head
6. Return detailed result

**Deep Reorg (Stubbed):**
```rust
pub async fn reorganize_deep(...) -> Result<ReorganizationResult, ChainError> {
    tracing::error!("Deep chain reorganization not yet implemented");
    Err(ChainError::Internal("Deep chain reorganization not yet implemented".into()))
}
```

### Component 3: EngineActor Integration

**Location:** `app/src/actors_v2/chain/handlers.rs:1118-1185`

**Critical Feature:** After a reorg, the execution layer (Reth) must be notified to update its fork choice. This IS implemented:

```rust
// After successful reorganization:
let fork_choice_result = engine_actor
    .send(EngineMessage::UpdateForkChoice {
        head_hash: new_execution_hash,
        safe_hash: new_execution_hash,
        finalized_hash: finalized_hash,
        correlation_id: Some(correlation_id),
    })
    .await??;
```

**Why This Matters:**
- Without this, consensus layer (CL) and execution layer (EL) would desync
- EL would still think old block is canonical
- Transactions and state queries would return wrong data

### Component 4: Metrics & Observability

**Location:** `app/src/actors_v2/chain/metrics.rs:93-120`

**Available Metrics:**

| Metric | Type | Description |
|--------|------|-------------|
| `alys_chain_reorganizations_total` | Counter | Total reorgs performed |
| `alys_chain_reorganization_depth` | Histogram | Blocks rolled back per reorg |
| `forks_detected` | Counter | Fork detection events |
| `fork_choice_failures_after_reorg` | Counter | Engine sync failures post-reorg |

**Prometheus Queries:**
```promql
# Reorgs per minute
rate(alys_chain_reorganizations_total[5m]) * 60

# Average reorg depth
alys_chain_reorganization_depth_sum / alys_chain_reorganization_depth_count
```

### Component 5: Storage Schema

**Location:** `app/src/actors_v2/storage/database.rs`

**Current Column Families:**
```rust
BLOCKS:        hash → serialized block (all blocks including orphans)
BLOCK_HEIGHTS: height → hash (canonical chain mapping only)
STATE:         key → value (arbitrary state)
CHAIN_HEAD:    "head" → BlockRef (current tip)
```

**Reorg Behavior:**
- New block stored in BLOCKS by hash
- BLOCK_HEIGHTS mapping overwritten to point to new block
- Old block remains in BLOCKS but becomes orphaned (unreachable by height)
- No explicit canonical flag stored
- No CANONICAL_BLOCKS tracking

---

## Part 3: AuxPoW and Chain Reorganization

### Current State: AuxPoW is Ignored in Fork Choice

**Critical Gap:** The fork choice rule does NOT consider AuxPoW (Auxiliary Proof of Work) when deciding between competing blocks.

**Current Behavior:**
```
Block A: Strong AuxPoW (high difficulty), timestamp: 1001
Block B: Weak AuxPoW (low difficulty),   timestamp: 1000

V2 Decision: Block B wins (earlier timestamp)
Correct:     Block A should win (more proof-of-work)
```

**Why This Matters:**

In merge-mining systems, the fundamental security assumption is **"most work wins"**:
- Blocks with AuxPoW have been validated by Bitcoin miners
- Higher difficulty = more computational work = more security
- Ignoring this undermines the entire security model

### AuxPoW Block Structure

**Location:** `app/src/actors_v2/chain/auxpow.rs`

An AuxPoW block contains:
```rust
pub struct AuxPowHeader {
    pub coinbase_tx: Transaction,      // Bitcoin coinbase with Alys commitment
    pub block_hash: BlockHash,         // Bitcoin block hash
    pub merkle_branch: Vec<TxMerkleNode>, // Proof of inclusion
    pub chain_merkle_branch: Vec<TxMerkleNode>,
    pub parent_block: Header,          // Bitcoin block header
}
```

**Key Fields for Fork Choice:**
- `parent_block.bits` - Bitcoin difficulty target
- `parent_block.nonce` - Proof that work was done
- Can calculate: `difficulty = target_to_difficulty(bits)`

### Open Design Questions

#### Question 1: Should AuxPoW blocks be finalized?

**Option A: AuxPoW = Finality**
```
Blocks:  [99] → [100] → [101-AuxPoW] → [102] → [103]
                              ↑
                    Cannot reorg past this point
```

**Pros:**
- Strong security guarantee - Bitcoin-backed finality
- Prevents deep reorgs that undo merge-mined blocks
- Simpler mental model for users

**Cons:**
- Reduces flexibility in adversarial scenarios
- What if AuxPoW block contains invalid transactions?
- Could be exploited if AuxPoW generation is cheap

**Option B: AuxPoW = Weight, Not Finality**
```
Fork choice considers AuxPoW difficulty as weight:
- Block with AuxPoW gets difficulty bonus
- But can still be reorged if competing chain has more total work
```

**Pros:**
- More flexible consensus
- Aligns with "most work wins" principle
- Can handle edge cases (invalid blocks, attacks)

**Cons:**
- More complex implementation
- Users may see AuxPoW blocks reorged (confusing)

**Recommendation:** Option B with configurable "soft finality" depth after AuxPoW

#### Question 2: How should AuxPoW difficulty affect fork choice?

**Option A: AuxPoW as Primary Rule**
```rust
fn compare_blocks(a, b) -> ForkChoice {
    // 1. Compare cumulative difficulty (AuxPoW-aware)
    let difficulty_a = cumulative_difficulty(a);
    let difficulty_b = cumulative_difficulty(b);

    if difficulty_b > difficulty_a {
        return ForkChoice::Reorganize;
    }

    // 2. Fall back to timestamp only if difficulty equal
    apply_timestamp_tiebreaker(a, b)
}
```

**Option B: AuxPoW as Tiebreaker**
```rust
fn compare_blocks(a, b) -> ForkChoice {
    // 1. Longest chain wins
    if height_b > height_a { return ForkChoice::Reorganize; }

    // 2. Same height: Compare AuxPoW
    if has_auxpow(b) && !has_auxpow(a) {
        return ForkChoice::Reorganize;  // AuxPoW beats no-AuxPoW
    }

    // 3. Both have AuxPoW: Higher difficulty wins
    // 4. Fall back to timestamp
}
```

**Recommendation:** Option A - AuxPoW difficulty should be primary consideration

#### Question 3: What's the maximum allowed reorg depth?

**Considerations:**
- **2-node regtest:** Any depth acceptable for testing
- **Multi-validator testnet:** 10-50 blocks reasonable
- **Mainnet:** Should be limited (e.g., 100 blocks) with alerts

**Proposed Configuration:**
```rust
pub struct ReorgConfig {
    /// Maximum blocks that can be rolled back automatically
    pub max_automatic_reorg_depth: u64,  // Default: 100

    /// Blocks after AuxPoW before considered "soft final"
    pub auxpow_finality_depth: u64,      // Default: 6

    /// Alert threshold for deep reorgs
    pub alert_reorg_depth: u64,          // Default: 10
}
```

---

## Part 4: Gaps and Limitations

> **Note:** All severity ratings are assessed for **3+ node network deployments**, our target environment.

### Gap 1: Deep Reorg Not Implemented (CRITICAL BLOCKER)

**Severity:** 🔴 **CRITICAL - Deployment Blocker**

**Current State:**
- `reorganize_deep()` returns error immediately
- Only same-height reorgs work
- Any multi-block fork causes chain to halt

**Why This is Critical for 3+ Node Networks:**

In a 3+ validator network, deep reorgs are **not exceptional - they are expected**:

| Scenario | Frequency | Typical Depth |
|----------|-----------|---------------|
| Network partition (brief) | Weekly | 2-5 blocks |
| Validator restart | Per deployment | 1-10 blocks |
| New node sync race | Per new node | 5-50 blocks |
| Geographic latency | Daily | 1-3 blocks |
| Validator failure | Monthly | 10-100 blocks |

**Current Behavior:**
```
Node A: 99 → 100a → 101a → 102a (canonical)
Node B: 99 → 100b → 101b → 102b → 103b (received from network)

Result: ERROR "Deep chain reorganization not yet implemented"
Chain: HALTED - cannot process competing chain
```

**Required Changes:**
1. Implement common ancestor finding algorithm
2. Build rollback loop for old chain
3. Build apply loop for new chain
4. Coordinate with SyncActor and EngineActor

### Gap 2: No AuxPoW Consideration in Fork Choice (CRITICAL BLOCKER)

**Severity:** 🔴 **CRITICAL - Deployment Blocker**

**Current State:**
- Fork choice uses only timestamp + hash
- AuxPoW difficulty completely ignored
- No cumulative work tracking

**Why This is Critical for 3+ Node Networks:**

The entire security model of Alys relies on merge-mining with Bitcoin. Ignoring AuxPoW means:

1. **Security Violation:** Blocks without Bitcoin backing can win over secured blocks
2. **Non-Deterministic Consensus:** Nodes may choose different chains based on message arrival order
3. **Attack Vector:** Adversary can timestamp-game weak blocks to win fork choice

**Example Attack:**
```
Honest validator: Block 100 with strong AuxPoW (difficulty 1M), timestamp: 1001
Attacker:         Block 100 with NO AuxPoW,                     timestamp: 1000

Current V2: Attacker wins (earlier timestamp)
Correct:    Honest validator should win (has proof-of-work)
```

**Required Changes:**
1. Add cumulative difficulty field to ChainState
2. Store difficulty per block in database
3. Update fork_choice.rs to use "most work wins"
4. Calculate difficulty from AuxPoW headers

### Gap 3: Parent Hash Validation Missing (HIGH)

**Severity:** 🔴 **HIGH - Security Risk**

**Current State:**
- `compare_blocks()` doesn't verify parent hashes match
- Could accept blocks with different parents at same height

**Why This Matters for 3+ Node Networks:**

With more validators, the chance of receiving malformed or malicious blocks increases:

```
Valid same-height fork:
    99 → 100a (parent = hash(99))
     └→ 100b (parent = hash(99))  ✓ Same parent - valid competition

Invalid (current code accepts this!):
    99a → 100a (parent = hash(99a))
    99b → 100b (parent = hash(99b))  ✗ Different parents - broken chain!
```

**Impact:**
- Broken chain links possible
- Invalid blocks could become canonical
- Chain state corruption

**Required Changes:**
```rust
// In fork_choice.rs:compare_blocks()
if current_block.parent_hash != new_block.parent_hash {
    // Not a same-height fork - different chains!
    return ForkChoice::RequiresDeepAnalysis;
}
```

### Gap 4: SyncActor Coordination (MEDIUM)

**Severity:** 🟡 **MEDIUM - Operational Risk**

**Current State:**
- No notification to SyncActor during reorg
- Sync might download blocks from wrong fork
- Wasted bandwidth and validation effort

**Impact in 3+ Node Networks:**
- Inefficient sync during frequent reorgs
- Potential state inconsistency during partition recovery
- Sync could stall or loop

**Required Changes:**
1. Add `ReorgStarting` / `ReorgCompleted` messages
2. SyncActor pauses during reorg
3. SyncActor resumes from new tip

### Gap 5: Non-Canonical Block Tracking (LOW)

**Severity:** 🟢 **LOW - Operational Improvement**

**Current State:**
- Old blocks become orphaned, not tracked
- No way to query "all blocks at height X"
- Can retrieve by hash if known, but impractical

**Impact:**
- Limited forensics capability for debugging reorg issues
- Can't audit reorg history
- Debugging multi-validator issues more difficult

**Required Changes:**
1. Add CANONICAL_BLOCKS column family
2. Implement `MarkNonCanonical` storage message
3. Add `GetAllBlocksAtHeight` query

### Gap Summary Table (3+ Node Focus)

| Gap | Severity | Blocker? | Must Fix Before 3+ Deploy |
|-----|----------|----------|---------------------------|
| Deep reorg | 🔴 Critical | **YES** | ✅ Required |
| AuxPoW fork choice | 🔴 Critical | **YES** | ✅ Required |
| Parent hash validation | 🔴 High | **YES** | ✅ Required |
| SyncActor coordination | 🟡 Medium | No | Recommended |
| Non-canonical tracking | 🟢 Low | No | Nice to have |

### Risk Matrix: Deploying Without Fixes

| Scenario | Probability | Impact | Risk |
|----------|-------------|--------|------|
| Multi-block fork occurs | **Certain** | Chain halts | 🔴 **Unacceptable** |
| Weak block wins over AuxPoW block | High | Security breach | 🔴 **Unacceptable** |
| Invalid parent hash accepted | Medium | Chain corruption | 🔴 **Unacceptable** |
| Sync inefficiency during reorg | High | Degraded performance | 🟡 Manageable |
| Can't debug reorg issues | Certain | Slower incident response | 🟢 Acceptable |

---

## Part 5: Implementation Plan

> **Priority:** This implementation is a **blocker for 3+ node deployment**. All Phase 1-3 items must be completed before deploying to multi-validator networks.

### Implementation Priority Order

The implementation is ordered by **deployment criticality**, not complexity:

| Priority | Component | Rationale |
|----------|-----------|-----------|
| P0 | Deep reorg | Chain halts without this |
| P0 | AuxPoW fork choice | Security model broken without this |
| P0 | Parent hash validation | Chain corruption possible without this |
| P1 | SyncActor coordination | Efficiency and stability |
| P2 | Non-canonical tracking | Operational visibility |

### Phase 1: Critical Blockers - Part A (Week 1-2)

**Epic: AuxPoW Fork Choice Foundation**

#### Story 1.1: Cumulative Difficulty Storage (8 hours)

**Tasks:**
- [ ] Add `CUMULATIVE_DIFFICULTY` column family to database.rs
- [ ] Create `StoreDifficultyMessage` in storage/messages.rs
- [ ] Implement `put_cumulative_difficulty()` and `get_cumulative_difficulty()`
- [ ] Add migration script for existing chains
- [ ] Unit tests for storage operations

**Acceptance Criteria:**
- Can store and retrieve difficulty per block height
- Existing chains can be migrated (calculate from genesis)

#### Story 1.2: Difficulty Calculation from AuxPoW (6 hours)

**Tasks:**
- [ ] Add `calculate_block_difficulty()` function in auxpow.rs
- [ ] Extract difficulty from AuxPoW header's `bits` field
- [ ] Handle blocks without AuxPoW (use base difficulty)
- [ ] Add `cumulative_difficulty` field to ChainState
- [ ] Unit tests for difficulty calculation

**Acceptance Criteria:**
- Can calculate difficulty for any block (with or without AuxPoW)
- ChainState tracks cumulative difficulty

#### Story 1.3: Parent Hash Validation (3 hours)

**Tasks:**
- [ ] Add parent hash check in `fork_choice::compare_blocks()`
- [ ] Return new `ForkChoice::RequiresDeepAnalysis` variant if parents differ
- [ ] Update handlers.rs to handle new variant
- [ ] Unit tests for parent hash validation

**Acceptance Criteria:**
- Same-height blocks with different parents are detected
- Proper error/handling path exists

### Phase 2: Critical Blockers - Part B (Week 2-3)

**Epic: AuxPoW-Aware Fork Choice**

#### Story 2.1: Update Fork Choice Rule (8 hours)

**Tasks:**
- [ ] Modify `compare_blocks()` to accept cumulative difficulties
- [ ] Implement "most work wins" as primary rule
- [ ] Keep timestamp as secondary tiebreaker
- [ ] Keep hash as tertiary tiebreaker
- [ ] Add comprehensive logging
- [ ] Unit tests for all scenarios

**New Fork Choice Logic:**
```rust
pub fn compare_blocks_with_difficulty(
    current_block: &SignedConsensusBlock,
    current_cumulative_difficulty: u64,
    new_block: &SignedConsensusBlock,
    new_cumulative_difficulty: u64,
) -> ForkChoice {
    // 1. Validate same parent (true same-height fork)
    if current_block.parent_hash != new_block.parent_hash {
        return ForkChoice::RequiresDeepAnalysis;
    }

    // 2. Most work wins (primary rule)
    if new_cumulative_difficulty > current_cumulative_difficulty {
        return ForkChoice::Reorganize { ... };
    } else if current_cumulative_difficulty > new_cumulative_difficulty {
        return ForkChoice::KeepCurrent;
    }

    // 3. Same difficulty: Timestamp tiebreaker
    // 4. Same timestamp: Hash tiebreaker
    apply_tiebreaker(current_block, new_block)
}
```

#### Story 2.2: ChainActor Integration (6 hours)

**Tasks:**
- [ ] Update handlers.rs to pass difficulty to fork choice
- [ ] Fetch cumulative difficulty from storage during comparison
- [ ] Update difficulty after successful reorg
- [ ] Add difficulty to reorg metrics
- [ ] Integration tests

**Acceptance Criteria:**
- Fork choice considers AuxPoW difficulty
- Metrics include difficulty information

### Phase 3: Critical Blockers - Part C (Week 3-4)

**Epic: Deep Chain Reorganization** (🔴 **HIGHEST PRIORITY - Chain halts without this**)

#### Story 3.1: Common Ancestor Algorithm (6 hours)

**Tasks:**
- [ ] Implement `find_common_ancestor()` with full chain traversal
- [ ] Follow parent_hash links backwards on both chains
- [ ] Handle missing blocks (request from network)
- [ ] Optimize with caching for large chains
- [ ] Unit tests with various fork scenarios

**Algorithm:**
```rust
async fn find_common_ancestor(
    our_tip: &Block,
    their_tip: &Block,
    storage: &StorageActor,
) -> Result<u64, ChainError> {
    let mut our_chain = trace_back(our_tip, storage).await?;
    let mut their_chain = trace_back(their_tip, storage).await?;

    // Find first matching block
    for height in (0..=our_tip.height).rev() {
        if our_chain[height].hash == their_chain[height].hash {
            return Ok(height);
        }
    }

    Err(ChainError::NoCommonAncestor)
}
```

#### Story 3.2: Deep Reorg Execution (10 hours)

**Tasks:**
- [ ] Implement rollback loop (mark blocks non-canonical)
- [ ] Implement apply loop (store new chain blocks)
- [ ] Ensure atomicity (all-or-nothing)
- [ ] Handle partial failures with recovery
- [ ] Update EngineActor at each step (or batch)
- [ ] Comprehensive logging and metrics
- [ ] Integration tests with 5, 10, 50 block reorgs

**Execution Flow:**
```rust
pub async fn reorganize_deep(
    their_tip: &Block,
    common_ancestor: u64,
    storage: &StorageActor,
    engine: &EngineActor,
) -> Result<ReorganizationResult, ChainError> {
    // Phase 1: Rollback our chain
    for height in (common_ancestor + 1..=our_height).rev() {
        storage.send(MarkNonCanonical { height }).await??;
    }

    // Phase 2: Apply their chain
    let their_chain = build_chain_from_tip(their_tip, common_ancestor).await?;
    for block in their_chain {
        storage.send(StoreBlock { block, canonical: true }).await??;
        engine.send(CommitBlock { block }).await??;
    }

    // Phase 3: Update head
    storage.send(UpdateChainHead { their_tip }).await??;
    engine.send(UpdateForkChoice { their_tip }).await??;

    Ok(ReorganizationResult { ... })
}
```

#### Story 3.3: AuxPoW Finality Check (4 hours)

**Tasks:**
- [ ] Add configurable `auxpow_finality_depth` parameter
- [ ] Check if reorg would cross finalized AuxPoW block
- [ ] Implement `is_finalized()` check in reorg path
- [ ] Add override for emergency scenarios
- [ ] Tests for finality protection

**Finality Check:**
```rust
fn validate_reorg_safety(
    common_ancestor: u64,
    last_auxpow_height: Option<u64>,
    config: &ReorgConfig,
) -> Result<(), ChainError> {
    if let Some(auxpow_height) = last_auxpow_height {
        let blocks_since_auxpow = current_height - auxpow_height;

        if common_ancestor < auxpow_height
           && blocks_since_auxpow >= config.auxpow_finality_depth {
            return Err(ChainError::FinalityViolation {
                auxpow_height,
                attempted_rollback_to: common_ancestor,
            });
        }
    }
    Ok(())
}
```

### Phase 4: Stability Improvements (Week 4-5)

**Epic: System Coordination** (🟡 Recommended for production stability)

#### Story 4.1: SyncActor Coordination (4 hours)

**Tasks:**
- [ ] Add `ReorgStarting` message to ChainActor
- [ ] Add `PauseSync` / `ResumeSync` to SyncActor
- [ ] ChainActor notifies SyncActor before reorg
- [ ] SyncActor cancels in-flight requests
- [ ] SyncActor resumes from new tip after reorg
- [ ] Integration test: reorg during active sync

#### Story 4.2: Non-Canonical Block Tracking (6 hours)

**Tasks:**
- [ ] Add `CANONICAL_BLOCKS` column family
- [ ] Store `Vec<(hash, is_canonical)>` per height
- [ ] Implement `MarkNonCanonical` message
- [ ] Implement `GetAllBlocksAtHeight` query
- [ ] Migration for existing data
- [ ] Unit tests

#### Story 4.3: Enhanced Metrics & Alerting (4 hours)

**Tasks:**
- [ ] Add `deep_reorganizations_total` counter
- [ ] Add `max_reorg_depth_gauge` gauge
- [ ] Add `reorg_duration_seconds` histogram
- [ ] Add `auxpow_finality_violations_total` counter
- [ ] Configure alert thresholds
- [ ] Update Grafana dashboards

### Phase 5: Testing & Validation (Week 5-6)

**Epic: Quality Assurance** (🔴 Required before 3+ node deployment)

#### Story 5.1: Comprehensive Testing (12 hours)

**Test Scenarios:**
- [ ] Simple reorg: same height, timestamp wins
- [ ] Simple reorg: same height, hash wins
- [ ] Simple reorg: AuxPoW block wins over non-AuxPoW
- [ ] Deep reorg: 5 blocks
- [ ] Deep reorg: 50 blocks
- [ ] Deep reorg: with AuxPoW finality check
- [ ] Reorg during active sync
- [ ] Reorg with missing blocks (network fetch)
- [ ] Partial failure recovery
- [ ] Concurrent reorg attempts

#### Story 5.2: Documentation Update (4 hours)

**Tasks:**
- [ ] Update this document with implementation details
- [ ] Add runbook for reorg incidents
- [ ] Document configuration options
- [ ] Add troubleshooting guide
- [ ] Update architecture diagrams

### Timeline Summary

| Phase | Duration | Stories | Hours | 3+ Node Blocker? |
|-------|----------|---------|-------|------------------|
| Phase 1: Foundation | Week 1-2 | 3 | 17 | 🔴 YES |
| Phase 2: Fork Choice | Week 2-3 | 2 | 14 | 🔴 YES |
| Phase 3: Deep Reorg | Week 3-4 | 3 | 20 | 🔴 YES |
| Phase 4: Coordination | Week 4-5 | 3 | 14 | 🟡 Recommended |
| Phase 5: Testing | Week 5-6 | 2 | 16 | 🔴 YES |
| **Total** | **6 weeks** | **13** | **81 hours** | |

### Milestones

| Milestone | Target | Deliverable | 3+ Node Deploy? |
|-----------|--------|-------------|-----------------|
| M1: AuxPoW Fork Choice | End of Week 3 | Fork choice considers difficulty | 🔴 Not yet |
| M2: Deep Reorg MVP | End of Week 4 | Deep reorgs work for <50 blocks | 🔴 Not yet |
| M3: **3+ Node Ready** | End of Week 5 | All blockers resolved + basic testing | ✅ **CAN DEPLOY** |
| M4: Production Hardened | End of Week 6 | Full testing + operational tools | ✅ Recommended |

### Minimum Viable 3+ Node Deployment

To deploy to 3+ nodes with minimal implementation, the following are **non-negotiable**:

| Component | Hours | Why Non-Negotiable |
|-----------|-------|-------------------|
| Cumulative difficulty storage | 8 | Fork choice requires this |
| Difficulty calculation | 6 | Fork choice requires this |
| Parent hash validation | 3 | Prevents chain corruption |
| AuxPoW fork choice rule | 8 | Security model requires this |
| Deep reorg execution | 16 | Chain halts without this |
| Basic integration tests | 6 | Verify it works |
| **Minimum Total** | **47 hours** | **~4 weeks** |

The remaining 34 hours (SyncActor coordination, non-canonical tracking, comprehensive testing, documentation) can be deferred to post-deployment if needed.

---

## Part 6: Decision Log

### Decisions Made

| Decision | Choice | Rationale | Date |
|----------|--------|-----------|------|
| Simple reorg: timestamp tiebreaker | Implemented | Fast, deterministic for initial development | Pre-2026 |
| Deep reorg: intentionally stubbed | Implemented | Deferred for 2-node regtest phase | Pre-2026 |
| EngineActor sync after reorg | Implemented | Critical for CL/EL consistency | Pre-2026 |
| **Target: 3+ node networks** | Adopted | Primary deployment target | 2026-01 |

### Decisions Pending (Required Before 3+ Node Deployment)

| Decision | Options | Recommendation | Owner | Urgency |
|----------|---------|----------------|-------|---------|
| AuxPoW as finality? | A: Yes (hard), B: Weight only | B with soft finality | Team | 🔴 High |
| Max automatic reorg depth | 10, 50, 100, unlimited | 100 with alerts at 10 | Team | 🟡 Medium |
| Fork choice primary rule | Timestamp, Difficulty, Height | Difficulty (most work wins) | Team | 🔴 High |
| AuxPoW finality depth | 3, 6, 12 blocks | 6 blocks (like Bitcoin) | Team | 🟡 Medium |
| Deep reorg max depth | 50, 100, 500, unlimited | 100 with operator override | Team | 🟡 Medium |

### Decision: AuxPoW Finality Model (Proposed)

**Recommendation:** Soft finality with configurable depth

```rust
pub struct ReorgConfig {
    /// Blocks after AuxPoW before considered "soft final"
    /// Reorgs past this point require operator override
    pub auxpow_finality_depth: u64,  // Recommended: 6

    /// Maximum automatic reorg depth
    /// Deeper reorgs require operator approval
    pub max_automatic_reorg_depth: u64,  // Recommended: 100

    /// Alert threshold for operator notification
    pub alert_reorg_depth: u64,  // Recommended: 10
}
```

**Rationale:**
- AuxPoW blocks gain finality over time (like Bitcoin confirmations)
- 6 blocks mirrors Bitcoin's "6 confirmation" standard
- Allows flexibility for edge cases while providing security guarantees
- Operator can override in emergency (with audit trail)

**Team Input Required:** Confirm or modify these defaults before implementation.

---

## Part 7: Appendix

### A. Code References

| Component | File | Lines | Description |
|-----------|------|-------|-------------|
| Fork choice enum | fork_choice.rs | 13-23 | ForkChoice type definition |
| Tiebreaker logic | fork_choice.rs | 84-136 | Timestamp/hash comparison |
| Simple reorg | reorganization.rs | 55-214 | Same-height reorg execution |
| Deep reorg stub | reorganization.rs | 230-240 | Stubbed implementation |
| Fork detection | handlers.rs | 1004-1050 | Detect same-height conflict |
| Reorg execution | handlers.rs | 1050-1195 | Execute reorg + engine sync |
| Reorg metrics | metrics.rs | 93-120 | Prometheus metrics |
| Storage schema | database.rs | 43-51 | Column family definitions |
| AuxPoW validation | auxpow.rs | 22-119 | AuxPoW header validation |

### B. Useful Commands

```bash
# View reorg logs
docker logs alys-node-1 2>&1 | grep -i "reorg\|fork" | tail -50

# Check reorg metrics
curl -s http://localhost:9615/metrics | grep -E "reorganization|fork"

# Query block by hash (can find orphaned blocks)
alys-cli chain get-block --hash 0xabc...

# Query canonical block by height
alys-cli chain get-block --height 100

# Trigger test reorg (dev mode only)
alys-cli dev trigger-reorg --height 100 --depth 5
```

### C. Glossary

| Term | Definition |
|------|------------|
| **Canonical chain** | The chain of blocks considered "correct" by consensus |
| **Orphan block** | A valid block that is not part of the canonical chain |
| **Fork** | Two or more blocks at the same height with same parent |
| **Reorg** | Switching the canonical chain to a different fork |
| **AuxPoW** | Auxiliary Proof of Work - merge mining with Bitcoin |
| **Cumulative difficulty** | Sum of all block difficulties from genesis |
| **Finality** | Point after which blocks cannot be reorged |

### D. Related Documents

- [Original Reorg Presentation](./reorg-presentation.md) - Historical reference
- [V2 Architecture Overview](../../README.md) - System architecture
- [ChainActor Design](./README.md) - ChainActor details
- [StorageActor Design](../storage/README.md) - Storage patterns

---

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 2.0 | 2026-01 | Engineering | Complete rewrite for 3+ node deployment focus |
| 1.0 | 2025 | Engineering | Original presentation (2-node regtest focus) |

---

## Quick Reference: What Must Be Done Before 3+ Node Deployment

### Blockers (Cannot Deploy Without)

- [ ] **Deep reorg implementation** - `reorganize_deep()` must work
- [ ] **AuxPoW fork choice** - "Most work wins" rule
- [ ] **Parent hash validation** - Prevent chain corruption
- [ ] **Cumulative difficulty tracking** - Storage + ChainState
- [ ] **Basic integration tests** - Verify reorg scenarios

### Recommended (Should Have)

- [ ] SyncActor coordination during reorg
- [ ] Non-canonical block tracking
- [ ] Enhanced metrics and alerting
- [ ] Comprehensive test coverage

### Can Defer

- [ ] Detailed documentation
- [ ] Grafana dashboard updates
- [ ] CLI forensics tools
