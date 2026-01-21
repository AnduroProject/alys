//! RocksDB database integration for Storage Actor - V2
//!
//! This module provides the core database operations using RocksDB as the persistent
//! storage backend for blocks, state, receipts, and other blockchain data.

use super::actor::{AlysConsensusBlock, BlockRef, StorageError};
use super::messages::WriteOperation;
use crate::auxpow_miner::BlockIndex;
use crate::block::ConvertBlockHash;
use lighthouse_wrapper::types::Hash256;
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, DB};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::*;

/// Database manager for RocksDB operations
#[derive(Debug, Clone)]
pub struct DatabaseManager {
    /// Main database connection
    main_db: Arc<RwLock<DB>>,
    /// Optional archive database for old data
    archive_db: Option<Arc<RwLock<DB>>>,
    /// Column family handles
    column_families: HashMap<String, String>,
    /// Database configuration
    config: DatabaseConfig,
}

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub main_path: String,
    pub archive_path: Option<String>,
    pub cache_size_mb: usize,
    pub write_buffer_size_mb: usize,
    pub max_open_files: u32,
    pub compression_enabled: bool,
}

/// Column family names used by the storage system
pub mod column_families {
    pub const BLOCKS: &str = "blocks";
    pub const BLOCK_HEIGHTS: &str = "block_heights";
    pub const STATE: &str = "state";
    pub const RECEIPTS: &str = "receipts";
    pub const LOGS: &str = "logs";
    pub const METADATA: &str = "metadata";
    pub const CHAIN_HEAD: &str = "chain_head";
    /// Column family for storing cumulative difficulty per block height.
    /// Key: block height (u64, big-endian)
    /// Value: cumulative difficulty (u128, big-endian)
    pub const CUMULATIVE_DIFFICULTY: &str = "cumulative_difficulty";
    /// Column family for storing orphaned (non-canonical) blocks.
    /// Key: block hash
    /// Value: serialized block
    pub const ORPHANED_BLOCKS: &str = "orphaned_blocks";
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_size_bytes: u64,
    pub column_family_sizes: HashMap<String, u64>,
    pub total_keys: u64,
}

impl DatabaseManager {
    /// Create a new database manager with the given configuration
    pub async fn new(config: DatabaseConfig) -> Result<Self, StorageError> {
        info!(
            "Initializing database manager at path: {}",
            config.main_path
        );

        let main_db = Self::open_database(&config.main_path, &config)?;

        let archive_db = if let Some(archive_path) = &config.archive_path {
            info!("Opening archive database at: {}", archive_path);
            Some(Self::open_database(archive_path, &config)?)
        } else {
            None
        };

        let column_families = Self::get_column_family_names();

        Ok(DatabaseManager {
            main_db: Arc::new(RwLock::new(main_db)),
            archive_db: archive_db.map(|db| Arc::new(RwLock::new(db))),
            column_families,
            config,
        })
    }

    /// Open a RocksDB database with proper configuration
    fn open_database(path: &str, config: &DatabaseConfig) -> Result<DB, StorageError> {
        let path = Path::new(path);

        // Create directory if it doesn't exist - use std::fs since we're in blocking context
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Configure RocksDB options
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        opts.set_max_open_files(config.max_open_files as i32);
        opts.set_write_buffer_size(config.write_buffer_size_mb * 1024 * 1024);
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base((config.write_buffer_size_mb * 1024 * 1024) as u64);
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_level_zero_slowdown_writes_trigger(20);
        opts.set_level_zero_stop_writes_trigger(30);
        opts.set_max_background_jobs(4);

        if config.compression_enabled {
            opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        }

        // Configure column families
        let column_families = Self::get_column_family_descriptors(config);

        // RocksDB operations are blocking, but we're already in a blocking context
        // (app.rs wraps V2 initialization in spawn_blocking), so we can call directly
        let path_display = path.display().to_string();
        let db = DB::open_cf_descriptors(&opts, path, column_families).map_err(|e| {
            StorageError::Database(format!(
                "Failed to open database at {}: {}",
                path_display, e
            ))
        })?;

        info!("Successfully opened database at: {}", path_display);
        Ok(db)
    }

    /// Get column family descriptors with proper configuration
    fn get_column_family_descriptors(config: &DatabaseConfig) -> Vec<ColumnFamilyDescriptor> {
        let cf_names = [
            column_families::BLOCKS,
            column_families::BLOCK_HEIGHTS,
            column_families::STATE,
            column_families::RECEIPTS,
            column_families::LOGS,
            column_families::METADATA,
            column_families::CHAIN_HEAD,
            column_families::CUMULATIVE_DIFFICULTY,
            column_families::ORPHANED_BLOCKS,
        ];

        cf_names
            .iter()
            .map(|&name| {
                let mut cf_opts = Options::default();
                cf_opts.set_max_write_buffer_number(3);
                cf_opts.set_write_buffer_size(
                    config.write_buffer_size_mb * 1024 * 1024 / cf_names.len(),
                );
                cf_opts.set_target_file_size_base(64 * 1024 * 1024);

                if config.compression_enabled {
                    cf_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
                }

                ColumnFamilyDescriptor::new(name, cf_opts)
            })
            .collect()
    }

    /// Get column family names mapping
    fn get_column_family_names() -> HashMap<String, String> {
        let mut cf_map = HashMap::new();
        cf_map.insert("blocks".to_string(), column_families::BLOCKS.to_string());
        cf_map.insert(
            "block_heights".to_string(),
            column_families::BLOCK_HEIGHTS.to_string(),
        );
        cf_map.insert("state".to_string(), column_families::STATE.to_string());
        cf_map.insert(
            "receipts".to_string(),
            column_families::RECEIPTS.to_string(),
        );
        cf_map.insert("logs".to_string(), column_families::LOGS.to_string());
        cf_map.insert(
            "metadata".to_string(),
            column_families::METADATA.to_string(),
        );
        cf_map.insert(
            "chain_head".to_string(),
            column_families::CHAIN_HEAD.to_string(),
        );
        cf_map.insert(
            "cumulative_difficulty".to_string(),
            column_families::CUMULATIVE_DIFFICULTY.to_string(),
        );
        cf_map.insert(
            "orphaned_blocks".to_string(),
            column_families::ORPHANED_BLOCKS.to_string(),
        );
        cf_map
    }

    /// Store a block in the database
    pub async fn put_block(&self, block: &AlysConsensusBlock) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::BLOCKS)
            .ok_or_else(|| StorageError::Database("BLOCKS column family not found".to_string()))?;

        let block_hash = block.message.block_hash().to_block_hash();
        let key = block_hash.as_bytes();
        let value =
            serde_json::to_vec(block).map_err(|e| StorageError::Serialization(e.to_string()))?;

        db.put_cf(&cf, key, value)
            .map_err(|e| StorageError::Database(format!("Failed to store block: {}", e)))?;

        // Also store by height for efficient lookups
        let height_cf = db
            .cf_handle(column_families::BLOCK_HEIGHTS)
            .ok_or_else(|| {
                StorageError::Database("BLOCK_HEIGHTS column family not found".to_string())
            })?;

        let height_key = block.message.execution_payload.block_number.to_be_bytes();
        db.put_cf(&height_cf, &height_key, key).map_err(|e| {
            StorageError::Database(format!("Failed to store block height index: {}", e))
        })?;

        debug!(
            "Stored block {} at height {}",
            block.message.block_hash().to_block_hash(),
            block.message.execution_payload.block_number
        );
        Ok(())
    }

    /// Retrieve a block from the database by hash
    pub async fn get_block(
        &self,
        block_hash: &Hash256,
    ) -> Result<Option<AlysConsensusBlock>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::BLOCKS)
            .ok_or_else(|| StorageError::Database("BLOCKS column family not found".to_string()))?;

        let key = block_hash.as_bytes();
        match db
            .get_cf(&cf, key)
            .map_err(|e| StorageError::Database(format!("Failed to retrieve block: {}", e)))?
        {
            Some(value) => {
                let block: AlysConsensusBlock = serde_json::from_slice(&value)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    /// Retrieve a block from the database by height
    pub async fn get_block_by_height(
        &self,
        height: u64,
    ) -> Result<Option<AlysConsensusBlock>, StorageError> {
        let db = self.main_db.read().await;
        let height_cf = db
            .cf_handle(column_families::BLOCK_HEIGHTS)
            .ok_or_else(|| {
                StorageError::Database("BLOCK_HEIGHTS column family not found".to_string())
            })?;

        let height_key = height.to_be_bytes();
        match db.get_cf(&height_cf, &height_key).map_err(|e| {
            StorageError::Database(format!("Failed to retrieve block height index: {}", e))
        })? {
            Some(block_hash_bytes) => {
                // Now get the actual block
                let mut hash_bytes = [0u8; 32];
                hash_bytes.copy_from_slice(&block_hash_bytes[..32]);
                let block_hash = Hash256::from(hash_bytes);
                self.get_block(&block_hash).await
            }
            None => Ok(None),
        }
    }

    /// Store state data
    pub async fn put_state(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::STATE)
            .ok_or_else(|| StorageError::Database("STATE column family not found".to_string()))?;

        db.put_cf(&cf, key, value)
            .map_err(|e| StorageError::Database(format!("Failed to store state: {}", e)))?;

        Ok(())
    }

    /// Retrieve state data
    pub async fn get_state(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::STATE)
            .ok_or_else(|| StorageError::Database("STATE column family not found".to_string()))?;

        db.get_cf(&cf, key)
            .map_err(|e| StorageError::Database(format!("Failed to retrieve state: {}", e)))
    }

    /// Store chain head
    pub async fn put_chain_head(&self, head: &BlockRef) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db.cf_handle(column_families::CHAIN_HEAD).ok_or_else(|| {
            StorageError::Database("CHAIN_HEAD column family not found".to_string())
        })?;

        let value =
            serde_json::to_vec(head).map_err(|e| StorageError::Serialization(e.to_string()))?;

        db.put_cf(&cf, b"current", value)
            .map_err(|e| StorageError::Database(format!("Failed to store chain head: {}", e)))?;

        Ok(())
    }

    /// Retrieve chain head
    pub async fn get_chain_head(&self) -> Result<Option<BlockRef>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db.cf_handle(column_families::CHAIN_HEAD).ok_or_else(|| {
            StorageError::Database("CHAIN_HEAD column family not found".to_string())
        })?;

        match db
            .get_cf(&cf, b"current")
            .map_err(|e| StorageError::Database(format!("Failed to retrieve chain head: {}", e)))?
        {
            Some(value) => {
                let head: BlockRef = serde_json::from_slice(&value)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(head))
            }
            None => Ok(None),
        }
    }

    // ========================================================================
    // Cumulative Difficulty Storage (Gap FC-2)
    // ========================================================================

    /// Store the cumulative difficulty at a given height.
    ///
    /// This is the total proof-of-work from genesis to the block at this height.
    /// Used for "most work wins" fork choice decisions.
    pub async fn put_cumulative_difficulty(
        &self,
        height: u64,
        cumulative_difficulty: u128,
    ) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::CUMULATIVE_DIFFICULTY)
            .ok_or_else(|| {
                StorageError::Database("CUMULATIVE_DIFFICULTY column family not found".to_string())
            })?;

        let key = height.to_be_bytes();
        let value = cumulative_difficulty.to_be_bytes();

        db.put_cf(&cf, key, value).map_err(|e| {
            StorageError::Database(format!("Failed to store cumulative difficulty: {}", e))
        })?;

        debug!(
            height = height,
            cumulative_difficulty = cumulative_difficulty,
            "Stored cumulative difficulty"
        );

        Ok(())
    }

    /// Retrieve the cumulative difficulty at a given height.
    ///
    /// Returns None if no difficulty is stored at that height.
    pub async fn get_cumulative_difficulty(
        &self,
        height: u64,
    ) -> Result<Option<u128>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::CUMULATIVE_DIFFICULTY)
            .ok_or_else(|| {
                StorageError::Database("CUMULATIVE_DIFFICULTY column family not found".to_string())
            })?;

        let key = height.to_be_bytes();

        match db.get_cf(&cf, key).map_err(|e| {
            StorageError::Database(format!("Failed to retrieve cumulative difficulty: {}", e))
        })? {
            Some(bytes) => {
                if bytes.len() != 16 {
                    return Err(StorageError::Serialization(format!(
                        "Invalid cumulative difficulty length: {} (expected 16)",
                        bytes.len()
                    )));
                }
                let arr: [u8; 16] = bytes.as_slice().try_into().map_err(|_| {
                    StorageError::Serialization("Failed to convert difficulty bytes".to_string())
                })?;
                Ok(Some(u128::from_be_bytes(arr)))
            }
            None => Ok(None),
        }
    }

    /// Get the cumulative difficulty at the chain tip.
    ///
    /// Convenience method that combines get_chain_head + get_cumulative_difficulty.
    pub async fn get_tip_cumulative_difficulty(&self) -> Result<Option<u128>, StorageError> {
        let head = match self.get_chain_head().await? {
            Some(h) => h,
            None => return Ok(None),
        };

        self.get_cumulative_difficulty(head.number).await
    }

    // ========================================================================
    // Orphaned Block Storage
    // ========================================================================

    /// Store a block as orphaned (non-canonical).
    ///
    /// Used when a block loses fork choice but we want to keep it for potential future reorgs.
    pub async fn put_orphaned_block(&self, block: &AlysConsensusBlock) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::ORPHANED_BLOCKS)
            .ok_or_else(|| {
                StorageError::Database("ORPHANED_BLOCKS column family not found".to_string())
            })?;

        let block_hash = block.message.block_hash().to_block_hash();
        let key = block_hash.as_bytes();
        let value =
            serde_json::to_vec(block).map_err(|e| StorageError::Serialization(e.to_string()))?;

        db.put_cf(&cf, key, value).map_err(|e| {
            StorageError::Database(format!("Failed to store orphaned block: {}", e))
        })?;

        debug!(
            block_hash = %block_hash,
            height = block.message.execution_payload.block_number,
            "Stored orphaned block"
        );

        Ok(())
    }

    /// Retrieve an orphaned block by hash.
    pub async fn get_orphaned_block(
        &self,
        block_hash: &Hash256,
    ) -> Result<Option<AlysConsensusBlock>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::ORPHANED_BLOCKS)
            .ok_or_else(|| {
                StorageError::Database("ORPHANED_BLOCKS column family not found".to_string())
            })?;

        let key = block_hash.as_bytes();

        match db.get_cf(&cf, key).map_err(|e| {
            StorageError::Database(format!("Failed to retrieve orphaned block: {}", e))
        })? {
            Some(value) => {
                let block: AlysConsensusBlock = serde_json::from_slice(&value)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    /// Delete an orphaned block (e.g., when it becomes canonical or is too old).
    pub async fn delete_orphaned_block(&self, block_hash: &Hash256) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::ORPHANED_BLOCKS)
            .ok_or_else(|| {
                StorageError::Database("ORPHANED_BLOCKS column family not found".to_string())
            })?;

        let key = block_hash.as_bytes();

        db.delete_cf(&cf, key).map_err(|e| {
            StorageError::Database(format!("Failed to delete orphaned block: {}", e))
        })?;

        debug!(block_hash = %block_hash, "Deleted orphaned block");

        Ok(())
    }

    /// Execute batch write operations
    pub async fn batch_write(&self, operations: Vec<WriteOperation>) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let mut batch = WriteBatch::default();

        for operation in operations {
            match operation {
                WriteOperation::Put { key, value } => {
                    let cf = db.cf_handle(column_families::STATE).ok_or_else(|| {
                        StorageError::Database("STATE column family not found".to_string())
                    })?;
                    batch.put_cf(&cf, &key, &value);
                }
                WriteOperation::Delete { key } => {
                    let cf = db.cf_handle(column_families::STATE).ok_or_else(|| {
                        StorageError::Database("STATE column family not found".to_string())
                    })?;
                    batch.delete_cf(&cf, &key);
                }
                WriteOperation::PutBlock {
                    block,
                    canonical: _,
                } => {
                    let cf = db.cf_handle(column_families::BLOCKS).ok_or_else(|| {
                        StorageError::Database("BLOCKS column family not found".to_string())
                    })?;
                    let block_hash = block.message.block_hash().to_block_hash();
                    let key = block_hash.as_bytes();
                    let value = serde_json::to_vec(&block)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                    batch.put_cf(&cf, key, value);
                }
                WriteOperation::UpdateHead { head } => {
                    let cf = db.cf_handle(column_families::CHAIN_HEAD).ok_or_else(|| {
                        StorageError::Database("CHAIN_HEAD column family not found".to_string())
                    })?;
                    let value = serde_json::to_vec(&head)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                    batch.put_cf(&cf, b"current", value);
                }
                _ => {
                    warn!("Unsupported batch operation: {:?}", operation);
                }
            }
        }

        db.write(batch)
            .map_err(|e| StorageError::Database(format!("Failed to execute batch write: {}", e)))?;

        Ok(())
    }

    /// Compact the database
    pub async fn compact_database(&self) -> Result<(), StorageError> {
        let db = self.main_db.read().await;

        // Compact all column families
        for cf_name in [
            column_families::BLOCKS,
            column_families::BLOCK_HEIGHTS,
            column_families::STATE,
            column_families::RECEIPTS,
            column_families::LOGS,
            column_families::METADATA,
            column_families::CHAIN_HEAD,
            column_families::CUMULATIVE_DIFFICULTY,
            column_families::ORPHANED_BLOCKS,
        ] {
            if let Some(cf) = db.cf_handle(cf_name) {
                db.compact_range_cf(&cf, None::<&[u8]>, None::<&[u8]>);
                info!("Compacted column family: {}", cf_name);
            }
        }

        info!("Database compaction completed");
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats, StorageError> {
        let db = self.main_db.read().await;
        let mut column_family_sizes = HashMap::new();
        let mut total_size_bytes = 0u64;
        let mut total_keys = 0u64;

        for cf_name in [
            column_families::BLOCKS,
            column_families::BLOCK_HEIGHTS,
            column_families::STATE,
            column_families::RECEIPTS,
            column_families::LOGS,
            column_families::METADATA,
            column_families::CHAIN_HEAD,
            column_families::CUMULATIVE_DIFFICULTY,
            column_families::ORPHANED_BLOCKS,
        ] {
            if let Some(cf) = db.cf_handle(cf_name) {
                // Get approximate size
                if let Ok(Some(size_str)) =
                    db.property_value_cf(&cf, "rocksdb.estimate-live-data-size")
                {
                    if let Ok(size) = size_str.parse::<u64>() {
                        column_family_sizes.insert(cf_name.to_string(), size);
                        total_size_bytes += size;
                    }
                }

                // Count keys (approximate)
                if let Ok(Some(keys_str)) = db.property_value_cf(&cf, "rocksdb.estimate-num-keys") {
                    if let Ok(keys) = keys_str.parse::<u64>() {
                        total_keys += keys;
                    }
                }
            }
        }

        Ok(DatabaseStats {
            total_size_bytes,
            column_family_sizes,
            total_keys,
        })
    }

    /// Get database handle for indexing system
    pub fn get_database_handle(&self) -> Arc<RwLock<DB>> {
        self.main_db.clone()
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            main_path: "data/storage".to_string(),
            archive_path: None,
            cache_size_mb: 256,
            write_buffer_size_mb: 64,
            max_open_files: 1000,
            compression_enabled: true,
        }
    }
}
