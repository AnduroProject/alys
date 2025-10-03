//! ChainActor V2 Implementation
//!
//! Simplified blockchain actor that replaces both V1 ChainActor complexity and monolithic chain.rs.
//! Follows standard Actix patterns like StorageActor/NetworkActor V2.

use actix::prelude::*;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use ethereum_types::H256;
use bitcoin::hashes::Hash;

use super::{
    ChainConfig, ChainError, ChainMetrics, ChainState,
};

use crate::actors_v2::{
    storage::StorageActor,
    network::{NetworkActor, SyncActor},
    engine::EngineActor,
};

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
        }
    }

    /// Set storage actor address
    pub fn set_storage_actor(&mut self, addr: Addr<StorageActor>) {
        self.storage_actor = Some(addr);
    }

    /// Set network actor addresses
    pub fn set_network_actors(&mut self, network_addr: Addr<NetworkActor>, sync_addr: Addr<SyncActor>) {
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
            if let Ok(response) = network_actor.send(crate::actors_v2::network::NetworkMessage::GetNetworkStatus).await {
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
                priority: true
            };
            network_actor.send(msg).await
                .map_err(|e| ChainError::NetworkError(e.to_string()))?
                .map_err(ChainError::Network)?;
        }
        Ok(())
    }

    /// Request missing blocks for sync
    pub(crate) async fn request_blocks(&self, start_height: u64, count: u32) -> Result<(), ChainError> {
        if let Some(ref sync_actor) = self.sync_actor {
            let msg = crate::actors_v2::network::SyncMessage::RequestBlocks {
                start_height,
                count,
                peer_id: None
            };
            sync_actor.send(msg).await
                .map_err(|e| ChainError::NetworkError(e.to_string()))?
                .map_err(ChainError::Sync)?;
        }
        Ok(())
    }

    /// Store block via StorageActor
    pub(crate) async fn store_block(&self, block: crate::block::SignedConsensusBlock<lighthouse_wrapper::types::MainnetEthSpec>, canonical: bool) -> Result<(), ChainError> {
        if let Some(ref storage_actor) = self.storage_actor {
            // Store the complete signed block (AlysConsensusBlock now expects SignedConsensusBlock)
            let store_msg = crate::actors_v2::storage::messages::StoreBlockMessage {
                block,
                canonical,
                correlation_id: Some(Uuid::new_v4()), // Generate correlation ID for tracing
            };

            storage_actor.send(store_msg).await
                .map_err(|e| ChainError::NetworkError(format!("Failed to send store message: {}", e)))?
                .map_err(|e| ChainError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    /// Process peg-in from imported block (Phase 3 - Task 3.1.2) - Real implementation
    pub async fn process_block_pegin(&self, pegin: &bridge::PegInInfo, block_hash: &H256) -> Result<(), ChainError> {
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
            return Err(ChainError::Bridge("Peg-in has zero EVM account".to_string()));
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
                    return Err(ChainError::Bridge("Bitcoin transaction not found".to_string()));
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
                return Err(ChainError::Bridge(format!("Wallet registration failed: {:?}", wallet_error)));
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
    pub async fn process_finalized_pegout(&self, pegout: &bitcoin::Transaction, block_hash: &H256) -> Result<(), ChainError> {
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
            return Err(ChainError::Bridge("Peg-out has zero output value".to_string()));
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

    /// Update chain head after successful block import (Phase 3 - Task 3.1.2)
    pub async fn update_chain_head(&self, new_head: crate::actors_v2::storage::actor::BlockRef) -> Result<(), ChainError> {
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
                Ok(storage_result) => {
                    match storage_result {
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
                    }
                }
                Err(e) => {
                    error!(
                        head_hash = %new_head.hash,
                        error = ?e,
                        "Communication error updating chain head"
                    );
                    Err(ChainError::NetworkError(format!("Storage communication failed: {}", e)))
                }
            }
        } else {
            Err(ChainError::Storage("StorageActor not available".to_string()))
        }
    }
}

impl Actor for ChainActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        info!("ChainActor V2 started - is_validator: {}", self.config.is_validator);
        self.record_activity();
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