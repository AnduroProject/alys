# Tendermint Chaos Testing Guide

A comprehensive guide to the Tendermint BFT consensus chaos testing framework for the Alys V2 blockchain.

**Version:** 1.0
**Last Updated:** March 2026
**Target Audience:** Developers running, debugging, and extending chaos tests

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Architecture](#2-architecture)
3. [Quick Start](#3-quick-start)
4. [Scenario Reference](#4-scenario-reference)
5. [Extending the Framework](#5-extending-the-framework)
6. [Troubleshooting](#6-troubleshooting)
7. [Operational Runbooks](#7-operational-runbooks)
8. [Appendices](#8-appendices)

---

# 1. Introduction

## 1.1 Purpose and Scope

This chaos testing framework validates the correctness, safety, and liveness of the Tendermint BFT consensus implementation in Alys V2. It replaces the legacy Aura-based chaos testing infrastructure and is specifically designed for BFT consensus properties.

**What This Framework Tests:**

1. **Consensus Operations:** Locking rules, Proof-of-Lock (POL), vote thresholds, round advancement, proposer rotation, and commit finality
2. **Safety Guarantees:** No equivocation, no forks after commit, correct WAL recovery
3. **Liveness Properties:** Timeout handling, round progression, partition recovery
4. **System Resilience:** Validator failures, network partitions, external dependency failures

**Out of Scope:**

- Application-level logic testing (covered by integration tests)
- Performance benchmarking (separate performance test suite)
- Security penetration testing (separate security audit process)

## 1.2 Tendermint Consensus Overview (Testing Perspective)

Tendermint BFT is a Byzantine Fault Tolerant consensus algorithm requiring >2/3 honest validators. Understanding these mechanics is essential for interpreting test results.

### Consensus State Machine

```
┌─────────────────────────────────────────────────────────────────┐
│                         Height H                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                      Round R                             │    │
│  │                                                          │    │
│  │  ┌─────────┐    ┌──────────┐    ┌────────────┐    ┌────┐│    │
│  │  │ Propose │───>│ Prevote  │───>│ Precommit  │───>│Commit│    │
│  │  └─────────┘    └──────────┘    └────────────┘    └────┘│    │
│  │       │              │               │                   │    │
│  │       │ timeout      │ timeout       │ timeout          │    │
│  │       ▼              ▼               ▼                   │    │
│  │  ┌─────────────────────────────────────────────────────┐│    │
│  │  │              Round R+1 (new proposer)               ││    │
│  │  └─────────────────────────────────────────────────────┘│    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### Key Properties Being Tested

| Property | Description | Test Category |
|----------|-------------|---------------|
| **Locking** | Validators lock on a block after seeing >2/3 prevotes | TM-T1, TM-V* |
| **POL Unlocking** | Validators can unlock with Proof-of-Lock from higher round | TM-T2 |
| **Vote Thresholds** | Strictly >2/3 required (not >=2/3) | TM-V1-V4 |
| **Proposer Rotation** | `(height + round) % num_validators` | TM-T7 |
| **Instant Finality** | No forks possible after commit | TM-B*, TM-L* |
| **Equivocation Detection** | Conflicting votes generate evidence | TM-E* |
| **WAL Recovery** | No equivocation after crash/restart | TM-D*, TM-E5 |

### BFT Quorum (n=3)

With 3 validators:
- **Quorum:** `floor(2×3/3) + 1 = 3` (100% required)
- **Fault Tolerance:** 0 (cannot tolerate any failures)
- **Implication:** Single validator failure **halts** consensus (expected behavior)

Tests verify:
- Correct halt when quorum is lost
- Proper recovery when quorum is restored
- No safety violations during or after chaos

## 1.3 BFT Properties Being Validated

### Safety Properties (Must Never Violate)

1. **Agreement:** All honest validators commit the same block at each height
2. **Validity:** Only proposed blocks can be committed
3. **No Equivocation:** Validators cannot vote for conflicting blocks

### Liveness Properties (Must Eventually Satisfy)

1. **Termination:** Consensus eventually produces blocks
2. **Progress:** Height continues to advance
3. **Recovery:** System recovers from transient failures

## 1.4 Relationship to Production Monitoring

The metrics and RPC endpoints used in chaos testing are the same as production monitoring:

| Component | Chaos Testing Use | Production Use |
|-----------|-------------------|----------------|
| `tendermint_consensusState` | Verify height/round | Dashboard monitoring |
| `tendermint_evidence` | Check for equivocation | Alert on evidence |
| Prometheus metrics | Test verification | Performance monitoring |
| Container logs | Debug failures | Incident investigation |

---

# 2. Architecture

## 2.1 Test Infrastructure Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    Chaos Testing Framework                       │
│                                                                  │
│  ┌──────────────────────────┐   ┌──────────────────────────┐   │
│  │  Layer 1: Docker/Bash    │   │  Layer 2: Rust Unit      │   │
│  │                          │   │                          │   │
│  │  - tendermint-chaos.sh   │   │  - tendermint_chaos.rs   │   │
│  │  - wait-for-consensus.sh │   │  - chain_chaos_tests.rs  │   │
│  │  - Docker commands       │   │  - driver_chaos_tests.rs │   │
│  │                          │   │                          │   │
│  │  Tests:                  │   │  Tests:                  │   │
│  │  - Real network I/O      │   │  - Mock validators       │   │
│  │  - Container lifecycle   │   │  - State inspection      │   │
│  │  - External services     │   │  - Precise vote control  │   │
│  └──────────────────────────┘   └──────────────────────────┘   │
│              │                              │                    │
│              ▼                              ▼                    │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              3-Node Tendermint Testnet                    │   │
│  │  ┌────────┐  ┌────────┐  ┌────────┐  ┌──────────┐        │   │
│  │  │Node 1  │  │Node 2  │  │Node 3  │  │ Execution│        │   │
│  │  │:3001   │  │:3011   │  │:3021   │  │  Layer   │        │   │
│  │  └────────┘  └────────┘  └────────┘  └──────────┘        │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## 2.2 Layer 1 (Docker/Bash) vs Layer 2 (Rust)

### When to Use Each Layer

| Test Type | Layer 1 (Docker) | Layer 2 (Rust) |
|-----------|------------------|----------------|
| Network partitions | ✅ Real isolation | ❌ Not possible |
| Node crashes | ✅ Real process termination | ❌ Not applicable |
| WAL recovery | ✅ Real file system | ⚠️ Mock WAL |
| Locking/POL logic | ⚠️ Hard to observe | ✅ Direct state inspection |
| Vote thresholds | ⚠️ Hard to control | ✅ Precise control |
| Equivocation detection | ✅ Real evidence | ✅ Controlled injection |
| Timeout behavior | ✅ Real timing | ✅ Simulated time |

### Layer 1 Verifies Observable Outcomes

- ✅ Height advancement
- ✅ Consensus halt when quorum lost
- ✅ Recovery after chaos ends
- ✅ No forks (via `tendermint_commit`)
- ❌ Cannot inspect individual votes
- ❌ Cannot verify exact lock state

### Layer 2 Verifies Internal State

- ✅ Exact vote threshold behavior
- ✅ Locking/unlocking transitions
- ✅ POL validation logic
- ✅ Evidence generation internals
- ❌ No real network I/O
- ❌ No real process lifecycle

## 2.3 Component Interactions

```
┌─────────────────────────────────────────────────────────────────┐
│                        Chaos Test                                │
│                            │                                     │
│                            ▼                                     │
│                   ┌─────────────────┐                           │
│                   │ Chaos Injection │                           │
│                   │  - stop/kill    │                           │
│                   │  - disconnect   │                           │
│                   │  - latency      │                           │
│                   └────────┬────────┘                           │
│                            │                                     │
│         ┌──────────────────┼──────────────────┐                 │
│         ▼                  ▼                  ▼                 │
│  ┌────────────┐    ┌────────────┐    ┌────────────┐            │
│  │ Validator 1│    │ Validator 2│    │ Validator 3│            │
│  │            │    │  (target)  │    │            │            │
│  │ ChainActor │    │ ChainActor │    │ ChainActor │            │
│  │     │      │    │     │      │    │     │      │            │
│  │     ▼      │    │     ▼      │    │     ▼      │            │
│  │ Tendermint │    │ Tendermint │    │ Tendermint │            │
│  │   State    │    │   State    │    │   State    │            │
│  └──────┬─────┘    └──────┬─────┘    └──────┬─────┘            │
│         │                 │                 │                   │
│         └────────┬────────┴────────┬────────┘                   │
│                  ▼                 ▼                            │
│           ┌────────────┐   ┌────────────┐                      │
│           │  RPC Query │   │  Log Check │                      │
│           └──────┬─────┘   └──────┬─────┘                      │
│                  │                │                             │
│                  ▼                ▼                             │
│           ┌─────────────────────────────┐                      │
│           │      Verification           │                      │
│           │  - Height advanced?         │                      │
│           │  - Nodes synced?            │                      │
│           │  - Evidence generated?      │                      │
│           └─────────────────────────────┘                      │
└─────────────────────────────────────────────────────────────────┘
```

## 2.4 Data Flow During Tests

### Typical Test Flow

1. **Pre-condition Check:** Verify all validators active
2. **Record Initial State:** Capture current height
3. **Inject Chaos:** Stop node / partition network / add latency
4. **Wait Period:** Allow chaos effects to manifest
5. **Verify Expected Behavior:** Check halt or degradation
6. **Recovery:** Restore normal operation
7. **Verify Recovery:** Check consensus resumed, no forks
8. **Record Result:** Pass or fail with details

### RPC Verification Flow

```
Test Script                          Validator Node
    │                                      │
    │  POST /tendermint_consensusState     │
    │─────────────────────────────────────>│
    │                                      │
    │  {height, round, step, votes}        │
    │<─────────────────────────────────────│
    │                                      │
    │  POST /tendermint_evidence           │
    │─────────────────────────────────────>│
    │                                      │
    │  {evidence: [], total: 0}            │
    │<─────────────────────────────────────│
    │                                      │
    │  [Assert: no evidence = PASS]        │
    │                                      │
```

---

# 3. Quick Start

See [CHAOS_TESTING_QUICKSTART.md](./CHAOS_TESTING_QUICKSTART.md) for a 5-minute getting started guide.

## 3.1 Prerequisites

- Docker 24.0+ with Compose v2
- `jq` for JSON parsing
- `bc` for floating-point calculations
- `tc` for network shaping (Linux only, optional)

## 3.2 Running Tests

```bash
# Start testnet
cd /path/to/alys-v2/etc
docker compose -f docker-compose.tendermint-3node.yml up -d

# Wait for stability
cd chaos-testing
./wait-for-consensus.sh --timeout 120

# Run tier 1 tests
./tendermint-chaos.sh --scenario tier1 --verbose

# Cleanup
docker compose -f docker-compose.tendermint-3node.yml down -v
```

---

# 4. Scenario Reference

## 4.1 Category T: Core Tendermint Consensus Mechanics

### TM-T1: Locking on >2/3 Prevotes

**Purpose:** Verify validators correctly lock after seeing >2/3 prevotes.

**Layer:** L2 (Rust unit tests)

**Test Logic:**
```rust
#[test]
fn test_lock_on_two_thirds_prevotes() {
    let mut harness = TendermintTestHarness::new(3);
    let block_hash = H256::from_slice(&[1u8; 32]);

    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));
    assert_eq!(votes.len(), 3);
}
```

**Pass Criteria:** All 3 validators vote for same block, quorum achieved.

**Fail Criteria:** Quorum not achieved with all honest validators.

---

### TM-T4: NIL Prevote on Timeout

**Purpose:** Verify validators send NIL prevote when no proposal received.

**Layer:** L1 + L2

**L1 Verification:**
```bash
# Force proposal timeout by stopping proposer
./tendermint-chaos.sh --scenario TM-B4  # Proposer isolation
# Round should advance (round > 0 in consensus state)
```

**L2 Test:**
```rust
#[test]
fn test_nil_prevote_behavior() {
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(0, VoteBehavior::AlwaysNil);
    harness.set_validator_behavior(1, VoteBehavior::AlwaysNil);
    harness.set_validator_behavior(2, VoteBehavior::AlwaysNil);

    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));
    assert!(votes.iter().all(|v| v.block_hash.is_none()));
}
```

---

### TM-T7: Proposer Rotation

**Purpose:** Verify correct proposer selection: `(height + round) % n`

**Layer:** L1

**Procedure:**
1. Query current height from consensus state
2. Calculate expected proposer: `(height + round) % 3`
3. Verify actual proposer matches expected

**Manual Verification:**
```bash
HEIGHT=$(curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' | jq -r '.result.height')
ROUND=$(curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' | jq -r '.result.round')
EXPECTED_PROPOSER=$(( (HEIGHT + ROUND) % 3 ))
echo "Expected proposer index: $EXPECTED_PROPOSER"
```

---

## 4.2 Category V: Vote Collection & Thresholds

> **Note:** These tests require Layer 2 (Rust) for precise vote control.

### TM-V1: Prevote Threshold (exactly 2/3)

**Purpose:** Verify that exactly 2/3 prevotes do NOT trigger lock.

**Test:**
```rust
#[test]
fn test_prevote_threshold_exactly_two_thirds() {
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(2, VoteBehavior::Silent);

    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert_eq!(votes.len(), 2);  // Only 2 voted
    assert!(!harness.has_vote_quorum(&votes, Some(block_hash)));  // No quorum
}
```

---

### TM-V2: Prevote Threshold (>2/3)

**Purpose:** Verify that >2/3 prevotes trigger lock.

**Test:**
```rust
#[test]
fn test_prevote_threshold_more_than_two_thirds() {
    let mut harness = TendermintTestHarness::new(3);
    let votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert_eq!(votes.len(), 3);  // All voted
    assert!(harness.has_vote_quorum(&votes, Some(block_hash)));  // Quorum achieved
}
```

---

## 4.3 Category E: Equivocation & Evidence

### TM-E1: Double Prevote Detection

**Purpose:** Verify conflicting prevotes generate evidence.

**Layer:** L2

**Test:**
```rust
#[test]
fn test_double_prevote_generates_evidence() {
    let mut harness = TendermintTestHarness::new(3);
    harness.set_validator_behavior(1, VoteBehavior::Equivocate);

    let _votes = harness.simulate_votes(1, 0, VoteType::Prevote, Some(block_hash));

    assert!(harness.has_evidence());
    assert_eq!(harness.evidence_count(), 1);
    harness.assert_evidence_for(ValidatorId::new(1));
}
```

---

### TM-E5: No Self-Equivocation After Crash

**Purpose:** Verify WAL prevents equivocation after crash/restart.

**Layer:** L1

**Procedure:**
1. Record evidence count before test
2. Crash all validators one at a time
3. Restart each validator
4. Wait for recovery
5. Check evidence count unchanged

**Script Implementation:**
```bash
run_TM_E5() {
    local evidence_before=$(get_evidence "alys-node-1" | jq -r '.total // 0')

    for node in alys-node-1 alys-node-2 alys-node-3; do
        crash_node "$node"
        sleep 1
        restart_node "$node"
        sleep 5
    done

    wait_for_validator_sync "alys-node-1" 30
    wait_for_validator_sync "alys-node-2" 30
    wait_for_validator_sync "alys-node-3" 30

    local evidence_after=$(get_evidence "alys-node-1" | jq -r '.total // 0')

    if [[ "$evidence_after" -gt "$evidence_before" ]]; then
        record_test_result "TM-E5" "FAILED" "Self-equivocation detected"
    else
        record_test_result "TM-E5" "PASSED" "No self-equivocation"
    fi
}
```

---

## 4.4 Category A: Validator Failures

### TM-A1: Single Validator Crash

**Purpose:** Verify consensus halts with 1 of 3 validators down, recovers after restart.

**Layer:** L1

**Expected Behavior:**
- Consensus halts (n=3 requires 100%)
- Consensus resumes after recovery
- All nodes sync to same height

**Procedure:**
1. Verify all validators active
2. Stop one validator
3. Verify consensus halted (height not advancing)
4. Restart validator
5. Verify consensus resumed

**Manual Test:**
```bash
# Initial height
H1=$(curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' | jq -r '.result.height')

# Stop validator
docker stop alys-node-2

# Wait
sleep 15

# Check height hasn't advanced significantly
H2=$(curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' | jq -r '.result.height')
echo "Height change: $((H2 - H1))"  # Should be 0-1

# Restart
docker start alys-node-2

# Wait for recovery
sleep 20

# Verify advancing
H3=$(curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' | jq -r '.result.height')
echo "Height after recovery: $H3"  # Should be > H2
```

---

### TM-A3: Validator Restart & WAL Recovery

**Purpose:** Verify WAL prevents equivocation after crash/restart.

**Layer:** L1

**Critical Safety Check:** After crash and restart, validator must not double-vote.

**Pass Criteria:**
- Validator recovers and syncs
- No equivocation evidence generated
- All nodes at same height

---

## 4.5 Category B: Network Partitions

### TM-B1: Single Node Isolation

**Purpose:** Verify consensus halts when node isolated, recovers when reconnected.

**Layer:** L1

**Procedure:**
```bash
# Isolate node
docker network disconnect alys-tendermint-3 alys-node-2

# Verify halt
sleep 15
# Height should not advance

# Reconnect
docker network connect alys-tendermint-3 alys-node-2

# Verify recovery
sleep 20
# Height should advance
```

---

### TM-B5: Partition Heal & Recovery

**Purpose:** Verify correct recovery after partition heals.

**Layer:** L1

**Critical Checks:**
- No fork after partition heal
- All nodes converge to same height
- Recovery time within bounds

---

## 4.6 Category C: Consensus Timing

### TM-C1: Timeout Storm

**Purpose:** Verify correct handling when all nodes timeout simultaneously.

**Layer:** L1

**Procedure:**
1. Add high latency to all nodes (5s exceeds all timeouts)
2. Wait for timeout cycles
3. Remove latency
4. Verify consensus recovers

---

### TM-C2: Vote Delay Chaos

**Purpose:** Verify consensus works with vote delays.

**Layer:** L1

**Procedure:**
```bash
# Add latency
docker exec alys-node-2 tc qdisc add dev eth0 root netem delay 500ms

# Run for 30 seconds
sleep 30

# Remove latency
docker exec alys-node-2 tc qdisc del dev eth0 root

# Verify blocks were produced (slower but correct)
```

---

## 4.7 Category D: WAL & Crash Recovery

### TM-D1: Crash After Prevote

**Purpose:** Verify WAL prevents double-prevote after crash.

**Layer:** L1

**Procedure:**
1. Crash validator during voting
2. Restart immediately
3. Wait for recovery
4. Check no equivocation evidence

---

### TM-D3: Crash While Locked

**Purpose:** Verify lock state preserved across crash.

**Layer:** L1 + L2

**L1 Verification:** All nodes commit same block after recovery
**L2 Verification:** Lock state matches pre-crash state

---

## 4.8 Category L: Liveness & Safety

### TM-L1: Round Stall Recovery

**Purpose:** Verify consensus recovers from stalled round.

**Layer:** L1

---

### TM-L3: Height Progression

**Purpose:** Verify steady block production under normal conditions.

**Layer:** L1

**Procedure:**
1. Run for 60 seconds
2. Measure blocks produced
3. Verify rate is acceptable

**Pass Criteria:** At least 10 blocks in 60 seconds (~1 block per 6 seconds with round 0)

---

## 4.9 Category F: External Dependencies

### TM-F1: Execution Layer Failure

**Purpose:** Verify graceful handling when execution layer stops.

**Layer:** L1

**Procedure:**
```bash
docker stop execution
sleep 20
docker start execution
sleep 20
# Verify consensus resumed
```

---

### TM-F2: Bitcoin Core Failure

**Purpose:** Verify consensus continues without Bitcoin Core (AuxPoW not required for consensus).

**Layer:** L1

---

# 5. Extending the Framework

## 5.1 Adding New Bash Scenarios

### Template

```bash
run_TM_XX() {
    log_info "[TM-XX] Running: Description"

    # Pre-conditions
    if ! verify_all_validators_active; then
        record_test_result "TM-XX" "FAILED" "Pre-condition failed"
        return
    fi

    local initial_height=$(get_consensus_height "alys-node-1")

    # Execute chaos
    # ... your chaos injection here ...

    # Verify expected behavior
    # ... your verification here ...

    # Recovery
    # ... cleanup chaos ...

    # Record result
    if [[ condition ]]; then
        record_test_result "TM-XX" "PASSED" "Description"
    else
        record_test_result "TM-XX" "FAILED" "Reason"
    fi
}
```

### Integration Steps

1. Add function `run_TM_XX()` to `tendermint-chaos.sh`
2. Add case `TM-XX) run_TM_XX ;;` to dispatcher
3. Add to appropriate scenario group (tier1, tier2, etc.)
4. Update usage documentation
5. Update this guide

## 5.2 Adding New Rust Chaos Injectors

### Add Scenario to Enum

```rust
// In tendermint_chaos.rs
pub enum TendermintChaosScenario {
    // ... existing scenarios ...

    /// Your new scenario
    NewScenario { param: u64 },
}
```

### Implement Injection Logic

```rust
impl TendermintChaosInjector {
    pub fn handle_new_scenario(&mut self, param: u64) -> bool {
        // Implementation
    }
}
```

### Add Test

```rust
#[test]
fn test_new_scenario() {
    let mut harness = TendermintTestHarness::new(3);
    harness.inject_chaos(TendermintChaosScenario::NewScenario { param: 100 });
    // Assertions
}
```

## 5.3 Adding New RPC Verification Methods

Follow the pattern in `handlers.rs` for existing Tendermint RPC methods.

---

# 6. Troubleshooting

## 6.1 Common Test Failures

| Symptom | Likely Cause | Resolution |
|---------|--------------|------------|
| "Not all validators active" | Containers not running | `docker compose up -d` |
| "Consensus halted" (unexpected) | n=3 requires 100% | Check all nodes running |
| "Height not advancing" | Execution layer down | Check `docker logs execution` |
| "Evidence detected" | Bug or test design | Review what caused equivocation |
| "Timeout waiting for height" | Slow consensus | Increase timeout, check resources |
| "WAL recovery failed" | Disk space | Check available disk space |
| "iptables not available" | Missing NET_ADMIN | Add capability to container |

## 6.2 Debug Logging

Enable verbose logging:

```bash
# Bash tests
./tendermint-chaos.sh --scenario TM-A1 --verbose

# Rust tests
RUST_LOG=debug cargo test --package alys --lib actors_v2::testing::chaos
```

## 6.3 Container Inspection

```bash
# Container status
docker ps -a --filter "name=alys"

# Container logs
docker logs alys-node-1 --tail 100

# Execute commands in container
docker exec -it alys-node-1 /bin/sh

# Inspect network
docker network inspect alys-tendermint-3
```

## 6.4 RPC Debugging

```bash
# Consensus state
curl -s localhost:3001 \
  -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' \
  | jq .

# Evidence check
curl -s localhost:3001 \
  -d '{"jsonrpc":"2.0","method":"tendermint_evidence","params":[],"id":1}' \
  | jq .

# Validator set
curl -s localhost:3001 \
  -d '{"jsonrpc":"2.0","method":"tendermint_validators","params":[],"id":1}' \
  | jq .
```

---

# 7. Operational Runbooks

See [CHAOS_TESTING_RUNBOOK.md](./CHAOS_TESTING_RUNBOOK.md) for detailed operational procedures.

---

# 8. Appendices

## Appendix A: RPC Endpoint Reference

| Method | Purpose | Parameters |
|--------|---------|------------|
| `tendermint_consensusState` | Current height/round/step/votes | None |
| `tendermint_validators` | Validator set | `height`, `page`, `per_page` |
| `tendermint_commit` | Commit proof | `height` |
| `tendermint_evidence` | Detected equivocation | `max_age_blocks` |
| `tendermint_params` | Consensus parameters | None |

## Appendix B: Docker Command Reference

| Action | Command |
|--------|---------|
| Start testnet | `docker compose -f docker-compose.tendermint-3node.yml up -d` |
| Stop testnet | `docker compose -f docker-compose.tendermint-3node.yml down` |
| Stop with volumes | `docker compose -f docker-compose.tendermint-3node.yml down -v` |
| View logs | `docker logs -f alys-node-1` |
| Restart node | `docker restart alys-node-2` |
| Stop node gracefully | `docker stop alys-node-2` |
| Kill node (crash) | `docker kill alys-node-2` |
| Network isolate | `docker network disconnect alys-tendermint-3 alys-node-2` |
| Network reconnect | `docker network connect alys-tendermint-3 alys-node-2` |
| Add latency | `docker exec alys-node-2 tc qdisc add dev eth0 root netem delay 500ms` |
| Remove latency | `docker exec alys-node-2 tc qdisc del dev eth0 root` |

## Appendix C: Prometheus Metrics Reference

| Metric | Description |
|--------|-------------|
| `tendermint_consensus_height` | Current consensus height |
| `tendermint_consensus_round` | Current round number |
| `tendermint_consensus_validators` | Number of validators |
| `tendermint_p2p_peers` | Connected peers count |
| `tendermint_p2p_message_receive_total` | Messages received |
| `tendermint_p2p_message_send_total` | Messages sent |

## Appendix D: Glossary

| Term | Definition |
|------|------------|
| **BFT** | Byzantine Fault Tolerant |
| **Commit** | Final agreement on block (>2/3 precommits) |
| **Evidence** | Proof of validator misbehavior (equivocation) |
| **Height** | Block number in the chain |
| **Lock** | Validator commitment to a block after >2/3 prevotes |
| **NIL** | Vote for no block (timeout or invalid proposal) |
| **POL** | Proof-of-Lock (>2/3 prevotes from higher round) |
| **Precommit** | Second voting phase (after prevote) |
| **Prevote** | First voting phase (after proposal) |
| **Proposer** | Validator selected to propose block for round |
| **Quorum** | >2/3 of voting power |
| **Round** | Attempt to reach consensus at a height |
| **WAL** | Write-Ahead Log (crash recovery) |

## Appendix E: Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | March 2026 | Initial release |
