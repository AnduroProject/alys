//! Root Supervisor for V2 Actor System
//!
//! Provides supervision, lifecycle management, and fault tolerance for all actors
//! in the V2 architecture. Implements the supervisor pattern with restart policies.

use actix::prelude::*;
use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Root supervisor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    /// Restart policy for failed actors
    pub restart_policy: RestartPolicy,
    /// Maximum number of restarts within the time window
    pub max_restarts: u32,
    /// Backoff time between restart attempts
    pub backoff_seconds: u64,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Test mode (disables some checks)
    pub test_mode: bool,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            restart_policy: RestartPolicy::OneForOne,
            max_restarts: 5,
            backoff_seconds: 5,
            health_check_interval: Duration::from_secs(30),
            test_mode: false,
        }
    }
}

/// Actor restart policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RestartPolicy {
    /// Always restart failed actors
    Always,
    /// Restart only on temporary failures
    OnFailure,
    /// One-for-one restart (only failed actor)
    OneForOne,
    /// Never restart (manual intervention required)
    Never,
}

/// Root supervisor actor
pub struct RootSupervisor {
    /// Supervisor configuration
    config: SupervisorConfig,
    /// Supervised actors registry
    actors: HashMap<String, ActorInfo>,
    /// System startup time
    startup_time: SystemTime,
    /// Health check metrics
    health_metrics: HealthMetrics,
}

impl RootSupervisor {
    /// Create new root supervisor
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            actors: HashMap::new(),
            startup_time: SystemTime::now(),
            health_metrics: HealthMetrics::default(),
        }
    }

    /// Register an actor with the supervisor
    pub fn register_actor(&mut self, name: String, info: ActorInfo) {
        self.actors.insert(name, info);
    }

    /// Get supervisor status
    pub fn get_status(&self) -> SupervisorStatus {
        let total_actors = self.actors.len() as u32;
        let failed_actors = self.actors
            .values()
            .filter(|info| matches!(info.status, ActorStatus::Failed))
            .count() as u32;

        SupervisorStatus {
            total_actors,
            failed_actors,
            uptime: self.startup_time.elapsed().unwrap_or_default(),
            restart_count: self.health_metrics.total_restarts,
        }
    }
}

impl Actor for RootSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("🎯 RootSupervisor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("🛑 RootSupervisor stopped");
    }
}

/// Actor information tracked by supervisor
#[derive(Debug, Clone)]
pub struct ActorInfo {
    /// Actor type identifier
    pub actor_type: String,
    /// Current actor status
    pub status: ActorStatus,
    /// Last restart time
    pub last_restart: Option<SystemTime>,
    /// Restart count
    pub restart_count: u32,
}

/// Actor status enumeration
#[derive(Debug, Clone)]
pub enum ActorStatus {
    /// Actor is running normally
    Running,
    /// Actor is starting up
    Starting,
    /// Actor is shutting down
    Stopping,
    /// Actor has failed
    Failed,
    /// Actor is restarting
    Restarting,
}

/// Health metrics for the supervisor
#[derive(Debug, Default)]
pub struct HealthMetrics {
    /// Total number of restarts performed
    pub total_restarts: u32,
    /// Number of health checks performed
    pub health_checks: u64,
    /// Last health check time
    pub last_health_check: Option<SystemTime>,
}

/// Supervisor status response
#[derive(Debug, Clone)]
pub struct SupervisorStatus {
    /// Total number of supervised actors
    pub total_actors: u32,
    /// Number of failed actors
    pub failed_actors: u32,
    /// Supervisor uptime
    pub uptime: Duration,
    /// Total restart operations
    pub restart_count: u32,
}

/// Message to get supervisor status
#[derive(Message)]
#[rtype(result = "SupervisorStatus")]
pub struct GetSupervisorStatus;

impl Handler<GetSupervisorStatus> for RootSupervisor {
    type Result = SupervisorStatus;

    fn handle(&mut self, _msg: GetSupervisorStatus, _ctx: &mut Self::Context) -> Self::Result {
        self.get_status()
    }
}

/// Message to register an actor with supervisor
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterActor {
    pub name: String,
    pub actor_info: ActorInfo,
}

impl Handler<RegisterActor> for RootSupervisor {
    type Result = ();

    fn handle(&mut self, msg: RegisterActor, _ctx: &mut Self::Context) -> Self::Result {
        self.register_actor(msg.name, msg.actor_info);
    }
}

/// Message to request actor restart
#[derive(Message)]
#[rtype(result = "Result<(), SupervisorError>")]
pub struct RestartActor {
    pub actor_name: String,
    pub reason: String,
}

impl Handler<RestartActor> for RootSupervisor {
    type Result = Result<(), SupervisorError>;

    fn handle(&mut self, msg: RestartActor, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(actor_info) = self.actors.get_mut(&msg.actor_name) {
            match self.config.restart_policy {
                RestartPolicy::Always | RestartPolicy::OneForOne => {
                    actor_info.status = ActorStatus::Restarting;
                    actor_info.restart_count += 1;
                    actor_info.last_restart = Some(SystemTime::now());
                    self.health_metrics.total_restarts += 1;
                    
                    println!("🔄 Restarting actor '{}': {}", msg.actor_name, msg.reason);
                    Ok(())
                }
                RestartPolicy::Never => {
                    Err(SupervisorError::RestartDisabled)
                }
                RestartPolicy::OnFailure => {
                    // Would need more context about failure type
                    Ok(())
                }
            }
        } else {
            Err(SupervisorError::ActorNotFound)
        }
    }
}

/// Supervisor error types
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("Actor not found in supervisor registry")]
    ActorNotFound,
    #[error("Actor restart is disabled by policy")]
    RestartDisabled,
    #[error("Maximum restart limit exceeded")]
    RestartLimitExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supervisor_config_default() {
        let config = SupervisorConfig::default();
        assert_eq!(config.max_restarts, 5);
        assert_eq!(config.backoff_seconds, 5);
        assert!(!config.test_mode);
    }

    #[test]
    fn test_actor_registration() {
        let config = SupervisorConfig::default();
        let mut supervisor = RootSupervisor::new(config);
        
        let actor_info = ActorInfo {
            actor_type: "ChainActor".to_string(),
            status: ActorStatus::Running,
            last_restart: None,
            restart_count: 0,
        };
        
        supervisor.register_actor("chain".to_string(), actor_info);
        
        let status = supervisor.get_status();
        assert_eq!(status.total_actors, 1);
        assert_eq!(status.failed_actors, 0);
    }

    #[actix::test]
    async fn test_supervisor_messages() {
        let config = SupervisorConfig::default();
        let supervisor = RootSupervisor::new(config).start();
        
        let status = supervisor.send(GetSupervisorStatus).await.unwrap();
        assert_eq!(status.total_actors, 0);
    }
}