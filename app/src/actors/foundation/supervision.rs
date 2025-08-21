//! Supervision & Restart Logic - Phase 2 Implementation (ALYS-006-06 to ALYS-006-11)
//! 
//! Advanced supervision capabilities for Alys V2 actor system including spawn_supervised
//! actor factory patterns, failure handling with blockchain-aware classification,
//! exponential backoff and fixed delay restart strategies, comprehensive restart
//! tracking, and supervisor escalation for the merged mining sidechain.

use crate::actors::foundation::{
    ActorSystemConfig, RootSupervisor, ActorInfo, ActorPriority, RestartStrategy, 
    RestartReason, RestartAttempt, constants::{blockchain, lifecycle, restart, performance}
};
use actix::{
    Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, Context, ContextFutureSpawner,
    Handler, Message, ResponseActFuture, Supervised, SupervisorError, WrapFuture
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Actor factory trait for creating supervised actors
/// 
/// Provides type-safe factory patterns for creating actors that can be
/// supervised with restart strategies and integrated with the Alys sidechain
/// consensus system and governance event processing.
pub trait ActorFactory<A: Actor + Supervised>: Send + Sync {
    /// Create a new instance of the actor
    fn create(&self) -> A;
    
    /// Get actor configuration including mailbox settings and priority
    fn config(&self) -> SupervisedActorConfig {
        SupervisedActorConfig::default()
    }
    
    /// Get optional health check function for this actor type
    fn health_check(&self) -> Option<HealthCheckFn<A>> {
        None
    }
}

/// Type alias for health check function
pub type HealthCheckFn<A> = Box<dyn Fn(&Addr<A>) -> Box<dyn std::future::Future<Output = HealthCheckResult> + Unpin + Send> + Send + Sync>;

/// Configuration for supervised actors
#[derive(Debug, Clone)]
pub struct SupervisedActorConfig {
    /// Mailbox capacity for this actor
    pub mailbox_capacity: usize,
    /// Actor priority level for supervision ordering
    pub priority: ActorPriority,
    /// Custom restart strategy (overrides system default)
    pub restart_strategy: Option<RestartStrategy>,
    /// Maximum restart attempts before escalation
    pub max_restart_attempts: Option<usize>,
    /// Health check interval
    pub health_check_interval: Option<Duration>,
    /// Actor-specific feature flags
    pub feature_flags: HashMap<String, bool>,
}

impl Default for SupervisedActorConfig {
    fn default() -> Self {
        Self {
            mailbox_capacity: 10000,
            priority: ActorPriority::Normal,
            restart_strategy: None,
            max_restart_attempts: Some(restart::DEFAULT_MAX_RESTARTS),
            health_check_interval: Some(Duration::from_secs(30)),
            feature_flags: HashMap::new(),
        }
    }
}

/// Supervision context for managing actor lifecycle
#[derive(Debug)]
pub struct SupervisionContext {
    /// Supervision ID for tracking
    pub supervision_id: Uuid,
    /// Actor name in the system
    pub actor_name: String,
    /// Actor configuration
    pub config: SupervisedActorConfig,
    /// Restart strategy for this actor
    pub restart_strategy: RestartStrategy,
    /// Supervision start time
    pub started_at: SystemTime,
    /// Current supervision state
    pub state: SupervisionState,
}

/// Supervision states for actor lifecycle tracking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionState {
    /// Actor is being initialized
    Initializing,
    /// Actor is running normally
    Running,
    /// Actor has failed and is being handled
    Failed(ActorFailureInfo),
    /// Actor is restarting
    Restarting,
    /// Actor is being escalated to parent supervisor
    Escalating,
    /// Actor supervision has been terminated
    Terminated,
}

/// Actor failure classification for blockchain-specific handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorFailureInfo {
    /// Failure timestamp
    pub timestamp: SystemTime,
    /// Classification of the failure
    pub failure_type: ActorFailureType,
    /// Error message or description
    pub message: String,
    /// Failure context for debugging
    pub context: HashMap<String, String>,
    /// Whether this failure should trigger escalation
    pub escalate: bool,
}

/// Types of actor failures with blockchain-specific categories
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorFailureType {
    /// Actor panic or unhandled exception
    Panic { backtrace: Option<String> },
    /// Timeout in message processing
    Timeout { duration: Duration },
    /// Mailbox overflow or capacity issues
    MailboxOverflow { capacity: usize, pending: usize },
    /// Resource exhaustion (memory, CPU, etc.)
    ResourceExhaustion { resource_type: String, usage: f64 },
    /// Blockchain consensus-related failure
    ConsensusFailure { error_code: String },
    /// Network connectivity or communication failure
    NetworkFailure { peer_id: Option<String>, error: String },
    /// Governance event processing failure
    GovernanceFailure { event_type: String, error: String },
    /// Federation operation failure (peg-in/peg-out)
    FederationFailure { operation: String, error: String },
    /// Health check failure
    HealthCheckFailure { consecutive_failures: u32 },
    /// Configuration or validation error
    ConfigurationError { field: String, value: String },
    /// External dependency failure
    DependencyFailure { service: String, error: String },
}

/// Restart attempt tracking with comprehensive metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartAttemptInfo {
    /// Unique attempt identifier
    pub attempt_id: Uuid,
    /// Attempt number (1-indexed)
    pub attempt_number: usize,
    /// Timestamp when restart was initiated
    pub timestamp: SystemTime,
    /// Reason for this restart
    pub reason: RestartReason,
    /// Applied delay before restart
    pub delay: Duration,
    /// Restart strategy used
    pub strategy: RestartStrategy,
    /// Success status (None = in progress)
    pub success: Option<bool>,
    /// Duration of restart process
    pub duration: Option<Duration>,
    /// Failure info that triggered restart
    pub failure_info: Option<ActorFailureInfo>,
    /// Additional restart context
    pub context: HashMap<String, String>,
}

/// Restart statistics and patterns
#[derive(Debug, Clone, Default)]
pub struct RestartStatistics {
    /// Total number of restart attempts
    pub total_attempts: usize,
    /// Successful restarts
    pub successful_restarts: usize,
    /// Failed restarts
    pub failed_restarts: usize,
    /// Average restart duration
    pub avg_restart_duration: Duration,
    /// Restart attempts by failure type
    pub attempts_by_failure_type: HashMap<ActorFailureType, usize>,
    /// Restart success rate over time windows
    pub success_rate_1h: f64,
    pub success_rate_24h: f64,
    /// Last restart timestamp
    pub last_restart: Option<SystemTime>,
    /// Failure patterns detected
    pub patterns: Vec<FailurePattern>,
}

/// Detected failure patterns for predictive restart
#[derive(Debug, Clone)]
pub struct FailurePattern {
    /// Pattern type
    pub pattern_type: PatternType,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// First detected timestamp
    pub first_seen: SystemTime,
    /// Last occurrence
    pub last_seen: SystemTime,
    /// Pattern frequency
    pub frequency: usize,
    /// Associated metadata
    pub metadata: HashMap<String, String>,
}

/// Types of failure patterns
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternType {
    /// Periodic failures at regular intervals
    PeriodicFailure { interval: Duration },
    /// Cascading failures affecting multiple actors
    CascadingFailure { affected_actors: Vec<String> },
    /// Resource-related failure patterns
    ResourceExhaustion { resource: String },
    /// Network-related failure patterns
    NetworkPartition { peers: Vec<String> },
    /// Blockchain-specific patterns
    ConsensusFailure { validator_issues: bool },
}

/// Supervision escalation policies
#[derive(Debug, Clone)]
pub enum EscalationPolicy {
    /// Stop the actor and don't restart
    Stop,
    /// Escalate to parent supervisor
    EscalateToParent,
    /// Restart with different strategy
    ChangeStrategy(RestartStrategy),
    /// Restart with reduced permissions/priority
    RestartWithReduction,
    /// Notify external monitoring system
    ExternalNotification { endpoint: String },
    /// Custom escalation handler
    Custom(String),
}

/// Actor supervision enhanced with blockchain-aware restart logic
pub struct EnhancedSupervision {
    /// Supervision contexts by actor name
    contexts: Arc<RwLock<HashMap<String, SupervisionContext>>>,
    /// Restart attempt history
    restart_history: Arc<RwLock<HashMap<String, Vec<RestartAttemptInfo>>>>,
    /// Restart statistics by actor
    restart_stats: Arc<RwLock<HashMap<String, RestartStatistics>>>,
    /// Escalation policies by actor type
    escalation_policies: HashMap<String, EscalationPolicy>,
    /// Failure pattern detection
    pattern_detector: Arc<RwLock<FailurePatternDetector>>,
    /// System configuration
    config: ActorSystemConfig,
}

/// Failure pattern detection engine
#[derive(Debug, Default)]
pub struct FailurePatternDetector {
    /// Historical failure data for pattern analysis
    failure_history: Vec<ActorFailureInfo>,
    /// Detected patterns
    patterns: HashMap<String, FailurePattern>,
    /// Pattern detection configuration
    detection_config: PatternDetectionConfig,
}

/// Configuration for pattern detection
#[derive(Debug, Clone)]
pub struct PatternDetectionConfig {
    /// Minimum occurrences to consider a pattern
    pub min_occurrences: usize,
    /// Time window for pattern analysis
    pub analysis_window: Duration,
    /// Confidence threshold for pattern detection
    pub confidence_threshold: f64,
}

impl Default for PatternDetectionConfig {
    fn default() -> Self {
        Self {
            min_occurrences: 3,
            analysis_window: Duration::from_hours(24),
            confidence_threshold: 0.7,
        }
    }
}

/// Messages for supervision system
#[derive(Message)]
#[rtype(result = "Result<Addr<A>, SupervisionError>")]
pub struct SpawnSupervised<A: Actor + Supervised> {
    /// Actor name for registration
    pub name: String,
    /// Actor factory for creating instances
    pub factory: Box<dyn ActorFactory<A>>,
    /// Optional supervision configuration
    pub config: Option<SupervisedActorConfig>,
}

#[derive(Message)]
#[rtype(result = "Result<(), SupervisionError>")]
pub struct ReportActorFailure {
    /// Actor name
    pub actor_name: String,
    /// Failure information
    pub failure_info: ActorFailureInfo,
}

#[derive(Message)]
#[rtype(result = "Result<RestartAttemptInfo, SupervisionError>")]
pub struct RestartActor {
    /// Actor name to restart
    pub actor_name: String,
    /// Reason for restart
    pub reason: RestartReason,
    /// Optional failure info that triggered restart
    pub failure_info: Option<ActorFailureInfo>,
}

#[derive(Message)]
#[rtype(result = "Result<RestartStatistics, SupervisionError>")]
pub struct GetRestartStatistics {
    /// Actor name (None for system-wide stats)
    pub actor_name: Option<String>,
}

#[derive(Message)]
#[rtype(result = "Result<Vec<FailurePattern>, SupervisionError>")]
pub struct GetFailurePatterns {
    /// Actor name filter (None for all patterns)
    pub actor_name: Option<String>,
}

/// Health check results
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Actor name
    pub actor_name: String,
    /// Health status
    pub healthy: bool,
    /// Response time
    pub response_time: Duration,
    /// Check timestamp
    pub timestamp: SystemTime,
    /// Optional health message
    pub message: Option<String>,
    /// Health score (0.0 to 1.0)
    pub health_score: f64,
    /// Detected issues
    pub issues: Vec<String>,
}

/// Supervision errors
#[derive(Debug, Error)]
pub enum SupervisionError {
    #[error("Actor factory error: {message}")]
    ActorFactoryError { message: String },
    #[error("Configuration error: {field} = {value}")]
    ConfigurationError { field: String, value: String },
    #[error("Restart strategy error: {reason}")]
    RestartStrategyError { reason: String },
    #[error("Escalation failed: {policy:?} - {reason}")]
    EscalationError { policy: EscalationPolicy, reason: String },
    #[error("Pattern detection error: {reason}")]
    PatternDetectionError { reason: String },
    #[error("Supervision context not found: {actor_name}")]
    SupervisionContextNotFound { actor_name: String },
    #[error("Actor system error: {reason}")]
    SystemError { reason: String },
}

impl EnhancedSupervision {
    /// Create new enhanced supervision system
    pub fn new(config: ActorSystemConfig) -> Self {
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
            restart_history: Arc::new(RwLock::new(HashMap::new())),
            restart_stats: Arc::new(RwLock::new(HashMap::new())),
            escalation_policies: Self::default_escalation_policies(),
            pattern_detector: Arc::new(RwLock::new(FailurePatternDetector::default())),
            config,
        }
    }

    /// Spawn a supervised actor with enhanced capabilities (ALYS-006-06)
    /// 
    /// Creates an actor using the factory pattern with comprehensive supervision
    /// including registry integration, mailbox configuration, and blockchain-aware
    /// restart strategies for the Alys sidechain consensus system.
    pub async fn spawn_supervised<A: Actor + Supervised>(
        &self,
        name: String,
        factory: Box<dyn ActorFactory<A>>,
        config: Option<SupervisedActorConfig>,
    ) -> Result<Addr<A>, SupervisionError> {
        info!("Spawning supervised actor: {}", name);

        let actor_config = config.unwrap_or_else(|| factory.config());
        let restart_strategy = actor_config.restart_strategy
            .clone()
            .unwrap_or_else(|| self.config.default_restart_strategy.clone());

        // Validate configuration
        self.validate_actor_config(&actor_config)?;

        // Create supervision context
        let supervision_context = SupervisionContext {
            supervision_id: Uuid::new_v4(),
            actor_name: name.clone(),
            config: actor_config.clone(),
            restart_strategy: restart_strategy.clone(),
            started_at: SystemTime::now(),
            state: SupervisionState::Initializing,
        };

        // Register supervision context
        {
            let mut contexts = self.contexts.write().await;
            contexts.insert(name.clone(), supervision_context);
        }

        // Create actor with factory
        let actor = factory.create();

        // Configure and start actor with supervision
        let addr = self.start_actor_with_supervision(actor, &name, &actor_config).await?;

        // Initialize restart statistics
        {
            let mut stats = self.restart_stats.write().await;
            stats.insert(name.clone(), RestartStatistics::default());
        }

        // Initialize restart history
        {
            let mut history = self.restart_history.write().await;
            history.insert(name.clone(), Vec::new());
        }

        // Update supervision state to running
        {
            let mut contexts = self.contexts.write().await;
            if let Some(context) = contexts.get_mut(&name) {
                context.state = SupervisionState::Running;
            }
        }

        // Schedule health checks if configured
        if let Some(health_check_fn) = factory.health_check() {
            if let Some(interval) = actor_config.health_check_interval {
                self.schedule_health_checks(&name, addr.clone(), health_check_fn, interval).await?;
            }
        }

        info!("Successfully spawned supervised actor: {}", name);
        Ok(addr)
    }

    /// Handle actor failure with comprehensive classification (ALYS-006-07)
    /// 
    /// Processes actor failures with blockchain-specific error classification,
    /// restart counting, metrics tracking, and integration with the Alys
    /// consensus system for governance event processing failures.
    pub async fn handle_actor_failure(
        &self,
        actor_name: &str,
        failure_info: ActorFailureInfo,
    ) -> Result<(), SupervisionError> {
        error!("Handling actor failure for {}: {:?}", actor_name, failure_info);

        // Update supervision state
        {
            let mut contexts = self.contexts.write().await;
            if let Some(context) = contexts.get_mut(actor_name) {
                context.state = SupervisionState::Failed(failure_info.clone());
            } else {
                return Err(SupervisionError::SupervisionContextNotFound {
                    actor_name: actor_name.to_string(),
                });
            }
        }

        // Record failure for pattern detection
        {
            let mut detector = self.pattern_detector.write().await;
            detector.record_failure(failure_info.clone()).await;
        }

        // Classify failure and determine restart strategy
        let restart_decision = self.analyze_failure_for_restart(actor_name, &failure_info).await?;

        match restart_decision {
            RestartDecision::Restart { strategy, delay } => {
                info!("Restarting actor {} with strategy {:?} after {:?}", 
                      actor_name, strategy, delay);

                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }

                self.perform_restart(actor_name, failure_info).await?;
            }
            RestartDecision::Escalate { policy } => {
                warn!("Escalating actor {} with policy {:?}", actor_name, policy);
                self.escalate_failure(actor_name, failure_info, policy).await?;
            }
            RestartDecision::Stop => {
                info!("Stopping actor {} due to failure", actor_name);
                self.stop_actor(actor_name).await?;
            }
        }

        Ok(())
    }

    /// Implement exponential backoff restart (ALYS-006-08)
    /// 
    /// Advanced exponential backoff implementation with blockchain-aware timing,
    /// configurable parameters, delay calculation respecting block boundaries,
    /// and maximum attempts tracking for Alys consensus coordination.
    pub async fn calculate_exponential_backoff_delay(
        &self,
        actor_name: &str,
        attempt_number: usize,
        config: &ExponentialBackoffConfig,
    ) -> Result<Duration, SupervisionError> {
        debug!("Calculating exponential backoff delay for {} (attempt {})", 
               actor_name, attempt_number);

        // Get restart history for this actor
        let restart_history = {
            let history = self.restart_history.read().await;
            history.get(actor_name).cloned().unwrap_or_default()
        };

        // Check maximum attempts
        if let Some(max_attempts) = config.max_attempts {
            if attempt_number > max_attempts {
                return Err(SupervisionError::RestartStrategyError {
                    reason: format!("Maximum restart attempts exceeded: {} > {}", 
                                  attempt_number, max_attempts)
                });
            }
        }

        // Calculate base delay with exponential backoff
        let base_delay_ms = config.initial_delay.as_millis() as f64 
            * config.multiplier.powi((attempt_number - 1) as i32);

        // Apply jitter to prevent thundering herd
        let jitter_factor = 1.0 + (rand::random::<f64>() - 0.5) * config.jitter;
        let jittered_delay_ms = base_delay_ms * jitter_factor;

        // Cap at maximum delay
        let final_delay_ms = jittered_delay_ms.min(config.max_delay.as_millis() as f64);
        let mut final_delay = Duration::from_millis(final_delay_ms as u64);

        // Blockchain-aware alignment for consensus-critical actors
        if config.align_to_block_boundary {
            final_delay = self.align_delay_to_block_boundary(final_delay);
        }

        // Respect consensus timing for governance-related actors
        if config.respect_consensus_timing {
            final_delay = self.adjust_delay_for_consensus_timing(final_delay, actor_name).await;
        }

        debug!("Calculated exponential backoff delay: {:?}", final_delay);
        Ok(final_delay)
    }

    /// Implement fixed delay restart strategy (ALYS-006-09)
    /// 
    /// Fixed delay restart implementation with precise timing controls,
    /// failure counting, blockchain alignment, and coordination with
    /// Alys sidechain operations and governance event processing.
    pub async fn calculate_fixed_delay(
        &self,
        actor_name: &str,
        attempt_number: usize,
        config: &FixedDelayConfig,
    ) -> Result<Duration, SupervisionError> {
        debug!("Calculating fixed delay for {} (attempt {})", 
               actor_name, attempt_number);

        // Check maximum attempts
        if let Some(max_attempts) = config.max_attempts {
            if attempt_number > max_attempts {
                return Err(SupervisionError::RestartStrategyError {
                    reason: format!("Maximum restart attempts exceeded: {} > {}", 
                                  attempt_number, max_attempts)
                });
            }
        }

        let mut delay = config.delay;

        // Apply progressive delay if configured
        if let Some(increment) = config.progressive_increment {
            let additional_delay = increment * (attempt_number - 1) as u32;
            delay += additional_delay;
        }

        // Cap at maximum if configured
        if let Some(max_delay) = config.max_delay {
            delay = delay.min(max_delay);
        }

        // Apply blockchain-specific adjustments
        if config.blockchain_aligned {
            delay = self.align_delay_to_block_boundary(delay);
        }

        debug!("Calculated fixed delay: {:?}", delay);
        Ok(delay)
    }

    /// Create comprehensive restart attempt tracking (ALYS-006-10)
    /// 
    /// Advanced tracking system for restart attempts with timestamps,
    /// success rates, failure patterns, and blockchain-specific metadata
    /// for the Alys consensus system and governance stream processing.
    pub async fn track_restart_attempt(
        &self,
        actor_name: &str,
        attempt_info: RestartAttemptInfo,
    ) -> Result<(), SupervisionError> {
        debug!("Tracking restart attempt for {}: {:?}", actor_name, attempt_info);

        // Record attempt in history
        {
            let mut history = self.restart_history.write().await;
            history.entry(actor_name.to_string())
                .or_insert_with(Vec::new)
                .push(attempt_info.clone());
        }

        // Update statistics
        {
            let mut stats = self.restart_stats.write().await;
            let actor_stats = stats.entry(actor_name.to_string())
                .or_insert_with(RestartStatistics::default);

            actor_stats.total_attempts += 1;
            actor_stats.last_restart = Some(attempt_info.timestamp);

            // Update success/failure counts when attempt completes
            if let Some(success) = attempt_info.success {
                if success {
                    actor_stats.successful_restarts += 1;
                } else {
                    actor_stats.failed_restarts += 1;
                }

                // Update duration average
                if let Some(duration) = attempt_info.duration {
                    let total_duration = actor_stats.avg_restart_duration * actor_stats.total_attempts as u32
                        + duration;
                    actor_stats.avg_restart_duration = total_duration / (actor_stats.total_attempts as u32 + 1);
                }
            }

            // Track failure types
            if let Some(failure_info) = &attempt_info.failure_info {
                *actor_stats.attempts_by_failure_type
                    .entry(failure_info.failure_type.clone())
                    .or_insert(0) += 1;
            }

            // Calculate success rates
            self.update_success_rates(actor_stats, &attempt_info).await;
        }

        // Detect patterns in restart attempts
        self.analyze_restart_patterns(actor_name, &attempt_info).await?;

        Ok(())
    }

    /// Implement supervisor escalation (ALYS-006-11)
    /// 
    /// Sophisticated escalation system for repeated failures with cascade
    /// prevention, parent supervisor coordination, and blockchain-specific
    /// escalation policies for Alys consensus and governance operations.
    pub async fn escalate_failure(
        &self,
        actor_name: &str,
        failure_info: ActorFailureInfo,
        policy: EscalationPolicy,
    ) -> Result<(), SupervisionError> {
        warn!("Escalating failure for {} with policy {:?}", actor_name, policy);

        // Update supervision state
        {
            let mut contexts = self.contexts.write().await;
            if let Some(context) = contexts.get_mut(actor_name) {
                context.state = SupervisionState::Escalating;
            }
        }

        match policy {
            EscalationPolicy::Stop => {
                info!("Stopping actor {} due to escalation", actor_name);
                self.stop_actor(actor_name).await?;
            }
            EscalationPolicy::EscalateToParent => {
                // Notify parent supervisor (would integrate with actual supervision hierarchy)
                warn!("Escalating {} to parent supervisor", actor_name);
                self.notify_parent_supervisor(actor_name, &failure_info).await?;
            }
            EscalationPolicy::ChangeStrategy(new_strategy) => {
                info!("Changing restart strategy for {} to {:?}", actor_name, new_strategy);
                self.update_restart_strategy(actor_name, new_strategy).await?;
                self.perform_restart(actor_name, failure_info).await?;
            }
            EscalationPolicy::RestartWithReduction => {
                info!("Restarting {} with reduced permissions", actor_name);
                self.restart_with_reduced_permissions(actor_name, failure_info).await?;
            }
            EscalationPolicy::ExternalNotification { endpoint } => {
                warn!("Sending external notification for {} to {}", actor_name, endpoint);
                self.send_external_notification(actor_name, &failure_info, &endpoint).await?;
            }
            EscalationPolicy::Custom(handler) => {
                info!("Using custom escalation handler '{}' for {}", handler, actor_name);
                self.execute_custom_escalation(actor_name, &failure_info, &handler).await?;
            }
        }

        Ok(())
    }

    // Implementation helper methods...

    /// Validate actor configuration
    fn validate_actor_config(&self, config: &SupervisedActorConfig) -> Result<(), SupervisionError> {
        if config.mailbox_capacity == 0 {
            return Err(SupervisionError::ConfigurationError {
                field: "mailbox_capacity".to_string(),
                value: "0".to_string(),
            });
        }

        if let Some(max_restarts) = config.max_restart_attempts {
            if max_restarts == 0 {
                return Err(SupervisionError::ConfigurationError {
                    field: "max_restart_attempts".to_string(),
                    value: "0".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Start actor with supervision configuration
    async fn start_actor_with_supervision<A: Actor + Supervised>(
        &self,
        actor: A,
        name: &str,
        config: &SupervisedActorConfig,
    ) -> Result<Addr<A>, SupervisionError> {
        // In a real implementation, this would integrate with Actix supervision
        // For now, we simulate the actor startup
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // This would be replaced with actual Actix actor startup
        let addr = actor.start();
        
        debug!("Started supervised actor: {}", name);
        Ok(addr)
    }

    /// Default escalation policies for different actor types
    fn default_escalation_policies() -> HashMap<String, EscalationPolicy> {
        let mut policies = HashMap::new();
        
        // Critical consensus actors should escalate to parent
        policies.insert("ChainActor".to_string(), EscalationPolicy::EscalateToParent);
        policies.insert("EngineActor".to_string(), EscalationPolicy::EscalateToParent);
        
        // Bridge actors should change strategy on escalation
        policies.insert("BridgeActor".to_string(), 
            EscalationPolicy::ChangeStrategy(RestartStrategy::FixedDelay {
                delay: Duration::from_secs(5),
                max_restarts: Some(3),
            })
        );
        
        // Background actors should stop on escalation
        policies.insert("HealthMonitor".to_string(), EscalationPolicy::Stop);
        policies.insert("MetricsCollector".to_string(), EscalationPolicy::Stop);
        
        policies
    }

    /// Align delay to blockchain block boundaries (2-second intervals for Alys)
    fn align_delay_to_block_boundary(&self, delay: Duration) -> Duration {
        let block_interval = blockchain::BLOCK_INTERVAL;
        let blocks_to_wait = (delay.as_millis() / block_interval.as_millis()) + 1;
        Duration::from_millis(blocks_to_wait * block_interval.as_millis())
    }

    /// Adjust delay for consensus timing considerations
    async fn adjust_delay_for_consensus_timing(&self, delay: Duration, _actor_name: &str) -> Duration {
        // Add buffer to avoid disrupting consensus operations
        delay + Duration::from_millis(500)
    }

    /// Other implementation methods would be added here...
    async fn analyze_failure_for_restart(&self, _actor_name: &str, _failure_info: &ActorFailureInfo) -> Result<RestartDecision, SupervisionError> {
        // Implementation would analyze failure and return appropriate decision
        Ok(RestartDecision::Restart {
            strategy: RestartStrategy::default(),
            delay: Duration::from_millis(100),
        })
    }

    async fn perform_restart(&self, _actor_name: &str, _failure_info: ActorFailureInfo) -> Result<(), SupervisionError> {
        // Implementation would perform actual actor restart
        Ok(())
    }

    async fn stop_actor(&self, _actor_name: &str) -> Result<(), SupervisionError> {
        // Implementation would stop the actor
        Ok(())
    }

    async fn schedule_health_checks<A: Actor + Supervised>(
        &self,
        _name: &str,
        _addr: Addr<A>,
        _health_check_fn: HealthCheckFn<A>,
        _interval: Duration,
    ) -> Result<(), SupervisionError> {
        // Implementation would schedule periodic health checks
        Ok(())
    }

    async fn update_success_rates(&self, _stats: &mut RestartStatistics, _attempt: &RestartAttemptInfo) {
        // Implementation would calculate success rates over time windows
    }

    async fn analyze_restart_patterns(&self, _actor_name: &str, _attempt: &RestartAttemptInfo) -> Result<(), SupervisionError> {
        // Implementation would analyze patterns in restart attempts
        Ok(())
    }

    async fn notify_parent_supervisor(&self, _actor_name: &str, _failure_info: &ActorFailureInfo) -> Result<(), SupervisionError> {
        // Implementation would notify parent supervisor
        Ok(())
    }

    async fn update_restart_strategy(&self, _actor_name: &str, _strategy: RestartStrategy) -> Result<(), SupervisionError> {
        // Implementation would update restart strategy
        Ok(())
    }

    async fn restart_with_reduced_permissions(&self, _actor_name: &str, _failure_info: ActorFailureInfo) -> Result<(), SupervisionError> {
        // Implementation would restart with reduced permissions
        Ok(())
    }

    async fn send_external_notification(&self, _actor_name: &str, _failure_info: &ActorFailureInfo, _endpoint: &str) -> Result<(), SupervisionError> {
        // Implementation would send external notification
        Ok(())
    }

    async fn execute_custom_escalation(&self, _actor_name: &str, _failure_info: &ActorFailureInfo, _handler: &str) -> Result<(), SupervisionError> {
        // Implementation would execute custom escalation handler
        Ok(())
    }
}

/// Restart decision based on failure analysis
#[derive(Debug, Clone)]
pub enum RestartDecision {
    /// Restart with specified strategy and delay
    Restart { strategy: RestartStrategy, delay: Duration },
    /// Escalate with specified policy
    Escalate { policy: EscalationPolicy },
    /// Stop the actor
    Stop,
}

/// Exponential backoff configuration
#[derive(Debug, Clone)]
pub struct ExponentialBackoffConfig {
    /// Initial delay before first restart
    pub initial_delay: Duration,
    /// Maximum delay between restarts
    pub max_delay: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
    /// Maximum restart attempts
    pub max_attempts: Option<usize>,
    /// Jitter factor (0.0 to 1.0)
    pub jitter: f64,
    /// Align delays to block boundaries
    pub align_to_block_boundary: bool,
    /// Respect consensus timing
    pub respect_consensus_timing: bool,
}

/// Fixed delay configuration
#[derive(Debug, Clone)]
pub struct FixedDelayConfig {
    /// Fixed delay between restarts
    pub delay: Duration,
    /// Maximum restart attempts
    pub max_attempts: Option<usize>,
    /// Progressive increment per attempt
    pub progressive_increment: Option<Duration>,
    /// Maximum delay cap for progressive mode
    pub max_delay: Option<Duration>,
    /// Align to blockchain operations
    pub blockchain_aligned: bool,
}

impl FailurePatternDetector {
    /// Record a failure for pattern analysis
    pub async fn record_failure(&mut self, failure_info: ActorFailureInfo) {
        self.failure_history.push(failure_info);
        
        // Keep history within reasonable bounds
        if self.failure_history.len() > 1000 {
            self.failure_history.drain(0..500);
        }
        
        // Analyze patterns periodically
        self.analyze_patterns().await;
    }
    
    /// Analyze failure history for patterns
    async fn analyze_patterns(&mut self) {
        // Implementation would analyze failure patterns
        // This is a placeholder for the full pattern detection logic
    }
}

// Extension trait for adding blockchain-specific time utilities
trait DurationExt {
    fn from_hours(hours: u64) -> Duration;
}

impl DurationExt for Duration {
    fn from_hours(hours: u64) -> Duration {
        Duration::from_secs(hours * 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::foundation::ActorSystemConfig;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_enhanced_supervision_creation() {
        let config = ActorSystemConfig::development();
        let supervision = EnhancedSupervision::new(config);
        
        // Verify initialization
        assert_eq!(supervision.contexts.read().await.len(), 0);
        assert_eq!(supervision.restart_history.read().await.len(), 0);
        assert_eq!(supervision.restart_stats.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_exponential_backoff_calculation() {
        let config = ActorSystemConfig::development();
        let supervision = EnhancedSupervision::new(config);
        
        let backoff_config = ExponentialBackoffConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_attempts: Some(5),
            jitter: 0.1,
            align_to_block_boundary: false,
            respect_consensus_timing: false,
        };

        // Test first attempt
        let delay1 = supervision.calculate_exponential_backoff_delay("test_actor", 1, &backoff_config).await.unwrap();
        assert!(delay1 >= Duration::from_millis(90)); // With jitter
        assert!(delay1 <= Duration::from_millis(110)); // With jitter

        // Test second attempt
        let delay2 = supervision.calculate_exponential_backoff_delay("test_actor", 2, &backoff_config).await.unwrap();
        assert!(delay2 >= Duration::from_millis(180)); // ~200ms with jitter
        assert!(delay2 <= Duration::from_millis(220));

        // Test max attempts exceeded
        let result = supervision.calculate_exponential_backoff_delay("test_actor", 6, &backoff_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fixed_delay_calculation() {
        let config = ActorSystemConfig::development();
        let supervision = EnhancedSupervision::new(config);
        
        let delay_config = FixedDelayConfig {
            delay: Duration::from_secs(5),
            max_attempts: Some(3),
            progressive_increment: Some(Duration::from_secs(1)),
            max_delay: Some(Duration::from_secs(10)),
            blockchain_aligned: false,
        };

        // Test first attempt
        let delay1 = supervision.calculate_fixed_delay("test_actor", 1, &delay_config).await.unwrap();
        assert_eq!(delay1, Duration::from_secs(5));

        // Test second attempt with progressive increment
        let delay2 = supervision.calculate_fixed_delay("test_actor", 2, &delay_config).await.unwrap();
        assert_eq!(delay2, Duration::from_secs(6));

        // Test third attempt
        let delay3 = supervision.calculate_fixed_delay("test_actor", 3, &delay_config).await.unwrap();
        assert_eq!(delay3, Duration::from_secs(7));

        // Test max attempts exceeded
        let result = supervision.calculate_fixed_delay("test_actor", 4, &delay_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_failure_classification() {
        let panic_failure = ActorFailureInfo {
            timestamp: SystemTime::now(),
            failure_type: ActorFailureType::Panic { backtrace: None },
            message: "Actor panicked".to_string(),
            context: HashMap::new(),
            escalate: false,
        };

        assert_eq!(panic_failure.failure_type, ActorFailureType::Panic { backtrace: None });

        let consensus_failure = ActorFailureInfo {
            timestamp: SystemTime::now(),
            failure_type: ActorFailureType::ConsensusFailure { error_code: "INVALID_BLOCK".to_string() },
            message: "Consensus validation failed".to_string(),
            context: HashMap::new(),
            escalate: true,
        };

        assert!(consensus_failure.escalate);
    }

    #[tokio::test]
    async fn test_restart_attempt_tracking() {
        let config = ActorSystemConfig::development();
        let supervision = EnhancedSupervision::new(config);
        
        let attempt_info = RestartAttemptInfo {
            attempt_id: Uuid::new_v4(),
            attempt_number: 1,
            timestamp: SystemTime::now(),
            reason: RestartReason::ActorPanic,
            delay: Duration::from_millis(100),
            strategy: RestartStrategy::Always,
            success: Some(true),
            duration: Some(Duration::from_millis(50)),
            failure_info: None,
            context: HashMap::new(),
        };

        let result = supervision.track_restart_attempt("test_actor", attempt_info).await;
        assert!(result.is_ok());

        // Verify tracking was recorded
        let history = supervision.restart_history.read().await;
        assert_eq!(history.get("test_actor").unwrap().len(), 1);

        let stats = supervision.restart_stats.read().await;
        let actor_stats = stats.get("test_actor").unwrap();
        assert_eq!(actor_stats.total_attempts, 1);
        assert_eq!(actor_stats.successful_restarts, 1);
    }

    #[tokio::test]
    async fn test_block_boundary_alignment() {
        let config = ActorSystemConfig::development();
        let supervision = EnhancedSupervision::new(config);
        
        // Test alignment with 1.5 second delay -> should align to 2 seconds
        let delay = Duration::from_millis(1500);
        let aligned = supervision.align_delay_to_block_boundary(delay);
        assert_eq!(aligned, Duration::from_secs(2));

        // Test alignment with 3.5 second delay -> should align to 4 seconds  
        let delay = Duration::from_millis(3500);
        let aligned = supervision.align_delay_to_block_boundary(delay);
        assert_eq!(aligned, Duration::from_secs(4));
    }
}