//! StreamActor V2 Enhanced Metrics
//!
//! This module provides comprehensive metrics for the V2 StreamActor, focusing on
//! governance communication, gRPC connection monitoring, and message correlation tracking.
//! Implements Plan A from ALYS-003 Next Steps: StreamActor Monitoring Enhancement.

use crate::metrics::{ALYS_REGISTRY, MetricLabels};
use std::time::{Duration, Instant, SystemTime};
use std::collections::HashMap;
use std::sync::Arc;
use prometheus::{
    register_histogram_with_registry, register_histogram_vec_with_registry,
    register_counter_with_registry, register_counter_vec_with_registry,
    register_gauge_with_registry, register_gauge_vec_with_registry,
    register_int_gauge_with_registry, register_int_gauge_vec_with_registry,
    Histogram, HistogramVec, Counter, CounterVec, Gauge, GaugeVec, IntGauge, IntGaugeVec,
    HistogramOpts, Opts,
};
use lazy_static::lazy_static;
use tracing::*;
use serde_json;
use parking_lot::RwLock;

lazy_static! {
    // === StreamActor Governance Connection Metrics ===
    pub static ref GOVERNANCE_CONNECTION_STATUS: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_governance_connection_status",
        "Governance connection status (0=disconnected, 1=connected, 2=authenticated, 3=streaming)",
        &["endpoint", "node_id"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_CONNECTION_LATENCY: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_governance_connection_latency_seconds",
            "gRPC connection establishment latency to governance nodes"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]),
        &["endpoint"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_MESSAGE_BUFFER_SIZE: IntGaugeVec = register_int_gauge_vec_with_registry!(
        "alys_governance_message_buffer_size",
        "Number of buffered messages during disconnection",
        &["endpoint", "message_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_RECONNECT_ATTEMPTS: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_reconnect_attempts_total",
        "Total governance reconnection attempts",
        &["endpoint", "reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_REQUEST_CORRELATION: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_governance_request_correlation_duration_seconds",
            "Time from request to correlated response"
        ).buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0]),
        &["request_type", "endpoint"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref FEDERATION_UPDATE_PROCESSING_TIME: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_federation_update_processing_duration_seconds",
            "Time to process federation updates"
        ).buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]),
        &["update_type", "processing_stage"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === StreamActor Message Flow Metrics ===
    pub static ref GOVERNANCE_MESSAGES_SENT: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_messages_sent_total",
        "Total messages sent to governance nodes",
        &["endpoint", "message_type", "stream_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_MESSAGES_RECEIVED: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_messages_received_total",
        "Total messages received from governance nodes",
        &["endpoint", "message_type", "stream_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_MESSAGE_ERRORS: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_message_errors_total",
        "Total governance message processing errors",
        &["endpoint", "error_type", "message_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_MESSAGE_DROPPED: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_messages_dropped_total",
        "Total messages dropped due to buffer overflow",
        &["endpoint", "message_type", "reason"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === StreamActor Health & Quality Metrics ===
    pub static ref GOVERNANCE_ENDPOINT_HEALTH: GaugeVec = register_gauge_vec_with_registry!(
        "alys_governance_endpoint_health_score",
        "Health score for governance endpoints (0.0 to 1.0)",
        &["endpoint"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_STREAM_QUALITY: GaugeVec = register_gauge_vec_with_registry!(
        "alys_governance_stream_quality_score",
        "Stream quality score based on latency, errors, and throughput",
        &["endpoint", "stream_type"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_SIGNATURE_CORRELATION_RATE: GaugeVec = register_gauge_vec_with_registry!(
        "alys_governance_signature_correlation_rate",
        "Rate of successful signature request/response correlations",
        &["endpoint"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    // === StreamActor Advanced Tracking Metrics ===
    pub static ref GOVERNANCE_HEARTBEAT_RTT: HistogramVec = register_histogram_vec_with_registry!(
        HistogramOpts::new(
            "alys_governance_heartbeat_rtt_seconds",
            "Round-trip time for governance heartbeats"
        ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
        &["endpoint"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_BACKPRESSURE_EVENTS: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_backpressure_events_total",
        "Total backpressure events during message sending",
        &["endpoint", "severity"],
        ALYS_REGISTRY
    )
    .unwrap();
    
    pub static ref GOVERNANCE_STREAM_INTERRUPTIONS: CounterVec = register_counter_vec_with_registry!(
        "alys_governance_stream_interruptions_total",
        "Total stream interruption events",
        &["endpoint", "interruption_type"],
        ALYS_REGISTRY
    )
    .unwrap();
}

/// StreamActor connection state for metrics tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamConnectionState {
    Disconnected = 0,
    Connected = 1,
    Authenticated = 2,
    Streaming = 3,
}

impl StreamConnectionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            StreamConnectionState::Disconnected => "disconnected",
            StreamConnectionState::Connected => "connected",
            StreamConnectionState::Authenticated => "authenticated",
            StreamConnectionState::Streaming => "streaming",
        }
    }
    
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }
}

/// Message correlation tracking entry
#[derive(Debug, Clone)]
pub struct MessageCorrelation {
    pub request_id: String,
    pub request_type: String,
    pub endpoint: String,
    pub sent_at: Instant,
    pub timeout: Duration,
}

/// Stream quality metrics aggregation
#[derive(Debug, Default)]
pub struct StreamQualityMetrics {
    pub message_count: u64,
    pub error_count: u64,
    pub total_latency: Duration,
    pub connection_uptime: Duration,
    pub last_quality_calculation: Instant,
}

impl StreamQualityMetrics {
    /// Calculate stream quality score (0.0 to 1.0)
    pub fn calculate_quality_score(&self) -> f64 {
        if self.message_count == 0 {
            return 0.5; // Neutral score for inactive streams
        }
        
        // Error rate score (lower is better)
        let error_rate = self.error_count as f64 / self.message_count as f64;
        let error_score = (1.0 - error_rate.min(1.0)).max(0.0);
        
        // Latency score (lower is better)
        let avg_latency_ms = self.total_latency.as_millis() as f64 / self.message_count as f64;
        let latency_score = if avg_latency_ms < 100.0 {
            1.0
        } else if avg_latency_ms < 1000.0 {
            1.0 - (avg_latency_ms - 100.0) / 900.0 * 0.5
        } else {
            0.5 - (avg_latency_ms - 1000.0) / 10000.0 * 0.5
        }.max(0.0);
        
        // Uptime score
        let uptime_score = if self.connection_uptime.as_secs() > 300 {
            1.0
        } else {
            self.connection_uptime.as_secs() as f64 / 300.0
        };
        
        // Weighted average: error (50%), latency (30%), uptime (20%)
        0.5 * error_score + 0.3 * latency_score + 0.2 * uptime_score
    }
    
    /// Update metrics with new message
    pub fn record_message(&mut self, latency: Duration, has_error: bool) {
        self.message_count += 1;
        if has_error {
            self.error_count += 1;
        }
        self.total_latency += latency;
    }
    
    /// Reset metrics for new quality calculation period
    pub fn reset_for_new_period(&mut self) {
        self.message_count = 0;
        self.error_count = 0;
        self.total_latency = Duration::ZERO;
        self.last_quality_calculation = Instant::now();
    }
}

/// Enhanced StreamActor metrics collector
pub struct StreamActorMetricsCollector {
    /// Message correlation tracking
    pending_correlations: Arc<RwLock<HashMap<String, MessageCorrelation>>>,
    /// Stream quality metrics per endpoint
    stream_quality_metrics: Arc<RwLock<HashMap<String, StreamQualityMetrics>>>,
    /// Connection establishment times
    connection_times: Arc<RwLock<HashMap<String, Instant>>>,
    /// Heartbeat tracking
    heartbeat_tracking: Arc<RwLock<HashMap<String, Instant>>>,
    /// Quality calculation interval
    quality_calculation_interval: Duration,
}

impl StreamActorMetricsCollector {
    /// Create a new StreamActor metrics collector
    pub fn new() -> Self {
        Self {
            pending_correlations: Arc::new(RwLock::new(HashMap::new())),
            stream_quality_metrics: Arc::new(RwLock::new(HashMap::new())),
            connection_times: Arc::new(RwLock::new(HashMap::new())),
            heartbeat_tracking: Arc::new(RwLock::new(HashMap::new())),
            quality_calculation_interval: Duration::from_secs(60),
        }
    }
    
    /// Record connection state change
    pub fn record_connection_state_change(&self, endpoint: &str, node_id: &str, state: StreamConnectionState) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_node_id = MetricLabels::sanitize_label_value(node_id);
        
        GOVERNANCE_CONNECTION_STATUS
            .with_label_values(&[&sanitized_endpoint, &sanitized_node_id])
            .set(state.as_i64());
        
        // Track connection establishment time for latency calculation
        if state == StreamConnectionState::Connected {
            self.connection_times.write().insert(endpoint.to_string(), Instant::now());
        }
        
        info!(
            endpoint = endpoint,
            node_id = node_id,
            state = ?state,
            "StreamActor connection state changed"
        );
    }
    
    /// Record connection establishment latency
    pub fn record_connection_latency(&self, endpoint: &str, duration: Duration) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        
        GOVERNANCE_CONNECTION_LATENCY
            .with_label_values(&[&sanitized_endpoint])
            .observe(duration.as_secs_f64());
        
        debug!(
            endpoint = endpoint,
            latency_ms = duration.as_millis(),
            "Connection latency recorded"
        );
    }
    
    /// Record message buffered during disconnection
    pub fn record_message_buffered(&self, endpoint: &str, message_type: &str, buffer_size: usize) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_message_type = MetricLabels::sanitize_label_value(message_type);
        
        GOVERNANCE_MESSAGE_BUFFER_SIZE
            .with_label_values(&[&sanitized_endpoint, &sanitized_message_type])
            .set(buffer_size as i64);
        
        if buffer_size > 100 {
            warn!(
                endpoint = endpoint,
                message_type = message_type,
                buffer_size = buffer_size,
                "High message buffer size detected"
            );
        }
    }
    
    /// Record reconnection attempt
    pub fn record_reconnection_attempt(&self, endpoint: &str, reason: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_reason = MetricLabels::sanitize_label_value(reason);
        
        GOVERNANCE_RECONNECT_ATTEMPTS
            .with_label_values(&[&sanitized_endpoint, &sanitized_reason])
            .inc();
        
        info!(
            endpoint = endpoint,
            reason = reason,
            "Governance reconnection attempt recorded"
        );
    }
    
    /// Start request correlation tracking
    pub fn start_request_correlation(&self, request_id: &str, request_type: &str, endpoint: &str, timeout: Duration) {
        let correlation = MessageCorrelation {
            request_id: request_id.to_string(),
            request_type: request_type.to_string(),
            endpoint: endpoint.to_string(),
            sent_at: Instant::now(),
            timeout,
        };
        
        self.pending_correlations.write().insert(request_id.to_string(), correlation);
        
        trace!(
            request_id = request_id,
            request_type = request_type,
            endpoint = endpoint,
            "Started request correlation tracking"
        );
    }
    
    /// Complete request correlation tracking
    pub fn complete_request_correlation(&self, request_id: &str) -> Option<Duration> {
        let correlation = self.pending_correlations.write().remove(request_id)?;
        let duration = correlation.sent_at.elapsed();
        
        let sanitized_request_type = MetricLabels::sanitize_label_value(&correlation.request_type);
        let sanitized_endpoint = MetricLabels::sanitize_label_value(&correlation.endpoint);
        
        GOVERNANCE_REQUEST_CORRELATION
            .with_label_values(&[&sanitized_request_type, &sanitized_endpoint])
            .observe(duration.as_secs_f64());
        
        // Update stream quality metrics
        {
            let mut quality_metrics = self.stream_quality_metrics.write();
            let endpoint_metrics = quality_metrics
                .entry(correlation.endpoint.clone())
                .or_insert_with(StreamQualityMetrics::default);
            endpoint_metrics.record_message(duration, false);
        }
        
        debug!(
            request_id = request_id,
            request_type = correlation.request_type,
            endpoint = correlation.endpoint,
            duration_ms = duration.as_millis(),
            "Request correlation completed"
        );
        
        Some(duration)
    }
    
    /// Record federation update processing
    pub fn record_federation_update_processing(&self, update_type: &str, processing_stage: &str, duration: Duration) {
        let sanitized_update_type = MetricLabels::sanitize_label_value(update_type);
        let sanitized_processing_stage = MetricLabels::sanitize_label_value(processing_stage);
        
        FEDERATION_UPDATE_PROCESSING_TIME
            .with_label_values(&[&sanitized_update_type, &sanitized_processing_stage])
            .observe(duration.as_secs_f64());
        
        debug!(
            update_type = update_type,
            processing_stage = processing_stage,
            duration_ms = duration.as_millis(),
            "Federation update processing recorded"
        );
    }
    
    /// Record message sent to governance node
    pub fn record_message_sent(&self, endpoint: &str, message_type: &str, stream_type: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_message_type = MetricLabels::sanitize_label_value(message_type);
        let sanitized_stream_type = MetricLabels::sanitize_label_value(stream_type);
        
        GOVERNANCE_MESSAGES_SENT
            .with_label_values(&[&sanitized_endpoint, &sanitized_message_type, &sanitized_stream_type])
            .inc();
        
        trace!(
            endpoint = endpoint,
            message_type = message_type,
            stream_type = stream_type,
            "Message sent to governance node"
        );
    }
    
    /// Record message received from governance node
    pub fn record_message_received(&self, endpoint: &str, message_type: &str, stream_type: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_message_type = MetricLabels::sanitize_label_value(message_type);
        let sanitized_stream_type = MetricLabels::sanitize_label_value(stream_type);
        
        GOVERNANCE_MESSAGES_RECEIVED
            .with_label_values(&[&sanitized_endpoint, &sanitized_message_type, &sanitized_stream_type])
            .inc();
        
        trace!(
            endpoint = endpoint,
            message_type = message_type,
            stream_type = stream_type,
            "Message received from governance node"
        );
    }
    
    /// Record governance message error
    pub fn record_message_error(&self, endpoint: &str, error_type: &str, message_type: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_error_type = MetricLabels::sanitize_label_value(error_type);
        let sanitized_message_type = MetricLabels::sanitize_label_value(message_type);
        
        GOVERNANCE_MESSAGE_ERRORS
            .with_label_values(&[&sanitized_endpoint, &sanitized_error_type, &sanitized_message_type])
            .inc();
        
        // Update stream quality metrics with error
        {
            let mut quality_metrics = self.stream_quality_metrics.write();
            let endpoint_metrics = quality_metrics
                .entry(endpoint.to_string())
                .or_insert_with(StreamQualityMetrics::default);
            endpoint_metrics.record_message(Duration::ZERO, true);
        }
        
        warn!(
            endpoint = endpoint,
            error_type = error_type,
            message_type = message_type,
            "Governance message error recorded"
        );
    }
    
    /// Record message dropped due to buffer overflow
    pub fn record_message_dropped(&self, endpoint: &str, message_type: &str, reason: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_message_type = MetricLabels::sanitize_label_value(message_type);
        let sanitized_reason = MetricLabels::sanitize_label_value(reason);
        
        GOVERNANCE_MESSAGE_DROPPED
            .with_label_values(&[&sanitized_endpoint, &sanitized_message_type, &sanitized_reason])
            .inc();
        
        error!(
            endpoint = endpoint,
            message_type = message_type,
            reason = reason,
            "Message dropped due to buffer overflow"
        );
    }
    
    /// Record heartbeat round-trip time
    pub fn record_heartbeat_rtt(&self, endpoint: &str, duration: Duration) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        
        GOVERNANCE_HEARTBEAT_RTT
            .with_label_values(&[&sanitized_endpoint])
            .observe(duration.as_secs_f64());
        
        // Track heartbeat timing for health calculations
        self.heartbeat_tracking.write().insert(endpoint.to_string(), Instant::now());
        
        trace!(
            endpoint = endpoint,
            rtt_ms = duration.as_millis(),
            "Heartbeat RTT recorded"
        );
    }
    
    /// Record backpressure event
    pub fn record_backpressure_event(&self, endpoint: &str, severity: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_severity = MetricLabels::sanitize_label_value(severity);
        
        GOVERNANCE_BACKPRESSURE_EVENTS
            .with_label_values(&[&sanitized_endpoint, &sanitized_severity])
            .inc();
        
        warn!(
            endpoint = endpoint,
            severity = severity,
            "Backpressure event recorded"
        );
    }
    
    /// Record stream interruption
    pub fn record_stream_interruption(&self, endpoint: &str, interruption_type: &str) {
        let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
        let sanitized_interruption_type = MetricLabels::sanitize_label_value(interruption_type);
        
        GOVERNANCE_STREAM_INTERRUPTIONS
            .with_label_values(&[&sanitized_endpoint, &sanitized_interruption_type])
            .inc();
        
        warn!(
            endpoint = endpoint,
            interruption_type = interruption_type,
            "Stream interruption recorded"
        );
    }
    
    /// Calculate and update endpoint health scores
    pub fn calculate_endpoint_health(&self) {
        let quality_metrics = self.stream_quality_metrics.read();
        let heartbeat_tracking = self.heartbeat_tracking.read();
        let connection_times = self.connection_times.read();
        
        for (endpoint, metrics) in quality_metrics.iter() {
            let mut health_score = 0.0;
            
            // Base quality score from stream metrics (60% weight)
            let quality_score = metrics.calculate_quality_score();
            health_score += 0.6 * quality_score;
            
            // Heartbeat health (20% weight)
            let heartbeat_health = if let Some(last_heartbeat) = heartbeat_tracking.get(endpoint) {
                let time_since_heartbeat = last_heartbeat.elapsed();
                if time_since_heartbeat < Duration::from_secs(60) {
                    1.0
                } else if time_since_heartbeat < Duration::from_secs(300) {
                    0.5
                } else {
                    0.0
                }
            } else {
                0.5 // Neutral if no heartbeat data
            };
            health_score += 0.2 * heartbeat_health;
            
            // Connection stability (20% weight)
            let connection_stability = if let Some(connection_start) = connection_times.get(endpoint) {
                let uptime = connection_start.elapsed();
                if uptime > Duration::from_secs(3600) {
                    1.0
                } else {
                    uptime.as_secs() as f64 / 3600.0
                }
            } else {
                0.0 // No connection
            };
            health_score += 0.2 * connection_stability;
            
            let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
            GOVERNANCE_ENDPOINT_HEALTH
                .with_label_values(&[&sanitized_endpoint])
                .set(health_score);
            
            debug!(
                endpoint = endpoint,
                health_score = %format!("{:.3}", health_score),
                quality_score = %format!("{:.3}", quality_score),
                heartbeat_health = %format!("{:.3}", heartbeat_health),
                connection_stability = %format!("{:.3}", connection_stability),
                "Endpoint health calculated"
            );
        }
    }
    
    /// Update stream quality scores
    pub fn update_stream_quality_scores(&self) {
        let mut quality_metrics = self.stream_quality_metrics.write();
        
        for (endpoint, metrics) in quality_metrics.iter_mut() {
            let quality_score = metrics.calculate_quality_score();
            
            let sanitized_endpoint = MetricLabels::sanitize_label_value(endpoint);
            
            // Update quality score for different stream types
            for stream_type in &["consensus", "federation", "chain_data", "proposals", "attestations"] {
                GOVERNANCE_STREAM_QUALITY
                    .with_label_values(&[&sanitized_endpoint, stream_type])
                    .set(quality_score);
            }
            
            // Calculate signature correlation rate
            let correlation_rate = if metrics.message_count > 0 {
                1.0 - (metrics.error_count as f64 / metrics.message_count as f64)
            } else {
                0.5
            };
            
            GOVERNANCE_SIGNATURE_CORRELATION_RATE
                .with_label_values(&[&sanitized_endpoint])
                .set(correlation_rate);
            
            trace!(
                endpoint = endpoint,
                quality_score = %format!("{:.3}", quality_score),
                correlation_rate = %format!("{:.3}", correlation_rate),
                message_count = metrics.message_count,
                error_count = metrics.error_count,
                "Stream quality score updated"
            );
            
            // Reset metrics for next calculation period if enough time has passed
            if metrics.last_quality_calculation.elapsed() >= self.quality_calculation_interval {
                metrics.reset_for_new_period();
            }
        }
    }
    
    /// Start periodic health and quality calculations
    pub async fn start_periodic_calculations(&self) -> tokio::task::JoinHandle<()> {
        let collector = Arc::new(self.clone());
        let calculation_interval = self.quality_calculation_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(calculation_interval);
            
            info!(
                calculation_interval_secs = calculation_interval.as_secs(),
                "Starting StreamActor periodic metrics calculations"
            );
            
            loop {
                interval.tick().await;
                
                let calculation_start = Instant::now();
                
                // Calculate endpoint health scores
                collector.calculate_endpoint_health();
                
                // Update stream quality scores
                collector.update_stream_quality_scores();
                
                // Clean up expired correlations
                collector.cleanup_expired_correlations().await;
                
                let calculation_duration = calculation_start.elapsed();
                
                trace!(
                    calculation_duration_ms = calculation_duration.as_millis(),
                    "StreamActor periodic calculations completed"
                );
                
                if calculation_duration > Duration::from_secs(5) {
                    warn!(
                        calculation_duration_ms = calculation_duration.as_millis(),
                        "StreamActor metrics calculations taking too long"
                    );
                }
            }
        })
    }
    
    /// Clean up expired correlation tracking entries
    async fn cleanup_expired_correlations(&self) {
        let mut expired_correlations = Vec::new();
        
        {
            let correlations = self.pending_correlations.read();
            for (request_id, correlation) in correlations.iter() {
                if correlation.sent_at.elapsed() > correlation.timeout {
                    expired_correlations.push(request_id.clone());
                }
            }
        }
        
        if !expired_correlations.is_empty() {
            let mut correlations = self.pending_correlations.write();
            for request_id in &expired_correlations {
                if let Some(correlation) = correlations.remove(request_id) {
                    warn!(
                        request_id = request_id,
                        request_type = correlation.request_type,
                        endpoint = correlation.endpoint,
                        elapsed_secs = correlation.sent_at.elapsed().as_secs(),
                        timeout_secs = correlation.timeout.as_secs(),
                        "Request correlation timed out"
                    );
                    
                    // Record timeout as an error
                    self.record_message_error(&correlation.endpoint, "timeout", &correlation.request_type);
                }
            }
            
            info!(
                expired_count = expired_correlations.len(),
                "Cleaned up expired correlation tracking entries"
            );
        }
    }
    
    /// Get comprehensive StreamActor metrics summary
    pub fn get_metrics_summary(&self) -> serde_json::Value {
        let correlations_count = self.pending_correlations.read().len();
        let quality_metrics = self.stream_quality_metrics.read();
        let connection_times = self.connection_times.read();
        let heartbeat_tracking = self.heartbeat_tracking.read();
        
        let mut endpoint_summaries = serde_json::Map::new();
        
        for (endpoint, metrics) in quality_metrics.iter() {
            let connection_uptime = connection_times
                .get(endpoint)
                .map(|start| start.elapsed().as_secs())
                .unwrap_or(0);
            
            let last_heartbeat = heartbeat_tracking
                .get(endpoint)
                .map(|last| last.elapsed().as_secs())
                .unwrap_or(u64::MAX);
            
            let endpoint_summary = serde_json::json!({
                "message_count": metrics.message_count,
                "error_count": metrics.error_count,
                "quality_score": metrics.calculate_quality_score(),
                "connection_uptime_secs": connection_uptime,
                "last_heartbeat_secs_ago": last_heartbeat,
                "avg_latency_ms": if metrics.message_count > 0 {
                    metrics.total_latency.as_millis() as f64 / metrics.message_count as f64
                } else {
                    0.0
                }
            });
            
            endpoint_summaries.insert(endpoint.clone(), endpoint_summary);
        }
        
        serde_json::json!({
            "pending_correlations": correlations_count,
            "tracked_endpoints": quality_metrics.len(),
            "calculation_interval_secs": self.quality_calculation_interval.as_secs(),
            "endpoints": endpoint_summaries
        })
    }
}

impl Clone for StreamActorMetricsCollector {
    fn clone(&self) -> Self {
        Self {
            pending_correlations: self.pending_correlations.clone(),
            stream_quality_metrics: self.stream_quality_metrics.clone(),
            connection_times: self.connection_times.clone(),
            heartbeat_tracking: self.heartbeat_tracking.clone(),
            quality_calculation_interval: self.quality_calculation_interval,
        }
    }
}

impl Default for StreamActorMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize StreamActor metrics
pub fn initialize_stream_actor_metrics() -> Result<(), Box<dyn std::error::Error>> {
    info!("Initializing StreamActor V2 enhanced metrics");
    
    // Test metric registration by accessing lazy statics
    let _test_access = [
        GOVERNANCE_CONNECTION_STATUS.clone(),
        GOVERNANCE_MESSAGE_BUFFER_SIZE.clone(),
        GOVERNANCE_REQUEST_CORRELATION.clone(),
        FEDERATION_UPDATE_PROCESSING_TIME.clone(),
    ];
    
    info!("StreamActor V2 metrics initialization completed");
    info!("Available StreamActor metrics: Connection Status, Message Buffering, Request Correlation, Federation Processing, Health Scores");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[test]
    fn test_stream_quality_metrics_calculation() {
        let mut metrics = StreamQualityMetrics::default();
        
        // Record some messages
        metrics.record_message(Duration::from_millis(50), false);
        metrics.record_message(Duration::from_millis(100), false);
        metrics.record_message(Duration::from_millis(75), true); // with error
        
        let quality_score = metrics.calculate_quality_score();
        
        // Should have a quality score between 0.0 and 1.0
        assert!(quality_score >= 0.0 && quality_score <= 1.0);
        assert_eq!(metrics.message_count, 3);
        assert_eq!(metrics.error_count, 1);
    }
    
    #[tokio::test]
    async fn test_request_correlation_tracking() {
        let collector = StreamActorMetricsCollector::new();
        
        let request_id = "test-request-123";
        let request_type = "signature_request";
        let endpoint = "governance-node-1:50051";
        
        // Start correlation tracking
        collector.start_request_correlation(
            request_id,
            request_type,
            endpoint,
            Duration::from_secs(30)
        );
        
        // Verify correlation is tracked
        assert_eq!(collector.pending_correlations.read().len(), 1);
        
        // Simulate some processing time
        sleep(Duration::from_millis(10)).await;
        
        // Complete correlation
        let duration = collector.complete_request_correlation(request_id);
        assert!(duration.is_some());
        assert!(duration.unwrap() >= Duration::from_millis(10));
        
        // Verify correlation is removed
        assert_eq!(collector.pending_correlations.read().len(), 0);
    }
    
    #[test]
    fn test_stream_connection_state() {
        assert_eq!(StreamConnectionState::Disconnected.as_i64(), 0);
        assert_eq!(StreamConnectionState::Connected.as_i64(), 1);
        assert_eq!(StreamConnectionState::Authenticated.as_i64(), 2);
        assert_eq!(StreamConnectionState::Streaming.as_i64(), 3);
        
        assert_eq!(StreamConnectionState::Streaming.as_str(), "streaming");
    }
    
    #[tokio::test]
    async fn test_metrics_collector_initialization() {
        let collector = StreamActorMetricsCollector::new();
        
        // Test basic functionality
        collector.record_connection_state_change(
            "test-endpoint",
            "test-node",
            StreamConnectionState::Connected
        );
        
        collector.record_message_sent("test-endpoint", "heartbeat", "consensus");
        collector.record_message_received("test-endpoint", "heartbeat_response", "consensus");
        
        let summary = collector.get_metrics_summary();
        assert!(summary.is_object());
        assert_eq!(summary["pending_correlations"], 0);
    }
}