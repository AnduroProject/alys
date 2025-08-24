Detailed Implementation Plan: Create Chain Actor Module Directory

  Current State Analysis:

  - Current chain actor logic is spread across multiple files:
    - chain_actor.rs (1,392 lines) - Main implementation
    - chain_actor_handlers.rs - Message handlers
    - chain_actor_supervision.rs - Supervision logic
    - chain_actor_tests.rs - Tests
    - chain_migration_adapter.rs - Migration utilities

  Proposed Directory Structure:

  app/src/actors/chain/
  ├── mod.rs                    # Module exports and public interface
  ├── actor.rs                  # Core ChainActor implementation (moved from chain_actor.rs)
  ├── config.rs                 # Configuration structures and defaults
  ├── state.rs                  # Chain state and related structures
  ├── messages.rs               # Chain-specific message definitions
  ├── handlers/                 # Message handler implementations
  │   ├── mod.rs
  │   ├── block_handlers.rs     # Block import/production handlers
  │   ├── consensus_handlers.rs # Consensus-related handlers  
  │   ├── auxpow_handlers.rs    # AuxPoW/mining handlers
  │   └── peg_handlers.rs       # Peg-in/peg-out handlers
  ├── supervision.rs            # Supervision strategies (moved from chain_actor_supervision.rs)
  ├── migration.rs              # Migration adapter (moved from chain_migration_adapter.rs)
  ├── metrics.rs                # Chain-specific metrics and performance tracking
  ├── validation.rs             # Block and transaction validation logic
  └── tests/                    # Test organization
      ├── mod.rs
      ├── unit_tests.rs         # Core unit tests
      ├── integration_tests.rs  # Integration tests
      ├── performance_tests.rs  # Performance benchmarks
      └── mock_helpers.rs       # Test utilities and mocks

  Implementation Steps:

  Phase 1: Directory Setup and Core Structure

  1. Create base directory structure:
    - Create app/src/actors/chain/ directory
    - Create all subdirectories (handlers/, tests/)
    - Create empty stub files for each module
  2. Create module interface (mod.rs):
    - Define public exports for the chain module
    - Re-export core types and traits
    - Maintain backward compatibility with existing imports
  3. Extract configuration (config.rs):
    - Move ChainActorConfig, PerformanceTargets from chain_actor.rs
    - Add environment-specific configuration loading
    - Include validation for configuration parameters

  Phase 2: Core Implementation Migration

  4. Extract state management (state.rs):
    - Move ChainState, FederationState, AuxPowState from chain_actor.rs
    - Move all state-related structures and implementations
    - Add state serialization/deserialization if needed
  5. Extract core actor (actor.rs):
    - Move main ChainActor struct and core implementation
    - Move Actor, AlysActor, BlockchainAwareActor trait implementations
    - Keep startup/shutdown logic and timers
  6. Create message definitions (messages.rs):
    - Define all chain-specific message types
    - Include message correlation and tracing support
    - Add message validation and serialization

  Phase 3: Handler Organization

  7. Create handler modules:
    - block_handlers.rs: Import/export block operations
    - consensus_handlers.rs: Aura PoA consensus logic
    - auxpow_handlers.rs: Bitcoin merged mining operations
    - peg_handlers.rs: Two-way peg operations
  8. Move existing handlers:
    - Extract relevant handlers from chain_actor_handlers.rs
    - Organize by functional area
    - Maintain message routing and correlation IDs

  Phase 4: Supporting Modules

  9. Extract supervision logic (supervision.rs):
    - Move content from chain_actor_supervision.rs
    - Add blockchain-specific supervision policies
    - Include restart strategies and health checks
  10. Extract migration utilities (migration.rs):
    - Move content from chain_migration_adapter.rs
    - Add version compatibility checks
    - Include rollback mechanisms
  11. Create metrics module (metrics.rs):
    - Extract ChainActorMetrics and related structures
    - Add Prometheus integration
    - Include performance dashboards configuration
  12. Create validation module (validation.rs):
    - Extract validation logic from main actor
    - Add comprehensive block/transaction validation
    - Include signature verification and consensus rules

  Phase 5: Testing Infrastructure

  13. Reorganize tests:
    - Move existing tests from chain_actor_tests.rs
    - Create test categories: unit, integration, performance
    - Add mock helpers and test utilities
  14. Add comprehensive test coverage:
    - Unit tests for each module
    - Integration tests for actor interactions
    - Performance benchmarks for critical paths
    - Chaos engineering tests for fault tolerance

  Phase 6: Integration and Cleanup

  15. Update imports throughout codebase:
    - Update app/src/actors/mod.rs to use new module structure
    - Update all references to chain actor components
    - Ensure backward compatibility where needed
  16. Cleanup old files:
    - Remove original chain_actor.rs and related files
    - Update documentation and examples
    - Run comprehensive tests to ensure no regressions