# StreamActor Architecture Diagrams

## System Overview

```mermaid
graph TB
    subgraph "Alys Node"
        subgraph "Actor System"
            SA[StreamActor]
            BA[BridgeActor]
            CA[ChainActor]
            NA[NetworkActor]
            STA[StorageActor]
            SYA[SyncActor]
            SUP[Supervisor]
        end
        
        subgraph "Integration Layer"
            BC[Bitcoin Client]
            EC[Execution Client]
            MS[Metrics System]
            CS[Config System]
        end
    end
    
    subgraph "External Systems"
        subgraph "Anduro Governance"
            GN1[Governance Node 1]
            GN2[Governance Node 2]
            GN3[Governance Node N]
        end
        
        subgraph "Blockchain Networks"
            BTC[Bitcoin Network]
            ALYS[Alys Network]
        end
        
        subgraph "Monitoring"
            PROM[Prometheus]
            GRAF[Grafana]
            ALERT[Alerting]
        end
    end
    
    SA <--> GN1
    SA <--> GN2
    SA <--> GN3
    SA --> BA
    SA --> CA
    SA --> NA
    SA --> SYA
    SA --> STA
    SUP --> SA
    
    BA --> BC
    CA --> EC
    NA --> ALYS
    STA --> CS
    
    SA --> MS
    MS --> PROM
    PROM --> GRAF
    PROM --> ALERT
```

## StreamActor Internal Architecture

```mermaid
graph TB
    subgraph "StreamActor Core"
        AM[Actor Manager]
        SM[State Manager]
        MM[Message Manager]
        HM[Health Monitor]
        
        subgraph "Connection Management"
            CM[Connection Manager]
            RM[Reconnect Manager]
            LB[Load Balancer]
        end
        
        subgraph "Protocol Layer"
            PH[Protocol Handler]
            AUTH[Auth Manager]
            SER[Serializer]
            COMP[Compressor]
        end
        
        subgraph "Message System"
            BUF[Message Buffer]
            RT[Router]
            PQ[Priority Queue]
            DLQ[Dead Letter Queue]
        end
        
        subgraph "Observability"
            MET[Metrics Collector]
            TR[Tracer]
            LOG[Logger]
        end
    end
    
    AM --> SM
    AM --> MM
    AM --> HM
    
    MM --> CM
    MM --> BUF
    MM --> RT
    
    CM --> RM
    CM --> LB
    CM --> PH
    
    PH --> AUTH
    PH --> SER
    PH --> COMP
    
    BUF --> PQ
    RT --> DLQ
    
    HM --> MET
    HM --> TR
    HM --> LOG
```

## Message Flow Architecture

```mermaid
sequenceDiagram
    participant Client as Client Actor
    participant SA as StreamActor
    participant CM as Connection Manager
    participant PH as Protocol Handler
    participant BUF as Message Buffer
    participant GN as Governance Node
    
    Client->>SA: Send Message
    SA->>BUF: Buffer Message
    SA->>CM: Check Connection
    
    alt Connection Available
        CM->>PH: Send via Protocol
        PH->>GN: gRPC Stream
        GN-->>PH: gRPC Response
        PH-->>SA: Response Message
        SA-->>Client: Forward Response
    else Connection Unavailable
        CM->>CM: Attempt Reconnection
        BUF->>BUF: Hold Message
        Note over BUF: Message held until reconnection
    end
    
    CM->>SA: Connection Restored
    SA->>BUF: Flush Buffer
    BUF->>PH: Replay Messages
    PH->>GN: Send Buffered Messages
```

## Connection State Machine

```mermaid
stateDiagram-v2
    [*] --> Disconnected
    
    Disconnected --> Connecting : Establish Connection
    Connecting --> Authenticating : TCP Connected
    Authenticating --> Connected : Auth Success
    
    Connected --> Streaming : Start gRPC Stream
    Streaming --> Healthy : Stream Active
    
    Healthy --> Warning : Minor Issues
    Warning --> Healthy : Issues Resolved
    Warning --> Critical : Issues Escalate
    
    Critical --> Reconnecting : Connection Lost
    Reconnecting --> Connecting : Retry Connection
    
    Authenticating --> Failed : Auth Failed
    Connecting --> Failed : Connection Failed
    Failed --> Connecting : Backoff Expired
    
    Connected --> Suspended : Governance Suspend
    Suspended --> Connected : Resume Command
    
    Streaming --> Connected : Stream Closed
    Healthy --> Streaming : Stream Restart
```

## Actor Supervision Hierarchy

```mermaid
graph TD
    ROOT[Root Supervisor]
    
    ROOT --> SYS_SUP[System Supervisor]
    ROOT --> NET_SUP[Network Supervisor]
    ROOT --> STOR_SUP[Storage Supervisor]
    
    SYS_SUP --> SA[StreamActor]
    SYS_SUP --> BA[BridgeActor]
    SYS_SUP --> CA[ChainActor]
    
    NET_SUP --> NA[NetworkActor]
    NET_SUP --> SYA[SyncActor]
    
    STOR_SUP --> STA[StorageActor]
    
    SA --> CONN1[Connection 1]
    SA --> CONN2[Connection 2]
    SA --> CONN3[Connection N]
    
    CONN1 --> STREAM1[gRPC Stream 1]
    CONN2 --> STREAM2[gRPC Stream 2]
    CONN3 --> STREAMN[gRPC Stream N]
```

## Data Flow Patterns

```mermaid
graph LR
    subgraph "Inbound Flow"
        GN[Governance Node] --> GP[gRPC Protocol]
        GP --> DES[Deserializer]
        DES --> VAL[Validator]
        VAL --> RT_IN[Router]
        RT_IN --> TARGET[Target Actor]
    end
    
    subgraph "Outbound Flow"
        SOURCE[Source Actor] --> RT_OUT[Router]
        RT_OUT --> PQ[Priority Queue]
        PQ --> BUF[Buffer]
        BUF --> SER[Serializer]
        SER --> GP_OUT[gRPC Protocol]
        GP_OUT --> GN_OUT[Governance Node]
    end
    
    subgraph "Error Flow"
        ERR[Error Source] --> EH[Error Handler]
        EH --> REC[Recovery Logic]
        REC --> RETRY[Retry Queue]
        RETRY --> RT_ERR[Router]
        RT_ERR --> DLQ[Dead Letter Queue]
    end
```

## Load Balancing Strategy

```mermaid
graph TB
    subgraph "Load Balancer"
        LB[Load Balancer]
        subgraph "Selection Strategies"
            RR[Round Robin]
            PRIO[Priority Based]
            LAT[Latency Based]
            LC[Least Connections]
            WRR[Weighted Round Robin]
        end
    end
    
    subgraph "Governance Endpoints"
        EP1[Endpoint 1<br/>Priority: 100<br/>Region: US-East]
        EP2[Endpoint 2<br/>Priority: 90<br/>Region: US-West]
        EP3[Endpoint 3<br/>Priority: 80<br/>Region: EU-West]
        EP4[Endpoint 4<br/>Priority: 70<br/>Region: Asia-Pacific]
    end
    
    LB --> RR
    LB --> PRIO
    LB --> LAT
    LB --> LC
    LB --> WRR
    
    RR --> EP1
    RR --> EP2
    RR --> EP3
    RR --> EP4
    
    PRIO --> EP1
    PRIO --> EP2
    
    LAT --> EP1
    LAT --> EP3
    
    LC --> EP2
    LC --> EP4
```

## Security Architecture

```mermaid
graph TB
    subgraph "Security Layers"
        subgraph "Network Security"
            TLS[TLS 1.3 Encryption]
            CERT[Certificate Validation]
            PIN[Certificate Pinning]
        end
        
        subgraph "Authentication"
            BEARER[Bearer Token]
            MTLS[Mutual TLS]
            SIG[Digital Signature]
            API[API Key]
        end
        
        subgraph "Authorization"
            ACL[Access Control Lists]
            RATE[Rate Limiting]
            FILTER[Message Filtering]
        end
        
        subgraph "Audit & Monitoring"
            LOG[Audit Logging]
            MON[Security Monitoring]
            ALERT[Threat Detection]
        end
    end
    
    subgraph "Data Flow"
        REQ[Request] --> TLS
        TLS --> CERT
        CERT --> PIN
        PIN --> BEARER
        BEARER --> ACL
        ACL --> RATE
        RATE --> FILTER
        FILTER --> PROC[Process Request]
        
        PROC --> LOG
        PROC --> MON
        MON --> ALERT
    end
```

## Performance Monitoring

```mermaid
graph TB
    subgraph "Metrics Collection"
        APP[Application Metrics]
        SYS[System Metrics]
        NET[Network Metrics]
        BUS[Business Metrics]
    end
    
    subgraph "Processing Pipeline"
        COLL[Metrics Collector]
        AGG[Aggregator]
        STORE[Time Series DB]
        ALERT[Alert Manager]
    end
    
    subgraph "Visualization"
        DASH[Dashboards]
        REPORT[Reports]
        NOTIFY[Notifications]
    end
    
    APP --> COLL
    SYS --> COLL
    NET --> COLL
    BUS --> COLL
    
    COLL --> AGG
    AGG --> STORE
    STORE --> ALERT
    
    STORE --> DASH
    STORE --> REPORT
    ALERT --> NOTIFY
    
    subgraph "Key Metrics"
        CONN[Active Connections]
        MSG[Messages/Second]
        LAT[Latency P99]
        ERR[Error Rate]
        MEM[Memory Usage]
        CPU[CPU Usage]
    end
    
    CONN --> APP
    MSG --> APP
    LAT --> NET
    ERR --> APP
    MEM --> SYS
    CPU --> SYS
```

## Deployment Architecture

```mermaid
graph TB
    subgraph "Production Environment"
        subgraph "Load Balancer Tier"
            LB1[Load Balancer 1]
            LB2[Load Balancer 2]
        end
        
        subgraph "Application Tier"
            NODE1[Alys Node 1<br/>Primary]
            NODE2[Alys Node 2<br/>Secondary]
            NODE3[Alys Node 3<br/>Observer]
        end
        
        subgraph "Data Tier"
            DB1[Database 1<br/>Master]
            DB2[Database 2<br/>Replica]
            CACHE[Redis Cache]
        end
        
        subgraph "Monitoring Tier"
            PROM[Prometheus]
            GRAF[Grafana]
            ALERT[AlertManager]
        end
    end
    
    subgraph "External Services"
        GOV1[Governance Node 1]
        GOV2[Governance Node 2]
        GOV3[Governance Node 3]
    end
    
    LB1 --> NODE1
    LB1 --> NODE2
    LB2 --> NODE2
    LB2 --> NODE3
    
    NODE1 --> DB1
    NODE2 --> DB2
    NODE3 --> DB2
    
    NODE1 --> CACHE
    NODE2 --> CACHE
    NODE3 --> CACHE
    
    NODE1 --> GOV1
    NODE1 --> GOV2
    NODE2 --> GOV2
    NODE2 --> GOV3
    NODE3 --> GOV1
    NODE3 --> GOV3
    
    NODE1 --> PROM
    NODE2 --> PROM
    NODE3 --> PROM
    
    PROM --> GRAF
    PROM --> ALERT
```

## Configuration Management

```mermaid
graph LR
    subgraph "Configuration Sources"
        FILE[Config Files<br/>YAML/TOML/JSON]
        ENV[Environment Variables]
        CLI[Command Line Args]
        REMOTE[Remote Config Service]
    end
    
    subgraph "Configuration System"
        LOADER[Config Loader]
        MERGER[Config Merger]
        VALIDATOR[Validator]
        WATCHER[Hot Reload Watcher]
    end
    
    subgraph "Configuration Consumers"
        ACTOR[StreamActor]
        PROTO[Protocol Layer]
        CONN[Connection Manager]
        AUTH[Auth Manager]
    end
    
    FILE --> LOADER
    ENV --> LOADER
    CLI --> LOADER
    REMOTE --> LOADER
    
    LOADER --> MERGER
    MERGER --> VALIDATOR
    VALIDATOR --> ACTOR
    VALIDATOR --> PROTO
    VALIDATOR --> CONN
    VALIDATOR --> AUTH
    
    WATCHER --> LOADER
    REMOTE --> WATCHER
```

## Error Handling Flow

```mermaid
graph TB
    subgraph "Error Sources"
        CONN_ERR[Connection Errors]
        PROTO_ERR[Protocol Errors]
        AUTH_ERR[Auth Errors]
        SYS_ERR[System Errors]
    end
    
    subgraph "Error Processing"
        CATCH[Error Catcher]
        CLASS[Error Classifier]
        CTX[Context Enrichment]
        LOG[Error Logger]
    end
    
    subgraph "Recovery Strategies"
        RETRY[Retry Logic]
        FB[Fallback]
        CB[Circuit Breaker]
        DEGRADE[Graceful Degradation]
    end
    
    subgraph "Escalation"
        ALERT[Alert System]
        SUPER[Supervisor]
        HUMAN[Human Intervention]
    end
    
    CONN_ERR --> CATCH
    PROTO_ERR --> CATCH
    AUTH_ERR --> CATCH
    SYS_ERR --> CATCH
    
    CATCH --> CLASS
    CLASS --> CTX
    CTX --> LOG
    
    LOG --> RETRY
    LOG --> FB
    LOG --> CB
    LOG --> DEGRADE
    
    RETRY --> ALERT
    FB --> ALERT
    CB --> SUPER
    DEGRADE --> HUMAN
```