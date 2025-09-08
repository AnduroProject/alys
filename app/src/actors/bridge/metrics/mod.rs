//! Bridge Actor Metrics Collection
//! 
//! Comprehensive metrics for bridge operations with actor_system compatibility

pub mod bridge_metrics;
pub mod pegin_metrics;
pub mod pegout_metrics;
pub mod stream_metrics;

pub use bridge_metrics::*;
pub use pegin_metrics::*;
pub use pegout_metrics::*;
pub use stream_metrics::*;

use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Core metrics trait for bridge actors
pub trait BridgeMetrics: Send + Sync {
    /// Record an operation completion
    fn record_operation(&self, operation: &str, duration: Duration, success: bool);
    
    /// Record an error
    fn record_error(&self, error_type: &str);
    
    /// Get current metrics snapshot
    fn get_metrics_snapshot(&self) -> MetricsSnapshot;
    
    /// Reset metrics
    fn reset_metrics(&self);
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Timestamp when snapshot was taken
    pub timestamp: Instant,
    
    /// Total operations count
    pub total_operations: u64,
    
    /// Successful operations count
    pub successful_operations: u64,
    
    /// Failed operations count
    pub failed_operations: u64,
    
    /// Total errors by type
    pub errors_by_type: HashMap<String, u64>,
    
    /// Average operation duration
    pub avg_operation_duration: Option<Duration>,
    
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    
    /// Operations per second
    pub operations_per_second: f64,
}

/// Base metrics collector for all bridge actors
pub struct BaseMetricsCollector {
    /// Actor name
    actor_name: String,
    
    /// Start time
    start_time: Instant,
    
    /// Total operations counter
    total_operations: AtomicU64,
    
    /// Successful operations counter
    successful_operations: AtomicU64,
    
    /// Failed operations counter
    failed_operations: AtomicU64,
    
    /// Error counters by type
    errors_by_type: Arc<std::sync::RwLock<HashMap<String, AtomicU64>>>,
    
    /// Operation duration tracking
    total_duration: Arc<std::sync::RwLock<Duration>>,
}

impl BaseMetricsCollector {
    pub fn new(actor_name: impl Into<String>) -> Self {
        Self {
            actor_name: actor_name.into(),
            start_time: Instant::now(),
            total_operations: AtomicU64::new(0),
            successful_operations: AtomicU64::new(0),
            failed_operations: AtomicU64::new(0),
            errors_by_type: Arc::new(std::sync::RwLock::new(HashMap::new())),
            total_duration: Arc::new(std::sync::RwLock::new(Duration::from_secs(0))),
        }
    }
    
    pub fn actor_name(&self) -> &str {
        &self.actor_name
    }
}

impl BridgeMetrics for BaseMetricsCollector {
    fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.total_operations.fetch_add(1, Ordering::Relaxed);
        
        if success {
            self.successful_operations.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_operations.fetch_add(1, Ordering::Relaxed);
        }
        
        // Update total duration
        if let Ok(mut total_duration) = self.total_duration.write() {
            *total_duration += duration;
        }
    }
    
    fn record_error(&self, error_type: &str) {
        if let Ok(mut errors) = self.errors_by_type.write() {
            let counter = errors.entry(error_type.to_string())
                .or_insert_with(|| AtomicU64::new(0));
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        let total_ops = self.total_operations.load(Ordering::Relaxed);
        let successful_ops = self.successful_operations.load(Ordering::Relaxed);
        let failed_ops = self.failed_operations.load(Ordering::Relaxed);
        
        let errors_by_type = if let Ok(errors) = self.errors_by_type.read() {
            errors.iter()
                .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
                .collect()
        } else {
            HashMap::new()
        };
        
        let avg_operation_duration = if total_ops > 0 {
            if let Ok(total_duration) = self.total_duration.read() {
                Some(*total_duration / total_ops as u32)
            } else {
                None
            }
        } else {
            None
        };
        
        let success_rate = if total_ops > 0 {
            successful_ops as f64 / total_ops as f64
        } else {
            0.0
        };
        
        let elapsed_time = self.start_time.elapsed();
        let operations_per_second = if elapsed_time.as_secs() > 0 {
            total_ops as f64 / elapsed_time.as_secs() as f64
        } else {
            0.0
        };
        
        MetricsSnapshot {
            timestamp: Instant::now(),
            total_operations: total_ops,
            successful_operations: successful_ops,
            failed_operations: failed_ops,
            errors_by_type,
            avg_operation_duration,
            success_rate,
            operations_per_second,
        }
    }
    
    fn reset_metrics(&self) {
        self.total_operations.store(0, Ordering::Relaxed);
        self.successful_operations.store(0, Ordering::Relaxed);
        self.failed_operations.store(0, Ordering::Relaxed);
        
        if let Ok(mut errors) = self.errors_by_type.write() {
            errors.clear();
        }
        
        if let Ok(mut total_duration) = self.total_duration.write() {
            *total_duration = Duration::from_secs(0);
        }
    }
}