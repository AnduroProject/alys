//! Storage Actor Module
//!
//! The Storage Actor provides persistent storage for all blockchain data including
//! blocks, state, receipts, and metadata. It features:
//! 
//! - RocksDB-based persistent storage with column families
//! - Multi-level caching for performance optimization  
//! - Advanced indexing for efficient queries and lookups
//! - Batch operations for high throughput
//! - Comprehensive metrics and monitoring
//! - Maintenance operations (compaction, pruning, backup)
//! - Integration with ChainActor for block persistence

pub mod actor;
pub mod database;
pub mod cache;
pub mod indexing;
pub mod messages;
pub mod metrics;
pub mod handlers;

// Re-export main types for easy access
pub use actor::{StorageActor, StorageConfig, WritePriority};
pub use database::{DatabaseManager, DatabaseConfig};
pub use cache::{StorageCache, CacheConfig, CacheStats};
pub use indexing::{StorageIndexing, IndexingStats, TransactionIndex, AddressIndex, BlockRange};
pub use messages::*;
pub use metrics::{StorageActorMetrics, StorageAlertThresholds};