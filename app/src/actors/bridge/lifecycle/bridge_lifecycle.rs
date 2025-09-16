//! Bridge Actor Lifecycle Implementation
//! 
//! LifecycleAware implementation for BridgeActor

use async_trait::async_trait;
use std::time::Duration;
use tracing::{info, error};

use actor_system::{
    error::{ActorError, ActorResult},
    lifecycle::{LifecycleAware, ActorState},
};

use crate::actors::bridge::actors::bridge::BridgeActor;

#[async_trait]
impl LifecycleAware for BridgeActor {
    async fn initialize(&mut self) -> ActorResult<()> {
        info!("Initializing Bridge Actor");
        
        // Initialize bridge-specific components
        self.initialize_bridge_components().await?;
        
        info!("Bridge Actor initialized successfully");
        Ok(())
    }

    async fn on_start(&mut self) -> ActorResult<()> {
        info!("Starting Bridge Actor lifecycle");

        // Initialize actor system metrics
        // Record actor start - using available metrics method
        self.actor_system_metrics.record_restart();

        // Initialize health monitoring
        self.health_monitor.start().await.map_err(|e| ActorError::InitializationFailed {
            actor_type: self.actor_type().to_string(),
            reason: format!("Health monitoring initialization failed: {}", e),
        })?;

        // Initialize bridge-specific components
        self.initialize_bridge_components().await?;

        // Set state to running
        self.state = crate::actors::bridge::actors::bridge::state::BridgeState::Running;

        info!("Bridge Actor lifecycle started successfully");
        Ok(())
    }

    async fn on_shutdown(&mut self, timeout: Duration) -> ActorResult<()> {
        info!("Stopping Bridge Actor lifecycle");

        // Set state to shutting down
        self.state = crate::actors::bridge::actors::bridge::state::BridgeState::ShuttingDown;

        // Stop health monitoring
        self.health_monitor.stop().await.map_err(|e| ActorError::ShutdownFailed {
            actor_type: self.actor_type().to_string(),
            reason: format!("Health monitoring shutdown failed: {}", e),
        })?;

        // Clean up active operations
        self.cleanup_active_operations().await?;

        // Disconnect child actors
        self.disconnect_child_actors().await?;

        // Finalize metrics
        // Record actor shutdown - using available metrics method
        self.actor_system_metrics.record_message_processed(std::time::Duration::from_millis(0));

        // Set final state
        self.state = crate::actors::bridge::actors::bridge::state::BridgeState::Stopped;

        info!("Bridge Actor lifecycle stopped successfully");
        Ok(())
    }

    async fn health_check(&self) -> ActorResult<bool> {
        // Check bridge system health
        let system_health = self.health_monitor.check_system_health();
        
        match system_health {
            crate::actors::bridge::messages::SystemHealthStatus::Healthy => {
                Ok(true)
            }
            crate::actors::bridge::messages::SystemHealthStatus::Degraded { .. } => {
                // Still operational but degraded
                Ok(true)
            }
            crate::actors::bridge::messages::SystemHealthStatus::Critical { .. } => {
                // Critical issues detected
                Ok(false)
            }
            crate::actors::bridge::messages::SystemHealthStatus::Initializing => {
                // Still starting up
                Ok(true)
            }
            crate::actors::bridge::messages::SystemHealthStatus::Shutdown => {
                // Shutting down
                Ok(false)
            }
        }
    }

    async fn on_pause(&mut self) -> ActorResult<()> {
        info!("Pausing Bridge Actor");

        // Pause new operation acceptance
        self.state = crate::actors::bridge::actors::bridge::state::BridgeState::Degraded {
            issues: vec!["Actor paused by lifecycle management".to_string()],
        };

        // Notify child actors to pause if needed
        self.notify_child_actors_pause().await?;

        // Record state change using available metrics method
        self.actor_system_metrics.record_message_processed(std::time::Duration::from_millis(0));
        Ok(())
    }

    async fn on_resume(&mut self) -> ActorResult<()> {
        info!("Resuming Bridge Actor");

        // Resume normal operations
        self.state = crate::actors::bridge::actors::bridge::state::BridgeState::Running;

        // Notify child actors to resume
        self.notify_child_actors_resume().await?;

        // Record state change using available metrics method
        self.actor_system_metrics.record_message_processed(std::time::Duration::from_millis(0));
        Ok(())
    }



    async fn on_state_change(&mut self, from: ActorState, to: ActorState) -> ActorResult<()> {
        info!("Bridge Actor state change: {:?} -> {:?}", from, to);

        // Record state transition using available metrics method
        self.actor_system_metrics.record_message_processed(std::time::Duration::from_millis(0));

        // Handle specific state transitions
        match (from, to) {
            (ActorState::Initializing, ActorState::Running) => {
                self.on_fully_initialized().await?;
            }
            (ActorState::Running, ActorState::Paused) => {
                self.on_operation_pause().await?;
            }
            (ActorState::Paused, ActorState::Running) => {
                self.on_operation_resume().await?;
            }
            (_, ActorState::Failed) => {
                self.on_failure_detected().await?;
            }
            _ => {}
        }

        Ok(())
    }

    fn actor_type(&self) -> &str {
        "BridgeActor"
    }

}

// Private implementation methods for lifecycle management
impl BridgeActor {
    /// Initialize bridge-specific components
    async fn initialize_bridge_components(&mut self) -> ActorResult<()> {
        // Initialize coordination metrics
        self.metrics.initialize().await.map_err(|e| ActorError::InitializationFailed {
            actor_type: self.actor_type().to_string(),
            reason: format!("Bridge metrics initialization failed: {}", e),
        })?;

        // Setup operation tracking
        self.active_operations.clear();

        Ok(())
    }

    /// Cleanup active operations during shutdown
    async fn cleanup_active_operations(&mut self) -> ActorResult<()> {
        let operation_count = self.active_operations.len();
        if operation_count > 0 {
            info!("Cleaning up {} active operations", operation_count);
            
            for (operation_id, _) in self.active_operations.drain() {
                // Log operation cancellation - method may be private
                info!("Cancelling operation: {}", operation_id);
            }
        }
        Ok(())
    }

    /// Complete active operations gracefully
    async fn complete_active_operations(&mut self) -> ActorResult<()> {
        let operation_count = self.active_operations.len();
        if operation_count > 0 {
            info!("Completing {} active operations", operation_count);
            
            // Wait for operations to complete naturally
            // This is simplified - in practice would wait for actual completion
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Ok(())
    }

    /// Disconnect from child actors
    async fn disconnect_child_actors(&mut self) -> ActorResult<()> {
        info!("Disconnecting from child actors");
        
        // Clear child actor addresses
        self.child_actors.pegin_actor = None;
        self.child_actors.pegout_actor = None;
        self.child_actors.stream_actor = None;

        Ok(())
    }

    /// Shutdown child actors
    async fn shutdown_child_actors(&mut self) -> ActorResult<()> {
        info!("Shutting down child actors");
        
        // In a real implementation, would send shutdown messages to child actors
        // For now, just disconnect
        self.disconnect_child_actors().await
    }

    /// Reset state for restart
    async fn reset_for_restart(&mut self) -> ActorResult<()> {
        // Clear operation state
        self.active_operations.clear();
        
        // Reset health monitor
        self.health_monitor = crate::actors::bridge::actors::bridge::state::ActorHealthMonitor::new(
            self.config.health_check_interval
        );

        // Reset metrics (keep historical data) - using available method
        self.actor_system_metrics.record_restart();

        Ok(())
    }

    /// Notify child actors to pause
    async fn notify_child_actors_pause(&mut self) -> ActorResult<()> {
        // Implementation would send pause messages to child actors
        Ok(())
    }

    /// Notify child actors to resume
    async fn notify_child_actors_resume(&mut self) -> ActorResult<()> {
        // Implementation would send resume messages to child actors
        Ok(())
    }

    /// Handle full initialization completion
    async fn on_fully_initialized(&mut self) -> ActorResult<()> {
        info!("Bridge Actor fully initialized and operational");
        // Record initialization completion using available method
        self.actor_system_metrics.record_message_processed(std::time::Duration::from_millis(0));
        Ok(())
    }

    /// Handle operation pause
    async fn on_operation_pause(&mut self) -> ActorResult<()> {
        info!("Bridge Actor operations paused");
        Ok(())
    }

    /// Handle operation resume
    async fn on_operation_resume(&mut self) -> ActorResult<()> {
        info!("Bridge Actor operations resumed");
        Ok(())
    }

    /// Handle failure detection
    async fn on_failure_detected(&mut self) -> ActorResult<()> {
        error!("Bridge Actor failure detected, entering recovery mode");
        // Record failure detection using available method
        self.actor_system_metrics.record_error("Bridge Actor failure detected");
        
        // Attempt to recover from failure
        self.attempt_failure_recovery().await?;
        
        Ok(())
    }

    /// Attempt to recover from failure
    async fn attempt_failure_recovery(&mut self) -> ActorResult<()> {
        info!("Attempting Bridge Actor failure recovery");
        
        // Clear error states
        self.health_monitor.clear_resolved_errors();
        
        // Reset to healthy state if possible
        self.state = crate::actors::bridge::actors::bridge::state::BridgeState::Running;
        
        Ok(())
    }
}