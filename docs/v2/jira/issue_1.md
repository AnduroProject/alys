# ALYS-001: V2 Codebase Structure & Foundation Setup

## Issue Type
Task

## Summary
Establish foundational V2 codebase structure with actor system architecture, directory reorganization, and core infrastructure components to support the complete Alys migration to Anduro Governance client, transition to message-passing actor model, and upgrade to Lighthouse V5.

### Current Problems
- **Deadlock Risk**: Multiple `Arc<RwLock<>>` fields create lock ordering issues
- **Poor Concurrency**: Shared state prevents true parallelism
- **Complex Testing**: Interdependent components difficult to test in isolation  
- **Fault Propagation**: Single component failure can crash entire system

### V2 Solution Architecture
- **Actor System**: Message-passing with isolated state per actor
- **Supervision Trees**: Hierarchical fault tolerance with automatic restart
- **Clean Separation**: Distinct actors for Chain, Engine, Bridge, Sync, Network operations
- **Workflow-Based**: Business logic flows separate from actor implementations

## Acceptance Criteria

## Detailed Implementation Subtasks (42 tasks across 7 phases)

### Phase 1: Architecture Planning & Design Review (6 tasks)
- [X] **ALYS-001-01**: Review V2 architecture documentation and validate actor system design patterns
- [X] **ALYS-001-02**: Design actor supervision hierarchy with restart strategies and fault isolation boundaries [https://marathondh.atlassian.net/browse/AN-287]
- [X] **ALYS-001-03**: Define message passing protocols and message envelope structure for typed communication [https://marathondh.atlassian.net/browse/AN-288]
- [X] **ALYS-001-04**: Create actor lifecycle state machine with initialization, running, stopping, and recovery states [https://marathondh.atlassian.net/browse/AN-289]
- [X] **ALYS-001-05**: Design configuration loading system with environment-specific overrides and validation [https://marathondh.atlassian.net/browse/AN-290]
- [X] **ALYS-001-06**: Document actor interaction patterns and establish communication flow diagrams [https://marathondh.atlassian.net/browse/AN-291]

### Phase 2: Directory Structure & Workspace Setup (8 tasks)
- [X] **ALYS-001-07**: Create complete directory structure for `app/src/actors/` with all actor implementations [https://marathondh.atlassian.net/browse/AN-292]
- [X] **ALYS-001-08**: Create `app/src/messages/` directory with typed message definitions for each actor domain [https://marathondh.atlassian.net/browse/AN-293]
- [X] **ALYS-001-09**: Create `app/src/workflows/` directory for business logic flows and state machines [https://marathondh.atlassian.net/browse/AN-294]
- [X] **ALYS-001-10**: Create `app/src/types/` directory with actor-friendly data structures and message envelopes [https://marathondh.atlassian.net/browse/AN-295]
- [X] **ALYS-001-11**: Create `app/src/config/` directory with comprehensive configuration management [https://marathondh.atlassian.net/browse/AN-296]
- [X] **ALYS-001-12**: Create `app/src/integration/` directory for external system interfaces and client wrappers [https://marathondh.atlassian.net/browse/AN-297]
- [X] **ALYS-001-13**: Create `crates/actor_system/` workspace crate with core actor framework implementation [https://marathondh.atlassian.net/browse/AN-298]
- [X] **ALYS-001-14**: Update root `Cargo.toml` workspace configuration and dependency management [https://marathondh.atlassian.net/browse/AN-299]

### Phase 3: Core Actor System Implementation (12 tasks)
- [X] **ALYS-001-15**: Implement `crates/actor_system/supervisor.rs` with supervision trees and restart strategies [https://marathondh.atlassian.net/browse/AN-300]
- [X] **ALYS-001-16**: Implement `crates/actor_system/mailbox.rs` with message queuing, backpressure, and bounded channels [https://marathondh.atlassian.net/browse/AN-301]
- [X] **ALYS-001-17**: Implement `crates/actor_system/lifecycle.rs` with actor spawning, stopping, and graceful shutdown [https://marathondh.atlassian.net/browse/AN-302]
- [X] **ALYS-001-18**: Implement `crates/actor_system/metrics.rs` with actor performance monitoring and telemetry [https://marathondh.atlassian.net/browse/AN-303]
- [X] **ALYS-001-19**: Define `AlysActor` trait with standardized interface, configuration, and metrics support [https://marathondh.atlassian.net/browse/AN-304]
- [X] **ALYS-001-20**: Implement `AlysSystem` root supervisor with hierarchical supervision and system health monitoring [https://marathondh.atlassian.net/browse/AN-305]
- [X] **ALYS-001-21**: Create `ChainSupervisor` for consensus layer supervision with blockchain-specific restart policies [https://marathondh.atlassian.net/browse/AN-306]
- [X] **ALYS-001-22**: Create `NetworkSupervisor` for P2P and sync supervision with connection recovery strategies [https://marathondh.atlassian.net/browse/AN-307]
- [X] **ALYS-001-23**: Create `BridgeSupervisor` for peg operations supervision with transaction retry mechanisms [https://marathondh.atlassian.net/browse/AN-308]
- [X] **ALYS-001-24**: Create `StorageSupervisor` for database operations supervision with connection pooling [https://marathondh.atlassian.net/browse/AN-309]
- [X] **ALYS-001-25**: Implement actor registration system with health checks and dependency tracking [https://marathondh.atlassian.net/browse/AN-310]
- [X] **ALYS-001-26**: Create actor communication bus for system-wide messaging and event distribution [https://marathondh.atlassian.net/browse/AN-311]

### Phase 4: Enhanced Data Structures & Types (6 tasks)
- [X] **ALYS-001-27**: Implement `ConsensusBlock` unified block representation with Lighthouse V5 compatibility [https://marathondh.atlassian.net/browse/AN-312]
- [X] **ALYS-001-28**: Implement `SyncProgress` advanced sync state tracking with parallel download coordination [https://marathondh.atlassian.net/browse/AN-313]
- [X] **ALYS-001-29**: Implement `PegOperation` enhanced peg tracking with governance integration and status workflow [https://marathondh.atlassian.net/browse/AN-314]
- [X] **ALYS-001-30**: Implement `MessageEnvelope<T>` actor message wrapper with distributed tracing and correlation IDs [https://marathondh.atlassian.net/browse/AN-315]
- [ ] **ALYS-001-31**: Create actor-specific error types with context preservation and recovery recommendations [https://marathondh.atlassian.net/browse/AN-316]
- [ ] **ALYS-001-32**: Implement serialization/deserialization support for all actor messages and state structures [https://marathondh.atlassian.net/browse/AN-317]

### Phase 5: Configuration & Integration Points (4 tasks)
- [ ] **ALYS-001-33**: Implement `AlysConfig` master configuration structure with validation and environment overrides [https://marathondh.atlassian.net/browse/AN-318]
- [ ] **ALYS-001-34**: Implement `ActorConfig` system settings including restart strategies, mailbox capacity, and timeouts [https://marathondh.atlassian.net/browse/AN-319]
- [ ] **ALYS-001-35**: Create integration clients: `GovernanceClient` (gRPC streaming), `BitcoinClient` (RPC), `ExecutionClient` (Geth/Reth) [https://marathondh.atlassian.net/browse/AN-320]
- [ ] **ALYS-001-36**: Implement configuration hot-reload system with actor notification and state preservation [https://marathondh.atlassian.net/browse/AN-321]

### Phase 6: Testing Infrastructure (4 tasks)
- [ ] **ALYS-001-37**: Create `ActorTestHarness` for integration testing with isolated actor environments [https://marathondh.atlassian.net/browse/AN-322]
- [ ] **ALYS-001-38**: Implement property-based testing framework for message ordering and actor state consistency [https://marathondh.atlassian.net/browse/AN-323]
- [ ] **ALYS-001-39**: Create chaos testing capabilities with network partitions, actor failures, and resource constraints [https://marathondh.atlassian.net/browse/AN-324]
- [ ] **ALYS-001-40**: Set up test utilities, mocks, and fixtures for external system integration testing [https://marathondh.atlassian.net/browse/AN-325]

### Phase 7: Documentation & Validation (2 tasks)
- [ ] **ALYS-001-41**: Create comprehensive documentation including architecture guides, API references, and code examples
- [ ] **ALYS-001-42**: Perform final integration testing with performance benchmarks and system validation

###  Directory Structure Implementation
- [ ] Create `app/src/actors/` with all actor implementations:
  - [ ] `supervisor.rs` - Root supervisor & fault tolerance
  - [ ] `chain_actor.rs` - Consensus coordination
  - [ ] `engine_actor.rs` - EVM execution interface  
  - [ ] `bridge_actor.rs` - Peg operations coordinator
  - [ ] `sync_actor.rs` - Parallel syncing logic
  - [ ] `network_actor.rs` - P2P networking
  - [ ] `stream_actor.rs` - Governance communication
  - [ ] `storage_actor.rs` - Database operations

- [ ] Create `app/src/messages/` with typed message definitions:
  - [ ] `chain_messages.rs` - Block production/import messages
  - [ ] `bridge_messages.rs` - Peg-in/out operation messages  
  - [ ] `sync_messages.rs` - Sync coordination messages
  - [ ] `system_messages.rs` - System-wide control messages

- [ ] Create `app/src/workflows/` for business logic flows:
  - [ ] `block_production.rs` - Block production workflow
  - [ ] `block_import.rs` - Block validation workflow
  - [ ] `peg_operations.rs` - Peg-in/out workflows
  - [ ] `sync_recovery.rs` - Sync & checkpoint recovery

###  Actor System Foundation
- [ ] Implement `crates/actor_system/` with core components:
  - [ ] `supervisor.rs` - Supervision trees with restart strategies
  - [ ] `mailbox.rs` - Message queuing with backpressure
  - [ ] `lifecycle.rs` - Actor lifecycle management
  - [ ] `metrics.rs` - Actor performance metrics

- [ ] Define `AlysActor` trait with standardized interface:
  ```rust
  pub trait AlysActor: Actor {
      type Config: Clone + Send + 'static;
      type Metrics: Default + Clone;
      fn new(config: Self::Config) -> Self;
      fn metrics(&self) -> &Self::Metrics;
  }
  ```

- [ ] Implement `AlysSystem` supervisor hierarchy:
  - [ ] `ChainSupervisor` - Consensus layer supervision
  - [ ] `NetworkSupervisor` - P2P and sync supervision
  - [ ] `BridgeSupervisor` - Peg operations supervision
  - [ ] `StorageSupervisor` - Database operations supervision

###  Enhanced Data Structures
- [ ] Create `app/src/types/` with actor-friendly types:
  - [ ] `ConsensusBlock` - Unified block representation with Lighthouse v5 support
  - [ ] `SyncProgress` - Advanced sync state tracking with production capabilities at 99.5%
  - [ ] `PegOperation` - Enhanced peg tracking with governance integration
  - [ ] `MessageEnvelope<T>` - Actor message wrapper with tracing

###  Configuration Architecture
- [ ] Implement `app/src/config/` with comprehensive configuration:
  - [ ] `AlysConfig` - Master configuration structure
  - [ ] `ActorConfig` - Actor system settings (restart strategies, mailbox capacity)
  - [ ] `SyncConfig` - Advanced sync settings (parallel downloads, checkpoint intervals)
  - [ ] `GovernanceConfig` - Governance streaming configuration

###  Integration Points
- [ ] Create `app/src/integration/` for external systems:
  - [ ] `GovernanceClient` - gRPC streaming to Anduro governance
  - [ ] `BitcoinClient` - Enhanced Bitcoin integration with UTXO tracking
  - [ ] `ExecutionClient` - Abstraction supporting Geth/Reth

###  Legacy Compatibility
- [ ] Maintain existing functionality during transition:
  - [ ] Refactor `chain.rs` to lightweight coordinator
  - [ ] Enhance `engine.rs` with actor wrapper
  - [ ] Update `aura.rs` with improved signature handling
  - [ ] Integrate `auxpow_miner.rs` with actor system

## Implementation Steps

### Phase 1: Directory Structure (Week 1)
1. Create all directory structures as specified
2. Add placeholder files with proper module declarations
3. Update `Cargo.toml` workspace configuration
4. Ensure compilation passes with stub implementations

### Phase 2: Actor Framework (Week 1-2)
1. Implement core actor system in `crates/actor_system/`
2. Create `AlysActor` trait and basic supervisor
3. Set up message passing infrastructure
4. Add basic lifecycle management

### Phase 3: Core Types & Config (Week 2)
1. Define enhanced data structures in `app/src/types/`
2. Implement comprehensive configuration system
3. Create integration point interfaces
4. Set up metrics and monitoring hooks

### Phase 4: Testing Infrastructure (Week 2)
1. Create `ActorTestHarness` for integration testing
2. Add property-based testing framework
3. Set up chaos testing capabilities
4. Implement test utilities and mocks

## Testing Requirements

### Unit Testing
- [ ] Actor isolation tests - verify no shared state
- [ ] Message handling tests for each actor type
- [ ] Supervisor restart policy verification
- [ ] Configuration loading and validation tests

### Integration Testing  
- [ ] Full system startup and shutdown procedures
- [ ] Actor communication patterns verification
- [ ] External system integration tests (mocked)
- [ ] Configuration hot-reload testing

### Property Testing
- [ ] Message ordering guarantees under load
- [ ] Actor restart behavior under various failure modes
- [ ] Memory usage bounds under sustained load
- [ ] No deadlock properties with concurrent messaging

## Dependencies
- **Actix**: Actor system implementation framework
- **Tokio**: Async runtime for message handling
- **Serde**: Configuration serialization/deserialization  
- **Tracing**: Distributed tracing support
- **Proptest**: Property-based testing framework

## Risk Analysis

### Technical Risks
- **Complexity**: Actor system adds conceptual overhead � *Mitigation: Comprehensive documentation and examples*
- **Performance**: Message passing overhead � *Mitigation: Benchmarking shows >5x gains from parallelism*
- **Learning Curve**: Team familiarity with actor model � *Mitigation: Training sessions and pair programming*

### Integration Risks  
- **Compilation**: Large structural changes may break builds � *Mitigation: Incremental rollout with feature flags*
- **State Migration**: Existing state structures need conversion � *Mitigation: Compatibility shims during transition*

## Success Metrics

### Performance Targets
- [ ] Compilation time: <2 minutes for full build
- [ ] Test execution: All unit tests <30 seconds
- [ ] Memory usage: Foundation components <100MB baseline
- [ ] Actor message latency: p99 <10ms

### Quality Gates
- [ ] Zero compilation warnings in new code
- [ ] 100% test coverage for actor framework
- [ ] All integration tests passing
- [ ] Code review approval from 2+ senior engineers

## Documentation Deliverables
- [ ] `docs/v2/architecture-overview.md` - System design documentation
- [ ] `docs/v2/actor-system-guide.md` - Developer guide for actor implementation
- [ ] `docs/v2/migration-strategy.md` - Step-by-step migration approach
- [ ] `examples/actor-patterns/` - Code examples for common actor patterns

## Definition of Done
- [ ] All directory structures created and populated
- [ ] Actor system framework fully implemented and tested
- [ ] Configuration system supports all required scenarios
- [ ] Integration points defined and stubbed
- [ ] Legacy compatibility maintained
- [ ] Test infrastructure operational
- [ ] Documentation complete and reviewed
- [ ] Code review completed and approved
- [ ] Performance benchmarks meet targets

## Estimated Effort
**Time Estimate**: 3-4 days (24-32 hours total) with detailed breakdown:
- Phase 1 - Architecture planning & design review: 4-6 hours (includes documentation review, supervision design, message protocol definition)
- Phase 2 - Directory structure & workspace setup: 6-8 hours (includes all directory creation, Cargo.toml updates, module structure)
- Phase 3 - Core actor system implementation: 12-16 hours (includes supervisor trees, mailbox system, lifecycle management, metrics)
- Phase 4 - Enhanced data structures & types: 3-4 hours (includes ConsensusBlock, SyncProgress, MessageEnvelope implementations)
- Phase 5 - Configuration & integration points: 2-3 hours (includes config system, external client interfaces)
- Phase 6 - Testing infrastructure: 4-6 hours (includes test harness, property testing, chaos testing setup)
- Phase 7 - Documentation & validation: 2-3 hours (includes final documentation, integration testing, benchmarks)

**Critical Path Dependencies**: Phase 1 → Phase 2 → Phase 3 → (Phase 4,5,6 in parallel) → Phase 7
**Resource Requirements**: 1 senior developer with Rust/Actix experience, access to development environment
**Risk Buffer**: 20% additional time allocated for unexpected integration issues and debugging

## Labels
`alys`, `v2`

## Components
- Infrastructure
- Consensus  
- Federation
- Smart Contracts

---

*This epic establishes the foundation for all subsequent V2 migration work. Success here is critical for the timeline and quality of the overall migration.*