//! Storage Actor messages for ALYS V2 Storage System
//!
//! This module defines the comprehensive message protocol for the StorageActor that handles
//! all persistent storage operations for the Alys blockchain including blocks, state,
//! receipts, and advanced indexing operations.

use super::actor::{AlysConsensusBlock, BlockRef, StorageError};
use super::cache::{CacheStats, TransactionReceipt};
use super::database::DatabaseStats;
use super::indexing::{AddressIndex, BlockRange, IndexType, IndexingStats, TransactionIndex};
use actix::prelude::*;
use ethereum_types::{Address, H256, U256};
use lighthouse_wrapper::types::Hash256;
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;

// =============================================================================
// BLOCK OPERATIONS
// =============================================================================

/// Message to store a block in the database with indexing
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreBlockMessage {
    /// The consensus block to store
    pub block: AlysConsensusBlock,
    /// Whether this block is part of the canonical chain
    pub canonical: bool,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get a block from storage by hash
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<AlysConsensusBlock>, StorageError>")]
pub struct GetBlockMessage {
    /// Hash of the block to retrieve
    pub block_hash: Hash256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get a block by number using indexing
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<AlysConsensusBlock>, StorageError>")]
pub struct GetBlockByHeightMessage {
    /// Height/slot number of the block
    pub height: u64,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get a range of blocks by height
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<AlysConsensusBlock>, StorageError>")]
pub struct GetBlockRangeMessage {
    /// Starting height (inclusive)
    pub start_height: u64,
    /// Ending height (inclusive)
    pub end_height: u64,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to check if a block exists
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<bool, StorageError>")]
pub struct BlockExistsMessage {
    /// Hash of the block to check
    pub block_hash: Hash256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// STATE OPERATIONS
// =============================================================================

/// Message to update state in storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct UpdateStateMessage {
    /// State key
    pub key: Vec<u8>,
    /// State value
    pub value: Vec<u8>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get state from storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<Vec<u8>>, StorageError>")]
pub struct GetStateMessage {
    /// State key to retrieve
    pub key: Vec<u8>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to perform batch write operations
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct BatchWriteMessage {
    /// List of write operations to perform atomically
    pub operations: Vec<WriteOperation>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// RECEIPT OPERATIONS
// =============================================================================

/// Message to store transaction receipt
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreReceiptMessage {
    /// Transaction receipt to store
    pub receipt: TransactionReceipt,
    /// Hash of the block containing this transaction
    pub block_hash: Hash256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get transaction receipt
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<TransactionReceipt>, StorageError>")]
pub struct GetReceiptMessage {
    /// Transaction hash
    pub tx_hash: H256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// ADVANCED QUERY OPERATIONS
// =============================================================================

/// Message to get a transaction by hash with block info
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<TransactionWithBlockInfo>, StorageError>")]
pub struct GetTransactionByHashMessage {
    /// Transaction hash to look up
    pub tx_hash: H256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get transaction history for an address
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<AddressTransactionInfo>, StorageError>")]
pub struct GetAddressTransactionsMessage {
    /// Address to query transactions for
    pub address: Address,
    /// Maximum number of transactions to return
    pub limit: Option<usize>,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to query logs with filtering
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<EventLog>, StorageError>")]
pub struct QueryLogsMessage {
    /// Log filter criteria
    pub filter: LogFilter,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to store logs
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreLogsMessage {
    /// Event logs to store
    pub logs: Vec<EventLog>,
    /// Block hash containing these logs
    pub block_hash: Hash256,
    /// Transaction hash that generated these logs
    pub tx_hash: H256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// FEE ACCUMULATION OPERATIONS (V0 Compatibility)
// =============================================================================

/// Message to get accumulated fees for a block (matches V0 storage.get_accumulated_block_fees)
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<U256>, StorageError>")]
pub struct GetAccumulatedFeesMessage {
    /// Block root hash to get accumulated fees for
    pub block_root: Hash256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to set accumulated fees for a block (matches V0 storage.set_accumulated_block_fees)
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct SetAccumulatedFeesMessage {
    /// Block root hash to set accumulated fees for
    pub block_root: Hash256,
    /// Total accumulated fees amount
    pub fees: U256,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// CHAIN HEAD OPERATIONS
// =============================================================================

/// Message to get chain head from storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<BlockRef>, StorageError>")]
pub struct GetChainHeadMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get current chain height
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<u64, StorageError>")]
pub struct GetChainHeightMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to update chain head in storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct UpdateChainHeadMessage {
    /// New chain head reference
    pub new_head: BlockRef,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// STATISTICS AND MONITORING
// =============================================================================

/// Message to get storage statistics
#[derive(Message, Debug, Clone)]
#[rtype(result = "StorageStats")]
pub struct GetStatsMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get cache statistics
#[derive(Message, Debug, Clone)]
#[rtype(result = "CacheStats")]
pub struct GetCacheStatsMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// MAINTENANCE OPERATIONS
// =============================================================================

/// Message to compact database
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct CompactDatabaseMessage {
    /// Name of the database to compact
    pub database_name: String,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to prune old data
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<PruneResult, StorageError>")]
pub struct PruneDataMessage {
    /// Pruning configuration
    pub prune_config: PruneConfig,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to create database snapshot
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<SnapshotInfo, StorageError>")]
pub struct CreateSnapshotMessage {
    /// Name for the snapshot
    pub snapshot_name: String,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to restore from snapshot
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct RestoreSnapshotMessage {
    /// Name of the snapshot to restore
    pub snapshot_name: String,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to create database backup
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<BackupInfo, StorageError>")]
pub struct CreateBackupMessage {
    /// Backup configuration
    pub config: BackupConfig,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to flush cache
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct FlushCacheMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// ADVANCED INDEXING OPERATIONS
// =============================================================================

/// Message to rebuild storage indices
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct RebuildIndexMessage {
    /// Type of index to rebuild
    pub index_type: IndexType,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to analyze database health and performance
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<DatabaseAnalysis, StorageError>")]
pub struct AnalyzeDatabaseMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to optimize database performance
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<OptimizationResult, StorageError>")]
pub struct OptimizeDatabaseMessage {
    /// Type of optimization to perform
    pub optimization_type: OptimizationType,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message for health check (Phase 4: Task 4.3.1)
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct HealthCheckMessage {
    /// Optional correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

// =============================================================================
// SUPPORTING DATA STRUCTURES
// =============================================================================

/// Write operation types for batch operations
#[derive(Debug, Clone)]
pub enum WriteOperation {
    /// Put key-value pair
    Put { key: Vec<u8>, value: Vec<u8> },
    /// Delete key
    Delete { key: Vec<u8> },
    /// Put block with canonical flag
    PutBlock {
        block: AlysConsensusBlock,
        canonical: bool,
    },
    /// Put transaction receipt
    PutReceipt {
        receipt: TransactionReceipt,
        block_hash: Hash256,
    },
    /// Update chain head
    UpdateHead { head: BlockRef },
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    /// Total number of blocks stored
    pub blocks_stored: u64,
    /// Number of blocks cached
    pub blocks_cached: u64,
    /// Number of state entries
    pub state_entries: u64,
    /// Number of state entries cached
    pub state_cached: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: f64,
    /// Number of pending write operations
    pub pending_writes: u64,
    /// Database size in MB
    pub database_size_mb: u64,
}

/// Event log entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventLog {
    /// Contract address that emitted the log
    pub address: Address,
    /// Event topics
    pub topics: Vec<H256>,
    /// Event data
    pub data: Vec<u8>,
    /// Block hash containing this log
    pub block_hash: Hash256,
    /// Block number containing this log
    pub block_number: u64,
    /// Transaction hash that generated this log
    pub transaction_hash: H256,
    /// Log index within the transaction
    pub log_index: u64,
}

/// Transaction with associated block information
#[derive(Debug, Clone)]
pub struct TransactionWithBlockInfo {
    /// Transaction hash
    pub transaction_hash: H256,
    /// Block hash containing this transaction
    pub block_hash: Hash256,
    /// Block number containing this transaction
    pub block_number: u64,
    /// Transaction index in the block
    pub transaction_index: u32,
}

/// Address transaction information
#[derive(Debug, Clone)]
pub struct AddressTransactionInfo {
    /// Transaction hash
    pub transaction_hash: H256,
    /// Block number containing the transaction
    pub block_number: u64,
    /// Transaction value
    pub value: U256,
    /// Whether the address was the sender
    pub is_sender: bool,
    /// Type of transaction
    pub transaction_type: String,
}

/// Log filtering options
#[derive(Debug, Clone)]
pub struct LogFilter {
    /// Starting block number (inclusive)
    pub from_block: Option<u64>,
    /// Ending block number (inclusive)
    pub to_block: Option<u64>,
    /// Contract address filter
    pub address: Option<Address>,
    /// Event topics to filter by
    pub topics: Vec<H256>,
    /// Maximum number of logs to return
    pub limit: Option<usize>,
}

/// Database snapshot information
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Snapshot name
    pub name: String,
    /// When the snapshot was created
    pub created_at: SystemTime,
    /// Snapshot size in bytes
    pub size_bytes: u64,
    /// Block number at snapshot time
    pub block_number: u64,
    /// State root at snapshot time
    pub state_root: Hash256,
}

/// Pruning configuration
#[derive(Debug, Clone)]
pub struct PruneConfig {
    /// Number of recent blocks to keep
    pub keep_blocks: u64,
    /// Whether to prune transaction receipts
    pub prune_receipts: bool,
    /// Whether to prune old state
    pub prune_state: bool,
    /// Whether to prune event logs
    pub prune_logs: bool,
}

/// Pruning operation result
#[derive(Debug, Clone)]
pub struct PruneResult {
    /// Number of blocks pruned
    pub blocks_pruned: u64,
    /// Number of receipts pruned
    pub receipts_pruned: u64,
    /// Number of state entries pruned
    pub state_entries_pruned: u64,
    /// Number of logs pruned
    pub logs_pruned: u64,
    /// Space freed in bytes
    pub space_freed_bytes: u64,
}

/// Database backup configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    /// Destination path for backup
    pub destination: String,
    /// Whether to compress the backup
    pub compress: bool,
    /// Whether to create incremental backup
    pub incremental: bool,
    /// Whether to include state data
    pub include_state: bool,
}

/// Backup information
#[derive(Debug, Clone)]
pub struct BackupInfo {
    /// Backup file path
    pub path: String,
    /// When backup was created
    pub created_at: SystemTime,
    /// Backup size in bytes
    pub size_bytes: u64,
    /// Whether backup is compressed
    pub compressed: bool,
    /// Backup checksum for integrity verification
    pub checksum: String,
}

/// Database analysis results
#[derive(Debug, Clone)]
pub struct DatabaseAnalysis {
    /// Total database size in bytes
    pub total_size_bytes: u64,
    /// Total number of blocks
    pub total_blocks: u64,
    /// Total number of transactions
    pub total_transactions: u64,
    /// Size of each column family
    pub column_family_sizes: HashMap<String, u64>,
    /// Index consistency issues found
    pub index_inconsistencies: Vec<String>,
    /// Database fragmentation ratio (0.0 to 1.0)
    pub fragmentation_ratio: f64,
    /// Time of last compaction
    pub last_compaction: Option<SystemTime>,
    /// Recommended maintenance actions
    pub recommended_actions: Vec<String>,
}

/// Database optimization types
#[derive(Debug, Clone)]
pub enum OptimizationType {
    /// Compact database files
    Compact,
    /// Vacuum unused space
    Vacuum,
    /// Reorganize indices for better performance
    ReorganizeIndices,
    /// Optimize cache configuration
    OptimizeCache,
    /// Perform all optimizations
    Full,
}

/// Database optimization results
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// Type of optimization performed
    pub optimization_type: OptimizationType,
    /// Space saved in bytes
    pub space_saved_bytes: u64,
    /// Time taken for optimization
    pub duration_seconds: f64,
    /// List of improvements made
    pub improvements: Vec<String>,
}

// =============================================================================
// AUXPOW DIFFICULTY PERSISTENCE OPERATIONS
// =============================================================================

/// Message to get stored difficulty history from database
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Vec<DifficultyEntry>, StorageError>")]
pub struct GetStoredDifficultyHistory {
    /// Maximum number of entries to return (None = all)
    pub limit: Option<usize>,
    /// Starting height filter (None = from beginning)
    pub start_height: Option<u64>,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to save difficulty entry to persistent storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct SaveDifficultyEntry {
    /// Difficulty entry to persist
    pub entry: DifficultyEntry,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to get last retarget height from storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<Option<u64>, StorageError>")]
pub struct GetLastRetargetHeight {
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Message to save retarget height to storage
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), StorageError>")]
pub struct SaveRetargetHeight {
    /// Height at which retargeting occurred
    pub height: u64,
    /// Correlation ID for tracing
    pub correlation_id: Option<Uuid>,
}

/// Difficulty entry for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DifficultyEntry {
    /// Block height of this difficulty entry
    pub height: u64,
    /// Timestamp when this difficulty was calculated
    pub timestamp: std::time::Duration,
    /// Difficulty target as compact bits representation
    pub bits: u32, // Simplified - in real impl would use bitcoin::CompactTarget
    /// Number of AuxPow submissions at this height
    pub auxpow_count: u32,
}
