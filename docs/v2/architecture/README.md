# Alys V2 Architecture Documentation

This directory contains comprehensive documentation for the Alys V2 actor-based architecture, including interaction patterns, communication flows, lifecycle management, and supervision hierarchy.

## Documentation Overview

### 📋 [Actor Interaction Patterns](./actor-interaction-patterns.md)
Comprehensive guide to how actors communicate and interact in the V2 system:
- Core actor types and their responsibilities
- Message flow patterns for key operations (block production, peg operations, sync)
- Communication patterns (request-response, fire-and-forget, streaming, supervision)
- State management and error handling principles
- Migration guide from V1 Arc<RwLock<>> patterns to V2 message passing

### 📊 [Communication Flow Diagrams](./diagrams/communication-flows.md)
Visual representations of system interactions using Mermaid diagrams:
- System overview architecture with supervision hierarchy
- Detailed sequence diagrams for critical flows:
  - Block production and finalization
  - Bitcoin peg-in operations with governance approval
  - Ethereum peg-out operations with federation signatures
  - Blockchain sync recovery with parallel downloads
  - Governance message routing and emergency procedures
- Actor state machines and lifecycle transitions
- Performance characteristics and backpressure management

### 🔄 [Actor Lifecycle Management](./actor-lifecycle-management.md)
Detailed documentation of actor lifecycle states and management:
- Actor state transitions (Initializing → Running → Stopping → Stopped)
- AlysActor trait implementation with initialization, health checks, and shutdown
- Supervision strategies (immediate restart, exponential backoff, circuit breaker)
- Health monitoring and status aggregation
- Configuration hot-reload without service interruption
- Graceful shutdown coordination with dependency ordering
- Comprehensive metrics collection and observability

### 🏗️ [Supervision Hierarchy](./supervision-hierarchy.md)
Architecture for fault tolerance and automatic recovery:
- Hierarchical supervision tree with domain-specific supervisors
- Fault isolation boundaries (Consensus, Network, Bridge, Storage)
- Restart strategies based on error types and severity
- Domain-specific supervisors (ChainSupervisor, NetworkSupervisor, BridgeSupervisor)
- Error classification with severity levels and recommended actions
- Emergency procedures and coordinated system response
- Supervision metrics and health dashboard

## Architecture Principles

### 1. Actor-Based Concurrency
- **Message Passing**: All communication through asynchronous messages
- **Isolated State**: Each actor owns its state completely
- **Fault Isolation**: Actor failures don't cascade to other components
- **Supervision Trees**: Hierarchical fault tolerance with automatic restart

### 2. No Shared Mutable State
- **Eliminates Deadlocks**: No Arc<RwLock<>> patterns that can cause lock ordering issues
- **True Parallelism**: Actors can process messages concurrently without lock contention  
- **Simplified Testing**: Each actor can be tested in isolation
- **Clear Ownership**: State ownership is explicit and unambiguous

### 3. Domain-Driven Design
- **Clear Boundaries**: Actors grouped by domain (Consensus, Network, Bridge, Storage)
- **Single Responsibility**: Each actor has a well-defined purpose
- **Dependency Injection**: Actors receive dependencies through configuration
- **Interface Segregation**: Actors expose minimal, focused interfaces

### 4. Observability First
- **Comprehensive Metrics**: Every actor reports detailed metrics
- **Distributed Tracing**: Message flows tracked across actor boundaries
- **Health Monitoring**: Continuous health checks with alerting
- **Error Classification**: Structured error handling with severity levels

### 5. Fault Tolerance
- **Supervision Strategies**: Multiple restart strategies based on failure types
- **Circuit Breakers**: Prevent cascading failures in external dependencies
- **Emergency Procedures**: Coordinated response to critical system failures
- **Graceful Degradation**: System continues operating with reduced functionality

## Migration from V1

### Before (V1 Problems)
```rust
// V1 - Deadlock prone
pub struct Chain {
    engine: Arc<RwLock<Engine>>,
    bridge: Arc<RwLock<Bridge>>,
    network: Arc<RwLock<Network>>,
}

// Multiple locks can cause deadlocks
let engine = self.engine.write().await;  // Lock 1
let bridge = self.bridge.write().await;  // Lock 2 - potential deadlock
```

### After (V2 Solution)
```rust
// V2 - Deadlock free message passing
impl Handler<ProcessBlock> for ChainActor {
    fn handle(&mut self, msg: ProcessBlock, _ctx: &mut Context<Self>) -> Self::Result {
        // Sequential message passing - no locks
        let engine_result = self.engine_actor.send(ExecuteBlock { block }).await?;
        let bridge_result = self.bridge_actor.send(ValidatePegOps { block }).await?;
        // Combine results without holding any locks
    }
}
```

## System Benefits

### Performance Improvements
- **5x Throughput Increase**: Elimination of lock contention enables true parallelism
- **<1ms Message Latency**: Efficient actor message passing (p99 <10ms cross-actor)
- **Memory Efficiency**: No shared state reduces memory fragmentation
- **CPU Utilization**: Better multi-core utilization with parallel actors

### Reliability Improvements
- **Zero Deadlocks**: Message-passing architecture prevents lock ordering issues
- **Fault Isolation**: Component failures contained within actor boundaries
- **Automatic Recovery**: Supervision trees provide self-healing capabilities
- **Graceful Degradation**: System continues with reduced functionality during failures

### Development Experience
- **Easier Testing**: Actors can be unit tested in isolation
- **Clear Dependencies**: Message contracts make component relationships explicit
- **Maintainability**: Well-defined actor boundaries reduce coupling
- **Debugging**: Message tracing provides clear execution flow visibility

## Integration Points

### External Systems
- **Bitcoin Network**: BridgeActor manages Bitcoin node connections and UTXO tracking
- **Anduro Governance**: StreamActor handles bi-directional gRPC streaming
- **Ethereum Execution**: EngineActor interfaces with Geth/Reth clients
- **Database**: StorageActor provides centralized persistence layer

### Legacy Compatibility
During migration, the system maintains compatibility with existing interfaces while gradually moving to the actor model. The supervisor system can manage both V1 and V2 components during the transition period.

## Performance Characteristics

### Message Throughput Targets
- ChainActor: 1,000 messages/sec (block production)
- NetworkActor: 10,000 messages/sec (peer communication)
- BridgeActor: 100 messages/sec (peg operations)
- SyncActor: 5,000 messages/sec (sync coordination)
- StorageActor: 2,000 messages/sec (database operations)

### Latency Requirements
- Intra-actor messaging: <1ms p99
- Cross-actor messaging: <5ms p99
- External system calls: <100ms p99
- Database operations: <10ms p99

### Resource Usage
- Memory: <100MB baseline for actor framework
- CPU: <5% overhead for message passing
- Network: Minimal overhead for internal communication
- Storage: Efficient actor state persistence

## Future Enhancements

### Planned Improvements
1. **Distributed Actors**: Support for actors across multiple nodes
2. **Actor Migration**: Hot migration of actors between nodes
3. **Advanced Supervision**: ML-based failure prediction and prevention
4. **Performance Optimization**: Zero-copy message passing for large payloads
5. **Security Enhancements**: Actor-level security policies and sandboxing

### Monitoring and Alerting
1. **Actor Health Dashboard**: Real-time system health visualization
2. **Predictive Alerting**: AI-based failure prediction
3. **Performance Benchmarking**: Automated performance regression testing
4. **Chaos Engineering**: Automated failure injection testing

This architecture provides a solid foundation for the Alys V2 system with improved performance, reliability, and maintainability compared to the V1 implementation.