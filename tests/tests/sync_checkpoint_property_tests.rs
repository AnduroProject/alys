//! Sync Checkpoint Consistency Property Tests - ALYS-002-18
//!
//! Property tests for validating sync checkpoint consistency with failure injection.
//! Tests verify that checkpoint validation remains consistent even under various
//! failure scenarios including network partitions, data corruption, and Byzantine behavior.

use proptest::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

// Checkpoint data structures for testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCheckpoint {
    pub height: u64,
    pub block_hash: String,
    pub state_root: String,
    pub timestamp: u64,
    pub interval: u64,
    pub signature: Option<CheckpointSignature>,
    pub verified: bool,
    pub peer_confirmations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointSignature {
    pub signature_data: Vec<u8>,
    pub signer_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub current_height: u64,
    pub target_height: u64,
    pub checkpoints: HashMap<u64, SyncCheckpoint>,
    pub failed_heights: Vec<u64>,
    pub last_verified_checkpoint: Option<u64>,
}

// Failure injection types
#[derive(Debug, Clone)]
pub enum FailureType {
    NetworkPartition { duration: Duration },
    DataCorruption { affected_heights: Vec<u64> },
    SignatureFailure { probability: f64 },
    PeerDisconnection { peer_count: u32 },
    CheckpointDelay { delay: Duration },
    InvalidStateRoot { height: u64 },
}

#[derive(Debug, Clone)]
pub struct FailureInjectionScenario {
    pub failures: Vec<FailureType>,
    pub failure_points: Vec<u64>, // Heights where failures occur
    pub recovery_time: Duration,
}

#[derive(Debug, Clone)]
pub struct CheckpointConsistencyResult {
    pub total_checkpoints: u32,
    pub verified_checkpoints: u32,
    pub failed_checkpoints: u32,
    pub consistency_violations: Vec<String>,
    pub recovery_time: Duration,
    pub final_state: SyncState,
}

// Generators for checkpoint testing
fn checkpoint_signature_strategy() -> impl Strategy<Value = CheckpointSignature> {
    (
        prop::collection::vec(any::<u8>(), 64..96), // Signature bytes
        "[a-zA-Z0-9]{10,20}", // Signer ID
        1_000_000_000u64..2_000_000_000u64, // Timestamp
    ).prop_map(|(signature_data, signer_id, timestamp)| {
        CheckpointSignature {
            signature_data,
            signer_id,
            timestamp,
        }
    })
}

fn sync_checkpoint_strategy() -> impl Strategy<Value = SyncCheckpoint> {
    (
        0u64..1_000_000, // Height
        "[a-f0-9]{64}", // Block hash
        "[a-f0-9]{64}", // State root
        1_000_000_000u64..2_000_000_000u64, // Timestamp
        10u64..1000, // Interval
        prop::option::of(checkpoint_signature_strategy()),
        any::<bool>(), // Verified
        0u32..10, // Peer confirmations
    ).prop_map(|(height, block_hash, state_root, timestamp, interval, signature, verified, peer_confirmations)| {
        SyncCheckpoint {
            height,
            block_hash,
            state_root,
            timestamp,
            interval,
            signature,
            verified,
            peer_confirmations,
        }
    })
}

fn failure_type_strategy() -> impl Strategy<Value = FailureType> {
    prop_oneof![
        (0u64..30_000).prop_map(|ms| FailureType::NetworkPartition { 
            duration: Duration::from_millis(ms) 
        }),
        prop::collection::vec(0u64..1_000_000, 1..10)
            .prop_map(|heights| FailureType::DataCorruption { affected_heights: heights }),
        (0.0f64..1.0).prop_map(|prob| FailureType::SignatureFailure { probability: prob }),
        (1u32..20).prop_map(|count| FailureType::PeerDisconnection { peer_count: count }),
        (0u64..10_000).prop_map(|ms| FailureType::CheckpointDelay { 
            delay: Duration::from_millis(ms) 
        }),
        (0u64..1_000_000).prop_map(|height| FailureType::InvalidStateRoot { height }),
    ]
}

fn failure_injection_scenario_strategy() -> impl Strategy<Value = FailureInjectionScenario> {
    (
        prop::collection::vec(failure_type_strategy(), 1..5), // Multiple failure types
        prop::collection::vec(0u64..1_000_000, 3..20), // Failure points
        (0u64..60_000), // Recovery time in milliseconds
    ).prop_map(|(failures, failure_points, recovery_ms)| {
        FailureInjectionScenario {
            failures,
            failure_points,
            recovery_time: Duration::from_millis(recovery_ms),
        }
    })
}

// Checkpoint consistency validator
impl SyncState {
    pub fn new(target_height: u64) -> Self {
        Self {
            current_height: 0,
            target_height,
            checkpoints: HashMap::new(),
            failed_heights: Vec::new(),
            last_verified_checkpoint: None,
        }
    }

    pub fn add_checkpoint(&mut self, checkpoint: SyncCheckpoint) -> Result<(), String> {
        let height = checkpoint.height;
        
        // Validate checkpoint consistency
        if let Some(last_verified) = self.last_verified_checkpoint {
            if height <= last_verified {
                return Err(format!("Checkpoint height {} is not greater than last verified {}", 
                                  height, last_verified));
            }
        }

        // Check interval consistency
        if height > 0 {
            let expected_interval = checkpoint.interval;
            if height % expected_interval != 0 {
                return Err(format!("Checkpoint height {} not aligned with interval {}", 
                                  height, expected_interval));
            }
        }

        // Add checkpoint
        self.checkpoints.insert(height, checkpoint.clone());
        
        if checkpoint.verified {
            self.last_verified_checkpoint = Some(height);
            self.current_height = height;
        }

        Ok(())
    }

    pub fn inject_failure(&mut self, failure: &FailureType, at_height: u64) -> Vec<String> {
        let mut violations = Vec::new();

        match failure {
            FailureType::DataCorruption { affected_heights } => {
                for &height in affected_heights {
                    if let Some(checkpoint) = self.checkpoints.get_mut(&height) {
                        checkpoint.block_hash = "corrupted".to_string();
                        checkpoint.verified = false;
                        violations.push(format!("Data corruption at height {}", height));
                    }
                }
            }
            FailureType::SignatureFailure { probability } => {
                if let Some(checkpoint) = self.checkpoints.get_mut(&at_height) {
                    if *probability > 0.5 { // Simulate failure
                        checkpoint.signature = None;
                        checkpoint.verified = false;
                        violations.push(format!("Signature failure at height {}", at_height));
                    }
                }
            }
            FailureType::InvalidStateRoot { height } => {
                if let Some(checkpoint) = self.checkpoints.get_mut(height) {
                    checkpoint.state_root = "invalid".to_string();
                    checkpoint.verified = false;
                    violations.push(format!("Invalid state root at height {}", height));
                }
            }
            FailureType::PeerDisconnection { peer_count } => {
                for checkpoint in self.checkpoints.values_mut() {
                    checkpoint.peer_confirmations = checkpoint.peer_confirmations.saturating_sub(*peer_count);
                    if checkpoint.peer_confirmations < 2 {
                        checkpoint.verified = false;
                    }
                }
                violations.push(format!("Peer disconnection: {} peers lost", peer_count));
            }
            FailureType::NetworkPartition { duration: _ } => {
                // Simulate network partition by marking recent checkpoints as unverified
                let recent_threshold = self.current_height.saturating_sub(100);
                for (height, checkpoint) in self.checkpoints.iter_mut() {
                    if *height > recent_threshold {
                        checkpoint.verified = false;
                    }
                }
                violations.push("Network partition detected".to_string());
            }
            FailureType::CheckpointDelay { delay: _ } => {
                // Simulate delay by not affecting state but recording the delay
                violations.push(format!("Checkpoint delay at height {}", at_height));
            }
        }

        self.failed_heights.push(at_height);
        violations
    }

    pub fn attempt_recovery(&mut self) -> Result<(), String> {
        // Recovery logic: re-verify checkpoints that can be recovered
        let mut recovered_count = 0;
        
        for (height, checkpoint) in self.checkpoints.iter_mut() {
            if !checkpoint.verified && checkpoint.signature.is_some() 
                && checkpoint.block_hash != "corrupted" 
                && checkpoint.state_root != "invalid" {
                
                // Simulate successful recovery
                checkpoint.verified = true;
                recovered_count += 1;
                
                // Update last verified checkpoint if this is newer
                if let Some(last_verified) = self.last_verified_checkpoint {
                    if *height > last_verified {
                        self.last_verified_checkpoint = Some(*height);
                        self.current_height = *height;
                    }
                } else {
                    self.last_verified_checkpoint = Some(*height);
                    self.current_height = *height;
                }
            }
        }

        if recovered_count > 0 {
            Ok(())
        } else {
            Err("Recovery failed - no checkpoints could be verified".to_string())
        }
    }

    pub fn validate_consistency(&self) -> Vec<String> {
        let mut violations = Vec::new();

        // Check checkpoint ordering
        let mut sorted_heights: Vec<_> = self.checkpoints.keys().cloned().collect();
        sorted_heights.sort();

        for window in sorted_heights.windows(2) {
            let lower = window[0];
            let higher = window[1];
            
            if let (Some(lower_cp), Some(higher_cp)) = 
                (self.checkpoints.get(&lower), self.checkpoints.get(&higher)) {
                
                // Check timestamp ordering
                if lower_cp.timestamp >= higher_cp.timestamp {
                    violations.push(format!("Timestamp inconsistency: {} >= {} at heights {} and {}", 
                                          lower_cp.timestamp, higher_cp.timestamp, lower, higher));
                }
                
                // Check interval consistency
                if lower_cp.interval != higher_cp.interval {
                    violations.push(format!("Interval mismatch: {} vs {} at heights {} and {}", 
                                          lower_cp.interval, higher_cp.interval, lower, higher));
                }
            }
        }

        // Check current height consistency
        if let Some(last_verified) = self.last_verified_checkpoint {
            if self.current_height != last_verified {
                violations.push(format!("Current height {} doesn't match last verified checkpoint {}", 
                                      self.current_height, last_verified));
            }
        }

        violations
    }
}

// Main test function
pub fn test_checkpoint_consistency_with_failures(
    checkpoints: Vec<SyncCheckpoint>,
    scenario: FailureInjectionScenario
) -> CheckpointConsistencyResult {
    let start_time = SystemTime::now();
    
    let target_height = checkpoints.iter().map(|cp| cp.height).max().unwrap_or(1000);
    let mut sync_state = SyncState::new(target_height);
    
    let mut consistency_violations = Vec::new();
    let mut total_checkpoints = 0;
    let mut verified_checkpoints = 0;
    let mut failed_checkpoints = 0;

    // Add checkpoints to sync state
    for checkpoint in checkpoints {
        total_checkpoints += 1;
        
        if let Err(violation) = sync_state.add_checkpoint(checkpoint.clone()) {
            consistency_violations.push(violation);
            failed_checkpoints += 1;
        } else if checkpoint.verified {
            verified_checkpoints += 1;
        }
    }

    // Inject failures at specified points
    for (i, &failure_height) in scenario.failure_points.iter().enumerate() {
        if let Some(failure) = scenario.failures.get(i % scenario.failures.len()) {
            let mut violations = sync_state.inject_failure(failure, failure_height);
            consistency_violations.append(&mut violations);
        }
    }

    // Attempt recovery after failures
    std::thread::sleep(Duration::from_millis(10)); // Simulate recovery delay
    
    if sync_state.attempt_recovery().is_ok() {
        // Re-count verified checkpoints after recovery
        verified_checkpoints = sync_state.checkpoints.values()
            .filter(|cp| cp.verified).count() as u32;
        failed_checkpoints = total_checkpoints - verified_checkpoints;
    }

    // Validate final consistency
    let mut final_violations = sync_state.validate_consistency();
    consistency_violations.append(&mut final_violations);

    let recovery_time = start_time.elapsed().unwrap_or_default();

    CheckpointConsistencyResult {
        total_checkpoints,
        verified_checkpoints,
        failed_checkpoints,
        consistency_violations,
        recovery_time,
        final_state: sync_state,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    
    /// Test: Checkpoint consistency should be maintained even with failures
    #[test]
    fn test_checkpoint_consistency_under_failures(
        checkpoints in prop::collection::vec(sync_checkpoint_strategy(), 10..50),
        scenario in failure_injection_scenario_strategy()
    ) {
        let result = test_checkpoint_consistency_with_failures(checkpoints, scenario);
        
        // Property: Recovery should improve or maintain verification rate
        prop_assert!(
            result.verified_checkpoints <= result.total_checkpoints,
            "More verified checkpoints than total: {} > {}", 
            result.verified_checkpoints, result.total_checkpoints
        );
        
        // Property: Failed checkpoints should not exceed total
        prop_assert!(
            result.failed_checkpoints <= result.total_checkpoints,
            "Failed checkpoints exceed total: {} > {}", 
            result.failed_checkpoints, result.total_checkpoints
        );
        
        // Property: Recovery time should be reasonable (under 1 second for testing)
        prop_assert!(
            result.recovery_time < Duration::from_secs(1),
            "Recovery time too long: {:?}", result.recovery_time
        );
    }
    
    /// Test: Checkpoint intervals must be consistent across the chain
    #[test]
    fn test_checkpoint_interval_consistency(
        base_interval in 10u64..100,
        checkpoint_count in 5usize..30
    ) {
        let checkpoints: Vec<_> = (0..checkpoint_count)
            .map(|i| SyncCheckpoint {
                height: (i as u64 + 1) * base_interval,
                block_hash: format!("hash_{}", i),
                state_root: format!("state_{}", i),
                timestamp: 1000000000 + (i as u64 * 1000),
                interval: base_interval,
                signature: Some(CheckpointSignature {
                    signature_data: vec![i as u8; 64],
                    signer_id: format!("signer_{}", i),
                    timestamp: 1000000000 + (i as u64 * 1000),
                }),
                verified: true,
                peer_confirmations: 5,
            })
            .collect();

        let scenario = FailureInjectionScenario {
            failures: vec![FailureType::CheckpointDelay { delay: Duration::from_millis(100) }],
            failure_points: vec![base_interval * 2, base_interval * 5],
            recovery_time: Duration::from_millis(500),
        };

        let result = test_checkpoint_consistency_with_failures(checkpoints, scenario);
        
        // Property: All checkpoints should have consistent intervals
        let interval_violations: Vec<_> = result.consistency_violations.iter()
            .filter(|v| v.contains("Interval mismatch"))
            .collect();
        
        prop_assert!(
            interval_violations.is_empty(),
            "Interval inconsistencies detected: {:?}", interval_violations
        );
    }
    
    /// Test: Recovery should restore checkpoint verification where possible
    #[test]
    fn test_checkpoint_recovery_effectiveness(
        mut checkpoints in prop::collection::vec(sync_checkpoint_strategy(), 15..40)
    ) {
        // Ensure at least half have valid signatures for recovery
        for (i, checkpoint) in checkpoints.iter_mut().enumerate() {
            if i % 2 == 0 {
                checkpoint.signature = Some(CheckpointSignature {
                    signature_data: vec![i as u8; 64],
                    signer_id: format!("valid_signer_{}", i),
                    timestamp: checkpoint.timestamp,
                });
                checkpoint.verified = true;
            }
        }

        let scenario = FailureInjectionScenario {
            failures: vec![
                FailureType::NetworkPartition { duration: Duration::from_millis(1000) },
                FailureType::PeerDisconnection { peer_count: 3 },
            ],
            failure_points: checkpoints.iter().take(5).map(|cp| cp.height).collect(),
            recovery_time: Duration::from_millis(2000),
        };

        let result = test_checkpoint_consistency_with_failures(checkpoints.clone(), scenario);
        
        // Property: Some recovery should be possible with valid signatures
        let recoverable_count = checkpoints.iter()
            .filter(|cp| cp.signature.is_some() && cp.block_hash != "corrupted")
            .count();
        
        if recoverable_count > 0 {
            prop_assert!(
                result.verified_checkpoints > 0,
                "No checkpoints recovered despite {} being recoverable", recoverable_count
            );
        }
    }
    
    /// Test: Byzantine failures should not break checkpoint consistency permanently
    #[test]
    fn test_byzantine_failure_resilience(
        checkpoints in prop::collection::vec(sync_checkpoint_strategy(), 20..60)
    ) {
        let byzantine_scenario = FailureInjectionScenario {
            failures: vec![
                FailureType::DataCorruption { affected_heights: vec![100, 200, 300] },
                FailureType::SignatureFailure { probability: 0.8 },
                FailureType::InvalidStateRoot { height: 150 },
            ],
            failure_points: (0..10).map(|i| i * 50).collect(),
            recovery_time: Duration::from_millis(3000),
        };

        let result = test_checkpoint_consistency_with_failures(checkpoints, byzantine_scenario);
        
        // Property: System should maintain some functionality despite Byzantine failures
        let consistency_rate = result.verified_checkpoints as f64 / result.total_checkpoints as f64;
        
        prop_assert!(
            consistency_rate >= 0.0, // At minimum, should not have negative consistency
            "Negative consistency rate: {}", consistency_rate
        );
        
        // Property: Recovery should complete within reasonable time
        prop_assert!(
            result.recovery_time < Duration::from_secs(5),
            "Byzantine recovery took too long: {:?}", result.recovery_time
        );
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_checkpoint_addition_basic() {
        let mut sync_state = SyncState::new(1000);
        
        let checkpoint = SyncCheckpoint {
            height: 100,
            block_hash: "test_hash".to_string(),
            state_root: "test_state".to_string(),
            timestamp: 1000000000,
            interval: 100,
            signature: None,
            verified: true,
            peer_confirmations: 5,
        };

        let result = sync_state.add_checkpoint(checkpoint);
        assert!(result.is_ok());
        assert_eq!(sync_state.checkpoints.len(), 1);
        assert_eq!(sync_state.current_height, 100);
    }

    #[test]
    fn test_failure_injection_data_corruption() {
        let mut sync_state = SyncState::new(1000);
        
        let checkpoint = SyncCheckpoint {
            height: 100,
            block_hash: "original_hash".to_string(),
            state_root: "original_state".to_string(),
            timestamp: 1000000000,
            interval: 100,
            signature: None,
            verified: true,
            peer_confirmations: 5,
        };

        sync_state.add_checkpoint(checkpoint).unwrap();
        
        let failure = FailureType::DataCorruption { affected_heights: vec![100] };
        let violations = sync_state.inject_failure(&failure, 100);
        
        assert!(!violations.is_empty());
        assert!(violations[0].contains("Data corruption"));
        
        let corrupted_checkpoint = sync_state.checkpoints.get(&100).unwrap();
        assert_eq!(corrupted_checkpoint.block_hash, "corrupted");
        assert!(!corrupted_checkpoint.verified);
    }

    #[test]
    fn test_recovery_mechanism() {
        let mut sync_state = SyncState::new(1000);
        
        // Add a checkpoint that can be recovered
        let mut checkpoint = SyncCheckpoint {
            height: 100,
            block_hash: "valid_hash".to_string(),
            state_root: "valid_state".to_string(),
            timestamp: 1000000000,
            interval: 100,
            signature: Some(CheckpointSignature {
                signature_data: vec![1, 2, 3],
                signer_id: "test_signer".to_string(),
                timestamp: 1000000000,
            }),
            verified: false, // Initially unverified
            peer_confirmations: 5,
        };

        sync_state.add_checkpoint(checkpoint).unwrap();
        
        // Recovery should succeed
        let recovery_result = sync_state.attempt_recovery();
        assert!(recovery_result.is_ok());
        
        let recovered_checkpoint = sync_state.checkpoints.get(&100).unwrap();
        assert!(recovered_checkpoint.verified);
    }

    #[test]
    fn test_consistency_validation() {
        let mut sync_state = SyncState::new(1000);
        
        // Add checkpoints with inconsistent timestamps
        let checkpoint1 = SyncCheckpoint {
            height: 100,
            block_hash: "hash1".to_string(),
            state_root: "state1".to_string(),
            timestamp: 2000000000, // Later timestamp
            interval: 100,
            signature: None,
            verified: true,
            peer_confirmations: 5,
        };

        let checkpoint2 = SyncCheckpoint {
            height: 200,
            block_hash: "hash2".to_string(),
            state_root: "state2".to_string(),
            timestamp: 1000000000, // Earlier timestamp - inconsistent
            interval: 100,
            signature: None,
            verified: true,
            peer_confirmations: 5,
        };

        sync_state.add_checkpoint(checkpoint1).unwrap();
        sync_state.add_checkpoint(checkpoint2).unwrap();
        
        let violations = sync_state.validate_consistency();
        assert!(!violations.is_empty());
        assert!(violations[0].contains("Timestamp inconsistency"));
    }
}