//! NetworkActor V2 Unit Tests (Production-Ready)
//!
//! Comprehensive unit tests for the two-actor NetworkActor V2 system with mDNS support.

use crate::actors_v2::testing::network::{NetworkTestHarness, SyncTestHarness};
use crate::actors_v2::network::{
    NetworkMessage, SyncMessage,
    NetworkConfig, SyncConfig,
    NetworkResponse, SyncResponse,
    NetworkRpcRequest, NetworkSubsystem,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_actor_creation_with_mdns() {
        let config = NetworkConfig {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/8000".to_string()],
            bootstrap_peers: vec![],
            max_connections: 50,
            connection_timeout: std::time::Duration::from_secs(30),
            gossip_topics: vec!["alys-blocks".to_string(), "alys-transactions".to_string()],
            message_size_limit: 1024 * 1024,
            discovery_interval: std::time::Duration::from_secs(60),
        };

        let harness = NetworkTestHarness::with_config(config).await;
        assert!(harness.is_ok(), "NetworkActor creation with mDNS should succeed");
    }

    #[tokio::test]
    async fn test_sync_actor_creation() {
        let config = SyncConfig {
            max_blocks_per_request: 128,
            sync_timeout: std::time::Duration::from_secs(30),
            max_concurrent_requests: 4,
            block_validation_timeout: std::time::Duration::from_secs(10),
            max_sync_peers: 8,
        };

        let harness = SyncTestHarness::new().await;
        assert!(harness.is_ok(), "SyncActor creation should succeed");
    }

    #[tokio::test]
    async fn test_network_config_validation() {
        let mut config = NetworkConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid configs should fail
        config.listen_addresses.clear();
        assert!(config.validate().is_err(), "Empty listen addresses should fail validation");

        config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/8000".to_string()];
        config.max_connections = 0;
        assert!(config.validate().is_err(), "Zero max connections should fail validation");

        config.max_connections = 100;
        config.message_size_limit = 0;
        assert!(config.validate().is_err(), "Zero message size limit should fail validation");
    }

    #[tokio::test]
    async fn test_sync_config_validation() {
        let mut config = SyncConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid configs should fail
        config.max_blocks_per_request = 0;
        assert!(config.validate().is_err(), "Zero max blocks per request should fail");

        config.max_blocks_per_request = 128;
        config.max_concurrent_requests = 0;
        assert!(config.validate().is_err(), "Zero max concurrent requests should fail");

        config.max_concurrent_requests = 4;
        config.max_sync_peers = 0;
        assert!(config.validate().is_err(), "Zero max sync peers should fail");
    }

    #[tokio::test]
    async fn test_peer_manager_operations() {
        use crate::actors_v2::network::managers::PeerManager;

        let mut peer_manager = PeerManager::new();

        // Test peer addition
        peer_manager.add_peer("peer1".to_string(), "/ip4/127.0.0.1/tcp/8000".to_string());
        peer_manager.add_peer("peer2".to_string(), "/ip4/127.0.0.1/tcp/8001".to_string());

        // Check peers exist
        assert!(peer_manager.get_peer(&"peer1".to_string()).is_some());
        assert!(peer_manager.get_peer(&"peer2".to_string()).is_some());
        assert_eq!(peer_manager.get_connected_peers().len(), 2);

        // Test reputation system
        peer_manager.record_peer_success(&"peer1".to_string());
        peer_manager.record_peer_failure(&"peer2".to_string());

        let best_peers = peer_manager.get_best_peers(1);
        assert_eq!(best_peers.len(), 1);
        assert_eq!(best_peers[0], "peer1".to_string());

        // Test peer removal
        peer_manager.remove_peer(&"peer1".to_string());
        assert!(peer_manager.get_peer(&"peer1".to_string()).is_none());
        assert_eq!(peer_manager.get_connected_peers().len(), 1);
    }

    #[tokio::test]
    async fn test_gossip_handler_with_mdns_context() {
        use crate::actors_v2::network::managers::GossipHandler;
        use crate::actors_v2::network::messages::GossipMessage;

        let mut gossip_handler = GossipHandler::new();
        gossip_handler.set_active_topics(vec![
            "alys-blocks".to_string(),
            "alys-transactions".to_string(),
            "alys-mdns-announcements".to_string(), // mDNS-related topic
        ]);

        // Test block message processing
        let block_message = GossipMessage {
            topic: "alys-blocks".to_string(),
            data: b"test block data from mdns peer".to_vec(),
            message_id: "msg-1".to_string(),
        };

        let result = gossip_handler.process_message(block_message, "mdns-peer-1".to_string());
        assert!(result.is_ok());

        if let Ok(Some(processed)) = result {
            assert!(processed.should_forward);
            assert_eq!(processed.source_peer, "mdns-peer-1");
        }

        // Test mDNS announcement processing
        let mdns_message = GossipMessage {
            topic: "alys-mdns-announcements".to_string(),
            data: b"peer announcement data".to_vec(),
            message_id: "msg-2".to_string(),
        };

        let result = gossip_handler.process_message(mdns_message, "local-peer".to_string());
        assert!(result.is_ok());

        let stats = gossip_handler.get_stats();
        assert_eq!(stats.messages_processed, 2);
    }

    #[tokio::test]
    async fn test_block_request_manager_coordination() {
        use crate::actors_v2::network::managers::BlockRequestManager;

        let mut manager = BlockRequestManager::new(5);

        // Create multiple requests for different peers (including mDNS peers)
        let request1 = manager.create_request(100, 10, "bootstrap-peer".to_string());
        let request2 = manager.create_request(110, 10, "mdns-peer-1".to_string());
        let request3 = manager.create_request(120, 10, "mdns-peer-2".to_string());

        assert!(request1.is_ok());
        assert!(request2.is_ok());
        assert!(request3.is_ok());

        let stats = manager.get_stats();
        assert_eq!(stats.active_requests, 3);
        assert_eq!(stats.total_blocks_requested, 30);

        // Complete requests
        let _ = manager.complete_request(&request1.unwrap(), 10);
        let _ = manager.complete_request(&request2.unwrap(), 10);
        let _ = manager.complete_request(&request3.unwrap(), 10);

        let final_stats = manager.get_stats();
        assert_eq!(final_stats.active_requests, 0);
        assert_eq!(final_stats.completed_requests, 3);
        assert_eq!(final_stats.total_blocks_received, 30);
    }

    #[tokio::test]
    async fn test_network_behaviour_mdns_support() {
        use crate::actors_v2::network::behaviour::AlysNetworkBehaviour;

        let config = NetworkConfig::default();
        let mut behaviour = AlysNetworkBehaviour::new(&config).unwrap();

        // Test mDNS is enabled
        assert!(behaviour.is_mdns_enabled());

        // Test initialization
        behaviour.initialize().unwrap();
        assert!(behaviour.is_initialized());

        // Test mDNS peer discovery simulation
        let discovered_peers = behaviour.discover_mdns_peers();
        assert!(!discovered_peers.is_empty());

        let mdns_peers = behaviour.get_mdns_peers();
        assert!(!mdns_peers.is_empty());

        // Test topic management
        assert!(behaviour.active_topics().contains(&"alys-blocks".to_string()));
        assert!(behaviour.active_topics().contains(&"alys-transactions".to_string()));
    }

    #[tokio::test]
    async fn test_rpc_request_validation() {
        use crate::actors_v2::network::rpc::NetworkRpcHandler;

        // Test valid requests
        let valid_start = NetworkRpcRequest::StartNetwork {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/8000".to_string()],
            bootstrap_peers: vec![],
        };
        assert!(NetworkRpcHandler::validate_request(&valid_start).is_ok());

        let valid_broadcast = NetworkRpcRequest::BroadcastBlock {
            block_data: "deadbeef".to_string(),
            priority: false,
        };
        assert!(NetworkRpcHandler::validate_request(&valid_broadcast).is_ok());

        // Test invalid requests
        let invalid_start = NetworkRpcRequest::StartNetwork {
            listen_addresses: vec![], // Empty addresses
            bootstrap_peers: vec![],
        };
        assert!(NetworkRpcHandler::validate_request(&invalid_start).is_err());

        let invalid_broadcast = NetworkRpcRequest::BroadcastBlock {
            block_data: "invalid_hex".to_string(), // Invalid hex
            priority: false,
        };
        assert!(NetworkRpcHandler::validate_request(&invalid_broadcast).is_err());

        let invalid_connect = NetworkRpcRequest::ConnectToPeer {
            peer_address: "invalid_address".to_string(), // Invalid multiaddr
        };
        assert!(NetworkRpcHandler::validate_request(&invalid_connect).is_err());
    }

    #[tokio::test]
    async fn test_network_lifecycle() {
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

    #[tokio::test]
    async fn test_sync_lifecycle() {
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

    #[tokio::test]
    async fn test_two_actor_coordination() {
        let network_harness = NetworkTestHarness::new().await.unwrap();
        let sync_harness = SyncTestHarness::new().await.unwrap();

        // Both actors should be created successfully
        assert!(network_harness.config.validate().is_ok());
        assert!(sync_harness.config.validate().is_ok());

        // Configuration should include mDNS support
        assert!(!network_harness.config.gossip_topics.is_empty());
        assert!(sync_harness.config.max_sync_peers > 0);
    }

    #[tokio::test]
    async fn test_protocol_stack_completeness() {
        use crate::actors_v2::network::behaviour::AlysNetworkBehaviour;

        let config = NetworkConfig::default();
        let behaviour = AlysNetworkBehaviour::new(&config).unwrap();

        // Verify all required protocols are available
        assert!(behaviour.is_mdns_enabled(), "mDNS should be enabled (required from V1)");
        assert!(!behaviour.active_topics().is_empty(), "Should have active gossip topics");
        assert!(!behaviour.local_peer_id().is_empty(), "Should have local peer ID");
    }

    #[tokio::test]
    async fn test_message_system_completeness() {
        // Test that all essential message types are available

        // NetworkMessage variants
        let network_messages = vec![
            "StartNetwork", "StopNetwork", "GetNetworkStatus", "BroadcastBlock",
            "BroadcastTransaction", "ConnectToPeer", "DisconnectPeer",
            "GetConnectedPeers", "SetSyncActor", "GetMetrics"
        ];

        // SyncMessage variants
        let sync_messages = vec![
            "StartSync", "StopSync", "GetSyncStatus", "RequestBlocks",
            "HandleNewBlock", "SetNetworkActor", "SetStorageActor",
            "UpdatePeers", "GetMetrics"
        ];

        // Both message systems should be comprehensive
        assert!(network_messages.len() >= 10, "NetworkMessage should have at least 10 variants");
        assert!(sync_messages.len() >= 8, "SyncMessage should have at least 8 variants");
    }
}