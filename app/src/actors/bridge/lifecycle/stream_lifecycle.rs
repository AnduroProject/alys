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
use crate::actors::bridge::shared::lifecycle::{
    ActorLifecycle, LifecyclePhase, LifecycleMetrics, LifecycleError,
    LifecycleEvent, LifecycleCommand, LifecycleState, LifecycleHooks,
    StartupHook, ShutdownHook, RestartHook, HealthCheckHook
};

/// Lifecycle manager for Stream actors
pub struct StreamLifecycle {
    /// Actor reference
    actor_ref: Option<Addr<StreamActor>>,
    
    /// Current lifecycle phase
    phase: LifecyclePhase,
    
    /// Configuration
    config: StreamConfig,
    
    /// Metrics collection
    metrics: LifecycleMetrics,
    
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
            phase: LifecyclePhase::Initialized,
            config,
            metrics: LifecycleMetrics::new("stream"),
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
    async fn stream_startup_checks(&self) -> Result<(), LifecycleError> {
        info!("Performing Stream startup checks");
        
        // Governance connection check
        if let Some(check) = &self.hooks.governance_connection_check {
            match check() {
                Ok(connected) => {
                    if !connected {
                        return Err(LifecycleError::StartupFailed(
                            "Governance connection not available".to_string()
                        ));
                    }
                },
                Err(e) => {
                    return Err(LifecycleError::StartupFailed(
                        format!("Governance connection check failed: {}", e)
                    ));
                }
            }
        }
        
        // gRPC protocol setup
        if let Some(setup) = &self.hooks.grpc_protocol_setup {
            if let Err(e) = setup() {
                return Err(LifecycleError::StartupFailed(
                    format!("gRPC protocol setup failed: {}", e)
                ));
            }
        }
        
        // Message buffer validation
        if let Some(validate) = &self.hooks.buffer_validation {
            if let Err(e) = validate() {
                return Err(LifecycleError::StartupFailed(
                    format!("Message buffer validation failed: {}", e)
                ));
            }
        }
        
        // Reconnection strategy setup
        if let Some(setup) = &self.hooks.reconnection_setup {
            if let Err(e) = setup() {
                return Err(LifecycleError::StartupFailed(
                    format!("Reconnection setup failed: {}", e)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Perform health check
    async fn health_check(&mut self) -> Result<bool, LifecycleError> {
        if let Some(actor_ref) = &self.actor_ref {
            match actor_ref.send(crate::actors::bridge::actors::stream::messages::GetStreamStatus).await {
                Ok(status) => {
                    self.last_health_check = Some(Instant::now());
                    self.metrics.record_health_check(true);
                    Ok(status.healthy)
                },
                Err(e) => {
                    warn!("Stream health check failed: {}", e);
                    self.metrics.record_health_check(false);
                    Err(LifecycleError::HealthCheckFailed(format!("Actor communication failed: {}", e)))
                }
            }
        } else {
            Err(LifecycleError::HealthCheckFailed("No actor reference available".to_string()))
        }
    }
}

impl ActorLifecycle for StreamLifecycle {
    type Actor = StreamActor;
    type Config = StreamConfig;
    type Error = LifecycleError;
    
    fn actor_name(&self) -> &'static str {
        "StreamActor"
    }
    
    fn current_phase(&self) -> LifecyclePhase {
        self.phase
    }
    
    fn metrics(&self) -> &LifecycleMetrics {
        &self.metrics
    }
    
    async fn start(&mut self) -> Result<Addr<Self::Actor>, Self::Error> {
        info!("Starting Stream actor lifecycle");
        self.phase = LifecyclePhase::Starting;
        self.startup_start = Some(Instant::now());
        
        // Perform startup checks
        self.stream_startup_checks().await?;
        
        // Create and start actor
        let actor = StreamActor::new(self.config.clone())
            .map_err(|e| LifecycleError::StartupFailed(format!("Actor creation failed: {}", e)))?;
        
        let addr = actor.start();
        self.actor_ref = Some(addr.clone());
        self.phase = LifecyclePhase::Running;
        
        // Record startup metrics
        if let Some(start_time) = self.startup_start {
            let startup_duration = start_time.elapsed();
            self.metrics.record_startup(startup_duration);
            info!("Stream actor started in {:?}", startup_duration);
        }
        
        Ok(addr)
    }
    
    async fn stop(&mut self) -> Result<(), Self::Error> {
        info!("Stopping Stream actor");
        self.phase = LifecyclePhase::Stopping;
        
        if let Some(actor_ref) = self.actor_ref.take() {
            // Graceful shutdown with timeout
            let shutdown_timeout = Duration::from_secs(30);
            
            match tokio::time::timeout(shutdown_timeout, async {
                actor_ref.send(actix::prelude::System::current().stop()).await
            }).await {
                Ok(_) => {
                    info!("Stream actor stopped gracefully");
                    self.phase = LifecyclePhase::Stopped;
                    self.metrics.record_shutdown();
                    Ok(())
                },
                Err(_) => {
                    warn!("Stream actor shutdown timeout, forcing stop");
                    self.phase = LifecyclePhase::Failed;
                    Err(LifecycleError::ShutdownTimeout)
                }
            }
        } else {
            warn!("No Stream actor reference to stop");
            self.phase = LifecyclePhase::Stopped;
            Ok(())
        }
    }
    
    async fn restart(&mut self) -> Result<Addr<Self::Actor>, Self::Error> {
        info!("Restarting Stream actor (attempt #{})", self.restart_count + 1);
        self.restart_count += 1;
        
        // Stop current instance
        if let Err(e) = self.stop().await {
            warn!("Error stopping Stream actor during restart: {}", e);
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