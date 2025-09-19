use async_trait::async_trait;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tokio::time::interval;
use tracing::{info, warn, error, debug};
use super::super::base::SystemHealthReport;

/// System monitoring trait for chaos testing
#[async_trait]
pub trait SystemMonitor: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start monitoring system health
    async fn start_monitoring(&mut self) -> Result<(), Self::Error>;

    /// Stop monitoring and return final report
    async fn stop_monitoring(&mut self) -> Result<MonitoringSummary, Self::Error>;

    /// Get current system health
    async fn get_health(&self) -> Result<SystemHealthReport, Self::Error>;

    /// Check if system is responsive
    async fn is_system_responsive(&self) -> Result<bool, Self::Error>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSummary {
    pub duration: Duration,
    pub total_checks: u64,
    pub successful_checks: u64,
    pub failed_checks: u64,
    pub average_response_time: Duration,
    pub max_response_time: Duration,
    pub min_response_time: Duration,
    pub health_snapshots: Vec<SystemHealthReport>,
    pub anomalies_detected: Vec<HealthAnomaly>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAnomaly {
    pub timestamp: std::time::SystemTime,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub description: String,
    pub metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyType {
    HighMemoryUsage,
    SlowResponseTime,
    HighErrorRate,
    SystemUnresponsive,
    ResourceExhaustion,
    UnexpectedBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Comprehensive system monitor for storage actors
pub struct StorageActorMonitor {
    monitoring_active: bool,
    start_time: Option<Instant>,
    check_interval: Duration,
    health_snapshots: Vec<SystemHealthReport>,
    response_times: Vec<Duration>,
    anomalies: Vec<HealthAnomaly>,
    thresholds: MonitoringThresholds,
    stats: MonitoringStats,
}

#[derive(Debug, Clone)]
pub struct MonitoringThresholds {
    pub max_response_time: Duration,
    pub max_memory_usage: u64,
    pub max_error_rate: f64,
    pub min_success_rate: f64,
}

impl Default for MonitoringThresholds {
    fn default() -> Self {
        Self {
            max_response_time: Duration::from_secs(5),
            max_memory_usage: 1024 * 1024 * 1024, // 1GB
            max_error_rate: 0.1, // 10%
            min_success_rate: 0.9, // 90%
        }
    }
}

#[derive(Debug, Clone, Default)]
struct MonitoringStats {
    total_checks: u64,
    successful_checks: u64,
    failed_checks: u64,
}

impl StorageActorMonitor {
    pub fn new() -> Self {
        Self {
            monitoring_active: false,
            start_time: None,
            check_interval: Duration::from_secs(1),
            health_snapshots: Vec::new(),
            response_times: Vec::new(),
            anomalies: Vec::new(),
            thresholds: MonitoringThresholds::default(),
            stats: MonitoringStats::default(),
        }
    }

    pub fn with_thresholds(mut self, thresholds: MonitoringThresholds) -> Self {
        self.thresholds = thresholds;
        self
    }

    pub fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Perform a health check and analyze results
    async fn perform_health_check(&mut self) -> Result<SystemHealthReport, MonitoringError> {
        let start = Instant::now();
        self.stats.total_checks += 1;

        // Simulate health check (in real implementation, this would check actual system)
        let health_report = self.simulate_health_check().await;
        let response_time = start.elapsed();
        self.response_times.push(response_time);

        match health_report {
            Ok(report) => {
                self.stats.successful_checks += 1;
                self.health_snapshots.push(report.clone());

                // Analyze for anomalies
                self.detect_anomalies(&report, response_time).await;

                Ok(report)
            },
            Err(e) => {
                self.stats.failed_checks += 1;
                error!("Health check failed: {:?}", e);
                Err(e)
            }
        }
    }

    /// Simulate a health check (replace with actual system checks)
    async fn simulate_health_check(&self) -> Result<SystemHealthReport, MonitoringError> {
        // Simulate some variability
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let memory_usage = rng.gen_range(100_000_000..500_000_000); // 100MB to 500MB
        let error_count = rng.gen_range(0..5);
        let responsive = rng.gen_bool(0.95); // 95% chance of being responsive

        let mut custom_metrics = HashMap::new();
        custom_metrics.insert("cpu_usage".to_string(), rng.gen_range(0.1..0.9));
        custom_metrics.insert("disk_usage".to_string(), rng.gen_range(0.2..0.8));
        custom_metrics.insert("network_latency".to_string(), rng.gen_range(1.0..100.0));

        Ok(SystemHealthReport {
            timestamp: std::time::SystemTime::now(),
            actor_responsive: responsive,
            memory_usage,
            active_connections: rng.gen_range(1..10),
            error_count,
            custom_metrics,
        })
    }

    /// Detect anomalies in system health
    async fn detect_anomalies(&mut self, report: &SystemHealthReport, response_time: Duration) {
        let now = std::time::SystemTime::now();

        // Check response time anomaly
        if response_time > self.thresholds.max_response_time {
            let anomaly = HealthAnomaly {
                timestamp: now,
                anomaly_type: AnomalyType::SlowResponseTime,
                severity: if response_time > self.thresholds.max_response_time * 2 {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                },
                description: format!("Response time {}ms exceeds threshold {}ms",
                                   response_time.as_millis(),
                                   self.thresholds.max_response_time.as_millis()),
                metrics: {
                    let mut metrics = HashMap::new();
                    metrics.insert("response_time_ms".to_string(), response_time.as_millis() as f64);
                    metrics.insert("threshold_ms".to_string(), self.thresholds.max_response_time.as_millis() as f64);
                    metrics
                },
            };
            warn!("Detected anomaly: {:?}", anomaly);
            self.anomalies.push(anomaly);
        }

        // Check memory usage anomaly
        if report.memory_usage > self.thresholds.max_memory_usage {
            let anomaly = HealthAnomaly {
                timestamp: now,
                anomaly_type: AnomalyType::HighMemoryUsage,
                severity: if report.memory_usage > self.thresholds.max_memory_usage * 2 {
                    AnomalySeverity::Critical
                } else {
                    AnomalySeverity::High
                },
                description: format!("Memory usage {}MB exceeds threshold {}MB",
                                   report.memory_usage / (1024 * 1024),
                                   self.thresholds.max_memory_usage / (1024 * 1024)),
                metrics: {
                    let mut metrics = HashMap::new();
                    metrics.insert("memory_usage_bytes".to_string(), report.memory_usage as f64);
                    metrics.insert("threshold_bytes".to_string(), self.thresholds.max_memory_usage as f64);
                    metrics
                },
            };
            warn!("Detected anomaly: {:?}", anomaly);
            self.anomalies.push(anomaly);
        }

        // Check system responsiveness
        if !report.actor_responsive {
            let anomaly = HealthAnomaly {
                timestamp: now,
                anomaly_type: AnomalyType::SystemUnresponsive,
                severity: AnomalySeverity::Critical,
                description: "System is not responding to health checks".to_string(),
                metrics: HashMap::new(),
            };
            error!("Detected critical anomaly: {:?}", anomaly);
            self.anomalies.push(anomaly);
        }

        // Check error rate
        let error_rate = report.error_count as f64 / 100.0; // Assuming 100 is max expected operations
        if error_rate > self.thresholds.max_error_rate {
            let anomaly = HealthAnomaly {
                timestamp: now,
                anomaly_type: AnomalyType::HighErrorRate,
                severity: if error_rate > self.thresholds.max_error_rate * 2.0 {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                },
                description: format!("Error rate {:.2}% exceeds threshold {:.2}%",
                                   error_rate * 100.0,
                                   self.thresholds.max_error_rate * 100.0),
                metrics: {
                    let mut metrics = HashMap::new();
                    metrics.insert("error_rate".to_string(), error_rate);
                    metrics.insert("threshold".to_string(), self.thresholds.max_error_rate);
                    metrics.insert("error_count".to_string(), report.error_count as f64);
                    metrics
                },
            };
            warn!("Detected anomaly: {:?}", anomaly);
            self.anomalies.push(anomaly);
        }
    }

    /// Calculate monitoring summary statistics
    fn calculate_summary(&self) -> MonitoringSummary {
        let duration = self.start_time.map(|start| start.elapsed()).unwrap_or_default();

        let (avg_response, max_response, min_response) = if self.response_times.is_empty() {
            (Duration::default(), Duration::default(), Duration::default())
        } else {
            let total_ms: u64 = self.response_times.iter().map(|d| d.as_millis() as u64).sum();
            let avg_ms = total_ms / self.response_times.len() as u64;
            let max_ms = self.response_times.iter().max().copied().unwrap_or_default();
            let min_ms = self.response_times.iter().min().copied().unwrap_or_default();

            (Duration::from_millis(avg_ms), max_ms, min_ms)
        };

        MonitoringSummary {
            duration,
            total_checks: self.stats.total_checks,
            successful_checks: self.stats.successful_checks,
            failed_checks: self.stats.failed_checks,
            average_response_time: avg_response,
            max_response_time: max_response,
            min_response_time: min_response,
            health_snapshots: self.health_snapshots.clone(),
            anomalies_detected: self.anomalies.clone(),
        }
    }
}

#[async_trait]
impl SystemMonitor for StorageActorMonitor {
    type Error = MonitoringError;

    async fn start_monitoring(&mut self) -> Result<(), Self::Error> {
        if self.monitoring_active {
            return Err(MonitoringError::AlreadyMonitoring);
        }

        info!("Starting system monitoring with interval: {:?}", self.check_interval);

        self.monitoring_active = true;
        self.start_time = Some(Instant::now());

        // Reset previous data
        self.health_snapshots.clear();
        self.response_times.clear();
        self.anomalies.clear();
        self.stats = MonitoringStats::default();

        Ok(())
    }

    async fn stop_monitoring(&mut self) -> Result<MonitoringSummary, Self::Error> {
        if !self.monitoring_active {
            return Err(MonitoringError::NotMonitoring);
        }

        info!("Stopping system monitoring");
        self.monitoring_active = false;

        let summary = self.calculate_summary();

        info!("Monitoring summary: {} total checks, {} successful, {} failed, {} anomalies detected",
              summary.total_checks, summary.successful_checks, summary.failed_checks, summary.anomalies_detected.len());

        Ok(summary)
    }

    async fn get_health(&self) -> Result<SystemHealthReport, Self::Error> {
        if !self.monitoring_active {
            return Err(MonitoringError::NotMonitoring);
        }

        // Return the most recent health snapshot or perform a new check
        if let Some(latest) = self.health_snapshots.last() {
            Ok(latest.clone())
        } else {
            Err(MonitoringError::NoDataAvailable)
        }
    }

    async fn is_system_responsive(&self) -> Result<bool, Self::Error> {
        let health = self.get_health().await?;
        Ok(health.actor_responsive)
    }
}

/// Background monitoring task that runs continuously
pub struct ContinuousMonitor {
    monitor: StorageActorMonitor,
    task_handle: Option<tokio::task::JoinHandle<Result<MonitoringSummary, MonitoringError>>>,
}

impl ContinuousMonitor {
    pub fn new(monitor: StorageActorMonitor) -> Self {
        Self {
            monitor,
            task_handle: None,
        }
    }

    /// Start continuous monitoring in background
    pub async fn start(&mut self) -> Result<(), MonitoringError> {
        if self.task_handle.is_some() {
            return Err(MonitoringError::AlreadyMonitoring);
        }

        let mut monitor_clone = self.monitor.clone(); // This would need proper cloning
        let check_interval = monitor_clone.check_interval;

        let handle = tokio::spawn(async move {
            monitor_clone.start_monitoring().await?;

            let mut interval = interval(check_interval);

            while monitor_clone.monitoring_active {
                interval.tick().await;

                if let Err(e) = monitor_clone.perform_health_check().await {
                    warn!("Health check failed: {:?}", e);
                }
            }

            monitor_clone.stop_monitoring().await
        });

        self.task_handle = Some(handle);
        Ok(())
    }

    /// Stop continuous monitoring and get results
    pub async fn stop(&mut self) -> Result<MonitoringSummary, MonitoringError> {
        if let Some(handle) = self.task_handle.take() {
            self.monitor.monitoring_active = false;
            handle.await.map_err(|e| MonitoringError::TaskError(e.to_string()))?
        } else {
            Err(MonitoringError::NotMonitoring)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MonitoringError {
    #[error("Monitoring is already active")]
    AlreadyMonitoring,
    #[error("Monitoring is not active")]
    NotMonitoring,
    #[error("No monitoring data available")]
    NoDataAvailable,
    #[error("Task execution error: {0}")]
    TaskError(String),
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
}

impl Default for StorageActorMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// Note: Clone would need proper implementation for concurrent use
impl Clone for StorageActorMonitor {
    fn clone(&self) -> Self {
        Self {
            monitoring_active: false, // Don't clone active state
            start_time: None,
            check_interval: self.check_interval,
            health_snapshots: Vec::new(),
            response_times: Vec::new(),
            anomalies: Vec::new(),
            thresholds: self.thresholds.clone(),
            stats: MonitoringStats::default(),
        }
    }
}