//! Actor System Utilities - Phase 1 Implementation
//! 
//! Comprehensive utility functions and helper types for the Alys V2 actor system.
//! Provides blockchain-specific utilities, actor lifecycle helpers, configuration
//! validation, metrics collection utilities, and testing support functions.

use crate::actors::foundation::{
    ActorSystemConfig, ActorPriority, 
    blockchain, lifecycle, performance, validation
};
use crate::actors::foundation::restart_strategy::{RestartStrategy, RestartReason};
use crate::actors::foundation::root_supervisor::SystemHealth;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use std::fmt;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Utility functions for blockchain-specific operations
pub mod blockchain_utils {
    use super::*;

    /// Calculate the next block boundary timestamp
    /// 
    /// Given the current time, calculates when the next 2-second block
    /// boundary will occur for the Alys sidechain.
    pub fn next_block_boundary(current_time: SystemTime) -> SystemTime {
        let block_interval = blockchain::BLOCK_INTERVAL;
        let duration_since_epoch = current_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        
        let blocks_elapsed = duration_since_epoch.as_secs() / block_interval.as_secs();
        let next_block_time = (blocks_elapsed + 1) * block_interval.as_secs();
        
        SystemTime::UNIX_EPOCH + Duration::from_secs(next_block_time)
    }

    /// Check if we're currently within a block production window
    /// 
    /// Returns true if we're in the first half of a block interval,
    /// which is typically when block production should occur.
    pub fn is_block_production_window() -> bool {
        let now = SystemTime::now();
        let duration_since_epoch = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        
        let block_interval_ms = blockchain::BLOCK_INTERVAL.as_millis();
        let current_ms = duration_since_epoch.as_millis();
        let position_in_block = current_ms % block_interval_ms;
        
        // First half of block interval is production window
        position_in_block < (block_interval_ms / 2)
    }

    /// Calculate delay until next block production window
    pub fn delay_to_next_production_window() -> Duration {
        let now = SystemTime::now();
        let duration_since_epoch = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        
        let block_interval_ms = blockchain::BLOCK_INTERVAL.as_millis();
        let current_ms = duration_since_epoch.as_millis();
        let position_in_block = current_ms % block_interval_ms;
        
        if position_in_block < (block_interval_ms / 2) {
            // Already in production window
            Duration::ZERO
        } else {
            // Wait until next block starts
            let delay_ms = block_interval_ms - position_in_block;
            Duration::from_millis(delay_ms as u64)
        }
    }

    /// Convert Bitcoin confirmations to estimated time
    pub fn bitcoin_confirmations_to_time(confirmations: u32) -> Duration {
        // Bitcoin blocks average ~10 minutes
        const BITCOIN_BLOCK_TIME: Duration = Duration::from_secs(600);
        BITCOIN_BLOCK_TIME * confirmations
    }

    /// Check if a restart should be aligned with consensus operations
    pub fn should_align_restart_with_consensus(reason: &RestartReason) -> bool {
        matches!(reason, 
            RestartReason::ConsensusFailure | 
            RestartReason::NetworkFailure |
            RestartReason::SupervisionEscalation
        )
    }
}

/// Utility functions for actor lifecycle management
pub mod lifecycle_utils {
    use super::*;

    /// Calculate startup priority order based on actor priority and dependencies
    pub fn calculate_startup_order(
        configs: &HashMap<String, ActorPriority>
    ) -> Vec<String> {
        let mut actors: Vec<_> = configs.iter().collect();
        
        // Sort by priority (Critical first, Background last)
        actors.sort_by(|a, b| b.1.cmp(a.1));
        
        actors.into_iter().map(|(name, _)| name.clone()).collect()
    }

    /// Check if an actor should be restarted based on its state and configuration
    pub fn should_restart_actor(
        restart_strategy: &RestartStrategy,
        attempt_count: usize,
        last_restart: Option<SystemTime>,
        reason: &RestartReason,
    ) -> bool {
        // Check if strategy allows restart
        if !restart_strategy.should_restart(attempt_count, reason, &[]) {
            return false;
        }

        // Check minimum time between restarts for stability
        if let Some(last) = last_restart {
            let time_since_last = SystemTime::now()
                .duration_since(last)
                .unwrap_or(Duration::MAX);
            
            if time_since_last < Duration::from_millis(100) {
                debug!("Restart too soon after last attempt, delaying");
                return false;
            }
        }

        true
    }

    /// Calculate health check interval based on actor priority
    pub fn calculate_health_check_interval(priority: ActorPriority) -> Duration {
        match priority {
            ActorPriority::Critical => Duration::from_secs(5),
            ActorPriority::High => Duration::from_secs(10),
            ActorPriority::Normal => Duration::from_secs(30),
            ActorPriority::Low => Duration::from_secs(60),
            ActorPriority::Background => Duration::from_secs(120),
        }
    }

    /// Generate unique actor instance ID
    pub fn generate_actor_id(actor_type: &str) -> String {
        format!("{}_{}", actor_type.to_lowercase(), Uuid::new_v4().to_string()[..8].to_string())
    }
}

/// Configuration validation utilities
pub mod config_utils {
    use super::*;

    /// Validate mailbox capacity is within reasonable bounds
    pub fn validate_mailbox_capacity(capacity: usize) -> Result<(), ConfigValidationError> {
        if capacity < validation::MIN_MAILBOX_CAPACITY {
            return Err(ConfigValidationError::InvalidMailboxCapacity {
                capacity,
                min: validation::MIN_MAILBOX_CAPACITY,
                max: validation::MAX_MAILBOX_CAPACITY,
            });
        }

        if capacity > validation::MAX_MAILBOX_CAPACITY {
            return Err(ConfigValidationError::InvalidMailboxCapacity {
                capacity,
                min: validation::MIN_MAILBOX_CAPACITY,
                max: validation::MAX_MAILBOX_CAPACITY,
            });
        }

        Ok(())
    }

    /// Validate timeout duration is reasonable
    pub fn validate_timeout(timeout: Duration) -> Result<(), ConfigValidationError> {
        if timeout.is_zero() {
            return Err(ConfigValidationError::InvalidTimeout {
                timeout,
                reason: "Timeout cannot be zero".to_string(),
            });
        }

        if timeout > Duration::from_secs(3600) {
            return Err(ConfigValidationError::InvalidTimeout {
                timeout,
                reason: "Timeout too large (>1 hour)".to_string(),
            });
        }

        Ok(())
    }

    /// Validate restart strategy configuration
    pub fn validate_restart_strategy(strategy: &RestartStrategy) -> Result<(), ConfigValidationError> {
        strategy.validate().map_err(|e| ConfigValidationError::InvalidRestartStrategy {
            reason: e.to_string(),
        })
    }

    /// Generate recommended configuration for actor type
    pub fn recommend_config_for_actor(actor_type: &str) -> ActorRecommendations {
        let priority = match actor_type {
            "ChainActor" | "EngineActor" => ActorPriority::Critical,
            "BridgeActor" | "AuxPowMinerActor" => ActorPriority::High,
            "HealthMonitor" | "MetricsCollector" => ActorPriority::Background,
            _ => ActorPriority::Normal,
        };

        let mailbox_capacity = match priority {
            ActorPriority::Critical => 100000,
            ActorPriority::High => 50000,
            ActorPriority::Normal => 10000,
            ActorPriority::Low => 5000,
            ActorPriority::Background => 1000,
        };

        let restart_strategy = match priority {
            ActorPriority::Critical => RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(50),
                max_delay: Duration::from_secs(30),
                multiplier: 1.5,
                max_restarts: Some(20),
            },
            _ => RestartStrategy::default(),
        };

        ActorRecommendations {
            actor_type: actor_type.to_string(),
            recommended_priority: priority,
            recommended_mailbox_capacity: mailbox_capacity,
            recommended_restart_strategy: restart_strategy,
            health_check_interval: lifecycle_utils::calculate_health_check_interval(priority),
        }
    }
}

/// Metrics collection utilities
pub mod metrics_utils {
    use super::*;

    /// Actor performance metrics
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ActorMetrics {
        pub actor_type: String,
        pub message_count: u64,
        pub average_processing_time: Duration,
        pub error_count: u64,
        pub restart_count: u32,
        pub last_health_check: Option<SystemTime>,
        pub memory_usage: u64,
        pub cpu_usage: f64,
    }

    /// System-wide metrics aggregation
    #[derive(Debug, Clone, Default)]
    pub struct SystemMetrics {
        pub total_actors: usize,
        pub healthy_actors: usize,
        pub unhealthy_actors: usize,
        pub total_messages_processed: u64,
        pub total_errors: u64,
        pub total_restarts: u32,
        pub average_response_time: Duration,
        pub system_uptime: Duration,
        pub memory_usage: u64,
        pub cpu_usage: f64,
    }

    /// Calculate system health score (0-100)
    pub fn calculate_health_score(metrics: &SystemMetrics) -> u32 {
        if metrics.total_actors == 0 {
            return 0;
        }

        let health_ratio = metrics.healthy_actors as f64 / metrics.total_actors as f64;
        let error_penalty = if metrics.total_messages_processed > 0 {
            (metrics.total_errors as f64 / metrics.total_messages_processed as f64).min(1.0)
        } else {
            0.0
        };

        let score = (health_ratio * 100.0) - (error_penalty * 20.0);
        score.max(0.0).min(100.0) as u32
    }

    /// Determine system health status from metrics
    pub fn determine_health_status(metrics: &SystemMetrics) -> SystemHealth {
        let score = calculate_health_score(metrics);
        
        match score {
            90..=100 => SystemHealth::Healthy,
            70..=89 => SystemHealth::Warning,
            50..=69 => SystemHealth::Critical,
            _ => SystemHealth::Degraded,
        }
    }

    /// Create metrics summary for reporting
    pub fn create_metrics_summary(metrics: &SystemMetrics) -> MetricsSummary {
        MetricsSummary {
            timestamp: SystemTime::now(),
            health_score: calculate_health_score(metrics),
            health_status: determine_health_status(metrics),
            total_actors: metrics.total_actors,
            performance_summary: PerformanceSummary {
                messages_per_second: if metrics.system_uptime.as_secs() > 0 {
                    metrics.total_messages_processed / metrics.system_uptime.as_secs()
                } else {
                    0
                },
                error_rate: if metrics.total_messages_processed > 0 {
                    (metrics.total_errors as f64 / metrics.total_messages_processed as f64) * 100.0
                } else {
                    0.0
                },
                average_response_time: metrics.average_response_time,
                memory_usage_mb: metrics.memory_usage / (1024 * 1024),
                cpu_usage_percent: metrics.cpu_usage,
            },
        }
    }
}

/// Testing utilities for actor system
pub mod testing_utils {
    use super::*;

    /// Mock actor configuration for testing
    pub fn create_mock_actor_config(actor_type: &str) -> ActorTestConfig {
        ActorTestConfig {
            actor_type: actor_type.to_string(),
            priority: ActorPriority::Normal,
            restart_strategy: RestartStrategy::Always,
            mailbox_capacity: 1000,
            simulate_failures: false,
            failure_rate: 0.0,
            response_delay: Duration::from_millis(10),
        }
    }

    /// Create test configuration with minimal settings
    pub fn create_test_config() -> ActorSystemConfig {
        ActorSystemConfig::development()
    }

    /// Simulate actor failure for testing
    pub fn simulate_actor_failure(failure_type: TestFailureType) -> ActorFailureSimulation {
        ActorFailureSimulation {
            failure_type,
            timestamp: SystemTime::now(),
            recovery_time: match failure_type {
                TestFailureType::Panic => Duration::from_millis(100),
                TestFailureType::Timeout => Duration::from_secs(1),
                TestFailureType::HealthCheckFailure => Duration::from_millis(500),
                TestFailureType::ResourceExhaustion => Duration::from_secs(2),
            },
            requires_restart: true,
        }
    }

    /// Generate test workload for performance testing
    pub fn generate_test_workload(
        message_count: usize,
        message_rate: f64,
        duration: Duration,
    ) -> TestWorkload {
        TestWorkload {
            message_count,
            message_rate,
            duration,
            message_types: vec!["TestMessage".to_string()],
            concurrency_level: 10,
        }
    }
}

/// Error types for configuration validation
#[derive(Debug, Error)]
pub enum ConfigValidationError {
    #[error("Invalid mailbox capacity {capacity}: must be between {min} and {max}")]
    InvalidMailboxCapacity { capacity: usize, min: usize, max: usize },
    #[error("Invalid timeout {timeout:?}: {reason}")]
    InvalidTimeout { timeout: Duration, reason: String },
    #[error("Invalid restart strategy: {reason}")]
    InvalidRestartStrategy { reason: String },
    #[error("Configuration inconsistency: {reason}")]
    ConfigurationInconsistency { reason: String },
}

/// Actor configuration recommendations
#[derive(Debug, Clone)]
pub struct ActorRecommendations {
    pub actor_type: String,
    pub recommended_priority: ActorPriority,
    pub recommended_mailbox_capacity: usize,
    pub recommended_restart_strategy: RestartStrategy,
    pub health_check_interval: Duration,
}

/// Metrics summary for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub timestamp: SystemTime,
    pub health_score: u32,
    pub health_status: SystemHealth,
    pub total_actors: usize,
    pub performance_summary: PerformanceSummary,
}

/// Performance metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub messages_per_second: u64,
    pub error_rate: f64,
    pub average_response_time: Duration,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
}

/// Test configuration for actors
#[derive(Debug, Clone)]
pub struct ActorTestConfig {
    pub actor_type: String,
    pub priority: ActorPriority,
    pub restart_strategy: RestartStrategy,
    pub mailbox_capacity: usize,
    pub simulate_failures: bool,
    pub failure_rate: f64,
    pub response_delay: Duration,
}

/// Test failure types for simulation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestFailureType {
    /// Actor panic/crash
    Panic,
    /// Operation timeout
    Timeout,
    /// Health check failure
    HealthCheckFailure,
    /// Resource exhaustion
    ResourceExhaustion,
}

/// Actor failure simulation
#[derive(Debug, Clone)]
pub struct ActorFailureSimulation {
    pub failure_type: TestFailureType,
    pub timestamp: SystemTime,
    pub recovery_time: Duration,
    pub requires_restart: bool,
}

/// Test workload configuration
#[derive(Debug, Clone)]
pub struct TestWorkload {
    pub message_count: usize,
    pub message_rate: f64,
    pub duration: Duration,
    pub message_types: Vec<String>,
    pub concurrency_level: usize,
}

/// Utility functions for formatting and display
pub mod format_utils {
    use super::*;

    /// Format duration in human-readable form
    pub fn format_duration(duration: Duration) -> String {
        if duration < Duration::from_secs(1) {
            format!("{}ms", duration.as_millis())
        } else if duration < Duration::from_secs(60) {
            format!("{:.1}s", duration.as_secs_f64())
        } else if duration < Duration::from_secs(3600) {
            format!("{}m {:02}s", duration.as_secs() / 60, duration.as_secs() % 60)
        } else {
            format!("{}h {:02}m", duration.as_secs() / 3600, (duration.as_secs() % 3600) / 60)
        }
    }

    /// Format memory size in human-readable form
    pub fn format_memory(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} bytes", bytes)
        }
    }

    /// Format actor priority as string
    pub fn format_priority(priority: ActorPriority) -> &'static str {
        match priority {
            ActorPriority::Critical => "CRITICAL",
            ActorPriority::High => "HIGH",
            ActorPriority::Normal => "NORMAL",
            ActorPriority::Low => "LOW",
            ActorPriority::Background => "BACKGROUND",
        }
    }

    /// Format system health as colored string (for terminal output)
    pub fn format_health_status(status: SystemHealth) -> String {
        match status {
            SystemHealth::Healthy => "🟢 HEALTHY".to_string(),
            SystemHealth::Warning => "🟡 WARNING".to_string(),
            SystemHealth::Critical => "🔴 CRITICAL".to_string(),
            SystemHealth::Degraded => "⚫ DEGRADED".to_string(),
        }
    }
}

/// Display implementations for better debugging
impl fmt::Display for ActorPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_utils::format_priority(*self))
    }
}

impl fmt::Display for SystemHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_utils::format_health_status(self.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_blockchain_utils_next_block_boundary() {
        let now = SystemTime::now();
        let next_boundary = blockchain_utils::next_block_boundary(now);
        assert!(next_boundary > now);
        
        let time_to_boundary = next_boundary.duration_since(now).unwrap();
        assert!(time_to_boundary <= blockchain::BLOCK_INTERVAL);
    }

    #[test]
    fn test_blockchain_utils_production_window() {
        // Note: This test depends on current time, so it may be flaky
        let _is_production = blockchain_utils::is_block_production_window();
        let delay = blockchain_utils::delay_to_next_production_window();
        assert!(delay <= blockchain::BLOCK_INTERVAL);
    }

    #[test]
    fn test_lifecycle_utils_startup_order() {
        let mut configs = HashMap::new();
        configs.insert("Actor1".to_string(), ActorPriority::Normal);
        configs.insert("Actor2".to_string(), ActorPriority::Critical);
        configs.insert("Actor3".to_string(), ActorPriority::Background);
        
        let order = lifecycle_utils::calculate_startup_order(&configs);
        
        // Critical should come first, Background last
        assert_eq!(order[0], "Actor2"); // Critical
        assert_eq!(order[2], "Actor3"); // Background
    }

    #[test]
    fn test_config_utils_mailbox_validation() {
        // Valid capacity
        assert!(config_utils::validate_mailbox_capacity(1000).is_ok());
        
        // Too small
        assert!(config_utils::validate_mailbox_capacity(50).is_err());
        
        // Too large
        assert!(config_utils::validate_mailbox_capacity(2_000_000).is_err());
    }

    #[test]
    fn test_config_utils_timeout_validation() {
        // Valid timeout
        assert!(config_utils::validate_timeout(Duration::from_secs(10)).is_ok());
        
        // Zero timeout
        assert!(config_utils::validate_timeout(Duration::ZERO).is_err());
        
        // Too large timeout
        assert!(config_utils::validate_timeout(Duration::from_secs(7200)).is_err());
    }

    #[test]
    fn test_metrics_utils_health_score() {
        let metrics = metrics_utils::SystemMetrics {
            total_actors: 10,
            healthy_actors: 9,
            unhealthy_actors: 1,
            total_messages_processed: 1000,
            total_errors: 10,
            ..Default::default()
        };
        
        let score = metrics_utils::calculate_health_score(&metrics);
        assert!(score >= 70); // Should be good health with 90% healthy actors
        assert!(score <= 100);
    }

    #[test]
    fn test_metrics_utils_health_status() {
        let high_score_metrics = metrics_utils::SystemMetrics {
            total_actors: 10,
            healthy_actors: 10,
            total_messages_processed: 1000,
            total_errors: 1,
            ..Default::default()
        };
        
        let status = metrics_utils::determine_health_status(&high_score_metrics);
        assert!(matches!(status, SystemHealth::Healthy | SystemHealth::Warning));
    }

    #[test]
    fn test_format_utils_duration() {
        assert_eq!(format_utils::format_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(format_utils::format_duration(Duration::from_secs(5)), "5.0s");
        assert_eq!(format_utils::format_duration(Duration::from_secs(125)), "2m 05s");
    }

    #[test]
    fn test_format_utils_memory() {
        assert_eq!(format_utils::format_memory(512), "512 bytes");
        assert_eq!(format_utils::format_memory(1536), "1.5 KB");
        assert_eq!(format_utils::format_memory(2048 * 1024), "2.0 MB");
    }

    #[test]
    fn test_testing_utils_mock_config() {
        let config = testing_utils::create_mock_actor_config("TestActor");
        assert_eq!(config.actor_type, "TestActor");
        assert_eq!(config.priority, ActorPriority::Normal);
    }

    #[test]
    fn test_testing_utils_failure_simulation() {
        let failure = testing_utils::simulate_actor_failure(TestFailureType::Panic);
        assert_eq!(failure.failure_type, TestFailureType::Panic);
        assert!(failure.requires_restart);
    }

    #[test]
    fn test_lifecycle_utils_should_restart() {
        let strategy = RestartStrategy::Always;
        
        // Should restart with Always strategy
        assert!(lifecycle_utils::should_restart_actor(
            &strategy,
            0,
            None,
            &RestartReason::ActorPanic
        ));
        
        // Should not restart if too soon after last restart
        let recent_restart = Some(SystemTime::now() - Duration::from_millis(50));
        assert!(!lifecycle_utils::should_restart_actor(
            &strategy,
            1,
            recent_restart,
            &RestartReason::ActorPanic
        ));
    }

    #[test]
    fn test_config_utils_actor_recommendations() {
        let recommendations = config_utils::recommend_config_for_actor("ChainActor");
        assert_eq!(recommendations.recommended_priority, ActorPriority::Critical);
        assert!(recommendations.recommended_mailbox_capacity > 50000);
        
        let bg_recommendations = config_utils::recommend_config_for_actor("HealthMonitor");
        assert_eq!(bg_recommendations.recommended_priority, ActorPriority::Background);
    }
}