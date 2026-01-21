//! State-related message handlers for Storage Actor - V2

use crate::actors_v2::storage::{
    actor::{StorageActor, StorageError},
    messages::*,
};
use crate::auxpow_miner::BlockIndex;
use crate::block::ConvertBlockHash;
use actix::prelude::*;
use ethereum_types::U256;
use std::time::Instant;
use tracing::*;

impl Handler<UpdateStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: UpdateStateMessage, _: &mut Context<Self>) -> Self::Result {
        let _correlation_id = msg.correlation_id;
        debug!(
            "Handling UpdateStateMessage for key with {} bytes",
            msg.key.len()
        );

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
        debug!(
            "Handling GetStateMessage for key with {} bytes",
            msg.key.len()
        );

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
        info!(
            "Handling BatchWriteMessage with {} operations",
            msg.operations.len()
        );

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
                        let block_hash = block.message.block_hash().to_block_hash();
                        cache.put_block(block_hash, block.clone()).await;

                        if *canonical {
                            metrics.record_block_stored(
                                block.message.slot,
                                std::time::Duration::default(),
                                true,
                            );
                        }
                    }
                    WriteOperation::Put { key, value } => {
                        cache.put_state(key.clone(), value.clone()).await;
                    }
                    _ => {} // Other operations don't affect cache
                }
            }

            let batch_time = start_time.elapsed();
            let operations_len = operations.len();
            metrics.record_batch_operation(operations_len, batch_time);

            info!(
                "Batch write completed with {} operations in {:?}",
                operations_len, batch_time
            );
            Ok(())
        })
    }
}

// =============================================================================
// FEE ACCUMULATION HANDLERS (V0 Compatibility)
// =============================================================================

impl Handler<GetAccumulatedFeesMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<U256>, StorageError>>;

    fn handle(&mut self, msg: GetAccumulatedFeesMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        debug!(
            correlation_id = ?correlation_id,
            block_root = %msg.block_root,
            "Getting accumulated fees for block"
        );

        let database = self.database.clone();
        let block_root = msg.block_root;

        Box::pin(async move {
            // Use same key format as V0 storage (fee accumulation by block root)
            let fee_key = format!("accumulated_fees_{}", block_root);

            match database.get_state(fee_key.as_bytes()).await {
                Ok(Some(fee_data)) => {
                    // Deserialize U256 from stored bytes
                    match serde_json::from_slice::<U256>(&fee_data) {
                        Ok(fees) => {
                            debug!(
                                correlation_id = ?correlation_id,
                                block_root = %block_root,
                                accumulated_fees = %fees,
                                "Retrieved accumulated fees from storage"
                            );
                            Ok(Some(fees))
                        }
                        Err(e) => {
                            error!(
                                correlation_id = ?correlation_id,
                                error = ?e,
                                "Failed to deserialize accumulated fees"
                            );
                            Err(StorageError::Serialization(format!(
                                "Fee deserialization failed: {}",
                                e
                            )))
                        }
                    }
                }
                Ok(None) => {
                    debug!(
                        correlation_id = ?correlation_id,
                        block_root = %block_root,
                        "No accumulated fees found for block (genesis or first block)"
                    );
                    Ok(None)
                }
                Err(e) => {
                    error!(
                        correlation_id = ?correlation_id,
                        error = ?e,
                        "Failed to get accumulated fees from storage"
                    );
                    Err(StorageError::Database(format!(
                        "Failed to get accumulated fees: {}",
                        e
                    )))
                }
            }
        })
    }
}

impl Handler<SetAccumulatedFeesMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: SetAccumulatedFeesMessage, _: &mut Context<Self>) -> Self::Result {
        let correlation_id = msg.correlation_id;
        debug!(
            correlation_id = ?correlation_id,
            block_root = %msg.block_root,
            fees = %msg.fees,
            "Setting accumulated fees for block"
        );

        let database = self.database.clone();
        let block_root = msg.block_root;
        let fees = msg.fees;

        Box::pin(async move {
            // Serialize U256 fees for storage
            let fee_data = match serde_json::to_vec(&fees) {
                Ok(data) => data,
                Err(e) => {
                    error!(
                        correlation_id = ?correlation_id,
                        error = ?e,
                        "Failed to serialize accumulated fees"
                    );
                    return Err(StorageError::Serialization(format!(
                        "Fee serialization failed: {}",
                        e
                    )));
                }
            };

            // Use same key format as V0 storage
            let fee_key = format!("accumulated_fees_{}", block_root);

            match database.put_state(fee_key.as_bytes(), &fee_data).await {
                Ok(()) => {
                    info!(
                        correlation_id = ?correlation_id,
                        block_root = %block_root,
                        fees = %fees,
                        "Successfully stored accumulated fees"
                    );
                    Ok(())
                }
                Err(e) => {
                    error!(
                        correlation_id = ?correlation_id,
                        error = ?e,
                        "Failed to store accumulated fees"
                    );
                    Err(StorageError::Database(format!(
                        "Failed to store accumulated fees: {}",
                        e
                    )))
                }
            }
        })
    }
}

// =============================================================================
// CUMULATIVE DIFFICULTY HANDLERS
// =============================================================================

impl Handler<PutCumulativeDifficultyMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(
        &mut self,
        msg: PutCumulativeDifficultyMessage,
        _: &mut Context<Self>,
    ) -> Self::Result {
        let correlation_id = msg.correlation_id;
        debug!(
            correlation_id = ?correlation_id,
            height = msg.height,
            cumulative_difficulty = msg.cumulative_difficulty,
            "Handling PutCumulativeDifficultyMessage"
        );

        let database = self.database.clone();
        let height = msg.height;
        let cumulative_difficulty = msg.cumulative_difficulty;

        Box::pin(async move {
            database
                .put_cumulative_difficulty(height, cumulative_difficulty)
                .await?;

            debug!(
                height = height,
                cumulative_difficulty = cumulative_difficulty,
                "Stored cumulative difficulty"
            );

            Ok(())
        })
    }
}

impl Handler<GetCumulativeDifficultyMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<u128>, StorageError>>;

    fn handle(
        &mut self,
        msg: GetCumulativeDifficultyMessage,
        _: &mut Context<Self>,
    ) -> Self::Result {
        let correlation_id = msg.correlation_id;
        debug!(
            correlation_id = ?correlation_id,
            height = msg.height,
            "Handling GetCumulativeDifficultyMessage"
        );

        let database = self.database.clone();
        let height = msg.height;

        Box::pin(async move {
            let result = database.get_cumulative_difficulty(height).await?;

            debug!(
                height = height,
                found = result.is_some(),
                "Retrieved cumulative difficulty"
            );

            Ok(result)
        })
    }
}
