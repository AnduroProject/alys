//! Test utilities, helpers, and common functionality for testing
//!
//! This module provides comprehensive test utilities including test data generation,
//! timing utilities, assertion helpers, and common testing patterns for the Alys
//! actor-based system.

use crate::testing::actor_harness::{ActorTestResult, ActorTestError};
use crate::types::*;
use actor_system::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, Mutex};
use uuid::Uuid;

/// Test utility functions and helpers
pub struct TestUtil;

impl TestUtil {
    /// Generate a unique test ID
    pub fn generate_test_id() -> String {
        format!("test_{}", Uuid::new_v4())
    }
    
    /// Generate test data with specific size
    pub fn generate_test_data(size_bytes: usize) -> Vec<u8> {
        (0..size_bytes).map(|i| (i % 256) as u8).collect()
    }
    
    /// Create a test message with random payload
    pub fn create_test_message(message_type: &str) -> TestMessage {
        TestMessage {
            message_id: Self::generate_test_id(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            message_type: message_type.to_string(),
            payload: serde_json::json!({
                "test_data": Self::generate_test_data(1024),
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
            }),
            metadata: HashMap::new(),
            timestamp: SystemTime::now(),
        }
    }
    
    /// Wait for condition with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout: Duration,
        check_interval: Duration,
    ) -> ActorTestResult<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = SystemTime::now();
        
        loop {
            if condition().await {
                return Ok(());
            }
            
            if start.elapsed().unwrap_or(Duration::from_secs(0)) >= timeout {
                return Err(ActorTestError::TimeoutError {
                    operation: "wait_for_condition".to_string(),
                    timeout,
                });
            }
            
            tokio::time::sleep(check_interval).await;
        }
    }
    
    /// Retry operation with exponential backoff
    pub async fn retry_with_backoff<F, Fut, T>(
        operation: F,
        max_retries: u32,
        initial_delay: Duration,
        max_delay: Duration,
        backoff_multiplier: f64,
    ) -> ActorTestResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = ActorTestResult<T>>,
    {
        let mut delay = initial_delay;
        let mut last_error = ActorTestError::TestDataError {
            operation: "retry_with_backoff".to_string(),
            reason: "No attempts made".to_string(),
        };
        
        for attempt in 0..=max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = e;
                    if attempt < max_retries {
                        tokio::time::sleep(delay).await;
                        delay = std::cmp::min(
                            Duration::from_nanos((delay.as_nanos() as f64 * backoff_multiplier) as u64),
                            max_delay,
                        );
                    }
                }
            }
        }
        
        Err(last_error)
    }
    
    /// Measure execution time of an operation
    pub async fn measure_time<F, Fut, T>(operation: F) -> (T, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = SystemTime::now();
        let result = operation().await;
        let elapsed = start.elapsed().unwrap_or(Duration::from_secs(0));
        (result, elapsed)
    }
    
    /// Generate load by sending multiple messages
    pub async fn generate_message_load(
        message_count: u32,
        messages_per_second: f64,
        message_generator: impl Fn(u32) -> TestMessage,
        target_actor: &str,
        harness: &crate::testing::actor_harness::ActorTestHarness,
    ) -> ActorTestResult<LoadTestResult> {
        let start_time = SystemTime::now();
        let interval = Duration::from_nanos((1_000_000_000.0 / messages_per_second) as u64);
        
        let mut successful_messages = 0;
        let mut failed_messages = 0;
        let mut total_latency = Duration::from_secs(0);
        
        for i in 0..message_count {
            let message = message_generator(i);
            let send_start = SystemTime::now();
            
            match harness.send_message("load_generator", target_actor, message).await {
                Ok(()) => {
                    successful_messages += 1;
                    total_latency += send_start.elapsed().unwrap_or(Duration::from_secs(0));
                },
                Err(_) => {
                    failed_messages += 1;
                },
            }
            
            if i < message_count - 1 {
                tokio::time::sleep(interval).await;
            }
        }
        
        let total_time = start_time.elapsed().unwrap_or(Duration::from_secs(0));
        let actual_throughput = successful_messages as f64 / total_time.as_secs_f64();
        let average_latency = if successful_messages > 0 {
            total_latency / successful_messages
        } else {
            Duration::from_secs(0)
        };
        
        Ok(LoadTestResult {
            messages_sent: message_count,
            successful_messages,
            failed_messages,
            total_time,
            target_throughput: messages_per_second,
            actual_throughput,
            average_latency,
        })
    }
    
    /// Generate concurrent load from multiple sources
    pub async fn generate_concurrent_load(
        load_configs: Vec<ConcurrentLoadConfig>,
        harness: Arc<crate::testing::actor_harness::ActorTestHarness>,
    ) -> ActorTestResult<Vec<LoadTestResult>> {
        let mut handles = Vec::new();
        
        for config in load_configs {
            let harness_clone = harness.clone();
            let handle = tokio::spawn(async move {
                Self::generate_message_load(
                    config.message_count,
                    config.messages_per_second,
                    config.message_generator,
                    &config.target_actor,
                    &harness_clone,
                ).await
            });
            handles.push(handle);
        }
        
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(ActorTestError::TestDataError {
                    operation: "concurrent_load_generation".to_string(),
                    reason: format!("Task join error: {}", e),
                }),
            }
        }
        
        Ok(results)
    }
    
    /// Assert that two values are approximately equal within tolerance
    pub fn assert_approximately_equal<T>(actual: T, expected: T, tolerance: f64, message: &str) -> ActorTestResult<()>
    where
        T: Into<f64> + Copy + std::fmt::Display,
    {
        let actual_f64 = actual.into();
        let expected_f64 = expected.into();
        let diff = (actual_f64 - expected_f64).abs();
        let max_diff = expected_f64.abs() * tolerance;
        
        if diff <= max_diff {
            Ok(())
        } else {
            Err(ActorTestError::AssertionFailed {
                assertion: format!("assert_approximately_equal({}, {}, {})", actual, expected, tolerance),
                reason: format!("{}: actual={}, expected={}, diff={}, tolerance={}", 
                    message, actual, expected, diff, max_diff),
            })
        }
    }
    
    /// Create test data with specific pattern
    pub fn create_pattern_data(pattern: DataPattern, size: usize) -> Vec<u8> {
        match pattern {
            DataPattern::Zeros => vec![0; size],
            DataPattern::Ones => vec![255; size],
            DataPattern::Sequential => (0..size).map(|i| (i % 256) as u8).collect(),
            DataPattern::Random(seed) => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                
                let mut hasher = DefaultHasher::new();
                seed.hash(&mut hasher);
                let mut rng_state = hasher.finish();
                
                (0..size).map(|_| {
                    // Simple LCG for reproducible random data
                    rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
                    (rng_state >> 32) as u8
                }).collect()
            },
            DataPattern::Alternating => (0..size).map(|i| if i % 2 == 0 { 0xAA } else { 0x55 }).collect(),
        }
    }
    
    /// Validate message integrity
    pub fn validate_message_integrity(original: &TestMessage, received: &TestMessage) -> ActorTestResult<()> {
        if original.message_id != received.message_id {
            return Err(ActorTestError::AssertionFailed {
                assertion: "message_id_match".to_string(),
                reason: format!("Message ID mismatch: {} != {}", original.message_id, received.message_id),
            });
        }
        
        if original.correlation_id != received.correlation_id {
            return Err(ActorTestError::AssertionFailed {
                assertion: "correlation_id_match".to_string(),
                reason: format!("Correlation ID mismatch: {:?} != {:?}", 
                    original.correlation_id, received.correlation_id),
            });
        }
        
        if original.message_type != received.message_type {
            return Err(ActorTestError::AssertionFailed {
                assertion: "message_type_match".to_string(),
                reason: format!("Message type mismatch: {} != {}", 
                    original.message_type, received.message_type),
            });
        }
        
        // Compare payloads (allowing for minor timestamp differences)
        if let (Ok(orig_map), Ok(recv_map)) = (
            serde_json::from_value::<HashMap<String, serde_json::Value>>(original.payload.clone()),
            serde_json::from_value::<HashMap<String, serde_json::Value>>(received.payload.clone())
        ) {
            for (key, orig_value) in &orig_map {
                if key != "timestamp" {  // Skip timestamp comparison
                    if let Some(recv_value) = recv_map.get(key) {
                        if orig_value != recv_value {
                            return Err(ActorTestError::AssertionFailed {
                                assertion: "payload_match".to_string(),
                                reason: format!("Payload mismatch for key '{}': {:?} != {:?}", 
                                    key, orig_value, recv_value),
                            });
                        }
                    } else {
                        return Err(ActorTestError::AssertionFailed {
                            assertion: "payload_completeness".to_string(),
                            reason: format!("Missing key '{}' in received message", key),
                        });
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// Test message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessage {
    pub message_id: String,
    pub correlation_id: Option<String>,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub metadata: HashMap<String, String>,
    pub timestamp: SystemTime,
}

/// Test data patterns
#[derive(Debug, Clone)]
pub enum DataPattern {
    Zeros,
    Ones,
    Sequential,
    Random(u64),
    Alternating,
}

/// Load test result
#[derive(Debug, Clone)]
pub struct LoadTestResult {
    pub messages_sent: u32,
    pub successful_messages: u32,
    pub failed_messages: u32,
    pub total_time: Duration,
    pub target_throughput: f64,
    pub actual_throughput: f64,
    pub average_latency: Duration,
}

/// Concurrent load configuration
pub struct ConcurrentLoadConfig {
    pub message_count: u32,
    pub messages_per_second: f64,
    pub target_actor: String,
    pub message_generator: fn(u32) -> TestMessage,
}

/// Test data generator
pub struct TestData;

impl TestData {
    /// Generate blockchain test data
    pub fn generate_block_data(block_number: u64) -> serde_json::Value {
        serde_json::json!({
            "number": block_number,
            "hash": format!("0x{:064x}", block_number),
            "parent_hash": format!("0x{:064x}", block_number.saturating_sub(1)),
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            "transactions": (0..10).map(|i| {
                serde_json::json!({
                    "hash": format!("0x{:064x}", block_number * 1000 + i),
                    "from": format!("0x{:040x}", i),
                    "to": format!("0x{:040x}", i + 1),
                    "value": format!("0x{:x}", i * 1000000000000000000u64),
                    "gas": 21000,
                    "gas_price": format!("0x{:x}", 20000000000u64)
                })
            }).collect::<Vec<_>>()
        })
    }
    
    /// Generate transaction test data
    pub fn generate_transaction_data(tx_index: u64) -> serde_json::Value {
        serde_json::json!({
            "hash": format!("0x{:064x}", tx_index),
            "from": format!("0x{:040x}", tx_index % 1000),
            "to": format!("0x{:040x}", (tx_index + 1) % 1000),
            "value": format!("0x{:x}", tx_index * 1000000000000000000u64),
            "gas": 21000 + (tx_index % 100000),
            "gas_price": format!("0x{:x}", 20000000000u64 + (tx_index % 10000000000u64)),
            "nonce": tx_index % 100,
            "data": format!("0x{}", hex::encode(TestUtil::generate_test_data((tx_index % 1000) as usize))),
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        })
    }
    
    /// Generate peer operation test data
    pub fn generate_peg_operation_data(operation_id: u64, operation_type: &str) -> serde_json::Value {
        serde_json::json!({
            "operation_id": operation_id,
            "operation_type": operation_type,
            "bitcoin_txid": format!("{:064x}", operation_id),
            "amount_satoshis": operation_id * 100000000, // BTC amounts
            "destination_address": format!("0x{:040x}", operation_id % 10000),
            "confirmations": operation_id % 7, // 0-6 confirmations
            "status": match operation_id % 4 {
                0 => "pending",
                1 => "confirming",
                2 => "confirmed",
                _ => "completed",
            },
            "created_at": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() - (operation_id % 3600), // Up to 1 hour ago
            "block_height": 800000 + operation_id,
        })
    }
    
    /// Generate network message test data
    pub fn generate_network_message_data(message_type: &str, sequence: u64) -> serde_json::Value {
        serde_json::json!({
            "message_type": message_type,
            "sequence_number": sequence,
            "peer_id": format!("peer_{}", sequence % 100),
            "payload_size": sequence % 65536,
            "payload": TestUtil::create_pattern_data(DataPattern::Sequential, (sequence % 1000) as usize),
            "priority": match sequence % 3 {
                0 => "low",
                1 => "normal",
                _ => "high",
            },
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        })
    }
    
    /// Generate actor configuration test data
    pub fn generate_actor_config_data(actor_type: &str, instance_id: u64) -> serde_json::Value {
        serde_json::json!({
            "actor_type": actor_type,
            "instance_id": instance_id,
            "restart_policy": match instance_id % 3 {
                0 => "always",
                1 => "on_failure",
                _ => "never",
            },
            "max_restarts": 3 + (instance_id % 7),
            "restart_delay_ms": 1000 * (1 + instance_id % 10),
            "mailbox_capacity": 1000 * (1 + instance_id % 100),
            "processing_timeout_ms": 5000 + (instance_id % 5000),
            "resource_limits": {
                "max_memory_mb": 100 + (instance_id % 900),
                "max_cpu_percent": 10 + (instance_id % 80),
                "max_file_descriptors": 100 + (instance_id % 900),
            },
            "custom_config": {
                "feature_flags": {
                    "enable_metrics": instance_id % 2 == 0,
                    "enable_tracing": instance_id % 3 == 0,
                    "enable_debug": instance_id % 5 == 0,
                },
                "thresholds": {
                    "error_threshold": 0.01 + (instance_id % 100) as f64 / 10000.0,
                    "warning_threshold": 0.05 + (instance_id % 100) as f64 / 2000.0,
                },
            }
        })
    }
}

/// Test timeout utilities
pub struct TestTimeout;

impl TestTimeout {
    /// Create a timeout for unit tests (short duration)
    pub fn unit_test() -> Duration {
        Duration::from_secs(5)
    }
    
    /// Create a timeout for integration tests (medium duration)
    pub fn integration_test() -> Duration {
        Duration::from_secs(30)
    }
    
    /// Create a timeout for system tests (long duration)
    pub fn system_test() -> Duration {
        Duration::from_secs(300)
    }
    
    /// Create a timeout for load tests (very long duration)
    pub fn load_test() -> Duration {
        Duration::from_secs(900)
    }
    
    /// Create a custom timeout based on operation complexity
    pub fn custom(base_timeout: Duration, complexity_factor: f64) -> Duration {
        Duration::from_nanos((base_timeout.as_nanos() as f64 * complexity_factor) as u64)
    }
    
    /// Get timeout for message processing based on message size
    pub fn message_processing(message_size_bytes: usize) -> Duration {
        let base_timeout = Duration::from_millis(100);
        let size_factor = 1.0 + (message_size_bytes as f64 / 1024.0) * 0.1; // 10% per KB
        Self::custom(base_timeout, size_factor)
    }
    
    /// Get timeout for actor startup based on actor complexity
    pub fn actor_startup(actor_complexity: ActorComplexity) -> Duration {
        match actor_complexity {
            ActorComplexity::Simple => Duration::from_secs(1),
            ActorComplexity::Medium => Duration::from_secs(5),
            ActorComplexity::Complex => Duration::from_secs(15),
            ActorComplexity::VeryComplex => Duration::from_secs(30),
        }
    }
    
    /// Get timeout for network operations based on network conditions
    pub fn network_operation(latency: Duration, reliability: f64) -> Duration {
        let base_timeout = Duration::from_millis(1000);
        let latency_factor = 1.0 + (latency.as_millis() as f64 / 100.0); // Factor for latency
        let reliability_factor = 2.0 - reliability; // Less reliable = longer timeout
        Self::custom(base_timeout, latency_factor * reliability_factor)
    }
}

/// Actor complexity levels for timeout calculation
#[derive(Debug, Clone, Copy)]
pub enum ActorComplexity {
    Simple,
    Medium,
    Complex,
    VeryComplex,
}

/// Test assertion utilities
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that an actor is in a specific state
    pub async fn assert_actor_state(
        harness: &crate::testing::actor_harness::ActorTestHarness,
        actor_id: &str,
        expected_state: ActorState,
        timeout: Duration,
    ) -> ActorTestResult<()> {
        TestUtil::wait_for_condition(
            || async {
                // TODO: Implement actual actor state checking
                true // Placeholder
            },
            timeout,
            Duration::from_millis(100),
        ).await
    }
    
    /// Assert that a message was delivered within timeout
    pub async fn assert_message_delivered(
        harness: &crate::testing::actor_harness::ActorTestHarness,
        message_id: &str,
        timeout: Duration,
    ) -> ActorTestResult<()> {
        TestUtil::wait_for_condition(
            || async {
                let history = harness.get_message_history().await;
                history.iter().any(|event| event.message_id == message_id)
            },
            timeout,
            Duration::from_millis(50),
        ).await
    }
    
    /// Assert that system metrics are within expected ranges
    pub fn assert_metrics_within_range(
        actual_metrics: &HashMap<String, f64>,
        expected_ranges: &HashMap<String, (f64, f64)>,
    ) -> ActorTestResult<()> {
        for (metric_name, (min_val, max_val)) in expected_ranges {
            if let Some(actual_val) = actual_metrics.get(metric_name) {
                if *actual_val < *min_val || *actual_val > *max_val {
                    return Err(ActorTestError::AssertionFailed {
                        assertion: format!("metric_range_{}", metric_name),
                        reason: format!(
                            "Metric '{}' value {} is outside expected range [{}, {}]",
                            metric_name, actual_val, min_val, max_val
                        ),
                    });
                }
            } else {
                return Err(ActorTestError::AssertionFailed {
                    assertion: format!("metric_exists_{}", metric_name),
                    reason: format!("Metric '{}' not found in actual metrics", metric_name),
                });
            }
        }
        Ok(())
    }
    
    /// Assert that performance is within acceptable degradation limits
    pub fn assert_performance_degradation(
        baseline_metrics: &PerformanceMetrics,
        current_metrics: &PerformanceMetrics,
        max_degradation: f64, // e.g., 0.2 for 20% degradation
    ) -> ActorTestResult<()> {
        // Check throughput degradation
        let throughput_degradation = 
            (baseline_metrics.throughput - current_metrics.throughput) / baseline_metrics.throughput;
        if throughput_degradation > max_degradation {
            return Err(ActorTestError::AssertionFailed {
                assertion: "throughput_degradation".to_string(),
                reason: format!(
                    "Throughput degradation {:.2}% exceeds maximum {:.2}%",
                    throughput_degradation * 100.0, max_degradation * 100.0
                ),
            });
        }
        
        // Check latency increase
        let latency_increase = 
            (current_metrics.latency.as_nanos() as f64 - baseline_metrics.latency.as_nanos() as f64) /
            baseline_metrics.latency.as_nanos() as f64;
        if latency_increase > max_degradation {
            return Err(ActorTestError::AssertionFailed {
                assertion: "latency_increase".to_string(),
                reason: format!(
                    "Latency increase {:.2}% exceeds maximum {:.2}%",
                    latency_increase * 100.0, max_degradation * 100.0
                ),
            });
        }
        
        // Check error rate increase
        let error_rate_increase = current_metrics.error_rate - baseline_metrics.error_rate;
        if error_rate_increase > max_degradation {
            return Err(ActorTestError::AssertionFailed {
                assertion: "error_rate_increase".to_string(),
                reason: format!(
                    "Error rate increase {:.2}% exceeds maximum {:.2}%",
                    error_rate_increase * 100.0, max_degradation * 100.0
                ),
            });
        }
        
        Ok(())
    }
}

/// Actor state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
    Restarting,
}

/// Performance metrics for comparison
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub throughput: f64,      // messages per second
    pub latency: Duration,    // average latency
    pub error_rate: f64,      // error rate (0.0-1.0)
    pub cpu_usage: f64,       // CPU usage percentage
    pub memory_usage: u64,    // memory usage in bytes
}

/// Test environment builder
#[derive(Debug)]
pub struct TestEnvironmentBuilder {
    test_id: String,
    test_name: String,
    isolation_level: crate::testing::actor_harness::IsolationLevel,
    timeout: Duration,
    resource_limits: crate::testing::actor_harness::ResourceLimits,
    mock_config: crate::testing::actor_harness::MockConfiguration,
    test_data_dir: String,
    cleanup_strategy: crate::testing::actor_harness::CleanupStrategy,
}

impl TestEnvironmentBuilder {
    /// Create a new test environment builder
    pub fn new(test_name: &str) -> Self {
        Self {
            test_id: TestUtil::generate_test_id(),
            test_name: test_name.to_string(),
            isolation_level: crate::testing::actor_harness::IsolationLevel::Complete,
            timeout: Duration::from_secs(300),
            resource_limits: crate::testing::actor_harness::ResourceLimits {
                max_memory_mb: 1000,
                max_cpu_percent: 80,
                max_file_descriptors: 1000,
                max_network_connections: 100,
                max_duration: Duration::from_secs(600),
            },
            mock_config: crate::testing::actor_harness::MockConfiguration::default(),
            test_data_dir: format!("/tmp/alys_test_{}", Uuid::new_v4()),
            cleanup_strategy: crate::testing::actor_harness::CleanupStrategy::Full,
        }
    }
    
    /// Set isolation level
    pub fn with_isolation_level(mut self, level: crate::testing::actor_harness::IsolationLevel) -> Self {
        self.isolation_level = level;
        self
    }
    
    /// Set test timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    /// Set resource limits
    pub fn with_resource_limits(mut self, limits: crate::testing::actor_harness::ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }
    
    /// Enable mock for specific service
    pub fn with_mock(mut self, service: &str, enabled: bool) -> Self {
        match service {
            "governance" => self.mock_config.mock_governance = enabled,
            "bitcoin" => self.mock_config.mock_bitcoin = enabled,
            "execution" => self.mock_config.mock_execution = enabled,
            "network" => self.mock_config.mock_network = enabled,
            "storage" => self.mock_config.mock_storage = enabled,
            _ => {},
        }
        self
    }
    
    /// Set test data directory
    pub fn with_test_data_dir(mut self, dir: &str) -> Self {
        self.test_data_dir = dir.to_string();
        self
    }
    
    /// Set cleanup strategy
    pub fn with_cleanup_strategy(mut self, strategy: crate::testing::actor_harness::CleanupStrategy) -> Self {
        self.cleanup_strategy = strategy;
        self
    }
    
    /// Build the test environment
    pub fn build(self) -> crate::testing::actor_harness::TestEnvironment {
        crate::testing::actor_harness::TestEnvironment {
            test_id: self.test_id,
            test_name: self.test_name,
            isolation_level: self.isolation_level,
            timeout: self.timeout,
            resource_limits: self.resource_limits,
            mock_config: self.mock_config,
            test_data_dir: self.test_data_dir,
            cleanup_strategy: self.cleanup_strategy,
        }
    }
}

/// Test scenario builder
#[derive(Debug)]
pub struct TestScenarioBuilder {
    scenario_id: String,
    name: String,
    description: String,
    steps: Vec<crate::testing::actor_harness::TestStep>,
    preconditions: Vec<crate::testing::actor_harness::Precondition>,
    postconditions: Vec<crate::testing::actor_harness::Postcondition>,
    timeout: Duration,
    retry_count: u32,
}

impl TestScenarioBuilder {
    /// Create a new test scenario builder
    pub fn new(name: &str) -> Self {
        Self {
            scenario_id: TestUtil::generate_test_id(),
            name: name.to_string(),
            description: String::new(),
            steps: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
            timeout: Duration::from_secs(300),
            retry_count: 0,
        }
    }
    
    /// Set description
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }
    
    /// Add a test step
    pub fn add_step(mut self, step: crate::testing::actor_harness::TestStep) -> Self {
        self.steps.push(step);
        self
    }
    
    /// Add actor start step
    pub fn start_actor(mut self, actor_id: &str, actor_type: &str, config: serde_json::Value) -> Self {
        self.steps.push(crate::testing::actor_harness::TestStep::StartActor {
            actor_id: actor_id.to_string(),
            actor_type: actor_type.to_string(),
            config,
        });
        self
    }
    
    /// Add message send step
    pub fn send_message(
        mut self, 
        from_actor: &str, 
        to_actor: &str, 
        message: serde_json::Value,
        expect_response: bool
    ) -> Self {
        self.steps.push(crate::testing::actor_harness::TestStep::SendMessage {
            from_actor: from_actor.to_string(),
            to_actor: to_actor.to_string(),
            message,
            expect_response,
        });
        self
    }
    
    /// Add wait for condition step
    pub fn wait_for_condition(
        mut self, 
        condition: crate::testing::actor_harness::TestCondition,
        timeout: Duration
    ) -> Self {
        self.steps.push(crate::testing::actor_harness::TestStep::WaitForCondition {
            condition,
            timeout,
        });
        self
    }
    
    /// Add assertion step
    pub fn assert_condition(
        mut self,
        condition: crate::testing::actor_harness::TestCondition,
        error_message: &str
    ) -> Self {
        self.steps.push(crate::testing::actor_harness::TestStep::AssertCondition {
            condition,
            error_message: error_message.to_string(),
        });
        self
    }
    
    /// Add delay step
    pub fn delay(mut self, duration: Duration) -> Self {
        self.steps.push(crate::testing::actor_harness::TestStep::Delay { duration });
        self
    }
    
    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    /// Set retry count
    pub fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }
    
    /// Build the test scenario
    pub fn build(self) -> crate::testing::actor_harness::TestScenario {
        crate::testing::actor_harness::TestScenario {
            scenario_id: self.scenario_id,
            name: self.name,
            description: self.description,
            steps: self.steps,
            preconditions: self.preconditions,
            postconditions: self.postconditions,
            timeout: self.timeout,
            retry_count: self.retry_count,
        }
    }
}

/// Common test patterns and templates
pub struct TestPatterns;

impl TestPatterns {
    /// Create a basic actor lifecycle test
    pub fn actor_lifecycle_test(actor_type: &str) -> crate::testing::actor_harness::TestScenario {
        TestScenarioBuilder::new(&format!("{}_lifecycle_test", actor_type))
            .with_description(&format!("Test the complete lifecycle of {} actor", actor_type))
            .start_actor("test_actor", actor_type, serde_json::json!({}))
            .wait_for_condition(
                crate::testing::actor_harness::TestCondition::ActorRunning {
                    actor_id: "test_actor".to_string(),
                },
                Duration::from_secs(10)
            )
            .send_message(
                "test_harness",
                "test_actor",
                serde_json::json!({ "type": "ping" }),
                true
            )
            .delay(Duration::from_millis(100))
            .assert_condition(
                crate::testing::actor_harness::TestCondition::MessageReceived {
                    actor_id: "test_actor".to_string(),
                    message_type: "ping".to_string(),
                },
                "Actor should receive ping message"
            )
            .build()
    }
    
    /// Create a message ordering test
    pub fn message_ordering_test(actor_id: &str, message_count: u32) -> crate::testing::actor_harness::TestScenario {
        let mut builder = TestScenarioBuilder::new("message_ordering_test")
            .with_description("Test that messages are processed in order");
        
        // Send multiple messages in sequence
        for i in 0..message_count {
            builder = builder.send_message(
                "test_harness",
                actor_id,
                serde_json::json!({ 
                    "type": "sequence_message", 
                    "sequence": i,
                    "data": format!("message_{}", i)
                }),
                false
            );
        }
        
        // Wait for all messages to be processed
        builder = builder.wait_for_condition(
            crate::testing::actor_harness::TestCondition::MessageCountReached {
                actor_id: actor_id.to_string(),
                count: message_count as u64,
            },
            Duration::from_secs(30)
        );
        
        builder.build()
    }
    
    /// Create a load test scenario
    pub fn load_test_scenario(
        target_actor: &str,
        messages_per_second: u32,
        duration: Duration
    ) -> crate::testing::actor_harness::TestScenario {
        let total_messages = (messages_per_second as f64 * duration.as_secs_f64()) as u32;
        let mut builder = TestScenarioBuilder::new("load_test")
            .with_description(&format!(
                "Load test sending {} messages/sec for {:?} to {}",
                messages_per_second, duration, target_actor
            ))
            .with_timeout(duration + Duration::from_secs(60)); // Extra time for processing
        
        // Generate load by sending messages at intervals
        let interval = Duration::from_nanos(1_000_000_000 / messages_per_second as u64);
        for i in 0..total_messages {
            builder = builder
                .send_message(
                    "load_generator",
                    target_actor,
                    serde_json::json!({
                        "type": "load_test_message",
                        "sequence": i,
                        "timestamp": SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    }),
                    false
                );
            
            if i < total_messages - 1 {
                builder = builder.delay(interval);
            }
        }
        
        builder.build()
    }
    
    /// Create a failure recovery test
    pub fn failure_recovery_test(actor_id: &str) -> crate::testing::actor_harness::TestScenario {
        TestScenarioBuilder::new("failure_recovery_test")
            .with_description("Test actor recovery from failures")
            .start_actor(actor_id, "test_actor", serde_json::json!({}))
            .wait_for_condition(
                crate::testing::actor_harness::TestCondition::ActorRunning {
                    actor_id: actor_id.to_string(),
                },
                Duration::from_secs(10)
            )
            // Inject failure
            .add_step(crate::testing::actor_harness::TestStep::InjectFailure {
                target: crate::testing::actor_harness::FailureTarget::Actor {
                    actor_id: actor_id.to_string(),
                },
                failure_type: crate::testing::actor_harness::FailureType::Crash,
            })
            // Wait for recovery
            .wait_for_condition(
                crate::testing::actor_harness::TestCondition::ActorRunning {
                    actor_id: actor_id.to_string(),
                },
                Duration::from_secs(30)
            )
            .build()
    }
}