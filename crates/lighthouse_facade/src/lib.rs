//! # Lighthouse Facade
//!
//! This crate provides a unified facade interface for Lighthouse integration,
//! abstracting over both v4 and v7 implementations. It follows the Facade pattern
//! to provide a simple, consistent interface while hiding the complexity of
//! version-specific implementations and migration logic.
//!
//! ## Architecture
//!
//! The facade provides:
//! - **Unified Interface**: Single API for all Lighthouse operations
//! - **Version Abstraction**: Hides v4/v7 differences from callers
//! - **Migration Support**: Seamless migration with rollback capabilities
//! - **Error Handling**: Consistent error handling across versions
//! - **Monitoring Integration**: Built-in metrics and health monitoring
//!
//! ## Example
//!
//! ```rust,no_run
//! use lighthouse_facade::{LighthouseFacade, FacadeConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = FacadeConfig::default();
//!     let facade = LighthouseFacade::new(config).await?;
//!     
//!     // Use unified API regardless of underlying version
//!     let payload = facade.new_payload(execution_payload).await?;
//!     let forkchoice = facade.forkchoice_updated(state, attrs).await?;
//!     
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(unreachable_pub)]
#![deny(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// Core modules
pub mod facade;
pub mod config;
pub mod error;
pub mod types;
pub mod execution_layer;
pub mod compatibility;
pub mod simple_facade;
pub mod conversion;
pub mod health;
pub mod metrics;

// Re-exports for convenience
pub use crate::{
    facade::LighthouseFacade,
    simple_facade::SimpleLighthouseFacade,
    config::FacadeConfig,
    error::{FacadeError, FacadeResult},
    types::*,
    compatibility::{LighthouseCompat, MigrationMode},
    health::HealthMonitor,
    metrics::MetricsCollector,
};

// Re-export compatibility modules for backward compatibility
pub use crate::types::{
    // Basic types
    Uint256, Hash256, BlockHash, PayloadId,
    // Complex types  
    ExecutionPayload, ExecutionPayloadCapella,
    PayloadStatus, ForkchoiceState, PayloadAttributes,
    Withdrawal, FixedVector, VariableList, Transactions, Withdrawals,
    BitVector, BitList, BeaconBlockHeader, ForkName,
    // Specs
    MainnetEthSpec, EthSpec,
    // Crypto
    PublicKey, SecretKey, Signature, AggregateSignature, Keypair,
};

// Re-export Address for compatibility
pub use ethereum_types::Address;

// Module re-exports - execution_layer is already available as a module

// Compatibility module re-exports
pub mod bls {
    pub use crate::types::{PublicKey, SecretKey, Signature, AggregateSignature, Keypair};
    
    /// BLS signature set for batch verification
    #[derive(Debug, Clone)]
    pub struct SignatureSet {
        pub public_key: PublicKey,
        pub signature: Signature,
        pub message: Vec<u8>,
    }
}

pub mod sensitive_url {
    pub use crate::execution_layer::SensitiveUrl;
}

pub mod store {
    pub use crate::execution_layer::{get_key_for_col, LevelDB, MemoryStore, Store as ItemStore};
    pub use crate::types::MainnetEthSpec;
    
    /// Key-value store operation
    #[derive(Debug, Clone)]
    pub enum KeyValueStoreOp {
        PutKeyValue(Vec<u8>, Vec<u8>),
        DeleteKey(Vec<u8>),
    }
}

/// Prelude module for common imports
pub mod prelude {
    pub use crate::{
        facade::LighthouseFacade,
        config::FacadeConfig,
        error::{FacadeError, FacadeResult},
        types::*,
        compatibility::MigrationMode,
    };
}

/// Version information
pub mod version {
    /// Facade crate version
    pub const FACADE_VERSION: &str = env!("CARGO_PKG_VERSION");
}

/// Initialize the facade with logging
pub fn init() -> FacadeResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .map_err(|e| FacadeError::Internal {
            reason: format!("Failed to initialize logging: {}", e),
        })?;
        
    tracing::info!(
        "Lighthouse Facade v{} initialized", 
        version::FACADE_VERSION
    );
    
    Ok(())
}