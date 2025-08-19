/*!
 * Test Reporting System for Alys V2 Testing Framework
 * 
 * This module provides comprehensive test reporting capabilities including:
 * - Coverage analysis and trending
 * - Performance benchmarking analysis and regression detection
 * - Chaos testing results and system stability metrics
 * - HTML and JSON report generation
 * - Historical trend analysis
 * - Integration with CI/CD pipelines
 */

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs::create_dir_all;
use uuid::Uuid;

use crate::framework::chaos::ChaosTestResult;
use crate::framework::performance::{PerformanceMetrics, BenchmarkResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub id: Uuid,
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub duration_seconds: f64,
    pub summary: TestSummary,
    pub coverage: Option<CoverageReport>,
    pub performance: Option<PerformanceReport>,
    pub chaos: Option<ChaosReport>,
    pub artifacts: Vec<String>,
    pub environment: EnvironmentInfo,
    pub git_info: Option<GitInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub total_tests: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub success_rate: f64,
    pub test_categories: HashMap<String, CategorySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub overall_percentage: f64,
    pub lines_covered: u32,
    pub lines_total: u32,
    pub functions_covered: u32,
    pub functions_total: u32,
    pub branches_covered: u32,
    pub branches_total: u32,
    pub file_coverage: HashMap<String, FileCoverage>,
    pub trend: Option<CoverageTrend>,
    pub threshold_met: bool,
    pub minimum_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCoverage {
    pub file_path: String,
    pub lines_covered: u32,
    pub lines_total: u32,
    pub coverage_percentage: f64,
    pub uncovered_lines: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageTrend {
    pub current: f64,
    pub previous: f64,
    pub change: f64,
    pub trend_direction: TrendDirection,
    pub history: Vec<CoverageDataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageDataPoint {
    pub timestamp: DateTime<Utc>,
    pub coverage_percentage: f64,
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub benchmarks: HashMap<String, BenchmarkSummary>,
    pub regressions: Vec<PerformanceRegression>,
    pub improvements: Vec<PerformanceImprovement>,
    pub trend_analysis: PerformanceTrendAnalysis,
    pub threshold_violations: Vec<ThresholdViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub name: String,
    pub current_value: f64,
    pub unit: String,
    pub baseline: Option<f64>,
    pub change_percentage: Option<f64>,
    pub trend: TrendDirection,
    pub history: Vec<PerformanceDataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceDataPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub commit_hash: Option<String>,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRegression {
    pub benchmark_name: String,
    pub current_value: f64,
    pub baseline_value: f64,
    pub degradation_percentage: f64,
    pub severity: RegressionSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceImprovement {
    pub benchmark_name: String,
    pub current_value: f64,
    pub baseline_value: f64,
    pub improvement_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrendAnalysis {
    pub overall_trend: TrendDirection,
    pub trend_confidence: f64,
    pub key_metrics: HashMap<String, MetricTrend>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricTrend {
    pub metric_name: String,
    pub trend_direction: TrendDirection,
    pub rate_of_change: f64,
    pub stability_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    pub metric_name: String,
    pub current_value: f64,
    pub threshold: f64,
    pub violation_type: ViolationType,
    pub severity: RegressionSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosReport {
    pub experiments_conducted: u32,
    pub experiments_passed: u32,
    pub experiments_failed: u32,
    pub overall_resilience_score: f64,
    pub system_stability_metrics: SystemStabilityMetrics,
    pub fault_categories: HashMap<String, FaultCategoryResult>,
    pub recovery_analysis: RecoveryAnalysis,
    pub recommendations: Vec<ResilienceRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStabilityMetrics {
    pub mean_time_to_failure: f64,
    pub mean_time_to_recovery: f64,
    pub availability_percentage: f64,
    pub error_rate: f64,
    pub throughput_degradation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultCategoryResult {
    pub category: String,
    pub experiments: u32,
    pub success_rate: f64,
    pub avg_recovery_time: f64,
    pub critical_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAnalysis {
    pub fastest_recovery_ms: u64,
    pub slowest_recovery_ms: u64,
    pub median_recovery_ms: u64,
    pub recovery_success_rate: f64,
    pub auto_recovery_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceRecommendation {
    pub category: String,
    pub priority: RecommendationPriority,
    pub description: String,
    pub impact: String,
    pub effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub os: String,
    pub architecture: String,
    pub rust_version: String,
    pub cargo_version: String,
    pub test_environment: String,
    pub docker_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub commit_hash: String,
    pub branch: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Improving,
    Stable,
    Degrading,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegressionSeverity {
    Critical,  // > 50% degradation
    Major,     // 20-50% degradation
    Minor,     // 5-20% degradation
    Negligible, // < 5% degradation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    Exceeds,
    Below,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Critical,
    High,
    Medium,
    Low,
}

pub struct ReportGenerator {
    output_dir: PathBuf,
    artifact_dir: PathBuf,
    minimum_coverage_threshold: f64,
    performance_regression_threshold: f64,
}

impl ReportGenerator {
    pub fn new(
        output_dir: PathBuf,
        artifact_dir: PathBuf,
        minimum_coverage_threshold: f64,
        performance_regression_threshold: f64,
    ) -> Self {
        Self {
            output_dir,
            artifact_dir,
            minimum_coverage_threshold,
            performance_regression_threshold,
        }
    }

    pub async fn generate_comprehensive_report(
        &self,
        test_results: &HashMap<String, TestResult>,
        coverage_data: Option<&CoverageData>,
        performance_data: Option<&[BenchmarkResult]>,
        chaos_results: Option<&[ChaosTestResult]>,
    ) -> Result<TestReport> {
        let report_id = Uuid::new_v4();
        let timestamp = Utc::now();
        
        // Ensure output directories exist
        create_dir_all(&self.output_dir).await?;
        create_dir_all(&self.artifact_dir).await?;
        
        // Generate test summary
        let summary = self.generate_test_summary(test_results)?;
        
        // Generate coverage report
        let coverage = if let Some(coverage_data) = coverage_data {
            Some(self.generate_coverage_report(coverage_data).await?)
        } else {
            None
        };
        
        // Generate performance report
        let performance = if let Some(performance_data) = performance_data {
            Some(self.generate_performance_report(performance_data).await?)
        } else {
            None
        };
        
        // Generate chaos report
        let chaos = if let Some(chaos_results) = chaos_results {
            Some(self.generate_chaos_report(chaos_results)?)
        } else {
            None
        };
        
        // Collect artifacts
        let artifacts = self.collect_artifacts().await?;
        
        // Get environment info
        let environment = self.collect_environment_info().await?;
        
        // Get git info
        let git_info = self.collect_git_info().await.ok();
        
        let report = TestReport {
            id: report_id,
            name: format!("Alys V2 Test Report - {}", timestamp.format("%Y-%m-%d %H:%M:%S UTC")),
            timestamp,
            duration_seconds: self.calculate_total_duration(test_results),
            summary,
            coverage,
            performance,
            chaos,
            artifacts,
            environment,
            git_info,
        };
        
        // Generate HTML report
        self.generate_html_report(&report).await?;
        
        // Generate JSON report
        self.generate_json_report(&report).await?;
        
        Ok(report)
    }

    fn generate_test_summary(&self, test_results: &HashMap<String, TestResult>) -> Result<TestSummary> {
        let mut total_tests = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut test_categories = HashMap::new();
        
        for (category, result) in test_results {
            total_tests += result.total;
            passed += result.passed;
            failed += result.failed;
            skipped += result.skipped;
            
            test_categories.insert(category.clone(), CategorySummary {
                total: result.total,
                passed: result.passed,
                failed: result.failed,
                duration_seconds: result.duration_seconds,
            });
        }
        
        let success_rate = if total_tests > 0 {
            (passed as f64 / total_tests as f64) * 100.0
        } else {
            0.0
        };
        
        Ok(TestSummary {
            total_tests,
            passed,
            failed,
            skipped,
            success_rate,
            test_categories,
        })
    }

    async fn generate_coverage_report(&self, coverage_data: &CoverageData) -> Result<CoverageReport> {
        let overall_percentage = coverage_data.calculate_overall_percentage();
        let threshold_met = overall_percentage >= self.minimum_coverage_threshold;
        
        // Load historical coverage data for trend analysis
        let trend = self.calculate_coverage_trend(overall_percentage).await?;
        
        Ok(CoverageReport {
            overall_percentage,
            lines_covered: coverage_data.lines_covered,
            lines_total: coverage_data.lines_total,
            functions_covered: coverage_data.functions_covered,
            functions_total: coverage_data.functions_total,
            branches_covered: coverage_data.branches_covered,
            branches_total: coverage_data.branches_total,
            file_coverage: coverage_data.file_coverage.clone(),
            trend: Some(trend),
            threshold_met,
            minimum_threshold: self.minimum_coverage_threshold,
        })
    }

    async fn generate_performance_report(&self, benchmark_data: &[BenchmarkResult]) -> Result<PerformanceReport> {
        let mut benchmarks = HashMap::new();
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        let mut threshold_violations = Vec::new();
        
        for result in benchmark_data {
            // Load historical data for this benchmark
            let history = self.load_benchmark_history(&result.name).await?;
            
            let baseline = history.last().map(|h| h.value);
            let change_percentage = if let Some(baseline) = baseline {
                ((result.value - baseline) / baseline) * 100.0
            } else {
                None
            };
            
            let trend = self.calculate_performance_trend(&history, result.value);
            
            // Check for regressions
            if let Some(baseline) = baseline {
                let degradation = ((result.value - baseline) / baseline) * 100.0;
                if degradation > self.performance_regression_threshold {
                    let severity = match degradation {
                        d if d > 50.0 => RegressionSeverity::Critical,
                        d if d > 20.0 => RegressionSeverity::Major,
                        d if d > 5.0 => RegressionSeverity::Minor,
                        _ => RegressionSeverity::Negligible,
                    };
                    
                    regressions.push(PerformanceRegression {
                        benchmark_name: result.name.clone(),
                        current_value: result.value,
                        baseline_value: baseline,
                        degradation_percentage: degradation,
                        severity,
                    });
                } else if degradation < -5.0 { // Improvement
                    improvements.push(PerformanceImprovement {
                        benchmark_name: result.name.clone(),
                        current_value: result.value,
                        baseline_value: baseline,
                        improvement_percentage: -degradation,
                    });
                }
            }
            
            benchmarks.insert(result.name.clone(), BenchmarkSummary {
                name: result.name.clone(),
                current_value: result.value,
                unit: result.unit.clone(),
                baseline,
                change_percentage,
                trend: trend.clone(),
                history,
            });
        }
        
        let trend_analysis = self.analyze_performance_trends(&benchmarks)?;
        
        Ok(PerformanceReport {
            benchmarks,
            regressions,
            improvements,
            trend_analysis,
            threshold_violations,
        })
    }

    fn generate_chaos_report(&self, chaos_results: &[ChaosTestResult]) -> Result<ChaosReport> {
        let total_experiments = chaos_results.len() as u32;
        let passed_experiments = chaos_results.iter()
            .filter(|r| r.success)
            .count() as u32;
        let failed_experiments = total_experiments - passed_experiments;
        
        let overall_resilience_score = if total_experiments > 0 {
            (passed_experiments as f64 / total_experiments as f64) * 100.0
        } else {
            0.0
        };
        
        // Calculate system stability metrics
        let recovery_times: Vec<f64> = chaos_results.iter()
            .filter_map(|r| r.recovery_time_ms.map(|t| t as f64))
            .collect();
        
        let mean_recovery_time = if !recovery_times.is_empty() {
            recovery_times.iter().sum::<f64>() / recovery_times.len() as f64
        } else {
            0.0
        };
        
        let system_stability_metrics = SystemStabilityMetrics {
            mean_time_to_failure: self.calculate_mttf(chaos_results),
            mean_time_to_recovery: mean_recovery_time,
            availability_percentage: overall_resilience_score,
            error_rate: (failed_experiments as f64 / total_experiments as f64) * 100.0,
            throughput_degradation: self.calculate_throughput_degradation(chaos_results),
        };
        
        // Group by fault categories
        let mut fault_categories = HashMap::new();
        for result in chaos_results {
            let entry = fault_categories
                .entry(result.fault_type.clone())
                .or_insert(FaultCategoryResult {
                    category: result.fault_type.clone(),
                    experiments: 0,
                    success_rate: 0.0,
                    avg_recovery_time: 0.0,
                    critical_failures: 0,
                });
            
            entry.experiments += 1;
            if result.success {
                entry.success_rate += 1.0;
            }
            if result.severity == "critical" {
                entry.critical_failures += 1;
            }
            if let Some(recovery_time) = result.recovery_time_ms {
                entry.avg_recovery_time += recovery_time as f64;
            }
        }
        
        // Calculate success rates and averages
        for category_result in fault_categories.values_mut() {
            category_result.success_rate = (category_result.success_rate / category_result.experiments as f64) * 100.0;
            category_result.avg_recovery_time /= category_result.experiments as f64;
        }
        
        let recovery_analysis = self.analyze_recovery_patterns(chaos_results);
        let recommendations = self.generate_resilience_recommendations(chaos_results, &system_stability_metrics);
        
        Ok(ChaosReport {
            experiments_conducted: total_experiments,
            experiments_passed: passed_experiments,
            experiments_failed: failed_experiments,
            overall_resilience_score,
            system_stability_metrics,
            fault_categories,
            recovery_analysis,
            recommendations,
        })
    }

    async fn generate_html_report(&self, report: &TestReport) -> Result<()> {
        let html_content = self.render_html_template(report)?;
        let html_path = self.output_dir.join(format!("report_{}.html", report.id));
        tokio::fs::write(&html_path, html_content).await?;
        
        // Also create an index.html that points to the latest report
        let index_content = self.render_index_template(report)?;
        let index_path = self.output_dir.join("index.html");
        tokio::fs::write(&index_path, index_content).await?;
        
        Ok(())
    }

    async fn generate_json_report(&self, report: &TestReport) -> Result<()> {
        let json_content = serde_json::to_string_pretty(report)?;
        let json_path = self.output_dir.join(format!("report_{}.json", report.id));
        tokio::fs::write(&json_path, json_content).await?;
        Ok(())
    }

    // Helper methods for calculations and analysis
    
    fn calculate_total_duration(&self, test_results: &HashMap<String, TestResult>) -> f64 {
        test_results.values().map(|r| r.duration_seconds).sum()
    }

    async fn calculate_coverage_trend(&self, current_coverage: f64) -> Result<CoverageTrend> {
        // Load historical coverage data
        let history = self.load_coverage_history().await?;
        let previous = history.last().map(|h| h.coverage_percentage).unwrap_or(current_coverage);
        let change = current_coverage - previous;
        
        let trend_direction = match change {
            c if c > 1.0 => TrendDirection::Improving,
            c if c < -1.0 => TrendDirection::Degrading,
            _ => TrendDirection::Stable,
        };
        
        Ok(CoverageTrend {
            current: current_coverage,
            previous,
            change,
            trend_direction,
            history,
        })
    }

    async fn load_coverage_history(&self) -> Result<Vec<CoverageDataPoint>> {
        // Implementation would load from database or files
        // For now, return empty history
        Ok(Vec::new())
    }

    async fn load_benchmark_history(&self, benchmark_name: &str) -> Result<Vec<PerformanceDataPoint>> {
        // Implementation would load from database or files
        // For now, return empty history
        Ok(Vec::new())
    }

    fn calculate_performance_trend(&self, history: &[PerformanceDataPoint], current_value: f64) -> TrendDirection {
        if history.len() < 2 {
            return TrendDirection::Unknown;
        }
        
        let recent_values: Vec<f64> = history.iter().rev().take(5).map(|p| p.value).collect();
        let slope = self.calculate_linear_regression_slope(&recent_values);
        
        match slope {
            s if s > 0.05 => TrendDirection::Improving,
            s if s < -0.05 => TrendDirection::Degrading,
            _ => TrendDirection::Stable,
        }
    }

    fn calculate_linear_regression_slope(&self, values: &[f64]) -> f64 {
        if values.len() < 2 {
            return 0.0;
        }
        
        let n = values.len() as f64;
        let x_sum: f64 = (0..values.len()).map(|i| i as f64).sum();
        let y_sum: f64 = values.iter().sum();
        let xy_sum: f64 = values.iter().enumerate().map(|(i, &y)| i as f64 * y).sum();
        let x_squared_sum: f64 = (0..values.len()).map(|i| (i as f64).powi(2)).sum();
        
        (n * xy_sum - x_sum * y_sum) / (n * x_squared_sum - x_sum.powi(2))
    }

    fn analyze_performance_trends(&self, benchmarks: &HashMap<String, BenchmarkSummary>) -> Result<PerformanceTrendAnalysis> {
        let improving_count = benchmarks.values()
            .filter(|b| matches!(b.trend, TrendDirection::Improving))
            .count();
        let degrading_count = benchmarks.values()
            .filter(|b| matches!(b.trend, TrendDirection::Degrading))
            .count();
        
        let overall_trend = match (improving_count, degrading_count) {
            (i, d) if i > d => TrendDirection::Improving,
            (i, d) if d > i => TrendDirection::Degrading,
            _ => TrendDirection::Stable,
        };
        
        let trend_confidence = (improving_count as f64 + degrading_count as f64) / benchmarks.len() as f64;
        
        let key_metrics = benchmarks.iter()
            .map(|(name, summary)| {
                (name.clone(), MetricTrend {
                    metric_name: name.clone(),
                    trend_direction: summary.trend.clone(),
                    rate_of_change: summary.change_percentage.unwrap_or(0.0),
                    stability_score: self.calculate_stability_score(&summary.history),
                })
            })
            .collect();
        
        Ok(PerformanceTrendAnalysis {
            overall_trend,
            trend_confidence,
            key_metrics,
        })
    }

    fn calculate_stability_score(&self, history: &[PerformanceDataPoint]) -> f64 {
        if history.len() < 2 {
            return 100.0;
        }
        
        let values: Vec<f64> = history.iter().map(|p| p.value).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();
        
        // Convert coefficient of variation to stability score (inverted)
        let cv = std_dev / mean;
        ((1.0 - cv.min(1.0)) * 100.0).max(0.0)
    }

    fn calculate_mttf(&self, chaos_results: &[ChaosTestResult]) -> f64 {
        // Calculate Mean Time To Failure based on chaos test results
        let failure_intervals: Vec<f64> = chaos_results.iter()
            .filter(|r| !r.success)
            .filter_map(|r| r.failure_time_ms.map(|t| t as f64))
            .collect();
        
        if failure_intervals.is_empty() {
            return f64::INFINITY; // No failures observed
        }
        
        failure_intervals.iter().sum::<f64>() / failure_intervals.len() as f64
    }

    fn calculate_throughput_degradation(&self, chaos_results: &[ChaosTestResult]) -> f64 {
        // Calculate average throughput degradation during chaos tests
        let degradations: Vec<f64> = chaos_results.iter()
            .filter_map(|r| r.performance_impact.as_ref())
            .filter_map(|impact| impact.get("throughput_degradation_percent"))
            .filter_map(|v| v.as_f64())
            .collect();
        
        if degradations.is_empty() {
            return 0.0;
        }
        
        degradations.iter().sum::<f64>() / degradations.len() as f64
    }

    fn analyze_recovery_patterns(&self, chaos_results: &[ChaosTestResult]) -> RecoveryAnalysis {
        let recovery_times: Vec<u64> = chaos_results.iter()
            .filter_map(|r| r.recovery_time_ms)
            .collect();
        
        if recovery_times.is_empty() {
            return RecoveryAnalysis {
                fastest_recovery_ms: 0,
                slowest_recovery_ms: 0,
                median_recovery_ms: 0,
                recovery_success_rate: 0.0,
                auto_recovery_rate: 0.0,
            };
        }
        
        let mut sorted_times = recovery_times.clone();
        sorted_times.sort();
        
        let fastest = *sorted_times.first().unwrap_or(&0);
        let slowest = *sorted_times.last().unwrap_or(&0);
        let median = sorted_times[sorted_times.len() / 2];
        
        let successful_recoveries = chaos_results.iter()
            .filter(|r| r.recovery_time_ms.is_some())
            .count();
        let recovery_success_rate = (successful_recoveries as f64 / chaos_results.len() as f64) * 100.0;
        
        let auto_recoveries = chaos_results.iter()
            .filter(|r| r.auto_recovery.unwrap_or(false))
            .count();
        let auto_recovery_rate = (auto_recoveries as f64 / chaos_results.len() as f64) * 100.0;
        
        RecoveryAnalysis {
            fastest_recovery_ms: fastest,
            slowest_recovery_ms: slowest,
            median_recovery_ms: median,
            recovery_success_rate,
            auto_recovery_rate,
        }
    }

    fn generate_resilience_recommendations(
        &self,
        chaos_results: &[ChaosTestResult],
        stability_metrics: &SystemStabilityMetrics,
    ) -> Vec<ResilienceRecommendation> {
        let mut recommendations = Vec::new();
        
        // Analyze failure patterns and generate recommendations
        if stability_metrics.availability_percentage < 99.0 {
            recommendations.push(ResilienceRecommendation {
                category: "Availability".to_string(),
                priority: RecommendationPriority::Critical,
                description: "System availability is below 99%. Implement redundancy and failover mechanisms.".to_string(),
                impact: "High - affects user experience and system reliability".to_string(),
                effort: "Medium - requires architecture changes".to_string(),
            });
        }
        
        if stability_metrics.mean_time_to_recovery > 60000.0 { // > 1 minute
            recommendations.push(ResilienceRecommendation {
                category: "Recovery Time".to_string(),
                priority: RecommendationPriority::High,
                description: "Mean time to recovery exceeds 1 minute. Implement faster detection and automated recovery.".to_string(),
                impact: "Medium - extends downtime during failures".to_string(),
                effort: "Medium - requires monitoring and automation improvements".to_string(),
            });
        }
        
        if stability_metrics.error_rate > 5.0 {
            recommendations.push(ResilienceRecommendation {
                category: "Error Handling".to_string(),
                priority: RecommendationPriority::High,
                description: "Error rate exceeds 5%. Improve error handling and fault tolerance.".to_string(),
                impact: "Medium - affects system stability".to_string(),
                effort: "Low to Medium - code improvements and better error handling".to_string(),
            });
        }
        
        recommendations
    }

    async fn collect_artifacts(&self) -> Result<Vec<String>> {
        let mut artifacts = Vec::new();
        
        // Collect various test artifacts
        if let Ok(entries) = fs::read_dir(&self.artifact_dir) {
            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    artifacts.push(file_name);
                }
            }
        }
        
        Ok(artifacts)
    }

    async fn collect_environment_info(&self) -> Result<EnvironmentInfo> {
        Ok(EnvironmentInfo {
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            rust_version: self.get_rust_version().await.unwrap_or_else(|| "unknown".to_string()),
            cargo_version: self.get_cargo_version().await.unwrap_or_else(|| "unknown".to_string()),
            test_environment: "docker".to_string(),
            docker_version: self.get_docker_version().await,
        })
    }

    async fn get_rust_version(&self) -> Option<String> {
        Command::new("rustc")
            .arg("--version")
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    async fn get_cargo_version(&self) -> Option<String> {
        Command::new("cargo")
            .arg("--version")
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    async fn get_docker_version(&self) -> Option<String> {
        Command::new("docker")
            .arg("--version")
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    async fn collect_git_info(&self) -> Result<GitInfo> {
        let commit_hash = self.get_git_commit_hash().await?;
        let branch = self.get_git_branch().await?;
        let author = self.get_git_author().await?;
        let timestamp = self.get_git_timestamp().await?;
        let message = self.get_git_message().await?;
        
        Ok(GitInfo {
            commit_hash,
            branch,
            author,
            timestamp,
            message,
        })
    }

    async fn get_git_commit_hash(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()?;
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    async fn get_git_branch(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()?;
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    async fn get_git_author(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["log", "-1", "--pretty=format:%an"])
            .output()?;
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    async fn get_git_timestamp(&self) -> Result<DateTime<Utc>> {
        let output = Command::new("git")
            .args(["log", "-1", "--pretty=format:%ct"])
            .output()?;
        let timestamp_str = String::from_utf8(output.stdout)?.trim();
        let timestamp: i64 = timestamp_str.parse()?;
        Ok(DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now))
    }

    async fn get_git_message(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["log", "-1", "--pretty=format:%s"])
            .output()?;
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    fn render_html_template(&self, report: &TestReport) -> Result<String> {
        // This would use a proper template engine like Tera or handlebars
        // For now, return a simple HTML template
        let html = format!(
            include_str!("../templates/report_template.html"),
            report_id = report.id,
            report_name = report.name,
            timestamp = report.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            duration = report.duration_seconds,
            total_tests = report.summary.total_tests,
            passed_tests = report.summary.passed,
            failed_tests = report.summary.failed,
            success_rate = report.summary.success_rate,
            coverage_percentage = report.coverage.as_ref().map(|c| c.overall_percentage).unwrap_or(0.0),
            performance_summary = self.render_performance_summary(&report.performance),
            chaos_summary = self.render_chaos_summary(&report.chaos),
        );
        
        Ok(html)
    }

    fn render_performance_summary(&self, performance: &Option<PerformanceReport>) -> String {
        match performance {
            Some(perf) => format!(
                "Benchmarks: {}, Regressions: {}, Improvements: {}",
                perf.benchmarks.len(),
                perf.regressions.len(),
                perf.improvements.len()
            ),
            None => "No performance data available".to_string(),
        }
    }

    fn render_chaos_summary(&self, chaos: &Option<ChaosReport>) -> String {
        match chaos {
            Some(chaos) => format!(
                "Experiments: {}, Success Rate: {:.1}%, Resilience Score: {:.1}%",
                chaos.experiments_conducted,
                (chaos.experiments_passed as f64 / chaos.experiments_conducted as f64) * 100.0,
                chaos.overall_resilience_score
            ),
            None => "No chaos testing data available".to_string(),
        }
    }

    fn render_index_template(&self, report: &TestReport) -> Result<String> {
        let html = format!(
            r#"
            <!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>Alys V2 Test Reports</title>
                <style>
                    body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 0; padding: 20px; background: #f5f5f5; }}
                    .container {{ max-width: 1200px; margin: 0 auto; }}
                    .header {{ background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); margin-bottom: 20px; }}
                    .latest-report {{ background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
                    .metrics {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 15px; margin: 20px 0; }}
                    .metric {{ background: #f8f9fa; padding: 15px; border-radius: 5px; text-align: center; }}
                    .metric h3 {{ margin: 0 0 10px 0; color: #495057; }}
                    .metric .value {{ font-size: 24px; font-weight: bold; color: #007bff; }}
                    .success {{ color: #28a745; }}
                    .warning {{ color: #ffc107; }}
                    .danger {{ color: #dc3545; }}
                </style>
            </head>
            <body>
                <div class="container">
                    <div class="header">
                        <h1>Alys V2 Testing Framework</h1>
                        <p>Comprehensive testing results and analysis</p>
                    </div>
                    
                    <div class="latest-report">
                        <h2>Latest Test Report</h2>
                        <p><strong>Report ID:</strong> {}</p>
                        <p><strong>Generated:</strong> {}</p>
                        <p><strong>Duration:</strong> {:.2} seconds</p>
                        
                        <div class="metrics">
                            <div class="metric">
                                <h3>Total Tests</h3>
                                <div class="value">{}</div>
                            </div>
                            <div class="metric">
                                <h3>Success Rate</h3>
                                <div class="value {}">{:.1}%</div>
                            </div>
                            <div class="metric">
                                <h3>Coverage</h3>
                                <div class="value {}">{:.1}%</div>
                            </div>
                            <div class="metric">
                                <h3>Performance</h3>
                                <div class="value">{}</div>
                            </div>
                        </div>
                        
                        <p><a href="report_{}.html" style="background: #007bff; color: white; padding: 10px 20px; text-decoration: none; border-radius: 5px;">View Full Report</a></p>
                    </div>
                </div>
            </body>
            </html>
            "#,
            report.id,
            report.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            report.duration_seconds,
            report.summary.total_tests,
            if report.summary.success_rate >= 95.0 { "success" } else if report.summary.success_rate >= 80.0 { "warning" } else { "danger" },
            report.summary.success_rate,
            if report.coverage.as_ref().map(|c| c.overall_percentage).unwrap_or(0.0) >= 80.0 { "success" } else { "warning" },
            report.coverage.as_ref().map(|c| c.overall_percentage).unwrap_or(0.0),
            self.render_performance_summary(&report.performance),
            report.id
        );
        
        Ok(html)
    }
}

// Supporting data structures

#[derive(Debug, Clone)]
pub struct TestResult {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct CoverageData {
    pub lines_covered: u32,
    pub lines_total: u32,
    pub functions_covered: u32,
    pub functions_total: u32,
    pub branches_covered: u32,
    pub branches_total: u32,
    pub file_coverage: HashMap<String, FileCoverage>,
}

impl CoverageData {
    pub fn calculate_overall_percentage(&self) -> f64 {
        if self.lines_total == 0 {
            return 0.0;
        }
        (self.lines_covered as f64 / self.lines_total as f64) * 100.0
    }
}