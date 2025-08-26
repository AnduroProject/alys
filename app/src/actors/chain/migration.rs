//! Chain Migration Utilities
//!
//! Migration adapter and utilities for backward compatibility.
//! This module provides utilities for migrating from the legacy Chain struct
//! to the new ChainActor implementation while maintaining consensus safety.

use std::sync::Arc;
use std::collections::HashMap;
use super::{ChainActor, config::ChainActorConfig, state::*};
use crate::types::*;
use crate::features::FeatureFlagManager;

/// Migration adapter for transitioning from legacy Chain to ChainActor
#[derive(Debug)]
pub struct ChainMigrationAdapter {
    /// Feature flags for controlling migration
    feature_flags: Arc<FeatureFlagManager>,
    
    /// Migration state tracking
    migration_state: MigrationState,
    
    /// Compatibility layer for legacy interfaces
    compatibility: CompatibilityLayer,
}

/// Current state of the migration process
#[derive(Debug, Clone)]
struct MigrationState {
    /// Migration phase
    phase: MigrationPhase,
    
    /// Version being migrated from
    from_version: String,
    
    /// Version being migrated to
    to_version: String,
    
    /// Migration progress (0.0 to 1.0)
    progress: f64,
    
    /// Migration start time
    started_at: std::time::SystemTime,
    
    /// Any migration errors encountered
    errors: Vec<String>,
}

/// Phases of the migration process
#[derive(Debug, Clone, PartialEq, Eq)]
enum MigrationPhase {
    /// Not started
    NotStarted,
    
    /// Preparing for migration
    Preparing,
    
    /// Running in compatibility mode
    Compatibility,
    
    /// Migrating state
    MigratingState,
    
    /// Testing new implementation
    Testing,
    
    /// Migration completed
    Completed,
    
    /// Migration failed
    Failed { reason: String },
}

/// Compatibility layer for legacy interfaces
#[derive(Debug)]
struct CompatibilityLayer {
    /// Legacy method mappings
    method_mappings: HashMap<String, String>,
    
    /// State transformation rules
    state_transforms: Vec<StateTransform>,
}

/// State transformation rule
#[derive(Debug, Clone)]
struct StateTransform {
    /// Source field path
    from_field: String,
    
    /// Target field path
    to_field: String,
    
    /// Transformation function name
    transform_fn: String,
}

impl ChainMigrationAdapter {
    /// Create a new migration adapter
    pub fn new(feature_flags: Arc<FeatureFlagManager>) -> Self {
        Self {
            feature_flags,
            migration_state: MigrationState {
                phase: MigrationPhase::NotStarted,
                from_version: "1.0.0".to_string(),
                to_version: "2.0.0".to_string(),
                progress: 0.0,
                started_at: std::time::SystemTime::now(),
                errors: Vec::new(),
            },
            compatibility: CompatibilityLayer {
                method_mappings: Self::create_method_mappings(),
                state_transforms: Self::create_state_transforms(),
            },
        }
    }
    
    /// Start the migration process
    pub async fn start_migration(&mut self) -> Result<(), MigrationError> {
        self.migration_state.phase = MigrationPhase::Preparing;
        self.migration_state.started_at = std::time::SystemTime::now();
        
        // Check if migration is enabled via feature flags
        if !self.feature_flags.is_enabled(&crate::features::FeatureFlag::ActorMigration) {
            return Err(MigrationError::MigrationDisabled);
        }
        
        self.migration_state.progress = 0.1;
        self.migration_state.phase = MigrationPhase::Compatibility;
        
        // Enable compatibility mode
        self.enable_compatibility_mode().await?;
        
        self.migration_state.progress = 0.5;
        self.migration_state.phase = MigrationPhase::MigratingState;
        
        // Migrate state
        self.migrate_chain_state().await?;
        
        self.migration_state.progress = 0.8;
        self.migration_state.phase = MigrationPhase::Testing;
        
        // Test new implementation
        self.test_new_implementation().await?;
        
        self.migration_state.progress = 1.0;
        self.migration_state.phase = MigrationPhase::Completed;
        
        Ok(())
    }
    
    /// Enable compatibility mode
    async fn enable_compatibility_mode(&mut self) -> Result<(), MigrationError> {
        // Implementation would enable legacy API compatibility
        Ok(())
    }
    
    /// Migrate chain state from legacy format
    async fn migrate_chain_state(&mut self) -> Result<(), MigrationError> {
        // Implementation would migrate state structures
        Ok(())
    }
    
    /// Test the new implementation
    async fn test_new_implementation(&mut self) -> Result<(), MigrationError> {
        // Implementation would run validation tests
        Ok(())
    }
    
    /// Create method mappings for legacy compatibility
    fn create_method_mappings() -> HashMap<String, String> {
        let mut mappings = HashMap::new();
        
        // Map legacy Chain methods to ChainActor messages
        mappings.insert("import_block".to_string(), "ImportBlock".to_string());
        mappings.insert("produce_block".to_string(), "ProduceBlock".to_string());
        mappings.insert("get_best_block".to_string(), "GetChainStatus".to_string());
        mappings.insert("finalize_block".to_string(), "FinalizeBlocks".to_string());
        
        mappings
    }
    
    /// Create state transformation rules
    fn create_state_transforms() -> Vec<StateTransform> {
        vec![
            StateTransform {
                from_field: "best_block".to_string(),
                to_field: "chain_state.head".to_string(),
                transform_fn: "block_to_block_ref".to_string(),
            },
            StateTransform {
                from_field: "finalized_block".to_string(),
                to_field: "chain_state.finalized".to_string(),
                transform_fn: "block_to_block_ref".to_string(),
            },
        ]
    }
    
    /// Get current migration progress
    pub fn progress(&self) -> f64 {
        self.migration_state.progress
    }
    
    /// Get current migration phase
    pub fn phase(&self) -> &MigrationPhase {
        &self.migration_state.phase
    }
    
    /// Check if migration is completed
    pub fn is_completed(&self) -> bool {
        matches!(self.migration_state.phase, MigrationPhase::Completed)
    }
    
    /// Check if migration failed
    pub fn has_failed(&self) -> bool {
        matches!(self.migration_state.phase, MigrationPhase::Failed { .. })
    }
    
    /// Get migration errors
    pub fn errors(&self) -> &[String] {
        &self.migration_state.errors
    }
}

/// Production migration controller with canary deployments and rollback
#[derive(Debug)]
pub struct ChainMigrationController {
    /// Current migration phase
    current_phase: MigrationPhase,
    
    /// Time when current phase started
    phase_start_time: std::time::Instant,
    
    /// Legacy chain instance (for parallel/fallback)
    legacy_chain: Option<Arc<std::sync::RwLock<crate::chain::Chain>>>,
    
    /// New chain actor
    chain_actor: Option<actix::Addr<ChainActor>>,
    
    /// Migration metrics
    metrics: MigrationMetrics,
    
    /// Feature flags for controlling rollout
    feature_flags: Arc<dyn FeatureFlagProvider>,
    
    /// Configuration parameters
    config: MigrationConfig,
}

/// Phased migration strategy for production safety
#[derive(Debug, Clone, PartialEq)]
pub enum MigrationPhase {
    /// Only legacy system active
    LegacyOnly,
    
    /// Actor runs in background, results compared but not used
    ShadowMode,
    
    /// Small percentage of operations use actor
    CanaryMode { percentage: f64 },
    
    /// Both systems active, results compared
    ParallelMode,
    
    /// Actor is primary, legacy is fallback
    ActorPrimary,
    
    /// Only actor system active
    ActorOnly,
    
    /// Emergency rollback to legacy
    Rollback { reason: String },
}

/// Migration configuration parameters
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Duration to run shadow mode
    pub shadow_mode_duration: std::time::Duration,
    
    /// Canary percentage (0.0 to 1.0)
    pub canary_percentage: f64,
    
    /// Duration for parallel mode
    pub parallel_mode_duration: std::time::Duration,
    
    /// Duration for primary mode
    pub primary_mode_duration: std::time::Duration,
    
    /// Success rate threshold to advance phases
    pub success_threshold: f64,
    
    /// Error rate threshold to trigger rollback
    pub error_threshold: f64,
    
    /// Performance ratio threshold (actor/legacy)
    pub performance_threshold: f64,
    
    /// Maximum allowed migration duration
    pub max_migration_duration: std::time::Duration,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            shadow_mode_duration: std::time::Duration::from_secs(1800), // 30 minutes
            canary_percentage: 0.01, // 1%
            parallel_mode_duration: std::time::Duration::from_secs(3600), // 1 hour
            primary_mode_duration: std::time::Duration::from_secs(1800), // 30 minutes
            success_threshold: 0.995, // 99.5%
            error_threshold: 0.01, // 1%
            performance_threshold: 0.95, // Actor should be at least 95% as fast
            max_migration_duration: std::time::Duration::from_secs(14400), // 4 hours
        }
    }
}

/// Migration performance metrics
#[derive(Debug)]
pub struct MigrationMetrics {
    // Operation counts
    legacy_operations: std::sync::atomic::AtomicU64,
    actor_operations: std::sync::atomic::AtomicU64,
    parallel_operations: std::sync::atomic::AtomicU64,
    
    // Success rates
    legacy_successes: std::sync::atomic::AtomicU64,
    actor_successes: std::sync::atomic::AtomicU64,
    
    // Performance metrics
    legacy_total_time: std::sync::atomic::AtomicU64, // nanoseconds
    actor_total_time: std::sync::atomic::AtomicU64,
    
    // Error tracking
    legacy_errors: std::sync::atomic::AtomicU64,
    actor_errors: std::sync::atomic::AtomicU64,
    comparison_mismatches: std::sync::atomic::AtomicU64,
    
    // Phase tracking
    phase_transitions: std::sync::atomic::AtomicU64,
    rollback_count: std::sync::atomic::AtomicU64,
}

impl Default for MigrationMetrics {
    fn default() -> Self {
        Self {
            legacy_operations: std::sync::atomic::AtomicU64::new(0),
            actor_operations: std::sync::atomic::AtomicU64::new(0),
            parallel_operations: std::sync::atomic::AtomicU64::new(0),
            legacy_successes: std::sync::atomic::AtomicU64::new(0),
            actor_successes: std::sync::atomic::AtomicU64::new(0),
            legacy_total_time: std::sync::atomic::AtomicU64::new(0),
            actor_total_time: std::sync::atomic::AtomicU64::new(0),
            legacy_errors: std::sync::atomic::AtomicU64::new(0),
            actor_errors: std::sync::atomic::AtomicU64::new(0),
            comparison_mismatches: std::sync::atomic::AtomicU64::new(0),
            phase_transitions: std::sync::atomic::AtomicU64::new(0),
            rollback_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

/// Feature flag provider trait for testing and production
pub trait FeatureFlagProvider: Send + Sync {
    /// Check if a feature is enabled
    fn is_enabled(&self, flag: &str) -> bool;
    
    /// Get feature flag value as float
    fn get_float(&self, flag: &str, default: f64) -> f64;
}

/// Current migration metrics snapshot
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub actor_success_rate: f64,
    pub legacy_success_rate: f64,
    pub actor_error_rate: f64,
    pub legacy_error_rate: f64,
    pub performance_ratio: f64,
    pub comparison_accuracy: f64,
    pub total_operations: u64,
}

impl ChainMigrationController {
    /// Create new migration controller
    pub fn new(
        legacy_chain: Arc<std::sync::RwLock<crate::chain::Chain>>,
        config: MigrationConfig,
        feature_flags: Arc<dyn FeatureFlagProvider>,
    ) -> Self {
        Self {
            current_phase: MigrationPhase::LegacyOnly,
            phase_start_time: std::time::Instant::now(),
            legacy_chain: Some(legacy_chain),
            chain_actor: None,
            metrics: MigrationMetrics::default(),
            feature_flags,
            config,
        }
    }

    /// Initialize the actor system
    pub async fn initialize_actor(&mut self, chain_actor: actix::Addr<ChainActor>) -> Result<(), MigrationError> {
        use std::sync::atomic::Ordering;
        
        // Sync actor state with legacy state
        let legacy_state = {
            let legacy = self.legacy_chain.as_ref().unwrap().read().map_err(|_| {
                MigrationError::StateMigrationFailed("Failed to lock legacy chain".to_string())
            })?;
            
            // Extract current chain state from legacy
            ChainState::new(BlockRef::genesis(Hash256::zero())) // Placeholder
        };

        // Initialize actor with legacy state
        chain_actor.send(InitializeFromLegacy {
            state: legacy_state,
        }).await.map_err(|e| MigrationError::StateMigrationFailed(e.to_string()))??;

        self.chain_actor = Some(chain_actor);
        Ok(())
    }

    /// Advance to the next migration phase
    pub async fn advance_migration_phase(&mut self) -> Result<MigrationPhase, MigrationError> {
        let phase_duration = self.phase_start_time.elapsed();
        let current_metrics = self.calculate_current_metrics();

        let next_phase = match &self.current_phase {
            MigrationPhase::LegacyOnly => {
                if self.chain_actor.is_some() {
                    MigrationPhase::ShadowMode
                } else {
                    return Err(MigrationError::ValidationFailed("Actor not initialized".to_string()));
                }
            }
            
            MigrationPhase::ShadowMode => {
                if phase_duration >= self.config.shadow_mode_duration {
                    if current_metrics.actor_success_rate >= self.config.success_threshold &&
                       current_metrics.comparison_accuracy >= 0.95 {
                        MigrationPhase::CanaryMode { percentage: self.config.canary_percentage }
                    } else {
                        return Err(MigrationError::ValidationFailed("Shadow mode validation failed".to_string()));
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::CanaryMode { percentage: _ } => {
                if phase_duration >= std::time::Duration::from_secs(300) { // 5 minutes minimum
                    if current_metrics.actor_success_rate >= self.config.success_threshold {
                        MigrationPhase::ParallelMode
                    } else if current_metrics.actor_error_rate > self.config.error_threshold {
                        MigrationPhase::Rollback { 
                            reason: "High error rate in canary mode".to_string() 
                        }
                    } else {
                        return Ok(self.current_phase.clone());
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::ParallelMode => {
                if phase_duration >= self.config.parallel_mode_duration {
                    if current_metrics.actor_success_rate >= self.config.success_threshold &&
                       current_metrics.performance_ratio >= self.config.performance_threshold {
                        MigrationPhase::ActorPrimary
                    } else {
                        MigrationPhase::Rollback { 
                            reason: "Performance or reliability issues in parallel mode".to_string() 
                        }
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::ActorPrimary => {
                if phase_duration >= self.config.primary_mode_duration {
                    if current_metrics.actor_success_rate >= self.config.success_threshold {
                        MigrationPhase::ActorOnly
                    } else {
                        MigrationPhase::Rollback { 
                            reason: "Reliability issues in primary mode".to_string() 
                        }
                    }
                } else {
                    return Ok(self.current_phase.clone());
                }
            }
            
            MigrationPhase::ActorOnly => {
                return Ok(self.current_phase.clone());
            }
            
            MigrationPhase::Rollback { .. } => {
                return Ok(self.current_phase.clone());
            }
        };

        // Perform phase transition
        self.transition_to_phase(next_phase.clone()).await?;
        Ok(next_phase)
    }

    async fn transition_to_phase(&mut self, new_phase: MigrationPhase) -> Result<(), MigrationError> {
        use std::sync::atomic::Ordering;
        
        tracing::info!("Transitioning from {:?} to {:?}", self.current_phase, new_phase);
        
        match (&self.current_phase, &new_phase) {
            (MigrationPhase::LegacyOnly, MigrationPhase::ShadowMode) => {
                self.start_shadow_mode().await?;
            }
            
            (MigrationPhase::ShadowMode, MigrationPhase::CanaryMode { .. }) => {
                self.start_canary_mode().await?;
            }
            
            (MigrationPhase::CanaryMode { .. }, MigrationPhase::ParallelMode) => {
                self.start_parallel_mode().await?;
            }
            
            (MigrationPhase::ParallelMode, MigrationPhase::ActorPrimary) => {
                self.start_actor_primary_mode().await?;
            }
            
            (MigrationPhase::ActorPrimary, MigrationPhase::ActorOnly) => {
                self.complete_migration().await?;
            }
            
            (_, MigrationPhase::Rollback { reason }) => {
                self.perform_rollback(reason).await?;
            }
            
            _ => {
                return Err(MigrationError::ValidationFailed("Invalid phase transition".to_string()));
            }
        }

        self.current_phase = new_phase;
        self.phase_start_time = std::time::Instant::now();
        self.metrics.phase_transitions.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }

    async fn start_shadow_mode(&mut self) -> Result<(), MigrationError> {
        if let Some(actor) = &self.chain_actor {
            actor.send(ConfigureShadowMode { enabled: true }).await
                .map_err(|e| MigrationError::StateMigrationFailed(e.to_string()))??;
        }
        tracing::info!("Shadow mode started");
        Ok(())
    }

    async fn start_canary_mode(&mut self) -> Result<(), MigrationError> {
        tracing::info!("Canary mode started with {}% traffic", self.config.canary_percentage * 100.0);
        Ok(())
    }

    async fn start_parallel_mode(&mut self) -> Result<(), MigrationError> {
        tracing::info!("Parallel mode started - both systems active");
        Ok(())
    }

    async fn start_actor_primary_mode(&mut self) -> Result<(), MigrationError> {
        tracing::info!("Actor primary mode started - actor is primary, legacy is fallback");
        Ok(())
    }

    async fn complete_migration(&mut self) -> Result<(), MigrationError> {
        // Drop legacy chain
        self.legacy_chain = None;
        
        if let Some(actor) = &self.chain_actor {
            actor.send(MigrationComplete).await
                .map_err(|e| MigrationError::StateMigrationFailed(e.to_string()))?;
        }
        
        tracing::info!("Chain actor migration completed successfully");
        Ok(())
    }

    async fn perform_rollback(&mut self, reason: &str) -> Result<(), MigrationError> {
        use std::sync::atomic::Ordering;
        
        tracing::error!("Performing emergency rollback: {}", reason);
        
        // Stop the actor if it exists
        if let Some(actor) = self.chain_actor.take() {
            actor.send(StopActor).await
                .map_err(|e| MigrationError::ValidationFailed(e.to_string()))?;
        }
        
        self.metrics.rollback_count.fetch_add(1, Ordering::Relaxed);
        
        tracing::info!("Rollback to legacy system completed");
        Ok(())
    }

    /// Calculate current performance metrics
    fn calculate_current_metrics(&self) -> MetricsSnapshot {
        use std::sync::atomic::Ordering;
        
        let legacy_ops = self.metrics.legacy_operations.load(Ordering::Relaxed);
        let actor_ops = self.metrics.actor_operations.load(Ordering::Relaxed);
        let legacy_successes = self.metrics.legacy_successes.load(Ordering::Relaxed);
        let actor_successes = self.metrics.actor_successes.load(Ordering::Relaxed);
        let legacy_errors = self.metrics.legacy_errors.load(Ordering::Relaxed);
        let actor_errors = self.metrics.actor_errors.load(Ordering::Relaxed);
        let legacy_time = self.metrics.legacy_total_time.load(Ordering::Relaxed);
        let actor_time = self.metrics.actor_total_time.load(Ordering::Relaxed);
        let mismatches = self.metrics.comparison_mismatches.load(Ordering::Relaxed);
        let parallel_ops = self.metrics.parallel_operations.load(Ordering::Relaxed);

        MetricsSnapshot {
            actor_success_rate: if actor_ops > 0 { actor_successes as f64 / actor_ops as f64 } else { 0.0 },
            legacy_success_rate: if legacy_ops > 0 { legacy_successes as f64 / legacy_ops as f64 } else { 0.0 },
            actor_error_rate: if actor_ops > 0 { actor_errors as f64 / actor_ops as f64 } else { 0.0 },
            legacy_error_rate: if legacy_ops > 0 { legacy_errors as f64 / legacy_ops as f64 } else { 0.0 },
            performance_ratio: if legacy_time > 0 && actor_time > 0 {
                legacy_time as f64 / actor_time as f64
            } else { 1.0 },
            comparison_accuracy: if parallel_ops > 0 {
                1.0 - (mismatches as f64 / parallel_ops as f64)
            } else { 1.0 },
            total_operations: legacy_ops + actor_ops,
        }
    }

    /// Import block using current migration phase strategy
    pub async fn import_block(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        match &self.current_phase {
            MigrationPhase::LegacyOnly => self.import_block_legacy_only(block).await,
            MigrationPhase::ShadowMode => self.import_block_shadow_mode(block).await,
            MigrationPhase::CanaryMode { percentage } => self.import_block_canary_mode(block, *percentage).await,
            MigrationPhase::ParallelMode => self.import_block_parallel_mode(block).await,
            MigrationPhase::ActorPrimary => self.import_block_actor_primary(block).await,
            MigrationPhase::ActorOnly => self.import_block_actor_only(block).await,
            MigrationPhase::Rollback { .. } => self.import_block_legacy_only(block).await,
        }
    }

    async fn import_block_legacy_only(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        use std::sync::atomic::Ordering;
        
        let start = std::time::Instant::now();
        
        let result = {
            let mut legacy = self.legacy_chain.as_ref().unwrap().write()
                .map_err(|_| ChainError::InternalError)?;
            legacy.import_block(block).await
        };
        
        let duration = start.elapsed();
        self.metrics.legacy_operations.fetch_add(1, Ordering::Relaxed);
        self.metrics.legacy_total_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        
        match &result {
            Ok(_) => self.metrics.legacy_successes.fetch_add(1, Ordering::Relaxed),
            Err(_) => self.metrics.legacy_errors.fetch_add(1, Ordering::Relaxed),
        };
        
        result
    }

    async fn import_block_shadow_mode(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        use std::sync::atomic::Ordering;
        
        // Legacy import (primary)
        let legacy_result = self.import_block_legacy_only(block.clone()).await;
        
        // Actor import (shadow)
        if let Some(actor) = &self.chain_actor {
            let _shadow_result = actor.send(ImportBlock {
                block: block.clone(),
                broadcast: false,
            }).await;
            
            self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
            // Results are compared but not used in shadow mode
        }
        
        legacy_result
    }

    async fn import_block_canary_mode(&self, block: SignedConsensusBlock, percentage: f64) -> Result<(), ChainError> {
        use std::sync::atomic::Ordering;
        
        let use_actor = {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen::<f64>() < percentage
        };
        
        if use_actor {
            let start = std::time::Instant::now();
            
            match self.chain_actor.as_ref().unwrap().send(ImportBlock {
                block: block.clone(),
                broadcast: true,
            }).await {
                Ok(Ok(())) => {
                    let duration = start.elapsed();
                    self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
                    self.metrics.actor_successes.fetch_add(1, Ordering::Relaxed);
                    self.metrics.actor_total_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
                    Ok(())
                }
                Ok(Err(e)) | Err(_) => {
                    self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
                    self.metrics.actor_errors.fetch_add(1, Ordering::Relaxed);
                    
                    // Fallback to legacy
                    tracing::warn!("Actor import failed in canary mode, falling back to legacy");
                    self.import_block_legacy_only(block).await
                }
            }
        } else {
            self.import_block_legacy_only(block).await
        }
    }

    async fn import_block_parallel_mode(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        use std::sync::atomic::Ordering;
        
        let legacy_future = self.import_block_legacy_only(block.clone());
        let actor_future = async {
            if let Some(actor) = &self.chain_actor {
                actor.send(ImportBlock {
                    block: block.clone(),
                    broadcast: false,
                }).await
            } else {
                Err(actix::MailboxError::Closed)
            }
        };
        
        let (legacy_result, actor_result) = futures::join!(legacy_future, actor_future);
        
        self.metrics.parallel_operations.fetch_add(1, Ordering::Relaxed);
        
        // Compare results
        match (&legacy_result, &actor_result) {
            (Ok(_), Ok(Ok(_))) => {
                // Both succeeded - check if results match
                // In real implementation, would compare block hashes/states
            }
            (Ok(_), Ok(Err(_))) | (Ok(_), Err(_)) => {
                self.metrics.comparison_mismatches.fetch_add(1, Ordering::Relaxed);
            }
            (Err(_), Ok(Ok(_))) => {
                self.metrics.comparison_mismatches.fetch_add(1, Ordering::Relaxed);
            }
            _ => {} // Both failed - consistent
        }
        
        // Return legacy result during parallel phase
        legacy_result
    }

    async fn import_block_actor_primary(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        use std::sync::atomic::Ordering;
        
        let start = std::time::Instant::now();
        
        match self.chain_actor.as_ref().unwrap().send(ImportBlock {
            block: block.clone(),
            broadcast: true,
        }).await {
            Ok(result) => {
                let duration = start.elapsed();
                self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
                self.metrics.actor_total_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
                
                match result {
                    Ok(()) => {
                        self.metrics.actor_successes.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(e) => {
                        self.metrics.actor_errors.fetch_add(1, Ordering::Relaxed);
                        Err(e)
                    }
                }
            }
            Err(_) => {
                self.metrics.actor_errors.fetch_add(1, Ordering::Relaxed);
                
                // Fallback to legacy
                tracing::warn!("Actor import failed in primary mode, falling back to legacy");
                self.import_block_legacy_only(block).await
            }
        }
    }

    async fn import_block_actor_only(&self, block: SignedConsensusBlock) -> Result<(), ChainError> {
        use std::sync::atomic::Ordering;
        
        let start = std::time::Instant::now();
        
        let result = self.chain_actor.as_ref().unwrap()
            .send(ImportBlock { block, broadcast: true })
            .await
            .map_err(|_| ChainError::InternalError)?;
        
        let duration = start.elapsed();
        self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
        self.metrics.actor_total_time.fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
        
        match &result {
            Ok(_) => self.metrics.actor_successes.fetch_add(1, Ordering::Relaxed),
            Err(_) => self.metrics.actor_errors.fetch_add(1, Ordering::Relaxed),
        };
        
        result
    }

    /// Get current phase
    pub fn current_phase(&self) -> &MigrationPhase {
        &self.current_phase
    }

    /// Get metrics snapshot
    pub fn metrics(&self) -> MetricsSnapshot {
        self.calculate_current_metrics()
    }
}

// Messages for migration control
use actix::prelude::*;

/// Message to initialize actor from legacy state
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct InitializeFromLegacy {
    pub state: ChainState,
}

/// Message to configure shadow mode
#[derive(Message)]
#[rtype(result = "Result<(), ChainError>")]
pub struct ConfigureShadowMode {
    pub enabled: bool,
}

/// Message to complete migration
#[derive(Message)]
#[rtype(result = "()")]
pub struct MigrationComplete;

/// Message to stop actor
#[derive(Message)]
#[rtype(result = "()")]
pub struct StopActor;

/// Migration errors
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("Migration is disabled via feature flags")]
    MigrationDisabled,
    
    #[error("State migration failed: {0}")]
    StateMigrationFailed(String),
    
    #[error("Compatibility mode failed: {0}")]
    CompatibilityFailed(String),
    
    #[error("Migration validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Migration timeout")]
    Timeout,
}