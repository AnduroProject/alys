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
use crate::actors::bridge::shared::lifecycle::{
    ActorLifecycle, LifecyclePhase, LifecycleMetrics, LifecycleError,
    LifecycleEvent, LifecycleCommand, LifecycleState, LifecycleHooks,
    StartupHook, ShutdownHook, RestartHook, HealthCheckHook
};

/// Lifecycle manager for PegIn actors
pub struct PegInLifecycle {
    /// Actor reference
    actor_ref: Option<Addr<PegInActor>>,
    
    /// Current lifecycle phase
    phase: LifecyclePhase,
    
    /// Configuration
    config: PegInConfig,
    
    /// Metrics collection
    metrics: LifecycleMetrics,
    
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
            phase: LifecyclePhase::Initialized,
            config,
            metrics: LifecycleMetrics::new("pegin"),
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
    async fn pegin_startup_checks(&self) -> Result<(), LifecycleError> {
        info!("Performing PegIn startup checks");
        
        // Bitcoin connection check
        if let Some(check) = &self.hooks.bitcoin_connection_check {
            match check() {
                Ok(connected) => {
                    if !connected {
                        return Err(LifecycleError::StartupFailed(
                            "Bitcoin connection not available".to_string()
                        ));
                    }
                },
                Err(e) => {
                    return Err(LifecycleError::StartupFailed(
                        format!("Bitcoin connection check failed: {}", e)
                    ));
                }
            }
        }
        
        // Queue validation
        if let Some(validate) = &self.hooks.queue_validation {
            if let Err(e) = validate() {
                return Err(LifecycleError::StartupFailed(
                    format!("PegIn queue validation failed: {}", e)
                ));
            }
        }
        
        // Signature setup
        if let Some(setup) = &self.hooks.signature_setup {
            if let Err(e) = setup() {
                return Err(LifecycleError::StartupFailed(
                    format!("Signature setup failed: {}", e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, LifecycleError> {
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
                    Err(LifecycleError::HealthCheckFailed(format!("Actor communication failed: {}", e)))
                }
            }
        } else {
            Err(LifecycleError::HealthCheckFailed("No actor reference available".to_string()))
        }
    }
}

impl ActorLifecycle for PegInLifecycle {
    type Actor = PegInActor;
    type Config = PegInConfig;
    type Error = LifecycleError;
    
    fn actor_name(&self) -> &'static str {
        "PegInActor"
    }
    
    fn current_phase(&self) -> LifecyclePhase {
        self.phase
    }
    
    fn metrics(&self) -> &LifecycleMetrics {
        &self.metrics
    }
    
    async fn start(&mut self) -> Result<Addr<Self::Actor>, Self::Error> {
        info!("Starting PegIn actor lifecycle");
        self.phase = LifecyclePhase::Starting;
        self.startup_start = Some(Instant::now());
        
        // Perform startup checks
        self.pegin_startup_checks().await?;
        
        // Create and start actor
        let actor = PegInActor::new(self.config.clone())
            .map_err(|e| LifecycleError::StartupFailed(format!("Actor creation failed: {}", e)))?;
        
        let addr = actor.start();
        self.actor_ref = Some(addr.clone());
        self.phase = LifecyclePhase::Running;
        
        // Record startup metrics
        if let Some(start_time) = self.startup_start {
            let startup_duration = start_time.elapsed();
            self.metrics.record_startup(startup_duration);
            info!("PegIn actor started in {:?}", startup_duration);
        }
        
        Ok(addr)
    }
    
    async fn stop(&mut self) -> Result<(), Self::Error> {
        info!("Stopping PegIn actor");
        self.phase = LifecyclePhase::Stopping;
        
        if let Some(actor_ref) = self.actor_ref.take() {
            // Graceful shutdown with timeout
            let shutdown_timeout = Duration::from_secs(30);
            
            match tokio::time::timeout(shutdown_timeout, async {
                actor_ref.send(actix::prelude::System::current().stop()).await
            }).await {
                Ok(_) => {
                    info!("PegIn actor stopped gracefully");
                    self.phase = LifecyclePhase::Stopped;
                    self.metrics.record_shutdown();
                    Ok(())
                },
                Err(_) => {
                    warn!("PegIn actor shutdown timeout, forcing stop");
                    self.phase = LifecyclePhase::Failed;
                    Err(LifecycleError::ShutdownTimeout)
                }
            }
        } else {
            warn!("No PegIn actor reference to stop");
            self.phase = LifecyclePhase::Stopped;
            Ok(())
        }
    }
    
    async fn restart(&mut self) -> Result<Addr<Self::Actor>, Self::Error> {
        info!("Restarting PegIn actor (attempt #{})", self.restart_count + 1);
        self.restart_count += 1;
        
        // Stop current instance
        if let Err(e) = self.stop().await {
            warn!("Error stopping PegIn actor during restart: {}", e);
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