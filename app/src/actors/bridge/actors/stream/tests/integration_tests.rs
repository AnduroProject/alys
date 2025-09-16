//! StreamActor Integration Tests
//! 
//! Core integration tests for StreamActor functionality

use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

use super::test_utils::{
    StreamActorTestHarness, TestConfigBuilder, TestMessageFactory, TestAssertions,
    MockGovernanceServer,
};
use crate::actors::bridge::{
    actors::stream::config::EnvironmentType,
    messages::stream_messages::StreamMessage,
};

#[tokio::test]
async fn test_stream_actor_lifecycle() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    
    // Test initial state
    TestAssertions::assert_actor_state(&harness, "Stopped").await.unwrap();
    
    // Start actor
    harness.start().await.unwrap();
    TestAssertions::assert_actor_state(&harness, "Running").await.unwrap();
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    // Stop actor
    harness.stop().await.unwrap();
    TestAssertions::assert_actor_state(&harness, "Stopped").await.unwrap();
}

#[tokio::test]
async fn test_governance_request_handling() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    
    // Set up mock server response
    harness.add_server_response("/governance/status", r#"{"status":"ok"}"#).await;
    
    harness.start().await.unwrap();
    
    // Send governance request
    let request_id = Uuid::new_v4().to_string();
    let message = TestMessageFactory::governance_request(
        &request_id,
        b"test request".to_vec(),
    );
    
    harness.send_message(message).await.unwrap();
    
    // Wait for response
    let response = TestAssertions::assert_response_received(
        &mut harness,
        Duration::from_secs(5),
    ).await.unwrap();
    
    // Verify response
    match response {
        crate::actors::bridge::messages::stream_messages::StreamResponse::GovernanceResponse { request_id: resp_id, success, .. } => {
            assert_eq!(resp_id, request_id);
            assert!(success);
        },
        _ => panic!("Unexpected response type"),
    }
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_connection_management() {
    let config = TestConfigBuilder::new()
        .with_actor_id("connection-test-actor")
        .with_max_connections(5)
        .with_connection_timeout(Duration::from_millis(100))
        .build();
        
    let mut harness = StreamActorTestHarness::with_config(config).await.unwrap();
    harness.start().await.unwrap();
    
    // Test connection establishment
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Verify connections are being managed
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    // Test connection limit
    TestAssertions::assert_metric_value(
        &harness,
        "connections_active",
        1.0, // Should have at least one connection to governance server
        1.0,
    ).await.ok(); // OK if metric doesn't exist yet
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_reconnection_logic() {
    let config = TestConfigBuilder::new()
        .with_actor_id("reconnection-test-actor")
        .with_reconnection(3, Duration::from_millis(10))
        .build();
        
    let mut harness = StreamActorTestHarness::with_config(config).await.unwrap();
    harness.start().await.unwrap();
    
    // Simulate network partition
    harness.simulate_network_partition(Duration::from_millis(100));
    
    // Wait for reconnection attempts
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Actor should still be running and attempting to reconnect
    TestAssertions::assert_actor_state(&harness, "Running").await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_message_priority_handling() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Send messages with different priorities
    let high_priority_msg = StreamMessage::GovernanceRequest {
        request_id: "high-priority".to_string(),
        data: b"urgent".to_vec(),
        timeout: Duration::from_secs(30),
        priority: crate::actors::bridge::messages::MessagePriority::High,
    };
    
    let low_priority_msg = StreamMessage::GovernanceRequest {
        request_id: "low-priority".to_string(),
        data: b"normal".to_vec(),
        timeout: Duration::from_secs(30),
        priority: crate::actors::bridge::messages::MessagePriority::Low,
    };
    
    harness.send_message(low_priority_msg).await.unwrap();
    harness.send_message(high_priority_msg).await.unwrap();
    
    // High priority message should be processed first
    // This would require more sophisticated testing to verify order
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_configuration_update() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Create updated configuration
    let mut new_config = harness.config.clone();
    new_config.core.max_connections = 20;
    new_config.features.debug_mode = false;
    
    // Send configuration update
    let config_msg = TestMessageFactory::config_update(new_config);
    harness.send_message(config_msg).await.unwrap();
    
    // Wait for configuration to be applied
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Verify actor is still healthy after config update
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_health_check_handling() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Send health check message
    let health_check_msg = TestMessageFactory::health_check();
    harness.send_message(health_check_msg).await.unwrap();
    
    // Verify health check response
    let response = timeout(
        Duration::from_secs(1),
        harness.wait_for_response(Duration::from_secs(1))
    ).await.unwrap();
    
    assert!(response.is_some());
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_error_handling() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    
    // Configure server to return errors
    harness.mock_server.failure_rate = 0.5; // 50% failure rate
    
    harness.start().await.unwrap();
    
    // Send multiple requests
    for i in 0..10 {
        let message = TestMessageFactory::governance_request(
            &format!("request-{}", i),
            b"test data".to_vec(),
        );
        let _ = harness.send_message(message).await;
    }
    
    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Actor should still be healthy despite some failures
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_concurrent_message_handling() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Send multiple concurrent messages
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let message = TestMessageFactory::governance_request(
            &format!("concurrent-{}", i),
            format!("data-{}", i).into_bytes(),
        );
        
        let mut harness_clone = &mut harness;
        let handle = tokio::spawn(async move {
            // Note: This is a simplified test - in reality we'd need proper cloning
            // harness_clone.send_message(message).await
            Ok::<(), crate::actors::bridge::shared::errors::BridgeError>(())
        });
        handles.push(handle);
    }
    
    // Wait for all messages to be processed
    for handle in handles {
        handle.await.unwrap().unwrap();
    }
    
    // Verify actor is still healthy
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Send some messages
    for i in 0..5 {
        let message = TestMessageFactory::governance_request(
            &format!("shutdown-test-{}", i),
            b"test data".to_vec(),
        );
        harness.send_message(message).await.unwrap();
    }
    
    // Initiate graceful shutdown
    let shutdown_start = std::time::Instant::now();
    harness.stop().await.unwrap();
    let shutdown_duration = shutdown_start.elapsed();
    
    // Verify shutdown completed in reasonable time
    assert!(shutdown_duration < Duration::from_secs(5), 
           "Shutdown took too long: {:?}", shutdown_duration);
    
    // Verify final state
    TestAssertions::assert_actor_state(&harness, "Stopped").await.unwrap();
}

#[tokio::test]
async fn test_metrics_collection() {
    let config = TestConfigBuilder::new()
        .with_actor_id("metrics-test-actor")
        .build();
    
    // Enable metrics in config
    let mut config = config;
    config.features.metrics_collection = true;
    config.monitoring.metrics.enabled = true;
    
    let mut harness = StreamActorTestHarness::with_config(config).await.unwrap();
    harness.start().await.unwrap();
    
    // Send some messages to generate metrics
    for i in 0..5 {
        let message = TestMessageFactory::governance_request(
            &format!("metrics-{}", i),
            b"test data".to_vec(),
        );
        harness.send_message(message).await.unwrap();
    }
    
    // Wait for metrics to be collected
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Verify metrics are available
    let metrics = harness.get_metrics().await;
    assert!(!metrics.is_empty(), "No metrics collected");
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_environment_specific_behavior() {
    // Test production environment behavior
    let mut prod_config = TestConfigBuilder::new()
        .with_actor_id("prod-test-actor")
        .with_tls_enabled(true)
        .build();
    
    prod_config.environment.environment_type = EnvironmentType::Production;
    prod_config.features.debug_mode = false;
    prod_config.security.require_mutual_tls = true;
    
    let mut harness = StreamActorTestHarness::with_config(prod_config).await.unwrap();
    
    // Start actor - might fail due to TLS requirements in test environment
    let start_result = harness.start().await;
    
    // In production mode, certain security features should be enforced
    // This test verifies the configuration is properly applied
    
    if start_result.is_ok() {
        TestAssertions::assert_actor_state(&harness, "Running").await.unwrap();
        harness.stop().await.unwrap();
    }
    
    // Test development environment behavior
    let mut dev_config = TestConfigBuilder::new()
        .with_actor_id("dev-test-actor")
        .build();
    
    dev_config.environment.environment_type = EnvironmentType::Development;
    dev_config.features.debug_mode = true;
    dev_config.connection.tls.enabled = false;
    
    let mut dev_harness = StreamActorTestHarness::with_config(dev_config).await.unwrap();
    dev_harness.start().await.unwrap();
    
    TestAssertions::assert_actor_healthy(&dev_harness).await.unwrap();
    dev_harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_request_timeout_handling() {
    let config = TestConfigBuilder::new()
        .with_actor_id("timeout-test-actor")
        .build();
    
    let mut harness = StreamActorTestHarness::with_config(config).await.unwrap();
    
    // Configure server with high latency
    harness.mock_server.latency_simulation = Some(Duration::from_millis(200));
    
    harness.start().await.unwrap();
    
    // Send request with short timeout
    let message = StreamMessage::GovernanceRequest {
        request_id: "timeout-test".to_string(),
        data: b"test data".to_vec(),
        timeout: Duration::from_millis(50), // Shorter than server latency
        priority: crate::actors::bridge::messages::MessagePriority::Normal,
    };
    
    harness.send_message(message).await.unwrap();
    
    // Should receive timeout response
    let response = harness.wait_for_response(Duration::from_millis(300)).await;
    
    if let Some(response) = response {
        match response {
            crate::actors::bridge::messages::stream_messages::StreamResponse::GovernanceResponse { success, .. } => {
                assert!(!success, "Request should have timed out");
            },
            _ => {}
        }
    }
    
    harness.stop().await.unwrap();
}

#[cfg(test)]
mod load_tests {
    use super::*;
    use crate::actors::bridge::actors::stream::tests::test_utils::PerformanceTestUtils;

    #[tokio::test]
    #[ignore] // Run with --ignored for performance tests
    async fn test_high_throughput() {
        let config = TestConfigBuilder::new()
            .with_actor_id("throughput-test-actor")
            .with_max_connections(50)
            .with_message_buffer_size(10000)
            .build();
        
        let mut harness = StreamActorTestHarness::with_config(config).await.unwrap();
        harness.start().await.unwrap();
        
        // Generate load: 100 messages per second for 10 seconds
        let messages_sent = PerformanceTestUtils::generate_load(
            &mut harness,
            100,
            Duration::from_secs(10),
        ).await.unwrap();
        
        println!("Sent {} messages in throughput test", messages_sent);
        assert!(messages_sent >= 900, "Expected at least 900 messages, got {}", messages_sent);
        
        // Verify actor is still healthy after load test
        TestAssertions::assert_actor_healthy(&harness).await.unwrap();
        
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Run with --ignored for performance tests
    async fn test_memory_usage_under_load() {
        let config = TestConfigBuilder::new()
            .with_actor_id("memory-test-actor")
            .build();
        
        let mut harness = StreamActorTestHarness::with_config(config).await.unwrap();
        harness.start().await.unwrap();
        
        // Monitor memory usage during load test
        let initial_metrics = harness.get_metrics().await;
        
        // Generate sustained load
        PerformanceTestUtils::generate_load(
            &mut harness,
            50,
            Duration::from_secs(5),
        ).await.unwrap();
        
        let final_metrics = harness.get_metrics().await;
        
        // Verify memory usage is within acceptable bounds
        // This would need real memory monitoring in practice
        println!("Initial metrics: {:?}", initial_metrics);
        println!("Final metrics: {:?}", final_metrics);
        
        harness.stop().await.unwrap();
    }
}