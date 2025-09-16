//! Supervision module for actor fault tolerance and recovery
//!
//! This module provides core supervision functionality including restart strategies,
//! supervision decisions, and supervisor behavior patterns.

use crate::error::{ActorError, ActorResult};
use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub use crate::supervisor::{RestartStrategy, SupervisionConfig};
pub use crate::supervision_tests::SupervisionStrategy;

/// Supervision decision for handling actor failures
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupervisionDecision {
    /// Resume the actor without restarting
    Resume,
    /// Restart the actor
    Restart,
    /// Stop the actor permanently
    Stop,
    /// Escalate to parent supervisor
    Escalate,
}

/// Supervisor strategy for handling child actor failures
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupervisorStrategy {
    /// Only restart the failed actor
    OneForOne,
    /// Restart all child actors when one fails
    OneForAll,
    /// Restart actors in sequence after the failed one
    RestOfChain,
}

impl Default for SupervisorStrategy {
    fn default() -> Self {
        SupervisorStrategy::OneForOne
    }
}

/// Supervision context providing information about failures
#[derive(Debug, Clone)]
pub struct SupervisionContext {
    /// The actor that failed
    pub actor_id: String,
    /// The error that caused the failure
    pub error: ActorError,
    /// Number of previous restarts
    pub restart_count: u32,
    /// Time since last restart
    pub time_since_last_restart: Duration,
    /// Whether this is a critical actor
    pub is_critical: bool,
}

/// Trait for customizable supervision policies
pub trait SupervisionPolicy: Send + Sync {
    /// Decide what action to take for a failed actor
    fn decide(&self, context: &SupervisionContext) -> SupervisionDecision;
    
    /// Whether to restart children when supervisor restarts
    fn restart_children(&self) -> bool {
        true
    }
    
    /// Maximum restart attempts within time window
    fn max_restarts(&self) -> u32 {
        5
    }
    
    /// Time window for restart counting
    fn restart_window(&self) -> Duration {
        Duration::from_secs(60)
    }
}

/// Default supervision policy implementation
#[derive(Debug, Clone)]
pub struct DefaultSupervisionPolicy {
    pub strategy: SupervisorStrategy,
    pub max_restarts: u32,
    pub restart_window: Duration,
}

impl Default for DefaultSupervisionPolicy {
    fn default() -> Self {
        Self {
            strategy: SupervisorStrategy::OneForOne,
            max_restarts: 5,
            restart_window: Duration::from_secs(60),
        }
    }
}

impl SupervisionPolicy for DefaultSupervisionPolicy {
    fn decide(&self, context: &SupervisionContext) -> SupervisionDecision {
        if context.restart_count >= self.max_restarts {
            if context.is_critical {
                SupervisionDecision::Escalate
            } else {
                SupervisionDecision::Stop
            }
        } else {
            match &context.error {
                ActorError::SystemFailure { .. } => SupervisionDecision::Restart,
                ActorError::MessageHandlingFailed { .. } => SupervisionDecision::Resume,
                ActorError::StartupFailed { .. } | ActorError::ShutdownFailed { .. } => SupervisionDecision::Stop,
                _ => SupervisionDecision::Restart,
            }
        }
    }
    
    fn max_restarts(&self) -> u32 {
        self.max_restarts
    }
    
    fn restart_window(&self) -> Duration {
        self.restart_window
    }
}

/// Supervision directive message
#[derive(Debug, Clone, Message)]
#[rtype(result = "SupervisionDecision")]
pub struct SupervisionDirective {
    pub context: SupervisionContext,
}

/// Actor supervision capabilities
#[async_trait::async_trait]
pub trait Supervised {
    /// Handle supervision directive
    async fn supervise(&self, directive: SupervisionDirective) -> SupervisionDecision;
    
    /// Get supervision policy for this actor
    fn supervision_policy(&self) -> Box<dyn SupervisionPolicy> {
        Box::new(DefaultSupervisionPolicy::default())
    }
    
    /// Whether this actor is critical to system operation
    fn is_critical(&self) -> bool {
        false
    }
    
    /// Custom restart logic
    async fn on_restart(&mut self) -> ActorResult<()> {
        Ok(())
    }
    
    /// Custom stop logic
    async fn on_stop(&mut self) -> ActorResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_supervision_decision() {
        let policy = DefaultSupervisionPolicy::default();
        let context = SupervisionContext {
            actor_id: "test".to_string(),
            error: ActorError::MessageHandlingFailed { 
                message_type: "test".to_string(),
                reason: "test error".to_string(),
            },
            restart_count: 0,
            time_since_last_restart: Duration::from_secs(0),
            is_critical: false,
        };
        
        let decision = policy.decide(&context);
        assert_eq!(decision, SupervisionDecision::Resume);
    }
    
    #[test]
    fn test_max_restarts() {
        let policy = DefaultSupervisionPolicy::default();
        let context = SupervisionContext {
            actor_id: "test".to_string(),
            error: ActorError::SystemFailure { 
                reason: "test error".to_string(),
            },
            restart_count: 10,
            time_since_last_restart: Duration::from_secs(0),
            is_critical: false,
        };
        
        let decision = policy.decide(&context);
        assert_eq!(decision, SupervisionDecision::Stop);
    }
}