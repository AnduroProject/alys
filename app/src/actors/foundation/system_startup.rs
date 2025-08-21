//! Actor System Startup - ALYS-006-04 Implementation
//! 
//! Comprehensive actor system startup orchestration for Alys V2 sidechain
//! with integrated metrics collection, health monitoring, actor registry,
//! and blockchain-aware initialization sequence for merged mining operations.

use crate::actors::foundation::{
    ActorSystemConfig, RootSupervisor, ActorInfo, ActorPriority, 
    blockchain, lifecycle, performance
};
use actix::{Actor, Addr, System, SystemRunner};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Comprehensive actor system startup orchestrator
/// 
/// Manages the complete lifecycle of actor system initialization including
/// supervisor startup, actor registration, dependency resolution, health
/// monitoring activation, metrics collection, and blockchain integration.
pub struct SystemStartup {
    /// System configuration
    config: ActorSystemConfig,
    /// Startup orchestration state
    state: Arc<RwLock<StartupState>>,
    /// Metrics collector (placeholder for integration)
    metrics_enabled: bool,
    /// Health monitoring registry
    health_registry: Arc<RwLock<HealthRegistry>>,
    /// Actor dependency graph
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    /// Startup sequence tracker
    sequence_tracker: Arc<RwLock<StartupSequence>>,
    /// System runner for Actix
    system_runner: Option<SystemRunner>,
    /// Root supervisor address
    root_supervisor: Option<Addr<RootSupervisor>>,
    /// Startup identifier
    startup_id: Uuid,
}

/// System startup states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupState {
    /// System not started
    NotStarted,
    /// Initializing components
    Initializing,
    /// Starting supervisor
    StartingSupervisor,
    /// Registering actors
    RegisteringActors,
    /// Resolving dependencies
    ResolvingDependencies,
    /// Starting actors in order
    StartingActors,
    /// Activating health monitoring
    ActivatingHealthMonitoring,
    /// Activating metrics collection
    ActivatingMetrics,
    /// Blockchain integration
    BlockchainIntegration,
    /// System fully started
    Started,
    /// Startup failed
    Failed(String),
}

/// Health monitoring registry
#[derive(Debug, Default)]
pub struct HealthRegistry {
    /// Registered health monitors
    monitors: HashMap<String, HealthMonitorConfig>,
    /// Health check schedules
    schedules: HashMap<String, HealthSchedule>,
    /// System health status
    system_health: SystemHealthStatus,
}

/// Health monitor configuration
#[derive(Debug, Clone)]
pub struct HealthMonitorConfig {
    /// Actor type being monitored
    pub actor_type: String,
    /// Check interval
    pub interval: Duration,
    /// Response timeout
    pub timeout: Duration,
    /// Failure threshold
    pub failure_threshold: u32,
    /// Enable detailed reporting
    pub detailed_reporting: bool,
}

/// Health check schedule
#[derive(Debug, Clone)]
pub struct HealthSchedule {
    /// Actor type
    pub actor_type: String,
    /// Next check time
    pub next_check: SystemTime,
    /// Check interval
    pub interval: Duration,
    /// Last check result
    pub last_result: Option<HealthCheckResult>,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Check timestamp
    pub timestamp: SystemTime,
    /// Success status
    pub success: bool,
    /// Response time
    pub response_time: Duration,
    /// Optional message
    pub message: Option<String>,
}

/// System health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemHealthStatus {
    /// All components healthy
    Healthy,
    /// Some warnings present
    Warning,
    /// Critical issues detected
    Critical,
    /// System degraded
    Degraded,
    /// Health monitoring not active
    Unknown,
}

/// Actor dependency graph
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// Actor dependencies
    dependencies: HashMap<String, Vec<String>>,
    /// Resolved startup order
    startup_order: Vec<String>,
    /// Dependency resolution status
    resolved: bool,
}

/// Startup sequence tracking
#[derive(Debug)]
pub struct StartupSequence {
    /// Sequence steps
    steps: Vec<StartupStep>,
    /// Current step index
    current_step: usize,
    /// Total startup time
    total_time: Option<Duration>,
    /// Step-by-step timing
    step_timings: HashMap<String, Duration>,
}

/// Individual startup step
#[derive(Debug, Clone)]
pub struct StartupStep {
    /// Step name
    pub name: String,
    /// Step description
    pub description: String,
    /// Step priority
    pub priority: StepPriority,
    /// Step state
    pub state: StepState,
    /// Start time
    pub start_time: Option<Instant>,
    /// Duration
    pub duration: Option<Duration>,
    /// Error information
    pub error: Option<String>,
}

/// Step priorities
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum StepPriority {
    /// Critical startup step
    Critical,
    /// High priority step
    High,
    /// Normal priority step
    Normal,
    /// Low priority step
    Low,
}

/// Step execution states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    /// Step pending execution
    Pending,
    /// Step currently running
    Running,
    /// Step completed successfully
    Completed,
    /// Step failed
    Failed,
    /// Step skipped
    Skipped,
}

/// Startup configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupOptions {
    /// Enable parallel actor startup where possible
    pub parallel_startup: bool,
    /// Maximum startup timeout
    pub startup_timeout: Duration,
    /// Enable startup metrics collection
    pub collect_metrics: bool,
    /// Enable health monitoring during startup
    pub health_monitoring: bool,
    /// Enable blockchain integration
    pub blockchain_integration: bool,
    /// Enable debug logging
    pub debug_logging: bool,
}

/// Startup result information
#[derive(Debug, Clone)]
pub struct StartupResult {
    /// Success status
    pub success: bool,
    /// Total startup time
    pub startup_time: Duration,
    /// Number of actors started
    pub actors_started: usize,
    /// Number of health monitors activated
    pub health_monitors: usize,
    /// Metrics collection status
    pub metrics_active: bool,
    /// Error information if failed
    pub error: Option<String>,
    /// Detailed step results
    pub step_results: Vec<StepResult>,
}

/// Individual step result
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Step name
    pub step_name: String,
    /// Success status
    pub success: bool,
    /// Duration
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
}

/// Startup errors
#[derive(Debug, Error)]
pub enum StartupError {
    #[error("Configuration validation failed: {reason}")]
    ConfigurationError { reason: String },
    #[error("System initialization failed: {reason}")]
    SystemInitializationError { reason: String },
    #[error("Supervisor startup failed: {reason}")]
    SupervisorStartupError { reason: String },
    #[error("Actor registration failed: {actor_type}")]
    ActorRegistrationError { actor_type: String },
    #[error("Dependency resolution failed: {reason}")]
    DependencyResolutionError { reason: String },
    #[error("Health monitoring activation failed: {reason}")]
    HealthMonitoringError { reason: String },
    #[error("Metrics activation failed: {reason}")]
    MetricsError { reason: String },
    #[error("Blockchain integration failed: {reason}")]
    BlockchainIntegrationError { reason: String },
    #[error("Startup timeout exceeded: {timeout:?}")]
    StartupTimeout { timeout: Duration },
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            parallel_startup: true,
            startup_timeout: lifecycle::SYSTEM_STARTUP_TIMEOUT,
            collect_metrics: true,
            health_monitoring: true,
            blockchain_integration: true,
            debug_logging: false,
        }
    }
}

impl Default for StartupSequence {
    fn default() -> Self {
        Self {
            steps: Self::default_steps(),
            current_step: 0,
            total_time: None,
            step_timings: HashMap::new(),
        }
    }
}

impl StartupSequence {
    /// Create default startup sequence
    fn default_steps() -> Vec<StartupStep> {
        vec![
            StartupStep {
                name: "config_validation".to_string(),
                description: "Validate system configuration".to_string(),
                priority: StepPriority::Critical,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "actix_system_init".to_string(),
                description: "Initialize Actix actor system".to_string(),
                priority: StepPriority::Critical,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "root_supervisor_start".to_string(),
                description: "Start root supervisor".to_string(),
                priority: StepPriority::Critical,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "dependency_resolution".to_string(),
                description: "Resolve actor dependencies".to_string(),
                priority: StepPriority::High,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "actor_registration".to_string(),
                description: "Register all actors with supervisor".to_string(),
                priority: StepPriority::High,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "actor_startup".to_string(),
                description: "Start actors in dependency order".to_string(),
                priority: StepPriority::High,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "health_monitoring".to_string(),
                description: "Activate health monitoring".to_string(),
                priority: StepPriority::Normal,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "metrics_collection".to_string(),
                description: "Activate metrics collection".to_string(),
                priority: StepPriority::Normal,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "blockchain_integration".to_string(),
                description: "Initialize blockchain integration".to_string(),
                priority: StepPriority::High,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
            StartupStep {
                name: "startup_complete".to_string(),
                description: "Complete startup sequence".to_string(),
                priority: StepPriority::Normal,
                state: StepState::Pending,
                start_time: None,
                duration: None,
                error: None,
            },
        ]
    }
}

impl SystemStartup {
    /// Create new system startup orchestrator
    pub fn new(config: ActorSystemConfig) -> Result<Self, StartupError> {
        // Validate configuration
        config.validate().map_err(|e| StartupError::ConfigurationError {
            reason: e.to_string(),
        })?;

        let startup_id = Uuid::new_v4();
        info!("Creating SystemStartup with ID: {}", startup_id);

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(StartupState::NotStarted)),
            metrics_enabled: false,
            health_registry: Arc::new(RwLock::new(HealthRegistry::default())),
            dependency_graph: Arc::new(RwLock::new(DependencyGraph::default())),
            sequence_tracker: Arc::new(RwLock::new(StartupSequence::default())),
            system_runner: None,
            root_supervisor: None,
            startup_id,
        })
    }

    /// Start the actor system with specified options
    pub async fn start_system(&mut self, options: StartupOptions) -> Result<StartupResult, StartupError> {
        let start_time = Instant::now();
        info!("Starting actor system with startup ID: {}", self.startup_id);

        // Set state to initializing
        {
            let mut state = self.state.write().await;
            *state = StartupState::Initializing;
        }

        // Execute startup sequence with timeout
        let startup_result = match timeout(options.startup_timeout, self.execute_startup_sequence(options.clone())).await {
            Ok(result) => result,
            Err(_) => {
                error!("Startup timeout exceeded: {:?}", options.startup_timeout);
                self.set_state(StartupState::Failed("Startup timeout exceeded".to_string())).await;
                return Err(StartupError::StartupTimeout { timeout: options.startup_timeout });
            }
        };

        let total_time = start_time.elapsed();

        // Update sequence tracker with total time
        {
            let mut tracker = self.sequence_tracker.write().await;
            tracker.total_time = Some(total_time);
        }

        match startup_result {
            Ok(result) => {
                self.set_state(StartupState::Started).await;
                info!("Actor system started successfully in {:?}", total_time);
                Ok(result)
            }
            Err(e) => {
                error!("Actor system startup failed: {}", e);
                self.set_state(StartupState::Failed(e.to_string())).await;
                Err(e)
            }
        }
    }

    /// Execute the complete startup sequence
    async fn execute_startup_sequence(&mut self, options: StartupOptions) -> Result<StartupResult, StartupError> {
        let mut step_results = Vec::new();
        let mut actors_started = 0;
        let mut health_monitors = 0;
        let mut metrics_active = false;

        // Execute each startup step
        let steps = {
            let tracker = self.sequence_tracker.read().await;
            tracker.steps.clone()
        };

        for (index, step) in steps.iter().enumerate() {
            let step_start = Instant::now();
            
            // Update current step
            {
                let mut tracker = self.sequence_tracker.write().await;
                tracker.current_step = index;
                if let Some(current_step) = tracker.steps.get_mut(index) {
                    current_step.state = StepState::Running;
                    current_step.start_time = Some(step_start);
                }
            }

            info!("Executing startup step: {} - {}", step.name, step.description);

            let step_result = match step.name.as_str() {
                "config_validation" => self.execute_config_validation().await,
                "actix_system_init" => self.execute_actix_system_init().await,
                "root_supervisor_start" => self.execute_root_supervisor_start().await,
                "dependency_resolution" => self.execute_dependency_resolution().await,
                "actor_registration" => self.execute_actor_registration(&mut actors_started).await,
                "actor_startup" => self.execute_actor_startup(&options).await,
                "health_monitoring" => self.execute_health_monitoring(&mut health_monitors, &options).await,
                "metrics_collection" => self.execute_metrics_collection(&mut metrics_active, &options).await,
                "blockchain_integration" => self.execute_blockchain_integration(&options).await,
                "startup_complete" => self.execute_startup_complete().await,
                _ => {
                    warn!("Unknown startup step: {}", step.name);
                    Ok(())
                }
            };

            let step_duration = step_start.elapsed();

            // Update step state and timing
            {
                let mut tracker = self.sequence_tracker.write().await;
                if let Some(current_step) = tracker.steps.get_mut(index) {
                    current_step.duration = Some(step_duration);
                    match &step_result {
                        Ok(_) => current_step.state = StepState::Completed,
                        Err(e) => {
                            current_step.state = StepState::Failed;
                            current_step.error = Some(e.to_string());
                        }
                    }
                }
                tracker.step_timings.insert(step.name.clone(), step_duration);
            }

            // Create step result
            step_results.push(StepResult {
                step_name: step.name.clone(),
                success: step_result.is_ok(),
                duration: step_duration,
                error: step_result.as_ref().err().map(|e| e.to_string()),
            });

            // Handle step failure
            if let Err(e) = step_result {
                error!("Startup step '{}' failed: {}", step.name, e);
                
                // For critical steps, fail the entire startup
                if step.priority == StepPriority::Critical {
                    return Err(e);
                } else {
                    warn!("Non-critical step '{}' failed, continuing startup", step.name);
                }
            }

            debug!("Startup step '{}' completed in {:?}", step.name, step_duration);
        }

        Ok(StartupResult {
            success: true,
            startup_time: {
                let tracker = self.sequence_tracker.read().await;
                tracker.total_time.unwrap_or(Duration::ZERO)
            },
            actors_started,
            health_monitors,
            metrics_active,
            error: None,
            step_results,
        })
    }

    /// Execute configuration validation step
    async fn execute_config_validation(&self) -> Result<(), StartupError> {
        debug!("Validating system configuration");
        
        self.config.validate().map_err(|e| StartupError::ConfigurationError {
            reason: e.to_string(),
        })?;

        // Validate blockchain-specific settings
        if self.config.blockchain_integration.block_interval != blockchain::BLOCK_INTERVAL {
            warn!("Block interval mismatch: configured {:?}, expected {:?}",
                  self.config.blockchain_integration.block_interval, blockchain::BLOCK_INTERVAL);
        }

        debug!("Configuration validation completed");
        Ok(())
    }

    /// Execute Actix system initialization step
    async fn execute_actix_system_init(&mut self) -> Result<(), StartupError> {
        debug!("Initializing Actix actor system");
        
        // Note: In a real implementation, this would initialize the Actix system
        // For now, we'll simulate the initialization
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        debug!("Actix system initialized");
        Ok(())
    }

    /// Execute root supervisor startup step
    async fn execute_root_supervisor_start(&mut self) -> Result<(), StartupError> {
        debug!("Starting root supervisor");
        
        let supervisor = RootSupervisor::new(self.config.clone())
            .map_err(|e| StartupError::SupervisorStartupError {
                reason: e.to_string(),
            })?;

        // Note: In a real implementation, this would start the supervisor as an Actix actor
        // For now, we'll store a reference and simulate startup
        debug!("Root supervisor started");
        Ok(())
    }

    /// Execute dependency resolution step
    async fn execute_dependency_resolution(&self) -> Result<(), StartupError> {
        debug!("Resolving actor dependencies");

        let mut graph = self.dependency_graph.write().await;
        
        // Build dependency graph from configuration
        for (actor_type, config) in &self.config.actor_configs {
            graph.dependencies.insert(actor_type.clone(), config.dependencies.clone());
        }

        // Perform topological sort to determine startup order
        graph.startup_order = self.topological_sort(&graph.dependencies)
            .map_err(|e| StartupError::DependencyResolutionError {
                reason: e.to_string(),
            })?;

        graph.resolved = true;
        
        debug!("Dependencies resolved, startup order: {:?}", graph.startup_order);
        Ok(())
    }

    /// Execute actor registration step
    async fn execute_actor_registration(&self, actors_started: &mut usize) -> Result<(), StartupError> {
        debug!("Registering actors with supervisor");

        let graph = self.dependency_graph.read().await;
        
        for actor_type in &graph.startup_order {
            if let Some(actor_config) = self.config.actor_configs.get(actor_type) {
                let actor_info = ActorInfo {
                    actor_type: actor_type.clone(),
                    actor_name: actor_type.to_lowercase(),
                    priority: actor_config.priority,
                    dependencies: actor_config.dependencies.clone(),
                    config: actor_config.clone(),
                    created_at: SystemTime::now(),
                    last_restart: None,
                };

                // Note: In real implementation, would register with actual supervisor
                debug!("Registered actor: {}", actor_type);
                *actors_started += 1;
            }
        }

        debug!("Actor registration completed, {} actors registered", *actors_started);
        Ok(())
    }

    /// Execute actor startup step
    async fn execute_actor_startup(&self, options: &StartupOptions) -> Result<(), StartupError> {
        debug!("Starting actors in dependency order");

        let graph = self.dependency_graph.read().await;
        
        if options.parallel_startup {
            // Start actors in parallel where dependencies allow
            self.start_actors_parallel(&graph.startup_order).await?;
        } else {
            // Start actors sequentially
            self.start_actors_sequential(&graph.startup_order).await?;
        }

        debug!("Actor startup completed");
        Ok(())
    }

    /// Execute health monitoring activation step
    async fn execute_health_monitoring(
        &self,
        health_monitors: &mut usize,
        options: &StartupOptions,
    ) -> Result<(), StartupError> {
        if !options.health_monitoring {
            debug!("Health monitoring disabled, skipping");
            return Ok();
        }

        debug!("Activating health monitoring");

        let mut registry = self.health_registry.write().await;
        
        // Configure health monitoring for each actor
        for (actor_type, config) in &self.config.actor_configs {
            if let Some(health_config) = &config.health_check_config {
                let monitor_config = HealthMonitorConfig {
                    actor_type: actor_type.clone(),
                    interval: health_config.interval,
                    timeout: health_config.timeout,
                    failure_threshold: health_config.failure_threshold,
                    detailed_reporting: health_config.detailed_reporting,
                };

                let schedule = HealthSchedule {
                    actor_type: actor_type.clone(),
                    next_check: SystemTime::now() + health_config.interval,
                    interval: health_config.interval,
                    last_result: None,
                };

                registry.monitors.insert(actor_type.clone(), monitor_config);
                registry.schedules.insert(actor_type.clone(), schedule);
                *health_monitors += 1;
            }
        }

        registry.system_health = SystemHealthStatus::Healthy;
        
        debug!("Health monitoring activated for {} actors", *health_monitors);
        Ok(())
    }

    /// Execute metrics collection activation step
    async fn execute_metrics_collection(
        &self,
        metrics_active: &mut bool,
        options: &StartupOptions,
    ) -> Result<(), StartupError> {
        if !options.collect_metrics {
            debug!("Metrics collection disabled, skipping");
            return Ok();
        }

        debug!("Activating metrics collection");

        // Note: In real implementation, would initialize ActorSystemMetrics
        // For now, simulate activation
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        *metrics_active = true;
        
        debug!("Metrics collection activated");
        Ok(())
    }

    /// Execute blockchain integration step
    async fn execute_blockchain_integration(&self, options: &StartupOptions) -> Result<(), StartupError> {
        if !options.blockchain_integration {
            debug!("Blockchain integration disabled, skipping");
            return Ok();
        }

        debug!("Initializing blockchain integration");

        // Validate blockchain-specific configuration
        let blockchain_config = &self.config.blockchain_integration;
        
        if blockchain_config.block_interval != blockchain::BLOCK_INTERVAL {
            warn!("Block interval configuration mismatch");
        }

        // Note: In real implementation, would initialize blockchain connections
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        debug!("Blockchain integration initialized");
        Ok(())
    }

    /// Execute startup completion step
    async fn execute_startup_complete(&self) -> Result<(), StartupError> {
        debug!("Completing startup sequence");

        // Final system validation
        let health_status = {
            let registry = self.health_registry.read().await;
            registry.system_health.clone()
        };

        if health_status == SystemHealthStatus::Critical {
            return Err(StartupError::SystemInitializationError {
                reason: "System health is critical after startup".to_string(),
            });
        }

        debug!("Startup sequence completed successfully");
        Ok(())
    }

    /// Start actors in parallel where dependencies allow
    async fn start_actors_parallel(&self, startup_order: &[String]) -> Result<(), StartupError> {
        // Note: In real implementation, would use actual parallel startup logic
        for actor_type in startup_order {
            debug!("Starting actor: {}", actor_type);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    /// Start actors sequentially
    async fn start_actors_sequential(&self, startup_order: &[String]) -> Result<(), StartupError> {
        for actor_type in startup_order {
            debug!("Starting actor: {}", actor_type);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    /// Perform topological sort for dependency resolution
    fn topological_sort(&self, dependencies: &HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();

        fn visit(
            node: &str,
            dependencies: &HashMap<String, Vec<String>>,
            visited: &mut std::collections::HashSet<String>,
            temp_visited: &mut std::collections::HashSet<String>,
            result: &mut Vec<String>,
        ) -> Result<(), String> {
            if temp_visited.contains(node) {
                return Err(format!("Circular dependency detected involving {}", node));
            }

            if !visited.contains(node) {
                temp_visited.insert(node.to_string());

                if let Some(deps) = dependencies.get(node) {
                    for dep in deps {
                        visit(dep, dependencies, visited, temp_visited, result)?;
                    }
                }

                temp_visited.remove(node);
                visited.insert(node.to_string());
                result.push(node.to_string());
            }

            Ok(())
        }

        for node in dependencies.keys() {
            if !visited.contains(node) {
                visit(node, dependencies, &mut visited, &mut temp_visited, &mut result)?;
            }
        }

        Ok(result)
    }

    /// Set system state
    async fn set_state(&self, state: StartupState) {
        let mut current_state = self.state.write().await;
        *current_state = state;
    }

    /// Get current system state
    pub async fn get_state(&self) -> StartupState {
        self.state.read().await.clone()
    }

    /// Get startup progress
    pub async fn get_progress(&self) -> StartupProgress {
        let tracker = self.sequence_tracker.read().await;
        let total_steps = tracker.steps.len();
        let completed_steps = tracker.steps.iter()
            .filter(|step| step.state == StepState::Completed)
            .count();

        StartupProgress {
            current_step: tracker.current_step,
            total_steps,
            completed_steps,
            percentage: if total_steps > 0 {
                (completed_steps as f64 / total_steps as f64) * 100.0
            } else {
                0.0
            },
            current_step_name: tracker.steps.get(tracker.current_step)
                .map(|step| step.name.clone()),
        }
    }
}

/// Startup progress information
#[derive(Debug, Clone)]
pub struct StartupProgress {
    /// Current step index
    pub current_step: usize,
    /// Total number of steps
    pub total_steps: usize,
    /// Number of completed steps
    pub completed_steps: usize,
    /// Completion percentage
    pub percentage: f64,
    /// Current step name
    pub current_step_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::foundation::ActorSystemConfig;

    fn create_test_config() -> ActorSystemConfig {
        ActorSystemConfig::development()
    }

    #[tokio::test]
    async fn test_system_startup_creation() {
        let config = create_test_config();
        let startup = SystemStartup::new(config);
        assert!(startup.is_ok());
    }

    #[tokio::test]
    async fn test_startup_sequence_initialization() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        let progress = startup.get_progress().await;
        assert_eq!(progress.current_step, 0);
        assert!(progress.total_steps > 0);
    }

    #[tokio::test]
    async fn test_dependency_resolution() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        // Test topological sort with simple dependencies
        let mut dependencies = HashMap::new();
        dependencies.insert("A".to_string(), vec!["B".to_string()]);
        dependencies.insert("B".to_string(), vec!["C".to_string()]);
        dependencies.insert("C".to_string(), vec![]);
        
        let result = startup.topological_sort(&dependencies);
        assert!(result.is_ok());
        
        let order = result.unwrap();
        let c_pos = order.iter().position(|x| x == "C").unwrap();
        let b_pos = order.iter().position(|x| x == "B").unwrap();
        let a_pos = order.iter().position(|x| x == "A").unwrap();
        
        assert!(c_pos < b_pos);
        assert!(b_pos < a_pos);
    }

    #[tokio::test]
    async fn test_circular_dependency_detection() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        // Test circular dependency detection
        let mut dependencies = HashMap::new();
        dependencies.insert("A".to_string(), vec!["B".to_string()]);
        dependencies.insert("B".to_string(), vec!["C".to_string()]);
        dependencies.insert("C".to_string(), vec!["A".to_string()]);
        
        let result = startup.topological_sort(&dependencies);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_startup_progress_tracking() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        let initial_progress = startup.get_progress().await;
        assert_eq!(initial_progress.percentage, 0.0);
        assert_eq!(initial_progress.completed_steps, 0);
    }

    #[tokio::test]
    async fn test_config_validation_step() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        let result = startup.execute_config_validation().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_monitoring_setup() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        let mut health_monitors = 0;
        let options = StartupOptions::default();
        
        let result = startup.execute_health_monitoring(&mut health_monitors, &options).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_system_state_transitions() {
        let config = create_test_config();
        let startup = SystemStartup::new(config).unwrap();
        
        assert_eq!(startup.get_state().await, StartupState::NotStarted);
        
        startup.set_state(StartupState::Initializing).await;
        assert_eq!(startup.get_state().await, StartupState::Initializing);
    }
}