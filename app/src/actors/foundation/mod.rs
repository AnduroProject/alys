//! Actor System Foundation - Phase 1 Implementation
//! 
//! Enhanced actor system infrastructure for Alys V2 sidechain following
//! ALYS-006 Phase 1 requirements. Builds upon the existing actor_system crate
//! to provide blockchain-specific supervision, restart strategies, and
//! system-wide coordination for the merged mining Bitcoin sidechain.

pub mod bridge;
pub mod config;
pub mod constants;
pub mod registry;
pub mod restart_strategy;
pub mod root_supervisor;
pub mod supervision;
pub mod system_startup;
pub mod utilities;

#[cfg(test)]
pub mod tests;

// Re-exports for convenience
pub use bridge::*;
pub use config::*;
pub use constants::*;
pub use registry::*;
pub use restart_strategy::*;
pub use root_supervisor::*;
pub use supervision::*;
pub use system_startup::*;
pub use utilities::*;