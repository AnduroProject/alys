# Tendermint Chaos Testing Framework

A comprehensive chaos engineering framework for testing Tendermint BFT consensus resilience, safety, and liveness.

## Overview

This framework validates:

1. **Tendermint Consensus Operations**: Locking rules, POL (Proof-of-Lock), vote thresholds, round advancement, proposer rotation, and commit finality
2. **Safety Guarantees**: No equivocation, no forks after commit, correct WAL recovery
3. **Liveness Properties**: Timeout handling, round progression, partition recovery
4. **System Resilience**: Validator failures, network partitions, crash recovery

## Quick Start

### 1. Start the Tendermint Testnet

```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/etc
docker compose -f docker-compose.tendermint-3node.yml up -d
```

### 2. Wait for Consensus to Stabilize

```bash
cd chaos-testing
./wait-for-consensus.sh --timeout 120 --min-blocks 5
```

### 3. Run Chaos Tests

```bash
# Run core tier 1 scenarios
./tendermint-chaos.sh --scenario tier1

# Run a specific scenario
./tendermint-chaos.sh --scenario TM-A1 --verbose

# Run all 24 scenarios
./tendermint-chaos.sh --scenario all
```

## Test Scenarios

### Category A: Validator Failures

| ID | Scenario | Description |
|----|----------|-------------|
| TM-A1 | Single Validator Crash | Stop 1 of 3 validators, verify halt & recovery |
| TM-A2 | Proposer Crash | Stop current proposer, verify round advancement |
| TM-A3 | WAL Recovery | Crash and restart, verify no equivocation |
| TM-A4 | Recovery Timing | Measure time to resume consensus |
| TM-A5 | Sequential Restarts | Restart each validator sequentially |

### Category B: Network Partitions

| ID | Scenario | Description |
|----|----------|-------------|
| TM-B1 | Single Node Isolation | Disconnect one node from network |
| TM-B2 | 2-1 Partition | Split into 2 nodes vs 1 node |
| TM-B3 | Asymmetric Partition | One-way network connectivity |
| TM-B4 | Proposer Isolation | Isolate the current proposer |
| TM-B5 | Partition Heal | Create partition, then heal and verify recovery |

### Category C: Timing

| ID | Scenario | Description |
|----|----------|-------------|
| TM-C1 | Timeout Storm | Force all nodes to timeout simultaneously |
| TM-C2 | Vote Delay | Add 500ms latency to vote propagation |
| TM-C3 | Slow Proposal | Delay proposal broadcast near timeout |
| TM-C4 | Fast Rounds | Verify handling of rapid state transitions |

### Category D: WAL & Crash Recovery

| ID | Scenario | Description |
|----|----------|-------------|
| TM-D1 | Crash After Prevote | WAL prevents double-prevote |
| TM-D2 | Crash After Precommit | WAL prevents double-precommit |
| TM-D3 | Crash While Locked | Lock state preserved across crash |
| TM-D4 | Corrupt WAL | Safe recovery from corruption |
| TM-D5 | Repeated Crashes | Multiple crash/recovery cycles |

### Category E: Equivocation Prevention

| ID | Scenario | Description |
|----|----------|-------------|
| TM-E5 | No Self-Equivocation | WAL prevents self-equivocation after crash |

### Category F: External Dependencies

| ID | Scenario | Description |
|----|----------|-------------|
| TM-F1 | Execution Layer Failure | Stop Reth, verify graceful handling |
| TM-F2 | Bitcoin Core Failure | Stop Bitcoin Core, consensus continues |
| TM-F3 | Monitoring Failure | Stop Prometheus, consensus unaffected |

### Category L: Liveness & Safety

| ID | Scenario | Description |
|----|----------|-------------|
| TM-L1 | Round Stall Recovery | Force stall, then recover |
| TM-L2 | Multi-Round Block | Block commits after multiple rounds |
| TM-L3 | Height Progression | Verify steady block production |

## Scenario Groups

| Group | Scenarios | Duration |
|-------|-----------|----------|
| `tier1` | A1, A3, B1, B5, L3 | ~10 min |
| `tier2` | D1-D3, E5, F1-F2 | ~15 min |
| `validator` | A1-A5 | ~15 min |
| `network` | B1-B5 | ~15 min |
| `timing` | C1-C4 | ~10 min |
| `wal` | D1-D5, E5 | ~15 min |
| `external` | F1-F3 | ~10 min |
| `liveness` | L1-L3 | ~10 min |
| `all` | All 24 scenarios | ~60 min |

## BFT Properties (n=3)

With 3 validators:
- **Quorum**: `floor(2×3/3) + 1 = 3` (100% required)
- **Fault Tolerance**: 0 (cannot tolerate any failures)
- **Implication**: Any single failure **halts consensus** (expected behavior)

Tests focus on:
- Correct halt behavior when quorum is lost
- Proper recovery when quorum is restored
- No safety violations during or after chaos

## Files

| File | Purpose |
|------|---------|
| `tendermint-chaos.sh` | Main chaos testing script (24 scenarios) |
| `wait-for-consensus.sh` | Startup verification before tests |

## Layer 2 Tests (Rust)

For precise control over consensus state, use the Rust unit tests:

```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/app

# Run all Tendermint chaos tests
cargo test --package alys --lib actors_v2::testing::chaos::tendermint

# Run specific test categories
cargo test --package alys --lib actors_v2::testing::chaos::chain_chaos_tests
cargo test --package alys --lib actors_v2::testing::chaos::driver_chaos_tests
```

These tests provide:
- Exact vote threshold verification
- Locking/unlocking state inspection
- Evidence generation testing
- Chaos injection with precise control

## Monitoring

During tests, you can monitor via:

- **Grafana**: http://localhost:3030 (admin/admin)
- **Prometheus**: http://localhost:9093
- **Node logs**: `docker logs -f alys-node-1`

## CI/CD

Chaos tests run automatically:
- **Daily**: Tier 1 scenarios at 2 AM UTC
- **Manual**: Via GitHub Actions workflow dispatch

See `.github/workflows/chaos-testing.yml`

## Troubleshooting

### "Not all validators are active"

```bash
docker compose -f docker-compose.tendermint-3node.yml up -d
./wait-for-consensus.sh --timeout 180
```

### "iptables not available"

Some network chaos scenarios require `NET_ADMIN` capability. Ensure containers have:
```yaml
cap_add:
  - NET_ADMIN
```

### Tests timing out

Increase timeouts or check if execution layer is healthy:
```bash
docker logs execution | tail -50
```

## Contributing

To add new scenarios:

1. Add scenario function `run_TM_XX()` in `tendermint-chaos.sh`
2. Add case in `run_scenario()` dispatcher
3. Update usage documentation
4. Add to appropriate scenario group
5. Update this README

## References

- [Tendermint BFT Paper](https://arxiv.org/abs/1807.04938)
- [TENDERMINT_CHAOS_TESTING_PLAN.md](../docs/v2_alpha/tendermint_implementation/TENDERMINT_CHAOS_TESTING_PLAN.md)
