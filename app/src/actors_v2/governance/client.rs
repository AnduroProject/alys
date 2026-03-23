//! gRPC client for governance service.

use super::config::GovernanceConfig;
use super::messages::{GovernanceError, PeginVerificationResult, VerifyPegin};
use bitcoin::hashes::Hash;
use futures::StreamExt;
use governance_proto::{
    governance_request, governance_response, GovernanceRequest, GovernanceServiceClient, Heartbeat,
    PeginVerifyRequest,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, RwLock};
use tonic::transport::Channel;
use tonic::Request;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Pending peg-in verification request awaiting response.
struct PendingVerification {
    correlation_id: Uuid,
    response_tx: oneshot::Sender<PeginVerificationResult>,
}

/// gRPC client wrapper for governance service.
pub struct GovernanceClient {
    pub(crate) config: GovernanceConfig,
    /// Sender for outgoing requests to the stream
    pub(crate) request_tx: Option<mpsc::Sender<GovernanceRequest>>,
    /// Pending verification requests keyed by request_id
    pub(crate) pending_verifications: Arc<RwLock<HashMap<String, PendingVerification>>>,
    /// Connection status
    pub(crate) connected: Arc<RwLock<bool>>,
}

impl GovernanceClient {
    /// Create a new governance client.
    pub fn new(config: GovernanceConfig) -> Self {
        Self {
            config,
            request_tx: None,
            pending_verifications: Arc::new(RwLock::new(HashMap::new())),
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Connect to the governance service.
    pub async fn connect(&mut self) -> Result<mpsc::Receiver<governance_response::Payload>, GovernanceError> {
        info!(url = %self.config.grpc_url, "Connecting to governance service");

        // Create gRPC channel
        let channel = Channel::from_shared(self.config.grpc_url.clone())
            .map_err(|e| GovernanceError::ConnectionFailed(e.to_string()))?
            .connect()
            .await
            .map_err(|e| GovernanceError::ConnectionFailed(e.to_string()))?;

        let mut client = GovernanceServiceClient::new(channel);

        // Create channel for outgoing requests
        let (request_tx, request_rx) = mpsc::channel::<GovernanceRequest>(100);

        // Create channel for governance updates to forward to ChainActor
        let (update_tx, update_rx) = mpsc::channel::<governance_response::Payload>(100);

        // Convert request receiver to stream
        let request_stream = tokio_stream::wrappers::ReceiverStream::new(request_rx);

        // Create request with auth metadata
        let mut request = Request::new(request_stream);
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", self.config.auth_token)
                .parse()
                .unwrap(),
        );

        // Start bidirectional stream
        let response = client
            .governance_stream(request)
            .await
            .map_err(|e| GovernanceError::ConnectionFailed(e.to_string()))?;

        let mut response_stream = response.into_inner();

        // Store request sender
        self.request_tx = Some(request_tx);
        *self.connected.write().await = true;

        // Spawn task to process incoming responses
        let pending_verifications = self.pending_verifications.clone();
        let connected = self.connected.clone();
        tokio::spawn(async move {
            while let Some(result) = response_stream.next().await {
                match result {
                    Ok(response) => {
                        let request_id = response.request_id.clone();

                        match response.payload {
                            Some(governance_response::Payload::PeginVerify(verify_response)) => {
                                // Find and complete the pending verification
                                let mut pending = pending_verifications.write().await;
                                if let Some(pending_req) = pending.remove(&request_id) {
                                    let result = PeginVerificationResult {
                                        verified: verify_response.verified,
                                        reason: verify_response.reason,
                                        confirmations: verify_response.confirmations,
                                        correlation_id: pending_req.correlation_id,
                                    };
                                    let _ = pending_req.response_tx.send(result);
                                } else {
                                    warn!(
                                        request_id = %request_id,
                                        "Received pegin verify response for unknown request"
                                    );
                                }
                            }
                            Some(governance_response::Payload::HeartbeatAck(ack)) => {
                                debug!(timestamp = ack.timestamp, "Received heartbeat ACK");
                            }
                            Some(payload @ governance_response::Payload::ValidatorUpdate(_))
                            | Some(payload @ governance_response::Payload::ParameterUpdate(_))
                            | Some(payload @ governance_response::Payload::Emergency(_)) => {
                                // Forward governance updates to the actor
                                if update_tx.send(payload).await.is_err() {
                                    warn!("Failed to forward governance update - receiver dropped");
                                    break;
                                }
                            }
                            None => {
                                debug!("Received response with no payload");
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Stream error from governance service");
                        break;
                    }
                }
            }

            // Mark as disconnected
            *connected.write().await = false;
            info!("Governance stream ended");
        });

        info!("Connected to governance service");
        Ok(update_rx)
    }

    /// Check if connected to governance service.
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Send a peg-in verification request and wait for response.
    pub async fn verify_pegin(
        &self,
        request: &VerifyPegin,
    ) -> Result<PeginVerificationResult, GovernanceError> {
        let request_tx = self
            .request_tx
            .as_ref()
            .ok_or(GovernanceError::NotConnected)?;

        let request_id = request.correlation_id.to_string();

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_verifications.write().await;
            pending.insert(
                request_id.clone(),
                PendingVerification {
                    correlation_id: request.correlation_id,
                    response_tx,
                },
            );
        }

        // Build proto request
        let proto_request = GovernanceRequest {
            chain: self.config.chain_id.clone(),
            request_id: request_id.clone(),
            payload: Some(governance_request::Payload::PeginVerify(PeginVerifyRequest {
                txid: request.txid.to_byte_array().to_vec(),
                block_hash: request.block_hash.to_byte_array().to_vec(),
                evm_account: request.evm_account.as_bytes().to_vec(),
                required_confirmations: request.required_confirmations,
                amount: request.amount,
            })),
        };

        // Send request
        request_tx
            .send(proto_request)
            .await
            .map_err(|_| GovernanceError::NotConnected)?;

        debug!(
            request_id = %request_id,
            txid = %request.txid,
            "Sent pegin verify request"
        );

        // Wait for response with timeout
        match tokio::time::timeout(self.config.verify_timeout, response_rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                // Channel closed - remove from pending
                self.pending_verifications.write().await.remove(&request_id);
                Err(GovernanceError::RequestFailed(
                    "Response channel closed".to_string(),
                ))
            }
            Err(_) => {
                // Timeout - remove from pending
                self.pending_verifications.write().await.remove(&request_id);
                Err(GovernanceError::Timeout)
            }
        }
    }

    /// Send a heartbeat to keep the connection alive.
    pub async fn send_heartbeat(&self) -> Result<(), GovernanceError> {
        let request_tx = self
            .request_tx
            .as_ref()
            .ok_or(GovernanceError::NotConnected)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let request = GovernanceRequest {
            chain: self.config.chain_id.clone(),
            request_id: Uuid::new_v4().to_string(),
            payload: Some(governance_request::Payload::Heartbeat(Heartbeat {
                timestamp,
            })),
        };

        request_tx
            .send(request)
            .await
            .map_err(|_| GovernanceError::NotConnected)?;

        debug!(timestamp = timestamp, "Sent heartbeat");
        Ok(())
    }

    /// Disconnect from governance service.
    pub async fn disconnect(&mut self) {
        self.request_tx = None;
        *self.connected.write().await = false;

        // Cancel all pending verifications
        let mut pending = self.pending_verifications.write().await;
        for (request_id, pending_req) in pending.drain() {
            warn!(request_id = %request_id, "Cancelling pending verification due to disconnect");
            let _ = pending_req.response_tx.send(PeginVerificationResult {
                verified: false,
                reason: "Disconnected from governance".to_string(),
                confirmations: 0,
                correlation_id: pending_req.correlation_id,
            });
        }

        info!("Disconnected from governance service");
    }
}
