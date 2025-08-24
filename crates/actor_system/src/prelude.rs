//! Prelude module for convenient imports of the Alys actor system
//!
//! This module provides a unified interface combining core actor framework
//! capabilities with blockchain-specific extensions for the Alys V2 sidechain.

// Core actor framework re-exports
pub use crate::actor::*;
pub use crate::supervisor::*;
pub use crate::registry::*;
pub use crate::mailbox::*;
pub use crate::message::*;
pub use crate::metrics::*;
pub use crate::lifecycle::*;
pub use crate::error::*;
pub use crate::system::*;

// Actix framework essentials
pub use actix::{
    Actor, ActorContext, ActorFuture, ActorFutureExt, Addr, AsyncContext, 
    Context, ContextFutureSpawner, Handler, Message, MessageResult,
    Recipient, ResponseActFuture, ResponseFuture, Running, StreamHandler,
    Supervised, Supervisor, System, SystemService, WrapFuture
};

// Common standard library imports for actor development
pub use std::{
    collections::{HashMap, VecDeque, HashSet, BTreeMap},
    sync::{Arc, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    fmt::{Debug, Display},
    error::Error,
};

// Async/concurrency primitives
pub use tokio::{
    sync::{RwLock, Mutex, mpsc, oneshot, broadcast, Semaphore},
    time::{interval, timeout, sleep, Interval},
    task::{spawn, spawn_blocking, JoinHandle},
};

// Serialization and logging
pub use serde::{Serialize, Deserialize};
pub use tracing::{debug, error, info, warn, trace, instrument, span, Level};
pub use uuid::Uuid;

// Blockchain-specific types and constants
pub use crate::blockchain::*;

/// Result type alias for actor operations
pub type ActorResult<T> = Result<T, ActorError>;

/// Future type alias for async actor operations
pub type ActorFut<T> = ResponseActFuture<dyn Actor, ActorResult<T>>;

// Convenience macros for common actor patterns
pub use crate::actor_macros::*;