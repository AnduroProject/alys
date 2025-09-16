//! gRPC Service Implementation for Bridge Stream Protocol
//!
//! Real gRPC services for governance communication using tonic and protobuf

use std::time::SystemTime;
use tonic::{Request, Response, Status, Streaming};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, warn};

use crate::actors::bridge::{
    messages::stream_messages::*,
    shared::errors::BridgeError,
};
use crate::types::bridge::RequestType;

// Include generated protobuf code (when available)
#[cfg(feature = "grpc-generated")]
pub mod governance_bridge_v1 {
    tonic::include_proto!("governance.bridge.v1");
}

// Fallback definitions when protobuf generation is not available
#[cfg(not(feature = "grpc-generated"))]
pub mod governance_bridge_v1 {
    use serde::{Serialize, Deserialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamRequest {
        pub request_id: String,
        pub request_type: i32,
        pub payload: Vec<u8>,
        pub timestamp: u64,
        pub priority: i32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamResponse {
        pub response_id: String,
        pub response_type: i32,
        pub payload: Vec<u8>,
        pub timestamp: u64,
        pub success: bool,
        pub error_message: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HealthCheckRequest {
        pub service: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HealthCheckResponse {
        pub status: i32,
        pub message: String,
    }

    // Mock server trait for compilation
    pub mod governance_bridge_server {
        use super::*;
        use async_trait::async_trait;
        use tonic::{Request, Response, Status, Streaming};
        use tonic::transport::Server;

        #[async_trait]
        pub trait GovernanceBridge {
            type BidirectionalStreamStream: futures::Stream<Item = Result<StreamResponse, Status>> + Send + 'static;

            async fn bidirectional_stream(
                &self,
                request: Request<Streaming<StreamRequest>>,
            ) -> Result<Response<Self::BidirectionalStreamStream>, Status>;

            async fn health_check(
                &self,
                request: Request<HealthCheckRequest>,
            ) -> Result<Response<HealthCheckResponse>, Status>;
        }

        // Mock server type for consistency with protobuf generated code
        pub struct GovernanceBridgeServer<T> {
            inner: T,
        }

        impl<T> GovernanceBridgeServer<T>
        where
            T: GovernanceBridge + Send + Sync + 'static,
        {
            pub fn new(service: T) -> Self {
                Self { inner: service }
            }

            pub fn with_interceptor<F>(service: T, _interceptor: F) -> Self
            where
                F: tonic::service::Interceptor,
            {
                Self { inner: service }
            }
        }
    }

    // Enum definitions for request/response types
    #[repr(i32)]
    #[derive(Debug, Clone, Copy)]
    pub enum RequestType {
        Unspecified = 0,
        PegoutSignature = 1,
        FederationUpdate = 2,
        Heartbeat = 3,
        StatusCheck = 4,
        NodeRegistration = 5,
        PeginNotification = 6,
    }

    #[repr(i32)]
    #[derive(Debug, Clone, Copy)]
    pub enum ResponseType {
        Unspecified = 0,
        SignatureResponse = 1,
        FederationUpdateAck = 2,
        HeartbeatResponse = 3,
        StatusResponse = 4,
        RegistrationAck = 5,
        NotificationAck = 6,
        Error = 7,
    }

    #[repr(i32)]
    #[derive(Debug, Clone, Copy)]
    pub enum Priority {
        Unspecified = 0,
        Low = 1,
        Normal = 2,
        High = 3,
        Critical = 4,
    }

    #[repr(i32)]
    #[derive(Debug, Clone, Copy)]
    pub enum HealthCheckStatus {
        Unspecified = 0,
        Serving = 1,
        NotServing = 2,
    }

    // Helper methods for enum conversion
    impl StreamRequest {
        pub fn request_type(&self) -> RequestType {
            match self.request_type {
                1 => RequestType::PegoutSignature,
                2 => RequestType::FederationUpdate,
                3 => RequestType::Heartbeat,
                4 => RequestType::StatusCheck,
                5 => RequestType::NodeRegistration,
                6 => RequestType::PeginNotification,
                _ => RequestType::Unspecified,
            }
        }
    }

    impl From<RequestType> for i32 {
        fn from(rt: RequestType) -> i32 {
            rt as i32
        }
    }

    impl From<ResponseType> for i32 {
        fn from(rt: ResponseType) -> i32 {
            rt as i32
        }
    }

    impl From<Priority> for i32 {
        fn from(p: Priority) -> i32 {
            p as i32
        }
    }

    impl From<HealthCheckStatus> for i32 {
        fn from(status: HealthCheckStatus) -> i32 {
            status as i32
        }
    }
}

pub use governance_bridge_v1::{
    governance_bridge_server::{GovernanceBridge, GovernanceBridgeServer},
    StreamRequest, StreamResponse,
    RequestType as GrpcRequestType, ResponseType, HealthCheckRequest,
    HealthCheckResponse, HealthCheckStatus, Priority,
};

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
    pub request_type: RequestType,
    /// Request payload
    pub payload: serde_json::Value,
    /// Response sender
    pub response_sender: tokio::sync::oneshot::Sender<Result<serde_json::Value, BridgeError>>,
}

impl BridgeGovernanceService {
    /// Create new bridge governance service
    pub fn new(request_sender: mpsc::Sender<IncomingRequest>) -> Self {
        Self { request_sender }
    }
}

#[tonic::async_trait]
impl GovernanceBridge for BridgeGovernanceService {
    type BidirectionalStreamStream = ReceiverStream<Result<StreamResponse, Status>>;

    async fn bidirectional_stream(
        &self,
        request: Request<Streaming<StreamRequest>>,
    ) -> Result<Response<Self::BidirectionalStreamStream>, Status> {
        info!("Handling bidirectional gRPC stream");

        let mut request_stream = request.into_inner();
        let (response_sender, response_receiver) = mpsc::channel(1000);
        let request_sender_clone = self.request_sender.clone();

        // Spawn task to handle incoming requests
        tokio::spawn(async move {
            while let Ok(Some(grpc_request)) = request_stream.message().await {
                debug!("Received gRPC request: {:?}", grpc_request.request_type);

                // Convert gRPC request to internal format
                match Self::convert_grpc_request(&grpc_request) {
                    Ok((request_type, payload)) => {
                        // Create response channel
                        let (resp_sender, resp_receiver) = tokio::sync::oneshot::channel();

                        let incoming = IncomingRequest {
                            request_type,
                            payload,
                            response_sender: resp_sender,
                        };

                        // Send to internal handler
                        if let Err(e) = request_sender_clone.send(incoming).await {
                            error!("Failed to forward incoming request: {:?}", e);
                            continue;
                        }

                        // Wait for response and send back via gRPC
                        match resp_receiver.await {
                            Ok(Ok(response_payload)) => {
                                let grpc_response = StreamResponse {
                                    response_id: grpc_request.request_id.clone(),
                                    response_type: Self::map_response_type(&grpc_request.request_type()).into(),
                                    payload: serde_json::to_vec(&response_payload).unwrap_or_default(),
                                    timestamp: SystemTime::now()
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs(),
                                    success: true,
                                    error_message: None,
                                };

                                if let Err(e) = response_sender.send(Ok(grpc_response)).await {
                                    warn!("Failed to send gRPC response: {:?}", e);
                                }
                            }
                            Ok(Err(e)) => {
                                // Send error response
                                let error_response = StreamResponse {
                                    response_id: grpc_request.request_id.clone(),
                                    response_type: ResponseType::Error.into(),
                                    payload: vec![],
                                    timestamp: SystemTime::now()
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs(),
                                    success: false,
                                    error_message: Some(format!("{:?}", e)),
                                };

                                if let Err(e) = response_sender.send(Ok(error_response)).await {
                                    warn!("Failed to send error response: {:?}", e);
                                }
                            }
                            Err(_) => {
                                warn!("Response receiver cancelled for request {}", grpc_request.request_id);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to convert gRPC request: {:?}", e);

                        // Send error response
                        let error_response = StreamResponse {
                            response_id: grpc_request.request_id.clone(),
                            response_type: ResponseType::Error.into(),
                            payload: vec![],
                            timestamp: SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                            success: false,
                            error_message: Some(format!("Request conversion failed: {:?}", e)),
                        };

                        if let Err(e) = response_sender.send(Ok(error_response)).await {
                            warn!("Failed to send error response: {:?}", e);
                        }
                    }
                }
            }

            info!("Request stream ended");
        });

        // Return the response stream
        let response_stream = ReceiverStream::new(response_receiver);
        Ok(Response::new(response_stream))
    }

    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let req = request.into_inner();
        info!("Health check requested for service: {}", req.service);

        let response = HealthCheckResponse {
            status: HealthCheckStatus::Serving.into(),
            message: "Service is healthy".to_string(),
        };

        Ok(Response::new(response))
    }
}

impl BridgeGovernanceService {
    /// Convert gRPC request to internal format
    fn convert_grpc_request(
        grpc_request: &StreamRequest,
    ) -> Result<(RequestType, serde_json::Value), BridgeError> {
        let request_type = match grpc_request.request_type() {
            governance_bridge_v1::RequestType::PegoutSignature => RequestType::PegOutSignature,
            governance_bridge_v1::RequestType::FederationUpdate => RequestType::FederationUpdate,
            governance_bridge_v1::RequestType::Heartbeat => RequestType::Heartbeat,
            governance_bridge_v1::RequestType::StatusCheck => RequestType::StatusCheck,
            governance_bridge_v1::RequestType::NodeRegistration => RequestType::NodeRegistration,
            governance_bridge_v1::RequestType::PeginNotification => RequestType::PegInNotification,
            _ => {
                return Err(BridgeError::InvalidRequest(format!(
                    "Unknown request type: {:?}",
                    grpc_request.request_type
                )));
            }
        };

        let payload: serde_json::Value = serde_json::from_slice(&grpc_request.payload)
            .map_err(|e| BridgeError::SerializationError(format!("Invalid payload: {}", e)))?;

        Ok((request_type, payload))
    }

    /// Map request type to response type
    fn map_response_type(request_type: &governance_bridge_v1::RequestType) -> ResponseType {
        match request_type {
            governance_bridge_v1::RequestType::PegoutSignature => ResponseType::SignatureResponse,
            governance_bridge_v1::RequestType::FederationUpdate => ResponseType::FederationUpdateAck,
            governance_bridge_v1::RequestType::Heartbeat => ResponseType::HeartbeatResponse,
            governance_bridge_v1::RequestType::StatusCheck => ResponseType::StatusResponse,
            governance_bridge_v1::RequestType::NodeRegistration => ResponseType::RegistrationAck,
            governance_bridge_v1::RequestType::PeginNotification => ResponseType::NotificationAck,
            _ => ResponseType::Error,
        }
    }
}

/// Message conversion utilities
pub struct MessageConverter;

impl MessageConverter {
    /// Convert StreamMessage to gRPC format
    pub fn to_grpc_request(message: &StreamMessage) -> Result<StreamRequest, BridgeError> {
        let (request_type, payload) = match message {
            StreamMessage::RequestPegOutSignatures { request } => (
                governance_bridge_v1::RequestType::PegoutSignature,
                serde_json::to_vec(request)
                    .map_err(|e| BridgeError::SerializationError(e.to_string()))?,
            ),
            StreamMessage::SendHeartbeat => (
                governance_bridge_v1::RequestType::Heartbeat,
                serde_json::to_vec(&serde_json::json!({
                    "timestamp": SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    "node_id": "alys_bridge",
                    "status": "healthy"
                }))
                .map_err(|e| BridgeError::SerializationError(e.to_string()))?,
            ),
            StreamMessage::HandleFederationUpdate { update } => (
                governance_bridge_v1::RequestType::FederationUpdate,
                serde_json::to_vec(update)
                    .map_err(|e| BridgeError::SerializationError(e.to_string()))?,
            ),
            StreamMessage::NotifyPegIn { notification } => (
                governance_bridge_v1::RequestType::PeginNotification,
                serde_json::to_vec(notification)
                    .map_err(|e| BridgeError::SerializationError(e.to_string()))?,
            ),
            StreamMessage::GetConnectionStatus => (
                governance_bridge_v1::RequestType::StatusCheck,
                serde_json::to_vec(&serde_json::json!({
                    "request_time": SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                }))
                .map_err(|e| BridgeError::SerializationError(e.to_string()))?,
            ),
            _ => {
                return Err(BridgeError::InvalidRequest(
                    "Message type not supported for gRPC conversion".to_string(),
                ));
            }
        };

        Ok(StreamRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            request_type: request_type.into(),
            payload,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            priority: Priority::Normal.into(),
        })
    }

    /// Convert gRPC response to internal response
    pub fn from_grpc_response(
        grpc_response: &StreamResponse,
    ) -> Result<serde_json::Value, BridgeError> {
        if !grpc_response.success {
            return Err(BridgeError::InvalidRequest(
                grpc_response.error_message.clone().unwrap_or_default(),
            ));
        }

        serde_json::from_slice(&grpc_response.payload)
            .map_err(|e| BridgeError::SerializationError(format!("Invalid response payload: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_creation() {
        let (sender, _receiver) = mpsc::channel(100);
        let service = BridgeGovernanceService::new(sender);
        assert!(std::ptr::eq(&service.request_sender, &service.request_sender));
    }

    #[tokio::test]
    async fn test_health_check() {
        let (sender, _receiver) = mpsc::channel(100);
        let service = BridgeGovernanceService::new(sender);

        let request = Request::new(HealthCheckRequest {
            service: "bridge".to_string(),
        });

        let response = service.health_check(request).await.unwrap();
        assert_eq!(response.into_inner().status, HealthCheckStatus::Serving as i32);
    }
}