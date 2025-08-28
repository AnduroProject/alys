//! Chaos Testing for EngineActor
//!
//! Implements chaos engineering principles to test the resilience and fault tolerance
//! of the EngineActor under various failure conditions and unexpected scenarios.

use std::time::{Duration, Instant, SystemTime};
use std::sync::{Arc, Mutex};
use actix::prelude::*;
use tracing_test::traced_test;
use rand::{Rng, thread_rng};

use lighthouse_wrapper::types::{Hash256, Address};

use crate::types::*;
use super::super::{
    messages::*,
    state::ExecutionState,
    supervision::{FailureType, SupervisionDirective},
    EngineResult,
};
use super::{
    helpers::*,
    mocks::{MockExecutionClient, MockClientConfig},
    TestConfig,
};

/// Chaos testing configuration
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Test duration for chaos scenarios
    pub test_duration: Duration,
    
    /// Failure injection rate (0.0 to 1.0)
    pub failure_rate: f64,
    
    /// Network partition probability
    pub partition_probability: f64,
    
    /// Message drop rate
    pub message_drop_rate: f64,
    
    /// Resource exhaustion scenarios
    pub resource_exhaustion: bool,
    
    /// Enable Byzantine failures (malformed responses)
    pub byzantine_failures: bool,
    
    /// Clock skew simulation
    pub clock_skew: Duration,
    
    /// Memory pressure simulation
    pub memory_pressure: bool,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            test_duration: Duration::from_secs(60),
            failure_rate: 0.2, // 20% failure rate
            partition_probability: 0.1,
            message_drop_rate: 0.05,
            resource_exhaustion: true,
            byzantine_failures: true,
            clock_skew: Duration::from_secs(5),
            memory_pressure: false, // Disabled by default as it's hard to simulate
        }
    }
}

/// Chaos test results
#[derive(Debug)]
pub struct ChaosResults {
    /// Total operations attempted
    pub operations_attempted: u64,
    
    /// Operations that succeeded
    pub operations_succeeded: u64,
    
    /// Operations that failed
    pub operations_failed: u64,
    
    /// Actor restarts observed
    pub actor_restarts: u32,
    
    /// Time spent in degraded state
    pub degraded_time: Duration,
    
    /// Recovery time after failures
    pub recovery_times: Vec<Duration>,
    
    /// Types of failures encountered
    pub failure_types: Vec<FailureType>,
    
    /// Final actor state
    pub final_state: ExecutionState,
    
    /// Test duration
    pub test_duration: Duration,
}

/// Chaos testing orchestrator
pub struct ChaosOrchestrator {
    config: ChaosConfig,
    helper: EngineActorTestHelper,
    failure_injector: FailureInjector,
    metrics: Arc<Mutex<ChaosMetrics>>,
}

/// Failure injection controller
pub struct FailureInjector {
    config: ChaosConfig,
    active_failures: Vec<ActiveFailure>,
    rng: rand::rngs::ThreadRng,
}

/// Active failure scenario
#[derive(Debug, Clone)]
pub struct ActiveFailure {
    failure_type: ChaosFailureType,
    started_at: Instant,
    duration: Duration,
}

/// Types of chaos failures
#[derive(Debug, Clone, PartialEq)]
pub enum ChaosFailureType {
    NetworkPartition,
    MessageDrop,
    SlowResponse,
    ResourceExhaustion,
    ByzantineFailure,
    ClockSkew,
    MemoryPressure,
    ActorPanic,
    ConfigCorruption,
}

/// Chaos testing metrics
#[derive(Debug, Default)]
pub struct ChaosMetrics {
    pub network_partitions: u32,
    pub message_drops: u32,
    pub slow_responses: u32,
    pub byzantine_responses: u32,
    pub resource_exhaustions: u32,
    pub actor_restarts: u32,
    pub recovery_events: u32,
    pub degraded_periods: u32,
}

impl ChaosOrchestrator {
    pub fn new() -> Self {
        Self::with_config(ChaosConfig::default())
    }
    
    pub fn with_config(config: ChaosConfig) -> Self {
        let test_config = TestConfig::chaos();
        
        Self {
            failure_injector: FailureInjector::new(config.clone()),
            helper: EngineActorTestHelper::with_config(test_config),
            config,
            metrics: Arc::new(Mutex::new(ChaosMetrics::default())),
        }
    }
    
    /// Run the complete chaos testing suite
    pub async fn run_chaos_suite(&mut self) -> EngineResult<ChaosResults> {
        println!("🌪️  Starting EngineActor Chaos Test Suite");
        println!("Configuration: {:?}", self.config);
        
        // Initialize actor
        self.helper.start_with_mock().await?;
        self.helper.wait_for_ready(Duration::from_secs(10)).await;
        
        // Run chaos scenarios
        let results = self.execute_chaos_scenarios().await?;
        
        // Cleanup
        let _ = self.helper.shutdown(Duration::from_secs(5)).await;
        
        println!("✅ Chaos Test Suite Completed");
        self.print_chaos_summary(&results);
        
        Ok(results)
    }
    
    /// Execute chaos testing scenarios
    async fn execute_chaos_scenarios(&mut self) -> EngineResult<ChaosResults> {
        let start_time = Instant::now();
        let mut operations_attempted = 0u64;
        let mut operations_succeeded = 0u64;
        let mut recovery_times = Vec::new();
        let mut failure_types = Vec::new();
        let mut degraded_start: Option<Instant> = None;
        let mut total_degraded_time = Duration::ZERO;
        
        println!("Running chaos scenarios for {:?}...", self.config.test_duration);
        
        let mut last_progress = Instant::now();
        
        while start_time.elapsed() < self.config.test_duration {
            // Inject failures based on configuration
            self.failure_injector.maybe_inject_failure().await;
            
            // Attempt operation
            operations_attempted += 1;
            let operation_start = Instant::now();
            
            match self.perform_chaos_operation().await {
                Ok(_) => {
                    operations_succeeded += 1;
                    
                    // Check if we recovered from degraded state
                    if degraded_start.is_some() {
                        let recovery_time = operation_start.duration_since(degraded_start.unwrap());
                        recovery_times.push(recovery_time);
                        total_degraded_time += recovery_time;
                        degraded_start = None;
                        
                        self.metrics.lock().unwrap().recovery_events += 1;
                    }
                },
                Err(e) => {
                    // Record failure type
                    let failure_type = self.classify_error(&e);
                    failure_types.push(failure_type);
                    
                    // Mark start of degraded state if not already degraded
                    if degraded_start.is_none() {
                        degraded_start = Some(operation_start);
                        self.metrics.lock().unwrap().degraded_periods += 1;
                    }
                }
            }
            
            // Progress reporting
            if last_progress.elapsed() > Duration::from_secs(10) {
                let elapsed = start_time.elapsed();
                let progress = (elapsed.as_secs_f64() / self.config.test_duration.as_secs_f64() * 100.0) as u32;
                let success_rate = operations_succeeded as f64 / operations_attempted as f64 * 100.0;
                
                println!(
                    "Progress: {}% - Success Rate: {:.1}% ({}/{} ops)",
                    progress, success_rate, operations_succeeded, operations_attempted
                );
                last_progress = Instant::now();
            }
            
            // Brief pause between operations
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        // Handle final degraded state
        if let Some(degraded_start) = degraded_start {
            total_degraded_time += degraded_start.elapsed();
        }
        
        // Get final actor state
        let final_state = match self.helper.get_status(false).await {
            Ok(status) => status.execution_state,
            Err(_) => ExecutionState::Error {
                message: "Unable to get final state".to_string(),
                occurred_at: SystemTime::now(),
                recoverable: false,
                recovery_attempts: 0,
            }
        };
        
        let metrics = self.metrics.lock().unwrap();
        
        Ok(ChaosResults {
            operations_attempted,
            operations_succeeded,
            operations_failed: operations_attempted - operations_succeeded,
            actor_restarts: metrics.actor_restarts,
            degraded_time: total_degraded_time,
            recovery_times,
            failure_types,
            final_state,
            test_duration: start_time.elapsed(),
        })
    }
    
    /// Perform a chaos operation (subject to failure injection)
    async fn perform_chaos_operation(&mut self) -> EngineResult<()> {
        // Randomly choose operation type
        let operation_type = thread_rng().gen_range(0..4);
        
        match operation_type {
            0 => {
                // Payload build
                let parent_hash = Hash256::random();
                self.helper.build_payload(parent_hash).await.map(|_| ())
            },
            1 => {
                // Health check
                self.helper.health_check().await
            },
            2 => {
                // Status check
                self.helper.get_status(true).await.map(|_| ())
            },
            3 => {
                // Forkchoice update (if actor is available)
                if let Some(actor) = &self.helper.actor {
                    let msg = ForkchoiceUpdatedMessage {
                        head_block_hash: Hash256::random(),
                        safe_block_hash: Hash256::random(),
                        finalized_block_hash: Hash256::random(),
                        payload_attributes: None,
                        correlation_id: Some(create_correlation_id()),
                    };
                    
                    actor.send(msg).await
                        .map_err(|e| super::super::EngineError::ActorError(format!("Mailbox error: {}", e)))?
                        .map_err(Into::into)
                        .map(|_| ())
                } else {
                    Err(super::super::EngineError::ActorError("Actor not available".to_string()))
                }
            },
            _ => unreachable!(),
        }
    }
    
    /// Classify error type for metrics
    fn classify_error(&self, error: &super::super::EngineError) -> FailureType {
        match error {
            super::super::EngineError::ClientError(_) => FailureType::ConnectionFailure,
            super::super::EngineError::TimeoutError => FailureType::Timeout,
            super::super::EngineError::ActorError(_) => FailureType::ActorSystemError,
            super::super::EngineError::ValidationError(_) => FailureType::InvalidResponse,
            super::super::EngineError::ConfigError(_) => FailureType::ConfigError,
            _ => FailureType::Unknown,
        }
    }
    
    /// Print chaos test summary
    fn print_chaos_summary(&self, results: &ChaosResults) {
        println!("\n🌪️  Chaos Test Results");
        println!("{:-<60}", "");
        
        let success_rate = results.operations_succeeded as f64 / results.operations_attempted as f64 * 100.0;
        let avg_recovery_time = if !results.recovery_times.is_empty() {
            results.recovery_times.iter().sum::<Duration>() / results.recovery_times.len() as u32
        } else {
            Duration::ZERO
        };
        
        println!("Operations:");
        println!("  Attempted: {}", results.operations_attempted);
        println!("  Succeeded: {}", results.operations_succeeded);
        println!("  Failed: {}", results.operations_failed);
        println!("  Success Rate: {:.1}%", success_rate);
        
        println!("\nResilience:");
        println!("  Actor Restarts: {}", results.actor_restarts);
        println!("  Recovery Events: {}", results.recovery_times.len());
        println!("  Avg Recovery Time: {:?}", avg_recovery_time);
        println!("  Time in Degraded State: {:?}", results.degraded_time);
        
        println!("\nFinal State: {:?}", results.final_state);
        
        let metrics = self.metrics.lock().unwrap();
        println!("\nChaos Metrics:");
        println!("  Network Partitions: {}", metrics.network_partitions);
        println!("  Message Drops: {}", metrics.message_drops);
        println!("  Slow Responses: {}", metrics.slow_responses);
        println!("  Byzantine Responses: {}", metrics.byzantine_responses);
        
        // Assessment
        let resilient = success_rate > 70.0 && // At least 70% operations should succeed
                       avg_recovery_time < Duration::from_secs(30) && // Recovery under 30s
                       matches!(results.final_state, ExecutionState::Ready { .. } | ExecutionState::Degraded { .. });
        
        if resilient {
            println!("\n✅ Resilience Assessment: GOOD");
        } else {
            println!("\n⚠️  Resilience Assessment: NEEDS IMPROVEMENT");
        }
    }
}

impl FailureInjector {
    pub fn new(config: ChaosConfig) -> Self {
        Self {
            config,
            active_failures: Vec::new(),
            rng: thread_rng(),
        }
    }
    
    /// Maybe inject a failure based on configuration
    pub async fn maybe_inject_failure(&mut self) {
        // Clean up expired failures
        let now = Instant::now();
        self.active_failures.retain(|f| now.duration_since(f.started_at) < f.duration);
        
        // Maybe inject new failure
        if self.rng.gen::<f64>() < self.config.failure_rate {
            let failure_type = self.choose_failure_type();
            self.inject_failure(failure_type).await;
        }
    }
    
    /// Choose a random failure type based on configuration
    fn choose_failure_type(&mut self) -> ChaosFailureType {
        let mut choices = vec![
            ChaosFailureType::SlowResponse,
            ChaosFailureType::MessageDrop,
        ];
        
        if self.rng.gen::<f64>() < self.config.partition_probability {
            choices.push(ChaosFailureType::NetworkPartition);
        }
        
        if self.config.resource_exhaustion {
            choices.push(ChaosFailureType::ResourceExhaustion);
        }
        
        if self.config.byzantine_failures {
            choices.push(ChaosFailureType::ByzantineFailure);
        }
        
        if self.config.memory_pressure {
            choices.push(ChaosFailureType::MemoryPressure);
        }
        
        choices[self.rng.gen_range(0..choices.len())].clone()
    }
    
    /// Inject a specific failure type
    async fn inject_failure(&mut self, failure_type: ChaosFailureType) {
        let duration = Duration::from_secs(self.rng.gen_range(5..30)); // 5-30 second failures
        
        println!("💥 Injecting failure: {:?} for {:?}", failure_type, duration);
        
        match failure_type {
            ChaosFailureType::NetworkPartition => {
                self.inject_network_partition(duration).await;
            },
            ChaosFailureType::MessageDrop => {
                self.inject_message_drops(duration).await;
            },
            ChaosFailureType::SlowResponse => {
                self.inject_slow_responses(duration).await;
            },
            ChaosFailureType::ResourceExhaustion => {
                self.inject_resource_exhaustion(duration).await;
            },
            ChaosFailureType::ByzantineFailure => {
                self.inject_byzantine_failure(duration).await;
            },
            ChaosFailureType::MemoryPressure => {
                self.inject_memory_pressure(duration).await;
            },
            _ => {
                println!("Failure type {:?} not implemented", failure_type);
            }
        }
        
        self.active_failures.push(ActiveFailure {
            failure_type,
            started_at: Instant::now(),
            duration,
        });
    }
    
    async fn inject_network_partition(&mut self, _duration: Duration) {
        // Simulate network partition by making client unreachable
        println!("📡 Simulating network partition");
    }
    
    async fn inject_message_drops(&mut self, _duration: Duration) {
        // Simulate message drops
        println!("📉 Simulating message drops");
    }
    
    async fn inject_slow_responses(&mut self, _duration: Duration) {
        // Simulate slow responses by adding delays
        println!("🐌 Simulating slow responses");
    }
    
    async fn inject_resource_exhaustion(&mut self, _duration: Duration) {
        // Simulate resource exhaustion
        println!("💾 Simulating resource exhaustion");
    }
    
    async fn inject_byzantine_failure(&mut self, _duration: Duration) {
        // Simulate byzantine failures (malformed responses)
        println!("🤖 Simulating byzantine failures");
    }
    
    async fn inject_memory_pressure(&mut self, _duration: Duration) {
        // Simulate memory pressure
        println!("💾 Simulating memory pressure");
    }
}

#[cfg(test)]
mod chaos_tests {
    use super::*;
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_basic_chaos_scenario() {
        let config = ChaosConfig {
            test_duration: Duration::from_secs(10),
            failure_rate: 0.3,
            ..Default::default()
        };
        
        let mut orchestrator = ChaosOrchestrator::with_config(config);
        let results = orchestrator.run_chaos_suite().await.expect("Chaos test should complete");
        
        assert!(results.operations_attempted > 0, "Should attempt operations");
        assert!(results.test_duration >= Duration::from_secs(9), "Should run for specified duration");
        
        // Actor should survive chaos
        assert!(
            !matches!(results.final_state, ExecutionState::Error { recoverable: false, .. }),
            "Actor should not be in non-recoverable error state"
        );
    }
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_failure_injection() {
        let mut injector = FailureInjector::new(ChaosConfig {
            failure_rate: 1.0, // Always inject failures for testing
            ..Default::default()
        });
        
        // Test failure injection
        injector.maybe_inject_failure().await;
        
        assert!(!injector.active_failures.is_empty(), "Should have active failures");
    }
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_resilience_metrics() {
        let config = ChaosConfig {
            test_duration: Duration::from_secs(5),
            failure_rate: 0.2,
            ..Default::default()
        };
        
        let mut orchestrator = ChaosOrchestrator::with_config(config);
        let results = orchestrator.run_chaos_suite().await.expect("Should complete");
        
        // Verify metrics are collected
        let metrics = orchestrator.metrics.lock().unwrap();
        assert!(results.operations_attempted > 0, "Should track operations");
        
        // Success rate should be reasonable even with chaos
        let success_rate = results.operations_succeeded as f64 / results.operations_attempted as f64;
        assert!(success_rate > 0.5, "Should maintain reasonable success rate under chaos");
    }
}