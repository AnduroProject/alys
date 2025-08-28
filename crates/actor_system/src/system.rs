//! Alys root actor system implementation
//!
//! This module provides the root supervisor and system-wide coordination
//! for all Alys actors with hierarchical supervision and health monitoring.

use crate::{
    actor::{ActorFactory, ActorRegistry, AlysActor},
    error::{ActorError, ActorResult},
    lifecycle::{LifecycleManager, LifecycleMetadata},
    message::{AlysMessage, MessageEnvelope, MessagePriority},
    metrics::{ActorMetrics, MetricsCollector, AggregateStats},
    supervisor::{Supervisor, SupervisorMessage, SupervisorResponse, SupervisionPolicy},
};
use actix::{prelude::*, Addr, Recipient};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Alys root actor system
pub struct AlysSystem {
    /// System identifier
    system_id: String,
    /// Root supervisor
    root_supervisor: Option<Addr<Supervisor>>,
    /// Actor registry
    registry: Arc<RwLock<ActorRegistry>>,
    /// Lifecycle manager
    lifecycle_manager: Arc<LifecycleManager>,
    /// Metrics collector
    metrics_collector: Arc<MetricsCollector>,
    /// System configuration
    config: AlysSystemConfig,
    /// System start time
    start_time: SystemTime,
    /// System health status
    health_status: Arc<RwLock<SystemHealthStatus>>,
    /// Domain supervisors
    domain_supervisors: Arc<RwLock<HashMap<String, Addr<Supervisor>>>>,
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlysSystemConfig {
    /// System name
    pub system_name: String,
    /// Root supervision policy
    pub root_supervision_policy: SupervisionPolicy,
    /// System health check interval
    pub health_check_interval: Duration,
    /// Metrics collection interval
    pub metrics_interval: Duration,
    /// Maximum startup time for the system
    pub startup_timeout: Duration,
    /// Maximum shutdown time for the system
    pub shutdown_timeout: Duration,
    /// Enable automatic actor discovery
    pub auto_discovery: bool,
    /// System resource limits
    pub resource_limits: ResourceLimits,
}

impl Default for AlysSystemConfig {
    fn default() -> Self {
        Self {
            system_name: "alys-system".to_string(),
            root_supervision_policy: SupervisionPolicy::default(),
            health_check_interval: Duration::from_secs(30),
            metrics_interval: Duration::from_secs(10),
            startup_timeout: Duration::from_secs(120),
            shutdown_timeout: Duration::from_secs(30),
            auto_discovery: true,
            resource_limits: ResourceLimits::default(),
        }
    }
}

/// System resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum number of actors
    pub max_actors: usize,
    /// Maximum memory usage (bytes)
    pub max_memory_bytes: u64,
    /// Maximum CPU percentage
    pub max_cpu_percent: f64,
    /// Maximum file descriptors
    pub max_file_descriptors: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_actors: 10000,
            max_memory_bytes: 8 * 1024 * 1024 * 1024, // 8GB
            max_cpu_percent: 90.0,
            max_file_descriptors: 65536,
        }
    }
}

/// System health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthStatus {
    /// Overall system health
    pub is_healthy: bool,
    /// System uptime
    pub uptime: Duration,
    /// Total actors
    pub total_actors: usize,
    /// Healthy actors
    pub healthy_actors: usize,
    /// Failed actors
    pub failed_actors: usize,
    /// System resource usage
    pub resource_usage: ResourceUsage,
    /// Last health check time
    pub last_health_check: SystemTime,
    /// Health issues
    pub health_issues: Vec<String>,
}

/// Current resource usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage
    pub cpu_percent: f64,
    /// File descriptors in use
    pub file_descriptors: u32,
    /// Network connections
    pub network_connections: u32,
}

impl Default for SystemHealthStatus {
    fn default() -> Self {
        Self {
            is_healthy: true,
            uptime: Duration::ZERO,
            total_actors: 0,
            healthy_actors: 0,
            failed_actors: 0,
            resource_usage: ResourceUsage {
                memory_bytes: 0,
                cpu_percent: 0.0,
                file_descriptors: 0,
                network_connections: 0,
            },
            last_health_check: SystemTime::now(),
            health_issues: Vec::new(),
        }
    }
}

impl AlysSystem {
    /// Create new Alys system
    pub fn new(system_id: String, config: AlysSystemConfig) -> Self {
        let lifecycle_manager = Arc::new(LifecycleManager::new());
        let metrics_collector = Arc::new(MetricsCollector::new(config.metrics_interval));

        Self {
            system_id,
            root_supervisor: None,
            registry: Arc::new(RwLock::new(ActorRegistry::new())),
            lifecycle_manager,
            metrics_collector,
            config,
            start_time: SystemTime::now(),
            health_status: Arc::new(RwLock::new(SystemHealthStatus::default())),
            domain_supervisors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the Alys system
    pub async fn start(&mut self) -> ActorResult<()> {
        info!(system_id = %self.system_id, "Starting Alys actor system");

        // Start lifecycle manager
        let mut lifecycle_manager = Arc::try_unwrap(self.lifecycle_manager.clone())
            .unwrap_or_else(|arc| (*arc).clone());
        lifecycle_manager.start().await?;

        // Create root supervisor
        let root_supervisor = Supervisor::with_policy(
            "root_supervisor".to_string(),
            self.config.root_supervision_policy.clone(),
        ).start();

        self.root_supervisor = Some(root_supervisor);

        // Start metrics collection
        self.metrics_collector.start_collection();

        // Start health monitoring
        self.start_health_monitoring().await;

        info!(
            system_id = %self.system_id,
            startup_time = ?self.start_time.elapsed().unwrap_or_default(),
            "Alys actor system started successfully"
        );

        Ok(())
    }

    /// Stop the Alys system
    pub async fn stop(&mut self) -> ActorResult<()> {
        info!(system_id = %self.system_id, "Stopping Alys actor system");

        let shutdown_start = SystemTime::now();

        // Stop all domain supervisors
        {
            let supervisors = self.domain_supervisors.read().await;
            for (domain, supervisor) in supervisors.iter() {
                info!("Shutting down domain supervisor: {}", domain);
                let shutdown_msg = SupervisorMessage::Shutdown {
                    timeout: self.config.shutdown_timeout,
                };
                let _ = supervisor.try_send(shutdown_msg);
            }
        }

        // Stop root supervisor
        if let Some(root_supervisor) = &self.root_supervisor {
            let shutdown_msg = SupervisorMessage::Shutdown {
                timeout: self.config.shutdown_timeout,
            };
            let _ = root_supervisor.try_send(shutdown_msg);
        }

        // Stop lifecycle manager
        let mut lifecycle_manager = Arc::try_unwrap(self.lifecycle_manager.clone())
            .unwrap_or_else(|arc| (*arc).clone());
        lifecycle_manager.stop(self.config.shutdown_timeout).await?;

        let shutdown_duration = shutdown_start.elapsed().unwrap_or_default();
        info!(
            system_id = %self.system_id,
            shutdown_time = ?shutdown_duration,
            "Alys actor system stopped"
        );

        Ok(())
    }

    /// Create and register a domain supervisor
    pub async fn create_domain_supervisor(
        &mut self,
        domain: String,
        policy: Option<SupervisionPolicy>,
    ) -> ActorResult<Addr<Supervisor>> {
        let supervisor_id = format!("{}_supervisor", domain);
        let supervision_policy = policy.unwrap_or_else(|| self.config.root_supervision_policy.clone());

        let supervisor = Supervisor::with_policy(supervisor_id, supervision_policy).start();

        // Register with root supervisor if available
        if let Some(root_supervisor) = &self.root_supervisor {
            let parent_msg = SupervisorMessage::AddChild {
                child_id: domain.clone(),
                actor_type: "DomainSupervisor".to_string(),
                policy: None,
            };
            let _ = root_supervisor.try_send(parent_msg);
        }

        // Store domain supervisor
        {
            let mut supervisors = self.domain_supervisors.write().await;
            supervisors.insert(domain.clone(), supervisor.clone());
        }

        info!(domain = %domain, "Created domain supervisor");
        Ok(supervisor)
    }

    /// Register actor with the system
    pub async fn register_actor<A>(
        &mut self,
        actor_id: String,
        domain: String,
        config: A::Config,
    ) -> ActorResult<Addr<A>>
    where
        A: AlysActor + Actor<Context = Context<A>> + 'static,
        A::Config: Default,
    {
        // Ensure domain supervisor exists
        let domain_supervisor = {
            let supervisors = self.domain_supervisors.read().await;
            supervisors.get(&domain).cloned()
        };

        let domain_supervisor = match domain_supervisor {
            Some(supervisor) => supervisor,
            None => {
                // Create domain supervisor if it doesn't exist
                self.create_domain_supervisor(domain.clone(), None).await?
            }
        };

        // Create the actor
        let addr = ActorFactory::create_supervised_actor(
            actor_id.clone(),
            config,
            domain_supervisor.recipient(),
        ).await?;

        // Register with actor registry
        let metrics = Arc::new(ActorMetrics::new());
        {
            let mut registry = self.registry.write().await;
            registry.register(actor_id.clone(), addr.clone(), metrics.clone())?;
        }

        // Register with metrics collector
        self.metrics_collector.register_actor(actor_id.clone(), metrics);

        info!(
            actor_id = %actor_id,
            domain = %domain,
            actor_type = %std::any::type_name::<A>(),
            "Actor registered with system"
        );

        Ok(addr)
    }

    /// Unregister actor from the system
    pub async fn unregister_actor(&mut self, actor_id: &str) -> ActorResult<()> {
        // Remove from registry
        {
            let mut registry = self.registry.write().await;
            registry.unregister(actor_id)?;
        }

        // Remove from metrics collector
        self.metrics_collector.unregister_actor(actor_id);

        info!(actor_id = %actor_id, "Actor unregistered from system");
        Ok(())
    }

    /// Get system health status
    pub async fn get_health_status(&self) -> SystemHealthStatus {
        let health_status = self.health_status.read().await;
        let mut status = health_status.clone();
        
        // Update uptime
        status.uptime = self.start_time.elapsed().unwrap_or_default();
        
        status
    }

    /// Get system metrics
    pub async fn get_system_metrics(&self) -> AggregateStats {
        self.metrics_collector.get_aggregate_stats()
    }

    /// Get all registered actors
    pub async fn get_all_actors(&self) -> HashMap<String, String> {
        let registry = self.registry.read().await;
        registry
            .all_actors()
            .iter()
            .map(|(id, registration)| (id.clone(), registration.actor_type.clone()))
            .collect()
    }

    /// Perform system health check
    pub async fn perform_health_check(&self) -> ActorResult<SystemHealthStatus> {
        let mut health_issues = Vec::new();
        let mut healthy_actors = 0;
        let mut failed_actors = 0;

        // Check all actors
        let registry = self.registry.read().await;
        let total_actors = registry.all_actors().len();

        for (actor_id, registration) in registry.all_actors() {
            let metrics_snapshot = registration.metrics.snapshot();
            if metrics_snapshot.is_healthy() {
                healthy_actors += 1;
            } else {
                failed_actors += 1;
                health_issues.push(format!("Actor {} is unhealthy", actor_id));
            }
        }
        drop(registry);

        // Check resource usage
        let resource_usage = self.get_resource_usage().await;

        // Check resource limits
        if resource_usage.memory_bytes > self.config.resource_limits.max_memory_bytes {
            health_issues.push(format!(
                "Memory usage ({} MB) exceeds limit ({} MB)",
                resource_usage.memory_bytes / (1024 * 1024),
                self.config.resource_limits.max_memory_bytes / (1024 * 1024)
            ));
        }

        if resource_usage.cpu_percent > self.config.resource_limits.max_cpu_percent {
            health_issues.push(format!(
                "CPU usage ({:.1}%) exceeds limit ({:.1}%)",
                resource_usage.cpu_percent,
                self.config.resource_limits.max_cpu_percent
            ));
        }

        if total_actors > self.config.resource_limits.max_actors {
            health_issues.push(format!(
                "Actor count ({}) exceeds limit ({})",
                total_actors,
                self.config.resource_limits.max_actors
            ));
        }

        let is_healthy = health_issues.is_empty() && failed_actors == 0;

        let health_status = SystemHealthStatus {
            is_healthy,
            uptime: self.start_time.elapsed().unwrap_or_default(),
            total_actors,
            healthy_actors,
            failed_actors,
            resource_usage,
            last_health_check: SystemTime::now(),
            health_issues,
        };

        // Update stored health status
        {
            let mut stored_status = self.health_status.write().await;
            *stored_status = health_status.clone();
        }

        if !is_healthy {
            warn!(
                system_id = %self.system_id,
                health_issues = ?health_status.health_issues,
                "System health check failed"
            );
        }

        Ok(health_status)
    }

    /// Start health monitoring background task
    async fn start_health_monitoring(&self) {
        let system_id = self.system_id.clone();
        let health_status = self.health_status.clone();
        let interval = self.config.health_check_interval;
        let registry = self.registry.clone();
        let resource_limits = self.config.resource_limits.clone();
        let start_time = self.start_time;

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                // Perform health check
                let mut health_issues = Vec::new();
                let mut healthy_actors = 0;
                let mut failed_actors = 0;

                // Check actors
                {
                    let registry_guard = registry.read().await;
                    let total_actors = registry_guard.all_actors().len();

                    for (actor_id, registration) in registry_guard.all_actors() {
                        let metrics_snapshot = registration.metrics.snapshot();
                        if metrics_snapshot.is_healthy() {
                            healthy_actors += 1;
                        } else {
                            failed_actors += 1;
                            health_issues.push(format!("Actor {} is unhealthy", actor_id));
                        }
                    }

                    // Check resource limits
                    if total_actors > resource_limits.max_actors {
                        health_issues.push(format!(
                            "Actor count ({}) exceeds limit ({})",
                            total_actors,
                            resource_limits.max_actors
                        ));
                    }
                }

                let is_healthy = health_issues.is_empty() && failed_actors == 0;

                // Update health status
                {
                    let mut status = health_status.write().await;
                    status.is_healthy = is_healthy;
                    status.uptime = start_time.elapsed().unwrap_or_default();
                    status.healthy_actors = healthy_actors;
                    status.failed_actors = failed_actors;
                    status.last_health_check = SystemTime::now();
                    status.health_issues = health_issues;
                }

                debug!(
                    system_id = %system_id,
                    healthy = is_healthy,
                    healthy_actors,
                    failed_actors,
                    "Health check completed"
                );
            }
        });
    }

    /// Get current resource usage
    async fn get_resource_usage(&self) -> ResourceUsage {
        // This would typically interface with system monitoring tools
        // For now, return placeholder values
        ResourceUsage {
            memory_bytes: 0, // Would get actual memory usage
            cpu_percent: 0.0, // Would get actual CPU usage
            file_descriptors: 0, // Would get actual FD count
            network_connections: 0, // Would get actual connection count
        }
    }

    /// Get system configuration
    pub fn config(&self) -> &AlysSystemConfig {
        &self.config
    }

    /// Update system configuration
    pub async fn update_config(&mut self, new_config: AlysSystemConfig) -> ActorResult<()> {
        info!(system_id = %self.system_id, "Updating system configuration");
        self.config = new_config;
        Ok(())
    }

    /// Get actor registry
    pub fn registry(&self) -> Arc<RwLock<ActorRegistry>> {
        self.registry.clone()
    }

    /// Get lifecycle manager
    pub fn lifecycle_manager(&self) -> Arc<LifecycleManager> {
        self.lifecycle_manager.clone()
    }

    /// Get metrics collector
    pub fn metrics_collector(&self) -> Arc<MetricsCollector> {
        self.metrics_collector.clone()
    }
}

/// System messages
#[derive(Debug, Clone)]
pub enum SystemMessage {
    /// Get system status
    GetStatus,
    /// Get system metrics
    GetMetrics,
    /// Perform health check
    HealthCheck,
    /// Shutdown system
    Shutdown { timeout: Duration },
    /// Update configuration
    UpdateConfig { config: AlysSystemConfig },
    /// Get all registered actors
    GetActors,
    /// Register new domain
    RegisterDomain { domain: String, policy: Option<SupervisionPolicy> },
}

impl Message for SystemMessage {
    type Result = ActorResult<SystemResponse>;
}

impl AlysMessage for SystemMessage {
    fn priority(&self) -> MessagePriority {
        match self {
            SystemMessage::Shutdown { .. } => MessagePriority::Emergency,
            SystemMessage::HealthCheck => MessagePriority::High,
            _ => MessagePriority::Normal,
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            SystemMessage::Shutdown { timeout } => *timeout,
            _ => Duration::from_secs(30),
        }
    }
}

/// System response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemResponse {
    /// System status
    Status(SystemHealthStatus),
    /// System metrics
    Metrics(AggregateStats),
    /// Actor list
    Actors(HashMap<String, String>),
    /// Operation successful
    Success,
    /// Operation failed
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_config_defaults() {
        let config = AlysSystemConfig::default();
        assert_eq!(config.system_name, "alys-system");
        assert_eq!(config.startup_timeout, Duration::from_secs(120));
        assert_eq!(config.shutdown_timeout, Duration::from_secs(30));
        assert!(config.auto_discovery);
    }

    #[test]
    fn test_resource_limits_defaults() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_actors, 10000);
        assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024 * 1024);
        assert_eq!(limits.max_cpu_percent, 90.0);
        assert_eq!(limits.max_file_descriptors, 65536);
    }

    #[tokio::test]
    async fn test_system_creation() {
        let config = AlysSystemConfig::default();
        let system = AlysSystem::new("test_system".to_string(), config);
        
        assert_eq!(system.system_id, "test_system");
        assert!(system.root_supervisor.is_none());
    }

    #[tokio::test]
    async fn test_health_status_defaults() {
        let status = SystemHealthStatus::default();
        assert!(status.is_healthy);
        assert_eq!(status.total_actors, 0);
        assert_eq!(status.healthy_actors, 0);
        assert_eq!(status.failed_actors, 0);
        assert!(status.health_issues.is_empty());
    }
}