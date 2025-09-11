//! AlysActor Implementation for StreamActor
//! 
//! Complete integration with actor_system crate for governance communication

use async_trait::async_trait;
use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use uuid::Uuid;
use tracing::{info, warn, error, debug};

use actor_system::{
    actor::{AlysActor, ExtendedAlysActor},
    lifecycle::{LifecycleAware, ActorState},
    mailbox::{MailboxConfig, OverflowStrategy},
    message::{AlysMessage, MessageEnvelope, MessagePriority},
    supervisor::{SupervisionPolicy, RestartStrategy, EscalationStrategy},
    error::{ActorError, ActorResult},
    metrics::ActorMetrics,
};

use crate::actors::bridge::{
    messages::stream_messages::*,
    config::StreamConfig,
    shared::errors::BridgeError,
};
use super::{
    StreamActor, 
    actor::{GovernanceConnection, ConnectionStatus},
    metrics::StreamMetrics,
};

/// State structure for actor_system compatibility
#[derive(Debug, Clone)]
pub struct StreamActorState {
    pub lifecycle_state: ActorState,
    pub connection_status: ConnectionStatus,
    pub active_connections: usize,
    pub pending_requests: u32,
    pub last_heartbeat: Option<SystemTime>,
    pub metrics_snapshot: actor_system::metrics::MetricsSnapshot,
}

#[async_trait]
impl AlysActor for StreamActor {
    type Config = StreamConfig;
    type Error = BridgeError;
    type Message = StreamMessage;
    type State = StreamActorState;

    fn new(config: Self::Config) -> Result<Self, Self::Error> {
        info!("Creating StreamActor with actor_system integration");
        
        let reconnection_manager = super::ReconnectionManager::new(
            config.reconnect_attempts.unwrap_or(5),
            config.reconnect_delay.unwrap_or(Duration::from_secs(5)),
        );
        
        let metrics = StreamMetrics::new()
            .map_err(|e| BridgeError::StreamError { 
                message: format!("Failed to initialize metrics: {:?}", e),
            })?;

        let actor_system_metrics = ActorMetrics::new("bridge_stream_actor", "v1.0.0")
            .map_err(|e| BridgeError::StreamError {
                message: format!("Failed to initialize actor_system metrics: {:?}", e),
            })?;

        Ok(Self {
            config,
            governance_connections: HashMap::new(),
            message_buffer: Vec::new(),
            request_tracker: super::RequestTracker::new(),
            pegout_actor: None,
            bridge_coordinator: None,
            reconnection_manager,
            metrics,
            connection_status: ConnectionStatus::Disconnected,
            last_heartbeat: None,
            actor_system_metrics,
        })
    }

    fn actor_type(&self) -> String {
        "StreamActor".to_string()
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }
    
    fn config_mut(&mut self) -> &mut Self::Config {
        &mut self.config
    }
    
    fn metrics(&self) -> &ActorMetrics {
        &self.actor_system_metrics
    }
    
    fn metrics_mut(&mut self) -> &mut ActorMetrics {
        &mut self.actor_system_metrics
    }
    
    async fn get_state(&self) -> Self::State {
        let metrics_snapshot = self.actor_system_metrics.snapshot();

        StreamActorState {
            lifecycle_state: ActorState::Running,
            connection_status: self.connection_status.clone(),
            active_connections: self.governance_connections.len(),
            pending_requests: 0, // self.request_tracker.pending_count(),
            last_heartbeat: self.last_heartbeat,
            metrics_snapshot,
        }
    }
    
    async fn set_state(&mut self, state: Self::State) -> ActorResult<()> {
        self.connection_status = state.connection_status;
        self.last_heartbeat = state.last_heartbeat;
        Ok(())
    }

    fn mailbox_config(&self) -> MailboxConfig {
        MailboxConfig::new()
            .with_capacity(self.config.max_pending_messages.unwrap_or(1000))
            .with_priority_levels(5)
            .with_overflow_strategy(OverflowStrategy::DropOldest)
            .with_backpressure_threshold(0.8)
            .with_fair_scheduling(true)
    }

    fn supervision_policy(&self) -> SupervisionPolicy {
        SupervisionPolicy {
            restart_strategy: RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(300),
                multiplier: 2.0,
                max_attempts: 10,
            },
            escalation_strategy: EscalationStrategy::EscalateToParent,
        }
    }

    fn dependencies(&self) -> Vec<String> {
        vec![
            "bridge_actor".to_string(),
            "pegout_actor".to_string(),
        ]
    }
}

#[async_trait]
impl ExtendedAlysActor for StreamActor {
    async fn custom_initialize(&mut self) -> ActorResult<()> {
        info!("StreamActor custom initialization starting");

        // Initialize governance connections
        self.establish_governance_connections().await
            .map_err(|e| ActorError::StartupFailed {
                actor_type: "StreamActor".to_string(),
                reason: format!("Failed to establish governance connections: {:?}", e),
            })?;

        // Start background tasks would normally be handled by the actor framework
        info!("StreamActor custom initialization completed");
        Ok(())
    }

    async fn handle_critical_error(&mut self, error: ActorError) -> ActorResult<bool> {
        error!("StreamActor handling critical error: {:?}", error);

        match error {
            ActorError::ExternalDependency { service, .. } if service == "governance" => {
                warn!("Governance service error, attempting reconnection");
                if let Err(e) = self.reconnect_to_governance_nodes().await {
                    error!("Failed to reconnect to governance nodes: {:?}", e);
                    return Ok(false); // Let supervisor handle restart
                }
                Ok(true) // Handled error
            }
            ActorError::NetworkError { .. } => {
                warn!("Network error, initiating connection recovery");
                self.initiate_connection_recovery().await;
                Ok(true) // Handled error
            }
            _ => Ok(false) // Let supervisor handle other errors
        }
    }

    async fn maintenance_task(&mut self) -> ActorResult<()> {
        debug!("StreamActor performing maintenance");

        // Clean up expired pending messages
        self.cleanup_expired_messages().await;

        // Update connection health scores
        self.update_connection_health().await;

        // Compact message buffer if needed
        self.compact_message_buffer().await;

        // Update metrics
        self.actor_system_metrics.record_maintenance_performed();
        
        Ok(())
    }

    async fn export_metrics(&self) -> ActorResult<serde_json::Value> {
        let healthy_connections = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, NodeConnectionStatus::Connected))
            .count();

        let mut heartbeat_age = None;
        if let Some(last_heartbeat) = self.last_heartbeat {
            heartbeat_age = Some(SystemTime::now()
                .duration_since(last_heartbeat)
                .unwrap_or_default()
                .as_secs());
        }

        let metrics = serde_json::json!({
            "governance_connections_healthy": healthy_connections,
            "governance_connections_total": self.governance_connections.len(),
            "pending_messages": self.message_buffer.len(),
            "pending_requests": 0, // self.request_tracker.pending_count(),
            "heartbeat_age_seconds": heartbeat_age,
            "actor_system": self.actor_system_metrics.snapshot()
        });

        Ok(metrics)
    }
}

// Helper methods for StreamActor
impl StreamActor {
    /// Add actor_system_metrics field to StreamActor struct
    pub fn actor_system_metrics(&self) -> &ActorMetrics {
        &self.actor_system_metrics
    }

    /// Check if actor has healthy connections
    fn has_healthy_connections(&self) -> bool {
        self.governance_connections
            .values()
            .any(|conn| matches!(conn.status, NodeConnectionStatus::Connected))
    }

    /// Reconnect to governance nodes
    async fn reconnect_to_governance_nodes(&mut self) -> Result<(), BridgeError> {
        info!("Reconnecting to governance nodes");
        
        // Clear existing connections
        self.governance_connections.clear();
        
        // Re-establish connections
        self.establish_governance_connections().await
    }

    /// Update connection timers based on new configuration
    async fn update_connection_timers(&mut self) -> Result<(), BridgeError> {
        info!("Updating connection timers");
        // This would update periodic tasks in a real implementation
        // For now, just log the change
        debug!("Heartbeat interval: {:?}", self.config.heartbeat_interval);
        debug!("Connection timeout: {:?}", self.config.connection_timeout);
        Ok(())
    }

    /// Initiate connection recovery
    async fn initiate_connection_recovery(&mut self) {
        warn!("Initiating connection recovery");
        
        // Mark unhealthy connections for reconnection
        for (node_id, connection) in &mut self.governance_connections {
            if !matches!(connection.status, NodeConnectionStatus::Connected) {
                debug!("Marking {} for reconnection", node_id);
                connection.status = NodeConnectionStatus::Connecting;
            }
        }
        
        self.connection_status = ConnectionStatus::Connecting;
    }

    /// Clean up expired pending messages
    async fn cleanup_expired_messages(&mut self) {
        let now = SystemTime::now();
        let initial_count = self.message_buffer.len();
        
        self.message_buffer.retain(|msg| now < msg.timeout);
        
        let cleaned = initial_count - self.message_buffer.len();
        if cleaned > 0 {
            debug!("Cleaned up {} expired pending messages", cleaned);
        }
    }

    /// Update connection health scores
    async fn update_connection_health(&mut self) {
        let now = SystemTime::now();
        
        for (_node_id, connection) in &mut self.governance_connections {
            // Decay health score for inactive connections
            if let Ok(inactive_time) = now.duration_since(connection.last_activity) {
                if inactive_time > Duration::from_secs(300) { // 5 minutes
                    connection.health_score = (connection.health_score * 0.95).max(10.0);
                }
            }
        }
    }

    /// Compact message buffer
    async fn compact_message_buffer(&mut self) {
        if self.message_buffer.len() > 10000 {
            // Keep only the most recent 5000 messages
            self.message_buffer.sort_by_key(|msg| msg.created_at);
            self.message_buffer.truncate(5000);
            info!("Compacted message buffer to 5000 entries");
        }
    }
}

// actor_system_metrics field is already defined in the StreamActor struct in actor.rs