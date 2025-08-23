//! Exponential backoff reconnection strategy for governance stream connections
//!
//! This module implements a robust reconnection system with exponential backoff,
//! jitter, circuit breaker patterns, and advanced failure detection. It ensures
//! reliable connection recovery while preventing thundering herd effects and
//! graceful degradation under persistent failures.

use crate::actors::governance_stream::error::*;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime};
use tracing::*;

/// Exponential backoff reconnection strategy with jitter and circuit breaker
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    /// Configuration parameters
    config: BackoffConfig,
    /// Current state
    state: BackoffState,
    /// Failure statistics
    stats: BackoffStats,
    /// Circuit breaker state
    circuit_breaker: CircuitBreakerState,
}

/// Configuration for exponential backoff strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    /// Initial delay between reconnection attempts
    pub initial_delay: Duration,
    /// Maximum delay between attempts (cap)
    pub max_delay: Duration,
    /// Backoff multiplier for exponential growth
    pub multiplier: f64,
    /// Maximum number of consecutive attempts before giving up
    pub max_attempts: Option<u32>,
    /// Whether to add jitter to prevent thundering herd
    pub use_jitter: bool,
    /// Jitter factor (0.0 to 1.0) - percentage of delay to randomize
    pub jitter_factor: f64,
    /// Reset attempt count after successful connection lasting this long
    pub reset_threshold: Duration,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
}

/// Current state of the backoff strategy
#[derive(Debug, Clone)]
struct BackoffState {
    /// Current attempt number (resets on success)
    attempt_count: u32,
    /// Last attempt timestamp
    last_attempt: Option<Instant>,
    /// Last successful connection timestamp
    last_success: Option<Instant>,
    /// Current delay for next attempt
    current_delay: Duration,
    /// Whether backoff is active
    active: bool,
}

/// Statistics for backoff performance monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffStats {
    /// Total reconnection attempts made
    pub total_attempts: u64,
    /// Total successful reconnections
    pub successful_reconnections: u64,
    /// Total failed attempts
    pub failed_attempts: u64,
    /// Average time to successful reconnection
    pub avg_reconnection_time: Duration,
    /// Maximum consecutive failures
    pub max_consecutive_failures: u32,
    /// Current consecutive failures
    pub current_consecutive_failures: u32,
    /// Last reset timestamp
    pub last_reset: Option<SystemTime>,
    /// Time spent in backoff state
    pub total_backoff_time: Duration,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Enable circuit breaker functionality
    pub enabled: bool,
    /// Failure threshold to trip circuit breaker
    pub failure_threshold: u32,
    /// Time to wait before attempting to close circuit
    pub recovery_timeout: Duration,
    /// Number of test attempts in half-open state
    pub test_attempts: u32,
    /// Success rate required to close circuit (0.0 to 1.0)
    pub success_rate_threshold: f64,
    /// Time window for calculating success rate
    pub success_rate_window: Duration,
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    /// Circuit is closed - normal operation
    Closed,
    /// Circuit is open - failing fast
    Open { opened_at: Instant },
    /// Circuit is half-open - testing recovery
    HalfOpen { test_attempts: u32 },
}

/// Backoff decision result
#[derive(Debug, Clone)]
pub enum BackoffDecision {
    /// Proceed with reconnection attempt
    Proceed,
    /// Wait for specified duration before next attempt
    Wait { delay: Duration },
    /// Give up - max attempts reached
    GiveUp { reason: BackoffGiveUpReason },
    /// Circuit breaker is open - fail fast
    CircuitOpen { recovery_time: Duration },
}

/// Reasons for giving up reconnection attempts
#[derive(Debug, Clone)]
pub enum BackoffGiveUpReason {
    /// Maximum attempts exceeded
    MaxAttemptsExceeded { max_attempts: u32 },
    /// Circuit breaker permanently open
    CircuitBreakerPermanent,
    /// Configuration prevents further attempts
    ConfigurationRestriction,
    /// External signal to stop
    ExternalStop,
}

/// Result of a reconnection attempt
#[derive(Debug, Clone)]
pub enum ReconnectionResult {
    /// Connection successful
    Success,
    /// Connection failed with retryable error
    RetryableFailure { error: ConnectionError },
    /// Connection failed with permanent error
    PermanentFailure { error: ConnectionError },
    /// Connection cancelled
    Cancelled,
}

impl ExponentialBackoff {
    /// Create new exponential backoff strategy with configuration
    pub fn new(config: BackoffConfig) -> Self {
        Self {
            config: config.clone(),
            state: BackoffState {
                attempt_count: 0,
                last_attempt: None,
                last_success: None,
                current_delay: config.initial_delay,
                active: false,
            },
            stats: BackoffStats::default(),
            circuit_breaker: CircuitBreakerState::Closed,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(BackoffConfig::default())
    }

    /// Create with custom delays
    pub fn with_delays(initial: Duration, max: Duration, multiplier: f64) -> Self {
        let mut config = BackoffConfig::default();
        config.initial_delay = initial;
        config.max_delay = max;
        config.multiplier = multiplier;
        Self::new(config)
    }

    /// Get next backoff decision
    pub fn next_attempt(&mut self) -> BackoffDecision {
        let now = Instant::now();

        // Check circuit breaker state
        if let Some(circuit_decision) = self.check_circuit_breaker(now) {
            return circuit_decision;
        }

        // Check if we've exceeded maximum attempts
        if let Some(max_attempts) = self.config.max_attempts {
            if self.state.attempt_count >= max_attempts {
                return BackoffDecision::GiveUp {
                    reason: BackoffGiveUpReason::MaxAttemptsExceeded { max_attempts },
                };
            }
        }

        // If this is the first attempt or we should proceed immediately
        if self.state.attempt_count == 0 || !self.state.active {
            self.state.active = true;
            return BackoffDecision::Proceed;
        }

        // Calculate delay for next attempt
        let delay = self.calculate_delay();
        
        // Check if enough time has passed since last attempt
        if let Some(last_attempt) = self.state.last_attempt {
            let elapsed = now.duration_since(last_attempt);
            if elapsed < delay {
                return BackoffDecision::Wait {
                    delay: delay - elapsed,
                };
            }
        }

        BackoffDecision::Proceed
    }

    /// Record the result of a reconnection attempt
    pub fn record_attempt(&mut self, result: ReconnectionResult) {
        let now = Instant::now();
        self.state.last_attempt = Some(now);
        self.state.attempt_count += 1;
        self.stats.total_attempts += 1;

        match result {
            ReconnectionResult::Success => {
                self.record_success(now);
            }
            ReconnectionResult::RetryableFailure { error } => {
                self.record_failure(error, true);
            }
            ReconnectionResult::PermanentFailure { error } => {
                self.record_failure(error, false);
            }
            ReconnectionResult::Cancelled => {
                // Don't count cancellations as failures
                self.state.attempt_count = self.state.attempt_count.saturating_sub(1);
                self.stats.total_attempts = self.stats.total_attempts.saturating_sub(1);
            }
        }

        // Update current delay for next attempt
        self.state.current_delay = self.calculate_delay();
    }

    /// Record successful connection
    fn record_success(&mut self, timestamp: Instant) {
        info!(
            "Reconnection successful after {} attempts in {:?}",
            self.state.attempt_count,
            self.state.last_attempt
                .and_then(|last| self.state.last_success.map(|success| timestamp.duration_since(success)))
                .unwrap_or_default()
        );

        self.stats.successful_reconnections += 1;
        self.state.last_success = Some(timestamp);
        
        // Update average reconnection time
        if let Some(last_success) = self.state.last_success {
            let reconnection_time = timestamp.duration_since(last_success);
            self.update_average_reconnection_time(reconnection_time);
        }

        // Reset attempt count and circuit breaker on success
        self.reset_on_success();
    }

    /// Record failed connection attempt
    fn record_failure(&mut self, error: ConnectionError, retryable: bool) {
        warn!(
            "Reconnection attempt {} failed: {} (retryable: {})",
            self.state.attempt_count, error, retryable
        );

        self.stats.failed_attempts += 1;
        self.stats.current_consecutive_failures += 1;
        
        if self.stats.current_consecutive_failures > self.stats.max_consecutive_failures {
            self.stats.max_consecutive_failures = self.stats.current_consecutive_failures;
        }

        // Update circuit breaker state
        self.update_circuit_breaker_on_failure();

        if !retryable {
            // For permanent failures, give up immediately
            self.state.active = false;
        }
    }

    /// Reset state after successful connection
    pub fn reset_on_success(&mut self) {
        self.state.attempt_count = 0;
        self.state.current_delay = self.config.initial_delay;
        self.state.active = false;
        self.stats.current_consecutive_failures = 0;
        self.stats.last_reset = Some(SystemTime::now());
        self.circuit_breaker = CircuitBreakerState::Closed;

        debug!("Backoff strategy reset after successful connection");
    }

    /// Force reset of backoff state (e.g., configuration change)
    pub fn force_reset(&mut self) {
        *self = Self::new(self.config.clone());
        info!("Backoff strategy force reset");
    }

    /// Calculate delay for next attempt with jitter
    fn calculate_delay(&self) -> Duration {
        let mut delay = self.config.initial_delay;
        
        // Apply exponential backoff
        for _ in 0..self.state.attempt_count {
            delay = Duration::from_nanos(
                (delay.as_nanos() as f64 * self.config.multiplier) as u64
            );
            
            // Cap at maximum delay
            if delay > self.config.max_delay {
                delay = self.config.max_delay;
                break;
            }
        }

        // Apply jitter if enabled
        if self.config.use_jitter && self.config.jitter_factor > 0.0 {
            delay = self.apply_jitter(delay);
        }

        delay
    }

    /// Apply jitter to delay to prevent thundering herd
    fn apply_jitter(&self, base_delay: Duration) -> Duration {
        use rand::Rng;
        
        let jitter_amount = (base_delay.as_nanos() as f64 * self.config.jitter_factor) as u64;
        let mut rng = rand::thread_rng();
        
        // Generate random jitter between -jitter_amount and +jitter_amount
        let jitter: i64 = rng.gen_range(-(jitter_amount as i64)..=(jitter_amount as i64));
        
        let final_delay = if jitter < 0 {
            base_delay.saturating_sub(Duration::from_nanos((-jitter) as u64))
        } else {
            base_delay.saturating_add(Duration::from_nanos(jitter as u64))
        };

        // Ensure minimum delay of 100ms to prevent too aggressive retries
        final_delay.max(Duration::from_millis(100))
    }

    /// Check circuit breaker state and make decision
    fn check_circuit_breaker(&mut self, now: Instant) -> Option<BackoffDecision> {
        if !self.config.circuit_breaker.enabled {
            return None;
        }

        match &mut self.circuit_breaker {
            CircuitBreakerState::Closed => {
                // Check if we should trip the circuit breaker
                if self.stats.current_consecutive_failures >= self.config.circuit_breaker.failure_threshold {
                    self.circuit_breaker = CircuitBreakerState::Open { opened_at: now };
                    warn!("Circuit breaker opened after {} consecutive failures", self.stats.current_consecutive_failures);
                    
                    return Some(BackoffDecision::CircuitOpen {
                        recovery_time: self.config.circuit_breaker.recovery_timeout,
                    });
                }
                None
            }
            CircuitBreakerState::Open { opened_at } => {
                // Check if recovery timeout has elapsed
                if now.duration_since(*opened_at) >= self.config.circuit_breaker.recovery_timeout {
                    self.circuit_breaker = CircuitBreakerState::HalfOpen { test_attempts: 0 };
                    info!("Circuit breaker moved to half-open state");
                    None
                } else {
                    let remaining = self.config.circuit_breaker.recovery_timeout
                        .saturating_sub(now.duration_since(*opened_at));
                    Some(BackoffDecision::CircuitOpen {
                        recovery_time: remaining,
                    })
                }
            }
            CircuitBreakerState::HalfOpen { test_attempts } => {
                if *test_attempts < self.config.circuit_breaker.test_attempts {
                    *test_attempts += 1;
                    None
                } else {
                    // Exceeded test attempts, go back to open
                    self.circuit_breaker = CircuitBreakerState::Open { opened_at: now };
                    Some(BackoffDecision::CircuitOpen {
                        recovery_time: self.config.circuit_breaker.recovery_timeout,
                    })
                }
            }
        }
    }

    /// Update circuit breaker state on failure
    fn update_circuit_breaker_on_failure(&mut self) {
        if !self.config.circuit_breaker.enabled {
            return;
        }

        match &mut self.circuit_breaker {
            CircuitBreakerState::HalfOpen { .. } => {
                // Failure in half-open state - go back to open
                self.circuit_breaker = CircuitBreakerState::Open { opened_at: Instant::now() };
                warn!("Circuit breaker reopened due to failure in half-open state");
            }
            _ => {} // Other states handled in check_circuit_breaker
        }
    }

    /// Update average reconnection time statistics
    fn update_average_reconnection_time(&mut self, new_time: Duration) {
        let count = self.stats.successful_reconnections;
        if count <= 1 {
            self.stats.avg_reconnection_time = new_time;
        } else {
            // Calculate running average
            let current_total = self.stats.avg_reconnection_time.as_nanos() * (count - 1) as u128;
            let new_total = current_total + new_time.as_nanos();
            self.stats.avg_reconnection_time = Duration::from_nanos((new_total / count as u128) as u64);
        }
    }

    /// Get current backoff statistics
    pub fn stats(&self) -> &BackoffStats {
        &self.stats
    }

    /// Get current configuration
    pub fn config(&self) -> &BackoffConfig {
        &self.config
    }

    /// Update configuration (resets state)
    pub fn update_config(&mut self, config: BackoffConfig) {
        self.config = config;
        self.force_reset();
    }

    /// Get current attempt count
    pub fn attempt_count(&self) -> u32 {
        self.state.attempt_count
    }

    /// Check if backoff should give up
    pub fn should_give_up(&self) -> bool {
        if let Some(max_attempts) = self.config.max_attempts {
            self.state.attempt_count >= max_attempts
        } else {
            false
        }
    }

    /// Get time until next attempt is allowed
    pub fn time_until_next_attempt(&self) -> Option<Duration> {
        if !self.state.active {
            return None;
        }

        let delay = self.calculate_delay();
        if let Some(last_attempt) = self.state.last_attempt {
            let elapsed = Instant::now().duration_since(last_attempt);
            if elapsed < delay {
                Some(delay - elapsed)
            } else {
                Some(Duration::from_secs(0))
            }
        } else {
            Some(Duration::from_secs(0))
        }
    }

    /// Check if circuit breaker is open
    pub fn is_circuit_open(&self) -> bool {
        matches!(self.circuit_breaker, CircuitBreakerState::Open { .. })
    }

    /// Get circuit breaker state description
    pub fn circuit_breaker_state(&self) -> String {
        match &self.circuit_breaker {
            CircuitBreakerState::Closed => "closed".to_string(),
            CircuitBreakerState::Open { opened_at } => {
                format!("open (opened {:?} ago)", Instant::now().duration_since(*opened_at))
            }
            CircuitBreakerState::HalfOpen { test_attempts } => {
                format!("half-open (test attempts: {})", test_attempts)
            }
        }
    }

    /// Calculate success rate over the configured window
    pub fn calculate_success_rate(&self) -> f64 {
        let total_attempts = self.stats.total_attempts;
        if total_attempts == 0 {
            return 1.0;
        }
        
        let successful = self.stats.successful_reconnections;
        successful as f64 / total_attempts as f64
    }

    /// Check if we should reset attempt count based on success duration
    pub fn check_reset_threshold(&mut self) {
        if let Some(last_success) = self.state.last_success {
            if Instant::now().duration_since(last_success) >= self.config.reset_threshold {
                self.reset_on_success();
                debug!("Reset backoff due to long-running successful connection");
            }
        }
    }
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(300),
            multiplier: 2.0,
            max_attempts: Some(100),
            use_jitter: true,
            jitter_factor: 0.1,
            reset_threshold: Duration::from_secs(60),
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            test_attempts: 3,
            success_rate_threshold: 0.8,
            success_rate_window: Duration::from_secs(300),
        }
    }
}

impl Default for BackoffStats {
    fn default() -> Self {
        Self {
            total_attempts: 0,
            successful_reconnections: 0,
            failed_attempts: 0,
            avg_reconnection_time: Duration::from_secs(0),
            max_consecutive_failures: 0,
            current_consecutive_failures: 0,
            last_reset: None,
            total_backoff_time: Duration::from_secs(0),
        }
    }
}

impl BackoffDecision {
    /// Check if decision allows proceeding with connection attempt
    pub fn should_proceed(&self) -> bool {
        matches!(self, BackoffDecision::Proceed)
    }

    /// Get delay if decision is to wait
    pub fn wait_time(&self) -> Option<Duration> {
        match self {
            BackoffDecision::Wait { delay } => Some(*delay),
            BackoffDecision::CircuitOpen { recovery_time } => Some(*recovery_time),
            _ => None,
        }
    }

    /// Check if decision is to give up
    pub fn should_give_up(&self) -> bool {
        matches!(self, BackoffDecision::GiveUp { .. })
    }
}

impl std::fmt::Display for BackoffDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackoffDecision::Proceed => write!(f, "proceed with attempt"),
            BackoffDecision::Wait { delay } => write!(f, "wait {:?} before next attempt", delay),
            BackoffDecision::GiveUp { reason } => write!(f, "give up: {:?}", reason),
            BackoffDecision::CircuitOpen { recovery_time } => {
                write!(f, "circuit open, recovery in {:?}", recovery_time)
            }
        }
    }
}

impl std::fmt::Display for BackoffGiveUpReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackoffGiveUpReason::MaxAttemptsExceeded { max_attempts } => {
                write!(f, "maximum attempts ({}) exceeded", max_attempts)
            }
            BackoffGiveUpReason::CircuitBreakerPermanent => {
                write!(f, "circuit breaker permanently open")
            }
            BackoffGiveUpReason::ConfigurationRestriction => {
                write!(f, "configuration prevents further attempts")
            }
            BackoffGiveUpReason::ExternalStop => {
                write!(f, "external signal to stop")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    
    #[test]
    fn test_exponential_backoff_basic() {
        let mut backoff = ExponentialBackoff::with_delays(
            Duration::from_millis(100),
            Duration::from_secs(60),
            2.0,
        );

        // First attempt should proceed immediately
        let decision = backoff.next_attempt();
        assert!(decision.should_proceed());

        // Record failure and check next decision requires waiting
        backoff.record_attempt(ReconnectionResult::RetryableFailure {
            error: ConnectionError::ConnectionFailed {
                endpoint: "test".to_string(),
                reason: "test".to_string(),
            },
        });

        let decision = backoff.next_attempt();
        assert!(decision.wait_time().is_some());
    }

    #[test]
    fn test_jitter_applied() {
        let config = BackoffConfig {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_attempts: Some(5),
            use_jitter: true,
            jitter_factor: 0.5,
            reset_threshold: Duration::from_secs(60),
            circuit_breaker: CircuitBreakerConfig::default(),
        };

        let backoff = ExponentialBackoff::new(config);
        let delay1 = backoff.calculate_delay();
        let delay2 = backoff.calculate_delay();
        
        // With jitter, delays should potentially be different
        // (though they might be the same due to randomness)
        assert!(delay1 >= Duration::from_millis(500)); // At least 50% of base with jitter
        assert!(delay2 >= Duration::from_millis(500));
    }

    #[test]
    fn test_circuit_breaker() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_attempts: None,
            use_jitter: false,
            jitter_factor: 0.0,
            reset_threshold: Duration::from_secs(60),
            circuit_breaker: CircuitBreakerConfig {
                enabled: true,
                failure_threshold: 3,
                recovery_timeout: Duration::from_secs(5),
                test_attempts: 1,
                success_rate_threshold: 0.8,
                success_rate_window: Duration::from_secs(300),
            },
        };

        let mut backoff = ExponentialBackoff::new(config);

        // Record enough failures to trip circuit breaker
        for _ in 0..3 {
            backoff.next_attempt();
            backoff.record_attempt(ReconnectionResult::RetryableFailure {
                error: ConnectionError::ConnectionFailed {
                    endpoint: "test".to_string(),
                    reason: "test".to_string(),
                },
            });
        }

        // Circuit should be open now
        assert!(backoff.is_circuit_open());
        let decision = backoff.next_attempt();
        assert!(matches!(decision, BackoffDecision::CircuitOpen { .. }));
    }

    #[test]
    fn test_success_resets_backoff() {
        let mut backoff = ExponentialBackoff::with_delays(
            Duration::from_millis(100),
            Duration::from_secs(60),
            2.0,
        );

        // Make several failed attempts
        for _ in 0..3 {
            backoff.next_attempt();
            backoff.record_attempt(ReconnectionResult::RetryableFailure {
                error: ConnectionError::ConnectionFailed {
                    endpoint: "test".to_string(),
                    reason: "test".to_string(),
                },
            });
        }

        let attempt_count_before = backoff.attempt_count();
        assert!(attempt_count_before > 0);

        // Record success
        backoff.record_attempt(ReconnectionResult::Success);

        // Attempt count should be reset
        assert_eq!(backoff.attempt_count(), 0);
        assert_eq!(backoff.stats().current_consecutive_failures, 0);
    }

    #[test]
    fn test_max_attempts() {
        let config = BackoffConfig {
            max_attempts: Some(3),
            ..Default::default()
        };

        let mut backoff = ExponentialBackoff::new(config);

        // Make max attempts
        for _ in 0..3 {
            let decision = backoff.next_attempt();
            if decision.should_proceed() {
                backoff.record_attempt(ReconnectionResult::RetryableFailure {
                    error: ConnectionError::ConnectionFailed {
                        endpoint: "test".to_string(),
                        reason: "test".to_string(),
                    },
                });
            }
        }

        // Next attempt should give up
        let decision = backoff.next_attempt();
        assert!(decision.should_give_up());
    }

    #[tokio::test]
    async fn test_backoff_timing() {
        let mut backoff = ExponentialBackoff::with_delays(
            Duration::from_millis(50),
            Duration::from_secs(1),
            2.0,
        );

        // First attempt should proceed
        assert!(backoff.next_attempt().should_proceed());

        // Record failure
        backoff.record_attempt(ReconnectionResult::RetryableFailure {
            error: ConnectionError::ConnectionFailed {
                endpoint: "test".to_string(),
                reason: "test".to_string(),
            },
        });

        // Should need to wait
        let decision = backoff.next_attempt();
        assert!(decision.wait_time().is_some());

        // Wait and then should be able to proceed
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(backoff.next_attempt().should_proceed());
    }
}