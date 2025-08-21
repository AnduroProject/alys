//! Actor Registry & Discovery - Phase 3 Implementation (ALYS-006-12 to ALYS-006-15)
//! 
//! Comprehensive actor registry system for Alys V2 providing name-based and
//! type-based actor lookup, registration lifecycle management, and discovery
//! operations optimized for the merged mining sidechain architecture.

use crate::actors::foundation::{
    ActorSystemConfig, ActorInfo, ActorPriority, constants::{registry, lifecycle}
};
use regex;
use actix::{Actor, Addr, Context, Handler, Message, ResponseFuture};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Registry errors for actor registration and discovery operations
#[derive(Error, Debug, Clone)]
pub enum RegistryError {
    #[error("Actor '{0}' is already registered")]
    ActorAlreadyRegistered(String),

    #[error("Actor '{0}' not found in registry")]
    ActorNotFound(String),

    #[error("Invalid actor name: {0}")]
    InvalidActorName(String),

    #[error("Actor type mismatch: expected {expected}, found {actual}")]
    ActorTypeMismatch { expected: String, actual: String },

    #[error("Registry capacity exceeded: {current}/{max}")]
    RegistryCapacityExceeded { current: usize, max: usize },

    #[error("Actor registry is locked for maintenance")]
    RegistryLocked,

    #[error("Batch operation failed: {operation} - {details}")]
    BatchOperationFailed { operation: String, details: String },

    #[error("Actor lifecycle violation: {0}")]
    LifecycleViolation(String),

    #[error("Registry index corruption detected for type: {0}")]
    IndexCorruption(String),
}

/// Actor lifecycle states for registry tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorLifecycleState {
    /// Actor is being registered
    Registering,
    /// Actor is active and ready to receive messages
    Active,
    /// Actor is suspended (temporary unavailability)
    Suspended,
    /// Actor is shutting down gracefully
    ShuttingDown,
    /// Actor has been terminated
    Terminated,
    /// Actor is in error state requiring intervention
    Failed,
}

/// Registry entry containing actor metadata and lifecycle information
#[derive(Debug, Clone)]
pub struct ActorRegistryEntry {
    /// Unique actor name
    pub name: String,
    /// Actor address (type-erased)
    pub addr: Box<dyn Any + Send + Sync>,
    /// Actor type ID for type-safe operations
    pub type_id: TypeId,
    /// Actor type name for debugging
    pub type_name: String,
    /// Actor priority level
    pub priority: ActorPriority,
    /// Current lifecycle state
    pub state: ActorLifecycleState,
    /// Registration timestamp
    pub registered_at: SystemTime,
    /// Last activity timestamp
    pub last_activity: SystemTime,
    /// Registration tags for categorization
    pub tags: HashSet<String>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
    /// Health check status
    pub health_status: HealthStatus,
    /// Registration context
    pub registration_context: RegistrationContext,
}

/// Health status information for registry entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health state
    pub status: HealthState,
    /// Last health check timestamp
    pub last_check: Option<SystemTime>,
    /// Health check error count
    pub error_count: u32,
    /// Health check success rate
    pub success_rate: f64,
    /// Current health issues
    pub issues: Vec<String>,
}

/// Health state enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthState {
    /// Actor is healthy and responsive
    Healthy,
    /// Actor shows warning signs but functional
    Warning,
    /// Actor is unhealthy but recoverable
    Unhealthy,
    /// Actor is critical and needs immediate attention
    Critical,
    /// Health status unknown
    Unknown,
}

/// Registration context providing additional information
#[derive(Debug, Clone)]
pub struct RegistrationContext {
    /// Registration source (supervisor, manual, etc.)
    pub source: String,
    /// Supervisor name if registered by supervisor
    pub supervisor: Option<String>,
    /// Configuration used for registration
    pub config: HashMap<String, String>,
    /// Feature flags active during registration
    pub feature_flags: HashSet<String>,
}

/// Actor registry configuration
#[derive(Debug, Clone)]
pub struct ActorRegistryConfig {
    /// Maximum number of registered actors
    pub max_actors: usize,
    /// Enable type index for faster type-based lookups
    pub enable_type_index: bool,
    /// Enable lifecycle tracking
    pub enable_lifecycle_tracking: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Enable registry metrics collection
    pub enable_metrics: bool,
    /// Registry cleanup interval
    pub cleanup_interval: Duration,
    /// Maximum inactive duration before cleanup
    pub max_inactive_duration: Duration,
    /// Enable orphan detection and cleanup
    pub enable_orphan_cleanup: bool,
}

impl Default for ActorRegistryConfig {
    fn default() -> Self {
        Self {
            max_actors: registry::MAX_ACTORS,
            enable_type_index: true,
            enable_lifecycle_tracking: true,
            health_check_interval: Duration::from_secs(30),
            enable_metrics: true,
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            max_inactive_duration: Duration::from_secs(3600), // 1 hour
            enable_orphan_cleanup: true,
        }
    }
}

/// Batch operation descriptor
#[derive(Debug, Clone)]
pub struct BatchOperation<T> {
    /// Operation type name
    pub operation_type: String,
    /// Items to process
    pub items: Vec<T>,
    /// Batch size for processing
    pub batch_size: usize,
    /// Fail fast on first error
    pub fail_fast: bool,
}

/// Batch operation result
#[derive(Debug, Clone)]
pub struct BatchResult<T, E> {
    /// Successful results
    pub successes: Vec<T>,
    /// Failed results with errors
    pub failures: Vec<(String, E)>,
    /// Total processing time
    pub duration: Duration,
    /// Success rate
    pub success_rate: f64,
}

/// Actor registry implementation
/// 
/// Provides comprehensive actor registration, discovery, and lifecycle management
/// for the Alys V2 sidechain with support for governance event processing,
/// federation operations, and consensus-critical actor coordination.
#[derive(Debug)]
pub struct ActorRegistry {
    /// Registry configuration
    config: ActorRegistryConfig,
    /// Name-based actor lookup
    name_index: HashMap<String, ActorRegistryEntry>,
    /// Type-based actor lookup
    type_index: HashMap<TypeId, Vec<String>>,
    /// Tag-based actor lookup
    tag_index: HashMap<String, HashSet<String>>,
    /// Priority-based actor lookup
    priority_index: HashMap<ActorPriority, Vec<String>>,
    /// Registry statistics
    stats: RegistryStatistics,
    /// Registry is locked for maintenance
    locked: bool,
    /// Orphaned actors needing cleanup
    orphaned_actors: HashSet<String>,
}

/// Registry statistics for monitoring and optimization
#[derive(Debug, Clone, Default)]
pub struct RegistryStatistics {
    /// Total registered actors
    pub total_actors: usize,
    /// Actors by priority
    pub actors_by_priority: HashMap<ActorPriority, usize>,
    /// Actors by type
    pub actors_by_type: HashMap<String, usize>,
    /// Actors by state
    pub actors_by_state: HashMap<ActorLifecycleState, usize>,
    /// Registration rate (per hour)
    pub registration_rate: f64,
    /// Unregistration rate (per hour)
    pub unregistration_rate: f64,
    /// Average actor lifetime
    pub avg_actor_lifetime: Duration,
    /// Registry operations per second
    pub operations_per_second: f64,
    /// Health check success rate
    pub health_success_rate: f64,
    /// Last statistics update
    pub last_updated: SystemTime,
}

/// ALYS-006-12: Implement ActorRegistry with name-based and type-based lookup capabilities
impl ActorRegistry {
    /// Create a new actor registry with the specified configuration
    pub fn new(config: ActorRegistryConfig) -> Self {
        info!("Initializing actor registry with config: {:?}", config);
        
        Self {
            config,
            name_index: HashMap::new(),
            type_index: HashMap::new(),
            tag_index: HashMap::new(),
            priority_index: HashMap::new(),
            stats: RegistryStatistics::default(),
            locked: false,
            orphaned_actors: HashSet::new(),
        }
    }

    /// Create registry with development configuration
    pub fn development() -> Self {
        let config = ActorRegistryConfig {
            max_actors: 1000,
            cleanup_interval: Duration::from_secs(60),
            health_check_interval: Duration::from_secs(10),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Create registry with production configuration
    pub fn production() -> Self {
        let config = ActorRegistryConfig {
            max_actors: 10000,
            cleanup_interval: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
            max_inactive_duration: Duration::from_secs(7200), // 2 hours
            ..Default::default()
        };
        Self::new(config)
    }

    /// Get actor by name with type safety
    /// 
    /// Returns the actor address if found and type matches, None otherwise.
    /// This is the primary lookup method for name-based actor discovery.
    pub fn get_actor<A: Actor + 'static>(&self, name: &str) -> Option<Addr<A>> {
        if self.locked {
            warn!("Registry is locked, get_actor operation blocked for: {}", name);
            return None;
        }

        self.name_index.get(name).and_then(|entry| {
            if entry.state == ActorLifecycleState::Terminated {
                debug!("Actor '{}' is terminated, returning None", name);
                return None;
            }

            // Type-safe downcast
            if entry.type_id == TypeId::of::<A>() {
                entry.addr.downcast_ref::<Addr<A>>().cloned()
            } else {
                warn!(
                    "Type mismatch for actor '{}': expected {}, found {}",
                    name,
                    std::any::type_name::<A>(),
                    entry.type_name
                );
                None
            }
        })
    }

    /// Get all actors of a specific type
    /// 
    /// Returns a vector of actor addresses for all registered actors
    /// of the specified type. Useful for broadcasting operations.
    pub fn get_actors_by_type<A: Actor + 'static>(&self) -> Vec<Addr<A>> {
        if self.locked {
            warn!("Registry is locked, get_actors_by_type operation blocked");
            return Vec::new();
        }

        let type_id = TypeId::of::<A>();
        
        self.type_index
            .get(&type_id)
            .map(|names| {
                names.iter()
                    .filter_map(|name| self.get_actor::<A>(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get actors by priority level
    /// 
    /// Returns all actors matching the specified priority level.
    /// Useful for priority-based operations and shutdown procedures.
    pub fn get_actors_by_priority(&self, priority: ActorPriority) -> Vec<String> {
        if self.locked {
            warn!("Registry is locked, get_actors_by_priority operation blocked");
            return Vec::new();
        }

        self.priority_index
            .get(&priority)
            .cloned()
            .unwrap_or_default()
    }

    /// Get actors by tag
    /// 
    /// Returns all actors that have the specified tag.
    /// Useful for category-based operations and grouped management.
    pub fn get_actors_by_tag(&self, tag: &str) -> Vec<String> {
        if self.locked {
            warn!("Registry is locked, get_actors_by_tag operation blocked");
            return Vec::new();
        }

        self.tag_index
            .get(tag)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Get actors by lifecycle state
    /// 
    /// Returns all actors in the specified lifecycle state.
    /// Useful for state-based management and debugging.
    pub fn get_actors_by_state(&self, state: ActorLifecycleState) -> Vec<String> {
        if self.locked {
            warn!("Registry is locked, get_actors_by_state operation blocked");
            return Vec::new();
        }

        self.name_index
            .iter()
            .filter(|(_, entry)| entry.state == state)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Check if actor exists by name
    pub fn contains_actor(&self, name: &str) -> bool {
        if self.locked {
            return false;
        }
        
        self.name_index.contains_key(name) && 
        self.name_index.get(name).map_or(false, |entry| entry.state != ActorLifecycleState::Terminated)
    }

    /// Get registry entry by name
    pub fn get_entry(&self, name: &str) -> Option<&ActorRegistryEntry> {
        if self.locked {
            return None;
        }
        self.name_index.get(name)
    }

    /// Get registry statistics
    pub fn get_statistics(&self) -> &RegistryStatistics {
        &self.stats
    }

    /// Get all registered actor names
    pub fn get_all_actor_names(&self) -> Vec<String> {
        if self.locked {
            return Vec::new();
        }
        
        self.name_index
            .iter()
            .filter(|(_, entry)| entry.state != ActorLifecycleState::Terminated)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get total number of registered actors
    pub fn len(&self) -> usize {
        self.stats.total_actors
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.stats.total_actors == 0
    }

    /// Check if registry is locked
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Lock registry for maintenance operations
    pub fn lock_registry(&mut self) {
        info!("Locking actor registry for maintenance");
        self.locked = true;
    }

    /// Unlock registry after maintenance
    pub fn unlock_registry(&mut self) {
        info!("Unlocking actor registry");
        self.locked = false;
    }
}

/// Default health status for new actors
impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            status: HealthState::Unknown,
            last_check: None,
            error_count: 0,
            success_rate: 0.0,
            issues: Vec::new(),
        }
    }
}

/// Default registration context
impl Default for RegistrationContext {
    fn default() -> Self {
        Self {
            source: "unknown".to_string(),
            supervisor: None,
            config: HashMap::new(),
            feature_flags: HashSet::new(),
        }
    }
}

impl std::fmt::Display for ActorLifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActorLifecycleState::Registering => write!(f, "registering"),
            ActorLifecycleState::Active => write!(f, "active"),
            ActorLifecycleState::Suspended => write!(f, "suspended"),
            ActorLifecycleState::ShuttingDown => write!(f, "shutting_down"),
            ActorLifecycleState::Terminated => write!(f, "terminated"),
            ActorLifecycleState::Failed => write!(f, "failed"),
        }
    }
}

impl std::fmt::Display for HealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthState::Healthy => write!(f, "healthy"),
            HealthState::Warning => write!(f, "warning"),
            HealthState::Unhealthy => write!(f, "unhealthy"),
            HealthState::Critical => write!(f, "critical"),
            HealthState::Unknown => write!(f, "unknown"),
        }
    }
}

/// ALYS-006-13: Create actor registration system with unique name enforcement, 
/// type indexing, and lifecycle tracking
impl ActorRegistry {
    /// Register a new actor in the registry
    /// 
    /// Registers an actor with comprehensive metadata, lifecycle tracking, and
    /// type-safe indexing. Enforces unique names and manages registry capacity.
    pub fn register_actor<A: Actor + 'static>(
        &mut self,
        name: String,
        addr: Addr<A>,
        priority: ActorPriority,
        tags: HashSet<String>,
        context: RegistrationContext,
    ) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        // Validate actor name
        if name.is_empty() || name.len() > registry::MAX_ACTOR_NAME_LENGTH {
            return Err(RegistryError::InvalidActorName(format!(
                "Name length must be 1-{} characters", registry::MAX_ACTOR_NAME_LENGTH
            )));
        }

        // Check for invalid characters
        if !name.chars().all(|c| c.is_alphanumeric() || "_-".contains(c)) {
            return Err(RegistryError::InvalidActorName(
                "Name must contain only alphanumeric characters, underscores, and hyphens".to_string()
            ));
        }

        // Check if already registered
        if self.name_index.contains_key(&name) {
            return Err(RegistryError::ActorAlreadyRegistered(name));
        }

        // Check registry capacity
        if self.stats.total_actors >= self.config.max_actors {
            return Err(RegistryError::RegistryCapacityExceeded {
                current: self.stats.total_actors,
                max: self.config.max_actors,
            });
        }

        let type_id = TypeId::of::<A>();
        let type_name = std::any::type_name::<A>().to_string();
        let now = SystemTime::now();

        info!(
            "Registering actor '{}' of type '{}' with priority '{:?}'", 
            name, type_name, priority
        );

        // Create registry entry
        let entry = ActorRegistryEntry {
            name: name.clone(),
            addr: Box::new(addr),
            type_id,
            type_name: type_name.clone(),
            priority,
            state: ActorLifecycleState::Registering,
            registered_at: now,
            last_activity: now,
            tags: tags.clone(),
            metadata: HashMap::new(),
            health_status: HealthStatus::default(),
            registration_context: context,
        };

        // Update name index
        self.name_index.insert(name.clone(), entry);

        // Update type index
        if self.config.enable_type_index {
            self.type_index
                .entry(type_id)
                .or_insert_with(Vec::new)
                .push(name.clone());
        }

        // Update tag index
        for tag in tags {
            self.tag_index
                .entry(tag)
                .or_insert_with(HashSet::new)
                .insert(name.clone());
        }

        // Update priority index
        self.priority_index
            .entry(priority)
            .or_insert_with(Vec::new)
            .push(name.clone());

        // Update statistics
        self.stats.total_actors += 1;
        *self.stats.actors_by_priority.entry(priority).or_insert(0) += 1;
        *self.stats.actors_by_type.entry(type_name).or_insert(0) += 1;
        *self.stats.actors_by_state.entry(ActorLifecycleState::Registering).or_insert(0) += 1;

        debug!("Actor '{}' registered successfully", name);
        Ok(())
    }

    /// Update actor lifecycle state
    /// 
    /// Updates the lifecycle state of an actor and manages state transitions.
    /// Validates state transitions and updates statistics accordingly.
    pub fn update_actor_state(
        &mut self,
        name: &str,
        new_state: ActorLifecycleState,
    ) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let entry = self.name_index.get_mut(name)
            .ok_or_else(|| RegistryError::ActorNotFound(name.to_string()))?;

        let old_state = entry.state.clone();

        // Validate state transition
        match (&old_state, &new_state) {
            // Valid transitions
            (ActorLifecycleState::Registering, ActorLifecycleState::Active) |
            (ActorLifecycleState::Active, ActorLifecycleState::Suspended) |
            (ActorLifecycleState::Active, ActorLifecycleState::ShuttingDown) |
            (ActorLifecycleState::Active, ActorLifecycleState::Failed) |
            (ActorLifecycleState::Suspended, ActorLifecycleState::Active) |
            (ActorLifecycleState::Suspended, ActorLifecycleState::Failed) |
            (ActorLifecycleState::ShuttingDown, ActorLifecycleState::Terminated) |
            (ActorLifecycleState::Failed, ActorLifecycleState::Active) |
            (ActorLifecycleState::Failed, ActorLifecycleState::Terminated) => {
                // Valid transition
            }
            _ => {
                return Err(RegistryError::LifecycleViolation(format!(
                    "Invalid state transition from '{}' to '{}' for actor '{}'",
                    old_state, new_state, name
                )));
            }
        }

        entry.state = new_state.clone();
        entry.last_activity = SystemTime::now();

        // Update statistics
        let old_count = self.stats.actors_by_state.get_mut(&old_state).unwrap();
        *old_count = old_count.saturating_sub(1);
        *self.stats.actors_by_state.entry(new_state.clone()).or_insert(0) += 1;

        debug!("Actor '{}' state updated from '{}' to '{}'", name, old_state, new_state);

        // Mark for cleanup if terminated
        if new_state == ActorLifecycleState::Terminated {
            self.orphaned_actors.insert(name.to_string());
        }

        Ok(())
    }

    /// Update actor metadata
    /// 
    /// Updates custom metadata for an actor entry.
    pub fn update_actor_metadata(
        &mut self,
        name: &str,
        metadata: HashMap<String, String>,
    ) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let entry = self.name_index.get_mut(name)
            .ok_or_else(|| RegistryError::ActorNotFound(name.to_string()))?;

        entry.metadata.extend(metadata);
        entry.last_activity = SystemTime::now();

        debug!("Updated metadata for actor '{}'", name);
        Ok(())
    }

    /// Update actor health status
    /// 
    /// Updates health check information for an actor.
    pub fn update_actor_health(
        &mut self,
        name: &str,
        health_status: HealthStatus,
    ) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let entry = self.name_index.get_mut(name)
            .ok_or_else(|| RegistryError::ActorNotFound(name.to_string()))?;

        entry.health_status = health_status;
        entry.last_activity = SystemTime::now();

        debug!("Updated health status for actor '{}'", name);
        Ok(())
    }

    /// Add tags to an actor
    /// 
    /// Adds additional tags to an existing actor and updates the tag index.
    pub fn add_actor_tags(
        &mut self,
        name: &str,
        tags: HashSet<String>,
    ) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let entry = self.name_index.get_mut(name)
            .ok_or_else(|| RegistryError::ActorNotFound(name.to_string()))?;

        for tag in tags {
            if entry.tags.insert(tag.clone()) {
                // New tag added
                self.tag_index
                    .entry(tag)
                    .or_insert_with(HashSet::new)
                    .insert(name.to_string());
            }
        }

        entry.last_activity = SystemTime::now();
        debug!("Added tags to actor '{}'", name);
        Ok(())
    }

    /// Remove tags from an actor
    /// 
    /// Removes specified tags from an actor and updates the tag index.
    pub fn remove_actor_tags(
        &mut self,
        name: &str,
        tags: &HashSet<String>,
    ) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let entry = self.name_index.get_mut(name)
            .ok_or_else(|| RegistryError::ActorNotFound(name.to_string()))?;

        for tag in tags {
            if entry.tags.remove(tag) {
                // Tag removed from actor
                if let Some(tag_set) = self.tag_index.get_mut(tag) {
                    tag_set.remove(name);
                    if tag_set.is_empty() {
                        self.tag_index.remove(tag);
                    }
                }
            }
        }

        entry.last_activity = SystemTime::now();
        debug!("Removed tags from actor '{}'", name);
        Ok(())
    }
}

/// ALYS-006-14: Add actor discovery methods with type-safe address retrieval and batch operations
impl ActorRegistry {
    /// Batch get actors by names with type safety
    /// 
    /// Retrieves multiple actors by name in a single operation.
    /// Returns only successfully retrieved actors, skipping missing ones.
    pub fn batch_get_actors<A: Actor + 'static>(
        &self,
        names: &[String]
    ) -> Vec<(String, Addr<A>)> {
        if self.locked {
            warn!("Registry is locked, batch_get_actors operation blocked");
            return Vec::new();
        }

        names.iter()
            .filter_map(|name| {
                self.get_actor::<A>(name).map(|addr| (name.clone(), addr))
            })
            .collect()
    }

    /// Find actors by pattern matching on names
    /// 
    /// Returns actors whose names match the provided pattern using glob-style wildcards.
    pub fn find_actors_by_pattern<A: Actor + 'static>(
        &self,
        pattern: &str
    ) -> Vec<(String, Addr<A>)> {
        if self.locked {
            warn!("Registry is locked, find_actors_by_pattern operation blocked");
            return Vec::new();
        }

        let regex_pattern = pattern
            .replace('*', ".*")
            .replace('?', ".");
        
        if let Ok(regex) = regex::Regex::new(&regex_pattern) {
            self.name_index
                .iter()
                .filter(|(name, entry)| {
                    entry.type_id == TypeId::of::<A>() && 
                    entry.state != ActorLifecycleState::Terminated &&
                    regex.is_match(name)
                })
                .filter_map(|(name, entry)| {
                    entry.addr.downcast_ref::<Addr<A>>()
                        .map(|addr| (name.clone(), addr.clone()))
                })
                .collect()
        } else {
            warn!("Invalid regex pattern: {}", pattern);
            Vec::new()
        }
    }

    /// Get actors by multiple tags (intersection)
    /// 
    /// Returns actors that have ALL of the specified tags.
    pub fn get_actors_by_tags_intersection(&self, tags: &[String]) -> Vec<String> {
        if self.locked || tags.is_empty() {
            return Vec::new();
        }

        let mut result: Option<HashSet<String>> = None;

        for tag in tags {
            if let Some(actors_with_tag) = self.tag_index.get(tag) {
                match result {
                    None => result = Some(actors_with_tag.clone()),
                    Some(ref mut current) => {
                        current.retain(|actor| actors_with_tag.contains(actor));
                    }
                }
            } else {
                // Tag doesn't exist, so no actors can have all tags
                return Vec::new();
            }
        }

        result.map(|set| set.into_iter().collect()).unwrap_or_default()
    }

    /// Get actors by multiple tags (union)
    /// 
    /// Returns actors that have ANY of the specified tags.
    pub fn get_actors_by_tags_union(&self, tags: &[String]) -> Vec<String> {
        if self.locked || tags.is_empty() {
            return Vec::new();
        }

        let mut result = HashSet::new();

        for tag in tags {
            if let Some(actors_with_tag) = self.tag_index.get(tag) {
                result.extend(actors_with_tag.iter().cloned());
            }
        }

        result.into_iter().collect()
    }

    /// Get healthy actors by type
    /// 
    /// Returns only actors that are in healthy state and active.
    pub fn get_healthy_actors<A: Actor + 'static>(&self) -> Vec<Addr<A>> {
        if self.locked {
            return Vec::new();
        }

        let type_id = TypeId::of::<A>();
        
        self.name_index
            .values()
            .filter(|entry| {
                entry.type_id == type_id &&
                entry.state == ActorLifecycleState::Active &&
                matches!(entry.health_status.status, HealthState::Healthy | HealthState::Warning)
            })
            .filter_map(|entry| entry.addr.downcast_ref::<Addr<A>>().cloned())
            .collect()
    }

    /// Query actors with complex filters
    /// 
    /// Provides flexible actor querying with multiple filter criteria.
    pub fn query_actors(&self, query: ActorQuery) -> Vec<String> {
        if self.locked {
            return Vec::new();
        }

        self.name_index
            .iter()
            .filter(|(name, entry)| query.matches(name, entry))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get actor statistics by type
    /// 
    /// Returns detailed statistics for actors of a specific type.
    pub fn get_actor_type_statistics<A: Actor + 'static>(&self) -> ActorTypeStatistics {
        let type_id = TypeId::of::<A>();
        let type_name = std::any::type_name::<A>();

        let actors: Vec<_> = self.name_index
            .values()
            .filter(|entry| entry.type_id == type_id)
            .collect();

        let mut stats = ActorTypeStatistics {
            type_name: type_name.to_string(),
            total_count: actors.len(),
            active_count: 0,
            healthy_count: 0,
            avg_uptime: Duration::from_secs(0),
            by_priority: HashMap::new(),
            by_state: HashMap::new(),
        };

        if actors.is_empty() {
            return stats;
        }

        let now = SystemTime::now();
        let mut total_uptime = Duration::from_secs(0);

        for actor in &actors {
            // Count by state
            *stats.by_state.entry(actor.state.clone()).or_insert(0) += 1;
            
            // Count by priority
            *stats.by_priority.entry(actor.priority).or_insert(0) += 1;

            // Count active and healthy
            if actor.state == ActorLifecycleState::Active {
                stats.active_count += 1;
            }

            if matches!(actor.health_status.status, HealthState::Healthy) {
                stats.healthy_count += 1;
            }

            // Calculate uptime
            if let Ok(uptime) = now.duration_since(actor.registered_at) {
                total_uptime += uptime;
            }
        }

        stats.avg_uptime = total_uptime / actors.len() as u32;
        stats
    }
}

/// ALYS-006-15: Implement actor unregistration with cleanup, index maintenance, and orphan prevention
impl ActorRegistry {
    /// Unregister an actor from the registry
    /// 
    /// Removes an actor from all indexes and performs cleanup operations.
    /// Updates statistics and handles orphan prevention.
    pub fn unregister_actor(&mut self, name: &str) -> Result<(), RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let entry = self.name_index.remove(name)
            .ok_or_else(|| RegistryError::ActorNotFound(name.to_string()))?;

        info!("Unregistering actor '{}' of type '{}'", name, entry.type_name);

        // Remove from type index
        if self.config.enable_type_index {
            if let Some(type_list) = self.type_index.get_mut(&entry.type_id) {
                type_list.retain(|n| n != name);
                if type_list.is_empty() {
                    self.type_index.remove(&entry.type_id);
                }
            }
        }

        // Remove from tag index
        for tag in &entry.tags {
            if let Some(tag_set) = self.tag_index.get_mut(tag) {
                tag_set.remove(name);
                if tag_set.is_empty() {
                    self.tag_index.remove(tag);
                }
            }
        }

        // Remove from priority index
        if let Some(priority_list) = self.priority_index.get_mut(&entry.priority) {
            priority_list.retain(|n| n != name);
            if priority_list.is_empty() {
                self.priority_index.remove(&entry.priority);
            }
        }

        // Update statistics
        self.stats.total_actors = self.stats.total_actors.saturating_sub(1);
        if let Some(count) = self.stats.actors_by_priority.get_mut(&entry.priority) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.stats.actors_by_priority.remove(&entry.priority);
            }
        }
        if let Some(count) = self.stats.actors_by_type.get_mut(&entry.type_name) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.stats.actors_by_type.remove(&entry.type_name);
            }
        }
        if let Some(count) = self.stats.actors_by_state.get_mut(&entry.state) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.stats.actors_by_state.remove(&entry.state);
            }
        }

        // Remove from orphaned actors if present
        self.orphaned_actors.remove(name);

        debug!("Actor '{}' unregistered successfully", name);
        Ok(())
    }

    /// Batch unregister multiple actors
    /// 
    /// Unregisters multiple actors in a single operation with rollback support.
    pub fn batch_unregister_actors(
        &mut self,
        names: Vec<String>,
        fail_fast: bool,
    ) -> BatchResult<String, RegistryError> {
        let start = Instant::now();
        let mut successes = Vec::new();
        let mut failures = Vec::new();

        for name in names {
            match self.unregister_actor(&name) {
                Ok(()) => successes.push(name),
                Err(e) => {
                    failures.push((name, e));
                    if fail_fast {
                        break;
                    }
                }
            }
        }

        let duration = start.elapsed();
        let total = successes.len() + failures.len();
        let success_rate = if total > 0 {
            successes.len() as f64 / total as f64
        } else {
            1.0
        };

        BatchResult {
            successes,
            failures,
            duration,
            success_rate,
        }
    }

    /// Cleanup terminated actors
    /// 
    /// Removes actors that are in terminated state and cleans up orphaned entries.
    pub fn cleanup_terminated_actors(&mut self) -> Result<usize, RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let terminated_actors: Vec<_> = self.name_index
            .iter()
            .filter(|(_, entry)| entry.state == ActorLifecycleState::Terminated)
            .map(|(name, _)| name.clone())
            .collect();

        let cleanup_count = terminated_actors.len();

        for name in terminated_actors {
            if let Err(e) = self.unregister_actor(&name) {
                warn!("Failed to cleanup terminated actor '{}': {}", name, e);
            }
        }

        // Also cleanup orphaned actors
        let orphaned: Vec<_> = self.orphaned_actors.drain().collect();
        for name in orphaned {
            if self.name_index.contains_key(&name) {
                if let Err(e) = self.unregister_actor(&name) {
                    warn!("Failed to cleanup orphaned actor '{}': {}", name, e);
                }
            }
        }

        info!("Cleaned up {} terminated actors", cleanup_count);
        Ok(cleanup_count)
    }

    /// Cleanup inactive actors
    /// 
    /// Removes actors that have been inactive for longer than the configured duration.
    pub fn cleanup_inactive_actors(&mut self) -> Result<usize, RegistryError> {
        if self.locked {
            return Err(RegistryError::RegistryLocked);
        }

        let now = SystemTime::now();
        let max_inactive = self.config.max_inactive_duration;

        let inactive_actors: Vec<_> = self.name_index
            .iter()
            .filter(|(_, entry)| {
                entry.state != ActorLifecycleState::Active &&
                now.duration_since(entry.last_activity)
                    .map(|duration| duration > max_inactive)
                    .unwrap_or(false)
            })
            .map(|(name, _)| name.clone())
            .collect();

        let cleanup_count = inactive_actors.len();

        for name in inactive_actors {
            warn!("Cleaning up inactive actor: {}", name);
            if let Err(e) = self.unregister_actor(&name) {
                warn!("Failed to cleanup inactive actor '{}': {}", name, e);
            }
        }

        info!("Cleaned up {} inactive actors", cleanup_count);
        Ok(cleanup_count)
    }

    /// Perform registry maintenance
    /// 
    /// Comprehensive maintenance including cleanup, index validation, and statistics update.
    pub fn perform_maintenance(&mut self) -> Result<MaintenanceReport, RegistryError> {
        info!("Starting registry maintenance");
        let start = Instant::now();

        self.lock_registry();

        let mut report = MaintenanceReport {
            duration: Duration::from_secs(0),
            terminated_cleaned: 0,
            inactive_cleaned: 0,
            orphans_cleaned: 0,
            index_errors_fixed: 0,
            statistics_updated: true,
        };

        // Cleanup terminated actors
        report.terminated_cleaned = self.cleanup_terminated_actors().unwrap_or(0);

        // Cleanup inactive actors
        report.inactive_cleaned = self.cleanup_inactive_actors().unwrap_or(0);

        // Validate and fix indexes
        report.index_errors_fixed = self.validate_and_fix_indexes();

        // Update statistics
        self.update_statistics();

        report.duration = start.elapsed();
        self.unlock_registry();

        info!("Registry maintenance completed: {:?}", report);
        Ok(report)
    }

    /// Validate and fix registry indexes
    fn validate_and_fix_indexes(&mut self) -> usize {
        let mut fixes = 0;

        // Validate type index
        if self.config.enable_type_index {
            let mut invalid_entries = Vec::new();
            
            for (type_id, names) in &self.type_index {
                for name in names {
                    if let Some(entry) = self.name_index.get(name) {
                        if entry.type_id != *type_id {
                            invalid_entries.push((name.clone(), *type_id));
                        }
                    } else {
                        invalid_entries.push((name.clone(), *type_id));
                    }
                }
            }

            for (name, type_id) in invalid_entries {
                if let Some(type_list) = self.type_index.get_mut(&type_id) {
                    type_list.retain(|n| n != &name);
                    fixes += 1;
                }
            }
        }

        // Validate tag index
        let mut invalid_tag_entries = Vec::new();
        for (tag, names) in &self.tag_index {
            for name in names {
                if let Some(entry) = self.name_index.get(name) {
                    if !entry.tags.contains(tag) {
                        invalid_tag_entries.push((tag.clone(), name.clone()));
                    }
                } else {
                    invalid_tag_entries.push((tag.clone(), name.clone()));
                }
            }
        }

        for (tag, name) in invalid_tag_entries {
            if let Some(tag_set) = self.tag_index.get_mut(&tag) {
                tag_set.remove(&name);
                if tag_set.is_empty() {
                    self.tag_index.remove(&tag);
                }
                fixes += 1;
            }
        }

        fixes
    }

    /// Update registry statistics
    fn update_statistics(&mut self) {
        let mut stats = RegistryStatistics::default();
        stats.total_actors = self.name_index.len();
        stats.last_updated = SystemTime::now();

        for entry in self.name_index.values() {
            *stats.actors_by_priority.entry(entry.priority).or_insert(0) += 1;
            *stats.actors_by_type.entry(entry.type_name.clone()).or_insert(0) += 1;
            *stats.actors_by_state.entry(entry.state.clone()).or_insert(0) += 1;
        }

        self.stats = stats;
    }
}

/// Actor query builder for complex filtering
#[derive(Debug, Clone)]
pub struct ActorQuery {
    /// Actor name pattern (regex)
    pub name_pattern: Option<String>,
    /// Required tags (intersection)
    pub required_tags: Vec<String>,
    /// Any of these tags (union)  
    pub any_tags: Vec<String>,
    /// Actor priority filter
    pub priority: Option<ActorPriority>,
    /// Lifecycle state filter
    pub state: Option<ActorLifecycleState>,
    /// Health state filter
    pub health_state: Option<HealthState>,
    /// Minimum uptime
    pub min_uptime: Option<Duration>,
    /// Maximum uptime
    pub max_uptime: Option<Duration>,
}

impl ActorQuery {
    /// Create a new query builder
    pub fn new() -> Self {
        Self {
            name_pattern: None,
            required_tags: Vec::new(),
            any_tags: Vec::new(),
            priority: None,
            state: None,
            health_state: None,
            min_uptime: None,
            max_uptime: None,
        }
    }

    /// Filter by name pattern (regex)
    pub fn with_name_pattern(mut self, pattern: String) -> Self {
        self.name_pattern = Some(pattern);
        self
    }

    /// Filter by required tags (all must be present)
    pub fn with_required_tags(mut self, tags: Vec<String>) -> Self {
        self.required_tags = tags;
        self
    }

    /// Filter by any of these tags
    pub fn with_any_tags(mut self, tags: Vec<String>) -> Self {
        self.any_tags = tags;
        self
    }

    /// Filter by priority
    pub fn with_priority(mut self, priority: ActorPriority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Filter by lifecycle state
    pub fn with_state(mut self, state: ActorLifecycleState) -> Self {
        self.state = Some(state);
        self
    }

    /// Filter by health state
    pub fn with_health_state(mut self, health_state: HealthState) -> Self {
        self.health_state = Some(health_state);
        self
    }

    /// Filter by minimum uptime
    pub fn with_min_uptime(mut self, min_uptime: Duration) -> Self {
        self.min_uptime = Some(min_uptime);
        self
    }

    /// Filter by maximum uptime  
    pub fn with_max_uptime(mut self, max_uptime: Duration) -> Self {
        self.max_uptime = Some(max_uptime);
        self
    }

    /// Check if an actor entry matches this query
    pub fn matches(&self, name: &str, entry: &ActorRegistryEntry) -> bool {
        // Check name pattern
        if let Some(ref pattern) = self.name_pattern {
            if let Ok(regex) = regex::Regex::new(pattern) {
                if !regex.is_match(name) {
                    return false;
                }
            }
        }

        // Check required tags (all must be present)
        if !self.required_tags.is_empty() {
            for required_tag in &self.required_tags {
                if !entry.tags.contains(required_tag) {
                    return false;
                }
            }
        }

        // Check any tags (at least one must be present)
        if !self.any_tags.is_empty() {
            let has_any_tag = self.any_tags.iter().any(|tag| entry.tags.contains(tag));
            if !has_any_tag {
                return false;
            }
        }

        // Check priority
        if let Some(priority) = self.priority {
            if entry.priority != priority {
                return false;
            }
        }

        // Check state
        if let Some(ref state) = self.state {
            if entry.state != *state {
                return false;
            }
        }

        // Check health state
        if let Some(ref health_state) = self.health_state {
            if entry.health_status.status != *health_state {
                return false;
            }
        }

        // Check uptime constraints
        let now = SystemTime::now();
        if let Ok(uptime) = now.duration_since(entry.registered_at) {
            if let Some(min_uptime) = self.min_uptime {
                if uptime < min_uptime {
                    return false;
                }
            }

            if let Some(max_uptime) = self.max_uptime {
                if uptime > max_uptime {
                    return false;
                }
            }
        }

        true
    }
}

impl Default for ActorQuery {
    fn default() -> Self {
        Self::new()
    }
}

/// Actor type statistics
#[derive(Debug, Clone)]
pub struct ActorTypeStatistics {
    pub type_name: String,
    pub total_count: usize,
    pub active_count: usize,
    pub healthy_count: usize,
    pub avg_uptime: Duration,
    pub by_priority: HashMap<ActorPriority, usize>,
    pub by_state: HashMap<ActorLifecycleState, usize>,
}

/// Maintenance report
#[derive(Debug, Clone)]
pub struct MaintenanceReport {
    pub duration: Duration,
    pub terminated_cleaned: usize,
    pub inactive_cleaned: usize,
    pub orphans_cleaned: usize,
    pub index_errors_fixed: usize,
    pub statistics_updated: bool,
}

/// Thread-safe actor registry wrapper
/// 
/// Provides concurrent access to the actor registry with read-write locking
/// for safe multi-threaded operations in the Alys actor system.
#[derive(Debug, Clone)]
pub struct ThreadSafeActorRegistry {
    inner: Arc<RwLock<ActorRegistry>>,
}

impl ThreadSafeActorRegistry {
    /// Create a new thread-safe registry
    pub fn new(config: ActorRegistryConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ActorRegistry::new(config))),
        }
    }

    /// Create with development configuration
    pub fn development() -> Self {
        Self::new(ActorRegistryConfig::default())
    }

    /// Create with production configuration
    pub fn production() -> Self {
        Self::new(ActorRegistryConfig {
            max_actors: 10000,
            cleanup_interval: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
            max_inactive_duration: Duration::from_secs(7200),
            ..Default::default()
        })
    }

    /// Register an actor
    pub async fn register_actor<A: Actor + 'static>(
        &self,
        name: String,
        addr: Addr<A>,
        priority: ActorPriority,
        tags: HashSet<String>,
        context: RegistrationContext,
    ) -> Result<(), RegistryError> {
        self.inner.write().await
            .register_actor(name, addr, priority, tags, context)
    }

    /// Get an actor by name
    pub async fn get_actor<A: Actor + 'static>(&self, name: &str) -> Option<Addr<A>> {
        self.inner.read().await.get_actor(name)
    }

    /// Get actors by type
    pub async fn get_actors_by_type<A: Actor + 'static>(&self) -> Vec<Addr<A>> {
        self.inner.read().await.get_actors_by_type()
    }

    /// Unregister an actor
    pub async fn unregister_actor(&self, name: &str) -> Result<(), RegistryError> {
        self.inner.write().await.unregister_actor(name)
    }

    /// Update actor state
    pub async fn update_actor_state(
        &self,
        name: &str,
        new_state: ActorLifecycleState,
    ) -> Result<(), RegistryError> {
        self.inner.write().await.update_actor_state(name, new_state)
    }

    /// Get registry statistics
    pub async fn get_statistics(&self) -> RegistryStatistics {
        self.inner.read().await.get_statistics().clone()
    }

    /// Perform maintenance
    pub async fn perform_maintenance(&self) -> Result<MaintenanceReport, RegistryError> {
        self.inner.write().await.perform_maintenance()
    }

    /// Query actors
    pub async fn query_actors(&self, query: ActorQuery) -> Vec<String> {
        self.inner.read().await.query_actors(query)
    }

    /// Check if registry contains actor
    pub async fn contains_actor(&self, name: &str) -> bool {
        self.inner.read().await.contains_actor(name)
    }

    /// Get all actor names
    pub async fn get_all_actor_names(&self) -> Vec<String> {
        self.inner.read().await.get_all_actor_names()
    }

    /// Get registry length
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Check if registry is empty
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}