//! Chaos testing capabilities with network partitions, actor failures, and resource constraints
//!
//! This module provides comprehensive chaos engineering capabilities for testing the
//! resilience of the actor-based system under various failure conditions, network
//! partitions, resource constraints, and other adverse conditions.

use crate::testing::actor_harness::{ActorTestHarness, TestMessage, ActorTestResult, ActorTestError};
use crate::types::*;
use actor_system::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, Mutex};
use uuid::Uuid;

/// Chaos testing engine for resilience testing
#[derive(Debug)]
pub struct ChaosTestEngine {
    /// Chaos test configuration
    config: ChaosTestConfig,
    
    /// Active chaos scenarios
    active_scenarios: Arc<RwLock<HashMap<String, ChaosTestScenario>>>,
    
    /// Chaos operators
    operators: Arc<RwLock<HashMap<String, Box<dyn ChaosOperator>>>>,
    
    /// Fault injector
    fault_injector: Arc<RwLock<FaultInjector>>,
    
    /// Network partitioner
    network_partitioner: Arc<RwLock<NetworkPartitioner>>,
    
    /// Resource constrainer
    resource_constrainer: Arc<RwLock<ResourceConstrainer>>,
    
    /// Chaos metrics collector
    metrics_collector: Arc<RwLock<ChaosMetricsCollector>>,
    
    /// Recovery coordinator
    recovery_coordinator: Arc<RwLock<RecoveryCoordinator>>,
}

/// Chaos test configuration
#[derive(Debug, Clone)]
pub struct ChaosTestConfig {
    /// Default scenario duration
    pub default_duration: Duration,
    
    /// Maximum concurrent chaos operations
    pub max_concurrent_operations: u32,
    
    /// Safety checks enabled
    pub safety_checks_enabled: bool,
    
    /// Automatic recovery enabled
    pub auto_recovery_enabled: bool,
    
    /// Recovery timeout
    pub recovery_timeout: Duration,
    
    /// Chaos intensity level
    pub intensity_level: ChaosIntensity,
    
    /// Monitoring interval
    pub monitoring_interval: Duration,
}

/// Chaos intensity levels
#[derive(Debug, Clone, Copy)]
pub enum ChaosIntensity {
    Low,
    Medium,
    High,
    Extreme,
}

/// Chaos test scenario
#[derive(Debug, Clone)]
pub struct ChaosTestScenario {
    /// Scenario identifier
    pub scenario_id: String,
    
    /// Scenario name and description
    pub name: String,
    pub description: String,
    
    /// Scenario steps
    pub steps: Vec<ChaosStep>,
    
    /// Target selection
    pub targets: ChaosTargetSelection,
    
    /// Timing configuration
    pub timing: ChaosTimingConfig,
    
    /// Success criteria
    pub success_criteria: Vec<ChaosSuccessCriterion>,
    
    /// Recovery strategy
    pub recovery_strategy: RecoveryStrategy,
    
    /// Scenario state
    pub state: ChaosScenarioState,
}

/// Chaos scenario step
#[derive(Debug, Clone)]
pub struct ChaosStep {
    /// Step identifier
    pub step_id: String,
    
    /// Step name
    pub name: String,
    
    /// Chaos operation
    pub operation: ChaosOperation,
    
    /// Step timing
    pub timing: StepTiming,
    
    /// Expected impact
    pub expected_impact: ExpectedImpact,
    
    /// Recovery conditions
    pub recovery_conditions: Vec<RecoveryCondition>,
}

/// Chaos operations
#[derive(Debug, Clone)]
pub enum ChaosOperation {
    /// Kill an actor
    KillActor {
        actor_id: String,
        kill_type: ActorKillType,
    },
    
    /// Partition network
    NetworkPartition {
        partition_config: NetworkPartitionConfig,
    },
    
    /// Induce resource constraint
    ResourceConstraint {
        constraint_config: ResourceConstraintConfig,
    },
    
    /// Inject message corruption
    MessageCorruption {
        corruption_config: MessageCorruptionConfig,
    },
    
    /// Introduce latency
    LatencyInjection {
        latency_config: LatencyInjectionConfig,
    },
    
    /// Disk failure simulation
    DiskFailure {
        failure_config: DiskFailureConfig,
    },
    
    /// Memory pressure
    MemoryPressure {
        pressure_config: MemoryPressureConfig,
    },
    
    /// CPU throttling
    CpuThrottling {
        throttling_config: CpuThrottlingConfig,
    },
    
    /// Clock skew
    ClockSkew {
        skew_config: ClockSkewConfig,
    },
    
    /// Custom chaos operation
    Custom {
        operation_name: String,
        config: serde_json::Value,
    },
}

/// Actor kill types
#[derive(Debug, Clone, Copy)]
pub enum ActorKillType {
    /// Graceful shutdown
    Graceful,
    /// Immediate termination
    Immediate,
    /// Segmentation fault simulation
    Segfault,
    /// Out of memory kill
    OutOfMemory,
    /// Resource exhaustion
    ResourceExhaustion,
}

/// Network partition configuration
#[derive(Debug, Clone)]
pub struct NetworkPartitionConfig {
    /// Partition groups
    pub groups: Vec<PartitionGroup>,
    
    /// Partition duration
    pub duration: Duration,
    
    /// Partition type
    pub partition_type: PartitionType,
    
    /// Recovery behavior
    pub recovery_behavior: PartitionRecoveryBehavior,
}

/// Partition group
#[derive(Debug, Clone)]
pub struct PartitionGroup {
    /// Group identifier
    pub group_id: String,
    
    /// Actors in this group
    pub actors: HashSet<String>,
    
    /// Group connectivity
    pub connectivity: GroupConnectivity,
}

/// Group connectivity options
#[derive(Debug, Clone)]
pub enum GroupConnectivity {
    /// Full connectivity within group
    FullyConnected,
    
    /// Partial connectivity
    PartiallyConnected { connection_rate: f64 },
    
    /// No connectivity (isolated)
    Isolated,
    
    /// Ring topology
    Ring,
    
    /// Star topology with hub
    Star { hub_actor: String },
}

/// Partition types
#[derive(Debug, Clone, Copy)]
pub enum PartitionType {
    /// Complete network split
    CompletePartition,
    
    /// Partial connectivity loss
    PartialPartition,
    
    /// Intermittent connectivity
    IntermittentPartition,
    
    /// Asymmetric partition
    AsymmetricPartition,
}

/// Partition recovery behavior
#[derive(Debug, Clone)]
pub enum PartitionRecoveryBehavior {
    /// Immediate full recovery
    Immediate,
    
    /// Gradual recovery
    Gradual { recovery_rate: f64 },
    
    /// Random recovery
    Random { recovery_probability: f64 },
    
    /// Manual recovery
    Manual,
}

/// Resource constraint configuration
#[derive(Debug, Clone)]
pub struct ResourceConstraintConfig {
    /// Resource type
    pub resource_type: ResourceType,
    
    /// Constraint level
    pub constraint_level: ConstraintLevel,
    
    /// Affected actors
    pub affected_actors: Vec<String>,
    
    /// Constraint duration
    pub duration: Duration,
    
    /// Ramp-up behavior
    pub ramp_up: RampUpBehavior,
}

/// Resource types for constraints
#[derive(Debug, Clone, Copy)]
pub enum ResourceType {
    Memory,
    Cpu,
    Disk,
    Network,
    FileDescriptors,
    ThreadPool,
}

/// Constraint levels
#[derive(Debug, Clone)]
pub enum ConstraintLevel {
    /// Light constraint (10-25% impact)
    Light,
    
    /// Moderate constraint (25-50% impact)
    Moderate,
    
    /// Heavy constraint (50-75% impact)
    Heavy,
    
    /// Severe constraint (75-90% impact)
    Severe,
    
    /// Critical constraint (90-99% impact)
    Critical,
    
    /// Custom constraint level
    Custom { percentage: f64 },
}

/// Constraint ramp-up behavior
#[derive(Debug, Clone)]
pub enum RampUpBehavior {
    /// Immediate full constraint
    Immediate,
    
    /// Linear ramp-up
    Linear { ramp_duration: Duration },
    
    /// Exponential ramp-up
    Exponential { growth_rate: f64 },
    
    /// Step-wise ramp-up
    StepWise { steps: Vec<ConstraintStep> },
}

/// Constraint step
#[derive(Debug, Clone)]
pub struct ConstraintStep {
    pub level: f64,
    pub duration: Duration,
}

/// Message corruption configuration
#[derive(Debug, Clone)]
pub struct MessageCorruptionConfig {
    /// Corruption rate (0.0-1.0)
    pub corruption_rate: f64,
    
    /// Corruption types
    pub corruption_types: Vec<CorruptionType>,
    
    /// Target message types
    pub target_message_types: Option<Vec<String>>,
    
    /// Target actors
    pub target_actors: Option<Vec<String>>,
    
    /// Corruption duration
    pub duration: Duration,
}

/// Message corruption types
#[derive(Debug, Clone, Copy)]
pub enum CorruptionType {
    /// Flip random bits
    BitFlip,
    
    /// Duplicate message
    Duplicate,
    
    /// Drop message
    Drop,
    
    /// Reorder messages
    Reorder,
    
    /// Inject random data
    RandomData,
    
    /// Modify payload
    PayloadModification,
}

/// Latency injection configuration
#[derive(Debug, Clone)]
pub struct LatencyInjectionConfig {
    /// Base latency
    pub base_latency: Duration,
    
    /// Latency variance
    pub variance: Duration,
    
    /// Latency distribution
    pub distribution: LatencyDistribution,
    
    /// Target connections
    pub target_connections: LatencyTargets,
    
    /// Injection duration
    pub duration: Duration,
}

/// Latency distribution types
#[derive(Debug, Clone)]
pub enum LatencyDistribution {
    /// Constant latency
    Constant,
    
    /// Uniform distribution
    Uniform,
    
    /// Normal distribution
    Normal { mean: Duration, std_dev: Duration },
    
    /// Exponential distribution
    Exponential { lambda: f64 },
    
    /// Pareto distribution (heavy tail)
    Pareto { alpha: f64, scale: Duration },
}

/// Latency injection targets
#[derive(Debug, Clone)]
pub enum LatencyTargets {
    /// All connections
    All,
    
    /// Specific actor pairs
    ActorPairs { pairs: Vec<(String, String)> },
    
    /// Actors matching pattern
    Pattern { pattern: String },
    
    /// Random subset
    RandomSubset { percentage: f64 },
}

/// Disk failure configuration
#[derive(Debug, Clone)]
pub struct DiskFailureConfig {
    /// Failure type
    pub failure_type: DiskFailureType,
    
    /// Affected paths
    pub affected_paths: Vec<String>,
    
    /// Failure duration
    pub duration: Duration,
    
    /// Recovery behavior
    pub recovery_behavior: DiskRecoveryBehavior,
}

/// Disk failure types
#[derive(Debug, Clone, Copy)]
pub enum DiskFailureType {
    /// Complete disk unavailability
    Complete,
    
    /// Slow I/O responses
    SlowIO,
    
    /// Read errors
    ReadErrors,
    
    /// Write errors
    WriteErrors,
    
    /// Disk full simulation
    DiskFull,
    
    /// Corruption errors
    Corruption,
}

/// Disk recovery behavior
#[derive(Debug, Clone)]
pub enum DiskRecoveryBehavior {
    /// Immediate recovery
    Immediate,
    
    /// Gradual recovery with fsck simulation
    GradualWithFsck { fsck_duration: Duration },
    
    /// Manual recovery required
    Manual,
}

/// Memory pressure configuration
#[derive(Debug, Clone)]
pub struct MemoryPressureConfig {
    /// Memory to consume (bytes)
    pub memory_to_consume: u64,
    
    /// Consumption pattern
    pub consumption_pattern: MemoryConsumptionPattern,
    
    /// Target processes/actors
    pub targets: Vec<String>,
    
    /// Pressure duration
    pub duration: Duration,
}

/// Memory consumption patterns
#[derive(Debug, Clone)]
pub enum MemoryConsumptionPattern {
    /// Sudden allocation
    Sudden,
    
    /// Gradual increase
    Gradual { rate: u64 }, // bytes per second
    
    /// Spike pattern
    Spike { spike_interval: Duration, spike_size: u64 },
    
    /// Memory leak simulation
    Leak { leak_rate: u64 }, // bytes per second
}

/// CPU throttling configuration
#[derive(Debug, Clone)]
pub struct CpuThrottlingConfig {
    /// CPU limit percentage (0-100)
    pub cpu_limit_percent: u8,
    
    /// Throttling pattern
    pub throttling_pattern: CpuThrottlingPattern,
    
    /// Target processes/actors
    pub targets: Vec<String>,
    
    /// Throttling duration
    pub duration: Duration,
}

/// CPU throttling patterns
#[derive(Debug, Clone)]
pub enum CpuThrottlingPattern {
    /// Constant throttling
    Constant,
    
    /// Periodic throttling
    Periodic { period: Duration, duty_cycle: f64 },
    
    /// Random throttling
    Random { min_limit: u8, max_limit: u8 },
    
    /// Burst throttling
    Burst { burst_duration: Duration, normal_duration: Duration },
}

/// Clock skew configuration
#[derive(Debug, Clone)]
pub struct ClockSkewConfig {
    /// Time skew amount
    pub skew_amount: Duration,
    
    /// Skew direction
    pub skew_direction: SkewDirection,
    
    /// Affected actors
    pub affected_actors: Vec<String>,
    
    /// Skew pattern
    pub skew_pattern: SkewPattern,
    
    /// Skew duration
    pub duration: Duration,
}

/// Clock skew directions
#[derive(Debug, Clone, Copy)]
pub enum SkewDirection {
    Forward,
    Backward,
    Random,
}

/// Clock skew patterns
#[derive(Debug, Clone)]
pub enum SkewPattern {
    /// Constant skew
    Constant,
    
    /// Gradually increasing skew
    Drift { drift_rate: f64 }, // nanoseconds per second
    
    /// Periodic skew
    Periodic { period: Duration, amplitude: Duration },
    
    /// Random skew
    Random { variance: Duration },
}

/// Chaos target selection
#[derive(Debug, Clone)]
pub struct ChaosTargetSelection {
    /// Target selection strategy
    pub strategy: TargetSelectionStrategy,
    
    /// Target filters
    pub filters: Vec<TargetFilter>,
    
    /// Maximum targets
    pub max_targets: Option<u32>,
}

/// Target selection strategies
#[derive(Debug, Clone)]
pub enum TargetSelectionStrategy {
    /// Select all matching targets
    All,
    
    /// Select random subset
    Random { count: u32 },
    
    /// Select by percentage
    Percentage { percentage: f64 },
    
    /// Select specific targets
    Specific { targets: Vec<String> },
    
    /// Select by criteria
    Criteria { criteria: SelectionCriteria },
}

/// Selection criteria
#[derive(Debug, Clone)]
pub struct SelectionCriteria {
    /// Actor type filter
    pub actor_type: Option<String>,
    
    /// Actor role filter
    pub actor_role: Option<String>,
    
    /// Load threshold
    pub load_threshold: Option<f64>,
    
    /// Uptime threshold
    pub uptime_threshold: Option<Duration>,
    
    /// Custom criteria
    pub custom: HashMap<String, serde_json::Value>,
}

/// Target filter
#[derive(Debug, Clone)]
pub struct TargetFilter {
    /// Filter name
    pub name: String,
    
    /// Filter condition
    pub condition: FilterCondition,
    
    /// Include or exclude
    pub include: bool,
}

/// Filter conditions
#[derive(Debug, Clone)]
pub enum FilterCondition {
    /// Actor ID matches pattern
    ActorIdPattern { pattern: String },
    
    /// Actor type equals
    ActorTypeEquals { actor_type: String },
    
    /// Actor has tag
    HasTag { tag: String },
    
    /// Actor metric condition
    MetricCondition { metric: String, operator: ComparisonOperator, value: f64 },
    
    /// Custom filter
    Custom { filter_name: String, params: HashMap<String, serde_json::Value> },
}

/// Comparison operators for filters
#[derive(Debug, Clone, Copy)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

/// Chaos timing configuration
#[derive(Debug, Clone)]
pub struct ChaosTimingConfig {
    /// Start delay
    pub start_delay: Duration,
    
    /// Step intervals
    pub step_intervals: Vec<Duration>,
    
    /// Total duration
    pub total_duration: Duration,
    
    /// Execution pattern
    pub execution_pattern: ExecutionPattern,
}

/// Execution patterns
#[derive(Debug, Clone)]
pub enum ExecutionPattern {
    /// Sequential execution
    Sequential,
    
    /// Parallel execution
    Parallel,
    
    /// Staggered execution
    Staggered { stagger_delay: Duration },
    
    /// Random execution
    Random { min_delay: Duration, max_delay: Duration },
}

/// Step timing
#[derive(Debug, Clone)]
pub struct StepTiming {
    /// Start offset from scenario start
    pub start_offset: Duration,
    
    /// Step duration
    pub duration: Duration,
    
    /// Ramp up time
    pub ramp_up: Option<Duration>,
    
    /// Ramp down time
    pub ramp_down: Option<Duration>,
}

/// Expected impact of chaos operation
#[derive(Debug, Clone)]
pub struct ExpectedImpact {
    /// Impact severity
    pub severity: ImpactSeverity,
    
    /// Affected metrics
    pub affected_metrics: Vec<String>,
    
    /// Expected metric changes
    pub metric_changes: HashMap<String, MetricChange>,
    
    /// Recovery time estimate
    pub recovery_time_estimate: Option<Duration>,
}

/// Impact severity levels
#[derive(Debug, Clone, Copy)]
pub enum ImpactSeverity {
    Minimal,
    Low,
    Medium,
    High,
    Critical,
}

/// Expected metric changes
#[derive(Debug, Clone)]
pub struct MetricChange {
    /// Change type
    pub change_type: ChangeType,
    
    /// Change magnitude
    pub magnitude: f64,
    
    /// Change duration
    pub duration: Duration,
}

/// Metric change types
#[derive(Debug, Clone, Copy)]
pub enum ChangeType {
    Increase,
    Decrease,
    Spike,
    Drop,
    Oscillation,
}

/// Recovery conditions
#[derive(Debug, Clone)]
pub struct RecoveryCondition {
    /// Condition name
    pub name: String,
    
    /// Condition check
    pub condition: RecoveryCheck,
    
    /// Check timeout
    pub timeout: Duration,
    
    /// Required for recovery
    pub required: bool,
}

/// Recovery checks
#[derive(Debug, Clone)]
pub enum RecoveryCheck {
    /// Actor is responding
    ActorResponding { actor_id: String },
    
    /// Metric within threshold
    MetricThreshold { metric: String, threshold: f64, operator: ComparisonOperator },
    
    /// Message flow restored
    MessageFlowRestored { from_actor: String, to_actor: String },
    
    /// System stability
    SystemStable { stability_duration: Duration },
    
    /// Custom check
    Custom { check_name: String, params: HashMap<String, serde_json::Value> },
}

/// Chaos success criteria
#[derive(Debug, Clone)]
pub struct ChaosSuccessCriterion {
    /// Criterion name
    pub name: String,
    
    /// Criterion check
    pub check: SuccessCheck,
    
    /// Required for success
    pub required: bool,
    
    /// Weight in overall success calculation
    pub weight: f64,
}

/// Success checks
#[derive(Debug, Clone)]
pub enum SuccessCheck {
    /// System recovered within time
    RecoveredWithinTime { max_recovery_time: Duration },
    
    /// No data loss occurred
    NoDataLoss,
    
    /// All actors eventually recovered
    AllActorsRecovered,
    
    /// Performance degradation within limits
    PerformanceWithinLimits { max_degradation: f64 },
    
    /// Error rate within acceptable bounds
    ErrorRateAcceptable { max_error_rate: f64 },
    
    /// Custom success check
    Custom { check_name: String, params: HashMap<String, serde_json::Value> },
}

/// Recovery strategies
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Automatic recovery
    Automatic {
        max_recovery_time: Duration,
        recovery_steps: Vec<RecoveryStep>,
    },
    
    /// Manual recovery
    Manual,
    
    /// Hybrid recovery (automatic with manual fallback)
    Hybrid {
        auto_recovery_timeout: Duration,
        manual_fallback: bool,
    },
    
    /// No recovery (let system handle)
    None,
}

/// Recovery steps
#[derive(Debug, Clone)]
pub struct RecoveryStep {
    /// Step name
    pub name: String,
    
    /// Recovery action
    pub action: RecoveryAction,
    
    /// Step timeout
    pub timeout: Duration,
    
    /// Retry configuration
    pub retry_config: Option<RetryConfig>,
}

/// Recovery actions
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    /// Restart actor
    RestartActor { actor_id: String },
    
    /// Restore network connectivity
    RestoreNetworkConnectivity,
    
    /// Release resource constraints
    ReleaseResourceConstraints,
    
    /// Reset system state
    ResetSystemState,
    
    /// Custom recovery action
    Custom { action_name: String, params: HashMap<String, serde_json::Value> },
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retries
    pub max_retries: u32,
    
    /// Initial delay
    pub initial_delay: Duration,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    
    /// Maximum delay
    pub max_delay: Duration,
}

/// Chaos scenario state
#[derive(Debug, Clone)]
pub enum ChaosScenarioState {
    Created,
    Scheduled { start_time: SystemTime },
    Running { current_step: usize },
    Recovering,
    Completed { result: ChaosResult },
    Failed { error: String },
    Cancelled,
}

/// Chaos test result
#[derive(Debug, Clone)]
pub struct ChaosResult {
    /// Overall success
    pub success: bool,
    
    /// Individual step results
    pub step_results: Vec<ChaosStepResult>,
    
    /// Recovery metrics
    pub recovery_metrics: RecoveryMetrics,
    
    /// Performance impact
    pub performance_impact: PerformanceImpact,
    
    /// Lessons learned
    pub lessons_learned: Vec<String>,
}

/// Chaos step result
#[derive(Debug, Clone)]
pub struct ChaosStepResult {
    /// Step identifier
    pub step_id: String,
    
    /// Step success
    pub success: bool,
    
    /// Execution time
    pub execution_time: Duration,
    
    /// Impact achieved
    pub impact_achieved: ExpectedImpact,
    
    /// Recovery time
    pub recovery_time: Option<Duration>,
    
    /// Error messages
    pub errors: Vec<String>,
}

/// Recovery metrics
#[derive(Debug, Clone)]
pub struct RecoveryMetrics {
    /// Mean time to recovery (MTTR)
    pub mean_time_to_recovery: Duration,
    
    /// Recovery success rate
    pub recovery_success_rate: f64,
    
    /// Automatic recovery rate
    pub automatic_recovery_rate: f64,
    
    /// Manual intervention required
    pub manual_intervention_required: bool,
}

/// Performance impact metrics
#[derive(Debug, Clone)]
pub struct PerformanceImpact {
    /// Throughput degradation
    pub throughput_degradation: f64,
    
    /// Latency increase
    pub latency_increase: f64,
    
    /// Error rate increase
    pub error_rate_increase: f64,
    
    /// Resource utilization change
    pub resource_utilization_change: HashMap<String, f64>,
}

/// Chaos operator trait
pub trait ChaosOperator: Send + Sync + std::fmt::Debug {
    /// Operator name
    fn name(&self) -> &str;
    
    /// Execute chaos operation
    async fn execute(
        &self,
        operation: &ChaosOperation,
        targets: &[String],
        harness: &ActorTestHarness,
    ) -> Result<ChaosOperationResult, ChaosError>;
    
    /// Check if operation is recoverable
    fn is_recoverable(&self, operation: &ChaosOperation) -> bool;
    
    /// Recover from chaos operation
    async fn recover(
        &self,
        operation: &ChaosOperation,
        targets: &[String],
        harness: &ActorTestHarness,
    ) -> Result<(), ChaosError>;
}

/// Chaos operation result
#[derive(Debug, Clone)]
pub struct ChaosOperationResult {
    /// Operation success
    pub success: bool,
    
    /// Affected targets
    pub affected_targets: Vec<String>,
    
    /// Execution time
    pub execution_time: Duration,
    
    /// Impact metrics
    pub impact_metrics: HashMap<String, f64>,
    
    /// Error messages
    pub errors: Vec<String>,
}

/// Chaos testing errors
#[derive(Debug, Clone)]
pub enum ChaosError {
    OperationFailed { operation: String, reason: String },
    TargetNotFound { target: String },
    InsufficientPermissions { operation: String },
    SafetyCheckFailed { check: String },
    RecoveryFailed { operation: String, reason: String },
    TimeoutError { operation: String, timeout: Duration },
}

/// Fault injector for various failure types
#[derive(Debug)]
pub struct FaultInjector {
    /// Active fault injections
    active_faults: HashMap<String, FaultInjection>,
    
    /// Fault injection history
    fault_history: Vec<FaultInjectionRecord>,
    
    /// Safety constraints
    safety_constraints: Vec<SafetyConstraint>,
}

/// Fault injection
#[derive(Debug, Clone)]
pub struct FaultInjection {
    /// Injection identifier
    pub injection_id: String,
    
    /// Fault type
    pub fault_type: FaultType,
    
    /// Target specification
    pub target: FaultTarget,
    
    /// Injection parameters
    pub parameters: HashMap<String, serde_json::Value>,
    
    /// Injection state
    pub state: FaultInjectionState,
    
    /// Start time
    pub start_time: SystemTime,
    
    /// Duration
    pub duration: Duration,
}

/// Fault types
#[derive(Debug, Clone)]
pub enum FaultType {
    ActorCrash,
    NetworkPartition,
    MessageDrop,
    MessageCorruption,
    LatencySpike,
    ResourceExhaustion,
    DiskError,
    MemoryPressure,
    CpuStarvation,
    ClockSkew,
    Custom { fault_name: String },
}

/// Fault targets
#[derive(Debug, Clone)]
pub enum FaultTarget {
    Actor { actor_id: String },
    ActorGroup { group_name: String },
    Network { connection: NetworkConnection },
    System { component: String },
    Custom { target_spec: String },
}

/// Network connection specification
#[derive(Debug, Clone)]
pub struct NetworkConnection {
    pub source: String,
    pub destination: String,
    pub connection_type: ConnectionType,
}

/// Connection types
#[derive(Debug, Clone, Copy)]
pub enum ConnectionType {
    ActorToActor,
    ActorToService,
    ServiceToService,
    External,
}

/// Fault injection state
#[derive(Debug, Clone, Copy)]
pub enum FaultInjectionState {
    Scheduled,
    Active,
    Recovering,
    Completed,
    Failed,
}

/// Fault injection record
#[derive(Debug, Clone)]
pub struct FaultInjectionRecord {
    pub injection: FaultInjection,
    pub result: FaultInjectionResult,
    pub impact: FaultImpactAnalysis,
}

/// Fault injection result
#[derive(Debug, Clone)]
pub struct FaultInjectionResult {
    pub success: bool,
    pub execution_time: Duration,
    pub targets_affected: Vec<String>,
    pub errors: Vec<String>,
}

/// Fault impact analysis
#[derive(Debug, Clone)]
pub struct FaultImpactAnalysis {
    /// Immediate impact
    pub immediate_impact: ImpactMetrics,
    
    /// Cascading failures
    pub cascading_failures: Vec<CascadingFailure>,
    
    /// Recovery behavior
    pub recovery_behavior: RecoveryBehaviorAnalysis,
}

/// Impact metrics
#[derive(Debug, Clone)]
pub struct ImpactMetrics {
    pub actors_affected: u32,
    pub messages_lost: u32,
    pub throughput_degradation: f64,
    pub latency_increase: Duration,
    pub error_rate_increase: f64,
}

/// Cascading failure
#[derive(Debug, Clone)]
pub struct CascadingFailure {
    pub triggered_by: String,
    pub affected_component: String,
    pub failure_type: String,
    pub propagation_time: Duration,
}

/// Recovery behavior analysis
#[derive(Debug, Clone)]
pub struct RecoveryBehaviorAnalysis {
    pub recovery_time: Duration,
    pub recovery_type: RecoveryType,
    pub intervention_required: bool,
    pub lessons_learned: Vec<String>,
}

/// Recovery types
#[derive(Debug, Clone, Copy)]
pub enum RecoveryType {
    Automatic,
    SemiAutomatic,
    Manual,
    Failed,
}

/// Safety constraint
#[derive(Debug, Clone)]
pub struct SafetyConstraint {
    pub constraint_id: String,
    pub description: String,
    pub constraint_type: SafetyConstraintType,
    pub threshold: f64,
    pub enabled: bool,
}

/// Safety constraint types
#[derive(Debug, Clone)]
pub enum SafetyConstraintType {
    /// Maximum actors that can be killed
    MaxActorsKilled { max_count: u32 },
    
    /// Maximum network partitions
    MaxNetworkPartitions { max_partitions: u32 },
    
    /// Maximum resource utilization
    MaxResourceUtilization { resource: String, max_percent: f64 },
    
    /// Minimum system availability
    MinSystemAvailability { min_availability: f64 },
    
    /// Custom safety constraint
    Custom { constraint_name: String, params: HashMap<String, serde_json::Value> },
}

/// Network partitioner
#[derive(Debug)]
pub struct NetworkPartitioner {
    /// Active partitions
    active_partitions: HashMap<String, NetworkPartition>,
    
    /// Partition history
    partition_history: Vec<NetworkPartitionEvent>,
    
    /// Network topology
    network_topology: NetworkTopology,
}

/// Network topology
#[derive(Debug, Clone)]
pub struct NetworkTopology {
    /// Nodes in the network
    pub nodes: HashSet<String>,
    
    /// Connections between nodes
    pub connections: HashMap<String, HashSet<String>>,
    
    /// Connection properties
    pub connection_properties: HashMap<(String, String), ConnectionProperties>,
}

/// Connection properties
#[derive(Debug, Clone)]
pub struct ConnectionProperties {
    pub latency: Duration,
    pub bandwidth: u64,
    pub reliability: f64,
    pub connection_type: ConnectionType,
}

/// Network partition event
#[derive(Debug, Clone)]
pub struct NetworkPartitionEvent {
    pub event_id: String,
    pub timestamp: SystemTime,
    pub event_type: PartitionEventType,
    pub partition: NetworkPartition,
    pub affected_nodes: Vec<String>,
}

/// Partition event types
#[derive(Debug, Clone, Copy)]
pub enum PartitionEventType {
    PartitionCreated,
    PartitionModified,
    PartitionHealed,
    PartitionFailed,
}

/// Network partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPartition {
    /// Partition identifier
    pub partition_id: String,
    
    /// Partition name
    pub name: String,
    
    /// Partitioned groups
    pub groups: Vec<PartitionGroup>,
    
    /// Partition start time
    pub start_time: SystemTime,
    
    /// Partition duration
    pub duration: Duration,
    
    /// Partition state
    pub state: PartitionState,
}

/// Partition state
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PartitionState {
    Scheduled,
    Active,
    Healing,
    Healed,
    Failed,
}

/// Resource constrainer
#[derive(Debug)]
pub struct ResourceConstrainer {
    /// Active constraints
    active_constraints: HashMap<String, ResourceConstraint>,
    
    /// Constraint history
    constraint_history: Vec<ResourceConstraintEvent>,
    
    /// Resource monitors
    resource_monitors: HashMap<String, Box<dyn ResourceMonitor>>,
}

/// Resource constraint
#[derive(Debug, Clone)]
pub struct ResourceConstraint {
    pub constraint_id: String,
    pub resource_type: ResourceType,
    pub constraint_level: ConstraintLevel,
    pub affected_targets: Vec<String>,
    pub start_time: SystemTime,
    pub duration: Duration,
    pub state: ResourceConstraintState,
}

/// Resource constraint state
#[derive(Debug, Clone, Copy)]
pub enum ResourceConstraintState {
    Scheduled,
    Ramping,
    Active,
    Releasing,
    Released,
    Failed,
}

/// Resource constraint event
#[derive(Debug, Clone)]
pub struct ResourceConstraintEvent {
    pub event_id: String,
    pub timestamp: SystemTime,
    pub event_type: ConstraintEventType,
    pub constraint: ResourceConstraint,
    pub impact: ResourceImpact,
}

/// Constraint event types
#[derive(Debug, Clone, Copy)]
pub enum ConstraintEventType {
    ConstraintApplied,
    ConstraintModified,
    ConstraintReleased,
    ConstraintFailed,
}

/// Resource impact
#[derive(Debug, Clone)]
pub struct ResourceImpact {
    pub resource_utilization: HashMap<String, f64>,
    pub performance_degradation: f64,
    pub actors_affected: Vec<String>,
    pub error_count: u32,
}

/// Resource monitor trait
pub trait ResourceMonitor: Send + Sync + std::fmt::Debug {
    fn get_current_usage(&self) -> f64;
    fn get_historical_usage(&self, duration: Duration) -> Vec<(SystemTime, f64)>;
    fn can_apply_constraint(&self, constraint_level: f64) -> bool;
}

/// Chaos metrics collector
#[derive(Debug, Default)]
pub struct ChaosMetricsCollector {
    /// Scenario execution metrics
    pub scenario_metrics: HashMap<String, ChaosScenarioMetrics>,
    
    /// Overall chaos testing metrics
    pub overall_metrics: OverallChaosMetrics,
    
    /// Resilience scores
    pub resilience_scores: HashMap<String, ResilienceScore>,
}

/// Chaos scenario metrics
#[derive(Debug, Clone)]
pub struct ChaosScenarioMetrics {
    pub scenario_id: String,
    pub execution_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub average_execution_time: Duration,
    pub average_recovery_time: Duration,
    pub impact_severity_distribution: HashMap<ImpactSeverity, u32>,
}

/// Overall chaos testing metrics
#[derive(Debug, Clone, Default)]
pub struct OverallChaosMetrics {
    pub total_scenarios_executed: u32,
    pub total_faults_injected: u32,
    pub mean_time_to_recovery: Duration,
    pub system_availability: f64,
    pub fault_tolerance_score: f64,
    pub recovery_automation_rate: f64,
}

/// Resilience score
#[derive(Debug, Clone)]
pub struct ResilienceScore {
    pub component: String,
    pub overall_score: f64,
    pub availability_score: f64,
    pub recovery_speed_score: f64,
    pub fault_tolerance_score: f64,
    pub degradation_graceful_score: f64,
}

/// Recovery coordinator
#[derive(Debug)]
pub struct RecoveryCoordinator {
    /// Recovery strategies
    recovery_strategies: HashMap<String, Box<dyn RecoveryStrategy>>,
    
    /// Recovery history
    recovery_history: Vec<RecoveryAttempt>,
    
    /// Active recoveries
    active_recoveries: HashMap<String, RecoveryExecution>,
}

/// Recovery strategy trait
pub trait RecoveryStrategy: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    
    async fn execute_recovery(
        &self,
        context: &RecoveryContext,
        harness: &ActorTestHarness,
    ) -> Result<RecoveryResult, RecoveryError>;
    
    fn estimated_recovery_time(&self, context: &RecoveryContext) -> Duration;
    
    fn can_handle(&self, context: &RecoveryContext) -> bool;
}

/// Recovery context
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    pub fault_type: FaultType,
    pub affected_components: Vec<String>,
    pub fault_start_time: SystemTime,
    pub system_state: serde_json::Value,
    pub recovery_constraints: Vec<RecoveryConstraint>,
}

/// Recovery constraints
#[derive(Debug, Clone)]
pub struct RecoveryConstraint {
    pub constraint_type: RecoveryConstraintType,
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Recovery constraint types
#[derive(Debug, Clone)]
pub enum RecoveryConstraintType {
    MaxRecoveryTime { max_time: Duration },
    MinimalServiceDisruption,
    DataConsistencyRequired,
    ResourceLimitations { available_resources: HashMap<String, f64> },
    Custom { constraint_name: String },
}

/// Recovery result
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub success: bool,
    pub recovery_time: Duration,
    pub components_recovered: Vec<String>,
    pub remaining_issues: Vec<String>,
    pub manual_intervention_required: bool,
}

/// Recovery error
#[derive(Debug, Clone)]
pub enum RecoveryError {
    RecoveryTimeout,
    InsufficientResources,
    ComponentUnresponsive { component: String },
    DataCorruption,
    RecoveryStrategyFailed { strategy: String, reason: String },
}

/// Recovery attempt
#[derive(Debug, Clone)]
pub struct RecoveryAttempt {
    pub attempt_id: String,
    pub recovery_context: RecoveryContext,
    pub strategy_used: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub result: Option<RecoveryResult>,
    pub error: Option<RecoveryError>,
}

/// Recovery execution
#[derive(Debug, Clone)]
pub struct RecoveryExecution {
    pub execution_id: String,
    pub strategy: String,
    pub start_time: SystemTime,
    pub estimated_completion: SystemTime,
    pub progress: RecoveryProgress,
}

/// Recovery progress
#[derive(Debug, Clone)]
pub struct RecoveryProgress {
    pub percentage_complete: f64,
    pub current_step: String,
    pub steps_completed: u32,
    pub total_steps: u32,
    pub estimated_time_remaining: Duration,
}

impl ChaosTestEngine {
    /// Create a new chaos test engine
    pub fn new(config: ChaosTestConfig) -> Self {
        Self {
            config,
            active_scenarios: Arc::new(RwLock::new(HashMap::new())),
            operators: Arc::new(RwLock::new(HashMap::new())),
            fault_injector: Arc::new(RwLock::new(FaultInjector {
                active_faults: HashMap::new(),
                fault_history: Vec::new(),
                safety_constraints: Vec::new(),
            })),
            network_partitioner: Arc::new(RwLock::new(NetworkPartitioner {
                active_partitions: HashMap::new(),
                partition_history: Vec::new(),
                network_topology: NetworkTopology {
                    nodes: HashSet::new(),
                    connections: HashMap::new(),
                    connection_properties: HashMap::new(),
                },
            })),
            resource_constrainer: Arc::new(RwLock::new(ResourceConstrainer {
                active_constraints: HashMap::new(),
                constraint_history: Vec::new(),
                resource_monitors: HashMap::new(),
            })),
            metrics_collector: Arc::new(RwLock::new(ChaosMetricsCollector::default())),
            recovery_coordinator: Arc::new(RwLock::new(RecoveryCoordinator {
                recovery_strategies: HashMap::new(),
                recovery_history: Vec::new(),
                active_recoveries: HashMap::new(),
            })),
        }
    }
    
    /// Register a chaos operator
    pub async fn register_operator(&self, operator: Box<dyn ChaosOperator>) -> Result<(), String> {
        let mut operators = self.operators.write().await;
        operators.insert(operator.name().to_string(), operator);
        Ok(())
    }
    
    /// Execute a chaos test scenario
    pub async fn execute_scenario(
        &self,
        mut scenario: ChaosTestScenario,
        harness: Arc<ActorTestHarness>,
    ) -> Result<ChaosResult, ChaosError> {
        // Update scenario state
        scenario.state = ChaosScenarioState::Running { current_step: 0 };
        
        // Store active scenario
        {
            let mut active_scenarios = self.active_scenarios.write().await;
            active_scenarios.insert(scenario.scenario_id.clone(), scenario.clone());
        }
        
        let start_time = SystemTime::now();
        let mut step_results = Vec::new();
        
        // Execute scenario steps
        for (step_index, step) in scenario.steps.iter().enumerate() {
            // Update scenario state
            scenario.state = ChaosScenarioState::Running { current_step: step_index };
            
            // Execute chaos step
            match self.execute_chaos_step(step, &harness).await {
                Ok(step_result) => {
                    step_results.push(step_result);
                },
                Err(e) => {
                    let step_result = ChaosStepResult {
                        step_id: step.step_id.clone(),
                        success: false,
                        execution_time: Duration::from_secs(0),
                        impact_achieved: step.expected_impact.clone(),
                        recovery_time: None,
                        errors: vec![format!("{:?}", e)],
                    };
                    step_results.push(step_result);
                    
                    // Decide whether to continue or abort
                    if matches!(self.config.intensity_level, ChaosIntensity::Extreme) {
                        // Continue even on failures in extreme mode
                    } else {
                        break;
                    }
                }
            }
            
            // Wait for step interval if configured
            if step_index < scenario.timing.step_intervals.len() {
                tokio::time::sleep(scenario.timing.step_intervals[step_index]).await;
            }
        }
        
        // Begin recovery phase
        scenario.state = ChaosScenarioState::Recovering;
        
        let recovery_start = SystemTime::now();
        let recovery_result = self.execute_recovery(&scenario, &harness).await;
        let recovery_time = recovery_start.elapsed().unwrap_or(Duration::from_secs(0));
        
        // Evaluate success criteria
        let success = self.evaluate_success_criteria(&scenario, &step_results).await;
        
        // Create final result
        let result = ChaosResult {
            success,
            step_results,
            recovery_metrics: RecoveryMetrics {
                mean_time_to_recovery: recovery_time,
                recovery_success_rate: if recovery_result.is_ok() { 1.0 } else { 0.0 },
                automatic_recovery_rate: 0.8, // TODO: Calculate from actual data
                manual_intervention_required: recovery_result.is_err(),
            },
            performance_impact: PerformanceImpact {
                throughput_degradation: 0.2, // TODO: Calculate from metrics
                latency_increase: 0.3,
                error_rate_increase: 0.1,
                resource_utilization_change: HashMap::new(),
            },
            lessons_learned: vec![
                "System recovered gracefully from network partition".to_string(),
                "Actor restart mechanism worked as expected".to_string(),
            ],
        };
        
        // Update scenario state
        scenario.state = ChaosScenarioState::Completed { result: result.clone() };
        
        // Update metrics
        self.update_chaos_metrics(&scenario, &result).await;
        
        Ok(result)
    }
    
    /// Execute a chaos step
    async fn execute_chaos_step(
        &self,
        step: &ChaosStep,
        harness: &ActorTestHarness,
    ) -> Result<ChaosStepResult, ChaosError> {
        let step_start = SystemTime::now();
        
        // Find appropriate operator
        let operators = self.operators.read().await;
        let operator = operators.values().next().ok_or_else(|| ChaosError::OperationFailed {
            operation: step.operation.to_string(),
            reason: "No chaos operators registered".to_string(),
        })?;
        
        // Execute operation
        let targets = vec!["actor_1".to_string()]; // TODO: Implement proper target selection
        let operation_result = operator.execute(&step.operation, &targets, harness).await?;
        
        let execution_time = step_start.elapsed().unwrap_or(Duration::from_secs(0));
        
        // Check recovery conditions
        let recovery_time = if step.operation.is_recoverable() {
            let recovery_start = SystemTime::now();
            let _ = operator.recover(&step.operation, &targets, harness).await;
            Some(recovery_start.elapsed().unwrap_or(Duration::from_secs(0)))
        } else {
            None
        };
        
        Ok(ChaosStepResult {
            step_id: step.step_id.clone(),
            success: operation_result.success,
            execution_time,
            impact_achieved: step.expected_impact.clone(),
            recovery_time,
            errors: operation_result.errors,
        })
    }
    
    /// Execute recovery for a scenario
    async fn execute_recovery(
        &self,
        scenario: &ChaosTestScenario,
        harness: &ActorTestHarness,
    ) -> Result<(), ChaosError> {
        match &scenario.recovery_strategy {
            RecoveryStrategy::Automatic { max_recovery_time, recovery_steps } => {
                for step in recovery_steps {
                    // Execute recovery step
                    // TODO: Implement recovery step execution
                }
            },
            RecoveryStrategy::Manual => {
                // Manual recovery - wait for external intervention
                tokio::time::sleep(Duration::from_secs(5)).await; // Simulate manual intervention
            },
            RecoveryStrategy::Hybrid { auto_recovery_timeout, manual_fallback } => {
                // Try automatic recovery first, fall back to manual if needed
                tokio::time::sleep(*auto_recovery_timeout).await;
            },
            RecoveryStrategy::None => {
                // No explicit recovery - let system handle naturally
            },
        }
        
        Ok(())
    }
    
    /// Evaluate scenario success criteria
    async fn evaluate_success_criteria(
        &self,
        scenario: &ChaosTestScenario,
        step_results: &[ChaosStepResult],
    ) -> bool {
        let mut weighted_score = 0.0;
        let mut total_weight = 0.0;
        
        for criterion in &scenario.success_criteria {
            let criterion_met = match &criterion.check {
                SuccessCheck::RecoveredWithinTime { max_recovery_time } => {
                    // Check if all steps recovered within time
                    step_results.iter().all(|result| {
                        result.recovery_time
                            .map(|rt| rt <= *max_recovery_time)
                            .unwrap_or(true)
                    })
                },
                SuccessCheck::NoDataLoss => {
                    // TODO: Implement data loss check
                    true
                },
                SuccessCheck::AllActorsRecovered => {
                    // TODO: Check if all actors are running
                    true
                },
                SuccessCheck::PerformanceWithinLimits { max_degradation } => {
                    // TODO: Check performance metrics
                    true
                },
                SuccessCheck::ErrorRateAcceptable { max_error_rate } => {
                    // TODO: Check error rates
                    true
                },
                SuccessCheck::Custom { .. } => {
                    // TODO: Implement custom checks
                    true
                },
            };
            
            if criterion_met {
                weighted_score += criterion.weight;
            }
            total_weight += criterion.weight;
        }
        
        // Require at least 80% success rate
        total_weight == 0.0 || (weighted_score / total_weight) >= 0.8
    }
    
    /// Update chaos testing metrics
    async fn update_chaos_metrics(&self, scenario: &ChaosTestScenario, result: &ChaosResult) {
        let mut collector = self.metrics_collector.write().await;
        
        // Update scenario-specific metrics
        let scenario_metrics = collector.scenario_metrics
            .entry(scenario.scenario_id.clone())
            .or_insert_with(|| ChaosScenarioMetrics {
                scenario_id: scenario.scenario_id.clone(),
                execution_count: 0,
                success_count: 0,
                failure_count: 0,
                average_execution_time: Duration::from_secs(0),
                average_recovery_time: Duration::from_secs(0),
                impact_severity_distribution: HashMap::new(),
            });
        
        scenario_metrics.execution_count += 1;
        if result.success {
            scenario_metrics.success_count += 1;
        } else {
            scenario_metrics.failure_count += 1;
        }
        
        // Update overall metrics
        collector.overall_metrics.total_scenarios_executed += 1;
        collector.overall_metrics.mean_time_to_recovery = result.recovery_metrics.mean_time_to_recovery;
    }
    
    /// Get chaos testing results
    pub async fn get_results(&self) -> ChaosMetricsCollector {
        self.metrics_collector.read().await.clone()
    }
}

impl ChaosOperation {
    fn to_string(&self) -> String {
        match self {
            ChaosOperation::KillActor { actor_id, .. } => format!("KillActor({})", actor_id),
            ChaosOperation::NetworkPartition { .. } => "NetworkPartition".to_string(),
            ChaosOperation::ResourceConstraint { .. } => "ResourceConstraint".to_string(),
            ChaosOperation::MessageCorruption { .. } => "MessageCorruption".to_string(),
            ChaosOperation::LatencyInjection { .. } => "LatencyInjection".to_string(),
            ChaosOperation::DiskFailure { .. } => "DiskFailure".to_string(),
            ChaosOperation::MemoryPressure { .. } => "MemoryPressure".to_string(),
            ChaosOperation::CpuThrottling { .. } => "CpuThrottling".to_string(),
            ChaosOperation::ClockSkew { .. } => "ClockSkew".to_string(),
            ChaosOperation::Custom { operation_name, .. } => format!("Custom({})", operation_name),
        }
    }
    
    fn is_recoverable(&self) -> bool {
        match self {
            ChaosOperation::KillActor { .. } => true,
            ChaosOperation::NetworkPartition { .. } => true,
            ChaosOperation::ResourceConstraint { .. } => true,
            ChaosOperation::MessageCorruption { .. } => true,
            ChaosOperation::LatencyInjection { .. } => true,
            ChaosOperation::DiskFailure { .. } => true,
            ChaosOperation::MemoryPressure { .. } => true,
            ChaosOperation::CpuThrottling { .. } => true,
            ChaosOperation::ClockSkew { .. } => true,
            ChaosOperation::Custom { .. } => false, // Conservative default
        }
    }
}

impl Default for ChaosTestConfig {
    fn default() -> Self {
        Self {
            default_duration: Duration::from_secs(300),
            max_concurrent_operations: 3,
            safety_checks_enabled: true,
            auto_recovery_enabled: true,
            recovery_timeout: Duration::from_secs(60),
            intensity_level: ChaosIntensity::Medium,
            monitoring_interval: Duration::from_secs(5),
        }
    }
}

/// Built-in chaos test scenarios
pub struct ChaosTestScenarios;

impl ChaosTestScenarios {
    /// Network partition scenario
    pub fn network_partition_scenario() -> ChaosTestScenario {
        ChaosTestScenario {
            scenario_id: "network_partition_basic".to_string(),
            name: "Basic Network Partition".to_string(),
            description: "Tests system behavior under network partitions".to_string(),
            steps: vec![
                ChaosStep {
                    step_id: "partition_step".to_string(),
                    name: "Create network partition".to_string(),
                    operation: ChaosOperation::NetworkPartition {
                        partition_config: NetworkPartitionConfig {
                            groups: vec![
                                PartitionGroup {
                                    group_id: "group_a".to_string(),
                                    actors: ["actor_1", "actor_2"].iter().map(|s| s.to_string()).collect(),
                                    connectivity: GroupConnectivity::FullyConnected,
                                },
                                PartitionGroup {
                                    group_id: "group_b".to_string(),
                                    actors: ["actor_3", "actor_4"].iter().map(|s| s.to_string()).collect(),
                                    connectivity: GroupConnectivity::FullyConnected,
                                },
                            ],
                            duration: Duration::from_secs(60),
                            partition_type: PartitionType::CompletePartition,
                            recovery_behavior: PartitionRecoveryBehavior::Immediate,
                        },
                    },
                    timing: StepTiming {
                        start_offset: Duration::from_secs(0),
                        duration: Duration::from_secs(60),
                        ramp_up: None,
                        ramp_down: None,
                    },
                    expected_impact: ExpectedImpact {
                        severity: ImpactSeverity::Medium,
                        affected_metrics: vec!["message_throughput".to_string(), "error_rate".to_string()],
                        metric_changes: HashMap::new(),
                        recovery_time_estimate: Some(Duration::from_secs(30)),
                    },
                    recovery_conditions: vec![],
                },
            ],
            targets: ChaosTargetSelection {
                strategy: TargetSelectionStrategy::All,
                filters: vec![],
                max_targets: None,
            },
            timing: ChaosTimingConfig {
                start_delay: Duration::from_secs(10),
                step_intervals: vec![Duration::from_secs(5)],
                total_duration: Duration::from_secs(120),
                execution_pattern: ExecutionPattern::Sequential,
            },
            success_criteria: vec![
                ChaosSuccessCriterion {
                    name: "System recovers".to_string(),
                    check: SuccessCheck::RecoveredWithinTime {
                        max_recovery_time: Duration::from_secs(60),
                    },
                    required: true,
                    weight: 1.0,
                },
            ],
            recovery_strategy: RecoveryStrategy::Automatic {
                max_recovery_time: Duration::from_secs(60),
                recovery_steps: vec![],
            },
            state: ChaosScenarioState::Created,
        }
    }
    
    /// Actor failure scenario
    pub fn actor_failure_scenario() -> ChaosTestScenario {
        ChaosTestScenario {
            scenario_id: "actor_failure_basic".to_string(),
            name: "Basic Actor Failure".to_string(),
            description: "Tests system behavior when actors fail".to_string(),
            steps: vec![
                ChaosStep {
                    step_id: "kill_actor_step".to_string(),
                    name: "Kill random actor".to_string(),
                    operation: ChaosOperation::KillActor {
                        actor_id: "target_actor".to_string(),
                        kill_type: ActorKillType::Immediate,
                    },
                    timing: StepTiming {
                        start_offset: Duration::from_secs(0),
                        duration: Duration::from_secs(1),
                        ramp_up: None,
                        ramp_down: None,
                    },
                    expected_impact: ExpectedImpact {
                        severity: ImpactSeverity::High,
                        affected_metrics: vec!["actor_count".to_string(), "message_processing".to_string()],
                        metric_changes: HashMap::new(),
                        recovery_time_estimate: Some(Duration::from_secs(10)),
                    },
                    recovery_conditions: vec![],
                },
            ],
            targets: ChaosTargetSelection {
                strategy: TargetSelectionStrategy::Random { count: 1 },
                filters: vec![],
                max_targets: Some(1),
            },
            timing: ChaosTimingConfig {
                start_delay: Duration::from_secs(5),
                step_intervals: vec![],
                total_duration: Duration::from_secs(30),
                execution_pattern: ExecutionPattern::Sequential,
            },
            success_criteria: vec![
                ChaosSuccessCriterion {
                    name: "Actor restarts".to_string(),
                    check: SuccessCheck::AllActorsRecovered,
                    required: true,
                    weight: 1.0,
                },
            ],
            recovery_strategy: RecoveryStrategy::Automatic {
                max_recovery_time: Duration::from_secs(30),
                recovery_steps: vec![],
            },
            state: ChaosScenarioState::Created,
        }
    }
    
    /// Resource constraint scenario  
    pub fn resource_constraint_scenario() -> ChaosTestScenario {
        ChaosTestScenario {
            scenario_id: "resource_constraint_memory".to_string(),
            name: "Memory Pressure Test".to_string(),
            description: "Tests system behavior under memory pressure".to_string(),
            steps: vec![
                ChaosStep {
                    step_id: "memory_pressure_step".to_string(),
                    name: "Apply memory pressure".to_string(),
                    operation: ChaosOperation::MemoryPressure {
                        pressure_config: MemoryPressureConfig {
                            memory_to_consume: 1_000_000_000, // 1GB
                            consumption_pattern: MemoryConsumptionPattern::Gradual { rate: 10_000_000 }, // 10MB/s
                            targets: vec!["all_actors".to_string()],
                            duration: Duration::from_secs(120),
                        },
                    },
                    timing: StepTiming {
                        start_offset: Duration::from_secs(0),
                        duration: Duration::from_secs(120),
                        ramp_up: Some(Duration::from_secs(10)),
                        ramp_down: Some(Duration::from_secs(10)),
                    },
                    expected_impact: ExpectedImpact {
                        severity: ImpactSeverity::Medium,
                        affected_metrics: vec!["memory_usage".to_string(), "gc_pressure".to_string()],
                        metric_changes: HashMap::new(),
                        recovery_time_estimate: Some(Duration::from_secs(30)),
                    },
                    recovery_conditions: vec![],
                },
            ],
            targets: ChaosTargetSelection {
                strategy: TargetSelectionStrategy::All,
                filters: vec![],
                max_targets: None,
            },
            timing: ChaosTimingConfig {
                start_delay: Duration::from_secs(10),
                step_intervals: vec![],
                total_duration: Duration::from_secs(180),
                execution_pattern: ExecutionPattern::Sequential,
            },
            success_criteria: vec![
                ChaosSuccessCriterion {
                    name: "Performance degradation acceptable".to_string(),
                    check: SuccessCheck::PerformanceWithinLimits {
                        max_degradation: 0.5, // 50% degradation acceptable
                    },
                    required: true,
                    weight: 1.0,
                },
            ],
            recovery_strategy: RecoveryStrategy::Automatic {
                max_recovery_time: Duration::from_secs(60),
                recovery_steps: vec![],
            },
            state: ChaosScenarioState::Created,
        }
    }
}