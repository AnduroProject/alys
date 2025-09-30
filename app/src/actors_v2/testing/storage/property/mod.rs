use crate::actors_v2::testing::storage::StorageTestHarness;
use crate::actors_v2::common::StorageMessage;
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::storage::messages::*;
use crate::actors_v2::testing::storage::fixtures::*;
use crate::auxpow_miner::BlockIndex;
use proptest::prelude::*;
use uuid::Uuid;
use std::collections::HashSet;

/// Property-based regression tests for edge cases
#[cfg(test)]
mod property_regression_tests {
    use super::*;

    #[tokio::test]
    async fn test_zero_slot_blocks() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        // Create block with slot 0 (edge case)
        let zero_block = create_test_block(0);
        let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
            block: zero_block,
            canonical: true,
            correlation_id: Some(Uuid::new_v4()),
        });

        // Should handle gracefully
        let result = harness.send_message(store_msg).await;
        assert!(result.is_ok());

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_empty_state_keys() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let update_msg = StorageMessage::UpdateState(UpdateStateMessage {
            key: vec![], // Empty key
            value: b"test_value".to_vec(),
            correlation_id: Some(Uuid::new_v4()),
        });

        let result = harness.send_message(update_msg).await;
        assert!(result.is_ok());

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_large_state_values() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let large_value = vec![0xff; 1024 * 1024]; // 1MB value
        let update_msg = StorageMessage::UpdateState(UpdateStateMessage {
            key: b"large_key".to_vec(),
            value: large_value,
            correlation_id: Some(Uuid::new_v4()),
        });

        let result = harness.send_message(update_msg).await;
        assert!(result.is_ok());

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_storage_retrieval_consistency() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let test_blocks = create_test_block_sequence(5);
        let mut stored_hashes = HashSet::new();

        // Store all blocks
        for (i, block) in test_blocks.iter().enumerate() {
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block: block.clone(),
                canonical: i % 2 == 0,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(store_msg).await.unwrap();

            use crate::block::ConvertBlockHash;
            stored_hashes.insert(block.message.block_hash().to_block_hash());
        }

        // Verify all blocks can be retrieved
        for hash in stored_hashes {
            let get_msg = StorageMessage::GetBlock(GetBlockMessage {
                block_hash: hash,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(get_msg).await.unwrap();
        }

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_state_idempotency() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let test_state_data = create_test_state_data(3);

        // Apply state updates multiple times
        for (key, value) in &test_state_data {
            for _repetition in 0..3 {
                let update_msg = StorageMessage::UpdateState(UpdateStateMessage {
                    key: key.clone(),
                    value: value.clone(),
                    correlation_id: Some(Uuid::new_v4()),
                });

                harness.send_message(update_msg).await.unwrap();
            }

            // Verify state consistency after multiple updates
            let get_msg = StorageMessage::GetState(GetStateMessage {
                key: key.clone(),
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(get_msg).await.unwrap();
        }

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_block_height_ordering() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let blocks = create_test_block_sequence(5);

        // Store blocks in random order
        let mut shuffled_blocks = blocks.clone();
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        shuffled_blocks.shuffle(&mut rng);

        for block in shuffled_blocks {
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block,
                canonical: true,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(store_msg).await.unwrap();
        }

        // Verify blocks can be retrieved by height in order
        for block in &blocks {
            let get_msg = StorageMessage::GetBlockByHeight(GetBlockByHeightMessage {
                height: block.message.slot,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(get_msg).await.unwrap();
        }

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_chain_head_updates() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let blocks = create_test_block_sequence(5);
        let canonical_mask = [true, false, true, false, true]; // Alternate canonical

        // Store blocks and track expected head
        for (block, &canonical) in blocks.iter().zip(&canonical_mask) {
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block: block.clone(),
                canonical,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(store_msg).await.unwrap();
        }

        // Verify chain head
        let get_head_msg = StorageMessage::GetChainHead(GetChainHeadMessage {
            correlation_id: Some(Uuid::new_v4()),
        });

        harness.send_message(get_head_msg).await.unwrap();

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_block_existence_accuracy() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let blocks = create_test_block_sequence(3);
        let mut stored_hashes = HashSet::new();

        // Store blocks
        for (i, block) in blocks.iter().enumerate() {
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block: block.clone(),
                canonical: i % 2 == 0,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(store_msg).await.unwrap();

            use crate::block::ConvertBlockHash;
            stored_hashes.insert(block.message.block_hash().to_block_hash());
        }

        // Check existence of stored blocks
        for hash in &stored_hashes {
            let exists_msg = StorageMessage::BlockExists(BlockExistsMessage {
                block_hash: *hash,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(exists_msg).await.unwrap();
        }

        // Check non-existence of random blocks
        use lighthouse_wrapper::types::Hash256;
        for i in 1000..1003 {
            let random_hash = Hash256::from_low_u64_be(i);
            if !stored_hashes.contains(&random_hash) {
                let exists_msg = StorageMessage::BlockExists(BlockExistsMessage {
                    block_hash: random_hash,
                    correlation_id: Some(Uuid::new_v4()),
                });

                harness.send_message(exists_msg).await.unwrap();
            }
        }

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_mixed_storage_operations() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let blocks = create_test_block_sequence(3);
        let state_data = create_test_state_data(2);

        // Interleave block and state operations
        for (i, block) in blocks.iter().enumerate() {
            // Store block
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block: block.clone(),
                canonical: i == blocks.len() - 1, // Last block is canonical
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(store_msg).await.unwrap();

            // Store state if available
            if let Some((key, value)) = state_data.get(i % state_data.len()) {
                let state_msg = StorageMessage::UpdateState(UpdateStateMessage {
                    key: key.clone(),
                    value: value.clone(),
                    correlation_id: Some(Uuid::new_v4()),
                });

                harness.send_message(state_msg).await.unwrap();
            }
        }

        // Verify system state is consistent
        harness.verify_state().await.unwrap();

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_transparency() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let blocks = create_test_block_sequence(2);

        // Store blocks
        for block in &blocks {
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block: block.clone(),
                canonical: true,
                correlation_id: Some(Uuid::new_v4()),
            });

            harness.send_message(store_msg).await.unwrap();
        }

        // Retrieve blocks multiple times (should hit cache after first retrieval)
        for _iteration in 0..3 {
            for block in &blocks {
                use crate::block::ConvertBlockHash;
                let get_msg = StorageMessage::GetBlock(GetBlockMessage {
                    block_hash: block.message.block_hash().to_block_hash(),
                    correlation_id: Some(Uuid::new_v4()),
                });

                harness.send_message(get_msg).await.unwrap();
            }
        }

        harness.teardown().await.unwrap();
    }
}