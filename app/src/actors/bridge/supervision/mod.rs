//! Bridge Supervision System
//! 
//! Supervisor for bridge actor ecosystem

pub mod strategies;
pub mod health;
pub mod recovery;

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error};
use uuid::Uuid;

use crate::actors::bridge::{
    config::{BridgeSystemConfig, SupervisionConfig},
    messages::*,
    actors::{bridge::BridgeActor, pegin::PegInActor, pegout::PegOutActor, stream::StreamActor},
    shared::*,
};
use crate::types::*;
use strategies::*;
use health::*;
use recovery::*;

/// Bridge supervisor actor
pub struct BridgeSupervisor {
    /// Configuration
    config: SupervisionConfig,
    
    /// Supervised actors
    bridge_actor: Option<Addr<BridgeActor>>,
    pegin_actor: Option<Addr<PegInActor>>,
    pegout_actor: Option<Addr<PegOutActor>>,
    stream_actor: Option<Addr<StreamActor>>,
    
    /// Supervision state
    actor_health: HashMap<ActorId, ActorHealth>,
    restart_strategies: HashMap<ActorId, RestartStrategy>,
    supervision_metrics: SupervisionMetrics,
    
    /// System integration
    system_registry: Option<Addr<actor_system::ActorRegistry>>,
    
    /// Health monitoring
    health_monitor: SupervisionHealthMonitor,
    
    /// Recovery coordinator
    recovery_coordinator: RecoveryCoordinator,
    
    /// Supervisor startup time
    started_at: SystemTime,
}

/// Actor health tracking
#[derive(Debug, Clone)]
pub struct ActorHealth {
    pub status: HealthStatus,
    pub last_heartbeat: SystemTime,
    pub failure_count: u32,
    pub restart_count: u32,
    pub performance_metrics: PerformanceMetrics,
    pub health_score: f64,
}

/// Health status
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Failed,
    Restarting,
}

/// Performance metrics
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub message_throughput: f64,
    pub error_rate: f64,
    pub response_time: Duration,
}

/// Restart strategies
#[derive(Debug, Clone)]
pub enum RestartStrategy {
    ImmediateRestart,
    ExponentialBackoff { 
        base_delay: Duration, 
        max_delay: Duration,
        current_delay: Duration,
    },
    CircuitBreaker { 
        failure_threshold: u32, 
        recovery_timeout: Duration,
        current_failures: u32,
        last_failure: Option<SystemTime>,
    },
    GracefulRestart { 
        drain_timeout: Duration 
    },
}

/// Supervision metrics
#[derive(Debug, Default)]
pub struct SupervisionMetrics {
    pub actors_supervised: u32,
    pub total_restarts: u64,
    pub successful_recoveries: u64,
    pub failed_recoveries: u64,
    pub health_checks_performed: u64,
    pub average_recovery_time: Duration,
    pub system_uptime: Duration,
}

/// Actor identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ActorId {
    Bridge,
    PegIn,
    PegOut,
    Stream,
}

impl BridgeSupervisor {
    /// Create new bridge supervisor
    pub fn new(config: SupervisionConfig) -> Self {
        let health_monitor = SupervisionHealthMonitor::new(config.health_check_interval);
        let recovery_coordinator = RecoveryCoordinator::new(config.max_restart_attempts);
        
        // Initialize restart strategies
        let mut restart_strategies = HashMap::new();
        restart_strategies.insert(ActorId::Bridge, RestartStrategy::GracefulRestart { 
            drain_timeout: Duration::from_secs(30) 
        });
        restart_strategies.insert(ActorId::PegIn, RestartStrategy::ExponentialBackoff {
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300),
            current_delay: Duration::from_secs(5),
        });
        restart_strategies.insert(ActorId::PegOut, RestartStrategy::ExponentialBackoff {
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300),
            current_delay: Duration::from_secs(5),
        });
        restart_strategies.insert(ActorId::Stream, RestartStrategy::CircuitBreaker {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(60),
            current_failures: 0,
            last_failure: None,
        });

        Self {
            config,
            bridge_actor: None,
            pegin_actor: None,
            pegout_actor: None,
            stream_actor: None,
            actor_health: HashMap::new(),
            restart_strategies,
            supervision_metrics: SupervisionMetrics::default(),
            system_registry: None,
            health_monitor,
            recovery_coordinator,
            started_at: SystemTime::now(),
        }
    }

    /// Initialize supervisor
    async fn initialize(&mut self, ctx: &mut Context<Self>) -> Result<(), SupervisionError> {
        info!("Initializing bridge supervisor");

        // Start supervised actors
        self.start_supervised_actors(ctx).await?;
        
        // Start supervision tasks
        self.start_health_monitoring(ctx);
        self.start_metrics_collection(ctx);
        
        // Update metrics
        self.supervision_metrics.actors_supervised = 4; // Bridge, PegIn, PegOut, Stream
        
        info!("Bridge supervisor initialized successfully");
        Ok(())
    }

    /// Start all supervised actors
    async fn start_supervised_actors(&mut self, ctx: &mut Context<Self>) -> Result<(), SupervisionError> {
        info!("Starting supervised bridge actors");

        // Start Bridge Actor (coordinator)
        let bridge_config = crate::actors::bridge::config::BridgeConfig::default();
        let bridge_actor = BridgeActor::new(bridge_config)
            .map_err(|e| SupervisionError::ActorStartFailed(format!("BridgeActor: {:?}", e)))?
            .start();
        
        self.bridge_actor = Some(bridge_actor);
        self.initialize_actor_health(ActorId::Bridge);

        // Start PegIn Actor
        let pegin_config = crate::actors::bridge::config::PegInConfig::default();
        let bitcoin_client = BitcoinClientFactory::create_mock(); // Use mock for testing
        let monitored_addresses = vec![]; // Would be populated from config
        
        let pegin_actor = PegInActor::new(pegin_config, bitcoin_client, monitored_addresses)
            .map_err(|e| SupervisionError::ActorStartFailed(format!("PegInActor: {:?}", e)))?
            .start();
        
        self.pegin_actor = Some(pegin_actor);
        self.initialize_actor_health(ActorId::PegIn);

        // Start PegOut Actor  
        let pegout_config = crate::actors::bridge::config::PegOutConfig::default();
        let utxo_manager = UtxoManager::new(
            bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").unwrap(),
            bitcoin::ScriptBuf::new(),
        );
        let bitcoin_client = BitcoinClientFactory::create_mock();
        let federation_config = actor_system::blockchain::FederationConfig::default();
        
        let pegout_actor = PegOutActor::new(pegout_config, utxo_manager, federation_config)
            .map_err(|e| SupervisionError::ActorStartFailed(format!("PegOutActor: {:?}", e)))?
            .start();
            
        self.pegout_actor = Some(pegout_actor);
        self.initialize_actor_health(ActorId::PegOut);

        // Start Stream Actor
        let stream_config = crate::actors::bridge::config::StreamConfig::default();
        let stream_actor = StreamActor::new(stream_config)
            .map_err(|e| SupervisionError::ActorStartFailed(format!("StreamActor: {:?}", e)))?
            .start();
            
        self.stream_actor = Some(stream_actor);
        self.initialize_actor_health(ActorId::Stream);

        // Register actors with bridge coordinator
        self.register_actors_with_coordinator().await?;

        info!("All supervised actors started successfully");
        Ok(())
    }

    /// Register actors with bridge coordinator
    async fn register_actors_with_coordinator(&mut self) -> Result<(), SupervisionError> {
        if let Some(bridge_actor) = &self.bridge_actor {
            // Register PegIn Actor
            if let Some(pegin_actor) = &self.pegin_actor {
                let msg = BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone());
                bridge_actor.send(msg).await
                    .map_err(|e| SupervisionError::RegistrationFailed(format!("PegInActor: {:?}", e)))?
                    .map_err(|e| SupervisionError::RegistrationFailed(format!("PegInActor: {:?}", e)))?;
            }

            // Register PegOut Actor
            if let Some(pegout_actor) = &self.pegout_actor {
                let msg = BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor.clone());
                bridge_actor.send(msg).await
                    .map_err(|e| SupervisionError::RegistrationFailed(format!("PegOutActor: {:?}", e)))?
                    .map_err(|e| SupervisionError::RegistrationFailed(format!("PegOutActor: {:?}", e)))?;
            }

            // Register Stream Actor
            if let Some(stream_actor) = &self.stream_actor {
                let msg = BridgeCoordinationMessage::RegisterStreamActor(stream_actor.clone());
                bridge_actor.send(msg).await
                    .map_err(|e| SupervisionError::RegistrationFailed(format!("StreamActor: {:?}", e)))?
                    .map_err(|e| SupervisionError::RegistrationFailed(format!("StreamActor: {:?}", e)))?;
            }
        }

        info!("Actors registered with bridge coordinator");
        Ok(())
    }

    /// Initialize actor health tracking
    fn initialize_actor_health(&mut self, actor_id: ActorId) {
        let health = ActorHealth {
            status: HealthStatus::Healthy,
            last_heartbeat: SystemTime::now(),
            failure_count: 0,
            restart_count: 0,
            performance_metrics: PerformanceMetrics::default(),
            health_score: 100.0,
        };
        
        self.actor_health.insert(actor_id, health);
    }

    /// Start health monitoring
    fn start_health_monitoring(&mut self, ctx: &mut Context<Self>) {
        let check_interval = self.config.health_check_interval;
        ctx.run_interval(check_interval, |actor, _ctx| {
            actor.perform_health_checks();
        });
    }

    /// Perform health checks on all actors
    fn perform_health_checks(&mut self) {
        self.supervision_metrics.health_checks_performed += 1;
        
        // Check each supervised actor
        for (actor_id, health) in &mut self.actor_health {
            let previous_status = health.status.clone();
            
            // Perform health check (simplified)
            let new_status = self.health_monitor.check_actor_health(actor_id);
            health.status = new_status.clone();
            health.last_heartbeat = SystemTime::now();

            // Handle status changes
            if !matches!(previous_status, new_status) {
                self.handle_health_status_change(actor_id.clone(), previous_status, new_status);
            }

            // Update health score
            health.health_score = self.calculate_health_score(health);
        }
    }

    /// Handle actor health status change
    fn handle_health_status_change(
        &mut self,
        actor_id: ActorId,
        previous_status: HealthStatus,
        new_status: HealthStatus,
    ) {
        info!("Actor {:?} health status changed: {:?} -> {:?}", actor_id, previous_status, new_status);

        match new_status {
            HealthStatus::Failed => {
                warn!("Actor {:?} has failed, initiating recovery", actor_id);
                self.initiate_actor_recovery(actor_id);
            }
            HealthStatus::Degraded => {
                warn!("Actor {:?} is degraded, monitoring closely", actor_id);
            }
            HealthStatus::Healthy => {
                if matches!(previous_status, HealthStatus::Failed | HealthStatus::Degraded) {
                    info!("Actor {:?} has recovered", actor_id);
                    self.supervision_metrics.successful_recoveries += 1;
                }
            }
            _ => {}
        }
    }

    /// Initiate actor recovery
    fn initiate_actor_recovery(&mut self, actor_id: ActorId) {
        if let Some(strategy) = self.restart_strategies.get(&actor_id) {
            self.recovery_coordinator.initiate_recovery(actor_id.clone(), strategy.clone());
            
            if let Some(health) = self.actor_health.get_mut(&actor_id) {
                health.restart_count += 1;
                health.failure_count += 1;
            }
            
            self.supervision_metrics.total_restarts += 1;
        }
    }

    /// Calculate health score
    fn calculate_health_score(&self, health: &ActorHealth) -> f64 {
        let mut score = 100.0;

        // Penalize failures
        score -= (health.failure_count as f64) * 10.0;
        
        // Penalize restarts
        score -= (health.restart_count as f64) * 5.0;

        // Factor in performance metrics
        score -= health.performance_metrics.error_rate * 20.0;

        score.max(0.0).min(100.0)
    }

    /// Start metrics collection
    fn start_metrics_collection(&mut self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::from_secs(30), |actor, _ctx| {
            actor.update_supervision_metrics();
        });
    }

    /// Update supervision metrics
    fn update_supervision_metrics(&mut self) {
        self.supervision_metrics.system_uptime = SystemTime::now()
            .duration_since(self.started_at)
            .unwrap_or_default();
    }

    /// Get system status
    pub fn get_system_status(&self) -> SupervisionSystemStatus {
        let actor_statuses: HashMap<ActorId, ActorHealth> = self.actor_health.clone();
        
        let overall_health = if actor_statuses.values().all(|h| matches!(h.status, HealthStatus::Healthy)) {
            SystemHealth::Healthy
        } else if actor_statuses.values().any(|h| matches!(h.status, HealthStatus::Failed)) {
            SystemHealth::Critical
        } else {
            SystemHealth::Degraded
        };

        SupervisionSystemStatus {
            overall_health,
            actor_statuses,
            metrics: self.supervision_metrics.clone(),
            uptime: self.supervision_metrics.system_uptime,
        }
    }
}

/// System status response
#[derive(Debug, Clone)]
pub struct SupervisionSystemStatus {
    pub overall_health: SystemHealth,
    pub actor_statuses: HashMap<ActorId, ActorHealth>,
    pub metrics: SupervisionMetrics,
    pub uptime: Duration,
}

/// System health
#[derive(Debug, Clone)]
pub enum SystemHealth {
    Healthy,
    Degraded,
    Critical,
}

impl Actor for BridgeSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Bridge supervisor starting");
        
        let fut = self.initialize(ctx);
        let fut = actix::fut::wrap_future::<_, Self>(fut);
        ctx.spawn(fut.map(|result, _actor, ctx| {
            match result {
                Ok(_) => {
                    info!("Bridge supervisor started successfully");
                }
                Err(e) => {
                    error!("Failed to initialize bridge supervisor: {:?}", e);
                    ctx.stop();
                }
            }
        }));
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Bridge supervisor stopped");
    }
}

/// Supervision errors
#[derive(Debug, thiserror::Error)]
pub enum SupervisionError {
    #[error("Actor start failed: {0}")]
    ActorStartFailed(String),
    
    #[error("Registration failed: {0}")]
    RegistrationFailed(String),
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    
    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}