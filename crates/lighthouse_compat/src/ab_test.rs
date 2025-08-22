//! A/B Testing Framework for Lighthouse V4/V5 Migration
//! 
//! This module provides a comprehensive A/B testing framework for safely migrating
//! between Lighthouse versions. It supports sticky sessions, statistical analysis,
//! and automated decision making based on performance metrics.

use crate::config::{ABTestingConfig, ABTestGroup, TrafficSplitStrategy};
use crate::error::{CompatError, CompatResult};
use crate::metrics::ABTestMetrics;
use actix::prelude::*;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A/B Testing Controller for Lighthouse version migration
#[derive(Clone)]
pub struct ABTestController {
    config: ABTestingConfig,
    active_tests: Arc<RwLock<HashMap<String, ActiveTest>>>,
    session_manager: Arc<SessionManager>,
    metrics_collector: Arc<ABTestMetrics>,
    decision_engine: Arc<DecisionEngine>,
}

/// Active A/B test configuration and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub groups: Vec<ABTestGroup>,
    pub traffic_split: TrafficSplitStrategy,
    pub started_at: DateTime<Utc>,
    pub duration: Duration,
    pub status: TestStatus,
    pub metadata: HashMap<String, String>,
    pub statistical_config: StatisticalConfig,
}

/// A/B test status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestStatus {
    Active,
    Paused,
    Completed,
    Stopped,
    Failed,
}

/// Statistical configuration for A/B tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalConfig {
    pub confidence_level: f64,
    pub statistical_power: f64,
    pub minimum_sample_size: u64,
    pub effect_size: f64,
    pub significance_threshold: f64,
    pub early_stopping_enabled: bool,
    pub sequential_testing: bool,
}

/// Session management for sticky A/B testing
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, UserSession>>>,
    session_timeout: Duration,
    cleanup_interval: Duration,
}

/// User session for sticky A/B testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub session_id: String,
    pub user_id: Option<String>,
    pub assigned_group: String,
    pub test_id: String,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
    pub lighthouse_version: String,
}

/// Statistical decision engine for A/B tests
pub struct DecisionEngine {
    statistical_models: Arc<RwLock<HashMap<String, StatisticalModel>>>,
    bayesian_engine: BayesianEngine,
    frequentist_engine: FrequentistEngine,
}

/// Statistical model for A/B test analysis
#[derive(Debug, Clone)]
pub struct StatisticalModel {
    pub model_type: ModelType,
    pub parameters: HashMap<String, f64>,
    pub confidence_intervals: HashMap<String, (f64, f64)>,
    pub p_values: HashMap<String, f64>,
    pub effect_sizes: HashMap<String, f64>,
}

/// Statistical model types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    Frequentist,
    Bayesian,
    Sequential,
    MultiArmed,
}

/// Test assignment result
#[derive(Debug, Clone)]
pub struct TestAssignment {
    pub test_id: String,
    pub group_id: String,
    pub session_id: String,
    pub lighthouse_version: String,
    pub assignment_reason: AssignmentReason,
    pub metadata: HashMap<String, String>,
}

/// Reason for test assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssignmentReason {
    NewUser,
    StickySession,
    Override,
    Default,
    Rollback,
}

/// Test results and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    pub test_id: String,
    pub groups: Vec<GroupResults>,
    pub statistical_significance: bool,
    pub confidence_level: f64,
    pub p_value: f64,
    pub effect_size: f64,
    pub winner: Option<String>,
    pub recommendation: TestRecommendation,
    pub analysis_timestamp: DateTime<Utc>,
}

/// Results for a specific test group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupResults {
    pub group_id: String,
    pub lighthouse_version: String,
    pub sample_size: u64,
    pub conversion_rate: f64,
    pub confidence_interval: (f64, f64),
    pub metrics: HashMap<String, MetricResult>,
}

/// Individual metric result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    pub name: String,
    pub value: f64,
    pub confidence_interval: (f64, f64),
    pub improvement: Option<f64>,
    pub significance: bool,
}

/// Test recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestRecommendation {
    ContinueTest,
    StopAndRollout(String),
    StopAndRollback,
    IncreaseTraffic(String, f64),
    DecreaseTraffic(String, f64),
}

/// Bayesian statistical engine
pub struct BayesianEngine {
    priors: Arc<RwLock<HashMap<String, BayesianPrior>>>,
}

/// Bayesian prior distribution
#[derive(Debug, Clone)]
pub struct BayesianPrior {
    pub distribution_type: DistributionType,
    pub parameters: HashMap<String, f64>,
    pub confidence: f64,
}

/// Statistical distribution types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistributionType {
    Beta,
    Normal,
    Gamma,
    Uniform,
}

/// Frequentist statistical engine
pub struct FrequentistEngine {
    test_configs: Arc<RwLock<HashMap<String, FrequentistConfig>>>,
}

/// Frequentist test configuration
#[derive(Debug, Clone)]
pub struct FrequentistConfig {
    pub test_type: FrequentistTestType,
    pub alpha: f64,
    pub power: f64,
    pub two_tailed: bool,
}

/// Frequentist test types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FrequentistTestType {
    TTest,
    ChiSquare,
    WelchTest,
    MannWhitney,
}

impl ABTestController {
    /// Create a new A/B test controller
    pub fn new(
        config: ABTestingConfig,
        metrics_collector: Arc<ABTestMetrics>,
    ) -> CompatResult<Self> {
        let session_manager = Arc::new(SessionManager::new(
            Duration::hours(24), // 24-hour session timeout
            Duration::minutes(30), // Cleanup every 30 minutes
        )?);
        
        let decision_engine = Arc::new(DecisionEngine::new()?);
        
        Ok(Self {
            config,
            active_tests: Arc::new(RwLock::new(HashMap::new())),
            session_manager,
            metrics_collector,
            decision_engine,
        })
    }

    /// Start a new A/B test
    pub async fn start_test(&self, test_config: ActiveTest) -> CompatResult<String> {
        // Validate test configuration
        self.validate_test_config(&test_config)?;
        
        // Check for conflicting tests
        self.check_test_conflicts(&test_config).await?;
        
        // Initialize statistical models
        self.decision_engine.initialize_models(&test_config).await?;
        
        // Store active test
        let mut active_tests = self.active_tests.write().await;
        active_tests.insert(test_config.id.clone(), test_config.clone());
        
        // Log test start
        tracing::info!(
            test_id = %test_config.id,
            test_name = %test_config.name,
            groups = ?test_config.groups.len(),
            "Started A/B test"
        );
        
        // Update metrics
        self.metrics_collector.record_test_started(&test_config).await;
        
        Ok(test_config.id)
    }

    /// Assign user to test group
    pub async fn assign_user(
        &self,
        test_id: &str,
        user_identifier: Option<&str>,
        session_id: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> CompatResult<TestAssignment> {
        let active_tests = self.active_tests.read().await;
        let test = active_tests.get(test_id)
            .ok_or_else(|| CompatError::ABTestNotFound { test_id: test_id.to_string() })?;

        // Check if user has existing session
        if let Some(sid) = session_id {
            if let Some(existing_session) = self.session_manager.get_session(sid).await? {
                if existing_session.test_id == test_id {
                    return Ok(TestAssignment {
                        test_id: test_id.to_string(),
                        group_id: existing_session.assigned_group.clone(),
                        session_id: existing_session.session_id.clone(),
                        lighthouse_version: existing_session.lighthouse_version.clone(),
                        assignment_reason: AssignmentReason::StickySession,
                        metadata: existing_session.metadata.clone(),
                    });
                }
            }
        }

        // Assign to new group based on traffic split strategy
        let assignment = self.assign_to_group(test, user_identifier, metadata).await?;
        
        // Create session
        let session = UserSession {
            session_id: assignment.session_id.clone(),
            user_id: user_identifier.map(|s| s.to_string()),
            assigned_group: assignment.group_id.clone(),
            test_id: test_id.to_string(),
            created_at: Utc::now(),
            last_activity: Utc::now(),
            metadata: assignment.metadata.clone(),
            lighthouse_version: assignment.lighthouse_version.clone(),
        };
        
        self.session_manager.create_session(session).await?;
        
        // Update metrics
        self.metrics_collector.record_user_assignment(&assignment).await;
        
        Ok(assignment)
    }

    /// Get test results and statistical analysis
    pub async fn get_test_results(&self, test_id: &str) -> CompatResult<TestResults> {
        let active_tests = self.active_tests.read().await;
        let test = active_tests.get(test_id)
            .ok_or_else(|| CompatError::ABTestNotFound { test_id: test_id.to_string() })?;

        // Get metrics for all groups
        let group_results = self.collect_group_results(test).await?;
        
        // Perform statistical analysis
        let analysis = self.decision_engine.analyze_results(test, &group_results).await?;
        
        Ok(TestResults {
            test_id: test_id.to_string(),
            groups: group_results,
            statistical_significance: analysis.is_significant,
            confidence_level: test.statistical_config.confidence_level,
            p_value: analysis.p_value,
            effect_size: analysis.effect_size,
            winner: analysis.winner,
            recommendation: analysis.recommendation,
            analysis_timestamp: Utc::now(),
        })
    }

    /// Stop an active test
    pub async fn stop_test(&self, test_id: &str, reason: String) -> CompatResult<()> {
        let mut active_tests = self.active_tests.write().await;
        if let Some(mut test) = active_tests.get_mut(test_id) {
            test.status = TestStatus::Stopped;
            
            tracing::info!(
                test_id = %test_id,
                reason = %reason,
                "Stopped A/B test"
            );
            
            // Clean up sessions
            self.session_manager.cleanup_test_sessions(test_id).await?;
            
            // Update metrics
            self.metrics_collector.record_test_stopped(test_id, &reason).await;
        }
        
        Ok(())
    }

    /// Validate test configuration
    fn validate_test_config(&self, test: &ActiveTest) -> CompatResult<()> {
        // Validate groups
        if test.groups.is_empty() {
            return Err(CompatError::ABTestInvalidConfig {
                reason: "Test must have at least one group".to_string(),
            });
        }
        
        // Validate traffic split
        let total_traffic: f64 = test.groups.iter().map(|g| g.traffic_percentage).sum();
        if (total_traffic - 100.0).abs() > 0.01 {
            return Err(CompatError::ABTestInvalidConfig {
                reason: format!("Traffic split must sum to 100%, got {}", total_traffic),
            });
        }
        
        // Validate statistical configuration
        if test.statistical_config.confidence_level < 0.5 || test.statistical_config.confidence_level > 0.999 {
            return Err(CompatError::ABTestInvalidConfig {
                reason: "Confidence level must be between 0.5 and 0.999".to_string(),
            });
        }
        
        Ok(())
    }

    /// Check for conflicting tests
    async fn check_test_conflicts(&self, new_test: &ActiveTest) -> CompatResult<()> {
        let active_tests = self.active_tests.read().await;
        
        for (_, existing_test) in active_tests.iter() {
            if existing_test.status == TestStatus::Active {
                // Check for overlapping lighthouse versions
                let new_versions: std::collections::HashSet<_> = new_test.groups.iter()
                    .map(|g| &g.lighthouse_version)
                    .collect();
                let existing_versions: std::collections::HashSet<_> = existing_test.groups.iter()
                    .map(|g| &g.lighthouse_version)
                    .collect();
                
                if !new_versions.is_disjoint(&existing_versions) {
                    return Err(CompatError::ABTestConflict {
                        existing_test: existing_test.id.clone(),
                        new_test: new_test.id.clone(),
                        reason: "Overlapping lighthouse versions".to_string(),
                    });
                }
            }
        }
        
        Ok(())
    }

    /// Assign user to test group based on strategy
    async fn assign_to_group(
        &self,
        test: &ActiveTest,
        user_identifier: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> CompatResult<TestAssignment> {
        let session_id = Uuid::new_v4().to_string();
        
        let assigned_group = match &test.traffic_split {
            TrafficSplitStrategy::Random => {
                self.random_assignment(&test.groups)
            },
            TrafficSplitStrategy::Hash => {
                let hash_input = user_identifier.unwrap_or(&session_id);
                self.hash_assignment(&test.groups, hash_input)
            },
            TrafficSplitStrategy::Weighted => {
                self.weighted_assignment(&test.groups)
            },
        };
        
        let group = test.groups.iter()
            .find(|g| g.id == assigned_group)
            .ok_or_else(|| CompatError::ABTestInvalidConfig {
                reason: format!("Group not found: {}", assigned_group),
            })?;
        
        Ok(TestAssignment {
            test_id: test.id.clone(),
            group_id: assigned_group,
            session_id,
            lighthouse_version: group.lighthouse_version.clone(),
            assignment_reason: AssignmentReason::NewUser,
            metadata: metadata.unwrap_or_default(),
        })
    }

    /// Random assignment to test group
    fn random_assignment(&self, groups: &[ABTestGroup]) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_value: f64 = rng.gen();
        
        let mut cumulative = 0.0;
        for group in groups {
            cumulative += group.traffic_percentage / 100.0;
            if random_value <= cumulative {
                return group.id.clone();
            }
        }
        
        // Fallback to first group
        groups[0].id.clone()
    }

    /// Hash-based assignment for consistent user experience
    fn hash_assignment(&self, groups: &[ABTestGroup], hash_input: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        hash_input.hash(&mut hasher);
        let hash_value = hasher.finish();
        
        let normalized_value = (hash_value % 10000) as f64 / 10000.0;
        
        let mut cumulative = 0.0;
        for group in groups {
            cumulative += group.traffic_percentage / 100.0;
            if normalized_value <= cumulative {
                return group.id.clone();
            }
        }
        
        // Fallback to first group
        groups[0].id.clone()
    }

    /// Weighted assignment based on group configuration
    fn weighted_assignment(&self, groups: &[ABTestGroup]) -> String {
        // For now, same as random assignment
        // Could be enhanced with dynamic weighting based on performance
        self.random_assignment(groups)
    }

    /// Collect results for all test groups
    async fn collect_group_results(&self, test: &ActiveTest) -> CompatResult<Vec<GroupResults>> {
        let mut results = Vec::new();
        
        for group in &test.groups {
            let group_metrics = self.metrics_collector
                .get_group_metrics(&test.id, &group.id)
                .await?;
            
            results.push(GroupResults {
                group_id: group.id.clone(),
                lighthouse_version: group.lighthouse_version.clone(),
                sample_size: group_metrics.sample_size,
                conversion_rate: group_metrics.conversion_rate,
                confidence_interval: group_metrics.confidence_interval,
                metrics: group_metrics.detailed_metrics,
            });
        }
        
        Ok(results)
    }
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(session_timeout: Duration, cleanup_interval: Duration) -> CompatResult<Self> {
        let manager = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_timeout,
            cleanup_interval,
        };
        
        // Start cleanup task
        manager.start_cleanup_task()?;
        
        Ok(manager)
    }

    /// Get existing session
    pub async fn get_session(&self, session_id: &str) -> CompatResult<Option<UserSession>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    /// Create new session
    pub async fn create_session(&self, session: UserSession) -> CompatResult<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.session_id.clone(), session);
        Ok(())
    }

    /// Update session activity
    pub async fn update_activity(&self, session_id: &str) -> CompatResult<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity = Utc::now();
        }
        Ok(())
    }

    /// Clean up sessions for a specific test
    pub async fn cleanup_test_sessions(&self, test_id: &str) -> CompatResult<()> {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, session| session.test_id != test_id);
        Ok(())
    }

    /// Start background cleanup task
    fn start_cleanup_task(&self) -> CompatResult<()> {
        let sessions = Arc::clone(&self.sessions);
        let timeout = self.session_timeout;
        let interval = self.cleanup_interval;
        
        actix::spawn(async move {
            let mut cleanup_interval = tokio::time::interval(interval.to_std().unwrap());
            
            loop {
                cleanup_interval.tick().await;
                
                let now = Utc::now();
                let mut sessions = sessions.write().await;
                let initial_count = sessions.len();
                
                sessions.retain(|_, session| {
                    now.signed_duration_since(session.last_activity) < timeout
                });
                
                let cleaned_count = initial_count - sessions.len();
                if cleaned_count > 0 {
                    tracing::debug!(
                        cleaned_sessions = cleaned_count,
                        remaining_sessions = sessions.len(),
                        "Cleaned up expired A/B test sessions"
                    );
                }
            }
        });
        
        Ok(())
    }
}

impl DecisionEngine {
    /// Create a new decision engine
    pub fn new() -> CompatResult<Self> {
        Ok(Self {
            statistical_models: Arc::new(RwLock::new(HashMap::new())),
            bayesian_engine: BayesianEngine::new()?,
            frequentist_engine: FrequentistEngine::new()?,
        })
    }

    /// Initialize statistical models for a test
    pub async fn initialize_models(&self, test: &ActiveTest) -> CompatResult<()> {
        let model = match test.statistical_config.sequential_testing {
            true => StatisticalModel {
                model_type: ModelType::Sequential,
                parameters: HashMap::new(),
                confidence_intervals: HashMap::new(),
                p_values: HashMap::new(),
                effect_sizes: HashMap::new(),
            },
            false => StatisticalModel {
                model_type: ModelType::Frequentist,
                parameters: HashMap::new(),
                confidence_intervals: HashMap::new(),
                p_values: HashMap::new(),
                effect_sizes: HashMap::new(),
            },
        };
        
        let mut models = self.statistical_models.write().await;
        models.insert(test.id.clone(), model);
        
        Ok(())
    }

    /// Analyze test results
    pub async fn analyze_results(
        &self,
        test: &ActiveTest,
        group_results: &[GroupResults],
    ) -> CompatResult<StatisticalAnalysis> {
        if group_results.len() < 2 {
            return Err(CompatError::ABTestInsufficientData {
                reason: "Need at least 2 groups for comparison".to_string(),
            });
        }

        // Perform statistical test based on configuration
        let analysis = match test.statistical_config.sequential_testing {
            true => self.sequential_analysis(test, group_results).await?,
            false => self.frequentist_analysis(test, group_results).await?,
        };

        // Check for early stopping criteria
        if test.statistical_config.early_stopping_enabled {
            self.check_early_stopping(&analysis, test).await?;
        }

        Ok(analysis)
    }

    /// Perform sequential statistical analysis
    async fn sequential_analysis(
        &self,
        test: &ActiveTest,
        group_results: &[GroupResults],
    ) -> CompatResult<StatisticalAnalysis> {
        // Sequential probability ratio test implementation
        let control_group = &group_results[0];
        let treatment_group = &group_results[1];
        
        let log_likelihood_ratio = self.calculate_log_likelihood_ratio(
            control_group.conversion_rate,
            treatment_group.conversion_rate,
            control_group.sample_size,
            treatment_group.sample_size,
        );
        
        let upper_boundary = (1.0 / test.statistical_config.significance_threshold).ln();
        let lower_boundary = test.statistical_config.significance_threshold.ln();
        
        let is_significant = log_likelihood_ratio > upper_boundary || log_likelihood_ratio < lower_boundary;
        let winner = if log_likelihood_ratio > upper_boundary {
            Some(treatment_group.group_id.clone())
        } else if log_likelihood_ratio < lower_boundary {
            Some(control_group.group_id.clone())
        } else {
            None
        };
        
        Ok(StatisticalAnalysis {
            is_significant,
            p_value: self.calculate_p_value_sequential(log_likelihood_ratio),
            effect_size: (treatment_group.conversion_rate - control_group.conversion_rate).abs(),
            winner,
            recommendation: self.generate_recommendation(is_significant, winner.as_deref()),
        })
    }

    /// Perform frequentist statistical analysis
    async fn frequentist_analysis(
        &self,
        test: &ActiveTest,
        group_results: &[GroupResults],
    ) -> CompatResult<StatisticalAnalysis> {
        // Two-sample t-test implementation
        let control_group = &group_results[0];
        let treatment_group = &group_results[1];
        
        let t_statistic = self.calculate_t_statistic(
            control_group.conversion_rate,
            treatment_group.conversion_rate,
            control_group.sample_size,
            treatment_group.sample_size,
        );
        
        let degrees_of_freedom = control_group.sample_size + treatment_group.sample_size - 2;
        let p_value = self.calculate_p_value_t_test(t_statistic, degrees_of_freedom);
        
        let is_significant = p_value < test.statistical_config.significance_threshold;
        let winner = if is_significant {
            if treatment_group.conversion_rate > control_group.conversion_rate {
                Some(treatment_group.group_id.clone())
            } else {
                Some(control_group.group_id.clone())
            }
        } else {
            None
        };
        
        Ok(StatisticalAnalysis {
            is_significant,
            p_value,
            effect_size: (treatment_group.conversion_rate - control_group.conversion_rate).abs(),
            winner,
            recommendation: self.generate_recommendation(is_significant, winner.as_deref()),
        })
    }

    /// Calculate log likelihood ratio for sequential testing
    fn calculate_log_likelihood_ratio(
        &self,
        control_rate: f64,
        treatment_rate: f64,
        control_n: u64,
        treatment_n: u64,
    ) -> f64 {
        let control_successes = (control_rate * control_n as f64).round() as u64;
        let treatment_successes = (treatment_rate * treatment_n as f64).round() as u64;
        
        let log_likelihood_h1 = 
            control_successes as f64 * control_rate.ln() +
            (control_n - control_successes) as f64 * (1.0 - control_rate).ln() +
            treatment_successes as f64 * treatment_rate.ln() +
            (treatment_n - treatment_successes) as f64 * (1.0 - treatment_rate).ln();
        
        let pooled_rate = (control_successes + treatment_successes) as f64 / (control_n + treatment_n) as f64;
        let log_likelihood_h0 = 
            (control_successes + treatment_successes) as f64 * pooled_rate.ln() +
            (control_n + treatment_n - control_successes - treatment_successes) as f64 * (1.0 - pooled_rate).ln();
        
        log_likelihood_h1 - log_likelihood_h0
    }

    /// Calculate p-value for sequential test
    fn calculate_p_value_sequential(&self, log_likelihood_ratio: f64) -> f64 {
        // Simplified p-value calculation for sequential test
        let chi_square_stat = 2.0 * log_likelihood_ratio.abs();
        1.0 - self.chi_square_cdf(chi_square_stat, 1.0)
    }

    /// Calculate t-statistic for two-sample test
    fn calculate_t_statistic(
        &self,
        mean1: f64,
        mean2: f64,
        n1: u64,
        n2: u64,
    ) -> f64 {
        let variance1 = mean1 * (1.0 - mean1) / n1 as f64;
        let variance2 = mean2 * (1.0 - mean2) / n2 as f64;
        let pooled_se = (variance1 + variance2).sqrt();
        
        (mean2 - mean1) / pooled_se
    }

    /// Calculate p-value for t-test
    fn calculate_p_value_t_test(&self, t_stat: f64, df: u64) -> f64 {
        // Simplified p-value calculation using normal approximation for large df
        if df > 30 {
            2.0 * (1.0 - self.normal_cdf(t_stat.abs()))
        } else {
            // For small df, use t-distribution approximation
            2.0 * (1.0 - self.t_cdf(t_stat.abs(), df))
        }
    }

    /// Chi-square CDF approximation
    fn chi_square_cdf(&self, x: f64, df: f64) -> f64 {
        // Simplified chi-square CDF using gamma function approximation
        if x <= 0.0 {
            0.0
        } else {
            // Very rough approximation
            (x / (x + df)).powf(df / 2.0)
        }
    }

    /// Normal CDF approximation
    fn normal_cdf(&self, x: f64) -> f64 {
        0.5 * (1.0 + self.erf(x / std::f64::consts::SQRT_2))
    }

    /// t-distribution CDF approximation
    fn t_cdf(&self, t: f64, df: u64) -> f64 {
        // Approximate t-distribution with normal for simplicity
        self.normal_cdf(t)
    }

    /// Error function approximation
    fn erf(&self, x: f64) -> f64 {
        // Abramowitz and Stegun approximation
        let a1 = 0.254829592;
        let a2 = -0.284496736;
        let a3 = 1.421413741;
        let a4 = -1.453152027;
        let a5 = 1.061405429;
        let p = 0.3275911;

        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = x.abs();

        let t = 1.0 / (1.0 + p * x);
        let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

        sign * y
    }

    /// Check early stopping criteria
    async fn check_early_stopping(
        &self,
        analysis: &StatisticalAnalysis,
        test: &ActiveTest,
    ) -> CompatResult<()> {
        // Could implement more sophisticated early stopping rules
        if analysis.is_significant && analysis.effect_size > test.statistical_config.effect_size {
            tracing::info!(
                test_id = %test.id,
                winner = ?analysis.winner,
                p_value = analysis.p_value,
                effect_size = analysis.effect_size,
                "Early stopping criteria met"
            );
        }
        
        Ok(())
    }

    /// Generate recommendation based on analysis
    fn generate_recommendation(
        &self,
        is_significant: bool,
        winner: Option<&str>,
    ) -> TestRecommendation {
        if is_significant {
            if let Some(winning_group) = winner {
                TestRecommendation::StopAndRollout(winning_group.to_string())
            } else {
                TestRecommendation::ContinueTest
            }
        } else {
            TestRecommendation::ContinueTest
        }
    }
}

/// Statistical analysis result
#[derive(Debug, Clone)]
pub struct StatisticalAnalysis {
    pub is_significant: bool,
    pub p_value: f64,
    pub effect_size: f64,
    pub winner: Option<String>,
    pub recommendation: TestRecommendation,
}

impl BayesianEngine {
    pub fn new() -> CompatResult<Self> {
        Ok(Self {
            priors: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

impl FrequentistEngine {
    pub fn new() -> CompatResult<Self> {
        Ok(Self {
            test_configs: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

impl Default for StatisticalConfig {
    fn default() -> Self {
        Self {
            confidence_level: 0.95,
            statistical_power: 0.80,
            minimum_sample_size: 1000,
            effect_size: 0.05,
            significance_threshold: 0.05,
            early_stopping_enabled: true,
            sequential_testing: false,
        }
    }
}

impl Default for TestStatus {
    fn default() -> Self {
        TestStatus::Active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ABTestGroup};

    #[tokio::test]
    async fn test_ab_controller_creation() {
        let config = ABTestingConfig::default();
        let metrics = Arc::new(ABTestMetrics::new().unwrap());
        
        let controller = ABTestController::new(config, metrics);
        assert!(controller.is_ok());
    }

    #[tokio::test]
    async fn test_traffic_assignment() {
        let config = ABTestingConfig::default();
        let metrics = Arc::new(ABTestMetrics::new().unwrap());
        let controller = ABTestController::new(config, metrics).unwrap();
        
        let groups = vec![
            ABTestGroup {
                id: "control".to_string(),
                name: "Control Group".to_string(),
                lighthouse_version: "v4".to_string(),
                traffic_percentage: 50.0,
                features: HashMap::new(),
            },
            ABTestGroup {
                id: "treatment".to_string(),
                name: "Treatment Group".to_string(),
                lighthouse_version: "v5".to_string(),
                traffic_percentage: 50.0,
                features: HashMap::new(),
            },
        ];
        
        // Test multiple assignments to verify distribution
        let mut assignments = Vec::new();
        for i in 0..100 {
            let assignment = controller.hash_assignment(&groups, &format!("user_{}", i));
            assignments.push(assignment);
        }
        
        let control_count = assignments.iter().filter(|&a| a == "control").count();
        let treatment_count = assignments.iter().filter(|&a| a == "treatment").count();
        
        // Should be roughly 50/50 distribution
        assert!(control_count > 30 && control_count < 70);
        assert!(treatment_count > 30 && treatment_count < 70);
        assert_eq!(control_count + treatment_count, 100);
    }

    #[tokio::test]
    async fn test_session_management() {
        let session_manager = SessionManager::new(
            Duration::hours(1),
            Duration::minutes(5),
        ).unwrap();
        
        let session = UserSession {
            session_id: "test_session".to_string(),
            user_id: Some("test_user".to_string()),
            assigned_group: "control".to_string(),
            test_id: "test_id".to_string(),
            created_at: Utc::now(),
            last_activity: Utc::now(),
            metadata: HashMap::new(),
            lighthouse_version: "v4".to_string(),
        };
        
        // Create session
        session_manager.create_session(session.clone()).await.unwrap();
        
        // Retrieve session
        let retrieved = session_manager.get_session("test_session").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().session_id, "test_session");
    }

    #[test]
    fn test_statistical_calculations() {
        let decision_engine = DecisionEngine::new().unwrap();
        
        // Test t-statistic calculation
        let t_stat = decision_engine.calculate_t_statistic(0.1, 0.12, 1000, 1000);
        assert!(t_stat > 0.0);
        
        // Test normal CDF
        let cdf_value = decision_engine.normal_cdf(1.96);
        assert!(cdf_value > 0.97 && cdf_value < 0.98);
        
        // Test error function
        let erf_value = decision_engine.erf(1.0);
        assert!(erf_value > 0.84 && erf_value < 0.85);
    }
}