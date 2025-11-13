# SyncActor Arc<RwLock<T>> Refactor Implementation Plan

**Date**: 2025-11-13
**Status**: Production-Ready Solution Design
**Estimated Effort**: 6-8 hours
**Complexity**: Medium (Architectural change with testing requirements)

---

## Executive Summary

The SyncActor currently has a **fundamental architectural flaw** where synchronous message handlers (`Handler<SyncMessage>`) cannot execute asynchronous workflow methods. This results in:

1. **Fresh nodes never synchronize** - StartSync handler only sets state variables but never triggers the async workflow
2. **Blocks queued but never imported** - HandleBlockResponse queues blocks but never processes them
3. **Fragile checkpoint loading** - Uses tokio::spawn workaround instead of proper async integration
4. **Disconnect between handlers and workflows** - Complete async workflow exists but is unreachable from synchronous handlers

### Root Cause Analysis

```rust
// Current problematic pattern:
impl Handler<SyncMessage> for SyncActor {
    type Result = Result<SyncResponse, SyncError>;  // ← SYNCHRONOUS

    fn handle(&mut self, msg: SyncMessage, ctx: &mut Context<Self>) -> Self::Result {
        // Cannot use .await here!
        // self.start_sync().await?;  // ← COMPILER ERROR

        // Can only set state variables:
        self.is_running = true;
        self.transition_to_state(SyncState::Starting);
        Ok(SyncResponse::Started)

        // Result: Workflows never execute, node never syncs
    }
}
```

### Solution: Arc<RwLock<T>> Pattern

Wrap mutable state in `Arc<RwLock<T>>`, enabling workflows to execute independently of handlers through actor context scheduling.

**Key Benefits**:
- ✅ Handlers remain synchronous (fast response)
- ✅ Workflows execute asynchronously (can use .await)
- ✅ State shared safely between handler and workflow contexts
- ✅ No breaking changes to message interfaces
- ✅ Production-ready pattern used in distributed systems

---

## Problem Statement Deep Dive

### Current Architecture Issues

#### Issue 1: StartSync Handler Never Triggers Workflow

**Location**: `sync_actor.rs:1304-1362`

```rust
SyncMessage::StartSync { start_height, target_height } => {
    // Sets state variables only
    self.current_height = start_height;
    self.transition_to_state(SyncState::Starting);
    self.is_running = true;

    // Cannot call async workflow:
    // self.start_sync().await?;  // ← Would require async handler

    Ok(SyncResponse::Started)

    // Result: Node reports "sync started" but nothing happens
}
```

**Impact**: Fresh nodes stay at genesis block indefinitely. Logs show "Starting blockchain synchronization" but:
- `initialize_height()` never called
- `discover_sync_peers()` never executed
- `start_block_requests()` never triggered
- Node appears "running" but is completely idle

#### Issue 2: Block Queue Never Processes

**Location**: `sync_actor.rs:1425-1453`

```rust
SyncMessage::HandleBlockResponse { blocks, request_id } => {
    // Queues blocks successfully
    for block in blocks {
        self.block_queue.push_back((block, request_info.peer_id.clone()));
    }

    // But never processes them:
    // self.process_block_queue_optimized().await?;  // ← Cannot await

    Ok(SyncResponse::BlockProcessed { block_height: self.current_height })

    // Result: Blocks accumulate in memory, blockchain height never increases
}
```

**Impact**: Node receives blocks from network but blockchain remains frozen:
- Block queue grows unbounded (potential memory leak)
- Blocks validated but never imported to chain
- Sync progress shows 0% despite network activity
- Peers timeout waiting for responses

#### Issue 3: Checkpoint Loading Uses Workaround

**Location**: `sync_actor.rs:1518-1532`

```rust
SyncMessage::LoadCheckpoint => {
    // Spawns task with no way to update actor state
    tokio::spawn({
        let addr = _ctx.address();
        async move {
            // Comment admits defeat:
            // "This is a workaround since we can't call async methods from sync handlers"
            // "In practice, this is handled in Actor::started()"
        }
    });

    Ok(SyncResponse::Started)
}
```

**Impact**: Checkpoint loading is fragile and incomplete:
- Spawned task has no access to actor state
- Cannot update `current_height`, `target_height`, `sync_state`
- Checkpoint data loaded but never applied
- Restart after crash loses all sync progress

#### Issue 4: Workflow Methods Exist But Are Unreachable

**Implemented but unused workflows** (compiler warnings confirm):
- `start_sync()` - Line 109-126
- `initialize_height()` - Line 132-163
- `discover_sync_peers()` - Line 188-227
- `discover_target_height()` - Line 272-373
- `process_block_queue_optimized()` - Line 1082-1125
- `load_checkpoint()` - Line 1132-1190

**Impact**: ~800 lines of production-ready async code completely disconnected from message handling system.

---

## Solution Architecture

### Arc<RwLock<T>> Pattern Overview

```rust
// New state management pattern:
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SyncActor {
    // Shared state wrapped in Arc<RwLock<T>>
    state: Arc<RwLock<SyncActorState>>,

    // Immutable configuration (no lock needed)
    config: SyncConfig,

    // Actor addresses (no lock needed)
    network_actor: Option<Addr<NetworkActor>>,
    chain_actor: Option<Addr<ChainActor>>,
}

struct SyncActorState {
    // All mutable state moves here
    sync_state: SyncState,
    current_height: u64,
    target_height: u64,
    metrics: SyncMetrics,
    block_queue: VecDeque<(Block, PeerId)>,
    active_requests: HashMap<String, BlockRequestInfo>,
    sync_peers: Vec<PeerId>,
    peer_selection_index: usize,
    is_running: bool,
    shutdown_requested: bool,
    state_entered_at: SystemTime,
    discovery_time_accumulated: Duration,
}
```

### Handler Pattern (Synchronous)

```rust
impl Handler<SyncMessage> for SyncActor {
    type Result = Result<SyncResponse, SyncError>;

    fn handle(&mut self, msg: SyncMessage, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            SyncMessage::StartSync { start_height, target_height } => {
                // Quick validation (synchronous, no lock needed)
                let state_clone = Arc::clone(&self.state);

                // Schedule async workflow execution
                ctx.spawn(
                    async move {
                        let mut state = state_clone.write().await;

                        // Validate current state
                        if state.sync_state != SyncState::Stopped {
                            return; // Already running
                        }

                        // Update state
                        state.current_height = start_height;
                        state.is_running = true;
                        state.transition_to_state(SyncState::Starting);

                        // Release lock before calling workflow
                        drop(state);

                        // Execute workflow (can use .await)
                        if let Err(e) = Self::start_sync_workflow(state_clone).await {
                            tracing::error!("Sync workflow failed: {}", e);
                        }
                    }
                    .into_actor(self)
                );

                // Return immediately (non-blocking)
                Ok(SyncResponse::Started)
            }
        }
    }
}
```

### Workflow Pattern (Asynchronous)

```rust
impl SyncActor {
    /// Async workflow method (no &mut self, uses Arc<RwLock<State>>)
    async fn start_sync_workflow(
        state: Arc<RwLock<SyncActorState>>
    ) -> Result<()> {
        // Can use .await freely
        {
            let mut s = state.write().await;
            s.transition_to_state(SyncState::Starting);
        }

        // Execute workflow steps
        Self::initialize_height_workflow(state.clone()).await?;
        Self::discover_sync_peers_workflow(state.clone()).await?;
        Self::start_block_requests_workflow(state.clone()).await?;

        Ok(())
    }

    /// Individual workflow step
    async fn initialize_height_workflow(
        state: Arc<RwLock<SyncActorState>>
    ) -> Result<()> {
        // Query ChainActor (async)
        let height = /* chain_actor.send(...).await */;

        // Update state
        {
            let mut s = state.write().await;
            s.current_height = height;
        }

        Ok(())
    }
}
```

---

## Implementation Plan

### Phase 1: State Refactoring (2-3 hours)

#### Task 1.1: Create SyncActorState Struct
**File**: `app/src/actors_v2/network/sync_actor.rs`

```rust
/// Mutable state extracted for Arc<RwLock<T>> wrapping
#[derive(Clone)]
struct SyncActorState {
    // Move all mutable fields from SyncActor here
    sync_state: SyncState,
    current_height: u64,
    target_height: u64,
    metrics: SyncMetrics,
    block_queue: VecDeque<(Block, PeerId)>,
    active_requests: HashMap<String, BlockRequestInfo>,
    sync_peers: Vec<PeerId>,
    peer_selection_index: usize,
    is_running: bool,
    shutdown_requested: bool,
    state_entered_at: SystemTime,
    discovery_time_accumulated: Duration,
}

impl SyncActorState {
    fn new() -> Self {
        Self {
            sync_state: SyncState::Stopped,
            current_height: 0,
            target_height: 0,
            metrics: SyncMetrics::new(),
            block_queue: VecDeque::new(),
            active_requests: HashMap::new(),
            sync_peers: Vec::new(),
            peer_selection_index: 0,
            is_running: false,
            shutdown_requested: false,
            state_entered_at: SystemTime::now(),
            discovery_time_accumulated: Duration::ZERO,
        }
    }

    /// Transition to new state with timestamp tracking
    fn transition_to_state(&mut self, new_state: SyncState) {
        let previous_state = self.sync_state.clone();

        // Accumulate discovery time if leaving DiscoveringPeers
        if previous_state == SyncState::DiscoveringPeers {
            if let Ok(elapsed) = self.state_entered_at.elapsed() {
                self.discovery_time_accumulated += elapsed;
            }
        }

        // Reset discovery accumulator if stopping or synced
        if matches!(new_state, SyncState::Stopped | SyncState::Synced) {
            self.discovery_time_accumulated = Duration::ZERO;
        }

        self.sync_state = new_state.clone();
        self.state_entered_at = SystemTime::now();

        tracing::debug!(
            previous_state = ?previous_state,
            new_state = ?new_state,
            "Sync state transition"
        );
    }

    /// Get current sync status (no async needed)
    fn get_sync_status(&self) -> SyncStatus {
        SyncStatus {
            current_height: self.current_height,
            target_height: self.target_height,
            is_syncing: self.is_running,
            sync_peers: self.sync_peers.clone(),
            pending_requests: self.active_requests.len(),
        }
    }

    /// Select next peer (round-robin)
    fn select_sync_peer(&mut self) -> PeerId {
        if self.sync_peers.is_empty() {
            return "no_peers".to_string();
        }

        let peer = self.sync_peers[self.peer_selection_index].clone();
        self.peer_selection_index =
            (self.peer_selection_index + 1) % self.sync_peers.len();

        peer
    }

    /// Handle request timeouts
    fn handle_timeouts(&mut self, timeout: Duration) {
        let mut timed_out_requests = Vec::new();
        let now = SystemTime::now();

        for (request_id, request_info) in &self.active_requests {
            if let Ok(elapsed) = now.duration_since(request_info.requested_at) {
                if elapsed > timeout {
                    timed_out_requests.push(request_id.clone());
                }
            }
        }

        for request_id in timed_out_requests {
            if let Some(request_info) = self.active_requests.remove(&request_id) {
                tracing::warn!(
                    request_id = %request_id,
                    peer_id = %request_info.peer_id,
                    elapsed_secs = ?now.duration_since(request_info.requested_at),
                    "Block request timed out"
                );

                self.metrics.record_request_failure(&request_info.peer_id);
            }
        }
    }
}
```

**Validation**:
- [ ] Compile without errors
- [ ] All fields have appropriate types
- [ ] Helper methods work correctly

---

#### Task 1.2: Refactor SyncActor Struct
**File**: `app/src/actors_v2/network/sync_actor.rs`

```rust
pub struct SyncActor {
    /// Shared mutable state (wrapped for async access)
    state: Arc<RwLock<SyncActorState>>,

    /// Immutable configuration (no lock needed)
    config: SyncConfig,

    /// Actor addresses for coordination (set once, never mutated)
    network_actor: Option<Addr<NetworkActor>>,
    chain_actor: Option<Addr<ChainActor>>,
}

impl SyncActor {
    pub fn new(config: SyncConfig) -> Result<Self> {
        tracing::info!("Creating SyncActor V2 with Arc<RwLock<State>> pattern");

        config.validate()
            .map_err(|e| anyhow!("Invalid sync configuration: {}", e))?;

        Ok(Self {
            state: Arc::new(RwLock::new(SyncActorState::new())),
            config,
            network_actor: None,
            chain_actor: None,
        })
    }
}
```

**Validation**:
- [ ] Constructor compiles
- [ ] Actor initialization works
- [ ] Tests compile (may need updates)

---

#### Task 1.3: Update Actor Lifecycle
**File**: `app/src/actors_v2/network/sync_actor.rs`

```rust
impl Actor for SyncActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("SyncActor V2 started");

        // Schedule checkpoint loading
        let state = Arc::clone(&self.state);
        let data_dir = self.config.data_dir.clone();

        ctx.spawn(
            async move {
                if let Err(e) = Self::load_checkpoint_workflow(state, data_dir).await {
                    tracing::error!("Failed to load checkpoint: {}", e);
                }
            }
            .into_actor(self)
        );

        // Periodic timeout checking
        ctx.run_interval(Duration::from_secs(10), |act, _ctx| {
            let state = Arc::clone(&act.state);
            let timeout = act.config.sync_timeout;

            tokio::spawn(async move {
                let mut s = state.write().await;
                s.handle_timeouts(timeout);
            });
        });

        // Periodic sync progress updates
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            let state = Arc::clone(&act.state);

            tokio::spawn(async move {
                let s = state.read().await;
                if s.is_running {
                    let progress = s.metrics.get_sync_progress();
                    tracing::debug!(
                        "Sync progress: {:.1}% ({}/{})",
                        progress * 100.0,
                        s.current_height,
                        s.target_height
                    );
                }
            });
        });

        // Periodic checkpoint saving
        ctx.run_interval(Duration::from_secs(30), |act, _ctx| {
            let state = Arc::clone(&act.state);
            let data_dir = act.config.data_dir.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::save_checkpoint_workflow(state, data_dir).await {
                    tracing::error!("Failed to save checkpoint: {}", e);
                }
            });
        });
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("SyncActor V2 stopping");

        // Update state synchronously (blocking is acceptable in shutdown)
        let state = self.state.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut s = state.write().await;
                s.shutdown_requested = true;
                s.sync_state = SyncState::Stopped;
                s.is_running = false;
            })
        });

        Running::Stop
    }
}
```

**Validation**:
- [ ] Actor starts correctly
- [ ] Periodic tasks execute
- [ ] Shutdown cleans up state

---

### Phase 2: Handler Refactoring (2-3 hours)

#### Task 2.1: Refactor StartSync Handler
**File**: `app/src/actors_v2/network/sync_actor.rs`

```rust
impl Handler<SyncMessage> for SyncActor {
    type Result = Result<SyncResponse, SyncError>;

    fn handle(&mut self, msg: SyncMessage, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            SyncMessage::StartSync { start_height, target_height } => {
                tracing::info!(
                    start_height = start_height,
                    target_height = ?target_height,
                    "Received StartSync message"
                );

                // Clone Arc for workflow execution
                let state = Arc::clone(&self.state);
                let network_actor = self.network_actor.clone();
                let chain_actor = self.chain_actor.clone();

                // Schedule async workflow
                ctx.spawn(
                    async move {
                        // Acquire write lock
                        let mut s = state.write().await;

                        // Validate state
                        if s.sync_state != SyncState::Stopped
                            && s.sync_state != SyncState::Synced {
                            tracing::warn!(
                                state = ?s.sync_state,
                                "Sync already running, ignoring StartSync"
                            );
                            return;
                        }

                        // Update state
                        s.current_height = start_height;
                        s.target_height = target_height.unwrap_or(0);
                        s.is_running = true;
                        s.transition_to_state(SyncState::Starting);

                        // Check if already synced
                        const SYNC_THRESHOLD: u64 = 2;
                        if s.target_height > 0
                            && s.current_height + SYNC_THRESHOLD >= s.target_height {
                            tracing::info!(
                                current_height = s.current_height,
                                target_height = s.target_height,
                                "Already synced (within threshold)"
                            );
                            s.transition_to_state(SyncState::Synced);
                            s.is_running = false;
                            return;
                        }

                        // Release lock before workflow
                        drop(s);

                        // Execute async workflow
                        if let Err(e) = Self::start_sync_workflow(
                            state,
                            network_actor,
                            chain_actor
                        ).await {
                            tracing::error!("Sync workflow failed: {}", e);

                            let mut s = state.write().await;
                            s.transition_to_state(
                                SyncState::Error(format!("Sync failed: {}", e))
                            );
                            s.is_running = false;
                        }
                    }
                    .into_actor(self)
                );

                // Return immediately (non-blocking response)
                Ok(SyncResponse::Started)
            }

            SyncMessage::StopSync => {
                let state = Arc::clone(&self.state);

                ctx.spawn(
                    async move {
                        let mut s = state.write().await;
                        s.sync_state = SyncState::Stopped;
                        s.metrics.stop_sync();
                        s.is_running = false;

                        tracing::info!("Sync stopped");
                    }
                    .into_actor(self)
                );

                Ok(SyncResponse::Stopped)
            }

            SyncMessage::GetSyncStatus => {
                // Read-only access (can block briefly)
                let state = self.state.clone();
                let status = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let s = state.read().await;
                        s.get_sync_status()
                    })
                });

                Ok(SyncResponse::Status(status))
            }

            // Continue with remaining handlers...
        }
    }
}
```

**Key Pattern**: Synchronous handlers schedule async workflows using `ctx.spawn()`.

---

#### Task 2.2: Refactor Block Handling Handlers

```rust
SyncMessage::HandleBlockResponse { blocks, request_id } => {
    tracing::debug!(
        "Received {} blocks for request {}",
        blocks.len(),
        request_id
    );

    let state = Arc::clone(&self.state);
    let chain_actor = self.chain_actor.clone();

    // Schedule async processing
    ctx.spawn(
        async move {
            // Update state with received blocks
            {
                let mut s = state.write().await;

                // Find and complete the request
                if let Some(request_info) = s.active_requests.remove(&request_id) {
                    s.metrics.record_block_response(blocks.len() as u32);

                    // Queue blocks for processing
                    for block in blocks.clone() {
                        s.block_queue.push_back((block, request_info.peer_id.clone()));
                    }

                    tracing::debug!(
                        "Queued {} blocks (queue size: {})",
                        blocks.len(),
                        s.block_queue.len()
                    );
                }
            }

            // Process block queue asynchronously
            if let Err(e) = Self::process_block_queue_workflow(
                state,
                chain_actor
            ).await {
                tracing::error!("Block processing failed: {}", e);
            }
        }
        .into_actor(self)
    );

    Ok(SyncResponse::BlockProcessed {
        block_height: 0 // Will be updated by workflow
    })
}

SyncMessage::HandleNewBlock { block, peer_id } => {
    let state = Arc::clone(&self.state);
    let chain_actor = self.chain_actor.clone();

    ctx.spawn(
        async move {
            // Queue block
            {
                let mut s = state.write().await;
                s.block_queue.push_back((block, peer_id.clone()));

                tracing::debug!(
                    "Queued new block from peer {} (queue size: {})",
                    peer_id,
                    s.block_queue.len()
                );
            }

            // Process immediately if not already processing
            let should_process = {
                let s = state.read().await;
                s.sync_state != SyncState::ProcessingBlocks
            };

            if should_process {
                if let Err(e) = Self::process_block_queue_workflow(
                    state,
                    chain_actor
                ).await {
                    tracing::error!("Block processing failed: {}", e);
                }
            }
        }
        .into_actor(self)
    );

    Ok(SyncResponse::BlockProcessed { block_height: 0 })
}
```

**Validation**:
- [ ] Blocks are queued correctly
- [ ] Processing workflow triggers automatically
- [ ] No race conditions in queue access

---

#### Task 2.3: Refactor Checkpoint Handlers

```rust
SyncMessage::LoadCheckpoint => {
    tracing::debug!("Loading sync checkpoint");

    let state = Arc::clone(&self.state);
    let data_dir = self.config.data_dir.clone();

    ctx.spawn(
        async move {
            if let Err(e) = Self::load_checkpoint_workflow(
                state,
                data_dir
            ).await {
                tracing::error!("Failed to load checkpoint: {}", e);
            }
        }
        .into_actor(self)
    );

    Ok(SyncResponse::Started)
}

SyncMessage::SaveCheckpoint => {
    tracing::trace!("Saving sync checkpoint");

    let state = Arc::clone(&self.state);
    let data_dir = self.config.data_dir.clone();

    ctx.spawn(
        async move {
            if let Err(e) = Self::save_checkpoint_workflow(
                state,
                data_dir
            ).await {
                tracing::error!("Failed to save checkpoint: {}", e);
            }
        }
        .into_actor(self)
    );

    Ok(SyncResponse::Started)
}

SyncMessage::ClearCheckpoint => {
    tracing::debug!("Clearing sync checkpoint");

    let data_dir = self.config.data_dir.clone();

    tokio::spawn(async move {
        if let Err(e) = SyncCheckpoint::delete(&data_dir).await {
            tracing::error!("Failed to clear checkpoint: {}", e);
        }
    });

    Ok(SyncResponse::Started)
}
```

---

### Phase 3: Workflow Refactoring (2-3 hours)

#### Task 3.1: Refactor start_sync Workflow

```rust
impl SyncActor {
    /// Main synchronization workflow (now callable from handlers)
    async fn start_sync_workflow(
        state: Arc<RwLock<SyncActorState>>,
        network_actor: Option<Addr<NetworkActor>>,
        chain_actor: Option<Addr<ChainActor>>,
    ) -> Result<()> {
        tracing::info!("Executing start_sync workflow");

        // Transition to starting
        {
            let mut s = state.write().await;
            s.transition_to_state(SyncState::Starting);
        }

        // Initialize height from ChainActor
        Self::initialize_height_workflow(
            state.clone(),
            chain_actor.clone()
        ).await?;

        // Discover peers
        {
            let mut s = state.write().await;
            s.transition_to_state(SyncState::DiscoveringPeers);
        }

        Self::discover_sync_peers_workflow(
            state.clone(),
            network_actor.clone()
        ).await?;

        // Start requesting blocks
        Self::start_block_requests_workflow(
            state.clone(),
            network_actor.clone(),
            chain_actor.clone()
        ).await?;

        Ok(())
    }

    /// Initialize sync state by querying ChainActor for current chain height
    async fn initialize_height_workflow(
        state: Arc<RwLock<SyncActorState>>,
        chain_actor: Option<Addr<ChainActor>>,
    ) -> Result<()> {
        let chain_actor = chain_actor
            .ok_or_else(|| anyhow!("ChainActor not set"))?;

        // Query ChainActor for current chain state
        let msg = crate::actors_v2::chain::messages::ChainMessage::GetChainStatus;

        match chain_actor.send(msg).await {
            Ok(Ok(response)) => {
                use crate::actors_v2::chain::messages::ChainResponse;
                if let ChainResponse::ChainStatus(status) = response {
                    // Update state with current height
                    let mut s = state.write().await;
                    s.current_height = status.height;

                    tracing::info!(
                        current_height = status.height,
                        "Initialized sync from current chain height"
                    );

                    Ok(())
                } else {
                    Err(anyhow!("Unexpected response from ChainActor"))
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Failed to query chain height: {}", e);
                Err(anyhow!("Chain height query failed: {}", e))
            }
            Err(e) => {
                tracing::error!("ChainActor mailbox error: {}", e);
                Err(anyhow!("Mailbox error: {}", e))
            }
        }
    }

    /// Discover peers for synchronization
    async fn discover_sync_peers_workflow(
        state: Arc<RwLock<SyncActorState>>,
        network_actor: Option<Addr<NetworkActor>>,
    ) -> Result<()> {
        tracing::info!("Discovering sync peers");

        let network_actor = network_actor
            .ok_or_else(|| anyhow!("NetworkActor not set"))?;

        // Request connected peers from NetworkActor
        match network_actor.send(NetworkMessage::GetConnectedPeers).await {
            Ok(Ok(response)) => {
                if let NetworkResponse::Peers(peers) = response {
                    let peer_ids: Vec<PeerId> =
                        peers.into_iter().map(|p| p.peer_id).collect();

                    tracing::info!("Found {} sync peers", peer_ids.len());

                    if peer_ids.is_empty() {
                        let mut s = state.write().await;
                        s.sync_state =
                            SyncState::Error("No peers available for sync".to_string());
                        return Err(anyhow!("No peers available for sync"));
                    }

                    // Update state with discovered peers
                    {
                        let mut s = state.write().await;
                        s.sync_peers = peer_ids;
                        s.peer_selection_index = 0;
                    }

                    Ok(())
                } else {
                    Err(anyhow!("Unexpected response from NetworkActor"))
                }
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to get peers from network: {:?}", e);
                let mut s = state.write().await;
                s.sync_state = SyncState::Error(error_msg.clone());
                Err(anyhow!(error_msg))
            }
            Err(e) => {
                let error_msg = format!("Network actor communication error: {}", e);
                let mut s = state.write().await;
                s.sync_state = SyncState::Error(error_msg.clone());
                Err(anyhow!(error_msg))
            }
        }
    }

    /// Start requesting blocks from peers
    async fn start_block_requests_workflow(
        state: Arc<RwLock<SyncActorState>>,
        network_actor: Option<Addr<NetworkActor>>,
        chain_actor: Option<Addr<ChainActor>>,
    ) -> Result<()> {
        // Check if peers available
        let has_peers = {
            let s = state.read().await;
            !s.sync_peers.is_empty()
        };

        if !has_peers {
            return Err(anyhow!("No peers available for sync"));
        }

        // Discover target height from network consensus
        Self::discover_target_height_workflow(
            state.clone(),
            network_actor.clone(),
            chain_actor.clone()
        ).await?;

        // Check if already synced
        let (current, target) = {
            let s = state.read().await;
            (s.current_height, s.target_height)
        };

        if current >= target.saturating_sub(2) {
            // Already synced (within 2 blocks tolerance)
            tracing::info!("Already synced at height {}", current);
            let mut s = state.write().await;
            s.transition_to_state(SyncState::Synced);
            return Ok(());
        }

        tracing::info!(
            "Starting block sync from height {} to {} (gap: {} blocks)",
            current,
            target,
            target - current
        );

        // Transition to requesting blocks
        {
            let mut s = state.write().await;
            s.transition_to_state(SyncState::RequestingBlocks);
        }

        // Start requesting blocks in batches
        Self::request_next_batch_workflow(
            state,
            network_actor
        ).await?;

        Ok(())
    }
}
```

---

#### Task 3.2: Refactor Block Processing Workflow

```rust
impl SyncActor {
    /// Process queued blocks asynchronously
    async fn process_block_queue_workflow(
        state: Arc<RwLock<SyncActorState>>,
        chain_actor: Option<Addr<ChainActor>>,
    ) -> Result<()> {
        const PARALLEL_THRESHOLD: usize = 20;

        // Transition to processing state
        {
            let mut s = state.write().await;

            // Don't process if already processing
            if s.sync_state == SyncState::ProcessingBlocks {
                return Ok(());
            }

            s.transition_to_state(SyncState::ProcessingBlocks);
        }

        // Determine queue size
        let queue_size = {
            let s = state.read().await;
            s.block_queue.len()
        };

        if queue_size == 0 {
            tracing::trace!("Block queue empty, nothing to process");
            let mut s = state.write().await;
            s.transition_to_state(SyncState::RequestingBlocks);
            return Ok(());
        }

        tracing::debug!(
            queue_size = queue_size,
            "Processing block queue"
        );

        if queue_size >= PARALLEL_THRESHOLD {
            // Parallel processing for large queues
            Self::process_blocks_parallel_workflow(
                state.clone(),
                chain_actor
            ).await?;
        } else {
            // Sequential processing for small queues
            Self::process_blocks_sequential_workflow(
                state.clone(),
                chain_actor
            ).await?;
        }

        // Check if sync complete
        let (current, target, should_continue) = {
            let s = state.read().await;
            (
                s.current_height,
                s.target_height,
                s.current_height < s.target_height
            )
        };

        if should_continue {
            let mut s = state.write().await;
            s.transition_to_state(SyncState::RequestingBlocks);

            tracing::debug!(
                "Block processing complete, {} blocks remaining",
                target - current
            );
        } else {
            let mut s = state.write().await;
            s.transition_to_state(SyncState::Synced);
            s.is_running = false;

            tracing::info!("Blockchain synchronization complete at height {}", current);
        }

        Ok(())
    }

    /// Process blocks sequentially
    async fn process_blocks_sequential_workflow(
        state: Arc<RwLock<SyncActorState>>,
        chain_actor: Option<Addr<ChainActor>>,
    ) -> Result<()> {
        let chain_actor = chain_actor
            .ok_or_else(|| anyhow!("ChainActor not set"))?;

        loop {
            // Pop next block from queue
            let block_opt = {
                let mut s = state.write().await;
                s.block_queue.pop_front()
            };

            let Some((block, peer_id)) = block_opt else {
                break; // Queue exhausted
            };

            // Process block
            match Self::process_single_block(
                state.clone(),
                chain_actor.clone(),
                block,
                peer_id.clone()
            ).await {
                Ok(height) => {
                    // Update current height
                    let mut s = state.write().await;
                    s.current_height = height;
                    s.metrics.record_block_processed();

                    tracing::debug!(
                        height = height,
                        peer_id = %peer_id,
                        "Block processed successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        peer_id = %peer_id,
                        error = %e,
                        "Failed to process block"
                    );

                    let mut s = state.write().await;
                    s.metrics.record_block_rejected();
                }
            }
        }

        Ok(())
    }

    /// Process single block by forwarding to ChainActor
    async fn process_single_block(
        state: Arc<RwLock<SyncActorState>>,
        chain_actor: Addr<ChainActor>,
        block: Block,
        peer_id: PeerId,
    ) -> Result<u64> {
        use crate::actors_v2::chain::messages::{ChainMessage, ChainResponse};

        // Send block to ChainActor for validation and import
        let msg = ChainMessage::ImportBlock {
            block_data: block,
            peer_id: peer_id.clone(),
        };

        match chain_actor.send(msg).await {
            Ok(Ok(ChainResponse::BlockImported { height })) => {
                Ok(height)
            }
            Ok(Ok(response)) => {
                Err(anyhow!("Unexpected response from ChainActor: {:?}", response))
            }
            Ok(Err(e)) => {
                Err(anyhow!("Block import failed: {}", e))
            }
            Err(e) => {
                Err(anyhow!("ChainActor mailbox error: {}", e))
            }
        }
    }
}
```

---

#### Task 3.3: Refactor Checkpoint Workflows

```rust
impl SyncActor {
    /// Load checkpoint on startup
    async fn load_checkpoint_workflow(
        state: Arc<RwLock<SyncActorState>>,
        data_dir: std::path::PathBuf,
    ) -> Result<()> {
        match SyncCheckpoint::load(&data_dir).await {
            Ok(Some(checkpoint)) => {
                // Check if checkpoint is stale (older than 24 hours)
                let stale_threshold = Duration::from_secs(24 * 60 * 60);
                if checkpoint.is_stale(stale_threshold) {
                    tracing::warn!(
                        age_secs = checkpoint
                            .last_checkpoint_time
                            .elapsed()
                            .unwrap_or(Duration::ZERO)
                            .as_secs(),
                        "Checkpoint is stale, ignoring"
                    );

                    // Delete stale checkpoint
                    SyncCheckpoint::delete(&data_dir).await?;
                    return Ok(());
                }

                // Restore state from checkpoint
                {
                    let mut s = state.write().await;
                    s.current_height = checkpoint.current_height;
                    s.target_height = checkpoint.target_height;

                    tracing::info!(
                        current_height = checkpoint.current_height,
                        target_height = checkpoint.target_height,
                        progress = checkpoint.blocks_synced,
                        "Restored sync from checkpoint"
                    );
                }

                Ok(())
            }
            Ok(None) => {
                tracing::debug!("No checkpoint found, starting fresh sync");
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to load checkpoint: {}", e);
                Err(e)
            }
        }
    }

    /// Save checkpoint during sync
    async fn save_checkpoint_workflow(
        state: Arc<RwLock<SyncActorState>>,
        data_dir: std::path::PathBuf,
    ) -> Result<()> {
        // Read current state
        let (current_height, target_height, sync_state, blocks_synced) = {
            let s = state.read().await;
            (
                s.current_height,
                s.target_height,
                s.sync_state.clone(),
                s.metrics.blocks_synced,
            )
        };

        // Only save if actively syncing
        if !matches!(
            sync_state,
            SyncState::RequestingBlocks | SyncState::ProcessingBlocks
        ) {
            return Ok(());
        }

        let checkpoint = SyncCheckpoint::new(
            current_height,
            target_height,
            blocks_synced,
        );

        checkpoint.save(&data_dir).await?;

        tracing::trace!(
            current_height = current_height,
            target_height = target_height,
            "Checkpoint saved"
        );

        Ok(())
    }
}
```

---

### Phase 4: Testing & Validation (1-2 hours)

#### Task 4.1: Update Unit Tests

**File**: `app/src/actors_v2/testing/network/unit/sync_tests.rs`

```rust
#[cfg(test)]
mod refactored_sync_tests {
    use super::*;

    #[actix::test]
    async fn test_start_sync_executes_workflow() {
        // Setup
        let config = SyncConfig {
            max_blocks_per_request: 100,
            sync_timeout: Duration::from_secs(30),
            max_concurrent_requests: 5,
            block_validation_timeout: Duration::from_secs(10),
            max_sync_peers: 10,
            data_dir: std::path::PathBuf::from("/tmp/test"),
        };

        let sync_actor = SyncActor::new(config).unwrap().start();

        // Send StartSync message
        let result = sync_actor
            .send(SyncMessage::StartSync {
                start_height: 0,
                target_height: Some(100),
            })
            .await;

        assert!(result.is_ok());

        // Wait for workflow to execute
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify state changed (workflow executed)
        let status = sync_actor
            .send(SyncMessage::GetSyncStatus)
            .await
            .unwrap()
            .unwrap();

        if let SyncResponse::Status(status) = status {
            assert!(status.is_syncing);
            assert_eq!(status.current_height, 0);
            assert_eq!(status.target_height, 100);
        } else {
            panic!("Expected Status response");
        }
    }

    #[actix::test]
    async fn test_block_queue_processes_automatically() {
        // Setup actor with mock ChainActor
        // Send HandleBlockResponse with blocks
        // Verify blocks are processed (not just queued)
        // Verify current_height increases
    }

    #[actix::test]
    async fn test_checkpoint_loading_updates_state() {
        // Create checkpoint file
        // Start actor
        // Verify state restored from checkpoint
    }
}
```

**Validation**:
- [ ] All existing tests pass
- [ ] New tests verify workflow execution
- [ ] No race conditions detected

---

#### Task 4.2: Integration Testing

**File**: `app/src/actors_v2/testing/integration/sync_coordination_tests.rs`

```rust
#[actix::test]
async fn test_full_sync_workflow() {
    // Setup: Start NetworkActor, ChainActor, StorageActor
    // Start SyncActor with actor addresses set
    // Trigger StartSync
    // Verify:
    //   - Peer discovery executes
    //   - Block requests are sent
    //   - Blocks are processed
    //   - Current height increases
    //   - Sync completes at target height
}

#[actix::test]
async fn test_fresh_node_synchronization() {
    // Simulate fresh node (height = 0)
    // Network has blocks 1-1000
    // Start sync
    // Verify node reaches height 1000
}

#[actix::test]
async fn test_checkpoint_resume() {
    // Start sync, let it progress to height 500
    // Save checkpoint
    // Stop actor
    // Restart actor
    // Verify sync resumes from height 500
}
```

---

#### Task 4.3: Manual Testing Protocol

**Test Case 1: Fresh Node Sync**
```bash
# 1. Clean state
rm -rf /tmp/alys-test-fresh

# 2. Start node with SyncActor
cargo run --bin alys-node -- --data-dir /tmp/alys-test-fresh

# 3. Verify logs show:
#    - "Executing start_sync workflow"
#    - "Discovering sync peers"
#    - "Starting block requests"
#    - "Block processed successfully" (repeated)
#    - "Blockchain synchronization complete"

# 4. Check final height matches network
```

**Test Case 2: Block Queue Processing**
```bash
# 1. Start node mid-sync
# 2. Monitor logs for "Processing block queue"
# 3. Verify blocks are not accumulating in queue
# 4. Verify current_height increases steadily
```

**Test Case 3: Checkpoint Resume**
```bash
# 1. Start sync, wait for checkpoint save
# 2. Kill process (SIGTERM)
# 3. Restart node
# 4. Verify "Restored sync from checkpoint" log
# 5. Verify sync continues from saved height
```

---

## Risk Analysis & Mitigation

### Risk 1: Race Conditions in State Access
**Likelihood**: Medium
**Impact**: High (data corruption)

**Mitigation**:
- Use `RwLock` correctly (minimize lock duration)
- Never hold lock across `.await` points
- Use `drop(lock)` explicitly before async calls
- Add logging for lock acquisition/release in debug mode

**Verification**:
```rust
// Good pattern:
let data = {
    let s = state.write().await;
    let data = s.field.clone();
    drop(s); // Explicit release
    data
};
process_data(data).await;

// Bad pattern (NEVER do this):
let mut s = state.write().await;
process_data(s.field).await; // ← Lock held across await!
```

---

### Risk 2: Deadlocks
**Likelihood**: Low
**Impact**: High (node freeze)

**Mitigation**:
- Consistent lock ordering (always acquire in same order)
- Use timeouts for lock acquisition
- Monitor lock contention with metrics

**Detection**:
```rust
// Add timeout to detect deadlocks
use tokio::time::timeout;

let state = timeout(
    Duration::from_secs(5),
    state_arc.write()
).await.expect("Lock acquisition timeout - possible deadlock");
```

---

### Risk 3: Performance Degradation
**Likelihood**: Low
**Impact**: Medium (slower sync)

**Mitigation**:
- Profile lock contention with `tracing`
- Use `read()` instead of `write()` when possible
- Batch state updates to reduce lock acquisition
- Consider lock-free alternatives for hot paths (atomic counters)

**Monitoring**:
```rust
let start = std::time::Instant::now();
let s = state.write().await;
let acquire_time = start.elapsed();

if acquire_time > Duration::from_millis(100) {
    tracing::warn!(
        "Slow lock acquisition: {:?}",
        acquire_time
    );
}
```

---

### Risk 4: Message Handler Blocking
**Likelihood**: Medium
**Impact**: Medium (unresponsive actor)

**Mitigation**:
- Minimize work in handlers (offload to workflows)
- Use `block_in_place` only for read-only access
- Return immediately after spawning workflow
- Document handler response time expectations

**Pattern**:
```rust
// Handler should complete in <1ms
fn handle(&mut self, msg: SyncMessage, ctx: &mut Context<Self>) -> Self::Result {
    let start = std::time::Instant::now();

    // Quick validation + spawn workflow
    ctx.spawn(/* async workflow */);

    let elapsed = start.elapsed();
    if elapsed > Duration::from_millis(1) {
        tracing::warn!("Slow handler: {:?}", elapsed);
    }

    Ok(response)
}
```

---

## Migration Strategy

### Backward Compatibility

**Message Interface**: No changes required
- `SyncMessage` enum unchanged
- `SyncResponse` enum unchanged
- External callers unaffected

**Actor Interface**: No changes required
- Actor address type unchanged
- `send()` calls work identically
- Supervision unchanged

**State Visibility**: Internal change only
- State structure refactored but behavior identical
- External observers see same responses
- Metrics continue working

---

## Performance Expectations

### Before (Current Broken State)
- StartSync response: <1ms (but workflow never executes)
- Block processing: 0 blocks/sec (queue never processed)
- Checkpoint loading: Fails silently
- Fresh node sync: Never completes

### After (Arc Refactor)
- StartSync response: <1ms (handler returns immediately)
- Workflow execution: 10-50ms (async execution)
- Block processing: 50-200 blocks/sec (depends on validation)
- Checkpoint loading: 5-20ms (full state restoration)
- Fresh node sync: Completes successfully

### Lock Contention Targets
- Lock acquisition: <10ms p99
- Lock hold duration: <5ms average
- Concurrent readers: Unlimited (RwLock benefit)
- Write lock frequency: <10/sec

---

## Success Criteria

### Functional Requirements (Must Pass)
- [ ] Fresh node synchronizes from genesis to network height
- [ ] StartSync handler triggers full workflow execution
- [ ] Block queue processes automatically when blocks arrive
- [ ] Checkpoint loading restores state correctly
- [ ] Sync resumes from checkpoint after restart
- [ ] All existing unit tests pass
- [ ] Integration tests verify end-to-end sync

### Performance Requirements (Should Meet)
- [ ] Handler response time <1ms p99
- [ ] Block processing rate >50 blocks/sec
- [ ] Lock acquisition time <10ms p99
- [ ] Memory overhead <10MB for state management

### Code Quality Requirements (Must Maintain)
- [ ] No new compiler warnings
- [ ] Tracing coverage for all workflows
- [ ] Error handling for all async operations
- [ ] Documentation updated for Arc pattern
- [ ] Code review approval from team

---

## Implementation Timeline

### Hour 1-2: State Refactoring
- Create `SyncActorState` struct
- Move mutable fields to state
- Wrap in `Arc<RwLock<T>>`
- Update `SyncActor::new()`
- Verify compilation

### Hour 3-4: Handler Refactoring
- Refactor StartSync handler
- Refactor StopSync handler
- Refactor block handling handlers
- Refactor checkpoint handlers
- Test handler response times

### Hour 5-6: Workflow Refactoring
- Refactor start_sync workflow
- Refactor block processing workflow
- Refactor checkpoint workflows
- Add workflow error handling
- Test workflow execution

### Hour 7-8: Testing & Validation
- Run unit tests
- Run integration tests
- Manual testing protocol
- Performance benchmarking
- Documentation updates

---

## Post-Implementation Tasks

### Monitoring
- Add metrics for lock contention
- Track workflow execution times
- Monitor block processing rate
- Alert on deadlock detection

### Documentation
- Update architecture docs
- Add Arc pattern guide
- Document workflow execution model
- Create troubleshooting guide

### Future Optimizations
- Consider lock-free queue for blocks
- Evaluate `parking_lot::RwLock` for performance
- Profile hot paths for optimization
- Add adaptive batch sizing

---

## Conclusion

The Arc<RwLock<T>> refactor solves the fundamental disconnect between synchronous handlers and asynchronous workflows. This enables:

1. ✅ **Fresh nodes synchronize** - StartSync triggers full workflow
2. ✅ **Blocks process automatically** - Queue processing executes on arrival
3. ✅ **Checkpoint loading works** - State restoration integrates properly
4. ✅ **Production-ready architecture** - Pattern proven in distributed systems

**Estimated effort**: 6-8 hours
**Risk level**: Medium (architectural change)
**Success probability**: High (clear pattern, proven approach)

**Next Steps**: Begin Phase 1 (State Refactoring) immediately.
