//! Block production workflow
//! 
//! This workflow orchestrates the complex process of creating new blocks,
//! including transaction selection, payload building, and consensus validation.

use crate::types::*;
use tracing::*;

/// Workflow for producing new blocks
#[derive(Debug)]
pub struct BlockProductionWorkflow {
    config: ChainConfig,
    transaction_pool: TransactionPool,
    gas_estimator: GasEstimator,
    metrics: ProductionMetrics,
}

/// Configuration for block production
#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub slot_duration: std::time::Duration,
    pub max_blocks_without_pow: u64,
    pub is_validator: bool,
    pub federation: Vec<Address>,
    pub gas_limit: u64,
    pub base_fee_per_gas: U256,
}

/// Transaction pool for block production
#[derive(Debug)]
pub struct TransactionPool {
    pending_transactions: std::collections::HashMap<H256, PendingTransaction>,
    queued_transactions: std::collections::BTreeMap<(Address, u64), PendingTransaction>,
    max_pool_size: usize,
}

/// Gas estimation utility
#[derive(Debug)]
pub struct GasEstimator {
    base_fee: U256,
    gas_used_history: std::collections::VecDeque<u64>,
}

/// Block production metrics
#[derive(Debug, Default)]
pub struct ProductionMetrics {
    pub blocks_produced: u64,
    pub transactions_included: u64,
    pub average_block_time: std::time::Duration,
    pub gas_utilization: f64,
}

/// Pending transaction in the pool
#[derive(Debug, Clone)]
pub struct PendingTransaction {
    pub transaction: Transaction,
    pub added_at: std::time::Instant,
    pub gas_price: U256,
    pub priority_fee: U256,
}

/// Transaction selection criteria
#[derive(Debug, Clone)]
pub struct TransactionSelectionCriteria {
    pub max_transactions: usize,
    pub max_gas: u64,
    pub min_gas_price: U256,
    pub prioritize_fee: bool,
}

/// Block building result
#[derive(Debug, Clone)]
pub struct BlockBuildResult {
    pub block: ConsensusBlock,
    pub execution_payload: ExecutionPayload,
    pub selected_transactions: Vec<Transaction>,
    pub total_gas_used: u64,
    pub block_reward: U256,
}

impl BlockProductionWorkflow {
    pub fn new(config: ChainConfig) -> Self {
        Self {
            config: config.clone(),
            transaction_pool: TransactionPool::new(10_000), // Max 10k transactions
            gas_estimator: GasEstimator::new(config.base_fee_per_gas),
            metrics: ProductionMetrics::default(),
        }
    }

    /// Create a new block with optimal transaction selection
    pub async fn create_block(
        &mut self,
        parent_head: Option<&BlockRef>,
        config: &ChainConfig,
    ) -> Result<ConsensusBlock, ChainError> {
        info!("Starting block production workflow");
        
        let start_time = std::time::Instant::now();
        
        // Step 1: Validate we can produce blocks
        self.validate_production_conditions(parent_head, config).await?;
        
        // Step 2: Select optimal transactions
        let selection_criteria = TransactionSelectionCriteria {
            max_transactions: 1000,
            max_gas: config.gas_limit,
            min_gas_price: self.gas_estimator.get_min_gas_price(),
            prioritize_fee: true,
        };
        
        let selected_transactions = self.select_transactions(selection_criteria).await?;
        
        // Step 3: Build execution payload
        let execution_payload = self.build_execution_payload(
            parent_head,
            &selected_transactions,
        ).await?;
        
        // Step 4: Create consensus block
        let block = self.create_consensus_block(
            parent_head,
            execution_payload,
            selected_transactions.clone(),
        ).await?;
        
        // Step 5: Update metrics
        let production_time = start_time.elapsed();
        self.update_metrics(&block, &selected_transactions, production_time);
        
        info!("Block production completed: {} with {} transactions", 
              block.hash(), selected_transactions.len());
        
        Ok(block)
    }

    /// Validate that we can produce blocks
    async fn validate_production_conditions(
        &self,
        parent_head: Option<&BlockRef>,
        config: &ChainConfig,
    ) -> Result<(), ChainError> {
        // Check if we're a validator
        if !config.is_validator {
            return Err(ChainError::NotValidator);
        }
        
        // Check if we have a valid parent
        if parent_head.is_none() {
            return Err(ChainError::NoParentBlock);
        }
        
        // Check if enough time has passed since last block
        let parent = parent_head.unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let slot_duration_secs = config.slot_duration.as_secs();
        if now < parent.number * slot_duration_secs + slot_duration_secs {
            return Err(ChainError::TooEarly);
        }
        
        Ok(())
    }

    /// Select optimal transactions for the block
    async fn select_transactions(
        &mut self,
        criteria: TransactionSelectionCriteria,
    ) -> Result<Vec<Transaction>, ChainError> {
        info!("Selecting transactions for block production");
        
        let mut selected = Vec::new();
        let mut total_gas = 0u64;
        
        // Sort transactions by priority (gas price descending)
        let mut candidates: Vec<_> = self.transaction_pool.pending_transactions
            .values()
            .filter(|tx| tx.gas_price >= criteria.min_gas_price)
            .collect();
        
        if criteria.prioritize_fee {
            candidates.sort_by(|a, b| b.priority_fee.cmp(&a.priority_fee));
        }
        
        // Select transactions until we hit limits
        for pending_tx in candidates {
            if selected.len() >= criteria.max_transactions {
                break;
            }
            
            let estimated_gas = self.estimate_transaction_gas(&pending_tx.transaction).await?;
            
            if total_gas + estimated_gas > criteria.max_gas {
                continue; // Skip transactions that would exceed gas limit
            }
            
            selected.push(pending_tx.transaction.clone());
            total_gas += estimated_gas;
        }
        
        info!("Selected {} transactions using {} gas", selected.len(), total_gas);
        Ok(selected)
    }

    /// Build execution payload for the block
    async fn build_execution_payload(
        &self,
        parent_head: Option<&BlockRef>,
        transactions: &[Transaction],
    ) -> Result<ExecutionPayload, ChainError> {
        info!("Building execution payload");
        
        let parent_hash = parent_head
            .map(|h| h.hash)
            .unwrap_or_default();
        
        let block_number = parent_head
            .map(|h| h.number + 1)
            .unwrap_or(1);
        
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Calculate gas used
        let mut total_gas_used = 0u64;
        for tx in transactions {
            total_gas_used += self.estimate_transaction_gas(tx).await?;
        }
        
        // Serialize transactions
        let serialized_transactions: Vec<Vec<u8>> = transactions
            .iter()
            .map(|tx| self.serialize_transaction(tx))
            .collect::<Result<Vec<_>, _>>()?;
        
        let payload = ExecutionPayload {
            block_hash: BlockHash::default(), // Will be calculated later
            parent_hash,
            fee_recipient: Address::zero(), // TODO: Use actual fee recipient
            state_root: Hash256::default(), // Will be calculated by execution client
            receipts_root: Hash256::default(), // Will be calculated by execution client
            logs_bloom: vec![0u8; 256], // Will be calculated by execution client
            prev_randao: Hash256::default(), // TODO: Use actual randao
            block_number,
            gas_limit: self.config.gas_limit,
            gas_used: total_gas_used,
            timestamp,
            extra_data: b"Alys".to_vec(),
            base_fee_per_gas: self.config.base_fee_per_gas,
            transactions: serialized_transactions,
            withdrawals: None, // Alys doesn't support withdrawals yet
        };
        
        Ok(payload)
    }

    /// Create the final consensus block
    async fn create_consensus_block(
        &self,
        parent_head: Option<&BlockRef>,
        execution_payload: ExecutionPayload,
        transactions: Vec<Transaction>,
    ) -> Result<ConsensusBlock, ChainError> {
        info!("Creating consensus block");
        
        let parent_hash = parent_head
            .map(|h| h.hash)
            .unwrap_or_default();
        
        let block_number = parent_head
            .map(|h| h.number + 1)
            .unwrap_or(1);
        
        // Calculate transactions root
        let tx_hashes: Vec<H256> = transactions
            .iter()
            .map(|tx| tx.hash)
            .collect();
        let transactions_root = self.calculate_merkle_root(&tx_hashes);
        
        let block = ConsensusBlock {
            header: BlockHeader {
                parent_hash,
                transactions_root,
                state_root: execution_payload.state_root,
                receipts_root: execution_payload.receipts_root,
                logs_bloom: execution_payload.logs_bloom.clone(),
                number: block_number,
                gas_limit: execution_payload.gas_limit,
                gas_used: execution_payload.gas_used,
                timestamp: execution_payload.timestamp,
                extra_data: execution_payload.extra_data.clone(),
                base_fee_per_gas: execution_payload.base_fee_per_gas,
            },
            transactions,
            signature: None, // Will be added by consensus layer
        };
        
        Ok(block)
    }

    /// Estimate gas usage for a transaction
    async fn estimate_transaction_gas(&self, transaction: &Transaction) -> Result<u64, ChainError> {
        // Simplified gas estimation
        // TODO: Integrate with actual EVM execution for accurate estimation
        
        let base_gas = 21_000u64; // Base transaction cost
        let data_gas = transaction.data.len() as u64 * 16; // 16 gas per byte
        
        let estimated = base_gas + data_gas;
        
        // Cap at the transaction's gas limit
        Ok(estimated.min(transaction.gas_limit))
    }

    /// Serialize a transaction for execution payload
    fn serialize_transaction(&self, transaction: &Transaction) -> Result<Vec<u8>, ChainError> {
        // TODO: Implement proper transaction serialization (RLP encoding)
        // For now, return placeholder
        Ok(transaction.hash.as_bytes().to_vec())
    }

    /// Calculate merkle root of transaction hashes
    fn calculate_merkle_root(&self, hashes: &[H256]) -> Hash256 {
        // TODO: Implement proper merkle tree calculation
        // For now, return hash of concatenated hashes
        if hashes.is_empty() {
            return Hash256::default();
        }
        
        // Simple implementation - hash all hashes together
        let mut hasher = sha2::Sha256::new();
        for hash in hashes {
            hasher.update(hash.as_bytes());
        }
        let result = hasher.finalize();
        Hash256::from_slice(&result)
    }

    /// Update production metrics
    fn update_metrics(
        &mut self,
        block: &ConsensusBlock,
        transactions: &[Transaction],
        production_time: std::time::Duration,
    ) {
        self.metrics.blocks_produced += 1;
        self.metrics.transactions_included += transactions.len() as u64;
        
        // Update average block time (simple moving average)
        let total_time = self.metrics.average_block_time.as_millis() as u64
            * (self.metrics.blocks_produced - 1)
            + production_time.as_millis() as u64;
        self.metrics.average_block_time = std::time::Duration::from_millis(
            total_time / self.metrics.blocks_produced
        );
        
        // Update gas utilization
        self.metrics.gas_utilization = 
            (block.header.gas_used as f64) / (block.header.gas_limit as f64);
        
        debug!("Production metrics updated: blocks={}, avg_time={}ms, gas_util={:.2}%",
               self.metrics.blocks_produced,
               self.metrics.average_block_time.as_millis(),
               self.metrics.gas_utilization * 100.0);
    }
}

impl TransactionPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            pending_transactions: std::collections::HashMap::new(),
            queued_transactions: std::collections::BTreeMap::new(),
            max_pool_size: max_size,
        }
    }
}

impl GasEstimator {
    pub fn new(base_fee: U256) -> Self {
        Self {
            base_fee,
            gas_used_history: std::collections::VecDeque::with_capacity(100),
        }
    }
    
    pub fn get_min_gas_price(&self) -> U256 {
        // Return base fee as minimum
        self.base_fee
    }
}