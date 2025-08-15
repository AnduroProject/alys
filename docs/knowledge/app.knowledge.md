# Alys App Knowledge Graph

## Overview
This knowledge graph maps the architecture and relationships within the `app/src/` directory of the Alys Bitcoin sidechain project. The application implements a hybrid consensus system combining federated Proof-of-Authority block production with Bitcoin merged mining finalization.

## Core Architecture Layers

### 1. Application Entry Point
```
main.rs → app.rs (run function)
```

**Key Components:**
- **main.rs**: Simple entry point calling `app::run()`
- **app.rs**: Main application orchestrator with CLI argument parsing and system initialization

**Dependencies:**
- Imports all major subsystems: `aura`, `chain`, `engine`, `spec`, `store`
- Integrates with external `bridge` crate for federation operations
- Uses `lighthouse_wrapper` for BLS cryptography and Ethereum types

### 2. Consensus Layer

```
aura.rs ← chain.rs → auxpow_miner.rs
    ↑                    ↓
signatures.rs ← auxpow.rs
```

**Components:**
- **aura.rs**: Implements Aura Proof-of-Authority consensus for federated block production
  - Manages authority rotation and slot timing
  - Validates block signatures from federation members
  - Integrates with BLS signature verification

- **auxpow_miner.rs**: Manages auxiliary Proof-of-Work mining integration
  - Interfaces with Bitcoin miners for merged mining
  - Handles difficulty adjustments and target calculations
  - Manages AuxPow block submission and validation

- **auxpow.rs**: Core auxiliary Proof-of-Work data structures and validation
  - Bitcoin auxiliary proof-of-work verification
  - Chain ID and merge mining protocol implementation

- **signatures.rs**: Cryptographic signature handling and validation
  - BLS signature aggregation for federation consensus
  - Individual approval signature verification

### 3. Block Management Layer

```
block.rs ← block_candidate/ ← chain.rs → block_hash_cache.rs
    ↓           ↓                ↓
 engine.rs → storage.rs → metrics.rs
```

**Components:**
- **block.rs**: Core block data structures and serialization
  - `SignedConsensusBlock` - Federation-signed blocks
  - `AuxPowHeader` - Auxiliary proof-of-work headers
  - Block validation and conversion utilities

- **block_candidate/**: Block candidate management system
  - `block_candidate_cache.rs`: Thread-safe caching of pending block candidates
  - `candidate_state.rs`: State management for block approval process
  - `mod.rs`: Async wrapper providing thread-safe access

- **chain.rs**: Core blockchain state management and operations
  - Bitcoin wallet integration (`BitcoinWallet = UtxoManager<Tree>`)
  - Peg-in/peg-out processing through bridge integration
  - Block production, validation, and finalization
  - P2P network message handling
  - RPC circuit breaker for peer management

- **block_hash_cache.rs**: Performance optimization for block hash lookups
  - Caches frequently accessed block hashes
  - Reduces database lookup overhead

### 4. Execution Layer Integration

```
engine.rs ← chain.rs → rpc.rs
    ↓         ↓         ↓
lighthouse_wrapper (Geth/Reth interface)
```

**Components:**
- **engine.rs**: Ethereum execution layer interface
  - Integrates with Geth/Reth via Engine API
  - Handles block building, execution, and finalization
  - Manages withdrawals for peg-in operations
  - Converts between consensus and execution formats

- **rpc.rs**: JSON-RPC server for external API access
  - Consensus layer RPC methods (port 3000)
  - Mining-related endpoints (`createauxblock`, `submitauxblock`)
  - Bridge operations (`getdepositaddress`)
  - Integration with AuxPow miner

### 5. Network Layer

```
network/mod.rs → network/rpc/ → chain.rs
       ↓              ↓
   P2P Gossip    Direct RPC
   (libp2p)     (Request/Response)
```

**Network Components:**
- **network/mod.rs**: P2P networking foundation
  - libp2p integration with Gossipsub for message broadcasting
  - Network behavior management (`MyBehaviour`)
  - Peer discovery and connection management
  - Message types: `ConsensusBlock`, `ApproveBlock`, `QueuePow`, `PegoutSignatures`

- **network/rpc/**: Direct peer-to-peer RPC communication
  - **protocol.rs**: RPC protocol definition and message handling
  - **handler.rs**: Connection and substream management
  - **methods.rs**: RPC method implementations and response handling
  - **codec/**: Message encoding/decoding (SSZ with Snappy compression)
  - **rate_limiter.rs**: Request rate limiting and DoS protection
  - **outbound.rs**: Outbound RPC request management

### 6. Storage and State Management

```
store.rs ← chain.rs → spec.rs
   ↓                    ↓
leveldb            Configuration
```

**Components:**
- **store.rs**: Persistent storage abstraction
  - LevelDB backend for block and state storage
  - Database column organization (`ChainInfo`, `Block`, `AuxPowBlockHeight`)
  - Key-value operations with typed access patterns
  - Head tracking and finalization state

- **spec.rs**: Chain specification and configuration
  - Genesis parameters and authority sets
  - Bitcoin network configuration
  - Difficulty adjustment parameters
  - Federation and consensus settings

### 7. Supporting Infrastructure

```
metrics.rs ← All Components
error.rs ← All Components
```

**Components:**
- **metrics.rs**: Prometheus metrics collection
  - Consensus metrics (block production, slot tracking)
  - Network metrics (peer counts, message totals)
  - Mining metrics (AuxPow processing, difficulty)
  - Bridge metrics (peg-in/peg-out operations)

- **error.rs**: Centralized error handling
  - Consensus errors (invalid blocks, signature failures)
  - Network errors (peer failures, protocol violations)
  - Mining errors (invalid proof-of-work, chain mismatch)
  - Bridge operation errors

## Key Data Flow Patterns

### 1. Block Production Flow
```
aura.rs (slot timing) → chain.rs (build block) → engine.rs (execution) → network/mod.rs (broadcast)
```

### 2. Mining Integration Flow
```
rpc.rs (mining API) → auxpow_miner.rs (manage work) → chain.rs (process AuxPow) → store.rs (persist)
```

### 3. Peg-in Processing Flow
```
bridge crate (detect Bitcoin tx) → chain.rs (process peg-in) → engine.rs (mint tokens) → store.rs (record)
```

### 4. Network Message Flow
```
network/mod.rs (receive) → network/rpc/ (decode) → chain.rs (process) → network/mod.rs (respond/broadcast)
```

## External Dependencies

### Bridge Integration
- **Purpose**: Two-way peg functionality
- **Components Used**: `BitcoinCore`, `BitcoinSigner`, `Bridge`, `Federation`
- **Integration Points**: `chain.rs` for peg-in/peg-out processing

### Lighthouse Wrapper
- **Purpose**: Ethereum consensus layer types and cryptography
- **Components Used**: BLS signatures, execution layer interface, storage abstraction
- **Integration Points**: Throughout consensus and execution layers

### Bitcoin Integration
- **Purpose**: Merged mining and Bitcoin transaction processing
- **Integration Points**: `auxpow.rs`, `auxpow_miner.rs`, `chain.rs`

## Critical Relationships

1. **Chain ↔ Engine**: Bidirectional execution layer integration
2. **Chain ↔ Network**: P2P message processing and broadcast
3. **Aura ↔ Chain**: Consensus timing and block validation
4. **AuxPow Miner ↔ Chain**: Mining work distribution and result processing
5. **RPC ↔ Chain**: External API access to blockchain state
6. **Store ↔ Chain**: Persistent state management and retrieval

## Performance Considerations

- **Block Hash Cache**: Optimizes frequent hash lookups
- **Block Candidate Cache**: Thread-safe pending block management
- **RPC Circuit Breaker**: Prevents overwhelming failing peers
- **Rate Limiting**: Protects against DoS attacks on network layer
- **Async Processing**: Non-blocking I/O throughout the application

This knowledge graph represents a sophisticated blockchain implementation that successfully integrates Bitcoin merged mining with Ethereum-compatible execution, federated consensus, and comprehensive two-way peg functionality.