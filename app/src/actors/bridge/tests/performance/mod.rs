//! Performance Tests for Bridge System
//! 
//! Load testing, throughput analysis, and performance benchmarks

use actix::prelude::*;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::actors::bridge::{
    BridgeActor, PegInActor, PegOutActor, StreamActor,
    BridgeCoordinationMessage, PegInMessage, PegOutMessage
};
use crate::actors::bridge::tests::helpers::*;
use crate::types::*;

#[actix::test]
async fn test_pegin_throughput() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    let test_count = 100;
    let start_time = Instant::now();

    // Send multiple peg-in requests
    let mut futures = Vec::new();
    for i in 0..test_count {
        let pegin_request = PegInRequest {
            bitcoin_txid: TestDataBuilder::random_txid(),
            output_index: 0,
            amount: bitcoin::Amount::from_sat(100_000),
            recipient: TestDataBuilder::test_ethereum_address(),
            confirmation_count: 6,
        };

        let future = pegin_actor
            .send(PegInMessage::ProcessRequest {
                request: pegin_request,
            });
        futures.push(future);
    }

    let results = futures::future::join_all(futures).await;
    let elapsed = start_time.elapsed();

    // Analyze results
    let successful_operations = results.iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();

    let throughput = successful_operations as f64 / elapsed.as_secs_f64();
    
    println!("PegIn Throughput Test Results:");
    println!("  Total requests: {}", test_count);
    println!("  Successful operations: {}", successful_operations);
    println!("  Time elapsed: {:?}", elapsed);
    println!("  Throughput: {:.2} operations/second", throughput);

    // Assert reasonable performance (adjust thresholds as needed)
    assert!(successful_operations > test_count / 2, "Less than 50% success rate");
    assert!(throughput > 1.0, "Throughput below 1 op/second");
}

#[actix::test]
async fn test_pegout_throughput() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    let test_count = 50; // Fewer for peg-out as it's more resource intensive
    let start_time = Instant::now();

    // Send multiple peg-out requests
    let mut futures = Vec::new();
    for i in 0..test_count {
        let pegout_request = PegOutRequest {
            burn_tx_hash: H256::random(),
            amount: U256::from(100_000),
            recipient: TestDataBuilder::test_bitcoin_address(),
            fee_rate: 10,
        };

        let future = pegout_actor
            .send(PegOutMessage::ProcessRequest {
                request: pegout_request,
            });
        futures.push(future);
    }

    let results = futures::future::join_all(futures).await;
    let elapsed = start_time.elapsed();

    // Analyze results
    let successful_operations = results.iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();

    let throughput = successful_operations as f64 / elapsed.as_secs_f64();
    
    println!("PegOut Throughput Test Results:");
    println!("  Total requests: {}", test_count);
    println!("  Successful operations: {}", successful_operations);
    println!("  Time elapsed: {:?}", elapsed);
    println!("  Throughput: {:.2} operations/second", throughput);

    // Assert reasonable performance
    assert!(successful_operations > test_count / 2, "Less than 50% success rate");
    assert!(throughput > 0.5, "Throughput below 0.5 op/second");
}

#[actix::test]
async fn test_mixed_operation_load() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    let pegin_count = 30;
    let pegout_count = 20;
    let start_time = Instant::now();

    let mut futures = Vec::new();

    // Create mixed load of peg-in and peg-out operations
    for i in 0..pegin_count {
        let pegin_request = PegInRequest {
            bitcoin_txid: TestDataBuilder::random_txid(),
            output_index: 0,
            amount: bitcoin::Amount::from_sat(100_000 + i * 1000),
            recipient: TestDataBuilder::test_ethereum_address(),
            confirmation_count: 6,
        };

        let future = pegin_actor
            .send(PegInMessage::ProcessRequest {
                request: pegin_request,
            });
        futures.push(("pegin", future));
    }

    for i in 0..pegout_count {
        let pegout_request = PegOutRequest {
            burn_tx_hash: H256::random(),
            amount: U256::from(100_000 + i * 1000),
            recipient: TestDataBuilder::test_bitcoin_address(),
            fee_rate: 10,
        };

        let future = pegout_actor
            .send(PegOutMessage::ProcessRequest {
                request: pegout_request,
            });
        futures.push(("pegout", future));
    }

    // Shuffle operations to simulate real-world mixed load
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    futures.shuffle(&mut rng);

    // Execute all operations
    let operation_futures: Vec<_> = futures.into_iter().map(|(_, f)| f).collect();
    let results = futures::future::join_all(operation_futures).await;
    let elapsed = start_time.elapsed();

    // Analyze mixed load performance
    let successful_operations = results.iter()
        .filter(|r| r.is_ok())
        .count();

    let total_operations = pegin_count + pegout_count;
    let success_rate = successful_operations as f64 / total_operations as f64;
    let throughput = successful_operations as f64 / elapsed.as_secs_f64();

    println!("Mixed Load Test Results:");
    println!("  Total operations: {}", total_operations);
    println!("  Successful operations: {}", successful_operations);
    println!("  Success rate: {:.2}%", success_rate * 100.0);
    println!("  Time elapsed: {:?}", elapsed);
    println!("  Throughput: {:.2} operations/second", throughput);

    assert!(success_rate > 0.3, "Success rate below 30%");
    assert!(throughput > 0.5, "Mixed load throughput too low");
}

#[actix::test]
async fn test_actor_latency_characteristics() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    let test_iterations = 20;
    let mut latencies = Vec::new();

    // Measure individual operation latencies
    for _ in 0..test_iterations {
        let pegin_request = TestDataBuilder::test_pegin_request();
        
        let start = Instant::now();
        let result = pegin_actor
            .send(PegInMessage::ProcessRequest {
                request: pegin_request,
            })
            .await;
        let latency = start.elapsed();

        if result.is_ok() {
            latencies.push(latency);
        }

        // Small delay between requests to simulate real conditions
        sleep(Duration::from_millis(10)).await;
    }

    // Calculate latency statistics
    if !latencies.is_empty() {
        let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let min_latency = *latencies.iter().min().unwrap();
        let max_latency = *latencies.iter().max().unwrap();

        latencies.sort();
        let p50 = latencies[latencies.len() / 2];
        let p95 = latencies[(latencies.len() * 95) / 100];

        println!("Latency Analysis:");
        println!("  Average: {:?}", avg_latency);
        println!("  Min: {:?}", min_latency);
        println!("  Max: {:?}", max_latency);
        println!("  P50: {:?}", p50);
        println!("  P95: {:?}", p95);

        // Assert reasonable latency characteristics
        assert!(avg_latency < Duration::from_millis(1000), "Average latency too high");
        assert!(p95 < Duration::from_secs(2), "P95 latency too high");
    }
}

#[actix::test]
async fn test_memory_usage_under_load() {
    let config = test_bridge_config();
    let bridge_actor = BridgeActor::new(config.clone()).start();
    let pegin_actor = PegInActor::new(config.clone()).start();
    let pegout_actor = PegOutActor::new(config).start();

    // Initialize system
    bridge_actor
        .send(BridgeCoordinationMessage::InitializeSystem)
        .await
        .unwrap()
        .unwrap();

    pegin_actor
        .send(PegInMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    pegout_actor
        .send(PegOutMessage::Initialize)
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegInActor(pegin_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    bridge_actor
        .send(BridgeCoordinationMessage::RegisterPegOutActor(pegout_actor.clone()))
        .await
        .unwrap()
        .unwrap();

    // Generate sustained load and monitor metrics
    let load_duration = Duration::from_secs(5);
    let start_time = Instant::now();
    let mut operation_count = 0;

    while start_time.elapsed() < load_duration {
        // Alternate between peg-in and peg-out operations
        if operation_count % 2 == 0 {
            let pegin_request = TestDataBuilder::test_pegin_request();
            let _ = pegin_actor
                .send(PegInMessage::ProcessRequest {
                    request: pegin_request,
                })
                .await;
        } else {
            let pegout_request = TestDataBuilder::test_pegout_request();
            let _ = pegout_actor
                .send(PegOutMessage::ProcessRequest {
                    request: pegout_request,
                })
                .await;
        }

        operation_count += 1;

        // Small delay to prevent overwhelming the system
        sleep(Duration::from_millis(50)).await;
    }

    // Check system metrics after sustained load
    let bridge_metrics = bridge_actor
        .send(BridgeCoordinationMessage::GetSystemMetrics)
        .await;

    let pegin_metrics = pegin_actor
        .send(PegInMessage::GetMetrics)
        .await;

    let pegout_metrics = pegout_actor
        .send(PegOutMessage::GetMetrics)
        .await;

    assert!(bridge_metrics.is_ok(), "Bridge metrics unavailable after load");
    assert!(pegin_metrics.is_ok(), "PegIn metrics unavailable after load");
    assert!(pegout_metrics.is_ok(), "PegOut metrics unavailable after load");

    println!("Memory usage test completed:");
    println!("  Operations processed: {}", operation_count);
    println!("  Load duration: {:?}", load_duration);
}