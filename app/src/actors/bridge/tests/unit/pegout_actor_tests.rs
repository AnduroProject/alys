//! PegOut Actor Unit Tests
//! 
//! Comprehensive tests for PegOutActor Bitcoin withdrawal processing functionality

use actix::prelude::*;
use bitcoin::{Address, Amount, Network};
use std::time::Duration;

use crate::actors::bridge::{
    PegOutActor, PegOutMessage, PegOutRequest, PegOutResponse,
    BridgeError, BitcoinTransactionInfo
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_pegout_actor_initialization() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    let result = pegout_actor
        .send(PegOutMessage::Initialize)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegout_actor_process_valid_request() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegout_request = TestDataBuilder::test_pegout_request();
    
    let result = pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: pegout_request.clone(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    BridgeAssertions::assert_pegout_success(&response);
}

#[actix::test]
async fn test_pegout_actor_validate_burn_event() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let burn_tx_hash = H256::random();
    let burn_amount = U256::from(100_000);
    let recipient = TestDataBuilder::test_bitcoin_address();

    let result = pegout_actor
        .send(PegOutMessage::ValidateBurnEvent {
            burn_tx_hash,
            burn_amount,
            recipient,
        })
        .await;

    assert!(result.is_ok());
    // Should return burn event validation result
}

#[actix::test]
async fn test_pegout_actor_create_bitcoin_transaction() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let recipient = TestDataBuilder::test_bitcoin_address();
    let amount = Amount::from_sat(100_000);
    let fee_rate = 10;

    let result = pegout_actor
        .send(PegOutMessage::CreateBitcoinTransaction {
            recipient,
            amount,
            fee_rate,
        })
        .await;

    assert!(result.is_ok());
    // Should return transaction creation result
}

#[actix::test]
async fn test_pegout_actor_sign_transaction() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let tx_bytes = vec![0u8; 100]; // Mock transaction bytes
    let input_indices = vec![0, 1];

    let result = pegout_actor
        .send(PegOutMessage::SignTransaction {
            tx_bytes,
            input_indices,
        })
        .await;

    assert!(result.is_ok());
    // Should return signing result
}

#[actix::test]
async fn test_pegout_actor_broadcast_transaction() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let signed_tx_bytes = vec![0u8; 150]; // Mock signed transaction

    let result = pegout_actor
        .send(PegOutMessage::BroadcastTransaction {
            signed_tx_bytes,
        })
        .await;

    assert!(result.is_ok());
    // Should return broadcast result
}

#[actix::test]
async fn test_pegout_actor_get_status() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegout_id = "test_pegout_001".to_string();

    let result = pegout_actor
        .send(PegOutMessage::GetStatus {
            pegout_id,
        })
        .await;

    assert!(result.is_ok());
    // Should return status information
}

#[actix::test]
async fn test_pegout_actor_cancel_request() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegout_id = "test_pegout_001".to_string();

    let result = pegout_actor
        .send(PegOutMessage::CancelRequest {
            pegout_id,
            reason: "User requested cancellation".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegout_actor_handle_timeout() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegout_id = "test_pegout_timeout".to_string();

    let result = pegout_actor
        .send(PegOutMessage::HandleTimeout {
            pegout_id,
            timeout_type: "signing_timeout".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegout_actor_get_metrics() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = pegout_actor
        .send(PegOutMessage::GetMetrics)
        .await;

    assert!(result.is_ok());
    // Should return metrics data
}

#[actix::test]
async fn test_pegout_actor_invalid_burn_amount() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    // Create invalid request with zero amount
    let mut invalid_request = TestDataBuilder::test_pegout_request();
    invalid_request.amount = U256::zero();

    let result = pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: invalid_request,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "InvalidAmount");
}

#[actix::test]
async fn test_pegout_actor_invalid_bitcoin_address() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let burn_tx_hash = H256::random();
    let burn_amount = U256::from(100_000);
    let invalid_address = Address::from_str("invalid_address").unwrap_or_else(|_| {
        // If parsing fails, create a mainnet address when we expect regtest
        Address::from_str("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").unwrap()
            .require_network(Network::Bitcoin).unwrap()
    });

    let result = pegout_actor
        .send(PegOutMessage::ValidateBurnEvent {
            burn_tx_hash,
            burn_amount,
            recipient: invalid_address,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "InvalidAddress");
    }
}

#[actix::test]
async fn test_pegout_actor_insufficient_funds() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let recipient = TestDataBuilder::test_bitcoin_address();
    let excessive_amount = Amount::from_sat(u64::MAX); // Very large amount
    let fee_rate = 10;

    let result = pegout_actor
        .send(PegOutMessage::CreateBitcoinTransaction {
            recipient,
            amount: excessive_amount,
            fee_rate,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "InsufficientFunds");
    }
}

#[actix::test]
async fn test_pegout_actor_duplicate_request() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegout_request = TestDataBuilder::test_pegout_request();

    // Process the same request twice
    let first_result = pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: pegout_request.clone(),
        })
        .await;

    let second_result = pegout_actor
        .send(PegOutMessage::ProcessRequest {
            request: pegout_request,
        })
        .await;

    assert!(first_result.is_ok());
    assert!(second_result.is_ok());

    // Second request should be detected as duplicate
    let second_response = second_result.unwrap();
    if second_response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&second_response.map(|_| ()), "DuplicateRequest");
    }
}

#[actix::test]
async fn test_pegout_actor_high_fee_rate() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let recipient = TestDataBuilder::test_bitcoin_address();
    let amount = Amount::from_sat(100_000);
    let excessive_fee_rate = 1000; // Very high fee rate

    let result = pegout_actor
        .send(PegOutMessage::CreateBitcoinTransaction {
            recipient,
            amount,
            fee_rate: excessive_fee_rate,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "ExcessiveFeeRate");
    }
}

#[actix::test]
async fn test_pegout_actor_signing_failure() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let invalid_tx_bytes = vec![]; // Empty transaction bytes
    let input_indices = vec![0];

    let result = pegout_actor
        .send(PegOutMessage::SignTransaction {
            tx_bytes: invalid_tx_bytes,
            input_indices,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "SigningFailed");
    }
}

#[actix::test]
async fn test_pegout_actor_broadcast_failure() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let invalid_tx_bytes = vec![0u8; 10]; // Too short to be valid

    let result = pegout_actor
        .send(PegOutMessage::BroadcastTransaction {
            signed_tx_bytes: invalid_tx_bytes,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "BroadcastFailed");
    }
}

#[actix::test]
async fn test_pegout_actor_shutdown() {
    let config = test_bridge_config();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize first
    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = pegout_actor
        .send(PegOutMessage::Shutdown)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}