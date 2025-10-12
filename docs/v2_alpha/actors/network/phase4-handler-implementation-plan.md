# Phase 4 NetworkActor Message Handler Implementation - Complete Todo List

## Context Analysis

**Current State:**
- NetworkActor has working foundation: libp2p behaviour, PeerManager, Gossipsub topics
- Block broadcasting is implemented via `broadcast_message()` using gossipsub
- Request-response protocol exists but not fully integrated
- PeerManager tracks connected peers with reputation system
- Two handlers have placeholder implementations: `BroadcastAuxPow` and `RequestBlocks`

**Architecture:**
- Gossipsub for broadcasts (blocks, transactions, AuxPoW)
- Request-Response protocol for direct peer queries (GetBlocks)
- PeerManager for peer selection and reputation
- SyncActor for handling received blocks
- Metrics tracking for all network operations

---

## 1. BroadcastAuxPow Handler Implementation

### 1.1 Gossip Topic Definition
**Task**: Add AuxPoW gossip topic to existing topic enumeration
- Add `AuxPow` variant to `GossipTopic` enum in `protocols/gossip.rs`
- Implement `to_topic()` → `"alys-auxpow"`
- Implement `as_str()` → `"alys-auxpow"`
- Implement `from_str()` → `Some(GossipTopic::AuxPow)`

### 1.2 Topic Subscription
**Task**: Subscribe to AuxPoW topic during network initialization
- Add `"alys-auxpow"` to default `gossip_topics` in `NetworkConfig::default()`
- Ensure `AlysNetworkBehaviour::initialize()` subscribes to topic
- Verify subscription in `AlysNetworkBehaviour::subscribe_to_topic()`

### 1.3 Serialization Validation
**Task**: Validate AuxPoW data format before broadcasting
- Add deserialization check: `serde_json::from_slice::<AuxPowHeader>(&auxpow_data)`
- Return `NetworkError::Protocol("Invalid AuxPoW format")` if invalid
- Log serialization errors with correlation_id

### 1.4 Broadcast Implementation
**Task**: Use existing `broadcast_message()` infrastructure
- Call `self.broadcast_message("alys-auxpow", auxpow_data, false)`
- Handle `Err` case: return `NetworkError::Protocol`
- Handle `Ok` case: extract message_id, count peers, return response

### 1.5 Metrics Integration
**Task**: Add AuxPoW-specific metrics
- Add `auxpow_broadcasts` counter to `NetworkMetrics`
- Add `auxpow_broadcast_bytes` histogram to `NetworkMetrics`
- Record metrics in handler: `self.metrics.record_auxpow_broadcast(data_len)`
- Update Prometheus registry

### 1.6 Peer Selection Logic
**Task**: Ensure AuxPoW reaches mining peers
- Check `self.peer_manager.get_connected_peers().len()` before broadcasting
- Return `NetworkError::NoPeers` if no peers connected
- Log warning if peer_count < 3 (insufficient mining network)

### 1.7 Error Handling
**Task**: Comprehensive error handling with correlation tracking
- Network not running: Return `NetworkError::NotStarted` with correlation_id log
- Behaviour unavailable: Return `NetworkError::Internal("Behaviour not initialized")`
- Broadcast failure: Return `NetworkError::Protocol` with error details
- Add timeout handling (5 second timeout for gossip propagation)

### 1.8 Testing
**Task**: Unit and integration tests
- Unit test: Handler returns correct response structure
- Unit test: Validates invalid AuxPoW data format
- Unit test: Returns error when network not running
- Integration test: AuxPoW reaches subscribed peers
- Integration test: Metrics updated correctly

---

## 2. RequestBlocks Handler Implementation

### 2.1 Request-Response Protocol Enhancement
**Task**: Extend existing request-response protocol for block requests
- Check if `NetworkRequest::GetBlocks` already exists (it does)
- Ensure request-response protocol in `AlysNetworkBehaviour` handles GetBlocks
- Add `send_request()` wrapper in `AlysNetworkBehaviour` for GetBlocks specifically

### 2.2 Peer Selection Strategy
**Task**: Select best peers for block requests based on reputation
- Add `PeerManager::select_peers_for_blocks(&self, count: usize) -> Vec<PeerId>`
- Selection criteria: reputation > 50.0, success_rate > 0.7, recently active
- Sort by reputation descending, take top N peers
- Return error if no suitable peers found

### 2.3 Request Tracking System
**Task**: Track pending block requests for response correlation
- Create `BlockRequest` struct with `request_id`, `peer_ids`, `start_height`, `count`, `timestamp`
- Add `pending_block_requests: HashMap<Uuid, BlockRequest>` to `NetworkActor`
- Store request in map before sending: `self.pending_block_requests.insert(request_id, request)`
- Add cleanup mechanism: Remove requests older than 60 seconds

### 2.4 Multi-Peer Request Distribution
**Task**: Send requests to multiple peers for redundancy and speed
- Select 3-5 peers using peer selection strategy
- Send request to each peer via `behaviour.send_request(peer_id, NetworkRequest::GetBlocks { ... })`
- Track which peers received request
- Return `BlocksRequested` with `peer_count` and `request_id`

### 2.5 Request Timeout Handling
**Task**: Handle timeout for block requests
- Add `REQUEST_TIMEOUT: Duration = Duration::from_secs(30)`
- Store request timestamp when sending
- Add periodic cleanup task to check for timed-out requests
- Log timeout warnings with correlation_id

### 2.6 Response Handling Integration
**Task**: Handle block responses from peers
- Add `NetworkMessage::HandleBlockResponse { blocks, request_id, peer_id }` handler
- Look up pending request: `self.pending_block_requests.get(&request_id)`
- Validate blocks: check height range, verify count
- Update peer reputation based on response quality
- Forward blocks to SyncActor: `sync_actor.send(SyncMessage::HandleBlockResponse { ... })`
- Remove request from pending map

### 2.7 Error Handling
**Task**: Comprehensive error handling with peer reputation updates
- No peers available: Return `NetworkError::NoPeers`, don't penalize peers
- Request send failure: Return `NetworkError::Protocol`, penalize specific peer
- Invalid height range: Return `NetworkError::Protocol("Invalid block range")`
- Network not running: Return `NetworkError::NotStarted`
- All peers failed: Return aggregated error, log all failures

### 2.8 Metrics Integration
**Task**: Add block request metrics
- Add `block_requests_sent` counter to `NetworkMetrics`
- Add `block_request_latency` histogram to `NetworkMetrics`
- Add `block_responses_received` counter to `NetworkMetrics`
- Record metrics in handler and response handler
- Track average peer response time

### 2.9 Rate Limiting
**Task**: Prevent spam and DoS via excessive block requests
- Add `max_concurrent_block_requests: usize = 10` to `NetworkActor`
- Check `self.pending_block_requests.len() < max_concurrent` before sending
- Return `NetworkError::Internal("Too many pending requests")` if limit exceeded
- Log rate limit events

### 2.10 Testing
**Task**: Comprehensive test coverage
- Unit test: Peer selection with various reputation scenarios
- Unit test: Request tracking and cleanup
- Unit test: Rate limiting enforcement
- Integration test: Multi-peer request distribution
- Integration test: Response handling and forwarding to SyncActor
- Integration test: Timeout handling and request cleanup
- Integration test: Peer reputation updates on success/failure

---

## 3. Supporting Infrastructure

### 3.1 SyncActor Integration
**Task**: Ensure SyncActor can receive and process block responses
- Verify `SyncMessage::HandleBlockResponse` exists and is implemented
- Add validation in SyncActor: check blocks match requested range
- Add duplicate block detection in SyncActor
- Forward valid blocks to ChainActor for import

### 3.2 AlysNetworkBehaviour Enhancements
**Task**: Complete libp2p behaviour implementation
- Implement actual gossipsub broadcasting (currently TODO)
- Implement actual request-response sending (currently TODO)
- Add response handling callback from libp2p events
- Integrate with NetworkActor event loop

### 3.3 Metrics Dashboard
**Task**: Add Phase 4 metrics to monitoring
- Add AuxPoW broadcast metrics to Prometheus
- Add block request/response metrics to Prometheus
- Create Grafana dashboard for Phase 4 network operations
- Add alerting for high failure rates or timeouts

### 3.4 Configuration
**Task**: Add Phase 4 configuration options
- Add `enable_auxpow_broadcast: bool` to `NetworkConfig`
- Add `max_block_request_count: u32 = 100` to `NetworkConfig`
- Add `block_request_timeout: Duration` to `NetworkConfig`
- Add `min_peers_for_requests: usize = 3` to `NetworkConfig`

### 3.5 Documentation
**Task**: Document Phase 4 network message flows
- Document AuxPoW broadcast flow: ChainActor → NetworkActor → Gossipsub → Peers
- Document block request flow: Requester → NetworkActor → Request-Response → Peer → Response
- Add sequence diagrams for both flows
- Document error scenarios and recovery procedures

---

## 4. Integration & Testing

### 4.1 End-to-End AuxPoW Flow
**Task**: Test complete AuxPoW mining coordination
- ChainActor calls `NetworkActor::BroadcastAuxPow`
- NetworkActor broadcasts to all mining peers
- Mining peers receive AuxPoW via gossipsub
- Mining peers submit completed work back
- Verify timing and propagation metrics

### 4.2 End-to-End Block Sync Flow
**Task**: Test complete block synchronization
- SyncActor detects missing blocks (height gap)
- SyncActor requests blocks via `NetworkActor::RequestBlocks`
- NetworkActor sends requests to 3-5 peers
- Peers respond with blocks
- NetworkActor forwards responses to SyncActor
- SyncActor validates and forwards to ChainActor for import
- Verify sync completes successfully

### 4.3 Failure Scenario Testing
**Task**: Test error handling and recovery
- Test: All peers timeout → request should fail gracefully
- Test: Invalid AuxPoW format → should reject with clear error
- Test: No peers connected → should return NoPeers error
- Test: Rate limiting → should enforce limits correctly
- Test: Peer sends invalid blocks → should penalize reputation

### 4.4 Performance Testing
**Task**: Validate performance under load
- Test: 100 AuxPoW broadcasts/minute → measure propagation time
- Test: 50 concurrent block requests → verify no deadlocks
- Test: Large block responses (100 blocks) → measure throughput
- Test: 100+ connected peers → verify scalability

---

## 5. Production Readiness

### 5.1 Monitoring
**Task**: Add production monitoring and alerting
- Alert: AuxPoW broadcast failure rate > 5%
- Alert: Block request timeout rate > 10%
- Alert: Average peer response time > 5 seconds
- Alert: No peers connected for > 1 minute
- Dashboard: Real-time network health visualization

### 5.2 Logging
**Task**: Production-quality structured logging
- Log all AuxPoW broadcasts with correlation_id, peer_count, data_size
- Log all block requests with correlation_id, peer_ids, height_range
- Log all responses with latency, peer_id, block_count
- Log all errors with full context for debugging
- Use appropriate log levels (debug/info/warn/error)

### 5.3 Security
**Task**: Secure network operations
- Validate AuxPoW data size limits (prevent DoS)
- Validate block request ranges (prevent spam)
- Add peer reputation penalties for malicious behavior
- Rate limit requests per peer
- Add request authentication (future enhancement)

### 5.4 Final Integration Testing
**Task**: Test with V0 components
- Test AuxPoW broadcast integration with existing V0 mining infrastructure
- Test block sync with V0 storage layer
- Verify no regressions in existing functionality
- Performance comparison: V2 vs V0 network operations

---

## Summary

**Total Tasks**: 38 discrete implementation tasks across 5 major areas

**Critical Path**:
1. Complete `AlysNetworkBehaviour` libp2p integration (currently has TODOs)
2. Implement `BroadcastAuxPow` handler (simpler, builds on existing broadcast)
3. Implement `RequestBlocks` handler (more complex, requires request tracking)
4. Add metrics and monitoring
5. Integration testing with SyncActor and ChainActor

**Estimated Complexity**:
- **BroadcastAuxPow**: Medium (builds on existing gossipsub infrastructure)
- **RequestBlocks**: High (requires request-response protocol, peer selection, tracking, timeouts)
- **Supporting Infrastructure**: Medium-High (AlysNetworkBehaviour TODOs, metrics, testing)

**Dependencies**:
- `AlysNetworkBehaviour::broadcast_message()` must implement actual libp2p gossipsub
- `AlysNetworkBehaviour::send_request()` must implement actual libp2p request-response
- SyncActor must be ready to receive block responses
- Metrics system must support new counters/histograms

---

## Implementation Priority

### Phase 1: Foundation (Days 1-2)
1. Complete `AlysNetworkBehaviour` libp2p gossipsub implementation
2. Complete `AlysNetworkBehaviour` request-response implementation
3. Add AuxPoW gossip topic definition
4. Add peer selection strategy to PeerManager

### Phase 2: BroadcastAuxPow (Days 3-4)
1. Implement handler with validation and broadcasting
2. Add metrics integration
3. Add error handling
4. Write unit tests
5. Integration testing

### Phase 3: RequestBlocks Foundation (Days 5-7)
1. Implement request tracking system
2. Implement peer selection for requests
3. Implement multi-peer request distribution
4. Add timeout handling
5. Write unit tests

### Phase 4: RequestBlocks Integration (Days 8-9)
1. Implement response handling
2. Integrate with SyncActor
3. Add metrics integration
4. Add rate limiting
5. Integration testing

### Phase 5: Production Readiness (Days 10-12)
1. End-to-end testing (AuxPoW and block sync flows)
2. Failure scenario testing
3. Performance testing
4. Monitoring and alerting setup
5. Documentation completion
6. Final integration testing with V0 components
