//! AlysActor Implementation for BridgeActor
//! 
//! Integration with actor_system crate's standardized actor interface

use async_trait::async_trait;
use actix::prelude::*;
use std::time::Duration;

use actor_system::{
    actor::{AlysActor, ExtendedAlysActor},
    error::{ActorError, ActorResult},
    lifecycle::LifecycleAware,
    mailbox::MailboxConfig,
    metrics::ActorMetrics,
    supervisor::{SupervisionPolicy, SupervisorMessage},
    message::AlysMessage,
};

use crate::actors::bridge::{
    config::BridgeConfig,
    messages::BridgeCoordinationMessage,
    actors::bridge::BridgeActor,
};

use super::state::BridgeActorState;
use crate::actors::bridge::shared::errors::BridgeError;

/// Convert BridgeError to ActorError
impl From<BridgeError> for ActorError {
    fn from(err: BridgeError) -> Self {
        match err {
            BridgeError::ConnectionError(reason) => ActorError::NetworkError { reason },
            BridgeError::NetworkError(reason) => ActorError::NetworkError { reason },
            BridgeError::AuthenticationError(reason) => ActorError::PermissionDenied { resource: "authentication".to_string(), reason },
            BridgeError::ConfigurationError(reason) => ActorError::ConfigurationError { parameter: "bridge_config".to_string(), reason },
            BridgeError::ValidationError { field, reason } => ActorError::ValidationFailed { field, reason },
            BridgeError::SimpleValidationError(reason) => ActorError::ValidationFailed { field: "general".to_string(), reason },
            BridgeError::SerializationError(reason) => ActorError::SerializationFailed { reason },
            BridgeError::InternalError(reason) => ActorError::Internal { reason },
            BridgeError::ActorSystemError(reason) => ActorError::SystemFailure { reason },
            BridgeError::PegInError { pegin_id, reason } => ActorError::MessageHandlingFailed { message_type: "PegIn".to_string(), reason: format!("{}: {}", pegin_id, reason) },
            BridgeError::PegOutError { pegout_id, reason } => ActorError::MessageHandlingFailed { message_type: "PegOut".to_string(), reason: format!("{}: {}", pegout_id, reason) },
            BridgeError::RequestTimeout { request_id, timeout } => ActorError::Timeout { operation: format!("request_{}", request_id), timeout },
            BridgeError::RequestCancelled { request_id } => ActorError::MessageHandlingFailed { message_type: "Request".to_string(), reason: format!("Request {} cancelled", request_id) },
            BridgeError::RequestNotFound { request_id } => ActorError::NotFound { resource: "request".to_string(), id: request_id },
            BridgeError::InvalidRequest(reason) => ActorError::ValidationFailed { field: "request".to_string(), reason },
            BridgeError::UnknownRequest(request_id) => ActorError::NotFound { resource: "request".to_string(), id: request_id },
            BridgeError::SignatureCollectionFailed { request_id, reason } => ActorError::MessageHandlingFailed { message_type: "SignatureCollection".to_string(), reason: format!("{}: {}", request_id, reason) },
            BridgeError::InsufficientSignatures { request_id, collected, required } => ActorError::ValidationFailed { field: "signatures".to_string(), reason: format!("Request {}: {}/{} signatures", request_id, collected, required) },
            BridgeError::FederationUpdateFailed { update_id, reason } => ActorError::MessageHandlingFailed { message_type: "FederationUpdate".to_string(), reason: format!("{}: {}", update_id, reason) },
            BridgeError::GrpcError(reason) => ActorError::ExternalDependency { service: "grpc".to_string(), reason },
            BridgeError::GovernanceError(reason) => ActorError::PermissionDenied { resource: "governance".to_string(), reason },
            BridgeError::ResourceExhausted { resource, details } => ActorError::ResourceExhausted { resource, details },
            BridgeError::InvalidStateTransition { from, to, reason } => ActorError::InvalidStateTransition { from, to, reason },
            BridgeError::ServiceUnavailable { service, .. } => ActorError::ExternalDependency { service, reason: "Service unavailable".to_string() },
            BridgeError::RateLimitExceeded { limit, window } => ActorError::RateLimitExceeded { limit, window },
        }
    }
}

#[async_trait]
impl AlysActor for BridgeActor {
    type Config = BridgeConfig;
    type Error = BridgeError;
    type Message = BridgeCoordinationMessage;
    type State = BridgeActorState;

    fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Self::new(config)
    }

    fn actor_type(&self) -> String {
        "BridgeActor".to_string()
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
        BridgeActorState {
            current_state: self.state.clone(),
            active_operations: self.active_operations.len() as u32,
            registered_actors: self.child_actors.get_registered_count(),
            last_health_check: self.health_monitor.get_last_check_time(),
            metrics_snapshot: self.metrics.create_snapshot(),
        }
    }

    async fn set_state(&mut self, state: Self::State) -> ActorResult<()> {
        // Validate state transition
        if !self.is_valid_state_transition(&state.current_state) {
            return Err(ActorError::InvalidStateTransition {
                from: format!("{:?}", self.state),
                to: format!("{:?}", state.current_state),
                reason: "Invalid Bridge actor state transition".to_string(),
            });
        }

        self.state = state.current_state;
        self.health_monitor.update_last_check(state.last_health_check);
        
        Ok(())
    }

    fn mailbox_config(&self) -> MailboxConfig {
        MailboxConfig {
            capacity: self.config.max_concurrent_operations,
            enable_priority: true,
            processing_timeout: self.config.operation_timeout,
            backpressure_threshold: 0.8,
            drop_on_full: false,
            metrics_interval: Duration::from_secs(10),
        }
    }

    fn supervision_policy(&self) -> SupervisionPolicy {
        SupervisionPolicy {
            restart_strategy: actor_system::supervisor::RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(30),
                multiplier: 2.0,
            },
            max_restarts: 5,
            restart_window: Duration::from_secs(60),
            escalation_strategy: actor_system::supervisor::EscalationStrategy::EscalateToParent,
            shutdown_timeout: Duration::from_secs(30),
            isolate_failures: false, // Bridge coordinator should not be isolated
        }
    }

    fn dependencies(&self) -> Vec<String> {
        vec![
            "actor_registry".to_string(),
            "metrics_collector".to_string(),
            "supervision_tree".to_string(),
        ]
    }

    async fn on_config_update(&mut self, new_config: Self::Config) -> ActorResult<()> {
        tracing::info!("Updating bridge actor configuration");

        // Validate new configuration
        if new_config.max_concurrent_operations == 0 {
            return Err(ActorError::ConfigurationError {
                parameter: "max_concurrent_operations".to_string(),
                reason: "Must be greater than 0".to_string(),
            });
        }

        // Update configuration
        let old_config = self.config.clone();
        self.config = new_config;

        // Handle configuration changes that require actor updates
        if old_config.health_check_interval != self.config.health_check_interval {
            self.health_monitor.update_interval(self.config.health_check_interval).map_err(|e| ActorError::ConfigurationError {
                parameter: "health_check_interval".to_string(),
                reason: format!("Failed to update health check interval: {}", e),
            })?;
        }

        if old_config.max_concurrent_operations != self.config.max_concurrent_operations {
            // Update operation limits
            self.update_operation_limits(self.config.max_concurrent_operations as u32).await?;
        }

        // Update metrics
        self.metrics_mut().record_config_update();

        Ok(())
    }

    async fn handle_supervisor_message(&mut self, msg: SupervisorMessage) -> ActorResult<()> {
        tracing::debug!("Bridge actor received supervisor message: {:?}", msg);

        match msg {
            SupervisorMessage::HealthCheck => {
                let health_result = self.health_check().await;
                match health_result {
                    Ok(healthy) => {
                        if healthy {
                            self.metrics_mut().record_health_check_success();
                        } else {
                            self.metrics_mut().record_health_check_failure();
                            tracing::warn!("Bridge actor health check failed");
                        }
                        Ok(())
                    }
                    Err(e) => {
                        let actor_error: ActorError = e.into();
                        self.metrics_mut().record_health_check_error(&actor_error.to_string());
                        Err(actor_error)
                    }
                }
            }
            SupervisorMessage::Shutdown { timeout } => {
                tracing::info!("Bridge actor received shutdown signal with timeout {:?}", timeout);
                self.on_shutdown(timeout).await
            }
            SupervisorMessage::AddChild { child_id, actor_type, policy } => {
                tracing::info!("Adding child actor: {} of type {}", child_id, actor_type);
                self.handle_add_child(child_id, actor_type, policy).await
            }
            SupervisorMessage::RemoveChild { child_id } => {
                tracing::info!("Removing child actor: {}", child_id);
                self.handle_remove_child(child_id).await
            }
            SupervisorMessage::GetTreeStatus => {
                // Return current supervision tree status
                self.get_supervision_status().await
            }
            SupervisorMessage::ChildFailed { supervisor_id, child_id, error } => {
                tracing::error!("Child actor failed: {} in supervisor {}: {}", child_id, supervisor_id, error);
                self.handle_child_failure(child_id, error).await
            }
        }
    }

    async fn pre_process_message(&mut self, envelope: &actor_system::message::MessageEnvelope<Self::Message>) -> ActorResult<()> {
        // Update message metrics
        self.metrics_mut().record_message_received(&envelope.payload.message_type());
        
        // Log high-priority messages
        if envelope.metadata.priority.is_urgent() {
            tracing::info!(
                message_id = %envelope.id,
                message_type = %envelope.payload.message_type(),
                priority = ?envelope.metadata.priority,
                "Processing urgent bridge message"
            );
        }

        // Rate limiting for certain message types
        if !self.check_rate_limits(&envelope.payload).await? {
            return Err(ActorError::RateLimitExceeded {
                limit: 100, // Default rate limit
                window: Duration::from_secs(60), // 1 minute window
            });
        }

        Ok(())
    }

    async fn post_process_message(&mut self, envelope: &actor_system::message::MessageEnvelope<Self::Message>, result: &<Self::Message as actix::Message>::Result) -> ActorResult<()> {
        // Update metrics based on result
        match result {
            Ok(_) => {
                self.metrics_mut().record_message_processed_successfully(&envelope.payload.message_type(), Duration::from_millis(0));
            }
            Err(e) => {
                self.metrics_mut().record_message_failed(&format!("{}: {}", envelope.payload.message_type(), e));
            }
        }

        // Log completion of critical operations
        if envelope.metadata.priority.is_critical() {
            let duration = std::time::SystemTime::now()
                .duration_since(envelope.metadata.created_at)
                .unwrap_or_default();
            
            tracing::info!(
                message_id = %envelope.id,
                message_type = %envelope.payload.message_type(),
                duration_ms = duration.as_millis(),
                success = result.is_ok(),
                "Completed critical bridge message processing"
            );
        }

        Ok(())
    }

    async fn handle_message_error(&mut self, envelope: &actor_system::message::MessageEnvelope<Self::Message>, error: &ActorError) -> ActorResult<()> {
        self.metrics_mut().record_message_failed(&format!("{}: {}", envelope.payload.message_type(), error));
        
        tracing::error!(
            message_id = %envelope.id,
            message_type = %envelope.payload.message_type(),
            error = %error,
            actor_type = %AlysActor::actor_type(self),
            "Bridge message processing failed"
        );

        // Handle specific error types
        match error {
            ActorError::Timeout { .. } => {
                // Increment timeout counter for this message type
                self.handle_message_timeout(&envelope.payload).await?;
            }
            ActorError::RateLimitExceeded { .. } => {
                // Log rate limiting incident
                self.handle_rate_limit_exceeded(&envelope.payload).await?;
            }
            _ => {
                // General error handling
                self.handle_general_message_error(envelope, error).await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ExtendedAlysActor for BridgeActor {
    async fn custom_initialize(&mut self) -> ActorResult<()> {
        tracing::info!("Initializing bridge actor with extended capabilities");

        // Initialize health monitoring
        self.health_monitor.start().await.map_err(|e| ActorError::StartupFailed {
            actor_type: AlysActor::actor_type(self),
            reason: format!("Health monitoring initialization failed: {}", e),
        })?;

        // Initialize metrics collection
        self.metrics.initialize().await.map_err(|e| ActorError::StartupFailed {
            actor_type: AlysActor::actor_type(self),
            reason: format!("Metrics initialization failed: {}", e),
        })?;

        // Set up periodic tasks
        self.setup_periodic_tasks().await?;

        Ok(())
    }

    async fn handle_critical_error(&mut self, error: ActorError) -> ActorResult<bool> {
        tracing::error!(
            actor_type = %AlysActor::actor_type(self),
            error = %error,
            "Critical error occurred in bridge actor"
        );

        // Update error metrics
        self.metrics_mut().record_critical_error(&error.to_string());

        // Determine if restart is needed based on error type
        let should_restart = match &error {
            ActorError::SystemFailure { .. } => true,
            ActorError::ResourceExhausted { .. } => true,
            ActorError::Timeout { .. } if self.get_timeout_count() > 5 => true,
            ActorError::ActorNotFound { .. } => false, // Don't restart for missing actors
            ActorError::ConfigurationError { .. } => false, // Don't restart for config issues
            _ => error.severity().is_critical(),
        };

        if should_restart {
            tracing::warn!("Bridge actor requesting restart due to critical error");
            // Perform cleanup before restart
            self.cleanup_resources().await?;
        }

        Ok(should_restart)
    }

    async fn maintenance_task(&mut self) -> ActorResult<()> {
        tracing::debug!("Performing bridge actor maintenance");

        // Clean up completed operations
        self.cleanup_completed_operations().await?;

        // Update health status
        self.update_health_status().await?;

        // Perform metrics aggregation
        self.aggregate_metrics().await?;

        // Check child actor health
        self.check_child_actor_health().await?;

        // Update performance metrics
        self.metrics_mut().record_maintenance_completed();

        Ok(())
    }

    async fn export_metrics(&self) -> ActorResult<serde_json::Value> {
        let snapshot = self.metrics().snapshot();
        let bridge_metrics = self.get_bridge_specific_metrics().await?;
        
        let combined_metrics = serde_json::json!({
            "actor_system_metrics": snapshot,
            "bridge_metrics": bridge_metrics,
            "active_operations": self.active_operations.len(),
            "registered_actors": self.child_actors.get_registered_count(),
            "system_state": self.state,
            "uptime": std::time::SystemTime::now()
                .duration_since(self.started_at)
                .unwrap_or_default()
                .as_secs()
        });

        Ok(combined_metrics)
    }

    async fn cleanup_resources(&mut self) -> ActorResult<()> {
        tracing::info!("Cleaning up bridge actor resources");

        // Cancel active operations
        let operation_ids: Vec<String> = self.active_operations.keys().cloned().collect();
        for operation_id in operation_ids {
            tracing::debug!("Cancelling active operation: {}", operation_id);
            self.cancel_operation(&operation_id).await?;
        }

        // Close connections to child actors
        // Clear child actor addresses (equivalent to disconnect_child_actors)
        // Implementation would clear child actor references here

        // Release monitoring resources
        self.health_monitor.stop().await.map_err(|e| ActorError::SystemFailure {
            reason: format!("Failed to stop health monitor for {}: {}", AlysActor::actor_type(self), e),
        })?;

        // Flush metrics
        self.metrics.flush().await.map_err(|e| ActorError::SystemFailure {
            reason: format!("Failed to flush metrics for {}: {}", AlysActor::actor_type(self), e),
        })?;

        Ok(())
    }
}

// Private implementation methods for BridgeActor
impl BridgeActor {
    /// Check if state transition is valid
    fn is_valid_state_transition(&self, new_state: &crate::actors::bridge::actors::bridge::state::BridgeState) -> bool {
        use crate::actors::bridge::actors::bridge::state::BridgeState;
        
        match (&self.state, new_state) {
            (BridgeState::Initializing, BridgeState::Running) => true,
            (BridgeState::Running, BridgeState::Paused) => true,
            (BridgeState::Paused, BridgeState::Running) => true,
            (_, BridgeState::ShuttingDown) => true,
            (BridgeState::ShuttingDown, BridgeState::Stopped) => true,
            _ => false,
        }
    }

    /// Update operation limits
    async fn update_operation_limits(&mut self, new_limit: u32) -> ActorResult<()> {
        if self.active_operations.len() > new_limit as usize {
            tracing::warn!(
                "Current operations ({}) exceed new limit ({}), will complete existing operations",
                self.active_operations.len(),
                new_limit
            );
        }
        Ok(())
    }

    /// Check rate limits for message processing
    async fn check_rate_limits(&self, _message: &BridgeCoordinationMessage) -> ActorResult<bool> {
        // Implement rate limiting logic based on message type
        // For now, always allow
        Ok(true)
    }

    /// Handle supervisor message to add child
    async fn handle_add_child(&mut self, child_id: String, actor_type: String, _policy: Option<SupervisionPolicy>) -> ActorResult<()> {
        tracing::info!("Adding child actor {} of type {}", child_id, actor_type);
        // Implementation depends on specific child actor management
        Ok(())
    }

    /// Handle supervisor message to remove child
    async fn handle_remove_child(&mut self, child_id: String) -> ActorResult<()> {
        tracing::info!("Removing child actor {}", child_id);
        // Implementation depends on specific child actor management
        Ok(())
    }

    /// Get current supervision status
    async fn get_supervision_status(&self) -> ActorResult<()> {
        // Return supervision tree status
        Ok(())
    }

    /// Handle child actor failure
    async fn handle_child_failure(&mut self, child_id: String, error: ActorError) -> ActorResult<()> {
        tracing::error!("Handling child failure: {} - {}", child_id, error);
        // Implement child failure handling logic
        Ok(())
    }

    /// Handle message timeout
    async fn handle_message_timeout(&mut self, _message: &BridgeCoordinationMessage) -> ActorResult<()> {
        // Implement timeout handling
        Ok(())
    }

    /// Handle rate limit exceeded
    async fn handle_rate_limit_exceeded(&mut self, _message: &BridgeCoordinationMessage) -> ActorResult<()> {
        // Implement rate limit handling
        Ok(())
    }

    /// Handle general message error
    async fn handle_general_message_error(&mut self, _envelope: &actor_system::message::MessageEnvelope<<Self as AlysActor>::Message>, _error: &ActorError) -> ActorResult<()> {
        // Implement general error handling
        Ok(())
    }

    /// Get timeout count
    fn get_timeout_count(&self) -> u32 {
        // Implementation to track timeout counts
        0
    }

    /// Setup periodic tasks
    async fn setup_periodic_tasks(&mut self) -> ActorResult<()> {
        // Setup periodic maintenance tasks
        Ok(())
    }

    /// Cleanup completed operations
    async fn cleanup_completed_operations(&mut self) -> ActorResult<()> {
        // Remove completed operations from active list
        Ok(())
    }

    /// Update health status
    async fn update_health_status(&mut self) -> ActorResult<()> {
        // Update actor health status
        Ok(())
    }

    /// Aggregate metrics
    async fn aggregate_metrics(&mut self) -> ActorResult<()> {
        // Perform metrics aggregation
        Ok(())
    }

    /// Check child actor health
    async fn check_child_actor_health(&mut self) -> ActorResult<()> {
        // Check health of child actors
        Ok(())
    }

    /// Get bridge-specific metrics
    async fn get_bridge_specific_metrics(&self) -> ActorResult<serde_json::Value> {
        Ok(serde_json::json!({
            "coordination_operations": self.metrics.coordination_operations.load(std::sync::atomic::Ordering::Relaxed),
            "active_pegin_operations": self.get_active_pegin_count(),
            "active_pegout_operations": self.get_active_pegout_count(),
        }))
    }

    /// Cancel operation
    async fn cancel_operation(&mut self, _operation_id: &str) -> ActorResult<()> {
        // Cancel active operation
        Ok(())
    }


    /// Get active peg-in count
    fn get_active_pegin_count(&self) -> u32 {
        // Count active peg-in operations
        0
    }

    /// Get active peg-out count
    fn get_active_pegout_count(&self) -> u32 {
        // Count active peg-out operations
        0
    }
}