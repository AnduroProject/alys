//! Advanced Synchronization Engine
//!
//! This crate provides a high-performance synchronization engine for the Alys blockchain,
//! supporting both full sync and optimistic sync modes with efficient peer management,
//! state synchronization, and block downloading capabilities.

#![warn(missing_docs)]

pub mod engine;
pub mod peer;
pub mod state;
pub mod download;
pub mod verify;
pub mod storage;
pub mod protocol;
pub mod error;

// Re-exports for convenience
pub use engine::*;
pub use peer::*;
pub use state::*;
pub use download::*;
pub use verify::*;
pub use storage::*;
pub use protocol::*;
pub use error::*;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        SyncEngine, SyncConfig, SyncStatus, SyncError, SyncResult,
        PeerManager, PeerInfo, PeerStatus,
        StateSync, StateSyncConfig, StateSyncStatus,
        BlockDownloader, DownloadRequest, DownloadResult,
        BlockVerifier, VerificationResult,
        SyncStorage, SyncProtocol,
    };
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use std::collections::HashMap;
    pub use std::sync::Arc;
    pub use std::time::{Duration, SystemTime};
    pub use tokio::sync::{mpsc, oneshot, RwLock};
    pub use tracing::{debug, error, info, trace, warn};
}