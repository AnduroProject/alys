//! Mock helpers and test utilities for Storage Actor testing
//!
//! This module provides mock implementations, test fixtures, and helper
//! functions to support comprehensive testing of the Storage Actor system.

use crate::types::*;
use crate::actors::storage::database::{DatabaseManager, DatabaseConfig, DatabaseStats};
use crate::actors::storage::cache::{StorageCache, CacheConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use rand::Rng;

/// Mock database for testing that simulates database operations in memory
pub struct MockDatabase {
    blocks: Arc<Mutex<HashMap<Hash256, ConsensusBlock>>>,
    state: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
    receipts: Arc<Mutex<HashMap<H256, TransactionReceipt>>>,
    chain_head: Arc<Mutex<Option<BlockRef>>>,
    operation_delay: Duration,
    fail_probability: f64,
    pub operation_count: Arc<Mutex<u64>>,
}

impl MockDatabase {
    /// Create a new mock database
    pub fn new() -> Self {
        MockDatabase {
            blocks: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(HashMap::new())),
            receipts: Arc::new(Mutex::new(HashMap::new())),
            chain_head: Arc::new(Mutex::new(None)),
            operation_delay: Duration::from_millis(0),
            fail_probability: 0.0,
            operation_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a mock database that simulates slow operations
    pub fn new_slow(delay: Duration) -> Self {
        let mut db = Self::new();
        db.operation_delay = delay;
        db
    }

    /// Create a mock database that occasionally fails
    pub fn new_unreliable(fail_probability: f64) -> Self {
        let mut db = Self::new();
        db.fail_probability = fail_probability;
        db
    }

    /// Simulate operation delay and potential failure
    async fn simulate_operation(&self) -> Result<(), StorageError> {
        // Increment operation count
        {
            let mut count = self.operation_count.lock().unwrap();
            *count += 1;
        }

        // Simulate delay
        if self.operation_delay > Duration::from_millis(0) {
            tokio::time::sleep(self.operation_delay).await;
        }

        // Simulate random failures
        if self.fail_probability > 0.0 {
            let mut rng = rand::thread_rng();
            if rng.gen::<f64>() < self.fail_probability {
                return Err(StorageError::Database("Simulated database failure".to_string()));
            }
        }

        Ok(())
    }

    /// Store a block in the mock database
    pub async fn put_block(&self, block: &ConsensusBlock) -> Result<(), StorageError> {
        self.simulate_operation().await?;
        
        let mut blocks = self.blocks.lock().unwrap();
        blocks.insert(block.hash(), block.clone());
        Ok(())
    }

    /// Retrieve a block from the mock database
    pub async fn get_block(&self, hash: &Hash256) -> Result<Option<ConsensusBlock>, StorageError> {
        self.simulate_operation().await?;
        
        let blocks = self.blocks.lock().unwrap();
        Ok(blocks.get(hash).cloned())
    }

    /// Store state in the mock database
    pub async fn put_state(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.simulate_operation().await?;
        
        let mut state = self.state.lock().unwrap();
        state.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    /// Retrieve state from the mock database
    pub async fn get_state(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        self.simulate_operation().await?;
        
        let state = self.state.lock().unwrap();
        Ok(state.get(key).cloned())
    }

    /// Store chain head
    pub async fn put_chain_head(&self, head: &BlockRef) -> Result<(), StorageError> {
        self.simulate_operation().await?;
        
        let mut chain_head = self.chain_head.lock().unwrap();
        *chain_head = Some(head.clone());
        Ok(())
    }

    /// Get chain head
    pub async fn get_chain_head(&self) -> Result<Option<BlockRef>, StorageError> {
        self.simulate_operation().await?;
        
        let chain_head = self.chain_head.lock().unwrap();
        Ok(chain_head.clone())
    }

    /// Get mock database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats, StorageError> {
        self.simulate_operation().await?;
        
        let blocks = self.blocks.lock().unwrap();
        let state = self.state.lock().unwrap();
        let receipts = self.receipts.lock().unwrap();
        
        Ok(DatabaseStats {
            total_size_bytes: (blocks.len() * 1024 + state.len() * 64 + receipts.len() * 256) as u64,
            total_blocks: blocks.len() as u64,
            total_state_entries: state.len() as u64,
            total_receipts: receipts.len() as u64,
            compaction_pending: false,
        })
    }

    /// Get number of operations performed
    pub fn get_operation_count(&self) -> u64 {
        *self.operation_count.lock().unwrap()
    }

    /// Reset operation count
    pub fn reset_operation_count(&self) {
        let mut count = self.operation_count.lock().unwrap();
        *count = 0;
    }
}

/// Test data generator for creating realistic blockchain test scenarios
pub struct TestDataGenerator {
    rng: rand::rngs::ThreadRng,
}

impl TestDataGenerator {
    pub fn new() -> Self {
        TestDataGenerator {
            rng: rand::thread_rng(),
        }
    }

    /// Generate a chain of connected blocks
    pub fn generate_block_chain(&mut self, length: usize, tx_per_block: usize) -> Vec<ConsensusBlock> {
        let mut chain = Vec::with_capacity(length);
        let mut parent_hash = Hash256::zero();
        let base_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for i in 0..length {
            let block = self.generate_block_with_parent(
                i as u64,
                parent_hash,
                tx_per_block,
                base_timestamp + (i as u64 * 2), // 2 second block times
            );
            parent_hash = block.hash();
            chain.push(block);
        }

        chain
    }

    /// Generate a block with specific parent
    pub fn generate_block_with_parent(
        &mut self,
        slot: u64,
        parent_hash: Hash256,
        tx_count: usize,
        timestamp: u64,
    ) -> ConsensusBlock {
        let mut transactions = Vec::with_capacity(tx_count);
        let mut receipts = Vec::with_capacity(tx_count);

        for i in 0..tx_count {
            let tx = self.generate_transaction(i as u64);
            let receipt = self.generate_receipt(&tx, slot, i as u32);
            transactions.push(tx);
            receipts.push(receipt);
        }

        ConsensusBlock {
            parent_hash,
            slot,
            execution_payload: ExecutionPayload {
                parent_hash,
                fee_recipient: self.random_address(),
                state_root: Hash256::random(),
                receipts_root: Hash256::random(),
                logs_bloom: vec![0u8; 256],
                prev_randao: Hash256::random(),
                block_number: slot,
                gas_limit: 30_000_000,
                gas_used: transactions.iter().map(|tx| tx.gas_limit).sum(),
                timestamp,
                extra_data: vec![],
                base_fee_per_gas: U256::from(self.rng.gen_range(1_000_000_000u64..10_000_000_000u64)),
                block_hash: Hash256::random(),
                transactions,
                withdrawals: if slot % 10 == 0 { self.generate_withdrawals() } else { vec![] },
                receipts: Some(receipts),
            },
            randao_reveal: vec![0u8; 96],
            signature: vec![0u8; 96],
        }
    }

    /// Generate a realistic transaction
    pub fn generate_transaction(&mut self, nonce: u64) -> EthereumTransaction {
        let tx_type = self.rng.gen_range(0..4);
        
        EthereumTransaction {
            hash: H256::random(),
            from: self.random_address(),
            to: match tx_type {
                0 => None, // Contract deployment
                _ => Some(self.random_address()),
            },
            value: match tx_type {
                1 => U256::from(self.rng.gen_range(1_000_000_000_000_000u64..10_000_000_000_000_000_000u64)), // 0.001 to 10 ETH
                _ => U256::zero(), // Contract calls typically have 0 value
            },
            gas_price: U256::from(self.rng.gen_range(1_000_000_000u64..100_000_000_000u64)), // 1-100 gwei
            gas_limit: match tx_type {
                0 => self.rng.gen_range(200_000..2_000_000), // Contract deployment
                1 => 21_000, // Simple transfer
                _ => self.rng.gen_range(50_000..500_000), // Contract call
            },
            input: match tx_type {
                0 => self.generate_bytecode(), // Contract deployment
                2 | 3 => self.generate_call_data(), // Contract call
                _ => vec![], // Simple transfer
            },
            nonce,
            v: 27 + (self.rng.gen::<u8>() % 2),
            r: U256::from(self.rng.gen::<u64>()),
            s: U256::from(self.rng.gen::<u64>()),
        }
    }

    /// Generate a transaction receipt
    pub fn generate_receipt(&mut self, tx: &EthereumTransaction, block_number: u64, tx_index: u32) -> TransactionReceipt {
        let success = self.rng.gen_range(0..100) < 95; // 95% success rate
        let logs = if success && tx.to.is_some() {
            self.generate_logs(tx_index)
        } else {
            vec![]
        };

        TransactionReceipt {
            transaction_hash: tx.hash(),
            transaction_index: tx_index,
            block_hash: Hash256::random(),
            block_number,
            cumulative_gas_used: (tx_index as u64 + 1) * 21_000, // Simplified
            gas_used: if success { 
                std::cmp::min(tx.gas_limit, self.rng.gen_range(15_000..tx.gas_limit + 1))
            } else {
                tx.gas_limit // Failed transactions consume all gas
            },
            contract_address: if tx.to.is_none() { Some(self.random_address()) } else { None },
            logs,
            logs_bloom: vec![0u8; 256], // Simplified
            status: if success {
                TransactionStatus::Success
            } else {
                match self.rng.gen_range(0..3) {
                    0 => TransactionStatus::Failed,
                    _ => TransactionStatus::Reverted { 
                        reason: Some("Execution reverted".to_string()) 
                    },
                }
            },
        }
    }

    /// Generate contract bytecode
    fn generate_bytecode(&mut self) -> Vec<u8> {
        let size = self.rng.gen_range(100..2000);
        (0..size).map(|_| self.rng.gen()).collect()
    }

    /// Generate contract call data
    fn generate_call_data(&mut self) -> Vec<u8> {
        let size = self.rng.gen_range(4..200);
        (0..size).map(|_| self.rng.gen()).collect()
    }

    /// Generate event logs
    fn generate_logs(&mut self, tx_index: u32) -> Vec<EventLog> {
        let log_count = self.rng.gen_range(0..5);
        (0..log_count).enumerate().map(|(i, _)| {
            let topic_count = self.rng.gen_range(1..5);
            let topics = (0..topic_count).map(|_| H256::random()).collect();
            
            EventLog {
                address: self.random_address(),
                topics,
                data: (0..self.rng.gen_range(0..200)).map(|_| self.rng.gen()).collect(),
                block_hash: Hash256::random(),
                block_number: 0, // Will be set by caller
                transaction_hash: H256::random(),
                transaction_index: tx_index,
                log_index: i as u32,
                removed: false,
            }
        }).collect()
    }

    /// Generate withdrawal records
    fn generate_withdrawals(&mut self) -> Vec<Withdrawal> {
        let count = self.rng.gen_range(0..10);
        (0..count).map(|i| Withdrawal {
            index: i as u64,
            validator_index: self.rng.gen_range(0..1_000_000),
            address: self.random_address(),
            amount: self.rng.gen_range(1_000_000..1_000_000_000), // Gwei
        }).collect()
    }

    /// Generate random address
    fn random_address(&mut self) -> Address {
        Address::from([
            self.rng.gen(), self.rng.gen(), self.rng.gen(), self.rng.gen(),
            self.rng.gen(), self.rng.gen(), self.rng.gen(), self.rng.gen(),
            self.rng.gen(), self.rng.gen(), self.rng.gen(), self.rng.gen(),
            self.rng.gen(), self.rng.gen(), self.rng.gen(), self.rng.gen(),
            self.rng.gen(), self.rng.gen(), self.rng.gen(), self.rng.gen(),
        ])
    }
}

/// Test fixture for creating consistent test environments
pub struct StorageTestFixture {
    pub temp_dir: TempDir,
    pub database_config: DatabaseConfig,
    pub cache_config: CacheConfig,
    pub test_blocks: Vec<ConsensusBlock>,
    pub mock_database: Option<MockDatabase>,
}

impl StorageTestFixture {
    /// Create a new test fixture with default configuration
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test_storage").to_string_lossy().to_string();
        
        let database_config = DatabaseConfig {
            main_path: db_path,
            archive_path: None,
            cache_size_mb: 32,
            write_buffer_size_mb: 8,
            max_open_files: 100,
            compression_enabled: true,
        };
        
        let cache_config = CacheConfig {
            max_blocks: 100,
            max_state_entries: 1000,
            max_receipts: 500,
            state_ttl: Duration::from_secs(60),
            receipt_ttl: Duration::from_secs(120),
            enable_warming: false,
        };

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(10, 5);

        StorageTestFixture {
            temp_dir,
            database_config,
            cache_config,
            test_blocks,
            mock_database: None,
        }
    }

    /// Create a test fixture with mock database
    pub fn with_mock_database() -> Self {
        let mut fixture = Self::new();
        fixture.mock_database = Some(MockDatabase::new());
        fixture
    }

    /// Create a test fixture optimized for performance testing
    pub fn for_performance_testing() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("perf_test_storage").to_string_lossy().to_string();
        
        let database_config = DatabaseConfig {
            main_path: db_path,
            archive_path: None,
            cache_size_mb: 128,
            write_buffer_size_mb: 32,
            max_open_files: 1000,
            compression_enabled: false, // Faster for testing
        };
        
        let cache_config = CacheConfig {
            max_blocks: 1000,
            max_state_entries: 10000,
            max_receipts: 5000,
            state_ttl: Duration::from_secs(300),
            receipt_ttl: Duration::from_secs(600),
            enable_warming: true,
        };

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(100, 20); // Larger dataset

        StorageTestFixture {
            temp_dir,
            database_config,
            cache_config,
            test_blocks,
            mock_database: None,
        }
    }

    /// Get a specific test block by index
    pub fn get_test_block(&self, index: usize) -> Option<&ConsensusBlock> {
        self.test_blocks.get(index)
    }

    /// Get all test block hashes
    pub fn get_test_block_hashes(&self) -> Vec<Hash256> {
        self.test_blocks.iter().map(|b| b.hash()).collect()
    }

    /// Get test transactions from all blocks
    pub fn get_test_transactions(&self) -> Vec<&EthereumTransaction> {
        self.test_blocks
            .iter()
            .flat_map(|b| &b.execution_payload.transactions)
            .collect()
    }

    /// Get unique addresses from test data
    pub fn get_test_addresses(&self) -> Vec<Address> {
        let mut addresses = std::collections::HashSet::new();
        
        for block in &self.test_blocks {
            for tx in &block.execution_payload.transactions {
                addresses.insert(tx.from);
                if let Some(to) = tx.to {
                    addresses.insert(to);
                }
            }
        }
        
        addresses.into_iter().collect()
    }
}

/// Assertion helpers for testing storage operations
pub struct StorageAssertions;

impl StorageAssertions {
    /// Assert that two blocks are equivalent
    pub fn assert_blocks_equal(expected: &ConsensusBlock, actual: &ConsensusBlock) {
        assert_eq!(expected.slot, actual.slot, "Block slots don't match");
        assert_eq!(expected.parent_hash, actual.parent_hash, "Parent hashes don't match");
        assert_eq!(expected.hash(), actual.hash(), "Block hashes don't match");
        assert_eq!(
            expected.execution_payload.transactions.len(),
            actual.execution_payload.transactions.len(),
            "Transaction count doesn't match"
        );
        
        for (i, (expected_tx, actual_tx)) in expected.execution_payload.transactions
            .iter()
            .zip(&actual.execution_payload.transactions)
            .enumerate() 
        {
            assert_eq!(expected_tx.hash(), actual_tx.hash(), "Transaction {} hash doesn't match", i);
            assert_eq!(expected_tx.from, actual_tx.from, "Transaction {} from doesn't match", i);
            assert_eq!(expected_tx.to, actual_tx.to, "Transaction {} to doesn't match", i);
            assert_eq!(expected_tx.value, actual_tx.value, "Transaction {} value doesn't match", i);
        }
    }

    /// Assert cache statistics are within expected ranges
    pub fn assert_cache_stats_reasonable(stats: &crate::actors::storage::cache::StorageCacheStats) {
        assert!(stats.overall_hit_rate() <= 1.0, "Hit rate cannot exceed 100%");
        assert!(stats.overall_hit_rate() >= 0.0, "Hit rate cannot be negative");
        assert!(stats.total_memory_bytes > 0, "Cache should use some memory");
    }

    /// Assert database statistics are reasonable
    pub fn assert_database_stats_reasonable(stats: &DatabaseStats) {
        assert!(stats.total_size_bytes > 0, "Database should have some size");
        assert!(
            stats.total_blocks >= stats.total_receipts || stats.total_receipts == 0,
            "Cannot have more receipts than blocks"
        );
    }

    /// Assert performance metrics meet minimum requirements
    pub fn assert_performance_acceptable(
        operations: u64,
        duration: Duration,
        min_ops_per_second: f64,
    ) {
        let actual_rate = operations as f64 / duration.as_secs_f64();
        assert!(
            actual_rate >= min_ops_per_second,
            "Performance {} ops/sec is below minimum {} ops/sec",
            actual_rate,
            min_ops_per_second
        );
    }
}

/// Utility functions for test setup and cleanup
pub mod test_utils {
    use super::*;
    use std::future::Future;
    use std::time::Instant;

    /// Time a future and return both the result and duration
    pub async fn time_async<F, T>(future: F) -> (T, Duration)
    where
        F: Future<Output = T>,
    {
        let start = Instant::now();
        let result = future.await;
        let duration = start.elapsed();
        (result, duration)
    }

    /// Run a test with timeout
    pub async fn with_timeout<F, T>(
        future: F,
        timeout: Duration,
    ) -> Result<T, tokio::time::error::Elapsed>
    where
        F: Future<Output = T>,
    {
        tokio::time::timeout(timeout, future).await
    }

    /// Generate random test data of specified size
    pub fn generate_random_data(size: usize) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        (0..size).map(|_| rng.gen()).collect()
    }

    /// Create a temporary directory for testing
    pub fn create_temp_dir(prefix: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("Failed to create temporary directory")
    }

    /// Wait for a condition to become true or timeout
    pub async fn wait_for_condition<F>(
        mut condition: F,
        timeout: Duration,
        check_interval: Duration,
    ) -> bool
    where
        F: FnMut() -> bool,
    {
        let start = Instant::now();
        
        while start.elapsed() < timeout {
            if condition() {
                return true;
            }
            tokio::time::sleep(check_interval).await;
        }
        
        false
    }
}

// Re-export commonly used test types
pub use rand;
pub use tempfile;