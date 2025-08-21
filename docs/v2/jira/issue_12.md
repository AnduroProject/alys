# ALYS-012: Implement StreamActor for Governance Communication

## Issue Type
Task

## Priority
Critical

## Story Points
8

## Sprint
Migration Sprint 5

## Component
Governance Integration

## Labels
`migration`, `phase-5`, `governance`, `actor-system`, `stream`

## Description

Implement the StreamActor to establish and maintain persistent bi-directional streaming communication with Anduro Governance. This actor handles message routing, connection resilience, buffering during disconnections, and serves as the gateway for all governance operations including signature requests and federation updates.

## Acceptance Criteria

- [ ] StreamActor maintains persistent gRPC stream connection
- [ ] Automatic reconnection with exponential backoff
- [ ] Message buffering during disconnections
- [ ] Bi-directional message routing implemented
- [ ] Health monitoring and status reporting
- [ ] No cryptographic operations (delegated to governance)
- [ ] Integration with BridgeActor for signatures
- [ ] Federation membership updates handled
- [ ] Comprehensive error handling and recovery

## Technical Details

### Implementation Steps

1. **Define Stream Protocol and Messages**
```rust
// src/actors/stream/messages.rs

use actix::prelude::*;
use tonic::Streaming;
use prost::Message as ProstMessage;

// Proto definitions
pub mod governance {
    tonic::include_proto!("governance.v1");
}

use governance::{StreamRequest, StreamResponse};

/// Messages handled by StreamActor
#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct EstablishConnection {
    pub endpoint: String,
    pub auth_token: Option<String>,
    pub chain_id: String,
}

#[derive(Message)]
#[rtype(result = "Result<ConnectionStatus, StreamError>")]
pub struct GetConnectionStatus;

#[derive(Message)]
#[rtype(result = "Result<String, StreamError>")]
pub struct RequestSignatures {
    pub request_id: String,
    pub tx_hex: String,
    pub input_indices: Vec<usize>,
    pub amounts: Vec<u64>,
    pub tx_type: TransactionType,
}

#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct NotifyPegin {
    pub txid: bitcoin::Txid,
    pub amount: u64,
    pub evm_address: H160,
}

#[derive(Message)]
#[rtype(result = "Result<(), StreamError>")]
pub struct RegisterNode {
    pub node_id: String,
    pub public_key: PublicKey,
    pub capabilities: NodeCapabilities,
}

// Internal messages from governance
#[derive(Message)]
#[rtype(result = "()")]
pub struct SignatureResponse {
    pub request_id: String,
    pub witnesses: Vec<WitnessData>,
    pub status: SignatureStatus,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct FederationUpdate {
    pub version: u32,
    pub members: Vec<FederationMember>,
    pub threshold: usize,
    pub p2wsh_address: bitcoin::Address,
    pub activation_height: Option<u64>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProposalNotification {
    pub proposal_id: String,
    pub proposal_type: ProposalType,
    pub data: serde_json::Value,
    pub voting_deadline: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub endpoint: String,
    pub last_heartbeat: Option<Instant>,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub connection_uptime: Duration,
    pub reconnect_count: u32,
}

#[derive(Debug, Clone)]
pub enum TransactionType {
    Pegout,
    FederationChange,
    Emergency,
}

#[derive(Debug, Clone)]
pub enum SignatureStatus {
    Pending,
    InProgress { collected: usize, required: usize },
    Complete,
    Failed { reason: String },
    Timeout,
}
```

2. **Implement StreamActor Core**
```rust
// src/actors/stream/mod.rs

use actix::prelude::*;
use tonic::transport::{Channel, Endpoint};
use tokio::sync::mpsc;
use std::collections::VecDeque;

pub struct StreamActor {
    // Connection management
    config: StreamConfig,
    endpoint: Option<String>,
    channel: Option<Channel>,
    stream: Option<Streaming<StreamResponse>>,
    sender: Option<mpsc::Sender<StreamRequest>>,
    
    // Connection state
    connection_state: ConnectionState,
    reconnect_strategy: ExponentialBackoff,
    last_heartbeat: Option<Instant>,
    
    // Message handling
    message_buffer: VecDeque<PendingMessage>,
    pending_requests: HashMap<String, PendingRequest>,
    
    // Actor references for routing
    bridge_actor: Option<Addr<BridgeActor>>,
    chain_actor: Option<Addr<ChainActor>>,
    
    // Metrics
    metrics: StreamMetrics,
}

#[derive(Clone)]
pub struct StreamConfig {
    pub governance_endpoint: String,
    pub reconnect_initial_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub reconnect_multiplier: f64,
    pub heartbeat_interval: Duration,
    pub request_timeout: Duration,
    pub max_buffer_size: usize,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32, next_retry: Instant },
    Connected { since: Instant },
    Reconnecting { reason: String, attempt: u32 },
    Failed { reason: String, permanent: bool },
}

struct PendingMessage {
    message: StreamRequest,
    timestamp: Instant,
    retry_count: u32,
}

struct PendingRequest {
    request_type: RequestType,
    timestamp: Instant,
    timeout: Duration,
    callback: Option<oneshot::Sender<Result<StreamResponse, StreamError>>>,
}

impl StreamActor {
    pub fn new(config: StreamConfig) -> Self {
        Self {
            endpoint: Some(config.governance_endpoint.clone()),
            config,
            channel: None,
            stream: None,
            sender: None,
            connection_state: ConnectionState::Disconnected,
            reconnect_strategy: ExponentialBackoff::new(
                config.reconnect_initial_delay,
                config.reconnect_max_delay,
                config.reconnect_multiplier,
            ),
            last_heartbeat: None,
            message_buffer: VecDeque::with_capacity(config.max_buffer_size),
            pending_requests: HashMap::new(),
            bridge_actor: None,
            chain_actor: None,
            metrics: StreamMetrics::new(),
        }
    }
    
    pub fn with_actors(
        mut self,
        bridge_actor: Addr<BridgeActor>,
        chain_actor: Addr<ChainActor>,
    ) -> Self {
        self.bridge_actor = Some(bridge_actor);
        self.chain_actor = Some(chain_actor);
        self
    }
}

impl Actor for StreamActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("StreamActor started, connecting to governance");
        
        // Start connection attempt
        ctx.spawn(
            async move {
                self.establish_connection().await
            }
            .into_actor(self)
        );
        
        // Start heartbeat timer
        ctx.run_interval(self.config.heartbeat_interval, |act, ctx| {
            ctx.spawn(
                async move {
                    act.send_heartbeat().await
                }
                .into_actor(act)
            );
        });
        
        // Start request timeout checker
        ctx.run_interval(Duration::from_secs(5), |act, _| {
            act.check_request_timeouts();
        });
        
        // Start stream reader
        ctx.spawn(
            async move {
                self.read_stream_loop().await
            }
            .into_actor(self)
        );
    }
    
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        info!("StreamActor stopping");
        
        // Close stream gracefully
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(StreamRequest {
                request: Some(governance::stream_request::Request::Disconnect(
                    governance::Disconnect {
                        reason: "Node shutting down".to_string(),
                    }
                )),
            });
        }
        
        Running::Stop
    }
}

impl StreamActor {
    async fn establish_connection(&mut self) -> Result<(), StreamError> {
        let endpoint = self.endpoint.as_ref()
            .ok_or(StreamError::NoEndpoint)?;
        
        info!("Connecting to governance at {}", endpoint);
        
        self.connection_state = ConnectionState::Connecting {
            attempt: self.reconnect_strategy.attempt_count(),
            next_retry: Instant::now(),
        };
        
        // Create gRPC channel
        let channel = Endpoint::from_shared(endpoint.clone())?
            .timeout(Duration::from_secs(30))
            .connect()
            .await
            .map_err(|e| {
                self.metrics.connection_failures.inc();
                StreamError::ConnectionFailed(e.to_string())
            })?;
        
        self.channel = Some(channel.clone());
        
        // Create bidirectional stream
        let mut client = governance::stream_client::StreamClient::new(channel);
        
        let (tx, rx) = mpsc::channel(100);
        let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        
        let response_stream = client
            .bidirectional_stream(request_stream)
            .await
            .map_err(|e| StreamError::StreamCreationFailed(e.to_string()))?
            .into_inner();
        
        self.stream = Some(response_stream);
        self.sender = Some(tx);
        
        // Send initial registration
        self.send_registration().await?;
        
        // Update state
        self.connection_state = ConnectionState::Connected {
            since: Instant::now(),
        };
        
        self.metrics.connections_established.inc();
        self.reconnect_strategy.reset();
        
        // Flush buffered messages
        self.flush_message_buffer().await?;
        
        info!("Successfully connected to governance");
        
        Ok(())
    }
    
    async fn read_stream_loop(&mut self) {
        while let Some(stream) = &mut self.stream {
            match stream.message().await {
                Ok(Some(response)) => {
                    self.metrics.messages_received.inc();
                    if let Err(e) = self.handle_stream_response(response).await {
                        error!("Failed to handle stream response: {}", e);
                    }
                }
                Ok(None) => {
                    // Stream closed by server
                    warn!("Stream closed by governance");
                    self.handle_disconnection("Stream closed by server").await;
                    break;
                }
                Err(e) => {
                    error!("Stream read error: {}", e);
                    self.handle_disconnection(&e.to_string()).await;
                    break;
                }
            }
        }
    }
    
    async fn handle_stream_response(&mut self, response: StreamResponse) -> Result<(), StreamError> {
        use governance::stream_response::Response;
        
        match response.response {
            Some(Response::SignatureResponse(sig_resp)) => {
                self.handle_signature_response(sig_resp).await?;
            }
            Some(Response::FederationUpdate(update)) => {
                self.handle_federation_update(update).await?;
            }
            Some(Response::ProposalNotification(proposal)) => {
                self.handle_proposal_notification(proposal).await?;
            }
            Some(Response::Heartbeat(_)) => {
                self.last_heartbeat = Some(Instant::now());
            }
            Some(Response::Error(error)) => {
                error!("Governance error: {} (code: {})", error.message, error.code);
                self.metrics.governance_errors.inc();
            }
            None => {
                warn!("Received empty response from governance");
            }
        }
        
        Ok(())
    }
    
    async fn handle_signature_response(&mut self, response: governance::SignatureResponse) -> Result<(), StreamError> {
        info!("Received signature response for request {}", response.request_id);
        
        // Convert to internal format
        let witnesses = response.witnesses
            .into_iter()
            .map(|w| WitnessData {
                input_index: w.input_index as usize,
                witness: w.witness_data,
            })
            .collect();
        
        // Send to BridgeActor
        if let Some(bridge) = &self.bridge_actor {
            bridge.send(ApplySignatures {
                request_id: response.request_id.clone(),
                witnesses,
            }).await??;
        }
        
        // Remove from pending
        self.pending_requests.remove(&response.request_id);
        
        self.metrics.signatures_received.inc();
        
        Ok(())
    }
    
    async fn handle_disconnection(&mut self, reason: &str) {
        warn!("Disconnected from governance: {}", reason);
        
        self.connection_state = ConnectionState::Reconnecting {
            reason: reason.to_string(),
            attempt: self.reconnect_strategy.attempt_count(),
        };
        
        self.stream = None;
        self.sender = None;
        self.channel = None;
        
        self.metrics.disconnections.inc();
        
        // Schedule reconnection
        let delay = self.reconnect_strategy.next_delay();
        info!("Reconnecting in {:?}", delay);
        
        tokio::time::sleep(delay).await;
        
        if let Err(e) = self.establish_connection().await {
            error!("Reconnection failed: {}", e);
            
            if self.reconnect_strategy.should_give_up() {
                self.connection_state = ConnectionState::Failed {
                    reason: format!("Max reconnection attempts exceeded: {}", e),
                    permanent: false,
                };
            }
        }
    }
    
    async fn send_heartbeat(&mut self) -> Result<(), StreamError> {
        if let Some(sender) = &self.sender {
            let heartbeat = StreamRequest {
                request: Some(governance::stream_request::Request::Heartbeat(
                    governance::Heartbeat {
                        timestamp: Utc::now().timestamp(),
                        node_id: self.config.node_id.clone(),
                    }
                )),
            };
            
            sender.send(heartbeat).await
                .map_err(|e| StreamError::SendFailed(e.to_string()))?;
        }
        
        Ok(())
    }
    
    async fn flush_message_buffer(&mut self) -> Result<(), StreamError> {
        while let Some(pending) = self.message_buffer.pop_front() {
            if let Some(sender) = &self.sender {
                sender.send(pending.message).await
                    .map_err(|e| StreamError::SendFailed(e.to_string()))?;
                
                self.metrics.buffered_messages_sent.inc();
            }
        }
        
        Ok(())
    }
}

impl Handler<RequestSignatures> for StreamActor {
    type Result = ResponseActFuture<Self, Result<String, StreamError>>;
    
    fn handle(&mut self, msg: RequestSignatures, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let request = StreamRequest {
                request: Some(governance::stream_request::Request::SignatureRequest(
                    governance::SignatureRequest {
                        request_id: msg.request_id.clone(),
                        chain: "alys".to_string(),
                        tx_hex: msg.tx_hex,
                        input_indices: msg.input_indices.into_iter().map(|i| i as u32).collect(),
                        amounts: msg.amounts,
                        tx_type: match msg.tx_type {
                            TransactionType::Pegout => governance::TxType::Pegout as i32,
                            TransactionType::FederationChange => governance::TxType::FederationChange as i32,
                            TransactionType::Emergency => governance::TxType::Emergency as i32,
                        },
                    }
                )),
            };
            
            if let Some(sender) = &self.sender {
                sender.send(request).await
                    .map_err(|e| StreamError::SendFailed(e.to_string()))?;
                
                // Track pending request
                self.pending_requests.insert(msg.request_id.clone(), PendingRequest {
                    request_type: RequestType::Signature,
                    timestamp: Instant::now(),
                    timeout: self.config.request_timeout,
                    callback: None,
                });
                
                self.metrics.signature_requests.inc();
                
                Ok(msg.request_id)
            } else {
                // Buffer if disconnected
                self.message_buffer.push_back(PendingMessage {
                    message: request,
                    timestamp: Instant::now(),
                    retry_count: 0,
                });
                
                Err(StreamError::NotConnected)
            }
        }.into_actor(self))
    }
}
```

3. **Implement Reconnection Strategy**
```rust
// src/actors/stream/reconnect.rs

pub struct ExponentialBackoff {
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
    attempt_count: u32,
    max_attempts: Option<u32>,
}

impl ExponentialBackoff {
    pub fn new(initial: Duration, max: Duration, multiplier: f64) -> Self {
        Self {
            initial_delay: initial,
            max_delay: max,
            multiplier,
            attempt_count: 0,
            max_attempts: Some(100),
        }
    }
    
    pub fn next_delay(&mut self) -> Duration {
        self.attempt_count += 1;
        
        let delay_ms = self.initial_delay.as_millis() as f64
            * self.multiplier.powi(self.attempt_count.saturating_sub(1) as i32);
        
        let delay_ms = delay_ms.min(self.max_delay.as_millis() as f64);
        
        // Add jitter (±10%)
        let jitter = delay_ms * 0.1 * (rand::random::<f64>() - 0.5) * 2.0;
        let final_delay = (delay_ms + jitter).max(0.0) as u64;
        
        Duration::from_millis(final_delay)
    }
    
    pub fn reset(&mut self) {
        self.attempt_count = 0;
    }
    
    pub fn should_give_up(&self) -> bool {
        if let Some(max) = self.max_attempts {
            self.attempt_count >= max
        } else {
            false
        }
    }
    
    pub fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
}
```

## Testing Plan

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[actix::test]
    async fn test_connection_establishment() {
        let stream = StreamActor::new(test_config());
        let addr = stream.start();
        
        addr.send(EstablishConnection {
            endpoint: "http://localhost:50051".to_string(),
            auth_token: None,
            chain_id: "alys-test".to_string(),
        }).await.unwrap().unwrap();
        
        let status = addr.send(GetConnectionStatus).await.unwrap().unwrap();
        assert!(status.connected);
    }
    
    #[actix::test]
    async fn test_message_buffering() {
        let mut stream = StreamActor::new(test_config());
        
        // Simulate disconnection
        stream.connection_state = ConnectionState::Disconnected;
        
        // Send messages while disconnected
        for i in 0..10 {
            stream.message_buffer.push_back(PendingMessage {
                message: create_test_message(i),
                timestamp: Instant::now(),
                retry_count: 0,
            });
        }
        
        assert_eq!(stream.message_buffer.len(), 10);
        
        // Simulate reconnection
        stream.flush_message_buffer().await.unwrap();
        
        assert_eq!(stream.message_buffer.len(), 0);
    }
    
    #[tokio::test]
    async fn test_exponential_backoff() {
        let mut backoff = ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(60),
            2.0,
        );
        
        let delay1 = backoff.next_delay();
        let delay2 = backoff.next_delay();
        let delay3 = backoff.next_delay();
        
        assert!(delay1 < delay2);
        assert!(delay2 < delay3);
        assert!(delay3 <= Duration::from_secs(60));
    }
    
    #[actix::test]
    async fn test_signature_request_routing() {
        let bridge = create_mock_bridge_actor();
        let stream = StreamActor::new(test_config())
            .with_actors(bridge.clone(), create_mock_chain_actor());
        
        let addr = stream.start();
        
        // Send signature request
        let request_id = addr.send(RequestSignatures {
            request_id: "test-123".to_string(),
            tx_hex: "0x1234".to_string(),
            input_indices: vec![0],
            amounts: vec![100000000],
            tx_type: TransactionType::Pegout,
        }).await.unwrap().unwrap();
        
        assert_eq!(request_id, "test-123");
    }
}
```

### Integration Tests
1. Test with mock governance server
2. Test disconnection and reconnection
3. Test message ordering preservation
4. Test timeout handling
5. Test federation update propagation

### Performance Tests
```rust
#[bench]
fn bench_message_throughput(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stream = runtime.block_on(create_connected_stream_actor());
    
    b.iter(|| {
        runtime.block_on(async {
            for _ in 0..1000 {
                stream.send(create_test_message()).await.unwrap();
            }
        })
    });
}
```

## Dependencies

### Blockers
- ALYS-009: BridgeActor for signature application

### Related Issues
- ALYS-013: Governance signature collection
- ALYS-014: Federation management
- ALYS-015: P2WSH implementation

## Definition of Done

- [ ] StreamActor fully implemented
- [ ] Bi-directional streaming working
- [ ] Reconnection logic tested
- [ ] Message buffering operational
- [ ] Integration with BridgeActor complete
- [ ] Health monitoring implemented
- [ ] All tests passing
- [ ] Documentation complete
- [ ] Code review completed

## Notes

- Add support for multiple governance endpoints
- Implement circuit breaker pattern