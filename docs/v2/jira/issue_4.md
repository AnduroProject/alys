# ALYS-004: Implement Feature Flag System

## Issue Type
Task

## Priority
Critical

## Sprint
Migration Sprint 1

## Component
Infrastructure

## Labels
`alys`, `v2`

## Description

Implement a robust feature flag system that allows gradual rollout of migration changes, A/B testing, and instant rollback capabilities. This system is critical for safely deploying changes throughout the migration process.

## Acceptance Criteria

## Detailed Implementation Subtasks (12 tasks across 4 phases)

### Phase 1: Core Feature Flag System (4 tasks)
- [ ] **ALYS-004-01**: Design `FeatureFlag` data structure with rollout percentages, targeting, and conditional logic
- [ ] **ALYS-004-02**: Implement `FeatureFlagManager` with configuration loading, flag evaluation, and caching
- [ ] **ALYS-004-03**: Create `EvaluationContext` with node identity, environment, chain state, and custom attributes
- [ ] **ALYS-004-04**: Implement flag evaluation algorithm with conditions, targets, and percentage-based rollouts

### Phase 2: Configuration & Hot Reload (3 tasks)
- [ ] **ALYS-004-05**: Create TOML configuration file structure with feature definitions and metadata
- [ ] **ALYS-004-06**: Implement file watcher system with hot-reload capability without application restart
- [ ] **ALYS-004-07**: Add configuration validation with schema checking and error reporting

### Phase 3: Performance & Caching (3 tasks)
- [ ] **ALYS-004-08**: Implement `feature_enabled!` macro with 5-second caching to minimize performance impact
- [ ] **ALYS-004-09**: Create hash-based context evaluation for consistent percentage rollouts
- [ ] **ALYS-004-10**: Add performance benchmarking with <1ms target per flag check

### Phase 4: Basic Logging & Metrics Integration (2 tasks)
- [ ] **ALYS-004-11**: Add basic audit logging for flag changes detected through file watcher
- [ ] **ALYS-004-12**: Integrate with metrics system for flag usage tracking and evaluation performance monitoring

## Original Acceptance Criteria
- [ ] Feature flag configuration file structure defined
- [ ] Runtime feature flag evaluation implemented
- [ ] Hot-reload capability for flag changes without restart
- [ ] Percentage-based rollout support
- [ ] User/node targeting capabilities
- [ ] Audit log for flag changes
- [ ] Performance impact < 1ms per flag check
- [ ] Integration with monitoring system

## Technical Details

### Implementation Steps

1. **Define Feature Flag Configuration**
```rust
// src/features/mod.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    pub name: String,
    pub enabled: bool,
    pub rollout_percentage: Option<u8>,
    pub targets: Option<FeatureTargets>,
    pub conditions: Option<Vec<FeatureCondition>>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureTargets {
    pub node_ids: Option<Vec<String>>,
    pub validator_keys: Option<Vec<String>>,
    pub ip_ranges: Option<Vec<String>>,
    pub environments: Option<Vec<Environment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureCondition {
    After(DateTime<Utc>),
    Before(DateTime<Utc>),
    ChainHeight(u64),
    SyncProgress(f64),
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Environment {
    Development,
    Testnet,
    Mainnet,
    Canary,
}

pub struct FeatureFlagManager {
    flags: Arc<RwLock<HashMap<String, FeatureFlag>>>,
    config_path: PathBuf,
    watcher: Option<FileWatcher>,
    audit_log: AuditLogger,
}

impl FeatureFlagManager {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let flags = Self::load_flags(&config_path)?;
        
        Ok(Self {
            flags: Arc::new(RwLock::new(flags)),
            config_path: config_path.clone(),
            watcher: None,
            audit_log: AuditLogger::new(),
        })
    }
    
    pub async fn start_watching(&mut self) -> Result<()> {
        let flags = self.flags.clone();
        let path = self.config_path.clone();
        let audit_log = self.audit_log.clone();
        
        let watcher = FileWatcher::new(path.clone(), move |event| {
            if let FileEvent::Modified = event {
                let flags = flags.clone();
                let path = path.clone();
                let audit_log = audit_log.clone();
                
                tokio::spawn(async move {
                    if let Ok(new_flags) = Self::load_flags(&path) {
                        let mut flags_guard = flags.write().await;
                        
                        // Log changes
                        for (name, flag) in &new_flags {
                            if let Some(old_flag) = flags_guard.get(name) {
                                if old_flag.enabled != flag.enabled {
                                    audit_log.log_change(name, old_flag, flag).await;
                                }
                            }
                        }
                        
                        *flags_guard = new_flags;
                        info!("Feature flags reloaded from {}", path.display());
                    }
                });
            }
        })?;
        
        self.watcher = Some(watcher);
        Ok(())
    }
    
    pub async fn is_enabled(&self, flag_name: &str, context: &EvaluationContext) -> bool {
        let flags = self.flags.read().await;
        
        if let Some(flag) = flags.get(flag_name) {
            self.evaluate_flag(flag, context).await
        } else {
            false
        }
    }
    
    async fn evaluate_flag(&self, flag: &FeatureFlag, context: &EvaluationContext) -> bool {
        // Check if globally disabled
        if !flag.enabled {
            return false;
        }
        
        // Check conditions
        if let Some(conditions) = &flag.conditions {
            for condition in conditions {
                if !self.evaluate_condition(condition, context).await {
                    return false;
                }
            }
        }
        
        // Check targets
        if let Some(targets) = &flag.targets {
            if !self.matches_target(targets, context) {
                return false;
            }
        }
        
        // Check rollout percentage
        if let Some(percentage) = flag.rollout_percentage {
            let hash = self.hash_context(context);
            let threshold = (percentage as f64 / 100.0 * u64::MAX as f64) as u64;
            return hash < threshold;
        }
        
        true
    }
    
    fn hash_context(&self, context: &EvaluationContext) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        context.node_id.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Debug, Clone)]
pub struct EvaluationContext {
    pub node_id: String,
    pub environment: Environment,
    pub chain_height: u64,
    pub sync_progress: f64,
    pub validator_key: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub custom_attributes: HashMap<String, String>,
}
```

2. **Create Feature Flag Configuration File**
```toml
# config/features.toml

[features.actor_system]
enabled = false
rollout_percentage = 0
description = "Enable actor-based architecture"
metadata = { risk = "high", owner = "platform-team" }

[features.actor_system.conditions]
after = "2024-02-01T00:00:00Z"
chain_height = 1000000

[features.improved_sync]
enabled = false
rollout_percentage = 0
description = "Use improved sync algorithm"
metadata = { risk = "medium", owner = "sync-team" }

[features.improved_sync.targets]
environments = ["testnet", "canary"]

[features.lighthouse_v5]
enabled = false
rollout_percentage = 0
description = "Use Lighthouse v5 instead of v4"
metadata = { risk = "high", owner = "consensus-team" }

[features.governance_integration]
enabled = false
description = "Enable Anduro Governance integration"
metadata = { risk = "critical", owner = "security-team" }

[features.parallel_validation]
enabled = true
rollout_percentage = 100
description = "Enable parallel block validation"
metadata = { risk = "low", owner = "performance-team" }
```

3. **Implement Feature Flag Checks**
```rust
// src/features/checks.rs

/// Macro for checking feature flags with caching
#[macro_export]
macro_rules! feature_enabled {
    ($flag:expr) => {{
        use once_cell::sync::Lazy;
        use std::time::{Duration, Instant};
        use tokio::sync::RwLock;
        
        static CACHE: Lazy<RwLock<(bool, Instant)>> = Lazy::new(|| {
            RwLock::new((false, Instant::now() - Duration::from_secs(60)))
        });
        
        let cache = CACHE.read().await;
        if cache.1.elapsed() < Duration::from_secs(5) {
            cache.0
        } else {
            drop(cache);
            let mut cache = CACHE.write().await;
            let context = get_evaluation_context().await;
            let enabled = FEATURE_FLAGS.is_enabled($flag, &context).await;
            *cache = (enabled, Instant::now());
            enabled
        }
    }};
}

// Usage in code
impl ChainActor {
    pub async fn process_block(&mut self, block: Block) -> Result<()> {
        if feature_enabled!("parallel_validation").await {
            self.process_block_parallel(block).await
        } else {
            self.process_block_sequential(block).await
        }
    }
}
```

4. **Implement A/B Testing Support**
```rust
// src/features/ab_testing.rs

pub struct ABTestManager {
    tests: Arc<RwLock<HashMap<String, ABTest>>>,
    metrics: ABTestMetrics,
}

#[derive(Debug, Clone)]
pub struct ABTest {
    pub name: String,
    pub variants: Vec<Variant>,
    pub allocation: AllocationStrategy,
    pub metrics: Vec<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub percentage: u8,
    pub feature_overrides: HashMap<String, bool>,
}

impl ABTestManager {
    pub async fn get_variant(&self, test_name: &str, context: &EvaluationContext) -> Option<String> {
        let tests = self.tests.read().await;
        
        if let Some(test) = tests.get(test_name) {
            // Check if test is active
            let now = Utc::now();
            if now < test.start_time || test.end_time.map(|end| now > end).unwrap_or(false) {
                return None;
            }
            
            // Determine variant based on allocation
            let hash = self.hash_for_allocation(context, test_name);
            let mut cumulative = 0u8;
            
            for variant in &test.variants {
                cumulative += variant.percentage;
                if hash < (cumulative as f64 / 100.0 * u64::MAX as f64) as u64 {
                    // Track assignment
                    self.metrics.record_assignment(test_name, &variant.name).await;
                    return Some(variant.name.clone());
                }
            }
        }
        
        None
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
    async fn test_feature_flag_evaluation() {
        let manager = FeatureFlagManager::new("test-features.toml".into()).unwrap();
        
        let context = EvaluationContext {
            node_id: "test-node".to_string(),
            environment: Environment::Testnet,
            chain_height: 1000,
            sync_progress: 0.5,
            validator_key: None,
            ip_address: None,
            custom_attributes: HashMap::new(),
        };
        
        // Test disabled flag
        assert!(!manager.is_enabled("disabled_feature", &context).await);
        
        // Test enabled flag
        assert!(manager.is_enabled("enabled_feature", &context).await);
        
        // Test percentage rollout
        let mut enabled_count = 0;
        for i in 0..1000 {
            let mut ctx = context.clone();
            ctx.node_id = format!("node-{}", i);
            if manager.is_enabled("fifty_percent_feature", &ctx).await {
                enabled_count += 1;
            }
        }
        assert!((450..550).contains(&enabled_count)); // ~50% should be enabled
    }
    
    #[tokio::test]
    async fn test_hot_reload() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();
        
        // Write initial config
        std::fs::write(&path, r#"
            [features.test_flag]
            enabled = false
        "#).unwrap();
        
        let mut manager = FeatureFlagManager::new(path.clone()).unwrap();
        manager.start_watching().await.unwrap();
        
        let context = EvaluationContext::default();
        assert!(!manager.is_enabled("test_flag", &context).await);
        
        // Update config
        std::fs::write(&path, r#"
            [features.test_flag]
            enabled = true
        "#).unwrap();
        
        // Wait for reload
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        assert!(manager.is_enabled("test_flag", &context).await);
    }
}
```

### Integration Tests
1. Test feature flag changes during runtime
2. Verify rollout percentages are accurate
3. Test targeting specific nodes
4. Validate audit logging

### Performance Tests
```rust
#[bench]
fn bench_feature_flag_check(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let manager = FeatureFlagManager::new("features.toml".into()).unwrap();
    let context = EvaluationContext::default();
    
    b.iter(|| {
        runtime.block_on(async {
            black_box(manager.is_enabled("test_flag", &context).await)
        })
    });
}
```

## Dependencies

### Blockers
None

### Blocked By
- ALYS-003: Metrics needed for flag evaluation tracking

### Related Issues
- ALYS-005: CI/CD integration with feature flags
- All migration phase tickets depend on this

## Definition of Done

- [ ] Feature flag system implemented and tested
- [ ] Hot reload working without restart
- [ ] Audit logging implemented
- [ ] Performance benchmarks met (< 1ms)
- [ ] Documentation complete
- [ ] Integration with deployment pipeline

## Notes

- Consider using LaunchDarkly or similar for production
- Implement gradual rollout strategies (canary, blue-green)
- Add support for complex targeting rules
- Consider feature flag inheritance/dependencies

## Time Tracking

**Time Estimate**: 1.5-2 days (12-16 hours total) with detailed breakdown:
- Phase 1 - Core feature flag system: 4-5 hours (includes data structures, manager implementation, evaluation algorithm)
- Phase 2 - Configuration & hot reload: 3-4 hours (includes TOML parsing, file watching, validation)
- Phase 3 - Performance & caching: 3-4 hours (includes macro creation, caching system, benchmarking)
- Phase 4 - Basic logging & metrics integration: 2-3 hours (includes audit logging, metrics integration)

**Critical Path Dependencies**: Phase 1 → Phase 2 → Phase 3 → Phase 4
**Resource Requirements**: 1 Rust developer with configuration management experience
**Risk Buffer**: 20% additional time for file watcher edge cases and performance optimization
**Prerequisites**: ALYS-003 metrics system for flag usage tracking
**Performance Target**: <1ms per flag check, <5ms for hot reload
**Note**: Simplified approach using file-based configuration management instead of web UI/API

- Actual: _To be filled_