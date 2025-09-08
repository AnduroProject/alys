//! Network Supervisor Tests
//! 
//! Unit tests for NetworkSupervisor functionality including fault tolerance,
//! actor restart policies, and system recovery.

use actix::prelude::*;
use std::time::Duration;

use crate::actors::network::{supervisor::NetworkSupervisor, messages::*};
use crate::actors::network::tests::helpers::*;

#[actix::test]
async fn test_supervisor_initialization() {
    let config = test_supervisor_config();
    let supervisor = NetworkSupervisor::new(config);
    
    // Test that supervisor initializes correctly
    assert!(supervisor.is_ok());
}

#[actix::test]
async fn test_supervisor_start_all_actors() {
    let config = test_supervisor_config();
    let supervisor = NetworkSupervisor::new(config).unwrap();
    let addr = supervisor.start();
    
    let msg = StartAllNetworkActors {
        network_config: test_network_config(),
        sync_config: test_sync_config(),
        peer_config: test_peer_config(),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_supervisor_stop_all_actors() {
    let config = test_supervisor_config();
    let supervisor = NetworkSupervisor::new(config).unwrap();
    let addr = supervisor.start();
    
    let msg = StopAllNetworkActors { 
        graceful: true 
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_supervisor_health_monitoring() {
    let config = test_supervisor_config();
    let supervisor = NetworkSupervisor::new(config).unwrap();
    let addr = supervisor.start();
    
    let msg = GetSystemHealth;
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(health)) = result {
        assert!(health.overall_health_score >= 0.0);
    }
}

#[actix::test]
async fn test_supervisor_actor_restart() {
    let config = test_supervisor_config();
    let supervisor = NetworkSupervisor::new(config).unwrap();
    let addr = supervisor.start();
    
    // Simulate actor failure
    let msg = HandleActorFailure {
        actor_type: NetworkActorType::Network,
        failure_reason: "Simulated crash".to_string(),
        restart_policy: RestartPolicy::Immediate,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_supervisor_escalation_policy() {
    let config = test_supervisor_config();
    let supervisor = NetworkSupervisor::new(config).unwrap();
    let addr = supervisor.start();
    
    // Test escalation when multiple failures occur
    for i in 0..3 {
        let msg = HandleActorFailure {
            actor_type: NetworkActorType::Peer,
            failure_reason: format!("Failure #{}", i + 1),
            restart_policy: RestartPolicy::Escalate,
        };
        
        let result = addr.send(msg).await;
        assert!(result.is_ok());
    }
}