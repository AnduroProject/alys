# SyncActor Bootstrap Deadlock: Implementation Plan & Technical Analysis

**Status**: Critical Bug - Network Cannot Bootstrap
**Priority**: P0 - Blocks all regtest/devnet deployment
**Affected Components**: SyncActor V2, ChainActor V2, Block Production Pipeline
**Author**: Engineering Team
**Date**: 2025-11-11

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Problem Statement](#problem-statement)
3. [Technical Deep Dive](#technical-deep-dive)
4. [Solution: State-Based Bootstrap Detection](#solution-state-based-bootstrap-detection)
5. [Implementation Plan](#implementation-plan)
6. [Comprehensive Sync Workflow Review](#comprehensive-sync-workflow-review)
7. [Testing & Validation](#testing--validation)
8. [Risks & Mitigations](#risks--mitigations)

---

## Executive Summary

### The Problem
**The Alys V2 network cannot bootstrap from genesis.** When launching a new regtest network, both nodes refuse to produce blocks because they are stuck in a "discovering peers" state, creating a permanent deadlock.

### Root Cause
The SyncActor's sync detection logic is **too conservative** for genesis/bootstrap scenarios. It reports `is_syncing = true` while discovering peers, which prevents block production. However, in a genesis scenario (height 0, no peers), the node MUST produce blocks to bootstrap the network.

### Impact
- ❌ Cannot start new regtest networks
- ❌ Cannot test V2 sync functionality
- ❌ Blocks integration testing and development
- ❌ Network has never successfully bootstrapped in testing

### Solution
Implement **State-Based Bootstrap Detection** that allows block production when:
1. Current height = 0 (genesis state)
2. No peers discovered after reasonable timeout
3. Multi-factor heuristics prevent false positives

### Timeline
- **Phase 1** (Day 1-2): Core implementation + unit tests
- **Phase 2** (Day 3): Integration testing with regtest
- **Phase 3** (Day 4): Code review + documentation
- **Total**: 4 days to production-ready

---

## Problem Statement

### The Bootstrap Paradox

In a blockchain network, there's a fundamental chicken-and-egg problem at genesis:

```
┌─────────────────────────────────────────────────────────────┐
│  Bootstrap Scenario (Two-Node Regtest)                      │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  T=0s:   Node-1 starts at height=0, no peers                │
│          State: DiscoveringPeers                             │
│          is_syncing: true  ← PROBLEM!                        │
│          Block production: REJECTED (NotSynced)              │
│                                                              │
│  T=30s:  Node-2 starts at height=0, no peers                │
│          State: DiscoveringPeers                             │
│          is_syncing: true  ← PROBLEM!                        │
│          Block production: REJECTED (NotSynced)              │
│                                                              │
│  T=∞:    Both nodes wait forever                             │
│          Neither can produce blocks                          │
│          Network NEVER bootstraps                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Expected Behavior

```
┌─────────────────────────────────────────────────────────────┐
│  Correct Bootstrap Behavior                                  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  T=0s:   Node-1 starts at height=0, no peers                │
│          State: DiscoveringPeers                             │
│          Timeout: 30s remaining                              │
│                                                              │
│  T=30s:  Node-1 bootstrap timeout reached                   │
│          is_syncing: false  ← FIXED!                         │
│          Block production: ALLOWED                           │
│          → Produces block #1                                 │
│                                                              │
│  T=30s:  Node-2 starts, discovers Node-1                    │
│          Queries Node-1: height=1                            │
│          State: RequestingBlocks (sync from Node-1)          │
│          is_syncing: true (legitimately)                     │
│                                                              │
│  T=35s:  Node-2 syncs block #1                              │
│          is_syncing: false                                   │
│          Both nodes can now produce blocks                   │
│          Network successfully bootstrapped! ✓                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Technical Deep Dive

### Current Code Flow

#### 1. Initialization (app/src/app.rs)

```rust
// ChainActor initializes and sends StartSync to SyncActor
let msg = SyncMessage::StartSync {
    start_height: 0,      // Genesis height
    target_height: None,  // Unknown - must discover from peers
};
sync_actor.send(msg).await
```

#### 2. SyncActor Receives StartSync (sync_actor.rs:1182-1240)

```rust
SyncMessage::StartSync { start_height, target_height } => {
    self.current_height = start_height;  // = 0
    self.sync_state = SyncState::Starting;

    if let Some(target) = target_height {
        self.target_height = target;
    } else {
        self.target_height = 0;  // Unknown!
        tracing::info!("Target height unknown, will discover from network");
    }

    // Check if already synced
    if self.target_height > 0 && self.current_height >= self.target_height {
        self.sync_state = SyncState::Synced;
        return Ok(SyncResponse::Started);
    }

    // Since target_height = 0, we enter discovery mode
    self.sync_state = SyncState::DiscoveringPeers;  // ← KEY STATE
    Ok(SyncResponse::Started)
}
```

#### 3. Block Production Attempts (slot_worker.rs → handlers.rs:58-120)

```rust
// SlotWorker tries to produce block
ChainMessage::ProduceBlock { slot, timestamp } => {
    // Check sync status before producing blocks
    if let Some(ref sync_actor) = sync_actor {
        match sync_actor.send(SyncMessage::GetSyncStatus).await {
            Ok(Ok(SyncResponse::Status(status))) => {
                if status.is_syncing {  // ← PROBLEM: true during bootstrap!
                    info!("Skipping block production - node is syncing");
                    return Err(ChainError::NotSynced);  // ← REJECTED!
                }
            }
        }
    }

    // Never reaches here during bootstrap...
    // Block production code
}
```

#### 4. Sync Status Determination (sync_actor.rs:698-725)

```rust
fn get_sync_status(&self) -> SyncStatus {
    const SYNC_THRESHOLD: u64 = 2;

    let is_syncing = if self.target_height > 0 {
        // Case 1: Target known - compare heights
        self.current_height + SYNC_THRESHOLD < self.target_height
    } else {
        // Case 2: Target unknown - check sync state
        // ⚠️ PROBLEM: During bootstrap, we're in DiscoveringPeers
        matches!(
            self.sync_state,
            SyncState::Starting
                | SyncState::DiscoveringPeers  // ← Returns TRUE
                | SyncState::RequestingBlocks
                | SyncState::ProcessingBlocks
        )
    };

    SyncStatus {
        current_height: self.current_height,  // = 0
        target_height: self.target_height,    // = 0 (unknown)
        is_syncing,                           // = true ← DEADLOCK!
        sync_peers: self.sync_peers.clone(),  // = []
        pending_requests: self.active_requests.len(),  // = 0
    }
}
```

### Why This Logic Exists

The conservative sync detection was designed to prevent:
1. **Producing blocks while behind** - Don't create forks when catching up
2. **Wasting resources** - Don't build on stale chain tips
3. **Network pollution** - Don't broadcast invalid blocks

**This logic is CORRECT for normal operation** but fails catastrophically at genesis.

### The Fundamental Conflict

```
Production Networks (After Bootstrap):
    Rule: "Don't produce blocks while syncing"
    Reason: Prevents forks, ensures consensus
    Implementation: is_syncing = true during peer discovery
    Result: ✓ Correct behavior

Genesis Networks (Before Bootstrap):
    Rule: "Don't produce blocks while syncing"  ← SAME RULE
    Reason: Prevents forks, ensures consensus
    Implementation: is_syncing = true during peer discovery
    Result: ✗ Permanent deadlock - network never starts!
```

**The Insight**: We need **different rules for genesis vs. production**, but the current code treats them identically.

---

## Solution: State-Based Bootstrap Detection

### Design Principles

1. **Multi-Factor Detection**: Use multiple signals to identify bootstrap scenarios
2. **Conservative Defaults**: Only allow bootstrap after exhausting peer discovery
3. **Explicit Logging**: Make bootstrap activation highly visible
4. **Self-Correcting**: Automatically switch to normal sync once peers appear
5. **Minimal Changes**: Isolated fix in sync_actor.rs, no API changes

### Solution Architecture

```rust
┌─────────────────────────────────────────────────────────────┐
│  Bootstrap Detection Logic                                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Conditions Checked (ALL must be true):                     │
│                                                              │
│  1. current_height == 0        (Genesis state)              │
│  2. sync_peers.is_empty()      (No peers discovered)        │
│  3. sync_state == DiscoveringPeers  (In discovery phase)    │
│  4. discovery_elapsed > BOOTSTRAP_TIMEOUT  (Timeout reached)│
│                                                              │
│  If ALL true → Bootstrap Mode:                              │
│    - Set is_syncing = false                                 │
│    - Allow block production                                 │
│    - Log bootstrap activation                               │
│                                                              │
│  If ANY false → Normal Sync Mode:                           │
│    - Set is_syncing = true                                  │
│    - Block production rejected                              │
│    - Continue peer discovery/sync                           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Implementation Details

#### Step 1: Add Bootstrap Timing Fields

```rust
pub struct SyncActor {
    // ... existing fields ...

    /// Timestamp when current sync_state was entered
    state_entered_at: SystemTime,

    /// Total time spent in DiscoveringPeers state
    discovery_time_accumulated: Duration,
}
```

**Rationale**: We need to track how long we've been discovering peers to implement the timeout.

#### Step 2: Update State Transitions

```rust
impl SyncActor {
    fn transition_to_state(&mut self, new_state: SyncState) {
        // Accumulate discovery time before transitioning
        if self.sync_state == SyncState::DiscoveringPeers {
            let time_in_discovery = self.state_entered_at.elapsed()
                .unwrap_or(Duration::ZERO);
            self.discovery_time_accumulated += time_in_discovery;
        }

        // Transition to new state
        self.sync_state = new_state;
        self.state_entered_at = SystemTime::now();

        tracing::debug!(
            new_state = ?self.sync_state,
            discovery_time_secs = self.discovery_time_accumulated.as_secs(),
            "SyncActor state transition"
        );
    }
}
```

**Rationale**: Accurate state timing enables bootstrap detection. We accumulate discovery time across multiple attempts.

#### Step 3: Implement Bootstrap Detection

```rust
/// Bootstrap detection timeout
/// - Set to 30 seconds to match node-2 startup delay in regtest
/// - Production networks may use longer values (60-120s)
const BOOTSTRAP_DETECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Determine if we're syncing or in bootstrap mode
///
/// This is the core logic that decides whether to allow block production.
fn determine_sync_state(&self) -> bool {
    const SYNC_THRESHOLD: u64 = 2;

    // Case 1: Target height is known → Simple comparison
    if self.target_height > 0 {
        let is_behind = self.current_height + SYNC_THRESHOLD < self.target_height;

        if is_behind {
            tracing::trace!(
                current = self.current_height,
                target = self.target_height,
                gap = self.target_height - self.current_height,
                "Syncing: behind target height"
            );
        }

        return is_behind;
    }

    // Case 2: Target unknown → Bootstrap detection required
    match self.sync_state {
        SyncState::DiscoveringPeers => {
            self.check_bootstrap_mode()
        }

        SyncState::RequestingBlocks | SyncState::ProcessingBlocks => {
            // Actively syncing with known peers
            true
        }

        SyncState::Starting => {
            // Just started, give it time to discover
            true
        }

        SyncState::Synced | SyncState::Stopped => {
            // Not syncing
            false
        }

        SyncState::Error(_) => {
            // Error state - don't produce blocks
            false
        }
    }
}

/// Check if we should enter bootstrap mode
///
/// Bootstrap mode is activated when:
/// 1. We're at genesis (height 0)
/// 2. No peers have been discovered
/// 3. We've been trying to discover peers for BOOTSTRAP_TIMEOUT
///
/// This allows genesis nodes to start producing blocks when isolated.
fn check_bootstrap_mode(&self) -> bool {
    // Multi-factor bootstrap detection
    let at_genesis = self.current_height == 0;
    let no_peers_found = self.sync_peers.is_empty();
    let discovery_timeout_reached =
        self.discovery_time_accumulated >= BOOTSTRAP_DETECTION_TIMEOUT;

    if at_genesis && no_peers_found && discovery_timeout_reached {
        // Bootstrap mode activated
        tracing::warn!(
            discovery_time_secs = self.discovery_time_accumulated.as_secs(),
            timeout_secs = BOOTSTRAP_DETECTION_TIMEOUT.as_secs(),
            "🚀 BOOTSTRAP MODE ACTIVATED: No peers discovered at genesis, allowing block production"
        );

        return false;  // is_syncing = false → Allow block production
    }

    // Still discovering or have peers
    if !no_peers_found {
        tracing::debug!(
            peer_count = self.sync_peers.len(),
            "Peers discovered, continuing normal sync"
        );
    } else {
        let remaining = BOOTSTRAP_DETECTION_TIMEOUT
            .saturating_sub(self.discovery_time_accumulated);

        tracing::trace!(
            remaining_secs = remaining.as_secs(),
            "Peer discovery in progress, waiting for bootstrap timeout"
        );
    }

    true  // is_syncing = true → Block production prevented
}
```

**Rationale**:
- Multi-factor checks prevent false positives
- Explicit logging makes bootstrap activation visible
- Progressive logging (trace → debug → warn) based on severity
- Self-correcting when peers appear

#### Step 4: Update get_sync_status()

```rust
fn get_sync_status(&self) -> SyncStatus {
    let is_syncing = self.determine_sync_state();

    SyncStatus {
        current_height: self.current_height,
        target_height: self.target_height,
        is_syncing,
        sync_peers: self.sync_peers.clone(),
        pending_requests: self.active_requests.len(),
    }
}
```

**Rationale**: Delegate complex logic to dedicated method for testability.

#### Step 5: Update UpdatePeers Handler

```rust
SyncMessage::UpdatePeers { peers } => {
    let previous_count = self.sync_peers.len();
    self.sync_peers = peers;
    self.peer_selection_index = 0;

    tracing::info!(
        "Updated sync peers: {} -> {} peers",
        previous_count,
        self.sync_peers.len()
    );

    // If peers just appeared, reset discovery timer
    // This prevents bootstrap mode from activating after peers connect
    if previous_count == 0 && self.sync_peers.len() > 0 {
        tracing::info!(
            peer_count = self.sync_peers.len(),
            "First peers discovered - resetting bootstrap timer"
        );

        // Reset accumulated discovery time
        self.discovery_time_accumulated = Duration::ZERO;
        self.state_entered_at = SystemTime::now();
    }

    Ok(SyncResponse::Started)
}
```

**Rationale**: When peers appear, we should immediately exit bootstrap detection path.

---

## Implementation Plan

### Phase 1: Core Implementation (Days 1-2)

#### Task 1.1: Update SyncActor Struct
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Lines**: ~42-72 (struct definition)

```rust
pub struct SyncActor {
    // ... existing fields ...

    /// NEW: Timestamp when current sync_state was entered
    state_entered_at: SystemTime,

    /// NEW: Total time spent in DiscoveringPeers state
    /// Accumulates across multiple discovery attempts
    discovery_time_accumulated: Duration,
}
```

**Testing**:
- ✓ Struct compiles
- ✓ Default values set in `new()`
- ✓ No breaking changes to existing code

---

#### Task 1.2: Add State Transition Helper
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Location**: After `get_sync_status()` method (~line 750)

```rust
/// Transition to new sync state with timing tracking
///
/// This method ensures state timing is properly tracked for bootstrap detection.
fn transition_to_state(&mut self, new_state: SyncState) {
    // Accumulate discovery time before transitioning
    if self.sync_state == SyncState::DiscoveringPeers {
        if let Ok(elapsed) = self.state_entered_at.elapsed() {
            self.discovery_time_accumulated += elapsed;

            tracing::debug!(
                discovery_time_secs = self.discovery_time_accumulated.as_secs(),
                "Accumulated discovery time"
            );
        }
    }

    // Reset accumulated time when entering DiscoveringPeers from other states
    if new_state == SyncState::DiscoveringPeers
        && self.sync_state != SyncState::DiscoveringPeers
    {
        self.discovery_time_accumulated = Duration::ZERO;
        tracing::debug!("Reset discovery time for new discovery cycle");
    }

    // Transition to new state
    let old_state = self.sync_state.clone();
    self.sync_state = new_state;
    self.state_entered_at = SystemTime::now();

    tracing::info!(
        old_state = ?old_state,
        new_state = ?self.sync_state,
        "SyncActor state transition"
    );
}
```

**Testing**:
- ✓ Time accumulates correctly across states
- ✓ Time resets when re-entering discovery
- ✓ Logging provides visibility

---

#### Task 1.3: Implement Bootstrap Detection Logic
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Location**: After `transition_to_state()` method

```rust
/// Bootstrap detection timeout (configurable via SyncConfig in future)
const BOOTSTRAP_DETECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Determine if we're in active sync or bootstrap mode
fn determine_sync_state(&self) -> bool {
    const SYNC_THRESHOLD: u64 = 2;

    // Case 1: Target height known → Simple comparison
    if self.target_height > 0 {
        return self.current_height + SYNC_THRESHOLD < self.target_height;
    }

    // Case 2: Target unknown → State-based logic with bootstrap detection
    match self.sync_state {
        SyncState::DiscoveringPeers => self.check_bootstrap_mode(),
        SyncState::RequestingBlocks | SyncState::ProcessingBlocks => true,
        SyncState::Starting => true,
        SyncState::Synced | SyncState::Stopped => false,
        SyncState::Error(_) => false,
    }
}

/// Check if bootstrap mode should be activated
fn check_bootstrap_mode(&self) -> bool {
    let at_genesis = self.current_height == 0;
    let no_peers_found = self.sync_peers.is_empty();
    let discovery_timeout_reached =
        self.discovery_time_accumulated >= BOOTSTRAP_DETECTION_TIMEOUT;

    if at_genesis && no_peers_found && discovery_timeout_reached {
        tracing::warn!(
            discovery_time_secs = self.discovery_time_accumulated.as_secs(),
            timeout_secs = BOOTSTRAP_DETECTION_TIMEOUT.as_secs(),
            current_height = self.current_height,
            peer_count = self.sync_peers.len(),
            "🚀 BOOTSTRAP MODE ACTIVATED: Genesis node with no peers, allowing block production"
        );
        return false;  // Not syncing → allow blocks
    }

    // Log progress
    if !no_peers_found {
        tracing::debug!(
            peer_count = self.sync_peers.len(),
            "Peers discovered - using normal sync logic"
        );
    } else {
        let remaining = BOOTSTRAP_DETECTION_TIMEOUT
            .saturating_sub(self.discovery_time_accumulated);

        if remaining.as_secs() <= 10 {
            tracing::info!(
                remaining_secs = remaining.as_secs(),
                "Approaching bootstrap timeout"
            );
        } else {
            tracing::trace!(
                remaining_secs = remaining.as_secs(),
                "Peer discovery in progress"
            );
        }
    }

    true  // Still syncing
}
```

**Testing**:
- ✓ Returns false only when all conditions met
- ✓ Logs appropriate messages at each stage
- ✓ Handles edge cases (overflow, time errors)

---

#### Task 1.4: Update get_sync_status()
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Lines**: ~698-725

Replace existing implementation with:

```rust
fn get_sync_status(&self) -> SyncStatus {
    let is_syncing = self.determine_sync_state();

    SyncStatus {
        current_height: self.current_height,
        target_height: self.target_height,
        is_syncing,
        sync_peers: self.sync_peers.clone(),
        pending_requests: self.active_requests.len(),
    }
}
```

**Testing**:
- ✓ Returns correct is_syncing value in all scenarios
- ✓ No changes to SyncStatus struct
- ✓ Existing callers work unchanged

---

#### Task 1.5: Update State Transitions
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Lines**: Multiple locations

Replace all direct `self.sync_state = ...` assignments with `self.transition_to_state(...)`:

```rust
// Before:
self.sync_state = SyncState::DiscoveringPeers;

// After:
self.transition_to_state(SyncState::DiscoveringPeers);
```

**Locations to update**:
1. Line ~114: `discover_sync_peers()`
2. Line ~171: `stop_sync()`
3. Line ~199: After discovering peers
4. Line ~233: When already synced
5. Line ~599: `complete_sync()`
6. Line ~1236: StartSync handler
7. Other state transitions (search for `self.sync_state =`)

**Testing**:
- ✓ All state transitions tracked
- ✓ Discovery time accumulated correctly
- ✓ Logs show state progression

---

#### Task 1.6: Update UpdatePeers Handler
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Lines**: ~1345-1356

```rust
SyncMessage::UpdatePeers { peers } => {
    let previous_count = self.sync_peers.len();
    self.sync_peers = peers;
    self.peer_selection_index = 0;

    tracing::info!(
        previous_count = previous_count,
        new_count = self.sync_peers.len(),
        "Updated sync peers"
    );

    // Reset bootstrap timer when peers first appear
    if previous_count == 0 && self.sync_peers.len() > 0 {
        tracing::info!(
            peer_count = self.sync_peers.len(),
            "First peers discovered - resetting bootstrap detection timer"
        );

        self.discovery_time_accumulated = Duration::ZERO;
        self.state_entered_at = SystemTime::now();
    }

    Ok(SyncResponse::Started)
}
```

**Testing**:
- ✓ Peers update correctly
- ✓ Timer resets when peers appear
- ✓ No timer reset on subsequent updates

---

#### Task 1.7: Initialize New Fields in Constructor
**File**: `app/src/actors_v2/network/sync_actor.rs`
**Lines**: ~74-98 (`new()` function)

```rust
pub fn new(config: SyncConfig) -> Result<Self> {
    tracing::info!("Creating SyncActor V2");

    config.validate()
        .map_err(|e| anyhow!("Invalid sync configuration: {}", e))?;

    Ok(Self {
        config,
        sync_state: SyncState::Stopped,
        current_height: 0,
        target_height: 0,
        metrics: SyncMetrics::new(),
        block_queue: VecDeque::new(),
        active_requests: HashMap::new(),
        sync_peers: Vec::new(),
        peer_selection_index: 0,
        network_actor: None,
        chain_actor: None,
        is_running: false,
        shutdown_requested: false,

        // NEW: Initialize bootstrap detection fields
        state_entered_at: SystemTime::now(),
        discovery_time_accumulated: Duration::ZERO,
    })
}
```

**Testing**:
- ✓ Struct initialization succeeds
- ✓ Default values appropriate
- ✓ No compilation errors

---

### Phase 2: Integration Testing (Day 3)

#### Test 2.1: Unit Tests for Bootstrap Detection

**File**: `app/src/actors_v2/network/sync_actor.rs` (at end)

```rust
#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    #[test]
    fn test_bootstrap_detection_genesis_no_peers_timeout() {
        let config = SyncConfig::default();
        let mut actor = SyncActor::new(config).unwrap();

        // Setup: Genesis state, no peers
        actor.current_height = 0;
        actor.target_height = 0;
        actor.sync_peers = vec![];
        actor.transition_to_state(SyncState::DiscoveringPeers);

        // Before timeout: should be syncing
        assert_eq!(actor.check_bootstrap_mode(), true);

        // After timeout: should NOT be syncing (bootstrap mode)
        actor.discovery_time_accumulated = Duration::from_secs(31);
        assert_eq!(actor.check_bootstrap_mode(), false);
    }

    #[test]
    fn test_bootstrap_detection_not_at_genesis() {
        let config = SyncConfig::default();
        let mut actor = SyncActor::new(config).unwrap();

        // Setup: NOT at genesis, no peers, timeout reached
        actor.current_height = 10;  // Not genesis
        actor.target_height = 0;
        actor.sync_peers = vec![];
        actor.discovery_time_accumulated = Duration::from_secs(31);
        actor.transition_to_state(SyncState::DiscoveringPeers);

        // Should still be syncing (not genesis)
        assert_eq!(actor.check_bootstrap_mode(), true);
    }

    #[test]
    fn test_bootstrap_detection_has_peers() {
        let config = SyncConfig::default();
        let mut actor = SyncActor::new(config).unwrap();

        // Setup: Genesis, HAS peers, timeout reached
        actor.current_height = 0;
        actor.target_height = 0;
        actor.sync_peers = vec!["peer1".to_string()];  // Has peer
        actor.discovery_time_accumulated = Duration::from_secs(31);
        actor.transition_to_state(SyncState::DiscoveringPeers);

        // Should still be syncing (has peers to sync from)
        assert_eq!(actor.check_bootstrap_mode(), true);
    }

    #[test]
    fn test_bootstrap_detection_before_timeout() {
        let config = SyncConfig::default();
        let mut actor = SyncActor::new(config).unwrap();

        // Setup: Genesis, no peers, BEFORE timeout
        actor.current_height = 0;
        actor.target_height = 0;
        actor.sync_peers = vec![];
        actor.discovery_time_accumulated = Duration::from_secs(15);  // Half timeout
        actor.transition_to_state(SyncState::DiscoveringPeers);

        // Should still be syncing (timeout not reached)
        assert_eq!(actor.check_bootstrap_mode(), true);
    }

    #[test]
    fn test_discovery_time_accumulation() {
        let config = SyncConfig::default();
        let mut actor = SyncActor::new(config).unwrap();

        // Simulate multiple discovery attempts
        actor.transition_to_state(SyncState::DiscoveringPeers);
        std::thread::sleep(Duration::from_millis(100));

        actor.transition_to_state(SyncState::RequestingBlocks);
        let accumulated = actor.discovery_time_accumulated;
        assert!(accumulated >= Duration::from_millis(90));
        assert!(accumulated <= Duration::from_millis(200));

        // Re-enter discovery
        actor.transition_to_state(SyncState::DiscoveringPeers);
        std::thread::sleep(Duration::from_millis(100));

        actor.transition_to_state(SyncState::Synced);
        let total_accumulated = actor.discovery_time_accumulated;
        assert!(total_accumulated >= Duration::from_millis(180));
    }
}
```

**Expected Results**:
- ✓ All unit tests pass
- ✓ Edge cases covered
- ✓ Timing behavior validated

---

#### Test 2.2: Regtest Bootstrap Test

**File**: `docs/v2_alpha/local-regtest/BOOTSTRAP_TEST.md`

```markdown
# Bootstrap Test Procedure

## Objective
Verify that a two-node regtest network successfully bootstraps from genesis.

## Prerequisites
- Docker Compose configured (etc/docker-compose.v2-regtest.yml)
- Clean state (no existing data directories)

## Test Steps

### Step 1: Clean Environment
```bash
rm -rf data/node1 data/node2 logs/
docker compose -f etc/docker-compose.v2-regtest.yml down -v
```

### Step 2: Start Node 1
```bash
docker compose -f etc/docker-compose.v2-regtest.yml up node1
```

### Step 3: Monitor Node 1 Logs
Expected log sequence:
```
T+0s:   SyncActor V2 started
T+0s:   Starting blockchain synchronization
T+0s:   Sync state: DiscoveringPeers
T+1s:   Peer discovery in progress, remaining: 29s
T+10s:  Peer discovery in progress, remaining: 20s
T+20s:  Approaching bootstrap timeout, remaining: 10s
T+30s:  🚀 BOOTSTRAP MODE ACTIVATED: Genesis node with no peers, allowing block production
T+30s:  Block produced successfully, height=1
T+38s:  Block produced successfully, height=2
```

### Step 4: Verify Node 1 Block Production
```bash
# Query node 1 for chain height
curl -X POST http://localhost:3000 -d '{"jsonrpc":"2.0","method":"getBlockCount","params":[],"id":1}'

# Expected: height > 0
```

### Step 5: Start Node 2
```bash
# In new terminal
docker compose -f etc/docker-compose.v2-regtest.yml up node2
```

### Step 6: Monitor Node 2 Logs
Expected log sequence:
```
T+0s:   SyncActor V2 started
T+0s:   Starting blockchain synchronization
T+0s:   Sync state: DiscoveringPeers
T+2s:   Peer discovered: <node1-peer-id>
T+2s:   First peers discovered - resetting bootstrap detection timer
T+2s:   Querying peers for chain height
T+3s:   Received chain height from peer: 5
T+3s:   Starting block sync from height 0 to 5
T+5s:   Block successfully imported, height=1
T+5s:   Block successfully imported, height=2
...
T+8s:   ✓✓✓ Sync completed successfully, final_height=5
T+8s:   Node is synced - proceeding with block production
```

### Step 7: Verify Both Nodes Producing
```bash
# Watch logs for alternating block production
docker compose -f etc/docker-compose.v2-regtest.yml logs -f node1 node2 | grep "Block produced"

# Expected: Both nodes producing blocks
```

## Success Criteria
- ✅ Node 1 enters bootstrap mode after 30s
- ✅ Node 1 produces first block at height 1
- ✅ Node 2 discovers Node 1 as peer
- ✅ Node 2 syncs blocks 1-N from Node 1
- ✅ Node 2 completes sync and starts producing
- ✅ Both nodes continue producing blocks
- ✅ No "NotSynced" errors after bootstrap

## Failure Scenarios
If test fails, collect diagnostics:
```bash
# Node logs
docker compose -f etc/docker-compose.v2-regtest.yml logs node1 > node1-bootstrap-test.log
docker compose -f etc/docker-compose.v2-regtest.yml logs node2 > node2-bootstrap-test.log

# Search for key events
grep "BOOTSTRAP MODE" node1-bootstrap-test.log
grep "First peers discovered" node2-bootstrap-test.log
grep "NotSynced" *.log
```
```

**Expected Results**:
- ✓ Bootstrap completes in 30-40 seconds
- ✓ Both nodes producing blocks
- ✓ No deadlock observed

---

### Phase 3: Code Review & Documentation (Day 4)

#### Task 3.1: Code Review Checklist

**Reviewer Checklist**:
- [ ] Bootstrap timeout value appropriate (30s for regtest)
- [ ] All state transitions use `transition_to_state()`
- [ ] Discovery time accumulation logic correct
- [ ] Edge cases handled (time overflow, concurrent updates)
- [ ] Logging provides adequate visibility
- [ ] Unit tests cover all conditions
- [ ] Integration test procedure documented
- [ ] No performance regressions
- [ ] No security concerns (timing attacks, DoS vectors)

---

#### Task 3.2: Update Documentation

**Files to Update**:

1. **CHANGELOG.md**
```markdown
## [Unreleased]

### Fixed
- **Critical**: Fixed bootstrap deadlock preventing genesis network initialization
  - SyncActor now implements state-based bootstrap detection
  - Genesis nodes enter bootstrap mode after 30s timeout with no peers
  - Allows first block production to initialize network
  - Self-corrects when peers appear and normal sync resumes
  - Resolves regtest deployment failures
```

2. **docs/v2_alpha/actors/network/sync-actor.md**
```markdown
## Bootstrap Detection

SyncActor implements intelligent bootstrap detection to handle genesis scenarios:

### Bootstrap Mode Activation

When ALL conditions are met:
1. Current height = 0 (genesis state)
2. No peers discovered
3. In DiscoveringPeers state
4. Discovery time >= 30 seconds

The node enters **Bootstrap Mode**:
- `is_syncing` returns `false`
- Block production allowed
- Network can initialize

### Automatic Recovery

Bootstrap mode automatically deactivates when:
- Peers are discovered (normal sync begins)
- Height advances (no longer at genesis)
- Sync completes

### Configuration

Bootstrap timeout (default 30s):
- Regtest: 30s (matches node startup intervals)
- Devnet: 60s (allows for slower peer discovery)
- Testnet: 120s (conservative for public networks)
- Mainnet: 180s (maximum safety)

*Future: Configurable via SyncConfig*
```

3. **app/src/actors_v2/network/sync_actor.rs** (inline docs)
```rust
/// # Bootstrap Detection
///
/// SyncActor implements state-based bootstrap detection to handle genesis scenarios.
///
/// ## Problem
/// At genesis (height 0), nodes must produce blocks even without peers, otherwise
/// the network cannot initialize. However, during normal operation, nodes should
/// NOT produce blocks while discovering peers (they may be behind).
///
/// ## Solution
/// Multi-factor detection activates "bootstrap mode" when:
/// 1. Current height = 0 (genesis)
/// 2. No peers discovered after exhaustive search
/// 3. Discovery timeout reached (30s default)
///
/// Bootstrap mode allows block production to initialize the network.
///
/// ## Self-Correction
/// When peers appear, bootstrap detection automatically deactivates and normal
/// sync resumes. This handles network partitions and delayed peer discovery.
///
/// ## See Also
/// - SYNC_BOOTSTRAP_IMPLEMENTATION_PLAN.md for full technical analysis
/// - Bootstrap detection tests in bootstrap_tests module
```

---

## Comprehensive Sync Workflow Review

### Current Sync Workflow Analysis

Let me trace through ALL sync-related code paths to identify potential issues:

#### Workflow 1: Normal Startup with Peers Available

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: Node joins existing network                      │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. App.rs initializes actors                               │
│     ├─ StorageActor starts                                  │
│     ├─ NetworkActor starts (libp2p swarm)                   │
│     ├─ SyncActor starts                                     │
│     └─ ChainActor starts                                    │
│                                                              │
│  2. ChainActor queries storage for current height           │
│     └─ Returns height = N                                   │
│                                                              │
│  3. ChainActor sends StartSync { start_height: N, ... }     │
│                                                              │
│  4. SyncActor receives StartSync                            │
│     ├─ Sets current_height = N                              │
│     ├─ Sets target_height = 0 (unknown)                     │
│     └─ Transitions to DiscoveringPeers                      │
│                                                              │
│  5. NetworkActor discovers peers via mDNS/DHT               │
│     └─ Sends UpdatePeers to SyncActor                       │
│                                                              │
│  6. SyncActor receives UpdatePeers                          │
│     ├─ sync_peers = [peer1, peer2, ...]                     │
│     └─ Resets discovery timer (prevents bootstrap)          │
│                                                              │
│  7. SyncActor discovers target height                       │
│     ├─ Queries peers for their heights                      │
│     ├─ Takes consensus (mode/median)                        │
│     └─ Sets target_height = M (where M > N)                 │
│                                                              │
│  8. SyncActor creates block requests                        │
│     ├─ Requests blocks N+1 to M                             │
│     ├─ Sends GetBlocks to NetworkActor                      │
│     └─ Transitions to RequestingBlocks                      │
│                                                              │
│  9. NetworkActor fetches blocks from peers                  │
│     └─ Sends BlockResponse back to SyncActor                │
│                                                              │
│  10. SyncActor processes blocks                             │
│      ├─ Validates basic structure                           │
│      ├─ Forwards to ChainActor via ImportBlock              │
│      └─ Transitions to ProcessingBlocks                     │
│                                                              │
│  11. ChainActor validates and stores blocks                 │
│      ├─ Engine validation (consensus rules)                 │
│      ├─ StorageActor persistence                            │
│      └─ Returns BlockImported                               │
│                                                              │
│  12. SyncActor updates progress                             │
│      ├─ current_height increments                           │
│      └─ Requests more blocks if N < M                       │
│                                                              │
│  13. Sync completes when current_height >= target_height    │
│      ├─ Transitions to Synced                               │
│      ├─ Notifies ChainActor via SyncCompleted               │
│      └─ Block production allowed                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ✅ WORKING (assuming peer connections succeed)
Issues: None identified
```

#### Workflow 2: Genesis Bootstrap (BROKEN → FIXED)

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: First node starts at genesis                     │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  BEFORE FIX:                                                │
│  ❌ 1-13: Stuck in DiscoveringPeers forever                 │
│  ❌ Block production rejected (NotSynced)                   │
│  ❌ Network never bootstraps                                │
│                                                              │
│  AFTER FIX:                                                 │
│  ✅ 1-4: Same as Workflow 1                                 │
│  ✅ 5: NetworkActor finds no peers (as expected)            │
│  ✅ 6: Discovery time accumulates                           │
│  ✅ 7: After 30s, bootstrap mode activates                  │
│  ✅ 8: is_syncing = false → block production allowed        │
│  ✅ 9: SlotWorker produces block 1                          │
│  ✅ 10: ChainActor imports block 1                          │
│  ✅ 11: StorageActor persists block 1                       │
│  ✅ 12: current_height = 1                                  │
│  ✅ 13: Node continues producing (height > 0)               │
│  ✅ 14: When peer joins, normal sync resumes                │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ✅ FIXED by this implementation
Issues: Resolved
```

#### Workflow 3: Re-sync After Restart

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: Node restarts, behind by K blocks                │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. App.rs initializes actors                               │
│                                                              │
│  2. ChainActor queries storage for current height           │
│     └─ Returns height = N (last persisted)                  │
│                                                              │
│  3. ChainActor sends StartSync { start_height: N, ... }     │
│                                                              │
│  4. SyncActor receives StartSync                            │
│     ├─ current_height = N                                   │
│     ├─ target_height = 0 (will discover)                    │
│     └─ State: DiscoveringPeers                              │
│                                                              │
│  5. NetworkActor discovers peers (existing network)         │
│     └─ UpdatePeers → sync_peers populated quickly           │
│                                                              │
│  6. SyncActor discovers target = N+K                        │
│                                                              │
│  7. Normal sync workflow resumes                            │
│     └─ Syncs blocks N+1 to N+K                              │
│                                                              │
│  8. Sync completes, block production resumes                │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ✅ WORKING
Issues: None (peers discovered quickly, bootstrap not triggered)
```

#### Workflow 4: Network Partition Recovery

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: Node isolated, then reconnects                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Phase 1: Isolation                                         │
│  1. Node at height N loses all peer connections             │
│  2. UpdatePeers { peers: [] } received                      │
│  3. State: DiscoveringPeers                                 │
│  4. Discovery timer starts accumulating                     │
│  5. After 30s, bootstrap mode activates                     │
│  6. ⚠️ ISSUE: Node at height N > 0 produces blocks          │
│     └─ Creates FORK (mining on isolated chain)              │
│                                                              │
│  Phase 2: Reconnection                                      │
│  7. Peers reconnect                                         │
│  8. UpdatePeers { peers: [peer1, ...] } received            │
│  9. Discovery timer resets                                  │
│  10. SyncActor queries peer heights                         │
│  11. Discovers peer height = N+K (ahead)                    │
│  12. Transitions to RequestingBlocks                        │
│  13. ❌ LOCAL FORK NOT HANDLED                              │
│      └─ Node has blocks N+1..N+M (forked)                   │
│      └─ Needs to revert before syncing canonical chain      │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ⚠️ POTENTIAL ISSUE - Fork handling not verified
Recommendation: Review ChainActor reorg logic
```

**ISSUE IDENTIFIED**: Bootstrap detection only checks `current_height == 0`. A partitioned node at height N > 0 could also enter bootstrap mode and create a fork.

**Fix Required**:
```rust
fn check_bootstrap_mode(&self) -> bool {
    let at_genesis = self.current_height == 0;  // ✅ Keep this check
    let no_peers_found = self.sync_peers.is_empty();
    let discovery_timeout_reached =
        self.discovery_time_accumulated >= BOOTSTRAP_DETECTION_TIMEOUT;

    // ✅ ADDITIONAL CHECK: Only allow bootstrap at genesis
    // Non-genesis nodes should never enter bootstrap mode (wait for peers)
    if !at_genesis {
        tracing::warn!(
            current_height = self.current_height,
            "Non-genesis node in discovery mode - will not enter bootstrap"
        );
        return true;  // Keep syncing, don't produce blocks
    }

    if at_genesis && no_peers_found && discovery_timeout_reached {
        // ... existing bootstrap activation ...
    }

    true
}
```

This ensures ONLY genesis nodes can bootstrap. Non-genesis isolated nodes wait indefinitely for peers (correct behavior - prevents forks).

---

#### Workflow 5: Peer Discovery Failure (Non-Genesis)

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: Node at height N > 0, no peers discovered        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Node starts at height N (N > 0)                         │
│  2. SyncActor transitions to DiscoveringPeers               │
│  3. NetworkActor attempts peer discovery                    │
│  4. No peers found (network down, config error, etc.)       │
│  5. Discovery timer accumulates                             │
│  6. After 30s, bootstrap check runs                         │
│  7. ✅ at_genesis = false → bootstrap NOT activated         │
│  8. Node remains in DiscoveringPeers                        │
│  9. Block production remains blocked (NotSynced)            │
│  10. Operator intervention required                         │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ✅ CORRECT BEHAVIOR (with genesis-only check)
Rationale: Non-genesis node without peers is useless anyway
```

---

#### Workflow 6: Height Discovery Failure

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: Peers found but height query fails               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. SyncActor receives UpdatePeers { peers: [peer1, ...] }  │
│  2. sync_peers populated                                    │
│  3. Discovery timer resets                                  │
│  4. Attempts to discover target height                      │
│  5. Queries peer1 for status → TIMEOUT                      │
│  6. Queries peer2 for status → TIMEOUT                      │
│  7. Queries peer3 for status → TIMEOUT                      │
│  8. discover_target_height() returns Error                  │
│  9. ❌ State: Error("No peers responded with height")       │
│  10. Sync stops, block production blocked                   │
│                                                              │
│  ISSUE: No retry mechanism for height discovery             │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ⚠️ POTENTIAL ISSUE - No height discovery retry
Recommendation: Add periodic retry in started() hook
```

**Fix Required** (Optional Enhancement):
```rust
fn started(&mut self, ctx: &mut Self::Context) {
    tracing::info!("SyncActor V2 started");
    self.is_running = true;

    // Existing periodic update
    ctx.run_interval(Duration::from_secs(5), |act, _ctx| {
        if act.sync_state == SyncState::RequestingBlocks
            || act.sync_state == SyncState::ProcessingBlocks
        {
            // ... existing sync progress updates ...
        }
    });

    // ✅ NEW: Periodic height discovery retry for Error state
    ctx.run_interval(Duration::from_secs(30), |act, ctx| {
        if matches!(act.sync_state, SyncState::Error(_))
            && !act.sync_peers.is_empty()
        {
            tracing::warn!("Retrying sync after error state");

            // Retry by transitioning back to discovery
            act.transition_to_state(SyncState::DiscoveringPeers);

            // Trigger discovery async
            let addr = ctx.address();
            tokio::spawn(async move {
                // Trigger rediscovery
            });
        }
    });
}
```

---

#### Workflow 7: Block Import Failure During Sync

```
┌─────────────────────────────────────────────────────────────┐
│  Sequence: Received block fails validation                  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. SyncActor receives BlockResponse from peer              │
│  2. Queues blocks for processing                            │
│  3. Sends ImportBlock to ChainActor                         │
│  4. ChainActor validates block                              │
│  5. ❌ Validation fails (bad signature, wrong parent, etc.) │
│  6. ChainActor returns BlockRejected { reason }             │
│  7. SyncActor receives rejection                            │
│  8. ✅ Logs warning, continues with next block              │
│  9. ⚠️ ISSUE: current_height NOT updated                    │
│  10. ⚠️ ISSUE: No peer reputation penalty                   │
│  11. ⚠️ ISSUE: May request same bad block again             │
│                                                              │
└─────────────────────────────────────────────────────────────┘

Status: ⚠️ NEEDS IMPROVEMENT - Error handling incomplete
Recommendation: Implement peer scoring and block retry logic
```

**Current Code** (sync_actor.rs:506-515):
```rust
ChainResponse::BlockRejected { reason } => {
    self.metrics.record_block_rejected(&reason);
    tracing::warn!(
        block_height = block_height,
        reason = %reason,
        peer_id = %peer_id,
        "Block rejected by ChainActor during sync"
    );
    Err(anyhow!("Block rejected: {}", reason))  // ← Just errors out
}
```

**Recommended Enhancement**:
```rust
ChainResponse::BlockRejected { reason } => {
    self.metrics.record_block_rejected(&reason);

    // Penalize peer for bad block
    if let Some(ref network_actor) = self.network_actor {
        let penalty_msg = NetworkMessage::UpdatePeerReputation {
            peer_id: peer_id.clone(),
            delta: -10.0,  // Significant penalty
            reason: format!("Invalid block: {}", reason),
        };

        if let Err(e) = network_actor.try_send(penalty_msg) {
            tracing::error!("Failed to penalize peer: {}", e);
        }
    }

    // Classify rejection reason
    let is_permanent = reason.contains("signature")
        || reason.contains("parent hash")
        || reason.contains("consensus");

    if is_permanent {
        tracing::error!(
            block_height = block_height,
            reason = %reason,
            peer_id = %peer_id,
            "Permanent validation failure - peer sent invalid block"
        );

        // TODO: Ban peer or switch to different peer for this range
    } else {
        tracing::warn!(
            block_height = block_height,
            reason = %reason,
            "Temporary validation failure - will retry"
        );

        // TODO: Retry with backoff
    }

    Err(anyhow!("Block rejected: {}", reason))
}
```

**Status**: Enhancement recommended but not blocking (can be added later)

---

### Sync Workflow Issues Summary

| Workflow | Status | Issues Identified | Priority |
|----------|--------|-------------------|----------|
| Normal Startup | ✅ Working | None | N/A |
| Genesis Bootstrap | ❌ **BROKEN** | **Deadlock** | **P0 - This PR** |
| Re-sync After Restart | ✅ Working | None | N/A |
| Network Partition | ⚠️ Potential Issue | Fork creation if height > 0 | **P1 - Add to PR** |
| Peer Discovery Failure | ✅ Correct | None (with genesis check) | N/A |
| Height Discovery Failure | ⚠️ Needs Improvement | No retry mechanism | P2 - Future work |
| Block Import Failure | ⚠️ Needs Improvement | No peer scoring | P2 - Future work |

### Critical Issue: Network Partition at Height > 0

**MUST FIX in this PR** to prevent forks:

```rust
fn check_bootstrap_mode(&self) -> bool {
    // ✅ CRITICAL: Only genesis nodes can enter bootstrap mode
    if self.current_height > 0 {
        tracing::debug!(
            current_height = self.current_height,
            peer_count = self.sync_peers.len(),
            "Non-genesis node waiting for peers - bootstrap mode not applicable"
        );
        return true;  // Keep is_syncing = true
    }

    // Original bootstrap logic (only reachable if height == 0)
    let at_genesis = self.current_height == 0;
    let no_peers_found = self.sync_peers.is_empty();
    let discovery_timeout_reached =
        self.discovery_time_accumulated >= BOOTSTRAP_DETECTION_TIMEOUT;

    if at_genesis && no_peers_found && discovery_timeout_reached {
        // ... bootstrap activation ...
    }

    true
}
```

---

## Testing & Validation

### Unit Tests (app/src/actors_v2/network/sync_actor.rs)

See Phase 2, Task 2.1 for complete unit test suite.

**Coverage**:
- ✅ Bootstrap at genesis with timeout
- ✅ Non-genesis does not bootstrap
- ✅ Peers present does not bootstrap
- ✅ Timeout not reached does not bootstrap
- ✅ Discovery time accumulation
- ✅ Timer reset on peer discovery

---

### Integration Tests (Manual - Regtest)

See Phase 2, Task 2.2 for complete test procedure.

**Test Scenarios**:
1. ✅ Genesis bootstrap (both nodes cold start)
2. ✅ Join existing network (node 2 syncs from node 1)
3. ✅ Restart and re-sync (node 2 restarts after blocks produced)
4. ⚠️ Network partition (TODO: Add test case)

---

### Performance Testing

**Metrics to Monitor**:
- Bootstrap latency (target: 30-40s for regtest)
- Sync throughput (blocks/sec during catch-up)
- Memory usage (block queue size)
- CPU usage (validation overhead)

**Acceptance Criteria**:
- Bootstrap completes < 45s in regtest
- Sync throughput >= 10 blocks/sec
- Memory usage < 500MB for 1000 block sync
- No memory leaks over 24h run

---

## Risks & Mitigations

### Risk 1: False Bootstrap Activation

**Risk**: Node activates bootstrap mode incorrectly, creates fork

**Likelihood**: Low
**Impact**: High (consensus failure)

**Mitigation**:
- ✅ Multi-factor detection (height AND peers AND timeout)
- ✅ Genesis-only check (height == 0)
- ✅ Explicit logging (highly visible)
- ✅ Comprehensive unit tests

**Residual Risk**: Very Low

---

### Risk 2: Bootstrap Timeout Too Short

**Risk**: Peers exist but not discovered within 30s, unnecessary bootstrap

**Likelihood**: Low (regtest), Medium (prod)
**Impact**: Medium (temporary fork, self-corrects)

**Mitigation**:
- ✅ 30s appropriate for regtest (node 2 starts at T+30s)
- ✅ Auto-correction when peers appear
- ⚠️ Future: Make timeout configurable per network

**Residual Risk**: Low (self-correcting)

---

### Risk 3: Network Partition Fork (height > 0)

**Risk**: Isolated node at height N > 0 enters bootstrap, creates fork

**Likelihood**: Medium (network issues)
**Impact**: High (requires reorg)

**Mitigation**:
- ✅ **Added in this PR**: height > 0 check prevents bootstrap
- ✅ Non-genesis nodes wait indefinitely (correct behavior)
- ⚠️ Future: Add operator alerts for extended isolation

**Residual Risk**: Very Low (prevented by height check)

---

### Risk 4: Time Accumulation Bugs

**Risk**: Discovery time tracked incorrectly, premature/delayed bootstrap

**Likelihood**: Low
**Impact**: Medium (wrong timing)

**Mitigation**:
- ✅ Dedicated transition_to_state() method
- ✅ Time accumulation tested in unit tests
- ✅ Defensive programming (unwrap_or for time errors)

**Residual Risk**: Very Low

---

### Risk 5: Peer Discovery Race Condition

**Risk**: Peers discovered just as bootstrap activates

**Likelihood**: Low
**Impact**: Low (produces 1 block, then syncs normally)

**Mitigation**:
- ✅ UpdatePeers resets timer immediately
- ✅ Self-correcting (sync starts after bootstrap)
- ✅ No consensus impact (valid block produced)

**Residual Risk**: Very Low (benign outcome)

---

## Future Enhancements

### Enhancement 1: Configurable Bootstrap Timeout

```rust
pub struct SyncConfig {
    /// Bootstrap detection timeout (network-specific)
    pub bootstrap_timeout: Duration,
}

impl SyncConfig {
    pub fn for_regtest() -> Self {
        Self {
            bootstrap_timeout: Duration::from_secs(30),
            // ...
        }
    }

    pub fn for_mainnet() -> Self {
        Self {
            bootstrap_timeout: Duration::from_secs(180),  // 3 minutes
            // ...
        }
    }
}
```

**Priority**: P2 (nice to have)

---

### Enhancement 2: Peer Reputation & Scoring

Implement comprehensive peer scoring as identified in Workflow 7 review.

**Priority**: P2 (improves reliability)

---

### Enhancement 3: Height Discovery Retry

Add periodic retry for Error state as identified in Workflow 6 review.

**Priority**: P3 (edge case improvement)

---

### Enhancement 4: Operator Alerts

Add prometheus metrics for:
- Time in bootstrap mode
- Bootstrap activations count
- Time in DiscoveringPeers state
- Failed height discoveries

**Priority**: P3 (operational visibility)

---

## Conclusion

This implementation plan provides:

1. ✅ **Complete solution** to bootstrap deadlock
2. ✅ **Educational context** for team understanding
3. ✅ **Comprehensive workflow review** identifying 2 additional issues
4. ✅ **Detailed implementation steps** with code examples
5. ✅ **Testing strategy** with acceptance criteria
6. ✅ **Risk analysis** with mitigations
7. ✅ **Future roadmap** for enhancements

**Estimated Effort**: 4 days (including testing)
**Risk Level**: Low (isolated change, extensive testing)
**Impact**: Critical (unblocks all V2 development)

---

## Appendix: Key Files Modified

1. `app/src/actors_v2/network/sync_actor.rs` - Core implementation
2. `app/src/actors_v2/network/sync_actor.rs` - Unit tests
3. `docs/v2_alpha/actors/network/sync-actor.md` - Documentation
4. `docs/v2_alpha/local-regtest/BOOTSTRAP_TEST.md` - Test procedure
5. `CHANGELOG.md` - Release notes

**Total Lines Changed**: ~200-250 lines (mostly new code)
