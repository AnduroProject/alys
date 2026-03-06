//! Storage Actor metrics and monitoring - V2
//!
//! This module provides comprehensive metrics collection and monitoring
//! for the storage actor performance and health.

use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_gauge, register_histogram, register_int_counter, register_int_gauge,
    Counter, Gauge, Histogram, IntCounter, IntGauge,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::*;

lazy_static! {
    // Block storage metrics
    static ref BLOCKS_STORED: IntCounter = register_int_counter!(
        "alys_storage_blocks_stored_total",
        "Total number of blocks stored"
    ).unwrap();

    static ref BLOCKS_RETRIEVED: IntCounter = register_int_counter!(
        "alys_storage_blocks_retrieved_total",
        "Total number of blocks retrieved"
    ).unwrap();

    static ref BLOCK_NOT_FOUND: IntCounter = register_int_counter!(
        "alys_storage_blocks_not_found_total",
        "Total number of block retrieval misses"
    ).unwrap();

    static ref BLOCK_STORAGE_DURATION: Histogram = register_histogram!(
        "alys_storage_block_storage_duration_seconds",
        "Time taken to store a block",
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]
    ).unwrap();

    static ref BLOCK_RETRIEVAL_DURATION: Histogram = register_histogram!(
        "alys_storage_block_retrieval_duration_seconds",
        "Time taken to retrieve a block",
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
    ).unwrap();

    // State storage metrics
    static ref STATE_UPDATES: IntCounter = register_int_counter!(
        "alys_storage_state_updates_total",
        "Total number of state updates"
    ).unwrap();

    static ref STATE_QUERIES: IntCounter = register_int_counter!(
        "alys_storage_state_queries_total",
        "Total number of state queries"
    ).unwrap();

    static ref STATE_NOT_FOUND: IntCounter = register_int_counter!(
        "alys_storage_state_not_found_total",
        "Total number of state query misses"
    ).unwrap();

    static ref STATE_UPDATE_DURATION: Histogram = register_histogram!(
        "alys_storage_state_update_duration_seconds",
        "Time taken to update state",
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]
    ).unwrap();

    static ref STATE_QUERY_DURATION: Histogram = register_histogram!(
        "alys_storage_state_query_duration_seconds",
        "Time taken to query state",
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]
    ).unwrap();

    // Cache metrics
    static ref CACHE_HITS: IntCounter = register_int_counter!(
        "alys_storage_cache_hits_total",
        "Total number of cache hits"
    ).unwrap();

    static ref CACHE_MISSES: IntCounter = register_int_counter!(
        "alys_storage_cache_misses_total",
        "Total number of cache misses"
    ).unwrap();

    static ref CACHE_MEMORY_USAGE: Gauge = register_gauge!(
        "alys_storage_cache_memory_bytes",
        "Current cache memory usage in bytes"
    ).unwrap();

    // Write operation metrics
    static ref WRITE_OPERATIONS: IntCounter = register_int_counter!(
        "alys_storage_write_operations_total",
        "Total number of write operations"
    ).unwrap();

    static ref WRITE_FAILURES: IntCounter = register_int_counter!(
        "alys_storage_write_failures_total",
        "Total number of write operation failures"
    ).unwrap();

    static ref BATCH_OPERATIONS: IntCounter = register_int_counter!(
        "alys_storage_batch_operations_total",
        "Total number of batch operations"
    ).unwrap();

    static ref BATCH_SIZE: Histogram = register_histogram!(
        "alys_storage_batch_size",
        "Size of batch operations",
        vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0]
    ).unwrap();

    static ref BATCH_DURATION: Histogram = register_histogram!(
        "alys_storage_batch_duration_seconds",
        "Time taken for batch operations",
        vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0]
    ).unwrap();

    // Chain head metrics
    static ref CHAIN_HEAD_UPDATES: IntCounter = register_int_counter!(
        "alys_storage_chain_head_updates_total",
        "Total number of chain head updates"
    ).unwrap();

    static ref CURRENT_CHAIN_HEIGHT: IntGauge = register_int_gauge!(
        "alys_storage_current_chain_height",
        "Current chain head height"
    ).unwrap();

    // Database metrics
    static ref DATABASE_SIZE: Gauge = register_gauge!(
        "alys_storage_database_size_bytes",
        "Current database size in bytes"
    ).unwrap();

    static ref COMPACTION_COUNT: IntCounter = register_int_counter!(
        "alys_storage_compaction_operations_total",
        "Total number of database compaction operations"
    ).unwrap();

    // Actor lifecycle metrics
    static ref ACTOR_STARTS: IntCounter = register_int_counter!(
        "alys_storage_actor_starts_total",
        "Total number of storage actor starts"
    ).unwrap();

    static ref ACTOR_STOPS: IntCounter = register_int_counter!(
        "alys_storage_actor_stops_total",
        "Total number of storage actor stops"
    ).unwrap();

    static ref ACTOR_UPTIME: Gauge = register_gauge!(
        "alys_storage_actor_uptime_seconds",
        "Storage actor uptime in seconds"
    ).unwrap();
}

/// Storage actor metrics collector
///
/// Task 1.2: Uses atomic counters for thread-safe updates from async contexts.
/// This fixes the metrics clone loss issue where cloning for async blocks
/// would create separate copies that didn't update the original.
#[derive(Debug)]
pub struct StorageActorMetrics {
    pub blocks_stored: AtomicU64,
    pub blocks_retrieved: AtomicU64,
    pub state_updates: AtomicU64,
    pub state_queries: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub write_operations: AtomicU64,
    pub write_failures: AtomicU64,
    pub batch_operations: AtomicU64,
}

impl Clone for StorageActorMetrics {
    fn clone(&self) -> Self {
        // Clone by loading current values atomically
        Self {
            blocks_stored: AtomicU64::new(self.blocks_stored.load(Ordering::Relaxed)),
            blocks_retrieved: AtomicU64::new(self.blocks_retrieved.load(Ordering::Relaxed)),
            state_updates: AtomicU64::new(self.state_updates.load(Ordering::Relaxed)),
            state_queries: AtomicU64::new(self.state_queries.load(Ordering::Relaxed)),
            cache_hits: AtomicU64::new(self.cache_hits.load(Ordering::Relaxed)),
            cache_misses: AtomicU64::new(self.cache_misses.load(Ordering::Relaxed)),
            write_operations: AtomicU64::new(self.write_operations.load(Ordering::Relaxed)),
            write_failures: AtomicU64::new(self.write_failures.load(Ordering::Relaxed)),
            batch_operations: AtomicU64::new(self.batch_operations.load(Ordering::Relaxed)),
        }
    }
}

/// Alert thresholds for storage monitoring
#[derive(Debug, Clone)]
pub struct StorageAlertThresholds {
    pub max_cache_miss_rate: f64,
    pub max_write_failure_rate: f64,
    pub max_storage_duration_ms: u64,
    pub max_memory_usage_mb: f64,
    pub max_database_size_gb: f64,
}

impl StorageActorMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            blocks_stored: AtomicU64::new(0),
            blocks_retrieved: AtomicU64::new(0),
            state_updates: AtomicU64::new(0),
            state_queries: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            write_operations: AtomicU64::new(0),
            write_failures: AtomicU64::new(0),
            batch_operations: AtomicU64::new(0),
        }
    }

    /// Record actor startup
    pub fn record_startup(&self) {
        ACTOR_STARTS.inc();
        info!("Storage actor startup recorded");
    }

    /// Record actor shutdown
    pub fn record_shutdown(&self) {
        ACTOR_STOPS.inc();
        info!("Storage actor shutdown recorded");
    }

    /// Record a block storage operation
    pub fn record_block_stored(&self, height: u64, duration: Duration, canonical: bool) {
        self.blocks_stored.fetch_add(1, Ordering::Relaxed);
        BLOCKS_STORED.inc();
        BLOCK_STORAGE_DURATION.observe(duration.as_secs_f64());

        if canonical {
            CURRENT_CHAIN_HEIGHT.set(height as i64);
        }

        debug!(
            "Block storage recorded: height={}, duration={:?}, canonical={}",
            height, duration, canonical
        );
    }

    /// Record a block retrieval operation
    pub fn record_block_retrieved(&self, duration: Duration, from_cache: bool) {
        self.blocks_retrieved.fetch_add(1, Ordering::Relaxed);
        BLOCKS_RETRIEVED.inc();
        BLOCK_RETRIEVAL_DURATION.observe(duration.as_secs_f64());

        if from_cache {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            CACHE_HITS.inc();
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
            CACHE_MISSES.inc();
        }

        debug!(
            "Block retrieval recorded: duration={:?}, from_cache={}",
            duration, from_cache
        );
    }

    /// Record a block not found
    pub fn record_block_not_found(&self) {
        BLOCK_NOT_FOUND.inc();
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        CACHE_MISSES.inc();
    }

    /// Record a state update operation
    pub fn record_state_update(&self, duration: Duration) {
        self.state_updates.fetch_add(1, Ordering::Relaxed);
        STATE_UPDATES.inc();
        STATE_UPDATE_DURATION.observe(duration.as_secs_f64());

        debug!("State update recorded: duration={:?}", duration);
    }

    /// Record a state query operation
    pub fn record_state_query(&self, duration: Duration, from_cache: bool) {
        self.state_queries.fetch_add(1, Ordering::Relaxed);
        STATE_QUERIES.inc();
        STATE_QUERY_DURATION.observe(duration.as_secs_f64());

        if from_cache {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            CACHE_HITS.inc();
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
            CACHE_MISSES.inc();
        }

        debug!(
            "State query recorded: duration={:?}, from_cache={}",
            duration, from_cache
        );
    }

    /// Record a state not found
    pub fn record_state_not_found(&self) {
        STATE_NOT_FOUND.inc();
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        CACHE_MISSES.inc();
    }

    /// Record a batch operation
    pub fn record_batch_operation(&self, batch_size: usize, duration: Duration) {
        self.batch_operations.fetch_add(1, Ordering::Relaxed);
        BATCH_OPERATIONS.inc();
        BATCH_SIZE.observe(batch_size as f64);
        BATCH_DURATION.observe(duration.as_secs_f64());

        info!(
            "Batch operation recorded: size={}, duration={:?}",
            batch_size, duration
        );
    }

    /// Record a write completion
    pub fn record_write_completion(&self) {
        self.write_operations.fetch_add(1, Ordering::Relaxed);
        WRITE_OPERATIONS.inc();
    }

    /// Record a write failure
    pub fn record_write_failure(&self) {
        self.write_failures.fetch_add(1, Ordering::Relaxed);
        WRITE_FAILURES.inc();
        warn!("Write operation failure recorded");
    }

    /// Record chain head update
    pub fn record_chain_head_update(&self) {
        CHAIN_HEAD_UPDATES.inc();
        debug!("Chain head update recorded");
    }

    /// Update cache memory usage
    pub fn update_cache_memory_usage(&self, bytes: f64) {
        CACHE_MEMORY_USAGE.set(bytes);
    }

    /// Update database size
    pub fn update_database_size(&self, bytes: f64) {
        DATABASE_SIZE.set(bytes);
    }

    /// Record database compaction
    pub fn record_compaction(&self) {
        COMPACTION_COUNT.inc();
        info!("Database compaction recorded");
    }

    /// Update actor uptime
    pub fn update_uptime(&self, seconds: f64) {
        ACTOR_UPTIME.set(seconds);
    }

    /// Calculate cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Calculate write failure rate
    pub fn write_failure_rate(&self) -> f64 {
        let operations = self.write_operations.load(Ordering::Relaxed);
        let failures = self.write_failures.load(Ordering::Relaxed);
        let total = operations + failures;
        if total > 0 {
            failures as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Check if any alert thresholds are exceeded
    pub fn check_alerts(&self, thresholds: &StorageAlertThresholds) -> Vec<String> {
        let mut alerts = Vec::new();

        // Check cache miss rate
        if self.cache_hit_rate() < (1.0 - thresholds.max_cache_miss_rate) {
            alerts.push(format!(
                "High cache miss rate: {:.2}% (threshold: {:.2}%)",
                (1.0 - self.cache_hit_rate()) * 100.0,
                thresholds.max_cache_miss_rate * 100.0
            ));
        }

        // Check write failure rate
        if self.write_failure_rate() > thresholds.max_write_failure_rate {
            alerts.push(format!(
                "High write failure rate: {:.2}% (threshold: {:.2}%)",
                self.write_failure_rate() * 100.0,
                thresholds.max_write_failure_rate * 100.0
            ));
        }

        alerts
    }

    /// Get metrics summary
    pub fn summary(&self) -> String {
        format!(
            "StorageMetrics {{ blocks_stored: {}, blocks_retrieved: {}, state_updates: {}, cache_hit_rate: {:.2}%, write_failure_rate: {:.2}% }}",
            self.blocks_stored.load(Ordering::Relaxed),
            self.blocks_retrieved.load(Ordering::Relaxed),
            self.state_updates.load(Ordering::Relaxed),
            self.cache_hit_rate() * 100.0,
            self.write_failure_rate() * 100.0
        )
    }

    /// Get blocks stored count (for logging)
    pub fn get_blocks_stored(&self) -> u64 {
        self.blocks_stored.load(Ordering::Relaxed)
    }

    /// Get blocks retrieved count (for logging)
    pub fn get_blocks_retrieved(&self) -> u64 {
        self.blocks_retrieved.load(Ordering::Relaxed)
    }
}

impl Default for StorageActorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for StorageAlertThresholds {
    fn default() -> Self {
        Self {
            max_cache_miss_rate: 0.2,      // 20% cache miss rate
            max_write_failure_rate: 0.01,  // 1% write failure rate
            max_storage_duration_ms: 1000, // 1 second storage duration
            max_memory_usage_mb: 1024.0,   // 1GB memory usage
            max_database_size_gb: 100.0,   // 100GB database size
        }
    }
}
