# ALYS-002: Setup Comprehensive Testing Framework

## Issue Type
Task

## Priority
Critical

## Sprint
Migration Sprint 1

## Component
Testing

## Labels
`alys`, `v2`

## Description

Establish a comprehensive testing framework that will be used throughout the migration process. This includes unit testing, integration testing, property-based testing, chaos testing, and performance benchmarking capabilities.

## Acceptance Criteria

## Detailed Implementation Subtasks (28 tasks across 7 phases)

### Phase 1: Test Infrastructure Foundation (4 tasks)
- [ ] **ALYS-002-01**: Design and implement `MigrationTestFramework` core structure with runtime management and configuration [https://marathondh.atlassian.net/browse/AN-329]
- [ ] **ALYS-002-02**: Create `TestConfig` system with environment-specific settings and validation [https://marathondh.atlassian.net/browse/AN-330]
- [ ] **ALYS-002-03**: Implement `TestHarnesses` collection with specialized harnesses for each migration component [https://marathondh.atlassian.net/browse/AN-331]
- [ ] **ALYS-002-04**: Set up test metrics collection system with `MetricsCollector` and reporting capabilities [https://marathondh.atlassian.net/browse/AN-332]

### Phase 2: Actor Testing Framework (6 tasks)
- [ ] **ALYS-002-05**: Implement `ActorTestHarness` with actor lifecycle management and supervision testing [https://marathondh.atlassian.net/browse/AN-333]
- [ ] **ALYS-002-06**: Create actor recovery testing with panic injection and supervisor restart validation [https://marathondh.atlassian.net/browse/AN-334]
- [ ] **ALYS-002-07**: Implement concurrent message testing with 1000+ message load verification [https://marathondh.atlassian.net/browse/AN-335]
- [ ] **ALYS-002-08**: Create message ordering verification system with sequence tracking [https://marathondh.atlassian.net/browse/AN-336]
- [ ] **ALYS-002-09**: Implement mailbox overflow testing with backpressure validation [https://marathondh.atlassian.net/browse/AN-337]
- [ ] **ALYS-002-10**: Create actor communication testing with cross-actor message flows [https://marathondh.atlassian.net/browse/AN-338]

### Phase 3: Sync Testing Framework (5 tasks)
- [ ] **ALYS-002-11**: Implement `SyncTestHarness` with mock P2P network and simulated blockchain [https://marathondh.atlassian.net/browse/AN-339]
- [ ] **ALYS-002-12**: Create full sync testing from genesis to tip with 10,000+ block validation [https://marathondh.atlassian.net/browse/AN-340]
- [ ] **ALYS-002-13**: Implement sync resilience testing with network failures and peer disconnections [https://marathondh.atlassian.net/browse/AN-341]
- [ ] **ALYS-002-14**: Create checkpoint consistency testing with configurable intervals [https://marathondh.atlassian.net/browse/AN-342]
- [ ] **ALYS-002-15**: Implement parallel sync testing with multiple peer scenarios [https://marathondh.atlassian.net/browse/AN-343]

### Phase 4: Property-Based Testing (4 tasks)
- [ ] **ALYS-002-16**: Set up PropTest framework with custom generators for blockchain data structures [https://marathondh.atlassian.net/browse/AN-344]
- [ ] **ALYS-002-17**: Implement actor message ordering property tests with sequence verification [https://marathondh.atlassian.net/browse/AN-345]
- [ ] **ALYS-002-18**: Create sync checkpoint consistency property tests with failure injection [https://marathondh.atlassian.net/browse/AN-346]
- [ ] **ALYS-002-19**: Implement governance signature validation property tests with Byzantine scenarios [https://marathondh.atlassian.net/browse/AN-347]

### Phase 5: Chaos Testing Framework (4 tasks)
- [ ] **ALYS-002-20**: Implement `ChaosTestFramework` with configurable chaos injection strategies [https://marathondh.atlassian.net/browse/AN-348]
- [ ] **ALYS-002-21**: Create network chaos testing with partitions, latency, and message corruption [https://marathondh.atlassian.net/browse/AN-349]
- [ ] **ALYS-002-22**: Implement system resource chaos with memory pressure, CPU stress, and disk failures [https://marathondh.atlassian.net/browse/AN-350]
- [ ] **ALYS-002-23**: Create Byzantine behavior simulation with malicious actor injection [https://marathondh.atlassian.net/browse/AN-351]

### Phase 6: Performance Benchmarking (3 tasks)
- [ ] **ALYS-002-24**: Set up Criterion.rs benchmarking suite with actor throughput measurements [https://marathondh.atlassian.net/browse/AN-352]
- [ ] **ALYS-002-25**: Implement sync performance benchmarks with block processing rate validation [https://marathondh.atlassian.net/browse/AN-353]
- [ ] **ALYS-002-26**: Create memory and CPU profiling integration with flamegraph generation [https://marathondh.atlassian.net/browse/AN-354]

### Phase 7: CI/CD Integration & Reporting (2 tasks)
- [ ] **ALYS-002-27**: Implement Docker Compose test environment with Bitcoin regtest and Reth [https://marathondh.atlassian.net/browse/AN-355]
- [ ] **ALYS-002-28**: Create test reporting system with coverage analysis, performance trending, and chaos test results [https://marathondh.atlassian.net/browse/AN-356]

## Original Acceptance Criteria
- [ ] Test harness structure created and documented
- [ ] Unit test framework configured with coverage reporting
- [ ] Integration test environment with Docker Compose
- [ ] Property-based testing with proptest configured
- [ ] Chaos testing framework implemented
- [ ] Performance benchmarking suite ready
- [ ] CI/CD pipeline integrated with all test types
- [ ] Test reports automatically generated
- [ ] Minimum 80% code coverage achieved for new code

## Technical Details

### Implementation Steps

1. **Create Test Framework Structure**
```rust
// tests/framework/mod.rs

pub mod harness;
pub mod validators;
pub mod generators;
pub mod chaos;
pub mod performance;

use std::sync::Arc;
use tokio::runtime::Runtime;

/// Master test framework for migration testing
pub struct MigrationTestFramework {
    runtime: Arc<Runtime>,
    config: TestConfig,
    harnesses: TestHarnesses,
    validators: Validators,
    metrics: MetricsCollector,
}

#[derive(Debug, Clone)]
pub struct TestConfig {
    pub parallel_tests: bool,
    pub chaos_enabled: bool,
    pub performance_tracking: bool,
    pub coverage_enabled: bool,
    pub docker_compose_file: String,
    pub test_data_dir: PathBuf,
}

pub struct TestHarnesses {
    pub sync_harness: SyncTestHarness,
    pub actor_harness: ActorTestHarness,
    pub lighthouse_harness: LighthouseCompatHarness,
    pub governance_harness: GovernanceIntegrationHarness,
    pub network_harness: NetworkTestHarness,
}

impl MigrationTestFramework {
    pub fn new(config: TestConfig) -> Result<Self> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(8)
                .enable_all()
                .build()?
        );
        
        Ok(Self {
            runtime: runtime.clone(),
            config: config.clone(),
            harnesses: TestHarnesses::new(config.clone(), runtime.clone())?,
            validators: Validators::new(),
            metrics: MetricsCollector::new(),
        })
    }
    
    pub async fn run_phase_validation(&self, phase: MigrationPhase) -> ValidationResult {
        let start = Instant::now();
        
        // Run tests specific to migration phase
        let results = match phase {
            MigrationPhase::Foundation => self.validate_foundation().await,
            MigrationPhase::ActorCore => self.validate_actor_core().await,
            MigrationPhase::SyncImprovement => self.validate_sync().await,
            MigrationPhase::LighthouseMigration => self.validate_lighthouse().await,
            MigrationPhase::GovernanceIntegration => self.validate_governance().await,
        };
        
        // Collect metrics
        self.metrics.record_phase_validation(phase, start.elapsed(), &results);
        
        results
    }
}
```

2. **Setup Actor Test Harness**
```rust
// tests/framework/harness/actor.rs

use actix::prelude::*;
use std::time::Duration;

pub struct ActorTestHarness {
    system: System,
    test_actors: HashMap<String, Addr<TestActor>>,
    message_log: Arc<RwLock<Vec<TestMessage>>>,
}

impl ActorTestHarness {
    pub fn new() -> Self {
        let system = System::new();
        Self {
            system,
            test_actors: HashMap::new(),
            message_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Test actor supervision and recovery
    pub async fn test_actor_recovery(&mut self) -> Result<()> {
        // Create supervised actor
        let actor = TestActor::new("test_actor".to_string());
        let addr = Supervisor::start(|_| actor);
        
        // Send message that causes panic
        addr.send(PanicMessage).await?;
        
        // Wait for supervisor to restart actor
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify actor is responsive
        let response = addr.send(PingMessage).await?;
        assert_eq!(response, "pong");
        
        Ok(())
    }
    
    /// Test concurrent message handling
    pub async fn test_concurrent_messages(&mut self) -> Result<()> {
        let actor = TestActor::new("concurrent_test".to_string());
        let addr = actor.start();
        
        // Send 1000 messages concurrently
        let futures: Vec<_> = (0..1000)
            .map(|i| addr.send(TestMessage { id: i }))
            .collect();
        
        let results = futures::future::join_all(futures).await;
        
        // Verify all messages processed
        assert_eq!(results.len(), 1000);
        for result in results {
            assert!(result.is_ok());
        }
        
        Ok(())
    }
}
```

3. **Setup Sync Test Harness**
```rust
// tests/framework/harness/sync.rs

pub struct SyncTestHarness {
    mock_network: MockP2PNetwork,
    simulated_chain: SimulatedBlockchain,
    sync_actor: Option<Addr<SyncActor>>,
    config: SyncTestConfig,
}

#[derive(Debug, Clone)]
pub struct SyncTestConfig {
    pub chain_height: u64,
    pub block_time: Duration,
    pub network_latency: Duration,
    pub peer_count: usize,
    pub failure_rate: f64,
    pub partition_probability: f64,
}

impl SyncTestHarness {
    /// Test sync from genesis to tip
    pub async fn test_full_sync(&mut self) -> Result<TestResult> {
        // Generate blockchain
        self.simulated_chain.generate_blocks(10_000).await?;
        
        // Start sync
        let sync_actor = self.create_sync_actor().await?;
        sync_actor.send(StartSync {
            from_height: Some(0),
            target_height: Some(10_000),
        }).await??;
        
        // Monitor progress
        let mut last_height = 0;
        let timeout = Duration::from_secs(60);
        let start = Instant::now();
        
        while start.elapsed() < timeout {
            let status = sync_actor.send(GetSyncStatus).await??;
            
            if status.current_height == 10_000 {
                return Ok(TestResult::Success {
                    duration: start.elapsed(),
                    metrics: self.collect_metrics(),
                });
            }
            
            // Check progress
            assert!(status.current_height >= last_height, "Sync went backwards!");
            last_height = status.current_height;
            
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        Err(Error::Timeout)
    }
    
    /// Test sync with network failures
    pub async fn test_sync_resilience(&mut self) -> Result<()> {
        self.simulated_chain.generate_blocks(1_000).await?;
        
        let sync_handle = tokio::spawn({
            let sync_actor = self.sync_actor.clone();
            async move {
                sync_actor.send(StartSync::default()).await
            }
        });
        
        // Inject failures
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            self.mock_network.disconnect_random_peer().await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            self.mock_network.reconnect_peers().await;
        }
        
        // Should still complete
        sync_handle.await???;
        
        Ok(())
    }
}
```

4. **Setup Property-Based Testing**
```rust
// tests/framework/property.rs

use proptest::prelude::*;

proptest! {
    #[test]
    fn test_actor_message_ordering(
        messages in prop::collection::vec(any::<TestMessage>(), 1..100)
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let actor = OrderedActor::new();
            let addr = actor.start();
            
            // Send all messages
            for msg in &messages {
                addr.send(msg.clone()).await.unwrap();
            }
            
            // Verify ordering preserved
            let log = addr.send(GetMessageLog).await.unwrap();
            assert_eq!(log, messages);
        });
    }
    
    #[test]
    fn test_sync_checkpoint_consistency(
        checkpoint_interval in 10u64..100,
        blocks_to_sync in 100u64..1000,
        failure_points in prop::collection::vec(0u64..1000, 0..10)
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut harness = SyncTestHarness::new_with_checkpoint_interval(
                checkpoint_interval
            );
            
            // Inject failures at specified points
            for point in failure_points {
                harness.inject_failure_at_height(point);
            }
            
            // Sync should still complete
            harness.sync_to_height(blocks_to_sync).await.unwrap();
            
            // Verify all checkpoints valid
            let checkpoints = harness.get_all_checkpoints().await.unwrap();
            for checkpoint in checkpoints {
                assert!(checkpoint.verified);
                assert_eq!(checkpoint.height % checkpoint_interval, 0);
            }
        });
    }
}
```

5. **Setup Chaos Testing Framework**
```rust
// tests/framework/chaos.rs

pub struct ChaosTestFramework {
    harness: Box<dyn TestHarness>,
    chaos_config: ChaosConfig,
    chaos_injector: ChaosInjector,
    report: ChaosReport,
}

#[derive(Debug, Clone)]
pub struct ChaosConfig {
    pub random_disconnects: bool,
    pub corrupt_messages: bool,
    pub slow_network: bool,
    pub memory_pressure: bool,
    pub cpu_stress: bool,
    pub disk_failures: bool,
    pub clock_skew: bool,
}

impl ChaosTestFramework {
    pub async fn run_chaos_test(&mut self, duration: Duration) -> Result<ChaosReport> {
        let start = Instant::now();
        
        // Start normal operations
        self.harness.start_normal_operations().await?;
        
        // Inject chaos
        while start.elapsed() < duration {
            let chaos_event = self.select_random_chaos();
            self.inject_chaos_event(chaos_event).await?;
            
            // Random delay between chaos events
            let delay = Duration::from_millis(rand::gen_range(100..5000));
            tokio::time::sleep(delay).await;
        }
        
        // Verify system recovered
        self.verify_system_health().await?;
        
        Ok(self.report.clone())
    }
    
    async fn inject_chaos_event(&mut self, event: ChaosEvent) -> Result<()> {
        match event {
            ChaosEvent::NetworkPartition => {
                self.chaos_injector.partition_network(0.5).await?;
                self.report.network_partitions += 1;
            }
            ChaosEvent::CorruptMessage => {
                self.chaos_injector.corrupt_next_message().await?;
                self.report.corrupted_messages += 1;
            }
            ChaosEvent::SlowNetwork => {
                self.chaos_injector.add_latency(Duration::from_secs(5)).await?;
                self.report.slow_network_events += 1;
            }
            ChaosEvent::ProcessCrash => {
                self.chaos_injector.crash_random_process().await?;
                self.report.process_crashes += 1;
            }
        }
        Ok(())
    }
}
```

6. **Setup Performance Benchmarking**
```rust
// tests/framework/performance.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};

pub fn benchmark_actor_throughput(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("actor_message_throughput", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = TestActor::new();
                let addr = actor.start();
                
                for i in 0..10000 {
                    addr.send(TestMessage { id: i }).await.unwrap();
                }
            })
        })
    });
}

pub fn benchmark_sync_speed(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("sync_1000_blocks", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let mut harness = SyncTestHarness::new();
                harness.sync_blocks(black_box(1000)).await.unwrap()
            })
        })
    });
}

criterion_group!(benches, benchmark_actor_throughput, benchmark_sync_speed);
criterion_main!(benches);
```

7. **Docker Compose Test Environment**
```yaml
# docker-compose.test.yml
services:
  bitcoin-core:
    image: balajimara/bitcoin:25.99
    container_name: bitcoin-test
    restart: unless-stopped
    ports:
      - "18333:18333"
      - "18443:18443"
    volumes:
      - ./test-data/bitcoin:/home/bitcoin/.bitcoin
    command:
      - -printtoconsole
      - -debug=1
      - -regtest=1
      - -fallbackfee=0.002
      - -rpcallowip=0.0.0.0/0
      - -rpcbind=0.0.0.0
      - -server
      - -rpcuser=rpcuser
      - -rpcpassword=rpcpassword
      - -port=18333
      - -rpcport=18443
      - -txindex

  execution:
    container_name: execution-test
    restart: unless-stopped
    image: ghcr.io/paradigmxyz/reth:v1.1.3
    ports:
      - '19001:19001' # metrics
      - '30303:30303' # eth/66 peering
      - '8545:8545' # rpc
      - '8456:8456' # ws
      - '8551:8551' # engine
    volumes:
      - ./test-data/execution/logs:/opt/alys/execution/logs
      - ./test-data/execution/data:/opt/alys/execution/data
      - ./test-config:/opt/alys/execution/config
    pid: host
    environment:
      RUST_LOG: debug
      RUST_BACKTRACE: full
    command: > 
      node
      --dev
      --log.file.directory /opt/alys/execution/logs
      --datadir "/opt/alys/execution/data"
      --metrics 0.0.0.0:9001
      --authrpc.addr 0.0.0.0
      --authrpc.port 8551
      --authrpc.jwtsecret /opt/alys/execution/config/jwt.hex
      --http --http.addr 0.0.0.0 --http.port 8545
      --http.api "admin,debug,eth,net,trace,txpool,web3,rpc,reth"
      --http.corsdomain "*"
      --ws.api "admin,debug,eth,net,trace,txpool,web3,rpc,reth"
      --ws
      --ws.addr "0.0.0.0"
      --ws.port 8456
      --ws.origins "*"
      --port 30303
      --dev.block_time 2s

  consensus:
    container_name: consensus-test
    restart: unless-stopped
    build:
      context: ../
      dockerfile: etc/Dockerfile
      target: builder
    ports:
      - "3000:3000"
      - "55444:55444"
      - '9002:9001' # metrics (different port to avoid conflicts)
    volumes:
      - ./test-data/alys/db:/lib/alys/data/db
      - ./test-data/alys/wallet:/lib/alys/data/wallet
      - ./test-config/chain-test.json:/lib/alys/config/chain.json:ro
    environment:
      RUST_LOG: debug
      RUST_BACKTRACE: full
      TEST_MODE: "true"
    command:
      - /opt/alys/target/debug/app
      - --dev
      - --chain
      - /lib/alys/config/chain.json
      - --geth-url
      - http://execution:8551/
      - --db-path
      - /lib/alys/data/db
      - --wallet-path
      - /lib/alys/data/wallet
      - --bitcoin-rpc-url
      - http://bitcoin-core:18443
      - --bitcoin-rpc-user
      - rpcuser
      - --bitcoin-rpc-pass
      - rpcpassword
      - --geth-execution-url
      - http://execution:8545
      - --p2p-port
      - "55444"
    depends_on:
      - execution
      - bitcoin-core

volumes:
  test-logs:
    driver: local
  test-data:
    driver: local
```

## Testing Plan

### Unit Tests
```bash
# Run all unit tests with coverage
cargo test --all-features --workspace
cargo tarpaulin --out Html --output-dir coverage/
```

### Integration Tests
```bash
# Start test environment
docker-compose -f docker-compose.test.yml up -d

# Run integration tests
cargo test --test integration_tests --features integration

# Cleanup
docker-compose -f docker-compose.test.yml down -v
```

### Property Tests
```bash
# Run property-based tests with more iterations
PROPTEST_CASES=10000 cargo test --test property_tests
```

### Chaos Tests
```bash
# Run chaos testing suite
cargo test --test chaos_tests --features chaos --release
```

### Performance Tests
```bash
# Run benchmarks
cargo bench --features bench

# Compare with baseline
cargo bench --features bench -- --baseline main
```

## Dependencies

### Blockers
None

### Blocked By
- ALYS-001: Backup system needed for test recovery scenarios

### Related Issues
- ALYS-003: Metrics infrastructure for test reporting
- ALYS-004: CI/CD pipeline integration

## Definition of Done

- [ ] All test harnesses implemented and documented
- [ ] Property-based tests covering critical paths
- [ ] Chaos testing framework operational
- [ ] Performance benchmarks established
- [ ] CI/CD integration complete
- [ ] Test coverage > 80% for new code
- [ ] Test reports automatically generated
- [ ] Documentation updated with test guide

## Notes

- Use `nextest` for faster test execution
- Consider using `insta` for snapshot testing
- Implement test data generators for realistic scenarios
- Setup mutation testing with `cargo-mutants`

## Time Tracking

**Time Estimate**: 4-5 days (32-40 hours total) with detailed breakdown:
- Phase 1 - Test infrastructure foundation: 4-5 hours (includes framework design, configuration system, harness collection)
- Phase 2 - Actor testing framework: 8-10 hours (includes supervision testing, concurrent messaging, recovery scenarios)
- Phase 3 - Sync testing framework: 6-8 hours (includes P2P simulation, resilience testing, checkpoint validation)
- Phase 4 - Property-based testing: 4-6 hours (includes PropTest setup, custom generators, property definitions)
- Phase 5 - Chaos testing framework: 6-8 hours (includes chaos injection, Byzantine simulation, resource stress testing)
- Phase 6 - Performance benchmarking: 3-4 hours (includes Criterion setup, profiling integration, flamegraph generation)
- Phase 7 - CI/CD integration & reporting: 3-4 hours (includes Docker environment, reporting system, coverage analysis)

**Critical Path Dependencies**: Phase 1 → (Phase 2,3 in parallel) → Phase 4 → Phase 5 → (Phase 6,7 in parallel)
**Resource Requirements**: 1 senior developer with Rust testing experience, access to container orchestration
**Risk Buffer**: 25% additional time for framework integration issues and Docker environment setup
**Prerequisites**: ALYS-001 foundation must be complete for actor testing framework

- Actual: _To be filled_