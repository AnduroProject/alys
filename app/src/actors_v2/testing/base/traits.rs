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
    async fn actor(&self) -> &Self::Actor;

    /// Get a mutable reference to the underlying actor
    async fn actor_mut(&mut self) -> &mut Self::Actor;

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