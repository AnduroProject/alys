# Alys V2 Actor System Refactoring Project

## System Architecture Overview

### V0 (Current Working System)
- **Location**: `/Users/michael/zDevelopment/Mara/alys-v2/` (excluding `app/src/actors_v2/`)
- **Status**:  **ACTIVELY WORKING VERSION** - Production system in use
- **Architecture**: Monolithic design with `chain.rs` (2000+ lines) handling core blockchain operations
- **Components**: `aura.rs`, `engine.rs`, `bridge`, storage systems all functional
- **Critical**: This system must remain operational during V2 transition

### V1 (Failed Refactor Attempt)
- **Location**: `/Users/michael/zDevelopment/Mara/alys/app/src/actors/` (218 files)
- **Status**: L **FAILED ATTEMPT** - Overly complex, non-functional
- **Issues**:
  - Too complex with unnecessary features/enhancements
  - Multi-level supervision hierarchies
  - Tightly coupled actor dependencies
  - Never reached working state
- **Usage**: Reference only for architecture ideas, NOT for implementation

### V2 (Current Refactoring Effort)
- **Location**: `/Users/michael/zDevelopment/Mara/alys-v2/app/src/actors_v2/` (85 files)
- **Status**: =� **IN DEVELOPMENT** - 30% functionally complete
- **Goal**: Simple, concise, easy-to-understand actor-based model
- **Strategy**: Incremental migration with V0 co-existence
- **Architecture**: Streamlined n-actor system (Chain, Storage, Network, Sync, ...n)

## Key Development Principles

### 1. **Co-existence First**
- V2 must run alongside V0 without breaking existing functionality
- Shared infrastructure components (`aura.rs`, `engine.rs`, `bridge`)
- Namespace isolation between V0 and V2 systems
- Gradual transition, not big-bang replacement

### 2. **Simplicity Over Complexity**
- Learn from V1's failure - avoid over-engineering
- Clear separation of concerns between actors
- Minimal supervision hierarchy (flat structure)
- Focus on core functionality first, enhancements later

### 3. **Incremental Migration Strategy**
- Phase 1: Complete V2 core handlers and cross-actor integration
- Phase 2: Implement full block production/import pipelines
- Phase 3: Migrate advanced features (AuxPoW, mining coordination)
- Phase 4: Deprecate V0 components safely

## Current Implementation Status

###  Completed (90-100%)
- StorageActor V2: Production-ready with comprehensive testing
- NetworkActor V2: Working libp2p foundation
- ChainActor V2: Architecture and message system complete
- Testing framework: 43 passing tests

### =6 Partial (30-60%)
- ChainActor handlers: Status queries work, block operations are placeholders
- Cross-actor methods: Implemented but not connected to handlers
- Integration patterns: Methods exist but unused (compiler warnings confirm)

### L Missing (0-20%)
- Block production pipeline: Engine/Aura integration needed
- Block import/validation: Storage integration required
- Sync coordination: NetworkActor/SyncActor workflows
- Full blockchain functionality: Core operations non-functional

## Critical Success Factors

1. **Keep V0 Working**: Never break the current production system
2. **Avoid V1 Mistakes**: Resist complexity creep and over-engineering
3. **Incremental Progress**: Small, testable steps with clear milestones
4. **Clear Interfaces**: Well-defined actor boundaries and message contracts
5. **Comprehensive Testing**: Maintain test coverage throughout migration

## Next Immediate Priorities

1. Connect existing cross-actor methods to ChainActor handlers (high impact, low risk)
2. Implement GetBlockByHash/Height with StorageActor integration
3. Enable BroadcastBlock handler with NetworkActor calls
4. Add block import pipeline using existing Engine/Aura/Storage components

**Remember**: The goal is a working, maintainable system - not a complex showcase of actor patterns.