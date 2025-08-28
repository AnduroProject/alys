# Detailed Implementation Plan: Create Engine Actor Module Directory

## Current State Analysis

- Current engine logic is spread across multiple files:
  - `engine_actor.rs` (373 lines) - Basic actor implementation with placeholder logic
  - `engine.rs` (375 lines) - Core execution engine implementation with Geth/Reth integration
  - Engine functionality is embedded within the main consensus layer

## Proposed Directory Structure

```
app/src/actors/engine/
├── mod.rs                    # Module exports and public interface
├── actor.rs                  # Core EngineActor implementation (moved from engine_actor.rs)
├── config.rs                 # Configuration structures and defaults
├── state.rs                  # Engine state and execution tracking
├── messages.rs               # Engine-specific message definitions
├── handlers/                 # Message handler implementations
│   ├── mod.rs
│   ├── payload_handlers.rs   # Payload building and execution handlers
│   ├── forkchoice_handlers.rs# Forkchoice update handlers
│   ├── sync_handlers.rs      # Engine sync status handlers
│   └── client_handlers.rs    # Execution client management handlers
├── client.rs                 # Execution client abstraction (Geth/Reth)
├── engine.rs                 # Core engine logic (moved from engine.rs)
├── metrics.rs                # Engine-specific metrics and performance tracking
├── validation.rs             # Payload and execution validation logic
├── supervision.rs            # Engine supervision strategies
└── tests/                    # Test organization
    ├── mod.rs
    ├── unit_tests.rs         # Core unit tests
    ├── integration_tests.rs  # Integration tests with execution clients
    ├── performance_tests.rs  # Performance benchmarks
    ├── chaos_tests.rs        # Fault injection and resilience tests
    └── mock_helpers.rs       # Test utilities and mocks
```

## Implementation Steps

### Phase 1: Directory Setup and Core Structure

1. **Create base directory structure:**
   - Create `app/src/actors/engine/` directory
   - Create all subdirectories (`handlers/`, `tests/`)
   - Create empty stub files for each module

2. **Create module interface (mod.rs):**
   - Define public exports for the engine module
   - Re-export core types and traits
   - Maintain backward compatibility with existing imports

3. **Extract configuration (config.rs):**
   - Move `EngineConfig` from engine_actor.rs
   - Add environment-specific configuration loading
   - Include JWT authentication, timeouts, and URL configurations
   - Add support for multiple execution client types (Geth/Reth)

### Phase 2: Core Implementation Migration

4. **Extract state management (state.rs):**
   - Move `ExecutionState`, `PayloadStatus` from engine_actor.rs
   - Add comprehensive execution state tracking
   - Include sync status, health monitoring, and error tracking
   - Add state serialization for persistence across restarts

5. **Extract core actor (actor.rs):**
   - Move main `EngineActor` struct and core implementation
   - Move `Actor` trait implementations
   - Keep startup/shutdown logic and periodic tasks
   - Add proper async/await handling for engine operations

6. **Create message definitions (messages.rs):**
   - Define all engine-specific message types
   - Include correlation IDs and tracing support
   - Add message validation and serialization
   - Support for Engine API messages (forkchoiceUpdated, newPayload, etc.)
   - Add inter-actor message types for ChainActor, BridgeActor, StorageActor integration

### Phase 3: Client Abstraction and Engine Logic

7. **Create execution client abstraction (client.rs):**
   - Abstract `ExecutionClient`, `EngineApiClient`, `PublicApiClient` types
   - Support multiple execution client implementations
   - Handle authentication, connection management, and failover
   - Include health checks and connection pooling

8. **Extract engine logic (engine.rs):**
   - Move core `Engine` struct and implementation from main engine.rs
   - Preserve all existing functionality (build_block, commit_block, etc.)
   - Add proper error handling and retry logic
   - Include performance optimizations and caching

### Phase 4: Handler Organization

9. **Create handler modules:**
   - `payload_handlers.rs`: Build and execute payload operations
   - `forkchoice_handlers.rs`: Forkchoice update and finalization
   - `sync_handlers.rs`: Engine synchronization status
   - `client_handlers.rs`: Client lifecycle and health management

10. **Implement message handlers:**
    - Extract relevant handlers from engine_actor.rs
    - Add comprehensive error handling and recovery
    - Include proper async handling and timeout management
    - Add message correlation and distributed tracing

### Phase 5: Supporting Modules

11. **Create metrics module (metrics.rs):**
    - Extract `EngineActorMetrics` and related structures
    - Add Prometheus integration for monitoring
    - Include performance dashboards configuration
    - Track payload building times, execution latency, error rates

12. **Create validation module (validation.rs):**
    - Add payload validation logic
    - Include execution result verification
    - Add block hash validation and consistency checks
    - Include gas limit and fee validation

13. **Create supervision module (supervision.rs):**
    - Add engine-specific supervision policies
    - Include restart strategies for failed execution clients
    - Add circuit breaker patterns for unhealthy clients
    - Include escalation policies for critical failures

### Phase 6: Testing Infrastructure

14. **Reorganize tests:**
    - Create comprehensive unit test suite
    - Add integration tests with real Geth/Reth instances
    - Include performance benchmarks for critical paths
    - Add chaos engineering tests for fault tolerance

15. **Add specialized test utilities:**
    - Mock execution clients for unit testing
    - Test fixtures for common payload scenarios
    - Performance test harnesses
    - Integration test orchestration tools

### Phase 7: Actor Integration and Advanced Features

16. **Implement actor integration patterns:**
    - Add message handlers for inter-actor communication
    - Implement ChainActor ↔ EngineActor message flows
    - Add BridgeActor integration for peg-out burn event detection
    - Include NetworkActor integration for transaction forwarding
    - Add StorageActor integration for execution data persistence

17. **Add advanced features:**
    - Payload caching and optimization
    - Connection pooling for multiple execution clients
    - Load balancing between multiple client instances
    - Engine API version compatibility handling

18. **Update imports throughout codebase:**
    - Update `app/src/actors/mod.rs` to use new module structure
    - Update all references to engine components
    - Ensure backward compatibility where needed
    - Update documentation and examples

19. **Cleanup and optimization:**
    - Remove original engine_actor.rs
    - Optimize performance critical paths
    - Add comprehensive documentation
    - Run integration tests to ensure no regressions

## Key Design Considerations

### Performance Requirements
- Payload building: < 100ms average latency
- Payload execution: < 200ms average latency
- Client health checks: < 5s intervals
- Error recovery: < 10s maximum downtime

### Reliability Features
- Automatic failover between execution clients
- Circuit breaker patterns for unhealthy clients
- Exponential backoff for failed requests
- Comprehensive error tracking and alerting

### Scalability Considerations
- Support for multiple concurrent payload operations
- Connection pooling for high throughput
- Efficient caching of frequently accessed data
- Load balancing across multiple client instances

### Security Requirements
- Secure JWT token management and rotation
- TLS encryption for all client communications
- Input validation for all external data
- Rate limiting and abuse prevention

## Migration Strategy

### Phase 1-2: Foundation (Week 1)
- Set up directory structure and basic modules
- Migrate configuration and state management
- Ensure no disruption to existing functionality

### Phase 3-4: Core Logic (Week 2)
- Migrate engine logic and client abstraction
- Implement message handlers
- Maintain full backward compatibility

### Phase 5-6: Enhancement (Week 3)
- Add metrics, validation, and supervision
- Implement comprehensive test suite
- Performance optimization and tuning

### Phase 7: Completion (Week 4)
- Advanced features and final integration
- Documentation and cleanup
- Production readiness validation

## Actor Integration Patterns

### Core Integrations

#### 1. ChainActor ↔ EngineActor (Primary Integration)
The most critical integration handles block production and execution flow:

**Block Production Flow**:
```
ChainActor → BuildPayloadMessage → EngineActor
EngineActor → PayloadBuilt → ChainActor  
ChainActor → ExecutePayloadMessage → EngineActor
EngineActor → PayloadExecuted → ChainActor
```

**Message Types**:
- `BuildPayloadMessage` - Request payload construction with withdrawals
- `GetPayloadMessage` - Retrieve built payload by ID
- `ExecutePayloadMessage` - Execute payload and update forkchoice
- `ForkchoiceUpdatedMessage` - Update execution layer head/finalized state
- `EngineStatusMessage` - Health and sync status reporting

#### 2. BridgeActor → EngineActor (Peg-Out Detection)
Bridge operations monitor EVM events for peg-out requests:

**Peg-Out Flow**:
```
EngineActor → BurnEventDetected → BridgeActor
BridgeActor → ValidatePegOut → EngineActor
EngineActor → PegOutValidated → BridgeActor
```

**Message Types**:
- `BurnEventDetected` - EVM burn event notification
- `ValidatePegOutMessage` - Verify burn transaction authenticity
- `GetTransactionReceiptMessage` - Fetch transaction receipt details

#### 3. StorageActor ↔ EngineActor (Data Persistence)
Engine execution data must be persisted for historical queries:

**Storage Flow**:
```
EngineActor → StoreExecutionData → StorageActor
StorageActor → QueryExecutionData → EngineActor
```

**Message Types**:
- `StoreExecutionDataMessage` - Persist execution results, receipts, logs
- `QueryExecutionDataMessage` - Retrieve historical execution data
- `StorePayloadMessage` - Cache built payloads for recovery

#### 4. NetworkActor → EngineActor (Transaction Processing)
Incoming transactions need validation and pool management:

**Transaction Flow**:
```
NetworkActor → ValidateTransactionMessage → EngineActor
EngineActor → TransactionValidated → NetworkActor
EngineActor → AddToTxPoolMessage → Internal
```

**Message Types**:
- `ValidateTransactionMessage` - Validate incoming transaction
- `AddToTxPoolMessage` - Add valid transaction to mempool
- `GetTxPoolStatusMessage` - Query mempool state

### Integration Architecture

#### Actor Address Management
```rust
pub struct ActorAddresses {
    pub chain_actor: Addr<ChainActor>,
    pub storage_actor: Option<Addr<StorageActor>>,
    pub bridge_actor: Option<Addr<BridgeActor>>,
    pub network_actor: Option<Addr<NetworkActor>>,
}
```

#### Message Routing
- All inter-actor messages include correlation IDs for tracing
- Timeout handling for actor communication failures
- Circuit breaker patterns for unhealthy actor dependencies
- Graceful degradation when optional actors are unavailable

#### Error Handling Strategy
- Non-critical integrations (StorageActor) are optional
- Critical integrations (ChainActor) trigger engine halt on failure
- Automatic retry with exponential backoff for transient failures
- Comprehensive error reporting and alerting

### Supervision Integration
The EngineActor integrates with the Alys V2 supervision hierarchy:

- **Priority**: Consensus-level (highest priority restart)
- **Dependencies**: ChainActor (bidirectional), StorageActor (optional)
- **Health Checks**: Execution client connectivity, payload building latency
- **Failure Modes**: Execution client disconnect, payload timeout, validation failure

## Success Criteria

1. **Functional Completeness**: All existing engine functionality preserved
2. **Performance Targets**: Meet or exceed current performance benchmarks
3. **Reliability**: 99.9% uptime with automatic failure recovery
4. **Maintainability**: Clear separation of concerns and comprehensive tests
5. **Documentation**: Complete API documentation and usage examples
6. **Integration Completeness**: Seamless actor communication with proper error handling