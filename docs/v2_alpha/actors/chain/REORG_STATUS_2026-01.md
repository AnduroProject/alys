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

---

### Fork Choice Gap Analysis: Detailed Technical Specification

The following sections provide detailed technical specifications for each missing fork choice component. These specifications are intended to guide implementation.

---

#### Gap FC-1: AuxPoW Difficulty/Work

##### 1.1 Conceptual Overview

**What is AuxPoW Difficulty?**

AuxPoW (Auxiliary Proof of Work) is the mechanism by which Alys blocks are secured through Bitcoin's mining network. When a Bitcoin miner includes an Alys block commitment in their coinbase transaction and successfully mines a Bitcoin block, they produce an AuxPoW proof.

The **difficulty** of an AuxPoW block represents the computational work required to produce it:

```
Difficulty = Maximum Target / Block Target

Where:
- Maximum Target = 0x00000000FFFF... (Bitcoin's maximum target)
- Block Target = value derived from the 'bits' field in the Bitcoin block header
```

Higher difficulty = more computational work = stronger security guarantee.

**Why This Matters for Fork Choice:**

In proof-of-work systems, the fundamental consensus rule is **"most work wins"**. This is not arbitrary - it's the core security assumption:

1. **Economic Security:** More work = more electricity spent = more expensive to attack
2. **Sybil Resistance:** Can't fake computational work without actually doing it
3. **Convergence:** All honest nodes will eventually agree on the chain with most work

Without considering AuxPoW difficulty, Alys's fork choice violates its own security model.

##### 1.2 Current State

**What Exists:**
- AuxPoW validation exists in `auxpow.rs` (validates proofs are correct)
- AuxPoW headers are stored with blocks
- Difficulty target is available in `parent_block.bits`

**What's Missing:**
- No extraction of difficulty value from AuxPoW headers
- No difficulty comparison in fork choice
- No cumulative difficulty tracking
- Fork choice ignores AuxPoW entirely

**Current Vulnerable Code Path:**
```rust
// fork_choice.rs - CURRENT (vulnerable)
fn apply_tiebreaker(block_a, block_b) -> ForkChoice {
    // Only looks at timestamps and hashes
    // A block with NO AuxPoW can beat a block WITH AuxPoW
    // if it has an earlier timestamp!
}
```

##### 1.3 Complete Implementation Specification

**1.3.1 Difficulty Extraction Function**

```rust
// Location: app/src/actors_v2/chain/auxpow.rs

use bitcoin::blockdata::constants::max_target;
use bitcoin::Target;

/// Calculate the difficulty value from an AuxPoW header.
///
/// Returns the difficulty as a u128 to handle Bitcoin's large difficulty values.
/// For blocks without AuxPoW, returns a base difficulty of 1.
///
/// # Formula
/// difficulty = max_target / block_target
///
/// # Example
/// - Bitcoin block with bits=0x1d00ffff has difficulty ≈ 1
/// - Bitcoin block with bits=0x1a0575ee has difficulty ≈ 1,873,105
pub fn calculate_block_difficulty(block: &SignedConsensusBlock) -> u128 {
    // Check if block has AuxPoW
    let auxpow = match &block.message.auxpow {
        Some(auxpow) => auxpow,
        None => {
            // No AuxPoW - return base difficulty
            // This represents an "unsecured" block
            return BASE_DIFFICULTY; // Configurable, recommend: 1
        }
    };

    // Extract the target from the Bitcoin block header's 'bits' field
    let bits = auxpow.parent_block.bits;
    let target = Target::from_compact(bits);

    // Calculate difficulty
    // difficulty = max_target / target
    let max_target = max_target(bitcoin::Network::Bitcoin);

    // Handle division carefully to avoid overflow
    let difficulty = target_to_difficulty(target, max_target);

    tracing::trace!(
        bits = %bits,
        difficulty = difficulty,
        block_hash = %calculate_block_hash(block),
        "Calculated AuxPoW difficulty"
    );

    difficulty
}

/// Convert a target to a difficulty value.
///
/// Uses u256 arithmetic internally to handle Bitcoin's large values,
/// then truncates to u128 (sufficient for practical difficulty values).
fn target_to_difficulty(target: Target, max_target: Target) -> u128 {
    // Difficulty = max_target / target
    // We need arbitrary precision here

    let max_target_u256 = U256::from_be_bytes(max_target.to_be_bytes());
    let target_u256 = U256::from_be_bytes(target.to_be_bytes());

    if target_u256.is_zero() {
        return u128::MAX; // Infinite difficulty (shouldn't happen)
    }

    let difficulty_u256 = max_target_u256 / target_u256;

    // Truncate to u128 - sufficient for Bitcoin difficulty values
    // Current Bitcoin difficulty is ~80 trillion, well within u128
    difficulty_u256.as_u128()
}

/// Base difficulty for blocks without AuxPoW.
///
/// This value represents the "work" of a block that has no proof-of-work.
/// Setting this to 1 means AuxPoW blocks will always beat non-AuxPoW blocks
/// in fork choice (assuming any real Bitcoin difficulty >> 1).
///
/// Configurable based on network requirements.
pub const BASE_DIFFICULTY: u128 = 1;
```

**1.3.2 Fork Choice Integration**

```rust
// Location: app/src/actors_v2/chain/fork_choice.rs

/// Compare two competing blocks considering AuxPoW difficulty.
///
/// # Fork Choice Rules (in priority order)
///
/// 1. **Most Work Wins (Primary):** Block with higher cumulative difficulty wins
/// 2. **Timestamp Tiebreaker:** If difficulty equal, earlier timestamp wins
/// 3. **Hash Tiebreaker:** If timestamp equal, lower hash wins (deterministic)
///
/// # Arguments
///
/// * `current_block` - The block currently in our canonical chain
/// * `current_cumulative_difficulty` - Total difficulty of chain ending at current_block
/// * `new_block` - The competing block received from the network
/// * `new_cumulative_difficulty` - Total difficulty of chain ending at new_block
///
/// # Returns
///
/// * `ForkChoice::KeepCurrent` - Current block wins, reject new block
/// * `ForkChoice::Reorganize` - New block wins, reorganize to it
/// * `ForkChoice::RequiresDeepAnalysis` - Blocks have different parents, need deep reorg
pub fn compare_blocks_with_difficulty(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    current_cumulative_difficulty: u128,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_cumulative_difficulty: u128,
) -> ForkChoice {
    let current_height = current_block.message.execution_payload.block_number;
    let new_height = new_block.message.execution_payload.block_number;
    let current_hash = calculate_block_hash(current_block);
    let new_hash = calculate_block_hash(new_block);

    // Validate same height (for same-height fork comparison)
    if current_height != new_height {
        tracing::warn!(
            current_height = current_height,
            new_height = new_height,
            "compare_blocks called with different heights - use deep reorg"
        );
        return ForkChoice::RequiresDeepAnalysis;
    }

    // === RULE 0: Parent Hash Validation ===
    // True same-height forks must share the same parent
    let current_parent = current_block.message.parent_root;
    let new_parent = new_block.message.parent_root;

    if current_parent != new_parent {
        tracing::warn!(
            current_parent = %current_parent,
            new_parent = %new_parent,
            height = current_height,
            "Same-height blocks have different parents - not a true fork"
        );
        return ForkChoice::RequiresDeepAnalysis;
    }

    // === RULE 1: Most Work Wins (Primary Rule) ===
    // The chain with more cumulative proof-of-work is canonical
    if new_cumulative_difficulty > current_cumulative_difficulty {
        tracing::info!(
            current_difficulty = current_cumulative_difficulty,
            new_difficulty = new_cumulative_difficulty,
            diff = new_cumulative_difficulty - current_cumulative_difficulty,
            winner = "new_block",
            "Fork choice: new block has more work"
        );
        return ForkChoice::Reorganize {
            new_tip: new_hash,
            rollback_to: current_height,
        };
    } else if current_cumulative_difficulty > new_cumulative_difficulty {
        tracing::info!(
            current_difficulty = current_cumulative_difficulty,
            new_difficulty = new_cumulative_difficulty,
            diff = current_cumulative_difficulty - new_cumulative_difficulty,
            winner = "current_block",
            "Fork choice: current block has more work"
        );
        return ForkChoice::KeepCurrent;
    }

    // Difficulties are equal - fall through to tiebreakers
    tracing::debug!(
        difficulty = current_cumulative_difficulty,
        "Difficulties equal, using tiebreakers"
    );

    // === RULE 2: Timestamp Tiebreaker ===
    let current_timestamp = current_block.message.execution_payload.timestamp;
    let new_timestamp = new_block.message.execution_payload.timestamp;

    if new_timestamp < current_timestamp {
        tracing::info!(
            current_timestamp = current_timestamp,
            new_timestamp = new_timestamp,
            winner = "new_block",
            "Fork choice: new block has earlier timestamp"
        );
        return ForkChoice::Reorganize {
            new_tip: new_hash,
            rollback_to: current_height,
        };
    } else if current_timestamp < new_timestamp {
        tracing::info!(
            current_timestamp = current_timestamp,
            new_timestamp = new_timestamp,
            winner = "current_block",
            "Fork choice: current block has earlier timestamp"
        );
        return ForkChoice::KeepCurrent;
    }

    // Timestamps are equal - fall through to hash tiebreaker
    tracing::debug!(
        timestamp = current_timestamp,
        "Timestamps equal, using hash tiebreaker"
    );

    // === RULE 3: Hash Tiebreaker (Deterministic Fallback) ===
    // Lower hash wins - ensures all nodes make the same decision
    if new_hash < current_hash {
        tracing::info!(
            current_hash = %current_hash,
            new_hash = %new_hash,
            winner = "new_block",
            "Fork choice: new block has lower hash"
        );
        ForkChoice::Reorganize {
            new_tip: new_hash,
            rollback_to: current_height,
        }
    } else {
        tracing::info!(
            current_hash = %current_hash,
            new_hash = %new_hash,
            winner = "current_block",
            "Fork choice: current block has lower hash (or equal)"
        );
        ForkChoice::KeepCurrent
    }
}
```

##### 1.4 Edge Cases

| Edge Case | Behavior | Rationale |
|-----------|----------|-----------|
| Neither block has AuxPoW | Both have BASE_DIFFICULTY=1, use timestamp tiebreaker | Fair comparison |
| One block has AuxPoW, other doesn't | AuxPoW block always wins | Security requirement |
| Both have AuxPoW with same difficulty | Extremely rare, use timestamp | Difficulty is derived from Bitcoin blocks |
| AuxPoW with difficulty=0 | Treat as BASE_DIFFICULTY | Invalid AuxPoW shouldn't give advantage |
| Difficulty overflow (u128) | Saturate at u128::MAX | Theoretical, current Bitcoin difficulty is ~80T |

##### 1.5 Testing Requirements

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auxpow_beats_no_auxpow() {
        let block_with_auxpow = create_block_with_auxpow(difficulty: 1_000_000);
        let block_without_auxpow = create_block_without_auxpow();

        // Even if non-AuxPoW has earlier timestamp
        block_without_auxpow.timestamp = 1000;
        block_with_auxpow.timestamp = 2000;

        let result = compare_blocks_with_difficulty(
            &block_without_auxpow, BASE_DIFFICULTY,
            &block_with_auxpow, 1_000_000,
        );

        assert!(matches!(result, ForkChoice::Reorganize { .. }));
    }

    #[test]
    fn test_higher_difficulty_wins() {
        let block_a = create_block_with_auxpow(difficulty: 1_000_000);
        let block_b = create_block_with_auxpow(difficulty: 2_000_000);

        let result = compare_blocks_with_difficulty(
            &block_a, 1_000_000,
            &block_b, 2_000_000,
        );

        assert!(matches!(result, ForkChoice::Reorganize { .. }));
    }

    #[test]
    fn test_equal_difficulty_uses_timestamp() {
        let mut block_a = create_block_with_auxpow(difficulty: 1_000_000);
        let mut block_b = create_block_with_auxpow(difficulty: 1_000_000);

        block_a.timestamp = 2000;
        block_b.timestamp = 1000; // Earlier

        let result = compare_blocks_with_difficulty(
            &block_a, 1_000_000,
            &block_b, 1_000_000,
        );

        assert!(matches!(result, ForkChoice::Reorganize { .. }));
    }
}
```

---

#### Gap FC-2: Cumulative Chain Weight

##### 2.1 Conceptual Overview

**What is Cumulative Chain Weight?**

Cumulative chain weight (also called "total work" or "chainwork") is the sum of all difficulty values from genesis to the current block:

```
cumulative_difficulty(block_N) = Σ difficulty(block_i) for i = 0 to N
                                = cumulative_difficulty(block_N-1) + difficulty(block_N)
```

This is the fundamental metric for comparing chains in proof-of-work systems.

**Why Cumulative, Not Per-Block?**

Consider this scenario:
```
Chain A: [100 work] → [100 work] → [100 work] → [100 work]
         Total: 400 work

Chain B: [50 work] → [50 work] → [50 work] → [50 work] → [200 work]
         Total: 400 work
```

Per-block difficulty doesn't tell you which chain is "better" - you need the cumulative sum.

**Why This Matters for Fork Choice:**

1. **Deep Reorgs:** When comparing chains that diverged many blocks ago, you need cumulative difficulty to determine which chain has more total work
2. **Security Guarantee:** A longer chain with less total work should NOT beat a shorter chain with more total work
3. **Attack Resistance:** Attackers can't win by producing many low-difficulty blocks quickly

##### 2.2 Current State

**What's Missing:**
- No `cumulative_difficulty` field in ChainState
- No difficulty stored per-block in database
- No way to calculate total chain work
- Deep reorg can't compare chain weights

**Impact:**
```
Scenario: Network partition heals after 10 blocks

Chain A (our chain): 10 blocks, all with AuxPoW (high difficulty)
Chain B (their chain): 15 blocks, none with AuxPoW (base difficulty)

Current behavior: Can't compare - deep reorg not implemented
Correct behavior: Chain A should win (more total work despite fewer blocks)
```

##### 2.3 Complete Implementation Specification

**2.3.1 Storage Schema**

```rust
// Location: app/src/actors_v2/storage/database.rs

/// Column family for storing cumulative difficulty per block height.
///
/// Key: block height (u64, big-endian)
/// Value: cumulative difficulty (u128, big-endian)
///
/// This enables O(1) lookup of cumulative difficulty at any height,
/// which is required for efficient fork choice comparison.
pub const CUMULATIVE_DIFFICULTY: &str = "cumulative_difficulty";

impl Database {
    /// Store the cumulative difficulty at a given height.
    pub fn put_cumulative_difficulty(
        &self,
        height: u64,
        cumulative_difficulty: u128,
    ) -> Result<(), StorageError> {
        let cf = self.db.cf_handle(CUMULATIVE_DIFFICULTY)
            .ok_or(StorageError::ColumnFamilyNotFound)?;

        let key = height.to_be_bytes();
        let value = cumulative_difficulty.to_be_bytes();

        self.db.put_cf(&cf, key, value)?;

        tracing::trace!(
            height = height,
            cumulative_difficulty = cumulative_difficulty,
            "Stored cumulative difficulty"
        );

        Ok(())
    }

    /// Retrieve the cumulative difficulty at a given height.
    ///
    /// Returns None if no difficulty is stored at that height.
    pub fn get_cumulative_difficulty(
        &self,
        height: u64,
    ) -> Result<Option<u128>, StorageError> {
        let cf = self.db.cf_handle(CUMULATIVE_DIFFICULTY)
            .ok_or(StorageError::ColumnFamilyNotFound)?;

        let key = height.to_be_bytes();

        match self.db.get_cf(&cf, key)? {
            Some(bytes) => {
                let arr: [u8; 16] = bytes.as_slice().try_into()
                    .map_err(|_| StorageError::InvalidData)?;
                Ok(Some(u128::from_be_bytes(arr)))
            }
            None => Ok(None),
        }
    }

    /// Get the cumulative difficulty at the chain tip.
    ///
    /// Convenience method that combines get_chain_height + get_cumulative_difficulty.
    pub fn get_tip_cumulative_difficulty(&self) -> Result<u128, StorageError> {
        let head = self.get_chain_head()?
            .ok_or(StorageError::ChainNotInitialized)?;

        self.get_cumulative_difficulty(head.number)?
            .ok_or(StorageError::DifficultyNotFound { height: head.number })
    }
}
```

**2.3.2 ChainState Integration**

```rust
// Location: app/src/actors_v2/chain/state.rs

pub struct ChainState {
    // ... existing fields ...

    /// Cumulative difficulty of the current canonical chain tip.
    ///
    /// This is cached in memory for fast fork choice decisions.
    /// It's updated on every block import and reorg.
    ///
    /// Formula: cumulative_difficulty = parent_cumulative_difficulty + block_difficulty
    pub cumulative_difficulty: u128,

    /// Cache of recent cumulative difficulties for fork choice.
    ///
    /// Stores the last N heights' cumulative difficulties to avoid
    /// frequent database lookups during fork detection.
    ///
    /// Key: height, Value: cumulative_difficulty
    pub difficulty_cache: LruCache<u64, u128>,
}

impl ChainState {
    /// Update cumulative difficulty when a new block is imported.
    pub fn update_difficulty_on_import(
        &mut self,
        new_block: &SignedConsensusBlock,
        storage: &Database,
    ) -> Result<(), ChainError> {
        let block_height = new_block.message.execution_payload.block_number;
        let block_difficulty = calculate_block_difficulty(new_block);

        // Get parent's cumulative difficulty
        let parent_cumulative = if block_height == 0 {
            0 // Genesis block
        } else {
            self.get_cumulative_difficulty_at(block_height - 1, storage)?
        };

        // Calculate new cumulative difficulty
        let new_cumulative = parent_cumulative.saturating_add(block_difficulty);

        // Update state
        self.cumulative_difficulty = new_cumulative;
        self.difficulty_cache.put(block_height, new_cumulative);

        // Persist to storage
        storage.put_cumulative_difficulty(block_height, new_cumulative)?;

        tracing::debug!(
            height = block_height,
            block_difficulty = block_difficulty,
            parent_cumulative = parent_cumulative,
            new_cumulative = new_cumulative,
            "Updated cumulative difficulty"
        );

        Ok(())
    }

    /// Get cumulative difficulty at a specific height.
    ///
    /// Checks cache first, then falls back to database.
    pub fn get_cumulative_difficulty_at(
        &mut self,
        height: u64,
        storage: &Database,
    ) -> Result<u128, ChainError> {
        // Check cache first
        if let Some(&difficulty) = self.difficulty_cache.get(&height) {
            return Ok(difficulty);
        }

        // Fall back to database
        let difficulty = storage.get_cumulative_difficulty(height)?
            .ok_or(ChainError::DifficultyNotFound { height })?;

        // Cache for future lookups
        self.difficulty_cache.put(height, difficulty);

        Ok(difficulty)
    }
}
```

**2.3.3 Block Import Integration**

```rust
// Location: app/src/actors_v2/chain/handlers.rs (block import path)

async fn import_block(
    &mut self,
    block: SignedConsensusBlock,
    correlation_id: Uuid,
) -> Result<ChainResponse, ChainError> {
    // ... existing validation ...

    // Calculate and store cumulative difficulty
    let block_height = block.message.execution_payload.block_number;
    let block_difficulty = calculate_block_difficulty(&block);

    let parent_cumulative = if block_height == 0 {
        0
    } else {
        self.state.get_cumulative_difficulty_at(
            block_height - 1,
            &self.storage
        )?
    };

    let new_cumulative = parent_cumulative.saturating_add(block_difficulty);

    // Store block with difficulty
    self.storage_actor.send(StoreBlockMessage {
        block: block.clone(),
        canonical: true,
        cumulative_difficulty: Some(new_cumulative), // NEW FIELD
        correlation_id: Some(correlation_id),
    }).await??;

    // Update state
    self.state.cumulative_difficulty = new_cumulative;
    self.state.difficulty_cache.put(block_height, new_cumulative);

    // ... rest of import ...
}
```

##### 2.4 Migration Strategy

For existing chains that don't have cumulative difficulty stored:

```rust
/// Migrate an existing chain to include cumulative difficulty.
///
/// This should be run once during upgrade. It walks the chain from genesis
/// to tip, calculating and storing cumulative difficulty at each height.
pub async fn migrate_cumulative_difficulty(
    storage: &Database,
) -> Result<(), MigrationError> {
    let tip_height = storage.get_chain_height()?
        .ok_or(MigrationError::NoChain)?;

    let mut cumulative: u128 = 0;

    for height in 0..=tip_height {
        let block = storage.get_block_by_height(height)?
            .ok_or(MigrationError::MissingBlock { height })?;

        let block_difficulty = calculate_block_difficulty(&block);
        cumulative = cumulative.saturating_add(block_difficulty);

        storage.put_cumulative_difficulty(height, cumulative)?;

        if height % 1000 == 0 {
            tracing::info!(
                height = height,
                cumulative = cumulative,
                "Migration progress"
            );
        }
    }

    tracing::info!(
        tip_height = tip_height,
        final_cumulative = cumulative,
        "Cumulative difficulty migration complete"
    );

    Ok(())
}
```

##### 2.5 Deep Reorg Integration

```rust
/// Compare cumulative difficulties of two chains for deep reorg decision.
///
/// # Arguments
/// * `our_tip_height` - Height of our canonical chain tip
/// * `their_tip_height` - Height of the competing chain tip
/// * `common_ancestor_height` - Height where chains diverged
///
/// # Returns
/// * `true` if their chain has more cumulative work (should reorg)
/// * `false` if our chain has more or equal work (keep current)
pub async fn should_reorg_to_chain(
    our_tip_height: u64,
    our_cumulative_difficulty: u128,
    their_tip_height: u64,
    their_cumulative_difficulty: u128,
) -> bool {
    // Primary rule: Most work wins
    if their_cumulative_difficulty > our_cumulative_difficulty {
        tracing::info!(
            our_difficulty = our_cumulative_difficulty,
            their_difficulty = their_cumulative_difficulty,
            our_height = our_tip_height,
            their_height = their_tip_height,
            "Their chain has more work - should reorg"
        );
        return true;
    }

    if their_cumulative_difficulty < our_cumulative_difficulty {
        tracing::info!(
            our_difficulty = our_cumulative_difficulty,
            their_difficulty = their_cumulative_difficulty,
            "Our chain has more work - keep current"
        );
        return false;
    }

    // Equal difficulty - secondary rules
    // For deep reorgs, we generally prefer to keep our chain if difficulty is equal
    // This provides stability and prevents unnecessary reorgs
    tracing::info!(
        difficulty = our_cumulative_difficulty,
        "Equal difficulty - keeping current chain for stability"
    );
    false
}
```

---

#### Gap FC-3: Parent Hash Validation

##### 3.1 Conceptual Overview

**What is Parent Hash Validation?**

Every block contains a `parent_hash` field that points to the previous block in the chain:

```
Block N:   { parent_hash: hash(Block N-1), ... }
Block N-1: { parent_hash: hash(Block N-2), ... }
```

For a **same-height fork** to be valid, both competing blocks must have the same parent:

```
Valid Same-Height Fork:
    Block 99 ──┬── Block 100a (parent_hash = hash(99))
               └── Block 100b (parent_hash = hash(99))
                   ↑ Both point to same parent

Invalid "Same-Height Fork":
    Block 99a ──── Block 100a (parent_hash = hash(99a))
    Block 99b ──── Block 100b (parent_hash = hash(99b))
                   ↑ Different parents! This is a DEEP fork, not same-height!
```

**Why This Matters:**

Without parent hash validation, the fork choice rule can make incorrect decisions:

1. **Chain Corruption:** Accepting a block with a parent we don't have breaks the chain
2. **Security Bypass:** Attacker can send blocks from a completely different chain
3. **Consensus Failure:** Different nodes may make inconsistent decisions

##### 3.2 Current State

**What's Missing:**

The current `compare_blocks()` function in `fork_choice.rs` does NOT validate parent hashes:

```rust
// CURRENT CODE (vulnerable)
pub fn compare_blocks(
    current_block: &SignedConsensusBlock,
    new_block: &SignedConsensusBlock,
) -> ForkChoice {
    // Checks height match ✓
    if current_height != new_height { ... }

    // Applies tiebreaker ✓
    apply_tiebreaker(current_block, new_block)

    // MISSING: Parent hash validation ✗
}
```

**Attack Scenario:**

```
Honest network:  Genesis → A1 → A2 → A3 → A4 → A5 (height 5)

Attacker creates: Genesis' → B1 → B2 → B3 → B4 → B5 (height 5)
                  (Different genesis, completely separate chain)

Attacker sends B5 to victim node:
- B5.height == 5 (matches our height)
- B5.timestamp < A5.timestamp (attacker sets earlier timestamp)

Current behavior: Victim accepts B5 and reorganizes to it!
Result: Chain is now broken - B5's parent (B4) doesn't exist in victim's database
```

##### 3.3 Complete Implementation Specification

**3.3.1 Add Parent Hash Check**

```rust
// Location: app/src/actors_v2/chain/fork_choice.rs

/// Result of parent hash validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ParentHashValidation {
    /// Both blocks share the same parent - valid same-height fork
    Valid,
    /// Blocks have different parents - this is a deep fork, not same-height
    DifferentParents {
        current_parent: H256,
        new_parent: H256,
    },
    /// New block's parent doesn't exist in our chain
    ParentNotFound {
        missing_parent: H256,
    },
}

/// Validate that two same-height blocks share the same parent.
///
/// This MUST be called before comparing blocks for same-height fork choice.
/// If validation fails, the blocks should be handled as a deep fork instead.
pub fn validate_parent_hashes(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
) -> ParentHashValidation {
    let current_parent = current_block.message.parent_root;
    let new_parent = new_block.message.parent_root;

    if current_parent == new_parent {
        tracing::trace!(
            parent_hash = %current_parent,
            "Parent hash validation passed - same parent"
        );
        ParentHashValidation::Valid
    } else {
        tracing::warn!(
            current_parent = %current_parent,
            new_parent = %new_parent,
            height = current_block.message.execution_payload.block_number,
            "Parent hash mismatch - blocks have different parents"
        );
        ParentHashValidation::DifferentParents {
            current_parent,
            new_parent,
        }
    }
}

/// Extended fork choice that includes parent validation.
///
/// This is the main entry point for fork choice decisions.
pub fn compare_blocks_full(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    current_cumulative_difficulty: u128,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_cumulative_difficulty: u128,
) -> ForkChoice {
    // Step 1: Validate parent hashes
    match validate_parent_hashes(current_block, new_block) {
        ParentHashValidation::Valid => {
            // Continue to normal fork choice
        }
        ParentHashValidation::DifferentParents { current_parent, new_parent } => {
            // This is not a same-height fork - needs deep analysis
            tracing::info!(
                current_parent = %current_parent,
                new_parent = %new_parent,
                "Blocks at same height have different parents - requires deep reorg analysis"
            );
            return ForkChoice::RequiresDeepAnalysis;
        }
        ParentHashValidation::ParentNotFound { missing_parent } => {
            // New block references a parent we don't have
            tracing::warn!(
                missing_parent = %missing_parent,
                "New block references unknown parent - may need to sync"
            );
            return ForkChoice::RequiresDeepAnalysis;
        }
    }

    // Step 2: Apply difficulty-aware fork choice (from FC-1 and FC-2)
    compare_blocks_with_difficulty(
        current_block,
        current_cumulative_difficulty,
        new_block,
        new_cumulative_difficulty,
    )
}
```

**3.3.2 Handler Integration**

```rust
// Location: app/src/actors_v2/chain/handlers.rs

async fn handle_fork_at_same_height(
    &mut self,
    existing_block: &SignedConsensusBlock,
    new_block: &SignedConsensusBlock,
    correlation_id: Uuid,
) -> Result<ChainResponse, ChainError> {
    // Get cumulative difficulties
    let current_height = existing_block.message.execution_payload.block_number;
    let current_cumulative = self.state.cumulative_difficulty;
    let new_cumulative = self.calculate_cumulative_for_block(new_block)?;

    // Run full fork choice with parent validation
    match fork_choice::compare_blocks_full(
        existing_block,
        current_cumulative,
        new_block,
        new_cumulative,
    ) {
        ForkChoice::KeepCurrent => {
            tracing::info!(
                height = current_height,
                "Fork choice: keeping current block"
            );
            Ok(ChainResponse::BlockRejected {
                reason: "Lost fork choice".into(),
            })
        }

        ForkChoice::Reorganize { new_tip, rollback_to } => {
            tracing::warn!(
                height = current_height,
                new_tip = %new_tip,
                "Fork choice: reorganizing to new block"
            );
            self.execute_simple_reorg(new_block, correlation_id).await
        }

        ForkChoice::RequiresDeepAnalysis => {
            // Blocks have different parents - this is a deep fork
            tracing::warn!(
                height = current_height,
                "Same-height blocks have different parents - initiating deep reorg analysis"
            );

            // Find common ancestor and compare chains
            self.handle_deep_fork(existing_block, new_block, correlation_id).await
        }

        ForkChoice::Tiebreak { winner } => {
            // Handle explicit tiebreak result
            if winner == calculate_block_hash(new_block) {
                self.execute_simple_reorg(new_block, correlation_id).await
            } else {
                Ok(ChainResponse::BlockRejected {
                    reason: "Lost tiebreaker".into(),
                })
            }
        }
    }
}
```

##### 3.4 Edge Cases

| Scenario | Current Behavior | Correct Behavior |
|----------|-----------------|------------------|
| Same parent, different blocks | Tiebreaker | Tiebreaker (correct) |
| Different parents, same height | Tiebreaker (WRONG) | `RequiresDeepAnalysis` |
| Parent not in database | May corrupt chain | `RequiresDeepAnalysis` + sync |
| Genesis blocks (no parent) | Undefined | Special case: compare genesis hashes |
| Orphan block (parent not yet received) | May reject valid block | Queue for later processing |

---

#### Gap FC-4: Block Validity Beyond Height

##### 4.1 Conceptual Overview

**What is "Block Validity Beyond Height"?**

Currently, fork choice only verifies that blocks have matching heights. A complete implementation should verify additional validity criteria:

1. **Execution Validity:** Block's transactions are valid and execution is correct
2. **Consensus Validity:** Block satisfies consensus rules (signatures, proposer, etc.)
3. **AuxPoW Validity:** If present, AuxPoW proof is cryptographically valid
4. **State Validity:** Block's state root matches expected post-execution state
5. **Temporal Validity:** Block timestamp is within acceptable bounds

**Why This Matters:**

A block can have the correct height but still be invalid:

```
Valid block at height 100:
- Correct parent hash ✓
- Valid transactions ✓
- Valid execution ✓
- Valid state root ✓
- Valid timestamp ✓
- Valid AuxPoW (if present) ✓

Invalid block at height 100:
- Correct parent hash ✓
- Invalid transaction (double spend) ✗
- Or: Invalid state root ✗
- Or: Future timestamp ✗
- Or: Invalid AuxPoW proof ✗
```

Fork choice should NEVER prefer an invalid block over a valid one, regardless of difficulty or timestamp.

##### 4.2 Current State

**What Validation Currently Happens:**
- Block height is checked
- Basic structural validation (can deserialize)
- AuxPoW validation happens during block production (not import)

**What's NOT Validated During Fork Choice:**
- Execution validity (transactions, state transitions)
- Consensus validity (proposer, signatures in some paths)
- AuxPoW validity for received blocks
- Timestamp bounds

**Assumption Made:**
The current code assumes blocks received from the network have been pre-validated. This assumption may not hold in adversarial scenarios.

##### 4.3 Complete Implementation Specification

**4.3.1 Block Validity Check Before Fork Choice**

```rust
// Location: app/src/actors_v2/chain/validation.rs (new file)

use crate::actors_v2::chain::auxpow;

/// Result of full block validation.
#[derive(Debug)]
pub enum BlockValidation {
    Valid,
    Invalid(BlockInvalidReason),
}

#[derive(Debug)]
pub enum BlockInvalidReason {
    /// Block execution failed
    ExecutionFailed { error: String },
    /// State root doesn't match
    StateRootMismatch { expected: H256, actual: H256 },
    /// AuxPoW proof is invalid
    InvalidAuxPow { error: String },
    /// Timestamp is in the future
    FutureTimestamp { block_time: u64, current_time: u64 },
    /// Timestamp is too old
    TimestampTooOld { block_time: u64, min_time: u64 },
    /// Invalid proposer/validator
    InvalidProposer { expected: Address, actual: Address },
    /// Signature verification failed
    InvalidSignature { error: String },
    /// Parent block not found
    ParentNotFound { parent_hash: H256 },
}

/// Validate a block before considering it for fork choice.
///
/// This performs full validation to ensure we never prefer an invalid block.
///
/// # Validation Steps
/// 1. Validate timestamp bounds
/// 2. Validate AuxPoW (if present)
/// 3. Validate proposer/validator
/// 4. Validate block signature
/// 5. Note: Execution validation happens separately in EngineActor
pub async fn validate_block_for_fork_choice(
    block: &SignedConsensusBlock<MainnetEthSpec>,
    chain_state: &ChainState,
    current_time: u64,
) -> BlockValidation {
    let block_time = block.message.execution_payload.timestamp;
    let block_hash = calculate_block_hash(block);

    // === Step 1: Timestamp Validation ===
    // Block timestamp must not be more than 15 seconds in the future
    const MAX_FUTURE_SECONDS: u64 = 15;
    if block_time > current_time + MAX_FUTURE_SECONDS {
        tracing::warn!(
            block_time = block_time,
            current_time = current_time,
            block_hash = %block_hash,
            "Block timestamp too far in future"
        );
        return BlockValidation::Invalid(BlockInvalidReason::FutureTimestamp {
            block_time,
            current_time,
        });
    }

    // Block timestamp must not be before parent timestamp
    if let Some(parent_time) = chain_state.last_block_time {
        let parent_timestamp = parent_time.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if block_time < parent_timestamp {
            tracing::warn!(
                block_time = block_time,
                parent_time = parent_timestamp,
                block_hash = %block_hash,
                "Block timestamp before parent"
            );
            return BlockValidation::Invalid(BlockInvalidReason::TimestampTooOld {
                block_time,
                min_time: parent_timestamp,
            });
        }
    }

    // === Step 2: AuxPoW Validation ===
    if let Some(ref auxpow_header) = block.message.auxpow {
        match auxpow::validate_auxpow(auxpow_header, &block.message) {
            Ok(()) => {
                tracing::trace!(block_hash = %block_hash, "AuxPoW validation passed");
            }
            Err(e) => {
                tracing::warn!(
                    block_hash = %block_hash,
                    error = %e,
                    "AuxPoW validation failed"
                );
                return BlockValidation::Invalid(BlockInvalidReason::InvalidAuxPow {
                    error: e.to_string(),
                });
            }
        }
    }

    // === Step 3: Validator/Proposer Check ===
    // Verify the block was produced by a valid validator for this slot
    let proposer = block.message.proposer_index;
    if !chain_state.aura.is_valid_proposer(proposer, block_time) {
        tracing::warn!(
            proposer = proposer,
            block_time = block_time,
            block_hash = %block_hash,
            "Invalid proposer for this slot"
        );
        // Note: For now, log warning but don't reject
        // Full Aura validation requires more context
    }

    // === Step 4: Signature Validation ===
    // Verify the block signature is valid
    match verify_block_signature(block, chain_state) {
        Ok(()) => {
            tracing::trace!(block_hash = %block_hash, "Signature validation passed");
        }
        Err(e) => {
            tracing::warn!(
                block_hash = %block_hash,
                error = %e,
                "Signature validation failed"
            );
            return BlockValidation::Invalid(BlockInvalidReason::InvalidSignature {
                error: e.to_string(),
            });
        }
    }

    BlockValidation::Valid
}
```

**4.3.2 Integration with Fork Choice**

```rust
// Location: app/src/actors_v2/chain/handlers.rs

async fn handle_network_block(
    &mut self,
    block: SignedConsensusBlock,
    correlation_id: Uuid,
) -> Result<ChainResponse, ChainError> {
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // === Step 1: Validate block before any fork choice logic ===
    match validation::validate_block_for_fork_choice(
        &block,
        &self.state,
        current_time,
    ).await {
        BlockValidation::Valid => {
            tracing::debug!(
                block_hash = %calculate_block_hash(&block),
                "Block passed pre-fork-choice validation"
            );
        }
        BlockValidation::Invalid(reason) => {
            tracing::warn!(
                block_hash = %calculate_block_hash(&block),
                reason = ?reason,
                "Block failed validation - rejecting"
            );
            self.metrics.blocks_rejected_invalid.inc();
            return Ok(ChainResponse::BlockRejected {
                reason: format!("Invalid block: {:?}", reason),
            });
        }
    }

    // === Step 2: Check if this creates a fork ===
    let block_height = block.message.execution_payload.block_number;

    if let Some(existing_block) = self.get_block_at_height(block_height).await? {
        // Fork detected - run fork choice
        self.handle_fork_at_same_height(&existing_block, &block, correlation_id).await
    } else if block_height == self.state.head.map(|h| h.number + 1).unwrap_or(0) {
        // Normal next block
        self.import_block(block, correlation_id).await
    } else {
        // Gap or future block
        self.handle_out_of_order_block(block, correlation_id).await
    }
}
```

##### 4.4 Validation Order and Performance

**Validation should be ordered by cost (cheapest first):**

| Order | Validation | Cost | Reason |
|-------|------------|------|--------|
| 1 | Height check | O(1) | Simple comparison |
| 2 | Timestamp bounds | O(1) | Simple comparison |
| 3 | Parent hash | O(1) | Hash comparison |
| 4 | Signature | O(1) | Cryptographic verify |
| 5 | AuxPoW proof | O(log n) | Merkle proof verify |
| 6 | Execution | O(n) | Full EVM execution |

**Early Exit Strategy:**
```rust
// Fail fast on cheap checks
if !check_height() { return Invalid; }      // Free
if !check_timestamp() { return Invalid; }   // Free
if !check_parent() { return Invalid; }      // Free
if !check_signature() { return Invalid; }   // ~100μs
if !check_auxpow() { return Invalid; }      // ~1ms
// Only do expensive execution if all above pass
if !check_execution() { return Invalid; }   // ~10-100ms
```

##### 4.5 Summary: Complete Fork Choice Implementation

Putting it all together, the complete fork choice flow should be:

```
┌─────────────────────────────────────────────────────────────────┐
│                    COMPLETE FORK CHOICE FLOW                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   Receive Block from Network                                    │
│         │                                                        │
│         ▼                                                        │
│   ┌─────────────────────────────────────┐                       │
│   │ STEP 1: Pre-Validation (FC-4)       │                       │
│   │ - Timestamp bounds                   │                       │
│   │ - AuxPoW validity                    │                       │
│   │ - Signature validity                 │                       │
│   └─────────────────────────────────────┘                       │
│         │                                                        │
│         ▼ (pass)                                                 │
│   ┌─────────────────────────────────────┐                       │
│   │ STEP 2: Parent Validation (FC-3)    │                       │
│   │ - Check parent hash matches         │                       │
│   │ - Verify parent exists              │                       │
│   └─────────────────────────────────────┘                       │
│         │                                                        │
│         ├─── Different Parents ──▶ Deep Reorg Analysis          │
│         │                                                        │
│         ▼ (same parent)                                         │
│   ┌─────────────────────────────────────┐                       │
│   │ STEP 3: Get Cumulative Difficulty   │                       │
│   │ (FC-2)                              │                       │
│   │ - Current chain difficulty          │                       │
│   │ - New block chain difficulty        │                       │
│   └─────────────────────────────────────┘                       │
│         │                                                        │
│         ▼                                                        │
│   ┌─────────────────────────────────────┐                       │
│   │ STEP 4: Fork Choice Rules (FC-1)    │                       │
│   │ - Rule 1: Most work wins            │                       │
│   │ - Rule 2: Timestamp tiebreaker      │                       │
│   │ - Rule 3: Hash tiebreaker           │                       │
│   └─────────────────────────────────────┘                       │
│         │                                                        │
│         ├─── KeepCurrent ──▶ Reject new block                   │
│         │                                                        │
│         ▼ (Reorganize)                                          │
│   ┌─────────────────────────────────────┐                       │
│   │ STEP 5: Execute Reorg               │                       │
│   │ - Update StorageActor               │                       │
│   │ - Update EngineActor                │                       │
│   │ - Update ChainState                 │                       │
│   │ - Emit Metrics                      │                       │
│   └─────────────────────────────────────┘                       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

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
