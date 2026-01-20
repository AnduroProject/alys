# Chain Reorganization in Alys V2 - Status & Implementation Plan

**Document Version:** 2.2
**Date:** January 20, 2026
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

### Key Decisions Made

1. ✅ **AuxPoW finality model:** Soft finality with 6 confirmations (not hard finality)
2. ✅ **Fork choice rule:** "Most work wins" (cumulative difficulty) as primary rule
3. ✅ **Maximum reorg depth:** 100 blocks automatic, alerts at 10, operator override for deeper
4. ⏳ **Deployment timeline:** Pending decision

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

##### 2.4 Deep Reorg Integration

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

**3.3.1 ParentHashValidation Enum Definition**

```rust
// Location: app/src/actors_v2/chain/fork_choice.rs

/// Result of validating parent hashes between two competing same-height blocks.
///
/// When we receive a block at the same height as our current tip, we must verify
/// that both blocks have the SAME parent before running fork choice. If they have
/// different parents, this isn't a simple same-height fork - it's a deeper divergence.
#[derive(Debug, Clone, PartialEq)]
pub enum ParentHashValidation {
    /// Both blocks share the same parent - this is a valid same-height fork.
    /// Fork choice can proceed with tiebreaker rules.
    Valid,

    /// Blocks have different parents despite being at the same height.
    /// This indicates chains diverged earlier - requires deep reorg analysis.
    DifferentParents {
        current_parent: H256,
        new_parent: H256,
    },

    /// The new block references a parent hash we don't have in our database.
    /// Either we're missing blocks or this block is from a completely different chain.
    ParentNotFound {
        missing_parent: H256,
    },
}
```

---

**Variant 1: `Valid`**

**Meaning:** Both blocks extend from the same parent. This is a true same-height fork caused by two validators producing blocks simultaneously.

*Example Scenario:*
```
Timeline:
  t=0:  Block 99 is canonical on all nodes
  t=1:  Validator A produces Block 100a
  t=1:  Validator B produces Block 100b (simultaneously)
  t=2:  Your node has 100a, receives 100b from network
```

*Your Node's View:*
```
YOUR LOCAL CHAIN:

    ┌──────────┐      ┌───────────┐
    │ Block 99 │─────▶│ Block 100a│  ◀── Your canonical tip
    │ hash: 0x99│      │ hash: 0xAA│
    └──────────┘      │ parent: 0x99│
                      └───────────┘
```

*Block Received from Network:*
```
INCOMING BLOCK:

    ┌───────────┐
    │ Block 100b│  ◀── Received from peer
    │ hash: 0xBB│
    │ parent: 0x99│  ◀── SAME parent as 100a!
    └───────────┘
```

*Validation Result:*
```rust
validate_parent_hashes(&block_100a, &block_100b, &storage).await
// Returns: ParentHashValidation::Valid
//
// Because:
//   block_100a.parent_root = 0x99
//   block_100b.parent_root = 0x99
//   They match! This is a valid same-height fork.
```

*What Happens Next:* Fork choice proceeds with tiebreaker (difficulty → timestamp → hash). One block becomes canonical, other becomes orphan.

---

**Variant 2: `DifferentParents`**

**Meaning:** The blocks are at the same height but extend from different parents. This means the chains diverged **earlier than this height** - it's a deep fork masquerading as a same-height fork.

*Example Scenario:*
```
Timeline:
  t=0:  Network partition occurs
  t=1:  Partition A: Block 99a is produced
  t=1:  Partition B: Block 99b is produced (different from 99a!)
  t=2:  Partition A: Block 100a (child of 99a)
  t=2:  Partition B: Block 100b (child of 99b)
  t=3:  Partition heals, your node receives 100b
```

*Your Node's View:*
```
YOUR LOCAL CHAIN:

    ┌──────────┐      ┌───────────┐      ┌───────────┐
    │ Block 98 │─────▶│ Block 99a │─────▶│ Block 100a│  ◀── Your canonical tip
    │ hash: 0x98│      │ hash: 0x9A│      │ hash: 0xAA│
    └──────────┘      └───────────┘      │ parent: 0x9A│
                                         └───────────┘
```

*Block Received from Network:*
```
INCOMING BLOCK:

    ┌───────────┐
    │ Block 100b│  ◀── Received from peer
    │ hash: 0xBB│
    │ parent: 0x9B│  ◀── DIFFERENT parent! Points to Block 99b
    └───────────┘

    You don't have Block 99b (hash: 0x9B) - it was produced
    on the other partition
```

*The Full Picture (actual chain structure):*
```
                         ┌───────────┐      ┌───────────┐
                    ┌───▶│ Block 99a │─────▶│ Block 100a│  ◀── Your chain
    ┌──────────┐    │    │ hash: 0x9A│      │ parent: 0x9A│
    │ Block 98 │────┤    └───────────┘      └───────────┘
    │ hash: 0x98│    │
    └──────────┘    │    ┌───────────┐      ┌───────────┐
                    └───▶│ Block 99b │─────▶│ Block 100b│  ◀── Their chain
                         │ hash: 0x9B│      │ parent: 0x9B│
                         └───────────┘      └───────────┘

    Fork point is Block 98, NOT Block 99!
```

*Validation Result:*
```rust
validate_parent_hashes(&block_100a, &block_100b, &storage).await
// Returns: ParentHashValidation::DifferentParents {
//     current_parent: 0x9A,  // Your block's parent
//     new_parent: 0x9B,      // Their block's parent
// }
//
// Because:
//   block_100a.parent_root = 0x9A
//   block_100b.parent_root = 0x9B
//   They DON'T match! This is NOT a simple same-height fork.
```

*What Happens Next:* Cannot use simple fork choice - must find common ancestor (Block 98), compare cumulative difficulty of both chains, and execute deep reorg if their chain has more work.

---

**Variant 3: `ParentNotFound`**

**Meaning:** The new block references a parent hash that doesn't exist in your database at all. This could mean:
- You're missing blocks (need to sync)
- The block is from a completely different/invalid chain
- Malicious actor sending garbage

*Example Scenario:*
```
Timeline:
  t=0:  Your node is at Block 95 (behind the network)
  t=1:  You receive Block 100 directly (skipping 96-99)
```

*Your Node's View:*
```
YOUR LOCAL CHAIN (incomplete):

    ┌──────────┐      ┌──────────┐
    │ Block 94 │─────▶│ Block 95 │  ◀── Your tip (you're behind!)
    │ hash: 0x94│      │ hash: 0x95│
    └──────────┘      └──────────┘

    You're missing blocks 96, 97, 98, 99
```

*Block Received from Network:*
```
INCOMING BLOCK:

    ┌───────────┐
    │ Block 100 │  ◀── Received from peer
    │ hash: 0xAA│
    │ parent: 0x99│  ◀── Points to Block 99... which you don't have!
    └───────────┘
```

*What You're Missing:*
```
THE FULL CHAIN (you're missing the middle):

    Block 95 ──?──▶ Block 96 ──▶ Block 97 ──▶ Block 98 ──▶ Block 99 ──▶ Block 100
    (you have)      (missing)    (missing)    (missing)    (missing)    (received)
```

*Validation Result:*
```rust
// When validating, we check if we have the parent of the new block
let parent_exists = storage.get_block_by_hash(block_100.parent_root)?;

if parent_exists.is_none() {
    // Returns: ParentHashValidation::ParentNotFound {
    //     missing_parent: 0x99
    // }
}
```

*What Happens Next:* Queue block for later, request missing blocks (96-99) from network via SyncActor, then process once complete chain is available.

---

**ParentHashValidation Summary Table**

| Variant | Your Chain | Incoming Block | Meaning | Action |
|---------|-----------|----------------|---------|--------|
| `Valid` | `...→99→100a` | `100b (parent=99)` | True same-height fork | Run fork choice tiebreaker |
| `DifferentParents` | `...→99a→100a` | `100b (parent=99b)` | Deep fork (diverged earlier) | Find common ancestor, deep reorg |
| `ParentNotFound` | `...→95` | `100 (parent=99)` | Missing blocks | Sync first, then process |

---

**3.3.2 Complete Validation Function**

```rust
// Location: app/src/actors_v2/chain/fork_choice.rs

/// Validate that two same-height blocks share the same parent.
///
/// # Arguments
/// * `current_block` - Block currently in our canonical chain
/// * `new_block` - Block received from the network
/// * `storage` - Database for looking up parent blocks
///
/// # Returns
/// * `Valid` - Blocks share parent, can proceed with fork choice
/// * `DifferentParents` - Chains diverged earlier, need deep analysis
/// * `ParentNotFound` - Missing the new block's parent, need to sync
pub async fn validate_parent_hashes(
    current_block: &SignedConsensusBlock<MainnetEthSpec>,
    new_block: &SignedConsensusBlock<MainnetEthSpec>,
    storage: &Database,
) -> ParentHashValidation {
    let current_parent = current_block.message.parent_root;
    let new_parent = new_block.message.parent_root;

    // Fast path: parents match
    if current_parent == new_parent {
        tracing::trace!(
            parent = %current_parent,
            height = current_block.message.execution_payload.block_number,
            "Parent validation passed - valid same-height fork"
        );
        return ParentHashValidation::Valid;
    }

    // Parents don't match - check if we have the new block's parent
    match storage.get_block_by_hash(new_parent) {
        Ok(Some(_)) => {
            // We have both parents, they're just different
            // This means chains diverged before this height
            tracing::warn!(
                current_parent = %current_parent,
                new_parent = %new_parent,
                height = current_block.message.execution_payload.block_number,
                "Same-height blocks have different parents - deep fork detected"
            );
            ParentHashValidation::DifferentParents {
                current_parent,
                new_parent,
            }
        }
        Ok(None) => {
            // We don't have the new block's parent at all
            tracing::info!(
                missing_parent = %new_parent,
                our_parent = %current_parent,
                "New block references unknown parent - may need sync"
            );
            ParentHashValidation::ParentNotFound {
                missing_parent: new_parent,
            }
        }
        Err(e) => {
            // Database error - treat as parent not found
            tracing::error!(error = %e, "Database error during parent lookup");
            ParentHashValidation::ParentNotFound {
                missing_parent: new_parent,
            }
        }
    }
}
```

**3.3.3 Fork Choice Integration**

```rust
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

---

##### 3.5 RequiresDeepAnalysis: Complete End-to-End Walkthrough

When `ForkChoice::RequiresDeepAnalysis` is returned, simple same-height fork choice **cannot** be used because:
1. The blocks have **different parents** (chains diverged earlier)
2. The incoming block's parent is **not in our database** (we're missing blocks)

This signals: *"We can't just compare two blocks - we need to analyze entire chain segments."*

The following walkthrough demonstrates the complete flow from your node's perspective during a network partition heal scenario.

---

**Step 1: Initial State - Network Partition Occurs**

```
NETWORK STATE BEFORE PARTITION:

All nodes agree on this chain:
    Block 95 → Block 96 → Block 97
                              ↑
                         All nodes here

TIME: t=0 - Network splits into two partitions

┌─────────────────────────────────┐    ┌─────────────────────────────────┐
│      PARTITION A (Your Node)    │    │      PARTITION B (Other Nodes)   │
│                                 │    │                                 │
│  Validators: Alice, Bob         │    │  Validators: Carol, Dave        │
│  Your node is here              │    │  Cannot communicate with you    │
└─────────────────────────────────┘    └─────────────────────────────────┘
```

---

**Step 2: Both Partitions Produce Blocks Independently**

```
TIME: t=1 to t=4 - Each partition extends its own chain

YOUR NODE'S VIEW (Partition A):

    Block 97 → Block 98a → Block 99a → Block 100a → Block 101a
               (Alice)     (Bob)       (Alice)      (Bob)
                                                      ↑
                                              Your canonical tip

PARTITION B's VIEW (unknown to you):

    Block 97 → Block 98b → Block 99b → Block 100b → Block 101b → Block 102b
               (Carol)     (Dave)      (Carol)      (Dave)       (Carol)
                                                                    ↑
                                                            Their canonical tip
```

---

**Step 3: Partition Heals - You Receive a Block**

```
TIME: t=5 - Network partition heals, you receive Block 102b from a peer

┌──────────────────────────────────────────────────────────────────────────┐
│                         YOUR NODE RECEIVES MESSAGE                        │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   NetworkActor receives gossip:                                          │
│                                                                           │
│   GossipMessage::NewBlock {                                              │
│       block: Block 102b,                                                 │
│       sender: peer_id_from_partition_b,                                  │
│   }                                                                       │
│                                                                           │
│   Block 102b details:                                                    │
│   - height: 102                                                          │
│   - hash: 0x102B                                                         │
│   - parent_hash: 0x101B  ← Points to Block 101b                         │
│   - cumulative_difficulty: 6,500,000                                     │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 4: ChainActor Receives Block for Processing**

```rust
// NetworkActor forwards to ChainActor
chain_actor.send(ChainMessage::ImportBlock {
    block: block_102b,
    source: BlockSource::Gossip,
    correlation_id: uuid!("..."),
}).await;
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      CHAINACTOR: handle_network_block()                   │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   Received block:                                                        │
│   - Block 102b (height 102, parent 0x101B)                               │
│                                                                           │
│   Your current state:                                                    │
│   - Canonical tip: Block 101a (height 101)                               │
│   - Cumulative difficulty: 5,000,000                                     │
│                                                                           │
│   First observation: Block is AHEAD of us (102 > 101)                    │
│   This is NOT a same-height fork!                                        │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 5: Check If Parent Exists**

```rust
// ChainActor checks if we have the parent of this block
let parent_hash = block_102b.message.parent_root; // 0x101B

let parent_exists = self.storage_actor
    .send(StorageMessage::GetBlockByHash { hash: parent_hash })
    .await??;
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      STORAGE LOOKUP: Parent Block                         │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   Query: GetBlockByHash(0x101B)                                          │
│   Result: None  ← We don't have Block 101b!                              │
│                                                                           │
│   Your database contains:                                                │
│   - Block 97  (hash: 0x97)                                               │
│   - Block 98a (hash: 0x98A, parent: 0x97)                                │
│   - Block 99a (hash: 0x99A, parent: 0x98A)                               │
│   - Block 100a (hash: 0x100A, parent: 0x99A)                             │
│   - Block 101a (hash: 0x101A, parent: 0x100A)                            │
│                                                                           │
│   Block 101b (hash: 0x101B) is NOT in your database!                     │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 6: Fork Choice Returns `RequiresDeepAnalysis`**

```rust
// Since we can't find the parent, this triggers deep analysis
let fork_choice_result = ForkChoice::RequiresDeepAnalysis;

tracing::info!(
    received_block = %block_102b.hash(),
    received_height = 102,
    missing_parent = %"0x101B",
    our_tip_height = 101,
    "Block references unknown parent - initiating deep analysis"
);
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      DECISION: RequiresDeepAnalysis                       │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   Why deep analysis?                                                     │
│                                                                           │
│   1. Block 102b's parent (0x101B) is not in our database                 │
│   2. This means we're missing part of their chain                        │
│   3. We need to:                                                         │
│      a) Fetch the missing blocks                                         │
│      b) Find where our chains diverged (common ancestor)                 │
│      c) Compare total work of both chains                                │
│      d) Decide whether to reorg                                          │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 7: Request Missing Blocks from Network**

```rust
// ChainActor asks SyncActor to fetch missing chain segment
self.sync_actor.send(SyncMessage::RequestAncestors {
    tip_block: block_102b.clone(),
    tip_hash: block_102b.hash(),
    from_peer: peer_id,
    correlation_id,
}).await?;

// Also queue the received block for later processing
self.pending_blocks.insert(block_102b.hash(), block_102b);
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      SYNCACTOR: Fetch Missing Chain                       │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   SyncActor sends request to peer:                                       │
│                                                                           │
│   NetworkMessage::GetBlocksByRange {                                     │
│       start_hash: 0x102B,      // Start from their tip                   │
│       direction: Backwards,    // Walk backwards to find common ancestor │
│       max_blocks: 100,                                                   │
│   }                                                                       │
│                                                                           │
│   Peer responds with blocks (newest to oldest):                          │
│   - Block 102b (already have)                                            │
│   - Block 101b (NEW)                                                     │
│   - Block 100b (NEW)                                                     │
│   - Block 99b (NEW)                                                      │
│   - Block 98b (NEW)                                                      │
│   - Block 97 (FOUND! This is our common ancestor)                        │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 8: Find Common Ancestor**

```rust
async fn find_common_ancestor(
    our_chain: &[BlockRef],
    their_chain: &[Block],
    storage: &Database,
) -> Result<u64, ChainError> {
    // Walk backwards through their chain until we find a block we have
    for block in their_chain.iter().rev() {
        let hash = block.hash();
        if let Some(_) = storage.get_block_by_hash(hash).await? {
            return Ok(block.height());
        }
    }
    Err(ChainError::NoCommonAncestor)
}
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      FINDING COMMON ANCESTOR                              │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   Walking backwards through their chain:                                 │
│                                                                           │
│   Check Block 102b (0x102B): Not in our DB                               │
│   Check Block 101b (0x101B): Not in our DB                               │
│   Check Block 100b (0x100B): Not in our DB                               │
│   Check Block 99b (0x99B): Not in our DB                                 │
│   Check Block 98b (0x98B): Not in our DB                                 │
│   Check Block 97 (0x97): ✓ FOUND! We have this block!                    │
│                                                                           │
│   Common ancestor: Block 97 (height 97)                                  │
│                                                                           │
│   Visual:                                                                │
│                    COMMON                                                │
│                   ANCESTOR                                               │
│                      ↓                                                   │
│   ... → Block 97 ─┬─→ Block 98a → 99a → 100a → 101a  (Our chain)        │
│                   │                                                      │
│                   └─→ Block 98b → 99b → 100b → 101b → 102b (Their chain)│
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 9: Calculate Cumulative Difficulty for Both Chains**

```rust
// Calculate difficulty for our chain (from common ancestor to our tip)
let our_difficulty = calculate_chain_difficulty(
    from_height: 97,  // common ancestor
    to_height: 101,   // our tip
    chain: OurCanonical,
);

// Calculate difficulty for their chain (from common ancestor to their tip)
let their_difficulty = calculate_chain_difficulty(
    from_height: 97,  // common ancestor
    to_height: 102,   // their tip
    chain: their_blocks,
);
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      COMPARING CHAIN DIFFICULTIES                         │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   OUR CHAIN (97 → 101a):                                                 │
│   ┌─────────┬────────────┬────────────────────┐                          │
│   │ Block   │ Has AuxPoW │ Difficulty         │                          │
│   ├─────────┼────────────┼────────────────────┤                          │
│   │ 97      │ Yes        │ 1,000,000 (shared) │                          │
│   │ 98a     │ No         │ 1 (base)           │                          │
│   │ 99a     │ Yes        │ 1,200,000          │                          │
│   │ 100a    │ No         │ 1 (base)           │                          │
│   │ 101a    │ Yes        │ 1,100,000          │                          │
│   ├─────────┼────────────┼────────────────────┤                          │
│   │ TOTAL   │            │ 3,300,003          │                          │
│   └─────────┴────────────┴────────────────────┘                          │
│                                                                           │
│   THEIR CHAIN (97 → 102b):                                               │
│   ┌─────────┬────────────┬────────────────────┐                          │
│   │ Block   │ Has AuxPoW │ Difficulty         │                          │
│   ├─────────┼────────────┼────────────────────┤                          │
│   │ 97      │ Yes        │ 1,000,000 (shared) │                          │
│   │ 98b     │ Yes        │ 1,300,000          │                          │
│   │ 99b     │ No         │ 1 (base)           │                          │
│   │ 100b    │ Yes        │ 1,400,000          │                          │
│   │ 101b    │ Yes        │ 1,500,000          │                          │
│   │ 102b    │ No         │ 1 (base)           │                          │
│   ├─────────┼────────────┼────────────────────┤                          │
│   │ TOTAL   │            │ 5,200,002          │                          │
│   └─────────┴────────────┴────────────────────┘                          │
│                                                                           │
│   COMPARISON: 5,200,002 > 3,300,003                                      │
│   DECISION: Their chain has MORE WORK → We should REORG                  │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 10: Execute Deep Reorganization - Phase 1: Rollback**

```rust
async fn execute_deep_reorg(
    &mut self,
    common_ancestor_height: u64,
    new_chain: Vec<SignedConsensusBlock>,
    correlation_id: Uuid,
) -> Result<ReorganizationResult, ChainError> {
    let our_tip_height = self.state.head.unwrap().number;

    tracing::warn!(
        common_ancestor = common_ancestor_height,
        our_tip = our_tip_height,
        their_tip = new_chain.last().unwrap().height(),
        rollback_depth = our_tip_height - common_ancestor_height,
        "Executing deep chain reorganization"
    );

    // Phase 1: Rollback our chain
    // Phase 2: Apply their chain
    // Phase 3: Update engine and state
}
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      PHASE 1: ROLLBACK OUR CHAIN                          │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   Rolling back blocks 101a → 100a → 99a → 98a (4 blocks)                 │
│                                                                           │
│   For each block (newest to oldest):                                     │
│                                                                           │
│   Step 1.1: Rollback Block 101a                                          │
│   - Mark as non-canonical in storage                                     │
│   - Notify EngineActor to revert execution state                         │
│   - Update cumulative difficulty                                         │
│                                                                           │
│   Step 1.2: Rollback Block 100a                                          │
│   - Mark as non-canonical                                                │
│   - Revert execution state                                               │
│                                                                           │
│   Step 1.3: Rollback Block 99a                                           │
│   - Mark as non-canonical                                                │
│   - Revert execution state                                               │
│                                                                           │
│   Step 1.4: Rollback Block 98a                                           │
│   - Mark as non-canonical                                                │
│   - Revert execution state                                               │
│                                                                           │
│   Chain state after rollback:                                            │
│                                                                           │
│   ... → Block 96 → Block 97  ← New temporary tip                         │
│                        ↓                                                 │
│              (orphaned) 98a → 99a → 100a → 101a                          │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 11: Execute Deep Reorganization - Phase 2: Apply New Chain**

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      PHASE 2: APPLY THEIR CHAIN                           │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   Applying blocks 98b → 99b → 100b → 101b → 102b (5 blocks)              │
│                                                                           │
│   For each block (oldest to newest):                                     │
│                                                                           │
│   Step 2.1: Apply Block 98b                                              │
│   ┌────────────────────────────────────────────────────────────────┐     │
│   │ storage.store_block(block_98b, canonical: true)                │     │
│   │ engine.send(NewPayload { block_98b.execution_payload })        │     │
│   │ → PayloadStatus::Valid ✓                                       │     │
│   │ Update cumulative_difficulty += 1,300,000                      │     │
│   └────────────────────────────────────────────────────────────────┘     │
│                                                                           │
│   Step 2.2: Apply Block 99b                                              │
│   ┌────────────────────────────────────────────────────────────────┐     │
│   │ storage.store_block(block_99b, canonical: true)                │     │
│   │ engine.send(NewPayload { block_99b.execution_payload })        │     │
│   │ → PayloadStatus::Valid ✓                                       │     │
│   │ Update cumulative_difficulty += 1                              │     │
│   └────────────────────────────────────────────────────────────────┘     │
│                                                                           │
│   Step 2.3: Apply Block 100b                                             │
│   ┌────────────────────────────────────────────────────────────────┐     │
│   │ storage.store_block(block_100b, canonical: true)               │     │
│   │ engine.send(NewPayload { block_100b.execution_payload })       │     │
│   │ → PayloadStatus::Valid ✓                                       │     │
│   │ Update cumulative_difficulty += 1,400,000                      │     │
│   └────────────────────────────────────────────────────────────────┘     │
│                                                                           │
│   Step 2.4: Apply Block 101b                                             │
│   ┌────────────────────────────────────────────────────────────────┐     │
│   │ storage.store_block(block_101b, canonical: true)               │     │
│   │ engine.send(NewPayload { block_101b.execution_payload })       │     │
│   │ → PayloadStatus::Valid ✓                                       │     │
│   │ Update cumulative_difficulty += 1,500,000                      │     │
│   └────────────────────────────────────────────────────────────────┘     │
│                                                                           │
│   Step 2.5: Apply Block 102b                                             │
│   ┌────────────────────────────────────────────────────────────────┐     │
│   │ storage.store_block(block_102b, canonical: true)               │     │
│   │ engine.send(NewPayload { block_102b.execution_payload })       │     │
│   │ → PayloadStatus::Valid ✓                                       │     │
│   │ Update cumulative_difficulty += 1                              │     │
│   └────────────────────────────────────────────────────────────────┘     │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 12: Sync Execution Layer Fork Choice**

```rust
// Tell Reth about the new canonical chain
self.engine_actor.send(EngineMessage::UpdateForkChoice {
    head_hash: block_102b.execution_hash(),      // New tip
    safe_hash: block_102b.execution_hash(),      // Safe head
    finalized_hash: block_97.execution_hash(),   // Finalized at common ancestor
    correlation_id: Some(correlation_id),
}).await??;
```

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      PHASE 3: SYNC EXECUTION LAYER                        │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   EngineActor → Reth (Engine API):                                       │
│                                                                           │
│   engine_forkchoiceUpdatedV3({                                           │
│       forkchoiceState: {                                                 │
│           headBlockHash: "0x102B_exec",      // Block 102b               │
│           safeBlockHash: "0x102B_exec",                                  │
│           finalizedBlockHash: "0x97_exec",   // Common ancestor          │
│       },                                                                 │
│       payloadAttributes: null,               // Not building new block   │
│   })                                                                     │
│                                                                           │
│   Reth response: { payloadStatus: "VALID", payloadId: null }             │
│                                                                           │
│   ✓ Execution layer now tracks same chain as consensus layer             │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Step 13: Final State After Reorg**

```
┌──────────────────────────────────────────────────────────────────────────┐
│                      YOUR NODE'S FINAL STATE                              │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│   NEW CANONICAL CHAIN:                                                   │
│                                                                           │
│   ...→ Block 97 → Block 98b → Block 99b → Block 100b → Block 101b → Block 102b
│                                                                       ↑   │
│                                                              Your new tip │
│                                                                           │
│   ORPHANED BLOCKS (still in database, marked non-canonical):             │
│                                                                           │
│              Block 98a → Block 99a → Block 100a → Block 101a             │
│                                                                           │
│   ChainState:                                                            │
│   - head: BlockRef { height: 102, hash: 0x102B }                         │
│   - cumulative_difficulty: 5,200,002                                     │
│   - last_block_time: timestamp of Block 102b                             │
│                                                                           │
│   Metrics emitted:                                                       │
│   - alys_chain_reorganizations_total: +1                                 │
│   - alys_chain_reorganization_depth: 4 (blocks rolled back)              │
│   - alys_chain_deep_reorg_total: +1                                      │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘
```

---

**Complete Handler Code**

```rust
/// Handle the RequiresDeepAnalysis fork choice result
async fn handle_deep_analysis(
    &mut self,
    received_block: SignedConsensusBlock,
    correlation_id: Uuid,
) -> Result<ChainResponse, ChainError> {

    // Step 1: Queue the received block
    self.pending_blocks.insert(received_block.hash(), received_block.clone());

    // Step 2: Request missing ancestors from network
    let missing_chain = self.sync_actor
        .send(SyncMessage::RequestAncestors {
            tip: received_block.clone(),
            correlation_id,
        })
        .await??;

    // Step 3: Find common ancestor
    let common_ancestor = find_common_ancestor(&missing_chain, &self.storage).await?;

    // Step 4: Calculate difficulties
    let our_difficulty = self.get_cumulative_difficulty_at(self.state.head.unwrap().number)?;
    let their_difficulty = calculate_chain_difficulty(&missing_chain)?;

    // Step 5: Compare and decide
    if their_difficulty <= our_difficulty {
        tracing::info!(
            our_difficulty = our_difficulty,
            their_difficulty = their_difficulty,
            "Their chain has less/equal work - keeping current chain"
        );
        return Ok(ChainResponse::BlockRejected {
            reason: "Competing chain has less cumulative work".into(),
        });
    }

    // Step 6: Execute deep reorg
    tracing::warn!(
        our_difficulty = our_difficulty,
        their_difficulty = their_difficulty,
        reorg_depth = self.state.head.unwrap().number - common_ancestor,
        "Their chain has more work - executing deep reorg"
    );

    let result = self.execute_deep_reorg(
        common_ancestor,
        missing_chain,
        correlation_id,
    ).await?;

    // Step 7: Return success
    Ok(ChainResponse::ReorganizationComplete {
        old_tip: result.old_tip,
        new_tip: result.new_tip,
        blocks_rolled_back: result.rollback_count,
        blocks_applied: result.apply_count,
    })
}
```

---

**Decision Tree Summary**

```
                    Receive Block from Network
                              │
                              ▼
                    ┌─────────────────────┐
                    │ Do we have parent?  │
                    └─────────────────────┘
                         /          \
                       Yes           No
                        │             │
                        ▼             ▼
              ┌──────────────┐   RequiresDeepAnalysis
              │ Same height  │   (fetch missing blocks)
              │ as our tip?  │         │
              └──────────────┘         │
                 /        \            │
               Yes         No          │
                │           │          │
                ▼           ▼          │
         Same parent?   Future/Past    │
            /    \        block        │
          Yes     No        │          │
           │       │        │          │
           ▼       ▼        ▼          ▼
        Simple   RequiresDeepAnalysis ◄─┘
      Fork Choice  (different parents)
           │               │
           ▼               ▼
    Tiebreaker rules   Find common ancestor
    (difficulty →      Compare chain weights
     timestamp →       Execute deep reorg
     hash)             if their chain wins
```

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

**What Validation Currently Happens in V2 ChainActor:**

The V2 ChainActor (`handlers.rs`) does have execution validation implemented:

```rust
// handlers.rs:1254-1306
// Step 3: Execution payload validation via EngineActor
if let Some(ref engine_actor) = engine_actor {
    let msg = EngineMessage::ValidatePayload {
        payload: block.message.execution_payload.clone(),
        correlation_id: Some(correlation_id),
    };
    match engine_actor.send(msg).await {
        Ok(Ok(EngineResponse::PayloadValid { is_valid: true, .. })) => { /* continue */ }
        Ok(Ok(EngineResponse::PayloadValid { is_valid: false, .. })) => {
            return Err(ChainError::InvalidBlock("Execution payload validation failed"));
        }
    }
}
```

**However, there is a critical flow problem:**

```
┌─────────────────────────────────────────────────────────────────┐
│              CURRENT V2 ImportBlock FLOW (PROBLEM)               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   Step 1.9: Fork Detection (line 1004)                          │
│        │                                                         │
│        ├─── Fork detected? ────────────────────┐                │
│        │         │                             │                │
│        │         ▼                             │                │
│        │    Fork Choice (line 1044)            │                │
│        │         │                             │                │
│        │         ├── KeepCurrent → Return      │                │
│        │         │                             │                │
│        │         └── Reorganize                │                │
│        │              │                        │                │
│        │              ▼                        │                │
│        │         Execute Reorg (line 1095)     │                │
│        │              │                        │                │
│        │              ▼                        │                │
│        │         Return (line 1187) ◄──────────┤ ⚠️ SKIPS       │
│        │                                       │    Steps 2-3!  │
│        │                                       │                │
│   No fork (line 1205) ◄────────────────────────┘                │
│        │                                                         │
│        ▼                                                         │
│   Step 2: Aura Validation (line 1237)                           │
│        │                                                         │
│        ▼                                                         │
│   Step 3: Execution Validation (line 1254) ◄── Only for no-fork │
│        │                                                         │
│        ▼                                                         │
│   Step 4+: Storage, commit, etc.                                │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**The Gap:**

| Scenario | Execution Validation? | Impact |
|----------|----------------------|--------|
| Normal import (no fork) | ✅ Yes (Step 3) | Correct |
| Fork → KeepCurrent | ❌ Skipped | Minor (block rejected) |
| Fork → Reorganize | ❌ **SKIPPED** | **CRITICAL: Unvalidated block becomes canonical!** |

When `ForkChoice::Reorganize` is returned:
1. `reorganize_to_new_tip()` executes (stores new block as canonical)
2. `EngineMessage::UpdateForkChoice` syncs execution layer
3. Code returns at line 1187 **WITHOUT** execution validation

**This means an invalid block could win fork choice and become canonical without EVM execution validation!**

**What's Still NOT Validated During Fork Choice:**
- ❌ Execution validity on reorg path (CRITICAL GAP)
- ❌ Timestamp bounds checks
- ❌ AuxPoW validity for received blocks (only validated during production)

**Assumption Made:**
The current code assumes blocks that win fork choice are valid. This assumption is dangerous in adversarial scenarios.

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
│   │ STEP 5: Execution Validation        │  ◄── CRITICAL!        │
│   │ - EngineMessage::ValidatePayload    │      Must happen      │
│   │ - Verify EVM execution is valid     │      BEFORE reorg     │
│   │ - Reject if invalid (keep current)  │                       │
│   └─────────────────────────────────────┘                       │
│         │                                                        │
│         ├─── Invalid ──▶ Reject (keep current block)            │
│         │                                                        │
│         ▼ (Valid)                                                │
│   ┌─────────────────────────────────────┐                       │
│   │ STEP 6: Execute Reorg               │                       │
│   │ - Update StorageActor               │                       │
│   │ - Update EngineActor fork choice    │                       │
│   │ - Update ChainState                 │                       │
│   │ - Emit Metrics                      │                       │
│   └─────────────────────────────────────┘                       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

##### 4.6 Required Fix: Execution Validation Before Reorg

The reorg path in `handlers.rs` must validate the winning block BEFORE executing the reorganization:

```rust
// handlers.rs - FIX for ForkChoice::Reorganize path (around line 1095)

ForkChoice::Reorganize { new_tip, rollback_to } => {
    warn!(
        correlation_id = %correlation_id,
        existing_hash = %existing_hash,
        new_hash = %block_hash,
        "Fork choice: new block wins - validating before reorganization"
    );

    // ════════════════════════════════════════════════════════════════
    // CRITICAL FIX: Validate execution payload BEFORE executing reorg
    // ════════════════════════════════════════════════════════════════
    if let Some(ref engine_actor) = engine_actor {
        let validate_msg = EngineMessage::ValidatePayload {
            payload: ExecutionPayload::Capella(block.message.execution_payload.clone()),
            correlation_id: Some(correlation_id),
        };

        match engine_actor.send(validate_msg).await {
            Ok(Ok(EngineResponse::PayloadValid { is_valid: true, validation_time })) => {
                debug!(
                    correlation_id = %correlation_id,
                    block_hash = %block_hash,
                    validation_time_ms = validation_time.as_millis(),
                    "Winning block passed execution validation - proceeding with reorg"
                );
            }
            Ok(Ok(EngineResponse::PayloadValid { is_valid: false, .. })) => {
                // Block won fork choice but failed execution validation!
                // Reject the block and keep current chain
                error!(
                    correlation_id = %correlation_id,
                    block_hash = %block_hash,
                    "CRITICAL: Block won fork choice but FAILED execution validation - rejecting"
                );
                self.metrics.blocks_rejected_invalid_execution.inc();
                return Ok(ChainResponse::BlockRejected {
                    reason: "Block failed execution validation despite winning fork choice".into(),
                });
            }
            Ok(Err(e)) => {
                error!(
                    correlation_id = %correlation_id,
                    error = ?e,
                    "Engine error during pre-reorg validation - rejecting block"
                );
                return Err(ChainError::Engine(format!("Pre-reorg validation failed: {}", e)));
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    error = ?e,
                    "Communication error with EngineActor - rejecting block"
                );
                return Err(ChainError::NetworkError(format!("Engine communication failed: {}", e)));
            }
        }
    } else {
        warn!(
            correlation_id = %correlation_id,
            "EngineActor not available - UNSAFE: proceeding with reorg without execution validation"
        );
    }

    // NOW safe to execute the reorganization
    let reorg_result = reorganize_to_new_tip(
        &block,
        block_height,
        storage_actor,
        correlation_id,
    ).await?;

    // ... rest of reorg handling ...
}
```

**Why This Fix Is Critical:**

| Without Fix | With Fix |
|-------------|----------|
| Attacker sends block with valid timestamp but invalid txs | Block validated before becoming canonical |
| Block wins fork choice (earlier timestamp) | If validation fails, current block kept |
| Invalid block becomes canonical | Only valid blocks can win reorg |
| Chain state corrupted | Chain integrity preserved |

**Metrics to Add:**
- `blocks_rejected_invalid_execution` - Blocks that won fork choice but failed execution
- `reorg_execution_validation_time` - Time spent validating winning blocks

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

### Design Decisions

#### Decision 1: AuxPoW Finality Model ✅ DECIDED

**Choice: Option B - Soft Finality with Configurable Depth**

```
SOFT FINALITY MODEL:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Configuration: auxpow_finality_depth = 6

Block 100 (AuxPoW) ──→ 101 ──→ 102 ──→ 103 ──→ 104 ──→ 105 ──→ 106
                                                               ↑
                                                          Current tip

Confirmations on Block 100: 6
Status: "SOFT FINAL" - reorg requires operator override
```

**What This Means:**
- AuxPoW blocks *gain* finality over time as more blocks build on top
- After `auxpow_finality_depth` confirmations (default: 6), block is "soft final"
- Reorgs past soft-final blocks require explicit operator override
- Emergency recovery remains possible (attack scenarios, Byzantine validators)

---

##### Why Not Option A (Hard Finality)?

**The Problem:** AuxPoW validation and transaction execution happen at different times:

```
MERGE-MINING TIMELINE:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

t=0: Validator Creates Block
┌─────────────────────────────────────────────────────────────┐
│ Block 100 assembled with transactions                        │
│ Block hash computed, commitment sent to Bitcoin miners       │
│ NO AuxPoW yet                                                │
└─────────────────────────────────────────────────────────────┘
                              │
           ╔══════════════════╧══════════════════╗
           ║     ~10 MINUTES PASS (avg)          ║
           ║     State can change during this!   ║
           ╚══════════════════╤══════════════════╝
                              │
t=10: Bitcoin Block Found
┌─────────────────────────────────────────────────────────────┐
│ AuxPoW proof now exists for Block 100                        │
│ BUT: The transactions may no longer be valid!                │
└─────────────────────────────────────────────────────────────┘
```

**Concrete Example - Double-Spend During Mining Window:**

```
STARTING STATE (Block 99):
┌──────────────────────────────────┐
│ Alice's Balance: 100 ALYS        │
└──────────────────────────────────┘

t=0: Validator A creates Block 100a
┌────────────────────────────────────────────────────────┐
│ Transaction: Alice → Bob (100 ALYS)                   │
│ Sent to Bitcoin miners for merge-mining               │
└────────────────────────────────────────────────────────┘

t=1: Validator B creates Block 100b (different fork)
┌────────────────────────────────────────────────────────┐
│ Transaction: Alice → Carol (100 ALYS)                 │
│ Block 100b wins fork choice (earlier timestamp)       │
│ Alice's 100 ALYS now belongs to Carol                 │
└────────────────────────────────────────────────────────┘

t=10: AuxPoW arrives for Block 100a
┌────────────────────────────────────────────────────────┐
│ Block 100a has strong AuxPoW proof                    │
│                                                        │
│ ⚠️ PROBLEM:                                            │
│ Block 100a's transaction (Alice → Bob) is INVALID!   │
│ Alice's 100 ALYS was already spent to Carol.          │
│                                                        │
│ WITH HARD FINALITY:                                   │
│ We must accept invalid Block 100a → Consensus failure │
│                                                        │
│ WITH SOFT FINALITY:                                   │
│ Block 100a can be rejected despite AuxPoW ✓           │
└────────────────────────────────────────────────────────┘
```

**Key Insight:** AuxPoW proves work was done, but does NOT prove transaction validity.

---

##### Option Comparison

| Criterion | Option A (Hard Finality) | Option B (Soft Finality) |
|-----------|-------------------------|-------------------------|
| Invalid transaction handling | ❌ Cannot recover | ✅ Can still reorg if needed |
| Attack recovery | ❌ No mechanism | ✅ Operator override available |
| "Most work wins" principle | ❌ Violated | ✅ Mostly preserved |
| Implementation complexity | Low | Medium |
| Bitcoin analogy | ❌ Bitcoin has no hard finality | ✅ Matches probabilistic model |

---

##### Why 6 Confirmations?

Mirrors Bitcoin's convention for probabilistic finality:
- 1 confirmation: 50% chance of natural reorg
- 3 confirmations: 12.5% chance
- 6 confirmations: 1.56% chance
- 100 confirmations: Negligible

For Alys with ~10 second blocks:
- 6 confirmations = ~1 minute of additional blocks
- Reasonable assurance while allowing quick attack response

---

##### Soft Finality Decision Flow

```
INCOMING REORG REQUEST:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1. Does reorg cross an AuxPoW block?
   │
   ├── NO → Apply normal "most work wins" rule
   │
   └── YES → Check if AuxPoW block is "soft final"
       │
       ├── Confirmations >= auxpow_finality_depth (6)?
       │   └── YES → REJECT (unless operator override)
       │             Log: "Cannot reorg past soft-finalized AuxPoW block"
       │
       └── NO (fewer than 6 confirmations)
           └── ALLOW with warning
               Log: "Reorging past recent AuxPoW block"
```

---

##### Impact on V2 System Components

| Component | Required Changes |
|-----------|-----------------|
| **StorageActor** | Track `has_auxpow`, `last_auxpow_height`, `last_auxpow_confirmations` per block |
| **ChainActor** | Check soft finality before executing reorg; add `ReorgConfig` |
| **SyncActor** | Reject sync requests for chains that violate soft finality |
| **NetworkActor** | Log warning for blocks that would violate finality |
| **Metrics** | Add `soft_finality_rejections_total`, `finality_override_requests_total` |

---

### Open Design Questions

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

**Overview:**

When a chain reorganization occurs, blocks that were previously canonical become **orphaned**. Currently, these blocks exist in the database but there's no way to distinguish them from canonical blocks or query them efficiently.

```
AFTER REORG:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Block 99 → Block 100b → Block 101b → Block 102b  ← New canonical tip
              │
              └── Block 100a → Block 101a → Block 102a  ← ORPHANED
                      ↑
                  Where are these blocks?
                  How do we know they're orphaned?
```

**Tasks:**
- [ ] Add `CANONICAL_BLOCKS` column family
- [ ] Define `BlockCanonicalStatus` struct
- [ ] Implement `StoreBlockWithTracking` message
- [ ] Implement `MarkBlockNonCanonical` / `MarkBlockCanonical` messages
- [ ] Implement `GetAllBlocksAtHeight` query
- [ ] Implement `GetReorgHistory` query
- [ ] Unit tests

---

##### 4.2.1 Current Storage Schema (The Problem)

```
CURRENT STORAGE:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

BLOCKS column family:
┌────────────────────────────────────────────────────────────────┐
│ Key: block_hash          │ Value: serialized block            │
├──────────────────────────┼─────────────────────────────────────┤
│ 0x100A (Block 100a)      │ { height: 100, txs: [...], ... }   │
│ 0x100B (Block 100b)      │ { height: 100, txs: [...], ... }   │
└──────────────────────────┴─────────────────────────────────────┘

BLOCK_HEIGHTS column family:
┌────────────────────────────────────────────────────────────────┐
│ Key: height (u64)        │ Value: block_hash (canonical only) │
├──────────────────────────┼─────────────────────────────────────┤
│ 100                      │ 0x100B  ← Only the canonical block │
└──────────────────────────┴─────────────────────────────────────┘

PROBLEM: We can't distinguish orphaned blocks from canonical blocks!
- Block 100a exists in BLOCKS but there's no record it's orphaned
- We can only get Block 100a if we know its exact hash
- No way to query "all blocks at height 100"
```

---

##### 4.2.2 Proposed Storage Schema

```
PROPOSED STORAGE:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

BLOCKS column family: (unchanged)
BLOCK_HEIGHTS column family: (unchanged - still points to canonical)

NEW: CANONICAL_BLOCKS column family:
┌────────────────────────────────────────────────────────────────┐
│ Key: height (u64)        │ Value: Vec<BlockCanonicalStatus>   │
├──────────────────────────┼─────────────────────────────────────┤
│ 100                      │ [                                   │
│                          │   { hash: 0x100B, is_canonical: true },  │
│                          │   { hash: 0x100A, is_canonical: false }, │
│                          │ ]                                   │
└──────────────────────────┴─────────────────────────────────────┘

Benefits:
✅ Can query "all blocks at height X"
✅ Can identify which blocks are orphaned
✅ Full reorg history preserved
✅ Enables forensics and debugging
```

---

##### 4.2.3 Implementation Specification

**Storage Schema:**

```rust
// Location: app/src/actors_v2/storage/database.rs

/// Column family for tracking all blocks at each height and their canonical status.
pub const CANONICAL_BLOCKS: &str = "canonical_blocks";

/// Represents a block's canonical status at a specific height.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCanonicalStatus {
    /// The block hash
    pub hash: H256,

    /// Whether this block is currently canonical
    pub is_canonical: bool,

    /// When this block was first stored (Unix timestamp)
    pub stored_at: u64,

    /// When this block's canonical status last changed
    pub status_changed_at: Option<u64>,

    /// Number of times this block's status has changed
    /// 0 = never changed, 1+ = reorged
    pub status_change_count: u32,
}
```

**New Storage Messages:**

```rust
// Location: app/src/actors_v2/storage/messages.rs

/// Mark a block as non-canonical (orphaned).
#[derive(Debug, Clone)]
pub struct MarkBlockNonCanonical {
    pub height: u64,
    pub hash: H256,
    pub correlation_id: Option<Uuid>,
}

/// Mark a block as canonical.
#[derive(Debug, Clone)]
pub struct MarkBlockCanonical {
    pub height: u64,
    pub hash: H256,
    pub correlation_id: Option<Uuid>,
}

/// Query all blocks at a specific height.
#[derive(Debug, Clone)]
pub struct GetAllBlocksAtHeight {
    pub height: u64,
}

/// Response containing all blocks at a height.
#[derive(Debug, Clone)]
pub struct AllBlocksAtHeightResponse {
    pub height: u64,
    pub blocks: Vec<BlockCanonicalStatus>,
    pub canonical_hash: Option<H256>,
}

/// Query reorg history for a specific height range.
#[derive(Debug, Clone)]
pub struct GetReorgHistory {
    pub from_height: u64,
    pub to_height: u64,
}

/// Response containing blocks that were reorged in the range.
#[derive(Debug, Clone)]
pub struct ReorgHistoryResponse {
    pub orphaned_blocks: Vec<(u64, H256)>,  // (height, hash)
    pub reorg_count: u32,
}
```

---

##### 4.2.4 Use Cases

**1. Debugging Reorg Issues:**

```rust
// Query: "What blocks exist at height 12345?"
let response = storage_actor.send(GetAllBlocksAtHeight { height: 12345 }).await?;

// Response shows:
// - Block 0xABC is canonical (became canonical after reorg)
// - Block 0xDEF is orphaned (was orphaned by reorg)
// - Timestamps show when reorg occurred
```

**2. Monitoring Reorg Frequency:**

```rust
// Alert: "Are we seeing too many reorgs?"
let history = storage_actor.send(GetReorgHistory {
    from_height: current_height - 1000,
    to_height: current_height,
}).await?;

if history.reorg_count > 10 {
    tracing::warn!("High reorg frequency detected");
}
```

**3. Recovering Orphaned Transactions:**

```rust
// A transaction was in an orphaned block - find it
let blocks = storage_actor.send(GetAllBlocksAtHeight { height }).await?;

for block_status in blocks.blocks.iter().filter(|b| !b.is_canonical) {
    let block = storage_actor.send(GetBlockByHash { hash: block_status.hash }).await?;
    if block.contains_transaction(tx_hash) {
        // Found it! Inform user to resubmit
    }
}
```

---

##### 4.2.5 Summary

| Aspect | Details |
|--------|---------|
| **Priority** | 🟢 Low - Nice to have for production |
| **Estimated Effort** | 6 hours |
| **Dependencies** | None (can be implemented independently) |
| **Risk** | Low - additive change, doesn't modify critical paths |
| **Value** | Debugging, forensics, monitoring |

---

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
| **AuxPoW finality model** | **Option B: Soft Finality** | Hard finality prevents recovery from invalid AuxPoW blocks; soft finality preserves "most work wins" while providing security | 2026-01 |
| **AuxPoW finality depth** | **6 confirmations** | Mirrors Bitcoin's probabilistic finality; ~1 minute at 10s blocks | 2026-01 |
| **Fork choice primary rule** | **Most work wins (cumulative difficulty)** | Aligns with PoW security model; timestamp only as tiebreaker | 2026-01 |
| **Max automatic reorg depth** | **100 blocks (alerts at 10)** | Handles extended outages; deeper reorgs require operator override | 2026-01 |

### Decisions Pending (Required Before 3+ Node Deployment)

*No pending decisions - all required decisions have been made.*

### Decision: AuxPoW Finality Model ✅ DECIDED

**Choice:** Soft finality with configurable depth (Option B)

```rust
pub struct ReorgConfig {
    /// Blocks after AuxPoW before considered "soft final"
    /// Reorgs past this point require operator override
    pub auxpow_finality_depth: u64,  // DECIDED: 6

    /// Maximum automatic reorg depth
    /// Deeper reorgs require operator approval
    pub max_automatic_reorg_depth: u64,  // DECIDED: 100

    /// Alert threshold for operator notification
    pub alert_reorg_depth: u64,  // DECIDED: 10
}
```

**Rationale:**
- AuxPoW blocks gain finality over time (like Bitcoin confirmations)
- 6 blocks mirrors Bitcoin's "6 confirmation" standard
- Allows flexibility for edge cases while providing security guarantees
- Operator can override in emergency (with audit trail)
- **Critical:** Hard finality cannot handle AuxPoW blocks with invalid transactions (see Decision 1 in Part 3)

**Implementation Impact:**
- StorageActor: Track AuxPoW status per block
- ChainActor: Check soft finality before executing reorg
- SyncActor: Reject chains violating soft finality
- Metrics: Add finality-related counters

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
| 2.2 | 2026-01-20 | Engineering | Consolidated reorg depth settings: max_automatic_reorg_depth=100, alert_reorg_depth=10. Added detailed Story 4.2 specification for non-canonical block tracking. All pending decisions now resolved. |
| 2.1 | 2026-01-20 | Engineering | **Key decisions made:** (1) AuxPoW soft finality with 6 confirmations, (2) "Most work wins" fork choice rule, (3) Added detailed rationale for soft finality including invalid transaction example |
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
