//! Recovery Coordination
//! 
//! Coordinates actor recovery and restart operations

use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use tracing::{info, warn, error};
use super::{ActorId, RestartStrategy};

/// Recovery coordinator for failed actors
#[derive(Debug)]
pub struct RecoveryCoordinator {
    max_restart_attempts: u32,
    active_recoveries: HashMap<ActorId, RecoveryOperation>,
    recovery_history: Vec<RecoveryRecord>,
}

/// Recovery operation tracking
#[derive(Debug, Clone)]
pub struct RecoveryOperation {
    pub actor_id: ActorId,
    pub strategy: RestartStrategy,
    pub attempt_count: u32,
    pub started_at: SystemTime,
    pub last_attempt: SystemTime,
    pub status: RecoveryStatus,
}

/// Recovery status
#[derive(Debug, Clone)]
pub enum RecoveryStatus {
    Initiated,
    InProgress,
    WaitingForRestart,
    Completed,
    Failed,
}

/// Recovery record for history
#[derive(Debug, Clone)]
pub struct RecoveryRecord {
    pub actor_id: ActorId,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub success: bool,
    pub attempt_count: u32,
    pub total_duration: Option<Duration>,
}

impl RecoveryCoordinator {
    pub fn new(max_restart_attempts: u32) -> Self {
        Self {
            max_restart_attempts,
            active_recoveries: HashMap::new(),
            recovery_history: Vec::new(),
        }
    }

    /// Initiate recovery for failed actor
    pub fn initiate_recovery(&mut self, actor_id: ActorId, strategy: RestartStrategy) {
        info!("Initiating recovery for actor {:?}", actor_id);

        // Check if already recovering
        if self.active_recoveries.contains_key(&actor_id) {
            warn!("Recovery already in progress for actor {:?}", actor_id);
            return;
        }

        let recovery_operation = RecoveryOperation {
            actor_id: actor_id.clone(),
            strategy,
            attempt_count: 0,
            started_at: SystemTime::now(),
            last_attempt: SystemTime::now(),
            status: RecoveryStatus::Initiated,
        };

        self.active_recoveries.insert(actor_id, recovery_operation);
    }

    /// Process recovery operations
    pub fn process_recoveries(&mut self) -> Vec<ActorId> {
        let mut completed_recoveries = Vec::new();
        let now = SystemTime::now();

        for (actor_id, operation) in &mut self.active_recoveries {
            match operation.status {
                RecoveryStatus::Initiated => {
                    operation.status = RecoveryStatus::InProgress;
                    operation.attempt_count += 1;
                    operation.last_attempt = now;
                    info!("Starting recovery attempt {} for actor {:?}", 
                          operation.attempt_count, actor_id);
                }
                RecoveryStatus::InProgress => {
                    // Check if restart delay has passed
                    let restart_delay = self.get_restart_delay(&operation.strategy, operation.attempt_count);
                    if now.duration_since(operation.last_attempt).unwrap_or_default() >= restart_delay {
                        operation.status = RecoveryStatus::WaitingForRestart;
                        info!("Ready to restart actor {:?}", actor_id);
                    }
                }
                RecoveryStatus::WaitingForRestart => {
                    // Attempt to restart actor
                    if self.attempt_actor_restart(actor_id) {
                        operation.status = RecoveryStatus::Completed;
                        completed_recoveries.push(actor_id.clone());
                        info!("Successfully recovered actor {:?}", actor_id);
                    } else if operation.attempt_count >= self.max_restart_attempts {
                        operation.status = RecoveryStatus::Failed;
                        completed_recoveries.push(actor_id.clone());
                        error!("Failed to recover actor {:?} after {} attempts", 
                               actor_id, operation.attempt_count);
                    } else {
                        // Retry with backoff
                        operation.status = RecoveryStatus::InProgress;
                        operation.attempt_count += 1;
                        operation.last_attempt = now;
                        warn!("Recovery attempt {} failed for actor {:?}, retrying", 
                              operation.attempt_count, actor_id);
                    }
                }
                _ => {} // Already completed or failed
            }
        }

        // Clean up completed recoveries
        for actor_id in &completed_recoveries {
            if let Some(operation) = self.active_recoveries.remove(actor_id) {
                self.record_recovery_completion(operation);
            }
        }

        completed_recoveries
    }

    /// Attempt to restart an actor
    fn attempt_actor_restart(&self, actor_id: &ActorId) -> bool {
        // This is simplified - in practice would restart the actual actor
        info!("Attempting to restart actor {:?}", actor_id);
        
        // Simulate restart success/failure
        match actor_id {
            ActorId::Bridge => {
                // Bridge actor restart logic
                true // Assume success for demo
            }
            ActorId::PegIn => {
                // PegIn actor restart logic
                true // Assume success for demo
            }
            ActorId::PegOut => {
                // PegOut actor restart logic
                true // Assume success for demo
            }
            ActorId::Stream => {
                // Stream actor restart logic
                true // Assume success for demo
            }
        }
    }

    /// Get restart delay based on strategy
    fn get_restart_delay(&self, strategy: &RestartStrategy, attempt_count: u32) -> Duration {
        match strategy {
            RestartStrategy::ImmediateRestart => Duration::from_secs(0),
            RestartStrategy::ExponentialBackoff { base_delay, max_delay, .. } => {
                let delay = *base_delay * 2_u32.pow(attempt_count.min(8));
                delay.min(*max_delay)
            }
            RestartStrategy::CircuitBreaker { recovery_timeout, failure_threshold, .. } => {
                if attempt_count >= *failure_threshold {
                    *recovery_timeout
                } else {
                    Duration::from_secs(1)
                }
            }
            RestartStrategy::GracefulRestart { drain_timeout } => *drain_timeout,
        }
    }

    /// Record recovery completion
    fn record_recovery_completion(&mut self, operation: RecoveryOperation) {
        let now = SystemTime::now();
        let success = matches!(operation.status, RecoveryStatus::Completed);
        let total_duration = now.duration_since(operation.started_at).ok();

        let record = RecoveryRecord {
            actor_id: operation.actor_id,
            started_at: operation.started_at,
            completed_at: Some(now),
            success,
            attempt_count: operation.attempt_count,
            total_duration,
        };

        self.recovery_history.push(record);

        // Keep only recent history
        if self.recovery_history.len() > 100 {
            self.recovery_history.drain(0..10);
        }
    }

    /// Get recovery statistics
    pub fn get_recovery_stats(&self) -> RecoveryStats {
        let total_recoveries = self.recovery_history.len();
        let successful_recoveries = self.recovery_history.iter()
            .filter(|r| r.success)
            .count();

        let average_duration = if total_recoveries > 0 {
            let total_duration: Duration = self.recovery_history.iter()
                .filter_map(|r| r.total_duration)
                .sum();
            total_duration / total_recoveries as u32
        } else {
            Duration::from_secs(0)
        };

        RecoveryStats {
            total_recoveries: total_recoveries as u64,
            successful_recoveries: successful_recoveries as u64,
            success_rate: if total_recoveries > 0 {
                successful_recoveries as f64 / total_recoveries as f64
            } else {
                0.0
            },
            average_recovery_time: average_duration,
            active_recoveries: self.active_recoveries.len() as u32,
        }
    }

    /// Check if actor is currently recovering
    pub fn is_recovering(&self, actor_id: &ActorId) -> bool {
        self.active_recoveries.contains_key(actor_id)
    }
}

/// Recovery statistics
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    pub total_recoveries: u64,
    pub successful_recoveries: u64,
    pub success_rate: f64,
    pub average_recovery_time: Duration,
    pub active_recoveries: u32,
}