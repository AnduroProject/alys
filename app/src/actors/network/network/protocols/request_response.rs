//! Request-Response Protocol Implementation
//! 
//! Alys-specific request-response protocol for block downloads, sync coordination,
//! and federation communication with custom codec and timeout management.

use libp2p::{
    request_response::{
        self, Behaviour as RequestResponse, Config as RequestResponseConfig, Event as RequestResponseEvent, 
        Message as RequestResponseMessage, ResponseChannel, OutboundRequestId,
    },
    futures::prelude::*,
    identity::Keypair,
    PeerId, StreamProtocol,
};

// Type alias for compatibility
type RequestId = OutboundRequestId;
use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::io;
use ethereum_types::H256;

/// Read length-prefixed data from an async reader
async fn read_length_prefixed<T>(io: &mut T, max_size: usize) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin,
{
    let mut length_bytes = [0u8; 4];
    io.read_exact(&mut length_bytes).await?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    
    if length > max_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Message too large: {} bytes (max: {})", length, max_size),
        ));
    }
    
    let mut buffer = vec![0u8; length];
    io.read_exact(&mut buffer).await?;
    Ok(buffer)
}

/// Write length-prefixed data to an async writer
async fn write_length_prefixed<T>(io: &mut T, data: Vec<u8>) -> io::Result<()>
where
    T: AsyncWrite + Unpin,
{
    let length = data.len() as u32;
    let length_bytes = length.to_be_bytes();
    
    io.write_all(&length_bytes).await?;
    io.write_all(&data).await?;
    io.flush().await?;
    Ok(())
}

/// Alys request-response protocol for blockchain operations
pub struct AlysRequestResponse {
    /// Core request-response behaviour
    request_response: RequestResponse<AlysCodec>,
    /// Active outbound requests
    active_requests: HashMap<RequestId, ActiveRequest>,
    /// Request handlers for different message types
    request_handlers: HashMap<AlysRequestType, Box<dyn RequestHandler>>,
    /// Performance metrics
    metrics: RequestResponseMetrics,
    /// Configuration
    config: RequestResponseConfig,
}

impl AlysRequestResponse {
    /// Create a new Alys request-response protocol
    pub fn new() -> Self {
        let protocol = AlysProtocol;
        let codec = AlysCodec::default();
        
        let mut config = RequestResponseConfig::default();
        config.set_request_timeout(Duration::from_secs(30)); // 30 second timeout
        config.set_connection_keep_alive(Duration::from_secs(60)); // Keep alive for 1 minute

        let request_response = RequestResponse::new(
            codec,
            std::iter::once((protocol, request_response::ProtocolSupport::Full)),
            config.clone(),
        );

        let mut handlers: HashMap<AlysRequestType, Box<dyn RequestHandler>> = HashMap::new();
        handlers.insert(AlysRequestType::BlockRequest, Box::new(BlockRequestHandler::new()));
        handlers.insert(AlysRequestType::SyncStatus, Box::new(SyncStatusHandler::new()));
        handlers.insert(AlysRequestType::FederationMessage, Box::new(FederationHandler::new()));
        handlers.insert(AlysRequestType::PeerInfo, Box::new(PeerInfoHandler::new()));

        Self {
            request_response,
            active_requests: HashMap::new(),
            request_handlers: handlers,
            metrics: RequestResponseMetrics::default(),
            config,
        }
    }

    /// Send a request to a peer
    pub fn send_request(
        &mut self,
        peer_id: PeerId,
        request: AlysRequest,
        timeout: Option<Duration>,
    ) -> RequestId {
        let request_id = self.request_response.send_request(&peer_id, request.clone());

        // Track active request
        self.active_requests.insert(request_id, ActiveRequest {
            peer_id,
            request: request.clone(),
            started_at: Instant::now(),
            timeout: timeout.unwrap_or(Duration::from_secs(30)),
        });

        self.metrics.requests_sent += 1;
        tracing::debug!("Sent {:?} request to {} (ID: {:?})", request.request_type(), peer_id, request_id);

        request_id
    }

    /// Send a response to an incoming request
    pub fn send_response(
        &mut self,
        channel: ResponseChannel<AlysResponse>,
        response: AlysResponse,
    ) -> Result<(), AlysResponse> {
        self.metrics.responses_sent += 1;
        self.request_response.send_response(channel, response)
    }

    /// Handle incoming request-response events
    pub fn handle_event(&mut self, event: RequestResponseEvent<AlysRequest, AlysResponse>) -> Vec<AlysRequestResponseEvent> {
        let mut alys_events = Vec::new();

        match event {
            RequestResponseEvent::Message { peer, message } => {
                match message {
                    RequestResponseMessage::Request { request_id, request, channel } => {
                        self.metrics.requests_received += 1;
                        tracing::debug!("Received {:?} request from {} (ID: {:?})", 
                            request.request_type(), peer, request_id);

                        // Handle the request
                        let response = self.handle_incoming_request(request.clone(), &peer);
                        
                        // Send response
                        match self.send_response(channel, response.clone()) {
                            Ok(_) => {
                                tracing::debug!("Sent response to {} for request {:?}", peer, request_id);
                            }
                            Err(e) => {
                                tracing::error!("Failed to send response to {}: {:?}", peer, e);
                                self.metrics.response_failures += 1;
                            }
                        }

                        alys_events.push(AlysRequestResponseEvent::InboundRequest {
                            peer_id: peer,
                            request_id,
                            request,
                            response,
                        });
                    }
                    RequestResponseMessage::Response { request_id, response } => {
                        self.metrics.responses_received += 1;
                        
                        // Remove from active requests and calculate duration
                        let duration = if let Some(active_request) = self.active_requests.remove(&request_id) {
                            let duration = active_request.started_at.elapsed();
                            self.metrics.update_response_time(duration);
                            duration
                        } else {
                            Duration::from_secs(0)
                        };

                        tracing::debug!("Received response from {} for request {:?} in {:?}", 
                            peer, request_id, duration);

                        alys_events.push(AlysRequestResponseEvent::InboundResponse {
                            peer_id: peer,
                            request_id,
                            response,
                            duration,
                        });
                    }
                }
            }
            RequestResponseEvent::OutboundFailure { peer, request_id, error } => {
                self.metrics.request_failures += 1;
                
                // Remove from active requests
                self.active_requests.remove(&request_id);
                
                tracing::warn!("Outbound request {:?} to {} failed: {:?}", request_id, peer, error);
                
                alys_events.push(AlysRequestResponseEvent::OutboundFailure {
                    peer_id: peer,
                    request_id,
                    error: error.to_string(),
                });
            }
            RequestResponseEvent::InboundFailure { peer, request_id, error } => {
                self.metrics.response_failures += 1;
                tracing::warn!("Inbound request {:?} from {} failed: {:?}", request_id, peer, error);
                
                alys_events.push(AlysRequestResponseEvent::InboundFailure {
                    peer_id: peer,
                    request_id,
                    error: error.to_string(),
                });
            }
            RequestResponseEvent::ResponseSent { peer, request_id } => {
                tracing::debug!("Response sent to {} for request {:?}", peer, request_id);
            }
        }

        // Clean up expired requests
        self.cleanup_expired_requests();

        alys_events
    }

    /// Get current metrics
    pub fn metrics(&self) -> &RequestResponseMetrics {
        &self.metrics
    }

    // Private helper methods

    fn handle_incoming_request(&self, request: AlysRequest, peer: &PeerId) -> AlysResponse {
        let request_type = request.request_type();
        
        if let Some(handler) = self.request_handlers.get(&request_type) {
            handler.handle_request(request, peer)
        } else {
            AlysResponse::Error {
                code: 404,
                message: format!("No handler for request type: {:?}", request_type),
            }
        }
    }

    fn cleanup_expired_requests(&mut self) {
        let now = Instant::now();
        let expired_requests: Vec<_> = self.active_requests
            .iter()
            .filter(|(_, req)| now.duration_since(req.started_at) > req.timeout)
            .map(|(id, _)| *id)
            .collect();

        for request_id in expired_requests {
            if let Some(expired_request) = self.active_requests.remove(&request_id) {
                self.metrics.request_timeouts += 1;
                tracing::warn!(
                    "Request {:?} to {} timed out after {:?}",
                    request_id, expired_request.peer_id, expired_request.timeout
                );
            }
        }
    }
}

// Protocol definition

#[derive(Debug, Clone)]
pub struct AlysProtocol;

impl From<AlysProtocol> for StreamProtocol {
    fn from(_: AlysProtocol) -> Self {
        StreamProtocol::new("/alys/req-resp/1.0.0")
    }
}

// Codec for serializing/deserializing requests and responses

#[derive(Debug, Clone, Default)]
pub struct AlysCodec;

#[async_trait]
impl request_response::Codec for AlysCodec {
    type Protocol = AlysProtocol;
    type Request = AlysRequest;
    type Response = AlysResponse;

    async fn read_request<T>(&mut self, _protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_length_prefixed(io, 1024 * 1024).await?; // 1MB max
        let request: AlysRequest = bincode::deserialize(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(request)
    }

    async fn read_response<T>(&mut self, _protocol: &Self::Protocol, io: &mut T) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_length_prefixed(io, 1024 * 1024).await?; // 1MB max
        let response: AlysResponse = bincode::deserialize(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(response)
    }

    async fn write_request<T>(&mut self, _protocol: &Self::Protocol, io: &mut T, req: Self::Request) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_length_prefixed(io, bytes).await
    }

    async fn write_response<T>(&mut self, _protocol: &Self::Protocol, io: &mut T, res: Self::Response) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = bincode::serialize(&res)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_length_prefixed(io, bytes).await
    }
}

// Request and Response types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlysRequest {
    /// Request specific blocks by height range
    BlockRequest {
        start_height: u64,
        end_height: u64,
        max_blocks: u32,
    },
    /// Request current sync status
    SyncStatus,
    /// Request peer information
    PeerInfo,
    /// Federation-specific message
    FederationMessage {
        message_type: FederationMessageType,
        data: Vec<u8>,
        signature: Option<Vec<u8>>,
    },
    /// Request transaction pool status
    TxPoolStatus,
    /// Custom request type for extensions
    Custom {
        request_type: String,
        data: Vec<u8>,
    },
}

impl AlysRequest {
    pub fn request_type(&self) -> AlysRequestType {
        match self {
            AlysRequest::BlockRequest { .. } => AlysRequestType::BlockRequest,
            AlysRequest::SyncStatus => AlysRequestType::SyncStatus,
            AlysRequest::PeerInfo => AlysRequestType::PeerInfo,
            AlysRequest::FederationMessage { .. } => AlysRequestType::FederationMessage,
            AlysRequest::TxPoolStatus => AlysRequestType::TxPoolStatus,
            AlysRequest::Custom { .. } => AlysRequestType::Custom,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlysResponse {
    /// Block data response
    Blocks {
        blocks: Vec<BlockInfo>,
        has_more: bool,
    },
    /// Sync status response
    SyncStatus {
        current_height: u64,
        target_height: Option<u64>,
        is_syncing: bool,
        progress: f64,
    },
    /// Peer information response
    PeerInfo {
        peer_id: String,
        addresses: Vec<String>,
        protocols: Vec<String>,
        is_federation_peer: bool,
    },
    /// Federation message response
    FederationResponse {
        success: bool,
        data: Vec<u8>,
    },
    /// Transaction pool status response
    TxPoolStatus {
        pending_count: u32,
        queued_count: u32,
        total_size_bytes: u64,
    },
    /// Error response
    Error {
        code: u32,
        message: String,
    },
    /// Custom response
    Custom {
        response_type: String,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlysRequestType {
    BlockRequest,
    SyncStatus,
    PeerInfo,
    FederationMessage,
    TxPoolStatus,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationMessageType {
    ConsensusMessage,
    BlockProposal,
    EmergencySignal,
    ConfigUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub height: u64,
    pub hash: H256,
    pub parent_hash: H256,
    pub timestamp: u64,
    pub data: Vec<u8>,
}

// Request handlers

trait RequestHandler: Send + Sync {
    fn handle_request(&self, request: AlysRequest, peer: &PeerId) -> AlysResponse;
}

struct BlockRequestHandler;

impl BlockRequestHandler {
    fn new() -> Self {
        Self
    }
}

impl RequestHandler for BlockRequestHandler {
    fn handle_request(&self, request: AlysRequest, _peer: &PeerId) -> AlysResponse {
        if let AlysRequest::BlockRequest { start_height, end_height, max_blocks } = request {
            // In a real implementation, this would fetch blocks from storage
            let blocks = Vec::new(); // Placeholder
            
            AlysResponse::Blocks {
                blocks,
                has_more: false,
            }
        } else {
            AlysResponse::Error {
                code: 400,
                message: "Invalid request type for BlockRequestHandler".to_string(),
            }
        }
    }
}

struct SyncStatusHandler;

impl SyncStatusHandler {
    fn new() -> Self {
        Self
    }
}

impl RequestHandler for SyncStatusHandler {
    fn handle_request(&self, request: AlysRequest, _peer: &PeerId) -> AlysResponse {
        if let AlysRequest::SyncStatus = request {
            // In a real implementation, this would get status from SyncActor
            AlysResponse::SyncStatus {
                current_height: 1000,
                target_height: Some(1050),
                is_syncing: true,
                progress: 0.95,
            }
        } else {
            AlysResponse::Error {
                code: 400,
                message: "Invalid request type for SyncStatusHandler".to_string(),
            }
        }
    }
}

struct FederationHandler;

impl FederationHandler {
    fn new() -> Self {
        Self
    }
}

impl RequestHandler for FederationHandler {
    fn handle_request(&self, request: AlysRequest, peer: &PeerId) -> AlysResponse {
        if let AlysRequest::FederationMessage { message_type, data, signature } = request {
            tracing::info!("Handling federation {:?} from {}", message_type, peer);
            
            // In a real implementation, this would:
            // 1. Verify signature
            // 2. Process message based on type
            // 3. Return appropriate response
            
            AlysResponse::FederationResponse {
                success: true,
                data: vec![],
            }
        } else {
            AlysResponse::Error {
                code: 400,
                message: "Invalid request type for FederationHandler".to_string(),
            }
        }
    }
}

struct PeerInfoHandler;

impl PeerInfoHandler {
    fn new() -> Self {
        Self
    }
}

impl RequestHandler for PeerInfoHandler {
    fn handle_request(&self, request: AlysRequest, _peer: &PeerId) -> AlysResponse {
        if let AlysRequest::PeerInfo = request {
            AlysResponse::PeerInfo {
                peer_id: "12D3KooW...".to_string(), // Would be actual peer ID
                addresses: vec!["/ip4/127.0.0.1/tcp/8000".to_string()],
                protocols: vec!["alys/req-resp/1.0.0".to_string()],
                is_federation_peer: false,
            }
        } else {
            AlysResponse::Error {
                code: 400,
                message: "Invalid request type for PeerInfoHandler".to_string(),
            }
        }
    }
}

// Supporting types

#[derive(Debug)]
pub struct ActiveRequest {
    pub peer_id: PeerId,
    pub request: AlysRequest,
    pub started_at: Instant,
    pub timeout: Duration,
}

#[derive(Debug)]
pub enum AlysRequestResponseEvent {
    InboundRequest {
        peer_id: PeerId,
        request_id: RequestId,
        request: AlysRequest,
        response: AlysResponse,
    },
    InboundResponse {
        peer_id: PeerId,
        request_id: RequestId,
        response: AlysResponse,
        duration: Duration,
    },
    OutboundFailure {
        peer_id: PeerId,
        request_id: RequestId,
        error: String,
    },
    InboundFailure {
        peer_id: PeerId,
        request_id: RequestId,
        error: String,
    },
}

#[derive(Default)]
pub struct RequestResponseMetrics {
    pub requests_sent: u64,
    pub requests_received: u64,
    pub responses_sent: u64,
    pub responses_received: u64,
    pub request_failures: u64,
    pub response_failures: u64,
    pub request_timeouts: u64,
    pub total_response_time: Duration,
    pub response_count: u64,
}

impl RequestResponseMetrics {
    pub fn update_response_time(&mut self, duration: Duration) {
        self.total_response_time += duration;
        self.response_count += 1;
    }

    pub fn average_response_time(&self) -> Duration {
        if self.response_count > 0 {
            self.total_response_time / self.response_count as u32
        } else {
            Duration::from_secs(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_type_mapping() {
        let block_req = AlysRequest::BlockRequest {
            start_height: 100,
            end_height: 200,
            max_blocks: 100,
        };
        
        assert_eq!(block_req.request_type(), AlysRequestType::BlockRequest);
        
        let sync_req = AlysRequest::SyncStatus;
        assert_eq!(sync_req.request_type(), AlysRequestType::SyncStatus);
    }

    #[test]
    fn test_block_request_handler() {
        let handler = BlockRequestHandler::new();
        let request = AlysRequest::BlockRequest {
            start_height: 100,
            end_height: 150,
            max_blocks: 50,
        };
        let peer = PeerId::random();
        
        let response = handler.handle_request(request, &peer);
        match response {
            AlysResponse::Blocks { blocks, has_more } => {
                // Placeholder returns empty blocks
                assert_eq!(blocks.len(), 0);
                assert!(!has_more);
            }
            _ => panic!("Expected Blocks response"),
        }
    }

    #[test]
    fn test_federation_message_serialization() {
        let request = AlysRequest::FederationMessage {
            message_type: FederationMessageType::ConsensusMessage,
            data: vec![1, 2, 3, 4],
            signature: Some(vec![5, 6, 7, 8]),
        };

        let serialized = bincode::serialize(&request).unwrap();
        let deserialized: AlysRequest = bincode::deserialize(&serialized).unwrap();

        if let AlysRequest::FederationMessage { message_type, data, signature } = deserialized {
            assert_eq!(data, vec![1, 2, 3, 4]);
            assert_eq!(signature, Some(vec![5, 6, 7, 8]));
        } else {
            panic!("Deserialization failed");
        }
    }

    #[test]
    fn test_metrics_response_time_calculation() {
        let mut metrics = RequestResponseMetrics::default();
        
        metrics.update_response_time(Duration::from_millis(100));
        metrics.update_response_time(Duration::from_millis(200));
        metrics.update_response_time(Duration::from_millis(300));
        
        let avg = metrics.average_response_time();
        assert_eq!(avg, Duration::from_millis(200));
    }
}