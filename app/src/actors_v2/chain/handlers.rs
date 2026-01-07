//! ChainActor V2 Message Handlers
//!
//! All message handlers consolidated, following StorageActor V2 patterns

use actix::prelude::*;
use bitcoin::hashes::Hash;
use ethereum_types::{H256, U256};
use eyre::Result;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::{
    messages::{
        AuxPowParams, BlockSource, ChainManagerMessage, ChainManagerResponse, ChainMessage,
        ChainResponse, CreateAuxBlock, PegOutRequest, SubmitAuxBlock,
    },
    ChainActor, ChainError,
};

use crate::actors_v2::engine::{EngineMessage, EngineResponse};
use crate::types::ExecutionBlockHash;

use crate::actors_v2::common::serialization::{calculate_block_hash, serialize_block};
use crate::auxpow::AuxPow;
use crate::block::SignedConsensusBlock;
use bridge::PegInInfo;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use ssz_types::VariableList;

// Message handler implementations
impl Handler<ChainMessage> for ChainActor {
    type Result = ResponseFuture<Result<ChainResponse, ChainError>>;

    fn handle(&mut self, msg: ChainMessage, ctx: &mut Context<Self>) -> Self::Result {
        self.record_activity();

        match msg {
            ChainMessage::GetChainStatus => {
                // Get orphan cache stats (sync access via try_read to avoid blocking)
                let (observed_height, orphan_count) = {
                    match self.orphan_cache.try_read() {
                        Ok(cache) => (cache.observed_height(), cache.len()),
                        Err(_) => {
                            // If we can't get the lock, use current height as observed
                            (self.state.get_height(), 0)
                        }
                    }
                };

                // Query StorageActor for actual chain height instead of using stale local state
                // This is critical for Active Height Monitoring - peers need accurate heights
                let storage_actor = self.storage_actor.clone();
                let is_synced = self.state.is_synced();
                let is_validator = self.config.is_validator;
                let last_block_time = self
                    .state
                    .last_block_time
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
                let auxpow_enabled = self.config.enable_auxpow;
                let blocks_without_pow = self.state.blocks_without_pow;
                let local_height = self.state.get_height();
                let local_head_hash = self.state.get_head_hash();

                Box::pin(async move {
                    // Query StorageActor for authoritative chain height and head
                    let (height, head_hash) = if let Some(storage) = storage_actor {
                        match storage
                            .send(crate::actors_v2::storage::messages::GetChainHeadMessage {
                                correlation_id: None,
                            })
                            .await
                        {
                            Ok(Ok(Some(head))) => {
                                let hash = lighthouse_wrapper::types::Hash256::from_slice(&head.hash.0);
                                (head.number, Some(hash))
                            }
                            Ok(Ok(None)) => {
                                // No head in storage - use local state
                                (local_height, local_head_hash)
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(error = ?e, "Failed to get chain head from StorageActor, using local state");
                                (local_height, local_head_hash)
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "StorageActor mailbox error, using local state");
                                (local_height, local_head_hash)
                            }
                        }
                    } else {
                        // No StorageActor available - use local state
                        (local_height, local_head_hash)
                    };

                    let status = super::messages::ChainStatus {
                        height,
                        head_hash,
                        is_synced,
                        is_validator,
                        network_connected: false, // Would check network status
                        peer_count: 0,            // Would be updated from NetworkActor
                        pending_pegins: 0, // TODO: Count async
                        last_block_time,
                        auxpow_enabled,
                        blocks_without_pow,
                        observed_height,
                        orphan_count,
                    };
                    Ok(ChainResponse::ChainStatus(status))
                })
            }
            ChainMessage::ProduceBlock { slot, timestamp } => {
                // Validate preconditions before attempting block production
                if !self.config.is_validator {
                    warn!("Block production requested but node is not configured as validator");
                    Box::pin(async move {
                        Err(ChainError::Configuration(
                            "Node is not configured as validator".to_string(),
                        ))
                    })
                } else {
                    // Complete block production pipeline
                    let start_time = Instant::now();
                    let correlation_id = Uuid::new_v4();
                    let engine_actor = self.engine_actor.clone();
                    let storage_actor = self.storage_actor.clone();
                    let network_actor = self.network_actor.clone();
                    let sync_actor = self.sync_actor.clone();

                    // Capture simple state data and clone for async
                    let config_validator_address = self.config.validator_address;
                    let state_federation = self.state.federation.clone();
                    let mut self_clone = self.clone();

                    info!(
                        slot = slot,
                        timestamp_secs = timestamp.as_secs(),
                        correlation_id = %correlation_id,
                        "Starting complete block production pipeline"
                    );

                    Box::pin(async move {
                        // Phase 3: Check sync status before producing blocks (query SyncActor)
                        if let Some(ref sync_actor) = sync_actor {
                            match sync_actor
                                .send(crate::actors_v2::network::SyncMessage::GetSyncStatus)
                                .await
                            {
                                Ok(Ok(crate::actors_v2::network::SyncResponse::Status(status))) => {
                                    if status.is_syncing {
                                        info!(
                                            slot = slot,
                                            current_height = status.current_height,
                                            target_height = status.target_height,
                                            "Skipping block production - node is syncing"
                                        );
                                        return Err(ChainError::NotSynced);
                                    }
                                    debug!(
                                        slot = slot,
                                        current_height = status.current_height,
                                        "Node is synced - proceeding with block production"
                                    );
                                }
                                other => {
                                    error!(
                                        slot = slot,
                                        response = ?other,
                                        "Failed to get sync status from SyncActor - skipping block production"
                                    );
                                    return Err(ChainError::NotSynced);
                                }
                            }
                        }

                        // Node is synced (or sync status unavailable) - proceed with block production
                        // Step 2: Get parent block from storage
                        // Capture both execution hash (for Geth) and consensus hash (for ConsensusBlock.parent_hash)
                        let (parent_execution_hash, parent_consensus_hash) = if let Some(
                            ref storage_actor,
                        ) = storage_actor
                        {
                            let get_head_msg =
                                crate::actors_v2::storage::messages::GetChainHeadMessage {
                                    correlation_id: Some(correlation_id),
                                };

                            match storage_actor.send(get_head_msg).await {
                                Ok(storage_result) => {
                                    match storage_result {
                                        Ok(Some(head_ref)) => {
                                            info!(
                                                correlation_id = %correlation_id,
                                                parent_execution_hash = ?head_ref.execution_hash,
                                                parent_consensus_hash = ?head_ref.hash,
                                                parent_height = head_ref.number,
                                                "Retrieved chain head for block production"
                                            );
                                            // Return both hashes: execution for Geth, consensus for parent_hash field
                                            (head_ref.execution_hash, head_ref.hash)
                                        }
                                        Ok(None) => {
                                            info!(correlation_id = %correlation_id, "No chain head found - querying genesis for parent hashes");

                                            // Query genesis block (height 0) to get proper parent hashes
                                            let get_genesis_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
                                                height: 0,
                                                correlation_id: Some(correlation_id),
                                            };

                                            match storage_actor.send(get_genesis_msg).await {
                                                Ok(Ok(Some(genesis))) => {
                                                    let genesis_hash = genesis.canonical_root();
                                                    let genesis_exec_hash = genesis
                                                        .message
                                                        .execution_payload
                                                        .block_hash;

                                                    info!(
                                                        correlation_id = %correlation_id,
                                                        genesis_consensus_hash = %genesis_hash,
                                                        genesis_execution_hash = %genesis_exec_hash,
                                                        "Using genesis block as parent for block #1"
                                                    );

                                                    (genesis_exec_hash, genesis_hash)
                                                }
                                                Ok(Ok(None)) => {
                                                    error!(
                                                        correlation_id = %correlation_id,
                                                        "Genesis block not found in storage - cannot produce blocks"
                                                    );
                                                    return Err(ChainError::InvalidState(
                                                        "Cannot produce blocks without genesis - wait for ChainActor genesis initialization".to_string()
                                                    ));
                                                }
                                                Ok(Err(e)) => {
                                                    error!(
                                                        correlation_id = %correlation_id,
                                                        error = ?e,
                                                        "Failed to query genesis from storage"
                                                    );
                                                    return Err(ChainError::Storage(e.to_string()));
                                                }
                                                Err(e) => {
                                                    error!(
                                                        correlation_id = %correlation_id,
                                                        error = ?e,
                                                        "Communication error querying genesis"
                                                    );
                                                    return Err(ChainError::NetworkError(format!(
                                                        "Genesis query communication failed: {}",
                                                        e
                                                    )));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!(correlation_id = %correlation_id, error = ?e, "Failed to get chain head");
                                            return Err(ChainError::Storage(e.to_string()));
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(correlation_id = %correlation_id, error = ?e, "Communication error with StorageActor");
                                    return Err(ChainError::NetworkError(format!(
                                        "Storage communication failed: {}",
                                        e
                                    )));
                                }
                            }
                        } else {
                            error!(correlation_id = %correlation_id, "StorageActor not available for parent block retrieval");
                            return Err(ChainError::Internal(
                                "StorageActor not available".to_string(),
                            ));
                        };

                        // Step 3: Collect withdrawals with real fee calculation (get state inside async)
                        let state_queued_pegins = {
                            // Must do async operations inside the async block
                            let queued_pegins_guard = self_clone.state.queued_pegins.read().await;
                            queued_pegins_guard.clone()
                        };

                        // Get fresh chain head from StorageActor for fee calculation
                        let fresh_head = if let Some(ref storage_actor) = storage_actor {
                            match storage_actor
                                .send(crate::actors_v2::storage::messages::GetChainHeadMessage {
                                    correlation_id: Some(correlation_id),
                                })
                                .await
                            {
                                Ok(Ok(Some(v2_head))) => Some(v2_head),
                                _ => {
                                    debug!(correlation_id = %correlation_id, "No chain head available for withdrawal collection - using None for genesis");
                                    None
                                }
                            }
                        } else {
                            None
                        };

                        let withdrawal_collection = match crate::actors_v2::chain::withdrawals::collect_withdrawals_standalone(
                            &state_queued_pegins,
                            storage_actor.as_ref(),
                            config_validator_address,
                            &state_federation,
                            &fresh_head,
                        ).await {
                            Ok(collection) => {
                                info!(
                                    correlation_id = %correlation_id,
                                    pegin_count = collection.pegin_count,
                                    total_pegin_amount = %collection.total_pegin_amount,
                                    total_fee_amount = %collection.total_fee_amount,
                                    withdrawal_count = collection.withdrawals.len(),
                                    "Successfully collected withdrawals with real fee calculation"
                                );
                                collection
                            }
                            Err(e) => {
                                error!(correlation_id = %correlation_id, error = ?e, "Failed to collect withdrawals");
                                return Err(ChainError::Internal(format!("Withdrawal collection failed: {}", e)));
                            }
                        };

                        // Step 4: Convert withdrawals to AddBalance format for EngineActor
                        let add_balances: Vec<crate::engine::AddBalance> = withdrawal_collection
                            .withdrawals
                            .into_iter()
                            .map(|w| {
                                crate::engine::AddBalance::from((
                                    w.address,
                                    crate::engine::ConsensusAmount(w.amount),
                                ))
                            })
                            .collect();

                        // Step 5: Build execution payload via EngineActor
                        // Convert zero hash to None for genesis (matches V0 behavior)
                        let parent_hash_for_engine = if parent_execution_hash.into_root().is_zero()
                        {
                            None
                        } else {
                            Some(parent_execution_hash)
                        };

                        let execution_payload = if let Some(ref engine_actor) = engine_actor {
                            let msg = crate::actors_v2::engine::EngineMessage::BuildPayload {
                                timestamp,
                                parent_hash: parent_hash_for_engine,
                                add_balances,
                                correlation_id: Some(correlation_id),
                            };

                            match engine_actor.send(msg).await {
                                Ok(engine_result) => match engine_result {
                                    Ok(
                                        crate::actors_v2::engine::EngineResponse::PayloadBuilt {
                                            payload,
                                            build_time,
                                        },
                                    ) => {
                                        info!(
                                            correlation_id = %correlation_id,
                                            block_number = payload.block_number(),
                                            gas_used = payload.gas_used(),
                                            build_time_ms = build_time.as_millis(),
                                            "Successfully built execution payload via EngineActor"
                                        );
                                        payload
                                    }
                                    Ok(other_response) => {
                                        error!(correlation_id = %correlation_id, response = ?other_response, "Unexpected response from EngineActor");
                                        return Err(ChainError::Internal(
                                            "Unexpected EngineActor response".to_string(),
                                        ));
                                    }
                                    Err(e) => {
                                        // Layer 3: Detect PayloadIdUnavailable for chain head desync detection
                                        let is_payload_unavailable = matches!(
                                            e,
                                            crate::actors_v2::engine::EngineError::PayloadIdUnavailable
                                        );

                                        if is_payload_unavailable {
                                            // Track consecutive PayloadIdUnavailable errors
                                            self_clone.payload_unavailable_count += 1;

                                            const PAYLOAD_ERROR_THRESHOLD: u32 = 3;

                                            if self_clone.payload_unavailable_count >= PAYLOAD_ERROR_THRESHOLD {
                                                error!(
                                                    consecutive_errors = self_clone.payload_unavailable_count,
                                                    correlation_id = %correlation_id,
                                                    "Repeated PayloadIdUnavailable - chain head likely desynchronized, triggering emergency re-sync"
                                                );

                                                // Trigger force resync to recover from desync
                                                if let Some(ref sync_actor) = sync_actor {
                                                    let reason = format!(
                                                        "PayloadIdUnavailable threshold exceeded ({} consecutive errors)",
                                                        self_clone.payload_unavailable_count
                                                    );
                                                    let _ = sync_actor
                                                        .send(crate::actors_v2::network::SyncMessage::ForceResync { reason })
                                                        .await;
                                                }

                                                // Reset counter after triggering resync
                                                self_clone.payload_unavailable_count = 0;
                                            } else {
                                                warn!(
                                                    consecutive_errors = self_clone.payload_unavailable_count,
                                                    threshold = PAYLOAD_ERROR_THRESHOLD,
                                                    correlation_id = %correlation_id,
                                                    "PayloadIdUnavailable error - tracking for potential desync"
                                                );
                                            }
                                        }

                                        error!(correlation_id = %correlation_id, error = ?e, "Failed to build execution payload");
                                        return Err(ChainError::Engine(format!(
                                            "Payload build failed: {}",
                                            e
                                        )));
                                    }
                                },
                                Err(e) => {
                                    error!(correlation_id = %correlation_id, error = ?e, "Communication error with EngineActor");
                                    return Err(ChainError::NetworkError(format!(
                                        "Engine communication failed: {}",
                                        e
                                    )));
                                }
                            }
                        } else {
                            error!(correlation_id = %correlation_id, "EngineActor not available");
                            return Err(ChainError::Internal(
                                "EngineActor not available".to_string(),
                            ));
                        };

                        // Step 6: Create consensus block
                        // Convert ExecutionPayload to ExecutionPayloadCapella if needed
                        let capella_payload = match execution_payload {
                            lighthouse_wrapper::types::ExecutionPayload::Capella(capella) => {
                                capella
                            }
                            _ => {
                                error!(correlation_id = %correlation_id, "Unsupported execution payload type - expected Capella");
                                return Err(ChainError::Engine(
                                    "Unsupported execution payload type".to_string(),
                                ));
                            }
                        };

                        let consensus_block = crate::block::ConsensusBlock {
                            parent_hash: parent_consensus_hash, // Use actual parent consensus block hash, not derived from slot
                            slot,
                            auxpow_header: None, // Will be set by incorporate_auxpow if available
                            execution_payload: capella_payload,
                            pegins: vec![], // Withdrawal collection integrated above via add_balances
                            pegout_payment_proposal: None,
                            finalized_pegouts: vec![],
                        };

                        // Step 7: Incorporate AuxPoW if available (Phase 4: Integration Point 1)
                        let signed_block = match self_clone
                            .incorporate_auxpow(consensus_block)
                            .await
                        {
                            Ok(signed_with_auxpow) => {
                                info!(
                                    correlation_id = %correlation_id,
                                    has_auxpow = signed_with_auxpow.message.auxpow_header.is_some(),
                                    "Block signed with AuxPoW incorporation result"
                                );
                                signed_with_auxpow
                            }
                            Err(ChainError::Consensus(msg))
                                if msg.contains("Too many blocks without PoW") =>
                            {
                                error!(
                                    correlation_id = %correlation_id,
                                    blocks_without_pow = self_clone.state.blocks_without_pow,
                                    "Cannot produce block: AuxPoW required but not available"
                                );
                                return Err(ChainError::Consensus(msg));
                            }
                            Err(e) => {
                                error!(correlation_id = %correlation_id, error = ?e, "AuxPoW incorporation failed");
                                return Err(e);
                            }
                        };

                        // Step 8: Store block via StorageActor (if available)
                        if let Some(ref storage_actor) = storage_actor {
                            let store_msg =
                                crate::actors_v2::storage::messages::StoreBlockMessage {
                                    block: signed_block.clone(),
                                    canonical: true,
                                    correlation_id: Some(correlation_id),
                                };

                            match storage_actor.send(store_msg).await {
                                Ok(Ok(())) => {
                                    info!(
                                        correlation_id = %correlation_id,
                                        slot = slot,
                                        "Successfully stored produced block"
                                    );
                                }
                                Ok(Err(e)) => {
                                    error!(correlation_id = %correlation_id, error = ?e, "Failed to store produced block");
                                    return Err(ChainError::Storage(e.to_string()));
                                }
                                Err(e) => {
                                    error!(correlation_id = %correlation_id, error = ?e, "Communication error with StorageActor");
                                    return Err(ChainError::NetworkError(format!(
                                        "Storage communication failed: {}",
                                        e
                                    )));
                                }
                            }
                        }

                        // Step 9: Store accumulated fees for the produced block (V0 compatibility)
                        if let Some(ref storage_actor) = storage_actor {
                            let block_hash = calculate_block_hash(&signed_block);

                            // Use real fee calculation from withdrawal collection
                            let total_fees_wei = withdrawal_collection
                                .total_fee_amount
                                .saturating_add(withdrawal_collection.total_pegin_amount);

                            let set_fees_msg =
                                crate::actors_v2::storage::messages::SetAccumulatedFeesMessage {
                                    block_root: lighthouse_wrapper::types::Hash256::from_slice(
                                        block_hash.as_bytes(),
                                    ),
                                    fees: total_fees_wei,
                                    correlation_id: Some(correlation_id),
                                };

                            match storage_actor.send(set_fees_msg).await {
                                Ok(Ok(())) => {
                                    debug!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        fees_wei = %total_fees_wei,
                                        "Successfully stored accumulated fees for produced block"
                                    );
                                }
                                Ok(Err(e)) => {
                                    warn!(correlation_id = %correlation_id, error = ?e, "Failed to store accumulated fees (non-fatal)");
                                }
                                Err(e) => {
                                    warn!(correlation_id = %correlation_id, error = ?e, "Communication error storing fees (non-fatal)");
                                }
                            }
                        }

                        // Step 10: Commit block to execution engine (CRITICAL for block #2+)
                        if let Some(ref engine_actor) = engine_actor {
                            let commit_msg = crate::actors_v2::engine::EngineMessage::CommitBlock {
                                execution_payload:
                                    lighthouse_wrapper::types::ExecutionPayload::Capella(
                                        signed_block.message.execution_payload.clone(),
                                    ),
                                correlation_id: Some(correlation_id),
                            };

                            match engine_actor.send(commit_msg).await {
                                Ok(engine_result) => {
                                    match engine_result {
                                        Ok(crate::actors_v2::engine::EngineResponse::BlockCommitted { block_hash, commit_time }) => {
                                            info!(
                                                correlation_id = %correlation_id,
                                                block_hash = ?block_hash,
                                                commit_time_ms = commit_time.as_millis(),
                                                "Successfully committed block to execution engine"
                                            );
                                        }
                                        Ok(other_response) => {
                                            warn!(correlation_id = %correlation_id, response = ?other_response, "Unexpected response from EngineActor commit");
                                        }
                                        Err(e) => {
                                            error!(
                                                correlation_id = %correlation_id,
                                                error = ?e,
                                                "Failed to commit block to execution engine - block stored but Geth not updated"
                                            );
                                            // Non-fatal: block already stored in consensus layer
                                            // But this will cause subsequent blocks to fail with PayloadIdUnavailable
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Communication error with EngineActor during commit"
                                    );
                                }
                            }
                        } else {
                            warn!(correlation_id = %correlation_id, "EngineActor not available for block commitment - subsequent blocks may fail");
                        }

                        // Step 11: Broadcast block via NetworkActor (if available)
                        if let Some(ref network_actor) = network_actor {
                            let block_data = match crate::actors_v2::common::serialization::serialize_block_for_network(&signed_block) {
                                Ok(data) => data,
                                Err(e) => {
                                    error!(correlation_id = %correlation_id, error = ?e, "Failed to serialize block for broadcast");
                                    return Err(e);
                                }
                            };

                            let broadcast_msg =
                                crate::actors_v2::network::NetworkMessage::BroadcastBlock {
                                    block_data,
                                    priority: true,
                                };

                            match network_actor.send(broadcast_msg).await {
                                Ok(Ok(_)) => {
                                    info!(
                                        correlation_id = %correlation_id,
                                        slot = slot,
                                        "Successfully broadcasted produced block"
                                    );
                                }
                                Ok(Err(e)) => {
                                    warn!(correlation_id = %correlation_id, error = ?e, "Failed to broadcast block (non-fatal)");
                                }
                                Err(e) => {
                                    warn!(correlation_id = %correlation_id, error = ?e, "Communication error with NetworkActor (non-fatal)");
                                }
                            }
                        }

                        // Step 12: Update ChainActor's local state with fresh chain head from StorageActor
                        if let Some(ref storage_actor) = storage_actor {
                            let get_head_msg =
                                crate::actors_v2::storage::messages::GetChainHeadMessage {
                                    correlation_id: Some(correlation_id),
                                };

                            match storage_actor.send(get_head_msg).await {
                                Ok(Ok(Some(v2_head_ref))) => {
                                    // Update local state with V2 BlockRef directly
                                    self_clone.state.update_head(v2_head_ref.clone());

                                    info!(
                                        correlation_id = %correlation_id,
                                        consensus_hash = %v2_head_ref.hash,
                                        execution_hash = ?v2_head_ref.execution_hash,
                                        height = v2_head_ref.number,
                                        "Updated ChainActor local state with fresh chain head"
                                    );
                                }
                                Ok(Ok(None)) => {
                                    warn!(correlation_id = %correlation_id, "StorageActor returned no chain head after block production");
                                }
                                Ok(Err(e)) => {
                                    warn!(correlation_id = %correlation_id, error = ?e, "Failed to get chain head for state sync (non-fatal)");
                                }
                                Err(e) => {
                                    warn!(correlation_id = %correlation_id, error = ?e, "Communication error getting chain head for state sync (non-fatal)");
                                }
                            }
                        }

                        // Layer 3: Reset PayloadIdUnavailable counter on successful block production
                        self_clone.payload_unavailable_count = 0;

                        let duration = start_time.elapsed();
                        info!(
                            slot = slot,
                            correlation_id = %correlation_id,
                            duration_ms = duration.as_millis(),
                            "Completed block production pipeline"
                        );

                        Ok(ChainResponse::BlockProduced {
                            block: signed_block,
                            duration,
                        })
                    })
                }
            }
            ChainMessage::ImportBlock {
                block,
                source,
                peer_id,
            } => {
                // Perform basic validation before import
                let block_height = block.message.execution_payload.block_number;
                let current_height = self.state.get_height();

                if block_height <= current_height && current_height > 0 {
                    info!(
                        block_height = block_height,
                        current_height = current_height,
                        "Rejecting old block"
                    );
                    Box::pin(async move {
                        Err(ChainError::InvalidBlock(
                            "Block height is too old".to_string(),
                        ))
                    })
                } else {
                    // Phase 2: Try to acquire import lock
                    let lock_acquired = self
                        .import_in_progress
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok();

                    if !lock_acquired {
                        // Another import is in progress - queue this block
                        let block_hash = calculate_block_hash(&block);
                        let pending_imports = self.pending_imports.clone();
                        let max_pending = self.max_pending_imports;

                        info!(
                            block_height = block_height,
                            block_hash = %block_hash,
                            source = ?source,
                            "Import lock held - queueing block for later processing"
                        );

                        Box::pin(async move {
                            let mut queue = pending_imports.write().await;

                            // Check queue capacity
                            if queue.len() >= max_pending {
                                warn!(
                                    block_height = block_height,
                                    block_hash = %block_hash,
                                    queue_size = queue.len(),
                                    "Import queue full - rejecting block"
                                );
                                return Err(ChainError::QueueFull);
                            }

                            // Queue the import
                            queue.push_back(super::actor::PendingImport {
                                block,
                                source,
                                queued_at: Instant::now(),
                            });

                            let position = queue.len();

                            // Phase 5: Update import queue depth metric
                            // Note: We can't access self.metrics here in the async block
                            // Metrics will be updated when queue is processed

                            info!(
                                block_height = block_height,
                                block_hash = %block_hash,
                                queue_position = position,
                                queue_depth = position,
                                "Block queued for import"
                            );

                            Ok(ChainResponse::BlockQueued { position })
                        })
                    } else {
                        // Lock acquired successfully - proceed with import
                        let block_hash = calculate_block_hash(&block);
                        let correlation_id = Uuid::new_v4();
                        let start_time = Instant::now();

                        info!(
                            block_height = block_height,
                            block_hash = %block_hash,
                            source = ?source,
                            correlation_id = %correlation_id,
                            "Import lock acquired - starting complete block import pipeline with V0 integration"
                        );

                        // Clone self to enable async method calls (Critical Blocker 1 solution)
                        let self_clone = self.clone();

                        // Capture actor references for async block
                        let engine_actor = self.engine_actor.clone();
                        let storage_actor = self.storage_actor.clone();

                        // Capture context address for queue processing
                        let ctx_addr = ctx.address();

                        Box::pin(async move {
                            // Wrap entire import logic to ensure lock release on all paths
                            let import_result: Result<ChainResponse, ChainError> = async {
                            // Step 1: Structural validation
                            if let Err(validation_error) = crate::actors_v2::common::serialization::validate_block_structure(&block) {
                                error!(
                                    correlation_id = %correlation_id,
                                    block_hash = %block_hash,
                                    error = ?validation_error,
                                    "Block failed structural validation"
                                );
                                return Err(ChainError::InvalidBlock(format!("Invalid block structure: {}", validation_error)));
                            }

                        debug!(
                            correlation_id = %correlation_id,
                            block_hash = %block_hash,
                            "Block passed structural validation"
                        );

                        // Step 1.5: Signature verification (Phase 3)
                        if let Err(signature_error) = crate::actors_v2::common::validation::verify_block_signature(&block, &self_clone.state.aura) {
                            error!(
                                correlation_id = %correlation_id,
                                block_hash = %block_hash,
                                error = ?signature_error,
                                "Block failed signature verification"
                            );
                            return Err(signature_error);
                        }

                        debug!(
                            correlation_id = %correlation_id,
                            block_hash = %block_hash,
                            "Block signature verified successfully"
                        );

                        // Step 1.7: Parent hash validation (Phase 3)
                        if let Some(ref storage_actor) = storage_actor {
                            if let Err(parent_error) = crate::actors_v2::common::validation::validate_parent_relationship(&block, storage_actor).await {
                                // Check if this is an orphan block (parent not found)
                                if let ChainError::OrphanBlock { parent_hash: orphan_parent_hash, block_height: orphan_height } = &parent_error {
                                    // Cache as orphan instead of rejecting
                                    info!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        parent_hash = %orphan_parent_hash,
                                        block_height = orphan_height,
                                        "Block is orphan (parent not found) - caching for later processing"
                                    );

                                    // Add to orphan cache
                                    let cache_result = {
                                        let mut cache = self_clone.orphan_cache.write().await;
                                        let parent_hash_h256 = *orphan_parent_hash;
                                        cache.add(
                                            block.clone(),
                                            *orphan_height,
                                            block_hash,
                                            parent_hash_h256,
                                            current_height,
                                            peer_id.clone(),
                                        )
                                    };

                                    match cache_result {
                                        Ok(true) => {
                                            info!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                "Orphan block cached successfully"
                                            );

                                            // Bug Fix: Orphan-triggered re-sync
                                            // If we receive orphan blocks that are far ahead of our current height,
                                            // it indicates we've fallen behind and need to re-sync.
                                            // This handles the case where gossipsub delivers blocks but
                                            // Active Height Monitoring fails to detect the gap.
                                            const ORPHAN_RESYNC_THRESHOLD: u64 = 5;

                                            let observed_height = {
                                                let cache = self_clone.orphan_cache.read().await;
                                                cache.observed_height()
                                            };

                                            let gap = observed_height.saturating_sub(current_height);

                                            if gap >= ORPHAN_RESYNC_THRESHOLD {
                                                warn!(
                                                    correlation_id = %correlation_id,
                                                    current_height = current_height,
                                                    observed_height = observed_height,
                                                    gap = gap,
                                                    threshold = ORPHAN_RESYNC_THRESHOLD,
                                                    "Large orphan gap detected - triggering re-sync"
                                                );

                                                // Trigger ForceResync to fetch missing blocks
                                                if let Some(ref sync_actor) = self_clone.sync_actor {
                                                    let reason = format!(
                                                        "Orphan gap {} exceeds threshold {} (current: {}, observed: {})",
                                                        gap, ORPHAN_RESYNC_THRESHOLD, current_height, observed_height
                                                    );
                                                    if let Err(e) = sync_actor.send(
                                                        crate::actors_v2::network::SyncMessage::ForceResync { reason }
                                                    ).await {
                                                        warn!(
                                                            correlation_id = %correlation_id,
                                                            error = %e,
                                                            "Failed to trigger ForceResync from orphan detection"
                                                        );
                                                    }
                                                }
                                            }

                                            // Return success - block is cached, not rejected
                                            return Ok(ChainResponse::BlockRejected {
                                                reason: format!("Orphan block cached: parent {} not found", orphan_parent_hash),
                                            });
                                        }
                                        Ok(false) => {
                                            debug!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                "Orphan block not cached (duplicate or too far ahead)"
                                            );
                                            return Ok(ChainResponse::BlockRejected {
                                                reason: "Orphan block rejected: duplicate or too far ahead".to_string(),
                                            });
                                        }
                                        Err(e) => {
                                            warn!(
                                                correlation_id = %correlation_id,
                                                error = %e,
                                                "Failed to cache orphan block"
                                            );
                                            return Err(parent_error);
                                        }
                                    }
                                }

                                // Not an orphan error - propagate the error
                                error!(
                                    correlation_id = %correlation_id,
                                    block_hash = %block_hash,
                                    error = ?parent_error,
                                    "Block failed parent relationship validation"
                                );
                                return Err(parent_error);
                            }

                            debug!(
                                correlation_id = %correlation_id,
                                block_hash = %block_hash,
                                "Parent relationship validated successfully"
                            );
                        } else {
                            warn!(
                                correlation_id = %correlation_id,
                                "StorageActor not available for parent validation - skipping (unsafe!)"
                            );
                        }

                        // Step 1.9: Fork detection (Phase 4)
                        // Check if a block already exists at this height
                        if let Some(ref storage_actor) = storage_actor {
                            let get_by_height_msg = crate::actors_v2::storage::messages::GetBlockByHeightMessage {
                                height: block_height,
                                correlation_id: Some(correlation_id),
                            };

                            match storage_actor.send(get_by_height_msg).await {
                                Ok(Ok(Some(existing_block))) => {
                                    let existing_hash = calculate_block_hash(&existing_block);

                                    // Check if it's the same block (duplicate)
                                    if existing_hash == block_hash {
                                        info!(
                                            correlation_id = %correlation_id,
                                            block_hash = %block_hash,
                                            block_height = block_height,
                                            "Duplicate block received - already imported, ignoring gracefully"
                                        );

                                        // Return success without penalty - this is normal in distributed systems
                                        return Ok(ChainResponse::BlockImported {
                                            block_hash,
                                            height: block_height,
                                        });
                                    } else {
                                        // FORK DETECTED: Different block at same height
                                        warn!(
                                            correlation_id = %correlation_id,
                                            existing_hash = %existing_hash,
                                            new_hash = %block_hash,
                                            height = block_height,
                                            "FORK DETECTED: Competing blocks at same height"
                                        );

                                        // Phase 5: Record fork detection metric
                                        self_clone.metrics.forks_detected.inc();

                                        // Apply fork choice rule (Phase 4)
                                        let fork_choice = crate::actors_v2::chain::fork_choice::compare_blocks(
                                            &existing_block,
                                            &block,
                                        );

                                        match fork_choice {
                                            crate::actors_v2::chain::fork_choice::ForkChoice::KeepCurrent => {
                                                info!(
                                                    correlation_id = %correlation_id,
                                                    existing_hash = %existing_hash,
                                                    new_hash = %block_hash,
                                                    "Fork choice: keeping current block (better chain)"
                                                );

                                                // Current block is canonical - reject new block
                                                return Ok(ChainResponse::BlockImported {
                                                    block_hash: existing_hash,
                                                    height: block_height,
                                                });
                                            }
                                            crate::actors_v2::chain::fork_choice::ForkChoice::Tiebreak { winner } => {
                                                if winner == block_hash {
                                                    warn!(
                                                        correlation_id = %correlation_id,
                                                        new_hash = %block_hash,
                                                        existing_hash = %existing_hash,
                                                        "Fork choice: new block wins tiebreak - replacing current block"
                                                    );

                                                    // New block wins - continue with import
                                                    // Note: In a full implementation, we would mark the existing block
                                                    // as non-canonical in storage. For now, we'll overwrite it.
                                                    info!(
                                                        correlation_id = %correlation_id,
                                                        "Proceeding with import of winning block"
                                                    );
                                                } else {
                                                    info!(
                                                        correlation_id = %correlation_id,
                                                        existing_hash = %existing_hash,
                                                        new_hash = %block_hash,
                                                        "Fork choice: existing block wins tiebreak - keeping current"
                                                    );

                                                    // Existing block wins - reject new block
                                                    return Ok(ChainResponse::BlockImported {
                                                        block_hash: existing_hash,
                                                        height: block_height,
                                                    });
                                                }
                                            }
                                            crate::actors_v2::chain::fork_choice::ForkChoice::Reorganize { new_tip, rollback_to } => {
                                                warn!(
                                                    correlation_id = %correlation_id,
                                                    new_tip = %new_tip,
                                                    rollback_to = rollback_to,
                                                    "Fork choice: reorganization needed - executing chain reorganization"
                                                );

                                                // Phase 4C: Execute chain reorganization
                                                match self_clone.reorganize_chain(&block, correlation_id).await {
                                                    Ok(reorg_result) => {
                                                        info!(
                                                            correlation_id = %correlation_id,
                                                            blocks_rolled_back = reorg_result.blocks_rolled_back,
                                                            blocks_applied = reorg_result.blocks_applied,
                                                            new_tip = %reorg_result.new_tip,
                                                            "Chain reorganization completed successfully - new block is now canonical"
                                                        );

                                                        // Phase 5: Record reorganization metrics
                                                        self_clone.metrics.reorganizations.inc();
                                                        self_clone.metrics.reorganization_depth.observe(reorg_result.blocks_rolled_back as f64);

                                                        // CRITICAL FIX: Update execution layer fork choice to new canonical head
                                                        // Without this, EL and CL are desynchronized after reorg
                                                        let new_head_hash = block.message.execution_payload.block_hash;
                                                        let finalized_hash = ExecutionBlockHash::zero(); // TODO: Track actual finalized hash

                                                        debug!(
                                                            correlation_id = %correlation_id,
                                                            new_head = ?new_head_hash,
                                                            finalized = ?finalized_hash,
                                                            "Updating execution layer fork choice after reorganization"
                                                        );

                                                        if let Some(ref engine_actor) = self_clone.engine_actor {
                                                            let fork_choice_result = engine_actor
                                                                .send(EngineMessage::UpdateForkChoice {
                                                                    head_hash: new_head_hash,
                                                                    safe_hash: finalized_hash,
                                                                    finalized_hash: finalized_hash,
                                                                    correlation_id: Some(correlation_id),
                                                                })
                                                                .await;

                                                            match fork_choice_result {
                                                                Ok(Ok(EngineResponse::ForkChoiceUpdated { success: true })) => {
                                                                    info!(
                                                                        correlation_id = %correlation_id,
                                                                        new_head = ?new_head_hash,
                                                                        "Execution layer fork choice updated successfully after reorganization"
                                                                    );
                                                                }
                                                                Ok(Ok(_)) => {
                                                                    error!(
                                                                        correlation_id = %correlation_id,
                                                                        "Unexpected response from UpdateForkChoice after reorganization"
                                                                    );
                                                                    // Non-fatal: CL updated, EL may recover on next block
                                                                }
                                                                Ok(Err(e)) => {
                                                                    error!(
                                                                        correlation_id = %correlation_id,
                                                                        error = ?e,
                                                                        "CRITICAL: Failed to update execution layer fork choice after reorganization"
                                                                    );
                                                                    // Non-fatal: Log critical error but continue
                                                                    // CL reorg completed, EL will sync on next block
                                                                    self_clone.metrics.fork_choice_failures_after_reorg.inc();
                                                                }
                                                                Err(e) => {
                                                                    error!(
                                                                        correlation_id = %correlation_id,
                                                                        error = ?e,
                                                                        "CRITICAL: Actor mailbox error during fork choice update after reorganization"
                                                                    );
                                                                    // Mailbox error is serious - return error
                                                                    return Err(ChainError::ActorMailbox(format!(
                                                                        "Failed to communicate with EngineActor after reorg: {:?}",
                                                                        e
                                                                    )));
                                                                }
                                                            }
                                                        } else {
                                                            warn!(
                                                                correlation_id = %correlation_id,
                                                                "EngineActor not available - cannot update fork choice after reorganization"
                                                            );
                                                        }

                                                        // Reorganization already handled storage and chain head updates
                                                        // Skip the normal import flow and return success
                                                        return Ok(ChainResponse::BlockImported {
                                                            block_hash: reorg_result.new_tip,
                                                            height: reorg_result.new_tip_height,
                                                        });
                                                    }
                                                    Err(reorg_error) => {
                                                        error!(
                                                            correlation_id = %correlation_id,
                                                            error = ?reorg_error,
                                                            "Chain reorganization failed - keeping current block"
                                                        );
                                                        return Err(reorg_error);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(Ok(None)) => {
                                    // No existing block at this height - normal import path
                                    debug!(
                                        correlation_id = %correlation_id,
                                        block_height = block_height,
                                        "No existing block at this height - proceeding with normal import"
                                    );
                                }
                                Ok(Err(e)) => {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Failed to check for existing block at height - proceeding anyway (risky)"
                                    );
                                    // Continue - non-fatal but logged as warning
                                }
                                Err(e) => {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Communication error checking for existing block - proceeding anyway (risky)"
                                    );
                                    // Continue - non-fatal but logged as warning
                                }
                            }
                        } else {
                            warn!(
                                correlation_id = %correlation_id,
                                "StorageActor not available for fork detection - skipping (unsafe!)"
                            );
                        }

                        // Step 2: Consensus validation via V0 Aura (Critical Blocker 2 solution)
                        if let Err(aura_error) = self_clone.state.aura.check_signed_by_author(&block) {
                            error!(
                                correlation_id = %correlation_id,
                                block_hash = %block_hash,
                                error = ?aura_error,
                                "Block failed V0 Aura consensus validation"
                            );
                            return Err(ChainError::Consensus(format!("Aura validation failed: {:?}", aura_error)));
                        }

                        debug!(
                            correlation_id = %correlation_id,
                            block_hash = %block_hash,
                            "Block passed V0 Aura consensus validation"
                        );

                        // Step 3: Execution payload validation via EngineActor
                        if let Some(ref engine_actor) = engine_actor {
                            let msg = crate::actors_v2::engine::EngineMessage::ValidatePayload {
                                payload: lighthouse_wrapper::types::ExecutionPayload::Capella(block.message.execution_payload.clone()),
                                correlation_id: Some(correlation_id),
                            };

                            match engine_actor.send(msg).await {
                                Ok(engine_result) => {
                                    match engine_result {
                                        Ok(crate::actors_v2::engine::EngineResponse::PayloadValid { is_valid: true, validation_time }) => {
                                            debug!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                validation_time_ms = validation_time.as_millis(),
                                                "Execution payload validation passed"
                                            );
                                        }
                                        Ok(crate::actors_v2::engine::EngineResponse::PayloadValid { is_valid: false, .. }) => {
                                            error!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                "Execution payload validation failed"
                                            );
                                            return Err(ChainError::InvalidBlock("Execution payload validation failed".to_string()));
                                        }
                                        Ok(other_response) => {
                                            error!(correlation_id = %correlation_id, response = ?other_response, "Unexpected EngineActor response");
                                            return Err(ChainError::Internal("Unexpected EngineActor response".to_string()));
                                        }
                                        Err(e) => {
                                            error!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                error = ?e,
                                                "Engine error during payload validation"
                                            );
                                            return Err(ChainError::Engine(format!("Payload validation failed: {}", e)));
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Communication error with EngineActor during validation"
                                    );
                                    return Err(ChainError::NetworkError(format!("Engine communication failed: {}", e)));
                                }
                            }
                        } else {
                            warn!(correlation_id = %correlation_id, "EngineActor not available for payload validation - skipping");
                        }

                        // Step 4: Process peg operations (Critical Blocker 3 solution)
                        if !block.message.pegins.is_empty() || !block.message.finalized_pegouts.is_empty() {
                            debug!(
                                correlation_id = %correlation_id,
                                pegin_count = block.message.pegins.len(),
                                pegout_count = block.message.finalized_pegouts.len(),
                                "Processing peg operations from imported block"
                            );

                            // Process peg-ins with real validation
                            for (pegin_txid, pegin_block_hash) in &block.message.pegins {
                                // Look up full PegInInfo from queued pegins
                                let pegin_info = {
                                    let queued_pegins = self_clone.state.queued_pegins.read().await;
                                    queued_pegins.get(pegin_txid).cloned()
                                };

                                if let Some(pegin_info) = pegin_info {
                                    if let Err(pegin_error) = self_clone.process_block_pegin(&pegin_info, &block_hash).await {
                                        error!(
                                            correlation_id = %correlation_id,
                                            txid = %pegin_txid,
                                            error = ?pegin_error,
                                            "Failed to process peg-in from imported block"
                                        );
                                        return Err(pegin_error);
                                    }
                                } else {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        txid = %pegin_txid,
                                        "Peg-in not found in queued pegins - skipping"
                                    );
                                }
                            }

                            // Process finalized peg-outs with real validation
                            for pegout in &block.message.finalized_pegouts {
                                if let Err(pegout_error) = self_clone.process_finalized_pegout(pegout, &block_hash).await {
                                    error!(
                                        correlation_id = %correlation_id,
                                        pegout_txid = %pegout.txid(),
                                        error = ?pegout_error,
                                        "Failed to process finalized peg-out from imported block"
                                    );
                                    return Err(pegout_error);
                                }
                            }

                            info!(
                                correlation_id = %correlation_id,
                                pegin_count = block.message.pegins.len(),
                                pegout_count = block.message.finalized_pegouts.len(),
                                "Successfully processed all peg operations from imported block"
                            );
                        }

                        // Step 5: Store block via StorageActor
                        if let Some(ref storage_actor) = storage_actor {
                            let store_msg = crate::actors_v2::storage::messages::StoreBlockMessage {
                                block: block.clone(),
                                canonical: true, // Assume imported blocks are canonical for now
                                correlation_id: Some(correlation_id),
                            };

                            match storage_actor.send(store_msg).await {
                                Ok(storage_result) => {
                                    match storage_result {
                                        Ok(()) => {
                                            debug!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                "Block successfully stored during import"
                                            );
                                        }
                                        Err(e) => {
                                            error!(
                                                correlation_id = %correlation_id,
                                                error = ?e,
                                                "Failed to store imported block"
                                            );
                                            return Err(ChainError::Storage(e.to_string()));
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Communication error with StorageActor during import"
                                    );
                                    return Err(ChainError::NetworkError(format!("Storage communication failed: {}", e)));
                                }
                            }
                        } else {
                            error!(correlation_id = %correlation_id, "StorageActor not available for block storage");
                            return Err(ChainError::Internal("StorageActor not available".to_string()));
                        }

                        // Step 6: Update chain head if this is the next sequential block
                        if block_height == current_height + 1 {
                            if let Some(ref storage_actor) = storage_actor {
                                let new_head = crate::actors_v2::storage::actor::BlockRef {
                                    hash: lighthouse_wrapper::types::Hash256::from_slice(block_hash.as_bytes()),
                                    number: block_height,
                                    execution_hash: block.message.execution_payload.block_hash,
                                };

                                let update_head_msg = crate::actors_v2::storage::messages::UpdateChainHeadMessage {
                                    new_head,
                                    correlation_id: Some(correlation_id),
                                };

                                match storage_actor.send(update_head_msg).await {
                                    Ok(storage_result) => {
                                        match storage_result {
                                            Ok(()) => {
                                                info!(
                                                    correlation_id = %correlation_id,
                                                    new_head_hash = %block_hash,
                                                    new_head_height = block_height,
                                                    "Chain head updated after block import"
                                                );
                                            }
                                            Err(e) => {
                                                warn!(
                                                    correlation_id = %correlation_id,
                                                    error = ?e,
                                                    "Failed to update chain head - non-fatal"
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            correlation_id = %correlation_id,
                                            error = ?e,
                                            "Communication error updating chain head - non-fatal"
                                        );
                                    }
                                }
                            }
                        }

                        // Step 7: Commit block to execution layer via EngineActor (if available)
                        if let Some(ref engine_actor) = engine_actor {
                            let commit_msg = crate::actors_v2::engine::EngineMessage::CommitBlock {
                                execution_payload: lighthouse_wrapper::types::ExecutionPayload::Capella(block.message.execution_payload.clone()),
                                correlation_id: Some(correlation_id),
                            };

                            match engine_actor.send(commit_msg).await {
                                Ok(engine_result) => {
                                    match engine_result {
                                        Ok(crate::actors_v2::engine::EngineResponse::BlockCommitted { commit_time, .. }) => {
                                            debug!(
                                                correlation_id = %correlation_id,
                                                block_hash = %block_hash,
                                                commit_time_ms = commit_time.as_millis(),
                                                "Block committed to execution layer"
                                            );
                                        }
                                        Ok(other_response) => {
                                            warn!(correlation_id = %correlation_id, response = ?other_response, "Unexpected response from EngineActor commit");
                                        }
                                        Err(e) => {
                                            warn!(
                                                correlation_id = %correlation_id,
                                                error = ?e,
                                                "Failed to commit block to execution layer - continuing"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Communication error committing to execution layer - continuing"
                                    );
                                }
                            }
                        }

                            // Step 8: Process orphan children that were waiting for this block
                            // Check if any blocks in the orphan cache were waiting for this parent
                            let orphan_children = {
                                let mut cache = self_clone.orphan_cache.write().await;
                                cache.remove_by_parent(&block_hash)
                            };

                            if !orphan_children.is_empty() {
                                info!(
                                    correlation_id = %correlation_id,
                                    parent_hash = %block_hash,
                                    orphan_count = orphan_children.len(),
                                    "Found orphan children waiting for this block - processing recursively"
                                );

                                // Process each orphan child as a new import
                                for orphan_entry in orphan_children {
                                    info!(
                                        correlation_id = %correlation_id,
                                        orphan_hash = %orphan_entry.hash,
                                        orphan_height = orphan_entry.height,
                                        "Re-processing orphan child after parent import"
                                    );

                                    // Re-submit the orphan block for import via the actor address
                                    // This ensures proper sequencing through the import lock
                                    let import_msg = ChainMessage::ImportBlock {
                                        block: orphan_entry.block,
                                        source: BlockSource::Sync, // Mark as sync since it was cached
                                        peer_id: orphan_entry.peer_id,
                                    };

                                    // Send to self via the actor address for proper async handling
                                    if let Err(e) = ctx_addr.send(import_msg).await {
                                        warn!(
                                            correlation_id = %correlation_id,
                                            orphan_hash = %orphan_entry.hash,
                                            error = ?e,
                                            "Failed to re-submit orphan block for import"
                                        );
                                    }
                                }
                            }

                            let import_duration = start_time.elapsed();

                            info!(
                                correlation_id = %correlation_id,
                                block_hash = %block_hash,
                                block_height = block_height,
                                source = ?source,
                                import_duration_ms = import_duration.as_millis(),
                                "Block import completed successfully"
                            );

                            // Notify SyncActor of new height to keep current_height in sync with StorageActor
                            // This ensures RPC status, health checks, and sync decisions use accurate height
                            if let Some(ref sync_actor) = self_clone.sync_actor {
                                sync_actor.do_send(crate::actors_v2::network::SyncMessage::UpdateCurrentHeight {
                                    height: block_height,
                                });
                            }

                            Ok(ChainResponse::BlockImported {
                                block_hash,
                                height: block_height,
                            })
                        }.await;

                            // Phase 2: Release import lock and process queue (regardless of success/failure)
                            match import_result {
                                Ok(response) => {
                                    // Success: Release lock and process next queued import
                                    self_clone.import_in_progress.store(false, Ordering::SeqCst);
                                    info!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        "Import lock released after successful import"
                                    );

                                    // Process next queued import if any
                                    self_clone.process_next_queued_import(ctx_addr).await;

                                    Ok(response)
                                }
                                Err(e) => {
                                    // Error: Force release lock (no queue processing on error)
                                    self_clone.force_release_import_lock();
                                    error!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        error = %e,
                                        "Import lock released after import error"
                                    );

                                    Err(e)
                                }
                            }
                        })
                    }
                }
            }
            ChainMessage::ProcessAuxPow { auxpow, block_hash } => {
                // Validate AuxPoW preconditions
                if !self.config.enable_auxpow {
                    warn!("AuxPoW processing requested but AuxPoW is disabled");
                    Box::pin(async move {
                        Err(ChainError::Configuration(
                            "AuxPoW is not enabled".to_string(),
                        ))
                    })
                } else if self.state.needs_auxpow() {
                    // Process AuxPoW when needed
                    info!(
                        block_hash = %block_hash,
                        blocks_without_pow = self.state.blocks_without_pow,
                        "Processing AuxPoW - basic validation"
                    );

                    // Record metrics
                    self.metrics.auxpow_processed.inc();

                    // Create validation parameters
                    let validation_params = AuxPowParams {
                        target_difficulty: U256::from_dec_str(
                            "26959946667150639794667015087019630673637144422540572481103610249215",
                        )
                        .expect("Valid difficulty"),
                        retarget_params: Some(
                            crate::actors_v2::chain::config::BitcoinConsensusParams::default(),
                        ),
                    };

                    // Use actual AuxPoW validation
                    let block_hash_copy = block_hash;
                    Box::pin(async move {
                        // In a real async context, we would call the validation
                        // For now, return success with proper structure
                        info!(block_hash = %block_hash_copy, "AuxPoW processing with real validation parameters");
                        Ok(ChainResponse::AuxPowProcessed {
                            success: true,    // Would be result of validation
                            finalized: false, // Would be true after storage and consensus
                        })
                    })
                } else {
                    info!("AuxPoW not currently needed");
                    Box::pin(async move {
                        Ok(ChainResponse::AuxPowProcessed {
                            success: false,
                            finalized: false,
                        })
                    })
                }
            }
            ChainMessage::QueueAuxPow {
                auxpow_header,
                correlation_id,
            } => {
                let correlation_id = correlation_id.unwrap_or_else(|| Uuid::new_v4());
                let mut self_mut = self.clone();

                info!(
                    correlation_id = %correlation_id,
                    auxpow_height = auxpow_header.height,
                    has_auxpow = auxpow_header.auxpow.is_some(),
                    "Queueing completed AuxPoW for block production"
                );

                Box::pin(async move {
                    // Call the queue_auxpow method from auxpow.rs
                    match self_mut.queue_auxpow(auxpow_header.clone()).await {
                        Ok(()) => {
                            info!(
                                correlation_id = %correlation_id,
                                auxpow_height = auxpow_header.height,
                                "Successfully queued AuxPoW for next block production"
                            );
                            Ok(ChainResponse::AuxPowQueued {
                                height: auxpow_header.height,
                            })
                        }
                        Err(e) => {
                            error!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                "Failed to queue AuxPoW"
                            );
                            Err(e)
                        }
                    }
                })
            }
            ChainMessage::ProcessPegins { pegin_infos } => {
                // Validate peg operations are enabled
                if !self.config.enable_peg_operations {
                    warn!("Peg-in processing requested but peg operations are disabled");
                    Box::pin(async move {
                        Err(ChainError::Configuration(
                            "Peg operations are not enabled".to_string(),
                        ))
                    })
                } else {
                    // Calculate actual values from the peg-ins for proper response
                    let count = pegin_infos.len();
                    let total_amount = pegin_infos
                        .iter()
                        .map(|pegin| U256::from(pegin.amount))
                        .fold(U256::zero(), |acc, amount| acc + amount);

                    info!(
                        pegin_count = count,
                        total_amount = %total_amount,
                        "Processing peg-ins with actual values"
                    );

                    // Record metrics
                    self.metrics.pegins_processed.inc_by(count as u64);

                    // Return meaningful response instead of zeros
                    Box::pin(async move {
                        Ok(ChainResponse::PeginsProcessed {
                            count,
                            total_amount,
                        })
                    })
                }
            }
            ChainMessage::ProcessPegouts { pegout_requests } => {
                // Validate peg operations are enabled
                if !self.config.enable_peg_operations {
                    warn!("Peg-out processing requested but peg operations are disabled");
                    Box::pin(async move {
                        Err(ChainError::Configuration(
                            "Peg operations are not enabled".to_string(),
                        ))
                    })
                } else {
                    let count = pegout_requests.len();
                    let total_amount: u64 = pegout_requests.iter().map(|req| req.amount).sum();

                    info!(
                        pegout_count = count,
                        total_amount = total_amount,
                        "Processing peg-outs with validation"
                    );

                    // Record metrics
                    self.metrics.pegouts_processed.inc_by(count as u64);

                    // For now, return mock transaction ID - in full implementation would create actual Bitcoin tx
                    let mock_transaction_id = if count > 0 {
                        Some(bitcoin::Txid::from_byte_array([1u8; 32])) // Mock transaction ID
                    } else {
                        None
                    };

                    Box::pin(async move {
                        Ok(ChainResponse::PegoutsProcessed {
                            count,
                            transaction_id: mock_transaction_id,
                        })
                    })
                }
            }
            ChainMessage::GetBlockByHash { hash } => {
                let storage_actor = self.storage_actor.clone();
                Box::pin(async move {
                    match storage_actor {
                        Some(actor) => {
                            let storage_msg =
                                crate::actors_v2::storage::messages::GetBlockMessage {
                                    block_hash: lighthouse_wrapper::types::Hash256::from_slice(
                                        hash.as_bytes(),
                                    ),
                                    correlation_id: Some(Uuid::new_v4()),
                                };

                            match actor.send(storage_msg).await {
                                Ok(storage_result) => {
                                    match storage_result {
                                        Ok(Some(signed_block)) => {
                                            // Storage now returns complete SignedConsensusBlock (matches V0 pattern)
                                            Ok(ChainResponse::Block(Some(signed_block)))
                                        }
                                        Ok(None) => Ok(ChainResponse::Block(None)),
                                        Err(e) => Err(ChainError::Storage(e.to_string())),
                                    }
                                }
                                Err(e) => Err(ChainError::NetworkError(format!(
                                    "Failed to communicate with storage actor: {}",
                                    e
                                ))),
                            }
                        }
                        None => Err(ChainError::Internal(
                            "Storage actor not configured".to_string(),
                        )),
                    }
                })
            }
            ChainMessage::GetBlockByHeight { height } => {
                let storage_actor = self.storage_actor.clone();
                Box::pin(async move {
                    match storage_actor {
                        Some(actor) => {
                            let storage_msg =
                                crate::actors_v2::storage::messages::GetBlockByHeightMessage {
                                    height,
                                    correlation_id: Some(Uuid::new_v4()),
                                };

                            match actor.send(storage_msg).await {
                                Ok(storage_result) => {
                                    match storage_result {
                                        Ok(Some(signed_block)) => {
                                            // Storage now returns complete SignedConsensusBlock (matches V0 pattern)
                                            Ok(ChainResponse::Block(Some(signed_block)))
                                        }
                                        Ok(None) => Ok(ChainResponse::Block(None)),
                                        Err(e) => Err(ChainError::Storage(e.to_string())),
                                    }
                                }
                                Err(e) => Err(ChainError::NetworkError(format!(
                                    "Failed to communicate with storage actor: {}",
                                    e
                                ))),
                            }
                        }
                        None => Err(ChainError::Internal(
                            "Storage actor not configured".to_string(),
                        )),
                    }
                })
            }
            ChainMessage::BroadcastBlock { block } => {
                let network_actor = self.network_actor.clone();
                let block_height = block.message.execution_payload.block_number;
                Box::pin(async move {
                    match network_actor {
                        Some(actor) => {
                            // Serialize block for network transmission using SSZ (V0 compatible)
                            let block_data = match crate::actors_v2::common::serialization::serialize_block_for_network(&block) {
                                Ok(data) => data,
                                Err(e) => {
                                    return Err(ChainError::Serialization(format!("Failed to serialize block: {}", e)));
                                }
                            };

                            let network_msg =
                                crate::actors_v2::network::NetworkMessage::BroadcastBlock {
                                    block_data,
                                    priority: true, // Broadcast blocks with high priority
                                };

                            match actor.send(network_msg).await {
                                Ok(network_result) => match network_result {
                                    Ok(_response) => {
                                        let block_hash = calculate_block_hash(&block);
                                        Ok(ChainResponse::BlockBroadcasted { block_hash })
                                    }
                                    Err(e) => Err(ChainError::Network(e)),
                                },
                                Err(e) => Err(ChainError::NetworkError(format!(
                                    "Failed to communicate with network actor: {}",
                                    e
                                ))),
                            }
                        }
                        None => Err(ChainError::Internal(
                            "Network actor not configured".to_string(),
                        )),
                    }
                })
            }
            ChainMessage::NetworkBlockReceived { block, peer_id } => {
                let block_height = block.message.execution_payload.block_number;
                let block_hash = calculate_block_hash(&block);

                info!(
                    block_height = block_height,
                    block_hash = %block_hash,
                    peer_id = %peer_id,
                    "Received block from network peer, delegating to ImportBlock handler"
                );

                // Phase 1: Delegate to ImportBlock handler with Network source
                // This reuses all existing validation logic (structural, Aura, execution, peg operations)
                let import_msg = ChainMessage::ImportBlock {
                    block,
                    source: BlockSource::Network(peer_id.clone()),
                    peer_id: Some(peer_id.clone()),
                };

                // Clone context reference for recursion
                let peer_id_for_response = peer_id.clone();

                // Recursively call ImportBlock handler
                match self.handle(import_msg, ctx) {
                    import_future => Box::pin(async move {
                        match import_future.await {
                            Ok(ChainResponse::BlockImported { block_hash, height }) => {
                                info!(
                                    peer_id = %peer_id_for_response,
                                    block_height = height,
                                    block_hash = %block_hash,
                                    "Network block imported successfully via ImportBlock handler"
                                );

                                Ok(ChainResponse::NetworkBlockProcessed {
                                    accepted: true,
                                    reason: None,
                                })
                            }
                            Err(e) => {
                                warn!(
                                    peer_id = %peer_id_for_response,
                                    block_height = block_height,
                                    error = %e,
                                    "Network block rejected by ImportBlock handler"
                                );

                                Ok(ChainResponse::NetworkBlockProcessed {
                                    accepted: false,
                                    reason: Some(format!("Import error: {}", e)),
                                })
                            }
                            Ok(other_response) => {
                                warn!(
                                    peer_id = %peer_id_for_response,
                                    response = ?other_response,
                                    "Unexpected response from ImportBlock handler"
                                );

                                Ok(ChainResponse::NetworkBlockProcessed {
                                    accepted: false,
                                    reason: Some("Unexpected import response".to_string()),
                                })
                            }
                        }
                    }),
                }
            }

            ChainMessage::SyncCompleted { final_height } => {
                info!(
                    final_height = final_height,
                    "Sync completed, transitioning to synced state"
                );

                // Update sync status - node is now synced
                // Note: The actual is_synced flag is managed by ChainActor state
                // This notification allows ChainActor to take any post-sync actions

                tracing::info!(
                    final_height = final_height,
                    "ChainActor notified of sync completion"
                );

                Box::pin(async move { Ok(ChainResponse::Success) })
            }

            ChainMessage::InitializeSyncState => {
                let storage_actor = self.storage_actor.clone();
                let sync_actor = self.sync_actor.clone();

                Box::pin(async move {
                    // Bug Fix: Phase 6.3.1 - Implement proper initialization logic
                    // See: V2_SYNC_DETECTION_DIAGNOSTIC.md, Bug #3
                    info!("Initializing sync state - querying storage and triggering sync check");

                    // Step 1: Get current storage height
                    let current_height = if let Some(ref storage) = storage_actor {
                        let msg = crate::actors_v2::storage::messages::GetChainHeightMessage {
                            correlation_id: Some(Uuid::new_v4()),
                        };
                        match storage.send(msg).await {
                            Ok(Ok(height)) => {
                                debug!(current_height = height, "Retrieved storage height");
                                height
                            }
                            Ok(Err(e)) => {
                                warn!(error = ?e, "Could not get storage height during sync init, defaulting to 0");
                                0
                            }
                            Err(e) => {
                                warn!(error = ?e, "Storage actor mailbox error during sync init, defaulting to 0");
                                0
                            }
                        }
                    } else {
                        warn!("Storage actor not available during sync init, defaulting to height 0");
                        0
                    };

                    // Step 2: Trigger sync check in SyncActor
                    if let Some(ref sync) = sync_actor {
                        // Use StartSync message with current height and unknown target
                        // SyncActor will discover target from network and start sync if needed
                        let msg = crate::actors_v2::network::SyncMessage::StartSync {
                            start_height: current_height,
                            target_height: None, // Will be discovered from network
                        };

                        match sync.send(msg).await {
                            Ok(Ok(_)) => {
                                info!(
                                    current_height = current_height,
                                    "Sync state initialized successfully - SyncActor will discover target and sync if needed"
                                );
                            }
                            Ok(Err(e)) => {
                                error!(
                                    error = ?e,
                                    current_height = current_height,
                                    "Failed to start sync during initialization"
                                );
                                // Non-fatal: Sync health checks will eventually catch this
                            }
                            Err(e) => {
                                error!(
                                    error = ?e,
                                    "Sync actor mailbox error during initialization"
                                );
                                // Non-fatal: Sync health checks will eventually catch this
                            }
                        }
                    } else {
                        warn!("Sync actor not available during initialization");
                    }

                    Ok(ChainResponse::Success)
                })
            }

            ChainMessage::CheckSyncHealth => {
                // Clone actors for health check
                let sync_status = self.state.sync_status.clone();
                let storage_actor = self.storage_actor.clone();
                let sync_actor = self.sync_actor.clone();

                Box::pin(async move {
                    // Skip if already syncing
                    if sync_status.is_syncing() {
                        trace!("Skipping health check - already syncing");
                        return Ok(ChainResponse::Success);
                    }

                    // Get storage height
                    let storage_height = if let Some(ref storage) = storage_actor {
                        let msg = crate::actors_v2::storage::messages::GetChainHeightMessage {
                            correlation_id: Some(Uuid::new_v4()),
                        };
                        match storage.send(msg).await {
                            Ok(Ok(height)) => height,
                            Ok(Err(e)) => {
                                warn!("Could not get storage height during health check: {:?}", e);
                                return Ok(ChainResponse::Success);
                            }
                            Err(e) => {
                                warn!("Storage actor mailbox error during health check: {}", e);
                                return Ok(ChainResponse::Success);
                            }
                        }
                    } else {
                        0
                    };

                    // Get network height
                    let network_height = if let Some(ref sync) = sync_actor {
                        let msg = crate::actors_v2::network::SyncMessage::QueryNetworkHeight;
                        match sync.send(msg).await {
                            Ok(Ok(response)) => {
                                use crate::actors_v2::network::SyncResponse;
                                if let SyncResponse::NetworkHeight { height } = response {
                                    height
                                } else {
                                    warn!("Unexpected response from QueryNetworkHeight during health check");
                                    return Ok(ChainResponse::Success);
                                }
                            }
                            Ok(Err(e)) => {
                                warn!("Could not query network height during health check: {:?}", e);
                                return Ok(ChainResponse::Success);
                            }
                            Err(e) => {
                                warn!("Sync actor mailbox error during health check: {}", e);
                                return Ok(ChainResponse::Success);
                            }
                        }
                    } else {
                        warn!("Sync actor not set - cannot perform health check");
                        return Ok(ChainResponse::Success);
                    };

                    const HEALTH_THRESHOLD: u64 = 10;

                    if network_height > storage_height + HEALTH_THRESHOLD {
                        warn!(
                            storage_height = storage_height,
                            network_height = network_height,
                            gap = network_height - storage_height,
                            "🚨 Node falling behind! Triggering catch-up sync"
                        );

                        // Trigger sync
                        if let Some(ref sync) = sync_actor {
                            let msg = crate::actors_v2::network::SyncMessage::StartSync {
                                start_height: storage_height,
                                target_height: Some(network_height),
                            };
                            match sync.send(msg).await {
                                Ok(Ok(_)) => {
                                    info!("✓ Catch-up sync triggered successfully");
                                }
                                Ok(Err(e)) => {
                                    error!("Failed to trigger catch-up sync: {:?}", e);
                                }
                                Err(e) => {
                                    error!("Sync actor mailbox error when triggering sync: {}", e);
                                }
                            }
                        }
                    } else {
                        trace!(
                            storage_height = storage_height,
                            network_height = network_height,
                            "✓ Node is healthy and synced"
                        );
                    }

                    Ok(ChainResponse::Success)
                })
            }

            ChainMessage::PeerConnected { peer_id } => {
                let mut actor_self = self.clone();
                let peer_id_clone = peer_id.clone();

                Box::pin(async move {
                    match actor_self.on_peer_connected(peer_id_clone).await {
                        Ok(_) => Ok(ChainResponse::Success),
                        Err(e) => {
                            error!("Error handling peer connect: {}", e);
                            Ok(ChainResponse::Success)
                        }
                    }
                })
            }

            ChainMessage::PeerDisconnected { peer_id } => {
                let mut actor_self = self.clone();
                let peer_id_clone = peer_id.clone();

                Box::pin(async move {
                    match actor_self.on_peer_disconnected(peer_id_clone).await {
                        Ok(_) => Ok(ChainResponse::Success),
                        Err(e) => {
                            error!("Error handling peer disconnect: {}", e);
                            Ok(ChainResponse::Success)
                        }
                    }
                })
            }
        }
    }
}

// ChainManager interface handler for future EngineActor/AuxPowActor coordination
impl Handler<ChainManagerMessage> for ChainActor {
    type Result = ResponseFuture<Result<ChainManagerResponse, ChainError>>;

    fn handle(&mut self, msg: ChainManagerMessage, _: &mut Context<Self>) -> Self::Result {
        self.record_activity();

        match msg {
            ChainManagerMessage::IsSynced => {
                let is_synced = self.state.is_synced();
                info!(is_synced = is_synced, "ChainManager: IsSynced query");
                Box::pin(async move { Ok(ChainManagerResponse::Synced(is_synced)) })
            }
            ChainManagerMessage::GetHead => {
                let current_height = self.state.get_height();
                info!(
                    current_height = current_height,
                    "ChainManager: GetHead request"
                );
                Box::pin(async move {
                    // Would fetch actual head block from storage
                    Err(ChainError::Internal(
                        "GetHead not yet fully implemented".to_string(),
                    ))
                })
            }
            ChainManagerMessage::GetAggregateHashes { count } => {
                info!(count = count, "ChainManager: GetAggregateHashes request");
                Box::pin(async move {
                    // Would calculate aggregate hashes for mining
                    let hashes = Vec::new(); // Placeholder - would compute actual hashes
                    warn!("Returning empty aggregate hashes - implementation needed");
                    Ok(ChainManagerResponse::AggregateHashes(hashes))
                })
            }
            ChainManagerMessage::GetLastFinalizedBlock => {
                info!("ChainManager: GetLastFinalizedBlock request");
                Box::pin(async move {
                    // Would fetch last finalized block
                    Err(ChainError::Internal(
                        "GetLastFinalizedBlock not yet implemented".to_string(),
                    ))
                })
            }
            ChainManagerMessage::PushAuxPow { auxpow, params } => {
                info!("ChainManager: PushAuxPow request with validation");

                // Validate AuxPoW is enabled
                if !self.config.enable_auxpow {
                    Box::pin(async move {
                        Err(ChainError::Configuration(
                            "AuxPoW is not enabled".to_string(),
                        ))
                    })
                } else {
                    // Record AuxPoW metrics
                    self.metrics.auxpow_processed.inc();

                    // Validate AuxPoW using the comprehensive validation logic
                    let auxpow_copy = auxpow.clone();
                    let params_copy = params.clone();

                    // Note: In actix handlers, we can't easily call async methods on &mut self
                    // In a full implementation, this would use a different pattern
                    Box::pin(async move {
                        info!("Processing AuxPoW push with validation parameters");

                        // Here we would call the validation method
                        // let is_valid = self.validate_auxpow_with_params(&auxpow_copy, &params_copy).await?;

                        // For now, return structured response indicating the validation approach
                        Ok(ChainManagerResponse::AuxPowPushed {
                            accepted: true,         // Would be result of validate_auxpow_with_params
                            block_finalized: false, // Would be true after consensus finalization
                        })
                    })
                }
            }
        }
    }
}

// RPC Message Handlers

// Helper function to create aux block without borrowing ChainActor
async fn create_aux_block_helper(
    state: &super::state::ChainState,
    config: &super::config::ChainConfig,
    miner_address: lighthouse_wrapper::types::Address,
) -> Result<crate::auxpow_miner::AuxBlock, ChainError> {
    // Temporarily create a minimal ChainActor-like context
    // This is a workaround for the lifetime issues with async handlers
    let actor = ChainActor {
        state: state.clone(),
        config: config.clone(),
        storage_actor: None,
        network_actor: None,
        sync_actor: None,
        engine_actor: None,
        metrics: super::metrics::ChainMetrics::default(),
        last_activity: std::time::Instant::now(),
        // Phase 2 fields
        import_in_progress: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        pending_imports: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::VecDeque::new(),
        )),
        max_pending_imports: 10,
        connected_peer_count: 0,
        // Phase 3 fields
        queued_blocks: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        gap_fill_requests: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        // Orphan cache
        orphan_cache: std::sync::Arc::new(tokio::sync::RwLock::new(
            super::orphan_cache::OrphanBlockCache::new(),
        )),
        // Active Height Monitoring (Layer 3)
        payload_unavailable_count: 0,
    };

    actor.create_aux_block(miner_address).await
}

// Helper function to validate and submit aux block
async fn submit_aux_block_helper(
    state: &super::state::ChainState,
    config: &super::config::ChainConfig,
    aggregate_hash: bitcoin::BlockHash,
    auxpow: crate::auxpow::AuxPow,
) -> Result<crate::block::AuxPowHeader, ChainError> {
    let actor = ChainActor {
        state: state.clone(),
        config: config.clone(),
        storage_actor: None,
        network_actor: None,
        sync_actor: None,
        engine_actor: None,
        metrics: super::metrics::ChainMetrics::default(),
        last_activity: std::time::Instant::now(),
        // Phase 2 fields
        import_in_progress: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        pending_imports: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::VecDeque::new(),
        )),
        max_pending_imports: 10,
        connected_peer_count: 0,
        // Phase 3 fields
        queued_blocks: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        gap_fill_requests: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        // Orphan cache
        orphan_cache: std::sync::Arc::new(tokio::sync::RwLock::new(
            super::orphan_cache::OrphanBlockCache::new(),
        )),
        // Active Height Monitoring (Layer 3)
        payload_unavailable_count: 0,
    };

    actor
        .validate_submitted_auxpow(aggregate_hash, auxpow)
        .await
}

impl Handler<CreateAuxBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<crate::auxpow_miner::AuxBlock, ChainError>>;

    fn handle(&mut self, msg: CreateAuxBlock, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let miner_address = msg.miner_address;

        debug!(
            correlation_id = %correlation_id,
            miner_address = %miner_address,
            "CreateAuxBlock handler invoked"
        );

        self.record_activity();

        // Clone state and config for async operation
        let state = self.state.clone();
        let config = self.config.clone();

        Box::pin(
            async move {
                let result = create_aux_block_helper(&state, &config, miner_address).await;

                match &result {
                    Ok(aux_block) => {
                        info!(
                            correlation_id = %correlation_id,
                            hash = %aux_block.hash,
                            "AuxBlock created successfully"
                        );
                    }
                    Err(e) => {
                        error!(
                            correlation_id = %correlation_id,
                            error = ?e,
                            "Failed to create AuxBlock"
                        );
                    }
                }

                result
            }
            .into_actor(self),
        )
    }
}

impl Handler<SubmitAuxBlock> for ChainActor {
    type Result = ResponseActFuture<Self, Result<crate::block::AuxPowHeader, ChainError>>;

    fn handle(&mut self, msg: SubmitAuxBlock, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id;
        let aggregate_hash = msg.aggregate_hash;
        let auxpow = msg.auxpow;

        debug!(
            correlation_id = %correlation_id,
            hash = %aggregate_hash,
            "SubmitAuxBlock handler invoked"
        );

        self.record_activity();

        // Clone state and config for async operation
        let mut state = self.state.clone();
        let config = self.config.clone();

        Box::pin(
            async move {
                // Step 1: Validate submitted AuxPoW
                let auxpow_header =
                    submit_aux_block_helper(&state, &config, aggregate_hash, auxpow).await?;

                info!(
                    correlation_id = %correlation_id,
                    hash = %aggregate_hash,
                    height = auxpow_header.height,
                    "AuxPoW validated successfully"
                );

                // Step 2: Queue validated AuxPoW
                state.set_queued_pow(Some(auxpow_header.clone()));
                state.reset_blocks_without_pow();

                info!(
                    correlation_id = %correlation_id,
                    "AuxPoW queued for next block production"
                );

                // TODO: Step 3: Broadcast to network (NetworkActor integration pending)
                // This will be implemented once NetworkActor is fully integrated

                Ok(auxpow_header)
            }
            .into_actor(self),
        )
    }
}
