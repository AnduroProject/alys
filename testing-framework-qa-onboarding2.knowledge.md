# Alys V2 Testing Framework: Complete QA Engineer Onboarding Guide

## Table of Contents

1. [Welcome & Overview](#welcome--overview)
2. [Local Development Environment Setup](#local-development-environment-setup)
3. [Understanding the Testing Framework Architecture](#understanding-the-testing-framework-architecture)
4. [Getting Started: Your First Tests](#getting-started-your-first-tests)
5. [Mastering Test Harnesses](#mastering-test-harnesses)
6. [Advanced Testing Patterns](#advanced-testing-patterns)
7. [CI/CD Integration & Automation](#cicd-integration--automation)
8. [Performance Testing & Benchmarking](#performance-testing--benchmarking)
9. [Chaos Engineering & Resilience Testing](#chaos-engineering--resilience-testing)
10. [Troubleshooting & Debugging](#troubleshooting--debugging)
11. [Pro Tips! 🚀](#pro-tips-)
12. [End-to-End Workflow Demonstrations](#end-to-end-workflow-demonstrations)
13. [Reference & Cheat Sheets](#reference--cheat-sheets)

---

## Welcome & Overview

Welcome to the Alys V2 Testing Framework! This comprehensive guide will transform you from a testing newcomer to a framework power user. The Alys V2 Migration Testing Framework is a sophisticated, multi-phase testing infrastructure designed to validate the complex migration from Alys V1 to V2.

### What Makes This Framework Special?

```mermaid
graph TD
    A[Alys V2 Testing Framework] --> B[7 Complete Phases]
    A --> C[62+ Metrics Collection Points]
    A --> D[Production-Ready CI/CD]
    
    B --> B1[Foundation & Infrastructure]
    B --> B2[Actor System Testing]
    B --> B3[Sync & Blockchain Testing]
    B --> B4[Property-Based Testing]
    B --> B5[Chaos Engineering]
    B --> B6[Performance Benchmarking]
    B --> B7[Complete CI/CD Integration]
    
    C --> C1[Migration Phase Tracking]
    C --> C2[Actor Performance Metrics]
    C --> C3[Sync Performance Tracking]
    C --> C4[System Resource Monitoring]
    
    D --> D1[Docker Test Environment]
    D --> D2[Test Coordinator Service]
    D --> D3[Comprehensive Reporting]
    D --> D4[Historical Trend Analysis]
```

### Framework Components Overview

The testing framework consists of 7 major phases with 28 completed tasks:

**✅ Phase 1: Foundation** (4 tasks) - Core infrastructure, configuration, harnesses, metrics
**✅ Phase 2: Actor Testing** (6 tasks) - Lifecycle, recovery, concurrency, messaging, overflow, communication  
**✅ Phase 3: Sync Testing** (5 tasks) - Full sync, resilience, checkpoints, parallel operations
**✅ Phase 4: Property-Based Testing** (4 tasks) - PropTest generators, invariant validation, edge cases
**✅ Phase 5: Chaos Engineering** (4 tasks) - Network chaos, resource stress, Byzantine attacks
**✅ Phase 6: Performance Benchmarking** (3 tasks) - Criterion.rs integration, profiling, flamegraphs
**✅ Phase 7: CI/CD Integration** (2 tasks) - Docker environment, comprehensive reporting

---

## Local Development Environment Setup

### Prerequisites

Before diving into the testing framework, ensure your development environment is properly configured.

#### System Requirements

```bash
# Operating System: macOS, Linux, or Windows with WSL2
# Memory: Minimum 8GB RAM (16GB recommended)
# Disk Space: At least 20GB free space
# CPU: Multi-core processor recommended
```

#### Essential Tools Installation

**1. Install Rust (Version 1.87.0+)**
```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version  # Should show 1.87.0+
cargo --version
```

**2. Install Docker & Docker Compose**
```bash
# macOS with Homebrew
brew install docker docker-compose

# Linux (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install docker.io docker-compose

# Verify installation
docker --version
docker-compose --version
```

**3. Install Additional Testing Tools**
```bash
# Install cargo testing extensions
cargo install cargo-tarpaulin  # Code coverage
cargo install cargo-nextest    # Faster test execution
cargo install cargo-watch      # File watching for tests
cargo install cargo-mutants    # Mutation testing
cargo install criterion        # Benchmarking
```

**4. Development Tools**
```bash
# Install build dependencies (Ubuntu/Debian)
sudo apt-get install build-essential clang cmake pkg-config libssl-dev

# macOS
xcode-select --install
brew install cmake openssl
```

#### Clone and Setup the Alys Repository

```bash
# Clone the repository
git clone https://github.com/AnduroProject/alys.git
cd alys

# Switch to the V2 development branch
git checkout v2

# Verify workspace structure
ls -la
# Should see: app/, crates/, contracts/, tests/, etc/
```

#### Build the Testing Framework

```bash
# Build the entire workspace including tests
cargo build

# Build with release optimizations (for performance testing)
cargo build --release

# Verify the testing framework builds successfully
cd tests
cargo check
cargo build

# Expected output: successful compilation with no errors
```

#### Initialize Test Environment

```bash
# Create test data directories
mkdir -p /tmp/alys-test-results
mkdir -p test-data/{bitcoin,execution,alys}

# Set up test configuration files
cp etc/config/chain.json test-config/chain-test.json

# Generate JWT secret for execution client
openssl rand -hex 32 > test-config/jwt.hex

# Verify Docker environment
docker-compose -f docker-compose.test.yml config
```

### Quick Verification Test

Let's verify your setup with a simple test run:

```bash
# Run a quick framework verification test
cd tests
cargo test --lib framework::test_framework_initialization -- --nocapture

# Expected output: Test passes showing framework initializes correctly
# If this fails, revisit the previous setup steps
```

---

## Understanding the Testing Framework Architecture

### Core Architecture Overview

The Alys V2 Testing Framework is built around a sophisticated architecture that supports comprehensive validation across all migration phases. Let's understand its key components:

```mermaid
graph TD
    A[MigrationTestFramework] --> B[Configuration System]
    A --> C[Test Harnesses Collection]
    A --> D[Validation System]  
    A --> E[Metrics Collection]
    A --> F[Runtime Management]
    
    B --> B1[TestConfig]
    B --> B2[Environment Presets]
    B --> B3[Docker Configuration]
    
    C --> C1[ActorTestHarness]
    C --> C2[SyncTestHarness]
    C --> C3[LighthouseCompatHarness]
    C --> C4[GovernanceIntegrationHarness]
    C --> C5[NetworkTestHarness]
    
    D --> D1[Phase Validators]
    D --> D2[Result Validators]
    D --> D3[Quality Gates]
    
    E --> E1[PhaseMetrics]
    E --> E2[ResourceMetrics]
    E --> E3[ExecutionMetrics]
    E --> E4[PerformanceMetrics]
    
    F --> F1[8-Worker Tokio Runtime]
    F --> F2[Async Test Execution]
    F --> F3[Parallel Coordination]
```

### Core Framework Structure

**Location:** `tests/src/framework/mod.rs:97-158`

The `MigrationTestFramework` is the central orchestrator that manages all testing operations:

```rust
pub struct MigrationTestFramework {
    runtime: Arc<Runtime>,           // Shared 8-worker Tokio runtime
    config: TestConfig,              // Environment-specific configuration
    harnesses: TestHarnesses,        // Collection of 5 specialized harnesses
    validators: Validators,          // Phase & result validation system
    metrics: MetricsCollector,       // Metrics collection & reporting
    start_time: SystemTime,          // Framework initialization timestamp
}
```

### Migration Phase Flow

The framework validates five migration phases sequentially:

```mermaid
graph TD
    A[Foundation Phase] --> B[ActorCore Phase]
    B --> C[SyncImprovement Phase]
    C --> D[LighthouseMigration Phase]
    D --> E[GovernanceIntegration Phase]
    
    A1[Framework Init<br/>Config Validation<br/>Harness Coordination] --> A
    B1[Actor Lifecycle<br/>Message Ordering<br/>Recovery Testing] --> B
    C1[Full Sync Testing<br/>Network Resilience<br/>Parallel Sync] --> C
    D1[API Compatibility<br/>Consensus Integration] --> D
    E1[Workflow Testing<br/>Signature Validation] --> E
```

### Test Harness Pattern

Each harness implements the common `TestHarness` trait for consistency:

**Location:** `tests/src/framework/harness/mod.rs:21-98`

```rust
pub trait TestHarness: Send + Sync {
    fn name(&self) -> &str;
    async fn health_check(&self) -> bool;
    async fn initialize(&mut self) -> Result<()>;
    async fn run_all_tests(&self) -> Vec<TestResult>;
    async fn shutdown(&self) -> Result<()>;
    async fn get_metrics(&self) -> serde_json::Value;
}
```

---

## Getting Started: Your First Tests

### Understanding Test Configuration

Before running any tests, you need to understand the configuration system. The framework uses environment-specific configurations:

**Location:** `tests/src/framework/config.rs:16-162`

```rust
pub struct TestConfig {
    pub parallel_tests: bool,                    // Enable parallel execution
    pub chaos_enabled: bool,                     // Enable chaos testing
    pub performance_tracking: bool,              // Enable perf metrics
    pub coverage_enabled: bool,                  // Enable code coverage
    pub docker_compose_file: String,             // Test environment setup
    pub test_data_dir: PathBuf,                 // Temporary test data
    pub network: NetworkConfig,                 // P2P network settings
    pub actor_system: ActorSystemConfig,        // Actor testing config
    pub sync: SyncConfig,                       // Sync testing config
    pub performance: PerformanceConfig,         // Performance testing
    pub chaos: ChaosConfig,                     // Chaos testing setup
}
```

### Configuration Presets

The framework provides two main presets:

```bash
# Development preset - debugging-friendly settings
TestConfig::development()  # Defined at config.rs:218-232

# CI/CD preset - optimized for automation  
TestConfig::ci_cd()       # Defined at config.rs:240-254
```

### Your First Test Run

Let's start with basic framework functionality:

```bash
# 1. Run the foundation phase tests
cd tests
cargo test --lib foundation --verbose

# 2. Check test output - you should see:
# - Framework initialization ✓
# - Configuration validation ✓
# - Harness coordination ✓
# - Metrics collection setup ✓

# 3. Run with coverage tracking
cargo tarpaulin --out Html --output-dir coverage/ --skip-clean
```

### Running Actor System Tests

The actor system is critical to Alys V2. Let's test it:

```bash
# Run all actor tests
cargo test --lib actor --verbose

# Run specific actor test categories
cargo test --lib test_actor_lifecycle_tests
cargo test --lib test_message_ordering_tests  
cargo test --lib test_recovery_tests

# Expected output shows 18 specialized test methods across 6 categories:
# - Lifecycle Testing (3 tests)
# - Message Ordering (3 tests) 
# - Recovery Testing (3 tests)
# - Overflow Testing (6 tests)
# - Cross-Actor Communication (6 tests)
```

### Understanding Test Results

When tests run, you'll see structured output like this:

```
test framework::harness::actor::test_actor_creation_lifecycle ... ok (125ms)
test framework::harness::actor::test_concurrent_message_processing ... ok (87ms)  
test framework::harness::actor::test_panic_injection_recovery ... ok (156ms)
```

Each test provides:
- **Test name** - Describes what's being tested
- **Status** - `ok` (passed), `FAILED` (failed), or `ignored` (skipped)
- **Duration** - Execution time in milliseconds

---

## Mastering Test Harnesses

The testing framework uses specialized harnesses for different system components. Let's dive deep into each one:

### ActorTestHarness - Complete Actor System Testing

**Location:** `tests/src/framework/harness/actor.rs:25-146`

The ActorTestHarness is the most comprehensive harness, providing 18 specialized test methods across 6 categories:

#### 1. Lifecycle Testing (3 tests)
```rust
// Core lifecycle test methods - Location: actor.rs:1763-1951
pub async fn test_actor_creation_lifecycle(&self) -> TestResult
pub async fn test_actor_supervision_tree(&self) -> TestResult  
pub async fn test_actor_state_transitions(&self) -> TestResult
```

**Practical Example:**
```bash
# Run lifecycle tests with detailed output
cargo test test_actor_creation_lifecycle -- --nocapture

# This tests the complete actor lifecycle:
# Uninitialized → Starting → Running → Stopping → Stopped
#        ↓            ↓
#     Failed ← → Recovering
```

#### 2. Recovery Testing (3 tests)
```rust
// Recovery testing methods - Location: actor.rs:1953-2159
pub async fn test_panic_injection_recovery(&self) -> TestResult
pub async fn test_supervisor_restart_validation(&self) -> TestResult
pub async fn test_cascading_failure_prevention(&self) -> TestResult
```

**What happens during recovery testing:**
- Deliberate actor failure simulation with various failure modes
- Automatic restart validation with configurable strategies
- Protection against failure propagation across actor hierarchies
- Recovery metrics collection (success rates, restart times, stability)

#### 3. Concurrent Message Testing (3 tests)
```rust
// High-concurrency validation - Location: actor.rs:2161-2326
pub async fn test_concurrent_message_processing(&self) -> TestResult
pub async fn test_high_throughput_messaging(&self) -> TestResult
pub async fn test_message_load_balancing(&self) -> TestResult
```

**Performance Targets:**
- **Message Volume**: 1000+ concurrent messages
- **Processing Rate**: 100+ messages/second throughput
- **Latency**: Sub-100ms average message processing time
- **Success Rate**: 99%+ successful message delivery

#### 4. Message Ordering Verification (3 tests)
```rust  
// Ordering verification methods - Location: actor.rs:2328-2520
pub async fn test_fifo_message_ordering(&self) -> TestResult
pub async fn test_priority_message_ordering(&self) -> TestResult  
pub async fn test_concurrent_ordering_verification(&self) -> TestResult
```

#### 5. Mailbox Overflow Testing (6 tests)
```rust
// Comprehensive overflow scenarios - Location: actor.rs:3077-3259
pub async fn test_mailbox_overflow_detection(&self) -> TestResult
pub async fn test_backpressure_mechanisms(&self) -> TestResult
pub async fn test_overflow_recovery(&self) -> TestResult
pub async fn test_message_dropping_policies(&self) -> TestResult
pub async fn test_overflow_under_load(&self) -> TestResult
pub async fn test_cascading_overflow_prevention(&self) -> TestResult
```

#### 6. Cross-Actor Communication (6 tests)
```rust
// Communication patterns - Location: actor.rs:3261-3730
pub async fn test_direct_actor_messaging(&self) -> TestResult
pub async fn test_broadcast_messaging(&self) -> TestResult  
pub async fn test_request_response_patterns(&self) -> TestResult
pub async fn test_message_routing_chains(&self) -> TestResult
pub async fn test_multi_actor_workflows(&self) -> TestResult
pub async fn test_actor_discovery_communication(&self) -> TestResult
```

**Communication Patterns Tested:**
1. **Direct Messaging**: Point-to-point communication between two actors
2. **Broadcast Messaging**: One-to-many communication pattern
3. **Request-Response**: RPC-style communication patterns
4. **Message Routing Chains**: Pipeline processing through actor chains
5. **Multi-Actor Workflows**: Complex distributed workflow orchestration
6. **Actor Discovery**: Dynamic service discovery and communication

### SyncTestHarness - Blockchain Synchronization Testing

**Location:** `tests/src/framework/harness/sync.rs:21-37`

The SyncTestHarness tests blockchain synchronization with sophisticated P2P network simulation:

```rust
pub struct SyncTestHarness {
    config: SyncConfig,                     // Sync configuration
    runtime: Arc<Runtime>,                  // Shared runtime
    mock_network: MockP2PNetwork,           // Complete peer simulation
    simulated_chain: SimulatedBlockchain,   // Genesis blocks, checkpoints, forks
    metrics: SyncHarnessMetrics,           // Sync performance metrics
}
```

#### Full Sync Testing (10,000+ Blocks)
```bash
# Test full blockchain sync
cargo test test_genesis_to_tip_sync -- --nocapture

# Test configurable large chain sync
cargo test test_full_sync_large_chain -- --nocapture

# This tests:
# - Large scale testing: 10,000+ block synchronization capability
# - Batch processing: Efficient 1000-block batch sync with validation
# - Progressive validation: Checkpoint validation throughout sync process
# - Performance metrics: Blocks/second throughput and validation counts
```

#### Resilience Testing
```bash
# Test sync with network failures
cargo test test_cascading_peer_disconnections -- --nocapture
cargo test test_network_partition_tolerance -- --nocapture  
cargo test test_message_corruption_handling -- --nocapture
```

**Failure Scenarios Tested:**
1. **Network Partitions**: Split network into isolated groups
2. **Peer Disconnections**: Random and cascading peer failures
3. **Message Corruption**: Invalid message handling and recovery
4. **Slow Peers**: Latency injection and timeout handling
5. **Cascading Failures**: Multi-peer failure propagation testing

#### Parallel Sync Testing
```bash
# Test parallel sync scenarios
cargo test test_concurrent_sync_sessions -- --nocapture
cargo test test_multi_peer_load_balancing -- --nocapture
cargo test test_race_condition_handling -- --nocapture
```

### Other Specialized Harnesses

#### LighthouseCompatHarness
**Location:** `tests/src/framework/harness/lighthouse.rs`
- **Purpose**: Lighthouse consensus client compatibility testing
- **Key Features**: API compatibility, consensus protocol integration

#### GovernanceIntegrationHarness
**Location:** `tests/src/framework/harness/governance.rs`
- **Purpose**: Governance workflow and signature validation testing
- **Key Features**: BLS signatures, multi-signature validation, proposal workflows

#### NetworkTestHarness  
**Location:** `tests/src/framework/harness/network.rs`
- **Purpose**: P2P networking and communication testing
- **Key Features**: Peer discovery, message propagation, network resilience

---

## Advanced Testing Patterns

### Property-Based Testing with PropTest

**Location:** `tests/src/framework/generators.rs`

The framework includes 50+ generator functions covering all major Alys blockchain data structures:

#### Blockchain Data Structure Generators
```rust
// Core blockchain structures
pub fn signed_block_strategy() -> impl Strategy<Value = SignedBlock>
pub fn mined_block_strategy() -> impl Strategy<Value = MinedBlock>
pub fn transaction_strategy() -> impl Strategy<Value = Transaction>
pub fn auxpow_strategy() -> impl Strategy<Value = AuxPoW>
pub fn bitcoin_block_header_strategy() -> impl Strategy<Value = BitcoinBlockHeader>
```

#### Running Property Tests
```bash
# Run all property tests
cargo test --test minimal_property_tests
cargo test --test sync_checkpoint_property_tests  
cargo test --test governance_signature_property_tests

# Run with increased test cases for thorough validation
PROPTEST_CASES=10000 cargo test --test property_tests
```

#### Key Properties Validated

**Actor Message Ordering Properties:**
- **Sequence Preservation**: Monotonic sequence numbers within same sender
- **Priority Ordering**: Critical → High → Normal → Low priority enforcement
- **FIFO Within Priority**: First-in-first-out within same priority level
- **Throughput Requirements**: Minimum 100 messages/second processing rate

**Sync Checkpoint Consistency Properties:**
- **Consistency Maintenance**: Checkpoints remain consistent despite failures
- **Interval Consistency**: All checkpoints follow same interval pattern
- **Recovery Effectiveness**: System recovers verifiable checkpoints
- **Byzantine Resilience**: System maintains functionality under Byzantine failures

**Governance Signature Validation Properties:**
- **Byzantine Attack Detection**: Malicious signatures identified and rejected
- **Threshold Enforcement**: Signature weight thresholds correctly enforced
- **Double Signing Detection**: Multiple signatures from same signer detected
- **Byzantine Tolerance**: System rejects proposals exceeding Byzantine tolerance

### State Machine Testing Pattern

The framework uses state machine patterns for lifecycle validation:

```rust
pub enum ActorState {
    Uninitialized → Starting → Running → Stopping → Stopped
                      ↓            ↓
                   Failed ← → Recovering
}
```

### Event Sourcing for Validation

All test events are captured for analysis and replay:

```rust
pub struct TestEvent {
    pub event_id: EventId,
    pub timestamp: SystemTime,
    pub event_type: TestEventType,  // ActorCreated, MessageSent, etc.
    pub source: EventSource,
    pub metadata: EventMetadata,
}
```

---

## CI/CD Integration & Automation

### Docker Test Environment

The framework provides a complete containerized test environment with Bitcoin regtest and Reth execution layer.

**Location:** `tests/docker-compose.test.yml`

#### Starting the Test Environment
```bash
# Start complete test environment
docker-compose -f docker-compose.test.yml up -d

# Check service health
curl http://localhost:8080/health

# View logs
docker-compose -f docker-compose.test.yml logs -f
```

#### Services Included
```yaml
# Bitcoin Core Regtest
bitcoin-core:
  - Port: 18443 (RPC)
  - Features: Full regtest environment with ZMQ pub/sub
  - Configuration: 6-confirmation requirement, full RPC access

# Reth Execution Client  
execution:
  - Ports: 8545 (JSON-RPC), 8551 (Engine API)
  - Features: Ethereum-compatible execution layer
  - Configuration: 2-second block times, full API support

# Alys Consensus Client
consensus:
  - Ports: 3000 (Consensus RPC), 55444 (P2P)
  - Features: Hybrid PoA/PoW consensus, federation integration
  - Configuration: Peg-in/peg-out capability, P2P networking

# Prometheus Monitoring
prometheus:
  - Port: 9090
  - Features: Metrics collection from all services
  - Configuration: 5-second scrape intervals, 24-hour retention

# Grafana Visualization  
grafana:
  - Port: 3001
  - Features: Real-time dashboards for test metrics
  - Configuration: Service health monitoring, system performance
```

### Test Coordinator Service

**Location:** `tests/src/bin/test_coordinator.rs` (944 lines)

The test coordinator orchestrates test execution, monitors service health, and collects results:

#### Starting the Test Coordinator
```bash
# Start the test coordinator
cargo run --bin test_coordinator

# Access the web dashboard
open http://localhost:8081

# API endpoints available:
# GET  /health              - Service health check
# GET  /status              - Comprehensive service status
# GET  /test-runs           - List all test runs
# POST /test-runs           - Create new test run
# GET  /metrics             - Prometheus metrics
```

#### API Usage Examples
```bash
# Create a new test run
curl -X POST http://localhost:8080/test-runs \
  -H "Content-Type: application/json" \
  -d '{
    "test_suite": "full_migration_validation",
    "configuration": "ci_cd",
    "parallel_execution": true
  }'

# Check test run status
TEST_RUN_ID=$(curl -s http://localhost:8080/test-runs | jq -r '.[-1].id')
curl http://localhost:8080/test-runs/$TEST_RUN_ID
```

### Comprehensive Test Execution Script

**Location:** `tests/scripts/run_comprehensive_tests.sh` (423 lines)

```bash
# Run all test categories
./tests/scripts/run_comprehensive_tests.sh

# Run specific test category
./tests/scripts/run_comprehensive_tests.sh unit
./tests/scripts/run_comprehensive_tests.sh performance
./tests/scripts/run_comprehensive_tests.sh coverage
./tests/scripts/run_comprehensive_tests.sh chaos
```

### GitHub Actions Integration

```yaml
# Example CI/CD integration
- name: Start Test Environment
  run: docker-compose -f tests/docker-compose.test.yml up -d

- name: Wait for Service Health
  run: curl --retry 30 --retry-delay 2 http://localhost:8080/health

- name: Execute Test Suite
  run: |
    export TEST_RUN_ID=$(uuidgen)
    ./tests/scripts/run_comprehensive_tests.sh
    
- name: Generate Reports
  run: curl -X POST http://localhost:8080/test-runs

- name: Archive Results
  uses: actions/upload-artifact@v3
  with:
    name: test-results
    path: /tmp/alys-test-results/
```

### Quality Gates & Success Criteria

The framework enforces strict quality gates:

- **Unit Test Success Rate**: 100% required
- **Integration Test Success Rate**: 95% required
- **Code Coverage Threshold**: 80% minimum
- **Performance Regression**: 20% degradation threshold
- **Chaos Test Resilience**: 80% success rate required

---

## Performance Testing & Benchmarking

### Criterion.rs Integration

**Location:** `tests/benches/`

The framework includes comprehensive benchmarking with 17 different benchmark types:

#### Actor Performance Benchmarks
```bash
# Run actor benchmarks
cargo bench --bench actor_benchmarks

# Specific benchmark categories:
cargo bench message_processing_throughput
cargo bench actor_creation_performance  
cargo bench concurrent_message_handling
cargo bench memory_usage_patterns
cargo bench mailbox_overflow_handling
cargo bench cross_actor_communication
```

**Performance Targets:**
- **Message Volume**: 1000+ concurrent messages
- **Processing Rate**: 100+ messages/second throughput  
- **Latency**: Sub-100ms average message processing
- **Success Rate**: 99%+ successful message delivery

#### Sync Performance Benchmarks
```bash
# Run sync benchmarks
cargo bench --bench sync_benchmarks

# Benchmark categories:
cargo bench block_processing_rate        # Target: >500 blocks/second
cargo bench parallel_block_processing    # 1-8 parallel workers
cargo bench checkpoint_validation        # 10-250 block intervals
cargo bench network_failure_resilience   # 0-20% failure rates
cargo bench peer_coordination           # 1-10 peers
cargo bench memory_usage_during_sync    # Batch efficiency
cargo bench transaction_throughput      # 1-100 tx/block
```

#### System Profiling Benchmarks
```bash
# Run system benchmarks
cargo bench --bench system_benchmarks

# Benchmark categories:
cargo bench cpu_intensive_operations     # 1K-1M operations
cargo bench memory_allocation_patterns   # Sequential, scattered, chunked
cargo bench concurrent_stress_testing    # 1-8 workers
cargo bench memory_fragmentation        # Allocation/deallocation cycles
cargo bench stack_vs_heap_performance   # Performance comparison
cargo bench cache_performance_analysis  # L1, L2, L3 cache levels
cargo bench async_task_overhead         # Task spawning costs
```

### Flamegraph Generation

The framework includes automated flamegraph generation for performance analysis:

```bash
# Generate flamegraphs during benchmarks
FLAMEGRAPH=1 cargo bench --bench system_benchmarks

# View generated flamegraphs
open target/performance/flamegraph.svg

# Generated files:
# - target/performance/flamegraph.svg
# - target/performance/cpu_profile.json  
# - target/performance/memory_profile.json
```

### Performance Reports

The framework generates comprehensive performance reports:

```rust
pub struct PerformanceReport {
    pub benchmarks: Vec<BenchmarkResult>,
    pub regressions: Vec<PerformanceRegression>,
    pub improvements: Vec<PerformanceImprovement>,
    pub flamegraph_path: Option<PathBuf>,
    pub cpu_profile_path: Option<PathBuf>,
    pub memory_profile_path: Option<PathBuf>,
    pub performance_score: f64,  // 0-100 score
    pub generated_at: SystemTime,
    pub environment_info: EnvironmentInfo,
}
```

#### Viewing Performance Results
```bash
# HTML Reports
open target/criterion/*/report/index.html

# Performance Summary
cat target/performance/performance_report.json | jq '.performance_score'

# Regression Analysis
cat target/performance/performance_report.json | jq '.regressions'
```

---

## Chaos Engineering & Resilience Testing

### ChaosTestFramework Overview

**Location:** `tests/src/framework/chaos.rs:22-43`

The framework provides 17 comprehensive chaos event types across three categories:

```mermaid
graph TD
    A[ChaosTestFramework] --> B[Network Chaos]
    A --> C[Resource Chaos]  
    A --> D[Byzantine Chaos]
    
    B --> B1[Network Partitions]
    B --> B2[Latency Injection]
    B --> B3[Message Corruption]
    B --> B4[Peer Disconnections]
    B --> B5[Network Congestion]
    
    C --> C1[Memory Pressure]
    C --> C2[CPU Stress Testing]
    C --> C3[Disk I/O Failures]
    C --> C4[File System Corruption]
    
    D --> D1[Malicious Actors]
    D --> D2[Consensus Attacks]
    D --> D3[Sybil Attacks]
    D --> D4[Data Corruption]
```

### Running Chaos Tests

#### Network Chaos Testing
```bash
# Test network resilience
cargo test --test chaos_tests test_network_partition_resilience -- --nocapture
cargo test --test chaos_tests test_latency_injection_tolerance -- --nocapture
cargo test --test chaos_tests test_message_corruption_recovery -- --nocapture

# Expected behavior:
# - Network partitions lasting 10-60 seconds
# - Latency injection of 100ms-5s with 50ms jitter
# - Message corruption rates of 1-10%
# - Automatic network recovery validation
```

#### Resource Chaos Testing
```bash
# Test resource exhaustion scenarios  
cargo test --test chaos_tests test_memory_pressure_handling -- --nocapture
cargo test --test chaos_tests test_cpu_stress_resilience -- --nocapture
cargo test --test chaos_tests test_disk_failure_tolerance -- --nocapture

# Expected behavior:
# - Memory pressure up to 80% system utilization
# - CPU stress testing up to 90% utilization
# - Disk I/O failure rates of 5-25%
# - Graceful degradation validation
```

#### Byzantine Behavior Testing
```bash
# Test Byzantine fault tolerance
cargo test --test chaos_tests test_malicious_actor_injection -- --nocapture
cargo test --test chaos_tests test_consensus_attack_resistance -- --nocapture
cargo test --test chaos_tests test_sybil_attack_prevention -- --nocapture

# Attack patterns tested:
# - Double signing detection
# - Vote flipping prevention  
# - Message withholding tolerance
# - Fake proposal rejection
# - Invalid signature detection
```

### Chaos Event Types

#### Network Chaos Events
```rust
pub enum ChaosEvent {
    NetworkPartition { 
        partition_groups: Vec<Vec<String>>, 
        duration: Duration 
    },
    NetworkLatencyInjection { 
        target_peers: Vec<String>, 
        latency: Duration, 
        jitter: Duration 
    },
    MessageCorruption { 
        corruption_rate: f64, 
        target_message_types: Vec<String>, 
        duration: Duration 
    },
    PeerDisconnection { 
        target_peers: Vec<String>, 
        reconnect_delay: Duration 
    },
    NetworkCongestion { 
        congestion_level: f64, 
        duration: Duration 
    }
}
```

#### Byzantine Attack Types  
```rust
pub enum AttackPattern {
    DoubleSigning,           // Sign conflicting blocks
    VoteFlipping,            // Change vote after commitment
    MessageWithholding,      // Withhold critical messages
    FakeProposals,          // Submit invalid proposals
    ConsensusDelay,         // Delay consensus participation
    InvalidSignatures,      // Submit cryptographically invalid signatures
}

pub enum ConsensusAttackType {
    NothingAtStake,         // Vote for multiple competing chains
    LongRangeAttack,        // Attempt to rewrite historical blocks
    FinalizationStall,      // Prevent consensus finalization
    ValidatorCartels,       // Coordinated validator collusion
}
```

### System Health Monitoring

During chaos testing, the framework continuously monitors system health:

```rust
pub struct SystemHealthMonitor {
    config: HealthMonitoringConfig,
    metrics: HealthMetrics,
    component_status: HashMap<String, ComponentHealth>,
    health_history: VecDeque<HealthSnapshot>,
}
```

#### Health Monitoring Features
- **Continuous Monitoring**: Real-time health tracking during chaos injection
- **Component Health**: Individual component health status monitoring  
- **Recovery Detection**: Automatic detection of system recovery after chaos events
- **Resilience Scoring**: Quantitative resilience scoring based on recovery performance
- **Baseline Comparison**: Health metric comparison against pre-chaos baselines

### Success Criteria & Quality Gates

**Chaos Testing Quality Gates:**
- **Chaos Injection Success**: 95%+ successful chaos event injection and execution
- **Recovery Validation**: 80%+ system recovery success rate after chaos events
- **Health Monitoring**: Continuous health tracking with sub-second monitoring intervals
- **Byzantine Tolerance**: Correct Byzantine fault tolerance threshold enforcement
- **Network Resilience**: System functionality maintenance during network failures
- **Resource Management**: Graceful degradation under resource pressure scenarios

---

## Troubleshooting & Debugging

### Common Issues & Solutions

#### Test Environment Issues

**Docker Services Not Starting:**
```bash
# Check Docker status
docker system info

# Check port conflicts
netstat -tlnp | grep :8545
netstat -tlnp | grep :18443

# Clean up previous containers
docker-compose -f docker-compose.test.yml down -v
docker system prune -f

# Restart with fresh environment
docker-compose -f docker-compose.test.yml up -d
```

**Service Health Check Failures:**
```bash
# Check individual service health
curl http://localhost:8081/health      # Test coordinator
curl http://localhost:8545/           # Execution client
curl http://bitcoin:rpcpassword@localhost:18443/ # Bitcoin Core

# Check logs for errors
docker-compose -f docker-compose.test.yml logs bitcoin-core
docker-compose -f docker-compose.test.yml logs execution  
docker-compose -f docker-compose.test.yml logs consensus
```

#### Test Execution Issues

**Tests Failing with Timeout Errors:**
```bash
# Increase timeout settings
export TEST_TIMEOUT=300  # 5 minutes
export RUST_LOG=debug

# Run with verbose output
cargo test --verbose -- --nocapture

# Check for resource constraints
htop
df -h
```

**Actor Tests Failing:**
```bash
# Debug actor system issues
cargo test test_actor_creation_lifecycle --verbose -- --nocapture

# Common issues:
# - Insufficient memory for 1000+ concurrent actors
# - Race conditions in message ordering tests
# - Supervisor restart timing issues

# Solutions:
export ACTOR_TEST_SCALE_FACTOR=0.5  # Reduce test scale
export ACTOR_TIMEOUT_MS=5000        # Increase timeouts
```

**Sync Tests Failing:**
```bash
# Debug sync system issues
cargo test test_full_sync_large_chain --verbose -- --nocapture

# Common issues:
# - Network simulation timing issues
# - Mock blockchain generation failures
# - P2P message handling race conditions

# Solutions:
export SYNC_TEST_CHAIN_SIZE=1000    # Reduce chain size
export MOCK_NETWORK_LATENCY=10      # Reduce simulated latency
```

#### Performance Test Issues

**Benchmarks Running Slowly:**
```bash
# Build in release mode for accurate benchmarks
cargo build --release

# Run benchmarks with optimizations
cargo bench --release

# Disable debug assertions for performance tests
export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false
```

**Memory Issues During Testing:**
```bash
# Monitor memory usage during tests
watch -n 1 'ps aux | grep -E "(cargo|alys)" | head -10'

# Increase available memory
ulimit -v 16777216  # 16GB virtual memory limit

# Run tests sequentially to reduce memory pressure
cargo test --jobs 1
```

### Debug Logging & Tracing

#### Enabling Debug Logs
```bash
# Enable comprehensive debug logging
export RUST_LOG=debug
export RUST_BACKTRACE=full

# Framework-specific logging
export RUST_LOG=alys_test_framework=debug,tokio=debug

# Test-specific logging  
export RUST_LOG=tests::framework::harness::actor=trace
export RUST_LOG=tests::framework::harness::sync=trace
```

#### Log Analysis
```bash
# Analyze test execution logs
tail -f /tmp/alys-test-results/test-execution.log

# Filter for specific components
grep "ActorTestHarness" /tmp/alys-test-results/test-execution.log
grep "SyncTestHarness" /tmp/alys-test-results/test-execution.log
grep "ERROR\|WARN" /tmp/alys-test-results/test-execution.log
```

### Test Data Debugging

#### Inspecting Test Data
```bash
# View test configuration
cat test-config/chain-test.json | jq '.'

# Check test data directories
ls -la test-data/
find test-data/ -type f -name "*.log" | head -5

# Examine test metrics
cat /tmp/alys-test-results/metrics.json | jq '.phase_metrics'
```

#### Database Debugging
```bash
# Connect to test coordinator database
sqlite3 /tmp/alys-test-coordinator.db

# Query test run history
.mode table
SELECT id, timestamp, status, success_rate FROM test_runs ORDER BY timestamp DESC LIMIT 10;

# Analyze test results
SELECT test_name, success, avg(duration) FROM test_results GROUP BY test_name;
```

---

## Pro Tips! 🚀

### Productivity Hacks

#### 1. Fast Test Iteration
```bash
# Use cargo-watch for continuous testing
cargo install cargo-watch
cargo watch -x 'test --lib framework::test_framework_initialization'

# Use nextest for faster parallel execution
cargo install cargo-nextest  
cargo nextest run

# Skip expensive tests during development
cargo test --lib -- --skip test_full_sync_large_chain
```

#### 2. Smart Test Selection
```bash
# Run only actor tests
cargo test actor

# Run only sync tests  
cargo test sync

# Run tests matching a pattern
cargo test "test_.*_lifecycle"

# Run tests for a specific phase
cargo test --lib foundation
cargo test --lib performance
```

#### 3. Configuration Shortcuts
```bash
# Create development config alias
alias test-dev='cargo test --lib --config env.TEST_CONFIG=development'

# Create CI config alias  
alias test-ci='cargo test --lib --config env.TEST_CONFIG=ci_cd'

# Quick Docker environment
alias start-test-env='docker-compose -f docker-compose.test.yml up -d'
alias stop-test-env='docker-compose -f docker-compose.test.yml down -v'
```

### Advanced Commands

#### 1. Coverage Analysis
```bash
# Generate detailed coverage report
cargo tarpaulin --out Html --output-dir coverage/ \
  --skip-clean --verbose --timeout 300

# Coverage with branch analysis
cargo tarpaulin --out Json --output-dir coverage/ \
  --branch --forward --force-clean

# View coverage in browser
open coverage/tarpaulin-report.html
```

#### 2. Performance Analysis
```bash
# Benchmark with profiling
cargo bench --bench actor_benchmarks -- --profile-time=30

# Generate flamegraphs
FLAMEGRAPH=1 cargo bench --bench system_benchmarks

# Compare benchmarks over time
cargo bench --bench sync_benchmarks -- --save-baseline main
git checkout feature-branch
cargo bench --bench sync_benchmarks -- --baseline main
```

#### 3. Property Test Tuning
```bash
# Run property tests with custom parameters
PROPTEST_CASES=5000 PROPTEST_MAX_SHRINK_ITERS=10000 \
  cargo test --test governance_signature_property_tests

# Generate test failure cases
PROPTEST_VERBOSE=1 cargo test --test property_tests 2>&1 | \
  grep -A 10 "Test failed"
```

#### 4. Chaos Testing Optimization
```bash
# Run specific chaos scenarios
cargo test test_network_partition_resilience \
  --features chaos -- --nocapture \
  --test-threads=1

# Custom chaos configuration
export CHAOS_DURATION=300          # 5 minutes
export CHAOS_EVENT_INTERVAL=10     # 10 seconds between events
export CHAOS_RECOVERY_TIMEOUT=60   # 1 minute recovery validation
```

#### 5. Database Query Shortcuts
```bash
# Create useful database aliases
alias test-db='sqlite3 /tmp/alys-test-coordinator.db'
alias test-metrics='sqlite3 /tmp/alys-test-coordinator.db "SELECT * FROM latest_test_run_summary;"'
alias test-coverage='sqlite3 /tmp/alys-test-coordinator.db "SELECT * FROM coverage_trends ORDER BY timestamp DESC LIMIT 10;"'
```

### Essential Environment Variables

```bash
# Create a .env file for consistent configuration
cat > tests/.env << 'EOF'
# Test Configuration
TEST_CONFIG=development
TEST_TIMEOUT=300
TEST_DATA_DIR=/tmp/alys-test-results
TEST_PARALLEL_JOBS=4

# Logging Configuration
RUST_LOG=info
RUST_BACKTRACE=1

# Docker Configuration  
DOCKER_COMPOSE_FILE=docker-compose.test.yml
DOCKER_HOST_DATA_DIR=./test-data

# Performance Configuration
CRITERION_SAMPLE_SIZE=100
FLAMEGRAPH_ENABLED=false
MEMORY_PROFILING=false

# Chaos Configuration
CHAOS_ENABLED=false
CHAOS_DURATION=60
CHAOS_EVENT_INTERVAL=10

# PropTest Configuration
PROPTEST_CASES=1000
PROPTEST_MAX_SHRINK_ITERS=1000
PROPTEST_MAX_LOCAL_REJECTS=100
EOF

# Load environment variables
source tests/.env
```

### IDE Integration Tips

#### VS Code Configuration
```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.features": ["testing"],
    "rust-analyzer.checkOnSave.command": "test",
    "rust-analyzer.checkOnSave.extraArgs": ["--lib"],
    "rust-analyzer.lens.enable": true,
    "rust-analyzer.lens.run": true,
    "rust-analyzer.lens.implementations": true,
    "rust-analyzer.runnables.cargoExtraArgs": [
        "--features", "testing"
    ]
}
```

#### VS Code Tasks
```json
// .vscode/tasks.json  
{
    "version": "2.0.0",
    "tasks": [
        {
            "label": "Test Framework Quick Check",
            "type": "shell",
            "command": "cargo",
            "args": ["test", "--lib", "framework::test_framework_initialization"],
            "group": "test",
            "presentation": {
                "echo": true,
                "reveal": "always",
                "focus": false,
                "panel": "shared"
            }
        },
        {
            "label": "Run All Actor Tests",
            "type": "shell", 
            "command": "cargo",
            "args": ["test", "--lib", "actor", "--", "--nocapture"],
            "group": "test"
        },
        {
            "label": "Start Test Environment",
            "type": "shell",
            "command": "docker-compose",
            "args": ["-f", "docker-compose.test.yml", "up", "-d"],
            "group": "build"
        }
    ]
}
```

### Useful Aliases & Functions

```bash
# Add to ~/.bashrc or ~/.zshrc
alias tt='cargo test --lib'                                    # Quick test
alias ttf='cargo test --lib -- --nocapture'                   # Test with output
alias ttw='cargo watch -x "test --lib"'                       # Watch tests
alias tth='cargo test --help'                                 # Test help
alias ttc='cargo test --lib && cargo tarpaulin --skip-clean'  # Test + coverage

# Test result analysis function
analyze_test_results() {
    local log_file="/tmp/alys-test-results/test-execution.log"
    echo "=== Test Summary ==="
    grep -E "(test result:|passed:|failed:)" "$log_file" | tail -10
    echo -e "\n=== Recent Failures ==="  
    grep -A 5 "FAILED" "$log_file" | tail -20
    echo -e "\n=== Performance Summary ==="
    grep -E "Duration:|Throughput:" "$log_file" | tail -10
}

# Quick environment check function
check_test_env() {
    echo "=== Environment Status ==="
    echo "Rust version: $(rustc --version)"
    echo "Cargo version: $(cargo --version)"
    echo "Docker version: $(docker --version)"
    echo "Test data dir: $TEST_DATA_DIR"
    echo -e "\n=== Service Status ==="
    curl -s http://localhost:8081/health | jq '.' || echo "Test coordinator not running"
    docker-compose -f docker-compose.test.yml ps
}

# Benchmark comparison function
compare_benchmarks() {
    local baseline=${1:-main}
    echo "Comparing benchmarks against baseline: $baseline"
    cargo bench -- --save-baseline "$baseline"
    echo "Baseline saved. Run tests and then execute:"
    echo "cargo bench -- --baseline $baseline"
}
```

### Quick Reference Commands

```bash
# Essential Commands Quick Reference
cargo test --lib                                    # Run all library tests
cargo test --lib framework                          # Run framework tests
cargo test --lib actor                             # Run actor tests  
cargo test --lib sync                              # Run sync tests
cargo test --test property_tests                   # Run property tests
cargo test --test chaos_tests                      # Run chaos tests
cargo bench --bench actor_benchmarks               # Run actor benchmarks
cargo tarpaulin --out Html                         # Generate coverage report
docker-compose -f docker-compose.test.yml up -d    # Start test environment
./tests/scripts/run_comprehensive_tests.sh         # Run complete test suite

# Debugging Commands
RUST_LOG=debug cargo test --lib -- --nocapture     # Debug logging
cargo test --lib -- --test-threads=1               # Single-threaded tests
cargo test --lib -- --ignored                      # Run ignored tests  
cargo test --lib -- --exact test_name              # Run specific test
strace -e trace=network cargo test sync            # Trace network calls

# Performance Commands  
cargo build --release                              # Optimized build
cargo bench --features bench                       # Run benchmarks
FLAMEGRAPH=1 cargo bench --bench system_benchmarks # Generate flamegraphs
cargo test --release                               # Optimized test run
time cargo test --lib                              # Time test execution

# Coverage & Quality
cargo tarpaulin --out Html,Json                    # Multiple output formats
cargo audit                                        # Security audit
cargo clippy -- -D warnings                       # Lint with errors
cargo fmt --check                                  # Format check
```

---

## End-to-End Workflow Demonstrations

Let's walk through complete testing workflows that demonstrate the framework's power and versatility.

### Workflow 1: Full Migration Phase Validation

This workflow demonstrates how to validate an entire migration phase from start to finish:

```bash
# Step 1: Prepare the environment
echo "🚀 Starting Full Migration Phase Validation Workflow"

# Clean previous test data
rm -rf /tmp/alys-test-results/*
rm -rf test-data/*

# Start fresh Docker environment  
docker-compose -f docker-compose.test.yml down -v
docker-compose -f docker-compose.test.yml up -d

# Wait for services to be healthy
echo "⏳ Waiting for services to start..."
sleep 30
curl --retry 30 --retry-delay 2 http://localhost:8080/health

# Step 2: Run Foundation Phase Tests  
echo "🔧 Phase 1: Testing Foundation Infrastructure"
cargo test --lib foundation --verbose -- --nocapture

# Verify foundation metrics
echo "📊 Foundation Metrics:"
cat /tmp/alys-test-results/metrics.json | jq '.phase_metrics.foundation'

# Step 3: Run Actor Core Phase Tests
echo "🎭 Phase 2: Testing Actor Core System"
cargo test --lib actor --verbose -- --nocapture

# Check actor test results
echo "📊 Actor System Metrics:" 
cat /tmp/alys-test-results/metrics.json | jq '.phase_metrics.actor_core'

# Step 4: Run Sync Improvement Phase Tests
echo "🔄 Phase 3: Testing Sync Improvement"  
cargo test --lib sync --verbose -- --nocapture

# Verify sync performance
echo "📊 Sync Performance Metrics:"
cat /tmp/alys-test-results/metrics.json | jq '.phase_metrics.sync_improvement'

# Step 5: Run Property-Based Tests
echo "🎲 Phase 4: Running Property-Based Tests"
PROPTEST_CASES=1000 cargo test --test minimal_property_tests -- --nocapture
PROPTEST_CASES=1000 cargo test --test sync_checkpoint_property_tests -- --nocapture
PROPTEST_CASES=1000 cargo test --test governance_signature_property_tests -- --nocapture

# Step 6: Run Chaos Testing
echo "⚡ Phase 5: Running Chaos Engineering Tests"
cargo test --test chaos_tests --release -- --nocapture --test-threads=1

# Step 7: Generate Comprehensive Report
echo "📋 Generating Comprehensive Report"
./tests/scripts/run_comprehensive_tests.sh

# Step 8: Analyze Results
echo "🔍 Migration Phase Validation Results:"
echo "======================================"
cat /tmp/alys-test-results/summary.json | jq '{
    overall_success_rate: .overall_success_rate,
    phases_completed: .phases_completed,
    total_tests_run: .total_tests_run,
    total_duration: .total_duration,
    coverage_percentage: .coverage_percentage
}'

echo "✅ Full Migration Phase Validation Completed!"
```

### Workflow 2: Performance Regression Testing

This workflow demonstrates how to detect and analyze performance regressions:

```bash
echo "🏎️  Starting Performance Regression Testing Workflow"

# Step 1: Establish Performance Baseline
echo "📊 Step 1: Establishing Performance Baseline"
git checkout main
cargo build --release

# Run benchmarks and save as baseline
cargo bench --bench actor_benchmarks -- --save-baseline main
cargo bench --bench sync_benchmarks -- --save-baseline main  
cargo bench --bench system_benchmarks -- --save-baseline main

echo "✅ Baseline established on main branch"

# Step 2: Switch to Feature Branch
echo "🔀 Step 2: Testing Feature Branch Performance"
git checkout feature-branch  # Replace with actual branch name
cargo build --release

# Step 3: Run Performance Tests with Comparison
echo "⚡ Step 3: Running Performance Benchmarks"

# Actor performance testing
echo "Testing Actor Performance..."
cargo bench --bench actor_benchmarks -- --baseline main

# Sync performance testing  
echo "Testing Sync Performance..."
cargo bench --bench sync_benchmarks -- --baseline main

# System performance testing
echo "Testing System Performance..."
cargo bench --bench system_benchmarks -- --baseline main

# Step 4: Generate Flamegraphs for Analysis
echo "🔥 Step 4: Generating Performance Analysis"
FLAMEGRAPH=1 cargo bench --bench system_benchmarks

# Step 5: Analyze Performance Results
echo "📈 Step 5: Analyzing Performance Results"
echo "======================================="

# Check for regressions
echo "Performance Report:"
cat target/performance/performance_report.json | jq '{
    performance_score: .performance_score,
    regressions_count: (.regressions | length),
    improvements_count: (.improvements | length),
    major_regressions: [.regressions[] | select(.severity == "Major" or .severity == "Critical")]
}'

# Display flamegraph location  
echo "Flamegraph generated at: $(find target/performance -name "*.svg" | head -1)"

# Step 6: Performance Quality Gate Check
echo "🚦 Step 6: Quality Gate Validation"
PERFORMANCE_SCORE=$(cat target/performance/performance_report.json | jq -r '.performance_score')
REGRESSION_COUNT=$(cat target/performance/performance_report.json | jq -r '.regressions | length')

if (( $(echo "$PERFORMANCE_SCORE >= 75.0" | bc -l) )) && [ "$REGRESSION_COUNT" -eq 0 ]; then
    echo "✅ Performance Quality Gate: PASSED"
    echo "   Performance Score: $PERFORMANCE_SCORE/100"
    echo "   Regressions: $REGRESSION_COUNT"
else
    echo "❌ Performance Quality Gate: FAILED"
    echo "   Performance Score: $PERFORMANCE_SCORE/100 (minimum: 75.0)"
    echo "   Regressions: $REGRESSION_COUNT (maximum: 0)"
    
    # Show regression details
    cat target/performance/performance_report.json | jq -r '
        .regressions[] | 
        "- \(.benchmark_name): \(.change_percent)% slower (\(.severity) regression)"
    '
fi

echo "🏁 Performance Regression Testing Completed!"
```

### Workflow 3: Chaos Engineering Resilience Validation

This workflow demonstrates comprehensive chaos engineering testing:

```bash
echo "⚡ Starting Chaos Engineering Resilience Validation"

# Step 1: Prepare System for Chaos Testing
echo "🛠️  Step 1: Preparing Chaos Testing Environment"

# Ensure robust test environment
docker-compose -f docker-compose.test.yml down -v
docker system prune -f
docker-compose -f docker-compose.test.yml up -d

# Wait for full system initialization
echo "⏳ Waiting for system stabilization..."
sleep 60

# Verify all services are healthy
for service in bitcoin-core execution consensus prometheus grafana; do
    echo "Checking $service health..."
    docker-compose -f docker-compose.test.yml ps $service
done

# Step 2: Baseline System Performance
echo "📊 Step 2: Establishing Baseline Performance"
curl -X POST http://localhost:8080/test-runs \
  -H "Content-Type: application/json" \
  -d '{
    "test_suite": "baseline_performance",
    "configuration": "production",
    "chaos_enabled": false
  }'

BASELINE_RUN_ID=$(curl -s http://localhost:8080/test-runs | jq -r '.[-1].id')
echo "Baseline run ID: $BASELINE_RUN_ID"

# Wait for baseline completion
while [ "$(curl -s http://localhost:8080/test-runs/$BASELINE_RUN_ID | jq -r '.status')" = "running" ]; do
    echo "Baseline tests running..."
    sleep 10
done

echo "✅ Baseline performance established"

# Step 3: Network Chaos Testing
echo "🌐 Step 3: Network Chaos Engineering"

# Test network partition resilience
echo "Testing network partitions..."
cargo test test_network_partition_resilience \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee /tmp/alys-test-results/network-chaos.log

# Test latency injection tolerance  
echo "Testing latency injection..."
cargo test test_latency_injection_tolerance \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee -a /tmp/alys-test-results/network-chaos.log

# Test message corruption recovery
echo "Testing message corruption..."
cargo test test_message_corruption_recovery \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee -a /tmp/alys-test-results/network-chaos.log

# Step 4: Resource Chaos Testing
echo "💾 Step 4: Resource Chaos Engineering"

# Test memory pressure handling
echo "Testing memory pressure..."
cargo test test_memory_pressure_handling \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee /tmp/alys-test-results/resource-chaos.log

# Test CPU stress resilience
echo "Testing CPU stress..."  
cargo test test_cpu_stress_resilience \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee -a /tmp/alys-test-results/resource-chaos.log

# Test disk I/O failure tolerance
echo "Testing disk failures..."
cargo test test_disk_failure_tolerance \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee -a /tmp/alys-test-results/resource-chaos.log

# Step 5: Byzantine Chaos Testing  
echo "🏛️  Step 5: Byzantine Behavior Testing"

# Test malicious actor injection
echo "Testing malicious actors..."
cargo test test_malicious_actor_injection \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee /tmp/alys-test-results/byzantine-chaos.log

# Test consensus attack resistance
echo "Testing consensus attacks..."
cargo test test_consensus_attack_resistance \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee -a /tmp/alys-test-results/byzantine-chaos.log

# Test Sybil attack prevention
echo "Testing Sybil attacks..."
cargo test test_sybil_attack_prevention \
    --features chaos -- --nocapture --test-threads=1 \
    2>&1 | tee -a /tmp/alys-test-results/byzantine-chaos.log

# Step 6: Comprehensive Chaos Testing
echo "🌪️  Step 6: Multi-Category Chaos Testing"
curl -X POST http://localhost:8080/test-runs \
  -H "Content-Type: application/json" \
  -d '{
    "test_suite": "comprehensive_chaos",
    "configuration": "chaos_enabled",  
    "chaos_enabled": true,
    "chaos_duration": 300,
    "chaos_event_interval": 15
  }'

CHAOS_RUN_ID=$(curl -s http://localhost:8080/test-runs | jq -r '.[-1].id')
echo "Comprehensive chaos run ID: $CHAOS_RUN_ID"

# Monitor chaos testing progress
echo "🔍 Monitoring chaos testing progress..."
while [ "$(curl -s http://localhost:8080/test-runs/$CHAOS_RUN_ID | jq -r '.status')" = "running" ]; do
    PROGRESS=$(curl -s http://localhost:8080/test-runs/$CHAOS_RUN_ID | jq -r '.progress_percentage')
    echo "Chaos testing progress: $PROGRESS%"
    sleep 30
done

# Step 7: Resilience Analysis
echo "📊 Step 7: Resilience Analysis & Reporting"

echo "Chaos Engineering Results:"
echo "=========================="

# Network resilience analysis
NETWORK_SUCCESS=$(grep -c "test result: ok" /tmp/alys-test-results/network-chaos.log || echo "0")
NETWORK_TOTAL=$(grep -c "test " /tmp/alys-test-results/network-chaos.log || echo "1")  
NETWORK_SUCCESS_RATE=$((NETWORK_SUCCESS * 100 / NETWORK_TOTAL))
echo "Network Resilience: $NETWORK_SUCCESS_RATE% ($NETWORK_SUCCESS/$NETWORK_TOTAL tests passed)"

# Resource resilience analysis
RESOURCE_SUCCESS=$(grep -c "test result: ok" /tmp/alys-test-results/resource-chaos.log || echo "0")
RESOURCE_TOTAL=$(grep -c "test " /tmp/alys-test-results/resource-chaos.log || echo "1")
RESOURCE_SUCCESS_RATE=$((RESOURCE_SUCCESS * 100 / RESOURCE_TOTAL))
echo "Resource Resilience: $RESOURCE_SUCCESS_RATE% ($RESOURCE_SUCCESS/$RESOURCE_TOTAL tests passed)"

# Byzantine resilience analysis  
BYZANTINE_SUCCESS=$(grep -c "test result: ok" /tmp/alys-test-results/byzantine-chaos.log || echo "0")
BYZANTINE_TOTAL=$(grep -c "test " /tmp/alys-test-results/byzantine-chaos.log || echo "1")
BYZANTINE_SUCCESS_RATE=$((BYZANTINE_SUCCESS * 100 / BYZANTINE_TOTAL))
echo "Byzantine Resilience: $BYZANTINE_SUCCESS_RATE% ($BYZANTINE_SUCCESS/$BYZANTINE_TOTAL tests passed)"

# Overall resilience score
OVERALL_SUCCESS=$((NETWORK_SUCCESS + RESOURCE_SUCCESS + BYZANTINE_SUCCESS))
OVERALL_TOTAL=$((NETWORK_TOTAL + RESOURCE_TOTAL + BYZANTINE_TOTAL))
OVERALL_SUCCESS_RATE=$((OVERALL_SUCCESS * 100 / OVERALL_TOTAL))

echo ""
echo "Overall Resilience Score: $OVERALL_SUCCESS_RATE% ($OVERALL_SUCCESS/$OVERALL_TOTAL)"

# Step 8: Quality Gate Validation
echo "🚦 Step 8: Resilience Quality Gate"
if [ "$OVERALL_SUCCESS_RATE" -ge 80 ]; then
    echo "✅ Chaos Engineering Quality Gate: PASSED"
    echo "   System demonstrates adequate resilience (≥80%)"
else
    echo "❌ Chaos Engineering Quality Gate: FAILED"  
    echo "   System resilience below threshold (<80%)"
    echo "   Recommendation: Review and strengthen fault tolerance mechanisms"
fi

# Generate detailed resilience report
echo "📋 Generating Detailed Resilience Report"
cat > /tmp/alys-test-results/chaos-engineering-report.md << EOF
# Chaos Engineering Resilience Report

## Executive Summary
- Overall Resilience Score: **$OVERALL_SUCCESS_RATE%**
- Tests Executed: $OVERALL_TOTAL
- Tests Passed: $OVERALL_SUCCESS  
- Quality Gate: $([ "$OVERALL_SUCCESS_RATE" -ge 80 ] && echo "✅ PASSED" || echo "❌ FAILED")

## Category Breakdown
- **Network Resilience**: $NETWORK_SUCCESS_RATE% ($NETWORK_SUCCESS/$NETWORK_TOTAL)
- **Resource Resilience**: $RESOURCE_SUCCESS_RATE% ($RESOURCE_SUCCESS/$RESOURCE_TOTAL)  
- **Byzantine Resilience**: $BYZANTINE_SUCCESS_RATE% ($BYZANTINE_SUCCESS/$BYZANTINE_TOTAL)

## Test Artifacts
- Network chaos logs: /tmp/alys-test-results/network-chaos.log
- Resource chaos logs: /tmp/alys-test-results/resource-chaos.log
- Byzantine chaos logs: /tmp/alys-test-results/byzantine-chaos.log
- Comprehensive test run: $CHAOS_RUN_ID

## Next Steps
$([ "$OVERALL_SUCCESS_RATE" -ge 80 ] && echo "System demonstrates adequate resilience. Continue monitoring." || echo "System requires resilience improvements. Focus on failed test scenarios.")
EOF

echo "📄 Detailed report generated: /tmp/alys-test-results/chaos-engineering-report.md"
echo "⚡ Chaos Engineering Resilience Validation Completed!"
```

### Workflow 4: Complete CI/CD Integration Testing

This workflow demonstrates the complete CI/CD integration process:

```bash
echo "🔄 Starting Complete CI/CD Integration Testing Workflow"

# Step 1: Environment Preparation
echo "🛠️  Step 1: CI/CD Environment Preparation"

# Clean slate
docker system prune -a -f --volumes
git clean -fdx

# Setup CI/CD specific configuration
export TEST_CONFIG=ci_cd
export RUST_LOG=info  
export TEST_TIMEOUT=600
export PARALLEL_JOBS=4
export COVERAGE_THRESHOLD=80

# Step 2: Pull Latest Code & Dependencies
echo "📦 Step 2: Code & Dependencies"
git fetch --all  
git checkout main
git pull origin main

# Update Rust and dependencies
rustup update stable
cargo update

# Build all components
cargo build --all-targets
cargo build --release

# Step 3: Start Complete Test Environment
echo "🐳 Step 3: Starting Complete Test Environment"
docker-compose -f docker-compose.test.yml up -d

# Health check all services with retries
echo "🏥 Health checking all services..."
for service in bitcoin-core execution consensus prometheus grafana; do
    echo "Checking $service..."
    timeout=60
    until [ $timeout -le 0 ] || docker-compose -f docker-compose.test.yml exec -T $service echo "healthy" 2>/dev/null; do
        echo "Waiting for $service... ($timeout seconds remaining)"
        sleep 5
        timeout=$((timeout-5))
    done
    
    if [ $timeout -le 0 ]; then
        echo "❌ $service failed to start within timeout"
        docker-compose -f docker-compose.test.yml logs $service
        exit 1
    else
        echo "✅ $service is healthy"
    fi
done

# Step 4: Unit Testing Phase
echo "🧪 Step 4: Unit Testing Phase"
echo "============================="

# Run all unit tests with coverage
cargo tarpaulin --out Json --output-dir /tmp/alys-test-results/ \
    --skip-clean --timeout 300 --verbose

# Parse coverage results
COVERAGE=$(cat /tmp/alys-test-results/tarpaulin-report.json | jq -r '.files | add | .coverage')
echo "Unit Test Coverage: $COVERAGE%"

if (( $(echo "$COVERAGE >= $COVERAGE_THRESHOLD" | bc -l) )); then
    echo "✅ Coverage Quality Gate: PASSED ($COVERAGE% >= $COVERAGE_THRESHOLD%)"
else
    echo "❌ Coverage Quality Gate: FAILED ($COVERAGE% < $COVERAGE_THRESHOLD%)"
    echo "Low coverage files:"
    cat /tmp/alys-test-results/tarpaulin-report.json | jq -r '
        .files | to_entries[] | select(.value.coverage < 80) | 
        "\(.key): \(.value.coverage)%"
    ' | head -10
fi

# Step 5: Integration Testing Phase  
echo "🔧 Step 5: Integration Testing Phase"
echo "===================================="

# Wait for test coordinator to be ready
curl --retry 30 --retry-delay 2 http://localhost:8080/health

# Run comprehensive integration tests
./tests/scripts/run_comprehensive_tests.sh

# Check integration test results
INTEGRATION_RESULTS=$(cat /tmp/alys-test-results/summary.json)
INTEGRATION_SUCCESS_RATE=$(echo $INTEGRATION_RESULTS | jq -r '.overall_success_rate')
INTEGRATION_TESTS_RUN=$(echo $INTEGRATION_RESULTS | jq -r '.total_tests_run')

echo "Integration Tests: $INTEGRATION_SUCCESS_RATE% ($INTEGRATION_TESTS_RUN tests)"

# Step 6: Property-Based Testing Phase
echo "🎲 Step 6: Property-Based Testing Phase"  
echo "========================================"

# Run property tests with CI-appropriate parameters
PROPTEST_CASES=5000 PROPTEST_MAX_SHRINK_ITERS=1000 \
    cargo test --test minimal_property_tests --release

PROPTEST_CASES=5000 PROPTEST_MAX_SHRINK_ITERS=1000 \
    cargo test --test sync_checkpoint_property_tests --release

PROPTEST_CASES=5000 PROPTEST_MAX_SHRINK_ITERS=1000 \
    cargo test --test governance_signature_property_tests --release

echo "✅ Property-based testing completed"

# Step 7: Performance Benchmarking Phase
echo "🏎️  Step 7: Performance Benchmarking Phase"
echo "==========================================="

# Run benchmarks with baseline comparison
if [ -f "target/criterion/.baseline" ]; then
    echo "Comparing against existing baseline..."
    cargo bench --bench actor_benchmarks -- --baseline previous
    cargo bench --bench sync_benchmarks -- --baseline previous
    cargo bench --bench system_benchmarks -- --baseline previous
else
    echo "Creating new baseline..."
    cargo bench --bench actor_benchmarks -- --save-baseline current
    cargo bench --bench sync_benchmarks -- --save-baseline current
    cargo bench --bench system_benchmarks -- --save-baseline current
fi

# Analyze performance results
PERFORMANCE_SCORE=$(cat target/performance/performance_report.json | jq -r '.performance_score // 85')
REGRESSION_COUNT=$(cat target/performance/performance_report.json | jq -r '.regressions | length // 0')

echo "Performance Score: $PERFORMANCE_SCORE/100"
echo "Performance Regressions: $REGRESSION_COUNT"

# Step 8: Chaos Engineering Phase (Optional in CI/CD)
if [ "${SKIP_CHAOS_TESTS:-false}" != "true" ]; then
    echo "⚡ Step 8: Chaos Engineering Phase"
    echo "=================================="
    
    # Run lightweight chaos tests suitable for CI/CD
    timeout 300 cargo test test_lightweight_network_chaos \
        --features chaos -- --nocapture --test-threads=1 || echo "Chaos tests timed out"
    
    timeout 300 cargo test test_lightweight_resource_chaos \
        --features chaos -- --nocapture --test-threads=1 || echo "Chaos tests timed out"
        
    echo "✅ Lightweight chaos testing completed"
else
    echo "⏭️  Step 8: Skipping Chaos Engineering (SKIP_CHAOS_TESTS=true)"
fi

# Step 9: Quality Gate Evaluation
echo "🚦 Step 9: Quality Gate Evaluation"
echo "=================================="

# Collect all quality metrics
UNIT_TEST_SUCCESS=true
INTEGRATION_SUCCESS=$(echo "$INTEGRATION_SUCCESS_RATE >= 95" | bc -l)  
COVERAGE_SUCCESS=$(echo "$COVERAGE >= $COVERAGE_THRESHOLD" | bc -l)
PERFORMANCE_SUCCESS=$(echo "$PERFORMANCE_SCORE >= 75 && $REGRESSION_COUNT == 0" | bc -l)

echo "Quality Gate Results:"
echo "===================="
echo "Unit Tests:        $([ "$UNIT_TEST_SUCCESS" = "true" ] && echo "✅ PASS" || echo "❌ FAIL")"
echo "Integration Tests: $([ "$INTEGRATION_SUCCESS" = "1" ] && echo "✅ PASS ($INTEGRATION_SUCCESS_RATE%)" || echo "❌ FAIL ($INTEGRATION_SUCCESS_RATE%)")"
echo "Code Coverage:     $([ "$COVERAGE_SUCCESS" = "1" ] && echo "✅ PASS ($COVERAGE%)" || echo "❌ FAIL ($COVERAGE%)")"
echo "Performance:       $([ "$PERFORMANCE_SUCCESS" = "1" ] && echo "✅ PASS ($PERFORMANCE_SCORE/100, $REGRESSION_COUNT regressions)" || echo "❌ FAIL ($PERFORMANCE_SCORE/100, $REGRESSION_COUNT regressions)")"

# Overall quality gate decision
if [ "$UNIT_TEST_SUCCESS" = "true" ] && [ "$INTEGRATION_SUCCESS" = "1" ] && [ "$COVERAGE_SUCCESS" = "1" ] && [ "$PERFORMANCE_SUCCESS" = "1" ]; then
    OVERALL_QUALITY_GATE="PASS"
    echo ""
    echo "🎉 OVERALL QUALITY GATE: ✅ PASSED"
    echo "All quality criteria met. Build is ready for deployment."
else
    OVERALL_QUALITY_GATE="FAIL"
    echo ""
    echo "🚨 OVERALL QUALITY GATE: ❌ FAILED"
    echo "One or more quality criteria not met. Build blocked from deployment."
fi

# Step 10: Generate CI/CD Report
echo "📊 Step 10: Generating CI/CD Report"
echo "==================================="

# Create comprehensive CI/CD report
cat > /tmp/alys-test-results/cicd-report.json << EOF
{
    "build_info": {
        "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
        "git_commit": "$(git rev-parse HEAD)",
        "git_branch": "$(git rev-parse --abbrev-ref HEAD)",
        "rust_version": "$(rustc --version)",
        "docker_version": "$(docker --version)"
    },
    "quality_gates": {
        "overall": "$OVERALL_QUALITY_GATE",
        "unit_tests": $([ "$UNIT_TEST_SUCCESS" = "true" ] && echo "true" || echo "false"),
        "integration_tests": $([ "$INTEGRATION_SUCCESS" = "1" ] && echo "true" || echo "false"),
        "code_coverage": $([ "$COVERAGE_SUCCESS" = "1" ] && echo "true" || echo "false"),
        "performance": $([ "$PERFORMANCE_SUCCESS" = "1" ] && echo "true" || echo "false")
    },
    "metrics": {
        "coverage_percentage": $COVERAGE,
        "integration_success_rate": $INTEGRATION_SUCCESS_RATE,
        "integration_tests_run": $INTEGRATION_TESTS_RUN,
        "performance_score": $PERFORMANCE_SCORE,
        "performance_regressions": $REGRESSION_COUNT
    },
    "artifacts": {
        "coverage_report": "/tmp/alys-test-results/tarpaulin-report.html",
        "integration_summary": "/tmp/alys-test-results/summary.json",
        "performance_report": "target/criterion/report/index.html",
        "test_logs": "/tmp/alys-test-results/"
    }
}
EOF

echo "📄 CI/CD Report generated: /tmp/alys-test-results/cicd-report.json"

# Step 11: Cleanup & Artifact Archival
echo "🗑️  Step 11: Cleanup & Artifact Archival"
echo "========================================"

# Archive important artifacts
tar -czf /tmp/alys-test-results/test-artifacts.tar.gz \
    /tmp/alys-test-results/ target/criterion/ coverage/ 2>/dev/null || true

echo "📦 Test artifacts archived: /tmp/alys-test-results/test-artifacts.tar.gz"

# Cleanup Docker environment
docker-compose -f docker-compose.test.yml down -v

# Final CI/CD status
if [ "$OVERALL_QUALITY_GATE" = "PASS" ]; then
    echo "🎊 CI/CD Integration Testing: SUCCESS"
    exit 0
else
    echo "💥 CI/CD Integration Testing: FAILURE"
    exit 1
fi
```

These end-to-end workflows demonstrate the complete power and flexibility of the Alys V2 Testing Framework. Each workflow builds upon the previous sections and shows how to combine different testing approaches for maximum effectiveness.

---

## Reference & Cheat Sheets

### Quick Command Reference

```bash
# Essential Test Commands
cargo test --lib                           # Run all library tests
cargo test --lib framework                 # Run framework tests only
cargo test --test property_tests           # Run property tests
cargo test --test chaos_tests             # Run chaos tests
cargo bench                               # Run all benchmarks
cargo tarpaulin --out Html                # Generate coverage report

# Docker Environment
docker-compose -f docker-compose.test.yml up -d      # Start test environment
docker-compose -f docker-compose.test.yml down -v    # Stop and clean environment
docker-compose -f docker-compose.test.yml ps         # Check service status
docker-compose -f docker-compose.test.yml logs       # View all logs

# Test Coordinator API
curl http://localhost:8080/health          # Check coordinator health
curl http://localhost:8080/status          # Get detailed status
curl http://localhost:8080/test-runs       # List test runs
curl -X POST http://localhost:8080/test-runs # Create new test run

# Performance Testing
cargo bench --bench actor_benchmarks      # Actor performance tests
cargo bench --bench sync_benchmarks       # Sync performance tests  
cargo bench --bench system_benchmarks     # System performance tests
FLAMEGRAPH=1 cargo bench                  # Generate flamegraphs

# Debug Commands
RUST_LOG=debug cargo test -- --nocapture  # Debug logging
cargo test -- --test-threads=1            # Single-threaded execution
cargo test --verbose                      # Verbose output
```

### Environment Variables

```bash
# Core Configuration
export TEST_CONFIG=development             # or ci_cd
export TEST_TIMEOUT=300                   # Test timeout in seconds  
export TEST_DATA_DIR=/tmp/alys-test-results
export RUST_LOG=info                     # debug, info, warn, error
export RUST_BACKTRACE=1                  # Enable backtraces

# Performance Configuration  
export CRITERION_SAMPLE_SIZE=100          # Benchmark sample size
export FLAMEGRAPH_ENABLED=true           # Enable flamegraph generation
export MEMORY_PROFILING=true             # Enable memory profiling

# Property Test Configuration
export PROPTEST_CASES=1000               # Number of test cases
export PROPTEST_MAX_SHRINK_ITERS=1000    # Shrinking iterations
export PROPTEST_MAX_LOCAL_REJECTS=100    # Local rejection limit

# Chaos Test Configuration
export CHAOS_ENABLED=true                # Enable chaos testing
export CHAOS_DURATION=60                 # Chaos duration in seconds
export CHAOS_EVENT_INTERVAL=10           # Seconds between events
```

### File Locations Reference

```
tests/
├── src/
│   ├── framework/
│   │   ├── mod.rs                 # Main framework (lines 97-158)
│   │   ├── config.rs              # Configuration system (lines 16-162)
│   │   ├── harness/
│   │   │   ├── mod.rs             # Harness collection (lines 21-98)
│   │   │   ├── actor.rs           # Actor testing (lines 25-3866)
│   │   │   ├── sync.rs            # Sync testing (lines 21-2570)
│   │   │   ├── lighthouse.rs      # Lighthouse compatibility
│   │   │   ├── governance.rs      # Governance testing
│   │   │   └── network.rs         # Network testing
│   │   ├── validators.rs          # Validation system (lines 12-147)
│   │   ├── metrics.rs             # Metrics collection (lines 16-246)
│   │   ├── generators.rs          # PropTest generators
│   │   ├── chaos.rs               # Chaos testing (lines 22-2191)
│   │   └── performance.rs         # Performance framework
│   ├── lib.rs                     # Library entry point
│   └── bin/
│       └── test_coordinator.rs    # Test coordinator service (944 lines)
├── tests/
│   ├── minimal_property_tests.rs  # Basic property tests
│   ├── sync_checkpoint_property_tests.rs  # Sync property tests
│   └── governance_signature_property_tests.rs  # Governance property tests
├── benches/
│   ├── actor_benchmarks.rs        # Actor benchmarks (556 lines)
│   ├── sync_benchmarks.rs         # Sync benchmarks (709 lines)
│   └── system_benchmarks.rs       # System benchmarks (560 lines)
├── scripts/
│   └── run_comprehensive_tests.sh # Complete test execution (423 lines)
├── docker-compose.test.yml        # Docker test environment
├── test-config/                   # Test configuration files
└── migrations/                    # Database migrations
```

### Key Metrics & Thresholds

```bash
# Quality Gate Thresholds
UNIT_TEST_SUCCESS_RATE=100%               # Must pass all unit tests
INTEGRATION_TEST_SUCCESS_RATE=95%         # 95% minimum for integration
CODE_COVERAGE_THRESHOLD=80%               # Minimum code coverage
PERFORMANCE_REGRESSION_THRESHOLD=20%      # Maximum allowed regression
CHAOS_TEST_RESILIENCE_THRESHOLD=80%       # Minimum resilience score

# Performance Targets
ACTOR_MESSAGE_THROUGHPUT=1000             # Messages per second
ACTOR_MESSAGE_LATENCY=100                 # Milliseconds maximum
SYNC_BLOCK_PROCESSING=500                 # Blocks per second
SYNC_CHECKPOINT_VALIDATION=250            # Checkpoints validated
PROPERTY_TEST_CASES=1000                  # Minimum test cases
CHAOS_EVENT_RECOVERY=80                   # Recovery success rate %
```

### Troubleshooting Quick Reference

| Problem | Solution |
|---------|----------|
| Docker services not starting | `docker system prune -f && docker-compose up -d` |
| Port conflicts | `netstat -tlnp \| grep :PORT` then kill conflicting process |
| Test timeouts | Increase `TEST_TIMEOUT` environment variable |
| Memory issues during testing | `ulimit -v 16777216` and run tests sequentially |
| Coverage report generation fails | `cargo clean && cargo tarpaulin --skip-clean` |
| Benchmarks running slowly | Build with `--release` flag |
| Property tests failing frequently | Reduce `PROPTEST_CASES` for debugging |
| Chaos tests hanging | Use `--test-threads=1` and add timeouts |
| Database connection issues | Check `/tmp/alys-test-coordinator.db` permissions |
| Flamegraph generation failing | Install `cargo install flamegraph` |

---

## Conclusion

Congratulations! 🎉 You've completed the comprehensive Alys V2 Testing Framework onboarding. You now have the knowledge and tools to:

- ✅ Set up and configure the complete testing environment
- ✅ Execute all 7 phases of the testing framework
- ✅ Understand and utilize 5 specialized test harnesses
- ✅ Implement property-based testing with PropTest
- ✅ Perform chaos engineering and resilience testing
- ✅ Conduct performance benchmarking and regression analysis
- ✅ Integrate with CI/CD pipelines for automated testing
- ✅ Troubleshoot common issues and optimize test execution
- ✅ Generate comprehensive test reports and analyze results

### Your Next Steps

1. **Practice**: Start with simple test runs and gradually work up to complex scenarios
2. **Contribute**: Begin contributing to the testing framework by adding new test cases
3. **Optimize**: Help optimize test execution times and resource usage
4. **Share**: Share your knowledge with other team members and contribute to documentation

### Support & Resources

- **Documentation**: All code references are provided throughout this guide
- **Community**: Join the Alys development community for support and collaboration
- **Updates**: This framework is continuously evolving - stay updated with the latest changes

Remember: Testing is not just about finding bugs - it's about building confidence in the system's reliability, performance, and resilience. The Alys V2 Testing Framework provides you with the most comprehensive tools available to ensure the migration's success.

Happy Testing! 🚀🧪⚡

<function_calls>
<invoke name="TodoWrite">
<parameter name="todos">[{"id": "1", "status": "completed", "content": "Read the complete testing framework documentation to understand all components"}, {"id": "2", "status": "completed", "content": "Create comprehensive QA onboarding guide structure with sequential sections"}, {"id": "3", "status": "completed", "content": "Write local dev environment setup section with prerequisites and commands"}, {"id": "4", "status": "in_progress", "content": "Create testing framework interaction section with hands-on examples"}, {"id": "5", "status": "pending", "content": "Add CI/CD integration section with configuration and workflows"}, {"id": "6", "status": "pending", "content": "Write Pro Tips section with productivity hacks and useful commands"}, {"id": "7", "status": "pending", "content": "Add mermaid diagrams and code references throughout"}, {"id": "8", "status": "pending", "content": "Include end-to-end workflow demonstrations"}, {"id": "9", "status": "pending", "content": "Review and polish the complete guide for accuracy and completeness"}, {"id": "10", "status": "pending", "content": "Output final guide to testing-framework-qa-onboarding2.knowledge.md"}]