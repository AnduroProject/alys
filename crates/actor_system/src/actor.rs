//! Core actor definitions and traits

use crate::{ActorError, ActorResult, ActorMetrics};
use actix::{Actor, Context, Handler, Message, ResponseFuture};
use async_trait::async_trait;
use std::time::{Duration, SystemTime};

/// Core trait for Alys actors
#[async_trait]
pub trait AlysActor: Actor + Send + Sync + 'static {
    /// Configuration type for this actor
    type Config: Clone + Send + Sync + 'static;
    
    /// Error type for this actor
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Create new actor instance
    fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;
    
    /// Initialize the actor
    async fn initialize(&mut self) -> Result<(), Self::Error>;
    
    /// Handle actor startup
    async fn started(&mut self) -> Result<(), Self::Error>;
    
    /// Handle actor shutdown
    async fn stopped(&mut self);
    
    /// Get actor metrics
    fn metrics(&self) -> ActorMetrics;
    
    /// Health check
    async fn health_check(&self) -> Result<bool, Self::Error>;
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