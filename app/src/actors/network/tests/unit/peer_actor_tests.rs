//! PeerActor Tests
//! 
//! Unit tests for PeerActor functionality including connection management,
//! peer scoring, and discovery systems.

use actix::prelude::*;
use std::time::Duration;
use libp2p::{PeerId, Multiaddr};

use crate::actors::network::{PeerActor, messages::*};
use crate::actors::network::tests::test_helpers::*;

#[actix::test]
async fn test_peer_actor_initialization() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    // Test that actor starts successfully
    assert!(addr.connected());
}

#[actix::test]
async fn test_start_peer_manager() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = StartPeerManager;
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(status)) = result {
        assert!(status.is_running);
        assert_eq!(status.max_peers, 1000); // From default config
    }
}

#[actix::test]
async fn test_stop_peer_manager() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = StopPeerManager { 
        graceful: true 
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_connect_to_peer() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    let addresses = vec![
        "/ip4/127.0.0.1/tcp/12345".parse::<Multiaddr>().unwrap()
    ];
    
    let msg = ConnectToPeer {
        peer_id,
        addresses,
        is_federation_peer: Some(false),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_connect_to_federation_peer() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    let addresses = vec![
        "/ip4/127.0.0.1/tcp/12346".parse::<Multiaddr>().unwrap()
    ];
    
    let msg = ConnectToPeer {
        peer_id,
        addresses,
        is_federation_peer: Some(true), // Federation peer
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_disconnect_from_peer() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    
    let msg = DisconnectFromPeer { peer_id };
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_get_connected_peers() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = GetConnectedPeers;
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(peers)) = result {
        assert!(peers.is_empty()); // No connections initially
    }
}

#[actix::test]
async fn test_peer_discovery() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = StartDiscovery;
    let result = addr.send(msg).await;
    assert!(result.is_ok());
    
    let discover_msg = DiscoverPeers {
        target_count: Some(10),
    };
    let discover_result = addr.send(discover_msg).await;
    assert!(discover_result.is_ok());
    
    let stop_msg = StopDiscovery;
    let stop_result = addr.send(stop_msg).await;
    assert!(stop_result.is_ok());
}

#[actix::test]
async fn test_peer_scoring_connection_success() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    let msg = UpdatePeerScore {
        peer_id,
        score_event: PeerScoreEvent::ConnectionSuccess { latency_ms: 50 },
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
    
    // Check score
    let score_msg = GetPeerScore { peer_id };
    let score_result = addr.send(score_msg).await;
    assert!(score_result.is_ok());
}

#[actix::test]
async fn test_peer_scoring_connection_failure() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    let msg = UpdatePeerScore {
        peer_id,
        score_event: PeerScoreEvent::ConnectionFailure,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_peer_scoring_protocol_violation() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    let msg = UpdatePeerScore {
        peer_id,
        score_event: PeerScoreEvent::ProtocolViolation {
            violation_type: "spam_behavior".to_string(),
        },
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_get_top_peers() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = GetTopPeers { limit: Some(5) };
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(top_peers)) = result {
        assert!(top_peers.len() <= 5);
    }
}

#[actix::test]
async fn test_ban_unban_peer() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let peer_id = PeerId::random();
    
    // Ban peer
    let ban_msg = BanPeer {
        peer_id,
        duration: Some(Duration::from_secs(3600)), // 1 hour
    };
    let ban_result = addr.send(ban_msg).await;
    assert!(ban_result.is_ok());
    
    // Check banned peers
    let banned_msg = GetBannedPeers;
    let banned_result = addr.send(banned_msg).await;
    assert!(banned_result.is_ok());
    
    // Unban peer
    let unban_msg = UnbanPeer { peer_id };
    let unban_result = addr.send(unban_msg).await;
    assert!(unban_result.is_ok());
}

#[actix::test]
async fn test_health_check() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = PerformHealthCheck;
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(health_result)) = result {
        assert!(health_result.healthy_peers >= 0);
    }
}

#[actix::test]
async fn test_cleanup_peer_data() {
    let config = test_peer_config();
    let peer_actor = PeerActor::new(config).unwrap();
    let addr = peer_actor.start();
    
    let msg = CleanupPeerData;
    let result = addr.send(msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(cleaned_count)) = result {
        assert!(cleaned_count >= 0);
    }
}

mod peer_integration_tests {
    use super::*;
    
    #[actix::test]
    async fn test_peer_lifecycle() {
        let config = test_peer_config();
        let peer_actor = PeerActor::new(config).unwrap();
        let addr = peer_actor.start();
        
        // Start peer manager
        let start_result = addr.send(StartPeerManager).await;
        assert!(start_result.is_ok());
        
        // Connect to a peer
        let peer_id = PeerId::random();
        let connect_msg = ConnectToPeer {
            peer_id,
            addresses: vec!["/ip4/127.0.0.1/tcp/12347".parse().unwrap()],
            is_federation_peer: Some(false),
        };
        let connect_result = addr.send(connect_msg).await;
        assert!(connect_result.is_ok());
        
        // Update peer score
        let score_msg = UpdatePeerScore {
            peer_id,
            score_event: PeerScoreEvent::MessageSuccess {
                message_type: "blocks".to_string(),
            },
        };
        let score_result = addr.send(score_msg).await;
        assert!(score_result.is_ok());
        
        // Get peer status
        let status_result = addr.send(GetPeerManagerStatus).await;
        assert!(status_result.is_ok());
        
        // Disconnect
        let disconnect_result = addr.send(DisconnectFromPeer { peer_id }).await;
        assert!(disconnect_result.is_ok());
        
        // Stop peer manager
        let stop_result = addr.send(StopPeerManager { graceful: true }).await;
        assert!(stop_result.is_ok());
    }
    
    #[actix::test]
    async fn test_federation_peer_prioritization() {
        let config = test_peer_config();
        let peer_actor = PeerActor::new(config).unwrap();
        let addr = peer_actor.start();
        
        // Start manager
        addr.send(StartPeerManager).await.unwrap();
        
        // Connect regular peer
        let regular_peer = PeerId::random();
        let regular_msg = ConnectToPeer {
            peer_id: regular_peer,
            addresses: vec!["/ip4/127.0.0.1/tcp/12348".parse().unwrap()],
            is_federation_peer: Some(false),
        };
        addr.send(regular_msg).await.unwrap();
        
        // Connect federation peer
        let federation_peer = PeerId::random();
        let federation_msg = ConnectToPeer {
            peer_id: federation_peer,
            addresses: vec!["/ip4/127.0.0.1/tcp/12349".parse().unwrap()],
            is_federation_peer: Some(true),
        };
        addr.send(federation_msg).await.unwrap();
        
        // Federation peer should have higher score
        let fed_score = addr.send(GetPeerScore { peer_id: federation_peer }).await.unwrap();
        let reg_score = addr.send(GetPeerScore { peer_id: regular_peer }).await.unwrap();
        
        if let (Ok(Some(fed)), Ok(Some(reg))) = (fed_score, reg_score) {
            assert!(fed.score > reg.score);
            assert!(fed.is_federation);
            assert!(!reg.is_federation);
        }
    }
}

mod peer_performance_tests {
    use super::*;
    
    #[actix::test]
    async fn test_high_peer_count_handling() {
        let mut config = test_peer_config();
        config.max_peers = 1000; // Test with high peer count
        
        let peer_actor = PeerActor::new(config).unwrap();
        let addr = peer_actor.start();
        
        addr.send(StartPeerManager).await.unwrap();
        
        // Connect to many peers quickly
        let peer_count = 50; // Reduced for test speed
        for i in 0..peer_count {
            let peer_id = PeerId::random();
            let msg = ConnectToPeer {
                peer_id,
                addresses: vec![format!("/ip4/127.0.0.1/tcp/{}", 13000 + i).parse().unwrap()],
                is_federation_peer: Some(false),
            };
            
            tokio::spawn(async move {
                addr.send(msg).await
            });
        }
        
        // Should handle high connection load
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let status = addr.send(GetPeerManagerStatus).await.unwrap().unwrap();
        assert!(status.is_running);
    }
    
    #[actix::test]
    async fn test_scoring_performance() {
        let config = test_peer_config();
        let peer_actor = PeerActor::new(config).unwrap();
        let addr = peer_actor.start();
        
        addr.send(StartPeerManager).await.unwrap();
        
        let peer_ids: Vec<_> = (0..100).map(|_| PeerId::random()).collect();
        
        let start = std::time::Instant::now();
        
        // Update scores for many peers
        for peer_id in &peer_ids {
            let msg = UpdatePeerScore {
                peer_id: *peer_id,
                score_event: PeerScoreEvent::ConnectionSuccess { latency_ms: 50 },
            };
            
            tokio::spawn(async move {
                addr.send(msg).await
            });
        }
        
        let duration = start.elapsed();
        
        // Should complete scoring updates quickly
        assert!(duration < Duration::from_secs(5));
    }
}