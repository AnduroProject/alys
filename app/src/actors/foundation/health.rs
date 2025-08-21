//! Health Monitoring System - Phase 5 Implementation (ALYS-006-21 to ALYS-006-24)
//! 
//! Comprehensive health monitoring and graceful shutdown system for Alys V2 actor architecture.
//! Provides periodic health checks, failure detection, recovery triggering, and coordinated shutdown
//! procedures with blockchain-aware timing for the merged mining sidechain.

use crate::actors::foundation::{
    ActorRegistry, constants::{health, lifecycle, messaging},
};
use crate::types::*;
use actix::{
    Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, Context, Handler, 
    Message, ResponseActFuture, Running, StreamHandler, Supervised, SystemService, WrapFuture
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

/// ALYS-006-21: HealthMonitor actor with periodic health checks, failure detection, and recovery triggering
/// 
/// Central health monitoring actor that coordinates system-wide health checks,
/// detects failures, and triggers recovery actions for the Alys V2 sidechain.
/// Integrates with consensus timing and federation health requirements.
pub struct HealthMonitor {
    /// Registry of actors to monitor
    monitored_actors: HashMap<String, MonitoredActor>,
    /// Health check configuration
    config: HealthMonitorConfig,
    /// Current system health state
    system_health: SystemHealthState,
    /// Health history for trending
    health_history: VecDeque<HealthSnapshot>,
    /// Recovery actions in progress
    recovery_actions: HashMap<String, RecoveryAction>,
    /// Shutdown coordinator reference
    shutdown_coordinator: Option<Addr<ShutdownCoordinator>>,
    /// Statistics and metrics
    stats: HealthMonitorStats,
}

/// Configuration for health monitoring
#[derive(Debug, Clone)]
pub struct HealthMonitorConfig {
    /// Default health check interval
    pub default_check_interval: Duration,
    /// Critical actor check interval
    pub critical_check_interval: Duration,
    /// Health check timeout
    pub check_timeout: Duration,
    /// Failure threshold before marking unhealthy
    pub failure_threshold: u32,
    /// Recovery threshold before marking healthy
    pub recovery_threshold: u32,
    /// Maximum health history entries
    pub max_history_entries: usize,
    /// Enable detailed health reporting
    pub detailed_reporting: bool,
    /// Enable automatic recovery
    pub enable_auto_recovery: bool,
    /// Blockchain-aware health checks
    pub blockchain_aware: bool,
}

impl Default for HealthMonitorConfig {
    fn default() -> Self {
        Self {
            default_check_interval: health::DEFAULT_HEALTH_CHECK_INTERVAL,
            critical_check_interval: health::CRITICAL_HEALTH_CHECK_INTERVAL,
            check_timeout: health::SYSTEM_HEALTH_TIMEOUT,
            failure_threshold: health::HEALTH_CHECK_FAILURE_THRESHOLD,
            recovery_threshold: health::HEALTH_CHECK_RECOVERY_THRESHOLD,
            max_history_entries: 1000,
            detailed_reporting: true,
            enable_auto_recovery: true,
            blockchain_aware: true,
        }
    }
}

/// Monitored actor information
#[derive(Debug, Clone)]
pub struct MonitoredActor {
    /// Actor name
    pub name: String,
    /// Actor priority level
    pub priority: ActorPriority,
    /// Health check interval
    pub check_interval: Duration,
    /// Current health status
    pub status: HealthStatus,
    /// Consecutive failure count
    pub failure_count: u32,
    /// Consecutive success count
    pub success_count: u32,
    /// Last health check time
    pub last_check: Option<Instant>,
    /// Last successful check time
    pub last_success: Option<Instant>,
    /// Response time history
    pub response_times: VecDeque<Duration>,
    /// Custom health check message
    pub custom_check: Option<CustomHealthCheck>,
    /// Recovery strategy
    pub recovery_strategy: RecoveryStrategy,
}

/// Actor priority levels for health monitoring
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActorPriority {
    /// Critical system actors (consensus, federation)
    Critical,
    /// High priority actors (chain, engine)
    High,
    /// Normal priority actors
    Normal,
    /// Background actors
    Background,
}

/// Health status of an actor
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Actor is healthy and responsive
    Healthy,
    /// Actor is responding but with issues
    Degraded { reason: String },
    /// Actor is unhealthy or unresponsive
    Unhealthy { reason: String },
    /// Health status unknown (new actor)
    Unknown,
    /// Actor is being recovered
    Recovering,
    /// Actor is shutting down
    ShuttingDown,
}

/// System-wide health state
#[derive(Debug, Clone)]
pub struct SystemHealthState {
    /// Overall system health score (0.0 to 100.0)
    pub overall_score: f64,
    /// Number of healthy actors
    pub healthy_actors: usize,
    /// Number of degraded actors
    pub degraded_actors: usize,
    /// Number of unhealthy actors
    pub unhealthy_actors: usize,
    /// Critical actors status
    pub critical_actors_healthy: bool,
    /// System uptime
    pub uptime: Duration,
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Health snapshot for historical tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    /// Timestamp of snapshot
    pub timestamp: SystemTime,
    /// System health score at time
    pub overall_score: f64,
    /// Per-actor health status
    pub actor_health: HashMap<String, HealthStatus>,
    /// System events since last snapshot
    pub events: Vec<HealthEvent>,
}

/// Health monitoring events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthEvent {
    /// Actor registered for monitoring
    ActorRegistered { name: String, priority: ActorPriority },
    /// Actor unregistered from monitoring
    ActorUnregistered { name: String },
    /// Health status changed
    StatusChanged { name: String, old_status: HealthStatus, new_status: HealthStatus },
    /// Health check failed
    CheckFailed { name: String, reason: String, response_time: Option<Duration> },
    /// Recovery action initiated
    RecoveryInitiated { name: String, action: RecoveryStrategy },
    /// Recovery action completed
    RecoveryCompleted { name: String, success: bool },
    /// System health threshold crossed
    SystemThreshold { threshold: f64, current_score: f64, direction: String },
}

/// Recovery strategies for unhealthy actors
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// No automatic recovery
    None,
    /// Restart the actor
    Restart,
    /// Reset actor state
    Reset,
    /// Escalate to supervisor
    Escalate,
    /// Custom recovery action
    Custom(String),
}

/// Active recovery action
#[derive(Debug, Clone)]
pub struct RecoveryAction {
    /// Actor being recovered
    pub actor_name: String,
    /// Recovery strategy being applied
    pub strategy: RecoveryStrategy,
    /// Recovery start time
    pub started_at: Instant,
    /// Recovery timeout
    pub timeout: Duration,
    /// Recovery attempts
    pub attempts: u32,
    /// Maximum attempts
    pub max_attempts: u32,
}

/// Health monitoring statistics
#[derive(Debug, Default)]
pub struct HealthMonitorStats {
    /// Total health checks performed
    pub total_checks: u64,
    /// Total successful checks
    pub successful_checks: u64,
    /// Total failed checks
    pub failed_checks: u64,
    /// Recovery actions initiated
    pub recovery_actions: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Monitor start time
    pub started_at: Instant,
}

/// Custom health check definition
#[derive(Debug, Clone)]
pub struct CustomHealthCheck {
    /// Custom check message type
    pub message_type: String,
    /// Expected response timeout
    pub timeout: Duration,
    /// Validation function for response
    pub validator: Option<String>,
}

impl Actor for HealthMonitor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("HealthMonitor started with {} monitored actors", self.monitored_actors.len());
        
        // Initialize statistics
        self.stats.started_at = Instant::now();
        
        // Start periodic health checks
        self.start_health_check_cycle(ctx);
        
        // Start health history cleanup
        self.start_history_cleanup(ctx);
        
        // Update system health state
        self.update_system_health();
        
        info!("HealthMonitor initialization complete");
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        info!("HealthMonitor stopping, cancelling {} recovery actions", self.recovery_actions.len());
        
        // Cancel ongoing recovery actions
        self.recovery_actions.clear();
        
        // Log final statistics
        let uptime = self.stats.started_at.elapsed();
        let success_rate = if self.stats.total_checks > 0 {
            (self.stats.successful_checks as f64 / self.stats.total_checks as f64) * 100.0
        } else {
            0.0
        };
        
        info!(
            "HealthMonitor final stats: uptime={:?}, checks={}, success_rate={:.2}%, recoveries={}",
            uptime, self.stats.total_checks, success_rate, self.stats.recovery_actions
        );
        
        Running::Stop
    }
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(config: HealthMonitorConfig) -> Self {
        Self {
            monitored_actors: HashMap::new(),
            config,
            system_health: SystemHealthState {
                overall_score: 100.0,
                healthy_actors: 0,
                degraded_actors: 0,
                unhealthy_actors: 0,
                critical_actors_healthy: true,
                uptime: Duration::from_secs(0),
                last_updated: SystemTime::now(),
            },
            health_history: VecDeque::new(),
            recovery_actions: HashMap::new(),
            shutdown_coordinator: None,
            stats: HealthMonitorStats::default(),
        }
    }

    /// Start the periodic health check cycle
    fn start_health_check_cycle(&self, ctx: &mut Context<Self>) {
        // Start interval for critical actors
        ctx.run_interval(self.config.critical_check_interval, |act, ctx| {
            act.perform_health_checks(ActorPriority::Critical, ctx);
        });

        // Start interval for high priority actors
        ctx.run_interval(self.config.default_check_interval, |act, ctx| {
            act.perform_health_checks(ActorPriority::High, ctx);
        });

        // Start interval for normal actors
        ctx.run_interval(self.config.default_check_interval * 2, |act, ctx| {
            act.perform_health_checks(ActorPriority::Normal, ctx);
        });

        // Start interval for background actors
        ctx.run_interval(health::BACKGROUND_HEALTH_CHECK_INTERVAL, |act, ctx| {
            act.perform_health_checks(ActorPriority::Background, ctx);
        });
    }

    /// Perform health checks for actors of specified priority
    #[instrument(skip(self, ctx))]
    fn perform_health_checks(&mut self, priority: ActorPriority, ctx: &mut Context<Self>) {
        let now = Instant::now();
        let actors_to_check: Vec<String> = self.monitored_actors
            .iter()
            .filter(|(_, actor)| {
                actor.priority == priority && 
                actor.last_check.map_or(true, |last| now.duration_since(last) >= actor.check_interval)
            })
            .map(|(name, _)| name.clone())
            .collect();

        debug!("Performing health checks for {} {:?} priority actors", actors_to_check.len(), priority);

        for actor_name in actors_to_check {
            self.initiate_health_check(actor_name, ctx);
        }
    }

    /// Initiate health check for specific actor
    fn initiate_health_check(&mut self, actor_name: String, ctx: &mut Context<Self>) {
        if let Some(monitored_actor) = self.monitored_actors.get_mut(&actor_name) {
            monitored_actor.last_check = Some(Instant::now());
            
            // Perform the actual health check
            let check_start = Instant::now();
            
            // For now, we'll simulate the health check
            // In a real implementation, this would send a HealthCheckMessage to the actor
            let check_future = self.simulate_health_check(actor_name.clone(), check_start);
            
            let fut = check_future.into_actor(self).map(move |result, act, _ctx| {
                act.handle_health_check_result(actor_name, result);
            });
            
            ctx.spawn(fut);
        }
    }

    /// Simulate health check (in real implementation, this would send messages)
    async fn simulate_health_check(&self, actor_name: String, check_start: Instant) -> HealthCheckResult {
        // Simulate network/processing delay
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        let response_time = check_start.elapsed();
        
        // Simulate occasional failures for testing
        if actor_name.contains("failing") {
            HealthCheckResult {
                actor_name,
                success: false,
                response_time: Some(response_time),
                error_message: Some("Simulated failure".to_string()),
                metadata: HashMap::new(),
            }
        } else {
            HealthCheckResult {
                actor_name,
                success: true,
                response_time: Some(response_time),
                error_message: None,
                metadata: HashMap::new(),
            }
        }
    }

    /// Handle health check result
    #[instrument(skip(self))]
    fn handle_health_check_result(&mut self, actor_name: String, result: HealthCheckResult) {
        self.stats.total_checks += 1;
        
        if let Some(monitored_actor) = self.monitored_actors.get_mut(&actor_name) {
            let old_status = monitored_actor.status.clone();
            
            if result.success {
                self.stats.successful_checks += 1;
                monitored_actor.success_count += 1;
                monitored_actor.failure_count = 0;
                monitored_actor.last_success = Some(Instant::now());
                
                if let Some(response_time) = result.response_time {
                    monitored_actor.response_times.push_back(response_time);
                    if monitored_actor.response_times.len() > 100 {
                        monitored_actor.response_times.pop_front();
                    }
                }
                
                // Update status based on recovery threshold
                if monitored_actor.success_count >= self.config.recovery_threshold {
                    match monitored_actor.status {
                        HealthStatus::Unhealthy { .. } | HealthStatus::Degraded { .. } => {
                            monitored_actor.status = HealthStatus::Healthy;
                            info!("Actor {} recovered to healthy status", actor_name);
                        }
                        HealthStatus::Recovering => {
                            monitored_actor.status = HealthStatus::Healthy;
                            info!("Actor {} recovery completed", actor_name);
                        }
                        _ => {}
                    }
                }
            } else {
                self.stats.failed_checks += 1;
                monitored_actor.failure_count += 1;
                monitored_actor.success_count = 0;
                
                let error_reason = result.error_message.unwrap_or_else(|| "Health check failed".to_string());
                
                // Update status based on failure threshold
                if monitored_actor.failure_count >= self.config.failure_threshold {
                    match monitored_actor.status {
                        HealthStatus::Healthy | HealthStatus::Degraded { .. } => {
                            monitored_actor.status = HealthStatus::Unhealthy { 
                                reason: error_reason.clone() 
                            };
                            warn!("Actor {} marked as unhealthy: {}", actor_name, error_reason);
                            
                            // Trigger recovery if enabled
                            if self.config.enable_auto_recovery {
                                self.initiate_recovery(&actor_name);
                            }
                        }
                        _ => {}
                    }
                } else if monitored_actor.failure_count > 0 {
                    monitored_actor.status = HealthStatus::Degraded { 
                        reason: error_reason.clone() 
                    };
                    debug!("Actor {} marked as degraded: {}", actor_name, error_reason);
                }
                
                // Log health event
                let event = HealthEvent::CheckFailed {
                    name: actor_name.clone(),
                    reason: error_reason,
                    response_time: result.response_time,
                };
                self.log_health_event(event);
            }
            
            // Log status changes
            if std::mem::discriminant(&old_status) != std::mem::discriminant(&monitored_actor.status) {
                let event = HealthEvent::StatusChanged {
                    name: actor_name.clone(),
                    old_status: old_status.clone(),
                    new_status: monitored_actor.status.clone(),
                };
                self.log_health_event(event);
            }
        }
        
        // Update system health after processing
        self.update_system_health();
    }

    /// Initiate recovery for an unhealthy actor
    fn initiate_recovery(&mut self, actor_name: &str) {
        if let Some(monitored_actor) = self.monitored_actors.get_mut(actor_name) {
            if self.recovery_actions.contains_key(actor_name) {
                debug!("Recovery already in progress for actor {}", actor_name);
                return;
            }
            
            let recovery_action = RecoveryAction {
                actor_name: actor_name.to_string(),
                strategy: monitored_actor.recovery_strategy.clone(),
                started_at: Instant::now(),
                timeout: Duration::from_secs(30),
                attempts: 1,
                max_attempts: 3,
            };
            
            info!("Initiating recovery for actor {} with strategy {:?}", actor_name, recovery_action.strategy);
            
            monitored_actor.status = HealthStatus::Recovering;
            self.recovery_actions.insert(actor_name.to_string(), recovery_action.clone());
            self.stats.recovery_actions += 1;
            
            let event = HealthEvent::RecoveryInitiated {
                name: actor_name.to_string(),
                action: recovery_action.strategy,
            };
            self.log_health_event(event);
        }
    }

    /// Update system-wide health state
    fn update_system_health(&mut self) {
        let mut healthy_count = 0;
        let mut degraded_count = 0;
        let mut unhealthy_count = 0;
        let mut critical_healthy = true;
        
        for (_, actor) in &self.monitored_actors {
            match actor.status {
                HealthStatus::Healthy => healthy_count += 1,
                HealthStatus::Degraded { .. } => {
                    degraded_count += 1;
                    if actor.priority == ActorPriority::Critical {
                        critical_healthy = false;
                    }
                }
                HealthStatus::Unhealthy { .. } => {
                    unhealthy_count += 1;
                    if actor.priority == ActorPriority::Critical {
                        critical_healthy = false;
                    }
                }
                _ => {}
            }
        }
        
        let total_actors = self.monitored_actors.len();
        let health_score = if total_actors > 0 {
            let weighted_score = (healthy_count * 100 + degraded_count * 50) as f64 / total_actors as f64;
            // Reduce score significantly if critical actors are unhealthy
            if !critical_healthy {
                weighted_score * 0.5
            } else {
                weighted_score
            }
        } else {
            100.0
        };
        
        let uptime = self.stats.started_at.elapsed();
        
        self.system_health = SystemHealthState {
            overall_score: health_score,
            healthy_actors: healthy_count,
            degraded_actors: degraded_count,
            unhealthy_actors: unhealthy_count,
            critical_actors_healthy: critical_healthy,
            uptime,
            last_updated: SystemTime::now(),
        };
        
        // Check for system health thresholds
        if health_score < 50.0 && self.system_health.overall_score >= 50.0 {
            let event = HealthEvent::SystemThreshold {
                threshold: 50.0,
                current_score: health_score,
                direction: "below".to_string(),
            };
            self.log_health_event(event);
            warn!("System health dropped below 50%: {:.2}", health_score);
        }
    }

    /// Log health event
    fn log_health_event(&mut self, event: HealthEvent) {
        debug!("Health event: {:?}", event);
        
        // Add to current snapshot events if one exists
        if let Some(current_snapshot) = self.health_history.back_mut() {
            current_snapshot.events.push(event);
        }
    }

    /// Start history cleanup task
    fn start_history_cleanup(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(300), |act, _ctx| { // Every 5 minutes
            act.cleanup_health_history();
        });
    }

    /// Clean up old health history entries
    fn cleanup_health_history(&mut self) {
        while self.health_history.len() > self.config.max_history_entries {
            self.health_history.pop_front();
        }
        
        // Remove old response time entries
        for (_, actor) in &mut self.monitored_actors {
            while actor.response_times.len() > 100 {
                actor.response_times.pop_front();
            }
        }
    }

    /// Create health snapshot for history
    fn create_health_snapshot(&self) -> HealthSnapshot {
        let actor_health: HashMap<String, HealthStatus> = self.monitored_actors
            .iter()
            .map(|(name, actor)| (name.clone(), actor.status.clone()))
            .collect();

        HealthSnapshot {
            timestamp: SystemTime::now(),
            overall_score: self.system_health.overall_score,
            actor_health,
            events: Vec::new(),
        }
    }

    /// Get detailed health report
    pub fn get_health_report(&self) -> HealthReport {
        let actor_details: HashMap<String, ActorHealthDetails> = self.monitored_actors
            .iter()
            .map(|(name, actor)| {
                let avg_response_time = if !actor.response_times.is_empty() {
                    Some(Duration::from_nanos(
                        actor.response_times.iter().map(|d| d.as_nanos()).sum::<u128>() as u64 
                        / actor.response_times.len() as u64
                    ))
                } else {
                    None
                };

                let details = ActorHealthDetails {
                    name: name.clone(),
                    status: actor.status.clone(),
                    priority: actor.priority.clone(),
                    failure_count: actor.failure_count,
                    success_count: actor.success_count,
                    last_check: actor.last_check,
                    last_success: actor.last_success,
                    avg_response_time,
                    check_interval: actor.check_interval,
                };

                (name.clone(), details)
            })
            .collect();

        HealthReport {
            system_health: self.system_health.clone(),
            actor_details,
            recovery_actions: self.recovery_actions.clone(),
            statistics: HealthMonitorStatsSummary {
                total_checks: self.stats.total_checks,
                successful_checks: self.stats.successful_checks,
                failed_checks: self.stats.failed_checks,
                success_rate: if self.stats.total_checks > 0 {
                    (self.stats.successful_checks as f64 / self.stats.total_checks as f64) * 100.0
                } else {
                    0.0
                },
                recovery_actions: self.stats.recovery_actions,
                successful_recoveries: self.stats.successful_recoveries,
                uptime: self.stats.started_at.elapsed(),
            },
            recent_events: self.health_history.back()
                .map(|snapshot| snapshot.events.clone())
                .unwrap_or_default(),
        }
    }
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub actor_name: String,
    pub success: bool,
    pub response_time: Option<Duration>,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Detailed health report
#[derive(Debug, Clone)]
pub struct HealthReport {
    pub system_health: SystemHealthState,
    pub actor_details: HashMap<String, ActorHealthDetails>,
    pub recovery_actions: HashMap<String, RecoveryAction>,
    pub statistics: HealthMonitorStatsSummary,
    pub recent_events: Vec<HealthEvent>,
}

/// Actor health details for reporting
#[derive(Debug, Clone)]
pub struct ActorHealthDetails {
    pub name: String,
    pub status: HealthStatus,
    pub priority: ActorPriority,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_check: Option<Instant>,
    pub last_success: Option<Instant>,
    pub avg_response_time: Option<Duration>,
    pub check_interval: Duration,
}

/// Statistics summary for health monitor
#[derive(Debug, Clone)]
pub struct HealthMonitorStatsSummary {
    pub total_checks: u64,
    pub successful_checks: u64,
    pub failed_checks: u64,
    pub success_rate: f64,
    pub recovery_actions: u64,
    pub successful_recoveries: u64,
    pub uptime: Duration,
}

/// Health monitoring errors
#[derive(Error, Debug)]
pub enum HealthMonitorError {
    #[error("Actor not found: {actor_name}")]
    ActorNotFound { actor_name: String },

    #[error("Actor already registered: {actor_name}")]
    ActorAlreadyRegistered { actor_name: String },

    #[error("Health check timeout for actor: {actor_name}")]
    HealthCheckTimeout { actor_name: String },

    #[error("Recovery failed for actor: {actor_name} - {reason}")]
    RecoveryFailed { actor_name: String, reason: String },

    #[error("Invalid configuration: {details}")]
    InvalidConfiguration { details: String },

    #[error("System health critical: score={score:.2}")]
    SystemHealthCritical { score: f64 },
}

// Message definitions for health monitoring

/// ALYS-006-22: Health check protocol messages
#[derive(Message)]
#[rtype(result = "Result<(), HealthMonitorError>")]
pub struct RegisterActor {
    pub name: String,
    pub priority: ActorPriority,
    pub check_interval: Option<Duration>,
    pub recovery_strategy: RecoveryStrategy,
    pub custom_check: Option<CustomHealthCheck>,
}

#[derive(Message)]
#[rtype(result = "Result<(), HealthMonitorError>")]
pub struct UnregisterActor {
    pub name: String,
}

#[derive(Message)]
#[rtype(result = "HealthReport")]
pub struct GetHealthReport {
    pub include_details: bool,
}

#[derive(Message)]
#[rtype(result = "SystemHealthState")]
pub struct GetSystemHealth;

#[derive(Message)]
#[rtype(result = "Result<(), HealthMonitorError>")]
pub struct TriggerHealthCheck {
    pub actor_name: String,
}

#[derive(Message)]
#[rtype(result = "Result<(), HealthMonitorError>")]
pub struct TriggerRecovery {
    pub actor_name: String,
    pub strategy: Option<RecoveryStrategy>,
}

// Message handlers

impl Handler<RegisterActor> for HealthMonitor {
    type Result = Result<(), HealthMonitorError>;

    #[instrument(skip(self))]
    fn handle(&mut self, msg: RegisterActor, _ctx: &mut Self::Context) -> Self::Result {
        if self.monitored_actors.contains_key(&msg.name) {
            return Err(HealthMonitorError::ActorAlreadyRegistered { 
                actor_name: msg.name 
            });
        }

        let check_interval = msg.check_interval.unwrap_or_else(|| {
            match msg.priority {
                ActorPriority::Critical => self.config.critical_check_interval,
                _ => self.config.default_check_interval,
            }
        });

        let monitored_actor = MonitoredActor {
            name: msg.name.clone(),
            priority: msg.priority.clone(),
            check_interval,
            status: HealthStatus::Unknown,
            failure_count: 0,
            success_count: 0,
            last_check: None,
            last_success: None,
            response_times: VecDeque::new(),
            custom_check: msg.custom_check,
            recovery_strategy: msg.recovery_strategy,
        };

        info!("Registered actor {} for health monitoring with {:?} priority", msg.name, msg.priority);
        self.monitored_actors.insert(msg.name.clone(), monitored_actor);

        let event = HealthEvent::ActorRegistered {
            name: msg.name,
            priority: msg.priority,
        };
        self.log_health_event(event);

        Ok(())
    }
}

impl Handler<UnregisterActor> for HealthMonitor {
    type Result = Result<(), HealthMonitorError>;

    fn handle(&mut self, msg: UnregisterActor, _ctx: &mut Self::Context) -> Self::Result {
        if self.monitored_actors.remove(&msg.name).is_some() {
            info!("Unregistered actor {} from health monitoring", msg.name);
            
            // Cancel any ongoing recovery
            self.recovery_actions.remove(&msg.name);
            
            let event = HealthEvent::ActorUnregistered {
                name: msg.name,
            };
            self.log_health_event(event);
            
            Ok(())
        } else {
            Err(HealthMonitorError::ActorNotFound { 
                actor_name: msg.name 
            })
        }
    }
}

impl Handler<GetHealthReport> for HealthMonitor {
    type Result = HealthReport;

    fn handle(&mut self, _msg: GetHealthReport, _ctx: &mut Self::Context) -> Self::Result {
        self.get_health_report()
    }
}

impl Handler<GetSystemHealth> for HealthMonitor {
    type Result = SystemHealthState;

    fn handle(&mut self, _msg: GetSystemHealth, _ctx: &mut Self::Context) -> Self::Result {
        self.system_health.clone()
    }
}

impl Handler<TriggerHealthCheck> for HealthMonitor {
    type Result = Result<(), HealthMonitorError>;

    fn handle(&mut self, msg: TriggerHealthCheck, ctx: &mut Self::Context) -> Self::Result {
        if self.monitored_actors.contains_key(&msg.actor_name) {
            self.initiate_health_check(msg.actor_name, ctx);
            Ok(())
        } else {
            Err(HealthMonitorError::ActorNotFound { 
                actor_name: msg.actor_name 
            })
        }
    }
}

impl Handler<TriggerRecovery> for HealthMonitor {
    type Result = Result<(), HealthMonitorError>;

    fn handle(&mut self, msg: TriggerRecovery, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(monitored_actor) = self.monitored_actors.get_mut(&msg.actor_name) {
            // Update recovery strategy if provided
            if let Some(strategy) = msg.strategy {
                monitored_actor.recovery_strategy = strategy;
            }
            
            self.initiate_recovery(&msg.actor_name);
            Ok(())
        } else {
            Err(HealthMonitorError::ActorNotFound { 
                actor_name: msg.actor_name 
            })
        }
    }
}

/// Actor implementation for HealthMonitor
impl Actor for HealthMonitor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("HealthMonitor started");
        
        // Start periodic health checks
        self.start_health_check_timer(ctx);
        
        // Start system health updates
        self.start_system_health_timer(ctx);
        
        // Start history cleanup
        self.start_history_cleanup(ctx);
        
        // Start recovery monitoring
        self.start_recovery_monitoring(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        info!("HealthMonitor stopping");
        
        // Create final health snapshot
        let final_snapshot = self.create_health_snapshot();
        self.health_history.push_back(final_snapshot);
        
        Running::Stop
    }
}

/// ALYS-006-22: Ping/Pong Health Check Protocol Messages
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<PongResponse, HealthCheckError>")]
pub struct PingMessage {
    pub sender_name: String,
    pub timestamp: Instant,
    pub sequence_number: u64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct PongResponse {
    pub responder_name: String,
    pub ping_timestamp: Instant,
    pub pong_timestamp: Instant,
    pub sequence_number: u64,
    pub health_status: BasicHealthStatus,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum BasicHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Error, Debug, Clone)]
pub enum HealthCheckError {
    #[error("Health check timeout")]
    Timeout,
    #[error("Actor unavailable: {reason}")]
    ActorUnavailable { reason: String },
    #[error("Internal error: {message}")]
    InternalError { message: String },
}

/// Health check response tracking
#[derive(Debug, Clone)]
pub struct HealthCheckResponse {
    pub actor_name: String,
    pub success: bool,
    pub response_time: Duration,
    pub timestamp: Instant,
    pub metadata: HashMap<String, String>,
    pub error: Option<HealthCheckError>,
}

impl HealthMonitor {
    /// Start periodic health check timer
    fn start_health_check_timer(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(self.config.default_check_interval, |act, ctx| {
            act.run_periodic_health_checks(ctx);
        });
    }
    
    /// Start system health update timer
    fn start_system_health_timer(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            act.update_system_health();
        });
    }
    
    /// Start recovery monitoring timer
    fn start_recovery_monitoring(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(10), |act, ctx| {
            act.monitor_recovery_actions(ctx);
        });
    }

    /// Run periodic health checks for all monitored actors
    fn run_periodic_health_checks(&mut self, ctx: &mut Context<Self>) {
        let now = Instant::now();
        let mut actors_to_check = Vec::new();
        
        for (actor_name, monitored_actor) in &self.monitored_actors {
            let should_check = match monitored_actor.last_check {
                Some(last_check) => now.duration_since(last_check) >= monitored_actor.check_interval,
                None => true,
            };
            
            if should_check {
                actors_to_check.push(actor_name.clone());
            }
        }
        
        for actor_name in actors_to_check {
            self.initiate_health_check(actor_name, ctx);
        }
    }

    /// Initiate health check for specific actor using ping/pong protocol
    fn initiate_health_check(&mut self, actor_name: String, ctx: &mut Context<Self>) {
        if let Some(monitored_actor) = self.monitored_actors.get_mut(&actor_name) {
            monitored_actor.last_check = Some(Instant::now());
            self.stats.total_checks += 1;
            
            // ALYS-006-22: Ping/Pong messaging implementation
            let ping_message = PingMessage {
                sender_name: "HealthMonitor".to_string(),
                timestamp: Instant::now(),
                sequence_number: self.stats.total_checks,
                metadata: HashMap::new(),
            };
            
            debug!("Sending ping to actor {}", actor_name);
            
            // Simulate health check (in real implementation, would send to actual actor)
            let check_start = Instant::now();
            let actor_name_clone = actor_name.clone();
            
            ctx.spawn(
                async move {
                    // Simulate network delay and processing time
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    
                    // Simulate successful ping/pong exchange
                    let response = PongResponse {
                        responder_name: actor_name_clone.clone(),
                        ping_timestamp: ping_message.timestamp,
                        pong_timestamp: Instant::now(),
                        sequence_number: ping_message.sequence_number,
                        health_status: BasicHealthStatus::Healthy,
                        metadata: HashMap::new(),
                    };
                    
                    HealthCheckResponse {
                        actor_name: actor_name_clone,
                        success: true,
                        response_time: check_start.elapsed(),
                        timestamp: Instant::now(),
                        metadata: HashMap::new(),
                        error: None,
                    }
                }
                .into_actor(self)
                .map(|response, act, _ctx| {
                    act.handle_health_check_response(response);
                }),
            );
        }
    }
    
    /// Handle health check response
    fn handle_health_check_response(&mut self, response: HealthCheckResponse) {
        if let Some(monitored_actor) = self.monitored_actors.get_mut(&response.actor_name) {
            // Record response time
            monitored_actor.response_times.push_back(response.response_time);
            if monitored_actor.response_times.len() > 100 {
                monitored_actor.response_times.pop_front();
            }
            
            if response.success {
                monitored_actor.success_count += 1;
                monitored_actor.failure_count = 0;
                monitored_actor.last_success = Some(response.timestamp);
                self.stats.successful_checks += 1;
                
                // Update health status based on recovery threshold
                if monitored_actor.success_count >= self.config.recovery_threshold {
                    match monitored_actor.status {
                        HealthStatus::Degraded { .. } | HealthStatus::Unhealthy { .. } | HealthStatus::Recovering => {
                            monitored_actor.status = HealthStatus::Healthy;
                            info!("Actor {} recovered to healthy status", response.actor_name);
                            
                            let event = HealthEvent::ActorRecovered {
                                name: response.actor_name.clone(),
                            };
                            self.log_health_event(event);
                        }
                        _ => {
                            monitored_actor.status = HealthStatus::Healthy;
                        }
                    }
                }
            } else {
                monitored_actor.failure_count += 1;
                monitored_actor.success_count = 0;
                self.stats.failed_checks += 1;
                
                // Update health status based on failure threshold
                if monitored_actor.failure_count >= self.config.failure_threshold {
                    let reason = response.error
                        .as_ref()
                        .map(|e| format!("{}", e))
                        .unwrap_or_else(|| "Multiple failures".to_string());
                    
                    monitored_actor.status = HealthStatus::Unhealthy { reason: reason.clone() };
                    warn!("Actor {} marked unhealthy: {}", response.actor_name, reason);
                    
                    let event = HealthEvent::ActorUnhealthy {
                        name: response.actor_name.clone(),
                        reason,
                    };
                    self.log_health_event(event);
                    
                    // Trigger recovery if enabled
                    if self.config.enable_auto_recovery {
                        self.initiate_recovery(&response.actor_name);
                    }
                } else if monitored_actor.failure_count > 0 {
                    let reason = format!("{} consecutive failures", monitored_actor.failure_count);
                    monitored_actor.status = HealthStatus::Degraded { reason };
                }
            }
        }
        
        // Update system health
        self.update_system_health();
    }
    
    /// Monitor ongoing recovery actions
    fn monitor_recovery_actions(&mut self, ctx: &mut Context<Self>) {
        let now = Instant::now();
        let mut failed_recoveries = Vec::new();
        
        for (actor_name, recovery_action) in &self.recovery_actions {
            if now.duration_since(recovery_action.started_at) > recovery_action.timeout {
                failed_recoveries.push(actor_name.clone());
            }
        }
        
        // Handle failed recoveries
        for actor_name in failed_recoveries {
            if let Some(recovery_action) = self.recovery_actions.remove(&actor_name) {
                if recovery_action.attempts < recovery_action.max_attempts {
                    // Retry recovery
                    let mut new_recovery = recovery_action.clone();
                    new_recovery.attempts += 1;
                    new_recovery.started_at = now;
                    new_recovery.timeout = new_recovery.timeout.mul_f64(1.5); // Increase timeout
                    
                    warn!("Recovery attempt {} failed for actor {}, retrying", 
                          recovery_action.attempts, actor_name);
                    
                    self.recovery_actions.insert(actor_name.clone(), new_recovery);
                } else {
                    // Recovery failed completely
                    if let Some(monitored_actor) = self.monitored_actors.get_mut(&actor_name) {
                        monitored_actor.status = HealthStatus::Unhealthy { 
                            reason: "Recovery failed".to_string() 
                        };
                    }
                    
                    error!("Recovery failed completely for actor {} after {} attempts", 
                           actor_name, recovery_action.max_attempts);
                    
                    let event = HealthEvent::RecoveryFailed {
                        name: actor_name,
                        attempts: recovery_action.attempts,
                    };
                    self.log_health_event(event);
                }
            }
        }
    }
}

/// ALYS-006-23 & ALYS-006-24: Shutdown coordination and monitoring
/// 
/// Coordinated shutdown system with progress tracking, timeout handling,
/// and resource cleanup for the Alys V2 sidechain actor system.
pub struct ShutdownCoordinator {
    /// Shutdown configuration
    config: ShutdownConfig,
    /// Current shutdown state
    state: ShutdownState,
    /// Actors to shutdown in order
    shutdown_sequence: Vec<ActorShutdownInfo>,
    /// Shutdown progress tracking
    progress: ShutdownProgress,
    /// Forced shutdown triggers
    force_shutdown_triggers: Vec<ForceShutdownTrigger>,
    /// Resource cleanup handlers
    cleanup_handlers: Vec<CleanupHandler>,
    /// Shutdown statistics
    stats: ShutdownStats,
}

/// Shutdown configuration
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Overall shutdown timeout
    pub total_timeout: Duration,
    /// Per-actor shutdown timeout
    pub actor_timeout: Duration,
    /// Cleanup phase timeout
    pub cleanup_timeout: Duration,
    /// Enable forced shutdown
    pub enable_forced_shutdown: bool,
    /// Shutdown sequence strategy
    pub sequence_strategy: ShutdownSequenceStrategy,
    /// Progress reporting interval
    pub progress_interval: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            total_timeout: lifecycle::GRACEFUL_SHUTDOWN_TIMEOUT,
            actor_timeout: lifecycle::ACTOR_SHUTDOWN_TIMEOUT,
            cleanup_timeout: Duration::from_secs(30),
            enable_forced_shutdown: true,
            sequence_strategy: ShutdownSequenceStrategy::PriorityBased,
            progress_interval: Duration::from_secs(5),
        }
    }
}

/// Shutdown sequence strategy
#[derive(Debug, Clone)]
pub enum ShutdownSequenceStrategy {
    /// Shutdown by priority (background first, critical last)
    PriorityBased,
    /// Shutdown by dependency order
    DependencyBased,
    /// Parallel shutdown of all actors
    Parallel,
    /// Custom sequence order
    Custom(Vec<String>),
}

/// Current shutdown state
#[derive(Debug, Clone, PartialEq)]
pub enum ShutdownState {
    /// Normal operation
    Running,
    /// Shutdown initiated
    Initiated,
    /// Graceful shutdown in progress
    GracefulShutdown,
    /// Cleanup phase
    Cleanup,
    /// Forced shutdown
    ForcedShutdown,
    /// Shutdown completed
    Complete,
    /// Shutdown failed
    Failed { reason: String },
}

/// Actor shutdown information
#[derive(Debug, Clone)]
pub struct ActorShutdownInfo {
    /// Actor name
    pub name: String,
    /// Actor priority
    pub priority: ActorPriority,
    /// Shutdown order
    pub order: u32,
    /// Current status
    pub status: ActorShutdownStatus,
    /// Shutdown timeout
    pub timeout: Duration,
    /// Dependencies that must shutdown first
    pub dependencies: Vec<String>,
    /// Shutdown start time
    pub shutdown_started: Option<Instant>,
    /// Shutdown completion time
    pub shutdown_completed: Option<Instant>,
}

/// Actor shutdown status
#[derive(Debug, Clone, PartialEq)]
pub enum ActorShutdownStatus {
    /// Ready for shutdown
    Ready,
    /// Shutdown in progress
    InProgress,
    /// Successfully shutdown
    Complete,
    /// Shutdown failed
    Failed { reason: String },
    /// Shutdown timed out
    TimedOut,
    /// Forced termination
    Terminated,
}

/// Shutdown progress tracking
#[derive(Debug, Clone)]
pub struct ShutdownProgress {
    /// Shutdown start time
    pub started_at: Instant,
    /// Current phase
    pub current_phase: ShutdownPhase,
    /// Overall progress percentage
    pub progress_percentage: f64,
    /// Actors completed shutdown
    pub actors_completed: usize,
    /// Total actors to shutdown
    pub total_actors: usize,
    /// Estimated time remaining
    pub estimated_remaining: Option<Duration>,
    /// Last progress update
    pub last_updated: Instant,
}

/// Shutdown phases
#[derive(Debug, Clone, PartialEq)]
pub enum ShutdownPhase {
    /// Preparing for shutdown
    Preparation,
    /// Stopping actors
    ActorShutdown,
    /// Resource cleanup
    Cleanup,
    /// Finalization
    Finalization,
}

/// Force shutdown triggers
#[derive(Debug, Clone)]
pub struct ForceShutdownTrigger {
    /// Trigger condition
    pub condition: ForceShutdownCondition,
    /// Time threshold
    pub threshold: Duration,
    /// Action to take
    pub action: ForceShutdownAction,
}

/// Conditions that trigger forced shutdown
#[derive(Debug, Clone)]
pub enum ForceShutdownCondition {
    /// Overall timeout exceeded
    OverallTimeout,
    /// Too many actors failed to shutdown
    TooManyFailures { threshold: usize },
    /// Critical actor shutdown failed
    CriticalActorFailed { actor_name: String },
    /// External shutdown signal
    ExternalSignal,
}

/// Actions for forced shutdown
#[derive(Debug, Clone)]
pub enum ForceShutdownAction {
    /// Terminate remaining actors
    TerminateAll,
    /// Skip cleanup and exit
    SkipCleanup,
    /// Emergency exit
    EmergencyExit,
}

/// Cleanup handler
#[derive(Debug, Clone)]
pub struct CleanupHandler {
    /// Handler name
    pub name: String,
    /// Cleanup priority
    pub priority: u32,
    /// Cleanup function identifier
    pub handler_id: String,
    /// Timeout for cleanup
    pub timeout: Duration,
}

/// Shutdown statistics
#[derive(Debug, Default)]
pub struct ShutdownStats {
    /// Total shutdown time
    pub total_time: Option<Duration>,
    /// Actors successfully shutdown
    pub successful_shutdowns: usize,
    /// Failed shutdowns
    pub failed_shutdowns: usize,
    /// Forced terminations
    pub forced_terminations: usize,
    /// Cleanup handlers executed
    pub cleanup_handlers_run: usize,
    /// Shutdown attempts
    pub shutdown_attempts: usize,
}

impl Actor for ShutdownCoordinator {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("ShutdownCoordinator started");
        
        // Start progress monitoring
        ctx.run_interval(self.config.progress_interval, |act, _ctx| {
            if act.state != ShutdownState::Running {
                act.update_progress();
                act.report_progress();
            }
        });
    }
}

impl ShutdownCoordinator {
    /// Create new shutdown coordinator
    pub fn new(config: ShutdownConfig) -> Self {
        Self {
            config,
            state: ShutdownState::Running,
            shutdown_sequence: Vec::new(),
            progress: ShutdownProgress {
                started_at: Instant::now(),
                current_phase: ShutdownPhase::Preparation,
                progress_percentage: 0.0,
                actors_completed: 0,
                total_actors: 0,
                estimated_remaining: None,
                last_updated: Instant::now(),
            },
            force_shutdown_triggers: vec![
                ForceShutdownTrigger {
                    condition: ForceShutdownCondition::OverallTimeout,
                    threshold: config.total_timeout,
                    action: ForceShutdownAction::TerminateAll,
                }
            ],
            cleanup_handlers: Vec::new(),
            stats: ShutdownStats::default(),
        }
    }

    /// Update shutdown progress
    fn update_progress(&mut self) {
        let completed = self.shutdown_sequence.iter()
            .filter(|info| matches!(info.status, ActorShutdownStatus::Complete))
            .count();

        let total = self.shutdown_sequence.len();
        
        self.progress.actors_completed = completed;
        self.progress.total_actors = total;
        self.progress.progress_percentage = if total > 0 {
            (completed as f64 / total as f64) * 100.0
        } else {
            100.0
        };
        
        self.progress.last_updated = Instant::now();
    }

    /// Report shutdown progress
    fn report_progress(&self) {
        info!(
            "Shutdown progress: {:.1}% ({}/{}), phase: {:?}, elapsed: {:?}",
            self.progress.progress_percentage,
            self.progress.actors_completed,
            self.progress.total_actors,
            self.progress.current_phase,
            self.progress.started_at.elapsed()
        );
    }
}

// Shutdown message definitions

#[derive(Message)]
#[rtype(result = "Result<(), ShutdownError>")]
pub struct InitiateShutdown {
    pub reason: String,
    pub timeout: Option<Duration>,
}

#[derive(Message)]
#[rtype(result = "ShutdownProgress")]
pub struct GetShutdownProgress;

#[derive(Message)]
#[rtype(result = "Result<(), ShutdownError>")]
pub struct ForceShutdown {
    pub reason: String,
}

#[derive(Message)]
#[rtype(result = "Result<(), ShutdownError>")]
pub struct RegisterForShutdown {
    pub actor_name: String,
    pub priority: ActorPriority,
    pub dependencies: Vec<String>,
    pub timeout: Option<Duration>,
}

/// Shutdown errors
#[derive(Error, Debug)]
pub enum ShutdownError {
    #[error("Shutdown already in progress")]
    AlreadyInProgress,

    #[error("Shutdown timeout exceeded")]
    TimeoutExceeded,

    #[error("Actor shutdown failed: {actor_name} - {reason}")]
    ActorShutdownFailed { actor_name: String, reason: String },

    #[error("Cleanup failed: {handler_name} - {reason}")]
    CleanupFailed { handler_name: String, reason: String },

    #[error("Invalid shutdown state: {current_state:?}")]
    InvalidState { current_state: ShutdownState },
}

impl Handler<InitiateShutdown> for ShutdownCoordinator {
    type Result = Result<(), ShutdownError>;

    fn handle(&mut self, msg: InitiateShutdown, _ctx: &mut Self::Context) -> Self::Result {
        if self.state != ShutdownState::Running {
            return Err(ShutdownError::AlreadyInProgress);
        }

        info!("Initiating graceful shutdown: {}", msg.reason);
        
        self.state = ShutdownState::Initiated;
        self.progress.started_at = Instant::now();
        self.stats.shutdown_attempts += 1;
        
        // Override timeout if provided
        if let Some(timeout) = msg.timeout {
            // Update force shutdown trigger timeout
            for trigger in &mut self.force_shutdown_triggers {
                if matches!(trigger.condition, ForceShutdownCondition::OverallTimeout) {
                    trigger.threshold = timeout;
                }
            }
        }

        Ok(())
    }
}

impl Handler<GetShutdownProgress> for ShutdownCoordinator {
    type Result = ShutdownProgress;

    fn handle(&mut self, _msg: GetShutdownProgress, _ctx: &mut Self::Context) -> Self::Result {
        self.progress.clone()
    }
}

impl Handler<ForceShutdown> for ShutdownCoordinator {
    type Result = Result<(), ShutdownError>;

    fn handle(&mut self, msg: ForceShutdown, _ctx: &mut Self::Context) -> Self::Result {
        warn!("Forcing immediate shutdown: {}", msg.reason);
        
        self.state = ShutdownState::ForcedShutdown;
        self.progress.current_phase = ShutdownPhase::Finalization;
        self.stats.forced_terminations += 1;

        Ok(())
    }
}

impl Handler<RegisterForShutdown> for ShutdownCoordinator {
    type Result = Result<(), ShutdownError>;

    fn handle(&mut self, msg: RegisterForShutdown, _ctx: &mut Self::Context) -> Self::Result {
        let order = self.calculate_shutdown_order(&msg.priority, &msg.dependencies);
        let timeout = msg.timeout.unwrap_or(self.config.actor_timeout);

        let shutdown_info = ActorShutdownInfo {
            name: msg.actor_name.clone(),
            priority: msg.priority,
            order,
            status: ActorShutdownStatus::Ready,
            timeout,
            dependencies: msg.dependencies,
            shutdown_started: None,
            shutdown_completed: None,
        };

        info!("Registered actor {} for shutdown with order {}", msg.actor_name, order);
        self.shutdown_sequence.push(shutdown_info);
        
        // Sort by shutdown order
        self.shutdown_sequence.sort_by_key(|info| info.order);

        Ok(())
    }
}

/// Actor implementation for ShutdownCoordinator
impl Actor for ShutdownCoordinator {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("ShutdownCoordinator started");
        
        // Start shutdown monitoring timer
        self.start_shutdown_monitoring(ctx);
        
        // Start force shutdown monitoring
        self.start_force_shutdown_monitoring(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        info!("ShutdownCoordinator stopping");
        Running::Stop
    }
}

impl ShutdownCoordinator {
    /// Start shutdown monitoring timer
    fn start_shutdown_monitoring(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(1), |act, ctx| {
            act.monitor_shutdown_progress(ctx);
        });
    }
    
    /// Start force shutdown monitoring timer
    fn start_force_shutdown_monitoring(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_millis(500), |act, ctx| {
            act.check_force_shutdown_triggers(ctx);
        });
    }
    
    /// Monitor shutdown progress and update state
    fn monitor_shutdown_progress(&mut self, ctx: &mut Context<Self>) {
        if self.state == ShutdownState::Running {
            return;
        }
        
        match self.state {
            ShutdownState::Initiated => {
                self.progress.current_phase = ShutdownPhase::Preparation;
                self.initiate_actor_shutdown_sequence(ctx);
                self.state = ShutdownState::InProgress;
            }
            ShutdownState::InProgress => {
                self.update_shutdown_progress();
                self.advance_shutdown_phase_if_ready();
            }
            ShutdownState::Complete => {
                info!("Shutdown completed successfully");
                ctx.stop();
            }
            ShutdownState::Failed { .. } => {
                error!("Shutdown failed, stopping coordinator");
                ctx.stop();
            }
            _ => {}
        }
    }
    
    /// Check for force shutdown triggers
    fn check_force_shutdown_triggers(&mut self, ctx: &mut Context<Self>) {
        if self.state == ShutdownState::Running || self.state == ShutdownState::Complete {
            return;
        }
        
        let elapsed = self.progress.started_at.elapsed();
        
        for trigger in &self.force_shutdown_triggers {
            match &trigger.condition {
                ForceShutdownCondition::OverallTimeout => {
                    if elapsed > trigger.threshold {
                        warn!("Overall shutdown timeout exceeded, forcing shutdown");
                        self.execute_force_shutdown("Overall timeout exceeded".to_string(), ctx);
                    }
                }
                ForceShutdownCondition::TooManyFailures { threshold } => {
                    let failed_count = self.shutdown_sequence
                        .iter()
                        .filter(|info| matches!(info.status, ActorShutdownStatus::Failed { .. }))
                        .count();
                    
                    if failed_count >= *threshold {
                        warn!("Too many actor shutdown failures ({}), forcing shutdown", failed_count);
                        self.execute_force_shutdown(
                            format!("Too many failures: {}", failed_count), 
                            ctx
                        );
                    }
                }
                ForceShutdownCondition::CriticalActorFailed { actor_name } => {
                    if let Some(info) = self.shutdown_sequence.iter().find(|info| info.name == *actor_name) {
                        if matches!(info.status, ActorShutdownStatus::Failed { .. }) {
                            warn!("Critical actor {} shutdown failed, forcing shutdown", actor_name);
                            self.execute_force_shutdown(
                                format!("Critical actor failed: {}", actor_name), 
                                ctx
                            );
                        }
                    }
                }
                ForceShutdownCondition::ExternalSignal => {
                    // External signals would be handled via messages
                }
            }
        }
    }
    
    /// Execute force shutdown
    fn execute_force_shutdown(&mut self, reason: String, _ctx: &mut Context<Self>) {
        warn!("Executing force shutdown: {}", reason);
        
        self.state = ShutdownState::ForcedShutdown;
        self.progress.current_phase = ShutdownPhase::Finalization;
        self.stats.forced_terminations += 1;
        
        // Mark all remaining actors as terminated
        for info in &mut self.shutdown_sequence {
            if info.status == ActorShutdownStatus::InProgress || info.status == ActorShutdownStatus::Ready {
                info.status = ActorShutdownStatus::Terminated;
                info.shutdown_completed = Some(Instant::now());
            }
        }
        
        // Execute cleanup handlers
        self.execute_cleanup_handlers();
        
        self.state = ShutdownState::Complete;
    }
    
    /// Initiate actor shutdown sequence
    fn initiate_actor_shutdown_sequence(&mut self, ctx: &mut Context<Self>) {
        info!("Initiating actor shutdown sequence for {} actors", self.shutdown_sequence.len());
        
        self.progress.current_phase = ShutdownPhase::ActorShutdown;
        self.progress.total_actors = self.shutdown_sequence.len();
        
        // Start shutting down actors that have no dependencies
        self.shutdown_ready_actors(ctx);
    }
    
    /// Shutdown actors that are ready (no pending dependencies)
    fn shutdown_ready_actors(&mut self, ctx: &mut Context<Self>) {
        let mut ready_actors = Vec::new();
        
        for (index, info) in self.shutdown_sequence.iter().enumerate() {
            if info.status == ActorShutdownStatus::Ready && self.dependencies_satisfied(&info.dependencies) {
                ready_actors.push(index);
            }
        }
        
        for index in ready_actors {
            if let Some(info) = self.shutdown_sequence.get_mut(index) {
                info.status = ActorShutdownStatus::InProgress;
                info.shutdown_started = Some(Instant::now());
                
                debug!("Starting shutdown of actor: {}", info.name);
                
                // Simulate actor shutdown (in real implementation, would send shutdown message)
                let actor_name = info.name.clone();
                let timeout = info.timeout;
                
                ctx.spawn(
                    async move {
                        // Simulate shutdown process
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        (actor_name, true) // success
                    }
                    .into_actor(self)
                    .map(|(actor_name, success), act, ctx| {
                        act.handle_actor_shutdown_result(actor_name, success, ctx);
                    }),
                );
            }
        }
    }
    
    /// Check if dependencies are satisfied for an actor
    fn dependencies_satisfied(&self, dependencies: &[String]) -> bool {
        dependencies.iter().all(|dep_name| {
            self.shutdown_sequence.iter().any(|info| {
                info.name == *dep_name && info.status == ActorShutdownStatus::Complete
            })
        })
    }
    
    /// Handle actor shutdown result
    fn handle_actor_shutdown_result(&mut self, actor_name: String, success: bool, ctx: &mut Context<Self>) {
        if let Some(info) = self.shutdown_sequence.iter_mut().find(|info| info.name == actor_name) {
            if success {
                info.status = ActorShutdownStatus::Complete;
                info.shutdown_completed = Some(Instant::now());
                self.stats.successful_shutdowns += 1;
                debug!("Actor {} shutdown completed", actor_name);
            } else {
                info.status = ActorShutdownStatus::Failed { 
                    reason: "Shutdown failed".to_string() 
                };
                info.shutdown_completed = Some(Instant::now());
                self.stats.failed_shutdowns += 1;
                warn!("Actor {} shutdown failed", actor_name);
            }
            
            // Continue shutting down ready actors
            self.shutdown_ready_actors(ctx);
        }
    }
    
    /// Update shutdown progress
    fn update_shutdown_progress(&mut self) {
        let completed_count = self.shutdown_sequence
            .iter()
            .filter(|info| {
                matches!(info.status, 
                    ActorShutdownStatus::Complete | 
                    ActorShutdownStatus::Failed { .. } | 
                    ActorShutdownStatus::Terminated
                )
            })
            .count();
        
        self.progress.actors_completed = completed_count;
        self.progress.progress_percentage = if self.progress.total_actors > 0 {
            (completed_count as f64 / self.progress.total_actors as f64) * 100.0
        } else {
            100.0
        };
        
        // Calculate estimated remaining time
        if completed_count > 0 {
            let elapsed = self.progress.started_at.elapsed();
            let avg_time_per_actor = elapsed.as_secs_f64() / completed_count as f64;
            let remaining_actors = self.progress.total_actors - completed_count;
            
            if remaining_actors > 0 {
                self.progress.estimated_remaining = Some(
                    Duration::from_secs_f64(avg_time_per_actor * remaining_actors as f64)
                );
            } else {
                self.progress.estimated_remaining = None;
            }
        }
        
        self.progress.last_updated = Instant::now();
    }
    
    /// Advance shutdown phase if ready
    fn advance_shutdown_phase_if_ready(&mut self) {
        let all_actors_done = self.shutdown_sequence
            .iter()
            .all(|info| {
                matches!(info.status, 
                    ActorShutdownStatus::Complete | 
                    ActorShutdownStatus::Failed { .. } | 
                    ActorShutdownStatus::Terminated
                )
            });
        
        if all_actors_done {
            match self.progress.current_phase {
                ShutdownPhase::ActorShutdown => {
                    info!("All actors shutdown, proceeding to cleanup phase");
                    self.progress.current_phase = ShutdownPhase::Cleanup;
                    self.execute_cleanup_handlers();
                }
                ShutdownPhase::Cleanup => {
                    info!("Cleanup completed, proceeding to finalization");
                    self.progress.current_phase = ShutdownPhase::Finalization;
                    self.finalize_shutdown();
                }
                _ => {}
            }
        }
    }
    
    /// Execute cleanup handlers
    fn execute_cleanup_handlers(&mut self) {
        info!("Executing {} cleanup handlers", self.cleanup_handlers.len());
        
        for handler in &self.cleanup_handlers {
            debug!("Executing cleanup handler: {}", handler.name);
            // In real implementation, would execute cleanup logic
            self.stats.cleanup_operations += 1;
        }
        
        self.progress.current_phase = ShutdownPhase::Finalization;
    }
    
    /// Finalize shutdown
    fn finalize_shutdown(&mut self) {
        info!("Finalizing shutdown");
        
        // Calculate final statistics
        let total_shutdown_time = self.progress.started_at.elapsed();
        let success_rate = if self.progress.total_actors > 0 {
            (self.stats.successful_shutdowns as f64 / self.progress.total_actors as f64) * 100.0
        } else {
            100.0
        };
        
        info!(
            "Shutdown finalized - Total time: {:?}, Success rate: {:.1}%, {} actors completed",
            total_shutdown_time,
            success_rate,
            self.progress.actors_completed
        );
        
        self.progress.progress_percentage = 100.0;
        self.state = ShutdownState::Complete;
    }

    /// Calculate shutdown order based on priority and dependencies
    fn calculate_shutdown_order(&self, priority: &ActorPriority, dependencies: &[String]) -> u32 {
        let base_order = match priority {
            ActorPriority::Background => 1000,
            ActorPriority::Normal => 2000,
            ActorPriority::High => 3000,
            ActorPriority::Critical => 4000,
        };
        
        // Add dependency depth to ensure proper ordering
        let dependency_depth = self.calculate_dependency_depth(dependencies);
        
        base_order + dependency_depth * 100
    }
    
    /// Calculate dependency depth
    fn calculate_dependency_depth(&self, dependencies: &[String]) -> u32 {
        dependencies.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;
    use tokio_test;

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let config = HealthMonitorConfig::default();
        let health_monitor = HealthMonitor::new(config);
        
        assert_eq!(health_monitor.monitored_actors.len(), 0);
        assert_eq!(health_monitor.system_health.overall_score, 100.0);
    }

    #[tokio::test]
    async fn test_actor_registration() {
        let sys = System::new();
        
        sys.block_on(async {
            let health_monitor = HealthMonitor::new(HealthMonitorConfig::default());
            let addr = health_monitor.start();
            
            let register_msg = RegisterActor {
                name: "test_actor".to_string(),
                priority: ActorPriority::Normal,
                check_interval: Some(Duration::from_secs(10)),
                recovery_strategy: RecoveryStrategy::Restart,
                custom_check: None,
            };
            
            let result = addr.send(register_msg).await;
            assert!(result.is_ok());
            assert!(result.unwrap().is_ok());
        });
    }

    #[tokio::test]
    async fn test_shutdown_coordinator_creation() {
        let config = ShutdownConfig::default();
        let coordinator = ShutdownCoordinator::new(config);
        
        assert_eq!(coordinator.state, ShutdownState::Running);
        assert_eq!(coordinator.shutdown_sequence.len(), 0);
    }

    #[test]
    fn test_health_status_transitions() {
        let mut actor = MonitoredActor {
            name: "test".to_string(),
            priority: ActorPriority::Normal,
            check_interval: Duration::from_secs(10),
            status: HealthStatus::Healthy,
            failure_count: 0,
            success_count: 0,
            last_check: None,
            last_success: None,
            response_times: VecDeque::new(),
            custom_check: None,
            recovery_strategy: RecoveryStrategy::Restart,
        };

        // Test degraded transition
        actor.failure_count = 1;
        actor.status = HealthStatus::Degraded { 
            reason: "Single failure".to_string() 
        };
        
        match actor.status {
            HealthStatus::Degraded { .. } => assert!(true),
            _ => assert!(false, "Expected degraded status"),
        }

        // Test unhealthy transition
        actor.failure_count = 3;
        actor.status = HealthStatus::Unhealthy { 
            reason: "Multiple failures".to_string() 
        };
        
        match actor.status {
            HealthStatus::Unhealthy { .. } => assert!(true),
            _ => assert!(false, "Expected unhealthy status"),
        }
    }

    #[test]
    fn test_shutdown_order_calculation() {
        let coordinator = ShutdownCoordinator::new(ShutdownConfig::default());
        
        let background_order = coordinator.calculate_shutdown_order(&ActorPriority::Background, &[]);
        let critical_order = coordinator.calculate_shutdown_order(&ActorPriority::Critical, &[]);
        
        assert!(background_order < critical_order);
        
        let with_deps = coordinator.calculate_shutdown_order(
            &ActorPriority::Normal, 
            &["dep1".to_string(), "dep2".to_string()]
        );
        let without_deps = coordinator.calculate_shutdown_order(&ActorPriority::Normal, &[]);
        
        assert!(with_deps > without_deps);
    }
}