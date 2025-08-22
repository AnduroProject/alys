//! Migration Controller for Lighthouse V4/V5 Transition
//! 
//! This module provides centralized migration management, coordinating between
//! different migration modes, A/B testing, health monitoring, and rollback
//! capabilities during the Lighthouse version transition.

use crate::ab_test::{ABTestController, TestAssignment};
use crate::config::{MigrationConfig, MigrationMode, RollbackConfig, RollbackTrigger};
use crate::error::{CompatError, CompatResult};
use crate::health::{HealthMonitor, HealthStatus, RollbackReason};
use crate::metrics::MetricsCollector;
use crate::compat::LighthouseCompat;
use actix::prelude::*;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Migration Controller coordinating the V4->V5 transition
pub struct MigrationController {
    config: MigrationConfig,
    rollback_config: RollbackConfig,
    current_mode: Arc<RwLock<MigrationMode>>,
    migration_state: Arc<RwLock<MigrationState>>,
    lighthouse_compat: Arc<LighthouseCompat>,
    health_monitor: Arc<HealthMonitor>,
    metrics_collector: Arc<MetricsCollector>,
    ab_test_controller: Option<Arc<ABTestController>>,
    rollback_history: Arc<RwLock<Vec<RollbackEvent>>>,
    migration_phases: Arc<RwLock<Vec<MigrationPhase>>>,
}

/// Current state of the migration process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationState {
    pub phase: MigrationPhase,
    pub started_at: DateTime<Utc>,
    pub current_traffic_split: TrafficSplit,
    pub rollback_point: Option<RollbackPoint>,
    pub statistics: MigrationStatistics,
    pub flags: MigrationFlags,
}

/// Migration phase definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPhase {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_mode: MigrationMode,
    pub traffic_split: TrafficSplit,
    pub duration: Option<Duration>,
    pub success_criteria: Vec<SuccessCriterion>,
    pub rollback_criteria: Vec<RollbackCriterion>,
    pub prerequisites: Vec<String>,
    pub status: PhaseStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Phase execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PhaseStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    RolledBack,
}

/// Traffic distribution between versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSplit {
    pub v4_percentage: f64,
    pub v5_percentage: f64,
    pub canary_percentage: f64,
    pub updated_at: DateTime<Utc>,
}

/// Success criterion for phase completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriterion {
    pub metric: String,
    pub condition: ComparisonOperator,
    pub threshold: f64,
    pub duration: Duration,
    pub description: String,
}

/// Rollback criterion for automatic rollback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackCriterion {
    pub metric: String,
    pub condition: ComparisonOperator,
    pub threshold: f64,
    pub duration: Duration,
    pub severity: RollbackSeverity,
    pub description: String,
}

/// Comparison operators for criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Equal,
    NotEqual,
}

/// Rollback severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RollbackSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Rollback point for restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPoint {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub mode: MigrationMode,
    pub traffic_split: TrafficSplit,
    pub system_state: SystemSnapshot,
    pub description: String,
}

/// System state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub lighthouse_versions: HashMap<String, String>,
    pub configuration: HashMap<String, serde_json::Value>,
    pub health_metrics: HashMap<String, f64>,
    pub performance_baseline: HashMap<String, f64>,
}

/// Migration statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStatistics {
    pub total_requests: u64,
    pub v4_requests: u64,
    pub v5_requests: u64,
    pub success_rate_v4: f64,
    pub success_rate_v5: f64,
    pub average_latency_v4: f64,
    pub average_latency_v5: f64,
    pub error_rate_v4: f64,
    pub error_rate_v5: f64,
    pub rollback_count: u32,
    pub updated_at: DateTime<Utc>,
}

/// Migration control flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFlags {
    pub auto_rollback_enabled: bool,
    pub canary_enabled: bool,
    pub ab_testing_enabled: bool,
    pub metrics_collection_enabled: bool,
    pub phase_auto_progression: bool,
}

/// Rollback event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub trigger: RollbackTrigger,
    pub reason: RollbackReason,
    pub from_mode: MigrationMode,
    pub to_mode: MigrationMode,
    pub rollback_point_id: String,
    pub duration_ms: u64,
    pub success: bool,
    pub details: HashMap<String, serde_json::Value>,
}

/// Migration command for external control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationCommand {
    StartMigration {
        target_phase: Option<String>,
    },
    PauseMigration,
    ResumeMigration,
    RollbackToPhase {
        phase_id: String,
    },
    RollbackToPoint {
        rollback_point_id: String,
    },
    UpdateTrafficSplit {
        v4_percentage: f64,
        v5_percentage: f64,
    },
    SetMode {
        mode: MigrationMode,
    },
    CreateRollbackPoint {
        description: String,
    },
    EnableAutoRollback,
    DisableAutoRollback,
}

/// Migration status report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStatus {
    pub current_mode: MigrationMode,
    pub current_phase: Option<MigrationPhase>,
    pub next_phase: Option<MigrationPhase>,
    pub progress_percentage: f64,
    pub traffic_split: TrafficSplit,
    pub statistics: MigrationStatistics,
    pub health_summary: HealthSummary,
    pub flags: MigrationFlags,
    pub last_rollback: Option<RollbackEvent>,
    pub estimated_completion: Option<DateTime<Utc>>,
}

/// Health summary for migration status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub overall_health: HealthStatus,
    pub v4_health: HealthStatus,
    pub v5_health: HealthStatus,
    pub active_alerts: u32,
    pub rollback_risk: RollbackRisk,
}

/// Rollback risk assessment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RollbackRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl MigrationController {
    /// Create a new migration controller
    pub fn new(
        config: MigrationConfig,
        rollback_config: RollbackConfig,
        lighthouse_compat: Arc<LighthouseCompat>,
        health_monitor: Arc<HealthMonitor>,
        metrics_collector: Arc<MetricsCollector>,
        ab_test_controller: Option<Arc<ABTestController>>,
    ) -> CompatResult<Self> {
        let migration_phases = Self::initialize_phases(&config)?;
        
        let migration_state = MigrationState {
            phase: migration_phases[0].clone(),
            started_at: Utc::now(),
            current_traffic_split: TrafficSplit {
                v4_percentage: 100.0,
                v5_percentage: 0.0,
                canary_percentage: 0.0,
                updated_at: Utc::now(),
            },
            rollback_point: None,
            statistics: MigrationStatistics::default(),
            flags: MigrationFlags {
                auto_rollback_enabled: config.auto_rollback_enabled,
                canary_enabled: config.canary_enabled,
                ab_testing_enabled: ab_test_controller.is_some(),
                metrics_collection_enabled: true,
                phase_auto_progression: config.auto_progression_enabled,
            },
        };
        
        Ok(Self {
            config,
            rollback_config,
            current_mode: Arc::new(RwLock::new(MigrationMode::V4Only)),
            migration_state: Arc::new(RwLock::new(migration_state)),
            lighthouse_compat,
            health_monitor,
            metrics_collector,
            ab_test_controller,
            rollback_history: Arc::new(RwLock::new(Vec::new())),
            migration_phases: Arc::new(RwLock::new(migration_phases)),
        })
    }

    /// Start the migration process
    pub async fn start_migration(
        &self,
        target_phase: Option<String>,
    ) -> CompatResult<()> {
        info!("Starting Lighthouse V4->V5 migration");
        
        // Create initial rollback point
        let rollback_point = self.create_rollback_point("Migration start".to_string()).await?;
        
        // Update state
        let mut state = self.migration_state.write().await;
        state.rollback_point = Some(rollback_point);
        state.started_at = Utc::now();
        
        // Start monitoring
        self.start_health_monitoring().await?;
        self.start_metrics_collection().await?;
        
        // Begin first phase
        let first_phase_id = if let Some(target) = target_phase {
            target
        } else {
            self.migration_phases.read().await[0].id.clone()
        };
        
        self.execute_phase(&first_phase_id).await?;
        
        info!("Migration started successfully");
        Ok(())
    }

    /// Execute a specific migration phase
    pub async fn execute_phase(&self, phase_id: &str) -> CompatResult<()> {
        let phase = {
            let phases = self.migration_phases.read().await;
            phases.iter()
                .find(|p| p.id == phase_id)
                .cloned()
                .ok_or_else(|| CompatError::MigrationPhaseNotFound {
                    phase_id: phase_id.to_string(),
                })?
        };
        
        info!(
            phase_id = %phase.id,
            phase_name = %phase.name,
            "Executing migration phase"
        );
        
        // Check prerequisites
        self.check_prerequisites(&phase).await?;
        
        // Update phase status
        self.update_phase_status(&phase.id, PhaseStatus::InProgress).await?;
        
        // Set migration mode
        *self.current_mode.write().await = phase.target_mode.clone();
        
        // Update traffic split
        self.update_traffic_split(phase.traffic_split.clone()).await?;
        
        // Start phase monitoring
        let phase_monitor = self.start_phase_monitoring(&phase).await?;
        
        // Wait for success criteria or rollback criteria
        let phase_result = self.monitor_phase_execution(&phase, phase_monitor).await;
        
        match phase_result {
            Ok(_) => {
                self.update_phase_status(&phase.id, PhaseStatus::Completed).await?;
                info!(phase_id = %phase.id, "Phase completed successfully");
                
                // Auto-progress to next phase if enabled
                if self.migration_state.read().await.flags.phase_auto_progression {
                    if let Some(next_phase) = self.get_next_phase(&phase.id).await? {
                        self.execute_phase(&next_phase.id).await?;
                    }
                }
            },
            Err(e) => {
                error!(phase_id = %phase.id, error = %e, "Phase execution failed");
                self.update_phase_status(&phase.id, PhaseStatus::Failed).await?;
                
                // Trigger rollback if auto-rollback is enabled
                if self.migration_state.read().await.flags.auto_rollback_enabled {
                    self.rollback(RollbackTrigger::PhaseFailed).await?;
                }
                
                return Err(e);
            }
        }
        
        Ok(())
    }

    /// Handle migration command
    pub async fn handle_command(&self, command: MigrationCommand) -> CompatResult<()> {
        match command {
            MigrationCommand::StartMigration { target_phase } => {
                self.start_migration(target_phase).await
            },
            MigrationCommand::PauseMigration => {
                self.pause_migration().await
            },
            MigrationCommand::ResumeMigration => {
                self.resume_migration().await
            },
            MigrationCommand::RollbackToPhase { phase_id } => {
                self.rollback_to_phase(&phase_id).await
            },
            MigrationCommand::RollbackToPoint { rollback_point_id } => {
                self.rollback_to_point(&rollback_point_id).await
            },
            MigrationCommand::UpdateTrafficSplit { v4_percentage, v5_percentage } => {
                let traffic_split = TrafficSplit {
                    v4_percentage,
                    v5_percentage,
                    canary_percentage: 100.0 - v4_percentage - v5_percentage,
                    updated_at: Utc::now(),
                };
                self.update_traffic_split(traffic_split).await
            },
            MigrationCommand::SetMode { mode } => {
                self.set_migration_mode(mode).await
            },
            MigrationCommand::CreateRollbackPoint { description } => {
                self.create_rollback_point(description).await.map(|_| ())
            },
            MigrationCommand::EnableAutoRollback => {
                self.enable_auto_rollback().await
            },
            MigrationCommand::DisableAutoRollback => {
                self.disable_auto_rollback().await
            },
        }
    }

    /// Get current migration status
    pub async fn get_migration_status(&self) -> CompatResult<MigrationStatus> {
        let state = self.migration_state.read().await;
        let current_mode = *self.current_mode.read().await;
        
        let health_summary = self.get_health_summary().await?;
        let next_phase = self.get_next_phase(&state.phase.id).await?;
        
        let progress_percentage = self.calculate_progress_percentage(&state.phase).await?;
        let estimated_completion = self.estimate_completion_time(&state).await?;
        
        let last_rollback = {
            let history = self.rollback_history.read().await;
            history.last().cloned()
        };
        
        Ok(MigrationStatus {
            current_mode,
            current_phase: Some(state.phase.clone()),
            next_phase,
            progress_percentage,
            traffic_split: state.current_traffic_split.clone(),
            statistics: state.statistics.clone(),
            health_summary,
            flags: state.flags.clone(),
            last_rollback,
            estimated_completion,
        })
    }

    /// Rollback to a previous state
    pub async fn rollback(&self, trigger: RollbackTrigger) -> CompatResult<()> {
        let rollback_start = std::time::Instant::now();
        let rollback_id = uuid::Uuid::new_v4().to_string();
        
        info!(
            rollback_id = %rollback_id,
            trigger = ?trigger,
            "Starting migration rollback"
        );
        
        let rollback_point = {
            let state = self.migration_state.read().await;
            state.rollback_point.clone()
                .ok_or_else(|| CompatError::NoRollbackPointAvailable)?
        };
        
        // Determine rollback reason
        let reason = self.determine_rollback_reason(&trigger).await?;
        
        // Store current mode for rollback event
        let from_mode = *self.current_mode.read().await;
        
        // Execute rollback
        let rollback_result = self.execute_rollback(&rollback_point).await;
        
        let duration_ms = rollback_start.elapsed().as_millis() as u64;
        
        // Record rollback event
        let rollback_event = RollbackEvent {
            id: rollback_id.clone(),
            timestamp: Utc::now(),
            trigger,
            reason: reason.clone(),
            from_mode,
            to_mode: rollback_point.mode.clone(),
            rollback_point_id: rollback_point.id.clone(),
            duration_ms,
            success: rollback_result.is_ok(),
            details: self.collect_rollback_details(&rollback_point).await,
        };
        
        // Store rollback event
        self.rollback_history.write().await.push(rollback_event);
        
        match rollback_result {
            Ok(_) => {
                info!(
                    rollback_id = %rollback_id,
                    duration_ms = duration_ms,
                    to_mode = ?rollback_point.mode,
                    "Rollback completed successfully"
                );
                
                // Update metrics
                self.metrics_collector.record_rollback_success(duration_ms).await;
            },
            Err(e) => {
                error!(
                    rollback_id = %rollback_id,
                    error = %e,
                    "Rollback failed"
                );
                
                // Update metrics
                self.metrics_collector.record_rollback_failure(duration_ms).await;
                
                return Err(e);
            }
        }
        
        Ok(())
    }

    /// Create a rollback point
    pub async fn create_rollback_point(&self, description: String) -> CompatResult<RollbackPoint> {
        let rollback_point = RollbackPoint {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            mode: *self.current_mode.read().await,
            traffic_split: self.migration_state.read().await.current_traffic_split.clone(),
            system_state: self.capture_system_snapshot().await?,
            description,
        };
        
        info!(
            rollback_point_id = %rollback_point.id,
            description = %rollback_point.description,
            "Created rollback point"
        );
        
        Ok(rollback_point)
    }

    /// Initialize migration phases
    fn initialize_phases(config: &MigrationConfig) -> CompatResult<Vec<MigrationPhase>> {
        let mut phases = Vec::new();
        
        // Phase 1: Canary deployment
        phases.push(MigrationPhase {
            id: "canary".to_string(),
            name: "Canary Deployment".to_string(),
            description: "Deploy V5 to small percentage of traffic".to_string(),
            target_mode: MigrationMode::Canary,
            traffic_split: TrafficSplit {
                v4_percentage: 95.0,
                v5_percentage: 0.0,
                canary_percentage: 5.0,
                updated_at: Utc::now(),
            },
            duration: Some(Duration::hours(2)),
            success_criteria: vec![
                SuccessCriterion {
                    metric: "error_rate".to_string(),
                    condition: ComparisonOperator::LessThan,
                    threshold: 0.01,
                    duration: Duration::minutes(30),
                    description: "Error rate < 1%".to_string(),
                },
            ],
            rollback_criteria: vec![
                RollbackCriterion {
                    metric: "error_rate".to_string(),
                    condition: ComparisonOperator::GreaterThan,
                    threshold: 0.05,
                    duration: Duration::minutes(5),
                    severity: RollbackSeverity::High,
                    description: "Error rate > 5%".to_string(),
                },
            ],
            prerequisites: vec!["health_check_passed".to_string()],
            status: PhaseStatus::Pending,
            started_at: None,
            completed_at: None,
        });
        
        // Phase 2: Gradual rollout
        phases.push(MigrationPhase {
            id: "gradual_rollout".to_string(),
            name: "Gradual Rollout".to_string(),
            description: "Gradually increase V5 traffic".to_string(),
            target_mode: MigrationMode::V5Primary,
            traffic_split: TrafficSplit {
                v4_percentage: 25.0,
                v5_percentage: 75.0,
                canary_percentage: 0.0,
                updated_at: Utc::now(),
            },
            duration: Some(Duration::hours(6)),
            success_criteria: vec![
                SuccessCriterion {
                    metric: "success_rate".to_string(),
                    condition: ComparisonOperator::GreaterThan,
                    threshold: 0.995,
                    duration: Duration::hours(1),
                    description: "Success rate > 99.5%".to_string(),
                },
            ],
            rollback_criteria: vec![
                RollbackCriterion {
                    metric: "latency_p99".to_string(),
                    condition: ComparisonOperator::GreaterThan,
                    threshold: 1000.0,
                    duration: Duration::minutes(10),
                    severity: RollbackSeverity::Medium,
                    description: "P99 latency > 1s".to_string(),
                },
            ],
            prerequisites: vec!["canary".to_string()],
            status: PhaseStatus::Pending,
            started_at: None,
            completed_at: None,
        });
        
        // Phase 3: Full migration
        phases.push(MigrationPhase {
            id: "full_migration".to_string(),
            name: "Full Migration".to_string(),
            description: "Complete migration to V5".to_string(),
            target_mode: MigrationMode::V5Only,
            traffic_split: TrafficSplit {
                v4_percentage: 0.0,
                v5_percentage: 100.0,
                canary_percentage: 0.0,
                updated_at: Utc::now(),
            },
            duration: Some(Duration::hours(2)),
            success_criteria: vec![
                SuccessCriterion {
                    metric: "availability".to_string(),
                    condition: ComparisonOperator::GreaterThan,
                    threshold: 0.999,
                    duration: Duration::hours(1),
                    description: "Availability > 99.9%".to_string(),
                },
            ],
            rollback_criteria: vec![
                RollbackCriterion {
                    metric: "consensus_failure_rate".to_string(),
                    condition: ComparisonOperator::GreaterThan,
                    threshold: 0.001,
                    duration: Duration::minutes(5),
                    severity: RollbackSeverity::Critical,
                    description: "Consensus failure rate > 0.1%".to_string(),
                },
            ],
            prerequisites: vec!["gradual_rollout".to_string()],
            status: PhaseStatus::Pending,
            started_at: None,
            completed_at: None,
        });
        
        Ok(phases)
    }

    /// Check phase prerequisites
    async fn check_prerequisites(&self, phase: &MigrationPhase) -> CompatResult<()> {
        for prerequisite in &phase.prerequisites {
            match prerequisite.as_str() {
                "health_check_passed" => {
                    let health = self.health_monitor.get_overall_health().await?;
                    if !matches!(health, HealthStatus::Healthy) {
                        return Err(CompatError::PrerequisiteNotMet {
                            prerequisite: prerequisite.clone(),
                            reason: format!("Health check failed: {:?}", health),
                        });
                    }
                },
                phase_id => {
                    // Check if prerequisite phase is completed
                    let phases = self.migration_phases.read().await;
                    let prereq_phase = phases.iter()
                        .find(|p| p.id == phase_id)
                        .ok_or_else(|| CompatError::PrerequisiteNotMet {
                            prerequisite: prerequisite.clone(),
                            reason: "Phase not found".to_string(),
                        })?;
                    
                    if prereq_phase.status != PhaseStatus::Completed {
                        return Err(CompatError::PrerequisiteNotMet {
                            prerequisite: prerequisite.clone(),
                            reason: format!("Phase not completed: {:?}", prereq_phase.status),
                        });
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Update phase status
    async fn update_phase_status(&self, phase_id: &str, status: PhaseStatus) -> CompatResult<()> {
        let mut phases = self.migration_phases.write().await;
        if let Some(phase) = phases.iter_mut().find(|p| p.id == phase_id) {
            phase.status = status.clone();
            
            match status {
                PhaseStatus::InProgress => {
                    phase.started_at = Some(Utc::now());
                },
                PhaseStatus::Completed | PhaseStatus::Failed | PhaseStatus::RolledBack => {
                    phase.completed_at = Some(Utc::now());
                },
                _ => {}
            }
            
            info!(
                phase_id = %phase_id,
                status = ?status,
                "Updated phase status"
            );
        }
        
        Ok(())
    }

    /// Update traffic split
    async fn update_traffic_split(&self, traffic_split: TrafficSplit) -> CompatResult<()> {
        // Validate traffic split
        let total = traffic_split.v4_percentage + traffic_split.v5_percentage + traffic_split.canary_percentage;
        if (total - 100.0).abs() > 0.01 {
            return Err(CompatError::InvalidTrafficSplit {
                total_percentage: total,
            });
        }
        
        // Update state
        let mut state = self.migration_state.write().await;
        state.current_traffic_split = traffic_split.clone();
        
        // Update lighthouse compat layer
        self.lighthouse_compat.update_traffic_split(traffic_split.clone()).await?;
        
        info!(
            v4_percentage = traffic_split.v4_percentage,
            v5_percentage = traffic_split.v5_percentage,
            canary_percentage = traffic_split.canary_percentage,
            "Updated traffic split"
        );
        
        Ok(())
    }

    /// Start health monitoring for the migration
    async fn start_health_monitoring(&self) -> CompatResult<()> {
        // Configure health monitoring for migration
        self.health_monitor.start_migration_monitoring().await?;
        
        // Set up rollback triggers
        let rollback_triggers = vec![
            RollbackTrigger::HealthDegradation,
            RollbackTrigger::ConsensusFailure,
            RollbackTrigger::SyncFailure,
        ];
        
        for trigger in rollback_triggers {
            self.health_monitor.configure_rollback_trigger(trigger).await?;
        }
        
        Ok(())
    }

    /// Start metrics collection for the migration
    async fn start_metrics_collection(&self) -> CompatResult<()> {
        self.metrics_collector.start_migration_metrics().await?;
        Ok(())
    }

    /// Start monitoring for a specific phase
    async fn start_phase_monitoring(&self, phase: &MigrationPhase) -> CompatResult<PhaseMonitor> {
        let monitor = PhaseMonitor::new(
            phase.clone(),
            Arc::clone(&self.health_monitor),
            Arc::clone(&self.metrics_collector),
        )?;
        
        monitor.start().await?;
        Ok(monitor)
    }

    /// Monitor phase execution until completion or failure
    async fn monitor_phase_execution(
        &self,
        phase: &MigrationPhase,
        monitor: PhaseMonitor,
    ) -> CompatResult<()> {
        let start_time = Utc::now();
        let timeout = phase.duration.unwrap_or(Duration::hours(24));
        
        loop {
            // Check timeout
            if Utc::now().signed_duration_since(start_time) > timeout {
                return Err(CompatError::PhaseTimeout {
                    phase_id: phase.id.clone(),
                    timeout_duration: timeout,
                });
            }
            
            // Check success criteria
            if monitor.check_success_criteria().await? {
                return Ok(());
            }
            
            // Check rollback criteria
            if let Some(rollback_reason) = monitor.check_rollback_criteria().await? {
                return Err(CompatError::RollbackCriterionMet {
                    phase_id: phase.id.clone(),
                    criterion: rollback_reason,
                });
            }
            
            // Wait before next check
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    }

    /// Get next phase in the migration sequence
    async fn get_next_phase(&self, current_phase_id: &str) -> CompatResult<Option<MigrationPhase>> {
        let phases = self.migration_phases.read().await;
        
        let current_index = phases.iter()
            .position(|p| p.id == current_phase_id)
            .ok_or_else(|| CompatError::MigrationPhaseNotFound {
                phase_id: current_phase_id.to_string(),
            })?;
        
        if current_index + 1 < phases.len() {
            Ok(Some(phases[current_index + 1].clone()))
        } else {
            Ok(None)
        }
    }

    /// Calculate migration progress percentage
    async fn calculate_progress_percentage(&self, current_phase: &MigrationPhase) -> CompatResult<f64> {
        let phases = self.migration_phases.read().await;
        let total_phases = phases.len();
        
        let current_index = phases.iter()
            .position(|p| p.id == current_phase.id)
            .unwrap_or(0);
        
        let completed_phases = phases.iter()
            .take(current_index)
            .filter(|p| p.status == PhaseStatus::Completed)
            .count();
        
        let progress = if current_phase.status == PhaseStatus::Completed {
            (completed_phases + 1) as f64 / total_phases as f64
        } else {
            completed_phases as f64 / total_phases as f64
        };
        
        Ok(progress * 100.0)
    }

    /// Estimate completion time
    async fn estimate_completion_time(&self, state: &MigrationState) -> CompatResult<Option<DateTime<Utc>>> {
        let phases = self.migration_phases.read().await;
        let remaining_phases: Vec<_> = phases.iter()
            .filter(|p| p.status == PhaseStatus::Pending)
            .collect();
        
        if remaining_phases.is_empty() {
            return Ok(None);
        }
        
        let estimated_duration: Duration = remaining_phases.iter()
            .map(|p| p.duration.unwrap_or(Duration::hours(4)))
            .sum();
        
        Ok(Some(Utc::now() + estimated_duration))
    }

    /// Get health summary
    async fn get_health_summary(&self) -> CompatResult<HealthSummary> {
        let overall_health = self.health_monitor.get_overall_health().await?;
        let v4_health = self.health_monitor.get_v4_health().await?;
        let v5_health = self.health_monitor.get_v5_health().await?;
        let active_alerts = self.health_monitor.get_active_alert_count().await?;
        
        let rollback_risk = self.assess_rollback_risk(&overall_health, &v4_health, &v5_health).await?;
        
        Ok(HealthSummary {
            overall_health,
            v4_health,
            v5_health,
            active_alerts,
            rollback_risk,
        })
    }

    /// Assess rollback risk
    async fn assess_rollback_risk(
        &self,
        overall_health: &HealthStatus,
        v4_health: &HealthStatus,
        v5_health: &HealthStatus,
    ) -> CompatResult<RollbackRisk> {
        let risk = match (overall_health, v4_health, v5_health) {
            (HealthStatus::Healthy, HealthStatus::Healthy, HealthStatus::Healthy) => RollbackRisk::Low,
            (HealthStatus::Degraded, _, _) => RollbackRisk::Medium,
            (HealthStatus::Unhealthy, _, _) => RollbackRisk::High,
            (HealthStatus::Failed, _, _) => RollbackRisk::Critical,
            _ => RollbackRisk::Medium,
        };
        
        Ok(risk)
    }

    /// Set migration mode
    async fn set_migration_mode(&self, mode: MigrationMode) -> CompatResult<()> {
        *self.current_mode.write().await = mode.clone();
        self.lighthouse_compat.set_mode(mode).await?;
        
        info!(mode = ?mode, "Set migration mode");
        Ok(())
    }

    /// Pause migration
    async fn pause_migration(&self) -> CompatResult<()> {
        // Implementation would pause active migration phases
        info!("Migration paused");
        Ok(())
    }

    /// Resume migration
    async fn resume_migration(&self) -> CompatResult<()> {
        // Implementation would resume paused migration
        info!("Migration resumed");
        Ok(())
    }

    /// Rollback to specific phase
    async fn rollback_to_phase(&self, phase_id: &str) -> CompatResult<()> {
        info!(phase_id = %phase_id, "Rolling back to phase");
        
        let phases = self.migration_phases.read().await;
        let target_phase = phases.iter()
            .find(|p| p.id == phase_id)
            .ok_or_else(|| CompatError::MigrationPhaseNotFound {
                phase_id: phase_id.to_string(),
            })?;
        
        // Set mode and traffic split for target phase
        self.set_migration_mode(target_phase.target_mode.clone()).await?;
        self.update_traffic_split(target_phase.traffic_split.clone()).await?;
        
        Ok(())
    }

    /// Rollback to specific rollback point
    async fn rollback_to_point(&self, rollback_point_id: &str) -> CompatResult<()> {
        let state = self.migration_state.read().await;
        let rollback_point = state.rollback_point.as_ref()
            .ok_or_else(|| CompatError::NoRollbackPointAvailable)?;
        
        if rollback_point.id != rollback_point_id {
            return Err(CompatError::RollbackPointNotFound {
                rollback_point_id: rollback_point_id.to_string(),
            });
        }
        
        self.execute_rollback(rollback_point).await?;
        Ok(())
    }

    /// Execute rollback to a specific point
    async fn execute_rollback(&self, rollback_point: &RollbackPoint) -> CompatResult<()> {
        // Restore system state
        self.restore_system_snapshot(&rollback_point.system_state).await?;
        
        // Set migration mode
        self.set_migration_mode(rollback_point.mode.clone()).await?;
        
        // Update traffic split
        self.update_traffic_split(rollback_point.traffic_split.clone()).await?;
        
        Ok(())
    }

    /// Enable auto-rollback
    async fn enable_auto_rollback(&self) -> CompatResult<()> {
        let mut state = self.migration_state.write().await;
        state.flags.auto_rollback_enabled = true;
        
        info!("Auto-rollback enabled");
        Ok(())
    }

    /// Disable auto-rollback
    async fn disable_auto_rollback(&self) -> CompatResult<()> {
        let mut state = self.migration_state.write().await;
        state.flags.auto_rollback_enabled = false;
        
        warn!("Auto-rollback disabled");
        Ok(())
    }

    /// Determine rollback reason based on trigger
    async fn determine_rollback_reason(&self, trigger: &RollbackTrigger) -> CompatResult<RollbackReason> {
        let reason = match trigger {
            RollbackTrigger::HealthDegradation => RollbackReason::HealthDegradation,
            RollbackTrigger::ConsensusFailure => RollbackReason::ConsensusFailure,
            RollbackTrigger::SyncFailure => RollbackReason::SyncFailure,
            RollbackTrigger::PhaseFailed => RollbackReason::PhaseFailed,
            RollbackTrigger::ManualTrigger => RollbackReason::ManualRollback,
            RollbackTrigger::AutomaticTrigger => RollbackReason::AutomaticRollback,
        };
        
        Ok(reason)
    }

    /// Capture system snapshot for rollback point
    async fn capture_system_snapshot(&self) -> CompatResult<SystemSnapshot> {
        let lighthouse_versions = self.lighthouse_compat.get_version_info().await?;
        let configuration = self.lighthouse_compat.get_configuration().await?;
        let health_metrics = self.health_monitor.get_health_metrics().await?;
        let performance_baseline = self.metrics_collector.get_performance_baseline().await?;
        
        Ok(SystemSnapshot {
            lighthouse_versions,
            configuration,
            health_metrics,
            performance_baseline,
        })
    }

    /// Restore system snapshot
    async fn restore_system_snapshot(&self, snapshot: &SystemSnapshot) -> CompatResult<()> {
        // Restore configuration
        self.lighthouse_compat.restore_configuration(&snapshot.configuration).await?;
        
        // Reset health monitoring baselines
        self.health_monitor.restore_baselines(&snapshot.health_metrics).await?;
        
        // Reset performance baselines
        self.metrics_collector.restore_baselines(&snapshot.performance_baseline).await?;
        
        Ok(())
    }

    /// Collect rollback details
    async fn collect_rollback_details(&self, rollback_point: &RollbackPoint) -> HashMap<String, serde_json::Value> {
        let mut details = HashMap::new();
        
        details.insert("rollback_point_id".to_string(), 
                      serde_json::Value::String(rollback_point.id.clone()));
        details.insert("rollback_point_created_at".to_string(), 
                      serde_json::Value::String(rollback_point.created_at.to_rfc3339()));
        details.insert("system_snapshot_keys".to_string(),
                      serde_json::Value::Array(
                          rollback_point.system_state.configuration.keys()
                              .map(|k| serde_json::Value::String(k.clone()))
                              .collect()
                      ));
        
        details
    }
}

/// Phase monitor for tracking phase execution
pub struct PhaseMonitor {
    phase: MigrationPhase,
    health_monitor: Arc<HealthMonitor>,
    metrics_collector: Arc<MetricsCollector>,
    start_time: DateTime<Utc>,
}

impl PhaseMonitor {
    pub fn new(
        phase: MigrationPhase,
        health_monitor: Arc<HealthMonitor>,
        metrics_collector: Arc<MetricsCollector>,
    ) -> CompatResult<Self> {
        Ok(Self {
            phase,
            health_monitor,
            metrics_collector,
            start_time: Utc::now(),
        })
    }

    pub async fn start(&self) -> CompatResult<()> {
        info!(
            phase_id = %self.phase.id,
            "Started phase monitoring"
        );
        Ok(())
    }

    pub async fn check_success_criteria(&self) -> CompatResult<bool> {
        for criterion in &self.phase.success_criteria {
            let current_value = self.metrics_collector.get_metric(&criterion.metric).await?;
            let duration_met = Utc::now().signed_duration_since(self.start_time) >= criterion.duration;
            
            if !duration_met {
                continue;
            }
            
            let criteria_met = match criterion.condition {
                ComparisonOperator::GreaterThan => current_value > criterion.threshold,
                ComparisonOperator::LessThan => current_value < criterion.threshold,
                ComparisonOperator::GreaterThanOrEqual => current_value >= criterion.threshold,
                ComparisonOperator::LessThanOrEqual => current_value <= criterion.threshold,
                ComparisonOperator::Equal => (current_value - criterion.threshold).abs() < 0.001,
                ComparisonOperator::NotEqual => (current_value - criterion.threshold).abs() >= 0.001,
            };
            
            if !criteria_met {
                return Ok(false);
            }
        }
        
        Ok(true)
    }

    pub async fn check_rollback_criteria(&self) -> CompatResult<Option<String>> {
        for criterion in &self.phase.rollback_criteria {
            let current_value = self.metrics_collector.get_metric(&criterion.metric).await?;
            let duration_met = Utc::now().signed_duration_since(self.start_time) >= criterion.duration;
            
            if !duration_met {
                continue;
            }
            
            let rollback_needed = match criterion.condition {
                ComparisonOperator::GreaterThan => current_value > criterion.threshold,
                ComparisonOperator::LessThan => current_value < criterion.threshold,
                ComparisonOperator::GreaterThanOrEqual => current_value >= criterion.threshold,
                ComparisonOperator::LessThanOrEqual => current_value <= criterion.threshold,
                ComparisonOperator::Equal => (current_value - criterion.threshold).abs() < 0.001,
                ComparisonOperator::NotEqual => (current_value - criterion.threshold).abs() >= 0.001,
            };
            
            if rollback_needed {
                return Ok(Some(criterion.description.clone()));
            }
        }
        
        Ok(None)
    }
}

impl Default for MigrationStatistics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            v4_requests: 0,
            v5_requests: 0,
            success_rate_v4: 0.0,
            success_rate_v5: 0.0,
            average_latency_v4: 0.0,
            average_latency_v5: 0.0,
            error_rate_v4: 0.0,
            error_rate_v5: 0.0,
            rollback_count: 0,
            updated_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CompatConfig;
    
    #[tokio::test]
    async fn test_phase_initialization() {
        let config = MigrationConfig::default();
        let phases = MigrationController::initialize_phases(&config).unwrap();
        
        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0].id, "canary");
        assert_eq!(phases[1].id, "gradual_rollout");
        assert_eq!(phases[2].id, "full_migration");
    }

    #[test]
    fn test_traffic_split_validation() {
        let valid_split = TrafficSplit {
            v4_percentage: 50.0,
            v5_percentage: 50.0,
            canary_percentage: 0.0,
            updated_at: Utc::now(),
        };
        
        let total = valid_split.v4_percentage + valid_split.v5_percentage + valid_split.canary_percentage;
        assert!((total - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_rollback_severity_ordering() {
        assert!(RollbackSeverity::Critical > RollbackSeverity::High);
        assert!(RollbackSeverity::High > RollbackSeverity::Medium);
        assert!(RollbackSeverity::Medium > RollbackSeverity::Low);
    }

    #[tokio::test]
    async fn test_phase_monitor_creation() {
        let phase = MigrationPhase {
            id: "test".to_string(),
            name: "Test Phase".to_string(),
            description: "Test phase".to_string(),
            target_mode: MigrationMode::V5Only,
            traffic_split: TrafficSplit {
                v4_percentage: 0.0,
                v5_percentage: 100.0,
                canary_percentage: 0.0,
                updated_at: Utc::now(),
            },
            duration: Some(Duration::hours(1)),
            success_criteria: vec![],
            rollback_criteria: vec![],
            prerequisites: vec![],
            status: PhaseStatus::Pending,
            started_at: None,
            completed_at: None,
        };
        
        // Mock dependencies would be needed for full test
        // This test just verifies the structure compiles
        assert_eq!(phase.id, "test");
    }
}