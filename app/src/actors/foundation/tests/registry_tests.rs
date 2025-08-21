//! Comprehensive Test Suite for Phase 3: Actor Registry & Discovery
//! 
//! Tests for ALYS-006-12 through ALYS-006-15 covering all actor registry
//! functionality including registration, discovery, lifecycle management,
//! and cleanup operations with Alys Testing Framework integration.

use crate::actors::foundation::{
    ActorRegistry, ActorRegistryConfig, ActorRegistryEntry, ActorLifecycleState,
    ActorPriority, ActorQuery, HealthState, HealthStatus, RegistrationContext,
    RegistryError, ThreadSafeActorRegistry, MaintenanceReport, BatchResult,
    ActorTypeStatistics, constants::registry
};
use actix::{Actor, ActorContext, Addr, Context, Handler, Message};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;
use uuid::Uuid;

/// Test actor for registry testing
#[derive(Debug)]
struct TestActor {
    name: String,
    value: i32,
}

impl TestActor {
    fn new(name: String) -> Self {
        Self { name, value: 0 }
    }

    fn with_value(name: String, value: i32) -> Self {
        Self { name, value }
    }
}

impl Actor for TestActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("TestActor '{}' started", self.name);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("TestActor '{}' stopped", self.name);
    }
}

/// Test message for actor communication
#[derive(Debug, Message)]
#[rtype(result = "String")]
struct TestMessage(String);

impl Handler<TestMessage> for TestActor {
    type Result = String;

    fn handle(&mut self, msg: TestMessage, _ctx: &mut Self::Context) -> Self::Result {
        format!("TestActor '{}' received: {}", self.name, msg.0)
    }
}

/// Different test actor type for type-based testing
#[derive(Debug)]
struct MockChainActor {
    block_height: u64,
}

impl MockChainActor {
    fn new() -> Self {
        Self { block_height: 0 }
    }
}

impl Actor for MockChainActor {
    type Context = Context<Self>;
}

/// Another test actor type
#[derive(Debug)]
struct MockEngineActor {
    engine_state: String,
}

impl MockEngineActor {
    fn new() -> Self {
        Self {
            engine_state: "initialized".to_string(),
        }
    }
}

impl Actor for MockEngineActor {
    type Context = Context<Self>;
}

/// Test helper to create default registration context
fn default_registration_context() -> RegistrationContext {
    RegistrationContext {
        source: "test".to_string(),
        supervisor: Some("test_supervisor".to_string()),
        config: HashMap::new(),
        feature_flags: HashSet::new(),
    }
}

/// Test helper to create test tags
fn test_tags(tags: &[&str]) -> HashSet<String> {
    tags.iter().map(|&s| s.to_string()).collect()
}

#[cfg(test)]
mod registry_core_tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let config = ActorRegistryConfig::default();
        let registry = ActorRegistry::new(config);
        
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
        assert!(!registry.is_locked());
    }

    #[test]
    fn test_registry_development_config() {
        let registry = ActorRegistry::development();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_production_config() {
        let registry = ActorRegistry::production();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn test_basic_actor_registration() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        let result = registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        
        assert!(result.is_ok());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains_actor("test_actor"));
    }

    #[tokio::test]
    async fn test_actor_registration_validation() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        // Test empty name
        let result = registry.register_actor(
            "".to_string(),
            addr.clone(),
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(matches!(result, Err(RegistryError::InvalidActorName(_))));
        
        // Test name too long
        let long_name = "a".repeat(registry::MAX_ACTOR_NAME_LENGTH + 1);
        let result = registry.register_actor(
            long_name,
            addr.clone(),
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(matches!(result, Err(RegistryError::InvalidActorName(_))));
        
        // Test invalid characters
        let result = registry.register_actor(
            "test@actor!".to_string(),
            addr.clone(),
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(matches!(result, Err(RegistryError::InvalidActorName(_))));
        
        // Test valid name
        let result = registry.register_actor(
            "test_actor-01".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let mut registry = ActorRegistry::development();
        let actor1 = TestActor::new("test_actor".to_string());
        let actor2 = TestActor::new("test_actor".to_string());
        let addr1 = actor1.start();
        let addr2 = actor2.start();
        
        // Register first actor
        let result = registry.register_actor(
            "test_actor".to_string(),
            addr1,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(result.is_ok());
        
        // Try to register second actor with same name
        let result = registry.register_actor(
            "test_actor".to_string(),
            addr2,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(matches!(result, Err(RegistryError::ActorAlreadyRegistered(_))));
    }

    #[tokio::test]
    async fn test_registry_capacity_limit() {
        let config = ActorRegistryConfig {
            max_actors: 2,
            ..Default::default()
        };
        let mut registry = ActorRegistry::new(config);
        
        // Register two actors (at capacity)
        for i in 0..2 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            let result = registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            );
            assert!(result.is_ok());
        }
        
        // Try to register third actor (should fail)
        let actor = TestActor::new("actor_2".to_string());
        let addr = actor.start();
        let result = registry.register_actor(
            "actor_2".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        );
        assert!(matches!(result, Err(RegistryError::RegistryCapacityExceeded { .. })));
    }
}

#[cfg(test)]
mod registry_lookup_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_actor_by_name() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let original_addr = actor.start();
        
        registry.register_actor(
            "test_actor".to_string(),
            original_addr.clone(),
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        ).unwrap();
        
        let retrieved_addr: Option<Addr<TestActor>> = registry.get_actor("test_actor");
        assert!(retrieved_addr.is_some());
        
        // Test non-existent actor
        let missing_addr: Option<Addr<TestActor>> = registry.get_actor("missing_actor");
        assert!(missing_addr.is_none());
    }

    #[tokio::test]
    async fn test_get_actors_by_type() {
        let mut registry = ActorRegistry::development();
        
        // Register multiple TestActors
        for i in 0..3 {
            let actor = TestActor::new(format!("test_actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("test_actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
        }
        
        // Register a different type
        let chain_actor = MockChainActor::new();
        let chain_addr = chain_actor.start();
        registry.register_actor(
            "chain_actor".to_string(),
            chain_addr,
            ActorPriority::Critical,
            HashSet::new(),
            default_registration_context(),
        ).unwrap();
        
        // Get all TestActors
        let test_actors: Vec<Addr<TestActor>> = registry.get_actors_by_type();
        assert_eq!(test_actors.len(), 3);
        
        // Get all MockChainActors
        let chain_actors: Vec<Addr<MockChainActor>> = registry.get_actors_by_type();
        assert_eq!(chain_actors.len(), 1);
        
        // Get non-existent type
        let engine_actors: Vec<Addr<MockEngineActor>> = registry.get_actors_by_type();
        assert_eq!(engine_actors.len(), 0);
    }

    #[tokio::test]
    async fn test_get_actors_by_priority() {
        let mut registry = ActorRegistry::development();
        
        // Register actors with different priorities
        let priorities = [ActorPriority::Critical, ActorPriority::High, ActorPriority::Normal, ActorPriority::Low];
        
        for (i, &priority) in priorities.iter().enumerate() {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                priority,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
        }
        
        // Test getting actors by each priority
        let critical_actors = registry.get_actors_by_priority(ActorPriority::Critical);
        assert_eq!(critical_actors.len(), 1);
        assert_eq!(critical_actors[0], "actor_0");
        
        let high_actors = registry.get_actors_by_priority(ActorPriority::High);
        assert_eq!(high_actors.len(), 1);
        assert_eq!(high_actors[0], "actor_1");
        
        let normal_actors = registry.get_actors_by_priority(ActorPriority::Normal);
        assert_eq!(normal_actors.len(), 1);
        
        let low_actors = registry.get_actors_by_priority(ActorPriority::Low);
        assert_eq!(low_actors.len(), 1);
        
        // Test non-existent priority
        let background_actors = registry.get_actors_by_priority(ActorPriority::Background);
        assert_eq!(background_actors.len(), 0);
    }

    #[tokio::test]
    async fn test_get_actors_by_tag() {
        let mut registry = ActorRegistry::development();
        
        // Register actors with different tags
        let actor1 = TestActor::new("actor1".to_string());
        let addr1 = actor1.start();
        registry.register_actor(
            "actor1".to_string(),
            addr1,
            ActorPriority::Normal,
            test_tags(&["consensus", "critical"]),
            default_registration_context(),
        ).unwrap();
        
        let actor2 = TestActor::new("actor2".to_string());
        let addr2 = actor2.start();
        registry.register_actor(
            "actor2".to_string(),
            addr2,
            ActorPriority::Normal,
            test_tags(&["network", "p2p"]),
            default_registration_context(),
        ).unwrap();
        
        let actor3 = TestActor::new("actor3".to_string());
        let addr3 = actor3.start();
        registry.register_actor(
            "actor3".to_string(),
            addr3,
            ActorPriority::Normal,
            test_tags(&["consensus", "network"]),
            default_registration_context(),
        ).unwrap();
        
        // Test tag-based lookup
        let consensus_actors = registry.get_actors_by_tag("consensus");
        assert_eq!(consensus_actors.len(), 2);
        assert!(consensus_actors.contains(&"actor1".to_string()));
        assert!(consensus_actors.contains(&"actor3".to_string()));
        
        let network_actors = registry.get_actors_by_tag("network");
        assert_eq!(network_actors.len(), 2);
        
        let critical_actors = registry.get_actors_by_tag("critical");
        assert_eq!(critical_actors.len(), 1);
        assert_eq!(critical_actors[0], "actor1");
        
        let missing_actors = registry.get_actors_by_tag("missing");
        assert_eq!(missing_actors.len(), 0);
    }

    #[tokio::test]
    async fn test_get_actors_by_state() {
        let mut registry = ActorRegistry::development();
        
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        ).unwrap();
        
        // Initially in Registering state
        let registering_actors = registry.get_actors_by_state(ActorLifecycleState::Registering);
        assert_eq!(registering_actors.len(), 1);
        
        // Update to Active state
        registry.update_actor_state("test_actor", ActorLifecycleState::Active).unwrap();
        let active_actors = registry.get_actors_by_state(ActorLifecycleState::Active);
        assert_eq!(active_actors.len(), 1);
        
        let registering_actors = registry.get_actors_by_state(ActorLifecycleState::Registering);
        assert_eq!(registering_actors.len(), 0);
    }
}

#[cfg(test)]
mod registry_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_state_transitions() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        ).unwrap();
        
        // Test valid state transitions
        assert!(registry.update_actor_state("test_actor", ActorLifecycleState::Active).is_ok());
        assert!(registry.update_actor_state("test_actor", ActorLifecycleState::Suspended).is_ok());
        assert!(registry.update_actor_state("test_actor", ActorLifecycleState::Active).is_ok());
        assert!(registry.update_actor_state("test_actor", ActorLifecycleState::ShuttingDown).is_ok());
        assert!(registry.update_actor_state("test_actor", ActorLifecycleState::Terminated).is_ok());
        
        // Test invalid state transition
        let result = registry.update_actor_state("test_actor", ActorLifecycleState::Active);
        assert!(matches!(result, Err(RegistryError::LifecycleViolation(_))));
    }

    #[tokio::test]
    async fn test_actor_metadata_updates() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        ).unwrap();
        
        let mut metadata = HashMap::new();
        metadata.insert("version".to_string(), "1.0.0".to_string());
        metadata.insert("component".to_string(), "test".to_string());
        
        let result = registry.update_actor_metadata("test_actor", metadata);
        assert!(result.is_ok());
        
        let entry = registry.get_entry("test_actor").unwrap();
        assert_eq!(entry.metadata.get("version"), Some(&"1.0.0".to_string()));
        assert_eq!(entry.metadata.get("component"), Some(&"test".to_string()));
        
        // Test updating non-existent actor
        let mut metadata = HashMap::new();
        metadata.insert("test".to_string(), "value".to_string());
        let result = registry.update_actor_metadata("missing_actor", metadata);
        assert!(matches!(result, Err(RegistryError::ActorNotFound(_))));
    }

    #[tokio::test]
    async fn test_actor_health_updates() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        ).unwrap();
        
        let health_status = HealthStatus {
            status: HealthState::Healthy,
            last_check: Some(SystemTime::now()),
            error_count: 0,
            success_rate: 1.0,
            issues: vec![],
        };
        
        let result = registry.update_actor_health("test_actor", health_status.clone());
        assert!(result.is_ok());
        
        let entry = registry.get_entry("test_actor").unwrap();
        assert_eq!(entry.health_status.status, HealthState::Healthy);
        assert_eq!(entry.health_status.success_rate, 1.0);
    }

    #[tokio::test]
    async fn test_actor_tag_management() {
        let mut registry = ActorRegistry::development();
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            test_tags(&["initial"]),
            default_registration_context(),
        ).unwrap();
        
        // Add new tags
        let new_tags = test_tags(&["consensus", "critical"]);
        let result = registry.add_actor_tags("test_actor", new_tags);
        assert!(result.is_ok());
        
        let entry = registry.get_entry("test_actor").unwrap();
        assert!(entry.tags.contains("initial"));
        assert!(entry.tags.contains("consensus"));
        assert!(entry.tags.contains("critical"));
        
        // Remove a tag
        let tags_to_remove = test_tags(&["initial"]);
        let result = registry.remove_actor_tags("test_actor", &tags_to_remove);
        assert!(result.is_ok());
        
        let entry = registry.get_entry("test_actor").unwrap();
        assert!(!entry.tags.contains("initial"));
        assert!(entry.tags.contains("consensus"));
        assert!(entry.tags.contains("critical"));
    }
}

#[cfg(test)]
mod registry_discovery_tests {
    use super::*;

    #[tokio::test]
    async fn test_batch_get_actors() {
        let mut registry = ActorRegistry::development();
        
        // Register multiple actors
        for i in 0..3 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
        }
        
        let names = vec!["actor_0".to_string(), "actor_2".to_string(), "missing_actor".to_string()];
        let results: Vec<(String, Addr<TestActor>)> = registry.batch_get_actors(&names);
        
        assert_eq!(results.len(), 2); // Should only return existing actors
        assert!(results.iter().any(|(name, _)| name == "actor_0"));
        assert!(results.iter().any(|(name, _)| name == "actor_2"));
    }

    #[tokio::test]
    async fn test_find_actors_by_pattern() {
        let mut registry = ActorRegistry::development();
        
        // Register actors with pattern-matching names
        let names = ["test_actor_1", "test_actor_2", "prod_actor_1", "dev_service"];
        for name in &names {
            let actor = TestActor::new(name.to_string());
            let addr = actor.start();
            registry.register_actor(
                name.to_string(),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
        }
        
        // Test pattern matching
        let test_actors: Vec<(String, Addr<TestActor>)> = registry.find_actors_by_pattern("test_actor_*");
        assert_eq!(test_actors.len(), 2);
        
        let actor_actors: Vec<(String, Addr<TestActor>)> = registry.find_actors_by_pattern("*_actor_*");
        assert_eq!(actor_actors.len(), 3);
        
        let service_actors: Vec<(String, Addr<TestActor>)> = registry.find_actors_by_pattern("*_service");
        assert_eq!(service_actors.len(), 1);
    }

    #[tokio::test]
    async fn test_get_actors_by_tags_intersection() {
        let mut registry = ActorRegistry::development();
        
        let actor1 = TestActor::new("actor1".to_string());
        let addr1 = actor1.start();
        registry.register_actor(
            "actor1".to_string(),
            addr1,
            ActorPriority::Normal,
            test_tags(&["consensus", "critical", "blockchain"]),
            default_registration_context(),
        ).unwrap();
        
        let actor2 = TestActor::new("actor2".to_string());
        let addr2 = actor2.start();
        registry.register_actor(
            "actor2".to_string(),
            addr2,
            ActorPriority::Normal,
            test_tags(&["consensus", "network"]),
            default_registration_context(),
        ).unwrap();
        
        let actor3 = TestActor::new("actor3".to_string());
        let addr3 = actor3.start();
        registry.register_actor(
            "actor3".to_string(),
            addr3,
            ActorPriority::Normal,
            test_tags(&["critical", "blockchain"]),
            default_registration_context(),
        ).unwrap();
        
        // Test intersection (all tags must be present)
        let consensus_critical = registry.get_actors_by_tags_intersection(&["consensus".to_string(), "critical".to_string()]);
        assert_eq!(consensus_critical.len(), 1);
        assert!(consensus_critical.contains(&"actor1".to_string()));
        
        let consensus_only = registry.get_actors_by_tags_intersection(&["consensus".to_string()]);
        assert_eq!(consensus_only.len(), 2);
        
        let impossible = registry.get_actors_by_tags_intersection(&["consensus".to_string(), "network".to_string(), "critical".to_string()]);
        assert_eq!(impossible.len(), 0);
    }

    #[tokio::test]
    async fn test_get_actors_by_tags_union() {
        let mut registry = ActorRegistry::development();
        
        let actor1 = TestActor::new("actor1".to_string());
        let addr1 = actor1.start();
        registry.register_actor(
            "actor1".to_string(),
            addr1,
            ActorPriority::Normal,
            test_tags(&["consensus"]),
            default_registration_context(),
        ).unwrap();
        
        let actor2 = TestActor::new("actor2".to_string());
        let addr2 = actor2.start();
        registry.register_actor(
            "actor2".to_string(),
            addr2,
            ActorPriority::Normal,
            test_tags(&["network"]),
            default_registration_context(),
        ).unwrap();
        
        let actor3 = TestActor::new("actor3".to_string());
        let addr3 = actor3.start();
        registry.register_actor(
            "actor3".to_string(),
            addr3,
            ActorPriority::Normal,
            test_tags(&["storage"]),
            default_registration_context(),
        ).unwrap();
        
        // Test union (any tag can be present)
        let consensus_or_network = registry.get_actors_by_tags_union(&["consensus".to_string(), "network".to_string()]);
        assert_eq!(consensus_or_network.len(), 2);
        
        let all_tags = registry.get_actors_by_tags_union(&["consensus".to_string(), "network".to_string(), "storage".to_string()]);
        assert_eq!(all_tags.len(), 3);
        
        let missing_tag = registry.get_actors_by_tags_union(&["missing".to_string()]);
        assert_eq!(missing_tag.len(), 0);
    }

    #[tokio::test]
    async fn test_get_healthy_actors() {
        let mut registry = ActorRegistry::development();
        
        // Register actors and set different health states
        for i in 0..4 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
            
            // Set to active state
            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
        }
        
        // Set different health statuses
        let health_states = [HealthState::Healthy, HealthState::Warning, HealthState::Unhealthy, HealthState::Critical];
        
        for (i, &health_state) in health_states.iter().enumerate() {
            let health_status = HealthStatus {
                status: health_state,
                last_check: Some(SystemTime::now()),
                error_count: 0,
                success_rate: 0.0,
                issues: vec![],
            };
            registry.update_actor_health(&format!("actor_{}", i), health_status).unwrap();
        }
        
        // Get healthy actors (should include Healthy and Warning)
        let healthy_actors: Vec<Addr<TestActor>> = registry.get_healthy_actors();
        assert_eq!(healthy_actors.len(), 2);
    }
}

#[cfg(test)]
mod registry_query_tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_query_builder() {
        let mut registry = ActorRegistry::development();
        
        // Register test actors with various attributes
        for i in 0..5 {
            let actor = TestActor::new(format!("test_actor_{}", i));
            let addr = actor.start();
            
            let priority = match i {
                0 | 1 => ActorPriority::Critical,
                2 | 3 => ActorPriority::Normal,
                _ => ActorPriority::Low,
            };
            
            let tags = match i {
                0 => test_tags(&["consensus", "critical"]),
                1 => test_tags(&["network", "critical"]),
                2 => test_tags(&["consensus", "normal"]),
                3 => test_tags(&["storage", "normal"]),
                _ => test_tags(&["background"]),
            };
            
            registry.register_actor(
                format!("test_actor_{}", i),
                addr,
                priority,
                tags,
                default_registration_context(),
            ).unwrap();
            
            registry.update_actor_state(&format!("test_actor_{}", i), ActorLifecycleState::Active).unwrap();
        }
        
        // Test query by priority
        let query = ActorQuery::new().with_priority(ActorPriority::Critical);
        let results = registry.query_actors(query);
        assert_eq!(results.len(), 2);
        
        // Test query by tags
        let query = ActorQuery::new().with_required_tags(vec!["consensus".to_string()]);
        let results = registry.query_actors(query);
        assert_eq!(results.len(), 2);
        
        // Test query by name pattern
        let query = ActorQuery::new().with_name_pattern("test_actor_[0-2]".to_string());
        let results = registry.query_actors(query);
        assert_eq!(results.len(), 3);
        
        // Test complex query
        let query = ActorQuery::new()
            .with_priority(ActorPriority::Critical)
            .with_any_tags(vec!["consensus".to_string(), "network".to_string()])
            .with_state(ActorLifecycleState::Active);
        let results = registry.query_actors(query);
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_actor_type_statistics() {
        let mut registry = ActorRegistry::development();
        
        // Register multiple TestActors with different states
        for i in 0..5 {
            let actor = TestActor::new(format!("test_actor_{}", i));
            let addr = actor.start();
            
            let priority = if i < 2 { ActorPriority::Critical } else { ActorPriority::Normal };
            
            registry.register_actor(
                format!("test_actor_{}", i),
                addr,
                priority,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
            
            // Set different states
            let state = if i < 3 { ActorLifecycleState::Active } else { ActorLifecycleState::Suspended };
            registry.update_actor_state(&format!("test_actor_{}", i), state).unwrap();
            
            // Set health status
            let health_status = HealthStatus {
                status: if i < 4 { HealthState::Healthy } else { HealthState::Unhealthy },
                last_check: Some(SystemTime::now()),
                error_count: 0,
                success_rate: 1.0,
                issues: vec![],
            };
            registry.update_actor_health(&format!("test_actor_{}", i), health_status).unwrap();
        }
        
        let stats: ActorTypeStatistics = registry.get_actor_type_statistics::<TestActor>();
        
        assert_eq!(stats.total_count, 5);
        assert_eq!(stats.active_count, 3);
        assert_eq!(stats.healthy_count, 4);
        assert_eq!(*stats.by_priority.get(&ActorPriority::Critical).unwrap_or(&0), 2);
        assert_eq!(*stats.by_priority.get(&ActorPriority::Normal).unwrap_or(&0), 3);
        assert_eq!(*stats.by_state.get(&ActorLifecycleState::Active).unwrap_or(&0), 3);
        assert_eq!(*stats.by_state.get(&ActorLifecycleState::Suspended).unwrap_or(&0), 2);
    }
}

#[cfg(test)]
mod registry_cleanup_tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_unregistration() {
        let mut registry = ActorRegistry::development();
        
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            test_tags(&["test"]),
            default_registration_context(),
        ).unwrap();
        
        assert!(registry.contains_actor("test_actor"));
        assert_eq!(registry.len(), 1);
        
        let result = registry.unregister_actor("test_actor");
        assert!(result.is_ok());
        assert!(!registry.contains_actor("test_actor"));
        assert_eq!(registry.len(), 0);
        
        // Test unregistering non-existent actor
        let result = registry.unregister_actor("missing_actor");
        assert!(matches!(result, Err(RegistryError::ActorNotFound(_))));
    }

    #[tokio::test]
    async fn test_batch_unregistration() {
        let mut registry = ActorRegistry::development();
        
        // Register multiple actors
        for i in 0..5 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
        }
        
        assert_eq!(registry.len(), 5);
        
        let names_to_remove = vec![
            "actor_0".to_string(),
            "actor_2".to_string(),
            "actor_4".to_string(),
            "missing_actor".to_string(),
        ];
        
        let result = registry.batch_unregister_actors(names_to_remove, false);
        
        assert_eq!(result.successes.len(), 3);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.success_rate, 0.75);
        assert_eq!(registry.len(), 2);
        
        // Remaining actors should be actor_1 and actor_3
        assert!(registry.contains_actor("actor_1"));
        assert!(registry.contains_actor("actor_3"));
    }

    #[tokio::test]
    async fn test_cleanup_terminated_actors() {
        let mut registry = ActorRegistry::development();
        
        // Register actors and set some to terminated
        for i in 0..5 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
            
            if i % 2 == 0 {
                registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
                registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::ShuttingDown).unwrap();
                registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Terminated).unwrap();
            } else {
                registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
            }
        }
        
        assert_eq!(registry.len(), 5);
        
        let cleaned_count = registry.cleanup_terminated_actors().unwrap();
        assert_eq!(cleaned_count, 3); // actors 0, 2, 4
        assert_eq!(registry.len(), 2); // actors 1, 3 remaining
    }

    #[tokio::test]
    async fn test_cleanup_inactive_actors() {
        let config = ActorRegistryConfig {
            max_inactive_duration: Duration::from_millis(100), // Very short for testing
            ..Default::default()
        };
        let mut registry = ActorRegistry::new(config);
        
        // Register actors
        for i in 0..3 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
            
            // Set different states
            if i == 0 {
                registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
            } else {
                registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Suspended).unwrap();
            }
        }
        
        // Wait for timeout
        sleep(Duration::from_millis(150)).await;
        
        let cleaned_count = registry.cleanup_inactive_actors().unwrap();
        assert_eq!(cleaned_count, 2); // Suspended actors should be cleaned up
        assert_eq!(registry.len(), 1); // Only active actor remains
    }

    #[tokio::test]
    async fn test_registry_maintenance() {
        let mut registry = ActorRegistry::development();
        
        // Register actors with various states
        for i in 0..10 {
            let actor = TestActor::new(format!("actor_{}", i));
            let addr = actor.start();
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                ActorPriority::Normal,
                HashSet::new(),
                default_registration_context(),
            ).unwrap();
            
            match i % 3 {
                0 => {
                    registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
                    registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::ShuttingDown).unwrap();
                    registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Terminated).unwrap();
                }
                1 => {
                    registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
                }
                _ => {
                    registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Suspended).unwrap();
                }
            }
        }
        
        let initial_count = registry.len();
        assert_eq!(initial_count, 10);
        
        let report = registry.perform_maintenance().unwrap();
        
        assert!(report.duration.as_millis() > 0);
        assert_eq!(report.terminated_cleaned, 4); // actors 0, 3, 6, 9
        assert!(report.statistics_updated);
        assert_eq!(registry.len(), 6); // 6 actors remaining
    }
}

#[cfg(test)]
mod thread_safe_registry_tests {
    use super::*;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn test_thread_safe_registry_basic_operations() {
        let registry = ThreadSafeActorRegistry::development();
        
        let actor = TestActor::new("test_actor".to_string());
        let addr = actor.start();
        
        let result = registry.register_actor(
            "test_actor".to_string(),
            addr,
            ActorPriority::Normal,
            HashSet::new(),
            default_registration_context(),
        ).await;
        assert!(result.is_ok());
        
        assert!(registry.contains_actor("test_actor").await);
        assert_eq!(registry.len().await, 1);
        
        let retrieved_addr: Option<Addr<TestActor>> = registry.get_actor("test_actor").await;
        assert!(retrieved_addr.is_some());
        
        let result = registry.unregister_actor("test_actor").await;
        assert!(result.is_ok());
        assert_eq!(registry.len().await, 0);
    }

    #[tokio::test]
    async fn test_concurrent_registry_operations() {
        let registry = Arc::new(ThreadSafeActorRegistry::development());
        let mut join_set = JoinSet::new();
        
        // Spawn multiple tasks to register actors concurrently
        for i in 0..10 {
            let registry_clone = Arc::clone(&registry);
            join_set.spawn(async move {
                let actor = TestActor::new(format!("actor_{}", i));
                let addr = actor.start();
                
                let result = registry_clone.register_actor(
                    format!("actor_{}", i),
                    addr,
                    ActorPriority::Normal,
                    HashSet::new(),
                    default_registration_context(),
                ).await;
                
                result.is_ok()
            });
        }
        
        // Wait for all registrations to complete
        let mut success_count = 0;
        while let Some(result) = join_set.join_next().await {
            if result.unwrap() {
                success_count += 1;
            }
        }
        
        assert_eq!(success_count, 10);
        assert_eq!(registry.len().await, 10);
        
        // Test concurrent lookups
        let mut join_set = JoinSet::new();
        for i in 0..10 {
            let registry_clone = Arc::clone(&registry);
            join_set.spawn(async move {
                let addr: Option<Addr<TestActor>> = registry_clone.get_actor(&format!("actor_{}", i)).await;
                addr.is_some()
            });
        }
        
        let mut found_count = 0;
        while let Some(result) = join_set.join_next().await {
            if result.unwrap() {
                found_count += 1;
            }
        }
        
        assert_eq!(found_count, 10);
    }

    #[tokio::test]
    async fn test_registry_statistics_concurrent() {
        let registry = Arc::new(ThreadSafeActorRegistry::development());
        let mut join_set = JoinSet::new();
        
        // Register actors with different priorities concurrently
        for i in 0..20 {
            let registry_clone = Arc::clone(&registry);
            join_set.spawn(async move {
                let actor = TestActor::new(format!("actor_{}", i));
                let addr = actor.start();
                
                let priority = match i % 4 {
                    0 => ActorPriority::Critical,
                    1 => ActorPriority::High,
                    2 => ActorPriority::Normal,
                    _ => ActorPriority::Low,
                };
                
                registry_clone.register_actor(
                    format!("actor_{}", i),
                    addr,
                    priority,
                    HashSet::new(),
                    default_registration_context(),
                ).await
            });
        }
        
        // Wait for all registrations
        while let Some(_) = join_set.join_next().await {}
        
        let stats = registry.get_statistics().await;
        assert_eq!(stats.total_actors, 20);
        assert_eq!(*stats.actors_by_priority.get(&ActorPriority::Critical).unwrap(), 5);
        assert_eq!(*stats.actors_by_priority.get(&ActorPriority::High).unwrap(), 5);
        assert_eq!(*stats.actors_by_priority.get(&ActorPriority::Normal).unwrap(), 5);
        assert_eq!(*stats.actors_by_priority.get(&ActorPriority::Low).unwrap(), 5);
    }
}