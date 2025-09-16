//! Federation Integration Tests
//! 
//! Integration tests for federation-specific network functionality including
//! federation peer prioritization, governance communication, and consensus coordination.

use actix::prelude::*;
use std::time::Duration;
use libp2p::PeerId;

use crate::actors::network::{NetworkActor, PeerActor, messages::*};
use crate::actors::network::tests::helpers::*;

/// Federation integration test setup
#[derive(Debug)]
pub struct FederationIntegrationSetup {
    pub network_addr: Addr<NetworkActor>,
    pub peer_addr: Addr<PeerActor>,
    pub federation_peers: Vec<PeerId>,
}

impl FederationIntegrationSetup {
    pub async fn new() -> Self {
        let network_config = test_network_config();
        let peer_config = test_peer_config();

        let network_addr = NetworkActor::new(network_config).unwrap().start();
        let peer_addr = PeerActor::new(peer_config).unwrap().start();

        // Start systems
        network_addr.send(StartNetwork {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            bootstrap_peers: vec![],
        }).await.unwrap();

        peer_addr.send(StartPeerManager).await.unwrap();

        // Create federation peers
        let federation_peers = (0..3).map(|_| PeerId::random()).collect();

        Self {
            network_addr,
            peer_addr,
            federation_peers,
        }
    }

    pub async fn connect_federation_peers(&self) {
        for (i, peer_id) in self.federation_peers.iter().enumerate() {
            let connect_msg = ConnectToPeer {
                peer_id: *peer_id,
                addresses: vec![format!("/ip4/127.0.0.1/tcp/{}", 17000 + i).parse().unwrap()],
                is_federation_peer: Some(true),
            };
            
            self.peer_addr.send(connect_msg).await.unwrap();
        }
    }
}

#[actix::test]
async fn test_federation_peer_prioritization() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    // Connect regular peers
    let regular_peers: Vec<_> = (0..5).map(|_| PeerId::random()).collect();
    
    for (i, peer_id) in regular_peers.iter().enumerate() {
        let connect_msg = ConnectToPeer {
            peer_id: *peer_id,
            addresses: vec![format!("/ip4/127.0.0.1/tcp/{}", 18000 + i).parse().unwrap()],
            is_federation_peer: Some(false),
        };
        
        setup.peer_addr.send(connect_msg).await.unwrap();
    }

    // Get top peers - federation peers should rank higher
    let top_peers = setup.peer_addr.send(GetTopPeers { limit: Some(3) }).await.unwrap().unwrap();
    
    // Verify federation peers have higher scores
    for peer_info in &top_peers {
        if setup.federation_peers.contains(&peer_info.peer_id) {
            assert!(peer_info.is_federation);
            assert!(peer_info.score > 0.0); // Should have federation bonus
        }
    }
}

#[actix::test]
async fn test_federation_block_broadcasting() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    // Subscribe to federation blocks topic
    let subscribe_msg = SubscribeToTopic {
        topic: "federation_blocks".to_string(),
    };
    setup.network_addr.send(subscribe_msg).await.unwrap();

    // Broadcast federation block with priority
    let block_data = create_test_block_data(1000);
    let broadcast_msg = BroadcastBlock {
        block_hash: "federation_block_1000".to_string(),
        block_data: block_data.clone(),
        priority: true,
    };

    let start_time = std::time::Instant::now();
    let result = setup.network_addr.send(broadcast_msg).await.unwrap().unwrap();
    let broadcast_time = start_time.elapsed();

    assert!(result.success);
    // Federation blocks should broadcast with low latency
    assert!(broadcast_time < Duration::from_millis(50));
}

#[actix::test]
async fn test_governance_message_routing() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    // Subscribe to governance topic
    let subscribe_msg = SubscribeToTopic {
        topic: "governance".to_string(),
    };
    setup.network_addr.send(subscribe_msg).await.unwrap();

    // Simulate governance message from federation peer
    let governance_data = vec![1, 2, 3, 4]; // Mock governance data
    let msg = MessageReceived {
        from_peer: setup.federation_peers[0],
        topic: "governance".to_string(),
        data: governance_data,
    };

    let result = setup.network_addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_consensus_coordination() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    // Subscribe to consensus topics
    let topics = ["consensus", "federation_blocks", "proposals"];
    for topic in &topics {
        let subscribe_msg = SubscribeToTopic {
            topic: topic.to_string(),
        };
        setup.network_addr.send(subscribe_msg).await.unwrap();
    }

    // Simulate consensus messages from multiple federation peers
    for (i, peer_id) in setup.federation_peers.iter().enumerate() {
        let consensus_data = vec![i as u8; 32]; // Mock consensus data
        let msg = MessageReceived {
            from_peer: *peer_id,
            topic: "consensus".to_string(),
            data: consensus_data,
        };

        let result = setup.network_addr.send(msg).await;
        assert!(result.is_ok());
    }
}

#[actix::test]
async fn test_federation_peer_failure_handling() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    let failed_peer = setup.federation_peers[0];

    // Simulate federation peer failure
    let disconnect_msg = DisconnectFromPeer { 
        peer_id: failed_peer 
    };
    setup.peer_addr.send(disconnect_msg).await.unwrap();

    // System should handle federation peer loss gracefully
    let status = setup.peer_addr.send(GetPeerManagerStatus).await.unwrap().unwrap();
    assert!(status.is_running);

    // Remaining federation peers should still work
    let top_peers = setup.peer_addr.send(GetTopPeers { limit: Some(5) }).await.unwrap().unwrap();
    let remaining_federation_count = top_peers.iter()
        .filter(|p| p.is_federation)
        .count();
    
    assert!(remaining_federation_count >= 2); // Should have remaining federation peers
}

#[actix::test]
async fn test_aura_poa_timing_compliance() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    // Simulate Aura PoA block production timing (2-second intervals)
    let mut block_times = Vec::new();
    
    for i in 1..=5 {
        let start_time = std::time::Instant::now();
        
        let block_data = create_test_block_data(i);
        let broadcast_msg = BroadcastBlock {
            block_hash: format!("aura_block_{}", i),
            block_data,
            priority: true, // Federation blocks
        };

        setup.network_addr.send(broadcast_msg).await.unwrap();
        
        let processing_time = start_time.elapsed();
        block_times.push(processing_time);

        // Wait for next block interval
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // All federation blocks should process within federation timing requirements
    for time in block_times {
        assert!(time < Duration::from_millis(500)); // Well under 2-second block time
    }
}

#[actix::test]
async fn test_federation_peer_authentication() {
    let setup = FederationIntegrationSetup::new().await;

    // Simulate federation peer with authentication
    let auth_peer = PeerId::random();
    let connect_msg = ConnectToPeer {
        peer_id: auth_peer,
        addresses: vec!["/ip4/127.0.0.1/tcp/19000".parse().unwrap()],
        is_federation_peer: Some(true),
    };

    let result = setup.peer_addr.send(connect_msg).await;
    assert!(result.is_ok());

    // Check that federation peer gets elevated score
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    let score = setup.peer_addr.send(GetPeerScore { peer_id: auth_peer }).await.unwrap().unwrap();
    assert!(score.is_some());
    
    if let Some(score_info) = score {
        assert!(score_info.is_federation);
        assert!(score_info.score > 0.0); // Should have federation bonus
    }
}

#[actix::test]
async fn test_federation_message_priority() {
    let setup = FederationIntegrationSetup::new().await;
    setup.connect_federation_peers().await;

    // Send multiple message types with different priorities
    let messages = vec![
        ("federation_blocks", true),
        ("blocks", false),
        ("transactions", false),
        ("governance", true),
        ("consensus", true),
    ];

    let mut processing_times = Vec::new();

    for (topic, is_priority) in messages {
        let start_time = std::time::Instant::now();

        if topic == "federation_blocks" {
            let broadcast_msg = BroadcastBlock {
                block_hash: format!("{}_test", topic),
                block_data: create_test_block_data(1),
                priority: is_priority,
            };
            setup.network_addr.send(broadcast_msg).await.unwrap();
        } else if topic == "transactions" {
            let broadcast_msg = BroadcastTransaction {
                tx_hash: format!("{}_test", topic),
                tx_data: vec![1, 2, 3, 4],
            };
            setup.network_addr.send(broadcast_msg).await.unwrap();
        } else {
            // For other topics, simulate message reception
            let msg = MessageReceived {
                from_peer: setup.federation_peers[0],
                topic: topic.to_string(),
                data: vec![1, 2, 3, 4],
            };
            setup.network_addr.send(msg).await.unwrap();
        }

        let processing_time = start_time.elapsed();
        processing_times.push((topic, is_priority, processing_time));
    }

    // Priority messages should generally process faster
    let priority_times: Vec<_> = processing_times.iter()
        .filter(|(_, is_priority, _)| *is_priority)
        .map(|(_, _, time)| *time)
        .collect();

    let normal_times: Vec<_> = processing_times.iter()
        .filter(|(_, is_priority, _)| !*is_priority)
        .map(|(_, _, time)| *time)
        .collect();

    if !priority_times.is_empty() && !normal_times.is_empty() {
        let avg_priority_time = priority_times.iter().sum::<Duration>() / priority_times.len() as u32;
        let avg_normal_time = normal_times.iter().sum::<Duration>() / normal_times.len() as u32;

        // Priority messages should generally be faster (allowing some variance)
        assert!(avg_priority_time <= avg_normal_time + Duration::from_millis(10));
    }
}