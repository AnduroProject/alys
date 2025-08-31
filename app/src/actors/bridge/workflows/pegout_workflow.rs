//! Peg-Out Workflow Implementation
//! 
//! Complete end-to-end peg-out workflow coordination

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error};

use crate::actors::bridge::{
    messages::*,
    actors::{bridge::BridgeActor, pegout::PegOutActor},
    integration::{CoordinationManager, StateSyncManager},
    shared::{
        validation::{ValidationEngine, ValidationResult},
        utxo::UtxoSelectionStrategy,
        federation::FederationConfig,
    },
};

/// Complete peg-out workflow orchestrator
pub struct PegOutWorkflowOrchestrator {
    /// Actor addresses
    bridge_actor: Addr<BridgeActor>,
    pegout_actor: Addr<PegOutActor>,
    
    /// Workflow components
    coordination_manager: CoordinationManager,
    state_sync_manager: StateSyncManager,
    validation_engine: ValidationEngine,
    
    /// Active workflows
    active_workflows: HashMap<String, PegOutWorkflow>,
    workflow_metrics: PegOutWorkflowMetrics,
}

/// Peg-out workflow state machine
#[derive(Debug, Clone)]
pub struct PegOutWorkflow {
    pub workflow_id: String,
    pub burn_tx_hash: ethereum_types::H256,
    pub bitcoin_destination: bitcoin::Address,
    pub amount: u64,
    pub fee_rate: u64,
    pub status: PegOutWorkflowStatus,
    pub current_step: PegOutWorkflowStep,
    pub started_at: SystemTime,
    pub required_signatures: u32,
    pub collected_signatures: u32,
    pub error_count: u32,
    pub retry_attempts: HashMap<PegOutWorkflowStep, u32>,
    pub validation_results: Vec<ValidationResult>,
    pub step_history: Vec<WorkflowStepRecord>,
    pub bitcoin_transaction: Option<bitcoin::Transaction>,
    pub selected_utxos: Vec<bitcoin::OutPoint>,
}

/// Peg-out workflow status
#[derive(Debug, Clone, PartialEq)]
pub enum PegOutWorkflowStatus {
    Initiated,
    ValidatingBurn,
    SelectingUtxos,
    BuildingTransaction,
    CollectingSignatures,
    Broadcasting,
    Completed,
    Failed(String),
    Cancelled,
}

/// Peg-out workflow steps
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum PegOutWorkflowStep {
    BurnValidation,
    UtxoSelection,
    TransactionConstruction,
    SignatureCollection,
    TransactionValidation,
    Broadcasting,
    ConfirmationMonitoring,
    CompletionNotification,
}

/// Workflow step record for audit trail
#[derive(Debug, Clone)]
pub struct WorkflowStepRecord {
    pub step: PegOutWorkflowStep,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub status: StepStatus,
    pub details: String,
    pub error: Option<String>,
}

/// Step execution status
#[derive(Debug, Clone)]
pub enum StepStatus {
    InProgress,
    Completed,
    Failed,
    Retrying,
    Skipped,
}

/// Peg-out workflow metrics
#[derive(Debug, Default)]
pub struct PegOutWorkflowMetrics {
    pub total_workflows: u64,
    pub completed_workflows: u64,
    pub failed_workflows: u64,
    pub average_completion_time: Duration,
    pub average_signature_collection_time: Duration,
    pub active_workflows_count: u32,
    pub step_success_rates: HashMap<PegOutWorkflowStep, f64>,
    pub error_distribution: HashMap<String, u32>,
    pub utxo_selection_stats: UtxoSelectionStats,
}

/// UTXO selection statistics
#[derive(Debug, Default)]
pub struct UtxoSelectionStats {
    pub total_selections: u64,
    pub average_utxos_per_transaction: f64,
    pub fee_efficiency_score: f64,
}

impl PegOutWorkflowOrchestrator {
    pub fn new(
        bridge_actor: Addr<BridgeActor>,
        pegout_actor: Addr<PegOutActor>,
        coordination_manager: CoordinationManager,
        state_sync_manager: StateSyncManager,
    ) -> Self {
        let validation_engine = ValidationEngine::new();
        
        Self {
            bridge_actor,
            pegout_actor,
            coordination_manager,
            state_sync_manager,
            validation_engine,
            active_workflows: HashMap::new(),
            workflow_metrics: PegOutWorkflowMetrics::default(),
        }
    }

    /// Initiate complete peg-out workflow
    pub async fn initiate_pegout_workflow(
        &mut self,
        burn_tx_hash: ethereum_types::H256,
        bitcoin_destination: bitcoin::Address,
        amount: u64,
        fee_rate: u64,
        required_signatures: u32,
    ) -> Result<String, PegOutWorkflowError> {
        let workflow_id = format!("pegout_{}", uuid::Uuid::new_v4());
        
        info!("Initiating peg-out workflow {} for burn tx {:?}", workflow_id, burn_tx_hash);

        // Create workflow state
        let workflow = PegOutWorkflow {
            workflow_id: workflow_id.clone(),
            burn_tx_hash,
            bitcoin_destination,
            amount,
            fee_rate,
            status: PegOutWorkflowStatus::Initiated,
            current_step: PegOutWorkflowStep::BurnValidation,
            started_at: SystemTime::now(),
            required_signatures,
            collected_signatures: 0,
            error_count: 0,
            retry_attempts: HashMap::new(),
            validation_results: Vec::new(),
            step_history: Vec::new(),
            bitcoin_transaction: None,
            selected_utxos: Vec::new(),
        };

        self.active_workflows.insert(workflow_id.clone(), workflow);
        self.workflow_metrics.total_workflows += 1;
        self.workflow_metrics.active_workflows_count += 1;

        // Start burn validation
        self.execute_workflow_step(&workflow_id, PegOutWorkflowStep::BurnValidation).await?;

        Ok(workflow_id)
    }

    /// Execute specific workflow step
    async fn execute_workflow_step(
        &mut self,
        workflow_id: &str,
        step: PegOutWorkflowStep,
    ) -> Result<(), PegOutWorkflowError> {
        if let Some(workflow) = self.active_workflows.get_mut(workflow_id) {
            // Record step start
            let step_record = WorkflowStepRecord {
                step: step.clone(),
                started_at: SystemTime::now(),
                completed_at: None,
                status: StepStatus::InProgress,
                details: String::new(),
                error: None,
            };
            workflow.step_history.push(step_record);
            workflow.current_step = step.clone();

            // Execute step
            let result = match &step {
                PegOutWorkflowStep::BurnValidation => {
                    self.execute_burn_validation(workflow).await
                }
                PegOutWorkflowStep::UtxoSelection => {
                    self.execute_utxo_selection(workflow).await
                }
                PegOutWorkflowStep::TransactionConstruction => {
                    self.execute_transaction_construction(workflow).await
                }
                PegOutWorkflowStep::SignatureCollection => {
                    self.execute_signature_collection(workflow).await
                }
                PegOutWorkflowStep::TransactionValidation => {
                    self.execute_transaction_validation(workflow).await
                }
                PegOutWorkflowStep::Broadcasting => {
                    self.execute_broadcasting(workflow).await
                }
                PegOutWorkflowStep::ConfirmationMonitoring => {
                    self.execute_confirmation_monitoring(workflow).await
                }
                PegOutWorkflowStep::CompletionNotification => {
                    self.execute_completion_notification(workflow).await
                }
            };

            // Update step record and handle result
            if let Some(last_step) = workflow.step_history.last_mut() {
                last_step.completed_at = Some(SystemTime::now());
                match &result {
                    Ok(_) => {
                        last_step.status = StepStatus::Completed;
                        info!("Completed step {:?} for workflow {}", step, workflow_id);
                        
                        // Move to next step
                        if let Some(next_step) = self.get_next_step(&step) {
                            workflow.current_step = next_step.clone();
                            Box::pin(self.execute_workflow_step(workflow_id, next_step)).await?;
                        } else {
                            // Workflow complete
                            workflow.status = PegOutWorkflowStatus::Completed;
                            self.complete_workflow(workflow_id).await?;
                        }
                    }
                    Err(e) => {
                        last_step.status = StepStatus::Failed;
                        last_step.error = Some(e.to_string());
                        workflow.error_count += 1;
                        
                        // Handle retry logic
                        if self.should_retry_step(&step, workflow) {
                            warn!("Retrying step {:?} for workflow {}", step, workflow_id);
                            let retry_count = workflow.retry_attempts.entry(step.clone()).or_insert(0);
                            *retry_count += 1;
                            
                            // Wait before retry
                            let delay = self.calculate_retry_delay(*retry_count);
                            tokio::time::sleep(delay).await;
                            
                            last_step.status = StepStatus::Retrying;
                            Box::pin(self.execute_workflow_step(workflow_id, step)).await?;
                        } else {
                            // Fail workflow
                            workflow.status = PegOutWorkflowStatus::Failed(e.to_string());
                            self.fail_workflow(workflow_id, e.to_string()).await?;
                        }
                    }
                }
            }

            Ok(())
        } else {
            Err(PegOutWorkflowError::WorkflowNotFound(workflow_id.to_string()))
        }
    }

    /// Execute burn validation step
    async fn execute_burn_validation(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing burn validation for workflow {}", workflow.workflow_id);

        // Validate burn transaction and extract parameters
        let validation_result = self.validation_engine.validate_burn_transaction(
            workflow.burn_tx_hash,
            workflow.amount,
            workflow.bitcoin_destination.clone(),
        ).await.map_err(|e| PegOutWorkflowError::ValidationFailed(e.to_string()))?;

        workflow.validation_results.push(validation_result);
        workflow.status = PegOutWorkflowStatus::ValidatingBurn;

        Ok(())
    }

    /// Execute UTXO selection step
    async fn execute_utxo_selection(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing UTXO selection for workflow {}", workflow.workflow_id);

        // Request UTXO selection from PegOut actor
        let selection_msg = PegOutMessage::SelectUtxos {
            amount: workflow.amount,
            fee_rate: workflow.fee_rate,
            strategy: UtxoSelectionStrategy::BranchAndBound,
        };

        let utxos = self.pegout_actor.send(selection_msg).await
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(e.to_string()))?
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(format!("{:?}", e)))?;

        workflow.selected_utxos = utxos;
        workflow.status = PegOutWorkflowStatus::SelectingUtxos;

        // Update selection statistics
        self.workflow_metrics.utxo_selection_stats.total_selections += 1;
        let current_avg = self.workflow_metrics.utxo_selection_stats.average_utxos_per_transaction;
        let total = self.workflow_metrics.utxo_selection_stats.total_selections;
        let new_avg = (current_avg * (total - 1) as f64 + workflow.selected_utxos.len() as f64) / total as f64;
        self.workflow_metrics.utxo_selection_stats.average_utxos_per_transaction = new_avg;

        Ok(())
    }

    /// Execute transaction construction step
    async fn execute_transaction_construction(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing transaction construction for workflow {}", workflow.workflow_id);

        // Build Bitcoin transaction
        let construction_msg = PegOutMessage::BuildTransaction {
            withdrawal_id: workflow.workflow_id.clone(),
            destination: workflow.bitcoin_destination.clone(),
            amount: workflow.amount,
            fee_rate: workflow.fee_rate,
        };

        let transaction = self.pegout_actor.send(construction_msg).await
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(e.to_string()))?
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(format!("{:?}", e)))?;

        workflow.bitcoin_transaction = Some(transaction);
        workflow.status = PegOutWorkflowStatus::BuildingTransaction;

        Ok(())
    }

    /// Execute signature collection step
    async fn execute_signature_collection(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing signature collection for workflow {}", workflow.workflow_id);
        let collection_start = SystemTime::now();

        if let Some(transaction) = &workflow.bitcoin_transaction {
            // Request signatures from federation members
            let signature_msg = PegOutMessage::CollectSignatures {
                withdrawal_id: workflow.workflow_id.clone(),
                transaction: transaction.clone(),
                required_signatures: workflow.required_signatures,
            };

            let signatures = self.pegout_actor.send(signature_msg).await
                .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(e.to_string()))?
                .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(format!("{:?}", e)))?;

            workflow.collected_signatures = signatures.len() as u32;
            workflow.status = PegOutWorkflowStatus::CollectingSignatures;

            if workflow.collected_signatures >= workflow.required_signatures {
                // Update signature collection timing
                let collection_time = SystemTime::now().duration_since(collection_start).unwrap_or_default();
                let completed = self.workflow_metrics.completed_workflows + 1;
                let current_total = self.workflow_metrics.average_signature_collection_time * (completed - 1) as u32;
                self.workflow_metrics.average_signature_collection_time = (current_total + collection_time) / completed as u32;
                
                Ok(())
            } else {
                Err(PegOutWorkflowError::InsufficientSignatures {
                    collected: workflow.collected_signatures,
                    required: workflow.required_signatures,
                })
            }
        } else {
            Err(PegOutWorkflowError::TransactionNotConstructed)
        }
    }

    /// Execute transaction validation step
    async fn execute_transaction_validation(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing transaction validation for workflow {}", workflow.workflow_id);

        if let Some(transaction) = &workflow.bitcoin_transaction {
            // Validate fully signed transaction
            let validation_result = self.validation_engine.validate_signed_transaction(
                transaction.clone(),
                workflow.amount,
                workflow.bitcoin_destination.clone(),
                workflow.collected_signatures,
            ).await.map_err(|e| PegOutWorkflowError::ValidationFailed(e.to_string()))?;

            workflow.validation_results.push(validation_result);
            Ok(())
        } else {
            Err(PegOutWorkflowError::TransactionNotConstructed)
        }
    }

    /// Execute broadcasting step
    async fn execute_broadcasting(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing broadcasting for workflow {}", workflow.workflow_id);

        if let Some(transaction) = &workflow.bitcoin_transaction {
            // Broadcast transaction to Bitcoin network
            let broadcast_msg = PegOutMessage::BroadcastTransaction {
                withdrawal_id: workflow.workflow_id.clone(),
                transaction: transaction.clone(),
            };

            let txid = self.pegout_actor.send(broadcast_msg).await
                .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(e.to_string()))?
                .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(format!("{:?}", e)))?;

            workflow.status = PegOutWorkflowStatus::Broadcasting;
            info!("Broadcast peg-out transaction {} for workflow {}", txid, workflow.workflow_id);

            Ok(())
        } else {
            Err(PegOutWorkflowError::TransactionNotConstructed)
        }
    }

    /// Execute confirmation monitoring step
    async fn execute_confirmation_monitoring(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing confirmation monitoring for workflow {}", workflow.workflow_id);

        // Monitor transaction confirmations
        let monitor_msg = PegOutMessage::MonitorConfirmations {
            withdrawal_id: workflow.workflow_id.clone(),
            required_confirmations: 1, // Just one confirmation for completion
        };

        let confirmed = self.pegout_actor.send(monitor_msg).await
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(e.to_string()))?
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(format!("{:?}", e)))?;

        if confirmed {
            Ok(())
        } else {
            Err(PegOutWorkflowError::ConfirmationTimeout)
        }
    }

    /// Execute completion notification step
    async fn execute_completion_notification(
        &mut self,
        workflow: &mut PegOutWorkflow,
    ) -> Result<(), PegOutWorkflowError> {
        info!("Executing completion notification for workflow {}", workflow.workflow_id);

        // Notify all relevant actors of successful completion
        let completion_msg = BridgeCoordinationMessage::PegOutCompleted {
            pegout_id: workflow.workflow_id.clone(),
            burn_tx_hash: workflow.burn_tx_hash,
            bitcoin_destination: workflow.bitcoin_destination.clone(),
            amount: workflow.amount,
        };

        self.bridge_actor.send(completion_msg).await
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(e.to_string()))?
            .map_err(|e| PegOutWorkflowError::ActorCommunicationFailed(format!("{:?}", e)))?;

        Ok(())
    }

    /// Get next step in workflow
    fn get_next_step(&self, current_step: &PegOutWorkflowStep) -> Option<PegOutWorkflowStep> {
        match current_step {
            PegOutWorkflowStep::BurnValidation => Some(PegOutWorkflowStep::UtxoSelection),
            PegOutWorkflowStep::UtxoSelection => Some(PegOutWorkflowStep::TransactionConstruction),
            PegOutWorkflowStep::TransactionConstruction => Some(PegOutWorkflowStep::SignatureCollection),
            PegOutWorkflowStep::SignatureCollection => Some(PegOutWorkflowStep::TransactionValidation),
            PegOutWorkflowStep::TransactionValidation => Some(PegOutWorkflowStep::Broadcasting),
            PegOutWorkflowStep::Broadcasting => Some(PegOutWorkflowStep::ConfirmationMonitoring),
            PegOutWorkflowStep::ConfirmationMonitoring => Some(PegOutWorkflowStep::CompletionNotification),
            PegOutWorkflowStep::CompletionNotification => None,
        }
    }

    /// Check if step should be retried
    fn should_retry_step(&self, step: &PegOutWorkflowStep, workflow: &PegOutWorkflow) -> bool {
        let max_retries = match step {
            PegOutWorkflowStep::UtxoSelection => 5,
            PegOutWorkflowStep::SignatureCollection => 10,
            PegOutWorkflowStep::Broadcasting => 3,
            PegOutWorkflowStep::ConfirmationMonitoring => 20,
            _ => 3,
        };

        let retry_count = workflow.retry_attempts.get(step).unwrap_or(&0);
        *retry_count < max_retries
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, retry_count: u32) -> Duration {
        let base_delay = Duration::from_secs(45);
        let max_delay = Duration::from_secs(600);
        
        let delay = base_delay * 2_u32.pow(retry_count.min(8));
        delay.min(max_delay)
    }

    /// Complete workflow successfully
    async fn complete_workflow(&mut self, workflow_id: &str) -> Result<(), PegOutWorkflowError> {
        if let Some(workflow) = self.active_workflows.remove(workflow_id) {
            let completion_time = SystemTime::now()
                .duration_since(workflow.started_at)
                .unwrap_or_default();

            // Update metrics
            self.workflow_metrics.completed_workflows += 1;
            self.workflow_metrics.active_workflows_count -= 1;
            
            // Update average completion time
            let total_completed = self.workflow_metrics.completed_workflows;
            let current_total = self.workflow_metrics.average_completion_time * (total_completed - 1) as u32;
            self.workflow_metrics.average_completion_time = (current_total + completion_time) / total_completed as u32;

            info!("Successfully completed peg-out workflow {} in {:?}", workflow_id, completion_time);
        }

        Ok(())
    }

    /// Fail workflow with error
    async fn fail_workflow(&mut self, workflow_id: &str, error: String) -> Result<(), PegOutWorkflowError> {
        if let Some(_workflow) = self.active_workflows.remove(workflow_id) {
            self.workflow_metrics.failed_workflows += 1;
            self.workflow_metrics.active_workflows_count -= 1;
            
            // Track error type
            let error_type = self.classify_error(&error);
            let count = self.workflow_metrics.error_distribution.entry(error_type).or_insert(0);
            *count += 1;

            error!("Failed peg-out workflow {}: {}", workflow_id, error);
        }

        Ok(())
    }

    /// Classify error for metrics
    fn classify_error(&self, error: &str) -> String {
        if error.contains("validation") {
            "Validation Error".to_string()
        } else if error.contains("signature") {
            "Signature Error".to_string()
        } else if error.contains("broadcast") {
            "Broadcasting Error".to_string()
        } else if error.contains("utxo") {
            "UTXO Error".to_string()
        } else if error.contains("communication") {
            "Communication Error".to_string()
        } else {
            "Unknown Error".to_string()
        }
    }

    /// Get workflow metrics
    pub fn get_metrics(&self) -> &PegOutWorkflowMetrics {
        &self.workflow_metrics
    }

    /// Get active workflows
    pub fn get_active_workflows(&self) -> &HashMap<String, PegOutWorkflow> {
        &self.active_workflows
    }
}

/// Peg-out workflow errors
#[derive(Debug, thiserror::Error)]
pub enum PegOutWorkflowError {
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Actor communication failed: {0}")]
    ActorCommunicationFailed(String),
    
    #[error("Insufficient signatures: {collected}/{required}")]
    InsufficientSignatures { collected: u32, required: u32 },
    
    #[error("Transaction not constructed")]
    TransactionNotConstructed,
    
    #[error("Confirmation timeout")]
    ConfirmationTimeout,
    
    #[error("UTXO selection failed: {0}")]
    UtxoSelectionFailed(String),
    
    #[error("Broadcasting failed: {0}")]
    BroadcastingFailed(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}