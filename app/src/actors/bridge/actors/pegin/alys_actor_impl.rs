//! AlysActor Implementation for PegInActor
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
    message::AlysMessage,
    metrics::ActorMetrics,
    supervisor::{SupervisionPolicy, SupervisorMessage},
};

use crate::actors::bridge::{
    config::PegInConfig,
    messages::PegInMessage,
};

use super::{actor::PegInActor, state::PegInActorState};
use crate::actors::bridge::shared::errors::BridgeError;

#[async_trait]
impl AlysActor for PegInActor {
    type Config = PegInConfig;
    type Error = BridgeError;
    type Message = PegInMessage;
    type State = PegInActorState;

    fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        // Create mock bitcoin client and empty monitored addresses for AlysActor compatibility
        use crate::actors::bridge::shared::bitcoin_client::BitcoinClientFactory;
        let bitcoin_client = BitcoinClientFactory::create_mock();
        let monitored_addresses = vec![];
        
        Self::new(config, bitcoin_client, monitored_addresses)
            .map_err(|e| BridgeError::PegInError {
                pegin_id: "new_actor".to_string(),
                reason: format!("Failed to create PegInActor: {}", e)
            })
    }

    fn actor_type(&self) -> String {
        "PegInActor".to_string()
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
        PegInActorState {
            current_state: self.state.clone(),
            pending_deposits: self.pending_deposits.len() as u32,
            confirmed_deposits: self.get_confirmed_deposit_count(),
            monitored_addresses: self.monitored_addresses.len() as u32,
            last_block_checked: self.last_block_checked,
            error_count: self.recent_errors.len() as u32,
            metrics_snapshot: self.actor_system_metrics.snapshot(),
        }
    }

    async fn set_state(&mut self, state: Self::State) -> ActorResult<()> {
        // Validate state transition
        if !self.is_valid_state_transition(&state.current_state) {
            return Err(ActorError::InvalidStateTransition {
                from: format!("{:?}", self.state),
                to: format!("{:?}", state.current_state),
                reason: "Invalid PegIn actor state transition".to_string(),
            });
        }

        self.state = state.current_state;
        self.last_block_checked = state.last_block_checked;
        
        Ok(())
    }

    fn mailbox_config(&self) -> MailboxConfig {
        MailboxConfig {
            capacity: self.config.max_pending_deposits as usize,
            enable_priority: true,
            processing_timeout: Duration::from_secs(30),
            backpressure_threshold: 0.9, // Higher threshold for deposit processing
            drop_on_full: true, // Drop oldest messages under backpressure
            metrics_interval: Duration::from_secs(10),
        }
    }

    fn supervision_policy(&self) -> SupervisionPolicy {
        SupervisionPolicy {
            restart_strategy: actor_system::supervisor::RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(500),
                max_delay: Duration::from_secs(60),
                multiplier: 2.0,
            },
            max_restarts: 8, // More restarts for deposit processing
            restart_window: Duration::from_secs(300), // 5 minute window
            escalation_strategy: actor_system::supervisor::EscalationStrategy::EscalateToParent,
            shutdown_timeout: Duration::from_secs(60), // Longer shutdown for pending deposits
            isolate_failures: true, // Isolate deposit processing failures
        }
    }

    fn dependencies(&self) -> Vec<String> {
        vec![
            "bridge_actor".to_string(),
            "bitcoin_client".to_string(),
            "confirmation_tracker".to_string(),
        ]
    }

    async fn on_config_update(&mut self, new_config: Self::Config) -> ActorResult<()> {
        tracing::info!("Updating PegIn actor configuration");

        // Validate new configuration
        if new_config.confirmation_threshold == 0 {
            return Err(ActorError::ConfigurationError {
                parameter: "confirmation_threshold".to_string(),
                reason: "Must be greater than 0".to_string(),
            });
        }

        // Update configuration
        let old_config = self.config.clone();
        self.config = new_config;

        // Handle configuration changes
        if old_config.confirmation_threshold != self.config.confirmation_threshold {
            self.confirmation_tracker.update_threshold(self.config.confirmation_threshold);
        }

        if old_config.max_pending_deposits != self.config.max_pending_deposits {
            self.update_deposit_limits(self.config.max_pending_deposits as u32).await?;
        }

        // Update metrics
        self.metrics_mut().record_config_update();

        Ok(())
    }

    async fn handle_supervisor_message(&mut self, msg: SupervisorMessage) -> ActorResult<()> {
        tracing::debug!("PegIn actor received supervisor message: {:?}", msg);

        match msg {
            SupervisorMessage::HealthCheck => {
                let health_result = self.health_check().await;
                match health_result {
                    Ok(healthy) => {
                        if healthy {
                            self.metrics_mut().record_health_check_success();
                        } else {
                            self.metrics_mut().record_health_check_failure();
                            tracing::warn!("PegIn actor health check failed");
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
                tracing::info!("PegIn actor received shutdown signal with timeout {:?}", timeout);
                self.on_shutdown(timeout).await
            }
            _ => {
                // Delegate other supervisor messages to default handling
                Ok(())
            }
        }
    }

    async fn pre_process_message(&mut self, envelope: &actor_system::message::MessageEnvelope<Self::Message>) -> ActorResult<()> {
        // Update message metrics
        self.metrics_mut().record_message_received(&envelope.payload.message_type());
        
        // Rate limiting for deposit processing
        if !self.check_deposit_rate_limits(&envelope.payload).await? {
            return Err(ActorError::RateLimitExceeded {
                limit: 100, // messages per window
                window: std::time::Duration::from_secs(60), // 1 minute window
            });
        }

        // Validate system state for processing
        if !self.can_process_deposits() {
            return Err(ActorError::ActorNotReady {
                actor_type: AlysActor::actor_type(self),
                reason: "PegIn actor not ready for deposit processing".to_string(),
            });
        }

        Ok(())
    }

    async fn post_process_message(&mut self, envelope: &actor_system::message::MessageEnvelope<Self::Message>, result: &<Self::Message as Message>::Result) -> ActorResult<()> {
        // Update metrics based on result
        match result {
            Ok(_) => {
                self.metrics_mut().record_message_processed_successfully(&envelope.payload.message_type(), Duration::from_millis(0));
            }
            Err(e) => {
                self.metrics_mut().record_message_failed(&format!("{}: {}", envelope.payload.message_type(), e));
            }
        }

        // Update deposit processing metrics
        if let PegInMessage::ProcessDeposit { .. } = &envelope.payload {
            match result {
                Ok(_) => self.metrics.record_deposit_completed(),
                Err(_) => self.metrics.record_deposit_failed(),
            }
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
            "PegIn message processing failed"
        );

        // Handle deposit-specific errors
        if let PegInMessage::ProcessDeposit { txid, .. } = &envelope.payload {
            self.handle_deposit_error(*txid, error.clone()).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl ExtendedAlysActor for PegInActor {
    async fn custom_initialize(&mut self) -> ActorResult<()> {
        tracing::info!("Initializing PegIn actor with extended capabilities");

        // Initialize deposit validation
        // Initialize deposit validation (simplified for now)
        // self.validator.initialize().await.map_err(|e| ActorError::InitializationFailed {
        //     actor_type: AlysActor::actor_type(self),
        //     reason: format!("Deposit validator initialization failed: {}", e),
        // })?;

        // Initialize confirmation tracking (simplified for now)
        // self.confirmation_tracker.start().await.map_err(|e| ActorError::InitializationFailed {
        //     actor_type: AlysActor::actor_type(self),
        //     reason: format!("Confirmation tracker initialization failed: {}", e),
        // })?;

        // Start performance monitoring (simplified for now)
        // self.performance_tracker.start().await.map_err(|e| ActorError::InitializationFailed {
        //     actor_type: AlysActor::actor_type(self),
        //     reason: format!("Performance tracker initialization failed: {}", e),
        // })?;

        Ok(())
    }

    async fn handle_critical_error(&mut self, error: ActorError) -> ActorResult<bool> {
        tracing::error!(
            actor_type = %AlysActor::actor_type(self),
            error = %error,
            "Critical error occurred in PegIn actor"
        );

        // Update error metrics
        self.metrics_mut().record_critical_error(&error.to_string());

        // Determine if restart is needed
        let should_restart = match &error {
            ActorError::SystemFailure { .. } => true,
            ActorError::ResourceExhausted { .. } => {
                // Check if we can recover by clearing old deposits
                self.cleanup_old_deposits().await.is_err()
            }
            ActorError::MessageTimeout { .. } if self.get_timeout_count() > 10 => true,
            ActorError::ExternalServiceError { .. } => {
                // Bitcoin client errors might require restart
                true
            }
            _ => error.severity().is_critical(),
        };

        if should_restart {
            tracing::warn!("PegIn actor requesting restart due to critical error");
            self.cleanup_resources().await?;
        }

        Ok(should_restart)
    }

    async fn maintenance_task(&mut self) -> ActorResult<()> {
        tracing::debug!("Performing PegIn actor maintenance");

        // Clean up old deposits
        self.cleanup_old_deposits().await?;

        // Update confirmation tracking
        self.update_confirmations().await?;

        // Process retry queue
        self.process_retry_queue().await?;

        // Update performance metrics (simplified for now)
        // self.performance_tracker.update_metrics().await?;

        self.metrics_mut().record_maintenance_completed();
        Ok(())
    }

    async fn export_metrics(&self) -> ActorResult<serde_json::Value> {
        let snapshot = self.metrics().snapshot();
        let pegin_metrics = self.get_pegin_specific_metrics().await?;
        
        let combined_metrics = serde_json::json!({
            "actor_system_metrics": snapshot,
            "pegin_metrics": pegin_metrics,
            "pending_deposits": self.pending_deposits.len(),
            "monitored_addresses": self.monitored_addresses.len(),
            "last_block_checked": self.last_block_checked,
            "recent_errors": self.recent_errors.len(),
        });

        Ok(combined_metrics)
    }

    async fn cleanup_resources(&mut self) -> ActorResult<()> {
        tracing::info!("Cleaning up PegIn actor resources");

        // Stop confirmation tracking (simplified for now)
        // self.confirmation_tracker.stop().await.map_err(|e| ActorError::ResourceCleanupFailed {
        //     actor_type: AlysActor::actor_type(self),
        //     resource: "confirmation_tracker".to_string(),
        //     reason: e.to_string(),
        // })?;

        // Clean up pending deposits
        self.pending_deposits.clear();

        // Clear retry queue
        self.retry_queue.clear();

        // Clear recent errors
        self.recent_errors.clear();

        Ok(())
    }
}

// Private implementation methods for PegInActor
impl PegInActor {
    /// Check if state transition is valid
    fn is_valid_state_transition(&self, new_state: &super::state::PegInState) -> bool {
        use super::state::PegInState;
        
        match (&self.state, new_state) {
            (PegInState::Initializing, PegInState::Running) => true,
            (PegInState::Running, PegInState::Processing) => true,
            (PegInState::Processing, PegInState::Running) => true,
            (_, PegInState::ShuttingDown) => true,
            (PegInState::ShuttingDown, PegInState::Stopped) => true,
            _ => false,
        }
    }

    /// Add actor system metrics field
    pub fn add_actor_system_metrics(&mut self) {
        self.actor_system_metrics = ActorMetrics::new();
    }

    /// Get confirmed deposit count
    fn get_confirmed_deposit_count(&self) -> u32 {
        // Count deposits with sufficient confirmations
        self.pending_deposits.values()
            .filter(|deposit| deposit.confirmations >= self.config.confirmation_threshold)
            .count() as u32
    }

    /// Update deposit limits
    async fn update_deposit_limits(&mut self, new_limit: u32) -> ActorResult<()> {
        if self.pending_deposits.len() > new_limit as usize {
            tracing::warn!(
                "Current deposits ({}) exceed new limit ({})",
                self.pending_deposits.len(),
                new_limit
            );
        }
        Ok(())
    }

    /// Check deposit rate limits
    async fn check_deposit_rate_limits(&self, _message: &PegInMessage) -> ActorResult<bool> {
        // Implement rate limiting logic
        Ok(true)
    }

    /// Check if actor can process deposits
    fn can_process_deposits(&self) -> bool {
        use super::state::PegInState;
        matches!(self.state,
            PegInState::Running | PegInState::Processing
        )
    }

    /// Handle deposit processing error
    async fn handle_deposit_error(&mut self, txid: bitcoin::Txid, error: ActorError) -> ActorResult<()> {
        tracing::error!("Deposit processing error for {}: {}", txid, error);
        
        // Note: Would store error in recent_errors if field type supported BridgeError
        let _pegin_error = BridgeError::PegInError {
            pegin_id: format!("deposit_{}", txid),
            reason: format!("Deposit processing failed: {}", error)
        };
        
        // Keep only recent errors
        if self.recent_errors.len() > 100 {
            self.recent_errors.drain(0..10);
        }
        
        Ok(())
    }

    /// Get timeout count
    fn get_timeout_count(&self) -> u32 {
        // TODO: Implement proper error tracking when recent_errors field type is clarified
        0 // self.recent_errors.iter().filter(timeout_errors).count() as u32
    }

    /// Clean up old deposits
    async fn cleanup_old_deposits(&mut self) -> ActorResult<()> {
        let cutoff_time = std::time::SystemTime::now() - std::time::Duration::from_secs(3600); // 1 hour
        
        let old_deposits: Vec<bitcoin::Txid> = self.pending_deposits.iter()
            .filter(|(_, deposit)| deposit.created_at < cutoff_time)
            .map(|(txid, _)| *txid)
            .collect();
            
        for txid in old_deposits {
            self.pending_deposits.remove(&txid);
        }
        
        Ok(())
    }

    /// Update confirmation tracking
    async fn update_confirmations(&mut self) -> ActorResult<()> {
        // Get transactions that need updates
        let txids_needing_updates = self.confirmation_tracker.get_transactions_needing_updates();

        // Update each transaction's confirmations (would need Bitcoin RPC client here)
        for _txid in txids_needing_updates {
            // In a real implementation, would call Bitcoin RPC to get confirmations
            // self.confirmation_tracker.update_confirmations(txid, confirmations, block_height);
        }

        Ok(())
    }

    /// Process retry queue
    async fn process_retry_queue(&mut self) -> ActorResult<()> {
        let now = std::time::SystemTime::now();
        let ready_retries: Vec<_> = self.retry_queue.iter()
            .enumerate()
            .filter(|(_, op)| now >= op.next_retry)
            .map(|(idx, _)| idx)
            .collect();

        // Process ready retries (simplified)
        for idx in ready_retries.into_iter().rev() {
            let _retry_op = self.retry_queue.remove(idx);
            // Would actually retry the operation here
        }

        Ok(())
    }

    /// Get PegIn-specific metrics
    async fn get_pegin_specific_metrics(&self) -> ActorResult<serde_json::Value> {
        let snapshot = self.metrics.get_snapshot();
        Ok(serde_json::json!({
            "successful_deposits": snapshot.deposits_completed,
            "failed_deposits": snapshot.deposits_failed,
            "detected_deposits": snapshot.deposits_detected,
            "confirmed_deposits": snapshot.deposits_confirmed,
            "success_rate": snapshot.success_rate,
            "error_rate": snapshot.error_rate,
            "blocks_processed": snapshot.blocks_processed,
        }))
    }
}