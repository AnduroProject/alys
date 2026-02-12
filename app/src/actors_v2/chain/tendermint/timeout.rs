//! Timeout management for Tendermint consensus.
//!
//! This module provides timeout scheduling for each consensus step.
//! Timeouts are critical for liveness - without them, the protocol
//! would halt if a proposer fails.
//!
//! # Timeout Behavior
//!
//! - **Propose timeout**: If no proposal received, prevote NIL
//! - **Prevote timeout**: After 2/3+ any, if no majority, precommit
//! - **Precommit timeout**: After 2/3+ any, if no majority, new round
//!
//! # Exponential Backoff
//!
//! Timeouts increase with each round to handle network delays:
//! `timeout(round) = base_timeout + (round * delta)`

use super::types::*;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tracing::{debug, warn};

/// Timeout configuration
///
/// Configures the base timeouts and per-round delta for each step.
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Base timeout for propose step
    pub propose_timeout: Duration,

    /// Base timeout for prevote step
    pub prevote_timeout: Duration,

    /// Base timeout for precommit step
    pub precommit_timeout: Duration,

    /// Additional time per round (exponential backoff)
    pub timeout_delta: Duration,

    /// Maximum timeout (cap for backoff)
    pub max_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            propose_timeout: Duration::from_millis(3000),
            prevote_timeout: Duration::from_millis(1000),
            precommit_timeout: Duration::from_millis(1000),
            timeout_delta: Duration::from_millis(500),
            max_timeout: Duration::from_secs(30),
        }
    }
}

impl TimeoutConfig {
    /// Calculate timeout for a step at a given round
    ///
    /// # Commit Step
    ///
    /// Returns `Duration::ZERO` for `TendermintStep::Commit` because:
    /// - Commit is not a waiting state - it's the final action
    /// - Once 2/3+ precommits are collected, commit happens immediately
    /// - No timeout needed; the block is finalized synchronously
    pub fn timeout_for(&self, step: TendermintStep, round: u32) -> Duration {
        // Commit step has no timeout - return immediately
        if step == TendermintStep::Commit {
            return Duration::ZERO;
        }

        let base = match step {
            TendermintStep::Propose => self.propose_timeout,
            TendermintStep::Prevote => self.prevote_timeout,
            TendermintStep::Precommit => self.precommit_timeout,
            TendermintStep::Commit => unreachable!(), // Already handled above
        };

        let with_backoff = base + self.timeout_delta * round;

        // Cap at maximum
        std::cmp::min(with_backoff, self.max_timeout)
    }

    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self {
            propose_timeout: Duration::from_millis(
                std::env::var("TENDERMINT_PROPOSE_TIMEOUT_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3000),
            ),
            prevote_timeout: Duration::from_millis(
                std::env::var("TENDERMINT_PREVOTE_TIMEOUT_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1000),
            ),
            precommit_timeout: Duration::from_millis(
                std::env::var("TENDERMINT_PRECOMMIT_TIMEOUT_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1000),
            ),
            timeout_delta: Duration::from_millis(
                std::env::var("TENDERMINT_TIMEOUT_DELTA_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(500),
            ),
            max_timeout: Duration::from_secs(
                std::env::var("TENDERMINT_MAX_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
        }
    }

    /// Aggressive config for testing
    pub fn fast_for_testing() -> Self {
        Self {
            propose_timeout: Duration::from_millis(100),
            prevote_timeout: Duration::from_millis(50),
            precommit_timeout: Duration::from_millis(50),
            timeout_delta: Duration::from_millis(25),
            max_timeout: Duration::from_secs(5),
        }
    }
}

/// A scheduled timeout that can be cancelled
#[derive(Debug)]
pub struct ScheduledTimeout {
    /// When this timeout was scheduled
    pub scheduled_at: Instant,

    /// When this timeout expires
    pub expires_at: Instant,

    /// The step this timeout is for
    pub step: TendermintStep,

    /// Height/round this timeout is for
    pub height: Height,
    pub round: Round,

    /// Handle to cancel this timeout
    cancel_tx: mpsc::Sender<()>,
}

/// Timeout event sent when a timeout expires
#[derive(Debug, Clone)]
pub struct TimeoutEvent {
    pub height: Height,
    pub round: Round,
    pub step: TendermintStep,
}

/// Timeout-related errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum TimeoutError {
    /// Attempted to schedule timeout for invalid step (e.g., Commit)
    #[error("Cannot schedule timeout for step: {0:?}")]
    InvalidStep(TendermintStep),

    /// Scheduler channel closed
    #[error("Timeout event channel closed")]
    ChannelClosed,

    /// Scheduler not initialized
    #[error("Timeout scheduler not initialized")]
    NotInitialized,
}

/// Manages timeout scheduling for Tendermint consensus
///
/// The scheduler maintains at most one active timeout per step.
/// When a new timeout is scheduled for a step, any existing timeout
/// for that step is cancelled.
pub struct TimeoutScheduler {
    /// Configuration
    config: TimeoutConfig,

    /// Currently active timeouts
    active_timeouts: Vec<ScheduledTimeout>,

    /// Channel to send timeout events
    event_tx: mpsc::Sender<TimeoutEvent>,

    /// Current consensus position
    current_height: Height,
    current_round: Round,
}

impl TimeoutScheduler {
    /// Create a new timeout scheduler
    pub fn new(config: TimeoutConfig, event_tx: mpsc::Sender<TimeoutEvent>) -> Self {
        Self {
            config,
            active_timeouts: Vec::new(),
            event_tx,
            current_height: 0,
            current_round: 0,
        }
    }

    /// Update current position and cancel stale timeouts
    pub fn set_position(&mut self, height: Height, round: Round) {
        if height != self.current_height || round != self.current_round {
            // Cancel all timeouts from previous height/round
            self.cancel_all_stale(height, round);
            self.current_height = height;
            self.current_round = round;
        }
    }

    /// Get the current position
    pub fn position(&self) -> (Height, Round) {
        (self.current_height, self.current_round)
    }

    /// Schedule a timeout for a step
    ///
    /// If a timeout for this step already exists, it is cancelled first.
    ///
    /// # Errors
    ///
    /// Returns `TimeoutError::InvalidStep` if attempting to schedule
    /// a timeout for `TendermintStep::Commit`.
    pub fn schedule(&mut self, step: TendermintStep) -> Result<(), TimeoutError> {
        // Commit step has no timeout - it's a logic error to schedule one
        if step == TendermintStep::Commit {
            warn!("Attempted to schedule timeout for Commit step");
            return Err(TimeoutError::InvalidStep(step));
        }

        // Cancel existing timeout for this step (prevents duplicates)
        self.cancel_step(step);

        // Calculate timeout duration
        let duration = self.config.timeout_for(step, self.current_round);

        // Create cancellation channel
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

        let height = self.current_height;
        let round = self.current_round;
        let event_tx = self.event_tx.clone();

        // Store scheduled timeout info
        let now = Instant::now();
        self.active_timeouts.push(ScheduledTimeout {
            scheduled_at: now,
            expires_at: now + duration,
            step,
            height,
            round,
            cancel_tx: cancel_tx.clone(),
        });

        debug!(
            height,
            round,
            step = ?step,
            duration_ms = duration.as_millis(),
            "Scheduled timeout"
        );

        // Spawn timeout task
        tokio::spawn(async move {
            tokio::select! {
                _ = sleep(duration) => {
                    // Timeout expired
                    let event = TimeoutEvent { height, round, step };
                    if event_tx.send(event).await.is_err() {
                        warn!("Failed to send timeout event");
                    }
                    debug!(height, round, step = ?step, "Timeout expired");
                }
                _ = cancel_rx.recv() => {
                    // Timeout was cancelled
                    debug!(height, round, step = ?step, "Timeout cancelled");
                }
            }
        });

        Ok(())
    }

    /// Cancel timeout for a specific step
    pub fn cancel_step(&mut self, step: TendermintStep) {
        self.active_timeouts.retain(|t| {
            if t.step == step && t.height == self.current_height && t.round == self.current_round {
                // Send cancel signal (ignore errors)
                let _ = t.cancel_tx.try_send(());
                false // Remove from list
            } else {
                true // Keep
            }
        });
    }

    /// Cancel all stale timeouts (from previous heights/rounds)
    fn cancel_all_stale(&mut self, new_height: Height, new_round: Round) {
        self.active_timeouts.retain(|t| {
            if t.height < new_height || (t.height == new_height && t.round < new_round) {
                let _ = t.cancel_tx.try_send(());
                false
            } else {
                true
            }
        });
    }

    /// Cancel all active timeouts
    pub fn cancel_all(&mut self) {
        for timeout in &self.active_timeouts {
            let _ = timeout.cancel_tx.try_send(());
        }
        self.active_timeouts.clear();
    }

    /// Get remaining time for a step's timeout
    pub fn time_remaining(&self, step: TendermintStep) -> Option<Duration> {
        self.active_timeouts
            .iter()
            .find(|t| {
                t.step == step && t.height == self.current_height && t.round == self.current_round
            })
            .map(|t| {
                let now = Instant::now();
                if now >= t.expires_at {
                    Duration::ZERO
                } else {
                    t.expires_at - now
                }
            })
    }

    /// Check if a timeout is active for a step
    pub fn is_active(&self, step: TendermintStep) -> bool {
        self.active_timeouts.iter().any(|t| {
            t.step == step && t.height == self.current_height && t.round == self.current_round
        })
    }

    /// Get the number of active timeouts
    pub fn active_count(&self) -> usize {
        self.active_timeouts.len()
    }

    /// Get the timeout configuration
    pub fn config(&self) -> &TimeoutConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_timeout_fires() {
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let config = TimeoutConfig {
            propose_timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);
        scheduler.schedule(TendermintStep::Propose).unwrap();

        // Should receive event within 100ms
        let event = timeout(Duration::from_millis(100), event_rx.recv())
            .await
            .expect("Timeout waiting for event")
            .expect("Channel closed");

        assert_eq!(event.height, 100);
        assert_eq!(event.round, 0);
        assert_eq!(event.step, TendermintStep::Propose);
    }

    #[tokio::test]
    async fn test_timeout_cancelled() {
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let config = TimeoutConfig {
            propose_timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);
        scheduler.schedule(TendermintStep::Propose).unwrap();

        // Cancel immediately
        scheduler.cancel_step(TendermintStep::Propose);

        // Should NOT receive event
        let result = timeout(Duration::from_millis(100), event_rx.recv()).await;
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_stale_timeouts_cancelled() {
        // Test that cancel_all_stale removes old timeouts
        let (event_tx, _event_rx) = mpsc::channel(32);
        let config = TimeoutConfig::fast_for_testing();

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);
        scheduler.schedule(TendermintStep::Propose).unwrap();
        scheduler.schedule(TendermintStep::Prevote).unwrap();

        assert_eq!(scheduler.active_count(), 2);

        // Advance to new round - stale timeouts should be removed
        scheduler.set_position(100, 1);

        // Old timeouts should be removed from active list
        assert_eq!(scheduler.active_count(), 0);

        // Schedule new timeouts
        scheduler.schedule(TendermintStep::Propose).unwrap();
        assert_eq!(scheduler.active_count(), 1);
    }

    #[test]
    fn test_timeout_backoff() {
        let config = TimeoutConfig::default();

        // Round 0
        assert_eq!(
            config.timeout_for(TendermintStep::Propose, 0),
            Duration::from_millis(3000)
        );

        // Round 1 (+500ms)
        assert_eq!(
            config.timeout_for(TendermintStep::Propose, 1),
            Duration::from_millis(3500)
        );

        // Round 10 (+5000ms)
        assert_eq!(
            config.timeout_for(TendermintStep::Propose, 10),
            Duration::from_millis(8000)
        );

        // Round 100 - should cap at max
        let timeout_100 = config.timeout_for(TendermintStep::Propose, 100);
        assert!(timeout_100 <= config.max_timeout);
    }

    #[tokio::test]
    async fn test_commit_step_rejected() {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let config = TimeoutConfig::default();

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);

        // Commit step should be rejected
        let result = scheduler.schedule(TendermintStep::Commit);
        assert!(matches!(result, Err(TimeoutError::InvalidStep(_))));
    }

    #[tokio::test]
    async fn test_duplicate_scheduling_cancels_previous() {
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let config = TimeoutConfig {
            propose_timeout: Duration::from_millis(100),
            ..Default::default()
        };

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);

        // Schedule twice in quick succession
        scheduler.schedule(TendermintStep::Propose).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        scheduler.schedule(TendermintStep::Propose).unwrap();

        // Should only receive ONE event (second scheduling cancels first)
        let event = timeout(Duration::from_millis(150), event_rx.recv())
            .await
            .expect("Timeout waiting for event")
            .expect("Channel closed");

        assert_eq!(event.step, TendermintStep::Propose);

        // No second event should come
        let result = timeout(Duration::from_millis(50), event_rx.recv()).await;
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_is_active() {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let config = TimeoutConfig {
            propose_timeout: Duration::from_millis(1000),
            ..Default::default()
        };

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);

        assert!(!scheduler.is_active(TendermintStep::Propose));

        scheduler.schedule(TendermintStep::Propose).unwrap();

        assert!(scheduler.is_active(TendermintStep::Propose));
        assert!(!scheduler.is_active(TendermintStep::Prevote));
    }

    #[tokio::test]
    async fn test_time_remaining() {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let config = TimeoutConfig {
            propose_timeout: Duration::from_millis(1000),
            ..Default::default()
        };

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);

        assert!(scheduler.time_remaining(TendermintStep::Propose).is_none());

        scheduler.schedule(TendermintStep::Propose).unwrap();

        let remaining = scheduler.time_remaining(TendermintStep::Propose);
        assert!(remaining.is_some());
        assert!(remaining.unwrap() > Duration::ZERO);
        assert!(remaining.unwrap() <= Duration::from_millis(1000));
    }

    #[test]
    fn test_fast_for_testing_config() {
        let config = TimeoutConfig::fast_for_testing();
        assert_eq!(config.propose_timeout, Duration::from_millis(100));
        assert_eq!(config.prevote_timeout, Duration::from_millis(50));
        assert_eq!(config.precommit_timeout, Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_cancel_all() {
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let config = TimeoutConfig {
            propose_timeout: Duration::from_millis(100),
            prevote_timeout: Duration::from_millis(100),
            ..Default::default()
        };

        let mut scheduler = TimeoutScheduler::new(config, event_tx);
        scheduler.set_position(100, 0);

        scheduler.schedule(TendermintStep::Propose).unwrap();
        scheduler.schedule(TendermintStep::Prevote).unwrap();

        assert_eq!(scheduler.active_count(), 2);

        scheduler.cancel_all();

        assert_eq!(scheduler.active_count(), 0);

        // No events should be received
        let result = timeout(Duration::from_millis(150), event_rx.recv()).await;
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_position_tracking() {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let config = TimeoutConfig::default();

        let mut scheduler = TimeoutScheduler::new(config, event_tx);

        assert_eq!(scheduler.position(), (0, 0));

        scheduler.set_position(100, 5);
        assert_eq!(scheduler.position(), (100, 5));

        scheduler.set_position(101, 0);
        assert_eq!(scheduler.position(), (101, 0));
    }

    #[test]
    fn test_commit_step_zero_duration() {
        let config = TimeoutConfig::default();
        assert_eq!(
            config.timeout_for(TendermintStep::Commit, 0),
            Duration::ZERO
        );
        assert_eq!(
            config.timeout_for(TendermintStep::Commit, 10),
            Duration::ZERO
        );
    }
}
