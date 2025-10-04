//! ChainActor V2 Performance Monitoring System (Phase 4: Task 4.3.2)
//!
//! Production-ready performance monitoring, optimization detection, and alerting.
//! Tracks critical operation timing, cross-actor latency, and memory trends.

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::ChainActor;

/// Performance status for health monitoring
#[derive(Debug, Clone)]
pub struct PerformanceStatus {
    pub block_production_healthy: bool,
    pub block_import_healthy: bool,
    pub communication_healthy: bool,
    pub overall_healthy: bool,
}

impl PerformanceStatus {
    pub fn new() -> Self {
        Self {
            block_production_healthy: true,
            block_import_healthy: true,
            communication_healthy: true,
            overall_healthy: true,
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.block_production_healthy &&
        self.block_import_healthy &&
        self.communication_healthy
    }

    pub fn calculate_overall(&mut self) {
        self.overall_healthy = self.is_healthy();
    }
}

/// Performance metrics tracker with rolling window
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Block production timings (rolling window of last 100)
    block_production_times: Arc<RwLock<VecDeque<Duration>>>,

    /// Block import timings (rolling window of last 100)
    block_import_times: Arc<RwLock<VecDeque<Duration>>>,

    /// Cross-actor communication latencies
    communication_latencies: Arc<RwLock<VecDeque<Duration>>>,

    /// Success/failure counts
    production_success_count: Arc<RwLock<u64>>,
    production_failure_count: Arc<RwLock<u64>>,
    import_success_count: Arc<RwLock<u64>>,
    import_failure_count: Arc<RwLock<u64>>,

    /// Performance thresholds (configurable)
    max_production_time: Duration,
    max_import_time: Duration,
    max_communication_latency: Duration,

    /// Rolling window size
    window_size: usize,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            block_production_times: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            block_import_times: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            communication_latencies: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            production_success_count: Arc::new(RwLock::new(0)),
            production_failure_count: Arc::new(RwLock::new(0)),
            import_success_count: Arc::new(RwLock::new(0)),
            import_failure_count: Arc::new(RwLock::new(0)),
            max_production_time: Duration::from_secs(5),
            max_import_time: Duration::from_secs(2),
            max_communication_latency: Duration::from_millis(100),
            window_size: 100,
        }
    }

    /// Record block production timing
    pub fn record_block_production(&self, duration: Duration, success: bool) {
        if let Ok(mut times) = self.block_production_times.write() {
            if times.len() >= self.window_size {
                times.pop_front();
            }
            times.push_back(duration);
        }

        if success {
            if let Ok(mut count) = self.production_success_count.write() {
                *count += 1;
            }
        } else {
            if let Ok(mut count) = self.production_failure_count.write() {
                *count += 1;
            }
        }

        if duration > self.max_production_time {
            warn!(
                duration_ms = duration.as_millis(),
                threshold_ms = self.max_production_time.as_millis(),
                "Block production exceeded performance threshold"
            );
        }
    }

    /// Record block import timing
    pub fn record_block_import(&self, duration: Duration, success: bool) {
        if let Ok(mut times) = self.block_import_times.write() {
            if times.len() >= self.window_size {
                times.pop_front();
            }
            times.push_back(duration);
        }

        if success {
            if let Ok(mut count) = self.import_success_count.write() {
                *count += 1;
            }
        } else {
            if let Ok(mut count) = self.import_failure_count.write() {
                *count += 1;
            }
        }

        if duration > self.max_import_time {
            warn!(
                duration_ms = duration.as_millis(),
                threshold_ms = self.max_import_time.as_millis(),
                "Block import exceeded performance threshold"
            );
        }
    }

    /// Record cross-actor communication latency
    pub fn record_communication_latency(&self, duration: Duration) {
        if let Ok(mut latencies) = self.communication_latencies.write() {
            if latencies.len() >= self.window_size {
                latencies.pop_front();
            }
            latencies.push_back(duration);
        }

        if duration > self.max_communication_latency {
            warn!(
                duration_ms = duration.as_millis(),
                threshold_ms = self.max_communication_latency.as_millis(),
                "Cross-actor communication exceeded latency threshold"
            );
        }
    }

    /// Get average block production time
    pub fn get_average_block_production_time(&self) -> Duration {
        if let Ok(times) = self.block_production_times.read() {
            if times.is_empty() {
                return Duration::from_secs(0);
            }
            let sum: Duration = times.iter().sum();
            sum / times.len() as u32
        } else {
            Duration::from_secs(0)
        }
    }

    /// Get average block import time
    pub fn get_average_block_import_time(&self) -> Duration {
        if let Ok(times) = self.block_import_times.read() {
            if times.is_empty() {
                return Duration::from_secs(0);
            }
            let sum: Duration = times.iter().sum();
            sum / times.len() as u32
        } else {
            Duration::from_secs(0)
        }
    }

    /// Get 95th percentile block production time
    pub fn get_p95_block_production_time(&self) -> Duration {
        if let Ok(times) = self.block_production_times.read() {
            let mut sorted: Vec<Duration> = times.iter().copied().collect();
            sorted.sort();
            let index = (sorted.len() as f64 * 0.95) as usize;
            sorted.get(index).copied().unwrap_or(Duration::from_secs(0))
        } else {
            Duration::from_secs(0)
        }
    }

    /// Get production success rate
    pub fn get_production_success_rate(&self) -> f64 {
        let success = self.production_success_count.read().ok().map(|c| *c).unwrap_or(0);
        let failure = self.production_failure_count.read().ok().map(|c| *c).unwrap_or(0);
        let total = success + failure;

        if total == 0 {
            1.0
        } else {
            success as f64 / total as f64
        }
    }

    /// Get import success rate
    pub fn get_import_success_rate(&self) -> f64 {
        let success = self.import_success_count.read().ok().map(|c| *c).unwrap_or(0);
        let failure = self.import_failure_count.read().ok().map(|c| *c).unwrap_or(0);
        let total = success + failure;

        if total == 0 {
            1.0
        } else {
            success as f64 / total as f64
        }
    }
}

impl ChainActor {
    /// Monitor block production performance (Phase 4: Task 4.3.2)
    pub fn monitor_block_production(&self, duration: Duration, success: bool) {
        self.metrics.performance.record_block_production(duration, success);

        if success {
            info!(
                duration_ms = duration.as_millis(),
                "Block production completed successfully"
            );
        } else {
            warn!(
                duration_ms = duration.as_millis(),
                "Block production failed"
            );
        }
    }

    /// Monitor block import performance (Phase 4: Task 4.3.2)
    pub fn monitor_block_import(&self, duration: Duration, success: bool) {
        self.metrics.performance.record_block_import(duration, success);

        if success {
            debug!(
                duration_ms = duration.as_millis(),
                "Block import completed successfully"
            );
        } else {
            warn!(
                duration_ms = duration.as_millis(),
                "Block import failed"
            );
        }
    }

    /// Check for performance degradation (Phase 4: Task 4.3.2)
    pub async fn check_performance_health(&self) -> PerformanceStatus {
        let mut status = PerformanceStatus::new();

        // Check average block production time
        let avg_production_time = self.metrics.performance.get_average_block_production_time();
        status.block_production_healthy = avg_production_time < Duration::from_secs(5);

        if !status.block_production_healthy {
            warn!(
                avg_duration_ms = avg_production_time.as_millis(),
                "Block production performance degraded"
            );
        }

        // Check average block import time
        let avg_import_time = self.metrics.performance.get_average_block_import_time();
        status.block_import_healthy = avg_import_time < Duration::from_secs(2);

        if !status.block_import_healthy {
            warn!(
                avg_duration_ms = avg_import_time.as_millis(),
                "Block import performance degraded"
            );
        }

        // Check cross-actor communication latency
        let comm_latency = self.measure_cross_actor_latency().await;
        status.communication_healthy = comm_latency < Duration::from_millis(100);

        if !status.communication_healthy {
            warn!(
                latency_ms = comm_latency.as_millis(),
                "Cross-actor communication latency degraded"
            );
        }

        status.calculate_overall();

        info!(
            block_production_healthy = status.block_production_healthy,
            block_import_healthy = status.block_import_healthy,
            communication_healthy = status.communication_healthy,
            overall_healthy = status.overall_healthy,
            "Performance health check completed"
        );

        status
    }

    /// Measure cross-actor communication latency (Phase 4: Task 4.3.2)
    pub async fn measure_cross_actor_latency(&self) -> Duration {
        let start = Instant::now();

        // Test storage communication
        if let Some(ref storage_actor) = self.storage_actor {
            let health_msg = crate::actors_v2::storage::messages::HealthCheckMessage {
                correlation_id: Some(Uuid::new_v4()),
            };
            let _ = storage_actor.send(health_msg).await;
        }

        let latency = start.elapsed();
        self.metrics.performance.record_communication_latency(latency);
        latency
    }

    /// Get performance summary for monitoring dashboards (Phase 4)
    pub fn get_performance_summary(&self) -> PerformanceSummary {
        let avg_production = self.metrics.performance.get_average_block_production_time();
        let p95_production = self.metrics.performance.get_p95_block_production_time();
        let avg_import = self.metrics.performance.get_average_block_import_time();
        let production_rate = self.metrics.performance.get_production_success_rate();
        let import_rate = self.metrics.performance.get_import_success_rate();

        PerformanceSummary {
            avg_block_production_ms: avg_production.as_millis() as u64,
            p95_block_production_ms: p95_production.as_millis() as u64,
            avg_block_import_ms: avg_import.as_millis() as u64,
            production_success_rate: production_rate,
            import_success_rate: import_rate,
        }
    }
}

/// Performance summary for external monitoring
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub avg_block_production_ms: u64,
    pub p95_block_production_ms: u64,
    pub avg_block_import_ms: u64,
    pub production_success_rate: f64,
    pub import_success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_metrics_creation() {
        let metrics = PerformanceMetrics::new();
        assert_eq!(metrics.get_average_block_production_time(), Duration::from_secs(0));
        assert_eq!(metrics.get_production_success_rate(), 1.0);
    }

    #[test]
    fn test_performance_metrics_recording() {
        let metrics = PerformanceMetrics::new();

        metrics.record_block_production(Duration::from_secs(2), true);
        metrics.record_block_production(Duration::from_secs(3), true);

        let avg = metrics.get_average_block_production_time();
        assert!(avg >= Duration::from_millis(2000));
        assert!(avg <= Duration::from_millis(3000));

        assert_eq!(metrics.get_production_success_rate(), 1.0);
    }

    #[test]
    fn test_performance_metrics_success_rate() {
        let metrics = PerformanceMetrics::new();

        metrics.record_block_production(Duration::from_secs(1), true);
        metrics.record_block_production(Duration::from_secs(1), true);
        metrics.record_block_production(Duration::from_secs(1), false);

        assert!((metrics.get_production_success_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_performance_status() {
        let mut status = PerformanceStatus::new();
        assert!(status.is_healthy());

        status.block_production_healthy = false;
        status.calculate_overall();
        assert!(!status.is_healthy());
        assert!(!status.overall_healthy);
    }
}
