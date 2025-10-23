//! ChainActor Unit Tests
//!
//! Basic unit tests for ChainActor V2 functionality

#[cfg(test)]
mod tests {
    use crate::actors_v2::testing::chain::fixtures::*;
    use bitcoin::hashes::Hash;
    use lighthouse_wrapper::types::ExecutionBlockHash;

    #[tokio::test]
    async fn test_chain_config_validation() {
        // Test configuration validation
        let config = validator_config();
        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.max_blocks_without_pow = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_chain_status_creation() {
        // Test chain status message creation
        let status = mock_chain_status();
        assert_eq!(status.height, 100);
        assert_eq!(status.peer_count, 5);
        assert!(status.is_synced);
        assert!(status.is_validator);
        assert!(status.auxpow_enabled);
    }

    #[tokio::test]
    async fn test_pegin_fixtures() {
        // Test peg-in data fixtures
        let pegin = mock_pegin_info();
        assert_eq!(pegin.amount, 100000000); // 1 BTC

        let multiple_pegins = mock_multiple_pegins();
        assert_eq!(multiple_pegins.len(), 3);
        assert_eq!(
            multiple_pegins.iter().map(|p| p.amount).sum::<u64>(),
            350000000
        ); // 3.5 BTC total
    }

    #[tokio::test]
    async fn test_pegout_fixtures() {
        // Test peg-out data fixtures
        let pegout = mock_pegout_request();
        assert_eq!(pegout.amount, 50000000); // 0.5 BTC

        let multiple_pegouts = mock_multiple_pegouts();
        assert_eq!(multiple_pegouts.len(), 2);
        assert_eq!(
            multiple_pegouts.iter().map(|p| p.amount).sum::<u64>(),
            100000000
        ); // 1 BTC total
    }

    #[tokio::test]
    async fn test_chain_state_creation() {
        // Test ChainState creation and initial values
        use crate::actors_v2::testing::chain::ChainTestHarness;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;
        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);

        // Test initial state
        assert_eq!(state.get_height(), 0);
        assert!(state.get_head_hash().is_none());
        assert!(state.is_synced());
        assert_eq!(state.blocks_without_pow, 0);
        assert!(!state.needs_auxpow());
        assert!(state.get_queued_pow().is_none());
        assert!(state.queued_pegins.read().await.is_empty());
        assert_eq!(state.is_validator, true);
        assert!(state.block_hash_cache.is_some());
        assert!(state.last_block_time.is_none());
    }

    #[tokio::test]
    async fn test_chain_state_height_methods() {
        // Test height-related methods
        use crate::actors_v2::storage::actor::BlockRef;
        use crate::actors_v2::testing::chain::ChainTestHarness;
        use ethereum_types::H256;
        use lighthouse_wrapper::types::ExecutionBlockHash;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;
        let mut state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);

        // Test initial height
        assert_eq!(state.get_height(), 0);

        // Test height updates
        let block_ref_1 = BlockRef {
            hash: H256::from_low_u64_be(1),
            number: 100,
            execution_hash: ExecutionBlockHash::zero(),
        };
        state.update_head(block_ref_1.clone());
        assert_eq!(state.get_height(), 100);
        assert_eq!(state.get_head_hash(), Some(H256::from_low_u64_be(1)));
        assert!(state.last_block_time.is_some());

        let block_ref_2 = BlockRef {
            hash: H256::from_low_u64_be(2),
            number: 200,
            execution_hash: ExecutionBlockHash::zero(),
        };
        state.update_head(block_ref_2.clone());
        assert_eq!(state.get_height(), 200);
        assert_eq!(state.get_head_hash(), Some(H256::from_low_u64_be(2)));
    }

    #[tokio::test]
    async fn test_chain_state_sync_methods() {
        // Test sync status methods
        use crate::actors_v2::chain::state::SyncStatus;
        use crate::actors_v2::testing::chain::ChainTestHarness;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;
        let mut state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);

        // Test initial sync status
        assert!(state.is_synced());
        assert!(matches!(state.sync_status, SyncStatus::Synced));

        // Test sync status transitions
        state.set_sync_status(SyncStatus::NotSynced);
        assert!(!state.is_synced());

        state.set_sync_status(SyncStatus::Syncing {
            progress: 0.5,
            target_height: 1000,
        });
        assert!(!state.is_synced());

        state.set_sync_status(SyncStatus::Error("Network timeout".to_string()));
        assert!(!state.is_synced());

        state.set_sync_status(SyncStatus::Synced);
        assert!(state.is_synced());
    }

    #[tokio::test]
    async fn test_chain_state_auxpow_methods() {
        // Test AuxPoW-related methods
        use crate::actors_v2::testing::chain::ChainTestHarness;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let mut state = harness.into_chain_state(
            true, // is_validator
            10,   // max_blocks_without_pow
            None,
        );

        // Test initial AuxPoW state
        assert!(!state.needs_auxpow());
        assert_eq!(state.blocks_without_pow, 0);

        // Test incrementing blocks without PoW
        for i in 1..10 {
            state.increment_blocks_without_pow();
            assert_eq!(state.blocks_without_pow, i);
            assert!(!state.needs_auxpow());
        }

        // After max_blocks_without_pow, should need AuxPoW
        state.increment_blocks_without_pow();
        assert_eq!(state.blocks_without_pow, 10);
        assert!(state.needs_auxpow());

        // Test reset
        state.reset_blocks_without_pow();
        assert_eq!(state.blocks_without_pow, 0);
        assert!(!state.needs_auxpow());
    }

    #[tokio::test]
    async fn test_chain_state_queued_pow_methods() {
        // Test queued AuxPoW methods
        use crate::actors_v2::testing::chain::ChainTestHarness;
        use crate::block::AuxPowHeader;
        use ethereum_types::H256;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;
        let mut state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);

        // Test initial state
        assert!(state.get_queued_pow().is_none());

        // Create and set queued AuxPoW
        let auxpow = mock_auxpow();
        let auxpow_header = AuxPowHeader {
            range_start: H256::zero(),
            range_end: H256::from_low_u64_be(42),
            bits: 0x1d00ffff,
            chain_id: 1,
            height: 100,
            auxpow: Some(auxpow),
            fee_recipient: ethereum_types::Address::zero(),
        };

        state.set_queued_pow(Some(auxpow_header.clone()));
        assert!(state.get_queued_pow().is_some());

        let queued = state.get_queued_pow().as_ref().unwrap();
        assert_eq!(queued.range_end, H256::from_low_u64_be(42));

        // Test clearing queued AuxPoW
        state.set_queued_pow(None);
        assert!(state.get_queued_pow().is_none());
    }

    #[tokio::test]
    async fn test_chain_state_pegin_methods() {
        // Test peg-in management methods
        use crate::actors_v2::testing::chain::ChainTestHarness;
        use bitcoin::Txid;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;
        let mut state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);

        // Test initial state (async RwLock access)
        assert!(state.queued_pegins.read().await.is_empty());

        // Add peg-ins (async methods)
        let pegin = mock_pegin_info();
        let txid1 = Txid::from_byte_array([1u8; 32]);
        let txid2 = Txid::from_byte_array([2u8; 32]);

        state.add_queued_pegin(txid1, pegin.clone()).await;
        assert_eq!(state.queued_pegins.read().await.len(), 1);
        assert!(state.queued_pegins.read().await.contains_key(&txid1));

        state.add_queued_pegin(txid2, pegin.clone()).await;
        assert_eq!(state.queued_pegins.read().await.len(), 2);

        // Remove peg-in (async method)
        let removed = state.remove_queued_pegin(&txid1).await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().amount, pegin.amount);
        assert_eq!(state.queued_pegins.read().await.len(), 1);
        assert!(!state.queued_pegins.read().await.contains_key(&txid1));

        // Try to remove non-existent peg-in
        let non_existent = state
            .remove_queued_pegin(&Txid::from_byte_array([99u8; 32]))
            .await;
        assert!(non_existent.is_none());
        assert_eq!(state.queued_pegins.read().await.len(), 1);

        // Remove remaining peg-in
        let removed2 = state.remove_queued_pegin(&txid2).await;
        assert!(removed2.is_some());
        assert!(state.queued_pegins.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_chain_state_edge_cases() {
        // Test edge cases and boundary conditions
        use crate::actors_v2::storage::actor::BlockRef;
        use crate::actors_v2::testing::chain::ChainTestHarness;
        use ethereum_types::H256;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create test harness");

        let mut state = harness.into_chain_state(
            true, // is_validator
            1,    // max_blocks_without_pow = 1 for edge testing
            None,
        );

        // Test immediate AuxPoW requirement
        assert!(!state.needs_auxpow());
        state.increment_blocks_without_pow();
        assert!(state.needs_auxpow());

        // Test multiple resets
        state.reset_blocks_without_pow();
        state.reset_blocks_without_pow(); // Should not panic
        assert!(!state.needs_auxpow());

        // Test head updates with same height
        let block_ref_1 = BlockRef {
            hash: H256::from_low_u64_be(1),
            number: 100,
            execution_hash: ExecutionBlockHash::zero(),
        };
        let block_ref_2 = BlockRef {
            hash: H256::from_low_u64_be(2),
            number: 100,
            execution_hash: ExecutionBlockHash::zero(),
        };

        state.update_head(block_ref_1.clone());
        assert_eq!(state.get_height(), 100);
        assert_eq!(state.get_head_hash(), Some(H256::from_low_u64_be(1)));

        state.update_head(block_ref_2.clone());
        assert_eq!(state.get_height(), 100);
        assert_eq!(state.get_head_hash(), Some(H256::from_low_u64_be(2)));

        // Test decreasing height (reorg simulation)
        let block_ref_3 = BlockRef {
            hash: H256::from_low_u64_be(3),
            number: 50,
            execution_hash: ExecutionBlockHash::zero(),
        };
        state.update_head(block_ref_3.clone());
        assert_eq!(state.get_height(), 50);
        assert_eq!(state.get_head_hash(), Some(H256::from_low_u64_be(3)));
    }
}
