//! Advanced Connection Management and Reconnection
//! 
//! Sophisticated reconnection system with exponential backoff, jitter, circuit breaker patterns,
//! and advanced failure detection for bridge governance connections.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use serde::{Deserialize, Serialize};
use tracing::*;

use crate::actors::bridge::shared::errors::BridgeError;

/// Advanced reconnection manager for governance connections with circuit breaker
#[derive(Debug)]
pub struct ReconnectionManager {
    /// Per-node reconnection strategies
    strategies: HashMap<String, ExponentialBackoff>,
    /// Global configuration
    global_config: BackoffConfig,
    /// Connection health monitor
    health_monitor: ConnectionHealthMonitor,
}

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

/// Connection health monitor for proactive failure detection
#[derive(Debug, Clone)]
pub struct ConnectionHealthMonitor {
    /// Health check configuration
    config: HealthCheckConfig,
    /// Per-node health metrics
    node_health: HashMap<String, NodeHealthMetrics>,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Enable health monitoring
    pub enabled: bool,
    /// Interval between health checks
    pub check_interval: Duration,
    /// Timeout for individual health checks
    pub check_timeout: Duration,
    /// Number of failed health checks before marking unhealthy
    pub failure_threshold: u32,
    /// Latency threshold for degraded health
    pub latency_threshold: Duration,
}

/// Health metrics for individual nodes
#[derive(Debug, Clone)]
pub struct NodeHealthMetrics {
    /// Current health status
    pub status: NodeHealthStatus,
    /// Consecutive failed health checks
    pub consecutive_failures: u32,
    /// Last successful health check
    pub last_success: Option<Instant>,
    /// Recent latency measurements
    pub latency_history: Vec<Duration>,
    /// Health check success rate (0.0 to 1.0)
    pub success_rate: f64,
}

/// Node health status
#[derive(Debug, Clone, PartialEq)]
pub enum NodeHealthStatus {
    /// Node is healthy and responsive
    Healthy,
    /// Node is experiencing degraded performance
    Degraded,
    /// Node is unhealthy or unresponsive
    Unhealthy,
    /// Health status unknown (insufficient data)
    Unknown,
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
    RetryableFailure { error: BridgeError },
    /// Connection failed with permanent error
    PermanentFailure { error: BridgeError },
    /// Connection cancelled
    Cancelled,
}

impl ReconnectionManager {
    /// Create new advanced reconnection manager
    pub fn new(max_attempts: u32, base_delay: Duration) -> Self {
        let global_config = BackoffConfig {
            initial_delay: base_delay,
            max_attempts: Some(max_attempts),
            ..Default::default()
        };
        
        Self {
            strategies: HashMap::new(),
            global_config,
            health_monitor: ConnectionHealthMonitor::new(HealthCheckConfig::default()),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: BackoffConfig) -> Self {
        Self {
            strategies: HashMap::new(),
            global_config: config,
            health_monitor: ConnectionHealthMonitor::new(HealthCheckConfig::default()),
        }
    }

    /// Get or create backoff strategy for node
    fn get_strategy(&mut self, node_id: &str) -> &mut ExponentialBackoff {
        self.strategies.entry(node_id.to_string())
            .or_insert_with(|| ExponentialBackoff::new(self.global_config.clone()))
    }

    /// Record connection failure
    pub fn record_failure(&mut self, node_id: String, error: BridgeError) {
        let is_retryable = match &error {
            BridgeError::ConnectionError(_) => true,
            BridgeError::NetworkError(_) => true,
            BridgeError::AuthenticationError(_) => false, // Usually not retryable
            _ => true,
        };

        let result = if is_retryable {
            ReconnectionResult::RetryableFailure { error }
        } else {
            ReconnectionResult::PermanentFailure { error }
        };

        // Record attempt and get next decision, limiting the scope of mutable borrow
        let (next_decision, attempt_count, circuit_breaker_state) = {
            let strategy = self.get_strategy(&node_id);
            strategy.record_attempt(result);
            let decision = strategy.next_attempt();
            let count = strategy.attempt_count();
            let cb_state = strategy.circuit_breaker_state();
            (decision, count, cb_state)
        };

        // Update health monitor (now we can borrow health_monitor mutably)
        self.health_monitor.record_failure(&node_id);

        warn!(
            "Connection failure for {}: attempt {}, circuit breaker: {}, next decision: {}",
            node_id,
            attempt_count,
            circuit_breaker_state,
            next_decision
        );
    }

    /// Check if reconnection should be attempted
    pub fn should_reconnect(&mut self, node_id: &str) -> BackoffDecision {
        let strategy = self.get_strategy(node_id);
        let decision = strategy.next_attempt();
        
        // Consider health monitor input
        if let Some(health) = self.health_monitor.node_health.get(node_id) {
            if matches!(health.status, NodeHealthStatus::Unhealthy) && 
               health.consecutive_failures > 10 {
                // Override with permanent failure if health is consistently bad
                return BackoffDecision::GiveUp { 
                    reason: BackoffGiveUpReason::ConfigurationRestriction 
                };
            }
        }

        decision
    }

    /// Record successful connection
    pub fn record_success(&mut self, node_id: String) {
        if let Some(strategy) = self.strategies.get_mut(&node_id) {
            strategy.record_attempt(ReconnectionResult::Success);
            info!(
                "Successful reconnection to {} after {} attempts",
                node_id,
                strategy.attempt_count()
            );
        }

        // Update health monitor
        self.health_monitor.record_success(&node_id);
    }

    /// Get reconnection statistics for a node
    pub fn get_stats(&self, node_id: &str) -> Option<&BackoffStats> {
        self.strategies.get(node_id).map(|s| s.stats())
    }

    /// Get overall reconnection statistics
    pub fn get_overall_stats(&self) -> BackoffStats {
        let mut overall = BackoffStats::default();
        
        for strategy in self.strategies.values() {
            let stats = strategy.stats();
            overall.total_attempts += stats.total_attempts;
            overall.successful_reconnections += stats.successful_reconnections;
            overall.failed_attempts += stats.failed_attempts;
            overall.max_consecutive_failures = overall.max_consecutive_failures
                .max(stats.max_consecutive_failures);
            overall.total_backoff_time += stats.total_backoff_time;
        }

        overall
    }

    /// Update global configuration
    pub fn update_config(&mut self, config: BackoffConfig) {
        self.global_config = config.clone();
        // Apply new config to existing strategies
        for strategy in self.strategies.values_mut() {
            strategy.update_config(config.clone());
        }
    }

    /// Check and update reset thresholds for all strategies
    pub fn check_reset_thresholds(&mut self) {
        for strategy in self.strategies.values_mut() {
            strategy.check_reset_threshold();
        }
    }

    /// Get health status for a node
    pub fn get_node_health(&self, node_id: &str) -> NodeHealthStatus {
        self.health_monitor.get_node_health(node_id)
    }

    /// Perform health check on all nodes
    pub async fn perform_health_checks(&mut self) -> HashMap<String, NodeHealthStatus> {
        self.health_monitor.check_all_nodes().await
    }

    /// Force reset reconnection state for a node
    pub fn force_reset(&mut self, node_id: &str) {
        if let Some(strategy) = self.strategies.get_mut(node_id) {
            strategy.force_reset();
        }
        self.health_monitor.reset_node_health(node_id);
        info!("Force reset reconnection state for {}", node_id);
    }
}

impl ExponentialBackoff {
    /// Create new exponential backoff strategy
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
            ReconnectionResult::RetryableFailure { error: _ } => {
                self.record_failure(true);
            }
            ReconnectionResult::PermanentFailure { error: _ } => {
                self.record_failure(false);
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
        self.stats.successful_reconnections += 1;
        self.state.last_success = Some(timestamp);
        self.reset_on_success();
    }

    /// Record failed connection attempt
    fn record_failure(&mut self, retryable: bool) {
        self.stats.failed_attempts += 1;
        self.stats.current_consecutive_failures += 1;
        
        if self.stats.current_consecutive_failures > self.stats.max_consecutive_failures {
            self.stats.max_consecutive_failures = self.stats.current_consecutive_failures;
        }

        // Update circuit breaker state
        self.update_circuit_breaker_on_failure();

        if !retryable {
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
    }

    /// Calculate delay with exponential backoff and jitter
    fn calculate_delay(&self) -> Duration {
        let mut delay = self.config.initial_delay;
        
        // Apply exponential backoff
        for _ in 0..self.state.attempt_count {
            delay = Duration::from_nanos(
                (delay.as_nanos() as f64 * self.config.multiplier) as u64
            );
            
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

    /// Apply jitter to prevent thundering herd
    fn apply_jitter(&self, base_delay: Duration) -> Duration {
        use rand::Rng;
        
        let jitter_amount = (base_delay.as_nanos() as f64 * self.config.jitter_factor) as u64;
        let mut rng = rand::thread_rng();
        
        let jitter: i64 = rng.gen_range(-(jitter_amount as i64)..=(jitter_amount as i64));
        
        let final_delay = if jitter < 0 {
            base_delay.saturating_sub(Duration::from_nanos((-jitter) as u64))
        } else {
            base_delay.saturating_add(Duration::from_nanos(jitter as u64))
        };

        final_delay.max(Duration::from_millis(100))
    }

    /// Check circuit breaker state
    fn check_circuit_breaker(&mut self, now: Instant) -> Option<BackoffDecision> {
        if !self.config.circuit_breaker.enabled {
            return None;
        }

        match &mut self.circuit_breaker {
            CircuitBreakerState::Closed => {
                if self.stats.current_consecutive_failures >= self.config.circuit_breaker.failure_threshold {
                    self.circuit_breaker = CircuitBreakerState::Open { opened_at: now };
                    warn!("Circuit breaker opened after {} consecutive failures", 
                         self.stats.current_consecutive_failures);
                    
                    return Some(BackoffDecision::CircuitOpen {
                        recovery_time: self.config.circuit_breaker.recovery_timeout,
                    });
                }
                None
            }
            CircuitBreakerState::Open { opened_at } => {
                if now.duration_since(*opened_at) >= self.config.circuit_breaker.recovery_timeout {
                    self.circuit_breaker = CircuitBreakerState::HalfOpen { test_attempts: 0 };
                    info!("Circuit breaker moved to half-open state");
                    None
                } else {
                    let remaining = self.config.circuit_breaker.recovery_timeout
                        .saturating_sub(now.duration_since(*opened_at));
                    Some(BackoffDecision::CircuitOpen { recovery_time: remaining })
                }
            }
            CircuitBreakerState::HalfOpen { test_attempts } => {
                if *test_attempts < self.config.circuit_breaker.test_attempts {
                    *test_attempts += 1;
                    None
                } else {
                    self.circuit_breaker = CircuitBreakerState::Open { opened_at: now };
                    Some(BackoffDecision::CircuitOpen {
                        recovery_time: self.config.circuit_breaker.recovery_timeout,
                    })
                }
            }
        }
    }

    /// Update circuit breaker on failure
    fn update_circuit_breaker_on_failure(&mut self) {
        if let CircuitBreakerState::HalfOpen { .. } = &mut self.circuit_breaker {
            self.circuit_breaker = CircuitBreakerState::Open { opened_at: Instant::now() };
            warn!("Circuit breaker reopened due to failure in half-open state");
        }
    }

    pub fn stats(&self) -> &BackoffStats {
        &self.stats
    }

    pub fn attempt_count(&self) -> u32 {
        self.state.attempt_count
    }

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

    pub fn check_reset_threshold(&mut self) {
        if let Some(last_success) = self.state.last_success {
            if Instant::now().duration_since(last_success) >= self.config.reset_threshold {
                self.reset_on_success();
                debug!("Reset backoff due to long-running successful connection");
            }
        }
    }

    pub fn force_reset(&mut self) {
        *self = Self::new(self.config.clone());
    }

    pub fn update_config(&mut self, config: BackoffConfig) {
        self.config = config;
        self.force_reset();
    }
}

impl ConnectionHealthMonitor {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            node_health: HashMap::new(),
        }
    }

    pub fn record_failure(&mut self, node_id: &str) {
        let health = self.node_health.entry(node_id.to_string())
            .or_insert_with(NodeHealthMetrics::default);
        
        health.consecutive_failures += 1;
        
        health.status = if health.consecutive_failures >= self.config.failure_threshold {
            NodeHealthStatus::Unhealthy
        } else {
            NodeHealthStatus::Degraded
        };
    }

    pub fn record_success(&mut self, node_id: &str) {
        let health = self.node_health.entry(node_id.to_string())
            .or_insert_with(NodeHealthMetrics::default);
        
        health.consecutive_failures = 0;
        health.last_success = Some(Instant::now());
        health.status = NodeHealthStatus::Healthy;
    }

    pub fn get_node_health(&self, node_id: &str) -> NodeHealthStatus {
        self.node_health.get(node_id)
            .map(|h| h.status.clone())
            .unwrap_or(NodeHealthStatus::Unknown)
    }

    pub async fn check_all_nodes(&mut self) -> HashMap<String, NodeHealthStatus> {
        let mut results = HashMap::new();
        
        for (node_id, health) in &self.node_health {
            results.insert(node_id.clone(), health.status.clone());
        }
        
        results
    }

    pub fn reset_node_health(&mut self, node_id: &str) {
        self.node_health.remove(node_id);
    }
}

impl Default for NodeHealthMetrics {
    fn default() -> Self {
        Self {
            status: NodeHealthStatus::Unknown,
            consecutive_failures: 0,
            last_success: None,
            latency_history: Vec::new(),
            success_rate: 1.0,
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

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(30),
            check_timeout: Duration::from_secs(5),
            failure_threshold: 3,
            latency_threshold: Duration::from_secs(2),
        }
    }
}

impl BackoffDecision {
    pub fn should_proceed(&self) -> bool {
        matches!(self, BackoffDecision::Proceed)
    }

    pub fn wait_time(&self) -> Option<Duration> {
        match self {
            BackoffDecision::Wait { delay } => Some(*delay),
            BackoffDecision::CircuitOpen { recovery_time } => Some(*recovery_time),
            _ => None,
        }
    }

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