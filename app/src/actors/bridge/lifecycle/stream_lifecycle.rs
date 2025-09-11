//! Stream Actor Lifecycle Management
//! 
//! Lifecycle implementation for Stream actors with actor_system compatibility

use actix::prelude::*;
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::actors::bridge::{
    actors::stream::StreamActor,
    shared::errors::BridgeError,
    config::StreamConfig,
};
use actor_system::{
    lifecycle::{LifecycleAware, ActorState, LifecycleMetadata},
    error::ActorError,
};

/// Lifecycle manager for Stream actors
pub struct StreamLifecycle {
    /// Actor reference
    actor_ref: Option<Addr<StreamActor>>,
    
    /// Current lifecycle phase
    phase: ActorState,
    
    /// Configuration
    config: StreamConfig,
    
    /// Metrics collection
    metrics: LifecycleMetadata,
    
    /// Lifecycle hooks
    hooks: StreamLifecycleHooks,
    
    /// Startup time tracking
    startup_start: Option<Instant>,
    
    /// Last health check time
    last_health_check: Option<Instant>,
    
    /// Restart attempt count
    restart_count: u32,
}

/// Stream-specific lifecycle hooks
pub struct StreamLifecycleHooks {
    /// Governance connection verification
    governance_connection_check: Option<Box<dyn Fn() -> Result<bool, BridgeError> + Send + Sync>>,
    
    /// gRPC protocol setup
    grpc_protocol_setup: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
    
    /// Message buffer validation
    buffer_validation: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
    
    /// Reconnection strategy setup
    reconnection_setup: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
}

impl Default for StreamLifecycleHooks {
    fn default() -> Self {
        Self {
            governance_connection_check: None,
            grpc_protocol_setup: None,
            buffer_validation: None,
            reconnection_setup: None,
        }
    }
}

impl StreamLifecycle {
    /// Create new Stream lifecycle manager
    pub fn new(config: StreamConfig) -> Self {
        Self {
            actor_ref: None,
            phase: ActorState::Initialized,
            config,
            metrics: LifecycleMetadata::new("stream"),
            hooks: StreamLifecycleHooks::default(),
            startup_start: None,
            last_health_check: None,
            restart_count: 0,
        }
    }
    
    /// Set custom hooks
    pub fn with_hooks(mut self, hooks: StreamLifecycleHooks) -> Self {
        self.hooks = hooks;
        self
    }
    
    /// Perform Stream-specific startup checks
    async fn stream_startup_checks(&self) -> Result<(), ActorError> {
        info!("Performing Stream startup checks");
        
        // Governance connection check
        if let Some(check) = &self.hooks.governance_connection_check {
            match check() {
                Ok(connected) => {
                    if !connected {
                        return Err(ActorError::StartupFailed(
                            "Governance connection not available".to_string()
                        ));
                    }
                },
                Err(e) => {
                    return Err(ActorError::StartupFailed(
                        format!("Governance connection check failed: {}", e)
                    ));
                }
            }
        }
        
        // gRPC protocol setup
        if let Some(setup) = &self.hooks.grpc_protocol_setup {
            if let Err(e) = setup() {
                return Err(ActorError::StartupFailed(
                    format!("gRPC protocol setup failed: {}", e)
                ));
            }
        }
        
        // Message buffer validation
        if let Some(validate) = &self.hooks.buffer_validation {
            if let Err(e) = validate() {
                return Err(ActorError::StartupFailed(
                    format!("Message buffer validation failed: {}", e)
                ));
            }
        }
        
        // Reconnection strategy setup
        if let Some(setup) = &self.hooks.reconnection_setup {
            if let Err(e) = setup() {
                return Err(ActorError::StartupFailed(
                    format!("Reconnection setup failed: {}", e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, ActorError> {
        if let Some(actor_ref) = &self.actor_ref {
            match actor_ref.send(crate::actors::bridge::messages::stream_messages::StreamMessage::GetConnectionStatus).await {
                Ok(status) => {
                    self.last_health_check = Some(Instant::now());
                    self.metrics.record_health_check(true);
                    Ok(status.healthy)
                },
                Err(e) => {
                    warn!("Stream health check failed: {}", e);
                    self.metrics.record_health_check(false);
                    Err(ActorError::HealthCheckFailed(format!("Actor communication failed: {}", e)))
                }
            }
        } else {
            Err(ActorError::HealthCheckFailed("No actor reference available".to_string()))
        }
    }
}

// TODO: Implement proper LifecycleAware trait when interface is stabilized

/// Builder for Stream lifecycle configuration
pub struct StreamLifecycleBuilder {
    config: StreamConfig,
    hooks: StreamLifecycleHooks,
}

impl StreamLifecycleBuilder {
    pub fn new(config: StreamConfig) -> Self {
        Self {
            config,
            hooks: StreamLifecycleHooks::default(),
        }
    }
    
    pub fn with_governance_check<F>(mut self, check: F) -> Self 
    where F: Fn() -> Result<bool, BridgeError> + Send + Sync + 'static 
    {
        self.hooks.governance_connection_check = Some(Box::new(check));
        self
    }
    
    pub fn with_grpc_setup<F>(mut self, setup: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.grpc_protocol_setup = Some(Box::new(setup));
        self
    }
    
    pub fn with_buffer_validation<F>(mut self, validate: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.buffer_validation = Some(Box::new(validate));
        self
    }
    
    pub fn with_reconnection_setup<F>(mut self, setup: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.reconnection_setup = Some(Box::new(setup));
        self
    }
    
    pub fn build(self) -> StreamLifecycle {
        StreamLifecycle::new(self.config).with_hooks(self.hooks)
    }
}