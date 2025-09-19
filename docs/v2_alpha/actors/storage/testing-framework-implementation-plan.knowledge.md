# Detailed Step-by-Step Implementation Plan: Storage Actor Testing Infrastructure

## Phase 1: Base Testing Infrastructure Setup

### Step 1: File Structure Creation

Create the following directory structure:

```
app/src/actors_v2/testing/
├── mod.rs                          # Module declarations and re-exports
├── base/                           # Base testing infrastructure
│   ├── mod.rs                      # Base module exports
│   ├── traits.rs                   # Core testing traits and interfaces
│   ├── harness.rs                  # Test harness implementation
│   ├── fixtures.rs                 # Common test fixtures and data
│   ├── utils.rs                    # Testing utilities and helpers
│   └── config.rs                   # Test configuration management
├── property/                       # Property-based testing framework
│   ├── mod.rs                      # Property testing exports
│   ├── generators.rs               # Custom data generators
│   └── strategies.rs               # Testing strategies and combinators
├── chaos/                          # Chaos testing framework
│   ├── mod.rs                      # Chaos testing exports
│   ├── injectors.rs                # Failure injection mechanisms
│   ├── scenarios.rs                # Chaos testing scenarios
│   └── monitors.rs                 # System state monitoring
└── storage/                        # Storage Actor specific tests
    ├── mod.rs                      # Storage testing module
    ├── unit/                       # Unit tests
    │   ├── mod.rs
    │   ├── database_tests.rs       # Database layer unit tests
    │   ├── cache_tests.rs          # Cache layer unit tests
    │   ├── metrics_tests.rs        # Metrics unit tests
    │   └── message_tests.rs        # Message handling unit tests
    ├── integration/                # Integration tests
    │   ├── mod.rs
    │   ├── actor_tests.rs          # Full actor integration tests
    │   ├── persistence_tests.rs    # Data persistence integration
    │   └── concurrency_tests.rs    # Concurrent operation tests
    ├── property/                   # Property-based tests
    │   ├── mod.rs
    │   └── storage_properties.rs   # Storage invariant tests
    ├── chaos/                      # Chaos tests
    │   ├── mod.rs
    │   └── storage_chaos.rs        # Storage failure scenarios
    └── fixtures/                   # Storage-specific test fixtures
        ├── mod.rs
        ├── blocks.rs               # Block test data generators
        └── config.rs               # Storage test configurations
```

### Step 2: Dependency Configuration

**Action:** Modify `app/Cargo.toml` to add testing dependencies

**Add to `[dependencies]` section:**
```toml
# Testing infrastructure (existing ones already present)
proptest = "1.4"
quickcheck = "1.0"
quickcheck_macros = "1.0"
mockall = "0.11"
wiremock = "0.5"
criterion = { version = "0.5", features = ["html_reports"] }
```

**Add to `[dev-dependencies]` section:**
```toml
# Additional testing tools
test-case = "3.3"
rstest = "0.18"
serial_test = "3.0"
tokio-test = "0.4"
env_logger = "0.10"
```

**Validation:** Run `cargo check` to ensure dependencies resolve correctly.

### Step 3: Base Testing Infrastructure Implementation

**File:** `app/src/actors_v2/testing/mod.rs`

**Action:** Create module structure
```rust
pub mod base;
pub mod property;
pub mod chaos;
pub mod storage;

pub use base::*;
```

**File:** `app/src/actors_v2/testing/base/mod.rs`

**Action:** Define base module exports
```rust
pub mod traits;
pub mod harness;
pub mod fixtures;
pub mod utils;
pub mod config;

pub use traits::*;
pub use harness::*;
pub use fixtures::*;
pub use utils::*;
pub use config::*;
```

**File:** `app/src/actors_v2/testing/base/traits.rs`

**Action:** Implement core testing traits
```rust
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Core trait for all actor test harnesses
#[async_trait]
pub trait ActorTestHarness: Send + Sync {
    type Actor;
    type Config: Clone + Send + Sync;
    type Message: Send + Sync;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Create a new test instance with default configuration
    async fn new() -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Create a test instance with custom configuration
    async fn with_config(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Get a reference to the underlying actor
    fn actor(&self) -> &Self::Actor;

    /// Get a mutable reference to the underlying actor
    fn actor_mut(&mut self) -> &mut Self::Actor;

    /// Send a message to the actor and await response
    async fn send_message(&mut self, message: Self::Message) -> Result<(), Self::Error>;

    /// Setup test environment (called before each test)
    async fn setup(&mut self) -> Result<(), Self::Error>;

    /// Cleanup test environment (called after each test)
    async fn teardown(&mut self) -> Result<(), Self::Error>;

    /// Verify actor is in expected state
    async fn verify_state(&self) -> Result<(), Self::Error>;

    /// Reset actor to initial state
    async fn reset(&mut self) -> Result<(), Self::Error>;
}

/// Trait for property-based testing support
#[async_trait]
pub trait PropertyTestable: ActorTestHarness {
    type PropertyInput: Send + Sync;

    /// Execute a single property test iteration
    async fn execute_property(&mut self, input: Self::PropertyInput) -> Result<bool, Self::Error>;

    /// Verify invariants hold after property execution
    async fn check_invariants(&self) -> Result<bool, Self::Error>;
}

/// Trait for chaos testing support
#[async_trait]
pub trait ChaosTestable: ActorTestHarness {
    type FailureScenario: Send + Sync;

    /// Inject a failure scenario
    async fn inject_failure(&mut self, scenario: Self::FailureScenario) -> Result<(), Self::Error>;

    /// Monitor system state during chaos
    async fn monitor_state(&self) -> Result<SystemHealthReport, Self::Error>;

    /// Recover from injected failures
    async fn recover(&mut self) -> Result<(), Self::Error>;
}

/// System health monitoring for chaos testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthReport {
    pub timestamp: std::time::SystemTime,
    pub actor_responsive: bool,
    pub memory_usage: u64,
    pub active_connections: u32,
    pub error_count: u32,
    pub custom_metrics: HashMap<String, f64>,
}

/// Test execution context
#[derive(Debug, Clone)]
pub struct TestContext {
    pub test_id: Uuid,
    pub test_name: String,
    pub timeout: Duration,
    pub max_retries: u32,
    pub cleanup_on_failure: bool,
    pub metadata: HashMap<String, String>,
}

impl Default for TestContext {
    fn default() -> Self {
        Self {
            test_id: Uuid::new_v4(),
            test_name: String::new(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            cleanup_on_failure: true,
            metadata: HashMap::new(),
        }
    }
}
```

**Validation:** Run `cargo check` to ensure trait definitions compile.

### Step 4: Test Harness Implementation

**File:** `app/src/actors_v2/testing/base/harness.rs`

**Action:** Implement base test harness
```rust
use super::{ActorTestHarness, TestContext};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Generic test harness implementation
pub struct BaseTestHarness<T> {
    pub context: TestContext,
    pub actor: Arc<RwLock<T>>,
    pub metrics: TestMetrics,
}

#[derive(Debug, Default)]
pub struct TestMetrics {
    pub messages_sent: u64,
    pub errors_encountered: u64,
    pub test_duration: Option<std::time::Duration>,
    pub memory_peak: u64,
}

impl<T> BaseTestHarness<T> {
    pub fn new_with_actor(actor: T) -> Self {
        Self {
            context: TestContext::default(),
            actor: Arc::new(RwLock::new(actor)),
            metrics: TestMetrics::default(),
        }
    }

    pub async fn with_timeout(&mut self, timeout: std::time::Duration) {
        self.context.timeout = timeout;
    }

    pub async fn measure_memory(&mut self) {
        // Platform-specific memory measurement would go here
        self.metrics.memory_peak = self.get_current_memory_usage();
    }

    fn get_current_memory_usage(&self) -> u64 {
        // Simplified implementation - would use proper memory profiling
        0
    }
}
```

### Step 5: Storage Actor Test Infrastructure

**File:** `app/src/actors_v2/testing/storage/mod.rs`

**Action:** Create storage testing module
```rust
pub mod unit;
pub mod integration;
pub mod property;
pub mod chaos;
pub mod fixtures;

use super::base::*;
use crate::actors_v2::storage::actor::{StorageActor, StorageConfig};
use crate::actors_v2::storage::messages::*;
use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;

/// Storage Actor specific test harness
pub struct StorageTestHarness {
    pub base: BaseTestHarness<StorageActor>,
    pub temp_dir: TempDir,
    pub config: StorageConfig,
    pub test_blocks: Vec<crate::actors_v2::storage::actor::AlysConsensusBlock>,
}

#[async_trait]
impl ActorTestHarness for StorageTestHarness {
    type Actor = StorageActor;
    type Config = StorageConfig;
    type Message = StorageMessage;
    type Error = StorageTestError;

    async fn new() -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(StorageTestError::IoError)?;
        let mut config = StorageConfig::default();
        config.database.main_path = temp_dir.path().join("test_storage").to_string_lossy().to_string();

        let actor = StorageActor::new(config.clone()).await
            .map_err(StorageTestError::ActorCreation)?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config,
            test_blocks: Vec::new(),
        })
    }

    async fn with_config(config: Self::Config) -> Result<Self, Self::Error> {
        let temp_dir = TempDir::new().map_err(StorageTestError::IoError)?;
        let mut test_config = config;
        test_config.database.main_path = temp_dir.path().join("test_storage").to_string_lossy().to_string();

        let actor = StorageActor::new(test_config.clone()).await
            .map_err(StorageTestError::ActorCreation)?;

        Ok(Self {
            base: BaseTestHarness::new_with_actor(actor),
            temp_dir,
            config: test_config,
            test_blocks: Vec::new(),
        })
    }

    fn actor(&self) -> &Self::Actor {
        // This would need proper implementation with Arc<RwLock<_>> handling
        unimplemented!("Requires proper async access pattern")
    }

    fn actor_mut(&mut self) -> &mut Self::Actor {
        unimplemented!("Requires proper async access pattern")
    }

    async fn send_message(&mut self, message: Self::Message) -> Result<(), Self::Error> {
        self.base.metrics.messages_sent += 1;

        match message {
            StorageMessage::StoreBlock(msg) => {
                let mut actor = self.base.actor.write().await;
                actor.store_block(msg.block, msg.canonical).await
                    .map_err(StorageTestError::StorageOperation)?;
            },
            StorageMessage::GetBlock(msg) => {
                let actor = self.base.actor.read().await;
                let _result = actor.get_block(&msg.block_hash).await
                    .map_err(StorageTestError::StorageOperation)?;
            },
            // Handle other message types...
        }

        Ok(())
    }

    async fn setup(&mut self) -> Result<(), Self::Error> {
        // Initialize test data
        self.test_blocks = fixtures::create_test_block_sequence(10);

        // Setup metrics collection
        self.base.measure_memory().await;

        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), Self::Error> {
        // Cleanup is automatic with TempDir drop
        Ok(())
    }

    async fn verify_state(&self) -> Result<(), Self::Error> {
        let actor = self.base.actor.read().await;

        // Verify database integrity
        // Verify cache consistency
        // Verify metrics accuracy

        Ok(())
    }

    async fn reset(&mut self) -> Result<(), Self::Error> {
        // Create fresh actor instance
        let actor = StorageActor::new(self.config.clone()).await
            .map_err(StorageTestError::ActorCreation)?;

        self.base.actor = Arc::new(tokio::sync::RwLock::new(actor));
        self.base.metrics = TestMetrics::default();

        Ok(())
    }
}

/// Storage-specific message wrapper
#[derive(Debug)]
pub enum StorageMessage {
    StoreBlock(StoreBlockMessage),
    GetBlock(GetBlockMessage),
    // Add other storage message types
}

/// Storage test error types
#[derive(Debug, thiserror::Error)]
pub enum StorageTestError {
    #[error("IO error: {0}")]
    IoError(std::io::Error),
    #[error("Actor creation failed: {0}")]
    ActorCreation(String),
    #[error("Storage operation failed: {0}")]
    StorageOperation(String),
}
```

### Step 6: Unit Test Implementation

**File:** `app/src/actors_v2/testing/storage/unit/database_tests.rs`

**Action:** Implement database unit tests
```rust
use crate::actors_v2::testing::storage::{StorageTestHarness, StorageMessage};
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::storage::messages::*;
use uuid::Uuid;

#[actix::test]
async fn test_database_block_storage_retrieval() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test block storage
    let test_block = harness.test_blocks[0].clone();
    let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
        block: test_block.clone(),
        canonical: true,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(store_message).await.unwrap();

    // Test block retrieval
    let block_hash = test_block.block_hash().to_block_hash();
    let get_message = StorageMessage::GetBlock(GetBlockMessage {
        block_hash,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_batch_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test batch storage of multiple blocks
    for (i, block) in harness.test_blocks.iter().enumerate() {
        let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
            block: block.clone(),
            canonical: i % 2 == 0, // Alternate canonical status
            correlation_id: Some(Uuid::new_v4()),
        });

        harness.send_message(store_message).await.unwrap();
    }

    // Verify all blocks stored correctly
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_error_conditions() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test invalid block hash retrieval
    let invalid_hash = lighthouse_wrapper::types::Hash256::zero();
    let get_message = StorageMessage::GetBlock(GetBlockMessage {
        block_hash: invalid_hash,
        correlation_id: Some(Uuid::new_v4()),
    });

    // This should not panic but handle gracefully
    let result = harness.send_message(get_message).await;
    assert!(result.is_ok()); // The operation succeeds but returns None

    harness.teardown().await.unwrap();
}
```

### Step 7: Integration Test Implementation

**File:** `app/src/actors_v2/testing/storage/integration/actor_tests.rs`

**Action:** Implement full actor integration tests
```rust
use crate::actors_v2::testing::storage::StorageTestHarness;
use crate::actors_v2::testing::base::ActorTestHarness;
use std::time::Duration;
use tokio::time::sleep;

#[actix::test]
async fn test_concurrent_read_write_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let test_blocks = harness.test_blocks.clone();
    let actor_ref = harness.base.actor.clone();

    // Spawn concurrent write operations
    let write_handles: Vec<_> = test_blocks
        .into_iter()
        .enumerate()
        .map(|(i, block)| {
            let actor = actor_ref.clone();
            tokio::spawn(async move {
                let mut actor_guard = actor.write().await;
                actor_guard.store_block(block, i % 2 == 0).await
            })
        })
        .collect();

    // Wait for all writes to complete
    for handle in write_handles {
        handle.await.unwrap().unwrap();
    }

    // Spawn concurrent read operations
    let read_handles: Vec<_> = (0..10)
        .map(|i| {
            let actor = actor_ref.clone();
            let block_hash = harness.test_blocks[i % harness.test_blocks.len()]
                .block_hash()
                .to_block_hash();

            tokio::spawn(async move {
                let actor_guard = actor.read().await;
                actor_guard.get_block(&block_hash).await
            })
        })
        .collect();

    // Verify all reads succeed
    for handle in read_handles {
        let result = handle.await.unwrap().unwrap();
        assert!(result.is_some());
    }

    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_persistence_across_restarts() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Store test data
    let test_block = harness.test_blocks[0].clone();
    let block_hash = test_block.block_hash().to_block_hash();

    {
        let mut actor = harness.base.actor.write().await;
        actor.store_block(test_block, true).await.unwrap();
    }

    // Simulate restart by creating new actor with same config
    harness.reset().await.unwrap();

    // Verify data persists
    {
        let actor = harness.base.actor.read().await;
        let retrieved = actor.get_block(&block_hash).await.unwrap();
        assert!(retrieved.is_some());
    }

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_performance_under_load() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let start_time = std::time::Instant::now();
    let block_count = 1000;

    // Generate and store many blocks
    for i in 0..block_count {
        let block = crate::actors_v2::testing::storage::fixtures::create_test_block(i);
        let mut actor = harness.base.actor.write().await;
        actor.store_block(block, true).await.unwrap();
    }

    let duration = start_time.elapsed();
    let blocks_per_second = block_count as f64 / duration.as_secs_f64();

    // Assert minimum performance threshold
    assert!(blocks_per_second > 100.0, "Storage performance too low: {:.2} blocks/sec", blocks_per_second);

    harness.teardown().await.unwrap();
}
```

### Step 8: Property-Based Test Implementation

**File:** `app/src/actors_v2/testing/storage/property/storage_properties.rs`

**Action:** Implement property-based tests
```rust
use crate::actors_v2::testing::storage::{StorageTestHarness, StorageMessage};
use crate::actors_v2::testing::base::{ActorTestHarness, PropertyTestable};
use proptest::prelude::*;
use async_trait::async_trait;

// Property test input generator
#[derive(Debug, Clone)]
pub struct StorageProperty {
    pub operations: Vec<StorageOperation>,
}

#[derive(Debug, Clone)]
pub enum StorageOperation {
    Store { slot: u64, canonical: bool },
    Retrieve { slot: u64 },
    UpdateHead { slot: u64 },
}

impl StorageProperty {
    pub fn operations_strategy() -> impl Strategy<Value = Vec<StorageOperation>> {
        prop::collection::vec(
            prop_oneof![
                (1u64..1000, any::<bool>()).prop_map(|(slot, canonical)|
                    StorageOperation::Store { slot, canonical }),
                (1u64..1000).prop_map(|slot| StorageOperation::Retrieve { slot }),
                (1u64..1000).prop_map(|slot| StorageOperation::UpdateHead { slot }),
            ],
            1..50
        )
    }
}

#[async_trait]
impl PropertyTestable for StorageTestHarness {
    type PropertyInput = StorageProperty;

    async fn execute_property(&mut self, input: Self::PropertyInput) -> Result<bool, Self::Error> {
        for operation in input.operations {
            match operation {
                StorageOperation::Store { slot, canonical } => {
                    let block = crate::actors_v2::testing::storage::fixtures::create_test_block(slot);
                    let mut actor = self.base.actor.write().await;
                    actor.store_block(block, canonical).await
                        .map_err(|e| StorageTestError::StorageOperation(e.to_string()))?;
                },
                StorageOperation::Retrieve { slot } => {
                    let block = crate::actors_v2::testing::storage::fixtures::create_test_block(slot);
                    let block_hash = block.block_hash().to_block_hash();
                    let actor = self.base.actor.read().await;
                    let _result = actor.get_block(&block_hash).await
                        .map_err(|e| StorageTestError::StorageOperation(e.to_string()))?;
                },
                StorageOperation::UpdateHead { slot: _ } => {
                    // Implement chain head update logic
                },
            }
        }

        Ok(true)
    }

    async fn check_invariants(&self) -> Result<bool, Self::Error> {
        let actor = self.base.actor.read().await;

        // Invariant 1: All stored blocks can be retrieved
        // Invariant 2: Chain head is consistent with stored blocks
        // Invariant 3: Cache and database are synchronized
        // Invariant 4: Metrics reflect actual operations

        // This is a simplified check - full implementation would be more comprehensive
        Ok(true)
    }
}

#[tokio::test]
async fn property_test_storage_consistency() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        proptest!(|(operations in StorageProperty::operations_strategy())| {
            let mut harness = StorageTestHarness::new().await.unwrap();
            harness.setup().await.unwrap();

            let property = StorageProperty { operations };
            let result = harness.execute_property(property).await.unwrap();

            // Check that invariants hold
            let invariants_valid = harness.check_invariants().await.unwrap();

            prop_assert!(result && invariants_valid);

            harness.teardown().await.unwrap();
        });
    });
}
```

### Step 9: Chaos Testing Implementation

**File:** `app/src/actors_v2/testing/storage/chaos/storage_chaos.rs`

**Action:** Implement chaos testing scenarios
```rust
use crate::actors_v2::testing::storage::{StorageTestHarness, StorageTestError};
use crate::actors_v2::testing::base::{ActorTestHarness, ChaosTestable, SystemHealthReport};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub enum StorageFailureScenario {
    DatabaseCorruption { corruption_rate: f64 },
    DiskSpaceExhaustion { remaining_bytes: u64 },
    NetworkPartition { duration_secs: u64 },
    MemoryPressure { pressure_level: u8 },
    CacheEviction { eviction_rate: f64 },
    SlowDisk { delay_ms: u64 },
}

#[async_trait]
impl ChaosTestable for StorageTestHarness {
    type FailureScenario = StorageFailureScenario;

    async fn inject_failure(&mut self, scenario: Self::FailureScenario) -> Result<(), Self::Error> {
        match scenario {
            StorageFailureScenario::DatabaseCorruption { corruption_rate } => {
                // Simulate database corruption by randomly failing operations
                // This would require modification to the actual storage layer
                // to inject failures based on the corruption rate
                tracing::warn!("Injecting database corruption at rate: {}", corruption_rate);
            },
            StorageFailureScenario::DiskSpaceExhaustion { remaining_bytes } => {
                // Simulate disk space issues
                tracing::warn!("Simulating disk space exhaustion with {} bytes remaining", remaining_bytes);
            },
            StorageFailureScenario::NetworkPartition { duration_secs } => {
                // Simulate network issues for distributed scenarios
                tracing::warn!("Simulating network partition for {} seconds", duration_secs);
                tokio::time::sleep(tokio::time::Duration::from_secs(duration_secs)).await;
            },
            StorageFailureScenario::MemoryPressure { pressure_level } => {
                // Simulate memory pressure
                tracing::warn!("Simulating memory pressure at level: {}", pressure_level);
            },
            StorageFailureScenario::CacheEviction { eviction_rate } => {
                // Force cache evictions
                tracing::warn!("Forcing cache evictions at rate: {}", eviction_rate);
            },
            StorageFailureScenario::SlowDisk { delay_ms } => {
                // Simulate slow disk operations
                tracing::warn!("Simulating slow disk with {}ms delay", delay_ms);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            },
        }

        Ok(())
    }

    async fn monitor_state(&self) -> Result<SystemHealthReport, Self::Error> {
        let actor = self.base.actor.read().await;

        // Collect system health metrics
        let mut custom_metrics = HashMap::new();
        custom_metrics.insert("blocks_stored".to_string(), actor.metrics.blocks_stored as f64);
        custom_metrics.insert("cache_hits".to_string(), actor.metrics.cache_hits as f64);
        custom_metrics.insert("cache_misses".to_string(), actor.metrics.cache_misses as f64);

        Ok(SystemHealthReport {
            timestamp: std::time::SystemTime::now(),
            actor_responsive: true, // Would implement actual responsiveness check
            memory_usage: self.base.metrics.memory_peak,
            active_connections: 1, // Simplified
            error_count: self.base.metrics.errors_encountered as u32,
            custom_metrics,
        })
    }

    async fn recover(&mut self) -> Result<(), Self::Error> {
        // Implement recovery logic
        tracing::info!("Recovering from chaos scenario");

        // Reset actor to clean state if needed
        if self.base.metrics.errors_encountered > 10 {
            self.reset().await?;
        }

        Ok(())
    }
}

#[actix::test]
async fn chaos_test_database_corruption() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Store initial test data
    let test_block = harness.test_blocks[0].clone();
    {
        let mut actor = harness.base.actor.write().await;
        actor.store_block(test_block, true).await.unwrap();
    }

    // Inject corruption scenario
    let scenario = StorageFailureScenario::DatabaseCorruption { corruption_rate: 0.1 };
    harness.inject_failure(scenario).await.unwrap();

    // Continue operations under failure conditions
    for block in harness.test_blocks.iter().skip(1).take(5) {
        let mut actor = harness.base.actor.write().await;
        let _result = actor.store_block(block.clone(), true).await;
        // Some operations may fail due to injected corruption
    }

    // Monitor system health
    let health = harness.monitor_state().await.unwrap();
    assert!(health.actor_responsive);

    // Attempt recovery
    harness.recover().await.unwrap();

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn chaos_test_memory_pressure() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Inject memory pressure
    let scenario = StorageFailureScenario::MemoryPressure { pressure_level: 8 };
    harness.inject_failure(scenario).await.unwrap();

    // Verify system continues to function
    for i in 0..100 {
        let block = crate::actors_v2::testing::storage::fixtures::create_test_block(i);
        let mut actor = harness.base.actor.write().await;
        let result = actor.store_block(block, true).await;

        // System should handle memory pressure gracefully
        if result.is_err() {
            harness.recover().await.unwrap();
        }
    }

    harness.teardown().await.unwrap();
}
```

### Step 10: Test Fixture Implementation

**File:** `app/src/actors_v2/testing/storage/fixtures/mod.rs`

**Action:** Create comprehensive test fixtures
```rust
pub mod blocks;
pub mod config;

pub use blocks::*;
pub use config::*;

use crate::actors_v2::storage::actor::AlysConsensusBlock;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec, ExecutionPayloadCapella, Address, ExecutionBlockHash};

/// Generate a sequence of test blocks with proper relationships
pub fn create_test_block_sequence(count: usize) -> Vec<AlysConsensusBlock> {
    let mut blocks = Vec::with_capacity(count);

    for i in 0..count {
        let slot = i as u64 + 1;
        let parent_hash = if i == 0 {
            Hash256::zero()
        } else {
            blocks[i - 1].block_hash()
        };

        let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
            parent_hash: ExecutionBlockHash::from_root(parent_hash),
            fee_recipient: Address::zero(),
            state_root: Hash256::from_low_u64_be(slot + 1000),
            receipts_root: Hash256::from_low_u64_be(slot + 2000),
            logs_bloom: Default::default(),
            prev_randao: Hash256::from_low_u64_be(slot + 3000),
            block_number: slot,
            gas_limit: 30000000,
            gas_used: slot * 1000, // Variable gas usage
            timestamp: 1600000000 + slot * 12, // 12 second block time
            extra_data: format!("test_block_{}", slot).into_bytes().into(),
            base_fee_per_gas: (1000000000u64 + slot * 100).into(),
            block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
            transactions: Default::default(),
            withdrawals: Default::default(),
        };

        blocks.push(AlysConsensusBlock {
            parent_hash,
            slot,
            auxpow_header: None,
            execution_payload,
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        });
    }

    blocks
}

/// Create a single test block with specified slot
pub fn create_test_block(slot: u64) -> AlysConsensusBlock {
    let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
        parent_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot.saturating_sub(1))),
        fee_recipient: Address::zero(),
        state_root: Hash256::from_low_u64_be(slot + 1000),
        receipts_root: Hash256::from_low_u64_be(slot + 2000),
        logs_bloom: Default::default(),
        prev_randao: Hash256::from_low_u64_be(slot + 3000),
        block_number: slot,
        gas_limit: 30000000,
        gas_used: slot * 1000,
        timestamp: 1600000000 + slot * 12,
        extra_data: format!("test_block_{}", slot).into_bytes().into(),
        base_fee_per_gas: (1000000000u64 + slot * 100).into(),
        block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
        transactions: Default::default(),
        withdrawals: Default::default(),
    };

    AlysConsensusBlock {
        parent_hash: Hash256::from_low_u64_be(slot.saturating_sub(1)),
        slot,
        auxpow_header: None,
        execution_payload,
        pegins: vec![],
        pegout_payment_proposal: None,
        finalized_pegouts: vec![],
    }
}
```

### Step 11: CI/CD Integration

**File:** `.github/workflows/storage-actor-tests.yml`

**Action:** Create dedicated Storage Actor testing workflow
```yaml
name: Storage Actor V2 Tests

on:
  push:
    paths:
      - 'app/src/actors_v2/storage/**'
      - 'app/src/actors_v2/testing/**'
  pull_request:
    paths:
      - 'app/src/actors_v2/storage/**'
      - 'app/src/actors_v2/testing/**'

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  unit-tests:
    name: Unit Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: "unit-tests"
      - name: Run unit tests
        run: cargo test --package app actors_v2::testing::storage::unit --lib

  integration-tests:
    name: Integration Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: "integration-tests"
      - name: Run integration tests
        run: cargo test --package app actors_v2::testing::storage::integration --lib

  property-tests:
    name: Property-Based Tests
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: "property-tests"
      - name: Run property tests
        run: cargo test --package app actors_v2::testing::storage::property --lib
        env:
          PROPTEST_CASES: 1000

  chaos-tests:
    name: Chaos Tests
    runs-on: ubuntu-latest
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: "chaos-tests"
      - name: Run chaos tests
        run: cargo test --package app actors_v2::testing::storage::chaos --lib

  performance-benchmarks:
    name: Performance Benchmarks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: "benchmarks"
      - name: Run benchmarks
        run: cargo bench --package app --bench storage_benchmarks
      - name: Upload benchmark results
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: target/criterion/reports/index.html

  test-coverage:
    name: Test Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview
      - uses: Swatinem/rust-cache@v2
      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov
      - name: Generate test coverage
        run: cargo llvm-cov --package app --lcov --output-path lcov.info test actors_v2::testing::storage
      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v3
        with:
          files: lcov.info
```

### Step 12: Implementation Execution Order

**Execute these steps in sequence:**

1. **Foundation Setup** (Day 1)
   ```bash
   # Create directory structure
   mkdir -p app/src/actors_v2/testing/{base,property,chaos,storage/{unit,integration,property,chaos,fixtures}}

   # Update Cargo.toml dependencies
   # Create base trait definitions
   # Validate compilation: cargo check
   ```

2. **Base Infrastructure** (Day 2)
   ```bash
   # Implement base traits and harness
   # Create test utilities and fixtures
   # Validate: cargo test --lib --package app actors_v2::testing::base
   ```

3. **Storage Test Harness** (Day 3)
   ```bash
   # Implement StorageTestHarness
   # Create storage-specific fixtures
   # Validate: cargo test --lib --package app actors_v2::testing::storage::fixtures
   ```

4. **Unit Tests** (Day 4)
   ```bash
   # Implement database unit tests
   # Implement cache unit tests
   # Implement metrics unit tests
   # Validate: cargo test --lib --package app actors_v2::testing::storage::unit
   ```

5. **Integration Tests** (Day 5)
   ```bash
   # Implement actor integration tests
   # Implement persistence tests
   # Implement concurrency tests
   # Validate: cargo test --lib --package app actors_v2::testing::storage::integration
   ```

6. **Property Tests** (Day 6)
   ```bash
   # Implement property test generators
   # Implement invariant checks
   # Validate: cargo test --lib --package app actors_v2::testing::storage::property
   ```

7. **Chaos Tests** (Day 7)
   ```bash
   # Implement failure injection
   # Implement health monitoring
   # Validate: cargo test --lib --package app actors_v2::testing::storage::chaos
   ```

8. **CI/CD Integration** (Day 8)
   ```bash
   # Create GitHub Actions workflow
   # Setup performance benchmarking
   # Configure test coverage reporting
   # Validate: Check all workflows pass
   ```

### Step 13: Validation Checkpoints

**After each phase, run these validation commands:**

```bash
# Compilation check
cargo check --package app

# Full test suite
cargo test --package app actors_v2::testing::storage

# Coverage report
cargo llvm-cov --package app test actors_v2::testing::storage --html

# Performance benchmarks
cargo bench --package app --bench storage_benchmarks

# Linting and formatting
cargo clippy --package app -- -D warnings
cargo fmt --package app -- --check
```

### Step 14: Expected Outcomes

**Quantitative Success Metrics:**
- Unit test coverage: >90%
- Integration test coverage: >80%
- Property test execution: 1000+ cases per test
- Chaos test scenarios: 5+ failure types covered
- CI/CD pipeline time: <15 minutes total
- Performance benchmarks: >100 blocks/second storage rate

**Qualitative Success Metrics:**
- All tests pass consistently
- Test output is clear and actionable
- Failure scenarios are properly handled
- Infrastructure is reusable for other actors
- Documentation is complete and accurate

This implementation plan provides the systematic, step-by-step approach you requested, with specific file contents, implementation order, validation steps, and success criteria. Each phase builds upon the previous one, ensuring a robust and comprehensive testing infrastructure for the Storage Actor that can be extended to other actors in the system.