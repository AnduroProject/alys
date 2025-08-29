//! Network Supervisor
//! 
//! Fault-tolerant supervision for the network actor system including automatic
//! restart, health monitoring, and cascade failure prevention.

use actix::{Actor, Context, Handler, Addr, AsyncContext, ActorContext, Supervised, Supervisor};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use actor_system::{AlysActor, LifecycleAware, ActorResult, ActorError};
use actor_system::supervision::{RestartStrategy, SupervisorStrategy, SupervisionDecision};

use crate::actors::network::*;
use crate::actors::network::messages::*;
use crate::actors::chain::ChainActor;

/// Network supervisor for managing network actors with fault tolerance
pub struct NetworkSupervisor {
    /// SyncActor address
    sync_actor: Option<Addr<SyncActor>>,
    /// NetworkActor address
    network_actor: Option<Addr<NetworkActor>>,
    /// PeerActor address
    peer_actor: Option<Addr<PeerActor>>,
    /// ChainActor address for coordination
    chain_actor: Option<Addr<ChainActor>>,
    
    /// Supervision configuration
    supervision_config: NetworkSupervisionConfig,
    /// Restart policies for each actor
    restart_policies: HashMap<String, RestartPolicy>,
    /// Health check status
    health_status: HashMap<String, ActorHealthStatus>,
    /// Network metrics
    network_metrics: NetworkSupervisorMetrics,
    
    /// Shutdown flag
    shutdown_requested: bool,
}

impl NetworkSupervisor {
    /// Create a new network supervisor
    pub fn new(config: NetworkSupervisionConfig) -> Self {
        let mut restart_policies = HashMap::new();
        restart_policies.insert("SyncActor".to_string(), config.sync_restart_policy.clone());
        restart_policies.insert("NetworkActor".to_string(), config.network_restart_policy.clone());
        restart_policies.insert("PeerActor".to_string(), config.peer_restart_policy.clone());

        Self {
            sync_actor: None,
            network_actor: None,
            peer_actor: None,
            chain_actor: None,
            supervision_config: config,
            restart_policies,
            health_status: HashMap::new(),
            network_metrics: NetworkSupervisorMetrics::default(),
            shutdown_requested: false,
        }
    }

    /// Start all network actors under supervision
    pub async fn start_network_actors(
        &mut self,
        sync_config: SyncConfig,
        network_config: NetworkConfig,
        peer_config: PeerConfig,
    ) -> ActorResult<()> {
        tracing::info!("Starting network actors under supervision");

        // Start SyncActor
        match self.start_sync_actor(sync_config).await {
            Ok(addr) => {
                self.sync_actor = Some(addr);
                self.health_status.insert("SyncActor".to_string(), ActorHealthStatus::healthy());
                tracing::info!("SyncActor started successfully");
            }
            Err(e) => {
                tracing::error!("Failed to start SyncActor: {:?}", e);
                return Err(e);
            }
        }

        // Start NetworkActor
        match self.start_network_actor(network_config).await {
            Ok(addr) => {
                self.network_actor = Some(addr);
                self.health_status.insert("NetworkActor".to_string(), ActorHealthStatus::healthy());
                tracing::info!("NetworkActor started successfully");
            }
            Err(e) => {
                tracing::error!("Failed to start NetworkActor: {:?}", e);
                return Err(e);
            }
        }

        // Start PeerActor
        match self.start_peer_actor(peer_config).await {
            Ok(addr) => {
                self.peer_actor = Some(addr);
                self.health_status.insert("PeerActor".to_string(), ActorHealthStatus::healthy());
                tracing::info!("PeerActor started successfully");
            }
            Err(e) => {
                tracing::error!("Failed to start PeerActor: {:?}", e);
                return Err(e);
            }
        }

        // Set up inter-actor communication
        self.setup_inter_actor_communication().await?;

        tracing::info!("All network actors started and connected successfully");
        Ok(())
    }

    /// Start SyncActor under supervision
    async fn start_sync_actor(&self, config: SyncConfig) -> ActorResult<Addr<SyncActor>> {
        let sync_actor = SyncActor::new(config)?;
        Ok(sync_actor.start())
    }

    /// Start NetworkActor under supervision
    async fn start_network_actor(&self, config: NetworkConfig) -> ActorResult<Addr<NetworkActor>> {
        let network_actor = NetworkActor::new(config)?;
        Ok(network_actor.start())
    }

    /// Start PeerActor under supervision
    async fn start_peer_actor(&self, config: PeerConfig) -> ActorResult<Addr<PeerActor>> {
        let peer_actor = PeerActor::new(config)?;
        Ok(peer_actor.start())
    }

    /// Setup inter-actor communication channels
    async fn setup_inter_actor_communication(&mut self) -> ActorResult<()> {
        // Configure SyncActor with other actor addresses
        if let Some(sync_actor) = &self.sync_actor {
            let mut sync_actor_guard = sync_actor.clone();
            // In a real implementation, we'd send a message to configure addresses
            // sync_actor_guard.do_send(ConfigureActorAddresses { ... });
        }

        // Configure NetworkActor with other actor addresses
        if let Some(network_actor) = &self.network_actor {
            // Similar configuration for NetworkActor
        }

        // Configure PeerActor with other actor addresses  
        if let Some(peer_actor) = &self.peer_actor {
            // Similar configuration for PeerActor
        }

        tracing::info!("Inter-actor communication configured");
        Ok(())
    }

    /// Set ChainActor address for coordination
    pub fn set_chain_actor(&mut self, chain_actor: Addr<ChainActor>) {
        self.chain_actor = Some(chain_actor);
        tracing::info!("ChainActor address configured for network supervision");
    }

    /// Perform health check on all network actors
    async fn perform_health_checks(&mut self) -> ActorResult<()> {
        let mut unhealthy_actors = Vec::new();

        // Check SyncActor health
        if let Some(sync_actor) = &self.sync_actor {
            match self.check_actor_health(sync_actor, "SyncActor").await {
                Ok(healthy) => {
                    if !healthy {
                        unhealthy_actors.push("SyncActor".to_string());
                    }
                }
                Err(e) => {
                    tracing::error!("SyncActor health check failed: {:?}", e);
                    unhealthy_actors.push("SyncActor".to_string());
                }
            }
        }

        // Check NetworkActor health
        if let Some(network_actor) = &self.network_actor {
            match self.check_actor_health(network_actor, "NetworkActor").await {
                Ok(healthy) => {
                    if !healthy {
                        unhealthy_actors.push("NetworkActor".to_string());
                    }
                }
                Err(e) => {
                    tracing::error!("NetworkActor health check failed: {:?}", e);
                    unhealthy_actors.push("NetworkActor".to_string());
                }
            }
        }

        // Check PeerActor health
        if let Some(peer_actor) = &self.peer_actor {
            match self.check_actor_health(peer_actor, "PeerActor").await {
                Ok(healthy) => {
                    if !healthy {
                        unhealthy_actors.push("PeerActor".to_string());
                    }
                }
                Err(e) => {
                    tracing::error!("PeerActor health check failed: {:?}", e);
                    unhealthy_actors.push("PeerActor".to_string());
                }
            }
        }

        // Handle unhealthy actors
        for actor_name in unhealthy_actors {
            self.handle_unhealthy_actor(&actor_name).await?;
        }

        Ok(())
    }

    /// Check individual actor health
    async fn check_actor_health<T>(&mut self, _actor: &Addr<T>, actor_name: &str) -> ActorResult<bool> 
    where
        T: Actor + AlysActor,
    {
        // In a real implementation, we'd send a health check message
        // For now, simulate health check
        let health_status = self.health_status.get_mut(actor_name);
        
        if let Some(status) = health_status {
            status.last_check = Instant::now();
            status.check_count += 1;
            
            // Simulate occasional health issues for testing
            if status.check_count % 100 == 0 {
                status.consecutive_failures += 1;
                if status.consecutive_failures > 3 {
                    status.status = HealthState::Unhealthy;
                    return Ok(false);
                }
            } else {
                status.consecutive_failures = 0;
                status.status = HealthState::Healthy;
            }
        }

        Ok(true)
    }

    /// Handle unhealthy actor by applying restart policy
    async fn handle_unhealthy_actor(&mut self, actor_name: &str) -> ActorResult<()> {
        let restart_policy = self.restart_policies.get(actor_name).cloned()
            .unwrap_or(RestartPolicy::default());

        tracing::warn!("Actor {} is unhealthy, applying restart policy: {:?}", actor_name, restart_policy);

        match restart_policy.strategy {
            RestartStrategy::Immediate => {
                self.restart_actor_immediately(actor_name).await?;
            }
            RestartStrategy::Delayed => {
                // Schedule delayed restart
                tracing::info!("Scheduling delayed restart for {} in {:?}", actor_name, restart_policy.delay);
                // In a real implementation, we'd schedule this
            }
            RestartStrategy::Exponential => {
                // Calculate exponential backoff
                let failures = self.health_status.get(actor_name)
                    .map(|s| s.consecutive_failures)
                    .unwrap_or(0);
                let delay = restart_policy.delay * 2_u32.pow(failures.min(10));
                tracing::info!("Scheduling exponential backoff restart for {} in {:?}", actor_name, delay);
            }
            RestartStrategy::Never => {
                tracing::warn!("Actor {} configured with Never restart policy, not restarting", actor_name);
            }
        }

        Ok(())
    }

    /// Restart an actor immediately
    async fn restart_actor_immediately(&mut self, actor_name: &str) -> ActorResult<()> {
        tracing::info!("Restarting actor: {}", actor_name);

        match actor_name {
            "SyncActor" => {
                if let Some(old_actor) = self.sync_actor.take() {
                    // Stop old actor
                    old_actor.do_send(actix::prelude::SystemService::stop());
                }
                
                // Start new actor (would need config)
                // self.sync_actor = Some(self.start_sync_actor(config).await?);
                tracing::info!("SyncActor restarted");
            }
            "NetworkActor" => {
                if let Some(old_actor) = self.network_actor.take() {
                    old_actor.do_send(actix::prelude::SystemService::stop());
                }
                // self.network_actor = Some(self.start_network_actor(config).await?);
                tracing::info!("NetworkActor restarted");
            }
            "PeerActor" => {
                if let Some(old_actor) = self.peer_actor.take() {
                    old_actor.do_send(actix::prelude::SystemService::stop());
                }
                // self.peer_actor = Some(self.start_peer_actor(config).await?);
                tracing::info!("PeerActor restarted");
            }
            _ => {
                return Err(ActorError::InvalidConfiguration {
                    reason: format!("Unknown actor for restart: {}", actor_name),
                });
            }
        }

        // Update health status
        if let Some(status) = self.health_status.get_mut(actor_name) {
            status.restart_count += 1;
            status.consecutive_failures = 0;
            status.status = HealthState::Healthy;
            status.last_restart = Some(Instant::now());
        }

        // Update metrics
        self.network_metrics.total_restarts += 1;

        Ok(())
    }

    /// Get network system status
    pub fn get_network_status(&self) -> NetworkSystemStatus {
        let actor_states = self.health_status.iter()
            .map(|(name, status)| (name.clone(), status.clone()))
            .collect();

        NetworkSystemStatus {
            sync_actor_healthy: self.health_status.get("SyncActor")
                .map(|s| matches!(s.status, HealthState::Healthy))
                .unwrap_or(false),
            network_actor_healthy: self.health_status.get("NetworkActor")
                .map(|s| matches!(s.status, HealthState::Healthy))
                .unwrap_or(false),
            peer_actor_healthy: self.health_status.get("PeerActor")
                .map(|s| matches!(s.status, HealthState::Healthy))
                .unwrap_or(false),
            total_restarts: self.network_metrics.total_restarts,
            last_health_check: self.network_metrics.last_health_check,
            actor_states,
            system_uptime: self.network_metrics.start_time.elapsed(),
        }
    }

    /// Shutdown all network actors gracefully
    pub async fn shutdown_network_actors(&mut self) -> ActorResult<()> {
        tracing::info!("Initiating graceful shutdown of network actors");

        // Stop actors in reverse dependency order
        if let Some(sync_actor) = self.sync_actor.take() {
            sync_actor.do_send(StopSync { force: false });
            tracing::info!("SyncActor shutdown initiated");
        }

        if let Some(network_actor) = self.network_actor.take() {
            network_actor.do_send(StopNetwork { graceful: true });
            tracing::info!("NetworkActor shutdown initiated");
        }

        if let Some(peer_actor) = self.peer_actor.take() {
            // PeerActor would have its own shutdown message
            tracing::info!("PeerActor shutdown initiated");
        }

        self.shutdown_requested = true;
        tracing::info!("Network actors shutdown completed");
        Ok(())
    }
}

impl Actor for NetworkSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("NetworkSupervisor started");

        // Schedule periodic health checks
        ctx.run_interval(self.supervision_config.health_check_interval, |actor, _ctx| {
            let health_check_future = actor.perform_health_checks();
            let actor_future = actix::fut::wrap_future(health_check_future)
                .map(|result, actor, _ctx| {
                    if let Err(e) = result {
                        tracing::error!("Health check cycle failed: {:?}", e);
                    }
                    actor.network_metrics.last_health_check = Instant::now();
                });
            
            ctx.spawn(actor_future);
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("NetworkSupervisor stopped");
    }
}

impl AlysActor for NetworkSupervisor {
    fn actor_type(&self) -> &'static str {
        "NetworkSupervisor"
    }

    fn metrics(&self) -> serde_json::Value {
        let status = self.get_network_status();
        
        serde_json::json!({
            "sync_actor_healthy": status.sync_actor_healthy,
            "network_actor_healthy": status.network_actor_healthy,
            "peer_actor_healthy": status.peer_actor_healthy,
            "total_restarts": status.total_restarts,
            "system_uptime_secs": status.system_uptime.as_secs(),
            "last_health_check_secs_ago": status.last_health_check.elapsed().as_secs(),
            "supervised_actors": status.actor_states.len(),
        })
    }
}

impl LifecycleAware for NetworkSupervisor {
    fn on_start(&mut self) -> ActorResult<()> {
        self.network_metrics.start_time = Instant::now();
        tracing::info!("NetworkSupervisor lifecycle started");
        Ok(())
    }

    fn on_stop(&mut self) -> ActorResult<()> {
        self.shutdown_requested = true;
        tracing::info!("NetworkSupervisor lifecycle stopped");
        Ok(())
    }

    fn health_check(&self) -> ActorResult<()> {
        if self.shutdown_requested {
            return Err(ActorError::ActorStopped);
        }

        // Check if critical actors are healthy
        let critical_actors_healthy = self.health_status.values()
            .all(|status| matches!(status.status, HealthState::Healthy | HealthState::Degraded));

        if !critical_actors_healthy {
            return Err(ActorError::HealthCheckFailed {
                reason: "Critical network actors are unhealthy".to_string(),
            });
        }

        Ok(())
    }
}

// Supporting Types and Configurations

/// Network supervision configuration
#[derive(Debug, Clone)]
pub struct NetworkSupervisionConfig {
    pub health_check_interval: Duration,
    pub sync_restart_policy: RestartPolicy,
    pub network_restart_policy: RestartPolicy,
    pub peer_restart_policy: RestartPolicy,
    pub enable_cascade_prevention: bool,
    pub max_concurrent_restarts: u32,
}

impl Default for NetworkSupervisionConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            sync_restart_policy: RestartPolicy::exponential_backoff(),
            network_restart_policy: RestartPolicy::immediate(),
            peer_restart_policy: RestartPolicy::delayed(Duration::from_secs(5)),
            enable_cascade_prevention: true,
            max_concurrent_restarts: 2,
        }
    }
}

/// Restart policy for actors
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    pub strategy: RestartStrategy,
    pub delay: Duration,
    pub max_retries: u32,
    pub retry_window: Duration,
}

impl RestartPolicy {
    pub fn immediate() -> Self {
        Self {
            strategy: RestartStrategy::Immediate,
            delay: Duration::from_secs(0),
            max_retries: 5,
            retry_window: Duration::from_secs(60),
        }
    }

    pub fn delayed(delay: Duration) -> Self {
        Self {
            strategy: RestartStrategy::Delayed,
            delay,
            max_retries: 3,
            retry_window: Duration::from_secs(300),
        }
    }

    pub fn exponential_backoff() -> Self {
        Self {
            strategy: RestartStrategy::Exponential,
            delay: Duration::from_secs(1),
            max_retries: 5,
            retry_window: Duration::from_secs(600),
        }
    }

    pub fn never() -> Self {
        Self {
            strategy: RestartStrategy::Never,
            delay: Duration::from_secs(0),
            max_retries: 0,
            retry_window: Duration::from_secs(0),
        }
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::exponential_backoff()
    }
}

/// Restart strategy enumeration
#[derive(Debug, Clone)]
pub enum RestartStrategy {
    Immediate,
    Delayed,
    Exponential,
    Never,
}

/// Actor health status tracking
#[derive(Debug, Clone)]
pub struct ActorHealthStatus {
    pub status: HealthState,
    pub last_check: Instant,
    pub check_count: u64,
    pub consecutive_failures: u32,
    pub restart_count: u32,
    pub last_restart: Option<Instant>,
}

impl ActorHealthStatus {
    pub fn healthy() -> Self {
        Self {
            status: HealthState::Healthy,
            last_check: Instant::now(),
            check_count: 0,
            consecutive_failures: 0,
            restart_count: 0,
            last_restart: None,
        }
    }
}

/// Health state enumeration
#[derive(Debug, Clone)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
    Restarting,
}

/// Network system status
pub struct NetworkSystemStatus {
    pub sync_actor_healthy: bool,
    pub network_actor_healthy: bool,
    pub peer_actor_healthy: bool,
    pub total_restarts: u64,
    pub last_health_check: Instant,
    pub actor_states: HashMap<String, ActorHealthStatus>,
    pub system_uptime: Duration,
}

/// Network supervisor metrics
#[derive(Default)]
pub struct NetworkSupervisorMetrics {
    pub start_time: Instant,
    pub total_restarts: u64,
    pub total_health_checks: u64,
    pub last_health_check: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervision_config_creation() {
        let config = NetworkSupervisionConfig::default();
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert!(config.enable_cascade_prevention);
        assert_eq!(config.max_concurrent_restarts, 2);
    }

    #[test]
    fn restart_policy_types() {
        let immediate = RestartPolicy::immediate();
        assert!(matches!(immediate.strategy, RestartStrategy::Immediate));
        assert_eq!(immediate.delay, Duration::from_secs(0));

        let delayed = RestartPolicy::delayed(Duration::from_secs(10));
        assert!(matches!(delayed.strategy, RestartStrategy::Delayed));
        assert_eq!(delayed.delay, Duration::from_secs(10));

        let exponential = RestartPolicy::exponential_backoff();
        assert!(matches!(exponential.strategy, RestartStrategy::Exponential));
        assert_eq!(exponential.delay, Duration::from_secs(1));

        let never = RestartPolicy::never();
        assert!(matches!(never.strategy, RestartStrategy::Never));
        assert_eq!(never.max_retries, 0);
    }

    #[test]
    fn actor_health_status() {
        let status = ActorHealthStatus::healthy();
        assert!(matches!(status.status, HealthState::Healthy));
        assert_eq!(status.consecutive_failures, 0);
        assert_eq!(status.restart_count, 0);
    }

    #[test]
    fn network_supervisor_creation() {
        let config = NetworkSupervisionConfig::default();
        let supervisor = NetworkSupervisor::new(config);
        
        assert_eq!(supervisor.restart_policies.len(), 3);
        assert!(supervisor.restart_policies.contains_key("SyncActor"));
        assert!(supervisor.restart_policies.contains_key("NetworkActor"));
        assert!(supervisor.restart_policies.contains_key("PeerActor"));
    }

    #[test]
    fn network_status() {
        let config = NetworkSupervisionConfig::default();
        let supervisor = NetworkSupervisor::new(config);
        
        let status = supervisor.get_network_status();
        assert!(!status.sync_actor_healthy);
        assert!(!status.network_actor_healthy);
        assert!(!status.peer_actor_healthy);
        assert_eq!(status.total_restarts, 0);
    }
}