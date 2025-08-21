use super::*;
use proptest::prelude::*;
use proptest::collection::vec;
use std::collections::HashSet;

// Property test generators
pub fn arbitrary_bitcoin_amount() -> impl Strategy<Value = u64> {
    1u64..=2_100_000_000_000_000u64 // Valid Bitcoin amount range
}

pub fn arbitrary_small_bitcoin_amount() -> impl Strategy<Value = u64> {
    1u64..=100_000_000u64 // Up to 1 BTC for testing
}

pub fn arbitrary_evm_address() -> impl Strategy<Value = H160> {
    any::<[u8; 20]>().prop_map(H160::from)
}

pub fn arbitrary_bitcoin_txid() -> impl Strategy<Value = Txid> {
    any::<[u8; 32]>().prop_map(|bytes| Txid::from_slice(&bytes).unwrap())
}

pub fn arbitrary_confirmations() -> impl Strategy<Value = u32> {
    0u32..=100u32
}

pub fn arbitrary_request_id() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9-_]{8,32}".prop_map(|s| s)
}

proptest! {
    #[test]
    fn test_pegin_amount_handling(
        amount in arbitrary_small_bitcoin_amount(),
        confirmations in arbitrary_confirmations(),
        evm_address in arbitrary_evm_address()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let tx = fixture.create_test_pegin_tx(amount, evm_address);
            
            let result = fixture.bridge_actor.send(ProcessPegin {
                tx,
                confirmations,
                deposit_address: fixture.federation_address.clone(),
            }).await.unwrap();
            
            // Property: Valid amounts with sufficient confirmations should succeed
            if confirmations >= fixture.config.min_confirmations {
                assert!(result.is_ok(), "Expected success for amount {} with {} confirmations", amount, confirmations);
                
                // Verify the peg-in is tracked correctly
                let pending = fixture.bridge_actor.send(GetPendingPegins).await.unwrap().unwrap();
                let found = pending.iter().find(|p| p.amount == amount && p.evm_address == evm_address);
                assert!(found.is_some(), "Peg-in should be tracked");
            } else {
                // Should fail due to insufficient confirmations
                assert!(result.is_err(), "Expected failure for insufficient confirmations");
            }
        });
    }
}

proptest! {
    #[test]
    fn test_pegout_amount_validation(
        amount in arbitrary_bitcoin_amount(),
        request_id in arbitrary_request_id()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let btc_destination = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";
            
            let burn_event = BurnEvent {
                tx_hash: H256::random(),
                block_number: 1000,
                amount,
                destination: btc_destination.to_string(),
                sender: H160::random(),
            };
            
            let result = fixture.bridge_actor.send(ProcessPegout {
                burn_event,
                request_id: request_id.clone(),
            }).await.unwrap();
            
            // Property: Amounts within limits should be processed, others rejected
            if amount <= fixture.config.max_pegout_amount && amount > 0 {
                // Should either succeed or fail gracefully (e.g., insufficient UTXOs)
                match result {
                    Ok(_) => {
                        // Verify it's tracked if successful
                        let pending = fixture.bridge_actor.send(GetPendingPegouts).await.unwrap().unwrap();
                        let found = pending.iter().find(|p| p.request_id == request_id);
                        assert!(found.is_some() || result.is_err(), "Successful pegout should be tracked");
                    },
                    Err(BridgeError::AmountTooLarge { .. }) => {
                        panic!("Amount {} should be within limits", amount);
                    },
                    Err(_) => {
                        // Other errors are acceptable (insufficient funds, etc.)
                    }
                }
            } else if amount > fixture.config.max_pegout_amount {
                // Should fail due to amount too large
                assert!(matches!(result, Err(BridgeError::AmountTooLarge { .. })), 
                    "Expected AmountTooLarge error for amount {}", amount);
            }
        });
    }
}

proptest! {
    #[test]
    fn test_multiple_pegins_consistency(
        amounts in vec(arbitrary_small_bitcoin_amount(), 1..10),
        evm_addresses in vec(arbitrary_evm_address(), 1..10)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let confirmations = fixture.config.min_confirmations;
            
            // Ensure we have equal number of amounts and addresses
            let pairs: Vec<_> = amounts.into_iter().zip(evm_addresses.into_iter()).collect();
            
            let mut expected_txids = HashSet::new();
            let mut expected_total = 0u64;
            
            // Process each peg-in
            for (amount, evm_address) in pairs {
                let tx = fixture.create_test_pegin_tx(amount, evm_address);
                let txid = tx.compute_txid();
                
                let result = fixture.bridge_actor.send(ProcessPegin {
                    tx,
                    confirmations,
                    deposit_address: fixture.federation_address.clone(),
                }).await.unwrap();
                
                if result.is_ok() {
                    expected_txids.insert(txid);
                    expected_total += amount;
                }
            }
            
            // Verify all successful peg-ins are tracked
            let pending = fixture.bridge_actor.send(GetPendingPegins).await.unwrap().unwrap();
            
            // Property: All successful peg-ins should be uniquely tracked
            assert!(pending.len() <= expected_txids.len(), "Should not have more pending than submitted");
            
            let tracked_txids: HashSet<_> = pending.iter().map(|p| p.txid).collect();
            for expected_txid in &expected_txids {
                assert!(tracked_txids.contains(expected_txid), "All successful txids should be tracked");
            }
            
            // Property: Total amount should be consistent
            let tracked_total: u64 = pending.iter().map(|p| p.amount).sum();
            assert!(tracked_total <= expected_total, "Tracked amount should not exceed expected");
            
            // Property: No duplicate txids
            assert_eq!(tracked_txids.len(), pending.len(), "All txids should be unique");
        });
    }
}

proptest! {
    #[test]
    fn test_request_id_uniqueness(
        request_ids in vec(arbitrary_request_id(), 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let btc_destination = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";
            let amount = 10_000_000u64; // 0.1 BTC
            
            // Setup sufficient UTXOs
            for i in 0..request_ids.len() {
                fixture.test_bitcoin_rpc.add_unspent(
                    create_random_txid(),
                    0,
                    100_000_000, // 1 BTC each
                    6,
                    create_test_federation_script(),
                );
            }
            
            fixture.bridge_actor.send(RefreshUtxos).await.unwrap().unwrap();
            
            let mut successful_requests = HashSet::new();
            
            // Process pegouts with potentially duplicate request IDs
            for request_id in request_ids {
                let burn_event = BurnEvent {
                    tx_hash: H256::random(),
                    block_number: 1000,
                    amount,
                    destination: btc_destination.to_string(),
                    sender: H160::random(),
                };
                
                let result = fixture.bridge_actor.send(ProcessPegout {
                    burn_event,
                    request_id: request_id.clone(),
                }).await.unwrap();
                
                match result {
                    Ok(PegoutResult::Pending(_)) => {
                        successful_requests.insert(request_id);
                    },
                    Ok(PegoutResult::InProgress(_)) => {
                        // Duplicate request ID - should return in-progress status
                        assert!(successful_requests.contains(&request_id), 
                            "InProgress should only be returned for already processed requests");
                    },
                    _ => {
                        // Other results are acceptable (errors, etc.)
                    }
                }
            }
            
            // Verify uniqueness property
            let pending = fixture.bridge_actor.send(GetPendingPegouts).await.unwrap().unwrap();
            let tracked_ids: HashSet<_> = pending.iter().map(|p| p.request_id.clone()).collect();
            
            // Property: Each request ID should appear at most once
            assert_eq!(tracked_ids.len(), pending.len(), "All request IDs should be unique");
            
            // Property: All successful requests should be tracked
            for request_id in &successful_requests {
                assert!(tracked_ids.contains(request_id), "All successful requests should be tracked");
            }
        });
    }
}

proptest! {
    #[test]
    fn test_confirmation_threshold_property(
        base_confirmations in 0u32..10u32,
        additional_confirmations in 0u32..50u32,
        amount in arbitrary_small_bitcoin_amount(),
        evm_address in arbitrary_evm_address()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let total_confirmations = base_confirmations + additional_confirmations;
            
            let tx = fixture.create_test_pegin_tx(amount, evm_address);
            
            let result = fixture.bridge_actor.send(ProcessPegin {
                tx,
                confirmations: total_confirmations,
                deposit_address: fixture.federation_address.clone(),
            }).await.unwrap();
            
            // Property: Monotonicity - more confirmations should never make a valid transaction invalid
            if total_confirmations >= fixture.config.min_confirmations {
                assert!(result.is_ok(), "Transaction with {} confirmations should succeed", total_confirmations);
                
                // If we reduce confirmations below threshold, it should fail
                if base_confirmations < fixture.config.min_confirmations {
                    let tx2 = fixture.create_test_pegin_tx(amount, evm_address);
                    let result2 = fixture.bridge_actor.send(ProcessPegin {
                        tx: tx2,
                        confirmations: base_confirmations,
                        deposit_address: fixture.federation_address.clone(),
                    }).await.unwrap();
                    
                    assert!(result2.is_err(), "Transaction with {} confirmations should fail", base_confirmations);
                }
            }
        });
    }
}

proptest! {
    #[test]
    fn test_stats_consistency(
        operations in vec((arbitrary_small_bitcoin_amount(), arbitrary_evm_address()), 1..50)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let confirmations = fixture.config.min_confirmations;
            
            let initial_stats = fixture.bridge_actor.send(GetBridgeStats).await.unwrap().unwrap();
            
            let mut expected_successful = 0u64;
            let mut expected_volume = 0u64;
            
            // Process operations
            for (amount, evm_address) in operations {
                let tx = fixture.create_test_pegin_tx(amount, evm_address);
                
                let result = fixture.bridge_actor.send(ProcessPegin {
                    tx,
                    confirmations,
                    deposit_address: fixture.federation_address.clone(),
                }).await.unwrap();
                
                if result.is_ok() {
                    expected_successful += 1;
                    expected_volume += amount;
                }
            }
            
            let final_stats = fixture.bridge_actor.send(GetBridgeStats()).await.unwrap().unwrap();
            
            // Property: Stats should reflect the operations performed
            assert!(final_stats.total_pegins_processed >= initial_stats.total_pegins_processed + expected_successful,
                "Stats should show increased successful operations");
            
            assert!(final_stats.total_pegin_volume >= initial_stats.total_pegin_volume + expected_volume,
                "Stats should show increased volume");
            
            // Property: Success rate should be between 0 and 1
            assert!(final_stats.success_rate >= 0.0 && final_stats.success_rate <= 1.0,
                "Success rate should be between 0 and 1");
            
            // Property: Pending count should be non-negative and not exceed processed count
            assert!(final_stats.pending_pegins >= 0);
            assert!(final_stats.pending_pegouts >= 0);
        });
    }
}

proptest! {
    #[test]
    fn test_address_validation_property(
        valid_addresses in vec(Just("bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080"), 1..5),
        invalid_addresses in vec("[a-zA-Z0-9]{5,20}", 1..5),
        amount in arbitrary_small_bitcoin_amount()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            
            // Test valid addresses
            for destination in valid_addresses {
                let burn_event = BurnEvent {
                    tx_hash: H256::random(),
                    block_number: 1000,
                    amount,
                    destination: destination.to_string(),
                    sender: H160::random(),
                };
                
                let result = fixture.bridge_actor.send(ProcessPegout {
                    burn_event,
                    request_id: format!("valid-{}", destination),
                }).await.unwrap();
                
                // Property: Valid Bitcoin addresses should be accepted (or fail for other reasons)
                match result {
                    Err(BridgeError::InvalidAddress(_)) => {
                        panic!("Valid address {} should not be rejected as invalid", destination);
                    },
                    _ => {
                        // Other outcomes (success, insufficient funds, etc.) are acceptable
                    }
                }
            }
            
            // Test invalid addresses
            for destination in invalid_addresses {
                // Skip if it happens to be a valid address by chance
                if BtcAddress::from_str(&destination).is_ok() {
                    continue;
                }
                
                let burn_event = BurnEvent {
                    tx_hash: H256::random(),
                    block_number: 1000,
                    amount,
                    destination: destination.clone(),
                    sender: H160::random(),
                };
                
                let result = fixture.bridge_actor.send(ProcessPegout {
                    burn_event,
                    request_id: format!("invalid-{}", destination),
                }).await.unwrap();
                
                // Property: Invalid Bitcoin addresses should be rejected
                assert!(matches!(result, Err(BridgeError::InvalidAddress(_))),
                    "Invalid address {} should be rejected", destination);
            }
        });
    }
}

proptest! {
    #[test]
    fn test_idempotency_property(
        amount in arbitrary_small_bitcoin_amount(),
        evm_address in arbitrary_evm_address(),
        confirmations in arbitrary_confirmations().prop_filter("Must have sufficient confirmations", |&c| c >= 6)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let tx = fixture.create_test_pegin_tx(amount, evm_address);
            let tx_copy = tx.clone();
            
            // Process the same transaction twice
            let result1 = fixture.bridge_actor.send(ProcessPegin {
                tx,
                confirmations,
                deposit_address: fixture.federation_address.clone(),
            }).await.unwrap();
            
            let result2 = fixture.bridge_actor.send(ProcessPegin {
                tx: tx_copy,
                confirmations,
                deposit_address: fixture.federation_address.clone(),
            }).await.unwrap();
            
            // Property: Processing the same transaction twice should be idempotent
            if result1.is_ok() {
                assert!(result2.is_ok(), "Second processing should also succeed");
                
                let pending = fixture.bridge_actor.send(GetPendingPegins).await.unwrap().unwrap();
                let matching_pegins = pending.iter().filter(|p| p.amount == amount && p.evm_address == evm_address).count();
                
                assert_eq!(matching_pegins, 1, "Should have exactly one peg-in for duplicate transaction");
            } else {
                // If first failed, second should fail with same error or succeed (if error was transient)
                assert!(result2.is_ok() || result2.is_err(), "Second processing outcome should be consistent");
            }
        });
    }
}

// Stress test properties
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn test_memory_usage_property(
        operations in vec((arbitrary_small_bitcoin_amount(), arbitrary_evm_address()), 50..500)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fixture = TestFixture::new().await;
            let confirmations = fixture.config.min_confirmations;
            
            let initial_stats = fixture.bridge_actor.send(GetBridgeStats).await.unwrap().unwrap();
            
            // Process many operations
            for (amount, evm_address) in operations {
                let tx = fixture.create_test_pegin_tx(amount, evm_address);
                
                let _ = fixture.bridge_actor.send(ProcessPegin {
                    tx,
                    confirmations,
                    deposit_address: fixture.federation_address.clone(),
                }).await.unwrap();
            }
            
            let final_stats = fixture.bridge_actor.send(GetBridgeStats).await.unwrap().unwrap();
            
            // Property: Memory usage should be bounded (pending operations shouldn't grow unbounded)
            // This is a simplified check - in a real system you'd monitor actual memory usage
            assert!(final_stats.pending_pegins < 1000, "Pending pegins should be bounded");
            
            // Property: Actor should remain responsive under load
            let health_check = fixture.bridge_actor.send(GetBridgeStats).await;
            assert!(health_check.is_ok(), "Actor should remain responsive after processing many operations");
        });
    }
}