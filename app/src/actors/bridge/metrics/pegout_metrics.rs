//! PegOut Actor Metrics
//! 
//! Metrics collection for PegOut operations

use super::{BridgeMetrics, BaseMetricsCollector, MetricsSnapshot};
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics collector for PegOutActor
pub struct PegOutMetrics {
    /// Base metrics collector
    base: BaseMetricsCollector,
    
    /// PegOut-specific metrics
    withdrawal_requests: AtomicU64,
    bitcoin_transactions_broadcast: AtomicU64,
    fee_estimations: AtomicU64,
    utxo_selections: AtomicU64,
}

impl PegOutMetrics {
    pub fn new() -> Self {
        Self {
            base: BaseMetricsCollector::new("PegOutActor"),
            withdrawal_requests: AtomicU64::new(0),
            bitcoin_transactions_broadcast: AtomicU64::new(0),
            fee_estimations: AtomicU64::new(0),
            utxo_selections: AtomicU64::new(0),
        }
    }
    
    pub fn record_withdrawal_request(&self) {
        self.withdrawal_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_bitcoin_broadcast(&self) {
        self.bitcoin_transactions_broadcast.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_fee_estimation(&self) {
        self.fee_estimations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_utxo_selection(&self) {
        self.utxo_selections.fetch_add(1, Ordering::Relaxed);
    }
}

impl BridgeMetrics for PegOutMetrics {
    fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.base.record_operation(operation, duration, success);
        
        match operation {
            "withdrawal_request" => self.record_withdrawal_request(),
            "bitcoin_broadcast" => self.record_bitcoin_broadcast(),
            "fee_estimation" => self.record_fee_estimation(),
            "utxo_selection" => self.record_utxo_selection(),
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
        self.withdrawal_requests.store(0, Ordering::Relaxed);
        self.bitcoin_transactions_broadcast.store(0, Ordering::Relaxed);
        self.fee_estimations.store(0, Ordering::Relaxed);
        self.utxo_selections.store(0, Ordering::Relaxed);
    }
}

impl Default for PegOutMetrics {
    fn default() -> Self {
        Self::new()
    }
}