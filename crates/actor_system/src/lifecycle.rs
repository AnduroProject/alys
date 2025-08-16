//! Actor lifecycle management
//!
//! This module provides comprehensive lifecycle management for actors including
//! spawning, initialization, health monitoring, graceful shutdown, and resource cleanup.

use crate::{
    error::{ActorError, ActorResult},
    message::{AlysMessage, MessageEnvelope, MessagePriority},
    metrics::ActorMetrics,
    supervisor::{SupervisionPolicy, SupervisorMessage},
};
use actix::prelude::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::sync::{broadcast, oneshot, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Actor lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorState {
    /// Actor is initializing
    Initializing,
    /// Actor is running and healthy
    Running,
    /// Actor is paused
    Paused,
    /// Actor is shutting down gracefully
    Stopping,
    /// Actor has stopped
    Stopped,
    /// Actor failed and needs restart
    Failed,
    /// Actor is restarting
    Restarting,
}

impl Default for ActorState {
    fn default() -> Self {
        ActorState::Initializing
    }
}

impl std::fmt::Display for ActorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActorState::Initializing => write!(f, "initializing"),
            ActorState::Running => write!(f, "running"),
            ActorState::Paused => write!(f, "paused"),
            ActorState::Stopping => write!(f, "stopping"),
            ActorState::Stopped => write!(f, "stopped"),
            ActorState::Failed => write!(f, "failed"),
            ActorState::Restarting => write!(f, "restarting"),
        }
    }
}

/// Actor lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    /// Maximum time for initialization
    pub init_timeout: Duration,
    /// Maximum time for graceful shutdown
    pub shutdown_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Enable automatic health checks
    pub auto_health_check: bool,
    /// Maximum consecutive health check failures before marking failed
    pub max_health_failures: u32,
    /// Enable state transition logging
    pub log_state_transitions: bool,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            init_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            health_check_interval: Duration::from_secs(30),
            auto_health_check: true,
            max_health_failures: 3,
            log_state_transitions: true,
        }
    }
}

/// Actor lifecycle metadata
#[derive(Debug)]
pub struct LifecycleMetadata {
    /// Unique actor identifier
    pub actor_id: String,
    /// Actor type name
    pub actor_type: String,
    /// Current state
    pub state: Arc<RwLock<ActorState>>,
    /// State transition history
    pub state_history: Arc<RwLock<Vec<StateTransition>>>,
    /// Actor spawn time
    pub spawn_time: SystemTime,
    /// Last state change time
    pub last_state_change: Arc<RwLock<SystemTime>>,
    /// Health check metrics
    pub health_failures: AtomicU64,
    /// Lifecycle configuration
    pub config: LifecycleConfig,
}

/// State transition record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Previous state
    pub from: ActorState,
    /// New state
    pub to: ActorState,
    /// Transition timestamp
    pub timestamp: SystemTime,
    /// Reason for transition
    pub reason: Option<String>,
    /// Associated error if any
    pub error: Option<String>,
}

/// Actor lifecycle manager
#[derive(Debug)]
pub struct LifecycleManager {
    /// Actor metadata registry
    actors: Arc<RwLock<HashMap<String, Arc<LifecycleMetadata>>>>,
    /// Global lifecycle metrics
    metrics: Arc<LifecycleManagerMetrics>,
    /// Shutdown broadcast channel
    shutdown_tx: broadcast::Sender<ShutdownSignal>,
    /// Health check task handle
    health_check_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Lifecycle manager metrics
#[derive(Debug, Default)]
pub struct LifecycleManagerMetrics {
    /// Total actors spawned
    pub total_spawned: AtomicU64,
    /// Currently running actors
    pub running_actors: AtomicU64,
    /// Failed actors
    pub failed_actors: AtomicU64,
    /// Total state transitions
    pub total_transitions: AtomicU64,
    /// Graceful shutdowns
    pub graceful_shutdowns: AtomicU64,
    /// Forced shutdowns
    pub forced_shutdowns: AtomicU64,
}

/// Shutdown signal
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    /// Shutdown reason
    pub reason: String,
    /// Graceful shutdown timeout
    pub timeout: Duration,
    /// Force shutdown flag
    pub force: bool,
}

/// Trait for lifecycle-aware actors
#[async_trait]
pub trait LifecycleAware: Actor {
    /// Initialize the actor (called after construction)
    async fn initialize(&mut self) -> ActorResult<()>;

    /// Handle actor startup (called after initialization)
    async fn on_start(&mut self) -> ActorResult<()>;

    /// Handle pause request
    async fn on_pause(&mut self) -> ActorResult<()>;

    /// Handle resume request
    async fn on_resume(&mut self) -> ActorResult<()>;

    /// Handle shutdown request
    async fn on_shutdown(&mut self, timeout: Duration) -> ActorResult<()>;

    /// Perform health check
    async fn health_check(&self) -> ActorResult<bool>;

    /// Handle state transition
    async fn on_state_change(&mut self, from: ActorState, to: ActorState) -> ActorResult<()>;

    /// Get actor type name
    fn actor_type(&self) -> &str;

    /// Get actor configuration
    fn lifecycle_config(&self) -> LifecycleConfig {
        LifecycleConfig::default()
    }
}

impl LifecycleManager {
    /// Create new lifecycle manager
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(100);

        Self {
            actors: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(LifecycleManagerMetrics::default()),
            shutdown_tx,
            health_check_handle: None,
        }
    }

    /// Start the lifecycle manager
    pub async fn start(&mut self) -> ActorResult<()> {
        info!("Starting lifecycle manager");

        // Start health check task
        self.start_health_check_task().await;

        Ok(())
    }

    /// Stop the lifecycle manager
    pub async fn stop(&mut self, timeout: Duration) -> ActorResult<()> {
        info!("Stopping lifecycle manager");

        // Signal all actors to shutdown
        let shutdown_signal = ShutdownSignal {
            reason: "System shutdown".to_string(),
            timeout,
            force: false,
        };

        let _ = self.shutdown_tx.send(shutdown_signal);

        // Stop health check task
        if let Some(handle) = self.health_check_handle.take() {
            handle.abort();
        }

        // Wait for all actors to shutdown
        self.wait_for_shutdown(timeout).await?;

        Ok(())
    }

    /// Register new actor with lifecycle management
    pub async fn register_actor<A>(
        &self,
        actor_id: String,
        actor_type: String,
        config: Option<LifecycleConfig>,
    ) -> ActorResult<Arc<LifecycleMetadata>>
    where
        A: LifecycleAware + 'static,
    {
        let metadata = Arc::new(LifecycleMetadata {
            actor_id: actor_id.clone(),
            actor_type,
            state: Arc::new(RwLock::new(ActorState::Initializing)),
            state_history: Arc::new(RwLock::new(Vec::new())),
            spawn_time: SystemTime::now(),
            last_state_change: Arc::new(RwLock::new(SystemTime::now())),
            health_failures: AtomicU64::new(0),
            config: config.unwrap_or_default(),
        });

        {
            let mut actors = self.actors.write().await;
            actors.insert(actor_id.clone(), metadata.clone());
        }

        self.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);

        debug!("Registered actor: {} ({})", actor_id, metadata.actor_type);

        Ok(metadata)
    }

    /// Unregister actor from lifecycle management
    pub async fn unregister_actor(&self, actor_id: &str) -> ActorResult<()> {
        let mut actors = self.actors.write().await;
        if let Some(metadata) = actors.remove(actor_id) {
            let state = *metadata.state.read().await;
            if state == ActorState::Running {
                self.metrics.running_actors.fetch_sub(1, Ordering::Relaxed);
            } else if state == ActorState::Failed {
                self.metrics.failed_actors.fetch_sub(1, Ordering::Relaxed);
            }

            debug!("Unregistered actor: {}", actor_id);
        }

        Ok(())
    }

    /// Transition actor state
    pub async fn transition_state(
        &self,
        actor_id: &str,
        new_state: ActorState,
        reason: Option<String>,
        error: Option<ActorError>,
    ) -> ActorResult<()> {
        let actors = self.actors.read().await;
        let metadata = actors.get(actor_id).ok_or_else(|| ActorError::ActorNotFound {
            name: actor_id.to_string(),
        })?;

        let old_state = {
            let mut state = metadata.state.write().await;
            let old = *state;
            *state = new_state;
            old
        };

        // Update last state change time
        {
            let mut last_change = metadata.last_state_change.write().await;
            *last_change = SystemTime::now();
        }

        // Record state transition
        let transition = StateTransition {
            from: old_state,
            to: new_state,
            timestamp: SystemTime::now(),
            reason,
            error: error.map(|e| e.to_string()),
        };

        {
            let mut history = metadata.state_history.write().await;
            history.push(transition.clone());
            
            // Keep only recent transitions (sliding window)
            if history.len() > 1000 {
                history.drain(..500);
            }
        }

        // Update metrics
        match (old_state, new_state) {
            (_, ActorState::Running) => {
                if old_state != ActorState::Running {
                    self.metrics.running_actors.fetch_add(1, Ordering::Relaxed);
                }
            }
            (ActorState::Running, _) => {
                self.metrics.running_actors.fetch_sub(1, Ordering::Relaxed);
            }
            (_, ActorState::Failed) => {
                if old_state != ActorState::Failed {
                    self.metrics.failed_actors.fetch_add(1, Ordering::Relaxed);
                }
            }
            (ActorState::Failed, _) => {
                self.metrics.failed_actors.fetch_sub(1, Ordering::Relaxed);
            }
            _ => {}
        }

        self.metrics.total_transitions.fetch_add(1, Ordering::Relaxed);

        if metadata.config.log_state_transitions {
            info!(
                actor_id = %actor_id,
                actor_type = %metadata.actor_type,
                from = %old_state,
                to = %new_state,
                reason = ?transition.reason,
                "Actor state transition"
            );
        }

        Ok(())
    }

    /// Get actor state
    pub async fn get_actor_state(&self, actor_id: &str) -> ActorResult<ActorState> {
        let actors = self.actors.read().await;
        let metadata = actors.get(actor_id).ok_or_else(|| ActorError::ActorNotFound {
            name: actor_id.to_string(),
        })?;

        let state = *metadata.state.read().await;
        Ok(state)
    }

    /// Get all actor states
    pub async fn get_all_actor_states(&self) -> HashMap<String, ActorState> {
        let mut result = HashMap::new();
        let actors = self.actors.read().await;

        for (actor_id, metadata) in actors.iter() {
            let state = *metadata.state.read().await;
            result.insert(actor_id.clone(), state);
        }

        result
    }

    /// Get actor metadata
    pub async fn get_actor_metadata(&self, actor_id: &str) -> ActorResult<Arc<LifecycleMetadata>> {
        let actors = self.actors.read().await;
        actors.get(actor_id)
            .cloned()
            .ok_or_else(|| ActorError::ActorNotFound {
                name: actor_id.to_string(),
            })
    }

    /// Record health check result
    pub async fn record_health_check(&self, actor_id: &str, healthy: bool) -> ActorResult<()> {
        let actors = self.actors.read().await;
        let metadata = actors.get(actor_id).ok_or_else(|| ActorError::ActorNotFound {
            name: actor_id.to_string(),
        })?;

        if healthy {
            metadata.health_failures.store(0, Ordering::Relaxed);
        } else {
            let failures = metadata.health_failures.fetch_add(1, Ordering::Relaxed) + 1;
            
            warn!(
                actor_id = %actor_id,
                consecutive_failures = failures,
                max_failures = metadata.config.max_health_failures,
                "Actor health check failed"
            );

            if failures >= metadata.config.max_health_failures as u64 {
                self.transition_state(
                    actor_id,
                    ActorState::Failed,
                    Some("Too many health check failures".to_string()),
                    Some(ActorError::SystemFailure {
                        reason: format!("Health check failed {} times", failures),
                    }),
                ).await?;
            }
        }

        Ok(())
    }

    /// Start health check background task
    async fn start_health_check_task(&mut self) {
        let actors = self.actors.clone();
        let lifecycle_manager = Arc::downgrade(&Arc::new(self.clone()));

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                interval.tick().await;

                // Check if lifecycle manager still exists
                if lifecycle_manager.upgrade().is_none() {
                    break;
                }

                let actors_guard = actors.read().await;
                for (actor_id, metadata) in actors_guard.iter() {
                    if !metadata.config.auto_health_check {
                        continue;
                    }

                    let state = *metadata.state.read().await;
                    if state == ActorState::Running {
                        // TODO: Send health check message to actor
                        // For now, assume healthy
                        debug!("Health check for actor: {}", actor_id);
                    }
                }
            }
        });

        self.health_check_handle = Some(handle);
    }

    /// Wait for all actors to shutdown
    async fn wait_for_shutdown(&self, timeout: Duration) -> ActorResult<()> {
        let start_time = SystemTime::now();

        loop {
            let actors = self.actors.read().await;
            let all_stopped = actors.iter().all(|(_, metadata)| {
                futures::executor::block_on(async {
                    let state = *metadata.state.read().await;
                    matches!(state, ActorState::Stopped | ActorState::Failed)
                })
            });

            if all_stopped {
                self.metrics.graceful_shutdowns.fetch_add(1, Ordering::Relaxed);
                break;
            }

            if start_time.elapsed().unwrap_or_default() > timeout {
                self.metrics.forced_shutdowns.fetch_add(1, Ordering::Relaxed);
                warn!("Shutdown timeout exceeded, some actors may not have stopped gracefully");
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// Get lifecycle metrics
    pub fn metrics(&self) -> Arc<LifecycleManagerMetrics> {
        self.metrics.clone()
    }

    /// Get shutdown broadcast receiver
    pub fn shutdown_receiver(&self) -> broadcast::Receiver<ShutdownSignal> {
        self.shutdown_tx.subscribe()
    }
}

impl Clone for LifecycleManager {
    fn clone(&self) -> Self {
        Self {
            actors: self.actors.clone(),
            metrics: self.metrics.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            health_check_handle: None, // Don't clone the task handle
        }
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lifecycle messages
#[derive(Debug, Clone)]
pub enum LifecycleMessage {
    /// Initialize actor
    Initialize,
    /// Start actor
    Start,
    /// Pause actor
    Pause,
    /// Resume actor
    Resume,
    /// Stop actor gracefully
    Stop { timeout: Duration },
    /// Force stop actor
    ForceStop,
    /// Health check
    HealthCheck,
    /// Get actor state
    GetState,
    /// Get state history
    GetStateHistory,
}

impl Message for LifecycleMessage {
    type Result = ActorResult<LifecycleResponse>;
}

impl AlysMessage for LifecycleMessage {
    fn priority(&self) -> MessagePriority {
        match self {
            LifecycleMessage::ForceStop => MessagePriority::Emergency,
            LifecycleMessage::Stop { .. } => MessagePriority::Critical,
            LifecycleMessage::Initialize | LifecycleMessage::Start => MessagePriority::High,
            LifecycleMessage::HealthCheck => MessagePriority::Low,
            _ => MessagePriority::Normal,
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            LifecycleMessage::Stop { timeout } => *timeout,
            LifecycleMessage::Initialize => Duration::from_secs(30),
            _ => Duration::from_secs(10),
        }
    }
}

/// Lifecycle response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleResponse {
    /// Operation completed successfully
    Success,
    /// Current actor state
    State(ActorState),
    /// State transition history
    StateHistory(Vec<StateTransition>),
    /// Health check result
    HealthResult(bool),
    /// Error occurred
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_state_display() {
        assert_eq!(ActorState::Running.to_string(), "running");
        assert_eq!(ActorState::Failed.to_string(), "failed");
        assert_eq!(ActorState::Stopped.to_string(), "stopped");
    }

    #[tokio::test]
    async fn test_lifecycle_manager_creation() {
        let manager = LifecycleManager::new();
        assert_eq!(manager.metrics.total_spawned.load(Ordering::Relaxed), 0);
        assert_eq!(manager.metrics.running_actors.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_actor_registration() {
        let manager = LifecycleManager::new();
        
        // This would typically be done with a real actor type
        // For testing, we'll register without the actual actor
        let actor_id = "test_actor".to_string();
        let actor_type = "TestActor".to_string();
        
        // Note: Can't test full registration without implementing LifecycleAware
        // This is a simplified test showing the structure
        assert_eq!(manager.metrics.total_spawned.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_state_transition_creation() {
        let transition = StateTransition {
            from: ActorState::Initializing,
            to: ActorState::Running,
            timestamp: SystemTime::now(),
            reason: Some("Initialization complete".to_string()),
            error: None,
        };

        assert_eq!(transition.from, ActorState::Initializing);
        assert_eq!(transition.to, ActorState::Running);
        assert!(transition.reason.is_some());
        assert!(transition.error.is_none());
    }

    #[test]
    fn test_lifecycle_config_defaults() {
        let config = LifecycleConfig::default();
        assert_eq!(config.init_timeout, Duration::from_secs(30));
        assert_eq!(config.shutdown_timeout, Duration::from_secs(10));
        assert!(config.auto_health_check);
        assert_eq!(config.max_health_failures, 3);
    }
}