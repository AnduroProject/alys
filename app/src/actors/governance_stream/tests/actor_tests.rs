//! Unit tests for StreamActor core functionality
//!
//! This module tests the core StreamActor functionality including lifecycle
//! management, message handling, and state transitions using TDD principles.

use crate::actors::governance_stream::{
    actor::*, config::StreamConfig, error::*, messages::*, types::*
};
use crate::testing::{ActorTestHarness, TestEnvironment, TestUtil};
use actix::prelude::*;
use std::time::Duration;
use tokio_test;
use uuid::Uuid;

/// Test fixture for StreamActor tests
pub struct StreamActorTestFixture {
    pub config: StreamConfig,
    pub test_env: TestEnvironment,
    pub harness: ActorTestHarness,
}

impl StreamActorTestFixture {
    /// Create new test fixture with default configuration
    pub async fn new() -> Self {
        let mut config = StreamConfig::default();
        config.connection.governance_endpoints = vec![
            crate::actors::governance_stream::config::GovernanceEndpoint {
                url: "http://localhost:50051".to_string(),
                priority: 100,
                enabled: true,
                expected_latency_ms: Some(10),
                region: Some("test".to_string()),
                auth_override: None,
                metadata: std::collections::HashMap::new(),
            }
        ];

        let test_env = TestEnvironment::new();
        let harness = ActorTestHarness::new().await;

        Self { config, test_env, harness }
    }

    /// Create test fixture with custom configuration
    pub async fn with_config(config: StreamConfig) -> Self {
        let test_env = TestEnvironment::new();
        let harness = ActorTestHarness::new().await;
        Self { config, test_env, harness }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_creation_and_initialization() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        
        // Act
        let result = StreamActor::new(fixture.config.clone());
        
        // Assert
        assert!(result.is_ok());
        let actor = result.unwrap();
        assert_eq!(actor.state.lifecycle, ActorLifecycle::Initializing);
    }

    #[tokio::test]
    async fn test_actor_startup_lifecycle() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        
        // Act
        let addr = actor.start();
        
        // Give the actor time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Assert - actor should be running
        assert!(addr.connected());
    }

    #[tokio::test]
    async fn test_establish_connection_message() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        let addr = actor.start();

        let message = EstablishConnection {
            endpoint: "http://localhost:50051".to_string(),
            auth_token: Some("test_token".to_string()),
            chain_id: "alys-test".to_string(),
            priority: ConnectionPriority::High,
        };

        // Act
        let result = addr.send(message).await;

        // Assert
        // Note: In a real test, we would set up a mock gRPC server
        // For now, we expect it to fail gracefully
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_connection_status_no_connections() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        let addr = actor.start();

        let message = GetConnectionStatus {
            connection_id: None,
        };

        // Act
        let result = addr.send(message).await;

        // Assert
        assert!(result.is_ok());
        let status = result.unwrap().unwrap();
        assert!(!status.connected);
        assert_eq!(status.active_connections, 0);
    }

    #[tokio::test]
    async fn test_signature_request_message() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        let addr = actor.start();

        let request_id = Uuid::new_v4().to_string();
        let message = RequestSignatures {
            request_id: request_id.clone(),
            tx_hex: "0x1234567890abcdef".to_string(),
            input_indices: vec![0, 1],
            amounts: vec![100000000, 200000000],
            tx_type: TransactionType::Pegout,
            timeout: Some(Duration::from_secs(30)),
            priority: RequestPriority::High,
        };

        // Act
        let result = addr.send(message).await;

        // Assert
        // Should return error because no connections are established
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_heartbeat_message() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        let addr = actor.start();

        let message = SendHeartbeat {
            connection_id: None,
            include_status: true,
        };

        // Act
        let result = addr.send(message).await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_metrics() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        let addr = actor.start();

        // Act
        let result = addr.send(GetStreamMetrics).await;

        // Assert
        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.active_connections, 0);
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);
    }

    #[tokio::test]
    async fn test_actor_shutdown() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let actor = StreamActor::new(fixture.config).unwrap();
        let addr = actor.start();

        // Verify actor is running
        assert!(addr.connected());

        let message = ShutdownConnections {
            graceful: true,
            timeout: Some(Duration::from_secs(5)),
        };

        // Act
        let _result = addr.send(message).await;
        
        // Stop the actor
        addr.do_send(actix::dev::StopArbiter);
        
        // Give time for shutdown
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Assert
        // Actor should be stopped (we can't easily assert this in actix)
    }

    #[tokio::test]
    async fn test_configuration_validation() {
        // Arrange - create invalid config
        let mut config = StreamConfig::default();
        config.connection.governance_endpoints.clear(); // Invalid: no endpoints

        // Act
        let result = config.validate();

        // Assert
        assert!(result.is_err());
        match result {
            Err(ConfigurationError::ValidationFailed { validation_errors }) => {
                assert!(validation_errors.iter().any(|e| e.contains("endpoint")));
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }

    #[tokio::test]
    async fn test_actor_with_supervisor() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        
        // Create mock supervisor (in a real test, this would be a proper supervisor)
        let supervisor_addr = fixture.harness.create_test_supervisor().await;
        
        let actor = StreamActor::new(fixture.config).unwrap()
            .with_supervisor(supervisor_addr);

        // Act
        let addr = actor.start();

        // Assert
        assert!(addr.connected());
        // In a real test, we would verify supervisor registration
    }

    #[tokio::test]
    async fn test_actor_integration_setup() {
        // Arrange
        let fixture = StreamActorTestFixture::new().await;
        let integration = crate::actors::governance_stream::actor::ActorIntegration::default();
        
        let actor = StreamActor::new(fixture.config).unwrap()
            .with_integration(integration);

        // Act
        let addr = actor.start();

        // Assert
        assert!(addr.connected());
        // In a real test, we would verify integration setup
    }

    #[tokio::test]
    async fn test_connection_state_transitions() {
        // Test connection state is_active
        let state = ConnectionState::Connected { since: std::time::Instant::now() };
        assert!(state.is_active());

        let state = ConnectionState::Disconnected;
        assert!(!state.is_active());

        // Test is_connecting
        let state = ConnectionState::Connecting { 
            attempt: 1, 
            next_retry: std::time::Instant::now() 
        };
        assert!(state.is_connecting());

        // Test permanent failure
        let state = ConnectionState::Failed { 
            reason: "test".to_string(), 
            permanent: true 
        };
        assert!(state.is_permanently_failed());
    }

    #[tokio::test]
    async fn test_message_priority_handling() {
        // Test that message priorities work correctly
        use crate::actors::governance_stream::messages::MessagePriority;
        
        let high = MessagePriority::High;
        let normal = MessagePriority::Normal;
        let low = MessagePriority::Low;
        
        assert!(high > normal);
        assert!(normal > low);
        assert!(high > low);
    }

    #[tokio::test]
    async fn test_vote_type_classification() {
        // Test vote type helper methods
        let approve = VoteType::Approve;
        assert!(approve.is_positive());
        assert!(!approve.is_negative());

        let reject = VoteType::Reject;
        assert!(!reject.is_positive());
        assert!(reject.is_negative());

        let conditional = VoteType::ConditionalApprove { 
            conditions: vec!["test".to_string()] 
        };
        assert!(conditional.is_positive());
        assert!(!conditional.is_negative());

        let abstain = VoteType::Abstain;
        assert!(!abstain.is_positive());
        assert!(!abstain.is_negative());
    }

    #[tokio::test]
    async fn test_validation_severity_levels() {
        use crate::actors::governance_stream::types::ValidationSeverity;
        
        let critical = ValidationSeverity::Critical;
        assert!(critical.is_blocking());

        let high = ValidationSeverity::High;
        assert!(!high.is_blocking());
        
        // Test ordering
        assert!(critical > high);
        assert!(high > ValidationSeverity::Medium);
        assert!(ValidationSeverity::Medium > ValidationSeverity::Low);
    }

    #[tokio::test]
    async fn test_service_health_status() {
        use crate::actors::governance_stream::types::ServiceHealthStatus;
        
        let healthy = ServiceHealthStatus::Healthy;
        assert!(healthy.is_operational());

        let degraded = ServiceHealthStatus::Degraded;
        assert!(degraded.is_operational());

        let unhealthy = ServiceHealthStatus::Unhealthy;
        assert!(!unhealthy.is_operational());

        let critical = ServiceHealthStatus::Critical;
        assert!(!critical.is_operational());
    }

    #[tokio::test]
    async fn test_stage_status_completion() {
        use crate::actors::governance_stream::types::StageStatus;
        
        let completed = StageStatus::Completed;
        assert!(completed.is_complete());

        let failed = StageStatus::Failed;
        assert!(failed.is_complete());

        let skipped = StageStatus::Skipped;
        assert!(skipped.is_complete());

        let pending = StageStatus::Pending;
        assert!(!pending.is_complete());

        let in_progress = StageStatus::InProgress;
        assert!(!in_progress.is_complete());
    }
}

/// Integration tests that require more complex setup
#[cfg(test)]
mod integration_tests {
    use super::*;

    // These tests would require mock servers and more complex setup
    // They are marked as ignore to run separately

    #[tokio::test]
    #[ignore]
    async fn test_full_connection_lifecycle() {
        // This would test the full connection lifecycle with a mock gRPC server
        // Including authentication, message exchange, and disconnection
    }

    #[tokio::test]
    #[ignore]
    async fn test_reconnection_behavior() {
        // This would test the reconnection behavior when connections are lost
        // Requiring a mock server that can simulate disconnections
    }

    #[tokio::test]
    #[ignore]
    async fn test_message_ordering_guarantees() {
        // This would test that messages are processed in the correct order
        // Requiring controlled message injection
    }
}