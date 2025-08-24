//! Chain Actor Supervision
//!
//! Supervision strategies and health monitoring for ChainActor.
//! This module provides blockchain-specific supervision policies that understand
//! the timing constraints and fault tolerance requirements of consensus systems.

use std::time::Duration;
use actor_system::{
    SupervisionPolicy, SupervisionStrategy, RestartStrategy,
    BlockchainSupervisionPolicy, BlockchainRestartStrategy,
};
use super::config::ChainActorConfig;

/// Blockchain-specific supervision strategy for ChainActor
#[derive(Debug, Clone)]
pub struct ChainSupervisionStrategy {
    /// Base supervision policy
    policy: BlockchainSupervisionPolicy,
    
    /// Configuration for chain-specific supervision
    config: ChainSupervisionConfig,
}

/// Configuration for chain actor supervision
#[derive(Debug, Clone)]
pub struct ChainSupervisionConfig {
    /// Maximum restart attempts before giving up
    pub max_restart_attempts: u32,
    
    /// Restart delay aligned to block boundaries
    pub restart_delay: Duration,
    
    /// Whether to pause block production during restart
    pub pause_production_on_restart: bool,
    
    /// Health check interval for monitoring
    pub health_check_interval: Duration,
    
    /// Minimum health score before restart
    pub min_health_score: u8,
}

impl ChainSupervisionStrategy {
    /// Create a new chain supervision strategy
    pub fn new(chain_config: &ChainActorConfig) -> Self {
        let supervision_config = ChainSupervisionConfig {
            max_restart_attempts: 5,
            restart_delay: chain_config.slot_duration, // Align to block timing
            pause_production_on_restart: true,
            health_check_interval: Duration::from_secs(30),
            min_health_score: 70,
        };
        
        let policy = BlockchainSupervisionPolicy {
            base_policy: SupervisionPolicy {
                strategy: SupervisionStrategy::OneForOne,
                max_restart_frequency: 5,
                restart_window: Duration::from_secs(60),
                escalation_strategy: actor_system::EscalationStrategy::Restart,
            },
            blockchain_restart: BlockchainRestartStrategy::BlockAligned {
                slot_duration: chain_config.slot_duration,
                max_delay: Duration::from_secs(10),
            },
            federation_requirements: Some(actor_system::FederationHealthRequirement {
                min_healthy_members: 3,
                health_check_timeout: Duration::from_secs(5),
            }),
        };
        
        Self {
            policy,
            config: supervision_config,
        }
    }
    
    /// Get the supervision policy
    pub fn policy(&self) -> &BlockchainSupervisionPolicy {
        &self.policy
    }
    
    /// Get supervision configuration
    pub fn config(&self) -> &ChainSupervisionConfig {
        &self.config
    }
    
    /// Check if actor should be restarted based on health
    pub fn should_restart(&self, health_score: u8, consecutive_failures: u32) -> bool {
        health_score < self.config.min_health_score ||
        consecutive_failures >= self.config.max_restart_attempts
    }
    
    /// Calculate restart delay based on failure count
    pub fn restart_delay(&self, failure_count: u32) -> Duration {
        // Exponential backoff aligned to block boundaries
        let base_delay = self.config.restart_delay;
        let multiplier = 2_u32.pow(failure_count.min(5));
        base_delay * multiplier
    }
    
    /// Create supervision strategy for production environment
    pub fn production(chain_config: &ChainActorConfig) -> Self {
        let mut strategy = Self::new(chain_config);
        strategy.config.max_restart_attempts = 3;
        strategy.config.min_health_score = 80;
        strategy.config.health_check_interval = Duration::from_secs(15);
        strategy
    }
    
    /// Create supervision strategy for development environment
    pub fn development(chain_config: &ChainActorConfig) -> Self {
        let mut strategy = Self::new(chain_config);
        strategy.config.max_restart_attempts = 10;
        strategy.config.min_health_score = 50;
        strategy.config.health_check_interval = Duration::from_secs(60);
        strategy.config.pause_production_on_restart = false;
        strategy
    }
}

impl Default for ChainSupervisionStrategy {
    fn default() -> Self {
        // Create with default chain config
        Self::new(&ChainActorConfig::default())
    }
}