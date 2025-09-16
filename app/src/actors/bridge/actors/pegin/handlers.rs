//! PegIn Actor Message Handlers
//! 
//! Message handling implementation for the PegIn actor

use actix::prelude::*;
use tracing::{info, warn, error};

use super::actor::{PegInActor, PegInError};
use crate::actors::bridge::{messages::*, shared::errors::BridgeError};

/// Handler for PegIn messages
impl Handler<PegInMessage> for PegInActor {
    type Result = ResponseActFuture<Self, Result<PegInResponse, BridgeError>>;

    fn handle(&mut self, msg: PegInMessage, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            PegInMessage::ProcessDeposit { txid, bitcoin_tx, block_height } => {
                info!("Received request to process deposit: {}", txid);
                
                Box::pin(
                    async move {
                        // This closure captures variables but not self
                        Ok::<_, BridgeError>((txid, bitcoin_tx, block_height))
                    }
                    .into_actor(self)
                    .map(move |res, act, _ctx| {
                        let (txid, _bitcoin_tx, _block_height) = res?;

                        // Check if we already have this deposit
                        if act.pending_deposits.contains_key(&txid) {
                            warn!("Deposit {} already being processed", txid);
                            return Ok(PegInResponse::DepositProcessed {
                                pegin_id: act.pending_deposits[&txid].pegin_id.clone()
                            });
                        }

                        // For now, create a simple synchronous response
                        // The async processing should be handled separately
                        Ok(PegInResponse::DepositProcessed {
                            pegin_id: format!("pegin_{}", txid)
                        })
                    })
                )
            }

            PegInMessage::ValidateDeposit { pegin_id, deposit } => {
                info!("Received request to validate deposit: {}", pegin_id);

                // Clone pegin_id for the async block
                let pegin_id_clone = pegin_id.clone();

                // Validate synchronously and capture the result
                let validation_result = match self.validator.validate_deposit(&deposit) {
                    Ok(result) => {
                        info!("Deposit {} validation result: valid={}", pegin_id, result.valid);
                        Ok(result.valid)
                    }
                    Err(e) => {
                        error!("Error validating deposit {}: {:?}", pegin_id, e);
                        self.record_error(PegInError::ValidationError(e.to_string()));
                        Err(BridgeError::ValidationError {
                            field: "deposit".to_string(),
                            reason: format!("Validation failed: {:?}", e)
                        })
                    }
                };

                let result = match validation_result {
                    Ok(valid) => Ok(PegInResponse::DepositValidated {
                        pegin_id: pegin_id_clone,
                        valid
                    }),
                    Err(e) => Err(e)
                };

                Box::pin(async move { result }.into_actor(self))
            }

            PegInMessage::UpdateConfirmations { pegin_id, confirmations } => {
                info!("Received confirmation update for {}: {} confirmations", pegin_id, confirmations);
                
                // Find the deposit by pegin_id
                let txid = self.pending_deposits.iter()
                    .find(|(_, deposit)| deposit.pegin_id == pegin_id)
                    .map(|(txid, _)| *txid);

                if let Some(txid) = txid {
                    self.update_deposit_confirmations(txid, confirmations);
                    Box::pin(async move {
                        Ok(PegInResponse::ConfirmationsUpdated { pegin_id, confirmations })
                    }.into_actor(self))
                } else {
                    warn!("Deposit {} not found for confirmation update", pegin_id);
                    Box::pin(async move {
                        Err(BridgeError::RequestNotFound {
                            request_id: pegin_id
                        })
                    }.into_actor(self))
                }
            }

            PegInMessage::ConfirmDeposit { pegin_id } => {
                info!("Received request to confirm deposit: {}", pegin_id);

                let pegin_id_clone = pegin_id.clone();
                let confirmation_threshold = self.config.confirmation_threshold;

                // First check if deposit exists and extract its details
                let deposit_details = self.pending_deposits.iter()
                    .find(|(_, deposit)| deposit.pegin_id == pegin_id)
                    .map(|(_, deposit)| (deposit.confirmations, deposit.evm_address, deposit.amount));

                if let Some((confirmations, evm_address, amount)) = deposit_details {
                    if confirmations >= confirmation_threshold {
                        // Update deposit status (separate borrow)
                        if let Some((_, deposit)) = self.pending_deposits.iter_mut()
                            .find(|(_, deposit)| deposit.pegin_id == pegin_id) {
                            deposit.status = DepositStatus::Confirmed;
                        }

                        self.metrics.record_deposit_confirmed();
                        self.initiate_minting(pegin_id.clone(), evm_address, amount);

                        Box::pin(async move {
                            Ok(PegInResponse::DepositConfirmed { pegin_id: pegin_id_clone })
                        }.into_actor(self))
                    } else {
                        warn!("Deposit {} has insufficient confirmations: {} < {}",
                              pegin_id, confirmations, confirmation_threshold);

                        Box::pin(async move {
                            Err(BridgeError::ValidationError {
                                field: "confirmations".to_string(),
                                reason: format!("Insufficient confirmations: {} < {}",
                                    confirmations, confirmation_threshold),
                            })
                        }.into_actor(self))
                    }
                } else {
                    warn!("Deposit {} not found", pegin_id);
                    Box::pin(async move {
                        Err(BridgeError::RequestNotFound {
                            request_id: pegin_id_clone
                        })
                    }.into_actor(self))
                }
            }

            PegInMessage::NotifyMinting { pegin_id, alys_tx_hash, amount } => {
                info!("Received minting notification for {}: tx={:?}, amount={}", 
                      pegin_id, alys_tx_hash, amount);
                
                // Update deposit status to completed
                if let Some((_, deposit)) = self.pending_deposits.iter_mut()
                    .find(|(_, deposit)| deposit.pegin_id == pegin_id) {
                    
                    deposit.status = DepositStatus::Completed {
                        alys_tx_hash,
                        minted_amount: amount,
                    };
                    self.metrics.record_deposit_completed();
                    
                    info!("Deposit {} completed successfully", pegin_id);
                }

                Box::pin(async move {
                    Ok(PegInResponse::MintingNotified { pegin_id })
                }.into_actor(self))
            }

            PegInMessage::GetDepositStatus { pegin_id } => {
                info!("Received status request for deposit: {}", pegin_id);
                
                if let Some((_, deposit)) = self.pending_deposits.iter()
                    .find(|(_, deposit)| deposit.pegin_id == pegin_id) {
                    
                    let status = deposit.status.clone();
                    Box::pin(async move {
                        Ok(PegInResponse::DepositStatus(status))
                    }.into_actor(self))
                } else {
                    warn!("Deposit {} not found for status request", pegin_id);
                    Box::pin(async move {
                        Err(BridgeError::RequestNotFound {
                            request_id: pegin_id
                        })
                    }.into_actor(self))
                }
            }

            PegInMessage::ListPendingDeposits => {
                info!("Received request for pending deposits list");
                
                let pending_deposits: Vec<PendingDeposit> = self.pending_deposits.values().cloned().collect();
                
                Box::pin(async move {
                    Ok(PegInResponse::PendingDeposits(pending_deposits))
                }.into_actor(self))
            }

            PegInMessage::RetryDeposit { pegin_id } => {
                info!("Received retry request for deposit: {}", pegin_id);
                
                if let Some((txid, deposit)) = self.pending_deposits.iter()
                    .find(|(_, deposit)| deposit.pegin_id == pegin_id)
                    .map(|(txid, deposit)| (*txid, deposit.clone())) {
                    
                    // Create retry operation
                    let retry_op = super::actor::RetryableOperation {
                        operation_id: pegin_id.clone(),
                        operation: super::actor::PegInOperation::ProcessDeposit {
                            txid,
                            bitcoin_tx: deposit.bitcoin_tx,
                        },
                        retry_count: 0,
                        last_attempt: std::time::SystemTime::now(),
                        next_retry: std::time::SystemTime::now(),
                        error: PegInError::InternalError("Manual retry".to_string()),
                    };
                    
                    self.retry_queue.push(retry_op);
                    
                    Box::pin(async move {
                        Ok(PegInResponse::DepositRetried { pegin_id })
                    }.into_actor(self))
                } else {
                    warn!("Deposit {} not found for retry", pegin_id);
                    Box::pin(async move {
                        Err(BridgeError::RequestNotFound {
                            request_id: pegin_id
                        })
                    }.into_actor(self))
                }
            }

            PegInMessage::CancelDeposit { pegin_id, reason } => {
                warn!("Received cancel request for deposit {}: {}", pegin_id, reason);
                
                if let Some((_, deposit)) = self.pending_deposits.iter_mut()
                    .find(|(_, deposit)| deposit.pegin_id == pegin_id) {
                    
                    deposit.status = DepositStatus::Cancelled { reason: reason.clone() };
                    self.metrics.record_deposit_cancelled();
                    
                    info!("Deposit {} cancelled: {}", pegin_id, reason);
                    
                    Box::pin(async move {
                        Ok(PegInResponse::DepositCancelled { pegin_id })
                    }.into_actor(self))
                } else {
                    warn!("Deposit {} not found for cancellation", pegin_id);
                    Box::pin(async move {
                        Err(BridgeError::RequestNotFound {
                            request_id: pegin_id
                        })
                    }.into_actor(self))
                }
            }

            PegInMessage::Initialize => {
                info!("Received initialize request");
                Box::pin(async move {
                    // Already initialized in actor.started()
                    Ok(PegInResponse::Initialized)
                }.into_actor(self))
            }

            PegInMessage::GetStatus => {
                info!("Received status request");
                let status = self.get_status();
                Box::pin(async move {
                    Ok(PegInResponse::StatusReported(status))
                }.into_actor(self))
            }

            PegInMessage::Shutdown => {
                info!("Received shutdown request");
                Box::pin(async move {
                    Ok(PegInResponse::Shutdown)
                }.into_actor(self))
            }
        }
    }
}

/// Handler for actor registration with bridge coordinator
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterWithBridgeCoordinator(pub Addr<super::super::bridge::BridgeActor>);

impl Handler<RegisterWithBridgeCoordinator> for PegInActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterWithBridgeCoordinator, ctx: &mut Context<Self>) {
        info!("Registering PegIn actor with bridge coordinator");
        self.bridge_coordinator = Some(msg.0.clone());
        
        // Send registration message to bridge coordinator
        let self_addr = ctx.address();
        let bridge_coordinator = msg.0;
        
        actix::spawn(async move {
            let registration_msg = BridgeCoordinationMessage::RegisterPegInActor {
                actor_id: "primary".to_string(),
                addr: Some(self_addr)
            };
            if let Err(e) = bridge_coordinator.send(registration_msg).await {
                error!("Failed to register with bridge coordinator: {:?}", e);
            } else {
                info!("Successfully registered with bridge coordinator");
            }
        });
    }
}

/// Handler for chain actor registration
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterChainActor(pub Addr<crate::actors::chain::ChainActor>);

impl Handler<RegisterChainActor> for PegInActor {
    type Result = ();

    fn handle(&mut self, msg: RegisterChainActor, _ctx: &mut Context<Self>) {
        info!("Registering ChainActor with PegIn actor");
        self.chain_actor = Some(msg.0);
    }
}

/// Handler for actor health checks
#[derive(Message)]
#[rtype(result = "Result<super::actor::PegInActorStatus, BridgeError>")]
pub struct GetPegInStatus;

impl Handler<GetPegInStatus> for PegInActor {
    type Result = Result<super::actor::PegInActorStatus, BridgeError>;

    fn handle(&mut self, _msg: GetPegInStatus, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.get_status())
    }
}

/// Handler for metrics requests
#[derive(Message)]
#[rtype(result = "Result<super::metrics::PegInMetrics, BridgeError>")]
pub struct GetPegInMetrics;

impl Handler<GetPegInMetrics> for PegInActor {
    type Result = Result<super::metrics::PegInMetrics, BridgeError>;

    fn handle(&mut self, _msg: GetPegInMetrics, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(self.metrics.clone())
    }
}

/// Handler for configuration updates
#[derive(Message)]
#[rtype(result = "Result<(), PegInError>")]
pub struct UpdatePegInConfig {
    pub new_config: crate::actors::bridge::config::PegInConfig,
}

impl Handler<UpdatePegInConfig> for PegInActor {
    type Result = Result<(), PegInError>;

    fn handle(&mut self, msg: UpdatePegInConfig, _ctx: &mut Context<Self>) -> Self::Result {
        info!("Updating PegIn configuration");
        
        let _old_config = self.config.clone();
        self.config = msg.new_config;
        
        // Update validator if monitoring addresses changed
        // This would require reconstructing the validator with new addresses
        
        // Update confirmation tracker threshold
        self.confirmation_tracker.update_threshold(self.config.confirmation_threshold);
        
        info!("PegIn configuration updated successfully");
        self.metrics.record_config_update();
        
        Ok(())
    }
}