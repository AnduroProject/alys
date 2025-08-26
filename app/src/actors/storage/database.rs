//! RocksDB database integration for Storage Actor
//!
//! This module provides the core database operations using RocksDB as the persistent
//! storage backend for blocks, state, receipts, and other blockchain data.

use crate::types::*;
use rocksdb::{DB, Options, ColumnFamily, ColumnFamilyDescriptor, WriteBatch, IteratorMode};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::*;

/// Database manager for RocksDB operations
#[derive(Debug)]
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
}

impl DatabaseManager {
    /// Create a new database manager with the given configuration
    pub async fn new(config: DatabaseConfig) -> Result<Self, StorageError> {
        info!("Initializing database manager at path: {}", config.main_path);
        
        let main_db = Self::open_database(&config.main_path, &config).await?;
        
        let archive_db = if let Some(archive_path) = &config.archive_path {
            info!("Opening archive database at: {}", archive_path);
            Some(Self::open_database(archive_path, &config).await?)
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
    async fn open_database(path: &str, config: &DatabaseConfig) -> Result<DB, StorageError> {
        let path = Path::new(path);
        
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
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
        
        let db = DB::open_cf_descriptors(&opts, path, column_families)
            .map_err(|e| StorageError::DatabaseError(format!("Failed to open database: {}", e)))?;
        
        info!("Successfully opened database at: {}", path.display());
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
        ];
        
        cf_names.iter().map(|&name| {
            let mut cf_opts = Options::default();
            cf_opts.set_max_write_buffer_number(3);
            cf_opts.set_write_buffer_size(config.write_buffer_size_mb * 1024 * 1024 / cf_names.len());
            cf_opts.set_target_file_size_base(64 * 1024 * 1024);
            
            if config.compression_enabled {
                cf_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
            }
            
            ColumnFamilyDescriptor::new(name, cf_opts)
        }).collect()
    }
    
    /// Get column family names mapping
    fn get_column_family_names() -> HashMap<String, String> {
        let mut cf_map = HashMap::new();
        cf_map.insert("blocks".to_string(), column_families::BLOCKS.to_string());
        cf_map.insert("block_heights".to_string(), column_families::BLOCK_HEIGHTS.to_string());
        cf_map.insert("state".to_string(), column_families::STATE.to_string());
        cf_map.insert("receipts".to_string(), column_families::RECEIPTS.to_string());
        cf_map.insert("logs".to_string(), column_families::LOGS.to_string());
        cf_map.insert("metadata".to_string(), column_families::METADATA.to_string());
        cf_map.insert("chain_head".to_string(), column_families::CHAIN_HEAD.to_string());
        cf_map
    }
    
    /// Store a block in the database
    pub async fn put_block(&self, block: &ConsensusBlock) -> Result<(), StorageError> {
        let block_hash = block.hash();
        debug!("Storing block: {} at height: {}", block_hash, block.slot);
        
        let db = self.main_db.read().await;
        let blocks_cf = db.cf_handle(column_families::BLOCKS)
            .ok_or_else(|| StorageError::DatabaseError("Blocks column family not found".to_string()))?;
        let heights_cf = db.cf_handle(column_families::BLOCK_HEIGHTS)
            .ok_or_else(|| StorageError::DatabaseError("Block heights column family not found".to_string()))?;
        
        // Serialize the block
        let serialized_block = serde_json::to_vec(block)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize block: {}", e)))?;
        
        // Create atomic write batch
        let mut batch = WriteBatch::default();
        
        // Store block by hash
        batch.put_cf(&blocks_cf, block_hash.as_bytes(), &serialized_block);
        
        // Store height -> hash mapping
        batch.put_cf(&heights_cf, &block.slot.to_be_bytes(), block_hash.as_bytes());
        
        // Write batch atomically
        db.write(batch)
            .map_err(|e| StorageError::DatabaseError(format!("Failed to write block: {}", e)))?;
        
        debug!("Successfully stored block: {} at height: {}", block_hash, block.slot);
        Ok(())
    }
    
    /// Retrieve a block by its hash
    pub async fn get_block(&self, block_hash: &BlockHash) -> Result<Option<ConsensusBlock>, StorageError> {
        debug!("Retrieving block: {}", block_hash);
        
        let db = self.main_db.read().await;
        let blocks_cf = db.cf_handle(column_families::BLOCKS)
            .ok_or_else(|| StorageError::DatabaseError("Blocks column family not found".to_string()))?;
        
        match db.get_cf(&blocks_cf, block_hash.as_bytes()) {
            Ok(Some(data)) => {
                let block: ConsensusBlock = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize block: {}", e)))?;
                
                debug!("Successfully retrieved block: {}", block_hash);
                Ok(Some(block))
            },
            Ok(None) => {
                debug!("Block not found: {}", block_hash);
                Ok(None)
            },
            Err(e) => {
                error!("Database error retrieving block {}: {}", block_hash, e);
                Err(StorageError::DatabaseError(format!("Failed to get block: {}", e)))
            }
        }
    }
    
    /// Retrieve a block by its height
    pub async fn get_block_by_height(&self, height: u64) -> Result<Option<ConsensusBlock>, StorageError> {
        debug!("Retrieving block at height: {}", height);
        
        let db = self.main_db.read().await;
        let heights_cf = db.cf_handle(column_families::BLOCK_HEIGHTS)
            .ok_or_else(|| StorageError::DatabaseError("Block heights column family not found".to_string()))?;
        
        // Get block hash for height
        match db.get_cf(&heights_cf, &height.to_be_bytes()) {
            Ok(Some(hash_bytes)) => {
                if hash_bytes.len() != 32 {
                    return Err(StorageError::DatabaseError("Invalid block hash length".to_string()));
                }
                
                let mut hash_array = [0u8; 32];
                hash_array.copy_from_slice(&hash_bytes);
                let block_hash = Hash256::from(hash_array);
                
                // Get the actual block
                self.get_block(&block_hash).await
            },
            Ok(None) => {
                debug!("No block found at height: {}", height);
                Ok(None)
            },
            Err(e) => {
                error!("Database error retrieving block at height {}: {}", height, e);
                Err(StorageError::DatabaseError(format!("Failed to get block by height: {}", e)))
            }
        }
    }
    
    /// Store state data
    pub async fn put_state(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        debug!("Storing state key: {:?} (length: {})", hex::encode(&key[..std::cmp::min(key.len(), 8)]), key.len());
        
        let db = self.main_db.read().await;
        let state_cf = db.cf_handle(column_families::STATE)
            .ok_or_else(|| StorageError::DatabaseError("State column family not found".to_string()))?;
        
        db.put_cf(&state_cf, key, value)
            .map_err(|e| StorageError::DatabaseError(format!("Failed to put state: {}", e)))?;
        
        debug!("Successfully stored state key");
        Ok(())
    }
    
    /// Retrieve state data
    pub async fn get_state(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        debug!("Retrieving state key: {:?}", hex::encode(&key[..std::cmp::min(key.len(), 8)]));
        
        let db = self.main_db.read().await;
        let state_cf = db.cf_handle(column_families::STATE)
            .ok_or_else(|| StorageError::DatabaseError("State column family not found".to_string()))?;
        
        match db.get_cf(&state_cf, key) {
            Ok(Some(value)) => {
                debug!("Successfully retrieved state value (length: {})", value.len());
                Ok(Some(value))
            },
            Ok(None) => {
                debug!("State key not found");
                Ok(None)
            },
            Err(e) => {
                error!("Database error retrieving state: {}", e);
                Err(StorageError::DatabaseError(format!("Failed to get state: {}", e)))
            }
        }
    }
    
    /// Store the current chain head
    pub async fn put_chain_head(&self, head: &BlockRef) -> Result<(), StorageError> {
        debug!("Updating chain head to: {} at height: {}", head.hash, head.height);
        
        let db = self.main_db.read().await;
        let head_cf = db.cf_handle(column_families::CHAIN_HEAD)
            .ok_or_else(|| StorageError::DatabaseError("Chain head column family not found".to_string()))?;
        
        let serialized_head = serde_json::to_vec(head)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize chain head: {}", e)))?;
        
        db.put_cf(&head_cf, b"current_head", &serialized_head)
            .map_err(|e| StorageError::DatabaseError(format!("Failed to update chain head: {}", e)))?;
        
        info!("Chain head updated to: {} at height: {}", head.hash, head.height);
        Ok(())
    }
    
    /// Get the current chain head
    pub async fn get_chain_head(&self) -> Result<Option<BlockRef>, StorageError> {
        debug!("Retrieving current chain head");
        
        let db = self.main_db.read().await;
        let head_cf = db.cf_handle(column_families::CHAIN_HEAD)
            .ok_or_else(|| StorageError::DatabaseError("Chain head column family not found".to_string()))?;
        
        match db.get_cf(&head_cf, b"current_head") {
            Ok(Some(data)) => {
                let head: BlockRef = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize chain head: {}", e)))?;
                
                debug!("Retrieved chain head: {} at height: {}", head.hash, head.height);
                Ok(Some(head))
            },
            Ok(None) => {
                debug!("No chain head found");
                Ok(None)
            },
            Err(e) => {
                error!("Database error retrieving chain head: {}", e);
                Err(StorageError::DatabaseError(format!("Failed to get chain head: {}", e)))
            }
        }
    }
    
    /// Execute a batch write operation
    pub async fn batch_write(&self, operations: Vec<WriteOperation>) -> Result<(), StorageError> {
        debug!("Executing batch write with {} operations", operations.len());
        
        let db = self.main_db.read().await;
        let mut batch = WriteBatch::default();
        
        for operation in operations {
            match operation {
                WriteOperation::Put { key, value } => {
                    let state_cf = db.cf_handle(column_families::STATE)
                        .ok_or_else(|| StorageError::DatabaseError("State column family not found".to_string()))?;
                    batch.put_cf(&state_cf, &key, &value);
                },
                WriteOperation::Delete { key } => {
                    let state_cf = db.cf_handle(column_families::STATE)
                        .ok_or_else(|| StorageError::DatabaseError("State column family not found".to_string()))?;
                    batch.delete_cf(&state_cf, &key);
                },
                WriteOperation::PutBlock { block, canonical: _ } => {
                    let blocks_cf = db.cf_handle(column_families::BLOCKS)
                        .ok_or_else(|| StorageError::DatabaseError("Blocks column family not found".to_string()))?;
                    let heights_cf = db.cf_handle(column_families::BLOCK_HEIGHTS)
                        .ok_or_else(|| StorageError::DatabaseError("Block heights column family not found".to_string()))?;
                    
                    let block_hash = block.hash();
                    let serialized_block = serde_json::to_vec(&block)
                        .map_err(|e| StorageError::SerializationError(format!("Failed to serialize block: {}", e)))?;
                    
                    batch.put_cf(&blocks_cf, block_hash.as_bytes(), &serialized_block);
                    batch.put_cf(&heights_cf, &block.slot.to_be_bytes(), block_hash.as_bytes());
                },
                WriteOperation::UpdateHead { head } => {
                    let head_cf = db.cf_handle(column_families::CHAIN_HEAD)
                        .ok_or_else(|| StorageError::DatabaseError("Chain head column family not found".to_string()))?;
                    
                    let serialized_head = serde_json::to_vec(&head)
                        .map_err(|e| StorageError::SerializationError(format!("Failed to serialize chain head: {}", e)))?;
                    
                    batch.put_cf(&head_cf, b"current_head", &serialized_head);
                },
                _ => {
                    warn!("Unsupported batch operation type");
                }
            }
        }
        
        db.write(batch)
            .map_err(|e| StorageError::DatabaseError(format!("Failed to execute batch write: {}", e)))?;
        
        debug!("Successfully executed batch write");
        Ok(())
    }
    
    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats, StorageError> {
        let db = self.main_db.read().await;
        
        // Get approximate sizes for column families
        let mut total_size = 0u64;
        let mut cf_sizes = HashMap::new();
        
        for cf_name in [
            column_families::BLOCKS,
            column_families::BLOCK_HEIGHTS, 
            column_families::STATE,
            column_families::RECEIPTS,
            column_families::LOGS,
            column_families::METADATA,
            column_families::CHAIN_HEAD,
        ] {
            if let Some(cf) = db.cf_handle(cf_name) {
                if let Ok(Some(size_str)) = db.property_value_cf(&cf, "rocksdb.estimate-live-data-size") {
                    if let Ok(size) = size_str.parse::<u64>() {
                        cf_sizes.insert(cf_name.to_string(), size);
                        total_size += size;
                    }
                }
            }
        }
        
        Ok(DatabaseStats {
            total_size_bytes: total_size,
            column_family_sizes: cf_sizes,
            is_archive_enabled: self.archive_db.is_some(),
        })
    }
    
    /// Compact the database to reclaim space
    pub async fn compact_database(&self) -> Result<(), StorageError> {
        info!("Starting database compaction");
        
        let db = self.main_db.read().await;
        
        // Compact each column family
        for cf_name in [
            column_families::BLOCKS,
            column_families::BLOCK_HEIGHTS,
            column_families::STATE,
            column_families::RECEIPTS,
            column_families::LOGS,
            column_families::METADATA,
            column_families::CHAIN_HEAD,
        ] {
            if let Some(cf) = db.cf_handle(cf_name) {
                info!("Compacting column family: {}", cf_name);
                db.compact_range_cf(&cf, None::<&[u8]>, None::<&[u8]>);
            }
        }
        
        info!("Database compaction completed");
        Ok(())
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_size_bytes: u64,
    pub column_family_sizes: HashMap<String, u64>,
    pub is_archive_enabled: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            main_path: "./data/storage/main".to_string(),
            archive_path: None,
            cache_size_mb: 512,
            write_buffer_size_mb: 64,
            max_open_files: 1000,
            compression_enabled: true,
        }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::DatabaseError(format!("IO error: {}", err))
    }
}