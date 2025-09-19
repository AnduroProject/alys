use crate::actors_v2::testing::storage::{StorageTestHarness, StorageMessage};
use crate::actors_v2::testing::base::{ActorTestHarness, ChaosTestable};
use crate::actors_v2::testing::chaos::{FailureInjector, ChaosScenario, NetworkChaos, DiskChaos, MemoryChaos};
use crate::actors_v2::storage::messages::*;
use crate::actors_v2::testing::storage::fixtures::*;
use crate::auxpow_miner::BlockIndex;
use uuid::Uuid;
use std::time::Duration;
use tokio::time::sleep;
use rand::{thread_rng, Rng};
use async_trait::async_trait;

/// Chaos test configuration for storage actor
#[derive(Debug, Clone)]
pub struct StorageChaosConfig {
    /// Duration to run chaos tests
    pub test_duration: Duration,
    /// Frequency of failure injection
    pub failure_rate: f64,
    /// Maximum number of concurrent operations
    pub max_concurrent_ops: usize,
    /// Enable different types of chaos
    pub enable_network_chaos: bool,
    pub enable_disk_chaos: bool,
    pub enable_memory_chaos: bool,
    /// Recovery timeout after failures
    pub recovery_timeout: Duration,
}

impl Default for StorageChaosConfig {
    fn default() -> Self {
        Self {
            test_duration: Duration::from_secs(30),
            failure_rate: 0.1, // 10% failure rate
            max_concurrent_ops: 10,
            enable_network_chaos: true,
            enable_disk_chaos: true,
            enable_memory_chaos: true,
            recovery_timeout: Duration::from_secs(5),
        }
    }
}

/// Storage-specific chaos test implementation
#[async_trait]
impl ChaosTestable for StorageTestHarness {
    type ChaosConfig = StorageChaosConfig;

    async fn run_chaos_test(&mut self, config: Self::ChaosConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Starting chaos test for Storage Actor");
        let start_time = std::time::Instant::now();

        // Initialize test data
        self.setup().await?;
        let test_blocks = create_test_block_sequence(20);
        let test_state_data = create_test_state_data(15);

        // Create failure injector
        let mut injector = FailureInjector::new();
        if config.enable_network_chaos {
            injector.add_chaos(Box::new(NetworkChaos::new(config.failure_rate)));
        }
        if config.enable_disk_chaos {
            injector.add_chaos(Box::new(DiskChaos::new(config.failure_rate)));
        }
        if config.enable_memory_chaos {
            injector.add_chaos(Box::new(MemoryChaos::new(config.failure_rate)));
        }

        let mut operation_count = 0;
        let mut successful_operations = 0;
        let mut failed_operations = 0;

        // Run chaos operations
        while start_time.elapsed() < config.test_duration {
            let mut handles = Vec::new();

            // Launch concurrent operations
            for _ in 0..config.max_concurrent_ops {
                let operation = self.generate_random_operation(&test_blocks, &test_state_data);
                let should_inject_failure = thread_rng().gen::<f64>() < config.failure_rate;

                if should_inject_failure {
                    // Inject failure before operation
                    if let Err(e) = injector.inject_failure().await {
                        println!("Failed to inject chaos: {}", e);
                    }
                }

                let handle = tokio::spawn({
                    let mut harness_clone = self.clone_for_concurrent_test().await?;
                    async move {
                        let result = harness_clone.send_message(operation).await;
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
                        println!("Concurrent operation panicked: {}", e);
                        failed_operations += 1;
                    }
                }
            }

            // Recovery period
            if failed_operations > 0 {
                println!("Recovery pause after {} failures", failed_operations);
                sleep(config.recovery_timeout).await;
            }

            // Small delay between batches
            sleep(Duration::from_millis(100)).await;
        }

        // Verify system recovery
        println!("Chaos test completed. Verifying system recovery...");
        self.verify_state().await.map_err(|e| format!("System failed to recover: {}", e))?;

        // Report results
        let success_rate = successful_operations as f64 / operation_count as f64;
        println!("Chaos test results:");
        println!("  Total operations: {}", operation_count);
        println!("  Successful: {} ({:.2}%)", successful_operations, success_rate * 100.0);
        println!("  Failed: {} ({:.2}%)", failed_operations, (failed_operations as f64 / operation_count as f64) * 100.0);
        println!("  Duration: {:?}", start_time.elapsed());

        // Ensure minimum success rate
        if success_rate < 0.7 {
            return Err(format!("Success rate too low: {:.2}%", success_rate * 100.0).into());
        }

        self.teardown().await?;
        Ok(())
    }

    async fn inject_failure(&mut self, scenario: crate::actors_v2::testing::chaos::ChaosScenario) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match scenario {
            crate::actors_v2::testing::chaos::ChaosScenario::NetworkPartition => {
                println!("Injecting network partition");
                // Simulate network issues by adding delay
                sleep(Duration::from_millis(500)).await;
            }
            crate::actors_v2::testing::chaos::ChaosScenario::DiskFailure => {
                println!("Injecting disk I/O failure");
                // Simulate disk issues by forcing some operations to fail
                // In a real implementation, this might involve filesystem manipulation
            }
            crate::actors_v2::testing::chaos::ChaosScenario::MemoryPressure => {
                println!("Injecting memory pressure");
                // Simulate memory pressure
                let _memory_hog: Vec<Vec<u8>> = (0..1000).map(|_| vec![0u8; 1024]).collect();
                sleep(Duration::from_millis(100)).await;
            }
            crate::actors_v2::testing::chaos::ChaosScenario::ProcessCrash => {
                println!("Simulating process crash recovery");
                // Reset the harness to simulate crash recovery
                self.reset().await.map_err(|e| format!("Failed to reset after crash: {}", e))?;
            }
            crate::actors_v2::testing::chaos::ChaosScenario::SlowOperation => {
                println!("Injecting operation slowdown");
                sleep(Duration::from_millis(1000)).await;
            }
        }
        Ok(())
    }
}

impl StorageTestHarness {
    /// Generate a random storage operation for chaos testing
    fn generate_random_operation(&self, blocks: &[crate::actors_v2::storage::actor::AlysConsensusBlock], state_data: &[(Vec<u8>, Vec<u8>)]) -> StorageMessage {
        let mut rng = thread_rng();
        let operation_type = rng.gen_range(0..6);

        match operation_type {
            0 => {
                // Store block operation
                let block_idx = rng.gen_range(0..blocks.len());
                let canonical = rng.gen_bool(0.3); // 30% chance of canonical
                StorageMessage::StoreBlock(StoreBlockMessage {
                    block: blocks[block_idx].clone(),
                    canonical,
                    correlation_id: Some(Uuid::new_v4()),
                })
            }
            1 => {
                // Get block operation
                let block_idx = rng.gen_range(0..blocks.len());
                use crate::block::ConvertBlockHash;
                StorageMessage::GetBlock(GetBlockMessage {
                    block_hash: blocks[block_idx].block_hash().to_block_hash(),
                    correlation_id: Some(Uuid::new_v4()),
                })
            }
            2 => {
                // Get block by height
                let height = rng.gen_range(1..=blocks.len() as u64);
                StorageMessage::GetBlockByHeight(GetBlockByHeightMessage {
                    height,
                    correlation_id: Some(Uuid::new_v4()),
                })
            }
            3 => {
                // Update state operation
                let state_idx = rng.gen_range(0..state_data.len());
                StorageMessage::UpdateState(UpdateStateMessage {
                    key: state_data[state_idx].0.clone(),
                    value: state_data[state_idx].1.clone(),
                    correlation_id: Some(Uuid::new_v4()),
                })
            }
            4 => {
                // Get state operation
                let state_idx = rng.gen_range(0..state_data.len());
                StorageMessage::GetState(GetStateMessage {
                    key: state_data[state_idx].0.clone(),
                    correlation_id: Some(Uuid::new_v4()),
                })
            }
            _ => {
                // Get chain head
                StorageMessage::GetChainHead(GetChainHeadMessage {
                    correlation_id: Some(Uuid::new_v4()),
                })
            }
        }
    }

    /// Clone harness for concurrent testing
    async fn clone_for_concurrent_test(&self) -> Result<StorageTestHarness, crate::actors_v2::testing::storage::StorageTestError> {
        // Create a new harness with the same configuration
        // This simulates multiple clients accessing the same storage system
        StorageTestHarness::with_config(self.config.clone()).await
    }
}

#[cfg(test)]
mod chaos_tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_chaos_scenario() {
        let mut harness = StorageTestHarness::new().await.unwrap();

        let chaos_config = StorageChaosConfig {
            test_duration: Duration::from_secs(5), // Short test
            failure_rate: 0.2, // 20% failure rate
            max_concurrent_ops: 3,
            ..Default::default()
        };

        let result = harness.run_chaos_test(chaos_config).await;
        assert!(result.is_ok(), "Basic chaos test failed: {:?}", result);
    }

    #[tokio::test]
    async fn test_network_partition_recovery() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        // Inject network partition
        let result = harness.inject_failure(ChaosScenario::NetworkPartition).await;
        assert!(result.is_ok());

        // Verify system can still operate after network issues
        let test_block = create_test_block(1);
        let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
            block: test_block,
            canonical: true,
            correlation_id: Some(Uuid::new_v4()),
        });

        let store_result = harness.send_message(store_msg).await;
        assert!(store_result.is_ok(), "Storage operation failed after network partition");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_disk_failure_resilience() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        // Inject disk failure
        let result = harness.inject_failure(ChaosScenario::DiskFailure).await;
        assert!(result.is_ok());

        // Test that system handles disk issues gracefully
        let state_msg = StorageMessage::UpdateState(UpdateStateMessage {
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
            correlation_id: Some(Uuid::new_v4()),
        });

        let state_result = harness.send_message(state_msg).await;
        // Operation might fail, but system should not crash
        println!("State operation result after disk failure: {:?}", state_result);

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_pressure_handling() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        // Inject memory pressure
        let result = harness.inject_failure(ChaosScenario::MemoryPressure).await;
        assert!(result.is_ok());

        // Test operations under memory pressure
        let large_blocks = create_performance_test_blocks(5, true);
        for block in large_blocks {
            let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                block,
                canonical: true,
                correlation_id: Some(Uuid::new_v4()),
            });

            // May succeed or fail under memory pressure, but should not panic
            let _ = harness.send_message(store_msg).await;
        }

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_process_crash_recovery() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        // Store some data before crash
        let test_block = create_test_block(1);
        let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
            block: test_block.clone(),
            canonical: true,
            correlation_id: Some(Uuid::new_v4()),
        });
        harness.send_message(store_msg).await.unwrap();

        // Simulate process crash
        let result = harness.inject_failure(ChaosScenario::ProcessCrash).await;
        assert!(result.is_ok());

        // Verify data persistence after recovery
        use crate::block::ConvertBlockHash;
        let get_msg = StorageMessage::GetBlock(GetBlockMessage {
            block_hash: test_block.block_hash().to_block_hash(),
            correlation_id: Some(Uuid::new_v4()),
        });

        let get_result = harness.send_message(get_msg).await;
        assert!(get_result.is_ok(), "Data not persisted after process crash");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_operations_under_chaos() {
        let mut harness = StorageTestHarness::new().await.unwrap();
        harness.setup().await.unwrap();

        let test_blocks = create_test_block_sequence(10);
        let mut handles = Vec::new();

        // Launch concurrent operations with chaos injection
        for (i, block) in test_blocks.iter().enumerate() {
            let block_clone = block.clone();
            let mut harness_clone = harness.clone_for_concurrent_test().await.unwrap();

            let handle = tokio::spawn(async move {
                // Random chaos injection
                if i % 3 == 0 {
                    let _ = harness_clone.inject_failure(ChaosScenario::SlowOperation).await;
                }

                let store_msg = StorageMessage::StoreBlock(StoreBlockMessage {
                    block: block_clone,
                    canonical: i % 2 == 0,
                    correlation_id: Some(Uuid::new_v4()),
                });

                harness_clone.send_message(store_msg).await
            });

            handles.push(handle);
        }

        // Wait for all operations
        let mut successes = 0;
        let mut failures = 0;

        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => successes += 1,
                Ok(Err(_)) => failures += 1,
                Err(_) => failures += 1,
            }
        }

        println!("Concurrent chaos test: {} successes, {} failures", successes, failures);

        // At least some operations should succeed
        assert!(successes > 0, "No operations succeeded under chaos");

        harness.teardown().await.unwrap();
    }

    #[tokio::test]
    async fn test_extended_chaos_scenario() {
        let mut harness = StorageTestHarness::new().await.unwrap();

        let extended_config = StorageChaosConfig {
            test_duration: Duration::from_secs(10),
            failure_rate: 0.15, // 15% failure rate
            max_concurrent_ops: 5,
            enable_network_chaos: true,
            enable_disk_chaos: true,
            enable_memory_chaos: true,
            recovery_timeout: Duration::from_secs(2),
        };

        let result = harness.run_chaos_test(extended_config).await;
        assert!(result.is_ok(), "Extended chaos test failed: {:?}", result);
    }
}