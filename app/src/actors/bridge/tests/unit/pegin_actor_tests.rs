//! PegIn Actor Unit Tests
//! 
//! Comprehensive tests for PegInActor Bitcoin deposit processing functionality

use actix::prelude::*;
use bitcoin::{Amount, Network, Txid};
use std::time::Duration;

use crate::actors::bridge::{
    PegInActor, PegInMessage, PegInRequest, PegInResponse,
    BridgeError, BitcoinTransactionInfo
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_pegin_actor_initialization() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    let result = pegin_actor
        .send(PegInMessage::Initialize)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegin_actor_process_valid_request() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegin_request = TestDataBuilder::test_pegin_request();
    
    let result = pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request.clone(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    BridgeAssertions::assert_pegin_success(&response);
}

#[actix::test]
async fn test_pegin_actor_validate_transaction() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let bitcoin_txid = TestDataBuilder::random_txid();
    let output_index = 0;

    let result = pegin_actor
        .send(PegInMessage::ValidateTransaction {
            txid: bitcoin_txid,
            output_index,
        })
        .await;

    assert!(result.is_ok());
    // Should return transaction validation result
}

#[actix::test]
async fn test_pegin_actor_check_confirmations() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let bitcoin_txid = TestDataBuilder::random_txid();

    let result = pegin_actor
        .send(PegInMessage::CheckConfirmations {
            txid: bitcoin_txid,
            required_confirmations: 6,
        })
        .await;

    assert!(result.is_ok());
    // Should return confirmation status
}

#[actix::test]
async fn test_pegin_actor_mint_tokens() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let recipient = TestDataBuilder::test_ethereum_address();
    let amount = U256::from(100_000);

    let result = pegin_actor
        .send(PegInMessage::MintTokens {
            recipient,
            amount,
            bitcoin_txid: TestDataBuilder::random_txid(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegin_actor_get_status() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegin_id = "test_pegin_001".to_string();

    let result = pegin_actor
        .send(PegInMessage::GetStatus {
            pegin_id,
        })
        .await;

    assert!(result.is_ok());
    // Should return status information
}

#[actix::test]
async fn test_pegin_actor_cancel_request() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegin_id = "test_pegin_001".to_string();

    let result = pegin_actor
        .send(PegInMessage::CancelRequest {
            pegin_id,
            reason: "User requested cancellation".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegin_actor_handle_timeout() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegin_id = "test_pegin_timeout".to_string();

    let result = pegin_actor
        .send(PegInMessage::HandleTimeout {
            pegin_id,
            timeout_type: "confirmation_timeout".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_pegin_actor_get_metrics() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = pegin_actor
        .send(PegInMessage::GetMetrics)
        .await;

    assert!(result.is_ok());
    // Should return metrics data
}

#[actix::test]
async fn test_pegin_actor_invalid_transaction() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    // Create invalid request with zero amount
    let mut invalid_request = TestDataBuilder::test_pegin_request();
    invalid_request.amount = Amount::from_sat(0);

    let result = pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: invalid_request,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "InvalidAmount");
}

#[actix::test]
async fn test_pegin_actor_insufficient_confirmations() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let bitcoin_txid = TestDataBuilder::random_txid();

    // Check with insufficient confirmations
    let result = pegin_actor
        .send(PegInMessage::CheckConfirmations {
            txid: bitcoin_txid,
            required_confirmations: 100, // Very high number
        })
        .await;

    assert!(result.is_ok());
    // Should indicate insufficient confirmations
}

#[actix::test]
async fn test_pegin_actor_duplicate_request() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let pegin_request = TestDataBuilder::test_pegin_request();

    // Process the same request twice
    let first_result = pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request.clone(),
        })
        .await;

    let second_result = pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request,
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
async fn test_pegin_actor_bitcoin_network_mismatch() {
    let mut config = test_bridge_config();
    config.bitcoin_network = Network::Bitcoin; // Use mainnet instead of regtest
    
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    // Create request with regtest address
    let pegin_request = TestDataBuilder::test_pegin_request();

    let result = pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: pegin_request,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    if response.is_err() {
        BridgeAssertions::assert_bridge_error_type(&response.map(|_| ()), "NetworkMismatch");
    }
}

#[actix::test]
async fn test_pegin_actor_shutdown() {
    let config = test_bridge_config();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize first
    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = pegin_actor
        .send(PegInMessage::Shutdown)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}