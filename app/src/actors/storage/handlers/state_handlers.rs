//! State storage and retrieval message handlers
//!
//! This module implements message handlers for state-related storage operations
//! including storing, retrieving, and querying state data with caching optimization.

use crate::actors::storage::actor::StorageActor;
use crate::messages::storage_messages::*;
use crate::types::*;
use actix::prelude::*;
use tracing::*;

impl Handler<UpdateStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: UpdateStateMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received state update request: key length: {}, value length: {}", 
            msg.key.len(), msg.value.len());
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Update cache first for fast access
            cache.put_state(msg.key.clone(), msg.value.clone()).await;
            
            // Store in database
            match database.put_state(&msg.key, &msg.value).await {
                Ok(()) => {
                    debug!("Successfully updated state");
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to update state: {}", e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<GetStateMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<Vec<u8>>, StorageError>>;

    fn handle(&mut self, msg: GetStateMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received state query request: key length: {}", msg.key.len());
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        let key = msg.key;
        
        Box::pin(async move {
            // Check cache first
            if let Some(value) = cache.get_state(&key).await {
                debug!("State retrieved from cache");
                return Ok(Some(value));
            }
            
            // Fallback to database
            match database.get_state(&key).await {
                Ok(Some(value)) => {
                    // Cache for future access
                    cache.put_state(key, value.clone()).await;
                    debug!("State retrieved from database");
                    Ok(Some(value))
                },
                Ok(None) => {
                    debug!("State key not found");
                    Ok(None)
                },
                Err(e) => {
                    error!("Failed to retrieve state: {}", e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<BatchWriteMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: BatchWriteMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received batch write request with {} operations", msg.operations.len());
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Execute the batch in the database
            database.batch_write(msg.operations.clone()).await?;
            
            // Update cache for relevant operations
            for operation in &msg.operations {
                match operation {
                    WriteOperation::PutBlock { block, canonical: _ } => {
                        let block_hash = block.hash();
                        cache.put_block(block_hash, block.clone()).await;
                    },
                    WriteOperation::Put { key, value } => {
                        cache.put_state(key.clone(), value.clone()).await;
                    },
                    _ => {} // Other operations don't affect cache
                }
            }
            
            info!("Batch write completed with {} operations", msg.operations.len());
            Ok(())
        })
    }
}