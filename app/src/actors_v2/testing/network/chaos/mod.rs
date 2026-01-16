//! NetworkActor V2 Chaos Tests (Production-Ready)
//!
//! Chaos engineering tests for NetworkActor V2 system resilience.
//! 5% of total test suite (~6 tests) following StorageActor patterns.

use crate::actors_v2::testing::base::{ActorTestHarness, ChaosTestable};
use crate::actors_v2::testing::chaos::{FailureInjector, ChaosScenario};
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
use async_trait::async_trait;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;
use rand::{thread_rng, Rng};
use uuid::Uuid;
use tracing::{info, debug, error};

/// Chaos test configuration for NetworkActor V2 system
#[derive(Debug, Clone)]
pub struct NetworkChaosConfig {
    /// Duration to run chaos tests
    pub test_duration: Duration,
    /// Frequency of failure injection
    pub failure_rate: f64,
    /// Maximum number of concurrent operations
    pub max_concurrent_ops: usize,
    /// Enable different types of chaos
    pub enable_network_chaos: bool,
    pub enable_peer_churn: bool,
    pub enable_message_loss: bool,
    pub enable_slow_network: bool,
    /// Recovery timeout after failures
    pub recovery_timeout: Duration,
}

impl Default for NetworkChaosConfig {
    fn default() -> Self {
        Self {
            test_duration: Duration::from_secs(30),
            failure_rate: 0.1, // 10% failure rate
            max_concurrent_ops: 10,
            enable_network_chaos: true,
            enable_peer_churn: true,
            enable_message_loss: true,
            enable_slow_network: true,
            recovery_timeout: Duration::from_secs(5),
        }
    }
}

/// NetworkActor chaos test implementation
#[async_trait]
impl ChaosTestable for NetworkTestHarness {
    type ChaosConfig = NetworkChaosConfig;

    async fn run_chaos_test(&mut self, config: Self::ChaosConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting NetworkActor chaos test");
        let start_time = std::time::Instant::now();

        // Initialize test environment
        self.setup().await?;

        // Create test peers and messages
        let test_peers = create_test_peer_set(10, true);
        let test_messages = (0..50).map(|i| {
            if i % 2 == 0 {
                NetworkMessage::BroadcastBlock {
                    block_data: format!("chaos-block-{}", i).into_bytes(),
                    priority: i % 10 == 0,
                }
            } else {
                NetworkMessage::BroadcastTransaction {
                    tx_data: format!("chaos-tx-{}", i).into_bytes(),
                }
            }
        }).collect::<Vec<_>>();

        // Create failure injector
        let mut injector = FailureInjector::new();
        if config.enable_network_chaos {
            injector.add_chaos(Box::new(crate::actors_v2::testing::chaos::NetworkChaos::new(config.failure_rate)));
        }

        let mut operation_count = 0;
        let mut successful_operations = 0;
        let mut failed_operations = 0;

        // Run chaos operations
        while start_time.elapsed() < config.test_duration {
            let mut handles = Vec::new();

            // Launch concurrent operations
            for i in 0..config.max_concurrent_ops {
                let message = &test_messages[i % test_messages.len()];
                let should_inject_failure = thread_rng().gen::<f64>() < config.failure_rate;

                if should_inject_failure {
                    // Inject failure before operation
                    if let Err(e) = injector.inject_failure().await {
                        error!("Failed to inject chaos: {}", e);
                    }

                    // Simulate specific chaos types
                    if config.enable_peer_churn && thread_rng().gen_bool(0.3) {
                        self.simulate_peer_churn().await?;
                    }

                    if config.enable_message_loss && thread_rng().gen_bool(0.2) {
                        self.simulate_message_loss().await?;
                    }

                    if config.enable_slow_network && thread_rng().gen_bool(0.4) {
                        self.simulate_slow_network().await?;
                    }
                }

                let handle = tokio::spawn({
                    let mut harness_clone = NetworkTestHarness::new().await?;
                    let message_clone = message.clone();
                    async move {
                        let result = harness_clone.send_message(message_clone).await;
                        (result.is_ok(), result.is_err())
                    }
                });

                handles.push(handle);
            }

            // Wait for operations to complete
            for handle in handles {
                match handle.await {
                    Ok((success, failure)) => {
                        operation_count += 1;
                        if success {
                            successful_operations += 1;
                        } else if failure {
                            failed_operations += 1;
                        }
                    }
                    Err(e) => {
                        error!("Concurrent operation panicked: {}", e);
                        failed_operations += 1;
                    }
                }
            }

            // Recovery period
            if failed_operations > 0 {
                info!("Recovery pause after {} failures", failed_operations);
                sleep(config.recovery_timeout).await;
            }

            // Small delay between batches
            sleep(Duration::from_millis(100)).await;
        }

        // Verify system recovery
        info!("Chaos test completed. Verifying system recovery...");
        self.verify_state().await.map_err(|e| format!("System failed to recover: {}", e))?;

        // Report results
        let success_rate = successful_operations as f64 / operation_count as f64;
        info!("NetworkActor chaos test results:");
        info!("  Total operations: {}", operation_count);
        info!("  Successful: {} ({:.2}%)", successful_operations, success_rate * 100.0);
        info!("  Failed: {} ({:.2}%)", failed_operations, (failed_operations as f64 / operation_count as f64) * 100.0);
        info!("  Duration: {:?}", start_time.elapsed());

        // Ensure minimum success rate
        if success_rate < 0.7 {
            return Err(format!("Success rate too low: {:.2}%", success_rate * 100.0).into());
        }

        self.teardown().await?;
        Ok(())
    }

    async fn inject_failure(&mut self, scenario: ChaosScenario) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match scenario {
            ChaosScenario::NetworkPartition => {
                info!("Injecting network partition");
                self.simulate_network_partition().await?;
            }
            ChaosScenario::DiskFailure => {
                info!("Injecting disk I/O failure");
                // Network actors don't directly use disk, but may affect logging
                sleep(Duration::from_millis(100)).await;
            }
            ChaosScenario::MemoryPressure => {
                info!("Injecting memory pressure");
                let _memory_hog: Vec<Vec<u8>> = (0..1000).map(|_| vec![0u8; 1024]).collect();
                sleep(Duration::from_millis(100)).await;
            }
            ChaosScenario::ProcessCrash => {
                info!("Simulating process crash recovery");
                self.reset().await.map_err(|e| format!("Failed to reset after crash: {}", e))?;
            }
            ChaosScenario::SlowOperation => {
                info!("Injecting operation slowdown");
                sleep(Duration::from_millis(1000)).await;
            }
        }
        Ok(())
    }
}

/// SyncActor chaos test implementation
#[async_trait]
impl ChaosTestable for SyncTestHarness {
    type ChaosConfig = NetworkChaosConfig;

    async fn run_chaos_test(&mut self, config: Self::ChaosConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting SyncActor chaos test");
        let start_time = std::time::Instant::now();

        // Initialize test environment
        self.setup().await?;
        self.create_mock_network_actor().await?;

        // Create test blocks for chaos testing
        let test_blocks = create_chaos_test_blocks(100, true);

        let mut operation_count = 0;
        let mut successful_operations = 0;
        let mut failed_operations = 0;

        // Run chaos operations
        while start_time.elapsed() < config.test_duration {
            let mut handles = Vec::new();

            for i in 0..config.max_concurrent_ops {
                let block = &test_blocks[i % test_blocks.len()];
                let should_inject_failure = thread_rng().gen::<f64>() < config.failure_rate;

                let sync_msg = match i % 4 {
                    0 => SyncMessage::HandleNewBlock {
                        block: block.data.clone(),
                        peer_id: format!("chaos-peer-{}", i % 3),
                    },
                    1 => SyncMessage::RequestBlocks {
                        start_height: (i * 10) as u64,
                        count: 5,
                        peer_id: Some(format!("chaos-peer-{}", i % 3)),
                    },
                    2 => SyncMessage::GetSyncStatus,
                    _ => SyncMessage::GetMetrics,
                };

                if should_inject_failure {
                    // Inject various failure types
                    self.inject_failure(ChaosScenario::SlowOperation).await?;
                }

                let handle = tokio::spawn({
                    let mut harness_clone = SyncTestHarness::new().await?;
                    async move {
                        let result = harness_clone.send_message(sync_msg).await;
                        (result.is_ok(), result.is_err())
                    }
                });

                handles.push(handle);
            }

            // Process results
            for handle in handles {
                match handle.await {
                    Ok((success, failure)) => {
                        operation_count += 1;
                        if success {
                            successful_operations += 1;
                        } else if failure {
                            failed_operations += 1;
                        }
                    }
                    Err(e) => {
                        error!("Concurrent operation panicked: {}", e);
                        failed_operations += 1;
                    }
                }
            }

            // Recovery period
            if failed_operations > 0 {
                sleep(config.recovery_timeout).await;
            }

            sleep(Duration::from_millis(50)).await;
        }

        // Verify system recovery
        self.verify_state().await.map_err(|e| format!("SyncActor failed to recover: {}", e))?;

        // Report results
        let success_rate = successful_operations as f64 / operation_count as f64;
        info!("SyncActor chaos test results:");
        info!("  Total operations: {}", operation_count);
        info!("  Successful: {} ({:.2}%)", successful_operations, success_rate * 100.0);
        info!("  Failed: {} ({:.2}%)", failed_operations, (failed_operations as f64 / operation_count as f64) * 100.0);

        // Ensure minimum success rate
        if success_rate < 0.6 {
            return Err(format!("SyncActor success rate too low: {:.2}%", success_rate * 100.0).into());
        }

        self.teardown().await?;
        Ok(())
    }

    async fn inject_failure(&mut self, scenario: ChaosScenario) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match scenario {
            ChaosScenario::NetworkPartition => {
                info!("Injecting network partition for sync");
                // Simulate network issues affecting sync
                sleep(Duration::from_millis(500)).await;
            }
            ChaosScenario::MemoryPressure => {
                info!("Injecting memory pressure for sync");
                let _memory_hog: Vec<Vec<u8>> = (0..500).map(|_| vec![0u8; 2048]).collect();
                sleep(Duration::from_millis(200)).await;
            }
            ChaosScenario::ProcessCrash => {
                info!("Simulating sync process crash recovery");
                self.reset().await.map_err(|e| format!("Failed to reset sync after crash: {}", e))?;
            }
            ChaosScenario::SlowOperation => {
                info!("Injecting sync operation slowdown");
                sleep(Duration::from_millis(800)).await;
            }
            _ => {
                // Other scenarios less relevant to sync
                sleep(Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }
}

impl NetworkTestHarness {
    /// Simulate network partition
    async fn simulate_network_partition(&mut self) -> Result<(), NetworkTestError> {
        info!("Simulating network partition");

        // Disconnect half of the peers
        let peer_ids: Vec<String> = self.test_peers.keys().cloned().collect();
        let partition_count = peer_ids.len() / 2;

        for peer_id in peer_ids.iter().take(partition_count) {
            self.simulate_peer_disconnection(peer_id).await?;
        }

        // Wait for partition to take effect
        sleep(Duration::from_millis(500)).await;

        // Reconnect peers (healing)
        for peer_id in peer_ids.iter().take(partition_count) {
            self.simulate_peer_connection(peer_id).await?;
        }

        info!("Network partition simulation complete");
        Ok(())
    }

    /// Simulate peer churn
    async fn simulate_peer_churn(&mut self) -> Result<(), NetworkTestError> {
        info!("Simulating peer churn");

        let peer_ids: Vec<String> = self.test_peers.keys().cloned().collect();
        let churn_count = std::cmp::max(1, peer_ids.len() / 4);

        // Disconnect random peers
        for i in 0..churn_count {
            let peer_id = &peer_ids[i % peer_ids.len()];
            self.simulate_peer_disconnection(peer_id).await?;
        }

        // Add new peers
        for i in 0..churn_count {
            let new_peer_id = format!("churn-peer-{}", i);
            let new_peer = TestPeer::new_regular(
                new_peer_id.clone(),
                format!("/ip4/10.1.0.{}/tcp/8000", i + 100),
            );
            self.test_peers.insert(new_peer_id.clone(), new_peer);
            self.simulate_peer_connection(&new_peer_id).await?;
        }

        info!("Peer churn simulation complete");
        Ok(())
    }

    /// Simulate message loss
    async fn simulate_message_loss(&mut self) -> Result<(), NetworkTestError> {
        info!("Simulating message loss");

        // Create a message that will be "lost"
        let lost_msg = NetworkMessage::BroadcastBlock {
            block_data: b"lost message".to_vec(),
            priority: false,
        };

        // Simulate message loss by introducing delay and potential failure
        sleep(Duration::from_millis(200)).await;

        // Try to send message (may fail due to simulated loss)
        let _ = self.send_message(lost_msg).await; // Ignore result for chaos test

        info!("Message loss simulation complete");
        Ok(())
    }

    /// Simulate slow network conditions
    async fn simulate_slow_network(&mut self) -> Result<(), NetworkTestError> {
        info!("Simulating slow network conditions");

        // Add artificial delays to operations
        sleep(Duration::from_millis(1000)).await;

        info!("Slow network simulation complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // Network Chaos Tests (3 tests)
    // ========================================

    #[tokio::test]
    async fn test_network_partition_resilience() {
        let mut harness = NetworkTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        info!("Testing network partition resilience");

        // Inject network partition
        let result = harness.inject_failure(ChaosScenario::NetworkPartition).await;
        assert!(result.is_ok(), "Network partition injection should succeed");

        // Verify system can still operate after partition
        let test_msg = NetworkMessage::BroadcastBlock {
            block_data: b"partition test block".to_vec(),
            priority: true,
        };

        let result = harness.send_message(test_msg).await;
        assert!(result.is_ok(), "Network operation should succeed after partition");

        // Verify system recovery
        assert!(harness.verify_state().await.is_ok(), "System should recover from partition");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_high_peer_churn_handling() {
        let mut harness = NetworkTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        info!("Testing high peer churn handling");

        let initial_peer_count = harness.test_peers.len();

        // Simulate multiple rounds of peer churn
        for round in 0..5 {
            info!("Peer churn round {}", round);

            harness.simulate_peer_churn().await.unwrap();

            // Verify system remains functional
            let status_msg = NetworkMessage::GetNetworkStatus;
            assert!(harness.send_message(status_msg).await.is_ok(),
                "Network should remain functional during peer churn round {}", round);

            // Brief pause between churn rounds
            sleep(Duration::from_millis(200)).await;
        }

        // Verify final state
        assert!(harness.verify_state().await.is_ok(),
            "System should be stable after peer churn");

        // Should have some peers (may be different from initial)
        assert!(!harness.test_peers.is_empty(),
            "Should have peers after churn");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_message_loss_and_recovery() {
        let mut harness = NetworkTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        info!("Testing message loss and recovery");

        let message_count = 20;
        let mut successful_messages = 0;
        let mut lost_messages = 0;

        // Send messages with simulated loss
        for i in 0..message_count {
            let msg = NetworkMessage::BroadcastBlock {
                block_data: format!("loss-test-block-{}", i).into_bytes(),
                priority: i % 5 == 0,
            };

            // Randomly simulate message loss
            if thread_rng().gen_bool(0.3) {
                harness.simulate_message_loss().await.unwrap();
                lost_messages += 1;
            }

            let result = harness.send_message(msg).await;
            if result.is_ok() {
                successful_messages += 1;
            }
        }

        info!("Message loss test: {}/{} successful ({} simulated losses)",
            successful_messages, message_count, lost_messages);

        // System should handle message loss gracefully
        assert!(harness.verify_state().await.is_ok(),
            "System should remain stable despite message loss");

        harness.teardown().await.unwrap();
    }

    // ========================================
    // Sync Chaos Tests (2 tests)
    // ========================================

    #[tokio::test]
    async fn test_sync_under_network_instability() {
        let chaos_config = NetworkChaosConfig {
            test_duration: Duration::from_secs(15), // Shorter for test
            failure_rate: 0.2, // 20% failure rate
            max_concurrent_ops: 5,
            enable_network_chaos: true,
            enable_peer_churn: true,
            enable_message_loss: false, // Focus on network issues
            enable_slow_network: false,
            recovery_timeout: Duration::from_secs(2),
        };

        let mut harness = SyncTestHarness::new().await.unwrap();
        let result = harness.run_chaos_test(chaos_config).await;
        assert!(result.is_ok(), "SyncActor should handle network instability: {:?}", result);
    }

    #[tokio::test]
    async fn test_concurrent_sync_operations_under_stress() {
        let mut harness = SyncTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();
        harness.create_mock_network_actor().await.unwrap();

        info!("Testing concurrent sync operations under stress");

        let stress_duration = Duration::from_secs(10);
        let start_time = std::time::Instant::now();
        let mut handles = Vec::new();

        // Generate stress load
        while start_time.elapsed() < stress_duration {
            let operations_batch = 15; // High concurrency

            for i in 0..operations_batch {
                // Inject chaos randomly
                if thread_rng().gen_bool(0.3) {
                    let _ = harness.inject_failure(ChaosScenario::SlowOperation).await;
                }

                let sync_msg = match i % 3 {
                    0 => SyncMessage::RequestBlocks {
                        start_height: (i * 20) as u64,
                        count: 10,
                        peer_id: Some(format!("stress-peer-{}", i % 4)),
                    },
                    1 => SyncMessage::HandleNewBlock {
                        block: format!("stress-block-{}", i).into_bytes(),
                        peer_id: format!("stress-source-{}", i % 3),
                    },
                    _ => SyncMessage::GetSyncStatus,
                };

                let handle = tokio::spawn({
                    let mut stress_harness = SyncTestHarness::new().await.unwrap();
                    async move {
                        stress_harness.setup().await.unwrap();
                        let result = stress_harness.send_message(sync_msg).await;
                        stress_harness.teardown().await.unwrap();
                        result
                    }
                });

                handles.push(handle);
            }

            // Brief pause between batches
            sleep(Duration::from_millis(100)).await;
        }

        // Wait for all stress operations to complete
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

        info!("Stress test results: {}/{} success ({:.1}%)",
            success_count, total_ops, success_rate * 100.0);

        // Should maintain reasonable success rate under stress
        assert!(success_rate > 0.5,
            "Should maintain > 50% success rate under stress, got {:.1}%", success_rate * 100.0);

        harness.teardown().await.unwrap();
    }

    // ========================================
    // System-Level Chaos Test (1 test)
    // ========================================

    #[tokio::test]
    async fn test_integrated_system_chaos_resilience() {
        let chaos_config = NetworkChaosConfig {
            test_duration: Duration::from_secs(20),
            failure_rate: 0.15, // 15% failure rate
            max_concurrent_ops: 8,
            enable_network_chaos: true,
            enable_peer_churn: true,
            enable_message_loss: true,
            enable_slow_network: true,
            recovery_timeout: Duration::from_secs(3),
        };

        let mut env = NetworkSyncTestEnvironment::new().await.unwrap();
        env.setup_coordination().await.unwrap();

        info!("Testing integrated system chaos resilience");

        let start_time = std::time::Instant::now();
        let mut network_ops = 0;
        let mut sync_ops = 0;
        let mut network_failures = 0;
        let mut sync_failures = 0;

        // Run integrated chaos test
        while start_time.elapsed() < chaos_config.test_duration {
            let mut handles = Vec::new();

            // Network operations under chaos
            for i in 0..chaos_config.max_concurrent_ops / 2 {
                // Inject chaos randomly
                if thread_rng().gen_bool(chaos_config.failure_rate) {
                    let chaos_scenario = match i % 4 {
                        0 => ChaosScenario::NetworkPartition,
                        1 => ChaosScenario::MemoryPressure,
                        2 => ChaosScenario::SlowOperation,
                        _ => ChaosScenario::ProcessCrash,
                    };

                    let _ = env.network_harness.inject_failure(chaos_scenario).await;
                }

                let network_msg = match i % 3 {
                    0 => NetworkMessage::BroadcastBlock {
                        block_data: format!("chaos-block-{}", i).into_bytes(),
                        priority: i % 5 == 0,
                    },
                    1 => NetworkMessage::BroadcastTransaction {
                        tx_data: format!("chaos-tx-{}", i).into_bytes(),
                    },
                    _ => NetworkMessage::GetNetworkStatus,
                };

                handles.push(tokio::spawn({
                    let mut net_harness = NetworkTestHarness::new().await.unwrap();
                    async move {
                        net_harness.setup().await.unwrap();
                        let result = net_harness.send_message(network_msg).await;
                        net_harness.teardown().await.unwrap();
                        result
                    }
                }));
            }

            // Sync operations under chaos
            for i in 0..chaos_config.max_concurrent_ops / 2 {
                // Inject chaos randomly
                if thread_rng().gen_bool(chaos_config.failure_rate) {
                    let _ = env.sync_harness.inject_failure(ChaosScenario::SlowOperation).await;
                }

                let sync_msg = match i % 3 {
                    0 => SyncMessage::RequestBlocks {
                        start_height: (i * 50) as u64,
                        count: 10,
                        peer_id: Some(format!("chaos-peer-{}", i % 3)),
                    },
                    1 => SyncMessage::HandleNewBlock {
                        block: format!("chaos-sync-block-{}", i).into_bytes(),
                        peer_id: format!("chaos-source-{}", i % 2),
                    },
                    _ => SyncMessage::GetSyncStatus,
                };

                handles.push(tokio::spawn({
                    let mut sync_harness = SyncTestHarness::new().await.unwrap();
                    async move {
                        sync_harness.setup().await.unwrap();
                        let result = sync_harness.send_message(sync_msg).await;
                        sync_harness.teardown().await.unwrap();
                        result
                    }
                }));
            }

            // Wait for batch completion
            let batch_start = handles.len();
            for (i, handle) in handles.into_iter().enumerate() {
                match handle.await {
                    Ok(Ok(_)) => {
                        if i < batch_start / 2 {
                            network_ops += 1;
                        } else {
                            sync_ops += 1;
                        }
                    }
                    Ok(Err(_)) => {
                        if i < batch_start / 2 {
                            network_failures += 1;
                        } else {
                            sync_failures += 1;
                        }
                    }
                    Err(_) => {
                        if i < batch_start / 2 {
                            network_failures += 1;
                        } else {
                            sync_failures += 1;
                        }
                    }
                }
            }

            // Recovery pause
            sleep(Duration::from_millis(200)).await;
        }

        // Verify system recovery after chaos
        assert!(env.network_harness.verify_state().await.is_ok(),
            "NetworkActor should recover from chaos");
        assert!(env.sync_harness.verify_state().await.is_ok(),
            "SyncActor should recover from chaos");

        // Calculate success rates
        let network_total = network_ops + network_failures;
        let sync_total = sync_ops + sync_failures;

        let network_success_rate = if network_total > 0 {
            network_ops as f64 / network_total as f64
        } else {
            0.0
        };

        let sync_success_rate = if sync_total > 0 {
            sync_ops as f64 / sync_total as f64
        } else {
            0.0
        };

        info!("Integrated chaos test results:");
        info!("  Network: {}/{} success ({:.1}%)", network_ops, network_total, network_success_rate * 100.0);
        info!("  Sync: {}/{} success ({:.1}%)", sync_ops, sync_total, sync_success_rate * 100.0);

        // Both actors should maintain reasonable success rates under chaos
        assert!(network_success_rate > 0.6,
            "NetworkActor should maintain > 60% success under chaos, got {:.1}%", network_success_rate * 100.0);
        assert!(sync_success_rate > 0.6,
            "SyncActor should maintain > 60% success under chaos, got {:.1}%", sync_success_rate * 100.0);

        env.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_pressure_handling() {
        let mut env = NetworkSyncTestEnvironment::new().await.unwrap();
        env.setup_coordination().await.unwrap();

        info!("Testing memory pressure handling");

        // Create memory pressure
        let _memory_hog: Vec<Vec<u8>> = (0..2000).map(|_| vec![0u8; 1024 * 1024]).collect(); // 2GB

        // Test operations under memory pressure
        let large_blocks = create_chaos_test_blocks(10, true);
        for (i, block) in large_blocks.iter().enumerate() {
            let block_msg = NetworkMessage::BroadcastBlock {
                block_data: block.data.clone(),
                priority: i % 3 == 0,
            };

            // Should handle large blocks under memory pressure
            let result = env.network_harness.send_message(block_msg).await;
            assert!(result.is_ok(), "Should handle large block {} under memory pressure", i);

            let sync_msg = SyncMessage::HandleNewBlock {
                block: block.data.clone(),
                peer_id: format!("memory-peer-{}", i % 3),
            };

            let result = env.sync_harness.send_message(sync_msg).await;
            assert!(result.is_ok(), "Should handle sync block {} under memory pressure", i);
        }

        // Verify system stability under memory pressure
        assert!(env.network_harness.verify_state().await.is_ok());
        assert!(env.sync_harness.verify_state().await.is_ok());

        env.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_comprehensive_chaos_scenario() {
        let comprehensive_config = NetworkChaosConfig {
            test_duration: Duration::from_secs(25),
            failure_rate: 0.2, // 20% failure rate
            max_concurrent_ops: 12,
            enable_network_chaos: true,
            enable_peer_churn: true,
            enable_message_loss: true,
            enable_slow_network: true,
            recovery_timeout: Duration::from_secs(3),
        };

        let mut harness = NetworkTestHarness::new().await.unwrap();
        let result = harness.run_chaos_test(comprehensive_config).await;
        assert!(result.is_ok(), "Comprehensive chaos test should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn test_mdns_resilience_under_network_chaos() {
        let mut harness = NetworkTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        info!("Testing mDNS resilience under network chaos");

        // Test mDNS functionality under various chaos conditions
        for scenario in [
            ChaosScenario::NetworkPartition,
            ChaosScenario::SlowOperation,
            ChaosScenario::MemoryPressure,
        ] {
            info!("Testing mDNS under chaos scenario: {:?}", scenario);

            // Inject chaos
            harness.inject_failure(scenario).await.unwrap();

            // mDNS should still work
            let config = create_test_network_config();
            let mut behaviour = AlysNetworkBehaviour::new(&config).unwrap();
            behaviour.initialize().unwrap();

            // mDNS discovery should remain functional
            assert!(behaviour.is_mdns_enabled(), "mDNS should remain enabled under chaos");

            let discovered_peers = behaviour.discover_mdns_peers();
            assert!(!discovered_peers.is_empty(), "mDNS should discover peers even under chaos");

            // Discovered peers should remain valid
            for (peer_id, addresses) in &discovered_peers {
                assert!(!peer_id.is_empty());
                assert!(!addresses.is_empty());
                for addr in addresses {
                    assert!(addr.contains("192.168") || addr.contains("10.0"),
                        "mDNS addresses should remain local under chaos");
                }
            }

            // Recovery pause
            sleep(Duration::from_millis(500)).await;
        }

        // Final verification
        assert!(harness.verify_state().await.is_ok(),
            "NetworkActor should recover after mDNS chaos testing");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_system_recovery_after_cascade_failures() {
        let mut env = NetworkSyncTestEnvironment::new().await.unwrap();
        env.setup_coordination().await.unwrap();

        info!("Testing system recovery after cascade failures");

        // Simulate cascade of failures
        let failure_scenarios = vec![
            ChaosScenario::NetworkPartition,
            ChaosScenario::MemoryPressure,
            ChaosScenario::SlowOperation,
            ChaosScenario::ProcessCrash,
        ];

        for (i, scenario) in failure_scenarios.iter().enumerate() {
            info!("Injecting cascade failure {}: {:?}", i + 1, scenario);

            // Inject failure in both actors
            let _ = env.network_harness.inject_failure(*scenario).await;
            let _ = env.sync_harness.inject_failure(*scenario).await;

            // Brief pause for failure to take effect
            sleep(Duration::from_millis(300)).await;

            // Test operations still work (degraded but functional)
            let test_msg = NetworkMessage::GetNetworkStatus;
            let network_result = env.network_harness.send_message(test_msg).await;
            // May fail during chaos, but should not crash

            let sync_msg = SyncMessage::GetSyncStatus;
            let sync_result = env.sync_harness.send_message(sync_msg).await;
            // May fail during chaos, but should not crash

            info!("Cascade failure {} results: network={:?}, sync={:?}",
                i + 1, network_result.is_ok(), sync_result.is_ok());
        }

        // Recovery period
        info!("Allowing system recovery after cascade failures");
        sleep(Duration::from_secs(5)).await;

        // System should recover after cascade failures
        assert!(env.network_harness.verify_state().await.is_ok(),
            "NetworkActor should recover after cascade failures");
        assert!(env.sync_harness.verify_state().await.is_ok(),
            "SyncActor should recover after cascade failures");

        // Test full functionality after recovery
        assert!(env.test_inter_actor_communication().await.is_ok(),
            "Inter-actor communication should work after recovery");

        env.teardown().await.unwrap();
    }
}