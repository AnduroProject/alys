//! Sync Handler Implementation
//!
//! Handles engine synchronization status monitoring and sync-related operations.

use std::time::{Duration, Instant, SystemTime};
use tracing::*;
use actix::prelude::*;

use crate::types::*;
use super::super::{
    actor::EngineActor,
    messages::*,
    state::{ExecutionState, SyncStatus},
    EngineError, EngineResult,
};

/// Message to check engine sync status
#[derive(Message, Debug, Clone)]
#[rtype(result = "EngineResult<EngineSyncStatus>")]
pub struct CheckSyncStatusMessage {
    /// Include detailed sync information
    pub include_details: bool,
}

/// Engine sync status response
#[derive(Debug, Clone)]
pub struct EngineSyncStatus {
    /// Whether the engine is synced
    pub is_synced: bool,
    
    /// Current execution state
    pub execution_state: ExecutionState,
    
    /// Sync progress if available
    pub sync_progress: Option<SyncProgress>,
    
    /// Client health status
    pub client_healthy: bool,
    
    /// Last sync check timestamp
    pub last_checked: SystemTime,
}

/// Detailed sync progress information
#[derive(Debug, Clone)]
pub struct SyncProgress {
    /// Current block height
    pub current_block: u64,
    
    /// Target block height
    pub target_block: u64,
    
    /// Sync progress percentage (0.0 to 1.0)
    pub progress_percentage: f64,
    
    /// Estimated time remaining
    pub eta: Option<Duration>,
    
    /// Sync speed (blocks per second)
    pub blocks_per_second: f64,
}

/// Message to handle sync status changes from external sources
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct SyncStatusChangedMessage {
    /// New sync status
    pub synced: bool,
    
    /// Current block height
    pub current_height: u64,
    
    /// Target height (if known)
    pub target_height: Option<u64>,
    
    /// Source of the sync status update
    pub source: SyncStatusSource,
}

/// Source of sync status information
#[derive(Debug, Clone)]
pub enum SyncStatusSource {
    /// Update from execution client
    ExecutionClient,
    /// Update from consensus layer
    ConsensusLayer,
    /// Update from network layer
    NetworkLayer,
    /// Internal health check
    HealthCheck,
}

impl Handler<CheckSyncStatusMessage> for EngineActor {
    type Result = ResponseFuture<EngineResult<EngineSyncStatus>>;

    fn handle(&mut self, msg: CheckSyncStatusMessage, _ctx: &mut Self::Context) -> Self::Result {
        let engine = self.engine.clone();
        let client_health = self.health_monitor.is_healthy;
        let execution_state = self.state.execution_state.clone();
        
        debug!(
            include_details = %msg.include_details,
            "Checking engine sync status"
        );

        Box::pin(async move {
            let check_start = Instant::now();
            
            // Check if client is healthy first
            if !client_health {
                warn!("Cannot check sync status: client is unhealthy");
                return Ok(EngineSyncStatus {
                    is_synced: false,
                    execution_state,
                    sync_progress: None,
                    client_healthy: false,
                    last_checked: SystemTime::now(),
                });
            }
            
            // Get sync status from execution client
            match engine.is_syncing().await {
                Ok(is_syncing) => {
                    let sync_progress = if msg.include_details && is_syncing {
                        // Get detailed sync information
                        match get_detailed_sync_progress(&engine).await {
                            Ok(progress) => Some(progress),
                            Err(e) => {
                                warn!("Failed to get detailed sync progress: {}", e);
                                None
                            }
                        }
                    } else {
                        None
                    };
                    
                    let check_duration = check_start.elapsed();
                    
                    debug!(
                        is_syncing = %is_syncing,
                        check_time_ms = %check_duration.as_millis(),
                        "Sync status check completed"
                    );
                    
                    Ok(EngineSyncStatus {
                        is_synced: !is_syncing,
                        execution_state,
                        sync_progress,
                        client_healthy: true,
                        last_checked: SystemTime::now(),
                    })
                },
                Err(e) => {
                    warn!("Failed to check sync status: {}", e);
                    
                    Ok(EngineSyncStatus {
                        is_synced: false,
                        execution_state,
                        sync_progress: None,
                        client_healthy: false,
                        last_checked: SystemTime::now(),
                    })
                }
            }
        })
    }
}

impl Handler<SyncStatusChangedMessage> for EngineActor {
    type Result = ();

    fn handle(&mut self, msg: SyncStatusChangedMessage, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            synced = %msg.synced,
            current_height = %msg.current_height,
            target_height = ?msg.target_height,
            source = ?msg.source,
            "Received sync status change notification"
        );

        // Update execution state based on sync status
        match (msg.synced, &self.state.execution_state) {
            (true, ExecutionState::Syncing { .. }) => {
                // Transition from syncing to ready
                self.state.transition_state(
                    ExecutionState::Ready {
                        head_hash: None, // Will be updated by next forkchoice update
                        head_height: msg.current_height,
                        last_activity: SystemTime::now(),
                    },
                    format!("Sync completed via {:?}", msg.source)
                );
                
                info!(
                    height = %msg.current_height,
                    "Engine transitioned to Ready state after sync completion"
                );
                
                self.metrics.sync_completed();
            },
            (false, ExecutionState::Ready { .. }) => {
                // Transition from ready to syncing
                let target_height = msg.target_height.unwrap_or(msg.current_height);
                let progress = if target_height > 0 {
                    msg.current_height as f64 / target_height as f64
                } else {
                    0.0
                };
                
                self.state.transition_state(
                    ExecutionState::Syncing {
                        progress,
                        current_height: msg.current_height,
                        target_height,
                        eta: None,
                    },
                    format!("Sync status changed via {:?}", msg.source)
                );
                
                warn!(
                    current_height = %msg.current_height,
                    target_height = %target_height,
                    "Engine transitioned back to Syncing state"
                );
                
                self.metrics.sync_started();
            },
            (synced, current_state) => {
                // Log state but don't transition
                debug!(
                    synced = %synced,
                    current_state = ?current_state,
                    "Sync status notification received but no state change needed"
                );
            }
        }
        
        // Update sync metrics
        self.metrics.sync_status_checked();
    }
}

/// Get detailed sync progress from the execution client
async fn get_detailed_sync_progress(engine: &super::super::engine::Engine) -> Result<SyncProgress, crate::error::Error> {
    // Get current and latest block numbers
    let current_block = engine.get_latest_block_number().await?;
    
    // For detailed sync progress, we'd need to query the sync status
    // This is a simplified implementation
    let sync_progress = SyncProgress {
        current_block,
        target_block: current_block, // Would be fetched from peers
        progress_percentage: 1.0, // Would be calculated
        eta: None, // Would be estimated based on sync speed
        blocks_per_second: 0.0, // Would be calculated from recent progress
    };
    
    Ok(sync_progress)
}

impl EngineActor {
    /// Internal helper to monitor sync progress and update state
    pub(super) async fn monitor_sync_progress(&mut self) {
        if let ExecutionState::Syncing { ref mut progress, ref mut current_height, ref mut target_height, ref mut eta } = self.state.execution_state {
            match self.engine.get_latest_block_number().await {
                Ok(latest_block) => {
                    let old_height = *current_height;
                    *current_height = latest_block;
                    
                    // Calculate progress if we have a target
                    if *target_height > 0 {
                        *progress = latest_block as f64 / *target_height as f64;
                        
                        // Estimate ETA based on sync speed
                        if latest_block > old_height {
                            let blocks_synced = latest_block - old_height;
                            let blocks_remaining = target_height.saturating_sub(latest_block);
                            
                            if blocks_synced > 0 {
                                let sync_rate = blocks_synced as f64 / 10.0; // 10 second interval
                                let eta_seconds = blocks_remaining as f64 / sync_rate;
                                *eta = Some(Duration::from_secs_f64(eta_seconds));
                            }
                        }
                    }
                    
                    if latest_block != old_height {
                        debug!(
                            old_height = %old_height,
                            new_height = %latest_block,
                            progress = %progress,
                            eta = ?eta,
                            "Sync progress updated"
                        );
                    }
                },
                Err(e) => {
                    warn!("Failed to get latest block number for sync monitoring: {}", e);
                }
            }
        }
    }
    
    /// Internal helper to check if engine should transition to ready state
    pub(super) fn check_ready_transition(&mut self) -> bool {
        match &self.state.execution_state {
            ExecutionState::Syncing { progress, current_height, .. } => {
                // Transition to ready when sync is nearly complete (99.5%)
                if *progress >= 0.995 {
                    self.state.transition_state(
                        ExecutionState::Ready {
                            head_hash: None,
                            head_height: *current_height,
                            last_activity: SystemTime::now(),
                        },
                        "Sync progress reached threshold for ready state".to_string()
                    );
                    
                    info!(
                        height = %current_height,
                        progress = %(*progress * 100.0),
                        "Engine transitioned to Ready state (99.5% sync threshold reached)"
                    );
                    
                    return true;
                }
            },
            _ => {}
        }
        
        false
    }
}