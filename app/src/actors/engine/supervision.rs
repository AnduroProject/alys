//! Supervision and Fault Tolerance Implementation
//!
//! Provides supervision strategies, error recovery mechanisms, and fault tolerance
//! for the EngineActor to ensure high availability and resilience.

use std::time::{Duration, Instant, SystemTime};
use tracing::*;
use actix::prelude::*;

use crate::types::*;
use super::{
    actor::EngineActor,
    messages::*,
    state::ExecutionState,
    config::RestartStrategy,
    EngineError, EngineResult,
};

/// Supervision configuration for the EngineActor
#[derive(Debug, Clone)]
pub struct SupervisionConfig {
    /// Maximum number of restart attempts before giving up
    pub max_restart_attempts: u32,
    
    /// Base backoff time for exponential backoff
    pub base_backoff: Duration,
    
    /// Maximum backoff time
    pub max_backoff: Duration,
    
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    
    /// Restart window - resets restart count after this duration
    pub restart_window: Duration,
    
    /// Health check interval during degraded state
    pub degraded_health_check_interval: Duration,
    
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
}

/// Circuit breaker configuration for fault tolerance
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to trip the circuit breaker
    pub failure_threshold: u32,
    
    /// Success threshold to close the circuit breaker
    pub success_threshold: u32,
    
    /// Circuit breaker timeout before trying again
    pub timeout: Duration,
    
    /// Rolling window size for tracking failures
    pub rolling_window: Duration,
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    /// Normal operation
    Closed,
    
    /// Circuit is open, rejecting requests
    Open { opened_at: SystemTime },
    
    /// Circuit is half-open, testing recovery
    HalfOpen { test_started: SystemTime },
}

/// Supervision tracker for the EngineActor
#[derive(Debug)]
pub struct SupervisionTracker {
    /// Current supervision configuration
    pub config: SupervisionConfig,
    
    /// Number of restart attempts in current window
    pub restart_attempts: u32,
    
    /// When the current restart window started
    pub restart_window_start: SystemTime,
    
    /// Last restart timestamp
    pub last_restart: Option<SystemTime>,
    
    /// Circuit breaker state
    pub circuit_breaker: CircuitBreakerState,
    
    /// Recent failure history for circuit breaker
    pub failure_history: Vec<Instant>,
    
    /// Recent success count for half-open state
    pub recent_successes: u32,
    
    /// Degraded state start time
    pub degraded_since: Option<SystemTime>,
}

/// Supervision directive returned by supervisor
#[derive(Debug, Clone)]
pub enum SupervisionDirective {
    /// Resume normal operation
    Resume,
    
    /// Restart the actor
    Restart { delay: Option<Duration> },
    
    /// Stop the actor permanently
    Stop { reason: String },
    
    /// Enter degraded mode
    Degrade { reason: String },
    
    /// Escalate to parent supervisor
    Escalate { reason: String },
}

/// Message to report failures to the supervision system
#[derive(Message, Debug, Clone)]
#[rtype(result = "SupervisionDirective")]
pub struct FailureReportMessage {
    /// Type of failure that occurred
    pub failure_type: FailureType,
    
    /// Detailed error information
    pub error: EngineError,
    
    /// Context of when the failure occurred
    pub context: String,
    
    /// Whether this failure is recoverable
    pub recoverable: bool,
    
    /// Timestamp of the failure
    pub timestamp: SystemTime,
}

/// Types of failures that can be supervised
#[derive(Debug, Clone, PartialEq)]
pub enum FailureType {
    /// Connection failure to execution client
    ConnectionFailure,
    
    /// Timeout in operation
    Timeout,
    
    /// Invalid response from client
    InvalidResponse,
    
    /// Consensus failure
    ConsensusFailure,
    
    /// Resource exhaustion
    ResourceExhaustion,
    
    /// Configuration error
    ConfigError,
    
    /// Actor system error
    ActorSystemError,
    
    /// Unknown error
    Unknown,
}

impl Default for SupervisionConfig {
    fn default() -> Self {
        Self {
            max_restart_attempts: 5,
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            restart_window: Duration::from_minutes(10),
            degraded_health_check_interval: Duration::from_secs(30),
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            rolling_window: Duration::from_minutes(5),
        }
    }
}

impl SupervisionTracker {
    /// Create a new supervision tracker
    pub fn new(config: SupervisionConfig) -> Self {
        Self {
            config,
            restart_attempts: 0,
            restart_window_start: SystemTime::now(),
            last_restart: None,
            circuit_breaker: CircuitBreakerState::Closed,
            failure_history: Vec::new(),
            recent_successes: 0,
            degraded_since: None,
        }
    }
    
    /// Report a failure and get supervision directive
    pub fn report_failure(&mut self, failure: &FailureReportMessage) -> SupervisionDirective {
        info!(
            failure_type = ?failure.failure_type,
            error = %failure.error,
            context = %failure.context,
            recoverable = %failure.recoverable,
            "Reporting failure to supervision system"
        );
        
        // Record failure for circuit breaker
        self.record_failure();
        
        // Check circuit breaker state
        if let Some(directive) = self.check_circuit_breaker() {
            return directive;
        }
        
        // Handle non-recoverable failures
        if !failure.recoverable {
            return match failure.failure_type {
                FailureType::ConfigError => SupervisionDirective::Stop {
                    reason: "Non-recoverable configuration error".to_string(),
                },
                FailureType::ActorSystemError => SupervisionDirective::Escalate {
                    reason: "Actor system failure requires escalation".to_string(),
                },
                _ => SupervisionDirective::Restart { delay: None },
            };
        }
        
        // Check restart window and reset if needed
        self.check_restart_window();
        
        // Determine supervision action based on failure type and history
        match failure.failure_type {
            FailureType::ConnectionFailure | FailureType::Timeout => {
                self.handle_transient_failure()
            },
            FailureType::InvalidResponse => {
                if self.restart_attempts < 2 {
                    self.handle_transient_failure()
                } else {
                    SupervisionDirective::Degrade {
                        reason: "Multiple invalid responses, entering degraded mode".to_string(),
                    }
                }
            },
            FailureType::ConsensusFailure => {
                SupervisionDirective::Escalate {
                    reason: "Consensus failure requires parent intervention".to_string(),
                }
            },
            FailureType::ResourceExhaustion => {
                SupervisionDirective::Degrade {
                    reason: "Resource exhaustion, reducing load".to_string(),
                }
            },
            _ => self.handle_transient_failure(),
        }
    }
    
    /// Report a successful operation
    pub fn report_success(&mut self) {
        // Clear recent failures for circuit breaker
        let now = Instant::now();
        let window_start = now - self.config.circuit_breaker.rolling_window;
        self.failure_history.retain(|&timestamp| timestamp > window_start);
        
        // Handle half-open circuit breaker
        if matches!(self.circuit_breaker, CircuitBreakerState::HalfOpen { .. }) {
            self.recent_successes += 1;
            
            if self.recent_successes >= self.config.circuit_breaker.success_threshold {
                info!("Circuit breaker closing due to successful operations");
                self.circuit_breaker = CircuitBreakerState::Closed;
                self.recent_successes = 0;
            }
        }
        
        // Clear degraded state if we've been successful
        if self.degraded_since.is_some() {
            self.degraded_since = None;
            debug!("Clearing degraded state due to successful operation");
        }
    }
    
    /// Check if operations should be allowed based on circuit breaker
    pub fn should_allow_operation(&mut self) -> bool {
        match &self.circuit_breaker {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open { opened_at } => {
                // Check if timeout has elapsed
                if opened_at.elapsed().unwrap_or(Duration::ZERO) > self.config.circuit_breaker.timeout {
                    info!("Circuit breaker transitioning to half-open");
                    self.circuit_breaker = CircuitBreakerState::HalfOpen {
                        test_started: SystemTime::now(),
                    };
                    self.recent_successes = 0;
                    true
                } else {
                    false
                }
            },
            CircuitBreakerState::HalfOpen { .. } => true,
        }
    }
    
    /// Get current supervision status
    pub fn get_status(&self) -> SupervisionStatus {
        SupervisionStatus {
            restart_attempts: self.restart_attempts,
            circuit_breaker_state: self.circuit_breaker.clone(),
            degraded_since: self.degraded_since,
            failure_count: self.failure_history.len() as u32,
            last_restart: self.last_restart,
        }
    }
    
    /// Handle transient failures with restart logic
    fn handle_transient_failure(&mut self) -> SupervisionDirective {
        if self.restart_attempts >= self.config.max_restart_attempts {
            warn!(
                max_attempts = %self.config.max_restart_attempts,
                "Maximum restart attempts reached"
            );
            
            SupervisionDirective::Degrade {
                reason: "Maximum restart attempts exceeded".to_string(),
            }
        } else {
            self.restart_attempts += 1;
            self.last_restart = Some(SystemTime::now());
            
            let delay = self.calculate_backoff_delay();
            
            info!(
                attempt = %self.restart_attempts,
                delay_ms = %delay.as_millis(),
                "Scheduling restart with backoff"
            );
            
            SupervisionDirective::Restart { delay: Some(delay) }
        }
    }
    
    /// Calculate exponential backoff delay
    fn calculate_backoff_delay(&self) -> Duration {
        let base_delay = self.config.base_backoff.as_millis() as f64;
        let multiplier = self.config.backoff_multiplier;
        let attempt = (self.restart_attempts - 1) as f64;
        
        let delay_ms = base_delay * multiplier.powf(attempt);
        let delay = Duration::from_millis(delay_ms as u64);
        
        std::cmp::min(delay, self.config.max_backoff)
    }
    
    /// Record a failure for circuit breaker tracking
    fn record_failure(&mut self) {
        let now = Instant::now();
        self.failure_history.push(now);
        
        // Clean up old failures outside the rolling window
        let window_start = now - self.config.circuit_breaker.rolling_window;
        self.failure_history.retain(|&timestamp| timestamp > window_start);
    }
    
    /// Check circuit breaker state and potentially trip it
    fn check_circuit_breaker(&mut self) -> Option<SupervisionDirective> {
        let failure_count = self.failure_history.len() as u32;
        
        match self.circuit_breaker {
            CircuitBreakerState::Closed => {
                if failure_count >= self.config.circuit_breaker.failure_threshold {
                    warn!(
                        failure_count = %failure_count,
                        threshold = %self.config.circuit_breaker.failure_threshold,
                        "Circuit breaker opening due to failure threshold"
                    );
                    
                    self.circuit_breaker = CircuitBreakerState::Open {
                        opened_at: SystemTime::now(),
                    };
                    
                    Some(SupervisionDirective::Degrade {
                        reason: "Circuit breaker opened due to failures".to_string(),
                    })
                } else {
                    None
                }
            },
            _ => None,
        }
    }
    
    /// Check if restart window has elapsed and reset counters
    fn check_restart_window(&mut self) {
        let window_elapsed = self.restart_window_start
            .elapsed()
            .unwrap_or(Duration::ZERO);
            
        if window_elapsed > self.config.restart_window {
            debug!(
                previous_attempts = %self.restart_attempts,
                "Restart window elapsed, resetting counters"
            );
            
            self.restart_attempts = 0;
            self.restart_window_start = SystemTime::now();
        }
    }
}

/// Current supervision status
#[derive(Debug, Clone)]
pub struct SupervisionStatus {
    /// Number of restart attempts in current window
    pub restart_attempts: u32,
    
    /// Current circuit breaker state
    pub circuit_breaker_state: CircuitBreakerState,
    
    /// When degraded mode started (if applicable)
    pub degraded_since: Option<SystemTime>,
    
    /// Number of recent failures
    pub failure_count: u32,
    
    /// Last restart timestamp
    pub last_restart: Option<SystemTime>,
}

/// Handler for failure reports
impl Handler<FailureReportMessage> for EngineActor {
    type Result = MessageResult<SupervisionDirective>;
    
    fn handle(&mut self, msg: FailureReportMessage, ctx: &mut Self::Context) -> Self::Result {
        let directive = self.supervision.report_failure(&msg);
        
        debug!(
            failure_type = ?msg.failure_type,
            directive = ?directive,
            "Received supervision directive"
        );
        
        // Execute the supervision directive
        match &directive {
            SupervisionDirective::Resume => {
                // Continue normal operation
                debug!("Supervision directive: Resume normal operation");
            },
            SupervisionDirective::Restart { delay } => {
                let delay = delay.unwrap_or(Duration::from_millis(100));
                
                warn!(
                    delay_ms = %delay.as_millis(),
                    "Supervision directive: Restart actor"
                );
                
                // Schedule restart after delay
                ctx.run_later(delay, |actor, ctx| {
                    // Send restart message to self
                    let restart_msg = RestartEngineMessage {
                        reason: "Supervision restart".to_string(),
                        preserve_state: true,
                    };
                    
                    ctx.address().send(restart_msg);
                });
            },
            SupervisionDirective::Stop { reason } => {
                error!(reason = %reason, "Supervision directive: Stop actor");
                
                // Transition to error state
                self.state.transition_state(
                    ExecutionState::Error {
                        message: reason.clone(),
                        occurred_at: SystemTime::now(),
                        recoverable: false,
                        recovery_attempts: 0,
                    },
                    "Supervision stop directive".to_string()
                );
                
                ctx.stop();
            },
            SupervisionDirective::Degrade { reason } => {
                warn!(reason = %reason, "Supervision directive: Enter degraded mode");
                
                self.supervision.degraded_since = Some(SystemTime::now());
                
                // Transition to degraded state
                self.state.transition_state(
                    ExecutionState::Degraded {
                        reason: reason.clone(),
                        since: SystemTime::now(),
                        limited_operations: true,
                    },
                    "Supervision degraded directive".to_string()
                );
                
                // Start degraded mode health checks
                self.start_degraded_health_checks(ctx);
            },
            SupervisionDirective::Escalate { reason } => {
                error!(reason = %reason, "Supervision directive: Escalate to parent");
                
                // TODO: Implement escalation to parent supervisor
                // This would typically involve sending a message to a parent actor
                // or to the actor system supervisor
                
                // For now, log the escalation
                self.metrics.supervision_escalated();
            }
        }
        
        Ok(directive)
    }
}

impl EngineActor {
    /// Start degraded mode health checks
    fn start_degraded_health_checks(&mut self, ctx: &mut Context<Self>) {
        let interval = self.supervision.config.degraded_health_check_interval;
        
        info!(
            interval_ms = %interval.as_millis(),
            "Starting degraded mode health checks"
        );
        
        // Cancel existing health check interval
        if let Some(handle) = &self.health_check_interval {
            ctx.cancel_future(*handle);
        }
        
        // Start new health check interval for degraded mode
        self.health_check_interval = Some(ctx.run_interval(interval, |actor, _ctx| {
            // Perform health check in degraded mode
            let health_msg = HealthCheckMessage;
            // TODO: Send health check message to self
        }));
    }
    
    /// Report a failure to the supervision system
    pub fn report_failure(&mut self, failure_type: FailureType, error: EngineError, context: String) {
        let failure_report = FailureReportMessage {
            failure_type,
            error,
            context,
            recoverable: true, // Most failures are recoverable by default
            timestamp: SystemTime::now(),
        };
        
        // Handle the failure report directly
        let directive = self.supervision.report_failure(&failure_report);
        
        // Update metrics
        self.metrics.failure_reported(failure_type.clone());
        
        debug!(
            failure_type = ?failure_type,
            directive = ?directive,
            "Failure reported to supervision system"
        );
    }
    
    /// Get current supervision status
    pub fn get_supervision_status(&self) -> SupervisionStatus {
        self.supervision.get_status()
    }
    
    /// Check if operations should be allowed based on supervision state
    pub fn should_allow_operation(&mut self) -> bool {
        self.supervision.should_allow_operation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_supervision_tracker_creation() {
        let config = SupervisionConfig::default();
        let tracker = SupervisionTracker::new(config);
        
        assert_eq!(tracker.restart_attempts, 0);
        assert_eq!(tracker.circuit_breaker, CircuitBreakerState::Closed);
        assert!(tracker.failure_history.is_empty());
    }
    
    #[test]
    fn test_exponential_backoff() {
        let config = SupervisionConfig {
            base_backoff: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(10),
            ..Default::default()
        };
        
        let mut tracker = SupervisionTracker::new(config);
        
        // Simulate failures to test backoff
        for attempt in 1..=5 {
            tracker.restart_attempts = attempt;
            let delay = tracker.calculate_backoff_delay();
            
            let expected_ms = 100 * (2_u64.pow(attempt - 1));
            let expected = Duration::from_millis(expected_ms);
            
            assert_eq!(delay, expected.min(tracker.config.max_backoff));
        }
    }
    
    #[test]
    fn test_circuit_breaker_lifecycle() {
        let config = SupervisionConfig {
            circuit_breaker: CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout: Duration::from_secs(1),
                rolling_window: Duration::from_minutes(1),
            },
            ..Default::default()
        };
        
        let mut tracker = SupervisionTracker::new(config);
        
        // Circuit breaker starts closed
        assert_eq!(tracker.circuit_breaker, CircuitBreakerState::Closed);
        assert!(tracker.should_allow_operation());
        
        // Record failures to trip circuit breaker
        for _ in 0..3 {
            tracker.record_failure();
        }
        
        let failure_msg = FailureReportMessage {
            failure_type: FailureType::ConnectionFailure,
            error: EngineError::ClientError(crate::actors::engine::ClientError::ConnectionFailed("test".to_string())),
            context: "test".to_string(),
            recoverable: true,
            timestamp: SystemTime::now(),
        };
        
        let directive = tracker.report_failure(&failure_msg);
        
        // Should trip circuit breaker
        match directive {
            SupervisionDirective::Degrade { .. } => {
                assert!(matches!(tracker.circuit_breaker, CircuitBreakerState::Open { .. }));
            },
            _ => panic!("Expected degrade directive"),
        }
    }
}