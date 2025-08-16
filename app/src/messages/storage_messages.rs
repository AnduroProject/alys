//! Storage and database operation messages

use crate::types::*;
use actix::prelude::*;

/// Message to store a block in the database
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreBlockMessage {
    pub block: ConsensusBlock,
    pub canonical: bool,
}

/// Message to get a block from storage
#[derive(Message)]
#[rtype(result = "Result<Option<ConsensusBlock>, StorageError>")]
pub struct GetBlockMessage {
    pub block_hash: BlockHash,
}

/// Message to get a block by number
#[derive(Message)]
#[rtype(result = "Result<Option<ConsensusBlock>, StorageError>")]
pub struct GetBlockByNumberMessage {
    pub block_number: u64,
}

/// Message to store transaction receipt
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreReceiptMessage {
    pub receipt: TransactionReceipt,
    pub block_hash: BlockHash,
}

/// Message to get transaction receipt
#[derive(Message)]
#[rtype(result = "Result<Option<TransactionReceipt>, StorageError>")]
pub struct GetReceiptMessage {
    pub tx_hash: H256,
}

/// Message to update state in storage
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct UpdateStateMessage {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Message to get state from storage
#[derive(Message)]
#[rtype(result = "Result<Option<Vec<u8>>, StorageError>")]
pub struct GetStateMessage {
    pub key: Vec<u8>,
}

/// Message to perform batch write operations
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct BatchWriteMessage {
    pub operations: Vec<WriteOperation>,
}

/// Message to get storage statistics
#[derive(Message)]
#[rtype(result = "StorageStats")]
pub struct GetStatsMessage;

/// Message to compact database
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct CompactDatabaseMessage {
    pub database_name: String,
}

/// Message to create database snapshot
#[derive(Message)]
#[rtype(result = "Result<SnapshotInfo, StorageError>")]
pub struct CreateSnapshotMessage {
    pub snapshot_name: String,
}

/// Message to restore from snapshot
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct RestoreSnapshotMessage {
    pub snapshot_name: String,
}

/// Message to prune old data
#[derive(Message)]
#[rtype(result = "Result<PruneResult, StorageError>")]
pub struct PruneDataMessage {
    pub prune_config: PruneConfig,
}

/// Message to get chain head from storage
#[derive(Message)]
#[rtype(result = "Result<Option<BlockRef>, StorageError>")]
pub struct GetChainHeadMessage;

/// Message to update chain head in storage
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct UpdateChainHeadMessage {
    pub new_head: BlockRef,
}

/// Message to store logs
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct StoreLogsMessage {
    pub logs: Vec<EventLog>,
    pub block_hash: BlockHash,
    pub tx_hash: H256,
}

/// Message to query logs
#[derive(Message)]
#[rtype(result = "Result<Vec<EventLog>, StorageError>")]
pub struct QueryLogsMessage {
    pub filter: LogFilter,
}

/// Write operation types for batch operations
#[derive(Debug, Clone)]
pub enum WriteOperation {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
    PutBlock { block: ConsensusBlock, canonical: bool },
    PutReceipt { receipt: TransactionReceipt, block_hash: BlockHash },
    UpdateHead { head: BlockRef },
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_blocks: u64,
    pub canonical_blocks: u64,
    pub total_transactions: u64,
    pub total_receipts: u64,
    pub state_entries: u64,
    pub database_size_bytes: u64,
    pub cache_hit_rate: f64,
    pub pending_writes: u64,
}

/// Database snapshot information
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub name: String,
    pub created_at: std::time::SystemTime,
    pub size_bytes: u64,
    pub block_number: u64,
    pub state_root: Hash256,
}

/// Pruning configuration
#[derive(Debug, Clone)]
pub struct PruneConfig {
    pub keep_blocks: u64,
    pub prune_receipts: bool,
    pub prune_state: bool,
    pub prune_logs: bool,
}

/// Pruning operation result
#[derive(Debug, Clone)]
pub struct PruneResult {
    pub blocks_pruned: u64,
    pub receipts_pruned: u64,
    pub state_entries_pruned: u64,
    pub logs_pruned: u64,
    pub space_freed_bytes: u64,
}

/// Log filtering options
#[derive(Debug, Clone)]
pub struct LogFilter {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub address: Option<Vec<Address>>,
    pub topics: Option<Vec<Option<Vec<H256>>>>,
    pub limit: Option<usize>,
}

/// Event log entry
#[derive(Debug, Clone)]
pub struct EventLog {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
    pub block_hash: BlockHash,
    pub block_number: u64,
    pub transaction_hash: H256,
    pub transaction_index: u32,
    pub log_index: u32,
    pub removed: bool,
}

/// Transaction receipt
#[derive(Debug, Clone)]
pub struct TransactionReceipt {
    pub transaction_hash: H256,
    pub transaction_index: u32,
    pub block_hash: BlockHash,
    pub block_number: u64,
    pub cumulative_gas_used: u64,
    pub gas_used: u64,
    pub contract_address: Option<Address>,
    pub logs: Vec<EventLog>,
    pub logs_bloom: Vec<u8>,
    pub status: TransactionStatus,
}

/// Transaction status in receipt
#[derive(Debug, Clone)]
pub enum TransactionStatus {
    Success,
    Failed,
    Reverted { reason: Option<String> },
}

/// Database backup configuration
#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub destination: String,
    pub compress: bool,
    pub incremental: bool,
    pub include_state: bool,
}

/// Message to create database backup
#[derive(Message)]
#[rtype(result = "Result<BackupInfo, StorageError>")]
pub struct CreateBackupMessage {
    pub config: BackupConfig,
}

/// Backup information
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: String,
    pub created_at: std::time::SystemTime,
    pub size_bytes: u64,
    pub compressed: bool,
    pub checksum: String,
}

/// Storage indexing operations
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct RebuildIndexMessage {
    pub index_type: IndexType,
}

/// Types of storage indices
#[derive(Debug, Clone)]
pub enum IndexType {
    BlockByHash,
    BlockByNumber,
    TransactionByHash,
    ReceiptByHash,
    LogsByAddress,
    LogsByTopic,
    StateByKey,
}

/// Cache management messages
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct FlushCacheMessage;

/// Message to get cache statistics
#[derive(Message)]
#[rtype(result = "CacheStats")]
pub struct GetCacheStatsMessage;

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_size_bytes: u64,
    pub entry_count: u64,
    pub hit_rate: f64,
    pub eviction_count: u64,
    pub memory_usage_bytes: u64,
}

/// Archive storage operations
#[derive(Message)]
#[rtype(result = "Result<(), StorageError>")]
pub struct ArchiveBlocksMessage {
    pub from_block: u64,
    pub to_block: u64,
    pub archive_path: String,
}

/// Message to query archived data
#[derive(Message)]
#[rtype(result = "Result<Vec<ConsensusBlock>, StorageError>")]
pub struct QueryArchiveMessage {
    pub query: ArchiveQuery,
}

/// Archive query parameters
#[derive(Debug, Clone)]
pub struct ArchiveQuery {
    pub from_block: u64,
    pub to_block: u64,
    pub include_transactions: bool,
    pub include_receipts: bool,
}