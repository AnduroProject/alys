# ALYS-016: Production Deployment and Monitoring

## Issue Type
Epic

## Priority
Critical

## Story Points
13

## Sprint
Migration Sprint 8

## Component
Deployment

## Labels
`migration`, `phase-8`, `production`, `deployment`, `monitoring`

## Description

Execute the production deployment of the fully migrated Alys v2 system. This includes deploying to all production nodes, setting up comprehensive monitoring, establishing operational procedures, and ensuring system stability under production load.

## Acceptance Criteria

- [ ] All production nodes successfully deployed
- [ ] Zero downtime during deployment
- [ ] Monitoring dashboards fully operational
- [ ] Alert rules configured and tested
- [ ] Performance meets or exceeds baseline
- [ ] Rollback procedures validated
- [ ] Operational runbooks complete
- [ ] 99.9% uptime achieved in first week

## Technical Details

### Implementation Steps

1. **Production Deployment Script**
```bash
#!/bin/bash
# scripts/deploy_production.sh

set -euo pipefail

# Configuration
readonly DEPLOYMENT_ENV="production"
readonly DEPLOYMENT_VERSION=$(git describe --tags --always)
readonly DEPLOYMENT_DATE=$(date -u +"%Y-%m-%d %H:%M:%S UTC")
readonly NODES_FILE="etc/production/nodes.txt"
readonly ROLLBACK_DIR="/var/backups/alys/rollback"

# Color codes for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Pre-deployment checks
pre_deployment_checks() {
    log_info "Running pre-deployment checks..."
    
    # Check if all tests pass
    log_info "Running test suite..."
    if ! cargo test --release --quiet; then
        log_error "Tests failed. Aborting deployment."
        exit 1
    fi
    
    # Check if build succeeds
    log_info "Building release binary..."
    if ! cargo build --release; then
        log_error "Build failed. Aborting deployment."
        exit 1
    fi
    
    # Verify configuration files
    log_info "Validating configuration files..."
    for config in etc/config/*.json; do
        if ! jq empty "$config" 2>/dev/null; then
            log_error "Invalid JSON in $config"
            exit 1
        fi
    done
    
    # Check disk space on all nodes
    log_info "Checking disk space on production nodes..."
    while IFS= read -r node; do
        available=$(ssh "$node" "df -BG /var/lib/alys | awk 'NR==2 {print \$4}' | sed 's/G//'")
        if [ "$available" -lt 50 ]; then
            log_error "Insufficient disk space on $node: ${available}GB"
            exit 1
        fi
    done < "$NODES_FILE"
    
    log_info "✅ All pre-deployment checks passed"
}

# Create deployment backup
create_backup() {
    local node=$1
    log_info "Creating backup on $node..."
    
    ssh "$node" "mkdir -p $ROLLBACK_DIR"
    ssh "$node" "cp -r /opt/alys $ROLLBACK_DIR/alys-$(date +%Y%m%d-%H%M%S)"
    ssh "$node" "ln -sfn $ROLLBACK_DIR/alys-$(date +%Y%m%d-%H%M%S) $ROLLBACK_DIR/latest"
    
    log_info "Backup created on $node"
}

# Deploy to single node
deploy_node() {
    local node=$1
    local is_first=$2
    
    log_info "Deploying to $node..."
    
    # Create backup
    create_backup "$node"
    
    # Copy new binary and configs
    scp target/release/alys "$node:/opt/alys/bin/alys.new"
    scp -r etc/config/* "$node:/opt/alys/config/"
    
    # Atomic binary swap
    ssh "$node" "mv /opt/alys/bin/alys.new /opt/alys/bin/alys"
    
    # Restart service with grace period
    if [ "$is_first" = "true" ]; then
        # For first node, use longer grace period
        ssh "$node" "systemctl reload-or-restart alys --grace-period=60s"
    else
        # For subsequent nodes, shorter grace period
        ssh "$node" "systemctl reload-or-restart alys --grace-period=30s"
    fi
    
    # Wait for service to be healthy
    log_info "Waiting for $node to be healthy..."
    for i in {1..30}; do
        if ssh "$node" "curl -sf http://localhost:8545/health" > /dev/null; then
            log_info "✅ $node is healthy"
            return 0
        fi
        sleep 2
    done
    
    log_error "❌ $node failed health check"
    return 1
}

# Rolling deployment
rolling_deployment() {
    log_info "Starting rolling deployment to production..."
    
    local first_node=true
    local deployed_nodes=()
    
    while IFS= read -r node; do
        if deploy_node "$node" "$first_node"; then
            deployed_nodes+=("$node")
            first_node=false
            
            # Wait between deployments for stability
            if [ "$first_node" = "false" ]; then
                log_info "Waiting 60 seconds before next deployment..."
                sleep 60
            fi
        else
            log_error "Deployment to $node failed"
            
            # Rollback deployed nodes
            log_warn "Rolling back deployed nodes..."
            for deployed in "${deployed_nodes[@]}"; do
                rollback_node "$deployed"
            done
            
            exit 1
        fi
    done < "$NODES_FILE"
    
    log_info "✅ Rolling deployment completed successfully"
}

# Rollback single node
rollback_node() {
    local node=$1
    log_warn "Rolling back $node..."
    
    ssh "$node" "cp -r $ROLLBACK_DIR/latest/* /opt/alys/"
    ssh "$node" "systemctl restart alys"
    
    log_info "Rollback completed on $node"
}

# Post-deployment validation
post_deployment_validation() {
    log_info "Running post-deployment validation..."
    
    # Check all nodes are running new version
    while IFS= read -r node; do
        version=$(ssh "$node" "/opt/alys/bin/alys --version" | awk '{print $2}')
        if [ "$version" != "$DEPLOYMENT_VERSION" ]; then
            log_error "$node running wrong version: $version"
            return 1
        fi
    done < "$NODES_FILE"
    
    # Run smoke tests
    log_info "Running smoke tests..."
    ./scripts/smoke_tests.sh
    
    # Check cluster consensus
    log_info "Checking cluster consensus..."
    ./scripts/check_consensus.sh
    
    log_info "✅ Post-deployment validation passed"
}

# Update deployment record
update_deployment_record() {
    cat >> deployments.log <<EOF
Date: $DEPLOYMENT_DATE
Version: $DEPLOYMENT_VERSION
Status: SUCCESS
Nodes: $(wc -l < "$NODES_FILE")
Duration: $SECONDS seconds
Operator: $(whoami)
---
EOF
}

# Main deployment flow
main() {
    log_info "=== Alys Production Deployment ==="
    log_info "Version: $DEPLOYMENT_VERSION"
    log_info "Date: $DEPLOYMENT_DATE"
    
    pre_deployment_checks
    rolling_deployment
    post_deployment_validation
    update_deployment_record
    
    log_info "=== Deployment Complete ==="
    log_info "🎉 Successfully deployed version $DEPLOYMENT_VERSION to production"
}

# Run with error handling
if ! main; then
    log_error "Deployment failed. Check logs for details."
    exit 1
fi
```

2. **Monitoring Stack Configuration**
```yaml
# monitoring/prometheus/config.yml

global:
  scrape_interval: 15s
  evaluation_interval: 15s

alerting:
  alertmanagers:
    - static_configs:
        - targets: ['alertmanager:9093']

rule_files:
  - 'alerts/*.yml'

scrape_configs:
  # Alys consensus metrics
  - job_name: 'alys-consensus'
    static_configs:
      - targets:
        - 'node1.alys:9090'
        - 'node2.alys:9090'
        - 'node3.alys:9090'
    relabel_configs:
      - source_labels: [__address__]
        target_label: instance
        regex: '([^:]+):.*'
        replacement: '${1}'

  # Lighthouse metrics
  - job_name: 'lighthouse'
    static_configs:
      - targets:
        - 'node1.alys:5054'
        - 'node2.alys:5054'
        - 'node3.alys:5054'

  # Actor system metrics
  - job_name: 'actor-system'
    static_configs:
      - targets:
        - 'node1.alys:9091'
        - 'node2.alys:9091'
        - 'node3.alys:9091'

  # Governance stream metrics
  - job_name: 'governance'
    static_configs:
      - targets:
        - 'node1.alys:9092'
        - 'node2.alys:9092'
        - 'node3.alys:9092'
```

3. **Alert Rules**
```yaml
# monitoring/prometheus/alerts/critical.yml

groups:
  - name: critical_alerts
    interval: 30s
    rules:
      # Consensus alerts
      - alert: ConsensusHalted
        expr: rate(alys_blocks_produced[5m]) == 0
        for: 2m
        labels:
          severity: critical
          component: consensus
        annotations:
          summary: "Block production halted"
          description: "No blocks produced in the last 2 minutes"

      - alert: ConsensusDesync
        expr: |
          stddev(alys_chain_height) > 2
        for: 5m
        labels:
          severity: critical
          component: consensus
        annotations:
          summary: "Nodes are desynced"
          description: "Chain height differs by more than 2 blocks across nodes"

      # Actor system alerts
      - alert: ActorMailboxOverflow
        expr: actor_mailbox_size > 10000
        for: 1m
        labels:
          severity: warning
          component: actors
        annotations:
          summary: "Actor mailbox overflow"
          description: "Actor {{ $labels.actor }} has {{ $value }} messages in mailbox"

      - alert: ActorPanics
        expr: rate(actor_panics_total[5m]) > 0
        for: 1m
        labels:
          severity: critical
          component: actors
        annotations:
          summary: "Actor panicking"
          description: "Actor {{ $labels.actor }} is panicking"

      # Governance alerts
      - alert: GovernanceDisconnected
        expr: governance_stream_connected == 0
        for: 5m
        labels:
          severity: critical
          component: governance
        annotations:
          summary: "Governance stream disconnected"
          description: "Node {{ $labels.instance }} disconnected from governance"

      - alert: SignatureMismatch
        expr: |
          rate(signature_mismatches_total[5m]) > 0.001
        for: 10m
        labels:
          severity: warning
          component: governance
        annotations:
          summary: "Signature validation mismatches"
          description: "Signature mismatch rate: {{ $value }}"

      # Performance alerts
      - alert: HighLatency
        expr: |
          histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m])) > 1
        for: 5m
        labels:
          severity: warning
          component: api
        annotations:
          summary: "High API latency"
          description: "P99 latency is {{ $value }}s"

      - alert: MemoryLeak
        expr: |
          rate(process_resident_memory_bytes[1h]) > 0
          and process_resident_memory_bytes > 8e9
        for: 30m
        labels:
          severity: warning
          component: system
        annotations:
          summary: "Possible memory leak"
          description: "Memory usage growing and exceeds 8GB"
```

4. **Grafana Dashboard Configuration**
```json
{
  "dashboard": {
    "title": "Alys V2 Production Dashboard",
    "panels": [
      {
        "title": "Block Production Rate",
        "targets": [
          {
            "expr": "rate(alys_blocks_produced[5m])",
            "legendFormat": "{{instance}}"
          }
        ],
        "gridPos": { "h": 8, "w": 12, "x": 0, "y": 0 }
      },
      {
        "title": "Actor System Health",
        "targets": [
          {
            "expr": "sum by (actor) (actor_mailbox_size)",
            "legendFormat": "{{actor}}"
          }
        ],
        "gridPos": { "h": 8, "w": 12, "x": 12, "y": 0 }
      },
      {
        "title": "Governance Connection Status",
        "targets": [
          {
            "expr": "governance_stream_connected",
            "legendFormat": "{{instance}}"
          }
        ],
        "gridPos": { "h": 8, "w": 12, "x": 0, "y": 8 }
      },
      {
        "title": "Signature Validation Metrics",
        "targets": [
          {
            "expr": "rate(signature_matches_total[5m])",
            "legendFormat": "Matches"
          },
          {
            "expr": "rate(signature_mismatches_total[5m])",
            "legendFormat": "Mismatches"
          }
        ],
        "gridPos": { "h": 8, "w": 12, "x": 12, "y": 8 }
      },
      {
        "title": "System Resources",
        "targets": [
          {
            "expr": "process_resident_memory_bytes / 1e9",
            "legendFormat": "Memory (GB) - {{instance}}"
          },
          {
            "expr": "rate(process_cpu_seconds_total[5m]) * 100",
            "legendFormat": "CPU % - {{instance}}"
          }
        ],
        "gridPos": { "h": 8, "w": 24, "x": 0, "y": 16 }
      }
    ]
  }
}
```

5. **Operational Runbook**
```markdown
# Alys V2 Production Runbook

## Emergency Contacts
- On-call Engineer: [PagerDuty rotation]
- Team Lead: [Contact info]
- Security Team: [Contact info]

## Common Operations

### 1. Emergency Rollback
```bash
# Single node rollback
ssh node1.alys
sudo systemctl stop alys
sudo cp -r /var/backups/alys/rollback/latest/* /opt/alys/
sudo systemctl start alys

# Full cluster rollback
./scripts/emergency_rollback.sh
```

### 2. Governance Stream Recovery
```bash
# Check connection status
curl http://node1:9092/metrics | grep governance_stream_connected

# Force reconnection
curl -X POST http://node1:8545/admin/governance/reconnect

# Check logs
journalctl -u alys -f | grep -i governance
```

### 3. Actor System Issues
```bash
# Check actor health
curl http://node1:9091/actors/health

# Restart specific actor
curl -X POST http://node1:8545/admin/actors/restart -d '{"actor": "BridgeActor"}'

# Check mailbox sizes
curl http://node1:9091/metrics | grep actor_mailbox_size
```

### 4. Consensus Recovery
```bash
# Check consensus status
./scripts/check_consensus.sh

# Force resync from specific height
alys admin resync --from-height 1000000

# Clear corrupted database
systemctl stop alys
rm -rf /var/lib/alys/db/*
systemctl start alys
```

## Alert Response Procedures

### ConsensusHalted
1. Check all nodes are online
2. Review recent logs for errors
3. Check network connectivity between nodes
4. If isolated to one node, remove from rotation
5. If affecting all nodes, check external dependencies

### GovernanceDisconnected
1. Check governance service status
2. Verify network path to governance
3. Check authentication credentials
4. Review StreamActor logs
5. Force reconnection if needed

### HighMemoryUsage
1. Check for memory leaks in metrics
2. Review actor mailbox sizes
3. Check for stuck transactions
4. Restart affected service if necessary
5. Collect heap dump if issue persists

## Performance Tuning

### Database Optimization
```bash
# Compact database
alys admin db compact

# Optimize indexes
alys admin db optimize-indexes

# Clear old logs
find /var/log/alys -mtime +30 -delete
```

### Network Tuning
```bash
# Increase connection limits
sysctl -w net.core.somaxconn=65535
sysctl -w net.ipv4.tcp_max_syn_backlog=65535

# Optimize TCP settings
sysctl -w net.ipv4.tcp_fin_timeout=20
sysctl -w net.ipv4.tcp_tw_reuse=1
```

## Backup Procedures

### Daily Backups
- Database: Automated snapshots every 6 hours
- Configuration: Git repository with version control
- Keys: Encrypted backups to secure storage

### Recovery Testing
- Monthly recovery drill
- Document recovery time
- Update procedures as needed
```

## Testing Plan

### Load Testing
```rust
#[cfg(test)]
mod load_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_sustained_load() {
        let client = create_production_client();
        
        // Simulate production load
        for _ in 0..10000 {
            tokio::spawn(async move {
                client.send_transaction(create_test_tx()).await
            });
        }
        
        // Monitor metrics
        assert!(get_error_rate() < 0.001);
        assert!(get_p99_latency() < Duration::from_secs(1));
    }
}
```

### Chaos Testing
1. Random node failures
2. Network partition simulation
3. Resource exhaustion
4. Byzantine behavior injection

### Monitoring Validation
1. Alert rule testing
2. Dashboard accuracy verification
3. Metric collection validation

## Dependencies

### Blockers
- ALYS-015: Governance cutover must be complete

### Blocked By
None

### Related Issues
- ALYS-017: Documentation updates
- ALYS-018: Training materials

## Definition of Done

- [ ] All nodes deployed successfully
- [ ] Zero downtime achieved
- [ ] Monitoring stack operational
- [ ] All alerts configured
- [ ] Runbooks complete
- [ ] Load testing passed
- [ ] Chaos testing passed
- [ ] Team training complete
- [ ] Documentation updated

## Notes

- Schedule deployment during maintenance window
- Have rollback plan ready
- Ensure team availability during deployment
- Monitor for 48 hours post-deployment

## Time Tracking

- Estimated: 3 days
- Actual: _To be filled_