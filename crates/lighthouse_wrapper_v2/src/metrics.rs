use prometheus::{
    Counter, Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, 
    Opts, Registry, register_counter, register_gauge, register_histogram, register_int_counter, register_int_gauge,
};
use std::time::{Duration, Instant};
use lazy_static::lazy_static;

lazy_static! {
    // Engine API metrics
    pub static ref ENGINE_API_REQUESTS: IntCounter = register_int_counter!(
        "lighthouse_engine_api_requests_total",
        "Total number of Engine API requests"
    ).unwrap();
    
    pub static ref ENGINE_API_REQUEST_DURATION: Histogram = register_histogram!(
        HistogramOpts::new(
            "lighthouse_engine_api_request_duration_seconds",
            "Duration of Engine API requests in seconds"
        )
    ).unwrap();
    
    pub static ref ENGINE_API_ERRORS: IntCounter = register_int_counter!(
        "lighthouse_engine_api_errors_total",
        "Total number of Engine API errors"
    ).unwrap();
    
    // Payload building metrics
    pub static ref PAYLOAD_BUILD_SUCCESS: IntCounter = register_int_counter!(
        "lighthouse_payload_build_success_total",
        "Total number of successful payload builds"
    ).unwrap();
    
    pub static ref PAYLOAD_BUILD_FAILURES: IntCounter = register_int_counter!(
        "lighthouse_payload_build_failures_total",
        "Total number of failed payload builds"
    ).unwrap();
    
    pub static ref PAYLOAD_BUILD_DURATION: Histogram = register_histogram!(
        HistogramOpts::new(
            "lighthouse_payload_build_duration_seconds",
            "Duration of payload building in seconds"
        ).with_buckets(vec![0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0])
    ).unwrap();
    
    // Version compatibility metrics
    pub static ref V4_OPERATIONS: IntCounter = register_int_counter!(
        "lighthouse_v4_operations_total",
        "Total operations using Lighthouse v4"
    ).unwrap();
    
    pub static ref V5_OPERATIONS: IntCounter = register_int_counter!(
        "lighthouse_v5_operations_total",
        "Total operations using Lighthouse v5"
    ).unwrap();
    
    pub static ref VERSION_MISMATCHES: IntCounter = register_int_counter!(
        "lighthouse_version_mismatches_total",
        "Total version comparison mismatches"
    ).unwrap();
    
    pub static ref ACTIVE_VERSION: IntGauge = register_int_gauge!(
        "lighthouse_active_version",
        "Currently active Lighthouse version (4 or 5)"
    ).unwrap();
    
    // Migration metrics
    pub static ref MIGRATION_PROGRESS: Gauge = register_gauge!(
        "lighthouse_migration_progress_percent",
        "Migration progress percentage"
    ).unwrap();
    
    pub static ref TRAFFIC_SPLIT_V5: Gauge = register_gauge!(
        "lighthouse_traffic_split_v5_percent",
        "Percentage of traffic routed to v5"
    ).unwrap();
    
    pub static ref ROLLBACK_COUNT: IntCounter = register_int_counter!(
        "lighthouse_rollback_count_total",
        "Total number of rollbacks executed"
    ).unwrap();
    
    // BLS signature metrics
    pub static ref BLS_SIGNATURE_VERIFICATIONS: IntCounter = register_int_counter!(
        "lighthouse_bls_signature_verifications_total",
        "Total BLS signature verifications"
    ).unwrap();
    
    pub static ref BLS_SIGNATURE_ERRORS: IntCounter = register_int_counter!(
        "lighthouse_bls_signature_errors_total",
        "Total BLS signature verification errors"
    ).unwrap();
    
    pub static ref BLS_SIGNATURE_DURATION: Histogram = register_histogram!(
        HistogramOpts::new(
            "lighthouse_bls_signature_duration_seconds",
            "Duration of BLS signature operations"
        ).with_buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25])
    ).unwrap();
    
    // Storage metrics
    pub static ref STORAGE_OPERATIONS: IntCounter = register_int_counter!(
        "lighthouse_storage_operations_total",
        "Total storage operations"
    ).unwrap();
    
    pub static ref STORAGE_ERRORS: IntCounter = register_int_counter!(
        "lighthouse_storage_errors_total",
        "Total storage operation errors"
    ).unwrap();
    
    pub static ref STORAGE_SIZE_BYTES: IntGauge = register_int_gauge!(
        "lighthouse_storage_size_bytes",
        "Storage size in bytes"
    ).unwrap();
}

pub struct MetricsRecorder {
    start_time: Instant,
}

impl MetricsRecorder {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }
    
    pub fn record_engine_api_request(&self) {
        ENGINE_API_REQUESTS.inc();
    }
    
    pub fn record_engine_api_duration(&self, duration: Duration) {
        ENGINE_API_REQUEST_DURATION.observe(duration.as_secs_f64());
    }
    
    pub fn record_engine_api_error(&self) {
        ENGINE_API_ERRORS.inc();
    }
    
    pub fn record_payload_build_success(&self, duration: Duration) {
        PAYLOAD_BUILD_SUCCESS.inc();
        PAYLOAD_BUILD_DURATION.observe(duration.as_secs_f64());
    }
    
    pub fn record_payload_build_failure(&self) {
        PAYLOAD_BUILD_FAILURES.inc();
    }
    
    pub fn record_v4_operation(&self) {
        V4_OPERATIONS.inc();
        ACTIVE_VERSION.set(4);
    }
    
    pub fn record_v5_operation(&self) {
        V5_OPERATIONS.inc();
        ACTIVE_VERSION.set(5);
    }
    
    pub fn record_version_mismatch(&self) {
        VERSION_MISMATCHES.inc();
    }
    
    pub fn update_migration_progress(&self, percent: f64) {
        MIGRATION_PROGRESS.set(percent);
    }
    
    pub fn update_traffic_split(&self, v5_percent: f64) {
        TRAFFIC_SPLIT_V5.set(v5_percent);
    }
    
    pub fn record_rollback(&self) {
        ROLLBACK_COUNT.inc();
    }
    
    pub fn record_bls_verification(&self, duration: Duration, success: bool) {
        BLS_SIGNATURE_VERIFICATIONS.inc();
        BLS_SIGNATURE_DURATION.observe(duration.as_secs_f64());
        
        if !success {
            BLS_SIGNATURE_ERRORS.inc();
        }
    }
    
    pub fn record_storage_operation(&self, success: bool) {
        STORAGE_OPERATIONS.inc();
        if !success {
            STORAGE_ERRORS.inc();
        }
    }
    
    pub fn update_storage_size(&self, size_bytes: u64) {
        STORAGE_SIZE_BYTES.set(size_bytes as i64);
    }
}

#[derive(Clone)]
pub struct TimedOperation {
    start: Instant,
    name: String,
}

impl TimedOperation {
    pub fn new(name: String) -> Self {
        Self {
            start: Instant::now(),
            name,
        }
    }
    
    pub fn finish(self) -> Duration {
        let duration = self.start.elapsed();
        
        match self.name.as_str() {
            "engine_api" => ENGINE_API_REQUEST_DURATION.observe(duration.as_secs_f64()),
            "payload_build" => PAYLOAD_BUILD_DURATION.observe(duration.as_secs_f64()),
            "bls_signature" => BLS_SIGNATURE_DURATION.observe(duration.as_secs_f64()),
            _ => {}, // Unknown operation
        }
        
        duration
    }
}