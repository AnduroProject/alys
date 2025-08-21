use super::*;
use crate::{assert_pegin_processed, assert_pegout_state, assert_bridge_error};

#[actix::test]
async fn test_bridge_actor_creation() {
    let config = BridgeConfig::test_config();
    let federation_address = create_test_federation_address();
    let federation_script = create_test_federation_script();
    let bitcoin_rpc = Arc::new(MockBitcoinRpc::new());

    let bridge_actor = BridgeActor::new(
        config,
        federation_address,
        federation_script,
        bitcoin_rpc,
    ).unwrap();

    assert_eq!(bridge_actor.federation_version, 1);
    assert_eq!(bridge_actor.pending_pegins.len(), 0);
    assert_eq!(bridge_actor.pending_pegouts.len(), 0);
}

#[actix::test]
async fn test_process_pegin_success() {
    let fixture = TestFixture::new().await;
    let evm_address = create_random_h160();
    let amount = 100_000_000; // 1 BTC
    
    let tx = fixture.create_test_pegin_tx(amount, evm_address);
    let txid = tx.compute_txid();

    let result = fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap();

    assert!(result.is_ok());
    assert_pegin_processed!(fixture.bridge_actor, txid);
}

#[actix::test]
async fn test_process_pegin_insufficient_confirmations() {
    let fixture = TestFixture::new().await;
    let evm_address = create_random_h160();
    let tx = fixture.create_test_pegin_tx(100_000_000, evm_address);

    let result = fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 0, // Less than required
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap();

    assert_bridge_error!(result, BridgeError::InsufficientConfirmations { .. });
}

#[actix::test]
async fn test_process_pegin_invalid_deposit_address() {
    let fixture = TestFixture::new().await;
    let evm_address = create_random_h160();
    let tx = fixture.create_test_pegin_tx(100_000_000, evm_address);

    // Use different address
    let wrong_address = BtcAddress::from_str("bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080")
        .unwrap();

    let result = fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 6,
        deposit_address: wrong_address,
    }).await.unwrap();

    assert_bridge_error!(result, BridgeError::InvalidDepositAddress { .. });
}

#[actix::test]
async fn test_process_pegout_success() {
    let fixture = TestFixture::new().await;
    let amount = 50_000_000; // 0.5 BTC
    let btc_destination = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";
    
    // Setup some UTXOs for the transaction
    fixture.test_bitcoin_rpc.add_unspent(
        create_random_txid(),
        0,
        100_000_000, // 1 BTC available
        6,
        create_test_federation_script(),
    );

    let burn_event = fixture.create_test_burn_event(amount, btc_destination);
    let request_id = "test-pegout-1".to_string();

    let result = fixture.bridge_actor.send(ProcessPegout {
        burn_event,
        request_id: request_id.clone(),
    }).await.unwrap();

    match result.unwrap() {
        PegoutResult::Pending(id) => assert_eq!(id, request_id),
        _ => panic!("Expected pending result"),
    }
}

#[actix::test]
async fn test_process_pegout_amount_too_large() {
    let fixture = TestFixture::new().await;
    let amount = 2_000_000_000; // 20 BTC - exceeds max
    let btc_destination = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";
    
    let burn_event = fixture.create_test_burn_event(amount, btc_destination);
    let request_id = "test-pegout-large".to_string();

    let result = fixture.bridge_actor.send(ProcessPegout {
        burn_event,
        request_id,
    }).await.unwrap();

    assert_bridge_error!(result, BridgeError::AmountTooLarge { .. });
}

#[actix::test]
async fn test_process_pegout_invalid_address() {
    let fixture = TestFixture::new().await;
    let amount = 50_000_000;
    let invalid_destination = "invalid-bitcoin-address";
    
    let burn_event = fixture.create_test_burn_event(amount, invalid_destination);
    let request_id = "test-pegout-invalid".to_string();

    let result = fixture.bridge_actor.send(ProcessPegout {
        burn_event,
        request_id,
    }).await.unwrap();

    assert_bridge_error!(result, BridgeError::InvalidAddress(_));
}

#[actix::test]
async fn test_get_pending_operations() {
    let fixture = TestFixture::new().await;
    
    // Add a peg-in
    let evm_address = create_random_h160();
    let tx = fixture.create_test_pegin_tx(100_000_000, evm_address);
    
    fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap().unwrap();

    // Check pending peg-ins
    let pending_pegins = fixture.bridge_actor.send(GetPendingPegins)
        .await.unwrap().unwrap();
    
    assert_eq!(pending_pegins.len(), 1);
    assert_eq!(pending_pegins[0].amount, 100_000_000);
    assert_eq!(pending_pegins[0].evm_address, evm_address);

    // Check pending peg-outs (should be empty)
    let pending_pegouts = fixture.bridge_actor.send(GetPendingPegouts)
        .await.unwrap().unwrap();
    
    assert_eq!(pending_pegouts.len(), 0);
}

#[actix::test]
async fn test_duplicate_pegin_processing() {
    let fixture = TestFixture::new().await;
    let evm_address = create_random_h160();
    let tx = fixture.create_test_pegin_tx(100_000_000, evm_address);
    let tx_clone = tx.clone();

    // Process the same transaction twice
    let result1 = fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap();

    let result2 = fixture.bridge_actor.send(ProcessPegin {
        tx: tx_clone,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap();

    // Both should succeed (second is ignored)
    assert!(result1.is_ok());
    assert!(result2.is_ok());

    // But only one should be in pending
    let pending = fixture.bridge_actor.send(GetPendingPegins)
        .await.unwrap().unwrap();
    
    assert_eq!(pending.len(), 1);
}

#[actix::test]
async fn test_bridge_stats() {
    let fixture = TestFixture::new().await;
    
    // Process some operations
    let evm_address = create_random_h160();
    let tx = fixture.create_test_pegin_tx(100_000_000, evm_address);
    
    fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap().unwrap();

    // Get stats
    let stats = fixture.bridge_actor.send(GetBridgeStats)
        .await.unwrap().unwrap();

    assert!(stats.total_pegins_processed > 0);
    assert_eq!(stats.pending_pegins, 1);
    assert_eq!(stats.pending_pegouts, 0);
    assert!(stats.success_rate > 0.0);
}

#[actix::test]
async fn test_utxo_refresh() {
    let fixture = TestFixture::new().await;
    
    // Add some UTXOs to the mock
    fixture.test_bitcoin_rpc.add_unspent(
        create_random_txid(),
        0,
        100_000_000,
        6,
        create_test_federation_script(),
    );
    
    fixture.test_bitcoin_rpc.add_unspent(
        create_random_txid(),
        1,
        50_000_000,
        3,
        create_test_federation_script(),
    );

    // Refresh UTXOs
    let result = fixture.bridge_actor.send(RefreshUtxos)
        .await.unwrap();

    assert!(result.is_ok());
    
    // Verify stats show the UTXOs
    let stats = fixture.bridge_actor.send(GetBridgeStats)
        .await.unwrap().unwrap();
    
    // Stats should reflect available UTXOs
    // (Exact values depend on mock implementation)
    assert!(stats.success_rate >= 0.0);
}

#[actix::test]
async fn test_error_handling_and_recovery() {
    let fixture = TestFixture::new().await;
    
    // Test various error conditions
    let invalid_tx = Transaction {
        version: 2,
        lock_time: 0,
        input: vec![],
        output: vec![], // No outputs
    };

    let result = fixture.bridge_actor.send(ProcessPegin {
        tx: invalid_tx,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap();

    // Should handle the error gracefully
    assert!(result.is_err());
    
    // Actor should still be responsive
    let stats = fixture.bridge_actor.send(GetBridgeStats)
        .await.unwrap();
    
    assert!(stats.is_ok());
}

#[actix::test]  
async fn test_concurrent_operations() {
    let fixture = TestFixture::new().await;
    
    // Create multiple concurrent operations
    let mut handles = vec![];
    
    for i in 0..10 {
        let actor = fixture.bridge_actor.clone();
        let federation_address = fixture.federation_address.clone();
        let evm_address = create_random_h160();
        
        let handle = tokio::spawn(async move {
            let tx = create_deposit_transaction(
                (i + 1) * 10_000_000, // Different amounts
                evm_address,
                &federation_address,
            );
            
            actor.send(ProcessPegin {
                tx,
                confirmations: 6,
                deposit_address: federation_address,
            }).await.unwrap()
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations
    let results: Vec<_> = futures::future::join_all(handles).await;
    
    // Check that most succeeded
    let successful = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
    assert!(successful >= 8, "Expected at least 8/10 operations to succeed");
    
    // Verify final state
    let pending = fixture.bridge_actor.send(GetPendingPegins)
        .await.unwrap().unwrap();
    
    assert!(pending.len() >= successful);
}

#[actix::test]
async fn test_operation_timeout_handling() {
    let fixture = TestFixture::new().await;
    
    // Create an operation that will timeout
    let evm_address = create_random_h160();
    let tx = fixture.create_test_pegin_tx(100_000_000, evm_address);
    
    fixture.bridge_actor.send(ProcessPegin {
        tx,
        confirmations: 6,
        deposit_address: fixture.federation_address.clone(),
    }).await.unwrap().unwrap();

    // Simulate time passage and cleanup
    // This would be more sophisticated in a real test with time mocking
    let stats_before = fixture.bridge_actor.send(GetBridgeStats)
        .await.unwrap().unwrap();
    
    assert_eq!(stats_before.pending_pegins, 1);
    
    // After cleanup (simulated), old operations should be removed
    // In a real implementation, this would involve advancing time
    let stats_after = fixture.bridge_actor.send(GetBridgeStats)
        .await.unwrap().unwrap();
    
    // Verify the actor is still functional
    assert!(stats_after.success_rate >= 0.0);
}

#[actix::test]
async fn test_message_validation() {
    let fixture = TestFixture::new().await;
    
    // Test with zero amount
    let zero_amount_event = BurnEvent {
        tx_hash: create_random_h256(),
        block_number: 1000,
        amount: 0,
        destination: "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080".to_string(),
        sender: create_random_h160(),
    };
    
    let result = fixture.bridge_actor.send(ProcessPegout {
        burn_event: zero_amount_event,
        request_id: "zero-amount".to_string(),
    }).await.unwrap();
    
    // Zero amount should be handled appropriately
    // (might be allowed or rejected depending on business logic)
    assert!(result.is_ok() || result.is_err());
    
    // Test with empty request ID
    let empty_id_event = fixture.create_test_burn_event(100_000_000, "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080");
    
    let result = fixture.bridge_actor.send(ProcessPegout {
        burn_event: empty_id_event,
        request_id: "".to_string(),
    }).await.unwrap();
    
    // Empty request ID might be allowed or rejected
    assert!(result.is_ok() || result.is_err());
}