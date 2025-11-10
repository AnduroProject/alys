//! EngineActor V2 Metrics
//!
//! Comprehensive metrics collection for execution layer operations

use prometheus::{Counter, Histogram, HistogramOpts, IntGauge};

/// EngineActor metrics collection
#[derive(Debug, Clone)]
pub struct EngineActorMetrics {
    // Operation counters
    pub build_payload_calls: Counter,
    pub build_payload_success: Counter,
    pub build_payload_failed: Counter,

    pub validate_payload_calls: Counter,
    pub validate_payload_success: Counter,
    pub validate_payload_failed: Counter,

    pub commit_block_calls: Counter,
    pub commit_block_success: Counter,
    pub commit_block_failed: Counter,

    pub fork_choice_update_calls: Counter,
    pub fork_choice_update_success: Counter,
    pub fork_choice_update_failed: Counter,

    // Performance metrics
    pub build_payload_duration: Histogram,
    pub validate_payload_duration: Histogram,
    pub commit_block_duration: Histogram,
    pub fork_choice_update_duration: Histogram,

    // State metrics
    pub active_operations: IntGauge,
    pub finalized_block_height: IntGauge,
    pub head_block_height: IntGauge,

    // Error tracking
    pub engine_api_errors: Counter,
    pub timeout_errors: Counter,
    pub validation_errors: Counter,
}

impl Default for EngineActorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineActorMetrics {
    /// Create new EngineActorMetrics
    pub fn new() -> Self {
        Self {
            // Operation counters
            build_payload_calls: Counter::new(
                "engine_actor_build_payload_calls_total",
                "Total number of build payload requests",
            )
            .unwrap(),
            build_payload_success: Counter::new(
                "engine_actor_build_payload_success_total",
                "Successful build payload operations",
            )
            .unwrap(),
            build_payload_failed: Counter::new(
                "engine_actor_build_payload_failed_total",
                "Failed build payload operations",
            )
            .unwrap(),

            validate_payload_calls: Counter::new(
                "engine_actor_validate_payload_calls_total",
                "Total number of payload validation requests",
            )
            .unwrap(),
            validate_payload_success: Counter::new(
                "engine_actor_validate_payload_success_total",
                "Successful payload validations",
            )
            .unwrap(),
            validate_payload_failed: Counter::new(
                "engine_actor_validate_payload_failed_total",
                "Failed payload validations",
            )
            .unwrap(),

            commit_block_calls: Counter::new(
                "engine_actor_commit_block_calls_total",
                "Total number of block commit requests",
            )
            .unwrap(),
            commit_block_success: Counter::new(
                "engine_actor_commit_block_success_total",
                "Successful block commits",
            )
            .unwrap(),
            commit_block_failed: Counter::new(
                "engine_actor_commit_block_failed_total",
                "Failed block commits",
            )
            .unwrap(),

            fork_choice_update_calls: Counter::new(
                "engine_actor_fork_choice_update_calls_total",
                "Total number of fork choice update requests",
            )
            .unwrap(),
            fork_choice_update_success: Counter::new(
                "engine_actor_fork_choice_update_success_total",
                "Successful fork choice updates",
            )
            .unwrap(),
            fork_choice_update_failed: Counter::new(
                "engine_actor_fork_choice_update_failed_total",
                "Failed fork choice updates",
            )
            .unwrap(),

            // Performance metrics
            build_payload_duration: Histogram::with_opts(HistogramOpts::new(
                "engine_actor_build_payload_duration_seconds",
                "Time spent building execution payloads",
            ))
            .unwrap(),
            validate_payload_duration: Histogram::with_opts(HistogramOpts::new(
                "engine_actor_validate_payload_duration_seconds",
                "Time spent validating execution payloads",
            ))
            .unwrap(),
            commit_block_duration: Histogram::with_opts(HistogramOpts::new(
                "engine_actor_commit_block_duration_seconds",
                "Time spent committing blocks",
            ))
            .unwrap(),
            fork_choice_update_duration: Histogram::with_opts(HistogramOpts::new(
                "engine_actor_fork_choice_update_duration_seconds",
                "Fork choice update operation duration",
            ))
            .unwrap(),

            // State metrics
            active_operations: IntGauge::new(
                "engine_actor_active_operations",
                "Number of active engine operations",
            )
            .unwrap(),
            finalized_block_height: IntGauge::new(
                "engine_actor_finalized_block_height",
                "Height of last finalized block",
            )
            .unwrap(),
            head_block_height: IntGauge::new(
                "engine_actor_head_block_height",
                "Height of current head block",
            )
            .unwrap(),

            // Error tracking
            engine_api_errors: Counter::new(
                "engine_actor_api_errors_total",
                "Engine API errors encountered",
            )
            .unwrap(),
            timeout_errors: Counter::new(
                "engine_actor_timeout_errors_total",
                "Engine operation timeouts",
            )
            .unwrap(),
            validation_errors: Counter::new(
                "engine_actor_validation_errors_total",
                "Payload validation errors",
            )
            .unwrap(),
        }
    }

    /// Record successful build payload operation
    pub fn record_build_payload_success(&self, duration: std::time::Duration) {
        self.build_payload_calls.inc();
        self.build_payload_success.inc();
        self.build_payload_duration.observe(duration.as_secs_f64());
    }

    /// Record failed build payload operation
    pub fn record_build_payload_failure(&self, duration: std::time::Duration) {
        self.build_payload_calls.inc();
        self.build_payload_failed.inc();
        self.build_payload_duration.observe(duration.as_secs_f64());
    }

    /// Record successful payload validation
    pub fn record_validate_payload_success(&self, duration: std::time::Duration) {
        self.validate_payload_calls.inc();
        self.validate_payload_success.inc();
        self.validate_payload_duration
            .observe(duration.as_secs_f64());
    }

    /// Record failed payload validation
    pub fn record_validate_payload_failure(&self, duration: std::time::Duration) {
        self.validate_payload_calls.inc();
        self.validate_payload_failed.inc();
        self.validate_payload_duration
            .observe(duration.as_secs_f64());
    }

    /// Record successful block commit
    pub fn record_commit_block_success(&self, duration: std::time::Duration) {
        self.commit_block_calls.inc();
        self.commit_block_success.inc();
        self.commit_block_duration.observe(duration.as_secs_f64());
    }

    /// Record failed block commit
    pub fn record_commit_block_failure(&self, duration: std::time::Duration) {
        self.commit_block_calls.inc();
        self.commit_block_failed.inc();
        self.commit_block_duration.observe(duration.as_secs_f64());
    }

    /// Update active operation count
    pub fn set_active_operations(&self, count: i64) {
        self.active_operations.set(count);
    }

    /// Update finalized block height
    pub fn set_finalized_block_height(&self, height: u64) {
        self.finalized_block_height.set(height as i64);
    }

    /// Update head block height
    pub fn set_head_block_height(&self, height: u64) {
        self.head_block_height.set(height as i64);
    }

    /// Record engine API error
    pub fn record_engine_api_error(&self) {
        self.engine_api_errors.inc();
    }

    /// Record timeout error
    pub fn record_timeout_error(&self) {
        self.timeout_errors.inc();
    }

    /// Record validation error
    pub fn record_validation_error(&self) {
        self.validation_errors.inc();
    }

    /// Record successful fork choice update
    pub fn record_fork_choice_update_success(&self, duration: std::time::Duration) {
        self.fork_choice_update_calls.inc();
        self.fork_choice_update_success.inc();
        self.fork_choice_update_duration.observe(duration.as_secs_f64());
    }

    /// Record failed fork choice update
    pub fn record_fork_choice_update_failure(&self, duration: std::time::Duration) {
        self.fork_choice_update_calls.inc();
        self.fork_choice_update_failed.inc();
        self.fork_choice_update_duration.observe(duration.as_secs_f64());
    }
}
