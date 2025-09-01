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
    GovernanceConnection, 
    ConnectionStatus,
    StreamMetrics,
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

    async fn new(config: Self::Config) -> ActorResult<Self> {
        info!("Creating StreamActor with actor_system integration");
        
        let reconnection_manager = super::ReconnectionManager::new(
            config.reconnect_attempts.unwrap_or(5),
            config.reconnect_delay.unwrap_or(Duration::from_secs(5)),
        );
        
        let metrics = StreamMetrics::new()
            .map_err(|e| ActorError::StartupFailed {
                actor_type: "StreamActor".to_string(),
                reason: format!("Failed to initialize metrics: {:?}", e),
            })?;

        let actor_system_metrics = ActorMetrics::new("bridge_stream_actor", "v1.0.0")
            .map_err(|e| ActorError::StartupFailed {
                actor_type: "StreamActor".to_string(), 
                reason: format!("Failed to initialize actor_system metrics: {:?}", e),
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

    fn actor_type() -> String {
        "StreamActor".to_string()
    }

    fn version() -> String {
        "v1.0.0".to_string()
    }

    async fn health_check(&self) -> Result<bool, Self::Error> {
        let healthy_connections = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, super::NodeConnectionStatus::Connected))
            .count();
        
        let total_connections = self.governance_connections.len();
        let health_threshold = 0.5; // At least 50% connections healthy

        if total_connections == 0 {
            return Ok(false); // No connections configured
        }

        let health_ratio = healthy_connections as f64 / total_connections as f64;
        Ok(health_ratio >= health_threshold)
    }

    fn current_state(&self) -> Self::State {
        let metrics_snapshot = self.actor_system_metrics.snapshot()
            .unwrap_or_default();

        StreamActorState {
            lifecycle_state: ActorState::Running, // Will be managed by LifecycleAware
            connection_status: self.connection_status.clone(),
            active_connections: self.governance_connections.len(),
            pending_requests: self.request_tracker.pending_count(),
            last_heartbeat: self.last_heartbeat,
            metrics_snapshot,
        }
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

    async fn on_message_received(&mut self, envelope: &MessageEnvelope<Self::Message>) -> ActorResult<()> {
        debug!("StreamActor received message: {:?}", envelope.message_type());
        self.actor_system_metrics.record_message_received();
        
        // Update last activity
        if let Ok(mut state) = self.get_mutable_state() {
            state.last_activity = SystemTime::now();
        }
        
        Ok(())
    }

    async fn on_message_processed(&mut self, envelope: &MessageEnvelope<Self::Message>, success: bool) -> ActorResult<()> {
        if success {
            self.actor_system_metrics.record_message_processed();
            debug!("StreamActor successfully processed message: {:?}", envelope.message_type());
        } else {
            self.actor_system_metrics.record_message_failed();
            warn!("StreamActor failed to process message: {:?}", envelope.message_type());
        }
        Ok(())
    }

    async fn pre_message_hook(&mut self, envelope: &MessageEnvelope<Self::Message>) -> ActorResult<bool> {
        // Rate limiting check
        if let Some(rate_limit) = &self.config.rate_limit_config {
            if !rate_limit.allow_message(&envelope.message_type()) {
                warn!("Rate limit exceeded for message type: {:?}", envelope.message_type());
                return Ok(false); // Block message processing
            }
        }

        // Connection health check for governance messages
        match &envelope.message {
            StreamMessage::RequestPegOutSignatures { .. } |
            StreamMessage::SendHeartbeat |
            StreamMessage::HandleFederationUpdate { .. } => {
                if !self.has_healthy_connections() {
                    warn!("No healthy governance connections, deferring message");
                    return Ok(false); // Will be retried when connections recover
                }
            }
            _ => {} // Local messages don't need connection checks
        }

        Ok(true)
    }

    async fn post_message_hook(&mut self, envelope: &MessageEnvelope<Self::Message>, result: &ActorResult<()>) -> ActorResult<()> {
        // Log processing time
        if let Some(start_time) = envelope.metadata.get("start_time") {
            if let Ok(start) = start_time.parse::<u64>() {
                let duration = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64 - start;
                
                self.actor_system_metrics.record_processing_time(Duration::from_millis(duration));
            }
        }

        // Handle errors
        if let Err(error) = result {
            error!("StreamActor message processing error: {:?}", error);
            
            // Increment error counter for specific message types
            match &envelope.message {
                StreamMessage::RequestPegOutSignatures { .. } => {
                    self.metrics.record_signature_request_error();
                }
                StreamMessage::SendHeartbeat => {
                    self.metrics.record_heartbeat_failed();
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn get_dependencies(&self) -> Vec<String> {
        vec![
            "bridge_actor".to_string(),
            "pegout_actor".to_string(),
        ]
    }

    fn provides_services(&self) -> Vec<String> {
        vec![
            "governance_communication".to_string(),
            "signature_requests".to_string(),
            "federation_updates".to_string(),
            "heartbeat_monitoring".to_string(),
        ]
    }

    async fn validate_config(config: &Self::Config) -> ActorResult<()> {
        if config.governance_endpoints.is_empty() {
            return Err(ActorError::ConfigurationError {
                parameter: "governance_endpoints".to_string(),
                reason: "At least one governance endpoint must be configured".to_string(),
            });
        }

        if config.heartbeat_interval < Duration::from_secs(10) {
            return Err(ActorError::ConfigurationError {
                parameter: "heartbeat_interval".to_string(),
                reason: "Heartbeat interval must be at least 10 seconds".to_string(),
            });
        }

        if config.connection_timeout < Duration::from_secs(30) {
            return Err(ActorError::ConfigurationError {
                parameter: "connection_timeout".to_string(),
                reason: "Connection timeout must be at least 30 seconds".to_string(),
            });
        }

        if let Some(max_connections) = config.max_governance_connections {
            if max_connections == 0 {
                return Err(ActorError::ConfigurationError {
                    parameter: "max_governance_connections".to_string(),
                    reason: "Must allow at least 1 governance connection".to_string(),
                });
            }
        }

        Ok(())
    }

    fn get_metrics(&self) -> ActorMetrics {
        self.actor_system_metrics.clone()
    }

    async fn update_config(&mut self, new_config: Self::Config) -> ActorResult<()> {
        info!("Updating StreamActor configuration");

        // Validate new configuration
        Self::validate_config(&new_config).await?;

        // Check what changed
        let endpoints_changed = self.config.governance_endpoints != new_config.governance_endpoints;
        let connection_params_changed = 
            self.config.heartbeat_interval != new_config.heartbeat_interval ||
            self.config.connection_timeout != new_config.connection_timeout;

        // Update configuration
        self.config = new_config;

        // Handle configuration changes
        if endpoints_changed {
            info!("Governance endpoints changed, reconnecting...");
            self.reconnect_to_governance_nodes().await?;
        }

        if connection_params_changed {
            info!("Connection parameters changed, updating timers");
            self.update_connection_timers().await?;
        }

        self.actor_system_metrics.record_config_update();
        Ok(())
    }
}

#[async_trait]
impl ExtendedAlysActor for StreamActor {
    async fn custom_initialization(&mut self) -> ActorResult<()> {
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

    async fn handle_critical_error(&mut self, error: &ActorError) -> ActorResult<bool> {
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

    async fn perform_maintenance(&mut self) -> ActorResult<()> {
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

    async fn get_status_info(&self) -> ActorResult<serde_json::Value> {
        let healthy_connections = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, super::NodeConnectionStatus::Connected))
            .count();

        let status = serde_json::json!({
            "actor_type": "StreamActor",
            "version": "v1.0.0",
            "state": "Running",
            "governance_connections": {
                "total": self.governance_connections.len(),
                "healthy": healthy_connections,
                "endpoints": self.config.governance_endpoints,
            },
            "message_processing": {
                "pending_messages": self.message_buffer.len(),
                "pending_requests": self.request_tracker.pending_count(),
            },
            "last_heartbeat": self.last_heartbeat,
            "uptime": self.actor_system_metrics.uptime(),
            "metrics": {
                "messages_processed": self.actor_system_metrics.messages_processed(),
                "errors": self.actor_system_metrics.error_count(),
            }
        });

        Ok(status)
    }

    async fn export_metrics(&self) -> ActorResult<Vec<(String, f64)>> {
        let mut metrics = Vec::new();

        // Connection metrics
        let healthy_connections = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, super::NodeConnectionStatus::Connected))
            .count() as f64;
        
        metrics.push(("governance_connections_healthy".to_string(), healthy_connections));
        metrics.push(("governance_connections_total".to_string(), self.governance_connections.len() as f64));

        // Message metrics
        metrics.push(("pending_messages".to_string(), self.message_buffer.len() as f64));
        metrics.push(("pending_requests".to_string(), self.request_tracker.pending_count() as f64));

        // Heartbeat metrics
        if let Some(last_heartbeat) = self.last_heartbeat {
            let heartbeat_age = SystemTime::now()
                .duration_since(last_heartbeat)
                .unwrap_or_default()
                .as_secs() as f64;
            metrics.push(("heartbeat_age_seconds".to_string(), heartbeat_age));
        }

        // Add actor_system metrics
        let system_metrics = self.actor_system_metrics.export_metrics();
        metrics.extend(system_metrics);

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
            .any(|conn| matches!(conn.status, super::NodeConnectionStatus::Connected))
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
            if !matches!(connection.status, super::NodeConnectionStatus::Connected) {
                debug!("Marking {} for reconnection", node_id);
                connection.status = super::NodeConnectionStatus::Connecting;
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

// Extend StreamActor struct to include actor_system_metrics
impl StreamActor {
    /// Add the actor_system_metrics field to the existing struct
    /// This would typically be added to the struct definition
    actor_system_metrics: ActorMetrics,
}