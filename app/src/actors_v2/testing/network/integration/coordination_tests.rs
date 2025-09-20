use crate::actors_v2::testing::network::{NetworkTestHarness, SyncTestHarness};
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::network::{NetworkMessage, SyncMessage};

#[actix::test]
async fn test_network_sync_actor_coordination() {
    let mut network_harness = NetworkTestHarness::new().await.unwrap();
    let mut sync_harness = SyncTestHarness::new().await.unwrap();

    network_harness.setup().await.unwrap();
    sync_harness.setup().await.unwrap();

    // Test that both actors can be created and configured
    assert!(network_harness.verify_state().await.is_ok());
    assert!(sync_harness.verify_state().await.is_ok());

    // Test basic message processing in both actors
    let network_msg = NetworkMessage::GetNetworkStatus;
    network_harness.send_message(network_msg).await.unwrap();

    let sync_msg = SyncMessage::GetSyncStatus;
    sync_harness.send_message(sync_msg).await.unwrap();

    network_harness.teardown().await.unwrap();
    sync_harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_peer_discovery_workflow() {
    let mut network_harness = NetworkTestHarness::new().await.unwrap();
    let mut sync_harness = SyncTestHarness::new().await.unwrap();

    network_harness.setup().await.unwrap();
    sync_harness.setup().await.unwrap();

    // Simulate peer discovery in NetworkActor
    let connect_msg = NetworkMessage::ConnectToPeer {
        peer_addr: "/ip4/127.0.0.1/tcp/8000".to_string(),
    };
    network_harness.send_message(connect_msg).await.unwrap();

    // Update peers in SyncActor
    let update_peers_msg = SyncMessage::UpdatePeers {
        peers: vec!["discovered-peer".to_string()],
    };
    sync_harness.send_message(update_peers_msg).await.unwrap();

    // Test sync block request
    let block_request_msg = SyncMessage::RequestBlocks {
        start_height: 100,
        count: 10,
        peer_id: Some("discovered-peer".to_string()),
    };
    sync_harness.send_message(block_request_msg).await.unwrap();

    network_harness.teardown().await.unwrap();
    sync_harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_block_sync_workflow() {
    let mut network_harness = NetworkTestHarness::new().await.unwrap();
    let mut sync_harness = SyncTestHarness::new().await.unwrap();

    network_harness.setup().await.unwrap();
    sync_harness.setup().await.unwrap();

    // Start sync process
    let start_sync_msg = SyncMessage::StartSync;
    sync_harness.send_message(start_sync_msg).await.unwrap();

    // Simulate block broadcast from network
    let block_broadcast_msg = NetworkMessage::BroadcastBlock {
        block_data: b"sync workflow test block".to_vec(),
        priority: false,
    };
    network_harness.send_message(block_broadcast_msg).await.unwrap();

    // Handle new block in sync
    let new_block_msg = SyncMessage::HandleNewBlock {
        block: b"sync workflow test block".to_vec(),
        peer_id: "sync-peer".to_string(),
    };
    sync_harness.send_message(new_block_msg).await.unwrap();

    // Stop sync process
    let stop_sync_msg = SyncMessage::StopSync;
    sync_harness.send_message(stop_sync_msg).await.unwrap();

    network_harness.teardown().await.unwrap();
    sync_harness.teardown().await.unwrap();
}