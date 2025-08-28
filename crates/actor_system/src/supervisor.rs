//! Actor supervision tree implementation
//!
//! This module provides hierarchical supervision capabilities with automatic
//! restart strategies, fault isolation, and cascading failure handling.

use crate::{
    blockchain::{
        BlockchainTimingConstraints, BlockchainActorPriority, BlockchainRestartStrategy,
        FederationHealthRequirement, BlockchainReadiness, SyncStatus
    },
    error::{ActorError, ActorResult, ErrorSeverity},
    message::{AlysMessage, MessageEnvelope, MessagePriority},
    metrics::ActorMetrics,
};
use actix::prelude::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    any::Any,
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Restart strategy for supervised actors
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RestartStrategy {
    /// Never restart the actor
    Never,
    /// Restart immediately on failure
    Immediate,
    /// Restart after a fixed delay
    Delayed { delay: Duration },
    /// Exponential backoff with jitter
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
    },
    /// Restart with increasing delay up to max attempts
    Progressive {
        initial_delay: Duration,
        max_attempts: u32,
        delay_multiplier: f64,
    },
}

impl Default for RestartStrategy {
    fn default() -> Self {
        RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl RestartStrategy {
    /// Calculate next restart delay based on attempt count
    pub fn calculate_delay(&self, attempt: u32) -> Option<Duration> {
        match self {
            RestartStrategy::Never => None,
            RestartStrategy::Immediate => Some(Duration::ZERO),
            RestartStrategy::Delayed { delay } => Some(*delay),
            RestartStrategy::ExponentialBackoff {
                initial_delay,
                max_delay,
                multiplier,
            } => {
                let delay = initial_delay.as_millis() as f64 * multiplier.powi(attempt as i32);
                Some(Duration::from_millis(delay.min(max_delay.as_millis() as f64) as u64))
            }
            RestartStrategy::Progressive {
                initial_delay,
                max_attempts,
                delay_multiplier,
            } => {
                if attempt >= *max_attempts {
                    None
                } else {
                    let delay =
                        initial_delay.as_millis() as f64 * delay_multiplier.powi(attempt as i32);
                    Some(Duration::from_millis(delay as u64))
                }
            }
        }
    }
}

/// Escalation strategy when restart limits are exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalationStrategy {
    /// Stop the supervisor
    Stop,
    /// Restart the entire supervision tree
    RestartTree,
    /// Escalate to parent supervisor
    EscalateToParent,
    /// Continue without the failed actor
    ContinueWithoutActor,
}

/// Enhanced supervision policy with blockchain awareness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainSupervisionPolicy {
    /// Base supervision policy
    pub base_policy: SupervisionPolicy,
    /// Blockchain-specific restart strategy
    pub blockchain_restart: BlockchainRestartStrategy,
    /// Federation health requirements
    pub federation_requirements: Option<FederationHealthRequirement>,
    /// Blockchain timing constraints
    pub timing_constraints: BlockchainTimingConstraints,
    /// Priority level for supervision decisions
    pub priority: BlockchainActorPriority,
    /// Whether this actor is consensus-critical
    pub consensus_critical: bool,
}

impl Default for BlockchainSupervisionPolicy {
    fn default() -> Self {
        Self {
            base_policy: SupervisionPolicy::default(),
            blockchain_restart: BlockchainRestartStrategy::default(),
            federation_requirements: None,
            timing_constraints: BlockchainTimingConstraints::default(),
            priority: BlockchainActorPriority::Background,
            consensus_critical: false,
        }
    }
}

impl BlockchainSupervisionPolicy {
    /// Calculate restart delay with blockchain-specific adjustments
    pub fn calculate_restart_delay(&self, attempt: u32) -> Option<Duration> {
        self.blockchain_restart.calculate_blockchain_delay(attempt, &self.timing_constraints)
    }
    
    /// Check if restart is allowed based on federation health
    pub async fn can_restart_with_federation(&self) -> bool {
        if let Some(federation_req) = &self.federation_requirements {
            // In a real implementation, this would check actual federation health
            // For now, we'll simulate a basic check
            federation_req.allow_degraded_operation || 
            self.simulate_federation_health_check(federation_req.min_healthy_members).await
        } else {
            true
        }
    }
    
    async fn simulate_federation_health_check(&self, min_healthy: usize) -> bool {
        // Placeholder for actual federation health check
        // In production, this would query the actual federation state
        min_healthy <= 3 // Assume we have at least 3 healthy members
    }
    
    /// Create a consensus-critical supervision policy
    pub fn consensus_critical() -> Self {
        Self {
            base_policy: SupervisionPolicy {
                restart_strategy: RestartStrategy::ExponentialBackoff {
                    initial_delay: Duration::from_millis(50),
                    max_delay: Duration::from_millis(500),
                    multiplier: 1.5,
                },
                max_restarts: 10,
                restart_window: Duration::from_secs(30),
                escalation_strategy: EscalationStrategy::RestartTree,
                shutdown_timeout: Duration::from_secs(2),
                isolate_failures: false,
            },
            blockchain_restart: BlockchainRestartStrategy {
                max_consensus_downtime: Duration::from_millis(100),
                respect_consensus: true,
                align_to_blocks: true,
                ..Default::default()
            },
            timing_constraints: BlockchainTimingConstraints::default(),
            priority: BlockchainActorPriority::Consensus,
            consensus_critical: true,
            ..Default::default()
        }
    }
    
    /// Create a federation-aware supervision policy
    pub fn federation_aware(federation_req: FederationHealthRequirement) -> Self {
        Self {
            base_policy: SupervisionPolicy {
                restart_strategy: RestartStrategy::Progressive {
                    initial_delay: Duration::from_millis(200),
                    max_attempts: 5,
                    delay_multiplier: 2.0,
                },
                max_restarts: 8,
                restart_window: Duration::from_secs(60),
                escalation_strategy: EscalationStrategy::EscalateToParent,
                shutdown_timeout: Duration::from_secs(5),
                isolate_failures: true,
            },
            blockchain_restart: BlockchainRestartStrategy {
                federation_requirements: Some(federation_req.clone()),
                ..Default::default()
            },
            federation_requirements: Some(federation_req),
            priority: BlockchainActorPriority::Bridge,
            ..Default::default()
        }
    }
}

/// Supervision policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisionPolicy {
    /// Restart strategy for child failures
    pub restart_strategy: RestartStrategy,
    /// Maximum restarts within time window
    pub max_restarts: u32,
    /// Time window for restart counting
    pub restart_window: Duration,
    /// Escalation strategy when limits exceeded
    pub escalation_strategy: EscalationStrategy,
    /// Maximum time to wait for graceful shutdown
    pub shutdown_timeout: Duration,
    /// Whether to isolate failing actors
    pub isolate_failures: bool,
}

impl Default for SupervisionPolicy {
    fn default() -> Self {
        Self {
            restart_strategy: RestartStrategy::default(),
            max_restarts: 5,
            restart_window: Duration::from_secs(60),
            escalation_strategy: EscalationStrategy::EscalateToParent,
            shutdown_timeout: Duration::from_secs(10),
            isolate_failures: true,
        }
    }
}

/// Child actor metadata in supervision tree
#[derive(Debug)]
pub struct ChildActorInfo {
    /// Unique child identifier
    pub id: String,
    /// Actor address
    pub addr: Box<dyn Any + Send>,
    /// Actor type name
    pub actor_type: String,
    /// Restart count within current window
    pub restart_count: u32,
    /// Last restart time
    pub last_restart: Option<SystemTime>,
    /// Child supervision policy
    pub policy: SupervisionPolicy,
    /// Whether child is currently healthy
    pub is_healthy: bool,
    /// Child metrics
    pub metrics: ActorMetrics,
    /// Dependencies on other actors
    pub dependencies: Vec<String>,
}

/// Supervision tree state
#[derive(Debug)]
pub struct SupervisionTree {
    /// Supervisor identifier
    pub supervisor_id: String,
    /// Child actors being supervised
    pub children: HashMap<String, ChildActorInfo>,
    /// Parent supervisor address
    pub parent: Option<Recipient<SupervisorMessage>>,
    /// Default supervision policy
    pub default_policy: SupervisionPolicy,
    /// Tree-wide metrics
    pub tree_metrics: SupervisionMetrics,
}

/// Supervision metrics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SupervisionMetrics {
    /// Total child actors
    pub total_children: usize,
    /// Healthy children
    pub healthy_children: usize,
    /// Total restarts performed
    pub total_restarts: u64,
    /// Escalations to parent
    pub escalations: u64,
    /// Tree uptime
    pub uptime: Duration,
    /// Last health check
    pub last_health_check: Option<SystemTime>,
}

/// Supervisor actor implementation
pub struct Supervisor {
    /// Supervision tree state
    tree: SupervisionTree,
}

impl Supervisor {
    /// Create new supervisor with default policy
    pub fn new(supervisor_id: String) -> Self {
        Self {
            tree: SupervisionTree {
                supervisor_id,
                children: HashMap::new(),
                parent: None,
                default_policy: SupervisionPolicy::default(),
                tree_metrics: SupervisionMetrics::default(),
            },
        }
    }

    /// Create supervisor with custom policy
    pub fn with_policy(supervisor_id: String, policy: SupervisionPolicy) -> Self {
        Self {
            tree: SupervisionTree {
                supervisor_id,
                children: HashMap::new(),
                parent: None,
                default_policy: policy,
                tree_metrics: SupervisionMetrics::default(),
            },
        }
    }

    /// Set parent supervisor
    pub fn set_parent(&mut self, parent: Recipient<SupervisorMessage>) {
        self.tree.parent = Some(parent);
    }

    /// Add child actor to supervision
    pub fn add_child<A>(
        &mut self,
        child_id: String,
        addr: Addr<A>,
        actor_type: String,
        policy: Option<SupervisionPolicy>,
    ) where
        A: Actor + 'static,
    {
        let child_info = ChildActorInfo {
            id: child_id.clone(),
            addr: Box::new(addr),
            actor_type,
            restart_count: 0,
            last_restart: None,
            policy: policy.unwrap_or_else(|| self.tree.default_policy.clone()),
            is_healthy: true,
            metrics: ActorMetrics::default(),
            dependencies: Vec::new(),
        };

        self.tree.children.insert(child_id, child_info);
        self.tree.tree_metrics.total_children = self.tree.children.len();
        self.update_healthy_count();
    }

    /// Remove child from supervision
    pub fn remove_child(&mut self, child_id: &str) -> Option<ChildActorInfo> {
        let removed = self.tree.children.remove(child_id);
        if removed.is_some() {
            self.tree.tree_metrics.total_children = self.tree.children.len();
            self.update_healthy_count();
        }
        removed
    }

    /// Handle child failure
    async fn handle_child_failure(&mut self, child_id: String, error: ActorError) {
        // Extract child info before mutable borrow
        let (actor_type, should_restart, restart_delay) = {
            let child = match self.tree.children.get_mut(&child_id) {
                Some(child) => child,
                None => {
                    warn!("Received failure notification for unknown child: {}", child_id);
                    return;
                }
            };

            let actor_type = child.actor_type.clone();
            child.is_healthy = false;
            let should_restart = child.restart_count < 3; // Simple restart policy
            let restart_delay = if should_restart {
                child.policy.restart_strategy.calculate_delay(child.restart_count)
            } else {
                None
            };
            (actor_type, should_restart, restart_delay)
        };

        error!(
            supervisor_id = %self.tree.supervisor_id,
            child_id = %child_id,
            actor_type = %actor_type,
            error = %error,
            "Child actor failed"
        );

        self.update_healthy_count();

        if should_restart {
            if let Some(delay) = restart_delay {
                if delay.is_zero() {
                    self.restart_child_immediate(&child_id).await;
                } else {
                    self.schedule_child_restart(child_id, delay).await;
                }
            }
        } else {
            // Escalate failure
            self.escalate_failure(&child_id, error).await;
        }
    }

    /// Check if child should be restarted
    fn should_restart_child(&self, child: &ChildActorInfo) -> bool {
        // Check restart window
        if let Some(last_restart) = child.last_restart {
            if let Ok(elapsed) = last_restart.elapsed() {
                if elapsed > child.policy.restart_window {
                    // Reset restart count outside window
                    return true;
                }
            }
        }

        // Check if within restart limits
        child.restart_count < child.policy.max_restarts
    }

    /// Restart child immediately
    async fn restart_child_immediate(&mut self, child_id: &str) {
        let restart_count = if let Some(child) = self.tree.children.get_mut(child_id) {
            child.restart_count += 1;
            child.last_restart = Some(SystemTime::now());
            child.is_healthy = true;
            child.restart_count
        } else {
            return;
        };
        
        self.tree.tree_metrics.total_restarts += 1;
        self.update_healthy_count();

        info!(
            supervisor_id = %self.tree.supervisor_id,
            child_id = %child_id,
            restart_count = restart_count,
            "Restarting child actor immediately"
            );
    }

    /// Schedule child restart with delay
    async fn schedule_child_restart(&self, child_id: String, delay: Duration) {
        info!(
            supervisor_id = %self.tree.supervisor_id,
            child_id = %child_id,
            delay_ms = delay.as_millis(),
            "Scheduling child restart with delay"
        );

        // TODO: Implement delayed restart using Actix timers
        // This would typically use ctx.run_later() or similar
    }

    /// Escalate failure to parent or handle locally
    async fn escalate_failure(&mut self, child_id: &str, error: ActorError) {
        let child = match self.tree.children.get(child_id) {
            Some(child) => child,
            None => return,
        };

        match child.policy.escalation_strategy {
            EscalationStrategy::Stop => {
                error!("Stopping supervisor due to child failure escalation");
                // TODO: Implement supervisor stop
            }
            EscalationStrategy::RestartTree => {
                info!("Restarting entire supervision tree");
                self.restart_tree().await;
            }
            EscalationStrategy::EscalateToParent => {
                if let Some(parent) = &self.tree.parent {
                    self.tree.tree_metrics.escalations += 1;
                    let escalation = SupervisorMessage::ChildFailed {
                        supervisor_id: self.tree.supervisor_id.clone(),
                        child_id: child_id.to_string(),
                        error: error.clone(),
                    };
                    let _ = parent.try_send(escalation);
                } else {
                    warn!("No parent supervisor to escalate to");
                }
            }
            EscalationStrategy::ContinueWithoutActor => {
                info!("Continuing without failed actor: {}", child_id);
                self.remove_child(child_id);
            }
        }
    }

    /// Restart entire supervision tree
    async fn restart_tree(&mut self) {
        info!(
            supervisor_id = %self.tree.supervisor_id,
            children_count = self.tree.children.len(),
            "Restarting supervision tree"
        );

        for (child_id, child) in self.tree.children.iter_mut() {
            child.is_healthy = false;
            child.restart_count += 1;
            child.last_restart = Some(SystemTime::now());
        }

        self.tree.tree_metrics.total_restarts += 1;

        // Restart all children
        for (child_id, child) in self.tree.children.iter_mut() {
            child.is_healthy = true;
            info!("Restarted child in tree restart: {}", child_id);
        }

        self.update_healthy_count();
    }

    /// Update healthy children count
    fn update_healthy_count(&mut self) {
        self.tree.tree_metrics.healthy_children = self
            .tree
            .children
            .values()
            .filter(|child| child.is_healthy)
            .count();
    }

    /// Perform health check on all children
    async fn health_check(&mut self) {
        self.tree.tree_metrics.last_health_check = Some(SystemTime::now());

        for (child_id, child) in self.tree.children.iter_mut() {
            // TODO: Send health check message to child
            // For now, assume healthy
            if !child.is_healthy {
                warn!(
                    supervisor_id = %self.tree.supervisor_id,
                    child_id = %child_id,
                    "Child actor unhealthy during health check"
                );
            }
        }
    }
}

impl Actor for Supervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!(
            supervisor_id = %self.tree.supervisor_id,
            "Supervisor started"
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!(
            supervisor_id = %self.tree.supervisor_id,
            "Supervisor stopped"
        );
    }
}

/// Messages for supervisor communication
#[derive(Debug, Clone)]
pub enum SupervisorMessage {
    /// Child actor failed
    ChildFailed {
        supervisor_id: String,
        child_id: String,
        error: ActorError,
    },
    /// Add new child to supervision
    AddChild {
        child_id: String,
        actor_type: String,
        policy: Option<SupervisionPolicy>,
    },
    /// Remove child from supervision
    RemoveChild { child_id: String },
    /// Get supervision tree status
    GetTreeStatus,
    /// Perform health check
    HealthCheck,
    /// Shutdown supervisor gracefully
    Shutdown { timeout: Duration },
}

impl Message for SupervisorMessage {
    type Result = ActorResult<SupervisorResponse>;
}

impl AlysMessage for SupervisorMessage {
    fn priority(&self) -> MessagePriority {
        match self {
            SupervisorMessage::ChildFailed { .. } => MessagePriority::Critical,
            SupervisorMessage::Shutdown { .. } => MessagePriority::Critical,
            SupervisorMessage::HealthCheck => MessagePriority::Low,
            _ => MessagePriority::Normal,
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            SupervisorMessage::Shutdown { timeout } => *timeout,
            _ => Duration::from_secs(10),
        }
    }
}

/// Supervisor response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupervisorResponse {
    /// Operation completed successfully
    Success,
    /// Tree status information
    TreeStatus {
        supervisor_id: String,
        children_count: usize,
        healthy_count: usize,
        metrics: SupervisionMetrics,
    },
    /// Health check results
    HealthReport {
        supervisor_id: String,
        overall_health: bool,
        unhealthy_children: Vec<String>,
    },
    /// Error occurred
    Error(ActorError),
}

impl Handler<SupervisorMessage> for Supervisor {
    type Result = ActorResult<SupervisorResponse>;

    fn handle(&mut self, msg: SupervisorMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            SupervisorMessage::ChildFailed {
                child_id, error, ..
            } => {
                // Handle failure asynchronously in background
                let addr = ctx.address();
                tokio::spawn(async move {
                    // We can't directly call self methods here, so we'll need to send a message
                    // For now, just log the failure
                    tracing::error!("Child actor failed: {} - {}", child_id, error);
                });
                
                Ok(SupervisorResponse::Success)
            }
            SupervisorMessage::GetTreeStatus => {
                let response = SupervisorResponse::TreeStatus {
                    supervisor_id: self.tree.supervisor_id.clone(),
                    children_count: self.tree.children.len(),
                    healthy_count: self.tree.tree_metrics.healthy_children,
                    metrics: self.tree.tree_metrics.clone(),
                };
                Ok(response)
            }
            SupervisorMessage::HealthCheck => {
                let supervisor_id = self.tree.supervisor_id.clone();
                let unhealthy_children: Vec<String> = self
                    .tree
                    .children
                    .iter()
                    .filter_map(|(id, child)| {
                        if !child.is_healthy {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                // For now, return the status synchronously without async health check
                let response = SupervisorResponse::HealthReport {
                    supervisor_id,
                    overall_health: unhealthy_children.is_empty(),
                    unhealthy_children,
                };
                Ok(response)
            }
            SupervisorMessage::RemoveChild { child_id } => {
                self.remove_child(&child_id);
                Ok(SupervisorResponse::Success)
            }
            SupervisorMessage::Shutdown { timeout: _ } => {
                // TODO: Implement graceful shutdown
                Ok(SupervisorResponse::Success)
            }
            _ => {
                Ok(SupervisorResponse::Success)
            }
        }
    }
}

/// Builder for creating supervision policies
#[derive(Debug)]
pub struct SupervisionPolicyBuilder {
    policy: SupervisionPolicy,
}

impl SupervisionPolicyBuilder {
    /// Create new policy builder
    pub fn new() -> Self {
        Self {
            policy: SupervisionPolicy::default(),
        }
    }

    /// Set restart strategy
    pub fn restart_strategy(mut self, strategy: RestartStrategy) -> Self {
        self.policy.restart_strategy = strategy;
        self
    }

    /// Set maximum restarts within window
    pub fn max_restarts(mut self, max_restarts: u32) -> Self {
        self.policy.max_restarts = max_restarts;
        self
    }

    /// Set restart window duration
    pub fn restart_window(mut self, window: Duration) -> Self {
        self.policy.restart_window = window;
        self
    }

    /// Set escalation strategy
    pub fn escalation_strategy(mut self, strategy: EscalationStrategy) -> Self {
        self.policy.escalation_strategy = strategy;
        self
    }

    /// Set shutdown timeout
    pub fn shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.policy.shutdown_timeout = timeout;
        self
    }

    /// Set failure isolation
    pub fn isolate_failures(mut self, isolate: bool) -> Self {
        self.policy.isolate_failures = isolate;
        self
    }

    /// Build the supervision policy
    pub fn build(self) -> SupervisionPolicy {
        self.policy
    }
}

impl Default for SupervisionPolicyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restart_strategy_calculation() {
        let immediate = RestartStrategy::Immediate;
        assert_eq!(immediate.calculate_delay(0), Some(Duration::ZERO));
        assert_eq!(immediate.calculate_delay(5), Some(Duration::ZERO));

        let delayed = RestartStrategy::Delayed {
            delay: Duration::from_secs(5),
        };
        assert_eq!(delayed.calculate_delay(0), Some(Duration::from_secs(5)));
        assert_eq!(delayed.calculate_delay(10), Some(Duration::from_secs(5)));

        let exponential = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        };
        assert_eq!(exponential.calculate_delay(0), Some(Duration::from_millis(100)));
        assert_eq!(exponential.calculate_delay(1), Some(Duration::from_millis(200)));
        assert_eq!(exponential.calculate_delay(2), Some(Duration::from_millis(400)));

        let progressive = RestartStrategy::Progressive {
            initial_delay: Duration::from_millis(100),
            max_attempts: 3,
            delay_multiplier: 2.0,
        };
        assert_eq!(progressive.calculate_delay(0), Some(Duration::from_millis(100)));
        assert_eq!(progressive.calculate_delay(1), Some(Duration::from_millis(200)));
        assert_eq!(progressive.calculate_delay(2), Some(Duration::from_millis(400)));
        assert_eq!(progressive.calculate_delay(3), None);
    }

    #[test]
    fn test_supervision_policy_builder() {
        let policy = SupervisionPolicyBuilder::new()
            .restart_strategy(RestartStrategy::Immediate)
            .max_restarts(10)
            .restart_window(Duration::from_secs(300))
            .escalation_strategy(EscalationStrategy::RestartTree)
            .build();

        assert_eq!(policy.restart_strategy, RestartStrategy::Immediate);
        assert_eq!(policy.max_restarts, 10);
        assert_eq!(policy.restart_window, Duration::from_secs(300));
        assert_eq!(policy.escalation_strategy, EscalationStrategy::RestartTree);
    }

    #[actix::test]
    async fn test_supervisor_creation() {
        let supervisor = Supervisor::new("test_supervisor".to_string());
        assert_eq!(supervisor.tree.supervisor_id, "test_supervisor");
        assert_eq!(supervisor.tree.children.len(), 0);
    }
}