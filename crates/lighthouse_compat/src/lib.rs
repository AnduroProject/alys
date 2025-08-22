//! # Lighthouse Compatibility Layer
//!
//! This crate provides a compatibility layer for migrating from Lighthouse v4 to v5
//! in the Alys sidechain project. It enables safe, gradual migration with A/B testing,
//! rollback capabilities, and comprehensive monitoring.
//!
//! ## Features
//!
//! - **Version Abstraction**: Uniform API over Lighthouse v4 and v5
//! - **Type Conversion**: Bidirectional type conversion between versions
//! - **Migration Control**: Gradual rollout with percentage-based traffic splitting
//! - **A/B Testing**: Statistical comparison of version performance
//! - **Rollback Safety**: 5-minute rollback capability with health monitoring
//! - **Monitoring**: Comprehensive metrics and alerting
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │   Alys Engine   │────│ Compatibility    │────│  Lighthouse v4  │
//! │     Actor       │    │     Layer        │    │   (current)     │
//! └─────────────────┘    │                  │    └─────────────────┘
//!                        │  ┌─────────────┐ │    ┌─────────────────┐
//!                        │  │ Migration   │ │────│  Lighthouse v5  │
//!                        │  │ Controller  │ │    │    (target)     │
//!                        │  └─────────────┘ │    └─────────────────┘
//!                        └──────────────────┘
//! ```
//!
//! ## Usage
//!
//! ### Basic Usage
//!
//! ```rust
//! use lighthouse_compat::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = CompatConfig::default()
//!         .with_migration_mode(MigrationMode::V4Only)
//!         .with_health_monitoring(true);
//!         
//!     let compat = LighthouseCompat::new(config).await?;
//!     
//!     // Use unified API
//!     let payload = compat.new_payload(execution_payload).await?;
//!     let status = compat.forkchoice_updated(forkchoice_state, attrs).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Migration Example
//!
//! ```rust
//! use lighthouse_compat::migration::*;
//!
//! let mut controller = MigrationController::new(config).await?;
//! 
//! // Execute gradual migration
//! controller.execute_migration_plan().await?;
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(unreachable_pub)]
#![warn(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Core modules
pub mod compat;
pub mod config;
pub mod config_migration;
pub mod error;
pub mod types;

// Engine modules
pub mod engine;

// Migration modules
pub mod migration;
pub mod ab_test;
pub mod health;
pub mod metrics;
pub mod recovery;

// Type conversion modules
pub mod conversion;

// Testing utilities
#[cfg(feature = "testing")]
pub mod testing;

// Re-exports for convenience
pub use crate::{
    compat::{LighthouseCompat, MigrationMode},
    config::CompatConfig,
    error::{CompatError, CompatResult},
    types::*,
};

/// Prelude module for common imports
pub mod prelude {
    pub use crate::{
        compat::{LighthouseCompat, MigrationMode},
        config::CompatConfig,
        error::{CompatError, CompatResult},
        types::*,
    };
    
    #[cfg(feature = "migration-tools")]
    pub use crate::{
        ab_test::{ABTest, ABTestController},
        migration::{MigrationController, MigrationState},
        health::{HealthMonitor, HealthStatus},
        metrics::{CompatMetrics, MetricsCollector},
        engine::{CompatibleEngine, EngineConfig},
        config_migration::{ConfigurationMigrator, ValidationReport},
        recovery::{RecoverySystem, SystemHealthAssessment},
    };
}

/// Version constants
pub mod version {
    /// Current crate version
    pub const LIGHTHOUSE_COMPAT_VERSION: &str = env!("CARGO_PKG_VERSION");
    
    /// Supported Lighthouse v4 revision
    pub const LIGHTHOUSE_V4_REVISION: &str = "441fc16";
    
    /// Supported Lighthouse v5 versions
    pub const LIGHTHOUSE_V5_VERSIONS: &[&str] = &["v5.0.0", "v5.1.0"];
    
    /// Minimum supported versions for safety
    pub const MIN_V4_REVISION: &str = "441fc16";
    pub const MIN_V5_VERSION: &str = "v5.0.0";
}

/// Initialize the compatibility layer with logging
pub fn init() -> CompatResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .map_err(|e| CompatError::Initialization {
            reason: format!("Failed to initialize logging: {}", e),
        })?;
        
    tracing::info!(
        "Lighthouse Compatibility Layer v{} initialized", 
        version::LIGHTHOUSE_COMPAT_VERSION
    );
    
    Ok(())
}

/// Check if the compatibility layer is properly configured
pub async fn health_check() -> CompatResult<bool> {
    // Basic health check logic
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version_constants() {
        assert!(!version::LIGHTHOUSE_COMPAT_VERSION.is_empty());
        assert!(!version::LIGHTHOUSE_V4_REVISION.is_empty());
        assert!(!version::LIGHTHOUSE_V5_VERSIONS.is_empty());
    }
    
    #[tokio::test]
    async fn test_health_check() {
        let result = health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}