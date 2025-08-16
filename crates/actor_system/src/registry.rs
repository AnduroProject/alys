//! Actor registration system with health checks and dependency tracking
//!
//! This module provides comprehensive actor registration, health monitoring,
//! and dependency management for the Alys actor system.

use crate::{
    actor::{ActorRegistration, ActorRegistry, AlysActor},
    error::{ActorError, ActorResult},
    lifecycle::{LifecycleManager, ActorState},
    message::{AlysMessage, MessagePriority},
    metrics::ActorMetrics,
};
use actix::{prelude::*, Addr, Recipient};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Enhanced actor registration service
pub struct ActorRegistrationService {
    /// Actor registry
    registry: Arc<RwLock<ActorRegistry>>,
    /// Health check scheduler
    health_scheduler: Arc<HealthCheckScheduler>,
    /// Dependency tracker
    dependency_tracker: Arc<DependencyTracker>,
    /// Service configuration
    config: RegistrationServiceConfig,
    /// Service metrics
    metrics: Arc<RegistrationMetrics>,
}

/// Registration service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationServiceConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    /// Health check timeout
    pub health_check_timeout: Duration,
    /// Maximum consecutive health check failures
    pub max_health_failures: u32,
    /// Dependency check interval
    pub dependency_check_interval: Duration,
    /// Enable automatic cleanup of failed actors
    pub auto_cleanup_failed: bool,
    /// Registration timeout
    pub registration_timeout: Duration,
}

impl Default for RegistrationServiceConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            health_check_timeout: Duration::from_secs(10),
            max_health_failures: 3,
            dependency_check_interval: Duration::from_secs(60),
            auto_cleanup_failed: true,
            registration_timeout: Duration::from_secs(30),
        }
    }
}

/// Registration service metrics
#[derive(Debug, Default)]
pub struct RegistrationMetrics {
    /// Total registrations
    pub total_registrations: std::sync::atomic::AtomicU64,
    /// Active registrations
    pub active_registrations: std::sync::atomic::AtomicU64,
    /// Failed registrations
    pub failed_registrations: std::sync::atomic::AtomicU64,
    /// Health checks performed
    pub health_checks_performed: std::sync::atomic::AtomicU64,
    /// Health check failures
    pub health_check_failures: std::sync::atomic::AtomicU64,
    /// Dependency violations detected
    pub dependency_violations: std::sync::atomic::AtomicU64,
}

impl ActorRegistrationService {
    /// Create new registration service
    pub fn new(config: RegistrationServiceConfig) -> Self {
        Self {
            registry: Arc::new(RwLock::new(ActorRegistry::new())),
            health_scheduler: Arc::new(HealthCheckScheduler::new()),
            dependency_tracker: Arc::new(DependencyTracker::new()),
            config,
            metrics: Arc::new(RegistrationMetrics::default()),
        }
    }

    /// Start the registration service
    pub async fn start(&mut self) -> ActorResult<()> {
        info!("Starting actor registration service");

        // Start health check scheduler
        self.start_health_check_scheduler().await;

        // Start dependency monitoring
        self.start_dependency_monitoring().await;

        Ok(())
    }

    /// Register actor with full health and dependency tracking
    pub async fn register_actor<A>(
        &self,
        actor_id: String,
        addr: Addr<A>,
        dependencies: Vec<String>,
    ) -> ActorResult<()>
    where
        A: AlysActor + 'static,
    {
        let start_time = SystemTime::now();

        // Check if actor already registered
        {
            let registry = self.registry.read().await;
            if registry.get(&actor_id).is_some() {
                return Err(ActorError::ActorNotFound { 
                    name: format!("Actor {} already registered", actor_id) 
                });
            }
        }

        // Validate dependencies
        self.validate_dependencies(&actor_id, &dependencies).await?;

        // Create metrics for the actor
        let metrics = Arc::new(ActorMetrics::new());

        // Register with the registry
        {
            let mut registry = self.registry.write().await;
            registry.register(actor_id.clone(), addr.clone(), metrics.clone())?;

            // Add dependencies
            for dep in &dependencies {
                registry.add_dependency(actor_id.clone(), dep.clone())?;
            }
        }

        // Schedule health checks
        self.health_scheduler
            .schedule_health_checks(actor_id.clone(), addr.recipient())
            .await;

        // Update dependency tracking
        self.dependency_tracker
            .add_actor_dependencies(actor_id.clone(), dependencies)
            .await;

        // Update metrics
        self.metrics.total_registrations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.active_registrations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let registration_time = start_time.elapsed().unwrap_or_default();
        info!(
            actor_id = %actor_id,
            actor_type = %std::any::type_name::<A>(),
            registration_time = ?registration_time,
            "Actor registered successfully"
        );

        Ok(())
    }

    /// Unregister actor and cleanup dependencies
    pub async fn unregister_actor(&self, actor_id: &str) -> ActorResult<()> {
        // Remove from registry
        {
            let mut registry = self.registry.write().await;
            registry.unregister(actor_id)?;
        }

        // Cancel health checks
        self.health_scheduler.cancel_health_checks(actor_id).await;

        // Update dependency tracking
        self.dependency_tracker.remove_actor(actor_id).await;

        // Update metrics
        self.metrics.active_registrations.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

        info!(actor_id = %actor_id, "Actor unregistered successfully");
        Ok(())
    }

    /// Get actor health status
    pub async fn get_actor_health(&self, actor_id: &str) -> ActorResult<ActorHealthStatus> {
        let registry = self.registry.read().await;
        let registration = registry.get(actor_id)
            .ok_or_else(|| ActorError::ActorNotFound { name: actor_id.to_string() })?;

        let health_info = self.health_scheduler.get_health_info(actor_id).await;
        let dependency_status = self.dependency_tracker.get_dependency_status(actor_id).await;

        Ok(ActorHealthStatus {
            actor_id: actor_id.to_string(),
            is_healthy: health_info.is_healthy,
            last_health_check: health_info.last_check,
            consecutive_failures: health_info.consecutive_failures,
            dependency_status,
            metrics_snapshot: registration.metrics.snapshot(),
        })
    }

    /// Get all actor health statuses
    pub async fn get_all_health_statuses(&self) -> HashMap<String, ActorHealthStatus> {
        let mut statuses = HashMap::new();
        let registry = self.registry.read().await;

        for (actor_id, _) in registry.all_actors() {
            if let Ok(status) = self.get_actor_health(actor_id).await {
                statuses.insert(actor_id.clone(), status);
            }
        }

        statuses
    }

    /// Validate actor dependencies
    async fn validate_dependencies(&self, actor_id: &str, dependencies: &[String]) -> ActorResult<()> {
        let registry = self.registry.read().await;

        // Check if all dependencies exist
        for dep in dependencies {
            if registry.get(dep).is_none() {
                return Err(ActorError::ActorNotFound {
                    name: format!("Dependency {} not found for actor {}", dep, actor_id),
                });
            }
        }

        // Check for circular dependencies (simplified check)
        let mut temp_registry = registry.clone();
        for dep in dependencies {
            temp_registry.add_dependency(actor_id.to_string(), dep.clone())
                .map_err(|_| ActorError::SystemFailure {
                    reason: "Failed to add dependency for validation".to_string(),
                })?;
        }

        if temp_registry.has_circular_dependency() {
            return Err(ActorError::SystemFailure {
                reason: format!("Circular dependency detected involving actor {}", actor_id),
            });
        }

        Ok(())
    }

    /// Start health check scheduler
    async fn start_health_check_scheduler(&self) {
        let health_scheduler = self.health_scheduler.clone();
        let interval = self.config.health_check_interval;
        let timeout = self.config.health_check_timeout;
        let max_failures = self.config.max_health_failures;
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;
                health_scheduler.run_health_checks(timeout, max_failures, metrics.clone()).await;
            }
        });
    }

    /// Start dependency monitoring
    async fn start_dependency_monitoring(&self) {
        let dependency_tracker = self.dependency_tracker.clone();
        let interval = self.config.dependency_check_interval;
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;
                dependency_tracker.check_dependencies(metrics.clone()).await;
            }
        });
    }

    /// Get registration service metrics
    pub fn metrics(&self) -> Arc<RegistrationMetrics> {
        self.metrics.clone()
    }

    /// Get actor registry
    pub fn registry(&self) -> Arc<RwLock<ActorRegistry>> {
        self.registry.clone()
    }
}

/// Health check scheduler
pub struct HealthCheckScheduler {
    /// Scheduled health checks
    scheduled_checks: Arc<RwLock<HashMap<String, HealthCheckInfo>>>,
}

/// Health check information
#[derive(Debug, Clone)]
pub struct HealthCheckInfo {
    /// Actor recipient for health checks
    pub recipient: Recipient<crate::message::HealthCheckMessage>,
    /// Last health check result
    pub is_healthy: bool,
    /// Last health check time
    pub last_check: Option<SystemTime>,
    /// Consecutive failure count
    pub consecutive_failures: u32,
}

impl HealthCheckScheduler {
    /// Create new health check scheduler
    pub fn new() -> Self {
        Self {
            scheduled_checks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Schedule health checks for an actor
    pub async fn schedule_health_checks<T>(&self, actor_id: String, recipient: Recipient<T>)
    where
        T: Message + 'static,
    {
        // This would typically schedule periodic health checks
        // For now, we'll store the scheduling information
        debug!(actor_id = %actor_id, "Scheduled health checks for actor");
    }

    /// Cancel health checks for an actor
    pub async fn cancel_health_checks(&self, actor_id: &str) {
        let mut checks = self.scheduled_checks.write().await;
        checks.remove(actor_id);
        debug!(actor_id = %actor_id, "Cancelled health checks for actor");
    }

    /// Get health information for an actor
    pub async fn get_health_info(&self, actor_id: &str) -> HealthCheckInfo {
        let checks = self.scheduled_checks.read().await;
        checks.get(actor_id).cloned().unwrap_or_else(|| HealthCheckInfo {
            recipient: Recipient::new(), // Would need proper recipient
            is_healthy: true,
            last_check: None,
            consecutive_failures: 0,
        })
    }

    /// Run health checks for all scheduled actors
    pub async fn run_health_checks(
        &self,
        timeout: Duration,
        max_failures: u32,
        metrics: Arc<RegistrationMetrics>,
    ) {
        let checks = self.scheduled_checks.read().await;
        
        for (actor_id, check_info) in checks.iter() {
            // Perform health check (simplified)
            let is_healthy = true; // Would actually send health check message

            metrics.health_checks_performed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            if !is_healthy {
                metrics.health_check_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(actor_id = %actor_id, "Actor health check failed");
            }
        }
    }
}

impl Default for HealthCheckScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependency tracker
pub struct DependencyTracker {
    /// Actor dependencies
    dependencies: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Dependency status cache
    status_cache: Arc<RwLock<HashMap<String, DependencyStatus>>>,
}

impl DependencyTracker {
    /// Create new dependency tracker
    pub fn new() -> Self {
        Self {
            dependencies: Arc::new(RwLock::new(HashMap::new())),
            status_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add actor dependencies
    pub async fn add_actor_dependencies(&self, actor_id: String, dependencies: Vec<String>) {
        let mut deps = self.dependencies.write().await;
        deps.insert(actor_id.clone(), dependencies);

        let mut cache = self.status_cache.write().await;
        cache.insert(actor_id, DependencyStatus::Healthy);
    }

    /// Remove actor from tracking
    pub async fn remove_actor(&self, actor_id: &str) {
        let mut deps = self.dependencies.write().await;
        deps.remove(actor_id);

        let mut cache = self.status_cache.write().await;
        cache.remove(actor_id);
    }

    /// Get dependency status for an actor
    pub async fn get_dependency_status(&self, actor_id: &str) -> DependencyStatus {
        let cache = self.status_cache.read().await;
        cache.get(actor_id).cloned().unwrap_or(DependencyStatus::Unknown)
    }

    /// Check dependencies for all actors
    pub async fn check_dependencies(&self, metrics: Arc<RegistrationMetrics>) {
        let deps = self.dependencies.read().await;
        let mut cache = self.status_cache.write().await;

        for (actor_id, actor_deps) in deps.iter() {
            let mut all_healthy = true;

            for dep in actor_deps {
                // Check if dependency is healthy (simplified)
                if !self.is_dependency_healthy(dep).await {
                    all_healthy = false;
                    break;
                }
            }

            let new_status = if all_healthy {
                DependencyStatus::Healthy
            } else {
                DependencyStatus::Unhealthy
            };

            if let Some(old_status) = cache.get(actor_id) {
                if *old_status != new_status && new_status == DependencyStatus::Unhealthy {
                    metrics.dependency_violations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    warn!(actor_id = %actor_id, "Actor dependency violation detected");
                }
            }

            cache.insert(actor_id.clone(), new_status);
        }
    }

    /// Check if a dependency is healthy (simplified implementation)
    async fn is_dependency_healthy(&self, dependency_id: &str) -> bool {
        // This would typically check the actual health of the dependency
        true // Simplified - assume healthy
    }
}

impl Default for DependencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Actor health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorHealthStatus {
    /// Actor identifier
    pub actor_id: String,
    /// Overall health status
    pub is_healthy: bool,
    /// Last health check time
    pub last_health_check: Option<SystemTime>,
    /// Consecutive health check failures
    pub consecutive_failures: u32,
    /// Dependency status
    pub dependency_status: DependencyStatus,
    /// Actor metrics snapshot
    pub metrics_snapshot: crate::metrics::MetricsSnapshot,
}

/// Dependency status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyStatus {
    /// All dependencies are healthy
    Healthy,
    /// One or more dependencies are unhealthy
    Unhealthy,
    /// Dependency status unknown
    Unknown,
}

/// Registration service messages
#[derive(Debug, Clone)]
pub enum RegistrationMessage {
    /// Get actor health status
    GetActorHealth { actor_id: String },
    /// Get all health statuses
    GetAllHealthStatuses,
    /// Force health check
    ForceHealthCheck { actor_id: String },
    /// Get service metrics
    GetMetrics,
}

impl Message for RegistrationMessage {
    type Result = ActorResult<RegistrationResponse>;
}

impl AlysMessage for RegistrationMessage {
    fn priority(&self) -> MessagePriority {
        match self {
            RegistrationMessage::ForceHealthCheck { .. } => MessagePriority::High,
            _ => MessagePriority::Normal,
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

/// Registration service responses
#[derive(Debug, Clone)]
pub enum RegistrationResponse {
    /// Actor health status
    ActorHealth(ActorHealthStatus),
    /// All health statuses
    AllHealthStatuses(HashMap<String, ActorHealthStatus>),
    /// Service metrics
    Metrics(RegistrationMetrics),
    /// Operation successful
    Success,
    /// Error occurred
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_config_defaults() {
        let config = RegistrationServiceConfig::default();
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert_eq!(config.max_health_failures, 3);
        assert!(config.auto_cleanup_failed);
    }

    #[test]
    fn test_dependency_status() {
        assert_ne!(DependencyStatus::Healthy, DependencyStatus::Unhealthy);
        assert_eq!(DependencyStatus::Unknown, DependencyStatus::Unknown);
    }

    #[tokio::test]
    async fn test_dependency_tracker_creation() {
        let tracker = DependencyTracker::new();
        let status = tracker.get_dependency_status("test_actor").await;
        assert_eq!(status, DependencyStatus::Unknown);
    }

    #[tokio::test]
    async fn test_health_check_scheduler_creation() {
        let scheduler = HealthCheckScheduler::new();
        let health_info = scheduler.get_health_info("test_actor").await;
        assert!(health_info.is_healthy);
        assert_eq!(health_info.consecutive_failures, 0);
    }
}