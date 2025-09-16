# ALYS-013: Implement Parallel Signature Validation

## Issue Type
Task

## Priority
High

## Story Points
5

## Sprint
Migration Sprint 5

## Component
Governance Integration

## Labels
`migration`, `phase-6`, `governance`, `signatures`, `validation`

## Description

Implement parallel signature validation system that runs governance HSM signatures alongside local signatures for comparison and validation before full cutover. This allows safe testing of governance integration without risking production operations.

## Acceptance Criteria

- [ ] Parallel signature collection from both systems
- [ ] Signature comparison and discrepancy logging
- [ ] Metrics for match/mismatch rates
- [ ] Configurable validation mode (local-only, parallel, governance-only)
- [ ] Performance comparison between systems
- [ ] Fallback to local on governance failure
- [ ] No production impact during parallel mode
- [ ] Discrepancy rate < 0.1% before cutover

## Technical Details

### Implementation Steps

1. **Define Parallel Validation System**
```rust
// src/validation/parallel.rs

use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ParallelSignatureValidator {
    // Signature sources
    local_signer: Arc<LocalSigner>,
    governance_stream: Addr<StreamActor>,
    
    // Configuration
    config: ValidationConfig,
    mode: Arc<RwLock<ValidationMode>>,
    
    // Metrics
    metrics: ValidationMetrics,
    comparison_log: ComparisonLogger,
}

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub log_discrepancies: bool,
    pub alert_on_mismatch: bool,
    pub governance_timeout: Duration,
    pub fallback_on_error: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationMode {
    LocalOnly,
    Parallel { primary: SignatureSource },
    GovernanceOnly,
    Transitioning { from: Box<ValidationMode>, to: Box<ValidationMode>, progress: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignatureSource {
    Local,
    Governance,
}

#[derive(Debug)]
pub struct ComparisonResult {
    pub request_id: String,
    pub local_signature: Option<Vec<u8>>,
    pub governance_signature: Option<Vec<u8>>,
    pub matched: bool,
    pub local_time: Duration,
    pub governance_time: Duration,
    pub timestamp: Instant,
}

impl ParallelSignatureValidator {
    pub fn new(
        local_signer: Arc<LocalSigner>,
        governance_stream: Addr<StreamActor>,
        config: ValidationConfig,
    ) -> Self {
        Self {
            local_signer,
            governance_stream,
            config,
            mode: Arc::new(RwLock::new(ValidationMode::LocalOnly)),
            metrics: ValidationMetrics::new(),
            comparison_log: ComparisonLogger::new("signature_comparison.log"),
        }
    }
    
    pub async fn sign_transaction(
        &self,
        tx: &Transaction,
        inputs: Vec<InputToSign>,
    ) -> Result<SignedTransaction, ValidationError> {
        let mode = self.mode.read().await.clone();
        
        match mode {
            ValidationMode::LocalOnly => {
                self.sign_local_only(tx, inputs).await
            }
            ValidationMode::Parallel { primary } => {
                self.sign_parallel(tx, inputs, primary).await
            }
            ValidationMode::GovernanceOnly => {
                self.sign_governance_only(tx, inputs).await
            }
            ValidationMode::Transitioning { from, to, progress } => {
                self.sign_transitioning(tx, inputs, *from, *to, progress).await
            }
        }
    }
    
    async fn sign_parallel(
        &self,
        tx: &Transaction,
        inputs: Vec<InputToSign>,
        primary: SignatureSource,
    ) -> Result<SignedTransaction, ValidationError> {
        let request_id = generate_request_id();
        let start = Instant::now();
        
        // Launch both signing operations in parallel
        let local_future = self.sign_with_local(tx, inputs.clone());
        let governance_future = self.sign_with_governance(tx, inputs.clone(), &request_id);
        
        let (local_result, governance_result) = tokio::join!(local_future, governance_future);
        
        // Record timing
        let local_time = local_result.as_ref()
            .map(|_| start.elapsed())
            .unwrap_or_default();
        
        let governance_time = governance_result.as_ref()
            .map(|_| start.elapsed())
            .unwrap_or_default();
        
        // Compare results
        let comparison = self.compare_signatures(
            &request_id,
            &local_result,
            &governance_result,
            local_time,
            governance_time,
        ).await;
        
        // Log comparison
        self.comparison_log.log(&comparison).await;
        
        // Update metrics
        self.update_metrics(&comparison);
        
        // Decide which result to use based on primary source
        match primary {
            SignatureSource::Local => {
                match local_result {
                    Ok(signed) => Ok(signed),
                    Err(e) if self.config.fallback_on_error => {
                        warn!("Local signing failed, falling back to governance: {}", e);
                        governance_result
                    }
                    Err(e) => Err(e),
                }
            }
            SignatureSource::Governance => {
                match governance_result {
                    Ok(signed) => Ok(signed),
                    Err(e) if self.config.fallback_on_error => {
                        warn!("Governance signing failed, falling back to local: {}", e);
                        local_result
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }
    
    async fn compare_signatures(
        &self,
        request_id: &str,
        local_result: &Result<SignedTransaction, ValidationError>,
        governance_result: &Result<SignedTransaction, ValidationError>,
        local_time: Duration,
        governance_time: Duration,
    ) -> ComparisonResult {
        let local_sig = local_result.as_ref().ok()
            .and_then(|tx| tx.witness.first())
            .map(|w| w.to_vec());
        
        let governance_sig = governance_result.as_ref().ok()
            .and_then(|tx| tx.witness.first())
            .map(|w| w.to_vec());
        
        let matched = match (&local_sig, &governance_sig) {
            (Some(l), Some(g)) => l == g,
            _ => false,
        };
        
        // Alert on mismatch if configured
        if !matched && self.config.alert_on_mismatch {
            self.alert_mismatch(request_id, &local_sig, &governance_sig).await;
        }
        
        ComparisonResult {
            request_id: request_id.to_string(),
            local_signature: local_sig,
            governance_signature: governance_sig,
            matched,
            local_time,
            governance_time,
            timestamp: Instant::now(),
        }
    }
    
    fn update_metrics(&self, comparison: &ComparisonResult) {
        if comparison.matched {
            self.metrics.signature_matches.inc();
        } else {
            self.metrics.signature_mismatches.inc();
            
            // Categorize mismatch
            match (&comparison.local_signature, &comparison.governance_signature) {
                (Some(_), Some(_)) => self.metrics.both_signed_mismatch.inc(),
                (Some(_), None) => self.metrics.governance_failed.inc(),
                (None, Some(_)) => self.metrics.local_failed.inc(),
                (None, None) => self.metrics.both_failed.inc(),
            }
        }
        
        // Record timing metrics
        self.metrics.local_signing_time.observe(comparison.local_time.as_secs_f64());
        self.metrics.governance_signing_time.observe(comparison.governance_time.as_secs_f64());
        
        // Calculate match rate
        let total = self.metrics.signature_matches.get() + self.metrics.signature_mismatches.get();
        if total > 0 {
            let match_rate = self.metrics.signature_matches.get() as f64 / total as f64;
            self.metrics.match_rate.set(match_rate);
        }
    }
}
```

2. **Implement Mode Transition Controller**
```rust
// src/validation/transition.rs

use actix::prelude::*;

pub struct ValidationModeController {
    validator: Arc<ParallelSignatureValidator>,
    current_mode: ValidationMode,
    target_mode: ValidationMode,
    transition_plan: Option<TransitionPlan>,
    metrics_monitor: MetricsMonitor,
}

#[derive(Debug, Clone)]
pub struct TransitionPlan {
    pub from: ValidationMode,
    pub to: ValidationMode,
    pub stages: Vec<TransitionStage>,
    pub current_stage: usize,
    pub started_at: Instant,
    pub rollback_on_error: bool,
}

#[derive(Debug, Clone)]
pub struct TransitionStage {
    pub name: String,
    pub duration: Duration,
    pub validation_mode: ValidationMode,
    pub success_criteria: SuccessCriteria,
}

#[derive(Debug, Clone)]
pub struct SuccessCriteria {
    pub min_match_rate: f64,
    pub max_error_rate: f64,
    pub min_requests: u64,
    pub max_latency_increase: f64,
}

impl ValidationModeController {
    pub async fn transition_to_governance(&mut self) -> Result<(), TransitionError> {
        info!("Starting transition from local to governance signatures");
        
        let plan = TransitionPlan {
            from: ValidationMode::LocalOnly,
            to: ValidationMode::GovernanceOnly,
            stages: vec![
                TransitionStage {
                    name: "Parallel Testing".to_string(),
                    duration: Duration::from_hours(24),
                    validation_mode: ValidationMode::Parallel {
                        primary: SignatureSource::Local,
                    },
                    success_criteria: SuccessCriteria {
                        min_match_rate: 0.99,
                        max_error_rate: 0.01,
                        min_requests: 1000,
                        max_latency_increase: 1.5,
                    },
                },
                TransitionStage {
                    name: "Governance Primary".to_string(),
                    duration: Duration::from_hours(48),
                    validation_mode: ValidationMode::Parallel {
                        primary: SignatureSource::Governance,
                    },
                    success_criteria: SuccessCriteria {
                        min_match_rate: 0.999,
                        max_error_rate: 0.001,
                        min_requests: 5000,
                        max_latency_increase: 1.2,
                    },
                },
                TransitionStage {
                    name: "Governance Only".to_string(),
                    duration: Duration::from_hours(168), // 1 week monitoring
                    validation_mode: ValidationMode::GovernanceOnly,
                    success_criteria: SuccessCriteria {
                        min_match_rate: 1.0, // Not applicable
                        max_error_rate: 0.001,
                        min_requests: 10000,
                        max_latency_increase: 1.0,
                    },
                },
            ],
            current_stage: 0,
            started_at: Instant::now(),
            rollback_on_error: true,
        };
        
        self.transition_plan = Some(plan.clone());
        
        for (i, stage) in plan.stages.iter().enumerate() {
            info!("Executing transition stage {}: {}", i + 1, stage.name);
            
            // Update validation mode
            self.validator.set_mode(stage.validation_mode.clone()).await?;
            
            // Monitor for stage duration
            let result = self.monitor_stage(stage).await;
            
            match result {
                Ok(metrics) => {
                    if !self.validate_success_criteria(&metrics, &stage.success_criteria) {
                        if plan.rollback_on_error {
                            return self.rollback_transition("Success criteria not met").await;
                        }
                    }
                }
                Err(e) => {
                    error!("Stage monitoring failed: {}", e);
                    if plan.rollback_on_error {
                        return self.rollback_transition(&e.to_string()).await;
                    }
                }
            }
        }
        
        info!("Successfully transitioned to governance signatures");
        Ok(())
    }
    
    async fn monitor_stage(&self, stage: &TransitionStage) -> Result<StageMetrics, TransitionError> {
        let start = Instant::now();
        let mut metrics = StageMetrics::default();
        
        while start.elapsed() < stage.duration {
            // Collect metrics every minute
            tokio::time::sleep(Duration::from_secs(60)).await;
            
            let current = self.metrics_monitor.get_current_metrics().await?;
            metrics.update(&current);
            
            // Check for critical errors
            if current.error_rate > stage.success_criteria.max_error_rate * 2.0 {
                return Err(TransitionError::CriticalErrorRate(current.error_rate));
            }
        }
        
        Ok(metrics)
    }
    
    async fn rollback_transition(&mut self, reason: &str) -> Result<(), TransitionError> {
        error!("Rolling back transition: {}", reason);
        
        // Immediate switch back to local
        self.validator.set_mode(ValidationMode::LocalOnly).await?;
        
        // Clear transition plan
        self.transition_plan = None;
        
        // Alert operations team
        self.send_rollback_alert(reason).await;
        
        Err(TransitionError::RolledBack(reason.to_string()))
    }
}
```

3. **Create Comparison Logger**
```rust
// src/validation/logger.rs

use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

pub struct ComparisonLogger {
    log_path: PathBuf,
    buffer: Arc<Mutex<Vec<ComparisonResult>>>,
    flush_interval: Duration,
}

impl ComparisonLogger {
    pub fn new(log_path: impl Into<PathBuf>) -> Self {
        let logger = Self {
            log_path: log_path.into(),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(1000))),
            flush_interval: Duration::from_secs(10),
        };
        
        // Start flush task
        let buffer = logger.buffer.clone();
        let path = logger.log_path.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Self::flush_buffer(&buffer, &path).await;
            }
        });
        
        logger
    }
    
    pub async fn log(&self, comparison: &ComparisonResult) {
        let mut buffer = self.buffer.lock().await;
        buffer.push(comparison.clone());
        
        // Flush if buffer is full
        if buffer.len() >= 1000 {
            drop(buffer);
            Self::flush_buffer(&self.buffer, &self.log_path).await;
        }
    }
    
    async fn flush_buffer(buffer: &Arc<Mutex<Vec<ComparisonResult>>>, path: &Path) {
        let mut buffer = buffer.lock().await;
        if buffer.is_empty() {
            return;
        }
        
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open comparison log: {}", e);
                return;
            }
        };
        
        for comparison in buffer.drain(..) {
            let log_entry = format!(
                "{},{},{},{},{},{:.3},{:.3}\n",
                comparison.timestamp.elapsed().as_secs(),
                comparison.request_id,
                comparison.matched,
                comparison.local_signature.is_some(),
                comparison.governance_signature.is_some(),
                comparison.local_time.as_secs_f64(),
                comparison.governance_time.as_secs_f64(),
            );
            
            if let Err(e) = file.write_all(log_entry.as_bytes()).await {
                error!("Failed to write comparison log: {}", e);
            }
        }
        
        let _ = file.flush().await;
    }
}
```

## Testing Plan

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_parallel_validation() {
        let validator = create_test_validator();
        validator.set_mode(ValidationMode::Parallel {
            primary: SignatureSource::Local,
        }).await.unwrap();
        
        let tx = create_test_transaction();
        let inputs = vec![create_test_input()];
        
        let signed = validator.sign_transaction(&tx, inputs).await.unwrap();
        
        // Check metrics
        let metrics = validator.get_metrics();
        assert!(metrics.signature_matches.get() > 0 || metrics.signature_mismatches.get() > 0);
    }
    
    #[tokio::test]
    async fn test_mode_transition() {
        let mut controller = ValidationModeController::new(create_test_validator());
        
        // Simulate successful transition
        let result = controller.transition_to_governance().await;
        
        assert!(result.is_ok());
        assert_eq!(controller.current_mode, ValidationMode::GovernanceOnly);
    }
    
    #[tokio::test]
    async fn test_rollback_on_failure() {
        let mut controller = ValidationModeController::new(create_test_validator());
        
        // Inject failure condition
        inject_governance_failure();
        
        let result = controller.transition_to_governance().await;
        
        assert!(result.is_err());
        assert_eq!(controller.current_mode, ValidationMode::LocalOnly);
    }
    
    #[tokio::test]
    async fn test_comparison_logging() {
        let logger = ComparisonLogger::new("/tmp/test_comparison.log");
        
        for i in 0..100 {
            logger.log(&create_test_comparison(i)).await;
        }
        
        // Force flush
        tokio::time::sleep(Duration::from_secs(11)).await;
        
        // Verify log file exists and contains data
        let contents = tokio::fs::read_to_string("/tmp/test_comparison.log").await.unwrap();
        assert!(contents.lines().count() >= 100);
    }
}
```

### Integration Tests
1. Test with real governance connection
2. Test signature matching accuracy
3. Test performance under load
4. Test transition stages
5. Test rollback procedures

### Performance Tests
```rust
#[bench]
fn bench_parallel_signing(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let validator = runtime.block_on(create_test_validator());
    
    b.iter(|| {
        runtime.block_on(async {
            let tx = create_test_transaction();
            let inputs = create_test_inputs(10);
            validator.sign_transaction(&tx, inputs).await.unwrap()
        })
    });
}
```

## Dependencies

### Blockers
- ALYS-012: StreamActor for governance communication

### Blocked By
None

### Related Issues
- ALYS-014: Governance cutover
- ALYS-015: Key removal

## Definition of Done

- [ ] Parallel validation implemented
- [ ] Comparison logging working
- [ ] Metrics collection operational
- [ ] Mode transition controller tested
- [ ] Rollback procedures validated
- [ ] Match rate > 99.9% achieved
- [ ] Performance impact < 10%
- [ ] Documentation complete
- [ ] Code review completed

## Notes

- Consider caching validation results
- Implement alerting for high mismatch rates
- Add dashboard for monitoring transition
- Consider gradual rollout by transaction type

## Time Tracking

- Estimated: 3 days
- Actual: _To be filled_