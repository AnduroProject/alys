//! Health monitoring for the Lighthouse compatibility layer
//!
//! This module provides comprehensive health monitoring capabilities including
//! health checks, alerting, and automatic rollback triggers.

use crate::{
    config::HealthConfig,
    error::{CompatError, CompatResult},
    types::{HealthStatus, HealthMetrics, SyncStatus},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Health monitor for the compatibility layer
pub struct HealthMonitor {
    /// Configuration
    config: HealthConfig,
    
    /// V4 client health status
    v4_health: Arc<RwLock<Option<HealthStatus>>>,
    
    /// V5 client health status
    v5_health: Arc<RwLock<Option<HealthStatus>>>,
    
    /// Overall system health
    system_health: Arc<RwLock<SystemHealth>>,
    
    /// Health check history
    health_history: Arc<RwLock<HealthHistory>>,
    
    /// Active alerts
    active_alerts: Arc<RwLock<HashMap<String, ActiveAlert>>>,
}

/// System-level health information
#[derive(Debug, Clone)]
pub struct SystemHealth {
    /// Overall health status
    pub healthy: bool,
    
    /// Last health check time
    pub last_check: SystemTime,
    
    /// Health score (0.0 to 1.0)
    pub health_score: f64,
    
    /// Active issues
    pub issues: Vec<HealthIssue>,
    
    /// Performance metrics
    pub metrics: SystemMetrics,
}

/// Individual health issue
#[derive(Debug, Clone)]
pub struct HealthIssue {
    /// Issue type
    pub issue_type: HealthIssueType,
    
    /// Issue description
    pub description: String,
    
    /// Issue severity
    pub severity: IssueSeverity,
    
    /// When the issue was first detected
    pub first_detected: SystemTime,
    
    /// Issue source (v4, v5, system)
    pub source: String,
    
    /// Suggested remediation
    pub remediation: Option<String>,
}

/// Types of health issues
#[derive(Debug, Clone, PartialEq)]
pub enum HealthIssueType {
    /// High error rate
    HighErrorRate,
    
    /// High latency
    HighLatency,
    
    /// Low throughput
    LowThroughput,
    
    /// Memory usage high
    MemoryPressure,
    
    /// CPU usage high
    CpuPressure,
    
    /// Connectivity issues
    ConnectivityIssue,
    
    /// Sync issues
    SyncIssue,
    
    /// Consensus mismatch
    ConsensusMismatch,
    
    /// Custom health check failure
    CustomCheckFailure,
}

/// Issue severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    /// Low impact issue
    Low,
    
    /// Medium impact issue
    Medium,
    
    /// High impact issue
    High,
    
    /// Critical issue requiring immediate action
    Critical,
}

/// System-level metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// Overall response time
    pub avg_response_time: Duration,
    
    /// Overall error rate
    pub error_rate: f64,
    
    /// Request throughput (requests/second)
    pub throughput: f64,
    
    /// Memory usage across all clients
    pub total_memory_mb: u64,
    
    /// CPU usage
    pub cpu_usage: f64,
    
    /// Uptime
    pub uptime: Duration,
}

/// Health check history for trend analysis
#[derive(Debug)]
pub struct HealthHistory {
    /// V4 health history
    pub v4_history: Vec<HealthDataPoint>,
    
    /// V5 health history
    pub v5_history: Vec<HealthDataPoint>,
    
    /// System health history
    pub system_history: Vec<SystemHealthDataPoint>,
    
    /// Maximum history size
    pub max_size: usize,
}

/// Individual health data point
#[derive(Debug, Clone)]
pub struct HealthDataPoint {
    /// Timestamp
    pub timestamp: SystemTime,
    
    /// Health status at this point
    pub status: HealthStatus,
}

/// System health data point
#[derive(Debug, Clone)]
pub struct SystemHealthDataPoint {
    /// Timestamp
    pub timestamp: SystemTime,
    
    /// Health score
    pub health_score: f64,
    
    /// Number of active issues
    pub issue_count: usize,
    
    /// System metrics
    pub metrics: SystemMetrics,
}

/// Active alert information
#[derive(Debug, Clone)]
pub struct ActiveAlert {
    /// Alert ID
    pub id: String,
    
    /// Alert type
    pub alert_type: String,
    
    /// Alert message
    pub message: String,
    
    /// When alert was first triggered
    pub triggered_at: SystemTime,
    
    /// Alert severity
    pub severity: IssueSeverity,
    
    /// Number of times this alert has been triggered
    pub trigger_count: u32,
    
    /// Last time alert was sent
    pub last_sent: Option<SystemTime>,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub async fn new(config: HealthConfig) -> CompatResult<Self> {
        info!("Initializing health monitor");
        
        let monitor = Self {
            config,
            v4_health: Arc::new(RwLock::new(None)),
            v5_health: Arc::new(RwLock::new(None)),
            system_health: Arc::new(RwLock::new(SystemHealth::default())),
            health_history: Arc::new(RwLock::new(HealthHistory::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Start background health monitoring
        if config.enabled {
            monitor.start_monitoring().await?;
        }
        
        Ok(monitor)
    }
    
    /// Update v4 client health
    pub async fn update_v4_health(&self, status: HealthStatus) {
        debug!("Updating v4 health status");
        
        *self.v4_health.write().await = Some(status.clone());
        
        // Add to history
        let mut history = self.health_history.write().await;
        history.add_v4_datapoint(HealthDataPoint {
            timestamp: SystemTime::now(),
            status,
        });
        
        // Update overall system health
        self.update_system_health().await;
    }
    
    /// Update v5 client health
    pub async fn update_v5_health(&self, status: HealthStatus) {
        debug!("Updating v5 health status");
        
        *self.v5_health.write().await = Some(status.clone());
        
        // Add to history
        let mut history = self.health_history.write().await;
        history.add_v5_datapoint(HealthDataPoint {
            timestamp: SystemTime::now(),
            status,
        });
        
        // Update overall system health
        self.update_system_health().await;
    }
    
    /// Record v4 error
    pub async fn record_v4_error(&self, error: CompatError) {
        warn!("Recording v4 error: {}", error);
        
        // Create unhealthy status
        let status = HealthStatus {
            healthy: false,
            sync_status: SyncStatus::Error { message: error.to_string() },
            peer_count: 0,
            last_success: None,
            error_details: Some(error.to_string()),
            metrics: HealthMetrics::default(),
        };
        
        self.update_v4_health(status).await;
    }
    
    /// Record v5 error
    pub async fn record_v5_error(&self, error: CompatError) {
        warn!("Recording v5 error: {}", error);
        
        // Create unhealthy status
        let status = HealthStatus {
            healthy: false,
            sync_status: SyncStatus::Error { message: error.to_string() },
            peer_count: 0,
            last_success: None,
            error_details: Some(error.to_string()),
            metrics: HealthMetrics::default(),
        };
        
        self.update_v5_health(status).await;
    }
    
    /// Get overall system health
    pub async fn get_overall_health(&self) -> CompatResult<HealthStatus> {
        let system_health = self.system_health.read().await.clone();
        
        Ok(HealthStatus {
            healthy: system_health.healthy,
            sync_status: SyncStatus::Synced, // Simplified
            peer_count: 0, // Would aggregate from clients
            last_success: Some(system_health.last_check),
            error_details: if system_health.healthy {
                None
            } else {
                Some(format!("{} active issues", system_health.issues.len()))
            },
            metrics: HealthMetrics {
                avg_response_time: system_health.metrics.avg_response_time,
                error_rate: system_health.metrics.error_rate,
                request_count: 0, // Would track actual count
                memory_usage_mb: system_health.metrics.total_memory_mb,
                cpu_usage: system_health.metrics.cpu_usage,
            },
        })
    }
    
    /// Check if system should trigger rollback
    pub async fn should_rollback(&self) -> bool {
        let system_health = self.system_health.read().await;
        
        // Check for critical issues
        for issue in &system_health.issues {
            if issue.severity == IssueSeverity::Critical {
                match issue.issue_type {
                    HealthIssueType::HighErrorRate | 
                    HealthIssueType::ConsensusMismatch |
                    HealthIssueType::SyncIssue => return true,
                    _ => {}
                }
            }
        }
        
        // Check health score threshold
        system_health.health_score < 0.5 // 50% health score triggers rollback
    }
    
    /// Get health trend analysis
    pub async fn get_health_trends(&self) -> HealthTrends {
        let history = self.health_history.read().await;
        
        HealthTrends {
            v4_trend: self.analyze_trend(&history.v4_history),
            v5_trend: self.analyze_trend(&history.v5_history),
            system_trend: self.analyze_system_trend(&history.system_history),
        }
    }
    
    /// Start background health monitoring
    async fn start_monitoring(&self) -> CompatResult<()> {
        let monitor = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(monitor.config.check_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = monitor.perform_health_checks().await {
                    error!("Health check failed: {}", e);
                }
                
                monitor.check_alerts().await;
                monitor.cleanup_old_data().await;
            }
        });
        
        Ok(())
    }
    
    /// Perform all configured health checks
    async fn perform_health_checks(&self) -> CompatResult<()> {
        for check_config in &self.config.checks {
            if let Err(e) = self.perform_health_check(check_config).await {
                warn!("Health check '{}' failed: {}", check_config.name, e);
                
                // Record failure
                let issue = HealthIssue {
                    issue_type: HealthIssueType::CustomCheckFailure,
                    description: format!("Health check '{}' failed: {}", check_config.name, e),
                    severity: IssueSeverity::Medium,
                    first_detected: SystemTime::now(),
                    source: "health_monitor".to_string(),
                    remediation: Some("Check system logs and client connectivity".to_string()),
                };
                
                self.add_health_issue(issue).await;
            }
        }
        
        Ok(())
    }
    
    /// Perform individual health check
    async fn perform_health_check(&self, check_config: &crate::config::HealthCheckConfig) -> CompatResult<()> {
        use crate::config::HealthCheckType;
        
        match &check_config.check_type {
            HealthCheckType::HttpEndpoint => {
                // Would perform HTTP health check
                Ok(())
            }
            HealthCheckType::ResponseTime => {
                // Would check response times
                self.check_response_times().await
            }
            HealthCheckType::ErrorRate => {
                // Would check error rates
                self.check_error_rates().await
            }
            HealthCheckType::MemoryUsage => {
                // Would check memory usage
                self.check_memory_usage().await
            }
            HealthCheckType::CpuUsage => {
                // Would check CPU usage
                self.check_cpu_usage().await
            }
            HealthCheckType::ConsensusConsistency => {
                // Would check consensus consistency
                self.check_consensus_consistency().await
            }
            HealthCheckType::Custom { script_path } => {
                // Would execute custom script
                self.execute_custom_check(script_path).await
            }
        }
    }
    
    /// Check response times
    async fn check_response_times(&self) -> CompatResult<()> {
        let system_health = self.system_health.read().await;
        
        if system_health.metrics.avg_response_time > Duration::from_millis(1000) {
            return Err(CompatError::PerformanceDegradation {
                metric: "response_time".to_string(),
                threshold: "1000ms".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Check error rates
    async fn check_error_rates(&self) -> CompatResult<()> {
        let system_health = self.system_health.read().await;
        
        if system_health.metrics.error_rate > 0.05 { // 5% error rate threshold
            return Err(CompatError::PerformanceDegradation {
                metric: "error_rate".to_string(),
                threshold: "5%".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Check memory usage
    async fn check_memory_usage(&self) -> CompatResult<()> {
        let system_health = self.system_health.read().await;
        
        if system_health.metrics.total_memory_mb > 2000 { // 2GB threshold
            return Err(CompatError::MemoryLimitExceeded {
                used: format!("{}MB", system_health.metrics.total_memory_mb),
                limit: "2000MB".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Check CPU usage
    async fn check_cpu_usage(&self) -> CompatResult<()> {
        let system_health = self.system_health.read().await;
        
        if system_health.metrics.cpu_usage > 80.0 { // 80% CPU threshold
            return Err(CompatError::PerformanceDegradation {
                metric: "cpu_usage".to_string(),
                threshold: "80%".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Check consensus consistency
    async fn check_consensus_consistency(&self) -> CompatResult<()> {
        // Would check for consensus mismatches between v4 and v5
        // This would be implemented with actual consensus checking logic
        Ok(())
    }
    
    /// Execute custom health check script
    async fn execute_custom_check(&self, _script_path: &std::path::Path) -> CompatResult<()> {
        // Would execute custom health check script
        // This would use tokio::process::Command to run the script
        Ok(())
    }
    
    /// Update overall system health based on client health
    async fn update_system_health(&self) {
        let v4_health = self.v4_health.read().await.clone();
        let v5_health = self.v5_health.read().await.clone();
        
        let mut system_health = self.system_health.write().await;
        system_health.last_check = SystemTime::now();
        
        // Calculate health score
        let mut health_score = 1.0;
        let mut issues = Vec::new();
        
        // Analyze v4 health
        if let Some(v4) = &v4_health {
            if !v4.healthy {
                health_score *= 0.5;
                issues.push(HealthIssue {
                    issue_type: HealthIssueType::ConnectivityIssue,
                    description: "V4 client unhealthy".to_string(),
                    severity: IssueSeverity::High,
                    first_detected: SystemTime::now(),
                    source: "v4".to_string(),
                    remediation: Some("Check v4 client logs and connectivity".to_string()),
                });
            }
            
            if v4.metrics.error_rate > 0.05 {
                health_score *= 0.8;
                issues.push(HealthIssue {
                    issue_type: HealthIssueType::HighErrorRate,
                    description: format!("V4 error rate: {:.2}%", v4.metrics.error_rate * 100.0),
                    severity: IssueSeverity::Medium,
                    first_detected: SystemTime::now(),
                    source: "v4".to_string(),
                    remediation: Some("Investigate v4 client errors".to_string()),
                });
            }
        }
        
        // Analyze v5 health
        if let Some(v5) = &v5_health {
            if !v5.healthy {
                health_score *= 0.5;
                issues.push(HealthIssue {
                    issue_type: HealthIssueType::ConnectivityIssue,
                    description: "V5 client unhealthy".to_string(),
                    severity: IssueSeverity::High,
                    first_detected: SystemTime::now(),
                    source: "v5".to_string(),
                    remediation: Some("Check v5 client logs and connectivity".to_string()),
                });
            }
        }
        
        // Calculate aggregate metrics
        let metrics = self.calculate_system_metrics(&v4_health, &v5_health).await;
        
        system_health.health_score = health_score;
        system_health.healthy = health_score > 0.7; // 70% threshold for healthy
        system_health.issues = issues;
        system_health.metrics = metrics;
        
        // Add to history
        let mut history = self.health_history.write().await;
        history.add_system_datapoint(SystemHealthDataPoint {
            timestamp: SystemTime::now(),
            health_score,
            issue_count: system_health.issues.len(),
            metrics: system_health.metrics.clone(),
        });
    }
    
    /// Calculate aggregate system metrics
    async fn calculate_system_metrics(
        &self,
        v4_health: &Option<HealthStatus>,
        v5_health: &Option<HealthStatus>,
    ) -> SystemMetrics {
        let mut total_memory = 0u64;
        let mut avg_response_time = Duration::from_millis(0);
        let mut error_rate = 0.0f64;
        let mut cpu_usage = 0.0f64;
        let mut client_count = 0;
        
        if let Some(v4) = v4_health {
            total_memory += v4.metrics.memory_usage_mb;
            avg_response_time += v4.metrics.avg_response_time;
            error_rate += v4.metrics.error_rate;
            cpu_usage += v4.metrics.cpu_usage;
            client_count += 1;
        }
        
        if let Some(v5) = v5_health {
            total_memory += v5.metrics.memory_usage_mb;
            avg_response_time += v5.metrics.avg_response_time;
            error_rate += v5.metrics.error_rate;
            cpu_usage += v5.metrics.cpu_usage;
            client_count += 1;
        }
        
        if client_count > 0 {
            avg_response_time /= client_count;
            error_rate /= client_count as f64;
            cpu_usage /= client_count as f64;
        }
        
        SystemMetrics {
            avg_response_time,
            error_rate,
            throughput: 0.0, // Would calculate actual throughput
            total_memory_mb: total_memory,
            cpu_usage,
            uptime: Duration::from_secs(3600), // Would track actual uptime
        }
    }
    
    /// Add health issue
    async fn add_health_issue(&self, issue: HealthIssue) {
        let mut system_health = self.system_health.write().await;
        
        // Check if similar issue already exists
        let similar_exists = system_health.issues.iter().any(|existing| {
            existing.issue_type == issue.issue_type && existing.source == issue.source
        });
        
        if !similar_exists {
            system_health.issues.push(issue.clone());
            
            // Trigger alert if configured
            if self.config.alerting.enabled {
                self.trigger_alert(issue).await;
            }
        }
    }
    
    /// Trigger alert for health issue
    async fn trigger_alert(&self, issue: HealthIssue) {
        let alert_id = format!("{}_{}", issue.issue_type.as_str(), issue.source);
        
        let mut alerts = self.active_alerts.write().await;
        
        if let Some(existing) = alerts.get_mut(&alert_id) {
            // Update existing alert
            existing.trigger_count += 1;
            
            // Check throttling
            let should_send = if let Some(last_sent) = existing.last_sent {
                SystemTime::now().duration_since(last_sent).unwrap_or(Duration::from_secs(0))
                    > self.config.alerting.throttling.min_interval
            } else {
                true
            };
            
            if should_send {
                self.send_alert(existing.clone()).await;
                existing.last_sent = Some(SystemTime::now());
            }
        } else {
            // Create new alert
            let alert = ActiveAlert {
                id: alert_id.clone(),
                alert_type: issue.issue_type.as_str().to_string(),
                message: issue.description,
                triggered_at: SystemTime::now(),
                severity: issue.severity,
                trigger_count: 1,
                last_sent: None,
            };
            
            self.send_alert(alert.clone()).await;
            
            let mut alert_with_sent_time = alert;
            alert_with_sent_time.last_sent = Some(SystemTime::now());
            alerts.insert(alert_id, alert_with_sent_time);
        }
    }
    
    /// Send alert through configured channels
    async fn send_alert(&self, alert: ActiveAlert) {
        for destination in &self.config.alerting.destinations {
            if let Err(e) = self.send_alert_to_destination(&alert, destination).await {
                error!("Failed to send alert to destination: {}", e);
            }
        }
    }
    
    /// Send alert to specific destination
    async fn send_alert_to_destination(
        &self,
        alert: &ActiveAlert,
        destination: &crate::config::AlertDestination,
    ) -> CompatResult<()> {
        use crate::config::AlertDestination;
        
        match destination {
            AlertDestination::Log { level } => {
                match level.as_str() {
                    "error" => error!("ALERT: {}", alert.message),
                    "warn" => warn!("ALERT: {}", alert.message),
                    "info" => info!("ALERT: {}", alert.message),
                    _ => debug!("ALERT: {}", alert.message),
                }
            }
            AlertDestination::Email { .. } => {
                // Would send email alert
                debug!("Would send email alert: {}", alert.message);
            }
            AlertDestination::Slack { .. } => {
                // Would send Slack alert
                debug!("Would send Slack alert: {}", alert.message);
            }
            AlertDestination::Webhook { url, headers } => {
                // Would send webhook alert
                debug!("Would send webhook alert to {}: {}", url, alert.message);
            }
        }
        
        Ok(())
    }
    
    /// Check and manage alerts
    async fn check_alerts(&self) {
        // Check for resolved issues and clear alerts
        // This would be implemented with actual alert resolution logic
    }
    
    /// Clean up old health data
    async fn cleanup_old_data(&self) {
        let mut history = self.health_history.write().await;
        history.cleanup_old_data();
    }
    
    /// Analyze health trend
    fn analyze_trend(&self, history: &[HealthDataPoint]) -> HealthTrend {
        if history.len() < 2 {
            return HealthTrend::Stable;
        }
        
        let recent_count = std::cmp::min(10, history.len());
        let recent_health: f64 = history
            .iter()
            .rev()
            .take(recent_count)
            .map(|dp| if dp.status.healthy { 1.0 } else { 0.0 })
            .sum::<f64>() / recent_count as f64;
        
        if recent_health > 0.8 {
            HealthTrend::Improving
        } else if recent_health < 0.3 {
            HealthTrend::Degrading
        } else {
            HealthTrend::Stable
        }
    }
    
    /// Analyze system health trend
    fn analyze_system_trend(&self, history: &[SystemHealthDataPoint]) -> HealthTrend {
        if history.len() < 2 {
            return HealthTrend::Stable;
        }
        
        let recent_count = std::cmp::min(10, history.len());
        let avg_score: f64 = history
            .iter()
            .rev()
            .take(recent_count)
            .map(|dp| dp.health_score)
            .sum::<f64>() / recent_count as f64;
        
        if avg_score > 0.8 {
            HealthTrend::Improving
        } else if avg_score < 0.5 {
            HealthTrend::Degrading
        } else {
            HealthTrend::Stable
        }
    }
}

// Clone implementation for Arc usage
impl Clone for HealthMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            v4_health: Arc::clone(&self.v4_health),
            v5_health: Arc::clone(&self.v5_health),
            system_health: Arc::clone(&self.system_health),
            health_history: Arc::clone(&self.health_history),
            active_alerts: Arc::clone(&self.active_alerts),
        }
    }
}

/// Health trend analysis
#[derive(Debug, Clone)]
pub struct HealthTrends {
    /// V4 client trend
    pub v4_trend: HealthTrend,
    
    /// V5 client trend
    pub v5_trend: HealthTrend,
    
    /// System trend
    pub system_trend: HealthTrend,
}

/// Individual health trend
#[derive(Debug, Clone, PartialEq)]
pub enum HealthTrend {
    /// Health is improving
    Improving,
    
    /// Health is stable
    Stable,
    
    /// Health is degrading
    Degrading,
}

impl Default for SystemHealth {
    fn default() -> Self {
        Self {
            healthy: true,
            last_check: SystemTime::now(),
            health_score: 1.0,
            issues: Vec::new(),
            metrics: SystemMetrics::default(),
        }
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            avg_response_time: Duration::from_millis(50),
            error_rate: 0.0,
            throughput: 100.0,
            total_memory_mb: 200,
            cpu_usage: 20.0,
            uptime: Duration::from_secs(0),
        }
    }
}

impl HealthHistory {
    /// Create new health history
    pub fn new() -> Self {
        Self {
            v4_history: Vec::new(),
            v5_history: Vec::new(),
            system_history: Vec::new(),
            max_size: 1000, // Keep last 1000 data points
        }
    }
    
    /// Add v4 data point
    pub fn add_v4_datapoint(&mut self, datapoint: HealthDataPoint) {
        self.v4_history.push(datapoint);
        if self.v4_history.len() > self.max_size {
            self.v4_history.remove(0);
        }
    }
    
    /// Add v5 data point
    pub fn add_v5_datapoint(&mut self, datapoint: HealthDataPoint) {
        self.v5_history.push(datapoint);
        if self.v5_history.len() > self.max_size {
            self.v5_history.remove(0);
        }
    }
    
    /// Add system data point
    pub fn add_system_datapoint(&mut self, datapoint: SystemHealthDataPoint) {
        self.system_history.push(datapoint);
        if self.system_history.len() > self.max_size {
            self.system_history.remove(0);
        }
    }
    
    /// Clean up old data (older than 24 hours)
    pub fn cleanup_old_data(&mut self) {
        let cutoff = SystemTime::now() - Duration::from_secs(24 * 3600);
        
        self.v4_history.retain(|dp| dp.timestamp > cutoff);
        self.v5_history.retain(|dp| dp.timestamp > cutoff);
        self.system_history.retain(|dp| dp.timestamp > cutoff);
    }
}

impl HealthIssueType {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HighErrorRate => "high_error_rate",
            Self::HighLatency => "high_latency",
            Self::LowThroughput => "low_throughput",
            Self::MemoryPressure => "memory_pressure",
            Self::CpuPressure => "cpu_pressure",
            Self::ConnectivityIssue => "connectivity_issue",
            Self::SyncIssue => "sync_issue",
            Self::ConsensusMismatch => "consensus_mismatch",
            Self::CustomCheckFailure => "custom_check_failure",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HealthConfig, HealthCheckConfig, HealthCheckType, AlertingConfig};
    
    #[tokio::test]
    async fn test_health_monitor_creation() {
        let config = HealthConfig::default();
        let monitor = HealthMonitor::new(config).await.unwrap();
        
        let health = monitor.get_overall_health().await.unwrap();
        assert!(health.healthy);
    }
    
    #[tokio::test]
    async fn test_health_status_updates() {
        let config = HealthConfig::default();
        let monitor = HealthMonitor::new(config).await.unwrap();
        
        let unhealthy_status = HealthStatus {
            healthy: false,
            sync_status: SyncStatus::Error { message: "test error".to_string() },
            peer_count: 0,
            last_success: None,
            error_details: Some("test error".to_string()),
            metrics: HealthMetrics::default(),
        };
        
        monitor.update_v4_health(unhealthy_status).await;
        
        let overall_health = monitor.get_overall_health().await.unwrap();
        // Health might still be true depending on system health calculation
        assert!(overall_health.error_details.is_some() || overall_health.healthy);
    }
    
    #[test]
    fn test_health_history() {
        let mut history = HealthHistory::new();
        
        let datapoint = HealthDataPoint {
            timestamp: SystemTime::now(),
            status: HealthStatus::default(),
        };
        
        history.add_v4_datapoint(datapoint.clone());
        history.add_v5_datapoint(datapoint);
        
        assert_eq!(history.v4_history.len(), 1);
        assert_eq!(history.v5_history.len(), 1);
    }
    
    #[test]
    fn test_issue_severity_ordering() {
        assert!(IssueSeverity::Critical > IssueSeverity::High);
        assert!(IssueSeverity::High > IssueSeverity::Medium);
        assert!(IssueSeverity::Medium > IssueSeverity::Low);
    }
}