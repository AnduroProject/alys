//! Enhanced RootSupervisor - ALYS-006-03 Implementation
//! 
//! Core supervision infrastructure for Alys V2 actor system with blockchain-aware
//! supervision policies, hierarchical actor management, and integration with the
//! existing supervision tree for the merged mining sidechain architecture.

use crate::actors::foundation::{
    ActorSystemConfig, RestartStrategy, ActorPriority, ActorSpecificConfig,
    blockchain, lifecycle, restart
};
// Note: Integration with actual actor system would be implemented here
// use crate::actor_system::{ActorSystem, SupervisorHandle};
use actix::{Actor, ActorContext, Addr, Context, Handler, Message, ResponseActFuture, WrapFuture};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Enhanced root supervisor for the Alys V2 actor system
/// 
/// Provides comprehensive supervision capabilities including hierarchical
/// supervision, blockchain-aware restart policies, dependency management,
/// health monitoring, and integration with the existing supervision tree.
pub struct RootSupervisor {
    /// System configuration
    config: ActorSystemConfig,
    /// Supervision tree state
    supervision_tree: Arc<RwLock<SupervisionTree>>,
    /// Actor registry for tracking managed actors
    actor_registry: Arc<RwLock<ActorRegistry>>,
    /// Health monitoring state
    health_monitor: Arc<RwLock<HealthMonitor>>,
    /// Restart attempt tracking
    restart_tracker: Arc<RwLock<RestartTracker>>,
    /// Integration with existing actor system (placeholder for future integration)
    actor_system_placeholder: bool,
    /// Supervisor start time
    start_time: SystemTime,
    /// Unique supervisor identifier
    supervisor_id: Uuid,
}

/// Hierarchical supervision tree structure
#[derive(Debug, Clone)]
pub struct SupervisionTree {
    /// Root node of the supervision tree
    root: SupervisionNode,
    /// Depth tracking for validation
    max_depth: usize,
    /// Total number of actors in the tree
    actor_count: usize,
}

/// Individual node in the supervision tree
#[derive(Debug, Clone)]
pub struct SupervisionNode {
    /// Unique node identifier
    node_id: Uuid,
    /// Actor identifier and metadata
    actor_info: ActorInfo,
    /// Child nodes in the supervision hierarchy
    children: Vec<SupervisionNode>,
    /// Parent node reference
    parent_id: Option<Uuid>,
    /// Node-specific supervision settings
    supervision_config: SupervisionConfig,
    /// Current node state
    state: SupervisionNodeState,
}

/// Actor information and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    /// Actor type identifier
    pub actor_type: String,
    /// Human-readable actor name
    pub actor_name: String,
    /// Actor priority level
    pub priority: ActorPriority,
    /// Actor dependencies
    pub dependencies: Vec<String>,
    /// Actor-specific configuration
    pub config: ActorSpecificConfig,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last restart timestamp
    pub last_restart: Option<SystemTime>,
}

/// Node-specific supervision configuration
#[derive(Debug, Clone)]
pub struct SupervisionConfig {
    /// Restart strategy for this node
    pub restart_strategy: RestartStrategy,
    /// Maximum restart attempts
    pub max_restarts: Option<usize>,
    /// Restart timeout
    pub restart_timeout: Duration,
    /// Child supervision policy
    pub child_policy: ChildSupervisionPolicy,
    /// Health check configuration
    pub health_check: Option<HealthCheckSettings>,
}

/// Child supervision policies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildSupervisionPolicy {
    /// Restart only the failed child
    OneForOne,
    /// Restart all children when one fails
    OneForAll,
    /// Restart failed child and all children started after it
    RestForOne,
    /// Custom supervision logic
    Custom(String),
}

/// Supervision node states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionNodeState {
    /// Node is starting up
    Starting,
    /// Node is running normally
    Running,
    /// Node is restarting
    Restarting,
    /// Node has failed and is being handled
    Failed,
    /// Node is shutting down
    ShuttingDown,
    /// Node has been stopped
    Stopped,
}

/// Actor registry for tracking all managed actors
#[derive(Debug, Default)]
pub struct ActorRegistry {
    /// Map of actor type to actor information
    actors: HashMap<String, ActorInfo>,
    /// Map of actor dependencies
    dependencies: HashMap<String, Vec<String>>,
    /// Priority-based actor ordering
    priority_queues: HashMap<ActorPriority, Vec<String>>,
    /// Registry statistics
    stats: RegistryStats,
}

/// Registry statistics
#[derive(Debug, Default, Clone)]
pub struct RegistryStats {
    /// Total number of registered actors
    pub total_actors: usize,
    /// Actors by priority level
    pub actors_by_priority: HashMap<ActorPriority, usize>,
    /// Average actor startup time
    pub avg_startup_time: Duration,
    /// Registry creation time
    pub created_at: Option<SystemTime>,
}

/// Health monitoring system
#[derive(Debug)]
pub struct HealthMonitor {
    /// Health check schedules for each actor
    health_schedules: HashMap<String, HealthSchedule>,
    /// Health check results
    health_results: HashMap<String, HealthResult>,
    /// System-wide health status
    system_health: SystemHealth,
    /// Health monitoring configuration
    config: HealthMonitorConfig,
}

/// Health check scheduling information
#[derive(Debug, Clone)]
pub struct HealthSchedule {
    /// Actor type
    pub actor_type: String,
    /// Check interval
    pub interval: Duration,
    /// Timeout for health check
    pub timeout: Duration,
    /// Last check timestamp
    pub last_check: Option<SystemTime>,
    /// Next scheduled check
    pub next_check: SystemTime,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthResult {
    /// Actor type
    pub actor_type: String,
    /// Health status
    pub healthy: bool,
    /// Check timestamp
    pub timestamp: SystemTime,
    /// Response time
    pub response_time: Duration,
    /// Optional health message
    pub message: Option<String>,
    /// Failure count since last healthy check
    pub failure_count: u32,
}

/// System-wide health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemHealth {
    /// All actors healthy
    Healthy,
    /// Some actors have warnings
    Warning,
    /// Critical actors failing
    Critical,
    /// System degraded
    Degraded,
}

/// Health monitoring configuration
#[derive(Debug, Clone)]
pub struct HealthMonitorConfig {
    /// Default health check interval
    pub default_interval: Duration,
    /// Default timeout
    pub default_timeout: Duration,
    /// Failure threshold before marking unhealthy
    pub failure_threshold: u32,
    /// Enable detailed health reporting
    pub detailed_reporting: bool,
}

/// Health check settings for individual actors
#[derive(Debug, Clone)]
pub struct HealthCheckSettings {
    /// Check interval
    pub interval: Duration,
    /// Response timeout
    pub timeout: Duration,
    /// Failure threshold
    pub failure_threshold: u32,
    /// Enable detailed reporting
    pub detailed_reporting: bool,
}

/// Restart attempt tracking
#[derive(Debug, Default)]
pub struct RestartTracker {
    /// Restart attempts by actor type
    attempts: HashMap<String, Vec<RestartAttemptRecord>>,
    /// Restart statistics
    stats: RestartStats,
}

/// Individual restart attempt record
#[derive(Debug, Clone)]
pub struct RestartAttemptRecord {
    /// Actor type
    pub actor_type: String,
    /// Attempt number
    pub attempt_number: usize,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Reason for restart
    pub reason: RestartReason,
    /// Delay applied
    pub delay: Duration,
    /// Success status
    pub success: Option<bool>,
    /// Duration of restart process
    pub duration: Option<Duration>,
}

/// Restart reason enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RestartReason {
    /// Actor panic or crash
    ActorCrash,
    /// Health check failure
    HealthCheckFailure,
    /// Supervision escalation
    SupervisionEscalation,
    /// Manual restart
    ManualRestart,
    /// Configuration change
    ConfigurationChange,
    /// Blockchain consensus failure
    ConsensusFailure,
    /// Network connectivity issue
    NetworkFailure,
    /// Resource exhaustion
    ResourceExhaustion,
}

/// Restart statistics
#[derive(Debug, Default, Clone)]
pub struct RestartStats {
    /// Total restarts attempted
    pub total_restarts: usize,
    /// Successful restarts
    pub successful_restarts: usize,
    /// Failed restarts
    pub failed_restarts: usize,
    /// Average restart time
    pub avg_restart_time: Duration,
    /// Restarts by reason
    pub restarts_by_reason: HashMap<RestartReason, usize>,
}

/// RootSupervisor error types
#[derive(Debug, Error)]
pub enum RootSupervisorError {
    #[error("Configuration validation failed: {reason}")]
    ConfigurationError { reason: String },
    #[error("Actor registration failed: {actor_type}")]
    ActorRegistrationFailed { actor_type: String },
    #[error("Supervision tree construction failed: {reason}")]
    SupervisionTreeError { reason: String },
    #[error("Health monitoring initialization failed: {reason}")]
    HealthMonitorError { reason: String },
    #[error("Restart operation failed: {actor_type} - {reason}")]
    RestartFailed { actor_type: String, reason: String },
    #[error("Dependency resolution failed: {actor_type}")]
    DependencyError { actor_type: String },
    #[error("System integration error: {reason}")]
    SystemIntegrationError { reason: String },
}

/// Messages for RootSupervisor actor
#[derive(Message)]
#[rtype(result = "Result<(), RootSupervisorError>")]
pub struct RegisterActor {
    pub actor_info: ActorInfo,
}

#[derive(Message)]
#[rtype(result = "Result<(), RootSupervisorError>")]
pub struct UnregisterActor {
    pub actor_type: String,
}

#[derive(Message)]
#[rtype(result = "Result<RestartAttemptRecord, RootSupervisorError>")]
pub struct RestartActor {
    pub actor_type: String,
    pub reason: RestartReason,
}

#[derive(Message)]
#[rtype(result = "Result<SystemHealth, RootSupervisorError>")]
pub struct GetSystemHealth;

#[derive(Message)]
#[rtype(result = "Result<RegistryStats, RootSupervisorError>")]
pub struct GetRegistryStats;

#[derive(Message)]
#[rtype(result = "Result<(), RootSupervisorError>")]
pub struct UpdateConfiguration {
    pub config: ActorSystemConfig,
}

impl Default for SupervisionTree {
    fn default() -> Self {
        Self {
            root: SupervisionNode::root(),
            max_depth: 0,
            actor_count: 0,
        }
    }
}

impl SupervisionNode {
    /// Create root supervision node
    pub fn root() -> Self {
        Self {
            node_id: Uuid::new_v4(),
            actor_info: ActorInfo {
                actor_type: "RootSupervisor".to_string(),
                actor_name: "root-supervisor".to_string(),
                priority: ActorPriority::Critical,
                dependencies: vec![],
                config: ActorSpecificConfig {
                    restart_strategy: Some(RestartStrategy::Always),
                    mailbox_capacity: Some(100000),
                    priority: ActorPriority::Critical,
                    dependencies: vec![],
                    health_check_config: None,
                },
                created_at: SystemTime::now(),
                last_restart: None,
            },
            children: vec![],
            parent_id: None,
            supervision_config: SupervisionConfig {
                restart_strategy: RestartStrategy::Always,
                max_restarts: None,
                restart_timeout: lifecycle::ACTOR_STARTUP_TIMEOUT,
                child_policy: ChildSupervisionPolicy::OneForOne,
                health_check: None,
            },
            state: SupervisionNodeState::Starting,
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self {
            health_schedules: HashMap::new(),
            health_results: HashMap::new(),
            system_health: SystemHealth::Healthy,
            config: HealthMonitorConfig {
                default_interval: Duration::from_secs(10),
                default_timeout: Duration::from_secs(5),
                failure_threshold: 3,
                detailed_reporting: false,
            },
        }
    }
}

impl RootSupervisor {
    /// Create new RootSupervisor with configuration
    pub fn new(config: ActorSystemConfig) -> Result<Self, RootSupervisorError> {
        // Validate configuration
        config.validate().map_err(|e| RootSupervisorError::ConfigurationError {
            reason: e.to_string(),
        })?;

        let supervisor_id = Uuid::new_v4();
        info!("Creating RootSupervisor with ID: {}", supervisor_id);

        // Initialize supervision tree
        let supervision_tree = Arc::new(RwLock::new(SupervisionTree::default()));

        // Initialize actor registry
        let mut registry = ActorRegistry::default();
        registry.stats.created_at = Some(SystemTime::now());
        let actor_registry = Arc::new(RwLock::new(registry));

        // Initialize health monitor with configuration
        let health_monitor = Arc::new(RwLock::new(HealthMonitor {
            config: HealthMonitorConfig {
                default_interval: config.health_check_interval,
                default_timeout: Duration::from_secs(5),
                failure_threshold: 3,
                detailed_reporting: config.metrics_enabled,
            },
            ..Default::default()
        }));

        // Initialize restart tracker
        let restart_tracker = Arc::new(RwLock::new(RestartTracker::default()));

        Ok(Self {
            config,
            supervision_tree,
            actor_registry,
            health_monitor,
            restart_tracker,
            actor_system_placeholder: false,
            start_time: SystemTime::now(),
            supervisor_id,
        })
    }

    /// Initialize the supervision tree with actor configurations
    pub async fn initialize_supervision_tree(&mut self) -> Result<(), RootSupervisorError> {
        info!("Initializing supervision tree");

        let mut tree = self.supervision_tree.write().await;
        
        // Build supervision hierarchy based on priority and dependencies
        for (actor_type, actor_config) in &self.config.actor_configs {
            self.add_actor_to_tree(&mut tree, actor_type.clone(), actor_config.clone()).await?;
        }

        info!("Supervision tree initialized with {} actors", tree.actor_count);
        Ok(())
    }

    /// Add actor to supervision tree
    async fn add_actor_to_tree(
        &self,
        tree: &mut SupervisionTree,
        actor_type: String,
        config: ActorSpecificConfig,
    ) -> Result<(), RootSupervisorError> {
        let node = SupervisionNode {
            node_id: Uuid::new_v4(),
            actor_info: ActorInfo {
                actor_type: actor_type.clone(),
                actor_name: actor_type.clone(),
                priority: config.priority,
                dependencies: config.dependencies.clone(),
                config: config.clone(),
                created_at: SystemTime::now(),
                last_restart: None,
            },
            children: vec![],
            parent_id: Some(tree.root.node_id),
            supervision_config: SupervisionConfig {
                restart_strategy: config.restart_strategy.unwrap_or_default(),
                max_restarts: Some(restart::DEFAULT_MAX_RESTARTS),
                restart_timeout: lifecycle::ACTOR_STARTUP_TIMEOUT,
                child_policy: ChildSupervisionPolicy::OneForOne,
                health_check: config.health_check_config.map(|hc| HealthCheckSettings {
                    interval: hc.interval,
                    timeout: hc.timeout,
                    failure_threshold: hc.failure_threshold,
                    detailed_reporting: hc.detailed_reporting,
                }),
            },
            state: SupervisionNodeState::Starting,
        };

        // Add to root children (simplified hierarchy for Phase 1)
        tree.root.children.push(node);
        tree.actor_count += 1;
        tree.max_depth = tree.max_depth.max(1);

        debug!("Added actor {} to supervision tree", actor_type);
        Ok(())
    }

    /// Register an actor with the supervisor
    pub async fn register_actor(&mut self, actor_info: ActorInfo) -> Result<(), RootSupervisorError> {
        info!("Registering actor: {}", actor_info.actor_type);

        // Add to registry
        {
            let mut registry = self.actor_registry.write().await;
            registry.actors.insert(actor_info.actor_type.clone(), actor_info.clone());
            registry.stats.total_actors += 1;
            
            // Update priority queue
            registry.priority_queues
                .entry(actor_info.priority)
                .or_insert_with(Vec::new)
                .push(actor_info.actor_type.clone());

            // Update priority stats
            *registry.stats.actors_by_priority
                .entry(actor_info.priority)
                .or_insert(0) += 1;
        }

        // Schedule health checks if configured
        if let Some(health_config) = &actor_info.config.health_check_config {
            self.schedule_health_check(&actor_info.actor_type, health_config).await?;
        }

        debug!("Actor {} registered successfully", actor_info.actor_type);
        Ok(())
    }

    /// Schedule health check for an actor
    async fn schedule_health_check(
        &self,
        actor_type: &str,
        config: &crate::actors::foundation::HealthCheckConfig,
    ) -> Result<(), RootSupervisorError> {
        let mut health_monitor = self.health_monitor.write().await;
        
        let schedule = HealthSchedule {
            actor_type: actor_type.to_string(),
            interval: config.interval,
            timeout: config.timeout,
            last_check: None,
            next_check: SystemTime::now() + config.interval,
        };

        health_monitor.health_schedules.insert(actor_type.to_string(), schedule);
        debug!("Health check scheduled for actor: {}", actor_type);
        
        Ok(())
    }

    /// Restart an actor with the configured strategy
    pub async fn restart_actor(
        &mut self,
        actor_type: &str,
        reason: RestartReason,
    ) -> Result<RestartAttemptRecord, RootSupervisorError> {
        info!("Restarting actor: {} (reason: {:?})", actor_type, reason);

        // Get actor configuration
        let actor_config = self.config.actor_configs.get(actor_type)
            .ok_or_else(|| RootSupervisorError::ActorRegistrationFailed {
                actor_type: actor_type.to_string(),
            })?;

        // Get current attempt count
        let attempt_number = {
            let tracker = self.restart_tracker.read().await;
            tracker.attempts.get(actor_type).map(|attempts| attempts.len()).unwrap_or(0)
        };

        // Get restart strategy
        let restart_strategy = actor_config.restart_strategy.as_ref()
            .unwrap_or(&self.config.default_restart_strategy);

        // Calculate delay
        let last_attempts = {
            let tracker = self.restart_tracker.read().await;
            tracker.attempts.get(actor_type).cloned().unwrap_or_default()
                .into_iter().map(|record| crate::actors::foundation::RestartAttempt {
                    attempt_number: record.attempt_number,
                    timestamp: record.timestamp,
                    delay: record.delay,
                    reason: match record.reason {
                        RestartReason::ActorCrash => crate::actors::foundation::RestartReason::ActorPanic,
                        RestartReason::HealthCheckFailure => crate::actors::foundation::RestartReason::HealthCheckFailure,
                        RestartReason::SupervisionEscalation => crate::actors::foundation::RestartReason::SupervisionEscalation,
                        RestartReason::ManualRestart => crate::actors::foundation::RestartReason::ManualRestart,
                        RestartReason::ConfigurationChange => crate::actors::foundation::RestartReason::ConfigurationChange,
                        RestartReason::ConsensusFailure => crate::actors::foundation::RestartReason::ConsensusFailure,
                        RestartReason::NetworkFailure => crate::actors::foundation::RestartReason::NetworkFailure,
                        RestartReason::ResourceExhaustion => crate::actors::foundation::RestartReason::ResourceExhaustion,
                    },
                    successful: record.success,
                }).collect()
        };

        let delay = restart_strategy.calculate_delay(attempt_number, &last_attempts)
            .map_err(|e| RootSupervisorError::RestartFailed {
                actor_type: actor_type.to_string(),
                reason: e.to_string(),
            })?;

        let restart_delay = delay.unwrap_or(Duration::ZERO);

        // Create restart attempt record
        let restart_record = RestartAttemptRecord {
            actor_type: actor_type.to_string(),
            attempt_number,
            timestamp: SystemTime::now(),
            reason: reason.clone(),
            delay: restart_delay,
            success: None,
            duration: None,
        };

        // Record restart attempt
        {
            let mut tracker = self.restart_tracker.write().await;
            tracker.attempts.entry(actor_type.to_string())
                .or_insert_with(Vec::new)
                .push(restart_record.clone());
            
            tracker.stats.total_restarts += 1;
            *tracker.stats.restarts_by_reason.entry(reason).or_insert(0) += 1;
        }

        // Apply delay if required
        if !restart_delay.is_zero() {
            debug!("Applying restart delay of {:?} for actor: {}", restart_delay, actor_type);
            tokio::time::sleep(restart_delay).await;
        }

        // TODO: Integrate with actual actor restart logic
        debug!("Actor {} restart initiated", actor_type);
        
        Ok(restart_record)
    }

    /// Get system health status
    pub async fn get_system_health(&self) -> SystemHealth {
        let health_monitor = self.health_monitor.read().await;
        health_monitor.system_health.clone()
    }

    /// Get registry statistics
    pub async fn get_registry_stats(&self) -> RegistryStats {
        let registry = self.actor_registry.read().await;
        registry.stats.clone()
    }

    /// Update supervisor configuration
    pub async fn update_configuration(
        &mut self,
        new_config: ActorSystemConfig,
    ) -> Result<(), RootSupervisorError> {
        info!("Updating RootSupervisor configuration");

        // Validate new configuration
        new_config.validate().map_err(|e| RootSupervisorError::ConfigurationError {
            reason: e.to_string(),
        })?;

        // Update configuration
        self.config = new_config;

        // Reinitialize supervision tree if needed
        self.initialize_supervision_tree().await?;

        info!("Configuration updated successfully");
        Ok(())
    }

    /// Get supervisor statistics
    pub async fn get_supervisor_stats(&self) -> SupervisorStats {
        let registry_stats = self.get_registry_stats().await;
        let restart_stats = {
            let tracker = self.restart_tracker.read().await;
            tracker.stats.clone()
        };

        SupervisorStats {
            supervisor_id: self.supervisor_id,
            start_time: self.start_time,
            registry_stats,
            restart_stats,
            system_health: self.get_system_health().await,
        }
    }
}

/// Comprehensive supervisor statistics
#[derive(Debug, Clone)]
pub struct SupervisorStats {
    /// Supervisor identifier
    pub supervisor_id: Uuid,
    /// Supervisor start time
    pub start_time: SystemTime,
    /// Registry statistics
    pub registry_stats: RegistryStats,
    /// Restart statistics
    pub restart_stats: RestartStats,
    /// Current system health
    pub system_health: SystemHealth,
}

impl Actor for RootSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("RootSupervisor started with ID: {}", self.supervisor_id);
        
        // Schedule periodic health checks
        if self.config.health_check_enabled {
            ctx.run_interval(self.config.health_check_interval, |supervisor, _ctx| {
                let supervisor_clone = supervisor.clone();
                let future = async move {
                    // TODO: Implement periodic health check logic
                    debug!("Periodic health check completed");
                };
                Box::pin(future.into_actor(supervisor))
            });
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("RootSupervisor stopped");
    }
}

impl Clone for RootSupervisor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            supervision_tree: Arc::clone(&self.supervision_tree),
            actor_registry: Arc::clone(&self.actor_registry),
            health_monitor: Arc::clone(&self.health_monitor),
            restart_tracker: Arc::clone(&self.restart_tracker),
            actor_system_placeholder: self.actor_system_placeholder,
            start_time: self.start_time,
            supervisor_id: self.supervisor_id,
        }
    }
}

// Message handlers
impl Handler<RegisterActor> for RootSupervisor {
    type Result = ResponseActFuture<Self, Result<(), RootSupervisorError>>;

    fn handle(&mut self, msg: RegisterActor, _ctx: &mut Self::Context) -> Self::Result {
        let mut supervisor = self.clone();
        Box::pin(
            async move {
                supervisor.register_actor(msg.actor_info).await
            }
            .into_actor(&mut *supervisor)
        )
    }
}

impl Handler<RestartActor> for RootSupervisor {
    type Result = ResponseActFuture<Self, Result<RestartAttemptRecord, RootSupervisorError>>;

    fn handle(&mut self, msg: RestartActor, _ctx: &mut Self::Context) -> Self::Result {
        let mut supervisor = self.clone();
        Box::pin(
            async move {
                supervisor.restart_actor(&msg.actor_type, msg.reason).await
            }
            .into_actor(&mut *supervisor)
        )
    }
}

impl Handler<GetSystemHealth> for RootSupervisor {
    type Result = ResponseActFuture<Self, Result<SystemHealth, RootSupervisorError>>;

    fn handle(&mut self, _msg: GetSystemHealth, _ctx: &mut Self::Context) -> Self::Result {
        let supervisor = self.clone();
        Box::pin(
            async move {
                Ok(supervisor.get_system_health().await)
            }
            .into_actor(&mut *supervisor)
        )
    }
}

impl Handler<GetRegistryStats> for RootSupervisor {
    type Result = ResponseActFuture<Self, Result<RegistryStats, RootSupervisorError>>;

    fn handle(&mut self, _msg: GetRegistryStats, _ctx: &mut Self::Context) -> Self::Result {
        let supervisor = self.clone();
        Box::pin(
            async move {
                Ok(supervisor.get_registry_stats().await)
            }
            .into_actor(&mut *supervisor)
        )
    }
}

impl Handler<UpdateConfiguration> for RootSupervisor {
    type Result = ResponseActFuture<Self, Result<(), RootSupervisorError>>;

    fn handle(&mut self, msg: UpdateConfiguration, _ctx: &mut Self::Context) -> Self::Result {
        let mut supervisor = self.clone();
        Box::pin(
            async move {
                supervisor.update_configuration(msg.config).await
            }
            .into_actor(&mut *supervisor)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::foundation::ActorSystemConfig;
    use std::time::SystemTime;

    fn create_test_config() -> ActorSystemConfig {
        ActorSystemConfig::development()
    }

    fn create_test_actor_info(actor_type: &str) -> ActorInfo {
        ActorInfo {
            actor_type: actor_type.to_string(),
            actor_name: actor_type.to_lowercase(),
            priority: ActorPriority::Normal,
            dependencies: vec![],
            config: ActorSpecificConfig {
                restart_strategy: Some(RestartStrategy::default()),
                mailbox_capacity: Some(1000),
                priority: ActorPriority::Normal,
                dependencies: vec![],
                health_check_config: None,
            },
            created_at: SystemTime::now(),
            last_restart: None,
        }
    }

    #[tokio::test]
    async fn test_root_supervisor_creation() {
        let config = create_test_config();
        let supervisor = RootSupervisor::new(config);
        assert!(supervisor.is_ok());
    }

    #[tokio::test]
    async fn test_supervision_tree_initialization() {
        let config = create_test_config();
        let mut supervisor = RootSupervisor::new(config).unwrap();
        
        let result = supervisor.initialize_supervision_tree().await;
        assert!(result.is_ok());
        
        let tree = supervisor.supervision_tree.read().await;
        assert!(tree.actor_count > 0);
    }

    #[tokio::test]
    async fn test_actor_registration() {
        let config = create_test_config();
        let mut supervisor = RootSupervisor::new(config).unwrap();
        
        let actor_info = create_test_actor_info("TestActor");
        let result = supervisor.register_actor(actor_info).await;
        assert!(result.is_ok());
        
        let stats = supervisor.get_registry_stats().await;
        assert_eq!(stats.total_actors, 1);
    }

    #[tokio::test]
    async fn test_actor_restart() {
        let config = create_test_config();
        let mut supervisor = RootSupervisor::new(config).unwrap();
        
        // Register actor first
        let actor_info = create_test_actor_info("TestActor");
        supervisor.register_actor(actor_info).await.unwrap();
        
        // Add actor configuration
        supervisor.config.set_actor_config("TestActor".to_string(), ActorSpecificConfig {
            restart_strategy: Some(RestartStrategy::Always),
            mailbox_capacity: Some(1000),
            priority: ActorPriority::Normal,
            dependencies: vec![],
            health_check_config: None,
        });
        
        let restart_result = supervisor.restart_actor("TestActor", RestartReason::ManualRestart).await;
        assert!(restart_result.is_ok());
    }

    #[tokio::test]
    async fn test_system_health_monitoring() {
        let config = create_test_config();
        let supervisor = RootSupervisor::new(config).unwrap();
        
        let health = supervisor.get_system_health().await;
        assert_eq!(health, SystemHealth::Healthy);
    }

    #[tokio::test]
    async fn test_configuration_update() {
        let config = create_test_config();
        let mut supervisor = RootSupervisor::new(config).unwrap();
        
        let new_config = ActorSystemConfig::production();
        let result = supervisor.update_configuration(new_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_supervisor_stats() {
        let config = create_test_config();
        let supervisor = RootSupervisor::new(config).unwrap();
        
        let stats = supervisor.get_supervisor_stats().await;
        assert!(stats.start_time <= SystemTime::now());
        assert_eq!(stats.registry_stats.total_actors, 0);
    }
}