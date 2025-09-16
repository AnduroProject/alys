//! System Profiling Benchmarks using Criterion.rs
//!
//! Implements ALYS-002-26: Memory and CPU profiling integration with flamegraph generation
//! 
//! This benchmark suite measures:
//! - CPU-intensive operations performance
//! - Memory allocation patterns and efficiency
//! - Combined CPU and memory stress scenarios
//! - System resource utilization under load

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;
use std::collections::HashMap;
use tokio::runtime::Runtime;

/// Benchmark CPU-intensive cryptographic operations
fn bench_cpu_intensive_crypto(c: &mut Criterion) {
    let mut group = c.benchmark_group("cpu_intensive_crypto");
    
    // Test different workload sizes
    for operation_count in [1_000, 10_000, 100_000, 1_000_000].iter() {
        group.throughput(Throughput::Elements(*operation_count as u64));
        
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}operations", operation_count)),
            operation_count,
            |b, operation_count| {
                b.iter(|| {
                    let mut hash_result = 0u64;
                    
                    // Simulate CPU-intensive hashing operations
                    for i in 0..**operation_count {
                        // Simulate SHA256-like operations with multiple rounds
                        let mut data = i as u64;
                        
                        // Multiple rounds of bit operations to simulate hashing
                        for round in 0..64 { // 64 rounds like SHA256
                            data = data.wrapping_mul(1103515245);
                            data = data.wrapping_add(12345);
                            data ^= data >> 16;
                            data = data.wrapping_mul(2654435761);
                            data ^= data >> 13;
                            data = data.wrapping_mul(1697609667);
                            data ^= data >> 16;
                        }
                        
                        hash_result = hash_result.wrapping_add(data);
                    }
                    
                    black_box(hash_result)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory allocation patterns
fn bench_memory_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation_patterns");
    
    // Test different allocation patterns
    for pattern in ["sequential", "scattered", "chunked"].iter() {
        for allocation_size in [1_024, 64_1024, 1_048_576].iter() { // 1KB, 64KB, 1MB
            let allocation_count = 1000;
            group.throughput(Throughput::Bytes((allocation_count * *allocation_size) as u64));
            
            group.bench_with_input(
                BenchmarkId::from_parameter(format!("{}_pattern_{}bytes", pattern, allocation_size)),
                &(pattern, allocation_size),
                |b, &(pattern, allocation_size)| {
                    b.iter(|| {
                        match *pattern {
                            "sequential" => {
                                // Sequential allocation and immediate use
                                let mut allocations = Vec::new();
                                let mut checksum = 0u64;
                                
                                for i in 0..allocation_count {
                                    let mut buffer = vec![0u8; *allocation_size];
                                    
                                    // Write some data to ensure allocation
                                    buffer[0] = (i % 256) as u8;
                                    if buffer.len() > 1 {
                                        buffer[buffer.len() - 1] = ((i + 1) % 256) as u8;
                                    }
                                    
                                    checksum = checksum.wrapping_add(buffer[0] as u64);
                                    allocations.push(buffer);
                                }
                                
                                black_box((allocations.len(), checksum))
                            },
                            "scattered" => {
                                // Scattered allocation with interspersed operations
                                let mut allocations = HashMap::new();
                                let mut operation_result = 0u64;
                                
                                for i in 0..allocation_count {
                                    // Allocate buffer
                                    let mut buffer = vec![0u8; *allocation_size];
                                    buffer[0] = (i % 256) as u8;
                                    
                                    // Intersperse with computations
                                    for j in 0..10 {
                                        operation_result = operation_result.wrapping_add(i as u64 * j);
                                    }
                                    
                                    allocations.insert(i, buffer);
                                    
                                    // Occasionally free some allocations
                                    if i > 100 && i % 50 == 0 {
                                        allocations.remove(&(i - 100));
                                    }
                                }
                                
                                black_box((allocations.len(), operation_result))
                            },
                            "chunked" => {
                                // Chunked allocation in batches
                                let mut chunks = Vec::new();
                                let chunk_size = 100;
                                
                                for chunk_id in 0..(allocation_count / chunk_size) {
                                    let mut chunk = Vec::new();
                                    
                                    // Allocate chunk_size buffers at once
                                    for i in 0..chunk_size {
                                        let mut buffer = vec![0u8; *allocation_size];
                                        buffer[0] = ((chunk_id * chunk_size + i) % 256) as u8;
                                        chunk.push(buffer);
                                    }
                                    
                                    chunks.push(chunk);
                                    
                                    // Process chunk immediately
                                    let mut chunk_checksum = 0u64;
                                    for buffer in &chunks[chunk_id] {
                                        chunk_checksum = chunk_checksum.wrapping_add(buffer[0] as u64);
                                    }
                                }
                                
                                black_box(chunks.len())
                            },
                            _ => unreachable!(),
                        }
                    });
                },
            );
        }
    }
    
    group.finish();
}

/// Benchmark concurrent CPU and memory operations
fn bench_concurrent_cpu_memory_stress(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_cpu_memory_stress");
    
    // Test different concurrency levels
    for worker_count in [1, 2, 4, 8].iter() {
        let operations_per_worker = 10_000;
        group.throughput(Throughput::Elements((*worker_count * operations_per_worker) as u64));
        
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}workers", worker_count)),
            worker_count,
            |b, worker_count| {
                b.to_async(&runtime).iter(|| async {
                    let mut handles = Vec::new();
                    
                    // Spawn concurrent workers
                    for worker_id in 0..**worker_count {
                        let handle = tokio::spawn(async move {
                            let mut worker_result = 0u64;
                            let mut allocations = Vec::new();
                            
                            for i in 0..operations_per_worker {
                                // CPU work: Complex mathematical operations
                                let mut cpu_work = (worker_id * 1000 + i) as u64;
                                for _ in 0..50 { // 50 rounds of computation
                                    cpu_work = cpu_work.wrapping_mul(6364136223846793005);
                                    cpu_work = cpu_work.wrapping_add(1442695040888963407);
                                    cpu_work ^= cpu_work >> 32;
                                }
                                worker_result = worker_result.wrapping_add(cpu_work);
                                
                                // Memory work: Allocations every 10 operations
                                if i % 10 == 0 {
                                    let buffer_size = 4096 + (i % 1000) * 64; // 4KB to 68KB
                                    let mut buffer = vec![0u8; buffer_size];
                                    
                                    // Write pattern to prevent optimization
                                    for j in (0..buffer.len()).step_by(64) {
                                        buffer[j] = ((worker_id + i + j) % 256) as u8;
                                    }
                                    
                                    allocations.push(buffer);
                                    
                                    // Cleanup old allocations to prevent unbounded growth
                                    if allocations.len() > 50 {
                                        allocations.remove(0);
                                    }
                                }
                                
                                // Yield occasionally to allow other tasks to run
                                if i % 100 == 0 {
                                    tokio::task::yield_now().await;
                                }
                            }
                            
                            (worker_id, worker_result, allocations.len())
                        });
                        
                        handles.push(handle);
                    }
                    
                    // Wait for all workers to complete
                    let mut total_result = 0u64;
                    let mut total_allocations = 0usize;
                    
                    for handle in handles {
                        let (worker_id, result, allocation_count) = handle.await.unwrap();
                        total_result = total_result.wrapping_add(result);
                        total_allocations += allocation_count;
                    }
                    
                    black_box((total_result, total_allocations))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark memory fragmentation scenarios
fn bench_memory_fragmentation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_fragmentation");
    
    // Test different fragmentation patterns
    for pattern in ["uniform", "mixed", "alternating"].iter() {
        let allocation_cycles = 1000;
        group.throughput(Throughput::Elements(allocation_cycles as u64));
        
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_fragmentation", pattern)),
            pattern,
            |b, pattern| {
                b.iter(|| {
                    let mut allocations = HashMap::new();
                    let mut allocation_id = 0usize;
                    let mut total_allocated = 0usize;
                    
                    match *pattern {
                        "uniform" => {
                            // Uniform size allocations
                            let size = 4096; // 4KB blocks
                            
                            for cycle in 0..allocation_cycles {
                                // Allocate
                                let buffer = vec![0u8; size];
                                allocations.insert(allocation_id, buffer);
                                total_allocated += size;
                                allocation_id += 1;
                                
                                // Deallocate every few cycles to create fragmentation
                                if cycle > 100 && cycle % 10 == 0 {
                                    let old_id = allocation_id - 50;
                                    if let Some(removed) = allocations.remove(&old_id) {
                                        total_allocated -= removed.len();
                                    }
                                }
                            }
                        },
                        "mixed" => {
                            // Mixed size allocations
                            let sizes = [1024, 2048, 4096, 8192, 16384]; // 1KB to 16KB
                            
                            for cycle in 0..allocation_cycles {
                                let size = sizes[cycle % sizes.len()];
                                
                                // Allocate
                                let buffer = vec![0u8; size];
                                allocations.insert(allocation_id, buffer);
                                total_allocated += size;
                                allocation_id += 1;
                                
                                // Random deallocations to increase fragmentation
                                if cycle > 200 && (cycle * 7) % 13 == 0 {
                                    let old_id = allocation_id.saturating_sub(100 + (cycle % 50));
                                    if let Some(removed) = allocations.remove(&old_id) {
                                        total_allocated -= removed.len();
                                    }
                                }
                            }
                        },
                        "alternating" => {
                            // Alternating small/large allocations
                            let small_size = 512;   // 512 bytes
                            let large_size = 32768; // 32KB
                            
                            for cycle in 0..allocation_cycles {
                                let size = if cycle % 2 == 0 { small_size } else { large_size };
                                
                                // Allocate
                                let buffer = vec![0u8; size];
                                allocations.insert(allocation_id, buffer);
                                total_allocated += size;
                                allocation_id += 1;
                                
                                // Deallocate with alternating pattern
                                if cycle > 50 && cycle % 7 == 0 {
                                    let old_id = allocation_id - 30;
                                    if let Some(removed) = allocations.remove(&old_id) {
                                        total_allocated -= removed.len();
                                    }
                                }
                            }
                        },
                        _ => unreachable!(),
                    }
                    
                    black_box((allocations.len(), total_allocated))
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark stack vs heap performance
fn bench_stack_vs_heap_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("stack_vs_heap_performance");
    
    // Test different data sizes
    for data_size in [64, 512, 4096].iter() { // 64B, 512B, 4KB
        let iterations = 10_000;
        group.throughput(Throughput::Elements(iterations as u64));
        
        // Stack allocation benchmark
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("stack_{}bytes", data_size)),
            data_size,
            |b, data_size| {
                b.iter(|| {
                    let mut checksum = 0u64;
                    
                    for i in 0..iterations {
                        // Use const generic for stack allocation
                        // Note: This is a simplified example; real implementation
                        // would need to handle different sizes appropriately
                        if **data_size <= 64 {
                            let stack_data = [0u8; 64];
                            checksum = checksum.wrapping_add(stack_data[0] as u64 + i as u64);
                        } else if **data_size <= 512 {
                            let stack_data = [0u8; 512];
                            checksum = checksum.wrapping_add(stack_data[0] as u64 + i as u64);
                        } else {
                            let stack_data = [0u8; 4096];
                            checksum = checksum.wrapping_add(stack_data[0] as u64 + i as u64);
                        }
                    }
                    
                    black_box(checksum)
                });
            },
        );
        
        // Heap allocation benchmark
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("heap_{}bytes", data_size)),
            data_size,
            |b, data_size| {
                b.iter(|| {
                    let mut checksum = 0u64;
                    
                    for i in 0..iterations {
                        let heap_data = vec![0u8; **data_size];
                        checksum = checksum.wrapping_add(heap_data[0] as u64 + i as u64);
                    }
                    
                    black_box(checksum)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark cache performance with different access patterns
fn bench_cache_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_performance");
    
    // Test different array sizes and access patterns
    for array_size in [1_024, 64_1024, 1_048_576].iter() { // 1KB, 64KB, 1MB
        let access_count = 100_000;
        group.throughput(Throughput::Elements(access_count as u64));
        
        // Sequential access pattern
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("sequential_{}bytes", array_size)),
            array_size,
            |b, array_size| {
                b.iter(|| {
                    let data = vec![0u64; **array_size / 8]; // u64 elements
                    let mut sum = 0u64;
                    
                    for _ in 0..access_count {
                        for i in 0..data.len() {
                            sum = sum.wrapping_add(data[i]);
                        }
                    }
                    
                    black_box(sum)
                });
            },
        );
        
        // Random access pattern (cache unfriendly)
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("random_{}bytes", array_size)),
            array_size,
            |b, array_size| {
                b.iter(|| {
                    let data = vec![0u64; **array_size / 8]; // u64 elements
                    let mut sum = 0u64;
                    let mut index = 0usize;
                    
                    for i in 0..access_count {
                        // Simple PRNG for random access
                        index = (index.wrapping_mul(1103515245).wrapping_add(12345)) % data.len();
                        sum = sum.wrapping_add(data[index]);
                    }
                    
                    black_box(sum)
                });
            },
        );
        
        // Strided access pattern
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("strided_{}bytes", array_size)),
            array_size,
            |b, array_size| {
                b.iter(|| {
                    let data = vec![0u64; **array_size / 8]; // u64 elements
                    let mut sum = 0u64;
                    let stride = 16; // Access every 16th element
                    
                    for _ in 0..access_count {
                        for i in (0..data.len()).step_by(stride) {
                            sum = sum.wrapping_add(data[i]);
                        }
                    }
                    
                    black_box(sum)
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark async task overhead
fn bench_async_task_overhead(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("async_task_overhead");
    
    // Test different task spawning patterns
    for task_count in [10, 100, 1000, 5000].iter() {
        group.throughput(Throughput::Elements(*task_count as u64));
        
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}tasks", task_count)),
            task_count,
            |b, task_count| {
                b.to_async(&runtime).iter(|| async {
                    let mut handles = Vec::new();
                    
                    // Spawn tasks
                    for task_id in 0..**task_count {
                        let handle = tokio::spawn(async move {
                            // Minimal work per task
                            let mut result = task_id as u64;
                            
                            // Small amount of computation
                            for i in 0..10 {
                                result = result.wrapping_add(i);
                            }
                            
                            // Small async delay
                            tokio::time::sleep(Duration::from_nanos(1)).await;
                            
                            result
                        });
                        
                        handles.push(handle);
                    }
                    
                    // Wait for all tasks
                    let mut total_result = 0u64;
                    for handle in handles {
                        total_result = total_result.wrapping_add(handle.await.unwrap());
                    }
                    
                    black_box(total_result)
                });
            },
        );
    }
    
    group.finish();
}

// Configure Criterion benchmark groups
criterion_group!(
    system_benches,
    bench_cpu_intensive_crypto,
    bench_memory_allocation_patterns,
    bench_concurrent_cpu_memory_stress,
    bench_memory_fragmentation,
    bench_stack_vs_heap_performance,
    bench_cache_performance,
    bench_async_task_overhead
);

criterion_main!(system_benches);