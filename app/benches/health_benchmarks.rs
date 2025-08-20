//! Performance Benchmarks for Phase 5: Health Monitoring & Shutdown
//! 
//! Comprehensive performance testing using Criterion.rs to measure and track
//! performance characteristics of the health monitoring and shutdown systems.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

// Import health monitoring components
use app::actors::foundation::health::*;
use app::actors::foundation::constants::health;
use actix::{Actor, System};

/// Benchmark health monitor creation and configuration
fn bench_health_monitor_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_monitor_creation");
    
    group.bench_function("default_config", |b| {
        b.iter(|| {
            let config = HealthMonitorConfig::default();
            black_box(HealthMonitor::new(config));
        });
    });
    
    group.bench_function("custom_config", |b| {
        b.iter(|| {
            let config = HealthMonitorConfig {
                default_check_interval: Duration::from_secs(30),
                critical_check_interval: Duration::from_secs(10),
                check_timeout: Duration::from_secs(5),
                failure_threshold: 5,
                recovery_threshold: 2,
                max_history_entries: 500,
                detailed_reporting: true,
                enable_auto_recovery: true,
                blockchain_aware: true,
            };
            black_box(HealthMonitor::new(config));
        });
    });
    
    group.bench_function("blockchain_optimized_config", |b| {
        b.iter(|| {
            let config = HealthMonitorConfig {
                default_check_interval: health::DEFAULT_HEALTH_CHECK_INTERVAL,
                critical_check_interval: health::CRITICAL_HEALTH_CHECK_INTERVAL,
                check_timeout: Duration::from_millis(500), // Faster for blockchain
                failure_threshold: 3,
                recovery_threshold: 1,
                max_history_entries: 1000,
                detailed_reporting: false, // Reduced overhead
                enable_auto_recovery: true,
                blockchain_aware: true,
            };
            black_box(HealthMonitor::new(config));
        });
    });
    
    group.finish();
}

/// Benchmark actor registration performance
fn bench_actor_registration(c: &mut Criterion) {
    let mut group = c.benchmark_group("actor_registration");
    
    // Test different registration loads
    for actor_count in [1, 10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*actor_count as u64));
        group.bench_with_input(
            BenchmarkId::new("register_actors", actor_count),
            actor_count,
            |b, &actor_count| {
                let rt = Runtime::new().unwrap();
                b.to_async(&rt).iter(|| async {
                    let health_monitor = HealthMonitor::new(HealthMonitorConfig::default());
                    let addr = health_monitor.start();
                    
                    let start = Instant::now();
                    
                    for i in 0..actor_count {
                        let register_msg = RegisterActor {
                            name: format!("bench_actor_{}", i),
                            priority: match i % 4 {
                                0 => ActorPriority::Critical,
                                1 => ActorPriority::High,
                                2 => ActorPriority::Normal,
                                _ => ActorPriority::Background,
                            },
                            check_interval: Some(Duration::from_secs(60)),
                            recovery_strategy: RecoveryStrategy::Restart,
                            custom_check: None,
                        };
                        
                        let _ = addr.send(register_msg).await.unwrap().unwrap();
                    }
                    
                    black_box(start.elapsed());
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark health check protocol performance
fn bench_health_check_protocol(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_check_protocol");
    
    group.bench_function("ping_message_creation", |b| {
        b.iter(|| {
            let ping = PingMessage {
                sender_name: "HealthMonitor".to_string(),
                timestamp: Instant::now(),
                sequence_number: black_box(12345),
                metadata: HashMap::new(),
            };
            black_box(ping);
        });
    });
    
    group.bench_function("pong_response_creation", |b| {
        let ping_time = Instant::now();
        b.iter(|| {
            let pong = PongResponse {
                responder_name: "TestActor".to_string(),
                ping_timestamp: ping_time,
                pong_timestamp: Instant::now(),
                sequence_number: black_box(12345),
                health_status: BasicHealthStatus::Healthy,
                metadata: HashMap::new(),
            };
            black_box(pong);
        });
    });
    
    group.bench_function("health_check_response_processing", |b| {
        b.iter(|| {
            let response = HealthCheckResponse {
                actor_name: "bench_actor".to_string(),
                success: true,
                response_time: Duration::from_millis(black_box(50)),
                timestamp: Instant::now(),
                metadata: HashMap::new(),
                error: None,
            };
            black_box(response);
        });
    });
    
    // Benchmark ping-pong round trip simulation
    group.bench_function("ping_pong_round_trip", |b| {
        b.iter(|| {
            let ping_start = Instant::now();
            
            let ping = PingMessage {
                sender_name: "HealthMonitor".to_string(),
                timestamp: ping_start,
                sequence_number: 1,
                metadata: HashMap::new(),
            };
            
            // Simulate processing delay
            std::thread::sleep(Duration::from_micros(100));
            
            let pong = PongResponse {
                responder_name: "TestActor".to_string(),
                ping_timestamp: ping.timestamp,
                pong_timestamp: Instant::now(),
                sequence_number: ping.sequence_number,
                health_status: BasicHealthStatus::Healthy,
                metadata: HashMap::new(),
            };
            
            let total_time = pong.pong_timestamp.duration_since(pong.ping_timestamp);
            black_box(total_time);
        });
    });
    
    group.finish();
}

/// Benchmark system health calculation performance
fn bench_system_health_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("system_health_calculation");
    
    // Test with different numbers of monitored actors
    for actor_count in [10, 50, 100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("calculate_health_score", actor_count),
            actor_count,
            |b, &actor_count| {
                // Create a health monitor with many registered actors
                let rt = Runtime::new().unwrap();
                b.to_async(&rt).iter(|| async {
                    let health_monitor = HealthMonitor::new(HealthMonitorConfig::default());
                    let addr = health_monitor.start();
                    
                    // Register actors with mixed health statuses
                    for i in 0..actor_count {
                        let register_msg = RegisterActor {
                            name: format!("health_calc_actor_{}", i),
                            priority: ActorPriority::Normal,
                            check_interval: Some(Duration::from_secs(300)), // Long interval
                            recovery_strategy: RecoveryStrategy::Restart,
                            custom_check: None,
                        };
                        let _ = addr.send(register_msg).await.unwrap().unwrap();
                    }
                    
                    // Measure time to get system health
                    let start = Instant::now();
                    let system_health = addr.send(GetSystemHealth).await.unwrap();
                    let calculation_time = start.elapsed();
                    
                    black_box((system_health, calculation_time));
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark health report generation performance
fn bench_health_report_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_report_generation");
    
    // Test different report complexities
    for actor_count in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*actor_count as u64));
        group.bench_with_input(
            BenchmarkId::new("generate_detailed_report", actor_count),
            actor_count,
            |b, &actor_count| {
                let rt = Runtime::new().unwrap();
                b.to_async(&rt).iter(|| async {
                    let health_monitor = HealthMonitor::new(HealthMonitorConfig::default());
                    let addr = health_monitor.start();
                    
                    // Register actors with different priorities and histories
                    for i in 0..actor_count {
                        let register_msg = RegisterActor {
                            name: format!("report_actor_{}", i),
                            priority: match i % 4 {
                                0 => ActorPriority::Critical,
                                1 => ActorPriority::High,
                                2 => ActorPriority::Normal,
                                _ => ActorPriority::Background,
                            },
                            check_interval: Some(Duration::from_secs(300)),
                            recovery_strategy: RecoveryStrategy::Restart,
                            custom_check: None,
                        };
                        let _ = addr.send(register_msg).await.unwrap().unwrap();
                    }
                    
                    // Generate detailed report
                    let start = Instant::now();
                    let report = addr.send(GetHealthReport { 
                        include_details: true 
                    }).await.unwrap();
                    let generation_time = start.elapsed();
                    
                    black_box((report, generation_time));
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark shutdown coordinator performance
fn bench_shutdown_coordinator(c: &mut Criterion) {
    let mut group = c.benchmark_group("shutdown_coordinator");
    
    group.bench_function("coordinator_creation", |b| {
        b.iter(|| {
            let config = ShutdownConfig::default();
            black_box(ShutdownCoordinator::new(config));
        });
    });
    
    // Benchmark shutdown order calculation
    group.bench_function("shutdown_order_calculation", |b| {
        let coordinator = ShutdownCoordinator::new(ShutdownConfig::default());
        b.iter(|| {
            let priority = black_box(ActorPriority::Normal);
            let dependencies = black_box(vec![
                "dep1".to_string(),
                "dep2".to_string(),
                "dep3".to_string(),
            ]);
            let order = coordinator.calculate_shutdown_order(&priority, &dependencies);
            black_box(order);
        });
    });
    
    // Benchmark actor registration for shutdown
    for actor_count in [10, 50, 100, 200].iter() {
        group.bench_with_input(
            BenchmarkId::new("register_shutdown_actors", actor_count),
            actor_count,
            |b, &actor_count| {
                let rt = Runtime::new().unwrap();
                b.to_async(&rt).iter(|| async {
                    let coordinator = ShutdownCoordinator::new(ShutdownConfig::default());
                    let addr = coordinator.start();
                    
                    let start = Instant::now();
                    
                    for i in 0..actor_count {
                        let register_msg = RegisterForShutdown {
                            actor_name: format!("shutdown_bench_actor_{}", i),
                            priority: ActorPriority::Normal,
                            dependencies: if i > 0 {
                                vec![format!("shutdown_bench_actor_{}", i - 1)]
                            } else {
                                vec![]
                            },
                            timeout: Some(Duration::from_millis(100)),
                        };
                        let _ = addr.send(register_msg).await.unwrap().unwrap();
                    }
                    
                    black_box(start.elapsed());
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark shutdown execution performance
fn bench_shutdown_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("shutdown_execution");
    
    // Test shutdown execution with different actor counts
    for actor_count in [5, 10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("execute_shutdown", actor_count),
            actor_count,
            |b, &actor_count| {
                let rt = Runtime::new().unwrap();
                b.to_async(&rt).iter(|| async {
                    let coordinator = ShutdownCoordinator::new(ShutdownConfig::default());
                    let addr = coordinator.start();
                    
                    // Register actors
                    for i in 0..actor_count {
                        let register_msg = RegisterForShutdown {
                            actor_name: format!("exec_bench_actor_{}", i),
                            priority: ActorPriority::Normal,
                            dependencies: vec![],
                            timeout: Some(Duration::from_millis(50)), // Fast shutdown
                        };
                        let _ = addr.send(register_msg).await.unwrap().unwrap();
                    }
                    
                    // Measure shutdown execution time
                    let start = Instant::now();
                    
                    let shutdown_msg = InitiateShutdown {
                        reason: "Benchmark shutdown".to_string(),
                        timeout: Some(Duration::from_secs(30)),
                    };
                    let _ = addr.send(shutdown_msg).await.unwrap().unwrap();
                    
                    // Wait for shutdown to complete
                    let mut attempts = 0;
                    loop {
                        let progress = addr.send(GetShutdownProgress).await.unwrap();
                        if progress.progress_percentage >= 100.0 || attempts > 100 {
                            break;
                        }
                        attempts += 1;
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    
                    let execution_time = start.elapsed();
                    black_box(execution_time);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    group.bench_function("health_history_management", |b| {
        let rt = Runtime::new().unwrap();
        b.to_async(&rt).iter(|| async {
            let mut config = HealthMonitorConfig::default();
            config.max_history_entries = 100; // Limit for benchmark
            
            let health_monitor = HealthMonitor::new(config);
            let addr = health_monitor.start();
            
            // Register actor
            let register_msg = RegisterActor {
                name: "memory_bench_actor".to_string(),
                priority: ActorPriority::Normal,
                check_interval: Some(Duration::from_millis(10)),
                recovery_strategy: RecoveryStrategy::Restart,
                custom_check: None,
            };
            let _ = addr.send(register_msg).await.unwrap().unwrap();
            
            // Generate many health checks to test memory management
            for _ in 0..200 {
                let health_check_msg = TriggerHealthCheck {
                    actor_name: "memory_bench_actor".to_string(),
                };
                let _ = addr.send(health_check_msg).await.unwrap().unwrap();
            }
            
            // Wait for processing
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Get final report
            let report = addr.send(GetHealthReport { 
                include_details: true 
            }).await.unwrap();
            
            black_box(report);
        });
    });
    
    group.finish();
}

/// Benchmark blockchain-specific optimizations
fn bench_blockchain_optimizations(c: &mut Criterion) {
    let mut group = c.benchmark_group("blockchain_optimizations");
    
    // Test blockchain timing constraints (2-second block interval)
    group.bench_function("block_interval_health_check", |b| {
        let rt = Runtime::new().unwrap();
        b.to_async(&rt).iter(|| async {
            let mut config = HealthMonitorConfig::default();
            config.blockchain_aware = true;
            config.critical_check_interval = Duration::from_millis(500); // Under block interval
            
            let health_monitor = HealthMonitor::new(config);
            let addr = health_monitor.start();
            
            // Register critical blockchain actors
            let blockchain_actors = vec![
                ("chain_actor", ActorPriority::Critical),
                ("consensus_actor", ActorPriority::Critical),
                ("mining_actor", ActorPriority::High),
            ];
            
            for (name, priority) in blockchain_actors {
                let register_msg = RegisterActor {
                    name: name.to_string(),
                    priority,
                    check_interval: None,
                    recovery_strategy: RecoveryStrategy::Restart,
                    custom_check: None,
                };
                let _ = addr.send(register_msg).await.unwrap().unwrap();
            }
            
            // Measure health check under blockchain timing constraints
            let start = Instant::now();
            
            // Trigger health checks for all critical actors
            for (name, _) in &[
                ("chain_actor", ActorPriority::Critical),
                ("consensus_actor", ActorPriority::Critical),
            ] {
                let health_check_msg = TriggerHealthCheck {
                    actor_name: name.to_string(),
                };
                let _ = addr.send(health_check_msg).await.unwrap().unwrap();
            }
            
            let check_time = start.elapsed();
            
            // Should complete well under 2-second block interval
            assert!(check_time < Duration::from_millis(100));
            
            black_box(check_time);
        });
    });
    
    group.bench_function("federation_health_coordination", |b| {
        let rt = Runtime::new().unwrap();
        b.to_async(&rt).iter(|| async {
            let config = HealthMonitorConfig::default();
            let health_monitor = HealthMonitor::new(config);
            let addr = health_monitor.start();
            
            // Simulate federation nodes
            let federation_nodes = vec![
                "federation_node_1",
                "federation_node_2",
                "federation_node_3",
                "federation_node_4",
            ];
            
            for node_name in &federation_nodes {
                let register_msg = RegisterActor {
                    name: node_name.to_string(),
                    priority: ActorPriority::Critical,
                    check_interval: Some(Duration::from_millis(250)), // 4x per second
                    recovery_strategy: RecoveryStrategy::Restart,
                    custom_check: None,
                };
                let _ = addr.send(register_msg).await.unwrap().unwrap();
            }
            
            // Simulate concurrent federation health monitoring
            let start = Instant::now();
            
            let tasks: Vec<_> = federation_nodes.iter().map(|node_name| {
                let addr_clone = addr.clone();
                let node_name = node_name.to_string();
                tokio::spawn(async move {
                    let health_check_msg = TriggerHealthCheck { actor_name: node_name };
                    addr_clone.send(health_check_msg).await
                })
            }).collect();
            
            let _results = futures::future::join_all(tasks).await;
            let coordination_time = start.elapsed();
            
            black_box(coordination_time);
        });
    });
    
    group.finish();
}

// Define criterion groups
criterion_group!(
    health_benches,
    bench_health_monitor_creation,
    bench_actor_registration,
    bench_health_check_protocol,
    bench_system_health_calculation,
    bench_health_report_generation
);

criterion_group!(
    shutdown_benches,
    bench_shutdown_coordinator,
    bench_shutdown_execution
);

criterion_group!(
    performance_benches,
    bench_memory_usage,
    bench_blockchain_optimizations
);

criterion_main!(health_benches, shutdown_benches, performance_benches);