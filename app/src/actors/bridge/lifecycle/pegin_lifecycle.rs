//! PegIn Actor Lifecycle Management
//! 
//! Lifecycle implementation for PegIn actors with actor_system compatibility

use actix::prelude::*;
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::actors::bridge::{
    actors::pegin::PegInActor,
    shared::errors::BridgeError,
    config::PegInConfig,
};
use actor_system::{
    lifecycle::{LifecycleAware, ActorState, LifecycleMetadata},
    error::ActorError,
};

/// Lifecycle manager for PegIn actors
pub struct PegInLifecycle {
    /// Actor reference
    actor_ref: Option<Addr<PegInActor>>,
    
    /// Current lifecycle phase
    phase: ActorState,
    
    /// Configuration
    config: PegInConfig,
    
    /// Metrics collection
    metrics: LifecycleMetadata,
    
    /// Lifecycle hooks
    hooks: PegInLifecycleHooks,
    
    /// Startup time tracking
    startup_start: Option<Instant>,
    
    /// Last health check time
    last_health_check: Option<Instant>,
    
    /// Restart attempt count
    restart_count: u32,
}

/// PegIn-specific lifecycle hooks
pub struct PegInLifecycleHooks {
    /// Bitcoin connection verification
    bitcoin_connection_check: Option<Box<dyn Fn() -> Result<bool, BridgeError> + Send + Sync>>,
    
    /// PegIn queue validation
    queue_validation: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
    
    /// Signature verification setup
    signature_setup: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
}

impl Default for PegInLifecycleHooks {
    fn default() -> Self {
        Self {
            bitcoin_connection_check: None,
            queue_validation: None,
            signature_setup: None,
        }
    }
}

impl PegInLifecycle {
    /// Create new PegIn lifecycle manager
    pub fn new(config: PegInConfig) -> Self {
        Self {
            actor_ref: None,
            phase: ActorState::Initialized,
            config,
            metrics: LifecycleMetadata::new("pegin"),
            hooks: PegInLifecycleHooks::default(),
            startup_start: None,
            last_health_check: None,
            restart_count: 0,
        }
    }
    
    /// Set custom hooks
    pub fn with_hooks(mut self, hooks: PegInLifecycleHooks) -> Self {
        self.hooks = hooks;
        self
    }
    
    /// Perform PegIn-specific startup checks
    async fn pegin_startup_checks(&self) -> Result<(), ActorError> {
        info!("Performing PegIn startup checks");
        
        // Bitcoin connection check
        if let Some(check) = &self.hooks.bitcoin_connection_check {
            match check() {
                Ok(connected) => {
                    if !connected {
                        return Err(ActorError::StartupFailed(
                            "Bitcoin connection not available".to_string()
                        ));
                    }
                },
                Err(e) => {
                    return Err(ActorError::StartupFailed(
                        format!("Bitcoin connection check failed: {}", e)
                    ));
                }
            }
        }
        
        // Queue validation
        if let Some(validate) = &self.hooks.queue_validation {
            if let Err(e) = validate() {
                return Err(ActorError::StartupFailed(
                    format!("PegIn queue validation failed: {}", e)
                ));
            }
        }
        
        // Signature setup
        if let Some(setup) = &self.hooks.signature_setup {
            if let Err(e) = setup() {
                return Err(ActorError::StartupFailed(
                    format!("Signature setup failed: {}", e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, ActorError> {
        if let Some(actor_ref) = &self.actor_ref {
            match actor_ref.send(crate::actors::bridge::actors::pegin::handlers::GetPegInStatus).await {
                Ok(status) => {
                    self.last_health_check = Some(Instant::now());
                    self.metrics.record_health_check(true);
                    Ok(status.healthy)
                },
                Err(e) => {
                    warn!("PegIn health check failed: {}", e);
                    self.metrics.record_health_check(false);
                    Err(ActorError::HealthCheckFailed(format!("Actor communication failed: {}", e)))
                }
            }
        } else {
            Err(ActorError::HealthCheckFailed("No actor reference available".to_string()))
        }
    }
}


/// Builder for PegIn lifecycle configuration
pub struct PegInLifecycleBuilder {
    config: PegInConfig,
    hooks: PegInLifecycleHooks,
}

impl PegInLifecycleBuilder {
    pub fn new(config: PegInConfig) -> Self {
        Self {
            config,
            hooks: PegInLifecycleHooks::default(),
        }
    }
    
    pub fn with_bitcoin_check<F>(mut self, check: F) -> Self 
    where F: Fn() -> Result<bool, BridgeError> + Send + Sync + 'static 
    {
        self.hooks.bitcoin_connection_check = Some(Box::new(check));
        self
    }
    
    pub fn with_queue_validation<F>(mut self, validate: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.queue_validation = Some(Box::new(validate));
        self
    }
    
    pub fn with_signature_setup<F>(mut self, setup: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.signature_setup = Some(Box::new(setup));
        self
    }
    
    pub fn build(self) -> PegInLifecycle {
        PegInLifecycle::new(self.config).with_hooks(self.hooks)
    }
}