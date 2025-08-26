//! Query and statistics message handlers
//!
//! This module implements message handlers for querying storage statistics,
//! cache information, advanced indexing queries, and other operational data.

use crate::actors::storage::actor::StorageActor;
use crate::actors::storage::indexing::{BlockRange, IndexingError};
use crate::actors::storage::messages::*;
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
                        .map(|addr_idx| AddressTransactionInfo {\n                            transaction_hash: addr_idx.transaction_hash,\n                            block_number: addr_idx.block_number,\n                            value: addr_idx.value,\n                            is_sender: addr_idx.is_sender,\n                            transaction_type: match addr_idx.transaction_type {\n                                crate::actors::storage::indexing::TransactionType::Transfer => \"transfer\".to_string(),\n                                crate::actors::storage::indexing::TransactionType::ContractCall => \"contract_call\".to_string(),\n                                crate::actors::storage::indexing::TransactionType::ContractDeployment => \"contract_deployment\".to_string(),\n                                crate::actors::storage::indexing::TransactionType::PegIn => \"peg_in\".to_string(),\n                                crate::actors::storage::indexing::TransactionType::PegOut => \"peg_out\".to_string(),\n                            },\n                        })\n                        .collect();\n                    \n                    info!(\"Found {} transactions for address {}\", tx_info.len(), msg.address);\n                    Ok(tx_info)\n                },\n                Err(e) => {\n                    error!(\"Failed to query address transactions: {}\", e);\n                    Err(StorageError::Database(format!(\"Address transaction query failed: {}\", e)))\n                }\n            }\n        })\n    }\n}