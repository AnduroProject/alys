# Lighthouse Upgrade Implementation Plan

## Executive Summary

This document outlines a comprehensive strategy for upgrading Alys from Lighthouse v4.5.0 (commit 441fc16) to v7.1.0, with a future-proof architecture that can handle subsequent upgrades seamlessly. The plan uses a Facade pattern with crate consolidation to minimize risk and maximize maintainability.

## Current State Analysis

### Version Gap
- **Current**: Lighthouse v4.5.0 (commit 441fc16, ~September 2023)
- **Target**: Lighthouse v7.1.0 (latest stable, January 2025)
- **Gap**: ~2.5 years, 3 major versions (v4 → v5 → v6 → v7)

### Integration Scope
- **Deep Integration**: 59 files across app/src/ using Lighthouse components
- **Three Existing Crates**:
  - `lighthouse_wrapper` (6 lines) - Simple re-export wrapper
  - `lighthouse_wrapper_v2` (2,431+ lines) - Enhanced v5-ready wrapper
  - `lighthouse_compat` (3,000+ lines) - V4→V5 compatibility layer

### Key Dependencies
```toml
execution_layer = { git = "https://github.com/sigp/lighthouse", rev = "441fc16" }
sensitive_url = { git = "https://github.com/sigp/lighthouse", rev = "441fc16" }
types = { git = "https://github.com/sigp/lighthouse", rev = "441fc16" }
store = { git = "https://github.com/sigp/lighthouse", rev = "441fc16" }
bls = { git = "https://github.com/sigp/lighthouse", rev = "441fc16" }
```

### Lighthouse Workspace Structure
Lighthouse uses a **monorepo workspace** structure with individual crates:
```
lighthouse/
├── Cargo.toml (workspace)
├── consensus/
│   ├── types/          # Published as "types" crate
│   ├── store/          # Published as "store" crate
│   └── ...
├── beacon_node/
│   ├── execution_layer/ # Published as "execution_layer" crate
│   └── ...
└── crypto/
    ├── bls/            # Published as "bls" crate
    └── ...
```

When specifying multiple crates from the same git repository, Cargo:
- **Clones once**: Downloads the repository only once
- **Builds selectively**: Compiles only the specified crates
- **Shares dependencies**: Common dependencies are shared between crates
- **Optimizes efficiently**: No duplicate downloads or builds

## Critical Breaking Changes (v4 → v7)

### 1. Rust Version Requirements
- **v4**: Rust 1.70+
- **v7**: Rust 1.83+ (MSRV jump)

### 2. API Breaking Changes
- **Engine API**: Updated to support Electra fork
- **Types**: Hash256, MainnetEthSpec structure changes
- **Store**: Database schema v19 → v26 (automatic migration)
- **BLS**: Cryptographic API updates

### 3. Fork Compatibility
- **Current**: Supports up to Capella/Shanghai
- **v7**: Requires support for Deneb, Electra forks

## Architecture: Facade vs Shim Pattern Analysis

### Why Facade Pattern is Superior

**Facade Pattern Chosen Because:**
1. **System Complexity**: 59 files need unified interface
2. **Version Management**: v4 → v7 → v8+ upgrades need seamless transitions
3. **Feature Evolution**: A/B testing, canary deployments, rollback coordination
4. **Maintenance**: Single facade easier than multiple shims
5. **Performance**: No translation overhead - native version calls

### Facade vs Shim Comparison

| Aspect | 🎭 Facade | 🔌 Shim |
|--------|-----------|---------|
| **Purpose** | Simplify complex system | Enable compatibility |
| **Scope** | Broad, system-wide | Narrow, interface-specific |
| **Design Goal** | Clean, unified API | Minimal code changes |
| **Maintenance** | Single point of control | Distributed compatibility logic |
| **Performance** | Optimized for current use | Translation overhead |
| **Future-Proofing** | High - abstracts complexity | Low - tied to specific versions |

## Lighthouse Facade Architecture

### Core Design

```rust
// crates/lighthouse_facade/src/lib.rs
pub mod types;      // Unified type system across versions
pub mod execution;  // Execution layer interface abstraction
pub mod compat;     // A/B testing, rollback capabilities
pub mod migration;  // Version migration logic
pub mod testing;    // Integration test framework
pub mod metrics;    // Consolidated performance metrics

pub struct LighthouseFacade {
    #[cfg(feature = "v4")]
    v4_client: lighthouse_v4::Client,
    #[cfg(feature = "v7")]
    v7_client: lighthouse_v7::Client,
    config: FacadeConfig,
}

impl LighthouseFacade {
    pub async fn new_payload(&self, payload: UnifiedPayload) -> Result<PayloadStatus, Error> {
        match self.active_version() {
            Version::V4 => {
                let v4_payload = payload.to_v4();
                let result = self.v4_client.new_payload(v4_payload).await?;
                Ok(result.to_unified())
            }
            Version::V7 => {
                let v7_payload = payload.to_v7();
                let result = self.v7_client.new_payload(v7_payload).await?;
                Ok(result.to_unified())
            }
        }
    }
}
```

### Feature Flag System

```toml
[features]
default = ["v4"]
v4 = ["lighthouse_wrapper"]
v7 = ["execution_layer", "types", "store", "bls"]
migration = ["v4", "v7", "ab-testing"]
ab-testing = ["rand", "prometheus"]
canary = ["migration", "percentage-rollout"]
```

## Implementation Plan

### Phase 1: Foundation (Week 1-2)

#### Week 1: Facade Creation
```bash
# Create new facade crate structure
mkdir -p crates/lighthouse_facade/src/{types,execution,compat,migration,testing,metrics}

# Set up Cargo.toml with feature flags
cat > crates/lighthouse_facade/Cargo.toml << 'EOF'
[package]
name = "lighthouse_facade"
version = "1.0.0"
edition = "2021"

[features]
default = ["v4"]
v4 = ["lighthouse_wrapper"]
v7 = []
migration = ["v4", "v7", "ab-testing"]
ab-testing = ["rand", "metrics"]

[dependencies]
lighthouse_wrapper = { path = "../lighthouse_wrapper", optional = true }
tokio = "1.0"
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"
prometheus = { version = "0.13", optional = true }
rand = { version = "0.8", optional = true }
EOF
```

#### Week 2: Abstraction Layer Implementation
```rust
// crates/lighthouse_facade/src/types.rs
pub mod unified {
    // Version-agnostic types
    pub type Hash256 = ethereum_types::H256;
    pub type EthSpec = dyn EthSpecTrait;
    
    // Unified payload structure
    pub struct UnifiedPayload {
        // Common fields across versions
    }
    
    impl UnifiedPayload {
        pub fn to_v4(&self) -> lighthouse_v4::Payload { /* conversion */ }
        pub fn to_v7(&self) -> lighthouse_v7::Payload { /* conversion */ }
    }
}

// crates/lighthouse_facade/src/execution.rs
pub trait ExecutionLayerInterface {
    async fn new_payload(&self, payload: UnifiedPayload) -> Result<PayloadStatus, Error>;
    async fn get_payload(&self, id: PayloadId) -> Result<UnifiedPayload, Error>;
    async fn forkchoice_updated(&self, update: ForkchoiceUpdate) -> Result<(), Error>;
}
```

### Phase 2: Feature Consolidation (Week 3-4)

#### Week 3: Consolidate Existing Crates
```rust
// Absorb lighthouse_wrapper_v2 features
// crates/lighthouse_facade/src/migration.rs
pub mod v2_migration {
    // Copy migration logic from lighthouse_wrapper_v2
    pub use super::compatibility::*;
    pub use super::testing::*;
    
    pub const COMPATIBLE_LIGHTHOUSE_VERSIONS: &[&str] = &[
        "v4.5.0", "v5.0.0", "v6.0.0", "v7.0.0", "v7.1.0"
    ];
}

// Absorb lighthouse_compat features
// crates/lighthouse_facade/src/compat.rs
pub mod ab_testing {
    // Copy A/B testing logic from lighthouse_compat
    pub struct ABTestConfig {
        pub percentage_v7: u8,  // 0-100% traffic to v7
        pub duration: Duration,
        pub metrics_collection: bool,
    }
}

pub mod rollback {
    // Copy rollback logic from lighthouse_compat
    pub struct RollbackManager {
        pub rollback_threshold: Duration, // 5 minutes
        pub health_check_interval: Duration,
        pub auto_rollback_enabled: bool,
    }
}
```

#### Week 4: Adapter Pattern Implementation
```rust
// crates/lighthouse_facade/src/adapters.rs
pub struct LighthouseAdapter {
    #[cfg(feature = "v4")]
    v4_inner: lighthouse_v4::ExecutionLayer,
    #[cfg(feature = "v7")]
    v7_inner: lighthouse_v7::ExecutionLayer,
    
    active_version: Version,
    migration_config: MigrationConfig,
}

impl ExecutionLayerInterface for LighthouseAdapter {
    async fn new_payload(&self, payload: UnifiedPayload) -> Result<PayloadStatus, Error> {
        match self.determine_version() {
            Version::V4 => {
                #[cfg(feature = "v4")]
                {
                    let v4_payload = payload.to_v4();
                    let result = self.v4_inner.new_payload(v4_payload).await?;
                    Ok(result.to_unified())
                }
                #[cfg(not(feature = "v4"))]
                Err(Error::VersionNotAvailable("v4"))
            }
            Version::V7 => {
                #[cfg(feature = "v7")]
                {
                    let v7_payload = payload.to_v7();
                    let result = self.v7_inner.new_payload(v7_payload).await?;
                    Ok(result.to_unified())
                }
                #[cfg(not(feature = "v7"))]
                Err(Error::VersionNotAvailable("v7"))
            }
        }
    }
}
```

### Phase 3: Progressive Migration (Week 5-6)

#### Week 5: Import Replacement
```bash
# Automated import replacement across all 59 files
find app/src -name "*.rs" -exec sed -i 's/lighthouse_wrapper::/lighthouse_facade::/g' {} \;
find app/src -name "*.rs" -exec sed -i 's/lighthouse_wrapper_v2::/lighthouse_facade::migration::/g' {} \;
find app/src -name "*.rs" -exec sed -i 's/lighthouse_compat::/lighthouse_facade::compat::/g' {} \;

# Update Cargo.toml dependencies
# Replace in app/Cargo.toml:
# lighthouse_wrapper = { ... }           # REMOVE
# lighthouse_wrapper_v2 = { ... }        # REMOVE  
# lighthouse_compat = { ... }            # REMOVE
# lighthouse_facade = { path = "../crates/lighthouse_facade", features = ["migration"] }  # ADD
```

#### Week 6: Feature Flag Migration
```toml
# Cargo.toml - Enable dual compatibility
[dependencies]
lighthouse_facade = { 
    path = "../crates/lighthouse_facade", 
    features = ["migration", "ab-testing"] 
}

# Test with v4 (current)
[features]
default = []
lighthouse-v4 = ["lighthouse_facade/v4"]
lighthouse-v7 = ["lighthouse_facade/v7"]
lighthouse-migration = ["lighthouse_facade/migration"]
```

### Phase 4: Version Upgrade (Week 7-8)

#### Week 7: Lighthouse v7 Integration
```toml
# Add v7 dependencies to lighthouse_facade/Cargo.toml
[dependencies]
# Current v4 dependencies (existing)
lighthouse_wrapper = { path = "../lighthouse_wrapper", optional = true }

# Lighthouse v7 dependencies (individual crates from same repository)
execution_layer = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
types = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
store = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
bls = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }

# Common dependencies
tokio = "1.0"
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"
prometheus = { version = "0.13", optional = true }
rand = { version = "0.8", optional = true }

[features]
default = ["v4"]
v4 = ["lighthouse_wrapper"]
v7 = ["execution_layer", "types", "store", "bls"]
migration = ["v4", "v7", "ab-testing"]
ab-testing = ["rand", "prometheus"]
```

**Note**: Lighthouse uses a workspace structure with individual crates. Cargo efficiently clones the repository once and builds only the specified crates, sharing common dependencies between them.

#### Week 8: Breaking Changes Resolution
```rust
// lighthouse_facade/src/v7_adapter.rs
#[cfg(feature = "v7")]
mod v7_impl {
    use execution_layer::ExecutionLayer;  // Real crate name from Lighthouse workspace
    use types::{Hash256, MainnetEthSpec, ExecutionPayload};  // Real types from Lighthouse
    use store::Store;                     // Real store crate
    use bls::PublicKey;                  // Real BLS crate
    
    pub struct V7Adapter {
        execution_layer: ExecutionLayer<MainnetEthSpec>,
        store: Store<MainnetEthSpec>,
    }
}

// Handle v7 breaking changes with correct crate references
impl UnifiedPayload {
    #[cfg(feature = "v7")]
    pub fn to_v7(&self) -> types::ExecutionPayload {
        types::ExecutionPayload {
            // Map fields, handle new Electra fields
            parent_hash: self.parent_hash,
            fee_recipient: self.fee_recipient,
            // New in v7 for Electra fork:
            deposits: self.deposits.unwrap_or_default(),
            withdrawals: self.withdrawals.unwrap_or_default(),
            // Handle other breaking changes...
        }
    }
}

// Database migration support
impl LighthouseFacade {
    pub async fn migrate_database(&self) -> Result<(), MigrationError> {
        // Handle schema v19 → v26 migration
        // Lighthouse handles this automatically, but we need to coordinate
        match self.current_schema_version().await? {
            19..=25 => {
                info!("Migrating database schema to v26 for Lighthouse v7");
                // Let Lighthouse handle migration automatically
                Ok(())
            }
            26 => {
                info!("Database already at v26, ready for Lighthouse v7");
                Ok(())
            }
            v => Err(MigrationError::UnsupportedSchemaVersion(v)),
        }
    }
}
```

### Phase 5: Production Migration (Week 9-10)

#### Week 9: Pre-Migration Validation & Final Testing
```bash
# Comprehensive pre-migration testing
echo "=== Phase 5: Hard Cut-Over Migration Preparation ==="

# 1. Final integration testing with v7
cargo test --features lighthouse-v7 --release
cargo bench --features lighthouse-v7

# 2. Database migration dry-run
cargo run --bin migration-test --features lighthouse-v7 -- --dry-run

# 3. Backup critical data
./scripts/backup_lighthouse_data.sh
cp -r crates/lighthouse_wrapper crates/lighthouse_wrapper.backup
cp -r crates/lighthouse_wrapper_v2 crates/lighthouse_wrapper_v2.backup
cp -r crates/lighthouse_compat crates/lighthouse_compat.backup

# 4. Validate rollback capability
./scripts/validate_rollback_procedure.sh
```

```rust
// Production migration configuration - hard cut-over approach
let migration_config = MigrationConfig {
    mode: MigrationMode::HardCutover, // Direct v4 → v7 switch
    pre_migration_validation: true,
    rollback_threshold: Duration::from_mins(5),
    health_monitoring: true,
    post_migration_validation: true,
};

// Hard cut-over preparation checklist:
let pre_migration_checks = vec![
    "Database backup completed",
    "Integration tests pass on v7", 
    "Performance benchmarks within 5% of v4",
    "Rollback procedure validated",
    "Monitoring dashboards ready",
    "Emergency contacts notified",
];
```

#### Week 10: Hard Cut-Over Migration & Validation
```bash
# Day 1: Execute hard cut-over migration
echo "=== Lighthouse v7 Hard Cut-Over Migration ==="

# Step 1: Final validation before migration
cargo test --features lighthouse-v4 --release  # Confirm v4 baseline
cargo test --features lighthouse-v7 --release  # Confirm v7 readiness

# Step 2: Database migration (automatic, but monitored)
echo "Starting database schema migration (v19 → v26)..."
cargo run --bin alys --features lighthouse-v7 -- --migrate-db-only
echo "Database migration completed successfully"

# Step 3: Switch build configuration to v7
echo "Switching to Lighthouse v7..."
sed -i 's/lighthouse-v4/lighthouse-v7/g' Cargo.toml
sed -i 's/default = \["v4"\]/default = ["v7"]/g' crates/lighthouse_facade/Cargo.toml

# Step 4: Build and deploy v7
cargo build --release --features lighthouse-v7
./scripts/deploy_with_health_checks.sh

# Step 5: Post-migration validation
./scripts/validate_v7_functionality.sh
./scripts/monitor_system_health.sh --duration 30m

echo "Hard cut-over migration completed successfully!"
```

```rust
// Post-migration validation suite
pub struct PostMigrationValidator {
    start_time: Instant,
    health_checks: Vec<HealthCheck>,
    performance_baselines: PerformanceBaselines,
}

impl PostMigrationValidator {
    pub async fn validate_migration(&self) -> Result<MigrationReport, MigrationError> {
        let mut report = MigrationReport::new();
        
        // Critical functionality checks
        self.validate_execution_layer().await?;
        self.validate_consensus_operations().await?;
        self.validate_database_integrity().await?;
        self.validate_performance_metrics().await?;
        
        // Success criteria
        if report.all_checks_passed() && report.performance_within_threshold() {
            info!("✅ Hard cut-over migration validated successfully");
            self.cleanup_old_dependencies().await?;
        } else {
            warn!("❌ Migration validation failed, initiating rollback");
            self.initiate_emergency_rollback().await?;
        }
        
        Ok(report)
    }
    
    async fn cleanup_old_dependencies(&self) -> Result<(), CleanupError> {
        // Remove old crate directories only after successful validation
        tokio::fs::remove_dir_all("crates/lighthouse_wrapper.backup").await?;
        tokio::fs::remove_dir_all("crates/lighthouse_wrapper_v2.backup").await?;
        tokio::fs::remove_dir_all("crates/lighthouse_compat.backup").await?;
        
        // Update workspace Cargo.toml to remove old crate references
        self.update_workspace_config().await?;
        
        info!("✅ Old Lighthouse dependencies cleaned up successfully");
        Ok(())
    }
}
```

## Future-Proofing Strategy

### 1. Versioned Abstraction Layer
```rust
pub enum LighthouseVersion {
    V4, V5, V6, V7, V8, V9, // Future versions
}

pub trait VersionedInterface {
    fn version(&self) -> LighthouseVersion;
    fn is_compatible(&self, required: LighthouseVersion) -> bool;
    fn migration_path(&self, target: LighthouseVersion) -> Vec<LighthouseVersion>;
}
```

### 2. Plugin Architecture
```rust
pub trait LighthousePlugin {
    fn name(&self) -> &str;
    fn version_range(&self) -> (LighthouseVersion, LighthouseVersion);
    fn initialize(&self, config: PluginConfig) -> Result<(), Error>;
}

pub struct PluginManager {
    plugins: HashMap<String, Box<dyn LighthousePlugin>>,
    active_version: LighthouseVersion,
}
```

### 3. Configuration-Driven Updates
```toml
# lighthouse-config.toml
[lighthouse]
version = "7.1.0"
auto_update = true
compatibility_mode = "strict" # or "permissive"

[compatibility]
allow_version_drift = false
max_version_gap = 1
fallback_version = "6.x"

[migration]
canary_percentage = 10
rollback_threshold_minutes = 5
health_check_interval_seconds = 30
```

### 4. Continuous Integration Pipeline
```yaml
# .github/workflows/lighthouse-compatibility.yml
name: Lighthouse Compatibility Matrix
on: [push, pull_request]

jobs:
  test-versions:
    strategy:
      matrix:
        lighthouse-version: [v6.x, v7.x, v8.x-nightly]
        rust-version: [1.83.0, stable, beta]
    steps:
      - name: Test with Lighthouse ${{ matrix.lighthouse-version }}
        run: |
          cargo test --features lighthouse-${{ matrix.lighthouse-version }}
          cargo test --features lighthouse-migration
      
      - name: Performance Benchmark
        run: cargo bench --features lighthouse-${{ matrix.lighthouse-version }}
```

## Risk Mitigation

### 1. Backward Compatibility
- **Feature Flags**: Allow easy rollback between versions
- **Adapter Pattern**: Isolates breaking changes
- **Database Migrations**: Reversible with backup strategy

### 2. Testing Strategy
```rust
#[cfg(test)]
mod compatibility_tests {
    #[tokio::test]
    async fn test_v4_v7_payload_conversion() {
        let v4_payload = create_test_payload_v4();
        let unified = UnifiedPayload::from_v4(v4_payload);
        let v7_payload = unified.to_v7();
        
        // Verify no data loss in conversion
        assert_eq!(v4_payload.parent_hash, v7_payload.parent_hash);
    }
    
    #[tokio::test]
    async fn test_version_switching() {
        let facade = LighthouseFacade::new(test_config()).await?;
        
        // Test switching from v4 to v7
        facade.set_version(Version::V4);
        let result_v4 = facade.new_payload(test_payload()).await?;
        
        facade.set_version(Version::V7);
        let result_v7 = facade.new_payload(test_payload()).await?;
        
        // Results should be equivalent
        assert_equivalent(result_v4, result_v7);
    }
}
```

### 3. Deployment Strategy
- **Hard Cut-Over**: Direct v4 → v7 migration with comprehensive validation
- **Pre-Migration Testing**: Extensive validation before production switch
- **Automated Rollback**: 5-minute rollback window with health monitoring
- **Post-Migration Validation**: Comprehensive functionality and performance checks

### 4. Monitoring & Alerting
```rust
// Metrics collection
pub struct LighthouseMetrics {
    pub version_usage: Histogram,           // Time spent in each version
    pub conversion_latency: Histogram,      // Type conversion overhead
    pub migration_success_rate: Counter,    // Migration success/failure
    pub rollback_triggers: Counter,         // Automatic rollbacks
}

// Alert conditions
pub struct AlertConfig {
    pub error_rate_threshold: f64,          // > 5% error rate triggers rollback
    pub latency_increase_threshold: f64,    // > 50% latency increase
    pub memory_usage_threshold: f64,        // > 90% memory usage
}
```

## Expected Outcomes

### Before Implementation
- **Status**: Stuck on v4.5.0 (September 2023)
- **Issues**: Compilation errors, security vulnerabilities, missing Electra fork support
- **Maintainability**: Three separate crates, fragmented logic
- **Future Risk**: Increasing version drift, technical debt

### After Phase 1 (Foundation)
- **Status**: Clean abstraction layer, v4 still functional
- **Benefits**: Single interface for all Lighthouse operations
- **Risk Reduction**: Isolated dependencies, rollback capability

### After Phase 2 (Consolidation) 
- **Status**: Three crates consolidated into one facade
- **Benefits**: Unified maintenance, consistent API, preserved features
- **Capabilities**: A/B testing, canary deployment, automated rollback

### After Phase 3 (Migration)
- **Status**: All imports updated, dual-compatibility ready
- **Benefits**: Zero-downtime upgrade path, feature flag control
- **Testing**: Comprehensive compatibility matrix validated

### After Phase 4 (Upgrade)
- **Status**: Full Lighthouse v7.1.0 compatibility
- **Benefits**: Electra fork support, latest security patches, performance improvements
- **Database**: Schema migrated (v19 → v26), data preserved

### After Phase 5 (Production)
- **Status**: Production-ready v7 deployment via hard cut-over
- **Benefits**: Clean migration completed, comprehensive post-migration validation
- **Reliability**: Proven rollback capability, automated health monitoring
- **Future-Ready**: Architecture ready for v8, v9, etc.

## Long-Term Benefits

1. **Efficient Cut-Over Migrations**: Hard cut-over approach with comprehensive validation
2. **Future-Proof Architecture**: Abstraction layer works for any Lighthouse version
3. **Risk Reduction**: Thorough pre-migration testing with automated rollback capability
4. **Operational Excellence**: Comprehensive monitoring and post-migration validation
5. **Development Velocity**: Single crate to maintain instead of three
6. **Cost Efficiency**: Reduced technical debt, faster future upgrades
7. **Clean State**: Hard cut-over eliminates version drift and complexity

## Success Metrics

### Technical Metrics
- **Migration Success Rate**: > 99.9%
- **Rollback Time**: < 5 minutes
- **Performance Impact**: < 5% latency increase during migration
- **Memory Usage**: No significant increase
- **Test Coverage**: > 95% for facade and compatibility layers

### Operational Metrics  
- **Downtime**: Zero planned downtime during hard cut-over migration
- **Error Rate**: < 0.1% increase during post-migration validation period
- **Migration Time**: Complete cut-over within 30 minutes
- **Rollback Capability**: Verified < 5 minute rollback time
- **Support Tickets**: No increase in lighthouse-related issues
- **Developer Productivity**: 50% reduction in lighthouse integration time

### Business Metrics
- **Security Posture**: Up-to-date with latest Lighthouse security patches
- **Compliance**: Electra fork ready for Ethereum network upgrades
- **Technical Debt**: 75% reduction in lighthouse-related technical debt
- **Future Readiness**: Ready for next 3+ Lighthouse major versions

## Common Pitfalls & Troubleshooting

### ❌ Dependency Anti-Patterns to Avoid

**Don't create multiple fake dependencies:**
```toml
# WRONG - These aren't real crate names
lighthouse_v7_execution_layer = { git = "...", tag = "v7.1.0" }
lighthouse_v7_types = { git = "...", tag = "v7.1.0" }
lighthouse_v7_store = { git = "...", tag = "v7.1.0" }
```

**Don't use package aliases for the same repository:**
```toml
# WRONG - Unnecessary complexity
types_v7 = { git = "...", tag = "v7.1.0", package = "types" }
types_v4 = { git = "...", rev = "441fc16", package = "types" }
```

### ✅ Correct Patterns

**Use individual crate names from the workspace:**
```toml
# CORRECT - Real crate names from Lighthouse workspace
execution_layer = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
types = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
store = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
bls = { git = "https://github.com/sigp/lighthouse", tag = "v7.1.0", optional = true }
```

**Use feature flags to control versions:**
```toml
[features]
v4 = ["lighthouse_wrapper"]  # Points to existing wrapper
v7 = ["execution_layer", "types", "store", "bls"]  # Real crate names
```

### 🔧 Troubleshooting Build Issues

1. **"crate not found" errors**: Verify crate names exist in the Lighthouse workspace
2. **Multiple versions conflict**: Use feature flags to select only one version at a time
3. **Dependency resolution failures**: Check that all crates use the same git tag/revision
4. **Build time issues**: Cargo should clone once and build efficiently - if not, check for duplicate dependencies

This implementation plan provides a comprehensive, risk-mitigated approach to upgrading Alys's Lighthouse integration while establishing a foundation for seamless future upgrades.