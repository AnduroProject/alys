//! PegOut Actor State Management
//! 
//! State structures and management for PegOut operations

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// PegOut actor state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegOutState {
    /// Actor is initializing
    Initializing,
    /// Actor is operational
    Operational,
    /// Actor is in degraded state
    Degraded { issues: Vec<String> },
    /// Actor is paused
    Paused,
    /// Actor is stopping
    Stopping,
    /// Actor has stopped
    Stopped,
}

/// Operation tracker for performance monitoring
pub use crate::actors::bridge::actors::pegin::state::OperationTracker;

/// Operation event types specific to PegOut
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationEventType {
    BurnEventProcessed,
    TransactionBuilt,
    SignaturesRequested,
    SignaturesReceived,
    SignaturesApplied,
    TransactionBroadcast,
    TransactionConfirmed,
    PegOutCompleted,
    OperationFailed,
    OperationRetried,
}

/// PegOut actor metrics
#[derive(Debug, Clone)]
pub struct PegOutMetrics {
    pub start_time: SystemTime,
    
    // Operation counters
    burn_events_processed: u64,
    transactions_built: u64,
    signatures_requested: u64,
    signatures_applied: u64,
    transactions_broadcast: u64,
    pegouts_completed: u64,
    pegouts_failed: u64,
    
    // Error counters
    signature_timeouts: u64,
    broadcast_failures: u64,
    validation_errors: u64,
    max_retries_exceeded: u64,
    
    // Performance metrics
    average_processing_time: Duration,
    success_rate: f64,
    
    // System metrics
    actor_restarts: u64,
}

impl PegOutMetrics {
    /// Create new metrics instance
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            start_time: SystemTime::now(),
            burn_events_processed: 0,
            transactions_built: 0,
            signatures_requested: 0,
            signatures_applied: 0,
            transactions_broadcast: 0,
            pegouts_completed: 0,
            pegouts_failed: 0,
            signature_timeouts: 0,
            broadcast_failures: 0,
            validation_errors: 0,
            max_retries_exceeded: 0,
            average_processing_time: Duration::from_secs(0),
            success_rate: 0.0,
            actor_restarts: 0,
        })
    }

    pub fn record_actor_started(&mut self) {
        self.start_time = SystemTime::now();
    }

    pub fn record_actor_stopped(&mut self) {}

    pub fn record_burn_event_processed(&mut self) {
        self.burn_events_processed += 1;
    }

    pub fn record_transaction_built(&mut self) {
        self.transactions_built += 1;
    }

    pub fn record_signatures_requested(&mut self) {
        self.signatures_requested += 1;
    }

    pub fn record_signatures_applied(&mut self) {
        self.signatures_applied += 1;
    }

    pub fn record_transaction_broadcast(&mut self) {
        self.transactions_broadcast += 1;
    }

    pub fn record_pegout_completed(&mut self) {
        self.pegouts_completed += 1;
        self.update_success_rate();
    }

    pub fn record_signature_timeout(&mut self) {
        self.signature_timeouts += 1;
        self.pegouts_failed += 1;
        self.update_success_rate();
    }

    pub fn record_max_retries_exceeded(&mut self) {
        self.max_retries_exceeded += 1;
        self.pegouts_failed += 1;
        self.update_success_rate();
    }

    pub fn record_error(&mut self, _error: &super::actor::PegOutError) {
        self.validation_errors += 1;
    }

    pub fn get_pegouts_processed(&self) -> u64 {
        self.burn_events_processed
    }

    fn update_success_rate(&mut self) {
        let total = self.pegouts_completed + self.pegouts_failed;
        if total > 0 {
            self.success_rate = self.pegouts_completed as f64 / total as f64;
        }
    }
}

impl Default for PegOutState {
    fn default() -> Self {
        Self::Initializing
    }
}