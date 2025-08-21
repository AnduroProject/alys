use super::*;
use criterion::{black_box, Criterion, BenchmarkId};
use std::time::{Duration, Instant};
use futures::future::join_all;

// Performance test utilities
pub struct PerformanceTestSuite {
    pub fixture: TestFixture,
}

impl PerformanceTestSuite {
    pub async fn new() -> Self {
        Self {
            fixture: TestFixture::new().await,
        }
    }

    pub async fn setup_utxos(&self, count: usize, amount_each: u64) {
        for _ in 0..count {
            self.fixture.test_bitcoin_rpc.add_unspent(
                create_random_txid(),
                0,
                amount_each,
                6,
                create_test_federation_script(),
            );
        }
        
        self.fixture.bridge_actor.send(RefreshUtxos)
            .await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn bench_pegin_processing_throughput() {
    let suite = PerformanceTestSuite::new().await;
    let test_sizes = vec![10, 50, 100, 500];
    
    for size in test_sizes {
        let start = Instant::now();
        let mut handles = vec![];
        
        for i in 0..size {
            let actor = suite.fixture.bridge_actor.clone();
            let federation_address = suite.fixture.federation_address.clone();
            let evm_address = create_random_h160();
            
            let handle = tokio::spawn(async move {
                let tx = create_deposit_transaction(
                    (i as u64 + 1) * 1_000_000, // 0.01 BTC each with variation
                    evm_address,
                    &federation_address,
                );
                
                actor.send(ProcessPegin {
                    tx,
                    confirmations: 6,
                    deposit_address: federation_address,
                }).await.unwrap()
            });
            
            handles.push(handle);
        }
        
        let results: Vec<_> = join_all(handles).await;
        let duration = start.elapsed();
        
        let successful = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
        let throughput = successful as f64 / duration.as_secs_f64();
        
        println!(
            "Peg-in processing: {} operations in {:?} ({:.2} ops/sec, {}/{} successful)",
            size, duration, throughput, successful, size
        );
        
        // Performance assertions
        assert!(throughput > 5.0, "Should process at least 5 peg-ins per second");
        assert!(successful as f64 / size as f64 > 0.8, "Should have >80% success rate");
    }
}

#[tokio::test]
async fn bench_pegout_processing_throughput() {
    let suite = PerformanceTestSuite::new().await;
    
    // Setup sufficient UTXOs for testing
    suite.setup_utxos(100, 100_000_000).await; // 100 UTXOs of 1 BTC each
    
    let test_sizes = vec![5, 10, 25, 50]; // Smaller sizes due to UTXO constraints
    
    for size in test_sizes {
        let start = Instant::now();
        let mut handles = vec![];
        
        for i in 0..size {
            let actor = suite.fixture.bridge_actor.clone();
            
            let handle = tokio::spawn(async move {
                let burn_event = BurnEvent {
                    tx_hash: create_random_h256(),
                    block_number: 1000,
                    amount: (i as u64 + 1) * 5_000_000, // 0.05 BTC each with variation
                    destination: "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080".to_string(),
                    sender: create_random_h160(),
                };
                
                actor.send(ProcessPegout {
                    burn_event,
                    request_id: format!("bench-pegout-{}", i),
                }).await.unwrap()
            });
            
            handles.push(handle);
        }
        
        let results: Vec<_> = join_all(handles).await;
        let duration = start.elapsed();
        
        let successful = results.iter().filter(|r| {
            match r.as_ref().unwrap() {
                Ok(PegoutResult::Pending(_)) => true,
                _ => false,
            }
        }).count();
        
        let throughput = successful as f64 / duration.as_secs_f64();
        
        println!(
            "Peg-out processing: {} operations in {:?} ({:.2} ops/sec, {}/{} successful)",
            size, duration, throughput, successful, size
        );
        
        // Performance assertions
        assert!(throughput > 1.0, "Should process at least 1 peg-out per second");
        assert!(successful as f64 / size as f64 > 0.6, "Should have >60% success rate");
    }
}

#[tokio::test]
async fn bench_utxo_refresh_performance() {
    let suite = PerformanceTestSuite::new().await;
    let utxo_counts = vec![10, 50, 100, 500, 1000];
    
    for count in utxo_counts {
        // Setup UTXOs
        for _ in 0..count {
            suite.fixture.test_bitcoin_rpc.add_unspent(
                create_random_txid(),
                0,
                rand::random::<u32>() as u64 + 1_000_000, // Random amount > 0.01 BTC
                6,
                create_test_federation_script(),
            );
        }
        
        let start = Instant::now();
        
        let result = suite.fixture.bridge_actor.send(RefreshUtxos)
            .await.unwrap();
        
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "UTXO refresh should succeed");
        
        let throughput = count as f64 / duration.as_secs_f64();
        
        println!(
            "UTXO refresh: {} UTXOs in {:?} ({:.2} UTXOs/sec)",
            count, duration, throughput
        );
        
        // Performance assertions
        assert!(duration < Duration::from_secs(5), "UTXO refresh should complete within 5 seconds");
        assert!(throughput > 50.0, "Should process at least 50 UTXOs per second");
        
        // Clear for next test
        suite.fixture.test_bitcoin_rpc.clear();
    }
}

#[tokio::test]
async fn bench_concurrent_mixed_operations() {
    let suite = PerformanceTestSuite::new().await;
    suite.setup_utxos(200, 100_000_000).await; // 200 UTXOs for pegouts
    
    let operation_counts = vec![20, 50, 100];
    
    for total_ops in operation_counts {
        let pegin_count = total_ops * 2 / 3; // 2/3 peg-ins
        let pegout_count = total_ops / 3;    // 1/3 peg-outs
        
        let start = Instant::now();
        let mut handles = vec![];
        
        // Create peg-in operations
        for i in 0..pegin_count {
            let actor = suite.fixture.bridge_actor.clone();
            let federation_address = suite.fixture.federation_address.clone();
            let evm_address = create_random_h160();
            
            let handle = tokio::spawn(async move {
                let tx = create_deposit_transaction(
                    (i as u64 + 1) * 2_000_000, // 0.02 BTC each
                    evm_address,
                    &federation_address,
                );
                
                actor.send(ProcessPegin {
                    tx,
                    confirmations: 6,
                    deposit_address: federation_address,
                }).await.unwrap()
            });
            
            handles.push(handle);
        }
        
        // Create peg-out operations
        for i in 0..pegout_count {
            let actor = suite.fixture.bridge_actor.clone();
            
            let handle = tokio::spawn(async move {
                let burn_event = BurnEvent {
                    tx_hash: create_random_h256(),
                    block_number: 1000 + i as u64,
                    amount: (i as u64 + 1) * 10_000_000, // 0.1 BTC each
                    destination: "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080".to_string(),
                    sender: create_random_h160(),
                };
                
                actor.send(ProcessPegout {
                    burn_event,
                    request_id: format!("mixed-pegout-{}", i),
                }).await.unwrap()
            });
            
            handles.push(handle);
        }
        
        // Wait for all operations
        let results: Vec<_> = join_all(handles).await;
        let duration = start.elapsed();
        
        let successful = results.iter().filter(|r| {
            match r.as_ref().unwrap() {
                Ok(_) => true,
                Err(_) => false,
            }
        }).count();
        
        let throughput = successful as f64 / duration.as_secs_f64();
        
        println!(
            "Mixed operations: {} total ({} peg-ins, {} peg-outs) in {:?} ({:.2} ops/sec, {}/{} successful)",
            total_ops, pegin_count, pegout_count, duration, throughput, successful, total_ops
        );
        
        // Performance assertions
        assert!(throughput > 3.0, "Should process at least 3 mixed operations per second");
        assert!(successful as f64 / total_ops as f64 > 0.7, "Should have >70% success rate");
        
        // Verify state consistency after load
        let stats = suite.fixture.bridge_actor.send(GetBridgeStats)
            .await.unwrap().unwrap();
        
        assert!(stats.success_rate > 0.0, "Should maintain positive success rate");
    }
}

#[tokio::test]
async fn bench_memory_usage_under_load() {
    let suite = PerformanceTestSuite::new().await;
    
    // Create a baseline
    let initial_stats = suite.fixture.bridge_actor.send(GetBridgeStats)
        .await.unwrap().unwrap();
    
    let load_sizes = vec![100, 500, 1000];
    
    for load_size in load_sizes {
        let start = Instant::now();
        
        // Generate load with many peg-ins
        for i in 0..load_size {
            let evm_address = create_random_h160();
            let tx = suite.fixture.create_test_pegin_tx(
                (i % 100 + 1) * 1_000_000, // Varying amounts
                evm_address,
            );
            
            // Don't await individual operations to create backpressure
            let _ = suite.fixture.bridge_actor.try_send(ProcessPegin {
                tx,
                confirmations: 6,
                deposit_address: suite.fixture.federation_address.clone(),
            });
        }
        
        // Wait a bit for processing
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        let stats = suite.fixture.bridge_actor.send(GetBridgeStats)
            .await.unwrap().unwrap();
        
        let processing_time = start.elapsed();
        
        println!(
            "Memory test with {} operations: processed in {:?}, pending: {}, total processed: {}",
            load_size, processing_time, stats.pending_pegins, stats.total_pegins_processed
        );
        
        // Memory usage assertions
        assert!(stats.pending_pegins < load_size as i64, "Pending operations should be processed");
        assert!(stats.pending_pegins < 1000, "Should not accumulate excessive pending operations");
        
        // Actor should remain responsive
        let response_start = Instant::now();
        let health_check = suite.fixture.bridge_actor.send(GetBridgeStats).await;
        let response_time = response_start.elapsed();
        
        assert!(health_check.is_ok(), "Actor should remain responsive");
        assert!(response_time < Duration::from_secs(1), "Response time should be reasonable");
    }
}

#[tokio::test]
async fn bench_error_handling_performance() {
    let suite = PerformanceTestSuite::new().await;
    
    let error_counts = vec![10, 50, 100];
    
    for error_count in error_counts {
        let start = Instant::now();
        let mut handles = vec![];
        
        // Create operations that will fail
        for i in 0..error_count {
            let actor = suite.fixture.bridge_actor.clone();
            
            let handle = tokio::spawn(async move {
                // Create invalid transaction (no outputs)
                let invalid_tx = Transaction {
                    version: 2,
                    lock_time: 0,
                    input: vec![],
                    output: vec![],
                };
                
                actor.send(ProcessPegin {
                    tx: invalid_tx,
                    confirmations: 6,
                    deposit_address: create_test_federation_address(),
                }).await.unwrap()
            });
            
            handles.push(handle);
        }
        
        let results: Vec<_> = join_all(handles).await;
        let duration = start.elapsed();
        
        let errors = results.iter().filter(|r| r.as_ref().unwrap().is_err()).count();
        let error_throughput = errors as f64 / duration.as_secs_f64();
        
        println!(
            "Error handling: {} error operations in {:?} ({:.2} errors/sec, {}/{} failed as expected)",
            error_count, duration, error_throughput, errors, error_count
        );
        
        // Error handling performance assertions
        assert!(error_throughput > 10.0, "Should handle at least 10 errors per second");
        assert!(errors as f64 / error_count as f64 > 0.9, "Should properly fail >90% of invalid operations");
        
        // Verify actor is still responsive after errors
        let stats = suite.fixture.bridge_actor.send(GetBridgeStats)
            .await.unwrap().unwrap();
        
        assert!(stats.success_rate >= 0.0, "Stats should remain accessible after errors");
    }
}

#[tokio::test]
async fn bench_stats_query_performance() {
    let suite = PerformanceTestSuite::new().await;
    
    // Create some baseline operations
    for i in 0..50 {
        let evm_address = create_random_h160();
        let tx = suite.fixture.create_test_pegin_tx((i + 1) * 1_000_000, evm_address);
        
        let _ = suite.fixture.bridge_actor.send(ProcessPegin {
            tx,
            confirmations: 6,
            deposit_address: suite.fixture.federation_address.clone(),
        }).await.unwrap();
    }
    
    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    let query_counts = vec![10, 50, 100, 500];
    
    for query_count in query_counts {
        let start = Instant::now();
        let mut handles = vec![];
        
        for _ in 0..query_count {
            let actor = suite.fixture.bridge_actor.clone();
            
            let handle = tokio::spawn(async move {
                actor.send(GetBridgeStats).await.unwrap()
            });
            
            handles.push(handle);
        }
        
        let results: Vec<_> = join_all(handles).await;
        let duration = start.elapsed();
        
        let successful = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
        let query_throughput = successful as f64 / duration.as_secs_f64();
        
        println!(
            "Stats queries: {} queries in {:?} ({:.2} queries/sec, {}/{} successful)",
            query_count, duration, query_throughput, successful, query_count
        );
        
        // Query performance assertions
        assert!(query_throughput > 50.0, "Should handle at least 50 stats queries per second");
        assert!(successful == query_count, "All stats queries should succeed");
        assert!(duration < Duration::from_secs(2), "Queries should complete within 2 seconds");
    }
}

#[tokio::test]
async fn bench_startup_and_shutdown_performance() {
    let startup_times = vec![];
    let iterations = 10;
    
    for i in 0..iterations {
        let start = Instant::now();
        
        // Create new actor instance
        let config = BridgeConfig::test_config();
        let federation_address = create_test_federation_address();
        let federation_script = create_test_federation_script();
        let bitcoin_rpc = Arc::new(MockBitcoinRpc::new());
        
        let bridge_actor = BridgeActor::new(
            config,
            federation_address,
            federation_script,
            bitcoin_rpc,
        ).unwrap();
        
        let bridge_addr = bridge_actor.start();
        
        // Wait for startup
        let _ = bridge_addr.send(GetBridgeStats).await.unwrap();
        
        let startup_duration = start.elapsed();
        
        println!("Startup iteration {}: {:?}", i, startup_duration);
        
        // Shutdown
        let shutdown_start = Instant::now();
        drop(bridge_addr); // Trigger shutdown
        let shutdown_duration = shutdown_start.elapsed();
        
        println!("Shutdown iteration {}: {:?}", i, shutdown_duration);
        
        // Performance assertions
        assert!(startup_duration < Duration::from_secs(2), "Startup should complete within 2 seconds");
        assert!(shutdown_duration < Duration::from_secs(1), "Shutdown should complete within 1 second");
    }
}

// Criterion benchmarks for detailed performance measurement
pub fn criterion_benchmarks(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("pegin_processing", |b| {
        let suite = rt.block_on(PerformanceTestSuite::new());
        
        b.iter(|| {
            rt.block_on(async {
                let evm_address = create_random_h160();
                let tx = suite.fixture.create_test_pegin_tx(
                    black_box(100_000_000), 
                    black_box(evm_address)
                );
                
                suite.fixture.bridge_actor.send(ProcessPegin {
                    tx,
                    confirmations: 6,
                    deposit_address: suite.fixture.federation_address.clone(),
                }).await.unwrap()
            })
        })
    });
    
    c.bench_function("stats_query", |b| {
        let suite = rt.block_on(PerformanceTestSuite::new());
        
        b.iter(|| {
            rt.block_on(async {
                suite.fixture.bridge_actor.send(GetBridgeStats).await.unwrap()
            })
        })
    });
}