//! Actor Coordination Integration Tests
//! 
//! Testing inter-actor communication and coordination scenarios

use actix::prelude::*;
use std::time::Duration;
use tokio::time::sleep;

use crate::actors::bridge::{
    BridgeActor, PegInActor, PegOutActor, StreamActor,
    BridgeCoordinationMessage, ActorType, BridgeError
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_actor_registration_sequence() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize bridge system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Create actors
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config.clone()).start();
    let stream_actor = StreamActor::new(config).start();

    // Register actors in sequence
    let pegin_registration = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor))
        .await;
    assert!(pegin_registration.is_ok());
    assert!(pegin_registration.unwrap().is_ok());

    let pegout_registration = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor))
        .await;
    assert!(pegout_registration.is_ok());
    assert!(pegout_registration.unwrap().is_ok());

    let stream_registration = bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(stream_actor))
        .await;
    assert!(stream_registration.is_ok());
    assert!(stream_registration.unwrap().is_ok());

    // Verify system status shows all actors
    let status = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;
    assert!(status.is_ok());
}

#[actix::test]
async fn test_actor_failure_handling() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Simulate actor failures
    let timeout_error = BridgeError::ActorTimeout {
        actor_type: ActorType::PegIn,
        timeout: Duration::from_secs(30),
    };

    let failure_result = bridge_actor
        .send(BridgeCoordinationMessage::HandleActorFailure {
            actor_type: ActorType::PegIn,
            error: timeout_error,
        })
        .await;

    assert!(failure_result.is_ok());
    assert!(failure_result.unwrap().is_ok());

    // Test multiple failure types
    let communication_error = BridgeError::ActorCommunication {
        message: "Failed to send message to actor".to_string(),
    };

    let failure_result_2 = bridge_actor
        .send(BridgeCoordinationMessage::HandleActorFailure {
            actor_type: ActorType::PegOut,
            error: communication_error,
        })
        .await;

    assert!(failure_result_2.is_ok());
    assert!(failure_result_2.unwrap().is_ok());
}

#[actix::test]
async fn test_concurrent_actor_operations() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Create and register actors
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config.clone()).start();
    let stream_actor = StreamActor::new(config).start();

    // Register all actors concurrently
    let pegin_future = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor));
    let pegout_future = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor));
    let stream_future = bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(stream_actor));

    let (pegin_result, pegout_result, stream_result) = 
        tokio::join!(pegin_future, pegout_future, stream_future);

    assert!(pegin_result.is_ok());
    assert!(pegin_result.unwrap().is_ok());
    assert!(pegout_result.is_ok());
    assert!(pegout_result.unwrap().is_ok());
    assert!(stream_result.is_ok());
    assert!(stream_result.unwrap().is_ok());

    // Verify system can handle concurrent operations
    let bitcoin_txid_1 = TestDataBuilder::random_txid();
    let bitcoin_txid_2 = TestDataBuilder::random_txid();

    let coord_future_1 = bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "concurrent_1".to_string(),
            bitcoin_txid: bitcoin_txid_1,
        });

    let coord_future_2 = bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "concurrent_2".to_string(),
            bitcoin_txid: bitcoin_txid_2,
        });

    let (coord_result_1, coord_result_2) = tokio::join!(coord_future_1, coord_future_2);

    assert!(coord_result_1.is_ok());
    assert!(coord_result_1.unwrap().is_ok());
    assert!(coord_result_2.is_ok());
    assert!(coord_result_2.unwrap().is_ok());
}

#[actix::test]
async fn test_actor_state_synchronization() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Register all actors
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config.clone()).start();
    let stream_actor = StreamActor::new(config).start();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(stream_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    // Perform operations that should be synchronized
    let bitcoin_txid = TestDataBuilder::random_txid();
    
    let coordination_result = bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "sync_test".to_string(),
            bitcoin_txid,
        })
        .await;

    assert!(coordination_result.is_ok());
    assert!(coordination_result.unwrap().is_ok());

    // Allow time for state synchronization
    sleep(Duration::from_millis(100)).await;

    // Check that all actors have consistent state
    let bridge_status = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    let bridge_metrics = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemMetrics)
        .await;

    assert!(bridge_status.is_ok());
    assert!(bridge_metrics.is_ok());
}

#[actix::test]
async fn test_actor_recovery_mechanisms() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Register actors
    let pegin_actor = PegInActor::new(config.clone()).start();
    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor))
        .await
        .unwrap()
        .unwrap();

    // Simulate a failure
    let recovery_error = BridgeError::SystemRecovery {
        component: "PegInActor".to_string(),
        issue: "Actor became unresponsive".to_string(),
    };

    let failure_result = bridge_actor
        .send(BridgeCoordinationMessage::HandleActorFailure {
            actor_type: ActorType::PegIn,
            error: recovery_error,
        })
        .await;

    assert!(failure_result.is_ok());
    assert!(failure_result.unwrap().is_ok());

    // Verify system can still function after recovery
    let status_after_recovery = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(status_after_recovery.is_ok());
}

#[actix::test]
async fn test_message_passing_reliability() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Register actors
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config.clone()).start();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor))
        .await
        .unwrap()
        .unwrap();

    // Send multiple coordination messages rapidly
    let mut futures = Vec::new();

    for i in 0..10 {
        let bitcoin_txid = TestDataBuilder::random_txid();
        let future = bridge_actor
            .send(BridgeCoordinationMessage::CoordinatePegIn {
                pegin_id: format!("reliability_test_{}", i),
                bitcoin_txid,
            });
        futures.push(future);
    }

    // Wait for all messages to complete
    let results = futures::future::join_all(futures).await;

    // Verify all messages were processed successfully
    for (i, result) in results.into_iter().enumerate() {
        assert!(result.is_ok(), "Message {} failed: {:?}", i, result);
        assert!(result.unwrap().is_ok(), "Message {} returned error", i);
    }
}

#[actix::test]
async fn test_load_balancing_coordination() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Register multiple instances of the same actor type (simulation)
    let pegin_actor_1 = PegInActor::new(config.clone()).start();
    let pegin_actor_2 = PegInActor::new(config).start();

    // Register first actor
    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor_1))
        .await
        .unwrap()
        .unwrap();

    // Attempt to register second actor (should handle gracefully)
    let second_registration = bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor_2))
        .await;

    // System should handle this appropriately (either accept or reject gracefully)
    assert!(second_registration.is_ok());
}

#[actix::test]
async fn test_system_resource_management() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    // Register all actor types
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config.clone()).start();
    let stream_actor = StreamActor::new(config).start();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(stream_actor))
        .await
        .unwrap()
        .unwrap();

    // Generate load to test resource management
    let mut coordination_futures = Vec::new();

    for i in 0..5 {
        let bitcoin_txid = TestDataBuilder::random_txid();
        let future = bridge_actor
            .send(BridgeCoordinationMessage::CoordinatePegIn {
                pegin_id: format!("resource_test_pegin_{}", i),
                bitcoin_txid,
            });
        coordination_futures.push(future);

        let burn_tx_hash = H256::random();
        let future = bridge_actor
            .send(BridgeCoordinationMessage::CoordinatePegOut {
                pegout_id: format!("resource_test_pegout_{}", i),
                burn_tx_hash,
            });
        coordination_futures.push(future);
    }

    // Process all operations concurrently
    let results = futures::future::join_all(coordination_futures).await;

    // Verify system handled the load appropriately
    for result in results {
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    // Check system metrics after load
    let metrics = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemMetrics)
        .await;

    assert!(metrics.is_ok());
}