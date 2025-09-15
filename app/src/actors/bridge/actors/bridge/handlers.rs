//! Bridge Actor Message Handlers
//! 
//! Message handling implementation for the bridge coordinator

use actix::prelude::*;
use tracing::{info, warn, error};

use super::actor::BridgeActor;
use super::metrics::BridgeCoordinationMetrics;
use crate::actors::bridge::messages::{
    BridgeCoordinationMessage, GetSystemStatusResponse, BridgeSystemStatus,
    OperationState, OperationType, ActorStatus, ActorType
};
use crate::actors::bridge::actors::bridge::actor::OperationMetadata;
use crate::types::errors::BridgeError as TypesBridgeError;

/// Handler for bridge coordination messages
impl Handler<BridgeCoordinationMessage> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<(), TypesBridgeError>>;

    fn handle(&mut self, msg: BridgeCoordinationMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            BridgeCoordinationMessage::InitializeSystem => {
                Box::pin(async move {
                    info!("Received system initialization request");
                    Ok(())
                }.into_actor(self))
            }

            BridgeCoordinationMessage::RegisterPegInActor { actor_id, addr } => {
                info!("Registering PegInActor '{}' with bridge coordinator", actor_id);

                if let Some(addr) = addr {
                    // Register in the new registry
                    self.actor_registry.register_pegin(actor_id.clone(), addr.clone());

                    // Maintain backward compatibility - set as primary if it's the first/primary
                    if actor_id == "primary" || self.child_actors.pegin_actor.is_none() {
                        self.child_actors.pegin_actor = Some(addr);
                    }
                } else {
                    // If no address provided, this might be from deserialization
                    warn!("Received RegisterPegInActor message without actor address for ID: {}", actor_id);
                }

                self.metrics.record_actor_registration(ActorType::PegIn);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::RegisterPegOutActor { actor_id, addr } => {
                info!("Registering PegOutActor '{}' with bridge coordinator", actor_id);

                if let Some(addr) = addr {
                    // Register in the new registry
                    self.actor_registry.register_pegout(actor_id.clone(), addr.clone());

                    // Maintain backward compatibility - set as primary if it's the first/primary
                    if actor_id == "primary" || self.child_actors.pegout_actor.is_none() {
                        self.child_actors.pegout_actor = Some(addr);
                    }
                } else {
                    // If no address provided, this might be from deserialization
                    warn!("Received RegisterPegOutActor message without actor address for ID: {}", actor_id);
                }

                self.metrics.record_actor_registration(ActorType::PegOut);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::RegisterStreamActor { actor_id, addr } => {
                info!("Registering StreamActor '{}' with bridge coordinator", actor_id);

                if let Some(addr) = addr {
                    // Register in the new registry
                    self.actor_registry.register_stream(actor_id.clone(), addr.clone());

                    // Maintain backward compatibility - set as primary if it's the first/primary
                    if actor_id == "primary" || self.child_actors.stream_actor.is_none() {
                        self.child_actors.stream_actor = Some(addr);
                    }
                } else {
                    // If no address provided, this might be from deserialization
                    warn!("Received RegisterStreamActor message without actor address for ID: {}", actor_id);
                }

                self.metrics.record_actor_registration(ActorType::Stream);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::CoordinatePegIn { pegin_id, bitcoin_txid } => {
                // Process immediately without async block to avoid borrowing issues
                match self.child_actors.pegin_actor {
                    Some(ref pegin_actor) => {
                        // Create operation context
                        let operation = super::actor::OperationContext {
                            operation_id: pegin_id.clone(),
                            operation_type: OperationType::PegIn,
                            status: OperationState::Initiated,
                            created_at: std::time::SystemTime::now(),
                            last_updated: std::time::SystemTime::now(),
                            assigned_actor: Some("pegin_actor".to_string()),
                            retry_count: 0,
                            metadata: super::actor::OperationMetadata {
                                bitcoin_txid: Some(bitcoin_txid),
                                ..Default::default()
                            },
                        };

                        // Store operation
                        self.active_operations.insert(pegin_id.clone(), operation);
                        self.metrics.record_operation_started(OperationType::PegIn);
                        info!("Peg-in operation {} initiated", pegin_id);
                        
                        Box::pin(async { Ok(()) }.into_actor(self))
                    }
                    None => {
                        error!("PegInActor not registered for operation {}", pegin_id);
                        Box::pin(async { 
                            Err(TypesBridgeError::ActorCommunication { 
                                actor: "PegInActor".to_string(), 
                                reason: "Actor not available".to_string() 
                            })
                        }.into_actor(self))
                    }
                }
            }

            BridgeCoordinationMessage::CoordinatePegOut { pegout_id, burn_tx_hash } => {
                // Process immediately without async block to avoid borrowing issues
                match self.child_actors.pegout_actor {
                    Some(ref pegout_actor) => {
                        // Create operation context
                        let operation = super::actor::OperationContext {
                            operation_id: pegout_id.clone(),
                            operation_type: OperationType::PegOut,
                            status: OperationState::Initiated,
                            created_at: std::time::SystemTime::now(),
                            last_updated: std::time::SystemTime::now(),
                            assigned_actor: Some("pegout_actor".to_string()),
                            retry_count: 0,
                            metadata: super::actor::OperationMetadata {
                                alys_tx_hash: Some(burn_tx_hash),
                                ..Default::default()
                            },
                        };

                        // Store operation
                        self.active_operations.insert(pegout_id.clone(), operation);
                        self.metrics.record_operation_started(OperationType::PegOut);
                        info!("Peg-out operation {} initiated", pegout_id);
                        
                        Box::pin(async { Ok(()) }.into_actor(self))
                    }
                    None => {
                        error!("PegOutActor not registered for operation {}", pegout_id);
                        Box::pin(async { 
                            Err(TypesBridgeError::ActorCommunication { 
                                actor: "PegOutActor".to_string(), 
                                reason: "Actor not available".to_string() 
                            })
                        }.into_actor(self))
                    }
                }
            }

            BridgeCoordinationMessage::HandleActorFailure { actor_type, error } => {
                // Process immediately without async block
                error!("Actor failure detected: {:?} - {:?}", actor_type, error);
                self.metrics.record_actor_failure(&actor_type);
                self.health_monitor.record_actor_failure(actor_type);
                
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::GetSystemStatus => {
                let status = self.get_system_status();
                info!("System status requested: {:?}", status.status);
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::GetSystemMetrics => {
                let _metrics = self.metrics.get_current_metrics();
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

            BridgeCoordinationMessage::PegInCompleted { pegin_id, bitcoin_txid, recipient, amount } => {
                info!("PegIn completed - ID: {}, Bitcoin TX: {}, Recipient: {:?}, Amount: {}", 
                      pegin_id, bitcoin_txid, recipient, amount);
                self.metrics.record_successful_operation();
                Box::pin(async { Ok(()) }.into_actor(self))
            }

            BridgeCoordinationMessage::PegOutCompleted { pegout_id, burn_tx_hash, bitcoin_destination, amount } => {
                info!("PegOut completed - ID: {}, Burn TX: {:?}, Bitcoin Destination: {}, Amount: {}", 
                      pegout_id, burn_tx_hash, bitcoin_destination, amount);
                self.metrics.record_successful_operation();
                Box::pin(async { Ok(()) }.into_actor(self))
            }
        }
    }
}

/// Handler for system status requests
impl Handler<GetSystemStatusResponse> for BridgeActor {
    type Result = Result<BridgeSystemStatus, TypesBridgeError>;

    fn handle(&mut self, _msg: GetSystemStatusResponse, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.get_system_status())
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
#[rtype(result = "Result<ActorHealthStatus, TypesBridgeError>")]
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
    type Result = Result<ActorHealthStatus, TypesBridgeError>;

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

        Ok(ActorHealthStatus {
            status,
            uptime,
            active_operations: self.active_operations.len() as u32,
            total_operations: self.metrics.get_total_operations(),
            error_rate: self.metrics.get_error_rate(),
            last_error: self.health_monitor.get_last_error(),
        })
    }
}

/// Handler for metrics collection requests
#[derive(Message)]
#[rtype(result = "Result<BridgeCoordinationMetrics, TypesBridgeError>")]
pub struct MetricsRequest;

impl Handler<MetricsRequest> for BridgeActor {
    type Result = Result<BridgeCoordinationMetrics, TypesBridgeError>;

    fn handle(&mut self, _msg: MetricsRequest, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.metrics.clone())
    }
}

/// Handler for operation retry requests
#[derive(Message)]
#[rtype(result = "Result<(), TypesBridgeError>")]
pub struct RetryOperationRequest {
    pub operation_id: String,
}

impl Handler<RetryOperationRequest> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<(), TypesBridgeError>>;

    fn handle(&mut self, msg: RetryOperationRequest, _ctx: &mut Context<Self>) -> Self::Result {
        let operation_id = msg.operation_id;
        
        // Check if operation exists and handle retry immediately
        if let Some(operation) = self.active_operations.get_mut(&operation_id) {
            if operation.retry_count >= 3 {
                return Box::pin(async move {
                    Err(TypesBridgeError::MaxRetriesExceeded(operation_id))
                }.into_actor(self));
            }
            
            // Update retry count
            operation.retry_count += 1;
            operation.last_updated = std::time::SystemTime::now();
            
            info!("Retrying operation {} (attempt {})", operation_id, operation.retry_count);
            
            // Reset operation status to initiated for retry
            operation.status = OperationState::Initiated;
            
            Box::pin(async { Ok(()) }.into_actor(self))
        } else {
            Box::pin(async move {
                Err(TypesBridgeError::OperationNotFound(operation_id))
            }.into_actor(self))
        }
    }
}