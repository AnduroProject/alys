//! ChainActor V2 Implementation
//!
//! Simplified blockchain actor that replaces both V1 ChainActor complexity and monolithic chain.rs.
//! Follows standard Actix patterns like StorageActor/NetworkActor V2.

use actix::prelude::*;
use bitcoin::hashes::Hash;
use ethereum_types::H256;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::{
    messages::{BlockSource, ChainMessage},
    state::SyncStatus,
    ChainConfig, ChainError, ChainMetrics, ChainState,
};

use crate::actors_v2::{
    engine::EngineActor,
    network::{NetworkActor, SyncActor},
    storage::StorageActor,
};
use crate::block::SignedConsensusBlock;
use lighthouse_wrapper::types::MainnetEthSpec;

/// Pending import request queued when import lock is held (Phase 2)
#[derive(Debug, Clone)]
pub struct PendingImport {
    pub block: SignedConsensusBlock<MainnetEthSpec>,
    pub source: BlockSource,
    pub queued_at: Instant,
}

/// Simplified ChainActor - core blockchain functionality (Clone-enabled for async handlers)
#[derive(Clone)]
pub struct ChainActor {
    /// Configuration
    pub(crate) config: ChainConfig,

    /// Core blockchain state (derived from chain.rs)
    pub(crate) state: ChainState,

    /// Actor integration
    pub(crate) storage_actor: Option<Addr<StorageActor>>,
    pub(crate) network_actor: Option<Addr<NetworkActor>>,
    pub(crate) sync_actor: Option<Addr<SyncActor>>,
    pub(crate) engine_actor: Option<Addr<EngineActor>>,

    /// Simple metrics
    pub(crate) metrics: ChainMetrics,

    /// Last activity timestamp
    pub(crate) last_activity: Instant,

    /// Phase 2: Import lock to serialize block imports and prevent race conditions
    pub(crate) import_in_progress: Arc<AtomicBool>,

    /// Phase 2: Queue for pending import requests when lock is held
    pub(crate) pending_imports: Arc<RwLock<VecDeque<PendingImport>>>,

    /// Phase 2: Maximum pending import queue size
    pub(crate) max_pending_imports: usize,

    /// Phase 2: Connected peer count for sync triggering on first peer
    pub(crate) connected_peer_count: usize,
}

impl ChainActor {
    /// Create new ChainActor
    pub fn new(config: ChainConfig, state: ChainState) -> Self {
        let mut metrics = ChainMetrics::new();

        // Initialize metrics based on current state
        metrics.set_sync_status(state.is_synced());
        metrics.set_chain_height(state.get_height());

        Self {
            config,
            state,
            storage_actor: None,
            network_actor: None,
            sync_actor: None,
            engine_actor: None,
            metrics,
            last_activity: Instant::now(),
            // Phase 2: Initialize import serialization
            import_in_progress: Arc::new(AtomicBool::new(false)),
            pending_imports: Arc::new(RwLock::new(VecDeque::new())),
            max_pending_imports: 10, // Configurable limit
            connected_peer_count: 0, // Phase 2: Start with no peers
        }
    }

    /// Set storage actor address
    pub fn set_storage_actor(&mut self, addr: Addr<StorageActor>) {
        self.storage_actor = Some(addr);
    }

    /// Set network actor addresses
    pub fn set_network_actors(
        &mut self,
        network_addr: Addr<NetworkActor>,
        sync_addr: Addr<SyncActor>,
    ) {
        self.network_actor = Some(network_addr);
        self.sync_actor = Some(sync_addr);
    }

    /// Set engine actor address
    pub fn set_engine_actor(&mut self, addr: Addr<EngineActor>) {
        self.engine_actor = Some(addr);
    }

    /// Record activity and update metrics
    pub(crate) fn record_activity(&mut self) {
        self.last_activity = Instant::now();
        self.metrics.record_activity();
        self.metrics.set_chain_height(self.state.get_height());
        self.metrics.set_sync_status(self.state.is_synced());
    }

    /// Check if network is ready for consensus decisions
    pub(crate) async fn is_network_ready(&self) -> bool {
        if let Some(ref network_actor) = self.network_actor {
            if let Ok(response) = network_actor
                .send(crate::actors_v2::network::NetworkMessage::GetNetworkStatus)
                .await
            {
                if let Ok(crate::actors_v2::network::NetworkResponse::Status(status)) = response {
                    return status.is_running && status.connected_peers > 0;
                }
            }
        }
        false
    }

    /// Broadcast block to network
    pub(crate) async fn broadcast_block(&self, block_data: Vec<u8>) -> Result<(), ChainError> {
        if let Some(ref network_actor) = self.network_actor {
            let msg = crate::actors_v2::network::NetworkMessage::BroadcastBlock {
                block_data,
                priority: true,
            };
            network_actor
                .send(msg)
                .await
                .map_err(|e| ChainError::NetworkError(e.to_string()))?
                .map_err(ChainError::Network)?;
        }
        Ok(())
    }

    /// Request missing blocks for sync
    pub(crate) async fn request_blocks(
        &self,
        start_height: u64,
        count: u32,
    ) -> Result<(), ChainError> {
        if let Some(ref sync_actor) = self.sync_actor {
            let msg = crate::actors_v2::network::SyncMessage::RequestBlocks {
                start_height,
                count,
                peer_id: None,
            };
            sync_actor
                .send(msg)
                .await
                .map_err(|e| ChainError::NetworkError(e.to_string()))?
                .map_err(ChainError::Sync)?;
        }
        Ok(())
    }

    /// Store block via StorageActor
    pub(crate) async fn store_block(
        &self,
        block: crate::block::SignedConsensusBlock<lighthouse_wrapper::types::MainnetEthSpec>,
        canonical: bool,
    ) -> Result<(), ChainError> {
        if let Some(ref storage_actor) = self.storage_actor {
            // Store the complete signed block (AlysConsensusBlock now expects SignedConsensusBlock)
            let store_msg = crate::actors_v2::storage::messages::StoreBlockMessage {
                block,
                canonical,
                correlation_id: Some(Uuid::new_v4()), // Generate correlation ID for tracing
            };

            storage_actor
                .send(store_msg)
                .await
                .map_err(|e| {
                    ChainError::NetworkError(format!("Failed to send store message: {}", e))
                })?
                .map_err(|e| ChainError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    /// Process peg-in from imported block (Phase 3 - Task 3.1.2) - Real implementation
    pub async fn process_block_pegin(
        &self,
        pegin: &bridge::PegInInfo,
        block_hash: &H256,
    ) -> Result<(), ChainError> {
        debug!(
            txid = %pegin.txid,
            amount = pegin.amount,
            evm_account = ?pegin.evm_account,
            block_hash = %block_hash,
            "Processing peg-in from imported block"
        );

        // Peg-in processing based on V0 patterns (chain.rs:1706-1717):
        // 1. Validate peg-in amount and address
        if pegin.amount == 0 {
            error!(
                txid = %pegin.txid,
                "Peg-in has zero amount - invalid"
            );
            return Err(ChainError::Bridge("Peg-in has zero amount".to_string()));
        }

        if pegin.evm_account == lighthouse_wrapper::types::Address::zero() {
            error!(
                txid = %pegin.txid,
                "Peg-in has zero EVM account - invalid"
            );
            return Err(ChainError::Bridge(
                "Peg-in has zero EVM account".to_string(),
            ));
        }

        // 2. REAL IMPLEMENTATION: Remove from queued pegins (matches V0 line 1708)
        let removed_pegin = self.state.queued_pegins.write().await.remove(&pegin.txid);
        if removed_pegin.is_none() {
            warn!(
                txid = %pegin.txid,
                "Peg-in not found in queued pegins - may have been processed already"
            );
        }

        // 3. REAL IMPLEMENTATION: Fetch Bitcoin transaction using Bridge interface
        let bitcoin_tx = {
            let bridge = self.state.bridge.read().await;
            // Convert H256 to BlockHash for bridge interface
            let mut block_hash_bytes = [0u8; 32];
            block_hash_bytes.copy_from_slice(block_hash.as_bytes());
            let block_hash_bitcoin = bitcoin::BlockHash::from_byte_array(block_hash_bytes);

            match bridge.fetch_transaction(&pegin.txid, &block_hash_bitcoin) {
                Some(tx) => {
                    debug!(
                        txid = %pegin.txid,
                        block_hash = %block_hash,
                        "Successfully fetched Bitcoin transaction for peg-in"
                    );
                    tx
                }
                None => {
                    error!(
                        txid = %pegin.txid,
                        "Bitcoin transaction not found in block"
                    );
                    return Err(ChainError::Bridge(
                        "Bitcoin transaction not found".to_string(),
                    ));
                }
            }
        };

        // 4. REAL IMPLEMENTATION: Register with Bitcoin wallet (matches V0 line 1712-1716)
        {
            let mut wallet = self.state.bitcoin_wallet.write().await;
            if let Err(wallet_error) = wallet.register_pegin(&bitcoin_tx) {
                error!(
                    txid = %pegin.txid,
                    error = ?wallet_error,
                    "Failed to register peg-in with Bitcoin wallet"
                );
                return Err(ChainError::Bridge(format!(
                    "Wallet registration failed: {:?}",
                    wallet_error
                )));
            }
        }

        info!(
            txid = %pegin.txid,
            amount = pegin.amount,
            evm_account = ?pegin.evm_account,
            block_hash = %block_hash,
            "Successfully processed peg-in from imported block with real state changes"
        );

        Ok(())
    }

    /// Process finalized peg-out from imported block (Phase 3 - Task 3.1.2) - Real implementation
    pub async fn process_finalized_pegout(
        &self,
        pegout: &bitcoin::Transaction,
        block_hash: &H256,
    ) -> Result<(), ChainError> {
        debug!(
            pegout_txid = %pegout.txid(),
            block_hash = %block_hash,
            "Processing finalized peg-out from imported block"
        );

        // REAL peg-out processing based on V0 patterns (chain.rs:1734-1748):

        // 1. Validate transaction structure
        if pegout.input.is_empty() {
            error!(
                pegout_txid = %pegout.txid(),
                "Peg-out has no inputs - invalid transaction"
            );
            return Err(ChainError::Bridge("Peg-out has no inputs".to_string()));
        }

        if pegout.output.is_empty() {
            error!(
                pegout_txid = %pegout.txid(),
                "Peg-out has no outputs - invalid transaction"
            );
            return Err(ChainError::Bridge("Peg-out has no outputs".to_string()));
        }

        // 2. Calculate total peg-out amount
        let total_output_value: u64 = pegout.output.iter().map(|output| output.value).sum();
        if total_output_value == 0 {
            error!(
                pegout_txid = %pegout.txid(),
                "Peg-out has zero output value - invalid"
            );
            return Err(ChainError::Bridge(
                "Peg-out has zero output value".to_string(),
            ));
        }

        let txid = pegout.txid();

        // 3. REAL IMPLEMENTATION: Broadcast to Bitcoin network using Bridge interface
        {
            let bridge = self.state.bridge.read().await;
            match bridge.broadcast_signed_tx(pegout) {
                Ok(broadcast_txid) => {
                    info!(
                        pegout_txid = %txid,
                        broadcast_txid = %broadcast_txid,
                        "Successfully broadcasted peg-out to Bitcoin network"
                    );
                }
                Err(e) => {
                    warn!(
                        pegout_txid = %txid,
                        error = ?e,
                        "Failed to broadcast peg-out to Bitcoin network"
                    );
                    // V0 continues on broadcast failure (non-fatal) - matches V0 behavior
                }
            }
        }

        // 4. REAL IMPLEMENTATION: Cleanup signature tracking (matches V0 line 1744-1747)
        {
            let mut signature_collector = self.state.bitcoin_signature_collector.write().await;
            signature_collector.cleanup_signatures_for(&txid);
            debug!(
                pegout_txid = %txid,
                "Cleaned up signature tracking for finalized peg-out"
            );
        }

        info!(
            pegout_txid = %txid,
            total_value = total_output_value,
            input_count = pegout.input.len(),
            output_count = pegout.output.len(),
            block_hash = %block_hash,
            "Successfully processed and finalized peg-out from imported block with real state changes"
        );

        Ok(())
    }

    /// Phase 2: Force release import lock (for error recovery)
    pub fn force_release_import_lock(&self) {
        if self.import_in_progress.swap(false, Ordering::SeqCst) {
            warn!("Forced import lock release (error recovery)");
        }
    }

    /// Phase 2: Process next queued import after lock release
    pub async fn process_next_queued_import(&self, ctx_addr: Addr<ChainActor>) {
        // Check for queued imports
        let next_import = {
            let mut queue = self.pending_imports.write().await;
            queue.pop_front()
        };

        if let Some(pending) = next_import {
            let wait_time = pending.queued_at.elapsed();

            info!(
                queue_wait_ms = wait_time.as_millis(),
                block_height = pending.block.message.execution_payload.block_number,
                "Processing next queued block import"
            );

            // Send queued import to ChainActor
            ctx_addr.do_send(super::messages::ChainMessage::ImportBlock {
                block: pending.block,
                source: pending.source,
                peer_id: None, // Queued blocks don't have peer_id context
            });
        } else {
            debug!("Import queue empty after lock release");
        }
    }

    /// Update chain head after successful block import (Phase 3 - Task 3.1.2)
    pub async fn update_chain_head(
        &self,
        new_head: crate::actors_v2::storage::actor::BlockRef,
    ) -> Result<(), ChainError> {
        info!(
            new_head_hash = %new_head.hash,
            new_head_height = new_head.number,
            "Updating chain head after block import"
        );

        if let Some(ref storage_actor) = self.storage_actor {
            let msg = crate::actors_v2::storage::messages::UpdateChainHeadMessage {
                new_head: new_head.clone(),
                correlation_id: Some(uuid::Uuid::new_v4()),
            };

            match storage_actor.send(msg).await {
                Ok(storage_result) => match storage_result {
                    Ok(()) => {
                        info!(
                            head_hash = %new_head.hash,
                            head_height = new_head.number,
                            "Chain head updated successfully"
                        );
                        Ok(())
                    }
                    Err(e) => {
                        error!(
                            head_hash = %new_head.hash,
                            error = ?e,
                            "Failed to update chain head"
                        );
                        Err(ChainError::Storage(e.to_string()))
                    }
                },
                Err(e) => {
                    error!(
                        head_hash = %new_head.hash,
                        error = ?e,
                        "Communication error updating chain head"
                    );
                    Err(ChainError::NetworkError(format!(
                        "Storage communication failed: {}",
                        e
                    )))
                }
            }
        } else {
            Err(ChainError::Storage(
                "StorageActor not available".to_string(),
            ))
        }
    }

    /// Phase 4C: Reorganize chain to new canonical tip when fork choice determines it's better
    pub async fn reorganize_chain(
        &self,
        new_tip_block: &SignedConsensusBlock<MainnetEthSpec>,
        correlation_id: Uuid,
    ) -> Result<super::reorganization::ReorganizationResult, ChainError> {
        warn!(
            correlation_id = %correlation_id,
            new_tip_height = new_tip_block.message.execution_payload.block_number,
            "Starting chain reorganization"
        );

        if let Some(ref storage_actor) = self.storage_actor {
            let current_height = self.state.get_height();

            // Call the reorganization module
            let result = super::reorganization::reorganize_to_new_tip(
                new_tip_block,
                current_height,
                storage_actor,
                correlation_id,
            )
            .await?;

            info!(
                correlation_id = %correlation_id,
                reorg_height = result.reorg_height,
                blocks_rolled_back = result.blocks_rolled_back,
                blocks_applied = result.blocks_applied,
                new_tip = %result.new_tip,
                "Chain reorganization completed successfully"
            );

            Ok(result)
        } else {
            Err(ChainError::Storage(
                "StorageActor not available for reorganization".to_string(),
            ))
        }
    }

    /// Initialize sync state on startup
    pub async fn initialize_sync_state(&mut self) -> Result<(), ChainError> {
        info!("Initializing chain sync state");

        // Step 1: Get current storage height
        let storage_height = self.get_storage_height().await?;
        info!(storage_height = storage_height, "Current chain height");

        // Step 2: Query network for target height
        let network_height = match self.query_network_height().await {
            Ok(height) => height,
            Err(e) => {
                warn!(
                    "Could not determine network height: {}. Assuming synced.",
                    e
                );
                // Cannot determine network height - assume synced to avoid blocking startup
                return Ok(());
            }
        };

        info!(
            storage_height = storage_height,
            network_height = network_height,
            gap = network_height.saturating_sub(storage_height),
            "Network height determined"
        );

        // Step 3: Determine if sync is needed
        const SYNC_THRESHOLD: u64 = 10; // Trigger sync if >10 blocks behind

        if network_height > storage_height + SYNC_THRESHOLD {
            info!(
                "Node is {} blocks behind, triggering sync",
                network_height - storage_height
            );

            // Trigger sync via SyncActor
            self.trigger_sync().await?;
        } else {
            info!(
                "Node is synced (within {} blocks of network)",
                SYNC_THRESHOLD
            );
        }

        Ok(())
    }

    /// Query network peers for consensus chain height
    async fn query_network_height(&self) -> Result<u64, ChainError> {
        if let Some(ref sync_actor) = self.sync_actor {
            let msg = crate::actors_v2::network::SyncMessage::QueryNetworkHeight;

            match sync_actor.send(msg).await {
                Ok(Ok(response)) => {
                    use crate::actors_v2::network::SyncResponse;
                    if let SyncResponse::NetworkHeight { height } = response {
                        Ok(height)
                    } else {
                        Err(ChainError::UnexpectedResponse)
                    }
                }
                Ok(Err(e)) => Err(ChainError::Sync(e)),
                Err(e) => Err(ChainError::ActorMailbox(e.to_string())),
            }
        } else {
            Err(ChainError::SyncActorNotSet)
        }
    }

    /// Get current height from storage
    async fn get_storage_height(&self) -> Result<u64, ChainError> {
        if let Some(ref storage_actor) = self.storage_actor {
            let msg = crate::actors_v2::storage::messages::GetChainHeightMessage {
                correlation_id: Some(Uuid::new_v4()),
            };

            match storage_actor.send(msg).await {
                Ok(Ok(height)) => Ok(height),
                Ok(Err(e)) => Err(ChainError::Storage(format!("{:?}", e))),
                Err(e) => Err(ChainError::ActorMailbox(e.to_string())),
            }
        } else {
            // If no storage, assume genesis (height 0)
            Ok(0)
        }
    }

    /// Trigger sync via SyncActor
    async fn trigger_sync(&self) -> Result<(), ChainError> {
        if let Some(ref sync_actor) = self.sync_actor {
            info!("Triggering SyncActor to start sync");

            let msg = crate::actors_v2::network::SyncMessage::StartSync;

            match sync_actor.send(msg).await {
                Ok(Ok(response)) => {
                    use crate::actors_v2::network::SyncResponse;
                    match response {
                        SyncResponse::Started => {
                            info!("✓ Sync started successfully");
                            Ok(())
                        }
                        _ => {
                            info!("Sync already in progress or completed");
                            Ok(())
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("Failed to start sync: {:?}", e);
                    Err(ChainError::Sync(e))
                }
                Err(e) => {
                    error!("SyncActor mailbox error: {}", e);
                    Err(ChainError::ActorMailbox(e.to_string()))
                }
            }
        } else {
            Err(ChainError::SyncActorNotSet)
        }
    }

    /// Check if node is falling behind and trigger catch-up sync
    async fn check_sync_health(&mut self) -> Result<(), ChainError> {
        // Skip if already syncing
        if self.state.sync_status.is_syncing() {
            return Ok(());
        }

        // Get current heights
        let storage_height = self.get_storage_height().await?;
        let network_height = match self.query_network_height().await {
            Ok(h) => h,
            Err(e) => {
                warn!("Could not query network height during health check: {}", e);
                return Ok(());
            }
        };

        const HEALTH_THRESHOLD: u64 = 10;

        if network_height > storage_height + HEALTH_THRESHOLD {
            warn!(
                storage_height = storage_height,
                network_height = network_height,
                gap = network_height - storage_height,
                "🚨 Node falling behind! Triggering catch-up sync"
            );

            self.state.sync_status = SyncStatus::Syncing {
                progress: 0.0,
                target_height: network_height,
            };
            self.trigger_sync().await?;
        } else {
            trace!(
                storage_height = storage_height,
                network_height = network_height,
                "✓ Node is healthy and synced"
            );
        }

        Ok(())
    }

    /// Handle peer connection event
    pub async fn on_peer_connected(&mut self, peer_id: String) -> Result<(), ChainError> {
        debug!(peer_id = %peer_id, "Peer connected");

        // Check if this is first peer after isolation
        let was_isolated = self.connected_peer_count == 0;
        self.connected_peer_count += 1;

        if was_isolated && !self.state.is_synced() {
            info!("First peer connected after isolation, checking sync state");

            // Give peers a moment to stabilize
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Check if we need to sync
            match self.check_sync_health().await {
                Ok(_) => debug!("Sync health check completed after peer connect"),
                Err(e) => warn!("Sync health check failed: {}", e),
            }
        }

        Ok(())
    }

    /// Handle peer disconnection event
    pub async fn on_peer_disconnected(&mut self, peer_id: String) -> Result<(), ChainError> {
        debug!(peer_id = %peer_id, "Peer disconnected");

        self.connected_peer_count = self.connected_peer_count.saturating_sub(1);

        if self.connected_peer_count == 0 {
            warn!("All peers disconnected - node isolated");
        }

        Ok(())
    }

    /// Start background sync health monitoring
    pub fn start_sync_health_monitor(&self, ctx: &mut Context<Self>) {
        const CHECK_INTERVAL: Duration = Duration::from_secs(60);

        ctx.run_interval(CHECK_INTERVAL, |_actor, ctx| {
            let addr = ctx.address();
            tokio::spawn(async move {
                if let Err(e) = addr.send(ChainMessage::CheckSyncHealth).await {
                    error!("Sync health check failed: {}", e);
                }
            });
        });

        info!(
            interval_secs = CHECK_INTERVAL.as_secs(),
            "Sync health monitor started"
        );
    }
}

impl Actor for ChainActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        info!(
            "ChainActor V2 started - is_validator: {}",
            self.config.is_validator
        );
        self.record_activity();

        // Initialize genesis block if it doesn't exist
        // This is critical for consensus - all nodes must share the same genesis
        let storage = self.storage_actor.clone();
        let engine = self.engine_actor.clone();

        // Construct ChainSpec from state (Aura has authorities and slot_duration)
        let chain_spec = crate::spec::ChainSpec {
            slot_duration: self.state.aura.slot_duration,
            authorities: self.state.aura.authorities.clone(),
            federation: self.state.federation.clone(),
            federation_bitcoin_pubkeys: Vec::new(), // Not needed for genesis
            bits: self.state.retarget_params.pow_limit,
            chain_id: self.config.chain_id,
            max_blocks_without_pow: self.state.max_blocks_without_pow,
            bitcoin_start_height: 0, // Not relevant for genesis
            retarget_params: self.state.retarget_params.clone(),
            is_validator: self.state.is_validator,
            execution_timeout_length: 8,       // Default value
            required_btc_txn_confirmations: 6, // Default value
        };

        ctx.spawn(
            async move {
                // Check if genesis already exists
                if let (Some(storage_actor), Some(engine_actor)) = (storage, engine) {
                    match super::genesis::genesis_exists(&storage_actor).await {
                        Ok(true) => {
                            info!("Genesis block already exists in storage");
                        }
                        Ok(false) => {
                            info!("Genesis block not found - creating from execution layer");

                            // Create genesis block from execution layer
                            match super::genesis::create_genesis_block(&engine_actor, chain_spec)
                                .await
                            {
                                Ok(genesis) => {
                                    let genesis_hash = genesis.canonical_root();
                                    info!(
                                        consensus_hash = %genesis_hash,
                                        block_number = genesis.message.execution_payload.block_number,
                                        "Genesis block created successfully"
                                    );

                                    // Store genesis block
                                    let store_msg =
                                        crate::actors_v2::storage::messages::StoreBlockMessage {
                                            block: genesis.clone(),
                                            canonical: true, // Genesis is always canonical
                                            correlation_id: Some(Uuid::new_v4()),
                                        };

                                    if let Err(e) = storage_actor.send(store_msg).await {
                                        error!(
                                            error = ?e,
                                            "Failed to send genesis block to storage actor"
                                        );
                                    } else {
                                        info!("Genesis block stored successfully");
                                    }
                                }
                                Err(e) => {
                                    error!(error = ?e, "Failed to create genesis block");
                                    // Don't panic - this is a recoverable error
                                    // Node can sync genesis from peers if needed
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = ?e, "Failed to check for genesis block existence");
                        }
                    }
                } else {
                    warn!("Storage or Engine actor not set - skipping genesis initialization");
                }
            }
            .into_actor(self),
        );

        // Start periodic sync health monitoring
        self.start_sync_health_monitor(ctx);

        // Initialize sync state after genesis is ready
        let addr = ctx.address();
        ctx.spawn(
            async move {
                // Give genesis initialization time to complete
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                if let Err(e) = addr
                    .send(crate::actors_v2::chain::messages::ChainMessage::InitializeSyncState)
                    .await
                {
                    error!("Failed to initialize sync state: {}", e);
                }
            }
            .into_actor(self),
        );
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        info!("ChainActor V2 stopped");
    }
}

// TODO: ChainManager trait implementation for future EngineActor/AuxPowActor coordination
// This will be implemented when EngineActor/AuxPowActor integration is needed
// The current trait signatures don't match our simplified interface
/*
#[async_trait]
impl crate::auxpow_miner::ChainManager<BlockIndex> for ChainActor {
    // Implementation will be added when needed for EngineActor/AuxPowActor coordination
}
*/
