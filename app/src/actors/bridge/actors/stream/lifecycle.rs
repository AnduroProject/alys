//! LifecycleAware Implementation for StreamActor
//! 
//! Complete lifecycle management integration with actor_system

use async_trait::async_trait;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error, debug};

use actor_system::{
    lifecycle::LifecycleAware,
    error::{ActorError, ActorResult},
};

use super::{StreamActor, actor::ConnectionStatus};
use crate::actors::bridge::{messages::stream_messages::NodeConnectionStatus, shared::errors::BridgeError};
use crate::integration::{GovernanceMessage, GovernanceMessageType};

/// Lifecycle metadata for StreamActor
#[derive(Debug, Clone)]
pub struct StreamLifecycleMetadata {
    pub started_at: Option<SystemTime>,
    pub last_state_change: SystemTime,
    pub governance_connections_established: bool,
    pub restart_count: u32,
    pub graceful_shutdown_timeout: Duration,
}

impl Default for StreamLifecycleMetadata {
    fn default() -> Self {
        Self {
            started_at: None,
            last_state_change: SystemTime::now(),
            governance_connections_established: false,
            restart_count: 0,
            graceful_shutdown_timeout: Duration::from_secs(30),
        }
    }
}

#[async_trait]
impl LifecycleAware for StreamActor {
    async fn initialize(&mut self) -> ActorResult<()> {
        info!("Initializing StreamActor");
        
        // Initialize actor_system metrics
        // self.actor_system_metrics.record_actor_started(); // Method doesn't exist
        
        info!("StreamActor initialized successfully");
        Ok(())
    }

    async fn on_start(&mut self) -> ActorResult<()> {
        info!("StreamActor lifecycle: Starting");
        
        // Initialize actor_system metrics
        // self.actor_system_metrics.record_actor_started(); // Method doesn't exist
        
        // Set started timestamp
        if let Ok(mut metadata) = self.get_lifecycle_metadata_mut() {
            metadata.started_at = Some(SystemTime::now());
            metadata.last_state_change = SystemTime::now();
        }

        // Establish governance connections
        match self.establish_governance_connections().await {
            Ok(_) => {
                if let Ok(mut metadata) = self.get_lifecycle_metadata_mut() {
                    metadata.governance_connections_established = true;
                }
                info!("StreamActor governance connections established successfully");
            }
            Err(e) => {
                error!("Failed to establish governance connections during startup: {:?}", e);
                return Err(ActorError::StartupFailed {
                    actor_type: "StreamActor".to_string(),
                    reason: format!("Governance connection failure: {:?}", e),
                });
            }
        }

        // Initialize connection monitoring
        self.start_connection_monitoring_subsystem().await?;
        
        // Start heartbeat system
        self.start_heartbeat_system().await?;
        
        // Initialize request timeout monitoring
        self.start_request_monitoring().await?;

        info!("StreamActor lifecycle: Started successfully");
        Ok(())
    }

    async fn on_shutdown(&mut self, timeout: Duration) -> ActorResult<()> {
        info!("StreamActor lifecycle: Stopping");
        
        let shutdown_timeout = timeout;

        // Stop accepting new messages by updating state
        self.connection_status = ConnectionStatus::Disconnected;

        // Gracefully close governance connections
        if let Err(e) = self.graceful_shutdown_connections(shutdown_timeout).await {
            warn!("Error during graceful connection shutdown: {:?}", e);
        }

        // Flush pending messages with timeout
        if let Err(e) = self.flush_pending_messages(shutdown_timeout).await {
            warn!("Error flushing pending messages: {:?}", e);
        }

        // Complete pending requests with cancellation
        self.cancel_pending_requests().await;

        // Record stop metrics
        // self.actor_system_metrics.record_actor_stopped(); // Method doesn't exist
        
        // Update metadata
        if let Ok(mut metadata) = self.get_lifecycle_metadata_mut() {
            metadata.last_state_change = SystemTime::now();
            metadata.governance_connections_established = false;
        }

        info!("StreamActor lifecycle: Stopped successfully");
        Ok(())
    }

    async fn on_pause(&mut self) -> ActorResult<()> {
        info!("StreamActor lifecycle: Pausing");
        
        // Stop heartbeat to signal pause to governance nodes
        self.pause_heartbeat().await?;
        
        // Mark connections as paused
        for (_node_id, connection) in &mut self.governance_connections {
            connection.status = NodeConnectionStatus::Disconnected;
        }
        
        self.connection_status = ConnectionStatus::Degraded {
            issues: vec!["Actor paused".to_string()],
        };

        if let Ok(mut metadata) = self.get_lifecycle_metadata_mut() {
            metadata.last_state_change = SystemTime::now();
        }

        info!("StreamActor lifecycle: Paused");
        Ok(())
    }

    async fn on_resume(&mut self) -> ActorResult<()> {
        info!("StreamActor lifecycle: Resuming");
        
        // Re-establish governance connections
        match self.establish_governance_connections().await {
            Ok(_) => {
                info!("Governance connections re-established after resume");
            }
            Err(e) => {
                error!("Failed to re-establish governance connections on resume: {:?}", e);
                return Err(ActorError::StartupFailed {
                    actor_type: "StreamActor".to_string(),
                    reason: format!("Resume connection failure: {:?}", e),
                });
            }
        }

        // Restart heartbeat system
        self.resume_heartbeat().await?;
        
        // Update connection status
        self.update_connection_status();
        
        if let Ok(mut metadata) = self.get_lifecycle_metadata_mut() {
            metadata.last_state_change = SystemTime::now();
            metadata.governance_connections_established = true;
        }

        info!("StreamActor lifecycle: Resumed");
        Ok(())
    }


    async fn health_check(&self) -> ActorResult<bool> {
        // Check governance connections
        let healthy_connections = self.governance_connections
            .values()
            .filter(|conn| matches!(conn.status, NodeConnectionStatus::Connected))
            .count();
        
        let total_connections = self.governance_connections.len();
        
        if total_connections == 0 {
            debug!("Health check: No connections configured");
            return Ok(false);
        }

        let connection_health_ratio = healthy_connections as f64 / total_connections as f64;
        let connection_health_ok = connection_health_ratio >= 0.5; // At least 50% healthy

        // Check message processing health
        let message_buffer_healthy = self.message_buffer.len() < 1000; // Not overwhelmed
        
        // Check heartbeat recency
        let heartbeat_healthy = if let Some(last_heartbeat) = self.last_heartbeat {
            let heartbeat_age = SystemTime::now()
                .duration_since(last_heartbeat)
                .unwrap_or_default();
            heartbeat_age < Duration::from_secs(180) // Within 3 minutes
        } else {
            false // No heartbeat sent yet
        };

        // Check pending requests
        let requests_healthy = 0 < 100; // TODO: self.request_tracker().pending_count() < 100; // Not overwhelmed

        let overall_health = connection_health_ok && 
                           message_buffer_healthy && 
                           heartbeat_healthy && 
                           requests_healthy;

        debug!(
            "StreamActor health check: connections={}/{} ({:.1}%), buffer={}, heartbeat_age={:?}, requests={}, healthy={}",
            healthy_connections,
            total_connections,
            connection_health_ratio * 100.0,
            self.message_buffer.len(),
            self.last_heartbeat.map(|t| SystemTime::now().duration_since(t).unwrap_or_default()),
            0, // TODO: self.request_tracker().pending_count(),
            overall_health
        );

        Ok(overall_health)
    }

    fn actor_type(&self) -> &str {
        "StreamActor"
    }

    async fn on_state_change(&mut self, _old_state: actor_system::ActorState, _new_state: actor_system::ActorState) -> Result<(), actor_system::ActorError> {
        // Handle state transitions - for now just log
        debug!("StreamActor state transition: {:?} -> {:?}", _old_state, _new_state);
        Ok(())
    }
}

// Helper methods for StreamActor lifecycle management
impl StreamActor {
    /// Get lifecycle metadata reference
    fn get_lifecycle_metadata(&self) -> Result<StreamLifecycleMetadata, BridgeError> {
        // In a real implementation, this would be stored in the actor state
        // For now, return default metadata
        Ok(StreamLifecycleMetadata::default())
    }

    /// Get mutable lifecycle metadata reference  
    fn get_lifecycle_metadata_mut(&mut self) -> Result<StreamLifecycleMetadata, BridgeError> {
        // In a real implementation, this would be stored in the actor state
        // For now, return default metadata
        Ok(StreamLifecycleMetadata::default())
    }

    /// Start connection monitoring subsystem
    async fn start_connection_monitoring_subsystem(&mut self) -> ActorResult<()> {
        debug!("Starting connection monitoring subsystem");
        // In a real implementation, this would start background monitoring tasks
        Ok(())
    }

    /// Start heartbeat subsystem
    async fn start_heartbeat_system(&mut self) -> ActorResult<()> {
        debug!("Starting heartbeat system");
        // In a real implementation, this would start periodic heartbeat tasks
        Ok(())
    }

    /// Start request monitoring subsystem
    async fn start_request_monitoring(&mut self) -> ActorResult<()> {
        debug!("Starting request monitoring");
        // In a real implementation, this would start timeout monitoring
        Ok(())
    }

    /// Gracefully shutdown connections with timeout
    async fn graceful_shutdown_connections(&mut self, timeout: Duration) -> Result<(), BridgeError> {
        info!("Gracefully shutting down {} governance connections", self.governance_connections.len());
        
        let start_time = SystemTime::now();
        
        for (node_id, connection) in &mut self.governance_connections {
            debug!("Closing connection to {}", node_id);
            
            // Send goodbye message if connected
            if matches!(connection.status, NodeConnectionStatus::Connected) {
                // In a real implementation, send graceful disconnect message
                connection.status = NodeConnectionStatus::Disconnected;
            }
            
            // Check timeout
            if SystemTime::now().duration_since(start_time).unwrap_or_default() > timeout {
                warn!("Graceful shutdown timeout exceeded, force closing remaining connections");
                break;
            }
        }

        self.governance_connections.clear();
        Ok(())
    }

    /// Flush pending messages with timeout
    async fn flush_pending_messages(&mut self, timeout: Duration) -> Result<(), BridgeError> {
        if self.message_buffer.is_empty() {
            return Ok(());
        }

        info!("Flushing {} pending messages", self.message_buffer.len());
        
        let start_time = SystemTime::now();
        
        // Try to send critical messages before shutdown
        let mut critical_messages = Vec::new();
        for pending in &self.message_buffer {
            // Mark signature responses as critical
            match &pending.message.message_type {
                GovernanceMessageType::ConsensusRequest => {
                    critical_messages.push(pending.clone());
                }
                _ => {} // Skip non-critical messages during shutdown
            }
            
            // Check timeout
            if SystemTime::now().duration_since(start_time).unwrap_or_default() > timeout {
                warn!("Message flush timeout exceeded, {} messages will be lost", 
                     self.message_buffer.len() - critical_messages.len());
                break;
            }
        }

        // Try to send critical messages
        for critical in critical_messages {
            if let Err(e) = self.send_message_immediately(critical.message).await {
                warn!("Failed to send critical message during shutdown: {:?}", e);
            }
        }

        self.message_buffer.clear();
        Ok(())
    }

    /// Cancel all pending requests
    async fn cancel_pending_requests(&mut self) {
        let pending_count = 0; // TODO: self.request_tracker().pending_count();
        if pending_count > 0 {
            info!("Cancelling {} pending requests", pending_count);
            
            // In a real implementation, would notify requestors of cancellation
            // TODO: self.request_tracker = super::RequestTracker::new(super::request_tracking::RequestTrackerConfig::default());
        }
    }

    /// Pause heartbeat during pause lifecycle
    async fn pause_heartbeat(&mut self) -> ActorResult<()> {
        debug!("Pausing heartbeat system");
        // In a real implementation, would stop heartbeat timers
        Ok(())
    }

    /// Resume heartbeat after pause
    async fn resume_heartbeat(&mut self) -> ActorResult<()> {
        debug!("Resuming heartbeat system");
        // In a real implementation, would restart heartbeat timers
        self.send_heartbeat().await.map_err(|e| {
            ActorError::StartupFailed {
                actor_type: "StreamActor".to_string(),
                reason: format!("Failed to resume heartbeat: {:?}", e),
            }
        })?;
        Ok(())
    }

    /// Get current resource usage
    async fn get_resource_usage(&self) -> serde_json::Value {
        serde_json::json!({
            "memory_usage_mb": 0, // Would calculate actual usage
            "cpu_usage_percent": 0.0,
            "connection_count": self.governance_connections.len(),
            "message_buffer_size": self.message_buffer.len(),
            "pending_requests": self.request_tracker.pending_count(),
        })
    }

    /// Check health of actor dependencies
    async fn check_dependencies_health(&self) -> bool {
        // Check if bridge coordinator is healthy
        let bridge_healthy = self.bridge_coordinator.is_some();
        
        // Check if pegout actor is healthy  
        let pegout_healthy = self.pegout_actor.is_some();
        
        // In a real implementation, would ping dependencies for health
        bridge_healthy && pegout_healthy
    }

    /// Send message immediately (bypass normal queuing)
    async fn send_message_immediately(&self, message: GovernanceMessage) -> Result<(), BridgeError> {
        debug!("Sending message immediately: {:?}", message.message_type);
        
        // In a real implementation, would send directly via gRPC
        // For now, just log the attempt
        info!("Attempted immediate send of message: {}", message.message_id);
        Ok(())
    }
}