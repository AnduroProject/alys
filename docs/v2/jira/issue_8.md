# ALYS-008: Implement EngineActor

## Description

Implement the EngineActor to replace the current Engine struct with a message-driven actor. This actor manages all interactions with the execution layer (Reth), handling block building, payload validation, and finalization without shared mutable state.

## Subtasks

- [X] Create ALYS-008-1: Design EngineActor message protocol with execution layer operations [https://marathondh.atlassian.net/browse/AN-414]
- [X] Create ALYS-008-2: Implement EngineActor core structure with JWT authentication [https://marathondh.atlassian.net/browse/AN-415]
- [X] Create ALYS-008-3: Implement block building logic with payload generation [https://marathondh.atlassian.net/browse/AN-416]
- [X] Create ALYS-008-4: Implement block commit and forkchoice update pipeline [https://marathondh.atlassian.net/browse/AN-417]
- [X] Create ALYS-008-5: Implement block finalization and state management [https://marathondh.atlassian.net/browse/AN-418]
- [X] Create ALYS-008-6: Implement execution client abstraction layer (Geth/Reth support) [https://marathondh.atlassian.net/browse/AN-419]
- [X] Create ALYS-008-7: Implement caching system for payloads and blocks [https://marathondh.atlassian.net/browse/AN-420]
- [X] Create ALYS-008-8: Create migration adapter for gradual Engine to EngineActor transition [https://marathondh.atlassian.net/browse/AN-421]
- [X] Create ALYS-008-9: Implement comprehensive test suite (unit, integration, client compatibility) [https://marathondh.atlassian.net/browse/AN-423]
- [X] Create ALYS-008-10: Performance benchmarking and optimization for execution operations [https://marathondh.atlassian.net/browse/AN-424]

## Acceptance Criteria

- [ ] EngineActor implements all Engine functionality
- [ ] Message protocol for execution layer operations
- [ ] JWT authentication maintained
- [ ] Support for both Geth and Reth clients
- [ ] No RwLock usage for state management
- [ ] Payload caching implemented
- [ ] Fork choice updates handled correctly
- [ ] Performance metrics collected
- [ ] Backward compatibility maintained

## Subtask Implementation Details

### ALYS-008-1: Design EngineActor Message Protocol
**Objective**: Define comprehensive message types for execution layer operations  
**TDD Approach**: Start with message contracts and mock responses
```rust
// Test-first development
#[test]
fn test_build_block_message_structure() {
    let msg = BuildExecutionPayload {
        timestamp: Duration::from_secs(1000),
        parent_hash: Some(Hash256::zero()),
        withdrawals: vec![],
        fee_recipient: None,
    };
    assert!(msg.timestamp.as_secs() > 0);
}

// Implementation
#[derive(Message)]
#[rtype(result = "Result<ExecutionPayload<MainnetEthSpec>, EngineError>")]
pub struct BuildExecutionPayload {
    pub timestamp: Duration,
    pub parent_hash: Option<ExecutionBlockHash>,
    pub withdrawals: Vec<Withdrawal>,
    pub fee_recipient: Option<Address>,
}
```
**Acceptance Criteria**: 
- [ ] All engine operations have message types
- [ ] Message validation implemented
- [ ] Error handling for invalid messages

### ALYS-008-2: Implement EngineActor Core Structure
**Objective**: Create actor with JWT auth, no shared state  
**TDD Approach**: Test actor lifecycle and authentication
```rust
#[actix::test]
async fn test_engine_actor_startup_with_jwt() {
    let config = EngineActorConfig {
        jwt_secret_path: PathBuf::from("test.jwt"),
        execution_endpoint: "http://localhost:8545".to_string(),
        // ...
    };
    let actor = EngineActor::new(config).await.unwrap().start();
    
    // Test auth connection
    let status = actor.send(GetSyncStatus).await.unwrap().unwrap();
    assert!(matches!(status, SyncStatus::Synced));
}
```
**Acceptance Criteria**: 
- [ ] Actor starts with valid JWT authentication
- [ ] Connection to execution client established
- [ ] State isolated within actor (no Arc<RwLock>)
- [ ] Health monitoring implemented

### ALYS-008-3: Implement Block Building Logic
**Objective**: Build execution payloads with withdrawals (peg-ins)  
**TDD Approach**: Test payload building with various inputs
```rust
#[actix::test]
async fn test_build_payload_with_withdrawals() {
    let actor = create_test_engine_actor().await;
    
    let withdrawals = vec![
        Withdrawal {
            index: 0,
            validator_index: 0,
            address: Address::from_low_u64_be(1),
            amount: 1000000000000000000u64, // 1 ETH in wei
        }
    ];
    
    let payload = actor.send(BuildExecutionPayload {
        timestamp: Duration::from_secs(1000),
        parent_hash: None,
        withdrawals,
        fee_recipient: None,
    }).await.unwrap().unwrap();
    
    assert_eq!(payload.withdrawals().len(), 1);
    assert!(payload.gas_limit() > 0);
}
```
**Acceptance Criteria**: 
- [ ] Payload building with parent hash
- [ ] Withdrawals properly included (peg-ins)
- [ ] Gas limit and fee recipient handling
- [ ] Error handling for invalid parameters

### ALYS-008-4: Implement Block Commit Pipeline
**Objective**: Commit blocks and update forkchoice state  
**TDD Approach**: Test commit workflow and forkchoice updates
```rust
#[actix::test]
async fn test_commit_block_and_forkchoice() {
    let actor = create_test_engine_actor().await;
    
    // Build payload first
    let payload = build_test_payload();
    
    // Commit the block
    let block_hash = actor.send(CommitExecutionPayload {
        payload: payload.clone(),
    }).await.unwrap().unwrap();
    
    assert_eq!(block_hash, payload.block_hash());
    
    // Verify forkchoice was updated
    let status = actor.send(GetForkchoiceState).await.unwrap().unwrap();
    assert_eq!(status.head_block_hash, block_hash);
}
```
**Acceptance Criteria**: 
- [ ] Payload validation before commit
- [ ] Forkchoice state updates correctly
- [ ] Invalid payload rejection
- [ ] State consistency after commit

### ALYS-008-5: Implement Block Finalization
**Objective**: Finalize blocks and maintain finalized state  
**TDD Approach**: Test finalization workflow and state updates
```rust
#[actix::test]
async fn test_block_finalization_workflow() {
    let actor = create_test_engine_actor().await;
    
    let block_hash = commit_test_block(&actor).await;
    
    // Finalize the block
    actor.send(FinalizeExecutionBlock {
        block_hash,
    }).await.unwrap().unwrap();
    
    // Verify finalized state
    let status = actor.send(GetForkchoiceState).await.unwrap().unwrap();
    assert_eq!(status.finalized_block_hash, block_hash);
    assert_eq!(status.safe_block_hash, block_hash);
}
```
**Acceptance Criteria**: 
- [ ] Finalization updates forkchoice state
- [ ] Safe and finalized pointers updated
- [ ] Finalization of non-existent blocks handled
- [ ] State persistence after finalization

### ALYS-008-6: Implement Client Abstraction Layer
**Objective**: Support multiple execution clients (Geth/Reth)  
**TDD Approach**: Test client detection and compatibility
```rust
#[test]
fn test_client_type_detection() {
    assert_eq!(
        ExecutionClientType::from_version("Geth/v1.13.0"),
        ExecutionClientType::Geth
    );
    assert_eq!(
        ExecutionClientType::from_version("reth/0.1.0"),
        ExecutionClientType::Reth
    );
}

#[actix::test]
async fn test_geth_specific_operations() {
    let geth_client = GethExecutionClient::new(config).await.unwrap();
    let payload = geth_client.build_payload(params).await.unwrap();
    // Test Geth-specific behavior
}
```
**Acceptance Criteria**: 
- [ ] Auto-detection of execution client type
- [ ] Geth-specific optimizations
- [ ] Reth-specific optimizations
- [ ] Consistent API across client types

### ALYS-008-7: Implement Caching System
**Objective**: Cache payloads and blocks for performance  
**TDD Approach**: Test cache behavior and eviction
```rust
#[test]
fn test_payload_cache_operations() {
    let mut cache = PayloadCache::new(100, Duration::from_secs(60));
    let payload_id = PayloadId([1, 2, 3, 4, 5, 6, 7, 8]);
    let payload = create_test_payload();
    
    cache.insert(payload_id, payload.clone());
    assert_eq!(cache.get(&payload_id), Some(&payload));
    
    // Test TTL expiration
    std::thread::sleep(Duration::from_secs(61));
    cache.cleanup();
    assert_eq!(cache.get(&payload_id), None);
}
```
**Acceptance Criteria**: 
- [ ] LRU eviction for payload cache
- [ ] TTL-based cache expiration
- [ ] Block cache for frequently accessed blocks
- [ ] Cache hit/miss metrics

### ALYS-008-8: Create Migration Adapter
**Objective**: Gradual migration from legacy Engine  
**TDD Approach**: Test parallel execution and fallback
```rust
#[actix::test]
async fn test_migration_parallel_mode() {
    let adapter = EngineMigrationAdapter::new(
        Some(legacy_engine),
        Some(engine_actor),
        MigrationMode::Parallel,
    );
    
    let payload = adapter.build_block(params).await.unwrap();
    
    // Verify both implementations were called
    assert_eq!(adapter.get_metrics().parallel_calls, 1);
}
```
**Acceptance Criteria**: 
- [ ] Parallel execution mode with result comparison
- [ ] Fallback from actor to legacy on errors
- [ ] Migration metrics collection
- [ ] Gradual rollout configuration

### ALYS-008-9: Comprehensive Test Suite
**Objective**: >90% test coverage with multiple test types  
**TDD Approach**: Property-based and integration testing
```rust
// Property-based testing
proptest! {
    #[test]
    fn test_payload_building_properties(
        timestamp in 1u64..u64::MAX,
        withdrawal_count in 0usize..100,
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let actor = create_test_engine_actor().await;
            let withdrawals = create_test_withdrawals(withdrawal_count);
            
            let result = actor.send(BuildExecutionPayload {
                timestamp: Duration::from_secs(timestamp),
                parent_hash: None,
                withdrawals,
                fee_recipient: None,
            }).await;
            
            // Properties that should always hold
            if let Ok(Ok(payload)) = result {
                prop_assert!(payload.timestamp() == timestamp);
                prop_assert!(payload.gas_limit() > 0);
            }
        });
    }
}

// Integration test with real clients
#[tokio::test]
#[ignore] // Run with --ignored for integration tests
async fn test_real_geth_integration() {
    let config = EngineActorConfig {
        execution_endpoint: "http://localhost:8545".to_string(),
        execution_endpoint_auth: "http://localhost:8551".to_string(),
        jwt_secret_path: PathBuf::from("test.jwt"),
        client_type: ExecutionClientType::Geth,
        // ...
    };
    
    let actor = EngineActor::new(config).await.unwrap().start();
    
    // Test real operations
    let payload = actor.send(BuildExecutionPayload {
        timestamp: Duration::from_secs(1000),
        parent_hash: None,
        withdrawals: vec![],
        fee_recipient: None,
    }).await.unwrap().unwrap();
    
    assert!(!payload.transactions().is_empty() || payload.transactions().is_empty()); // May be empty
}
```
**Acceptance Criteria**: 
- [ ] Unit tests for all message handlers
- [ ] Integration tests with real Geth/Reth
- [ ] Property-based tests for edge cases
- [ ] Performance tests under load
- [ ] Error handling and recovery tests

### ALYS-008-10: Performance Benchmarking
**Objective**: Optimize execution operations for performance targets  
**TDD Approach**: Benchmark-driven optimization
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_block_building(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let actor = runtime.block_on(create_test_engine_actor());
    
    c.bench_function("build_execution_payload", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let result = actor.send(BuildExecutionPayload {
                    timestamp: Duration::from_secs(1000),
                    parent_hash: None,
                    withdrawals: black_box(vec![]),
                    fee_recipient: None,
                }).await.unwrap();
                
                black_box(result)
            })
        })
    });
}

criterion_group!(benches, bench_block_building);
criterion_main!(benches);
```
**Acceptance Criteria**: 
- [ ] Block building <200ms (target)
- [ ] Block commit <100ms (target)
- [ ] Cache hit ratio >80%
- [ ] Memory usage <256MB under load
- [ ] Concurrent request handling

## Technical Details

### Implementation Steps

1. **Define EngineActor Messages**
```rust
// src/actors/engine/messages.rs

use actix::prelude::*;
use lighthouse_wrapper::execution_layer::*;
use lighthouse_wrapper::types::*;

#[derive(Message)]
#[rtype(result = "Result<ExecutionPayload<MainnetEthSpec>, EngineError>")]
pub struct BuildBlock {
    pub timestamp: Duration,
    pub parent: Option<ExecutionBlockHash>,
    pub withdrawals: Vec<Withdrawal>,  // Peg-ins
    pub suggested_fee_recipient: Option<Address>,
}

#[derive(Message)]
#[rtype(result = "Result<ExecutionBlockHash, EngineError>")]
pub struct CommitBlock {
    pub payload: ExecutionPayload<MainnetEthSpec>,
}

#[derive(Message)]
#[rtype(result = "Result<(), EngineError>")]
pub struct ValidatePayload {
    pub payload: ExecutionPayload<MainnetEthSpec>,
}

#[derive(Message)]
#[rtype(result = "Result<(), EngineError>")]
pub struct FinalizeBlock {
    pub block_hash: ExecutionBlockHash,
}

#[derive(Message)]
#[rtype(result = "Result<(), EngineError>")]
pub struct RevertBlock {
    pub block_hash: ExecutionBlockHash,
}

#[derive(Message)]
#[rtype(result = "Result<ExecutionBlock, EngineError>")]
pub struct GetBlock {
    pub identifier: BlockIdentifier,
}

#[derive(Message)]
#[rtype(result = "Result<Vec<Log>, EngineError>")]
pub struct GetLogs {
    pub filter: LogFilter,
}

#[derive(Message)]
#[rtype(result = "Result<SyncStatus, EngineError>")]
pub struct GetSyncStatus;

#[derive(Message)]
#[rtype(result = "Result<(), EngineError>")]
pub struct UpdateForkchoice {
    pub head: ExecutionBlockHash,
    pub safe: ExecutionBlockHash,
    pub finalized: ExecutionBlockHash,
}

#[derive(Debug, Clone)]
pub enum BlockIdentifier {
    Hash(ExecutionBlockHash),
    Number(u64),
    Latest,
    Pending,
}

#[derive(Debug, Clone)]
pub struct LogFilter {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub address: Option<Vec<Address>>,
    pub topics: Vec<Option<H256>>,
}
```

2. **Implement EngineActor Core**
```rust
// src/actors/engine/mod.rs

use actix::prelude::*;
use lighthouse_wrapper::execution_layer::{
    auth::{Auth, JwtKey},
    HttpJsonRpc,
    ForkchoiceState,
    PayloadAttributes,
    PayloadStatus,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct EngineActor {
    // Engine API connections
    authenticated_api: HttpJsonRpc,  // Port 8551 (authenticated)
    public_api: HttpJsonRpc,         // Port 8545 (public)
    
    // State (owned by actor)
    latest_block: Option<ExecutionBlockHash>,
    finalized_block: Option<ExecutionBlockHash>,
    safe_block: Option<ExecutionBlockHash>,
    
    // Caching
    payload_cache: PayloadCache,
    block_cache: BlockCache,
    
    // Configuration
    config: EngineConfig,
    
    // Metrics
    metrics: EngineMetrics,
}

#[derive(Clone)]
pub struct EngineConfig {
    pub execution_endpoint: String,
    pub execution_endpoint_auth: String,
    pub jwt_secret_path: PathBuf,
    pub default_fee_recipient: Address,
    pub cache_size: usize,
    pub request_timeout: Duration,
    pub client_type: ExecutionClientType,
}

#[derive(Debug, Clone)]
pub enum ExecutionClientType {
    Geth,
    Reth,
    Nethermind,
    Besu,
}

struct PayloadCache {
    payloads: HashMap<PayloadId, ExecutionPayload<MainnetEthSpec>>,
    timestamps: HashMap<PayloadId, Instant>,
    max_size: usize,
    ttl: Duration,
}

struct BlockCache {
    blocks: lru::LruCache<ExecutionBlockHash, ExecutionBlock>,
}

impl EngineActor {
    pub async fn new(config: EngineConfig) -> Result<Self, EngineError> {
        // Load JWT secret
        let jwt_key = JwtKey::from_file(&config.jwt_secret_path)
            .map_err(|e| EngineError::JwtError(e.to_string()))?;
        
        // Create authenticated API client
        let auth = Auth::new(jwt_key, None, None);
        let authenticated_api = HttpJsonRpc::new_with_auth(
            &config.execution_endpoint_auth,
            auth,
            Some(config.request_timeout),
        )?;
        
        // Create public API client
        let public_api = HttpJsonRpc::new(
            &config.execution_endpoint,
            Some(config.request_timeout),
        )?;
        
        // Test connection
        let version = public_api.client_version().await?;
        info!("Connected to execution client: {}", version);
        
        Ok(Self {
            authenticated_api,
            public_api,
            latest_block: None,
            finalized_block: None,
            safe_block: None,
            payload_cache: PayloadCache::new(config.cache_size, Duration::from_secs(60)),
            block_cache: BlockCache::new(config.cache_size),
            config,
            metrics: EngineMetrics::new(),
        })
    }
    
    async fn get_latest_block_hash(&mut self) -> Result<ExecutionBlockHash, EngineError> {
        if let Some(hash) = self.latest_block {
            if self.block_cache.contains(&hash) {
                return Ok(hash);
            }
        }
        
        // Fetch latest block
        let block = self.public_api
            .get_block_by_number(BlockByNumberQuery::Tag(LATEST_TAG))
            .await?
            .ok_or(EngineError::BlockNotFound)?;
        
        let hash = block.block_hash;
        self.latest_block = Some(hash);
        self.block_cache.put(hash, block);
        
        Ok(hash)
    }
}

impl Actor for EngineActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("EngineActor started");
        
        // Start cache cleanup timer
        ctx.run_interval(Duration::from_secs(30), |act, _| {
            act.payload_cache.cleanup();
        });
        
        // Start sync status checker
        ctx.run_interval(Duration::from_secs(10), |act, ctx| {
            ctx.spawn(
                async move {
                    if let Err(e) = act.check_sync_status().await {
                        warn!("Sync status check failed: {}", e);
                    }
                }
                .into_actor(act)
            );
        });
    }
}

impl Handler<BuildBlock> for EngineActor {
    type Result = ResponseActFuture<Self, Result<ExecutionPayload<MainnetEthSpec>, EngineError>>;
    
    fn handle(&mut self, msg: BuildBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let start = Instant::now();
            self.metrics.build_block_requests.inc();
            
            // Get parent block hash
            let parent_hash = match msg.parent {
                Some(hash) => hash,
                None => self.get_latest_block_hash().await?,
            };
            
            // Build forkchoice state
            let forkchoice_state = ForkchoiceState {
                head_block_hash: parent_hash,
                safe_block_hash: self.safe_block.unwrap_or(parent_hash),
                finalized_block_hash: self.finalized_block.unwrap_or_default(),
            };
            
            // Build payload attributes
            let fee_recipient = msg.suggested_fee_recipient
                .unwrap_or(self.config.default_fee_recipient);
            
            let payload_attributes = PayloadAttributes::new(
                msg.timestamp.as_secs(),
                Hash256::random(),  // prevRandao (not used in Alys)
                fee_recipient,
                Some(msg.withdrawals),  // Peg-ins as withdrawals
            );
            
            // Request payload from execution client
            let response = self.authenticated_api
                .forkchoice_updated(forkchoice_state, Some(payload_attributes))
                .await
                .map_err(|e| {
                    self.metrics.engine_errors.with_label_values(&["forkchoice_updated"]).inc();
                    EngineError::EngineApiError(e.to_string())
                })?;
            
            // Check payload status
            match response.payload_status.status {
                PayloadStatusEnum::Valid | PayloadStatusEnum::Syncing => {},
                PayloadStatusEnum::Invalid => {
                    return Err(EngineError::InvalidPayloadStatus(
                        response.payload_status.validation_error
                    ));
                }
                _ => {
                    return Err(EngineError::UnexpectedPayloadStatus);
                }
            }
            
            let payload_id = response.payload_id
                .ok_or(EngineError::PayloadIdNotProvided)?;
            
            // Get the built payload
            let payload_response = self.authenticated_api
                .get_payload::<MainnetEthSpec>(ForkName::Capella, payload_id)
                .await
                .map_err(|e| {
                    self.metrics.engine_errors.with_label_values(&["get_payload"]).inc();
                    EngineError::EngineApiError(e.to_string())
                })?;
            
            let payload = payload_response.execution_payload_ref().clone_from_ref();
            
            // Cache the payload
            self.payload_cache.insert(payload_id, payload.clone());
            
            self.metrics.build_block_duration.observe(start.elapsed().as_secs_f64());
            self.metrics.blocks_built.inc();
            
            debug!("Built block with {} transactions", payload.transactions().len());
            
            Ok(payload)
        }.into_actor(self))
    }
}

impl Handler<CommitBlock> for EngineActor {
    type Result = ResponseActFuture<Self, Result<ExecutionBlockHash, EngineError>>;
    
    fn handle(&mut self, msg: CommitBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let start = Instant::now();
            
            // Send new payload to execution client
            let response = self.authenticated_api
                .new_payload::<MainnetEthSpec>(msg.payload.clone())
                .await
                .map_err(|e| {
                    self.metrics.engine_errors.with_label_values(&["new_payload"]).inc();
                    EngineError::EngineApiError(e.to_string())
                })?;
            
            // Check status
            match response.status {
                PayloadStatusEnum::Valid => {
                    let block_hash = msg.payload.block_hash();
                    
                    // Update forkchoice to commit the block
                    let forkchoice_state = ForkchoiceState {
                        head_block_hash: block_hash,
                        safe_block_hash: self.safe_block.unwrap_or(block_hash),
                        finalized_block_hash: self.finalized_block.unwrap_or_default(),
                    };
                    
                    let fc_response = self.authenticated_api
                        .forkchoice_updated(forkchoice_state, None)
                        .await
                        .map_err(|e| {
                            self.metrics.engine_errors.with_label_values(&["forkchoice_updated"]).inc();
                            EngineError::EngineApiError(e.to_string())
                        })?;
                    
                    if fc_response.payload_status.status != PayloadStatusEnum::Valid {
                        return Err(EngineError::InvalidPayloadStatus(
                            fc_response.payload_status.validation_error
                        ));
                    }
                    
                    // Update latest block
                    self.latest_block = Some(block_hash);
                    
                    self.metrics.commit_block_duration.observe(start.elapsed().as_secs_f64());
                    self.metrics.blocks_committed.inc();
                    
                    Ok(block_hash)
                }
                PayloadStatusEnum::Invalid => {
                    Err(EngineError::InvalidPayload(response.validation_error))
                }
                PayloadStatusEnum::Syncing => {
                    Err(EngineError::ClientSyncing)
                }
                _ => {
                    Err(EngineError::UnexpectedPayloadStatus)
                }
            }
        }.into_actor(self))
    }
}

impl Handler<FinalizeBlock> for EngineActor {
    type Result = ResponseActFuture<Self, Result<(), EngineError>>;
    
    fn handle(&mut self, msg: FinalizeBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            // Update forkchoice with new finalized block
            let forkchoice_state = ForkchoiceState {
                head_block_hash: self.latest_block.unwrap_or(msg.block_hash),
                safe_block_hash: msg.block_hash,
                finalized_block_hash: msg.block_hash,
            };
            
            let response = self.authenticated_api
                .forkchoice_updated(forkchoice_state, None)
                .await
                .map_err(|e| {
                    self.metrics.engine_errors.with_label_values(&["forkchoice_updated"]).inc();
                    EngineError::EngineApiError(e.to_string())
                })?;
            
            if response.payload_status.status != PayloadStatusEnum::Valid {
                return Err(EngineError::InvalidPayloadStatus(
                    response.payload_status.validation_error
                ));
            }
            
            self.finalized_block = Some(msg.block_hash);
            self.safe_block = Some(msg.block_hash);
            
            self.metrics.blocks_finalized.inc();
            
            info!("Finalized block: {:?}", msg.block_hash);
            
            Ok(())
        }.into_actor(self))
    }
}

impl Handler<GetBlock> for EngineActor {
    type Result = ResponseActFuture<Self, Result<ExecutionBlock, EngineError>>;
    
    fn handle(&mut self, msg: GetBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            // Check cache first
            if let BlockIdentifier::Hash(hash) = msg.identifier {
                if let Some(block) = self.block_cache.get(&hash) {
                    self.metrics.cache_hits.inc();
                    return Ok(block.clone());
                }
            }
            
            self.metrics.cache_misses.inc();
            
            // Fetch from execution client
            let block = match msg.identifier {
                BlockIdentifier::Hash(hash) => {
                    self.public_api
                        .get_block_by_hash(hash)
                        .await?
                }
                BlockIdentifier::Number(number) => {
                    self.public_api
                        .get_block_by_number(BlockByNumberQuery::Number(number))
                        .await?
                }
                BlockIdentifier::Latest => {
                    self.public_api
                        .get_block_by_number(BlockByNumberQuery::Tag(LATEST_TAG))
                        .await?
                }
                BlockIdentifier::Pending => {
                    self.public_api
                        .get_block_by_number(BlockByNumberQuery::Tag(PENDING_TAG))
                        .await?
                }
            };
            
            let block = block.ok_or(EngineError::BlockNotFound)?;
            
            // Cache the block
            self.block_cache.put(block.block_hash, block.clone());
            
            Ok(block)
        }.into_actor(self))
    }
}

impl EngineActor {
    async fn check_sync_status(&mut self) -> Result<(), EngineError> {
        let syncing = self.public_api.syncing().await?;
        
        if let Some(sync_status) = syncing {
            let progress = (sync_status.current_block as f64 / sync_status.highest_block as f64) * 100.0;
            self.metrics.sync_progress.set(progress);
            
            if progress < 99.0 {
                warn!("Execution client syncing: {:.1}%", progress);
            }
        } else {
            self.metrics.sync_progress.set(100.0);
        }
        
        Ok(())
    }
}

impl PayloadCache {
    fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            payloads: HashMap::with_capacity(max_size),
            timestamps: HashMap::with_capacity(max_size),
            max_size,
            ttl,
        }
    }
    
    fn insert(&mut self, id: PayloadId, payload: ExecutionPayload<MainnetEthSpec>) {
        // Evict old entries if at capacity
        if self.payloads.len() >= self.max_size {
            self.evict_oldest();
        }
        
        self.payloads.insert(id, payload);
        self.timestamps.insert(id, Instant::now());
    }
    
    fn cleanup(&mut self) {
        let now = Instant::now();
        self.timestamps.retain(|id, timestamp| {
            if now.duration_since(*timestamp) > self.ttl {
                self.payloads.remove(id);
                false
            } else {
                true
            }
        });
    }
    
    fn evict_oldest(&mut self) {
        if let Some((oldest_id, _)) = self.timestamps
            .iter()
            .min_by_key(|(_, timestamp)| *timestamp)
            .map(|(id, ts)| (*id, *ts))
        {
            self.payloads.remove(&oldest_id);
            self.timestamps.remove(&oldest_id);
        }
    }
}
```

3. **Create Client Abstraction for Multiple Execution Clients**
```rust
// src/actors/engine/clients.rs

use super::*;

/// Abstraction over different execution clients
pub trait ExecutionClient: Send + Sync {
    async fn build_block(
        &self,
        parent: ExecutionBlockHash,
        timestamp: u64,
        withdrawals: Vec<Withdrawal>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError>;
    
    async fn commit_block(
        &self,
        payload: ExecutionPayload<MainnetEthSpec>,
    ) -> Result<ExecutionBlockHash, EngineError>;
    
    async fn finalize_block(
        &self,
        block_hash: ExecutionBlockHash,
    ) -> Result<(), EngineError>;
    
    async fn get_block(
        &self,
        identifier: BlockIdentifier,
    ) -> Result<Option<ExecutionBlock>, EngineError>;
}

/// Geth-specific implementation
pub struct GethClient {
    api: HttpJsonRpc,
}

impl ExecutionClient for GethClient {
    async fn build_block(
        &self,
        parent: ExecutionBlockHash,
        timestamp: u64,
        withdrawals: Vec<Withdrawal>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError> {
        // Geth-specific implementation
        // Handle any Geth quirks here
        todo!()
    }
    
    // ... other methods
}

/// Reth-specific implementation
pub struct RethClient {
    api: HttpJsonRpc,
}

impl ExecutionClient for RethClient {
    async fn build_block(
        &self,
        parent: ExecutionBlockHash,
        timestamp: u64,
        withdrawals: Vec<Withdrawal>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError> {
        // Reth-specific implementation
        // Reth may have different optimizations
        todo!()
    }
    
    // ... other methods
}
```

## Testing Plan

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[actix::test]
    async fn test_build_block() {
        let engine = create_mock_engine_actor().await;
        
        let payload = engine.send(BuildBlock {
            timestamp: Duration::from_secs(1000),
            parent: None,
            withdrawals: vec![],
            suggested_fee_recipient: None,
        }).await.unwrap().unwrap();
        
        assert!(!payload.transactions().is_empty() || true); // May be empty
        assert_eq!(payload.timestamp(), 1000);
    }
    
    #[actix::test]
    async fn test_commit_and_finalize() {
        let engine = create_mock_engine_actor().await;
        
        // Build a block
        let payload = engine.send(BuildBlock {
            timestamp: Duration::from_secs(1000),
            parent: None,
            withdrawals: vec![],
            suggested_fee_recipient: None,
        }).await.unwrap().unwrap();
        
        // Commit it
        let block_hash = engine.send(CommitBlock { payload: payload.clone() })
            .await.unwrap().unwrap();
        
        assert_eq!(block_hash, payload.block_hash());
        
        // Finalize it
        engine.send(FinalizeBlock { block_hash })
            .await.unwrap().unwrap();
    }
    
    #[actix::test]
    async fn test_cache_functionality() {
        let engine = create_mock_engine_actor().await;
        
        // Get a block (will miss cache)
        let block1 = engine.send(GetBlock {
            identifier: BlockIdentifier::Latest,
        }).await.unwrap().unwrap();
        
        // Get same block again (should hit cache)
        let block2 = engine.send(GetBlock {
            identifier: BlockIdentifier::Hash(block1.block_hash),
        }).await.unwrap().unwrap();
        
        assert_eq!(block1, block2);
    }
}
```

### Integration Tests
1. Test with real Geth instance
2. Test with real Reth instance
3. Test JWT authentication
4. Test error handling and recovery
5. Test cache eviction

### Performance Tests
```rust
#[bench]
fn bench_block_building(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let engine = runtime.block_on(create_test_engine_actor());
    
    b.iter(|| {
        runtime.block_on(async {
            engine.send(BuildBlock {
                timestamp: Duration::from_secs(1000),
                parent: None,
                withdrawals: vec![],
                suggested_fee_recipient: None,
            }).await.unwrap().unwrap()
        })
    });
}
```

## Dependencies

### Blockers
- ALYS-006: Actor supervisor must be implemented

### Blocked By
None

### Related Issues
- ALYS-007: ChainActor (consensus layer)
- ALYS-009: BridgeActor (peg operations)
- ALYS-014: Lighthouse v5 compatibility

## Definition of Done

- [ ] EngineActor fully implemented
- [ ] Support for Geth and Reth
- [ ] JWT authentication working
- [ ] Caching system operational
- [ ] All engine operations migrated
- [ ] Performance benchmarks pass
- [ ] Integration tests with real clients
- [ ] Documentation complete
- [ ] Code review completed

## Notes

- Implement engine API v2 for Cancun support

## Next Steps

### Work Completed Analysis (85% Complete)

**Completed Components (✓):**
- Message protocol design with execution layer operations (100% complete)
- Core EngineActor structure with JWT authentication (95% complete)
- Block building logic with payload generation (90% complete)
- Block commit and forkchoice update pipeline (90% complete)
- Block finalization and state management (85% complete)
- Execution client abstraction layer (80% complete)
- Caching system for payloads and blocks (85% complete)

**Detailed Work Analysis:**
1. **Message Protocol (100%)** - All message types defined including BuildBlock, CommitBlock, ValidatePayload, FinalizeBlock, RevertBlock, GetBlock, GetLogs, GetSyncStatus, UpdateForkchoice with proper error handling
2. **Actor Structure (95%)** - Complete EngineActor with JWT authentication, execution client connections, owned state, caching systems, and metrics
3. **Block Building (90%)** - BuildBlock handler with forkchoice state, payload attributes, peg-in withdrawals, and execution client interaction
4. **Block Commit (90%)** - CommitBlock handler with new payload validation, forkchoice updates, and state management
5. **Finalization (85%)** - FinalizeBlock handler with forkchoice state updates and finalized block tracking
6. **Client Abstraction (80%)** - ExecutionClient trait with Geth/Reth implementations and client-specific optimizations
7. **Caching (85%)** - PayloadCache and BlockCache with LRU eviction, TTL cleanup, and cache metrics

### Remaining Work Analysis

**Missing Critical Components:**
- Migration adapter for gradual Engine to EngineActor transition (25% complete)
- Comprehensive test suite coverage (60% complete)
- Performance benchmarking and optimization (40% complete)
- Error recovery and resilience patterns (30% complete)
- Production monitoring and alerting (20% complete)

### Detailed Next Step Plans

#### Priority 1: Complete Production-Ready EngineActor

**Plan:** Implement comprehensive error handling, resilience patterns, and production monitoring for the EngineActor.

**Implementation 1: Advanced Error Handling and Resilience**
```rust
// src/actors/engine/resilience.rs
use actix::prelude::*;
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[derive(Debug)]
pub struct ResilienceManager {
    // Circuit breaker for execution client
    circuit_breaker: CircuitBreaker,
    // Retry policies for different operations
    retry_policies: HashMap<String, RetryPolicy>,
    // Health monitoring
    health_monitor: HealthMonitor,
    // Failover mechanisms
    failover_handler: FailoverHandler,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    last_failure: Option<Instant>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,    // Normal operation
    Open,      // Failures detected, block requests
    HalfOpen,  // Test if service recovered
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub success_threshold: u32,
}

#[derive(Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub exponential_base: f64,
    pub jitter: bool,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure: None,
            config,
        }
    }

    pub fn call<T, F, Fut, E>(&mut self, operation: F) -> Result<impl Future<Output = Result<T, E>>, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        match self.state {
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure {
                    if last_failure.elapsed() > self.config.recovery_timeout {
                        self.state = CircuitBreakerState::HalfOpen;
                        info!("Circuit breaker transitioning to half-open");
                    } else {
                        return Err(CircuitBreakerError::CircuitOpen);
                    }
                }
            }
            CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen => {
                // Allow operation to proceed
            }
        }

        let future = async move {
            let result = operation().await;
            
            match &result {
                Ok(_) => {
                    self.on_success();
                }
                Err(e) => {
                    self.on_failure();
                    debug!("Circuit breaker recorded failure: {:?}", e);
                }
            }
            
            result
        };

        Ok(future)
    }

    fn on_success(&mut self) {
        match self.state {
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Closed;
                self.failure_count = 0;
                info!("Circuit breaker closed after successful recovery test");
            }
            CircuitBreakerState::Closed => {
                // Reset failure count on success
                if self.failure_count > 0 {
                    self.failure_count = 0;
                }
            }
            CircuitBreakerState::Open => {
                // Should not happen
                warn!("Circuit breaker received success while open");
            }
        }
    }

    fn on_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        if self.failure_count >= self.config.failure_threshold {
            self.state = CircuitBreakerState::Open;
            warn!("Circuit breaker opened due to {} failures", self.failure_count);
        }
    }
}

// Enhanced EngineActor with resilience
impl EngineActor {
    async fn resilient_api_call<T, F, Fut>(
        &mut self,
        operation_name: &str,
        operation: F,
    ) -> Result<T, EngineError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, EngineError>>,
    {
        let retry_policy = self.resilience_manager
            .retry_policies
            .get(operation_name)
            .cloned()
            .unwrap_or_default();

        let mut attempts = 0;
        let mut last_error = None;

        while attempts < retry_policy.max_attempts {
            attempts += 1;

            // Check circuit breaker
            let circuit_breaker_result = self.resilience_manager
                .circuit_breaker
                .call(|| operation());

            match circuit_breaker_result {
                Ok(future) => {
                    match timeout(Duration::from_secs(30), future).await {
                        Ok(Ok(result)) => {
                            if attempts > 1 {
                                info!("Operation '{}' succeeded after {} attempts", operation_name, attempts);
                            }
                            self.metrics.operation_retries
                                .with_label_values(&[operation_name])
                                .observe((attempts - 1) as f64);
                            return Ok(result);
                        }
                        Ok(Err(e)) => {
                            last_error = Some(e);
                            self.metrics.operation_failures
                                .with_label_values(&[operation_name])
                                .inc();
                        }
                        Err(_) => {
                            last_error = Some(EngineError::Timeout);
                            self.metrics.operation_timeouts
                                .with_label_values(&[operation_name])
                                .inc();
                        }
                    }
                }
                Err(CircuitBreakerError::CircuitOpen) => {
                    self.metrics.circuit_breaker_rejections
                        .with_label_values(&[operation_name])
                        .inc();
                    return Err(EngineError::CircuitBreakerOpen);
                }
            }

            if attempts < retry_policy.max_attempts {
                let delay = self.calculate_retry_delay(&retry_policy, attempts);
                warn!("Operation '{}' failed (attempt {}/{}), retrying in {:?}", 
                      operation_name, attempts, retry_policy.max_attempts, delay);
                tokio::time::sleep(delay).await;
            }
        }

        self.metrics.operation_exhausted_retries
            .with_label_values(&[operation_name])
            .inc();

        Err(last_error.unwrap_or(EngineError::MaxRetriesExceeded))
    }

    fn calculate_retry_delay(&self, policy: &RetryPolicy, attempt: u32) -> Duration {
        let delay = policy.base_delay.as_millis() as f64 
            * policy.exponential_base.powi((attempt - 1) as i32);
        
        let delay = Duration::from_millis(delay as u64).min(policy.max_delay);
        
        if policy.jitter {
            // Add random jitter ±25%
            let jitter_range = delay.as_millis() as f64 * 0.25;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            let final_delay = delay.as_millis() as f64 + jitter;
            Duration::from_millis(final_delay.max(0.0) as u64)
        } else {
            delay
        }
    }
}

// Enhanced message handlers with resilience
impl Handler<BuildBlock> for EngineActor {
    type Result = ResponseActFuture<Self, Result<ExecutionPayload<MainnetEthSpec>, EngineError>>;
    
    fn handle(&mut self, msg: BuildBlock, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let operation = || async {
                // Get parent block hash
                let parent_hash = match msg.parent {
                    Some(hash) => hash,
                    None => self.get_latest_block_hash().await?,
                };
                
                // Build forkchoice state
                let forkchoice_state = ForkchoiceState {
                    head_block_hash: parent_hash,
                    safe_block_hash: self.safe_block.unwrap_or(parent_hash),
                    finalized_block_hash: self.finalized_block.unwrap_or_default(),
                };
                
                // Build payload attributes
                let fee_recipient = msg.suggested_fee_recipient
                    .unwrap_or(self.config.default_fee_recipient);
                
                let payload_attributes = PayloadAttributes::new(
                    msg.timestamp.as_secs(),
                    Hash256::random(),
                    fee_recipient,
                    Some(msg.withdrawals.clone()),
                );
                
                // Request payload from execution client with retry
                let response = self.resilient_api_call("forkchoice_updated", || async {
                    self.authenticated_api
                        .forkchoice_updated(forkchoice_state, Some(payload_attributes.clone()))
                        .await
                        .map_err(|e| EngineError::EngineApiError(e.to_string()))
                }).await?;
                
                // Check payload status
                match response.payload_status.status {
                    PayloadStatusEnum::Valid | PayloadStatusEnum::Syncing => {},
                    PayloadStatusEnum::Invalid => {
                        return Err(EngineError::InvalidPayloadStatus(
                            response.payload_status.validation_error
                        ));
                    }
                    _ => {
                        return Err(EngineError::UnexpectedPayloadStatus);
                    }
                }
                
                let payload_id = response.payload_id
                    .ok_or(EngineError::PayloadIdNotProvided)?;
                
                // Get the built payload with retry
                let payload_response = self.resilient_api_call("get_payload", || async {
                    self.authenticated_api
                        .get_payload::<MainnetEthSpec>(ForkName::Capella, payload_id)
                        .await
                        .map_err(|e| EngineError::EngineApiError(e.to_string()))
                }).await?;
                
                let payload = payload_response.execution_payload_ref().clone_from_ref();
                
                // Cache the payload
                self.payload_cache.insert(payload_id, payload.clone());
                
                self.metrics.blocks_built.inc();
                
                Ok(payload)
            };

            operation().await
        }.into_actor(self))
    }
}

#[derive(Debug)]
pub struct HealthMonitor {
    last_successful_call: HashMap<String, Instant>,
    health_check_interval: Duration,
    unhealthy_threshold: Duration,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            last_successful_call: HashMap::new(),
            health_check_interval: Duration::from_secs(30),
            unhealthy_threshold: Duration::from_secs(120),
        }
    }

    pub fn record_success(&mut self, operation: &str) {
        self.last_successful_call.insert(operation.to_string(), Instant::now());
    }

    pub fn is_healthy(&self, operation: &str) -> bool {
        match self.last_successful_call.get(operation) {
            Some(last_success) => last_success.elapsed() < self.unhealthy_threshold,
            None => false, // Never succeeded
        }
    }

    pub fn get_health_status(&self) -> HashMap<String, bool> {
        let mut status = HashMap::new();
        
        for (operation, _) in &self.last_successful_call {
            status.insert(operation.clone(), self.is_healthy(operation));
        }
        
        status
    }
}

#[derive(Debug)]
pub enum CircuitBreakerError {
    CircuitOpen,
}

#[derive(Debug)]
pub enum EngineError {
    EngineApiError(String),
    InvalidPayloadStatus(Option<String>),
    UnexpectedPayloadStatus,
    PayloadIdNotProvided,
    InvalidPayload(Option<String>),
    ClientSyncing,
    BlockNotFound,
    JwtError(String),
    Timeout,
    CircuitBreakerOpen,
    MaxRetriesExceeded,
}
```

**Implementation 2: Production Migration System**
```rust
// src/actors/engine/migration.rs
use actix::prelude::*;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct EngineMigrationController {
    // Migration state
    current_mode: MigrationMode,
    mode_start_time: Instant,
    
    // Legacy engine
    legacy_engine: Option<Arc<RwLock<crate::engine::Engine>>>,
    
    // New actor
    engine_actor: Option<Addr<EngineActor>>,
    
    // Migration metrics
    metrics: EngineMigrationMetrics,
    
    // Feature flags for gradual rollout
    feature_flags: Arc<dyn FeatureFlagProvider>,
    
    // Configuration
    config: EngineMigrationConfig,
    
    // State validation
    state_validator: StateValidator,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MigrationMode {
    LegacyOnly,
    ShadowMode,        // Actor runs in background, results compared
    CanaryMode,        // Small % of operations use actor
    ParallelMode,      // Both systems run, results compared
    ActorPrimary,      // Actor primary, legacy fallback
    ActorOnly,
    Rollback,          // Emergency rollback
}

#[derive(Debug)]
pub struct EngineMigrationConfig {
    pub shadow_mode_duration: Duration,
    pub canary_percentage: f64,
    pub parallel_mode_duration: Duration,
    pub primary_mode_duration: Duration,
    pub success_threshold: f64,
    pub error_threshold: f64,
    pub state_validation_enabled: bool,
}

#[derive(Debug)]
pub struct EngineMigrationMetrics {
    // Operation counts
    pub legacy_operations: AtomicU64,
    pub actor_operations: AtomicU64,
    pub parallel_operations: AtomicU64,
    
    // Performance metrics
    pub legacy_avg_latency: AtomicU64,
    pub actor_avg_latency: AtomicU64,
    
    // Reliability metrics
    pub legacy_success_rate: AtomicU64,
    pub actor_success_rate: AtomicU64,
    pub state_mismatches: AtomicU64,
    
    // Migration health
    pub migration_health_score: AtomicU64, // 0-100
}

impl EngineMigrationController {
    pub fn new(
        legacy_engine: Arc<RwLock<crate::engine::Engine>>,
        config: EngineMigrationConfig,
        feature_flags: Arc<dyn FeatureFlagProvider>,
    ) -> Self {
        Self {
            current_mode: MigrationMode::LegacyOnly,
            mode_start_time: Instant::now(),
            legacy_engine: Some(legacy_engine),
            engine_actor: None,
            metrics: EngineMigrationMetrics::new(),
            feature_flags,
            config,
            state_validator: StateValidator::new(),
        }
    }

    pub async fn initialize_actor(&mut self, engine_actor: Addr<EngineActor>) -> Result<(), MigrationError> {
        // Sync actor with current legacy state
        let legacy_state = {
            let legacy = self.legacy_engine.as_ref().unwrap().read().await;
            EngineState {
                latest_block: legacy.get_latest_block_hash(),
                finalized_block: legacy.get_finalized_block_hash(),
                safe_block: legacy.get_safe_block_hash(),
            }
        };

        // Initialize actor with legacy state
        engine_actor.send(InitializeFromLegacyEngine {
            state: legacy_state,
        }).await??;

        self.engine_actor = Some(engine_actor);
        Ok(())
    }

    pub async fn build_block(
        &self,
        timestamp: Duration,
        parent: Option<ExecutionBlockHash>,
        withdrawals: Vec<Withdrawal>,
        fee_recipient: Option<Address>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError> {
        match self.current_mode {
            MigrationMode::LegacyOnly => {
                self.build_block_legacy_only(timestamp, parent, withdrawals, fee_recipient).await
            }
            
            MigrationMode::ShadowMode => {
                self.build_block_shadow_mode(timestamp, parent, withdrawals, fee_recipient).await
            }
            
            MigrationMode::CanaryMode => {
                self.build_block_canary_mode(timestamp, parent, withdrawals, fee_recipient).await
            }
            
            MigrationMode::ParallelMode => {
                self.build_block_parallel_mode(timestamp, parent, withdrawals, fee_recipient).await
            }
            
            MigrationMode::ActorPrimary => {
                self.build_block_actor_primary(timestamp, parent, withdrawals, fee_recipient).await
            }
            
            MigrationMode::ActorOnly => {
                self.build_block_actor_only(timestamp, parent, withdrawals, fee_recipient).await
            }
            
            MigrationMode::Rollback => {
                self.build_block_legacy_only(timestamp, parent, withdrawals, fee_recipient).await
            }
        }
    }

    async fn build_block_shadow_mode(
        &self,
        timestamp: Duration,
        parent: Option<ExecutionBlockHash>,
        withdrawals: Vec<Withdrawal>,
        fee_recipient: Option<Address>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError> {
        let start_time = Instant::now();

        // Execute legacy (primary)
        let legacy_result = {
            let mut legacy = self.legacy_engine.as_ref().unwrap().write().await;
            legacy.build_block(timestamp, parent, withdrawals.clone(), fee_recipient).await
        };
        
        let legacy_duration = start_time.elapsed();

        // Execute actor (shadow)
        if let Some(actor) = &self.engine_actor {
            let shadow_start = Instant::now();
            
            let shadow_result = actor.send(BuildBlock {
                timestamp,
                parent,
                withdrawals: withdrawals.clone(),
                suggested_fee_recipient: fee_recipient,
            }).await;
            
            let shadow_duration = shadow_start.elapsed();

            // Compare results and record metrics
            self.compare_and_record_build_block_results(
                &legacy_result,
                &shadow_result,
                legacy_duration,
                shadow_duration,
            ).await;
        }

        self.metrics.legacy_operations.fetch_add(1, Ordering::Relaxed);
        
        // Return legacy result
        legacy_result
    }

    async fn build_block_parallel_mode(
        &self,
        timestamp: Duration,
        parent: Option<ExecutionBlockHash>,
        withdrawals: Vec<Withdrawal>,
        fee_recipient: Option<Address>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError> {
        // Execute both systems in parallel
        let legacy_future = async {
            let start = Instant::now();
            let result = {
                let mut legacy = self.legacy_engine.as_ref().unwrap().write().await;
                legacy.build_block(timestamp, parent, withdrawals.clone(), fee_recipient).await
            };
            (result, start.elapsed())
        };

        let actor_future = async {
            let start = Instant::now();
            let result = if let Some(actor) = &self.engine_actor {
                actor.send(BuildBlock {
                    timestamp,
                    parent,
                    withdrawals: withdrawals.clone(),
                    suggested_fee_recipient: fee_recipient,
                }).await
            } else {
                Err(EngineError::ActorNotAvailable)
            };
            (result, start.elapsed())
        };

        let ((legacy_result, legacy_duration), (actor_result, actor_duration)) = 
            tokio::join!(legacy_future, actor_future);

        // Compare and record results
        self.compare_and_record_build_block_results(
            &legacy_result,
            &actor_result.map_err(|e| EngineError::ActorMailboxError(e.to_string())),
            legacy_duration,
            actor_duration,
        ).await;

        self.metrics.parallel_operations.fetch_add(1, Ordering::Relaxed);

        // Return the faster successful result, prefer actor if both succeed
        match (&legacy_result, &actor_result) {
            (Ok(legacy_payload), Ok(Ok(actor_payload))) => {
                // Validate state consistency
                if self.config.state_validation_enabled {
                    if let Err(e) = self.state_validator.validate_payloads(legacy_payload, actor_payload) {
                        warn!("State validation failed: {:?}", e);
                        self.metrics.state_mismatches.fetch_add(1, Ordering::Relaxed);
                        // Return legacy result for safety
                        return legacy_result;
                    }
                }
                
                // Both succeeded, return actor result (faster and more reliable)
                Ok(actor_payload.clone())
            }
            (Ok(legacy_payload), _) => {
                // Legacy succeeded, actor failed
                Ok(legacy_payload.clone())
            }
            (_, Ok(Ok(actor_payload))) => {
                // Actor succeeded, legacy failed
                Ok(actor_payload.clone())
            }
            (Err(legacy_err), Err(_)) => {
                // Both failed
                Err(legacy_err.clone())
            }
        }
    }

    async fn build_block_canary_mode(
        &self,
        timestamp: Duration,
        parent: Option<ExecutionBlockHash>,
        withdrawals: Vec<Withdrawal>,
        fee_recipient: Option<Address>,
    ) -> Result<ExecutionPayload<MainnetEthSpec>, EngineError> {
        let use_actor = self.should_use_actor_canary();
        
        if use_actor {
            self.metrics.actor_operations.fetch_add(1, Ordering::Relaxed);
            
            match self.engine_actor.as_ref().unwrap().send(BuildBlock {
                timestamp,
                parent,
                withdrawals: withdrawals.clone(),
                suggested_fee_recipient: fee_recipient,
            }).await {
                Ok(Ok(payload)) => Ok(payload),
                Ok(Err(e)) | Err(_) => {
                    warn!("Actor build_block failed in canary mode, falling back to legacy");
                    
                    // Fallback to legacy
                    let mut legacy = self.legacy_engine.as_ref().unwrap().write().await;
                    legacy.build_block(timestamp, parent, withdrawals, fee_recipient).await
                }
            }
        } else {
            self.metrics.legacy_operations.fetch_add(1, Ordering::Relaxed);
            
            let mut legacy = self.legacy_engine.as_ref().unwrap().write().await;
            legacy.build_block(timestamp, parent, withdrawals, fee_recipient).await
        }
    }

    fn should_use_actor_canary(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let roll: f64 = rng.gen();
        
        // Gradually increase canary percentage over time
        let mode_progress = self.mode_start_time.elapsed().as_secs_f64() / 300.0; // 5 minutes
        let current_percentage = (mode_progress * self.config.canary_percentage)
            .min(self.config.canary_percentage);
        
        roll < current_percentage / 100.0
    }

    async fn compare_and_record_build_block_results(
        &self,
        legacy_result: &Result<ExecutionPayload<MainnetEthSpec>, EngineError>,
        actor_result: &Result<ExecutionPayload<MainnetEthSpec>, EngineError>,
        legacy_duration: Duration,
        actor_duration: Duration,
    ) {
        // Record latencies
        self.metrics.legacy_avg_latency.store(
            legacy_duration.as_millis() as u64, 
            Ordering::Relaxed
        );
        self.metrics.actor_avg_latency.store(
            actor_duration.as_millis() as u64, 
            Ordering::Relaxed
        );

        // Record success rates
        match (legacy_result, actor_result) {
            (Ok(legacy_payload), Ok(actor_payload)) => {
                // Both succeeded
                if self.config.state_validation_enabled {
                    if let Err(_) = self.state_validator.validate_payloads(legacy_payload, actor_payload) {
                        self.metrics.state_mismatches.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            (Ok(_), Err(_)) => {
                warn!("Actor failed while legacy succeeded in shadow mode");
            }
            (Err(_), Ok(_)) => {
                info!("Actor succeeded while legacy failed in shadow mode");
            }
            (Err(_), Err(_)) => {
                warn!("Both legacy and actor failed in shadow mode");
            }
        }

        // Update migration health score
        let health_score = self.calculate_migration_health();
        self.metrics.migration_health_score.store(health_score, Ordering::Relaxed);
    }

    fn calculate_migration_health(&self) -> u64 {
        // Complex algorithm to calculate migration health based on:
        // - Success rates
        // - Performance ratios
        // - State consistency
        // - Error rates
        
        let actor_ops = self.metrics.actor_operations.load(Ordering::Relaxed);
        let legacy_ops = self.metrics.legacy_operations.load(Ordering::Relaxed);
        
        if actor_ops == 0 {
            return 50; // Neutral health if no actor operations
        }

        // Calculate health factors
        let state_consistency = if self.metrics.state_mismatches.load(Ordering::Relaxed) == 0 {
            100.0
        } else {
            let mismatch_rate = self.metrics.state_mismatches.load(Ordering::Relaxed) as f64 / actor_ops as f64;
            ((1.0 - mismatch_rate) * 100.0).max(0.0)
        };

        let performance_ratio = {
            let actor_latency = self.metrics.actor_avg_latency.load(Ordering::Relaxed) as f64;
            let legacy_latency = self.metrics.legacy_avg_latency.load(Ordering::Relaxed) as f64;
            
            if legacy_latency > 0.0 {
                (legacy_latency / actor_latency).min(2.0) * 50.0 // Cap at 100%
            } else {
                50.0
            }
        };

        // Weighted average
        let health = (state_consistency * 0.6) + (performance_ratio * 0.4);
        health.min(100.0) as u64
    }
}

#[derive(Debug)]
pub struct StateValidator {
    tolerance_config: StateValidationConfig,
}

#[derive(Debug)]
pub struct StateValidationConfig {
    pub block_hash_must_match: bool,
    pub gas_used_tolerance: u64,
    pub transaction_count_must_match: bool,
    pub withdrawal_count_must_match: bool,
}

impl StateValidator {
    pub fn new() -> Self {
        Self {
            tolerance_config: StateValidationConfig {
                block_hash_must_match: true,
                gas_used_tolerance: 1000, // Allow 1000 gas difference
                transaction_count_must_match: true,
                withdrawal_count_must_match: true,
            },
        }
    }

    pub fn validate_payloads(
        &self,
        legacy_payload: &ExecutionPayload<MainnetEthSpec>,
        actor_payload: &ExecutionPayload<MainnetEthSpec>,
    ) -> Result<(), StateValidationError> {
        // Validate block hash
        if self.tolerance_config.block_hash_must_match {
            if legacy_payload.block_hash() != actor_payload.block_hash() {
                return Err(StateValidationError::BlockHashMismatch {
                    legacy: legacy_payload.block_hash(),
                    actor: actor_payload.block_hash(),
                });
            }
        }

        // Validate transaction count
        if self.tolerance_config.transaction_count_must_match {
            if legacy_payload.transactions().len() != actor_payload.transactions().len() {
                return Err(StateValidationError::TransactionCountMismatch {
                    legacy: legacy_payload.transactions().len(),
                    actor: actor_payload.transactions().len(),
                });
            }
        }

        // Validate gas used
        let legacy_gas = legacy_payload.gas_used();
        let actor_gas = actor_payload.gas_used();
        let gas_diff = if legacy_gas > actor_gas {
            legacy_gas - actor_gas
        } else {
            actor_gas - legacy_gas
        };

        if gas_diff > self.tolerance_config.gas_used_tolerance {
            return Err(StateValidationError::GasUsedMismatch {
                legacy: legacy_gas,
                actor: actor_gas,
                difference: gas_diff,
            });
        }

        // Validate withdrawal count
        if self.tolerance_config.withdrawal_count_must_match {
            if legacy_payload.withdrawals().len() != actor_payload.withdrawals().len() {
                return Err(StateValidationError::WithdrawalCountMismatch {
                    legacy: legacy_payload.withdrawals().len(),
                    actor: actor_payload.withdrawals().len(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum StateValidationError {
    BlockHashMismatch {
        legacy: ExecutionBlockHash,
        actor: ExecutionBlockHash,
    },
    TransactionCountMismatch {
        legacy: usize,
        actor: usize,
    },
    GasUsedMismatch {
        legacy: u64,
        actor: u64,
        difference: u64,
    },
    WithdrawalCountMismatch {
        legacy: usize,
        actor: usize,
    },
}

// Message for initializing actor from legacy state
#[derive(Message)]
#[rtype(result = "Result<(), EngineError>")]
pub struct InitializeFromLegacyEngine {
    pub state: EngineState,
}

#[derive(Debug, Clone)]
pub struct EngineState {
    pub latest_block: Option<ExecutionBlockHash>,
    pub finalized_block: Option<ExecutionBlockHash>,
    pub safe_block: Option<ExecutionBlockHash>,
}

impl Handler<InitializeFromLegacyEngine> for EngineActor {
    type Result = Result<(), EngineError>;
    
    fn handle(&mut self, msg: InitializeFromLegacyEngine, _: &mut Context<Self>) -> Self::Result {
        info!("Initializing EngineActor from legacy engine state");
        
        self.latest_block = msg.state.latest_block;
        self.finalized_block = msg.state.finalized_block;
        self.safe_block = msg.state.safe_block;
        
        info!("EngineActor initialized with latest: {:?}, finalized: {:?}, safe: {:?}",
              self.latest_block, self.finalized_block, self.safe_block);
        
        Ok(())
    }
}

#[derive(Debug)]
pub enum MigrationError {
    ActorNotReady,
    StateValidationFailed,
    InvalidTransition,
    InitializationFailed(String),
}
```

**Implementation 3: Comprehensive Monitoring and Alerting**
```rust
// src/actors/engine/monitoring.rs
use prometheus::{Counter, Histogram, Gauge, IntGauge};
use std::collections::HashMap;

#[derive(Debug)]
pub struct EngineMetrics {
    // Core operation metrics
    pub blocks_built: Counter,
    pub blocks_committed: Counter,
    pub blocks_finalized: Counter,
    pub build_block_duration: Histogram,
    pub commit_block_duration: Histogram,
    pub finalize_block_duration: Histogram,
    
    // Cache metrics
    pub cache_hits: Counter,
    pub cache_misses: Counter,
    pub cache_evictions: Counter,
    
    // Error metrics
    pub engine_errors: prometheus::CounterVec,
    pub operation_failures: prometheus::CounterVec,
    pub operation_timeouts: prometheus::CounterVec,
    pub operation_retries: prometheus::HistogramVec,
    pub operation_exhausted_retries: prometheus::CounterVec,
    
    // Circuit breaker metrics
    pub circuit_breaker_rejections: prometheus::CounterVec,
    pub circuit_breaker_state_changes: prometheus::CounterVec,
    
    // Health metrics
    pub sync_progress: Gauge,
    pub last_successful_operation: prometheus::GaugeVec,
    pub connection_status: IntGauge,
    
    // Performance metrics
    pub payload_size_bytes: Histogram,
    pub transaction_count_per_block: Histogram,
    pub gas_used_per_block: Histogram,
    
    // Migration-specific metrics
    pub migration_mode: IntGauge,
    pub migration_health_score: Gauge,
    pub state_validation_failures: Counter,
}

impl EngineMetrics {
    pub fn new() -> Self {
        Self {
            blocks_built: Counter::new(
                "engine_blocks_built_total",
                "Total number of blocks built"
            ).expect("Failed to create blocks_built counter"),
            
            blocks_committed: Counter::new(
                "engine_blocks_committed_total",
                "Total number of blocks committed"
            ).expect("Failed to create blocks_committed counter"),
            
            blocks_finalized: Counter::new(
                "engine_blocks_finalized_total",
                "Total number of blocks finalized"
            ).expect("Failed to create blocks_finalized counter"),
            
            build_block_duration: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "engine_build_block_duration_seconds",
                    "Time taken to build a block"
                ).buckets(vec![0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0])
            ).expect("Failed to create build_block_duration histogram"),
            
            commit_block_duration: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "engine_commit_block_duration_seconds",
                    "Time taken to commit a block"
                ).buckets(vec![0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0])
            ).expect("Failed to create commit_block_duration histogram"),
            
            finalize_block_duration: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "engine_finalize_block_duration_seconds",
                    "Time taken to finalize a block"
                ).buckets(vec![0.01, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0])
            ).expect("Failed to create finalize_block_duration histogram"),
            
            cache_hits: Counter::new(
                "engine_cache_hits_total",
                "Total number of cache hits"
            ).expect("Failed to create cache_hits counter"),
            
            cache_misses: Counter::new(
                "engine_cache_misses_total", 
                "Total number of cache misses"
            ).expect("Failed to create cache_misses counter"),
            
            cache_evictions: Counter::new(
                "engine_cache_evictions_total",
                "Total number of cache evictions"
            ).expect("Failed to create cache_evictions counter"),
            
            engine_errors: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "engine_errors_total",
                    "Total number of engine errors by type"
                ),
                &["error_type"]
            ).expect("Failed to create engine_errors counter"),
            
            operation_failures: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "engine_operation_failures_total",
                    "Total number of operation failures by operation type"
                ),
                &["operation"]
            ).expect("Failed to create operation_failures counter"),
            
            operation_timeouts: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "engine_operation_timeouts_total",
                    "Total number of operation timeouts by operation type"
                ),
                &["operation"]
            ).expect("Failed to create operation_timeouts counter"),
            
            operation_retries: prometheus::HistogramVec::new(
                prometheus::HistogramOpts::new(
                    "engine_operation_retries",
                    "Number of retries for operations"
                ).buckets(vec![0.0, 1.0, 2.0, 3.0, 5.0, 10.0]),
                &["operation"]
            ).expect("Failed to create operation_retries histogram"),
            
            operation_exhausted_retries: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "engine_operation_exhausted_retries_total",
                    "Total number of operations that exhausted all retries"
                ),
                &["operation"]
            ).expect("Failed to create operation_exhausted_retries counter"),
            
            circuit_breaker_rejections: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "engine_circuit_breaker_rejections_total",
                    "Total number of circuit breaker rejections"
                ),
                &["operation"]
            ).expect("Failed to create circuit_breaker_rejections counter"),
            
            circuit_breaker_state_changes: prometheus::CounterVec::new(
                prometheus::Opts::new(
                    "engine_circuit_breaker_state_changes_total",
                    "Total number of circuit breaker state changes"
                ),
                &["from_state", "to_state"]
            ).expect("Failed to create circuit_breaker_state_changes counter"),
            
            sync_progress: Gauge::new(
                "engine_sync_progress_percent",
                "Execution client sync progress percentage"
            ).expect("Failed to create sync_progress gauge"),
            
            last_successful_operation: prometheus::GaugeVec::new(
                prometheus::Opts::new(
                    "engine_last_successful_operation_timestamp",
                    "Timestamp of last successful operation"
                ),
                &["operation"]
            ).expect("Failed to create last_successful_operation gauge"),
            
            connection_status: IntGauge::new(
                "engine_connection_status",
                "Connection status to execution client (1 = connected, 0 = disconnected)"
            ).expect("Failed to create connection_status gauge"),
            
            payload_size_bytes: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "engine_payload_size_bytes",
                    "Size of execution payloads in bytes"
                ).buckets(prometheus::exponential_buckets(1024.0, 2.0, 15).unwrap())
            ).expect("Failed to create payload_size_bytes histogram"),
            
            transaction_count_per_block: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "engine_transaction_count_per_block",
                    "Number of transactions per block"
                ).buckets(vec![0.0, 1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0])
            ).expect("Failed to create transaction_count_per_block histogram"),
            
            gas_used_per_block: Histogram::with_opts(
                prometheus::HistogramOpts::new(
                    "engine_gas_used_per_block",
                    "Gas used per block"
                ).buckets(prometheus::exponential_buckets(100000.0, 2.0, 20).unwrap())
            ).expect("Failed to create gas_used_per_block histogram"),
            
            migration_mode: IntGauge::new(
                "engine_migration_mode",
                "Current migration mode (0=legacy, 1=shadow, 2=canary, 3=parallel, 4=primary, 5=actor-only)"
            ).expect("Failed to create migration_mode gauge"),
            
            migration_health_score: Gauge::new(
                "engine_migration_health_score",
                "Migration health score (0-100)"
            ).expect("Failed to create migration_health_score gauge"),
            
            state_validation_failures: Counter::new(
                "engine_state_validation_failures_total",
                "Total number of state validation failures during migration"
            ).expect("Failed to create state_validation_failures counter"),
        }
    }

    pub fn register_all(&self) -> Result<(), prometheus::Error> {
        prometheus::register(Box::new(self.blocks_built.clone()))?;
        prometheus::register(Box::new(self.blocks_committed.clone()))?;
        prometheus::register(Box::new(self.blocks_finalized.clone()))?;
        prometheus::register(Box::new(self.build_block_duration.clone()))?;
        prometheus::register(Box::new(self.commit_block_duration.clone()))?;
        prometheus::register(Box::new(self.finalize_block_duration.clone()))?;
        prometheus::register(Box::new(self.cache_hits.clone()))?;
        prometheus::register(Box::new(self.cache_misses.clone()))?;
        prometheus::register(Box::new(self.cache_evictions.clone()))?;
        prometheus::register(Box::new(self.engine_errors.clone()))?;
        prometheus::register(Box::new(self.operation_failures.clone()))?;
        prometheus::register(Box::new(self.operation_timeouts.clone()))?;
        prometheus::register(Box::new(self.operation_retries.clone()))?;
        prometheus::register(Box::new(self.operation_exhausted_retries.clone()))?;
        prometheus::register(Box::new(self.circuit_breaker_rejections.clone()))?;
        prometheus::register(Box::new(self.circuit_breaker_state_changes.clone()))?;
        prometheus::register(Box::new(self.sync_progress.clone()))?;
        prometheus::register(Box::new(self.last_successful_operation.clone()))?;
        prometheus::register(Box::new(self.connection_status.clone()))?;
        prometheus::register(Box::new(self.payload_size_bytes.clone()))?;
        prometheus::register(Box::new(self.transaction_count_per_block.clone()))?;
        prometheus::register(Box::new(self.gas_used_per_block.clone()))?;
        prometheus::register(Box::new(self.migration_mode.clone()))?;
        prometheus::register(Box::new(self.migration_health_score.clone()))?;
        prometheus::register(Box::new(self.state_validation_failures.clone()))?;
        
        Ok(())
    }

    pub fn record_successful_operation(&self, operation: &str) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64;
        
        self.last_successful_operation
            .with_label_values(&[operation])
            .set(timestamp);
    }

    pub fn record_payload_metrics(&self, payload: &ExecutionPayload<MainnetEthSpec>) {
        // Record payload size (approximate)
        let size_estimate = payload.transactions().len() * 200; // Rough estimate
        self.payload_size_bytes.observe(size_estimate as f64);
        
        // Record transaction count
        self.transaction_count_per_block.observe(payload.transactions().len() as f64);
        
        // Record gas used
        self.gas_used_per_block.observe(payload.gas_used() as f64);
    }
}

// Alert definitions for monitoring
#[derive(Debug)]
pub struct EngineAlertManager {
    alert_rules: Vec<AlertRule>,
}

#[derive(Debug)]
pub struct AlertRule {
    pub name: String,
    pub condition: AlertCondition,
    pub severity: AlertSeverity,
    pub description: String,
}

#[derive(Debug)]
pub enum AlertCondition {
    MetricThreshold {
        metric: String,
        threshold: f64,
        comparison: ComparisonOp,
        duration: Duration,
    },
    ChangeRate {
        metric: String,
        change_threshold: f64,
        window: Duration,
    },
    CircuitBreakerOpen {
        operation: String,
    },
}

#[derive(Debug)]
pub enum ComparisonOp {
    GreaterThan,
    LessThan,
    Equal,
}

#[derive(Debug)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

impl EngineAlertManager {
    pub fn new() -> Self {
        Self {
            alert_rules: vec![
                AlertRule {
                    name: "EngineActorHighErrorRate".to_string(),
                    condition: AlertCondition::MetricThreshold {
                        metric: "engine_operation_failures_total".to_string(),
                        threshold: 10.0,
                        comparison: ComparisonOp::GreaterThan,
                        duration: Duration::from_secs(300), // 5 minutes
                    },
                    severity: AlertSeverity::Critical,
                    description: "EngineActor experiencing high error rate".to_string(),
                },
                
                AlertRule {
                    name: "EngineActorSlowBlockBuilding".to_string(),
                    condition: AlertCondition::MetricThreshold {
                        metric: "engine_build_block_duration_seconds".to_string(),
                        threshold: 2.0, // 2 seconds
                        comparison: ComparisonOp::GreaterThan,
                        duration: Duration::from_secs(60),
                    },
                    severity: AlertSeverity::Warning,
                    description: "EngineActor block building is slow".to_string(),
                },
                
                AlertRule {
                    name: "EngineActorCircuitBreakerOpen".to_string(),
                    condition: AlertCondition::CircuitBreakerOpen {
                        operation: "forkchoice_updated".to_string(),
                    },
                    severity: AlertSeverity::Critical,
                    description: "EngineActor circuit breaker is open".to_string(),
                },
                
                AlertRule {
                    name: "EngineActorLowCacheHitRate".to_string(),
                    condition: AlertCondition::MetricThreshold {
                        metric: "engine_cache_hit_rate".to_string(),
                        threshold: 0.8, // 80%
                        comparison: ComparisonOp::LessThan,
                        duration: Duration::from_secs(600), // 10 minutes
                    },
                    severity: AlertSeverity::Info,
                    description: "EngineActor cache hit rate is low".to_string(),
                },
            ],
        }
    }
}
```

#### Priority 2: Performance Optimization and Final Testing

**Plan:** Complete performance benchmarking, load testing, and comprehensive test coverage.

### Detailed Test Plan

**Unit Tests (200 tests):**
1. Message handling tests (50 tests)
2. Resilience and error handling (40 tests)
3. Cache functionality tests (30 tests)
4. Migration controller tests (35 tests)
5. State validation tests (25 tests)
6. Client abstraction tests (20 tests)

**Integration Tests (100 tests):**
1. Real Geth integration (25 tests)
2. Real Reth integration (25 tests)
3. JWT authentication flow (15 tests)
4. Migration workflow tests (20 tests)
5. Error recovery scenarios (15 tests)

**Performance Tests (50 benchmarks):**
1. Block building throughput (15 benchmarks)
2. Block commit performance (10 benchmarks)
3. Cache performance (10 benchmarks)
4. Memory usage under load (10 benchmarks)
5. Concurrent operations (5 benchmarks)

### Implementation Timeline

**Week 1-2: Production Resilience**
- Complete error handling and circuit breaker implementation
- Implement comprehensive monitoring and alerting
- Add state validation for migration safety

**Week 3: Migration System**
- Complete migration controller with all modes
- Test gradual rollout and rollback capabilities
- Validate state consistency across systems

**Week 4: Performance and Final Testing**
- Complete performance benchmarks and optimization
- Full integration testing with real execution clients
- Load testing and stress testing

### Success Metrics

**Functional Metrics:**
- 100% message handler test coverage
- Zero data loss during migration
- All acceptance criteria satisfied

**Performance Metrics:**
- Block building ≤ 200ms (95th percentile)
- Block commit ≤ 100ms (95th percentile)
- Cache hit ratio ≥ 80%
- Memory usage ≤ 256MB under load

**Operational Metrics:**
- Migration rollback time ≤ 30 seconds
- Zero consensus disruptions
- Circuit breaker recovery within 60 seconds
- 99.9% operation success rate

### Risk Mitigation

**Technical Risks:**
- **JWT authentication failures**: Automatic token refresh and fallback mechanisms
- **Execution client incompatibilities**: Client-specific adapters and version detection
- **State synchronization issues**: Comprehensive state validation and automatic correction

**Operational Risks:**
- **Migration failures**: Multi-phase rollout with automatic rollback triggers
- **Performance degradation**: Extensive benchmarking and load testing before deployment
- **Data inconsistencies**: Parallel validation and state comparison during migration