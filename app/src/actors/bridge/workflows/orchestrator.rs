//! Workflow Orchestrator
//! 
//! High-level orchestrator managing all bridge workflows

use actix::prelude::*;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error};

use crate::actors::bridge::{
    actors::{bridge::BridgeActor, pegin::PegInActor, pegout::PegOutActor, stream::StreamActor},
    integration::{CoordinationManager, StateSyncManager},
    supervision::BridgeSupervisor,
};

use super::{
    pegin_workflow::{PegInWorkflowOrchestrator, PegInWorkflowMetrics},
    pegout_workflow::{PegOutWorkflowOrchestrator, PegOutWorkflowMetrics},
    monitoring::WorkflowMonitor,
};

/// Master workflow orchestrator for all bridge operations
pub struct BridgeWorkflowOrchestrator {
    /// Actor addresses
    bridge_supervisor: Addr<BridgeSupervisor>,
    bridge_actor: Option<Addr<BridgeActor>>,
    pegin_actor: Option<Addr<PegInActor>>,
    pegout_actor: Option<Addr<PegOutActor>>,
    stream_actor: Option<Addr<StreamActor>>,
    
    /// Specialized workflow orchestrators
    pegin_orchestrator: Option<PegInWorkflowOrchestrator>,
    pegout_orchestrator: Option<PegOutWorkflowOrchestrator>,
    
    /// Coordination and monitoring
    coordination_manager: CoordinationManager,
    state_sync_manager: StateSyncManager,
    workflow_monitor: WorkflowMonitor,
    
    /// System state
    orchestrator_metrics: OrchestratorMetrics,
    system_health: SystemHealthStatus,
    
    /// Configuration
    max_concurrent_workflows: usize,
    workflow_timeout: Duration,
    health_check_interval: Duration,
}

/// Overall system health status
#[derive(Debug, Clone)]
pub struct SystemHealthStatus {
    pub overall_status: OverallStatus,
    pub pegin_health: ComponentHealth,
    pub pegout_health: ComponentHealth,
    pub coordination_health: ComponentHealth,
    pub last_health_check: SystemTime,
}

/// Overall system status
#[derive(Debug, Clone, PartialEq)]
pub enum OverallStatus {
    Healthy,
    Degraded,
    Critical,
    Offline,
}

/// Individual component health
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    pub status: ComponentStatus,
    pub error_rate: f64,
    pub response_time: Duration,
    pub last_error: Option<String>,
}

/// Component status
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentStatus {
    Healthy,
    Degraded,
    Failed,
    Unknown,
}

/// Orchestrator metrics
#[derive(Debug, Default, Clone)]
pub struct OrchestratorMetrics {
    pub total_workflows_initiated: u64,
    pub workflows_completed: u64,
    pub workflows_failed: u64,
    pub average_workflow_duration: Duration,
    pub concurrent_workflows_peak: u32,
    pub system_uptime: Duration,
    pub health_checks_performed: u64,
    pub coordination_operations: u64,
    pub state_sync_operations: u64,
}

impl BridgeWorkflowOrchestrator {
    pub fn new(
        bridge_supervisor: Addr<BridgeSupervisor>,
        max_concurrent_workflows: usize,
        workflow_timeout: Duration,
        health_check_interval: Duration,
    ) -> Self {
        let coordination_manager = CoordinationManager::new();
        let state_sync_manager = StateSyncManager::new(
            health_check_interval,
            5, // max sync attempts
            Duration::from_secs(30), // sync timeout
        );
        let workflow_monitor = WorkflowMonitor::new();

        Self {
            bridge_supervisor,
            bridge_actor: None,
            pegin_actor: None,
            pegout_actor: None,
            stream_actor: None,
            pegin_orchestrator: None,
            pegout_orchestrator: None,
            coordination_manager,
            state_sync_manager,
            workflow_monitor,
            orchestrator_metrics: OrchestratorMetrics::default(),
            system_health: SystemHealthStatus {
                overall_status: OverallStatus::Offline,
                pegin_health: ComponentHealth {
                    status: ComponentStatus::Unknown,
                    error_rate: 0.0,
                    response_time: Duration::from_millis(0),
                    last_error: None,
                },
                pegout_health: ComponentHealth {
                    status: ComponentStatus::Unknown,
                    error_rate: 0.0,
                    response_time: Duration::from_millis(0),
                    last_error: None,
                },
                coordination_health: ComponentHealth {
                    status: ComponentStatus::Unknown,
                    error_rate: 0.0,
                    response_time: Duration::from_millis(0),
                    last_error: None,
                },
                last_health_check: SystemTime::now(),
            },
            max_concurrent_workflows,
            workflow_timeout,
            health_check_interval,
        }
    }

    /// Initialize orchestrator with actor addresses
    pub async fn initialize(
        &mut self,
        bridge_actor: Addr<BridgeActor>,
        pegin_actor: Addr<PegInActor>,
        pegout_actor: Addr<PegOutActor>,
        stream_actor: Addr<StreamActor>,
    ) -> Result<(), OrchestratorError> {
        info!("Initializing Bridge Workflow Orchestrator");

        // Store actor addresses
        self.bridge_actor = Some(bridge_actor.clone());
        self.pegin_actor = Some(pegin_actor.clone());
        self.pegout_actor = Some(pegout_actor.clone());
        self.stream_actor = Some(stream_actor.clone());

        // Register actors with coordination and state sync
        self.coordination_manager.register_actors(
            Some(bridge_actor.clone()),
            Some(pegin_actor.clone()),
            Some(pegout_actor.clone()),
            Some(stream_actor.clone()),
        );

        self.state_sync_manager.register_actors(
            Some(bridge_actor.clone()),
            Some(pegin_actor.clone()),
            Some(pegout_actor.clone()),
            Some(stream_actor.clone()),
        );

        // Initialize specialized orchestrators
        self.pegin_orchestrator = Some(PegInWorkflowOrchestrator::new(
            bridge_actor.clone(),
            pegin_actor.clone(),
            self.coordination_manager.clone(),
            self.state_sync_manager.clone(),
        ));

        self.pegout_orchestrator = Some(PegOutWorkflowOrchestrator::new(
            bridge_actor,
            pegout_actor,
            self.coordination_manager.clone(),
            self.state_sync_manager.clone(),
        ));

        // Start state synchronization
        self.state_sync_manager.start_periodic_sync().await
            .map_err(|e| OrchestratorError::InitializationFailed(e.to_string()))?;

        // Initialize workflow monitoring
        self.workflow_monitor.initialize().await
            .map_err(|e| OrchestratorError::InitializationFailed(e.to_string()))?;

        // Update system health
        self.system_health.overall_status = OverallStatus::Healthy;
        self.system_health.last_health_check = SystemTime::now();

        info!("Bridge Workflow Orchestrator initialized successfully");
        Ok(())
    }

    /// Initiate peg-in workflow
    pub async fn initiate_pegin(
        &mut self,
        bitcoin_txid: bitcoin::Txid,
        recipient: ethereum_types::Address,
        amount: u64,
        required_confirmations: u32,
    ) -> Result<String, OrchestratorError> {
        if self.get_active_workflow_count() >= self.max_concurrent_workflows {
            return Err(OrchestratorError::TooManyConcurrentWorkflows);
        }

        if let Some(pegin_orchestrator) = &mut self.pegin_orchestrator {
            let workflow_id = pegin_orchestrator
                .initiate_pegin_workflow(bitcoin_txid, recipient, amount, required_confirmations)
                .await
                .map_err(|e| OrchestratorError::WorkflowInitiationFailed(e.to_string()))?;

            // Update metrics
            self.orchestrator_metrics.total_workflows_initiated += 1;
            let current_concurrent = self.get_active_workflow_count() as u32;
            if current_concurrent > self.orchestrator_metrics.concurrent_workflows_peak {
                self.orchestrator_metrics.concurrent_workflows_peak = current_concurrent;
            }

            // Start monitoring
            self.workflow_monitor.start_monitoring_workflow(&workflow_id, "pegin".to_string()).await?;

            info!("Initiated peg-in workflow: {}", workflow_id);
            Ok(workflow_id)
        } else {
            Err(OrchestratorError::ComponentNotInitialized("PegIn orchestrator".to_string()))
        }
    }

    /// Initiate peg-out workflow
    pub async fn initiate_pegout(
        &mut self,
        burn_tx_hash: ethereum_types::H256,
        bitcoin_destination: bitcoin::Address,
        amount: u64,
        fee_rate: u64,
        required_signatures: u32,
    ) -> Result<String, OrchestratorError> {
        if self.get_active_workflow_count() >= self.max_concurrent_workflows {
            return Err(OrchestratorError::TooManyConcurrentWorkflows);
        }

        if let Some(pegout_orchestrator) = &mut self.pegout_orchestrator {
            let workflow_id = pegout_orchestrator
                .initiate_pegout_workflow(burn_tx_hash, bitcoin_destination, amount, fee_rate, required_signatures)
                .await
                .map_err(|e| OrchestratorError::WorkflowInitiationFailed(e.to_string()))?;

            // Update metrics
            self.orchestrator_metrics.total_workflows_initiated += 1;
            let current_concurrent = self.get_active_workflow_count() as u32;
            if current_concurrent > self.orchestrator_metrics.concurrent_workflows_peak {
                self.orchestrator_metrics.concurrent_workflows_peak = current_concurrent;
            }

            // Start monitoring
            self.workflow_monitor.start_monitoring_workflow(&workflow_id, "pegout".to_string()).await?;

            info!("Initiated peg-out workflow: {}", workflow_id);
            Ok(workflow_id)
        } else {
            Err(OrchestratorError::ComponentNotInitialized("PegOut orchestrator".to_string()))
        }
    }

    /// Perform system health check
    pub async fn perform_health_check(&mut self) -> Result<SystemHealthStatus, OrchestratorError> {
        info!("Performing comprehensive system health check");
        self.orchestrator_metrics.health_checks_performed += 1;

        let check_start = SystemTime::now();

        // Check PegIn component health
        let pegin_health = if let Some(pegin_orchestrator) = &self.pegin_orchestrator {
            let metrics = pegin_orchestrator.get_metrics();
            let total_workflows = metrics.total_workflows;
            let failed_workflows = metrics.failed_workflows;
            
            let error_rate = if total_workflows > 0 {
                failed_workflows as f64 / total_workflows as f64
            } else {
                0.0
            };

            let status = if error_rate > 0.2 {
                ComponentStatus::Failed
            } else if error_rate > 0.1 {
                ComponentStatus::Degraded
            } else {
                ComponentStatus::Healthy
            };

            ComponentHealth {
                status,
                error_rate,
                response_time: Duration::from_millis(50), // Placeholder
                last_error: None,
            }
        } else {
            ComponentHealth {
                status: ComponentStatus::Unknown,
                error_rate: 0.0,
                response_time: Duration::from_millis(0),
                last_error: Some("PegIn orchestrator not initialized".to_string()),
            }
        };

        // Check PegOut component health
        let pegout_health = if let Some(pegout_orchestrator) = &self.pegout_orchestrator {
            let metrics = pegout_orchestrator.get_metrics();
            let total_workflows = metrics.total_workflows;
            let failed_workflows = metrics.failed_workflows;
            
            let error_rate = if total_workflows > 0 {
                failed_workflows as f64 / total_workflows as f64
            } else {
                0.0
            };

            let status = if error_rate > 0.2 {
                ComponentStatus::Failed
            } else if error_rate > 0.1 {
                ComponentStatus::Degraded
            } else {
                ComponentStatus::Healthy
            };

            ComponentHealth {
                status,
                error_rate,
                response_time: Duration::from_millis(75), // Placeholder
                last_error: None,
            }
        } else {
            ComponentHealth {
                status: ComponentStatus::Unknown,
                error_rate: 0.0,
                response_time: Duration::from_millis(0),
                last_error: Some("PegOut orchestrator not initialized".to_string()),
            }
        };

        // Check coordination health
        let coordination_metrics = self.coordination_manager.get_metrics();
        let coordination_error_rate = if coordination_metrics.total_operations > 0 {
            coordination_metrics.failed_operations as f64 / coordination_metrics.total_operations as f64
        } else {
            0.0
        };

        let coordination_status = if coordination_error_rate > 0.15 {
            ComponentStatus::Failed
        } else if coordination_error_rate > 0.05 {
            ComponentStatus::Degraded
        } else {
            ComponentStatus::Healthy
        };

        let coordination_health = ComponentHealth {
            status: coordination_status,
            error_rate: coordination_error_rate,
            response_time: coordination_metrics.average_completion_time,
            last_error: None,
        };

        // Determine overall status
        let overall_status = match (
            pegin_health.status.clone(),
            pegout_health.status.clone(),
            coordination_health.status.clone(),
        ) {
            (ComponentStatus::Healthy, ComponentStatus::Healthy, ComponentStatus::Healthy) => {
                OverallStatus::Healthy
            }
            (ComponentStatus::Failed, _, _)
            | (_, ComponentStatus::Failed, _)
            | (_, _, ComponentStatus::Failed) => OverallStatus::Critical,
            (ComponentStatus::Degraded, _, _)
            | (_, ComponentStatus::Degraded, _)
            | (_, _, ComponentStatus::Degraded) => OverallStatus::Degraded,
            _ => OverallStatus::Unknown,
        };

        self.system_health = SystemHealthStatus {
            overall_status,
            pegin_health,
            pegout_health,
            coordination_health,
            last_health_check: SystemTime::now(),
        };

        let health_check_duration = SystemTime::now().duration_since(check_start).unwrap_or_default();
        info!("Health check completed in {:?}, overall status: {:?}", 
              health_check_duration, self.system_health.overall_status);

        Ok(self.system_health.clone())
    }

    /// Get current active workflow count
    pub fn get_active_workflow_count(&self) -> usize {
        let pegin_count = self.pegin_orchestrator
            .as_ref()
            .map(|o| o.get_active_workflows().len())
            .unwrap_or(0);

        let pegout_count = self.pegout_orchestrator
            .as_ref()
            .map(|o| o.get_active_workflows().len())
            .unwrap_or(0);

        pegin_count + pegout_count
    }

    /// Get comprehensive workflow statistics
    pub fn get_workflow_statistics(&self) -> WorkflowStatistics {
        let pegin_metrics = self.pegin_orchestrator
            .as_ref()
            .map(|o| o.get_metrics().clone())
            .unwrap_or_default();

        let pegout_metrics = self.pegout_orchestrator
            .as_ref()
            .map(|o| o.get_metrics().clone())
            .unwrap_or_default();

        WorkflowStatistics {
            pegin_metrics,
            pegout_metrics,
            orchestrator_metrics: self.orchestrator_metrics.clone(),
            system_health: self.system_health.clone(),
            active_workflow_count: self.get_active_workflow_count(),
        }
    }

    /// Process periodic maintenance tasks
    pub async fn process_maintenance(&mut self) -> Result<(), OrchestratorError> {
        // Process coordination operations
        let completed_coords = self.coordination_manager.process_operations();
        self.orchestrator_metrics.coordination_operations += completed_coords.len() as u64;

        // Detect and handle state inconsistencies
        let inconsistencies = self.state_sync_manager.detect_inconsistencies();
        if !inconsistencies.is_empty() {
            warn!("Detected {} state inconsistencies", inconsistencies.len());
            // Handle critical inconsistencies
            for inconsistency in &inconsistencies {
                if matches!(inconsistency.severity, crate::actors::bridge::integration::state_sync::InconsistencySeverity::Critical) {
                    error!("Critical state inconsistency detected: {:?}", inconsistency);
                }
            }
        }

        // Update system uptime
        self.orchestrator_metrics.system_uptime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        // Process workflow monitoring
        self.workflow_monitor.process_monitoring().await?;

        Ok(())
    }
}

/// Comprehensive workflow statistics
#[derive(Debug)]
pub struct WorkflowStatistics {
    pub pegin_metrics: PegInWorkflowMetrics,
    pub pegout_metrics: PegOutWorkflowMetrics,
    pub orchestrator_metrics: OrchestratorMetrics,
    pub system_health: SystemHealthStatus,
    pub active_workflow_count: usize,
}

/// Orchestrator errors
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("Initialization failed: {0}")]
    InitializationFailed(String),
    
    #[error("Component not initialized: {0}")]
    ComponentNotInitialized(String),
    
    #[error("Workflow initiation failed: {0}")]
    WorkflowInitiationFailed(String),
    
    #[error("Too many concurrent workflows")]
    TooManyConcurrentWorkflows,
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    
    #[error("Monitoring error: {0}")]
    MonitoringError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}