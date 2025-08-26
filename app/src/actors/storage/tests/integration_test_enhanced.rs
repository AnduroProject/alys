//! Enhanced Integration tests for Storage Actor with full indexing support
//!
//! These tests verify that the Storage Actor correctly integrates with ChainActor
//! and other components of the Alys V2 system, including advanced indexing features.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::actors::storage::actor::{StorageActor, StorageConfig};
    use crate::actors::storage::database::DatabaseConfig;
    use crate::actors::storage::cache::CacheConfig;
    use crate::actors::storage::indexing::BlockRange;
    use crate::types::*;
    use super::mock_helpers::{TestDataGenerator, StorageTestFixture, StorageAssertions};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::test;

    /// Create enhanced test configuration with indexing support
    fn create_enhanced_test_config() -> (StorageConfig, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("enhanced_test_storage").to_string_lossy().to_string();
        
        let config = StorageConfig {
            database: DatabaseConfig {
                main_path: db_path,
                archive_path: None,
                cache_size_mb: 64, // Larger cache for testing
                write_buffer_size_mb: 16,
                max_open_files: 200,
                compression_enabled: true,
            },
            cache: CacheConfig {
                max_blocks: 200,
                max_state_entries: 2000,
                max_receipts: 1000,
                state_ttl: Duration::from_secs(300),
                receipt_ttl: Duration::from_secs(600),
                enable_warming: true,
            },
            write_batch_size: 50,
            sync_interval: Duration::from_secs(1),
            maintenance_interval: Duration::from_secs(60),
            enable_auto_compaction: true,
            metrics_reporting_interval: Duration::from_secs(30),
        };
        
        (config, temp_dir)
    }

    #[test]
    async fn test_enhanced_storage_actor_creation_with_indexing() {
        let (config, _temp_dir) = create_enhanced_test_config();
        
        // Create storage actor with indexing enabled
        let storage_actor = StorageActor::new(config).await
            .expect("Failed to create enhanced storage actor");
        
        // Verify components are properly initialized
        assert!(storage_actor.database.get_stats().await.is_ok());
        assert!(storage_actor.indexing.read().unwrap().get_stats().await.total_indexed_blocks == 0);
        
        let cache_stats = storage_actor.cache.get_stats().await;
        assert_eq!(cache_stats.block_cache_entries, 0);
        assert_eq!(cache_stats.state_cache_entries, 0);
    }

    #[test]
    async fn test_full_block_storage_and_indexing_pipeline() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(10, 5); // 10 blocks, 5 transactions each
        
        println!("Testing full pipeline with {} blocks", test_blocks.len());

        // Store all blocks and verify indexing
        for (i, block) in test_blocks.iter().enumerate() {
            let block_hash = block.hash();
            let height = block.slot;
            
            // Store block (this should automatically index it)
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
            
            // Verify block storage
            let retrieved_block = storage_actor.get_block(&block_hash).await
                .expect("Failed to retrieve block")
                .expect("Block not found");
            
            StorageAssertions::assert_blocks_equal(block, &retrieved_block);
            
            // Verify indexing worked
            let indexed_hash = storage_actor.indexing.read().unwrap()
                .get_block_hash_by_height(height).await
                .expect("Failed to query height index")
                .expect("Block not found in height index");
            
            assert_eq!(indexed_hash, block_hash, "Indexed hash doesn't match for block {}", i);
        }

        // Test range queries
        let range = BlockRange { start: 2, end: 7 };
        let range_hashes = storage_actor.indexing.read().unwrap()
            .get_blocks_in_range(range).await
            .expect("Failed to perform range query");
        
        assert_eq!(range_hashes.len(), 6, "Range query should return 6 blocks");
        
        for (i, hash) in range_hashes.iter().enumerate() {
            let expected_hash = test_blocks[i + 2].hash();
            assert_eq!(*hash, expected_hash, "Range query hash mismatch at index {}", i);
        }

        println!("✅ Full pipeline test completed successfully");
    }

    #[test]
    async fn test_transaction_indexing_and_queries() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(5, 10); // 5 blocks, 10 transactions each
        
        // Store blocks with transaction indexing
        for block in &test_blocks {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
        }

        // Test transaction lookups by hash
        for (block_idx, block) in test_blocks.iter().enumerate() {
            for (tx_idx, tx) in block.execution_payload.transactions.iter().enumerate() {
                let tx_hash = tx.hash();
                
                // Query transaction by hash
                let tx_info = storage_actor.indexing.read().unwrap()
                    .get_transaction_by_hash(&tx_hash).await
                    .expect("Failed to query transaction")
                    .expect("Transaction not found in index");
                
                assert_eq!(tx_info.block_hash, block.hash());
                assert_eq!(tx_info.block_number, block.slot);
                assert_eq!(tx_info.transaction_index, tx_idx as u32);
                assert_eq!(tx_info.from_address, tx.from);
                assert_eq!(tx_info.to_address, tx.to);
                assert_eq!(tx_info.value, tx.value);
            }
        }

        println!("✅ Transaction indexing test completed successfully");
    }

    #[test]
    async fn test_address_transaction_history() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let test_address = Address::random();
        let mut generator = TestDataGenerator::new();
        
        // Create blocks where transactions involve the test address
        let mut test_blocks = Vec::new();
        for i in 0..5 {
            let mut block = generator.generate_block_with_parent(i, Hash256::zero(), 3, 1234567890 + i * 2);
            
            // Modify first transaction to involve test address as sender
            block.execution_payload.transactions[0].from = test_address;
            
            // Modify second transaction to involve test address as recipient
            if block.execution_payload.transactions.len() > 1 {
                block.execution_payload.transactions[1].to = Some(test_address);
            }
            
            test_blocks.push(block);
        }

        // Store blocks
        for block in &test_blocks {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
        }

        // Query address transaction history
        let address_txs = storage_actor.indexing.read().unwrap()
            .get_address_transactions(&test_address, Some(20)).await
            .expect("Failed to query address transactions");

        // Should find at least 10 transactions (2 per block * 5 blocks)
        assert!(address_txs.len() >= 10, "Should find at least 10 transactions, found {}", address_txs.len());

        // Verify transactions are sorted by block number (most recent first)
        for i in 1..address_txs.len() {
            assert!(address_txs[i-1].block_number >= address_txs[i].block_number,
                   "Address transactions should be sorted by block number");
        }

        // Verify address involvement
        for addr_tx in &address_txs {
            assert_eq!(addr_tx.address, test_address);
        }

        println!("✅ Address transaction history test completed successfully");
    }

    #[test]
    async fn test_cache_and_database_integration() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(20, 3); // More blocks than cache can hold
        
        // Store blocks (should populate both cache and database)
        for block in &test_blocks {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
        }

        // Test cache hits for recent blocks
        let recent_blocks = &test_blocks[test_blocks.len()-5..]; // Last 5 blocks
        for block in recent_blocks {
            let cached_block = storage_actor.cache.get_block(&block.hash()).await;
            assert!(cached_block.is_some(), "Recent block should be cached");
            
            let cached = cached_block.unwrap();
            StorageAssertions::assert_blocks_equal(block, &cached);
        }

        // Test database retrieval for all blocks
        for block in &test_blocks {
            let db_block = storage_actor.database.get_block(&block.hash()).await
                .expect("Failed to retrieve from database")
                .expect("Block not found in database");
            
            StorageAssertions::assert_blocks_equal(block, &db_block);
        }

        // Test cache statistics
        let cache_stats = storage_actor.cache.get_stats().await;
        StorageAssertions::assert_cache_stats_reasonable(&cache_stats);
        
        assert!(cache_stats.block_cache_entries > 0, "Cache should contain blocks");
        assert!(cache_stats.overall_hit_rate() >= 0.0, "Hit rate should be non-negative");

        println!("✅ Cache and database integration test completed successfully");
    }

    #[test]
    async fn test_state_storage_and_retrieval() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        // Test state operations
        let state_entries = vec![
            (b"account_balance_0x123".to_vec(), b"1000000000000000000".to_vec()), // 1 ETH
            (b"contract_storage_0x456_slot_1".to_vec(), b"0x789abc".to_vec()),
            (b"nonce_0x123".to_vec(), b"42".to_vec()),
        ];

        // Store state entries
        for (key, value) in &state_entries {
            storage_actor.database.put_state(key, value).await
                .expect("Failed to store state");
            
            // Also cache them
            storage_actor.cache.put_state(key.clone(), value.clone()).await;
        }

        // Retrieve and verify state entries
        for (key, expected_value) in &state_entries {
            // Test cache retrieval
            let cached_value = storage_actor.cache.get_state(key).await
                .expect("State not found in cache");
            assert_eq!(&cached_value, expected_value, "Cached state value mismatch");
            
            // Test database retrieval
            let db_value = storage_actor.database.get_state(key).await
                .expect("Failed to retrieve state from database")
                .expect("State not found in database");
            assert_eq!(&db_value, expected_value, "Database state value mismatch");
        }

        println!("✅ State storage and retrieval test completed successfully");
    }

    #[test]
    async fn test_maintenance_operations() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(10, 5);
        
        // Store blocks
        for block in &test_blocks {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
        }

        // Test database compaction
        let pre_compact_stats = storage_actor.database.get_stats().await
            .expect("Failed to get pre-compaction stats");
        
        storage_actor.database.compact_database().await
            .expect("Failed to compact database");
        
        let post_compact_stats = storage_actor.database.get_stats().await
            .expect("Failed to get post-compaction stats");
        
        // Compaction should maintain data integrity
        assert_eq!(post_compact_stats.total_blocks, pre_compact_stats.total_blocks,
                  "Block count should remain the same after compaction");

        // Test cache flush
        let pre_flush_stats = storage_actor.cache.get_stats().await;
        assert!(pre_flush_stats.block_cache_entries > 0, "Cache should have entries before flush");
        
        storage_actor.cache.clear_all().await;
        
        let post_flush_stats = storage_actor.cache.get_stats().await;
        assert_eq!(post_flush_stats.block_cache_entries, 0, "Cache should be empty after flush");

        // Verify data can still be retrieved from database
        for block in &test_blocks[..3] { // Test subset
            let retrieved = storage_actor.database.get_block(&block.hash()).await
                .expect("Failed to retrieve block after maintenance")
                .expect("Block not found after maintenance");
            
            StorageAssertions::assert_blocks_equal(block, &retrieved);
        }

        println!("✅ Maintenance operations test completed successfully");
    }

    #[test]
    async fn test_error_recovery_and_resilience() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_block = generator.generate_block_with_parent(1, Hash256::zero(), 3, 1234567890);
        
        // Test successful storage
        storage_actor.store_block(test_block.clone(), true).await
            .expect("Failed to store test block");

        // Verify block was stored
        let retrieved = storage_actor.get_block(&test_block.hash()).await
            .expect("Failed to retrieve block")
            .expect("Block not found");
        
        StorageAssertions::assert_blocks_equal(&test_block, &retrieved);

        // Test retrieval of non-existent block
        let fake_hash = Hash256::random();
        let result = storage_actor.get_block(&fake_hash).await
            .expect("Query should succeed even for non-existent block");
        
        assert!(result.is_none(), "Non-existent block should return None");

        // Test invalid state queries
        let invalid_key = b"non_existent_key".to_vec();
        let state_result = storage_actor.database.get_state(&invalid_key).await
            .expect("State query should succeed for non-existent key");
        
        assert!(state_result.is_none(), "Non-existent state should return None");

        println!("✅ Error recovery and resilience test completed successfully");
    }

    #[test]
    async fn test_concurrent_storage_operations() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let storage_actor = Arc::new(tokio::sync::Mutex::new(
            StorageActor::new(config).await.expect("Failed to create storage actor")
        ));

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(20, 2);
        
        // Split blocks among concurrent workers
        let chunks: Vec<Vec<ConsensusBlock>> = test_blocks.chunks(4).map(|chunk| chunk.to_vec()).collect();
        let mut handles = Vec::new();

        for (worker_id, chunk) in chunks.into_iter().enumerate() {
            let actor_clone = storage_actor.clone();
            
            let handle = tokio::spawn(async move {
                for block in chunk {
                    let mut actor = actor_clone.lock().await;
                    
                    // Store block
                    actor.store_block(block.clone(), true).await
                        .expect("Failed to store block in worker");
                    
                    // Retrieve and verify
                    let retrieved = actor.get_block(&block.hash()).await
                        .expect("Failed to retrieve block in worker")
                        .expect("Block not found in worker");
                    
                    assert_eq!(retrieved.slot, block.slot, "Worker {} block mismatch", worker_id);
                }
                
                worker_id
            });
            
            handles.push(handle);
        }

        // Wait for all workers
        for handle in handles {
            let worker_id = handle.await.expect("Worker failed");
            println!("Worker {} completed successfully", worker_id);
        }

        // Verify all blocks are accessible
        let actor = storage_actor.lock().await;
        for block in &test_blocks {
            let retrieved = actor.database.get_block(&block.hash()).await
                .expect("Failed to retrieve block after concurrent operations")
                .expect("Block not found after concurrent operations");
            
            assert_eq!(retrieved.slot, block.slot);
        }

        println!("✅ Concurrent operations test completed successfully");
    }

    #[test]
    async fn test_indexing_consistency_after_operations() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(15, 8);
        
        // Store blocks
        for block in &test_blocks {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
        }

        // Verify indexing consistency
        let indexing_stats = storage_actor.indexing.read().unwrap().get_stats().await;
        assert_eq!(indexing_stats.total_indexed_blocks, test_blocks.len() as u64,
                  "All blocks should be indexed");
        
        let expected_tx_count = test_blocks.iter()
            .map(|b| b.execution_payload.transactions.len() as u64)
            .sum::<u64>();
        assert_eq!(indexing_stats.total_indexed_transactions, expected_tx_count,
                  "All transactions should be indexed");

        // Test that all blocks can be found by height
        for (i, block) in test_blocks.iter().enumerate() {
            let indexed_hash = storage_actor.indexing.read().unwrap()
                .get_block_hash_by_height(i as u64).await
                .expect("Failed to query by height")
                .expect("Block not found in height index");
            
            assert_eq!(indexed_hash, block.hash(), "Height index inconsistency at block {}", i);
        }

        // Test that all transactions can be found by hash
        for block in &test_blocks[..5] { // Test subset for performance
            for tx in &block.execution_payload.transactions {
                let tx_info = storage_actor.indexing.read().unwrap()
                    .get_transaction_by_hash(&tx.hash()).await
                    .expect("Failed to query transaction")
                    .expect("Transaction not found in index");
                
                assert_eq!(tx_info.block_hash, block.hash());
                assert_eq!(tx_info.block_number, block.slot);
            }
        }

        println!("✅ Indexing consistency test completed successfully");
    }

    #[test]
    async fn test_metrics_and_monitoring() {
        let (config, _temp_dir) = create_enhanced_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");

        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(5, 3);
        
        // Initial metrics should be zero
        assert_eq!(storage_actor.metrics.blocks_stored.load(std::sync::atomic::Ordering::Relaxed), 0);
        
        // Store blocks and check metrics updates
        for (i, block) in test_blocks.iter().enumerate() {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
            
            let stored_count = storage_actor.metrics.blocks_stored.load(std::sync::atomic::Ordering::Relaxed);
            assert_eq!(stored_count, (i + 1) as u64, "Stored block count should increment");
        }

        // Test retrieval metrics
        let initial_retrievals = storage_actor.metrics.blocks_retrieved.load(std::sync::atomic::Ordering::Relaxed);
        
        for block in &test_blocks[..3] {
            let _retrieved = storage_actor.get_block(&block.hash()).await
                .expect("Failed to retrieve block");
        }
        
        let final_retrievals = storage_actor.metrics.blocks_retrieved.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(final_retrievals - initial_retrievals, 3, "Retrieved block count should increment");

        // Check cache statistics
        let cache_stats = storage_actor.cache.get_stats().await;
        StorageAssertions::assert_cache_stats_reasonable(&cache_stats);

        // Check database statistics  
        let db_stats = storage_actor.database.get_stats().await
            .expect("Failed to get database stats");
        StorageAssertions::assert_database_stats_reasonable(&db_stats);

        println!("✅ Metrics and monitoring test completed successfully");
    }
}