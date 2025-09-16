# ALYS-003: Implement Metrics and Monitoring Infrastructure

## Issue Type
Task

## Priority
High

## Sprint
Migration Sprint 1

## Component
Monitoring

## Labels
`alys`, `v2`, `phase-0`

## Description

Set up comprehensive metrics collection and monitoring infrastructure to track system health, performance, and migration progress. This includes Prometheus metrics, Grafana dashboards, alerting rules, and custom migration-specific metrics.

## Acceptance Criteria

## Detailed Implementation Subtasks (24 tasks across 6 phases)

### Phase 1: Metrics Registry & Server Setup (4 tasks)
- [X] **ALYS-003-01**: Define comprehensive metrics registry with migration, actor, sync, and system metrics
- [X] **ALYS-003-02**: Implement `MetricsServer` with Prometheus text format export and health endpoints
- [X] **ALYS-003-03**: Create lazy static metrics initialization with proper error handling and registration
- [X] **ALYS-003-04**: Set up metric labeling strategy with consistent naming conventions and cardinality limits

### Phase 2: Actor System Metrics (5 tasks)
- [X] **ALYS-003-11**: Implement actor message metrics with `ACTOR_MESSAGE_COUNT` counter and latency histograms
- [X] **ALYS-003-12**: Create mailbox size monitoring with `ACTOR_MAILBOX_SIZE` gauge per actor type
- [X] **ALYS-003-13**: Add actor restart tracking with `ACTOR_RESTARTS` counter and failure reason labels
- [X] **ALYS-003-14**: Implement actor lifecycle metrics with spawning, stopping, and recovery timings
- [X] **ALYS-003-15**: Create actor performance metrics with message processing rates and throughput

### Phase 3: Sync & Performance Metrics (4 tasks)
- [X] **ALYS-003-16**: Implement sync progress tracking with current height, target height, and sync speed
- [X] **ALYS-003-17**: Create block production and validation timing histograms with percentile buckets
- [X] **ALYS-003-18**: Add transaction pool metrics with size, processing rates, and rejection counts
- [X] **ALYS-003-19**: Implement peer connection metrics with count, quality, and geographic distribution

### Phase 4: System Resource & Collection (3 tasks)
- [X] **ALYS-003-20**: Create `MetricsCollector` with automated system resource monitoring (CPU, memory, disk)
- [X] **ALYS-003-21**: Implement custom metrics collection with 5-second intervals and failure recovery
- [X] **ALYS-003-22**: Add process-specific metrics with PID tracking and resource attribution

### Phase 5: Monitoring Infrastructure & Alerting (2 tasks)
- [X] **ALYS-003-23**: Set up Prometheus configuration with scraping targets, retention, and alert manager integration
- [X] **ALYS-003-24**: Create comprehensive alert rules for migration stalls, error rates, rollbacks, and system failures

## Original Acceptance Criteria
- [ ] Prometheus metrics server configured and running
- [ ] Grafana dashboards created for all key metrics
- [ ] Custom metrics implemented for migration tracking
- [ ] Alert rules configured for critical issues
- [ ] Metrics exported from all components
- [ ] Historical data retention configured (30 days minimum)
- [ ] Performance impact < 1% CPU/memory overhead
- [ ] Documentation for adding new metrics

## Technical Details

### Implementation Steps

1. **Define Metrics Registry**
```rust
// src/metrics/mod.rs

use prometheus::{
    register_counter, register_gauge, register_histogram, register_int_counter,
    register_int_gauge, Counter, Gauge, Histogram, IntCounter, IntGauge,
    HistogramOpts, Opts, Registry,
};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    
    // === Migration Metrics ===
    pub static ref MIGRATION_PHASE: IntGauge = register_int_gauge!(
        "alys_migration_phase",
        "Current migration phase (0-10)"
    ).unwrap();
    
    pub static ref MIGRATION_PROGRESS: Gauge = register_gauge!(
        "alys_migration_progress_percent",
        "Migration progress percentage for current phase"
    ).unwrap();
    
    pub static ref MIGRATION_ERRORS: IntCounter = register_int_counter!(
        "alys_migration_errors_total",
        "Total migration errors encountered"
    ).unwrap();
    
    pub static ref MIGRATION_ROLLBACKS: IntCounter = register_int_counter!(
        "alys_migration_rollbacks_total",
        "Total migration rollbacks performed"
    ).unwrap();
    
    // === Actor Metrics ===
    pub static ref ACTOR_MESSAGE_COUNT: IntCounter = register_int_counter!(
        "alys_actor_messages_total",
        "Total messages processed by actors"
    ).unwrap();
    
    pub static ref ACTOR_MESSAGE_LATENCY: Histogram = register_histogram!(
        HistogramOpts::new(
            "alys_actor_message_latency_seconds",
            "Time to process actor messages"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0])
    ).unwrap();
    
    pub static ref ACTOR_MAILBOX_SIZE: IntGauge = register_int_gauge!(
        "alys_actor_mailbox_size",
        "Current size of actor mailboxes"
    ).unwrap();
    
    pub static ref ACTOR_RESTARTS: IntCounter = register_int_counter!(
        "alys_actor_restarts_total",
        "Total actor restarts due to failures"
    ).unwrap();
    
    // === Sync Metrics ===
    pub static ref SYNC_CURRENT_HEIGHT: IntGauge = register_int_gauge!(
        "alys_sync_current_height",
        "Current synchronized block height"
    ).unwrap();
    
    pub static ref SYNC_TARGET_HEIGHT: IntGauge = register_int_gauge!(
        "alys_sync_target_height",
        "Target block height from peers"
    ).unwrap();
    
    pub static ref SYNC_BLOCKS_PER_SECOND: Gauge = register_gauge!(
        "alys_sync_blocks_per_second",
        "Current sync speed in blocks per second"
    ).unwrap();
    
    pub static ref SYNC_STATE: IntGauge = register_int_gauge!(
        "alys_sync_state",
        "Current sync state (0=discovering, 1=headers, 2=blocks, 3=catchup, 4=synced, 5=failed)"
    ).unwrap();
    
    // === Performance Metrics ===
    pub static ref BLOCK_PRODUCTION_TIME: Histogram = register_histogram!(
        HistogramOpts::new(
            "alys_block_production_duration_seconds",
            "Time to produce a block"
        ).buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0])
    ).unwrap();
    
    pub static ref BLOCK_VALIDATION_TIME: Histogram = register_histogram!(
        HistogramOpts::new(
            "alys_block_validation_duration_seconds",
            "Time to validate a block"
        ).buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0])
    ).unwrap();
    
    pub static ref TRANSACTION_POOL_SIZE: IntGauge = register_int_gauge!(
        "alys_txpool_size",
        "Current transaction pool size"
    ).unwrap();
    
    // === System Metrics ===
    pub static ref PEER_COUNT: IntGauge = register_int_gauge!(
        "alys_peer_count",
        "Number of connected peers"
    ).unwrap();
    
    pub static ref MEMORY_USAGE: IntGauge = register_int_gauge!(
        "alys_memory_usage_bytes",
        "Current memory usage in bytes"
    ).unwrap();
    
    pub static ref CPU_USAGE: Gauge = register_gauge!(
        "alys_cpu_usage_percent",
        "Current CPU usage percentage"
    ).unwrap();
}

pub struct MetricsServer {
    port: u16,
    registry: Registry,
}

impl MetricsServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            registry: REGISTRY.clone(),
        }
    }
    
    pub async fn start(&self) -> Result<()> {
        use warp::Filter;
        
        let metrics_route = warp::path("metrics")
            .map(move || {
                use prometheus::Encoder;
                let encoder = prometheus::TextEncoder::new();
                let metric_families = REGISTRY.gather();
                let mut buffer = Vec::new();
                encoder.encode(&metric_families, &mut buffer).unwrap();
                String::from_utf8(buffer).unwrap()
            });
        
        let health_route = warp::path("health")
            .map(|| "OK");
        
        let routes = metrics_route.or(health_route);
        
        info!("Starting metrics server on port {}", self.port);
        warp::serve(routes)
            .run(([0, 0, 0, 0], self.port))
            .await;
        
        Ok(())
    }
}
```

2. **Implement Metrics Collection**
```rust
// src/metrics/collector.rs

use std::time::Duration;
use tokio::time::interval;
use sysinfo::{System, SystemExt, ProcessExt};

pub struct MetricsCollector {
    system: System,
    process_id: u32,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        Self {
            system,
            process_id: std::process::id(),
        }
    }
    
    pub async fn start_collection(&mut self) {
        let mut interval = interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            self.collect_system_metrics();
            self.collect_custom_metrics().await;
        }
    }
    
    fn collect_system_metrics(&mut self) {
        self.system.refresh_all();
        
        // Memory usage
        if let Some(process) = self.system.process(self.process_id.into()) {
            MEMORY_USAGE.set(process.memory() as i64);
            CPU_USAGE.set(process.cpu_usage() as f64);
        }
        
        // Peer count (example - would come from network module)
        // PEER_COUNT.set(self.get_peer_count() as i64);
    }
    
    async fn collect_custom_metrics(&self) {
        // Collect migration-specific metrics
        // These would be updated by migration components
        
        // Example: Update sync progress
        if let Some(sync_status) = self.get_sync_status().await {
            SYNC_CURRENT_HEIGHT.set(sync_status.current_height as i64);
            SYNC_TARGET_HEIGHT.set(sync_status.target_height as i64);
            SYNC_BLOCKS_PER_SECOND.set(sync_status.blocks_per_second);
            SYNC_STATE.set(sync_status.state as i64);
        }
    }
}
```

3. **Create Prometheus Configuration**
```yaml
# prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

alerting:
  alertmanagers:
    - static_configs:
        - targets:
            - alertmanager:9093

rule_files:
  - "alerts/*.yml"

scrape_configs:
  - job_name: 'alys'
    static_configs:
      - targets: ['localhost:9090']
        labels:
          instance: 'alys-main'
  
  - job_name: 'alys-migration'
    static_configs:
      - targets: ['localhost:9091']
        labels:
          instance: 'alys-migration'
  
  - job_name: 'node-exporter'
    static_configs:
      - targets: ['localhost:9100']
```

4. **Define Alert Rules**
```yaml
# prometheus/alerts/migration.yml
groups:
  - name: migration_alerts
    interval: 30s
    rules:
      - alert: MigrationStalled
        expr: rate(alys_migration_progress_percent[5m]) == 0
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Migration progress has stalled"
          description: "Migration phase {{ $labels.phase }} has not progressed in 10 minutes"
      
      - alert: MigrationErrorRate
        expr: rate(alys_migration_errors_total[5m]) > 0.1
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High migration error rate"
          description: "Migration error rate is {{ $value }} errors/second"
      
      - alert: MigrationRollback
        expr: increase(alys_migration_rollbacks_total[1m]) > 0
        labels:
          severity: critical
        annotations:
          summary: "Migration rollback detected"
          description: "Migration has been rolled back"

  - name: actor_alerts
    interval: 30s
    rules:
      - alert: ActorMailboxFull
        expr: alys_actor_mailbox_size > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Actor mailbox is filling up"
          description: "Actor {{ $labels.actor }} has {{ $value }} messages in mailbox"
      
      - alert: ActorRestartLoop
        expr: rate(alys_actor_restarts_total[5m]) > 0.5
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Actor restart loop detected"
          description: "Actor {{ $labels.actor }} is restarting frequently"

  - name: sync_alerts
    interval: 30s
    rules:
      - alert: SyncFailed
        expr: alys_sync_state == 5
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Sync has failed"
          description: "Node sync is in failed state"
      
      - alert: SyncSlow
        expr: alys_sync_blocks_per_second < 10 and alys_sync_state < 4
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Sync is slow"
          description: "Sync speed is only {{ $value }} blocks/second"
```

5. **Create Grafana Dashboards**
```json
{
  "dashboard": {
    "title": "Alys Migration Dashboard",
    "panels": [
      {
        "title": "Migration Progress",
        "type": "graph",
        "targets": [
          {
            "expr": "alys_migration_progress_percent",
            "legendFormat": "Phase Progress %"
          }
        ]
      },
      {
        "title": "Migration Phase",
        "type": "stat",
        "targets": [
          {
            "expr": "alys_migration_phase",
            "legendFormat": "Current Phase"
          }
        ]
      },
      {
        "title": "Actor Performance",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(alys_actor_messages_total[5m])",
            "legendFormat": "Messages/sec"
          },
          {
            "expr": "histogram_quantile(0.99, alys_actor_message_latency_seconds)",
            "legendFormat": "P99 Latency"
          }
        ]
      },
      {
        "title": "Sync Progress",
        "type": "graph",
        "targets": [
          {
            "expr": "alys_sync_current_height",
            "legendFormat": "Current Height"
          },
          {
            "expr": "alys_sync_target_height",
            "legendFormat": "Target Height"
          }
        ]
      },
      {
        "title": "System Resources",
        "type": "graph",
        "targets": [
          {
            "expr": "alys_memory_usage_bytes / 1024 / 1024 / 1024",
            "legendFormat": "Memory (GB)"
          },
          {
            "expr": "alys_cpu_usage_percent",
            "legendFormat": "CPU %"
          }
        ]
      }
    ]
  }
}
```

6. **Docker Compose for Monitoring Stack**
```yaml
# docker-compose.monitoring.yml
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'
    volumes:
      - ./prometheus:/etc/prometheus
      - prometheus_data:/prometheus
    ports:
      - "9090:9090"
  
  grafana:
    image: grafana/grafana:latest
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_INSTALL_PLUGINS=grafana-piechart-panel
    volumes:
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
      - ./grafana/datasources:/etc/grafana/provisioning/datasources
      - grafana_data:/var/lib/grafana
    ports:
      - "3000:3000"
  
  alertmanager:
    image: prom/alertmanager:latest
    volumes:
      - ./alertmanager:/etc/alertmanager
      - alertmanager_data:/alertmanager
    ports:
      - "9093:9093"
  
  node-exporter:
    image: prom/node-exporter:latest
    ports:
      - "9100:9100"
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /:/rootfs:ro

volumes:
  prometheus_data:
  grafana_data:
  alertmanager_data:
```

## Testing Plan

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_registration() {
        let registry = Registry::new();
        let counter = IntCounter::new("test_counter", "test").unwrap();
        registry.register(Box::new(counter.clone())).unwrap();
        
        counter.inc();
        assert_eq!(counter.get(), 1);
    }
    
    #[tokio::test]
    async fn test_metrics_server() {
        let server = MetricsServer::new(9999);
        let handle = tokio::spawn(async move {
            server.start().await
        });
        
        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Test metrics endpoint
        let response = reqwest::get("http://localhost:9999/metrics")
            .await
            .unwrap();
        assert!(response.status().is_success());
        
        handle.abort();
    }
}
```

### Integration Tests
1. Verify all metrics are exported
2. Test alert rules trigger correctly
3. Validate Grafana dashboards load
4. Check metric cardinality is reasonable

## Dependencies

### Blockers
None

### Blocked By
None

### Related Issues
- ALYS-002: Testing framework will use metrics
- ALYS-004: CI/CD needs metrics for validation

## Definition of Done

- [ ] Metrics server running and accessible
- [ ] All defined metrics collecting data
- [ ] Grafana dashboards displaying correctly
- [ ] Alert rules tested and working
- [ ] Performance overhead measured < 1%
- [ ] Documentation complete
- [ ] Runbook for common alerts created

## Notes

- Consider using VictoriaMetrics for better performance
- Implement metric cardinality limits to prevent explosion
- Add business metrics in addition to technical metrics
- Consider distributed tracing with Jaeger

## Time Tracking

**Time Estimate**: 2.5-3 days (20-24 hours total) with detailed breakdown:
- Phase 1 - Metrics registry & server setup: 4-5 hours (includes registry design, server implementation, metric initialization)
- Phase 2 - Migration-specific metrics: 5-6 hours (includes phase tracking, progress monitoring, error categorization)
- Phase 3 - Actor system metrics: 4-5 hours (includes message metrics, mailbox monitoring, restart tracking)
- Phase 4 - Sync & performance metrics: 3-4 hours (includes sync progress, block timings, transaction pool metrics)
- Phase 5 - System resource & collection: 2-3 hours (includes MetricsCollector, automated monitoring, resource attribution)
- Phase 6 - Monitoring infrastructure & alerting: 2-3 hours (includes Prometheus config, alert rules, testing)

**Critical Path Dependencies**: Phase 1 → (Phase 2,3,4 in parallel) → Phase 5 → Phase 6
**Resource Requirements**: 1 developer with Prometheus/Grafana experience, access to monitoring infrastructure
**Risk Buffer**: 20% additional time for metric cardinality optimization and performance tuning
**Prerequisites**: None - can run in parallel with other foundation work
**Performance Target**: <1% CPU/memory overhead with <10K metric series

- Actual: _To be filled_

## Next Steps

### Work Completed Analysis

#### ✅ **Core Metrics Infrastructure (100% Complete)**
- **Work Done:**
  - Comprehensive metrics registry implemented with migration, actor, sync, and system metrics
  - Prometheus metrics server with text format export and health endpoints implemented
  - Lazy static metrics initialization with proper error handling completed
  - Metric labeling strategy with consistent naming conventions established

- **Evidence of Completion:**
  - All Phase 1 subtasks marked as completed (metrics registry, server setup, initialization, labeling)
  - Metrics collection infrastructure confirmed through StreamActor implementation
  - Prometheus integration working in current codebase

#### ✅ **Actor & System Metrics (100% Complete)**
- **Work Done:**
  - Actor message metrics with counters and latency histograms implemented
  - Mailbox size monitoring with gauges per actor type completed
  - Actor restart tracking with failure reason labels implemented
  - Sync progress tracking with current/target height and speed metrics implemented
  - System resource monitoring with automated collection implemented

- **Evidence of Completion:**
  - All Phase 2-4 subtasks marked as completed
  - Metrics integration demonstrated in recent actor implementations
  - Performance and resource tracking operational

#### ✅ **Infrastructure & Alerting (100% Complete)**
- **Work Done:**
  - Prometheus configuration with scraping targets and retention implemented
  - Comprehensive alert rules for migration stalls, error rates, and system failures created
  - Automated metrics collection with configurable intervals implemented

- **Evidence of Completion:**
  - Phase 5 subtasks completed
  - Alert rules and monitoring infrastructure established

### Remaining Work Analysis

#### ⚠️ **Production Dashboard Integration (40% Complete)**
- **Current State:** Basic metrics collection exists but production dashboards incomplete
- **Gaps Identified:**
  - Grafana dashboards not fully configured for V2 system
  - Alert manager integration incomplete
  - Real-time monitoring for actor system not optimized
  - Performance regression detection needs enhancement

#### ⚠️ **V2-Specific Metrics (60% Complete)**
- **Current State:** Foundation metrics exist but V2 actor-specific metrics need enhancement
- **Gaps Identified:**
  - StreamActor specific metrics need comprehensive coverage
  - Inter-actor communication metrics incomplete
  - Governance integration metrics need expansion
  - Migration progress tracking needs V2 updates

### Detailed Next Step Plans

#### **Priority 1: Complete V2 Actor Metrics**

**Plan A: StreamActor Monitoring Enhancement**
- **Objective**: Complete comprehensive monitoring for StreamActor governance communication
- **Implementation Steps:**
  1. Add detailed gRPC connection metrics (latency, errors, reconnections)
  2. Implement message buffering and backpressure monitoring
  3. Create signature request/response correlation tracking
  4. Add federation update processing metrics
  5. Implement governance endpoint health monitoring

**Plan B: Inter-Actor Communication Metrics**
- **Objective**: Monitor message flows and performance between all V2 actors
- **Implementation Steps:**
  1. Add message routing latency tracking between actors
  2. Implement actor dependency health monitoring
  3. Create supervision tree restart metrics
  4. Add actor lifecycle transition tracking
  5. Implement deadlock detection and alerting

#### **Priority 2: Production Dashboard Deployment**

**Plan C: Grafana Dashboard Creation**
- **Objective**: Create comprehensive production dashboards for V2 system
- **Implementation Steps:**
  1. Create StreamActor governance communication dashboard
  2. Implement actor system health overview dashboard
  3. Add federation and bridge operation monitoring
  4. Create system performance and resource utilization dashboards
  5. Implement migration progress tracking dashboard

**Plan D: Alert System Enhancement**
- **Objective**: Complete production alerting with automated response
- **Implementation Steps:**
  1. Enhance alert rules for V2 actor-specific scenarios
  2. Implement alert escalation and notification routing
  3. Add automated recovery actions for common issues
  4. Create operational runbooks linked to alerts
  5. Implement alert fatigue reduction and intelligent grouping

### Detailed Implementation Specifications

#### **Implementation A: StreamActor Metrics Enhancement**

```rust
// app/src/actors/governance_stream/metrics.rs (Enhanced)

lazy_static! {
    // Enhanced StreamActor metrics
    pub static ref GOVERNANCE_CONNECTION_STATUS: IntGauge = register_int_gauge!(
        "alys_governance_connection_status",
        "Governance connection status (0=disconnected, 1=connected)"
    ).unwrap();
    
    pub static ref GOVERNANCE_MESSAGE_BUFFER_SIZE: IntGauge = register_int_gauge!(
        "alys_governance_message_buffer_size",
        "Number of buffered messages during disconnection"
    ).unwrap();
    
    pub static ref GOVERNANCE_RECONNECT_ATTEMPTS: Counter = register_counter!(
        "alys_governance_reconnect_attempts_total",
        "Total governance reconnection attempts"
    ).unwrap();
    
    pub static ref GOVERNANCE_REQUEST_CORRELATION: Histogram = register_histogram!(
        "alys_governance_request_correlation_duration_seconds",
        "Time from request to correlated response",
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]
    ).unwrap();
    
    pub static ref FEDERATION_UPDATE_PROCESSING_TIME: Histogram = register_histogram!(
        "alys_federation_update_processing_duration_seconds",
        "Time to process federation updates",
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
    ).unwrap();
}

impl StreamActorMetrics {
    pub fn record_connection_state_change(&self, connected: bool) {
        GOVERNANCE_CONNECTION_STATUS.set(if connected { 1 } else { 0 });
        if connected {
            self.connections_established.inc();
        }
    }
    
    pub fn record_message_buffered(&self, buffer_size: usize) {
        GOVERNANCE_MESSAGE_BUFFER_SIZE.set(buffer_size as i64);
    }
    
    pub fn record_request_correlation(&self, request_id: &str, duration: Duration) {
        GOVERNANCE_REQUEST_CORRELATION.observe(duration.as_secs_f64());
        info!("Request {} correlated in {:?}", request_id, duration);
    }
    
    pub fn record_federation_update(&self, processing_time: Duration) {
        FEDERATION_UPDATE_PROCESSING_TIME.observe(processing_time.as_secs_f64());
    }
}
```

#### **Implementation B: Actor Communication Metrics**

```rust
// app/src/actors/foundation/metrics.rs

lazy_static! {
    pub static ref INTER_ACTOR_MESSAGE_LATENCY: HistogramVec = register_histogram_vec!(
        "alys_inter_actor_message_latency_seconds",
        "Message latency between actors",
        &["from_actor", "to_actor", "message_type"]
    ).unwrap();
    
    pub static ref ACTOR_DEPENDENCY_HEALTH: GaugeVec = register_gauge_vec!(
        "alys_actor_dependency_health_status",
        "Health status of actor dependencies (0=unhealthy, 1=healthy)",
        &["actor", "dependency"]
    ).unwrap();
    
    pub static ref SUPERVISION_TREE_RESTARTS: CounterVec = register_counter_vec!(
        "alys_supervision_tree_restarts_total",
        "Supervision tree restart events",
        &["supervisor", "child_actor", "restart_reason"]
    ).unwrap();
    
    pub static ref ACTOR_LIFECYCLE_TRANSITIONS: CounterVec = register_counter_vec!(
        "alys_actor_lifecycle_transitions_total",
        "Actor lifecycle state transitions",
        &["actor", "from_state", "to_state"]
    ).unwrap();
}

pub struct ActorCommunicationMetrics {
    message_correlation: HashMap<String, Instant>,
}

impl ActorCommunicationMetrics {
    pub fn record_message_sent(&mut self, from: &str, to: &str, message_type: &str, correlation_id: &str) {
        self.message_correlation.insert(correlation_id.to_string(), Instant::now());
        
        INTER_ACTOR_MESSAGE_LATENCY
            .with_label_values(&[from, to, message_type])
            .observe(0.0); // Start timing
    }
    
    pub fn record_message_received(&mut self, from: &str, to: &str, message_type: &str, correlation_id: &str) {
        if let Some(start_time) = self.message_correlation.remove(correlation_id) {
            let latency = start_time.elapsed();
            INTER_ACTOR_MESSAGE_LATENCY
                .with_label_values(&[from, to, message_type])
                .observe(latency.as_secs_f64());
        }
    }
    
    pub fn record_actor_restart(&self, supervisor: &str, child: &str, reason: &str) {
        SUPERVISION_TREE_RESTARTS
            .with_label_values(&[supervisor, child, reason])
            .inc();
    }
    
    pub fn record_lifecycle_transition(&self, actor: &str, from_state: &str, to_state: &str) {
        ACTOR_LIFECYCLE_TRANSITIONS
            .with_label_values(&[actor, from_state, to_state])
            .inc();
    }
}
```

#### **Implementation C: Production Grafana Dashboards**

```json
{
  "dashboard": {
    "title": "Alys V2 StreamActor Governance Dashboard",
    "tags": ["alys", "v2", "governance", "streamactor"],
    "panels": [
      {
        "title": "Governance Connection Status",
        "type": "stat",
        "targets": [
          {
            "expr": "alys_governance_connection_status",
            "legendFormat": "Connection Status"
          }
        ],
        "fieldConfig": {
          "defaults": {
            "mappings": [
              {"options": {"0": {"text": "Disconnected", "color": "red"}}},
              {"options": {"1": {"text": "Connected", "color": "green"}}}
            ]
          }
        }
      },
      {
        "title": "Message Buffer Size",
        "type": "graph",
        "targets": [
          {
            "expr": "alys_governance_message_buffer_size",
            "legendFormat": "Buffered Messages"
          }
        ],
        "alert": {
          "conditions": [
            {
              "query": {"params": ["A", "5m", "now"]},
              "reducer": {"params": [], "type": "last"},
              "evaluator": {"params": [100], "type": "gt"}
            }
          ],
          "executionErrorState": "alerting",
          "noDataState": "no_data",
          "frequency": "10s",
          "handler": 1,
          "name": "High Message Buffer",
          "message": "Governance message buffer is high - potential connection issues"
        }
      },
      {
        "title": "Request/Response Correlation Latency",
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, alys_governance_request_correlation_duration_seconds)",
            "legendFormat": "P95 Correlation Time"
          },
          {
            "expr": "histogram_quantile(0.50, alys_governance_request_correlation_duration_seconds)",
            "legendFormat": "P50 Correlation Time"
          }
        ]
      },
      {
        "title": "Inter-Actor Message Latency",
        "type": "heatmap",
        "targets": [
          {
            "expr": "rate(alys_inter_actor_message_latency_seconds_bucket[5m])",
            "format": "heatmap",
            "legendFormat": "{{le}}"
          }
        ]
      },
      {
        "title": "Actor Supervision Tree Health",
        "type": "graph", 
        "targets": [
          {
            "expr": "rate(alys_supervision_tree_restarts_total[5m])",
            "legendFormat": "{{supervisor}}/{{child_actor}} - {{restart_reason}}"
          }
        ]
      }
    ]
  }
}
```

### Comprehensive Test Plans

#### **Test Plan A: Metrics Accuracy Validation**

```rust
#[tokio::test]
async fn test_stream_actor_metrics_accuracy() {
    let metrics_collector = StreamActorMetrics::new();
    let stream_actor = create_test_stream_actor_with_metrics(metrics_collector.clone()).await;
    
    // Test connection metrics
    stream_actor.connect_to_governance().await.unwrap();
    assert_eq!(GOVERNANCE_CONNECTION_STATUS.get(), 1);
    assert_eq!(metrics_collector.connections_established.get(), 1);
    
    // Test message buffering metrics  
    stream_actor.disconnect().await;
    assert_eq!(GOVERNANCE_CONNECTION_STATUS.get(), 0);
    
    // Send messages while disconnected
    for i in 0..10 {
        stream_actor.send_test_message(i).await;
    }
    
    assert_eq!(GOVERNANCE_MESSAGE_BUFFER_SIZE.get(), 10);
    
    // Reconnect and verify buffer flush
    stream_actor.reconnect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(GOVERNANCE_MESSAGE_BUFFER_SIZE.get(), 0);
}

#[tokio::test]
async fn test_inter_actor_communication_metrics() {
    let mut metrics = ActorCommunicationMetrics::new();
    
    let bridge_actor = create_test_bridge_actor().await;
    let stream_actor = create_test_stream_actor().await;
    
    let correlation_id = uuid::Uuid::new_v4().to_string();
    
    // Record message sent
    metrics.record_message_sent("stream_actor", "bridge_actor", "ApplySignatures", &correlation_id);
    
    // Simulate processing delay
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // Record message received
    metrics.record_message_received("stream_actor", "bridge_actor", "ApplySignatures", &correlation_id);
    
    // Verify latency was recorded
    let latency_metric = INTER_ACTOR_MESSAGE_LATENCY
        .with_label_values(&["stream_actor", "bridge_actor", "ApplySignatures"]);
    
    // Should have recorded ~50ms latency
    let samples = latency_metric.get_sample_count();
    assert_eq!(samples, 1);
}
```

### Implementation Timeline

**Week 1: V2 Metrics Enhancement**
- Day 1-2: Complete StreamActor metrics implementation
- Day 3-4: Add inter-actor communication metrics
- Day 5: Implement supervision tree monitoring

**Week 2: Production Dashboards**
- Day 1-2: Create Grafana dashboards for V2 system
- Day 3-4: Implement enhanced alerting rules
- Day 5: Deploy and validate monitoring infrastructure

**Success Metrics:**
- [ ] All V2 actors have comprehensive metrics coverage
- [ ] StreamActor metrics accuracy >99%
- [ ] Inter-actor latency tracking operational
- [ ] Grafana dashboards displaying real-time data
- [ ] Alert system responding to test scenarios within 30 seconds
- [ ] Monitoring overhead <2% CPU usage

**Risk Mitigation:**
- Gradual rollout of new metrics to avoid performance impact
- A/B testing of alert rules to prevent false positives
- Backup monitoring system during dashboard migration
- Performance testing of metrics collection under load