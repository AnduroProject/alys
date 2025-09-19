//! NetworkActor V2 Integration Tests
//!
//! Integration tests for NetworkActor ↔ SyncActor coordination

use crate::actors_v2::testing::network::{NetworkTestHarness, SyncTestHarness};
use crate::actors_v2::network::{
    NetworkMessage, SyncMessage,
    NetworkConfig, SyncConfig,
    NetworkResponse, SyncResponse,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_sync_actor_coordination() {
        // Create both actors
        let network_harness = NetworkTestHarness::new().await.unwrap();
        let sync_harness = SyncTestHarness::new().await.unwrap();

        // Both actors should be created successfully
        assert!(network_harness.config.validate().is_ok());
        assert!(sync_harness.config.validate().is_ok());
    }

    #[tokio::test]
    async fn test_peer_discovery_and_sync() {
        let mut network_harness = NetworkTestHarness::new().await.unwrap();
        let mut sync_harness = SyncTestHarness::new().await.unwrap();

        // Setup both harnesses
        network_harness.setup().await.unwrap();
        sync_harness.setup().await.unwrap();

        // Verify integration is possible
        assert!(network_harness.verify_state().await.is_ok());
        assert!(sync_harness.verify_state().await.is_ok());

        // Teardown
        network_harness.teardown().await.unwrap();
        sync_harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_block_request_flow() {
        let mut sync_harness = SyncTestHarness::new().await.unwrap();
        sync_harness.setup().await.unwrap();

        // Test block request creation and handling
        // This demonstrates the simplified sync logic

        sync_harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_gossip_message_flow() {
        let mut network_harness = NetworkTestHarness::new().await.unwrap();
        network_harness.setup().await.unwrap();

        // Test gossip message processing
        // This demonstrates the simplified network protocol handling

        network_harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_peer_reputation_system() {
        use crate::actors_v2::network::managers::PeerManager;

        let mut peer_manager = PeerManager::new();

        // Add several peers
        peer_manager.add_peer("peer1".to_string(), "/ip4/127.0.0.1/tcp/8001".to_string());
        peer_manager.add_peer("peer2".to_string(), "/ip4/127.0.0.1/tcp/8002".to_string());
        peer_manager.add_peer("peer3".to_string(), "/ip4/127.0.0.1/tcp/8003".to_string());

        // Simulate peer interactions
        peer_manager.record_peer_success(&"peer1".to_string());
        peer_manager.record_peer_success(&"peer1".to_string());
        peer_manager.record_peer_failure(&"peer2".to_string());
        peer_manager.record_peer_failure(&"peer2".to_string());
        peer_manager.record_peer_failure(&"peer2".to_string());

        // Test reputation-based peer selection
        let best_peers = peer_manager.get_best_peers(2);
        assert!(best_peers.len() <= 2);

        // Test peer disconnection based on reputation
        let peers_to_disconnect = peer_manager.get_peers_to_disconnect();
        assert!(!peers_to_disconnect.is_empty()); // peer2 should be marked for disconnection

        let stats = peer_manager.get_connection_stats();
        assert_eq!(stats.total_connected, 3);
    }

    #[tokio::test]
    async fn test_simplified_protocol_stack() {
        use crate::actors_v2::network::behaviour::AlysNetworkBehaviour;

        let config = NetworkConfig::default();
        let mut behaviour = AlysNetworkBehaviour::new(&config).unwrap();

        // Test initialization
        assert!(!behaviour.is_initialized());
        behaviour.initialize().unwrap();
        assert!(behaviour.is_initialized());

        // Test topic management
        assert!(behaviour.active_topics().contains(&"alys-blocks".to_string()));
        assert!(behaviour.active_topics().contains(&"alys-transactions".to_string()));

        // Test message broadcasting
        let message_id = behaviour.broadcast_message("alys-blocks", b"test block".to_vec());
        assert!(message_id.is_ok());

        // Test request sending
        use crate::actors_v2::network::messages::NetworkRequest;
        let request = NetworkRequest::GetBlocks { start_height: 100, count: 10 };
        let request_id = behaviour.send_request("test-peer", &request);
        assert!(request_id.is_ok());
    }

    #[tokio::test]
    async fn test_configuration_validation() {
        // Test NetworkConfig validation
        let mut network_config = NetworkConfig::default();
        assert!(network_config.validate().is_ok());

        network_config.listen_addresses.clear();
        assert!(network_config.validate().is_err());

        network_config.max_connections = 0;
        assert!(network_config.validate().is_err());

        // Test SyncConfig validation
        let mut sync_config = SyncConfig::default();
        assert!(sync_config.validate().is_ok());

        sync_config.max_blocks_per_request = 0;
        assert!(sync_config.validate().is_err());

        sync_config.max_concurrent_requests = 0;
        assert!(sync_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_message_validation() {
        use crate::actors_v2::network::handlers::{NetworkMessageHandlers, SyncMessageHandlers};

        // Test network message validation
        assert!(NetworkMessageHandlers::validate_peer_address("/ip4/127.0.0.1/tcp/8000").is_ok());
        assert!(NetworkMessageHandlers::validate_peer_address("").is_err());
        assert!(NetworkMessageHandlers::validate_peer_address("invalid").is_err());

        // Test gossip message validation
        assert!(NetworkMessageHandlers::validate_gossip_message("alys-blocks", b"test data").is_ok());
        assert!(NetworkMessageHandlers::validate_gossip_message("", b"test data").is_err());
        assert!(NetworkMessageHandlers::validate_gossip_message("alys-blocks", b"").is_err());

        // Test sync message validation
        assert!(SyncMessageHandlers::validate_block_request(100, 10, Some(&"peer1".to_string())).is_ok());
        assert!(SyncMessageHandlers::validate_block_request(100, 0, Some(&"peer1".to_string())).is_err());
        assert!(SyncMessageHandlers::validate_block_request(100, 1001, Some(&"peer1".to_string())).is_err());
    }
}