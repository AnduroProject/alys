//! Phase 3 Task 3.4: Stress Testing
//!
//! High-load and performance tests:
//! - 1000 rapid gossip messages
//! - 100 concurrent block requests
//! - Peer churn (rapid connect/disconnect)
//! - Memory and channel backpressure handling

use actix::Actor;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::actors_v2::network::{
    NetworkActor, NetworkConfig, NetworkMessage, NetworkResponse,
};

/// Helper to create test actor
fn create_test_actor(port: u16) -> NetworkActor {
    let config = NetworkConfig {
        listen_addresses: vec![format!("/ip4/127.0.0.1/tcp/{}", port)],
        bootstrap_peers: vec![],
        max_connections: 1000,
        connection_timeout: Duration::from_secs(10),
        gossip_topics: vec!["test/blocks".to_string(), "test/transactions".to_string()],
        message_size_limit: 10 * 1024 * 1024, // 10MB for stress tests
        discovery_interval: Duration::from_secs(30),
        auto_dial_mdns_peers: false,
    };

    NetworkActor::new(config).expect("Failed to create NetworkActor")
}

/// Test 3.4.1: 1000 Rapid Gossip Messages
///
/// Verifies system can handle high-volume gossip message broadcasting.
#[actix::test]
async fn test_1000_rapid_gossip_messages() {
    let actor = create_test_actor(20001).start();

    // Start network
    actor.send(NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/20001".to_string()],
        bootstrap_peers: vec![],
    }).await.expect("Failed to start").expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    let start_time = Instant::now();
    let message_count = 1000;
    let mut success_count = 0;
    let mut error_count = 0;

    println!("Sending {} rapid gossip messages...", message_count);

    for i in 0..message_count {
        let block_data = format!("stress test block {}", i).into_bytes();
        let result = actor.send(NetworkMessage::BroadcastBlock {
            block_data,
            priority: i % 10 == 0, // Every 10th message is priority
        }).await;

        match result {
            Ok(Ok(_)) => success_count += 1,
            Ok(Err(e)) => {
                error_count += 1;
                if error_count < 10 {
                    println!("Message {} error: {:?}", i, e);
                }
            }
            Err(e) => {
                error_count += 1;
                println!("Mailbox error for message {}: {}", i, e);
            }
        }

        // Small delay to avoid completely overwhelming the system
        if i % 100 == 0 {
            tokio::task::yield_now().await;
        }
    }

    let elapsed = start_time.elapsed();
    let messages_per_second = message_count as f64 / elapsed.as_secs_f64();

    println!("Stress test completed:");
    println!("  Total messages: {}", message_count);
    println!("  Successful: {}", success_count);
    println!("  Errors: {}", error_count);
    println!("  Duration: {:?}", elapsed);
    println!("  Messages/second: {:.2}", messages_per_second);

    // Verify metrics
    let metrics = actor.send(NetworkMessage::GetMetrics).await
        .expect("Failed to get metrics")
        .expect("Metrics failed");

    match metrics {
        NetworkResponse::Metrics(m) => {
            println!("Final metrics:");
            println!("  Gossip published: {}", m.gossip_messages_published);
            println!("  Messages sent: {}", m.messages_sent);
            println!("  Bytes sent: {}", m.bytes_sent);
            assert!(m.gossip_messages_published > 0, "Should have published messages");
        }
        _ => panic!("Unexpected metrics response"),
    }

    // Cleanup
    actor.send(NetworkMessage::StopNetwork { graceful: true }).await.ok();
    sleep(Duration::from_millis(500)).await;
}

/// Test 3.4.2: 100 Concurrent Block Requests
///
/// Verifies system can handle many simultaneous block requests.
#[actix::test]
async fn test_100_concurrent_block_requests() {
    let actor = create_test_actor(20002).start();

    // Start network
    actor.send(NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/20002".to_string()],
        bootstrap_peers: vec![],
    }).await.expect("Failed to start").expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    let start_time = Instant::now();
    let request_count = 100;
    let mut success_count = 0;
    let mut error_count = 0;

    println!("Sending {} concurrent block requests...", request_count);

    // Send all requests concurrently
    let mut handles = vec![];

    for i in 0..request_count {
        let actor_clone = actor.clone();
        let handle = tokio::spawn(async move {
            actor_clone.send(NetworkMessage::RequestBlocks {
                start_height: i * 10,
                count: 10,
                correlation_id: None,
            }).await
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(Ok(Ok(_))) => success_count += 1,
            Ok(Ok(Err(e))) => {
                error_count += 1;
                if error_count < 10 {
                    println!("Request {} error: {:?}", i, e);
                }
            }
            Ok(Err(e)) => {
                error_count += 1;
                println!("Mailbox error for request {}: {}", i, e);
            }
            Err(e) => {
                error_count += 1;
                println!("Join error for request {}: {}", i, e);
            }
        }
    }

    let elapsed = start_time.elapsed();
    let requests_per_second = request_count as f64 / elapsed.as_secs_f64();

    println!("Concurrent request test completed:");
    println!("  Total requests: {}", request_count);
    println!("  Successful: {}", success_count);
    println!("  Errors: {} (expected - no peers)", error_count);
    println!("  Duration: {:?}", elapsed);
    println!("  Requests/second: {:.2}", requests_per_second);

    // Verify metrics
    let metrics = actor.send(NetworkMessage::GetMetrics).await
        .expect("Failed to get metrics")
        .expect("Metrics failed");

    match metrics {
        NetworkResponse::Metrics(m) => {
            println!("Final metrics:");
            println!("  Block requests sent: {}", m.block_requests_sent);
            println!("  Block response errors: {}", m.block_response_errors);
            // Most requests should fail due to no peers, which is expected
        }
        _ => panic!("Unexpected metrics response"),
    }

    // Cleanup
    actor.send(NetworkMessage::StopNetwork { graceful: true }).await.ok();
    sleep(Duration::from_millis(500)).await;
}

/// Test 3.4.3: Rapid Peer Churn
///
/// Verifies system handles rapid connect/disconnect cycles.
#[actix::test]
async fn test_rapid_peer_churn() {
    let main_actor = create_test_actor(20003).start();

    // Start main actor
    main_actor.send(NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/20003".to_string()],
        bootstrap_peers: vec![],
    }).await.expect("Failed to start").expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    let churn_cycles = 20;
    let peers_per_cycle = 3;

    println!("Starting peer churn test: {} cycles, {} peers per cycle", churn_cycles, peers_per_cycle);

    for cycle in 0..churn_cycles {
        // Start multiple peer actors
        let mut peer_actors = vec![];
        for peer_idx in 0..peers_per_cycle {
            let port = 20100 + (cycle * peers_per_cycle) + peer_idx;
            let peer = create_test_actor(port as u16).start();

            peer.send(NetworkMessage::StartNetwork {
                listen_addrs: vec![format!("/ip4/127.0.0.1/tcp/{}", port)],
                bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/20003".to_string()],
            }).await.ok();

            peer_actors.push(peer);
        }

        // Let connections establish
        sleep(Duration::from_millis(200)).await;

        // Disconnect all peers
        for peer in peer_actors {
            peer.send(NetworkMessage::StopNetwork { graceful: false }).await.ok();
        }

        // Brief pause between cycles
        sleep(Duration::from_millis(100)).await;

        if cycle % 5 == 0 {
            println!("Completed {} churn cycles...", cycle + 1);
        }
    }

    println!("Peer churn test completed");

    // Verify main actor is still operational
    let status = main_actor.send(NetworkMessage::GetNetworkStatus).await
        .expect("Failed to get status")
        .expect("Status failed");

    match status {
        NetworkResponse::Status(s) => {
            assert!(s.is_running, "Main actor should still be running after churn");
            println!("Main actor status: running={}, peers={}", s.is_running, s.connected_peers);
        }
        _ => panic!("Unexpected status response"),
    }

    // Check metrics
    let metrics = main_actor.send(NetworkMessage::GetMetrics).await
        .expect("Failed to get metrics")
        .expect("Metrics failed");

    match metrics {
        NetworkResponse::Metrics(m) => {
            println!("Churn metrics:");
            println!("  Total connections: {}", m.total_connections);
            println!("  Failed connections: {}", m.failed_connections);
            println!("  Connection errors: {}", m.connection_errors);
        }
        _ => panic!("Unexpected metrics response"),
    }

    // Cleanup
    main_actor.send(NetworkMessage::StopNetwork { graceful: true }).await.ok();
    sleep(Duration::from_millis(500)).await;
}

/// Test 3.4.4: Mixed High-Load Scenario
///
/// Verifies system handles multiple high-load operations simultaneously.
#[actix::test]
async fn test_mixed_high_load() {
    let actor = create_test_actor(20004).start();

    // Start network
    actor.send(NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/20004".to_string()],
        bootstrap_peers: vec![],
    }).await.expect("Failed to start").expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    let start_time = Instant::now();

    println!("Starting mixed high-load test...");

    // Spawn concurrent tasks for different operations
    let actor1 = actor.clone();
    let broadcast_handle = tokio::spawn(async move {
        for i in 0..200 {
            actor1.send(NetworkMessage::BroadcastBlock {
                block_data: format!("mixed load block {}", i).into_bytes(),
                priority: false,
            }).await.ok();
            if i % 50 == 0 {
                tokio::task::yield_now().await;
            }
        }
    });

    let actor2 = actor.clone();
    let transaction_handle = tokio::spawn(async move {
        for i in 0..200 {
            actor2.send(NetworkMessage::BroadcastTransaction {
                tx_data: format!("mixed load tx {}", i).into_bytes(),
            }).await.ok();
            if i % 50 == 0 {
                tokio::task::yield_now().await;
            }
        }
    });

    let actor3 = actor.clone();
    let request_handle = tokio::spawn(async move {
        for i in 0..50 {
            actor3.send(NetworkMessage::RequestBlocks {
                start_height: i * 20,
                count: 20,
                correlation_id: None,
            }).await.ok();
            if i % 10 == 0 {
                tokio::task::yield_now().await;
            }
        }
    });

    let actor4 = actor.clone();
    let status_handle = tokio::spawn(async move {
        for _ in 0..20 {
            actor4.send(NetworkMessage::GetNetworkStatus).await.ok();
            sleep(Duration::from_millis(50)).await;
        }
    });

    // Wait for all tasks to complete
    broadcast_handle.await.ok();
    transaction_handle.await.ok();
    request_handle.await.ok();
    status_handle.await.ok();

    let elapsed = start_time.elapsed();

    println!("Mixed high-load test completed in {:?}", elapsed);

    // Verify system is still responsive
    let final_status = actor.send(NetworkMessage::GetNetworkStatus).await
        .expect("Failed to get status")
        .expect("Status failed");

    match final_status {
        NetworkResponse::Status(s) => {
            assert!(s.is_running, "Actor should still be running after mixed load");
            println!("Final status: running={}", s.is_running);
        }
        _ => panic!("Unexpected status response"),
    }

    // Check final metrics
    let metrics = actor.send(NetworkMessage::GetMetrics).await
        .expect("Failed to get metrics")
        .expect("Metrics failed");

    match metrics {
        NetworkResponse::Metrics(m) => {
            println!("Mixed load metrics:");
            println!("  Gossip published: {}", m.gossip_messages_published);
            println!("  Block requests: {}", m.block_requests_sent);
            println!("  Messages sent: {}", m.messages_sent);
        }
        _ => panic!("Unexpected metrics response"),
    }

    // Cleanup
    actor.send(NetworkMessage::StopNetwork { graceful: true }).await.ok();
    sleep(Duration::from_millis(500)).await;
}

/// Test 3.4.5: Channel Backpressure Handling
///
/// Verifies system handles channel backpressure gracefully.
#[actix::test]
async fn test_channel_backpressure() {
    let actor = create_test_actor(20005).start();

    // Start network
    actor.send(NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/20005".to_string()],
        bootstrap_peers: vec![],
    }).await.expect("Failed to start").expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    println!("Testing channel backpressure with burst of messages...");

    // Send a very large burst of messages rapidly
    let burst_size = 2000;
    let mut sent_count = 0;

    for i in 0..burst_size {
        let result = actor.send(NetworkMessage::BroadcastBlock {
            block_data: vec![0u8; 1024], // 1KB blocks
            priority: false,
        }).await;

        if result.is_ok() {
            sent_count += 1;
        }

        // No yield - maximum pressure
    }

    println!("Sent {} messages in burst", sent_count);

    // Give system time to process queue
    sleep(Duration::from_secs(2)).await;

    // Verify system is still responsive
    let status = actor.send(NetworkMessage::GetNetworkStatus).await
        .expect("Failed to get status")
        .expect("Status failed");

    match status {
        NetworkResponse::Status(s) => {
            assert!(s.is_running, "Actor should handle backpressure gracefully");
            println!("System still operational after backpressure test");
        }
        _ => panic!("Unexpected status response"),
    }

    // Cleanup
    actor.send(NetworkMessage::StopNetwork { graceful: true }).await.ok();
    sleep(Duration::from_millis(500)).await;
}

/// Test 3.4.6: Long-Running Stability
///
/// Verifies system remains stable during extended operation.
#[actix::test]
async fn test_long_running_stability() {
    let actor = create_test_actor(20006).start();

    // Start network
    actor.send(NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/20006".to_string()],
        bootstrap_peers: vec![],
    }).await.expect("Failed to start").expect("Start failed");

    sleep(Duration::from_secs(1)).await;

    let test_duration = Duration::from_secs(10); // 10 seconds of continuous operation
    let start_time = Instant::now();
    let mut operation_count = 0;

    println!("Starting long-running stability test ({:?})...", test_duration);

    while start_time.elapsed() < test_duration {
        // Continuously perform various operations
        actor.send(NetworkMessage::BroadcastBlock {
            block_data: b"stability test block".to_vec(),
            priority: false,
        }).await.ok();

        actor.send(NetworkMessage::GetNetworkStatus).await.ok();

        operation_count += 2;

        sleep(Duration::from_millis(10)).await;
    }

    println!("Completed {} operations over {:?}", operation_count, test_duration);

    // Verify system is still healthy
    let health = actor.send(NetworkMessage::HealthCheck {
        correlation_id: Some(uuid::Uuid::new_v4()),
    }).await.expect("Failed health check").expect("Health check error");

    match health {
        NetworkResponse::Healthy { is_healthy, connected_peers: _, issues } => {
            assert!(is_healthy, "System should be healthy after long run");
            println!("System healthy after stability test");
            if !issues.is_empty() {
                println!("Issues reported: {:?}", issues);
            }
        }
        _ => panic!("Unexpected health response"),
    }

    // Cleanup
    actor.send(NetworkMessage::StopNetwork { graceful: true }).await.ok();
    sleep(Duration::from_millis(500)).await;
}
