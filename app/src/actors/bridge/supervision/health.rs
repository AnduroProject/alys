//! Health Monitoring
//! 
//! Health monitoring utilities for supervised actors

use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use tracing::{debug, warn};
use super::{ActorId, HealthStatus};

/// Health monitor for supervision
#[derive(Debug)]
pub struct SupervisionHealthMonitor {
    check_interval: Duration,
    last_health_checks: HashMap<ActorId, SystemTime>,
    health_history: HashMap<ActorId, Vec<HealthCheckResult>>,
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub timestamp: SystemTime,
    pub status: HealthStatus,
    pub response_time: Option<Duration>,
    pub error_message: Option<String>,
}

impl SupervisionHealthMonitor {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            check_interval,
            last_health_checks: HashMap::new(),
            health_history: HashMap::new(),
        }
    }

    /// Check actor health
    pub fn check_actor_health(&mut self, actor_id: &ActorId) -> HealthStatus {
        let now = SystemTime::now();
        
        // Record check time
        self.last_health_checks.insert(actor_id.clone(), now);

        // Simulate health check (in practice, this would ping the actor)
        let status = self.perform_health_check(actor_id);
        
        // Record result
        let result = HealthCheckResult {
            timestamp: now,
            status: status.clone(),
            response_time: Some(Duration::from_millis(10)), // Simulated
            error_message: None,
        };
        
        self.health_history.entry(actor_id.clone())
            .or_insert_with(Vec::new)
            .push(result);

        // Keep only recent history
        if let Some(history) = self.health_history.get_mut(actor_id) {
            if history.len() > 100 {
                history.drain(0..10);
            }
        }

        debug!("Health check for {:?}: {:?}", actor_id, status);
        status
    }

    /// Perform actual health check
    fn perform_health_check(&self, actor_id: &ActorId) -> HealthStatus {
        // This is simplified - in practice would send health check message to actor
        match actor_id {
            ActorId::Bridge => {
                // Check if bridge coordinator is responsive
                if self.is_actor_responsive(actor_id) {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded
                }
            }
            ActorId::PegIn => {
                // Check if PegIn actor is processing deposits
                if self.is_actor_responsive(actor_id) {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded
                }
            }
            ActorId::PegOut => {
                // Check if PegOut actor is processing withdrawals
                if self.is_actor_responsive(actor_id) {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded
                }
            }
            ActorId::Stream => {
                // Check if Stream actor has governance connections
                if self.is_actor_responsive(actor_id) {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded
                }
            }
        }
    }

    /// Check if actor is responsive (simplified)
    fn is_actor_responsive(&self, _actor_id: &ActorId) -> bool {
        // Simplified check - in practice would verify actor is responding to messages
        true
    }

    /// Get health history for actor
    pub fn get_health_history(&self, actor_id: &ActorId) -> Option<&Vec<HealthCheckResult>> {
        self.health_history.get(actor_id)
    }

    /// Get health trend for actor
    pub fn get_health_trend(&self, actor_id: &ActorId) -> HealthTrend {
        if let Some(history) = self.health_history.get(actor_id) {
            if history.len() < 2 {
                return HealthTrend::Stable;
            }

            let recent_count = 5.min(history.len());
            let recent_healthy = history.iter()
                .rev()
                .take(recent_count)
                .filter(|r| matches!(r.status, HealthStatus::Healthy))
                .count();

            let health_ratio = recent_healthy as f64 / recent_count as f64;

            if health_ratio > 0.8 {
                HealthTrend::Improving
            } else if health_ratio < 0.4 {
                HealthTrend::Declining
            } else {
                HealthTrend::Stable
            }
        } else {
            HealthTrend::Stable
        }
    }
}

/// Health trend indicators
#[derive(Debug, Clone)]
pub enum HealthTrend {
    Improving,
    Stable,
    Declining,
}