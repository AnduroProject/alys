//! Storage Actor Metrics
//!
//! Performance monitoring and metrics collection for StorageActor.
//! This module provides comprehensive metrics tracking, Prometheus integration,
//! and performance analysis tools for storage operations.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use actor_system::ActorMetrics;

/// Storage actor performance metrics
#[derive(Debug)]
pub struct StorageActorMetrics {
    /// Blocks stored successfully
    pub blocks_stored: u64,
    
    /// Blocks retrieved from storage
    pub blocks_retrieved: u64,
    
    /// Block lookups that resulted in not found
    pub blocks_not_found: u64,
    
    /// State updates performed
    pub state_updates: u64,
    
    /// State queries performed
    pub state_queries: u64,
    
    /// State lookups that resulted in not found
    pub state_not_found: u64,
    
    /// Total database operations processed
    pub operations_processed: u64,
    
    /// Write operations completed successfully
    pub writes_completed: u64,
    
    /// Write operations that failed
    pub writes_failed: u64,
    
    /// Batch operations executed
    pub batch_operations: u64,
    
    /// Chain head updates
    pub chain_head_updates: u64,
    
    /// Average block storage time
    pub avg_block_storage_time: MovingAverage,
    
    /// Average block retrieval time
    pub avg_block_retrieval_time: MovingAverage,
    
    /// Average state update time
    pub avg_state_update_time: MovingAverage,
    
    /// Average state query time
    pub avg_state_query_time: MovingAverage,
    
    /// Average batch operation time
    pub avg_batch_time: MovingAverage,
    
    /// Peak memory usage in bytes
    pub memory_usage_bytes: u64,
    
    /// Database size tracking
    pub database_size_bytes: u64,
    
    /// Cache hit statistics
    pub cache_hits: u64,
    pub cache_misses: u64,
    
    /// Error counters by category
    pub error_counters: ErrorCounters,
    
    /// Performance violations tracking
    pub performance_violations: PerformanceViolationTracker,
    
    /// Actor lifecycle tracking
    startup_time: Option<Instant>,
    total_runtime: Duration,
    last_metrics_report: Option<Instant>,
}

/// Moving average calculation for timing metrics
#[derive(Debug)]
pub struct MovingAverage {
    values: std::collections::VecDeque<f64>,
    window_size: usize,
    sum: f64,
}

/// Error counters for different failure types
#[derive(Debug)]
pub struct ErrorCounters {
    pub database_errors: u64,
    pub serialization_errors: u64,
    pub cache_errors: u64,
    pub timeout_errors: u64,
    pub corruption_errors: u64,
    pub disk_space_errors: u64,
}

/// Performance violation tracking for SLA monitoring
#[derive(Debug)]
pub struct PerformanceViolationTracker {
    pub slow_block_storage: u32,    // > 1s
    pub slow_block_retrieval: u32,  // > 100ms
    pub slow_state_updates: u32,    // > 50ms
    pub slow_state_queries: u32,    // > 10ms
    pub slow_batch_operations: u32, // > 5s
    pub memory_violations: u32,     // > threshold
    pub last_violation_at: Option<Instant>,
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: Instant,
    pub blocks_stored: u64,
    pub blocks_retrieved: u64,
    pub state_updates: u64,
    pub state_queries: u64,
    pub operations_processed: u64,
    pub avg_block_storage_time_ms: f64,
    pub avg_block_retrieval_time_ms: f64,
    pub avg_state_update_time_ms: f64,
    pub avg_state_query_time_ms: f64,
    pub cache_hit_rate: f64,
    pub total_errors: u64,
    pub memory_usage_mb: f64,
    pub database_size_mb: f64,
}

/// Storage performance alert thresholds
#[derive(Debug, Clone)]
pub struct StorageAlertThresholds {
    pub max_block_storage_time_ms: u64,
    pub max_block_retrieval_time_ms: u64,
    pub max_state_update_time_ms: u64,
    pub max_state_query_time_ms: u64,
    pub max_batch_operation_time_ms: u64,
    pub min_cache_hit_rate: f64,
    pub max_error_rate: f64,
    pub max_memory_usage_mb: u64,
}

impl StorageActorMetrics {
    /// Create a new metrics instance
    pub fn new() -> Self {
        Self {
            blocks_stored: 0,
            blocks_retrieved: 0,
            blocks_not_found: 0,
            state_updates: 0,
            state_queries: 0,
            state_not_found: 0,
            operations_processed: 0,
            writes_completed: 0,
            writes_failed: 0,
            batch_operations: 0,
            chain_head_updates: 0,
            avg_block_storage_time: MovingAverage::new(100),
            avg_block_retrieval_time: MovingAverage::new(200),
            avg_state_update_time: MovingAverage::new(200),
            avg_state_query_time: MovingAverage::new(500),
            avg_batch_time: MovingAverage::new(50),
            memory_usage_bytes: 0,
            database_size_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
            error_counters: ErrorCounters::default(),
            performance_violations: PerformanceViolationTracker::default(),
            startup_time: None,
            total_runtime: Duration::default(),
            last_metrics_report: None,
        }
    }
    
    /// Record actor startup
    pub fn record_actor_started(&mut self) {
        self.startup_time = Some(Instant::now());
    }
    
    /// Record actor shutdown
    pub fn record_actor_stopped(&mut self) {
        if let Some(startup) = self.startup_time {
            self.total_runtime = startup.elapsed();
        }
    }
    
    /// Record a successful block storage operation
    pub fn record_block_stored(&mut self, _height: u64, duration: Duration, _canonical: bool) {
        self.blocks_stored += 1;
        self.operations_processed += 1;
        
        let storage_time_ms = duration.as_millis() as f64;
        self.avg_block_storage_time.add(storage_time_ms);
        
        // Check for performance violations
        if storage_time_ms > 1000.0 { // 1 second threshold
            self.performance_violations.slow_block_storage += 1;
            self.performance_violations.last_violation_at = Some(Instant::now());
        }
    }
    
    /// Record a block retrieval operation
    pub fn record_block_retrieved(&mut self, duration: Duration, from_cache: bool) {
        self.blocks_retrieved += 1;
        self.operations_processed += 1;
        
        if from_cache {
            self.cache_hits += 1;
        } else {
            self.cache_misses += 1;
        }
        
        let retrieval_time_ms = duration.as_millis() as f64;
        self.avg_block_retrieval_time.add(retrieval_time_ms);
        
        // Check for performance violations
        if retrieval_time_ms > 100.0 { // 100ms threshold
            self.performance_violations.slow_block_retrieval += 1;
            self.performance_violations.last_violation_at = Some(Instant::now());
        }
    }
    
    /// Record a block not found result
    pub fn record_block_not_found(&mut self) {
        self.blocks_not_found += 1;
        self.operations_processed += 1;
    }
    
    /// Record a state update operation
    pub fn record_state_update(&mut self, duration: Duration) {
        self.state_updates += 1;
        self.operations_processed += 1;
        
        let update_time_ms = duration.as_millis() as f64;
        self.avg_state_update_time.add(update_time_ms);
        
        // Check for performance violations
        if update_time_ms > 50.0 { // 50ms threshold
            self.performance_violations.slow_state_updates += 1;
            self.performance_violations.last_violation_at = Some(Instant::now());
        }
    }
    
    /// Record a state query operation
    pub fn record_state_query(&mut self, duration: Duration, from_cache: bool) {
        self.state_queries += 1;
        self.operations_processed += 1;
        
        if from_cache {
            self.cache_hits += 1;
        } else {
            self.cache_misses += 1;
        }
        
        let query_time_ms = duration.as_millis() as f64;
        self.avg_state_query_time.add(query_time_ms);
        
        // Check for performance violations
        if query_time_ms > 10.0 { // 10ms threshold
            self.performance_violations.slow_state_queries += 1;
            self.performance_violations.last_violation_at = Some(Instant::now());
        }
    }
    
    /// Record a state not found result
    pub fn record_state_not_found(&mut self) {
        self.state_not_found += 1;
        self.operations_processed += 1;
    }
    
    /// Record a batch operation
    pub fn record_batch_operation(&mut self, operation_count: usize, duration: Duration) {
        self.batch_operations += 1;
        self.operations_processed += operation_count as u64;
        
        let batch_time_ms = duration.as_millis() as f64;
        self.avg_batch_time.add(batch_time_ms);
        
        // Check for performance violations
        if batch_time_ms > 5000.0 { // 5 second threshold
            self.performance_violations.slow_batch_operations += 1;
            self.performance_violations.last_violation_at = Some(Instant::now());
        }
    }
    
    /// Record a write completion
    pub fn record_write_completion(&mut self) {
        self.writes_completed += 1;
    }
    
    /// Record a write failure
    pub fn record_write_failure(&mut self) {
        self.writes_failed += 1;
        self.error_counters.database_errors += 1;
    }
    
    /// Record a chain head update
    pub fn record_chain_head_update(&mut self) {
        self.chain_head_updates += 1;
        self.operations_processed += 1;
    }
    
    /// Record database error
    pub fn record_database_error(&mut self) {
        self.error_counters.database_errors += 1;
    }
    
    /// Record serialization error
    pub fn record_serialization_error(&mut self) {
        self.error_counters.serialization_errors += 1;
    }
    
    /// Record cache error
    pub fn record_cache_error(&mut self) {
        self.error_counters.cache_errors += 1;
    }
    
    /// Update memory usage
    pub fn update_memory_usage(&mut self, bytes: u64) {
        self.memory_usage_bytes = bytes;
        
        // Check for memory violations (example: > 1GB)
        if bytes > 1_073_741_824 {
            self.performance_violations.memory_violations += 1;
            self.performance_violations.last_violation_at = Some(Instant::now());
        }
    }
    
    /// Update database size tracking
    pub fn update_database_size(&mut self, bytes: u64) {
        self.database_size_bytes = bytes;
    }
    
    /// Get total error count
    pub fn total_errors(&self) -> u64 {
        self.error_counters.database_errors +
        self.error_counters.serialization_errors +
        self.error_counters.cache_errors +
        self.error_counters.timeout_errors +
        self.error_counters.corruption_errors +
        self.error_counters.disk_space_errors
    }
    
    /// Calculate cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let total_requests = self.cache_hits + self.cache_misses;
        if total_requests > 0 {
            self.cache_hits as f64 / total_requests as f64
        } else {
            0.0
        }
    }
    
    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        if self.operations_processed > 0 {
            self.total_errors() as f64 / self.operations_processed as f64
        } else {
            0.0
        }
    }
    
    /// Create a metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp: Instant::now(),
            blocks_stored: self.blocks_stored,
            blocks_retrieved: self.blocks_retrieved,
            state_updates: self.state_updates,
            state_queries: self.state_queries,
            operations_processed: self.operations_processed,
            avg_block_storage_time_ms: self.avg_block_storage_time.current(),
            avg_block_retrieval_time_ms: self.avg_block_retrieval_time.current(),
            avg_state_update_time_ms: self.avg_state_update_time.current(),
            avg_state_query_time_ms: self.avg_state_query_time.current(),
            cache_hit_rate: self.cache_hit_rate(),
            total_errors: self.total_errors(),
            memory_usage_mb: self.memory_usage_bytes as f64 / (1024.0 * 1024.0),
            database_size_mb: self.database_size_bytes as f64 / (1024.0 * 1024.0),
        }
    }
    
    /// Check for alert conditions
    pub fn check_alerts(&self, thresholds: &StorageAlertThresholds) -> Vec<String> {
        let mut alerts = Vec::new();
        
        if self.avg_block_storage_time.current() > thresholds.max_block_storage_time_ms as f64 {
            alerts.push(format!("Block storage time exceeded: {:.2}ms > {}ms",
                self.avg_block_storage_time.current(), thresholds.max_block_storage_time_ms));
        }
        
        if self.avg_block_retrieval_time.current() > thresholds.max_block_retrieval_time_ms as f64 {
            alerts.push(format!("Block retrieval time exceeded: {:.2}ms > {}ms",
                self.avg_block_retrieval_time.current(), thresholds.max_block_retrieval_time_ms));
        }
        
        if self.avg_state_update_time.current() > thresholds.max_state_update_time_ms as f64 {
            alerts.push(format!("State update time exceeded: {:.2}ms > {}ms",
                self.avg_state_update_time.current(), thresholds.max_state_update_time_ms));
        }
        
        if self.avg_state_query_time.current() > thresholds.max_state_query_time_ms as f64 {
            alerts.push(format!("State query time exceeded: {:.2}ms > {}ms",
                self.avg_state_query_time.current(), thresholds.max_state_query_time_ms));
        }
        
        let cache_hit_rate = self.cache_hit_rate();
        if cache_hit_rate < thresholds.min_cache_hit_rate {
            alerts.push(format!("Cache hit rate too low: {:.2}% < {:.2}%",
                cache_hit_rate * 100.0, thresholds.min_cache_hit_rate * 100.0));
        }
        
        let error_rate = self.error_rate();
        if error_rate > thresholds.max_error_rate {
            alerts.push(format!("Error rate too high: {:.4}% > {:.4}%",
                error_rate * 100.0, thresholds.max_error_rate * 100.0));
        }
        
        let memory_mb = self.memory_usage_bytes / (1024 * 1024);
        if memory_mb > thresholds.max_memory_usage_mb {
            alerts.push(format!("Memory usage exceeded: {}MB > {}MB",
                memory_mb, thresholds.max_memory_usage_mb));
        }
        
        alerts
    }
    
    /// Export metrics in Prometheus format
    pub fn to_prometheus(&self, labels: &HashMap<String, String>) -> String {
        let mut output = String::new();
        
        let label_str = if labels.is_empty() {
            String::new()
        } else {
            let formatted_labels: Vec<String> = labels.iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, v))
                .collect();
            format!("{{{}}}", formatted_labels.join(","))
        };
        
        // Counter metrics
        output.push_str(&format!("alys_storage_blocks_stored_total{} {}\n", label_str, self.blocks_stored));
        output.push_str(&format!("alys_storage_blocks_retrieved_total{} {}\n", label_str, self.blocks_retrieved));
        output.push_str(&format!("alys_storage_state_updates_total{} {}\n", label_str, self.state_updates));
        output.push_str(&format!("alys_storage_state_queries_total{} {}\n", label_str, self.state_queries));
        output.push_str(&format!("alys_storage_operations_processed_total{} {}\n", label_str, self.operations_processed));
        
        // Timing metrics
        output.push_str(&format!("alys_storage_block_storage_time_ms{} {:.2}\n", 
            label_str, self.avg_block_storage_time.current()));
        output.push_str(&format!("alys_storage_block_retrieval_time_ms{} {:.2}\n", 
            label_str, self.avg_block_retrieval_time.current()));
        output.push_str(&format!("alys_storage_state_update_time_ms{} {:.2}\n", 
            label_str, self.avg_state_update_time.current()));
        output.push_str(&format!("alys_storage_state_query_time_ms{} {:.2}\n", 
            label_str, self.avg_state_query_time.current()));
        
        // Performance metrics
        output.push_str(&format!("alys_storage_cache_hit_rate{} {:.4}\n", label_str, self.cache_hit_rate()));
        output.push_str(&format!("alys_storage_error_rate{} {:.6}\n", label_str, self.error_rate()));
        
        // Resource usage
        let memory_mb = self.memory_usage_bytes as f64 / (1024.0 * 1024.0);
        output.push_str(&format!("alys_storage_memory_usage_mb{} {:.2}\n", label_str, memory_mb));
        
        let db_size_mb = self.database_size_bytes as f64 / (1024.0 * 1024.0);
        output.push_str(&format!("alys_storage_database_size_mb{} {:.2}\n", label_str, db_size_mb));
        
        // Error counters
        output.push_str(&format!("alys_storage_database_errors_total{} {}\n", 
            label_str, self.error_counters.database_errors));
        output.push_str(&format!("alys_storage_serialization_errors_total{} {}\n", 
            label_str, self.error_counters.serialization_errors));
        
        output
    }
    
    /// Convert to custom metrics map for ActorMetrics
    pub fn to_custom_metrics(&self) -> HashMap<String, f64> {
        let mut metrics = HashMap::new();
        
        metrics.insert("blocks_stored".to_string(), self.blocks_stored as f64);
        metrics.insert("blocks_retrieved".to_string(), self.blocks_retrieved as f64);
        metrics.insert("state_updates".to_string(), self.state_updates as f64);
        metrics.insert("state_queries".to_string(), self.state_queries as f64);
        metrics.insert("cache_hit_rate".to_string(), self.cache_hit_rate());
        metrics.insert("error_rate".to_string(), self.error_rate());
        metrics.insert("avg_block_storage_time_ms".to_string(), self.avg_block_storage_time.current());
        metrics.insert("avg_block_retrieval_time_ms".to_string(), self.avg_block_retrieval_time.current());
        metrics.insert("memory_usage_mb".to_string(), self.memory_usage_bytes as f64 / (1024.0 * 1024.0));
        metrics.insert("database_size_mb".to_string(), self.database_size_bytes as f64 / (1024.0 * 1024.0));
        
        metrics
    }
}

impl MovingAverage {
    /// Create a new moving average with the specified window size
    pub fn new(window_size: usize) -> Self {
        Self {
            values: std::collections::VecDeque::with_capacity(window_size),
            window_size,
            sum: 0.0,
        }
    }
    
    /// Add a new value to the moving average
    pub fn add(&mut self, value: f64) {
        if self.values.len() >= self.window_size {
            if let Some(old_value) = self.values.pop_front() {
                self.sum -= old_value;
            }
        }
        
        self.values.push_back(value);
        self.sum += value;
    }
    
    /// Get the current moving average value
    pub fn current(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f64
        }
    }
}

impl Default for ErrorCounters {
    fn default() -> Self {
        Self {
            database_errors: 0,
            serialization_errors: 0,
            cache_errors: 0,
            timeout_errors: 0,
            corruption_errors: 0,
            disk_space_errors: 0,
        }
    }
}

impl Default for PerformanceViolationTracker {
    fn default() -> Self {
        Self {
            slow_block_storage: 0,
            slow_block_retrieval: 0,
            slow_state_updates: 0,
            slow_state_queries: 0,
            slow_batch_operations: 0,
            memory_violations: 0,
            last_violation_at: None,
        }
    }
}

impl Default for StorageAlertThresholds {
    fn default() -> Self {
        Self {
            max_block_storage_time_ms: 1000,     // 1 second
            max_block_retrieval_time_ms: 100,    // 100ms
            max_state_update_time_ms: 50,        // 50ms
            max_state_query_time_ms: 10,         // 10ms
            max_batch_operation_time_ms: 5000,   // 5 seconds
            min_cache_hit_rate: 0.8,             // 80%
            max_error_rate: 0.01,                // 1%
            max_memory_usage_mb: 1024,           // 1GB
        }
    }
}