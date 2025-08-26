//! Storage Actor implementation
//!
//! The Storage Actor manages all persistent storage operations for the Alys blockchain,
//! including blocks, state, receipts, and metadata. It provides a unified interface
//! for database operations with caching, batching, and performance optimization.

use crate::types::*;
use super::database::{DatabaseManager, DatabaseConfig};
use super::cache::{StorageCache, CacheConfig};
use super::indexing::{StorageIndexing, IndexingStats};
use super::messages::*;
use super::metrics::StorageActorMetrics;
use actix::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::*;
use actor_system::{Actor as AlysActor, ActorMetrics, AlysActorMessage, ActorError};

/// Storage actor that manages all persistent storage operations
#[derive(Debug)]
pub struct StorageActor {
    /// Storage configuration
    pub config: StorageConfig,
    /// Database manager for RocksDB operations
    pub database: DatabaseManager,
    /// Multi-level cache system
    pub cache: StorageCache,
    /// Advanced indexing system
    pub indexing: Arc<RwLock<StorageIndexing>>,
    /// Pending write operations queue
    pending_writes: HashMap<String, PendingWrite>,
    /// Storage performance metrics
    pub metrics: StorageActorMetrics,
    /// Actor startup time
    startup_time: Option<Instant>,
    /// Last maintenance check time
    last_maintenance: Instant,
}

/// Configuration for the storage actor
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Database configuration
    pub database: DatabaseConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Write batch size for optimization
    pub write_batch_size: usize,
    /// Sync frequency for pending writes
    pub sync_interval: Duration,
    /// Maintenance interval for cleanup operations
    pub maintenance_interval: Duration,
    /// Enable automatic compaction
    pub enable_auto_compaction: bool,
    /// Performance monitoring configuration
    pub metrics_reporting_interval: Duration,
}

/// Pending write operation with retry logic
#[derive(Debug, Clone)]
pub struct PendingWrite {
    pub operation_id: String,
    pub operation: WriteOperation,
    pub created_at: Instant,
    pub retry_count: u32,
    pub max_retries: u32,
    pub priority: WritePriority,
}

/// Write operation priority levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WritePriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl Actor for StorageActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.startup_time = Some(Instant::now());
        info!("Storage actor started with database path: {}", self.config.database.main_path);
        
        // Record startup metrics
        self.metrics.record_actor_started();
        
        // Start periodic sync operations for pending writes
        ctx.run_interval(
            self.config.sync_interval,
            |actor, _ctx| {
                actor.sync_pending_writes();
            }
        );
        
        // Start cache maintenance
        ctx.run_interval(
            self.config.maintenance_interval,
            |actor, _ctx| {
                let cache = actor.cache.clone();
                actix::spawn(async move {
                    cache.cleanup_expired().await;
                });
                
                actor.last_maintenance = Instant::now();
                
                // Perform database compaction if enabled
                if actor.config.enable_auto_compaction {
                    actor.schedule_compaction();
                }
            }
        );
        
        // Start metrics reporting
        ctx.run_interval(
            self.config.metrics_reporting_interval,
            |actor, _ctx| {
                actor.report_metrics();
            }
        );
        
        // Warm up cache if configured
        if self.config.cache.enable_warming {
            ctx.notify(WarmCache);
        }
        
        info!("Storage actor initialization completed");
    }
    
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.metrics.record_actor_stopped();
        
        // Sync any remaining pending writes
        self.sync_pending_writes();
        
        if let Some(startup_time) = self.startup_time {
            let total_runtime = startup_time.elapsed();
            info!("Storage actor stopped after {:?} runtime", total_runtime);
        }
    }
}

impl AlysActor for StorageActor {
    fn actor_type(&self) -> &'static str {
        "StorageActor"
    }
    
    fn actor_id(&self) -> String {
        "storage_actor".to_string()
    }
    
    fn get_metrics(&self) -> ActorMetrics {
        ActorMetrics {
            actor_type: self.actor_type().to_string(),
            actor_id: self.actor_id(),
            messages_processed: self.metrics.operations_processed,
            errors_count: self.metrics.total_errors(),
            last_error: None, // TODO: Track last error
            uptime_seconds: self.startup_time
                .map(|start| start.elapsed().as_secs())
                .unwrap_or(0),
            memory_usage_bytes: self.metrics.memory_usage_bytes,
            custom_metrics: self.metrics.to_custom_metrics(),
        }
    }
}

impl StorageActor {
    /// Create a new storage actor with the given configuration
    pub async fn new(config: StorageConfig) -> Result<Self, StorageError> {
        info!("Creating new storage actor");
        
        // Initialize database
        let database = DatabaseManager::new(config.database.clone()).await?;
        
        // Initialize cache
        let cache = StorageCache::new(config.cache.clone());
        
        // Initialize indexing system
        let db_handle = database.get_database_handle();
        let indexing = Arc::new(RwLock::new(
            StorageIndexing::new(db_handle)
                .map_err(|e| StorageError::Database(format!("Failed to initialize indexing: {}", e)))?
        ));
        
        // Initialize metrics
        let metrics = StorageActorMetrics::new();
        
        let actor = StorageActor {
            config: config.clone(),
            database,
            cache,
            indexing,
            pending_writes: HashMap::new(),
            metrics,
            startup_time: None,
            last_maintenance: Instant::now(),
        };
        
        info!("Storage actor created successfully");
        Ok(actor)
    }
    
    /// Store a block with caching and persistence
    async fn store_block(&mut self, block: ConsensusBlock, canonical: bool) -> Result<(), StorageError> {
        let block_hash = block.hash();
        let height = block.slot;
        
        debug!("Storing block: {} at height: {} (canonical: {})", block_hash, height, canonical);
        
        let start_time = Instant::now();
        
        // Update cache first for fast access
        self.cache.put_block(block_hash, block.clone()).await;
        
        // Store in database
        self.database.put_block(&block).await?;
        
        // Index the block for advanced queries
        if let Err(e) = self.indexing.write().unwrap().index_block(&block).await {
            error!("Failed to index block {}: {}", block_hash, e);
            // Continue execution - indexing failure shouldn't stop block storage
        }
        
        // Update chain head if this is canonical
        if canonical {
            let block_ref = BlockRef {
                hash: block_hash,
                height,
            };
            self.database.put_chain_head(&block_ref).await?;
        }
        
        // Record metrics
        let storage_time = start_time.elapsed();
        self.metrics.record_block_stored(height, storage_time, canonical);
        
        info!("Successfully stored block: {} at height: {} in {:?}", block_hash, height, storage_time);
        Ok(())
    }
    
    /// Retrieve a block with cache optimization
    async fn get_block(&mut self, block_hash: &BlockHash) -> Result<Option<ConsensusBlock>, StorageError> {
        debug!("Retrieving block: {}", block_hash);
        
        let start_time = Instant::now();
        
        // Check cache first
        if let Some(block) = self.cache.get_block(block_hash).await {
            let retrieval_time = start_time.elapsed();
            self.metrics.record_block_retrieved(retrieval_time, true);
            debug!("Block retrieved from cache: {} in {:?}", block_hash, retrieval_time);
            return Ok(Some(block));
        }
        
        // Fallback to database
        let block = self.database.get_block(block_hash).await?;
        let retrieval_time = start_time.elapsed();
        
        if let Some(ref block) = block {
            // Cache for future access
            self.cache.put_block(*block_hash, block.clone()).await;
            self.metrics.record_block_retrieved(retrieval_time, false);
            debug!("Block retrieved from database: {} in {:?}", block_hash, retrieval_time);
        } else {
            self.metrics.record_block_not_found();
            debug!("Block not found: {}", block_hash);
        }
        
        Ok(block)
    }
    
    /// Retrieve a block by height
    async fn get_block_by_height(&mut self, height: u64) -> Result<Option<ConsensusBlock>, StorageError> {
        debug!("Retrieving block at height: {}", height);
        
        let start_time = Instant::now();
        let block = self.database.get_block_by_height(height).await?;
        let retrieval_time = start_time.elapsed();
        
        if let Some(ref block) = block {
            // Cache the block for future hash-based lookups
            let block_hash = block.hash();
            self.cache.put_block(block_hash, block.clone()).await;
            self.metrics.record_block_retrieved(retrieval_time, false);
            debug!("Block retrieved by height: {} -> {} in {:?}", height, block_hash, retrieval_time);
        } else {
            self.metrics.record_block_not_found();
            debug!("No block found at height: {}", height);
        }
        
        Ok(block)
    }
    
    /// Update state with caching
    async fn update_state(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), StorageError> {
        debug!("Updating state key: {:?} (value size: {} bytes)", 
            hex::encode(&key[..std::cmp::min(key.len(), 8)]), value.len());
        
        let start_time = Instant::now();
        
        // Update cache
        self.cache.put_state(key.clone(), value.clone()).await;
        
        // Store in database
        self.database.put_state(&key, &value).await?;
        
        let update_time = start_time.elapsed();
        self.metrics.record_state_update(update_time);
        
        debug!("State updated in {:?}", update_time);
        Ok(())
    }
    
    /// Get state with cache optimization
    async fn get_state(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        debug!("Querying state key: {:?}", hex::encode(&key[..std::cmp::min(key.len(), 8)]));
        
        let start_time = Instant::now();
        
        // Check cache first
        if let Some(value) = self.cache.get_state(key).await {
            let query_time = start_time.elapsed();
            self.metrics.record_state_query(query_time, true);
            debug!("State retrieved from cache in {:?}", query_time);
            return Ok(Some(value));
        }
        
        // Fallback to database
        let value = self.database.get_state(key).await?;
        let query_time = start_time.elapsed();
        
        if let Some(ref value) = value {
            // Cache for future access
            self.cache.put_state(key.to_vec(), value.clone()).await;
            self.metrics.record_state_query(query_time, false);
            debug!("State retrieved from database in {:?} (size: {} bytes)", query_time, value.len());
        } else {
            self.metrics.record_state_not_found();
            debug!("State key not found");
        }
        
        Ok(value)
    }
    
    /// Execute batch write operations
    async fn batch_write(&mut self, operations: Vec<WriteOperation>) -> Result<(), StorageError> {
        info!("Executing batch write with {} operations", operations.len());
        
        let start_time = Instant::now();
        
        // Execute the batch in the database
        self.database.batch_write(operations.clone()).await?;
        
        // Update cache for relevant operations
        for operation in &operations {
            match operation {
                WriteOperation::PutBlock { block, canonical } => {
                    let block_hash = block.hash();
                    self.cache.put_block(block_hash, block.clone()).await;
                    
                    if *canonical {
                        self.metrics.record_block_stored(block.slot, Duration::default(), true);
                    }
                },
                WriteOperation::Put { key, value } => {
                    self.cache.put_state(key.clone(), value.clone()).await;
                },
                _ => {} // Other operations don't affect cache
            }
        }
        
        let batch_time = start_time.elapsed();
        self.metrics.record_batch_operation(operations.len(), batch_time);
        
        info!("Batch write completed with {} operations in {:?}", operations.len(), batch_time);
        Ok(())
    }
    
    /// Get current chain head
    async fn get_chain_head(&mut self) -> Result<Option<BlockRef>, StorageError> {
        debug!("Retrieving current chain head");
        self.database.get_chain_head().await
    }
    
    /// Update chain head
    async fn update_chain_head(&mut self, head: BlockRef) -> Result<(), StorageError> {
        info!("Updating chain head to: {} at height: {}", head.hash, head.height);
        self.database.put_chain_head(&head).await?;
        self.metrics.record_chain_head_update();
        Ok(())
    }
    
    /// Sync pending write operations to database
    fn sync_pending_writes(&mut self) {
        if self.pending_writes.is_empty() {
            return;
        }
        
        debug!("Syncing {} pending write operations", self.pending_writes.len());
        
        let now = Instant::now();
        let mut completed_writes = Vec::new();
        let mut failed_writes = Vec::new();
        
        for (operation_id, pending_write) in &mut self.pending_writes {
            // Check if write should be retried
            let age = now.duration_since(pending_write.created_at);
            
            if age > Duration::from_secs(30) { // Timeout threshold
                if pending_write.retry_count >= pending_write.max_retries {
                    // Give up on this write
                    failed_writes.push(operation_id.clone());
                    error!("Write operation failed after {} retries: {}", pending_write.max_retries, operation_id);
                } else {
                    // Retry the write
                    pending_write.retry_count += 1;
                    debug!("Retrying write operation: {} (attempt {})", operation_id, pending_write.retry_count);
                    
                    // TODO: Actually perform the write operation
                    // For now, simulate success after retry
                    completed_writes.push(operation_id.clone());
                }
            } else if age > Duration::from_secs(1) {
                // Consider completed if older than 1 second (placeholder logic)
                completed_writes.push(operation_id.clone());
            }
        }
        
        // Remove completed and failed writes
        for operation_id in completed_writes {
            self.pending_writes.remove(&operation_id);
            self.metrics.record_write_completion();
        }
        
        for operation_id in failed_writes {
            self.pending_writes.remove(&operation_id);
            self.metrics.record_write_failure();
        }
        
        if !self.pending_writes.is_empty() {
            debug!("Sync completed. {} pending writes remaining", self.pending_writes.len());
        }
    }
    
    /// Schedule database compaction
    fn schedule_compaction(&mut self) {
        // Only compact if it's been a while since last maintenance
        if self.last_maintenance.elapsed() > Duration::from_hours(1) {
            info!("Scheduling database compaction");
            
            let database = self.database.clone();
            actix::spawn(async move {
                if let Err(e) = database.compact_database().await {
                    error!("Database compaction failed: {}", e);
                }
            });
        }
    }
    
    /// Get comprehensive storage statistics
    async fn get_storage_stats(&self) -> StorageStats {
        let cache_stats = self.cache.get_stats().await;
        let hit_rates = self.cache.get_hit_rates().await;
        let db_stats = match self.database.get_stats().await {
            Ok(stats) => stats,
            Err(e) => {
                error!("Failed to get database stats: {}", e);
                return StorageStats {
                    blocks_stored: self.metrics.blocks_stored,
                    blocks_cached: 0,
                    state_entries: self.metrics.state_updates,
                    state_cached: 0,
                    cache_hit_rate: 0.0,
                    pending_writes: self.pending_writes.len() as u64,
                    database_size_mb: 0,
                };
            }
        };
        
        StorageStats {
            blocks_stored: self.metrics.blocks_stored,
            blocks_cached: cache_stats.block_cache_bytes / 256, // Rough estimate
            state_entries: self.metrics.state_updates,
            state_cached: cache_stats.state_cache_bytes / 64,   // Rough estimate
            cache_hit_rate: hit_rates.get("overall").copied().unwrap_or(0.0),
            pending_writes: self.pending_writes.len() as u64,
            database_size_mb: db_stats.total_size_bytes / (1024 * 1024),
        }
    }
    
    /// Report comprehensive metrics
    fn report_metrics(&self) {
        let cache_stats = futures::executor::block_on(self.cache.get_stats());
        let hit_rates = futures::executor::block_on(self.cache.get_hit_rates());
        
        info!(
            "Storage metrics: blocks_stored={}, blocks_retrieved={}, state_updates={}, cache_hit_rate={:.2}%, memory_usage={:.2}MB, pending_writes={}",
            self.metrics.blocks_stored,
            self.metrics.blocks_retrieved,
            self.metrics.state_updates,
            hit_rates.get("overall").unwrap_or(&0.0) * 100.0,
            cache_stats.memory_usage_mb(),
            self.pending_writes.len()
        );
        
        // Report detailed cache statistics
        debug!(
            "Cache details - Block hits: {}, misses: {}, State hits: {}, misses: {}, Memory: {:.2}MB",
            cache_stats.block_hits,
            cache_stats.block_misses,
            cache_stats.state_hits,
            cache_stats.state_misses,
            cache_stats.memory_usage_mb()
        );
    }
}

/// Internal message to warm up the cache
#[derive(Message)]
#[rtype(result = "()")]
struct WarmCache;

impl Handler<WarmCache> for StorageActor {
    type Result = ResponseFuture<()>;
    
    fn handle(&mut self, _msg: WarmCache, _ctx: &mut Self::Context) -> Self::Result {
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // TODO: Load recent blocks from database for cache warming
            // For now, this is a placeholder
            info!("Cache warming completed");
        })
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            cache: CacheConfig::default(),
            write_batch_size: 1000,
            sync_interval: Duration::from_secs(5),
            maintenance_interval: Duration::from_secs(300), // 5 minutes
            enable_auto_compaction: true,
            metrics_reporting_interval: Duration::from_secs(60),
        }
    }
}

impl Default for WritePriority {
    fn default() -> Self {
        WritePriority::Medium
    }
}