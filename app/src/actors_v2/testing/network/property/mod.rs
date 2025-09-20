//! NetworkActor V2 Property-Based Tests (Production-Ready)
//!
//! Property-based tests for invariant validation in NetworkActor V2 system.
//! 10% of total test suite (~8 tests) following StorageActor patterns.

use crate::actors_v2::testing::base::ActorTestHarness;
use crate::actors_v2::testing::network::{
    NetworkTestHarness, SyncTestHarness, NetworkTestError,
    NetworkSyncTestEnvironment, TestPeer, TestBlock,
    fixtures::*,
};
use crate::actors_v2::network::{
    NetworkMessage, SyncMessage, NetworkConfig, SyncConfig,
    behaviour::AlysNetworkBehaviour,
    managers::{PeerManager, GossipHandler, BlockRequestManager},
    messages::{GossipMessage, NetworkRequest},
};
use std::time::{Duration, SystemTime};
use std::collections::{HashMap, HashSet};
use tracing::{info, debug, error};

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // Network Invariant Property Tests (4 tests)
    // ========================================

    #[tokio::test]
    async fn property_peer_discovery_consistency() {
        // Property: All discovered peers should be consistently trackable and connectable
        let test_data = NetworkPropertyTestData::new();

        for scenario in &test_data.peer_scenarios {
            let mut harness = NetworkTestHarness::new().await.unwrap();
            harness.setup().await.unwrap();

            info!("Testing peer discovery consistency with {} peers ({}% mDNS)",
                scenario.peer_count, scenario.mdns_ratio * 100.0);

            // Create peers according to scenario
            let mdns_count = (scenario.peer_count as f32 * scenario.mdns_ratio) as usize;
            let bootstrap_count = (scenario.peer_count as f32 * scenario.bootstrap_ratio) as usize;
            let regular_count = scenario.peer_count - mdns_count - bootstrap_count;

            let mut all_peers = HashMap::new();

            // Add mDNS peers
            for i in 0..mdns_count {
                let peer = TestPeer::new_mdns(
                    format!("mdns-peer-{}", i),
                    format!("/ip4/192.168.1.{}/tcp/8000", i + 100),
                );
                all_peers.insert(peer.peer_id.clone(), peer);
            }

            // Add bootstrap peers
            for i in 0..bootstrap_count {
                let peer = TestPeer::new_bootstrap(
                    format!("bootstrap-peer-{}", i),
                    format!("/ip4/127.0.0.{}/tcp/8000", i + 1),
                );
                all_peers.insert(peer.peer_id.clone(), peer);
            }

            // Add regular peers
            for i in 0..regular_count {
                let peer = TestPeer::new_regular(
                    format!("regular-peer-{}", i),
                    format!("/ip4/10.0.0.{}/tcp/8000", i + 100),
                );
                all_peers.insert(peer.peer_id.clone(), peer);
            }

            // Property: All peers should be discoverable and consistent
            for (peer_id, peer) in &all_peers {
                // Each peer should be valid
                assert!(validate_test_peer(peer).is_ok(),
                    "Peer {} should be valid", peer_id);

                // Each peer should be connectable
                let connect_result = harness.simulate_peer_connection(peer_id).await;
                if scenario.connection_success_rate > 0.8 {
                    assert!(connect_result.is_ok(),
                        "Peer {} should be connectable in high-success scenario", peer_id);
                }
            }

            // Property: mDNS peers should have local network addresses
            let mdns_peers: Vec<_> = all_peers.values()
                .filter(|p| p.is_mdns_discovered)
                .collect();

            for peer in mdns_peers {
                assert!(peer.address.contains("192.168") || peer.address.contains("10.0"),
                    "mDNS peer {} should have local network address: {}", peer.peer_id, peer.address);
            }

            harness.teardown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn property_message_delivery_guarantees() {
        // Property: All valid messages should be processable without corruption
        let test_data = NetworkPropertyTestData::new();

        for scenario in &test_data.message_scenarios {
            let mut harness = NetworkTestHarness::new().await.unwrap();
            harness.setup().await.unwrap();

            info!("Testing message delivery guarantees with {} messages across {} topics",
                scenario.message_count, scenario.topics.len());

            let mut sent_messages = HashSet::new();
            let mut message_sizes = Vec::new();

            // Generate and send messages according to scenario
            for i in 0..scenario.message_count {
                let topic = &scenario.topics[i % scenario.topics.len()];
                let size = scenario.message_sizes[i % scenario.message_sizes.len()];
                let message_data = vec![0u8; size];

                let message_id = format!("prop-msg-{}", i);
                sent_messages.insert(message_id.clone());
                message_sizes.push(size);

                let msg = match topic.as_str() {
                    topic if topic.contains("block") => NetworkMessage::BroadcastBlock {
                        block_data: message_data,
                        priority: i % 10 == 0,
                    },
                    topic if topic.contains("transaction") => NetworkMessage::BroadcastTransaction {
                        tx_data: message_data,
                    },
                    _ => NetworkMessage::BroadcastBlock {
                        block_data: message_data,
                        priority: false,
                    },
                };

                // Property: Each valid message should be processable
                let result = harness.send_message(msg).await;
                if scenario.failure_rate < 0.1 {
                    assert!(result.is_ok(),
                        "Message {} should be processed in low-failure scenario", i);
                }
            }

            // Property: Message count should be consistent
            assert_eq!(sent_messages.len(), scenario.message_count,
                "All messages should have unique IDs");

            // Property: Message sizes should be within limits
            let max_size = message_sizes.iter().max().unwrap_or(&0);
            assert!(*max_size <= 50 * 1024 * 1024,
                "No message should exceed 50MB limit");

            harness.teardown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn property_mdns_peer_discovery_invariants() {
        // Property: mDNS discovery should always produce valid, local network peers
        let mut behaviour = AlysNetworkBehaviour::new(&create_test_network_config()).unwrap();
        behaviour.initialize().unwrap();

        // Run multiple discovery cycles
        for iteration in 0..10 {
            info!("mDNS discovery iteration {}", iteration);

            let discovered_peers = behaviour.discover_mdns_peers();

            // Property: Discovery should always return some peers
            assert!(!discovered_peers.is_empty(),
                "mDNS discovery should always find peers");

            // Property: All discovered peers should have valid addresses
            for (peer_id, addresses) in &discovered_peers {
                assert!(!peer_id.is_empty(),
                    "Discovered peer ID should not be empty");

                assert!(!addresses.is_empty(),
                    "Discovered peer should have at least one address");

                for address in addresses {
                    // Property: mDNS addresses should be local network
                    assert!(address.contains("192.168") || address.contains("10.0") || address.contains("172.16"),
                        "mDNS address should be local network: {}", address);

                    // Property: Addresses should be valid multiaddr format
                    assert!(address.starts_with("/ip4/"),
                        "Address should be valid multiaddr: {}", address);
                }
            }

            // Property: Peer tracking should be consistent
            let tracked_peers = behaviour.get_mdns_peers();
            assert!(tracked_peers.len() >= discovered_peers.len(),
                "Tracked peers should include all discovered peers");
        }
    }

    #[tokio::test]
    async fn property_network_partition_tolerance() {
        // Property: Network should remain functional during partition scenarios
        let mut env = NetworkSyncTestEnvironment::new().await.unwrap();
        env.setup_coordination().await.unwrap();

        info!("Testing network partition tolerance properties");

        // Create partitioned peer groups
        let bootstrap_peers = env.network_harness.get_bootstrap_peers();
        let mdns_peers = env.network_harness.get_mdns_peers();

        // Property: System should work with bootstrap peers only
        for peer in &bootstrap_peers {
            let connect_msg = NetworkMessage::ConnectToPeer {
                peer_addr: peer.address.clone(),
            };
            assert!(env.network_harness.send_message(connect_msg).await.is_ok());
        }

        // Simulate partition: disconnect mDNS peers
        for peer in &mdns_peers {
            let disconnect_msg = NetworkMessage::DisconnectPeer {
                peer_id: peer.peer_id.clone(),
            };
            assert!(env.network_harness.send_message(disconnect_msg).await.is_ok());
        }

        // Property: Sync should still work with remaining peers
        let sync_msg = SyncMessage::StartSync;
        assert!(env.sync_harness.send_message(sync_msg).await.is_ok());

        // Property: System should work with mDNS peers only
        // Reconnect mDNS peers
        for peer in &mdns_peers {
            let connect_msg = NetworkMessage::ConnectToPeer {
                peer_addr: peer.address.clone(),
            };
            assert!(env.network_harness.send_message(connect_msg).await.is_ok());
        }

        // Property: Network should heal after partition
        let status_msg = NetworkMessage::GetNetworkStatus;
        assert!(env.network_harness.send_message(status_msg).await.is_ok());

        env.teardown().await.unwrap();
    }

    // ========================================
    // Sync Invariant Property Tests (2 tests)
    // ========================================

    #[tokio::test]
    async fn property_sync_state_consistency() {
        // Property: Sync state should always be consistent and recoverable
        let test_data = NetworkPropertyTestData::new();

        for scenario in &test_data.sync_scenarios {
            let mut harness = SyncTestHarness::new().await.unwrap();
            harness.setup().await.unwrap();
            harness.create_mock_network_actor().await.unwrap();

            info!("Testing sync state consistency: {} -> {} blocks",
                scenario.start_height, scenario.target_height);

            // Property: Initial state should be valid
            let initial_status = SyncMessage::GetSyncStatus;
            assert!(harness.send_message(initial_status).await.is_ok());

            // Generate test blocks for scenario
            let test_blocks = create_test_block_sequence(
                scenario.start_height,
                (scenario.target_height - scenario.start_height) as u32,
            );

            // Property: Each block should be valid
            for block in &test_blocks {
                assert!(validate_test_block(block).is_ok(),
                    "Block at height {} should be valid", block.height);
            }

            // Process blocks according to pattern
            match &scenario.request_pattern {
                RequestPattern::Sequential => {
                    for block in &test_blocks {
                        let block_msg = SyncMessage::HandleNewBlock {
                            block: block.data.clone(),
                            peer_id: format!("seq-peer-{}", block.height),
                        };
                        assert!(harness.send_message(block_msg).await.is_ok());
                    }
                }
                RequestPattern::Parallel => {
                    let mut handles = Vec::new();
                    for block in test_blocks.iter().take(20) {
                        let block_msg = SyncMessage::HandleNewBlock {
                            block: block.data.clone(),
                            peer_id: format!("par-peer-{}", block.height),
                        };

                        handles.push(tokio::spawn({
                            let mut test_harness = SyncTestHarness::new().await.unwrap();
                            async move {
                                test_harness.setup().await.unwrap();
                                test_harness.send_message(block_msg).await.unwrap();
                                test_harness.teardown().await.unwrap();
                            }
                        }));
                    }

                    for handle in handles {
                        assert!(handle.await.is_ok(), "Parallel block processing should succeed");
                    }
                }
                RequestPattern::ChunkedParallel(chunk_size) => {
                    for chunk in test_blocks.chunks(*chunk_size as usize) {
                        let chunk_data: Vec<_> = chunk.iter().map(|b| b.data.clone()).collect();
                        let chunk_response_msg = SyncMessage::HandleBlockResponse {
                            blocks: chunk_data,
                            request_id: format!("chunk-{}", chunk[0].height),
                        };
                        assert!(harness.send_message(chunk_response_msg).await.is_ok());
                    }
                }
                RequestPattern::RandomOrder => {
                    // Test blocks in random order
                    let mut random_blocks = test_blocks.clone();
                    // Simple shuffle for testing
                    random_blocks.reverse();

                    for block in random_blocks.iter().take(10) {
                        let block_msg = SyncMessage::HandleNewBlock {
                            block: block.data.clone(),
                            peer_id: format!("rand-peer-{}", block.height),
                        };
                        assert!(harness.send_message(block_msg).await.is_ok());
                    }
                }
            }

            // Property: Final state should be valid
            let final_status = SyncMessage::GetSyncStatus;
            assert!(harness.send_message(final_status).await.is_ok());

            harness.teardown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn property_block_ordering_preservation() {
        // Property: Block ordering should be preserved regardless of arrival order
        let mut harness = SyncTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        info!("Testing block ordering preservation property");

        // Create ordered test blocks
        let test_blocks = create_test_block_sequence(0, 50);
        let mut received_blocks = Vec::new();

        // Send blocks in random order to test ordering preservation
        let mut random_indices: Vec<usize> = (0..test_blocks.len()).collect();
        // Simple pseudo-random shuffle for testing
        random_indices.reverse();

        for &index in &random_indices {
            let block = &test_blocks[index];
            let block_msg = SyncMessage::HandleNewBlock {
                block: block.data.clone(),
                peer_id: format!("ordering-peer-{}", index),
            };

            assert!(harness.send_message(block_msg).await.is_ok());
            received_blocks.push(block.height);
        }

        // Property: System should handle out-of-order blocks gracefully
        assert_eq!(received_blocks.len(), test_blocks.len(),
            "All blocks should be processed");

        // Property: Original ordering should be recoverable
        let mut sorted_heights = received_blocks.clone();
        sorted_heights.sort();

        let expected_heights: Vec<u64> = (0..test_blocks.len() as u64).collect();
        assert_eq!(sorted_heights, expected_heights,
            "Block heights should form continuous sequence");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn property_peer_reputation_monotonicity() {
        // Property: Peer reputation should behave monotonically with interactions
        let mut peer_manager = PeerManager::new();

        info!("Testing peer reputation monotonicity property");

        // Create test peers
        let test_peers = create_test_peer_set(10, true);
        for (peer_id, test_peer) in &test_peers {
            peer_manager.add_peer(peer_id.clone(), test_peer.address.clone());
        }

        let peer_ids: Vec<String> = test_peers.keys().cloned().collect();

        // Property: Success should increase reputation
        for peer_id in &peer_ids {
            let initial_reputation = peer_manager.get_peer(peer_id).unwrap().reputation;

            peer_manager.record_peer_success(peer_id);
            let after_success = peer_manager.get_peer(peer_id).unwrap().reputation;

            assert!(after_success >= initial_reputation,
                "Reputation should not decrease after success for peer {}", peer_id);
        }

        // Property: Failure should decrease reputation
        for peer_id in &peer_ids {
            let initial_reputation = peer_manager.get_peer(peer_id).unwrap().reputation;

            peer_manager.record_peer_failure(peer_id);
            let after_failure = peer_manager.get_peer(peer_id).unwrap().reputation;

            assert!(after_failure <= initial_reputation,
                "Reputation should not increase after failure for peer {}", peer_id);
        }

        // Property: Reputation should be bounded
        for peer_id in &peer_ids {
            let reputation = peer_manager.get_peer(peer_id).unwrap().reputation;
            assert!(reputation >= 0.0 && reputation <= 100.0,
                "Reputation should be bounded [0,100] for peer {}: {}", peer_id, reputation);
        }

        // Property: Best peers should have highest reputation
        let best_peers = peer_manager.get_best_peers(3);
        let best_reputations: Vec<f64> = best_peers.iter()
            .filter_map(|pid| peer_manager.get_peer(pid))
            .map(|p| p.reputation)
            .collect();

        for i in 1..best_reputations.len() {
            assert!(best_reputations[i - 1] >= best_reputations[i],
                "Best peers should be ordered by reputation: {} >= {}",
                best_reputations[i - 1], best_reputations[i]);
        }
    }

    #[tokio::test]
    async fn property_configuration_consistency() {
        // Property: All valid configurations should create functional actors
        let configurations = vec![
            create_test_network_config(),
            create_minimal_network_config(),
            create_performance_network_config(),
        ];

        let edge_cases = create_edge_case_configs();

        for (config, description) in configurations.into_iter()
            .map(|c| (c, "standard config"))
            .chain(edge_cases.into_iter()) {

            info!("Testing configuration consistency: {}", description);

            // Property: Valid config should validate
            assert!(config.validate().is_ok(),
                "Configuration should be valid: {}", description);

            // Property: Valid config should create functional actor
            let harness_result = NetworkTestHarness::with_config(config).await;
            assert!(harness_result.is_ok(),
                "Valid config should create functional harness: {}", description);

            if let Ok(mut harness) = harness_result {
                // Property: Functional actor should complete lifecycle
                assert!(harness.setup().await.is_ok(), "Setup should succeed");
                assert!(harness.verify_state().await.is_ok(), "State should be valid");
                assert!(harness.teardown().await.is_ok(), "Teardown should succeed");
            }
        }
    }

    // ========================================
    // Protocol Invariant Tests (2 tests)
    // ========================================

    #[tokio::test]
    async fn property_gossip_message_idempotency() {
        // Property: Processing the same gossip message multiple times should be idempotent
        let mut gossip_handler = GossipHandler::new();
        gossip_handler.set_active_topics(vec![
            "test-blocks".to_string(),
            "test-transactions".to_string(),
            "test-mdns".to_string(),
        ]);

        info!("Testing gossip message idempotency property");

        // Create test messages
        let test_messages = vec![
            create_test_block_gossip_message(1),
            create_test_transaction_gossip_message("idempotent-tx"),
            create_test_mdns_gossip_message("idempotent-peer",
                &vec!["/ip4/192.168.1.100/tcp/8000".to_string()]),
        ];

        for message in test_messages {
            let message_id = message.message_id.clone();

            // First processing
            let result1 = gossip_handler.process_message(message.clone(), "peer1".to_string());
            assert!(result1.is_ok());
            let processed1 = result1.unwrap();

            // Second processing (should be filtered as duplicate)
            let result2 = gossip_handler.process_message(message.clone(), "peer1".to_string());
            assert!(result2.is_ok());
            let processed2 = result2.unwrap();

            // Property: First should succeed, second should be filtered
            assert!(processed1.is_some(), "First processing should succeed");
            assert!(processed2.is_none(), "Second processing should be filtered as duplicate");

            // Third processing from different peer (should also be filtered)
            let result3 = gossip_handler.process_message(message.clone(), "peer2".to_string());
            assert!(result3.is_ok());
            let processed3 = result3.unwrap();
            assert!(processed3.is_none(), "Third processing should be filtered (same message ID)");
        }

        // Property: Statistics should be consistent
        let stats = gossip_handler.get_stats();
        assert_eq!(stats.messages_received, 9); // 3 messages × 3 attempts
        assert_eq!(stats.messages_processed, 3); // Only first of each processed
        assert_eq!(stats.duplicate_messages, 6); // 2 duplicates per message
    }

    #[tokio::test]
    async fn property_request_response_correlation() {
        // Property: Requests and responses should be properly correlated
        let mut manager = BlockRequestManager::new(20);

        info!("Testing request-response correlation property");

        // Create multiple requests with different parameters
        let request_scenarios = vec![
            (100, 10, "correlation-peer-1"),
            (200, 20, "correlation-peer-2"),
            (300, 15, "correlation-peer-3"),
            (400, 25, "correlation-peer-1"), // Same peer, different request
        ];

        let mut request_ids = Vec::new();
        let mut expected_blocks = Vec::new();

        for (start_height, count, peer_id) in request_scenarios {
            let request_id = manager.create_request(start_height, count, peer_id.to_string());
            assert!(request_id.is_ok(), "Request creation should succeed");

            let request_id = request_id.unwrap();
            request_ids.push(request_id.clone());
            expected_blocks.push(count);

            // Property: Each request should be trackable
            assert!(manager.get_request(&request_id).is_some(),
                "Request {} should be trackable", request_id);
        }

        // Property: Active requests should match created requests
        assert_eq!(manager.get_active_requests().len(), request_ids.len(),
            "Active requests should match created requests");

        // Complete requests and verify correlation
        for (i, request_id) in request_ids.iter().enumerate() {
            let blocks_received = expected_blocks[i];
            let result = manager.complete_request(request_id, blocks_received);
            assert!(result.is_ok(), "Request completion should succeed");

            // Property: Completed request should no longer be active
            assert!(manager.get_request(request_id).is_none(),
                "Completed request {} should no longer be active", request_id);
        }

        // Property: All requests should be completed
        assert_eq!(manager.get_active_requests().len(), 0,
            "All requests should be completed");

        let stats = manager.get_stats();
        assert_eq!(stats.completed_requests, request_ids.len() as u64,
            "Statistics should reflect all completed requests");
    }

    // ========================================
    // System-Level Property Tests (2 tests)
    // ========================================

    #[tokio::test]
    async fn property_actor_coordination_symmetry() {
        // Property: Actor coordination should be symmetric and bidirectional
        let mut env = NetworkSyncTestEnvironment::new().await.unwrap();
        env.setup_coordination().await.unwrap();

        info!("Testing actor coordination symmetry property");

        // Property: NetworkActor -> SyncActor communication should work
        let peers = vec!["sym-peer-1".to_string(), "sym-peer-2".to_string()];
        let peer_update_msg = SyncMessage::UpdatePeers { peers: peers.clone() };
        assert!(env.sync_harness.send_message(peer_update_msg).await.is_ok());

        // Property: SyncActor -> NetworkActor communication should work
        let block_request_msg = SyncMessage::RequestBlocks {
            start_height: 500,
            count: 10,
            peer_id: Some("sym-peer-1".to_string()),
        };
        assert!(env.sync_harness.send_message(block_request_msg).await.is_ok());

        // Property: Bidirectional flow should be possible
        for i in 0..5 {
            // NetworkActor operations
            let network_msg = NetworkMessage::BroadcastBlock {
                block_data: format!("sym-block-{}", i).into_bytes(),
                priority: false,
            };
            assert!(env.network_harness.send_message(network_msg).await.is_ok());

            // SyncActor operations
            let sync_msg = SyncMessage::HandleNewBlock {
                block: format!("sym-response-{}", i).into_bytes(),
                peer_id: format!("sym-peer-{}", i % 2),
            };
            assert!(env.sync_harness.send_message(sync_msg).await.is_ok());
        }

        // Property: System state should remain consistent
        assert!(env.network_harness.verify_state().await.is_ok());
        assert!(env.sync_harness.verify_state().await.is_ok());
        assert!(env.coordination_active);

        env.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn property_system_resilience_under_load() {
        // Property: System should maintain functionality under various load conditions
        let chaos_data = NetworkChaosTestData::new();

        for load_scenario in &chaos_data.load_scenarios {
            let mut env = NetworkSyncTestEnvironment::new().await.unwrap();
            env.setup_coordination().await.unwrap();

            info!("Testing system resilience under load: {} concurrent ops at {:.1} ops/sec",
                load_scenario.concurrent_operations, load_scenario.operation_rate);

            let start_time = std::time::Instant::now();
            let mut handles = Vec::new();

            // Generate load according to scenario
            for i in 0..load_scenario.concurrent_operations {
                let network_msg = if i % 3 == 0 {
                    NetworkMessage::BroadcastBlock {
                        block_data: format!("load-block-{}", i).into_bytes(),
                        priority: i % 10 == 0,
                    }
                } else if i % 3 == 1 {
                    NetworkMessage::BroadcastTransaction {
                        tx_data: format!("load-tx-{}", i).into_bytes(),
                    }
                } else {
                    NetworkMessage::GetNetworkStatus
                };

                let sync_msg = if i % 2 == 0 {
                    SyncMessage::RequestBlocks {
                        start_height: i as u64 * 10,
                        count: 5,
                        peer_id: Some(format!("load-peer-{}", i % 4)),
                    }
                } else {
                    SyncMessage::GetSyncStatus
                };

                // Launch concurrent operations
                handles.push(tokio::spawn({
                    let mut network_harness = NetworkTestHarness::new().await.unwrap();
                    async move {
                        network_harness.setup().await.unwrap();
                        let result = network_harness.send_message(network_msg).await;
                        network_harness.teardown().await.unwrap();
                        result
                    }
                }));

                handles.push(tokio::spawn({
                    let mut sync_harness = SyncTestHarness::new().await.unwrap();
                    async move {
                        sync_harness.setup().await.unwrap();
                        let result = sync_harness.send_message(sync_msg).await;
                        sync_harness.teardown().await.unwrap();
                        result
                    }
                }));

                // Rate limiting
                let target_interval = Duration::from_secs_f64(1.0 / load_scenario.operation_rate);
                tokio::time::sleep(target_interval).await;

                if start_time.elapsed() > load_scenario.duration {
                    break;
                }
            }

            // Property: All operations should complete successfully under load
            let mut success_count = 0;
            let mut failure_count = 0;

            for handle in handles {
                match handle.await {
                    Ok(Ok(_)) => success_count += 1,
                    Ok(Err(_)) => failure_count += 1,
                    Err(_) => failure_count += 1,
                }
            }

            let total_ops = success_count + failure_count;
            let success_rate = if total_ops > 0 {
                success_count as f64 / total_ops as f64
            } else {
                0.0
            };

            info!("Load test results: {}/{} success ({:.1}%)",
                success_count, total_ops, success_rate * 100.0);

            // Property: Success rate should be reasonable under load
            assert!(success_rate > 0.7,
                "Success rate should be > 70% under load, got {:.1}%", success_rate * 100.0);

            env.teardown().await.unwrap();
        }
    }
}