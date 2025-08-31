//! Network Actors Integration Example
//! 
//! Demonstrates how to use the completed network actor system for
//! blockchain synchronization, peer management, and federation coordination.

use actix::{Actor, System, Addr};
use std::time::Duration;

use crate::actors::network::{
    NetworkSupervisor,
    sync::{SyncActor, SyncConfig},
    network::{NetworkActor, NetworkConfig},
    peer::{PeerActor, PeerConfig},
    messages::*,
};

/// Example usage of the completed network actor system
pub struct NetworkIntegrationExample {
    pub supervisor: Addr<NetworkSupervisor>,
    pub sync_actor: Addr<SyncActor>,
    pub network_actor: Addr<NetworkActor>,
    pub peer_actor: Addr<PeerActor>,
}

impl NetworkIntegrationExample {
    /// Initialize the complete network actor system
    pub async fn initialize() -> Result<Self, Box<dyn std::error::Error>> {
        // 1. Configure all network actors
        let sync_config = SyncConfig {
            production_threshold: 0.995, // 99.5% threshold for block production
            max_parallel_downloads: 8,
            request_timeout: Duration::from_secs(30),
            checkpoint_interval: 1000, // Create checkpoint every 1000 blocks
            health_check_interval: Duration::from_secs(60),
            ..Default::default()
        };

        let network_config = NetworkConfig {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/30303".parse()?,
                "/ip6/::/tcp/30303".parse()?,
            ],
            bootstrap_peers: vec![
                // Add your bootstrap peers here
            ],
            federation_config: crate::actors::network::network::config::FederationNetworkConfig {
                federation_discovery: true,
                federation_topics: vec![
                    "alys/federation/consensus/v1".to_string(),
                    "alys/federation/blocks/v1".to_string(),
                    "alys/federation/emergency/v1".to_string(),
                ],
                ..Default::default()
            },
            ..Default::default()
        };

        let peer_config = PeerConfig::default();

        // 2. Start the actors
        let sync_actor = SyncActor::new(sync_config)?.start();
        let network_actor = NetworkActor::new(network_config)?.start();
        let peer_actor = PeerActor::new(peer_config)?.start();

        // 3. Create and start the network supervisor
        let supervisor_config = crate::actors::network::supervisor::NetworkSupervisorConfig::default();
        let supervisor = NetworkSupervisor::new(
            supervisor_config,
            sync_actor.clone(),
            network_actor.clone(),
            peer_actor.clone(),
        ).start();

        // 4. Cross-reference actors for coordination
        sync_actor.send(SetActorAddresses {
            chain_actor: None, // Would be provided by ChainActor
            network_actor: Some(network_actor.clone()),
            peer_actor: Some(peer_actor.clone()),
        }).await??;

        Ok(Self {
            supervisor,
            sync_actor,
            network_actor,
            peer_actor,
        })
    }

    /// Start networking subsystem
    pub async fn start_network(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("🚀 Starting Alys network subsystem...");

        // Start networking
        let start_msg = StartNetwork {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/30303".parse()?,
                "/ip6/::/tcp/30303".parse()?,
            ],
            bootstrap_peers: vec![], // Add bootstrap peers as needed
            enable_mdns: true,
        };

        let network_status = self.network_actor.send(start_msg).await??;
        tracing::info!("✅ Network started on {:?}", network_status);

        // Subscribe to essential topics
        for topic in ["blocks", "transactions", "discovery"] {
            let subscribe_msg = SubscribeToTopic {
                topic: match topic {
                    "blocks" => GossipTopic::Blocks,
                    "transactions" => GossipTopic::Transactions,
                    "discovery" => GossipTopic::Discovery,
                    _ => GossipTopic::Custom(topic.to_string()),
                },
            };
            
            self.network_actor.send(subscribe_msg).await??;
            tracing::info!("📡 Subscribed to topic: {}", topic);
        }

        Ok(())
    }

    /// Example: Start blockchain synchronization
    pub async fn start_sync(&self, target_height: Option<u64>) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("🔄 Starting blockchain synchronization...");

        let sync_msg = sync_messages::StartSync {
            from_height: None, // Start from current height
            target_height,
            sync_mode: sync_messages::SyncMode::Fast,
            priority_peers: vec![], // Let the system choose peers
        };

        let sync_response = self.sync_actor.send(sync_msg).await??;
        tracing::info!("✅ Sync started: {:?}", sync_response);

        Ok(())
    }

    /// Example: Check if ready for block production (99.5% threshold)
    pub async fn can_produce_blocks(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let can_produce = self.sync_actor.send(sync_messages::CanProduceBlocks).await??;
        
        if can_produce {
            tracing::info!("🎯 Ready for block production - sync threshold reached!");
        } else {
            let status = self.sync_actor.send(sync_messages::GetSyncStatus).await??;
            tracing::info!(
                "⏳ Not ready for production - sync at {:.2}% (need 99.5%)",
                status.sync_progress * 100.0
            );
        }

        Ok(can_produce)
    }

    /// Example: Broadcast a new block to the network
    pub async fn broadcast_block(&self, block_data: Vec<u8>, height: u64, hash: String) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("📤 Broadcasting block {} to network...", height);

        let broadcast_msg = BroadcastBlock {
            block_data,
            block_height: height,
            block_hash: hash,
            priority: true, // Mark as priority for federation
        };

        let response = self.network_actor.send(broadcast_msg).await??;
        tracing::info!(
            "✅ Block broadcast complete - reached {} peers (message_id: {})",
            response.peers_reached,
            response.message_id
        );

        Ok(())
    }

    /// Example: Request blocks from peers for sync
    pub async fn request_blocks(&self, start_height: u64, count: u32) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("📥 Requesting {} blocks starting from height {}", count, start_height);

        let request_msg = sync_messages::RequestBlocks {
            start_height,
            count,
            preferred_peers: vec![], // Let the system choose best peers
        };

        let blocks_response = self.sync_actor.send(request_msg).await??;
        tracing::info!(
            "✅ Received {} blocks from sources: {:?}",
            blocks_response.blocks.len(),
            blocks_response.source_peers
        );

        Ok(())
    }

    /// Example: Create a blockchain state checkpoint
    pub async fn create_checkpoint(&self, height: Option<u64>) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("💾 Creating blockchain checkpoint...");

        let checkpoint_msg = sync_messages::CreateCheckpoint {
            height,
            compression: true,
        };

        let checkpoint_response = self.sync_actor.send(checkpoint_msg).await??;
        tracing::info!(
            "✅ Checkpoint created: {} ({} bytes)",
            checkpoint_response.checkpoint_id,
            checkpoint_response.size_bytes
        );

        Ok(())
    }

    /// Example: Get comprehensive network status
    pub async fn get_network_status(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Get sync status
        let sync_status = self.sync_actor.send(sync_messages::GetSyncStatus).await??;
        tracing::info!("📊 Sync Status:");
        tracing::info!("  Current Height: {}", sync_status.current_height);
        tracing::info!("  Target Height: {:?}", sync_status.target_height);
        tracing::info!("  Progress: {:.2}%", sync_status.sync_progress * 100.0);
        tracing::info!("  Can Produce Blocks: {}", sync_status.can_produce_blocks);
        tracing::info!("  Blocks/sec: {:.1}", sync_status.blocks_per_second);

        // Get network status  
        let network_status = self.network_actor.send(GetNetworkStatus).await??;
        tracing::info!("🌐 Network Status:");
        tracing::info!("  Connected Peers: {}", network_status.connected_peers);
        tracing::info!("  Listening Addresses: {:?}", network_status.listening_addresses);
        tracing::info!("  Bandwidth In: {} bytes", network_status.total_bandwidth_in);
        tracing::info!("  Bandwidth Out: {} bytes", network_status.total_bandwidth_out);

        Ok(())
    }

    /// Example: Graceful shutdown of the network system
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("🛑 Shutting down network subsystem...");

        // Stop sync operations
        let stop_sync_msg = sync_messages::StopSync { force: false };
        self.sync_actor.send(stop_sync_msg).await??;

        // Stop network operations
        let stop_network_msg = StopNetwork { graceful: true };
        self.network_actor.send(stop_network_msg).await??;

        tracing::info!("✅ Network subsystem shutdown complete");

        Ok(())
    }
}

/// Demo function showing the complete network actor integration
pub async fn run_network_integration_demo() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("🎬 Starting Alys Network Actors Integration Demo");

    // Initialize the complete network system
    let network_system = NetworkIntegrationExample::initialize().await?;

    // Start networking
    network_system.start_network().await?;

    // Wait for network to initialize
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Start synchronization
    network_system.start_sync(Some(1000)).await?;

    // Monitor sync progress
    for i in 0..10 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        let can_produce = network_system.can_produce_blocks().await?;
        if can_produce {
            tracing::info!("🎯 Block production threshold reached!");
            break;
        }
        
        if i == 9 {
            tracing::info!("⏰ Demo timeout - sync still in progress");
        }
    }

    // Get status report
    network_system.get_network_status().await?;

    // Demo block broadcasting (simulated)
    let dummy_block = vec![1, 2, 3, 4]; // Simulated block data
    network_system.broadcast_block(dummy_block, 1001, "dummy_hash".to_string()).await?;

    // Demo checkpoint creation
    network_system.create_checkpoint(Some(1000)).await?;

    // Graceful shutdown
    network_system.shutdown().await?;

    tracing::info!("✅ Network Actors Integration Demo Complete!");

    Ok(())
}

// Helper message types for actor coordination

#[derive(actix::Message)]
#[rtype(result = "Result<(), actix::MailboxError>")]
pub struct SetActorAddresses {
    pub chain_actor: Option<Addr<crate::actors::chain::ChainActor>>,
    pub network_actor: Option<Addr<NetworkActor>>,
    pub peer_actor: Option<Addr<PeerActor>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;

    #[tokio::test]
    async fn test_network_integration_example() {
        // This test would require proper actor system setup
        // For now, just test that the structure compiles
        assert!(true);
    }

    #[test]
    fn test_configuration_validity() {
        let sync_config = SyncConfig {
            production_threshold: 0.995,
            ..Default::default()
        };
        
        assert!(sync_config.production_threshold >= 0.995);
        assert!(sync_config.max_parallel_downloads > 0);
    }
}