//! Bridge Actor Message Handlers
//! 
//! Message handling implementation for the bridge coordinator

use actix::prelude::*;
use tracing::{info, warn, error};
use uuid::Uuid;

use super::actor::*;
use crate::actors::bridge::messages::*;
use crate::types::*;

/// Handler for bridge coordination messages
impl Handler<BridgeCoordinationMessage> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<(), BridgeError>>;

    fn handle(&mut self, msg: BridgeCoordinationMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            BridgeCoordinationMessage::InitializeSystem => {
                Box::pin(async move {
                    info!("Received system initialization request");
                    Ok(())
                }.into_actor(self))
            }

            BridgeCoordinationMessage::RegisterPegInActor(addr) => {
                info!("Registering PegInActor with bridge coordinator");
                self.child_actors.pegin_actor = Some(addr);
                self.metrics.record_actor_registration(ActorType::PegIn);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::RegisterPegOutActor(addr) => {
                info!("Registering PegOutActor with bridge coordinator");
                self.child_actors.pegout_actor = Some(addr);
                self.metrics.record_actor_registration(ActorType::PegOut);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::RegisterStreamActor(addr) => {
                info!("Registering StreamActor with bridge coordinator");
                self.child_actors.stream_actor = Some(addr);
                self.metrics.record_actor_registration(ActorType::Stream);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::CoordinatePegIn { pegin_id, bitcoin_txid } => {
                let pegin_id = pegin_id;
                let bitcoin_txid = bitcoin_txid;
                Box::pin(async move {
                    self.start_pegin_operation(pegin_id, bitcoin_txid).await
                }.into_actor(self))
            }

            BridgeCoordinationMessage::CoordinatePegOut { pegout_id, burn_tx_hash } => {
                let pegout_id = pegout_id;
                let burn_tx_hash = burn_tx_hash;
                Box::pin(async move {
                    self.start_pegout_operation(pegout_id, burn_tx_hash).await
                }.into_actor(self))
            }

            BridgeCoordinationMessage::HandleActorFailure { actor_type, error } => {
                let actor_type = actor_type;
                let error = error;
                Box::pin(async move {
                    self.handle_actor_failure(actor_type, error).await;
                    Ok(())
                }.into_actor(self))
            }

            BridgeCoordinationMessage::GetSystemStatus => {
                let status = self.get_system_status();
                info!("System status requested: {:?}", status.status);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::GetSystemMetrics => {
                let metrics = self.metrics.get_current_metrics();
                info!("System metrics requested");
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::ShutdownSystem => {
                warn!("System shutdown requested");
                Box::pin(async move {
                    // Graceful shutdown logic would be implemented here
                    info!("Bridge system shutting down gracefully");
                    Ok(())
                }.into_actor(self))
            }
        }
    }
}

/// Handler for system status requests
impl Handler<GetSystemStatusResponse> for BridgeActor {
    type Result = BridgeSystemStatus;

    fn handle(&mut self, _msg: GetSystemStatusResponse, _ctx: &mut Context<Self>) -> Self::Result {
        self.get_system_status()
    }
}

/// Handler for operation status updates from child actors
#[derive(Message)]
#[rtype(result = "()")]
pub struct OperationStatusUpdate {
    pub operation_id: String,
    pub new_status: OperationState,
    pub metadata: Option<OperationMetadata>,
}

impl Handler<OperationStatusUpdate> for BridgeActor {
    type Result = ();

    fn handle(&mut self, msg: OperationStatusUpdate, _ctx: &mut Context<Self>) {
        info!("Received operation status update for {}: {:?}", msg.operation_id, msg.new_status);
        
        // Update operation status
        self.update_operation_status(msg.operation_id.clone(), msg.new_status.clone());
        
        // Update metadata if provided
        if let Some(new_metadata) = msg.metadata {
            if let Some(operation) = self.active_operations.get_mut(&msg.operation_id) {
                // Merge metadata
                if let Some(btc_txid) = new_metadata.bitcoin_txid {
                    operation.metadata.bitcoin_txid = Some(btc_txid);
                }
                if let Some(alys_tx) = new_metadata.alys_tx_hash {
                    operation.metadata.alys_tx_hash = Some(alys_tx);
                }
                if let Some(amount) = new_metadata.amount {
                    operation.metadata.amount = Some(amount);
                }
                if let Some(requester) = new_metadata.requester {
                    operation.metadata.requester = Some(requester);
                }
                if let Some(destination) = new_metadata.destination {
                    operation.metadata.destination = Some(destination);
                }
            }
        }

        // Handle operation completion
        match msg.new_status {
            OperationState::Completed => {
                info!("Operation {} completed successfully", msg.operation_id);
                self.metrics.record_successful_operation();
            }
            OperationState::Failed { ref reason } => {
                error!("Operation {} failed: {}", msg.operation_id, reason);
                self.metrics.record_failed_operation();
            }
            _ => {}
        }
    }
}

/// Handler for health check requests
#[derive(Message)]
#[rtype(result = "ActorHealthStatus")]
pub struct HealthCheckRequest;

#[derive(Debug, Clone)]
pub struct ActorHealthStatus {
    pub status: ActorStatus,
    pub uptime: std::time::Duration,
    pub active_operations: u32,
    pub total_operations: u64,
    pub error_rate: f64,
    pub last_error: Option<String>,
}

impl Handler<HealthCheckRequest> for BridgeActor {
    type Result = ActorHealthStatus;

    fn handle(&mut self, _msg: HealthCheckRequest, _ctx: &mut Context<Self>) -> Self::Result {
        let uptime = std::time::SystemTime::now()
            .duration_since(self.started_at)
            .unwrap_or_default();

        let status = if self.child_actors.pegin_actor.is_some() 
            && self.child_actors.pegout_actor.is_some() 
            && self.child_actors.stream_actor.is_some() {
            ActorStatus::Running
        } else {
            ActorStatus::Degraded
        };

        ActorHealthStatus {
            status,
            uptime,
            active_operations: self.active_operations.len() as u32,
            total_operations: self.metrics.get_total_operations(),
            error_rate: self.metrics.get_error_rate(),
            last_error: self.health_monitor.get_last_error(),
        }
    }
}

/// Handler for metrics collection requests
#[derive(Message)]
#[rtype(result = "BridgeCoordinationMetrics")]
pub struct MetricsRequest;

impl Handler<MetricsRequest> for BridgeActor {
    type Result = BridgeCoordinationMetrics;

    fn handle(&mut self, _msg: MetricsRequest, _ctx: &mut Context<Self>) -> Self::Result {
        self.metrics.clone()
    }
}

/// Handler for operation retry requests
#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct RetryOperationRequest {
    pub operation_id: String,
}

impl Handler<RetryOperationRequest> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<(), BridgeError>>;

    fn handle(&mut self, msg: RetryOperationRequest, _ctx: &mut Context<Self>) -> Self::Result {
        let operation_id = msg.operation_id;
        
        Box::pin(async move {
            if let Some(operation) = self.active_operations.get_mut(&operation_id) {
                if operation.retry_count >= 3 {
                    return Err(BridgeError::MaxRetriesExceeded(operation_id));
                }

                operation.retry_count += 1;
                operation.last_updated = std::time::SystemTime::now();
                
                info!("Retrying operation {} (attempt {})", operation_id, operation.retry_count);

                // Retry based on operation type
                match operation.operation_type {
                    OperationType::PegIn => {
                        if let Some(bitcoin_txid) = operation.metadata.bitcoin_txid {
                            self.start_pegin_operation(operation_id.clone(), bitcoin_txid).await?;
                        }
                    }
                    OperationType::PegOut => {
                        if let Some(burn_tx_hash) = operation.metadata.alys_tx_hash {
                            self.start_pegout_operation(operation_id.clone(), burn_tx_hash).await?;
                        }
                    }
                }

                Ok(())
            } else {
                Err(BridgeError::OperationNotFound(operation_id))
            }
        }.into_actor(self))
    }
}