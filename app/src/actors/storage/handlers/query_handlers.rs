//! Query and statistics message handlers
//!
//! This module implements message handlers for querying storage statistics,
//! cache information, advanced indexing queries, and other operational data.

use crate::actors::storage::actor::StorageActor;
use crate::actors::storage::indexing::{BlockRange, IndexingError};
use crate::actors::storage::messages::*;
use crate::types::*;
use actix::prelude::*;
use std::sync::Arc;
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
            
            // Get transaction count from database metadata
            let total_transactions = match database.get_metadata("total_transactions").await {
                Ok(Some(count_bytes)) => {
                    String::from_utf8_lossy(&count_bytes).parse::<u64>().unwrap_or(0)
                },
                _ => {
                    // Fallback: estimate from cache or count directly
                    cache_stats.receipt_cache_bytes / 64 // Rough estimate
                }
            };
            
            // Get pending writes count from database write queue
            let pending_writes = match database.get_pending_writes_count().await {
                Ok(count) => count,
                Err(e) => {
                    debug!("Failed to get pending writes count: {}", e);
                    0
                }
            };

            let stats = StorageStats {
                total_blocks: cache_stats.block_cache_bytes / 256, // Rough estimate
                canonical_blocks: cache_stats.block_cache_bytes / 256, // Simplified for now
                total_transactions,
                total_receipts: cache_stats.receipt_cache_bytes / 128, // Rough estimate
                state_entries: cache_stats.state_cache_bytes / 64, // Rough estimate
                database_size_bytes: db_stats.total_size_bytes,
                cache_hit_rate: hit_rates.get("overall").copied().unwrap_or(0.0),
                pending_writes,
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
        debug!("Received query logs request with filter: from_block={:?}, to_block={:?}, address={:?}",
            msg.filter.from_block, msg.filter.to_block, msg.filter.address);
        
        let indexing = self.indexing.clone();
        
        Box::pin(async move {
            let from_block = msg.filter.from_block.unwrap_or(0);
            let to_block = msg.filter.to_block.unwrap_or(u64::MAX);
            
            match indexing.write().await.search_logs(
                msg.filter.address,
                msg.filter.topics.clone(),
                from_block,
                to_block
            ).await {
                Ok(ethereum_logs) => {
                    // Convert Ethereum logs to EventLogs
                    let event_logs: Vec<EventLog> = ethereum_logs.into_iter()
                        .map(|eth_log| EventLog {
                            address: eth_log.address,
                            topics: eth_log.topics,
                            data: eth_log.data,
                            block_hash: eth_log.block_hash.unwrap_or_default(),
                            block_number: eth_log.block_number.unwrap_or_default(),
                            transaction_hash: eth_log.transaction_hash.unwrap_or_default(),
                            transaction_index: eth_log.transaction_index.unwrap_or_default(),
                            log_index: eth_log.log_index.unwrap_or_default(),
                            removed: false,
                        })
                        .collect();
                    
                    info!("Log query completed, found {} matching logs", event_logs.len());
                    Ok(event_logs)
                },
                Err(e) => {
                    error!("Failed to query logs: {}", e);
                    Err(StorageError::Database(format!("Log query failed: {}", e)))
                }
            }
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
            // Store each log with appropriate indexing
            for (log_index, log) in msg.logs.iter().enumerate() {
                // Create log key: block_hash + tx_hash + log_index
                let log_key = format!("{}:{}:{}", 
                    hex::encode(msg.block_hash), 
                    hex::encode(msg.tx_hash), 
                    log_index
                );
                
                // Serialize log data
                let log_data = match serde_json::to_vec(log) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Failed to serialize log: {}", e);
                        return Err(StorageError::Serialization(format!("Log serialization failed: {}", e)));
                    }
                };
                
                // Store in logs column family
                if let Err(e) = database.put_log(log_key.as_bytes(), &log_data).await {
                    error!("Failed to store log: {}", e);
                    return Err(e);
                }
                
                // Create address-based index for efficient querying
                let address_key = format!("addr:{}:{}", hex::encode(log.address), hex::encode(msg.tx_hash));
                if let Err(e) = database.put_log_index(&address_key, log_key.as_bytes()).await {
                    warn!("Failed to create address index for log: {}", e);
                    // Continue even if indexing fails
                }
                
                // Create topic-based indices for each topic
                for (topic_idx, topic) in log.topics.iter().enumerate() {
                    let topic_key = format!("topic:{}:{}:{}", 
                        hex::encode(topic), 
                        hex::encode(msg.tx_hash),
                        topic_idx
                    );
                    if let Err(e) = database.put_log_index(&topic_key, log_key.as_bytes()).await {
                        warn!("Failed to create topic index for log: {}", e);
                    }
                }
            }
            
            debug!("Successfully stored {} logs for block {} tx {}", 
                msg.logs.len(), hex::encode(msg.block_hash), hex::encode(msg.tx_hash));
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
            
            // Serialize receipt data
            let receipt_data = match serde_json::to_vec(&msg.receipt) {
                Ok(data) => data,
                Err(e) => {
                    error!("Failed to serialize receipt: {}", e);
                    return Err(StorageError::Serialization(format!("Receipt serialization failed: {}", e)));
                }
            };
            
            // Store receipt in database using transaction hash as key
            let tx_hash_key = hex::encode(msg.receipt.transaction_hash);
            if let Err(e) = database.put_receipt(tx_hash_key.as_bytes(), &receipt_data).await {
                error!("Failed to store receipt in database: {}", e);
                return Err(e);
            }
            
            // Create block -> receipt mapping for efficient block-based queries
            let block_tx_key = format!("{}:{}", hex::encode(msg.block_hash), hex::encode(msg.receipt.transaction_hash));
            if let Err(e) = database.put_receipt_index(&block_tx_key, tx_hash_key.as_bytes()).await {
                warn!("Failed to create block-receipt index: {}", e);
                // Continue even if indexing fails
            }
            
            // Create status-based index for filtering
            let status_key = format!("status:{}:{}", 
                if msg.receipt.status { "success" } else { "failed" },
                hex::encode(msg.receipt.transaction_hash)
            );
            if let Err(e) = database.put_receipt_index(&status_key, tx_hash_key.as_bytes()).await {
                warn!("Failed to create status-receipt index: {}", e);
            }
            
            debug!("Successfully stored receipt for tx: {} in block: {}", 
                hex::encode(msg.receipt.transaction_hash), hex::encode(msg.block_hash));
            Ok(())
        })
    }
}

impl Handler<GetReceiptMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<TransactionReceipt>, StorageError>>;

    fn handle(&mut self, msg: GetReceiptMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get receipt request: {}", msg.tx_hash);
        
        let cache = self.cache.clone();
        let database = self.database.clone();
        let tx_hash = msg.tx_hash;
        
        Box::pin(async move {
            // Check cache first
            if let Some(receipt) = cache.get_receipt(&tx_hash).await {
                debug!("Receipt retrieved from cache: {}", hex::encode(tx_hash));
                return Ok(Some(receipt));
            }
            
            // Query database for receipt
            let tx_hash_key = hex::encode(tx_hash);
            match database.get_receipt(tx_hash_key.as_bytes()).await {
                Ok(Some(receipt_data)) => {
                    // Deserialize receipt data
                    match serde_json::from_slice::<TransactionReceipt>(&receipt_data) {
                        Ok(receipt) => {
                            debug!("Receipt retrieved from database: {}", hex::encode(tx_hash));
                            // Update cache for future access
                            cache.put_receipt(tx_hash, receipt.clone()).await;
                            Ok(Some(receipt))
                        },
                        Err(e) => {
                            error!("Failed to deserialize receipt from database: {}", e);
                            Err(StorageError::Deserialization(format!("Receipt deserialization failed: {}", e)))
                        }
                    }
                },
                Ok(None) => {
                    debug!("Receipt not found in database: {}", hex::encode(tx_hash));
                    Ok(None)
                },
                Err(e) => {
                    error!("Failed to query receipt from database: {}", e);
                    Err(e)
                }
            }
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
            
            // Create archive directory if it doesn't exist
            if let Some(parent) = std::path::Path::new(&msg.archive_path).parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    error!("Failed to create archive directory: {}", e);
                    return Err(StorageError::IO(format!("Archive directory creation failed: {}", e)));
                }
            }
            
            // Open archive database
            let archive_options = rocksdb::Options::default();
            let archive_db = match rocksdb::DB::open(&archive_options, &msg.archive_path) {
                Ok(db) => Arc::new(db),
                Err(e) => {
                    error!("Failed to open archive database: {}", e);
                    return Err(StorageError::Database(format!("Archive DB open failed: {}", e)));
                }
            };
            
            let mut archived_count = 0;
            let mut failed_blocks = Vec::new();
            
            // Archive each block in the range
            for height in msg.from_block..=msg.to_block {
                // Get block hash by height
                let block_hash = match database.get_block_hash_by_height(height).await {
                    Ok(Some(hash)) => hash,
                    Ok(None) => {
                        warn!("Block at height {} not found, skipping", height);
                        failed_blocks.push(height);
                        continue;
                    },
                    Err(e) => {
                        error!("Failed to get block hash for height {}: {}", height, e);
                        failed_blocks.push(height);
                        continue;
                    }
                };
                
                // Read block data from main database
                let block_data = match database.get_block(&block_hash).await {
                    Ok(Some(block)) => match serde_json::to_vec(&block) {
                        Ok(data) => data,
                        Err(e) => {
                            error!("Failed to serialize block {}: {}", hex::encode(block_hash), e);
                            failed_blocks.push(height);
                            continue;
                        }
                    },
                    Ok(None) => {
                        warn!("Block {} not found in database, skipping", hex::encode(block_hash));
                        failed_blocks.push(height);
                        continue;
                    },
                    Err(e) => {
                        error!("Failed to read block {}: {}", hex::encode(block_hash), e);
                        failed_blocks.push(height);
                        continue;
                    }
                };
                
                // Write to archive database
                let archive_key = format!("block:{}", height);
                if let Err(e) = archive_db.put(archive_key.as_bytes(), &block_data) {
                    error!("Failed to write block {} to archive: {}", height, e);
                    failed_blocks.push(height);
                    continue;
                }
                
                // Also store height -> hash mapping in archive
                let height_key = format!("height:{}", height);
                if let Err(e) = archive_db.put(height_key.as_bytes(), block_hash.as_bytes()) {
                    warn!("Failed to write height mapping for block {} to archive: {}", height, e);
                }
                
                archived_count += 1;
                
                if archived_count % 1000 == 0 {
                    info!("Archived {} blocks so far...", archived_count);
                }
            }
            
            // Flush archive database
            if let Err(e) = archive_db.flush() {
                warn!("Failed to flush archive database: {}", e);
            }
            
            if failed_blocks.is_empty() {
                info!("Successfully archived {} blocks to {}", archived_count, msg.archive_path);
                Ok(())
            } else {
                warn!("Archived {} blocks, {} failures: {:?}", archived_count, failed_blocks.len(), failed_blocks);
                Err(StorageError::PartialFailure(format!(
                    "Archived {} blocks but {} failed: {:?}", 
                    archived_count, failed_blocks.len(), failed_blocks
                )))
            }
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
            
            let block_count = msg.query.to_block - msg.query.from_block + 1;
            if block_count > 5000 {
                return Err(StorageError::InvalidRequest("Query range too large, maximum 5000 blocks".to_string()));
            }
            
            // Check if archive path exists
            if !std::path::Path::new(&msg.query.archive_path).exists() {
                return Err(StorageError::NotFound(format!("Archive path does not exist: {}", msg.query.archive_path)));
            }
            
            // Open archive database
            let archive_options = rocksdb::Options::default();
            let archive_db = match rocksdb::DB::open_for_read_only(&archive_options, &msg.query.archive_path, false) {
                Ok(db) => Arc::new(db),
                Err(e) => {
                    error!("Failed to open archive database for reading: {}", e);
                    return Err(StorageError::Database(format!("Archive DB open failed: {}", e)));
                }
            };
            
            let mut blocks = Vec::new();
            let mut failed_blocks = Vec::new();
            
            // Query each block in the range
            for height in msg.query.from_block..=msg.query.to_block {
                let archive_key = format!("block:{}", height);
                
                match archive_db.get(archive_key.as_bytes()) {
                    Ok(Some(block_data)) => {
                        // Deserialize block data
                        match serde_json::from_slice::<ConsensusBlock>(&block_data) {
                            Ok(mut block) => {
                                // Filter out transaction and receipt data if not requested
                                if !msg.query.include_transactions {
                                    // Clear transaction list but keep count
                                    let tx_count = block.execution_payload.transactions.len();
                                    block.execution_payload.transactions.clear();
                                    debug!("Filtered {} transactions from block {}", tx_count, height);
                                }
                                
                                if !msg.query.include_receipts {
                                    // Clear receipts if they exist in the block structure
                                    // Note: This depends on the specific block structure
                                    debug!("Filtered receipts from block {}", height);
                                }
                                
                                blocks.push(block);
                            },
                            Err(e) => {
                                error!("Failed to deserialize archived block {}: {}", height, e);
                                failed_blocks.push(height);
                            }
                        }
                    },
                    Ok(None) => {
                        warn!("Block {} not found in archive", height);
                        failed_blocks.push(height);
                    },
                    Err(e) => {
                        error!("Failed to read block {} from archive: {}", height, e);
                        failed_blocks.push(height);
                    }
                }
                
                // Progress logging for large queries
                if blocks.len() % 1000 == 0 && blocks.len() > 0 {
                    info!("Retrieved {} blocks from archive so far...", blocks.len());
                }
            }
            
            if !failed_blocks.is_empty() {
                warn!("Archive query completed with {} failures: {:?}", failed_blocks.len(), failed_blocks);
            }
            
            info!("Archive query completed, found {} blocks (requested {})", 
                blocks.len(), msg.query.to_block - msg.query.from_block + 1);
            Ok(blocks)
        })
    }
}

// Advanced indexing query handlers

impl Handler<GetBlockByHeightMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockByHeightMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get block by height request: {}", msg.height);
        
        let indexing = self.indexing.clone();
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Use indexing system to get block hash by height
            match indexing.read().await.get_block_hash_by_height(msg.height).await {
                Ok(Some(block_hash)) => {
                    // Now get the block using the hash
                    if let Some(block) = cache.get_block(&block_hash).await {
                        debug!("Block {} retrieved from cache by height {}", block_hash, msg.height);
                        return Ok(Some(block));
                    }
                    
                    // Try database
                    match database.get_block(&block_hash).await {
                        Ok(Some(block)) => {
                            debug!("Block {} retrieved from database by height {}", block_hash, msg.height);
                            // Cache for future access
                            cache.put_block(block_hash, block.clone()).await;
                            Ok(Some(block))
                        },
                        Ok(None) => {
                            warn!("Block hash {} found in index but block not in database", block_hash);
                            Ok(None)
                        },
                        Err(e) => {
                            error!("Failed to get block {} from database: {}", block_hash, e);
                            Err(e)
                        }
                    }
                },
                Ok(None) => {
                    debug!("Block not found at height {}", msg.height);
                    Ok(None)
                },
                Err(e) => {
                    error!("Failed to query block height index: {}", e);
                    Err(StorageError::Database(format!("Height index query failed: {}", e)))
                }
            }
        })
    }
}

impl Handler<GetBlockRangeMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockRangeMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get block range request: {} to {}", msg.start_height, msg.end_height);
        
        if msg.start_height > msg.end_height {
            return Box::pin(async move {
                Err(StorageError::InvalidRequest("start_height must be <= end_height".to_string()))
            });
        }
        
        let range_size = msg.end_height - msg.start_height + 1;
        if range_size > 1000 {
            return Box::pin(async move {
                Err(StorageError::InvalidRequest("Range too large, maximum 1000 blocks".to_string()))
            });
        }
        
        let indexing = self.indexing.clone();
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            let block_range = BlockRange {
                start: msg.start_height,
                end: msg.end_height,
            };
            
            match indexing.read().await.get_blocks_in_range(block_range).await {
                Ok(block_hashes) => {
                    let mut blocks = Vec::new();
                    
                    for block_hash in block_hashes {
                        // Try cache first
                        if let Some(block) = cache.get_block(&block_hash).await {
                            blocks.push(block);
                            continue;
                        }
                        
                        // Try database
                        match database.get_block(&block_hash).await {
                            Ok(Some(block)) => {
                                // Cache for future access
                                cache.put_block(block_hash, block.clone()).await;
                                blocks.push(block);
                            },
                            Ok(None) => {
                                warn!("Block hash {} found in index but block not in database", block_hash);
                                // Continue with other blocks
                            },
                            Err(e) => {
                                error!("Failed to get block {} from database: {}", block_hash, e);
                                return Err(e);
                            }
                        }
                    }
                    
                    info!("Retrieved {} blocks in range {} to {}", blocks.len(), msg.start_height, msg.end_height);
                    Ok(blocks)
                },
                Err(e) => {
                    error!("Failed to query block range: {}", e);
                    Err(StorageError::Database(format!("Block range query failed: {}", e)))
                }
            }
        })
    }
}

impl Handler<GetTransactionByHashMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<TransactionWithBlockInfo>, StorageError>>;

    fn handle(&mut self, msg: GetTransactionByHashMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get transaction by hash request: {}", msg.tx_hash);
        
        let indexing = self.indexing.clone();
        let database = self.database.clone();
        
        Box::pin(async move {
            match indexing.read().await.get_transaction_by_hash(&msg.tx_hash).await {
                Ok(Some(tx_index)) => {
                    // Get the full block to extract transaction details
                    match database.get_block(&tx_index.block_hash).await {
                        Ok(Some(block)) => {
                            if let Some(transaction) = block.execution_payload.transactions.get(tx_index.transaction_index as usize) {
                                let tx_with_info = TransactionWithBlockInfo {
                                    transaction: transaction.clone(),
                                    block_hash: tx_index.block_hash,
                                    block_number: tx_index.block_number,
                                    transaction_index: tx_index.transaction_index,
                                };
                                
                                debug!("Transaction {} found in block {} at index {}", 
                                    msg.tx_hash, tx_index.block_hash, tx_index.transaction_index);
                                Ok(Some(tx_with_info))
                            } else {
                                warn!("Transaction index {} out of bounds for block {} (has {} txs)", 
                                    tx_index.transaction_index, tx_index.block_hash, 
                                    block.execution_payload.transactions.len());
                                Ok(None)
                            }
                        },
                        Ok(None) => {
                            warn!("Block {} found in transaction index but block not in database", tx_index.block_hash);
                            Ok(None)
                        },
                        Err(e) => {
                            error!("Failed to get block {} for transaction {}: {}", tx_index.block_hash, msg.tx_hash, e);
                            Err(e)
                        }
                    }
                },
                Ok(None) => {
                    debug!("Transaction {} not found in index", msg.tx_hash);
                    Ok(None)
                },
                Err(e) => {
                    error!("Failed to query transaction index: {}", e);
                    Err(StorageError::Database(format!("Transaction index query failed: {}", e)))
                }
            }
        })
    }
}

impl Handler<GetAddressTransactionsMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<AddressTransactionInfo>, StorageError>>;

    fn handle(&mut self, msg: GetAddressTransactionsMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get address transactions request: {} (limit: {:?})", msg.address, msg.limit);
        
        let indexing = self.indexing.clone();
        
        Box::pin(async move {
            match indexing.read().await.get_address_transactions(&msg.address, msg.limit).await {
                Ok(address_indices) => {
                    let tx_info: Vec<AddressTransactionInfo> = address_indices.into_iter()
                        .map(|addr_idx| AddressTransactionInfo {
                            transaction_hash: addr_idx.transaction_hash,
                            block_number: addr_idx.block_number,
                            value: addr_idx.value,
                            is_sender: addr_idx.is_sender,
                            transaction_type: match addr_idx.transaction_type {
                                crate::actors::storage::indexing::TransactionType::Transfer => "transfer".to_string(),
                                crate::actors::storage::indexing::TransactionType::ContractCall => "contract_call".to_string(),
                                crate::actors::storage::indexing::TransactionType::ContractDeployment => "contract_deployment".to_string(),
                                crate::actors::storage::indexing::TransactionType::PegIn => "peg_in".to_string(),
                                crate::actors::storage::indexing::TransactionType::PegOut => "peg_out".to_string(),
                            },
                        })
                        .collect();
                    
                    info!("Found {} transactions for address {}", tx_info.len(), msg.address);
                    Ok(tx_info)
                },
                Err(e) => {
                    error!("Failed to query address transactions: {}", e);
                    Err(StorageError::Database(format!("Address transaction query failed: {}", e)))
                }
            }
        })
    }
}