//! Stream Actor Unit Tests
//! 
//! Comprehensive tests for StreamActor governance communication functionality

use actix::prelude::*;
use std::time::Duration;

use crate::actors::bridge::{
    StreamActor, StreamMessage, GovernanceMessage, ConsensusMessage,
    BridgeError, StreamMetrics
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_stream_actor_initialization() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    let result = stream_actor
        .send(StreamMessage::Initialize)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_establish_connection() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let peer_id = "test_peer_001".to_string();
    let endpoint = "ws://localhost:9944".to_string();

    let result = stream_actor
        .send(StreamMessage::EstablishConnection {
            peer_id,
            endpoint,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_send_governance_message() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let governance_msg = GovernanceMessage {
        msg_type: "proposal".to_string(),
        proposal_id: "prop_001".to_string(),
        data: serde_json::json!({"title": "Test Proposal", "description": "Test Description"}),
        timestamp: std::time::SystemTime::now(),
    };

    let result = stream_actor
        .send(StreamMessage::SendGovernanceMessage {
            message: governance_msg,
            target_peers: vec!["peer_001".to_string(), "peer_002".to_string()],
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_receive_governance_message() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let governance_msg = GovernanceMessage {
        msg_type: "vote".to_string(),
        proposal_id: "prop_001".to_string(),
        data: serde_json::json!({"vote": "yes", "voter": "federation_member_1"}),
        timestamp: std::time::SystemTime::now(),
    };

    let result = stream_actor
        .send(StreamMessage::ReceiveGovernanceMessage {
            message: governance_msg,
            from_peer: "peer_001".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_send_consensus_message() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let consensus_msg = ConsensusMessage {
        msg_type: "block_proposal".to_string(),
        block_hash: H256::random(),
        block_number: 12345,
        data: serde_json::json!({"proposer": "validator_1", "timestamp": 1234567890}),
    };

    let result = stream_actor
        .send(StreamMessage::SendConsensusMessage {
            message: consensus_msg,
            target_peers: vec!["peer_001".to_string()],
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_receive_consensus_message() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let consensus_msg = ConsensusMessage {
        msg_type: "block_finalization".to_string(),
        block_hash: H256::random(),
        block_number: 12345,
        data: serde_json::json!({"finalized": true, "signatures": ["sig1", "sig2"]}),
    };

    let result = stream_actor
        .send(StreamMessage::ReceiveConsensusMessage {
            message: consensus_msg,
            from_peer: "peer_002".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_subscribe_to_events() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let event_types = vec!["governance".to_string(), "consensus".to_string()];

    let result = stream_actor
        .send(StreamMessage::SubscribeToEvents {
            event_types,
            callback_addr: None, // No callback for testing
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_unsubscribe_from_events() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let event_types = vec!["governance".to_string()];

    let result = stream_actor
        .send(StreamMessage::UnsubscribeFromEvents {
            event_types,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_get_connection_status() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = stream_actor
        .send(StreamMessage::GetConnectionStatus)
        .await;

    assert!(result.is_ok());
    // Should return connection status information
}

#[actix::test]
async fn test_stream_actor_disconnect_peer() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let peer_id = "test_peer_001".to_string();

    let result = stream_actor
        .send(StreamMessage::DisconnectPeer {
            peer_id,
            reason: "Test disconnection".to_string(),
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_get_metrics() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = stream_actor
        .send(StreamMessage::GetMetrics)
        .await;

    assert!(result.is_ok());
    // Should return metrics data
}

#[actix::test]
async fn test_stream_actor_handle_connection_error() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let peer_id = "problematic_peer".to_string();
    let error_msg = "Connection timeout".to_string();

    let result = stream_actor
        .send(StreamMessage::HandleConnectionError {
            peer_id,
            error: error_msg,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_invalid_endpoint() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let peer_id = "test_peer_invalid".to_string();
    let invalid_endpoint = "invalid_endpoint_format".to_string();

    let result = stream_actor
        .send(StreamMessage::EstablishConnection {
            peer_id,
            endpoint: invalid_endpoint,
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    BridgeAssertions::assert_bridge_error_type(&response, "InvalidEndpoint");
}

#[actix::test]
async fn test_stream_actor_malformed_governance_message() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let malformed_msg = GovernanceMessage {
        msg_type: "".to_string(), // Empty type
        proposal_id: "".to_string(), // Empty proposal ID
        data: serde_json::json!({}),
        timestamp: std::time::SystemTime::now(),
    };

    let result = stream_actor
        .send(StreamMessage::SendGovernanceMessage {
            message: malformed_msg,
            target_peers: vec!["peer_001".to_string()],
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    BridgeAssertions::assert_bridge_error_type(&response, "InvalidMessage");
}

#[actix::test]
async fn test_stream_actor_duplicate_subscription() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let event_types = vec!["governance".to_string()];

    // Subscribe twice to the same event type
    let first_result = stream_actor
        .send(StreamMessage::SubscribeToEvents {
            event_types: event_types.clone(),
            callback_addr: None,
        })
        .await;

    let second_result = stream_actor
        .send(StreamMessage::SubscribeToEvents {
            event_types,
            callback_addr: None,
        })
        .await;

    assert!(first_result.is_ok());
    assert!(second_result.is_ok());

    // Second subscription should either succeed silently or be handled gracefully
    let second_response = second_result.unwrap();
    assert!(second_response.is_ok());
}

#[actix::test]
async fn test_stream_actor_broadcast_to_all_peers() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let governance_msg = GovernanceMessage {
        msg_type: "announcement".to_string(),
        proposal_id: "announce_001".to_string(),
        data: serde_json::json!({"message": "System maintenance scheduled"}),
        timestamp: std::time::SystemTime::now(),
    };

    let result = stream_actor
        .send(StreamMessage::SendGovernanceMessage {
            message: governance_msg,
            target_peers: vec![], // Empty means broadcast to all
        })
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}

#[actix::test]
async fn test_stream_actor_message_ordering() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    // Send multiple messages in sequence
    for i in 0..5 {
        let consensus_msg = ConsensusMessage {
            msg_type: "block_proposal".to_string(),
            block_hash: H256::random(),
            block_number: i,
            data: serde_json::json!({"sequence": i}),
        };

        let result = stream_actor
            .send(StreamMessage::SendConsensusMessage {
                message: consensus_msg,
                target_peers: vec!["peer_001".to_string()],
            })
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_ok());
    }
}

#[actix::test]
async fn test_stream_actor_shutdown() {
    let config = test_bridge_config();
    let stream_actor = StreamActor::new(config).start();

    // Initialize first
    stream_actor
        .send(StreamMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    let result = stream_actor
        .send(StreamMessage::Shutdown)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.is_ok());
}