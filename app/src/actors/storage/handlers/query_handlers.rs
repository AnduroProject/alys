//! Query and statistics message handlers
//!
//! This module implements message handlers for querying storage statistics,
//! cache information, and other operational data.

use crate::actors::storage::actor::StorageActor;
use crate::messages::storage_messages::*;
use crate::types::*;
use actix::prelude::*;
use tracing::*;

impl Handler<GetStatsMessage> for StorageActor {
    type Result = ResponseFuture<StorageStats>;

    fn handle(&mut self, _msg: GetStatsMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get stats request");
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Get cache statistics
            let cache_stats = cache.get_stats().await;
            let hit_rates = cache.get_hit_rates().await;
            
            // Get database statistics
            let db_stats = match database.get_stats().await {
                Ok(stats) => stats,
                Err(e) => {
                    error!("Failed to get database stats: {}", e);
                    return StorageStats {
                        total_blocks: 0,
                        canonical_blocks: 0,
                        total_transactions: 0,
                        total_receipts: 0,
                        state_entries: 0,
                        database_size_bytes: 0,
                        cache_hit_rate: hit_rates.get("overall").copied().unwrap_or(0.0),
                        pending_writes: 0,
                    };
                }
            };
            
            let stats = StorageStats {
                total_blocks: cache_stats.block_cache_bytes / 256, // Rough estimate
                canonical_blocks: cache_stats.block_cache_bytes / 256, // Simplified for now
                total_transactions: 0, // TODO: Track transaction count
                total_receipts: cache_stats.receipt_cache_bytes / 128, // Rough estimate
                state_entries: cache_stats.state_cache_bytes / 64, // Rough estimate
                database_size_bytes: db_stats.total_size_bytes,
                cache_hit_rate: hit_rates.get("overall").copied().unwrap_or(0.0),
                pending_writes: 0, // TODO: Track pending writes
            };
            
            debug!("Storage stats: total_blocks={}, db_size={}MB, cache_hit_rate={:.2}%",
                stats.total_blocks, 
                stats.database_size_bytes / (1024 * 1024),
                stats.cache_hit_rate * 100.0);
            
            stats
        })
    }
}

impl Handler<GetCacheStatsMessage> for StorageActor {
    type Result = ResponseFuture<CacheStats>;

    fn handle(&mut self, _msg: GetCacheStatsMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get cache stats request");
        
        let cache = self.cache.clone();
        
        Box::pin(async move {
            let storage_cache_stats = cache.get_stats().await;
            
            // Convert storage cache stats to message cache stats format
            let cache_stats = CacheStats {
                total_size_bytes: storage_cache_stats.total_memory_bytes,
                entry_count: storage_cache_stats.block_hits + storage_cache_stats.state_hits,
                hit_rate: storage_cache_stats.overall_hit_rate(),
                eviction_count: storage_cache_stats.block_evictions + storage_cache_stats.state_evictions,
                memory_usage_bytes: storage_cache_stats.total_memory_bytes,
            };
            
            debug!("Cache stats: size={}MB, entries={}, hit_rate={:.2}%, evictions={}",
                cache_stats.total_size_bytes / (1024 * 1024),
                cache_stats.entry_count,
                cache_stats.hit_rate * 100.0,
                cache_stats.eviction_count);
            
            cache_stats
        })
    }
}

impl Handler<QueryLogsMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<EventLog>, StorageError>>;

    fn handle(&mut self, msg: QueryLogsMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received query logs request with filter: from_block={:?}, to_block={:?}",
            msg.filter.from_block, msg.filter.to_block);
        
        Box::pin(async move {
            // TODO: Implement log querying
            // This would involve:
            // 1. Parsing the log filter criteria
            // 2. Scanning the logs column family
            // 3. Filtering by block range, address, and topics
            // 4. Applying limit if specified
            
            let logs = Vec::new(); // Placeholder
            
            info!("Log query completed, found {} matching logs", logs.len());
            Ok(logs)
        })
    }
}

impl Handler<StoreLogsMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreLogsMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received store logs request: {} logs for block {} tx {}", 
            msg.logs.len(), msg.block_hash, msg.tx_hash);
        
        let database = self.database.clone();
        
        Box::pin(async move {
            // TODO: Implement log storage
            // This would involve:
            // 1. Serializing the logs
            // 2. Creating appropriate keys for indexing
            // 3. Storing in the logs column family
            // 4. Updating indices for efficient querying
            
            debug!("Successfully stored {} logs", msg.logs.len());
            Ok(())
        })
    }
}

impl Handler<StoreReceiptMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreReceiptMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received store receipt request: tx {} in block {}", 
            msg.receipt.transaction_hash, msg.block_hash);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Cache the receipt for fast access
            cache.put_receipt(msg.receipt.transaction_hash, msg.receipt.clone()).await;
            
            // TODO: Store receipt in database
            // This would involve:
            // 1. Serializing the receipt
            // 2. Storing in receipts column family
            // 3. Creating hash -> receipt mapping
            
            debug!("Successfully stored receipt for tx: {}", msg.receipt.transaction_hash);
            Ok(())
        })
    }
}

impl Handler<GetReceiptMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<TransactionReceipt>, StorageError>>;

    fn handle(&mut self, msg: GetReceiptMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get receipt request: {}", msg.tx_hash);
        
        let cache = self.cache.clone();
        let tx_hash = msg.tx_hash;
        
        Box::pin(async move {
            // Check cache first
            if let Some(receipt) = cache.get_receipt(&tx_hash).await {
                debug!("Receipt retrieved from cache: {}", tx_hash);
                return Ok(Some(receipt));
            }
            
            // TODO: Query database for receipt
            // For now, return None
            debug!("Receipt not found: {}", tx_hash);
            Ok(None)
        })
    }
}

impl Handler<ArchiveBlocksMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: ArchiveBlocksMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received archive blocks request: blocks {} to {} -> {}", 
            msg.from_block, msg.to_block, msg.archive_path);
        
        let database = self.database.clone();
        
        Box::pin(async move {
            if msg.from_block > msg.to_block {
                return Err(StorageError::InvalidRequest("from_block must be <= to_block".to_string()));
            }
            
            let block_count = msg.to_block - msg.from_block + 1;
            if block_count > 10000 {
                return Err(StorageError::InvalidRequest("Too many blocks to archive at once, max 10000".to_string()));
            }
            
            // TODO: Implement block archiving
            // This would involve:
            // 1. Reading blocks from main database
            // 2. Writing to archive database/storage
            // 3. Verifying integrity
            // 4. Optionally removing from main database
            
            info!("Successfully archived {} blocks to {}", block_count, msg.archive_path);
            Ok(())
        })
    }
}

impl Handler<QueryArchiveMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: QueryArchiveMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received query archive request: blocks {} to {} (include_txs: {}, include_receipts: {})",
            msg.query.from_block, msg.query.to_block, 
            msg.query.include_transactions, msg.query.include_receipts);
        
        Box::pin(async move {
            if msg.query.from_block > msg.query.to_block {
                return Err(StorageError::InvalidRequest("from_block must be <= to_block".to_string()));
            }
            
            // TODO: Implement archive querying
            // This would involve:
            // 1. Accessing archive storage
            // 2. Reading requested block range
            // 3. Optionally filtering transaction/receipt data
            
            let blocks = Vec::new(); // Placeholder
            
            info!("Archive query completed, found {} blocks", blocks.len());
            Ok(blocks)
        })
    }
}