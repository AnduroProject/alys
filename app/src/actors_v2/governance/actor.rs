//! GovernanceClientActor implementation.

use actix::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::client::GovernanceClient;
use super::config::GovernanceConfig;
use super::messages::{
    GovernanceError, GovernanceMessage, GovernanceResponse, GovernanceUpdateReceived, VerifyPegin,
};
use crate::actors_v2::chain::tendermint::{
    EmergencyAction, EmergencyActionKind, GovernanceUpdate, GovernableParam, ParameterUpdate,
    ParameterValue, ValidatorUpdate,
};
use crate::actors_v2::chain::ChainActor;
use governance_proto::governance_response;
use lighthouse_wrapper::bls::{PublicKey, Signature};

/// Actor for managing connection to governance service.
///
/// Handles:
/// - Connection lifecycle with automatic reconnection
/// - Peg-in verification requests (blocking)
/// - Forwarding governance updates to ChainActor
/// - Heartbeat keep-alive
pub struct GovernanceClientActor {
    config: GovernanceConfig,
    client: Arc<RwLock<GovernanceClient>>,
    chain_actor: Option<Addr<ChainActor>>,
}

impl GovernanceClientActor {
    /// Create a new governance client actor.
    pub fn new(config: GovernanceConfig) -> Self {
        let client = GovernanceClient::new(config.clone());
        Self {
            config,
            client: Arc::new(RwLock::new(client)),
            chain_actor: None,
        }
    }

    /// Start the governance update processing loop.
    fn start_update_processor(
        &self,
        ctx: &mut Context<Self>,
        mut update_rx: mpsc::Receiver<governance_response::Payload>,
    ) {
        let chain_actor = self.chain_actor.clone();

        ctx.spawn(
            async move {
                while let Some(payload) = update_rx.recv().await {
                    let update = match payload {
                        governance_response::Payload::ValidatorUpdate(validator_update) => {
                            // Convert proto to domain type
                            let public_key =
                                match PublicKey::deserialize(&validator_update.public_key) {
                                    Ok(pk) => pk,
                                    Err(e) => {
                                        error!(error = ?e, "Invalid public key in validator update");
                                        continue;
                                    }
                                };

                            let signature = match Signature::deserialize(
                                &validator_update.governance_signature,
                            ) {
                                Ok(sig) => sig,
                                Err(e) => {
                                    error!(error = ?e, "Invalid signature in validator update");
                                    continue;
                                }
                            };

                            GovernanceUpdate::Validator(ValidatorUpdate {
                                public_key,
                                power: validator_update.power,
                                governance_signature: signature,
                            })
                        }
                        governance_response::Payload::ParameterUpdate(param_update) => {
                            let signature =
                                match Signature::deserialize(&param_update.governance_signature) {
                                    Ok(sig) => sig,
                                    Err(e) => {
                                        error!(error = ?e, "Invalid signature in parameter update");
                                        continue;
                                    }
                                };

                            // Convert param_id to GovernableParam
                            let param =
                                match GovernableParam::try_from_u16(param_update.param_id as u16) {
                                    Some(p) => p,
                                    None => {
                                        error!(
                                            param_id = param_update.param_id,
                                            "Unknown parameter ID"
                                        );
                                        continue;
                                    }
                                };

                            GovernanceUpdate::Parameter(ParameterUpdate {
                                param,
                                value: ParameterValue::U64(param_update.value),
                                governance_signature: signature,
                            })
                        }
                        governance_response::Payload::Emergency(emergency_action) => {
                            let signature = match Signature::deserialize(
                                &emergency_action.governance_signature,
                            ) {
                                Ok(sig) => sig,
                                Err(e) => {
                                    error!(error = ?e, "Invalid signature in emergency action");
                                    continue;
                                }
                            };

                            let action = match emergency_action.action {
                                0 => EmergencyActionKind::PausePegIns,
                                1 => EmergencyActionKind::ResumePegIns,
                                2 => EmergencyActionKind::PausePegOuts,
                                3 => EmergencyActionKind::ResumePegOuts,
                                4 => EmergencyActionKind::PauseChain,
                                5 => EmergencyActionKind::ResumeChain,
                                _ => {
                                    error!(
                                        action = emergency_action.action,
                                        "Unknown emergency action"
                                    );
                                    continue;
                                }
                            };

                            GovernanceUpdate::Emergency(EmergencyAction {
                                action,
                                governance_signature: signature,
                            })
                        }
                        _ => continue, // HeartbeatAck and PeginVerify are handled elsewhere
                    };

                    // Forward to ChainActor
                    if let Some(ref chain_actor) = chain_actor {
                        let msg = GovernanceUpdateReceived {
                            update: update.clone(),
                            correlation_id: Uuid::new_v4(),
                        };

                        info!(
                            update_type = update.variant_name(),
                            "Forwarding governance update to ChainActor"
                        );

                        chain_actor.do_send(msg);
                    } else {
                        warn!("Received governance update but ChainActor not set");
                    }
                }

                info!("Governance update processor ended");
            }
            .into_actor(self),
        );
    }

    /// Schedule heartbeat timer.
    fn start_heartbeat_timer(&self, ctx: &mut Context<Self>) {
        let interval = self.config.heartbeat_interval;
        ctx.run_interval(interval, |_actor, ctx| {
            ctx.address().do_send(GovernanceMessage::SendHeartbeat);
        });
    }

    /// Schedule reconnection.
    fn schedule_reconnect(&self, ctx: &mut Context<Self>) {
        let delay = self.config.reconnect_interval;
        ctx.run_later(delay, |_, ctx| {
            ctx.address().do_send(GovernanceMessage::Reconnect);
        });
    }
}

impl Actor for GovernanceClientActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        info!(
            url = %self.config.grpc_url,
            "GovernanceClientActor started"
        );

        // Connect on startup
        ctx.address().do_send(GovernanceMessage::Connect);

        // Start heartbeat timer
        self.start_heartbeat_timer(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        info!("GovernanceClientActor stopped");
    }
}

impl Handler<GovernanceMessage> for GovernanceClientActor {
    type Result = ResponseActFuture<Self, Result<GovernanceResponse, GovernanceError>>;

    fn handle(&mut self, msg: GovernanceMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let client = self.client.clone();
        let config = self.config.clone();

        match msg {
            GovernanceMessage::Connect => {
                Box::pin(
                    async move {
                        let mut client_guard = client.write().await;
                        match client_guard.connect().await {
                            Ok(update_rx) => {
                                info!("Connected to governance service");
                                Ok((GovernanceResponse::Connected, Some(update_rx)))
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to connect to governance service");
                                Err(e)
                            }
                        }
                    }
                    .into_actor(self)
                    .map(|result, actor, ctx| {
                        match result {
                            Ok((response, Some(update_rx))) => {
                                // Start processing governance updates from the stream
                                actor.start_update_processor(ctx, update_rx);
                                Ok(response)
                            }
                            Ok((response, None)) => Ok(response),
                            Err(e) => {
                                // Schedule reconnection on failure
                                actor.schedule_reconnect(ctx);
                                Err(e)
                            }
                        }
                    }),
                )
            }

            GovernanceMessage::VerifyPegin(request) => {
                let txid = request.txid;
                let correlation_id = request.correlation_id;

                Box::pin(
                    async move {
                        let client_guard = client.read().await;

                        if !client_guard.is_connected().await {
                            return Err(GovernanceError::NotConnected);
                        }

                        info!(
                            txid = %txid,
                            correlation_id = %correlation_id,
                            "Verifying peg-in with governance"
                        );

                        match client_guard.verify_pegin(&request).await {
                            Ok(result) => {
                                info!(
                                    txid = %txid,
                                    verified = result.verified,
                                    reason = %result.reason,
                                    "Peg-in verification result"
                                );
                                Ok(GovernanceResponse::PeginVerified(result))
                            }
                            Err(e) => {
                                error!(
                                    txid = %txid,
                                    error = %e,
                                    "Peg-in verification failed"
                                );
                                Err(e)
                            }
                        }
                    }
                    .into_actor(self),
                )
            }

            GovernanceMessage::SetChainActor { addr } => {
                Box::pin(
                    async move { Ok::<_, GovernanceError>(addr) }
                        .into_actor(self)
                        .map(|result, actor, _ctx| {
                            if let Ok(addr) = result {
                                actor.chain_actor = Some(addr);
                                info!("ChainActor set in GovernanceClientActor");
                                Ok(GovernanceResponse::ChainActorSet)
                            } else {
                                Err(GovernanceError::ChainActorNotSet)
                            }
                        }),
                )
            }

            GovernanceMessage::SendHeartbeat => {
                Box::pin(
                    async move {
                        let client_guard = client.read().await;
                        match client_guard.send_heartbeat().await {
                            Ok(()) => {
                                debug!("Heartbeat sent");
                                Ok(GovernanceResponse::HeartbeatSent)
                            }
                            Err(e) => {
                                // Don't log warning for expected disconnection
                                if matches!(e, GovernanceError::NotConnected) {
                                    debug!("Heartbeat skipped - not connected");
                                } else {
                                    warn!(error = %e, "Failed to send heartbeat");
                                }
                                Err(e)
                            }
                        }
                    }
                    .into_actor(self),
                )
            }

            GovernanceMessage::Reconnect => {
                info!("Attempting to reconnect to governance service");
                Box::pin(
                    async move {
                        let mut client_guard = client.write().await;
                        match client_guard.connect().await {
                            Ok(update_rx) => {
                                info!("Reconnected to governance service");
                                Ok((GovernanceResponse::Connected, Some(update_rx)))
                            }
                            Err(e) => {
                                error!(error = %e, "Reconnection failed");
                                Err(e)
                            }
                        }
                    }
                    .into_actor(self)
                    .map(|result, actor, ctx| {
                        match result {
                            Ok((_, Some(update_rx))) => {
                                // Start processing governance updates from the stream
                                actor.start_update_processor(ctx, update_rx);
                                Ok(GovernanceResponse::ReconnectScheduled)
                            }
                            Ok((_, None)) => Ok(GovernanceResponse::ReconnectScheduled),
                            Err(e) => {
                                // Schedule another reconnection on failure
                                actor.schedule_reconnect(ctx);
                                Err(e)
                            }
                        }
                    }),
                )
            }
        }
    }
}
