//! Example demonstrating the enhanced actor system integration
//! 
//! This example shows how to use the consolidated actor_system crate
//! with blockchain-aware capabilities for the Alys V2 architecture.

use actor_system::prelude::*;
use std::time::Duration;
use tracing::info;

/// Example configuration for a simple blockchain actor
#[derive(Debug, Clone)]
pub struct ExampleConfig {
    pub actor_id: Option<String>,
    pub block_interval: Duration,
    pub federation_threshold: usize,
}

impl Default for ExampleConfig {
    fn default() -> Self {
        Self {
            actor_id: Some("example_actor".to_string()),
            block_interval: Duration::from_secs(2),
            federation_threshold: 3,
        }
    }
}

/// Example state for the actor
#[derive(Debug, Clone)]
pub struct ExampleState {
    pub current_height: u64,
    pub is_healthy: bool,
    pub last_block_time: SystemTime,
}

impl ExampleState {
    pub fn new() -> Self {
        Self {
            current_height: 0,
            is_healthy: true,
            last_block_time: SystemTime::now(),
        }
    }
}

/// Example message types
#[derive(Debug, Clone, Message)]
#[rtype(result = "ActorResult<()>")]
pub enum ExampleMessage {
    ProcessBlock { height: u64 },
    UpdateHealth { healthy: bool },
    GetStatus,
}

/// Example blockchain actor using the enhanced framework
pub struct ExampleBlockchainActor {
    config: ExampleConfig,
    state: ExampleState,
    metrics: ActorMetrics,
}

// Use the enhanced macro to implement basic actor traits
impl_blockchain_actor!(
    ExampleBlockchainActor,
    config = ExampleConfig,
    state = ExampleState, 
    message = ExampleMessage,
    priority = BlockchainActorPriority::Network
);

impl ExampleBlockchainActor {
    /// Handle ProcessBlock message
    async fn handle_process_block(&mut self, height: u64) -> ActorResult<()> {
        info!(height = height, "Processing block");
        
        self.state.current_height = height;
        self.state.last_block_time = SystemTime::now();
        
        // Record metrics
        self.metrics.record_message_processed("ProcessBlock", Duration::from_millis(10));
        
        Ok(())
    }
    
    /// Handle UpdateHealth message  
    async fn handle_update_health(&mut self, healthy: bool) -> ActorResult<()> {
        info!(healthy = healthy, "Updating health status");
        
        self.state.is_healthy = healthy;
        
        if healthy {
            self.metrics.record_health_check_passed();
        } else {
            self.metrics.record_health_check_failed();
        }
        
        Ok(())
    }
    
    /// Handle GetStatus message
    async fn handle_get_status(&mut self) -> ActorResult<()> {
        info!(
            height = self.state.current_height,
            healthy = self.state.is_healthy,
            "Current actor status"
        );
        Ok(())
    }
}

// Implement message handlers using the enhanced macro
impl_message_handler!(ExampleBlockchainActor, ExampleMessage => ActorResult<()>, handle_message);

impl ExampleBlockchainActor {
    /// Unified message handler
    async fn handle_message(&mut self, msg: ExampleMessage) -> ActorResult<()> {
        match msg {
            ExampleMessage::ProcessBlock { height } => {
                self.handle_process_block(height).await
            }
            ExampleMessage::UpdateHealth { healthy } => {
                self.handle_update_health(healthy).await
            }
            ExampleMessage::GetStatus => {
                self.handle_get_status().await
            }
        }
    }
}

// Enhanced BlockchainAwareActor implementation
#[async_trait]
impl BlockchainAwareActor for ExampleBlockchainActor {
    fn timing_constraints(&self) -> BlockchainTimingConstraints {
        BlockchainTimingConstraints {
            block_interval: self.config.block_interval,
            max_consensus_latency: Duration::from_millis(50),
            federation_timeout: Duration::from_millis(200),
            auxpow_window: Duration::from_secs(300),
        }
    }
    
    fn blockchain_priority(&self) -> BlockchainActorPriority {
        BlockchainActorPriority::Network
    }
    
    async fn handle_blockchain_event(&mut self, event: BlockchainEvent) -> ActorResult<()> {
        match event {
            BlockchainEvent::BlockProduced { height, .. } => {
                info!(height = height, "Received block production event");
                self.state.current_height = height;
                Ok(())
            }
            BlockchainEvent::ConsensusFailure { reason } => {
                warn!(reason = %reason, "Consensus failure detected");
                self.state.is_healthy = false;
                Ok(())
            }
            _ => {
                debug!("Received other blockchain event: {:?}", event);
                Ok(())
            }
        }
    }
    
    async fn validate_blockchain_readiness(&self) -> ActorResult<BlockchainReadiness> {
        Ok(BlockchainReadiness {
            can_produce_blocks: self.state.is_healthy && self.state.current_height > 0,
            can_validate_blocks: self.state.is_healthy,
            federation_healthy: true,
            sync_status: if self.state.current_height > 0 {
                SyncStatus::Synced
            } else {
                SyncStatus::NotSynced
            },
            last_validated: SystemTime::now(),
        })
    }
}

// Standard actor lifecycle implementations
impl LifecycleAware for ExampleBlockchainActor {
    async fn on_start(&mut self, _ctx: &mut Context<Self>) -> ActorResult<()> {
        info!(actor_id = ?self.config.actor_id, "Example actor starting");
        Ok(())
    }
    
    async fn on_shutdown(&mut self, _timeout: Option<Duration>) -> ActorResult<()> {
        info!(actor_id = ?self.config.actor_id, "Example actor shutting down");
        Ok(())
    }
    
    async fn health_check(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.state.is_healthy)
    }
}

/// Factory for creating the example actor
pub struct ExampleActorFactory {
    config: ExampleConfig,
}

impl ExampleActorFactory {
    pub fn new(config: ExampleConfig) -> Self {
        Self { config }
    }
}

impl ActorFactory<ExampleBlockchainActor> for ExampleActorFactory {
    fn create(&self) -> ExampleBlockchainActor {
        ExampleBlockchainActor::new(self.config.clone()).expect("Failed to create example actor")
    }
    
    fn config(&self) -> SupervisedActorConfig {
        SupervisedActorConfig {
            restart_strategy: RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(10),
                multiplier: 2.0,
            },
            max_restarts: Some(5),
            restart_window: Duration::from_secs(60),
            escalation_strategy: EscalationStrategy::EscalateToParent,
        }
    }
}

/// Example function showing how to create and start the enhanced actors
pub async fn create_enhanced_actor_system() -> ActorResult<()> {
    info!("Creating enhanced actor system example");
    
    // Create actor configuration
    let config = ExampleConfig {
        actor_id: Some("example_blockchain_actor".to_string()),
        block_interval: Duration::from_secs(2),
        federation_threshold: 3,
    };
    
    // Create the actor using the blockchain factory
    let addr = create_consensus_actor("example_actor".to_string(), config).await?;
    
    // Send some test messages
    addr.try_send(ExampleMessage::ProcessBlock { height: 1 })
        .map_err(|_| ActorError::MessageDeliveryFailed {
            from: "system".to_string(),
            to: "example_actor".to_string(),
            reason: "Failed to send ProcessBlock message".to_string(),
        })?;
    
    addr.try_send(ExampleMessage::GetStatus)
        .map_err(|_| ActorError::MessageDeliveryFailed {
            from: "system".to_string(),
            to: "example_actor".to_string(),
            reason: "Failed to send GetStatus message".to_string(),
        })?;
    
    info!("Enhanced actor system example completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;
    
    #[actix::test]
    async fn test_enhanced_actor_creation() {
        let config = ExampleConfig::default();
        let actor = ExampleBlockchainActor::new(config).expect("Should create actor");
        
        assert_eq!(actor.blockchain_priority(), BlockchainActorPriority::Network);
        assert!(!actor.is_consensus_critical());
        
        let readiness = actor.validate_blockchain_readiness().await.expect("Should validate readiness");
        assert!(!readiness.can_produce_blocks); // Not synced yet
        assert!(readiness.can_validate_blocks); // Healthy
    }
    
    #[actix::test]
    async fn test_blockchain_event_handling() {
        let config = ExampleConfig::default();
        let mut actor = ExampleBlockchainActor::new(config).expect("Should create actor");
        
        let event = BlockchainEvent::BlockProduced { height: 42, hash: [0; 32] };
        actor.handle_blockchain_event(event).await.expect("Should handle event");
        
        assert_eq!(actor.state.current_height, 42);
    }
    
    #[actix::test]
    async fn test_message_handling() {
        let config = ExampleConfig::default();
        let mut actor = ExampleBlockchainActor::new(config).expect("Should create actor");
        
        let msg = ExampleMessage::ProcessBlock { height: 100 };
        actor.handle_message(msg).await.expect("Should handle message");
        
        assert_eq!(actor.state.current_height, 100);
        
        let health_msg = ExampleMessage::UpdateHealth { healthy: false };
        actor.handle_message(health_msg).await.expect("Should handle health update");
        
        assert!(!actor.state.is_healthy);
    }
}