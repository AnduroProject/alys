//! Chain Actor Metrics
//!
//! Performance monitoring and metrics collection for ChainActor.
//! This module provides comprehensive metrics tracking, Prometheus integration,
//! and performance analysis tools for the chain actor system.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use actor_system::ActorMetrics;

/// Actor performance metrics for ChainActor
#[derive(Debug)]
pub struct ChainActorMetrics {
    /// Blocks produced by this actor
    pub blocks_produced: u64,
    
    /// Blocks imported successfully
    pub blocks_imported: u64,
    
    /// Blocks that failed validation
    pub validation_failures: u64,
    
    /// Chain reorganizations performed
    pub reorganizations: u32,
    
    /// Average block production time
    pub avg_production_time: MovingAverage,
    
    /// Average block import time
    pub avg_import_time: MovingAverage,
    
    /// Average validation time
    pub avg_validation_time: MovingAverage,
    
    /// Peak memory usage
    pub peak_memory_bytes: u64,
    
    /// Current queue depths
    pub queue_depths: QueueDepthTracker,
    
    /// Error counters
    pub error_counters: ErrorCounters,
    
    /// Performance violations
    pub performance_violations: PerformanceViolationTracker,
    
    /// Actor startup time
    startup_time: Option<Instant>,
    
    /// Total runtime
    total_runtime: Duration,
    
    /// Last metrics report time
    last_report: Option<Instant>,
}

/// Moving average calculation for performance metrics
#[derive(Debug)]
pub struct MovingAverage {
    values: VecDeque<f64>,
    window_size: usize,
    sum: f64,
}

/// Queue depth tracking for performance monitoring
#[derive(Debug)]
pub struct QueueDepthTracker {
    pub pending_blocks: usize,
    pub block_candidates: usize,
    pub validation_queue: usize,
    pub notification_queue: usize,
}

/// Error counters for monitoring different failure types
#[derive(Debug)]
pub struct ErrorCounters {
    pub validation_errors: u64,
    pub import_errors: u64,
    pub production_errors: u64,
    pub network_errors: u64,
    pub auxpow_errors: u64,
    pub peg_operation_errors: u64,
}

/// Performance violation tracking for SLA monitoring
#[derive(Debug)]
pub struct PerformanceViolationTracker {
    pub production_timeouts: u32,
    pub import_timeouts: u32,
    pub validation_timeouts: u32,
    pub memory_violations: u32,
    pub last_violation_at: Option<Instant>,
}

/// Prometheus metrics labels for better monitoring
#[derive(Debug, Clone)]
pub struct MetricsLabels {
    pub node_id: String,
    pub chain_id: String,
    pub version: String,
    pub environment: String,
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: Instant,
    pub blocks_produced: u64,
    pub blocks_imported: u64,
    pub avg_production_time_ms: f64,
    pub avg_import_time_ms: f64,
    pub avg_validation_time_ms: f64,
    pub total_errors: u64,
    pub queue_depths: QueueDepthTracker,
    pub memory_usage_mb: f64,
}

/// Performance alerts configuration
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub max_production_time_ms: u64,
    pub max_import_time_ms: u64,
    pub max_validation_time_ms: u64,
    pub max_queue_depth: usize,
    pub max_error_rate: f64,
    pub max_memory_mb: u64,
}

impl ChainActorMetrics {
    /// Create a new metrics instance
    pub fn new() -> Self {
        Self {
            blocks_produced: 0,
            blocks_imported: 0,
            validation_failures: 0,
            reorganizations: 0,
            avg_production_time: MovingAverage::new(50),
            avg_import_time: MovingAverage::new(100),
            avg_validation_time: MovingAverage::new(100),
            peak_memory_bytes: 0,
            queue_depths: QueueDepthTracker {
                pending_blocks: 0,
                block_candidates: 0,
                validation_queue: 0,
                notification_queue: 0,
            },
            error_counters: ErrorCounters {
                validation_errors: 0,
                import_errors: 0,
                production_errors: 0,
                network_errors: 0,
                auxpow_errors: 0,
                peg_operation_errors: 0,
            },
            performance_violations: PerformanceViolationTracker {
                production_timeouts: 0,
                import_timeouts: 0,
                validation_timeouts: 0,
                memory_violations: 0,
                last_violation_at: None,
            },
            startup_time: None,
            total_runtime: Duration::default(),
            last_report: None,
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
    
    /// Record a successful block production
    pub fn record_block_produced(&mut self, height: u64) {
        self.blocks_produced += 1;
    }
    
    /// Record a successful block import
    pub fn record_block_imported(&mut self, import_time: Duration) {
        self.blocks_imported += 1;
        self.avg_import_time.add(import_time.as_millis() as f64);
    }
    
    /// Record a block finalization
    pub fn record_block_finalized(&mut self, height: u64) {
        // Implementation for finalization metrics
    }
    
    /// Record a consensus failure
    pub fn record_consensus_failure(&mut self) {
        self.error_counters.validation_errors += 1;
    }
    
    /// Record a health check pass
    pub fn record_health_check_passed(&mut self) {
        // Implementation for health check metrics
    }
    
    /// Record a health check failure
    pub fn record_health_check_failed(&mut self) {
        // Implementation for health check metrics
    }
    
    /// Record block production time
    pub fn record_production_time(&mut self, duration: Duration) {
        self.avg_production_time.add(duration.as_millis() as f64);
    }
    
    /// Record validation time
    pub fn record_validation_time(&mut self, duration: Duration) {
        self.avg_validation_time.add(duration.as_millis() as f64);
    }
    
    /// Update queue depths
    pub fn update_queue_depths(&mut self, pending: usize, candidates: usize, validation: usize, notifications: usize) {
        self.queue_depths.pending_blocks = pending;
        self.queue_depths.block_candidates = candidates;
        self.queue_depths.validation_queue = validation;
        self.queue_depths.notification_queue = notifications;
    }
    
    /// Record memory usage
    pub fn record_memory_usage(&mut self, bytes: u64) {
        if bytes > self.peak_memory_bytes {
            self.peak_memory_bytes = bytes;
        }
    }
    
    /// Create a metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            timestamp: Instant::now(),
            blocks_produced: self.blocks_produced,
            blocks_imported: self.blocks_imported,
            avg_production_time_ms: self.avg_production_time.current(),
            avg_import_time_ms: self.avg_import_time.current(),
            avg_validation_time_ms: self.avg_validation_time.current(),
            total_errors: self.total_errors(),
            queue_depths: QueueDepthTracker {
                pending_blocks: self.queue_depths.pending_blocks,
                block_candidates: self.queue_depths.block_candidates,
                validation_queue: self.queue_depths.validation_queue,
                notification_queue: self.queue_depths.notification_queue,
            },
            memory_usage_mb: self.peak_memory_bytes as f64 / 1024.0 / 1024.0,
        }
    }
    
    /// Get total error count across all categories
    pub fn total_errors(&self) -> u64 {
        self.error_counters.validation_errors +
        self.error_counters.import_errors +
        self.error_counters.production_errors +
        self.error_counters.network_errors +
        self.error_counters.auxpow_errors +
        self.error_counters.peg_operation_errors
    }
    
    /// Check if any alert thresholds are exceeded
    pub fn check_alerts(&self, thresholds: &AlertThresholds) -> Vec<String> {
        let mut alerts = Vec::new();
        
        if self.avg_production_time.current() > thresholds.max_production_time_ms as f64 {
            alerts.push(format!("Block production time exceeded: {:.2}ms > {}ms", 
                self.avg_production_time.current(), thresholds.max_production_time_ms));
        }
        
        if self.avg_import_time.current() > thresholds.max_import_time_ms as f64 {
            alerts.push(format!("Block import time exceeded: {:.2}ms > {}ms",
                self.avg_import_time.current(), thresholds.max_import_time_ms));
        }
        
        if self.avg_validation_time.current() > thresholds.max_validation_time_ms as f64 {
            alerts.push(format!("Block validation time exceeded: {:.2}ms > {}ms",
                self.avg_validation_time.current(), thresholds.max_validation_time_ms));
        }
        
        if self.queue_depths.pending_blocks > thresholds.max_queue_depth {
            alerts.push(format!("Pending blocks queue depth exceeded: {} > {}",
                self.queue_depths.pending_blocks, thresholds.max_queue_depth));
        }
        
        let memory_mb = self.peak_memory_bytes / 1024 / 1024;
        if memory_mb > thresholds.max_memory_mb {
            alerts.push(format!("Memory usage exceeded: {}MB > {}MB",
                memory_mb, thresholds.max_memory_mb));
        }
        
        alerts
    }
    
    /// Export metrics in Prometheus format
    pub fn to_prometheus(&self, labels: &MetricsLabels) -> String {
        let mut output = String::new();
        
        // Block metrics
        output.push_str(&format!(
            "alys_chain_blocks_produced_total{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\"}} {}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.blocks_produced
        ));
        
        output.push_str(&format!(
            "alys_chain_blocks_imported_total{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\"}} {}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.blocks_imported
        ));
        
        // Timing metrics
        output.push_str(&format!(
            "alys_chain_block_production_time_ms{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\"}} {:.2}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.avg_production_time.current()
        ));
        
        output.push_str(&format!(
            "alys_chain_block_import_time_ms{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\"}} {:.2}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.avg_import_time.current()
        ));
        
        // Queue depth metrics
        output.push_str(&format!(
            "alys_chain_pending_blocks_queue_depth{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\"}} {}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.queue_depths.pending_blocks
        ));
        
        // Error metrics
        output.push_str(&format!(
            "alys_chain_errors_total{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\",type=\"validation\"}} {}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.error_counters.validation_errors
        ));
        
        output.push_str(&format!(
            "alys_chain_errors_total{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\",type=\"import\"}} {}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, self.error_counters.import_errors
        ));
        
        // Memory metrics
        let memory_mb = self.peak_memory_bytes as f64 / 1024.0 / 1024.0;
        output.push_str(&format!(
            "alys_chain_memory_usage_mb{{node_id=\"{}\",chain_id=\"{}\",version=\"{}\",environment=\"{}\"}} {:.2}\n",
            labels.node_id, labels.chain_id, labels.version, labels.environment, memory_mb
        ));
        
        output
    }
    
    /// Reset metrics (useful for testing)
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl MovingAverage {
    /// Create a new moving average with the specified window size
    pub fn new(window_size: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(window_size),
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
    
    /// Get the number of samples in the window
    pub fn sample_count(&self) -> usize {
        self.values.len()
    }
    
    /// Check if the window is full
    pub fn is_full(&self) -> bool {
        self.values.len() >= self.window_size
    }
    
    /// Clear all values
    pub fn clear(&mut self) {
        self.values.clear();
        self.sum = 0.0;
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_production_time_ms: 1000,
            max_import_time_ms: 200,
            max_validation_time_ms: 100,
            max_queue_depth: 200,
            max_error_rate: 0.05,
            max_memory_mb: 1024,
        }
    }
}

impl Clone for QueueDepthTracker {
    fn clone(&self) -> Self {
        Self {
            pending_blocks: self.pending_blocks,
            block_candidates: self.block_candidates,
            validation_queue: self.validation_queue,
            notification_queue: self.notification_queue,
        }
    }
}