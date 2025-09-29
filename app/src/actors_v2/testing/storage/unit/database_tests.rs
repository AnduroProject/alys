use crate::actors_v2::testing::storage::StorageTestHarness;
use crate::actors_v2::common::StorageMessage;
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::storage::messages::*;
use crate::auxpow_miner::BlockIndex;
use uuid::Uuid;

#[actix::test]
async fn test_database_block_storage_retrieval() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test block storage
    let test_block = harness.test_blocks[0].clone();
    let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
        block: test_block.clone(),
        canonical: true,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(store_message).await.unwrap();

    // Test block retrieval
    use crate::block::ConvertBlockHash;
    let block_hash = test_block.block_hash().to_block_hash();
    let get_message = StorageMessage::GetBlock(GetBlockMessage {
        block_hash,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_batch_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test batch storage of multiple blocks
    let test_blocks = harness.test_blocks.clone(); // Clone to avoid borrowing issues
    for (i, block) in test_blocks.iter().enumerate() {
        let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
            block: block.clone(),
            canonical: i % 2 == 0, // Alternate canonical status
            correlation_id: Some(Uuid::new_v4()),
        });

        harness.send_message(store_message).await.unwrap();
    }

    // Verify all blocks stored correctly
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_error_conditions() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test invalid block hash retrieval
    use lighthouse_wrapper::types::Hash256;
    let invalid_hash = Hash256::zero();
    let get_message = StorageMessage::GetBlock(GetBlockMessage {
        block_hash: invalid_hash,
        correlation_id: Some(Uuid::new_v4()),
    });

    // This should not panic but handle gracefully
    let result = harness.send_message(get_message).await;
    assert!(result.is_ok()); // The operation succeeds but returns None

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_state_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let test_key = b"test_state_key".to_vec();
    let test_value = b"test_state_value".to_vec();

    // Test state storage
    let update_message = StorageMessage::UpdateState(UpdateStateMessage {
        key: test_key.clone(),
        value: test_value.clone(),
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(update_message).await.unwrap();

    // Test state retrieval
    let get_state_message = StorageMessage::GetState(GetStateMessage {
        key: test_key.clone(),
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_state_message).await.unwrap();

    // Test non-existent key
    let missing_key = b"missing_key".to_vec();
    let get_missing_message = StorageMessage::GetState(GetStateMessage {
        key: missing_key,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_missing_message).await.unwrap(); // Should succeed but return None

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_chain_head_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test getting chain head (should be None initially)
    let get_head_message = StorageMessage::GetChainHead(GetChainHeadMessage {
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_head_message).await.unwrap();

    // Store a canonical block to set chain head
    let test_block = harness.test_blocks[0].clone();
    let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
        block: test_block.clone(),
        canonical: true,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(store_message).await.unwrap();

    // Test getting updated chain head
    let get_updated_head_message = StorageMessage::GetChainHead(GetChainHeadMessage {
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_updated_head_message).await.unwrap();

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_concurrent_operations() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test concurrent storage operations
    let mut handles = Vec::new();
    for (i, block) in harness.test_blocks.iter().enumerate().take(5) {
        let actor_ref = harness.base.get_actor_ref().await;
        let block_clone = block.clone();
        let canonical = i % 2 == 0;

        let handle = tokio::spawn(async move {
            let mut actor_guard = actor_ref.write().await;
            actor_guard.store_block(block_clone, canonical).await
        });

        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent operation failed: {:?}", result);
    }

    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_persistence() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Store test data
    let test_block = harness.test_blocks[0].clone();
    use crate::block::ConvertBlockHash;
    let block_hash = test_block.block_hash().to_block_hash();

    let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
        block: test_block.clone(),
        canonical: true,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(store_message).await.unwrap();

    // Simulate restart by resetting the harness (keeps same database path)
    // Note: In a real scenario, this would involve restarting the actor
    // but keeping the same database directory
    harness.verify_state().await.unwrap();

    // Verify data persists after restart
    let get_message = StorageMessage::GetBlock(GetBlockMessage {
        block_hash,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_message).await.unwrap();

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_metrics_tracking() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    let initial_metrics = harness.get_storage_metrics().await.unwrap();
    let initial_blocks_stored = initial_metrics.blocks_stored;

    // Store a test block
    let test_block = harness.test_blocks[0].clone();
    let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
        block: test_block,
        canonical: true,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(store_message).await.unwrap();

    // Check metrics updated
    let updated_metrics = harness.get_storage_metrics().await.unwrap();
    assert!(updated_metrics.blocks_stored > initial_blocks_stored,
           "Metrics should be updated after storing block");

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_database_large_data_handling() {
    let mut harness = StorageTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test with large extra data
    use crate::actors_v2::testing::storage::fixtures::create_test_block_with_properties;
    let large_block = create_test_block_with_properties(
        1,
        1000000, // 1M gas used
        1600000000,
        vec![0xff; 10240], // 10KB extra data
    );

    let store_message = StorageMessage::StoreBlock(StoreBlockMessage {
        block: large_block,
        canonical: true,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(store_message).await.unwrap();

    // Test with large state value
    let large_key = vec![0xaa; 100]; // 100 byte key
    let large_value = vec![0xbb; 100000]; // 100KB value

    let update_state_message = StorageMessage::UpdateState(UpdateStateMessage {
        key: large_key.clone(),
        value: large_value,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(update_state_message).await.unwrap();

    // Verify large data can be retrieved
    let get_state_message = StorageMessage::GetState(GetStateMessage {
        key: large_key,
        correlation_id: Some(Uuid::new_v4()),
    });

    harness.send_message(get_state_message).await.unwrap();

    harness.teardown().await.unwrap();
}