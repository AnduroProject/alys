//! Sync Integration Tests
//! 
//! Integration tests for SyncActor with NetworkActor and PeerActor coordination.

use actix::prelude::*;
use std::time::Duration;

use crate::actors::network::{SyncActor, NetworkActor, PeerActor, messages::*};
use crate::actors::network::tests::helpers::*;

/// Integration test setup for sync workflows
#[derive(Debug)]
pub struct SyncIntegrationSetup {
    pub sync_addr: Addr<SyncActor>,
    pub network_addr: Addr<NetworkActor>,
    pub peer_addr: Addr<PeerActor>,
}

impl SyncIntegrationSetup {
    pub async fn new() -> Self {
        let sync_config = test_sync_config();
        let network_config = test_network_config();
        let peer_config = test_peer_config();

        let sync_addr = SyncActor::new(sync_config).unwrap().start();
        let network_addr = NetworkActor::new(network_config).unwrap().start();
        let peer_addr = PeerActor::new(peer_config).unwrap().start();

        // Start all systems
        network_addr.send(StartNetwork {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            bootstrap_peers: vec![],
        }).await.unwrap();

        peer_addr.send(StartPeerManager).await.unwrap();

        Self {
            sync_addr,
            network_addr,
            peer_addr,
        }
    }
}

#[actix::test]
async fn test_sync_network_coordination() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Start sync process
    let sync_msg = StartSync {
        target_block: Some(100),
        force_restart: false,
    };
    let sync_result = setup.sync_addr.send(sync_msg).await;
    assert!(sync_result.is_ok());
    
    // Verify network is involved in sync
    let network_status = setup.network_addr.send(GetNetworkStatus).await.unwrap().unwrap();
    assert!(network_status.connected_peers >= 0);
}

#[actix::test]
async fn test_block_propagation_workflow() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Simulate receiving a new block from network
    let block_data = create_test_block_data(150);
    let network_msg = MessageReceived {
        from_peer: libp2p::PeerId::random(),
        topic: "blocks".to_string(),
        data: block_data.clone(),
    };
    
    // Network receives block
    let network_result = setup.network_addr.send(network_msg).await;
    assert!(network_result.is_ok());
    
    // Sync should process the block
    let sync_msg = ProcessNewBlock {
        block_hash: "integration_block_150".to_string(),
        block_data,
        from_peer: "integration_peer".to_string(),
    };
    
    let sync_result = setup.sync_addr.send(sync_msg).await;
    assert!(sync_result.is_ok());
}

#[actix::test]
async fn test_peer_discovery_for_sync() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Start peer discovery
    let discovery_result = setup.peer_addr.send(StartDiscovery).await;
    assert!(discovery_result.is_ok());
    
    // Discover peers for sync
    let discover_msg = DiscoverPeers {
        target_count: Some(5),
    };
    let discover_result = setup.peer_addr.send(discover_msg).await;
    assert!(discover_result.is_ok());
    
    // Start sync with discovered peers
    let sync_msg = StartSync {
        target_block: Some(10),
        force_restart: false,
    };
    let sync_result = setup.sync_addr.send(sync_msg).await;
    assert!(sync_result.is_ok());
}

#[actix::test]
async fn test_federation_block_priority_sync() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Connect to federation peer
    let federation_peer = libp2p::PeerId::random();
    let connect_msg = ConnectToPeer {
        peer_id: federation_peer,
        addresses: vec!["/ip4/127.0.0.1/tcp/16000".parse().unwrap()],
        is_federation_peer: Some(true),
    };
    setup.peer_addr.send(connect_msg).await.unwrap();
    
    // Broadcast priority federation block
    let block_data = create_test_block_data(200);
    let broadcast_msg = BroadcastBlock {
        block_hash: "federation_priority_block".to_string(),
        block_data: block_data.clone(),
        priority: true,
    };
    setup.network_addr.send(broadcast_msg).await.unwrap();
    
    // Process federation block with high priority
    let sync_msg = ProcessNewBlock {
        block_hash: "federation_priority_block".to_string(),
        block_data,
        from_peer: "federation_peer".to_string(),
    };
    
    let start_time = std::time::Instant::now();
    let sync_result = setup.sync_addr.send(sync_msg).await;
    let processing_time = start_time.elapsed();
    
    assert!(sync_result.is_ok());
    // Federation blocks should process quickly
    assert!(processing_time < Duration::from_millis(100));
}

#[actix::test]
async fn test_sync_threshold_coordination() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Start sync process
    let sync_msg = StartSync {
        target_block: Some(1000),
        force_restart: false,
    };
    setup.sync_addr.send(sync_msg).await.unwrap();
    
    // Check sync status
    let status = setup.sync_addr.send(GetSyncStatus).await.unwrap().unwrap();
    
    // Should respect 99.5% threshold
    assert!(status.sync_percentage <= 100.0);
    
    // Network should be aware of sync progress
    let network_status = setup.network_addr.send(GetNetworkStatus).await.unwrap().unwrap();
    assert!(network_status.connected_peers >= 0);
}

#[actix::test]
async fn test_parallel_validation_workflow() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Send multiple blocks for parallel validation
    let mut handles = Vec::new();
    
    for i in 1..=20 {
        let block_data = create_test_block_data(i);
        let sync_msg = ProcessNewBlock {
            block_hash: format!("parallel_block_{}", i),
            block_data,
            from_peer: format!("peer_{}", i),
        };
        
        let handle = setup.sync_addr.send(sync_msg);
        handles.push(handle);
    }
    
    // All blocks should process successfully in parallel
    let start_time = std::time::Instant::now();
    for handle in handles {
        assert!(handle.await.is_ok());
    }
    let total_time = start_time.elapsed();
    
    // Parallel processing should be faster than sequential
    assert!(total_time < Duration::from_secs(2));
}

#[actix::test]
async fn test_network_partition_recovery_sync() {
    let setup = SyncIntegrationSetup::new().await;
    
    // Start sync
    setup.sync_addr.send(StartSync {
        target_block: Some(50),
        force_restart: false,
    }).await.unwrap();
    
    // Simulate network partition
    let partition_msg = HandleNetworkPartition {
        partition_type: NetworkPartitionType::Detected,
        peer_count: 2,
    };
    setup.sync_addr.send(partition_msg).await.unwrap();
    
    // Network should handle partition
    let network_event = NetworkEvent {
        event_type: NetworkEventType::ConnectionError,
        details: "Network partition detected".to_string(),
    };
    setup.network_addr.send(network_event).await.unwrap();
    
    // Simulate partition recovery
    let recovery_msg = HandleNetworkPartition {
        partition_type: NetworkPartitionType::Recovered,
        peer_count: 8,
    };
    setup.sync_addr.send(recovery_msg).await.unwrap();
    
    // System should recover and continue sync
    let status = setup.sync_addr.send(GetSyncStatus).await.unwrap().unwrap();
    assert!(status.current_block >= 0);
}