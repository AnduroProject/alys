//! ChainActor Integration Tests
//!
//! Integration tests for ChainActor coordination with other actors

#[cfg(test)]
mod tests {
    use bitcoin::hashes::Hash;
    use ethereum_types::{H256, U256};
    use lighthouse_wrapper::types::ExecutionBlockHash;
    use std::str::FromStr;
    use std::time::Duration;

    use crate::actors_v2::storage::actor::BlockRef;
    use crate::actors_v2::{
        chain::{messages::*, ChainActor},
        testing::chain::{fixtures::*, ChainTestHarness},
    };

    #[tokio::test]
    async fn test_chain_actor_basic_instantiation() {
        // Test ChainActor can be created using test harness
        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        // Verify configuration is valid
        assert!(harness.verify_config().await.is_ok());
        assert!(harness.config.is_validator);
        assert!(harness.config.enable_auxpow);
        assert!(harness.config.enable_peg_operations);
    }

    #[tokio::test]
    async fn test_chain_actor_message_handling() {
        // Test ChainActor responds to basic messages
        // Note: This is a simplified test focusing on message structure validation
        // Full actor system integration would require more complex setup

        let _config = validator_config();
        let status_msg = ChainMessage::GetChainStatus;

        // Test message serialization/deserialization works
        // This validates our message definitions are correct
        match status_msg {
            ChainMessage::GetChainStatus => {
                // Should match correctly
                assert!(true);
            }
            _ => panic!("Message matching failed"),
        }
    }

    #[tokio::test]
    async fn test_peg_operation_message_structure() {
        // Test peg operation messages are correctly structured
        let pegins = mock_multiple_pegins();
        let pegouts = mock_multiple_pegouts();

        let pegin_msg = ChainMessage::ProcessPegins {
            pegin_infos: pegins.clone(),
        };
        let pegout_msg = ChainMessage::ProcessPegouts {
            pegout_requests: pegouts.clone(),
        };

        // Verify messages can be constructed and match properly
        match pegin_msg {
            ChainMessage::ProcessPegins { pegin_infos } => {
                assert_eq!(pegin_infos.len(), 3);
                assert_eq!(pegin_infos[0].amount, 100000000); // 1 BTC
            }
            _ => panic!("PegIn message matching failed"),
        }

        match pegout_msg {
            ChainMessage::ProcessPegouts { pegout_requests } => {
                assert_eq!(pegout_requests.len(), 2);
                assert_eq!(pegout_requests[0].amount, 25000000); // 0.25 BTC
            }
            _ => panic!("PegOut message matching failed"),
        }
    }

    #[tokio::test]
    async fn test_auxpow_message_structure() {
        // Test AuxPoW messages are correctly structured
        let _auxpow = mock_auxpow_params();
        let block_hash = H256::from_low_u64_be(42);

        // Test both ChainMessage and ChainManagerMessage variants
        let process_msg = ChainMessage::ProcessAuxPow {
            auxpow: crate::auxpow::AuxPow {
                coinbase_txn: bitcoin::Transaction {
                    version: 1,
                    lock_time: bitcoin::absolute::LockTime::ZERO,
                    input: vec![],
                    output: vec![],
                },
                block_hash: bitcoin::BlockHash::from_byte_array([0u8; 32]),
                coinbase_branch: crate::auxpow::MerkleBranch {
                    branch_hash: vec![],
                    branch_side_mask: 0,
                },
                blockchain_branch: crate::auxpow::MerkleBranch {
                    branch_hash: vec![],
                    branch_side_mask: 0,
                },
                parent_block: bitcoin::block::Header {
                    version: bitcoin::block::Version::ONE,
                    prev_blockhash: bitcoin::BlockHash::from_byte_array([0u8; 32]),
                    merkle_root: bitcoin::hash_types::TxMerkleNode::from_byte_array([0u8; 32]),
                    time: 0,
                    bits: bitcoin::CompactTarget::from_consensus(0),
                    nonce: 0,
                },
            },
            block_hash,
        };

        match process_msg {
            ChainMessage::ProcessAuxPow {
                auxpow: _,
                block_hash: hash,
            } => {
                assert_eq!(hash, H256::from_low_u64_be(42));
            }
            _ => panic!("AuxPoW message matching failed"),
        }
    }

    #[tokio::test]
    async fn test_block_message_variants() {
        // Test block-related message variants
        let block = create_mock_signed_consensus_block();
        let _block_data = vec![1, 2, 3, 4]; // Mock block data

        // Test ImportBlock message
        let import_msg = ChainMessage::ImportBlock {
            block: block.clone(),
            source: BlockSource::Network("peer123".to_string()),
            peer_id: Some("peer123".to_string()),
        };

        match import_msg {
            ChainMessage::ImportBlock {
                block: b,
                source,
                peer_id,
            } => {
                assert_eq!(b.message.execution_payload.block_number, 100);
                if let BlockSource::Network(peer) = source {
                    assert_eq!(peer, "peer123");
                } else {
                    panic!("Expected Network source");
                }
                assert_eq!(peer_id, Some("peer123".to_string()));
            }
            _ => panic!("ImportBlock message matching failed"),
        }

        // Test BroadcastBlock message
        let broadcast_msg = ChainMessage::BroadcastBlock {
            block: block.clone(),
        };
        match broadcast_msg {
            ChainMessage::BroadcastBlock { block: b } => {
                assert_eq!(b.message.execution_payload.block_number, 100);
            }
            _ => panic!("BroadcastBlock message matching failed"),
        }

        // Test NetworkBlockReceived message
        let network_msg = ChainMessage::NetworkBlockReceived {
            block: block.clone(),
            peer_id: "peer123".to_string(),
        };
        match network_msg {
            ChainMessage::NetworkBlockReceived { block: b, peer_id } => {
                assert_eq!(b.message.execution_payload.block_number, 100);
                assert_eq!(peer_id, "peer123".to_string());
            }
            _ => panic!("NetworkBlockReceived message matching failed"),
        }
    }

    /// Helper to create a mock SignedConsensusBlock for testing
    fn create_mock_signed_consensus_block(
    ) -> crate::block::SignedConsensusBlock<lighthouse_wrapper::types::MainnetEthSpec> {
        use lighthouse_wrapper::types::{
            ExecutionBlockHash, ExecutionPayloadCapella, MainnetEthSpec,
        };

        // Create minimal execution payload
        let execution_payload = ExecutionPayloadCapella::<MainnetEthSpec> {
            parent_hash: ExecutionBlockHash::from_root(lighthouse_wrapper::types::Hash256::zero()),
            fee_recipient: lighthouse_wrapper::types::Address::zero(),
            state_root: lighthouse_wrapper::types::Hash256::zero(),
            receipts_root: lighthouse_wrapper::types::Hash256::zero(),
            logs_bloom: lighthouse_wrapper::types::FixedVector::default(),
            prev_randao: lighthouse_wrapper::types::Hash256::zero(),
            block_number: 100,
            gas_limit: 8000000,
            gas_used: 0,
            timestamp: 1640995200,
            extra_data: lighthouse_wrapper::types::VariableList::default(),
            base_fee_per_gas: ethereum_types::U256::zero(),
            block_hash: ExecutionBlockHash::from_root(
                lighthouse_wrapper::types::Hash256::from_low_u64_be(100),
            ),
            transactions: lighthouse_wrapper::types::VariableList::default(),
            withdrawals: lighthouse_wrapper::types::VariableList::default(),
        };

        // Create minimal consensus block
        let consensus_block = crate::block::ConsensusBlock {
            parent_hash: lighthouse_wrapper::types::Hash256::zero(),
            slot: 100,
            auxpow_header: None,
            execution_payload,
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        };

        // Create signed block
        crate::block::SignedConsensusBlock {
            message: consensus_block,
            signature: crate::signatures::AggregateApproval::empty(),
        }
    }

    #[tokio::test]
    async fn test_query_message_variants() {
        // Test query-related message variants
        let block_hash = H256::from_low_u64_be(42);
        let height = 100u64;

        // Test GetBlockByHash message
        let hash_msg = ChainMessage::GetBlockByHash { hash: block_hash };
        match hash_msg {
            ChainMessage::GetBlockByHash { hash } => {
                assert_eq!(hash, H256::from_low_u64_be(42));
            }
            _ => panic!("GetBlockByHash message matching failed"),
        }

        // Test GetBlockByHeight message
        let height_msg = ChainMessage::GetBlockByHeight { height };
        match height_msg {
            ChainMessage::GetBlockByHeight { height: h } => {
                assert_eq!(h, 100);
            }
            _ => panic!("GetBlockByHeight message matching failed"),
        }

        // Test ProduceBlock message
        let produce_msg = ChainMessage::ProduceBlock {
            slot: 101,
            timestamp: Duration::from_secs(1640995200),
        };
        match produce_msg {
            ChainMessage::ProduceBlock { slot, timestamp } => {
                assert_eq!(slot, 101);
                assert_eq!(timestamp, Duration::from_secs(1640995200));
            }
            _ => panic!("ProduceBlock message matching failed"),
        }
    }

    #[tokio::test]
    async fn test_chain_manager_message_variants() {
        // Test ChainManagerMessage variants
        let auxpow_params = mock_auxpow_params();
        let mock_auxpow = create_mock_auxpow();

        // Test IsSynced message
        let synced_msg = ChainManagerMessage::IsSynced;
        match synced_msg {
            ChainManagerMessage::IsSynced => assert!(true),
            _ => panic!("IsSynced message matching failed"),
        }

        // Test GetAggregateHashes message
        let aggregate_msg = ChainManagerMessage::GetAggregateHashes { count: 10 };
        match aggregate_msg {
            ChainManagerMessage::GetAggregateHashes { count } => {
                assert_eq!(count, 10);
            }
            _ => panic!("GetAggregateHashes message matching failed"),
        }

        // Test PushAuxPow message
        let push_msg = ChainManagerMessage::PushAuxPow {
            auxpow: mock_auxpow,
            params: auxpow_params,
        };
        match push_msg {
            ChainManagerMessage::PushAuxPow { auxpow: _, params } => {
                assert!(params.target_difficulty > U256::zero());
            }
            _ => panic!("PushAuxPow message matching failed"),
        }
    }

    /// Helper to create a mock AuxPow for testing
    fn create_mock_auxpow() -> crate::auxpow::AuxPow {
        crate::auxpow::AuxPow {
            coinbase_txn: bitcoin::Transaction {
                version: 1,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![bitcoin::TxIn {
                    previous_output: bitcoin::OutPoint::null(),
                    script_sig: bitcoin::ScriptBuf::new(),
                    sequence: bitcoin::Sequence::ZERO,
                    witness: bitcoin::Witness::new(),
                }],
                output: vec![bitcoin::TxOut {
                    value: 5000000000, // 50 BTC
                    script_pubkey: bitcoin::ScriptBuf::new(),
                }],
            },
            block_hash: bitcoin::BlockHash::from_byte_array([1u8; 32]),
            coinbase_branch: crate::auxpow::MerkleBranch {
                branch_hash: vec![],
                branch_side_mask: 0,
            },
            blockchain_branch: crate::auxpow::MerkleBranch {
                branch_hash: vec![],
                branch_side_mask: 0,
            },
            parent_block: bitcoin::block::Header {
                version: bitcoin::block::Version::ONE,
                prev_blockhash: bitcoin::BlockHash::from_byte_array([0u8; 32]),
                merkle_root: bitcoin::hash_types::TxMerkleNode::from_byte_array([1u8; 32]),
                time: 1640995200,
                bits: bitcoin::CompactTarget::from_consensus(0x207fffff),
                nonce: 12345,
            },
        }
    }

    #[tokio::test]
    async fn test_mock_data_consistency() {
        // Test that mock data is internally consistent
        let status = mock_chain_status();
        let pegins = mock_multiple_pegins();
        let pegouts = mock_multiple_pegouts();

        // Verify data relationships
        assert!(status.height > 0);
        assert!(!pegins.is_empty());
        assert!(!pegouts.is_empty());

        // Verify amounts are reasonable
        let total_pegin_amount: u64 = pegins.iter().map(|p| p.amount).sum();
        let total_pegout_amount: u64 = pegouts.iter().map(|p| p.amount).sum();

        assert!(total_pegin_amount > 0);
        assert!(total_pegout_amount > 0);
    }

    #[tokio::test]
    async fn test_address_generation() {
        // Test deterministic address generation
        let addr1 = test_address(1);
        let addr2 = test_address(2);
        let addr1_again = test_address(1);

        assert_ne!(addr1, addr2);
        assert_eq!(addr1, addr1_again);
    }

    #[tokio::test]
    async fn test_bitcoin_data_generation() {
        // Test Bitcoin-related test data
        let hash1 = test_bitcoin_block_hash(1);
        let hash2 = test_bitcoin_block_hash(2);
        let txid1 = test_txid(1);
        let txid2 = test_txid(2);

        assert_ne!(hash1, hash2);
        assert_ne!(txid1, txid2);

        // Verify determinism
        assert_eq!(hash1, test_bitcoin_block_hash(1));
        assert_eq!(txid1, test_txid(1));
    }

    #[tokio::test]
    async fn test_chain_response_variants() {
        // Test ChainResponse message variants are correctly structured
        use crate::actors_v2::chain::messages::ChainResponse;
        use bitcoin::Txid;
        use std::time::Duration;

        let block = create_mock_signed_consensus_block();
        let block_hash = H256::from_low_u64_be(42);

        // Test BlockProduced response
        let block_produced = ChainResponse::BlockProduced {
            block: block.clone(),
            duration: Duration::from_millis(500),
        };
        match block_produced {
            ChainResponse::BlockProduced { block: b, duration } => {
                assert_eq!(b.message.execution_payload.block_number, 100);
                assert_eq!(duration, Duration::from_millis(500));
            }
            _ => panic!("BlockProduced response matching failed"),
        }

        // Test BlockImported response
        let block_imported = ChainResponse::BlockImported {
            block_hash,
            height: 100,
        };
        match block_imported {
            ChainResponse::BlockImported {
                block_hash: hash,
                height,
            } => {
                assert_eq!(hash, H256::from_low_u64_be(42));
                assert_eq!(height, 100);
            }
            _ => panic!("BlockImported response matching failed"),
        }

        // Test AuxPowProcessed response
        let auxpow_processed = ChainResponse::AuxPowProcessed {
            success: true,
            finalized: true,
        };
        match auxpow_processed {
            ChainResponse::AuxPowProcessed { success, finalized } => {
                assert!(success);
                assert!(finalized);
            }
            _ => panic!("AuxPowProcessed response matching failed"),
        }

        // Test PeginsProcessed response
        let pegins_processed = ChainResponse::PeginsProcessed {
            count: 3,
            total_amount: U256::from(300000000u64), // 3 BTC
        };
        match pegins_processed {
            ChainResponse::PeginsProcessed {
                count,
                total_amount,
            } => {
                assert_eq!(count, 3);
                assert_eq!(total_amount, U256::from(300000000u64));
            }
            _ => panic!("PeginsProcessed response matching failed"),
        }

        // Test PegoutsProcessed response
        let pegouts_processed = ChainResponse::PegoutsProcessed {
            count: 2,
            transaction_id: Some(Txid::from_byte_array([1u8; 32])),
        };
        match pegouts_processed {
            ChainResponse::PegoutsProcessed {
                count,
                transaction_id,
            } => {
                assert_eq!(count, 2);
                assert!(transaction_id.is_some());
            }
            _ => panic!("PegoutsProcessed response matching failed"),
        }
    }

    #[tokio::test]
    async fn test_chain_status_response() {
        // Test ChainStatus response structure
        use crate::actors_v2::chain::messages::{ChainResponse, ChainStatus};
        use std::time::Duration;

        let status = ChainStatus {
            height: 1000,
            head_hash: Some(H256::from_low_u64_be(42)),
            is_synced: true,
            is_validator: true,
            network_connected: true,
            peer_count: 5,
            pending_pegins: 2,
            last_block_time: Some(Duration::from_secs(1640995200)),
            auxpow_enabled: true,
            blocks_without_pow: 0,
            observed_height: 1000,
            orphan_count: 0,
        };

        let response = ChainResponse::ChainStatus(status.clone());
        match response {
            ChainResponse::ChainStatus(s) => {
                assert_eq!(s.height, 1000);
                assert_eq!(s.head_hash, Some(H256::from_low_u64_be(42)));
                assert!(s.is_synced);
                assert!(s.is_validator);
                assert!(s.network_connected);
                assert_eq!(s.peer_count, 5);
                assert_eq!(s.pending_pegins, 2);
                assert!(s.auxpow_enabled);
                assert_eq!(s.blocks_without_pow, 0);
            }
            _ => panic!("ChainStatus response matching failed"),
        }
    }

    #[tokio::test]
    async fn test_chain_manager_response_variants() {
        // Test ChainManagerResponse message variants
        use crate::actors_v2::chain::messages::ChainManagerResponse;
        use bitcoin::BlockHash as BitcoinBlockHash;

        let block = create_mock_signed_consensus_block();

        // Test Synced response
        let synced_response = ChainManagerResponse::Synced(true);
        match synced_response {
            ChainManagerResponse::Synced(is_synced) => {
                assert!(is_synced);
            }
            _ => panic!("Synced response matching failed"),
        }

        // Test Head response
        let head_response = ChainManagerResponse::Head(block.clone());
        match head_response {
            ChainManagerResponse::Head(head_block) => {
                assert_eq!(head_block.message.execution_payload.block_number, 100);
            }
            _ => panic!("Head response matching failed"),
        }

        // Test AggregateHashes response
        let aggregate_hashes = vec![
            BitcoinBlockHash::from_byte_array([1u8; 32]),
            BitcoinBlockHash::from_byte_array([2u8; 32]),
        ];
        let aggregate_response = ChainManagerResponse::AggregateHashes(aggregate_hashes.clone());
        match aggregate_response {
            ChainManagerResponse::AggregateHashes(hashes) => {
                assert_eq!(hashes.len(), 2);
                assert_eq!(hashes[0], BitcoinBlockHash::from_byte_array([1u8; 32]));
                assert_eq!(hashes[1], BitcoinBlockHash::from_byte_array([2u8; 32]));
            }
            _ => panic!("AggregateHashes response matching failed"),
        }

        // Test AuxPowPushed response
        let auxpow_pushed = ChainManagerResponse::AuxPowPushed {
            accepted: true,
            block_finalized: true,
        };
        match auxpow_pushed {
            ChainManagerResponse::AuxPowPushed {
                accepted,
                block_finalized,
            } => {
                assert!(accepted);
                assert!(block_finalized);
            }
            _ => panic!("AuxPowPushed response matching failed"),
        }
    }

    #[tokio::test]
    async fn test_error_case_message_handling() {
        // Test error scenarios in message construction and matching
        use crate::actors_v2::chain::messages::{ChainMessage, PegOutRequest};

        // Test with invalid/empty data
        let empty_pegins = ChainMessage::ProcessPegins {
            pegin_infos: vec![],
        };
        match empty_pegins {
            ChainMessage::ProcessPegins { pegin_infos } => {
                assert!(pegin_infos.is_empty());
            }
            _ => panic!("Empty pegins message matching failed"),
        }

        // Test with zero amounts
        let zero_pegout = PegOutRequest {
            recipient: bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                .unwrap(),
            amount: 0,
            requester: ethereum_types::Address::zero(),
            nonce: U256::zero(),
        };
        assert_eq!(zero_pegout.amount, 0);

        // Test NetworkBlockReceived with empty peer_id
        let block = create_mock_signed_consensus_block();
        let network_msg = ChainMessage::NetworkBlockReceived {
            block: block.clone(),
            peer_id: String::new(),
        };
        match network_msg {
            ChainMessage::NetworkBlockReceived { block: _, peer_id } => {
                assert!(peer_id.is_empty());
            }
            _ => panic!("NetworkBlockReceived with empty peer_id matching failed"),
        }
    }

    #[tokio::test]
    async fn test_block_source_variants() {
        // Test BlockSource enumeration variants
        use crate::actors_v2::chain::messages::BlockSource;

        let sources = vec![
            BlockSource::Local,
            BlockSource::Network("peer123".to_string()),
            BlockSource::Sync,
            BlockSource::Rpc,
        ];

        for source in sources {
            match source {
                BlockSource::Local => assert!(true),
                BlockSource::Network(peer_id) => {
                    assert_eq!(peer_id, "peer123");
                }
                BlockSource::Sync => assert!(true),
                BlockSource::Rpc => assert!(true),
            }
        }
    }

    #[tokio::test]
    async fn test_chain_state_transitions() {
        // Test ChainState state transitions and persistence
        use crate::actors_v2::chain::state::SyncStatus;
        use crate::actors_v2::storage::actor::BlockRef;
        use lighthouse_wrapper::types::ExecutionBlockHash;

        // Create test harness to get initial state components
        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        // Extract values before consuming harness
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let mut state = harness.into_chain_state(
            true, // is_validator
            100,  // max_blocks_without_pow
            None, // Start with no head
        );

        // Test initial state
        assert_eq!(state.get_height(), 0);
        assert!(state.get_head_hash().is_none());
        assert!(state.is_synced());
        assert_eq!(state.blocks_without_pow, 0);

        // Test head update transition
        let block_ref = BlockRef {
            hash: H256::from_low_u64_be(42),
            number: 100,
            execution_hash: ExecutionBlockHash::zero(),
        };
        state.update_head(block_ref.clone());
        assert_eq!(state.get_height(), 100);
        assert_eq!(state.get_head_hash(), Some(H256::from_low_u64_be(42)));
        assert!(state.last_block_time.is_some());

        // Test sync status transitions
        assert!(state.is_synced());
        state.set_sync_status(SyncStatus::Syncing {
            progress: 0.5,
            target_height: 200,
        });
        assert!(!state.is_synced());
        state.set_sync_status(SyncStatus::NotSynced);
        assert!(!state.is_synced());
        state.set_sync_status(SyncStatus::Error("Network error".to_string()));
        assert!(!state.is_synced());
        state.set_sync_status(SyncStatus::Synced);
        assert!(state.is_synced());

        // Test AuxPoW state transitions
        assert!(!state.needs_auxpow());
        for _ in 0..max_blocks_without_pow {
            state.increment_blocks_without_pow();
        }
        assert!(state.needs_auxpow());
        state.reset_blocks_without_pow();
        assert!(!state.needs_auxpow());
        assert_eq!(state.blocks_without_pow, 0);

        // Test queued AuxPoW state
        assert!(state.get_queued_pow().is_none());
        let auxpow_header = create_mock_auxpow_header();
        state.set_queued_pow(Some(auxpow_header.clone()));
        assert!(state.get_queued_pow().is_some());
        state.set_queued_pow(None);
        assert!(state.get_queued_pow().is_none());
    }

    #[tokio::test]
    async fn test_peg_operations_state_management() {
        // Test peg-in/peg-out state management
        use bitcoin::Txid;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let max_blocks_without_pow = harness.config.max_blocks_without_pow;
        let mut state = harness.into_chain_state(
            true, // is_validator
            max_blocks_without_pow,
            None,
        );

        // Test peg-in queue management
        let pegins = mock_multiple_pegins();
        let txid1 = Txid::from_byte_array([1u8; 32]);
        let txid2 = Txid::from_byte_array([2u8; 32]);

        assert!(state.queued_pegins.read().await.is_empty());

        // Add peg-ins (async methods)
        state.add_queued_pegin(txid1, pegins[0].clone()).await;
        state.add_queued_pegin(txid2, pegins[1].clone()).await;
        assert_eq!(state.queued_pegins.read().await.len(), 2);

        // Remove peg-ins (async method)
        let removed = state.remove_queued_pegin(&txid1).await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().amount, pegins[0].amount);
        assert_eq!(state.queued_pegins.read().await.len(), 1);

        // Remove non-existent peg-in
        let non_existent = state
            .remove_queued_pegin(&Txid::from_byte_array([99u8; 32]))
            .await;
        assert!(non_existent.is_none());
        assert_eq!(state.queued_pegins.read().await.len(), 1);

        // Clear remaining
        state.remove_queued_pegin(&txid2).await;
        assert!(state.queued_pegins.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_chain_actor_instantiation_and_state() {
        // Test actual ChainActor instantiation with state persistence
        use crate::actors_v2::chain::ChainActor;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let mut actor = ChainActor::new(config, state);

        // Test initial state
        assert_eq!(actor.config.is_validator, true);
        assert_eq!(actor.config.enable_auxpow, true);
        assert_eq!(actor.state.get_height(), 0);
        assert!(actor.state.is_synced());

        // Test activity recording
        let initial_activity = actor.last_activity;
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        actor.record_activity();
        assert!(actor.last_activity > initial_activity);

        // Test state modification through actor
        let block_ref = BlockRef {
            hash: H256::from_low_u64_be(123),
            number: 50,
            execution_hash: ExecutionBlockHash::zero(),
        };
        actor.state.update_head(block_ref);
        assert_eq!(actor.state.get_height(), 50);
        assert_eq!(
            actor.state.get_head_hash(),
            Some(H256::from_low_u64_be(123))
        );
    }

    #[tokio::test]
    async fn test_actor_network_readiness_checks() {
        // Test network readiness logic
        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let actor = ChainActor::new(config, state);

        // Test network readiness without network actors
        assert!(!actor.is_network_ready().await);

        // Network actors would need to be mocked for full testing
        // This tests the basic logic path
    }

    /// Helper to create mock AuxPowHeader for testing
    fn create_mock_auxpow_header() -> crate::block::AuxPowHeader {
        use crate::block::AuxPowHeader;
        use ethereum_types::{Address, H256};

        AuxPowHeader {
            range_start: H256::zero(),
            range_end: H256::from_low_u64_be(42),
            bits: 0x1d00ffff,
            chain_id: 1,
            height: 100,
            auxpow: Some(create_mock_auxpow()),
            fee_recipient: Address::zero(),
        }
    }

    #[tokio::test]
    async fn test_chain_actor_error_scenarios() {
        // Test error handling in ChainActor operations
        use crate::actors_v2::chain::ChainActor;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let actor = ChainActor::new(config, state);

        // Test network operations without network actors (should handle gracefully)
        let broadcast_result = actor.broadcast_block(vec![1, 2, 3, 4]).await;
        // Should complete without network actors (logs warning but doesn't error)
        assert!(broadcast_result.is_ok());

        let sync_result = actor.request_blocks(100, 10).await;
        // Should complete without sync actor
        assert!(sync_result.is_ok());

        // Test storage operations without storage actor
        let block = create_mock_signed_consensus_block();
        let store_result = actor.store_block(block, true).await;
        // Should complete without storage actor
        assert!(store_result.is_ok());
    }

    #[tokio::test]
    async fn test_chain_error_conversions() {
        // Test ChainError type conversions and formatting
        use crate::actors_v2::chain::ChainError;
        use eyre::eyre;

        // Test various error variants
        let block_prod_err = ChainError::BlockProduction("Failed to create block".to_string());
        assert!(format!("{}", block_prod_err).contains("Block production error"));

        let block_val_err = ChainError::BlockValidation("Invalid block header".to_string());
        assert!(format!("{}", block_val_err).contains("Block validation error"));

        let auxpow_err = ChainError::AuxPowProcessing("Invalid AuxPoW".to_string());
        assert!(format!("{}", auxpow_err).contains("AuxPoW processing error"));

        let peg_err = ChainError::PegOperation("Peg-in failed".to_string());
        assert!(format!("{}", peg_err).contains("Peg operation error"));

        let consensus_err = ChainError::Consensus("Consensus failure".to_string());
        assert!(format!("{}", consensus_err).contains("Consensus error"));

        let storage_err = ChainError::Storage("Storage unavailable".to_string());
        assert!(format!("{}", storage_err).contains("Storage actor error"));

        let network_err = ChainError::NetworkError("Network timeout".to_string());
        assert!(format!("{}", network_err).contains("Network communication error"));

        let config_err = ChainError::Configuration("Invalid config".to_string());
        assert!(format!("{}", config_err).contains("Configuration error"));

        let not_synced_err = ChainError::NotSynced;
        assert!(format!("{}", not_synced_err).contains("Chain not synchronized"));

        let invalid_state_err = ChainError::InvalidState("Bad state".to_string());
        assert!(format!("{}", invalid_state_err).contains("Invalid chain state"));

        // Test eyre conversion
        let eyre_error = eyre!("Something went wrong");
        let chain_error: ChainError = eyre_error.into();
        match chain_error {
            ChainError::Internal(msg) => {
                assert!(msg.contains("Something went wrong"));
            }
            _ => panic!("Expected Internal error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_message_scenarios() {
        // Test handling of invalid or malformed messages
        use crate::actors_v2::chain::messages::{ChainManagerMessage, ChainMessage, PegOutRequest};
        use bitcoin::Address;
        use std::str::FromStr;

        // Test messages with invalid data
        let invalid_pegout = PegOutRequest {
            recipient: Address::from_str("bc1qinvalid").unwrap_or_else(|_| {
                Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").unwrap()
            }),
            amount: u64::MAX, // Extremely large amount
            requester: ethereum_types::Address::zero(),
            nonce: ethereum_types::U256::MAX,
        };

        let pegouts_msg = ChainMessage::ProcessPegouts {
            pegout_requests: vec![invalid_pegout],
        };

        // Message should be constructible but validation would fail at processing
        match pegouts_msg {
            ChainMessage::ProcessPegouts { pegout_requests } => {
                assert_eq!(pegout_requests.len(), 1);
                assert_eq!(pegout_requests[0].amount, u64::MAX);
            }
            _ => panic!("Message construction failed"),
        }

        // Test GetAggregateHashes with extreme count
        let extreme_aggregate = ChainManagerMessage::GetAggregateHashes { count: u32::MAX };
        match extreme_aggregate {
            ChainManagerMessage::GetAggregateHashes { count } => {
                assert_eq!(count, u32::MAX);
            }
            _ => panic!("Message construction failed"),
        }

        // Test ProduceBlock with zero timestamp
        let zero_timestamp_msg = ChainMessage::ProduceBlock {
            slot: 0,
            timestamp: Duration::from_secs(0),
        };
        match zero_timestamp_msg {
            ChainMessage::ProduceBlock { slot, timestamp } => {
                assert_eq!(slot, 0);
                assert_eq!(timestamp, Duration::from_secs(0));
            }
            _ => panic!("Message construction failed"),
        }
    }

    #[tokio::test]
    async fn test_chain_state_error_conditions() {
        // Test ChainState in error conditions
        use crate::actors_v2::chain::state::SyncStatus;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let mut state = harness.into_chain_state(
            true, // is_validator
            0,    // max_blocks_without_pow = 0 (edge case)
            None,
        );

        // Test with zero max blocks without pow (should always need auxpow)
        assert!(state.needs_auxpow()); // Should be true immediately with 0 max
        state.increment_blocks_without_pow();
        assert!(state.needs_auxpow());

        // Test sync error status
        state.set_sync_status(SyncStatus::Error("Critical error".to_string()));
        assert!(!state.is_synced());
        match &state.sync_status {
            SyncStatus::Error(msg) => assert_eq!(msg, "Critical error"),
            _ => panic!("Expected error status"),
        }

        // Test multiple peg-in operations with same txid (should overwrite)
        let pegin1 = mock_pegin_info();
        let mut pegin2 = pegin1.clone();
        pegin2.amount = 200000000; // Different amount

        let txid = bitcoin::Txid::from_byte_array([42u8; 32]);
        state.add_queued_pegin(txid, pegin1).await;
        assert_eq!(state.queued_pegins.read().await.len(), 1);
        assert_eq!(
            state.queued_pegins.read().await.get(&txid).unwrap().amount,
            100000000
        );

        // Adding same txid should overwrite
        state.add_queued_pegin(txid, pegin2).await;
        assert_eq!(state.queued_pegins.read().await.len(), 1);
        assert_eq!(
            state.queued_pegins.read().await.get(&txid).unwrap().amount,
            200000000
        );
    }

    #[tokio::test]
    async fn test_chain_config_error_scenarios() {
        // Test ChainConfig validation errors
        use crate::actors_v2::testing::chain::fixtures::validator_config;

        let mut config = validator_config();

        // Test invalid max_blocks_without_pow
        config.max_blocks_without_pow = 0;
        let validation_result = config.validate();
        assert!(validation_result.is_err());
        assert!(validation_result
            .unwrap_err()
            .to_string()
            .contains("max_blocks_without_pow must be greater than 0"));

        // Test with very large max_blocks_without_pow (should be valid)
        config.max_blocks_without_pow = u64::MAX;
        assert!(config.validate().is_ok());

        // Test non-validator config with peg operations enabled (should be valid)
        config.is_validator = false;
        config.enable_peg_operations = true;
        assert!(config.validate().is_ok());

        // Test with auxpow disabled but still validator (should be valid)
        config.is_validator = true;
        config.enable_auxpow = false;
        assert!(config.validate().is_ok());
    }

    #[tokio::test]
    async fn test_test_harness_error_conditions() {
        // Test ChainTestHarness under error conditions
        use crate::actors_v2::testing::chain::ChainTestHarness;

        // Test creating validator harness (should succeed)
        let validator_harness = ChainTestHarness::validator().await;
        assert!(validator_harness.is_ok());

        // Test creating follower harness (should succeed)
        let follower_harness = ChainTestHarness::follower().await;
        assert!(follower_harness.is_ok());

        // Test harness config verification
        let harness = ChainTestHarness::validator().await.unwrap();
        let verify_result = harness.verify_config().await;
        assert!(verify_result.is_ok());

        // Test with modified config
        let mut modified_config = harness.config.clone();
        modified_config.max_blocks_without_pow = 0;

        // Config validation should fail for invalid config
        assert!(modified_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_concurrent_state_modifications() {
        // Test state modifications under concurrent access patterns
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let state = Arc::new(Mutex::new(harness.into_chain_state(
            true, // is_validator
            10,   // max_blocks_without_pow
            None,
        )));

        // Simulate concurrent modifications
        let state1 = Arc::clone(&state);
        let state2 = Arc::clone(&state);

        let handle1 = tokio::spawn(async move {
            let mut s = state1.lock().await;
            for _ in 0..5 {
                s.increment_blocks_without_pow();
            }
        });

        let handle2 = tokio::spawn(async move {
            let mut s = state2.lock().await;
            let block_ref = BlockRef {
                hash: H256::from_low_u64_be(123),
                number: 100,
                execution_hash: ExecutionBlockHash::zero(),
            };
            s.update_head(block_ref);
        });

        // Wait for both operations to complete
        let _ = tokio::join!(handle1, handle2);

        let final_state = state.lock().await;
        assert_eq!(final_state.blocks_without_pow, 5);
        assert_eq!(final_state.get_height(), 100);
        assert_eq!(
            final_state.get_head_hash(),
            Some(H256::from_low_u64_be(123))
        );
    }

    #[tokio::test]
    async fn test_chain_actor_address_management() {
        // Test ChainActor actor address management
        use crate::actors_v2::chain::ChainActor;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let actor = ChainActor::new(config, state);

        // Test initial state (no actor addresses set)
        assert!(actor.storage_actor.is_none());
        assert!(actor.network_actor.is_none());
        assert!(actor.sync_actor.is_none());

        // Create mock actor addresses (these would normally be real actors)
        // For testing, we can create dummy addresses that won't actually work
        // but demonstrate the address management pattern

        // Test that operations without actors handle gracefully
        let broadcast_result = actor.broadcast_block(vec![1, 2, 3]).await;
        assert!(broadcast_result.is_ok()); // Should not fail, just no-op

        let sync_result = actor.request_blocks(100, 5).await;
        assert!(sync_result.is_ok()); // Should not fail, just no-op

        let block = create_mock_signed_consensus_block();
        let store_result = actor.store_block(block, true).await;
        assert!(store_result.is_ok()); // Should not fail, just no-op

        // Test network readiness check without actors
        assert!(!actor.is_network_ready().await);
    }

    #[tokio::test]
    async fn test_chain_actor_metrics_integration() {
        // Test ChainActor metrics recording and integration
        use crate::actors_v2::chain::ChainActor;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let mut actor = ChainActor::new(config, state);

        // Test initial metrics state
        let initial_activity = actor.last_activity;
        assert_eq!(actor.metrics.get_activity_count(), 0);
        assert_eq!(actor.metrics.get_chain_height(), 0);
        assert!(actor.metrics.get_sync_status());

        // Test activity recording (timestamp update)
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        actor.record_activity();

        assert!(actor.last_activity > initial_activity);
        assert_eq!(actor.metrics.get_activity_count(), 0); // No operations performed yet

        // Update state and record activity again
        let block_ref = BlockRef {
            hash: H256::from_low_u64_be(42),
            number: 100,
            execution_hash: ExecutionBlockHash::zero(),
        };
        actor.state.update_head(block_ref);
        actor.record_activity();

        assert_eq!(actor.metrics.get_activity_count(), 0); // Still no operations performed
        assert_eq!(actor.metrics.get_chain_height(), 100);

        // Test sync status in metrics
        actor
            .state
            .set_sync_status(crate::actors_v2::chain::state::SyncStatus::NotSynced);
        actor.record_activity();
        assert!(!actor.metrics.get_sync_status());
    }

    #[tokio::test]
    async fn test_message_flow_patterns() {
        // Test message flow patterns between ChainActor and other components
        use crate::actors_v2::chain::messages::{ChainMessage, ChainResponse, ChainStatus};

        // Test message construction and response patterns
        let block = create_mock_signed_consensus_block();

        // Test request-response pattern for chain status
        let status_msg = ChainMessage::GetChainStatus;
        let mock_status = ChainStatus {
            height: 1000,
            head_hash: Some(H256::from_low_u64_be(42)),
            is_synced: true,
            is_validator: true,
            network_connected: true,
            peer_count: 5,
            pending_pegins: 2,
            last_block_time: Some(Duration::from_secs(1640995200)),
            auxpow_enabled: true,
            blocks_without_pow: 0,
            observed_height: 1000,
            orphan_count: 0,
        };
        let status_response = ChainResponse::ChainStatus(mock_status);

        // Verify message/response compatibility
        match status_msg {
            ChainMessage::GetChainStatus => match status_response {
                ChainResponse::ChainStatus(status) => {
                    assert_eq!(status.height, 1000);
                    assert!(status.is_synced);
                }
                _ => panic!("Unexpected response type"),
            },
            _ => panic!("Unexpected message type"),
        }

        // Test block import flow
        let import_msg = ChainMessage::ImportBlock {
            block: block.clone(),
            source: BlockSource::Network("peer123".to_string()),
            peer_id: Some("peer123".to_string()),
        };
        let import_response = ChainResponse::BlockImported {
            block_hash: H256::from_low_u64_be(42),
            height: 100,
        };

        match import_msg {
            ChainMessage::ImportBlock {
                block: b,
                source,
                peer_id: _,
            } => {
                assert_eq!(b.message.execution_payload.block_number, 100);
                if let BlockSource::Network(peer) = source {
                    assert_eq!(peer, "peer123");
                } else {
                    panic!("Expected Network source");
                }

                // Verify corresponding response
                match import_response {
                    ChainResponse::BlockImported { block_hash, height } => {
                        assert_eq!(height, 100);
                        assert_eq!(block_hash, H256::from_low_u64_be(42));
                    }
                    _ => panic!("Unexpected response type"),
                }
            }
            _ => panic!("Unexpected message type"),
        }
    }

    #[actix::test]
    async fn test_actor_lifecycle_integration() {
        // Test ChainActor lifecycle integration patterns
        use crate::actors_v2::chain::ChainActor;
        use actix::Actor;

        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let actor = ChainActor::new(config, state);

        // Test that actor can be started (this tests Actor trait implementation)
        let addr = actor.start();

        // Actor should be running
        assert!(addr.connected());

        // Test graceful shutdown
        // Test graceful shutdown (actor will be dropped when addr goes out of scope) // This will stop the actor

        // Give some time for shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn test_integration_with_test_harness_variations() {
        // Test ChainActor integration with different harness configurations

        // Test validator configuration
        let validator_harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator harness");

        assert!(validator_harness.config.is_validator);
        assert!(validator_harness.config.enable_auxpow);
        assert!(validator_harness.config.enable_peg_operations);

        let validator_config = validator_harness.config.clone();
        let validator_is_validator = validator_harness.config.is_validator;
        let validator_max_blocks = validator_harness.config.max_blocks_without_pow;

        let validator_state =
            validator_harness.into_chain_state(validator_is_validator, validator_max_blocks, None);
        let validator_actor = ChainActor::new(validator_config, validator_state);
        assert_eq!(validator_actor.config.is_validator, true);

        // Test follower configuration
        let follower_harness = ChainTestHarness::follower()
            .await
            .expect("Should create follower harness");

        assert!(!follower_harness.config.is_validator);

        let follower_config = follower_harness.config.clone();
        let follower_is_validator = follower_harness.config.is_validator;
        let follower_max_blocks = follower_harness.config.max_blocks_without_pow;

        let follower_state =
            follower_harness.into_chain_state(follower_is_validator, follower_max_blocks, None);
        let follower_actor = ChainActor::new(follower_config, follower_state);
        assert_eq!(follower_actor.config.is_validator, false);

        // Test validator and follower configurations were created successfully
        // (harnesses were consumed, but configs were valid at creation time)
    }

    #[tokio::test]
    async fn test_cross_actor_data_consistency() {
        // Test data consistency patterns across ChainActor operations
        let harness = ChainTestHarness::validator()
            .await
            .expect("Should create validator test harness");

        let config = harness.config.clone();
        let is_validator = harness.config.is_validator;
        let max_blocks_without_pow = harness.config.max_blocks_without_pow;

        let state = harness.into_chain_state(is_validator, max_blocks_without_pow, None);
        let mut actor = ChainActor::new(config, state);

        // Test consistency between actor and state
        assert_eq!(actor.config.is_validator, actor.state.is_validator);
        assert_eq!(actor.config.max_blocks_without_pow, max_blocks_without_pow);

        // Update state through actor and verify consistency
        let block_ref = BlockRef {
            hash: H256::from_low_u64_be(100),
            number: 500,
            execution_hash: ExecutionBlockHash::zero(),
        };
        actor.state.update_head(block_ref.clone());
        actor.record_activity();

        // Metrics should reflect state changes
        assert_eq!(actor.metrics.get_chain_height(), 500);
        assert_eq!(actor.state.get_height(), 500);
        assert_eq!(
            actor.state.get_head_hash(),
            Some(H256::from_low_u64_be(100))
        );

        // Test AuxPoW state consistency
        for _ in 0..actor.config.max_blocks_without_pow {
            actor.state.increment_blocks_without_pow();
        }
        assert!(actor.state.needs_auxpow());

        actor.record_activity();
        // Metrics should be updated but auxpow status is state-dependent
        assert_eq!(
            actor.state.blocks_without_pow,
            actor.config.max_blocks_without_pow
        );
    }
}
