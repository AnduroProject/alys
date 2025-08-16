//! Storage actor for data persistence
//! 
//! This actor manages database operations, block storage, state persistence,
//! and provides a unified interface for all data storage needs.

use crate::messages::storage_messages::*;
use crate::types::*;
use actix::prelude::*;
use std::collections::HashMap;
use tracing::*;

/// Storage actor that manages data persistence
#[derive(Debug)]
pub struct StorageActor {
    /// Storage configuration
    config: StorageConfig,
    /// Database connections
    databases: HashMap<String, DatabaseConnection>,
    /// Cache layer
    cache: StorageCache,
    /// Pending write operations
    pending_writes: HashMap<WriteId, PendingWrite>,
    /// Storage metrics
    metrics: StorageActorMetrics,
}

/// Configuration for the storage actor
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Main database path
    pub database_path: String,
    /// Archive database path
    pub archive_path: Option<String>,
    /// Cache size in MB
    pub cache_size_mb: usize,
    /// Write batch size
    pub write_batch_size: usize,
    /// Sync frequency for writes
    pub sync_interval: std::time::Duration,
}

/// Database connection wrapper
#[derive(Debug)]
pub struct DatabaseConnection {
    pub name: String,
    pub path: String,
    pub connection_type: DatabaseType,
    // This would contain the actual database connection (RocksDB, etc.)
    pub is_connected: bool,
}

/// Type of database
#[derive(Debug, Clone)]
pub enum DatabaseType {
    Main,
    Archive,
    Index,
    State,
}

/// Storage cache layer
#[derive(Debug)]
pub struct StorageCache {
    /// Block cache
    blocks: std::collections::BTreeMap<BlockHash, ConsensusBlock>,
    /// State cache
    state: std::collections::HashMap<StateKey, StateValue>,
    /// Cache size limits
    max_blocks: usize,
    max_state_entries: usize,
    /// Cache hit/miss statistics
    block_hits: u64,
    block_misses: u64,
    state_hits: u64,
    state_misses: u64,
}

/// Unique identifier for write operations
pub type WriteId = String;

/// State key type
pub type StateKey = Vec<u8>;

/// State value type
pub type StateValue = Vec<u8>;

/// Pending write operation
#[derive(Debug, Clone)]
pub struct PendingWrite {
    pub write_id: WriteId,
    pub operation: WriteOperation,
    pub created_at: std::time::Instant,
    pub retry_count: u32,
}

/// Types of write operations
#[derive(Debug, Clone)]
pub enum WriteOperation {
    StoreBlock {
        block: ConsensusBlock,
    },
    UpdateState {
        key: StateKey,
        value: StateValue,
    },
    DeleteState {
        key: StateKey,
    },
    StoreBatch {
        operations: Vec<WriteOperation>,
    },
}

/// Storage performance metrics
#[derive(Debug, Default)]
pub struct StorageActorMetrics {
    pub blocks_stored: u64,
    pub blocks_retrieved: u64,
    pub state_updates: u64,
    pub state_queries: u64,
    pub cache_hit_rate: f64,
    pub average_write_time_ms: u64,
    pub average_read_time_ms: u64,
    pub database_size_mb: u64,
}

impl Actor for StorageActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Storage actor started with database path: {}", self.config.database_path);
        
        // Initialize database connections
        ctx.notify(InitializeDatabases);
        
        // Start periodic sync operations
        ctx.run_interval(
            self.config.sync_interval,
            |actor, _ctx| {
                actor.sync_pending_writes();
            }
        );
        
        // Start cache maintenance
        ctx.run_interval(
            std::time::Duration::from_secs(300), // 5 minutes
            |actor, _ctx| {
                actor.maintain_cache();
            }
        );
        
        // Start metrics reporting
        ctx.run_interval(
            std::time::Duration::from_secs(60),
            |actor, _ctx| {
                actor.report_metrics();
            }
        );
    }
}

impl StorageActor {
    pub fn new(config: StorageConfig) -> Self {
        let cache = StorageCache {
            blocks: std::collections::BTreeMap::new(),
            state: std::collections::HashMap::new(),
            max_blocks: 1000,
            max_state_entries: 10000,
            block_hits: 0,
            block_misses: 0,
            state_hits: 0,
            state_misses: 0,
        };

        Self {
            config: config.clone(),
            databases: HashMap::new(),
            cache,
            pending_writes: HashMap::new(),
            metrics: StorageActorMetrics::default(),
        }
    }

    /// Initialize database connections
    async fn initialize_databases(&mut self) -> Result<(), StorageError> {
        info!("Initializing database connections");
        
        // Initialize main database
        let main_db = DatabaseConnection {
            name: "main".to_string(),
            path: self.config.database_path.clone(),
            connection_type: DatabaseType::Main,
            is_connected: false,
        };
        
        // TODO: Actually open database connection (RocksDB, etc.)
        // For now, just mark as connected
        let mut main_db = main_db;
        main_db.is_connected = true;
        self.databases.insert("main".to_string(), main_db);
        
        // Initialize archive database if configured
        if let Some(archive_path) = &self.config.archive_path {
            let archive_db = DatabaseConnection {
                name: "archive".to_string(),
                path: archive_path.clone(),
                connection_type: DatabaseType::Archive,
                is_connected: true,
            };
            self.databases.insert("archive".to_string(), archive_db);
        }
        
        info!("Database connections initialized");
        Ok(())
    }

    /// Store a block in the database
    async fn store_block(&mut self, block: ConsensusBlock) -> Result<(), StorageError> {
        let block_hash = block.hash();
        info!("Storing block: {}", block_hash);
        
        let start_time = std::time::Instant::now();
        
        // Add to cache
        self.cache.blocks.insert(block_hash, block.clone());
        
        // Create write operation
        let write_id = format!("block_{}", block_hash);
        let operation = WriteOperation::StoreBlock { block };
        
        let pending_write = PendingWrite {
            write_id: write_id.clone(),
            operation,
            created_at: std::time::Instant::now(),
            retry_count: 0,
        };
        
        self.pending_writes.insert(write_id, pending_write);
        
        // TODO: Actually write to database
        // For now, just simulate the operation
        
        let write_time = start_time.elapsed();
        self.metrics.average_write_time_ms = write_time.as_millis() as u64;
        self.metrics.blocks_stored += 1;
        
        Ok(())
    }

    /// Retrieve a block from storage
    async fn get_block(&mut self, block_hash: BlockHash) -> Result<Option<ConsensusBlock>, StorageError> {
        debug!("Retrieving block: {}", block_hash);
        
        let start_time = std::time::Instant::now();
        
        // Check cache first
        if let Some(block) = self.cache.blocks.get(&block_hash) {
            self.cache.block_hits += 1;
            self.update_cache_hit_rate();
            return Ok(Some(block.clone()));
        }
        
        self.cache.block_misses += 1;
        self.update_cache_hit_rate();
        
        // TODO: Query database
        // For now, return None (not found)
        
        let read_time = start_time.elapsed();
        self.metrics.average_read_time_ms = read_time.as_millis() as u64;
        self.metrics.blocks_retrieved += 1;
        
        Ok(None)
    }

    /// Update state in storage
    async fn update_state(&mut self, key: StateKey, value: StateValue) -> Result<(), StorageError> {
        debug!("Updating state key: {:?}", key);
        
        // Update cache
        self.cache.state.insert(key.clone(), value.clone());
        
        // Create write operation
        let write_id = format!("state_{:?}", std::time::SystemTime::now());
        let operation = WriteOperation::UpdateState { key, value };
        
        let pending_write = PendingWrite {
            write_id: write_id.clone(),
            operation,
            created_at: std::time::Instant::now(),
            retry_count: 0,
        };
        
        self.pending_writes.insert(write_id, pending_write);
        self.metrics.state_updates += 1;
        
        Ok(())
    }

    /// Get state from storage
    async fn get_state(&mut self, key: StateKey) -> Result<Option<StateValue>, StorageError> {
        debug!("Querying state key: {:?}", key);
        
        // Check cache first
        if let Some(value) = self.cache.state.get(&key) {
            self.cache.state_hits += 1;
            self.update_cache_hit_rate();
            return Ok(Some(value.clone()));
        }
        
        self.cache.state_misses += 1;
        self.update_cache_hit_rate();
        
        // TODO: Query database
        // For now, return None (not found)
        
        self.metrics.state_queries += 1;
        
        Ok(None)
    }

    /// Perform batch write operations
    async fn batch_write(&mut self, operations: Vec<WriteOperation>) -> Result<(), StorageError> {
        info!("Performing batch write with {} operations", operations.len());
        
        let write_id = format!("batch_{:?}", std::time::SystemTime::now());
        let batch_operation = WriteOperation::StoreBatch { operations };
        
        let pending_write = PendingWrite {
            write_id: write_id.clone(),
            operation: batch_operation,
            created_at: std::time::Instant::now(),
            retry_count: 0,
        };
        
        self.pending_writes.insert(write_id, pending_write);
        
        Ok(())
    }

    /// Sync pending writes to database
    fn sync_pending_writes(&mut self) {
        if self.pending_writes.is_empty() {
            return;
        }
        
        debug!("Syncing {} pending write operations", self.pending_writes.len());
        
        let mut completed_writes = Vec::new();
        
        for (write_id, pending_write) in &mut self.pending_writes {
            // TODO: Actually perform the write operation
            // For now, just mark as completed after 1 second
            if pending_write.created_at.elapsed() > std::time::Duration::from_secs(1) {
                completed_writes.push(write_id.clone());
            }
        }
        
        // Remove completed writes
        for write_id in completed_writes {
            self.pending_writes.remove(&write_id);
        }
    }

    /// Maintain cache by removing old entries
    fn maintain_cache(&mut self) {
        // Maintain block cache size
        while self.cache.blocks.len() > self.cache.max_blocks {
            // Remove oldest block (BTreeMap maintains order)
            if let Some((_, _)) = self.cache.blocks.pop_first() {
                debug!("Evicted block from cache");
            }
        }
        
        // Maintain state cache size
        while self.cache.state.len() > self.cache.max_state_entries {
            // Remove arbitrary entry (HashMap doesn't maintain order)
            if let Some(key) = self.cache.state.keys().next().cloned() {
                self.cache.state.remove(&key);
                debug!("Evicted state entry from cache");
            }
        }
    }

    /// Update cache hit rate metrics
    fn update_cache_hit_rate(&mut self) {
        let total_block_accesses = self.cache.block_hits + self.cache.block_misses;
        let total_state_accesses = self.cache.state_hits + self.cache.state_misses;
        let total_accesses = total_block_accesses + total_state_accesses;
        let total_hits = self.cache.block_hits + self.cache.state_hits;
        
        if total_accesses > 0 {
            self.metrics.cache_hit_rate = (total_hits as f64) / (total_accesses as f64);
        }
    }

    /// Get storage statistics
    async fn get_stats(&self) -> StorageStats {
        StorageStats {
            blocks_stored: self.metrics.blocks_stored,
            blocks_cached: self.cache.blocks.len() as u64,
            state_entries: self.metrics.state_updates,
            state_cached: self.cache.state.len() as u64,
            cache_hit_rate: self.metrics.cache_hit_rate,
            pending_writes: self.pending_writes.len() as u64,
            database_size_mb: self.metrics.database_size_mb,
        }
    }

    /// Report storage metrics
    fn report_metrics(&self) {
        info!(
            "Storage metrics: blocks_stored={}, blocks_retrieved={}, state_updates={}, cache_hit_rate={:.2}%, pending_writes={}",
            self.metrics.blocks_stored,
            self.metrics.blocks_retrieved,
            self.metrics.state_updates,
            self.metrics.cache_hit_rate * 100.0,
            self.pending_writes.len()
        );
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub blocks_stored: u64,
    pub blocks_cached: u64,
    pub state_entries: u64,
    pub state_cached: u64,
    pub cache_hit_rate: f64,
    pub pending_writes: u64,
    pub database_size_mb: u64,
}

/// Internal message to initialize databases
#[derive(Message)]
#[rtype(result = "()")]
struct InitializeDatabases;

impl Handler<InitializeDatabases> for StorageActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, _msg: InitializeDatabases, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Initializing database connections");
            // Note: Actual implementation would call self.initialize_databases().await
        })
    }
}

// Message handlers

impl Handler<StoreBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Received store block request: {}", msg.block.hash());
            Ok(())
        })
    }
}

impl Handler<GetBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            debug!("Received get block request: {}", msg.block_hash);
            Ok(None)
        })
    }
}

impl Handler<UpdateStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: UpdateStateMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            debug!("Received state update request");
            Ok(())
        })
    }
}

impl Handler<GetStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<StateValue>, StorageError>>;

    fn handle(&mut self, msg: GetStateMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            debug!("Received state query request");
            Ok(None)
        })
    }
}

impl Handler<BatchWriteMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: BatchWriteMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            info!("Received batch write request with {} operations", msg.operations.len());
            Ok(())
        })
    }
}

impl Handler<GetStatsMessage> for StorageActor {
    type Result = ResponseFuture<StorageStats>;

    fn handle(&mut self, _msg: GetStatsMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            StorageStats {
                blocks_stored: 0,
                blocks_cached: 0,
                state_entries: 0,
                state_cached: 0,
                cache_hit_rate: 0.0,
                pending_writes: 0,
                database_size_mb: 0,
            }
        })
    }
}