//! ChainActor V2 Implementation
//!
//! Simplified blockchain actor that replaces both V1 ChainActor complexity and monolithic chain.rs.
//! Follows standard Actix patterns like StorageActor/NetworkActor V2.

use actix::prelude::*;
use bitcoin::hashes::Hash;
use ethereum_types::H256;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use super::{
    messages::{BlockSource, ChainMessage},
    orphan_cache::OrphanBlockCache,
    state::SyncStatus,
    ChainConfig, ChainError, ChainMetrics, ChainState,
};

use crate::actors_v2::{
    engine::EngineActor,
    network::{NetworkActor, SyncActor},
    storage::StorageActor,
};
use crate::block::SignedConsensusBlock;
use lighthouse_wrapper::bls::Keypair as BLSKeypair;
use lighthouse_wrapper::types::MainnetEthSpec;

// Tendermint imports
use super::tendermint::{
    Commit, ConsensusWAL, TendermintState, TimeoutEvent, TimeoutScheduler, ValidatorSet,
};

pub(crate) const DEFAULT_MAX_PENDING_IMPORTS: usize = 1000;

/// Pending import request queued when import lock is held (Phase 2)
#[derive(Debug, Clone)]
pub struct PendingImport {
    pub block: SignedConsensusBlock<MainnetEthSpec>,
    pub source: BlockSource,
    pub queued_at: Instant,
}

/// Queued block waiting for gap fill (Phase 3)
#[derive(Debug, Clone)]
pub struct QueuedBlock {
    pub block: SignedConsensusBlock<MainnetEthSpec>,
    pub source: BlockSource,
    pub peer_id: Option<String>,
    pub queued_at: Instant,
}

/// Queue statistics (Phase 3)
#[derive(Debug)]
pub struct QueueStats {
    pub size: usize,
    pub min_height: u64,
    pub max_height: u64,
    pub oldest_age_secs: u64,
}

/// Gap fill request tracking (Phase 3)
#[derive(Debug, Clone)]
pub struct GapFillRequest {
    pub start_height: u64,
    pub count: u32,
    pub requested_at: Instant,
    pub retry_count: u32,
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

    /// Phase 3: Blocks queued due to gaps (height -> QueuedBlock)
    pub(crate) queued_blocks: Arc<RwLock<HashMap<u64, QueuedBlock>>>,

    /// Phase 3: Active gap fill requests (start_height -> GapFillRequest)
    pub(crate) gap_fill_requests: Arc<RwLock<HashMap<u64, GapFillRequest>>>,

    /// Orphan block cache: stores blocks whose parents haven't been imported yet
    /// Used for out-of-order block reception and tracking observed network height
    pub(crate) orphan_cache: Arc<RwLock<OrphanBlockCache>>,

    /// Active Height Monitoring (Layer 3): Consecutive PayloadIdUnavailable errors
    /// Used to detect chain head desynchronization and trigger emergency re-sync
    pub(crate) payload_unavailable_count: u32,

    // ===== Tendermint Consensus State =====

    /// Tendermint consensus state machine (None if Tendermint mode disabled)
    pub(crate) tendermint_state: Option<Arc<RwLock<TendermintState>>>,

    /// Timeout scheduler for Tendermint consensus phases
    pub(crate) timeout_scheduler: Option<Arc<RwLock<TimeoutScheduler>>>,

    /// Write-ahead log for consensus safety
    pub(crate) consensus_wal: Option<Arc<RwLock<ConsensusWAL>>>,

    /// Validator keypair for signing (None if not a validator)
    pub(crate) validator_keypair: Option<Arc<BLSKeypair>>,

    /// Current validator set (loaded from storage at startup)
    pub(crate) validator_set: Option<Arc<RwLock<ValidatorSet>>>,

    /// Cached last commit for embedding in next block's last_commit field
    pub(crate) cached_last_commit: Option<Arc<RwLock<Commit>>>,

    /// Whether Tendermint consensus mode is enabled
    pub(crate) tendermint_enabled: bool,

    /// Receiver for timeout events from the TimeoutScheduler
    /// Used to trigger TendermintTimeout messages when consensus phases expire.
    /// Wrapped in Arc<Mutex<Option>> to allow Clone while ensuring only one
    /// consumer can take ownership of the receiver.
    pub(crate) timeout_receiver: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<TimeoutEvent>>>>,

    /// TendermintDriver address for consensus coordination
    /// Used to send commit notifications after block finalization
    pub(crate) tendermint_driver: Option<actix::Addr<crate::actors_v2::tendermint_driver::TendermintDriver>>,
}

impl ChainActor {
    /// Create new ChainActor
    pub fn new(config: ChainConfig, state: ChainState) -> Self {
        let mut metrics = ChainMetrics::new();

        // Register metrics with Prometheus ALYS_REGISTRY for /metrics exposure
        tracing::info!("Registering ChainMetrics with Prometheus ALYS_REGISTRY...");
        match metrics.register() {
            Ok(()) => tracing::info!("✓ ChainMetrics registered successfully with Prometheus"),
            Err(e) => tracing::error!("✗ Failed to register ChainMetrics with Prometheus: {}", e),
        }

        // Initialize metrics based on current state (use blocking versions in sync context)
        metrics.set_sync_status(state.is_synced_blocking());
        metrics.set_chain_height(state.get_height_blocking());

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
            max_pending_imports: DEFAULT_MAX_PENDING_IMPORTS, // Configurable limit
            connected_peer_count: 0, // Phase 2: Start with no peers
            // Phase 3: Initialize gap detection queue
            queued_blocks: Arc::new(RwLock::new(HashMap::new())),
            gap_fill_requests: Arc::new(RwLock::new(HashMap::new())),
            // Orphan block cache for out-of-order block reception
            orphan_cache: Arc::new(RwLock::new(OrphanBlockCache::new())),
            // Active Height Monitoring (Layer 3): Initialize error counter
            payload_unavailable_count: 0,
            // Tendermint state (initialized as disabled, enable via configure_tendermint)
            tendermint_state: None,
            timeout_scheduler: None,
            consensus_wal: None,
            validator_keypair: None,
            validator_set: None,
            cached_last_commit: None,
            tendermint_enabled: false,
            timeout_receiver: Arc::new(tokio::sync::Mutex::new(None)),
            tendermint_driver: None,
        }
    }

    /// Set TendermintDriver address for consensus coordination.
    ///
    /// Called during initialization to establish bidirectional communication.
    pub fn set_tendermint_driver(&mut self, addr: actix::Addr<crate::actors_v2::tendermint_driver::TendermintDriver>) {
        self.tendermint_driver = Some(addr);
    }

    /// Configure Tendermint consensus mode.
    ///
    /// Must be called after actor creation to enable Tendermint consensus.
    /// This initializes the state machine, timeout scheduler, and WAL.
    ///
    /// # Arguments
    ///
    /// * `validator_keypair` - BLS keypair for signing (None if non-validator node)
    /// * `validator_set` - Initial validator set
    /// * `wal_path` - Directory path for write-ahead log
    pub fn configure_tendermint(
        &mut self,
        validator_keypair: Option<BLSKeypair>,
        validator_set: ValidatorSet,
        wal_path: &std::path::Path,
    ) -> Result<(), ChainError> {
        use super::tendermint::TimeoutConfig;

        // Wrap in Arc for sharing
        let validator_set = Arc::new(validator_set);

        // Determine our validator ID if we have a keypair
        let our_validator_id = validator_keypair.as_ref().and_then(|kp| {
            validator_set.find_validator(&kp.pk)
        });

        // Initialize state machine
        let state = TendermintState::new(
            self.state.get_height_blocking() + 1, // Start at next height (blocking for sync context)
            validator_set.clone(),
            our_validator_id,
        );

        // Initialize timeout scheduler with event channel
        let timeout_config = TimeoutConfig::default();
        let (timeout_tx, timeout_rx) = tokio::sync::mpsc::channel(100);
        let timeout_scheduler = TimeoutScheduler::new(timeout_config, timeout_tx);

        // Initialize WAL
        let wal = ConsensusWAL::new(wal_path)
            .map_err(|e| ChainError::Internal(format!("Failed to open WAL: {}", e)))?;

        // Clone validator_set for the RwLock
        let validator_set_for_state = (*validator_set).clone();

        // Store state
        self.tendermint_state = Some(Arc::new(RwLock::new(state)));
        self.timeout_scheduler = Some(Arc::new(RwLock::new(timeout_scheduler)));
        self.consensus_wal = Some(Arc::new(RwLock::new(wal)));
        self.validator_keypair = validator_keypair.map(Arc::new);
        self.validator_set = Some(Arc::new(RwLock::new(validator_set_for_state)));
        self.cached_last_commit = None;
        self.tendermint_enabled = true;
        // Store the timeout receiver (wrapped for Clone compatibility)
        *self.timeout_receiver.blocking_lock() = Some(timeout_rx);

        info!(
            our_validator_id = ?our_validator_id,
            "Tendermint consensus mode enabled"
        );

        Ok(())
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
        self.metrics.set_chain_height(self.state.get_height_blocking());
        self.metrics.set_sync_status(self.state.is_synced_blocking());
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
            let current_height = self.state.get_height().await;
            let current_cumulative_difficulty = self.state.get_cumulative_difficulty().await;

            // Call the reorganization module
            let result = super::reorganization::reorganize_to_new_tip(
                new_tip_block,
                current_height,
                current_cumulative_difficulty,
                storage_actor,
                correlation_id,
            )
            .await?;

            // Update ChainState with new cumulative difficulty after successful reorg
            self.state
                .set_cumulative_difficulty(result.new_cumulative_difficulty)
                .await;

            // Cache the new tip's cumulative difficulty
            self.state
                .cache_difficulty(result.new_tip_height, result.new_cumulative_difficulty)
                .await;

            // For deep reorgs, invalidate cache entries above reorg height
            if result.is_deep_reorg {
                // Clear entries above the common ancestor (rolled back blocks)
                // The rollback_difficulty method handles this
                self.state
                    .rollback_difficulty(result.reorg_height, result.new_cumulative_difficulty)
                    .await;
            }

            info!(
                correlation_id = %correlation_id,
                reorg_height = result.reorg_height,
                blocks_rolled_back = result.blocks_rolled_back,
                blocks_applied = result.blocks_applied,
                new_tip = %result.new_tip,
                new_cumulative_difficulty = result.new_cumulative_difficulty,
                "Chain reorganization completed - ChainState updated"
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

            let msg = crate::actors_v2::network::SyncMessage::StartSync {
                start_height: 0, // Will be determined from storage by SyncActor
                target_height: None, // Discover from network
            };

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
        let sync_status = self.state.get_sync_status().await;
        if sync_status.is_syncing() {
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

            self.state.set_sync_status(SyncStatus::Syncing {
                progress: 0.0,
                target_height: network_height,
            }).await;
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
    ///
    /// ACTIVE HEIGHT MONITORING (Layer 2): Always check sync health after isolation ends.
    /// Previously this only checked if `!is_synced()`, which missed the case where we
    /// were "synced" but fell behind during a network partition.
    pub async fn on_peer_connected(&mut self, peer_id: String) -> Result<(), ChainError> {
        debug!(peer_id = %peer_id, "Peer connected");

        // Check if this is first peer after isolation
        let was_isolated = self.connected_peer_count == 0;
        self.connected_peer_count += 1;

        // CHANGED: Always check sync health after isolation ends
        // Don't skip just because we think we're "synced" - we might have
        // fallen behind during the isolation period (e.g., network partition)
        if was_isolated {
            info!("First peer connected after isolation - checking sync state");

            // Give peer connection time to stabilize
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Force a fresh network height query before health check
            // This ensures we have up-to-date peer height information
            if let Some(ref sync_actor) = self.sync_actor {
                match sync_actor
                    .send(crate::actors_v2::network::SyncMessage::RefreshNetworkHeight)
                    .await
                {
                    Ok(Ok(_)) => {
                        // Give time for peer height responses to arrive
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        debug!("Network height refreshed after reconnection");
                    }
                    Ok(Err(e)) => {
                        warn!(error = ?e, "Failed to refresh network height after reconnection");
                    }
                    Err(e) => {
                        warn!(error = %e, "Mailbox error refreshing network height");
                    }
                }
            }

            // Check if we need to sync
            match self.check_sync_health().await {
                Ok(_) => debug!("Sync health check completed after reconnection"),
                Err(e) => warn!(error = ?e, "Sync health check failed after reconnection"),
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

    /// Import block with gap detection (Phase 3)
    pub async fn import_block_with_gap_detection(
        &mut self,
        block: SignedConsensusBlock<MainnetEthSpec>,
        source: BlockSource,
        peer_id: Option<String>,
    ) -> Result<(), ChainError> {
        let block_height = block.message.execution_payload.block_number;
        let current_height = self.state.get_height().await;
        let expected_height = current_height + 1;

        debug!(
            block_height = block_height,
            expected_height = expected_height,
            source = ?source,
            "Importing block with gap detection"
        );

        // Check for gap
        if block_height > expected_height {
            let gap_size = block_height - expected_height;

            warn!(
                block_height = block_height,
                expected_height = expected_height,
                gap_size = gap_size,
                "🔍 Gap detected! Missing {} blocks",
                gap_size
            );

            // Queue out-of-order block
            let mut queued_blocks = self.queued_blocks.write().await;
            queued_blocks.insert(
                block_height,
                QueuedBlock {
                    block: block.clone(),
                    source,
                    peer_id: peer_id.clone(),
                    queued_at: Instant::now(),
                },
            );

            info!(
                queued_height = block_height,
                queue_size = queued_blocks.len(),
                "Block queued, requesting missing blocks"
            );
            drop(queued_blocks);

            // Request missing blocks via SyncActor
            self.request_blocks(expected_height, gap_size as u32)
                .await?;

            return Ok(());
        }

        // Check for duplicate or old block
        if block_height < expected_height {
            debug!(
                block_height = block_height,
                expected_height = expected_height,
                "Ignoring old/duplicate block"
            );
            return Ok(());
        }

        // Normal import (block_height == expected_height)
        self.import_block_internal(block, source, peer_id).await?;

        // Check if we can process queued blocks
        self.process_queued_blocks().await?;

        Ok(())
    }

    /// Process queued blocks that can now be imported
    async fn process_queued_blocks(&mut self) -> Result<(), ChainError> {
        let mut processed_count = 0;

        loop {
            let current_height = self.state.get_height().await;
            let next_height = current_height + 1;

            // Check if we have the next sequential block
            let queued_block = {
                let mut queued_blocks = self.queued_blocks.write().await;
                queued_blocks.remove(&next_height)
            };

            if let Some(queued) = queued_block {
                info!(
                    height = next_height,
                    "Processing queued block"
                );

                // Import the queued block
                match self
                    .import_block_internal(queued.block, queued.source, queued.peer_id)
                    .await
                {
                    Ok(_) => {
                        processed_count += 1;
                    }
                    Err(e) => {
                        error!(
                            height = next_height,
                            error = ?e,
                            "Failed to import queued block"
                        );
                        // Continue with next block
                    }
                }
            } else {
                // No more sequential blocks available
                break;
            }
        }

        if processed_count > 0 {
            let queue_size = self.queued_blocks.read().await.len();
            info!(
                processed = processed_count,
                queue_remaining = queue_size,
                "✓ Processed queued blocks"
            );
        }

        // Clean up old queued blocks (older than 5 minutes)
        self.cleanup_stale_queued_blocks().await;

        Ok(())
    }

    /// Remove queued blocks that are too old
    async fn cleanup_stale_queued_blocks(&self) {
        const MAX_QUEUE_AGE: Duration = Duration::from_secs(300); // 5 minutes

        let now = Instant::now();
        let mut queued_blocks = self.queued_blocks.write().await;
        let initial_count = queued_blocks.len();

        queued_blocks.retain(|height, queued| {
            let age = now.duration_since(queued.queued_at);
            if age > MAX_QUEUE_AGE {
                warn!(
                    height = height,
                    age_secs = age.as_secs(),
                    "Removing stale queued block"
                );
                false
            } else {
                true
            }
        });

        let removed_count = initial_count - queued_blocks.len();
        if removed_count > 0 {
            warn!(
                removed = removed_count,
                remaining = queued_blocks.len(),
                "Cleaned up stale queued blocks"
            );
        }
    }

    /// Internal block import (assumes block is at correct height)
    async fn import_block_internal(
        &mut self,
        block: SignedConsensusBlock<MainnetEthSpec>,
        _source: BlockSource,
        _peer_id: Option<String>,
    ) -> Result<(), ChainError> {
        // TODO: Implement actual block validation and import logic
        // For now, just update the height
        let block_height = block.message.execution_payload.block_number;

        info!(
            height = block_height,
            "Block imported successfully (placeholder)"
        );

        // Update chain state height (placeholder)
        // In real implementation, this would be done by StorageActor
        // self.state.head.height = block_height;

        // Mark gap fill as progressing (Phase 3)
        self.complete_gap_fill(block_height, block_height).await;

        Ok(())
    }

    /// Add block to queue with overflow protection (Phase 3)
    async fn queue_block(
        &self,
        height: u64,
        block: SignedConsensusBlock<MainnetEthSpec>,
        source: BlockSource,
        peer_id: Option<String>,
    ) -> Result<(), ChainError> {
        const MAX_QUEUED_BLOCKS: usize = 1000;

        let mut queued_blocks = self.queued_blocks.write().await;

        // Check queue size limit
        if queued_blocks.len() >= MAX_QUEUED_BLOCKS {
            error!(
                queue_size = queued_blocks.len(),
                max_size = MAX_QUEUED_BLOCKS,
                "Queue full, rejecting block"
            );

            // Drop lock before cleanup
            drop(queued_blocks);

            // Emergency cleanup
            self.cleanup_stale_queued_blocks().await;

            // Re-acquire lock and check again
            {
                let queued_blocks_check = self.queued_blocks.read().await;
                if queued_blocks_check.len() >= MAX_QUEUED_BLOCKS {
                    return Err(ChainError::QueueFull);
                }
            }

            // Re-acquire write lock to continue
            queued_blocks = self.queued_blocks.write().await;
        }

        // Check for duplicate
        if queued_blocks.contains_key(&height) {
            debug!(height = height, "Block already queued, ignoring");
            return Ok(());
        }

        // Queue the block
        queued_blocks.insert(
            height,
            QueuedBlock {
                block,
                source,
                peer_id,
                queued_at: Instant::now(),
            },
        );

        info!(
            height = height,
            queue_size = queued_blocks.len(),
            "Block queued"
        );

        Ok(())
    }

    /// Get queue statistics
    async fn get_queue_stats(&self) -> QueueStats {
        let queued_blocks = self.queued_blocks.read().await;

        if queued_blocks.is_empty() {
            return QueueStats {
                size: 0,
                min_height: 0,
                max_height: 0,
                oldest_age_secs: 0,
            };
        }

        let min_height = *queued_blocks.keys().min().unwrap();
        let max_height = *queued_blocks.keys().max().unwrap();

        let oldest_age = queued_blocks
            .values()
            .map(|q| Instant::now().duration_since(q.queued_at))
            .max()
            .unwrap_or(Duration::ZERO);

        QueueStats {
            size: queued_blocks.len(),
            min_height,
            max_height,
            oldest_age_secs: oldest_age.as_secs(),
        }
    }

    /// Monitor queue health periodically (Phase 3)
    pub fn start_queue_monitor(&self, ctx: &mut Context<Self>) {
        const MONITOR_INTERVAL: Duration = Duration::from_secs(30);

        let actor_clone = self.clone();
        ctx.run_interval(MONITOR_INTERVAL, move |_actor, _ctx| {
            let actor_clone_inner = actor_clone.clone();
            tokio::spawn(async move {
                let stats = actor_clone_inner.get_queue_stats().await;

                if stats.size > 0 {
                    info!(
                        queue_size = stats.size,
                        min_height = stats.min_height,
                        max_height = stats.max_height,
                        oldest_age_secs = stats.oldest_age_secs,
                        "Queue status"
                    );

                    // Alert if queue is growing large
                    if stats.size > 500 {
                        warn!(
                            queue_size = stats.size,
                            "⚠️ Queue growing large - potential sync issue"
                        );
                    }

                    // Alert if blocks are getting stale
                    if stats.oldest_age_secs > 120 {
                        warn!(
                            oldest_age_secs = stats.oldest_age_secs,
                            "⚠️ Queued blocks getting old - gap fill may be stuck"
                        );
                    }
                }
            });
        });
    }

    /// Request blocks with retry tracking (Phase 3)
    pub async fn request_blocks_with_retry(
        &self,
        start_height: u64,
        count: u32,
    ) -> Result<(), ChainError> {
        const MAX_RETRIES: u32 = 3;

        let mut gap_fill_requests = self.gap_fill_requests.write().await;

        // Check if we already have an active request for this range
        let existing_request = gap_fill_requests.get(&start_height);

        if let Some(existing) = existing_request {
            // Check if request is recent (< 30 seconds)
            if existing.requested_at.elapsed() < Duration::from_secs(30) {
                debug!(
                    start_height = start_height,
                    age_secs = existing.requested_at.elapsed().as_secs(),
                    "Gap fill request already active, skipping"
                );
                return Ok(());
            }

            // Check retry limit
            if existing.retry_count >= MAX_RETRIES {
                error!(
                    start_height = start_height,
                    retry_count = existing.retry_count,
                    "Gap fill failed after max retries"
                );
                // Remove failed request
                gap_fill_requests.remove(&start_height);
                return Err(ChainError::Internal("Gap fill failed after max retries".to_string()));
            }
        }

        // Track retry count
        let retry_count = existing_request.map(|r| r.retry_count + 1).unwrap_or(0);

        // Drop write lock before sending message
        drop(gap_fill_requests);

        // Send request via SyncActor (reuse existing method)
        self.request_blocks(start_height, count).await?;

        // Re-acquire write lock to track request
        let mut gap_fill_requests = self.gap_fill_requests.write().await;
        gap_fill_requests.insert(
            start_height,
            GapFillRequest {
                start_height,
                count,
                requested_at: Instant::now(),
                retry_count,
            },
        );

        info!(
            start_height = start_height,
            count = count,
            retry_count = retry_count,
            "Gap fill request sent"
        );

        Ok(())
    }

    /// Mark gap fill request as completed
    async fn complete_gap_fill(&self, start_height: u64, end_height: u64) {
        let mut gap_fill_requests = self.gap_fill_requests.write().await;

        // Remove all completed requests in range
        let to_remove: Vec<u64> = gap_fill_requests
            .keys()
            .filter(|&&h| h >= start_height && h <= end_height)
            .copied()
            .collect();

        for height in to_remove {
            gap_fill_requests.remove(&height);
            debug!(height = height, "Gap fill completed");
        }
    }

    /// Cleanup stale gap fill requests
    async fn cleanup_stale_gap_requests(&self) {
        const MAX_REQUEST_AGE: Duration = Duration::from_secs(60);

        let now = Instant::now();
        let mut gap_fill_requests = self.gap_fill_requests.write().await;
        let initial_count = gap_fill_requests.len();

        gap_fill_requests.retain(|height, request| {
            let age = now.duration_since(request.requested_at);
            if age > MAX_REQUEST_AGE {
                warn!(
                    height = height,
                    age_secs = age.as_secs(),
                    retry_count = request.retry_count,
                    "Removing stale gap fill request"
                );
                false
            } else {
                true
            }
        });

        let removed_count = initial_count - gap_fill_requests.len();
        if removed_count > 0 {
            warn!(
                removed = removed_count,
                "Cleaned up stale gap fill requests"
            );
        }
    }

    /// Start background sync health monitoring
    ///
    /// Enhancement: Phase 6.4 - Add initial check after short delay
    pub fn start_sync_health_monitor(&self, ctx: &mut Context<Self>) {
        const CHECK_INTERVAL: Duration = Duration::from_secs(60);
        const INITIAL_CHECK_DELAY: Duration = Duration::from_secs(5);

        // Schedule initial check after short delay (let network settle)
        ctx.run_later(INITIAL_CHECK_DELAY, |_actor, ctx| {
            let addr = ctx.address();
            tokio::spawn(async move {
                if let Err(e) = addr.send(ChainMessage::CheckSyncHealth).await {
                    error!("Initial sync health check failed: {}", e);
                }
            });
        });

        // Then schedule periodic checks
        ctx.run_interval(CHECK_INTERVAL, |_actor, ctx| {
            let addr = ctx.address();
            tokio::spawn(async move {
                if let Err(e) = addr.send(ChainMessage::CheckSyncHealth).await {
                    error!("Sync health check failed: {}", e);
                }
            });
        });

        info!(
            initial_check_secs = INITIAL_CHECK_DELAY.as_secs(),
            interval_secs = CHECK_INTERVAL.as_secs(),
            "Sync health monitor started"
        );
    }

    /// Start the Tendermint timeout event loop.
    ///
    /// This spawns a background task that:
    /// 1. Takes ownership of the timeout receiver
    /// 2. Polls for timeout events from the TimeoutScheduler
    /// 3. Dispatches TendermintTimeout messages to self when timeouts fire
    ///
    /// This is critical for consensus liveness - timeouts drive round advancement
    /// when proposals or votes are not received in time.
    fn start_tendermint_timeout_loop(&self, ctx: &mut Context<Self>) {
        if !self.tendermint_enabled {
            return;
        }

        let timeout_receiver = self.timeout_receiver.clone();
        let addr = ctx.address();

        // Spawn a task that takes ownership of the receiver and processes events
        ctx.spawn(
            async move {
                // Try to take the receiver from the Option
                let mut receiver = {
                    let mut guard = timeout_receiver.lock().await;
                    match guard.take() {
                        Some(rx) => rx,
                        None => {
                            warn!("Tendermint timeout receiver already taken or not initialized");
                            return;
                        }
                    }
                };

                info!("Tendermint timeout event loop started");

                // Process timeout events until the channel closes
                while let Some(event) = receiver.recv().await {
                    let correlation_id = uuid::Uuid::new_v4();

                    debug!(
                        correlation_id = %correlation_id,
                        height = event.height,
                        round = event.round,
                        step = ?event.step,
                        "Timeout event received from scheduler"
                    );

                    // Send TendermintTimeout message to self
                    let msg = super::messages::ChainMessage::TendermintTimeout {
                        height: event.height,
                        round: event.round,
                        step: event.step,
                        correlation_id: Some(correlation_id),
                    };

                    if let Err(e) = addr.send(msg).await {
                        error!(
                            correlation_id = %correlation_id,
                            error = %e,
                            "Failed to send TendermintTimeout message to ChainActor"
                        );
                    }
                }

                info!("Tendermint timeout event loop ended (channel closed)");
            }
            .into_actor(self),
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

        // Start queue monitoring (Phase 3)
        self.start_queue_monitor(ctx);

        // Start Tendermint timeout event loop (if enabled)
        self.start_tendermint_timeout_loop(ctx);

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
