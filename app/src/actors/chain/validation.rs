//! Chain Validation Logic
//!
//! Block and transaction validation logic for ChainActor.
//! This module provides comprehensive validation for blocks, transactions,
//! consensus rules, and auxiliary proof-of-work submissions.

use std::collections::HashMap;
use std::time::Duration;

use super::messages::*;
use super::state::ValidationCache;
use crate::types::*;

/// Chain validator for comprehensive block and transaction validation
#[derive(Debug)]
pub struct ChainValidator {
    /// Configuration for validation rules
    config: ValidationConfig,
    
    /// Cache for validation results
    cache: ValidationCache,
    
    /// Validation performance metrics
    metrics: ValidationMetrics,
}

/// Configuration for chain validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Whether to use validation cache
    pub use_cache: bool,
    
    /// Cache TTL for validation results
    pub cache_ttl: Duration,
    
    /// Maximum validation time before timeout
    pub max_validation_time: Duration,
    
    /// Strict consensus rule enforcement
    pub strict_consensus: bool,
    
    /// Validate auxiliary proof-of-work
    pub validate_auxpow: bool,
}

/// Validation performance metrics
#[derive(Debug, Default)]
struct ValidationMetrics {
    /// Total validations performed
    total_validations: u64,
    
    /// Cache hit rate
    cache_hits: u64,
    
    /// Cache misses
    cache_misses: u64,
    
    /// Validation failures
    validation_failures: u64,
    
    /// Average validation time
    avg_validation_time: Duration,
}

impl ChainValidator {
    /// Create a new chain validator with the given configuration
    pub fn new(config: ValidationConfig, cache_size: usize) -> Self {
        Self {
            config,
            cache: ValidationCache::new(cache_size),
            metrics: ValidationMetrics::default(),
        }
    }
    
    /// Validate a block according to consensus rules
    pub async fn validate_block(
        &mut self,
        block: &SignedConsensusBlock,
        validation_level: ValidationLevel,
    ) -> Result<ValidationResult, ChainError> {
        let start_time = std::time::Instant::now();
        
        // Check cache first if enabled
        if self.config.use_cache && validation_level == ValidationLevel::Full {
            if let Some(cached_result) = self.cache.get(&block.hash) {
                self.metrics.cache_hits += 1;
                return Ok(cached_result);
            }
            self.metrics.cache_misses += 1;
        }
        
        // Perform validation based on level
        let mut result = ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            gas_used: 0,
            state_root: block.header.state_root,
            validation_metrics: ValidationMetrics::default(),
            checkpoints: Vec::new(),
            warnings: Vec::new(),
        };
        
        match validation_level {
            ValidationLevel::Basic => {
                self.validate_basic_structure(block, &mut result).await?;
            }
            ValidationLevel::Full => {
                self.validate_basic_structure(block, &mut result).await?;
                if result.is_valid {
                    self.validate_state_transitions(block, &mut result).await?;
                }
                if result.is_valid {
                    self.validate_consensus_rules(block, &mut result).await?;
                }
            }
            ValidationLevel::SignatureOnly => {
                self.validate_signatures(block, &mut result).await?;
            }
            ValidationLevel::ConsensusOnly => {
                self.validate_consensus_rules(block, &mut result).await?;
            }
        }
        
        // Update metrics
        let validation_time = start_time.elapsed();
        self.metrics.total_validations += 1;
        if !result.is_valid {
            self.metrics.validation_failures += 1;
        }
        
        // Cache result if enabled and it's a full validation
        if self.config.use_cache && validation_level == ValidationLevel::Full {
            self.cache.insert(block.hash, result.clone());
        }
        
        Ok(result)
    }
    
    /// Validate basic block structure
    async fn validate_basic_structure(
        &self,
        block: &SignedConsensusBlock,
        result: &mut ValidationResult,
    ) -> Result<(), ChainError> {
        result.checkpoints.push("basic_structure".to_string());
        
        // Validate block size
        if block.encoded_size() > MAX_BLOCK_SIZE {
            result.is_valid = false;
            result.errors.push(ValidationError::ConsensusError {
                rule: "block_size".to_string(),
                message: format!("Block size {} exceeds maximum {}", block.encoded_size(), MAX_BLOCK_SIZE),
            });
        }
        
        // Validate timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
            
        if block.header.timestamp > now.as_secs() + MAX_TIME_DRIFT {
            result.is_valid = false;
            result.errors.push(ValidationError::InvalidTimestamp {
                timestamp: block.header.timestamp,
                reason: TimestampError::TooFuture { max_drift_seconds: MAX_TIME_DRIFT },
            });
        }
        
        Ok(())
    }
    
    /// Validate state transitions
    async fn validate_state_transitions(
        &self,
        block: &SignedConsensusBlock,
        result: &mut ValidationResult,
    ) -> Result<(), ChainError> {
        result.checkpoints.push("state_transitions".to_string());
        
        // Placeholder for state transition validation
        // Would execute transactions and verify state root
        
        Ok(())
    }
    
    /// Validate consensus rules
    async fn validate_consensus_rules(
        &self,
        block: &SignedConsensusBlock,
        result: &mut ValidationResult,
    ) -> Result<(), ChainError> {
        result.checkpoints.push("consensus_rules".to_string());
        
        // Validate Aura PoA rules
        // Validate auxiliary PoW if present
        // Validate peg operations
        
        Ok(())
    }
    
    /// Validate block signatures
    async fn validate_signatures(
        &self,
        block: &SignedConsensusBlock,
        result: &mut ValidationResult,
    ) -> Result<(), ChainError> {
        result.checkpoints.push("signatures".to_string());
        
        // Validate block producer signature
        // Validate federation signatures if required
        
        Ok(())
    }
    
    /// Get validation cache statistics
    pub fn cache_stats(&self) -> (f64, u64, u64) {
        let hit_rate = if self.metrics.cache_hits + self.metrics.cache_misses > 0 {
            self.metrics.cache_hits as f64 / (self.metrics.cache_hits + self.metrics.cache_misses) as f64
        } else {
            0.0
        };
        (hit_rate, self.metrics.cache_hits, self.metrics.cache_misses)
    }
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            use_cache: true,
            cache_ttl: Duration::from_secs(300), // 5 minutes
            max_validation_time: Duration::from_millis(100),
            strict_consensus: true,
            validate_auxpow: true,
        }
    }
}

// Constants for validation
const MAX_BLOCK_SIZE: usize = 8 * 1024 * 1024; // 8MB
const MAX_TIME_DRIFT: u64 = 15; // 15 seconds

// Extend ValidationCache with additional methods
impl ValidationCache {
    /// Get a cached validation result
    pub fn get(&mut self, block_hash: &Hash256) -> Option<ValidationResult> {
        if let Some(cached) = self.cache.get(block_hash) {
            if cached.expires_at > std::time::Instant::now() {
                self.hits += 1;
                // Convert cached validation to ValidationResult
                Some(ValidationResult {
                    is_valid: cached.result,
                    errors: cached.errors.clone(),
                    gas_used: 0, // Would be stored in cache
                    state_root: Hash256::zero(), // Would be stored in cache
                    validation_metrics: ValidationMetrics::default(),
                    checkpoints: Vec::new(),
                    warnings: Vec::new(),
                })
            } else {
                // Expired entry
                self.cache.remove(block_hash);
                self.misses += 1;
                None
            }
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Insert a validation result into the cache
    pub fn insert(&mut self, block_hash: Hash256, result: ValidationResult) {
        let expires_at = std::time::Instant::now() + Duration::from_secs(300);
        let cached = super::state::CachedValidation {
            result: result.is_valid,
            errors: result.errors,
            cached_at: std::time::Instant::now(),
            expires_at,
        };
        
        // Remove oldest entry if cache is full
        if self.cache.len() >= self.max_size {
            if let Some((oldest_key, _)) = self.cache.iter().min_by_key(|(_, v)| v.cached_at) {
                let oldest_key = *oldest_key;
                self.cache.remove(&oldest_key);
            }
        }
        
        self.cache.insert(block_hash, cached);
    }
}