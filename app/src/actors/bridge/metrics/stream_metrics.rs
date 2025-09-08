//! Stream Actor Metrics
//! 
//! Metrics collection for Stream operations

use super::{BridgeMetrics, BaseMetricsCollector, MetricsSnapshot};
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics collector for StreamActor
pub struct StreamMetrics {
    /// Base metrics collector
    base: BaseMetricsCollector,
    
    /// Stream-specific metrics
    governance_messages: AtomicU64,
    grpc_connections: AtomicU64,
    message_buffer_operations: AtomicU64,
    reconnection_attempts: AtomicU64,
}

impl StreamMetrics {
    pub fn new() -> Self {
        Self {
            base: BaseMetricsCollector::new("StreamActor"),
            governance_messages: AtomicU64::new(0),
            grpc_connections: AtomicU64::new(0),
            message_buffer_operations: AtomicU64::new(0),
            reconnection_attempts: AtomicU64::new(0),
        }
    }
    
    pub fn record_governance_message(&self) {
        self.governance_messages.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_grpc_connection(&self) {
        self.grpc_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_buffer_operation(&self) {
        self.message_buffer_operations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_reconnection_attempt(&self) {
        self.reconnection_attempts.fetch_add(1, Ordering::Relaxed);
    }
}

impl BridgeMetrics for StreamMetrics {
    fn record_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.base.record_operation(operation, duration, success);
        
        match operation {
            "governance_message" => self.record_governance_message(),
            "grpc_connection" => self.record_grpc_connection(),
            "buffer_operation" => self.record_buffer_operation(),
            "reconnection_attempt" => self.record_reconnection_attempt(),
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
        self.governance_messages.store(0, Ordering::Relaxed);
        self.grpc_connections.store(0, Ordering::Relaxed);
        self.message_buffer_operations.store(0, Ordering::Relaxed);
        self.reconnection_attempts.store(0, Ordering::Relaxed);
    }
}

impl Default for StreamMetrics {
    fn default() -> Self {
        Self::new()
    }
}