//! Core actor framework for Alys blockchain
//! 
//! This crate provides the foundational actor system infrastructure
//! for the Alys V2 architecture, built on top of Actix.

#![warn(missing_docs)]

pub mod actor;
pub mod bus;
pub mod error;
pub mod lifecycle;
pub mod mailbox;
pub mod message;
pub mod metrics;
pub mod registry;
pub mod serialization;
pub mod supervisor;
pub mod supervisors;
pub mod system;

// Re-exports
pub use actor::*;
pub use bus::*;
pub use error::*;
pub use lifecycle::*;
pub use mailbox::*;
pub use message::*;
pub use metrics::*;
pub use registry::*;
pub use serialization::*;
pub use supervisor::*;
pub use supervisors::*;
pub use system::*;

// Actix re-exports for convenience
pub use actix::{
    Actor, ActorContext, AsyncContext, Context,
    Handler, Message, Recipient, ResponseFuture, Running,
    StreamHandler, System, SystemService, WrapFuture
};

/// Actor system version
pub const ACTOR_SYSTEM_VERSION: &str = "1.0.0";

/// Default system configuration
#[derive(Debug, Clone)]
pub struct ActorSystemConfig {
    /// System name
    pub name: String,
    /// Number of worker threads
    pub workers: Option<usize>,
    /// Enable tracing
    pub tracing: bool,
}

impl Default for ActorSystemConfig {
    fn default() -> Self {
        Self {
            name: "alys-actor-system".to_string(),
            workers: None,
            tracing: true,
        }
    }
}

/// Initialize the actor system
pub fn init_system(config: ActorSystemConfig) -> actix::SystemRunner {
    if config.tracing {
        tracing::info!("Initializing Alys actor system v{}", ACTOR_SYSTEM_VERSION);
    }
    
    // Use actix-rt System::new for basic initialization
    // The workers parameter is handled by the tokio runtime
    actix::System::new()
}