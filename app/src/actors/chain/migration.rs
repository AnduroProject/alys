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