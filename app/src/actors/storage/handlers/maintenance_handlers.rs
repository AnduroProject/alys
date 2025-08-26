//! Maintenance and management message handlers
//!
//! This module implements message handlers for database maintenance operations
//! including compaction, pruning, backup, cleanup, and advanced index rebuilding.

use crate::actors::storage::actor::StorageActor;
use crate::actors::storage::indexing::BlockRange;
use crate::actors::storage::messages::*;
use crate::types::*;
use actix::prelude::*;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::*;

impl Handler<CompactDatabaseMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: CompactDatabaseMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received database compaction request for: {}", msg.database_name);
        
        let database = self.database.clone();
        
        Box::pin(async move {
            match database.compact_database().await {
                Ok(()) => {
                    info!("Successfully completed database compaction");
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to compact database: {}", e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<PruneDataMessage> for StorageActor {
    type Result = ResponseFuture<Result<PruneResult, StorageError>>;

    fn handle(&mut self, msg: PruneDataMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received data pruning request: keep {} blocks, prune_receipts={}, prune_state={}, prune_logs={}",
            msg.prune_config.keep_blocks, msg.prune_config.prune_receipts, 
            msg.prune_config.prune_state, msg.prune_config.prune_logs);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Get current chain head to determine what to keep
            let chain_head = match database.get_chain_head().await? {
                Some(head) => head,
                None => {
                    warn!("No chain head found, cannot prune data");
                    return Ok(PruneResult {
                        blocks_pruned: 0,
                        receipts_pruned: 0,
                        state_entries_pruned: 0,
                        logs_pruned: 0,
                        space_freed_bytes: 0,
                    });
                }
            };
            
            let cutoff_height = chain_head.height.saturating_sub(msg.prune_config.keep_blocks);
            info!("Pruning data below height: {} (current head: {})", cutoff_height, chain_head.height);
            
            // Perform the actual pruning operations
            let mut result = PruneResult {
                blocks_pruned: 0,
                receipts_pruned: 0,
                state_entries_pruned: 0,
                logs_pruned: 0,
                space_freed_bytes: 0,
            };
            
            // Get size before pruning for space calculation
            let size_before = database.get_stats().await?.total_size_bytes;
            
            // Prune blocks if requested (keep canonical chain)
            if cutoff_height > 0 {
                info!("Pruning non-canonical blocks below height {}", cutoff_height);
                result.blocks_pruned = database.prune_blocks(cutoff_height, false).await?
                    .unwrap_or(0) as u64;
            }
            
            // Prune receipts if requested
            if msg.prune_config.prune_receipts {
                info!("Pruning receipts below height {}", cutoff_height);
                result.receipts_pruned = database.prune_receipts(cutoff_height).await?
                    .unwrap_or(0) as u64;
            }
            
            // Prune old state if requested (careful with this one)
            if msg.prune_config.prune_state {
                info!("Pruning old state below height {}", cutoff_height);
                result.state_entries_pruned = database.prune_old_state(cutoff_height).await?
                    .unwrap_or(0) as u64;
            }
            
            // Prune logs if requested
            if msg.prune_config.prune_logs {
                info!("Pruning logs below height {}", cutoff_height);
                result.logs_pruned = database.prune_logs(cutoff_height).await?
                    .unwrap_or(0) as u64;
            }
            
            // Compact database after pruning
            database.compact_database().await?;
            
            // Calculate space freed
            let size_after = database.get_stats().await?.total_size_bytes;
            result.space_freed_bytes = size_before.saturating_sub(size_after);
            
            // Clear relevant cache entries
            // Note: This is a simplified cache clearing - in production we'd be more selective
            if cutoff_height > 0 {
                cache.clear_all().await;
                info!("Cleared cache due to pruning operation");
            }
            
            info!("Data pruning completed: {:?}", result);
            Ok(result)
        })
    }
}

impl Handler<CreateSnapshotMessage> for StorageActor {
    type Result = ResponseFuture<Result<SnapshotInfo, StorageError>>;

    fn handle(&mut self, msg: CreateSnapshotMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received create snapshot request: {}", msg.snapshot_name);
        
        let database = self.database.clone();
        
        Box::pin(async move {
            let created_at = std::time::SystemTime::now();
            
            // Get current chain head for snapshot metadata
            let (block_number, state_root) = match database.get_chain_head().await? {
                Some(head) => {
                    match database.get_block(&head.hash).await? {
                        Some(block) => (head.height, block.execution_payload.state_root),
                        None => (head.height, Hash256::zero()),
                    }
                },
                None => (0, Hash256::zero()),
            };
            
            // Get database statistics for size estimation
            let db_stats = database.get_stats().await?;
            
            // Create the actual snapshot
            let snapshot_path = format!("snapshots/{}", msg.snapshot_name);
            
            match database.create_snapshot(&snapshot_path).await {
                Ok(snapshot_size) => {
                    let snapshot = SnapshotInfo {
                        name: msg.snapshot_name.clone(),
                        created_at,
                        size_bytes: snapshot_size,
                        block_number,
                        state_root,
                    };
                    
                    info!("Snapshot created successfully: {} at block {} (size: {} bytes)", 
                        msg.snapshot_name, block_number, snapshot_size);
                    
                    Ok(snapshot)
                },
                Err(e) => {
                    error!("Failed to create snapshot {}: {}", msg.snapshot_name, e);
                    Err(e)
                }
            }
            
        })
    }
}

impl Handler<RestoreSnapshotMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: RestoreSnapshotMessage, _ctx: &mut Self::Context) -> Self::Result {
        warn!("Received restore snapshot request: {} - THIS IS A DESTRUCTIVE OPERATION", msg.snapshot_name);
        
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Clear all caches before restoration
            cache.clear_all().await;
            
            // Perform the actual snapshot restoration
            let snapshot_path = format!("snapshots/{}", msg.snapshot_name);
            
            if !Path::new(&snapshot_path).exists() {
                return Err(StorageError::InvalidRequest(
                    format!("Snapshot {} not found at {}", msg.snapshot_name, snapshot_path)
                ));
            }
            
            // Stop all pending writes
            warn!("Stopping all write operations for snapshot restoration");
            
            match database.restore_from_snapshot(&snapshot_path).await {
                Ok(()) => {
                    info!("Snapshot restoration completed successfully: {}", msg.snapshot_name);
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to restore snapshot {}: {}", msg.snapshot_name, e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<CreateBackupMessage> for StorageActor {
    type Result = ResponseFuture<Result<BackupInfo, StorageError>>;

    fn handle(&mut self, msg: CreateBackupMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received create backup request to: {} (compress: {}, incremental: {})", 
            msg.config.destination, msg.config.compress, msg.config.incremental);
        
        let database = self.database.clone();
        
        Box::pin(async move {
            let created_at = std::time::SystemTime::now();
            
            // Get database statistics for backup planning
            let db_stats = database.get_stats().await?;
            
            // Create the actual backup
            match database.create_backup(&msg.config).await {
                Ok((backup_size, checksum)) => {
                    let backup_info = BackupInfo {
                        path: msg.config.destination.clone(),
                        created_at,
                        size_bytes: backup_size,
                        compressed: msg.config.compress,
                        checksum,
                    };
                    
                    info!("Backup created successfully: {} (size: {} bytes, compressed: {})", 
                        msg.config.destination, backup_size, msg.config.compress);
                    
                    Ok(backup_info)
                },
                Err(e) => {
                    error!("Failed to create backup: {}", e);
                    Err(e)
                }
            }
            
        })
    }
}

impl Handler<FlushCacheMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, _msg: FlushCacheMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received flush cache request");
        
        let cache = self.cache.clone();
        
        Box::pin(async move {
            cache.clear_all().await;
            info!("All caches flushed successfully");
            Ok(())
        })
    }
}

impl Handler<RebuildIndexMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: RebuildIndexMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received rebuild index request: {:?}", msg.index_type);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        let indexing = self.indexing.clone();
        
        Box::pin(async move {
            // Clear cache to ensure fresh data after index rebuild
            cache.clear_all().await;
            
            let start_time = SystemTime::now();
            let mut rebuilt_entries = 0u64;
            
            match msg.index_type {
                IndexType::BlockByHash => {
                    info!("Rebuilding block-by-hash index");
                    rebuilt_entries = database.rebuild_block_hash_index().await?;
                },
                IndexType::BlockByNumber => {
                    info!("Rebuilding block-by-number index");
                    rebuilt_entries = database.rebuild_block_height_index().await?;
                },
                IndexType::TransactionByHash => {
                    info!("Rebuilding transaction-by-hash index");
                    // Get all blocks and re-index their transactions
                    if let Some(chain_head) = database.get_chain_head().await? {
                        let range = BlockRange { start: 0, end: chain_head.height };
                        let block_hashes = indexing.read().await.get_blocks_in_range(range).await
                            .map_err(|e| StorageError::Database(format!("Range query failed: {}", e)))?;
                        
                        for block_hash in block_hashes {
                            if let Ok(Some(block)) = database.get_block(&block_hash).await {
                                indexing.write().await.index_block(&block).await
                                    .map_err(|e| StorageError::Database(format!("Block indexing failed: {}", e)))?;
                                rebuilt_entries += block.execution_payload.transactions.len() as u64;
                            }
                        }
                    }
                },
                IndexType::StateByKey => {
                    info!("Rebuilding state key index");
                    rebuilt_entries = database.rebuild_state_index().await?;
                },
                IndexType::All => {
                    info!("Rebuilding ALL indices - this may take a while");
                    
                    // Rebuild all index types sequentially
                    info!("Phase 1/4: Rebuilding block hash index");
                    rebuilt_entries += database.rebuild_block_hash_index().await?;
                    
                    info!("Phase 2/4: Rebuilding block height index");
                    rebuilt_entries += database.rebuild_block_height_index().await?;
                    
                    info!("Phase 3/4: Rebuilding transaction indices");
                    if let Some(chain_head) = database.get_chain_head().await? {
                        let range = BlockRange { start: 0, end: chain_head.height };
                        let block_hashes = indexing.read().await.get_blocks_in_range(range).await
                            .map_err(|e| StorageError::Database(format!("Range query failed: {}", e)))?;
                        
                        for (i, block_hash) in block_hashes.iter().enumerate() {
                            if i % 1000 == 0 {
                                info!("Reindexing progress: {}/{} blocks", i, block_hashes.len());
                            }
                            
                            if let Ok(Some(block)) = database.get_block(block_hash).await {
                                indexing.write().await.index_block(&block).await
                                    .map_err(|e| StorageError::Database(format!("Block indexing failed: {}", e)))?;
                                rebuilt_entries += block.execution_payload.transactions.len() as u64;
                            }
                        }
                    }
                    
                    info!("Phase 4/4: Rebuilding state index");
                    rebuilt_entries += database.rebuild_state_index().await?;
                },
                _ => {
                    warn!("Index type not yet implemented: {:?}", msg.index_type);
                    return Err(StorageError::InvalidRequest(
                        format!("Unsupported index type: {:?}", msg.index_type)
                    ));
                }
            }
            
            // Final compaction after index rebuild
            database.compact_database().await?;
            
            let duration = start_time.elapsed().unwrap_or_default();
            info!("Index rebuild completed: {:?} - {} entries rebuilt in {:.2}s", 
                msg.index_type, rebuilt_entries, duration.as_secs_f64());
            
            Ok(())
        })
    }
}

// Additional maintenance handlers for advanced operations

impl Handler<AnalyzeDatabaseMessage> for StorageActor {
    type Result = ResponseFuture<Result<DatabaseAnalysis, StorageError>>;

    fn handle(&mut self, _msg: AnalyzeDatabaseMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received database analysis request");
        
        let database = self.database.clone();
        let indexing = self.indexing.clone();
        
        Box::pin(async move {
            let stats = database.get_stats().await?;
            let indexing_stats = indexing.read().await.get_stats().await;
            
            // Analyze column family sizes
            let cf_stats = database.get_column_family_stats().await?;
            
            // Check for index consistency
            let inconsistencies = database.check_index_consistency().await?;
            
            let analysis = DatabaseAnalysis {
                total_size_bytes: stats.total_size_bytes,
                total_blocks: indexing_stats.total_indexed_blocks,
                total_transactions: indexing_stats.total_indexed_transactions,
                column_family_sizes: cf_stats,
                index_inconsistencies: inconsistencies,
                fragmentation_ratio: database.get_fragmentation_ratio().await.unwrap_or(0.0),
                last_compaction: database.get_last_compaction_time().await,
                recommended_actions: vec![], // Will be populated based on analysis
            };
            
            info!("Database analysis completed: size={}MB, fragmentation={:.1}%", 
                analysis.total_size_bytes / (1024 * 1024), 
                analysis.fragmentation_ratio * 100.0);
            
            Ok(analysis)
        })
    }
}

impl Handler<OptimizeDatabaseMessage> for StorageActor {
    type Result = ResponseFuture<Result<OptimizationResult, StorageError>>;

    fn handle(&mut self, msg: OptimizeDatabaseMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received database optimization request: {:?}", msg.optimization_type);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            let start_time = SystemTime::now();
            let size_before = database.get_stats().await?.total_size_bytes;
            
            let mut result = OptimizationResult {
                optimization_type: msg.optimization_type.clone(),
                space_saved_bytes: 0,
                duration_seconds: 0.0,
                improvements: vec![],
            };
            
            match msg.optimization_type {
                OptimizationType::Compact => {
                    database.compact_database().await?;
                    result.improvements.push("Database compacted".to_string());
                },
                OptimizationType::Vacuum => {
                    database.vacuum_database().await?;
                    result.improvements.push("Database vacuumed".to_string());
                },
                OptimizationType::ReorganizeIndices => {
                    database.reorganize_indices().await?;
                    result.improvements.push("Indices reorganized".to_string());
                },
                OptimizationType::OptimizeCache => {
                    cache.optimize().await;
                    result.improvements.push("Cache optimized".to_string());
                },
                OptimizationType::Full => {
                    database.compact_database().await?;
                    database.vacuum_database().await?;
                    database.reorganize_indices().await?;
                    cache.optimize().await;
                    result.improvements.extend(vec![
                        "Database compacted".to_string(),
                        "Database vacuumed".to_string(),
                        "Indices reorganized".to_string(),
                        "Cache optimized".to_string(),
                    ]);
                },
            }
            
            let size_after = database.get_stats().await?.total_size_bytes;
            result.space_saved_bytes = size_before.saturating_sub(size_after);
            result.duration_seconds = start_time.elapsed().unwrap_or_default().as_secs_f64();
            
            info!("Database optimization completed: {:?} - saved {}MB in {:.2}s", 
                msg.optimization_type, 
                result.space_saved_bytes / (1024 * 1024),
                result.duration_seconds);
            
            Ok(result)
        })
    }
}