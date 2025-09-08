//! Error Handling Integration Tests
//! 
//! Testing error scenarios and recovery mechanisms

use actix::prelude::*;
use std::time::Duration;

use crate::actors::bridge::{
    BridgeError, ActorType, BridgeCoordinationMessage
};
use crate::actors::bridge::tests::helpers::*;

#[actix::test]
async fn test_basic_error_handling() {
    let config = test_bridge_config();
    
    // Test that our test configuration can be created
    assert_eq!(config.bitcoin_network, bitcoin::Network::Regtest);
    assert_eq!(config.federation_size, 3);
    assert_eq!(config.min_confirmations, 6);
}

#[actix::test]
async fn test_bridge_error_creation() {
    let timeout_error = BridgeError::actor_timeout(ActorType::PegIn, Duration::from_secs(30));
    assert!(timeout_error.to_string().contains("PegIn"));

    let comm_error = BridgeError::actor_communication("Test message".to_string());
    assert!(comm_error.to_string().contains("Test message"));

    let recovery_error = BridgeError::system_recovery("TestComponent".to_string(), "Test issue".to_string());
    assert!(recovery_error.to_string().contains("TestComponent"));
}

#[actix::test]
async fn test_mock_data_creation() {
    let pegin_request = TestDataBuilder::test_pegin_request();
    assert!(pegin_request.amount.as_sat() > 0);
    assert_eq!(pegin_request.output_index, 0);
    assert_eq!(pegin_request.confirmation_count, 6);

    let pegout_request = TestDataBuilder::test_pegout_request();
    assert!(!pegout_request.amount.is_zero());
    assert_eq!(pegout_request.fee_rate, 10);
}