use crate::actors_v2::network::{NetworkConfig, NetworkMessage};
use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::testing::network::{NetworkTestError, NetworkTestHarness};
use uuid::Uuid;

#[actix::test]
async fn test_network_actor_creation_and_configuration() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test configuration validation
    assert!(harness.config.validate().is_ok());

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_network_start_stop_operations() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test network start
    let start_message = NetworkMessage::StartNetwork {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/8000".to_string()],
        bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
    };

    harness.send_message(start_message).await.unwrap();

    // Test network stop
    let stop_message = NetworkMessage::StopNetwork { graceful: true };

    harness.send_message(stop_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_block_broadcasting() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test regular block broadcast
    let block_message = NetworkMessage::BroadcastBlock {
        block_data: b"test block data".to_vec(),
        priority: false,
    };

    harness.send_message(block_message).await.unwrap();

    // Test priority block broadcast
    let priority_block_message = NetworkMessage::BroadcastBlock {
        block_data: b"priority block data".to_vec(),
        priority: true,
    };

    harness.send_message(priority_block_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_transaction_broadcasting() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test transaction broadcast
    let tx_message = NetworkMessage::BroadcastTransaction {
        tx_data: b"test transaction data".to_vec(),
    };

    harness.send_message(tx_message).await.unwrap();

    // Test large transaction
    let large_tx_data = vec![0u8; 10240]; // 10KB transaction
    let large_tx_message = NetworkMessage::BroadcastTransaction {
        tx_data: large_tx_data,
    };

    harness.send_message(large_tx_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_peer_connection_operations() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test peer connection
    let connect_message = NetworkMessage::ConnectToPeer {
        peer_addr: "/ip4/127.0.0.1/tcp/8001".to_string(),
    };

    harness.send_message(connect_message).await.unwrap();

    // Test peer disconnection
    let disconnect_message = NetworkMessage::DisconnectPeer {
        peer_id: "test-peer-1".to_string(),
    };

    harness.send_message(disconnect_message).await.unwrap();

    // Test multiple peer connections
    for i in 2..5 {
        let connect_msg = NetworkMessage::ConnectToPeer {
            peer_addr: format!("/ip4/127.0.0.{}/tcp/8000", i),
        };
        harness.send_message(connect_msg).await.unwrap();
    }

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_network_status_and_metrics() {
    let mut harness = NetworkTestHarness::new().await.unwrap();
    harness.setup().await.unwrap();

    // Test network status request
    let status_message = NetworkMessage::GetNetworkStatus;
    harness.send_message(status_message).await.unwrap();

    // Test connected peers request
    let peers_message = NetworkMessage::GetConnectedPeers;
    harness.send_message(peers_message).await.unwrap();

    // Test metrics request
    let metrics_message = NetworkMessage::GetMetrics;
    harness.send_message(metrics_message).await.unwrap();

    // Verify state consistency
    harness.verify_state().await.unwrap();
    harness.teardown().await.unwrap();
}

#[actix::test]
async fn test_network_configuration_validation() {
    // Test valid configuration
    let valid_config = NetworkConfig::default();
    assert!(valid_config.validate().is_ok());

    let harness = NetworkTestHarness::with_config(valid_config).await;
    assert!(harness.is_ok());

    // Test invalid configuration - empty listen addresses
    let mut invalid_config = NetworkConfig::default();
    invalid_config.listen_addresses.clear();
    assert!(invalid_config.validate().is_err());

    // Test invalid configuration - zero max connections
    let mut invalid_config2 = NetworkConfig::default();
    invalid_config2.max_connections = 0;
    assert!(invalid_config2.validate().is_err());

    // Test invalid configuration - zero message size limit
    let mut invalid_config3 = NetworkConfig::default();
    invalid_config3.message_size_limit = 0;
    assert!(invalid_config3.validate().is_err());
}

#[actix::test]
async fn test_network_harness_lifecycle() {
    let mut harness = NetworkTestHarness::new().await.unwrap();

    // Test setup
    assert!(harness.setup().await.is_ok());

    // Test state verification
    assert!(harness.verify_state().await.is_ok());

    // Test reset
    assert!(harness.reset().await.is_ok());

    // Test teardown
    assert!(harness.teardown().await.is_ok());
}
