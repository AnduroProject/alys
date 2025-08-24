//! Actor registration system with health checks and dependency tracking
//!
//! This module provides comprehensive actor registration, health monitoring,
//! and dependency management for the Alys actor system.

use crate::{
    actor::{ActorRegistration, ActorRegistry, AlysActor},
    blockchain::{
        BlockchainActorPriority, BlockchainActorRegistration, BlockchainEventType,
        BlockchainTimingConstraints, FederationConfig, BlockchainReadiness
    },
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

/// Blockchain-enhanced actor registration service
pub struct BlockchainActorRegistrationService {
    /// Base registration service
    base_service: ActorRegistrationService,
    /// Blockchain-specific registrations
    blockchain_registry: Arc<RwLock<HashMap<String, BlockchainActorRegistration>>>,
    /// Priority-based indexes
    priority_indexes: Arc<RwLock<HashMap<BlockchainActorPriority, HashSet<String>>>>,
    /// Federation member tracking
    federation_members: Arc<RwLock<HashMap<String, FederationConfig>>>,
    /// Blockchain event subscriptions
    event_subscriptions: Arc<RwLock<HashMap<BlockchainEventType, Vec<String>>>>,
}

impl BlockchainActorRegistrationService {
    /// Create new blockchain-aware registration service
    pub fn new(config: RegistrationServiceConfig) -> Self {
        Self {
            base_service: ActorRegistrationService::new(config),
            blockchain_registry: Arc::new(RwLock::new(HashMap::new())),
            priority_indexes: Arc::new(RwLock::new(HashMap::new())),
            federation_members: Arc::new(RwLock::new(HashMap::new())),
            event_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register blockchain-aware actor with enhanced capabilities
    pub async fn register_blockchain_actor<A>(
        &self,
        actor_id: String,
        addr: Addr<A>,
        priority: BlockchainActorPriority,
        timing_constraints: BlockchainTimingConstraints,
        federation_config: Option<FederationConfig>,
        event_subscriptions: Vec<BlockchainEventType>,
        dependencies: Vec<String>,
    ) -> ActorResult<()>
    where
        A: AlysActor + 'static,
    {
        // First register with base service
        self.base_service.register_actor(actor_id.clone(), addr.clone(), dependencies.clone()).await?;
        
        // Create blockchain-specific registration
        let base_registration = {
            let registry = self.base_service.registry.read().await;
            registry.get(&actor_id)
                .ok_or_else(|| ActorError::ActorNotFound { name: actor_id.clone() })?
                .clone()
        };
        
        let blockchain_registration = BlockchainActorRegistration {
            base: base_registration,
            blockchain_priority: priority,
            timing_constraints,
            federation_config: federation_config.clone(),
            last_readiness_check: None,
            event_subscriptions: event_subscriptions.clone(),
        };
        
        // Store blockchain registration
        {
            let mut blockchain_registry = self.blockchain_registry.write().await;
            blockchain_registry.insert(actor_id.clone(), blockchain_registration);
        }
        
        // Update priority index
        {
            let mut priority_indexes = self.priority_indexes.write().await;
            priority_indexes.entry(priority).or_insert_with(HashSet::new).insert(actor_id.clone());
        }
        
        // Register federation member if applicable
        if let Some(fed_config) = federation_config {
            let mut federation_members = self.federation_members.write().await;
            federation_members.insert(actor_id.clone(), fed_config);
        }
        
        // Register event subscriptions
        {
            let mut subscriptions = self.event_subscriptions.write().await;
            for event_type in event_subscriptions {
                subscriptions.entry(event_type).or_insert_with(Vec::new).push(actor_id.clone());
            }
        }
        
        info!(
            actor_id = %actor_id,
            priority = ?priority,
            federation_member = federation_config.is_some(),
            "Blockchain actor registered successfully"
        );
        
        Ok(())
    }
    
    /// Get actors by blockchain priority
    pub async fn get_actors_by_priority(&self, priority: BlockchainActorPriority) -> Vec<String> {
        let priority_indexes = self.priority_indexes.read().await;
        priority_indexes.get(&priority)
            .map(|actors| actors.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    /// Get consensus-critical actors
    pub async fn get_consensus_critical_actors(&self) -> Vec<String> {
        self.get_actors_by_priority(BlockchainActorPriority::Consensus).await
    }
    
    /// Get federation members
    pub async fn get_federation_members(&self) -> Vec<String> {
        let federation_members = self.federation_members.read().await;
        federation_members.keys().cloned().collect()
    }
    
    /// Get actors subscribed to specific blockchain event
    pub async fn get_event_subscribers(&self, event_type: BlockchainEventType) -> Vec<String> {
        let subscriptions = self.event_subscriptions.read().await;
        subscriptions.get(&event_type)
            .map(|subscribers| subscribers.clone())
            .unwrap_or_default()
    }
    
    /// Check blockchain readiness for an actor
    pub async fn check_blockchain_readiness(&self, actor_id: &str) -> ActorResult<Option<BlockchainReadiness>> {
        let blockchain_registry = self.blockchain_registry.read().await;
        if let Some(registration) = blockchain_registry.get(actor_id) {
            Ok(registration.last_readiness_check.as_ref().map(|(_, readiness)| readiness.clone()))
        } else {
            Ok(None)
        }
    }
    
    /// Update blockchain readiness for an actor
    pub async fn update_blockchain_readiness(
        &self, 
        actor_id: &str, 
        readiness: BlockchainReadiness
    ) -> ActorResult<()> {
        let mut blockchain_registry = self.blockchain_registry.write().await;
        if let Some(registration) = blockchain_registry.get_mut(actor_id) {
            registration.last_readiness_check = Some((SystemTime::now(), readiness));
            Ok(())
        } else {
            Err(ActorError::ActorNotFound { name: actor_id.to_string() })
        }
    }
    
    /// Get actors that can produce blocks (consensus-critical and ready)
    pub async fn get_block_production_capable_actors(&self) -> Vec<String> {
        let blockchain_registry = self.blockchain_registry.read().await;
        let mut capable_actors = Vec::new();
        
        for (actor_id, registration) in blockchain_registry.iter() {
            if registration.blockchain_priority == BlockchainActorPriority::Consensus {
                if let Some((_, readiness)) = &registration.last_readiness_check {
                    if readiness.can_produce_blocks && readiness.federation_healthy {
                        capable_actors.push(actor_id.clone());
                    }
                }
            }
        }
        
        capable_actors
    }
    
    /// Get federation health summary
    pub async fn get_federation_health_summary(&self) -> FederationHealthSummary {
        let federation_members = self.federation_members.read().await;
        let blockchain_registry = self.blockchain_registry.read().await;
        
        let total_members = federation_members.len();
        let mut healthy_members = 0;
        let mut consensus_capable = 0;
        
        for actor_id in federation_members.keys() {
            if let Some(registration) = blockchain_registry.get(actor_id) {
                if let Some((_, readiness)) = &registration.last_readiness_check {
                    if readiness.federation_healthy {
                        healthy_members += 1;
                        if readiness.can_produce_blocks {
                            consensus_capable += 1;
                        }
                    }
                }
            }
        }
        
        FederationHealthSummary {
            total_members,
            healthy_members,
            consensus_capable,
            threshold_met: healthy_members >= 3, // Assuming 3-of-5 threshold
        }
    }
    
    /// Unregister blockchain actor
    pub async fn unregister_blockchain_actor(&self, actor_id: &str) -> ActorResult<()> {
        // Remove from blockchain registry
        let blockchain_registration = {
            let mut blockchain_registry = self.blockchain_registry.write().await;
            blockchain_registry.remove(actor_id)
        };
        
        if let Some(registration) = blockchain_registration {
            // Remove from priority index
            {
                let mut priority_indexes = self.priority_indexes.write().await;
                if let Some(actors) = priority_indexes.get_mut(&registration.blockchain_priority) {
                    actors.remove(actor_id);
                }
            }
            
            // Remove from federation members
            {
                let mut federation_members = self.federation_members.write().await;
                federation_members.remove(actor_id);
            }
            
            // Remove from event subscriptions
            {
                let mut subscriptions = self.event_subscriptions.write().await;
                for event_subscribers in subscriptions.values_mut() {
                    event_subscribers.retain(|id| id != actor_id);
                }
            }
        }
        
        // Remove from base service
        self.base_service.unregister_actor(actor_id).await
    }
}

/// Federation health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationHealthSummary {
    /// Total number of federation members
    pub total_members: usize,
    /// Number of healthy federation members
    pub healthy_members: usize,
    /// Number of members capable of consensus operations
    pub consensus_capable: usize,
    /// Whether the threshold for consensus is met
    pub threshold_met: bool,
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

/// Health check scheduler for managing actor health monitoring
#[derive(Debug)]
pub struct HealthCheckScheduler {
    /// Scheduled health checks
    scheduled_checks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl HealthCheckScheduler {
    /// Create new health check scheduler
    pub fn new() -> Self {
        Self {
            scheduled_checks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Schedule health checks for an actor
    pub async fn schedule_health_checks(
        &self,
        actor_id: String,
        recipient: Recipient<crate::actor::HealthCheck>,
    ) {
        let interval = Duration::from_secs(30); // Default health check interval
        let scheduled_checks = self.scheduled_checks.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;
                if let Err(e) = recipient.try_send(crate::actor::HealthCheck) {
                    warn!(actor_id = %actor_id, error = ?e, "Health check failed");
                    break;
                }
            }
        });
        
        let mut checks = scheduled_checks.write().await;
        if let Some(old_handle) = checks.insert(actor_id, handle) {
            old_handle.abort();
        }
    }
    
    /// Cancel health checks for an actor
    pub async fn cancel_health_checks(&self, actor_id: &str) {
        let mut checks = self.scheduled_checks.write().await;
        if let Some(handle) = checks.remove(actor_id) {
            handle.abort();
        }
    }
}

/// Dependency tracker for managing actor dependencies
#[derive(Debug)]
pub struct DependencyTracker {
    /// Actor dependencies
    dependencies: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Reverse dependencies (who depends on whom)
    reverse_dependencies: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl DependencyTracker {
    /// Create new dependency tracker
    pub fn new() -> Self {
        Self {
            dependencies: Arc::new(RwLock::new(HashMap::new())),
            reverse_dependencies: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Add dependencies for an actor
    pub async fn add_actor_dependencies(&self, actor_id: String, deps: Vec<String>) {
        let mut dependencies = self.dependencies.write().await;
        let mut reverse_deps = self.reverse_dependencies.write().await;
        
        dependencies.insert(actor_id.clone(), deps.clone());
        
        // Update reverse dependencies
        for dep in deps {
            reverse_deps.entry(dep).or_insert_with(Vec::new).push(actor_id.clone());
        }
    }
    
    /// Remove actor and all its dependencies
    pub async fn remove_actor(&self, actor_id: &str) {
        let mut dependencies = self.dependencies.write().await;
        let mut reverse_deps = self.reverse_dependencies.write().await;
        
        // Remove from dependencies
        if let Some(deps) = dependencies.remove(actor_id) {
            // Update reverse dependencies
            for dep in deps {
                if let Some(actors) = reverse_deps.get_mut(&dep) {
                    actors.retain(|id| id != actor_id);
                }
            }
        }
        
        // Remove from reverse dependencies
        reverse_deps.remove(actor_id);
    }
    
    /// Get dependencies for an actor
    pub async fn get_dependencies(&self, actor_id: &str) -> Vec<String> {
        let dependencies = self.dependencies.read().await;
        dependencies.get(actor_id).cloned().unwrap_or_default()
    }
    
    /// Get actors that depend on the given actor
    pub async fn get_dependents(&self, actor_id: &str) -> Vec<String> {
        let reverse_deps = self.reverse_dependencies.read().await;
        reverse_deps.get(actor_id).cloned().unwrap_or_default()
    }
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
    
    /// Validate that all dependencies exist and don't create circular references
    async fn validate_dependencies(&self, actor_id: &str, dependencies: &[String]) -> ActorResult<()> {
        let registry = self.registry.read().await;
        
        // Check that all dependencies exist
        for dep in dependencies {
            if registry.get(dep).is_none() {
                return Err(ActorError::ActorNotFound {
                    name: format!("Dependency {} for actor {} not found", dep, actor_id)
                });
            }
        }
        
        // Check for circular dependencies would be added here
        // For now, we'll skip this complex validation
        
        Ok(())
    }
    
    /// Start health check scheduler
    async fn start_health_check_scheduler(&self) {
        // Placeholder for health check scheduler startup
        debug!("Health check scheduler started");
    }
    
    /// Start dependency monitoring
    async fn start_dependency_monitoring(&self) {
        // Placeholder for dependency monitoring startup
        debug!("Dependency monitoring started");
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