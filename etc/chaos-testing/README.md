# Alys V2 Chaos Testing Framework

A comprehensive chaos engineering framework for testing the Alys V2 local regtest environment's resilience and recovery capabilities.

## Overview

This chaos testing framework allows you to:
- Inject various failure scenarios into your running regtest environment
- Monitor system behavior during chaos events
- Verify automatic recovery
- Generate detailed reports with metrics and analysis

The framework is consolidated into a single unified script (`tier1-scenarios.sh`) that supports multiple modes of operation.

## Prerequisites

1. **Running Regtest Environment:**
   ```bash
   cd /Users/michael/zDevelopment/Mara/alys-v2/etc
   docker compose -f docker-compose.v2-regtest.yml up -d

   # Wait for system to stabilize
   sleep 30
   ```

2. **Required Tools:**
   - Docker & Docker Compose
   - `jq` for JSON processing
     - macOS: `brew install jq`
     - Ubuntu: `sudo apt-get install jq`
   - `bc` for calculations (usually pre-installed)

3. **Optional Tools:**
   - `tc` (traffic control) for advanced network chaos (requires NET_ADMIN capability)

---

## Three Modes of Operation

All modes are accessed through the unified `tier1-scenarios.sh` script:

### Mode 1: Scenario Testing (Default)

**Best for:**
- Validating V2 sync and recovery behavior
- Testing specific failure scenarios with assertions
- CI/CD regression testing
- Pass/fail verification of blockchain state

**Features:**
- Blockchain-aware verification (block heights, sync status)
- **Dynamic n-node support** - auto-detects all running `alys-node-*` containers
- **Random target selection** - each scenario randomly selects a node to disrupt
- Structured pass/fail assertions
- Automatic report generation

**Usage:**
```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/etc/chaos-testing

# Run all scenarios with auto-detected nodes
./tier1-scenarios.sh --scenario all

# Run only core tier 1 scenarios (1-3)
./tier1-scenarios.sh --scenario tier1

# Run specific scenario with verbose output
./tier1-scenarios.sh --scenario 1 --verbose

# Limit to first N nodes
./tier1-scenarios.sh --scenario all --nodes 2
```

**Command Line Options:**
```
Options:
    --mode scenario          Run structured test scenarios (default)
    --scenario <N|tier1|all> Run specific scenario(s)
    --nodes <N>              Limit to first N nodes (default: auto-detect all)
    --verbose                Enable verbose output
    --help                   Show help message

Scenarios:
    Core (Tier 1):
      1 - Network Partition
      2 - Node Restart
      3 - Leader Failover

    Network:
      4 - Network Latency
      5 - Packet Loss

    Resource:
      6 - Memory Pressure
      7 - Disk I/O Stress

    Infrastructure:
      8 - Execution Layer Failure
      9 - Bitcoin Core Failure

    Composite:
      tier1 - Run scenarios 1-3
      all   - Run scenarios 1-9
```

---

### Mode 2: Interactive Testing

**Best for:**
- Manual chaos injection and observation
- Learning how the system responds
- Debugging specific scenarios
- Quick ad-hoc testing

**Features:**
- Interactive menu-driven interface
- Live system status display
- Real-time log viewing
- Manual control over chaos duration
- Session logging and reporting

**Usage:**
```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/etc/chaos-testing
./tier1-scenarios.sh --mode interactive
```

**Workflow:**
1. Launch script
2. View current system status
3. Select chaos scenario from menu
4. Observe logs and behavior
5. Manually trigger recovery when ready
6. Generate report at end of session

---

### Mode 3: Stress Testing

**Best for:**
- Unattended testing
- Consistent reproducible tests
- Long-running stress tests
- CI/CD integration

**Features:**
- Fully automated execution with random chaos injection
- Configurable duration and failure rate
- Metrics collection
- Comprehensive markdown reports

**Usage:**
```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/etc/chaos-testing

# Quick 5-minute stress test
./tier1-scenarios.sh --mode stress

# Extended stress test with custom settings
./tier1-scenarios.sh --mode stress --duration 600 --failure-rate 0.3

# 10-minute stress test with 20% failure probability
./tier1-scenarios.sh --mode stress --duration 600 --failure-rate 0.2
```

**Command Line Options:**
```bash
./tier1-scenarios.sh --mode stress [OPTIONS]

Options:
  --duration SECONDS       Test duration in seconds (default: 300)
  --failure-rate RATE      Probability of chaos injection 0.0-1.0 (default: 0.3)
  --nodes <N>              Limit to first N nodes (default: auto-detect all)
  --verbose                Enable verbose output
```

---

## Available Chaos Scenarios

| # | Scenario | Description | Impact | Recovery Time |
|---|----------|-------------|--------|---------------|
| 1 | Network Partition | Disconnects node from network | High - Node cannot communicate | ~30s after restoration |
| 2 | Node Restart | Stops and restarts a random node | Critical - Node goes offline | ~15s restart + sync |
| 3 | Leader Failover | Takes down random "leader" node | Critical - Tests remaining nodes | ~15s restart + sync |
| 4 | Network Latency | Adds 500ms latency to all nodes | Medium - Slows communication | Immediate |
| 5 | Packet Loss | 10% packet loss on random node | Medium - Degrades networking | Immediate |
| 6 | Memory Pressure | 256MB memory stress on random node | Medium - May slow operations | Immediate |
| 7 | Disk I/O Stress | Heavy disk writes on random node | Medium - Database slows | Immediate |
| 8 | Execution Failure | Stops and restarts execution layer | Critical - No block production | ~20s restart + sync |
| 9 | Bitcoin Failure | Stops and restarts Bitcoin Core | High - No AuxPoW operations | ~20s restart + sync |

---

## Quick Start Guide

### Your First Chaos Test (Step-by-Step)

#### Step 1: Start Environment

```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/etc
docker compose -f docker-compose.v2-regtest.yml up -d
sleep 30  # Wait for startup
```

#### Step 2: Verify System Health

```bash
# Check all containers are running
docker compose -f docker-compose.v2-regtest.yml ps

# Should see:
# alys-node-1      Up
# alys-node-2      Up
# execution        Up
# bitcoin-core     Up
# prometheus       Up
# grafana          Up
```

#### Step 3: Open Monitoring (Optional but Recommended)

```bash
# Terminal 2: Watch Node 1 logs
docker logs -f alys-node-1

# Terminal 3: Watch Node 2 logs
docker logs -f alys-node-2

# Browser: Open Grafana
open http://localhost:3030
# Login: admin/admin
```

#### Step 4: Run Tier 1 Validation Tests

```bash
cd chaos-testing

# Run core tier 1 scenarios (network partition, restart, failover)
./tier1-scenarios.sh --scenario tier1

# Or run all 9 scenarios
./tier1-scenarios.sh --scenario all

# Expected output:
# ================================================================================
#   SCENARIO 1: Network Partition Recovery
# ================================================================================
# Target node: alys-node-2
# Reference node: alys-node-1
# Other nodes: alys-node-1
#
# [HH:MM:SS] -> Step 1: Recording initial state...
# [HH:MM:SS]   alys-node-1 height: 50
# [HH:MM:SS]   alys-node-2 height: 50
# [HH:MM:SS] -> Step 2: Disconnecting alys-node-2 from network...
# ... (steps continue) ...
# RESULT: PASSED (120 seconds)
```

#### Step 5: Run Interactive Chaos Test

```bash
./tier1-scenarios.sh --mode interactive

# In the menu:
# 1. Press '1' to inject network partition
# 2. Observe the system status change
# 3. Watch logs in real-time
# 4. Press Enter to restore network
# 5. Press 's' to check recovery
# 6. Press 'r' to generate report
# 7. Press 'q' to quit
```

#### Step 6: Run Automated Stress Test

```bash
# Run 3-minute stress test with random chaos
./tier1-scenarios.sh --mode stress --duration 180

# View report
cat ../../reports/chaos-testing/tier1-*-report.md | head -50
```

---

## Common Test Scenarios

### Scenario: Test Network Resilience

**Goal:** Verify nodes can recover from network partition

**Scenario Mode (Recommended):**
```bash
./tier1-scenarios.sh --scenario 1 --verbose
# Automatically selects random node, partitions it, verifies recovery
```

**Interactive Mode:**
```bash
./tier1-scenarios.sh --mode interactive
# Choose: 1) Network partition
# Observe: Nodes disconnect, gossipsub mesh breaks
# Wait: 30-60 seconds
# Restore: Press Enter
# Verify: Nodes reconnect and sync
```

**Stress Mode:**
```bash
./tier1-scenarios.sh --mode stress --duration 300
```

---

### Scenario: Test Node Crash Recovery

**Goal:** Verify data persistence and sync after node restart

**Scenario Mode (Recommended):**
```bash
./tier1-scenarios.sh --scenario 2 --verbose
# Automatically selects random node, stops it, restarts, verifies sync
```

**Interactive Mode:**
```bash
./tier1-scenarios.sh --mode interactive
# Choose: 4) Stop random node
# Observe: Node stops, logs cease
# Wait: Check other nodes continue
# Restore: Press Enter to restart
# Verify: Node syncs from peers
```

---

### Scenario: Test Leader Failover

**Goal:** Verify remaining nodes continue when one fails

**Scenario Mode (Recommended):**
```bash
./tier1-scenarios.sh --scenario 3 --verbose
# Automatically selects random "leader", takes it down, verifies others continue
```

---

### Scenario: Test Execution Layer Dependency

**Goal:** Verify behavior when execution layer is unavailable

**Scenario Mode:**
```bash
./tier1-scenarios.sh --scenario 8 --verbose
```

**Interactive Mode:**
```bash
./tier1-scenarios.sh --mode interactive
# Choose: 7) Execution layer failure
# Observe: Block production stops, nodes log errors
# Restore: Press Enter to restart Reth
# Verify: Block production resumes
```

---

## Understanding Reports

### Report Structure

Each chaos test generates:

1. **Markdown Report** (`chaos-YYYYMMDD-HHMMSS-report.md` or `tier1-YYYYMMDD-HHMMSS-report.md`)
   - Executive summary
   - Test configuration
   - Node count and node list
   - Results summary with success rate
   - Event timeline
   - Container health status
   - Recommendations

2. **Event Log** (`chaos-YYYYMMDD-HHMMSS-events.json`)
   - Structured JSON data
   - All chaos events with timestamps
   - Recovery status for each event
   - Test metadata

3. **Metrics** (`metrics-TIMESTAMP.json`)
   - Container CPU and memory usage
   - Container status and health
   - Collected every 10 seconds

4. **Container Logs** (`chaos-YYYYMMDD-HHMMSS-{node1,node2,...,execution,bitcoin}.log`)
   - Complete logs from each container
   - Useful for debugging failed recoveries

### Interpreting Results

#### Success Rates

| Success Rate | Status | Meaning |
|--------------|--------|---------|
| **95-100%** | Excellent | Robust fault tolerance |
| **80-94%** | Good | Generally resilient, some issues |
| **60-79%** | Fair | Significant recovery problems |
| **<60%** | Poor | Critical resilience issues |

#### Good Signs
- All containers restart successfully
- Network connectivity restored within 30s
- No panic/fatal errors in logs
- Block production resumes after recovery
- Peers reconnect automatically

#### Warning Signs
- Recovery takes >60 seconds
- Some errors persist after recovery
- Peers don't reconnect automatically
- Manual intervention sometimes needed
- Inconsistent recovery behavior

#### Bad Signs
- Containers don't restart
- Persistent errors after recovery
- Data corruption or loss
- Peers never reconnect
- System requires manual intervention

---

## Monitoring During Chaos Tests

### Real-Time Log Monitoring

```bash
# Terminal 1: Chaos test
./tier1-scenarios.sh --scenario all --verbose

# Terminal 2: Node 1 logs
docker logs -f --tail=100 alys-node-1

# Terminal 3: Node 2 logs
docker logs -f --tail=100 alys-node-2

# Terminal 4: Container stats
watch -n 2 'docker stats --no-stream'
```

### Grafana Dashboards

1. Open Grafana: http://localhost:3030
2. Login: admin/admin
3. Navigate to Alys V2 dashboards:
   - **Alys V2 Overview**: System-wide metrics
   - **Alys V2 Chain**: Block production and validation
   - **Alys V2 Network**: P2P networking and gossipsub
   - **Alys V2 Storage**: Database and cache metrics

### Prometheus Queries

Access Prometheus: http://localhost:9092

Useful queries:
```promql
# Block production rate
rate(alys_chain_blocks_produced_total[1m])

# Network peer count
alys_network_connected_peers

# Storage operations per second
rate(alys_storage_operations_total[1m])

# Error rate
rate(alys_errors_total[1m])
```

---

## Tips for Effective Chaos Testing

### 1. Start Small
- Begin with 1-2 minute tests
- Test one scenario at a time
- Use interactive mode to understand behavior
- Don't start with overnight tests

### 2. Monitor Actively
- Keep Grafana open during tests
- Watch logs in real-time
- Note interesting events and timestamps
- Don't run tests "blind"

### 3. Document Findings
- Generate reports after each session
- Note unexpected behaviors
- Track recovery times
- Keep a testing journal

### 4. Iterate and Improve
- Start with low failure rates (10-15%)
- Gradually increase intensity
- Test scenarios that previously failed
- Verify fixes actually work

### 5. Use the Right Mode
- **Tier 1**: Validation, CI/CD, pass/fail testing
- **Interactive**: Learning, debugging, exploration
- **Automated**: Regression testing, stress tests

---

## Advanced Usage

### Multi-Node Testing with Tier 1

```bash
# Run with 3 nodes
docker compose -f docker-compose.v2-regtest.yml up -d alys-node-1 alys-node-2 alys-node-3
./tier1-scenarios.sh --scenario all --verbose

# Run with only 2 of 3 available nodes
./tier1-scenarios.sh --nodes 2 --scenario 1
```

### Extended Stress Test

Run overnight stress test:
```bash
# 8-hour chaos test with 30% failure rate
./tier1-scenarios.sh --mode stress --duration 28800 --failure-rate 0.30
```

---

## Troubleshooting

### Error: "jq: command not found"

```bash
brew install jq  # macOS
sudo apt-get install jq  # Ubuntu
```

### Error: "Docker Compose environment is not running"

```bash
cd /Users/michael/zDevelopment/Mara/alys-v2/etc
docker compose -f docker-compose.v2-regtest.yml up -d
```

### Error: "At least 2 nodes required for chaos testing"

```bash
# Start additional nodes
docker compose -f docker-compose.v2-regtest.yml up -d alys-node-1 alys-node-2
```

### Error: Network chaos injection fails

**Problem:** `iptables` or `tc` commands fail in containers

**Solutions:**

1. **Add NET_ADMIN capability** (Recommended):
   Edit `docker-compose.v2-regtest.yml`:
   ```yaml
   alys-node-1:
     cap_add:
       - NET_ADMIN

   alys-node-2:
     cap_add:
       - NET_ADMIN
   ```

2. **Install tc in containers**:
   ```bash
   docker exec alys-node-1 apk add iproute2 iptables
   docker exec alys-node-2 apk add iproute2 iptables
   ```

3. **Use alternative chaos scenarios** that don't require network tools

### Chaos persists after test

```bash
# Clean network rules
docker exec alys-node-1 sh -c "tc qdisc del dev eth0 root 2>/dev/null" || true
docker exec alys-node-2 sh -c "tc qdisc del dev eth0 root 2>/dev/null" || true

# Restart containers
docker compose -f ../docker-compose.v2-regtest.yml restart
```

### Recovery always fails

**Diagnosis:**
1. Check recovery timeout (default: 120s for sync)
2. Review container logs for actual errors
3. Verify containers restart correctly

**Solutions:**
```bash
# Test manual recovery
docker compose -f docker-compose.v2-regtest.yml restart alys-node-1
sleep 30
docker exec alys-node-1 ps aux  # Verify process is running

# Run with verbose output to see what's failing
./tier1-scenarios.sh --scenario 1 --verbose
```

---

## What to Do If Recovery Fails

### 1. Collect Information

```bash
# Check container status
docker compose -f ../docker-compose.v2-regtest.yml ps

# View recent logs
docker logs --tail=100 alys-node-1
docker logs --tail=100 alys-node-2

# Check for errors
docker logs alys-node-1 2>&1 | grep -i error | tail -20
```

### 2. Manual Recovery

```bash
# Restart specific container
docker compose -f ../docker-compose.v2-regtest.yml restart alys-node-1

# Full system restart
docker compose -f ../docker-compose.v2-regtest.yml down
docker compose -f ../docker-compose.v2-regtest.yml up -d

# Clean network rules (if network chaos persists)
docker exec alys-node-1 sh -c "tc qdisc del dev eth0 root 2>/dev/null" || true
docker exec alys-node-2 sh -c "tc qdisc del dev eth0 root 2>/dev/null" || true
```

### 3. Report the Issue

```bash
# Save logs
docker logs alys-node-1 > failure-node1.log 2>&1
docker logs alys-node-2 > failure-node2.log 2>&1

# Create GitHub issue with:
# - Chaos scenario that failed
# - Recovery timeout used
# - Container logs
# - Steps to reproduce
```

---

## Best Practices

### Pre-Chaos Checklist
- Regtest environment is running and stable
- All containers show "Up" status
- Grafana dashboards accessible
- Baseline metrics collected (first 30s of test)

### During Chaos Testing
- Monitor Grafana dashboards
- Keep terminal logs open
- Note timestamps of interesting events
- Don't interfere with automatic recovery

### Post-Chaos Analysis
- Read generated report thoroughly
- Investigate all failed recoveries
- Compare metrics before/during/after chaos
- Document patterns and recurring issues
- Create issues for bugs discovered

### Iterative Testing
1. Start with low failure rate (0.10-0.15)
2. Test individual scenarios first
3. Gradually increase intensity
4. Run extended tests (hours) for stability verification
5. Test specific scenarios that previously failed

---

## Integration with CI/CD

### GitHub Actions Workflow (Future)

```yaml
name: Chaos Testing

on:
  schedule:
    - cron: '0 2 * * *'  # Nightly at 2 AM
  workflow_dispatch:

jobs:
  chaos-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Start regtest environment
        run: |
          cd etc
          docker compose -f docker-compose.v2-regtest.yml up -d
          sleep 30

      - name: Run all chaos scenarios
        run: |
          cd etc/chaos-testing
          ./tier1-scenarios.sh --scenario all

      - name: Run stress test
        run: |
          cd etc/chaos-testing
          ./tier1-scenarios.sh --mode stress --duration 600 --failure-rate 0.2

      - name: Upload reports
        uses: actions/upload-artifact@v3
        with:
          name: chaos-reports
          path: reports/chaos-testing/

      - name: Check success rate
        run: |
          # Fail if any tests failed
          grep -q "Failed Tests | 0" reports/chaos-testing/*.md
```

---

## Metrics and KPIs

### System Health Indicators

Track these over multiple chaos tests:

1. **Recovery Success Rate**: Should be >90%
2. **Mean Time to Recovery (MTTR)**: Should decrease over time
3. **Error Count During Chaos**: Should remain low
4. **Block Production Continuity**: Minimal gaps during chaos

### Improvement Tracking

| Week | Success Rate | MTTR (avg) | Failed Scenarios | Notes |
|------|--------------|------------|------------------|-------|
| W1 | 75% | 45s | node_crash, execution_failure | Initial baseline |
| W2 | 82% | 38s | execution_failure | Improved node crash recovery |
| W3 | 91% | 28s | - | All scenarios passing |
| W4 | 95% | 22s | - | Production-ready resilience |

---

## Next Steps

1. **Review the Master Testing Guide**: See how chaos tests fit into overall V2 testing strategy
   - `docs/v2_alpha/V2_MASTER_TESTING_GUIDE.knowledge.md`

2. **Examine Chaos Test Assessment**: Understand current chaos test status
   - `CHAOS_TESTS_STATUS.md`

3. **Integrate with Automated Testing**: Add chaos testing to your development workflow

4. **Expand Scenarios**: Create custom chaos scenarios for your specific use cases

5. **Monitor Production**: Use lessons learned to improve production resilience

---

## Contributing

To add new chaos scenarios:

1. Create injection function in `tier1-scenarios.sh` (in the "Chaos Injection Functions" section)
2. Create a new `scenario_N_*()` function following the existing pattern
3. Add the scenario to the CLI case statement in `main()`
4. Update `show_usage()` to document the new scenario
5. Document the scenario in this README
6. Test thoroughly before committing

---

## Support

- Issues: [GitHub Issues](https://github.com/anduroproject/alys/issues)
- Documentation: `docs/v2_alpha/`
- Testing Guide: `V2_MASTER_TESTING_GUIDE.knowledge.md`

---

**Version:** 2.0
**Last Updated:** January 2026
