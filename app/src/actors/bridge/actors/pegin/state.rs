//! PegIn Actor State Management
//! 
//! State structures and management for PegIn operations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use bitcoin::Txid;
use ethereum_types::{H160, H256};
use crate::actors::bridge::messages::*;

/// PegIn actor state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegInState {
    /// Actor is initializing
    Initializing,
    /// Actor is monitoring blockchain
    Monitoring,
    /// Actor is in degraded state
    Degraded { issues: Vec<String> },
    /// Actor is paused
    Paused,
    /// Actor is stopping
    Stopping,
    /// Actor has stopped
    Stopped,
}

/// Operation tracker for performance monitoring
#[derive(Debug)]
pub struct OperationTracker {
    /// Operation start times
    operation_start_times: HashMap<String, SystemTime>,
    
    /// Completed operation durations
    operation_durations: Vec<Duration>,
    
    /// Operation success/failure counts
    success_count: u64,
    failure_count: u64,
    
    /// Performance statistics
    performance_stats: PerformanceStats,
    
    /// Operation timeline
    operation_timeline: Vec<OperationEvent>,
}

/// Performance statistics for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub average_processing_time: Duration,
    pub median_processing_time: Duration,
    pub p95_processing_time: Duration,
    pub p99_processing_time: Duration,
    pub success_rate: f64,
    pub operations_per_minute: f64,
    pub peak_concurrent_operations: u32,
    pub last_updated: SystemTime,
}

/// Operation event for timeline tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationEvent {
    pub operation_id: String,
    pub operation_type: OperationEventType,
    pub timestamp: SystemTime,
    pub duration: Option<Duration>,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Types of operation events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationEventType {
    DepositDetected,
    ValidationStarted,
    ValidationCompleted,
    ConfirmationStarted,
    ConfirmationUpdated,
    ConfirmationCompleted,
    MintingInitiated,
    MintingCompleted,
    OperationFailed,
    OperationRetried,
}

/// PegIn actor metrics state
#[derive(Debug, Clone)]
pub struct PegInMetrics {
    /// Actor start time
    pub start_time: SystemTime,
    
    /// Deposit counters
    deposits_detected: u64,
    deposits_validated: u64,
    deposits_confirmed: u64,
    deposits_completed: u64,
    deposits_failed: u64,
    deposits_cancelled: u64,
    
    /// Processing time statistics
    average_validation_time: Duration,
    average_confirmation_time: Duration,
    
    /// Error counters
    validation_errors: u64,
    network_errors: u64,
    timeout_errors: u64,
    
    /// System counters
    actor_restarts: u64,
    config_updates: u64,
    
    /// Blockchain monitoring stats
    blocks_processed: u64,
    last_block_processed: u64,
    
    /// Performance metrics
    operations_per_second: f64,
    peak_memory_usage: u64,
    
    /// Health indicators
    last_successful_operation: Option<SystemTime>,
    consecutive_failures: u32,
    health_score: f64,
}

impl OperationTracker {
    /// Create new operation tracker
    pub fn new() -> Self {
        Self {
            operation_start_times: HashMap::new(),
            operation_durations: Vec::new(),
            success_count: 0,
            failure_count: 0,
            performance_stats: PerformanceStats::default(),
            operation_timeline: Vec::new(),
        }
    }

    /// Start tracking an operation
    pub fn start_operation(&mut self, operation_id: String) {
        self.operation_start_times.insert(operation_id, SystemTime::now());
    }

    /// Complete an operation successfully
    pub fn complete_operation(&mut self, operation_id: String, operation_type: OperationEventType) -> Option<Duration> {
        if let Some(start_time) = self.operation_start_times.remove(&operation_id) {
            let duration = SystemTime::now().duration_since(start_time).unwrap_or_default();
            self.operation_durations.push(duration);
            self.success_count += 1;

            // Record event
            let event = OperationEvent {
                operation_id,
                operation_type,
                timestamp: SystemTime::now(),
                duration: Some(duration),
                success: true,
                error_message: None,
            };
            self.operation_timeline.push(event);

            // Update performance stats
            self.update_performance_stats();

            Some(duration)
        } else {
            None
        }
    }

    /// Fail an operation
    pub fn fail_operation(&mut self, operation_id: String, operation_type: OperationEventType, error_message: String) {
        let duration = self.operation_start_times.remove(&operation_id)
            .and_then(|start_time| SystemTime::now().duration_since(start_time).ok());

        self.failure_count += 1;

        // Record event
        let event = OperationEvent {
            operation_id,
            operation_type,
            timestamp: SystemTime::now(),
            duration,
            success: false,
            error_message: Some(error_message),
        };
        self.operation_timeline.push(event);

        // Update performance stats
        self.update_performance_stats();
    }

    /// Update performance statistics
    fn update_performance_stats(&mut self) {
        if self.operation_durations.is_empty() {
            return;
        }

        let mut sorted_durations = self.operation_durations.clone();
        sorted_durations.sort();

        let total_duration: Duration = sorted_durations.iter().sum();
        let count = sorted_durations.len();

        self.performance_stats.average_processing_time = total_duration / count as u32;
        self.performance_stats.median_processing_time = sorted_durations[count / 2];
        self.performance_stats.p95_processing_time = sorted_durations[(count * 95) / 100];
        self.performance_stats.p99_processing_time = sorted_durations[(count * 99) / 100];
        
        let total_operations = self.success_count + self.failure_count;
        self.performance_stats.success_rate = if total_operations > 0 {
            self.success_count as f64 / total_operations as f64
        } else {
            0.0
        };

        self.performance_stats.last_updated = SystemTime::now();

        // Keep only recent durations for memory efficiency
        if self.operation_durations.len() > 1000 {
            self.operation_durations.drain(0..100);
        }

        // Keep only recent timeline events
        if self.operation_timeline.len() > 1000 {
            self.operation_timeline.drain(0..100);
        }
    }

    /// Get current performance statistics
    pub fn get_performance_stats(&self) -> PerformanceStats {
        self.performance_stats.clone()
    }

    /// Get operation timeline
    pub fn get_operation_timeline(&self, limit: Option<usize>) -> Vec<OperationEvent> {
        match limit {
            Some(limit) => self.operation_timeline.iter().rev().take(limit).cloned().collect(),
            None => self.operation_timeline.clone(),
        }
    }

    /// Get active operations count
    pub fn get_active_operations_count(&self) -> usize {
        self.operation_start_times.len()
    }

    /// Get total operations count
    pub fn get_total_operations_count(&self) -> u64 {
        self.success_count + self.failure_count
    }

    /// Get success rate
    pub fn get_success_rate(&self) -> f64 {
        self.performance_stats.success_rate
    }
}

impl PegInMetrics {
    /// Create new metrics instance
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            start_time: SystemTime::now(),
            deposits_detected: 0,
            deposits_validated: 0,
            deposits_confirmed: 0,
            deposits_completed: 0,
            deposits_failed: 0,
            deposits_cancelled: 0,
            average_validation_time: Duration::from_secs(0),
            average_confirmation_time: Duration::from_secs(0),
            validation_errors: 0,
            network_errors: 0,
            timeout_errors: 0,
            actor_restarts: 0,
            config_updates: 0,
            blocks_processed: 0,
            last_block_processed: 0,
            operations_per_second: 0.0,
            peak_memory_usage: 0,
            last_successful_operation: None,
            consecutive_failures: 0,
            health_score: 100.0,
        })
    }

    /// Record actor started
    pub fn record_actor_started(&mut self) {
        self.start_time = SystemTime::now();
        self.health_score = 100.0;
    }

    /// Record actor stopped
    pub fn record_actor_stopped(&mut self) {
        // Final metrics update could be added here
    }

    /// Record deposit detected
    pub fn record_deposit_detected(&mut self) {
        self.deposits_detected += 1;
        self.last_successful_operation = Some(SystemTime::now());
        self.consecutive_failures = 0;
        self.update_health_score();
    }

    /// Record deposit validated
    pub fn record_deposit_validated(&mut self) {
        self.deposits_validated += 1;
        self.last_successful_operation = Some(SystemTime::now());
        self.consecutive_failures = 0;
        self.update_health_score();
    }

    /// Record deposit confirmed
    pub fn record_deposit_confirmed(&mut self) {
        self.deposits_confirmed += 1;
        self.last_successful_operation = Some(SystemTime::now());
        self.consecutive_failures = 0;
        self.update_health_score();
    }

    /// Record deposit completed
    pub fn record_deposit_completed(&mut self) {
        self.deposits_completed += 1;
        self.last_successful_operation = Some(SystemTime::now());
        self.consecutive_failures = 0;
        self.update_health_score();
    }

    /// Record deposit failed
    pub fn record_deposit_failed(&mut self) {
        self.deposits_failed += 1;
        self.consecutive_failures += 1;
        self.update_health_score();
    }

    /// Record deposit cancelled
    pub fn record_deposit_cancelled(&mut self) {
        self.deposits_cancelled += 1;
    }

    /// Record minting initiated
    pub fn record_minting_initiated(&mut self) {
        // This could track minting-specific metrics
    }

    /// Record invalid deposit
    pub fn record_invalid_deposit(&mut self) {
        self.validation_errors += 1;
        self.consecutive_failures += 1;
        self.update_health_score();
    }

    /// Record blocks processed
    pub fn record_blocks_processed(&mut self, count: u64) {
        self.blocks_processed += count;
        self.last_block_processed = count; // This should be the actual block height
    }

    /// Record error
    pub fn record_error(&mut self, error: &super::actor::PegInError) {
        match error {
            super::actor::PegInError::ValidationError(_) => self.validation_errors += 1,
            super::actor::PegInError::BitcoinRpcError(_) => self.network_errors += 1,
            super::actor::PegInError::OperationTimeout(_) => self.timeout_errors += 1,
            _ => {} // Other errors
        }
        
        self.consecutive_failures += 1;
        self.update_health_score();
    }

    /// Record configuration update
    pub fn record_config_update(&mut self) {
        self.config_updates += 1;
    }

    /// Record max retries exceeded
    pub fn record_max_retries_exceeded(&mut self) {
        self.deposits_failed += 1;
        self.consecutive_failures += 1;
        self.update_health_score();
    }

    /// Update health score based on recent performance
    fn update_health_score(&mut self) {
        // Start with base score
        let mut score = 100.0;

        // Reduce score based on consecutive failures
        if self.consecutive_failures > 0 {
            score -= (self.consecutive_failures as f64) * 10.0;
        }

        // Reduce score based on error rates
        let total_operations = self.deposits_detected;
        if total_operations > 0 {
            let error_rate = (self.deposits_failed + self.validation_errors) as f64 / total_operations as f64;
            score -= error_rate * 50.0; // Max 50 points for error rate
        }

        // Check if recent activity exists
        if let Some(last_success) = self.last_successful_operation {
            if let Ok(time_since) = SystemTime::now().duration_since(last_success) {
                if time_since > Duration::from_secs(3600) { // 1 hour
                    score -= 20.0; // No recent successful operations
                }
            }
        } else {
            score -= 30.0; // Never had successful operations
        }

        // Ensure score is between 0 and 100
        self.health_score = score.max(0.0).min(100.0);
    }

    /// Get deposits processed count
    pub fn get_deposits_processed(&self) -> u64 {
        self.deposits_detected
    }

    /// Get current health score
    pub fn get_health_score(&self) -> f64 {
        self.health_score
    }

    /// Get uptime
    pub fn get_uptime(&self) -> Duration {
        SystemTime::now().duration_since(self.start_time).unwrap_or_default()
    }

    /// Get success rate
    pub fn get_success_rate(&self) -> f64 {
        if self.deposits_detected > 0 {
            self.deposits_completed as f64 / self.deposits_detected as f64
        } else {
            0.0
        }
    }

    /// Get error rate
    pub fn get_error_rate(&self) -> f64 {
        if self.deposits_detected > 0 {
            self.deposits_failed as f64 / self.deposits_detected as f64
        } else {
            0.0
        }
    }

    /// Get metrics snapshot
    pub fn get_snapshot(&self) -> PegInMetricsSnapshot {
        PegInMetricsSnapshot {
            uptime: self.get_uptime(),
            deposits_detected: self.deposits_detected,
            deposits_validated: self.deposits_validated,
            deposits_confirmed: self.deposits_confirmed,
            deposits_completed: self.deposits_completed,
            deposits_failed: self.deposits_failed,
            success_rate: self.get_success_rate(),
            error_rate: self.get_error_rate(),
            health_score: self.health_score,
            blocks_processed: self.blocks_processed,
            last_block_processed: self.last_block_processed,
            consecutive_failures: self.consecutive_failures,
            last_successful_operation: self.last_successful_operation,
        }
    }
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegInMetricsSnapshot {
    pub uptime: Duration,
    pub deposits_detected: u64,
    pub deposits_validated: u64,
    pub deposits_confirmed: u64,
    pub deposits_completed: u64,
    pub deposits_failed: u64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub health_score: f64,
    pub blocks_processed: u64,
    pub last_block_processed: u64,
    pub consecutive_failures: u32,
    pub last_successful_operation: Option<SystemTime>,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            average_processing_time: Duration::from_secs(0),
            median_processing_time: Duration::from_secs(0),
            p95_processing_time: Duration::from_secs(0),
            p99_processing_time: Duration::from_secs(0),
            success_rate: 0.0,
            operations_per_minute: 0.0,
            peak_concurrent_operations: 0,
            last_updated: SystemTime::now(),
        }
    }
}

impl Default for PegInState {
    fn default() -> Self {
        Self::Initializing
    }
}

impl PegInState {
    /// Check if state allows processing new deposits
    pub fn can_process_deposits(&self) -> bool {
        matches!(self, PegInState::Monitoring)
    }

    /// Check if state is operational
    pub fn is_operational(&self) -> bool {
        matches!(self, PegInState::Monitoring | PegInState::Degraded { .. })
    }

    /// Get state description
    pub fn description(&self) -> String {
        match self {
            PegInState::Initializing => "Initializing PegIn actor".to_string(),
            PegInState::Monitoring => "Monitoring Bitcoin blockchain for deposits".to_string(),
            PegInState::Degraded { issues } => format!("Degraded: {}", issues.join(", ")),
            PegInState::Paused => "Paused".to_string(),
            PegInState::Stopping => "Stopping".to_string(),
            PegInState::Stopped => "Stopped".to_string(),
        }
    }
}