//! Performance Benchmarks for Phase 3: Actor Registry & Discovery
//! 
//! Comprehensive performance benchmarking using Criterion.rs for actor registry
//! operations, discovery methods, lifecycle management, and concurrent access
//! patterns optimized for the Alys sidechain architecture.

use app::actors::foundation::{
    ActorRegistry, ActorRegistryConfig, ActorLifecycleState, ActorPriority,
    ActorQuery, HealthState, HealthStatus, RegistrationContext,
    ThreadSafeActorRegistry, constants::registry
};
use actix::{Actor, Addr, Context};
use criterion::{
    criterion_group, criterion_main, Criterion, BenchmarkId, Throughput,
    black_box, BatchSize, PlotConfiguration, AxisScale
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Benchmark test actor
#[derive(Debug)]
struct BenchmarkActor {
    id: u32,
    data: Vec<u8>,
}

impl BenchmarkActor {
    fn new(id: u32) -> Self {
        Self {
            id,
            data: vec![0u8; 1024], // 1KB of data
        }
    }
}

impl Actor for BenchmarkActor {
    type Context = Context<Self>;
}

/// Create default registration context for benchmarks
fn benchmark_registration_context() -> RegistrationContext {
    RegistrationContext {
        source: "benchmark".to_string(),
        supervisor: Some("benchmark_supervisor".to_string()),
        config: HashMap::new(),
        feature_flags: HashSet::new(),
    }
}

/// Create test tags for benchmarks
fn benchmark_tags(tags: &[&str]) -> HashSet<String> {
    tags.iter().map(|&s| s.to_string()).collect()
}

/// Benchmark registry creation and initialization
fn bench_registry_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry_creation");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    
    group.bench_function("new_development", |b| {
        b.iter(|| {
            black_box(ActorRegistry::development())
        })
    });
    
    group.bench_function("new_production", |b| {
        b.iter(|| {
            black_box(ActorRegistry::production())
        })
    });
    
    group.bench_function("new_custom_config", |b| {
        b.iter(|| {
            let config = ActorRegistryConfig {
                max_actors: 1000,
                enable_type_index: true,
                enable_lifecycle_tracking: true,
                health_check_interval: Duration::from_secs(30),
                enable_metrics: true,
                cleanup_interval: Duration::from_secs(300),
                max_inactive_duration: Duration::from_secs(3600),
                enable_orphan_cleanup: true,
            };
            black_box(ActorRegistry::new(config))
        })
    });
    
    group.bench_function("thread_safe_development", |b| {
        b.iter(|| {
            black_box(ThreadSafeActorRegistry::development())
        })
    });
    
    group.finish();
}

/// Benchmark actor registration operations
fn bench_actor_registration(c: &mut Criterion) {
    let mut group = c.benchmark_group("actor_registration");
    group.throughput(Throughput::Elements(1));
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    // Single registration benchmark
    group.bench_function("single_registration", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                let actor = BenchmarkActor::new(1);
                let addr = actor.start();
                (registry, addr)
            },
            |(mut registry, addr)| {
                black_box(
                    registry.register_actor(
                        "benchmark_actor".to_string(),
                        addr,
                        ActorPriority::Normal,
                        HashSet::new(),
                        benchmark_registration_context(),
                    ).unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    // Batch registration benchmark
    let batch_sizes = [10, 50, 100, 500, 1000];
    for &batch_size in &batch_sizes {
        group.bench_with_input(
            BenchmarkId::new("batch_registration", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter_batched(
                    || {
                        let mut registry = ActorRegistry::development();
                        let actors: Vec<_> = (0..batch_size)
                            .map(|i| {
                                let actor = BenchmarkActor::new(i as u32);
                                (format!("actor_{}", i), actor.start())
                            })
                            .collect();
                        (registry, actors)
                    },
                    |(mut registry, actors)| {
                        for (i, (name, addr)) in actors.into_iter().enumerate() {
                            let priority = match i % 4 {
                                0 => ActorPriority::Critical,
                                1 => ActorPriority::High,
                                2 => ActorPriority::Normal,
                                _ => ActorPriority::Low,
                            };
                            
                            let tags = if i % 3 == 0 {
                                benchmark_tags(&["consensus", "critical"])
                            } else if i % 3 == 1 {
                                benchmark_tags(&["network", "p2p"])
                            } else {
                                benchmark_tags(&["storage", "background"])
                            };
                            
                            registry.register_actor(
                                name,
                                addr,
                                priority,
                                tags,
                                benchmark_registration_context(),
                            ).unwrap();
                        }
                    },
                    BatchSize::SmallInput
                )
            }
        );
    }
    
    // Thread-safe registration benchmark
    group.bench_function("thread_safe_registration", |b| {
        b.iter_batched(
            || {
                let registry = ThreadSafeActorRegistry::development();
                let actor = BenchmarkActor::new(1);
                let addr = actor.start();
                (registry, addr)
            },
            |(registry, addr)| {
                rt.block_on(async {
                    black_box(
                        registry.register_actor(
                            "benchmark_actor".to_string(),
                            addr,
                            ActorPriority::Normal,
                            HashSet::new(),
                            benchmark_registration_context(),
                        ).await.unwrap()
                    )
                })
            },
            BatchSize::SmallInput
        )
    });
    
    group.finish();
}

/// Benchmark actor lookup operations
fn bench_actor_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("actor_lookup");
    group.throughput(Throughput::Elements(1));
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    // Prepare registry with various numbers of actors
    let actor_counts = [10, 100, 1000, 5000];
    
    for &count in &actor_counts {
        group.bench_with_input(
            BenchmarkId::new("get_actor_by_name", count),
            &count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let mut registry = ActorRegistry::development();
                        
                        // Register actors
                        for i in 0..count {
                            let actor = BenchmarkActor::new(i as u32);
                            let addr = actor.start();
                            registry.register_actor(
                                format!("actor_{}", i),
                                addr,
                                ActorPriority::Normal,
                                HashSet::new(),
                                benchmark_registration_context(),
                            ).unwrap();
                        }
                        
                        registry
                    },
                    |registry| {
                        // Lookup random actor
                        let lookup_id = (count / 2).max(1) - 1;
                        black_box(registry.get_actor::<BenchmarkActor>(&format!("actor_{}", lookup_id)))
                    },
                    BatchSize::SmallInput
                )
            }
        );
        
        group.bench_with_input(
            BenchmarkId::new("get_actors_by_type", count),
            &count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let mut registry = ActorRegistry::development();
                        
                        // Register actors
                        for i in 0..count {
                            let actor = BenchmarkActor::new(i as u32);
                            let addr = actor.start();
                            registry.register_actor(
                                format!("actor_{}", i),
                                addr,
                                ActorPriority::Normal,
                                HashSet::new(),
                                benchmark_registration_context(),
                            ).unwrap();
                        }
                        
                        registry
                    },
                    |registry| {
                        black_box(registry.get_actors_by_type::<BenchmarkActor>())
                    },
                    BatchSize::SmallInput
                )
            }
        );
    }
    
    // Benchmark different lookup methods
    let lookup_registry = {
        let mut registry = ActorRegistry::development();
        for i in 0..1000 {
            let actor = BenchmarkActor::new(i);
            let addr = actor.start();
            
            let priority = match i % 4 {
                0 => ActorPriority::Critical,
                1 => ActorPriority::High,
                2 => ActorPriority::Normal,
                _ => ActorPriority::Low,
            };
            
            let tags = match i % 3 {
                0 => benchmark_tags(&["consensus", "critical"]),
                1 => benchmark_tags(&["network", "p2p"]),
                _ => benchmark_tags(&["storage", "background"]),
            };
            
            registry.register_actor(
                format!("actor_{}", i),
                addr,
                priority,
                tags,
                benchmark_registration_context(),
            ).unwrap();
            
            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
        }
        registry
    };
    
    group.bench_function("get_actors_by_priority", |b| {
        b.iter(|| {
            black_box(lookup_registry.get_actors_by_priority(ActorPriority::Normal))
        })
    });
    
    group.bench_function("get_actors_by_tag", |b| {
        b.iter(|| {
            black_box(lookup_registry.get_actors_by_tag("consensus"))
        })
    });
    
    group.bench_function("get_actors_by_state", |b| {
        b.iter(|| {
            black_box(lookup_registry.get_actors_by_state(ActorLifecycleState::Active))
        })
    });
    
    group.bench_function("get_healthy_actors", |b| {
        b.iter(|| {
            black_box(lookup_registry.get_healthy_actors::<BenchmarkActor>())
        })
    });
    
    group.finish();
}

/// Benchmark advanced discovery operations
fn bench_discovery_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("discovery_operations");
    group.throughput(Throughput::Elements(1));
    
    // Prepare registry with test data
    let discovery_registry = {
        let mut registry = ActorRegistry::development();
        for i in 0..1000 {
            let actor = BenchmarkActor::new(i);
            let addr = actor.start();
            
            let priority = match i % 4 {
                0 => ActorPriority::Critical,
                1 => ActorPriority::High,
                2 => ActorPriority::Normal,
                _ => ActorPriority::Low,
            };
            
            let tags = match i % 5 {
                0 => benchmark_tags(&["consensus", "critical", "blockchain"]),
                1 => benchmark_tags(&["network", "p2p", "communication"]),
                2 => benchmark_tags(&["storage", "database", "persistence"]),
                3 => benchmark_tags(&["governance", "voting", "critical"]),
                _ => benchmark_tags(&["background", "maintenance"]),
            };
            
            registry.register_actor(
                format!("actor_{:04}", i), // Zero-padded for pattern matching
                addr,
                priority,
                tags,
                benchmark_registration_context(),
            ).unwrap();
            
            registry.update_actor_state(&format!("actor_{:04}", i), ActorLifecycleState::Active).unwrap();
        }
        registry
    };
    
    group.bench_function("batch_get_actors", |b| {
        let names = (0..50).map(|i| format!("actor_{:04}", i * 20)).collect::<Vec<_>>();
        b.iter(|| {
            black_box(discovery_registry.batch_get_actors::<BenchmarkActor>(&names))
        })
    });
    
    group.bench_function("find_actors_by_pattern", |b| {
        b.iter(|| {
            black_box(discovery_registry.find_actors_by_pattern::<BenchmarkActor>("actor_0*"))
        })
    });
    
    group.bench_function("get_actors_by_tags_intersection", |b| {
        b.iter(|| {
            black_box(discovery_registry.get_actors_by_tags_intersection(&[
                "consensus".to_string(),
                "critical".to_string()
            ]))
        })
    });
    
    group.bench_function("get_actors_by_tags_union", |b| {
        b.iter(|| {
            black_box(discovery_registry.get_actors_by_tags_union(&[
                "consensus".to_string(),
                "network".to_string(),
                "storage".to_string()
            ]))
        })
    });
    
    // Complex query benchmarks
    group.bench_function("simple_query", |b| {
        let query = ActorQuery::new()
            .with_priority(ActorPriority::Critical);
        b.iter(|| {
            black_box(discovery_registry.query_actors(query.clone()))
        })
    });
    
    group.bench_function("complex_query", |b| {
        let query = ActorQuery::new()
            .with_name_pattern("actor_0[0-4][0-9][0-9]".to_string())
            .with_priority(ActorPriority::Critical)
            .with_any_tags(vec!["consensus".to_string(), "governance".to_string()])
            .with_state(ActorLifecycleState::Active);
        b.iter(|| {
            black_box(discovery_registry.query_actors(query.clone()))
        })
    });
    
    group.bench_function("get_actor_type_statistics", |b| {
        b.iter(|| {
            black_box(discovery_registry.get_actor_type_statistics::<BenchmarkActor>())
        })
    });
    
    group.finish();
}

/// Benchmark lifecycle operations
fn bench_lifecycle_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("lifecycle_operations");
    group.throughput(Throughput::Elements(1));
    
    group.bench_function("update_actor_state", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                let actor = BenchmarkActor::new(1);
                let addr = actor.start();
                registry.register_actor(
                    "test_actor".to_string(),
                    addr,
                    ActorPriority::Normal,
                    HashSet::new(),
                    benchmark_registration_context(),
                ).unwrap();
                registry
            },
            |mut registry| {
                black_box(
                    registry.update_actor_state("test_actor", ActorLifecycleState::Active).unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    group.bench_function("update_actor_metadata", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                let actor = BenchmarkActor::new(1);
                let addr = actor.start();
                registry.register_actor(
                    "test_actor".to_string(),
                    addr,
                    ActorPriority::Normal,
                    HashSet::new(),
                    benchmark_registration_context(),
                ).unwrap();
                
                let mut metadata = HashMap::new();
                metadata.insert("version".to_string(), "1.0.0".to_string());
                metadata.insert("component".to_string(), "benchmark".to_string());
                
                (registry, metadata)
            },
            |(mut registry, metadata)| {
                black_box(
                    registry.update_actor_metadata("test_actor", metadata).unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    group.bench_function("update_actor_health", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                let actor = BenchmarkActor::new(1);
                let addr = actor.start();
                registry.register_actor(
                    "test_actor".to_string(),
                    addr,
                    ActorPriority::Normal,
                    HashSet::new(),
                    benchmark_registration_context(),
                ).unwrap();
                
                let health_status = HealthStatus {
                    status: HealthState::Healthy,
                    last_check: Some(SystemTime::now()),
                    error_count: 0,
                    success_rate: 1.0,
                    issues: vec![],
                };
                
                (registry, health_status)
            },
            |(mut registry, health_status)| {
                black_box(
                    registry.update_actor_health("test_actor", health_status).unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    group.bench_function("add_actor_tags", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                let actor = BenchmarkActor::new(1);
                let addr = actor.start();
                registry.register_actor(
                    "test_actor".to_string(),
                    addr,
                    ActorPriority::Normal,
                    HashSet::new(),
                    benchmark_registration_context(),
                ).unwrap();
                
                let tags = benchmark_tags(&["new_tag", "additional", "metadata"]);
                (registry, tags)
            },
            |(mut registry, tags)| {
                black_box(
                    registry.add_actor_tags("test_actor", tags).unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    group.finish();
}

/// Benchmark cleanup and maintenance operations
fn bench_cleanup_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("cleanup_operations");
    group.throughput(Throughput::Elements(1));
    
    let cleanup_counts = [10, 50, 100, 500];
    
    for &count in &cleanup_counts {
        group.bench_with_input(
            BenchmarkId::new("unregister_actor", count),
            &count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let mut registry = ActorRegistry::development();
                        
                        // Register actors
                        for i in 0..count {
                            let actor = BenchmarkActor::new(i as u32);
                            let addr = actor.start();
                            registry.register_actor(
                                format!("actor_{}", i),
                                addr,
                                ActorPriority::Normal,
                                HashSet::new(),
                                benchmark_registration_context(),
                            ).unwrap();
                        }
                        
                        registry
                    },
                    |mut registry| {
                        // Unregister half of the actors
                        for i in 0..(count / 2) {
                            registry.unregister_actor(&format!("actor_{}", i)).unwrap();
                        }
                    },
                    BatchSize::SmallInput
                )
            }
        );
        
        group.bench_with_input(
            BenchmarkId::new("batch_unregister", count),
            &count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let mut registry = ActorRegistry::development();
                        
                        // Register actors
                        for i in 0..count {
                            let actor = BenchmarkActor::new(i as u32);
                            let addr = actor.start();
                            registry.register_actor(
                                format!("actor_{}", i),
                                addr,
                                ActorPriority::Normal,
                                HashSet::new(),
                                benchmark_registration_context(),
                            ).unwrap();
                        }
                        
                        let names_to_remove: Vec<_> = (0..(count / 2))
                            .map(|i| format!("actor_{}", i))
                            .collect();
                        
                        (registry, names_to_remove)
                    },
                    |(mut registry, names)| {
                        black_box(
                            registry.batch_unregister_actors(names, false)
                        )
                    },
                    BatchSize::SmallInput
                )
            }
        );
    }
    
    group.bench_function("cleanup_terminated_actors", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                
                // Register actors and mark half as terminated
                for i in 0..100 {
                    let actor = BenchmarkActor::new(i);
                    let addr = actor.start();
                    registry.register_actor(
                        format!("actor_{}", i),
                        addr,
                        ActorPriority::Normal,
                        HashSet::new(),
                        benchmark_registration_context(),
                    ).unwrap();
                    
                    if i % 2 == 0 {
                        registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
                        registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::ShuttingDown).unwrap();
                        registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Terminated).unwrap();
                    }
                }
                
                registry
            },
            |mut registry| {
                black_box(
                    registry.cleanup_terminated_actors().unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    group.bench_function("perform_maintenance", |b| {
        b.iter_batched(
            || {
                let mut registry = ActorRegistry::development();
                
                // Register actors with various states
                for i in 0..200 {
                    let actor = BenchmarkActor::new(i);
                    let addr = actor.start();
                    registry.register_actor(
                        format!("actor_{}", i),
                        addr,
                        ActorPriority::Normal,
                        HashSet::new(),
                        benchmark_registration_context(),
                    ).unwrap();
                    
                    match i % 3 {
                        0 => {
                            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
                            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::ShuttingDown).unwrap();
                            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Terminated).unwrap();
                        }
                        1 => {
                            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Active).unwrap();
                        }
                        _ => {
                            registry.update_actor_state(&format!("actor_{}", i), ActorLifecycleState::Suspended).unwrap();
                        }
                    }
                }
                
                registry
            },
            |mut registry| {
                black_box(
                    registry.perform_maintenance().unwrap()
                )
            },
            BatchSize::SmallInput
        )
    });
    
    group.finish();
}

/// Benchmark concurrent access patterns
fn bench_concurrent_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_access");
    group.throughput(Throughput::Elements(1));
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    group.bench_function("thread_safe_concurrent_register", |b| {
        b.iter(|| {
            rt.block_on(async {
                let registry = Arc::new(ThreadSafeActorRegistry::development());
                let mut handles = Vec::new();
                
                for i in 0..50 {
                    let registry_clone = Arc::clone(&registry);
                    let handle = tokio::spawn(async move {
                        let actor = BenchmarkActor::new(i);
                        let addr = actor.start();
                        
                        registry_clone.register_actor(
                            format!("actor_{}", i),
                            addr,
                            ActorPriority::Normal,
                            HashSet::new(),
                            benchmark_registration_context(),
                        ).await.unwrap();
                    });
                    handles.push(handle);
                }
                
                futures::future::join_all(handles).await;
                
                black_box(registry.len().await)
            })
        })
    });
    
    group.bench_function("thread_safe_concurrent_lookup", |b| {
        b.iter_batched(
            || {
                rt.block_on(async {
                    let registry = Arc::new(ThreadSafeActorRegistry::development());
                    
                    // Pre-register actors
                    for i in 0..100 {
                        let actor = BenchmarkActor::new(i);
                        let addr = actor.start();
                        registry.register_actor(
                            format!("actor_{}", i),
                            addr,
                            ActorPriority::Normal,
                            HashSet::new(),
                            benchmark_registration_context(),
                        ).await.unwrap();
                    }
                    
                    registry
                })
            },
            |registry| {
                rt.block_on(async {
                    let mut handles = Vec::new();
                    
                    for i in 0..50 {
                        let registry_clone = Arc::clone(&registry);
                        let handle = tokio::spawn(async move {
                            registry_clone.get_actor::<BenchmarkActor>(&format!("actor_{}", i)).await
                        });
                        handles.push(handle);
                    }
                    
                    let results = futures::future::join_all(handles).await;
                    black_box(results.len())
                })
            },
            BatchSize::SmallInput
        )
    });
    
    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    group.bench_function("registry_memory_footprint", |b| {
        b.iter(|| {
            let mut registry = ActorRegistry::development();
            
            // Measure memory usage by registering many actors
            for i in 0..1000 {
                let actor = BenchmarkActor::new(i);
                let addr = actor.start();
                
                let tags = match i % 4 {
                    0 => benchmark_tags(&["consensus", "critical"]),
                    1 => benchmark_tags(&["network", "p2p"]),
                    2 => benchmark_tags(&["storage", "database"]),
                    _ => benchmark_tags(&["background"]),
                };
                
                registry.register_actor(
                    format!("actor_{:04}", i),
                    addr,
                    ActorPriority::Normal,
                    tags,
                    benchmark_registration_context(),
                ).unwrap();
                
                // Add metadata
                let mut metadata = HashMap::new();
                metadata.insert("id".to_string(), i.to_string());
                metadata.insert("type".to_string(), "benchmark".to_string());
                registry.update_actor_metadata(&format!("actor_{:04}", i), metadata).unwrap();
            }
            
            black_box(registry.len())
        })
    });
    
    group.finish();
}

// Benchmark group definitions
criterion_group!(
    registry_benches,
    bench_registry_creation,
    bench_actor_registration,
    bench_actor_lookup,
    bench_discovery_operations,
    bench_lifecycle_operations,
    bench_cleanup_operations,
    bench_concurrent_access,
    bench_memory_usage
);

criterion_main!(registry_benches);