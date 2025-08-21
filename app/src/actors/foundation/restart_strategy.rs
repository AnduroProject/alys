//! Enhanced Restart Strategy - ALYS-006-02 Implementation
//! 
//! Comprehensive restart strategies for Alys V2 actor system with
//! blockchain-specific timing considerations, exponential backoff,
//! fixed delays, and integration with the sidechain consensus system.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// Enhanced restart strategy for failed actors in the Alys sidechain
/// 
/// Provides comprehensive restart policies with blockchain-aware timing
/// that respects the 2-second block intervals and consensus coordination
/// requirements of the Alys merged mining sidechain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RestartStrategy {
    /// Always restart immediately without delay
    /// 
    /// Used for critical actors that must be available immediately,
    /// such as consensus coordinators and health monitors.
    Always,
    
    /// Never restart the actor
    /// 
    /// Used for actors that should fail permanently and not be
    /// automatically recovered, typically during graceful shutdowns
    /// or when manual intervention is required.
    Never,
    
    /// Restart with exponential backoff
    /// 
    /// Provides intelligent restart timing that backs off exponentially
    /// to prevent thundering herd problems while respecting blockchain
    /// timing constraints. Ideal for most blockchain actors.
    ExponentialBackoff {
        /// Initial delay before first restart
        initial_delay: Duration,
        /// Maximum delay between restart attempts
        max_delay: Duration,
        /// Multiplier for exponential backoff (typically 1.5 - 2.0)
        multiplier: f64,
        /// Maximum number of restart attempts (None = unlimited)
        max_restarts: Option<usize>,
    },
    
    /// Restart with fixed delay
    /// 
    /// Provides consistent restart timing suitable for actors that
    /// need predictable restart intervals, such as network actors
    /// or periodic data processors.
    FixedDelay {
        /// Fixed delay between restart attempts
        delay: Duration,
        /// Maximum number of restart attempts (None = unlimited)
        max_restarts: Option<usize>,
    },
    
    /// Progressive restart with increasing delays
    /// 
    /// Combines elements of fixed delay and exponential backoff,
    /// useful for actors that need graduated recovery timing.
    Progressive {
        /// Initial delay for first restart
        initial_delay: Duration,
        /// Maximum number of restart attempts
        max_attempts: usize,
        /// Delay increase per attempt
        delay_increment: Duration,
        /// Maximum delay cap
        max_delay: Duration,
    },
    
    /// Blockchain-aware restart strategy
    /// 
    /// Aligns restart timing with blockchain operations, ensuring
    /// restarts occur at optimal points in the block production cycle.
    BlockchainAware {
        /// Base restart strategy
        base_strategy: Box<RestartStrategy>,
        /// Wait for next block boundary before restart
        align_to_block_boundary: bool,
        /// Consider consensus state before restart
        respect_consensus_state: bool,
        /// Avoid restarts during critical consensus operations
        avoid_consensus_conflicts: bool,
    },
}

/// Restart attempt tracking and state management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartAttempt {
    /// Attempt number (0-indexed)
    pub attempt_number: usize,
    /// Timestamp of this attempt
    pub timestamp: std::time::SystemTime,
    /// Calculated delay for this attempt
    pub delay: Duration,
    /// Reason for restart
    pub reason: RestartReason,
    /// Success of this attempt
    pub successful: Option<bool>,
}

/// Reasons for actor restart
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RestartReason {
    /// Actor panic or unhandled error
    ActorPanic,
    /// Health check failure
    HealthCheckFailure,
    /// Supervision escalation
    SupervisionEscalation,
    /// Manual restart request
    ManualRestart,
    /// Configuration change
    ConfigurationChange,
    /// Blockchain consensus failure
    ConsensusFailure,
    /// Network partition or connectivity issue
    NetworkFailure,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Dependency failure
    DependencyFailure,
}

/// Restart strategy calculation errors
#[derive(Debug, Error)]
pub enum RestartStrategyError {
    #[error("Maximum restart attempts exceeded: {max_attempts}")]
    MaxAttemptsExceeded { max_attempts: usize },
    #[error("Invalid strategy configuration: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("Blockchain coordination failure: {reason}")]
    BlockchainCoordinationFailure { reason: String },
    #[error("Strategy calculation error: {reason}")]
    CalculationError { reason: String },
}

impl Default for RestartStrategy {
    /// Default restart strategy optimized for Alys blockchain operations
    fn default() -> Self {
        RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_restarts: Some(10),
        }
    }
}

impl RestartStrategy {
    /// Calculate the delay for a restart attempt
    /// 
    /// # Arguments
    /// * `attempt` - The attempt number (0-indexed)
    /// * `last_attempts` - Previous restart attempts for context
    /// 
    /// # Returns
    /// * `Some(Duration)` - Delay before restart
    /// * `None` - No restart should be attempted
    pub fn calculate_delay(
        &self,
        attempt: usize,
        last_attempts: &[RestartAttempt],
    ) -> Result<Option<Duration>, RestartStrategyError> {
        match self {
            RestartStrategy::Always => Ok(Some(Duration::ZERO)),
            
            RestartStrategy::Never => Ok(None),
            
            RestartStrategy::ExponentialBackoff {
                initial_delay,
                max_delay,
                multiplier,
                max_restarts,
            } => {
                if let Some(max) = max_restarts {
                    if attempt >= *max {
                        return Err(RestartStrategyError::MaxAttemptsExceeded {
                            max_attempts: *max,
                        });
                    }
                }
                
                let delay = Self::calculate_exponential_backoff(
                    *initial_delay,
                    *max_delay,
                    *multiplier,
                    attempt,
                )?;
                
                Ok(Some(delay))
            }
            
            RestartStrategy::FixedDelay { delay, max_restarts } => {
                if let Some(max) = max_restarts {
                    if attempt >= *max {
                        return Err(RestartStrategyError::MaxAttemptsExceeded {
                            max_attempts: *max,
                        });
                    }
                }
                
                Ok(Some(*delay))
            }
            
            RestartStrategy::Progressive {
                initial_delay,
                max_attempts,
                delay_increment,
                max_delay,
            } => {
                if attempt >= *max_attempts {
                    return Err(RestartStrategyError::MaxAttemptsExceeded {
                        max_attempts: *max_attempts,
                    });
                }
                
                let delay = *initial_delay + (*delay_increment * attempt as u32);
                let capped_delay = delay.min(*max_delay);
                
                Ok(Some(capped_delay))
            }
            
            RestartStrategy::BlockchainAware {
                base_strategy,
                align_to_block_boundary,
                respect_consensus_state,
                avoid_consensus_conflicts,
            } => {
                // First calculate base delay
                let base_delay = base_strategy.calculate_delay(attempt, last_attempts)?;
                
                if let Some(mut delay) = base_delay {
                    // Apply blockchain-aware adjustments
                    if *align_to_block_boundary {
                        delay = Self::align_to_next_block_boundary(delay);
                    }
                    
                    if *respect_consensus_state {
                        delay = Self::adjust_for_consensus_state(delay)?;
                    }
                    
                    if *avoid_consensus_conflicts {
                        delay = Self::avoid_consensus_conflicts(delay)?;
                    }
                    
                    Ok(Some(delay))
                } else {
                    Ok(None)
                }
            }
        }
    }
    
    /// Calculate exponential backoff delay
    fn calculate_exponential_backoff(
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        attempt: usize,
    ) -> Result<Duration, RestartStrategyError> {
        if multiplier <= 1.0 {
            return Err(RestartStrategyError::InvalidConfiguration {
                reason: format!("Multiplier must be > 1.0, got {}", multiplier),
            });
        }
        
        let delay_ms = initial_delay.as_millis() as f64 * multiplier.powi(attempt as i32);
        let capped_delay_ms = delay_ms.min(max_delay.as_millis() as f64);
        
        // Prevent overflow
        if capped_delay_ms > u64::MAX as f64 {
            return Ok(max_delay);
        }
        
        Ok(Duration::from_millis(capped_delay_ms as u64))
    }
    
    /// Align restart to next block boundary (2-second intervals for Alys)
    fn align_to_next_block_boundary(delay: Duration) -> Duration {
        const BLOCK_INTERVAL: Duration = Duration::from_secs(2);
        
        let blocks_to_wait = (delay.as_millis() / BLOCK_INTERVAL.as_millis()) + 1;
        Duration::from_millis(blocks_to_wait * BLOCK_INTERVAL.as_millis())
    }
    
    /// Adjust delay based on current consensus state
    fn adjust_for_consensus_state(delay: Duration) -> Result<Duration, RestartStrategyError> {
        // TODO: Integration with consensus state monitoring
        // For now, add a small buffer to avoid consensus disruption
        Ok(delay + Duration::from_millis(500))
    }
    
    /// Avoid restarting during critical consensus operations
    fn avoid_consensus_conflicts(delay: Duration) -> Result<Duration, RestartStrategyError> {
        // TODO: Integration with consensus scheduler
        // For now, ensure minimum delay to avoid block production conflicts
        const MIN_CONSENSUS_SAFE_DELAY: Duration = Duration::from_millis(1500);
        Ok(delay.max(MIN_CONSENSUS_SAFE_DELAY))
    }
    
    /// Check if this strategy should restart given the current context
    pub fn should_restart(
        &self,
        attempt: usize,
        reason: &RestartReason,
        last_attempts: &[RestartAttempt],
    ) -> bool {
        match self {
            RestartStrategy::Never => false,
            RestartStrategy::Always => true,
            _ => {
                // Check if we can calculate a delay (respects max attempts)
                self.calculate_delay(attempt, last_attempts).is_ok()
            }
        }
    }
    
    /// Create a blockchain-aware variant of this strategy
    pub fn make_blockchain_aware(
        self,
        align_to_block_boundary: bool,
        respect_consensus_state: bool,
        avoid_consensus_conflicts: bool,
    ) -> Self {
        RestartStrategy::BlockchainAware {
            base_strategy: Box::new(self),
            align_to_block_boundary,
            respect_consensus_state,
            avoid_consensus_conflicts,
        }
    }
    
    /// Get strategy name for logging and metrics
    pub fn strategy_name(&self) -> &'static str {
        match self {
            RestartStrategy::Always => "always",
            RestartStrategy::Never => "never",
            RestartStrategy::ExponentialBackoff { .. } => "exponential_backoff",
            RestartStrategy::FixedDelay { .. } => "fixed_delay",
            RestartStrategy::Progressive { .. } => "progressive",
            RestartStrategy::BlockchainAware { .. } => "blockchain_aware",
        }
    }
    
    /// Validate strategy configuration
    pub fn validate(&self) -> Result<(), RestartStrategyError> {
        match self {
            RestartStrategy::ExponentialBackoff {
                initial_delay,
                max_delay,
                multiplier,
                ..
            } => {
                if initial_delay.is_zero() {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Initial delay cannot be zero".to_string(),
                    });
                }
                
                if *max_delay < *initial_delay {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Max delay must be >= initial delay".to_string(),
                    });
                }
                
                if *multiplier <= 1.0 {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: format!("Multiplier must be > 1.0, got {}", multiplier),
                    });
                }
            }
            
            RestartStrategy::FixedDelay { delay, .. } => {
                if delay.is_zero() {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Fixed delay cannot be zero".to_string(),
                    });
                }
            }
            
            RestartStrategy::Progressive {
                initial_delay,
                max_attempts,
                delay_increment,
                max_delay,
            } => {
                if initial_delay.is_zero() {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Initial delay cannot be zero".to_string(),
                    });
                }
                
                if *max_attempts == 0 {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Max attempts must be > 0".to_string(),
                    });
                }
                
                if delay_increment.is_zero() {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Delay increment cannot be zero".to_string(),
                    });
                }
                
                if *max_delay < *initial_delay {
                    return Err(RestartStrategyError::InvalidConfiguration {
                        reason: "Max delay must be >= initial delay".to_string(),
                    });
                }
            }
            
            RestartStrategy::BlockchainAware { base_strategy, .. } => {
                base_strategy.validate()?;
            }
            
            RestartStrategy::Always | RestartStrategy::Never => {
                // These strategies have no configuration to validate
            }
        }
        
        Ok(())
    }
}

/// Builder for creating restart strategies with fluent API
pub struct RestartStrategyBuilder {
    strategy: RestartStrategy,
}

impl RestartStrategyBuilder {
    /// Create new builder with default strategy
    pub fn new() -> Self {
        Self {
            strategy: RestartStrategy::default(),
        }
    }
    
    /// Set to always restart
    pub fn always(mut self) -> Self {
        self.strategy = RestartStrategy::Always;
        self
    }
    
    /// Set to never restart
    pub fn never(mut self) -> Self {
        self.strategy = RestartStrategy::Never;
        self
    }
    
    /// Set exponential backoff strategy
    pub fn exponential_backoff(
        mut self,
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
    ) -> Self {
        self.strategy = RestartStrategy::ExponentialBackoff {
            initial_delay,
            max_delay,
            multiplier,
            max_restarts: None,
        };
        self
    }
    
    /// Set fixed delay strategy
    pub fn fixed_delay(mut self, delay: Duration) -> Self {
        self.strategy = RestartStrategy::FixedDelay {
            delay,
            max_restarts: None,
        };
        self
    }
    
    /// Set maximum restart attempts
    pub fn max_restarts(mut self, max_restarts: usize) -> Self {
        match &mut self.strategy {
            RestartStrategy::ExponentialBackoff { max_restarts: ref mut max, .. } => {
                *max = Some(max_restarts);
            }
            RestartStrategy::FixedDelay { max_restarts: ref mut max, .. } => {
                *max = Some(max_restarts);
            }
            _ => {
                // For other strategies, wrap in exponential backoff
                self.strategy = RestartStrategy::ExponentialBackoff {
                    initial_delay: Duration::from_millis(100),
                    max_delay: Duration::from_secs(60),
                    multiplier: 2.0,
                    max_restarts: Some(max_restarts),
                };
            }
        }
        self
    }
    
    /// Make strategy blockchain-aware
    pub fn blockchain_aware(
        mut self,
        align_to_block_boundary: bool,
        respect_consensus_state: bool,
        avoid_consensus_conflicts: bool,
    ) -> Self {
        self.strategy = self.strategy.make_blockchain_aware(
            align_to_block_boundary,
            respect_consensus_state,
            avoid_consensus_conflicts,
        );
        self
    }
    
    /// Build the restart strategy
    pub fn build(self) -> Result<RestartStrategy, RestartStrategyError> {
        self.strategy.validate()?;
        Ok(self.strategy)
    }
}

impl Default for RestartStrategyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    
    fn create_test_attempts(count: usize) -> Vec<RestartAttempt> {
        (0..count)
            .map(|i| RestartAttempt {
                attempt_number: i,
                timestamp: SystemTime::now(),
                delay: Duration::from_millis(100 * (i + 1) as u64),
                reason: RestartReason::ActorPanic,
                successful: Some(false),
            })
            .collect()
    }
    
    #[test]
    fn test_always_restart_strategy() {
        let strategy = RestartStrategy::Always;
        let attempts = create_test_attempts(0);
        
        let delay = strategy.calculate_delay(0, &attempts).unwrap();
        assert_eq!(delay, Some(Duration::ZERO));
        
        let delay = strategy.calculate_delay(100, &attempts).unwrap();
        assert_eq!(delay, Some(Duration::ZERO));
        
        assert!(strategy.should_restart(0, &RestartReason::ActorPanic, &attempts));
        assert!(strategy.should_restart(100, &RestartReason::ActorPanic, &attempts));
    }
    
    #[test]
    fn test_never_restart_strategy() {
        let strategy = RestartStrategy::Never;
        let attempts = create_test_attempts(0);
        
        let delay = strategy.calculate_delay(0, &attempts).unwrap();
        assert_eq!(delay, None);
        
        let delay = strategy.calculate_delay(100, &attempts).unwrap();
        assert_eq!(delay, None);
        
        assert!(!strategy.should_restart(0, &RestartReason::ActorPanic, &attempts));
        assert!(!strategy.should_restart(100, &RestartReason::ActorPanic, &attempts));
    }
    
    #[test]
    fn test_exponential_backoff_strategy() {
        let strategy = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
            max_restarts: Some(5),
        };
        let attempts = create_test_attempts(0);
        
        // Test first few attempts
        assert_eq!(
            strategy.calculate_delay(0, &attempts).unwrap(),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            strategy.calculate_delay(1, &attempts).unwrap(),
            Some(Duration::from_millis(200))
        );
        assert_eq!(
            strategy.calculate_delay(2, &attempts).unwrap(),
            Some(Duration::from_millis(400))
        );
        
        // Test max attempts exceeded
        assert!(strategy.calculate_delay(5, &attempts).is_err());
    }
    
    #[test]
    fn test_fixed_delay_strategy() {
        let strategy = RestartStrategy::FixedDelay {
            delay: Duration::from_secs(5),
            max_restarts: Some(3),
        };
        let attempts = create_test_attempts(0);
        
        // All attempts within limit should return same delay
        assert_eq!(
            strategy.calculate_delay(0, &attempts).unwrap(),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            strategy.calculate_delay(1, &attempts).unwrap(),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            strategy.calculate_delay(2, &attempts).unwrap(),
            Some(Duration::from_secs(5))
        );
        
        // Max attempts exceeded
        assert!(strategy.calculate_delay(3, &attempts).is_err());
    }
    
    #[test]
    fn test_progressive_strategy() {
        let strategy = RestartStrategy::Progressive {
            initial_delay: Duration::from_millis(100),
            max_attempts: 4,
            delay_increment: Duration::from_millis(50),
            max_delay: Duration::from_millis(500),
        };
        let attempts = create_test_attempts(0);
        
        // Test progressive delays
        assert_eq!(
            strategy.calculate_delay(0, &attempts).unwrap(),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            strategy.calculate_delay(1, &attempts).unwrap(),
            Some(Duration::from_millis(150))
        );
        assert_eq!(
            strategy.calculate_delay(2, &attempts).unwrap(),
            Some(Duration::from_millis(200))
        );
        
        // Should cap at max_delay
        assert_eq!(
            strategy.calculate_delay(10, &attempts).unwrap(),
            Some(Duration::from_millis(500))
        );
        
        // Max attempts exceeded
        assert!(strategy.calculate_delay(4, &attempts).is_err());
    }
    
    #[test]
    fn test_blockchain_aware_strategy() {
        let base_strategy = RestartStrategy::FixedDelay {
            delay: Duration::from_millis(1000),
            max_restarts: Some(5),
        };
        
        let strategy = RestartStrategy::BlockchainAware {
            base_strategy: Box::new(base_strategy),
            align_to_block_boundary: true,
            respect_consensus_state: true,
            avoid_consensus_conflicts: true,
        };
        
        let attempts = create_test_attempts(0);
        let delay = strategy.calculate_delay(0, &attempts).unwrap();
        
        // Should have additional delays from blockchain awareness
        assert!(delay.unwrap() > Duration::from_millis(1000));
    }
    
    #[test]
    fn test_strategy_builder() {
        let strategy = RestartStrategyBuilder::new()
            .exponential_backoff(
                Duration::from_millis(50),
                Duration::from_secs(30),
                1.5
            )
            .max_restarts(10)
            .blockchain_aware(true, true, false)
            .build()
            .unwrap();
        
        // Should be blockchain aware
        assert!(matches!(strategy, RestartStrategy::BlockchainAware { .. }));
    }
    
    #[test]
    fn test_strategy_validation() {
        // Valid strategy
        let valid_strategy = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_restarts: Some(10),
        };
        assert!(valid_strategy.validate().is_ok());
        
        // Invalid multiplier
        let invalid_strategy = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 0.5, // Invalid: must be > 1.0
            max_restarts: Some(10),
        };
        assert!(invalid_strategy.validate().is_err());
        
        // Invalid delay relationship
        let invalid_delay_strategy = RestartStrategy::ExponentialBackoff {
            initial_delay: Duration::from_secs(60),
            max_delay: Duration::from_millis(100), // Invalid: max < initial
            multiplier: 2.0,
            max_restarts: Some(10),
        };
        assert!(invalid_delay_strategy.validate().is_err());
    }
    
    #[test]
    fn test_block_boundary_alignment() {
        let delay = Duration::from_millis(1500);
        let aligned = RestartStrategy::align_to_next_block_boundary(delay);
        
        // Should align to next 2-second boundary
        assert_eq!(aligned, Duration::from_secs(2));
        
        let longer_delay = Duration::from_millis(5500);
        let aligned_longer = RestartStrategy::align_to_next_block_boundary(longer_delay);
        
        // Should align to next 2-second boundary after 5.5 seconds
        assert_eq!(aligned_longer, Duration::from_secs(6));
    }
    
    #[test]
    fn test_strategy_names() {
        assert_eq!(RestartStrategy::Always.strategy_name(), "always");
        assert_eq!(RestartStrategy::Never.strategy_name(), "never");
        assert_eq!(
            RestartStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(60),
                multiplier: 2.0,
                max_restarts: Some(10),
            }.strategy_name(),
            "exponential_backoff"
        );
    }
}