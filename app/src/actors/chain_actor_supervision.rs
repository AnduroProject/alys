//! ChainActor supervision integration
//!
//! This module provides integration between ChainActor and the Alys supervision system,
//! including health monitoring, restart strategies, and fault tolerance mechanisms.

use super::chain_actor::*;
use super::supervisor::*;
use crate::messages::{chain_messages::*, system_messages::*};
use crate::types::{blockchain::*, errors::*};

use actix::prelude::*;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, info, warn, error};
use uuid::Uuid;

/// ChainActor supervision configuration
#[derive(Debug, Clone)]
pub struct ChainSupervisionConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    
    /// Health check timeout
    pub health_check_timeout: Duration,
    
    /// Maximum consecutive failed health checks before considering actor unhealthy
    pub max_failed_health_checks: u32,
    
    /// Recovery strategy when actor becomes unhealthy
    pub recovery_strategy: ChainRecoveryStrategy,
    
    /// Performance thresholds for health monitoring
    pub performance_thresholds: PerformanceThresholds,
    
    /// Enable automatic state checkpoint creation
    pub enable_checkpoints: bool,
    
    /// Checkpoint interval
    pub checkpoint_interval: Duration,
}

impl Default for ChainSupervisionConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(10),
            health_check_timeout: Duration::from_secs(5),
            max_failed_health_checks: 3,
            recovery_strategy: ChainRecoveryStrategy::Restart,
            performance_thresholds: PerformanceThresholds::default(),
            enable_checkpoints: true,
            checkpoint_interval: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Performance thresholds for health monitoring
#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    /// Maximum block processing time before considered degraded
    pub max_block_processing_time: Duration,
    
    /// Maximum memory usage (MB) before considered degraded
    pub max_memory_usage_mb: u64,
    
    /// Maximum queue size before considered degraded
    pub max_queue_size: usize,
    
    /// Maximum error rate (per minute) before considered degraded
    pub max_error_rate_per_minute: u32,
    
    /// Minimum throughput (operations per second) before considered degraded
    pub min_throughput_ops_per_second: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_block_processing_time: Duration::from_millis(1000),
            max_memory_usage_mb: 512,
            max_queue_size: 1000,
            max_error_rate_per_minute: 10,
            min_throughput_ops_per_second: 1.0,
        }
    }
}

/// Recovery strategies for unhealthy ChainActor
#[derive(Debug, Clone)]
pub enum ChainRecoveryStrategy {
    /// Restart the actor with clean state
    Restart,
    /// Attempt to restore from last checkpoint
    RestoreFromCheckpoint,
    /// Gradual recovery with reduced load
    GradualRecovery,
    /// Switch to degraded mode with limited functionality
    DegradedMode,
}

/// ChainActor health status with detailed metrics
#[derive(Debug, Clone)]
pub struct ChainActorHealth {
    /// Overall health status
    pub status: ActorHealth,
    
    /// Last health check timestamp
    pub last_check: SystemTime,
    
    /// Performance metrics
    pub performance_metrics: ChainPerformanceMetrics,
    
    /// Error metrics
    pub error_metrics: ChainErrorMetrics,
    
    /// State integrity status
    pub state_integrity: StateIntegrityStatus,
    
    /// Resource usage metrics
    pub resource_usage: ResourceUsageMetrics,
}

/// Performance metrics for health monitoring
#[derive(Debug, Clone, Default)]
pub struct ChainPerformanceMetrics {
    /// Average block processing time
    pub avg_block_processing_time: Duration,
    
    /// Block processing throughput (blocks per second)
    pub block_throughput: f64,
    
    /// Current queue size
    pub queue_size: usize,
    
    /// Operations per second
    pub operations_per_second: f64,
    
    /// Last processing time measurement
    pub last_processing_time: Option<Duration>,
}

/// Error metrics for health monitoring
#[derive(Debug, Clone, Default)]
pub struct ChainErrorMetrics {
    /// Total errors in the last minute
    pub errors_per_minute: u32,
    
    /// Total errors since last reset
    pub total_errors: u64,
    
    /// Error rate (errors per operation)
    pub error_rate: f64,
    
    /// Last error timestamp
    pub last_error_time: Option<SystemTime>,
    
    /// Error categories breakdown
    pub error_breakdown: std::collections::HashMap<String, u32>,
}

/// State integrity status
#[derive(Debug, Clone)]
pub enum StateIntegrityStatus {
    Consistent,
    MinorInconsistency { details: String },
    MajorInconsistency { details: String },
    Corrupted { details: String },
}

impl Default for StateIntegrityStatus {
    fn default() -> Self {
        StateIntegrityStatus::Consistent
    }
}

/// Resource usage metrics
#[derive(Debug, Clone, Default)]
pub struct ResourceUsageMetrics {
    /// Current memory usage in MB
    pub memory_usage_mb: u64,
    
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    
    /// File descriptor count
    pub file_descriptors: u32,
    
    /// Network connection count
    pub network_connections: u32,
}

/// Supervised ChainActor wrapper
pub struct SupervisedChainActor {
    /// The actual ChainActor
    chain_actor: Addr<ChainActor>,
    
    /// Supervision configuration
    supervision_config: ChainSupervisionConfig,
    
    /// Health status tracking
    health_status: ChainActorHealth,
    
    /// Consecutive failed health checks
    failed_health_checks: u32,
    
    /// Last checkpoint timestamp
    last_checkpoint: Option<SystemTime>,
    
    /// Supervisor address
    supervisor: Addr<AlysRootSupervisor>,
}

impl Actor for SupervisedChainActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("SupervisedChainActor started");
        
        // Start periodic health checks
        self.start_health_monitoring(ctx);
        
        // Start periodic checkpoints if enabled
        if self.supervision_config.enable_checkpoints {
            self.start_checkpoint_creation(ctx);
        }
        
        // Register with supervisor
        self.register_with_supervisor(ctx);
    }
}

impl SupervisedChainActor {
    /// Create a new supervised ChainActor
    pub fn new(
        chain_actor: Addr<ChainActor>,
        supervision_config: ChainSupervisionConfig,
        supervisor: Addr<AlysRootSupervisor>,
    ) -> Self {
        let health_status = ChainActorHealth {
            status: ActorHealth::Healthy,
            last_check: SystemTime::now(),
            performance_metrics: ChainPerformanceMetrics::default(),
            error_metrics: ChainErrorMetrics::default(),
            state_integrity: StateIntegrityStatus::default(),
            resource_usage: ResourceUsageMetrics::default(),
        };

        Self {
            chain_actor,
            supervision_config,
            health_status,
            failed_health_checks: 0,
            last_checkpoint: None,
            supervisor,
        }
    }

    /// Start health monitoring
    fn start_health_monitoring(&self, ctx: &mut Context<Self>) {
        let interval = self.supervision_config.health_check_interval;
        
        ctx.run_interval(interval, |actor, ctx| {
            debug!("Performing ChainActor health check");
            
            let health_check_msg = PerformHealthCheck {
                correlation_id: Some(Uuid::new_v4()),
                include_detailed_metrics: true,
                timeout: actor.supervision_config.health_check_timeout,
            };
            
            let future = actor.chain_actor
                .send(health_check_msg)
                .into_actor(actor)
                .timeout(actor.supervision_config.health_check_timeout)
                .then(|result, actor, _ctx| {
                    actor.handle_health_check_result(result);
                    actix::fut::ready(())
                });
            
            ctx.spawn(future);
        });
    }

    /// Start checkpoint creation
    fn start_checkpoint_creation(&self, ctx: &mut Context<Self>) {
        let interval = self.supervision_config.checkpoint_interval;
        
        ctx.run_interval(interval, |actor, ctx| {
            debug!("Creating ChainActor checkpoint");
            
            let checkpoint_msg = CreateStateCheckpoint {
                checkpoint_id: format!("checkpoint_{}", SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()),
                correlation_id: Some(Uuid::new_v4()),
            };
            
            let future = actor.chain_actor
                .send(checkpoint_msg)
                .into_actor(actor)
                .then(|result, actor, _ctx| {
                    match result {
                        Ok(Ok(_)) => {
                            actor.last_checkpoint = Some(SystemTime::now());
                            debug!("ChainActor checkpoint created successfully");
                        },
                        Ok(Err(e)) => {
                            warn!("Failed to create ChainActor checkpoint: {}", e);
                        },
                        Err(e) => {
                            warn!("Checkpoint message delivery failed: {}", e);
                        }
                    }
                    actix::fut::ready(())
                });
            
            ctx.spawn(future);
        });
    }

    /// Register with supervisor
    fn register_with_supervisor(&self, ctx: &mut Context<Self>) {
        let register_msg = RegisterActor {
            actor_name: "ChainActor".to_string(),
            actor_type: ActorType::Chain,
            actor_address: ctx.address().recipient(),
            restart_policy: RestartPolicy::OneForOne,
            metadata: std::collections::HashMap::new(),
        };

        let future = self.supervisor
            .send(register_msg)
            .into_actor(self)
            .then(|result, _actor, _ctx| {
                match result {
                    Ok(Ok(_)) => {
                        info!("ChainActor successfully registered with supervisor");
                    },
                    Ok(Err(e)) => {
                        error!("Failed to register ChainActor with supervisor: {}", e);
                    },
                    Err(e) => {
                        error!("Supervisor registration message delivery failed: {}", e);
                    }
                }
                actix::fut::ready(())
            });

        ctx.spawn(future);
    }

    /// Handle health check result
    fn handle_health_check_result(&mut self, result: Result<Result<ChainActorHealth, ChainError>, actix::MailboxError>) {
        match result {
            Ok(Ok(health)) => {
                // Health check successful
                self.health_status = health;
                self.failed_health_checks = 0;
                
                // Analyze health status
                let overall_health = self.analyze_health_status();
                self.health_status.status = overall_health.clone();
                
                if !matches!(overall_health, ActorHealth::Healthy) {
                    self.handle_degraded_health(overall_health);
                }
                
                debug!("ChainActor health check completed: {:?}", self.health_status.status);
            },
            Ok(Err(e)) => {
                // Health check returned an error
                self.failed_health_checks += 1;
                self.health_status.status = ActorHealth::Failed { 
                    error: format!("Health check failed: {}", e) 
                };
                
                warn!(
                    failed_checks = self.failed_health_checks,
                    max_failed = self.supervision_config.max_failed_health_checks,
                    "ChainActor health check failed: {}", e
                );
                
                if self.failed_health_checks >= self.supervision_config.max_failed_health_checks {
                    self.trigger_recovery();
                }
            },
            Err(e) => {
                // Health check message delivery failed
                self.failed_health_checks += 1;
                self.health_status.status = ActorHealth::Failed { 
                    error: format!("Health check message delivery failed: {}", e) 
                };
                
                error!(
                    failed_checks = self.failed_health_checks,
                    "ChainActor health check message delivery failed: {}", e
                );
                
                if self.failed_health_checks >= self.supervision_config.max_failed_health_checks {
                    self.trigger_recovery();
                }
            }
        }
        
        self.health_status.last_check = SystemTime::now();
    }

    /// Analyze overall health status based on metrics
    fn analyze_health_status(&self) -> ActorHealth {
        let metrics = &self.health_status.performance_metrics;
        let thresholds = &self.supervision_config.performance_thresholds;
        let mut issues = Vec::new();

        // Check performance thresholds
        if metrics.avg_block_processing_time > thresholds.max_block_processing_time {
            issues.push(format!("Block processing time too high: {:?}", metrics.avg_block_processing_time));
        }

        if self.health_status.resource_usage.memory_usage_mb > thresholds.max_memory_usage_mb {
            issues.push(format!("Memory usage too high: {} MB", self.health_status.resource_usage.memory_usage_mb));
        }

        if metrics.queue_size > thresholds.max_queue_size {
            issues.push(format!("Queue size too high: {}", metrics.queue_size));
        }

        if self.health_status.error_metrics.errors_per_minute > thresholds.max_error_rate_per_minute {
            issues.push(format!("Error rate too high: {} errors/min", self.health_status.error_metrics.errors_per_minute));
        }

        if metrics.operations_per_second < thresholds.min_throughput_ops_per_second {
            issues.push(format!("Throughput too low: {} ops/sec", metrics.operations_per_second));
        }

        // Check state integrity
        match &self.health_status.state_integrity {
            StateIntegrityStatus::Consistent => {},
            StateIntegrityStatus::MinorInconsistency { details } => {
                issues.push(format!("Minor state inconsistency: {}", details));
            },
            StateIntegrityStatus::MajorInconsistency { details } => {
                return ActorHealth::Failed { error: format!("Major state inconsistency: {}", details) };
            },
            StateIntegrityStatus::Corrupted { details } => {
                return ActorHealth::Failed { error: format!("State corrupted: {}", details) };
            },
        }

        if issues.is_empty() {
            ActorHealth::Healthy
        } else {
            ActorHealth::Degraded { reason: issues.join("; ") }
        }
    }

    /// Handle degraded health status
    fn handle_degraded_health(&self, health_status: ActorHealth) {
        match health_status {
            ActorHealth::Degraded { ref reason } => {
                warn!("ChainActor is in degraded state: {}", reason);
                
                // Report to supervisor
                let health_report = ActorHealthReport {
                    actor_name: "ChainActor".to_string(),
                    health_status: health_status.clone(),
                    metrics: Some(serde_json::to_value(&self.health_status.performance_metrics).unwrap()),
                    timestamp: SystemTime::now(),
                    correlation_id: Some(Uuid::new_v4()),
                };

                let _ = self.supervisor.try_send(health_report);
            },
            _ => {}
        }
    }

    /// Trigger recovery process
    fn trigger_recovery(&self) {
        error!("Triggering ChainActor recovery due to consecutive health check failures");
        
        match self.supervision_config.recovery_strategy {
            ChainRecoveryStrategy::Restart => {
                self.request_actor_restart();
            },
            ChainRecoveryStrategy::RestoreFromCheckpoint => {
                self.request_checkpoint_restore();
            },
            ChainRecoveryStrategy::GradualRecovery => {
                self.initiate_gradual_recovery();
            },
            ChainRecoveryStrategy::DegradedMode => {
                self.switch_to_degraded_mode();
            },
        }
    }

    /// Request actor restart from supervisor
    fn request_actor_restart(&self) {
        let restart_request = RestartActorRequest {
            actor_name: "ChainActor".to_string(),
            restart_reason: "Health check failures exceeded threshold".to_string(),
            preserve_state: false,
            correlation_id: Some(Uuid::new_v4()),
        };

        let _ = self.supervisor.try_send(restart_request);
    }

    /// Request checkpoint restore
    fn request_checkpoint_restore(&self) {
        if let Some(checkpoint_time) = self.last_checkpoint {
            info!("Attempting to restore ChainActor from checkpoint");
            
            let restore_msg = RestoreFromCheckpoint {
                checkpoint_id: format!("checkpoint_{}", 
                    checkpoint_time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()),
                correlation_id: Some(Uuid::new_v4()),
            };

            let _ = self.chain_actor.try_send(restore_msg);
        } else {
            warn!("No checkpoint available for restore, falling back to restart");
            self.request_actor_restart();
        }
    }

    /// Initiate gradual recovery
    fn initiate_gradual_recovery(&self) {
        info!("Initiating gradual recovery for ChainActor");
        
        let recovery_msg = InitiateGradualRecovery {
            recovery_steps: vec![
                "Reduce processing load".to_string(),
                "Clear error conditions".to_string(),
                "Restart services gradually".to_string(),
                "Resume normal operation".to_string(),
            ],
            correlation_id: Some(Uuid::new_v4()),
        };

        let _ = self.chain_actor.try_send(recovery_msg);
    }

    /// Switch to degraded mode
    fn switch_to_degraded_mode(&self) {
        warn!("Switching ChainActor to degraded mode");
        
        let degraded_mode_msg = SwitchToDegradedMode {
            degraded_features: vec![
                "Block production".to_string(),
                "Complex validations".to_string(),
            ],
            essential_features: vec![
                "Block import".to_string(),
                "Chain status".to_string(),
            ],
            correlation_id: Some(Uuid::new_v4()),
        };

        let _ = self.chain_actor.try_send(degraded_mode_msg);
    }
}

/// Message handlers for supervision integration

impl Handler<GetActorHealth> for SupervisedChainActor {
    type Result = MessageResult<GetActorHealth>;

    fn handle(&mut self, _msg: GetActorHealth, _ctx: &mut Context<Self>) -> Self::Result {
        MessageResult(Ok(self.health_status.clone()))
    }
}

impl Handler<ActorShutdown> for SupervisedChainActor {
    type Result = MessageResult<ActorShutdown>;

    fn handle(&mut self, msg: ActorShutdown, ctx: &mut Context<Self>) -> Self::Result {
        info!("Received shutdown request for SupervisedChainActor: {}", msg.reason);
        
        // Shutdown the supervised ChainActor
        let shutdown_msg = ActorShutdown {
            reason: msg.reason.clone(),
            graceful: msg.graceful,
            timeout: msg.timeout,
        };
        
        let _ = self.chain_actor.try_send(shutdown_msg);
        
        // Stop this supervisor
        ctx.stop();
        
        MessageResult(Ok(()))
    }
}

// Additional message types for supervision

#[derive(Message)]
#[rtype(result = "Result<ChainActorHealth, ChainError>")]
pub struct PerformHealthCheck {
    pub correlation_id: Option<Uuid>,
    pub include_detailed_metrics: bool,
    pub timeout: Duration,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct CreateStateCheckpoint {
    pub checkpoint_id: String,
    pub correlation_id: Option<Uuid>,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct RestoreFromCheckpoint {
    pub checkpoint_id: String,
    pub correlation_id: Option<Uuid>,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct InitiateGradualRecovery {
    pub recovery_steps: Vec<String>,
    pub correlation_id: Option<Uuid>,
}

#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct SwitchToDegradedMode {
    pub degraded_features: Vec<String>,
    pub essential_features: Vec<String>,
    pub correlation_id: Option<Uuid>,
}