use crate::actors_v2::network::{SyncConfig, SyncMessage};
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::testing::network::{SyncTestError, SyncTestHarness};
use uuid::Uuid;

#[actix::test]
async fn test_sync_actor_creation_and_configuration() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test configuration validation
    assert!(harness.config.validate().is_ok());

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_sync_start_stop_operations() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test sync start
    let start_message = SyncMessage::StartSync { start_height: 0, target_height: None };
    harness.send_message(start_message).await.unwrap();

    // Test sync stop
    let stop_message = SyncMessage::StopSync;
    harness.send_message(stop_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_block_request_operations() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test block request with specific peer
    let request_message = SyncMessage::RequestBlocks {
        start_height: 100,
        count: 10,
        peer_id: Some("test-peer".to_string()),
    };

    harness.send_message(request_message).await.unwrap();

    // Test block request without specific peer
    let request_message_auto = SyncMessage::RequestBlocks {
        start_height: 200,
        count: 20,
        peer_id: None,
    };

    harness.send_message(request_message_auto).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_block_processing() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test new block handling
    let block_data = b"test block data for sync processing".to_vec();
    let new_block_message = SyncMessage::HandleNewBlock {
        block: block_data,
        peer_id: "source-peer".to_string(),
    };

    harness.send_message(new_block_message).await.unwrap();

    // Test block response handling
    let blocks_data = vec![
        b"block 1 data".to_vec(),
        b"block 2 data".to_vec(),
        b"block 3 data".to_vec(),
    ];

    let block_response_message = SyncMessage::HandleBlockResponse {
        blocks: blocks_data,
        request_id: Uuid::new_v4().to_string(),
        peer_id: "test-peer-1".to_string(),
    };

    harness.send_message(block_response_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_peer_management() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test peer updates
    let peers_message = SyncMessage::UpdatePeers {
        peers: vec![
            "peer-1".to_string(),
            "peer-2".to_string(),
            "peer-3".to_string(),
        ],
    };

    harness.send_message(peers_message).await.unwrap();

    // Test empty peer list
    let empty_peers_message = SyncMessage::UpdatePeers { peers: vec![] };

    harness.send_message(empty_peers_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_sync_status_and_metrics() {
    let mut harness = SyncTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test sync status request
    let status_message = SyncMessage::GetSyncStatus;
    harness.send_message(status_message).await.unwrap();

    // Test metrics request
    let metrics_message = SyncMessage::GetMetrics;
    harness.send_message(metrics_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_sync_configuration_validation() {
    // Test valid configuration
    let valid_config = SyncConfig::default();
    assert!(valid_config.validate().is_ok());

    let harness = SyncTestHarness::with_config(valid_config).await;
    assert!(harness.is_ok());

    // Test invalid configuration - zero max blocks per request
    let mut invalid_config = SyncConfig::default();
    invalid_config.max_blocks_per_request = 0;
    assert!(invalid_config.validate().is_err());

    // Test invalid configuration - zero max concurrent requests
    let mut invalid_config2 = SyncConfig::default();
    invalid_config2.max_concurrent_requests = 0;
    assert!(invalid_config2.validate().is_err());

    // Test invalid configuration - zero max sync peers
    let mut invalid_config3 = SyncConfig::default();
    invalid_config3.max_sync_peers = 0;
    assert!(invalid_config3.validate().is_err());
}

#[actix::test]
async fn test_sync_harness_lifecycle() {
    let mut harness = SyncTestHarness::new().await.unwrap();

    // Test setup
    assert!(harness.setup().await.is_ok());

    // Test state verification
    assert!(harness.verify_state().await.is_ok());

    // Test reset
    assert!(harness.reset().await.is_ok());

    // Test teardown
    assert!(harness.teardown().await.is_ok());
}
