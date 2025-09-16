# V2 Architecture Overview: Lead Engineer Reference

## System Architecture Transformation

The Alys V2 architecture represents a complete paradigm shift from monolithic, shared-state design to a message-passing actor system. This document provides detailed architectural context for lead engineers.

## Core Architectural Principles

### 1. Actor Model Implementation
```
┌─────────────────────────────────────────────────────────────────┐
│                        AlysSystem                               │
│                    (Root Supervisor)                           │
└─────────────┬───────────────────────────────────────────────────┘
              │
    ┌─────────┼─────────┬─────────┬─────────┐
    │         │         │         │         │
┌───▼───┐ ┌──▼───┐ ┌───▼───┐ ┌───▼───┐ ┌──▼────┐
│Chain  │ │Bridge│ │Network│ │Storage│ │Metrics│
│Super  │ │Super │ │Super  │ │Super  │ │Super  │
│visor  │ │visor │ │visor  │ │visor  │ │visor  │
└───┬───┘ └──┬───┘ └───┬───┘ └───┬───┘ └──┬────┘
    │        │         │         │        │
┌───▼───────▼───────▼─────────▼─────────▼────┐
│              Message Bus                   │
│         (Event Distribution)               │
└────────────────────────────────────────────┘
```

### 2. Message Flow Architecture
Every interaction follows strict message-passing patterns:

```rust
// Actor Communication Pattern
actor_1.send(Message::Request(data))
    ↓
MessageBus routes to actor_2
    ↓  
actor_2 processes and responds
    ↓
MessageBus routes response back
    ↓
actor_1 receives Response::Success(result)
```

### 3. Supervision Tree Design
```
AlysSystem (OneForAll restart)
├── ChainSupervisor (OneForOne restart)
│   ├── ChainActor (ExponentialBackoff)
│   ├── EngineActor (CircuitBreaker) 
│   └── AuxPowActor (OneForOne)
├── NetworkSupervisor (RestForOne restart)
│   ├── NetworkActor (CircuitBreaker)
│   ├── SyncActor (ExponentialBackoff)
│   └── StreamActor (OneForOne)
├── BridgeSupervisor (OneForOne restart)
│   ├── BridgeActor (CircuitBreaker)
│   └── FederationActor (ExponentialBackoff)
└── StorageSupervisor (OneForOne restart)
    ├── StorageActor (OneForOne)
    └── MetricsActor (Never restart)
```

## Actor System Deep Dive

### Core Actor Framework (`crates/actor_system/`)

#### 1. AlysActor Trait (`actor.rs:15-89`)
```rust
#[async_trait]
pub trait AlysActor: Send + Sync + 'static {
    type Config: Clone + Send + Sync + 'static;
    type State: Send + Sync + 'static;
    type Message: AlysMessage + Send + Sync + 'static;
    type Error: std::error::Error + Send + Sync + 'static;

    /// Create new actor instance with configuration
    async fn new(config: Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Handle incoming message
    async fn handle_message(
        &mut self,
        message: Self::Message,
        context: &mut ActorContext<Self>,
    ) -> Result<(), Self::Error>;

    /// Actor lifecycle hooks
    async fn started(&mut self, ctx: &mut ActorContext<Self>) -> Result<(), Self::Error> { Ok(()) }
    async fn stopped(&mut self, ctx: &mut ActorContext<Self>) -> Result<(), Self::Error> { Ok(()) }
    
    /// Health check implementation
    async fn health_check(&self) -> ActorHealth { ActorHealth::Healthy }
    
    /// Metrics collection
    fn metrics(&self) -> ActorMetrics { ActorMetrics::default() }
}
```

#### 2. Supervision System (`supervisor.rs:23-156`)
```rust
pub enum SupervisionStrategy {
    /// Restart only the failed actor
    OneForOne {
        max_retries: u32,
        within_time: Duration,
    },
    /// Restart all sibling actors when one fails
    OneForAll {
        max_retries: u32,
        within_time: Duration,
    },
    /// Restart the failed actor and all actors started after it
    RestForOne {
        max_retries: u32,
        within_time: Duration,
    },
    /// Exponential backoff restart strategy
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        max_retries: u32,
    },
    /// Circuit breaker pattern for external service failures
    CircuitBreaker {
        failure_threshold: u32,
        recovery_timeout: Duration,
        success_threshold: u32,
    },
    /// Never restart (for critical actors that require manual intervention)
    Never,
}
```

#### 3. Mailbox System (`mailbox.rs:18-234`)
```rust
pub struct ActorMailbox<M: AlysMessage> {
    /// Message queue with configurable capacity
    receiver: UnboundedReceiver<MessageEnvelope<M>>,
    sender: UnboundedSender<MessageEnvelope<M>>,
    
    /// Backpressure handling configuration
    backpressure_strategy: BackpressureStrategy,
    capacity: usize,
    
    /// Priority queue for high-priority messages
    priority_queue: Option<BinaryHeap<PriorityMessage<M>>>,
    
    /// Dead letter queue for undeliverable messages
    dead_letter_queue: DeadLetterQueue<M>,
    
    /// Message batching configuration
    batch_config: Option<MessageBatchConfig>,
}

pub enum BackpressureStrategy {
    /// Drop oldest messages when queue is full
    DropOldest,
    /// Drop newest messages when queue is full  
    DropNewest,
    /// Block sender until queue has space
    Block,
    /// Return error to sender when queue is full
    Fail,
}
```

## Configuration Architecture Deep Dive

### Master Configuration System (`app/src/config/alys_config.rs`)

#### Configuration Hierarchy
```rust
pub struct AlysConfig {
    /// Environment configuration (Development, Staging, Production)
    pub environment: Environment,
    
    /// System-wide settings (runtime, logging, monitoring)
    pub system: SystemConfig,
    
    /// Actor system configuration (supervision, mailboxes, timeouts)
    pub actors: ActorSystemConfig,
    
    /// Chain and consensus configuration
    pub chain: ChainConfig,
    
    /// Network and P2P configuration  
    pub network: NetworkConfig,
    
    /// Bridge and peg operations configuration
    pub bridge: BridgeConfig,
    
    /// Storage and database configuration
    pub storage: StorageConfig,
    
    /// Governance integration configuration
    pub governance: GovernanceConfig,
    
    /// Sync engine configuration
    pub sync: SyncConfig,
    
    /// Monitoring and metrics configuration
    pub monitoring: MonitoringConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
}
```

#### Layered Loading System (`alys_config.rs:670-696`)
```rust
impl AlysConfig {
    pub async fn load() -> Result<Self, ConfigError> {
        let mut config = Self::default(); // 1. Start with defaults
        
        // 2. Load from configuration files
        if let Ok(file_config) = Self::load_from_file("alys.toml").await {
            config = config.merge(file_config)?;
        }
        
        // 3. Override with environment variables
        config = config.apply_environment_overrides()?;
        
        // 4. Apply command line arguments (future)
        // config = config.apply_cli_overrides(args)?;
        
        // 5. Validate final configuration
        config.validate()?;
        
        Ok(config)
    }
}
```

### Hot-Reload System (`app/src/config/hot_reload.rs`)

#### File Watching Architecture
```rust
pub struct ConfigReloadManager {
    /// Current active configuration
    current_config: Arc<RwLock<AlysConfig>>,
    
    /// File system watcher for configuration files
    watcher: Arc<RwLock<Option<notify::RecommendedWatcher>>>,
    
    /// Actor notification system for config changes  
    actor_notifier: ActorNotificationSystem,
    
    /// State preservation manager
    state_preservation: StatePreservationManager,
    
    /// Automatic rollback on validation failures
    rollback_manager: RollbackManager,
}

impl ConfigReloadManager {
    /// Process configuration file changes
    async fn handle_file_change(&self, path: PathBuf) -> Result<(), ReloadError> {
        // 1. Load new configuration from file
        let new_config = AlysConfig::load_from_file(&path).await?;
        
        // 2. Validate new configuration
        new_config.validate()?;
        
        // 3. Determine which actors are affected
        let affected_actors = self.analyze_impact(&new_config).await?;
        
        // 4. Preserve state for affected actors
        self.state_preservation.preserve_state(&affected_actors).await?;
        
        // 5. Apply new configuration
        *self.current_config.write().await = new_config;
        
        // 6. Notify affected actors
        self.actor_notifier.notify_actors(&affected_actors).await?;
        
        Ok(())
    }
}
```

## Integration Architecture

### External System Integration Pattern

All external system integrations follow a consistent pattern:

```rust
// 1. Trait Definition (interface abstraction)
#[async_trait]
pub trait GovernanceIntegration: Send + Sync {
    async fn connect(&self, endpoint: String) -> Result<ConnectionHandle, SystemError>;
    async fn send_block_proposal(&self, block: ConsensusBlock) -> Result<(), SystemError>;
    // ... other methods
}

// 2. Concrete Implementation
pub struct GovernanceClient {
    config: GovernanceConfig,
    connection_pool: Arc<RwLock<ConnectionPool>>,
    metrics: Arc<GovernanceMetrics>,
}

// 3. Factory for Configuration-Driven Creation  
pub struct GovernanceClientFactory;
impl GovernanceClientFactory {
    pub async fn create(config: &GovernanceConfig) -> Result<GovernanceClient, SystemError> {
        // Configuration-driven client creation
    }
}

// 4. Actor Integration
impl StreamActor {
    async fn handle_governance_message(&mut self, msg: GovernanceMessage) -> Result<(), ActorError> {
        // Use integration client through trait
        self.governance_client.send_block_proposal(msg.block).await?;
        Ok(())
    }
}
```

### Bitcoin Integration Deep Dive (`app/src/integration/bitcoin.rs`)

#### Advanced UTXO Management
```rust
pub struct UtxoManager {
    /// Available UTXOs with metadata
    available_utxos: BTreeMap<OutPoint, UtxoInfo>,
    
    /// Reserved UTXOs (temporarily locked for transactions)
    reserved_utxos: HashMap<String, Vec<OutPoint>>, // reservation_id -> utxos
    
    /// UTXO selection strategies
    selection_strategy: UtxoSelectionStrategy,
}

pub enum UtxoSelectionStrategy {
    /// Select largest UTXOs first (minimize inputs)
    LargestFirst,
    /// Select smallest UTXOs first (minimize change)
    SmallestFirst,  
    /// Branch and bound algorithm for exact amounts
    BranchAndBound,
    /// Minimize transaction fees
    MinimizeFee,
}

impl UtxoManager {
    pub async fn reserve_utxos(
        &mut self,
        amount_needed: u64,
        reserved_by: String,
        purpose: String,
    ) -> Result<Vec<UtxoInfo>, BridgeError> {
        // Sophisticated UTXO selection logic
        let selected_utxos = match self.selection_strategy {
            UtxoSelectionStrategy::BranchAndBound => {
                self.branch_and_bound_selection(amount_needed)?
            }
            UtxoSelectionStrategy::LargestFirst => {
                self.largest_first_selection(amount_needed)?
            }
            // ... other strategies
        };
        
        // Reserve selected UTXOs
        self.reserved_utxos.insert(reserved_by, selected_utxos.clone());
        
        Ok(selected_utxos)
    }
}
```

### Execution Client Abstraction (`app/src/integration/execution.rs`)

#### Unified Geth/Reth Interface
```rust
pub enum ExecutionClientType {
    Geth(GethClient),
    Reth(RethClient),
}

impl ExecutionIntegration for ExecutionClientType {
    async fn get_block(&self, block_number: u64) -> Result<Block, EngineError> {
        match self {
            ExecutionClientType::Geth(client) => client.get_block(block_number).await,
            ExecutionClientType::Reth(client) => client.get_block(block_number).await,
        }
    }
    
    async fn send_transaction(&self, tx: Transaction) -> Result<TxHash, EngineError> {
        match self {
            ExecutionClientType::Geth(client) => client.send_transaction(tx).await,
            ExecutionClientType::Reth(client) => client.send_transaction(tx).await,
        }
    }
}

// Multi-level caching system
pub struct ExecutionClientCache {
    /// Block cache (most frequently accessed)
    block_cache: LruCache<u64, Block>,
    
    /// Transaction cache
    transaction_cache: LruCache<TxHash, Transaction>,
    
    /// Receipt cache
    receipt_cache: LruCache<TxHash, TransactionReceipt>,
    
    /// Account state cache  
    account_cache: LruCache<Address, AccountState>,
    
    /// Cache statistics for optimization
    cache_stats: CacheStatistics,
}
```

## Message System Architecture

### Message Envelope System (`crates/actor_system/message.rs`)

#### Universal Message Wrapper
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope<T: AlysMessage> {
    /// Unique message identifier
    pub message_id: MessageId,
    
    /// Correlation ID for request/response tracking
    pub correlation_id: Option<CorrelationId>,
    
    /// Message routing information
    pub routing: MessageRouting,
    
    /// The actual message payload
    pub payload: T,
    
    /// Message metadata and context
    pub metadata: MessageMetadata,
    
    /// Message priority (for priority queues)
    pub priority: MessagePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Timestamp when message was created
    pub created_at: SystemTime,
    
    /// Source actor that sent the message
    pub from_actor: ActorId,
    
    /// Destination actor (if point-to-point)
    pub to_actor: Option<ActorId>,
    
    /// Distributed tracing context
    pub trace_context: Option<TraceContext>,
    
    /// Message retry information
    pub retry_count: u32,
    pub max_retries: u32,
    
    /// Timeout information
    pub timeout: Option<Duration>,
}
```

#### Message Bus Implementation (`crates/actor_system/bus.rs`)
```rust
pub struct MessageBus {
    /// Actor registry for message routing
    actor_registry: Arc<RwLock<ActorRegistry>>,
    
    /// Message routing table
    routing_table: Arc<RwLock<RoutingTable>>,
    
    /// Event subscribers (for broadcast messages)
    subscribers: Arc<RwLock<HashMap<EventType, Vec<ActorId>>>>,
    
    /// Dead letter queue for undeliverable messages
    dead_letter_queue: DeadLetterQueue<AnyMessage>,
    
    /// Message bus metrics
    metrics: MessageBusMetrics,
}

impl MessageBus {
    /// Route message to appropriate actor(s)
    pub async fn route_message<T: AlysMessage>(
        &self,
        envelope: MessageEnvelope<T>
    ) -> Result<(), BusError> {
        // 1. Validate message envelope
        self.validate_envelope(&envelope)?;
        
        // 2. Determine routing strategy
        let routing_strategy = self.determine_routing(&envelope.routing)?;
        
        // 3. Route based on strategy
        match routing_strategy {
            RoutingStrategy::Direct(actor_id) => {
                self.route_to_actor(actor_id, envelope).await?;
            }
            RoutingStrategy::Broadcast(event_type) => {
                self.broadcast_to_subscribers(event_type, envelope).await?;
            }
            RoutingStrategy::LoadBalance(actor_group) => {
                let actor_id = self.select_actor_from_group(&actor_group).await?;
                self.route_to_actor(actor_id, envelope).await?;
            }
        }
        
        // 4. Update metrics
        self.metrics.message_routed();
        
        Ok(())
    }
}
```

## Workflow System Architecture

### Business Logic Separation (`app/src/workflows/`)

Workflows encapsulate business logic separately from actor implementations:

#### Block Import Workflow (`block_import.rs`)
```rust
pub struct BlockImportWorkflow {
    /// Current workflow state
    state: BlockImportState,
    
    /// Workflow configuration
    config: BlockImportConfig,
    
    /// External dependencies (through traits)
    chain_client: Arc<dyn ChainIntegration>,
    execution_client: Arc<dyn ExecutionIntegration>,
    storage_client: Arc<dyn StorageIntegration>,
}

#[derive(Debug, Clone)]
pub enum BlockImportState {
    /// Waiting for block to import
    WaitingForBlock,
    
    /// Validating block structure and signatures
    ValidatingBlock {
        block: ConsensusBlock,
        started_at: SystemTime,
    },
    
    /// Executing transactions in the block
    ExecutingTransactions {
        block: ConsensusBlock,
        executed_count: usize,
        total_count: usize,
    },
    
    /// Storing block and state updates
    StoringBlock {
        block: ConsensusBlock,
        execution_result: ExecutionResult,
    },
    
    /// Finalizing block import
    FinalizingImport {
        block: ConsensusBlock,
        finalization_data: FinalizationData,
    },
    
    /// Block import completed successfully
    ImportCompleted {
        block: ConsensusBlock,
        import_result: ImportResult,
    },
    
    /// Block import failed with error
    ImportFailed {
        block: ConsensusBlock,
        error: ImportError,
        retry_count: u32,
    },
}

impl BlockImportWorkflow {
    /// Execute the block import workflow
    pub async fn execute(&mut self, input: WorkflowInput) -> Result<WorkflowOutput, WorkflowError> {
        match &self.state {
            BlockImportState::WaitingForBlock => {
                self.start_validation(input.block).await?;
            }
            BlockImportState::ValidatingBlock { block, .. } => {
                self.execute_transactions(block.clone()).await?;
            }
            BlockImportState::ExecutingTransactions { block, .. } => {
                self.store_block_data(block.clone()).await?;
            }
            BlockImportState::StoringBlock { block, .. } => {
                self.finalize_import(block.clone()).await?;
            }
            BlockImportState::FinalizingImport { block, .. } => {
                self.complete_import(block.clone()).await?;
            }
            _ => {
                return Err(WorkflowError::InvalidStateTransition);
            }
        }
        
        Ok(WorkflowOutput::Success)
    }
}
```

## Testing Architecture Deep Dive

### Property-Based Testing System (`app/src/testing/property_testing.rs`)

#### Core Framework Architecture
```rust
pub struct PropertyTestFramework {
    /// Test configuration and parameters
    config: PropertyTestConfig,
    
    /// Test case generators for different data types
    generators: HashMap<String, Box<dyn TestCaseGenerator>>,
    
    /// Shrinking engines for minimizing failing test cases
    shrinkers: HashMap<String, Box<dyn TestCaseShrinker>>,
    
    /// Registry of properties to test
    property_registry: HashMap<String, Box<dyn PropertyTest>>,
    
    /// Test execution context and state
    execution_context: Option<TestExecutionContext>,
    
    /// Results collector and analyzer
    results_collector: Arc<PropertyTestResults>,
}

// Actor-specific property testing
pub struct ActorPropertyTest {
    /// Name of the property being tested
    property_name: String,
    
    /// Actor type under test
    actor_type: String,
    
    /// Property invariant function
    invariant: Box<dyn Fn(&ActorState) -> bool + Send + Sync>,
    
    /// Test case generator
    generator: Box<dyn TestCaseGenerator>,
    
    /// Shrinking strategy
    shrinking_strategy: Box<dyn TestCaseShrinker>,
    
    /// Test configuration
    config: PropertyTestConfig,
}

impl ActorPropertyTest {
    /// Execute property test with generated test cases
    pub async fn run_property_test(&self) -> Result<PropertyTestResult, PropertyTestError> {
        let mut test_cases = Vec::new();
        let mut failures = Vec::new();
        
        // Generate test cases
        for _ in 0..self.config.max_test_cases {
            let test_case = self.generator.generate()?;
            test_cases.push(test_case);
        }
        
        // Execute test cases
        for (index, test_case) in test_cases.iter().enumerate() {
            let result = self.execute_test_case(test_case).await?;
            
            if !result.success {
                // Shrink failing test case to minimal example
                let minimal_case = self.shrink_test_case(test_case)?;
                failures.push(PropertyTestFailure {
                    original_case: test_case.clone(),
                    minimal_case,
                    failure_reason: result.error_message,
                    test_case_index: index,
                });
                
                if failures.len() >= self.config.max_failures {
                    break;
                }
            }
        }
        
        Ok(PropertyTestResult {
            property_name: self.property_name.clone(),
            total_cases: test_cases.len(),
            successful_cases: test_cases.len() - failures.len(),
            failures,
            execution_time: std::time::Instant::now() - start_time,
        })
    }
}
```

### Chaos Testing Engine (`app/src/testing/chaos_testing.rs`)

#### Controlled Fault Injection
```rust
pub struct ChaosTestEngine {
    /// Unique engine identifier
    engine_id: String,
    
    /// Chaos testing configuration
    config: ChaosEngineConfig,
    
    /// Available chaos scenarios
    scenarios: HashMap<String, ChaosTestScenario>,
    
    /// Currently running experiments
    active_experiments: Arc<RwLock<HashMap<String, ActiveChaosExperiment>>>,
    
    /// Fault injection system
    fault_injector: Arc<FaultInjector>,
    
    /// Recovery monitoring system
    recovery_monitor: Arc<RecoveryMonitor>,
    
    /// Chaos testing metrics
    metrics_collector: Arc<ChaosMetricsCollector>,
}

// Network partition scenario
pub struct NetworkPartition {
    /// Groups of actors to partition
    partition_groups: Vec<Vec<ActorId>>,
    
    /// Partition duration
    duration: Duration,
    
    /// Partition severity (partial vs complete)
    severity: PartitionSeverity,
}

impl NetworkPartition {
    pub async fn inject_fault(&self, target_system: &ActorSystem) -> Result<FaultHandle, ChaosError> {
        // 1. Identify actors in each partition group
        let mut partitioned_actors = HashMap::new();
        for (group_id, actor_ids) in self.partition_groups.iter().enumerate() {
            partitioned_actors.insert(group_id, actor_ids.clone());
        }
        
        // 2. Install message filtering to simulate network partition
        let filter = MessageFilter::new(Box::new(move |envelope: &MessageEnvelope<_>| {
            // Block messages between different partition groups
            let sender_group = self.get_actor_group(&envelope.from_actor);
            let receiver_group = self.get_actor_group(&envelope.to_actor);
            sender_group == receiver_group
        }));
        
        // 3. Install filter in message bus
        target_system.message_bus().install_filter(filter).await?;
        
        // 4. Schedule partition removal
        tokio::spawn({
            let duration = self.duration;
            let system = target_system.clone();
            async move {
                tokio::time::sleep(duration).await;
                system.message_bus().remove_filter().await.ok();
            }
        });
        
        Ok(FaultHandle::new("network_partition", SystemTime::now()))
    }
}
```

## Performance Optimization Strategies

### Actor System Performance
1. **Mailbox Optimization**: Bounded mailboxes with backpressure
2. **Message Batching**: Batch processing for high-throughput scenarios  
3. **Priority Queues**: High-priority message handling
4. **Connection Pooling**: Efficient external system connections
5. **Caching Strategies**: Multi-level LRU caching

### Memory Management
```rust
// Bounded resources per actor
pub struct ActorResourceLimits {
    /// Maximum mailbox size
    max_mailbox_size: usize,
    
    /// Maximum memory usage per actor
    max_memory_usage: usize,
    
    /// Maximum CPU time per message
    max_cpu_time: Duration,
    
    /// Maximum concurrent operations
    max_concurrent_ops: usize,
}

// Resource monitoring and enforcement
impl ActorContext<A> {
    pub fn check_resource_limits(&self) -> Result<(), ResourceError> {
        // Monitor memory usage
        if self.memory_usage() > self.limits.max_memory_usage {
            return Err(ResourceError::MemoryLimitExceeded);
        }
        
        // Monitor mailbox size
        if self.mailbox.len() > self.limits.max_mailbox_size {
            return Err(ResourceError::MailboxOverflow);
        }
        
        Ok(())
    }
}
```

## Security Architecture

### Message Security
```rust
pub struct SecureMessageEnvelope<T: AlysMessage> {
    /// Standard message envelope
    envelope: MessageEnvelope<T>,
    
    /// Message authentication code
    mac: MessageAuthenticationCode,
    
    /// Sender authentication
    sender_auth: AuthenticationToken,
    
    /// Message encryption (for sensitive data)
    encryption: Option<EncryptionData>,
}

// Input validation for all external data
pub trait MessageValidator {
    fn validate_message<T: AlysMessage>(&self, message: &T) -> Result<(), ValidationError>;
    fn sanitize_input<T: AlysMessage>(&self, message: &mut T) -> Result<(), SanitizationError>;
}
```

### Access Control
```rust
pub struct ActorPermissions {
    /// Operations this actor can perform
    allowed_operations: HashSet<Operation>,
    
    /// Resources this actor can access
    accessible_resources: HashSet<ResourceId>,
    
    /// Other actors this actor can message
    messaging_permissions: HashSet<ActorId>,
}

impl ActorContext<A> {
    pub fn check_permission(&self, operation: Operation) -> Result<(), PermissionError> {
        if !self.permissions.allowed_operations.contains(&operation) {
            return Err(PermissionError::OperationNotAllowed { operation });
        }
        Ok(())
    }
}
```

## Migration and Deployment Considerations

### Gradual Migration Strategy
1. **Phase 1-2**: Infrastructure and foundation setup
2. **Phase 3-4**: Core actor system with enhanced types
3. **Phase 5**: Configuration and integration layers  
4. **Phase 6**: Testing infrastructure validation
5. **Phase 7**: Documentation and final validation

### Deployment Architecture
```yaml
# Kubernetes deployment example
apiVersion: apps/v1
kind: Deployment
metadata:
  name: alys-v2-node
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: alys-node
        image: alys:v2.0.0
        env:
        - name: ALYS_ENVIRONMENT
          value: "production"
        - name: ALYS_CONFIG_PATH  
          value: "/etc/alys/config.toml"
        resources:
          requests:
            memory: "2Gi"
            cpu: "1000m"
          limits:
            memory: "4Gi" 
            cpu: "2000m"
        ports:
        - containerPort: 8545  # EVM RPC
        - containerPort: 3000  # Consensus RPC
        - containerPort: 30303 # P2P
```

## Monitoring and Observability

### Actor System Metrics
```rust
pub struct SystemMetrics {
    /// Total number of active actors
    pub active_actors: Gauge,
    
    /// Total messages processed per second
    pub messages_per_second: Counter,
    
    /// Average message processing time
    pub message_processing_time: Histogram,
    
    /// Actor restart count
    pub actor_restarts: Counter,
    
    /// System uptime
    pub uptime: Gauge,
    
    /// Memory usage per supervisor
    pub memory_usage_by_supervisor: GaugeVec,
    
    /// Error rates by actor type
    pub error_rate_by_actor: CounterVec,
}
```

### Health Checks
```rust
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check_health(&self) -> HealthStatus;
}

pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}

// System-wide health aggregation
impl AlysSystem {
    pub async fn overall_health(&self) -> HealthStatus {
        let mut actor_healths = Vec::new();
        
        // Check health of all actors
        for actor_id in self.registry.list_actors().await {
            let health = self.registry.check_actor_health(&actor_id).await;
            actor_healths.push(health);
        }
        
        // Aggregate health status
        if actor_healths.iter().all(|h| matches!(h, HealthStatus::Healthy)) {
            HealthStatus::Healthy
        } else if actor_healths.iter().any(|h| matches!(h, HealthStatus::Unhealthy { .. })) {
            HealthStatus::Unhealthy { 
                reason: "One or more critical actors unhealthy".to_string() 
            }
        } else {
            HealthStatus::Degraded { 
                reason: "Some actors experiencing issues".to_string() 
            }
        }
    }
}
```

This architectural overview provides the technical foundation for understanding the V2 actor-based system implementation and serves as a reference for continued development and maintenance.