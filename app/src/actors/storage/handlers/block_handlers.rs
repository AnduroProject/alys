//! Block storage and retrieval message handlers
//!
//! This module implements message handlers for all block-related storage operations
//! including storing, retrieving, and querying blocks with caching optimization.

use crate::actors::storage::actor::StorageActor;
use crate::actors::storage::messages::*;
use crate::types::*;
use actix::prelude::*;
use std::sync::Arc;
use tracing::*;

impl Handler<StoreBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: StoreBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        let block_hash = msg.block.hash();
        let height = msg.block.slot;
        let canonical = msg.canonical;
        
        info!("Received store block request: {} at height: {} (canonical: {})", 
            block_hash, height, canonical);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            // Update cache first for fast access
            cache.put_block(block_hash, msg.block.clone()).await;
            
            // Store in database
            match database.put_block(&msg.block).await {
                Ok(()) => {
                    // Update chain head if canonical
                    if canonical {
                        let block_ref = BlockRef {
                            hash: block_hash,
                            height,
                        };
                        if let Err(e) = database.put_chain_head(&block_ref).await {
                            error!("Failed to update chain head: {}", e);
                            return Err(e);
                        }
                    }
                    
                    debug!("Successfully stored block: {} at height: {}", block_hash, height);
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to store block {}: {}", block_hash, e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<GetBlockMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get block request: {}", msg.block_hash);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        let block_hash = msg.block_hash;
        
        Box::pin(async move {
            // Check cache first
            if let Some(block) = cache.get_block(&block_hash).await {
                debug!("Block retrieved from cache: {}", block_hash);
                return Ok(Some(block));
            }
            
            // Fallback to database
            match database.get_block(&block_hash).await {
                Ok(Some(block)) => {
                    // Cache for future access
                    cache.put_block(block_hash, block.clone()).await;
                    debug!("Block retrieved from database: {}", block_hash);
                    Ok(Some(block))
                },
                Ok(None) => {
                    debug!("Block not found: {}", block_hash);
                    Ok(None)
                },
                Err(e) => {
                    error!("Failed to retrieve block {}: {}", block_hash, e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<GetBlockByNumberMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockByNumberMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get block by number request: {}", msg.block_number);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        let height = msg.block_number;
        
        Box::pin(async move {
            match database.get_block_by_height(height).await {
                Ok(Some(block)) => {
                    // Cache the block for future hash-based lookups
                    let block_hash = block.hash();
                    cache.put_block(block_hash, block.clone()).await;
                    debug!("Block retrieved by height: {} -> {}", height, block_hash);
                    Ok(Some(block))
                },
                Ok(None) => {
                    debug!("No block found at height: {}", height);
                    Ok(None)
                },
                Err(e) => {
                    error!("Failed to retrieve block at height {}: {}", height, e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<GetChainHeadMessage> for StorageActor {
    type Result = ResponseFuture<Result<Option<BlockRef>, StorageError>>;

    fn handle(&mut self, _msg: GetChainHeadMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get chain head request");
        
        let database = self.database.clone();
        
        Box::pin(async move {
            match database.get_chain_head().await {
                Ok(head) => {
                    if let Some(ref head) = head {
                        debug!("Retrieved chain head: {} at height: {}", head.hash, head.height);
                    } else {
                        debug!("No chain head found");
                    }
                    Ok(head)
                },
                Err(e) => {
                    error!("Failed to retrieve chain head: {}", e);
                    Err(e)
                }
            }
        })
    }
}

impl Handler<UpdateChainHeadMessage> for StorageActor {
    type Result = ResponseFuture<Result<(), StorageError>>;

    fn handle(&mut self, msg: UpdateChainHeadMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!("Received update chain head request: {} at height: {}", 
            msg.new_head.hash, msg.new_head.height);
        
        let database = self.database.clone();
        
        Box::pin(async move {
            match database.put_chain_head(&msg.new_head).await {
                Ok(()) => {
                    debug!("Successfully updated chain head");
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to update chain head: {}", e);
                    Err(e)
                }
            }
        })
    }
}

/// Block range query handler for retrieving multiple blocks
impl Handler<GetBlockRangeMessage> for StorageActor {
    type Result = ResponseFuture<Result<Vec<ConsensusBlock>, StorageError>>;

    fn handle(&mut self, msg: GetBlockRangeMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received get block range request: {} to {}", msg.start_height, msg.end_height);
        
        if msg.start_height > msg.end_height {
            return Box::pin(async move {
                Err(StorageError::InvalidRequest("Start height must be <= end height".to_string()))
            });
        }
        
        let range_size = msg.end_height - msg.start_height + 1;
        if range_size > 1000 {
            return Box::pin(async move {
                Err(StorageError::InvalidRequest("Range too large, max 1000 blocks".to_string()))
            });
        }
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        
        Box::pin(async move {
            let mut blocks = Vec::new();
            
            for height in msg.start_height..=msg.end_height {
                match database.get_block_by_height(height).await? {
                    Some(block) => {
                        // Cache the block for future access
                        let block_hash = block.hash();
                        cache.put_block(block_hash, block.clone()).await;
                        blocks.push(block);
                    },
                    None => {
                        debug!("Block not found at height: {}", height);
                        // Continue with the next block instead of failing
                    }
                }
            }
            
            info!("Retrieved {} blocks from range {} to {}", 
                blocks.len(), msg.start_height, msg.end_height);
            Ok(blocks)
        })
    }
}

/// Block existence check handler
impl Handler<BlockExistsMessage> for StorageActor {
    type Result = ResponseFuture<Result<bool, StorageError>>;

    fn handle(&mut self, msg: BlockExistsMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Received block exists check: {}", msg.block_hash);
        
        let database = self.database.clone();
        let cache = self.cache.clone();
        let block_hash = msg.block_hash;
        
        Box::pin(async move {
            // First check cache for fast response
            if cache.get_block(&block_hash).await.is_some() {
                debug!("Block exists in cache: {}", block_hash);
                return Ok(true);
            }
            
            // Check database
            match database.get_block(&block_hash).await? {
                Some(_) => {
                    debug!("Block exists in database: {}", block_hash);
                    Ok(true)
                },
                None => {
                    debug!("Block does not exist: {}", block_hash);
                    Ok(false)
                }
            }
        })
    }
}

// Additional message types for block range and existence queries
use actix::Message;

/// Message to retrieve a range of blocks by height
#[derive(Message)]
#[rtype(result = "Result<Vec<ConsensusBlock>, StorageError>")]
pub struct GetBlockRangeMessage {
    pub start_height: u64,
    pub end_height: u64,
}

/// Message to check if a block exists
#[derive(Message)]
#[rtype(result = "Result<bool, StorageError>")]
pub struct BlockExistsMessage {
    pub block_hash: BlockHash,
}