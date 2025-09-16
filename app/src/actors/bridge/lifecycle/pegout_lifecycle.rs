//! PegOut Actor Lifecycle Management
//! 
//! Lifecycle implementation for PegOut actors with actor_system compatibility

use actix::prelude::*;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::actors::bridge::{
    actors::pegout::PegOutActor,
    shared::errors::BridgeError,
    config::PegOutConfig,
};
use actor_system::{
    lifecycle::{LifecycleAware, ActorState, LifecycleMetadata},
    error::ActorError,
};

/// Lifecycle manager for PegOut actors
pub struct PegOutLifecycle {
    /// Actor reference
    actor_ref: Option<Addr<PegOutActor>>,
    
    /// Current lifecycle phase
    phase: ActorState,
    
    /// Configuration
    config: PegOutConfig,
    
    /// Metrics collection
    metrics: LifecycleMetadata,
    
    /// Lifecycle hooks
    hooks: PegOutLifecycleHooks,
    
    /// Startup time tracking
    startup_start: Option<Instant>,
    
    /// Last health check time
    last_health_check: Option<Instant>,
    
    /// Restart attempt count
    restart_count: u32,
}

/// PegOut-specific lifecycle hooks
pub struct PegOutLifecycleHooks {
    /// Bitcoin wallet verification
    wallet_verification: Option<Box<dyn Fn() -> Result<bool, BridgeError> + Send + Sync>>,
    
    /// Federation signature verification
    federation_check: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
    
    /// Transaction fee estimation setup
    fee_estimation_setup: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
    
    /// UTXO validation
    utxo_validation: Option<Box<dyn Fn() -> Result<(), BridgeError> + Send + Sync>>,
}

impl Default for PegOutLifecycleHooks {
    fn default() -> Self {
        Self {
            wallet_verification: None,
            federation_check: None,
            fee_estimation_setup: None,
            utxo_validation: None,
        }
    }
}

impl PegOutLifecycle {
    /// Create new PegOut lifecycle manager
    pub fn new(config: PegOutConfig) -> Self {
        Self {
            actor_ref: None,
            phase: ActorState::Initializing,
            config,
            metrics: LifecycleMetadata::default(),
            hooks: PegOutLifecycleHooks::default(),
            startup_start: None,
            last_health_check: None,
            restart_count: 0,
        }
    }
    
    /// Set custom hooks
    pub fn with_hooks(mut self, hooks: PegOutLifecycleHooks) -> Self {
        self.hooks = hooks;
        self
    }
    
    /// Perform PegOut-specific startup checks
    async fn pegout_startup_checks(&self) -> Result<(), ActorError> {
        info!("Performing PegOut startup checks");
        
        // Wallet verification
        if let Some(verify) = &self.hooks.wallet_verification {
            match verify() {
                Ok(verified) => {
                    if !verified {
                        return Err(ActorError::StartupFailed(
                            "Bitcoin wallet verification failed".to_string()
                        ));
                    }
                },
                Err(e) => {
                    return Err(ActorError::StartupFailed(
                        format!("Wallet verification error: {}", e)
                    ));
                }
            }
        }
        
        // Federation signature check
        if let Some(check) = &self.hooks.federation_check {
            if let Err(e) = check() {
                return Err(ActorError::StartupFailed(
                    format!("Federation signature check failed: {}", e)
                ));
            }
        }
        
        // Fee estimation setup
        if let Some(setup) = &self.hooks.fee_estimation_setup {
            if let Err(e) = setup() {
                return Err(ActorError::StartupFailed(
                    format!("Fee estimation setup failed: {}", e)
                ));
            }
        }
        
        // UTXO validation
        if let Some(validate) = &self.hooks.utxo_validation {
            if let Err(e) = validate() {
                return Err(ActorError::StartupFailed(
                    format!("UTXO validation failed: {}", e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, ActorError> {
        if let Some(actor_ref) = &self.actor_ref {
            match actor_ref.send(crate::actors::bridge::actors::pegout::handlers::GetPegOutStatus).await {
                Ok(status) => {
                    self.last_health_check = Some(Instant::now());
                    self.metrics.record_health_check(true);
                    Ok(status.healthy)
                },
                Err(e) => {
                    warn!("PegOut health check failed: {}", e);
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

/// Builder for PegOut lifecycle configuration
pub struct PegOutLifecycleBuilder {
    config: PegOutConfig,
    hooks: PegOutLifecycleHooks,
}

impl PegOutLifecycleBuilder {
    pub fn new(config: PegOutConfig) -> Self {
        Self {
            config,
            hooks: PegOutLifecycleHooks::default(),
        }
    }
    
    pub fn with_wallet_verification<F>(mut self, verify: F) -> Self 
    where F: Fn() -> Result<bool, BridgeError> + Send + Sync + 'static 
    {
        self.hooks.wallet_verification = Some(Box::new(verify));
        self
    }
    
    pub fn with_federation_check<F>(mut self, check: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.federation_check = Some(Box::new(check));
        self
    }
    
    pub fn with_fee_estimation<F>(mut self, setup: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.fee_estimation_setup = Some(Box::new(setup));
        self
    }
    
    pub fn with_utxo_validation<F>(mut self, validate: F) -> Self
    where F: Fn() -> Result<(), BridgeError> + Send + Sync + 'static
    {
        self.hooks.utxo_validation = Some(Box::new(validate));
        self
    }
    
    pub fn build(self) -> PegOutLifecycle {
        PegOutLifecycle::new(self.config).with_hooks(self.hooks)
    }
}