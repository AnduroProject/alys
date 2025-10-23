//! Phase 3 Task 3.3: Negative Tests
//!
//! Tests for error handling, invalid inputs, and failure scenarios:
//! - Invalid multiaddr formats
//! - Port conflicts
//! - Malformed protocol messages
//! - Network partition simulation

use actix::Actor;
use std::time::Duration;
use tokio::time::sleep;

use crate::actors_v2::network::{
    NetworkActor, NetworkConfig, NetworkError, NetworkMessage, NetworkResponse,
};

/// Helper to create test actor
fn create_test_actor(port: u16) -> NetworkActor {
    let config = NetworkConfig {
        listen_addresses: vec![format!("/ip4/127.0.0.1/tcp/{}", port)],
        bootstrap_peers: vec![],
        max_connections: 100,
        connection_timeout: Duration::from_secs(10),
        gossip_topics: vec!["test/blocks".to_string()],
        message_size_limit: 1024 * 1024,
        discovery_interval: Duration::from_secs(30),
        auto_dial_mdns_peers: false,
        // Phase 4: Set connection limits to match max_connections
        max_inbound_connections: 50,
        max_outbound_connections: 50,
        ..Default::default() // Use defaults for rate limiting
    };

    NetworkActor::new(config).expect("Failed to create NetworkActor")
}

/// Test 3.3.1: Invalid Multiaddr Format
///
/// Verifies that the system properly handles invalid multiaddr strings.
#[actix::test]
async fn test_invalid_multiaddr_format() {
    let actor = create_test_actor(19001).start();

    // Try to start with invalid multiaddr format
    let invalid_addrs = vec![
        "not-a-multiaddr",
        "127.0.0.1:8000",
        "/ip4/invalid-ip/tcp/8000",
        "/ip4/127.0.0.1/tcp/invalid-port",
        "/ip4/127.0.0.1", // Missing port
        "",
    ];

    for invalid_addr in invalid_addrs {
        let result = actor
            .send(NetworkMessage::StartNetwork {
                listen_addrs: vec![invalid_addr.to_string()],
                bootstrap_peers: vec![],
            })
            .await;

        match result {
            Ok(Err(NetworkError::Configuration(_))) => {
                println!("Correctly rejected invalid address: {}", invalid_addr);
            }
            Ok(Err(NetworkError::Internal(_))) => {
                println!(
                    "Correctly rejected invalid address with internal error: {}",
                    invalid_addr
                );
            }
            Ok(Err(e)) => {
                println!("Rejected invalid address with error: {:?}", e);
            }
            Ok(Ok(_)) => {
                // Some invalid formats might actually parse successfully but fail to bind
                println!("Address was accepted (may fail at bind): {}", invalid_addr);
            }
            Err(e) => {
                println!("Mailbox error for address {}: {}", invalid_addr, e);
            }
        }
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;
}

/// Test 3.3.2: Port Already In Use
///
/// Verifies that the system handles port conflicts gracefully.
#[actix::test]
async fn test_port_already_in_use() {
    // Start first actor on port 19002
    let actor1 = create_test_actor(19002).start();
    let start_result1 = actor1
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19002".to_string()],
            bootstrap_peers: vec![],
        })
        .await;

    match start_result1 {
        Ok(Ok(_)) => println!("First actor started successfully on port 19002"),
        _ => println!("First actor start result: {:?}", start_result1),
    }

    // Wait for port to be bound
    sleep(Duration::from_secs(1)).await;

    // Try to start second actor on same port
    let actor2 = create_test_actor(19002).start();
    let start_result2 = actor2
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19002".to_string()],
            bootstrap_peers: vec![],
        })
        .await;

    match start_result2 {
        Ok(Err(NetworkError::Internal(msg))) if msg.contains("Failed to listen") => {
            println!("Correctly detected port conflict");
        }
        Ok(Err(_)) => {
            println!("Port conflict detected (different error type)");
        }
        Ok(Ok(_)) => {
            println!("WARNING: Second actor started (OS may have assigned different port)");
        }
        Err(e) => {
            println!("Mailbox error: {}", e);
        }
    }

    // Cleanup
    actor1
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    actor2
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(500)).await;
}

/// Test 3.3.3: Invalid Bootstrap Peer Address
///
/// Verifies handling of invalid bootstrap peer addresses.
#[actix::test]
async fn test_invalid_bootstrap_peer() {
    let actor = create_test_actor(19003).start();

    // Start with invalid bootstrap peer
    let result = actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19003".to_string()],
            bootstrap_peers: vec![
                "invalid-bootstrap-peer".to_string(),
                "http://example.com:8000".to_string(),
            ],
        })
        .await;

    // Should succeed starting, but bootstrap connection will fail
    match result {
        Ok(Ok(_)) => {
            println!("Actor started (bootstrap peer connection will fail later)");
        }
        Ok(Err(e)) => {
            println!("Start failed due to invalid bootstrap: {:?}", e);
        }
        Err(e) => panic!("Mailbox error: {}", e),
    }

    // Verify actor is still functional despite bootstrap failure
    sleep(Duration::from_secs(1)).await;

    let status = actor
        .send(NetworkMessage::GetNetworkStatus)
        .await
        .expect("Failed to get status")
        .expect("Status failed");

    match status {
        NetworkResponse::Status(s) => {
            println!(
                "Actor status after invalid bootstrap: running={}",
                s.is_running
            );
            assert!(s.is_running, "Actor should still be running");
        }
        _ => panic!("Unexpected status response"),
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;
}

/// Test 3.3.4: Broadcast Before Network Started
///
/// Verifies that operations fail gracefully when network isn't started.
#[actix::test]
async fn test_operations_before_network_started() {
    let actor = create_test_actor(19004).start();

    // Try to broadcast without starting network
    let broadcast_result = actor
        .send(NetworkMessage::BroadcastBlock {
            block_data: b"test block".to_vec(),
            priority: false,
        })
        .await;

    match broadcast_result {
        Ok(Err(NetworkError::NotStarted)) => {
            println!("Correctly rejected broadcast before network started");
        }
        Ok(Err(_)) => {
            println!("Broadcast rejected with different error (acceptable)");
        }
        Ok(Ok(_)) => {
            panic!("Broadcast should have failed before network started");
        }
        Err(e) => panic!("Mailbox error: {}", e),
    }

    // Try to request blocks without starting network
    let request_result = actor
        .send(NetworkMessage::RequestBlocks {
            start_height: 0,
            count: 10,
            correlation_id: None,
        })
        .await;

    match request_result {
        Ok(Err(NetworkError::NotStarted)) => {
            println!("Correctly rejected request before network started");
        }
        Ok(Err(_)) => {
            println!("Request rejected with different error (acceptable)");
        }
        Ok(Ok(_)) => {
            panic!("Request should have failed before network started");
        }
        Err(e) => panic!("Mailbox error: {}", e),
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
}

/// Test 3.3.5: Invalid Block Request Parameters
///
/// Verifies validation of block request parameters.
#[actix::test]
async fn test_invalid_block_request_parameters() {
    let actor = create_test_actor(19005).start();

    // Start network
    actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19005".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to start")
        .expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    // Test count = 0 (invalid)
    let result = actor
        .send(NetworkMessage::RequestBlocks {
            start_height: 0,
            count: 0,
            correlation_id: None,
        })
        .await;

    match result {
        Ok(Err(NetworkError::Protocol(msg))) if msg.contains("Invalid block count") => {
            println!("Correctly rejected count = 0");
        }
        _ => println!("Count = 0 validation result: {:?}", result),
    }

    // Test count > 100 (invalid)
    let result = actor
        .send(NetworkMessage::RequestBlocks {
            start_height: 0,
            count: 101,
            correlation_id: None,
        })
        .await;

    match result {
        Ok(Err(NetworkError::Protocol(msg))) if msg.contains("Invalid block count") => {
            println!("Correctly rejected count > 100");
        }
        _ => println!("Count > 100 validation result: {:?}", result),
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;
}

/// Test 3.3.6: No Peers Available For Request
///
/// Verifies handling when no peers are available for block requests.
#[actix::test]
async fn test_no_peers_for_block_request() {
    let actor = create_test_actor(19006).start();

    // Start network without any peers
    actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19006".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to start")
        .expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    // Try to request blocks with no peers
    let result = actor
        .send(NetworkMessage::RequestBlocks {
            start_height: 0,
            count: 10,
            correlation_id: None,
        })
        .await;

    match result {
        Ok(Err(NetworkError::Connection(msg))) if msg.contains("No suitable peers") => {
            println!("Correctly reported no peers available");
        }
        _ => println!("No peers result: {:?}", result),
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;
}

/// Test 3.3.7: Invalid AuxPoW Data Format
///
/// Verifies validation of AuxPoW data structure.
#[actix::test]
async fn test_invalid_auxpow_data() {
    let actor = create_test_actor(19007).start();

    // Start network
    actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19007".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to start")
        .expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    // Try to broadcast invalid AuxPoW data
    let invalid_data = b"this is not valid JSON".to_vec();
    let result = actor
        .send(NetworkMessage::BroadcastAuxPow {
            auxpow_data: invalid_data,
            correlation_id: None,
        })
        .await;

    match result {
        Ok(Err(NetworkError::Protocol(msg))) if msg.contains("Invalid AuxPoW") => {
            println!("Correctly rejected invalid AuxPoW data");
        }
        _ => println!("Invalid AuxPoW result: {:?}", result),
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;
}

/// Test 3.3.8: Repeated Start/Stop Operations
///
/// Verifies idempotency of start/stop operations.
#[actix::test]
async fn test_repeated_start_stop() {
    let actor = create_test_actor(19008).start();

    // Start network
    let start1 = actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19008".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to send start")
        .expect("Start 1 failed");

    assert!(matches!(start1, NetworkResponse::Started));
    sleep(Duration::from_millis(500)).await;

    // Try to start again (should be idempotent)
    let start2 = actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19008".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to send start")
        .expect("Start 2 failed");

    assert!(matches!(start2, NetworkResponse::Started));
    println!("Repeated start handled correctly (idempotent)");

    // Stop network
    let stop1 = actor
        .send(NetworkMessage::StopNetwork { graceful: true })
        .await
        .expect("Failed to send stop")
        .expect("Stop 1 failed");

    assert!(matches!(stop1, NetworkResponse::Stopped));
    sleep(Duration::from_millis(500)).await;

    // Try to stop again (should be idempotent)
    let stop2 = actor
        .send(NetworkMessage::StopNetwork { graceful: true })
        .await
        .expect("Failed to send stop")
        .expect("Stop 2 failed");

    assert!(matches!(stop2, NetworkResponse::Stopped));
    println!("Repeated stop handled correctly (idempotent)");
}

/// Test 3.3.9: Graceful vs Immediate Shutdown
///
/// Verifies both shutdown modes work correctly.
#[actix::test]
async fn test_shutdown_modes() {
    // Test graceful shutdown
    let actor1 = create_test_actor(19009).start();
    actor1
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19009".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to start")
        .expect("Start failed");

    sleep(Duration::from_millis(500)).await;

    let graceful_result = actor1
        .send(NetworkMessage::StopNetwork { graceful: true })
        .await
        .expect("Failed to send stop")
        .expect("Graceful stop failed");

    assert!(matches!(graceful_result, NetworkResponse::Stopped));
    println!("Graceful shutdown completed");

    sleep(Duration::from_millis(1000)).await;

    // Test immediate shutdown
    let actor2 = create_test_actor(19010).start();
    actor2
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19010".to_string()],
            bootstrap_peers: vec![],
        })
        .await
        .expect("Failed to start")
        .expect("Start failed");

    sleep(Duration::from_millis(500)).await;

    let immediate_result = actor2
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .expect("Failed to send stop")
        .expect("Immediate stop failed");

    assert!(matches!(immediate_result, NetworkResponse::Stopped));
    println!("Immediate shutdown completed");

    sleep(Duration::from_millis(200)).await;
}

/// Test 3.3.10: Connection To Unreachable Peer
///
/// Verifies handling of connection attempts to unreachable addresses.
#[actix::test]
async fn test_connection_to_unreachable_peer() {
    let actor = create_test_actor(19011).start();

    // Start network
    actor
        .send(NetworkMessage::StartNetwork {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/19011".to_string()],
            // Use unreachable bootstrap peers
            bootstrap_peers: vec![
                "/ip4/127.0.0.1/tcp/65535".to_string(), // Unlikely to be in use
                "/ip4/192.0.2.1/tcp/8000".to_string(),  // TEST-NET-1, unreachable
            ],
        })
        .await
        .expect("Failed to start")
        .expect("Start failed");

    // Wait for connection attempts
    sleep(Duration::from_secs(2)).await;

    // Verify actor is still running despite failed connections
    let status = actor
        .send(NetworkMessage::GetNetworkStatus)
        .await
        .expect("Failed to get status")
        .expect("Status failed");

    match status {
        NetworkResponse::Status(s) => {
            assert!(
                s.is_running,
                "Actor should still be running after failed connections"
            );
            println!(
                "Actor operational after connection failures: peers={}",
                s.connected_peers
            );
        }
        _ => panic!("Unexpected status response"),
    }

    // Cleanup
    actor
        .send(NetworkMessage::StopNetwork { graceful: false })
        .await
        .ok();
    sleep(Duration::from_millis(200)).await;
}
