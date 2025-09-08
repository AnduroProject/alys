//! Bridge Actor Metrics
//! 
//! Metrics collection for the main BridgeActor

use super::{BridgeMetrics, BaseMetricsCollector, MetricsSnapshot};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics collector for BridgeActor
pub struct BridgeActorMetrics {
    /// Base metrics collector
    base: BaseMetricsCollector,
    
    /// Bridge-specific metrics
    peg_in_requests: AtomicU64,
    peg_out_requests: AtomicU64,
    governance_messages: AtomicU64,
    coordination_events: AtomicU64,
    actor_supervision_events: AtomicU64,
}

impl BridgeActorMetrics {
    pub fn new() -> Self {
        Self {
            base: BaseMetricsCollector::new("BridgeActor"),
            peg_in_requests: AtomicU64::new(0),
            peg_out_requests: AtomicU64::new(0),
            governance_messages: AtomicU64::new(0),
            coordination_events: AtomicU64::new(0),
            actor_supervision_events: AtomicU64::new(0),
        }
    }
    
    /// Record a peg-in request
    pub fn record_peg_in_request(&self) {
        self.peg_in_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a peg-out request
    pub fn record_peg_out_request(&self) {
        self.peg_out_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a governance message
    pub fn record_governance_message(&self) {
        self.governance_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a coordination event
    pub fn record_coordination_event(&self) {
        self.coordination_events.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record an actor supervision event
    pub fn record_supervision_event(&self) {
        self.actor_supervision_events.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get bridge-specific metrics
    pub fn get_bridge_metrics(&self) -> BridgeMetricsSnapshot {
        let base_snapshot = self.base.get_metrics_snapshot();
        
        BridgeMetricsSnapshot {
            base: base_snapshot,
            peg_in_requests: self.peg_in_requests.load(Ordering::Relaxed),
            peg_out_requests: self.peg_out_requests.load(Ordering::Relaxed),
            governance_messages: self.governance_messages.load(Ordering::Relaxed),
            coordination_events: self.coordination_events.load(Ordering::Relaxed),
            actor_supervision_events: self.actor_supervision_events.load(Ordering::Relaxed),
        }
    }
}

impl BridgeMetrics for BridgeActorMetrics {
    fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.base.record_operation(operation, duration, success);
        
        // Record specific operation types
        match operation {
            "peg_in" => self.record_peg_in_request(),
            "peg_out" => self.record_peg_out_request(),
            "governance" => self.record_governance_message(),
            "coordination" => self.record_coordination_event(),
            "supervision" => self.record_supervision_event(),
            _ => {}
        }
    }
    
    fn record_error(&self, error_type: &str) {
        self.base.record_error(error_type);
    }
    
    fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        self.base.get_metrics_snapshot()
    }
    
    fn reset_metrics(&self) {
        self.base.reset_metrics();
        self.peg_in_requests.store(0, Ordering::Relaxed);
        self.peg_out_requests.store(0, Ordering::Relaxed);
        self.governance_messages.store(0, Ordering::Relaxed);
        self.coordination_events.store(0, Ordering::Relaxed);
        self.actor_supervision_events.store(0, Ordering::Relaxed);
    }
}

/// Bridge-specific metrics snapshot
#[derive(Debug, Clone)]
pub struct BridgeMetricsSnapshot {
    /// Base metrics
    pub base: MetricsSnapshot,
    
    /// Total peg-in requests
    pub peg_in_requests: u64,
    
    /// Total peg-out requests
    pub peg_out_requests: u64,
    
    /// Total governance messages
    pub governance_messages: u64,
    
    /// Total coordination events
    pub coordination_events: u64,
    
    /// Total actor supervision events
    pub actor_supervision_events: u64,
}

impl Default for BridgeActorMetrics {
    fn default() -> Self {
        Self::new()
    }
}