//! Performance Tests for EngineActor
//!
//! Comprehensive performance testing including throughput, latency, memory usage,
//! and stress testing under various conditions.

use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use actix::prelude::*;
use tracing_test::traced_test;

use lighthouse_wrapper::types::{Hash256, Address};

use crate::types::*;
use super::super::{
    messages::*,
    state::ExecutionState,
    EngineResult,
};
use super::{
    helpers::*,
    mocks::{MockExecutionClient, MockClientConfig},
    TestConfig,
};

/// Performance test configuration
#[derive(Debug, Clone)]
pub struct PerformanceTestConfig {
    /// Duration of sustained load tests
    pub load_test_duration: Duration,
    
    /// Number of concurrent operations for concurrency tests
    pub concurrency_level: u32,
    
    /// Number of operations for throughput tests
    pub throughput_operations: u32,
    
    /// Maximum acceptable latency for operations
    pub max_latency: Duration,
    
    /// Minimum acceptable throughput (ops/sec)
    pub min_throughput: f64,
    
    /// Memory growth threshold (bytes)
    pub max_memory_growth: u64,
}

impl Default for PerformanceTestConfig {
    fn default() -> Self {
        Self {
            load_test_duration: Duration::from_secs(30),
            concurrency_level: 20,
            throughput_operations: 1000,
            max_latency: Duration::from_millis(100),
            min_throughput: 50.0, // 50 ops/sec minimum
            max_memory_growth: 50 * 1024 * 1024, // 50MB max growth
        }
    }
}

/// Performance test results
#[derive(Debug)]
pub struct PerformanceResults {
    /// Total operations performed
    pub total_operations: u64,
    
    /// Total test duration
    pub total_duration: Duration,
    
    /// Operations per second
    pub throughput: f64,
    
    /// Latency statistics
    pub latency_stats: LatencyStats,
    
    /// Memory usage statistics
    pub memory_stats: Option<MemoryStats>,
    
    /// Error count
    pub errors: u64,
    
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
}

/// Latency statistics
#[derive(Debug)]
pub struct LatencyStats {
    /// Minimum latency observed
    pub min: Duration,
    
    /// Maximum latency observed
    pub max: Duration,
    
    /// Average latency
    pub mean: Duration,
    
    /// 50th percentile
    pub p50: Duration,
    
    /// 95th percentile
    pub p95: Duration,
    
    /// 99th percentile
    pub p99: Duration,
}

/// Memory usage statistics
#[derive(Debug)]
pub struct MemoryStats {
    /// Initial memory usage
    pub initial: u64,
    
    /// Peak memory usage
    pub peak: u64,
    
    /// Final memory usage
    pub final_usage: u64,
    
    /// Memory growth
    pub growth: u64,
}

/// Performance test suite
pub struct PerformanceTester {
    config: PerformanceTestConfig,
    helper: EngineActorTestHelper,
    memory_tracker: MemoryTracker,
}

impl PerformanceTester {
    pub fn new() -> Self {
        Self::with_config(PerformanceTestConfig::default())
    }
    
    pub fn with_config(config: PerformanceTestConfig) -> Self {
        let test_config = TestConfig::performance();
        
        Self {
            config,
            helper: EngineActorTestHelper::with_config(test_config),
            memory_tracker: MemoryTracker::new(),
        }
    }
    
    /// Run complete performance test suite
    pub async fn run_full_suite(&mut self) -> EngineResult<HashMap<String, PerformanceResults>> {
        let mut results = HashMap::new();
        
        println!("🚀 Starting EngineActor Performance Test Suite");
        println!("Configuration: {:?}", self.config);
        
        // Initialize actor
        self.helper.start_with_mock().await?;
        self.helper.wait_for_ready(Duration::from_secs(10)).await;
        
        // Run individual performance tests
        results.insert("latency".to_string(), self.test_latency().await?);
        results.insert("throughput".to_string(), self.test_throughput().await?);
        results.insert("concurrency".to_string(), self.test_concurrency().await?);
        results.insert("sustained_load".to_string(), self.test_sustained_load().await?);
        results.insert("memory_usage".to_string(), self.test_memory_usage().await?);
        
        // Cleanup
        self.helper.shutdown(Duration::from_secs(5)).await?;
        
        println!("✅ Performance Test Suite Completed");
        self.print_summary(&results);
        
        Ok(results)
    }
    
    /// Test latency characteristics
    async fn test_latency(&mut self) -> EngineResult<PerformanceResults> {
        println!("📊 Testing Latency Characteristics");
        
        let mut latencies = Vec::new();
        let operations = 100;
        let start_time = Instant::now();
        let mut errors = 0;
        
        for i in 0..operations {
            let parent_hash = Hash256::random();
            let operation_start = Instant::now();
            
            match self.helper.build_payload(parent_hash).await {
                Ok(_) => {
                    let latency = operation_start.elapsed();
                    latencies.push(latency);
                    
                    if i % 20 == 0 {
                        print!(".");
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                },
                Err(_) => {
                    errors += 1;
                }
            }
            
            self.memory_tracker.update_peak();
        }
        
        println!(" Done!");
        
        let total_duration = start_time.elapsed();
        let latency_stats = self.calculate_latency_stats(&latencies);
        let success_rate = (operations - errors) as f64 / operations as f64;
        
        println!("Latency Results:");
        println!("  Mean: {:?}", latency_stats.mean);
        println!("  P95: {:?}", latency_stats.p95);
        println!("  P99: {:?}", latency_stats.p99);
        println!("  Max: {:?}", latency_stats.max);
        
        Ok(PerformanceResults {
            total_operations: operations,
            total_duration,
            throughput: operations as f64 / total_duration.as_secs_f64(),
            latency_stats,
            memory_stats: self.get_memory_stats(),
            errors,
            success_rate,
        })
    }
    
    /// Test throughput characteristics
    async fn test_throughput(&mut self) -> EngineResult<PerformanceResults> {
        println!("🔥 Testing Throughput Performance");
        
        let operations = self.config.throughput_operations as u64;
        let start_time = Instant::now();
        let mut latencies = Vec::new();
        let mut errors = 0;
        
        for i in 0..operations {
            let parent_hash = Hash256::random();
            let operation_start = Instant::now();
            
            match self.helper.build_payload(parent_hash).await {
                Ok(_) => {
                    latencies.push(operation_start.elapsed());
                    
                    if i % (operations / 10) == 0 {
                        let progress = (i * 100) / operations;
                        print!("\rProgress: {}%", progress);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                },
                Err(_) => {
                    errors += 1;
                }
            }
            
            self.memory_tracker.update_peak();
        }
        
        let total_duration = start_time.elapsed();
        let throughput = operations as f64 / total_duration.as_secs_f64();
        let success_rate = (operations - errors) as f64 / operations as f64;
        
        println!("\nThroughput Results:");
        println!("  Operations: {}", operations);
        println!("  Duration: {:?}", total_duration);
        println!("  Throughput: {:.2} ops/sec", throughput);
        println!("  Success Rate: {:.2}%", success_rate * 100.0);
        
        // Verify throughput meets requirements
        if throughput < self.config.min_throughput {
            println!("⚠️  Throughput {} below minimum {}", throughput, self.config.min_throughput);
        }
        
        Ok(PerformanceResults {
            total_operations: operations,
            total_duration,
            throughput,
            latency_stats: self.calculate_latency_stats(&latencies),
            memory_stats: self.get_memory_stats(),
            errors,
            success_rate,
        })
    }
    
    /// Test concurrent operation handling
    async fn test_concurrency(&mut self) -> EngineResult<PerformanceResults> {
        println!("⚡ Testing Concurrent Operations");
        
        let concurrency = self.config.concurrency_level;
        let operations_per_task = 50;
        let total_operations = concurrency as u64 * operations_per_task;
        
        let start_time = Instant::now();
        let results = Arc::new(Mutex::new(Vec::new()));
        let error_count = Arc::new(Mutex::new(0u64));
        
        let mut handles = Vec::new();
        
        for task_id in 0..concurrency {
            let actor = self.helper.actor.as_ref().unwrap().clone();
            let results_clone = Arc::clone(&results);
            let error_count_clone = Arc::clone(&error_count);
            
            let handle = tokio::spawn(async move {
                for i in 0..operations_per_task {
                    let parent_hash = Hash256::random();
                    let operation_start = Instant::now();
                    
                    let msg = BuildPayloadMessage {
                        parent_hash,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        fee_recipient: Address::zero(),
                        prev_randao: Hash256::random(),
                        withdrawals: vec![],
                        correlation_id: Some(format!("perf_test_{}_{}", task_id, i)),
                    };
                    
                    match actor.send(msg).await {
                        Ok(Ok(_)) => {
                            let latency = operation_start.elapsed();
                            results_clone.lock().unwrap().push(latency);
                        },
                        _ => {
                            *error_count_clone.lock().unwrap() += 1;
                        }
                    }
                }
            });
            
            handles.push(handle);
        }
        
        // Wait for all concurrent tasks
        for handle in handles {
            handle.await.map_err(|e| super::super::EngineError::ActorError(format!("Task join error: {}", e)))?;
        }
        
        let total_duration = start_time.elapsed();
        let latencies = results.lock().unwrap().clone();
        let errors = *error_count.lock().unwrap();
        let throughput = total_operations as f64 / total_duration.as_secs_f64();
        let success_rate = (total_operations - errors) as f64 / total_operations as f64;
        
        println!("Concurrency Results:");
        println!("  Concurrent Tasks: {}", concurrency);
        println!("  Total Operations: {}", total_operations);
        println!("  Duration: {:?}", total_duration);
        println!("  Throughput: {:.2} ops/sec", throughput);
        println!("  Success Rate: {:.2}%", success_rate * 100.0);
        
        Ok(PerformanceResults {
            total_operations,
            total_duration,
            throughput,
            latency_stats: self.calculate_latency_stats(&latencies),
            memory_stats: self.get_memory_stats(),
            errors,
            success_rate,
        })
    }
    
    /// Test sustained load performance
    async fn test_sustained_load(&mut self) -> EngineResult<PerformanceResults> {
        println!("⏱️  Testing Sustained Load Performance");
        
        let duration = self.config.load_test_duration;
        let start_time = Instant::now();
        let mut operations = 0u64;
        let mut latencies = Vec::new();
        let mut errors = 0u64;
        
        println!("Running for {:?}...", duration);
        
        let mut last_progress = Instant::now();
        
        while start_time.elapsed() < duration {
            let parent_hash = Hash256::random();
            let operation_start = Instant::now();
            
            match self.helper.build_payload(parent_hash).await {
                Ok(_) => {
                    operations += 1;
                    latencies.push(operation_start.elapsed());
                },
                Err(_) => {
                    errors += 1;
                }
            }
            
            self.memory_tracker.update_peak();
            
            // Progress reporting
            if last_progress.elapsed() > Duration::from_secs(5) {
                let elapsed = start_time.elapsed();
                let progress = (elapsed.as_secs_f64() / duration.as_secs_f64() * 100.0) as u32;
                let current_throughput = operations as f64 / elapsed.as_secs_f64();
                println!("Progress: {}% - Current throughput: {:.1} ops/sec", progress, current_throughput);
                last_progress = Instant::now();
            }
            
            // Small delay to prevent overwhelming
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let total_duration = start_time.elapsed();
        let throughput = operations as f64 / total_duration.as_secs_f64();
        let success_rate = operations as f64 / (operations + errors) as f64;
        
        println!("Sustained Load Results:");
        println!("  Duration: {:?}", total_duration);
        println!("  Operations: {}", operations);
        println!("  Throughput: {:.2} ops/sec", throughput);
        println!("  Error Rate: {:.2}%", (1.0 - success_rate) * 100.0);
        
        Ok(PerformanceResults {
            total_operations: operations,
            total_duration,
            throughput,
            latency_stats: self.calculate_latency_stats(&latencies),
            memory_stats: self.get_memory_stats(),
            errors,
            success_rate,
        })
    }
    
    /// Test memory usage characteristics
    async fn test_memory_usage(&mut self) -> EngineResult<PerformanceResults> {
        println!("💾 Testing Memory Usage");
        
        let operations = 500;
        let start_time = Instant::now();
        let mut latencies = Vec::new();
        let mut errors = 0;
        
        // Baseline memory measurement
        self.memory_tracker.update_peak();
        
        for i in 0..operations {
            let parent_hash = Hash256::random();
            let operation_start = Instant::now();
            
            match self.helper.build_payload(parent_hash).await {
                Ok(_) => {
                    latencies.push(operation_start.elapsed());
                },
                Err(_) => {
                    errors += 1;
                }
            }
            
            // Update memory tracking
            self.memory_tracker.update_peak();
            
            if i % 50 == 0 {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                
                // Force garbage collection (if applicable)
                tokio::task::yield_now().await;
            }
        }
        
        println!(" Done!");
        
        let total_duration = start_time.elapsed();
        let throughput = operations as f64 / total_duration.as_secs_f64();
        let success_rate = (operations - errors) as f64 / operations as f64;
        
        if let Some(memory_stats) = self.get_memory_stats() {
            println!("Memory Usage Results:");
            println!("  Initial: {} MB", memory_stats.initial / 1024 / 1024);
            println!("  Peak: {} MB", memory_stats.peak / 1024 / 1024);
            println!("  Growth: {} MB", memory_stats.growth / 1024 / 1024);
            
            // Check memory growth threshold
            if memory_stats.growth > self.config.max_memory_growth {
                println!("⚠️  Memory growth {} exceeds threshold {}", 
                        memory_stats.growth, self.config.max_memory_growth);
            }
        } else {
            println!("Memory tracking not available on this platform");
        }
        
        Ok(PerformanceResults {
            total_operations: operations,
            total_duration,
            throughput,
            latency_stats: self.calculate_latency_stats(&latencies),
            memory_stats: self.get_memory_stats(),
            errors,
            success_rate,
        })
    }
    
    /// Calculate latency statistics from measurements
    fn calculate_latency_stats(&self, latencies: &[Duration]) -> LatencyStats {
        if latencies.is_empty() {
            return LatencyStats {
                min: Duration::ZERO,
                max: Duration::ZERO,
                mean: Duration::ZERO,
                p50: Duration::ZERO,
                p95: Duration::ZERO,
                p99: Duration::ZERO,
            };
        }
        
        let mut sorted = latencies.to_vec();
        sorted.sort();
        
        let len = sorted.len();
        let sum: Duration = sorted.iter().sum();
        
        LatencyStats {
            min: sorted[0],
            max: sorted[len - 1],
            mean: sum / len as u32,
            p50: sorted[len * 50 / 100],
            p95: sorted[len * 95 / 100],
            p99: sorted[len * 99 / 100],
        }
    }
    
    /// Get memory statistics from tracker
    fn get_memory_stats(&self) -> Option<MemoryStats> {
        self.memory_tracker.get_memory_usage().map(|(initial, peak)| {
            MemoryStats {
                initial,
                peak,
                final_usage: peak, // Approximation
                growth: peak.saturating_sub(initial),
            }
        })
    }
    
    /// Print test suite summary
    fn print_summary(&self, results: &HashMap<String, PerformanceResults>) {
        println!("\n📋 Performance Test Summary");
        println!("{:-<60}", "");
        
        for (test_name, result) in results {
            println!("{}:", test_name.to_uppercase());
            println!("  Operations: {}", result.total_operations);
            println!("  Duration: {:?}", result.total_duration);
            println!("  Throughput: {:.2} ops/sec", result.throughput);
            println!("  Success Rate: {:.1}%", result.success_rate * 100.0);
            println!("  Mean Latency: {:?}", result.latency_stats.mean);
            println!("  P95 Latency: {:?}", result.latency_stats.p95);
            
            if let Some(ref memory) = result.memory_stats {
                println!("  Memory Growth: {} MB", memory.growth / 1024 / 1024);
            }
            
            println!();
        }
        
        // Overall assessment
        let overall_success = results.values().all(|r| {
            r.success_rate > 0.95 && // 95% success rate
            r.latency_stats.p95 < self.config.max_latency &&
            r.throughput > self.config.min_throughput * 0.8 // 80% of min throughput
        });
        
        if overall_success {
            println!("✅ Overall Assessment: PASS");
        } else {
            println!("❌ Overall Assessment: NEEDS IMPROVEMENT");
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_basic_latency() {
        let mut tester = PerformanceTester::with_config(PerformanceTestConfig {
            load_test_duration: Duration::from_secs(5),
            throughput_operations: 100,
            concurrency_level: 5,
            ..Default::default()
        });
        
        let result = tester.test_latency().await.expect("Latency test should complete");
        
        assert!(result.success_rate > 0.9, "Should have high success rate");
        assert!(result.latency_stats.mean < Duration::from_millis(50), "Mean latency should be reasonable");
    }
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_throughput_benchmark() {
        let mut tester = PerformanceTester::with_config(PerformanceTestConfig {
            throughput_operations: 200,
            min_throughput: 20.0, // Lower expectation for test environment
            ..Default::default()
        });
        
        let result = tester.test_throughput().await.expect("Throughput test should complete");
        
        assert!(result.total_operations > 0, "Should complete operations");
        assert!(result.throughput > 10.0, "Should achieve minimum throughput");
    }
    
    #[actix_rt::test]
    #[traced_test]
    async fn test_concurrency_handling() {
        let mut tester = PerformanceTester::with_config(PerformanceTestConfig {
            concurrency_level: 10,
            ..Default::default()
        });
        
        let result = tester.test_concurrency().await.expect("Concurrency test should complete");
        
        assert!(result.success_rate > 0.8, "Should handle concurrent operations well");
        assert!(result.total_operations > 0, "Should complete concurrent operations");
    }
}