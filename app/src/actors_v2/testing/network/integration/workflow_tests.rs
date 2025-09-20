use crate::actors_v2::testing::network::{NetworkTestHarness, SyncTestHarness};
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::network::{NetworkMessage, SyncMessage};
use uuid::Uuid;

#[actix::test]
async fn test_complete_network_startup_workflow() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Complete network startup workflow
    let start_msg = NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/0.0.0.0/tcp/8000".to_string()],
        bootstrap_peers: vec![
            "/ip4/127.0.0.1/tcp/9000".to_string(),
            "/ip4/127.0.0.1/tcp/9001".to_string(),
        ],
    };
    harness.send_message(start_msg).await.unwrap();

    // Test peer connections
    let connect_msg1 = NetworkMessage::ConnectToPeer {
        peer_addr: "/ip4/127.0.0.1/tcp/8001".to_string(),
    };
    harness.send_message(connect_msg1).await.unwrap();

    let connect_msg2 = NetworkMessage::ConnectToPeer {
        peer_addr: "/ip4/127.0.0.1/tcp/8002".to_string(),
    };
    harness.send_message(connect_msg2).await.unwrap();

    // Test message broadcasting
    let block_msg = NetworkMessage::BroadcastBlock {
        block_data: b"workflow test block".to_vec(),
        priority: false,
    };
    harness.send_message(block_msg).await.unwrap();

    let tx_msg = NetworkMessage::BroadcastTransaction {
        tx_data: b"workflow test transaction".to_vec(),
    };
    harness.send_message(tx_msg).await.unwrap();

    // Test network status
    let status_msg = NetworkMessage::GetNetworkStatus;
    harness.send_message(status_msg).await.unwrap();

    // Graceful shutdown
    let stop_msg = NetworkMessage::StopNetwork { graceful: true };
    harness.send_message(stop_msg).await.unwrap();

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_complete_sync_workflow() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Complete sync workflow
    let start_sync_msg = SyncMessage::StartSync;
    harness.send_message(start_sync_msg).await.unwrap();

    // Update peers for sync
    let update_peers_msg = SyncMessage::UpdatePeers {
        peers: vec![
            "sync-peer-1".to_string(),
            "sync-peer-2".to_string(),
            "sync-peer-3".to_string(),
        ],
    };
    harness.send_message(update_peers_msg).await.unwrap();

    // Request blocks from peers
    let block_request_msg = SyncMessage::RequestBlocks {
        start_height: 1000,
        count: 50,
        peer_id: Some("sync-peer-1".to_string()),
    };
    harness.send_message(block_request_msg).await.unwrap();

    // Handle incoming blocks
    for i in 0..5 {
        let block_data = format!("sync test block {}", i).into_bytes();
        let new_block_msg = SyncMessage::HandleNewBlock {
            block: block_data,
            peer_id: format!("sync-peer-{}", (i % 3) + 1),
        };
        harness.send_message(new_block_msg).await.unwrap();
    }

    // Handle block response
    let response_blocks = vec![
        b"response block 1".to_vec(),
        b"response block 2".to_vec(),
        b"response block 3".to_vec(),
    ];

    let block_response_msg = SyncMessage::HandleBlockResponse {
        blocks: response_blocks,
        request_id: Uuid::new_v4().to_string(),
    };
    harness.send_message(block_response_msg).await.unwrap();

    // Check sync status
    let status_msg = SyncMessage::GetSyncStatus;
    harness.send_message(status_msg).await.unwrap();

    // Get sync metrics
    let metrics_msg = SyncMessage::GetMetrics;
    harness.send_message(metrics_msg).await.unwrap();

    // Stop sync
    let stop_sync_msg = SyncMessage::StopSync;
    harness.send_message(stop_sync_msg).await.unwrap();

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_network_recovery_workflow() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Start network
    let start_msg = NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/0.0.0.0/tcp/8000".to_string()],
        bootstrap_peers: vec![],
    };
    harness.send_message(start_msg).await.unwrap();

    // Connect to peers
    let connect_msg = NetworkMessage::ConnectToPeer {
        peer_addr: "/ip4/127.0.0.1/tcp/8001".to_string(),
    };
    harness.send_message(connect_msg).await.unwrap();

    // Simulate network disruption (disconnect peer)
    let disconnect_msg = NetworkMessage::DisconnectPeer {
        peer_id: "test-peer".to_string(),
    };
    harness.send_message(disconnect_msg).await.unwrap();

    // Recovery: reconnect
    let reconnect_msg = NetworkMessage::ConnectToPeer {
        peer_addr: "/ip4/127.0.0.1/tcp/8001".to_string(),
    };
    harness.send_message(reconnect_msg).await.unwrap();

    // Verify network is functional after recovery
    let status_msg = NetworkMessage::GetNetworkStatus;
    harness.send_message(status_msg).await.unwrap();

    let stop_msg = NetworkMessage::StopNetwork { graceful: true };
    harness.send_message(stop_msg).await.unwrap();

    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_high_volume_message_processing() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Process multiple messages in sequence
    for i in 0..20 {
        let block_msg = NetworkMessage::BroadcastBlock {
            block_data: format!("volume test block {}", i).into_bytes(),
            priority: i % 5 == 0, // Every 5th block is priority
        };
        harness.send_message(block_msg).await.unwrap();

        let tx_msg = NetworkMessage::BroadcastTransaction {
            tx_data: format!("volume test tx {}", i).into_bytes(),
        };
        harness.send_message(tx_msg).await.unwrap();
    }

    // Verify system remains stable
    let status_msg = NetworkMessage::GetNetworkStatus;
    harness.send_message(status_msg).await.unwrap();

    harness.teardown().await.unwrap();
}