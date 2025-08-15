# ALYS-015: Governance Cutover and Local Key Removal

## Issue Type
Task

## Priority
Critical

## Story Points
8

## Sprint
Migration Sprint 7

## Component
Governance Integration

## Labels
`migration`, `phase-7`, `governance`, `security`, `cutover`

## Description

Execute the final cutover from local key management to Anduro Governance HSM. This includes transitioning signature authority, securely removing local keys, and ensuring zero disruption to peg operations during the transition.

## Acceptance Criteria

- [ ] Governance signing fully operational
- [ ] Local keys securely removed
- [ ] Zero peg operation failures during transition
- [ ] Emergency rollback plan tested
- [ ] Audit trail of key removal complete
- [ ] All federation members synchronized
- [ ] P2WSH addresses updated
- [ ] 48-hour stability period achieved

## Technical Details

### Implementation Steps

1. **Pre-Cutover Validation**
```rust
// src/governance/cutover_validator.rs

use std::collections::HashMap;

pub struct CutoverValidator {
    stream_actor: Addr<StreamActor>,
    bridge_actor: Addr<BridgeActor>,
    local_signer: Arc<LocalSigner>,
    metrics: CutoverMetrics,
}

impl CutoverValidator {
    pub async fn validate_readiness(&self) -> Result<CutoverReadiness, CutoverError> {
        info!("Starting governance cutover readiness validation");
        
        let mut readiness = CutoverReadiness::default();
        
        // Check 1: Governance connection stable
        readiness.governance_connection = self.validate_governance_connection().await?;
        
        // Check 2: Parallel validation success rate
        readiness.validation_success_rate = self.check_parallel_validation_metrics().await?;
        
        // Check 3: All federation members ready
        readiness.federation_ready = self.validate_federation_readiness().await?;
        
        // Check 4: Recent successful pegouts via governance
        readiness.recent_governance_pegouts = self.check_recent_governance_operations().await?;
        
        // Check 5: Emergency procedures ready
        readiness.emergency_procedures = self.validate_emergency_procedures().await?;
        
        // Check 6: Backup systems operational
        readiness.backup_systems = self.validate_backup_systems().await?;
        
        if readiness.is_ready() {
            info!("✅ All cutover readiness checks passed");
            Ok(readiness)
        } else {
            warn!("❌ Cutover readiness checks failed: {:?}", readiness.get_failures());
            Err(CutoverError::NotReady(readiness.get_failures()))
        }
    }
    
    async fn validate_governance_connection(&self) -> Result<ConnectionCheck, CutoverError> {
        let status = self.stream_actor
            .send(GetConnectionStatus)
            .await??;
        
        let uptime_hours = status.connection_uptime.as_secs() / 3600;
        let stable = status.connected && uptime_hours >= 24;
        
        Ok(ConnectionCheck {
            connected: status.connected,
            uptime: status.connection_uptime,
            stable,
            recent_disconnects: status.reconnect_count,
            passed: stable && status.reconnect_count == 0,
        })
    }
    
    async fn check_parallel_validation_metrics(&self) -> Result<ValidationMetrics, CutoverError> {
        let metrics = PARALLEL_VALIDATION_METRICS.collect();
        
        let total_validations = metrics.matches + metrics.mismatches;
        let success_rate = if total_validations > 0 {
            metrics.matches as f64 / total_validations as f64
        } else {
            0.0
        };
        
        Ok(ValidationMetrics {
            total_validations,
            success_rate,
            recent_failures: metrics.recent_failures,
            passed: success_rate >= 0.999 && total_validations >= 10000,
        })
    }
    
    async fn validate_federation_readiness(&self) -> Result<FederationReadiness, CutoverError> {
        // Query all federation members
        let members = self.stream_actor
            .send(GetFederationMembers)
            .await??;
        
        let mut member_status = HashMap::new();
        
        for member in &members {
            let ready = self.check_member_readiness(member).await?;
            member_status.insert(member.id.clone(), ready);
        }
        
        let all_ready = member_status.values().all(|&ready| ready);
        let ready_count = member_status.values().filter(|&&ready| ready).count();
        
        Ok(FederationReadiness {
            total_members: members.len(),
            ready_members: ready_count,
            member_status,
            threshold_met: ready_count >= members.len() * 2 / 3, // 2/3 threshold
            passed: all_ready,
        })
    }
}

#[derive(Debug, Default)]
pub struct CutoverReadiness {
    pub governance_connection: ConnectionCheck,
    pub validation_success_rate: ValidationMetrics,
    pub federation_ready: FederationReadiness,
    pub recent_governance_pegouts: RecentOperations,
    pub emergency_procedures: EmergencyCheck,
    pub backup_systems: BackupCheck,
}

impl CutoverReadiness {
    pub fn is_ready(&self) -> bool {
        self.governance_connection.passed &&
        self.validation_success_rate.passed &&
        self.federation_ready.passed &&
        self.recent_governance_pegouts.passed &&
        self.emergency_procedures.passed &&
        self.backup_systems.passed
    }
    
    pub fn get_failures(&self) -> Vec<String> {
        let mut failures = Vec::new();
        
        if !self.governance_connection.passed {
            failures.push("Governance connection unstable".to_string());
        }
        if !self.validation_success_rate.passed {
            failures.push(format!("Validation success rate too low: {:.2}%", 
                self.validation_success_rate.success_rate * 100.0));
        }
        if !self.federation_ready.passed {
            failures.push(format!("Only {}/{} federation members ready",
                self.federation_ready.ready_members,
                self.federation_ready.total_members));
        }
        
        failures
    }
}
```

2. **Implement Cutover Controller**
```rust
// src/governance/cutover_controller.rs

use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GovernanceCutoverController {
    validator: CutoverValidator,
    bridge_actor: Addr<BridgeActor>,
    key_manager: Arc<RwLock<KeyManager>>,
    state: Arc<RwLock<CutoverState>>,
    audit_logger: AuditLogger,
    emergency_rollback: EmergencyRollback,
}

#[derive(Debug, Clone)]
pub enum CutoverState {
    PreCutover,
    ValidatingReadiness,
    TransitioningAuthority,
    RemovingLocalKeys,
    Monitoring { since: Instant },
    Complete,
    RolledBack { reason: String },
}

impl GovernanceCutoverController {
    pub async fn execute_cutover(&mut self) -> Result<CutoverReport, CutoverError> {
        info!("🔐 Starting governance cutover process");
        
        let mut report = CutoverReport::new();
        *self.state.write().await = CutoverState::ValidatingReadiness;
        
        // Step 1: Validate readiness
        let readiness = self.validator.validate_readiness().await?;
        report.readiness_check = Some(readiness);
        
        if !readiness.is_ready() {
            return Err(CutoverError::NotReady(readiness.get_failures()));
        }
        
        // Step 2: Pause peg operations
        info!("Pausing peg operations for cutover");
        self.bridge_actor.send(PausePegOperations).await??;
        report.operations_paused_at = Some(Instant::now());
        
        // Step 3: Transition signing authority
        *self.state.write().await = CutoverState::TransitioningAuthority;
        self.transition_signing_authority().await?;
        report.authority_transitioned = true;
        
        // Step 4: Verify governance signing
        self.verify_governance_signing().await?;
        report.governance_verified = true;
        
        // Step 5: Remove local keys
        *self.state.write().await = CutoverState::RemovingLocalKeys;
        let removal_report = self.remove_local_keys().await?;
        report.key_removal = Some(removal_report);
        
        // Step 6: Resume operations
        self.bridge_actor.send(ResumePegOperations).await??;
        report.operations_resumed_at = Some(Instant::now());
        
        // Step 7: Monitor stability
        *self.state.write().await = CutoverState::Monitoring { since: Instant::now() };
        self.monitor_stability(Duration::from_hours(48)).await?;
        
        *self.state.write().await = CutoverState::Complete;
        info!("✅ Governance cutover completed successfully");
        
        Ok(report)
    }
    
    async fn transition_signing_authority(&mut self) -> Result<(), CutoverError> {
        info!("Transitioning signing authority to governance");
        
        // Update bridge actor to use governance only
        self.bridge_actor
            .send(SetSigningMode(SigningMode::GovernanceOnly))
            .await??;
        
        // Disable local signer
        self.key_manager.write().await.disable_signing()?;
        
        // Log transition
        self.audit_logger.log(AuditEvent::AuthorityTransitioned {
            from: "Local".to_string(),
            to: "Governance".to_string(),
            timestamp: Utc::now(),
        }).await;
        
        Ok(())
    }
    
    async fn remove_local_keys(&mut self) -> Result<KeyRemovalReport, CutoverError> {
        info!("Starting secure key removal process");
        
        let mut report = KeyRemovalReport::default();
        let key_manager = self.key_manager.write().await;
        
        // Step 1: Export keys for emergency recovery (encrypted)
        let encrypted_backup = key_manager.export_encrypted_backup()?;
        report.backup_created = true;
        report.backup_hash = calculate_sha256(&encrypted_backup);
        
        // Step 2: Overwrite key material in memory
        let keys_removed = key_manager.secure_wipe_keys()?;
        report.keys_removed = keys_removed;
        
        // Step 3: Remove key files from disk
        let files_removed = self.remove_key_files().await?;
        report.files_removed = files_removed;
        
        // Step 4: Verify removal
        let verification = self.verify_key_removal().await?;
        report.verification_passed = verification;
        
        // Step 5: Log removal
        self.audit_logger.log(AuditEvent::KeysRemoved {
            count: keys_removed,
            backup_hash: report.backup_hash.clone(),
            timestamp: Utc::now(),
            verified: verification,
        }).await;
        
        info!("Key removal complete: {} keys removed", keys_removed);
        
        Ok(report)
    }
    
    async fn remove_key_files(&self) -> Result<Vec<PathBuf>, CutoverError> {
        let key_dirs = vec![
            PathBuf::from("/var/lib/alys/keys"),
            PathBuf::from("/etc/alys/keys"),
            PathBuf::from("/home/alys/.alys/keys"),
        ];
        
        let mut removed_files = Vec::new();
        
        for dir in key_dirs {
            if dir.exists() {
                // Find all key files
                let key_files = glob::glob(&format!("{}/**/*.key", dir.display()))?
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>();
                
                for file in key_files {
                    // Securely overwrite file
                    secure_delete_file(&file).await?;
                    removed_files.push(file);
                }
                
                // Remove directory
                tokio::fs::remove_dir_all(&dir).await?;
            }
        }
        
        Ok(removed_files)
    }
    
    async fn verify_key_removal(&self) -> Result<bool, CutoverError> {
        // Check memory for key material
        let memory_clear = !self.key_manager.read().await.has_keys();
        
        // Check filesystem
        let filesystem_clear = !self.any_key_files_exist().await;
        
        // Try to sign with local keys (should fail)
        let signing_disabled = self.test_local_signing_fails().await;
        
        Ok(memory_clear && filesystem_clear && signing_disabled)
    }
    
    async fn monitor_stability(&self, duration: Duration) -> Result<(), CutoverError> {
        info!("Monitoring stability for {:?}", duration);
        
        let start = Instant::now();
        let mut check_interval = Duration::from_secs(300); // 5 minutes
        
        while start.elapsed() < duration {
            // Check system health
            let health = self.check_system_health().await?;
            
            if !health.is_healthy() {
                warn!("Health check failed during monitoring: {:?}", health);
                
                if health.is_critical() {
                    error!("Critical issue detected, initiating emergency rollback");
                    return self.emergency_rollback.execute().await;
                }
            }
            
            // Check for successful operations
            let operations = self.check_recent_operations().await?;
            if operations.failures > 0 {
                warn!("{} operation failures detected", operations.failures);
            }
            
            // Log progress
            let elapsed = start.elapsed();
            let remaining = duration - elapsed;
            info!("Stability monitoring: {:?} elapsed, {:?} remaining", elapsed, remaining);
            
            tokio::time::sleep(check_interval).await;
        }
        
        info!("✅ Stability monitoring completed successfully");
        Ok(())
    }
}
```

3. **Implement Emergency Rollback**
```rust
// src/governance/emergency_rollback.rs

pub struct EmergencyRollback {
    encrypted_keys: Arc<RwLock<Option<Vec<u8>>>>,
    bridge_actor: Addr<BridgeActor>,
    key_manager: Arc<RwLock<KeyManager>>,
    audit_logger: AuditLogger,
}

impl EmergencyRollback {
    pub async fn execute(&self) -> Result<(), CutoverError> {
        error!("🚨 EMERGENCY ROLLBACK INITIATED");
        
        // Step 1: Pause all operations
        self.bridge_actor.send(PausePegOperations).await??;
        
        // Step 2: Restore local keys from backup
        if let Some(encrypted_backup) = self.encrypted_keys.read().await.as_ref() {
            info!("Restoring keys from encrypted backup");
            
            // Decrypt with threshold of operators
            let decrypted = self.decrypt_with_threshold(encrypted_backup).await?;
            
            // Restore to key manager
            self.key_manager.write().await.restore_keys(decrypted)?;
            
            info!("Keys restored successfully");
        } else {
            return Err(CutoverError::NoBackupAvailable);
        }
        
        // Step 3: Switch back to local signing
        self.bridge_actor
            .send(SetSigningMode(SigningMode::LocalOnly))
            .await??;
        
        // Step 4: Verify local signing works
        self.verify_local_signing().await?;
        
        // Step 5: Resume operations
        self.bridge_actor.send(ResumePegOperations).await??;
        
        // Step 6: Log rollback
        self.audit_logger.log(AuditEvent::EmergencyRollback {
            timestamp: Utc::now(),
            reason: "Critical issue during cutover".to_string(),
        }).await;
        
        warn!("Emergency rollback completed - system using local keys");
        
        Ok(())
    }
    
    async fn decrypt_with_threshold(&self, encrypted: &[u8]) -> Result<Vec<u8>, CutoverError> {
        // Require M of N operators to provide decryption shares
        // This ensures no single operator can decrypt alone
        
        let threshold = 3; // Require 3 of 5 operators
        let mut shares = Vec::new();
        
        // Request shares from operators (would be interactive in production)
        for i in 0..threshold {
            let share = self.request_operator_share(i).await?;
            shares.push(share);
        }
        
        // Combine shares to decrypt
        let decrypted = shamir::combine_shares(shares)?;
        
        Ok(decrypted)
    }
}
```

4. **Secure Key Deletion**
```rust
// src/governance/secure_delete.rs

use rand::RngCore;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, AsyncSeekExt};

pub async fn secure_delete_file(path: &Path) -> Result<(), std::io::Error> {
    let metadata = tokio::fs::metadata(path).await?;
    let file_size = metadata.len();
    
    // Open file for writing
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .await?;
    
    // Pass 1: Overwrite with zeros
    file.seek(std::io::SeekFrom::Start(0)).await?;
    let zeros = vec![0u8; file_size as usize];
    file.write_all(&zeros).await?;
    file.sync_all().await?;
    
    // Pass 2: Overwrite with ones
    file.seek(std::io::SeekFrom::Start(0)).await?;
    let ones = vec![0xFFu8; file_size as usize];
    file.write_all(&ones).await?;
    file.sync_all().await?;
    
    // Pass 3: Overwrite with random data
    file.seek(std::io::SeekFrom::Start(0)).await?;
    let mut random_data = vec![0u8; file_size as usize];
    rand::thread_rng().fill_bytes(&mut random_data);
    file.write_all(&random_data).await?;
    file.sync_all().await?;
    
    // Close file
    drop(file);
    
    // Delete the file
    tokio::fs::remove_file(path).await?;
    
    Ok(())
}

pub struct KeyManager {
    keys: Arc<RwLock<HashMap<String, SensitiveKey>>>,
    signing_enabled: Arc<AtomicBool>,
}

impl KeyManager {
    pub fn secure_wipe_keys(&mut self) -> Result<usize, CutoverError> {
        let mut keys = self.keys.write().unwrap();
        let count = keys.len();
        
        // Overwrite each key in memory
        for (_, key) in keys.iter_mut() {
            key.secure_wipe();
        }
        
        // Clear the hashmap
        keys.clear();
        
        // Force garbage collection (hint to runtime)
        drop(keys);
        
        // Disable signing
        self.signing_enabled.store(false, Ordering::SeqCst);
        
        Ok(count)
    }
}

pub struct SensitiveKey {
    data: Vec<u8>,
}

impl SensitiveKey {
    pub fn secure_wipe(&mut self) {
        // Overwrite with random data multiple times
        for _ in 0..3 {
            rand::thread_rng().fill_bytes(&mut self.data);
        }
        
        // Final overwrite with zeros
        self.data.iter_mut().for_each(|byte| *byte = 0);
        
        // Clear the vector
        self.data.clear();
        self.data.shrink_to_fit();
    }
}

impl Drop for SensitiveKey {
    fn drop(&mut self) {
        self.secure_wipe();
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
    async fn test_cutover_readiness_validation() {
        let validator = create_test_validator();
        
        // Set up good conditions
        setup_successful_parallel_validation();
        
        let readiness = validator.validate_readiness().await.unwrap();
        assert!(readiness.is_ready());
    }
    
    #[tokio::test]
    async fn test_key_removal() {
        let controller = create_test_controller();
        
        // Create test keys
        create_test_keys();
        
        let report = controller.remove_local_keys().await.unwrap();
        
        assert!(report.verification_passed);
        assert!(report.keys_removed > 0);
        
        // Verify keys are gone
        assert!(!key_files_exist());
        assert!(test_signing_fails().await);
    }
    
    #[tokio::test]
    async fn test_emergency_rollback() {
        let rollback = create_test_rollback();
        
        // Simulate emergency
        rollback.execute().await.unwrap();
        
        // Verify local signing restored
        assert!(test_local_signing_works().await);
    }
    
    #[tokio::test]
    async fn test_secure_file_deletion() {
        let test_file = "/tmp/test_key.key";
        tokio::fs::write(test_file, b"secret_key_material").await.unwrap();
        
        secure_delete_file(Path::new(test_file)).await.unwrap();
        
        assert!(!Path::new(test_file).exists());
    }
}
```

### Integration Tests
1. Full cutover simulation
2. Emergency rollback drill
3. Multi-node federation sync
4. Peg operation continuity
5. Audit trail verification

## Dependencies

### Blockers
- ALYS-013: Parallel validation must show >99.9% success

### Blocked By
None

### Related Issues
- ALYS-016: Update security documentation
- ALYS-017: Federation member coordination

## Definition of Done

- [ ] Cutover readiness validated
- [ ] Governance signing active
- [ ] Local keys securely removed
- [ ] 48-hour stability achieved
- [ ] Emergency procedures tested
- [ ] Audit trail complete
- [ ] Documentation updated
- [ ] Security review passed
- [ ] Team trained on new procedures

## Notes

- Schedule during maintenance window
- Have security team on standby
- Backup encrypted keys to multiple locations
- Consider key ceremony for threshold decryption
- Update incident response procedures

## Time Tracking

- Estimated: 2 days (including monitoring)
- Actual: _To be filled_