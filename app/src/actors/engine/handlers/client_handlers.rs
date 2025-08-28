//! Client Handler Implementation
//!
//! Handles execution client lifecycle management, health checks, and connection management.

use std::time::{Duration, Instant, SystemTime};
use tracing::*;
use actix::prelude::*;

use crate::types::*;
use super::super::{
    actor::{EngineActor, HealthCheckResult},
    messages::*,
    state::ExecutionState,
    client::{HealthCheck, ClientCapabilities},
    EngineError, EngineResult,
};

impl Handler<HealthCheckMessage> for EngineActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, _msg: HealthCheckMessage, _ctx: &mut Self::Context) -> Self::Result {
        let client = self.client.clone();
        let max_failures = self.config.max_health_failures;
        
        Box::pin(async move {
            let check_start = Instant::now();
            
            // Perform health check on execution client
            let health_check = client.health_check().await;
            let check_duration = check_start.elapsed();
            
            debug!(
                reachable = %health_check.reachable,
                response_time_ms = %health_check.response_time.as_millis(),
                error = ?health_check.error,
                "Health check completed"
            );
            
            // This would typically update the actor's internal state
            // For now, we just log the result
            if health_check.reachable {
                info!("Execution client health check passed");
            } else {
                warn!("Execution client health check failed: {:?}", health_check.error);
            }
        })
    }
}

impl Handler<GetEngineStatusMessage> for EngineActor {
    type Result = MessageResult<EngineStatusResponse>;

    fn handle(&mut self, msg: GetEngineStatusMessage, _ctx: &mut Self::Context) -> Self::Result {
        debug!(
            include_metrics = %msg.include_metrics,
            include_payloads = %msg.include_payloads,
            "Getting engine status"
        );

        let metrics = if msg.include_metrics {
            Some(EnginePerformanceMetrics {
                payloads_built: self.metrics.payloads_built,
                payloads_executed: self.metrics.payloads_executed,
                failures: self.metrics.failures,
                avg_build_time_ms: self.state.metrics.avg_build_time.as_millis() as u64,
                avg_execution_time_ms: self.state.metrics.avg_execution_time.as_millis() as u64,
                success_rate: self.calculate_success_rate(),
                client_uptime: self.state.metrics.client_uptime,
            })
        } else {
            None
        };

        let payload_details = if msg.include_payloads {
            Some(self.get_payload_details())
        } else {
            None
        };

        let response = EngineStatusResponse {
            execution_state: self.state.execution_state.clone(),
            client_healthy: self.health_monitor.is_healthy,
            pending_payloads: self.state.pending_payloads.len(),
            metrics,
            payload_details,
            uptime: self.started_at.elapsed(),
        };

        Ok(response)
    }
}

impl Handler<ShutdownEngineMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<()>>;

    fn handle(&mut self, msg: ShutdownEngineMessage, ctx: &mut Self::Context) -> Self::Result {
        let timeout = msg.timeout;
        let wait_for_pending = msg.wait_for_pending;
        let pending_count = self.state.pending_payloads.len();
        
        info!(
            timeout_ms = %timeout.as_millis(),
            wait_for_pending = %wait_for_pending,
            pending_payloads = %pending_count,
            "Initiating graceful engine shutdown"
        );

        // Stop periodic tasks immediately
        self.stop_periodic_tasks();
        
        // Update state to indicate shutdown in progress
        self.state.transition_state(
            ExecutionState::Error {
                message: "Shutdown in progress".to_string(),
                occurred_at: SystemTime::now(),
                recoverable: false,
                recovery_attempts: 0,
            },
            "Graceful shutdown initiated".to_string()
        );

        Box::pin(async move {
            if wait_for_pending && pending_count > 0 {
                info!("Waiting for {} pending payloads to complete", pending_count);
                
                // TODO: Implement waiting for pending operations to complete
                // This would involve monitoring the pending_payloads map and waiting
                // until all operations are complete or the timeout is reached
                
                tokio::time::sleep(Duration::from_millis(100)).await; // Placeholder
            }
            
            info!("Engine actor graceful shutdown completed");
            
            // Stop the actor context
            ctx.stop();
            
            Ok(())
        })
    }
}

impl Handler<RestartEngineMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<()>>;

    fn handle(&mut self, msg: RestartEngineMessage, ctx: &mut Self::Context) -> Self::Result {
        let reason = msg.reason.clone();
        let preserve_state = msg.preserve_state;
        
        warn!(
            reason = %reason,
            preserve_state = %preserve_state,
            "Restarting engine actor"
        );

        // Update metrics
        self.metrics.actor_restarted();
        
        // Clear or preserve state based on request
        if !preserve_state {
            self.state.pending_payloads.clear();
            info!("Cleared pending payloads due to restart");
        }
        
        // Update state
        self.state.transition_state(
            ExecutionState::Initializing,
            format!("Actor restart: {}", reason)
        );

        let client = self.client.clone();
        let config = self.config.clone();

        Box::pin(async move {
            // Attempt to reconnect to execution client
            match client.reconnect().await {
                Ok(_) => {
                    info!("Successfully reconnected to execution client during restart");
                },
                Err(e) => {
                    error!("Failed to reconnect during restart: {}", e);
                    return Err(e);
                }
            }
            
            info!("Engine actor restart completed successfully");
            Ok(())
        })
    }
}

impl Handler<UpdateConfigMessage> for EngineActor {
    type Result = ResponseFuture<MessageResult<()>>;

    fn handle(&mut self, msg: UpdateConfigMessage, ctx: &mut Self::Context) -> Self::Result {
        let new_config = msg.config;
        let restart_if_needed = msg.restart_if_needed;
        let current_config = self.config.clone();
        
        info!("Updating engine configuration");

        Box::pin(async move {
            // Validate new configuration
            if let Err(e) = new_config.validate() {
                error!("Invalid configuration provided: {}", e);
                return Err(e);
            }
            
            // Check if restart is needed (e.g., URL changes)
            let needs_restart = current_config.engine_url != new_config.engine_url ||
                               current_config.public_url != new_config.public_url ||
                               current_config.jwt_secret != new_config.jwt_secret;
            
            if needs_restart && restart_if_needed {
                info!("Configuration change requires restart, initiating restart");
                
                // Send restart message to self
                ctx.address().send(RestartEngineMessage {
                    reason: "Configuration update".to_string(),
                    preserve_state: true,
                }).await??;
            } else if needs_restart {
                warn!("Configuration change requires restart but restart_if_needed is false");
                return Err(EngineError::ConfigError(
                    "Configuration change requires restart".to_string()
                ));
            }
            
            // Update configuration (this would be done in the actual implementation)
            info!("Configuration updated successfully");
            Ok(())
        })
    }
}

impl EngineActor {
    /// Calculate success rate for metrics
    fn calculate_success_rate(&self) -> f64 {
        let total_operations = self.metrics.payloads_built + self.metrics.payloads_executed;
        if total_operations == 0 {
            1.0 // No operations yet, consider 100% success
        } else {
            let successful = total_operations - self.metrics.failures;
            successful as f64 / total_operations as f64
        }
    }
    
    /// Get details about pending payloads for status reporting
    fn get_payload_details(&self) -> Vec<PayloadDetails> {
        let now = Instant::now();
        
        self.state.pending_payloads
            .iter()
            .map(|(id, payload)| {
                PayloadDetails {
                    payload_id: id.clone(),
                    status: payload.status.clone(),
                    age_ms: now.duration_since(payload.created_at).as_millis() as u64,
                    priority: payload.priority.clone(),
                    retry_attempts: payload.retry_attempts,
                }
            })
            .collect()
    }
    
    /// Perform comprehensive health check
    pub(super) async fn perform_health_check(&mut self) -> HealthCheckResult {
        let check_start = Instant::now();
        
        // Check client connectivity
        let client_healthy = self.engine.is_healthy().await;
        
        // Check sync status
        let sync_check = if client_healthy {
            match self.engine.is_syncing().await {
                Ok(is_syncing) => !is_syncing, // Healthy if not syncing
                Err(_) => false,
            }
        } else {
            false
        };
        
        let check_duration = check_start.elapsed();
        let overall_healthy = client_healthy && sync_check;
        
        let error = if !overall_healthy {
            Some(format!(
                "Health check failed: client_healthy={}, sync_healthy={}",
                client_healthy, sync_check
            ))
        } else {
            None
        };
        
        let result = HealthCheckResult {
            timestamp: check_start,
            passed: overall_healthy,
            duration: check_duration,
            error,
        };
        
        // Update health monitor
        self.health_monitor.record_health_check(
            overall_healthy,
            check_duration,
            result.error.clone()
        );
        
        // Update execution state if health changed significantly
        if !overall_healthy && self.health_monitor.consecutive_failures >= self.config.max_health_failures {
            self.state.transition_state(
                ExecutionState::Error {
                    message: "Client health check failed repeatedly".to_string(),
                    occurred_at: SystemTime::now(),
                    recoverable: true,
                    recovery_attempts: 0,
                },
                "Health check failure threshold exceeded".to_string()
            );
        }
        
        result
    }
    
    /// Attempt to recover from client errors
    pub(super) async fn attempt_client_recovery(&mut self) -> EngineResult<()> {
        info!("Attempting client recovery");
        
        match &mut self.state.execution_state {
            ExecutionState::Error { recovery_attempts, .. } => {
                *recovery_attempts += 1;
                
                if *recovery_attempts > 5 {
                    error!("Maximum recovery attempts exceeded");
                    return Err(EngineError::ClientError(
                        super::super::ClientError::ConnectionFailed(
                            "Maximum recovery attempts exceeded".to_string()
                        )
                    ));
                }
                
                // Attempt reconnection
                match self.client.reconnect().await {
                    Ok(_) => {
                        info!("Client reconnection successful");
                        
                        self.state.transition_state(
                            ExecutionState::Initializing,
                            "Recovery successful, reinitializing".to_string()
                        );
                        
                        // Reset health monitor
                        self.health_monitor.consecutive_failures = 0;
                        self.health_monitor.is_healthy = true;
                        
                        Ok(())
                    },
                    Err(e) => {
                        warn!("Client reconnection failed: {}", e);
                        Err(e)
                    }
                }
            },
            other_state => {
                debug!("Client recovery called in state: {:?}", other_state);
                Ok(())
            }
        }
    }
}

/// Handler for CleanupExpiredPayloadsMessage - cleans up expired payloads
impl Handler<CleanupExpiredPayloadsMessage> for EngineActor {
    type Result = ();

    fn handle(&mut self, _msg: CleanupExpiredPayloadsMessage, _ctx: &mut Self::Context) -> Self::Result {
        let now = std::time::Instant::now();
        let expiry_threshold = Duration::from_secs(300); // 5 minutes
        
        let expired_payloads: Vec<String> = self.state.pending_payloads
            .iter()
            .filter(|(_, payload)| {
                now.duration_since(payload.created_at) > expiry_threshold
            })
            .map(|(id, _)| id.clone())
            .collect();
        
        let expired_count = expired_payloads.len();
        if expired_count > 0 {
            info!("Cleaning up {} expired payloads", expired_count);
            
            for payload_id in expired_payloads {
                self.state.remove_pending_payload(&payload_id);
                self.metrics.payloads_expired += 1;
            }
            
            debug!("Payload cleanup completed, {} payloads remaining", 
                   self.state.pending_payloads.len());
        }
    }
}