//! Test Helper Functions and Utilities
//!
//! Common utilities and helper functions for EngineActor testing.

use std::time::{Duration, SystemTime};
use actix::prelude::*;
use lighthouse_wrapper::types::{Hash256, Address, MainnetEthSpec};
use lighthouse_wrapper::execution_layer::PayloadAttributes;

use crate::types::*;
use super::super::{
    actor::EngineActor,
    config::EngineConfig,
    messages::*,
    state::ExecutionState,
    EngineResult,
};
use super::mocks::{MockExecutionClient, MockClientConfig};

/// Test helper for creating and managing EngineActor instances
pub struct EngineActorTestHelper {
    /// The actor address
    pub actor: Option<Addr<EngineActor>>,
    
    /// Test configuration
    pub config: super::TestConfig,
}

impl EngineActorTestHelper {
    /// Create a new test helper with default configuration
    pub fn new() -> Self {
        Self {
            actor: None,
            config: super::TestConfig::default(),
        }
    }
    
    /// Create a test helper with custom configuration
    pub fn with_config(config: super::TestConfig) -> Self {
        Self {
            actor: None,
            config,
        }
    }
    
    /// Start the actor with mock client
    pub async fn start_with_mock(&mut self) -> EngineResult<&Addr<EngineActor>> {
        let engine_config = create_mock_engine_config();
        let mock_client = if self.config.simulate_failures {
            MockExecutionClient::with_config(MockClientConfig {
                simulate_failures: true,
                failure_rate: self.config.failure_rate,
                response_delay: self.config.mock_response_delay,
                ..Default::default()
            })
        } else {
            MockExecutionClient::with_config(MockClientConfig {
                response_delay: self.config.mock_response_delay,
                ..Default::default()
            })
        };
        
        // Create actor with mock client (this would need actual implementation)
        // For now, we'll create a placeholder
        let actor = EngineActor::create(|_ctx| {
            // This would need proper initialization with mock client
            // For testing purposes, we need to modify the actor creation
            unimplemented!("Mock actor creation needs implementation")
        });
        
        self.actor = Some(actor);
        Ok(self.actor.as_ref().unwrap())
    }
    
    /// Wait for actor to reach ready state
    pub async fn wait_for_ready(&self, timeout: Duration) -> bool {
        if let Some(actor) = &self.actor {
            super::wait_for_state(
                actor,
                |state| matches!(state, ExecutionState::Ready { .. }),
                timeout,
            ).await
        } else {
            false
        }
    }
    
    /// Wait for actor to reach syncing state
    pub async fn wait_for_syncing(&self, timeout: Duration) -> bool {
        if let Some(actor) = &self.actor {
            super::wait_for_state(
                actor,
                |state| matches!(state, ExecutionState::Syncing { .. }),
                timeout,
            ).await
        } else {
            false
        }
    }
    
    /// Send health check message
    pub async fn health_check(&self) -> EngineResult<()> {
        if let Some(actor) = &self.actor {
            actor.send(HealthCheckMessage).await
                .map_err(|e| super::super::EngineError::ActorError(format!("Mailbox error: {}", e)))?
        } else {
            Err(super::super::EngineError::ActorError("Actor not started".to_string()))
        }
    }
    
    /// Get current engine status
    pub async fn get_status(&self, include_metrics: bool) -> EngineResult<EngineStatusResponse> {
        if let Some(actor) = &self.actor {
            actor.send(GetEngineStatusMessage {
                include_metrics,
                include_payloads: true,
            }).await
                .map_err(|e| super::super::EngineError::ActorError(format!("Mailbox error: {}", e)))?
                .map_err(Into::into)
        } else {
            Err(super::super::EngineError::ActorError("Actor not started".to_string()))
        }
    }
    
    /// Build a test payload
    pub async fn build_payload(&self, parent_hash: Hash256) -> EngineResult<BuildPayloadResult> {
        if let Some(actor) = &self.actor {
            let msg = BuildPayloadMessage {
                parent_hash,
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                fee_recipient: Address::zero(),
                prev_randao: Hash256::random(),
                withdrawals: vec![],
                correlation_id: Some(format!("test_{}", rand::random::<u64>())),
            };
            
            actor.send(msg).await
                .map_err(|e| super::super::EngineError::ActorError(format!("Mailbox error: {}", e)))?
                .map_err(Into::into)
        } else {
            Err(super::super::EngineError::ActorError("Actor not started".to_string()))
        }
    }
    
    /// Execute a test payload
    pub async fn execute_payload(&self, payload_hash: Hash256) -> EngineResult<ExecutePayloadResult> {
        if let Some(actor) = &self.actor {
            let msg = ExecutePayloadMessage {
                payload_hash,
                correlation_id: Some(format!("test_{}", rand::random::<u64>())),
            };
            
            actor.send(msg).await
                .map_err(|e| super::super::EngineError::ActorError(format!("Mailbox error: {}", e)))?
                .map_err(Into::into)
        } else {
            Err(super::super::EngineError::ActorError("Actor not started".to_string()))
        }
    }
    
    /// Shutdown the actor gracefully
    pub async fn shutdown(&mut self, timeout: Duration) -> EngineResult<()> {
        if let Some(actor) = &self.actor {
            let msg = ShutdownEngineMessage {
                timeout,
                wait_for_pending: true,
            };
            
            actor.send(msg).await
                .map_err(|e| super::super::EngineError::ActorError(format!("Mailbox error: {}", e)))?
                .map_err(Into::into)?;
            
            self.actor = None;
            Ok(())
        } else {
            Ok(())
        }
    }
}

impl Drop for EngineActorTestHelper {
    fn drop(&mut self) {
        if let Some(actor) = &self.actor {
            // Send stop message to clean up
            actor.do_send(ShutdownEngineMessage {
                timeout: Duration::from_secs(5),
                wait_for_pending: false,
            });
        }
    }
}

/// Create a mock engine configuration for testing
pub fn create_mock_engine_config() -> EngineConfig {
    EngineConfig {
        jwt_secret: [0u8; 32],
        engine_url: "http://localhost:8551".to_string(),
        public_url: "http://localhost:8545".to_string(),
        client_type: super::super::config::ExecutionClientType::Mock, // Would need to add this variant
        performance: super::super::config::PerformanceConfig {
            max_payload_build_time: Duration::from_millis(100),
            max_payload_execution_time: Duration::from_millis(200),
            connection_pool_size: 1,
            request_timeout: Duration::from_secs(5),
            max_concurrent_requests: 10,
        },
        actor_integration: super::super::config::ActorIntegrationConfig::default(),
        health_check: super::super::config::HealthCheckConfig {
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            max_failures: 3,
            failure_threshold: Duration::from_secs(30),
        },
        timeouts: super::super::config::TimeoutConfig::test_defaults(),
    }
}

/// Create test payload attributes
pub fn create_test_payload_attributes() -> PayloadAttributes {
    PayloadAttributes::new(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        Hash256::random(),
        Address::zero(),
        None, // No withdrawals
    )
}

/// Create a test withdrawal
pub fn create_test_withdrawal(index: u64, amount: u64) -> Withdrawal {
    Withdrawal {
        index,
        validator_index: index,
        address: Address::random(),
        amount,
    }
}

/// Generate a random hash for testing
pub fn random_hash() -> Hash256 {
    Hash256::random()
}

/// Generate a random address for testing
pub fn random_address() -> Address {
    Address::random()
}

/// Create a test correlation ID
pub fn create_correlation_id() -> String {
    format!("test_correlation_{}", rand::random::<u64>())
}

/// Assert that an operation completes within a time limit
pub async fn assert_completes_within<F, Fut, T>(
    operation: F,
    timeout: Duration,
    description: &str,
) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    match tokio::time::timeout(timeout, operation()).await {
        Ok(result) => result,
        Err(_) => panic!("{} did not complete within {:?}", description, timeout),
    }
}

/// Wait for a condition to be true with polling
pub async fn wait_for_condition<F>(
    mut condition: F,
    timeout: Duration,
    poll_interval: Duration,
    description: &str,
) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        
        tokio::time::sleep(poll_interval).await;
    }
    
    eprintln!("Condition '{}' not met within {:?}", description, timeout);
    false
}

/// Measure memory usage during test execution
pub struct MemoryTracker {
    initial_memory: Option<u64>,
    peak_memory: Option<u64>,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self {
            initial_memory: Self::get_current_memory(),
            peak_memory: None,
        }
    }
    
    pub fn update_peak(&mut self) {
        if let Some(current) = Self::get_current_memory() {
            self.peak_memory = Some(
                self.peak_memory.map_or(current, |peak| peak.max(current))
            );
        }
    }
    
    pub fn get_memory_usage(&self) -> Option<(u64, u64)> {
        match (self.initial_memory, self.peak_memory) {
            (Some(initial), Some(peak)) => Some((initial, peak)),
            _ => None,
        }
    }
    
    #[cfg(target_os = "linux")]
    fn get_current_memory() -> Option<u64> {
        use std::fs;
        
        let status = fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse::<u64>().ok().map(|kb| kb * 1024);
                }
            }
        }
        None
    }
    
    #[cfg(not(target_os = "linux"))]
    fn get_current_memory() -> Option<u64> {
        // Memory tracking not implemented for non-Linux platforms
        None
    }
}

/// Test scenario builder for complex test cases
pub struct TestScenarioBuilder {
    steps: Vec<TestStep>,
    timeout: Duration,
    cleanup: bool,
}

pub struct TestStep {
    pub name: String,
    pub action: Box<dyn Fn(&mut EngineActorTestHelper) -> std::pin::Pin<Box<dyn std::future::Future<Output = EngineResult<()>> + Send>> + Send>,
    pub expected_duration: Option<Duration>,
}

impl TestScenarioBuilder {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            timeout: Duration::from_secs(60),
            cleanup: true,
        }
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    pub fn with_cleanup(mut self, cleanup: bool) -> Self {
        self.cleanup = cleanup;
        self
    }
    
    pub fn step<F, Fut>(mut self, name: &str, action: F) -> Self
    where
        F: Fn(&mut EngineActorTestHelper) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = EngineResult<()>> + Send + 'static,
    {
        self.steps.push(TestStep {
            name: name.to_string(),
            action: Box::new(move |helper| Box::pin(action(helper))),
            expected_duration: None,
        });
        self
    }
    
    pub async fn execute(self, helper: &mut EngineActorTestHelper) -> EngineResult<TestScenarioResult> {
        let start_time = std::time::Instant::now();
        let mut step_results = Vec::new();
        
        for (i, step) in self.steps.into_iter().enumerate() {
            let step_start = std::time::Instant::now();
            
            match tokio::time::timeout(self.timeout, (step.action)(helper)).await {
                Ok(Ok(())) => {
                    let step_duration = step_start.elapsed();
                    step_results.push(TestStepResult {
                        name: step.name,
                        success: true,
                        duration: step_duration,
                        error: None,
                    });
                },
                Ok(Err(e)) => {
                    let step_duration = step_start.elapsed();
                    step_results.push(TestStepResult {
                        name: step.name.clone(),
                        success: false,
                        duration: step_duration,
                        error: Some(format!("{}", e)),
                    });
                    
                    return Ok(TestScenarioResult {
                        total_duration: start_time.elapsed(),
                        steps: step_results,
                        success: false,
                        failed_step: Some(step.name),
                    });
                },
                Err(_) => {
                    step_results.push(TestStepResult {
                        name: step.name.clone(),
                        success: false,
                        duration: self.timeout,
                        error: Some("Timeout".to_string()),
                    });
                    
                    return Ok(TestScenarioResult {
                        total_duration: start_time.elapsed(),
                        steps: step_results,
                        success: false,
                        failed_step: Some(step.name),
                    });
                }
            }
        }
        
        Ok(TestScenarioResult {
            total_duration: start_time.elapsed(),
            steps: step_results,
            success: true,
            failed_step: None,
        })
    }
}

#[derive(Debug)]
pub struct TestScenarioResult {
    pub total_duration: Duration,
    pub steps: Vec<TestStepResult>,
    pub success: bool,
    pub failed_step: Option<String>,
}

#[derive(Debug)]
pub struct TestStepResult {
    pub name: String,
    pub success: bool,
    pub duration: Duration,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_helper_creation() {
        let helper = EngineActorTestHelper::new();
        assert!(helper.actor.is_none());
        assert!(helper.config.use_mock_client);
    }
    
    #[test]
    fn test_mock_config_creation() {
        let config = create_mock_engine_config();
        assert_eq!(config.jwt_secret, [0u8; 32]);
        assert_eq!(config.engine_url, "http://localhost:8551");
    }
    
    #[test]
    fn test_test_payload_attributes() {
        let attrs = create_test_payload_attributes();
        assert!(attrs.timestamp > 0);
        assert_eq!(attrs.suggested_fee_recipient, Address::zero());
    }
    
    #[test]
    fn test_memory_tracker() {
        let mut tracker = MemoryTracker::new();
        tracker.update_peak();
        
        // Memory tracking may not be available on all platforms
        // Just ensure it doesn't panic
    }
    
    #[test]
    fn test_scenario_builder() {
        let scenario = TestScenarioBuilder::new()
            .with_timeout(Duration::from_secs(30))
            .step("test_step", |_helper| async { Ok(()) });
        
        assert_eq!(scenario.steps.len(), 1);
        assert_eq!(scenario.timeout, Duration::from_secs(30));
    }
}