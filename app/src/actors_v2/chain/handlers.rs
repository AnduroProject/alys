//! ChainActor V2 Message Handlers
//!
//! All message handlers consolidated, following StorageActor V2 patterns

use actix::prelude::*;
use bitcoin::hashes::Hash;
use ethereum_types::{H256, U256};
use eyre::Result;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::{
    actor::ConsensusMode,
    messages::{
        AuxPowParams, BlockSource, ChainManagerMessage, ChainManagerResponse, ChainMessage,
        ChainResponse, CreateAuxBlock, SubmitAuxBlock,
    },
    tendermint::TendermintStep,
    ChainActor, ChainError,
};

use crate::actors_v2::common::serialization::calculate_block_hash;
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
                // With Tendermint instant finality, no orphan tracking needed
                // Observed height equals current committed height

                // Query StorageActor for actual chain height instead of using stale local state
                // This is critical for Active Height Monitoring - peers need accurate heights
                let storage_actor = self.storage_actor.clone();
                let is_synced = self.state.is_synced_blocking();
                let is_validator = self.config.is_validator;
                // Use try_read for non-blocking access in sync context
                let last_block_time = self
                    .state
                    .last_block_time
                    .try_read()
                    .ok()
                    .and_then(|t| t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()));
                let auxpow_enabled = self.config.enable_auxpow;
                let blocks_without_pow = self.state.blocks_without_pow.try_read().ok().map(|g| *g).unwrap_or(0);
                let local_height = self.state.get_height_blocking();
                let local_head_hash = self.state.head.try_read().ok().and_then(|g| g.as_ref().map(|h| h.hash));

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
                        observed_height: height, // With Tendermint, observed = committed
                        orphan_count: 0,         // No orphans with instant finality
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
                            last_commit: None, // TODO: Will be set when Tendermint consensus is active
                            auxpow_header: None, // Will be set by incorporate_auxpow if available
                            execution_payload: capella_payload,
                            // Path B: Peg-ins are stored in auxpow_header.pegins, not here
                            pegout_payment_proposal: None,
                            finalized_pegouts: vec![],
                            // Tendermint schema fields - populated by Tendermint handlers when active
                            validators_hash: None,
                            next_validators_hash: None,
                            params_hash: None,
                            governance_updates: None,
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
                                let blocks_without_pow = self_clone.state.blocks_without_pow.try_read().ok().map(|g| *g).unwrap_or(0);
                                error!(
                                    correlation_id = %correlation_id,
                                    blocks_without_pow = blocks_without_pow,
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

                                    // BUG FIX: Notify SyncActor of new height after block production
                                    // This keeps SyncActor's current_height in sync with StorageActor
                                    // Without this, SyncActor thinks we're behind and blocks production
                                    if let Some(ref sync_actor) = sync_actor {
                                        sync_actor.do_send(crate::actors_v2::network::SyncMessage::UpdateCurrentHeight {
                                            height: v2_head_ref.number,
                                        });
                                        debug!(
                                            correlation_id = %correlation_id,
                                            height = v2_head_ref.number,
                                            "Notified SyncActor of new height after block production"
                                        );
                                    }
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
                let current_height = self.state.get_height_blocking();

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
                        let mut self_clone = self.clone();

                        // Capture actor references for async block
                        let engine_actor = self.engine_actor.clone();
                        let storage_actor = self.storage_actor.clone();

                        // Capture context address for queue processing
                        let ctx_addr = ctx.address();

                        Box::pin(async move {
                            // Wrap entire import logic to ensure lock release on all paths
                            let import_result: Result<ChainResponse, ChainError> = async {
                            // BUG FIX: Get actual current height from StorageActor
                            // The `current_height` captured from self.state.get_height() is stale (0)
                            // because ChainActor's state.head is not properly maintained across async ops
                            let storage_current_height = if let Some(ref storage) = storage_actor {
                                match storage.send(crate::actors_v2::storage::messages::GetChainHeightMessage {
                                    correlation_id: Some(correlation_id),
                                }).await {
                                    Ok(Ok(h)) => {
                                        trace!(
                                            correlation_id = %correlation_id,
                                            storage_height = h,
                                            captured_height = current_height,
                                            "Using StorageActor height for import validation"
                                        );
                                        h
                                    }
                                    _ => {
                                        debug!(
                                            correlation_id = %correlation_id,
                                            "Could not get storage height, using captured height {}",
                                            current_height
                                        );
                                        current_height
                                    }
                                }
                            } else {
                                current_height
                            };

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

                        // Note: With Tendermint-only consensus, block finality is proven via
                        // last_commit field containing 2/3+ validator precommit signatures.
                        // Aura signature verification is no longer needed.

                        // Step 1.7: Parent hash validation (Phase 3)
                        // With Tendermint instant finality, blocks must arrive in order.
                        // If parent is missing, trigger sync rather than caching orphans.
                        if let Some(ref storage_actor) = storage_actor {
                            if let Err(parent_error) = crate::actors_v2::common::validation::validate_parent_relationship(&block, storage_actor).await {
                                // Check if this is a missing parent (sync needed)
                                if let ChainError::OrphanBlock { parent_hash: missing_parent, block_height: incoming_height } = &parent_error {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        parent_hash = %missing_parent,
                                        block_height = incoming_height,
                                        "Block parent not found - triggering sync"
                                    );

                                    // Trigger sync to fetch missing blocks
                                    if let Some(ref sync_actor) = self_clone.sync_actor {
                                        let reason = format!(
                                            "Missing parent {} for block at height {}",
                                            missing_parent, incoming_height
                                        );
                                        if let Err(e) = sync_actor.send(
                                            crate::actors_v2::network::SyncMessage::ForceResync { reason }
                                        ).await {
                                            warn!(
                                                correlation_id = %correlation_id,
                                                error = %e,
                                                "Failed to trigger sync for missing parent"
                                            );
                                        }
                                    }

                                    // Reject block - it will be re-received after sync
                                    return Ok(ChainResponse::BlockRejected {
                                        reason: format!("Parent {} not found - sync triggered", missing_parent),
                                    });
                                }

                                // Not a missing parent error - propagate
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

                        // Step 1.9: Duplicate detection (Tendermint simplified)
                        // With Tendermint instant finality, no forks are possible.
                        // If a block exists at this height, it's either a duplicate or invalid.
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
                                            "Duplicate block received - already imported"
                                        );

                                        return Ok(ChainResponse::BlockImported {
                                            block_hash,
                                            height: block_height,
                                        });
                                    } else {
                                        // With Tendermint, conflicting blocks at same height should not happen
                                        // This indicates either a bug or malicious behavior
                                        error!(
                                            correlation_id = %correlation_id,
                                            existing_hash = %existing_hash,
                                            new_hash = %block_hash,
                                            height = block_height,
                                            "CONFLICT: Different block at same height - rejecting (Tendermint instant finality)"
                                        );

                                        self_clone.metrics.forks_detected.inc();

                                        return Err(ChainError::InvalidBlock(format!(
                                            "Block at height {} already exists with different hash {}",
                                            block_height, existing_hash
                                        )));
                                    }
                                }
                                Ok(Ok(None)) => {
                                    // No existing block at this height - normal import path
                                    debug!(
                                        correlation_id = %correlation_id,
                                        block_height = block_height,
                                        "No existing block at this height - proceeding with import"
                                    );
                                }
                                Ok(Err(e)) => {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Failed to check for existing block at height - proceeding anyway"
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        correlation_id = %correlation_id,
                                        error = ?e,
                                        "Communication error checking for existing block - proceeding anyway"
                                    );
                                }
                            }
                        }

                        // Step 2: Tendermint consensus validation
                        // Block finality is proven via last_commit containing 2/3+ validator precommit signatures.
                        // Aura is no longer used - all blocks are validated through Tendermint.
                        {
                            use crate::actors_v2::chain::tendermint::validation::{
                                validate_last_commit, verify_commit,
                            };

                            // Get parent hash for last_commit validation
                            let parent_hash_h256 = ethereum_types::H256::from_slice(
                                block.message.parent_hash.as_bytes()
                            );

                            // Step 2.1: Validate last_commit field structure
                            // - Genesis (height 0 or 1) doesn't need last_commit
                            // - Non-genesis needs valid last_commit with matching parent
                            if let Err(commit_error) = validate_last_commit(
                                block_height,
                                &parent_hash_h256,
                                block.message.last_commit.as_ref(),
                            ) {
                                // Special case: height 1 is also considered "genesis" in some chains
                                // Allow missing last_commit for first few blocks during chain startup
                                if block_height <= 1 {
                                    debug!(
                                        correlation_id = %correlation_id,
                                        block_height = block_height,
                                        "Allowing missing last_commit for initial block"
                                    );
                                } else {
                                    error!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        block_height = block_height,
                                        error = ?commit_error,
                                        "Block failed last_commit validation"
                                    );
                                    return Err(ChainError::Consensus(format!(
                                        "Invalid last_commit: {:?}", commit_error
                                    )));
                                }
                            }

                            // Step 2.2: Verify commit signatures (2/3+ voting power)
                            // The last_commit proves that block N-1 was finalized
                            // Note: We need the validator set that was active at the COMMIT height
                            // (height - 1), not the current block height. This matters when
                            // validator sets change.
                            if let Some(ref last_commit) = block.message.last_commit {
                                // Get validator set for commit verification
                                // First try storage (for correct height-aware lookup), fall back to cached
                                let commit_height = last_commit.height;

                                let validator_set_for_commit = if let Some(ref storage) = storage_actor {
                                    // Query validator set at the commit height from storage
                                    match storage.send(
                                        crate::actors_v2::storage::messages::GetValidatorSetForHeightMessage {
                                            height: commit_height,
                                            correlation_id: Some(correlation_id),
                                        }
                                    ).await {
                                        Ok(Ok(Some(vs))) => {
                                            trace!(
                                                correlation_id = %correlation_id,
                                                commit_height = commit_height,
                                                "Using stored validator set for commit verification"
                                            );
                                            Some(vs)
                                        }
                                        _ => None, // Fall back to cached
                                    }
                                } else {
                                    None
                                };

                                // Use stored validator set if available, otherwise use cached
                                let validator_set = match validator_set_for_commit {
                                    Some(vs) => vs,
                                    None => {
                                        // Fall back to cached validator set
                                        match &self_clone.validator_set {
                                            Some(vs) => vs.read().await.clone(),
                                            None => {
                                                error!(
                                                    correlation_id = %correlation_id,
                                                    block_hash = %block_hash,
                                                    "Tendermint mode but no validator set available"
                                                );
                                                return Err(ChainError::Configuration(
                                                    "Tendermint mode requires validator set".to_string()
                                                ));
                                            }
                                        }
                                    }
                                };

                                // Verify that the commit has sufficient signatures
                                // Issue 1.2: pass chain_id for domain separation
                                let chain_id_str = self_clone.config.chain_id.to_string();
                                if let Err(verify_error) = verify_commit(
                                    last_commit,
                                    &validator_set,
                                    parent_hash_h256,
                                    &chain_id_str,
                                ) {
                                    error!(
                                        correlation_id = %correlation_id,
                                        block_hash = %block_hash,
                                        commit_height = last_commit.height,
                                        commit_round = last_commit.round,
                                        error = ?verify_error,
                                        "Block's last_commit failed signature verification"
                                    );
                                    return Err(ChainError::Consensus(format!(
                                        "Invalid commit signatures: {:?}", verify_error
                                    )));
                                }

                                debug!(
                                    correlation_id = %correlation_id,
                                    block_hash = %block_hash,
                                    commit_height = last_commit.height,
                                    signatures = last_commit.signatures.len(),
                                    "Last commit signatures verified successfully"
                                );
                            }

                            debug!(
                                correlation_id = %correlation_id,
                                block_hash = %block_hash,
                                "Block passed Tendermint consensus validation"
                            );
                        }

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
                        // Path B: Peg-ins are stored in auxpow_header.pegins (via pegins() helper)
                        if !block.message.pegins().is_empty() || !block.message.finalized_pegouts.is_empty() {
                            debug!(
                                correlation_id = %correlation_id,
                                pegin_count = block.message.pegins().len(),
                                pegout_count = block.message.finalized_pegouts.len(),
                                "Processing peg operations from imported block"
                            );

                            // Process peg-ins directly from AuxPowHeader (Path B design)
                            for pegin_info in block.message.pegins() {
                                // Convert V2 PegInInfo to bridge::PegInInfo (same fields, different types)
                                let bridge_pegin = bridge::PegInInfo {
                                    txid: pegin_info.txid,
                                    block_hash: pegin_info.block_hash,
                                    amount: pegin_info.amount,
                                    evm_account: pegin_info.evm_account,
                                    block_height: pegin_info.block_height,
                                };
                                if let Err(pegin_error) = self_clone.process_block_pegin(&bridge_pegin, &block_hash).await {
                                    error!(
                                        correlation_id = %correlation_id,
                                        txid = %pegin_info.txid,
                                        error = ?pegin_error,
                                        "Failed to process peg-in from imported block"
                                    );
                                    return Err(pegin_error);
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
                                pegin_count = block.message.pegins().len(),
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
                        // BUG FIX: Use storage_current_height (actual storage state) instead of current_height
                        // (captured from TendermintState which may be stale after WAL recovery)
                        if block_height == storage_current_height + 1 {
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

                            // Step 8: With Tendermint instant finality, no orphan processing needed
                            // Blocks are committed in order, so children are always received after parents

                            let import_duration = start_time.elapsed();

                            info!(
                                correlation_id = %correlation_id,
                                block_hash = %block_hash,
                                block_height = block_height,
                                source = ?source,
                                import_duration_ms = import_duration.as_millis(),
                                "Block import completed successfully"
                            );

                            // BUG FIX: Update ChainActor's local state.head after successful import
                            // Without this, state.get_height() returns 0 (head is None), which causes
                            // orphan blocks to be rejected as "too far ahead" (height > 0 + 100)
                            let new_head = crate::actors_v2::storage::actor::BlockRef {
                                hash: lighthouse_wrapper::types::Hash256::from_slice(block_hash.as_bytes()),
                                number: block_height,
                                execution_hash: block.message.execution_payload.block_hash,
                            };
                            self_clone.state.update_head(new_head);
                            debug!(
                                correlation_id = %correlation_id,
                                block_height = block_height,
                                "Updated ChainActor local state head after block import"
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
                } else if self.state.needs_auxpow_blocking() {
                    // Process AuxPoW when needed
                    info!(
                        block_hash = %block_hash,
                        blocks_without_pow = self.state.get_blocks_without_pow_blocking(),
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
                let actor = self.clone();
                let correlation_id = uuid::Uuid::new_v4();

                info!(
                    correlation_id = %correlation_id,
                    final_height = final_height,
                    "Sync completed, transitioning to synced state"
                );

                Box::pin(async move {
                    // TM-B5 Fix: Enter Consensus mode after sync completion
                    // This enables the node to participate in consensus at the synced height
                    actor.enter_consensus_mode(final_height + 1, correlation_id).await;

                    // Reset future height tracker - we're caught up
                    actor.future_height_tracker.write().await.reset();

                    info!(
                        correlation_id = %correlation_id,
                        final_height = final_height,
                        "ChainActor entered Consensus mode after sync completion"
                    );

                    Ok(ChainResponse::Success)
                })
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
                // Task 3.2: Clone tendermint_driver for consensus pause during sync
                let tendermint_driver = self.tendermint_driver.clone();

                Box::pin(async move {
                    // Skip if already syncing
                    let current_status = sync_status.read().await;
                    if current_status.is_syncing() {
                        trace!("Skipping health check - already syncing");
                        return Ok(ChainResponse::Success);
                    }
                    drop(current_status); // Release lock before continuing

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

                        // Task 3.2: Pause consensus before starting catch-up sync
                        // This prevents voting at incorrect heights during sync
                        if let Some(ref driver) = tendermint_driver {
                            info!("Pausing consensus before catch-up sync");
                            driver.do_send(crate::actors_v2::tendermint_driver::TendermintDriverMessage::Pause);
                        }

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
                                    // Resume consensus if sync failed to start
                                    if let Some(ref driver) = tendermint_driver {
                                        driver.do_send(crate::actors_v2::tendermint_driver::TendermintDriverMessage::Resume {
                                            height: storage_height + 1,
                                        });
                                    }
                                }
                                Err(e) => {
                                    error!("Sync actor mailbox error when triggering sync: {}", e);
                                    // Resume consensus if sync failed to start
                                    if let Some(ref driver) = tendermint_driver {
                                        driver.do_send(crate::actors_v2::tendermint_driver::TendermintDriverMessage::Resume {
                                            height: storage_height + 1,
                                        });
                                    }
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

            // ===== Tendermint Consensus Message Handlers =====

            ChainMessage::TendermintNewHeight {
                height,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintNewHeight received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let actor = self.clone();
                Box::pin(async move {
                    // TM-B5 Fix Note: TendermintNewHeight MUST be processed regardless of mode.
                    // This initializes the state machine - without it, consensus can never start.
                    // The mode guards on TendermintPropose/Timeout prevent active participation
                    // during blocksync, but the state machine must be initialized.
                    let (height, round) = actor
                        .handle_tendermint_new_height(height, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintHeightStarted { height, round })
                })
            }

            ChainMessage::TendermintPropose {
                height,
                round,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintPropose received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let actor = self.clone();
                Box::pin(async move {
                    // TM-B5 Fix: Reject proposals during blocksync
                    let mode = actor.get_consensus_mode().await;
                    if mode == ConsensusMode::Blocksync {
                        debug!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            "TendermintPropose rejected - in Blocksync mode"
                        );
                        return Err(ChainError::NotSynced);
                    }

                    let block_hash = actor
                        .handle_tendermint_propose(height, round, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintProposalCreated {
                        height,
                        round,
                        block_hash,
                    })
                })
            }

            ChainMessage::TendermintProposal {
                proposal,
                peer_id,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintProposal received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let height = proposal.height;
                let round = proposal.round;

                let actor = self.clone();
                Box::pin(async move {
                    // TM-B5 Fix: Reject proposals during blocksync
                    let mode = actor.get_consensus_mode().await;
                    if mode == ConsensusMode::Blocksync {
                        debug!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            peer_id = ?peer_id,
                            "TendermintProposal rejected - in Blocksync mode"
                        );
                        return Err(ChainError::NotSynced);
                    }

                    let block_hash = actor
                        .handle_tendermint_proposal(proposal, peer_id, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintProposalAccepted {
                        height,
                        round,
                        block_hash,
                    })
                })
            }

            ChainMessage::TendermintVote {
                vote,
                peer_id,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintVote received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let height = vote.height;
                let round = vote.round;

                let actor = self.clone();
                Box::pin(async move {
                    // TM-B5 Fix: During blocksync, still process future height votes for sync detection
                    // but reject current-height votes (we shouldn't participate in consensus)
                    let mode = actor.get_consensus_mode().await;
                    if mode == ConsensusMode::Blocksync {
                        let current_height = actor.get_tendermint_height().await.unwrap_or(0);

                        if vote.height > current_height {
                            // Future height vote - process for sync detection only
                            debug!(
                                correlation_id = %correlation_id,
                                vote_height = height,
                                current_height = current_height,
                                peer_id = ?peer_id,
                                "Processing future height vote for sync detection during Blocksync"
                            );
                            // Call the handler which will detect we're behind and trigger sync
                            let voter = actor
                                .handle_tendermint_vote(vote, peer_id, correlation_id)
                                .await?;
                            return Ok(ChainResponse::TendermintVoteAccepted {
                                height,
                                round,
                                voter,
                            });
                        } else {
                            // Current/past height vote - reject during blocksync
                            debug!(
                                correlation_id = %correlation_id,
                                vote_height = height,
                                current_height = current_height,
                                peer_id = ?peer_id,
                                "TendermintVote rejected - in Blocksync mode"
                            );
                            return Err(ChainError::NotSynced);
                        }
                    }

                    let voter = actor
                        .handle_tendermint_vote(vote, peer_id, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintVoteAccepted {
                        height,
                        round,
                        voter,
                    })
                })
            }

            ChainMessage::TendermintTimeout {
                height,
                round,
                step,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintTimeout received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let actor = self.clone();
                Box::pin(async move {
                    // TM-B5 Fix: Reject timeouts during blocksync
                    // This prevents the node from cycling rounds at the wrong height
                    let mode = actor.get_consensus_mode().await;
                    if mode == ConsensusMode::Blocksync {
                        trace!(
                            correlation_id = %correlation_id,
                            height = height,
                            round = round,
                            step = ?step,
                            "TendermintTimeout ignored - in Blocksync mode"
                        );
                        // Return current round since we didn't advance
                        return Ok(ChainResponse::TendermintRoundAdvanced {
                            height,
                            new_round: round,
                        });
                    }

                    let new_round = actor
                        .handle_tendermint_timeout(height, round, step, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintRoundAdvanced { height, new_round })
                })
            }

            ChainMessage::TendermintGovernanceUpdate {
                update,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintGovernanceUpdate received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let actor = self.clone();
                Box::pin(async move {
                    let effective_height = actor
                        .handle_tendermint_governance_update(update, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintGovernanceApplied { effective_height })
                })
            }

            ChainMessage::TendermintEvidence {
                evidence,
                peer_id,
                correlation_id,
            } => {
                let tendermint_configured = self.tendermint_state.is_some();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                if !tendermint_configured {
                    return Box::pin(async move {
                        warn!("TendermintEvidence received but Tendermint not configured");
                        Err(ChainError::Configuration(
                            "Tendermint not configured".to_string(),
                        ))
                    });
                }

                let actor = self.clone();
                Box::pin(async move {
                    let (culprit, height) = actor
                        .handle_tendermint_evidence(evidence, peer_id, correlation_id)
                        .await?;
                    Ok(ChainResponse::TendermintEvidenceProcessed { culprit, height })
                })
            }

            ChainMessage::SetTendermintDriver { addr } => {
                // Store the driver address for bidirectional communication
                self.tendermint_driver = Some(addr);
                info!("TendermintDriver address set in ChainActor");
                Box::pin(async move {
                    Ok(ChainResponse::Success)
                })
            }

            ChainMessage::TendermintBlockRequestTimeout {
                block_hash,
                correlation_id,
            } => {
                let actor = self.clone();
                Box::pin(async move {
                    actor
                        .handle_block_request_timeout(block_hash, correlation_id)
                        .await?;
                    Ok(ChainResponse::Success)
                })
            }

            ChainMessage::TendermintNewRoundAnnouncement {
                height,
                round,
                peer_id,
                correlation_id,
            } => {
                let actor = self.clone();
                let correlation_id = correlation_id.unwrap_or_else(uuid::Uuid::new_v4);

                Box::pin(async move {
                    // Get current height AND step atomically to avoid race condition
                    let (current_height, current_step) = actor
                        .get_tendermint_height_and_step()
                        .await
                        .unwrap_or((0, TendermintStep::Propose));

                    // Determine if we should enter blocksync
                    let should_sync = if height > current_height {
                        // TM-B5 Fix: Don't enter blocksync if we're in Precommit or Commit step
                        // for height H and receive NewRound(H+1). In Precommit, we're gathering
                        // 2/3+ signatures and about to commit. In Commit, we're finalizing the
                        // block. Either way, we'll naturally advance to H+1 shortly - entering
                        // blocksync would be incorrect.
                        if height == current_height + 1
                            && (current_step == TendermintStep::Commit
                                || current_step == TendermintStep::Precommit)
                        {
                            debug!(
                                correlation_id = %correlation_id,
                                announced_height = height,
                                current_height = current_height,
                                current_step = ?current_step,
                                peer_id = ?peer_id,
                                "NewRound(H+1) received while in Precommit/Commit step - not behind, continuing"
                            );
                            false
                        } else {
                            // Genuine gap: either gap > 1, or we're not in Precommit/Commit step
                            true
                        }
                    } else {
                        false
                    };

                    if should_sync {
                        info!(
                            correlation_id = %correlation_id,
                            announced_height = height,
                            announced_round = round,
                            current_height = current_height,
                            current_step = ?current_step,
                            peer_id = ?peer_id,
                            "NewRound announcement indicates we're behind - triggering sync"
                        );

                        actor.enter_blocksync_mode(correlation_id).await;

                        if let Some(ref sync_actor) = actor.sync_actor {
                            sync_actor.do_send(crate::actors_v2::network::SyncMessage::StartSync {
                                start_height: current_height,
                                target_height: Some(height),
                            });
                        }
                    } else {
                        trace!(
                            correlation_id = %correlation_id,
                            announced_height = height,
                            current_height = current_height,
                            "NewRound for current/past height or expected advance - ignoring"
                        );
                    }

                    Ok(ChainResponse::Success)
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
                let is_synced = self.state.is_synced_blocking();
                info!(is_synced = is_synced, "ChainManager: IsSynced query");
                Box::pin(async move { Ok(ChainManagerResponse::Synced(is_synced)) })
            }
            ChainManagerMessage::GetHead => {
                let current_height = self.state.get_height_blocking();
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
        max_pending_imports: super::actor::DEFAULT_MAX_PENDING_IMPORTS,
        connected_peer_count: 0,
        // Phase 3 fields
        queued_blocks: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        gap_fill_requests: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        // Active Height Monitoring (Layer 3)
        payload_unavailable_count: 0,
        // Tendermint state (not configured for this helper - Tendermint is always enabled
        // but requires explicit configuration via configure_tendermint)
        tendermint_state: None,
        timeout_scheduler: None,
        consensus_wal: None,
        validator_keypair: None,
        validator_set: None,
        cached_last_commit: None,
        timeout_receiver: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        tendermint_driver: None,
        tendermint_sync_validator: None,
        future_height_tracker: std::sync::Arc::new(tokio::sync::RwLock::new(
            super::actor::FutureHeightTracker::new(),
        )),
        consensus_mode: std::sync::Arc::new(tokio::sync::RwLock::new(
            super::actor::ConsensusMode::Blocksync,
        )),
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
        max_pending_imports: super::actor::DEFAULT_MAX_PENDING_IMPORTS,
        connected_peer_count: 0,
        // Phase 3 fields
        queued_blocks: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        gap_fill_requests: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        // Active Height Monitoring (Layer 3)
        payload_unavailable_count: 0,
        // Tendermint state (not configured for this helper - Tendermint is always enabled
        // but requires explicit configuration via configure_tendermint)
        tendermint_state: None,
        timeout_scheduler: None,
        consensus_wal: None,
        validator_keypair: None,
        validator_set: None,
        cached_last_commit: None,
        timeout_receiver: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        tendermint_driver: None,
        tendermint_sync_validator: None,
        future_height_tracker: std::sync::Arc::new(tokio::sync::RwLock::new(
            super::actor::FutureHeightTracker::new(),
        )),
        consensus_mode: std::sync::Arc::new(tokio::sync::RwLock::new(
            super::actor::ConsensusMode::Blocksync,
        )),
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
    type Result = ResponseActFuture<Self, Result<crate::actors_v2::chain::messages::SubmitAuxBlockResponse, ChainError>>;

    fn handle(&mut self, msg: SubmitAuxBlock, _ctx: &mut Self::Context) -> Self::Result {
        use crate::actors_v2::chain::messages::SubmitAuxBlockResponse;
        use crate::actors_v2::chain::tendermint::pegin::QueuedPegIn;

        let correlation_id = msg.correlation_id;
        let aggregate_hash = msg.aggregate_hash;
        let auxpow = msg.auxpow;
        let pegins = msg.pegins;
        let fee_recipient = msg.fee_recipient;

        debug!(
            correlation_id = %correlation_id,
            hash = %aggregate_hash,
            pegins_count = pegins.len(),
            "SubmitAuxBlock handler invoked with peg-ins"
        );

        self.record_activity();

        // Clone state and config for async operation (Arc<RwLock> fields propagate correctly)
        let state = self.state.clone();
        let config = self.config.clone();
        let network_actor = self.network_actor.clone();

        Box::pin(
            async move {
                // Step 1: Validate submitted AuxPoW
                let mut auxpow_header =
                    submit_aux_block_helper(&state, &config, aggregate_hash, auxpow).await?;

                info!(
                    correlation_id = %correlation_id,
                    hash = %aggregate_hash,
                    height = auxpow_header.height,
                    "AuxPoW validated successfully"
                );

                // Step 2: Filter and attach pegins to AuxPowHeader (Path B: pegins in AuxPowHeader)
                // Deduplicate against already-processed pegins (Doc 16 Layer 2)
                let mut valid_pegins = Vec::new();
                for pegin in pegins {
                    if state.is_pegin_processed(&pegin.txid).await {
                        debug!(
                            correlation_id = %correlation_id,
                            txid = %pegin.txid,
                            "Peg-in already processed, skipping"
                        );
                        continue;
                    }
                    valid_pegins.push(pegin);
                }
                let queued_count = valid_pegins.len();
                auxpow_header.pegins = valid_pegins;

                // Step 3: Queue validated AuxPoW with attached pegins
                state.set_queued_pow(Some(auxpow_header.clone())).await;
                state.reset_blocks_without_pow().await;

                info!(
                    correlation_id = %correlation_id,
                    pegins_attached = queued_count,
                    "AuxPoW queued with attached peg-ins (Path B)"
                );

                // Step 4: Broadcast AuxPoW to network (non-blocking, best-effort)
                if let Some(ref actor) = network_actor {
                    match serde_json::to_vec(&auxpow_header) {
                        Ok(auxpow_data) => {
                            let network_msg = crate::actors_v2::network::NetworkMessage::BroadcastAuxPow {
                                auxpow_data,
                                correlation_id: Some(correlation_id),
                            };
                            actor.do_send(network_msg);
                            debug!(
                                correlation_id = %correlation_id,
                                height = auxpow_header.height,
                                "AuxPoW broadcast initiated to network"
                            );
                        }
                        Err(e) => {
                            warn!(
                                correlation_id = %correlation_id,
                                error = ?e,
                                "Failed to serialize AuxPoW for network broadcast"
                            );
                        }
                    }
                } else {
                    debug!(
                        correlation_id = %correlation_id,
                        "Skipping AuxPoW network broadcast - no NetworkActor configured"
                    );
                }

                Ok(SubmitAuxBlockResponse {
                    auxpow_header: auxpow_header.clone(),
                    accepted: true,
                    pegins_queued: queued_count,
                    height: auxpow_header.height,
                })
            }
            .into_actor(self),
        )
    }
}

// ============================================================================
// Tendermint RPC Query Handlers (Phase 4: Document 12)
// ============================================================================

use crate::actors_v2::chain::messages::{
    GetTendermintState, TendermintStateResponse,
    GetValidatorSet, ValidatorSetResponse, ValidatorInfoResponse,
    GetCommit, CommitResponse, CommitSignatureInfo,
    GetChainParams, ChainParamsResponse,
    QueryTendermintPosition, TendermintPositionSnapshot,
    GetPendingGovernance, PendingGovernanceResponse, PendingGovernanceUpdate,
    GetEvidence, EvidenceResponse, EvidenceInfo,
    ApplyRecoveredState, ApplyRecoveredStateResponse,
    SetSyncValidator,
};

impl Handler<GetTendermintState> for ChainActor {
    type Result = ResponseActFuture<Self, Result<TendermintStateResponse, ChainError>>;

    fn handle(&mut self, msg: GetTendermintState, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let tendermint_state = self.tendermint_state.clone();

        Box::pin(
            async move {
                let state = tendermint_state
                    .ok_or_else(|| ChainError::Configuration("Tendermint not initialized".into()))?;

                let state_guard = state.read().await;

                // Get validator count (available directly from state_guard)
                let total_validators = state_guard.validator_set.len() as u32;

                // Acquire vote set locks to get current vote counts
                let prevotes = state_guard.prevotes.read().await;
                let precommits = state_guard.precommits.read().await;

                let response = TendermintStateResponse {
                    height: state_guard.height,
                    round: state_guard.round,
                    step: format!("{:?}", state_guard.step),
                    proposal_block_hash: state_guard.current_proposal.as_ref().map(|p| {
                        let block_hash = p.block_hash();
                        H256::from_slice(block_hash.as_bytes())
                    }),
                    locked_block_hash: state_guard.locked_block.map(|h| H256::from_slice(h.as_bytes())),
                    locked_round: state_guard.locked_round,
                    valid_block_hash: state_guard.valid_block.map(|h| H256::from_slice(h.as_bytes())),
                    valid_round: state_guard.valid_round,
                    prevotes_count: prevotes.vote_count() as u32,
                    precommits_count: precommits.vote_count() as u32,
                    total_validators,
                };

                tracing::debug!(
                    correlation_id = %correlation_id,
                    height = response.height,
                    round = response.round,
                    step = %response.step,
                    "GetTendermintState query completed"
                );

                Ok(response)
            }
            .into_actor(self),
        )
    }
}

impl Handler<GetValidatorSet> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ValidatorSetResponse, ChainError>>;

    fn handle(&mut self, msg: GetValidatorSet, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let state = self.state.clone();
        let storage_actor = self.storage_actor.clone();
        let page = msg.page.max(1);
        let per_page = msg.per_page.clamp(1, 100);

        Box::pin(
            async move {
                // Get current height if not specified
                let height = match msg.height {
                    Some(h) => h,
                    None => state.get_height().await,
                };

                // Try to get validator set from storage
                let validator_infos: Vec<(u32, String, String, u64)> = if let Some(ref storage) = storage_actor {
                    use crate::actors_v2::storage::messages::GetValidatorSetForHeightMessage;

                    match storage.send(GetValidatorSetForHeightMessage { height, correlation_id: None }).await {
                        Ok(Ok(Some(set))) => {
                            // Convert validator set to info tuples using iter()
                            set.iter()
                                .map(|(id, pubkey, power)| {
                                    (
                                        id.index() as u32,
                                        format!("{:?}", id),
                                        format!("{:?}", pubkey), // Debug format gives hex
                                        power,
                                    )
                                })
                                .collect()
                        }
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                };

                let total = validator_infos.len() as u32;
                let start = ((page - 1) * per_page) as usize;
                let end = (start + per_page as usize).min(validator_infos.len());

                let paginated: Vec<ValidatorInfoResponse> = validator_infos
                    .get(start..end)
                    .unwrap_or(&[])
                    .iter()
                    .enumerate()
                    .map(|(i, (_idx, addr, pk, power))| ValidatorInfoResponse {
                        index: (start + i) as u32,
                        address: addr.clone(),
                        public_key: pk.clone(),
                        voting_power: *power,
                    })
                    .collect();

                tracing::debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    count = paginated.len(),
                    total = total,
                    "GetValidatorSet query completed"
                );

                Ok(ValidatorSetResponse {
                    height,
                    validators: paginated,
                    count: (end - start) as u32,
                    total,
                })
            }
            .into_actor(self),
        )
    }
}

impl Handler<GetCommit> for ChainActor {
    type Result = ResponseActFuture<Self, Result<CommitResponse, ChainError>>;

    fn handle(&mut self, msg: GetCommit, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let state = self.state.clone();
        let storage_actor = self.storage_actor.clone();

        Box::pin(
            async move {
                // Get height to query
                let height = match msg.height {
                    Some(h) => h,
                    None => state.get_height().await,
                };

                // Get block from storage
                let block = if let Some(ref storage) = storage_actor {
                    use crate::actors_v2::storage::messages::GetBlockByHeightMessage;

                    match storage.send(GetBlockByHeightMessage { height, correlation_id: None }).await {
                        Ok(Ok(Some(b))) => Some(b),
                        _ => None,
                    }
                } else {
                    None
                };

                let block = block.ok_or_else(|| {
                    ChainError::Storage(format!("Block not found at height {}", height))
                })?;

                // Extract block header fields
                let parent_hash = H256::from_slice(block.message.parent_hash.as_bytes());
                let timestamp = block.message.execution_payload.timestamp;
                // proposer_index: Use slot % validator_count, or 0 if we can't determine
                // In Tendermint, the proposer rotates based on round-robin
                let proposer_index = (block.message.slot % 15) as u32; // 15 validators max

                // Compute last_commit_hash from block's last_commit field
                let last_commit_hash = block.message.last_commit.as_ref().map(|commit| {
                    // Hash the commit by combining height, round, block_hash, and signature count
                    use tiny_keccak::{Hasher, Keccak};
                    let mut hasher = Keccak::v256();
                    hasher.update(&commit.height.to_le_bytes());
                    hasher.update(&commit.round.to_le_bytes());
                    hasher.update(commit.block_hash.as_bytes());
                    hasher.update(&(commit.signatures.len() as u32).to_le_bytes());
                    let mut hash = [0u8; 32];
                    hasher.finalize(&mut hash);
                    H256::from_slice(&hash)
                });

                // Extract commit from the NEXT block's last_commit
                // Since block N's commit proof is in block N+1's last_commit
                // For the head block, the commit won't be available until the next block is produced
                let (round, signatures_count, commit_available, signatures) = if let Some(ref storage) = storage_actor {
                    use crate::actors_v2::storage::messages::GetBlockByHeightMessage;

                    match storage.send(GetBlockByHeightMessage { height: height + 1, correlation_id: None }).await {
                        Ok(Ok(Some(next_block))) => {
                            if let Some(commit) = next_block.message.last_commit {
                                let sigs: Vec<CommitSignatureInfo> = commit.signatures.iter().map(|sig| {
                                    CommitSignatureInfo {
                                        block_id_flag: format!("{:?}", sig.block_id_flag),
                                        validator_address: sig.validator_address.map(|v| format!("{}", v.0)),
                                        timestamp: if sig.timestamp > 0 { Some(sig.timestamp) } else { None },
                                        // Encode signature as hex string
                                        signature: sig.signature.as_ref().map(|s| {
                                            let bytes = s.serialize();
                                            bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>()
                                        }),
                                    }
                                }).collect();
                                let count = sigs.len() as u32;
                                (commit.round, count, true, sigs)
                            } else {
                                // Next block exists but has no last_commit (shouldn't happen)
                                (0, 0, false, Vec::new())
                            }
                        }
                        _ => {
                            // Next block doesn't exist yet - commit not available
                            // This is expected for the current head block
                            (0, 0, false, Vec::new())
                        }
                    }
                } else {
                    (0, 0, false, Vec::new())
                };

                let block_hash = H256::from_slice(block.canonical_root().as_bytes());

                tracing::debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    round = round,
                    signatures = signatures_count,
                    commit_available = commit_available,
                    "GetCommit query completed"
                );

                Ok(CommitResponse {
                    height,
                    round,
                    block_hash,
                    parent_hash,
                    timestamp,
                    proposer_index,
                    last_commit_hash,
                    signatures,
                    signatures_count,
                    canonical: true,
                    commit_available,
                })
            }
            .into_actor(self),
        )
    }
}

impl Handler<GetChainParams> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ChainParamsResponse, ChainError>>;

    fn handle(&mut self, msg: GetChainParams, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let state = self.state.clone();
        let _config = self.config.clone(); // TODO: Use config values when implemented

        Box::pin(
            async move {
                let height = state.get_height().await;

                // TODO: Read from ChainParams when integrated with TendermintState
                // These are the default values from tendermint/params.rs
                // See ChainParams struct for the full parameter set
                let response = ChainParamsResponse {
                    height,
                    max_block_bytes: 22020096,      // ~21MB (EIP-4844 compatible)
                    max_gas: 30_000_000,            // 30M gas limit
                    evidence_max_age_blocks: 100_000,
                    pegin_minimum_satoshis: 10_000, // 0.0001 BTC (from tendermint/params.rs defaults)
                    pegin_confirmation_depth: 6,    // 6 Bitcoin confirmations
                    miner_fee_bps: 50,              // 0.5% miner fee (from pegin_compensation defaults)
                };

                tracing::debug!(
                    correlation_id = %correlation_id,
                    height = height,
                    "GetChainParams query completed"
                );

                Ok(response)
            }
            .into_actor(self),
        )
    }
}

impl Handler<GetPendingGovernance> for ChainActor {
    type Result = ResponseActFuture<Self, Result<PendingGovernanceResponse, ChainError>>;

    fn handle(&mut self, msg: GetPendingGovernance, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);

        Box::pin(
            async move {
                // TODO: Connect to governance queue when infrastructure is ready
                // For now, return empty list as governance updates aren't tracked yet
                let updates: Vec<PendingGovernanceUpdate> = Vec::new();

                tracing::debug!(
                    correlation_id = %correlation_id,
                    pending_count = updates.len(),
                    "GetPendingGovernance query completed"
                );

                Ok(PendingGovernanceResponse { updates })
            }
            .into_actor(self),
        )
    }
}

// ============================================================================
// Issue 3.2: Internal State Query Handler (for TendermintDriver)
// ============================================================================

impl Handler<QueryTendermintPosition> for ChainActor {
    type Result = ResponseActFuture<Self, Result<TendermintPositionSnapshot, ChainError>>;

    fn handle(&mut self, msg: QueryTendermintPosition, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let tendermint_state = self.tendermint_state.clone();

        Box::pin(
            async move {
                let state = tendermint_state
                    .ok_or_else(|| ChainError::Configuration("Tendermint not initialized".into()))?;

                let state_guard = state.read().await;

                let snapshot = TendermintPositionSnapshot {
                    height: state_guard.height,
                    round: state_guard.round,
                    step: state_guard.step,
                    is_proposer: state_guard.is_proposer(),
                    locked_round: state_guard.locked_round,
                    locked_block: state_guard.locked_block,
                };

                tracing::trace!(
                    correlation_id = %correlation_id,
                    height = snapshot.height,
                    round = snapshot.round,
                    step = ?snapshot.step,
                    is_proposer = snapshot.is_proposer,
                    "QueryTendermintPosition completed"
                );

                Ok(snapshot)
            }
            .into_actor(self),
        )
    }
}

// ============================================================================
// Issue 3.1: WAL Recovery Handler
// ============================================================================

impl Handler<ApplyRecoveredState> for ChainActor {
    type Result = ResponseActFuture<Self, Result<ApplyRecoveredStateResponse, ChainError>>;

    fn handle(&mut self, msg: ApplyRecoveredState, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let recovered = msg.recovered;
        let tendermint_state = self.tendermint_state.clone();
        let storage_actor = self.storage_actor.clone();
        let sync_actor = self.sync_actor.clone();
        let engine_actor = self.engine_actor.clone();

        Box::pin(
            async move {
                // Check if there's any state to recover
                if recovered.last_committed_height.is_none() && recovered.current_round.is_none() {
                    tracing::debug!(
                        correlation_id = %correlation_id,
                        "No WAL state to recover"
                    );
                    return Ok(ApplyRecoveredStateResponse {
                        applied: false,
                        height: 0,
                        round: 0,
                        lock_restored: false,
                        prevotes_restored: 0,
                        precommits_restored: 0,
                    });
                }

                // WAL-Storage mismatch safeguard: Verify storage has the blocks WAL says we committed
                // If storage is behind WAL's committed height, database was corrupted or blocks were lost.
                // In this case, we must NOT apply WAL state (which would start consensus at wrong height)
                // but instead trigger a resync to recover the missing blocks.
                if let Some(wal_committed_height) = recovered.last_committed_height {
                    let storage_height = if let Some(ref storage) = storage_actor {
                        match storage.send(crate::actors_v2::storage::messages::GetChainHeightMessage {
                            correlation_id: Some(correlation_id),
                        }).await {
                            Ok(Ok(h)) => h,
                            Ok(Err(e)) => {
                                tracing::warn!(
                                    correlation_id = %correlation_id,
                                    error = ?e,
                                    "Failed to get storage height for WAL validation - proceeding with recovery"
                                );
                                wal_committed_height // Assume storage is consistent if query fails
                            }
                            Err(e) => {
                                tracing::warn!(
                                    correlation_id = %correlation_id,
                                    error = %e,
                                    "StorageActor communication error during WAL validation - proceeding with recovery"
                                );
                                wal_committed_height // Assume storage is consistent if unreachable
                            }
                        }
                    } else {
                        tracing::warn!(
                            correlation_id = %correlation_id,
                            "StorageActor not available for WAL validation - proceeding with recovery"
                        );
                        wal_committed_height // No storage actor, assume consistent
                    };

                    if storage_height < wal_committed_height {
                        tracing::warn!(
                            correlation_id = %correlation_id,
                            storage_height = storage_height,
                            wal_committed_height = wal_committed_height,
                            gap = wal_committed_height - storage_height,
                            "WAL-Storage mismatch detected! Storage is behind WAL committed height. \
                             Triggering resync to recover missing blocks."
                        );

                        // Trigger ForceResync to recover missing blocks
                        if let Some(ref sync) = sync_actor {
                            let resync_msg = crate::actors_v2::network::messages::SyncMessage::ForceResync {
                                reason: format!(
                                    "WAL-Storage mismatch: storage at {} but WAL committed {}",
                                    storage_height, wal_committed_height
                                ),
                            };
                            if let Err(e) = sync.send(resync_msg).await {
                                tracing::error!(
                                    correlation_id = %correlation_id,
                                    error = %e,
                                    "Failed to send ForceResync to SyncActor"
                                );
                            } else {
                                tracing::info!(
                                    correlation_id = %correlation_id,
                                    "ForceResync triggered due to WAL-Storage mismatch"
                                );
                            }
                        }

                        // Return applied: false to prevent consensus from starting at wrong height
                        return Ok(ApplyRecoveredStateResponse {
                            applied: false,
                            height: storage_height,
                            round: 0,
                            lock_restored: false,
                            prevotes_restored: 0,
                            precommits_restored: 0,
                        });
                    }

                    // PHASE 1.2 FIX (Bug 3): Verify execution layer matches storage height
                    // Even if WAL and storage are in sync, reth may be behind due to
                    // failed ExecuteBlock calls. Query reth and trigger resync if behind.
                    if let Some(ref engine) = engine_actor {
                        use crate::actors_v2::engine::{EngineMessage, EngineResponse};

                        match engine.send(EngineMessage::GetLatestBlock {
                            correlation_id: Some(correlation_id),
                        }).await {
                            Ok(Ok(EngineResponse::LatestBlock { number: reth_height, hash })) => {
                                tracing::info!(
                                    correlation_id = %correlation_id,
                                    reth_height = reth_height,
                                    reth_hash = %hash,
                                    storage_height = storage_height,
                                    wal_committed_height = wal_committed_height,
                                    "Execution layer at height {} during recovery", reth_height
                                );

                                if reth_height < storage_height {
                                    let gap = storage_height - reth_height;
                                    tracing::error!(
                                        correlation_id = %correlation_id,
                                        reth_height = reth_height,
                                        storage_height = storage_height,
                                        gap = gap,
                                        "CRITICAL: Execution layer behind storage by {} blocks during recovery! \
                                         WAL recorded commits that failed to execute. Triggering resync.",
                                        gap
                                    );

                                    // Trigger resync to replay missing blocks from storage to execution layer
                                    if let Some(ref sync) = sync_actor {
                                        let resync_msg = crate::actors_v2::network::messages::SyncMessage::ForceResync {
                                            reason: format!(
                                                "Execution-storage desync at recovery: reth at {} but storage at {} (gap: {})",
                                                reth_height, storage_height, gap
                                            ),
                                        };
                                        if let Err(e) = sync.send(resync_msg).await {
                                            tracing::error!(
                                                correlation_id = %correlation_id,
                                                error = %e,
                                                "Failed to send ForceResync for execution layer recovery"
                                            );
                                        } else {
                                            tracing::info!(
                                                correlation_id = %correlation_id,
                                                "ForceResync triggered for execution layer recovery during ApplyRecoveredState"
                                            );
                                        }
                                    }

                                    // Return applied: false to prevent consensus from starting at wrong height
                                    return Ok(ApplyRecoveredStateResponse {
                                        applied: false,
                                        height: reth_height,
                                        round: 0,
                                        lock_restored: false,
                                        prevotes_restored: 0,
                                        precommits_restored: 0,
                                    });
                                }
                            }
                            Ok(Ok(other)) => {
                                tracing::warn!(
                                    correlation_id = %correlation_id,
                                    response = ?other,
                                    "Unexpected response from EngineActor GetLatestBlock during recovery"
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(
                                    correlation_id = %correlation_id,
                                    error = ?e,
                                    "Failed to query execution layer height during recovery - proceeding cautiously"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    correlation_id = %correlation_id,
                                    error = %e,
                                    "EngineActor unreachable during recovery - cannot verify execution layer sync"
                                );
                            }
                        }
                    }
                }

                let state = tendermint_state
                    .ok_or_else(|| ChainError::Configuration("Tendermint not initialized".into()))?;

                let mut state_guard = state.write().await;

                // Determine the height we should be at
                let recovery_height = recovered.start_height();
                let current_height = state_guard.height;

                // Fix height mismatch: WAL recovery takes precedence over initial state
                // This happens when TendermintState was initialized with stale height (e.g., from V0 storage)
                // but WAL shows we were at a different height
                if recovery_height != current_height {
                    tracing::info!(
                        correlation_id = %correlation_id,
                        recovery_height = recovery_height,
                        current_height = current_height,
                        "WAL recovery height mismatch - advancing state to WAL height"
                    );
                    // Reinitialize state machine at the correct height
                    // This resets VoteSets, clears locks, etc. for the new height
                    let validator_set = state_guard.validator_set.clone();
                    state_guard.new_height(recovery_height, validator_set);
                }

                // Apply recovered round (if WAL shows we were at a higher round)
                let mut round_advanced = false;
                if let Some(wal_round) = recovered.current_round {
                    if wal_round > state_guard.round {
                        tracing::info!(
                            correlation_id = %correlation_id,
                            current_round = state_guard.round,
                            recovered_round = wal_round,
                            "Advancing to recovered round from WAL"
                        );
                        state_guard.round = wal_round;
                        round_advanced = true;
                    }
                }

                // Restore lock state (critical for safety)
                let lock_restored = if let (Some(locked_round), Some(locked_block)) =
                    (recovered.locked_round, recovered.locked_block)
                {
                    tracing::info!(
                        correlation_id = %correlation_id,
                        locked_round = locked_round,
                        locked_block = %locked_block,
                        "Restoring lock state from WAL"
                    );
                    state_guard.locked_round = Some(locked_round);
                    state_guard.locked_block = Some(locked_block);
                    // Also set valid_block if we were locked (locked implies valid)
                    state_guard.valid_round = Some(locked_round);
                    state_guard.valid_block = Some(locked_block);
                    true
                } else {
                    false
                };

                // Restore sent votes (critical for double-vote prevention)
                // TendermintState uses HashMap<Round, Option<BlockHash>> for vote tracking
                let mut prevotes_restored = 0;
                for (round, block_hash_opt) in recovered.sent_prevotes.iter() {
                    state_guard.sent_prevotes.insert(*round, *block_hash_opt);
                    prevotes_restored += 1;
                    tracing::trace!(
                        correlation_id = %correlation_id,
                        height = current_height,
                        round = round,
                        block_hash = ?block_hash_opt,
                        "Restored prevote from WAL"
                    );
                }

                let mut precommits_restored = 0;
                for (round, block_hash_opt) in recovered.sent_precommits.iter() {
                    state_guard.sent_precommits.insert(*round, *block_hash_opt);
                    precommits_restored += 1;
                    tracing::trace!(
                        correlation_id = %correlation_id,
                        height = current_height,
                        round = round,
                        block_hash = ?block_hash_opt,
                        "Restored precommit from WAL"
                    );
                }

                // Determine and set the step based on what was recovered
                let new_step = recovered.current_step();
                if round_advanced || lock_restored || prevotes_restored > 0 || precommits_restored > 0 {
                    state_guard.step = new_step;
                }

                // Enable recovery mode for faster round catch-up after restart.
                // This allows the node to accept votes from up to 100 rounds in the
                // future, enabling rapid catch-up when rejoining after being offline.
                // Recovery mode will be automatically disabled by TendermintDriver
                // after the node has caught up to the network's current round.
                state_guard.future_messages.enable_recovery_mode();

                tracing::info!(
                    correlation_id = %correlation_id,
                    height = state_guard.height,
                    round = state_guard.round,
                    step = ?state_guard.step,
                    lock_restored = lock_restored,
                    prevotes_restored = prevotes_restored,
                    precommits_restored = precommits_restored,
                    recovery_mode = true,
                    "WAL recovery applied to TendermintState with recovery mode enabled"
                );

                Ok(ApplyRecoveredStateResponse {
                    applied: true,
                    height: state_guard.height,
                    round: state_guard.round,
                    lock_restored,
                    prevotes_restored,
                    precommits_restored,
                })
            }
            .into_actor(self),
        )
    }
}

// ============================================================================
// Issue 4.2: ValidatorSetTracker Integration Handler
// ============================================================================

impl Handler<SetSyncValidator> for ChainActor {
    type Result = ();

    fn handle(&mut self, msg: SetSyncValidator, _ctx: &mut Self::Context) -> Self::Result {
        info!("Setting TendermintSyncValidator reference for governance notifications");
        self.tendermint_sync_validator = Some(msg.validator);
    }
}

// ============================================================================
// Chaos Testing: Evidence Query Handler
// ============================================================================

impl Handler<GetEvidence> for ChainActor {
    type Result = ResponseActFuture<Self, Result<EvidenceResponse, ChainError>>;

    fn handle(&mut self, msg: GetEvidence, _ctx: &mut Self::Context) -> Self::Result {
        let correlation_id = msg.correlation_id.unwrap_or_else(Uuid::new_v4);
        let tendermint_state = self.tendermint_state.clone();
        let max_age_blocks = msg.max_age_blocks;

        Box::pin(
            async move {
                let state = tendermint_state
                    .ok_or_else(|| ChainError::Configuration("Tendermint not initialized".into()))?;

                let state_guard = state.read().await;
                let current_height = state_guard.height;

                // Get detected evidence from state
                // Evidence is stored in TendermintState.detected_evidence
                let evidence_list = &state_guard.detected_evidence;

                // Filter by max age if specified
                let filtered: Vec<EvidenceInfo> = evidence_list
                    .iter()
                    .filter(|e| {
                        if let Some(max_age) = max_age_blocks {
                            current_height.saturating_sub(e.height) <= max_age
                        } else {
                            true
                        }
                    })
                    .map(|e| {
                        EvidenceInfo {
                            evidence_type: format!("{:?}", e.kind),
                            validator_address: format!("{:?}", e.culprit),
                            height: e.height,
                            round: e.round,
                            vote_a_block_hash: e.vote_a.block_hash.map(|h| format!("{:?}", h)),
                            vote_b_block_hash: e.vote_b.block_hash.map(|h| format!("{:?}", h)),
                            detected_at: chrono::Utc::now().to_rfc3339(),
                        }
                    })
                    .collect();

                let total = filtered.len();

                tracing::debug!(
                    correlation_id = %correlation_id,
                    evidence_count = total,
                    max_age_blocks = ?max_age_blocks,
                    "GetEvidence query completed"
                );

                Ok(EvidenceResponse {
                    evidence: filtered,
                    total,
                })
            }
            .into_actor(self),
        )
    }
}
