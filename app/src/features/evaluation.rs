//! Feature flag evaluation engine
//!
//! This module implements the core evaluation logic for feature flags, including
//! condition checking, targeting, and percentage-based rollouts.

use super::types::*;
use super::context::*;
use super::FeatureFlagResult;
use chrono::{Utc, Timelike};
use std::net::IpAddr;
use ipnetwork::IpNetwork;

/// Feature flag evaluation engine
pub struct FeatureFlagEvaluator {
    /// Performance settings
    max_evaluation_time_ms: u64,
}

impl FeatureFlagEvaluator {
    /// Create a new evaluator with default settings
    pub fn new() -> Self {
        Self {
            max_evaluation_time_ms: 1,
        }
    }
    
    /// Create evaluator with custom timeout
    pub fn with_timeout(max_evaluation_time_ms: u64) -> Self {
        Self {
            max_evaluation_time_ms,
        }
    }
    
    /// Evaluate a feature flag for the given context
    pub async fn evaluate_flag(
        &self, 
        flag: &FeatureFlag, 
        context: &EvaluationContext
    ) -> FeatureFlagResult<bool> {
        let start_time = std::time::Instant::now();
        
        // Check if globally disabled
        if !flag.enabled {
            return Ok(false);
        }
        
        // Check conditions first (fastest to evaluate)
        if let Some(conditions) = &flag.conditions {
            for condition in conditions {
                if !self.evaluate_condition(condition, context).await? {
                    return Ok(false);
                }
                
                // Check timeout
                if start_time.elapsed().as_millis() as u64 > self.max_evaluation_time_ms {
                    return Err(super::FeatureFlagError::EvaluationError {
                        reason: format!(
                            "Evaluation timeout after {}ms for flag '{}'", 
                            self.max_evaluation_time_ms, 
                            flag.name
                        ),
                    });
                }
            }
        }
        
        // Check targeting rules
        if let Some(targets) = &flag.targets {
            if !self.evaluate_targets(targets, context).await? {
                return Ok(false);
            }
        }
        
        // Check rollout percentage
        if let Some(percentage) = flag.rollout_percentage {
            let enabled = self.evaluate_percentage_rollout(percentage, context, &flag.name);
            return Ok(enabled);
        }
        
        // If we get here, the flag should be enabled
        Ok(true)
    }
    
    /// Evaluate a single condition
    async fn evaluate_condition(
        &self,
        condition: &FeatureCondition,
        context: &EvaluationContext,
    ) -> FeatureFlagResult<bool> {
        match condition {
            FeatureCondition::After(datetime) => {
                Ok(context.evaluation_time >= *datetime)
            }
            
            FeatureCondition::Before(datetime) => {
                Ok(context.evaluation_time < *datetime)
            }
            
            FeatureCondition::ChainHeightAbove(height) => {
                Ok(context.chain_height > *height)
            }
            
            FeatureCondition::ChainHeightBelow(height) => {
                Ok(context.chain_height < *height)
            }
            
            FeatureCondition::SyncProgressAbove(threshold) => {
                Ok(context.sync_progress > *threshold)
            }
            
            FeatureCondition::SyncProgressBelow(threshold) => {
                Ok(context.sync_progress < *threshold)
            }
            
            FeatureCondition::TimeWindow { start_hour, end_hour } => {
                let current_hour = context.evaluation_time.hour() as u8;
                if start_hour <= end_hour {
                    Ok(current_hour >= *start_hour && current_hour < *end_hour)
                } else {
                    // Crosses midnight
                    Ok(current_hour >= *start_hour || current_hour < *end_hour)
                }
            }
            
            FeatureCondition::NodeHealth { 
                min_peers, 
                max_memory_usage_mb, 
                max_cpu_usage_percent 
            } => {
                let health = &context.node_health;
                
                if let Some(min) = min_peers {
                    if health.peer_count < *min {
                        return Ok(false);
                    }
                }
                
                if let Some(max_mem) = max_memory_usage_mb {
                    if health.memory_usage_mb > *max_mem {
                        return Ok(false);
                    }
                }
                
                if let Some(max_cpu) = max_cpu_usage_percent {
                    if health.cpu_usage_percent > *max_cpu {
                        return Ok(false);
                    }
                }
                
                Ok(true)
            }
            
            FeatureCondition::Custom(expression) => {
                // For now, custom conditions are not implemented
                // In a full implementation, this could use a small expression language
                self.evaluate_custom_condition(expression, context).await
            }
        }
    }
    
    /// Evaluate targeting rules
    async fn evaluate_targets(
        &self,
        targets: &FeatureTargets,
        context: &EvaluationContext,
    ) -> FeatureFlagResult<bool> {
        // Node ID targeting
        if let Some(node_ids) = &targets.node_ids {
            if node_ids.contains(&context.node_id) {
                return Ok(true);
            }
        }
        
        // Validator key targeting
        if let Some(validator_keys) = &targets.validator_keys {
            if let Some(ref context_key) = context.validator_key {
                if validator_keys.contains(context_key) {
                    return Ok(true);
                }
            }
        }
        
        // Environment targeting
        if let Some(environments) = &targets.environments {
            if environments.contains(&context.environment) {
                return Ok(true);
            }
        }
        
        // IP range targeting
        if let Some(ip_ranges) = &targets.ip_ranges {
            if let Some(context_ip) = context.ip_address {
                for range_str in ip_ranges {
                    if let Ok(network) = range_str.parse::<IpNetwork>() {
                        if network.contains(context_ip) {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        
        // Custom attribute targeting
        if let Some(target_attrs) = &targets.custom_attributes {
            for (key, value) in target_attrs {
                if let Some(context_value) = context.custom_attributes.get(key) {
                    if context_value == value {
                        return Ok(true);
                    }
                }
            }
        }
        
        // If no targeting rules matched, and we have targets defined, return false
        if targets.node_ids.is_some() 
            || targets.validator_keys.is_some() 
            || targets.environments.is_some() 
            || targets.ip_ranges.is_some() 
            || targets.custom_attributes.is_some() {
            Ok(false)
        } else {
            // No targeting rules defined, so allow
            Ok(true)
        }
    }
    
    /// Evaluate percentage-based rollout using consistent hashing (ALYS-004-09)
    /// Uses enhanced hash-based context evaluation for guaranteed consistency
    fn evaluate_percentage_rollout(
        &self,
        percentage: u8,
        context: &EvaluationContext,
        flag_name: &str,
    ) -> bool {
        // Use the enhanced consistent hashing from performance module
        crate::features::performance::consistent_hashing::evaluate_consistent_percentage(
            percentage, context, flag_name
        )
    }
    
    /// Hash a string to u64 for consistent evaluation
    /// Note: Prefer using consistent_hashing module for percentage rollouts
    fn hash_string(&self, input: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Evaluate custom condition expressions
    async fn evaluate_custom_condition(
        &self,
        expression: &str,
        _context: &EvaluationContext,
    ) -> FeatureFlagResult<bool> {
        // For Phase 1, we'll only support simple boolean expressions
        // In a full implementation, this could use a proper expression parser
        match expression.trim().to_lowercase().as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => {
                // For now, unsupported custom expressions default to false
                tracing::warn!("Unsupported custom condition expression: {}", expression);
                Ok(false)
            }
        }
    }
}

impl Default for FeatureFlagEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluation result with additional metadata
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    /// Whether the flag is enabled
    pub enabled: bool,
    
    /// Reason for the evaluation result
    pub reason: EvaluationReason,
    
    /// Evaluation time in microseconds
    pub evaluation_time_us: u64,
    
    /// Flag that was evaluated
    pub flag_name: String,
    
    /// Context hash for consistency verification
    pub context_hash: u64,
}

/// Reason for evaluation result
#[derive(Debug, Clone)]
pub enum EvaluationReason {
    /// Flag is globally disabled
    GloballyDisabled,
    
    /// Failed condition check
    ConditionFailed(String),
    
    /// Targeting rules didn't match
    TargetingFailed,
    
    /// Percentage rollout excluded this context
    PercentageExcluded,
    
    /// All checks passed
    Enabled,
    
    /// Evaluation error occurred
    Error(String),
}

/// Enhanced evaluator with detailed results
pub struct DetailedFeatureFlagEvaluator {
    inner: FeatureFlagEvaluator,
}

impl DetailedFeatureFlagEvaluator {
    /// Create new detailed evaluator
    pub fn new() -> Self {
        Self {
            inner: FeatureFlagEvaluator::new(),
        }
    }
    
    /// Evaluate flag with detailed result
    pub async fn evaluate_flag_detailed(
        &self,
        flag: &FeatureFlag,
        context: &EvaluationContext,
    ) -> FeatureFlagResult<EvaluationResult> {
        let start_time = std::time::Instant::now();
        let flag_name = flag.name.clone();
        let context_hash = context.hash();
        
        // Check if globally disabled
        if !flag.enabled {
            return Ok(EvaluationResult {
                enabled: false,
                reason: EvaluationReason::GloballyDisabled,
                evaluation_time_us: start_time.elapsed().as_micros() as u64,
                flag_name,
                context_hash,
            });
        }
        
        // Check conditions
        if let Some(conditions) = &flag.conditions {
            for condition in conditions {
                if !self.inner.evaluate_condition(condition, context).await? {
                    return Ok(EvaluationResult {
                        enabled: false,
                        reason: EvaluationReason::ConditionFailed(format!("{:?}", condition)),
                        evaluation_time_us: start_time.elapsed().as_micros() as u64,
                        flag_name,
                        context_hash,
                    });
                }
            }
        }
        
        // Check targeting
        if let Some(targets) = &flag.targets {
            if !self.inner.evaluate_targets(targets, context).await? {
                return Ok(EvaluationResult {
                    enabled: false,
                    reason: EvaluationReason::TargetingFailed,
                    evaluation_time_us: start_time.elapsed().as_micros() as u64,
                    flag_name,
                    context_hash,
                });
            }
        }
        
        // Check percentage rollout
        if let Some(percentage) = flag.rollout_percentage {
            let enabled = self.inner.evaluate_percentage_rollout(percentage, context, &flag.name);
            if !enabled {
                return Ok(EvaluationResult {
                    enabled: false,
                    reason: EvaluationReason::PercentageExcluded,
                    evaluation_time_us: start_time.elapsed().as_micros() as u64,
                    flag_name,
                    context_hash,
                });
            }
        }
        
        Ok(EvaluationResult {
            enabled: true,
            reason: EvaluationReason::Enabled,
            evaluation_time_us: start_time.elapsed().as_micros() as u64,
            flag_name,
            context_hash,
        })
    }
}

impl Default for DetailedFeatureFlagEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Environment;
    
    #[tokio::test]
    async fn test_basic_evaluation() {
        let evaluator = FeatureFlagEvaluator::new();
        let context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        
        // Test enabled flag
        let flag = FeatureFlag::enabled("test_flag".to_string());
        let result = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        assert!(result);
        
        // Test disabled flag
        let flag = FeatureFlag::disabled("test_flag".to_string());
        let result = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        assert!(!result);
    }
    
    #[tokio::test]
    async fn test_percentage_rollout() {
        let evaluator = FeatureFlagEvaluator::new();
        
        // Test with multiple contexts to verify distribution
        let mut enabled_count = 0;
        for i in 0..1000 {
            let context = EvaluationContext::new(format!("node-{}", i), Environment::Development);
            let flag = FeatureFlag::with_percentage("test_flag".to_string(), true, 50);
            
            if evaluator.evaluate_flag(&flag, &context).await.unwrap() {
                enabled_count += 1;
            }
        }
        
        // Should be approximately 50% (allowing for variance)
        assert!(enabled_count > 400 && enabled_count < 600);
    }
    
    #[tokio::test]
    async fn test_condition_evaluation() {
        let evaluator = FeatureFlagEvaluator::new();
        let mut context = EvaluationContext::new("test-node".to_string(), Environment::Development);
        context.chain_height = 500;
        
        // Test chain height condition
        let conditions = vec![FeatureCondition::ChainHeightAbove(1000)];
        let flag = FeatureFlag::enabled("test_flag".to_string()).with_conditions(conditions);
        let result = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        assert!(!result); // Should be false since 500 <= 1000
        
        context.chain_height = 1500;
        let result = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        assert!(result); // Should be true since 1500 > 1000
    }
    
    #[tokio::test] 
    async fn test_targeting() {
        let evaluator = FeatureFlagEvaluator::new();
        let context = EvaluationContext::new("target-node".to_string(), Environment::Development);
        
        // Test node targeting
        let targets = FeatureTargets::new().with_node_ids(vec!["target-node".to_string()]);
        let flag = FeatureFlag::enabled("test_flag".to_string()).with_targets(targets);
        let result = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        assert!(result);
        
        // Test non-matching node
        let targets = FeatureTargets::new().with_node_ids(vec!["other-node".to_string()]);
        let flag = FeatureFlag::enabled("test_flag".to_string()).with_targets(targets);
        let result = evaluator.evaluate_flag(&flag, &context).await.unwrap();
        assert!(!result);
    }
}