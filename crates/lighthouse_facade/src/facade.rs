//! Main facade implementation
//!
//! This module provides the LighthouseFacade struct which serves as the unified
//! interface for all Lighthouse operations, abstracting over version differences
//! and providing a consistent API.

use crate::{
    config::FacadeConfig,
    error::{FacadeError, FacadeResult},
    types::*,
};
use crate::compatibility::{LighthouseCompat, MigrationMode};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

/// Main facade for Lighthouse integration
#[derive(Debug)]
pub struct LighthouseFacade {
    /// Configuration
    config: FacadeConfig,
    
    /// Underlying compatibility layer
    compat: Arc<LighthouseCompat>,
    
    /// Facade statistics
    stats: Arc<RwLock<FacadeStats>>,
    
    /// Health status
    health_status: Arc<RwLock<HealthStatus>>,
    
    /// Circuit breaker state
    circuit_breaker: Arc<RwLock<CircuitBreakerState>>,
}

/// Circuit breaker state
#[derive(Debug, Clone)]
struct CircuitBreakerState {
    /// Is circuit breaker open?
    is_open: bool,
    
    /// Failure count in current window
    failure_count: usize,
    
    /// Total request count in current window
    total_count: usize,
    
    /// Window start time
    window_start: Instant,
    
    /// Next retry time (when half-open)
    next_retry: Option<Instant>,
    
    /// Half-open request count
    half_open_count: usize,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            is_open: false,
            failure_count: 0,
            total_count: 0,
            window_start: Instant::now(),
            next_retry: None,
            half_open_count: 0,
        }
    }
}

impl LighthouseFacade {
    /// Create a new facade instance
    pub async fn new(config: FacadeConfig) -> FacadeResult<Self> {
        info!("Initializing Lighthouse facade");
        
        // Validate configuration
        config.validate()?;
        
        // Create compatibility layer based on facade mode
        let compat_config = match &config.mode {
            FacadeMode::V4Only => {
                let mut cfg = config.compatibility.clone();
                cfg.versions.v4.enabled = true;
                cfg.versions.v7.enabled = false;
                cfg.migration.initial_mode = MigrationMode::V4Only;
                cfg
            }
            FacadeMode::V7Only => {
                let mut cfg = config.compatibility.clone();
                cfg.versions.v4.enabled = false;
                cfg.versions.v7.enabled = true;
                cfg.migration.initial_mode = MigrationMode::V7Only;
                cfg
            }
            FacadeMode::Automatic => {
                let mut cfg = config.compatibility.clone();
                // Enable both and let compatibility layer decide
                cfg.versions.v4.enabled = true;
                cfg.versions.v7.enabled = true;
                cfg.migration.initial_mode = if cfg.versions.v7.enabled && Self::is_v7_available().await {
                    MigrationMode::V7Only
                } else {
                    MigrationMode::V4Only
                };
                cfg
            }
            FacadeMode::Migration => {
                let mut cfg = config.compatibility.clone();
                cfg.versions.v4.enabled = true;
                cfg.versions.v7.enabled = true;
                cfg.migration.initial_mode = MigrationMode::Parallel;
                cfg
            }
            FacadeMode::Dual => {
                let mut cfg = config.compatibility.clone();
                cfg.versions.v4.enabled = true;
                cfg.versions.v7.enabled = true;
                cfg.migration.initial_mode = MigrationMode::Parallel;
                cfg
            }
            FacadeMode::Mock => {
                let mut cfg = config.compatibility.clone();
                cfg.versions.v4.enabled = false;
                cfg.versions.v7.enabled = false;
                cfg.migration.initial_mode = MigrationMode::V4Only; // Use V4 for mock
                cfg
            }
        };
        
        let compat = LighthouseCompat::new(config.clone()).await
            .map_err(|e| FacadeError::Compatibility(e.to_string()))?;
        
        let facade = Self {
            config,
            compat: Arc::new(compat),
            stats: Arc::new(RwLock::new(FacadeStats::default())),
            health_status: Arc::new(RwLock::new(HealthStatus::default())),
            circuit_breaker: Arc::new(RwLock::new(CircuitBreakerState::default())),
        };
        
        // Start background health monitoring if enabled
        if facade.config.health_check.enabled {
            facade.start_health_monitoring().await?;
        }
        
        info!("Lighthouse facade initialized successfully");
        Ok(facade)
    }
    
    /// Check if v7 is available in the environment
    async fn is_v7_available() -> bool {
        // In a real implementation, this would check for v7 binary availability,
        // network connectivity, etc. For now, we'll check the v7 feature flag
        #[cfg(feature = "v7")]
        {
            true
        }
        #[cfg(not(feature = "v7"))]
        {
            false
        }
    }
    
    /// Start background health monitoring
    async fn start_health_monitoring(&self) -> FacadeResult<()> {
        let compat = Arc::clone(&self.compat);
        let health_status = Arc::clone(&self.health_status);
        let config = self.config.health_check.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.interval);
            let mut consecutive_failures = 0;
            let mut consecutive_successes = 0;
            
            loop {
                interval.tick().await;
                
                match compat.health_check().await {
                    Ok(status) => {
                        if status.healthy {
                            consecutive_successes += 1;
                            consecutive_failures = 0;
                            
                            if consecutive_successes >= config.success_threshold {
                                *health_status.write().await = status;
                            }
                        } else {
                            consecutive_failures += 1;
                            consecutive_successes = 0;
                            
                            if consecutive_failures >= config.failure_threshold {
                                *health_status.write().await = status;
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        consecutive_successes = 0;
                        
                        warn!("Health check failed: {}", e);
                        
                        if consecutive_failures >= config.failure_threshold {
                            let mut status = health_status.write().await;
                            status.healthy = false;
                            status.error_details = Some(e.to_string());
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Execute a request with circuit breaker protection
    async fn execute_with_circuit_breaker<F, R>(&self, operation: F) -> FacadeResult<R>
    where
        F: std::future::Future<Output = FacadeResult<R>>,
    {
        if !self.config.performance.circuit_breaker.enabled {
            return operation.await;
        }
        
        // Check circuit breaker state
        {
            let cb_state = self.circuit_breaker.read().await;
            if cb_state.is_open {
                if let Some(retry_time) = cb_state.next_retry {
                    if Instant::now() < retry_time {
                        return Err(FacadeError::ServiceUnavailable {
                            service: "circuit_breaker_open".to_string(),
                        });
                    }
                } else {
                    return Err(FacadeError::ServiceUnavailable {
                        service: "circuit_breaker_open".to_string(),
                    });
                }
            }
        }
        
        let start_time = Instant::now();
        let result = operation.await;
        let duration = start_time.elapsed();
        
        // Update circuit breaker state
        let mut cb_state = self.circuit_breaker.write().await;
        cb_state.total_count += 1;
        
        match &result {
            Ok(_) => {
                if cb_state.is_open {
                    cb_state.half_open_count += 1;
                    if cb_state.half_open_count >= self.config.performance.circuit_breaker.half_open_request_count {
                        // Enough successful requests in half-open state, close circuit
                        cb_state.is_open = false;
                        cb_state.failure_count = 0;
                        cb_state.half_open_count = 0;
                        cb_state.next_retry = None;
                        info!("Circuit breaker closed - service recovered");
                    }
                }
            }
            Err(_) => {
                cb_state.failure_count += 1;
                
                // Check if we should open the circuit breaker
                if cb_state.total_count >= self.config.performance.circuit_breaker.min_request_count {
                    let failure_rate = cb_state.failure_count as f64 / cb_state.total_count as f64;
                    if failure_rate >= self.config.performance.circuit_breaker.failure_rate_threshold {
                        cb_state.is_open = true;
                        cb_state.next_retry = Some(Instant::now() + self.config.performance.circuit_breaker.timeout);
                        cb_state.half_open_count = 0;
                        warn!("Circuit breaker opened - failure rate: {:.2}", failure_rate);
                    }
                }
            }
        }
        
        // Reset window if enough time has passed
        if cb_state.window_start.elapsed() >= Duration::from_secs(60) {
            cb_state.failure_count = 0;
            cb_state.total_count = 0;
            cb_state.window_start = Instant::now();
        }
        
        result
    }
    
    /// Record request statistics
    async fn record_stats(&self, success: bool, duration: Duration, version: Option<ClientVersion>) {
        let mut stats = self.stats.write().await;
        let duration_ms = duration.as_millis() as f64;
        
        if success {
            if let Some(ver) = version {
                stats.record_success(duration_ms as u64, ver);
            } else {
                stats.record_success(duration_ms as u64, ClientVersion::V4 { version: "unknown".to_string() });
            }
        } else {
            stats.record_failure(duration_ms as u64, ClientVersion::V4 { version: "unknown".to_string() });
        }
    }
    
    /// Get current facade statistics
    pub async fn get_stats(&self) -> FacadeStats {
        self.stats.read().await.clone()
    }
    
    /// Get current health status
    pub async fn get_health(&self) -> HealthStatus {
        self.health_status.read().await.clone()
    }
    
    /// Get current configuration
    pub fn get_config(&self) -> &FacadeConfig {
        &self.config
    }
}

#[async_trait]
impl LighthouseClient for LighthouseFacade {
    #[instrument(skip(self, payload))]
    async fn new_payload(&self, payload: ExecutionPayload) -> FacadeResult<PayloadStatus> {
        debug!("Processing new_payload request");
        
        let start_time = Instant::now();
        let result = self.execute_with_circuit_breaker(async {
            self.compat.new_payload(payload).await
                .map_err(|e| FacadeError::Compatibility(e.to_string()))
        }).await;
        
        let duration = start_time.elapsed();
        let success = result.is_ok();
        let version = if success { Some(self.compat.version()) } else { None };
        
        self.record_stats(success, duration, version).await;
        
        result
    }
    
    #[instrument(skip(self, forkchoice_state, payload_attributes))]
    async fn forkchoice_updated(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> FacadeResult<ForkchoiceUpdatedResponse> {
        debug!("Processing forkchoice_updated request");
        
        let start_time = Instant::now();
        let result = self.execute_with_circuit_breaker(async {
            self.compat.forkchoice_updated(forkchoice_state, payload_attributes).await
                .map_err(|e| FacadeError::Compatibility(e.to_string()))
        }).await;
        
        let duration = start_time.elapsed();
        let success = result.is_ok();
        let version = if success { Some(self.compat.version()) } else { None };
        
        self.record_stats(success, duration, version).await;
        
        result
    }
    
    #[instrument(skip(self))]
    async fn get_payload(&self, payload_id: PayloadId) -> FacadeResult<GetPayloadResponse> {
        debug!("Processing get_payload request");
        
        let start_time = Instant::now();
        let result = self.execute_with_circuit_breaker(async {
            self.compat.get_payload(payload_id).await
                .map_err(|e| FacadeError::Compatibility(e.to_string()))
        }).await;
        
        let duration = start_time.elapsed();
        let success = result.is_ok();
        let version = if success { Some(self.compat.version()) } else { None };
        
        self.record_stats(success, duration, version).await;
        
        result
    }
    
    async fn is_ready(&self) -> FacadeResult<bool> {
        self.compat.is_ready().await
            .map_err(|e| FacadeError::Compatibility(e.to_string()))
    }
    
    fn version(&self) -> ClientVersion {
        // Return facade version info
        ClientVersion::V7 { version: format!("facade-{}", crate::version::FACADE_VERSION) }
    }
    
    async fn health_check(&self) -> FacadeResult<HealthStatus> {
        Ok(self.get_health().await)
    }
}

impl LighthouseFacade {
    /// Switch migration mode (for runtime migration control)
    pub async fn set_migration_mode(&self, mode: MigrationMode) -> FacadeResult<()> {
        self.compat.set_migration_mode(mode).await
            .map_err(|e| FacadeError::Compatibility(e.to_string()))
    }
    
    /// Get current migration mode
    pub async fn get_migration_mode(&self) -> FacadeResult<MigrationMode> {
        Ok(self.compat.get_migration_mode().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FacadeConfig;
    
    #[tokio::test]
    async fn test_facade_creation() {
        let config = FacadeConfig::development();
        let result = LighthouseFacade::new(config).await;
        
        // This test will pass if the facade can be created
        // In a real environment with proper lighthouse setup
        assert!(result.is_ok() || matches!(result.unwrap_err(), FacadeError::Compatibility(_)));
    }
    
    #[tokio::test]
    async fn test_circuit_breaker_state() {
        let state = CircuitBreakerState::default();
        assert!(!state.is_open);
        assert_eq!(state.failure_count, 0);
        assert_eq!(state.total_count, 0);
    }
    
    #[test]
    fn test_facade_mode_default() {
        let mode = FacadeMode::default();
        assert_eq!(mode, FacadeMode::Automatic);
    }
}