//! Enhanced Federation System V2
//!
//! This crate provides the next-generation federation system for Alys, featuring
//! enhanced governance integration, distributed key management, and improved
//! Bitcoin bridge operations with Anduro Governance Node compatibility.

#![warn(missing_docs)]

pub mod governance;
pub mod keyring;
pub mod bitcoin;
pub mod signatures;
pub mod utxo;
pub mod transactions;
pub mod coordinator;
pub mod protocol;
pub mod error;

// Re-exports for convenience
pub use governance::*;
pub use keyring::*;
pub use bitcoin::*;
pub use signatures::*;
pub use utxo::*;
pub use transactions::*;
pub use coordinator::*;
pub use protocol::*;
pub use error::*;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        GovernanceIntegration, GovernanceNode, GovernanceMessage,
        FederationKeyring, KeyManager, KeyShare,
        BitcoinBridge, BridgeTransaction, BridgeStatus,
        SignatureManager, MultiSignature, ThresholdSignature,
        UtxoManager, UtxoSet, UtxoEntry,
        TransactionBuilder, PegInTransaction, PegOutTransaction,
        FederationCoordinator, CoordinatorConfig,
        FederationProtocol, ProtocolMessage,
        FederationError, FederationResult,
    };
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use std::collections::HashMap;
    pub use std::sync::Arc;
    pub use std::time::{Duration, SystemTime};
    pub use tokio::sync::{mpsc, oneshot, RwLock};
    pub use tracing::{debug, error, info, trace, warn};
}

/// Federation system version
pub const FEDERATION_VERSION: &str = "2.0.0";

/// Protocol compatibility versions
pub const PROTOCOL_VERSIONS: &[&str] = &["2.0.0", "1.9.0"];

/// Default federation configuration
pub fn default_config() -> coordinator::CoordinatorConfig {
    coordinator::CoordinatorConfig::default()
}