//! Core actor definitions and traits

use crate::{
    error::{ActorError, ActorResult},
    lifecycle::{LifecycleAware, LifecycleConfig, ActorState},
    mailbox::{EnhancedMailbox, MailboxConfig},
    message::{AlysMessage, MessageEnvelope},
    metrics::ActorMetrics,
    supervisor::{SupervisionPolicy, SupervisorMessage},
};
use actix::{Actor, Addr, Context, Handler, Message, Recipient, ResponseFuture};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tracing::{debug, error, info, warn};

/// Core trait for Alys actors with standardized interface
#[async_trait]
pub trait AlysActor: Actor + LifecycleAware + Send + Sync + 'static {
    /// Configuration type for this actor
    type Config: Clone + Send + Sync + 'static;
    
    /// Error type for this actor (unified with ActorError)
    type Error: Into<ActorError> + std::error::Error + Send + Sync + 'static;
    
    /// Message types this actor can handle
    type Message: AlysMessage + 'static;
    
    /// State type for this actor
    type State: Clone + Send + Sync + 'static;
    
    /// Create new actor instance with configuration
    fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;
    
    /// Get actor configuration
    fn config(&self) -> &Self::Config;
    
    /// Get mutable actor configuration
    fn config_mut(&mut self) -> &mut Self::Config;
    
    /// Get actor metrics
    fn metrics(&self) -> &ActorMetrics;
    
    /// Get mutable actor metrics
    fn metrics_mut(&mut self) -> &mut ActorMetrics;
    
    /// Get current actor state
    async fn get_state(&self) -> Self::State;
    
    /// Set actor state
    async fn set_state(&mut self, state: Self::State) -> ActorResult<()>;
    
    /// Get actor mailbox configuration
    fn mailbox_config(&self) -> MailboxConfig {
        MailboxConfig::default()
    }
    
    /// Get supervision policy for this actor
    fn supervision_policy(&self) -> SupervisionPolicy {
        SupervisionPolicy::default()
    }
    
    /// Get actor dependencies (other actors this actor depends on)
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }
    
    /// Handle configuration update
    async fn on_config_update(&mut self, new_config: Self::Config) -> ActorResult<()> {
        *self.config_mut() = new_config;
        Ok(())
    }
    
    /// Handle supervisor message
    async fn handle_supervisor_message(&mut self, msg: SupervisorMessage) -> ActorResult<()> {
        match msg {
            SupervisorMessage::HealthCheck => {
                let healthy = self.health_check().await.map_err(|e| e.into())?;
                if !healthy {
                    warn!(actor_type = self.actor_type(), "Actor health check failed");
                }
                Ok(())
            }
            SupervisorMessage::Shutdown { timeout } => {
                info!(actor_type = self.actor_type(), "Received shutdown signal");
                self.on_shutdown(timeout).await
            }
            _ => Ok(()),
        }
    }
    
    /// Pre-process message before handling
    async fn pre_process_message(&mut self, _envelope: &MessageEnvelope<Self::Message>) -> ActorResult<()> {
        Ok(())
    }
    
    /// Post-process message after handling
    async fn post_process_message(&mut self, _envelope: &MessageEnvelope<Self::Message>, _result: &<Self::Message as Message>::Result) -> ActorResult<()> {
        Ok(())
    }
    
    /// Handle message processing error
    async fn handle_message_error(&mut self, _envelope: &MessageEnvelope<Self::Message>, error: &ActorError) -> ActorResult<()> {
        self.metrics_mut().record_message_failed(&error.to_string());
        error!(
            actor_type = self.actor_type(),
            error = %error,
            "Message processing failed"
        );
        Ok(())
    }
}

/// Extended actor trait with additional capabilities
#[async_trait]
pub trait ExtendedAlysActor: AlysActor {
    /// Custom initialization logic
    async fn custom_initialize(&mut self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Handle critical errors that may require restart
    async fn handle_critical_error(&mut self, error: ActorError) -> ActorResult<bool> {
        error!(
            actor_type = self.actor_type(),
            error = %error,
            "Critical error occurred"
        );
        // Return true to request restart, false to continue
        Ok(error.severity().is_critical())
    }
    
    /// Perform periodic maintenance tasks
    async fn maintenance_task(&mut self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Export custom metrics
    async fn export_metrics(&self) -> ActorResult<serde_json::Value> {
        let snapshot = self.metrics().snapshot();
        Ok(serde_json::to_value(snapshot).unwrap_or_default())
    }
    
    /// Handle resource cleanup on restart
    async fn cleanup_resources(&mut self) -> ActorResult<()> {
        Ok(())
    }
}

/// Actor registry for managing actor addresses and metadata
#[derive(Debug)]
pub struct ActorRegistry {
    /// Registered actors with their addresses
    actors: std::collections::HashMap<String, ActorRegistration>,
    /// Actor dependencies graph
    dependencies: std::collections::HashMap<String, Vec<String>>,
}

/// Actor registration information
#[derive(Debug)]
pub struct ActorRegistration {
    /// Actor unique identifier
    pub id: String,
    /// Actor type name
    pub actor_type: String,
    /// Actor address (type-erased)
    pub addr: Box<dyn std::any::Any + Send>,
    /// Actor metrics
    pub metrics: Arc<ActorMetrics>,
    /// Registration timestamp
    pub registered_at: SystemTime,
    /// Last health check result
    pub last_health_check: Option<(SystemTime, bool)>,
    /// Actor dependencies
    pub dependencies: Vec<String>,
}

impl ActorRegistry {
    /// Create new actor registry
    pub fn new() -> Self {
        Self {
            actors: std::collections::HashMap::new(),
            dependencies: std::collections::HashMap::new(),
        }
    }
    
    /// Register actor with the registry
    pub fn register<A>(&mut self, 
        id: String, 
        addr: Addr<A>, 
        metrics: Arc<ActorMetrics>
    ) -> ActorResult<()>
    where
        A: AlysActor + 'static,
    {
        let actor_type = std::any::type_name::<A>().to_string();
        
        let registration = ActorRegistration {
            id: id.clone(),
            actor_type,
            addr: Box::new(addr),
            metrics,
            registered_at: SystemTime::now(),
            last_health_check: None,
            dependencies: Vec::new(),
        };
        
        self.actors.insert(id.clone(), registration);
        info!(actor_id = %id, "Actor registered");
        
        Ok(())
    }
    
    /// Unregister actor from the registry
    pub fn unregister(&mut self, id: &str) -> ActorResult<()> {
        if self.actors.remove(id).is_some() {
            self.dependencies.remove(id);
            // Remove from other actors' dependencies
            for deps in self.dependencies.values_mut() {
                deps.retain(|dep| dep != id);
            }
            info!(actor_id = %id, "Actor unregistered");
        }
        Ok(())
    }
    
    /// Get actor registration
    pub fn get(&self, id: &str) -> Option<&ActorRegistration> {
        self.actors.get(id)
    }
    
    /// Get all registered actors
    pub fn all_actors(&self) -> &std::collections::HashMap<String, ActorRegistration> {
        &self.actors
    }
    
    /// Add dependency between actors
    pub fn add_dependency(&mut self, actor_id: String, depends_on: String) -> ActorResult<()> {
        if !self.actors.contains_key(&actor_id) {
            return Err(ActorError::ActorNotFound { name: actor_id });
        }
        if !self.actors.contains_key(&depends_on) {
            return Err(ActorError::ActorNotFound { name: depends_on });
        }
        
        self.dependencies
            .entry(actor_id.clone())
            .or_insert_with(Vec::new)
            .push(depends_on);
            
        Ok(())
    }
    
    /// Get dependencies for an actor
    pub fn get_dependencies(&self, actor_id: &str) -> Vec<String> {
        self.dependencies.get(actor_id).cloned().unwrap_or_default()
    }
    
    /// Check for circular dependencies
    pub fn has_circular_dependency(&self) -> bool {
        // Simplified circular dependency detection using DFS
        for actor_id in self.actors.keys() {
            if self.has_circular_dependency_from(actor_id, actor_id, &mut std::collections::HashSet::new()) {
                return true;
            }
        }
        false
    }
    
    fn has_circular_dependency_from(&self, start: &str, current: &str, visited: &mut std::collections::HashSet<String>) -> bool {
        if visited.contains(current) {
            return current == start;
        }
        
        visited.insert(current.to_string());
        
        if let Some(deps) = self.dependencies.get(current) {
            for dep in deps {
                if self.has_circular_dependency_from(start, dep, visited) {
                    return true;
                }
            }
        }
        
        visited.remove(current);
        false
    }
    
    /// Get actor startup order based on dependencies
    pub fn get_startup_order(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        
        for actor_id in self.actors.keys() {
            self.topological_sort(actor_id, &mut visited, &mut result);
        }
        
        result
    }
    
    fn topological_sort(&self, actor_id: &str, visited: &mut std::collections::HashSet<String>, result: &mut Vec<String>) {
        if visited.contains(actor_id) {
            return;
        }
        
        visited.insert(actor_id.to_string());
        
        // Visit dependencies first
        if let Some(deps) = self.dependencies.get(actor_id) {
            for dep in deps {
                self.topological_sort(dep, visited, result);
            }
        }
        
        result.push(actor_id.to_string());
    }
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Actor factory for creating and configuring actors
pub struct ActorFactory;

impl ActorFactory {
    /// Create and start actor with default configuration
    pub async fn create_actor<A>(id: String) -> ActorResult<Addr<A>>
    where
        A: AlysActor + 'static,
        A::Config: Default,
    {
        Self::create_actor_with_config(id, A::Config::default()).await
    }
    
    /// Create and start actor with specific configuration
    pub async fn create_actor_with_config<A>(id: String, config: A::Config) -> ActorResult<Addr<A>>
    where
        A: AlysActor + 'static,
    {
        let actor = A::new(config).map_err(|e| e.into())?;
        let addr = actor.start();
        
        debug!(actor_id = %id, actor_type = %std::any::type_name::<A>(), "Actor created and started");
        
        Ok(addr)
    }
    
    /// Create supervised actor
    pub async fn create_supervised_actor<A>(
        id: String,
        config: A::Config,
        supervisor: Recipient<SupervisorMessage>,
    ) -> ActorResult<Addr<A>>
    where
        A: AlysActor + 'static,
    {
        let addr = Self::create_actor_with_config(id.clone(), config).await?;
        
        // Register with supervisor
        let supervisor_msg = SupervisorMessage::AddChild {
            child_id: id,
            actor_type: std::any::type_name::<A>().to_string(),
            policy: None,
        };
        
        supervisor.try_send(supervisor_msg)
            .map_err(|_| ActorError::MessageDeliveryFailed {
                from: "factory".to_string(),
                to: "supervisor".to_string(),
                reason: "Failed to register with supervisor".to_string(),
            })?;
        
        Ok(addr)
    }
}

// MessageEnvelope is defined in message.rs to avoid conflicts

/// Base actor implementation
pub struct BaseActor {
    /// Actor ID
    pub id: String,
    /// Actor metrics
    pub metrics: ActorMetrics,
    /// Actor start time
    pub start_time: SystemTime,
}

impl BaseActor {
    /// Create new base actor
    pub fn new(id: String) -> Self {
        Self {
            id,
            metrics: ActorMetrics::default(),
            start_time: SystemTime::now(),
        }
    }
}

impl Actor for BaseActor {
    type Context = Context<Self>;
    
    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(actor_id = %self.id, "Actor started");
        self.start_time = SystemTime::now();
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(actor_id = %self.id, "Actor stopped");
    }
}

/// Health check message
#[derive(Debug, Clone)]
pub struct HealthCheck;

impl Message for HealthCheck {
    type Result = ActorResult<bool>;
}

/// Shutdown message
#[derive(Debug, Clone)]
pub struct Shutdown {
    /// Graceful shutdown timeout
    pub timeout: Option<Duration>,
}

impl Message for Shutdown {
    type Result = ActorResult<()>;
}

/// Configuration update message
#[derive(Debug, Clone)]
pub struct ConfigUpdate<T> {
    /// New configuration
    pub config: T,
}

impl<T> Message for ConfigUpdate<T>
where
    T: Clone + Send + 'static,
{
    type Result = ActorResult<()>;
}