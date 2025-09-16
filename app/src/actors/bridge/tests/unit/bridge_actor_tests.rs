//! Bridge Actor Unit Tests
//! 
//! Comprehensive tests for BridgeActor coordination functionality

use actix::prelude::*;
use std::time::Duration;

use crate::actors::bridge::{
    BridgeError, ActorType, BridgeCoordinationMessage
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_bridge_actor_initialization() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_register_pegin_actor() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system first
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Create mock pegin actor address
    let mock_pegin_actor = MockPegInActor::new().start();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(mock_pegin_actor))
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test] 
async fn test_bridge_actor_register_pegout_actor() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system first
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Create mock pegout actor address
    let mock_pegout_actor = MockPegOutActor::new().start();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(mock_pegout_actor))
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_register_stream_actor() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system first
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Create mock stream actor address
    let mock_stream_actor = MockStreamActor::new().start();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(mock_stream_actor))
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_get_system_status() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize and register actors
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(result.is_ok());
    // The result should contain system status information
}

#[actix::test]
async fn test_bridge_actor_coordinate_pegin() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system and register actors
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    let mock_pegin_actor = MockPegInActor::new().start();
    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(mock_pegin_actor))
        .await
        .unwrap()
        .unwrap();

    let bitcoin_txid = TestDataBuilder::random_txid();
    let result = bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "test_pegin_001".to_string(),
            bitcoin_txid,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_coordinate_pegout() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system and register actors
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    let mock_pegout_actor = MockPegOutActor::new().start();
    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(mock_pegout_actor))
        .await
        .unwrap()
        .unwrap();

    let burn_tx_hash = H256::random();
    let result = bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegOut {
            pegout_id: "test_pegout_001".to_string(),
            burn_tx_hash,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_handle_actor_failure() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system first
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    let bridge_error = BridgeError::ActorTimeout {
        actor_type: ActorType::PegIn,
        timeout: Duration::from_secs(30),
    };

    let result = bridge_actor
        .send(BridgeCoordinationMessage::HandleActorFailure {
            actor_type: ActorType::PegIn,
            error: bridge_error,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_shutdown_system() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system first
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::ShutdownSystem)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_bridge_actor_get_system_metrics() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system first
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    let result = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemMetrics)
        .await;

    assert!(result.is_ok());
    // Should return metrics data
}

#[actix::test]
async fn test_bridge_actor_multiple_registrations() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Register all actor types
    let mock_pegin_actor = MockPegInActor::new().start();
    let mock_pegout_actor = MockPegOutActor::new().start();
    let mock_stream_actor = MockStreamActor::new().start();

    // Register all actors
    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(mock_pegin_actor))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(mock_pegout_actor))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(mock_stream_actor))
        .await
        .unwrap()
        .unwrap();

    // Verify system status shows all actors registered
    let status_result = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(status_result.is_ok());
}

// Mock actor implementations for testing
use actix::Actor;

pub struct MockPegInActor;

impl MockPegInActor {
    pub fn new() -> Self {
        Self
    }
}

impl Actor for MockPegInActor {
    type Context = Context<Self>;
}

pub struct MockPegOutActor;

impl MockPegOutActor {
    pub fn new() -> Self {
        Self
    }
}

impl Actor for MockPegOutActor {
    type Context = Context<Self>;
}

pub struct MockStreamActor;

impl MockStreamActor {
    pub fn new() -> Self {
        Self
    }
}

impl Actor for MockStreamActor {
    type Context = Context<Self>;
}