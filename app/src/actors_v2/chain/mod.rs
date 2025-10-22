//! ChainActor V2 Module
//!
//! Simplified blockchain actor that replaces both V1 ChainActor complexity and monolithic chain.rs.
//! Uses standard Actix patterns following StorageActor/NetworkActor V2 approach.
//!
//! Core features:
//! - Block production and validation
//! - AuxPoW processing and finalization
//! - Peg-in/peg-out operations
//! - Clean actor integration (StorageActor + NetworkActor V2)
//! - ChainManager interface for future EngineActor/AuxPowActor coordination
//!
//! Phase 4 features:
//! - Production-ready error recovery (recovery.rs)
//! - Performance monitoring and optimization (monitoring.rs)
//! - Health check system for all integrated actors
//! - AuxPoW block production integration

pub mod actor;
pub mod config;
pub mod error;
pub mod handlers;
pub mod messages;
pub mod metrics;
pub mod state;
pub mod withdrawals;

// Phase 4 production hardening modules
pub mod recovery;
pub mod monitoring;
pub mod auxpow;
pub mod fork_choice;

pub use actor::ChainActor;
pub use config::ChainConfig;
pub use error::ChainError;
pub use messages::{ChainMessage, ChainResponse};
pub use metrics::ChainMetrics;
pub use state::ChainState;

// Phase 4 exports
pub use recovery::HealthStatus;
pub use monitoring::{PerformanceStatus, PerformanceSummary, PerformanceMetrics};