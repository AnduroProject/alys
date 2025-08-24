//! Chain Actor Module
//!
//! This module contains the complete ChainActor implementation organized into
//! focused submodules for better maintainability and development experience.
//!
//! ## Architecture
//!
//! The chain module is organized into several key components:
//! - `actor`: Core ChainActor implementation
//! - `config`: Configuration structures and defaults
//! - `state`: Chain state management and related structures
//! - `messages`: Chain-specific message definitions
//! - `handlers`: Message handler implementations organized by functionality
//! - `supervision`: Actor supervision strategies and health monitoring
//! - `migration`: Migration utilities for backward compatibility
//! - `metrics`: Performance monitoring and metrics collection
//! - `validation`: Block and transaction validation logic
//! - `tests`: Comprehensive test suite

pub mod actor;
pub mod config;
pub mod state;
pub mod messages;
pub mod handlers;
pub mod supervision;
pub mod migration;
pub mod metrics;
pub mod validation;

#[cfg(test)]
pub mod tests;

// Re-export core types for backward compatibility
pub use actor::ChainActor;
pub use config::{ChainActorConfig, PerformanceTargets};
pub use state::{ChainState, FederationState, AuxPowState, PendingBlockInfo};
pub use messages::*;
pub use metrics::ChainActorMetrics;
pub use supervision::ChainSupervisionStrategy;
pub use migration::ChainMigrationAdapter;
pub use validation::ChainValidator;

// Re-export handler types
pub use handlers::{
    BlockHandler,
    ConsensusHandler, 
    AuxPowHandler,
    PegHandler,
};