//! Bridge Coordinator Metrics
//! 
//! Metrics collection and reporting for bridge coordination

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use crate::actors::bridge::messages::*;
use crate::types::*;

/// Bridge coordination metrics
#[derive(Debug, Clone)]
pub struct BridgeCoordinationMetrics {
    /// Operation counters
    operations_started: Arc<AtomicU64>,
    operations_completed: Arc<AtomicU64>,
    operations_failed: Arc<AtomicU64>,
    
    /// Operation type counters
    pegin_operations: Arc<AtomicU64>,
    pegout_operations: Arc<AtomicU64>,
    
    /// Actor registration counters
    actors_registered: Arc<AtomicU64>,
    actor_failures: Arc<AtomicU64>,
    
    /// System metrics
    system_starts: Arc<AtomicU64>,
    system_stops: Arc<AtomicU64>,
    uptime_start: SystemTime,
    
    /// Performance metrics
    operation_durations: Arc<std::sync::RwLock<Vec<Duration>>>,
    active_operations_gauge: Arc<AtomicU64>,
    
    /// Error tracking
    error_counts: Arc<std::sync::RwLock<HashMap<String, u64>>>,
    
    /// Timing metrics
    last_operation_time: Arc<std::sync::RwLock<Option<SystemTime>>>,
    
    /// Detailed metrics
    detailed_metrics: Arc<std::sync::RwLock<DetailedMetrics>>,
}

/// Detailed metrics structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedMetrics {
    /// Operations by status
    pub operations_by_status: HashMap<String, u64>,
    
    /// Operations by type and status
    pub operations_by_type_status: HashMap<String, HashMap<String, u64>>,
    
    /// Actor health metrics
    pub actor_health_metrics: HashMap<ActorType, ActorMetrics>,
    
    /// Performance statistics
    pub performance_stats: PerformanceStats,
    
    /// Error statistics
    pub error_stats: ErrorStats,
    
    /// Time-based metrics
    pub time_metrics: TimeMetrics,
}

/// Actor-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorMetrics {
    pub registrations: u64,
    pub failures: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub average_response_time: f64,
    pub last_activity: Option<SystemTime>,
}

/// Performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub average_operation_duration: f64,
    pub median_operation_duration: f64,
    pub p95_operation_duration: f64,
    pub p99_operation_duration: f64,
    pub operations_per_second: f64,
    pub peak_concurrent_operations: u64,
}

/// Error statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorStats {
    pub total_errors: u64,
    pub error_rate: f64,
    pub errors_by_type: HashMap<String, u64>,
    pub recent_error_rate: f64,
    pub mean_time_between_failures: f64,
}

/// Time-based metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeMetrics {
    pub system_uptime: Duration,
    pub time_since_last_operation: Option<Duration>,
    pub time_since_last_error: Option<Duration>,
    pub average_time_between_operations: f64,
}

impl BridgeCoordinationMetrics {
    /// Create new metrics instance
    pub fn new() -> Result<Self, BridgeError> {
        Ok(Self {
            operations_started: Arc::new(AtomicU64::new(0)),
            operations_completed: Arc::new(AtomicU64::new(0)),
            operations_failed: Arc::new(AtomicU64::new(0)),
            pegin_operations: Arc::new(AtomicU64::new(0)),
            pegout_operations: Arc::new(AtomicU64::new(0)),
            actors_registered: Arc::new(AtomicU64::new(0)),
            actor_failures: Arc::new(AtomicU64::new(0)),
            system_starts: Arc::new(AtomicU64::new(0)),
            system_stops: Arc::new(AtomicU64::new(0)),
            uptime_start: SystemTime::now(),
            operation_durations: Arc::new(std::sync::RwLock::new(Vec::new())),
            active_operations_gauge: Arc::new(AtomicU64::new(0)),
            error_counts: Arc::new(std::sync::RwLock::new(HashMap::new())),
            last_operation_time: Arc::new(std::sync::RwLock::new(None)),
            detailed_metrics: Arc::new(std::sync::RwLock::new(DetailedMetrics::default())),
        })
    }

    /// Record system start
    pub fn record_system_start(&self) {
        self.system_starts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record system stop
    pub fn record_system_stop(&self) {
        self.system_stops.fetch_add(1, Ordering::Relaxed);
    }

    /// Record actor registration
    pub fn record_actor_registration(&self, actor_type: ActorType) {
        self.actors_registered.fetch_add(1, Ordering::Relaxed);
        
        // Update detailed metrics
        if let Ok(mut detailed) = self.detailed_metrics.write() {
            let actor_metrics = detailed.actor_health_metrics
                .entry(actor_type)
                .or_insert_with(ActorMetrics::default);
            actor_metrics.registrations += 1;
            actor_metrics.last_activity = Some(SystemTime::now());
        }
    }

    /// Record actor failure
    pub fn record_actor_failure(&self, actor_type: &ActorType) {
        self.actor_failures.fetch_add(1, Ordering::Relaxed);
        
        // Update detailed metrics
        if let Ok(mut detailed) = self.detailed_metrics.write() {
            let actor_metrics = detailed.actor_health_metrics
                .entry(actor_type.clone())
                .or_insert_with(ActorMetrics::default);
            actor_metrics.failures += 1;
        }
        
        // Record error
        if let Ok(mut errors) = self.error_counts.write() {
            let error_key = format!("actor_failure_{:?}", actor_type);
            *errors.entry(error_key).or_insert(0) += 1;
        }
    }

    /// Record operation started
    pub fn record_operation_started(&self, operation_type: OperationType) {
        self.operations_started.fetch_add(1, Ordering::Relaxed);
        self.active_operations_gauge.fetch_add(1, Ordering::Relaxed);
        
        match operation_type {
            OperationType::PegIn => {
                self.pegin_operations.fetch_add(1, Ordering::Relaxed);
            }
            OperationType::PegOut => {
                self.pegout_operations.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        // Update last operation time
        if let Ok(mut last_time) = self.last_operation_time.write() {
            *last_time = Some(SystemTime::now());
        }
        
        // Update detailed metrics
        if let Ok(mut detailed) = self.detailed_metrics.write() {
            let type_key = format!("{:?}", operation_type);
            *detailed.operations_by_status.entry("started".to_string()).or_insert(0) += 1;
            
            let type_status = detailed.operations_by_type_status
                .entry(type_key)
                .or_insert_with(HashMap::new);
            *type_status.entry("started".to_string()).or_insert(0) += 1;
        }
    }

    /// Record operation completed
    pub fn record_operation_completed(&self, operation_type: &OperationType, success: bool) {
        self.active_operations_gauge.fetch_sub(1, Ordering::Relaxed);
        
        if success {
            self.operations_completed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.operations_failed.fetch_add(1, Ordering::Relaxed);
        }
        
        // Update detailed metrics
        if let Ok(mut detailed) = self.detailed_metrics.write() {
            let status = if success { "completed" } else { "failed" };
            let type_key = format!("{:?}", operation_type);
            
            *detailed.operations_by_status.entry(status.to_string()).or_insert(0) += 1;
            
            let type_status = detailed.operations_by_type_status
                .entry(type_key)
                .or_insert_with(HashMap::new);
            *type_status.entry(status.to_string()).or_insert(0) += 1;
        }
    }

    /// Record operation status change
    pub fn record_operation_status_change(
        &self,
        operation_type: &OperationType,
        old_status: &OperationState,
        new_status: &OperationState,
    ) {
        // Update detailed metrics
        if let Ok(mut detailed) = self.detailed_metrics.write() {
            let type_key = format!("{:?}", operation_type);
            let new_status_key = format!("{:?}", new_status);
            
            let type_status = detailed.operations_by_type_status
                .entry(type_key)
                .or_insert_with(HashMap::new);
            *type_status.entry(new_status_key).or_insert(0) += 1;
        }
    }

    /// Record operation duration
    pub fn record_operation_duration(&self, duration: Duration) {
        if let Ok(mut durations) = self.operation_durations.write() {
            durations.push(duration);
            
            // Keep only recent durations (last 1000)
            if durations.len() > 1000 {
                durations.drain(0..100);
            }
        }
    }

    /// Update active operations count
    pub fn update_active_operations(&self, count: usize) {
        self.active_operations_gauge.store(count as u64, Ordering::Relaxed);
    }

    /// Record successful operation
    pub fn record_successful_operation(&self) {
        // This is called from the operation completion handler
    }

    /// Record failed operation
    pub fn record_failed_operation(&self) {
        // This is called from the operation completion handler
    }

    /// Get current metrics snapshot
    pub fn get_current_metrics(&self) -> MetricsSnapshot {
        let operations_started = self.operations_started.load(Ordering::Relaxed);
        let operations_completed = self.operations_completed.load(Ordering::Relaxed);
        let operations_failed = self.operations_failed.load(Ordering::Relaxed);
        let active_operations = self.active_operations_gauge.load(Ordering::Relaxed);
        
        let uptime = SystemTime::now()
            .duration_since(self.uptime_start)
            .unwrap_or_default();

        let success_rate = if operations_started > 0 {
            operations_completed as f64 / operations_started as f64
        } else {
            0.0
        };

        let error_rate = if operations_started > 0 {
            operations_failed as f64 / operations_started as f64
        } else {
            0.0
        };

        MetricsSnapshot {
            operations_started,
            operations_completed,
            operations_failed,
            active_operations,
            pegin_operations: self.pegin_operations.load(Ordering::Relaxed),
            pegout_operations: self.pegout_operations.load(Ordering::Relaxed),
            actors_registered: self.actors_registered.load(Ordering::Relaxed),
            actor_failures: self.actor_failures.load(Ordering::Relaxed),
            uptime,
            success_rate,
            error_rate,
        }
    }

    /// Get total operations
    pub fn get_total_operations(&self) -> u64 {
        self.operations_started.load(Ordering::Relaxed)
    }

    /// Get error rate
    pub fn get_error_rate(&self) -> f64 {
        let operations_started = self.operations_started.load(Ordering::Relaxed);
        let operations_failed = self.operations_failed.load(Ordering::Relaxed);
        
        if operations_started > 0 {
            operations_failed as f64 / operations_started as f64
        } else {
            0.0
        }
    }

    /// Calculate performance statistics
    pub fn calculate_performance_stats(&self) -> PerformanceStats {
        let durations = self.operation_durations.read().unwrap();
        
        if durations.is_empty() {
            return PerformanceStats::default();
        }

        let mut sorted_durations = durations.clone();
        sorted_durations.sort();

        let total_duration: Duration = sorted_durations.iter().sum();
        let count = sorted_durations.len();

        let average_duration = total_duration.as_secs_f64() / count as f64;
        let median_duration = sorted_durations[count / 2].as_secs_f64();
        let p95_duration = sorted_durations[(count * 95) / 100].as_secs_f64();
        let p99_duration = sorted_durations[(count * 99) / 100].as_secs_f64();

        let uptime = SystemTime::now()
            .duration_since(self.uptime_start)
            .unwrap_or_default();
        
        let operations_per_second = if uptime.as_secs() > 0 {
            self.operations_completed.load(Ordering::Relaxed) as f64 / uptime.as_secs_f64()
        } else {
            0.0
        };

        PerformanceStats {
            average_operation_duration: average_duration,
            median_operation_duration: median_duration,
            p95_operation_duration: p95_duration,
            p99_operation_duration: p99_duration,
            operations_per_second,
            peak_concurrent_operations: self.active_operations_gauge.load(Ordering::Relaxed),
        }
    }
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub operations_started: u64,
    pub operations_completed: u64,
    pub operations_failed: u64,
    pub active_operations: u64,
    pub pegin_operations: u64,
    pub pegout_operations: u64,
    pub actors_registered: u64,
    pub actor_failures: u64,
    pub uptime: Duration,
    pub success_rate: f64,
    pub error_rate: f64,
}

impl Default for DetailedMetrics {
    fn default() -> Self {
        Self {
            operations_by_status: HashMap::new(),
            operations_by_type_status: HashMap::new(),
            actor_health_metrics: HashMap::new(),
            performance_stats: PerformanceStats::default(),
            error_stats: ErrorStats::default(),
            time_metrics: TimeMetrics::default(),
        }
    }
}

impl Default for ActorMetrics {
    fn default() -> Self {
        Self {
            registrations: 0,
            failures: 0,
            messages_sent: 0,
            messages_received: 0,
            average_response_time: 0.0,
            last_activity: None,
        }
    }
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            average_operation_duration: 0.0,
            median_operation_duration: 0.0,
            p95_operation_duration: 0.0,
            p99_operation_duration: 0.0,
            operations_per_second: 0.0,
            peak_concurrent_operations: 0,
        }
    }
}

impl Default for ErrorStats {
    fn default() -> Self {
        Self {
            total_errors: 0,
            error_rate: 0.0,
            errors_by_type: HashMap::new(),
            recent_error_rate: 0.0,
            mean_time_between_failures: 0.0,
        }
    }
}

impl Default for TimeMetrics {
    fn default() -> Self {
        Self {
            system_uptime: Duration::from_secs(0),
            time_since_last_operation: None,
            time_since_last_error: None,
            average_time_between_operations: 0.0,
        }
    }
}