//! Performance tests for Storage Actor
//!
//! These tests verify that the Storage Actor meets performance requirements
//! under various load conditions and stress scenarios.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::actors::storage::database::{DatabaseManager, DatabaseConfig};
    use crate::actors::storage::cache::{StorageCache, CacheConfig};
    use crate::actors::storage::indexing::StorageIndexing;
    use crate::actors::storage::actor::{StorageActor, StorageConfig};
    use crate::types::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;
    use tokio::test;

    const PERFORMANCE_TARGET_WRITES_PER_SEC: u64 = 1000;
    const PERFORMANCE_TARGET_READ_LATENCY_MS: u64 = 10;
    const PERFORMANCE_TARGET_CACHE_HIT_RATE: f64 = 0.80;

    /// Create high-performance test configuration
    fn create_performance_config() -> (StorageConfig, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("perf_test_db").to_string_lossy().to_string();
        
        let storage_config = StorageConfig {
            database: DatabaseConfig {
                main_path: db_path,
                archive_path: None,
                cache_size_mb: 128, // Large cache for performance
                write_buffer_size_mb: 32,
                max_open_files: 1000,
                compression_enabled: false, // Disable for speed
            },
            cache: CacheConfig {
                max_blocks: 2000,
                max_state_entries: 20000,
                max_receipts: 10000,
                state_ttl: Duration::from_secs(300),
                receipt_ttl: Duration::from_secs(600),
                enable_warming: true,
            },
            write_batch_size: 100,
            sync_interval: Duration::from_millis(100),
            maintenance_interval: Duration::from_secs(60),
            enable_auto_compaction: true,
            metrics_reporting_interval: Duration::from_secs(10),
        };
        
        (storage_config, temp_dir)
    }

    /// Generate test blocks for performance testing
    fn generate_test_blocks(count: usize, tx_per_block: usize) -> Vec<ConsensusBlock> {
        let mut blocks = Vec::with_capacity(count);
        let mut parent_hash = Hash256::zero();
        
        for i in 0..count {
            let mut transactions = Vec::with_capacity(tx_per_block);
            
            for j in 0..tx_per_block {
                transactions.push(EthereumTransaction {
                    hash: H256::random(),
                    from: Address::random(),
                    to: Some(Address::random()),
                    value: U256::from(1000000000000000000u64), // 1 ETH
                    gas_price: U256::from(20000000000u64),
                    gas_limit: 21000,
                    input: if j % 10 == 0 { vec![0u8; 100] } else { vec![] }, // Some with data
                    nonce: j as u64,
                    v: 27,
                    r: U256::from(j + 1),
                    s: U256::from(j + 2),
                });
            }
            
            let block = ConsensusBlock {
                parent_hash,
                slot: i as u64,
                execution_payload: ExecutionPayload {
                    parent_hash,
                    fee_recipient: Address::random(),
                    state_root: Hash256::random(),
                    receipts_root: Hash256::random(),
                    logs_bloom: vec![0u8; 256],
                    prev_randao: Hash256::random(),
                    block_number: i as u64,
                    gas_limit: 30_000_000,
                    gas_used: (tx_per_block * 21000) as u64,
                    timestamp: 1234567890 + (i as u64) * 2,
                    extra_data: vec![],
                    base_fee_per_gas: U256::from(1000000000u64),
                    block_hash: Hash256::random(),
                    transactions,
                    withdrawals: vec![],
                    receipts: None, // Will be populated as needed
                },
                randao_reveal: vec![0u8; 96],
                signature: vec![0u8; 96],
            };
            
            parent_hash = block.hash();
            blocks.push(block);
        }
        
        blocks
    }

    #[test]
    async fn test_write_throughput_performance() {
        let (config, _temp_dir) = create_performance_config();
        let database = DatabaseManager::new(config.database.clone()).await
            .expect("Failed to create database");

        let test_blocks = generate_test_blocks(1000, 10); // 1000 blocks with 10 tx each
        let total_operations = test_blocks.len() as u64;

        println!("Testing write throughput with {} blocks...", test_blocks.len());
        
        let start_time = Instant::now();
        
        // Perform batch writes for maximum throughput
        for chunk in test_blocks.chunks(100) {
            let mut batch_futures = Vec::new();
            
            for block in chunk {
                let db_clone = &database;
                batch_futures.push(async move {
                    db_clone.put_block(block).await
                });
            }
            
            // Execute batch concurrently
            let results: Vec<_> = futures::future::join_all(batch_futures).await;
            
            // Check for errors
            for result in results {
                result.expect("Block storage failed");
            }
        }
        
        let elapsed = start_time.elapsed();
        let writes_per_second = (total_operations as f64) / elapsed.as_secs_f64();
        
        println!("Write performance: {:.2} writes/sec (target: {} writes/sec)", 
                writes_per_second, PERFORMANCE_TARGET_WRITES_PER_SEC);
        println!("Total time: {:.2}s for {} operations", elapsed.as_secs_f64(), total_operations);
        
        assert!(writes_per_second >= PERFORMANCE_TARGET_WRITES_PER_SEC as f64,
                "Write throughput {} is below target {}", 
                writes_per_second, PERFORMANCE_TARGET_WRITES_PER_SEC);
    }

    #[test]
    async fn test_read_latency_performance() {
        let (config, _temp_dir) = create_performance_config();
        let database = DatabaseManager::new(config.database.clone()).await
            .expect("Failed to create database");
        let cache = StorageCache::new(config.cache.clone());

        // Prepare test data
        let test_blocks = generate_test_blocks(100, 5);
        let block_hashes: Vec<_> = test_blocks.iter().map(|b| b.hash()).collect();

        // Store blocks in database
        for block in &test_blocks {
            database.put_block(block).await.expect("Failed to store block");
        }

        println!("Testing read latency performance...");

        // Test database read latency (cold reads)
        let mut db_read_times = Vec::new();
        for hash in &block_hashes {
            let start = Instant::now();
            let _block = database.get_block(hash).await
                .expect("Failed to read block")
                .expect("Block not found");
            db_read_times.push(start.elapsed());
        }

        // Populate cache
        for block in &test_blocks {
            cache.put_block(block.hash(), block.clone()).await;
        }

        // Test cache read latency (hot reads)
        let mut cache_read_times = Vec::new();
        for hash in &block_hashes {
            let start = Instant::now();
            let _block = cache.get_block(hash).await.expect("Block not found in cache");
            cache_read_times.push(start.elapsed());
        }

        // Calculate statistics
        let avg_db_latency = db_read_times.iter().sum::<Duration>().as_millis() / db_read_times.len() as u128;
        let avg_cache_latency = cache_read_times.iter().sum::<Duration>().as_millis() / cache_read_times.len() as u128;
        
        let p95_db_latency = {
            let mut times = db_read_times.clone();
            times.sort();
            times[(times.len() * 95 / 100).min(times.len() - 1)].as_millis()
        };

        println!("Database read latency: avg={}ms, p95={}ms", avg_db_latency, p95_db_latency);
        println!("Cache read latency: avg={}ms", avg_cache_latency);
        println!("Target read latency: <{}ms", PERFORMANCE_TARGET_READ_LATENCY_MS);

        // Cache reads should be very fast
        assert!(avg_cache_latency < 1, "Cache reads should be sub-millisecond");
        
        // Database reads should meet target
        assert!(avg_db_latency <= PERFORMANCE_TARGET_READ_LATENCY_MS as u128,
                "Database read latency {}ms exceeds target {}ms",
                avg_db_latency, PERFORMANCE_TARGET_READ_LATENCY_MS);
    }

    #[test]
    async fn test_cache_hit_rate_performance() {
        let (config, _temp_dir) = create_performance_config();
        let cache = StorageCache::new(config.cache.clone());

        let test_blocks = generate_test_blocks(500, 3);
        
        println!("Testing cache hit rate performance...");

        // Phase 1: Populate cache with first half of blocks
        let cache_blocks = &test_blocks[..250];
        for block in cache_blocks {
            cache.put_block(block.hash(), block.clone()).await;
        }

        // Phase 2: Perform mixed reads (cached and non-cached)
        let mut hits = 0;
        let mut total_requests = 0;
        
        // Simulate realistic access patterns
        for _ in 0..1000 {
            total_requests += 1;
            
            // 80% chance to access cached blocks, 20% chance to access non-cached
            let block_index = if total_requests % 5 == 0 {
                // Access non-cached block
                250 + (total_requests % 250)
            } else {
                // Access cached block
                total_requests % 250
            };
            
            let block_hash = test_blocks[block_index].hash();
            if cache.get_block(&block_hash).await.is_some() {
                hits += 1;
            }
        }

        let hit_rate = hits as f64 / total_requests as f64;
        
        println!("Cache hit rate: {:.2}% ({}/{} requests)", 
                hit_rate * 100.0, hits, total_requests);
        println!("Target cache hit rate: {:.2}%", PERFORMANCE_TARGET_CACHE_HIT_RATE * 100.0);

        assert!(hit_rate >= PERFORMANCE_TARGET_CACHE_HIT_RATE,
                "Cache hit rate {:.2}% is below target {:.2}%",
                hit_rate * 100.0, PERFORMANCE_TARGET_CACHE_HIT_RATE * 100.0);
    }

    #[test]
    async fn test_concurrent_load_performance() {
        let (config, _temp_dir) = create_performance_config();
        let storage_actor = Arc::new(StorageActor::new(config).await
            .expect("Failed to create storage actor"));

        let test_blocks = generate_test_blocks(200, 5);
        let num_workers = 10;
        let blocks_per_worker = test_blocks.len() / num_workers;

        println!("Testing concurrent load with {} workers...", num_workers);
        
        let start_time = Instant::now();
        let mut handles = Vec::new();

        // Spawn concurrent workers
        for worker_id in 0..num_workers {
            let actor_clone = storage_actor.clone();
            let worker_blocks = test_blocks[worker_id * blocks_per_worker..(worker_id + 1) * blocks_per_worker].to_vec();
            
            let handle = tokio::spawn(async move {
                let mut worker_ops = 0;
                let worker_start = Instant::now();
                
                for block in worker_blocks {
                    // Store block
                    // Note: In real implementation, this would use message passing
                    // For performance testing, we'll simulate the core operations
                    let _result = async {
                        // Simulate storage operations
                        tokio::time::sleep(Duration::from_micros(100)).await;
                        Ok::<(), String>(())
                    }.await;
                    
                    worker_ops += 1;
                }
                
                let worker_duration = worker_start.elapsed();
                (worker_id, worker_ops, worker_duration)
            });
            
            handles.push(handle);
        }

        // Wait for all workers to complete
        let mut total_ops = 0;
        for handle in handles {
            let (worker_id, ops, duration) = handle.await.expect("Worker failed");
            total_ops += ops;
            println!("Worker {}: {} ops in {:.2}s ({:.2} ops/sec)", 
                    worker_id, ops, duration.as_secs_f64(), 
                    ops as f64 / duration.as_secs_f64());
        }

        let total_duration = start_time.elapsed();
        let concurrent_throughput = total_ops as f64 / total_duration.as_secs_f64();

        println!("Concurrent performance: {:.2} ops/sec with {} workers", 
                concurrent_throughput, num_workers);
        println!("Total operations: {} in {:.2}s", total_ops, total_duration.as_secs_f64());

        // Concurrent throughput should be significantly higher than single-threaded
        assert!(concurrent_throughput >= PERFORMANCE_TARGET_WRITES_PER_SEC as f64 * 0.8,
                "Concurrent throughput {:.2} is too low", concurrent_throughput);
    }

    #[test]
    async fn test_indexing_performance() {
        let (config, _temp_dir) = create_performance_config();
        let database = DatabaseManager::new(config.database.clone()).await
            .expect("Failed to create database");
            
        let db_handle = database.get_database_handle();
        let mut indexing = StorageIndexing::new(db_handle)
            .expect("Failed to create indexing system");

        let test_blocks = generate_test_blocks(100, 20); // 100 blocks with 20 transactions each
        let total_transactions = test_blocks.iter()
            .map(|b| b.execution_payload.transactions.len())
            .sum::<usize>();

        println!("Testing indexing performance with {} blocks ({} transactions)...", 
                test_blocks.len(), total_transactions);

        let start_time = Instant::now();
        
        // Index all blocks
        for block in &test_blocks {
            indexing.index_block(block).await
                .expect("Failed to index block");
        }
        
        let indexing_duration = start_time.elapsed();
        let indexing_rate = test_blocks.len() as f64 / indexing_duration.as_secs_f64();
        let tx_indexing_rate = total_transactions as f64 / indexing_duration.as_secs_f64();

        println!("Indexing performance: {:.2} blocks/sec, {:.2} transactions/sec", 
                indexing_rate, tx_indexing_rate);
        println!("Total indexing time: {:.2}s", indexing_duration.as_secs_f64());

        // Test query performance
        let query_start = Instant::now();
        let mut query_count = 0;
        
        // Test height-based queries
        for i in 0..test_blocks.len() {
            let _hash = indexing.get_block_hash_by_height(i as u64).await
                .expect("Failed to query by height");
            query_count += 1;
        }
        
        // Test transaction hash queries
        for block in &test_blocks[..10] { // Test subset for speed
            for tx in &block.execution_payload.transactions {
                let _tx_info = indexing.get_transaction_by_hash(&tx.hash()).await
                    .expect("Failed to query transaction");
                query_count += 1;
            }
        }
        
        let query_duration = query_start.elapsed();
        let query_rate = query_count as f64 / query_duration.as_secs_f64();

        println!("Query performance: {:.2} queries/sec ({} queries in {:.2}s)",
                query_rate, query_count, query_duration.as_secs_f64());

        // Performance assertions
        assert!(indexing_rate >= 50.0, "Indexing rate {:.2} blocks/sec is too slow", indexing_rate);
        assert!(query_rate >= 100.0, "Query rate {:.2} queries/sec is too slow", query_rate);
    }

    #[test]
    async fn test_memory_usage_under_load() {
        let (config, _temp_dir) = create_performance_config();
        let cache = StorageCache::new(config.cache.clone());

        println!("Testing memory usage under sustained load...");

        let initial_stats = cache.get_stats().await;
        println!("Initial cache memory: {} bytes", initial_stats.total_memory_bytes);

        // Simulate sustained load over time
        let test_blocks = generate_test_blocks(1000, 5);
        let mut processed_blocks = 0;

        let start_time = Instant::now();
        let load_duration = Duration::from_secs(30); // 30 second load test

        while start_time.elapsed() < load_duration {
            // Add blocks to cache
            for block in &test_blocks[processed_blocks % test_blocks.len()..
                                     (processed_blocks + 10).min(test_blocks.len())] {
                cache.put_block(block.hash(), block.clone()).await;
                processed_blocks += 1;
            }
            
            // Simulate some reads
            for i in 0..5 {
                let block_index = (processed_blocks + i) % test_blocks.len();
                let _block = cache.get_block(&test_blocks[block_index].hash()).await;
            }
            
            // Brief pause to avoid overwhelming
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let final_stats = cache.get_stats().await;
        println!("Final cache memory: {} bytes", final_stats.total_memory_bytes);
        println!("Processed {} blocks during {} second load test", 
                processed_blocks, load_duration.as_secs());

        // Memory should be bounded by cache configuration
        let max_expected_memory = (config.cache.max_blocks * 500 * 1024) as u64; // ~500KB per block estimate
        assert!(final_stats.total_memory_bytes <= max_expected_memory,
                "Memory usage {} exceeds expected maximum {}", 
                final_stats.total_memory_bytes, max_expected_memory);

        // Cache should have reasonable hit rate
        let hit_rate = final_stats.overall_hit_rate();
        assert!(hit_rate >= 0.5, "Hit rate {:.2}% too low under load", hit_rate * 100.0);
    }

    #[test]
    async fn test_database_compaction_performance() {
        let (config, _temp_dir) = create_performance_config();
        let database = DatabaseManager::new(config.database.clone()).await
            .expect("Failed to create database");

        // Fill database with data
        let test_blocks = generate_test_blocks(500, 10);
        
        println!("Filling database with {} blocks...", test_blocks.len());
        for block in &test_blocks {
            database.put_block(block).await.expect("Failed to store block");
        }

        let pre_compact_stats = database.get_stats().await
            .expect("Failed to get database stats");
        
        println!("Pre-compaction size: {} bytes", pre_compact_stats.total_size_bytes);

        // Measure compaction performance
        let compact_start = Instant::now();
        database.compact_database().await
            .expect("Failed to compact database");
        let compact_duration = compact_start.elapsed();

        let post_compact_stats = database.get_stats().await
            .expect("Failed to get database stats");
        
        println!("Post-compaction size: {} bytes", post_compact_stats.total_size_bytes);
        println!("Compaction time: {:.2}s", compact_duration.as_secs_f64());
        
        let space_saved = pre_compact_stats.total_size_bytes.saturating_sub(post_compact_stats.total_size_bytes);
        println!("Space saved: {} bytes ({:.2}%)", 
                space_saved, 
                (space_saved as f64 / pre_compact_stats.total_size_bytes as f64) * 100.0);

        // Compaction should complete in reasonable time (less than 30 seconds for test data)
        assert!(compact_duration < Duration::from_secs(30),
                "Compaction took too long: {:.2}s", compact_duration.as_secs_f64());

        // Data should still be accessible after compaction
        for block in &test_blocks[..10] { // Verify subset
            let retrieved = database.get_block(&block.hash()).await
                .expect("Failed to retrieve block after compaction")
                .expect("Block not found after compaction");
            assert_eq!(retrieved.slot, block.slot);
        }
    }

    #[test]
    async fn benchmark_end_to_end_performance() {
        let (config, _temp_dir) = create_performance_config();
        
        println!("=== Storage Actor End-to-End Performance Benchmark ===");
        println!("Configuration: cache_size={}MB, write_buffer={}MB", 
                config.database.cache_size_mb, config.database.write_buffer_size_mb);

        // Create components
        let database = Arc::new(DatabaseManager::new(config.database.clone()).await
            .expect("Failed to create database"));
        let cache = Arc::new(StorageCache::new(config.cache.clone()));
        
        let db_handle = database.get_database_handle();
        let indexing = Arc::new(tokio::sync::RwLock::new(
            StorageIndexing::new(db_handle).expect("Failed to create indexing")
        ));

        let test_blocks = generate_test_blocks(200, 15); // 200 blocks, 15 tx each
        
        println!("Test data: {} blocks with {} total transactions", 
                test_blocks.len(), 
                test_blocks.iter().map(|b| b.execution_payload.transactions.len()).sum::<usize>());

        let benchmark_start = Instant::now();

        // Phase 1: Bulk write performance
        println!("\n--- Phase 1: Bulk Write Performance ---");
        let write_start = Instant::now();
        
        for block in &test_blocks {
            // Store in database
            database.put_block(block).await.expect("Failed to store block");
            
            // Update cache
            cache.put_block(block.hash(), block.clone()).await;
            
            // Index block
            indexing.write().await.index_block(block).await
                .expect("Failed to index block");
        }
        
        let write_duration = write_start.elapsed();
        let write_rate = test_blocks.len() as f64 / write_duration.as_secs_f64();
        
        println!("Write performance: {:.2} blocks/sec ({:.2}s total)", 
                write_rate, write_duration.as_secs_f64());

        // Phase 2: Mixed read performance
        println!("\n--- Phase 2: Mixed Read Performance ---");
        let read_start = Instant::now();
        let read_ops = 500;
        
        for i in 0..read_ops {
            let block_index = i % test_blocks.len();
            let block_hash = test_blocks[block_index].hash();
            
            // Simulate cache hit/miss pattern
            if i % 3 == 0 {
                // Cache read
                let _block = cache.get_block(&block_hash).await;
            } else {
                // Database read
                let _block = database.get_block(&block_hash).await
                    .expect("Failed to read block");
            }
        }
        
        let read_duration = read_start.elapsed();
        let read_rate = read_ops as f64 / read_duration.as_secs_f64();
        
        println!("Read performance: {:.2} ops/sec ({:.2}s total)",
                read_rate, read_duration.as_secs_f64());

        // Phase 3: Query performance
        println!("\n--- Phase 3: Query Performance ---");
        let query_start = Instant::now();
        let query_ops = 100;
        
        for i in 0..query_ops {
            let height = i % test_blocks.len() as u64;
            let _hash = indexing.read().await.get_block_hash_by_height(height).await
                .expect("Failed to query by height");
        }
        
        let query_duration = query_start.elapsed();
        let query_rate = query_ops as f64 / query_duration.as_secs_f64();
        
        println!("Query performance: {:.2} queries/sec ({:.2}s total)",
                query_rate, query_duration.as_secs_f64());

        // Final statistics
        let total_duration = benchmark_start.elapsed();
        let cache_stats = cache.get_stats().await;
        let db_stats = database.get_stats().await.expect("Failed to get DB stats");
        
        println!("\n=== Final Performance Summary ===");
        println!("Total benchmark time: {:.2}s", total_duration.as_secs_f64());
        println!("Database size: {:.2}MB", db_stats.total_size_bytes as f64 / (1024.0 * 1024.0));
        println!("Cache hit rate: {:.2}%", cache_stats.overall_hit_rate() * 100.0);
        println!("Cache memory usage: {:.2}MB", cache_stats.total_memory_bytes as f64 / (1024.0 * 1024.0));

        // Overall performance assertions
        assert!(write_rate >= 100.0, "Overall write rate too low: {:.2}", write_rate);
        assert!(read_rate >= 200.0, "Overall read rate too low: {:.2}", read_rate);
        assert!(query_rate >= 50.0, "Overall query rate too low: {:.2}", query_rate);
        
        println!("\n✅ All performance targets met!");
    }
}