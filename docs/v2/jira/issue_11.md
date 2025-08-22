# ALYS-011: Implement Lighthouse V5 Compatibility Layer

## Issue Type
Task

## Description

Create a compatibility layer to enable migration from Lighthouse v4 (git revision) to Lighthouse v5 (versioned release). This layer will allow both versions to run in parallel for testing and gradual migration without service disruption.

## Acceptance Criteria

- [ ] Compatibility shim handles all API differences
- [ ] Type conversions between v4 and v5 structures
- [ ] Parallel execution mode for validation
- [ ] A/B testing framework operational
- [ ] Performance comparison metrics collected
- [ ] No consensus disruption during migration
- [ ] Feature flag control for version selection
- [ ] Rollback capability within 5 minutes

## Technical Details

### Implementation Steps

1. **Create Version Abstraction Layer**
```rust
// crates/lighthouse-compat/src/lib.rs

use std::marker::PhantomData;

/// Version-agnostic Lighthouse wrapper
pub enum LighthouseVersion {
    V4,
    V5,
}

pub trait LighthouseAPI: Send + Sync {
    type ExecutionPayload;
    type ForkchoiceState;
    type PayloadAttributes;
    type SignedBeaconBlock;
    
    async fn new_payload(&self, payload: Self::ExecutionPayload) -> Result<PayloadStatus>;
    async fn forkchoice_updated(
        &self,
        state: Self::ForkchoiceState,
        attrs: Option<Self::PayloadAttributes>,
    ) -> Result<ForkchoiceUpdatedResponse>;
    async fn get_payload(&self, id: PayloadId) -> Result<Self::ExecutionPayload>;
}

/// Compatibility layer for smooth migration
pub struct LighthouseCompat<V> {
    version: LighthouseVersion,
    v4_client: Option<lighthouse_v4::Client>,
    v5_client: Option<lighthouse_v5::Client>,
    migration_mode: MigrationMode,
    metrics: CompatMetrics,
    _phantom: PhantomData<V>,
}

#[derive(Debug, Clone)]
pub enum MigrationMode {
    V4Only,
    V5Only,
    Parallel,      // Run both, compare results
    V4Primary,     // V4 primary, V5 shadow
    V5Primary,     // V5 primary, V4 fallback
    Canary(u8),    // Percentage to V5
}

impl<V> LighthouseCompat<V> {
    pub fn new(config: CompatConfig) -> Result<Self> {
        let v4_client = if config.enable_v4 {
            Some(lighthouse_v4::Client::new(&config.v4_config)?)
        } else {
            None
        };
        
        let v5_client = if config.enable_v5 {
            Some(lighthouse_v5::Client::new(&config.v5_config)?)
        } else {
            None
        };
        
        Ok(Self {
            version: config.default_version,
            v4_client,
            v5_client,
            migration_mode: config.migration_mode,
            metrics: CompatMetrics::new(),
            _phantom: PhantomData,
        })
    }
}
```

2. **Implement Type Conversions**
```rust
// crates/lighthouse-compat/src/conversions.rs

use lighthouse_v4 as v4;
use lighthouse_v5 as v5;

/// Convert types from v4 to v5
pub mod v4_to_v5 {
    use super::*;
    
    pub fn convert_execution_payload(
        payload: v4::ExecutionPayloadCapella,
    ) -> v5::ExecutionPayloadDeneb {
        v5::ExecutionPayloadDeneb {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data,
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions: payload.transactions,
            withdrawals: payload.withdrawals,
            // New Deneb fields
            blob_gas_used: Some(0),
            excess_blob_gas: Some(0),
            // Note: No blobs in Alys currently
        }
    }
    
    pub fn convert_forkchoice_state(
        state: v4::ForkchoiceState,
    ) -> v5::ForkchoiceStateV3 {
        v5::ForkchoiceStateV3 {
            head_block_hash: state.head_block_hash,
            safe_block_hash: state.safe_block_hash,
            finalized_block_hash: state.finalized_block_hash,
            // New field in v5
            justified_block_hash: state.finalized_block_hash,
        }
    }
    
    pub fn convert_payload_attributes(
        attrs: v4::PayloadAttributes,
    ) -> v5::PayloadAttributesV3 {
        v5::PayloadAttributesV3 {
            timestamp: attrs.timestamp,
            prev_randao: attrs.prev_randao,
            suggested_fee_recipient: attrs.suggested_fee_recipient,
            withdrawals: attrs.withdrawals,
            // New field for Deneb
            parent_beacon_block_root: None,
        }
    }
    
    pub fn convert_block(
        block: v4::SignedBeaconBlockCapella,
    ) -> Result<v5::SignedBeaconBlockDeneb> {
        Ok(v5::SignedBeaconBlockDeneb {
            message: v5::BeaconBlockDeneb {
                slot: block.message.slot,
                proposer_index: block.message.proposer_index,
                parent_root: block.message.parent_root,
                state_root: block.message.state_root,
                body: convert_block_body(block.message.body)?,
            },
            signature: block.signature,
        })
    }
}

/// Convert types from v5 to v4 (for rollback)
pub mod v5_to_v4 {
    use super::*;
    
    pub fn convert_execution_payload(
        payload: v5::ExecutionPayloadDeneb,
    ) -> Result<v4::ExecutionPayloadCapella> {
        // Check if v5-specific features are used
        if payload.blob_gas_used.unwrap_or(0) > 0 {
            return Err(CompatError::IncompatibleFeature("blob_gas_used"));
        }
        
        Ok(v4::ExecutionPayloadCapella {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data,
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions: payload.transactions,
            withdrawals: payload.withdrawals,
        })
    }
}
```

3. **Implement Parallel Execution Mode**
```rust
// crates/lighthouse-compat/src/parallel.rs

use tokio::time::Instant;

impl LighthouseCompat<Parallel> {
    pub async fn execute_with_comparison<T, F, R>(
        &self,
        operation: &str,
        v4_op: F,
        v5_op: F,
    ) -> Result<R>
    where
        F: Future<Output = Result<R>> + Send,
        R: PartialEq + Debug + Clone,
    {
        let v4_start = Instant::now();
        let v4_future = v4_op();
        
        let v5_start = Instant::now();
        let v5_future = v5_op();
        
        // Execute both in parallel
        let (v4_result, v5_result) = tokio::join!(v4_future, v5_future);
        
        let v4_duration = v4_start.elapsed();
        let v5_duration = v5_start.elapsed();
        
        // Record metrics
        self.metrics.record_operation_time(operation, "v4", v4_duration);
        self.metrics.record_operation_time(operation, "v5", v5_duration);
        
        // Compare results
        match (&v4_result, &v5_result) {
            (Ok(v4_val), Ok(v5_val)) => {
                if v4_val == v5_val {
                    self.metrics.record_match(operation);
                } else {
                    self.metrics.record_mismatch(operation);
                    warn!("Result mismatch in {}: v4={:?}, v5={:?}", 
                        operation, v4_val, v5_val);
                }
            }
            (Ok(_), Err(e)) => {
                self.metrics.record_v5_only_error(operation);
                warn!("V5 failed while V4 succeeded in {}: {}", operation, e);
            }
            (Err(e), Ok(_)) => {
                self.metrics.record_v4_only_error(operation);
                warn!("V4 failed while V5 succeeded in {}: {}", operation, e);
            }
            (Err(e4), Err(e5)) => {
                self.metrics.record_both_errors(operation);
                error!("Both versions failed in {}: v4={}, v5={}", 
                    operation, e4, e5);
            }
        }
        
        // Return v4 result during parallel testing
        v4_result
    }
    
    pub async fn new_payload(&self, payload: ExecutionPayload) -> Result<PayloadStatus> {
        self.execute_with_comparison(
            "new_payload",
            async {
                let v4_payload = convert_to_v4(payload.clone())?;
                self.v4_client.new_payload(v4_payload).await
            },
            async {
                let v5_payload = convert_to_v5(payload.clone())?;
                self.v5_client.new_payload(v5_payload).await
            },
        ).await
    }
}
```

4. **Create A/B Testing Framework**
```rust
// crates/lighthouse-compat/src/ab_test.rs

use rand::Rng;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

pub struct ABTestController {
    tests: HashMap<String, ABTest>,
    metrics: ABTestMetrics,
}

#[derive(Debug, Clone)]
pub struct ABTest {
    pub name: String,
    pub v5_percentage: u8,
    pub start_time: Instant,
    pub duration: Duration,
    pub sticky_sessions: bool,
}

impl ABTestController {
    pub fn should_use_v5(&self, test_name: &str, session_id: &str) -> bool {
        if let Some(test) = self.tests.get(test_name) {
            // Check if test is active
            if test.start_time.elapsed() > test.duration {
                return false;
            }
            
            if test.sticky_sessions {
                // Use hash for consistent assignment
                let mut hasher = DefaultHasher::new();
                session_id.hash(&mut hasher);
                let hash = hasher.finish();
                let threshold = (u64::MAX / 100) * test.v5_percentage as u64;
                hash < threshold
            } else {
                // Random assignment
                let mut rng = rand::thread_rng();
                rng.gen_range(0..100) < test.v5_percentage
            }
        } else {
            false
        }
    }
    
    pub fn record_result(&mut self, test_name: &str, version: &str, success: bool, latency: Duration) {
        self.metrics.record_request(test_name, version, success, latency);
    }
    
    pub fn get_test_results(&self, test_name: &str) -> Option<TestResults> {
        self.metrics.get_results(test_name)
    }
}

#[derive(Debug, Clone)]
pub struct TestResults {
    pub v4_requests: u64,
    pub v5_requests: u64,
    pub v4_success_rate: f64,
    pub v5_success_rate: f64,
    pub v4_p50_latency: Duration,
    pub v5_p50_latency: Duration,
    pub v4_p99_latency: Duration,
    pub v5_p99_latency: Duration,
}
```

5. **Implement Migration Controller**
```rust
// crates/lighthouse-compat/src/migration.rs

use actix::prelude::*;

pub struct MigrationController {
    compat: Arc<LighthouseCompat<Dynamic>>,
    state: MigrationState,
    metrics: MigrationMetrics,
    rollback_plan: RollbackPlan,
}

#[derive(Debug, Clone)]
pub enum MigrationState {
    PreMigration,
    Testing { started: Instant, progress: f64 },
    Canary { percentage: u8 },
    Gradual { current: u8, target: u8, step: u8 },
    Complete,
    RolledBack { reason: String },
}

impl MigrationController {
    pub async fn execute_migration_plan(&mut self) -> Result<()> {
        info!("Starting Lighthouse v4 to v5 migration");
        
        // Phase 1: Parallel testing
        self.state = MigrationState::Testing {
            started: Instant::now(),
            progress: 0.0,
        };
        
        self.run_parallel_tests().await?;
        
        // Phase 2: Canary deployment (10%)
        self.state = MigrationState::Canary { percentage: 10 };
        self.compat.set_migration_mode(MigrationMode::Canary(10));
        
        // Monitor for 6 hours
        self.monitor_canary(Duration::from_hours(6)).await?;
        
        // Phase 3: Gradual rollout
        for percentage in [25, 50, 75, 90, 100] {
            self.state = MigrationState::Gradual {
                current: self.get_current_percentage(),
                target: percentage,
                step: 5,
            };
            
            self.gradual_rollout(percentage).await?;
            
            // Monitor at each stage
            self.monitor_health(Duration::from_hours(2)).await?;
        }
        
        // Phase 4: Complete migration
        self.state = MigrationState::Complete;
        self.compat.set_migration_mode(MigrationMode::V5Only);
        
        info!("Migration to Lighthouse v5 complete!");
        
        Ok(())
    }
    
    async fn monitor_health(&self, duration: Duration) -> Result<()> {
        let start = Instant::now();
        
        while start.elapsed() < duration {
            let health = self.check_system_health().await?;
            
            if !health.is_healthy() {
                warn!("Health check failed: {:?}", health);
                
                if health.should_rollback() {
                    return self.execute_rollback("Health check failure").await;
                }
            }
            
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        
        Ok(())
    }
    
    async fn execute_rollback(&mut self, reason: &str) -> Result<()> {
        error!("Executing rollback: {}", reason);
        
        self.state = MigrationState::RolledBack {
            reason: reason.to_string(),
        };
        
        // Immediate switch back to v4
        self.compat.set_migration_mode(MigrationMode::V4Only);
        
        // Verify rollback successful
        self.verify_rollback().await?;
        
        Err(MigrationError::RolledBack(reason.to_string()))
    }
}
```

6. **Create Compatibility Tests**
```rust
// tests/lighthouse_compat_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_type_conversions() {
        // Test v4 to v5 conversion
        let v4_payload = create_v4_payload();
        let v5_payload = v4_to_v5::convert_execution_payload(v4_payload.clone());
        
        // Verify essential fields preserved
        assert_eq!(v4_payload.block_hash, v5_payload.block_hash);
        assert_eq!(v4_payload.timestamp, v5_payload.timestamp);
        
        // Test v5 to v4 conversion (for rollback)
        let v4_recovered = v5_to_v4::convert_execution_payload(v5_payload).unwrap();
        assert_eq!(v4_payload, v4_recovered);
    }
    
    #[tokio::test]
    async fn test_parallel_execution() {
        let compat = LighthouseCompat::<Parallel>::new(test_config()).unwrap();
        
        let payload = create_test_payload();
        let status = compat.new_payload(payload).await.unwrap();
        
        // Check metrics were recorded
        let metrics = compat.get_metrics();
        assert!(metrics.operations_compared > 0);
        assert!(metrics.matches > 0 || metrics.mismatches > 0);
    }
    
    #[tokio::test]
    async fn test_ab_testing() {
        let mut controller = ABTestController::new();
        
        controller.create_test(ABTest {
            name: "lighthouse_v5".to_string(),
            v5_percentage: 50,
            start_time: Instant::now(),
            duration: Duration::from_hours(1),
            sticky_sessions: true,
        });
        
        // Test distribution
        let mut v4_count = 0;
        let mut v5_count = 0;
        
        for i in 0..1000 {
            let session_id = format!("session_{}", i);
            if controller.should_use_v5("lighthouse_v5", &session_id) {
                v5_count += 1;
            } else {
                v4_count += 1;
            }
        }
        
        // Should be roughly 50/50
        assert!((450..550).contains(&v5_count));
    }
    
    #[tokio::test]
    async fn test_rollback() {
        let mut controller = MigrationController::new(test_config()).unwrap();
        
        // Start migration
        controller.state = MigrationState::Canary { percentage: 10 };
        controller.compat.set_migration_mode(MigrationMode::Canary(10));
        
        // Simulate failure
        controller.execute_rollback("Test rollback").await.err();
        
        // Verify rolled back to v4
        assert!(matches!(controller.state, MigrationState::RolledBack { .. }));
        assert!(matches!(
            controller.compat.get_migration_mode(),
            MigrationMode::V4Only
        ));
    }
}
```

## Testing Plan

### Unit Tests
1. Type conversion correctness
2. API compatibility verification
3. Error handling in both versions
4. Metrics collection accuracy

### Integration Tests
1. Parallel execution with real clients
2. A/B testing distribution
3. Migration flow end-to-end
4. Rollback procedures

### Performance Tests
```rust
#[bench]
fn bench_v4_vs_v5_performance(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    b.iter(|| {
        runtime.block_on(async {
            let compat = create_test_compat();
            let payload = create_large_payload();
            
            // Measure both versions
            compat.new_payload(payload).await.unwrap()
        })
    });
}
```

## Dependencies

### Blockers
None

### Blocked By
- ALYS-008: EngineActor must be compatible

### Related Issues
- ALYS-012: Lighthouse V5 Migration Execution
- ALYS-013: Performance validation
- ALYS-014: Rollback procedures

## Definition of Done

- [ ] Compatibility layer implemented
- [ ] Type conversions working both ways
- [ ] Parallel execution mode tested
- [ ] A/B testing framework operational
- [ ] Migration controller ready
- [ ] Rollback tested successfully
- [ ] Performance metrics collected
- [ ] Documentation complete
- [ ] Code review completed

## Subtasks

### Phase 1: Foundation & Analysis (Story Points: 1)
- **ALYS-011-1**: Analyze Lighthouse v4 vs v5 API differences
  - [ ] Audit all current Lighthouse v4 usage in codebase
  - [ ] Document breaking changes in v5 API
  - [ ] Create compatibility matrix for types and methods
  - [ ] Identify potential migration risks and blockers
  - **DoD**: Complete API difference documentation with migration impact analysis

- **ALYS-011-2**: Design compatibility layer architecture
  - [ ] Create trait-based abstraction design
  - [ ] Design type conversion system
  - [ ] Plan migration modes and strategies
  - [ ] Design metrics collection framework
  - **DoD**: Architecture document with UML diagrams and type definitions

### Phase 2: Core Compatibility Implementation (Story Points: 3)
- **ALYS-011-3**: Implement version abstraction layer (TDD)
  - [ ] Write tests for LighthouseAPI trait
  - [ ] Implement LighthouseCompat struct with version switching
  - [ ] Create configuration system for migration modes
  - [ ] Add comprehensive error handling
  - **DoD**: All abstraction layer tests passing with >90% coverage

- **ALYS-011-4**: Implement bidirectional type conversions (TDD)
  - [ ] Write property-based tests for type conversions
  - [ ] Implement v4 → v5 type converters
  - [ ] Implement v5 → v4 type converters (for rollback)
  - [ ] Handle edge cases and validation errors
  - **DoD**: All conversion tests passing, including edge cases and error scenarios

- **ALYS-011-5**: Implement parallel execution mode (TDD)
  - [ ] Write tests for parallel execution with comparison
  - [ ] Implement side-by-side execution logic
  - [ ] Add result comparison and divergence detection
  - [ ] Create comprehensive metrics collection
  - **DoD**: Parallel mode working with metrics collection and mismatch detection

### Phase 3: Migration Framework (Story Points: 2)
- **ALYS-011-6**: Implement A/B testing framework (TDD)
  - [ ] Write tests for traffic splitting algorithms
  - [ ] Implement sticky session support
  - [ ] Add percentage-based traffic control
  - [ ] Create test result aggregation and reporting
  - **DoD**: A/B framework tested with statistical distribution validation

- **ALYS-011-7**: Implement migration controller (TDD)
  - [ ] Write tests for migration state management
  - [ ] Implement gradual rollout logic
  - [ ] Add health monitoring and rollback triggers
  - [ ] Create migration progress tracking
  - **DoD**: Migration controller with automated health checks and rollback capability

### Phase 4: Safety & Monitoring (Story Points: 1)
- **ALYS-011-8**: Implement rollback system (TDD)
  - [ ] Write tests for emergency rollback scenarios
  - [ ] Implement 5-minute rollback capability
  - [ ] Add rollback verification and health checks
  - [ ] Create rollback decision algorithms
  - **DoD**: Rollback system tested with sub-5-minute recovery time

- **ALYS-011-9**: Implement comprehensive monitoring (TDD)
  - [ ] Write tests for metrics collection
  - [ ] Add Prometheus metrics integration
  - [ ] Implement performance comparison dashboards
  - [ ] Create alerting for migration issues
  - **DoD**: Full monitoring suite with automated alerts and dashboards

### Phase 5: Integration & Validation (Story Points: 1)
- **ALYS-011-10**: Integration with existing EngineActor
  - [ ] Update EngineActor to use compatibility layer
  - [ ] Add feature flags for version selection
  - [ ] Test integration with consensus layer
  - [ ] Validate no performance regression
  - **DoD**: EngineActor integrated with compatibility layer, all tests passing

- **ALYS-011-11**: End-to-end migration testing
  - [ ] Create full migration test scenarios
  - [ ] Test rollback procedures under load
  - [ ] Validate consensus integrity during migration
  - [ ] Performance benchmark both versions
  - **DoD**: Complete migration tested successfully with performance validation

### Technical Implementation Guidelines

#### Test-Driven Development Approach
1. **Red Phase**: Write failing tests that define expected behavior
2. **Green Phase**: Implement minimal code to make tests pass
3. **Refactor Phase**: Clean up code while maintaining test coverage

#### Testing Strategy
- **Unit Tests**: >90% coverage for all compatibility layer components
- **Integration Tests**: End-to-end migration scenarios
- **Property-Based Tests**: Type conversion correctness with QuickCheck
- **Performance Tests**: Benchmark both versions under realistic load
- **Chaos Tests**: Network partition and failure scenarios during migration

#### Code Quality Standards
- **Static Analysis**: Clippy warnings addressed
- **Security Review**: All type conversions validated for safety
- **Documentation**: Comprehensive docs for migration procedures
- **Error Handling**: Graceful degradation and clear error messages

#### Deployment Strategy
- **Feature Flags**: Safe rollout with instant rollback capability
- **Blue-Green Deployment**: Zero-downtime migration approach
- **Canary Testing**: Start with 5% traffic to v5, gradually increase
- **Health Monitoring**: Automated rollback on performance degradation

#### Risk Mitigation
- **Consensus Safety**: Ensure no fork risks during migration
- **Data Integrity**: Validate all state transitions
- **Performance Impact**: Monitor latency and throughput during migration
- **Rollback Testing**: Regular drills to ensure 5-minute recovery time

## Notes

- Document all API differences
- Migration must maintain consensus integrity
- Zero-downtime requirement for production deployment
- All subtasks follow TDD methodology with comprehensive test coverage