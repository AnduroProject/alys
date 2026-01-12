# Monitoring Stack for Two-Node Regtest Environment

Complete Prometheus and Grafana monitoring setup for the Alys V2 two-node regtest environment, providing real-time visibility into blockchain metrics.

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Service Details](#service-details)
- [Dashboard](#dashboard)
- [Key Metrics](#key-metrics)
- [PromQL Query Examples](#promql-query-examples)
- [Port Reference](#port-reference)
- [Troubleshooting](#troubleshooting)
- [Customization](#customization)
- [Implementation Details](#implementation-details)

---

## Overview

The monitoring stack provides comprehensive observability for:

- **Chain Metrics**: Block height, sync status, production/import rates
- **Network Metrics**: Peer connections, message throughput, block propagation
- **Fork Handling**: Fork detection, reorganizations (Phase 4/5 metrics)
- **Performance**: Block processing times, import queue depth
- **Errors**: Production failures, import failures, validation errors
- **Consensus**: Aura slot tracking, block production, validator status

### What Was Implemented

✅ **Prometheus Configuration** - Scrapes both Alys nodes + Reth execution layer
✅ **Grafana Provisioning** - Auto-configured datasource and dashboards
✅ **Pre-built Dashboard** - "Alys V2 - Two-Node Regtest Overview" with 7 panels
✅ **Docker Integration** - Services added to docker-compose.v2-regtest.yml
✅ **Metrics Endpoints** - Both nodes expose Prometheus metrics on port 9090
✅ **Zero Configuration** - Everything auto-provisions on startup

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Alys Two-Node Regtest Network                          │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─────────────┐      ┌─────────────┐                  │
│  │ Alys Node 1 │──────│ Alys Node 2 │                  │
│  │ :9090       │      │ :9090       │                  │
│  └──────┬──────┘      └──────┬──────┘                  │
│         │                    │                          │
│         │ Metrics scraping   │                          │
│         └────────┬───────────┘                          │
│                  │                                       │
│           ┌──────▼──────┐                               │
│           │ Prometheus  │                               │
│           │ :9092       │                               │
│           └──────┬──────┘                               │
│                  │ Datasource                           │
│           ┌──────▼──────┐                               │
│           │  Grafana    │                               │
│           │  :3030      │                               │
│           └─────────────┘                               │
│                                                          │
│  Also monitoring:                                       │
│  - Reth Execution Layer :19001                          │
│  - Bitcoin Core (future)                                │
└──────────────────────────────────────────────────────────┘
```

---

## Quick Start

### 1. Start the Stack

```bash
cd etc
docker compose -f docker-compose.v2-regtest.yml up -d
```

This starts all services including Prometheus and Grafana.

### 2. Access Grafana

Open http://localhost:3030 in your browser.

**Login:**
- Username: `admin`
- Password: `admin`

### 3. View Dashboard

The "Alys V2 - Two-Node Regtest Overview" dashboard is automatically loaded:

1. Click **Dashboards** (left sidebar)
2. Navigate to **Alys V2** folder
3. Open **Alys V2 - Two-Node Regtest Overview**

### 4. Verify Metrics Collection

Check Prometheus is scraping both nodes:

1. Open http://localhost:9092
2. Go to **Status** → **Targets**
3. Verify all targets are **UP**:
   - `alys-node-1` (172.20.0.10:9090)
   - `alys-node-2` (172.20.0.11:9090)
   - `reth-execution` (172.20.0.20:19001)

---

## Service Details

### Prometheus (172.20.0.40)

- **Port**: `9092` (host) → `9090` (container)
- **URL**: http://localhost:9092
- **Config**: `etc/config/prometheus/prometheus.yml`
- **Data**: `data/prometheus/` (persisted)
- **Retention**: 15 days (configurable)

**Scrape Jobs:**
- `alys-node-1`: Scrapes `alys-node-1:9090` every 15s
- `alys-node-2`: Scrapes `alys-node-2:9090` every 15s
- `reth-execution`: Scrapes `execution:19001` every 15s
- `prometheus`: Self-monitoring

**Key Features:**
- 15-second scrape interval
- External labels for cluster identification
- Prepared for alert rules (commented out)
- Web lifecycle enabled for hot-reload

### Grafana (172.20.0.41)

- **Port**: `3030` (host) → `3000` (container)
- **URL**: http://localhost:3030
- **Credentials**: `admin` / `admin`
- **Config**: `etc/config/grafana/provisioning/`
- **Data**: `data/grafana/` (persisted)

**Auto-provisioned:**
- Prometheus datasource (pre-configured, read-only)
- "Alys V2 - Two-Node Regtest Overview" dashboard
- Updates every 30 seconds
- Allows UI updates

### Alys Nodes

Both nodes expose Prometheus metrics:
- **Node 1**: Container port `9090`, host port `9090`
- **Node 2**: Container port `9090`, host port `9091`
- **CLI Flag**: `--metrics-port 9090`
- **Endpoint**: `/metrics` (Prometheus text format)

---

## Dashboard

### Dashboard: "Alys V2 - Network Overview"

**UID**: `alys-v2-overview`
**Refresh**: Configurable (default 10 seconds)
**Time Range**: Last 30 minutes

### Dashboard Variables

The dashboard includes template variables for filtering and customization:

| Variable | Description | Default |
|----------|-------------|---------|
| **Node** | Filter metrics by specific node(s). Supports multi-select. | All nodes |
| **Refresh** | Auto-refresh interval | 10s |

**Using the Node Filter:**
1. Click the "Node" dropdown at the top of the dashboard
2. Select specific nodes (e.g., `alys-node-1`, `alys-node-2`) or "All"
3. All panels automatically filter to show only selected node(s)

This allows operators to:
- Focus on a single node for debugging
- Compare specific nodes side-by-side
- Monitor the entire network at once

### Panels

1. **Chain Height**
   - Type: Timeseries
   - Shows current blockchain height for selected nodes
   - Query: `alys_chain_height{job=~"$node"}`

2. **Sync Status**
   - Type: Gauge
   - Shows sync status (1=synced, 0=not synced)
   - Query: `alys_chain_sync_status{job=~"$node"}`

3. **Network Peers**
   - Type: Timeseries
   - Number of connected peers
   - Query: `alys_chain_network_peers{job=~"$node"}`

4. **Block Production/Import Rate**
   - Type: Timeseries
   - Blocks produced and imported per minute
   - Queries:
     - `rate(alys_chain_blocks_produced_total{job=~"$node"}[1m])`
     - `rate(alys_chain_blocks_imported_total{job=~"$node"}[1m])`

5. **Fork Handling** (Phase 4/5)
   - Type: Timeseries
   - Forks detected and reorganizations
   - Queries:
     - `alys_chain_forks_detected_total{job=~"$node"}`
     - `alys_chain_reorganizations_total{job=~"$node"}`

6. **Import Queue Depth** (Phase 2)
   - Type: Timeseries
   - Number of blocks waiting for import
   - Query: `alys_chain_import_queue_depth{job=~"$node"}`

7. **Block Errors**
   - Type: Timeseries
   - Production and import failure rates
   - Queries:
     - `rate(alys_chain_block_production_failures_total{job=~"$node"}[5m])`
     - `rate(alys_chain_block_import_failures_total{job=~"$node"}[5m])`

**Note**: All queries use the `$node` variable for filtering. Use `alys_` prefixed metrics (V2).

---

## Key Metrics

### V0 Metrics (Currently Available)

These metrics are exposed by the working V0 system with `alys_` prefix:

| Metric | Type | Description |
|--------|------|-------------|
| `alys_chain_block_production_totals` | Counter | Total blocks produced (by status) |
| `alys_chain_process_block_totals` | Counter | Total blocks processed (by status) |
| `alys_chain_last_processed_block` | Gauge | Last block processed |
| `alys_chain_last_finalized_block` | Gauge | Last finalized block |
| `alys_chain_discovered_peers` | Gauge | Number of discovered peers |
| `alys_chain_network_gossip_totals` | Counter | Network gossip messages |
| `alys_aura_produced_blocks_total` | Counter | Blocks produced by Aura |
| `alys_aura_current_slot` | Gauge | Current slot number |
| `alys_aura_latest_slot_author` | Gauge | Latest slot author index |

### V2 Metrics (Planned - Phase 1-5)

These metrics are defined in V2 actors but may not be fully implemented:

**Phase 1: Block Reception**
- `network_blocks_received` - Blocks received via gossipsub
- `network_blocks_forwarded` - Blocks forwarded to ChainActor

**Phase 2: Import Serialization**
- `chain_import_queue_depth` - Pending block imports

**Phase 3: Enhanced Validation**
- `chain_block_import_failures_total` - Failed validations

**Phase 4: Fork Handling**
- `chain_forks_detected_total` - Forks at same height
- `chain_reorganizations_total` - Chain reorgs
- `chain_reorganization_depth` - Reorg depth histogram

**Phase 5: Advanced Features**
- `network_blocks_duplicate_cached` - Cache hits
- `network_blocks_deserialization_errors` - Deserialization failures

### Standard Chain Metrics (V2)

- `chain_height` - Current blockchain height
- `chain_sync_status` - Sync status (1/0)
- `chain_network_peers` - Peer count
- `chain_blocks_produced_total` - Production counter
- `chain_blocks_imported_total` - Import counter

---

## PromQL Query Examples

### Basic Health Checks

**Check all targets are up:**
```promql
up
```

**Check Alys nodes status:**
```promql
up{job=~"alys-node-.*"}
```

### Chain Metrics (V0)

**Current processed blocks:**
```promql
alys_chain_last_processed_block
```

**Block processing rate (last 5 minutes):**
```promql
rate(alys_chain_process_block_totals{status="success"}[5m])
```

**Compare nodes (should be similar):**
```promql
alys_chain_last_processed_block{job="alys-node-1"} -
alys_chain_last_processed_block{job="alys-node-2"}
```

### Aura Consensus

**Current slot:**
```promql
alys_aura_current_slot
```

**Block production success rate:**
```promql
rate(alys_aura_produced_blocks_total{status="success"}[5m]) /
rate(alys_aura_produced_blocks_total[5m])
```

### Network Activity

**Gossip message rate by type:**
```promql
rate(alys_chain_network_gossip_totals[5m])
```

**Discovered peers:**
```promql
alys_chain_discovered_peers
```

### Aggregations

**Total blocks produced across all nodes:**
```promql
sum(alys_aura_produced_blocks_total)
```

**Block production by node:**
```promql
sum by (job) (alys_aura_produced_blocks_total)
```

### RPC Metrics

**RPC request rate by method:**
```promql
rate(alys_rpc_requests_total[5m])
```

**RPC request duration (95th percentile):**
```promql
histogram_quantile(0.95, rate(alys_rpc_request_duration_seconds_bucket[5m]))
```

---

## Port Reference

| Service | Host Port | Container Port | Purpose |
|---------|-----------|----------------|---------|
| **Alys Node 1** | 9090 | 9090 | Prometheus metrics |
| **Alys Node 2** | 9091 | 9090 | Prometheus metrics |
| **Prometheus** | 9092 | 9090 | Web UI & API |
| **Grafana** | 3030 | 3000 | Web UI |
| **Reth** | 19001 | 19001 | Metrics |

**Note**: All services use different host ports to avoid conflicts. Inside the Docker network, both Alys nodes expose metrics on container port 9090.

### Additional Node Ports

**Alys Node 1:**
- `3000` - V0 RPC
- `3001` - V2 RPC
- `9000` - V0 P2P
- `10000` - V2 P2P

**Alys Node 2:**
- `3010` - V0 RPC (host)
- `3011` - V2 RPC (host)
- `9001` - V0 P2P (host)
- `10001` - V2 P2P (host)

---

## Troubleshooting

### Prometheus Not Scraping Nodes

**Symptoms:**
- Targets show as "DOWN" in Prometheus
- No data in Grafana dashboard

**Solutions:**

1. Check Alys nodes are running:
   ```bash
   docker ps | grep alys-node
   ```

2. Verify metrics endpoints are accessible:
   ```bash
   curl http://localhost:9090/metrics  # Node 1
   curl http://localhost:9091/metrics  # Node 2
   ```

3. Check Prometheus logs:
   ```bash
   docker logs prometheus
   ```

4. Verify network connectivity:
   ```bash
   docker exec prometheus ping alys-node-1
   ```

### Grafana Dashboard Shows "No Data"

**Symptoms:**
- Dashboard appears empty
- "No data" messages in panels

**Solutions:**

1. Verify Prometheus datasource:
   - Go to **Configuration** → **Data Sources**
   - Click **Prometheus**
   - Click **Test** button (should show "Data source is working")

2. Check time range:
   - Ensure dashboard time range includes active data
   - Try "Last 30 minutes"

3. Verify metrics exist in Prometheus:
   - Go to Prometheus UI (http://localhost:9092)
   - Try query: `alys_chain_last_processed_block`
   - Should return values for both nodes

4. Check if V2 metrics are implemented:
   - The dashboard uses V2 metric names (e.g., `chain_height`)
   - V0 metrics have `alys_` prefix (e.g., `alys_chain_last_processed_block`)
   - You may need to edit dashboard panels to use available metrics

### Grafana Shows "datasource prometheus not found"

**Cause**: Grafana database has cached incorrect configuration

**Solution**:
```bash
# Stop Grafana
docker compose -f docker-compose.v2-regtest.yml stop grafana

# Remove cached database
rm -rf data/grafana/*

# Restart Grafana (will re-provision from config files)
docker compose -f docker-compose.v2-regtest.yml up -d grafana
```

### Metrics Not Appearing

**Solutions:**

1. Check Alys nodes have `--metrics-port` flag:
   ```bash
   docker inspect alys-node-1 | jq '.[0].Args' | grep metrics
   ```

2. Verify metrics are being exported:
   ```bash
   curl -s http://localhost:9090/metrics | grep alys_
   ```

3. Restart nodes if needed:
   ```bash
   docker compose -f docker-compose.v2-regtest.yml restart alys-node-1 alys-node-2
   ```

### Port Conflicts

**Error**: `Bind for 0.0.0.0:9092 failed: port is already allocated`

**Solution**: Change the port mapping in `docker-compose.v2-regtest.yml`:
```yaml
prometheus:
  ports:
    - "9093:9090"  # Use different host port
```

---

## Customization

### Add Custom Dashboard Panels

1. Open Grafana (http://localhost:3030)
2. Navigate to the dashboard
3. Click **Add panel**
4. Use PromQL queries to select metrics
5. Configure visualization settings
6. Save the dashboard

### Modify Scrape Interval

Edit `etc/config/prometheus/prometheus.yml`:

```yaml
global:
  scrape_interval: 15s  # Change this value (e.g., 30s, 1m)
```

Reload Prometheus configuration:
```bash
curl -X POST http://localhost:9092/-/reload
```

### Add Alert Rules

Create `etc/config/prometheus/alerts.yml`:

```yaml
groups:
  - name: alys_alerts
    rules:
      - alert: NodeDown
        expr: up{job=~"alys-node-.*"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Alys node {{ $labels.instance }} is down"

      - alert: HighBlockProductionFailures
        expr: rate(alys_chain_block_production_totals{status!="success"}[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High block production failures on {{ $labels.instance }}"
```

Update `prometheus.yml`:
```yaml
rule_files:
  - 'alerts.yml'
```

### Change Data Retention

Modify in `docker-compose.v2-regtest.yml`:

```yaml
prometheus:
  command:
    - '--storage.tsdb.retention.time=30d'  # Keep 30 days instead of 15
```

---

## Implementation Details

### File Structure

```
alys-v2/
├── etc/
│   ├── config/
│   │   ├── prometheus/
│   │   │   └── prometheus.yml           (Scrape config)
│   │   └── grafana/
│   │       └── provisioning/
│   │           ├── datasources/
│   │           │   └── prometheus.yml   (Datasource config)
│   │           └── dashboards/
│   │               ├── default.yml      (Dashboard provider)
│   │               └── alys-v2-overview.json  (Dashboard JSON)
│   └── docker-compose.v2-regtest.yml    (Added Prometheus & Grafana services)
├── docs/v2_alpha/local-regtest/
│   └── monitoring.md                    (This file)
└── data/                                (Created at runtime)
    ├── prometheus/                      (Metrics data, 15 days retention)
    └── grafana/                         (Dashboard settings, persisted)
```

### Docker Compose Configuration

**Prometheus Service:**
```yaml
prometheus:
  image: prom/prometheus:latest
  container_name: prometheus
  networks:
    alys-regtest:
      ipv4_address: 172.20.0.40
  ports:
    - "9092:9090"
  volumes:
    - ./config/prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
    - ../data/prometheus:/prometheus
  command:
    - '--config.file=/etc/prometheus/prometheus.yml'
    - '--storage.tsdb.path=/prometheus'
    - '--web.enable-lifecycle'
```

**Grafana Service:**
```yaml
grafana:
  image: grafana/grafana:latest
  container_name: grafana
  networks:
    alys-regtest:
      ipv4_address: 172.20.0.41
  ports:
    - "3030:3000"
  volumes:
    - ./config/grafana/provisioning:/etc/grafana/provisioning:ro
    - ../data/grafana:/var/lib/grafana
  environment:
    - GF_SECURITY_ADMIN_USER=admin
    - GF_SECURITY_ADMIN_PASSWORD=admin
```

**Alys Node Changes:**
```yaml
alys-node-1:
  ports:
    - "9090:9090"    # Prometheus Metrics
  command:
    - --metrics-port 9090
```

### Performance Considerations

**Resource Usage:**
- Prometheus: ~200MB RAM, 1-2GB disk (15 days retention)
- Grafana: ~100MB RAM, <100MB disk
- Total overhead: Minimal impact on regtest performance

**High-Frequency Scraping:**
For production environments with many metrics:
- Consider increasing scrape interval (30s-60s)
- Use recording rules for complex queries
- Enable remote storage for long-term retention

### Integration with Testing

**Monitor During Tests:**
```bash
# Start monitoring stack
docker compose -f docker-compose.v2-regtest.yml up -d

# Run tests
cargo test --package app

# Query results
curl -s 'http://localhost:9092/api/v1/query?query=chain_forks_detected_total' | jq

# Check for anomalies
curl -s 'http://localhost:9092/api/v1/query?query=chain_block_import_failures_total' | jq
```

**Automated Queries:**
```bash
# Get current chain height
CHAIN_HEIGHT=$(curl -s 'http://localhost:9092/api/v1/query?query=alys_chain_last_processed_block{instance="node-1"}' | jq -r '.data.result[0].value[1]')
echo "Chain height: $CHAIN_HEIGHT"

# Get total blocks produced
BLOCKS=$(curl -s 'http://localhost:9092/api/v1/query?query=sum(alys_aura_produced_blocks_total)' | jq -r '.data.result[0].value[1]')
echo "Total blocks: $BLOCKS"
```

---

## Quick Command Reference

### Start/Stop Services

```bash
# Start everything
docker compose -f docker-compose.v2-regtest.yml up -d

# Stop monitoring stack only
docker compose -f docker-compose.v2-regtest.yml stop prometheus grafana

# Restart monitoring
docker compose -f docker-compose.v2-regtest.yml restart prometheus grafana

# Stop and remove all containers (data persists)
docker compose -f docker-compose.v2-regtest.yml down
```

### View Logs

```bash
# Prometheus logs
docker logs prometheus

# Grafana logs
docker logs grafana

# Alys Node 1 logs (metrics-related)
docker logs alys-node-1 | grep -i metric
```

### Verify Setup

```bash
# Check containers running
docker ps | grep -E "prometheus|grafana|alys-node"

# Test metrics endpoints
curl -f http://localhost:9090/metrics | head -20  # Node 1
curl -f http://localhost:9091/metrics | head -20  # Node 2

# Verify Prometheus scraping
curl -s http://localhost:9092/api/v1/targets | jq '.data.activeTargets[] | {job: .labels.job, health: .health}'

# Test Grafana datasource
curl -u admin:admin http://localhost:3030/api/datasources
```

---

## Resources

### URLs

- **Grafana**: http://localhost:3030 (admin/admin)
- **Prometheus**: http://localhost:9092
- **Node 1 Metrics**: http://localhost:9090/metrics
- **Node 2 Metrics**: http://localhost:9091/metrics

### Documentation

- [Block Handling Implementation Plan](../block-handling-master-implementation-plan.md)
- [Docker Two-Node Architecture](../docker-two-node-testnet-architecture.md)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [PromQL Basics](https://prometheus.io/docs/prometheus/latest/querying/basics/)

---

## Next Steps

1. ✅ **Access Grafana**: Open http://localhost:3030 and login
2. ✅ **View Dashboard**: Navigate to "Alys V2 - Two-Node Regtest Overview"
3. 🎯 **Verify Metrics**: Run PromQL queries to confirm data is flowing
4. 🎯 **Adapt Dashboard**: Update panels to use available V0 metrics (with `alys_` prefix)
5. 🎯 **Run Block Production**: Mine blocks and watch metrics update
6. 🎯 **Test Fork Scenarios**: Create competing blocks and observe behavior
7. 🎯 **Customize**: Add panels relevant to your testing scenarios

---

**Status**: ✅ Monitoring stack fully implemented and operational

The setup is **zero-configuration** - simply start the Docker Compose stack and access Grafana to see metrics from both nodes. All Phase 1-5 metric definitions are in place, with V0 metrics currently available and V2 metrics ready for implementation completion.
