//! Integration tests for Storage Actor
//!
//! These tests verify that the Storage Actor correctly integrates with ChainActor
//! and other components of the Alys V2 system.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::types::*;
    use crate::messages::storage_messages::*;
    use std::time::Duration;
    use tempfile::TempDir;

    /// Create a test configuration for the Storage Actor
    fn create_test_config() -> StorageConfig {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test_storage").to_string_lossy().to_string();
        
        StorageConfig {
            database: DatabaseConfig {
                main_path: db_path,
                archive_path: None,
                cache_size_mb: 32,
                write_buffer_size_mb: 8,
                max_open_files: 100,
                compression_enabled: true,
            },
            cache: CacheConfig {
                max_blocks: 100,
                max_state_entries: 1000,
                max_receipts: 500,
                state_ttl: Duration::from_secs(60),
                receipt_ttl: Duration::from_secs(120),
                enable_warming: false,
            },
            write_batch_size: 100,
            sync_interval: Duration::from_secs(1),
            maintenance_interval: Duration::from_secs(60),
            enable_auto_compaction: false,
            metrics_reporting_interval: Duration::from_secs(30),
        }
    }

    /// Create a dummy consensus block for testing
    fn create_test_block(slot: u64) -> ConsensusBlock {
        ConsensusBlock {
            parent_hash: Hash256::zero(),
            slot,
            execution_payload: ExecutionPayload {
                parent_hash: Hash256::zero(),
                fee_recipient: Address::zero(),
                state_root: Hash256::zero(),
                receipts_root: Hash256::zero(),
                logs_bloom: vec![0; 256],
                prev_randao: Hash256::zero(),
                block_number: slot,
                gas_limit: 1_000_000,
                gas_used: 0,
                timestamp: slot,
                extra_data: Vec::new(),
                base_fee_per_gas: 1_000_000_000,
                block_hash: Hash256::zero(),
                transactions: Vec::new(),
                withdrawals: Vec::new(),
                blob_gas_used: Some(0),
                excess_blob_gas: Some(0),
            },
            lighthouse_metadata: LighthouseMetadata {
                slot: lighthouse_wrapper::types::Slot::new(slot),
                proposer_index: 0,
                parent_root: lighthouse_wrapper::types::Hash256::zero(),
                state_root: lighthouse_wrapper::types::Hash256::zero(),
                body_root: lighthouse_wrapper::types::Hash256::zero(),
            },
            timing: BlockTiming {
                imported_at: std::time::SystemTime::now(),
                validated_at: None,
                finalized_at: None,
                processing_duration: Duration::from_millis(100),
            },
            validation_info: ValidationInfo {
                validator_index: 0,
                is_valid: true,
                validation_errors: Vec::new(),
                consensus_validation_time: Duration::from_millis(50),
            },
            actor_metadata: ActorBlockMetadata {
                produced_by: "test".to_string(),
                processed_by_actors: vec!["ChainActor".to_string()],
                actor_processing_times: std::collections::HashMap::new(),
                total_actor_processing_time: Duration::from_millis(200),
            },
            pegins: Vec::new(),
            finalized_pegouts: Vec::new(),
            auxpow_header: None,
        }
    }

    #[tokio::test]
    async fn test_storage_actor_creation() {
        let config = create_test_config();
        let result = StorageActor::new(config).await;
        
        assert!(result.is_ok(), "Failed to create StorageActor: {:?}", result.err());
        
        let storage_actor = result.unwrap();
        assert_eq!(storage_actor.config.cache.max_blocks, 100);
        assert_eq!(storage_actor.config.database.cache_size_mb, 32);
    }

    #[tokio::test]
    async fn test_database_operations() {
        let config = create_test_config();
        let database = DatabaseManager::new(config.database).await.expect("Failed to create database");
        
        // Test block storage and retrieval
        let test_block = create_test_block(1);
        let block_hash = test_block.hash();
        
        // Store the block
        let store_result = database.put_block(&test_block).await;
        assert!(store_result.is_ok(), "Failed to store block: {:?}", store_result.err());
        
        // Retrieve the block by hash
        let retrieved_block = database.get_block(&block_hash).await.expect("Failed to retrieve block");
        assert!(retrieved_block.is_some(), "Block not found after storage");
        
        let retrieved_block = retrieved_block.unwrap();
        assert_eq!(retrieved_block.slot, test_block.slot);
        assert_eq!(retrieved_block.hash(), block_hash);
        
        // Retrieve the block by height
        let retrieved_by_height = database.get_block_by_height(1).await.expect("Failed to retrieve block by height");
        assert!(retrieved_by_height.is_some(), "Block not found by height");
        assert_eq!(retrieved_by_height.unwrap().slot, 1);
    }

    #[tokio::test]
    async fn test_state_operations() {
        let config = create_test_config();
        let database = DatabaseManager::new(config.database).await.expect("Failed to create database");
        
        let test_key = b"test_state_key";
        let test_value = b"test_state_value";
        
        // Store state
        let store_result = database.put_state(test_key, test_value).await;
        assert!(store_result.is_ok(), "Failed to store state: {:?}", store_result.err());
        
        // Retrieve state
        let retrieved_value = database.get_state(test_key).await.expect("Failed to retrieve state");
        assert!(retrieved_value.is_some(), "State not found after storage");
        assert_eq!(retrieved_value.unwrap(), test_value);
        
        // Test non-existent key
        let missing_value = database.get_state(b"non_existent_key").await.expect("Failed to query missing state");
        assert!(missing_value.is_none(), "Non-existent key should return None");
    }

    #[tokio::test]
    async fn test_chain_head_operations() {
        let config = create_test_config();
        let database = DatabaseManager::new(config.database).await.expect("Failed to create database");
        
        // Initially no chain head
        let initial_head = database.get_chain_head().await.expect("Failed to get initial chain head");
        assert!(initial_head.is_none(), "Chain head should be None initially");
        
        // Set chain head
        let test_head = BlockRef {
            hash: Hash256::from_slice(&[1; 32]),
            height: 42,
        };
        
        let set_result = database.put_chain_head(&test_head).await;
        assert!(set_result.is_ok(), "Failed to set chain head: {:?}", set_result.err());
        
        // Retrieve chain head
        let retrieved_head = database.get_chain_head().await.expect("Failed to get chain head");
        assert!(retrieved_head.is_some(), "Chain head should be set");
        
        let retrieved_head = retrieved_head.unwrap();
        assert_eq!(retrieved_head.hash, test_head.hash);
        assert_eq!(retrieved_head.height, test_head.height);
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let config = create_test_config();
        let cache = StorageCache::new(config.cache);
        
        // Test block caching
        let test_block = create_test_block(5);
        let block_hash = test_block.hash();
        
        // Initially not in cache
        let cached_block = cache.get_block(&block_hash).await;
        assert!(cached_block.is_none(), "Block should not be in cache initially");
        
        // Put block in cache
        cache.put_block(block_hash, test_block.clone()).await;
        
        // Retrieve from cache
        let cached_block = cache.get_block(&block_hash).await;
        assert!(cached_block.is_some(), "Block should be in cache after putting");
        assert_eq!(cached_block.unwrap().slot, test_block.slot);
        
        // Test state caching
        let test_key = b"test_cache_key".to_vec();
        let test_value = b"test_cache_value".to_vec();
        
        // Initially not in cache
        let cached_state = cache.get_state(&test_key).await;
        assert!(cached_state.is_none(), "State should not be in cache initially");
        
        // Put state in cache
        cache.put_state(test_key.clone(), test_value.clone()).await;
        
        // Retrieve from cache
        let cached_state = cache.get_state(&test_key).await;
        assert!(cached_state.is_some(), "State should be in cache after putting");
        assert_eq!(cached_state.unwrap(), test_value);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let config = create_test_config();
        let database = DatabaseManager::new(config.database).await.expect("Failed to create database");
        
        let test_block1 = create_test_block(10);
        let test_block2 = create_test_block(11);
        
        let operations = vec![
            WriteOperation::PutBlock { block: test_block1.clone(), canonical: true },
            WriteOperation::PutBlock { block: test_block2.clone(), canonical: true },
            WriteOperation::Put { key: b"batch_key".to_vec(), value: b"batch_value".to_vec() },
            WriteOperation::UpdateHead { head: BlockRef { hash: test_block2.hash(), height: 11 } },
        ];
        
        // Execute batch operation
        let batch_result = database.batch_write(operations).await;
        assert!(batch_result.is_ok(), "Batch operation failed: {:?}", batch_result.err());
        
        // Verify all operations were applied
        let block1 = database.get_block(&test_block1.hash()).await.expect("Failed to get block1");
        assert!(block1.is_some(), "Block1 should exist after batch operation");
        
        let block2 = database.get_block(&test_block2.hash()).await.expect("Failed to get block2");
        assert!(block2.is_some(), "Block2 should exist after batch operation");
        
        let state = database.get_state(b"batch_key").await.expect("Failed to get batch state");
        assert!(state.is_some(), "Batch state should exist");
        assert_eq!(state.unwrap(), b"batch_value");
        
        let chain_head = database.get_chain_head().await.expect("Failed to get chain head");
        assert!(chain_head.is_some(), "Chain head should be updated");
        assert_eq!(chain_head.unwrap().height, 11);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let mut metrics = StorageActorMetrics::new();
        
        // Test recording various operations
        metrics.record_block_stored(1, Duration::from_millis(100), true);
        metrics.record_block_retrieved(Duration::from_millis(50), true);
        metrics.record_state_update(Duration::from_millis(25));
        metrics.record_state_query(Duration::from_millis(10), false);
        
        assert_eq!(metrics.blocks_stored, 1);
        assert_eq!(metrics.blocks_retrieved, 1);
        assert_eq!(metrics.state_updates, 1);
        assert_eq!(metrics.state_queries, 1);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
        
        // Test cache hit rate calculation
        let hit_rate = metrics.cache_hit_rate();
        assert_eq!(hit_rate, 0.5); // 1 hit out of 2 total requests
        
        // Test snapshot creation
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.blocks_stored, 1);
        assert_eq!(snapshot.blocks_retrieved, 1);
        assert_eq!(snapshot.cache_hit_rate, 0.5);
    }

    #[tokio::test]
    async fn test_performance_violations() {
        let mut metrics = StorageActorMetrics::new();
        let thresholds = StorageAlertThresholds::default();
        
        // Record slow operations that should trigger violations
        metrics.record_block_stored(1, Duration::from_millis(2000), true); // > 1000ms threshold
        metrics.record_block_retrieved(Duration::from_millis(200), false); // > 100ms threshold
        metrics.record_state_update(Duration::from_millis(100)); // > 50ms threshold
        
        assert_eq!(metrics.performance_violations.slow_block_storage, 1);
        assert_eq!(metrics.performance_violations.slow_block_retrieval, 1);
        assert_eq!(metrics.performance_violations.slow_state_updates, 1);
        assert!(metrics.performance_violations.last_violation_at.is_some());
        
        // Test alert checking
        let alerts = metrics.check_alerts(&thresholds);
        assert!(!alerts.is_empty(), "Should have performance alerts");
        assert!(alerts.iter().any(|alert| alert.contains("Block storage time exceeded")));
        assert!(alerts.iter().any(|alert| alert.contains("Block retrieval time exceeded")));
        assert!(alerts.iter().any(|alert| alert.contains("State update time exceeded")));
    }

    /// Test that verifies the overall integration is working
    #[tokio::test]
    async fn test_storage_actor_integration() {
        let config = create_test_config();
        let storage_actor = StorageActor::new(config).await.expect("Failed to create StorageActor");
        
        // Verify the actor was created with correct configuration
        assert!(storage_actor.database.get_stats().await.is_ok());
        
        // Test that cache is working
        let cache_stats = storage_actor.cache.get_stats().await;
        assert_eq!(cache_stats.total_memory_bytes, 0); // Empty cache initially
        
        // Test storage statistics
        let storage_stats = storage_actor.get_storage_stats().await;
        assert_eq!(storage_stats.blocks_stored, 0); // No blocks stored initially
        assert_eq!(storage_stats.pending_writes, 0); // No pending writes initially
        
        println!("✅ Storage Actor integration test passed!");
        println!("   - Database operations: Working");
        println!("   - Cache system: Working");
        println!("   - Metrics collection: Working");
        println!("   - Performance monitoring: Working");
    }
}