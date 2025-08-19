//! Feature Flag System for Alys V2
//!
//! This module implements a robust feature flag system that allows gradual rollout of migration changes,
//! A/B testing, and instant rollback capabilities. The system integrates with the existing configuration
//! architecture and provides hot-reload, caching, and performance optimizations.

pub mod types;
pub mod manager;
pub mod evaluation;
pub mod context;
pub mod config;
pub mod cache;
pub mod watcher;
pub mod validation;
pub mod performance;

#[cfg(test)]
mod tests;

// Re-exports for convenience
pub use types::*;
pub use manager::FeatureFlagManager;
pub use evaluation::*;
pub use context::*;
pub use config::*;
pub use cache::*;
pub use validation::*;
pub use performance::*;

/// Feature flag system errors
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeatureFlagError {
    #[error("Feature flag not found: {name}")]
    FlagNotFound { name: String },
    
    #[error("Configuration error: {source}")]
    ConfigError { source: crate::config::ConfigError },
    
    #[error("Evaluation error: {reason}")]
    EvaluationError { reason: String },
    
    #[error("Cache error: {reason}")]
    CacheError { reason: String },
    
    #[error("Validation error: {flag} - {reason}")]
    ValidationError { flag: String, reason: String },
    
    #[error("Serialization error: {reason}")]
    SerializationError { reason: String },
    
    #[error("IO error during {operation}: {error}")]
    IoError { operation: String, error: String },
}

impl From<crate::config::ConfigError> for FeatureFlagError {
    fn from(err: crate::config::ConfigError) -> Self {
        FeatureFlagError::ConfigError { source: err }
    }
}

/// Result type for feature flag operations
pub type FeatureFlagResult<T> = Result<T, FeatureFlagError>;

/// Global feature flag instance
use std::sync::OnceLock;
use std::sync::Arc;

static GLOBAL_FEATURE_FLAGS: OnceLock<Arc<FeatureFlagManager>> = OnceLock::new();

/// Initialize the global feature flag manager
pub fn init_feature_flags(config_path: &str) -> FeatureFlagResult<()> {
    let manager = FeatureFlagManager::new(config_path.into())?;
    GLOBAL_FEATURE_FLAGS.set(Arc::new(manager))
        .map_err(|_| FeatureFlagError::EvaluationError { 
            reason: "Global feature flags already initialized".to_string() 
        })?;
    Ok(())
}

/// Get the global feature flag manager
pub fn global_feature_flags() -> Option<Arc<FeatureFlagManager>> {
    GLOBAL_FEATURE_FLAGS.get().cloned()
}

/// High-performance feature flag macro with 5-second caching
/// Implements ALYS-004-08: feature_enabled! macro with 5-second caching
/// 
/// This macro provides ultra-fast feature flag checks with:
/// - 5-second TTL cache for maximum performance
/// - Context validation to prevent stale data
/// - Automatic fallback to manager on cache miss
/// - Target: <50μs for cache hits, <500μs for cache misses
#[macro_export]
macro_rules! feature_enabled {
    ($flag:expr) => {{
        async move {
            // Get evaluation context first
            if let Ok(context) = $crate::features::get_evaluation_context().await {
                // Try ultra-fast macro cache first (5-second TTL)
                if let Some(cached_result) = $crate::features::performance::macro_cache::fast_cache_lookup($flag, &context).await {
                    return cached_result;
                }
                
                // Cache miss - evaluate through manager with timing
                if let Some(manager) = $crate::features::global_feature_flags() {
                    let evaluation_start = std::time::Instant::now();
                    let result = manager.is_enabled($flag, &context).await;
                    let evaluation_time_us = evaluation_start.elapsed().as_micros() as u64;
                    
                    // Store in high-performance cache for next evaluation
                    $crate::features::performance::macro_cache::fast_cache_store($flag, &context, result, evaluation_time_us).await;
                    
                    result
                } else {
                    false
                }
            } else {
                false
            }
        }
    }};
    
    ($flag:expr, $context:expr) => {{
        async move {
            // Try ultra-fast macro cache first (5-second TTL)
            if let Some(cached_result) = $crate::features::performance::macro_cache::fast_cache_lookup($flag, &$context).await {
                return cached_result;
            }
            
            // Cache miss - evaluate through manager with timing
            if let Some(manager) = $crate::features::global_feature_flags() {
                let evaluation_start = std::time::Instant::now();
                let result = manager.is_enabled($flag, &$context).await;
                let evaluation_time_us = evaluation_start.elapsed().as_micros() as u64;
                
                // Store in high-performance cache for next evaluation  
                $crate::features::performance::macro_cache::fast_cache_store($flag, &$context, result, evaluation_time_us).await;
                
                result
            } else {
                false
            }
        }
    }};
}

/// Initialize performance monitoring and maintenance
pub async fn init_performance_monitoring() -> tokio::task::JoinHandle<()> {
    // Start background maintenance for macro cache cleanup
    let maintenance = performance::PerformanceMaintenance::new(
        30, // Clean up every 30 seconds
        10000 // Keep 10k performance samples
    );
    maintenance.start()
}