//! Inter-Actor Coordination
//! 
//! Patterns and utilities for coordinating between bridge actors

use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tracing::{info, warn, error};

use crate::actors::bridge::{
    messages::*,
    actors::{bridge::BridgeActor, pegin::PegInActor, pegout::PegOutActor, stream::StreamActor},
};

/// Coordination manager for bridge operations
pub struct CoordinationManager {
    /// Actor addresses for coordination
    bridge_actor: Option<Addr<BridgeActor>>,
    pegin_actor: Option<Addr<PegInActor>>,
    pegout_actor: Option<Addr<PegOutActor>>,
    stream_actor: Option<Addr<StreamActor>>,
    
    /// Coordination state
    active_operations: HashMap<String, CoordinationOperation>,
    coordination_metrics: CoordinationMetrics,
}

/// Coordination operation tracking
#[derive(Debug, Clone)]
pub struct CoordinationOperation {
    pub operation_id: String,
    pub operation_type: CoordinationType,
    pub participants: Vec<ActorParticipant>,
    pub started_at: SystemTime,
    pub timeout: Duration,
    pub status: CoordinationStatus,
    pub step_count: u32,
    pub error_count: u32,
}

/// Types of coordination operations
#[derive(Debug, Clone)]
pub enum CoordinationType {
    PegIn {
        bitcoin_txid: bitcoin::Txid,
        amount: u64,
        destination: ethereum_types::Address,
    },
    PegOut {
        burn_tx_hash: ethereum_types::H256,
        amount: u64,
        destination: bitcoin::Address,
    },
    HealthSync,
    ConfigUpdate,
    EmergencyHalt,
}

/// Actor participation in coordination
#[derive(Debug, Clone)]
pub struct ActorParticipant {
    pub actor_type: ActorType,
    pub required: bool,
    pub status: ParticipantStatus,
    pub last_response: Option<SystemTime>,
}

/// Actor types for coordination
#[derive(Debug, Clone, PartialEq)]
pub enum ActorType {
    Bridge,
    PegIn,
    PegOut,
    Stream,
}

/// Participant status in coordination
#[derive(Debug, Clone)]
pub enum ParticipantStatus {
    Pending,
    Acknowledged,
    InProgress,
    Completed,
    Failed(String),
    Timeout,
}

/// Coordination status
#[derive(Debug, Clone)]
pub enum CoordinationStatus {
    Initiated,
    InProgress,
    WaitingForResponses,
    Completed,
    Failed(String),
    TimedOut,
}

/// Coordination metrics
#[derive(Debug, Default)]
pub struct CoordinationMetrics {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub timed_out_operations: u64,
    pub average_completion_time: Duration,
    pub active_operations_count: u32,
}

impl CoordinationManager {
    pub fn new() -> Self {
        Self {
            bridge_actor: None,
            pegin_actor: None,
            pegout_actor: None,
            stream_actor: None,
            active_operations: HashMap::new(),
            coordination_metrics: CoordinationMetrics::default(),
        }
    }

    /// Register actors for coordination
    pub fn register_actors(
        &mut self,
        bridge_actor: Option<Addr<BridgeActor>>,
        pegin_actor: Option<Addr<PegInActor>>,
        pegout_actor: Option<Addr<PegOutActor>>,
        stream_actor: Option<Addr<StreamActor>>,
    ) {
        self.bridge_actor = bridge_actor;
        self.pegin_actor = pegin_actor;
        self.pegout_actor = pegout_actor;
        self.stream_actor = stream_actor;
        
        info!("Actors registered for coordination");
    }

    /// Initiate peg-in coordination
    pub async fn coordinate_pegin(
        &mut self,
        bitcoin_txid: bitcoin::Txid,
        amount: u64,
        destination: ethereum_types::Address,
    ) -> Result<String, CoordinationError> {
        let operation_id = format!("pegin_{}", uuid::Uuid::new_v4());
        
        let participants = vec![
            ActorParticipant {
                actor_type: ActorType::Bridge,
                required: true,
                status: ParticipantStatus::Pending,
                last_response: None,
            },
            ActorParticipant {
                actor_type: ActorType::PegIn,
                required: true,
                status: ParticipantStatus::Pending,
                last_response: None,
            },
        ];

        let operation = CoordinationOperation {
            operation_id: operation_id.clone(),
            operation_type: CoordinationType::PegIn {
                bitcoin_txid,
                amount,
                destination,
            },
            participants,
            started_at: SystemTime::now(),
            timeout: Duration::from_secs(300), // 5 minutes
            status: CoordinationStatus::Initiated,
            step_count: 0,
            error_count: 0,
        };

        self.active_operations.insert(operation_id.clone(), operation);
        self.coordination_metrics.total_operations += 1;
        self.coordination_metrics.active_operations_count += 1;

        info!("Initiated peg-in coordination: {}", operation_id);

        // Notify participants
        self.notify_pegin_participants(&operation_id, bitcoin_txid, amount, destination).await?;

        Ok(operation_id)
    }

    /// Initiate peg-out coordination
    pub async fn coordinate_pegout(
        &mut self,
        burn_tx_hash: ethereum_types::H256,
        amount: u64,
        destination: bitcoin::Address,
    ) -> Result<String, CoordinationError> {
        let operation_id = format!("pegout_{}", uuid::Uuid::new_v4());
        
        let participants = vec![
            ActorParticipant {
                actor_type: ActorType::Bridge,
                required: true,
                status: ParticipantStatus::Pending,
                last_response: None,
            },
            ActorParticipant {
                actor_type: ActorType::PegOut,
                required: true,
                status: ParticipantStatus::Pending,
                last_response: None,
            },
        ];

        let operation = CoordinationOperation {
            operation_id: operation_id.clone(),
            operation_type: CoordinationType::PegOut {
                burn_tx_hash,
                amount,
                destination: destination.clone(),
            },
            participants,
            started_at: SystemTime::now(),
            timeout: Duration::from_secs(600), // 10 minutes
            status: CoordinationStatus::Initiated,
            step_count: 0,
            error_count: 0,
        };

        self.active_operations.insert(operation_id.clone(), operation);
        self.coordination_metrics.total_operations += 1;
        self.coordination_metrics.active_operations_count += 1;

        info!("Initiated peg-out coordination: {}", operation_id);

        // Notify participants
        self.notify_pegout_participants(&operation_id, burn_tx_hash, amount, destination).await?;

        Ok(operation_id)
    }

    /// Notify peg-in participants
    async fn notify_pegin_participants(
        &self,
        operation_id: &str,
        bitcoin_txid: bitcoin::Txid,
        amount: u64,
        destination: ethereum_types::Address,
    ) -> Result<(), CoordinationError> {
        // Notify Bridge Actor
        if let Some(bridge_actor) = &self.bridge_actor {
            let msg = BridgeCoordinationMessage::CoordinatePegIn {
                pegin_id: operation_id.to_string(),
                bitcoin_txid,
            };
            
            bridge_actor.send(msg).await
                .map_err(|e| CoordinationError::NotificationFailed(format!("Bridge: {}", e)))?
                .map_err(|e| CoordinationError::NotificationFailed(format!("Bridge: {:?}", e)))?;
        }

        // Notify PegIn Actor
        if let Some(pegin_actor) = &self.pegin_actor {
            // Create placeholder transaction for coordination context
            let placeholder_tx = bitcoin::Transaction {
                version: 2,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![],
                output: vec![],
            };

            let msg = PegInMessage::ProcessDeposit {
                txid: bitcoin_txid,
                bitcoin_tx: placeholder_tx,
                block_height: 0, // Will be updated when block is confirmed
            };
            
            pegin_actor.send(msg).await
                .map_err(|e| CoordinationError::NotificationFailed(format!("PegIn: {}", e)))?
                .map_err(|e| CoordinationError::NotificationFailed(format!("PegIn: {:?}", e)))?;
        }

        Ok(())
    }

    /// Notify peg-out participants
    async fn notify_pegout_participants(
        &self,
        operation_id: &str,
        burn_tx_hash: ethereum_types::H256,
        amount: u64,
        destination: bitcoin::Address,
    ) -> Result<(), CoordinationError> {
        // Notify Bridge Actor
        if let Some(bridge_actor) = &self.bridge_actor {
            let msg = BridgeCoordinationMessage::CoordinatePegOut {
                pegout_id: operation_id.to_string(),
                burn_tx_hash,
            };
            
            bridge_actor.send(msg).await
                .map_err(|e| CoordinationError::NotificationFailed(format!("Bridge: {}", e)))?
                .map_err(|e| CoordinationError::NotificationFailed(format!("Bridge: {:?}", e)))?;
        }

        // Notify PegOut Actor
        if let Some(pegout_actor) = &self.pegout_actor {
            let msg = PegOutMessage::ProcessWithdrawal {
                pegout_id: operation_id.to_string(),
                destination: destination.to_string(),
                amount,
            };
            
            pegout_actor.send(msg).await
                .map_err(|e| CoordinationError::NotificationFailed(format!("PegOut: {}", e)))?
                .map_err(|e| CoordinationError::NotificationFailed(format!("PegOut: {:?}", e)))?;
        }

        Ok(())
    }

    /// Process coordination timeouts and cleanup
    pub fn process_operations(&mut self) -> Vec<String> {
        let mut completed_operations = Vec::new();
        let now = SystemTime::now();

        for (operation_id, operation) in &mut self.active_operations {
            // Check for timeout
            if now.duration_since(operation.started_at).unwrap_or_default() >= operation.timeout {
                operation.status = CoordinationStatus::TimedOut;
                completed_operations.push(operation_id.clone());
                warn!("Coordination operation {} timed out", operation_id);
                continue;
            }

            // Check participant status
            let all_completed = operation.participants.iter()
                .filter(|p| p.required)
                .all(|p| matches!(p.status, ParticipantStatus::Completed));

            let any_failed = operation.participants.iter()
                .any(|p| matches!(p.status, ParticipantStatus::Failed(_)));

            if all_completed {
                operation.status = CoordinationStatus::Completed;
                completed_operations.push(operation_id.clone());
                info!("Coordination operation {} completed successfully", operation_id);
            } else if any_failed {
                operation.status = CoordinationStatus::Failed("Participant failure".to_string());
                completed_operations.push(operation_id.clone());
                error!("Coordination operation {} failed", operation_id);
            }
        }

        // Clean up completed operations
        for operation_id in &completed_operations {
            if let Some(operation) = self.active_operations.remove(operation_id) {
                self.update_metrics_on_completion(&operation);
            }
        }

        completed_operations
    }

    /// Update metrics when operation completes
    fn update_metrics_on_completion(&mut self, operation: &CoordinationOperation) {
        self.coordination_metrics.active_operations_count -= 1;

        match &operation.status {
            CoordinationStatus::Completed => {
                self.coordination_metrics.successful_operations += 1;
            }
            CoordinationStatus::Failed(_) => {
                self.coordination_metrics.failed_operations += 1;
            }
            CoordinationStatus::TimedOut => {
                self.coordination_metrics.timed_out_operations += 1;
            }
            _ => {}
        }

        // Update average completion time
        let completion_time = SystemTime::now()
            .duration_since(operation.started_at)
            .unwrap_or_default();
        
        let total_completed = self.coordination_metrics.successful_operations + 
                             self.coordination_metrics.failed_operations + 
                             self.coordination_metrics.timed_out_operations;
        
        if total_completed > 0 {
            let current_total = self.coordination_metrics.average_completion_time * (total_completed - 1) as u32;
            self.coordination_metrics.average_completion_time = (current_total + completion_time) / total_completed as u32;
        }
    }

    /// Update participant status
    pub fn update_participant_status(
        &mut self,
        operation_id: &str,
        actor_type: ActorType,
        status: ParticipantStatus,
    ) -> Result<(), CoordinationError> {
        if let Some(operation) = self.active_operations.get_mut(operation_id) {
            for participant in &mut operation.participants {
                if participant.actor_type == actor_type {
                    participant.status = status;
                    participant.last_response = Some(SystemTime::now());
                    return Ok(());
                }
            }
            Err(CoordinationError::ParticipantNotFound(actor_type))
        } else {
            Err(CoordinationError::OperationNotFound(operation_id.to_string()))
        }
    }

    /// Get coordination metrics
    pub fn get_metrics(&self) -> &CoordinationMetrics {
        &self.coordination_metrics
    }

    /// Get active operations count
    pub fn get_active_operations_count(&self) -> usize {
        self.active_operations.len()
    }
}

/// Coordination errors
#[derive(Debug, thiserror::Error)]
pub enum CoordinationError {
    #[error("Operation not found: {0}")]
    OperationNotFound(String),
    
    #[error("Participant not found: {0:?}")]
    ParticipantNotFound(ActorType),
    
    #[error("Notification failed: {0}")]
    NotificationFailed(String),
    
    #[error("Coordination timeout: {0}")]
    Timeout(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}