//! EngineActor V2 - Execution Layer Coordination
//!
//! Isolates complex Engine operations behind actor interface, resolving the architectural
//! violation where ChainState directly holds V0 Engine. This actor manages execution
//! payload building, validation, and finalization while providing proper concurrency
//! isolation for resource-intensive operations.

pub mod actor;
pub mod error;
pub mod messages;
pub mod metrics;

pub use actor::EngineActor;
pub use error::EngineError;
pub use messages::{EngineMessage, EngineResponse};
pub use metrics::EngineActorMetrics;
