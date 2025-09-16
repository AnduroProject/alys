//! Minimal Lighthouse Facade Library
//! 
//! This provides a minimal working version that successfully integrates with real
//! Lighthouse v7 dependencies for production use while maintaining compatibility.

#![warn(missing_docs)]
#![deny(unsafe_code)]

// Essential modules only
pub mod error;
pub mod types;
pub mod simple_facade;

// Re-exports
pub use crate::{
    error::{FacadeError, FacadeResult},
    simple_facade::SimpleLighthouseFacade,
    types::*,
};

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize minimal facade
pub fn init() -> FacadeResult<()> {
    tracing::info!("Lighthouse Facade Minimal v{} initialized", VERSION);
    Ok(())
}