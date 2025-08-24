//! Supervision tree testing scenarios for V2 actor system
//!
//! This module provides comprehensive testing for supervision hierarchies,
//! failure scenarios, restart policies, and cascading failure handling.

use crate::{
    error::{ActorError, ActorResult},
    metrics::{MetricsCollector, MetricsSnapshot},
    testing::{ActorTestHarness, TestEnvironment, TestUtil, MockGovernanceServer},
    Actor, Context, Handler, Message, ResponseFuture,
};
use actix::prelude::*;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Supervision strategies for testing
#[derive(Debug, Clone, PartialEq)]
pub enum SupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
    SimpleOneForOne,
}

/// Test actor for supervision scenarios
#[derive(Debug)]
pub struct TestActor {
    pub id: String,
    pub actor_type: String,
    pub fail_on_message: Option<String>,
    pub failure_count: Arc<AtomicU32>,
    pub restart_count: Arc<AtomicU32>,
    pub message_count: Arc<AtomicU32>,
    pub state: ActorState,
}

/// Test actor state
#[derive(Debug, Clone, PartialEq)]
pub enum ActorState {
    Initializing,
    Running,
    Failed,
    Restarting,
    Stopped,
}

/// Test messages for supervision scenarios
#[derive(Debug, Message)]
#[rtype(result = "ActorResult<String>")]
pub struct TestMessage {
    pub content: String,
    pub should_fail: bool,
    pub delay: Option<Duration>,
}

#[derive(Debug, Message)]
#[rtype(result = "ActorResult<ActorStats>")]
pub struct GetActorStats;

#[derive(Debug, Message)]
#[rtype(result = "ActorResult<()>")]
pub struct TriggerFailure {
    pub failure_type: FailureType,
}

#[derive(Debug, Message)]
#[rtype(result = "ActorResult<()>")]
pub struct SimulateRestart;

/// Types of failures for testing
#[derive(Debug, Clone)]
pub enum FailureType {
    /// Panic during message handling
    Panic,
    /// Timeout in processing
    Timeout,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Network failure
    NetworkFailure,
    /// Invalid state transition
    InvalidState,
    /// Custom error for testing
    Custom(String),
}

/// Statistics for test actors
#[derive(Debug, Clone)]
pub struct ActorStats {
    pub id: String,
    pub actor_type: String,
    pub state: ActorState,
    pub failure_count: u32,
    pub restart_count: u32,
    pub message_count: u32,
    pub uptime: Duration,
    pub last_failure: Option<String>,
}

impl TestActor {
    pub fn new(id: String, actor_type: String) -> Self {
        Self {
            id,
            actor_type,
            fail_on_message: None,
            failure_count: Arc::new(AtomicU32::new(0)),
            restart_count: Arc::new(AtomicU32::new(0)),
            message_count: Arc::new(AtomicU32::new(0)),
            state: ActorState::Initializing,
        }
    }

    pub fn with_failure_trigger(mut self, message: String) -> Self {
        self.fail_on_message = Some(message);
        self
    }

    fn simulate_failure(&self, failure_type: &FailureType) -> ActorError {
        match failure_type {
            FailureType::Panic => ActorError::SystemFailure {
                reason: "Simulated panic in actor".to_string(),
            },
            FailureType::Timeout => ActorError::Timeout {
                operation: "message_processing".to_string(),
                timeout: Duration::from_millis(5000),
            },
            FailureType::ResourceExhaustion => ActorError::ResourceExhausted {
                resource: "memory".to_string(),
                details: "Simulated OOM condition".to_string(),
            },
            FailureType::NetworkFailure => ActorError::NetworkError {
                reason: "Connection lost to peer".to_string(),
            },
            FailureType::InvalidState => ActorError::InvalidStateTransition {
                from: "Running".to_string(),
                to: "InvalidTarget".to_string(),
            },
            FailureType::Custom(msg) => ActorError::Custom {
                message: msg.clone(),
            },
        }
    }
}

impl Actor for TestActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Test actor {} started", self.id);
        self.state = ActorState::Running;
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Test actor {} stopped", self.id);
        self.state = ActorState::Stopped;
    }
}

impl Handler<TestMessage> for TestActor {
    type Result = ResponseFuture<ActorResult<String>>;

    fn handle(&mut self, msg: TestMessage, _ctx: &mut Self::Context) -> Self::Result {
        let id = self.id.clone();
        let message_count = self.message_count.clone();
        let failure_count = self.failure_count.clone();
        let fail_trigger = self.fail_on_message.clone();

        Box::pin(async move {
            message_count.fetch_add(1, Ordering::Relaxed);
            debug!("TestActor {} processing message: {}", id, msg.content);

            // Simulate processing delay if specified
            if let Some(delay) = msg.delay {
                tokio::time::sleep(delay).await;
            }

            // Check if this message should trigger a failure
            if msg.should_fail || fail_trigger.as_ref() == Some(&msg.content) {
                failure_count.fetch_add(1, Ordering::Relaxed);
                error!("TestActor {} failing on message: {}", id, msg.content);
                return Err(ActorError::MessageHandlingFailed {
                    message_type: "TestMessage".to_string(),
                    reason: format!("Simulated failure for message: {}", msg.content),
                });
            }

            Ok(format!("Processed: {} by {}", msg.content, id))
        })
    }
}

impl Handler<GetActorStats> for TestActor {
    type Result = ResponseFuture<ActorResult<ActorStats>>;

    fn handle(&mut self, _msg: GetActorStats, _ctx: &mut Self::Context) -> Self::Result {
        let stats = ActorStats {
            id: self.id.clone(),
            actor_type: self.actor_type.clone(),
            state: self.state.clone(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            restart_count: self.restart_count.load(Ordering::Relaxed),
            message_count: self.message_count.load(Ordering::Relaxed),
            uptime: Duration::from_secs(0), // Would track actual uptime in real implementation
            last_failure: None,
        };

        Box::pin(async move { Ok(stats) })
    }
}

impl Handler<TriggerFailure> for TestActor {
    type Result = ResponseFuture<ActorResult<()>>;

    fn handle(&mut self, msg: TriggerFailure, _ctx: &mut Self::Context) -> Self::Result {
        let error = self.simulate_failure(&msg.failure_type);
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        self.state = ActorState::Failed;
        
        error!("TestActor {} triggered failure: {:?}", self.id, msg.failure_type);
        Box::pin(async move { Err(error) })
    }
}

/// Supervision test harness for comprehensive supervision testing
pub struct SupervisionTestHarness {
    pub env: TestEnvironment,
    pub test_actors: HashMap<String, Addr<TestActor>>,
    pub supervisor_hierarchy: SupervisionHierarchy,
    pub failure_scenarios: Vec<FailureScenario>,
    pub test_results: Arc<Mutex<HashMap<String, TestResult>>>,
}

/// Supervision hierarchy for testing
#[derive(Debug)]
pub struct SupervisionHierarchy {
    pub root_supervisor: Option<Addr<TestSupervisor>>,
    pub supervisors: HashMap<String, SupervisorInfo>,
    pub actor_mappings: HashMap<String, String>, // actor_id -> supervisor_id
}

/// Supervisor information for testing
#[derive(Debug)]
pub struct SupervisorInfo {
    pub id: String,
    pub strategy: SupervisionStrategy,
    pub supervised_actors: Vec<String>,
    pub child_supervisors: Vec<String>,
    pub failure_count: Arc<AtomicU32>,
    pub restart_count: Arc<AtomicU32>,
}

/// Test supervisor for supervision scenarios
#[derive(Debug)]
pub struct TestSupervisor {
    pub id: String,
    pub strategy: SupervisionStrategy,
    pub supervised_actors: HashMap<String, ActorInfo>,
    pub failure_count: Arc<AtomicU32>,
    pub restart_count: Arc<AtomicU32>,
    pub escalation_count: Arc<AtomicU32>,
}

/// Actor information tracked by supervisor
#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub id: String,
    pub actor_type: String,
    pub start_time: Instant,
    pub failure_count: u32,
    pub restart_count: u32,
    pub state: ActorState,
    pub last_failure_time: Option<Instant>,
}

/// Failure scenario for testing
#[derive(Debug, Clone)]
pub struct FailureScenario {
    pub id: String,
    pub description: String,
    pub target_actors: Vec<String>,
    pub failure_types: Vec<FailureType>,
    pub expected_behavior: ExpectedBehavior,
    pub timeout: Duration,
}

/// Expected behavior after failure scenario
#[derive(Debug, Clone)]
pub struct ExpectedBehavior {
    pub should_restart: bool,
    pub should_escalate: bool,
    pub max_restart_attempts: u32,
    pub expected_final_state: ActorState,
    pub should_affect_siblings: bool,
}

/// Test result tracking
#[derive(Debug, Clone)]
pub struct TestResult {
    pub scenario_id: String,
    pub success: bool,
    pub execution_time: Duration,
    pub failures_detected: u32,
    pub restarts_observed: u32,
    pub escalations_observed: u32,
    pub final_actor_states: HashMap<String, ActorState>,
    pub error_messages: Vec<String>,
}

impl SupervisionTestHarness {
    pub fn new() -> Self {
        Self {
            env: TestEnvironment::new(),
            test_actors: HashMap::new(),
            supervisor_hierarchy: SupervisionHierarchy {
                root_supervisor: None,
                supervisors: HashMap::new(),
                actor_mappings: HashMap::new(),
            },
            failure_scenarios: Vec::new(),
            test_results: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a test actor
    pub async fn create_test_actor(
        &mut self,
        actor_type: String,
        supervisor_id: Option<String>,
    ) -> ActorResult<String> {
        let actor_id = format!("{}_{}", actor_type, Uuid::new_v4());
        let actor = TestActor::new(actor_id.clone(), actor_type);
        let addr = actor.start();

        self.test_actors.insert(actor_id.clone(), addr);

        if let Some(sup_id) = supervisor_id {
            self.supervisor_hierarchy.actor_mappings.insert(actor_id.clone(), sup_id);
        }

        info!("Created test actor: {}", actor_id);
        Ok(actor_id)
    }

    /// Create a test supervisor with strategy
    pub async fn create_test_supervisor(
        &mut self,
        strategy: SupervisionStrategy,
        parent_supervisor_id: Option<String>,
    ) -> ActorResult<String> {
        let supervisor_id = format!("supervisor_{}", Uuid::new_v4());
        
        let supervisor = TestSupervisor {
            id: supervisor_id.clone(),
            strategy: strategy.clone(),
            supervised_actors: HashMap::new(),
            failure_count: Arc::new(AtomicU32::new(0)),
            restart_count: Arc::new(AtomicU32::new(0)),
            escalation_count: Arc::new(AtomicU32::new(0)),
        };

        let addr = supervisor.start();
        
        let supervisor_info = SupervisorInfo {
            id: supervisor_id.clone(),
            strategy,
            supervised_actors: Vec::new(),
            child_supervisors: Vec::new(),
            failure_count: Arc::new(AtomicU32::new(0)),
            restart_count: Arc::new(AtomicU32::new(0)),
        };

        self.supervisor_hierarchy.supervisors.insert(supervisor_id.clone(), supervisor_info);

        if parent_supervisor_id.is_none() {
            self.supervisor_hierarchy.root_supervisor = Some(addr);
        }

        info!("Created test supervisor: {} with strategy: {:?}", supervisor_id, strategy);
        Ok(supervisor_id)
    }

    /// Add a failure scenario to test
    pub fn add_failure_scenario(&mut self, scenario: FailureScenario) {
        info!("Added failure scenario: {} - {}", scenario.id, scenario.description);
        self.failure_scenarios.push(scenario);
    }

    /// Execute all failure scenarios
    pub async fn execute_failure_scenarios(&mut self) -> ActorResult<Vec<TestResult>> {
        let mut results = Vec::new();

        for scenario in &self.failure_scenarios.clone() {
            info!("Executing failure scenario: {}", scenario.description);
            let result = self.execute_scenario(scenario).await?;
            results.push(result.clone());

            // Store result for later analysis
            let mut test_results = self.test_results.lock().unwrap();
            test_results.insert(scenario.id.clone(), result);
        }

        Ok(results)
    }

    /// Execute a single failure scenario
    async fn execute_scenario(&mut self, scenario: &FailureScenario) -> ActorResult<TestResult> {
        let start_time = Instant::now();
        let mut result = TestResult {
            scenario_id: scenario.id.clone(),
            success: false,
            execution_time: Duration::default(),
            failures_detected: 0,
            restarts_observed: 0,
            escalations_observed: 0,
            final_actor_states: HashMap::new(),
            error_messages: Vec::new(),
        };

        // Execute failure triggers for target actors
        for actor_id in &scenario.target_actors {
            if let Some(actor_addr) = self.test_actors.get(actor_id) {
                for failure_type in &scenario.failure_types {
                    let trigger_msg = TriggerFailure {
                        failure_type: failure_type.clone(),
                    };

                    match actor_addr.send(trigger_msg).await {
                        Ok(Err(error)) => {
                            result.failures_detected += 1;
                            result.error_messages.push(error.to_string());
                            debug!("Successfully triggered failure in {}: {}", actor_id, error);
                        }
                        Ok(Ok(())) => {
                            warn!("Expected failure but actor succeeded: {}", actor_id);
                        }
                        Err(mailbox_error) => {
                            result.error_messages.push(format!("Mailbox error: {}", mailbox_error));
                        }
                    }
                }
            } else {
                result.error_messages.push(format!("Actor not found: {}", actor_id));
            }
        }

        // Wait for supervision system to respond
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check final states
        for actor_id in &scenario.target_actors {
            if let Some(actor_addr) = self.test_actors.get(actor_id) {
                match actor_addr.send(GetActorStats).await {
                    Ok(Ok(stats)) => {
                        result.final_actor_states.insert(actor_id.clone(), stats.state.clone());
                        result.restarts_observed += stats.restart_count;

                        // Validate expected behavior
                        let behavior_valid = self.validate_expected_behavior(
                            &stats,
                            &scenario.expected_behavior,
                        );

                        if !behavior_valid {
                            result.error_messages.push(
                                format!("Actor {} did not behave as expected", actor_id)
                            );
                        }
                    }
                    Ok(Err(error)) => {
                        result.error_messages.push(format!("Failed to get stats: {}", error));
                    }
                    Err(mailbox_error) => {
                        result.error_messages.push(format!("Mailbox error: {}", mailbox_error));
                    }
                }
            }
        }

        result.execution_time = start_time.elapsed();
        result.success = result.error_messages.is_empty();

        info!(
            "Scenario {} completed: success={}, failures={}, restarts={}",
            scenario.id, result.success, result.failures_detected, result.restarts_observed
        );

        Ok(result)
    }

    /// Validate that actor behavior matches expectations
    fn validate_expected_behavior(
        &self,
        stats: &ActorStats,
        expected: &ExpectedBehavior,
    ) -> bool {
        // Check if restart behavior matches expectations
        if expected.should_restart {
            if stats.restart_count == 0 {
                warn!("Expected restart but none occurred for actor {}", stats.id);
                return false;
            }
            if stats.restart_count > expected.max_restart_attempts {
                warn!("Too many restarts for actor {}: {} > {}", 
                     stats.id, stats.restart_count, expected.max_restart_attempts);
                return false;
            }
        } else if stats.restart_count > 0 {
            warn!("Unexpected restart for actor {}: {}", stats.id, stats.restart_count);
            return false;
        }

        // Check final state
        if stats.state != expected.expected_final_state {
            warn!("Unexpected final state for actor {}: {:?} != {:?}", 
                 stats.id, stats.state, expected.expected_final_state);
            return false;
        }

        true
    }

    /// Create comprehensive test scenarios
    pub fn create_comprehensive_test_scenarios(&mut self) {
        // Scenario 1: Single actor failure with restart
        let scenario1 = FailureScenario {
            id: "single_actor_restart".to_string(),
            description: "Single actor fails and should restart".to_string(),
            target_actors: vec!["test_actor_1".to_string()],
            failure_types: vec![FailureType::Custom("test_failure".to_string())],
            expected_behavior: ExpectedBehavior {
                should_restart: true,
                should_escalate: false,
                max_restart_attempts: 3,
                expected_final_state: ActorState::Running,
                should_affect_siblings: false,
            },
            timeout: Duration::from_secs(10),
        };
        self.add_failure_scenario(scenario1);

        // Scenario 2: Cascading failure (OneForAll strategy)
        let scenario2 = FailureScenario {
            id: "cascading_failure".to_string(),
            description: "One actor fails, all siblings should restart (OneForAll)".to_string(),
            target_actors: vec!["test_actor_2".to_string()],
            failure_types: vec![FailureType::Panic],
            expected_behavior: ExpectedBehavior {
                should_restart: true,
                should_escalate: false,
                max_restart_attempts: 1,
                expected_final_state: ActorState::Running,
                should_affect_siblings: true,
            },
            timeout: Duration::from_secs(15),
        };
        self.add_failure_scenario(scenario2);

        // Scenario 3: Resource exhaustion escalation
        let scenario3 = FailureScenario {
            id: "resource_exhaustion_escalation".to_string(),
            description: "Resource exhaustion should escalate to supervisor".to_string(),
            target_actors: vec!["test_actor_3".to_string()],
            failure_types: vec![FailureType::ResourceExhaustion],
            expected_behavior: ExpectedBehavior {
                should_restart: false,
                should_escalate: true,
                max_restart_attempts: 0,
                expected_final_state: ActorState::Failed,
                should_affect_siblings: false,
            },
            timeout: Duration::from_secs(5),
        };
        self.add_failure_scenario(scenario3);

        // Scenario 4: Network failure with retry
        let scenario4 = FailureScenario {
            id: "network_failure_retry".to_string(),
            description: "Network failure should trigger retry behavior".to_string(),
            target_actors: vec!["test_actor_4".to_string()],
            failure_types: vec![FailureType::NetworkFailure],
            expected_behavior: ExpectedBehavior {
                should_restart: true,
                should_escalate: false,
                max_restart_attempts: 5,
                expected_final_state: ActorState::Running,
                should_affect_siblings: false,
            },
            timeout: Duration::from_secs(20),
        };
        self.add_failure_scenario(scenario4);
    }

    /// Get comprehensive test report
    pub fn generate_test_report(&self) -> SupervisionTestReport {
        let test_results = self.test_results.lock().unwrap();
        let total_scenarios = test_results.len();
        let successful_scenarios = test_results.values().filter(|r| r.success).count();
        let total_failures = test_results.values().map(|r| r.failures_detected).sum();
        let total_restarts = test_results.values().map(|r| r.restarts_observed).sum();
        let total_execution_time: Duration = test_results.values().map(|r| r.execution_time).sum();

        SupervisionTestReport {
            total_scenarios,
            successful_scenarios,
            failed_scenarios: total_scenarios - successful_scenarios,
            total_failures_triggered: total_failures,
            total_restarts_observed: total_restarts,
            total_execution_time,
            scenario_results: test_results.clone(),
            recommendations: self.generate_recommendations(&test_results),
        }
    }

    /// Generate recommendations based on test results
    fn generate_recommendations(
        &self,
        results: &HashMap<String, TestResult>,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        let failure_rate = if results.is_empty() {
            0.0
        } else {
            let failed_count = results.values().filter(|r| !r.success).count();
            failed_count as f64 / results.len() as f64
        };

        if failure_rate > 0.2 {
            recommendations.push(
                "High failure rate detected. Consider reviewing supervision strategies.".to_string(),
            );
        }

        let total_restarts: u32 = results.values().map(|r| r.restarts_observed).sum();
        let avg_restarts = if results.is_empty() {
            0.0
        } else {
            total_restarts as f64 / results.len() as f64
        };

        if avg_restarts > 3.0 {
            recommendations.push(
                "High restart frequency. Consider implementing circuit breaker patterns.".to_string(),
            );
        }

        if results.values().any(|r| r.execution_time > Duration::from_secs(30)) {
            recommendations.push(
                "Long execution times detected. Review timeout configurations.".to_string(),
            );
        }

        if recommendations.is_empty() {
            recommendations.push("Supervision system performing within expected parameters.".to_string());
        }

        recommendations
    }

    /// Clean up test resources
    pub async fn cleanup(&mut self) -> ActorResult<()> {
        info!("Cleaning up supervision test harness");

        // Stop all test actors
        for (id, _addr) in &self.test_actors {
            debug!("Stopping test actor: {}", id);
        }
        self.test_actors.clear();

        // Clear supervision hierarchy
        self.supervisor_hierarchy.root_supervisor = None;
        self.supervisor_hierarchy.supervisors.clear();
        self.supervisor_hierarchy.actor_mappings.clear();

        // Clear scenarios and results
        self.failure_scenarios.clear();
        self.test_results.lock().unwrap().clear();

        info!("Supervision test harness cleanup completed");
        Ok(())
    }
}

impl TestSupervisor {
    pub fn new(id: String, strategy: SupervisionStrategy) -> Self {
        Self {
            id,
            strategy,
            supervised_actors: HashMap::new(),
            failure_count: Arc::new(AtomicU32::new(0)),
            restart_count: Arc::new(AtomicU32::new(0)),
            escalation_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl Actor for TestSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Test supervisor {} started with strategy: {:?}", self.id, self.strategy);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Test supervisor {} stopped", self.id);
    }
}

/// Comprehensive supervision test report
#[derive(Debug, Clone)]
pub struct SupervisionTestReport {
    pub total_scenarios: usize,
    pub successful_scenarios: usize,
    pub failed_scenarios: usize,
    pub total_failures_triggered: u32,
    pub total_restarts_observed: u32,
    pub total_execution_time: Duration,
    pub scenario_results: HashMap<String, TestResult>,
    pub recommendations: Vec<String>,
}

impl SupervisionTestReport {
    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_scenarios == 0 {
            0.0
        } else {
            (self.successful_scenarios as f64 / self.total_scenarios as f64) * 100.0
        }
    }

    /// Print formatted report
    pub fn print_report(&self) {
        println!("\n=== Supervision Tree Test Report ===");
        println!("Total Scenarios: {}", self.total_scenarios);
        println!("Successful: {}", self.successful_scenarios);
        println!("Failed: {}", self.failed_scenarios);
        println!("Success Rate: {:.2}%", self.success_rate());
        println!("Total Failures Triggered: {}", self.total_failures_triggered);
        println!("Total Restarts Observed: {}", self.total_restarts_observed);
        println!("Total Execution Time: {:?}", self.total_execution_time);
        
        println!("\n=== Recommendations ===");
        for (i, rec) in self.recommendations.iter().enumerate() {
            println!("{}. {}", i + 1, rec);
        }
        
        if self.failed_scenarios > 0 {
            println!("\n=== Failed Scenarios ===");
            for (id, result) in &self.scenario_results {
                if !result.success {
                    println!("- {}: {:?}", id, result.error_messages);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_supervision_test_harness_creation() {
        let harness = SupervisionTestHarness::new();
        assert!(harness.test_actors.is_empty());
        assert!(harness.supervisor_hierarchy.supervisors.is_empty());
    }

    #[tokio::test]
    async fn test_actor_creation() {
        let mut harness = SupervisionTestHarness::new();
        let actor_id = harness
            .create_test_actor("StreamActor".to_string(), None)
            .await
            .unwrap();

        assert!(harness.test_actors.contains_key(&actor_id));
        assert!(actor_id.starts_with("StreamActor"));
    }

    #[tokio::test]
    async fn test_supervisor_creation() {
        let mut harness = SupervisionTestHarness::new();
        let supervisor_id = harness
            .create_test_supervisor(SupervisionStrategy::OneForOne, None)
            .await
            .unwrap();

        assert!(harness.supervisor_hierarchy.supervisors.contains_key(&supervisor_id));
    }

    #[tokio::test]
    async fn test_failure_scenario_execution() {
        let mut harness = SupervisionTestHarness::new();
        
        // Create test actor
        let actor_id = harness
            .create_test_actor("TestActor".to_string(), None)
            .await
            .unwrap();

        // Create failure scenario
        let scenario = FailureScenario {
            id: "test_scenario".to_string(),
            description: "Test failure scenario".to_string(),
            target_actors: vec![actor_id],
            failure_types: vec![FailureType::Custom("test".to_string())],
            expected_behavior: ExpectedBehavior {
                should_restart: false,
                should_escalate: false,
                max_restart_attempts: 0,
                expected_final_state: ActorState::Failed,
                should_affect_siblings: false,
            },
            timeout: Duration::from_secs(5),
        };

        harness.add_failure_scenario(scenario);
        let results = harness.execute_failure_scenarios().await.unwrap();
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].scenario_id, "test_scenario");
    }

    #[tokio::test]
    async fn test_actor_stats() {
        let actor = TestActor::new("test_actor".to_string(), "TestActor".to_string());
        let addr = actor.start();

        let stats = addr.send(GetActorStats).await.unwrap().unwrap();
        assert_eq!(stats.id, "test_actor");
        assert_eq!(stats.actor_type, "TestActor");
        assert_eq!(stats.message_count, 0);
    }

    #[tokio::test]
    async fn test_comprehensive_scenarios() {
        let mut harness = SupervisionTestHarness::new();
        harness.create_comprehensive_test_scenarios();
        
        assert_eq!(harness.failure_scenarios.len(), 4);
        assert!(harness.failure_scenarios.iter().any(|s| s.id == "single_actor_restart"));
        assert!(harness.failure_scenarios.iter().any(|s| s.id == "cascading_failure"));
    }

    #[tokio::test]
    async fn test_report_generation() {
        let harness = SupervisionTestHarness::new();
        let report = harness.generate_test_report();
        
        assert_eq!(report.total_scenarios, 0);
        assert_eq!(report.success_rate(), 0.0);
        assert!(!report.recommendations.is_empty());
    }
}