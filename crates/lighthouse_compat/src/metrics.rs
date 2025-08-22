//! Metrics collection and reporting for the Lighthouse compatibility layer
//!
//! This module provides comprehensive metrics collection including performance metrics,
//! migration statistics, A/B test results, and Prometheus integration.

use crate::{
    config::MetricsConfig,
    error::{CompatError, CompatResult},
    types::{MigrationStats, ABTestResults},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[cfg(feature = "metrics")]
use prometheus::{
    Counter, Gauge, Histogram, Registry, HistogramOpts, Opts,
    register_counter_with_registry, register_gauge_with_registry,
    register_histogram_with_registry, Encoder, TextEncoder,
};

/// Metrics collector for the compatibility layer
pub struct MetricsCollector {
    /// Configuration
    config: MetricsConfig,
    
    /// Request metrics
    request_metrics: Arc<RwLock<RequestMetrics>>,
    
    /// Version-specific metrics
    version_metrics: Arc<RwLock<VersionMetrics>>,
    
    /// Migration metrics
    migration_metrics: Arc<RwLock<MigrationMetrics>>,
    
    /// Consensus metrics
    consensus_metrics: Arc<RwLock<ConsensusMetrics>>,
    
    /// Performance metrics
    performance_metrics: Arc<RwLock<PerformanceMetrics>>,
    
    /// Prometheus registry
    #[cfg(feature = "metrics")]
    prometheus_registry: Registry,
    
    /// Prometheus metrics
    #[cfg(feature = "metrics")]
    prometheus_metrics: PrometheusMetrics,
}

/// Request-level metrics
#[derive(Debug, Clone, Default)]
pub struct RequestMetrics {
    /// Total requests by method
    pub requests_by_method: HashMap<String, u64>,
    
    /// Successful requests by method
    pub successful_requests: HashMap<String, u64>,
    
    /// Failed requests by method
    pub failed_requests: HashMap<String, u64>,
    
    /// Request latencies by method
    pub request_latencies: HashMap<String, Vec<Duration>>,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Version-specific metrics
#[derive(Debug, Clone, Default)]
pub struct VersionMetrics {
    /// V4 metrics
    pub v4_metrics: ClientMetrics,
    
    /// V5 metrics
    pub v5_metrics: ClientMetrics,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Individual client metrics
#[derive(Debug, Clone, Default)]
pub struct ClientMetrics {
    /// Total requests
    pub total_requests: u64,
    
    /// Successful requests
    pub successful_requests: u64,
    
    /// Failed requests
    pub failed_requests: u64,
    
    /// Average response time
    pub avg_response_time: Duration,
    
    /// P95 response time
    pub p95_response_time: Duration,
    
    /// P99 response time
    pub p99_response_time: Duration,
    
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    
    /// Memory usage (MB)
    pub memory_usage_mb: u64,
    
    /// CPU usage (0.0 to 100.0)
    pub cpu_usage: f64,
}

/// Migration-specific metrics
#[derive(Debug, Clone, Default)]
pub struct MigrationMetrics {
    /// Mode change count
    pub mode_changes: u64,
    
    /// Time in each mode
    pub time_in_mode: HashMap<String, Duration>,
    
    /// Rollback count
    pub rollbacks: u64,
    
    /// Rollback reasons
    pub rollback_reasons: HashMap<String, u64>,
    
    /// Migration success rate
    pub migration_success_rate: f64,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Consensus comparison metrics
#[derive(Debug, Clone, Default)]
pub struct ConsensusMetrics {
    /// Total comparisons
    pub total_comparisons: u64,
    
    /// Matching results
    pub consensus_matches: u64,
    
    /// Mismatching results
    pub consensus_mismatches: u64,
    
    /// V4-only errors
    pub v4_only_errors: u64,
    
    /// V5-only errors
    pub v5_only_errors: u64,
    
    /// Both version errors
    pub both_errors: u64,
    
    /// Shadow execution metrics
    pub shadow_successes: u64,
    
    /// Shadow execution errors
    pub shadow_errors: u64,
    
    /// Fallback executions
    pub fallbacks: u64,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Performance-related metrics
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// Throughput (requests per second)
    pub throughput: f64,
    
    /// Overall latency percentiles
    pub latency_percentiles: LatencyPercentiles,
    
    /// Resource utilization
    pub resource_usage: ResourceUsage,
    
    /// Health score
    pub health_score: f64,
    
    /// Uptime
    pub uptime: Duration,
    
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Latency percentile measurements
#[derive(Debug, Clone, Default)]
pub struct LatencyPercentiles {
    /// P50 latency
    pub p50: Duration,
    
    /// P75 latency
    pub p75: Duration,
    
    /// P90 latency
    pub p90: Duration,
    
    /// P95 latency
    pub p95: Duration,
    
    /// P99 latency
    pub p99: Duration,
    
    /// P99.9 latency
    pub p999: Duration,
}

/// Resource usage metrics
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// Total memory usage (MB)
    pub memory_mb: u64,
    
    /// CPU usage percentage
    pub cpu_percent: f64,
    
    /// Network bandwidth usage (bytes/sec)
    pub network_bytes_per_sec: u64,
    
    /// Disk I/O (bytes/sec)
    pub disk_io_bytes_per_sec: u64,
    
    /// Open file descriptors
    pub open_fds: u32,
    
    /// Active connections
    pub active_connections: u32,
}

#[cfg(feature = "metrics")]
/// Prometheus metrics integration
struct PrometheusMetrics {
    // Request metrics
    requests_total: Counter,
    request_duration: Histogram,
    
    // Version metrics
    v4_requests_total: Counter,
    v5_requests_total: Counter,
    v4_response_time: Histogram,
    v5_response_time: Histogram,
    
    // Migration metrics
    mode_changes_total: Counter,
    rollbacks_total: Counter,
    migration_health: Gauge,
    
    // Consensus metrics
    consensus_matches_total: Counter,
    consensus_mismatches_total: Counter,
    
    // Performance metrics
    throughput: Gauge,
    memory_usage: Gauge,
    cpu_usage: Gauge,
    health_score: Gauge,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> CompatResult<Self> {
        info!("Initializing metrics collector");
        
        #[cfg(feature = "metrics")]
        let prometheus_registry = Registry::new();
        
        #[cfg(feature = "metrics")]
        let prometheus_metrics = Self::create_prometheus_metrics(&prometheus_registry)?;
        
        Ok(Self {
            config,
            request_metrics: Arc::new(RwLock::new(RequestMetrics::default())),
            version_metrics: Arc::new(RwLock::new(VersionMetrics::default())),
            migration_metrics: Arc::new(RwLock::new(MigrationMetrics::default())),
            consensus_metrics: Arc::new(RwLock::new(ConsensusMetrics::default())),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            #[cfg(feature = "metrics")]
            prometheus_registry,
            #[cfg(feature = "metrics")]
            prometheus_metrics,
        })
    }
    
    /// Record a request with its result and duration
    pub async fn record_request<T>(
        &self,
        method: &str,
        result: &CompatResult<T>,
        duration: Duration,
    ) {
        let mut request_metrics = self.request_metrics.write().await;
        
        // Update request counts
        *request_metrics.requests_by_method.entry(method.to_string()).or_insert(0) += 1;
        
        if result.is_ok() {
            *request_metrics.successful_requests.entry(method.to_string()).or_insert(0) += 1;
        } else {
            *request_metrics.failed_requests.entry(method.to_string()).or_insert(0) += 1;
        }
        
        // Update latencies
        request_metrics
            .request_latencies
            .entry(method.to_string())
            .or_insert_with(Vec::new)
            .push(duration);
        
        request_metrics.last_updated = SystemTime::now();
        
        #[cfg(feature = "metrics")]
        {
            self.prometheus_metrics.requests_total.inc();
            self.prometheus_metrics.request_duration.observe(duration.as_secs_f64());
        }
        
        debug!("Recorded request: method={}, success={}, duration={:?}", 
               method, result.is_ok(), duration);
    }
    
    /// Record a mode change
    pub async fn record_mode_change(&self) {
        let mut migration_metrics = self.migration_metrics.write().await;
        migration_metrics.mode_changes += 1;
        migration_metrics.last_updated = SystemTime::now();
        
        #[cfg(feature = "metrics")]
        self.prometheus_metrics.mode_changes_total.inc();
        
        info!("Recorded mode change");
    }
    
    /// Record a rollback
    pub async fn record_rollback(&self, reason: &str) {
        let mut migration_metrics = self.migration_metrics.write().await;
        migration_metrics.rollbacks += 1;
        *migration_metrics.rollback_reasons.entry(reason.to_string()).or_insert(0) += 1;
        migration_metrics.last_updated = SystemTime::now();
        
        #[cfg(feature = "metrics")]
        self.prometheus_metrics.rollbacks_total.inc();
        
        warn!("Recorded rollback: reason={}", reason);
    }
    
    /// Record consensus match
    pub async fn record_consensus_match(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.total_comparisons += 1;
        consensus_metrics.consensus_matches += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        #[cfg(feature = "metrics")]
        self.prometheus_metrics.consensus_matches_total.inc();
        
        debug!("Recorded consensus match for method: {}", method);
    }
    
    /// Record consensus mismatch
    pub async fn record_consensus_mismatch(&self, method: &str, details: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.total_comparisons += 1;
        consensus_metrics.consensus_mismatches += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        #[cfg(feature = "metrics")]
        self.prometheus_metrics.consensus_mismatches_total.inc();
        
        warn!("Recorded consensus mismatch for method: {}, details: {}", method, details);
    }
    
    /// Record V4-only error
    pub async fn record_v4_only_error(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.v4_only_errors += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        warn!("Recorded V4-only error for method: {}", method);
    }
    
    /// Record V5-only error
    pub async fn record_v5_only_error(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.v5_only_errors += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        warn!("Recorded V5-only error for method: {}", method);
    }
    
    /// Record both version errors
    pub async fn record_both_errors(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.both_errors += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        warn!("Recorded both version errors for method: {}", method);
    }
    
    /// Record shadow execution success
    pub async fn record_shadow_success(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.shadow_successes += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        debug!("Recorded shadow success for method: {}", method);
    }
    
    /// Record shadow execution error
    pub async fn record_shadow_error(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.shadow_errors += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        debug!("Recorded shadow error for method: {}", method);
    }
    
    /// Record fallback execution
    pub async fn record_fallback(&self, method: &str) {
        let mut consensus_metrics = self.consensus_metrics.write().await;
        consensus_metrics.fallbacks += 1;
        consensus_metrics.last_updated = SystemTime::now();
        
        info!("Recorded fallback for method: {}", method);
    }
    
    /// Update version-specific metrics
    pub async fn update_version_metrics(&self, version: &str, metrics: ClientMetrics) {
        let mut version_metrics = self.version_metrics.write().await;
        
        match version {
            "v4" => version_metrics.v4_metrics = metrics.clone(),
            "v5" => version_metrics.v5_metrics = metrics.clone(),
            _ => warn!("Unknown version for metrics update: {}", version),
        }
        
        version_metrics.last_updated = SystemTime::now();
        
        #[cfg(feature = "metrics")]
        {
            match version {
                "v4" => {
                    self.prometheus_metrics.v4_response_time
                        .observe(metrics.avg_response_time.as_secs_f64());
                }
                "v5" => {
                    self.prometheus_metrics.v5_response_time
                        .observe(metrics.avg_response_time.as_secs_f64());
                }
                _ => {}
            }
        }
        
        debug!("Updated {} metrics", version);
    }
    
    /// Update performance metrics
    pub async fn update_performance_metrics(&self, metrics: PerformanceMetrics) {
        *self.performance_metrics.write().await = metrics.clone();
        
        #[cfg(feature = "metrics")]
        {
            self.prometheus_metrics.throughput.set(metrics.throughput);
            self.prometheus_metrics.memory_usage.set(metrics.resource_usage.memory_mb as f64);
            self.prometheus_metrics.cpu_usage.set(metrics.resource_usage.cpu_percent);
            self.prometheus_metrics.health_score.set(metrics.health_score);
        }
        
        debug!("Updated performance metrics");
    }
    
    /// Get migration statistics
    pub async fn get_migration_stats(&self) -> MigrationStats {
        let request_metrics = self.request_metrics.read().await;
        let version_metrics = self.version_metrics.read().await;
        let consensus_metrics = self.consensus_metrics.read().await;
        
        let total_requests = request_metrics
            .requests_by_method
            .values()
            .sum::<u64>();
            
        let successful_requests = request_metrics
            .successful_requests
            .values()
            .sum::<u64>();
            
        let failed_requests = request_metrics
            .failed_requests
            .values()
            .sum::<u64>();
        
        let v4_requests = version_metrics.v4_metrics.total_requests;
        let v5_requests = version_metrics.v5_metrics.total_requests;
        
        let consensus_agreement_rate = if consensus_metrics.total_comparisons > 0 {
            consensus_metrics.consensus_matches as f64 / consensus_metrics.total_comparisons as f64
        } else {
            0.0
        };
        
        let mut avg_response_time = HashMap::new();
        avg_response_time.insert("v4".to_string(), version_metrics.v4_metrics.avg_response_time);
        avg_response_time.insert("v5".to_string(), version_metrics.v5_metrics.avg_response_time);
        
        let mut error_rates = HashMap::new();
        error_rates.insert("v4".to_string(), version_metrics.v4_metrics.error_rate);
        error_rates.insert("v5".to_string(), version_metrics.v5_metrics.error_rate);
        
        MigrationStats {
            total_requests,
            v4_requests,
            v5_requests,
            successful_requests,
            failed_requests,
            avg_response_time,
            error_rates,
            result_mismatches: consensus_metrics.consensus_mismatches,
            consensus_agreement_rate,
            start_time: SystemTime::UNIX_EPOCH, // Would track actual start time
            last_update: SystemTime::now(),
        }
    }
    
    /// Get detailed metrics for reporting
    pub async fn get_detailed_metrics(&self) -> DetailedMetrics {
        DetailedMetrics {
            request_metrics: self.request_metrics.read().await.clone(),
            version_metrics: self.version_metrics.read().await.clone(),
            migration_metrics: self.migration_metrics.read().await.clone(),
            consensus_metrics: self.consensus_metrics.read().await.clone(),
            performance_metrics: self.performance_metrics.read().await.clone(),
        }
    }
    
    /// Calculate percentiles from latency data
    pub fn calculate_percentiles(latencies: &[Duration]) -> LatencyPercentiles {
        if latencies.is_empty() {
            return LatencyPercentiles::default();
        }
        
        let mut sorted_latencies = latencies.to_vec();
        sorted_latencies.sort();
        
        let len = sorted_latencies.len();
        
        LatencyPercentiles {
            p50: sorted_latencies[len * 50 / 100],
            p75: sorted_latencies[len * 75 / 100],
            p90: sorted_latencies[len * 90 / 100],
            p95: sorted_latencies[len * 95 / 100],
            p99: sorted_latencies[len * 99 / 100],
            p999: sorted_latencies[len * 999 / 1000],
        }
    }
    
    /// Export metrics in Prometheus format
    #[cfg(feature = "metrics")]
    pub fn export_prometheus_metrics(&self) -> CompatResult<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.prometheus_registry.gather();
        
        encoder.encode_to_string(&metric_families)
            .map_err(|e| CompatError::Internal {
                message: format!("Failed to encode Prometheus metrics: {}", e),
            })
    }
    
    /// Create Prometheus metrics
    #[cfg(feature = "metrics")]
    fn create_prometheus_metrics(registry: &Registry) -> CompatResult<PrometheusMetrics> {
        let requests_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_requests_total", "Total number of requests"),
            registry
        )?;
        
        let request_duration = register_histogram_with_registry!(
            HistogramOpts::new("lighthouse_compat_request_duration_seconds", "Request duration in seconds"),
            registry
        )?;
        
        let v4_requests_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_v4_requests_total", "Total V4 requests"),
            registry
        )?;
        
        let v5_requests_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_v5_requests_total", "Total V5 requests"),
            registry
        )?;
        
        let v4_response_time = register_histogram_with_registry!(
            HistogramOpts::new("lighthouse_compat_v4_response_time_seconds", "V4 response time in seconds"),
            registry
        )?;
        
        let v5_response_time = register_histogram_with_registry!(
            HistogramOpts::new("lighthouse_compat_v5_response_time_seconds", "V5 response time in seconds"),
            registry
        )?;
        
        let mode_changes_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_mode_changes_total", "Total migration mode changes"),
            registry
        )?;
        
        let rollbacks_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_rollbacks_total", "Total rollbacks"),
            registry
        )?;
        
        let migration_health = register_gauge_with_registry!(
            Opts::new("lighthouse_compat_migration_health", "Migration health score"),
            registry
        )?;
        
        let consensus_matches_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_consensus_matches_total", "Total consensus matches"),
            registry
        )?;
        
        let consensus_mismatches_total = register_counter_with_registry!(
            Opts::new("lighthouse_compat_consensus_mismatches_total", "Total consensus mismatches"),
            registry
        )?;
        
        let throughput = register_gauge_with_registry!(
            Opts::new("lighthouse_compat_throughput", "Current throughput (requests/second)"),
            registry
        )?;
        
        let memory_usage = register_gauge_with_registry!(
            Opts::new("lighthouse_compat_memory_usage_mb", "Current memory usage in MB"),
            registry
        )?;
        
        let cpu_usage = register_gauge_with_registry!(
            Opts::new("lighthouse_compat_cpu_usage_percent", "Current CPU usage percentage"),
            registry
        )?;
        
        let health_score = register_gauge_with_registry!(
            Opts::new("lighthouse_compat_health_score", "Overall health score"),
            registry
        )?;
        
        Ok(PrometheusMetrics {
            requests_total,
            request_duration,
            v4_requests_total,
            v5_requests_total,
            v4_response_time,
            v5_response_time,
            mode_changes_total,
            rollbacks_total,
            migration_health,
            consensus_matches_total,
            consensus_mismatches_total,
            throughput,
            memory_usage,
            cpu_usage,
            health_score,
        })
    }
    
    /// Start background metrics collection
    pub async fn start_collection(&self) -> CompatResult<()> {
        let collector = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = collector.collect_system_metrics().await {
                    warn!("Failed to collect system metrics: {}", e);
                }
                
                collector.cleanup_old_metrics().await;
            }
        });
        
        Ok(())
    }
    
    /// Collect system-level metrics
    async fn collect_system_metrics(&self) -> CompatResult<()> {
        // This would collect actual system metrics like memory, CPU, etc.
        // For now, we'll use placeholder values
        
        let resource_usage = ResourceUsage {
            memory_mb: 512, // Would query actual memory usage
            cpu_percent: 25.0, // Would query actual CPU usage
            network_bytes_per_sec: 1024 * 1024, // 1MB/s placeholder
            disk_io_bytes_per_sec: 512 * 1024, // 512KB/s placeholder
            open_fds: 100,
            active_connections: 10,
        };
        
        let performance_metrics = PerformanceMetrics {
            throughput: 150.0, // Would calculate actual throughput
            latency_percentiles: LatencyPercentiles::default(),
            resource_usage,
            health_score: 0.95, // Would calculate actual health score
            uptime: Duration::from_secs(3600), // Would track actual uptime
            last_updated: SystemTime::now(),
        };
        
        self.update_performance_metrics(performance_metrics).await;
        
        Ok(())
    }
    
    /// Clean up old metrics data
    async fn cleanup_old_metrics(&self) {
        // Clean up latency data older than 1 hour
        let cutoff = Duration::from_secs(3600);
        let mut request_metrics = self.request_metrics.write().await;
        
        for latencies in request_metrics.request_latencies.values_mut() {
            if latencies.len() > 1000 {
                latencies.drain(0..latencies.len() - 1000);
            }
        }
    }
}

// Clone implementation for Arc usage
impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            request_metrics: Arc::clone(&self.request_metrics),
            version_metrics: Arc::clone(&self.version_metrics),
            migration_metrics: Arc::clone(&self.migration_metrics),
            consensus_metrics: Arc::clone(&self.consensus_metrics),
            performance_metrics: Arc::clone(&self.performance_metrics),
            #[cfg(feature = "metrics")]
            prometheus_registry: self.prometheus_registry.clone(),
            #[cfg(feature = "metrics")]
            prometheus_metrics: PrometheusMetrics {
                requests_total: self.prometheus_metrics.requests_total.clone(),
                request_duration: self.prometheus_metrics.request_duration.clone(),
                v4_requests_total: self.prometheus_metrics.v4_requests_total.clone(),
                v5_requests_total: self.prometheus_metrics.v5_requests_total.clone(),
                v4_response_time: self.prometheus_metrics.v4_response_time.clone(),
                v5_response_time: self.prometheus_metrics.v5_response_time.clone(),
                mode_changes_total: self.prometheus_metrics.mode_changes_total.clone(),
                rollbacks_total: self.prometheus_metrics.rollbacks_total.clone(),
                migration_health: self.prometheus_metrics.migration_health.clone(),
                consensus_matches_total: self.prometheus_metrics.consensus_matches_total.clone(),
                consensus_mismatches_total: self.prometheus_metrics.consensus_mismatches_total.clone(),
                throughput: self.prometheus_metrics.throughput.clone(),
                memory_usage: self.prometheus_metrics.memory_usage.clone(),
                cpu_usage: self.prometheus_metrics.cpu_usage.clone(),
                health_score: self.prometheus_metrics.health_score.clone(),
            },
        }
    }
}

/// Aggregated metrics for reporting
#[derive(Debug, Clone)]
pub struct DetailedMetrics {
    /// Request metrics
    pub request_metrics: RequestMetrics,
    
    /// Version metrics
    pub version_metrics: VersionMetrics,
    
    /// Migration metrics
    pub migration_metrics: MigrationMetrics,
    
    /// Consensus metrics
    pub consensus_metrics: ConsensusMetrics,
    
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
}

impl ClientMetrics {
    /// Calculate error rate
    pub fn calculate_error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.failed_requests as f64 / self.total_requests as f64
        }
    }
    
    /// Calculate success rate
    pub fn calculate_success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.successful_requests as f64 / self.total_requests as f64
        }
    }
}

impl ConsensusMetrics {
    /// Calculate consensus agreement rate
    pub fn calculate_agreement_rate(&self) -> f64 {
        if self.total_comparisons == 0 {
            0.0
        } else {
            self.consensus_matches as f64 / self.total_comparisons as f64
        }
    }
    
    /// Calculate mismatch rate
    pub fn calculate_mismatch_rate(&self) -> f64 {
        if self.total_comparisons == 0 {
            0.0
        } else {
            self.consensus_mismatches as f64 / self.total_comparisons as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Test recording a successful request
        let result: CompatResult<()> = Ok(());
        collector.record_request("test_method", &result, Duration::from_millis(50)).await;
        
        let stats = collector.get_migration_stats().await;
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.successful_requests, 1);
        assert_eq!(stats.failed_requests, 0);
    }
    
    #[tokio::test]
    async fn test_consensus_metrics() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        collector.record_consensus_match("test_method").await;
        collector.record_consensus_mismatch("test_method", "test details").await;
        
        let detailed_metrics = collector.get_detailed_metrics().await;
        let consensus = &detailed_metrics.consensus_metrics;
        
        assert_eq!(consensus.total_comparisons, 2);
        assert_eq!(consensus.consensus_matches, 1);
        assert_eq!(consensus.consensus_mismatches, 1);
        assert_eq!(consensus.calculate_agreement_rate(), 0.5);
    }
    
    #[test]
    fn test_percentile_calculation() {
        let latencies = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
            Duration::from_millis(50),
            Duration::from_millis(60),
            Duration::from_millis(70),
            Duration::from_millis(80),
            Duration::from_millis(90),
            Duration::from_millis(100),
        ];
        
        let percentiles = MetricsCollector::calculate_percentiles(&latencies);
        
        assert_eq!(percentiles.p50, Duration::from_millis(50));
        assert_eq!(percentiles.p95, Duration::from_millis(90));
        assert_eq!(percentiles.p99, Duration::from_millis(90)); // Small dataset
    }
    
    #[tokio::test]
    async fn test_version_metrics() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        let v4_metrics = ClientMetrics {
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            avg_response_time: Duration::from_millis(50),
            error_rate: 0.05,
            ..Default::default()
        };
        
        collector.update_version_metrics("v4", v4_metrics.clone()).await;
        
        let detailed_metrics = collector.get_detailed_metrics().await;
        assert_eq!(detailed_metrics.version_metrics.v4_metrics.total_requests, 100);
        assert_eq!(detailed_metrics.version_metrics.v4_metrics.calculate_error_rate(), 0.05);
        assert_eq!(detailed_metrics.version_metrics.v4_metrics.calculate_success_rate(), 0.95);
    }
    
    #[tokio::test]
    async fn test_migration_metrics() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        collector.record_mode_change().await;
        collector.record_rollback("test reason").await;
        
        let detailed_metrics = collector.get_detailed_metrics().await;
        let migration = &detailed_metrics.migration_metrics;
        
        assert_eq!(migration.mode_changes, 1);
        assert_eq!(migration.rollbacks, 1);
        assert_eq!(migration.rollback_reasons.get("test reason"), Some(&1));
    }
}