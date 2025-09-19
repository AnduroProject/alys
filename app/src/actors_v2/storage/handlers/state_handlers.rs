//! State-related message handlers for Storage Actor - V2

use crate::actors_v2::storage::{
    actor::{StorageActor, StorageError},
    messages::*,
};
use crate::auxpow_miner::BlockIndex;
use crate::block::ConvertBlockHash;
use actix::prelude::*;
use tracing::*;
use std::time::Instant;

impl Handler<UpdateStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: UpdateStateMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling UpdateStateMessage for key with {} bytes", msg.key.len());

        let key = msg.key;
        let value = msg.value;
        let cache = self.cache.clone();
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        Box::pin(async move {
            let start_time = Instant::now();

            // Update cache first
            cache.put_state(key.clone(), value.clone()).await;

            // Store in database
            database.put_state(&key, &value).await?;

            let duration = start_time.elapsed();
            metrics.record_state_update(duration);

            debug!("State updated in {:?}", duration);
            Ok(())
        })
    }
}

impl Handler<GetStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<Vec<u8>>, StorageError>>;

    fn handle(&mut self, msg: GetStateMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!("Handling GetStateMessage for key with {} bytes", msg.key.len());

        let key = msg.key;
        let cache = self.cache.clone();
        let database = self.database.clone();
        let mut metrics = self.metrics.clone();

        Box::pin(async move {
            let start_time = Instant::now();

            // Check cache first
            if let Some(value) = cache.get_state(&key).await {
                let duration = start_time.elapsed();
                metrics.record_state_query(duration, true);
                return Ok(Some(value));
            }

            // Fallback to database
            let value = database.get_state(&key).await?;
            let duration = start_time.elapsed();

            if let Some(ref value) = value {
                // Cache for future access
                cache.put_state(key, value.clone()).await;
                metrics.record_state_query(duration, false);
            } else {
                metrics.record_state_not_found();
            }

            Ok(value)
        })
    }
}

impl Handler<BatchWriteMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: BatchWriteMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        info!("Handling BatchWriteMessage with {} operations", msg.operations.len());

        let operations = msg.operations;
        let database = self.database.clone();
        let cache = self.cache.clone();
        let mut metrics = self.metrics.clone();

        Box::pin(async move {
            let start_time = Instant::now();

            // Execute the batch in the database
            database.batch_write(operations.clone()).await?;

            // Update cache for relevant operations
            for operation in &operations {
                match operation {
                    WriteOperation::PutBlock { block, canonical } => {
                        let block_hash = block.block_hash().to_block_hash();
                        cache.put_block(block_hash, block.clone()).await;

                        if *canonical {
                            metrics.record_block_stored(block.slot, std::time::Duration::default(), true);
                        }
                    },
                    WriteOperation::Put { key, value } => {
                        cache.put_state(key.clone(), value.clone()).await;
                    },
                    _ => {} // Other operations don't affect cache
                }
            }

            let batch_time = start_time.elapsed();
            let operations_len = operations.len();
            metrics.record_batch_operation(operations_len, batch_time);

            info!("Batch write completed with {} operations in {:?}", operations_len, batch_time);
            Ok(())
        })
    }
}