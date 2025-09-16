//! Unit tests for Storage Actor components
//!
//! These tests verify the correctness of individual Storage Actor components
//! including database operations, cache behavior, indexing, and message handling.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::actors::storage::database::{DatabaseManager, DatabaseConfig};
    use crate::actors::storage::cache::{StorageCache, CacheConfig};
    use crate::actors::storage::indexing::{StorageIndexing, BlockRange};
    use crate::actors::storage::metrics::StorageActorMetrics;
    use crate::types::*;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::test;

    /// Create a test database configuration
    fn create_test_db_config() -> (DatabaseConfig, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test_db").to_string_lossy().to_string();
        
        let config = DatabaseConfig {
            main_path: db_path,
            archive_path: None,
            cache_size_mb: 16,
            write_buffer_size_mb: 4,
            max_open_files: 50,
            compression_enabled: true,
        };
        
        (config, temp_dir)
    }

    /// Create a test cache configuration
    fn create_test_cache_config() -> CacheConfig {
        CacheConfig {
            max_blocks: 50,
            max_state_entries: 500,
            max_receipts: 250,
            state_ttl: Duration::from_secs(30),
            receipt_ttl: Duration::from_secs(60),
            enable_warming: false,
        }
    }

    /// Create a dummy consensus block for testing
    fn create_test_block(slot: u64, parent_hash: Hash256) -> ConsensusBlock {
        ConsensusBlock {
            parent_hash,
            slot,
            execution_payload: ExecutionPayload {
                parent_hash,
                fee_recipient: Address::zero(),
                state_root: Hash256::random(),
                receipts_root: Hash256::random(),
                logs_bloom: vec![0u8; 256],
                prev_randao: Hash256::random(),
                block_number: slot,
                gas_limit: 30_000_000,
                gas_used: 21_000,
                timestamp: 1234567890 + slot * 2,
                extra_data: vec![],
                base_fee_per_gas: U256::from(1000000000u64), // 1 gwei
                block_hash: Hash256::random(),
                transactions: vec![create_test_transaction()],
                withdrawals: vec![],
                receipts: Some(vec![create_test_receipt()]),
            },
            randao_reveal: vec![0u8; 96],
            signature: vec![0u8; 96],
        }
    }

    /// Create a test Ethereum transaction
    fn create_test_transaction() -> EthereumTransaction {
        EthereumTransaction {
            hash: H256::random(),
            from: Address::random(),
            to: Some(Address::random()),
            value: U256::from(1000000000000000000u64), // 1 ETH
            gas_price: U256::from(20000000000u64), // 20 gwei
            gas_limit: 21000,
            input: vec![],
            nonce: 42,
            v: 27,
            r: U256::from(1),
            s: U256::from(1),
        }
    }

    /// Create a test transaction receipt
    fn create_test_receipt() -> TransactionReceipt {
        TransactionReceipt {
            transaction_hash: H256::random(),
            transaction_index: 0,
            block_hash: Hash256::random(),
            block_number: 1,
            cumulative_gas_used: 21000,
            gas_used: 21000,
            contract_address: None,
            logs: vec![],
            logs_bloom: vec![0u8; 256],
            status: TransactionStatus::Success,
        }
    }

    #[test]
    async fn test_database_block_operations() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");

        // Test block storage and retrieval
        let block = create_test_block(1, Hash256::zero());
        let block_hash = block.hash();

        // Store block
        database.put_block(&block).await
            .expect("Failed to store block");

        // Retrieve block
        let retrieved_block = database.get_block(&block_hash).await
            .expect("Failed to retrieve block")
            .expect("Block not found");

        assert_eq!(retrieved_block.slot, block.slot);
        assert_eq!(retrieved_block.hash(), block_hash);
        assert_eq!(retrieved_block.execution_payload.transactions.len(), 1);
    }

    #[test]
    async fn test_database_chain_head_operations() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");

        // Test chain head storage and retrieval
        let block_ref = BlockRef {
            hash: Hash256::random(),
            height: 42,
        };

        // Store chain head
        database.put_chain_head(&block_ref).await
            .expect("Failed to store chain head");

        // Retrieve chain head
        let retrieved_head = database.get_chain_head().await
            .expect("Failed to retrieve chain head")
            .expect("Chain head not found");

        assert_eq!(retrieved_head.hash, block_ref.hash);
        assert_eq!(retrieved_head.height, block_ref.height);
    }

    #[test]
    async fn test_database_state_operations() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");

        // Test state storage and retrieval
        let key = b"test_state_key".to_vec();
        let value = b"test_state_value".to_vec();

        // Store state
        database.put_state(&key, &value).await
            .expect("Failed to store state");

        // Retrieve state
        let retrieved_value = database.get_state(&key).await
            .expect("Failed to retrieve state")
            .expect("State not found");

        assert_eq!(retrieved_value, value);
    }

    #[test]
    async fn test_database_batch_operations() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");

        // Test batch write operations
        let mut operations = Vec::new();
        
        // Add multiple state operations
        for i in 0..10 {
            let key = format!("batch_key_{}", i).into_bytes();
            let value = format!("batch_value_{}", i).into_bytes();
            operations.push((key, value));
        }

        // Perform batch write
        database.batch_write_state(&operations).await
            .expect("Failed to perform batch write");

        // Verify all operations were applied
        for i in 0..10 {
            let key = format!("batch_key_{}", i).into_bytes();
            let expected_value = format!("batch_value_{}", i).into_bytes();
            
            let retrieved_value = database.get_state(&key).await
                .expect("Failed to retrieve state")
                .expect("State not found");
            
            assert_eq!(retrieved_value, expected_value);
        }
    }

    #[test]
    async fn test_cache_block_operations() {
        let config = create_test_cache_config();
        let cache = StorageCache::new(config);

        // Test block caching
        let block = create_test_block(1, Hash256::zero());
        let block_hash = block.hash();

        // Cache block
        cache.put_block(block_hash, block.clone()).await;

        // Retrieve from cache
        let cached_block = cache.get_block(&block_hash).await
            .expect("Block not found in cache");

        assert_eq!(cached_block.slot, block.slot);
        assert_eq!(cached_block.hash(), block_hash);
    }

    #[test]
    async fn test_cache_state_operations() {
        let config = create_test_cache_config();
        let cache = StorageCache::new(config);

        // Test state caching
        let key = b"test_cache_key".to_vec();
        let value = b"test_cache_value".to_vec();

        // Cache state
        cache.put_state(key.clone(), value.clone()).await;

        // Retrieve from cache
        let cached_value = cache.get_state(&key).await
            .expect("State not found in cache");

        assert_eq!(cached_value, value);
    }

    #[test]
    async fn test_cache_eviction_policy() {
        let mut config = create_test_cache_config();
        config.max_blocks = 3; // Small cache for eviction testing
        let cache = StorageCache::new(config);

        // Fill cache beyond capacity
        let mut blocks = Vec::new();
        for i in 0..5 {
            let block = create_test_block(i, Hash256::zero());
            blocks.push(block.clone());
            cache.put_block(block.hash(), block).await;
        }

        // Check that only the most recent blocks are cached
        let stats = cache.get_stats().await;
        assert!(stats.block_cache_entries <= 3);

        // The most recent blocks should still be cached
        for i in 2..5 {
            let block_hash = blocks[i as usize].hash();
            assert!(cache.get_block(&block_hash).await.is_some(), 
                   "Recent block {} should be cached", i);
        }
    }

    #[test]
    async fn test_cache_ttl_expiration() {
        let mut config = create_test_cache_config();
        config.state_ttl = Duration::from_millis(50); // Very short TTL
        let cache = StorageCache::new(config);

        let key = b"ttl_test_key".to_vec();
        let value = b"ttl_test_value".to_vec();

        // Cache state
        cache.put_state(key.clone(), value.clone()).await;

        // Should be retrievable immediately
        assert!(cache.get_state(&key).await.is_some());

        // Wait for TTL expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Manually trigger cleanup
        cache.cleanup_expired().await;

        // Should be expired now
        assert!(cache.get_state(&key).await.is_none());
    }

    #[test]
    async fn test_indexing_block_operations() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");
        
        let db_handle = database.get_database_handle();
        let mut indexing = StorageIndexing::new(db_handle)
            .expect("Failed to create indexing system");

        // Test block indexing
        let block = create_test_block(1, Hash256::zero());
        let block_hash = block.hash();

        // Index block
        indexing.index_block(&block).await
            .expect("Failed to index block");

        // Test height lookup
        let retrieved_hash = indexing.get_block_hash_by_height(1).await
            .expect("Failed to query height index")
            .expect("Block not found in height index");

        assert_eq!(retrieved_hash, block_hash);

        // Test transaction lookup
        let tx_hash = block.execution_payload.transactions[0].hash();
        let tx_index = indexing.get_transaction_by_hash(&tx_hash).await
            .expect("Failed to query transaction index")
            .expect("Transaction not found in index");

        assert_eq!(tx_index.block_hash, block_hash);
        assert_eq!(tx_index.block_number, 1);
        assert_eq!(tx_index.transaction_index, 0);
    }

    #[test]
    async fn test_indexing_range_queries() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");
        
        let db_handle = database.get_database_handle();
        let mut indexing = StorageIndexing::new(db_handle)
            .expect("Failed to create indexing system");

        // Index multiple blocks
        let mut blocks = Vec::new();
        for i in 0..10 {
            let parent_hash = if i == 0 { Hash256::zero() } else { blocks[i-1].hash() };
            let block = create_test_block(i, parent_hash);
            blocks.push(block.clone());
            
            indexing.index_block(&block).await
                .expect("Failed to index block");
        }

        // Test range query
        let range = BlockRange { start: 2, end: 7 };
        let block_hashes = indexing.get_blocks_in_range(range).await
            .expect("Failed to perform range query");

        assert_eq!(block_hashes.len(), 6); // 2, 3, 4, 5, 6, 7

        // Verify returned hashes match expected blocks
        for (i, hash) in block_hashes.iter().enumerate() {
            let expected_hash = blocks[(i + 2) as usize].hash();
            assert_eq!(*hash, expected_hash);
        }
    }

    #[test]
    async fn test_indexing_address_transactions() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");
        
        let db_handle = database.get_database_handle();
        let mut indexing = StorageIndexing::new(db_handle)
            .expect("Failed to create indexing system");

        let test_address = Address::random();

        // Create blocks with transactions from/to the test address
        let mut blocks = Vec::new();
        for i in 0..5 {
            let mut block = create_test_block(i, Hash256::zero());
            
            // Modify transaction to use test address
            block.execution_payload.transactions[0].from = test_address;
            if i % 2 == 0 {
                block.execution_payload.transactions[0].to = Some(Address::random());
            } else {
                block.execution_payload.transactions[0].to = Some(test_address);
            }
            
            blocks.push(block.clone());
            indexing.index_block(&block).await
                .expect("Failed to index block");
        }

        // Query address transactions
        let address_txs = indexing.get_address_transactions(&test_address, Some(10)).await
            .expect("Failed to query address transactions");

        // Should find transactions where address is sender or recipient
        assert!(address_txs.len() >= 5, "Should find at least 5 transactions");

        // Verify transactions are ordered by block number (most recent first)
        for i in 1..address_txs.len() {
            assert!(address_txs[i-1].block_number >= address_txs[i].block_number);
        }
    }

    #[test]
    async fn test_metrics_collection() {
        let metrics = StorageActorMetrics::new();

        // Test operation metrics
        let operation_time = Duration::from_millis(100);
        metrics.record_block_stored(1, operation_time, true);
        metrics.record_block_stored(2, operation_time, false);

        // Test retrieval metrics
        metrics.record_block_retrieved(Duration::from_millis(10), true);
        metrics.record_block_retrieved(Duration::from_millis(50), false);

        // Test error metrics
        metrics.record_storage_error("database".to_string(), "connection timeout".to_string());
        metrics.record_storage_error("cache".to_string(), "memory limit".to_string());

        // Verify metrics
        assert_eq!(metrics.blocks_stored.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert_eq!(metrics.canonical_blocks_stored.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(metrics.avg_storage_time.load(std::sync::atomic::Ordering::Relaxed) > 0.0);
        assert_eq!(metrics.total_errors(), 2);
    }

    #[test]
    async fn test_concurrent_operations() {
        let (config, _temp_dir) = create_test_db_config();
        let database = Arc::new(DatabaseManager::new(config).await
            .expect("Failed to create database manager"));

        let cache_config = create_test_cache_config();
        let cache = Arc::new(StorageCache::new(cache_config));

        // Test concurrent block operations
        let mut handles = Vec::new();
        
        for i in 0..10 {
            let db_clone = database.clone();
            let cache_clone = cache.clone();
            
            let handle = tokio::spawn(async move {
                let block = create_test_block(i, Hash256::zero());
                let block_hash = block.hash();
                
                // Store in database
                db_clone.put_block(&block).await
                    .expect("Failed to store block");
                
                // Cache block
                cache_clone.put_block(block_hash, block.clone()).await;
                
                // Retrieve and verify
                let retrieved = db_clone.get_block(&block_hash).await
                    .expect("Failed to retrieve block")
                    .expect("Block not found");
                
                assert_eq!(retrieved.slot, block.slot);
                
                let cached = cache_clone.get_block(&block_hash).await
                    .expect("Block not found in cache");
                
                assert_eq!(cached.slot, block.slot);
            });
            
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }
    }

    #[test]
    async fn test_error_handling() {
        // Test database errors
        let invalid_config = DatabaseConfig {
            main_path: "/invalid/path/that/does/not/exist".to_string(),
            archive_path: None,
            cache_size_mb: 16,
            write_buffer_size_mb: 4,
            max_open_files: 50,
            compression_enabled: true,
        };

        let result = DatabaseManager::new(invalid_config).await;
        assert!(result.is_err(), "Should fail with invalid path");

        // Test cache with zero capacity
        let invalid_cache_config = CacheConfig {
            max_blocks: 0,
            max_state_entries: 0,
            max_receipts: 0,
            state_ttl: Duration::from_secs(60),
            receipt_ttl: Duration::from_secs(120),
            enable_warming: false,
        };

        let cache = StorageCache::new(invalid_cache_config);
        let block = create_test_block(1, Hash256::zero());
        
        // Should handle zero capacity gracefully
        cache.put_block(block.hash(), block.clone()).await;
        let retrieved = cache.get_block(&block.hash()).await;
        assert!(retrieved.is_none(), "Should not cache with zero capacity");
    }

    #[test]
    async fn test_data_integrity() {
        let (config, _temp_dir) = create_test_db_config();
        let database = DatabaseManager::new(config).await
            .expect("Failed to create database manager");

        // Test that stored data matches exactly what was retrieved
        let original_block = create_test_block(42, Hash256::random());
        let block_hash = original_block.hash();

        // Store block
        database.put_block(&original_block).await
            .expect("Failed to store block");

        // Retrieve block
        let retrieved_block = database.get_block(&block_hash).await
            .expect("Failed to retrieve block")
            .expect("Block not found");

        // Verify all fields match exactly
        assert_eq!(retrieved_block.slot, original_block.slot);
        assert_eq!(retrieved_block.parent_hash, original_block.parent_hash);
        assert_eq!(retrieved_block.execution_payload.block_number, 
                  original_block.execution_payload.block_number);
        assert_eq!(retrieved_block.execution_payload.state_root,
                  original_block.execution_payload.state_root);
        assert_eq!(retrieved_block.execution_payload.transactions.len(),
                  original_block.execution_payload.transactions.len());
        
        // Verify transaction data
        if !original_block.execution_payload.transactions.is_empty() {
            let original_tx = &original_block.execution_payload.transactions[0];
            let retrieved_tx = &retrieved_block.execution_payload.transactions[0];
            
            assert_eq!(retrieved_tx.hash, original_tx.hash);
            assert_eq!(retrieved_tx.from, original_tx.from);
            assert_eq!(retrieved_tx.to, original_tx.to);
            assert_eq!(retrieved_tx.value, original_tx.value);
            assert_eq!(retrieved_tx.nonce, original_tx.nonce);
        }
    }
}