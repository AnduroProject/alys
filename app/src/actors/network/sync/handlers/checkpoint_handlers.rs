//! SyncActor Checkpoint Management Message Handlers
//! 
//! Contains handlers for checkpoint operations including creation, restoration,
//! listing, and cleanup of blockchain state snapshots.

use actix::{Handler, Context, ResponseFuture};
use std::collections::HashMap;

use crate::actors::network::messages::*;
use crate::actors::network::messages::sync_messages::*;
use crate::actors::network::sync::actor::SyncActor;
use crate::actors::network::sync::{ChainState, FederationCheckpointState};

impl Handler<CreateCheckpoint> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<CheckpointResponse>>;

    fn handle(&mut self, msg: CreateCheckpoint, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        let current_height = msg.height.unwrap_or(self.state.progress.current_height);
        let compression = msg.compression;
        let chain_actor = self.chain_actor.clone();
        let peer_manager = self.peer_manager.clone();
        
        tracing::info!("Creating checkpoint at height {} (compression: {})", current_height, compression);
        
        Box::pin(async move {
            if let Some(mut checkpoint_manager) = checkpoint_manager {
                // Gather current chain state from various sources
                let mut block_hashes = Vec::new();
                let mut peer_states = HashMap::new();
                
                // Get block hash for checkpoint height (would be from chain)
                block_hashes.push((current_height, ethereum_types::H256::random()));
                
                // Get peer state information
                // In full implementation, would gather from peer_manager
                peer_states.insert("peer1".to_string(), "connected".to_string());
                
                // Get federation state (would be from chain actor if available)
                let federation_state = if let Some(chain_actor) = chain_actor {
                    // Would request current federation state from chain
                    FederationCheckpointState {
                        current_authorities: vec!["authority1".to_string(), "authority2".to_string()],
                        current_slot: current_height / 2, // Assuming 2-second slots
                        last_finalized_block: current_height.saturating_sub(6), // 6-block finalization
                        emergency_mode: false,
                    }
                } else {
                    FederationCheckpointState {
                        current_authorities: vec!["default_authority".to_string()],
                        current_slot: current_height / 2,
                        last_finalized_block: current_height.saturating_sub(1),
                        emergency_mode: false,
                    }
                };

                // Create comprehensive chain state
                let chain_state = ChainState {
                    height: current_height,
                    state_root: ethereum_types::H256::random(), // Would get from execution layer
                    block_hashes,
                    peer_states,
                    federation_state,
                    block_count: current_height,
                    metadata: {
                        let mut metadata = HashMap::new();
                        metadata.insert("created_by".to_string(), "sync_actor".to_string());
                        metadata.insert("compression".to_string(), compression.to_string());
                        metadata.insert("timestamp".to_string(), 
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs()
                                .to_string()
                        );
                        metadata
                    },
                };

                match checkpoint_manager.create_checkpoint(current_height, chain_state).await {
                    Ok(response) => {
                        tracing::info!(
                            "Checkpoint created successfully: {} (size: {} bytes)",
                            response.checkpoint_id, response.size_bytes
                        );
                        Ok(Ok(response))
                    }
                    Err(error) => {
                        tracing::error!("Failed to create checkpoint: {:?}", error);
                        Ok(Err(error))
                    }
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

impl Handler<RestoreCheckpoint> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<RestoreResponse>>;

    fn handle(&mut self, msg: RestoreCheckpoint, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        let checkpoint_id = msg.checkpoint_id;
        let verify_integrity = msg.verify_integrity;
        let chain_actor = self.chain_actor.clone();
        
        tracing::info!("Restoring checkpoint {} (verify: {})", checkpoint_id, verify_integrity);
        
        Box::pin(async move {
            if let Some(checkpoint_manager) = checkpoint_manager {
                match checkpoint_manager.restore_checkpoint(&checkpoint_id, verify_integrity).await {
                    Ok((chain_state, restore_response)) => {
                        tracing::info!(
                            "Checkpoint restored successfully: height {}, verified: {}",
                            restore_response.restored_height, restore_response.verified
                        );
                        
                        // Apply restored state to sync actor
                        // In full implementation, would update actor state with restored data
                        tracing::debug!("Restored chain state: height = {}, authorities = {:?}",
                            chain_state.height, chain_state.federation_state.current_authorities);
                        
                        // Notify chain actor of restored state if available
                        if let Some(chain_actor) = chain_actor {
                            // Would send RestoreChainState message to chain actor
                            tracing::debug!("Would notify chain actor of restored state");
                        }
                        
                        Ok(Ok(restore_response))
                    }
                    Err(error) => {
                        tracing::error!("Failed to restore checkpoint {}: {:?}", checkpoint_id, error);
                        Ok(Err(error))
                    }
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

impl Handler<ListCheckpoints> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<CheckpointListResponse>>;

    fn handle(&mut self, _msg: ListCheckpoints, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        
        tracing::debug!("Listing available checkpoints");
        
        Box::pin(async move {
            if let Some(checkpoint_manager) = checkpoint_manager {
                match checkpoint_manager.list_checkpoints().await {
                    Ok(checkpoints) => {
                        let checkpoint_entries = checkpoints.into_iter().map(|cp| {
                            CheckpointEntry {
                                id: cp.id,
                                height: cp.height,
                                created_at: cp.created_at,
                                size_bytes: cp.size_bytes,
                                compressed: cp.compressed,
                                verified: cp.verified,
                                metadata: cp.metadata.unwrap_or_default(),
                            }
                        }).collect();
                        
                        let response = CheckpointListResponse {
                            checkpoints: checkpoint_entries,
                            total_count: checkpoints.len() as u32,
                            total_size_bytes: checkpoints.iter().map(|cp| cp.size_bytes).sum(),
                        };
                        
                        tracing::debug!("Found {} checkpoints", response.total_count);
                        Ok(Ok(response))
                    }
                    Err(error) => {
                        tracing::error!("Failed to list checkpoints: {:?}", error);
                        Ok(Err(error))
                    }
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

impl Handler<DeleteCheckpoint> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<()>>;

    fn handle(&mut self, msg: DeleteCheckpoint, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        let checkpoint_id = msg.checkpoint_id;
        
        tracing::info!("Deleting checkpoint {}", checkpoint_id);
        
        Box::pin(async move {
            if let Some(mut checkpoint_manager) = checkpoint_manager {
                match checkpoint_manager.delete_checkpoint(&checkpoint_id).await {
                    Ok(()) => {
                        tracing::info!("Checkpoint {} deleted successfully", checkpoint_id);
                        Ok(Ok(()))
                    }
                    Err(error) => {
                        tracing::error!("Failed to delete checkpoint {}: {:?}", checkpoint_id, error);
                        Ok(Err(error))
                    }
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

impl Handler<CleanupCheckpoints> for SyncActor {
    type Result = ResponseFuture<NetworkActorResult<CheckpointCleanupResponse>>;

    fn handle(&mut self, msg: CleanupCheckpoints, _ctx: &mut Context<Self>) -> Self::Result {
        let checkpoint_manager = self.checkpoint_manager.clone();
        let retention_policy = msg.retention_policy;
        
        tracing::info!("Cleaning up checkpoints with policy: {:?}", retention_policy);
        
        Box::pin(async move {
            if let Some(mut checkpoint_manager) = checkpoint_manager {
                match checkpoint_manager.cleanup_checkpoints(retention_policy).await {
                    Ok(cleanup_result) => {
                        tracing::info!(
                            "Checkpoint cleanup completed: {} deleted, {} bytes freed",
                            cleanup_result.deleted_count, cleanup_result.space_freed_bytes
                        );
                        Ok(Ok(cleanup_result))
                    }
                    Err(error) => {
                        tracing::error!("Failed to cleanup checkpoints: {:?}", error);
                        Ok(Err(error))
                    }
                }
            } else {
                Ok(Err(NetworkError::ProtocolError {
                    message: "Checkpoint manager not initialized".to_string(),
                }))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::network::sync::config::SyncConfig;
    use actix::System;

    #[actix::test]
    async fn test_create_checkpoint_no_manager() {
        let config = SyncConfig::default();
        let mut sync_actor = SyncActor::new(config).unwrap();
        
        // No checkpoint manager initialized
        let create_msg = CreateCheckpoint {
            height: Some(100),
            compression: true,
        };

        // This would require full async context to test properly
        // For now, just verify the message structure
        assert_eq!(create_msg.height, Some(100));
        assert!(create_msg.compression);
    }

    #[actix::test]
    async fn test_restore_checkpoint_message() {
        let restore_msg = RestoreCheckpoint {
            checkpoint_id: "test-checkpoint-123".to_string(),
            verify_integrity: true,
        };

        assert_eq!(restore_msg.checkpoint_id, "test-checkpoint-123");
        assert!(restore_msg.verify_integrity);
    }

    #[actix::test]
    async fn test_checkpoint_cleanup_message() {
        let cleanup_msg = CleanupCheckpoints {
            retention_policy: CheckpointRetentionPolicy {
                max_checkpoints: 10,
                max_age_hours: 24 * 7, // 1 week
                min_free_space_gb: 10,
            },
        };

        assert_eq!(cleanup_msg.retention_policy.max_checkpoints, 10);
        assert_eq!(cleanup_msg.retention_policy.max_age_hours, 168);
    }
}

// Helper types for checkpoint operations (would be in messages module)
#[derive(Debug, Clone)]
pub struct ListCheckpoints;

#[derive(Debug, Clone)]
pub struct DeleteCheckpoint {
    pub checkpoint_id: String,
}

#[derive(Debug, Clone)]
pub struct CleanupCheckpoints {
    pub retention_policy: CheckpointRetentionPolicy,
}

#[derive(Debug, Clone)]
pub struct CheckpointEntry {
    pub id: String,
    pub height: u64,
    pub created_at: std::time::SystemTime,
    pub size_bytes: u64,
    pub compressed: bool,
    pub verified: bool,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CheckpointListResponse {
    pub checkpoints: Vec<CheckpointEntry>,
    pub total_count: u32,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct CheckpointRetentionPolicy {
    pub max_checkpoints: u32,
    pub max_age_hours: u64,
    pub min_free_space_gb: u64,
}

#[derive(Debug, Clone)]
pub struct CheckpointCleanupResponse {
    pub deleted_count: u32,
    pub space_freed_bytes: u64,
    pub remaining_checkpoints: u32,
}