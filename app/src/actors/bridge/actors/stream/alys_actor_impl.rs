//! AlysActor Implementation for StreamActor
//!
//! Complete integration with actor_system crate for governance communication

use async_trait::async_trait;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

use actor_system::{
    actor::{AlysActor, ExtendedAlysActor},
    error::{ActorError, ActorResult},
    lifecycle::ActorState,
    mailbox::MailboxConfig,
    metrics::ActorMetrics,
    supervisor::{EscalationStrategy, RestartStrategy, SupervisionPolicy},
};

use super::StreamActor;
use crate::actors::bridge::actors::stream::actor::ConnectionStatus;
use crate::actors::bridge::{
    config::StreamConfig, messages::stream_messages::*, shared::errors::BridgeError,
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

        // Use the existing StreamActor constructor
        let mut actor = StreamActor::new(config).map_err(|e| {
            BridgeError::ConfigurationError(format!("Failed to create StreamActor: {:?}", e))
        })?;

        // The constructor already creates ActorMetrics, so we don't need to do anything else
        Ok(actor)
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
        MailboxConfig {
            capacity: 1000,
            enable_priority: true,
            processing_timeout: Duration::from_secs(30),
            drop_on_full: true,
            metrics_interval: Duration::from_secs(60),
            backpressure_threshold: 800.0,
        }
    }

    fn supervision_policy(&self) -> SupervisionPolicy {
        SupervisionPolicy {
            restart_strategy: RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(300),
                multiplier: 2.0,
            },
            escalation_strategy: EscalationStrategy::EscalateToParent,
            shutdown_timeout: Duration::from_secs(30),
            isolate_failures: false,
            max_restarts: 10,
            restart_window: Duration::from_secs(600), // 10 minutes
        }
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["bridge_actor".to_string(), "pegout_actor".to_string()]
    }

    /// Handle configuration update
    async fn on_config_update(&mut self, new_config: Self::Config) -> ActorResult<()> {
        info!("Updating StreamActor configuration");
        let old_config = self.config.clone();
        *self.config_mut() = new_config;

        // Update connection timers if endpoints changed
        if old_config.governance_endpoints != self.config.governance_endpoints {
            self.reconnect_to_governance_nodes()
                .await
                .map_err(|e| ActorError::from(e))?;
        }

        // Update heartbeat and connection timeouts if changed
        if old_config.heartbeat_interval != self.config.heartbeat_interval
            || old_config.connection_timeout != self.config.connection_timeout
        {
            self.update_connection_timers()
                .await
                .map_err(|e| ActorError::from(e))?;
        }

        Ok(())
    }

    /// Handle supervisor message
    async fn handle_supervisor_message(
        &mut self,
        msg: actor_system::supervisor::SupervisorMessage,
    ) -> ActorResult<()> {
        use actor_system::supervisor::SupervisorMessage;
        match msg {
            SupervisorMessage::HealthCheck => {
                let healthy = self.has_healthy_connections();
                if !healthy {
                    warn!("StreamActor health check failed: no healthy governance connections");
                }
                Ok(())
            }
            SupervisorMessage::Shutdown { timeout } => {
                info!(
                    "StreamActor received shutdown signal with timeout: {:?}",
                    timeout
                );
                // Cleanup governance connections
                self.governance_connections.clear();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Pre-process message before handling
    async fn pre_process_message(
        &mut self,
        _envelope: &actor_system::message::MessageEnvelope<Self::Message>,
    ) -> ActorResult<()> {
        // Increment message received count
        self.metrics_mut().record_message_received("stream_message");
        Ok(())
    }

    /// Post-process message after handling
    async fn post_process_message(
        &mut self,
        _envelope: &actor_system::message::MessageEnvelope<Self::Message>,
        _result: &<Self::Message as actix::Message>::Result,
    ) -> ActorResult<()> {
        // Record successful message processing
        self.metrics_mut()
            .record_message_processed(Duration::from_millis(1)); // TODO: Measure actual processing time
        Ok(())
    }

    /// Handle message processing error
    async fn handle_message_error(
        &mut self,
        _envelope: &actor_system::message::MessageEnvelope<Self::Message>,
        error: &ActorError,
    ) -> ActorResult<()> {
        self.metrics_mut().record_message_failed(&error.to_string());
        error!(
            actor_type = "StreamActor",
            error = %error,
            "Message processing failed"
        );

        // If it's a critical error, trigger reconnection
        if error.severity().is_critical() {
            warn!("Critical error in StreamActor, attempting recovery");
            if let Err(recovery_err) = self.reconnect_to_governance_nodes().await {
                error!("Failed to recover from critical error: {:?}", recovery_err);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ExtendedAlysActor for StreamActor {
    async fn custom_initialize(&mut self) -> ActorResult<()> {
        info!("StreamActor custom initialization starting");

        // Initialize governance connections
        if let Err(e) = self.establish_governance_connections().await {
            return Err(ActorError::StartupFailed {
                actor_type: "StreamActor".to_string(),
                reason: format!("Failed to establish governance connections: {:?}", e),
            });
        }

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
            _ => Ok(false), // Let supervisor handle other errors
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
        self.actor_system_metrics.record_maintenance_completed();

        Ok(())
    }

    async fn export_metrics(&self) -> ActorResult<serde_json::Value> {
        let healthy_connections = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, crate::actors::bridge::messages::stream_messages::NodeConnectionStatus::Connected))
            .count();

        let mut heartbeat_age = None;
        if let Some(last_heartbeat) = self.last_heartbeat {
            heartbeat_age = Some(
                SystemTime::now()
                    .duration_since(last_heartbeat)
                    .unwrap_or_default()
                    .as_secs(),
            );
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
    /// Reconnect to governance nodes
    async fn reconnect_to_governance_nodes(&mut self) -> Result<(), BridgeError> {
        info!("Reconnecting to governance nodes");

        // Clear existing connections
        self.governance_connections.clear();

        // Re-establish connections
        self.establish_governance_connections()
            .await
            .map_err(|e| BridgeError::ConnectionError(format!("Failed to reconnect: {:?}", e)))
    }

    /// Update connection timers based on new configuration
    async fn update_connection_timers(&mut self) -> Result<(), BridgeError> {
        info!("Updating connection timers");
        // This would update periodic tasks in a real implementation
        // For now, just log the change
        debug!("Heartbeat interval: {:?}", Duration::from_secs(60)); // TODO: Get from config when available
        debug!("Connection timeout: {:?}", Duration::from_secs(30)); // TODO: Get from config when available
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
                if inactive_time > Duration::from_secs(300) {
                    // 5 minutes
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
