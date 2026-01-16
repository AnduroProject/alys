use crate::actors_v2::network::managers::{BlockRequestManager, GossipHandler, PeerManager};
use crate::actors_v2::network::messages::GossipMessage;
use uuid::Uuid;

#[actix::test]
async fn test_peer_manager_basic_operations() {
    let mut peer_manager = PeerManager::new();

    // Test peer addition
    peer_manager.add_peer("peer-1".to_string(), "/ip4/127.0.0.1/tcp/8000".to_string());
    peer_manager.add_peer("peer-2".to_string(), "/ip4/127.0.0.1/tcp/8001".to_string());

    // Verify peers were added
    assert!(peer_manager.get_peer(&"peer-1".to_string()).is_some());
    assert!(peer_manager.get_peer(&"peer-2".to_string()).is_some());
    assert_eq!(peer_manager.get_connected_peers().len(), 2);

    // Test peer removal
    peer_manager.remove_peer(&"peer-1".to_string());
    assert!(peer_manager.get_peer(&"peer-1".to_string()).is_none());
    assert_eq!(peer_manager.get_connected_peers().len(), 1);
}

#[actix::test]
async fn test_peer_reputation_system() {
    let mut peer_manager = PeerManager::new();

    // Add test peers
    peer_manager.add_peer(
        "good-peer".to_string(),
        "/ip4/127.0.0.1/tcp/8000".to_string(),
    );
    peer_manager.add_peer(
        "bad-peer".to_string(),
        "/ip4/127.0.0.1/tcp/8001".to_string(),
    );

    // Record successes for good peer
    peer_manager.record_peer_success(&"good-peer".to_string());
    peer_manager.record_peer_success(&"good-peer".to_string());

    // Record failures for bad peer
    peer_manager.record_peer_failure(&"bad-peer".to_string());
    peer_manager.record_peer_failure(&"bad-peer".to_string());

    // Test best peer selection
    let best_peers = peer_manager.get_best_peers(1);
    assert_eq!(best_peers.len(), 1);
    assert_eq!(best_peers[0], "good-peer");

    // Test peer disconnection based on reputation
    let peers_to_disconnect = peer_manager.get_peers_to_disconnect();
    assert!(peers_to_disconnect.contains(&"bad-peer".to_string()));

    // Test connection statistics
    let stats = peer_manager.get_connection_stats();
    assert_eq!(stats.total_connected, 2);
    assert!(stats.average_reputation > 0.0);
}

#[actix::test]
async fn test_gossip_handler_message_processing() {
    let mut gossip_handler = GossipHandler::new();

    // Set active topics
    gossip_handler.set_active_topics(vec![
        "test-blocks".to_string(),
        "test-transactions".to_string(),
    ]);

    // Test block message processing (needs >= 100 bytes for validation)
    let block_data = vec![0u8; 150]; // 150 bytes - satisfies block validation requirement
    let block_message = GossipMessage {
        topic: "test-blocks".to_string(),
        data: block_data,
        message_id: Uuid::new_v4().to_string(),
    };

    let result = gossip_handler.process_message(block_message, "peer-1".to_string());
    assert!(
        result.is_ok(),
        "Block message processing should succeed: {:?}",
        result
    );

    // Test transaction message processing (needs >= 50 bytes for validation)
    let tx_data = vec![0u8; 60]; // 60 bytes - satisfies transaction validation requirement
    let tx_message = GossipMessage {
        topic: "test-transactions".to_string(),
        data: tx_data,
        message_id: Uuid::new_v4().to_string(),
    };

    let result = gossip_handler.process_message(tx_message, "peer-2".to_string());
    assert!(
        result.is_ok(),
        "Transaction message processing should succeed: {:?}",
        result
    );

    // Test message statistics
    let stats = gossip_handler.get_stats();
    assert_eq!(stats.messages_received, 2);
    assert_eq!(stats.messages_processed, 2);
}

#[actix::test]
async fn test_gossip_handler_duplicate_filtering() {
    let mut gossip_handler = GossipHandler::new();
    gossip_handler.set_active_topics(vec!["test-topic".to_string()]);

    let message_id = Uuid::new_v4().to_string();

    // First message should be processed
    let message1 = GossipMessage {
        topic: "test-topic".to_string(),
        data: b"duplicate test data".to_vec(),
        message_id: message_id.clone(),
    };

    let result1 = gossip_handler.process_message(message1, "peer-1".to_string());
    assert!(result1.is_ok());
    assert!(result1.unwrap().is_some()); // Should be processed

    // Duplicate message should be filtered
    let message2 = GossipMessage {
        topic: "test-topic".to_string(),
        data: b"duplicate test data".to_vec(),
        message_id: message_id.clone(),
    };

    let result2 = gossip_handler.process_message(message2, "peer-2".to_string());
    assert!(result2.is_ok());
    assert!(result2.unwrap().is_none()); // Should be filtered as duplicate

    // Verify statistics
    let stats = gossip_handler.get_stats();
    assert_eq!(stats.messages_received, 2);
    assert_eq!(stats.messages_processed, 1);
    assert_eq!(stats.duplicate_messages, 1);
}

#[actix::test]
async fn test_block_request_manager_operations() {
    let mut manager = BlockRequestManager::new(5);

    // Test request creation
    let request_id = manager.create_request(100, 10, "test-peer".to_string());
    assert!(request_id.is_ok());

    let request_id = request_id.unwrap();

    // Verify request tracking
    assert_eq!(manager.get_active_requests().len(), 1);
    assert!(manager.get_request(&request_id).is_some());

    // Test request completion
    let completion_result = manager.complete_request(&request_id, 10);
    assert!(completion_result.is_ok());

    // Verify request is no longer active
    assert_eq!(manager.get_active_requests().len(), 0);
    assert!(manager.get_request(&request_id).is_none());

    // Test statistics
    let stats = manager.get_stats();
    assert_eq!(stats.completed_requests, 1);
    assert_eq!(stats.total_blocks_received, 10);
}

#[actix::test]
async fn test_block_request_manager_timeout_handling() {
    let mut manager = BlockRequestManager::new(3);

    // Create multiple requests
    let request1 = manager
        .create_request(100, 5, "peer-1".to_string())
        .unwrap();
    let request2 = manager
        .create_request(200, 5, "peer-2".to_string())
        .unwrap();
    let request3 = manager
        .create_request(300, 5, "peer-3".to_string())
        .unwrap();

    assert_eq!(manager.get_active_requests().len(), 3);

    // Test timeout checking (should be empty for new requests)
    let timeouts = manager.check_timeouts();
    assert!(timeouts.is_empty());

    // Test request failure and retry
    let retry_result = manager.fail_request(&request1, "Peer timeout");
    assert!(retry_result.is_ok());

    // Complete remaining requests
    manager.complete_request(&request2, 5).unwrap();
    manager.complete_request(&request3, 3).unwrap();

    // Test final statistics
    let stats = manager.get_stats();
    assert!(stats.completed_requests >= 2);
    assert!(stats.total_blocks_received >= 8);
}

#[actix::test]
async fn test_block_request_manager_peer_coordination() {
    let mut manager = BlockRequestManager::new(10);

    // Create requests for different peers
    let peer1_request = manager
        .create_request(100, 10, "peer-1".to_string())
        .unwrap();
    let peer2_request = manager
        .create_request(200, 15, "peer-2".to_string())
        .unwrap();
    let peer1_request2 = manager
        .create_request(300, 5, "peer-1".to_string())
        .unwrap();

    // Test peer-specific request tracking
    let peer1_requests = manager.get_peer_requests(&"peer-1".to_string());
    assert_eq!(peer1_requests.len(), 2);

    let peer2_requests = manager.get_peer_requests(&"peer-2".to_string());
    assert_eq!(peer2_requests.len(), 1);

    // Test peer request cancellation
    let cancelled = manager.cancel_peer_requests(&"peer-1".to_string());
    assert_eq!(cancelled, 2);

    // Verify only peer-2 request remains
    assert_eq!(manager.get_active_requests().len(), 1);
    assert!(manager.get_request(&peer2_request).is_some());
    assert!(manager.get_request(&peer1_request).is_none());
    assert!(manager.get_request(&peer1_request2).is_none());
}
