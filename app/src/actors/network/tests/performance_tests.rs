//! Performance Tests for Network Actor System
//! 
//! Benchmarks and performance validation for the network actor system
//! to ensure it meets the specified targets.

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};
    use crate::actors::network::*;
    use crate::actors::network::messages::*;
    use crate::actors::network::tests::test_helpers::*;

    #[actix::test]
    async fn test_sync_throughput_target() {
        // Test that sync can achieve 250+ blocks/sec target
        let config = SyncConfig::high_performance();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();

        // Simulate high throughput scenario
        sync_actor_obj.state.metrics.current_bps = 300.0;
        sync_actor_obj.state.metrics.peak_bps = 350.0;
        sync_actor_obj.state.metrics.average_bps = 280.0;

        assert!(sync_actor_obj.state.is_meeting_targets());
        assert_eq!(sync_actor_obj.state.health_status(), SyncHealthStatus::Idle); // Not active, so idle
    }

    #[actix::test]
    async fn test_message_handling_latency() {
        let config = test_sync_config();
        let sync_actor = SyncActor::new(config).unwrap().start();

        let mut total_latency = Duration::from_secs(0);
        let iterations = 10;

        for _ in 0..iterations {
            let start = Instant::now();
            let _ = sync_actor.send(GetSyncStatus).await.unwrap();
            total_latency += start.elapsed();
        }

        let average_latency = total_latency / iterations;
        
        // Message handling should be under 10ms on average
        assert!(average_latency < Duration::from_millis(10), 
               "Average message latency too high: {:?}", average_latency);
        
        println!("Average message latency: {:?}", average_latency);
    }

    #[actix::test]
    async fn test_concurrent_message_throughput() {
        let config = test_sync_config();
        let sync_actor = SyncActor::new(config).unwrap().start();

        let concurrent_messages = 100;
        let start_time = Instant::now();

        // Send concurrent messages
        let mut futures = Vec::new();
        for _ in 0..concurrent_messages {
            futures.push(sync_actor.send(GetSyncStatus));
        }

        let results = futures::future::join_all(futures).await;
        let total_time = start_time.elapsed();

        // Verify all messages succeeded
        let successful = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(successful, concurrent_messages);

        // Calculate throughput
        let throughput = concurrent_messages as f64 / total_time.as_secs_f64();
        
        // Should handle at least 1000 messages/sec
        assert!(throughput > 1000.0, 
               "Message throughput too low: {:.2} msg/sec", throughput);
        
        println!("Concurrent message throughput: {:.2} msg/sec", throughput);
    }

    #[actix::test]
    async fn test_block_processing_performance() {
        let config = SyncConfig::high_performance();
        let processor = BlockProcessor::new(config);

        // Create test blocks
        let mut blocks = Vec::new();
        for i in 0..50 {
            blocks.push(create_test_block_data(i));
        }

        let start_time = Instant::now();
        let result = processor.process_block_batch(blocks).await;
        let processing_time = start_time.elapsed();

        assert!(result.is_ok());
        let processing_result = result.unwrap();

        // Calculate throughput
        let blocks_per_second = processing_result.processed_blocks as f64 / processing_time.as_secs_f64();
        
        println!("Block processing throughput: {:.2} blocks/sec", blocks_per_second);
        println!("Processing time: {:?}", processing_time);
        
        // Should process at least 100 blocks/sec in test environment
        assert!(blocks_per_second > 100.0, 
               "Block processing too slow: {:.2} blocks/sec", blocks_per_second);
    }

    #[actix::test]
    async fn test_memory_usage_patterns() {
        let config = SyncConfig::default();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();

        // Simulate various memory usage scenarios
        sync_actor_obj.state.metrics.recent_samples.clear();
        
        // Add performance samples to simulate memory usage
        for i in 0..1000 {
            sync_actor_obj.state.add_performance_sample(
                250.0 + (i % 50) as f64, // Varying throughput
                10.0 + (i % 20) as f64,  // Varying validation time
            );
        }

        // Check that memory usage is controlled (samples are limited)
        assert!(sync_actor_obj.state.metrics.recent_samples.len() <= 100);
        
        // Performance metrics should be reasonable
        assert!(sync_actor_obj.state.metrics.current_bps > 0.0);
        assert!(sync_actor_obj.state.metrics.average_bps > 0.0);
        assert!(sync_actor_obj.state.metrics.peak_bps >= sync_actor_obj.state.metrics.average_bps);
    }

    #[actix::test]
    async fn test_peer_connection_scalability() {
        let mut config = test_peer_config();
        config.max_peers = 100; // Test with more peers
        
        let peer_actor = PeerActor::new(config).unwrap().start();

        // Test peer status handling with no peers (should be fast)
        let start_time = Instant::now();
        let result = peer_actor.send(GetPeerStatus { peer_id: None }).await;
        let query_time = start_time.elapsed();

        assert!(result.is_ok());
        assert!(query_time < Duration::from_millis(50), 
               "Peer status query too slow: {:?}", query_time);

        // Test best peer selection
        let start_time = Instant::now();
        let best_peers_result = peer_actor.send(GetBestPeers {
            count: 10,
            operation_type: OperationType::BlockSync,
            exclude_peers: vec![],
        }).await;
        let selection_time = start_time.elapsed();

        assert!(best_peers_result.is_ok());
        assert!(selection_time < Duration::from_millis(100),
               "Peer selection too slow: {:?}", selection_time);
    }

    #[actix::test]
    async fn test_checkpoint_performance() {
        let temp_dir = create_test_checkpoint_dir();
        let checkpoint_manager = CheckpointManager::new(
            temp_dir.path().to_path_buf(),
            10,
            true, // Enable compression
        ).await;

        assert!(checkpoint_manager.is_ok());
        let mut manager = checkpoint_manager.unwrap();

        // Test checkpoint creation performance
        let chain_state = create_test_chain_state(1000);
        let start_time = Instant::now();
        
        let result = manager.create_checkpoint(1000, chain_state).await;
        let creation_time = start_time.elapsed();

        assert!(result.is_ok());
        let response = result.unwrap();
        
        println!("Checkpoint creation time: {:?}", creation_time);
        println!("Checkpoint size: {} bytes", response.size_bytes);
        
        // Should create checkpoint reasonably quickly
        assert!(creation_time < Duration::from_secs(5),
               "Checkpoint creation too slow: {:?}", creation_time);

        // Test checkpoint restoration performance
        let start_time = Instant::now();
        let restore_result = manager.restore_checkpoint(&response.checkpoint_id, true).await;
        let restore_time = start_time.elapsed();

        assert!(restore_result.is_ok());
        println!("Checkpoint restore time: {:?}", restore_time);
        
        // Restoration should be fast
        assert!(restore_time < Duration::from_secs(2),
               "Checkpoint restoration too slow: {:?}", restore_time);
    }

    #[actix::test]
    async fn test_network_supervision_overhead() {
        let config = test_supervision_config();
        let supervisor = NetworkSupervisor::new(config);

        // Test status retrieval performance
        let start_time = Instant::now();
        let status = supervisor.get_network_status();
        let status_time = start_time.elapsed();

        // Status retrieval should be very fast
        assert!(status_time < Duration::from_millis(10),
               "Supervision status too slow: {:?}", status_time);

        // Verify status structure
        assert_eq!(status.total_restarts, 0);
        assert!(status.system_uptime >= Duration::from_secs(0));
        assert_eq!(status.actor_states.len(), 0); // No actors started in test
    }

    #[tokio::test]
    async fn test_parallel_validation_scaling() {
        // Test different worker counts for parallel validation
        let worker_counts = [1, 2, 4, 8];
        let mut results = Vec::new();

        for &workers in &worker_counts {
            let mut config = SyncConfig::default();
            config.validation_workers = workers;
            
            let processor = BlockProcessor::new(config);
            
            // Create test blocks
            let blocks: Vec<_> = (0..20).map(create_test_block_data).collect();
            
            let start_time = Instant::now();
            let result = processor.process_block_batch(blocks).await;
            let processing_time = start_time.elapsed();
            
            assert!(result.is_ok());
            let processing_result = result.unwrap();
            
            let throughput = processing_result.processed_blocks as f64 / processing_time.as_secs_f64();
            results.push((workers, throughput));
            
            println!("Workers: {}, Throughput: {:.2} blocks/sec", workers, throughput);
        }

        // Generally, more workers should improve throughput (though not always linear)
        // At minimum, performance shouldn't degrade significantly with more workers
        let single_worker_throughput = results[0].1;
        let multi_worker_throughput = results.last().unwrap().1;
        
        // Multi-worker should be at least 80% of single worker (accounting for overhead)
        assert!(multi_worker_throughput >= single_worker_throughput * 0.8,
               "Multi-worker performance regression: single={:.2}, multi={:.2}", 
               single_worker_throughput, multi_worker_throughput);
    }

    #[actix::test]  
    async fn test_sync_mode_performance_characteristics() {
        // Test different sync modes have expected performance characteristics
        let modes = [SyncMode::Fast, SyncMode::Full, SyncMode::Recovery, SyncMode::Emergency];
        
        for mode in modes {
            let mut config = SyncConfig::default();
            let workers = mode.validation_workers(4);
            let batch_size = mode.batch_size(256);
            
            println!("Mode: {:?}, Workers: {}, Batch: {}", mode, workers, batch_size);
            
            // Fast mode should have standard settings
            if matches!(mode, SyncMode::Fast) {
                assert_eq!(workers, 4);
                assert_eq!(batch_size, 256);
                assert!(!mode.requires_full_validation());
                assert!(mode.supports_checkpoints());
            }
            
            // Full mode should have more workers, smaller batches
            if matches!(mode, SyncMode::Full) {
                assert_eq!(workers, 8); // 2x workers
                assert_eq!(batch_size, 128); // Half batch size
                assert!(mode.requires_full_validation());
                assert!(mode.supports_checkpoints());
            }
            
            // Emergency mode should be minimal
            if matches!(mode, SyncMode::Emergency) {
                assert_eq!(workers, 1); // Minimal workers
                assert_eq!(batch_size, 64); // Small batches
                assert!(!mode.supports_checkpoints());
            }
        }
    }

    #[actix::test]
    async fn test_configuration_performance_impact() {
        // Test performance impact of different configurations
        let configs = [
            ("Default", SyncConfig::default()),
            ("Lightweight", SyncConfig::lightweight()),
            ("High Performance", SyncConfig::high_performance()),
            ("Federation", SyncConfig::federation()),
        ];

        for (name, config) in configs {
            // Validate configuration
            assert!(config.validate().is_ok(), "Invalid config: {}", name);
            
            // Check performance-related settings
            println!("Config: {}", name);
            println!("  Max parallel downloads: {}", config.max_parallel_downloads);
            println!("  Validation workers: {}", config.validation_workers);
            println!("  Batch size: {}", config.batch_size);
            println!("  Cache size: {}", config.cache_size);
            println!("  SIMD enabled: {}", config.simd_enabled);
            
            // High performance should have more aggressive settings
            if name == "High Performance" {
                assert!(config.max_parallel_downloads >= 32);
                assert!(config.batch_size >= 512);
                assert!(config.simd_enabled);
            }
            
            // Lightweight should have conservative settings
            if name == "Lightweight" {
                assert!(config.max_parallel_downloads <= 8);
                assert!(config.cache_size <= 2000);
                assert!(config.memory_pool_size <= 512 * 1024 * 1024);
            }
        }
    }

    // Stress tests
    #[actix::test]
    async fn test_sustained_message_load() {
        let config = test_sync_config();
        let sync_actor = SyncActor::new(config).unwrap().start();

        let duration = Duration::from_secs(5);
        let start_time = Instant::now();
        let mut message_count = 0;

        // Send messages for the duration
        while start_time.elapsed() < duration {
            let result = sync_actor.send(GetSyncStatus).await;
            if result.is_ok() {
                message_count += 1;
            }
            
            // Small delay to avoid overwhelming
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let actual_duration = start_time.elapsed();
        let throughput = message_count as f64 / actual_duration.as_secs_f64();
        
        println!("Sustained load: {} messages in {:?} = {:.2} msg/sec", 
                message_count, actual_duration, throughput);
        
        // Should maintain at least 100 msg/sec under sustained load
        assert!(throughput >= 100.0, 
               "Sustained throughput too low: {:.2} msg/sec", throughput);
    }

    #[actix::test]
    async fn test_memory_stability_under_load() {
        // Test that memory usage remains stable under load
        let config = SyncConfig::default();
        let mut sync_actor_obj = SyncActor::new(config).unwrap();

        // Simulate load by adding many performance samples
        let initial_samples = sync_actor_obj.state.metrics.recent_samples.len();
        
        for i in 0..10000 {
            sync_actor_obj.state.add_performance_sample(
                200.0 + (i % 100) as f64,
                15.0 + (i % 10) as f64,
            );
        }

        let final_samples = sync_actor_obj.state.metrics.recent_samples.len();
        
        // Memory usage should be bounded (samples are capped at 100)
        assert!(final_samples <= 100, 
               "Memory usage not bounded: {} samples", final_samples);
        assert!(final_samples > initial_samples);
        
        // Metrics should still be reasonable
        assert!(sync_actor_obj.state.metrics.current_bps > 0.0);
        assert!(sync_actor_obj.state.metrics.average_bps > 0.0);
    }
}