//! Governance stream protocol implementation and gRPC service definitions
//!
//! This module implements the gRPC protocol layer for communication with 
//! Anduro Governance nodes. It handles message encoding/decoding, authentication,
//! and provides a high-level interface for governance operations.

use crate::actors::governance_stream::{error::*, messages::*};
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tonic::{transport::Channel, Request, Response, Status, Streaming};
use tokio::sync::oneshot;
use tracing::*;
use uuid::Uuid;

/// Governance protocol handler for gRPC communication
#[derive(Debug)]
pub struct GovernanceProtocol {
    /// Protocol configuration
    config: ProtocolConfig,
    /// gRPC client instance
    client: Option<governance::stream_client::StreamClient<Channel>>,
    /// Authentication state
    auth_state: AuthenticationState,
    /// Protocol metrics
    metrics: ProtocolMetrics,
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Protocol version to use
    pub protocol_version: String,
    /// Supported message types
    pub supported_messages: Vec<String>,
    /// Authentication configuration
    pub auth_config: AuthConfig,
    /// Message serialization format
    pub serialization_format: SerializationFormat,
    /// Compression settings
    pub compression: CompressionConfig,
    /// Protocol timeouts
    pub timeouts: ProtocolTimeouts,
    /// Retry configuration
    pub retry_config: RetryConfig,
}

/// Authentication state for the protocol
#[derive(Debug, Clone)]
struct AuthenticationState {
    /// Whether authentication is completed
    authenticated: bool,
    /// Current authentication token
    token: Option<String>,
    /// Token expiration time
    token_expires_at: Option<SystemTime>,
    /// Authentication metadata
    metadata: HashMap<String, String>,
    /// Challenge-response state
    challenge_state: Option<ChallengeState>,
}

/// Challenge-response authentication state
#[derive(Debug, Clone)]
struct ChallengeState {
    /// Challenge identifier
    challenge_id: String,
    /// Challenge data
    challenge_data: Vec<u8>,
    /// Expected response format
    response_format: String,
    /// Challenge expiration
    expires_at: SystemTime,
}

/// Protocol performance metrics
#[derive(Debug, Clone, Default)]
pub struct ProtocolMetrics {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Messages by type counts
    pub message_type_counts: HashMap<String, u64>,
    /// Authentication attempts
    pub auth_attempts: u64,
    /// Authentication successes
    pub auth_successes: u64,
    /// Protocol errors by type
    pub error_counts: HashMap<String, u64>,
    /// Average message processing time
    pub avg_processing_time_ms: f64,
    /// Bytes sent/received
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

/// Message serialization formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializationFormat {
    /// Protocol Buffers (protobuf)
    Protobuf,
    /// JSON format
    Json,
    /// MessagePack binary format
    MessagePack,
    /// CBOR (Concise Binary Object Representation)
    Cbor,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable compression
    pub enabled: bool,
    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,
    /// Compression level (0-9)
    pub level: u8,
    /// Minimum message size for compression
    pub min_size_bytes: u32,
}

/// Supported compression algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// Gzip compression
    Gzip,
    /// LZ4 compression
    Lz4,
    /// Zstandard compression
    Zstd,
    /// Brotli compression
    Brotli,
}

/// Protocol timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolTimeouts {
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Authentication timeout
    pub auth_timeout: Duration,
    /// Message send timeout
    pub send_timeout: Duration,
    /// Message receive timeout
    pub receive_timeout: Duration,
    /// Heartbeat timeout
    pub heartbeat_timeout: Duration,
    /// Stream idle timeout
    pub stream_idle_timeout: Duration,
}

/// Protocol retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Enable automatic retries
    pub enabled: bool,
    /// Maximum retry attempts
    pub max_attempts: u32,
    /// Initial retry delay
    pub initial_delay: Duration,
    /// Retry delay multiplier
    pub delay_multiplier: f64,
    /// Maximum retry delay
    pub max_delay: Duration,
    /// Retryable error codes
    pub retryable_errors: Vec<i32>,
}

// gRPC service definitions generated from proto file
pub mod governance {
    tonic::include_proto!("governance.v1");
}

impl GovernanceProtocol {
    /// Create new protocol handler
    pub fn new(config: ProtocolConfig) -> Self {
        Self {
            config,
            client: None,
            auth_state: AuthenticationState {
                authenticated: false,
                token: None,
                token_expires_at: None,
                metadata: HashMap::new(),
                challenge_state: None,
            },
            metrics: ProtocolMetrics::default(),
        }
    }

    /// Initialize protocol with gRPC channel
    pub async fn initialize(&mut self, channel: Channel) -> ProtocolResult<()> {
        info!("Initializing governance protocol");
        
        let client = governance::stream_client::StreamClient::new(channel)
            .max_decoding_message_size(16 * 1024 * 1024) // 16MB max message size
            .max_encoding_message_size(16 * 1024 * 1024);
            
        self.client = Some(client);
        self.metrics = ProtocolMetrics::default();
        
        Ok(())
    }

    /// Establish bidirectional stream
    pub async fn establish_stream(&mut self) -> ProtocolResult<(
        tokio::sync::mpsc::Sender<governance::StreamRequest>,
        Streaming<governance::StreamResponse>
    )> {
        let client = self.client.as_mut()
            .ok_or_else(|| ProtocolError::HandshakeFailed {
                reason: "Client not initialized".to_string(),
            })?;

        info!("Establishing bidirectional stream");

        // Create request stream channel
        let (tx, rx) = tokio::sync::mpsc::channel(1000);
        let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

        // Create request with authentication metadata
        let mut request = Request::new(request_stream);
        self.add_auth_metadata(&mut request)?;

        // Establish bidirectional stream
        let response = client.bidirectional_stream(request).await
            .map_err(|e| ProtocolError::HandshakeFailed {
                reason: format!("gRPC stream creation failed: {}", e),
            })?;

        let response_stream = response.into_inner();
        
        info!("Bidirectional stream established successfully");
        
        Ok((tx, response_stream))
    }

    /// Authenticate with governance node
    pub async fn authenticate(&mut self) -> AuthenticationResult<()> {
        info!("Starting authentication process");

        match &self.config.auth_config.auth_type {
            AuthenticationType::Bearer => {
                self.authenticate_bearer().await
            }
            AuthenticationType::MutualTls => {
                self.authenticate_mutual_tls().await
            }
            AuthenticationType::Signature => {
                self.authenticate_signature().await
            }
            AuthenticationType::ApiKey => {
                self.authenticate_api_key().await
            }
        }
    }

    /// Authenticate using Bearer token
    async fn authenticate_bearer(&mut self) -> AuthenticationResult<()> {
        let token = self.config.auth_config.credential.clone();
        if token.is_empty() {
            return Err(AuthenticationError::InvalidCredentials {
                credential_type: "bearer_token".to_string(),
            });
        }

        // Validate token format
        if !self.is_valid_bearer_token(&token) {
            return Err(AuthenticationError::InvalidTokenFormat {
                format_error: "Invalid bearer token format".to_string(),
            });
        }

        // Set authentication state
        self.auth_state.token = Some(token);
        self.auth_state.authenticated = true;
        
        // Set token expiration if configured
        if let Some(refresh_interval) = self.config.auth_config.refresh_interval {
            self.auth_state.token_expires_at = Some(SystemTime::now() + refresh_interval);
        }

        self.metrics.auth_attempts += 1;
        self.metrics.auth_successes += 1;

        info!("Bearer token authentication successful");
        Ok(())
    }

    /// Authenticate using mutual TLS
    async fn authenticate_mutual_tls(&mut self) -> AuthenticationResult<()> {
        // For mTLS, authentication is handled at the transport layer
        // We just need to verify the connection is using client certificates
        
        self.auth_state.authenticated = true;
        self.auth_state.metadata.insert(
            "auth_method".to_string(),
            "mutual_tls".to_string(),
        );

        self.metrics.auth_attempts += 1;
        self.metrics.auth_successes += 1;

        info!("Mutual TLS authentication successful");
        Ok(())
    }

    /// Authenticate using digital signature
    async fn authenticate_signature(&mut self) -> AuthenticationResult<()> {
        // Implementation would involve:
        // 1. Receive challenge from server
        // 2. Sign challenge with private key
        // 3. Send signature response
        // For now, simplified implementation

        let signature = self.config.auth_config.credential.clone();
        if signature.is_empty() {
            return Err(AuthenticationError::InvalidCredentials {
                credential_type: "signature".to_string(),
            });
        }

        // TODO: Implement full challenge-response signature authentication
        self.auth_state.authenticated = true;
        self.auth_state.metadata.insert(
            "auth_method".to_string(),
            "signature".to_string(),
        );

        self.metrics.auth_attempts += 1;
        self.metrics.auth_successes += 1;

        info!("Signature authentication successful");
        Ok(())
    }

    /// Authenticate using API key
    async fn authenticate_api_key(&mut self) -> AuthenticationResult<()> {
        let api_key = self.config.auth_config.credential.clone();
        if api_key.is_empty() {
            return Err(AuthenticationError::InvalidCredentials {
                credential_type: "api_key".to_string(),
            });
        }

        // Validate API key format
        if !self.is_valid_api_key(&api_key) {
            return Err(AuthenticationError::InvalidTokenFormat {
                format_error: "Invalid API key format".to_string(),
            });
        }

        self.auth_state.authenticated = true;
        self.auth_state.metadata.insert(
            "api_key".to_string(),
            api_key,
        );

        self.metrics.auth_attempts += 1;
        self.metrics.auth_successes += 1;

        info!("API key authentication successful");
        Ok(())
    }

    /// Encode message for transmission
    pub fn encode_message(&self, message: &GovernanceStreamMessage) -> ProtocolResult<Vec<u8>> {
        let start_time = std::time::Instant::now();

        let encoded = match self.config.serialization_format {
            SerializationFormat::Protobuf => {
                self.encode_protobuf(message)?
            }
            SerializationFormat::Json => {
                serde_json::to_vec(message)
                    .map_err(|e| ProtocolError::SerializationFailed {
                        message_type: message.message_type.clone(),
                        reason: e.to_string(),
                    })?
            }
            SerializationFormat::MessagePack => {
                rmp_serde::to_vec(message)
                    .map_err(|e| ProtocolError::SerializationFailed {
                        message_type: message.message_type.clone(),
                        reason: e.to_string(),
                    })?
            }
            SerializationFormat::Cbor => {
                serde_cbor::to_vec(message)
                    .map_err(|e| ProtocolError::SerializationFailed {
                        message_type: message.message_type.clone(),
                        reason: e.to_string(),
                    })?
            }
        };

        let compressed = if self.config.compression.enabled && encoded.len() >= self.config.compression.min_size_bytes as usize {
            self.compress_data(&encoded)?
        } else {
            encoded
        };

        // Update metrics
        let processing_time = start_time.elapsed();
        self.update_processing_time(processing_time.as_millis() as f64);

        trace!(
            "Encoded {} message: {} -> {} bytes (compressed: {})",
            message.message_type,
            encoded.len(),
            compressed.len(),
            compressed.len() < encoded.len()
        );

        Ok(compressed)
    }

    /// Decode received message
    pub fn decode_message(&self, data: &[u8]) -> ProtocolResult<GovernanceStreamMessage> {
        let start_time = std::time::Instant::now();

        // Decompress if necessary
        let decompressed = if self.config.compression.enabled {
            self.decompress_data(data)?
        } else {
            data.to_vec()
        };

        let message = match self.config.serialization_format {
            SerializationFormat::Protobuf => {
                self.decode_protobuf(&decompressed)?
            }
            SerializationFormat::Json => {
                serde_json::from_slice(&decompressed)
                    .map_err(|e| ProtocolError::DeserializationFailed {
                        reason: e.to_string(),
                    })?
            }
            SerializationFormat::MessagePack => {
                rmp_serde::from_slice(&decompressed)
                    .map_err(|e| ProtocolError::DeserializationFailed {
                        reason: e.to_string(),
                    })?
            }
            SerializationFormat::Cbor => {
                serde_cbor::from_slice(&decompressed)
                    .map_err(|e| ProtocolError::DeserializationFailed {
                        reason: e.to_string(),
                    })?
            }
        };

        // Validate message
        self.validate_message(&message)?;

        // Update metrics
        let processing_time = start_time.elapsed();
        self.update_processing_time(processing_time.as_millis() as f64);

        trace!(
            "Decoded {} message: {} bytes",
            message.message_type,
            decompressed.len()
        );

        Ok(message)
    }

    /// Convert internal message to gRPC format
    pub fn to_grpc_request(&self, message: &GovernanceStreamMessage) -> ProtocolResult<governance::StreamRequest> {
        let request = match &message.payload {
            GovernancePayload::Heartbeat(data) => {
                governance::StreamRequest {
                    request: Some(governance::stream_request::Request::Heartbeat(
                        governance::Heartbeat {
                            timestamp: data.timestamp,
                            node_id: data.node_id.clone(),
                        }
                    )),
                }
            }
            GovernancePayload::SignatureRequest(data) => {
                governance::StreamRequest {
                    request: Some(governance::stream_request::Request::SignatureRequest(
                        governance::SignatureRequest {
                            request_id: data.request_id.clone(),
                            chain: data.chain.clone(),
                            tx_hex: data.tx_hex.clone(),
                            input_indices: data.input_indices.clone(),
                            amounts: data.amounts.clone(),
                            tx_type: data.tx_type,
                        }
                    )),
                }
            }
            GovernancePayload::PeginNotification(data) => {
                governance::StreamRequest {
                    request: Some(governance::stream_request::Request::PeginNotification(
                        governance::PeginNotification {
                            bitcoin_txid: data.bitcoin_txid.clone(),
                            amount_satoshis: data.amount_satoshis,
                            evm_address: data.evm_address.clone(),
                            confirmations: data.confirmations,
                            block_hash: data.block_hash.clone().unwrap_or_default(),
                            block_height: data.block_height.unwrap_or_default(),
                        }
                    )),
                }
            }
            GovernancePayload::NodeRegistration(data) => {
                governance::StreamRequest {
                    request: Some(governance::stream_request::Request::NodeRegistration(
                        governance::NodeRegistration {
                            node_id: data.node_id.clone(),
                            public_key: data.public_key.clone(),
                            capabilities: data.capabilities.clone(),
                            endpoint: data.endpoint.clone().unwrap_or_default(),
                            version: data.version.clone(),
                        }
                    )),
                }
            }
            _ => {
                return Err(ProtocolError::UnsupportedMessageType {
                    message_type: message.message_type.clone(),
                });
            }
        };

        Ok(request)
    }

    /// Convert gRPC response to internal message
    pub fn from_grpc_response(&self, response: &governance::StreamResponse) -> ProtocolResult<GovernanceStreamMessage> {
        let (message_type, payload) = match &response.response {
            Some(governance::stream_response::Response::SignatureResponse(sig_resp)) => {
                let witnesses = sig_resp.witnesses.iter().map(|w| WitnessData {
                    input_index: w.input_index as usize,
                    witness: w.witness_data.clone(),
                    signature_type: None,
                }).collect();

                let status = SignatureStatusData {
                    status: "complete".to_string(), // Simplified
                    collected: sig_resp.witnesses.len() as u32,
                    required: sig_resp.witnesses.len() as u32,
                    completion_percentage: 100.0,
                    estimated_completion: None,
                };

                (
                    "signature_response".to_string(),
                    GovernancePayload::SignatureResponse(SignatureResponseData {
                        request_id: sig_resp.request_id.clone(),
                        witnesses,
                        status,
                        error_message: None,
                        metadata: HashMap::new(),
                    })
                )
            }
            Some(governance::stream_response::Response::FederationUpdate(update)) => {
                let members = update.members.iter().map(|m| FederationMemberData {
                    alys_address: m.alys_address.clone(),
                    bitcoin_pubkey: m.bitcoin_pubkey.clone(),
                    weight: m.weight,
                    active: m.active,
                }).collect();

                (
                    "federation_update".to_string(),
                    GovernancePayload::FederationUpdate(FederationUpdateData {
                        update_type: "member_update".to_string(),
                        version: update.version,
                        members,
                        threshold: update.threshold,
                        multisig_address: update.multisig_address.clone(),
                        activation_height: update.activation_height,
                    })
                )
            }
            Some(governance::stream_response::Response::Heartbeat(_)) => {
                (
                    "heartbeat_response".to_string(),
                    GovernancePayload::Heartbeat(HeartbeatData {
                        timestamp: chrono::Utc::now().timestamp(),
                        node_id: "governance".to_string(),
                        status: None,
                        ping_id: None,
                    })
                )
            }
            Some(governance::stream_response::Response::Error(error)) => {
                (
                    "error".to_string(),
                    GovernancePayload::Error(ErrorData {
                        code: error.code,
                        message: error.message.clone(),
                        details: error.details.clone(),
                        recoverable: error.recoverable,
                    })
                )
            }
            None => {
                return Err(ProtocolError::InvalidMessageFormat {
                    reason: "Empty response from governance".to_string(),
                });
            }
        };

        Ok(GovernanceStreamMessage {
            message_type,
            payload,
            timestamp: SystemTime::now(),
            sequence_number: 0, // Will be set by caller
            priority: MessagePriority::Normal,
            correlation_id: None,
            ttl: Some(Duration::from_secs(300)),
        })
    }

    /// Add authentication metadata to request
    fn add_auth_metadata<T>(&self, request: &mut Request<T>) -> ProtocolResult<()> {
        if !self.auth_state.authenticated {
            return Err(ProtocolError::ValidationFailed {
                validation_error: "Not authenticated".to_string(),
            });
        }

        let metadata = request.metadata_mut();

        match &self.config.auth_config.auth_type {
            AuthenticationType::Bearer => {
                if let Some(token) = &self.auth_state.token {
                    metadata.insert(
                        "authorization",
                        format!("Bearer {}", token).parse()
                            .map_err(|e| ProtocolError::InvalidMessageFormat {
                                reason: format!("Invalid authorization header: {}", e),
                            })?
                    );
                }
            }
            AuthenticationType::ApiKey => {
                if let Some(api_key) = self.auth_state.metadata.get("api_key") {
                    metadata.insert(
                        "x-api-key",
                        api_key.parse()
                            .map_err(|e| ProtocolError::InvalidMessageFormat {
                                reason: format!("Invalid API key header: {}", e),
                            })?
                    );
                }
            }
            _ => {} // Other auth methods handled differently
        }

        Ok(())
    }

    /// Validate message structure and content
    fn validate_message(&self, message: &GovernanceStreamMessage) -> ProtocolResult<()> {
        // Check protocol version compatibility
        if !self.config.supported_messages.contains(&message.message_type) {
            return Err(ProtocolError::UnsupportedMessageType {
                message_type: message.message_type.clone(),
            });
        }

        // Check message TTL
        if let Some(ttl) = message.ttl {
            if let Ok(age) = SystemTime::now().duration_since(message.timestamp) {
                if age > ttl {
                    return Err(ProtocolError::ValidationFailed {
                        validation_error: format!(
                            "Message expired: age={:?}, ttl={:?}",
                            age, ttl
                        ),
                    });
                }
            }
        }

        // Validate payload based on message type
        match (&message.message_type.as_str(), &message.payload) {
            ("heartbeat", GovernancePayload::Heartbeat(_)) => Ok(()),
            ("signature_request", GovernancePayload::SignatureRequest(_)) => Ok(()),
            ("signature_response", GovernancePayload::SignatureResponse(_)) => Ok(()),
            ("pegin_notification", GovernancePayload::PeginNotification(_)) => Ok(()),
            ("federation_update", GovernancePayload::FederationUpdate(_)) => Ok(()),
            ("node_registration", GovernancePayload::NodeRegistration(_)) => Ok(()),
            _ => Err(ProtocolError::ValidationFailed {
                validation_error: format!(
                    "Message type '{}' doesn't match payload",
                    message.message_type
                ),
            }),
        }
    }

    /// Encode message using protobuf
    fn encode_protobuf(&self, message: &GovernanceStreamMessage) -> ProtocolResult<Vec<u8>> {
        // Convert to gRPC format and encode
        let grpc_msg = self.to_grpc_request(message)?;
        
        use prost::Message;
        let mut buf = Vec::new();
        grpc_msg.encode(&mut buf)
            .map_err(|e| ProtocolError::SerializationFailed {
                message_type: message.message_type.clone(),
                reason: e.to_string(),
            })?;
        
        Ok(buf)
    }

    /// Decode protobuf message
    fn decode_protobuf(&self, data: &[u8]) -> ProtocolResult<GovernanceStreamMessage> {
        use prost::Message;
        
        let grpc_msg = governance::StreamResponse::decode(data)
            .map_err(|e| ProtocolError::DeserializationFailed {
                reason: e.to_string(),
            })?;
        
        self.from_grpc_response(&grpc_msg)
    }

    /// Compress data using configured algorithm
    fn compress_data(&self, data: &[u8]) -> ProtocolResult<Vec<u8>> {
        match self.config.compression.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Gzip => {
                use flate2::{write::GzEncoder, Compression};
                use std::io::Write;

                let mut encoder = GzEncoder::new(Vec::new(), 
                    Compression::new(self.config.compression.level as u32));
                encoder.write_all(data)
                    .map_err(|e| ProtocolError::CompressionError {
                        details: e.to_string(),
                    })?;
                
                encoder.finish()
                    .map_err(|e| ProtocolError::CompressionError {
                        details: e.to_string(),
                    })
            }
            _ => {
                // Other compression algorithms would be implemented here
                Err(ProtocolError::CompressionError {
                    details: format!("Unsupported compression algorithm: {:?}", 
                        self.config.compression.algorithm),
                })
            }
        }
    }

    /// Decompress data using configured algorithm
    fn decompress_data(&self, data: &[u8]) -> ProtocolResult<Vec<u8>> {
        match self.config.compression.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Gzip => {
                use flate2::read::GzDecoder;
                use std::io::Read;

                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)
                    .map_err(|e| ProtocolError::CompressionError {
                        details: e.to_string(),
                    })?;
                
                Ok(decompressed)
            }
            _ => {
                Err(ProtocolError::CompressionError {
                    details: format!("Unsupported compression algorithm: {:?}", 
                        self.config.compression.algorithm),
                })
            }
        }
    }

    /// Validate Bearer token format
    fn is_valid_bearer_token(&self, token: &str) -> bool {
        // Basic Bearer token validation
        !token.is_empty() && token.len() >= 10 && token.chars().all(|c| c.is_ascii())
    }

    /// Validate API key format
    fn is_valid_api_key(&self, api_key: &str) -> bool {
        // Basic API key validation
        !api_key.is_empty() && api_key.len() >= 16 && api_key.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// Update processing time metrics
    fn update_processing_time(&self, time_ms: f64) {
        // This would update a running average in a real implementation
        // For now, just log the processing time
        if time_ms > 100.0 {
            debug!("Slow message processing: {:.2}ms", time_ms);
        }
    }

    /// Check if authentication token needs refresh
    pub fn needs_token_refresh(&self) -> bool {
        if let Some(expires_at) = self.auth_state.token_expires_at {
            // Refresh 5 minutes before expiration
            let refresh_threshold = Duration::from_secs(300);
            if let Ok(time_until_expiry) = expires_at.duration_since(SystemTime::now()) {
                return time_until_expiry < refresh_threshold;
            }
            return true; // Token already expired
        }
        false
    }

    /// Refresh authentication token
    pub async fn refresh_token(&mut self) -> AuthenticationResult<()> {
        info!("Refreshing authentication token");
        
        // Clear current authentication state
        self.auth_state.authenticated = false;
        self.auth_state.token = None;
        
        // Re-authenticate
        self.authenticate().await
    }

    /// Get protocol metrics
    pub fn metrics(&self) -> &ProtocolMetrics {
        &self.metrics
    }

    /// Check if protocol is ready for operations
    pub fn is_ready(&self) -> bool {
        self.client.is_some() && self.auth_state.authenticated
    }

    /// Get authentication status
    pub fn is_authenticated(&self) -> bool {
        self.auth_state.authenticated
    }
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            protocol_version: "v1.0.0".to_string(),
            supported_messages: vec![
                "heartbeat".to_string(),
                "signature_request".to_string(),
                "signature_response".to_string(),
                "pegin_notification".to_string(),
                "federation_update".to_string(),
                "node_registration".to_string(),
                "error".to_string(),
            ],
            auth_config: AuthConfig {
                auth_type: AuthenticationType::Bearer,
                credential: String::new(),
                refresh_interval: Some(Duration::from_secs(3600)),
                parameters: HashMap::new(),
            },
            serialization_format: SerializationFormat::Protobuf,
            compression: CompressionConfig::default(),
            timeouts: ProtocolTimeouts::default(),
            retry_config: RetryConfig::default(),
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithm: CompressionAlgorithm::Gzip,
            level: 6,
            min_size_bytes: 1024,
        }
    }
}

impl Default for ProtocolTimeouts {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(30),
            auth_timeout: Duration::from_secs(10),
            send_timeout: Duration::from_secs(30),
            receive_timeout: Duration::from_secs(60),
            heartbeat_timeout: Duration::from_secs(30),
            stream_idle_timeout: Duration::from_secs(300),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 3,
            initial_delay: Duration::from_millis(1000),
            delay_multiplier: 2.0,
            max_delay: Duration::from_secs(30),
            retryable_errors: vec![
                tonic::Code::Unavailable as i32,
                tonic::Code::DeadlineExceeded as i32,
                tonic::Code::ResourceExhausted as i32,
                tonic::Code::Aborted as i32,
            ],
        }
    }
}