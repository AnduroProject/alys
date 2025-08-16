# Alys V2 Actor Communication Flow Diagrams

## System Overview Architecture

```mermaid
graph TB
    subgraph "Supervision Hierarchy"
        SV[SupervisorActor<br/>Root Supervision] --> CA[ChainActor<br/>Consensus]
        SV --> EA[EngineActor<br/>EVM Execution]
        SV --> BA[BridgeActor<br/>Peg Operations]
        SV --> SA[SyncActor<br/>Blockchain Sync]
        SV --> NA[NetworkActor<br/>P2P Networking]
        SV --> ST[StreamActor<br/>Governance gRPC]
        SV --> StA[StorageActor<br/>Database]
    end
    
    subgraph "External Systems"
        GN[Anduro Governance<br/>Nodes]
        BC[Bitcoin Network]
        EP[Ethereum Peers]
        DB[(Database)]
    end
    
    CA <--> EA
    CA <--> BA
    CA <--> SA
    CA <--> NA
    BA <--> ST
    ST <--> GN
    BA <--> BC
    NA <--> EP
    StA <--> DB
    
    style SV fill:#ff9999
    style CA fill:#99ccff
    style EA fill:#99ffcc
    style BA fill:#ffcc99
    style SA fill:#cc99ff
    style NA fill:#ffff99
    style ST fill:#ff99cc
    style StA fill:#99ff99
```

## 1. Block Production Flow

```mermaid
sequenceDiagram
    participant Timer as Aura Timer
    participant CA as ChainActor
    participant EA as EngineActor
    participant BA as BridgeActor
    participant NA as NetworkActor
    participant StA as StorageActor

    Timer->>CA: SlotTick(slot=123)
    
    Note over CA: Check if this node<br/>is slot authority
    
    CA->>EA: BuildBlockRequest
    Note over EA: Collect transactions<br/>from mempool
    EA-->>CA: BlockTemplate
    
    CA->>BA: ValidatePegOperations
    Note over BA: Verify peg-in/out<br/>transactions
    BA-->>CA: PegValidationResult
    
    Note over CA: Apply Aura consensus<br/>and create signed block
    
    CA->>NA: PropagateBlock
    Note over NA: Broadcast to<br/>libp2p peers
    
    CA->>StA: PersistBlock
    Note over StA: Save to database<br/>atomically
    
    Note over CA: Block finalized<br/>and committed
```

## 2. Bitcoin Peg-In Operation Flow

```mermaid
sequenceDiagram
    participant BC as Bitcoin Network
    participant BA as BridgeActor
    participant ST as StreamActor
    participant GN as Governance Nodes
    participant CA as ChainActor
    participant EA as EngineActor

    BC->>BA: BitcoinTransactionDetected
    Note over BA: Monitor federation<br/>multisig addresses
    
    BA->>BA: ValidatePegInTransaction
    Note over BA: Check confirmations<br/>and amount
    
    BA->>ST: GovernanceApprovalRequest
    ST->>GN: RequestFederationApproval
    Note over GN: Federation members<br/>vote on peg-in
    
    GN-->>ST: ApprovalResponse(approved=true)
    ST-->>BA: GovernanceApproval
    
    BA->>CA: PegInOperation
    Note over CA: Create peg-in<br/>consensus operation
    
    CA->>EA: MintTokensRequest
    Note over EA: Mint corresponding<br/>Alys tokens
    EA-->>CA: MintResult(success=true)
    
    CA->>BA: PegInComplete
    Note over BA: Update UTXO<br/>tracking
```

## 3. Ethereum Peg-Out Operation Flow

```mermaid
sequenceDiagram
    participant User as User/DApp
    participant EA as EngineActor
    participant BA as BridgeActor
    participant ST as StreamActor
    participant GN as Governance Nodes
    participant BC as Bitcoin Network

    User->>EA: BurnTransaction
    Note over EA: Burn tokens to<br/>0x000...dead address
    
    EA->>BA: BurnEventDetected
    Note over BA: Parse burn event<br/>for Bitcoin address
    
    BA->>BA: CreateBitcoinTransaction
    Note over BA: Build unsigned<br/>Bitcoin transaction
    
    BA->>ST: RequestFederationSignatures
    ST->>GN: SignatureRequest
    Note over GN: Federation members<br/>sign with private keys
    
    GN-->>ST: SignatureResponse
    ST-->>BA: CollectedSignatures
    
    Note over BA: Aggregate signatures<br/>into final transaction
    
    BA->>BC: BroadcastBitcoinTransaction
    Note over BC: Transaction sent<br/>to Bitcoin network
    
    BC-->>BA: TransactionConfirmed
    Note over BA: Update operation<br/>status to completed
```

## 4. Blockchain Sync Recovery Flow

```mermaid
sequenceDiagram
    participant CA as ChainActor
    participant SA as SyncActor
    participant NA as NetworkActor
    participant Peer1 as Peer A
    participant Peer2 as Peer B
    participant Peer3 as Peer C
    participant StA as StorageActor

    CA->>SA: SyncRequiredNotification
    Note over CA: Detected we are<br/>behind best chain
    
    SA->>NA: GetConnectedPeers
    NA-->>SA: PeerList[A, B, C]
    
    par Parallel Block Downloads
        SA->>Peer1: RequestBlocks(1000-1100)
        SA->>Peer2: RequestBlocks(1101-1200)  
        SA->>Peer3: RequestBlocks(1201-1300)
    end
    
    par Receive Block Batches
        Peer1-->>SA: BlockBatch(1000-1100)
        Peer2-->>SA: BlockBatch(1101-1200)
        Peer3-->>SA: BlockBatch(1201-1300)
    end
    
    Note over SA: Validate blocks<br/>and check integrity
    
    SA->>CA: ValidatedBlockBatch
    Note over CA: Import blocks<br/>sequentially
    
    CA->>StA: PersistBlockBatch
    Note over StA: Atomic database<br/>transaction
    
    loop Until Synced
        SA->>SA: CheckSyncProgress
        alt More blocks needed
            SA->>NA: RequestMoreBlocks
        else Sync Complete
            SA->>CA: SyncCompleted
        end
    end
```

## 5. Governance Message Routing

```mermaid
sequenceDiagram
    participant GN as Governance Node
    participant ST as StreamActor
    participant CA as ChainActor
    participant BA as BridgeActor
    participant SA as SyncActor
    participant EA as EngineActor

    GN->>ST: GovernanceMessage
    Note over ST: Bi-directional<br/>gRPC stream
    
    alt BlockProposal
        ST->>CA: GovernanceBlockProposal
        Note over CA: Process governance<br/>proposed block
        
    else FederationUpdate
        ST->>BA: FederationConfigUpdate
        Note over BA: Update federation<br/>member list
        
    else ChainStatus
        ST->>CA: RequestChainStatus
        CA-->>ST: ChainStatusResponse
        ST->>GN: ChainStatusUpdate
        
    else SyncRequest
        ST->>SA: GovernanceSyncRequest
        Note over SA: Priority sync<br/>for governance
        
    else EmergencyHalt
        ST->>CA: EmergencyHaltRequest
        Note over CA: Pause block<br/>production immediately
        
    else ConfigUpdate
        ST->>EA: UpdateExecutionConfig
        Note over EA: Hot-reload<br/>configuration
    end
```

## 6. Actor Supervision and Fault Recovery

```mermaid
stateDiagram-v2
    [*] --> Initializing : Actor Start
    
    Initializing --> Running : Successful Init
    Initializing --> Failed : Init Error
    
    Running --> Failed : Actor Error
    Running --> Stopping : Shutdown Signal
    
    Failed --> Restarting : Restart Strategy
    Failed --> Terminated : Max Retries Exceeded
    
    Restarting --> Initializing : Restart Attempt
    Restarting --> Terminated : Restart Failed
    
    Stopping --> Terminated : Graceful Shutdown
    
    Terminated --> [*]
    
    note right of Failed
        Supervisor decides restart strategy:
        • Immediate: Network errors
        • Exponential backoff: Service errors
        • Circuit breaker: Cascading failures
        • Terminate: Logic errors
    end note
```

## 7. Message Type Categories

```mermaid
classDiagram
    class MessageEnvelope {
        +correlation_id: String
        +timestamp: SystemTime
        +sender: ActorPath
        +message_type: String
        +payload: T
    }
    
    class ChainMessages {
        +ProcessBlock
        +BuildBlockRequest
        +SlotTick
        +FinalizeBlock
    }
    
    class BridgeMessages {
        +PegInOperation
        +PegOutOperation  
        +BitcoinTransactionDetected
        +BurnEventDetected
    }
    
    class SyncMessages {
        +SyncRequiredNotification
        +ParallelBlockDownloadRequest
        +ValidatedBlockBatch
        +SyncCompleted
    }
    
    class SystemMessages {
        +ActorStarted
        +ActorStopped
        +HealthCheck
        +ConfigUpdate
    }
    
    MessageEnvelope --> ChainMessages
    MessageEnvelope --> BridgeMessages  
    MessageEnvelope --> SyncMessages
    MessageEnvelope --> SystemMessages
```

## 8. Actor State Machines

### ChainActor State Machine
```mermaid
stateDiagram-v2
    [*] --> Initializing
    
    Initializing --> Syncing : Genesis loaded
    Initializing --> Failed : Genesis error
    
    Syncing --> Active : Caught up to network
    Syncing --> Failed : Sync error
    
    Active --> Producing : Assigned slot
    Active --> Importing : Received block
    Active --> Syncing : Fell behind
    
    Producing --> Active : Block produced
    Producing --> Failed : Production error
    
    Importing --> Active : Block imported
    Importing --> Failed : Import error
    
    Failed --> Syncing : Recovery attempt
    Failed --> [*] : Terminal error
```

### BridgeActor State Machine
```mermaid
stateDiagram-v2
    [*] --> Initializing
    
    Initializing --> Monitoring : Connected to Bitcoin
    Initializing --> Failed : Connection error
    
    Monitoring --> ProcessingPegIn : Bitcoin TX detected
    Monitoring --> ProcessingPegOut : Burn event detected
    
    ProcessingPegIn --> WaitingApproval : Validation passed
    ProcessingPegIn --> Monitoring : Validation failed
    
    ProcessingPegOut --> CollectingSignatures : TX created
    ProcessingPegOut --> Monitoring : TX creation failed
    
    WaitingApproval --> Monitoring : Governance approved
    WaitingApproval --> Monitoring : Governance denied
    
    CollectingSignatures --> Broadcasting : Signatures collected
    CollectingSignatures --> Monitoring : Signature timeout
    
    Broadcasting --> Monitoring : TX broadcasted
    Broadcasting --> Failed : Broadcast error
    
    Failed --> Monitoring : Recovery
    Failed --> [*] : Terminal error
```

## Performance Characteristics

### Message Throughput Targets
- **ChainActor**: 1,000 messages/second (block production)
- **NetworkActor**: 10,000 messages/second (peer communication)  
- **BridgeActor**: 100 messages/second (peg operations)
- **SyncActor**: 5,000 messages/second (sync operations)
- **StorageActor**: 2,000 messages/second (database ops)

### Latency Requirements
- **Intra-actor messaging**: <1ms p99
- **Cross-actor messaging**: <5ms p99
- **External system calls**: <100ms p99
- **Database operations**: <10ms p99

### Backpressure Management
```mermaid
flowchart TD
    A[Message Producer] --> B{Mailbox Full?}
    B -->|No| C[Queue Message]
    B -->|Yes| D{Backpressure Strategy}
    
    D --> E[Drop Oldest]
    D --> F[Drop Newest]  
    D --> G[Block Producer]
    D --> H[Return Error]
    
    E --> I[Log Dropped Message]
    F --> I
    G --> J[Wait for Capacity]
    H --> K[Handle Error]
    
    I --> C
    J --> C
```

This communication flow architecture ensures:
- **Fault Isolation**: Actor failures don't cascade
- **Scalability**: Parallel message processing  
- **Maintainability**: Clear component boundaries
- **Observability**: Full message tracing and metrics
- **Reliability**: Comprehensive error handling and recovery