//! Core Actor System Framework
//! 
//! This crate provides a high-performance actor system framework built on top of Actix,
//! designed specifically for blockchain applications. It includes supervision trees,
//! fault tolerance, message routing, and performance monitoring.

#![warn(missing_docs)]

pub mod actor;
pub mod supervisor;
pub mod registry;
pub mod message;
pub mod routing;
pub mod metrics;
pub mod error;
pub mod system;

// Re-exports for convenience
pub use actor::*;
pub use supervisor::*;
pub use registry::*;
pub use message::*;
pub use routing::*;
pub use metrics::*;
pub use error::*;
pub use system::*;

// Re-export essential actix types
pub use actix::{
    Actor, ActorContext, ActorFutureExt, ActorStreamExt, AsyncContext, Context, 
    Handler, Message, MessageResult, ResponseActFuture, ResponseFuture, 
    StreamHandler, System, SystemService, WrapFuture, WrappedStream
};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        ActorError, ActorResult, ActorMetrics, ActorRegistry, ActorSupervisor,
        MessageRouter, RestartStrategy, SupervisionStrategy, SystemManager,
        AlysActor, AlysMessage, AlysHandler, AlysContext, AlysSystem,
    };
    pub use actix::{
        Actor, ActorContext, AsyncContext, Context, Handler, Message, 
        MessageResult, ResponseFuture, System,
    };
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use std::{
        collections::HashMap,
        sync::Arc,
        time::{Duration, SystemTime},
    };
    pub use tokio::sync::{mpsc, oneshot, RwLock};
    pub use tracing::{debug, error, info, trace, warn};
    pub use uuid::Uuid;
}