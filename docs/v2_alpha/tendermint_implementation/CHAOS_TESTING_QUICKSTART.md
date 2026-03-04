# Tendermint Chaos Testing Quick Start

Get chaos tests running in under 5 minutes.

## Prerequisites

- Docker 24.0+ with Compose v2
- `jq` installed (`brew install jq` or `apt-get install jq`)
- Project cloned and built

## 1. Start the Testnet

```bash
cd /path/to/alys-v2/etc
docker compose -f docker-compose.tendermint-3node.yml up -d
```

## 2. Wait for Consensus

```bash
cd chaos-testing
./wait-for-consensus.sh --timeout 120 --min-blocks 5
```

Expected output:
```
[INFO] Waiting for Tendermint consensus to stabilize...
[INFO]   Timeout: 120s
[INFO]   Min blocks: 5
[INFO] All 3 nodes responding
[INFO] All nodes synchronized at height 12
[INFO] Waiting for 5 blocks to be produced...
[INFO] Consensus stabilized: 17 blocks produced
[INFO] Consensus ready in 45s
```

## 3. Run Tests

### Single Scenario
```bash
./tendermint-chaos.sh --scenario TM-A1 --verbose
```

### Tier 1 (Core Tests)
```bash
./tendermint-chaos.sh --scenario tier1
```

### All Tests
```bash
./tendermint-chaos.sh --scenario all
```

## 4. Read Results

```
==============================================
         CHAOS TEST SUMMARY
==============================================
[PASS] TM-A1: Single Validator Crash & Recovery
[PASS] TM-A3: Validator Restart & WAL Recovery
[PASS] TM-B1: Single Node Isolation & Recovery
[PASS] TM-B5: Partition Heal & Consensus Resume
[PASS] TM-L3: Height Progression

----------------------------------------------
Total: 5 | Passed: 5 | Failed: 0
==============================================
```

## 5. Cleanup

```bash
cd /path/to/alys-v2/etc
docker compose -f docker-compose.tendermint-3node.yml down -v
```

## Scenario Groups

| Group | Duration | What it Tests |
|-------|----------|---------------|
| `tier1` | ~10 min | Core crash/recovery |
| `tier2` | ~15 min | WAL & external deps |
| `validator` | ~15 min | All validator failures |
| `network` | ~15 min | Network partitions |
| `timing` | ~10 min | Timeout handling |
| `wal` | ~15 min | Crash recovery |
| `all` | ~60 min | Everything (24 scenarios) |

## Rust Unit Tests

For Layer 2 (internal consensus logic) tests:

```bash
cd /path/to/alys-v2/app
cargo test --package alys --lib actors_v2::testing::chaos
```

## Troubleshooting

### "Not all validators are active"
```bash
docker compose -f docker-compose.tendermint-3node.yml up -d
./wait-for-consensus.sh --timeout 180
```

### Tests timing out
Check execution layer health:
```bash
docker logs execution | tail -50
```

### Need more verbose output
```bash
./tendermint-chaos.sh --scenario TM-A1 --verbose
```

## Next Steps

- **Full Guide**: [TENDERMINT_CHAOS_TESTING_GUIDE.md](./TENDERMINT_CHAOS_TESTING_GUIDE.md)
- **Runbook**: [CHAOS_TESTING_RUNBOOK.md](./CHAOS_TESTING_RUNBOOK.md)
- **Plan**: [TENDERMINT_CHAOS_TESTING_PLAN.md](./TENDERMINT_CHAOS_TESTING_PLAN.md)
