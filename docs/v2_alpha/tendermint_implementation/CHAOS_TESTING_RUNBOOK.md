# Tendermint Chaos Testing Operational Runbook

This runbook provides operational procedures for running Tendermint chaos tests in CI/CD and manual testing environments.

---

## Table of Contents

1. [Environment Setup](#1-environment-setup)
2. [Pre-Flight Checklist](#2-pre-flight-checklist)
3. [Test Execution Procedures](#3-test-execution-procedures)
4. [Monitoring During Tests](#4-monitoring-during-tests)
5. [Post-Test Procedures](#5-post-test-procedures)
6. [Incident Response](#6-incident-response)
7. [Maintenance Tasks](#7-maintenance-tasks)

---

## 1. Environment Setup

### 1.1 Local Development Environment

**Required Software:**
```bash
# macOS
brew install docker jq bc

# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y docker.io docker-compose-v2 jq bc
```

**Verify Installation:**
```bash
docker --version        # Docker 24.0+
docker compose version  # Compose v2.0+
jq --version           # jq 1.6+
```

### 1.2 Start the Testnet

```bash
cd /path/to/alys-v2/etc

# Pull latest images
docker compose -f docker-compose.tendermint-3node.yml pull

# Start all services
docker compose -f docker-compose.tendermint-3node.yml up -d

# Verify containers are running
docker compose -f docker-compose.tendermint-3node.yml ps
```

**Expected Output:**
```
NAME           IMAGE                    STATUS         PORTS
alys-node-1    ghcr.io/.../alys:...    Up 2 minutes   0.0.0.0:3001->3001/tcp
alys-node-2    ghcr.io/.../alys:...    Up 2 minutes   0.0.0.0:3011->3001/tcp
alys-node-3    ghcr.io/.../alys:...    Up 2 minutes   0.0.0.0:3021->3001/tcp
execution      ghcr.io/.../reth:...    Up 2 minutes   0.0.0.0:8545->8545/tcp
bitcoin-core   ...                      Up 2 minutes   0.0.0.0:18443->18443/tcp
prometheus     prom/prometheus:...      Up 2 minutes   0.0.0.0:9093->9090/tcp
grafana        grafana/grafana:...      Up 2 minutes   0.0.0.0:3030->3000/tcp
```

### 1.3 Wait for Consensus Stability

```bash
cd chaos-testing
./wait-for-consensus.sh --timeout 180 --min-blocks 10 --verbose
```

**Success Indicators:**
- All 3 validator containers running
- All RPC endpoints responding
- Blocks being produced every ~3 seconds
- All nodes at same height (within 2 blocks)

---

## 2. Pre-Flight Checklist

Before running chaos tests, verify:

| Check | Command | Expected |
|-------|---------|----------|
| Containers running | `docker ps --filter "name=alys-node-" \| wc -l` | 3 |
| RPC responding | `curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' \| jq .result.height` | > 0 |
| Height advancing | Run twice with 5s gap, compare heights | Height increased |
| Nodes synced | Query all 3 nodes, compare heights | Within 2 blocks |
| Execution layer | `docker logs execution --tail 10` | No errors |
| Disk space | `df -h` | > 10GB free |

**Quick Pre-Flight Script:**
```bash
#!/bin/bash
# Save as: pre-flight-check.sh

echo "=== Pre-Flight Checklist ==="

# Check containers
CONTAINERS=$(docker ps --filter "name=alys-node-" --format '{{.Names}}' | wc -l | tr -d ' ')
echo "Validators running: $CONTAINERS/3"

# Check RPC
for port in 3001 3011 3021; do
    HEIGHT=$(curl -s localhost:$port -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' | jq -r '.result.height // 0')
    echo "Port $port height: $HEIGHT"
done

# Check execution
if docker logs execution --tail 1 2>&1 | grep -qi error; then
    echo "WARNING: Execution layer has errors"
else
    echo "Execution layer: OK"
fi

echo "=== Pre-Flight Complete ==="
```

---

## 3. Test Execution Procedures

### 3.1 Smoke Tests (5 min)

**Purpose:** Verify basic consensus works before extended testing.

```bash
cd /path/to/alys-v2/etc/chaos-testing
./tendermint-chaos.sh --scenario TM-L3
```

**Success Criteria:**
- Test passes
- Height advances during test
- No errors in logs

### 3.2 Tier 1 Tests (30 min)

**Purpose:** Core BFT scenarios for daily validation.

```bash
./tendermint-chaos.sh --scenario tier1 --verbose
```

**Scenarios Included:**
- TM-A1: Single Validator Crash
- TM-A3: WAL Recovery
- TM-B1: Node Isolation
- TM-B5: Partition Healing
- TM-L3: Height Progression

### 3.3 Tier 2 Tests (2 hours)

**Purpose:** Extended scenarios for weekly validation.

```bash
./tendermint-chaos.sh --scenario tier2 --verbose
```

**Scenarios Included:**
- TM-D1-D3: Crash recovery scenarios
- TM-E5: Equivocation prevention
- TM-F1-F2: External dependency failures

### 3.4 Full Suite (60 min)

**Purpose:** Complete validation before releases.

```bash
./tendermint-chaos.sh --scenario all --verbose --output-dir ./results-$(date +%Y%m%d)
```

### 3.5 Category-Specific Tests

| Category | Command | Duration |
|----------|---------|----------|
| Validator failures | `--scenario validator` | ~15 min |
| Network partitions | `--scenario network` | ~15 min |
| Timing issues | `--scenario timing` | ~10 min |
| WAL recovery | `--scenario wal` | ~15 min |
| External deps | `--scenario external` | ~10 min |
| Liveness | `--scenario liveness` | ~10 min |

---

## 4. Monitoring During Tests

### 4.1 Grafana Dashboard

**URL:** http://localhost:3030 (admin/admin)

**Key Panels to Watch:**
- Consensus height (all validators)
- Round number (should be 0 for healthy blocks)
- Vote collection latency
- Network message rates

### 4.2 Prometheus Queries

**URL:** http://localhost:9093

**Useful Queries:**
```promql
# Current height by validator
tendermint_consensus_height

# Rounds per height (should be ~1)
rate(tendermint_consensus_rounds_total[5m])

# Message latency
histogram_quantile(0.99, tendermint_p2p_message_latency_seconds_bucket)
```

### 4.3 Live Log Monitoring

```bash
# All validator logs (in separate terminals)
docker logs -f alys-node-1
docker logs -f alys-node-2
docker logs -f alys-node-3

# Filter for important events
docker logs -f alys-node-1 2>&1 | grep -E "(height|round|commit|error)"
```

### 4.4 Real-Time Consensus State

```bash
# Poll consensus state every 2 seconds
watch -n 2 'curl -s localhost:3001 -d "{\"jsonrpc\":\"2.0\",\"method\":\"tendermint_consensusState\",\"params\":[],\"id\":1}" | jq .result'
```

---

## 5. Post-Test Procedures

### 5.1 Collect Results

```bash
mkdir -p ./test-results/$(date +%Y%m%d)
cd ./test-results/$(date +%Y%m%d)

# Copy test output
cp /path/to/chaos-results/* .

# Collect logs
for node in alys-node-1 alys-node-2 alys-node-3 execution bitcoin-core; do
    docker logs "$node" > "${node}.log" 2>&1
done

# Collect metrics snapshot
curl -s localhost:9093/api/v1/query?query=tendermint_consensus_height > metrics.json
```

### 5.2 Generate Summary

```bash
# Extract test results
grep -E "^\[(PASS|FAIL)\]" output.log > summary.txt

# Count results
PASSED=$(grep -c "PASS" summary.txt || echo 0)
FAILED=$(grep -c "FAIL" summary.txt || echo 0)
echo "Total: $((PASSED + FAILED)) | Passed: $PASSED | Failed: $FAILED"
```

### 5.3 Cleanup

```bash
cd /path/to/alys-v2/etc

# Stop and remove containers
docker compose -f docker-compose.tendermint-3node.yml down

# Remove volumes (clean state for next run)
docker compose -f docker-compose.tendermint-3node.yml down -v

# Prune unused resources (optional)
docker system prune -f
```

### 5.4 Archive Results

```bash
# Create archive
tar -czvf chaos-results-$(date +%Y%m%d).tar.gz ./test-results/$(date +%Y%m%d)

# Upload to artifact storage (CI example)
# aws s3 cp chaos-results-*.tar.gz s3://artifacts/chaos-tests/
```

---

## 6. Incident Response

### 6.1 Test Failure Triage

| Symptom | Likely Cause | Resolution |
|---------|--------------|------------|
| "Not all validators active" | Containers crashed | `docker compose up -d` |
| "Consensus halted" unexpectedly | n=3 requires 100% uptime | Check if any node stopped |
| "Timeout waiting for height" | Execution layer issue | Check `docker logs execution` |
| "WAL recovery failed" | Disk space or permissions | Check disk, container mounts |
| "Evidence detected" (unexpected) | Bug in consensus | Investigate logs, report bug |

### 6.2 Debug Information Collection

When a test fails, collect:

```bash
# 1. Container states
docker ps -a --filter "name=alys" > container-states.txt

# 2. Full logs (last 1000 lines)
for node in alys-node-1 alys-node-2 alys-node-3; do
    docker logs --tail 1000 "$node" > "${node}-debug.log" 2>&1
done

# 3. Consensus state at failure
for port in 3001 3011 3021; do
    curl -s localhost:$port -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}' > "consensus-$port.json"
done

# 4. Evidence (if any)
curl -s localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_evidence","params":[],"id":1}' > evidence.json

# 5. Docker events
docker events --since 10m --until 0s --filter "container=alys" > docker-events.txt
```

### 6.3 Escalation

1. **Severity 1 (Safety violation):** Evidence detected, fork observed
   - Immediate escalation to consensus team
   - Preserve all logs and state
   - Do not restart containers

2. **Severity 2 (Liveness issue):** Consensus halted unexpectedly
   - Check external dependencies
   - Review recent code changes
   - Escalate if not resolved in 30 minutes

3. **Severity 3 (Test flakiness):** Intermittent failures
   - Add retry logic if appropriate
   - Review timing assumptions
   - Track flakiness rate

---

## 7. Maintenance Tasks

### 7.1 Weekly Tasks

- [ ] Run full test suite (`--scenario all`)
- [ ] Review and archive test results
- [ ] Check for new scenarios in plan document
- [ ] Update Docker images if needed

### 7.2 Monthly Tasks

- [ ] Review test coverage against plan
- [ ] Update documentation if procedures changed
- [ ] Clean up old test artifacts
- [ ] Review and update timeouts/thresholds

### 7.3 Quarterly Tasks

- [ ] Full documentation review
- [ ] Test framework code review
- [ ] Performance baseline update
- [ ] CI/CD pipeline optimization

---

## Appendix A: RPC Reference

| Method | Purpose | Example |
|--------|---------|---------|
| `tendermint_consensusState` | Current height/round/step | `curl localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_consensusState","params":[],"id":1}'` |
| `tendermint_validators` | Validator set | `curl localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_validators","params":[],"id":1}'` |
| `tendermint_commit` | Commit proof for height | `curl localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_commit","params":[{"height":100}],"id":1}'` |
| `tendermint_evidence` | Detected equivocation | `curl localhost:3001 -d '{"jsonrpc":"2.0","method":"tendermint_evidence","params":[],"id":1}'` |

---

## Appendix B: Docker Commands Reference

| Action | Command |
|--------|---------|
| Start testnet | `docker compose -f docker-compose.tendermint-3node.yml up -d` |
| Stop testnet | `docker compose -f docker-compose.tendermint-3node.yml down` |
| Stop with cleanup | `docker compose -f docker-compose.tendermint-3node.yml down -v` |
| View logs | `docker logs -f alys-node-1` |
| Restart node | `docker restart alys-node-2` |
| Stop node | `docker stop alys-node-2` |
| Crash node | `docker kill alys-node-2` |
| Isolate node | `docker network disconnect alys-tendermint-3 alys-node-2` |
| Reconnect node | `docker network connect alys-tendermint-3 alys-node-2` |
| Add latency | `docker exec alys-node-2 tc qdisc add dev eth0 root netem delay 500ms` |
| Remove latency | `docker exec alys-node-2 tc qdisc del dev eth0 root` |
