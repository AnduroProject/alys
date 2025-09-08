//! Chaos Engineering Tests for Bridge System
//! 
//! Testing system resilience under various failure conditions

use actix::prelude::*;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

use crate::actors::bridge::{
    BridgeActor, PegInActor, PegOutActor, StreamActor,
    BridgeCoordinationMessage, PegInMessage, PegOutMessage, StreamMessage,
    BridgeError, ActorType
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_random_actor_failures() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

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

    let mut rng = rand::thread_rng();
    let test_duration = Duration::from_secs(3);
    let start_time = std::time::Instant::now();
    let mut operations_attempted = 0;
    let mut failures_injected = 0;

    while start_time.elapsed() < test_duration {
        // Randomly choose between normal operation and failure injection
        if rng.gen_bool(0.2) { // 20% chance of failure injection
            let actor_types = [ActorType::PegIn, ActorType::PegOut, ActorType::Stream];
            let random_actor = actor_types[rng.gen_range(0..actor_types.len())];
            
            let error = match rng.gen_range(0..3) {
                0 => BridgeError::ActorTimeout {
                    actor_type: random_actor,
                    timeout: Duration::from_secs(30),
                },
                1 => BridgeError::ActorCommunication {
                    message: "Simulated communication failure".to_string(),
                },
                _ => BridgeError::SystemRecovery {
                    component: format!("{:?}Actor", random_actor),
                    issue: "Chaos engineering failure injection".to_string(),
                },
            };

            let _ = bridge_actor
                .send(BridgeCoordinationMessage::HandleActorFailure {
                    actor_type: random_actor,
                    error,
                })
                .await;

            failures_injected += 1;
        } else {
            // Normal operation
            if rng.gen_bool(0.6) { // 60% peg-in operations
                let pegin_request = TestDataBuilder::test_pegin_request();
                let _ = pegin_actor
                    .send(PegInMessage::ProcessRequest {
                        request: pegin_request,
                    })
                    .await;
            } else { // 40% peg-out operations
                let pegout_request = TestDataBuilder::test_pegout_request();
                let _ = pegout_actor
                    .send(PegOutMessage::ProcessRequest {
                        request: pegout_request,
                    })
                    .await;
            }
        }

        operations_attempted += 1;
        sleep(Duration::from_millis(rng.gen_range(10..100))).await;
    }

    // Verify system is still responsive after chaos
    let final_status = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(final_status.is_ok(), "System unresponsive after chaos testing");

    println!("Chaos test completed:");
    println!("  Operations attempted: {}", operations_attempted);
    println!("  Failures injected: {}", failures_injected);
    println!("  System remained responsive: {}", final_status.is_ok());
}

#[actix::test]
async fn test_network_partition_simulation() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let stream_actor = StreamActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterStreamActor(stream_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    // Establish connections
    let connection_result = stream_actor
        .send(StreamMessage::EstablishConnection {
            peer_id: "partition_test_peer".to_string(),
            endpoint: "ws://localhost:9944".to_string(),
        })
        .await;

    assert!(connection_result.is_ok());

    // Simulate network partition by forcing disconnection
    let disconnect_result = stream_actor
        .send(StreamMessage::DisconnectPeer {
            peer_id: "partition_test_peer".to_string(),
            reason: "Network partition simulation".to_string(),
        })
        .await;

    assert!(disconnect_result.is_ok());

    // Simulate connection errors during partition
    let connection_error_result = stream_actor
        .send(StreamMessage::HandleConnectionError {
            peer_id: "partition_test_peer".to_string(),
            error: "Network unreachable".to_string(),
        })
        .await;

    assert!(connection_error_result.is_ok());

    // Verify system attempts recovery
    let status_result = stream_actor
        .send(StreamMessage::GetConnectionStatus)
        .await;

    assert!(status_result.is_ok());

    // Simulate network recovery
    let reconnection_result = stream_actor
        .send(StreamMessage::EstablishConnection {
            peer_id: "partition_test_peer".to_string(),
            endpoint: "ws://localhost:9944".to_string(),
        })
        .await;

    // System should handle reconnection gracefully
    assert!(reconnection_result.is_ok());
}

#[actix::test]
async fn test_resource_exhaustion_scenarios() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

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

    // Simulate resource exhaustion by overwhelming the system
    let overwhelming_load_count = 200;
    let mut futures = Vec::new();

    for i in 0..overwhelming_load_count {
        if i % 2 == 0 {
            let pegin_request = PegInRequest {
                bitcoin_txid: TestDataBuilder::random_txid(),
                output_index: i % 10,
                amount: bitcoin::Amount::from_sat(1000 + i * 100),
                recipient: TestDataBuilder::test_ethereum_address(),
                confirmation_count: 6,
            };

            let future = pegin_actor
                .send(PegInMessage::ProcessRequest {
                    request: pegin_request,
                });
            futures.push(future);
        } else {
            let pegout_request = PegOutRequest {
                burn_tx_hash: H256::random(),
                amount: U256::from(1000 + i * 100),
                recipient: TestDataBuilder::test_bitcoin_address(),
                fee_rate: 10 + (i % 50),
            };

            let future = pegout_actor
                .send(PegOutMessage::ProcessRequest {
                    request: pegout_request,
                });
            futures.push(future);
        }
    }

    // Execute all requests simultaneously to create resource pressure
    let results = futures::future::join_all(futures).await;

    // Analyze how the system handled resource exhaustion
    let successful_operations = results.iter()
        .filter(|r| r.is_ok() && !r.as_ref().unwrap().is_err())
        .count();

    let failed_operations = overwhelming_load_count - successful_operations;
    let failure_rate = failed_operations as f64 / overwhelming_load_count as f64;

    println!("Resource exhaustion test results:");
    println!("  Total operations: {}", overwhelming_load_count);
    println!("  Successful operations: {}", successful_operations);
    println!("  Failed operations: {}", failed_operations);
    println!("  Failure rate: {:.2}%", failure_rate * 100.0);

    // System should gracefully handle overload (some failures expected)
    assert!(failure_rate < 0.9, "Excessive failure rate under load");

    // System should remain responsive
    let post_load_status = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(post_load_status.is_ok(), "System unresponsive after resource exhaustion");
}

#[actix::test]
async fn test_cascading_failure_scenarios() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config.clone()).start();
    let stream_actor = StreamActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

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

    // Trigger a cascade of failures
    let failure_sequence = vec![
        (ActorType::Stream, BridgeError::ActorCommunication {
            message: "Stream actor communication failed".to_string(),
        }),
        (ActorType::PegIn, BridgeError::ActorTimeout {
            actor_type: ActorType::PegIn,
            timeout: Duration::from_secs(1),
        }),
        (ActorType::PegOut, BridgeError::SystemRecovery {
            component: "PegOutActor".to_string(),
            issue: "Cascading failure from other actors".to_string(),
        }),
    ];

    // Inject failures in sequence with short delays
    for (actor_type, error) in failure_sequence {
        let failure_result = bridge_actor
            .send(BridgeCoordinationMessage::HandleActorFailure {
                actor_type,
                error,
            })
            .await;

        assert!(failure_result.is_ok(), "Failed to handle actor failure");
        
        // Short delay to allow failure propagation
        sleep(Duration::from_millis(100)).await;
    }

    // Verify system can recover from cascading failures
    let recovery_status = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(recovery_status.is_ok(), "System failed to recover from cascading failures");

    // Test that system can still coordinate operations after recovery
    let recovery_coordination = bridge_actor
        .send(BridgeCoordinationMessage::CoordinatePegIn {
            pegin_id: "post_cascade_test".to_string(),
            bitcoin_txid: TestDataBuilder::random_txid(),
        })
        .await;

    assert!(recovery_coordination.is_ok(), "System coordination failed after cascade recovery");
}

#[actix::test]
async fn test_data_corruption_resilience() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    // Test system resilience to corrupted/invalid data
    let corrupted_requests = vec![
        // Invalid Bitcoin transaction ID
        PegInRequest {
            bitcoin_txid: bitcoin::Txid::from_byte_array([0u8; 32]),
            output_index: 0,
            amount: bitcoin::Amount::from_sat(100_000),
            recipient: TestDataBuilder::test_ethereum_address(),
            confirmation_count: 6,
        },
        // Zero amount
        PegInRequest {
            bitcoin_txid: TestDataBuilder::random_txid(),
            output_index: 0,
            amount: bitcoin::Amount::from_sat(0),
            recipient: TestDataBuilder::test_ethereum_address(),
            confirmation_count: 6,
        },
        // Invalid output index
        PegInRequest {
            bitcoin_txid: TestDataBuilder::random_txid(),
            output_index: u32::MAX,
            amount: bitcoin::Amount::from_sat(100_000),
            recipient: TestDataBuilder::test_ethereum_address(),
            confirmation_count: 6,
        },
    ];

    for corrupted_request in corrupted_requests {
        let result = pegin_actor
            .send(PegInMessage::ProcessRequest {
                request: corrupted_request,
            })
            .await;

        // System should handle corrupted data gracefully
        assert!(result.is_ok(), "System crashed on corrupted data");
        
        // The operation should fail, but the actor should remain responsive
        if let Ok(response) = result {
            assert!(response.is_err(), "Corrupted data was processed successfully");
        }
    }

    // Verify system is still operational after corruption attacks
    let normal_request = TestDataBuilder::test_pegin_request();
    let normal_result = pegin_actor
        .send(PegInMessage::ProcessRequest {
            request: normal_request,
        })
        .await;

    assert!(normal_result.is_ok(), "System failed to process normal request after corruption");
}

#[actix::test] 
async fn test_timing_attack_resilience() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    // Test rapid-fire requests to check for timing vulnerabilities
    let rapid_requests_count = 50;
    let mut futures = Vec::new();

    let start_time = std::time::Instant::now();

    for _ in 0..rapid_requests_count {
        let pegin_request = TestDataBuilder::test_pegin_request();
        let future = pegin_actor
            .send(PegInMessage::ProcessRequest {
                request: pegin_request,
            });
        futures.push(future);
    }

    let results = futures::future::join_all(futures).await;
    let elapsed = start_time.elapsed();

    // System should handle rapid requests without crashing
    let successful_responses = results.iter()
        .filter(|r| r.is_ok())
        .count();

    println!("Timing attack resilience test:");
    println!("  Rapid requests: {}", rapid_requests_count);
    println!("  Successful responses: {}", successful_responses);
    println!("  Time elapsed: {:?}", elapsed);

    assert!(successful_responses > 0, "No successful responses to rapid requests");
    
    // System should remain responsive after rapid requests
    let post_attack_status = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemStatus)
        .await;

    assert!(post_attack_status.is_ok(), "System unresponsive after timing attack");
}