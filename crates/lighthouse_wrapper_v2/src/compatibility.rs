use crate::{CompatConfig, LighthouseVersion, MigrationMode, MetricsRecorder, LighthouseResult};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct LighthouseCompat {
    version: LighthouseVersion,
    migration_mode: MigrationMode,
    metrics: Arc<MetricsRecorder>,
    config: CompatConfig,
}

impl LighthouseCompat {
    pub fn new(config: CompatConfig) -> LighthouseResult<Self> {
        let metrics = Arc::new(MetricsRecorder::new());
        
        Ok(Self {
            version: config.default_version.clone(),
            migration_mode: config.migration_mode.clone(),
            metrics,
            config,
        })
    }
    
    pub fn set_migration_mode(&mut self, mode: MigrationMode) {
        self.migration_mode = mode;
        
        match &mode {
            MigrationMode::Canary(percent) => {
                self.metrics.update_traffic_split(*percent as f64);
            }
            MigrationMode::V4Only => {
                self.metrics.update_traffic_split(0.0);
            }
            MigrationMode::V5Only => {
                self.metrics.update_traffic_split(100.0);
            }
            _ => {}
        }
    }
    
    pub fn get_migration_mode(&self) -> &MigrationMode {
        &self.migration_mode
    }
    
    pub fn get_metrics(&self) -> Arc<MetricsRecorder> {
        self.metrics.clone()
    }
    
    pub async fn execute_with_comparison<T, F, R>(
        &self,
        operation: &str,
        v4_op: F,
        v5_op: F,
    ) -> LighthouseResult<R>
    where
        F: std::future::Future<Output = LighthouseResult<R>> + Send,
        R: PartialEq + std::fmt::Debug + Clone,
    {
        let v4_start = Instant::now();
        let v4_future = v4_op;
        
        let v5_start = Instant::now();
        let v5_future = v5_op;
        
        // Execute both in parallel
        let (v4_result, v5_result) = tokio::join!(v4_future, v5_future);
        
        let v4_duration = v4_start.elapsed();
        let v5_duration = v5_start.elapsed();
        
        // Record metrics
        self.record_operation_time(operation, "v4", v4_duration);
        self.record_operation_time(operation, "v5", v5_duration);
        
        // Compare results
        match (&v4_result, &v5_result) {
            (Ok(v4_val), Ok(v5_val)) => {
                if v4_val == v5_val {
                    self.record_match(operation);
                } else {
                    self.record_mismatch(operation);
                    tracing::warn!("Result mismatch in {}: v4={:?}, v5={:?}", 
                        operation, v4_val, v5_val);
                }
            }
            (Ok(_), Err(e)) => {
                self.record_v5_only_error(operation);
                tracing::warn!("V5 failed while V4 succeeded in {}: {}", operation, e);
            }
            (Err(e), Ok(_)) => {
                self.record_v4_only_error(operation);
                tracing::warn!("V4 failed while V5 succeeded in {}: {}", operation, e);
            }
            (Err(e4), Err(e5)) => {
                self.record_both_errors(operation);
                tracing::error!("Both versions failed in {}: v4={}, v5={}", 
                    operation, e4, e5);
            }
        }
        
        // Return v4 result during parallel testing
        v4_result
    }
    
    fn record_operation_time(&self, operation: &str, version: &str, duration: Duration) {
        tracing::debug!("Operation {} ({}): {:?}", operation, version, duration);
        // Record to metrics
        match operation {
            "engine_api" => self.metrics.record_engine_api_duration(duration),
            "payload_build" => {
                if duration < Duration::from_secs(5) {
                    self.metrics.record_payload_build_success(duration);
                }
            }
            "bls_signature" => self.metrics.record_bls_verification(duration, true),
            _ => {}
        }
    }
    
    fn record_match(&self, operation: &str) {
        tracing::debug!("Operation {} results match between v4 and v5", operation);
    }
    
    fn record_mismatch(&self, operation: &str) {
        tracing::warn!("Operation {} results mismatch between v4 and v5", operation);
        self.metrics.record_version_mismatch();
    }
    
    fn record_v4_only_error(&self, operation: &str) {
        tracing::warn!("Operation {} failed only in v4", operation);
    }
    
    fn record_v5_only_error(&self, operation: &str) {
        tracing::warn!("Operation {} failed only in v5", operation);
    }
    
    fn record_both_errors(&self, operation: &str) {
        tracing::error!("Operation {} failed in both v4 and v5", operation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_compatibility_layer_creation() {
        let config = CompatConfig::default();
        let compat = LighthouseCompat::new(config).unwrap();
        assert!(matches!(compat.migration_mode, MigrationMode::V4Only));
    }
    
    #[tokio::test] 
    async fn test_migration_mode_switching() {
        let config = CompatConfig::default();
        let mut compat = LighthouseCompat::new(config).unwrap();
        
        compat.set_migration_mode(MigrationMode::V5Only);
        assert!(matches!(compat.migration_mode, MigrationMode::V5Only));
        
        compat.set_migration_mode(MigrationMode::Canary(50));
        assert!(matches!(compat.migration_mode, MigrationMode::Canary(50)));
    }
}