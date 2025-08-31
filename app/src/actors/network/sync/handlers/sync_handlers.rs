//! SyncActor Synchronization Message Handlers
//! 
//! Contains handlers for core synchronization operations including
//! sync lifecycle, progress tracking, and production threshold management.

use actix::{Handler, Context, ResponseFuture};
use std::time::SystemTime;
use uuid::Uuid;

use crate::actors::network::messages::*;
use crate::actors::network::messages::sync_messages::*;
use crate::actors::network::sync::actor::SyncActor;
use crate::actors::network::sync::{SyncOperation, SyncStatus, OperationType};

impl Handler<StartSync> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<SyncResponse>>;

    fn handle(&mut self, msg: StartSync, _ctx: &mut Context<Self>) -> Self::Result {
        let mut actor = self.clone_for_async();
        
        Box::pin(async move {
            match actor.start_sync_operation(
                msg.from_height,
                msg.target_height,
                msg.sync_mode,
                msg.priority_peers,
            ).await {
                Ok(response) => Ok(Ok(response)),
                Err(error) => Ok(Err(error)),
            }
        })
    }
}

impl Handler<StopSync> for SyncActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: StopSync, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!("Stopping sync operations (force: {})", msg.force);
        
        if msg.force {
            // Force stop all operations immediately
            self.sync_operations.clear();
            self.state.progress.status = SyncStatus::Idle;
        } else {
            // Graceful stop - let current operations complete
            self.state.progress.status = SyncStatus::Idle;
        }

        Ok(Ok(()))
    }
}

impl Handler<GetSyncStatus> for SyncActor {
    type Result = NetworkActorResult<SyncStatus>;

    fn handle(&mut self, _msg: GetSyncStatus, _ctx: &mut Context<Self>) -> Self::Result {
        let status = self.get_sync_status();
        Ok(Ok(status))
    }
}

impl Handler<CanProduceBlocks> for SyncActor {
    type Result = NetworkActorResult<bool>;

    fn handle(&mut self, _msg: CanProduceBlocks, _ctx: &mut Context<Self>) -> Self::Result {
        let can_produce = self.can_produce_blocks();
        tracing::debug!("Block production check: {} (progress: {:.2}%)", 
            can_produce, self.state.progress.progress_percent * 100.0);
        Ok(Ok(can_produce))
    }
}

impl Handler<SyncProgressUpdate> for SyncActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: SyncProgressUpdate, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::debug!(
            "Sync progress update: height {} progress {:.2}% bps {:.1}",
            msg.current_height, msg.progress * 100.0, msg.blocks_per_second
        );

        // Update internal state
        self.state.progress.current_height = msg.current_height;
        self.state.progress.progress_percent = msg.progress;
        self.state.metrics.current_bps = msg.blocks_per_second;

        // Update metrics timestamp
        self.metrics.last_update = std::time::Instant::now();

        // Check if we've crossed the production threshold
        let can_produce = self.can_produce_blocks();
        if can_produce != self.state.progress.can_produce_blocks {
            self.state.progress.can_produce_blocks = can_produce;
            if can_produce {
                tracing::info!(
                    "🎯 Block production threshold reached! ({}% >= {}%)",
                    (msg.progress * 100.0).round(),
                    (self.config.production_threshold * 100.0).round()
                );

                // Notify ChainActor that block production is now allowed
                if let Some(chain_actor) = &self.chain_actor {
                    chain_actor.do_send(CanProduceBlocks);
                }
            }
        }

        Ok(Ok(()))
    }
}

impl Handler<SyncCompleted> for SyncActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: SyncCompleted, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::info!(
            "Sync completed! Height: {}, Duration: {:?}, Average BPS: {:.1}",
            msg.final_height, msg.duration, msg.average_bps
        );

        // Update final sync state
        self.state.progress.current_height = msg.final_height;
        self.state.progress.status = SyncStatus::Idle;
        self.state.progress.progress_percent = 1.0;
        self.state.progress.can_produce_blocks = true;

        // Update metrics
        self.metrics.total_blocks_synced = msg.total_blocks;
        self.metrics.average_bps = msg.average_bps;
        
        // Clear completed operations
        self.sync_operations.retain(|_, op| {
            op.status != SyncStatus::Completed
        });

        // Notify ChainActor that we're ready for block production
        if let Some(chain_actor) = &self.chain_actor {
            chain_actor.do_send(CanProduceBlocks);
        }

        Ok(Ok(()))
    }
}

impl Handler<SyncError> for SyncActor {
    type Result = NetworkActorResult<()>;

    fn handle(&mut self, msg: SyncError, _ctx: &mut Context<Self>) -> Self::Result {
        tracing::error!(
            "Sync error at height {:?}: {} (recoverable: {})",
            msg.height, msg.error, msg.recoverable
        );

        if msg.recoverable {
            // Attempt recovery
            self.state.progress.status = SyncStatus::Recovery;
            tracing::info!("Attempting sync recovery...");

            // Update error count for current operations
            for operation in self.sync_operations.values_mut() {
                operation.error_count += 1;
                if operation.error_count >= self.config.max_retries {
                    operation.status = SyncStatus::Failed;
                    tracing::error!("Operation {} failed after {} retries", 
                        operation.operation_id, operation.error_count);
                }
            }
        } else {
            // Non-recoverable error - stop sync
            self.state.progress.status = SyncStatus::Failed;
            self.sync_operations.clear();
            tracing::error!("Non-recoverable sync error - stopping all operations");
        }

        Ok(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::network::sync::config::SyncConfig;
    use actix::System;

    #[actix::test]
    async fn test_sync_progress_handler() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        let progress_msg = SyncProgressUpdate {
            current_height: 995,
            progress: 0.995,
            blocks_per_second: 100.0,
        };

        let result = sync_actor.handle(progress_msg, &mut Context::new());
        assert!(result.is_ok());
        
        // Should now be able to produce blocks (above 99.5% threshold)
        assert!(sync_actor.can_produce_blocks());
        assert_eq!(sync_actor.state.progress.current_height, 995);
    }

    #[actix::test]
    async fn test_can_produce_blocks_threshold() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        // Below threshold
        sync_actor.state.progress.progress_percent = 0.994;
        sync_actor.state.progress.can_produce_blocks = false;
        
        let can_produce_msg = CanProduceBlocks;
        let result = sync_actor.handle(can_produce_msg, &mut Context::new());
        assert!(result.is_ok());
        assert!(!result.unwrap().unwrap());

        // Above threshold
        sync_actor.state.progress.progress_percent = 0.996;
        sync_actor.state.progress.can_produce_blocks = true;
        
        let can_produce_msg = CanProduceBlocks;
        let result = sync_actor.handle(can_produce_msg, &mut Context::new());
        assert!(result.is_ok());
        assert!(result.unwrap().unwrap());
    }

    #[actix::test]
    async fn test_sync_error_handling() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        // Recoverable error
        let error_msg = SyncError {
            error: "Network timeout".to_string(),
            height: Some(100),
            recoverable: true,
        };
        
        let result = sync_actor.handle(error_msg, &mut Context::new());
        assert!(result.is_ok());
        assert_eq!(sync_actor.state.progress.status, SyncStatus::Recovery);

        // Non-recoverable error
        let fatal_error_msg = SyncError {
            error: "Corrupted state".to_string(),
            height: Some(100),
            recoverable: false,
        };
        
        let result = sync_actor.handle(fatal_error_msg, &mut Context::new());
        assert!(result.is_ok());
        assert_eq!(sync_actor.state.progress.status, SyncStatus::Failed);
    }
}