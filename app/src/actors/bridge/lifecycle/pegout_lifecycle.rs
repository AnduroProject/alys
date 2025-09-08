//! PegOut Actor Lifecycle Management
//! 
//! Lifecycle implementation for PegOut actors with actor_system compatibility

use actix::prelude::*;
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::actors::bridge::{
    actors::pegout::PegOutActor,
    shared::errors::BridgeError,
    config::PegOutConfig,
};
use crate::actors::bridge::shared::lifecycle::{
    ActorLifecycle, LifecyclePhase, LifecycleMetrics, LifecycleError,
    LifecycleEvent, LifecycleCommand, LifecycleState, LifecycleHooks,
    StartupHook, ShutdownHook, RestartHook, HealthCheckHook
};

/// Lifecycle manager for PegOut actors
pub struct PegOutLifecycle {
    /// Actor reference
    actor_ref: Option<Addr<PegOutActor>>,
    
    /// Current lifecycle phase
    phase: LifecyclePhase,
    
    /// Configuration
    config: PegOutConfig,
    
    /// Metrics collection
    metrics: LifecycleMetrics,
    
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
            phase: LifecyclePhase::Initialized,
            config,
            metrics: LifecycleMetrics::new("pegout"),
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
    async fn pegout_startup_checks(&self) -> Result<(), LifecycleError> {
        info!("Performing PegOut startup checks");
        
        // Wallet verification
        if let Some(verify) = &self.hooks.wallet_verification {
            match verify() {
                Ok(verified) => {
                    if !verified {
                        return Err(LifecycleError::StartupFailed(
                            "Bitcoin wallet verification failed".to_string()
                        ));
                    }
                },
                Err(e) => {
                    return Err(LifecycleError::StartupFailed(
                        format!("Wallet verification error: {}", e)
                    ));
                }
            }
        }
        
        // Federation signature check
        if let Some(check) = &self.hooks.federation_check {
            if let Err(e) = check() {
                return Err(LifecycleError::StartupFailed(
                    format!("Federation signature check failed: {}", e)
                ));
            }
        }
        
        // Fee estimation setup
        if let Some(setup) = &self.hooks.fee_estimation_setup {
            if let Err(e) = setup() {
                return Err(LifecycleError::StartupFailed(
                    format!("Fee estimation setup failed: {}", e)
                ));
            }
        }
        
        // UTXO validation
        if let Some(validate) = &self.hooks.utxo_validation {
            if let Err(e) = validate() {
                return Err(LifecycleError::StartupFailed(
                    format!("UTXO validation failed: {}", e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, LifecycleError> {
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
                    Err(LifecycleError::HealthCheckFailed(format!("Actor communication failed: {}", e)))
                }
            }
        } else {
            Err(LifecycleError::HealthCheckFailed("No actor reference available".to_string()))
        }
    }
}

impl ActorLifecycle for PegOutLifecycle {
    type Actor = PegOutActor;
    type Config = PegOutConfig;
    type Error = LifecycleError;
    
    fn actor_name(&self) -> &'static str {
        "PegOutActor"
    }
    
    fn current_phase(&self) -> LifecyclePhase {
        self.phase
    }
    
    fn metrics(&self) -> &LifecycleMetrics {
        &self.metrics
    }
    
    async fn start(&mut self) -> Result<Addr<Self::Actor>, Self::Error> {
        info!("Starting PegOut actor lifecycle");
        self.phase = LifecyclePhase::Starting;
        self.startup_start = Some(Instant::now());
        
        // Perform startup checks
        self.pegout_startup_checks().await?;
        
        // Create and start actor
        let actor = PegOutActor::new(self.config.clone())
            .map_err(|e| LifecycleError::StartupFailed(format!("Actor creation failed: {}", e)))?;
        
        let addr = actor.start();
        self.actor_ref = Some(addr.clone());
        self.phase = LifecyclePhase::Running;
        
        // Record startup metrics
        if let Some(start_time) = self.startup_start {
            let startup_duration = start_time.elapsed();
            self.metrics.record_startup(startup_duration);
            info!("PegOut actor started in {:?}", startup_duration);
        }
        
        Ok(addr)
    }
    
    async fn stop(&mut self) -> Result<(), Self::Error> {
        info!("Stopping PegOut actor");
        self.phase = LifecyclePhase::Stopping;
        
        if let Some(actor_ref) = self.actor_ref.take() {
            // Graceful shutdown with timeout
            let shutdown_timeout = Duration::from_secs(30);
            
            match tokio::time::timeout(shutdown_timeout, async {
                actor_ref.send(actix::prelude::System::current().stop()).await
            }).await {
                Ok(_) => {
                    info!("PegOut actor stopped gracefully");
                    self.phase = LifecyclePhase::Stopped;
                    self.metrics.record_shutdown();
                    Ok(())
                },
                Err(_) => {
                    warn!("PegOut actor shutdown timeout, forcing stop");
                    self.phase = LifecyclePhase::Failed;
                    Err(LifecycleError::ShutdownTimeout)
                }
            }
        } else {
            warn!("No PegOut actor reference to stop");
            self.phase = LifecyclePhase::Stopped;
            Ok(())
        }
    }
    
    async fn restart(&mut self) -> Result<Addr<Self::Actor>, Self::Error> {
        info!("Restarting PegOut actor (attempt #{})", self.restart_count + 1);
        self.restart_count += 1;
        
        // Stop current instance
        if let Err(e) = self.stop().await {
            warn!("Error stopping PegOut actor during restart: {}", e);
        }
        
        // Brief delay before restart
        tokio::time::sleep(Duration::from_millis(1000)).await;
        
        // Start new instance
        let addr = self.start().await?;
        self.metrics.record_restart(self.restart_count);
        
        Ok(addr)
    }
    
    async fn health_check(&mut self) -> Result<bool, Self::Error> {
        self.health_check().await
    }
}

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