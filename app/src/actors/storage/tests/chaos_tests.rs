//! Chaos engineering tests for Storage Actor resilience
//!
//! These tests simulate various failure scenarios and stress conditions
//! to verify that the Storage Actor can handle adverse situations gracefully
//! and maintain data integrity under extreme conditions.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::actors::storage::actor::{StorageActor, StorageConfig};
    use crate::actors::storage::database::{DatabaseManager, DatabaseConfig};
    use crate::actors::storage::cache::{StorageCache, CacheConfig};
    use super::mock_helpers::{MockDatabase, TestDataGenerator, StorageTestFixture, test_utils};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;
    use tokio::test;
    use rand::Rng;

    /// Configuration for chaos testing scenarios
    struct ChaosConfig {
        pub failure_rate: f64,
        pub network_delay: Duration,
        pub memory_pressure: bool,
        pub disk_full: bool,
        pub corruption_probability: f64,
    }

    impl Default for ChaosConfig {
        fn default() -> Self {
            ChaosConfig {
                failure_rate: 0.1, // 10% failure rate
                network_delay: Duration::from_millis(100),
                memory_pressure: false,
                disk_full: false,
                corruption_probability: 0.01, // 1% corruption probability
            }
        }
    }

    /// Create chaos test configuration
    fn create_chaos_test_config() -> (StorageConfig, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("chaos_test_storage").to_string_lossy().to_string();
        
        let config = StorageConfig {
            database: DatabaseConfig {
                main_path: db_path,
                archive_path: None,
                cache_size_mb: 32,
                write_buffer_size_mb: 8,
                max_open_files: 100,
                compression_enabled: true,
            },
            cache: CacheConfig {
                max_blocks: 100,
                max_state_entries: 1000,
                max_receipts: 500,
                state_ttl: Duration::from_secs(60),
                receipt_ttl: Duration::from_secs(120),
                enable_warming: false,
            },
            write_batch_size: 50,
            sync_interval: Duration::from_millis(100),
            maintenance_interval: Duration::from_secs(10),
            enable_auto_compaction: true,
            metrics_reporting_interval: Duration::from_secs(5),
        };
        
        (config, temp_dir)
    }

    #[test]
    async fn test_database_connection_failures() {
        println!("=== Testing Database Connection Failures ===");
        
        let mock_db = MockDatabase::new_unreliable(0.3); // 30% failure rate
        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(20, 5);
        
        let mut successful_stores = 0;
        let mut failed_stores = 0;
        
        // Attempt to store blocks with simulated database failures
        for (i, block) in test_blocks.iter().enumerate() {
            match mock_db.put_block(block).await {
                Ok(()) => {
                    successful_stores += 1;
                    
                    // Verify we can retrieve successful stores
                    let retrieved = mock_db.get_block(&block.hash()).await
                        .expect("Retrieval should not fail for successfully stored blocks")
                        .expect("Block should exist");
                    
                    assert_eq!(retrieved.slot, block.slot, "Block {} data should match", i);
                }
                Err(_) => {
                    failed_stores += 1;
                }
            }
        }
        
        println!("Storage results: {} successful, {} failed", successful_stores, failed_stores);
        
        // We should have some failures due to the 30% failure rate
        assert!(failed_stores > 0, "Should have some failures with unreliable database");
        assert!(successful_stores > 0, "Should have some successes despite failures");
        
        // Failure rate should be approximately 30% (with some tolerance)
        let actual_failure_rate = failed_stores as f64 / test_blocks.len() as f64;
        assert!(actual_failure_rate >= 0.15 && actual_failure_rate <= 0.45,
               "Failure rate {:.2} should be around 0.30", actual_failure_rate);
        
        println!("✅ Database connection failure test completed");
    }

    #[test]
    async fn test_high_latency_operations() {
        println!("=== Testing High Latency Operations ===");
        
        let high_latency = Duration::from_millis(500);
        let mock_db = MockDatabase::new_slow(high_latency);
        
        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(5, 3);
        
        let start_time = Instant::now();
        
        // Perform operations under high latency
        for block in &test_blocks {
            let operation_start = Instant::now();
            
            mock_db.put_block(block).await
                .expect("High latency operations should still succeed");
            
            let operation_time = operation_start.elapsed();
            assert!(operation_time >= high_latency,
                   "Operation should take at least the simulated latency time");
        }
        
        let total_time = start_time.elapsed();
        let min_expected_time = high_latency * test_blocks.len() as u32;
        
        assert!(total_time >= min_expected_time,
               "Total time should account for high latency");
        
        // Verify data integrity under high latency
        for block in &test_blocks {
            let retrieved = mock_db.get_block(&block.hash()).await
                .expect("Retrieval should succeed despite high latency")
                .expect("Block should exist");
            
            assert_eq!(retrieved.slot, block.slot, "Data integrity should be maintained");
        }
        
        println!("Total time under high latency: {:.2}s", total_time.as_secs_f64());
        println!("✅ High latency operations test completed");
    }

    #[test]
    async fn test_memory_pressure_scenarios() {
        println!("=== Testing Memory Pressure Scenarios ===");
        
        let (config, _temp_dir) = create_chaos_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");
        
        let mut generator = TestDataGenerator::new();
        
        // Create a large number of blocks to pressure memory
        let large_block_count = 500;
        let test_blocks = generator.generate_block_chain(large_block_count, 10);
        
        println!("Storing {} blocks to create memory pressure", large_block_count);
        
        let start_time = Instant::now();
        let mut stored_count = 0;
        
        // Store blocks rapidly to create memory pressure
        for (i, block) in test_blocks.iter().enumerate() {
            match storage_actor.store_block(block.clone(), true).await {
                Ok(()) => {
                    stored_count += 1;
                    
                    // Periodically check memory usage via cache stats
                    if i % 50 == 0 {
                        let cache_stats = storage_actor.cache.get_stats().await;
                        println!("Block {}: Cache memory: {}MB, entries: {}", 
                                i, cache_stats.total_memory_bytes / (1024 * 1024), 
                                cache_stats.block_cache_entries);
                        
                        // Memory should be bounded by cache limits
                        assert!(cache_stats.block_cache_entries <= 100, // Our cache limit
                               "Cache should respect memory limits under pressure");
                    }
                },
                Err(e) => {
                    println!("Storage failed at block {}: {}", i, e);
                    break;
                }
            }
        }
        
        let duration = start_time.elapsed();
        println!("Stored {} blocks in {:.2}s under memory pressure", stored_count, duration.as_secs_f64());
        
        // Should store at least most blocks despite memory pressure
        assert!(stored_count >= large_block_count * 90 / 100,
               "Should store at least 90% of blocks despite memory pressure");
        
        // Verify cache eviction is working
        let final_cache_stats = storage_actor.cache.get_stats().await;
        assert!(final_cache_stats.block_evictions > 0,
               "Cache should evict entries under memory pressure");
        
        println!("Cache evictions: {}", final_cache_stats.block_evictions);
        println!("✅ Memory pressure test completed");
    }

    #[test]
    async fn test_concurrent_stress_with_failures() {
        println!("=== Testing Concurrent Stress with Failures ===");
        
        let mock_db = Arc::new(MockDatabase::new_unreliable(0.2)); // 20% failure rate
        let mut generator = TestDataGenerator::new();
        
        let workers = 8;
        let blocks_per_worker = 25;
        let total_blocks = workers * blocks_per_worker;
        
        // Generate test data for all workers
        let all_blocks = generator.generate_block_chain(total_blocks, 3);
        let block_chunks: Vec<Vec<_>> = all_blocks.chunks(blocks_per_worker).map(|chunk| chunk.to_vec()).collect();
        
        println!("Starting {} workers with {} blocks each", workers, blocks_per_worker);
        
        let start_time = Instant::now();
        let mut handles = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));
        
        // Spawn concurrent workers
        for (worker_id, blocks) in block_chunks.into_iter().enumerate() {
            let db_clone = mock_db.clone();
            let results_clone = results.clone();
            
            let handle = tokio::spawn(async move {
                let mut worker_successes = 0;
                let mut worker_failures = 0;
                let worker_start = Instant::now();
                
                for block in blocks {
                    match db_clone.put_block(&block).await {
                        Ok(()) => {
                            worker_successes += 1;
                            
                            // Verify storage immediately
                            if let Ok(Some(retrieved)) = db_clone.get_block(&block.hash()).await {
                                assert_eq!(retrieved.slot, block.slot, 
                                          "Worker {} data integrity failure", worker_id);
                            }
                        }
                        Err(_) => {
                            worker_failures += 1;
                        }
                    }
                    
                    // Small random delay to add chaos
                    let delay = rand::thread_rng().gen_range(0..10);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                
                let worker_duration = worker_start.elapsed();
                let worker_result = (worker_id, worker_successes, worker_failures, worker_duration);
                
                results_clone.lock().unwrap().push(worker_result);
                worker_result
            });
            
            handles.push(handle);
        }
        
        // Wait for all workers to complete
        let mut total_successes = 0;
        let mut total_failures = 0;
        
        for handle in handles {
            let (worker_id, successes, failures, duration) = handle.await.expect("Worker should complete");
            total_successes += successes;
            total_failures += failures;
            
            println!("Worker {}: {} successes, {} failures in {:.2}s", 
                    worker_id, successes, failures, duration.as_secs_f64());
        }
        
        let total_duration = start_time.elapsed();
        let success_rate = total_successes as f64 / total_blocks as f64;
        
        println!("Overall: {} successes, {} failures in {:.2}s", 
                total_successes, total_failures, total_duration.as_secs_f64());
        println!("Success rate: {:.2}%", success_rate * 100.0);
        
        // Should handle concurrent stress reasonably well
        assert!(success_rate >= 0.6, "Success rate should be at least 60% under stress");
        
        // Check final operation count
        let operation_count = mock_db.get_operation_count();
        println!("Total database operations: {}", operation_count);
        
        println!("✅ Concurrent stress test completed");
    }

    #[test]
    async fn test_rapid_storage_actor_restarts() {
        println!("=== Testing Rapid Storage Actor Restarts ===");
        
        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(20, 5);
        
        // Store initial blocks
        let (config, temp_dir) = create_chaos_test_config();
        let initial_db_path = config.database.main_path.clone();
        
        {
            let mut storage_actor = StorageActor::new(config.clone()).await
                .expect("Failed to create initial storage actor");
            
            // Store first half of blocks
            for block in &test_blocks[..10] {
                storage_actor.store_block(block.clone(), true).await
                    .expect("Failed to store block in initial actor");
            }
            
            println!("Stored {} blocks in initial actor", 10);
        } // Drop initial actor to simulate shutdown
        
        // Simulate rapid restart cycles
        for restart_cycle in 0..5 {
            println!("Restart cycle {}", restart_cycle + 1);
            
            // Create new storage actor (simulating restart)
            let mut restarted_actor = StorageActor::new(config.clone()).await
                .expect("Failed to create restarted storage actor");
            
            // Verify previously stored data is accessible
            for block in &test_blocks[..10] {
                let retrieved = restarted_actor.get_block(&block.hash()).await
                    .expect("Failed to retrieve block after restart")
                    .expect("Block should exist after restart");
                
                assert_eq!(retrieved.slot, block.slot, 
                          "Block data should persist across restart {}", restart_cycle + 1);
            }
            
            // Store additional blocks
            if restart_cycle < test_blocks.len() - 10 {
                let block_to_store = &test_blocks[10 + restart_cycle];
                restarted_actor.store_block(block_to_store.clone(), true).await
                    .expect("Failed to store block after restart");
                
                println!("Stored additional block {} after restart {}", 
                        block_to_store.slot, restart_cycle + 1);
            }
            
            // Brief delay before next restart
            tokio::time::sleep(Duration::from_millis(100)).await;
        } // Drop actor to simulate shutdown
        
        // Final verification with new actor
        {
            let final_actor = StorageActor::new(config.clone()).await
                .expect("Failed to create final storage actor");
            
            let db_stats = final_actor.database.get_stats().await
                .expect("Failed to get final database stats");
            
            println!("Final database stats: {} blocks", db_stats.total_blocks);
            assert!(db_stats.total_blocks >= 10, "Should maintain persistent data across restarts");
        }
        
        println!("✅ Rapid restart test completed");
    }

    #[test]
    async fn test_cache_corruption_recovery() {
        println!("=== Testing Cache Corruption Recovery ===");
        
        let (config, _temp_dir) = create_chaos_test_config();
        let mut storage_actor = StorageActor::new(config).await
            .expect("Failed to create storage actor");
        
        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(15, 4);
        
        // Store blocks normally
        for block in &test_blocks {
            storage_actor.store_block(block.clone(), true).await
                .expect("Failed to store block");
        }
        
        // Verify blocks are cached
        let cache_stats = storage_actor.cache.get_stats().await;
        assert!(cache_stats.block_cache_entries > 0, "Blocks should be cached");
        
        // Simulate cache corruption by clearing it
        println!("Simulating cache corruption...");
        storage_actor.cache.clear_all().await;
        
        let corrupted_cache_stats = storage_actor.cache.get_stats().await;
        assert_eq!(corrupted_cache_stats.block_cache_entries, 0, "Cache should be empty after corruption");
        
        // Verify data recovery from database
        println!("Testing recovery from database...");
        for (i, block) in test_blocks.iter().enumerate() {
            let retrieved = storage_actor.get_block(&block.hash()).await
                .expect("Failed to retrieve block after cache corruption")
                .expect("Block should exist in database after cache corruption");
            
            assert_eq!(retrieved.slot, block.slot, "Block {} should be recoverable from database", i);
        }
        
        // Verify cache rebuilds correctly
        let recovery_cache_stats = storage_actor.cache.get_stats().await;
        println!("Cache entries after recovery: {}", recovery_cache_stats.block_cache_entries);
        
        // Some blocks should be back in cache after retrieval
        assert!(recovery_cache_stats.block_cache_entries > 0, "Cache should rebuild after recovery");
        
        println!("✅ Cache corruption recovery test completed");
    }

    #[test]
    async fn test_partial_write_failures() {
        println!("=== Testing Partial Write Failures ===");
        
        let mock_db = MockDatabase::new_unreliable(0.4); // High failure rate
        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(30, 6);
        
        let mut partial_success_blocks = Vec::new();
        let mut completely_failed_blocks = Vec::new();
        
        // Attempt to store blocks with high failure rate
        for block in &test_blocks {
            match mock_db.put_block(block).await {
                Ok(()) => {
                    // Successfully stored, verify it's accessible
                    match mock_db.get_block(&block.hash()).await {
                        Ok(Some(retrieved)) => {
                            assert_eq!(retrieved.slot, block.slot, "Successfully stored block should match");
                            partial_success_blocks.push(block.clone());
                        }
                        Ok(None) => {
                            panic!("Successfully stored block should be retrievable");
                        }
                        Err(_) => {
                            println!("Warning: Block stored but retrieval failed for block {}", block.slot);
                        }
                    }
                }
                Err(_) => {
                    completely_failed_blocks.push(block.clone());
                }
            }
        }
        
        println!("Results: {} partial successes, {} complete failures", 
                partial_success_blocks.len(), completely_failed_blocks.len());
        
        // Should have both successes and failures with high failure rate
        assert!(partial_success_blocks.len() > 0, "Should have some successful stores");
        assert!(completely_failed_blocks.len() > 0, "Should have some failed stores with high failure rate");
        
        // Test data consistency - all successful blocks should be fully retrievable
        for success_block in &partial_success_blocks {
            let retrieved = mock_db.get_block(&success_block.hash()).await
                .expect("Retrieval should work for successfully stored blocks")
                .expect("Successfully stored blocks should exist");
            
            assert_eq!(retrieved.slot, success_block.slot, "Data integrity should be maintained");
            assert_eq!(retrieved.execution_payload.transactions.len(), 
                      success_block.execution_payload.transactions.len(), 
                      "Transaction data should be complete");
        }
        
        // Failed blocks should consistently return None
        for failed_block in &completely_failed_blocks[..5] { // Test subset
            let result = mock_db.get_block(&failed_block.hash()).await
                .expect("Retrieval operation should succeed even for failed stores");
            
            assert!(result.is_none(), "Failed stores should consistently return None");
        }
        
        println!("✅ Partial write failure test completed");
    }

    #[test]
    async fn test_extreme_load_with_timeouts() {
        println!("=== Testing Extreme Load with Timeouts ===");
        
        let (config, _temp_dir) = create_chaos_test_config();
        let storage_actor = Arc::new(tokio::sync::Mutex::new(
            StorageActor::new(config).await.expect("Failed to create storage actor")
        ));
        
        let mut generator = TestDataGenerator::new();
        let extreme_block_count = 100;
        let test_blocks = generator.generate_block_chain(extreme_block_count, 15); // Large blocks
        
        let timeout_duration = Duration::from_secs(30); // Generous timeout
        let start_time = Instant::now();
        
        println!("Starting extreme load test with {} large blocks", extreme_block_count);
        
        // Create multiple concurrent streams of operations
        let stream_count = 4;
        let blocks_per_stream = test_blocks.len() / stream_count;
        let mut handles = Vec::new();
        
        for stream_id in 0..stream_count {
            let actor_clone = storage_actor.clone();
            let start_idx = stream_id * blocks_per_stream;
            let end_idx = if stream_id == stream_count - 1 { 
                test_blocks.len() 
            } else { 
                (stream_id + 1) * blocks_per_stream 
            };
            let stream_blocks = test_blocks[start_idx..end_idx].to_vec();
            
            let handle = tokio::spawn(async move {
                let mut stream_successes = 0;
                let mut stream_timeouts = 0;
                
                for block in stream_blocks {
                    // Apply timeout to each operation
                    let operation = async {
                        let mut actor = actor_clone.lock().await;
                        actor.store_block(block.clone(), true).await
                    };
                    
                    match test_utils::with_timeout(operation, Duration::from_secs(5)).await {
                        Ok(Ok(())) => {
                            stream_successes += 1;
                        }
                        Ok(Err(e)) => {
                            println!("Stream {} storage error: {}", stream_id, e);
                        }
                        Err(_) => {
                            stream_timeouts += 1;
                            println!("Stream {} operation timed out", stream_id);
                        }
                    }
                }
                
                (stream_id, stream_successes, stream_timeouts)
            });
            
            handles.push(handle);
        }
        
        // Wait for all streams with overall timeout
        let overall_result = test_utils::with_timeout(async {
            let mut total_successes = 0;
            let mut total_timeouts = 0;
            
            for handle in handles {
                let (stream_id, successes, timeouts) = handle.await.expect("Stream should complete");
                total_successes += successes;
                total_timeouts += timeouts;
                
                println!("Stream {}: {} successes, {} timeouts", stream_id, successes, timeouts);
            }
            
            (total_successes, total_timeouts)
        }, timeout_duration).await;
        
        let total_duration = start_time.elapsed();
        
        match overall_result {
            Ok((successes, timeouts)) => {
                println!("Extreme load results: {} successes, {} timeouts in {:.2}s", 
                        successes, timeouts, total_duration.as_secs_f64());
                
                // Should complete most operations even under extreme load
                let success_rate = successes as f64 / extreme_block_count as f64;
                assert!(success_rate >= 0.5, "Should complete at least 50% of operations under extreme load");
                
                // Verify system is still responsive
                let actor = storage_actor.lock().await;
                let final_stats = actor.database.get_stats().await
                    .expect("Database should be responsive after extreme load");
                
                assert!(final_stats.total_blocks > 0, "Should have stored some blocks");
                println!("Final database contains {} blocks", final_stats.total_blocks);
            }
            Err(_) => {
                panic!("Extreme load test timed out after {:.2}s", timeout_duration.as_secs_f64());
            }
        }
        
        println!("✅ Extreme load test completed");
    }

    #[test]
    async fn test_cascading_failure_recovery() {
        println!("=== Testing Cascading Failure Recovery ===");
        
        // Create multiple components with different failure characteristics
        let primary_db = Arc::new(MockDatabase::new_unreliable(0.1));
        let backup_db = Arc::new(MockDatabase::new_unreliable(0.05)); // More reliable backup
        
        let mut generator = TestDataGenerator::new();
        let test_blocks = generator.generate_block_chain(25, 4);
        
        let mut primary_failures = 0;
        let mut backup_successes = 0;
        let mut total_failures = 0;
        
        for block in &test_blocks {
            let block_hash = block.hash();
            
            // Try primary database first
            match primary_db.put_block(block).await {
                Ok(()) => {
                    // Primary success, verify
                    let retrieved = primary_db.get_block(&block_hash).await
                        .expect("Primary retrieval should work")
                        .expect("Primary stored block should exist");
                    
                    assert_eq!(retrieved.slot, block.slot, "Primary storage should be correct");
                }
                Err(_) => {
                    primary_failures += 1;
                    
                    // Primary failed, try backup
                    match backup_db.put_block(block).await {
                        Ok(()) => {
                            backup_successes += 1;
                            
                            // Verify backup storage
                            let retrieved = backup_db.get_block(&block_hash).await
                                .expect("Backup retrieval should work")
                                .expect("Backup stored block should exist");
                            
                            assert_eq!(retrieved.slot, block.slot, "Backup storage should be correct");
                        }
                        Err(_) => {
                            total_failures += 1;
                        }
                    }
                }
            }
        }
        
        println!("Cascading failure results:");
        println!("  Primary failures: {}", primary_failures);
        println!("  Backup recoveries: {}", backup_successes);
        println!("  Total failures: {}", total_failures);
        
        // Most primary failures should be recovered by backup
        if primary_failures > 0 {
            let recovery_rate = backup_successes as f64 / primary_failures as f64;
            assert!(recovery_rate >= 0.8, "Backup should recover most primary failures");
            println!("  Recovery rate: {:.2}%", recovery_rate * 100.0);
        }
        
        // Total system failure rate should be low
        let total_success_rate = (test_blocks.len() - total_failures) as f64 / test_blocks.len() as f64;
        assert!(total_success_rate >= 0.9, "Overall system should have high success rate");
        
        println!("  Overall success rate: {:.2}%", total_success_rate * 100.0);
        println!("✅ Cascading failure recovery test completed");
    }
}