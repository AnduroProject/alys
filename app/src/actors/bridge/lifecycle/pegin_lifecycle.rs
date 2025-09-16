//! PegIn Actor Lifecycle Management
//! 
//! Lifecycle implementation for PegIn actors with actor_system compatibility

use actix::prelude::*;
use std::time::Instant;
use tracing::{info, warn};

use crate::actors::bridge::{
    actors::pegin::PegInActor,
    shared::errors::BridgeError,
    config::PegInConfig,
};
use actor_system::{
    lifecycle::{ActorState, LifecycleMetadata, StateTransition},
    error::ActorError,
};
use tokio::sync::{Arc, RwLock};
use std::sync::atomic::AtomicU64;

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
            phase: ActorState::Initializing,
            config,
            metrics: LifecycleMetadata {
                actor_id: "pegin_lifecycle".to_string(),
                actor_type: "PegInLifecycle".to_string(),
                state: Arc::new(RwLock::new(ActorState::Initializing)),
                state_history: Arc::new(RwLock::new(Vec::new())),
                spawn_time: std::time::SystemTime::now(),
                last_state_change: Arc::new(RwLock::new(std::time::SystemTime::now())),
                health_failures: AtomicU64::new(0),
                config: actor_system::lifecycle::LifecycleConfig::default(),
            },
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
                        return Err(ActorError::StartupFailed {
                            actor_type: "PegInActor".to_string(),
                            reason: "Bitcoin connection not available".to_string(),
                        });
                    }
                },
                Err(e) => {
                    return Err(ActorError::StartupFailed {
                        actor_type: "PegInActor".to_string(),
                        reason: format!("Bitcoin connection check failed: {}", e),
                    });
                }
            }
        }
        
        // Queue validation
        if let Some(validate) = &self.hooks.queue_validation {
            if let Err(e) = validate() {
                return Err(ActorError::StartupFailed {
                    actor_type: "PegInActor".to_string(),
                    reason: format!("PegIn queue validation failed: {}", e),
                });
            }
        }
        
        // Signature setup
        if let Some(setup) = &self.hooks.signature_setup {
            if let Err(e) = setup() {
                return Err(ActorError::StartupFailed {
                    actor_type: "PegInActor".to_string(),
                    reason: format!("Signature setup failed: {}", e),
                });
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, ActorError> {
        if let Some(actor_ref) = &self.actor_ref {
            match actor_ref.send(crate::actors::bridge::actors::pegin::handlers::GetPegInStatus).await {
                Ok(status_result) => {
                    match status_result {
                        Ok(status) => {
                            self.last_health_check = Some(Instant::now());
                            // Health check based on actor state - simplified check
                            let is_healthy = status.processing_deposits > 0 || status.error_count < 10;
                            Ok(is_healthy)
                        },
                        Err(e) => {
                            warn!("PegIn status check failed: {}", e);
                            self.metrics.health_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            Ok(false) // Actor responding but in error state
                        }
                    }
                },
                Err(e) => {
                    warn!("PegIn health check failed: {}", e);
                    self.metrics.health_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Err(ActorError::ActorNotReady {
                        actor_type: "PegInActor".to_string(),
                        reason: format!("Actor communication failed: {}", e)
                    })
                }
            }
        } else {
            Err(ActorError::ActorNotReady {
                actor_type: "PegInActor".to_string(),
                reason: "No actor reference available".to_string()
            })
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