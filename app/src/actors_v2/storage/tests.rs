//! Basic integration tests for Storage Actor V2

#[cfg(test)]
mod tests {
    use crate::actors_v2::storage::{
        actor::{AlysConsensusBlock, BlockRef, StorageActor, StorageConfig},
        messages::{GetBlockMessage, StoreBlockMessage},
    };
    use crate::auxpow_miner::BlockIndex;
    use crate::block::ConvertBlockHash;
    use lighthouse_wrapper::types::{
        Address, ExecutionBlockHash, ExecutionPayloadCapella, Hash256, MainnetEthSpec,
    };
    use tempfile::tempdir;
    use uuid::Uuid;

    /// Create a test storage configuration
    fn create_test_config() -> StorageConfig {
        let temp_dir = tempdir().unwrap();
        let mut config = StorageConfig::default();
        config.database.main_path = temp_dir
            .path()
            .join("test_storage")
            .to_string_lossy()
            .to_string();
        config
    }

    /// Create a test consensus block
    fn create_test_block(slot: u64) -> AlysConsensusBlock {
        // Create a minimal ExecutionPayloadCapella for testing
        let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
            parent_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot - 1)),
            fee_recipient: Address::zero(),
            state_root: Hash256::from_low_u64_be(slot + 1000),
            receipts_root: Hash256::from_low_u64_be(slot + 2000),
            logs_bloom: Default::default(),
            prev_randao: Hash256::from_low_u64_be(slot + 3000),
            block_number: slot,
            gas_limit: 30000000,
            gas_used: 0,
            timestamp: 1600000000 + slot,
            extra_data: Default::default(),
            base_fee_per_gas: 1000000000u64.into(),
            block_hash: ExecutionBlockHash::from_root(Hash256::from_low_u64_be(slot + 4000)),
            transactions: Default::default(),
            withdrawals: Default::default(),
        };

        let consensus_block = crate::block::ConsensusBlock {
            parent_hash: Hash256::from_low_u64_be(slot - 1),
            slot,
            auxpow_header: None,
            execution_payload,
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        };

        AlysConsensusBlock {
            message: consensus_block,
            signature: crate::signatures::AggregateApproval::new(),
        }
    }

    #[actix::test]
    async fn test_storage_actor_creation() {
        let config = create_test_config();
        let result = StorageActor::new(config).await;
        assert!(
            result.is_ok(),
            "Failed to create storage actor: {:?}",
            result.err()
        );
    }

    #[actix::test]
    async fn test_block_storage_and_retrieval() {
        let config = create_test_config();
        let mut storage = StorageActor::new(config).await.unwrap();

        let test_block = create_test_block(100);
        let block_hash = test_block.message.block_hash().to_block_hash();

        // Test block storage
        let store_result = storage.store_block(test_block.clone(), true).await;
        assert!(
            store_result.is_ok(),
            "Failed to store block: {:?}",
            store_result.err()
        );

        // Test block retrieval
        let retrieved_block = storage.get_block(&block_hash).await.unwrap();
        assert!(retrieved_block.is_some(), "Block not found");

        let retrieved = retrieved_block.unwrap();
        assert_eq!(retrieved.message.slot, test_block.message.slot);
        assert_eq!(
            retrieved.message.execution_payload.state_root,
            test_block.message.execution_payload.state_root
        );
    }

    #[actix::test]
    async fn test_chain_head_operations() {
        let config = create_test_config();
        let storage = StorageActor::new(config).await.unwrap();

        // Test getting chain head (should be None initially)
        let initial_head = storage.database.get_chain_head().await.unwrap();
        assert!(
            initial_head.is_none(),
            "Chain head should be None initially"
        );

        // Test setting chain head
        let test_head = BlockRef {
            hash: Hash256::from_low_u64_be(42),
            number: 100,
            execution_hash: ExecutionBlockHash::zero(),
        };

        let put_result = storage.database.put_chain_head(&test_head).await;
        assert!(
            put_result.is_ok(),
            "Failed to set chain head: {:?}",
            put_result.err()
        );

        // Test getting updated chain head
        let updated_head = storage.database.get_chain_head().await.unwrap();
        assert!(updated_head.is_some(), "Chain head should exist");

        let head = updated_head.unwrap();
        assert_eq!(head.hash, test_head.hash);
        assert_eq!(head.number, test_head.number);
    }

    #[actix::test]
    async fn test_state_operations() {
        let config = create_test_config();
        let storage = StorageActor::new(config).await.unwrap();

        let test_key = b"test_state_key".to_vec();
        let test_value = b"test_state_value".to_vec();

        // Test state storage
        let put_result = storage.database.put_state(&test_key, &test_value).await;
        assert!(
            put_result.is_ok(),
            "Failed to store state: {:?}",
            put_result.err()
        );

        // Test state retrieval
        let retrieved_value = storage.database.get_state(&test_key).await.unwrap();
        assert!(retrieved_value.is_some(), "State value not found");
        assert_eq!(retrieved_value.unwrap(), test_value);

        // Test non-existent key
        let missing_value = storage.database.get_state(b"missing_key").await.unwrap();
        assert!(missing_value.is_none(), "Missing key should return None");
    }

    #[actix::test]
    async fn test_cache_operations() {
        let config = create_test_config();
        let storage = StorageActor::new(config).await.unwrap();

        let test_block = create_test_block(200);
        let block_hash = test_block.message.block_hash().to_block_hash();

        // Test cache storage
        storage
            .cache
            .put_block(block_hash, test_block.clone())
            .await;

        // Test cache retrieval
        let cached_block = storage.cache.get_block(&block_hash).await;
        assert!(cached_block.is_some(), "Block not found in cache");

        let cached = cached_block.unwrap();
        assert_eq!(cached.message.slot, test_block.message.slot);

        // Test cache miss
        let missing_block = storage
            .cache
            .get_block(&Hash256::from_low_u64_be(999))
            .await;
        assert!(
            missing_block.is_none(),
            "Non-existent block should not be in cache"
        );
    }

    #[actix::test]
    async fn test_metrics_collection() {
        let config = create_test_config();
        let mut storage = StorageActor::new(config).await.unwrap();

        let initial_blocks_stored = storage.metrics.blocks_stored;

        // Store a test block
        let test_block = create_test_block(300);
        let _store_result = storage.store_block(test_block, true).await;

        // Check metrics updated
        assert!(
            storage.metrics.blocks_stored > initial_blocks_stored,
            "Metrics should be updated"
        );
    }

    #[test]
    fn test_configuration_defaults() {
        let config = StorageConfig::default();

        assert_eq!(config.write_batch_size, 1000);
        assert_eq!(config.sync_interval.as_secs(), 5);
        assert_eq!(config.maintenance_interval.as_secs(), 300);
        assert!(config.enable_auto_compaction);
    }

    #[test]
    fn test_message_creation() {
        let test_block = create_test_block(400);
        let correlation_id = Uuid::new_v4();

        let store_msg = StoreBlockMessage {
            block: test_block.clone(),
            canonical: true,
            correlation_id: Some(correlation_id),
        };

        assert_eq!(store_msg.block.message.slot, 400);
        assert!(store_msg.canonical);
        assert_eq!(store_msg.correlation_id, Some(correlation_id));

        let get_msg = GetBlockMessage {
            block_hash: test_block.message.block_hash().to_block_hash(),
            correlation_id: Some(correlation_id),
        };

        assert_eq!(
            get_msg.block_hash,
            test_block.message.block_hash().to_block_hash()
        );
    }
}
