//! Enhanced actor traits and implementations

use crate::error::{ActorError, ActorResult, ErrorContext};
use crate::metrics::ActorMetrics;
use actix::prelude::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Enhanced actor trait with lifecycle management and error handling
#[async_trait]
pub trait AlysActor: Actor + Send + Sync {
    /// Actor type name for identification and logging
    fn actor_type(&self) -> &'static str;
    
    /// Actor instance name (unique identifier)
    fn actor_name(&self) -> &str;
    
    /// Actor configuration
    fn config(&self) -> &ActorConfig;
    
    /// Initialize actor resources
    async fn initialize(&mut self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Clean up actor resources
    async fn cleanup(&mut self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Handle actor restart
    async fn on_restart(&mut self, reason: &ActorError) -> ActorResult<()> {
        tracing::warn!(
            actor_name = self.actor_name(),
            actor_type = self.actor_type(),
            reason = %reason,
            "Actor restarting"
        );
        Ok(())
    }
    
    /// Check if actor should restart on error
    fn should_restart(&self, error: &ActorError) -> bool {
        error.should_restart_actor()
    }
    
    /// Get actor health status
    async fn health_check(&self) -> ActorResult<HealthStatus> {
        Ok(HealthStatus::Healthy)
    }
    
    /// Get actor metrics
    fn metrics(&self) -> &ActorMetrics {
        static EMPTY_METRICS: once_cell::sync::Lazy<ActorMetrics> = 
            once_cell::sync::Lazy::new(ActorMetrics::new);
        &EMPTY_METRICS
    }
    
    /// Handle graceful shutdown
    async fn prepare_shutdown(&mut self) -> ActorResult<()> {
        Ok(())
    }
}

/// Actor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorConfig {
    /// Actor name
    pub name: String,
    
    /// Maximum mailbox size (0 = unlimited)
    pub max_mailbox_size: usize,
    
    /// Message processing timeout
    pub message_timeout: Duration,
    
    /// Restart strategy
    pub restart_strategy: RestartStrategy,
    
    /// Health check interval
    pub health_check_interval: Duration,
    
    /// Enable metrics collection
    pub enable_metrics: bool,
    
    /// Actor-specific configuration
    pub custom_config: HashMap<String, serde_json::Value>,
}

impl Default for ActorConfig {
    fn default() -> Self {
        Self {
            name: format!("actor_{}", Uuid::new_v4().simple()),
            max_mailbox_size: 1000,
            message_timeout: Duration::from_secs(30),
            restart_strategy: RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(30),
                multiplier: 2.0,
                max_retries: 5,
            },
            health_check_interval: Duration::from_secs(30),
            enable_metrics: true,
            custom_config: HashMap::new(),
        }
    }
}

/// Restart strategies for actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RestartStrategy {
    /// Never restart
    Never,
    
    /// Restart immediately
    Immediate,
    
    /// Restart after a fixed delay
    FixedDelay(Duration),
    
    /// Exponential backoff with jitter
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        max_retries: u32,
    },
    
    /// Linear backoff
    LinearBackoff {
        initial_delay: Duration,
        increment: Duration,
        max_delay: Duration,
        max_retries: u32,
    },
}

impl RestartStrategy {
    /// Calculate delay for attempt number
    pub fn delay_for_attempt(&self, attempt: u32) -> Option<Duration> {
        match self {
            RestartStrategy::Never => None,
            RestartStrategy::Immediate => Some(Duration::from_millis(0)),
            RestartStrategy::FixedDelay(delay) => Some(*delay),
            RestartStrategy::ExponentialBackoff {
                initial_delay,
                max_delay,
                multiplier,
                max_retries,
            } => {
                if attempt >= *max_retries {
                    return None;
                }
                
                let delay = Duration::from_millis(
                    (initial_delay.as_millis() as f64 * multiplier.powi(attempt as i32)) as u64
                );
                
                Some(delay.min(*max_delay))
            }
            RestartStrategy::LinearBackoff {
                initial_delay,
                increment,
                max_delay,
                max_retries,
            } => {
                if attempt >= *max_retries {
                    return None;
                }
                
                let delay = *initial_delay + *increment * attempt;
                Some(delay.min(*max_delay))
            }
        }
    }
    
    /// Check if more restarts are allowed
    pub fn can_restart(&self, attempt: u32) -> bool {
        match self {
            RestartStrategy::Never => false,
            RestartStrategy::Immediate => true,
            RestartStrategy::FixedDelay(_) => true,
            RestartStrategy::ExponentialBackoff { max_retries, .. } => attempt < *max_retries,
            RestartStrategy::LinearBackoff { max_retries, .. } => attempt < *max_retries,
        }
    }
}

/// Actor health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    /// Actor is healthy and functioning normally
    Healthy,
    
    /// Actor is degraded but still functional
    Degraded { issues: Vec<String> },
    
    /// Actor is unhealthy and may not function correctly
    Unhealthy { critical_issues: Vec<String> },
    
    /// Actor is shutting down
    ShuttingDown,
    
    /// Actor has stopped
    Stopped,
}

impl HealthStatus {
    /// Check if actor is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded { .. })
    }
    
    /// Check if actor needs attention
    pub fn needs_attention(&self) -> bool {
        !matches!(self, HealthStatus::Healthy)
    }
}

/// Actor state for lifecycle management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActorState {
    /// Actor is initializing
    Initializing,
    
    /// Actor is running normally
    Running,
    
    /// Actor is paused
    Paused,
    
    /// Actor is restarting
    Restarting,
    
    /// Actor is shutting down
    ShuttingDown,
    
    /// Actor has stopped
    Stopped,
    
    /// Actor has failed
    Failed { reason: String },
}

impl ActorState {
    /// Check if actor is active
    pub fn is_active(&self) -> bool {
        matches!(self, ActorState::Running | ActorState::Paused)
    }
    
    /// Check if actor can receive messages
    pub fn can_receive_messages(&self) -> bool {
        matches!(self, ActorState::Running | ActorState::Paused)
    }
}

/// Enhanced actor context with additional functionality
pub trait AlysContext: AsyncContext<Self> {
    /// Get error context for current actor
    fn error_context(&self) -> ErrorContext;
    
    /// Report error with context
    fn report_error(&self, error: ActorError) {
        let context = self.error_context();
        crate::error::report_error(&error, Some(&context));
    }
    
    /// Schedule delayed message with timeout handling
    fn schedule_with_timeout<M>(&mut self, message: M, delay: Duration, timeout: Duration) -> SpawnHandle
    where
        M: Message + Send + 'static,
        Self: Handler<M>,
        M::Result: Send;
    
    /// Send message with retry logic
    fn send_with_retry<A, M>(&mut self, actor: &Addr<A>, message: M, max_retries: u32) -> ResponseFuture<Result<M::Result, ActorError>>
    where
        A: Actor + Handler<M>,
        M: Message + Send + Clone + 'static,
        M::Result: Send;
}

/// Base actor implementation with common functionality
pub struct BaseActor {
    pub config: ActorConfig,
    pub state: ActorState,
    pub metrics: ActorMetrics,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub restart_count: u32,
}

impl BaseActor {
    /// Create new base actor
    pub fn new(config: ActorConfig) -> Self {
        let now = SystemTime::now();
        Self {
            metrics: if config.enable_metrics {
                ActorMetrics::new()
            } else {
                ActorMetrics::disabled()
            },
            config,
            state: ActorState::Initializing,
            created_at: now,
            last_activity: now,
            restart_count: 0,
        }
    }
    
    /// Update last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = SystemTime::now();
        self.metrics.record_activity();
    }
    
    /// Transition actor state
    pub fn transition_state(&mut self, new_state: ActorState) -> ActorResult<()> {
        let old_state = self.state.clone();
        
        // Validate state transition
        let valid = match (&old_state, &new_state) {
            (ActorState::Initializing, ActorState::Running) => true,
            (ActorState::Initializing, ActorState::Failed { .. }) => true,
            (ActorState::Running, ActorState::Paused) => true,
            (ActorState::Running, ActorState::Restarting) => true,
            (ActorState::Running, ActorState::ShuttingDown) => true,
            (ActorState::Running, ActorState::Failed { .. }) => true,
            (ActorState::Paused, ActorState::Running) => true,
            (ActorState::Paused, ActorState::ShuttingDown) => true,
            (ActorState::Restarting, ActorState::Running) => true,
            (ActorState::Restarting, ActorState::Failed { .. }) => true,
            (ActorState::ShuttingDown, ActorState::Stopped) => true,
            (ActorState::Failed { .. }, ActorState::Restarting) => true,
            (ActorState::Failed { .. }, ActorState::Stopped) => true,
            _ => false,
        };
        
        if !valid {
            return Err(ActorError::InvalidStateTransition {
                from: format!("{:?}", old_state),
                to: format!("{:?}", new_state),
            });
        }
        
        self.state = new_state;
        self.metrics.record_state_transition();
        
        tracing::debug!(
            actor_name = %self.config.name,
            old_state = ?old_state,
            new_state = ?self.state,
            "Actor state transition"
        );
        
        Ok(())
    }
    
    /// Get uptime duration
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed().unwrap_or_default()
    }
    
    /// Get idle duration
    pub fn idle_duration(&self) -> Duration {
        self.last_activity.elapsed().unwrap_or_default()
    }
}

impl Actor for BaseActor {
    type Context = Context<Self>;
    
    fn started(&mut self, _ctx: &mut Self::Context) {
        let _ = self.transition_state(ActorState::Running);
        self.update_activity();
        
        tracing::info!(
            actor_name = %self.config.name,
            "Actor started"
        );
    }
    
    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        let _ = self.transition_state(ActorState::ShuttingDown);
        
        tracing::info!(
            actor_name = %self.config.name,
            uptime = ?self.uptime(),
            "Actor stopping"
        );
        
        Running::Stop
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        let _ = self.transition_state(ActorState::Stopped);
        
        tracing::info!(
            actor_name = %self.config.name,
            uptime = ?self.uptime(),
            restart_count = self.restart_count,
            "Actor stopped"
        );
    }
}

/// Actor wrapper for enhanced functionality
pub struct ActorWrapper<T> 
where
    T: AlysActor,
{
    inner: T,
    base: BaseActor,
}

impl<T> ActorWrapper<T> 
where 
    T: AlysActor,
{
    /// Create new actor wrapper
    pub fn new(actor: T, config: ActorConfig) -> Self {
        Self {
            inner: actor,
            base: BaseActor::new(config),
        }
    }
    
    /// Get reference to inner actor
    pub fn inner(&self) -> &T {
        &self.inner
    }
    
    /// Get mutable reference to inner actor
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }
    
    /// Get base actor
    pub fn base(&self) -> &BaseActor {
        &self.base
    }
    
    /// Get mutable base actor
    pub fn base_mut(&mut self) -> &mut BaseActor {
        &mut self.base
    }
}

impl<T> Actor for ActorWrapper<T>
where
    T: AlysActor + 'static,
{
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        self.base.started(ctx);
        
        // Initialize inner actor
        let inner_init = self.inner.initialize();
        let actor_name = self.inner.actor_name().to_string();
        
        ctx.spawn(
            async move {
                if let Err(e) = inner_init.await {
                    tracing::error!(
                        actor_name = %actor_name,
                        error = %e,
                        "Actor initialization failed"
                    );
                }
            }
            .into_actor(self)
            .map(|_, _, _| ())
        );
    }
    
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // Prepare inner actor for shutdown
        let inner_shutdown = self.inner.prepare_shutdown();
        let actor_name = self.inner.actor_name().to_string();
        
        ctx.spawn(
            async move {
                if let Err(e) = inner_shutdown.await {
                    tracing::error!(
                        actor_name = %actor_name,
                        error = %e,
                        "Actor shutdown preparation failed"
                    );
                }
            }
            .into_actor(self)
            .map(|_, _, _| ())
        );
        
        self.base.stopping(ctx)
    }
    
    fn stopped(&mut self, ctx: &mut Self::Context) {
        // Clean up inner actor
        let inner_cleanup = self.inner.cleanup();
        let actor_name = self.inner.actor_name().to_string();
        
        // Note: Can't spawn futures in stopped, so we block
        if let Err(e) = futures::executor::block_on(inner_cleanup) {
            tracing::error!(
                actor_name = %actor_name,
                error = %e,
                "Actor cleanup failed"
            );
        }
        
        self.base.stopped(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_restart_strategy_exponential_backoff() {
        let strategy = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            max_retries: 3,
        };
        
        assert_eq!(strategy.delay_for_attempt(0), Some(Duration::from_millis(100)));
        assert_eq!(strategy.delay_for_attempt(1), Some(Duration::from_millis(200)));
        assert_eq!(strategy.delay_for_attempt(2), Some(Duration::from_millis(400)));
        assert_eq!(strategy.delay_for_attempt(3), None);
        
        assert!(strategy.can_restart(0));
        assert!(strategy.can_restart(2));
        assert!(!strategy.can_restart(3));
    }
    
    #[test]
    fn test_actor_state_transitions() {
        let mut base = BaseActor::new(ActorConfig::default());
        
        assert!(base.transition_state(ActorState::Running).is_ok());
        assert!(base.transition_state(ActorState::Paused).is_ok());
        assert!(base.transition_state(ActorState::Running).is_ok());
        assert!(base.transition_state(ActorState::ShuttingDown).is_ok());
        assert!(base.transition_state(ActorState::Stopped).is_ok());
        
        // Invalid transition
        assert!(base.transition_state(ActorState::Running).is_err());
    }
    
    #[test]
    fn test_health_status() {
        assert!(HealthStatus::Healthy.is_operational());
        assert!(!HealthStatus::Healthy.needs_attention());
        
        let degraded = HealthStatus::Degraded { issues: vec!["minor issue".to_string()] };
        assert!(degraded.is_operational());
        assert!(degraded.needs_attention());
        
        let unhealthy = HealthStatus::Unhealthy { critical_issues: vec!["critical".to_string()] };
        assert!(!unhealthy.is_operational());
        assert!(unhealthy.needs_attention());
    }
}