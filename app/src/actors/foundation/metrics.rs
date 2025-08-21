//! Adapter Metrics Collection - ALYS-006-20 Implementation
//! 
//! Comprehensive metrics collection system for adapter performance monitoring,
//! latency comparison, migration progress tracking, and performance optimization
//! for the Alys V2 sidechain legacy integration adapters.

use crate::actors::foundation::{
    adapters::{AdapterMetrics, AdapterPerformanceSummary, MigrationState, MigrationPhase},
    constants::{adapter, migration, performance},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error, instrument};

/// Comprehensive metrics collector for adapter systems
pub struct AdapterMetricsCollector {
    /// Per-adapter metrics storage
    adapter_metrics: Arc<RwLock<HashMap<String, AdapterMetricsStorage>>>,
    /// Global migration metrics
    migration_metrics: Arc<RwLock<MigrationMetricsStorage>>,
    /// Performance trend analysis
    performance_trends: Arc<RwLock<PerformanceTrendAnalyzer>>,
    /// Alerting system
    alerting: Arc<RwLock<AdapterAlertingSystem>>,
    /// Metrics configuration
    config: MetricsConfig,
    /// Collection start time
    collection_start: Instant,
}

impl AdapterMetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        info!("Initializing adapter metrics collector with config: {:?}", config);
        
        Self {
            adapter_metrics: Arc::new(RwLock::new(HashMap::new())),
            migration_metrics: Arc::new(RwLock::new(MigrationMetricsStorage::new())),
            performance_trends: Arc::new(RwLock::new(PerformanceTrendAnalyzer::new(config.trend_analysis_window))),
            alerting: Arc::new(RwLock::new(AdapterAlertingSystem::new(config.alert_thresholds.clone()))),
            config,
            collection_start: Instant::now(),
        }
    }

    /// Record adapter operation metrics
    #[instrument(skip(self, metrics))]
    pub async fn record_adapter_metrics(&self, adapter_name: &str, metrics: AdapterMetrics) {
        debug!("Recording metrics for adapter: {}", adapter_name);
        
        let mut storage = self.adapter_metrics.write().await;
        let adapter_storage = storage.entry(adapter_name.to_string())
            .or_insert_with(|| AdapterMetricsStorage::new(adapter_name.to_string()));
        
        adapter_storage.add_metrics(metrics.clone()).await;
        
        // Update performance trends
        if let (Some(legacy_duration), Some(actor_duration)) = (metrics.legacy_duration, metrics.actor_duration) {
            let performance_ratio = actor_duration.as_nanos() as f64 / legacy_duration.as_nanos() as f64;
            
            let mut trends = self.performance_trends.write().await;
            trends.add_performance_sample(adapter_name, performance_ratio, metrics.timestamp);
        }
        
        // Check for alerts
        let mut alerting = self.alerting.write().await;
        if let Err(alert) = alerting.check_metrics(adapter_name, &metrics).await {
            warn!("Adapter alert triggered: {:?}", alert);
        }
    }

    /// Record migration state change
    #[instrument(skip(self))]
    pub async fn record_migration_state_change(
        &self,
        adapter_name: &str,
        old_state: MigrationState,
        new_state: MigrationState,
    ) {
        info!("Migration state change for {}: {:?} -> {:?}", adapter_name, old_state, new_state);
        
        let mut migration_metrics = self.migration_metrics.write().await;
        migration_metrics.record_state_transition(adapter_name, old_state, new_state).await;
    }

    /// Record migration phase change
    #[instrument(skip(self))]
    pub async fn record_migration_phase_change(
        &self,
        old_phase: MigrationPhase,
        new_phase: MigrationPhase,
        duration_in_phase: Duration,
    ) {
        info!("Migration phase change: {:?} -> {:?} (duration: {:?})", old_phase, new_phase, duration_in_phase);
        
        let mut migration_metrics = self.migration_metrics.write().await;
        migration_metrics.record_phase_transition(old_phase, new_phase, duration_in_phase).await;
    }

    /// Get adapter performance summary
    pub async fn get_adapter_summary(&self, adapter_name: &str) -> Option<AdapterPerformanceSummary> {
        let storage = self.adapter_metrics.read().await;
        
        if let Some(adapter_storage) = storage.get(adapter_name) {
            Some(adapter_storage.get_performance_summary().await)
        } else {
            None
        }
    }

    /// Get migration progress report
    pub async fn get_migration_progress_report(&self) -> MigrationProgressReport {
        let migration_metrics = self.migration_metrics.read().await;
        migration_metrics.get_progress_report().await
    }

    /// Get performance comparison report
    pub async fn get_performance_comparison_report(&self) -> PerformanceComparisonReport {
        let mut report = PerformanceComparisonReport::new();
        
        let storage = self.adapter_metrics.read().await;
        for (adapter_name, adapter_storage) in storage.iter() {
            let summary = adapter_storage.get_performance_summary().await;
            report.add_adapter_summary(adapter_name.clone(), summary);
        }
        
        let trends = self.performance_trends.read().await;
        report.performance_trends = Some(trends.get_trend_analysis().await);
        
        report.calculate_overall_metrics();
        report
    }

    /// Get latency analysis
    pub async fn get_latency_analysis(&self, adapter_name: &str) -> Option<LatencyAnalysis> {
        let storage = self.adapter_metrics.read().await;
        
        if let Some(adapter_storage) = storage.get(adapter_name) {
            Some(adapter_storage.get_latency_analysis().await)
        } else {
            None
        }
    }

    /// Get system health metrics
    pub async fn get_system_health_metrics(&self) -> SystemHealthMetrics {
        let mut health_metrics = SystemHealthMetrics::new();
        
        let storage = self.adapter_metrics.read().await;
        for (adapter_name, adapter_storage) in storage.iter() {
            let summary = adapter_storage.get_performance_summary().await;
            health_metrics.add_adapter_health(adapter_name.clone(), summary);
        }
        
        let alerting = self.alerting.read().await;
        health_metrics.active_alerts = alerting.get_active_alerts().await;
        
        health_metrics.calculate_overall_health();
        health_metrics
    }

    /// Generate comprehensive metrics report
    pub async fn generate_comprehensive_report(&self) -> ComprehensiveMetricsReport {
        info!("Generating comprehensive metrics report");
        
        let performance_report = self.get_performance_comparison_report().await;
        let migration_progress = self.get_migration_progress_report().await;
        let system_health = self.get_system_health_metrics().await;
        
        let mut latency_analyses = HashMap::new();
        let storage = self.adapter_metrics.read().await;
        for adapter_name in storage.keys() {
            if let Some(analysis) = self.get_latency_analysis(adapter_name).await {
                latency_analyses.insert(adapter_name.clone(), analysis);
            }
        }
        
        ComprehensiveMetricsReport {
            report_timestamp: SystemTime::now(),
            collection_duration: self.collection_start.elapsed(),
            performance_comparison: performance_report,
            migration_progress,
            system_health,
            latency_analyses,
            recommendations: self.generate_optimization_recommendations().await,
        }
    }

    /// Generate optimization recommendations based on metrics
    async fn generate_optimization_recommendations(&self) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();
        
        let storage = self.adapter_metrics.read().await;
        for (adapter_name, adapter_storage) in storage.iter() {
            let summary = adapter_storage.get_performance_summary().await;
            
            // Check for performance degradation
            if let Some(ratio) = summary.performance_ratio {
                if ratio > self.config.alert_thresholds.performance_degradation_threshold {
                    recommendations.push(OptimizationRecommendation {
                        adapter_name: adapter_name.clone(),
                        recommendation_type: RecommendationType::PerformanceOptimization,
                        priority: RecommendationPriority::High,
                        description: format!(
                            "Actor implementation is {:.2}x slower than legacy. Consider optimization.",
                            ratio
                        ),
                        estimated_impact: format!("Potential {:.1}% performance improvement", (ratio - 1.0) * 100.0),
                    });
                }
            }
            
            // Check for low success rate
            if summary.success_rate < self.config.alert_thresholds.success_rate_threshold {
                recommendations.push(OptimizationRecommendation {
                    adapter_name: adapter_name.clone(),
                    recommendation_type: RecommendationType::ReliabilityImprovement,
                    priority: RecommendationPriority::Critical,
                    description: format!(
                        "Low success rate: {:.1}%. Investigate error patterns.",
                        summary.success_rate * 100.0
                    ),
                    estimated_impact: "Critical for system stability".to_string(),
                });
            }
        }
        
        // Check migration progress
        let migration_metrics = self.migration_metrics.read().await;
        let progress_report = migration_metrics.get_progress_report().await;
        
        if progress_report.overall_progress < 50.0 && 
           self.collection_start.elapsed() > Duration::from_secs(3600) { // 1 hour
            recommendations.push(OptimizationRecommendation {
                adapter_name: "migration_coordinator".to_string(),
                recommendation_type: RecommendationType::MigrationAcceleration,
                priority: RecommendationPriority::Medium,
                description: "Migration progress is slow. Consider increasing feature flag rollout.".to_string(),
                estimated_impact: "Faster migration completion".to_string(),
            });
        }
        
        recommendations
    }

    /// Cleanup old metrics data
    #[instrument(skip(self))]
    pub async fn cleanup_old_metrics(&self) -> usize {
        let cutoff_time = SystemTime::now() - self.config.metrics_retention_period;
        let mut total_cleaned = 0;
        
        let mut storage = self.adapter_metrics.write().await;
        for (adapter_name, adapter_storage) in storage.iter_mut() {
            let cleaned = adapter_storage.cleanup_old_metrics(cutoff_time).await;
            total_cleaned += cleaned;
            debug!("Cleaned {} old metrics for adapter: {}", cleaned, adapter_name);
        }
        
        info!("Cleaned {} total old metrics entries", total_cleaned);
        total_cleaned
    }
}

/// Per-adapter metrics storage
pub struct AdapterMetricsStorage {
    adapter_name: String,
    metrics_history: VecDeque<AdapterMetrics>,
    performance_samples: VecDeque<PerformanceSample>,
    error_history: VecDeque<ErrorSample>,
    last_cleanup: SystemTime,
}

impl AdapterMetricsStorage {
    pub fn new(adapter_name: String) -> Self {
        Self {
            adapter_name,
            metrics_history: VecDeque::new(),
            performance_samples: VecDeque::new(),
            error_history: VecDeque::new(),
            last_cleanup: SystemTime::now(),
        }
    }

    pub async fn add_metrics(&mut self, metrics: AdapterMetrics) {
        self.metrics_history.push_back(metrics.clone());
        
        // Add performance sample if both durations are available
        if let (Some(legacy_duration), Some(actor_duration)) = (metrics.legacy_duration, metrics.actor_duration) {
            let sample = PerformanceSample {
                timestamp: metrics.timestamp,
                legacy_duration,
                actor_duration,
                operation: metrics.operation.clone(),
                success: metrics.success,
            };
            self.performance_samples.push_back(sample);
        }
        
        // Track errors
        if !metrics.success {
            let error_sample = ErrorSample {
                timestamp: metrics.timestamp,
                operation: metrics.operation.clone(),
                error_message: metrics.error.unwrap_or_else(|| "Unknown error".to_string()),
                metadata: metrics.metadata.clone(),
            };
            self.error_history.push_back(error_sample);
        }
        
        // Limit history size
        while self.metrics_history.len() > adapter::MAX_METRICS_HISTORY {
            self.metrics_history.pop_front();
        }
        
        while self.performance_samples.len() > adapter::MAX_METRICS_HISTORY {
            self.performance_samples.pop_front();
        }
        
        while self.error_history.len() > adapter::MAX_METRICS_HISTORY / 10 {
            self.error_history.pop_front();
        }
    }

    pub async fn get_performance_summary(&self) -> AdapterPerformanceSummary {
        let total_operations = self.metrics_history.len();
        let successful_operations = self.metrics_history.iter().filter(|m| m.success).count();
        
        let success_rate = if total_operations > 0 {
            successful_operations as f64 / total_operations as f64
        } else {
            0.0
        };
        
        let (legacy_avg, actor_avg, performance_ratio) = if !self.performance_samples.is_empty() {
            let legacy_total: Duration = self.performance_samples.iter().map(|s| s.legacy_duration).sum();
            let actor_total: Duration = self.performance_samples.iter().map(|s| s.actor_duration).sum();
            
            let legacy_avg = legacy_total / self.performance_samples.len() as u32;
            let actor_avg = actor_total / self.performance_samples.len() as u32;
            
            let ratio = if legacy_avg.as_nanos() > 0 {
                Some(actor_avg.as_nanos() as f64 / legacy_avg.as_nanos() as f64)
            } else {
                None
            };
            
            (Some(legacy_avg), Some(actor_avg), ratio)
        } else {
            (None, None, None)
        };
        
        AdapterPerformanceSummary {
            adapter_name: self.adapter_name.clone(),
            total_operations,
            success_rate,
            legacy_avg_duration: legacy_avg,
            actor_avg_duration: actor_avg,
            performance_ratio,
            last_updated: SystemTime::now(),
        }
    }

    pub async fn get_latency_analysis(&self) -> LatencyAnalysis {
        let mut analysis = LatencyAnalysis::new(self.adapter_name.clone());
        
        if self.performance_samples.is_empty() {
            return analysis;
        }
        
        // Calculate percentiles for legacy and actor durations
        let mut legacy_durations: Vec<_> = self.performance_samples.iter().map(|s| s.legacy_duration).collect();
        let mut actor_durations: Vec<_> = self.performance_samples.iter().map(|s| s.actor_duration).collect();
        
        legacy_durations.sort();
        actor_durations.sort();
        
        // Legacy percentiles
        analysis.legacy_percentiles = LatencyPercentiles {
            p50: legacy_durations[legacy_durations.len() * 50 / 100],
            p90: legacy_durations[legacy_durations.len() * 90 / 100],
            p95: legacy_durations[legacy_durations.len() * 95 / 100],
            p99: legacy_durations[legacy_durations.len() * 99 / 100],
            min: legacy_durations[0],
            max: legacy_durations[legacy_durations.len() - 1],
        };
        
        // Actor percentiles
        analysis.actor_percentiles = LatencyPercentiles {
            p50: actor_durations[actor_durations.len() * 50 / 100],
            p90: actor_durations[actor_durations.len() * 90 / 100],
            p95: actor_durations[actor_durations.len() * 95 / 100],
            p99: actor_durations[actor_durations.len() * 99 / 100],
            min: actor_durations[0],
            max: actor_durations[actor_durations.len() - 1],
        };
        
        // Calculate improvement metrics
        analysis.median_improvement = if analysis.legacy_percentiles.p50.as_nanos() > 0 {
            1.0 - (analysis.actor_percentiles.p50.as_nanos() as f64 / analysis.legacy_percentiles.p50.as_nanos() as f64)
        } else {
            0.0
        };
        
        analysis.p99_improvement = if analysis.legacy_percentiles.p99.as_nanos() > 0 {
            1.0 - (analysis.actor_percentiles.p99.as_nanos() as f64 / analysis.legacy_percentiles.p99.as_nanos() as f64)
        } else {
            0.0
        };
        
        analysis
    }

    pub async fn cleanup_old_metrics(&mut self, cutoff_time: SystemTime) -> usize {
        let initial_count = self.metrics_history.len();
        
        self.metrics_history.retain(|m| m.timestamp > cutoff_time);
        self.performance_samples.retain(|s| s.timestamp > cutoff_time);
        self.error_history.retain(|e| e.timestamp > cutoff_time);
        
        let cleaned_count = initial_count - self.metrics_history.len();
        self.last_cleanup = SystemTime::now();
        
        cleaned_count
    }
}

/// Migration metrics storage and analysis
#[derive(Debug)]
pub struct MigrationMetricsStorage {
    state_transitions: VecDeque<StateTransition>,
    phase_transitions: VecDeque<PhaseTransition>,
    migration_start_time: Option<SystemTime>,
    last_phase_change: SystemTime,
}

impl MigrationMetricsStorage {
    pub fn new() -> Self {
        Self {
            state_transitions: VecDeque::new(),
            phase_transitions: VecDeque::new(),
            migration_start_time: None,
            last_phase_change: SystemTime::now(),
        }
    }

    pub async fn record_state_transition(&mut self, adapter_name: &str, old_state: MigrationState, new_state: MigrationState) {
        let transition = StateTransition {
            adapter_name: adapter_name.to_string(),
            old_state,
            new_state,
            timestamp: SystemTime::now(),
        };
        
        self.state_transitions.push_back(transition);
        
        // Limit history
        while self.state_transitions.len() > migration::MAX_MIGRATION_PHASES * 100 {
            self.state_transitions.pop_front();
        }
    }

    pub async fn record_phase_transition(&mut self, old_phase: MigrationPhase, new_phase: MigrationPhase, duration: Duration) {
        if self.migration_start_time.is_none() && matches!(old_phase, MigrationPhase::Planning) {
            self.migration_start_time = Some(SystemTime::now() - duration);
        }

        let transition = PhaseTransition {
            old_phase,
            new_phase,
            duration_in_previous_phase: duration,
            timestamp: SystemTime::now(),
        };
        
        self.phase_transitions.push_back(transition);
        self.last_phase_change = SystemTime::now();
        
        // Limit history
        while self.phase_transitions.len() > migration::MAX_MIGRATION_PHASES * 10 {
            self.phase_transitions.pop_front();
        }
    }

    pub async fn get_progress_report(&self) -> MigrationProgressReport {
        let current_phase = self.phase_transitions.back()
            .map(|t| t.new_phase.clone())
            .unwrap_or(MigrationPhase::Planning);

        let progress_percentage = self.calculate_progress_percentage(&current_phase);
        
        let migration_duration = self.migration_start_time
            .map(|start| SystemTime::now().duration_since(start).unwrap_or_default())
            .unwrap_or_default();

        let estimated_completion = self.estimate_completion_time(&current_phase, migration_duration);

        // Calculate state distribution
        let mut state_distribution = HashMap::new();
        let recent_transitions = self.state_transitions.iter()
            .rev()
            .take(100) // Look at recent transitions only
            .collect::<Vec<_>>();

        for transition in recent_transitions {
            *state_distribution.entry(format!("{:?}", transition.new_state)).or_insert(0) += 1;
        }

        MigrationProgressReport {
            current_phase,
            overall_progress: progress_percentage,
            migration_duration,
            estimated_completion,
            phase_history: self.phase_transitions.iter().cloned().collect(),
            state_distribution,
            total_state_transitions: self.state_transitions.len(),
            last_phase_change: self.last_phase_change,
        }
    }

    fn calculate_progress_percentage(&self, current_phase: &MigrationPhase) -> f64 {
        match current_phase {
            MigrationPhase::Planning => 0.0,
            MigrationPhase::GradualRollout => 20.0,
            MigrationPhase::PerformanceValidation => 50.0,
            MigrationPhase::FinalCutover => 80.0,
            MigrationPhase::Complete => 100.0,
            MigrationPhase::Rollback { .. } => 0.0, // Rollback resets progress
        }
    }

    fn estimate_completion_time(&self, current_phase: &MigrationPhase, total_duration: Duration) -> Option<Duration> {
        let progress = self.calculate_progress_percentage(current_phase);
        
        if progress > 0.0 && progress < 100.0 {
            let estimated_total = total_duration.as_secs_f64() / (progress / 100.0);
            Some(Duration::from_secs_f64(estimated_total - total_duration.as_secs_f64()))
        } else {
            None
        }
    }
}

/// Performance trend analyzer
pub struct PerformanceTrendAnalyzer {
    trend_window: Duration,
    performance_history: HashMap<String, VecDeque<TrendSample>>,
}

impl PerformanceTrendAnalyzer {
    pub fn new(trend_window: Duration) -> Self {
        Self {
            trend_window,
            performance_history: HashMap::new(),
        }
    }

    pub fn add_performance_sample(&mut self, adapter_name: &str, performance_ratio: f64, timestamp: SystemTime) {
        let history = self.performance_history.entry(adapter_name.to_string())
            .or_insert_with(VecDeque::new);

        let sample = TrendSample {
            performance_ratio,
            timestamp,
        };
        
        history.push_back(sample);
        
        // Remove old samples outside the trend window
        let cutoff_time = timestamp - self.trend_window;
        history.retain(|s| s.timestamp > cutoff_time);
        
        // Limit history size
        while history.len() > 1000 {
            history.pop_front();
        }
    }

    pub async fn get_trend_analysis(&self) -> TrendAnalysis {
        let mut analysis = TrendAnalysis::new();
        
        for (adapter_name, history) in &self.performance_history {
            if history.len() < 2 {
                continue;
            }
            
            let trend = self.calculate_linear_trend(history);
            analysis.adapter_trends.insert(adapter_name.clone(), trend);
        }
        
        analysis
    }

    fn calculate_linear_trend(&self, samples: &VecDeque<TrendSample>) -> PerformanceTrend {
        if samples.len() < 2 {
            return PerformanceTrend {
                slope: 0.0,
                direction: TrendDirection::Stable,
                confidence: 0.0,
                recent_average: 0.0,
            };
        }

        // Simple linear regression
        let n = samples.len() as f64;
        let x_values: Vec<f64> = (0..samples.len()).map(|i| i as f64).collect();
        let y_values: Vec<f64> = samples.iter().map(|s| s.performance_ratio).collect();
        
        let x_mean = x_values.iter().sum::<f64>() / n;
        let y_mean = y_values.iter().sum::<f64>() / n;
        
        let numerator: f64 = x_values.iter().zip(y_values.iter())
            .map(|(x, y)| (x - x_mean) * (y - y_mean))
            .sum();
        
        let denominator: f64 = x_values.iter()
            .map(|x| (x - x_mean).powi(2))
            .sum();
        
        let slope = if denominator != 0.0 {
            numerator / denominator
        } else {
            0.0
        };
        
        let direction = if slope > 0.01 {
            TrendDirection::Degrading
        } else if slope < -0.01 {
            TrendDirection::Improving
        } else {
            TrendDirection::Stable
        };
        
        // Calculate R-squared for confidence
        let y_predicted: Vec<f64> = x_values.iter()
            .map(|x| y_mean + slope * (x - x_mean))
            .collect();
        
        let ss_tot: f64 = y_values.iter().map(|y| (y - y_mean).powi(2)).sum();
        let ss_res: f64 = y_values.iter().zip(y_predicted.iter())
            .map(|(y, y_pred)| (y - y_pred).powi(2))
            .sum();
        
        let r_squared = if ss_tot != 0.0 {
            1.0 - (ss_res / ss_tot)
        } else {
            0.0
        };
        
        let recent_average = y_values.iter().rev().take(10).sum::<f64>() / 10.0.min(y_values.len() as f64);
        
        PerformanceTrend {
            slope,
            direction,
            confidence: r_squared,
            recent_average,
        }
    }
}

/// Adapter alerting system
pub struct AdapterAlertingSystem {
    alert_thresholds: AlertThresholds,
    active_alerts: HashMap<String, Vec<AdapterAlert>>,
    alert_history: VecDeque<AdapterAlert>,
}

impl AdapterAlertingSystem {
    pub fn new(alert_thresholds: AlertThresholds) -> Self {
        Self {
            alert_thresholds,
            active_alerts: HashMap::new(),
            alert_history: VecDeque::new(),
        }
    }

    pub async fn check_metrics(&mut self, adapter_name: &str, metrics: &AdapterMetrics) -> Result<(), AdapterAlert> {
        let mut alerts = Vec::new();
        
        // Check success rate
        if !metrics.success && self.should_alert_on_failure(adapter_name) {
            alerts.push(AdapterAlert {
                adapter_name: adapter_name.to_string(),
                alert_type: AlertType::OperationFailure,
                severity: AlertSeverity::Medium,
                message: format!("Operation failed: {}", metrics.operation),
                timestamp: metrics.timestamp,
                metadata: metrics.metadata.clone(),
            });
        }
        
        // Check performance degradation
        if let (Some(legacy_duration), Some(actor_duration)) = (metrics.legacy_duration, metrics.actor_duration) {
            let performance_ratio = actor_duration.as_nanos() as f64 / legacy_duration.as_nanos() as f64;
            
            if performance_ratio > self.alert_thresholds.performance_degradation_threshold {
                alerts.push(AdapterAlert {
                    adapter_name: adapter_name.to_string(),
                    alert_type: AlertType::PerformanceDegradation,
                    severity: AlertSeverity::High,
                    message: format!("Performance degraded: {:.2}x slower than legacy", performance_ratio),
                    timestamp: metrics.timestamp,
                    metadata: vec![
                        ("performance_ratio".to_string(), performance_ratio.to_string()),
                        ("legacy_duration_ms".to_string(), legacy_duration.as_millis().to_string()),
                        ("actor_duration_ms".to_string(), actor_duration.as_millis().to_string()),
                    ].into_iter().collect(),
                });
            }
        }
        
        // Store alerts
        if !alerts.is_empty() {
            self.active_alerts.entry(adapter_name.to_string())
                .or_insert_with(Vec::new)
                .extend(alerts.clone());
            
            for alert in &alerts {
                self.alert_history.push_back(alert.clone());
            }
            
            // Limit alert history
            while self.alert_history.len() > 1000 {
                self.alert_history.pop_front();
            }
            
            return Err(alerts.into_iter().next().unwrap());
        }
        
        Ok(())
    }

    fn should_alert_on_failure(&self, adapter_name: &str) -> bool {
        // Implement rate limiting logic here
        // For now, alert on every failure
        true
    }

    pub async fn get_active_alerts(&self) -> Vec<AdapterAlert> {
        self.active_alerts.values().flatten().cloned().collect()
    }
}

/// Metrics configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// How long to retain metrics data
    pub metrics_retention_period: Duration,
    /// Window for trend analysis
    pub trend_analysis_window: Duration,
    /// Collection interval
    pub collection_interval: Duration,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            metrics_retention_period: Duration::from_secs(24 * 3600), // 24 hours
            trend_analysis_window: Duration::from_secs(3600), // 1 hour
            collection_interval: Duration::from_secs(60), // 1 minute
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

/// Alert thresholds configuration
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub performance_degradation_threshold: f64,
    pub success_rate_threshold: f64,
    pub latency_threshold: Duration,
    pub error_rate_threshold: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            performance_degradation_threshold: 2.0, // 2x slower than legacy
            success_rate_threshold: 0.95, // 95% success rate
            latency_threshold: Duration::from_secs(1),
            error_rate_threshold: 0.05, // 5% error rate
        }
    }
}

/// Performance sample for trend analysis
#[derive(Debug, Clone)]
pub struct PerformanceSample {
    pub timestamp: SystemTime,
    pub legacy_duration: Duration,
    pub actor_duration: Duration,
    pub operation: String,
    pub success: bool,
}

/// Error sample for error analysis
#[derive(Debug, Clone)]
pub struct ErrorSample {
    pub timestamp: SystemTime,
    pub operation: String,
    pub error_message: String,
    pub metadata: HashMap<String, String>,
}

/// State transition record
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub adapter_name: String,
    pub old_state: MigrationState,
    pub new_state: MigrationState,
    pub timestamp: SystemTime,
}

/// Phase transition record
#[derive(Debug, Clone)]
pub struct PhaseTransition {
    pub old_phase: MigrationPhase,
    pub new_phase: MigrationPhase,
    pub duration_in_previous_phase: Duration,
    pub timestamp: SystemTime,
}

/// Trend sample for performance analysis
#[derive(Debug, Clone)]
pub struct TrendSample {
    pub performance_ratio: f64,
    pub timestamp: SystemTime,
}

/// Latency analysis results
#[derive(Debug, Clone)]
pub struct LatencyAnalysis {
    pub adapter_name: String,
    pub legacy_percentiles: LatencyPercentiles,
    pub actor_percentiles: LatencyPercentiles,
    pub median_improvement: f64, // Positive means actor is faster
    pub p99_improvement: f64,    // Positive means actor is faster
}

impl LatencyAnalysis {
    pub fn new(adapter_name: String) -> Self {
        Self {
            adapter_name,
            legacy_percentiles: LatencyPercentiles::default(),
            actor_percentiles: LatencyPercentiles::default(),
            median_improvement: 0.0,
            p99_improvement: 0.0,
        }
    }
}

/// Latency percentiles
#[derive(Debug, Clone, Default)]
pub struct LatencyPercentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub min: Duration,
    pub max: Duration,
}

/// Performance comparison report
#[derive(Debug)]
pub struct PerformanceComparisonReport {
    pub adapter_summaries: HashMap<String, AdapterPerformanceSummary>,
    pub overall_performance_ratio: f64,
    pub overall_success_rate: f64,
    pub performance_trends: Option<TrendAnalysis>,
    pub report_timestamp: SystemTime,
}

impl PerformanceComparisonReport {
    pub fn new() -> Self {
        Self {
            adapter_summaries: HashMap::new(),
            overall_performance_ratio: 0.0,
            overall_success_rate: 0.0,
            performance_trends: None,
            report_timestamp: SystemTime::now(),
        }
    }

    pub fn add_adapter_summary(&mut self, adapter_name: String, summary: AdapterPerformanceSummary) {
        self.adapter_summaries.insert(adapter_name, summary);
    }

    pub fn calculate_overall_metrics(&mut self) {
        if self.adapter_summaries.is_empty() {
            return;
        }

        let total_operations: usize = self.adapter_summaries.values().map(|s| s.total_operations).sum();
        let weighted_success_rate: f64 = self.adapter_summaries.values()
            .map(|s| s.success_rate * s.total_operations as f64)
            .sum();
        
        self.overall_success_rate = if total_operations > 0 {
            weighted_success_rate / total_operations as f64
        } else {
            0.0
        };

        let valid_ratios: Vec<f64> = self.adapter_summaries.values()
            .filter_map(|s| s.performance_ratio)
            .collect();
        
        self.overall_performance_ratio = if !valid_ratios.is_empty() {
            valid_ratios.iter().sum::<f64>() / valid_ratios.len() as f64
        } else {
            0.0
        };
    }
}

/// Migration progress report
#[derive(Debug)]
pub struct MigrationProgressReport {
    pub current_phase: MigrationPhase,
    pub overall_progress: f64, // Percentage
    pub migration_duration: Duration,
    pub estimated_completion: Option<Duration>,
    pub phase_history: Vec<PhaseTransition>,
    pub state_distribution: HashMap<String, usize>,
    pub total_state_transitions: usize,
    pub last_phase_change: SystemTime,
}

/// System health metrics
#[derive(Debug)]
pub struct SystemHealthMetrics {
    pub overall_health_score: f64, // 0.0 to 100.0
    pub adapter_health_scores: HashMap<String, f64>,
    pub active_alerts: Vec<AdapterAlert>,
    pub performance_status: PerformanceStatus,
    pub migration_status: MigrationHealthStatus,
}

impl SystemHealthMetrics {
    pub fn new() -> Self {
        Self {
            overall_health_score: 0.0,
            adapter_health_scores: HashMap::new(),
            active_alerts: Vec::new(),
            performance_status: PerformanceStatus::Unknown,
            migration_status: MigrationHealthStatus::Unknown,
        }
    }

    pub fn add_adapter_health(&mut self, adapter_name: String, summary: AdapterPerformanceSummary) {
        // Calculate health score based on success rate and performance ratio
        let mut health_score = summary.success_rate * 100.0; // Start with success rate
        
        // Adjust based on performance ratio
        if let Some(ratio) = summary.performance_ratio {
            if ratio <= 1.0 {
                // Actor is faster, bonus points
                health_score += (1.0 - ratio) * 20.0;
            } else {
                // Actor is slower, penalty
                health_score -= (ratio - 1.0) * 30.0;
            }
        }
        
        health_score = health_score.max(0.0).min(100.0);
        self.adapter_health_scores.insert(adapter_name, health_score);
    }

    pub fn calculate_overall_health(&mut self) {
        if self.adapter_health_scores.is_empty() {
            return;
        }

        self.overall_health_score = self.adapter_health_scores.values().sum::<f64>() / self.adapter_health_scores.len() as f64;
        
        // Determine performance status
        self.performance_status = if self.overall_health_score >= 90.0 {
            PerformanceStatus::Excellent
        } else if self.overall_health_score >= 75.0 {
            PerformanceStatus::Good
        } else if self.overall_health_score >= 50.0 {
            PerformanceStatus::Fair
        } else {
            PerformanceStatus::Poor
        };

        // Factor in active alerts
        if self.active_alerts.len() > 10 {
            self.performance_status = PerformanceStatus::Poor;
        }
    }
}

/// Comprehensive metrics report
#[derive(Debug)]
pub struct ComprehensiveMetricsReport {
    pub report_timestamp: SystemTime,
    pub collection_duration: Duration,
    pub performance_comparison: PerformanceComparisonReport,
    pub migration_progress: MigrationProgressReport,
    pub system_health: SystemHealthMetrics,
    pub latency_analyses: HashMap<String, LatencyAnalysis>,
    pub recommendations: Vec<OptimizationRecommendation>,
}

/// Optimization recommendation
#[derive(Debug, Clone)]
pub struct OptimizationRecommendation {
    pub adapter_name: String,
    pub recommendation_type: RecommendationType,
    pub priority: RecommendationPriority,
    pub description: String,
    pub estimated_impact: String,
}

/// Recommendation types
#[derive(Debug, Clone)]
pub enum RecommendationType {
    PerformanceOptimization,
    ReliabilityImprovement,
    MigrationAcceleration,
    ResourceOptimization,
}

/// Recommendation priority
#[derive(Debug, Clone)]
pub enum RecommendationPriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Adapter alert
#[derive(Debug, Clone)]
pub struct AdapterAlert {
    pub adapter_name: String,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: SystemTime,
    pub metadata: HashMap<String, String>,
}

/// Alert types
#[derive(Debug, Clone)]
pub enum AlertType {
    OperationFailure,
    PerformanceDegradation,
    HighLatency,
    HighErrorRate,
    MigrationStalled,
}

/// Alert severity levels
#[derive(Debug, Clone)]
pub enum AlertSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Trend analysis
#[derive(Debug, Clone)]
pub struct TrendAnalysis {
    pub adapter_trends: HashMap<String, PerformanceTrend>,
}

impl TrendAnalysis {
    pub fn new() -> Self {
        Self {
            adapter_trends: HashMap::new(),
        }
    }
}

/// Performance trend
#[derive(Debug, Clone)]
pub struct PerformanceTrend {
    pub slope: f64,
    pub direction: TrendDirection,
    pub confidence: f64, // 0.0 to 1.0
    pub recent_average: f64,
}

/// Trend direction
#[derive(Debug, Clone)]
pub enum TrendDirection {
    Improving,
    Stable,
    Degrading,
}

/// Performance status
#[derive(Debug, Clone)]
pub enum PerformanceStatus {
    Excellent,
    Good,
    Fair,
    Poor,
    Unknown,
}

/// Migration health status
#[derive(Debug, Clone)]
pub enum MigrationHealthStatus {
    OnTrack,
    Delayed,
    Stalled,
    Failed,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let config = MetricsConfig::default();
        let _collector = AdapterMetricsCollector::new(config);
    }

    #[tokio::test]
    async fn test_adapter_metrics_storage() {
        let mut storage = AdapterMetricsStorage::new("test_adapter".to_string());
        
        let metrics = AdapterMetrics {
            operation: "test_operation".to_string(),
            legacy_duration: Some(Duration::from_millis(100)),
            actor_duration: Some(Duration::from_millis(120)),
            timestamp: SystemTime::now(),
            success: true,
            error: None,
            metadata: HashMap::new(),
        };
        
        storage.add_metrics(metrics).await;
        
        let summary = storage.get_performance_summary().await;
        assert_eq!(summary.total_operations, 1);
        assert_eq!(summary.success_rate, 1.0);
        assert!(summary.performance_ratio.unwrap() > 1.0); // Actor is slower
    }

    #[tokio::test]
    async fn test_migration_metrics_storage() {
        let mut storage = MigrationMetricsStorage::new();
        
        storage.record_state_transition(
            "test_adapter",
            MigrationState::LegacyOnly,
            MigrationState::DualPathLegacyPreferred
        ).await;
        
        storage.record_phase_transition(
            MigrationPhase::Planning,
            MigrationPhase::GradualRollout,
            Duration::from_secs(60)
        ).await;
        
        let report = storage.get_progress_report().await;
        assert!(matches!(report.current_phase, MigrationPhase::GradualRollout));
        assert!(report.overall_progress > 0.0);
    }

    #[tokio::test]
    async fn test_latency_analysis() {
        let mut storage = AdapterMetricsStorage::new("test_adapter".to_string());
        
        // Add sample metrics with different latencies
        for i in 0..100 {
            let metrics = AdapterMetrics {
                operation: "test_operation".to_string(),
                legacy_duration: Some(Duration::from_millis(100 + i)),
                actor_duration: Some(Duration::from_millis(90 + i)),
                timestamp: SystemTime::now(),
                success: true,
                error: None,
                metadata: HashMap::new(),
            };
            storage.add_metrics(metrics).await;
        }
        
        let analysis = storage.get_latency_analysis().await;
        assert!(analysis.median_improvement > 0.0); // Actor should be faster
        assert!(analysis.legacy_percentiles.p50 > Duration::from_millis(0));
    }

    #[test]
    fn test_performance_trend_calculation() {
        let mut analyzer = PerformanceTrendAnalyzer::new(Duration::from_secs(3600));
        
        // Add improving trend samples
        let base_time = SystemTime::now();
        for i in 0..10 {
            let timestamp = base_time + Duration::from_secs(i * 60);
            let ratio = 2.0 - (i as f64 * 0.1); // Improving trend from 2.0 to 1.0
            analyzer.add_performance_sample("test_adapter", ratio, timestamp);
        }
        
        let trend = analyzer.calculate_linear_trend(
            analyzer.performance_history.get("test_adapter").unwrap()
        );
        
        assert!(matches!(trend.direction, TrendDirection::Improving));
        assert!(trend.slope < 0.0); // Negative slope means improving
    }

    #[tokio::test]
    async fn test_alerting_system() {
        let thresholds = AlertThresholds::default();
        let mut alerting = AdapterAlertingSystem::new(thresholds);
        
        // Create metrics that should trigger an alert
        let bad_metrics = AdapterMetrics {
            operation: "test_operation".to_string(),
            legacy_duration: Some(Duration::from_millis(100)),
            actor_duration: Some(Duration::from_millis(300)), // 3x slower
            timestamp: SystemTime::now(),
            success: false,
            error: Some("Test error".to_string()),
            metadata: HashMap::new(),
        };
        
        let result = alerting.check_metrics("test_adapter", &bad_metrics).await;
        assert!(result.is_err()); // Should trigger an alert
    }
}