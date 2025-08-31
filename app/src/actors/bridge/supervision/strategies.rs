//! Supervision Strategies
//! 
//! Different strategies for actor supervision and recovery

use std::time::{Duration, SystemTime};
use tracing::{info, warn};

/// Supervision strategy implementation
pub trait SupervisionStrategy {
    fn should_restart(&self, failure_count: u32, last_failure: SystemTime) -> bool;
    fn get_restart_delay(&self, failure_count: u32) -> Duration;
    fn reset(&mut self);
}

/// Immediate restart strategy
#[derive(Debug, Clone)]
pub struct ImmediateRestartStrategy;

impl SupervisionStrategy for ImmediateRestartStrategy {
    fn should_restart(&self, _failure_count: u32, _last_failure: SystemTime) -> bool {
        true
    }

    fn get_restart_delay(&self, _failure_count: u32) -> Duration {
        Duration::from_secs(0)
    }

    fn reset(&mut self) {}
}

/// Exponential backoff strategy
#[derive(Debug, Clone)]
pub struct ExponentialBackoffStrategy {
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub current_delay: Duration,
}

impl SupervisionStrategy for ExponentialBackoffStrategy {
    fn should_restart(&self, failure_count: u32, _last_failure: SystemTime) -> bool {
        failure_count < 10 // Max 10 restart attempts
    }

    fn get_restart_delay(&self, failure_count: u32) -> Duration {
        let delay = self.base_delay * 2_u32.pow(failure_count.min(8));
        delay.min(self.max_delay)
    }

    fn reset(&mut self) {
        self.current_delay = self.base_delay;
    }
}

/// Circuit breaker strategy
#[derive(Debug, Clone)]
pub struct CircuitBreakerStrategy {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub current_failures: u32,
    pub last_failure: Option<SystemTime>,
}

impl SupervisionStrategy for CircuitBreakerStrategy {
    fn should_restart(&self, failure_count: u32, last_failure: SystemTime) -> bool {
        if failure_count >= self.failure_threshold {
            // Check if recovery timeout has passed
            SystemTime::now().duration_since(last_failure).unwrap_or_default() >= self.recovery_timeout
        } else {
            true
        }
    }

    fn get_restart_delay(&self, failure_count: u32) -> Duration {
        if failure_count >= self.failure_threshold {
            self.recovery_timeout
        } else {
            Duration::from_secs(1)
        }
    }

    fn reset(&mut self) {
        self.current_failures = 0;
        self.last_failure = None;
    }
}