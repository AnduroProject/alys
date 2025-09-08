//! gRPC Service Definitions for Bridge Stream Protocol
//! 
//! Bridge-optimized gRPC services for governance communication

use std::collections::HashMap;
use std::time::SystemTime;
use tonic::{Request, Response, Status, Streaming};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::actors::bridge::{
    messages::stream_messages::*,
    shared::errors::BridgeError,
};

/// gRPC service definitions
pub mod governance {
    // TODO: Add protobuf build setup and include generated code
    // tonic::include_proto!("governance.bridge.v1");
    
    // Placeholder structures until protobuf build is configured
    #[derive(Debug, Clone)]
    pub struct GovernanceRequest {
        pub id: String,
        pub data: Vec<u8>,
    }
    
    #[derive(Debug, Clone)]
    pub struct GovernanceResponse {
        pub status: String,
        pub data: Vec<u8>,
    }
}

/// Bridge governance service implementation
#[derive(Debug, Clone)]
pub struct BridgeGovernanceService {
    /// Message sender for incoming requests
    request_sender: mpsc::Sender<IncomingRequest>,
}

/// Incoming gRPC request from governance nodes
#[derive(Debug)]
pub struct IncomingRequest {
    /// Request type
    pub request_type: String,
    /// Request payload
    pub payload: serde_json::Value,
    /// Response sender
    pub response_sender: tokio::sync::oneshot::Sender<Result<serde_json::Value, BridgeError>>,
}

/// Stream request message for gRPC
#[derive(Debug, Clone)]
pub struct StreamRequest {
    /// Request identifier
    pub request_id: String,
    /// Request type
    pub request_type: RequestType,
    /// Request payload (JSON-encoded)
    pub payload: serde_json::Value,
    /// Request timestamp
    pub timestamp: SystemTime,
    /// Request priority
    pub priority: i32,
}

/// Stream response message for gRPC
#[derive(Debug, Clone)]
pub struct StreamResponse {
    /// Response identifier (matches request_id)
    pub response_id: String,
    /// Response type
    pub response_type: ResponseType,
    /// Response payload (JSON-encoded)
    pub payload: serde_json::Value,
    /// Response timestamp
    pub timestamp: SystemTime,
    /// Success flag
    pub success: bool,
    /// Error message if failed
    pub error_message: Option<String>,
}

/// Request types for gRPC communication
#[derive(Debug, Clone, PartialEq)]
pub enum RequestType {
    PegOutSignature,
    FederationUpdate,
    Heartbeat,
    StatusCheck,
    NodeRegistration,
    PegInNotification,
}

/// Response types for gRPC communication
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseType {
    SignatureResponse,
    FederationUpdateAck,
    HeartbeatResponse,
    StatusResponse,
    RegistrationAck,
    NotificationAck,
    Error,
}

impl BridgeGovernanceService {
    /// Create new bridge governance service
    pub fn new(request_sender: mpsc::Sender<IncomingRequest>) -> Self {
        Self {
            request_sender,
        }
    }

    /// Handle bidirectional streaming
    pub async fn handle_bidirectional_stream(
        &self,
        request_stream: Streaming<governance::StreamRequest>,
    ) -> Result<Streaming<governance::StreamResponse>, Status> {
        info!("Handling bidirectional gRPC stream");
        
        // Create response stream
        let (response_sender, response_receiver) = mpsc::channel(1000);
        let response_stream = tokio_stream::wrappers::ReceiverStream::new(response_receiver);
        
        // Spawn task to handle incoming requests
        let request_sender_clone = self.request_sender.clone();
        tokio::spawn(async move {
            Self::handle_request_stream(request_stream, request_sender_clone, response_sender).await;
        });
        
        Ok(Streaming::new(response_stream))
    }

    /// Handle incoming request stream
    async fn handle_request_stream(
        mut request_stream: Streaming<governance::StreamRequest>,
        request_sender: mpsc::Sender<IncomingRequest>,
        response_sender: mpsc::Sender<governance::StreamResponse>,
    ) {
        while let Ok(Some(request)) = request_stream.message().await {
            debug!("Received gRPC request: {:?}", request.request_type);
            
            // Convert gRPC request to internal format
            match Self::convert_grpc_request(&request) {
                Ok(internal_request) => {
                    // Create response channel
                    let (resp_sender, resp_receiver) = tokio::sync::oneshot::channel();
                    
                    let incoming = IncomingRequest {
                        request_type: internal_request.request_type.clone(),
                        payload: internal_request.payload.clone(),
                        response_sender: resp_sender,
                    };
                    
                    // Send to internal handler
                    if let Err(e) = request_sender.send(incoming).await {
                        error!("Failed to forward incoming request: {:?}", e);
                        continue;
                    }
                    
                    // Wait for response and send back via gRPC
                    match resp_receiver.await {
                        Ok(Ok(response_payload)) => {
                            let grpc_response = governance::StreamResponse {
                                response_id: request.request_id.clone(),
                                response_type: Self::map_response_type(&internal_request.request_type),
                                payload: response_payload.to_string(),
                                timestamp: SystemTime::now()
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                success: true,
                                error_message: None,
                            };
                            
                            if let Err(e) = response_sender.send(grpc_response).await {
                                warn!("Failed to send gRPC response: {:?}", e);
                            }
                        }
                        Ok(Err(e)) => {
                            // Send error response
                            let error_response = governance::StreamResponse {
                                response_id: request.request_id.clone(),
                                response_type: "error".to_string(),
                                payload: "{}".to_string(),
                                timestamp: SystemTime::now()
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                success: false,
                                error_message: Some(format!("{:?}", e)),
                            };
                            
                            if let Err(e) = response_sender.send(error_response).await {
                                warn!("Failed to send error response: {:?}", e);
                            }
                        }
                        Err(_) => {
                            warn!("Response receiver cancelled for request {}", request.request_id);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to convert gRPC request: {:?}", e);
                    
                    // Send error response
                    let error_response = governance::StreamResponse {
                        response_id: request.request_id.clone(),
                        response_type: "error".to_string(),
                        payload: "{}".to_string(),
                        timestamp: SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                        success: false,
                        error_message: Some(format!("Request conversion failed: {:?}", e)),
                    };
                    
                    if let Err(e) = response_sender.send(error_response).await {
                        warn!("Failed to send error response: {:?}", e);
                    }
                }
            }
        }
        
        info!("Request stream ended");
    }

    /// Convert gRPC request to internal format
    fn convert_grpc_request(grpc_request: &governance::StreamRequest) -> Result<StreamRequest, BridgeError> {
        let request_type = match grpc_request.request_type.as_str() {
            "pegout_signature" => RequestType::PegOutSignature,
            "federation_update" => RequestType::FederationUpdate,
            "heartbeat" => RequestType::Heartbeat,
            "status_check" => RequestType::StatusCheck,
            "node_registration" => RequestType::NodeRegistration,
            "pegin_notification" => RequestType::PegInNotification,
            _ => {
                return Err(BridgeError::InvalidRequest(format!(
                    "Unknown request type: {}",
                    grpc_request.request_type
                )));
            }
        };

        let payload: serde_json::Value = serde_json::from_str(&grpc_request.payload)
            .map_err(|e| BridgeError::SerializationError(format!("Invalid JSON payload: {}", e)))?;

        Ok(StreamRequest {
            request_id: grpc_request.request_id.clone(),
            request_type,
            payload,
            timestamp: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(grpc_request.timestamp),
            priority: grpc_request.priority,
        })
    }

    /// Map request type to response type
    fn map_response_type(request_type: &str) -> String {
        match request_type {
            "pegout_signature" => "signature_response".to_string(),
            "federation_update" => "federation_update_ack".to_string(),
            "heartbeat" => "heartbeat_response".to_string(),
            "status_check" => "status_response".to_string(),
            "node_registration" => "registration_ack".to_string(),
            "pegin_notification" => "notification_ack".to_string(),
            _ => "error".to_string(),
        }
    }
}

/// Message conversion utilities
pub struct MessageConverter;

impl MessageConverter {
    /// Convert StreamMessage to gRPC format
    pub fn to_grpc_request(message: &StreamMessage) -> Result<governance::StreamRequest, BridgeError> {
        let (request_type, payload) = match message {
            StreamMessage::RequestPegOutSignatures { request } => (
                "pegout_signature".to_string(),
                serde_json::to_value(request)
                    .map_err(|e| BridgeError::SerializationError(e.to_string()))?
            ),
            StreamMessage::SendHeartbeat => (
                "heartbeat".to_string(),
                serde_json::json!({
                    "timestamp": SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    "node_id": "alys_bridge",
                    "status": "healthy"
                })
            ),
            StreamMessage::HandleFederationUpdate { update } => (
                "federation_update".to_string(),
                serde_json::to_value(update)
                    .map_err(|e| BridgeError::SerializationError(e.to_string()))?
            ),
            StreamMessage::NotifyPegIn { notification } => (
                "pegin_notification".to_string(),
                serde_json::to_value(notification)
                    .map_err(|e| BridgeError::SerializationError(e.to_string()))?
            ),
            StreamMessage::GetConnectionStatus => (
                "status_check".to_string(),
                serde_json::json!({
                    "request_time": SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                })
            ),
            _ => {
                return Err(BridgeError::InvalidRequest(
                    "Message type not supported for gRPC conversion".to_string()
                ));
            }
        };

        Ok(governance::StreamRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            request_type,
            payload: payload.to_string(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            priority: message.priority() as i32,
        })
    }

    /// Convert gRPC response to StreamResponse
    pub fn from_grpc_response(grpc_response: &governance::StreamResponse) -> Result<StreamResponse, BridgeError> {
        let response_type = match grpc_response.response_type.as_str() {
            "signature_response" => ResponseType::SignatureResponse,
            "federation_update_ack" => ResponseType::FederationUpdateAck,
            "heartbeat_response" => ResponseType::HeartbeatResponse,
            "status_response" => ResponseType::StatusResponse,
            "registration_ack" => ResponseType::RegistrationAck,
            "notification_ack" => ResponseType::NotificationAck,
            "error" => ResponseType::Error,
            _ => ResponseType::Error,
        };

        let payload: serde_json::Value = serde_json::from_str(&grpc_response.payload)
            .map_err(|e| BridgeError::SerializationError(format!("Invalid response payload: {}", e)))?;

        Ok(StreamResponse {
            response_id: grpc_response.response_id.clone(),
            response_type,
            payload,
            timestamp: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(grpc_response.timestamp),
            success: grpc_response.success,
            error_message: grpc_response.error_message.clone(),
        })
    }
}

/// Protobuf definitions placeholder
/// In a real implementation, these would be generated from .proto files
pub mod proto_stubs {
    /// Simplified gRPC message structures
    /// These would normally be generated by tonic from .proto files
    
    #[derive(Debug, Clone)]
    pub struct StreamRequest {
        pub request_id: String,
        pub request_type: String,
        pub payload: String,
        pub timestamp: u64,
        pub priority: i32,
    }

    #[derive(Debug, Clone)]
    pub struct StreamResponse {
        pub response_id: String,
        pub response_type: String,
        pub payload: String,
        pub timestamp: u64,
        pub success: bool,
        pub error_message: Option<String>,
    }

    #[derive(Debug, Clone)]
    pub struct Heartbeat {
        pub timestamp: u64,
        pub node_id: String,
        pub status: String,
    }

    #[derive(Debug, Clone)]
    pub struct SignatureRequest {
        pub request_id: String,
        pub pegout_id: String,
        pub transaction_hex: String,
        pub destination_address: String,
        pub amount: u64,
        pub fee: u64,
    }

    #[derive(Debug, Clone)]
    pub struct SignatureResponse {
        pub request_id: String,
        pub pegout_id: String,
        pub signatures: Vec<String>,
        pub approval_status: String,
        pub responding_nodes: Vec<String>,
    }

    #[derive(Debug, Clone)]
    pub struct FederationUpdate {
        pub update_id: String,
        pub update_type: String,
        pub effective_height: u64,
        pub members: Vec<FederationMember>,
        pub threshold: u32,
    }

    #[derive(Debug, Clone)]
    pub struct FederationMember {
        pub alys_address: String,
        pub bitcoin_pubkey: String,
        pub weight: u32,
        pub active: bool,
    }
}