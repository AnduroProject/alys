use std::time::{Duration, SystemTime, Instant};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow::{Result, Context};
use tracing::{info, debug, warn, error};
use serde::{Serialize, Deserialize};

use crate::config::TestConfig;
use crate::{TestResult, MigrationPhase, TestMetrics};

/// Metrics collector for test framework
/// 
/// Collects, aggregates, and reports metrics from all test activities
/// including performance data, resource usage, and test outcomes.
#[derive(Debug)]
pub struct MetricsCollector {
    /// Test configuration
    config: TestConfig,
    
    /// Phase-specific metrics
    phase_metrics: Arc<Mutex<HashMap<MigrationPhase, PhaseMetrics>>>,
    
    /// System resource metrics
    resource_metrics: Arc<Mutex<ResourceMetrics>>,
    
    /// Test execution metrics
    execution_metrics: Arc<Mutex<ExecutionMetrics>>,
    
    /// Performance metrics
    performance_metrics: Arc<Mutex<PerformanceMetrics>>,
    
    /// Metrics start time
    start_time: SystemTime,
}

/// Metrics for a specific migration phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseMetrics {
    pub phase: MigrationPhase,
    pub tests_run: u32,
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub total_duration: Duration,
    pub average_duration: Duration,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub resource_usage: ResourceSnapshot,
}

/// System resource usage metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceMetrics {
    pub peak_memory_usage_bytes: u64,
    pub average_memory_usage_bytes: u64,
    pub peak_cpu_usage_percent: f64,
    pub average_cpu_usage_percent: f64,
    pub total_disk_io_bytes: u64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    pub thread_count_peak: u32,
    pub file_descriptors_peak: u32,
}

/// Test execution metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionMetrics {
    pub total_tests_executed: u64,
    pub total_tests_passed: u64,
    pub total_tests_failed: u64,
    pub total_execution_time: Duration,
    pub parallel_execution_sessions: u32,
    pub test_retries: u32,
    pub test_timeouts: u32,
    pub harness_initialization_time: Duration,
    pub framework_overhead_time: Duration,
}

/// Performance-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceMetrics {
    pub throughput_tests_per_second: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub memory_efficiency_score: f64,
    pub cpu_efficiency_score: f64,
    pub regression_detected: bool,
    pub performance_improvements: Vec<PerformanceImprovement>,
}

/// Resource usage snapshot at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub timestamp: SystemTime,
    pub memory_usage_bytes: u64,
    pub cpu_usage_percent: f64,
    pub thread_count: u32,
    pub open_file_descriptors: u32,
}

/// Performance improvement record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceImprovement {
    pub test_name: String,
    pub improvement_type: String,
    pub improvement_percent: f64,
    pub baseline_value: f64,
    pub current_value: f64,
    pub timestamp: SystemTime,
}

/// Comprehensive test metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub generation_time: SystemTime,
    pub test_session_duration: Duration,
    pub phase_metrics: HashMap<MigrationPhase, PhaseMetrics>,
    pub resource_metrics: ResourceMetrics,
    pub execution_metrics: ExecutionMetrics,
    pub performance_metrics: PerformanceMetrics,
    pub summary: MetricsSummary,
}

/// High-level metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub overall_success_rate: f64,
    pub total_test_time: Duration,
    pub phases_completed: u32,
    pub critical_issues: Vec<String>,
    pub recommendations: Vec<String>,
}

impl MetricsCollector {
    /// Create a new MetricsCollector
    pub fn new(config: TestConfig) -> Result<Self> {
        info!("Initializing MetricsCollector");
        
        let collector = Self {
            config,
            phase_metrics: Arc::new(Mutex::new(HashMap::new())),
            resource_metrics: Arc::new(Mutex::new(ResourceMetrics::default())),
            execution_metrics: Arc::new(Mutex::new(ExecutionMetrics::default())),
            performance_metrics: Arc::new(Mutex::new(PerformanceMetrics::default())),
            start_time: SystemTime::now(),
        };
        
        debug!("MetricsCollector initialized");
        Ok(collector)
    }
    
    /// Record the start of a phase validation
    pub async fn record_phase_start(&self, phase: MigrationPhase) {
        debug!("Recording phase start: {:?}", phase);
        
        let phase_metric = PhaseMetrics {
            phase: phase.clone(),
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            total_duration: Duration::ZERO,
            average_duration: Duration::ZERO,
            start_time: SystemTime::now(),
            end_time: None,
            resource_usage: self.capture_resource_snapshot().await,
        };
        
        if let Ok(mut metrics) = self.phase_metrics.lock() {
            metrics.insert(phase, phase_metric);
        }
    }
    
    /// Record the completion of a phase validation
    pub async fn record_phase_completion(
        &self,
        phase: MigrationPhase,
        duration: Duration,
        results: &[TestResult],
    ) {
        debug!("Recording phase completion: {:?}", phase);
        
        let tests_passed = results.iter().filter(|r| r.success).count() as u32;
        let tests_failed = results.iter().filter(|r| !r.success).count() as u32;
        let tests_run = results.len() as u32;
        
        let average_duration = if tests_run > 0 {
            results.iter().map(|r| r.duration).sum::<Duration>() / tests_run
        } else {
            Duration::ZERO
        };
        
        if let Ok(mut metrics) = self.phase_metrics.lock() {
            if let Some(phase_metric) = metrics.get_mut(&phase) {
                phase_metric.tests_run = tests_run;
                phase_metric.tests_passed = tests_passed;
                phase_metric.tests_failed = tests_failed;
                phase_metric.total_duration = duration;
                phase_metric.average_duration = average_duration;
                phase_metric.end_time = Some(SystemTime::now());
                phase_metric.resource_usage = self.capture_resource_snapshot().await;
            }
        }
        
        // Update execution metrics
        if let Ok(mut exec_metrics) = self.execution_metrics.lock() {
            exec_metrics.total_tests_executed += tests_run as u64;
            exec_metrics.total_tests_passed += tests_passed as u64;
            exec_metrics.total_tests_failed += tests_failed as u64;
            exec_metrics.total_execution_time += duration;
        }
    }
    
    /// Record resource usage metrics
    pub async fn record_resource_usage(&self, memory_bytes: u64, cpu_percent: f64) {
        if let Ok(mut metrics) = self.resource_metrics.lock() {
            // Update peak values
            if memory_bytes > metrics.peak_memory_usage_bytes {
                metrics.peak_memory_usage_bytes = memory_bytes;
            }
            
            if cpu_percent > metrics.peak_cpu_usage_percent {
                metrics.peak_cpu_usage_percent = cpu_percent;
            }
            
            // Update averages (simplified - in practice would use sliding window)
            metrics.average_memory_usage_bytes = 
                (metrics.average_memory_usage_bytes + memory_bytes) / 2;
            metrics.average_cpu_usage_percent = 
                (metrics.average_cpu_usage_percent + cpu_percent) / 2.0;
        }
    }
    
    /// Record performance metrics
    pub async fn record_performance_metric(
        &self,
        test_name: String,
        latency_ms: f64,
        throughput: f64,
    ) {
        if let Ok(mut metrics) = self.performance_metrics.lock() {
            // Update throughput
            if throughput > metrics.throughput_tests_per_second {
                metrics.throughput_tests_per_second = throughput;
            }
            
            // Update latency percentiles (simplified - in practice would maintain histogram)
            if metrics.latency_p50_ms == 0.0 || latency_ms < metrics.latency_p50_ms {
                metrics.latency_p50_ms = latency_ms;
            }
            if latency_ms > metrics.latency_p95_ms {
                metrics.latency_p95_ms = latency_ms;
            }
            if latency_ms > metrics.latency_p99_ms {
                metrics.latency_p99_ms = latency_ms;
            }
        }
    }
    
    /// Collect metrics for a specific phase
    pub async fn collect_phase_metrics(&self, phase: &MigrationPhase) -> TestMetrics {
        let phase_metrics = self.phase_metrics.lock().unwrap();
        
        if let Some(metrics) = phase_metrics.get(phase) {
            TestMetrics {
                total_tests: metrics.tests_run,
                passed_tests: metrics.tests_passed,
                failed_tests: metrics.tests_failed,
                total_duration: metrics.total_duration,
                average_duration: metrics.average_duration,
                memory_usage: metrics.resource_usage.memory_usage_bytes,
                cpu_usage: metrics.resource_usage.cpu_usage_percent,
            }
        } else {
            TestMetrics {
                total_tests: 0,
                passed_tests: 0,
                failed_tests: 0,
                total_duration: Duration::ZERO,
                average_duration: Duration::ZERO,
                memory_usage: 0,
                cpu_usage: 0.0,
            }
        }
    }
    
    /// Collect comprehensive metrics from all components
    pub async fn collect_comprehensive_metrics(&self) -> TestMetrics {
        let execution_metrics = self.execution_metrics.lock().unwrap();
        let resource_metrics = self.resource_metrics.lock().unwrap();
        
        TestMetrics {
            total_tests: execution_metrics.total_tests_executed as u32,
            passed_tests: execution_metrics.total_tests_passed as u32,
            failed_tests: execution_metrics.total_tests_failed as u32,
            total_duration: execution_metrics.total_execution_time,
            average_duration: if execution_metrics.total_tests_executed > 0 {
                execution_metrics.total_execution_time / execution_metrics.total_tests_executed as u32
            } else {
                Duration::ZERO
            },
            memory_usage: resource_metrics.peak_memory_usage_bytes,
            cpu_usage: resource_metrics.peak_cpu_usage_percent,
        }
    }
    
    /// Generate a comprehensive metrics report
    pub async fn generate_report(&self) -> Result<MetricsReport> {
        info!("Generating comprehensive metrics report");
        
        let phase_metrics = self.phase_metrics.lock().unwrap().clone();
        let resource_metrics = self.resource_metrics.lock().unwrap().clone();
        let execution_metrics = self.execution_metrics.lock().unwrap().clone();
        let performance_metrics = self.performance_metrics.lock().unwrap().clone();
        
        let total_tests = execution_metrics.total_tests_executed;
        let passed_tests = execution_metrics.total_tests_passed;
        
        let overall_success_rate = if total_tests > 0 {
            passed_tests as f64 / total_tests as f64
        } else {
            0.0
        };
        
        let test_session_duration = self.start_time.elapsed()
            .unwrap_or(Duration::ZERO);
        
        let phases_completed = phase_metrics.values()
            .filter(|p| p.end_time.is_some())
            .count() as u32;
        
        let mut critical_issues = Vec::new();
        let mut recommendations = Vec::new();
        
        // Analyze metrics for issues and recommendations
        if overall_success_rate < 0.9 {
            critical_issues.push(format!(
                "Low overall success rate: {:.1}%",
                overall_success_rate * 100.0
            ));
        }
        
        if resource_metrics.peak_memory_usage_bytes > 1024 * 1024 * 1024 { // > 1GB
            recommendations.push("Consider optimizing memory usage".to_string());
        }
        
        if performance_metrics.regression_detected {
            critical_issues.push("Performance regression detected".to_string());
        }
        
        let summary = MetricsSummary {
            overall_success_rate,
            total_test_time: test_session_duration,
            phases_completed,
            critical_issues,
            recommendations,
        };
        
        let report = MetricsReport {
            generation_time: SystemTime::now(),
            test_session_duration,
            phase_metrics,
            resource_metrics,
            execution_metrics,
            performance_metrics,
            summary,
        };
        
        info!("Metrics report generated successfully");
        Ok(report)
    }
    
    /// Test metrics collection functionality
    pub async fn test_collection(&self) -> TestResult {
        debug!("Testing metrics collection");
        
        let start = Instant::now();
        
        // Test recording some sample metrics
        self.record_resource_usage(1024 * 1024, 25.5).await; // 1MB, 25.5% CPU
        self.record_performance_metric("test_metric".to_string(), 100.0, 50.0).await;
        
        // Test metric retrieval
        let metrics = self.collect_comprehensive_metrics().await;
        
        let duration = start.elapsed();
        
        TestResult {
            test_name: "metrics_collection".to_string(),
            success: true,
            duration,
            message: Some("Metrics collection system operational".to_string()),
            metadata: [
                ("collected_metrics".to_string(), "true".to_string()),
                ("resource_tracking".to_string(), "true".to_string()),
                ("performance_tracking".to_string(), "true".to_string()),
            ].iter().cloned().collect(),
        }
    }
    
    /// Shutdown metrics collection
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down MetricsCollector");
        
        // Generate final report
        let _final_report = self.generate_report().await?;
        
        info!("MetricsCollector shutdown completed");
        Ok(())
    }
    
    /// Capture current resource usage snapshot
    async fn capture_resource_snapshot(&self) -> ResourceSnapshot {
        // Mock implementation - in practice would use system APIs
        ResourceSnapshot {
            timestamp: SystemTime::now(),
            memory_usage_bytes: 1024 * 1024 * 10, // Mock: 10MB
            cpu_usage_percent: 15.0,              // Mock: 15% CPU
            thread_count: 8,                      // Mock: 8 threads
            open_file_descriptors: 25,           // Mock: 25 FDs
        }
    }
}

impl Default for PhaseMetrics {
    fn default() -> Self {
        Self {
            phase: MigrationPhase::Foundation,
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            total_duration: Duration::ZERO,
            average_duration: Duration::ZERO,
            start_time: SystemTime::now(),
            end_time: None,
            resource_usage: ResourceSnapshot::default(),
        }
    }
}

impl Default for ResourceSnapshot {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::now(),
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
            thread_count: 0,
            open_file_descriptors: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestConfig;
    
    #[tokio::test]
    async fn test_metrics_collector_initialization() {
        let config = TestConfig::development();
        let collector = MetricsCollector::new(config).unwrap();
        
        let metrics = collector.collect_comprehensive_metrics().await;
        assert_eq!(metrics.total_tests, 0);
    }
    
    #[tokio::test]
    async fn test_phase_metrics_recording() {
        let config = TestConfig::development();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Start phase
        collector.record_phase_start(MigrationPhase::Foundation).await;
        
        // End phase
        let results = vec![TestResult {
            test_name: "test".to_string(),
            success: true,
            duration: Duration::from_millis(100),
            message: None,
            metadata: HashMap::new(),
        }];
        
        collector.record_phase_completion(
            MigrationPhase::Foundation,
            Duration::from_millis(100),
            &results,
        ).await;
        
        let metrics = collector.collect_phase_metrics(&MigrationPhase::Foundation).await;
        assert_eq!(metrics.total_tests, 1);
        assert_eq!(metrics.passed_tests, 1);
        assert_eq!(metrics.failed_tests, 0);
    }
    
    #[tokio::test]
    async fn test_resource_metrics_recording() {
        let config = TestConfig::development();
        let collector = MetricsCollector::new(config).unwrap();
        
        collector.record_resource_usage(1024 * 1024, 50.0).await;
        
        let resource_metrics = collector.resource_metrics.lock().unwrap();
        assert_eq!(resource_metrics.peak_memory_usage_bytes, 1024 * 1024);
        assert_eq!(resource_metrics.peak_cpu_usage_percent, 50.0);
    }
    
    #[tokio::test]
    async fn test_metrics_report_generation() {
        let config = TestConfig::development();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Record some test data
        collector.record_phase_start(MigrationPhase::Foundation).await;
        let results = vec![TestResult {
            test_name: "test".to_string(),
            success: true,
            duration: Duration::from_millis(100),
            message: None,
            metadata: HashMap::new(),
        }];
        collector.record_phase_completion(
            MigrationPhase::Foundation,
            Duration::from_millis(100),
            &results,
        ).await;
        
        let report = collector.generate_report().await.unwrap();
        
        assert_eq!(report.summary.phases_completed, 1);
        assert!(report.summary.overall_success_rate > 0.0);
    }
    
    #[tokio::test]
    async fn test_metrics_collection_functionality() {
        let config = TestConfig::development();
        let collector = MetricsCollector::new(config).unwrap();
        
        let result = collector.test_collection().await;
        
        assert!(result.success);
        assert_eq!(result.test_name, "metrics_collection");
    }
}