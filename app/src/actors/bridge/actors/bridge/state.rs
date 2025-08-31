//! Bridge Actor State Management
//! 
//! State structures and management for the bridge coordinator

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use crate::actors::bridge::messages::*;
use crate::types::*;

/// Bridge coordinator state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeState {
    /// System is initializing
    Initializing,
    /// System is running normally
    Running,
    /// System is in degraded state
    Degraded { issues: Vec<String> },
    /// System is shutting down
    ShuttingDown,
    /// System has stopped
    Stopped,
}

/// Actor health monitoring
#[derive(Debug)]
pub struct ActorHealthMonitor {
    health_check_interval: std::time::Duration,
    last_health_check: SystemTime,
    actor_health_status: HashMap<ActorType, ActorHealthInfo>,
    system_errors: Vec<SystemError>,
    last_error: Option<String>,
}

/// Actor health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorHealthInfo {
    pub actor_type: ActorType,
    pub status: ActorStatus,
    pub last_heartbeat: SystemTime,
    pub failure_count: u32,
    pub last_failure: Option<SystemTime>,
    pub response_time: Option<std::time::Duration>,
}

/// System error tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemError {
    pub error_type: SystemErrorType,
    pub message: String,
    pub actor_type: Option<ActorType>,
    pub operation_id: Option<String>,
    pub occurred_at: SystemTime,
    pub resolved_at: Option<SystemTime>,
}

/// Types of system errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemErrorType {
    ActorFailure,
    OperationTimeout,
    NetworkError,
    ValidationError,
    InsufficientFunds,
    SignatureFailure,
    Other(String),
}

impl ActorHealthMonitor {
    /// Create new health monitor
    pub fn new(health_check_interval: std::time::Duration) -> Self {
        Self {
            health_check_interval,
            last_health_check: SystemTime::now(),
            actor_health_status: HashMap::new(),
            system_errors: Vec::new(),
            last_error: None,
        }
    }

    /// Record actor registration
    pub fn register_actor(&mut self, actor_type: ActorType) {
        let health_info = ActorHealthInfo {
            actor_type: actor_type.clone(),
            status: ActorStatus::Running,
            last_heartbeat: SystemTime::now(),
            failure_count: 0,
            last_failure: None,
            response_time: None,
        };
        
        self.actor_health_status.insert(actor_type, health_info);
    }

    /// Record actor heartbeat
    pub fn record_heartbeat(&mut self, actor_type: ActorType, response_time: std::time::Duration) {
        if let Some(health_info) = self.actor_health_status.get_mut(&actor_type) {
            health_info.last_heartbeat = SystemTime::now();
            health_info.response_time = Some(response_time);
            
            // Update status based on health
            health_info.status = if response_time.as_millis() > 1000 {
                ActorStatus::Degraded
            } else {
                ActorStatus::Running
            };
        }
    }

    /// Record actor failure
    pub fn record_actor_failure(&mut self, actor_type: ActorType) {
        if let Some(health_info) = self.actor_health_status.get_mut(&actor_type) {
            health_info.failure_count += 1;
            health_info.last_failure = Some(SystemTime::now());
            health_info.status = ActorStatus::Failed;
        }

        // Record system error
        let error = SystemError {
            error_type: SystemErrorType::ActorFailure,
            message: format!("Actor {:?} failed", actor_type),
            actor_type: Some(actor_type),
            operation_id: None,
            occurred_at: SystemTime::now(),
            resolved_at: None,
        };

        self.system_errors.push(error);
        self.last_error = Some(format!("Actor {:?} failed", actor_type));
    }

    /// Check system health
    pub fn check_system_health(&mut self) -> SystemHealthStatus {
        self.last_health_check = SystemTime::now();
        
        let mut issues = Vec::new();
        let mut critical_issues = Vec::new();

        // Check each actor's health
        for (actor_type, health_info) in &self.actor_health_status {
            let time_since_heartbeat = SystemTime::now()
                .duration_since(health_info.last_heartbeat)
                .unwrap_or_default();

            if time_since_heartbeat > self.health_check_interval * 3 {
                critical_issues.push(format!("Actor {:?} not responding", actor_type));
            } else if time_since_heartbeat > self.health_check_interval * 2 {
                issues.push(format!("Actor {:?} delayed response", actor_type));
            }

            if health_info.failure_count > 3 {
                critical_issues.push(format!("Actor {:?} has high failure count", actor_type));
            }
        }

        // Check recent errors
        let recent_errors: Vec<&SystemError> = self.system_errors.iter()
            .filter(|e| {
                let time_since = SystemTime::now()
                    .duration_since(e.occurred_at)
                    .unwrap_or_default();
                time_since.as_secs() < 300 // Last 5 minutes
            })
            .collect();

        if recent_errors.len() > 10 {
            critical_issues.push("High error rate detected".to_string());
        } else if recent_errors.len() > 5 {
            issues.push("Elevated error rate".to_string());
        }

        // Determine overall health status
        if !critical_issues.is_empty() {
            SystemHealthStatus::Critical { errors: critical_issues }
        } else if !issues.is_empty() {
            SystemHealthStatus::Degraded { issues }
        } else {
            SystemHealthStatus::Healthy
        }
    }

    /// Get actor health status
    pub fn get_actor_health(&self, actor_type: &ActorType) -> Option<&ActorHealthInfo> {
        self.actor_health_status.get(actor_type)
    }

    /// Get recent errors
    pub fn get_recent_errors(&self, limit: usize) -> Vec<&SystemError> {
        self.system_errors
            .iter()
            .rev()
            .take(limit)
            .collect()
    }

    /// Get last error message
    pub fn get_last_error(&self) -> Option<String> {
        self.last_error.clone()
    }

    /// Clear resolved errors
    pub fn clear_resolved_errors(&mut self) {
        self.system_errors.retain(|error| error.resolved_at.is_none());
    }

    /// Mark error as resolved
    pub fn resolve_error(&mut self, error_index: usize) {
        if let Some(error) = self.system_errors.get_mut(error_index) {
            error.resolved_at = Some(SystemTime::now());
        }
    }
}

impl Default for BridgeState {
    fn default() -> Self {
        Self::Initializing
    }
}

impl BridgeState {
    /// Check if bridge is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, BridgeState::Running)
    }

    /// Check if bridge can accept new operations
    pub fn can_accept_operations(&self) -> bool {
        matches!(self, BridgeState::Running)
    }

    /// Get state description
    pub fn description(&self) -> String {
        match self {
            BridgeState::Initializing => "System is starting up".to_string(),
            BridgeState::Running => "System is operating normally".to_string(),
            BridgeState::Degraded { issues } => {
                format!("System is degraded: {}", issues.join(", "))
            }
            BridgeState::ShuttingDown => "System is shutting down".to_string(),
            BridgeState::Stopped => "System has stopped".to_string(),
        }
    }
}

/// State persistence utilities
pub mod persistence {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Save bridge state to disk
    pub fn save_state(state: &BridgeState, path: &Path) -> Result<(), std::io::Error> {
        let serialized = serde_json::to_string_pretty(state)?;
        fs::write(path, serialized)?;
        Ok(())
    }

    /// Load bridge state from disk
    pub fn load_state(path: &Path) -> Result<BridgeState, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let state = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Save health monitor state
    pub fn save_health_data(
        health_info: &HashMap<ActorType, ActorHealthInfo>,
        path: &Path,
    ) -> Result<(), std::io::Error> {
        let serialized = serde_json::to_string_pretty(health_info)?;
        fs::write(path, serialized)?;
        Ok(())
    }

    /// Load health monitor state
    pub fn load_health_data(
        path: &Path,
    ) -> Result<HashMap<ActorType, ActorHealthInfo>, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let health_info = serde_json::from_str(&content)?;
        Ok(health_info)
    }
}