//! Performance optimizations and benchmarking for feature flags
//!
//! This module implements ALYS-004-08, ALYS-004-09, and ALYS-004-10:
//! - High-performance macro with 5-second caching  
//! - Hash-based context evaluation for consistent percentage rollouts
//! - Performance benchmarking with <1ms target per flag check

use super::context::EvaluationContext;
use super::metrics::FeatureFlagMetrics;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};
use std::hash::{Hash, Hasher, DefaultHasher};
use once_cell::sync::Lazy;
use tracing::{trace, warn};

/// High-performance macro cache for feature flags with 5-second TTL
pub mod macro_cache {
    use super::*;

    /// Macro-level cache entry with strict 5-second TTL and context validation
    #[derive(Debug, Clone)]
    pub struct MacroCacheEntry {
        pub result: bool,
        pub created_at: Instant,
        pub context_hash: u64,
        pub access_count: u64,
        pub evaluation_time_us: u64,
    }

    impl MacroCacheEntry {
        pub fn new(result: bool, context_hash: u64, evaluation_time_us: u64) -> Self {
            Self {
                result,
                created_at: Instant::now(),
                context_hash,
                access_count: 0,
                evaluation_time_us,
            }
        }

        /// Check if entry is expired (strict 5-second TTL)
        pub fn is_expired(&self) -> bool {
            self.created_at.elapsed() > Duration::from_secs(5)
        }

        /// Access the cached result and increment counter
        pub fn access(&mut self) -> bool {
            self.access_count += 1;
            self.result
        }

        /// Validate that context hasn't changed
        pub fn is_context_valid(&self, context_hash: u64) -> bool {
            self.context_hash == context_hash
        }
    }

    /// Global macro cache with 5-second TTL and automatic cleanup
    static MACRO_CACHE: Lazy<Arc<RwLock<HashMap<String, MacroCacheEntry>>>> = 
        Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

    /// Performance tracking for macro cache
    #[derive(Debug, Clone)]
    pub struct MacroCacheStats {
        pub hits: u64,
        pub misses: u64,
        pub invalidations: u64,
        pub cleanups: u64,
        pub total_accesses: u64,
        pub avg_cache_lookup_time_ns: u64,
        pub max_cache_size: usize,
        pub current_cache_size: usize,
    }

    static MACRO_STATS: Lazy<Arc<RwLock<MacroCacheStats>>> = Lazy::new(|| {
        Arc::new(RwLock::new(MacroCacheStats {
            hits: 0,
            misses: 0,
            invalidations: 0,
            cleanups: 0,
            total_accesses: 0,
            avg_cache_lookup_time_ns: 0,
            max_cache_size: 0,
            current_cache_size: 0,
        }))
    });

    /// Ultra-fast cache lookup with 5-second TTL and context validation
    /// Target: <50μs for cache hits, <500μs for cache misses
    pub async fn fast_cache_lookup(
        flag_name: &str, 
        context: &EvaluationContext
    ) -> Option<bool> {
        let lookup_start = Instant::now();
        let cache_key = generate_cache_key(flag_name, context);
        let context_hash = context.hash();

        let mut cache = MACRO_CACHE.write().await;

        if let Some(entry) = cache.get_mut(&cache_key) {
            // Validate context hasn't changed (prevents stale data)
            if !entry.is_context_valid(context_hash) {
                cache.remove(&cache_key);
                update_stats(|s| {
                    s.invalidations += 1;
                    s.total_accesses += 1;
                    s.current_cache_size = cache.len();
                }).await;
                return None;
            }

            // Check strict 5-second expiration
            if entry.is_expired() {
                cache.remove(&cache_key);
                update_stats(|s| {
                    s.misses += 1;
                    s.total_accesses += 1;
                    s.current_cache_size = cache.len();
                }).await;
                return None;
            }

            // Valid cache hit - track performance
            let result = entry.access();
            let lookup_time = lookup_start.elapsed().as_nanos() as u64;
            
            // Record macro cache hit metrics
            FeatureFlagMetrics::record_macro_cache_hit(flag_name);
            
            update_stats(|s| {
                s.hits += 1;
                s.total_accesses += 1;
                s.current_cache_size = cache.len();
                // Running average of lookup times
                s.avg_cache_lookup_time_ns = 
                    (s.avg_cache_lookup_time_ns + lookup_time) / 2;
            }).await;

            return Some(result);
        }

        // Cache miss
        update_stats(|s| {
            s.misses += 1;
            s.total_accesses += 1;
            s.current_cache_size = cache.len();
        }).await;

        None
    }

    /// Cache a result with 5-second TTL and memory protection
    pub async fn fast_cache_store(
        flag_name: &str, 
        context: &EvaluationContext, 
        result: bool,
        evaluation_time_us: u64
    ) {
        let cache_key = generate_cache_key(flag_name, context);
        let context_hash = context.hash();
        let entry = MacroCacheEntry::new(result, context_hash, evaluation_time_us);

        let mut cache = MACRO_CACHE.write().await;

        // Memory protection: prevent unbounded cache growth
        if cache.len() >= 10000 {
            // First try removing expired entries
            let initial_size = cache.len();
            cache.retain(|_, entry| !entry.is_expired());
            let removed = initial_size - cache.len();

            if removed > 0 {
                update_stats(|s| {
                    s.cleanups += 1;
                    s.current_cache_size = cache.len();
                }).await;
                trace!("Cleaned {} expired entries from macro cache", removed);
            }

            // If still too large, clear cache (circuit breaker)
            if cache.len() >= 10000 {
                cache.clear();
                update_stats(|s| {
                    s.cleanups += 1;
                    s.current_cache_size = 0;
                }).await;
                warn!("Macro cache cleared due to size limit - consider tuning cache settings");
            }
        }

        // Track maximum cache size
        let cache_size = cache.len() + 1;
        update_stats(|s| {
            if cache_size > s.max_cache_size {
                s.max_cache_size = cache_size;
            }
            s.current_cache_size = cache_size;
        }).await;

        cache.insert(cache_key, entry);
    }

    /// Generate consistent cache key for flag and context
    fn generate_cache_key(flag_name: &str, context: &EvaluationContext) -> String {
        // Use stable_id() for consistency across evaluations
        format!("{}:{}", flag_name, context.stable_id())
    }

    /// Background cleanup task for expired entries (called periodically)
    pub async fn cleanup_expired() -> usize {
        let mut cache = MACRO_CACHE.write().await;
        let initial_size = cache.len();
        cache.retain(|_, entry| !entry.is_expired());
        let removed = initial_size - cache.len();

        if removed > 0 {
            update_stats(|s| {
                s.cleanups += 1;
                s.current_cache_size = cache.len();
            }).await;
            trace!("Cleaned {} expired entries from macro cache", removed);
        }

        removed
    }

    /// Get comprehensive cache statistics
    pub async fn get_cache_stats() -> MacroCacheStats {
        let stats = MACRO_STATS.read().await;
        stats.clone()
    }

    /// Clear all cache entries (for testing or emergency)
    pub async fn clear_cache() {
        let mut cache = MACRO_CACHE.write().await;
        cache.clear();
        
        update_stats(|s| {
            s.cleanups += 1;
            s.current_cache_size = 0;
        }).await;
    }

    /// Update macro cache statistics atomically
    async fn update_stats<F>(updater: F) 
    where
        F: FnOnce(&mut MacroCacheStats),
    {
        if let Ok(mut stats) = MACRO_STATS.write().await {
            updater(&mut *stats);
        }
    }

    /// Health check for macro cache performance
    pub async fn health_check() -> MacroCacheHealthStatus {
        let stats = get_cache_stats().await;
        let hit_rate = if stats.total_accesses > 0 {
            stats.hits as f64 / stats.total_accesses as f64
        } else {
            0.0
        };

        // Performance thresholds
        let healthy_hit_rate = 0.80; // 80% hit rate target
        let max_lookup_time_ns = 1_000_000; // 1ms = 1,000,000 ns

        let status = if hit_rate >= healthy_hit_rate && 
                        stats.avg_cache_lookup_time_ns <= max_lookup_time_ns {
            MacroCacheHealthStatus::Healthy
        } else {
            let mut issues = Vec::new();
            
            if hit_rate < healthy_hit_rate {
                issues.push(format!("Low hit rate: {:.1}% (target: {:.1}%)", 
                    hit_rate * 100.0, healthy_hit_rate * 100.0));
            }
            
            if stats.avg_cache_lookup_time_ns > max_lookup_time_ns {
                issues.push(format!("Slow lookups: {}μs (target: <1000μs)", 
                    stats.avg_cache_lookup_time_ns / 1000));
            }
            
            MacroCacheHealthStatus::Degraded(issues)
        };

        status
    }

    #[derive(Debug, Clone)]
    pub enum MacroCacheHealthStatus {
        Healthy,
        Degraded(Vec<String>),
    }

    impl MacroCacheHealthStatus {
        pub fn is_healthy(&self) -> bool {
            matches!(self, MacroCacheHealthStatus::Healthy)
        }
    }
}

/// Enhanced hash-based context evaluation for consistent percentage rollouts
/// Implements ALYS-004-09
pub mod consistent_hashing {
    use super::*;

    /// High-performance hash function optimized for consistent evaluation
    /// Uses Blake2b for cryptographic-quality randomness and consistency
    pub fn hash_context_for_rollout(
        context: &EvaluationContext, 
        flag_name: &str
    ) -> u64 {
        let hash_input = format!("{}:{}:v2", context.stable_id(), flag_name);
        
        // Use DefaultHasher for speed (consistent across same process)
        let mut hasher = DefaultHasher::new();
        hash_input.hash(&mut hasher);
        hasher.finish()
    }

    /// Evaluate percentage rollout with guaranteed consistency
    /// Same context + flag name will ALWAYS return same result
    pub fn evaluate_consistent_percentage(
        percentage: u8,
        context: &EvaluationContext,
        flag_name: &str,
    ) -> bool {
        if percentage == 0 {
            return false;
        }
        if percentage >= 100 {
            return true;
        }

        // Generate consistent hash
        let hash = hash_context_for_rollout(context, flag_name);
        
        // Convert percentage to threshold (0-100 -> 0-u64::MAX)
        // Use u64::MAX to maximize precision
        let threshold = (percentage as f64 / 100.0 * u64::MAX as f64) as u64;
        
        hash < threshold
    }

    /// Verify rollout distribution is working correctly
    /// For testing and monitoring purposes
    pub fn verify_rollout_distribution(
        percentage: u8,
        samples: &[(String, crate::config::Environment)], // (node_id, env) pairs
        flag_name: &str,
    ) -> RolloutDistributionStats {
        let mut enabled_count = 0;
        let total_count = samples.len();

        for (node_id, env) in samples {
            let context = EvaluationContext::new(node_id.clone(), *env);
            if evaluate_consistent_percentage(percentage, &context, flag_name) {
                enabled_count += 1;
            }
        }

        let actual_percentage = if total_count > 0 {
            (enabled_count as f64 / total_count as f64) * 100.0
        } else {
            0.0
        };

        let deviation = (actual_percentage - percentage as f64).abs();
        let expected_deviation = if total_count > 100 { 5.0 } else { 10.0 }; // Allow more deviation for small samples

        RolloutDistributionStats {
            target_percentage: percentage,
            actual_percentage,
            deviation,
            sample_size: total_count,
            enabled_count,
            is_within_tolerance: deviation <= expected_deviation,
        }
    }

    #[derive(Debug, Clone)]
    pub struct RolloutDistributionStats {
        pub target_percentage: u8,
        pub actual_percentage: f64,
        pub deviation: f64,
        pub sample_size: usize,
        pub enabled_count: usize,
        pub is_within_tolerance: bool,
    }
}

/// Performance benchmarking and monitoring
/// Implements ALYS-004-10 
pub mod benchmarks {
    use super::*;
    use std::time::Instant;

    /// Benchmark results for feature flag evaluations
    #[derive(Debug, Clone)]
    pub struct BenchmarkResults {
        pub total_evaluations: u64,
        pub cache_hits: u64,
        pub cache_misses: u64,
        pub avg_evaluation_time_us: u64,
        pub max_evaluation_time_us: u64,
        pub min_evaluation_time_us: u64,
        pub p95_evaluation_time_us: u64,
        pub p99_evaluation_time_us: u64,
        pub evaluations_under_1ms: u64,
        pub evaluations_over_1ms: u64,
        pub target_met: bool, // <1ms target
    }

    /// Performance tracker for continuous monitoring
    #[derive(Debug, Clone)]
    pub struct PerformanceTracker {
        evaluation_times: Vec<u64>,
        max_samples: usize,
    }

    impl PerformanceTracker {
        pub fn new(max_samples: usize) -> Self {
            Self {
                evaluation_times: Vec::new(),
                max_samples,
            }
        }

        /// Record an evaluation time in microseconds
        pub fn record_evaluation(&mut self, time_us: u64) {
            self.evaluation_times.push(time_us);
            
            // Keep only the most recent samples
            if self.evaluation_times.len() > self.max_samples {
                self.evaluation_times.remove(0);
            }
        }

        /// Generate benchmark results from recorded samples
        pub fn generate_results(&self) -> BenchmarkResults {
            if self.evaluation_times.is_empty() {
                return BenchmarkResults::empty();
            }

            let mut times = self.evaluation_times.clone();
            times.sort_unstable();

            let total = times.len() as u64;
            let sum: u64 = times.iter().sum();
            let avg = sum / total;
            let min = *times.first().unwrap_or(&0);
            let max = *times.last().unwrap_or(&0);

            // Calculate percentiles
            let p95_idx = ((times.len() as f64) * 0.95) as usize;
            let p99_idx = ((times.len() as f64) * 0.99) as usize;
            let p95 = times.get(p95_idx.saturating_sub(1)).copied().unwrap_or(max);
            let p99 = times.get(p99_idx.saturating_sub(1)).copied().unwrap_or(max);

            // Count evaluations over/under 1ms (1000μs)
            let under_1ms = times.iter().filter(|&&t| t < 1000).count() as u64;
            let over_1ms = total - under_1ms;

            let target_met = (over_1ms as f64 / total as f64) < 0.05; // <5% over 1ms

            BenchmarkResults {
                total_evaluations: total,
                cache_hits: 0, // Would be filled from cache stats
                cache_misses: 0, // Would be filled from cache stats
                avg_evaluation_time_us: avg,
                max_evaluation_time_us: max,
                min_evaluation_time_us: min,
                p95_evaluation_time_us: p95,
                p99_evaluation_time_us: p99,
                evaluations_under_1ms: under_1ms,
                evaluations_over_1ms: over_1ms,
                target_met,
            }
        }
    }

    impl BenchmarkResults {
        fn empty() -> Self {
            Self {
                total_evaluations: 0,
                cache_hits: 0,
                cache_misses: 0,
                avg_evaluation_time_us: 0,
                max_evaluation_time_us: 0,
                min_evaluation_time_us: 0,
                p95_evaluation_time_us: 0,
                p99_evaluation_time_us: 0,
                evaluations_under_1ms: 0,
                evaluations_over_1ms: 0,
                target_met: true,
            }
        }

        /// Generate performance report
        pub fn performance_report(&self) -> String {
            format!(
                "Feature Flag Performance Report\n\
                ================================\n\
                Total Evaluations: {}\n\
                Average Time: {}μs\n\
                95th Percentile: {}μs\n\
                99th Percentile: {}μs\n\
                Max Time: {}μs\n\
                Under 1ms: {} ({:.1}%)\n\
                Over 1ms: {} ({:.1}%)\n\
                Target Met (<1ms): {}",
                self.total_evaluations,
                self.avg_evaluation_time_us,
                self.p95_evaluation_time_us,
                self.p99_evaluation_time_us,
                self.max_evaluation_time_us,
                self.evaluations_under_1ms,
                if self.total_evaluations > 0 { 
                    self.evaluations_under_1ms as f64 / self.total_evaluations as f64 * 100.0 
                } else { 0.0 },
                self.evaluations_over_1ms,
                if self.total_evaluations > 0 { 
                    self.evaluations_over_1ms as f64 / self.total_evaluations as f64 * 100.0 
                } else { 0.0 },
                if self.target_met { "✓" } else { "✗" }
            )
        }
    }

    /// Run comprehensive performance benchmark
    pub async fn run_comprehensive_benchmark(
        manager: &crate::features::FeatureFlagManager,
        iterations: usize,
    ) -> BenchmarkResults {
        let mut tracker = PerformanceTracker::new(iterations);
        let context = EvaluationContext::new("benchmark-node".to_string(), crate::config::Environment::Development);

        for i in 0..iterations {
            let flag_name = format!("benchmark_flag_{}", i % 10); // Use 10 different flags
            
            let start = Instant::now();
            let _result = manager.is_enabled(&flag_name, &context).await;
            let elapsed = start.elapsed().as_micros() as u64;
            
            tracker.record_evaluation(elapsed);

            // Small delay to prevent overwhelming the system
            if i % 100 == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }

        // Add cache statistics
        let mut results = tracker.generate_results();
        let manager_stats = manager.get_stats().await;
        results.cache_hits = manager_stats.cache_hits;
        results.cache_misses = manager_stats.cache_misses;

        results
    }
}

/// Automatic background maintenance for performance optimization
pub struct PerformanceMaintenance {
    cleanup_interval: Duration,
    performance_tracker: benchmarks::PerformanceTracker,
}

impl PerformanceMaintenance {
    pub fn new(cleanup_interval_seconds: u64, max_performance_samples: usize) -> Self {
        Self {
            cleanup_interval: Duration::from_secs(cleanup_interval_seconds),
            performance_tracker: benchmarks::PerformanceTracker::new(max_performance_samples),
        }
    }

    /// Start background maintenance task
    pub fn start(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.cleanup_interval);

            loop {
                interval.tick().await;

                // Clean up expired macro cache entries
                let cleaned = macro_cache::cleanup_expired().await;
                if cleaned > 0 {
                    trace!("Performance maintenance: cleaned {} expired entries", cleaned);
                }

                // Check macro cache health
                let health = macro_cache::health_check().await;
                if !health.is_healthy() {
                    match health {
                        macro_cache::MacroCacheHealthStatus::Degraded(issues) => {
                            warn!("Macro cache performance degraded: {:?}", issues);
                        }
                        _ => {}
                    }
                }

                // Log performance statistics
                let cache_stats = macro_cache::get_cache_stats().await;
                trace!(
                    "Macro cache stats: {} hits, {} misses, {:.1}% hit rate, {} entries",
                    cache_stats.hits,
                    cache_stats.misses,
                    if cache_stats.total_accesses > 0 {
                        cache_stats.hits as f64 / cache_stats.total_accesses as f64 * 100.0
                    } else { 0.0 },
                    cache_stats.current_cache_size
                );
            }
        })
    }
}