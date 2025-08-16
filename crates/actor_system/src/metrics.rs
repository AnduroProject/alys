//! Actor performance metrics and monitoring

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Actor performance metrics
#[derive(Debug)]
pub struct ActorMetrics {
    /// Whether metrics collection is enabled
    enabled: bool,
    
    /// Message processing metrics
    pub messages_processed: AtomicU64,
    pub messages_failed: AtomicU64,
    pub message_processing_time: AtomicU64, // Total nanoseconds
    pub mailbox_size: AtomicU64,
    
    /// Lifecycle metrics
    pub restarts: AtomicU64,
    pub state_transitions: AtomicU64,
    pub last_activity: parking_lot::RwLock<SystemTime>,
    
    /// Performance metrics
    pub avg_response_time: parking_lot::RwLock<Duration>,
    pub peak_memory_usage: AtomicU64,
    pub cpu_time: AtomicU64, // Total CPU nanoseconds
    
    /// Error metrics
    pub error_counts: Arc<dashmap::DashMap<String, AtomicU64>>,
    
    /// Custom metrics
    pub custom_counters: Arc<dashmap::DashMap<String, AtomicU64>>,
    pub custom_gauges: Arc<dashmap::DashMap<String, parking_lot::RwLock<f64>>>,
}

impl ActorMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            enabled: true,
            messages_processed: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
            message_processing_time: AtomicU64::new(0),
            mailbox_size: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
            state_transitions: AtomicU64::new(0),
            last_activity: parking_lot::RwLock::new(SystemTime::now()),
            avg_response_time: parking_lot::RwLock::new(Duration::from_millis(0)),
            peak_memory_usage: AtomicU64::new(0),
            cpu_time: AtomicU64::new(0),
            error_counts: Arc::new(dashmap::DashMap::new()),
            custom_counters: Arc::new(dashmap::DashMap::new()),
            custom_gauges: Arc::new(dashmap::DashMap::new()),
        }
    }
    
    /// Create disabled metrics instance (no-op)
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            messages_processed: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
            message_processing_time: AtomicU64::new(0),
            mailbox_size: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
            state_transitions: AtomicU64::new(0),
            last_activity: parking_lot::RwLock::new(SystemTime::now()),
            avg_response_time: parking_lot::RwLock::new(Duration::from_millis(0)),
            peak_memory_usage: AtomicU64::new(0),
            cpu_time: AtomicU64::new(0),
            error_counts: Arc::new(dashmap::DashMap::new()),
            custom_counters: Arc::new(dashmap::DashMap::new()),
            custom_gauges: Arc::new(dashmap::DashMap::new()),
        }
    }
    
    /// Check if metrics are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// Record message processed
    pub fn record_message_processed(&self, processing_time: Duration) {
        if !self.enabled {
            return;
        }
        
        self.messages_processed.fetch_add(1, Ordering::Relaxed);
        self.message_processing_time.fetch_add(processing_time.as_nanos() as u64, Ordering::Relaxed);
        self.record_activity();
        
        // Update average response time
        let total_messages = self.messages_processed.load(Ordering::Relaxed);
        if total_messages > 0 {
            let total_time_nanos = self.message_processing_time.load(Ordering::Relaxed);
            let avg_nanos = total_time_nanos / total_messages;
            *self.avg_response_time.write() = Duration::from_nanos(avg_nanos);
        }
    }
    
    /// Record message failed
    pub fn record_message_failed(&self, error_type: &str) {
        if !self.enabled {
            return;
        }
        
        self.messages_failed.fetch_add(1, Ordering::Relaxed);
        self.record_error(error_type);
        self.record_activity();
    }
    
    /// Record error
    pub fn record_error(&self, error_type: &str) {
        if !self.enabled {
            return;
        }
        
        let counter = self.error_counts
            .entry(error_type.to_string())
            .or_insert_with(|| AtomicU64::new(0));
        counter.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record actor restart
    pub fn record_restart(&self) {
        if !self.enabled {
            return;
        }
        
        self.restarts.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }
    
    /// Record state transition
    pub fn record_state_transition(&self) {
        if !self.enabled {
            return;
        }
        
        self.state_transitions.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }
    
    /// Record activity timestamp
    pub fn record_activity(&self) {
        if !self.enabled {
            return;
        }
        
        *self.last_activity.write() = SystemTime::now();
    }
    
    /// Update mailbox size
    pub fn update_mailbox_size(&self, size: usize) {
        if !self.enabled {
            return;
        }
        
        self.mailbox_size.store(size as u64, Ordering::Relaxed);
    }
    
    /// Update memory usage
    pub fn update_memory_usage(&self, bytes: u64) {
        if !self.enabled {
            return;
        }
        
        let current_peak = self.peak_memory_usage.load(Ordering::Relaxed);
        if bytes > current_peak {
            self.peak_memory_usage.store(bytes, Ordering::Relaxed);
        }
    }
    
    /// Add CPU time
    pub fn add_cpu_time(&self, time: Duration) {
        if !self.enabled {
            return;
        }
        
        self.cpu_time.fetch_add(time.as_nanos() as u64, Ordering::Relaxed);
    }
    
    /// Increment custom counter
    pub fn increment_counter(&self, name: &str) {
        self.add_to_counter(name, 1);
    }
    
    /// Add to custom counter
    pub fn add_to_counter(&self, name: &str, value: u64) {
        if !self.enabled {
            return;
        }
        
        let counter = self.custom_counters
            .entry(name.to_string())
            .or_insert_with(|| AtomicU64::new(0));
        counter.fetch_add(value, Ordering::Relaxed);
    }
    
    /// Set custom gauge value
    pub fn set_gauge(&self, name: &str, value: f64) {
        if !self.enabled {
            return;
        }
        
        let gauge = self.custom_gauges
            .entry(name.to_string())
            .or_insert_with(|| parking_lot::RwLock::new(0.0));
        *gauge.write() = value;
    }
    
    /// Update custom gauge (add to current value)
    pub fn update_gauge(&self, name: &str, delta: f64) {
        if !self.enabled {
            return;
        }
        
        let gauge = self.custom_gauges
            .entry(name.to_string())
            .or_insert_with(|| parking_lot::RwLock::new(0.0));
        *gauge.write() += delta;
    }
    
    /// Get snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            enabled: self.enabled,
            messages_processed: self.messages_processed.load(Ordering::Relaxed),
            messages_failed: self.messages_failed.load(Ordering::Relaxed),
            avg_processing_time: if self.enabled {
                *self.avg_response_time.read()
            } else {
                Duration::from_millis(0)
            },
            mailbox_size: self.mailbox_size.load(Ordering::Relaxed),
            restarts: self.restarts.load(Ordering::Relaxed),
            state_transitions: self.state_transitions.load(Ordering::Relaxed),
            last_activity: if self.enabled {
                *self.last_activity.read()
            } else {
                SystemTime::now()
            },
            peak_memory_usage: self.peak_memory_usage.load(Ordering::Relaxed),
            total_cpu_time: Duration::from_nanos(self.cpu_time.load(Ordering::Relaxed)),
            error_counts: self.error_counts.iter()
                .map(|entry| (entry.key().clone(), entry.value().load(Ordering::Relaxed)))
                .collect(),
            custom_counters: self.custom_counters.iter()
                .map(|entry| (entry.key().clone(), entry.value().load(Ordering::Relaxed)))
                .collect(),
            custom_gauges: self.custom_gauges.iter()
                .map(|entry| (entry.key().clone(), *entry.value().read()))
                .collect(),
        }
    }
    
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.messages_processed.load(Ordering::Relaxed) + 
                   self.messages_failed.load(Ordering::Relaxed);
        
        if total == 0 {
            1.0
        } else {
            self.messages_processed.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
    
    /// Calculate messages per second (approximate)
    pub fn messages_per_second(&self, since: SystemTime) -> f64 {
        let duration = since.elapsed().unwrap_or_default();
        if duration.as_secs() == 0 {
            return 0.0;
        }
        
        let total_messages = self.messages_processed.load(Ordering::Relaxed);
        total_messages as f64 / duration.as_secs() as f64
    }
    
    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        let total = self.messages_processed.load(Ordering::Relaxed) + 
                   self.messages_failed.load(Ordering::Relaxed);
        
        if total == 0 {
            0.0
        } else {
            self.messages_failed.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
    
    /// Check if actor is healthy based on metrics
    pub fn is_healthy(&self) -> bool {
        let success_rate = self.success_rate();
        let error_rate = self.error_rate();
        
        success_rate > 0.95 && error_rate < 0.05
    }
    
    /// Reset all metrics
    pub fn reset(&self) {
        if !self.enabled {
            return;
        }
        
        self.messages_processed.store(0, Ordering::Relaxed);
        self.messages_failed.store(0, Ordering::Relaxed);
        self.message_processing_time.store(0, Ordering::Relaxed);
        self.mailbox_size.store(0, Ordering::Relaxed);
        self.restarts.store(0, Ordering::Relaxed);
        self.state_transitions.store(0, Ordering::Relaxed);
        self.peak_memory_usage.store(0, Ordering::Relaxed);
        self.cpu_time.store(0, Ordering::Relaxed);
        
        *self.last_activity.write() = SystemTime::now();
        *self.avg_response_time.write() = Duration::from_millis(0);
        
        self.error_counts.clear();
        self.custom_counters.clear();
        self.custom_gauges.clear();
    }
}

impl Default for ActorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ActorMetrics {
    fn clone(&self) -> Self {
        let snapshot = self.snapshot();
        let metrics = Self::new();
        
        metrics.messages_processed.store(snapshot.messages_processed, Ordering::Relaxed);
        metrics.messages_failed.store(snapshot.messages_failed, Ordering::Relaxed);
        metrics.mailbox_size.store(snapshot.mailbox_size, Ordering::Relaxed);
        metrics.restarts.store(snapshot.restarts, Ordering::Relaxed);
        metrics.state_transitions.store(snapshot.state_transitions, Ordering::Relaxed);
        metrics.peak_memory_usage.store(snapshot.peak_memory_usage, Ordering::Relaxed);
        
        *metrics.last_activity.write() = snapshot.last_activity;
        *metrics.avg_response_time.write() = snapshot.avg_processing_time;
        
        for (key, value) in snapshot.error_counts {
            metrics.error_counts.insert(key, AtomicU64::new(value));
        }
        
        for (key, value) in snapshot.custom_counters {
            metrics.custom_counters.insert(key, AtomicU64::new(value));
        }
        
        for (key, value) in snapshot.custom_gauges {
            metrics.custom_gauges.insert(key, parking_lot::RwLock::new(value));
        }
        
        metrics
    }
}

/// Immutable snapshot of metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub enabled: bool,
    pub messages_processed: u64,
    pub messages_failed: u64,
    pub avg_processing_time: Duration,
    pub mailbox_size: u64,
    pub restarts: u64,
    pub state_transitions: u64,
    pub last_activity: SystemTime,
    pub peak_memory_usage: u64,
    pub total_cpu_time: Duration,
    pub error_counts: HashMap<String, u64>,
    pub custom_counters: HashMap<String, u64>,
    pub custom_gauges: HashMap<String, f64>,
}

impl MetricsSnapshot {
    /// Calculate success rate from snapshot
    pub fn success_rate(&self) -> f64 {
        let total = self.messages_processed + self.messages_failed;
        if total == 0 {
            1.0
        } else {
            self.messages_processed as f64 / total as f64
        }
    }
    
    /// Calculate error rate from snapshot
    pub fn error_rate(&self) -> f64 {
        let total = self.messages_processed + self.messages_failed;
        if total == 0 {
            0.0
        } else {
            self.messages_failed as f64 / total as f64
        }
    }
    
    /// Get age since last activity
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed().unwrap_or_default()
    }
    
    /// Check if snapshot indicates healthy actor
    pub fn is_healthy(&self) -> bool {
        self.success_rate() > 0.95 && self.error_rate() < 0.05
    }
}

/// Metrics collector for aggregating metrics across multiple actors
#[derive(Debug)]
pub struct MetricsCollector {
    actor_metrics: Arc<dashmap::DashMap<String, Arc<ActorMetrics>>>,
    collection_interval: Duration,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new(collection_interval: Duration) -> Self {
        Self {
            actor_metrics: Arc::new(dashmap::DashMap::new()),
            collection_interval,
        }
    }
    
    /// Register actor for metrics collection
    pub fn register_actor(&self, actor_name: String, metrics: Arc<ActorMetrics>) {
        self.actor_metrics.insert(actor_name, metrics);
    }
    
    /// Unregister actor from metrics collection
    pub fn unregister_actor(&self, actor_name: &str) {
        self.actor_metrics.remove(actor_name);
    }
    
    /// Get metrics for specific actor
    pub fn get_actor_metrics(&self, actor_name: &str) -> Option<MetricsSnapshot> {
        self.actor_metrics.get(actor_name)
            .map(|entry| entry.value().snapshot())
    }
    
    /// Get all actor metrics
    pub fn get_all_metrics(&self) -> HashMap<String, MetricsSnapshot> {
        self.actor_metrics.iter()
            .map(|entry| (entry.key().clone(), entry.value().snapshot()))
            .collect()
    }
    
    /// Get aggregate statistics
    pub fn get_aggregate_stats(&self) -> AggregateStats {
        let snapshots: Vec<_> = self.actor_metrics.iter()
            .map(|entry| entry.value().snapshot())
            .collect();
        
        if snapshots.is_empty() {
            return AggregateStats::default();
        }
        
        let total_messages: u64 = snapshots.iter().map(|s| s.messages_processed).sum();
        let total_failed: u64 = snapshots.iter().map(|s| s.messages_failed).sum();
        let total_restarts: u64 = snapshots.iter().map(|s| s.restarts).sum();
        let total_memory: u64 = snapshots.iter().map(|s| s.peak_memory_usage).sum();
        
        let avg_response_time = if !snapshots.is_empty() {
            let total_nanos: u64 = snapshots.iter()
                .map(|s| s.avg_processing_time.as_nanos() as u64)
                .sum();
            Duration::from_nanos(total_nanos / snapshots.len() as u64)
        } else {
            Duration::from_millis(0)
        };
        
        let healthy_actors = snapshots.iter().filter(|s| s.is_healthy()).count();
        
        AggregateStats {
            total_actors: snapshots.len(),
            healthy_actors,
            total_messages_processed: total_messages,
            total_messages_failed: total_failed,
            total_restarts,
            avg_response_time,
            total_memory_usage: total_memory,
            overall_success_rate: if total_messages + total_failed > 0 {
                total_messages as f64 / (total_messages + total_failed) as f64
            } else {
                1.0
            },
        }
    }
    
    /// Start metrics collection background task
    pub fn start_collection(&self) -> tokio::task::JoinHandle<()> {
        let collector = self.actor_metrics.clone();
        let interval = self.collection_interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                // Collect and potentially export metrics
                let stats = Self::collect_stats(&collector);
                
                tracing::debug!(
                    total_actors = stats.total_actors,
                    healthy_actors = stats.healthy_actors,
                    success_rate = %format!("{:.2}%", stats.overall_success_rate * 100.0),
                    avg_response_time = ?stats.avg_response_time,
                    "Metrics collection completed"
                );
                
                // Here you could export metrics to external systems
                // like Prometheus, InfluxDB, etc.
            }
        })
    }
    
    fn collect_stats(collector: &dashmap::DashMap<String, Arc<ActorMetrics>>) -> AggregateStats {
        let snapshots: Vec<_> = collector.iter()
            .map(|entry| entry.value().snapshot())
            .collect();
        
        if snapshots.is_empty() {
            return AggregateStats::default();
        }
        
        let total_messages: u64 = snapshots.iter().map(|s| s.messages_processed).sum();
        let total_failed: u64 = snapshots.iter().map(|s| s.messages_failed).sum();
        let total_restarts: u64 = snapshots.iter().map(|s| s.restarts).sum();
        let total_memory: u64 = snapshots.iter().map(|s| s.peak_memory_usage).sum();
        
        let avg_response_time = if !snapshots.is_empty() {
            let total_nanos: u64 = snapshots.iter()
                .map(|s| s.avg_processing_time.as_nanos() as u64)
                .sum();
            Duration::from_nanos(total_nanos / snapshots.len() as u64)
        } else {
            Duration::from_millis(0)
        };
        
        let healthy_actors = snapshots.iter().filter(|s| s.is_healthy()).count();
        
        AggregateStats {
            total_actors: snapshots.len(),
            healthy_actors,
            total_messages_processed: total_messages,
            total_messages_failed: total_failed,
            total_restarts,
            avg_response_time,
            total_memory_usage: total_memory,
            overall_success_rate: if total_messages + total_failed > 0 {
                total_messages as f64 / (total_messages + total_failed) as f64
            } else {
                1.0
            },
        }
    }
}

/// Aggregate statistics across all actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateStats {
    pub total_actors: usize,
    pub healthy_actors: usize,
    pub total_messages_processed: u64,
    pub total_messages_failed: u64,
    pub total_restarts: u64,
    pub avg_response_time: Duration,
    pub total_memory_usage: u64,
    pub overall_success_rate: f64,
}

impl Default for AggregateStats {
    fn default() -> Self {
        Self {
            total_actors: 0,
            healthy_actors: 0,
            total_messages_processed: 0,
            total_messages_failed: 0,
            total_restarts: 0,
            avg_response_time: Duration::from_millis(0),
            total_memory_usage: 0,
            overall_success_rate: 1.0,
        }
    }
}

/// Mailbox-specific metrics
#[derive(Debug)]
pub struct MailboxMetrics {
    /// Messages queued
    pub messages_queued: AtomicU64,
    /// Messages processed
    pub messages_processed: AtomicU64,
    /// Messages dropped due to backpressure
    pub messages_dropped: AtomicU64,
    /// Current mailbox size
    pub current_size: std::sync::atomic::AtomicUsize,
    /// Maximum size reached
    pub max_size_reached: std::sync::atomic::AtomicUsize,
    /// Total wait time for messages
    pub total_wait_time: AtomicU64,
    /// Processing times for calculating averages
    pub processing_times: parking_lot::RwLock<Vec<Duration>>,
}

impl Default for MailboxMetrics {
    fn default() -> Self {
        Self {
            messages_queued: AtomicU64::new(0),
            messages_processed: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
            current_size: std::sync::atomic::AtomicUsize::new(0),
            max_size_reached: std::sync::atomic::AtomicUsize::new(0),
            total_wait_time: AtomicU64::new(0),
            processing_times: parking_lot::RwLock::new(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_metrics_basic_operations() {
        let metrics = ActorMetrics::new();
        
        // Test message processing
        metrics.record_message_processed(Duration::from_millis(100));
        assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 1);
        
        // Test failure recording
        metrics.record_message_failed("timeout");
        assert_eq!(metrics.messages_failed.load(Ordering::Relaxed), 1);
        
        // Test success rate
        assert_eq!(metrics.success_rate(), 0.5); // 1 success, 1 failure
    }
    
    #[test]
    fn test_custom_metrics() {
        let metrics = ActorMetrics::new();
        
        metrics.increment_counter("test_counter");
        metrics.add_to_counter("test_counter", 5);
        
        metrics.set_gauge("test_gauge", 42.0);
        metrics.update_gauge("test_gauge", 8.0);
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.custom_counters.get("test_counter"), Some(&6));
        assert_eq!(snapshot.custom_gauges.get("test_gauge"), Some(&50.0));
    }
    
    #[test]
    fn test_disabled_metrics() {
        let metrics = ActorMetrics::disabled();
        
        metrics.record_message_processed(Duration::from_millis(100));
        metrics.record_message_failed("error");
        metrics.increment_counter("test");
        
        // All operations should be no-ops
        assert_eq!(metrics.messages_processed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.messages_failed.load(Ordering::Relaxed), 0);
        assert!(metrics.custom_counters.is_empty());
    }
    
    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new(Duration::from_secs(1));
        
        let metrics1 = Arc::new(ActorMetrics::new());
        let metrics2 = Arc::new(ActorMetrics::new());
        
        collector.register_actor("actor1".to_string(), metrics1.clone());
        collector.register_actor("actor2".to_string(), metrics2.clone());
        
        metrics1.record_message_processed(Duration::from_millis(50));
        metrics2.record_message_processed(Duration::from_millis(75));
        
        let stats = collector.get_aggregate_stats();
        assert_eq!(stats.total_actors, 2);
        assert_eq!(stats.total_messages_processed, 2);
        assert_eq!(stats.overall_success_rate, 1.0);
    }
}