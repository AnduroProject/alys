//! Testing Infrastructure for EngineActor
//!
//! Provides comprehensive testing utilities, mocks, and test helpers for the EngineActor module.

pub mod mocks;
pub mod integration;
pub mod performance;
pub mod chaos;
pub mod helpers;

use std::time::Duration;
use actix::prelude::*;

use crate::types::*;
use super::{
    actor::EngineActor,
    config::EngineConfig,
    messages::*,
    EngineResult,
};

/// Test configuration for engine actor testing
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Use mock execution client
    pub use_mock_client: bool,
    
    /// Mock client response delays
    pub mock_response_delay: Duration,
    
    /// Simulate client failures
    pub simulate_failures: bool,
    
    /// Failure rate (0.0 to 1.0)
    pub failure_rate: f64,
    
    /// Test timeout duration
    pub test_timeout: Duration,
    
    /// Enable detailed logging in tests
    pub verbose_logging: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            use_mock_client: true,
            mock_response_delay: Duration::from_millis(10),
            simulate_failures: false,
            failure_rate: 0.0,
            test_timeout: Duration::from_secs(30),
            verbose_logging: false,
        }
    }
}

/// Test result with timing information
#[derive(Debug)]
pub struct TestResult<T> {
    /// The actual result
    pub result: T,
    
    /// Time taken for the operation
    pub duration: Duration,
    
    /// Additional metrics collected during test
    pub metrics: TestMetrics,
}

/// Metrics collected during tests
#[derive(Debug, Default)]
pub struct TestMetrics {
    /// Number of messages sent
    pub messages_sent: u32,
    
    /// Number of client calls made
    pub client_calls: u32,
    
    /// Number of errors encountered
    pub errors: u32,
    
    /// Peak memory usage (if available)
    pub peak_memory: Option<u64>,
}

/// Test utility functions
impl TestConfig {
    /// Create a test configuration for integration tests
    pub fn integration() -> Self {
        Self {
            use_mock_client: false,
            test_timeout: Duration::from_secs(60),
            verbose_logging: true,
            ..Default::default()
        }
    }
    
    /// Create a test configuration for performance tests
    pub fn performance() -> Self {
        Self {
            use_mock_client: true,
            mock_response_delay: Duration::from_millis(1),
            test_timeout: Duration::from_secs(300), // 5 minutes for performance tests
            verbose_logging: false,
            ..Default::default()
        }
    }
    
    /// Create a test configuration for chaos tests
    pub fn chaos() -> Self {
        Self {
            use_mock_client: true,
            simulate_failures: true,
            failure_rate: 0.1, // 10% failure rate
            test_timeout: Duration::from_secs(120),
            verbose_logging: true,
            ..Default::default()
        }
    }
}

/// Initialize test environment
pub fn init_test_env(config: TestConfig) {
    if config.verbose_logging {
        tracing_subscriber::fmt()
            .with_env_filter("debug")
            .init();
    }
}

/// Create a test engine configuration
pub fn create_test_engine_config() -> EngineConfig {
    EngineConfig {
        jwt_secret: [0u8; 32], // Test JWT secret
        engine_url: "http://localhost:8551".to_string(),
        public_url: "http://localhost:8545".to_string(),
        client_type: super::config::ExecutionClientType::Geth,
        performance: super::config::PerformanceConfig::default(),
        actor_integration: super::config::ActorIntegrationConfig::default(),
        health_check: super::config::HealthCheckConfig::default(),
        timeouts: super::config::TimeoutConfig::test_defaults(),
    }
}

/// Wait for actor to reach specific state with timeout
pub async fn wait_for_state<F>(
    actor: &Addr<EngineActor>,
    predicate: F,
    timeout: Duration,
) -> bool
where
    F: Fn(&super::state::ExecutionState) -> bool,
{
    use tokio::time::{sleep, Instant};
    
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        match actor.send(GetEngineStatusMessage {
            include_metrics: false,
            include_payloads: false,
        }).await {
            Ok(Ok(status)) => {
                if predicate(&status.execution_state) {
                    return true;
                }
            },
            _ => {}
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    false
}

/// Measure execution time of an async operation
pub async fn measure_time<F, Fut, T>(f: F) -> TestResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let start = std::time::Instant::now();
    let result = f().await;
    let duration = start.elapsed();
    
    TestResult {
        result,
        duration,
        metrics: TestMetrics::default(),
    }
}

/// Test assertion macros
#[macro_export]
macro_rules! assert_duration_less_than {
    ($duration:expr, $max:expr) => {
        assert!(
            $duration < $max,
            "Duration {:?} exceeds maximum {:?}",
            $duration,
            $max
        );
    };
}

#[macro_export]
macro_rules! assert_actor_state {
    ($actor:expr, $expected_state:pat) => {
        match $actor.send(GetEngineStatusMessage {
            include_metrics: false,
            include_payloads: false,
        }).await {
            Ok(Ok(status)) => {
                assert!(
                    matches!(status.execution_state, $expected_state),
                    "Actor state {:?} does not match expected pattern",
                    status.execution_state
                );
            },
            _ => panic!("Failed to get actor status"),
        }
    };
}

#[cfg(test)]
mod basic_tests {
    use super::*;
    use actix::Actor;
    
    #[actix_rt::test]
    async fn test_config_creation() {
        let config = create_test_engine_config();
        assert_eq!(config.engine_url, "http://localhost:8551");
        assert_eq!(config.jwt_secret, [0u8; 32]);
    }
    
    #[actix_rt::test]
    async fn test_test_config_variants() {
        let integration = TestConfig::integration();
        assert!(!integration.use_mock_client);
        assert!(integration.verbose_logging);
        
        let performance = TestConfig::performance();
        assert!(performance.use_mock_client);
        assert!(!performance.verbose_logging);
        assert_eq!(performance.test_timeout, Duration::from_secs(300));
        
        let chaos = TestConfig::chaos();
        assert!(chaos.simulate_failures);
        assert_eq!(chaos.failure_rate, 0.1);
    }
    
    #[actix_rt::test]
    async fn test_measure_time() {
        let result = measure_time(|| async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        }).await;
        
        assert_eq!(result.result, 42);
        assert!(result.duration >= Duration::from_millis(10));
        assert!(result.duration < Duration::from_millis(50)); // Allow some variance
    }
}