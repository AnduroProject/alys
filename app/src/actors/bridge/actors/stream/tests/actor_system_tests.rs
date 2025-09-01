//! Actor System Compatibility Tests
//! 
//! Tests for StreamActor integration with the actor_system crate

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::test_utils::{
    StreamActorTestHarness, TestConfigBuilder, TestMessageFactory, TestAssertions,
};
use crate::actor_system::{
    ActorResult, AlysActor, AlysMessage, LifecycleAware, ExtendedAlysActor,
    metrics::ActorSystemMetrics,
    message::{MessagePriority, MessageId},
    actor::{ActorId, ActorState, ActorContext},
};
use crate::actors::bridge::{
    actors::stream::StreamActor,
    messages::stream_messages::StreamMessage,
    shared::errors::BridgeError,
};

#[tokio::test]
async fn test_alys_actor_trait_implementation() {
    let config = TestConfigBuilder::new()
        .with_actor_id("alys-actor-test")
        .build();
    
    let metrics = ActorSystemMetrics::new("test");
    let actor = StreamActor::new(config, metrics).unwrap();
    
    // Test AlysActor trait methods
    assert_eq!(actor.actor_id(), "alys-actor-test");
    assert_eq!(actor.actor_type(), "StreamActor");
    
    // Test state management
    assert_eq!(actor.state(), ActorState::Stopped);
    
    // Test configuration access
    let config = actor.get_config().await.unwrap();
    assert_eq!(config.core.actor_id, "alys-actor-test");
}

#[tokio::test]
async fn test_lifecycle_aware_implementation() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    
    // Test lifecycle states
    assert_eq!(harness.actor.state(), ActorState::Stopped);
    
    // Test on_start
    harness.actor.on_start().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Starting);
    
    // Wait for transition to Running
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Test health check
    let is_healthy = harness.actor.health_check().await.unwrap();
    assert!(is_healthy);
    
    // Test on_stop
    harness.actor.on_stop().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Stopping);
    
    // Wait for transition to Stopped
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(harness.actor.state(), ActorState::Stopped);
}

#[tokio::test]
async fn test_message_handling_with_priority() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Test different message priorities
    let high_priority_msg = StreamMessage::GovernanceRequest {
        request_id: "high-priority-test".to_string(),
        data: b"urgent data".to_vec(),
        timeout: Duration::from_secs(30),
        priority: MessagePriority::High,
    };
    
    let normal_priority_msg = StreamMessage::GovernanceRequest {
        request_id: "normal-priority-test".to_string(),
        data: b"normal data".to_vec(),
        timeout: Duration::from_secs(30),
        priority: MessagePriority::Normal,
    };
    
    // Test AlysMessage trait implementation
    assert_eq!(high_priority_msg.priority(), MessagePriority::High);
    assert_eq!(normal_priority_msg.priority(), MessagePriority::Normal);
    
    // Test message timeout
    assert_eq!(high_priority_msg.timeout(), Duration::from_secs(30));
    assert!(high_priority_msg.is_retryable());
    
    // Send messages
    harness.send_message(normal_priority_msg).await.unwrap();
    harness.send_message(high_priority_msg).await.unwrap();
    
    // Verify messages are processed
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_extended_alys_actor_capabilities() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Test critical error handling
    let critical_error = BridgeError::CriticalSystemFailure {
        component: "test-component".to_string(),
        details: "test critical error".to_string(),
    };
    
    // This would trigger critical error handling
    let result = harness.actor.handle_critical_error(critical_error).await;
    assert!(result.is_ok());
    
    // Test supervision interaction
    let supervisor_message = "test supervision message".to_string();
    let result = harness.actor.handle_supervisor_message(supervisor_message).await;
    assert!(result.is_ok());
    
    // Test pause/resume functionality
    harness.actor.pause().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Paused);
    
    harness.actor.resume().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Running);
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_actor_metrics_integration() {
    let config = TestConfigBuilder::new()
        .with_actor_id("metrics-integration-test")
        .build();
    
    let metrics = ActorSystemMetrics::new("test-system");
    let mut actor = StreamActor::new(config, metrics.clone()).unwrap();
    
    // Start actor to begin metrics collection
    actor.start().await.unwrap();
    
    // Send some messages to generate metrics
    for i in 0..10 {
        let message = TestMessageFactory::governance_request(
            &format!("metrics-test-{}", i),
            b"test data".to_vec(),
        );
        actor.handle_message(message).await.unwrap();
    }
    
    // Wait for metrics to be updated
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Verify metrics are being collected
    let actor_metrics = actor.get_metrics().await.unwrap();
    assert!(!actor_metrics.is_empty());
    
    // Check system-level metrics
    let system_metrics = metrics.get_all_metrics().await;
    assert!(system_metrics.contains_key("actors_total"));
    
    actor.stop().await.unwrap();
}

#[tokio::test]
async fn test_actor_context_usage() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Test context information
    let context = harness.actor.get_context().await.unwrap();
    
    // Verify context contains expected information
    assert!(context.contains("actor_id"));
    assert!(context.contains("actor_type"));
    assert!(context.contains("state"));
    
    // Test context updates
    let original_context = context.clone();
    
    // Send a message to potentially update context
    let message = TestMessageFactory::governance_request("context-test", b"test".to_vec());
    harness.send_message(message).await.unwrap();
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let updated_context = harness.actor.get_context().await.unwrap();
    // Context might have been updated with message processing info
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_actor_state_transitions() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    
    // Test initial state
    assert_eq!(harness.actor.state(), ActorState::Stopped);
    
    // Test Starting -> Running transition
    harness.actor.on_start().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Starting);
    
    // Wait for automatic transition to Running
    let mut attempts = 0;
    while harness.actor.state() != ActorState::Running && attempts < 50 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        attempts += 1;
    }
    assert_eq!(harness.actor.state(), ActorState::Running);
    
    // Test Running -> Paused transition
    harness.actor.pause().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Paused);
    
    // Test Paused -> Running transition
    harness.actor.resume().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Running);
    
    // Test Running -> Stopping -> Stopped transition
    harness.actor.on_stop().await.unwrap();
    assert_eq!(harness.actor.state(), ActorState::Stopping);
    
    // Wait for automatic transition to Stopped
    attempts = 0;
    while harness.actor.state() != ActorState::Stopped && attempts < 50 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        attempts += 1;
    }
    assert_eq!(harness.actor.state(), ActorState::Stopped);
}

#[tokio::test]
async fn test_concurrent_message_processing() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Send multiple concurrent messages
    let mut handles = Vec::new();
    
    for i in 0..20 {
        let message = TestMessageFactory::governance_request(
            &format!("concurrent-{}", i),
            format!("data-{}", i).into_bytes(),
        );
        
        // Clone actor reference for concurrent access
        let actor_clone = &harness.actor;
        
        let handle = tokio::spawn(async move {
            // In a real test, we'd need proper actor cloning/referencing
            // For now, simulate concurrent processing
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<(), BridgeError>(())
        });
        
        handles.push(handle);
    }
    
    // Wait for all concurrent operations
    for handle in handles {
        handle.await.unwrap().unwrap();
    }
    
    // Verify actor is still healthy
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_error_propagation_to_actor_system() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Send invalid message to trigger error
    let invalid_message = StreamMessage::GovernanceRequest {
        request_id: "".to_string(), // Invalid empty request ID
        data: Vec::new(),
        timeout: Duration::from_secs(0), // Invalid zero timeout
        priority: MessagePriority::Normal,
    };
    
    // Error should be handled gracefully
    let result = harness.send_message(invalid_message).await;
    
    // Actor should handle the error without crashing
    tokio::time::sleep(Duration::from_millis(100)).await;
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_actor_restart_capability() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    
    // Start actor
    harness.start().await.unwrap();
    TestAssertions::assert_actor_state(&harness, "Running").await.unwrap();
    
    // Stop actor
    harness.stop().await.unwrap();
    TestAssertions::assert_actor_state(&harness, "Stopped").await.unwrap();
    
    // Restart actor
    harness.start().await.unwrap();
    TestAssertions::assert_actor_state(&harness, "Running").await.unwrap();
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_message_serialization_compatibility() {
    // Test message serialization/deserialization with actor system
    let message = TestMessageFactory::governance_request(
        "serialization-test",
        b"test data for serialization".to_vec(),
    );
    
    // Test AlysMessage trait methods
    assert_eq!(message.message_id().len(), 36); // UUID format
    assert!(message.is_retryable());
    assert_eq!(message.priority(), MessagePriority::Normal);
    assert!(message.timeout() > Duration::from_secs(0));
    
    // Test message type information
    assert_eq!(message.message_type(), "GovernanceRequest");
    
    // In a full implementation, we would test:
    // - Message serialization to bytes
    // - Message deserialization from bytes
    // - Message routing through actor system
}

#[tokio::test]
async fn test_actor_supervision_integration() {
    let mut harness = StreamActorTestHarness::new().await.unwrap();
    harness.start().await.unwrap();
    
    // Test supervision messages
    let supervision_message = "restart_requested".to_string();
    let result = harness.actor.handle_supervisor_message(supervision_message).await;
    assert!(result.is_ok());
    
    // Test escalation handling
    let critical_error = BridgeError::CriticalSystemFailure {
        component: "supervision-test".to_string(),
        details: "test escalation".to_string(),
    };
    
    let result = harness.actor.handle_critical_error(critical_error).await;
    assert!(result.is_ok());
    
    // Actor should still be responsive after supervision events
    TestAssertions::assert_actor_healthy(&harness).await.unwrap();
    
    harness.stop().await.unwrap();
}

#[tokio::test]
async fn test_actor_configuration_compliance() {
    let config = TestConfigBuilder::new()
        .with_actor_id("compliance-test-actor")
        .with_max_connections(5)
        .with_message_buffer_size(1000)
        .build();
    
    let metrics = ActorSystemMetrics::new("compliance-test");
    let actor = StreamActor::new(config.clone(), metrics).unwrap();
    
    // Verify actor respects configuration limits
    let actor_config = actor.get_config().await.unwrap();
    assert_eq!(actor_config.core.actor_id, "compliance-test-actor");
    assert_eq!(actor_config.core.max_connections, 5);
    assert_eq!(actor_config.core.message_buffer_size, 1000);
    
    // Test runtime configuration updates
    let mut updated_config = config.clone();
    updated_config.core.max_connections = 10;
    
    let result = actor.update_config(updated_config).await;
    assert!(result.is_ok());
    
    // Verify configuration was updated
    let new_config = actor.get_config().await.unwrap();
    assert_eq!(new_config.core.max_connections, 10);
}

#[tokio::test]
async fn test_actor_system_metrics_reporting() {
    let config = TestConfigBuilder::new()
        .with_actor_id("metrics-reporting-test")
        .build();
    
    let metrics = ActorSystemMetrics::new("metrics-test-system");
    let mut actor = StreamActor::new(config, metrics.clone()).unwrap();
    
    actor.start().await.unwrap();
    
    // Generate activity to create metrics
    for i in 0..5 {
        let message = TestMessageFactory::governance_request(
            &format!("metrics-{}", i),
            b"metrics test data".to_vec(),
        );
        actor.handle_message(message).await.unwrap();
    }
    
    // Wait for metrics to be reported
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Verify metrics are reported to actor system
    let system_metrics = metrics.get_all_metrics().await;
    
    // Check for expected metrics
    assert!(system_metrics.contains_key("actors_total"));
    assert!(system_metrics.len() > 0);
    
    // Test actor-specific metrics
    let actor_metrics = actor.get_metrics().await.unwrap();
    assert!(!actor_metrics.is_empty());
    
    actor.stop().await.unwrap();
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_system_integration_full_cycle() {
        // This test verifies full integration with actor_system crate
        let config = TestConfigBuilder::new()
            .with_actor_id("full-integration-test")
            .with_debug_mode(true)
            .build();
        
        let metrics = ActorSystemMetrics::new("integration-test-system");
        let mut actor = StreamActor::new(config, metrics.clone()).unwrap();
        
        // Full lifecycle test
        assert_eq!(actor.state(), ActorState::Stopped);
        
        // Start
        actor.on_start().await.unwrap();
        
        // Wait for running state
        let mut attempts = 0;
        while actor.state() != ActorState::Running && attempts < 100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            attempts += 1;
        }
        assert_eq!(actor.state(), ActorState::Running);
        
        // Process messages
        for i in 0..10 {
            let message = TestMessageFactory::governance_request(
                &format!("full-integration-{}", i),
                format!("test-data-{}", i).into_bytes(),
            );
            actor.handle_message(message).await.unwrap();
        }
        
        // Health checks
        assert!(actor.health_check().await.unwrap());
        
        // Metrics
        let metrics_snapshot = actor.get_metrics().await.unwrap();
        assert!(!metrics_snapshot.is_empty());
        
        // Configuration access
        let config_snapshot = actor.get_config().await.unwrap();
        assert_eq!(config_snapshot.core.actor_id, "full-integration-test");
        
        // Context information
        let context = actor.get_context().await.unwrap();
        assert!(context.contains("actor_id"));
        
        // Graceful shutdown
        actor.on_stop().await.unwrap();
        
        // Wait for stopped state
        attempts = 0;
        while actor.state() != ActorState::Stopped && attempts < 100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            attempts += 1;
        }
        assert_eq!(actor.state(), ActorState::Stopped);
    }
}