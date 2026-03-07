//! Block-related message handlers for Storage Actor - V2

use crate::actors_v2::storage::{
    actor::{AlysConsensusBlock, StorageActor, StorageError},
    messages::*,
};
use crate::auxpow_miner::BlockIndex;
use crate::block::ConvertBlockHash;
use actix::prelude::*;
use tracing::*;

impl Handler<StoreBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreBlockMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!(
            "Handling StoreBlockMessage for block at height {}",
            msg.block.message.execution_payload.block_number
        );

        let block = msg.block;
        let canonical = msg.canonical;
        let cache = self.cache.clone();
        let database = self.database.clone();
        let indexing = self.indexing.clone();
        let metrics = self.metrics.clone();

        Box::pin(async move {
            let block_hash = block.message.block_hash().to_block_hash();
            let height = block.message.execution_payload.block_number;

            debug!(
                "Storing block: {} at height: {} (canonical: {})",
                block_hash, height, canonical
            );

            let start_time = std::time::Instant::now();

            // Update cache first for fast access
            cache.put_block(block_hash, block.clone()).await;

            // Store in database
            database.put_block(&block).await?;

            // Index the block for advanced queries
            if let Err(e) = indexing.write().await.index_block(&block).await {
                error!("Failed to index block {}: {}", block_hash, e);
                // Continue execution - indexing failure shouldn't stop block storage
            }

            // Update chain head if this is canonical
            if canonical {
                let block_ref = crate::actors_v2::storage::actor::BlockRef {
                    hash: block_hash,
                    number: height,
                    execution_hash: block.message.execution_payload.block_hash,
                };
                database.put_chain_head(&block_ref).await?;
            }

            // Record metrics
            let storage_time = start_time.elapsed();
            metrics.record_block_stored(height, storage_time, canonical);

            info!(
                "Successfully stored block: {} at height: {} in {:?}",
                block_hash, height, storage_time
            );
            Ok(())
        })
    }
}

impl Handler<AtomicCommitBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: AtomicCommitBlockMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let height = msg.block.message.execution_payload.block_number;

        info!(
            correlation_id = ?correlation_id,
            height = height,
            hash = %msg.new_head.hash,
            "Handling AtomicCommitBlockMessage"
        );

        let block = msg.block;
        let new_head = msg.new_head;
        let cache = self.cache.clone();
        let database = self.database.clone();
        let indexing = self.indexing.clone();
        let metrics = self.metrics.clone();

        Box::pin(async move {
            let block_hash = block.message.block_hash().to_block_hash();
            let start_time = std::time::Instant::now();

            // Update cache first for fast access
            cache.put_block(block_hash, block.clone()).await;

            // Atomically commit block + height index + chain head with sync
            database.atomic_commit_block(&block, &new_head).await?;

            // Index the block (non-critical - failure shouldn't stop commit)
            if let Err(e) = indexing.write().await.index_block(&block).await {
                error!("Failed to index block {}: {}", block_hash, e);
            }

            // Record metrics
            let storage_time = start_time.elapsed();
            metrics.record_block_stored(height, storage_time, true);
            metrics.record_chain_head_update();

            info!(
                block_hash = %block_hash,
                height = height,
                duration_ms = storage_time.as_millis(),
                "Atomically committed block successfully"
            );

            Ok(())
        })
    }
}

impl Handler<GetBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<AlysConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetBlockMessage for block {}", msg.block_hash);

        let block_hash = msg.block_hash;
        let cache = self.cache.clone();
        let database = self.database.clone();
        let metrics = self.metrics.clone();

        Box::pin(async move {
            debug!("Retrieving block: {}", block_hash);

            let start_time = std::time::Instant::now();

            // Check cache first
            if let Some(block) = cache.get_block(&block_hash).await {
                let retrieval_time = start_time.elapsed();
                metrics.record_block_retrieved(retrieval_time, true);
                debug!(
                    "Block retrieved from cache: {} in {:?}",
                    block_hash, retrieval_time
                );
                return Ok(Some(block));
            }

            // Fallback to database
            let block = database.get_block(&block_hash).await?;
            let retrieval_time = start_time.elapsed();

            if let Some(ref block) = block {
                // Cache for future access
                cache.put_block(block_hash, block.clone()).await;
                metrics.record_block_retrieved(retrieval_time, false);
                debug!(
                    "Block retrieved from database: {} in {:?}",
                    block_hash, retrieval_time
                );
            } else {
                metrics.record_block_not_found();
                debug!("Block not found: {}", block_hash);
            }

            Ok(block)
        })
    }
}

impl Handler<GetBlockByHeightMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<AlysConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockByHeightMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetBlockByHeightMessage for height {}", msg.height);

        let height = msg.height;
        let database = self.database.clone();

        Box::pin(async move { database.get_block_by_height(height).await })
    }
}

impl Handler<GetBlockRangeMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<AlysConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockRangeMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!(
            "Handling GetBlockRangeMessage for range {} to {}",
            msg.start_height, msg.end_height
        );

        let start_height = msg.start_height;
        let end_height = msg.end_height;
        let database = self.database.clone();

        Box::pin(async move {
            let mut blocks = Vec::new();
            let mut missing_heights = Vec::new();
            let expected_count = end_height.saturating_sub(start_height) + 1;

            for height in start_height..=end_height {
                match database.get_block_by_height(height).await? {
                    Some(block) => blocks.push(block),
                    None => missing_heights.push(height),
                }
            }

            // Task 2.2: Add explicit logging when blocks are missing in range
            // This helps diagnose sync issues where empty responses are returned
            if !missing_heights.is_empty() {
                warn!(
                    start_height = start_height,
                    end_height = end_height,
                    expected_count = expected_count,
                    found_count = blocks.len(),
                    missing_count = missing_heights.len(),
                    first_missing = ?missing_heights.first(),
                    last_missing = ?missing_heights.last(),
                    "GetBlockRange: {} blocks missing in requested range {}-{}",
                    missing_heights.len(),
                    start_height,
                    end_height
                );
            } else {
                debug!(
                    start_height = start_height,
                    end_height = end_height,
                    count = blocks.len(),
                    "GetBlockRange: returning {} blocks",
                    blocks.len()
                );
            }

            // Return partial results instead of silently returning empty
            // This allows sync to make progress with available blocks
            Ok(blocks)
        })
    }
}

impl Handler<BlockExistsMessage> for StorageActor {
    type Result = ResponseFuture<Result<bool, StorageError>>;

    fn handle(&mut self, msg: BlockExistsMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling BlockExistsMessage for block {}", msg.block_hash);

        let block_hash = msg.block_hash;
        let database = self.database.clone();

        Box::pin(async move {
            match database.get_block(&block_hash).await? {
                Some(_) => Ok(true),
                None => Ok(false),
            }
        })
    }
}
