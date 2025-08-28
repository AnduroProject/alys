//! Engine Actor Metrics
//!
//! Comprehensive metrics collection and reporting for the EngineActor,
//! including Prometheus integration and performance monitoring.

use std::time::{Duration, Instant, SystemTime};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use prometheus::{Counter, Histogram, Gauge, IntGauge, register_counter, register_histogram, register_gauge, register_int_gauge};
use serde::{Deserialize, Serialize};
use tracing::*;

/// Engine actor metrics for performance monitoring and alerting
#[derive(Debug)]
pub struct EngineActorMetrics {
    // === Payload Metrics ===
    /// Total number of payloads built
    pub payloads_built: AtomicU64,
    
    /// Total number of payloads executed
    pub payloads_executed: AtomicU64,
    
    /// Total number of payload operations that failed
    pub failures: AtomicU64,
    
    /// Total number of payload timeouts
    pub timeouts: AtomicU64,
    
    /// Total number of payloads retrieved
    pub payloads_retrieved: AtomicU64,
    
    /// Total number of payload not found errors
    pub payloads_not_found: AtomicU64,
    
    // === Performance Metrics ===
    /// Payload build time histogram
    pub build_time_histogram: Histogram,
    
    /// Payload execution time histogram
    pub execution_time_histogram: Histogram,
    
    /// Client response time histogram
    pub client_response_histogram: Histogram,
    
    /// Current number of active payloads
    pub active_payloads: AtomicUsize,
    
    /// Peak number of concurrent payloads
    pub peak_concurrent_payloads: AtomicUsize,
    
    // === Health Metrics ===
    /// Total number of health checks performed
    pub health_checks_performed: AtomicU64,
    
    /// Number of health check failures
    pub health_check_failures: AtomicU64,
    
    /// Current client health status (0 = unhealthy, 1 = healthy)
    pub client_health_status: IntGauge,
    
    /// Client uptime percentage
    pub client_uptime_gauge: Gauge,
    
    // === Actor Lifecycle Metrics ===
    /// Number of times actor was started
    pub actor_starts: AtomicU64,
    
    /// Number of times actor was stopped
    pub actor_stops: AtomicU64,
    
    /// Number of times actor was restarted
    pub actor_restarts: AtomicU64,
    
    /// Actor uptime
    pub actor_started_at: Instant,
    
    // === Error Metrics ===
    /// Client connection errors
    pub connection_errors: AtomicU64,
    
    /// Authentication failures
    pub auth_failures: AtomicU64,
    
    /// RPC errors
    pub rpc_errors: AtomicU64,
    
    /// Network timeouts
    pub network_timeouts: AtomicU64,
    
    // === Integration Metrics ===
    /// Messages received from ChainActor
    pub chain_messages: AtomicU64,
    
    /// Messages sent to StorageActor
    pub storage_messages: AtomicU64,
    
    /// Messages sent to BridgeActor
    pub bridge_messages: AtomicU64,
    
    /// Messages received from NetworkActor
    pub network_messages: AtomicU64,
    
    // === Specialized Metrics ===
    /// Number of forkchoice updates processed
    pub forkchoice_updates: AtomicU64,
    
    /// Number of sync status changes handled
    pub sync_status_changes: AtomicU64,
    
    /// Number of reorgs handled
    pub reorgs_handled: AtomicU64,
    
    /// Number of stuck payloads detected
    pub stuck_payloads_detected: AtomicU64,
    
    /// Number of orphaned payloads cleaned up
    pub orphaned_payloads_cleaned: AtomicU64,
}

/// Snapshot of engine metrics for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineMetricsSnapshot {
    /// Timestamp when snapshot was taken
    pub timestamp: SystemTime,
    
    /// Payload metrics
    pub payloads_built: u64,
    pub payloads_executed: u64,
    pub failures: u64,
    pub success_rate: f64,
    
    /// Performance metrics
    pub avg_build_time_ms: u64,
    pub avg_execution_time_ms: u64,
    pub avg_client_response_ms: u64,
    pub active_payloads: usize,
    pub peak_concurrent_payloads: usize,
    
    /// Health metrics
    pub client_healthy: bool,
    pub health_checks_performed: u64,
    pub health_check_failures: u64,
    pub client_uptime_percentage: f64,
    
    /// Actor lifecycle
    pub actor_uptime_ms: u64,
    pub actor_restarts: u64,
    
    /// Error rates
    pub connection_error_rate: f64,
    pub rpc_error_rate: f64,
    pub timeout_rate: f64,
    
    /// Integration metrics
    pub chain_message_count: u64,
    pub storage_message_count: u64,
    pub bridge_message_count: u64,
    pub network_message_count: u64,
}

impl Default for EngineActorMetrics {
    fn default() -> Self {
        Self {
            // Payload metrics
            payloads_built: AtomicU64::new(0),
            payloads_executed: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            payloads_retrieved: AtomicU64::new(0),
            payloads_not_found: AtomicU64::new(0),
            
            // Performance metrics
            build_time_histogram: register_histogram!(
                "engine_payload_build_duration_seconds",
                "Time spent building execution payloads",
                prometheus::exponential_buckets(0.001, 2.0, 15).unwrap()
            ).unwrap(),
            
            execution_time_histogram: register_histogram!(
                "engine_payload_execution_duration_seconds", 
                "Time spent executing payloads",
                prometheus::exponential_buckets(0.001, 2.0, 15).unwrap()
            ).unwrap(),
            
            client_response_histogram: register_histogram!(
                "engine_client_response_duration_seconds",
                "Client response time for RPC calls",
                prometheus::exponential_buckets(0.001, 2.0, 15).unwrap()
            ).unwrap(),
            
            active_payloads: AtomicUsize::new(0),
            peak_concurrent_payloads: AtomicUsize::new(0),
            
            // Health metrics
            health_checks_performed: AtomicU64::new(0),
            health_check_failures: AtomicU64::new(0),
            
            client_health_status: register_int_gauge!(
                "engine_client_health_status",
                "Current health status of execution client (0=unhealthy, 1=healthy)"
            ).unwrap(),
            
            client_uptime_gauge: register_gauge!(
                "engine_client_uptime_percentage",
                "Uptime percentage of execution client"
            ).unwrap(),
            
            // Actor lifecycle
            actor_starts: AtomicU64::new(0),
            actor_stops: AtomicU64::new(0),
            actor_restarts: AtomicU64::new(0),
            actor_started_at: Instant::now(),
            
            // Error metrics
            connection_errors: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            rpc_errors: AtomicU64::new(0),
            network_timeouts: AtomicU64::new(0),
            
            // Integration metrics
            chain_messages: AtomicU64::new(0),
            storage_messages: AtomicU64::new(0),
            bridge_messages: AtomicU64::new(0),
            network_messages: AtomicU64::new(0),
            
            // Specialized metrics
            forkchoice_updates: AtomicU64::new(0),
            sync_status_changes: AtomicU64::new(0),
            reorgs_handled: AtomicU64::new(0),
            stuck_payloads_detected: AtomicU64::new(0),
            orphaned_payloads_cleaned: AtomicU64::new(0),
        }
    }
}

impl EngineActorMetrics {
    /// Record a payload build request
    pub fn payload_build_requested(&self) {
        // This would be recorded when the build starts, timing measured in handler
    }
    
    /// Record successful payload build with timing
    pub fn payload_build_completed(&self, duration: Duration) {
        self.payloads_built.fetch_add(1, Ordering::Relaxed);
        self.build_time_histogram.observe(duration.as_secs_f64());
    }
    
    /// Record payload execution request
    pub fn payload_execution_requested(&self) {
        // This would be recorded when execution starts
    }
    
    /// Record successful payload execution with timing
    pub fn payload_execution_completed(&self, duration: Duration) {
        self.payloads_executed.fetch_add(1, Ordering::Relaxed);
        self.execution_time_histogram.observe(duration.as_secs_f64());
    }
    
    /// Record payload retrieval
    pub fn payload_retrieved(&self) {
        self.payloads_retrieved.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record payload not found
    pub fn payload_not_found(&self) {
        self.payloads_not_found.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record payload timeout
    pub fn payload_timeout(&self) {
        self.timeouts.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record forkchoice update request
    pub fn forkchoice_update_requested(&self) {
        self.forkchoice_updates.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record health check performed
    pub fn health_check_performed(&self, passed: bool, duration: Duration) {
        self.health_checks_performed.fetch_add(1, Ordering::Relaxed);
        
        if !passed {
            self.health_check_failures.fetch_add(1, Ordering::Relaxed);
        }
        
        self.client_response_histogram.observe(duration.as_secs_f64());
        self.client_health_status.set(if passed { 1 } else { 0 });
    }
    
    /// Record sync status check
    pub fn sync_status_checked(&self) {
        self.sync_status_changes.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record sync completion
    pub fn sync_completed(&self) {
        info!("Engine sync completed - client is now ready");
    }
    
    /// Record sync start
    pub fn sync_started(&self) {
        info!("Engine sync started - client is syncing");
    }
    
    /// Record actor started
    pub fn actor_started(&self) {
        self.actor_starts.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record actor stopped
    pub fn actor_stopped(&self) {
        self.actor_stops.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record actor restarted
    pub fn actor_restarted(&self) {
        self.actor_restarts.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record connection error
    pub fn connection_error(&self) {
        self.connection_errors.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record authentication failure
    pub fn auth_failure(&self) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record RPC error
    pub fn rpc_error(&self) {
        self.rpc_errors.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record network timeout
    pub fn network_timeout(&self) {
        self.network_timeouts.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record message from ChainActor
    pub fn chain_message_received(&self) {
        self.chain_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record message sent to StorageActor
    pub fn storage_message_sent(&self) {
        self.storage_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record message sent to BridgeActor
    pub fn bridge_message_sent(&self) {
        self.bridge_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record message received from NetworkActor
    pub fn network_message_received(&self) {
        self.network_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Update active payload count
    pub fn update_active_payloads(&self, count: usize) {
        self.active_payloads.store(count, Ordering::Relaxed);
        
        // Update peak if this is a new high
        let current_peak = self.peak_concurrent_payloads.load(Ordering::Relaxed);
        if count > current_peak {
            self.peak_concurrent_payloads.store(count, Ordering::Relaxed);
        }
    }
    
    /// Create a snapshot of current metrics
    pub fn snapshot(&self) -> EngineMetricsSnapshot {
        let total_operations = self.payloads_built.load(Ordering::Relaxed) + 
                              self.payloads_executed.load(Ordering::Relaxed);
        let failures = self.failures.load(Ordering::Relaxed);
        
        let success_rate = if total_operations == 0 {
            1.0
        } else {
            (total_operations - failures) as f64 / total_operations as f64
        };
        
        let health_checks = self.health_checks_performed.load(Ordering::Relaxed);
        let health_failures = self.health_check_failures.load(Ordering::Relaxed);
        
        let connection_errors = self.connection_errors.load(Ordering::Relaxed);
        let rpc_errors = self.rpc_errors.load(Ordering::Relaxed);
        let timeouts = self.network_timeouts.load(Ordering::Relaxed);
        let total_requests = total_operations; // Approximation
        
        EngineMetricsSnapshot {
            timestamp: SystemTime::now(),
            payloads_built: self.payloads_built.load(Ordering::Relaxed),
            payloads_executed: self.payloads_executed.load(Ordering::Relaxed),
            failures,
            success_rate,
            avg_build_time_ms: self.get_avg_duration_ms(&self.build_time_histogram),
            avg_execution_time_ms: self.get_avg_duration_ms(&self.execution_time_histogram),
            avg_client_response_ms: self.get_avg_duration_ms(&self.client_response_histogram),
            active_payloads: self.active_payloads.load(Ordering::Relaxed),
            peak_concurrent_payloads: self.peak_concurrent_payloads.load(Ordering::Relaxed),
            client_healthy: self.client_health_status.get() == 1,
            health_checks_performed: health_checks,
            health_check_failures: health_failures,
            client_uptime_percentage: self.client_uptime_gauge.get(),
            actor_uptime_ms: self.actor_started_at.elapsed().as_millis() as u64,
            actor_restarts: self.actor_restarts.load(Ordering::Relaxed),
            connection_error_rate: if total_requests == 0 { 0.0 } else { connection_errors as f64 / total_requests as f64 },
            rpc_error_rate: if total_requests == 0 { 0.0 } else { rpc_errors as f64 / total_requests as f64 },
            timeout_rate: if total_requests == 0 { 0.0 } else { timeouts as f64 / total_requests as f64 },
            chain_message_count: self.chain_messages.load(Ordering::Relaxed),
            storage_message_count: self.storage_messages.load(Ordering::Relaxed),
            bridge_message_count: self.bridge_messages.load(Ordering::Relaxed),
            network_message_count: self.network_messages.load(Ordering::Relaxed),
        }
    }
    
    /// Get average duration from histogram in milliseconds
    fn get_avg_duration_ms(&self, histogram: &Histogram) -> u64 {
        let metric = histogram.get_sample_sum();
        let count = histogram.get_sample_count();
        
        if count == 0 {
            0
        } else {
            ((metric / count as f64) * 1000.0) as u64
        }
    }
    
    /// Log comprehensive metrics report
    pub fn log_metrics_report(&self) {
        let snapshot = self.snapshot();
        
        info!(
            "=== Engine Actor Metrics Report ===\n\
            Payload Operations:\n\
            - Built: {}\n\
            - Executed: {}\n\
            - Failures: {}\n\
            - Success Rate: {:.2}%\n\
            - Active: {}\n\
            - Peak Concurrent: {}\n\
            \n\
            Performance:\n\
            - Avg Build Time: {}ms\n\
            - Avg Execution Time: {}ms\n\
            - Avg Client Response: {}ms\n\
            \n\
            Health:\n\
            - Client Healthy: {}\n\
            - Health Checks: {}\n\
            - Health Failures: {}\n\
            - Client Uptime: {:.2}%\n\
            \n\
            Actor Lifecycle:\n\
            - Uptime: {}ms\n\
            - Restarts: {}\n\
            \n\
            Error Rates:\n\
            - Connection Errors: {:.2}%\n\
            - RPC Errors: {:.2}%\n\
            - Timeouts: {:.2}%\n\
            \n\
            Integration:\n\
            - Chain Messages: {}\n\
            - Storage Messages: {}\n\
            - Bridge Messages: {}\n\
            - Network Messages: {}",
            snapshot.payloads_built,
            snapshot.payloads_executed,
            snapshot.failures,
            snapshot.success_rate * 100.0,
            snapshot.active_payloads,
            snapshot.peak_concurrent_payloads,
            snapshot.avg_build_time_ms,
            snapshot.avg_execution_time_ms,
            snapshot.avg_client_response_ms,
            snapshot.client_healthy,
            snapshot.health_checks_performed,
            snapshot.health_check_failures,
            snapshot.client_uptime_percentage,
            snapshot.actor_uptime_ms,
            snapshot.actor_restarts,
            snapshot.connection_error_rate * 100.0,
            snapshot.rpc_error_rate * 100.0,
            snapshot.timeout_rate * 100.0,
            snapshot.chain_message_count,
            snapshot.storage_message_count,
            snapshot.bridge_message_count,
            snapshot.network_message_count
        );
    }
    
    /// Check if performance is within acceptable bounds
    pub fn is_performance_healthy(&self) -> bool {
        let snapshot = self.snapshot();
        
        // Define performance thresholds
        let max_build_time_ms = 500; // 500ms max build time
        let max_execution_time_ms = 1000; // 1s max execution time
        let min_success_rate = 0.95; // 95% min success rate
        let max_error_rate = 0.05; // 5% max error rate
        
        snapshot.avg_build_time_ms <= max_build_time_ms &&
        snapshot.avg_execution_time_ms <= max_execution_time_ms &&
        snapshot.success_rate >= min_success_rate &&
        snapshot.connection_error_rate <= max_error_rate &&
        snapshot.rpc_error_rate <= max_error_rate &&
        snapshot.timeout_rate <= max_error_rate
    }
    
    /// Get alerting recommendations based on current metrics
    pub fn get_alerts(&self) -> Vec<MetricAlert> {
        let mut alerts = Vec::new();
        let snapshot = self.snapshot();
        
        // Performance alerts
        if snapshot.avg_build_time_ms > 500 {
            alerts.push(MetricAlert {
                severity: AlertSeverity::Warning,
                message: format!("High payload build time: {}ms", snapshot.avg_build_time_ms),
                threshold: 500,
                current_value: snapshot.avg_build_time_ms as f64,
            });
        }
        
        if snapshot.avg_execution_time_ms > 1000 {
            alerts.push(MetricAlert {
                severity: AlertSeverity::Critical,
                message: format!("High payload execution time: {}ms", snapshot.avg_execution_time_ms),
                threshold: 1000,
                current_value: snapshot.avg_execution_time_ms as f64,
            });
        }
        
        // Error rate alerts
        if snapshot.success_rate < 0.95 {
            alerts.push(MetricAlert {
                severity: AlertSeverity::Critical,
                message: format!("Low success rate: {:.2}%", snapshot.success_rate * 100.0),
                threshold: 95,
                current_value: snapshot.success_rate * 100.0,
            });
        }
        
        // Health alerts
        if !snapshot.client_healthy {
            alerts.push(MetricAlert {
                severity: AlertSeverity::Critical,
                message: "Execution client is unhealthy".to_string(),
                threshold: 1,
                current_value: 0.0,
            });
        }
        
        if snapshot.client_uptime_percentage < 99.0 {
            alerts.push(MetricAlert {
                severity: AlertSeverity::Warning,
                message: format!("Low client uptime: {:.2}%", snapshot.client_uptime_percentage),
                threshold: 99,
                current_value: snapshot.client_uptime_percentage,
            });
        }
        
        alerts
    }
}

/// Metric alert information
#[derive(Debug, Clone)]
pub struct MetricAlert {
    /// Alert severity level
    pub severity: AlertSeverity,
    
    /// Human-readable alert message
    pub message: String,
    
    /// Threshold that was exceeded
    pub threshold: u64,
    
    /// Current metric value
    pub current_value: f64,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    /// Informational alert
    Info,
    /// Warning that should be investigated
    Warning,
    /// Critical issue requiring immediate attention
    Critical,
}

/// Handler for MetricsReportMessage - periodic metrics reporting
impl Handler<MetricsReportMessage> for super::actor::EngineActor {
    type Result = ();

    fn handle(&mut self, _msg: MetricsReportMessage, _ctx: &mut Self::Context) -> Self::Result {
        // Log comprehensive metrics report
        self.metrics.log_metrics_report();
        
        // Check for performance issues and log alerts
        let alerts = self.metrics.get_alerts();
        if !alerts.is_empty() {
            warn!("Engine performance alerts detected:");
            for alert in alerts {
                match alert.severity {
                    AlertSeverity::Critical => {
                        error!("CRITICAL: {}", alert.message);
                    },
                    AlertSeverity::Warning => {
                        warn!("WARNING: {}", alert.message);
                    },
                    AlertSeverity::Info => {
                        info!("INFO: {}", alert.message);
                    }
                }
            }
        } else {
            debug!("All engine performance metrics within normal bounds");
        }
        
        // Update Prometheus metrics
        self.update_prometheus_metrics();
    }
}

impl super::actor::EngineActor {
    /// Update Prometheus metrics with current values
    fn update_prometheus_metrics(&mut self) {
        // Update client uptime percentage
        let uptime_percentage = self.calculate_client_uptime_percentage();
        self.metrics.client_uptime_gauge.set(uptime_percentage);
        
        // Update health status
        self.metrics.client_health_status.set(if self.health_monitor.is_healthy { 1 } else { 0 });
        
        // Active payload count is updated in real-time by the state management
    }
    
    /// Calculate client uptime percentage
    fn calculate_client_uptime_percentage(&self) -> f64 {
        let total_checks = self.metrics.health_checks_performed.load(Ordering::Relaxed);
        let failed_checks = self.metrics.health_check_failures.load(Ordering::Relaxed);
        
        if total_checks == 0 {
            100.0 // No checks yet, assume healthy
        } else {
            let successful_checks = total_checks - failed_checks;
            (successful_checks as f64 / total_checks as f64) * 100.0
        }
    }
}