//! Workflow Monitoring
//! 
//! Real-time monitoring and alerting for bridge workflows

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error};

/// Workflow monitoring system
#[derive(Debug)]
pub struct WorkflowMonitor {
    /// Active workflow monitoring
    monitored_workflows: HashMap<String, WorkflowMonitoringState>,
    
    /// Monitoring configuration
    monitoring_config: MonitoringConfig,
    
    /// Alert system
    alert_manager: AlertManager,
    
    /// Metrics collection
    monitoring_metrics: MonitoringMetrics,
}

/// Individual workflow monitoring state
#[derive(Debug, Clone)]
pub struct WorkflowMonitoringState {
    pub workflow_id: String,
    pub workflow_type: String,
    pub started_at: SystemTime,
    pub last_heartbeat: SystemTime,
    pub expected_completion: SystemTime,
    pub status: WorkflowMonitoringStatus,
    pub alert_level: AlertLevel,
    pub performance_metrics: WorkflowPerformanceMetrics,
    pub anomalies_detected: Vec<WorkflowAnomaly>,
}

/// Workflow monitoring status
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowMonitoringStatus {
    Active,
    Delayed,
    Stalled,
    AtRisk,
    Completed,
    Failed,
}

/// Alert levels
#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Workflow performance metrics
#[derive(Debug, Clone, Default)]
pub struct WorkflowPerformanceMetrics {
    pub execution_time: Duration,
    pub step_completion_rate: f64,
    pub error_count: u32,
    pub retry_count: u32,
    pub resource_usage_score: f64,
    pub efficiency_score: f64,
}

/// Workflow anomalies
#[derive(Debug, Clone)]
pub struct WorkflowAnomaly {
    pub anomaly_type: AnomalyType,
    pub detected_at: SystemTime,
    pub severity: AnomalySeverity,
    pub description: String,
    pub suggested_action: String,
}

/// Types of anomalies
#[derive(Debug, Clone)]
pub enum AnomalyType {
    UnexpectedDelay,
    HighErrorRate,
    ExcessiveRetries,
    ResourceExhaustion,
    PerformanceDegradation,
    UnexpectedBehavior,
}

/// Anomaly severity
#[derive(Debug, Clone, PartialEq)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Monitoring configuration
#[derive(Debug)]
pub struct MonitoringConfig {
    pub heartbeat_interval: Duration,
    pub stall_detection_threshold: Duration,
    pub delay_warning_threshold: Duration,
    pub max_acceptable_error_rate: f64,
    pub performance_baseline: PerformanceBaseline,
}

/// Performance baseline for comparison
#[derive(Debug)]
pub struct PerformanceBaseline {
    pub expected_pegin_duration: Duration,
    pub expected_pegout_duration: Duration,
    pub acceptable_error_rate: f64,
    pub normal_retry_count: u32,
}

/// Alert management system
#[derive(Debug)]
pub struct AlertManager {
    active_alerts: HashMap<String, WorkflowAlert>,
    alert_history: Vec<AlertRecord>,
    alert_config: AlertConfig,
}

/// Workflow alert
#[derive(Debug, Clone)]
pub struct WorkflowAlert {
    pub alert_id: String,
    pub workflow_id: String,
    pub alert_type: AlertType,
    pub level: AlertLevel,
    pub message: String,
    pub created_at: SystemTime,
    pub acknowledged: bool,
    pub resolved: bool,
}

/// Alert types
#[derive(Debug, Clone)]
pub enum AlertType {
    WorkflowStalled,
    HighErrorRate,
    PerformanceDegradation,
    SystemOverload,
    ResourceExhaustion,
    SecurityAnomaly,
}

/// Alert record for history
#[derive(Debug, Clone)]
pub struct AlertRecord {
    pub alert: WorkflowAlert,
    pub resolved_at: Option<SystemTime>,
    pub resolution_action: Option<String>,
}

/// Alert configuration
#[derive(Debug)]
pub struct AlertConfig {
    pub enable_notifications: bool,
    pub notification_channels: Vec<NotificationChannel>,
    pub escalation_rules: Vec<EscalationRule>,
}

/// Notification channels
#[derive(Debug, Clone)]
pub enum NotificationChannel {
    Log,
    Email(String),
    Webhook(String),
    Slack(String),
}

/// Escalation rules
#[derive(Debug)]
pub struct EscalationRule {
    pub trigger_condition: EscalationCondition,
    pub escalation_delay: Duration,
    pub target_level: AlertLevel,
}

/// Escalation conditions
#[derive(Debug)]
pub enum EscalationCondition {
    UnacknowledgedAfter(Duration),
    RepeatedAlerts(u32),
    CriticalSystemState,
}

/// Monitoring metrics
#[derive(Debug, Default)]
pub struct MonitoringMetrics {
    pub workflows_monitored: u64,
    pub alerts_generated: u64,
    pub anomalies_detected: u64,
    pub average_workflow_duration: Duration,
    pub monitoring_overhead: Duration,
    pub alert_response_time: Duration,
}

impl WorkflowMonitor {
    pub fn new() -> Self {
        let monitoring_config = MonitoringConfig {
            heartbeat_interval: Duration::from_secs(30),
            stall_detection_threshold: Duration::from_secs(300),
            delay_warning_threshold: Duration::from_secs(180),
            max_acceptable_error_rate: 0.05,
            performance_baseline: PerformanceBaseline {
                expected_pegin_duration: Duration::from_secs(600),
                expected_pegout_duration: Duration::from_secs(900),
                acceptable_error_rate: 0.02,
                normal_retry_count: 2,
            },
        };

        let alert_config = AlertConfig {
            enable_notifications: true,
            notification_channels: vec![NotificationChannel::Log],
            escalation_rules: vec![
                EscalationRule {
                    trigger_condition: EscalationCondition::UnacknowledgedAfter(Duration::from_secs(300)),
                    escalation_delay: Duration::from_secs(60),
                    target_level: AlertLevel::Critical,
                },
            ],
        };

        let alert_manager = AlertManager {
            active_alerts: HashMap::new(),
            alert_history: Vec::new(),
            alert_config,
        };

        Self {
            monitored_workflows: HashMap::new(),
            monitoring_config,
            alert_manager,
            monitoring_metrics: MonitoringMetrics::default(),
        }
    }

    /// Initialize monitoring system
    pub async fn initialize(&mut self) -> Result<(), MonitoringError> {
        info!("Initializing workflow monitoring system");
        
        // Validate configuration
        self.validate_config()?;
        
        // Initialize alert manager
        self.alert_manager.initialize().await?;
        
        info!("Workflow monitoring system initialized successfully");
        Ok(())
    }

    /// Start monitoring a workflow
    pub async fn start_monitoring_workflow(
        &mut self,
        workflow_id: &str,
        workflow_type: String,
    ) -> Result<(), MonitoringError> {
        let now = SystemTime::now();
        let expected_duration = match workflow_type.as_str() {
            "pegin" => self.monitoring_config.performance_baseline.expected_pegin_duration,
            "pegout" => self.monitoring_config.performance_baseline.expected_pegout_duration,
            _ => Duration::from_secs(600), // Default
        };

        let monitoring_state = WorkflowMonitoringState {
            workflow_id: workflow_id.to_string(),
            workflow_type,
            started_at: now,
            last_heartbeat: now,
            expected_completion: now + expected_duration,
            status: WorkflowMonitoringStatus::Active,
            alert_level: AlertLevel::Info,
            performance_metrics: WorkflowPerformanceMetrics::default(),
            anomalies_detected: Vec::new(),
        };

        self.monitored_workflows.insert(workflow_id.to_string(), monitoring_state);
        self.monitoring_metrics.workflows_monitored += 1;

        info!("Started monitoring workflow: {}", workflow_id);
        Ok(())
    }

    /// Process monitoring tasks
    pub async fn process_monitoring(&mut self) -> Result<(), MonitoringError> {
        let monitoring_start = SystemTime::now();
        let now = SystemTime::now();

        // Check each monitored workflow - collect workflow IDs first to avoid borrow conflicts
        let workflow_ids: Vec<String> = self.monitored_workflows.keys().cloned().collect();
        let stall_threshold = self.monitoring_config.stall_detection_threshold;

        for workflow_id in workflow_ids {
            let mut alerts_to_generate = Vec::new();

            if let Some(monitoring_state) = self.monitored_workflows.get_mut(&workflow_id) {
                // Check for stalls
                let time_since_heartbeat = now.duration_since(monitoring_state.last_heartbeat)
                    .unwrap_or_default();

                if time_since_heartbeat > stall_threshold {
                    if monitoring_state.status != WorkflowMonitoringStatus::Stalled {
                        monitoring_state.status = WorkflowMonitoringStatus::Stalled;
                        alerts_to_generate.push((AlertType::WorkflowStalled, AlertLevel::Critical,
                            format!("Workflow {} has stalled (no heartbeat for {:?})", workflow_id, time_since_heartbeat)));
                    }
                }

                // Check for delays
                if now > monitoring_state.expected_completion {
                    if monitoring_state.status != WorkflowMonitoringStatus::Delayed
                        && monitoring_state.status != WorkflowMonitoringStatus::Stalled {
                        monitoring_state.status = WorkflowMonitoringStatus::Delayed;
                        let delay = now.duration_since(monitoring_state.expected_completion).unwrap_or_default();
                        alerts_to_generate.push((AlertType::PerformanceDegradation, AlertLevel::Warning,
                            format!("Workflow {} is delayed by {:?}", workflow_id, delay)));
                    }
                }

                // Update performance metrics while in scope
                monitoring_state.performance_metrics.execution_time = now
                    .duration_since(monitoring_state.started_at)
                    .unwrap_or_default();
            }

            // Generate alerts after updating state
            for (alert_type, level, message) in alerts_to_generate {
                self.generate_alert(&workflow_id, alert_type, level, message).await?;
            }
        }

        // Process alert escalations
        self.alert_manager.process_escalations().await?;

        // Update monitoring overhead
        let monitoring_duration = SystemTime::now().duration_since(monitoring_start).unwrap_or_default();
        self.monitoring_metrics.monitoring_overhead = monitoring_duration;

        Ok(())
    }

    /// Detect performance anomalies
    async fn detect_performance_anomalies(
        &mut self,
        workflow_id: &str,
        monitoring_state: &mut WorkflowMonitoringState,
    ) -> Result<(), MonitoringError> {
        let baseline = &self.monitoring_config.performance_baseline;
        let metrics = &monitoring_state.performance_metrics;

        // Check execution time anomaly
        let expected_duration = match monitoring_state.workflow_type.as_str() {
            "pegin" => baseline.expected_pegin_duration,
            "pegout" => baseline.expected_pegout_duration,
            _ => Duration::from_secs(600),
        };

        if metrics.execution_time > expected_duration * 2 {
            let anomaly = WorkflowAnomaly {
                anomaly_type: AnomalyType::UnexpectedDelay,
                detected_at: SystemTime::now(),
                severity: AnomalySeverity::High,
                description: format!("Execution time {} exceeds expected duration {} by 2x", 
                    metrics.execution_time.as_secs(), expected_duration.as_secs()),
                suggested_action: "Investigate workflow bottlenecks".to_string(),
            };
            monitoring_state.anomalies_detected.push(anomaly);
            self.monitoring_metrics.anomalies_detected += 1;
        }

        // Check error rate anomaly
        if metrics.error_count as f64 / metrics.execution_time.as_secs() as f64 > baseline.acceptable_error_rate {
            let anomaly = WorkflowAnomaly {
                anomaly_type: AnomalyType::HighErrorRate,
                detected_at: SystemTime::now(),
                severity: AnomalySeverity::Medium,
                description: format!("Error rate exceeds acceptable baseline: {} errors in {:?}", 
                    metrics.error_count, metrics.execution_time),
                suggested_action: "Review error logs and retry logic".to_string(),
            };
            monitoring_state.anomalies_detected.push(anomaly);
        }

        // Check excessive retries
        if metrics.retry_count > baseline.normal_retry_count * 3 {
            let anomaly = WorkflowAnomaly {
                anomaly_type: AnomalyType::ExcessiveRetries,
                detected_at: SystemTime::now(),
                severity: AnomalySeverity::Medium,
                description: format!("Retry count {} exceeds normal baseline {}", 
                    metrics.retry_count, baseline.normal_retry_count),
                suggested_action: "Investigate underlying causes of failures".to_string(),
            };
            monitoring_state.anomalies_detected.push(anomaly);
        }

        Ok(())
    }

    /// Generate alert
    async fn generate_alert(
        &mut self,
        workflow_id: &str,
        alert_type: AlertType,
        level: AlertLevel,
        message: String,
    ) -> Result<(), MonitoringError> {
        let alert_id = format!("alert_{}", uuid::Uuid::new_v4());
        
        let alert = WorkflowAlert {
            alert_id: alert_id.clone(),
            workflow_id: workflow_id.to_string(),
            alert_type,
            level: level.clone(),
            message: message.clone(),
            created_at: SystemTime::now(),
            acknowledged: false,
            resolved: false,
        };

        // Log alert
        match level {
            AlertLevel::Info => info!("Workflow Alert [{}]: {}", workflow_id, message),
            AlertLevel::Warning => warn!("Workflow Alert [{}]: {}", workflow_id, message),
            AlertLevel::Critical | AlertLevel::Emergency => {
                error!("Workflow Alert [{}]: {}", workflow_id, message);
            }
        }

        // Store alert
        self.alert_manager.active_alerts.insert(alert_id, alert);
        self.monitoring_metrics.alerts_generated += 1;

        // Send notifications if enabled
        if self.alert_manager.alert_config.enable_notifications {
            self.alert_manager.send_notifications(&message, &level).await?;
        }

        Ok(())
    }

    /// Complete workflow monitoring
    pub fn complete_workflow_monitoring(&mut self, workflow_id: &str) -> Result<(), MonitoringError> {
        if let Some(mut monitoring_state) = self.monitored_workflows.remove(workflow_id) {
            monitoring_state.status = WorkflowMonitoringStatus::Completed;
            
            // Update average duration metrics
            let duration = monitoring_state.performance_metrics.execution_time;
            let current_avg = self.monitoring_metrics.average_workflow_duration;
            let completed_count = self.monitoring_metrics.workflows_monitored;
            
            if completed_count > 0 {
                let total_time = current_avg * (completed_count - 1) as u32;
                self.monitoring_metrics.average_workflow_duration = (total_time + duration) / completed_count as u32;
            }

            info!("Completed monitoring for workflow {} in {:?}", workflow_id, duration);
        }

        Ok(())
    }

    /// Validate monitoring configuration
    fn validate_config(&self) -> Result<(), MonitoringError> {
        if self.monitoring_config.heartbeat_interval > self.monitoring_config.stall_detection_threshold {
            return Err(MonitoringError::InvalidConfiguration(
                "Heartbeat interval cannot be greater than stall detection threshold".to_string()
            ));
        }

        if self.monitoring_config.max_acceptable_error_rate > 1.0 {
            return Err(MonitoringError::InvalidConfiguration(
                "Max acceptable error rate cannot exceed 1.0".to_string()
            ));
        }

        Ok(())
    }

    /// Get monitoring statistics
    pub fn get_monitoring_statistics(&self) -> MonitoringStatistics {
        MonitoringStatistics {
            active_workflows: self.monitored_workflows.len(),
            total_workflows_monitored: self.monitoring_metrics.workflows_monitored,
            active_alerts: self.alert_manager.active_alerts.len(),
            total_alerts_generated: self.monitoring_metrics.alerts_generated,
            anomalies_detected: self.monitoring_metrics.anomalies_detected,
            average_workflow_duration: self.monitoring_metrics.average_workflow_duration,
            monitoring_overhead: self.monitoring_metrics.monitoring_overhead,
        }
    }
}

impl AlertManager {
    /// Initialize alert manager
    async fn initialize(&mut self) -> Result<(), MonitoringError> {
        info!("Initializing alert manager");
        
        // Validate notification channels
        for channel in &self.alert_config.notification_channels {
            match channel {
                NotificationChannel::Log => {
                    info!("Log notification channel enabled");
                }
                NotificationChannel::Email(email) => {
                    info!("Email notification channel enabled: {}", email);
                }
                NotificationChannel::Webhook(url) => {
                    info!("Webhook notification channel enabled: {}", url);
                }
                NotificationChannel::Slack(channel) => {
                    info!("Slack notification channel enabled: {}", channel);
                }
            }
        }

        Ok(())
    }

    /// Process alert escalations
    async fn process_escalations(&mut self) -> Result<(), MonitoringError> {
        let now = SystemTime::now();
        let mut alerts_to_escalate = Vec::new();

        for (alert_id, alert) in &self.active_alerts {
            for escalation_rule in &self.alert_config.escalation_rules {
                let should_escalate = match &escalation_rule.trigger_condition {
                    EscalationCondition::UnacknowledgedAfter(duration) => {
                        !alert.acknowledged && now.duration_since(alert.created_at).unwrap_or_default() >= *duration
                    }
                    EscalationCondition::RepeatedAlerts(count) => {
                        // Check if we have multiple unresolved alerts for the same workflow
                        let workflow_alerts: Vec<_> = self.active_alerts.values()
                            .filter(|a| a.workflow_id == alert.workflow_id && !a.resolved)
                            .collect();
                        workflow_alerts.len() >= *count as usize
                    }
                    EscalationCondition::CriticalSystemState => {
                        // This would check overall system health
                        false // Placeholder
                    }
                };

                if should_escalate {
                    alerts_to_escalate.push((alert_id.clone(), escalation_rule.target_level.clone()));
                }
            }
        }

        // Escalate alerts
        for (alert_id, new_level) in alerts_to_escalate {
            if let Some(alert) = self.active_alerts.get_mut(&alert_id) {
                if alert.level != new_level {
                    warn!("Escalating alert {} from {:?} to {:?}", alert_id, alert.level, new_level);
                    alert.level = new_level;
                }
            }
        }

        Ok(())
    }

    /// Send notifications
    async fn send_notifications(&self, message: &str, level: &AlertLevel) -> Result<(), MonitoringError> {
        for channel in &self.alert_config.notification_channels {
            match channel {
                NotificationChannel::Log => {
                    // Already logged in generate_alert
                }
                NotificationChannel::Email(email) => {
                    // In a real implementation, this would send an email
                    info!("Would send email to {}: {}", email, message);
                }
                NotificationChannel::Webhook(url) => {
                    // In a real implementation, this would make HTTP request
                    info!("Would send webhook to {}: {}", url, message);
                }
                NotificationChannel::Slack(channel) => {
                    // In a real implementation, this would send to Slack
                    info!("Would send Slack message to {}: {}", channel, message);
                }
            }
        }

        Ok(())
    }
}

/// Monitoring statistics
#[derive(Debug)]
pub struct MonitoringStatistics {
    pub active_workflows: usize,
    pub total_workflows_monitored: u64,
    pub active_alerts: usize,
    pub total_alerts_generated: u64,
    pub anomalies_detected: u64,
    pub average_workflow_duration: Duration,
    pub monitoring_overhead: Duration,
}

/// Monitoring errors
#[derive(Debug, thiserror::Error)]
pub enum MonitoringError {
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    
    #[error("Alert generation failed: {0}")]
    AlertGenerationFailed(String),
    
    #[error("Notification failed: {0}")]
    NotificationFailed(String),
    
    #[error("Monitoring processing failed: {0}")]
    ProcessingFailed(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}