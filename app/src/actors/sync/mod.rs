//! Advanced SyncActor implementation for Alys V2 blockchain synchronization
//!
//! This module provides a comprehensive synchronization actor that implements:
//! - Parallel block validation with worker pools
//! - Intelligent peer selection based on performance metrics
//! - Checkpoint-based recovery system
//! - 99.5% sync threshold for block production eligibility
//! - Adaptive batch sizing based on network conditions
//! - Network partition recovery and Byzantine fault tolerance
//! - Integration with Alys federated PoA consensus and merged mining
//!
//! The SyncActor is designed to work within Alys's unique architecture where:
//! - Federation nodes use Aura PoA consensus with 2-second slot durations
//! - Merged mining provides finalization through block bundles
//! - Block production halts if no PoW is received for 10,000 blocks
//! - Governance events from Anduro stream must be processed continuously

pub mod actor;
pub mod messages;
pub mod metrics;
pub mod peer;
pub mod processor;
pub mod checkpoint;
pub mod network;
pub mod optimization;
pub mod config;
pub mod errors;

// Integration testing modules
pub mod tests;

// Re-exports for convenience
pub use actor::*;
pub use messages::*;
pub use metrics::*;
pub use peer::*;
pub use processor::*;
pub use checkpoint::*;
pub use network::*;
pub use optimization::*;
pub use config::*;
pub use errors::*;

/// Prelude module for convenient imports
pub mod prelude {
    pub use super::{
        SyncActor, SyncActorHandle, SyncConfig, SyncState, SyncStatus, SyncProgress,
        SyncMetrics, SyncError, SyncResult,
        StartSync, PauseSync, ResumeSync, GetSyncStatus, CanProduceBlocks,
        PeerManager, PeerSyncInfo, PeerScore, PeerCapabilities,
        BlockProcessor, ValidationWorker, ValidationResult,
        CheckpointManager, BlockCheckpoint, CheckpointConfig, RecoveryResult,
        NetworkMonitor, NetworkHealth, NetworkConfig,
        PerformanceOptimizer, OptimizationLevel, OptimizationType,
        SyncActorConfig, PerformanceConfig, SecurityConfig,
    };
    
    // External dependencies commonly used in sync operations
    pub use actix::prelude::*;
    pub use std::collections::{HashMap, VecDeque, HashSet};
    pub use std::sync::Arc;
    pub use std::time::{Duration, Instant, SystemTime};
    pub use tokio::sync::{RwLock, Mutex, mpsc, oneshot};
    pub use tracing::{info, warn, error, debug, trace};
    pub use serde::{Serialize, Deserialize};
    pub use uuid::Uuid;
    
    // Alys-specific types and patterns
    pub use crate::types::*;
    pub use crate::config::*;
    pub use crate::metrics::*;
    pub use actor_system::prelude::*;
}

/// SyncActor version for compatibility tracking
pub const SYNC_ACTOR_VERSION: &str = "2.0.0-beta";

/// Maximum supported protocol version for peer communication
pub const MAX_PROTOCOL_VERSION: u32 = 1;

/// Default sync configurations optimized for Alys federated consensus
pub const DEFAULT_SYNC_BATCH_SIZE: usize = 128;
pub const DEFAULT_CHECKPOINT_INTERVAL: u64 = 1000;
pub const DEFAULT_PEER_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_PRODUCTION_THRESHOLD: f64 = 0.995; // 99.5%

/// Federation-specific constants from Alys architecture
pub const AURA_SLOT_DURATION_MS: u64 = 2000; // 2-second slots
pub const MAX_BLOCKS_WITHOUT_POW: u64 = 10000; // Mining timeout
pub const FEDERATION_SIGNATURE_REQUIRED: bool = true;
pub const BLOCK_BUNDLE_FINALIZATION: bool = true;

/// Network health thresholds for partition detection
pub const MIN_PEER_COUNT: usize = 3;
pub const NETWORK_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);
pub const PARTITION_DETECTION_THRESHOLD: Duration = Duration::from_secs(120);

/// Performance optimization constants
pub const DEFAULT_VALIDATION_WORKERS: usize = 4;
pub const PARALLEL_DOWNLOAD_LIMIT: usize = 16;
pub const MEMORY_POOL_SIZE: usize = 10000;
pub const SIMD_OPTIMIZATION_ENABLED: bool = true;

/// Anduro Governance stream integration constants
pub const GOVERNANCE_EVENT_BUFFER_SIZE: usize = 1000;
pub const GOVERNANCE_STREAM_TIMEOUT: Duration = Duration::from_secs(60);
pub const FEDERATION_CONSENSUS_TIMEOUT: Duration = Duration::from_secs(10);