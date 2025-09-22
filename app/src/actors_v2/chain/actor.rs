//! ChainActor V2 Implementation
//!
//! Simplified blockchain actor that replaces both V1 ChainActor complexity and monolithic chain.rs.
//! Follows standard Actix patterns like StorageActor/NetworkActor V2.

use actix::prelude::*;
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

use super::{
    ChainConfig, ChainError, ChainMetrics, ChainState,
};

use crate::actors_v2::{
    storage::StorageActor,
    network::{NetworkActor, SyncActor},
};

/// Simplified ChainActor - core blockchain functionality
pub struct ChainActor {
    /// Configuration
    pub(crate) config: ChainConfig,

    /// Core blockchain state (derived from chain.rs)
    pub(crate) state: ChainState,

    /// Actor integration
    pub(crate) storage_actor: Option<Addr<StorageActor>>,
    pub(crate) network_actor: Option<Addr<NetworkActor>>,
    pub(crate) sync_actor: Option<Addr<SyncActor>>,

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
            // Convert SignedConsensusBlock to AlysConsensusBlock (ConsensusBlock) for StorageActor
            // Extract the consensus block from the signed wrapper
            let alys_block = block.message; // SignedConsensusBlock.message contains the ConsensusBlock

            let store_msg = crate::actors_v2::storage::messages::StoreBlockMessage {
                block: alys_block,
                canonical,
                correlation_id: Some(Uuid::new_v4()), // Generate correlation ID for tracing
            };

            storage_actor.send(store_msg).await
                .map_err(|e| ChainError::NetworkError(format!("Failed to send store message: {}", e)))?
                .map_err(|e| ChainError::Storage(e.to_string()))?;
        }
        Ok(())
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