//! Storage indexing system for the Alys V2 blockchain
//!
//! This module provides advanced indexing capabilities for blocks, transactions,
//! and addresses to enable efficient queries and lookups.

use crate::types::*;
use rocksdb::{DB, ColumnFamily, WriteBatch, Direction, IteratorMode, ReadOptions};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::*;
use serde::{Serialize, Deserialize};

/// Indexing errors
#[derive(Debug, thiserror::Error)]
pub enum IndexingError {
    #[error("Database error: {0}")]
    Database(#[from] rocksdb::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    
    #[error("Index not found: {0}")]
    IndexNotFound(String),
    
    #[error("Invalid range query parameters")]
    InvalidRange,
}

/// Transaction index entry for efficient lookups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionIndex {
    pub block_hash: Hash256,
    pub block_number: u64,
    pub transaction_index: u32,
    pub from_address: Address,
    pub to_address: Option<Address>,
    pub value: U256,
    pub gas_used: u64,
}

/// Address index entry for transaction history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressIndex {
    pub address: Address,
    pub transaction_hash: Hash256,
    pub block_number: u64,
    pub transaction_type: TransactionType,
    pub value: U256,
    pub is_sender: bool,
}

/// Transaction type for indexing purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    Transfer,
    ContractCall,
    ContractDeployment,
    PegIn,
    PegOut,
}

/// Block range for efficient range queries
#[derive(Debug, Clone)]
pub struct BlockRange {
    pub start: u64,
    pub end: u64,
}

/// Storage indexing system
pub struct StorageIndexing {
    db: Arc<RwLock<DB>>,
    block_height_cf: String,
    tx_index_cf: String,
    address_index_cf: String,
    log_index_cf: String,
    stats: IndexingStats,
}

/// Indexing statistics
#[derive(Debug, Default)]
pub struct IndexingStats {
    pub total_indexed_blocks: u64,
    pub total_indexed_transactions: u64,
    pub total_indexed_addresses: u64,
    pub index_size_bytes: u64,
    pub last_indexed_block: u64,
}

impl StorageIndexing {
    /// Create new indexing system
    pub fn new(db: Arc<RwLock<DB>>) -> Result<Self, IndexingError> {
        Ok(StorageIndexing {
            db,
            block_height_cf: "block_heights".to_string(),
            tx_index_cf: "tx_index".to_string(),
            address_index_cf: "address_index".to_string(),
            log_index_cf: "log_index".to_string(),
            stats: IndexingStats::default(),
        })
    }

    /// Index a new block and its transactions
    pub async fn index_block(&mut self, block: &ConsensusBlock) -> Result<(), IndexingError> {
        let block_number = block.slot;
        let block_hash = block.hash();
        
        debug!("Indexing block {} at height {}", block_hash, block_number);
        
        let db = self.db.read().unwrap();
        let mut batch = WriteBatch::default();
        
        // Index block height -> block hash mapping
        self.index_block_height(&mut batch, block_number, &block_hash)?;
        
        // Index transactions in this block
        for (tx_index, transaction) in block.execution_payload.transactions.iter().enumerate() {
            self.index_transaction(&mut batch, &block_hash, block_number, tx_index as u32, transaction)?;
        }
        
        // Index logs from receipts if available
        if let Some(receipts) = &block.execution_payload.receipts {
            for (tx_index, receipt) in receipts.iter().enumerate() {
                self.index_logs(&mut batch, &block_hash, block_number, tx_index as u32, &receipt.logs)?;
            }
        }
        
        // Write batch to database
        db.write(batch)?;
        
        // Update statistics
        self.stats.total_indexed_blocks += 1;
        self.stats.last_indexed_block = block_number;
        
        debug!("Successfully indexed block {} with {} transactions", 
               block_hash, block.execution_payload.transactions.len());
        
        Ok(())
    }
    
    /// Index block height to hash mapping
    fn index_block_height(&self, batch: &mut WriteBatch, height: u64, hash: &Hash256) -> Result<(), IndexingError> {
        let height_key = height.to_be_bytes();
        let hash_value = bincode::serialize(hash)?;
        
        let cf = self.get_column_family(&self.block_height_cf)?;
        batch.put_cf(&cf, height_key, hash_value);
        
        Ok(())
    }
    
    /// Index a transaction for efficient lookups
    fn index_transaction(&mut self, batch: &mut WriteBatch, block_hash: &Hash256, 
                        block_number: u64, tx_index: u32, transaction: &EthereumTransaction) -> Result<(), IndexingError> {
        let tx_hash = transaction.hash();
        
        // Create transaction index entry
        let tx_index_entry = TransactionIndex {
            block_hash: *block_hash,
            block_number,
            transaction_index: tx_index,
            from_address: transaction.from,
            to_address: transaction.to,
            value: transaction.value,
            gas_used: transaction.gas_limit, // Will be updated with actual gas used from receipt
        };
        
        // Index by transaction hash
        let tx_key = tx_hash.as_bytes();
        let tx_value = bincode::serialize(&tx_index_entry)?;
        
        let tx_cf = self.get_column_family(&self.tx_index_cf)?;
        batch.put_cf(&tx_cf, tx_key, tx_value);
        
        // Index by sender address
        self.index_address_transaction(batch, &transaction.from, &tx_hash, block_number, 
                                     TransactionType::from_transaction(transaction), transaction.value, true)?;
        
        // Index by recipient address if present
        if let Some(to_address) = transaction.to {
            self.index_address_transaction(batch, &to_address, &tx_hash, block_number,
                                         TransactionType::from_transaction(transaction), transaction.value, false)?;
        }
        
        self.stats.total_indexed_transactions += 1;
        Ok(())
    }
    
    /// Index address to transaction mapping
    fn index_address_transaction(&self, batch: &mut WriteBatch, address: &Address, tx_hash: &Hash256,
                                block_number: u64, tx_type: TransactionType, value: U256, is_sender: bool) -> Result<(), IndexingError> {
        let address_index = AddressIndex {
            address: *address,
            transaction_hash: *tx_hash,
            block_number,
            transaction_type: tx_type,
            value,
            is_sender,
        };
        
        // Use address + block_number + tx_hash as composite key for ordering
        let mut key = Vec::new();
        key.extend_from_slice(address.as_bytes());
        key.extend_from_slice(&block_number.to_be_bytes());
        key.extend_from_slice(tx_hash.as_bytes());
        
        let value = bincode::serialize(&address_index)?;
        
        let addr_cf = self.get_column_family(&self.address_index_cf)?;
        batch.put_cf(&addr_cf, key, value);
        
        Ok(())
    }
    
    /// Index logs from transaction receipts
    fn index_logs(&self, batch: &mut WriteBatch, block_hash: &Hash256, block_number: u64,
                 tx_index: u32, logs: &[EthereumLog]) -> Result<(), IndexingError> {
        for (log_index, log) in logs.iter().enumerate() {
            // Create composite key: block_hash + tx_index + log_index
            let mut key = Vec::new();
            key.extend_from_slice(block_hash.as_bytes());
            key.extend_from_slice(&tx_index.to_be_bytes());
            key.extend_from_slice(&(log_index as u32).to_be_bytes());
            
            let value = bincode::serialize(log)?;
            
            let log_cf = self.get_column_family(&self.log_index_cf)?;
            batch.put_cf(&log_cf, key, value);
        }
        
        Ok(())
    }
    
    /// Get block hash by height
    pub async fn get_block_hash_by_height(&self, height: u64) -> Result<Option<Hash256>, IndexingError> {
        let db = self.db.read().unwrap();
        let cf = self.get_column_family(&self.block_height_cf)?;
        
        let height_key = height.to_be_bytes();
        match db.get_cf(&cf, height_key)? {
            Some(hash_bytes) => {
                let hash: Hash256 = bincode::deserialize(&hash_bytes)?;
                Ok(Some(hash))
            },
            None => Ok(None),
        }
    }
    
    /// Get transaction information by hash
    pub async fn get_transaction_by_hash(&self, tx_hash: &Hash256) -> Result<Option<TransactionIndex>, IndexingError> {
        let db = self.db.read().unwrap();
        let cf = self.get_column_family(&self.tx_index_cf)?;
        
        match db.get_cf(&cf, tx_hash.as_bytes())? {
            Some(tx_bytes) => {
                let tx_index: TransactionIndex = bincode::deserialize(&tx_bytes)?;
                Ok(Some(tx_index))
            },
            None => Ok(None),
        }
    }
    
    /// Get transaction history for an address
    pub async fn get_address_transactions(&self, address: &Address, limit: Option<usize>) -> Result<Vec<AddressIndex>, IndexingError> {
        let db = self.db.read().unwrap();
        let cf = self.get_column_family(&self.address_index_cf)?;
        
        let mut transactions = Vec::new();
        let prefix = address.as_bytes();
        let iter = db.prefix_iterator_cf(&cf, prefix);
        
        for (i, result) in iter.enumerate() {
            if let Some(limit) = limit {
                if i >= limit {
                    break;
                }
            }
            
            let (_key, value) = result?;
            let addr_index: AddressIndex = bincode::deserialize(&value)?;
            transactions.push(addr_index);
        }
        
        // Sort by block number (most recent first)
        transactions.sort_by(|a, b| b.block_number.cmp(&a.block_number));
        
        Ok(transactions)
    }
    
    /// Perform range query for blocks
    pub async fn get_blocks_in_range(&self, range: BlockRange) -> Result<Vec<Hash256>, IndexingError> {
        if range.start > range.end {
            return Err(IndexingError::InvalidRange);
        }
        
        let db = self.db.read().unwrap();
        let cf = self.get_column_family(&self.block_height_cf)?;
        
        let mut blocks = Vec::new();
        let start_key = range.start.to_be_bytes();
        let end_key = range.end.to_be_bytes();
        
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_upper_bound(&end_key);
        
        let iter = db.iterator_cf_opt(&cf, read_opts, IteratorMode::From(&start_key, Direction::Forward));
        
        for result in iter {
            let (_key, value) = result?;
            let hash: Hash256 = bincode::deserialize(&value)?;
            blocks.push(hash);
        }
        
        Ok(blocks)
    }
    
    /// Search logs by topics and address filters
    pub async fn search_logs(&self, address_filter: Option<Address>, 
                            topics: Vec<Hash256>, from_block: u64, to_block: u64) -> Result<Vec<EthereumLog>, IndexingError> {
        let db = self.db.read().unwrap();
        let cf = self.get_column_family(&self.log_index_cf)?;
        
        let mut matching_logs = Vec::new();
        
        // Get all blocks in range first
        let block_range = BlockRange { start: from_block, end: to_block };
        let block_hashes = self.get_blocks_in_range(block_range).await?;
        
        // Search logs in each block
        for block_hash in block_hashes {
            let prefix = block_hash.as_bytes();
            let iter = db.prefix_iterator_cf(&cf, prefix);
            
            for result in iter {
                let (_key, value) = result?;
                let log: EthereumLog = bincode::deserialize(&value)?;
                
                // Apply filters
                if let Some(addr_filter) = address_filter {
                    if log.address != addr_filter {
                        continue;
                    }
                }
                
                // Check topic filters
                if !topics.is_empty() {
                    let mut topic_match = false;
                    for topic in &topics {
                        if log.topics.contains(topic) {
                            topic_match = true;
                            break;
                        }
                    }
                    if !topic_match {
                        continue;
                    }
                }
                
                matching_logs.push(log);
            }
        }
        
        Ok(matching_logs)
    }
    
    /// Get indexing statistics
    pub async fn get_stats(&self) -> IndexingStats {
        self.stats.clone()
    }
    
    /// Rebuild indices for a range of blocks
    pub async fn rebuild_indices(&mut self, start_block: u64, end_block: u64) -> Result<(), IndexingError> {
        info!("Rebuilding indices for blocks {} to {}", start_block, end_block);
        
        // This would iterate through stored blocks and re-index them
        // Implementation would depend on how blocks are stored in the main database
        
        warn!("Index rebuilding not yet implemented");
        Ok(())
    }
    
    /// Helper function to get column family handle
    fn get_column_family(&self, cf_name: &str) -> Result<ColumnFamily, IndexingError> {
        let db = self.db.read().unwrap();
        db.cf_handle(cf_name)
            .ok_or_else(|| IndexingError::IndexNotFound(cf_name.to_string()))
    }
}

impl TransactionType {
    /// Determine transaction type from Ethereum transaction
    fn from_transaction(tx: &EthereumTransaction) -> Self {
        // Basic heuristics - could be enhanced with more sophisticated detection
        if tx.to.is_none() {
            TransactionType::ContractDeployment
        } else if tx.value > U256::zero() {
            TransactionType::Transfer
        } else {
            TransactionType::ContractCall
        }
    }
}

impl Clone for IndexingStats {
    fn clone(&self) -> Self {
        IndexingStats {
            total_indexed_blocks: self.total_indexed_blocks,
            total_indexed_transactions: self.total_indexed_transactions,
            total_indexed_addresses: self.total_indexed_addresses,
            index_size_bytes: self.index_size_bytes,
            last_indexed_block: self.last_indexed_block,
        }
    }
}