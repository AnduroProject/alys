use std::time::{Duration, Instant};
use std::collections::HashMap;
use anyhow::{Result, Context};
use tracing::{info, debug, warn, error};

use crate::{TestResult, TestError, ValidationResult, MigrationPhase};

/// Collection of test result validators
/// 
/// Provides validation logic for test results across different migration phases
/// and ensures test quality and consistency.
#[derive(Debug)]
pub struct Validators {
    /// Phase-specific validators
    phase_validators: HashMap<MigrationPhase, Box<dyn PhaseValidator>>,
    
    /// Generic result validators
    result_validators: Vec<Box<dyn ResultValidator>>,
    
    /// Validation metrics
    metrics: ValidatorMetrics,
}

/// Metrics for validation operations
#[derive(Debug, Clone, Default)]
pub struct ValidatorMetrics {
    pub validations_performed: u64,
    pub validations_passed: u64,
    pub validations_failed: u64,
    pub average_validation_time: Duration,
}

/// Trait for phase-specific validators
pub trait PhaseValidator: Send + Sync + std::fmt::Debug {
    /// Validate results for a specific migration phase
    fn validate_phase(&self, results: &[TestResult]) -> Result<ValidationSummary>;
    
    /// Get validator name
    fn name(&self) -> &str;
}

/// Trait for generic result validators
pub trait ResultValidator: Send + Sync + std::fmt::Debug {
    /// Validate individual test result
    fn validate_result(&self, result: &TestResult) -> Result<bool>;
    
    /// Get validator name
    fn name(&self) -> &str;
}

/// Summary of validation results
#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub phase: MigrationPhase,
    pub total_tests: u32,
    pub passed_tests: u32,
    pub failed_tests: u32,
    pub critical_failures: Vec<String>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Foundation phase validator
#[derive(Debug)]
pub struct FoundationValidator;

/// Actor core phase validator
#[derive(Debug)]
pub struct ActorCoreValidator;

/// Sync improvement phase validator
#[derive(Debug)]
pub struct SyncImprovementValidator;

/// Lighthouse migration phase validator
#[derive(Debug)]
pub struct LighthouseMigrationValidator;

/// Governance integration phase validator
#[derive(Debug)]
pub struct GovernanceIntegrationValidator;

/// Duration validator - ensures tests complete within reasonable time
#[derive(Debug)]
pub struct DurationValidator {
    max_duration: Duration,
}

/// Success rate validator - ensures minimum success rate
#[derive(Debug)]
pub struct SuccessRateValidator {
    min_success_rate: f64,
}

/// Performance regression validator
#[derive(Debug)]
pub struct PerformanceRegressionValidator {
    baseline_metrics: HashMap<String, f64>,
    regression_threshold: f64,
}

impl Validators {
    /// Create a new Validators instance
    pub fn new() -> Result<Self> {
        info!("Initializing test validators");
        
        let mut phase_validators: HashMap<MigrationPhase, Box<dyn PhaseValidator>> = HashMap::new();
        
        // Register phase-specific validators
        phase_validators.insert(
            MigrationPhase::Foundation,
            Box::new(FoundationValidator),
        );
        phase_validators.insert(
            MigrationPhase::ActorCore,
            Box::new(ActorCoreValidator),
        );
        phase_validators.insert(
            MigrationPhase::SyncImprovement,
            Box::new(SyncImprovementValidator),
        );
        phase_validators.insert(
            MigrationPhase::LighthouseMigration,
            Box::new(LighthouseMigrationValidator),
        );
        phase_validators.insert(
            MigrationPhase::GovernanceIntegration,
            Box::new(GovernanceIntegrationValidator),
        );
        
        // Register generic result validators
        let result_validators: Vec<Box<dyn ResultValidator>> = vec![
            Box::new(DurationValidator {
                max_duration: Duration::from_secs(300), // 5 minutes max per test
            }),
            Box::new(SuccessRateValidator {
                min_success_rate: 0.95, // 95% success rate minimum
            }),
            Box::new(PerformanceRegressionValidator {
                baseline_metrics: HashMap::new(),
                regression_threshold: 0.15, // 15% regression threshold
            }),
        ];
        
        let validators = Self {
            phase_validators,
            result_validators,
            metrics: ValidatorMetrics::default(),
        };
        
        info!("Validators initialized successfully");
        Ok(validators)
    }
    
    /// Validate results for a specific migration phase
    pub async fn validate_phase_results(
        &mut self,
        phase: MigrationPhase,
        results: &[TestResult],
    ) -> Result<ValidationSummary> {
        let start = Instant::now();
        info!("Validating results for phase: {:?}", phase);
        
        // Get phase-specific validator
        let validator = self.phase_validators.get(&phase)
            .ok_or_else(|| anyhow::anyhow!("No validator found for phase: {:?}", phase))?;
        
        // Run phase-specific validation
        let mut summary = validator.validate_phase(results)?;
        
        // Run generic result validators on each result
        for result in results {
            for result_validator in &self.result_validators {
                match result_validator.validate_result(result) {
                    Ok(valid) => {
                        if !valid {
                            summary.warnings.push(format!(
                                "Result validation '{}' failed for test: {}",
                                result_validator.name(),
                                result.test_name
                            ));
                        }
                    }
                    Err(e) => {
                        summary.critical_failures.push(format!(
                            "Result validator '{}' error for test {}: {}",
                            result_validator.name(),
                            result.test_name,
                            e
                        ));
                    }
                }
            }
        }
        
        let duration = start.elapsed();
        
        // Update metrics
        self.metrics.validations_performed += 1;
        if summary.critical_failures.is_empty() {
            self.metrics.validations_passed += 1;
        } else {
            self.metrics.validations_failed += 1;
        }
        
        // Update average validation time
        let total_time = self.metrics.average_validation_time * (self.metrics.validations_performed - 1) as u32 + duration;
        self.metrics.average_validation_time = total_time / self.metrics.validations_performed as u32;
        
        info!("Phase validation completed in {:?}", duration);
        Ok(summary)
    }
    
    /// Get validation metrics
    pub fn get_metrics(&self) -> &ValidatorMetrics {
        &self.metrics
    }
}

// Phase validator implementations

impl PhaseValidator for FoundationValidator {
    fn validate_phase(&self, results: &[TestResult]) -> Result<ValidationSummary> {
        let mut summary = ValidationSummary {
            phase: MigrationPhase::Foundation,
            total_tests: results.len() as u32,
            passed_tests: results.iter().filter(|r| r.success).count() as u32,
            failed_tests: results.iter().filter(|r| !r.success).count() as u32,
            critical_failures: Vec::new(),
            warnings: Vec::new(),
            recommendations: Vec::new(),
        };
        
        // Foundation-specific validations
        if summary.failed_tests > 0 {
            summary.critical_failures.push(
                "Foundation phase must have zero failures as it's critical for all other phases".to_string()
            );
        }
        
        // Check for framework initialization test
        if !results.iter().any(|r| r.test_name.contains("framework_initialization")) {
            summary.warnings.push("No framework initialization test found".to_string());
        }
        
        // Check for configuration validation test
        if !results.iter().any(|r| r.test_name.contains("configuration_validation")) {
            summary.warnings.push("No configuration validation test found".to_string());
        }
        
        if summary.passed_tests == summary.total_tests {
            summary.recommendations.push("Foundation phase validation successful".to_string());
        }
        
        Ok(summary)
    }
    
    fn name(&self) -> &str {
        "FoundationValidator"
    }
}

impl PhaseValidator for ActorCoreValidator {
    fn validate_phase(&self, results: &[TestResult]) -> Result<ValidationSummary> {
        let mut summary = ValidationSummary {
            phase: MigrationPhase::ActorCore,
            total_tests: results.len() as u32,
            passed_tests: results.iter().filter(|r| r.success).count() as u32,
            failed_tests: results.iter().filter(|r| !r.success).count() as u32,
            critical_failures: Vec::new(),
            warnings: Vec::new(),
            recommendations: Vec::new(),
        };
        
        // Actor-specific validations
        let lifecycle_tests = results.iter().filter(|r| r.test_name.contains("lifecycle")).count();
        if lifecycle_tests == 0 {
            summary.critical_failures.push("No actor lifecycle tests found".to_string());
        }
        
        let recovery_tests = results.iter().filter(|r| r.test_name.contains("recovery")).count();
        if recovery_tests == 0 {
            summary.warnings.push("No actor recovery tests found".to_string());
        }
        
        let message_ordering_tests = results.iter().filter(|r| r.test_name.contains("ordering")).count();
        if message_ordering_tests == 0 {
            summary.warnings.push("No message ordering tests found".to_string());
        }
        
        if summary.passed_tests as f64 / summary.total_tests as f64 >= 0.9 {
            summary.recommendations.push("Actor core validation successful".to_string());
        } else {
            summary.recommendations.push("Consider adding more actor stability tests".to_string());
        }
        
        Ok(summary)
    }
    
    fn name(&self) -> &str {
        "ActorCoreValidator"
    }
}

impl PhaseValidator for SyncImprovementValidator {
    fn validate_phase(&self, results: &[TestResult]) -> Result<ValidationSummary> {
        let summary = ValidationSummary {
            phase: MigrationPhase::SyncImprovement,
            total_tests: results.len() as u32,
            passed_tests: results.iter().filter(|r| r.success).count() as u32,
            failed_tests: results.iter().filter(|r| !r.success).count() as u32,
            critical_failures: Vec::new(),
            warnings: Vec::new(),
            recommendations: vec!["Sync improvement validation completed".to_string()],
        };
        
        Ok(summary)
    }
    
    fn name(&self) -> &str {
        "SyncImprovementValidator"
    }
}

impl PhaseValidator for LighthouseMigrationValidator {
    fn validate_phase(&self, results: &[TestResult]) -> Result<ValidationSummary> {
        let summary = ValidationSummary {
            phase: MigrationPhase::LighthouseMigration,
            total_tests: results.len() as u32,
            passed_tests: results.iter().filter(|r| r.success).count() as u32,
            failed_tests: results.iter().filter(|r| !r.success).count() as u32,
            critical_failures: Vec::new(),
            warnings: Vec::new(),
            recommendations: vec!["Lighthouse migration validation completed".to_string()],
        };
        
        Ok(summary)
    }
    
    fn name(&self) -> &str {
        "LighthouseMigrationValidator"
    }
}

impl PhaseValidator for GovernanceIntegrationValidator {
    fn validate_phase(&self, results: &[TestResult]) -> Result<ValidationSummary> {
        let summary = ValidationSummary {
            phase: MigrationPhase::GovernanceIntegration,
            total_tests: results.len() as u32,
            passed_tests: results.iter().filter(|r| r.success).count() as u32,
            failed_tests: results.iter().filter(|r| !r.success).count() as u32,
            critical_failures: Vec::new(),
            warnings: Vec::new(),
            recommendations: vec!["Governance integration validation completed".to_string()],
        };
        
        Ok(summary)
    }
    
    fn name(&self) -> &str {
        "GovernanceIntegrationValidator"
    }
}

// Result validator implementations

impl ResultValidator for DurationValidator {
    fn validate_result(&self, result: &TestResult) -> Result<bool> {
        let valid = result.duration <= self.max_duration;
        if !valid {
            warn!(
                "Test '{}' exceeded maximum duration: {:?} > {:?}",
                result.test_name, result.duration, self.max_duration
            );
        }
        Ok(valid)
    }
    
    fn name(&self) -> &str {
        "DurationValidator"
    }
}

impl ResultValidator for SuccessRateValidator {
    fn validate_result(&self, result: &TestResult) -> Result<bool> {
        // For individual results, this just checks success
        // In a real implementation, this might track success rates over time
        let valid = result.success;
        if !valid {
            debug!("Test '{}' failed", result.test_name);
        }
        Ok(valid)
    }
    
    fn name(&self) -> &str {
        "SuccessRateValidator"
    }
}

impl ResultValidator for PerformanceRegressionValidator {
    fn validate_result(&self, result: &TestResult) -> Result<bool> {
        // Check for performance regression based on duration
        // In a real implementation, this would compare against historical baselines
        let baseline_duration = self.baseline_metrics.get(&result.test_name)
            .copied()
            .unwrap_or(result.duration.as_millis() as f64);
        
        let current_duration = result.duration.as_millis() as f64;
        let regression_ratio = (current_duration - baseline_duration) / baseline_duration;
        
        let valid = regression_ratio <= self.regression_threshold;
        if !valid {
            warn!(
                "Performance regression detected for test '{}': {:.1}% slower",
                result.test_name, regression_ratio * 100.0
            );
        }
        
        Ok(valid)
    }
    
    fn name(&self) -> &str {
        "PerformanceRegressionValidator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    fn create_test_result(name: &str, success: bool, duration_ms: u64) -> TestResult {
        TestResult {
            test_name: name.to_string(),
            success,
            duration: Duration::from_millis(duration_ms),
            message: None,
            metadata: HashMap::new(),
        }
    }
    
    #[tokio::test]
    async fn test_validators_initialization() {
        let validators = Validators::new().unwrap();
        assert_eq!(validators.phase_validators.len(), 5);
        assert_eq!(validators.result_validators.len(), 3);
    }
    
    #[tokio::test]
    async fn test_foundation_validator() {
        let mut validators = Validators::new().unwrap();
        
        let results = vec![
            create_test_result("framework_initialization", true, 100),
            create_test_result("configuration_validation", true, 50),
        ];
        
        let summary = validators.validate_phase_results(MigrationPhase::Foundation, &results).await.unwrap();
        
        assert_eq!(summary.total_tests, 2);
        assert_eq!(summary.passed_tests, 2);
        assert_eq!(summary.failed_tests, 0);
        assert!(summary.critical_failures.is_empty());
    }
    
    #[tokio::test]
    async fn test_duration_validator() {
        let validator = DurationValidator {
            max_duration: Duration::from_millis(100),
        };
        
        let fast_result = create_test_result("fast_test", true, 50);
        let slow_result = create_test_result("slow_test", true, 200);
        
        assert!(validator.validate_result(&fast_result).unwrap());
        assert!(!validator.validate_result(&slow_result).unwrap());
    }
    
    #[tokio::test]
    async fn test_success_rate_validator() {
        let validator = SuccessRateValidator {
            min_success_rate: 0.95,
        };
        
        let success_result = create_test_result("success_test", true, 100);
        let failed_result = create_test_result("failed_test", false, 100);
        
        assert!(validator.validate_result(&success_result).unwrap());
        assert!(!validator.validate_result(&failed_result).unwrap());
    }
}