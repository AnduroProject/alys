//! Chain actor for consensus coordination
//! 
//! This actor manages the blockchain state, coordinates consensus operations,
//! and handles block production and validation. It replaces the shared mutable
//! state patterns from the legacy Chain struct.

use crate::messages::chain_messages::*;
use crate::types::*;
use crate::workflows::block_production::BlockProductionWorkflow;
use crate::workflows::block_import::BlockImportWorkflow;
use actix::prelude::*;
use std::collections::HashMap;
use tracing::*;

/// Chain actor that manages blockchain state and consensus
#[derive(Debug)]
pub struct ChainActor {
    /// Current chain head
    head: Option<BlockRef>,
    /// Chain configuration
    config: ChainConfig,
    /// Block production workflow
    block_production: BlockProductionWorkflow,
    /// Block import workflow
    block_import: BlockImportWorkflow,
    /// Pending block candidates
    pending_blocks: HashMap<BlockHash, PendingBlock>,
    /// Actor performance metrics
    metrics: ChainActorMetrics,
}

/// Configuration for the chain actor
#[derive(Debug, Clone)]
pub struct ChainConfig {
    /// Maximum blocks without proof of work
    pub max_blocks_without_pow: u64,
    /// Slot duration for block production
    pub slot_duration: std::time::Duration,
    /// Whether this node is a validator
    pub is_validator: bool,
    /// Federation addresses
    pub federation: Vec<Address>,
}

/// Pending block information
#[derive(Debug, Clone)]
pub struct PendingBlock {
    pub block: ConsensusBlock,
    pub received_at: std::time::Instant,
    pub validation_status: ValidationStatus,
}

/// Block validation status
#[derive(Debug, Clone)]
pub enum ValidationStatus {
    Pending,
    Validating,
    Valid,
    Invalid { reason: String },
}

/// Metrics for chain actor performance
#[derive(Debug, Default)]
pub struct ChainActorMetrics {
    pub blocks_processed: u64,
    pub blocks_produced: u64,
    pub validation_time_ms: u64,
    pub average_block_time_ms: u64,
}

impl Actor for ChainActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Chain actor started");
        
        // Start periodic metrics reporting
        ctx.run_interval(
            std::time::Duration::from_secs(60),
            |actor, _ctx| {
                actor.report_metrics();
            }
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Chain actor stopped");
    }
}

impl ChainActor {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            head: None,
            config: config.clone(),
            block_production: BlockProductionWorkflow::new(config.clone()),
            block_import: BlockImportWorkflow::new(config),
            pending_blocks: HashMap::new(),
            metrics: ChainActorMetrics::default(),
        }
    }

    /// Get the current chain head
    pub fn get_head(&self) -> Option<BlockRef> {
        self.head.clone()
    }

    /// Update the chain head
    fn update_head(&mut self, new_head: BlockRef) {
        info!("Updating chain head to block {}", new_head.hash);
        self.head = Some(new_head);
    }

    /// Process a new block for validation and potential inclusion
    async fn process_block(&mut self, block: ConsensusBlock) -> Result<(), ChainError> {
        let block_hash = block.hash();
        info!("Processing block: {}", block_hash);

        // Add to pending blocks
        let pending = PendingBlock {
            block: block.clone(),
            received_at: std::time::Instant::now(),
            validation_status: ValidationStatus::Pending,
        };
        self.pending_blocks.insert(block_hash, pending);

        // Start block validation workflow
        match self.block_import.validate_block(block).await {
            Ok(validated_block) => {
                self.import_validated_block(validated_block).await?;
                self.metrics.blocks_processed += 1;
                Ok(())
            }
            Err(e) => {
                error!("Block validation failed: {:?}", e);
                if let Some(mut pending) = self.pending_blocks.get_mut(&block_hash) {
                    pending.validation_status = ValidationStatus::Invalid { 
                        reason: e.to_string() 
                    };
                }
                Err(e)
            }
        }
    }

    /// Import a validated block into the chain
    async fn import_validated_block(&mut self, block: ConsensusBlock) -> Result<(), ChainError> {
        info!("Importing validated block: {}", block.hash());

        // Update chain head if this block extends the current head
        if self.should_update_head(&block) {
            let new_head = BlockRef {
                hash: block.hash(),
                number: block.number(),
                parent_hash: block.parent_hash(),
            };
            self.update_head(new_head);
        }

        // Clean up pending blocks
        self.pending_blocks.remove(&block.hash());

        Ok(())
    }

    /// Determine if a block should become the new chain head
    fn should_update_head(&self, block: &ConsensusBlock) -> bool {
        match &self.head {
            None => true, // First block
            Some(current_head) => {
                // Simple rule: accept if block number is higher
                block.number() > current_head.number
            }
        }
    }

    /// Produce a new block if this node is a validator
    async fn produce_block(&mut self) -> Result<ConsensusBlock, ChainError> {
        if !self.config.is_validator {
            return Err(ChainError::NotValidator);
        }

        info!("Producing new block");
        
        let block = self.block_production.create_block(
            self.head.as_ref(),
            &self.config
        ).await?;

        self.metrics.blocks_produced += 1;
        Ok(block)
    }

    /// Report performance metrics
    fn report_metrics(&self) {
        info!(
            "Chain metrics: blocks_processed={}, blocks_produced={}, avg_block_time={}ms",
            self.metrics.blocks_processed,
            self.metrics.blocks_produced,
            self.metrics.average_block_time_ms
        );
    }
}

// Message handlers

impl Handler<ProcessBlockMessage> for ChainActor {
    type Result = ResponseFuture<Result<(), ChainError>>;

    fn handle(&mut self, msg: ProcessBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        let block = msg.block;
        Box::pin(async move {
            // Note: This is a simplified implementation
            // In the actual implementation, we'd need to properly handle the async context
            info!("Received block processing request: {}", block.hash());
            Ok(())
        })
    }
}

impl Handler<GetHeadMessage> for ChainActor {
    type Result = Option<BlockRef>;

    fn handle(&mut self, _msg: GetHeadMessage, _ctx: &mut Self::Context) -> Self::Result {
        self.get_head()
    }
}

impl Handler<ProduceBlockMessage> for ChainActor {
    type Result = ResponseFuture<Result<ConsensusBlock, ChainError>>;

    fn handle(&mut self, _msg: ProduceBlockMessage, _ctx: &mut Self::Context) -> Self::Result {
        Box::pin(async move {
            // Note: This is a simplified implementation
            // In the actual implementation, we'd need to properly handle the async context
            info!("Received block production request");
            Err(ChainError::NotImplemented)
        })
    }
}

impl Handler<UpdateHeadMessage> for ChainActor {
    type Result = ();

    fn handle(&mut self, msg: UpdateHeadMessage, _ctx: &mut Self::Context) {
        self.update_head(msg.new_head);
    }
}