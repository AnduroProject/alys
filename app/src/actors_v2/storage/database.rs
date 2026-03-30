//! RocksDB database integration for Storage Actor - V2
//!
//! This module provides the core database operations using RocksDB as the persistent
//! storage backend for blocks, state, receipts, and other blockchain data.

use super::actor::{AlysConsensusBlock, BlockRef, StorageError};
use super::messages::WriteOperation;
use crate::auxpow_miner::BlockIndex;
use crate::block::ConvertBlockHash;
use lighthouse_wrapper::types::Hash256;
use rocksdb::{ColumnFamilyDescriptor, Options, WriteBatch, WriteOptions, DB};
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

    // ========================================================================
    // Tendermint consensus column families
    // ========================================================================

    /// Column family for storing validator sets at each effective height.
    /// Key: effective_height (u64, big-endian)
    /// Value: serialized ValidatorSet
    ///
    /// Validator set changes use H+2 rule (standard Tendermint):
    /// - Update included in block H
    /// - Effective at block H+2
    pub const VALIDATOR_SETS: &str = "validator_sets";

    /// Column family for storing governance parameter history.
    /// Key: [param_id (2 bytes)][effective_height (8 bytes BE)]
    /// Value: serialized parameter value
    ///
    /// Parameter changes use H+1 rule:
    /// - Update included in block H
    /// - Effective at block H+1
    pub const PARAMETER_HISTORY: &str = "parameter_history";

    // ========================================================================
    // Deprecated (Aura/fork-choice) column families - kept for V0 coexistence
    // ========================================================================

    /// Column family for storing cumulative difficulty per block height.
    /// Key: block height (u64, big-endian)
    /// Value: cumulative difficulty (u128, big-endian)
    ///
    /// DEPRECATED: Not used in Tendermint mode (instant finality, no forks).
    /// Kept for V0 Aura consensus coexistence.
    #[deprecated(note = "Not used in Tendermint mode - use VALIDATOR_SETS instead")]
    pub const CUMULATIVE_DIFFICULTY: &str = "cumulative_difficulty";

    /// Column family for storing orphaned (non-canonical) blocks.
    /// Key: block hash
    /// Value: serialized block
    ///
    /// DEPRECATED: Not used in Tendermint mode (no forks possible).
    /// Kept for V0 Aura consensus coexistence.
    #[deprecated(note = "Not used in Tendermint mode - Tendermint has no forks")]
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
    #[allow(deprecated)] // Allow deprecated CFs for V0 coexistence
    fn get_column_family_descriptors(config: &DatabaseConfig) -> Vec<ColumnFamilyDescriptor> {
        let cf_names = [
            column_families::BLOCKS,
            column_families::BLOCK_HEIGHTS,
            column_families::STATE,
            column_families::RECEIPTS,
            column_families::LOGS,
            column_families::METADATA,
            column_families::CHAIN_HEAD,
            // Tendermint consensus CFs
            column_families::VALIDATOR_SETS,
            column_families::PARAMETER_HISTORY,
            // Deprecated CFs (kept for V0 coexistence)
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
    #[allow(deprecated)] // Allow deprecated CFs for V0 coexistence
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
        // Tendermint consensus CFs
        cf_map.insert(
            "validator_sets".to_string(),
            column_families::VALIDATOR_SETS.to_string(),
        );
        cf_map.insert(
            "parameter_history".to_string(),
            column_families::PARAMETER_HISTORY.to_string(),
        );
        // Deprecated CFs (kept for V0 coexistence)
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

    /// Store a block in the database with sync writes for durability.
    ///
    /// Uses a WriteBatch with `set_sync(true)` to ensure both the block data
    /// and height index are atomically written and fsynced to disk. This prevents
    /// data loss on crash/SIGTERM that was causing WAL-storage mismatches.
    pub async fn put_block(&self, block: &AlysConsensusBlock) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let mut batch = WriteBatch::default();

        // 1. Block data
        let blocks_cf = db
            .cf_handle(column_families::BLOCKS)
            .ok_or_else(|| StorageError::Database("BLOCKS column family not found".to_string()))?;

        let block_hash = block.message.block_hash().to_block_hash();
        let key = block_hash.as_bytes();
        let value =
            serde_json::to_vec(block).map_err(|e| StorageError::Serialization(e.to_string()))?;

        batch.put_cf(&blocks_cf, key, &value);

        // 2. Height index for efficient lookups
        let height_cf = db
            .cf_handle(column_families::BLOCK_HEIGHTS)
            .ok_or_else(|| {
                StorageError::Database("BLOCK_HEIGHTS column family not found".to_string())
            })?;

        let height_key = block.message.execution_payload.block_number.to_be_bytes();
        batch.put_cf(&height_cf, &height_key, key);

        // 3. Write atomically with sync to prevent data loss on crash
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true);

        db.write_opt(batch, &write_opts).map_err(|e| {
            StorageError::Database(format!("Failed to store block: {}", e))
        })?;

        debug!(
            "Stored block {} at height {} (synced)",
            block_hash,
            block.message.execution_payload.block_number
        );
        Ok(())
    }

    /// Atomically commit a block with all related data.
    ///
    /// Writes the following in a single atomic batch with sync:
    /// 1. Block data (BLOCKS column family)
    /// 2. Height index (BLOCK_HEIGHTS column family)
    /// 3. Chain head (CHAIN_HEAD column family)
    ///
    /// The batch is committed with `set_sync(true)` to ensure all data
    /// survives SIGKILL. This is critical for WAL-storage consistency -
    /// without atomic sync writes, the WAL may show committed blocks while
    /// storage reports height 0 after a crash.
    ///
    /// # Arguments
    /// * `block` - The block to store
    /// * `head` - The new chain head reference
    ///
    /// # Performance
    ///
    /// Single fsync per block instead of multiple. While sync writes are
    /// slower than async writes, the atomicity guarantee is essential for
    /// crash recovery.
    pub async fn atomic_commit_block(
        &self,
        block: &AlysConsensusBlock,
        head: &BlockRef,
    ) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let mut batch = WriteBatch::default();

        // 1. Block data
        let blocks_cf = db
            .cf_handle(column_families::BLOCKS)
            .ok_or_else(|| StorageError::Database("BLOCKS column family not found".to_string()))?;

        let block_hash = block.message.block_hash().to_block_hash();
        let block_key = block_hash.as_bytes();
        let block_value = serde_json::to_vec(block)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        batch.put_cf(&blocks_cf, block_key, block_value);

        // 2. Height index
        let height_cf = db
            .cf_handle(column_families::BLOCK_HEIGHTS)
            .ok_or_else(|| StorageError::Database("BLOCK_HEIGHTS column family not found".to_string()))?;

        let height = block.message.execution_payload.block_number;
        let height_key = height.to_be_bytes();
        batch.put_cf(&height_cf, &height_key, block_key);

        // 3. Chain head
        let chain_head_cf = db
            .cf_handle(column_families::CHAIN_HEAD)
            .ok_or_else(|| StorageError::Database("CHAIN_HEAD column family not found".to_string()))?;

        let head_value = serde_json::to_vec(head)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        batch.put_cf(&chain_head_cf, b"current", head_value);

        // 4. Write atomically with sync
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true);

        db.write_opt(batch, &write_opts).map_err(|e| {
            StorageError::Database(format!("Failed to atomically commit block: {}", e))
        })?;

        debug!(
            block_hash = %block_hash,
            height = height,
            "Atomically committed block with sync"
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

    /// Find the highest block stored in the database.
    ///
    /// This is used for chain head reconstruction when the chain_head marker
    /// is missing but blocks exist in storage. Scans the BLOCK_HEIGHTS column
    /// family in reverse order to find the highest stored block.
    pub async fn find_highest_block(&self) -> Result<Option<AlysConsensusBlock>, StorageError> {
        // First, find the highest block hash (without holding references across await)
        let block_hash = {
            let db = self.main_db.read().await;
            let height_cf = db
                .cf_handle(column_families::BLOCK_HEIGHTS)
                .ok_or_else(|| {
                    StorageError::Database("BLOCK_HEIGHTS column family not found".to_string())
                })?;

            // Iterate from the end to find the highest height
            let mut iter = db.raw_iterator_cf(&height_cf);
            iter.seek_to_last();

            if iter.valid() {
                if let (Some(key), Some(value)) = (iter.key(), iter.value()) {
                    // Key is height in big-endian
                    if key.len() == 8 {
                        let height = u64::from_be_bytes(key.try_into().unwrap());
                        // Value is block hash
                        if value.len() >= 32 {
                            let mut hash_bytes = [0u8; 32];
                            hash_bytes.copy_from_slice(&value[..32]);
                            debug!(height = height, "Found highest block in database");
                            Some(Hash256::from(hash_bytes))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Now fetch the block with a fresh database read
        match block_hash {
            Some(hash) => self.get_block(&hash).await,
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

    /// Store chain head with synchronous write for crash safety.
    ///
    /// This uses sync writes to ensure the chain head survives SIGKILL (docker kill).
    /// The chain head is critical for recovery - without it, nodes lose track of
    /// their current position and may trigger unnecessary full resyncs.
    pub async fn put_chain_head(&self, head: &BlockRef) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db.cf_handle(column_families::CHAIN_HEAD).ok_or_else(|| {
            StorageError::Database("CHAIN_HEAD column family not found".to_string())
        })?;

        let value =
            serde_json::to_vec(head).map_err(|e| StorageError::Serialization(e.to_string()))?;

        // Use sync writes for chain head to ensure durability across SIGKILL.
        // This is critical for crash recovery - without a durable chain head,
        // nodes will report height 0 on restart and trigger full resync.
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true);

        db.put_cf_opt(&cf, b"current", value, &write_opts)
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
    // DEPRECATED: Not used in Tendermint mode (instant finality, no forks)
    // ========================================================================

    /// Store the cumulative difficulty at a given height.
    ///
    /// This is the total proof-of-work from genesis to the block at this height.
    /// Used for "most work wins" fork choice decisions.
    ///
    /// DEPRECATED: Not used in Tendermint mode - kept for V0 coexistence.
    #[deprecated(note = "Not used in Tendermint mode - use ValidatorSet storage instead")]
    #[allow(deprecated)]
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
    ///
    /// DEPRECATED: Not used in Tendermint mode - kept for V0 coexistence.
    #[deprecated(note = "Not used in Tendermint mode - use ValidatorSet storage instead")]
    #[allow(deprecated)]
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
    ///
    /// DEPRECATED: Not used in Tendermint mode - kept for V0 coexistence.
    #[deprecated(note = "Not used in Tendermint mode - use ValidatorSet storage instead")]
    #[allow(deprecated)]
    pub async fn get_tip_cumulative_difficulty(&self) -> Result<Option<u128>, StorageError> {
        let head = match self.get_chain_head().await? {
            Some(h) => h,
            None => return Ok(None),
        };

        self.get_cumulative_difficulty(head.number).await
    }

    // ========================================================================
    // Orphaned Block Storage
    // DEPRECATED: Not used in Tendermint mode (no forks possible)
    // ========================================================================

    /// Store a block as orphaned (non-canonical).
    ///
    /// Used when a block loses fork choice but we want to keep it for potential future reorgs.
    ///
    /// DEPRECATED: Not used in Tendermint mode (no forks) - kept for V0 coexistence.
    #[deprecated(note = "Not used in Tendermint mode - Tendermint has no forks")]
    #[allow(deprecated)]
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
    ///
    /// DEPRECATED: Not used in Tendermint mode (no forks) - kept for V0 coexistence.
    #[deprecated(note = "Not used in Tendermint mode - Tendermint has no forks")]
    #[allow(deprecated)]
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
    ///
    /// DEPRECATED: Not used in Tendermint mode (no forks) - kept for V0 coexistence.
    #[deprecated(note = "Not used in Tendermint mode - Tendermint has no forks")]
    #[allow(deprecated)]
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

    // ========================================================================
    // Tendermint Consensus Storage
    // ========================================================================

    /// Store a validator set at its effective height.
    ///
    /// Validator sets are stored by the height at which they become effective,
    /// following the Tendermint H+2 rule:
    /// - Update included in block H
    /// - Stored with key = H+2 (effective height)
    ///
    /// # Arguments
    /// * `effective_height` - The height at which this validator set becomes active
    /// * `validator_set` - The validator set to store (serialized as JSON)
    ///
    /// # Durability
    ///
    /// Uses synchronous writes to ensure the validator set is durably written
    /// before returning. This is critical for H+2 activation - without sync writes,
    /// a race condition can occur where height H+2 initializes before the new
    /// validator set is visible, causing consensus to use the wrong validator set
    /// and potentially stalling the network.
    pub async fn put_validator_set(
        &self,
        effective_height: u64,
        validator_set: &crate::actors_v2::chain::tendermint::ValidatorSet,
    ) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::VALIDATOR_SETS)
            .ok_or_else(|| {
                StorageError::Database("VALIDATOR_SETS column family not found".to_string())
            })?;

        let key = effective_height.to_be_bytes();
        let value = serde_json::to_vec(validator_set)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        // Use sync writes to ensure the validator set is visible immediately.
        // Without this, a race condition can occur where load_validator_set_for_height()
        // is called before the async write is flushed, causing height H+2 to initialize
        // with the old validator set and breaking consensus.
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true);

        db.put_cf_opt(&cf, key, value, &write_opts).map_err(|e| {
            StorageError::Database(format!("Failed to store validator set: {}", e))
        })?;

        debug!(
            effective_height = effective_height,
            validator_count = validator_set.len(),
            "Stored validator set (synced)"
        );

        Ok(())
    }

    /// Retrieve the validator set that was active at a given height.
    ///
    /// This performs a reverse lookup to find the most recent validator set
    /// that was effective at or before the given height.
    ///
    /// # Arguments
    /// * `height` - The height to query
    ///
    /// # Returns
    /// The validator set that was active at that height, or None if no validator set
    /// has been stored (e.g., before genesis).
    pub async fn get_validator_set_for_height(
        &self,
        height: u64,
    ) -> Result<Option<crate::actors_v2::chain::tendermint::ValidatorSet>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::VALIDATOR_SETS)
            .ok_or_else(|| {
                StorageError::Database("VALIDATOR_SETS column family not found".to_string())
            })?;

        // Search backwards from height to find the most recent validator set
        // Using iterator in reverse to find the most recent entry <= height
        let mut iter = db.raw_iterator_cf(&cf);
        let search_key = height.to_be_bytes();
        iter.seek_for_prev(&search_key);

        if iter.valid() {
            if let Some(value) = iter.value() {
                let validator_set: crate::actors_v2::chain::tendermint::ValidatorSet =
                    serde_json::from_slice(value)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                return Ok(Some(validator_set));
            }
        }

        Ok(None)
    }

    /// Retrieve the validator set stored at an exact effective height.
    ///
    /// Unlike `get_validator_set_for_height`, this only returns a validator set
    /// if there was a change at exactly that height.
    pub async fn get_validator_set_at_height(
        &self,
        effective_height: u64,
    ) -> Result<Option<crate::actors_v2::chain::tendermint::ValidatorSet>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::VALIDATOR_SETS)
            .ok_or_else(|| {
                StorageError::Database("VALIDATOR_SETS column family not found".to_string())
            })?;

        let key = effective_height.to_be_bytes();

        match db.get_cf(&cf, key).map_err(|e| {
            StorageError::Database(format!("Failed to retrieve validator set: {}", e))
        })? {
            Some(value) => {
                let validator_set: crate::actors_v2::chain::tendermint::ValidatorSet =
                    serde_json::from_slice(&value)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(validator_set))
            }
            None => Ok(None),
        }
    }

    /// Store a governance parameter update at its effective height.
    ///
    /// Parameters are stored by param_id and effective height, following
    /// the H+1 rule:
    /// - Update included in block H
    /// - Stored with key = [param_id][H+1] (effective height)
    pub async fn put_parameter_update(
        &self,
        param_id: crate::actors_v2::chain::tendermint::GovernableParam,
        effective_height: u64,
        value: &[u8],
    ) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::PARAMETER_HISTORY)
            .ok_or_else(|| {
                StorageError::Database("PARAMETER_HISTORY column family not found".to_string())
            })?;

        // Key format: [param_id (2 bytes)][effective_height (8 bytes BE)]
        let mut key = Vec::with_capacity(10);
        key.extend_from_slice(&(param_id as u16).to_be_bytes());
        key.extend_from_slice(&effective_height.to_be_bytes());

        db.put_cf(&cf, &key, value).map_err(|e| {
            StorageError::Database(format!("Failed to store parameter update: {}", e))
        })?;

        debug!(
            param_id = ?param_id,
            effective_height = effective_height,
            "Stored parameter update"
        );

        Ok(())
    }

    /// Retrieve a governance parameter value at a given height.
    ///
    /// This performs a reverse lookup to find the most recent parameter value
    /// that was effective at or before the given height.
    pub async fn get_parameter_at_height(
        &self,
        param_id: crate::actors_v2::chain::tendermint::GovernableParam,
        height: u64,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let db = self.main_db.read().await;
        let cf = db
            .cf_handle(column_families::PARAMETER_HISTORY)
            .ok_or_else(|| {
                StorageError::Database("PARAMETER_HISTORY column family not found".to_string())
            })?;

        // Build the search key for this param at the given height
        let mut search_key = Vec::with_capacity(10);
        search_key.extend_from_slice(&(param_id as u16).to_be_bytes());
        search_key.extend_from_slice(&height.to_be_bytes());

        // Build the prefix for this param (to ensure we don't read other params)
        let mut prefix = Vec::with_capacity(2);
        prefix.extend_from_slice(&(param_id as u16).to_be_bytes());

        let mut iter = db.raw_iterator_cf(&cf);
        iter.seek_for_prev(&search_key);

        if iter.valid() {
            if let (Some(key), Some(value)) = (iter.key(), iter.value()) {
                // Check that the key starts with our param prefix
                if key.starts_with(&prefix) {
                    return Ok(Some(value.to_vec()));
                }
            }
        }

        Ok(None)
    }

    /// Execute batch write operations
    ///
    /// Uses synchronous writes when the batch includes a chain head update
    /// to ensure durability across SIGKILL.
    pub async fn batch_write(&self, operations: Vec<WriteOperation>) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        let mut batch = WriteBatch::default();
        let mut has_head_update = false;

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
                    has_head_update = true;
                }
                _ => {
                    warn!("Unsupported batch operation: {:?}", operation);
                }
            }
        }

        // Use sync writes when updating chain head to ensure durability
        if has_head_update {
            let mut write_opts = WriteOptions::default();
            write_opts.set_sync(true);
            db.write_opt(batch, &write_opts)
                .map_err(|e| StorageError::Database(format!("Failed to execute batch write: {}", e)))?;
        } else {
            db.write(batch)
                .map_err(|e| StorageError::Database(format!("Failed to execute batch write: {}", e)))?;
        }

        Ok(())
    }

    /// Compact the database
    #[allow(deprecated)] // Allow deprecated CFs for V0 coexistence
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
            column_families::VALIDATOR_SETS,
            column_families::PARAMETER_HISTORY,
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
    #[allow(deprecated)] // Allow deprecated CFs for V0 coexistence
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
            column_families::VALIDATOR_SETS,
            column_families::PARAMETER_HISTORY,
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

    /// Flush all memtables to disk.
    ///
    /// This ensures all in-memory data is persisted to SST files.
    /// Should be called during graceful shutdown to prevent data loss.
    ///
    /// Note: This is a blocking operation intended for use in sync contexts
    /// (like Actor::stopped()). For async contexts, use `flush_async()`.
    pub fn flush(&self) -> Result<(), StorageError> {
        // Use try_read to avoid deadlocks in sync contexts
        // If we can't acquire the lock, the database is busy and data should be safe
        match self.main_db.try_read() {
            Ok(db) => {
                db.flush().map_err(|e| {
                    StorageError::Database(format!("Failed to flush database: {}", e))
                })?;
                info!("Database flushed successfully");
                Ok(())
            }
            Err(_) => {
                warn!("Could not acquire database lock for flush - database busy");
                Ok(()) // Not an error, just couldn't flush right now
            }
        }
    }

    /// Flush the write-ahead log (WAL) to disk.
    ///
    /// This is a lighter-weight operation than full flush - it syncs the WAL
    /// without forcing memtables to SST files. Good for periodic background syncing.
    ///
    /// Note: This is a blocking operation. Use sparingly in async contexts.
    pub fn flush_wal(&self) -> Result<(), StorageError> {
        match self.main_db.try_read() {
            Ok(db) => {
                // sync=true ensures WAL is durably written to disk
                db.flush_wal(true).map_err(|e| {
                    StorageError::Database(format!("Failed to flush WAL: {}", e))
                })?;
                debug!("WAL flushed successfully");
                Ok(())
            }
            Err(_) => {
                // Lock contention - skip this flush cycle
                Ok(())
            }
        }
    }

    /// Async version of flush for use in async contexts.
    pub async fn flush_async(&self) -> Result<(), StorageError> {
        let db = self.main_db.read().await;
        db.flush().map_err(|e| {
            StorageError::Database(format!("Failed to flush database: {}", e))
        })?;
        info!("Database flushed successfully (async)");
        Ok(())
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
