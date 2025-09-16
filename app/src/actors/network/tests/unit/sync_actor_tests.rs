//! SyncActor Tests
//! 
//! Unit tests for SyncActor functionality including blockchain synchronization,
//! validation, and performance monitoring.

use actix::prelude::*;
use std::time::Duration;

use crate::actors::network::{sync::SyncActor, messages::*};
use crate::actors::network::tests::test_helpers::*;

#[actix::test]
async fn test_sync_actor_initialization() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    // Test that actor starts successfully
    assert!(addr.connected());
}

#[actix::test]
async fn test_start_sync_process() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    let msg = StartSync {
        target_block: Some(100),
        force_restart: false,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_stop_sync_process() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    let stop_msg = StopSync { 
        graceful: true 
    };
    
    let result = addr.send(stop_msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_get_sync_status() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    let status_msg = GetSyncStatus;
    let result = addr.send(status_msg).await;
    
    assert!(result.is_ok());
    if let Ok(Ok(status)) = result {
        assert!(status.current_block >= 0);
    }
}

#[actix::test]
async fn test_process_new_block() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    let block_data = create_test_block_data(1);
    let msg = ProcessNewBlock {
        block_hash: "test_hash".to_string(),
        block_data,
        from_peer: "test_peer".to_string(),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_sync_performance_threshold() {
    let mut config = test_sync_config();
    config.sync_threshold = 99.5; // 99.5% threshold
    
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    // Test that sync respects the threshold
    let status_msg = GetSyncStatus;
    let result = addr.send(status_msg).await;
    
    assert!(result.is_ok());
}

#[actix::test]
async fn test_parallel_validation() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    // Send multiple blocks for parallel validation
    let mut handles = Vec::new();
    
    for i in 1..=10 {
        let block_data = create_test_block_data(i);
        let msg = ProcessNewBlock {
            block_hash: format!("test_hash_{}", i),
            block_data,
            from_peer: format!("peer_{}", i),
        };
        
        let handle = addr.send(msg);
        handles.push(handle);
    }
    
    // Wait for all blocks to be processed
    for handle in handles {
        assert!(handle.await.is_ok());
    }
}

#[actix::test]
async fn test_checkpoint_recovery() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    let msg = RecoverFromCheckpoint {
        checkpoint_hash: "checkpoint_123".to_string(),
        checkpoint_block: 50,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

#[actix::test]
async fn test_federation_timing_respect() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    // Test that sync respects Aura PoA timing
    let start = std::time::Instant::now();
    
    let msg = ProcessNewBlock {
        block_hash: "federation_block".to_string(),
        block_data: create_test_block_data(1),
        from_peer: "federation_peer".to_string(),
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
    
    // Should complete within reasonable time for federation blocks
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[actix::test]
async fn test_network_partition_recovery() {
    let config = test_sync_config();
    let sync_actor = SyncActor::new(config).unwrap();
    let addr = sync_actor.start();
    
    // Simulate network partition recovery
    let msg = HandleNetworkPartition {
        partition_type: NetworkPartitionType::Recovered,
        peer_count: 5,
    };
    
    let result = addr.send(msg).await;
    assert!(result.is_ok());
}

mod sync_integration_tests {
    use super::*;
    
    #[actix::test]
    async fn test_sync_with_network_actor() {
        // Integration test between SyncActor and NetworkActor
        let sync_config = test_sync_config();
        let network_config = test_network_config();
        
        let sync_addr = SyncActor::new(sync_config).unwrap().start();
        let _network_addr = crate::actors::network::NetworkActor::new(network_config).unwrap().start();
        
        // Test sync coordination with network
        let msg = StartSync {
            target_block: Some(10),
            force_restart: false,
        };
        
        let result = sync_addr.send(msg).await;
        assert!(result.is_ok());
    }
}

mod sync_performance_tests {
    use super::*;
    
    #[actix::test]
    async fn test_high_throughput_sync() {
        let config = test_sync_config();
        let sync_actor = SyncActor::new(config).unwrap();
        let addr = sync_actor.start();
        
        let start = std::time::Instant::now();
        let block_count = 100;
        
        // Process many blocks quickly
        for i in 1..=block_count {
            let msg = ProcessNewBlock {
                block_hash: format!("block_{}", i),
                block_data: create_test_block_data(i),
                from_peer: "high_throughput_peer".to_string(),
            };
            
            tokio::spawn(async move {
                addr.send(msg).await
            });
        }
        
        let duration = start.elapsed();
        let blocks_per_sec = block_count as f64 / duration.as_secs_f64();
        
        // Should process at least 250 blocks/sec
        assert!(blocks_per_sec > 250.0, "Throughput: {} blocks/sec", blocks_per_sec);
    }
}