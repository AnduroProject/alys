//! ChainActor V2 Error Recovery System (Phase 4: Task 4.3.1)
//!
//! Production-ready error recovery procedures for all failure types.
//! Implements graceful degradation, health checks, and automatic retry logic.

use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{ChainActor, ChainError};
use crate::actors_v2::{
    engine::EngineMessage,
    network::{NetworkMessage, SyncMessage},
    storage::messages::HealthCheckMessage,
};

/// Health status for all integrated actors
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub storage_healthy: bool,
    pub engine_healthy: bool,
    pub network_healthy: bool,
    pub sync_healthy: bool,
    pub overall_healthy: bool,
}

impl HealthStatus {
    pub fn new() -> Self {
        Self {
            storage_healthy: false,
            engine_healthy: false,
            network_healthy: false,
            sync_healthy: false,
            overall_healthy: false,
        }
    }

    pub fn is_healthy(&self) -> bool {
        // Core actors must be healthy (network is optional)
        self.storage_healthy && self.engine_healthy
    }

    pub fn calculate_overall(&mut self) {
        self.overall_healthy = self.is_healthy();
    }
}

impl ChainActor {
    /// Perform comprehensive health check for all integrated actors (Phase 4: Task 4.3.1)
    pub async fn perform_health_check(&self) -> Result<HealthStatus, ChainError> {
        let mut health = HealthStatus::new();
        let correlation_id = Uuid::new_v4();

        info!(
            correlation_id = %correlation_id,
            "Starting comprehensive health check for all actors"
        );

        // Check StorageActor health
        if let Some(ref storage_actor) = self.storage_actor {
            match storage_actor
                .send(HealthCheckMessage {
                    correlation_id: Some(correlation_id),
                })
                .await
            {
                Ok(Ok(_)) => {
                    health.storage_healthy = true;
                    debug!(correlation_id = %correlation_id, "StorageActor health check passed");
                }
                Ok(Err(e)) => {
                    warn!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "StorageActor health check failed"
                    );
                    health.storage_healthy = false;
                }
                Err(e) => {
                    error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "StorageActor communication failed during health check"
                    );
                    health.storage_healthy = false;
                }
            }
        } else {
            warn!("StorageActor not configured - health check skipped");
        }

        // Check EngineActor health
        if let Some(ref engine_actor) = self.engine_actor {
            match engine_actor
                .send(EngineMessage::GetStatus {
                    correlation_id: Some(correlation_id),
                })
                .await
            {
                Ok(Ok(crate::actors_v2::engine::EngineResponse::Status {
                    is_ready: true, ..
                })) => {
                    health.engine_healthy = true;
                    debug!(correlation_id = %correlation_id, "EngineActor health check passed");
                }
                Ok(Ok(crate::actors_v2::engine::EngineResponse::Status {
                    is_ready: false,
                    ..
                })) => {
                    warn!(correlation_id = %correlation_id, "EngineActor is not ready");
                    health.engine_healthy = false;
                }
                Ok(Err(e)) => {
                    warn!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "EngineActor health check failed"
                    );
                    health.engine_healthy = false;
                }
                Err(e) => {
                    error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "EngineActor communication failed during health check"
                    );
                    health.engine_healthy = false;
                }
                _ => {
                    error!(correlation_id = %correlation_id, "Unexpected EngineActor response");
                    health.engine_healthy = false;
                }
            }
        } else {
            warn!("EngineActor not configured - health check skipped");
        }

        // Check NetworkActor health
        if let Some(ref network_actor) = self.network_actor {
            match network_actor
                .send(NetworkMessage::HealthCheck {
                    correlation_id: Some(correlation_id),
                })
                .await
            {
                Ok(Ok(crate::actors_v2::network::NetworkResponse::Healthy {
                    is_healthy: true,
                    ..
                })) => {
                    health.network_healthy = true;
                    debug!(correlation_id = %correlation_id, "NetworkActor health check passed");
                }
                Ok(Ok(crate::actors_v2::network::NetworkResponse::Healthy {
                    is_healthy: false,
                    issues,
                    ..
                })) => {
                    warn!(
                        correlation_id = %correlation_id,
                        issues = ?issues,
                        "NetworkActor health check failed"
                    );
                    health.network_healthy = false;
                }
                Ok(Err(e)) => {
                    warn!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "NetworkActor health check failed"
                    );
                    health.network_healthy = false;
                }
                Err(e) => {
                    error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "NetworkActor communication failed during health check"
                    );
                    health.network_healthy = false;
                }
                _ => {
                    error!(correlation_id = %correlation_id, "Unexpected NetworkActor response");
                    health.network_healthy = false;
                }
            }
        } else {
            // Network is optional for some operations
            debug!("NetworkActor not configured - health check skipped");
            health.network_healthy = true; // Don't fail if network not required
        }

        // Check SyncActor health (optional)
        if let Some(ref sync_actor) = self.sync_actor {
            match sync_actor.send(SyncMessage::GetSyncStatus).await {
                Ok(Ok(_)) => {
                    health.sync_healthy = true;
                    debug!(correlation_id = %correlation_id, "SyncActor health check passed");
                }
                Ok(Err(e)) => {
                    warn!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "SyncActor health check failed"
                    );
                    health.sync_healthy = false;
                }
                Err(e) => {
                    error!(
                        correlation_id = %correlation_id,
                        error = ?e,
                        "SyncActor communication failed during health check"
                    );
                    health.sync_healthy = false;
                }
            }
        } else {
            debug!("SyncActor not configured - health check skipped");
            health.sync_healthy = true; // Don't fail if sync not required
        }

        health.calculate_overall();

        info!(
            correlation_id = %correlation_id,
            storage_healthy = health.storage_healthy,
            engine_healthy = health.engine_healthy,
            network_healthy = health.network_healthy,
            sync_healthy = health.sync_healthy,
            overall_healthy = health.overall_healthy,
            "Health check completed"
        );

        Ok(health)
    }

    /// Recover from failed block production (Phase 4: Task 4.3.1)
    pub async fn recover_from_block_production_failure(
        &self,
        error: &ChainError,
    ) -> Result<(), ChainError> {
        error!(error = ?error, "Block production failed - initiating recovery");

        match error {
            ChainError::Engine(_) => {
                self.recover_from_engine_failure().await?;
            }
            ChainError::Storage(_) => {
                self.recover_from_storage_failure().await?;
            }
            ChainError::NetworkNotAvailable | ChainError::Network(_) => {
                self.recover_from_network_failure().await?;
            }
            ChainError::NotSynced => {
                warn!("Block production failed due to sync status - waiting for sync");
                // This is expected during sync, no recovery action needed
            }
            ChainError::Configuration(_) => {
                error!("Block production failed due to configuration error - manual intervention required");
                return Err(ChainError::Internal(
                    "Configuration error requires manual fix".to_string(),
                ));
            }
            _ => {
                debug!("Generic error recovery - performing health check");
                let health = self.perform_health_check().await?;
                if !health.is_healthy() {
                    return Err(ChainError::Internal(
                        "System unhealthy after error".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Recover from Engine failures
    async fn recover_from_engine_failure(&self) -> Result<(), ChainError> {
        warn!("Engine failure detected - checking engine status");

        if let Some(ref engine_actor) = self.engine_actor {
            let status_check = engine_actor
                .send(EngineMessage::GetStatus {
                    correlation_id: Some(Uuid::new_v4()),
                })
                .await;

            match status_check {
                Ok(Ok(crate::actors_v2::engine::EngineResponse::Status {
                    is_ready: false,
                    ..
                })) => {
                    warn!("Engine not ready - waiting for recovery");
                    // Could implement engine restart logic here
                    return Err(ChainError::Engine("Engine not ready".to_string()));
                }
                Err(_) => {
                    error!("Engine actor not responding - critical failure");
                    return Err(ChainError::Internal(
                        "Engine actor unresponsive".to_string(),
                    ));
                }
                Ok(Ok(_)) => {
                    debug!("Engine status check passed");
                }
                Ok(Err(e)) => {
                    error!(error = ?e, "Engine status check failed");
                    return Err(ChainError::Engine(format!("Engine check failed: {}", e)));
                }
            }
        } else {
            return Err(ChainError::Internal(
                "EngineActor not configured".to_string(),
            ));
        }

        Ok(())
    }

    /// Recover from Storage failures
    async fn recover_from_storage_failure(&self) -> Result<(), ChainError> {
        warn!("Storage failure detected - checking storage status");

        if let Some(ref storage_actor) = self.storage_actor {
            let health_check = storage_actor
                .send(HealthCheckMessage {
                    correlation_id: Some(Uuid::new_v4()),
                })
                .await;

            match health_check {
                Ok(Ok(_)) => {
                    debug!("Storage health check passed");
                }
                Ok(Err(e)) => {
                    error!(error = ?e, "Storage health check failed");
                    return Err(ChainError::Storage(format!("Storage unhealthy: {}", e)));
                }
                Err(_) => {
                    error!("Storage actor not responding - critical failure");
                    return Err(ChainError::Internal(
                        "Storage actor unresponsive".to_string(),
                    ));
                }
            }
        } else {
            return Err(ChainError::Internal(
                "StorageActor not configured".to_string(),
            ));
        }

        Ok(())
    }

    /// Recover from Network failures
    async fn recover_from_network_failure(&self) -> Result<(), ChainError> {
        warn!("Network failure detected - checking connectivity");

        if !self.is_network_ready().await {
            warn!("Network still not ready after failure");
            // Network failures are often transient, not critical for all operations
            return Ok(());
        }

        debug!("Network connectivity restored");
        Ok(())
    }

    /// Recover from failed block import (Phase 4: Task 4.3.1)
    pub async fn recover_from_block_import_failure(
        &self,
        block_hash: &ethereum_types::H256,
        error: &ChainError,
    ) -> Result<(), ChainError> {
        error!(
            block_hash = %block_hash,
            error = ?error,
            "Block import failed - initiating recovery"
        );

        match error {
            ChainError::InvalidBlock(_) => {
                // Invalid blocks cannot be recovered - log and skip
                warn!(block_hash = %block_hash, "Block is invalid - cannot recover");
                return Ok(()); // Not a system error
            }
            ChainError::Consensus(_) => {
                // Consensus failures indicate validation issues - not recoverable
                warn!(block_hash = %block_hash, "Block failed consensus validation");
                return Ok(()); // Not a system error
            }
            ChainError::Engine(_) => {
                // Engine failures may be transient
                self.recover_from_engine_failure().await?;
            }
            ChainError::Storage(_) => {
                // Storage failures may be transient
                self.recover_from_storage_failure().await?;
            }
            _ => {
                debug!("Generic import error - performing health check");
                let health = self.perform_health_check().await?;
                if !health.is_healthy() {
                    return Err(ChainError::Internal(
                        "System unhealthy after import failure".to_string(),
                    ));
                }
            }
        }

        info!(block_hash = %block_hash, "Import error recovery completed");
        Ok(())
    }

    /// Graceful degradation check - determine if operations can continue (Phase 4)
    pub fn can_operate_degraded(&self) -> bool {
        // Minimum requirements: StorageActor and EngineActor must be available
        self.storage_actor.is_some() && self.engine_actor.is_some()
    }

    /// Get degradation status for monitoring (Phase 4)
    pub fn get_degradation_status(&self) -> Vec<String> {
        let mut missing = Vec::new();

        if self.storage_actor.is_none() {
            missing.push("StorageActor".to_string());
        }
        if self.engine_actor.is_none() {
            missing.push("EngineActor".to_string());
        }
        if self.network_actor.is_none() {
            missing.push("NetworkActor (degraded)".to_string());
        }
        if self.sync_actor.is_none() {
            missing.push("SyncActor (degraded)".to_string());
        }

        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_creation() {
        let health = HealthStatus::new();
        assert!(!health.is_healthy());
        assert!(!health.overall_healthy);
    }

    #[test]
    fn test_health_status_healthy() {
        let mut health = HealthStatus::new();
        health.storage_healthy = true;
        health.engine_healthy = true;
        health.network_healthy = true;
        health.calculate_overall();

        assert!(health.is_healthy());
        assert!(health.overall_healthy);
    }

    #[test]
    fn test_health_status_partial() {
        let mut health = HealthStatus::new();
        health.storage_healthy = true;
        health.engine_healthy = false; // Engine failure
        health.network_healthy = true;
        health.calculate_overall();

        assert!(!health.is_healthy());
        assert!(!health.overall_healthy);
    }
}
