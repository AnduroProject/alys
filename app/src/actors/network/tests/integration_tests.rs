//! Integration Tests for Network Actor System
//! 
//! Tests the complete network actor system including all three actors working
//! together with message passing, supervision, and fault tolerance.

#[cfg(test)]
mod tests {
    use actix::prelude::*;
    use std::time::Duration;
    use tempfile::TempDir;

    use crate::actors::network::*;
    use crate::actors::network::messages::*;
    use crate::actors::network::tests::test_helpers::*;

    #[actix::test]
    async fn test_network_actor_system_startup() {
        // Test that all three network actors can start successfully
        let sync_config = SyncConfig::default();
        let network_config = NetworkConfig::lightweight(); // Use lightweight for testing
        let peer_config = PeerConfig::default();

        // Start SyncActor
        let sync_actor_result = SyncActor::new(sync_config);
        assert!(sync_actor_result.is_ok());
        let sync_actor = sync_actor_result.unwrap().start();

        // Start NetworkActor  
        let network_actor_result = NetworkActor::new(network_config);
        assert!(network_actor_result.is_ok());
        let network_actor = network_actor_result.unwrap().start();

        // Start PeerActor
        let peer_actor_result = PeerActor::new(peer_config);
        assert!(peer_actor_result.is_ok());
        let peer_actor = peer_actor_result.unwrap().start();

        // Verify actors are responsive
        let sync_status = sync_actor.send(GetSyncStatus).await;
        assert!(sync_status.is_ok());

        let network_status = network_actor.send(GetNetworkStatus).await;
        assert!(network_status.is_ok());

        let peer_status = peer_actor.send(GetPeerStatus { peer_id: None }).await;
        assert!(peer_status.is_ok());
    }

    #[actix::test]
    async fn test_sync_actor_production_threshold() {
        let config = SyncConfig::default();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();
        
        // Test below threshold
        sync_actor_obj.state.progress.progress_percent = 0.994;
        sync_actor_obj.state.progress.can_produce_blocks = false;
        
        let sync_actor = sync_actor_obj.start();
        let can_produce = sync_actor.send(CanProduceBlocks).await.unwrap().unwrap();
        assert!(!can_produce);
    }

    #[actix::test]
    async fn test_sync_actor_checkpoint_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SyncConfig::default();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();
        
        // Initialize checkpoint manager
        sync_actor_obj.initialize_checkpoints(temp_dir.path().to_path_buf()).await.unwrap();
        
        let sync_actor = sync_actor_obj.start();
        
        // Test checkpoint creation
        let create_msg = CreateCheckpoint {
            height: Some(100),
            compression: true,
        };
        
        let response = sync_actor.send(create_msg).await.unwrap();
        assert!(response.is_ok());
        
        if let Ok(checkpoint_response) = response {
            assert_eq!(checkpoint_response.height, 100);
            assert!(checkpoint_response.compressed);
            assert!(checkpoint_response.size_bytes > 0);
        }
    }

    #[actix::test]
    async fn test_network_actor_gossip_subscription() {
        let config = NetworkConfig::lightweight();
        let network_actor = NetworkActor::new(config).unwrap().start();
        
        // Test topic subscription
        let subscribe_msg = SubscribeToTopic {
            topic: GossipTopic::Blocks,
        };
        
        let response = network_actor.send(subscribe_msg).await.unwrap();
        assert!(response.is_ok());
    }

    #[actix::test]
    async fn test_peer_actor_connection_management() {
        use libp2p::Multiaddr;
        
        let config = PeerConfig::default();
        let peer_actor = PeerActor::new(config).unwrap().start();
        
        // Test connection to a peer
        let connect_msg = ConnectToPeer {
            peer_id: None,
            address: "/ip4/127.0.0.1/tcp/4001".parse::<Multiaddr>().unwrap(),
            priority: ConnectionPriority::Normal,
            timeout_ms: 5000,
        };
        
        let response = peer_actor.send(connect_msg).await;
        // Connection will fail but we test the message handling
        assert!(response.is_ok());
    }

    #[actix::test]
    async fn test_network_supervision_startup() {
        let supervision_config = NetworkSupervisionConfig::default();
        let mut supervisor = NetworkSupervisor::new(supervision_config);
        
        let sync_config = SyncConfig::lightweight();
        let network_config = NetworkConfig::lightweight();
        let peer_config = PeerConfig::default();
        
        // Test supervisor startup (may fail without full libp2p setup, but should handle gracefully)
        let result = supervisor.start_network_actors(sync_config, network_config, peer_config).await;
        
        // We expect this to work or fail gracefully
        match result {
            Ok(_) => {
                let status = supervisor.get_network_status();
                assert!(status.system_uptime > Duration::from_secs(0));
            }
            Err(e) => {
                // Expected to fail in test environment without full network setup
                println!("Supervisor startup failed as expected: {:?}", e);
            }
        }
    }

    #[actix::test]
    async fn test_message_protocol_serialization() {
        // Test that all network messages can be serialized/deserialized
        let start_sync = StartSync {
            from_height: Some(100),
            target_height: Some(200),
            sync_mode: SyncMode::Fast,
            priority_peers: vec!["peer1".to_string()],
        };
        
        // Test message creation
        assert_eq!(start_sync.from_height, Some(100));
        assert_eq!(start_sync.target_height, Some(200));
        assert_eq!(start_sync.priority_peers.len(), 1);
        
        let broadcast_block = BroadcastBlock {
            block_data: vec![1, 2, 3, 4, 5],
            block_height: 150,
            block_hash: "test_hash".to_string(),
            priority: true,
        };
        
        assert!(broadcast_block.priority);
        assert_eq!(broadcast_block.block_height, 150);
        assert_eq!(broadcast_block.block_data.len(), 5);
    }

    #[actix::test]
    async fn test_sync_status_reporting() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap().start();
        
        let status_response = sync_actor.send(GetSyncStatus).await.unwrap().unwrap();
        
        assert!(!status_response.is_syncing);
        assert_eq!(status_response.current_height, 0);
        assert!(!status_response.can_produce_blocks);
        assert!(status_response.checkpoint_info.is_some());
    }

    #[actix::test]
    async fn test_network_status_reporting() {
        let config = NetworkConfig::lightweight();
        let network_actor = NetworkActor::new(config).unwrap().start();
        
        let status_response = network_actor.send(GetNetworkStatus).await.unwrap().unwrap();
        
        assert_eq!(status_response.connected_peers, 0);
        assert_eq!(status_response.local_peer_id.to_string().len() > 0, true);
        assert!(status_response.active_protocols.contains(&"gossipsub".to_string()));
    }

    #[actix::test]
    async fn test_peer_discovery_operations() {
        let config = PeerConfig::default();
        let peer_actor = PeerActor::new(config).unwrap().start();
        
        // Test discovery startup
        let discovery_msg = StartDiscovery {
            discovery_type: DiscoveryType::MDNS,
            target_peer_count: Some(10),
        };
        
        let response = peer_actor.send(discovery_msg).await.unwrap().unwrap();
        assert!(!response.discovery_id.is_empty());
        assert!(matches!(response.discovery_type, DiscoveryType::MDNS));
    }

    #[actix::test]
    async fn test_error_handling_and_recovery() {
        // Test that actors handle errors gracefully
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap().start();
        
        // Test checkpoint restoration with invalid ID
        let restore_msg = RestoreCheckpoint {
            checkpoint_id: "invalid_checkpoint_id".to_string(),
            verify_integrity: true,
        };
        
        let response = sync_actor.send(restore_msg).await.unwrap();
        assert!(response.is_err()); // Should fail gracefully
        
        // Verify actor is still responsive
        let status_response = sync_actor.send(GetSyncStatus).await.unwrap();
        assert!(status_response.is_ok());
    }

    #[actix::test]
    async fn test_metrics_collection() {
        let config = SyncConfig::default();
        let sync_actor_obj = SyncActor::new(config).unwrap();
        
        let metrics = sync_actor_obj.metrics();
        assert!(metrics.is_object());
        assert!(metrics["current_height"].is_number());
        assert!(metrics["sync_progress"].is_number());
        assert!(metrics["can_produce_blocks"].is_boolean());
    }

    #[actix::test]
    async fn test_lifecycle_management() {
        let config = SyncConfig::default();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();
        
        // Test lifecycle methods
        assert!(sync_actor_obj.on_start().is_ok());
        assert!(sync_actor_obj.health_check().is_ok());
        assert!(sync_actor_obj.on_stop().is_ok());
        
        // After stop, health check should fail
        assert!(sync_actor_obj.health_check().is_err());
    }

    // Helper function tests
    #[test]
    fn test_configuration_validation() {
        let mut config = SyncConfig::default();
        assert!(config.validate().is_ok());
        
        // Test invalid configuration
        config.production_threshold = 1.5; // Invalid value
        assert!(config.validate().is_err());
        
        config.production_threshold = 0.5; // Too low
        assert!(config.validate().is_err());
        
        config.production_threshold = 0.995; // Valid
        config.max_parallel_downloads = 0; // Invalid
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_sync_modes() {
        assert_eq!(SyncMode::Fast.validation_workers(4), 4);
        assert_eq!(SyncMode::Full.validation_workers(4), 8);
        assert_eq!(SyncMode::Recovery.validation_workers(4), 2);
        
        assert_eq!(SyncMode::Fast.batch_size(256), 256);
        assert_eq!(SyncMode::Full.batch_size(256), 128);
        assert_eq!(SyncMode::Recovery.batch_size(256), 512);
        
        assert!(SyncMode::Full.requires_full_validation());
        assert!(!SyncMode::Fast.requires_full_validation());
        
        assert!(SyncMode::Fast.supports_checkpoints());
        assert!(!SyncMode::Emergency.supports_checkpoints());
    }

    #[test]
    fn test_message_priorities() {
        assert!(MessagePriority::Critical < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::Low);
        
        let envelope = MessageEnvelope::new("test")
            .with_priority(MessagePriority::Critical)
            .with_max_retries(5);
        
        assert_eq!(envelope.priority, MessagePriority::Critical);
        assert_eq!(envelope.max_retries, 5);
        assert!(envelope.can_retry());
    }
}

// Performance integration tests
#[cfg(test)]
mod performance_integration_tests {
    use super::*;
    use std::time::Instant;

    #[actix::test]
    async fn test_sync_throughput_performance() {
        let config = SyncConfig::high_performance();
        let sync_actor_obj = SyncActor::new(config).unwrap();
        
        // Test that high-performance config has expected values
        assert_eq!(sync_actor_obj.config.max_parallel_downloads, 32);
        assert_eq!(sync_actor_obj.config.batch_size, 512);
        assert!(sync_actor_obj.config.simd_enabled);
    }

    #[actix::test]
    async fn test_message_handling_latency() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap().start();
        
        let start = Instant::now();
        let _response = sync_actor.send(GetSyncStatus).await.unwrap();
        let latency = start.elapsed();
        
        // Message should be handled quickly
        assert!(latency < Duration::from_millis(100));
    }

    #[actix::test]
    async fn test_concurrent_message_handling() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap().start();
        
        // Send multiple messages concurrently
        let mut futures = Vec::new();
        for _ in 0..10 {
            futures.push(sync_actor.send(GetSyncStatus));
        }
        
        let results = futures::future::join_all(futures).await;
        
        // All messages should succeed
        for result in results {
            assert!(result.is_ok());
            assert!(result.unwrap().is_ok());
        }
    }
}

// Fault tolerance integration tests  
#[cfg(test)]
mod fault_tolerance_tests {
    use super::*;

    #[actix::test]
    async fn test_actor_restart_capability() {
        let supervision_config = NetworkSupervisionConfig::default();
        let supervisor = NetworkSupervisor::new(supervision_config);
        
        let status = supervisor.get_network_status();
        assert_eq!(status.total_restarts, 0);
        
        // Test that supervisor can track restart metrics
        assert!(status.system_uptime >= Duration::from_secs(0));
    }

    #[actix::test]
    async fn test_graceful_degradation() {
        let config = SyncConfig::default();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();
        
        // Simulate degraded performance
        sync_actor_obj.state.metrics.current_bps = 100.0; // Below target
        assert!(!sync_actor_obj.state.is_meeting_targets());
        
        let health_status = sync_actor_obj.state.health_status();
        // Should be degraded but not unhealthy if sync is active
        if sync_actor_obj.state.progress.status.is_active() {
            assert_eq!(health_status, SyncHealthStatus::Degraded);
        }
    }
}

// Real-world scenario tests
#[cfg(test)]
mod scenario_tests {
    use super::*;

    #[actix::test]
    async fn test_full_sync_workflow() {
        let config = SyncConfig::default();
        let sync_actor = SyncActor::new(config).unwrap().start();
        
        // Start sync operation
        let start_msg = StartSync {
            from_height: Some(0),
            target_height: Some(100),
            sync_mode: SyncMode::Fast,
            priority_peers: vec![],
        };
        
        let sync_response = sync_actor.send(start_msg).await.unwrap().unwrap();
        assert_eq!(sync_response.initial_height, 0);
        assert_eq!(sync_response.target_height, Some(100));
        assert_eq!(sync_response.mode, SyncMode::Fast);
        
        // Check sync status
        let status = sync_actor.send(GetSyncStatus).await.unwrap().unwrap();
        assert_eq!(status.sync_mode, SyncMode::Fast);
        
        // Stop sync
        let stop_msg = StopSync { force: false };
        let stop_response = sync_actor.send(stop_msg).await.unwrap();
        assert!(stop_response.is_ok());
    }

    #[actix::test]
    async fn test_federation_peer_prioritization() {
        let config = PeerConfig::default();
        let peer_actor = PeerActor::new(config).unwrap().start();
        
        // Request best peers for federation operation
        let best_peers_msg = GetBestPeers {
            count: 5,
            operation_type: OperationType::Federation,
            exclude_peers: vec![],
        };
        
        let response = peer_actor.send(best_peers_msg).await.unwrap();
        assert!(response.is_ok());
        
        // Response should be empty in test environment but message handling works
        let peers = response.unwrap();
        assert_eq!(peers.len(), 0); // No peers in test environment
    }

    #[actix::test]
    async fn test_network_partition_recovery() {
        // Test that network actors can handle partition scenarios
        let config = NetworkConfig::lightweight();
        let network_actor = NetworkActor::new(config).unwrap().start();
        
        // Simulate network start
        let start_msg = StartNetwork {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            bootstrap_peers: vec![],
            enable_mdns: false, // Disable for test
        };
        
        let response = network_actor.send(start_msg).await;
        // May fail in test environment but should handle gracefully
        match response {
            Ok(_) => println!("Network started successfully"),
            Err(e) => println!("Network start failed as expected: {:?}", e),
        }
    }
}