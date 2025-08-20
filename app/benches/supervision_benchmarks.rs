//! Performance Benchmarks for Phase 2: Supervision & Restart Logic
//! 
//! Comprehensive performance benchmarking using Criterion.rs for supervision
//! system components, restart delay calculations, failure handling, and
//! integration with Alys blockchain timing requirements.

use app::actors::foundation::{
    ActorSystemConfig, EnhancedSupervision, ExponentialBackoffConfig, 
    FixedDelayConfig, ActorFailureInfo, ActorFailureType, RestartAttemptInfo,
    RestartReason, RestartStrategy, ActorPriority, SupervisedActorConfig,
    FailurePatternDetector
};
use criterion::{
    criterion_group, criterion_main, Criterion, BenchmarkId, Throughput,
    black_box, BatchSize
};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Benchmark supervision system initialization
fn bench_supervision_initialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("supervision_initialization");
    
    group.bench_function("new_supervision_system", |b| {
        b.iter(|| {
            let config = ActorSystemConfig::development();
            black_box(EnhancedSupervision::new(config))
        })
    });
    
    group.bench_function("new_supervision_with_production_config", |b| {
        b.iter(|| {
            let config = ActorSystemConfig::production();
            black_box(EnhancedSupervision::new(config))
        })
    });
    
    group.finish();
}

/// Benchmark exponential backoff delay calculations
fn bench_exponential_backoff_calculations(c: &mut Criterion) {
    let mut group = c.benchmark_group("exponential_backoff_calculations");
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = ActorSystemConfig::development();
    let supervision = EnhancedSupervision::new(config);
    
    let backoff_configs = vec![
        ("fast_backoff", ExponentialBackoffConfig {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            multiplier: 1.5,
            max_attempts: Some(5),
            jitter: 0.0,
            align_to_block_boundary: false,
            respect_consensus_timing: false,
        }),
        ("standard_backoff", ExponentialBackoffConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_attempts: Some(10),
            jitter: 0.1,
            align_to_block_boundary: false,
            respect_consensus_timing: false,
        }),
        ("blockchain_aware_backoff", ExponentialBackoffConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_attempts: Some(10),
            jitter: 0.1,
            align_to_block_boundary: true,
            respect_consensus_timing: true,
        }),
    ];
    
    for (name, backoff_config) in backoff_configs {
        group.bench_with_input(
            BenchmarkId::new("single_calculation", name),
            &backoff_config,
            |b, config| {
                b.to_async(&rt).iter(|| async {
                    black_box(
                        supervision.calculate_exponential_backoff_delay(
                            "benchmark_actor",
                            3, // 3rd attempt
                            config
                        ).await.unwrap()
                    )
                })
            }
        );
    }
    
    // Benchmark calculation performance across multiple attempts
    group.bench_function("multiple_attempts_calculation", |b| {
        let config = &backoff_configs[1].1; // Standard config
        b.to_async(&rt).iter(|| async {
            for attempt in 1..=10 {
                black_box(
                    supervision.calculate_exponential_backoff_delay(
                        "benchmark_actor",
                        attempt,
                        config
                    ).await.unwrap()
                );
            }
        })
    });
    
    group.finish();
}

/// Benchmark fixed delay calculations
fn bench_fixed_delay_calculations(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed_delay_calculations");
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = ActorSystemConfig::development();
    let supervision = EnhancedSupervision::new(config);
    
    let delay_configs = vec![
        ("simple_fixed", FixedDelayConfig {
            delay: Duration::from_secs(1),
            max_attempts: Some(5),
            progressive_increment: None,
            max_delay: None,
            blockchain_aligned: false,
        }),
        ("progressive_fixed", FixedDelayConfig {
            delay: Duration::from_secs(1),
            max_attempts: Some(10),
            progressive_increment: Some(Duration::from_millis(500)),
            max_delay: Some(Duration::from_secs(10)),
            blockchain_aligned: false,
        }),
        ("blockchain_aligned_fixed", FixedDelayConfig {
            delay: Duration::from_secs(1),
            max_attempts: Some(5),
            progressive_increment: None,
            max_delay: None,
            blockchain_aligned: true,
        }),
    ];
    
    for (name, delay_config) in delay_configs {
        group.bench_with_input(
            BenchmarkId::new("calculation", name),
            &delay_config,
            |b, config| {
                b.to_async(&rt).iter(|| async {
                    black_box(
                        supervision.calculate_fixed_delay(
                            "benchmark_actor",
                            3,
                            config
                        ).await.unwrap()
                    )
                })
            }
        );
    }
    
    group.finish();
}

/// Benchmark actor failure handling
fn bench_actor_failure_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("actor_failure_handling");
    group.throughput(Throughput::Elements(1));
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = ActorSystemConfig::development();
    let supervision = EnhancedSupervision::new(config);
    
    let failure_types = vec![
        ("panic_failure", ActorFailureType::Panic { backtrace: None }),
        ("timeout_failure", ActorFailureType::Timeout { duration: Duration::from_secs(5) }),
        ("consensus_failure", ActorFailureType::ConsensusFailure { 
            error_code: "INVALID_SIGNATURE".to_string() 
        }),
        ("network_failure", ActorFailureType::NetworkFailure { 
            peer_id: Some("peer_123".to_string()),
            error: "Connection timeout".to_string(),
        }),
        ("governance_failure", ActorFailureType::GovernanceFailure {
            event_type: "PROPOSAL_VALIDATION".to_string(),
            error: "Invalid proposal".to_string(),
        }),
    ];
    
    for (name, failure_type) in failure_types {
        let failure_info = ActorFailureInfo {
            timestamp: SystemTime::now(),
            failure_type: failure_type.clone(),
            message: format!("Benchmark failure: {}", name),
            context: HashMap::new(),
            escalate: false,
        };
        
        group.bench_with_input(
            BenchmarkId::new("handle_failure", name),
            &failure_info,
            |b, failure| {
                b.to_async(&rt).iter(|| async {
                    let actor_name = format!("benchmark_actor_{}", rand::random::<u32>());
                    black_box(
                        supervision.handle_actor_failure(&actor_name, failure.clone()).await
                    )
                })
            }
        );
    }
    
    group.finish();
}

/// Benchmark restart attempt tracking
fn bench_restart_attempt_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("restart_attempt_tracking");
    group.throughput(Throughput::Elements(1));
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = ActorSystemConfig::development();
    let supervision = EnhancedSupervision::new(config);
    
    // Benchmark single restart attempt tracking
    group.bench_function("single_attempt_tracking", |b| {
        b.iter_batched(
            || {
                RestartAttemptInfo {
                    attempt_id: Uuid::new_v4(),
                    attempt_number: 1,
                    timestamp: SystemTime::now(),
                    reason: RestartReason::ActorPanic,
                    delay: Duration::from_millis(100),
                    strategy: RestartStrategy::default(),
                    success: Some(true),
                    duration: Some(Duration::from_millis(50)),
                    failure_info: None,
                    context: HashMap::new(),
                }
            },
            |attempt_info| {
                rt.block_on(async {
                    let actor_name = format!("benchmark_actor_{}", rand::random::<u32>());
                    black_box(
                        supervision.track_restart_attempt(&actor_name, attempt_info).await
                    )
                })
            },
            BatchSize::SmallInput
        )
    });
    
    // Benchmark batch restart attempt tracking
    group.bench_function("batch_attempt_tracking", |b| {
        b.iter_batched(
            || {
                (0..100).map(|i| {
                    RestartAttemptInfo {
                        attempt_id: Uuid::new_v4(),
                        attempt_number: i % 10 + 1,
                        timestamp: SystemTime::now(),
                        reason: RestartReason::ActorPanic,
                        delay: Duration::from_millis(100 * (i % 5 + 1) as u64),
                        strategy: RestartStrategy::default(),
                        success: Some(i % 3 == 0), // 1/3 success rate
                        duration: Some(Duration::from_millis(50)),
                        failure_info: None,
                        context: HashMap::new(),
                    }
                }).collect::<Vec<_>>()
            },
            |attempts| {
                rt.block_on(async {
                    for (i, attempt) in attempts.into_iter().enumerate() {
                        let actor_name = format!("batch_actor_{}", i % 10);
                        supervision.track_restart_attempt(&actor_name, attempt).await.unwrap();
                    }
                })
            },
            BatchSize::SmallInput
        )
    });
    
    group.finish();
}

/// Benchmark blockchain alignment operations
fn bench_blockchain_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("blockchain_alignment");
    
    let config = ActorSystemConfig::development();
    let supervision = EnhancedSupervision::new(config);
    
    let test_delays = vec![
        Duration::from_millis(500),
        Duration::from_millis(1500),
        Duration::from_millis(3500),
        Duration::from_millis(7200),
        Duration::from_secs(15),
    ];
    
    group.bench_function("block_boundary_alignment", |b| {
        b.iter(|| {
            for delay in &test_delays {
                black_box(supervision.align_delay_to_block_boundary(*delay));
            }
        })
    });
    
    // Benchmark consensus timing adjustments
    let rt = tokio::runtime::Runtime::new().unwrap();
    group.bench_function("consensus_timing_adjustment", |b| {
        b.to_async(&rt).iter(|| async {
            for delay in &test_delays {
                black_box(
                    supervision.adjust_delay_for_consensus_timing(*delay, "benchmark_actor").await
                );
            }
        })
    });
    
    group.finish();
}

/// Benchmark failure pattern detection
fn bench_failure_pattern_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("failure_pattern_detection");
    group.throughput(Throughput::Elements(1));
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    // Create test failures for pattern detection
    let create_failure = |i: usize| ActorFailureInfo {
        timestamp: SystemTime::now(),
        failure_type: match i % 4 {
            0 => ActorFailureType::Panic { backtrace: None },
            1 => ActorFailureType::NetworkFailure { 
                peer_id: Some(format!("peer_{}", i % 5)),
                error: "Connection timeout".to_string(),
            },
            2 => ActorFailureType::ConsensusFailure { 
                error_code: format!("ERROR_{}", i % 3),
            },
            _ => ActorFailureType::ResourceExhaustion {
                resource_type: "memory".to_string(),
                usage: 80.0 + (i % 20) as f64,
            },
        },
        message: format!("Pattern test failure #{}", i),
        context: HashMap::new(),
        escalate: i % 3 == 0,
    };
    
    group.bench_function("single_failure_recording", |b| {
        b.iter_batched(
            || {
                let mut detector = FailurePatternDetector::default();
                let failure = create_failure(rand::random::<usize>() % 100);
                (detector, failure)
            },
            |(mut detector, failure)| {
                rt.block_on(async {
                    black_box(detector.record_failure(failure).await)
                })
            },
            BatchSize::SmallInput
        )
    });
    
    group.bench_function("batch_failure_recording", |b| {
        b.iter_batched(
            || {
                let mut detector = FailurePatternDetector::default();
                let failures: Vec<_> = (0..50).map(create_failure).collect();
                (detector, failures)
            },
            |(mut detector, failures)| {
                rt.block_on(async {
                    for failure in failures {
                        detector.record_failure(failure).await;
                    }
                })
            },
            BatchSize::SmallInput
        )
    });
    
    group.finish();
}

/// Benchmark supervision system under load
fn bench_supervision_load_testing(c: &mut Criterion) {
    let mut group = c.benchmark_group("supervision_load_testing");
    group.sample_size(10); // Fewer samples for load tests
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = ActorSystemConfig::production(); // Use production config for load testing
    
    // Test concurrent failure handling
    group.bench_function("concurrent_failure_handling", |b| {
        b.to_async(&rt).iter(|| async {
            let supervision = EnhancedSupervision::new(config.clone());
            
            // Simulate 100 concurrent failures
            let tasks: Vec<_> = (0..100).map(|i| {
                let supervision = &supervision;
                tokio::spawn(async move {
                    let failure_info = ActorFailureInfo {
                        timestamp: SystemTime::now(),
                        failure_type: ActorFailureType::Panic { backtrace: None },
                        message: format!("Load test failure #{}", i),
                        context: HashMap::new(),
                        escalate: false,
                    };
                    
                    let actor_name = format!("load_test_actor_{}", i % 10);
                    supervision.handle_actor_failure(&actor_name, failure_info).await.unwrap();
                })
            }).collect();
            
            futures::future::join_all(tasks).await;
        })
    });
    
    // Test high-frequency restart calculations
    group.bench_function("high_frequency_restart_calculations", |b| {
        b.to_async(&rt).iter(|| async {
            let supervision = EnhancedSupervision::new(config.clone());
            
            let backoff_config = ExponentialBackoffConfig {
                initial_delay: Duration::from_millis(50),
                max_delay: Duration::from_secs(30),
                multiplier: 1.8,
                max_attempts: Some(15),
                jitter: 0.05,
                align_to_block_boundary: false,
                respect_consensus_timing: false,
            };
            
            // Calculate delays for 1000 restart attempts across 100 actors
            for i in 0..1000 {
                let actor_name = format!("freq_test_actor_{}", i % 100);
                let attempt = (i % 10) + 1;
                
                supervision.calculate_exponential_backoff_delay(
                    &actor_name, attempt, &backoff_config
                ).await.unwrap();
            }
        })
    });
    
    group.finish();
}

/// Benchmark memory usage and allocation patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    group.bench_function("supervision_memory_footprint", |b| {
        b.iter(|| {
            let config = ActorSystemConfig::development();
            let supervision = EnhancedSupervision::new(config);
            
            // Simulate memory usage by creating supervision contexts
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // This would normally track actual memory usage
                // For benchmarking, we measure the allocation time
                for i in 0..100 {
                    let actor_name = format!("memory_test_actor_{}", i);
                    
                    let attempt_info = RestartAttemptInfo {
                        attempt_id: Uuid::new_v4(),
                        attempt_number: 1,
                        timestamp: SystemTime::now(),
                        reason: RestartReason::ActorPanic,
                        delay: Duration::from_millis(100),
                        strategy: RestartStrategy::default(),
                        success: Some(true),
                        duration: Some(Duration::from_millis(50)),
                        failure_info: None,
                        context: HashMap::new(),
                    };
                    
                    supervision.track_restart_attempt(&actor_name, attempt_info).await.unwrap();
                }
            });
            
            black_box(supervision)
        })
    });
    
    group.finish();
}

// Benchmark group definitions
criterion_group!(
    supervision_benches,
    bench_supervision_initialization,
    bench_exponential_backoff_calculations,
    bench_fixed_delay_calculations,
    bench_actor_failure_handling,
    bench_restart_attempt_tracking,
    bench_blockchain_alignment,
    bench_failure_pattern_detection,
    bench_supervision_load_testing,
    bench_memory_usage
);

criterion_main!(supervision_benches);