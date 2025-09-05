//! Health monitoring for Lighthouse facade
//!
//! This module provides health checking and monitoring capabilities for both
//! Lighthouse v4 and v7 clients, including overall facade health status.

use crate::{
    error::{FacadeError, FacadeResult},
    types::{HealthStatus, HealthMetrics, SyncStatus},
    config::HealthCheckConfig,
};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Health monitor for managing overall facade health
#[derive(Debug)]
pub struct HealthMonitor {
    /// Configuration
    config: HealthCheckConfig,
    
    /// V4 client health status
    v4_health: Arc<RwLock<Option<HealthStatus>>>,
    
    /// V7 client health status
    v7_health: Arc<RwLock<Option<HealthStatus>>>,
    
    /// Overall facade health
    overall_health: Arc<RwLock<HealthStatus>>,
    
    /// Health check statistics
    stats: Arc<RwLock<HealthStats>>,
}

/// Health statistics
#[derive(Debug, Clone)]
pub struct HealthStats {
    /// Total health checks performed
    pub total_checks: u64,
    
    /// Successful health checks
    pub successful_checks: u64,
    
    /// Failed health checks
    pub failed_checks: u64,
    
    /// V4 specific stats
    pub v4_stats: ClientHealthStats,
    
    /// V7 specific stats
    pub v7_stats: ClientHealthStats,
    
    /// Last overall health update
    pub last_update: SystemTime,
}

/// Health statistics for a specific client version
#[derive(Debug, Clone)]
pub struct ClientHealthStats {
    /// Total checks for this client
    pub total_checks: u64,
    
    /// Successful checks
    pub successful_checks: u64,
    
    /// Failed checks
    pub failed_checks: u64,
    
    /// Average response time
    pub avg_response_time: Duration,
    
    /// Last successful check
    pub last_success: Option<SystemTime>,
    
    /// Last failure
    pub last_failure: Option<SystemTime>,
    
    /// Consecutive failures
    pub consecutive_failures: u32,
    
    /// Consecutive successes
    pub consecutive_successes: u32,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub async fn new(config: HealthCheckConfig) -> FacadeResult<Self> {
        let monitor = Self {
            config,
            v4_health: Arc::new(RwLock::new(None)),
            v7_health: Arc::new(RwLock::new(None)),
            overall_health: Arc::new(RwLock::new(HealthStatus::default())),
            stats: Arc::new(RwLock::new(HealthStats::default())),
        };
        
        info!("Health monitor initialized with config: {:?}", monitor.config);
        Ok(monitor)
    }
    
    /// Update V4 client health status
    pub async fn update_v4_health(&self, status: HealthStatus) {
        *self.v4_health.write().await = Some(status.clone());
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.v4_stats.total_checks += 1;
        stats.v4_stats.avg_response_time = Duration::from_millis(status.metrics.response_time_ms);
        
        if status.healthy {
            stats.v4_stats.successful_checks += 1;
            stats.v4_stats.last_success = Some(SystemTime::now());
            stats.v4_stats.consecutive_successes += 1;
            stats.v4_stats.consecutive_failures = 0;
        } else {
            stats.v4_stats.failed_checks += 1;
            stats.v4_stats.last_failure = Some(SystemTime::now());
            stats.v4_stats.consecutive_failures += 1;
            stats.v4_stats.consecutive_successes = 0;
        }
        
        drop(stats);
        
        // Update overall health
        self.update_overall_health().await;
        
        debug!("V4 health updated: healthy={}", status.healthy);
    }
    
    /// Update V7 client health status
    pub async fn update_v7_health(&self, status: HealthStatus) {
        *self.v7_health.write().await = Some(status.clone());
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.v7_stats.total_checks += 1;
        stats.v7_stats.avg_response_time = Duration::from_millis(status.metrics.response_time_ms);
        
        if status.healthy {
            stats.v7_stats.successful_checks += 1;
            stats.v7_stats.last_success = Some(SystemTime::now());
            stats.v7_stats.consecutive_successes += 1;
            stats.v7_stats.consecutive_failures = 0;
        } else {
            stats.v7_stats.failed_checks += 1;
            stats.v7_stats.last_failure = Some(SystemTime::now());
            stats.v7_stats.consecutive_failures += 1;
            stats.v7_stats.consecutive_successes = 0;
        }
        
        drop(stats);
        
        // Update overall health
        self.update_overall_health().await;
        
        debug!("V7 health updated: healthy={}", status.healthy);
    }
    
    /// Record a V4 client error
    pub async fn record_v4_error(&self, error: FacadeError) {
        let error_status = HealthStatus {
            healthy: false,
            sync_status: SyncStatus::Error,
            peer_count: 0,
            last_success: None,
            error_details: Some(error.to_string()),
            metrics: HealthMetrics::default(),
        };
        
        self.update_v4_health(error_status).await;
        warn!("V4 health error recorded: {}", error);
    }
    
    /// Record a V7 client error
    pub async fn record_v7_error(&self, error: FacadeError) {
        let error_status = HealthStatus {
            healthy: false,
            sync_status: SyncStatus::Error,
            peer_count: 0,
            last_success: None,
            error_details: Some(error.to_string()),
            metrics: HealthMetrics::default(),
        };
        
        self.update_v7_health(error_status).await;
        warn!("V7 health error recorded: {}", error);
    }
    
    /// Get overall facade health status
    pub async fn get_overall_health(&self) -> FacadeResult<HealthStatus> {
        Ok(self.overall_health.read().await.clone())
    }
    
    /// Get health statistics
    pub async fn get_health_stats(&self) -> HealthStats {
        self.stats.read().await.clone()
    }
    
    /// Update overall health based on individual client health
    async fn update_overall_health(&self) {
        let v4_health = self.v4_health.read().await.clone();
        let v7_health = self.v7_health.read().await.clone();
        
        let overall_healthy = match (&v4_health, &v7_health) {
            (Some(v4), Some(v7)) => {
                // Both clients available - require at least one to be healthy
                v4.healthy || v7.healthy
            }
            (Some(v4), None) => {
                // Only V4 available
                v4.healthy
            }
            (None, Some(v7)) => {
                // Only V7 available
                v7.healthy
            }
            (None, None) => {
                // No clients available
                false
            }
        };
        
        let sync_status = match (&v4_health, &v7_health) {
            (Some(v4), Some(v7)) => {
                // Use the best sync status between the two
                if matches!(v4.sync_status, SyncStatus::Synced) || matches!(v7.sync_status, SyncStatus::Synced) {
                    SyncStatus::Synced
                } else if matches!(v4.sync_status, SyncStatus::Syncing) || matches!(v7.sync_status, SyncStatus::Syncing) {
                    SyncStatus::Syncing
                } else {
                    SyncStatus::Syncing
                }
            }
            (Some(v4), None) => v4.sync_status.clone(),
            (None, Some(v7)) => v7.sync_status.clone(),
            (None, None) => SyncStatus::Error,
        };
        
        let peer_count = match (&v4_health, &v7_health) {
            (Some(v4), Some(v7)) => std::cmp::max(v4.peer_count, v7.peer_count),
            (Some(v4), None) => v4.peer_count,
            (None, Some(v7)) => v7.peer_count,
            (None, None) => 0,
        };
        
        let error_details = if !overall_healthy {
            let mut errors = Vec::new();
            if let Some(v4) = &v4_health {
                if !v4.healthy {
                    if let Some(error) = &v4.error_details {
                        errors.push(format!("V4: {}", error));
                    } else {
                        errors.push("V4: unhealthy".to_string());
                    }
                }
            }
            if let Some(v7) = &v7_health {
                if !v7.healthy {
                    if let Some(error) = &v7.error_details {
                        errors.push(format!("V7: {}", error));
                    } else {
                        errors.push("V7: unhealthy".to_string());
                    }
                }
            }
            if errors.is_empty() {
                Some("No healthy clients available".to_string())
            } else {
                Some(errors.join("; "))
            }
        } else {
            None
        };
        
        let avg_response_time = match (&v4_health, &v7_health) {
            (Some(v4), Some(v7)) => {
                // Average the response times
                Duration::from_millis(
                    (v4.metrics.response_time_ms + v7.metrics.response_time_ms) / 2
                )
            }
            (Some(v4), None) => Duration::from_millis(v4.metrics.response_time_ms),
            (None, Some(v7)) => Duration::from_millis(v7.metrics.response_time_ms),
            (None, None) => Duration::from_millis(0),
        };
        
        let overall_status = HealthStatus {
            healthy: overall_healthy,
            sync_status,
            peer_count,
            last_success: if overall_healthy { Some(SystemTime::now()) } else { None },
            error_details,
            metrics: HealthMetrics {
                response_time_ms: avg_response_time.as_millis() as u64,
                error_rate: if overall_healthy { 0.0 } else { 1.0 },
                success_count: v4_health.as_ref().map(|h| h.metrics.success_count).unwrap_or(0) +
                              v7_health.as_ref().map(|h| h.metrics.success_count).unwrap_or(0),
                error_count: v4_health.as_ref().map(|h| h.metrics.error_count).unwrap_or(0) +
                            v7_health.as_ref().map(|h| h.metrics.error_count).unwrap_or(0),
                request_count: v4_health.as_ref().map(|h| h.metrics.request_count).unwrap_or(0) +
                              v7_health.as_ref().map(|h| h.metrics.request_count).unwrap_or(0),
                memory_usage_mb: v4_health.as_ref().map(|h| h.metrics.memory_usage_mb).unwrap_or(0) +
                                v7_health.as_ref().map(|h| h.metrics.memory_usage_mb).unwrap_or(0),
                cpu_usage: v4_health.as_ref().map(|h| h.metrics.cpu_usage).unwrap_or(0.0) +
                          v7_health.as_ref().map(|h| h.metrics.cpu_usage).unwrap_or(0.0),
            },
        };
        
        *self.overall_health.write().await = overall_status.clone();
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_checks += 1;
        stats.last_update = SystemTime::now();
        
        if overall_healthy {
            stats.successful_checks += 1;
        } else {
            stats.failed_checks += 1;
        }
        
        drop(stats);
        
        debug!("Overall health updated: healthy={}, sync_status={:?}", 
               overall_status.healthy, overall_status.sync_status);
    }
    
    /// Check if facade is degraded (some clients unhealthy)
    pub async fn is_degraded(&self) -> bool {
        let v4_health = self.v4_health.read().await;
        let v7_health = self.v7_health.read().await;
        
        match (&*v4_health, &*v7_health) {
            (Some(v4), Some(v7)) => {
                // Both available - degraded if one is unhealthy
                v4.healthy ^ v7.healthy
            }
            _ => false, // Not degraded if only one client is configured
        }
    }
    
    /// Get health summary for monitoring
    pub async fn get_health_summary(&self) -> HealthSummary {
        let v4_health = self.v4_health.read().await;
        let v7_health = self.v7_health.read().await;
        let overall = self.overall_health.read().await;
        let stats = self.stats.read().await;
        
        HealthSummary {
            overall_healthy: overall.healthy,
            degraded: self.is_degraded().await,
            v4_available: v4_health.is_some(),
            v4_healthy: v4_health.as_ref().map(|h| h.healthy).unwrap_or(false),
            v7_available: v7_health.is_some(),
            v7_healthy: v7_health.as_ref().map(|h| h.healthy).unwrap_or(false),
            sync_status: overall.sync_status.clone(),
            peer_count: overall.peer_count,
            total_checks: stats.total_checks,
            success_rate: if stats.total_checks > 0 {
                stats.successful_checks as f64 / stats.total_checks as f64
            } else {
                0.0
            },
            v4_consecutive_failures: stats.v4_stats.consecutive_failures,
            v7_consecutive_failures: stats.v7_stats.consecutive_failures,
        }
    }
}

/// Health summary for monitoring and alerting
#[derive(Debug, Clone)]
pub struct HealthSummary {
    /// Overall facade health
    pub overall_healthy: bool,
    
    /// Is facade in degraded state
    pub degraded: bool,
    
    /// V4 client availability
    pub v4_available: bool,
    
    /// V4 client health
    pub v4_healthy: bool,
    
    /// V7 client availability
    pub v7_available: bool,
    
    /// V7 client health
    pub v7_healthy: bool,
    
    /// Sync status
    pub sync_status: SyncStatus,
    
    /// Peer count
    pub peer_count: u32,
    
    /// Total health checks performed
    pub total_checks: u64,
    
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    
    /// V4 consecutive failures
    pub v4_consecutive_failures: u32,
    
    /// V7 consecutive failures
    pub v7_consecutive_failures: u32,
}

impl Default for HealthStats {
    fn default() -> Self {
        Self {
            total_checks: 0,
            successful_checks: 0,
            failed_checks: 0,
            v4_stats: ClientHealthStats::default(),
            v7_stats: ClientHealthStats::default(),
            last_update: SystemTime::now(),
        }
    }
}

impl Default for ClientHealthStats {
    fn default() -> Self {
        Self {
            total_checks: 0,
            successful_checks: 0,
            failed_checks: 0,
            avg_response_time: Duration::from_millis(0),
            last_success: None,
            last_failure: None,
            consecutive_failures: 0,
            consecutive_successes: 0,
        }
    }
}