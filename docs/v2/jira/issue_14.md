# ALYS-014: Execute Lighthouse V5 Migration

## Issue Type
Task

## Priority
Critical

## Story Points
10

## Sprint
Migration Sprint 6

## Component
Dependencies

## Labels
`migration`, `phase-4`, `lighthouse`, `execution`, `deployment`

## Description

Execute the controlled migration from Lighthouse v4 to v5 using the compatibility layer. This includes canary deployment, gradual traffic shifting, performance validation, and monitoring throughout the migration process.

## Acceptance Criteria

- [ ] Canary deployment successful (10% traffic)
- [ ] Performance metrics within acceptable range
- [ ] No consensus disruption observed
- [ ] Gradual rollout completed (25%, 50%, 75%, 100%)
- [ ] All validators updated successfully
- [ ] Rollback procedures tested and documented
- [ ] Zero downtime achieved
- [ ] Migration completed within planned window

## Technical Details

### Implementation Steps

1. **Pre-Migration Validation**
```bash
#!/bin/bash
# scripts/lighthouse_v5_pre_migration.sh

set -euo pipefail

echo "=== Lighthouse V5 Pre-Migration Checklist ==="

# Function to check requirement
check_requirement() {
    local name=$1
    local command=$2
    local expected=$3
    
    echo -n "Checking $name... "
    result=$($command 2>/dev/null || echo "FAILED")
    
    if [[ "$result" == *"$expected"* ]]; then
        echo "✓"
        return 0
    else
        echo "✗ (got: $result, expected: $expected)"
        return 1
    fi
}

# Check system requirements
check_requirement "Disk space" "df -h / | awk 'NR==2 {print \$4}' | sed 's/G//'" "50"
check_requirement "Memory available" "free -g | awk 'NR==2 {print \$7}'" "8"
check_requirement "CPU cores" "nproc" "8"

# Check current version
check_requirement "Current Lighthouse version" \
    "lighthouse --version | grep -o 'Lighthouse v[0-9.]*'" \
    "Lighthouse v4"

# Check compatibility layer
check_requirement "Compatibility layer" \
    "cargo test --package lighthouse-compat --quiet && echo 'OK'" \
    "OK"

# Verify backups
check_requirement "Recent backup exists" \
    "find /var/backups/alys -mtime -1 -type d | wc -l" \
    "1"

# Test rollback procedure
echo -n "Testing rollback procedure... "
if ./scripts/test_lighthouse_rollback.sh --dry-run > /dev/null 2>&1; then
    echo "✓"
else
    echo "✗"
    exit 1
fi

# Check metrics baseline
echo "=== Collecting Performance Baseline ==="
curl -s http://localhost:9090/metrics | grep -E "lighthouse_|block_production_|sync_" > /tmp/baseline_metrics.txt
echo "Baseline metrics saved to /tmp/baseline_metrics.txt"

echo ""
echo "=== Pre-Migration Status ==="
echo "All checks passed. Ready to proceed with migration."
echo "Baseline metrics collected for comparison."
```

2. **Implement Canary Deployment**
```rust
// src/migration/lighthouse_v5_canary.rs

use std::sync::Arc;
use tokio::sync::RwLock;

pub struct LighthouseV5Canary {
    compat_layer: Arc<LighthouseCompat>,
    traffic_controller: Arc<TrafficController>,
    health_monitor: HealthMonitor,
    metrics_collector: MetricsCollector,
    config: CanaryConfig,
}

#[derive(Clone)]
pub struct CanaryConfig {
    pub initial_percentage: u8,
    pub monitor_duration: Duration,
    pub success_criteria: SuccessCriteria,
    pub rollback_threshold: RollbackThreshold,
}

#[derive(Clone)]
pub struct SuccessCriteria {
    pub max_error_rate: f64,
    pub max_latency_increase: f64,
    pub min_success_rate: f64,
    pub max_memory_increase: f64,
}

#[derive(Clone)]
pub struct RollbackThreshold {
    pub error_spike: f64,
    pub consensus_failures: u32,
    pub memory_limit_gb: f64,
}

impl LighthouseV5Canary {
    pub async fn start_canary_deployment(&mut self) -> Result<CanaryResult, MigrationError> {
        info!("Starting Lighthouse V5 canary deployment");
        
        // Phase 1: Deploy canary instance
        self.deploy_canary_instance().await?;
        
        // Phase 2: Route initial traffic (10%)
        self.traffic_controller
            .set_v5_percentage(self.config.initial_percentage)
            .await?;
        
        info!("Routing {}% traffic to Lighthouse V5", self.config.initial_percentage);
        
        // Phase 3: Monitor for configured duration
        let monitoring_result = self.monitor_canary().await?;
        
        // Phase 4: Evaluate results
        self.evaluate_canary_results(monitoring_result).await
    }
    
    async fn deploy_canary_instance(&self) -> Result<(), MigrationError> {
        // Start V5 instance alongside V4
        let v5_config = LighthouseV5Config {
            execution_endpoint: std::env::var("EXECUTION_ENDPOINT")?,
            jwt_secret: std::env::var("JWT_SECRET_PATH")?,
            port: 8552, // Different port for canary
            metrics_port: 9091,
        };
        
        // Initialize V5 client
        let v5_client = lighthouse_v5::Client::new(v5_config)
            .await
            .map_err(|e| MigrationError::V5InitFailed(e.to_string()))?;
        
        // Verify V5 is operational
        let version = v5_client.get_version().await?;
        info!("Lighthouse V5 canary started: {}", version);
        
        // Update compatibility layer
        self.compat_layer.enable_v5(v5_client).await?;
        
        Ok(())
    }
    
    async fn monitor_canary(&self) -> Result<MonitoringResult, MigrationError> {
        let start = Instant::now();
        let mut result = MonitoringResult::default();
        
        while start.elapsed() < self.config.monitor_duration {
            // Collect metrics every 30 seconds
            let metrics = self.health_monitor.collect_metrics().await?;
            
            // Check for immediate rollback conditions
            if self.should_rollback_immediately(&metrics) {
                warn!("Immediate rollback triggered: {:?}", metrics);
                self.execute_rollback().await?;
                return Err(MigrationError::RollbackTriggered(
                    "Critical threshold exceeded".to_string()
                ));
            }
            
            // Update monitoring result
            result.update(&metrics);
            
            // Log progress
            if start.elapsed().as_secs() % 300 == 0 {
                info!("Canary monitoring progress: {:?}", result.summary());
            }
            
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        
        Ok(result)
    }
    
    fn should_rollback_immediately(&self, metrics: &HealthMetrics) -> bool {
        metrics.error_rate > self.config.rollback_threshold.error_spike ||
        metrics.consensus_failures > self.config.rollback_threshold.consensus_failures ||
        metrics.memory_usage_gb > self.config.rollback_threshold.memory_limit_gb
    }
    
    async fn evaluate_canary_results(
        &self,
        result: MonitoringResult,
    ) -> Result<CanaryResult, MigrationError> {
        let success_criteria = &self.config.success_criteria;
        
        let passed = result.avg_error_rate <= success_criteria.max_error_rate &&
                     result.latency_increase <= success_criteria.max_latency_increase &&
                     result.success_rate >= success_criteria.min_success_rate &&
                     result.memory_increase <= success_criteria.max_memory_increase;
        
        if passed {
            info!("✅ Canary deployment successful");
            Ok(CanaryResult::Success {
                metrics: result,
                recommendation: "Proceed with gradual rollout".to_string(),
            })
        } else {
            warn!("❌ Canary deployment did not meet success criteria");
            self.execute_rollback().await?;
            Ok(CanaryResult::Failed {
                metrics: result,
                reason: "Success criteria not met".to_string(),
            })
        }
    }
    
    async fn execute_rollback(&self) -> Result<(), MigrationError> {
        warn!("Executing canary rollback");
        
        // Route all traffic back to V4
        self.traffic_controller.set_v5_percentage(0).await?;
        
        // Disable V5 in compatibility layer
        self.compat_layer.disable_v5().await?;
        
        // Stop V5 instance
        // ... shutdown logic
        
        info!("Canary rollback completed");
        Ok(())
    }
}
```

3. **Implement Gradual Traffic Shifting**
```rust
// src/migration/traffic_controller.rs

use std::sync::atomic::{AtomicU8, Ordering};

pub struct TrafficController {
    v5_percentage: Arc<AtomicU8>,
    routing_strategy: RoutingStrategy,
    session_affinity: SessionAffinityManager,
    metrics: TrafficMetrics,
}

#[derive(Clone)]
pub enum RoutingStrategy {
    Random,
    HashBased,
    SessionAffinity,
    WeightedRoundRobin,
}

impl TrafficController {
    pub async fn execute_gradual_rollout(&self) -> Result<(), MigrationError> {
        let stages = vec![
            RolloutStage { percentage: 10, duration: Duration::from_hours(6), name: "Canary" },
            RolloutStage { percentage: 25, duration: Duration::from_hours(12), name: "Early Adopters" },
            RolloutStage { percentage: 50, duration: Duration::from_hours(24), name: "Half Migration" },
            RolloutStage { percentage: 75, duration: Duration::from_hours(12), name: "Majority" },
            RolloutStage { percentage: 90, duration: Duration::from_hours(6), name: "Near Complete" },
            RolloutStage { percentage: 100, duration: Duration::from_hours(24), name: "Full Migration" },
        ];
        
        for stage in stages {
            info!("🚀 Starting rollout stage: {} ({}%)", stage.name, stage.percentage);
            
            // Update traffic percentage
            self.set_v5_percentage(stage.percentage).await?;
            
            // Monitor for stage duration
            let monitor_result = self.monitor_stage(&stage).await?;
            
            // Evaluate stage results
            if !monitor_result.is_healthy() {
                warn!("Stage {} failed health checks", stage.name);
                return self.rollback_to_previous_stage().await;
            }
            
            info!("✅ Stage {} completed successfully", stage.name);
            
            // Save checkpoint for potential rollback
            self.save_rollout_checkpoint(&stage).await?;
        }
        
        info!("🎉 Gradual rollout completed successfully!");
        Ok(())
    }
    
    pub async fn set_v5_percentage(&self, percentage: u8) -> Result<(), MigrationError> {
        if percentage > 100 {
            return Err(MigrationError::InvalidPercentage(percentage));
        }
        
        let old_percentage = self.v5_percentage.load(Ordering::SeqCst);
        self.v5_percentage.store(percentage, Ordering::SeqCst);
        
        // Update routing rules
        self.update_routing_rules(percentage).await?;
        
        // Log change
        info!("Traffic routing updated: {}% -> {}% to V5", old_percentage, percentage);
        
        // Update metrics
        self.metrics.routing_changes.inc();
        self.metrics.current_v5_percentage.set(percentage as f64);
        
        Ok(())
    }
    
    pub fn should_route_to_v5(&self, request_id: &str) -> bool {
        let percentage = self.v5_percentage.load(Ordering::SeqCst);
        
        match self.routing_strategy {
            RoutingStrategy::Random => {
                rand::random::<u8>() < (percentage * 255 / 100)
            }
            RoutingStrategy::HashBased => {
                let hash = calculate_hash(request_id);
                (hash % 100) < percentage as u64
            }
            RoutingStrategy::SessionAffinity => {
                self.session_affinity.get_routing(request_id)
                    .unwrap_or_else(|| {
                        let route_to_v5 = rand::random::<u8>() < (percentage * 255 / 100);
                        self.session_affinity.set_routing(request_id, route_to_v5);
                        route_to_v5
                    })
            }
            RoutingStrategy::WeightedRoundRobin => {
                self.weighted_round_robin(percentage)
            }
        }
    }
    
    async fn monitor_stage(&self, stage: &RolloutStage) -> Result<StageMonitorResult, MigrationError> {
        let start = Instant::now();
        let mut result = StageMonitorResult::new(stage.name.clone());
        
        while start.elapsed() < stage.duration {
            let health = self.check_health().await?;
            result.update(&health);
            
            // Check for degradation
            if health.is_degraded() {
                warn!("Health degradation detected during stage {}", stage.name);
                if health.is_critical() {
                    return Err(MigrationError::CriticalHealthIssue);
                }
            }
            
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
        
        Ok(result)
    }
}
```

4. **Performance Validation System**
```rust
// src/migration/performance_validator.rs

pub struct PerformanceValidator {
    baseline_metrics: BaselineMetrics,
    current_metrics: Arc<RwLock<CurrentMetrics>>,
    thresholds: PerformanceThresholds,
}

#[derive(Clone)]
pub struct PerformanceThresholds {
    pub max_latency_increase_percent: f64,
    pub max_memory_increase_percent: f64,
    pub max_cpu_increase_percent: f64,
    pub min_throughput_percent: f64,
}

impl PerformanceValidator {
    pub async fn validate_migration_performance(&self) -> ValidationResult {
        let current = self.current_metrics.read().await;
        
        let validations = vec![
            self.validate_latency(&current),
            self.validate_memory(&current),
            self.validate_cpu(&current),
            self.validate_throughput(&current),
            self.validate_error_rates(&current),
        ];
        
        let failed_validations: Vec<_> = validations
            .iter()
            .filter(|v| !v.passed)
            .collect();
        
        if failed_validations.is_empty() {
            ValidationResult::Passed {
                summary: "All performance validations passed".to_string(),
            }
        } else {
            ValidationResult::Failed {
                failures: failed_validations.iter().map(|v| v.reason.clone()).collect(),
                recommendation: self.generate_recommendation(&failed_validations),
            }
        }
    }
    
    fn validate_latency(&self, current: &CurrentMetrics) -> Validation {
        let increase = (current.avg_latency - self.baseline_metrics.avg_latency) 
            / self.baseline_metrics.avg_latency * 100.0;
        
        Validation {
            metric: "Latency".to_string(),
            passed: increase <= self.thresholds.max_latency_increase_percent,
            value: format!("{:.2}ms ({:+.1}%)", current.avg_latency, increase),
            reason: if increase > self.thresholds.max_latency_increase_percent {
                format!("Latency increased by {:.1}% (threshold: {:.1}%)", 
                    increase, self.thresholds.max_latency_increase_percent)
            } else {
                "Within acceptable range".to_string()
            },
        }
    }
}
```

5. **Migration Orchestrator**
```rust
// src/migration/orchestrator.rs

pub struct LighthouseV5MigrationOrchestrator {
    canary: LighthouseV5Canary,
    traffic_controller: Arc<TrafficController>,
    performance_validator: PerformanceValidator,
    state_manager: MigrationStateManager,
    notification_service: NotificationService,
}

impl LighthouseV5MigrationOrchestrator {
    pub async fn execute_migration(&mut self) -> Result<MigrationReport, MigrationError> {
        info!("🚀 Starting Lighthouse V5 migration orchestration");
        
        let mut report = MigrationReport::new();
        
        // Step 1: Pre-migration validation
        self.state_manager.set_state(MigrationState::PreValidation).await;
        let pre_validation = self.run_pre_migration_checks().await?;
        report.pre_validation = Some(pre_validation);
        
        // Step 2: Canary deployment
        self.state_manager.set_state(MigrationState::Canary).await;
        self.notification_service.notify("Starting canary deployment").await;
        
        let canary_result = self.canary.start_canary_deployment().await?;
        report.canary_result = Some(canary_result);
        
        if !canary_result.is_successful() {
            return Ok(report.with_status(MigrationStatus::FailedAtCanary));
        }
        
        // Step 3: Gradual rollout
        self.state_manager.set_state(MigrationState::GradualRollout).await;
        self.notification_service.notify("Beginning gradual rollout").await;
        
        let rollout_result = self.traffic_controller.execute_gradual_rollout().await?;
        report.rollout_result = Some(rollout_result);
        
        // Step 4: Performance validation
        self.state_manager.set_state(MigrationState::Validation).await;
        let validation = self.performance_validator.validate_migration_performance().await;
        report.performance_validation = Some(validation);
        
        if !validation.is_passed() {
            warn!("Performance validation failed, initiating rollback");
            self.execute_full_rollback().await?;
            return Ok(report.with_status(MigrationStatus::RolledBack));
        }
        
        // Step 5: Finalization
        self.state_manager.set_state(MigrationState::Finalization).await;
        self.finalize_migration().await?;
        
        // Step 6: Cleanup
        self.state_manager.set_state(MigrationState::Cleanup).await;
        self.cleanup_v4_resources().await?;
        
        self.state_manager.set_state(MigrationState::Complete).await;
        self.notification_service.notify("✅ Migration completed successfully!").await;
        
        Ok(report.with_status(MigrationStatus::Success))
    }
    
    async fn finalize_migration(&self) -> Result<(), MigrationError> {
        // Remove V4 from compatibility layer
        self.compat_layer.set_mode(MigrationMode::V5Only).await?;
        
        // Update configuration
        self.update_configuration_for_v5().await?;
        
        // Verify all validators on V5
        self.verify_all_validators_migrated().await?;
        
        Ok(())
    }
}
```

## Testing Plan

### Integration Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_canary_deployment() {
        let mut canary = create_test_canary();
        let result = canary.start_canary_deployment().await.unwrap();
        
        assert!(result.is_successful());
        assert!(result.metrics.error_rate < 0.01);
    }
    
    #[tokio::test]
    async fn test_gradual_rollout() {
        let controller = create_test_traffic_controller();
        
        // Test each stage
        for percentage in [10, 25, 50, 75, 100] {
            controller.set_v5_percentage(percentage).await.unwrap();
            
            // Verify routing distribution
            let mut v5_count = 0;
            for _ in 0..1000 {
                if controller.should_route_to_v5(&generate_request_id()) {
                    v5_count += 1;
                }
            }
            
            let actual_percentage = v5_count as f64 / 10.0;
            assert!((actual_percentage - percentage as f64).abs() < 5.0);
        }
    }
    
    #[tokio::test]
    async fn test_rollback_on_failure() {
        let mut orchestrator = create_test_orchestrator();
        
        // Inject failure condition
        inject_performance_degradation();
        
        let report = orchestrator.execute_migration().await.unwrap();
        assert_eq!(report.status, MigrationStatus::RolledBack);
    }
}
```

### Performance Tests
```bash
#!/bin/bash
# scripts/lighthouse_performance_test.sh

echo "Running Lighthouse V5 performance comparison"

# Test V4 performance
echo "Testing V4 performance..."
ab -n 10000 -c 100 http://localhost:8551/v4/eth/v1/node/syncing > v4_perf.txt

# Test V5 performance
echo "Testing V5 performance..."
ab -n 10000 -c 100 http://localhost:8552/v5/eth/v1/node/syncing > v5_perf.txt

# Compare results
echo "Performance Comparison:"
echo "V4:" && grep "Requests per second\|Time per request" v4_perf.txt
echo "V5:" && grep "Requests per second\|Time per request" v5_perf.txt
```

## Dependencies

### Blockers
- ALYS-011: Compatibility layer must be ready

### Blocked By
None

### Related Issues
- ALYS-015: Remove V4 dependencies
- ALYS-016: Update documentation

## Definition of Done

- [ ] Canary deployment successful
- [ ] All rollout stages completed
- [ ] Performance validation passed
- [ ] All validators migrated
- [ ] V4 resources cleaned up
- [ ] Documentation updated
- [ ] Rollback procedures tested
- [ ] Team trained on V5
- [ ] Migration report generated

## Notes

- Schedule migration during low-traffic period
- Have team on standby for migration window
- Prepare communication plan for stakeholders
- Consider running V4 in standby mode for 1 week

## Time Tracking

- Estimated: 3 days (migration window)
- Actual: _To be filled_