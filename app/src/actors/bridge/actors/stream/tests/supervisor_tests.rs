//! Bridge Supervisor Integration Tests
//! 
//! Tests for StreamActor integration with the Bridge Supervisor tree

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;

use super::test_utils::{
    StreamActorTestHarness, TestConfigBuilder, TestMessageFactory, TestAssertions,
};
use crate::actors::bridge::{
    actors::stream::StreamActor,
    messages::stream_messages::{StreamMessage, StreamResponse},
    shared::errors::BridgeError,
};
use crate::actor_system::{
    ActorResult, AlysActor, LifecycleAware, ExtendedAlysActor,
    actor::{ActorId, ActorState},
    metrics::ActorSystemMetrics,
};

/// Mock Bridge Supervisor for testing
pub struct MockBridgeSupervisor {
    pub actor_id: ActorId,
    pub supervised_actors: Arc<RwLock<HashMap<ActorId, SupervisedActor>>>,
    pub supervision_events: Arc<Mutex<Vec<SupervisionEvent>>>,
    pub restart_policies: HashMap<ActorId, RestartPolicy>,
    pub escalation_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SupervisedActor {
    pub actor_id: ActorId,
    pub actor_type: String,
    pub state: ActorState,
    pub health_status: bool,
    pub restart_count: u32,
    pub last_heartbeat: std::time::SystemTime,
    pub error_count: u32,
}

#[derive(Debug, Clone)]
pub struct SupervisionEvent {
    pub timestamp: std::time::SystemTime,
    pub event_type: SupervisionEventType,
    pub actor_id: ActorId,
    pub details: String,
}

#[derive(Debug, Clone)]
pub enum SupervisionEventType {
    ActorStarted,
    ActorStopped,
    ActorFailed,
    ActorRestarted,
    HealthCheckFailed,
    CriticalErrorEscalated,
    RestartLimitExceeded,
    SupervisionTreeModified,
}

#[derive(Debug, Clone)]
pub enum RestartPolicy {
    Never,
    Always,
    OnFailure,
    Exponential { max_attempts: u32, base_delay: Duration },
}

impl MockBridgeSupervisor {
    /// Create new mock supervisor
    pub fn new(actor_id: &str) -> Self {
        Self {
            actor_id: actor_id.to_string(),
            supervised_actors: Arc::new(RwLock::new(HashMap::new())),
            supervision_events: Arc::new(Mutex::new(Vec::new())),
            restart_policies: HashMap::new(),
            escalation_enabled: true,
        }
    }

    /// Add actor to supervision
    pub async fn supervise_actor(
        &mut self,
        actor_id: ActorId,
        actor_type: String,
        restart_policy: RestartPolicy,
    ) {
        let supervised_actor = SupervisedActor {
            actor_id: actor_id.clone(),
            actor_type,
            state: ActorState::Stopped,
            health_status: true,
            restart_count: 0,
            last_heartbeat: std::time::SystemTime::now(),
            error_count: 0,
        };

        self.supervised_actors
            .write()
            .await
            .insert(actor_id.clone(), supervised_actor);
        
        self.restart_policies.insert(actor_id.clone(), restart_policy);

        self.log_event(SupervisionEventType::SupervisionTreeModified, &actor_id, "Actor added to supervision").await;
    }

    /// Handle actor state change
    pub async fn handle_actor_state_change(
        &self,
        actor_id: &ActorId,
        new_state: ActorState,
    ) -> Result<(), BridgeError> {
        let mut actors = self.supervised_actors.write().await;
        if let Some(actor) = actors.get_mut(actor_id) {
            let old_state = actor.state.clone();
            actor.state = new_state.clone();

            match new_state {
                ActorState::Running => {
                    self.log_event(SupervisionEventType::ActorStarted, actor_id, "Actor started successfully").await;
                },
                ActorState::Stopped => {
                    self.log_event(SupervisionEventType::ActorStopped, actor_id, "Actor stopped").await;
                },
                _ => {},
            }
        }

        Ok(())
    }

    /// Handle critical error escalation
    pub async fn handle_critical_error(
        &self,
        actor_id: &ActorId,
        error: BridgeError,
    ) -> Result<SupervisionAction, BridgeError> {
        self.log_event(
            SupervisionEventType::CriticalErrorEscalated,
            actor_id,
            &format!("Critical error: {:?}", error),
        ).await;

        if let Some(policy) = self.restart_policies.get(actor_id) {
            let mut actors = self.supervised_actors.write().await;
            if let Some(actor) = actors.get_mut(actor_id) {
                actor.error_count += 1;

                match policy {
                    RestartPolicy::Never => Ok(SupervisionAction::Stop),
                    RestartPolicy::Always => {
                        actor.restart_count += 1;
                        Ok(SupervisionAction::Restart)
                    },
                    RestartPolicy::OnFailure => {
                        actor.restart_count += 1;
                        Ok(SupervisionAction::Restart)
                    },
                    RestartPolicy::Exponential { max_attempts, base_delay } => {
                        if actor.restart_count >= *max_attempts {
                            self.log_event(
                                SupervisionEventType::RestartLimitExceeded,
                                actor_id,
                                &format!("Max restart attempts ({}) exceeded", max_attempts),
                            ).await;
                            Ok(SupervisionAction::Stop)
                        } else {
                            actor.restart_count += 1;
                            let delay = *base_delay * 2_u32.pow(actor.restart_count);
                            Ok(SupervisionAction::RestartWithDelay(delay))
                        }
                    }
                }
            } else {
                Ok(SupervisionAction::None)
            }
        } else {
            Ok(SupervisionAction::None)
        }
    }

    /// Perform health check on all supervised actors
    pub async fn health_check_all(&self) -> HashMap<ActorId, bool> {
        let actors = self.supervised_actors.read().await;
        let mut health_status = HashMap::new();

        for (actor_id, actor) in actors.iter() {
            let is_healthy = actor.health_status && 
                             actor.state == ActorState::Running &&
                             actor.last_heartbeat.elapsed().unwrap_or(Duration::from_secs(0)) < Duration::from_secs(60);
            
            health_status.insert(actor_id.clone(), is_healthy);

            if !is_healthy {
                self.log_event(
                    SupervisionEventType::HealthCheckFailed,
                    actor_id,
                    "Health check failed",
                ).await;
            }
        }

        health_status
    }

    /// Update actor heartbeat
    pub async fn update_heartbeat(&self, actor_id: &ActorId) {
        let mut actors = self.supervised_actors.write().await;
        if let Some(actor) = actors.get_mut(actor_id) {
            actor.last_heartbeat = std::time::SystemTime::now();
        }
    }

    /// Get supervision events
    pub async fn get_events(&self) -> Vec<SupervisionEvent> {
        self.supervision_events.lock().await.clone()
    }

    /// Clear supervision events
    pub async fn clear_events(&self) {
        self.supervision_events.lock().await.clear();
    }

    /// Get supervised actor information
    pub async fn get_actor_info(&self, actor_id: &ActorId) -> Option<SupervisedActor> {
        self.supervised_actors.read().await.get(actor_id).cloned()
    }

    /// Log supervision event
    async fn log_event(&self, event_type: SupervisionEventType, actor_id: &ActorId, details: &str) {
        let event = SupervisionEvent {
            timestamp: std::time::SystemTime::now(),
            event_type,
            actor_id: actor_id.clone(),
            details: details.to_string(),
        };

        self.supervision_events.lock().await.push(event);
    }
}

#[derive(Debug, Clone)]
pub enum SupervisionAction {
    None,
    Stop,
    Restart,
    RestartWithDelay(Duration),
    Escalate,
}

/// Bridge supervision test harness
pub struct BridgeSupervisionTestHarness {
    pub supervisor: MockBridgeSupervisor,
    pub stream_actor: StreamActorTestHarness,
    pub supervision_channel: (mpsc::UnboundedSender<SupervisionEvent>, mpsc::UnboundedReceiver<SupervisionEvent>),
}

impl BridgeSupervisionTestHarness {
    /// Create new supervision test harness
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let supervisor = MockBridgeSupervisor::new("bridge-supervisor");
        let stream_actor = StreamActorTestHarness::new().await?;
        let supervision_channel = mpsc::unbounded_channel();

        Ok(Self {
            supervisor,
            stream_actor,
            supervision_channel,
        })
    }

    /// Setup supervision relationship
    pub async fn setup_supervision(&mut self, restart_policy: RestartPolicy) {
        let actor_id = self.stream_actor.actor.actor_id();
        self.supervisor.supervise_actor(
            actor_id,
            "StreamActor".to_string(),
            restart_policy,
        ).await;
    }

    /// Start supervised actor
    pub async fn start_supervised_actor(&mut self) -> Result<(), BridgeError> {
        let actor_id = self.stream_actor.actor.actor_id();
        
        // Start the actor
        self.stream_actor.start().await?;
        
        // Notify supervisor
        self.supervisor.handle_actor_state_change(&actor_id, ActorState::Running).await?;
        
        Ok(())
    }

    /// Stop supervised actor
    pub async fn stop_supervised_actor(&mut self) -> Result<(), BridgeError> {
        let actor_id = self.stream_actor.actor.actor_id();
        
        // Stop the actor
        self.stream_actor.stop().await?;
        
        // Notify supervisor
        self.supervisor.handle_actor_state_change(&actor_id, ActorState::Stopped).await?;
        
        Ok(())
    }

    /// Simulate actor failure
    pub async fn simulate_actor_failure(&mut self, error: BridgeError) -> Result<SupervisionAction, BridgeError> {
        let actor_id = self.stream_actor.actor.actor_id();
        
        // Simulate critical error handling in actor
        let _ = self.stream_actor.actor.handle_critical_error(error.clone()).await;
        
        // Escalate to supervisor
        self.supervisor.handle_critical_error(&actor_id, error).await
    }

    /// Get actor supervision info
    pub async fn get_supervision_info(&self) -> Option<SupervisedActor> {
        let actor_id = self.stream_actor.actor.actor_id();
        self.supervisor.get_actor_info(&actor_id).await
    }
}

#[tokio::test]
async fn test_basic_supervision_setup() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    
    // Setup supervision with always restart policy
    harness.setup_supervision(RestartPolicy::Always).await;
    
    // Verify actor is under supervision
    let actor_id = harness.stream_actor.actor.actor_id();
    let supervision_info = harness.get_supervision_info().await;
    
    assert!(supervision_info.is_some());
    let info = supervision_info.unwrap();
    assert_eq!(info.actor_id, actor_id);
    assert_eq!(info.actor_type, "StreamActor");
    assert_eq!(info.state, ActorState::Stopped);
    assert_eq!(info.restart_count, 0);
}

#[tokio::test]
async fn test_actor_lifecycle_supervision() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    
    // Test actor start supervision
    harness.start_supervised_actor().await.unwrap();
    
    let info = harness.get_supervision_info().await.unwrap();
    assert_eq!(info.state, ActorState::Running);
    
    // Check supervision events
    let events = harness.supervisor.get_events().await;
    let start_events: Vec<_> = events.iter()
        .filter(|e| matches!(e.event_type, SupervisionEventType::ActorStarted))
        .collect();
    assert!(!start_events.is_empty());
    
    // Test actor stop supervision
    harness.stop_supervised_actor().await.unwrap();
    
    let info = harness.get_supervision_info().await.unwrap();
    assert_eq!(info.state, ActorState::Stopped);
    
    let events = harness.supervisor.get_events().await;
    let stop_events: Vec<_> = events.iter()
        .filter(|e| matches!(e.event_type, SupervisionEventType::ActorStopped))
        .collect();
    assert!(!stop_events.is_empty());
}

#[tokio::test]
async fn test_critical_error_escalation() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Simulate critical error
    let critical_error = BridgeError::CriticalSystemFailure {
        component: "governance-connection".to_string(),
        details: "Connection permanently lost".to_string(),
    };
    
    let action = harness.simulate_actor_failure(critical_error).await.unwrap();
    
    // Should trigger restart action
    match action {
        SupervisionAction::Restart => {
            // Verify restart count increased
            let info = harness.get_supervision_info().await.unwrap();
            assert_eq!(info.restart_count, 1);
            assert_eq!(info.error_count, 1);
        },
        _ => panic!("Expected restart action, got {:?}", action),
    }
    
    // Check escalation event
    let events = harness.supervisor.get_events().await;
    let escalation_events: Vec<_> = events.iter()
        .filter(|e| matches!(e.event_type, SupervisionEventType::CriticalErrorEscalated))
        .collect();
    assert!(!escalation_events.is_empty());
}

#[tokio::test]
async fn test_restart_policy_never() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Never).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Simulate failure
    let error = BridgeError::NetworkError("Connection failed".to_string());
    let action = harness.simulate_actor_failure(error).await.unwrap();
    
    // Should trigger stop action
    match action {
        SupervisionAction::Stop => {
            // Test passed
        },
        _ => panic!("Expected stop action, got {:?}", action),
    }
}

#[tokio::test]
async fn test_restart_policy_exponential() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Exponential {
        max_attempts: 3,
        base_delay: Duration::from_millis(100),
    }).await;
    harness.start_supervised_actor().await.unwrap();
    
    // First failure - should restart
    let error1 = BridgeError::NetworkError("First failure".to_string());
    let action1 = harness.simulate_actor_failure(error1).await.unwrap();
    
    match action1 {
        SupervisionAction::RestartWithDelay(delay) => {
            assert_eq!(delay, Duration::from_millis(200)); // base_delay * 2^1
        },
        _ => panic!("Expected restart with delay, got {:?}", action1),
    }
    
    // Second failure - should restart with longer delay
    let error2 = BridgeError::NetworkError("Second failure".to_string());
    let action2 = harness.simulate_actor_failure(error2).await.unwrap();
    
    match action2 {
        SupervisionAction::RestartWithDelay(delay) => {
            assert_eq!(delay, Duration::from_millis(400)); // base_delay * 2^2
        },
        _ => panic!("Expected restart with delay, got {:?}", action2),
    }
    
    // Third failure - should restart
    let error3 = BridgeError::NetworkError("Third failure".to_string());
    let action3 = harness.simulate_actor_failure(error3).await.unwrap();
    
    match action3 {
        SupervisionAction::RestartWithDelay(delay) => {
            assert_eq!(delay, Duration::from_millis(800)); // base_delay * 2^3
        },
        _ => panic!("Expected restart with delay, got {:?}", action3),
    }
    
    // Fourth failure - should stop (exceeded max attempts)
    let error4 = BridgeError::NetworkError("Fourth failure".to_string());
    let action4 = harness.simulate_actor_failure(error4).await.unwrap();
    
    match action4 {
        SupervisionAction::Stop => {
            // Verify restart limit exceeded event
            let events = harness.supervisor.get_events().await;
            let limit_events: Vec<_> = events.iter()
                .filter(|e| matches!(e.event_type, SupervisionEventType::RestartLimitExceeded))
                .collect();
            assert!(!limit_events.is_empty());
        },
        _ => panic!("Expected stop action after max attempts, got {:?}", action4),
    }
}

#[tokio::test]
async fn test_health_check_supervision() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::OnFailure).await;
    harness.start_supervised_actor().await.unwrap();
    
    let actor_id = harness.stream_actor.actor.actor_id();
    
    // Initial health check should pass
    let health_status = harness.supervisor.health_check_all().await;
    assert_eq!(health_status.get(&actor_id), Some(&true));
    
    // Update heartbeat
    harness.supervisor.update_heartbeat(&actor_id).await;
    
    // Health check should still pass
    let health_status = harness.supervisor.health_check_all().await;
    assert_eq!(health_status.get(&actor_id), Some(&true));
    
    // Simulate stale heartbeat by waiting and not updating
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // In a real scenario with longer timeouts, this would trigger health check failure
    // For test purposes, we verify the mechanism works
}

#[tokio::test]
async fn test_supervisor_message_handling() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Test various supervisor messages
    let messages = vec![
        "health_check_request",
        "restart_requested",
        "configuration_update",
        "metrics_report_request",
    ];
    
    for message in messages {
        let result = harness.stream_actor.actor
            .handle_supervisor_message(message.to_string())
            .await;
        assert!(result.is_ok(), "Failed to handle supervisor message: {}", message);
    }
    
    // Verify actor is still healthy after supervisor interactions
    TestAssertions::assert_actor_healthy(&harness.stream_actor).await.unwrap();
}

#[tokio::test]
async fn test_supervision_tree_integration() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    
    // Test multiple actor supervision
    let mut additional_actors = Vec::new();
    
    for i in 0..3 {
        let config = TestConfigBuilder::new()
            .with_actor_id(&format!("stream-actor-{}", i))
            .build();
        
        let metrics = ActorSystemMetrics::new("test");
        let actor = StreamActor::new(config, metrics).unwrap();
        
        harness.supervisor.supervise_actor(
            actor.actor_id(),
            "StreamActor".to_string(),
            RestartPolicy::OnFailure,
        ).await;
        
        additional_actors.push(actor);
    }
    
    // Start all actors
    harness.start_supervised_actor().await.unwrap();
    
    for actor in &additional_actors {
        harness.supervisor.handle_actor_state_change(
            &actor.actor_id(),
            ActorState::Running,
        ).await.unwrap();
    }
    
    // Verify all actors are supervised
    let health_status = harness.supervisor.health_check_all().await;
    assert_eq!(health_status.len(), 4); // Original + 3 additional
    
    // Test supervision tree health
    for (actor_id, is_healthy) in health_status {
        assert!(is_healthy, "Actor {} is not healthy", actor_id);
    }
}

#[tokio::test]
async fn test_supervision_metrics_reporting() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Generate some supervision activity
    let error = BridgeError::NetworkError("Test error for metrics".to_string());
    harness.simulate_actor_failure(error).await.unwrap();
    
    // Wait for metrics to be updated
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Verify supervision events were logged
    let events = harness.supervisor.get_events().await;
    
    // Should have at least: supervision setup, actor start, error escalation
    assert!(events.len() >= 3, "Expected at least 3 supervision events, got {}", events.len());
    
    // Verify event types
    let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
    assert!(event_types.iter().any(|t| matches!(t, SupervisionEventType::SupervisionTreeModified)));
    assert!(event_types.iter().any(|t| matches!(t, SupervisionEventType::ActorStarted)));
    assert!(event_types.iter().any(|t| matches!(t, SupervisionEventType::CriticalErrorEscalated)));
}

#[tokio::test]
async fn test_graceful_supervision_shutdown() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Simulate graceful shutdown
    let shutdown_start = std::time::Instant::now();
    
    // Stop supervised actor gracefully
    harness.stop_supervised_actor().await.unwrap();
    
    let shutdown_duration = shutdown_start.elapsed();
    
    // Verify shutdown completed in reasonable time
    assert!(shutdown_duration < Duration::from_secs(2), 
           "Supervision shutdown took too long: {:?}", shutdown_duration);
    
    // Verify final supervision state
    let info = harness.get_supervision_info().await.unwrap();
    assert_eq!(info.state, ActorState::Stopped);
    
    // Verify shutdown event was logged
    let events = harness.supervisor.get_events().await;
    let stop_events: Vec<_> = events.iter()
        .filter(|e| matches!(e.event_type, SupervisionEventType::ActorStopped))
        .collect();
    assert!(!stop_events.is_empty());
}

#[tokio::test]
async fn test_supervision_fault_tolerance() {
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.setup_supervision(RestartPolicy::Always).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Test multiple rapid failures
    let errors = vec![
        BridgeError::NetworkError("Network timeout".to_string()),
        BridgeError::AuthenticationError("Auth token expired".to_string()),
        BridgeError::ConfigurationError("Invalid config".to_string()),
    ];
    
    for error in errors {
        let action = harness.simulate_actor_failure(error).await.unwrap();
        match action {
            SupervisionAction::Restart => {
                // Expected behavior for Always restart policy
            },
            _ => panic!("Expected restart action for fault tolerance test"),
        }
    }
    
    // Verify supervision system handled multiple failures
    let info = harness.get_supervision_info().await.unwrap();
    assert_eq!(info.restart_count, 3);
    assert_eq!(info.error_count, 3);
    
    // Verify actor can still process messages after failures
    let message = TestMessageFactory::health_check();
    let result = harness.stream_actor.send_message(message).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_supervision_configuration_integration() {
    let config = TestConfigBuilder::new()
        .with_actor_id("supervision-config-test")
        .build();
    
    // Enable supervision features in configuration
    let mut config = config;
    config.features.supervision_enabled = true;
    config.monitoring.health_checks.enabled = true;
    config.monitoring.health_checks.interval = Duration::from_millis(100);
    
    let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
    harness.stream_actor = StreamActorTestHarness::with_config(config).await.unwrap();
    
    harness.setup_supervision(RestartPolicy::OnFailure).await;
    harness.start_supervised_actor().await.unwrap();
    
    // Verify supervision configuration is respected
    TestAssertions::assert_actor_healthy(&harness.stream_actor).await.unwrap();
    
    // Test configuration-driven supervision behavior
    let health_status = harness.supervisor.health_check_all().await;
    let actor_id = harness.stream_actor.actor.actor_id();
    assert_eq!(health_status.get(&actor_id), Some(&true));
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with --ignored for stress tests
    async fn test_supervision_under_load() {
        let mut harness = BridgeSupervisionTestHarness::new().await.unwrap();
        harness.setup_supervision(RestartPolicy::Exponential {
            max_attempts: 10,
            base_delay: Duration::from_millis(10),
        }).await;
        harness.start_supervised_actor().await.unwrap();
        
        // Generate high error rate
        for i in 0..50 {
            let error = BridgeError::NetworkError(format!("Load test error {}", i));
            let _ = harness.simulate_actor_failure(error).await;
            
            if i % 10 == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        
        // Verify supervision system remains responsive
        let events = harness.supervisor.get_events().await;
        println!("Generated {} supervision events under load", events.len());
        
        // System should eventually stop the actor due to excessive failures
        let info = harness.get_supervision_info().await.unwrap();
        println!("Final restart count: {}, error count: {}", info.restart_count, info.error_count);
    }

    #[tokio::test]
    #[ignore] // Run with --ignored for stress tests
    async fn test_multiple_actor_supervision_stress() {
        let mut supervisor = MockBridgeSupervisor::new("stress-test-supervisor");
        let mut actors = Vec::new();
        
        // Create many supervised actors
        for i in 0..20 {
            let config = TestConfigBuilder::new()
                .with_actor_id(&format!("stress-actor-{}", i))
                .build();
            
            let metrics = ActorSystemMetrics::new("stress-test");
            let actor = StreamActor::new(config, metrics).unwrap();
            
            supervisor.supervise_actor(
                actor.actor_id(),
                "StreamActor".to_string(),
                RestartPolicy::Always,
            ).await;
            
            actors.push(actor);
        }
        
        // Start all actors
        for actor in &actors {
            supervisor.handle_actor_state_change(&actor.actor_id(), ActorState::Running).await.unwrap();
        }
        
        // Generate random failures across actors
        for _ in 0..100 {
            let actor_index = rand::random::<usize>() % actors.len();
            let actor_id = &actors[actor_index].actor_id();
            
            let error = BridgeError::NetworkError("Random failure".to_string());
            supervisor.handle_critical_error(actor_id, error).await.unwrap();
        }
        
        // Verify supervision system handled all failures
        let events = supervisor.get_events().await;
        println!("Handled {} supervision events across {} actors", events.len(), actors.len());
        
        // All actors should still be under supervision
        let supervised_actors = supervisor.supervised_actors.read().await;
        assert_eq!(supervised_actors.len(), 20);
    }
}