# ALYS-008: Implement EngineActor

## Description

Implement the EngineActor to replace the current Engine struct with a message-driven actor. This actor manages all interactions with the execution layer (Reth), handling block building, payload validation, and finalization without shared mutable state.

## Subtasks

- [ ] Create ALYS-008-1: Design EngineActor message protocol with execution layer operations [https://marathondh.atlassian.net/browse/AN-414]
- [ ] Create ALYS-008-2: Implement EngineActor core structure with JWT authentication [https://marathondh.atlassian.net/browse/AN-415]
- [ ] Create ALYS-008-3: Implement block building logic with payload generation [https://marathondh.atlassian.net/browse/AN-416]
- [ ] Create ALYS-008-4: Implement block commit and forkchoice update pipeline [https://marathondh.atlassian.net/browse/AN-417]
- [ ] Create ALYS-008-5: Implement block finalization and state management [https://marathondh.atlassian.net/browse/AN-418]
- [ ] Create ALYS-008-6: Implement execution client abstraction layer (Geth/Reth support) [https://marathondh.atlassian.net/browse/AN-419]
- [ ] Create ALYS-008-7: Implement caching system for payloads and blocks [https://marathondh.atlassian.net/browse/AN-420]
- [ ] Create ALYS-008-8: Create migration adapter for gradual Engine to EngineActor transition [https://marathondh.atlassian.net/browse/AN-421]
- [ ] Create ALYS-008-9: Implement comprehensive test suite (unit, integration, client compatibility) [https://marathondh.atlassian.net/browse/AN-423]
- [ ] Create ALYS-008-10: Performance benchmarking and optimization for execution operations [https://marathondh.atlassian.net/browse/AN-424]

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