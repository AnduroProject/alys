//! PegIn Actor Metrics
//! 
//! Metrics collection for PegIn operations

use super::{BridgeMetrics, BaseMetricsCollector, MetricsSnapshot};
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics collector for PegInActor
pub struct PegInMetrics {
    /// Base metrics collector
    base: BaseMetricsCollector,
    
    /// PegIn-specific metrics
    bitcoin_confirmations_processed: AtomicU64,
    signature_verifications: AtomicU64,
    deposit_validations: AtomicU64,
    federation_notifications: AtomicU64,
}

impl PegInMetrics {
    pub fn new() -> Self {
        Self {
            base: BaseMetricsCollector::new("PegInActor"),
            bitcoin_confirmations_processed: AtomicU64::new(0),
            signature_verifications: AtomicU64::new(0),
            deposit_validations: AtomicU64::new(0),
            federation_notifications: AtomicU64::new(0),
        }
    }
    
    pub fn record_bitcoin_confirmation(&self) {
        self.bitcoin_confirmations_processed.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_signature_verification(&self) {
        self.signature_verifications.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_deposit_validation(&self) {
        self.deposit_validations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_federation_notification(&self) {
        self.federation_notifications.fetch_add(1, Ordering::Relaxed);
    }
}

impl BridgeMetrics for PegInMetrics {
    fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.base.record_operation(operation, duration, success);
        
        match operation {
            "bitcoin_confirmation" => self.record_bitcoin_confirmation(),
            "signature_verification" => self.record_signature_verification(),
            "deposit_validation" => self.record_deposit_validation(),
            "federation_notification" => self.record_federation_notification(),
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
        self.bitcoin_confirmations_processed.store(0, Ordering::Relaxed);
        self.signature_verifications.store(0, Ordering::Relaxed);
        self.deposit_validations.store(0, Ordering::Relaxed);
        self.federation_notifications.store(0, Ordering::Relaxed);
    }
}

impl Default for PegInMetrics {
    fn default() -> Self {
        Self::new()
    }
}