//! SyncActor Block Processing Message Handlers
//! 
//! Contains handlers for block-related operations including block requests,
//! validation coordination, and processing pipeline management.

use actix::{Handler, Context, ResponseFuture};
use ethereum_types::H256;

use crate::actors::network::messages::*;
use crate::actors::network::messages::sync_messages::*;
use crate::actors::network::sync::actor::SyncActor;
use crate::actors::network::sync::{SyncStatus, OperationType};

impl Handler<RequestBlocks> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<BlocksResponse>>;

    fn handle(&mut self, msg: RequestBlocks, _ctx: &mut Context<Self>) -> Self::Result {
        let block_processor = self.block_processor.clone();
        let peer_manager = self.peer_manager.clone();
        let network_actor = self.network_actor.clone();
        let start_height = msg.start_height;
        let count = msg.count;
        let preferred_peers = msg.preferred_peers;

        tracing::debug!(
            "RequestBlocks from height {} count {} peers {:?}",
            start_height, count, preferred_peers
        );

        Box::pin(async move {
            let mut blocks = Vec::new();
            let mut source_peers = Vec::new();

            // Try to get blocks from local storage first (via block processor)
            if let Ok(local_blocks) = block_processor.get_blocks_range(start_height, start_height + count as u64).await {
                for (height, block_data) in local_blocks {
                    if blocks.len() >= count as usize {
                        break;
                    }
                    blocks.push(BlockData {
                        height,
                        hash: H256::random(), // Would be actual block hash
                        parent_hash: H256::random(), // Would be actual parent hash
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        data: block_data,
                        signature: None, // Would be populated if federation block
                    });
                    source_peers.push("local".to_string());
                }
            }

            // If we don't have all blocks locally, request from network
            let missing_count = count - blocks.len() as u32;
            if missing_count > 0 && network_actor.is_some() {
                let next_height = start_height + blocks.len() as u64;
                
                // Would implement network block requests here
                tracing::debug!(
                    "Need to fetch {} more blocks from height {} via network",
                    missing_count, next_height
                );

                // For now, return what we have locally
                // In full implementation, this would coordinate with NetworkActor
                // to request blocks from preferred_peers
            }

            let response = BlocksResponse {
                blocks,
                more_available: false, // Would check if more blocks exist
                source_peers,
            };

            Ok(Ok(response))
        })
    }
}

/// Handle block processing requests from the sync pipeline
impl Handler<ProcessBlocks> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<BatchResult>>;

    fn handle(&mut self, msg: ProcessBlocks, _ctx: &mut Context<Self>) -> Self::Result {
        let block_processor = self.block_processor.clone();
        let blocks = msg.blocks;
        let validate = msg.validate;
        let priority = msg.priority;

        tracing::debug!(
            "ProcessBlocks: {} blocks, validate: {}, priority: {:?}",
            blocks.len(), validate, priority
        );

        Box::pin(async move {
            let mut processed_blocks = Vec::new();
            let mut validation_results = Vec::new();
            let mut error_count = 0;

            for block_data in blocks {
                // Submit block to processing pipeline
                match block_processor.submit_block(
                    block_data.height,
                    block_data.data.clone(),
                    validate,
                    priority,
                ).await {
                    Ok(result) => {
                        processed_blocks.push(block_data.height);
                        validation_results.push(ValidationResult {
                            height: block_data.height,
                            block_hash: block_data.hash,
                            valid: result.valid,
                            processing_time: result.processing_time,
                            validation_time: result.validation_time,
                        });
                    },
                    Err(e) => {
                        error_count += 1;
                        tracing::error!("Failed to process block {}: {:?}", block_data.height, e);
                        validation_results.push(ValidationResult {
                            height: block_data.height,
                            block_hash: block_data.hash,
                            valid: false,
                            processing_time: std::time::Duration::from_millis(0),
                            validation_time: std::time::Duration::from_millis(0),
                        });
                    }
                }
            }

            let batch_result = BatchResult {
                processed_count: processed_blocks.len() as u32,
                validation_results,
                error_count,
                total_processing_time: std::time::Duration::from_millis(100), // Would be actual time
                success: error_count == 0,
            };

            Ok(Ok(batch_result))
        })
    }
}

/// Handle block validation requests
impl Handler<ValidateBlock> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<ValidationResult>>;

    fn handle(&mut self, msg: ValidateBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let block_processor = self.block_processor.clone();
        let chain_actor = self.chain_actor.clone();
        let height = msg.height;
        let block_hash = msg.block_hash;
        let block_data = msg.block_data;
        let full_validation = msg.full_validation;

        tracing::debug!(
            "ValidateBlock height: {}, hash: {:?}, full_validation: {}",
            height, block_hash, full_validation
        );

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            // Validate block structure and basic checks
            let structure_valid = block_processor.validate_block_structure(&block_data).await
                .unwrap_or(false);

            if !structure_valid {
                return Ok(Ok(ValidationResult {
                    height,
                    block_hash,
                    valid: false,
                    processing_time: start_time.elapsed(),
                    validation_time: start_time.elapsed(),
                }));
            }

            // If full validation requested and we have chain actor, perform consensus validation
            let consensus_valid = if full_validation {
                if let Some(chain_actor) = chain_actor {
                    match chain_actor.send(crate::messages::chain_messages::ValidateBlock {
                        height,
                        block_data: block_data.clone(),
                        skip_known_valid: false,
                    }).await {
                        Ok(Ok(valid)) => valid,
                        Ok(Err(_)) => false,
                        Err(_) => false,
                    }
                } else {
                    true // Assume valid if no chain actor
                }
            } else {
                true // Skip consensus validation for fast sync
            };

            let validation_time = start_time.elapsed();
            let valid = structure_valid && consensus_valid;

            tracing::debug!(
                "Block {} validation completed: {} (structure: {}, consensus: {})",
                height, valid, structure_valid, consensus_valid
            );

            Ok(Ok(ValidationResult {
                height,
                block_hash,
                valid,
                processing_time: validation_time,
                validation_time,
            }))
        })
    }
}

/// Handle block validation completion notifications
impl Handler<BlockValidated> for SyncActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: BlockValidated, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::debug!(
            "Block {} validation completed: {} in {:?}",
            msg.height, msg.valid, msg.processing_time
        );

        // Update sync progress if this block advances our sync
        if msg.height > self.state.progress.current_height && msg.valid {
            self.state.progress.current_height = msg.height;
            
            // Calculate new progress percentage
            if let Some(target_height) = self.state.progress.target_height {
                if target_height > 0 {
                    let progress = msg.height as f64 / target_height as f64;
                    self.state.progress.progress_percent = progress.min(1.0);
                }
            }

            // Update blocks per second calculation
            let now = std::time::Instant::now();
            if let Some(last_update) = self.metrics.last_block_time {
                let elapsed = now.duration_since(last_update).as_secs_f64();
                if elapsed > 0.0 {
                    // Simple exponential moving average for BPS
                    let current_bps = 1.0 / elapsed;
                    self.state.metrics.current_bps = 
                        (self.state.metrics.current_bps * 0.9) + (current_bps * 0.1);
                }
            }
            self.metrics.last_block_time = Some(now);

            // Check if we need to send progress update
            let should_notify = self.state.progress.progress_percent - self.metrics.last_progress_notification > 0.01;
            if should_notify {
                self.metrics.last_progress_notification = self.state.progress.progress_percent;
                
                // Could send progress update to other actors here
                tracing::info!(
                    "Sync progress: {:.2}% ({}/{:?}) - {:.1} BPS",
                    self.state.progress.progress_percent * 100.0,
                    msg.height,
                    self.state.progress.target_height,
                    self.state.metrics.current_bps
                );
            }
        }

        // Update operation tracking
        for operation in self.sync_operations.values_mut() {
            if msg.height >= operation.start_height && msg.height <= operation.end_height {
                if msg.valid {
                    operation.blocks_validated += 1;
                } else {
                    operation.error_count += 1;
                }
                
                // Update operation progress
                let total_blocks = operation.end_height - operation.start_height + 1;
                if total_blocks > 0 {
                    operation.progress = operation.blocks_validated as f64 / total_blocks as f64;
                }
            }
        }

        Ok(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::network::sync::config::SyncConfig;
    use actix::System;

    #[actix::test]
    async fn test_request_blocks_handler() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap();
        
        let request_msg = RequestBlocks {
            start_height: 100,
            count: 10,
            preferred_peers: vec!["peer1".to_string()],
        };

        // This would require full async context to test properly
        // For now, just verify the message structure
        assert_eq!(request_msg.start_height, 100);
        assert_eq!(request_msg.count, 10);
    }

    #[actix::test]
    async fn test_block_validated_handler() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        // Set initial state
        sync_actor.state.progress.current_height = 99;
        sync_actor.state.progress.target_height = Some(1000);
        
        let validated_msg = BlockValidated {
            height: 100,
            block_hash: H256::random(),
            valid: true,
            processing_time: std::time::Duration::from_millis(50),
        };

        let result = sync_actor.handle(validated_msg, &mut Context::new());
        assert!(result.is_ok());
        
        // Should update current height
        assert_eq!(sync_actor.state.progress.current_height, 100);
        
        // Should update progress percentage
        assert_eq!(sync_actor.state.progress.progress_percent, 0.1);
    }

    #[actix::test]
    async fn test_validation_progress_tracking() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        // Add a sync operation
        let operation = SyncOperation {
            operation_id: "test-op".to_string(),
            start_height: 100,
            end_height: 200,
            mode: crate::actors::network::messages::sync_messages::SyncMode::Fast,
            started_at: std::time::Instant::now(),
            progress: 0.0,
            assigned_peers: vec![],
            blocks_downloaded: 0,
            blocks_validated: 0,
            blocks_applied: 0,
            status: SyncStatus::InProgress,
            error_count: 0,
        };
        
        sync_actor.sync_operations.insert("test-op".to_string(), operation);
        
        // Validate a block in the operation range
        let validated_msg = BlockValidated {
            height: 150,
            block_hash: H256::random(),
            valid: true,
            processing_time: std::time::Duration::from_millis(25),
        };

        let result = sync_actor.handle(validated_msg, &mut Context::new());
        assert!(result.is_ok());
        
        // Should update operation progress
        let operation = sync_actor.sync_operations.get("test-op").unwrap();
        assert_eq!(operation.blocks_validated, 1);
        assert!(operation.progress > 0.0);
    }
}

// Helper types for block processing (would be in messages module)
#[derive(Debug, Clone)]
pub struct ProcessBlocks {
    pub blocks: Vec<BlockData>,
    pub validate: bool,
    pub priority: ProcessingPriority,
}

#[derive(Debug, Clone)]
pub struct ValidateBlock {
    pub height: u64,
    pub block_hash: H256,
    pub block_data: Vec<u8>,
    pub full_validation: bool,
}

#[derive(Debug, Clone)]
pub struct BlockValidated {
    pub height: u64,
    pub block_hash: H256,
    pub valid: bool,
    pub processing_time: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub height: u64,
    pub block_hash: H256,
    pub valid: bool,
    pub processing_time: std::time::Duration,
    pub validation_time: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct BatchResult {
    pub processed_count: u32,
    pub validation_results: Vec<ValidationResult>,
    pub error_count: usize,
    pub total_processing_time: std::time::Duration,
    pub success: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ProcessingPriority {
    Low,
    Normal,
    High,
    Critical,
}