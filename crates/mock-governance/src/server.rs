//! gRPC server implementation for mock governance.

use crate::chaos::ChaosController;
use crate::config::Config;
use futures::StreamExt;
use governance_proto::{
    governance_request, governance_response, GovernanceRequest, GovernanceResponse,
    GovernanceService, HeartbeatAck, PeginVerifyResponse, ValidatorSetUpdate,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

/// Mock governance service implementation.
pub struct MockGovernanceService {
    config: Arc<Config>,
    chaos: Arc<ChaosController>,
}

impl MockGovernanceService {
    /// Create a new mock governance service.
    pub fn new(config: Config) -> Self {
        let chaos = ChaosController::new(
            config.pegin_reject_rate,
            config.disconnect_after,
            config.chaos_mode,
        );

        Self {
            config: Arc::new(config),
            chaos: Arc::new(chaos),
        }
    }

    /// Validate authentication metadata.
    fn validate_auth(&self, request: &Request<Streaming<GovernanceRequest>>) -> Result<(), Status> {
        // Check for authorization header
        if let Some(auth) = request.metadata().get("authorization") {
            let auth_str = auth
                .to_str()
                .map_err(|_| Status::unauthenticated("Invalid authorization header"))?;

            // Expect "Bearer <token>" format
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                if token == self.config.auth_token {
                    return Ok(());
                }
            }
        }

        // Also check x-auth-token header (alternative)
        if let Some(token) = request.metadata().get("x-auth-token") {
            let token_str = token
                .to_str()
                .map_err(|_| Status::unauthenticated("Invalid token header"))?;

            if token_str == self.config.auth_token {
                return Ok(());
            }
        }

        Err(Status::unauthenticated("Missing or invalid authentication"))
    }

    /// Handle a peg-in verification request.
    async fn handle_pegin_verify(
        &self,
        request_id: &str,
        req: &governance_proto::PeginVerifyRequest,
    ) -> GovernanceResponse {
        let txid_hex = hex::encode(&req.txid);
        let evm_hex = hex::encode(&req.evm_account);

        info!(
            request_id = request_id,
            txid = %txid_hex,
            amount = req.amount,
            evm_account = %evm_hex,
            required_confirmations = req.required_confirmations,
            "Processing peg-in verification request"
        );

        // Apply response delay if configured
        if self.config.response_delay_ms > 0 {
            tokio::time::sleep(self.config.response_delay()).await;
        }

        // Check chaos rejection
        if self.chaos.should_reject_pegin() {
            warn!(
                request_id = request_id,
                txid = %txid_hex,
                "Chaos: Rejecting peg-in"
            );

            return GovernanceResponse {
                chain: self.config.chain_id.clone(),
                request_id: request_id.to_string(),
                payload: Some(governance_response::Payload::PeginVerify(
                    PeginVerifyResponse {
                        verified: false,
                        reason: "Chaos: Random rejection for testing".to_string(),
                        confirmations: 0,
                    },
                )),
            };
        }

        // Auto-ACK if enabled
        if self.config.auto_ack_pegins {
            info!(
                request_id = request_id,
                txid = %txid_hex,
                "Auto-ACK: Approving peg-in"
            );

            return GovernanceResponse {
                chain: self.config.chain_id.clone(),
                request_id: request_id.to_string(),
                payload: Some(governance_response::Payload::PeginVerify(
                    PeginVerifyResponse {
                        verified: true,
                        reason: String::new(),
                        confirmations: req.required_confirmations + 10, // Well-confirmed
                    },
                )),
            };
        }

        // Default: reject (real governance would verify against Bitcoin)
        GovernanceResponse {
            chain: self.config.chain_id.clone(),
            request_id: request_id.to_string(),
            payload: Some(governance_response::Payload::PeginVerify(
                PeginVerifyResponse {
                    verified: false,
                    reason: "Mock governance: auto_ack_pegins not enabled".to_string(),
                    confirmations: 0,
                },
            )),
        }
    }

    /// Handle a heartbeat request.
    fn handle_heartbeat(&self, request_id: &str, timestamp: u64) -> GovernanceResponse {
        debug!(
            request_id = request_id,
            timestamp = timestamp,
            "Processing heartbeat"
        );

        GovernanceResponse {
            chain: self.config.chain_id.clone(),
            request_id: request_id.to_string(),
            payload: Some(governance_response::Payload::HeartbeatAck(HeartbeatAck {
                timestamp,
            })),
        }
    }

    /// Generate a mock validator set update for testing.
    fn generate_validator_update(&self) -> GovernanceResponse {
        // Generate a random public key for testing
        let mut pubkey = vec![0u8; 48];
        rand::Rng::fill(&mut rand::thread_rng(), &mut pubkey[..]);

        info!("Pushing mock validator set update");

        GovernanceResponse {
            chain: self.config.chain_id.clone(),
            request_id: String::new(), // Push notification - no request ID
            payload: Some(governance_response::Payload::ValidatorUpdate(
                ValidatorSetUpdate {
                    public_key: pubkey,
                    power: 100,
                    governance_signature: vec![0u8; 96], // Mock signature
                },
            )),
        }
    }
}

#[tonic::async_trait]
impl GovernanceService for MockGovernanceService {
    type GovernanceStreamStream =
        Pin<Box<dyn futures::Stream<Item = Result<GovernanceResponse, Status>> + Send>>;

    async fn governance_stream(
        &self,
        request: Request<Streaming<GovernanceRequest>>,
    ) -> Result<Response<Self::GovernanceStreamStream>, Status> {
        // Validate authentication
        self.validate_auth(&request)?;

        let peer_addr = request
            .remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        info!(peer = %peer_addr, "Validator connected to governance stream");

        // Reset chaos controller for new connection
        self.chaos.reset();

        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);
        let config = self.config.clone();
        let chaos = self.chaos.clone();

        // Clone self for the spawned tasks
        let service = Arc::new(Self {
            config: config.clone(),
            chaos: chaos.clone(),
        });

        // Spawn task to process incoming requests
        let service_clone = service.clone();
        let tx_clone = tx.clone();
        let peer_clone = peer_addr.clone();
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(request) => {
                        // Increment request counter
                        let count = service_clone.chaos.increment_requests();
                        debug!(request_count = count, "Processing request");

                        // Check if we should disconnect (chaos)
                        if service_clone.chaos.should_disconnect() {
                            warn!(
                                peer = %peer_clone,
                                request_count = count,
                                "Chaos: Disconnecting after {} requests",
                                count
                            );
                            break;
                        }

                        // Validate chain ID
                        if request.chain != service_clone.config.chain_id {
                            warn!(
                                expected = %service_clone.config.chain_id,
                                received = %request.chain,
                                "Chain ID mismatch"
                            );
                            continue;
                        }

                        // Process request based on payload type
                        let response = match request.payload {
                            Some(governance_request::Payload::PeginVerify(pegin_req)) => {
                                service_clone
                                    .handle_pegin_verify(&request.request_id, &pegin_req)
                                    .await
                            }
                            Some(governance_request::Payload::Heartbeat(heartbeat)) => {
                                service_clone
                                    .handle_heartbeat(&request.request_id, heartbeat.timestamp)
                            }
                            None => {
                                warn!(
                                    request_id = %request.request_id,
                                    "Received request with no payload"
                                );
                                continue;
                            }
                        };

                        // Send response
                        if tx_clone.send(Ok(response)).await.is_err() {
                            error!("Failed to send response - client disconnected");
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Error receiving request");
                        break;
                    }
                }
            }

            info!(peer = %peer_clone, "Validator stream ended");
        });

        // Spawn task to push periodic validator updates if configured
        if let Some(interval) = config.validator_update_interval() {
            let service_for_push = service.clone();
            let tx_for_push = tx.clone();
            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(interval);
                loop {
                    interval_timer.tick().await;

                    let update = service_for_push.generate_validator_update();
                    if tx_for_push.send(Ok(update)).await.is_err() {
                        // Client disconnected
                        break;
                    }
                }
            });
        }

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}
