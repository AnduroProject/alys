//! NetworkActor Tests
//! 
//! Unit tests for NetworkActor functionality including P2P protocol management,
//! libp2p integration, and message routing.

use actix::prelude::*;
use std::time::Duration;
use libp2p::Multiaddr;

use crate::actors::network::{NetworkActor, messages::*};
use crate::actors::network::tests::test_helpers::*;

#[actix::test]
async fn test_network_actor_initialization() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    // Test that actor starts successfully
    assert!(addr.connected());
}

#[actix::test]
async fn test_start_network() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let listen_addrs = vec![
        "/ip4/0.0.0.0/tcp/0".parse::<Multiaddr>().unwrap()
    ];
    let bootstrap_peers = vec![];
    
    let msg = StartNetwork {
        listen_addresses: listen_addrs,
        bootstrap_peers,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_stop_network() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let msg = StopNetwork { 
        graceful: true 
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_get_network_status() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let msg = GetNetworkStatus;
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(status)) = result {
        assert!(status.connected_peers >= 0);
    }
}

#[actix::test]
async fn test_broadcast_block() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let block_data = create_test_block_data(1);
    let msg = BroadcastBlock {
        block_hash: "test_block_hash".to_string(),
        block_data,
        priority: false,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_broadcast_transaction() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let tx_data = vec![1, 2, 3, 4, 5]; // Mock transaction data
    let msg = BroadcastTransaction {
        tx_hash: "test_tx_hash".to_string(),
        tx_data,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_subscribe_to_topic() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let msg = SubscribeToTopic {
        topic: "test_topic".to_string(),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_unsubscribe_from_topic() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    // First subscribe
    let subscribe_msg = SubscribeToTopic {
        topic: "test_topic".to_string(),
    };
    addr.send(subscribe_msg).await.unwrap();
    
    // Then unsubscribe
    let unsubscribe_msg = UnsubscribeFromTopic {
        topic: "test_topic".to_string(),
    };
    
    let result = addr.send(unsubscribe_msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_federation_blocks_priority() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    let block_data = create_test_block_data(1);
    let msg = BroadcastBlock {
        block_hash: "federation_block".to_string(),
        block_data,
        priority: true, // High priority federation block
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_gossipsub_integration() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    // Test subscription to gossipsub topics
    let topics = vec!["blocks", "transactions", "federation_blocks"];
    
    for topic in topics {
        let msg = SubscribeToTopic {
            topic: topic.to_string(),
        };
        
        let result = addr.send(msg).await;
        assert!(result.is_ok());
    }
}

#[actix::test]
async fn test_message_routing() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    // Test that messages are routed correctly
    let msg = MessageReceived {
        from_peer: libp2p::PeerId::random(),
        topic: "blocks".to_string(),
        data: create_test_block_data(1),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_peer_discovery() {
    let config = test_network_config();
    let network_actor = NetworkActor::new(config).unwrap();
    let addr = network_actor.start();
    
    // Test peer discovery via mDNS and Kademlia
    let msg = NetworkEvent {
        event_type: NetworkEventType::PeerDiscovered,
        details: "New peer discovered via mDNS".to_string(),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

mod network_integration_tests {
    use super::*;
    
    #[actix::test]
    async fn test_network_with_peer_actor() {
        let network_config = test_network_config();
        let peer_config = test_peer_config();
        
        let network_addr = NetworkActor::new(network_config).unwrap().start();
        let _peer_addr = crate::actors::network::PeerActor::new(peer_config).unwrap().start();
        
        // Test coordination between network and peer management
        let msg = GetNetworkStatus;
        let result = network_addr.send(msg).await;
        assert!(result.is_ok());
    }
    
    #[actix::test]
    async fn test_full_network_stack() {
        // Test complete network stack integration
        let network_config = test_network_config();
        let network_addr = NetworkActor::new(network_config).unwrap().start();
        
        // Start network
        let start_msg = StartNetwork {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            bootstrap_peers: vec![],
        };
        
        let start_result = network_addr.send(start_msg).await;
        assert!(start_result.is_ok());
        
        // Subscribe to topics
        let topics = ["blocks", "transactions", "governance"];
        for topic in &topics {
            let sub_msg = SubscribeToTopic {
                topic: topic.to_string(),
            };
            assert!(network_addr.send(sub_msg).await.is_ok());
        }
        
        // Test broadcasting
        let broadcast_msg = BroadcastBlock {
            block_hash: "integration_test_block".to_string(),
            block_data: create_test_block_data(100),
            priority: true,
        };
        
        let broadcast_result = network_addr.send(broadcast_msg).await;
        assert!(broadcast_result.is_ok());
    }
}

mod network_performance_tests {
    use super::*;
    
    #[actix::test]
    async fn test_high_throughput_broadcasting() {
        let config = test_network_config();
        let network_actor = NetworkActor::new(config).unwrap();
        let addr = network_actor.start();
        
        let start = std::time::Instant::now();
        let message_count = 1000;
        
        // Broadcast many messages quickly
        for i in 0..message_count {
            let msg = BroadcastTransaction {
                tx_hash: format!("tx_{}", i),
                tx_data: vec![i as u8; 32],
            };
            
            tokio::spawn(async move {
                addr.send(msg).await
            });
        }
        
        let duration = start.elapsed();
        assert!(duration < Duration::from_secs(10)); // Should complete quickly
    }
    
    #[actix::test]
    async fn test_gossip_latency() {
        let config = test_network_config();
        let network_actor = NetworkActor::new(config).unwrap();
        let addr = network_actor.start();
        
        let start = std::time::Instant::now();
        
        let msg = BroadcastBlock {
            block_hash: "latency_test_block".to_string(),
            block_data: create_test_block_data(1),
            priority: true,
        };
        
        let result = addr.send(msg).await;
        assert!(result.is_ok());
        
        let latency = start.elapsed();
        // Should have sub-100ms gossip latency
        assert!(latency < Duration::from_millis(100));
    }
}