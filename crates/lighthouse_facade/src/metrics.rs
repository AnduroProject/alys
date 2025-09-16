//! Metrics collection for Lighthouse facade
//!
//! This module provides comprehensive metrics collection and reporting for
//! Lighthouse operations, version comparison, and facade performance.

use crate::{
    error::{FacadeError, FacadeResult},
    types::ClientVersion,
    config::MetricsConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Metrics collector for facade operations
#[derive(Debug)]
pub struct MetricsCollector {
    /// Configuration
    config: MetricsConfig,
    
    /// Request metrics
    request_metrics: Arc<RwLock<RequestMetrics>>,
    
    /// Version comparison metrics
    comparison_metrics: Arc<RwLock<ComparisonMetrics>>,
    
    /// Performance metrics
    performance_metrics: Arc<RwLock<PerformanceMetrics>>,
    
    /// Error metrics
    error_metrics: Arc<RwLock<ErrorMetrics>>,
    
    /// Migration metrics
    migration_metrics: Arc<RwLock<MigrationMetrics>>,
}

/// Request-level metrics
#[derive(Debug, Clone)]
pub struct RequestMetrics {
    /// Total requests processed
    pub total_requests: u64,
    
    /// Successful requests
    pub successful_requests: u64,
    
    /// Failed requests
    pub failed_requests: u64,
    
    /// Request counts by method
    pub requests_by_method: HashMap<String, u64>,
    
    /// Response times by method (milliseconds)
    pub response_times: HashMap<String, Vec<f64>>,
    
    /// Request counts by version
    pub requests_by_version: HashMap<String, u64>,
    
    /// Average response time
    pub avg_response_time: Duration,
    
    /// Peak response time
    pub peak_response_time: Duration,
    
    /// Requests per second (sliding window)
    pub requests_per_second: f64,
    
    /// Last request timestamp
    pub last_request: SystemTime,
}

/// Version comparison metrics
#[derive(Debug, Clone, Default)]
pub struct ComparisonMetrics {
    /// Parallel execution attempts
    pub parallel_executions: u64,
    
    /// Consensus matches between versions
    pub consensus_matches: u64,
    
    /// Consensus mismatches
    pub consensus_mismatches: u64,
    
    /// V4-only successes
    pub v4_only_successes: u64,
    
    /// V7-only successes
    pub v7_only_successes: u64,
    
    /// Both versions failed
    pub both_failed: u64,
    
    /// Shadow execution metrics (V4 primary, V7 shadow)
    pub shadow_successes: u64,
    pub shadow_failures: u64,
    
    /// Fallback metrics (V7 primary, V4 fallback)
    pub fallback_activations: u64,
    pub fallback_successes: u64,
    pub fallback_failures: u64,
    
    /// Response time differences
    pub response_time_differences: Vec<i64>, // V7 - V4 in milliseconds
    
    /// Mismatch details
    pub mismatch_details: HashMap<String, u64>,
}

/// Performance metrics
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// CPU usage percentage
    pub cpu_usage: f64,
    
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    
    /// Network I/O metrics
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    
    /// Connection pool metrics
    pub active_connections: u32,
    pub connection_pool_size: u32,
    
    /// Cache metrics
    pub cache_hits: u64,
    pub cache_misses: u64,
    
    /// Garbage collection metrics (if applicable)
    pub gc_count: u64,
    pub gc_time_ms: u64,
}

/// Error metrics
#[derive(Debug, Clone, Default)]
pub struct ErrorMetrics {
    /// Total errors
    pub total_errors: u64,
    
    /// Errors by type
    pub errors_by_type: HashMap<String, u64>,
    
    /// Errors by version
    pub v4_errors: u64,
    pub v7_errors: u64,
    
    /// Error rates (errors per request)
    pub error_rate: f64,
    
    /// Error details
    pub error_details: Vec<ErrorDetails>,
    
    /// Recovery metrics
    pub recoveries: u64,
    pub recovery_time_ms: Vec<u64>,
}

/// Migration metrics
#[derive(Debug, Clone, Default)]
pub struct MigrationMetrics {
    /// Mode changes
    pub mode_changes: u64,
    
    /// Time in each mode (seconds)
    pub time_in_modes: HashMap<String, u64>,
    
    /// Migration success rate
    pub migration_success_rate: f64,
    
    /// Traffic split ratios
    pub traffic_splits: HashMap<String, f64>,
    
    /// Canary deployment metrics
    pub canary_percentage: u8,
    pub canary_successes: u64,
    pub canary_failures: u64,
    
    /// A/B test metrics
    pub ab_test_results: HashMap<String, ABTestMetrics>,
}

/// Error details for analysis
#[derive(Debug, Clone)]
pub struct ErrorDetails {
    /// Timestamp
    pub timestamp: SystemTime,
    
    /// Error type
    pub error_type: String,
    
    /// Error message
    pub message: String,
    
    /// Client version involved
    pub version: Option<ClientVersion>,
    
    /// Request method
    pub method: String,
    
    /// Recovery attempted
    pub recovery_attempted: bool,
    
    /// Recovery successful
    pub recovery_successful: bool,
}

/// A/B test metrics
#[derive(Debug, Clone, Default)]
pub struct ABTestMetrics {
    /// Test name
    pub test_name: String,
    
    /// V7 percentage
    pub v7_percentage: u8,
    
    /// Total requests in test
    pub total_requests: u64,
    
    /// V4 requests
    pub v4_requests: u64,
    
    /// V7 requests
    pub v7_requests: u64,
    
    /// V4 success rate
    pub v4_success_rate: f64,
    
    /// V7 success rate
    pub v7_success_rate: f64,
    
    /// Statistical significance
    pub statistically_significant: bool,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> FacadeResult<Self> {
        info!("Initializing metrics collector with config: {:?}", config);
        
        Ok(Self {
            config,
            request_metrics: Arc::new(RwLock::new(RequestMetrics::default())),
            comparison_metrics: Arc::new(RwLock::new(ComparisonMetrics::default())),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            error_metrics: Arc::new(RwLock::new(ErrorMetrics::default())),
            migration_metrics: Arc::new(RwLock::new(MigrationMetrics::default())),
        })
    }
    
    /// Record a request
    pub async fn record_request<T>(&self, method: &str, result: &Result<T, FacadeError>, duration: Duration) {
        let mut metrics = self.request_metrics.write().await;
        
        metrics.total_requests += 1;
        metrics.last_request = SystemTime::now();
        
        // Update method-specific metrics
        *metrics.requests_by_method.entry(method.to_string()).or_insert(0) += 1;
        
        let duration_ms = duration.as_millis() as f64;
        metrics.response_times.entry(method.to_string())
            .or_insert_with(Vec::new)
            .push(duration_ms);
        
        // Update overall timing metrics
        if duration > metrics.peak_response_time {
            metrics.peak_response_time = duration;
        }
        
        // Calculate rolling average (simple implementation)
        let total_duration_ms: f64 = metrics.response_times.values()
            .flat_map(|times| times.iter())
            .sum();
        let total_requests = metrics.response_times.values()
            .map(|times| times.len())
            .sum::<usize>() as f64;
        
        if total_requests > 0.0 {
            metrics.avg_response_time = Duration::from_millis((total_duration_ms / total_requests) as u64);
        }
        
        match result {
            Ok(_) => {
                metrics.successful_requests += 1;
            }
            Err(error) => {
                metrics.failed_requests += 1;
                drop(metrics);
                
                // Record error details
                self.record_error(method, error, None).await;
            }
        }
        
        debug!("Request recorded: method={}, duration={:?}, success={}", 
               method, duration, result.is_ok());
    }
    
    /// Record an error
    pub async fn record_error(&self, method: &str, error: &FacadeError, version: Option<ClientVersion>) {
        let mut metrics = self.error_metrics.write().await;
        
        metrics.total_errors += 1;
        
        let error_type = error.error_type();
        *metrics.errors_by_type.entry(error_type.clone()).or_insert(0) += 1;
        
        // Update version-specific error counts
        match &version {
            Some(ClientVersion::V4 { .. }) => metrics.v4_errors += 1,
            Some(ClientVersion::V7 { .. }) => metrics.v7_errors += 1,
            Some(ClientVersion::Mock { .. }) => {}, // Mock version - no specific tracking
            None => {}, // Unknown version
        }
        
        // Store error details
        let error_details = ErrorDetails {
            timestamp: SystemTime::now(),
            error_type,
            message: error.to_string(),
            version: version.clone(),
            method: method.to_string(),
            recovery_attempted: false,
            recovery_successful: false,
        };
        
        metrics.error_details.push(error_details);
        
        // Keep only recent error details (last 1000)
        if metrics.error_details.len() > 1000 {
            metrics.error_details.drain(0..100);
        }
        
        // Recalculate error rate
        let request_metrics = self.request_metrics.read().await;
        if request_metrics.total_requests > 0 {
            metrics.error_rate = metrics.total_errors as f64 / request_metrics.total_requests as f64;
        }
        
        debug!("Error recorded: method={}, error_type={}, version={:?}", 
               method, error.error_type(), version);
    }
    
    /// Record consensus match between versions
    pub async fn record_consensus_match(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.parallel_executions += 1;
        metrics.consensus_matches += 1;
        
        debug!("Consensus match recorded for method: {}", method);
    }
    
    /// Record consensus mismatch
    pub async fn record_consensus_mismatch(&self, method: &str, details: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.parallel_executions += 1;
        metrics.consensus_mismatches += 1;
        
        *metrics.mismatch_details.entry(details.to_string()).or_insert(0) += 1;
        
        debug!("Consensus mismatch recorded for method: {} - {}", method, details);
    }
    
    /// Record V4-only error
    pub async fn record_v4_only_error(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.v7_only_successes += 1;
        
        debug!("V4-only error recorded for method: {}", method);
    }
    
    /// Record V7-only error
    pub async fn record_v7_only_error(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.v4_only_successes += 1;
        
        debug!("V7-only error recorded for method: {}", method);
    }
    
    /// Record both versions failed
    pub async fn record_both_errors(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.both_failed += 1;
        
        debug!("Both versions failed for method: {}", method);
    }
    
    /// Record shadow execution success
    pub async fn record_shadow_success(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.shadow_successes += 1;
        
        debug!("Shadow execution success recorded for method: {}", method);
    }
    
    /// Record shadow execution error
    pub async fn record_shadow_error(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.shadow_failures += 1;
        
        debug!("Shadow execution error recorded for method: {}", method);
    }
    
    /// Record fallback activation
    pub async fn record_fallback(&self, method: &str) {
        let mut metrics = self.comparison_metrics.write().await;
        metrics.fallback_activations += 1;
        
        debug!("Fallback activation recorded for method: {}", method);
    }
    
    /// Record migration mode change
    pub async fn record_mode_change(&self) {
        let mut metrics = self.migration_metrics.write().await;
        metrics.mode_changes += 1;
        
        debug!("Migration mode change recorded");
    }
    
    /// Get current metrics snapshot
    pub async fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        let request_metrics = self.request_metrics.read().await.clone();
        let comparison_metrics = self.comparison_metrics.read().await.clone();
        let performance_metrics = self.performance_metrics.read().await.clone();
        let error_metrics = self.error_metrics.read().await.clone();
        let migration_metrics = self.migration_metrics.read().await.clone();
        
        MetricsSnapshot {
            timestamp: SystemTime::now(),
            requests: request_metrics,
            comparisons: comparison_metrics,
            performance: performance_metrics,
            errors: error_metrics,
            migrations: migration_metrics,
        }
    }
    
    /// Update performance metrics
    pub async fn update_performance_metrics(&self, cpu_usage: f64, memory_usage_mb: u64) {
        let mut metrics = self.performance_metrics.write().await;
        metrics.cpu_usage = cpu_usage;
        metrics.memory_usage_mb = memory_usage_mb;
        
        debug!("Performance metrics updated: CPU={:.1}%, Memory={}MB", cpu_usage, memory_usage_mb);
    }
    
    /// Export metrics for external monitoring systems
    pub async fn export_prometheus_metrics(&self) -> String {
        if !self.config.prometheus.enabled {
            return String::new();
        }
        
        let snapshot = self.get_metrics_snapshot().await;
        let mut output = String::new();
        
        // Request metrics
        output.push_str(&format!("lighthouse_facade_total_requests {}\n", snapshot.requests.total_requests));
        output.push_str(&format!("lighthouse_facade_successful_requests {}\n", snapshot.requests.successful_requests));
        output.push_str(&format!("lighthouse_facade_failed_requests {}\n", snapshot.requests.failed_requests));
        output.push_str(&format!("lighthouse_facade_avg_response_time_ms {}\n", snapshot.requests.avg_response_time.as_millis()));
        
        // Comparison metrics
        output.push_str(&format!("lighthouse_facade_consensus_matches {}\n", snapshot.comparisons.consensus_matches));
        output.push_str(&format!("lighthouse_facade_consensus_mismatches {}\n", snapshot.comparisons.consensus_mismatches));
        
        // Error metrics
        output.push_str(&format!("lighthouse_facade_total_errors {}\n", snapshot.errors.total_errors));
        output.push_str(&format!("lighthouse_facade_error_rate {}\n", snapshot.errors.error_rate));
        
        output
    }
}

/// Complete metrics snapshot
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Timestamp of snapshot
    pub timestamp: SystemTime,
    
    /// Request metrics
    pub requests: RequestMetrics,
    
    /// Comparison metrics
    pub comparisons: ComparisonMetrics,
    
    /// Performance metrics
    pub performance: PerformanceMetrics,
    
    /// Error metrics
    pub errors: ErrorMetrics,
    
    /// Migration metrics
    pub migrations: MigrationMetrics,
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            requests_by_method: HashMap::new(),
            response_times: HashMap::new(),
            requests_by_version: HashMap::new(),
            avg_response_time: Duration::from_millis(0),
            peak_response_time: Duration::from_millis(0),
            requests_per_second: 0.0,
            last_request: SystemTime::now(),
        }
    }
}

impl FacadeError {
    /// Get error type for metrics categorization
    pub fn error_type(&self) -> String {
        match self {
            FacadeError::Initialization { .. } => "initialization".to_string(),
            FacadeError::InvalidConfiguration { .. } => "invalid_configuration".to_string(),
            FacadeError::ServiceUnavailable { .. } => "service_unavailable".to_string(),
            FacadeError::EngineApi { .. } => "engine_api".to_string(),
            FacadeError::Conversion { .. } => "conversion".to_string(),
            FacadeError::Migration { .. } => "migration".to_string(),
            FacadeError::Internal { .. } => "internal".to_string(),
            FacadeError::Connection { .. } => "connection".to_string(),
            FacadeError::Api { .. } => "api".to_string(),
            FacadeError::Timeout { .. } => "timeout".to_string(),
            FacadeError::Configuration { .. } => "configuration".to_string(),
            FacadeError::TypeConversion { .. } => "type_conversion".to_string(),
            FacadeError::IncompatibleFeature { .. } => "incompatible_feature".to_string(),
            FacadeError::ValidationError { .. } => "validation".to_string(),
            FacadeError::Compatibility(_) => "compatibility".to_string(),
        }
    }
}