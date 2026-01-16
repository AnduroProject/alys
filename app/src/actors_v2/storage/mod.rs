//! Storage Actor Module - V2
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
pub mod cache;
pub mod database;
pub mod handlers;
pub mod indexing;
pub mod messages;
pub mod metrics;

#[cfg(test)]
mod tests;

// Re-export main types for easy access
pub use actor::{StorageActor, StorageConfig, WritePriority};
pub use cache::{CacheConfig, CacheStats, StorageCache};
pub use database::{DatabaseConfig, DatabaseManager};
pub use indexing::{AddressIndex, BlockRange, IndexingStats, StorageIndexing, TransactionIndex};
pub use messages::*;
pub use metrics::{StorageActorMetrics, StorageAlertThresholds};
