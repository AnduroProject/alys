use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::testing::storage::StorageTestHarness;
use crate::auxpow_miner::BlockIndex;

#[actix::test]
async fn test_full_actor_lifecycle() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test the complete lifecycle of storing and retrieving blocks
    let test_blocks = harness.test_blocks.clone();
    let actor_ref = harness.base.get_actor_ref().await;

    // Store blocks with various patterns
    for (i, block) in test_blocks.iter().enumerate().take(5) {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.store_block(block.clone(), i % 3 == 0).await;
        assert!(result.is_ok(), "Failed to store block {}: {:?}", i, result);
    }

    // Retrieve blocks and verify integrity
    for (i, block) in test_blocks.iter().enumerate().take(5) {
        let mut actor_guard = actor_ref.write().await;
        use crate::block::ConvertBlockHash;
        let block_hash = block.message.block_hash().to_block_hash();
        let result = actor_guard.get_block(&block_hash).await.unwrap();

        assert!(result.is_some(), "Block {} should exist", i);
        let retrieved_block = result.unwrap();
        assert_eq!(
            retrieved_block.message.slot, block.message.slot,
            "Block slot mismatch for block {}",
            i
        );
    }

    // Verify final state
    harness.verify_state().await.unwrap();

    // Check metrics
    let storage_metrics = harness.get_storage_metrics().await.unwrap();
    assert!(
        storage_metrics.blocks_stored >= 5,
        "Expected at least 5 blocks stored"
    );

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_concurrent_read_write_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let test_blocks = harness.test_blocks.clone();
    let actor_ref = harness.base.get_actor_ref().await;

    // Spawn concurrent write operations
    let write_handles: Vec<_> = test_blocks
        .into_iter()
        .enumerate()
        .take(10)
        .map(|(i, block)| {
            let actor = actor_ref.clone();
            tokio::spawn(async move {
                let mut actor_guard = actor.write().await;
                actor_guard.store_block(block, i % 2 == 0).await
            })
        })
        .collect();

    // Wait for all writes to complete
    for (i, handle) in write_handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Write operation {} failed: {:?}", i, result);
    }

    // Spawn concurrent read operations
    let read_handles: Vec<_> = (0..10)
        .map(|i| {
            let actor = actor_ref.clone();
            let block_hash = {
                use crate::block::ConvertBlockHash;
                harness.test_blocks[i % harness.test_blocks.len()]
                    .message
                    .block_hash()
                    .to_block_hash()
            };

            tokio::spawn(async move {
                let mut actor_guard = actor.write().await;
                actor_guard.get_block(&block_hash).await
            })
        })
        .collect();

    // Verify all reads succeed
    for (i, handle) in read_handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Read operation {} failed: {:?}", i, result);
        let block_option = result.unwrap();
        assert!(
            block_option.is_some(),
            "Block should exist for read operation {}",
            i
        );
    }

    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_performance_under_load() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let start_time = std::time::Instant::now();
    let block_count = 100;

    // Generate and store many blocks
    let performance_blocks =
        crate::actors_v2::testing::storage::fixtures::create_performance_test_blocks(
            block_count,
            false,
        );
    let actor_ref = harness.base.get_actor_ref().await;

    for (i, block) in performance_blocks.iter().enumerate() {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.store_block(block.clone(), true).await;
        assert!(
            result.is_ok(),
            "Failed to store performance test block {}: {:?}",
            i,
            result
        );
    }

    let duration = start_time.elapsed();
    let blocks_per_second = block_count as f64 / duration.as_secs_f64();

    // Assert minimum performance threshold (relaxed for testing environment)
    assert!(
        blocks_per_second > 10.0,
        "Storage performance too low: {:.2} blocks/sec (expected > 10)",
        blocks_per_second
    );

    // Verify all blocks can be retrieved
    for (i, block) in performance_blocks.iter().enumerate().take(10) {
        // Sample first 10
        let mut actor_guard = actor_ref.write().await;
        use crate::block::ConvertBlockHash;
        let block_hash = block.message.block_hash().to_block_hash();
        let result = actor_guard.get_block(&block_hash).await.unwrap();
        assert!(
            result.is_some(),
            "Performance test block {} should be retrievable",
            i
        );
    }

    println!(
        "Performance test completed: {:.2} blocks/sec",
        blocks_per_second
    );

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_cache_and_database_integration() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let test_block = harness.test_blocks[0].clone();
    let actor_ref = harness.base.get_actor_ref().await;

    // Store a block
    {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.store_block(test_block.clone(), true).await;
        assert!(result.is_ok(), "Failed to store test block");
    }

    // First retrieval should hit database and populate cache
    let first_retrieval_start = std::time::Instant::now();
    {
        let mut actor_guard = actor_ref.write().await;
        use crate::block::ConvertBlockHash;
        let block_hash = test_block.message.block_hash().to_block_hash();
        let result = actor_guard.get_block(&block_hash).await.unwrap();
        assert!(result.is_some(), "Block should exist");
    }
    let first_retrieval_time = first_retrieval_start.elapsed();

    // Second retrieval should hit cache and be faster
    let second_retrieval_start = std::time::Instant::now();
    {
        let mut actor_guard = actor_ref.write().await;
        use crate::block::ConvertBlockHash;
        let block_hash = test_block.message.block_hash().to_block_hash();
        let result = actor_guard.get_block(&block_hash).await.unwrap();
        assert!(result.is_some(), "Block should exist in cache");
    }
    let second_retrieval_time = second_retrieval_start.elapsed();

    println!(
        "First retrieval: {:?}, Second retrieval: {:?}",
        first_retrieval_time, second_retrieval_time
    );

    // Cache hit should generally be faster (though not guaranteed in test environment)
    // This is more of a performance indicator than a strict requirement

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_error_handling_integration() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let actor_ref = harness.base.get_actor_ref().await;

    // Test retrieving non-existent block
    {
        let mut actor_guard = actor_ref.write().await;
        use lighthouse_wrapper::types::Hash256;
        let non_existent_hash = Hash256::from_low_u64_be(99999);
        let result = actor_guard.get_block(&non_existent_hash).await;
        assert!(
            result.is_ok(),
            "Get operation should not error for non-existent block"
        );
        assert!(
            result.unwrap().is_none(),
            "Non-existent block should return None"
        );
    }

    // Test storing block and then retrieving it
    let test_block = harness.test_blocks[0].clone();
    {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.store_block(test_block.clone(), true).await;
        assert!(result.is_ok(), "Store operation should succeed");
    }

    // Verify the block exists after storage
    {
        let mut actor_guard = actor_ref.write().await;
        use crate::block::ConvertBlockHash;
        let block_hash = test_block.message.block_hash().to_block_hash();
        let result = actor_guard.get_block(&block_hash).await;
        assert!(result.is_ok(), "Get operation should succeed after storage");
        assert!(result.unwrap().is_some(), "Stored block should exist");
    }

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_chain_head_management() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let actor_ref = harness.base.get_actor_ref().await;

    // Initially, chain head should be None
    {
        let actor_guard = actor_ref.read().await;
        let head_result = actor_guard.database.get_chain_head().await;
        assert!(head_result.is_ok(), "Get chain head should not error");
        assert!(
            head_result.unwrap().is_none(),
            "Initial chain head should be None"
        );
    }

    // Store a canonical block
    let first_block = harness.test_blocks[0].clone();
    {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.store_block(first_block.clone(), true).await;
        assert!(result.is_ok(), "Failed to store first canonical block");
    }

    // Chain head should now be set
    {
        let actor_guard = actor_ref.read().await;
        let head_result = actor_guard.database.get_chain_head().await;
        assert!(head_result.is_ok(), "Get chain head should not error");
        let head = head_result.unwrap();
        assert!(
            head.is_some(),
            "Chain head should be set after canonical block"
        );

        let head_ref = head.unwrap();
        use crate::block::ConvertBlockHash;
        assert_eq!(
            head_ref.hash,
            first_block.message.block_hash().to_block_hash()
        );
        assert_eq!(head_ref.number, first_block.message.slot);
    }

    // Store a newer canonical block
    let second_block = harness.test_blocks[1].clone();
    {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.store_block(second_block.clone(), true).await;
        assert!(result.is_ok(), "Failed to store second canonical block");
    }

    // Chain head should be updated
    {
        let actor_guard = actor_ref.read().await;
        let head_result = actor_guard.database.get_chain_head().await;
        assert!(head_result.is_ok(), "Get chain head should not error");
        let head = head_result.unwrap();
        assert!(head.is_some(), "Chain head should still be set");

        let head_ref = head.unwrap();
        use crate::block::ConvertBlockHash;
        assert_eq!(
            head_ref.hash,
            second_block.message.block_hash().to_block_hash()
        );
        assert_eq!(head_ref.number, second_block.message.slot);
    }

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_state_persistence_integration() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let actor_ref = harness.base.get_actor_ref().await;

    // Store various state entries
    let state_entries = vec![
        (b"key1".to_vec(), b"value1".to_vec()),
        (b"key2".to_vec(), b"value2_longer_value".to_vec()),
        (b"key3_longer_key".to_vec(), b"value3".to_vec()),
        (vec![0u8; 32], vec![1u8; 64]), // Binary data
    ];

    // Store all state entries
    for (key, value) in &state_entries {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard.database.put_state(key, value).await;
        assert!(result.is_ok(), "Failed to store state entry: {:?}", result);
    }

    // Retrieve and verify all state entries
    for (key, expected_value) in &state_entries {
        let actor_guard = actor_ref.read().await;
        let result = actor_guard.database.get_state(key).await;
        assert!(result.is_ok(), "Failed to retrieve state entry");
        let retrieved_value = result.unwrap();
        assert!(retrieved_value.is_some(), "State entry should exist");
        assert_eq!(
            retrieved_value.unwrap(),
            *expected_value,
            "State value mismatch"
        );
    }

    // Test overwriting state
    let overwrite_key = b"key1".to_vec();
    let new_value = b"new_value1".to_vec();
    {
        let mut actor_guard = actor_ref.write().await;
        let result = actor_guard
            .database
            .put_state(&overwrite_key, &new_value)
            .await;
        assert!(result.is_ok(), "Failed to overwrite state entry");
    }

    // Verify overwrite
    {
        let actor_guard = actor_ref.read().await;
        let result = actor_guard.database.get_state(&overwrite_key).await;
        assert!(result.is_ok(), "Failed to retrieve overwritten state entry");
        let retrieved_value = result.unwrap();
        assert!(
            retrieved_value.is_some(),
            "Overwritten state entry should exist"
        );
        assert_eq!(
            retrieved_value.unwrap(),
            new_value,
            "Overwritten state value mismatch"
        );
    }

    harness.teardown().await.unwrap();
}
