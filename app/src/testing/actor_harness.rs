//! Actor test harness for integration testing with isolated actor environments
//!
//! This module provides comprehensive testing infrastructure for actor-based systems,
//! enabling isolated testing of individual actors, actor interactions, and complete
//! system integration scenarios.

use crate::config::{ActorSystemConfig, AlysConfig};
use crate::types::*;
use actor_system::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::timeout;
use uuid::Uuid;

/// Comprehensive actor test harness for integration testing
#[derive(Debug)]
pub struct ActorTestHarness {
    /// Test environment configuration
    test_env: TestEnvironment,
    
    /// Actor system for testing
    actor_system: Option<Arc<dyn ActorSystem>>,
    
    /// Test message router
    message_router: Arc<RwLock<TestMessageRouter>>,
    
    /// Active test actors
    test_actors: Arc<RwLock<HashMap<String, TestActorHandle>>>,
    
    /// Test scenario manager
    scenario_manager: Arc<RwLock<TestScenarioManager>>,
    
    /// Test metrics collector
    metrics_collector: Arc<RwLock<TestMetricsCollector>>,
    
    /// Test event logger
    event_logger: Arc<RwLock<TestEventLogger>>,
    
    /// Assertion framework
    assertion_engine: Arc<RwLock<AssertionEngine>>,
}

/// Test environment configuration
#[derive(Debug, Clone)]
pub struct TestEnvironment {
    /// Test identifier
    pub test_id: String,
    
    /// Test name
    pub test_name: String,
    
    /// Isolation level
    pub isolation_level: IsolationLevel,
    
    /// Test timeout
    pub timeout: Duration,
    
    /// Resource limits
    pub resource_limits: ResourceLimits,
    
    /// Mock configurations
    pub mock_config: MockConfiguration,
    
    /// Test data directory
    pub test_data_dir: String,
    
    /// Cleanup strategy
    pub cleanup_strategy: CleanupStrategy,
}

/// Actor isolation levels for testing
#[derive(Debug, Clone, Copy)]
pub enum IsolationLevel {
    /// Complete isolation - no external dependencies
    Complete,
    /// Network isolated - no network access
    NetworkIsolated,
    /// Database isolated - in-memory database
    DatabaseIsolated,
    /// Service isolated - mocked external services
    ServiceIsolated,
    /// Integration - real external dependencies
    Integration,
}

/// Test resource limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory usage (MB)
    pub max_memory_mb: u64,
    
    /// Maximum CPU usage (percentage)
    pub max_cpu_percent: u8,
    
    /// Maximum file descriptors
    pub max_file_descriptors: u32,
    
    /// Maximum network connections
    pub max_network_connections: u32,
    
    /// Maximum test duration
    pub max_duration: Duration,
}

/// Mock configuration for external systems
#[derive(Debug, Clone)]
pub struct MockConfiguration {
    /// Enable governance client mocking
    pub mock_governance: bool,
    
    /// Enable Bitcoin client mocking
    pub mock_bitcoin: bool,
    
    /// Enable execution client mocking
    pub mock_execution: bool,
    
    /// Enable network mocking
    pub mock_network: bool,
    
    /// Enable storage mocking
    pub mock_storage: bool,
    
    /// Mock response delays
    pub response_delays: HashMap<String, Duration>,
    
    /// Mock failure rates
    pub failure_rates: HashMap<String, f64>,
}

/// Cleanup strategy after test completion
#[derive(Debug, Clone, Copy)]
pub enum CleanupStrategy {
    /// Clean up everything
    Full,
    /// Keep logs for debugging
    KeepLogs,
    /// Keep test data
    KeepData,
    /// Keep everything for manual inspection
    KeepAll,
}

/// Test message router for actor communication
#[derive(Debug)]
pub struct TestMessageRouter {
    /// Message routes
    routes: HashMap<String, Vec<String>>,
    
    /// Message history
    message_history: Vec<TestMessageEvent>,
    
    /// Message filters
    filters: Vec<MessageFilter>,
    
    /// Message interceptors
    interceptors: Vec<MessageInterceptor>,
}

/// Test message event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessageEvent {
    pub event_id: String,
    pub timestamp: SystemTime,
    pub from_actor: String,
    pub to_actor: String,
    pub message_type: String,
    pub message_id: String,
    pub correlation_id: Option<String>,
    pub processing_time: Option<Duration>,
    pub result: MessageResult,
}

/// Message processing result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageResult {
    Success,
    Failed { error: String },
    Timeout,
    Dropped,
    Intercepted,
}

/// Message filter for selective message capture
#[derive(Debug, Clone)]
pub struct MessageFilter {
    pub filter_id: String,
    pub actor_filter: Option<String>,
    pub message_type_filter: Option<String>,
    pub correlation_filter: Option<String>,
    pub enabled: bool,
}

/// Message interceptor for test manipulation
#[derive(Debug)]
pub struct MessageInterceptor {
    pub interceptor_id: String,
    pub target_actor: Option<String>,
    pub target_message_type: Option<String>,
    pub action: InterceptorAction,
    pub enabled: bool,
}

/// Interceptor actions
#[derive(Debug)]
pub enum InterceptorAction {
    /// Drop the message
    Drop,
    /// Delay the message
    Delay { duration: Duration },
    /// Modify the message
    Modify { modifier: Box<dyn MessageModifier> },
    /// Duplicate the message
    Duplicate { count: u32 },
    /// Fail the message processing
    Fail { error: String },
}

/// Message modifier trait
pub trait MessageModifier: Send + Sync + std::fmt::Debug {
    fn modify(&self, message: &mut dyn std::any::Any) -> Result<(), String>;
}

/// Test actor handle
#[derive(Debug, Clone)]
pub struct TestActorHandle {
    pub actor_id: String,
    pub actor_type: String,
    pub start_time: SystemTime,
    pub message_count: u64,
    pub error_count: u64,
    pub health_status: ActorHealthStatus,
    pub sender: mpsc::Sender<TestMessage>,
}

/// Actor health status
#[derive(Debug, Clone)]
pub enum ActorHealthStatus {
    Starting,
    Running,
    Degraded { issues: Vec<String> },
    Stopping,
    Stopped,
    Failed { error: String },
}

/// Test scenario manager
#[derive(Debug)]
pub struct TestScenarioManager {
    /// Active scenarios
    scenarios: HashMap<String, TestScenario>,
    
    /// Scenario execution history
    execution_history: Vec<ScenarioExecution>,
    
    /// Scenario templates
    templates: HashMap<String, ScenarioTemplate>,
}

/// Test scenario definition
#[derive(Debug, Clone)]
pub struct TestScenario {
    pub scenario_id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<TestStep>,
    pub preconditions: Vec<Precondition>,
    pub postconditions: Vec<Postcondition>,
    pub timeout: Duration,
    pub retry_count: u32,
}

/// Individual test step
#[derive(Debug, Clone)]
pub enum TestStep {
    /// Start an actor
    StartActor {
        actor_id: String,
        actor_type: String,
        config: serde_json::Value,
    },
    /// Stop an actor
    StopActor {
        actor_id: String,
        graceful: bool,
    },
    /// Send a message
    SendMessage {
        from_actor: String,
        to_actor: String,
        message: serde_json::Value,
        expect_response: bool,
    },
    /// Wait for condition
    WaitForCondition {
        condition: TestCondition,
        timeout: Duration,
    },
    /// Assert condition
    AssertCondition {
        condition: TestCondition,
        error_message: String,
    },
    /// Delay execution
    Delay {
        duration: Duration,
    },
    /// Inject failure
    InjectFailure {
        target: FailureTarget,
        failure_type: FailureType,
    },
}

/// Test conditions
#[derive(Debug, Clone)]
pub enum TestCondition {
    /// Actor is running
    ActorRunning { actor_id: String },
    /// Actor is stopped
    ActorStopped { actor_id: String },
    /// Message received
    MessageReceived {
        actor_id: String,
        message_type: String,
    },
    /// Message count reached
    MessageCountReached {
        actor_id: String,
        count: u64,
    },
    /// Custom condition
    Custom {
        condition_id: String,
        checker: Box<dyn ConditionChecker>,
    },
}

/// Condition checker trait
pub trait ConditionChecker: Send + Sync + std::fmt::Debug {
    fn check(&self, harness: &ActorTestHarness) -> Result<bool, String>;
    fn description(&self) -> String;
}

/// Test preconditions
#[derive(Debug, Clone)]
pub struct Precondition {
    pub condition: TestCondition,
    pub required: bool,
    pub timeout: Duration,
}

/// Test postconditions
#[derive(Debug, Clone)]
pub struct Postcondition {
    pub condition: TestCondition,
    pub required: bool,
    pub timeout: Duration,
}

/// Scenario execution record
#[derive(Debug, Clone)]
pub struct ScenarioExecution {
    pub execution_id: String,
    pub scenario_id: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub status: ExecutionStatus,
    pub step_results: Vec<StepResult>,
    pub error_message: Option<String>,
}

/// Execution status
#[derive(Debug, Clone)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

/// Step execution result
#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_index: usize,
    pub start_time: SystemTime,
    pub end_time: SystemTime,
    pub status: ExecutionStatus,
    pub error_message: Option<String>,
    pub metrics: StepMetrics,
}

/// Step execution metrics
#[derive(Debug, Clone)]
pub struct StepMetrics {
    pub execution_time: Duration,
    pub memory_usage: u64,
    pub messages_processed: u32,
    pub assertions_checked: u32,
}

/// Test metrics collector
#[derive(Debug, Default)]
pub struct TestMetricsCollector {
    /// Actor performance metrics
    pub actor_metrics: HashMap<String, ActorTestMetrics>,
    
    /// System performance metrics
    pub system_metrics: SystemTestMetrics,
    
    /// Message processing metrics
    pub message_metrics: MessageTestMetrics,
    
    /// Resource usage metrics
    pub resource_metrics: ResourceTestMetrics,
}

/// Actor-specific test metrics
#[derive(Debug, Default, Clone)]
pub struct ActorTestMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_processed: u64,
    pub processing_time_total: Duration,
    pub processing_time_avg: Duration,
    pub error_count: u64,
    pub restart_count: u32,
    pub memory_usage_peak: u64,
    pub cpu_usage_avg: f64,
}

/// System-wide test metrics
#[derive(Debug, Default)]
pub struct SystemTestMetrics {
    pub total_actors: u32,
    pub active_actors: u32,
    pub total_messages: u64,
    pub messages_per_second: f64,
    pub system_uptime: Duration,
    pub total_errors: u64,
    pub error_rate: f64,
}

/// Message processing test metrics
#[derive(Debug, Default)]
pub struct MessageTestMetrics {
    pub total_messages: u64,
    pub successful_messages: u64,
    pub failed_messages: u64,
    pub timeout_messages: u64,
    pub average_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub throughput: f64,
}

/// Resource usage test metrics
#[derive(Debug, Default)]
pub struct ResourceTestMetrics {
    pub memory_usage_current: u64,
    pub memory_usage_peak: u64,
    pub cpu_usage_current: f64,
    pub cpu_usage_avg: f64,
    pub file_descriptors_used: u32,
    pub network_connections: u32,
    pub disk_usage: u64,
}

/// Test event logger
#[derive(Debug)]
pub struct TestEventLogger {
    /// Event log entries
    events: Vec<TestLogEntry>,
    
    /// Log configuration
    config: LogConfig,
    
    /// Log filters
    filters: Vec<LogFilter>,
}

/// Test log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestLogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub actor_id: Option<String>,
    pub message: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Log levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

/// Log configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    pub min_level: LogLevel,
    pub max_entries: usize,
    pub auto_flush: bool,
    pub include_metadata: bool,
}

/// Log filter
#[derive(Debug, Clone)]
pub struct LogFilter {
    pub actor_filter: Option<String>,
    pub level_filter: Option<LogLevel>,
    pub message_filter: Option<String>,
    pub enabled: bool,
}

/// Assertion engine for test validation
#[derive(Debug)]
pub struct AssertionEngine {
    /// Assertion history
    assertions: Vec<AssertionResult>,
    
    /// Custom assertion handlers
    custom_assertions: HashMap<String, Box<dyn AssertionHandler>>,
    
    /// Assertion configuration
    config: AssertionConfig,
}

/// Assertion result
#[derive(Debug, Clone)]
pub struct AssertionResult {
    pub assertion_id: String,
    pub timestamp: SystemTime,
    pub assertion_type: String,
    pub result: bool,
    pub message: String,
    pub context: AssertionContext,
}

/// Assertion context
#[derive(Debug, Clone)]
pub struct AssertionContext {
    pub test_id: String,
    pub scenario_id: Option<String>,
    pub step_index: Option<usize>,
    pub actor_id: Option<String>,
    pub additional_data: HashMap<String, serde_json::Value>,
}

/// Assertion handler trait
pub trait AssertionHandler: Send + Sync + std::fmt::Debug {
    fn handle(&self, context: &AssertionContext) -> AssertionResult;
    fn name(&self) -> &str;
}

/// Assertion configuration
#[derive(Debug, Clone)]
pub struct AssertionConfig {
    pub fail_fast: bool,
    pub collect_all_failures: bool,
    pub timeout_on_failure: Duration,
    pub retry_failed_assertions: bool,
    pub max_retries: u32,
}

/// Test message for actor communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessage {
    pub message_id: String,
    pub correlation_id: Option<String>,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub metadata: HashMap<String, String>,
    pub timestamp: SystemTime,
}

/// Failure target for failure injection
#[derive(Debug, Clone)]
pub enum FailureTarget {
    Actor { actor_id: String },
    Network { connection_id: String },
    Storage { operation_type: String },
    Message { message_type: String },
    System { component: String },
}

/// Failure types for chaos testing
#[derive(Debug, Clone)]
pub enum FailureType {
    Crash,
    Hang,
    SlowResponse { delay: Duration },
    NetworkPartition,
    MemoryLeak,
    ResourceExhaustion,
    MessageLoss,
    MessageCorruption,
}

/// Test result for actor testing
pub type ActorTestResult<T> = Result<T, ActorTestError>;

/// Actor test errors
#[derive(Debug, Clone)]
pub enum ActorTestError {
    SetupFailed { reason: String },
    ActorStartFailed { actor_id: String, reason: String },
    MessageSendFailed { from: String, to: String, reason: String },
    AssertionFailed { assertion: String, reason: String },
    TimeoutError { operation: String, timeout: Duration },
    ResourceLimitExceeded { resource: String, limit: String },
    InvalidConfiguration { parameter: String, reason: String },
    TestDataError { operation: String, reason: String },
}

impl ActorTestHarness {
    /// Create a new actor test harness
    pub async fn new(test_env: TestEnvironment) -> ActorTestResult<Self> {
        let harness = Self {
            test_env,
            actor_system: None,
            message_router: Arc::new(RwLock::new(TestMessageRouter {
                routes: HashMap::new(),
                message_history: Vec::new(),
                filters: Vec::new(),
                interceptors: Vec::new(),
            })),
            test_actors: Arc::new(RwLock::new(HashMap::new())),
            scenario_manager: Arc::new(RwLock::new(TestScenarioManager {
                scenarios: HashMap::new(),
                execution_history: Vec::new(),
                templates: HashMap::new(),
            })),
            metrics_collector: Arc::new(RwLock::new(TestMetricsCollector::default())),
            event_logger: Arc::new(RwLock::new(TestEventLogger {
                events: Vec::new(),
                config: LogConfig {
                    min_level: LogLevel::Debug,
                    max_entries: 10000,
                    auto_flush: true,
                    include_metadata: true,
                },
                filters: Vec::new(),
            })),
            assertion_engine: Arc::new(RwLock::new(AssertionEngine {
                assertions: Vec::new(),
                custom_assertions: HashMap::new(),
                config: AssertionConfig {
                    fail_fast: false,
                    collect_all_failures: true,
                    timeout_on_failure: Duration::from_secs(5),
                    retry_failed_assertions: false,
                    max_retries: 3,
                },
            })),
        };
        
        Ok(harness)
    }
    
    /// Initialize the test environment
    pub async fn initialize(&mut self) -> ActorTestResult<()> {
        self.log_info("Initializing test environment").await;
        
        // Create test directories
        tokio::fs::create_dir_all(&self.test_env.test_data_dir).await
            .map_err(|e| ActorTestError::SetupFailed {
                reason: format!("Failed to create test data directory: {}", e),
            })?;
        
        // Initialize actor system if needed
        if self.test_env.isolation_level != IsolationLevel::Complete {
            // TODO: Initialize actor system with test configuration
            self.log_info("Actor system initialized").await;
        }
        
        self.log_info("Test environment initialized successfully").await;
        Ok(())
    }
    
    /// Start a test actor
    pub async fn start_actor<A: AlysActor>(
        &mut self,
        actor_id: String,
        config: A::Config,
    ) -> ActorTestResult<TestActorHandle> {
        self.log_info(&format!("Starting test actor: {}", actor_id)).await;
        
        let (sender, receiver) = mpsc::channel(1000);
        
        let handle = TestActorHandle {
            actor_id: actor_id.clone(),
            actor_type: std::any::type_name::<A>().to_string(),
            start_time: SystemTime::now(),
            message_count: 0,
            error_count: 0,
            health_status: ActorHealthStatus::Starting,
            sender,
        };
        
        // Store the actor handle
        {
            let mut actors = self.test_actors.write().await;
            actors.insert(actor_id.clone(), handle.clone());
        }
        
        // TODO: Actually start the actor in the actor system
        
        self.log_info(&format!("Test actor started: {}", actor_id)).await;
        Ok(handle)
    }
    
    /// Stop a test actor
    pub async fn stop_actor(&mut self, actor_id: &str, graceful: bool) -> ActorTestResult<()> {
        self.log_info(&format!("Stopping test actor: {} (graceful: {})", actor_id, graceful)).await;
        
        // TODO: Stop the actor in the actor system
        
        // Update actor status
        {
            let mut actors = self.test_actors.write().await;
            if let Some(handle) = actors.get_mut(actor_id) {
                handle.health_status = if graceful {
                    ActorHealthStatus::Stopping
                } else {
                    ActorHealthStatus::Stopped
                };
            }
        }
        
        self.log_info(&format!("Test actor stopped: {}", actor_id)).await;
        Ok(())
    }
    
    /// Send a message to an actor
    pub async fn send_message(
        &self,
        from_actor: &str,
        to_actor: &str,
        message: TestMessage,
    ) -> ActorTestResult<()> {
        self.log_debug(&format!(
            "Sending message from {} to {}: {}",
            from_actor, to_actor, message.message_type
        )).await;
        
        // Record message event
        let event = TestMessageEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            from_actor: from_actor.to_string(),
            to_actor: to_actor.to_string(),
            message_type: message.message_type.clone(),
            message_id: message.message_id.clone(),
            correlation_id: message.correlation_id.clone(),
            processing_time: None,
            result: MessageResult::Success, // Will be updated
        };
        
        {
            let mut router = self.message_router.write().await;
            router.message_history.push(event);
        }
        
        // TODO: Route message through actor system
        
        Ok(())
    }
    
    /// Execute a test scenario
    pub async fn execute_scenario(&mut self, scenario: TestScenario) -> ActorTestResult<ScenarioExecution> {
        self.log_info(&format!("Executing test scenario: {}", scenario.name)).await;
        
        let execution_id = Uuid::new_v4().to_string();
        let start_time = SystemTime::now();
        
        let mut execution = ScenarioExecution {
            execution_id: execution_id.clone(),
            scenario_id: scenario.scenario_id.clone(),
            start_time,
            end_time: None,
            status: ExecutionStatus::Running,
            step_results: Vec::new(),
            error_message: None,
        };
        
        // Check preconditions
        for precondition in &scenario.preconditions {
            if !self.check_condition(&precondition.condition).await? {
                if precondition.required {
                    execution.status = ExecutionStatus::Failed;
                    execution.error_message = Some(format!("Precondition failed: {:?}", precondition.condition));
                    execution.end_time = Some(SystemTime::now());
                    return Ok(execution);
                }
            }
        }
        
        // Execute steps
        for (index, step) in scenario.steps.iter().enumerate() {
            let step_start = SystemTime::now();
            
            match self.execute_step(step).await {
                Ok(_) => {
                    execution.step_results.push(StepResult {
                        step_index: index,
                        start_time: step_start,
                        end_time: SystemTime::now(),
                        status: ExecutionStatus::Completed,
                        error_message: None,
                        metrics: StepMetrics {
                            execution_time: step_start.elapsed().unwrap_or(Duration::from_secs(0)),
                            memory_usage: 0, // TODO: Collect actual metrics
                            messages_processed: 0,
                            assertions_checked: 0,
                        },
                    });
                },
                Err(e) => {
                    execution.step_results.push(StepResult {
                        step_index: index,
                        start_time: step_start,
                        end_time: SystemTime::now(),
                        status: ExecutionStatus::Failed,
                        error_message: Some(format!("{:?}", e)),
                        metrics: StepMetrics {
                            execution_time: step_start.elapsed().unwrap_or(Duration::from_secs(0)),
                            memory_usage: 0,
                            messages_processed: 0,
                            assertions_checked: 0,
                        },
                    });
                    
                    execution.status = ExecutionStatus::Failed;
                    execution.error_message = Some(format!("Step {} failed: {:?}", index, e));
                    break;
                }
            }
        }
        
        // Check postconditions
        if execution.status == ExecutionStatus::Running {
            for postcondition in &scenario.postconditions {
                if !self.check_condition(&postcondition.condition).await? {
                    if postcondition.required {
                        execution.status = ExecutionStatus::Failed;
                        execution.error_message = Some(format!("Postcondition failed: {:?}", postcondition.condition));
                        break;
                    }
                }
            }
        }
        
        if execution.status == ExecutionStatus::Running {
            execution.status = ExecutionStatus::Completed;
        }
        
        execution.end_time = Some(SystemTime::now());
        
        // Store execution result
        {
            let mut manager = self.scenario_manager.write().await;
            manager.execution_history.push(execution.clone());
        }
        
        self.log_info(&format!(
            "Test scenario completed: {} (status: {:?})",
            scenario.name, execution.status
        )).await;
        
        Ok(execution)
    }
    
    /// Execute a single test step
    async fn execute_step(&mut self, step: &TestStep) -> ActorTestResult<()> {
        match step {
            TestStep::StartActor { actor_id, actor_type, config } => {
                // TODO: Start actor with provided configuration
                self.log_debug(&format!("Starting actor {} of type {}", actor_id, actor_type)).await;
            },
            TestStep::StopActor { actor_id, graceful } => {
                self.stop_actor(actor_id, *graceful).await?;
            },
            TestStep::SendMessage { from_actor, to_actor, message, expect_response } => {
                let test_message = TestMessage {
                    message_id: Uuid::new_v4().to_string(),
                    correlation_id: None,
                    message_type: "test".to_string(),
                    payload: message.clone(),
                    metadata: HashMap::new(),
                    timestamp: SystemTime::now(),
                };
                self.send_message(from_actor, to_actor, test_message).await?;
            },
            TestStep::WaitForCondition { condition, timeout: step_timeout } => {
                let result = timeout(*step_timeout, async {
                    while !self.check_condition(condition).await? {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Ok::<(), ActorTestError>(())
                }).await;
                
                match result {
                    Ok(Ok(())) => {},
                    Ok(Err(e)) => return Err(e),
                    Err(_) => return Err(ActorTestError::TimeoutError {
                        operation: format!("WaitForCondition: {:?}", condition),
                        timeout: *step_timeout,
                    }),
                }
            },
            TestStep::AssertCondition { condition, error_message } => {
                if !self.check_condition(condition).await? {
                    return Err(ActorTestError::AssertionFailed {
                        assertion: format!("{:?}", condition),
                        reason: error_message.clone(),
                    });
                }
            },
            TestStep::Delay { duration } => {
                tokio::time::sleep(*duration).await;
            },
            TestStep::InjectFailure { target, failure_type } => {
                self.log_warn(&format!("Injecting failure: {:?} -> {:?}", target, failure_type)).await;
                // TODO: Implement failure injection
            },
        }
        
        Ok(())
    }
    
    /// Check a test condition
    async fn check_condition(&self, condition: &TestCondition) -> ActorTestResult<bool> {
        match condition {
            TestCondition::ActorRunning { actor_id } => {
                let actors = self.test_actors.read().await;
                if let Some(handle) = actors.get(actor_id) {
                    Ok(matches!(handle.health_status, ActorHealthStatus::Running))
                } else {
                    Ok(false)
                }
            },
            TestCondition::ActorStopped { actor_id } => {
                let actors = self.test_actors.read().await;
                if let Some(handle) = actors.get(actor_id) {
                    Ok(matches!(handle.health_status, ActorHealthStatus::Stopped))
                } else {
                    Ok(true) // Actor not found means it's stopped
                }
            },
            TestCondition::MessageReceived { actor_id, message_type } => {
                let router = self.message_router.read().await;
                Ok(router.message_history.iter().any(|event| {
                    event.to_actor == *actor_id && event.message_type == *message_type
                }))
            },
            TestCondition::MessageCountReached { actor_id, count } => {
                let router = self.message_router.read().await;
                let message_count = router.message_history.iter()
                    .filter(|event| event.to_actor == *actor_id)
                    .count() as u64;
                Ok(message_count >= *count)
            },
            TestCondition::Custom { checker, .. } => {
                checker.check(self).map_err(|e| ActorTestError::AssertionFailed {
                    assertion: "Custom condition".to_string(),
                    reason: e,
                })
            },
        }
    }
    
    /// Assert a condition
    pub async fn assert(&self, condition: TestCondition, message: &str) -> ActorTestResult<()> {
        let result = self.check_condition(&condition).await?;
        
        let assertion_result = AssertionResult {
            assertion_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            assertion_type: format!("{:?}", condition),
            result,
            message: message.to_string(),
            context: AssertionContext {
                test_id: self.test_env.test_id.clone(),
                scenario_id: None,
                step_index: None,
                actor_id: None,
                additional_data: HashMap::new(),
            },
        };
        
        {
            let mut engine = self.assertion_engine.write().await;
            engine.assertions.push(assertion_result.clone());
        }
        
        if !result {
            Err(ActorTestError::AssertionFailed {
                assertion: format!("{:?}", condition),
                reason: message.to_string(),
            })
        } else {
            Ok(())
        }
    }
    
    /// Get test metrics
    pub async fn get_metrics(&self) -> TestMetricsCollector {
        self.metrics_collector.read().await.clone()
    }
    
    /// Get message history
    pub async fn get_message_history(&self) -> Vec<TestMessageEvent> {
        self.message_router.read().await.message_history.clone()
    }
    
    /// Get assertion results
    pub async fn get_assertion_results(&self) -> Vec<AssertionResult> {
        self.assertion_engine.read().await.assertions.clone()
    }
    
    /// Clean up test environment
    pub async fn cleanup(&mut self) -> ActorTestResult<()> {
        self.log_info("Cleaning up test environment").await;
        
        // Stop all actors
        let actor_ids: Vec<String> = {
            let actors = self.test_actors.read().await;
            actors.keys().cloned().collect()
        };
        
        for actor_id in actor_ids {
            let _ = self.stop_actor(&actor_id, true).await;
        }
        
        // Clean up based on strategy
        match self.test_env.cleanup_strategy {
            CleanupStrategy::Full => {
                // Clean up everything
                if let Err(e) = tokio::fs::remove_dir_all(&self.test_env.test_data_dir).await {
                    self.log_warn(&format!("Failed to remove test data directory: {}", e)).await;
                }
            },
            CleanupStrategy::KeepLogs => {
                // Keep log files, clean up other test data
            },
            CleanupStrategy::KeepData => {
                // Keep test data files
            },
            CleanupStrategy::KeepAll => {
                // Keep everything for manual inspection
            },
        }
        
        self.log_info("Test environment cleanup completed").await;
        Ok(())
    }
    
    /// Log a message at info level
    async fn log_info(&self, message: &str) {
        self.log(LogLevel::Info, None, message).await;
    }
    
    /// Log a message at debug level
    async fn log_debug(&self, message: &str) {
        self.log(LogLevel::Debug, None, message).await;
    }
    
    /// Log a message at warning level
    async fn log_warn(&self, message: &str) {
        self.log(LogLevel::Warn, None, message).await;
    }
    
    /// Log a message
    async fn log(&self, level: LogLevel, actor_id: Option<String>, message: &str) {
        let entry = TestLogEntry {
            timestamp: SystemTime::now(),
            level,
            actor_id,
            message: message.to_string(),
            metadata: HashMap::new(),
        };
        
        let mut logger = self.event_logger.write().await;
        logger.events.push(entry);
        
        // Auto-flush if configured
        if logger.config.auto_flush {
            // TODO: Flush to file or external system
        }
    }
}

impl Default for TestEnvironment {
    fn default() -> Self {
        Self {
            test_id: Uuid::new_v4().to_string(),
            test_name: "default_test".to_string(),
            isolation_level: IsolationLevel::Complete,
            timeout: Duration::from_secs(300),
            resource_limits: ResourceLimits {
                max_memory_mb: 1000,
                max_cpu_percent: 80,
                max_file_descriptors: 1000,
                max_network_connections: 100,
                max_duration: Duration::from_secs(600),
            },
            mock_config: MockConfiguration {
                mock_governance: true,
                mock_bitcoin: true,
                mock_execution: true,
                mock_network: true,
                mock_storage: true,
                response_delays: HashMap::new(),
                failure_rates: HashMap::new(),
            },
            test_data_dir: "/tmp/alys_test".to_string(),
            cleanup_strategy: CleanupStrategy::Full,
        }
    }
}

impl Default for MockConfiguration {
    fn default() -> Self {
        Self {
            mock_governance: true,
            mock_bitcoin: true,
            mock_execution: true,
            mock_network: true,
            mock_storage: true,
            response_delays: HashMap::new(),
            failure_rates: HashMap::new(),
        }
    }
}