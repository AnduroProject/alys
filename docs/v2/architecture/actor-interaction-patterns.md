# Alys V2 Actor Interaction Patterns

## Overview

The Alys V2 architecture implements a message-passing actor system that eliminates the Arc<RwLock<>> anti-patterns found in V1. This document describes the interaction patterns between actors and provides guidance for implementing new actors.

## Core Actor Types

### ChainActor (app/src/actors/chain_actor.rs)
**Primary Responsibility**: Consensus coordination and block lifecycle management

**Key Interactions**:
- Receives block proposals from EngineActor
- Coordinates with BridgeActor for peg operation validation
- Sends finalized blocks to NetworkActor for propagation
- Requests sync updates from SyncActor when behind
- Manages Aura PoA slot assignments and timing

### EngineActor (app/src/actors/engine_actor.rs)
**Primary Responsibility**: EVM execution layer interface (Geth/Reth)

**Key Interactions**:
- Executes transactions received from ChainActor
- Returns execution results and state changes
- Handles transaction pool management
- Provides block template construction
- Manages execution client lifecycle

### BridgeActor (app/src/actors/bridge_actor.rs)
**Primary Responsibility**: Bitcoin peg operations coordination

**Key Interactions**:
- Monitors Bitcoin blockchain for peg-in transactions
- Processes peg-out burn events from EngineActor
- Coordinates with FederationV2 for multi-signature operations
- Validates cross-chain transaction authenticity
- Manages UTXO tracking and Bitcoin wallet state

### SyncActor (app/src/actors/sync_actor.rs)
**Primary Responsibility**: Blockchain synchronization and parallel downloading

**Key Interactions**:
- Receives sync requests from ChainActor
- Downloads blocks from multiple NetworkActor peers simultaneously
- Validates block integrity before forwarding to ChainActor
- Manages sync progress and checkpoint recovery
- Handles fork detection and resolution

### NetworkActor (app/src/actors/network_actor.rs)
**Primary Responsibility**: P2P networking and peer management

**Key Interactions**:
- Propagates blocks received from ChainActor to peers
- Forwards transactions to EngineActor for validation
- Manages peer connections and libp2p gossipsub subscriptions
- Provides peer discovery and connection management
- Handles network-level message routing

### StreamActor (app/src/actors/stream_actor.rs)
**Primary Responsibility**: Anduro Governance Node gRPC streaming

**Key Interactions**:
- Maintains bi-directional gRPC streams with governance nodes
- Routes governance messages to appropriate actors
- Handles federation coordination messages
- Manages governance protocol authentication
- Provides governance event subscriptions

### StorageActor (app/src/actors/storage_actor.rs)
**Primary Responsibility**: Database operations and persistent state

**Key Interactions**:
- Stores blockchain data received from ChainActor
- Provides historical data queries for SyncActor
- Manages state snapshots and checkpoints
- Handles database migrations and maintenance
- Provides backup and recovery operations

### SupervisorActor (app/src/actors/supervisor.rs)
**Primary Responsibility**: Root supervision and fault tolerance

**Key Interactions**:
- Monitors health of all child actors
- Implements restart strategies on actor failures
- Manages system-wide configuration updates
- Coordinates graceful shutdown procedures
- Provides system metrics and health reporting

## Message Flow Patterns

### 1. Block Production Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│ ChainActor  │───▶│ EngineActor  │───▶│ ChainActor  │───▶│ NetworkActor │
│             │    │              │    │             │    │              │
│ Request     │    │ Build block  │    │ Finalize    │    │ Propagate    │
│ block       │    │ template     │    │ block       │    │ block        │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────────┘
```

**Message Types**:
- `BuildBlockRequest` → EngineActor
- `BlockTemplate` → ChainActor  
- `FinalizedBlock` → NetworkActor
- `BlockPropagation` → Peers

### 2. Peg-In Operation Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│ BridgeActor │───▶│ StreamActor  │───▶│ ChainActor  │───▶│ EngineActor  │
│             │    │              │    │             │    │              │
│ Detect      │    │ Governance   │    │ Validate    │    │ Mint tokens  │
│ Bitcoin TX  │    │ approval     │    │ peg-in      │    │ on Alys      │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────────┘
```

**Message Types**:
- `BitcoinTransactionDetected` → StreamActor
- `GovernanceApprovalRequest` → Governance nodes
- `PegInValidationRequest` → ChainActor
- `MintTokensRequest` → EngineActor

### 3. Peg-Out Operation Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│ EngineActor │───▶│ BridgeActor  │───▶│ StreamActor │───▶│ BridgeActor  │
│             │    │              │    │             │    │              │
│ Burn event  │    │ Create       │    │ Federation  │    │ Broadcast    │
│ detected    │    │ Bitcoin TX   │    │ signatures  │    │ Bitcoin TX   │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────────┘
```

**Message Types**:
- `BurnEventDetected` → BridgeActor
- `CreatePegOutTransaction` → Internal
- `RequestFederationSignatures` → StreamActor
- `BroadcastBitcoinTransaction` → Bitcoin network

### 4. Sync Recovery Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│ ChainActor  │───▶│ SyncActor    │───▶│ NetworkActor│───▶│ ChainActor   │
│             │    │              │    │             │    │              │
│ Behind      │    │ Request      │    │ Download    │    │ Import       │
│ detected    │    │ blocks       │    │ from peers  │    │ blocks       │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────────┘
```

**Message Types**:
- `SyncRequiredNotification` → SyncActor
- `ParallelBlockDownloadRequest` → NetworkActor
- `ValidatedBlockBatch` → ChainActor
- `BlockImportRequest` → Internal

## Actor Communication Patterns

### 1. Request-Response Pattern
Used for operations requiring acknowledgment or return data.

```rust
// Sender
let response = chain_actor
    .send(BuildBlockRequest { slot: 12345 })
    .await?;

// Receiver
impl Handler<BuildBlockRequest> for EngineActor {
    type Result = ResponseActFuture<Self, Result<BlockTemplate, EngineError>>;
    
    fn handle(&mut self, msg: BuildBlockRequest, _ctx: &mut Context<Self>) -> Self::Result {
        // Process request and return response
    }
}
```

### 2. Fire-and-Forget Pattern
Used for notifications and events that don't require responses.

```rust
// Sender
network_actor.do_send(PropagateBlock { 
    block: finalized_block 
});

// Receiver
impl Handler<PropagateBlock> for NetworkActor {
    type Result = ();
    
    fn handle(&mut self, msg: PropagateBlock, _ctx: &mut Context<Self>) -> Self::Result {
        // Process notification
    }
}
```

### 3. Stream Pattern
Used for continuous data flows and subscriptions.

```rust
// StreamActor governance subscription
impl StreamHandler<GovernanceMessage> for StreamActor {
    fn handle(&mut self, msg: GovernanceMessage, ctx: &mut Context<Self>) {
        match msg.payload {
            GovernancePayload::BlockProposal(block) => {
                // Route to ChainActor
                self.chain_actor.do_send(GovernanceBlockProposal { block });
            }
            GovernancePayload::FederationUpdate(update) => {
                // Route to BridgeActor
                self.bridge_actor.do_send(FederationConfigUpdate { update });
            }
        }
    }
}
```

### 4. Supervision Pattern
Used for fault tolerance and actor lifecycle management.

```rust
impl Supervisor for SupervisorActor {
    fn decide(&self, error: &ActorError) -> SupervisionDecision {
        match error {
            ActorError::Network(_) => SupervisionDecision::Restart,
            ActorError::Configuration(_) => SupervisionDecision::Stop,
            ActorError::Temporary(_) => SupervisionDecision::Resume,
            _ => SupervisionDecision::Escalate,
        }
    }
}
```

## Actor State Management

### State Isolation Principles
1. **No Shared Mutable State**: Each actor owns its state completely
2. **Message-Only Communication**: Actors interact only through messages
3. **Async by Default**: All actor operations are asynchronous
4. **Fault Isolation**: Actor failures don't cascade to other actors

### State Persistence Patterns
```rust
impl StorageActor {
    async fn save_blockchain_state(&self, state: BlockchainState) -> Result<(), StorageError> {
        // Atomic state persistence
        let transaction = self.db.begin_transaction().await?;
        transaction.save_state(state).await?;
        transaction.commit().await?;
        Ok(())
    }
}
```

## Error Handling and Recovery

### Error Propagation
```rust
// Errors are contained within actor boundaries
impl Handler<ProcessTransaction> for EngineActor {
    type Result = ResponseFuture<Result<TransactionResult, EngineError>>;
    
    fn handle(&mut self, msg: ProcessTransaction, _ctx: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            match self.execution_client.process_transaction(msg.transaction).await {
                Ok(result) => Ok(result),
                Err(e) => {
                    // Log error locally, don't crash system
                    error!("Transaction processing failed: {}", e);
                    Err(EngineError::TransactionFailed { reason: e.to_string() })
                }
            }
        })
    }
}
```

### Restart Strategies
- **Immediate Restart**: For temporary failures (network timeouts)
- **Exponential Backoff**: For recurring failures (external service issues)  
- **Circuit Breaker**: For cascading failures (dependency unavailable)
- **Escalation**: For configuration or logic errors

## Performance Considerations

### Message Batching
```rust
// Batch similar operations for efficiency
impl Handler<BatchBlockImport> for ChainActor {
    fn handle(&mut self, msg: BatchBlockImport, _ctx: &mut Context<Self>) -> Self::Result {
        // Process multiple blocks atomically
        for block in msg.blocks {
            self.import_block(block)?;
        }
        // Single checkpoint update
        self.update_checkpoint().await?;
    }
}
```

### Backpressure Management
```rust
// Use bounded channels to prevent memory exhaustion  
impl SyncActor {
    fn configure_mailbox() -> MailboxConfig {
        MailboxConfig {
            capacity: 1000,
            backpressure_strategy: BackpressureStrategy::DropOldest,
        }
    }
}
```

## Testing Patterns

### Actor Unit Testing
```rust
#[tokio::test]
async fn test_chain_actor_block_processing() {
    let (chain_actor, _) = ChainActor::start_in_test_context().await;
    
    let response = chain_actor
        .send(ProcessBlockRequest { 
            block: create_test_block() 
        })
        .await
        .unwrap();
        
    assert!(response.is_ok());
}
```

### Integration Testing
```rust
#[tokio::test]
async fn test_peg_in_workflow() {
    let system = TestActorSystem::new().await;
    let bitcoin_tx = create_test_bitcoin_transaction();
    
    // Inject Bitcoin transaction detection
    system.bridge_actor.do_send(BitcoinTransactionDetected { 
        tx: bitcoin_tx 
    });
    
    // Verify tokens minted on Alys side
    let balance = system.engine_actor
        .send(GetBalance { address: recipient })
        .await
        .unwrap();
        
    assert_eq!(balance, expected_amount);
}
```

## Migration from V1 Patterns

### Before (V1 - Arc<RwLock<>>)
```rust
// V1 - Deadlock prone
pub struct Chain {
    engine: Arc<RwLock<Engine>>,
    bridge: Arc<RwLock<Bridge>>,
    network: Arc<RwLock<Network>>,
}

impl Chain {
    pub async fn process_block(&self, block: Block) -> Result<(), Error> {
        let engine = self.engine.write().await; // Lock 1
        let bridge = self.bridge.write().await; // Lock 2 - potential deadlock
        // Process with both locks held
    }
}
```

### After (V2 - Actor Messages)
```rust
// V2 - Deadlock free
impl Handler<ProcessBlock> for ChainActor {
    fn handle(&mut self, msg: ProcessBlock, _ctx: &mut Context<Self>) -> Self::Result {
        let engine_actor = self.engine_actor.clone();
        let bridge_actor = self.bridge_actor.clone();
        
        Box::pin(async move {
            // Sequential message passing - no locks
            let execution_result = engine_actor
                .send(ExecuteBlock { block: msg.block })
                .await?;
                
            let validation_result = bridge_actor
                .send(ValidatePegOperations { block: msg.block })
                .await?;
                
            // Combine results without holding locks
        })
    }
}
```

This actor-based approach provides:
- **Deadlock Prevention**: No shared locks between components
- **Fault Isolation**: Component failures don't cascade  
- **Scalability**: True parallelism without lock contention
- **Maintainability**: Clear component boundaries and responsibilities
- **Testability**: Easy to mock and test individual components