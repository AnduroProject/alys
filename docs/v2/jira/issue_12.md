# ALYS-012: Implement StreamActor for Governance Communication

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

## Subtasks

### Phase 1: Foundation & Protocol Design (Story Points: 1)

#### **ALYS-012-1**: Design Stream Protocol and Define Message Types (TDD) [https://marathondh.atlassian.net/browse/AN-450]

* **Objective**: Define comprehensive gRPC protocol and Rust message types for governance communication
* **Test-First Approach**:
  - [ ] Write tests for message serialization/deserialization
  - [ ] Write tests for protocol buffer validation
  - [ ] Write tests for message type conversions
  - [ ] Write tests for error handling in message parsing
* **Implementation**:
  - [ ] Create `governance.proto` file with complete service definition
  - [ ] Generate Rust bindings with `tonic-build`
  - [ ] Implement Rust message types in `src/actors/stream/messages.rs`
  - [ ] Create conversion traits between proto and internal types
  - [ ] Add comprehensive error types for stream operations
* **DoD**: All message types compile, serialize correctly, and pass property-based tests

#### **ALYS-012-2**: Implement Exponential Backoff Reconnection Strategy (TDD) [https://marathondh.atlassian.net/browse/AN-451]

* **Objective**: Create robust reconnection logic with exponential backoff and jitter
* **Test-First Approach**:
  - [ ] Write tests for backoff delay calculation
  - [ ] Write tests for jitter randomization
  - [ ] Write tests for max attempts handling
  - [ ] Write tests for backoff reset functionality
* **Implementation**:
  - [ ] Create `src/actors/stream/reconnect.rs` module
  - [ ] Implement `ExponentialBackoff` struct with configurable parameters
  - [ ] Add jitter to prevent thundering herd
  - [ ] Implement circuit breaker pattern for permanent failures
  - [ ] Add metrics for reconnection attempts and success rates
* **DoD**: Reconnection strategy tested with statistical validation of delay distribution

### Phase 2: Core Actor Implementation (Story Points: 3)

#### **ALYS-012-3**: Implement StreamActor Core Structure (TDD) [https://marathondh.atlassian.net/browse/AN-452]

* **Objective**: Create the main StreamActor with state management and lifecycle
* **Test-First Approach**:
  - [ ] Write tests for actor initialization
  - [ ] Write tests for state transitions
  - [ ] Write tests for configuration validation
  - [ ] Write tests for actor lifecycle (start/stop)
* **Implementation**:
  - [ ] Create `src/actors/stream/mod.rs` with StreamActor struct
  - [ ] Implement connection state machine
  - [ ] Add configuration management
  - [ ] Implement actor lifecycle methods (started/stopping)
  - [ ] Add metrics collection infrastructure
* **DoD**: StreamActor can be instantiated, configured, and transitions through states correctly

#### **ALYS-012-4**: Implement gRPC Connection Management (TDD) [https://marathondh.atlassian.net/browse/AN-453]

* **Objective**: Handle gRPC channel creation, stream establishment, and connection health
* **Test-First Approach**:
  - [ ] Write tests for channel creation with various endpoints
  - [ ] Write tests for stream establishment success/failure scenarios
  - [ ] Write tests for connection timeout handling
  - [ ] Write tests for authentication token management
* **Implementation**:
  - [ ] Implement `establish_connection()` method
  - [ ] Create bidirectional gRPC stream
  - [ ] Handle authentication and authorization
  - [ ] Implement connection health checks
  - [ ] Add TLS support for production deployment
* **DoD**: Can establish secure gRPC connections with proper error handling and timeout management

#### **ALYS-012-5**: Implement Message Buffering System (TDD) [https://marathondh.atlassian.net/browse/AN-454]

* **Objective**: Buffer messages during disconnections and replay on reconnection
* **Test-First Approach**:
  - [ ] Write tests for message buffering during disconnection
  - [ ] Write tests for buffer overflow handling
  - [ ] Write tests for message ordering preservation
  - [ ] Write tests for buffer persistence across actor restarts
* **Implementation**:
  - [ ] Implement `VecDeque`-based message buffer
  - [ ] Add configurable buffer size limits
  - [ ] Implement message prioritization (signatures > heartbeats)
  - [ ] Add buffer persistence for critical messages
  - [ ] Implement message deduplication
* **DoD**: Messages are reliably buffered and replayed with correct ordering and no duplicates

### Phase 3: Message Handling & Routing (Story Points: 2)

#### **ALYS-012-6**: Implement Outbound Message Handlers (TDD) [https://marathondh.atlassian.net/browse/AN-456]

* **Objective**: Handle signature requests, peg-in notifications, and node registration
* **Test-First Approach**:
  - [ ] Write tests for `RequestSignatures` message handling
  - [ ] Write tests for `NotifyPegin` message processing
  - [ ] Write tests for `RegisterNode` functionality
  - [ ] Write tests for message timeout and retry logic
* **Implementation**:
  - [ ] Implement `Handler<RequestSignatures>` with proper error handling
  - [ ] Implement `Handler<NotifyPegin>` with validation
  - [ ] Implement `Handler<RegisterNode>` with capabilities reporting
  - [ ] Add request tracking with unique IDs
  - [ ] Implement timeout and retry mechanisms
* **DoD**: All outbound message types are handled correctly with comprehensive error handling

#### **ALYS-012-7**: Implement Inbound Message Processing (TDD) [https://marathondh.atlassian.net/browse/AN-459]

* **Objective**: Process responses from governance including signatures and federation updates
* **Test-First Approach**:
  - [ ] Write tests for signature response processing
  - [ ] Write tests for federation update handling
  - [ ] Write tests for proposal notification processing
  - [ ] Write tests for error response handling
* **Implementation**:
  - [ ] Implement `handle_signature_response()` with witness data conversion
  - [ ] Implement `handle_federation_update()` with validation
  - [ ] Implement `handle_proposal_notification()` with routing
  - [ ] Add proper error handling for malformed responses
  - [ ] Implement heartbeat processing for connection health
* **DoD**: All inbound message types are processed correctly with proper validation and error handling

#### **ALYS-012-8**: Implement Actor Integration & Routing (TDD) [https://marathondh.atlassian.net/browse/AN-460]

* **Objective**: Integrate with BridgeActor and ChainActor for message routing
* **Test-First Approach**:
  - [ ] Write tests for BridgeActor signature routing
  - [ ] Write tests for ChainActor federation update routing
  - [ ] Write tests for actor reference management
  - [ ] Write tests for routing failure recovery
* **Implementation**:
  - [ ] Add actor reference management in StreamActor
  - [ ] Implement signature routing to BridgeActor
  - [ ] Implement federation update routing to ChainActor
  - [ ] Add fallback handling for unavailable actors
  - [ ] Implement request-response correlation
* **DoD**: Messages are correctly routed to appropriate actors with proper error handling

### Phase 4: Health Monitoring & Observability (Story Points: 1)

#### **ALYS-012-9**: Implement Health Monitoring and Status Reporting (TDD) [https://marathondh.atlassian.net/browse/AN-461]

* **Objective**: Comprehensive health monitoring with metrics and status reporting
* **Test-First Approach**:
  - [ ] Write tests for connection status reporting
  - [ ] Write tests for health check functionality
  - [ ] Write tests for metrics collection accuracy
  - [ ] Write tests for status change notifications
* **Implementation**:
  - [ ] Implement `GetConnectionStatus` message handler
  - [ ] Add comprehensive metrics collection (Prometheus)
  - [ ] Implement heartbeat monitoring
  - [ ] Add connection uptime tracking
  - [ ] Create health status enumeration with detailed states
* **DoD**: Complete observability with accurate metrics and detailed status reporting

#### **ALYS-012-10**: Implement Request Timeout and Cleanup (TDD) [https://marathondh.atlassian.net/browse/AN-462]

* **Objective**: Manage request lifecycles with timeout handling and resource cleanup
* **Test-First Approach**:
  - [ ] Write tests for request timeout detection
  - [ ] Write tests for pending request cleanup
  - [ ] Write tests for timeout callback handling
  - [ ] Write tests for resource leak prevention
* **Implementation**:
  - [ ] Implement periodic timeout checking
  - [ ] Add request cleanup on timeout
  - [ ] Implement callback notification for timeouts
  - [ ] Add resource leak detection and prevention
  - [ ] Create configurable timeout policies per request type
* **DoD**: No resource leaks, reliable timeout handling, and proper cleanup of expired requests

### Phase 5: Integration & Error Handling (Story Points: 1)

#### **ALYS-012-11**: Implement Comprehensive Error Handling and Recovery (TDD) [https://marathondh.atlassian.net/browse/AN-463]

* **Objective**: Robust error handling with automatic recovery for all failure scenarios
* **Test-First Approach**:
  - [ ] Write tests for network failure scenarios
  - [ ] Write tests for governance service unavailability
  - [ ] Write tests for malformed message handling
  - [ ] Write tests for partial failure recovery
* **Implementation**:
  - [ ] Implement comprehensive `StreamError` enum
  - [ ] Add automatic error recovery strategies
  - [ ] Implement graceful degradation for non-critical failures
  - [ ] Add error reporting and alerting
  - [ ] Create failure analysis and debugging tools
* **DoD**: All error scenarios are handled gracefully with appropriate recovery strategies

#### **ALYS-012-12**: End-to-End Integration Testing and Optimization (TDD) [https://marathondh.atlassian.net/browse/AN-464]

* **Objective**: Complete integration testing with performance optimization
* **Test-First Approach**:
  - [ ] Write integration tests with mock governance server
  - [ ] Write tests for message ordering under high load
  - [ ] Write tests for reconnection scenarios with real network conditions
  - [ ] Write performance benchmarks for message throughput
* **Implementation**:
  - [ ] Create comprehensive integration test suite
  - [ ] Implement mock governance server for testing
  - [ ] Add performance benchmarking and optimization
  - [ ] Implement load testing scenarios
  - [ ] Add chaos engineering tests for resilience validation
* **DoD**: All integration tests pass, performance targets met, and system is production-ready

### Technical Implementation Guidelines

#### Test-Driven Development Approach

1. **Red Phase**: Write failing tests that define expected behavior
2. **Green Phase**: Implement minimal code to make tests pass
3. **Refactor Phase**: Clean up code while maintaining test coverage

#### Testing Strategy

* **Unit Tests**: >95% coverage for all StreamActor components
* **Integration Tests**: End-to-end scenarios with mock governance
* **Property-Based Tests**: Message serialization and protocol correctness
* **Performance Tests**: Throughput and latency benchmarks
* **Chaos Tests**: Network partitions and service failures

#### Code Quality Standards

* **Static Analysis**: Clippy warnings addressed
* **Security Review**: No secrets in logs, secure gRPC communication
* **Documentation**: Comprehensive API docs and usage examples
* **Error Handling**: Graceful degradation and clear error messages

#### Deployment Strategy

* **Feature Flags**: Safe rollout with configuration-based enabling
* **Metrics**: Comprehensive monitoring with alerts
* **Health Checks**: Kubernetes-ready health endpoints
* **Circuit Breakers**: Protection against cascade failures

#### Risk Mitigation

* **Network Partitions**: Robust reconnection with exponential backoff
* **Message Ordering**: Guaranteed delivery order for critical messages
* **Memory Management**: Bounded buffers and resource cleanup
* **Security**: Mutual TLS and token-based authentication

## Notes

- Add support for multiple governance endpoints
- Implement circuit breaker pattern