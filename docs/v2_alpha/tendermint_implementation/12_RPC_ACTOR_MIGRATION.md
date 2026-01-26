# Implementation Plan: RPC Actor Migration for Tendermint

## Overview

This document provides a comprehensive implementation guide for migrating the RpcActor from per-block mining pool RPC (`createauxblock`/`submitauxblock`) to checkpoint-based AuxPoW RPC for the security and checkpointing layer. Additionally, new validator-focused RPC endpoints are added for Tendermint consensus monitoring.

**Estimated Effort**: 2-3 days
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (Commit type)
- Section 4 of TENDERMINT_MIGRATION_ASSESSMENT.md (AuxPoW as checkpointing layer)
**Files to Modify**:
- `app/src/actors_v2/rpc/actor.rs`
- `app/src/actors_v2/rpc/handlers.rs`
- `app/src/actors_v2/rpc/messages.rs`

---

## 1. Conceptual Change

### 1.1 Current RPC (Per-Block Mining)

```
CURRENT FLOW:

  Mining Pool                        Alys Node
      │                                  │
      │──── createauxblock ─────────────►│
      │◄─── block_hash, target ──────────│
      │                                  │
      │  [Bitcoin mining finds solution] │
      │                                  │
      │──── submitauxblock ─────────────►│
      │     (hash, auxpow)               │
      │◄─── success/failure ─────────────│
      │                                  │
      │  [Block finalized with AuxPoW]   │
```

**Key Issue**: With Tendermint, blocks are finalized via consensus, not AuxPoW. Per-block mining is no longer the finalization mechanism.

### 1.2 New RPC (Checkpoint Mining)

```
TENDERMINT FLOW:

  Mining Pool                        Alys Node
      │                                  │
      │                    [Tendermint commits blocks 1-500]
      │                                  │
      │──── createauxblock ─────────────►│
      │◄─── checkpoint_hash, target ─────│  (covers blocks 1-500)
      │                                  │
      │  [Bitcoin mining finds solution] │
      │                                  │
      │──── submitauxblock ─────────────►│
      │     (hash, auxpow)               │
      │◄─── success ─────────────────────│
      │                                  │
      │  [Checkpoint anchored to Bitcoin]│
```

**Key Difference**: AuxPoW now creates checkpoints that anchor ranges of already-finalized blocks to Bitcoin for additional security.

---

## 2. RPC Method Changes

### 2.1 Method Comparison

| Method | Current Behavior | New Behavior |
|--------|------------------|--------------|
| `createauxblock` | Returns next block template | Returns checkpoint template (block range) |
| `submitauxblock` | Finalizes single block | Anchors checkpoint to Bitcoin |
| (new) `getcheckpointstatus` | N/A | Returns current checkpoint status |
| (new) `getvalidatorstatus` | N/A | Returns Tendermint validator info |
| (new) `getconsensusstate` | N/A | Returns current height/round/step |

### 2.2 API Compatibility

The external RPC interface (`createauxblock`/`submitauxblock`) remains compatible with existing mining pools. The internal behavior changes, but mining pools don't need modification.

---

## 3. Updated `createauxblock` Implementation

### 3.1 Current Implementation

```rust
// CURRENT: Creates single block template
pub async fn handle_createauxblock(
    params: Vec<Value>,
    chain_actor: Addr<ChainActor>,
) -> Result<Value, RpcError> {
    // Get next block to mine
    let block = chain_actor.send(ChainMessage::GetBlockTemplate).await??;

    Ok(json!({
        "hash": block.hash().to_string(),
        "chainid": CHAIN_ID,
        "target": calculate_target(&block),
    }))
}
```

### 3.2 New Implementation (Checkpoint-Based)

```rust
// NEW: Creates checkpoint template covering block range

/// Checkpoint work template for mining pools
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointTemplate {
    /// Hash to mine against (commitment of block range)
    pub hash: String,

    /// Chain ID for merge-mining
    pub chainid: u32,

    /// Mining target (Bitcoin difficulty)
    pub target: String,

    /// First block in checkpoint range
    pub range_start: u64,

    /// Last block in checkpoint range
    pub range_end: u64,

    /// Number of blocks in checkpoint
    pub block_count: u64,
}

impl CreateAuxBlockHandler {
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        // 1. Get checkpoint template from ChainActor
        let template = chain_actor
            .send(ChainMessage::GetCheckpointTemplate)
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        match template {
            ChainResponse::CheckpointTemplate {
                commitment,
                range_start,
                range_end,
                target,
            } => {
                let block_count = range_end - range_start + 1;

                tracing::info!(
                    range_start = range_start,
                    range_end = range_end,
                    block_count = block_count,
                    "Created checkpoint template for mining"
                );

                Ok(json!({
                    "hash": hex::encode(commitment),
                    "chainid": CHAIN_ID,
                    "target": format!("{:064x}", target),
                    "range_start": range_start,
                    "range_end": range_end,
                    "block_count": block_count,
                }))
            }
            ChainResponse::NoCheckpointNeeded { reason } => {
                // Not enough blocks accumulated for checkpoint
                Err(RpcError::NoWorkAvailable(reason))
            }
            _ => Err(RpcError::Internal("Unexpected response".to_string())),
        }
    }
}
```

### 3.3 ChainActor Handler for Checkpoint Template

```rust
// In chain/handlers.rs

ChainMessage::GetCheckpointTemplate => {
    let storage = self.storage_actor.as_ref()
        .ok_or(ChainError::StorageActorNotSet)?;

    // 1. Get last checkpoint
    let last_checkpoint = storage.send(GetLatestCheckpointMessage {
        correlation_id: None,
    }).await??;

    let range_start = match &last_checkpoint {
        Some(cp) => cp.range_end_height + 1,
        None => 1,  // Start from genesis
    };

    // 2. Get current finalized height
    let head = storage.send(GetChainHeadMessage {
        correlation_id: None,
    }).await??.ok_or(ChainError::NoChainHead)?;

    let range_end = head.number;

    // 3. Check if enough blocks for checkpoint
    let block_count = range_end.saturating_sub(range_start) + 1;
    let min_checkpoint_interval = self.config.checkpoint_config.min_checkpoint_interval;

    if block_count < min_checkpoint_interval {
        return Ok(ChainResponse::NoCheckpointNeeded {
            reason: format!(
                "Only {} blocks since last checkpoint, need {}",
                block_count, min_checkpoint_interval
            ),
        });
    }

    // 4. Compute commitment (merkle root of block range)
    let commitment = self.compute_checkpoint_commitment(range_start, range_end).await?;

    // 5. Calculate target from configuration
    let target = self.config.checkpoint_config.min_checkpoint_difficulty;

    Ok(ChainResponse::CheckpointTemplate {
        commitment,
        range_start,
        range_end,
        target,
    })
}
```

---

## 4. Updated `submitauxblock` Implementation

### 4.1 Current Implementation

```rust
// CURRENT: Submits AuxPoW for single block
pub async fn handle_submitauxblock(
    params: Vec<Value>,
    chain_actor: Addr<ChainActor>,
) -> Result<Value, RpcError> {
    let block_hash = parse_hash(&params[0])?;
    let auxpow = parse_auxpow(&params[1])?;

    chain_actor.send(ChainMessage::SubmitAuxPow {
        block_hash,
        auxpow,
    }).await??;

    Ok(json!(true))
}
```

### 4.2 New Implementation (Checkpoint Submission)

```rust
// NEW: Submits AuxPoW for checkpoint

impl SubmitAuxBlockHandler {
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        // Parse parameters (same format as before)
        let commitment_hash = parse_hash(&params.get(0)
            .ok_or(RpcError::InvalidParams("Missing hash".to_string()))?)?;

        let auxpow_hex = params.get(1)
            .and_then(|v| v.as_str())
            .ok_or(RpcError::InvalidParams("Missing auxpow".to_string()))?;

        let auxpow = AuxPow::from_hex(auxpow_hex)
            .map_err(|e| RpcError::InvalidParams(format!("Invalid auxpow: {}", e)))?;

        // Submit checkpoint to ChainActor
        let result = chain_actor
            .send(ChainMessage::SubmitCheckpoint {
                commitment: commitment_hash,
                auxpow,
            })
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        match result {
            Ok(ChainResponse::CheckpointAccepted {
                range_start,
                range_end,
            }) => {
                tracing::info!(
                    range_start = range_start,
                    range_end = range_end,
                    "Checkpoint accepted and anchored to Bitcoin"
                );

                // Return success with checkpoint info
                Ok(json!({
                    "accepted": true,
                    "range_start": range_start,
                    "range_end": range_end,
                }))
            }
            Ok(ChainResponse::CheckpointRejected { reason }) => {
                tracing::warn!(reason = %reason, "Checkpoint rejected");
                Err(RpcError::CheckpointRejected(reason))
            }
            Err(e) => Err(RpcError::Internal(e.to_string())),
            _ => Err(RpcError::Internal("Unexpected response".to_string())),
        }
    }
}
```

### 4.3 ChainActor Handler for Checkpoint Submission

```rust
// In chain/handlers.rs

ChainMessage::SubmitCheckpoint { commitment, auxpow } => {
    // 1. Verify we have a pending checkpoint with this commitment
    let pending = self.state.pending_checkpoint.as_ref()
        .ok_or(ChainError::NoPendingCheckpoint)?;

    if pending.commitment != commitment {
        return Ok(ChainResponse::CheckpointRejected {
            reason: "Commitment doesn't match pending checkpoint".to_string(),
        });
    }

    // 2. Validate AuxPoW against commitment
    if !auxpow.verify_against_hash(&commitment) {
        return Ok(ChainResponse::CheckpointRejected {
            reason: "Invalid AuxPoW proof".to_string(),
        });
    }

    // 3. Verify AuxPoW meets minimum difficulty
    let pow_difficulty = auxpow.difficulty();
    if pow_difficulty < self.config.checkpoint_config.min_checkpoint_difficulty {
        return Ok(ChainResponse::CheckpointRejected {
            reason: format!(
                "Insufficient difficulty: {} < {}",
                pow_difficulty, self.config.checkpoint_config.min_checkpoint_difficulty
            ),
        });
    }

    // 4. Create and store checkpoint
    let checkpoint = AuxPowCheckpoint {
        range_start: pending.range_start,
        range_start_height: pending.range_start,
        range_end: pending.range_end_hash,
        range_end_height: pending.range_end,
        commitment,
        auxpow,
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
    };

    let storage = self.storage_actor.as_ref()
        .ok_or(ChainError::StorageActorNotSet)?;

    storage.send(StoreCheckpointMessage {
        checkpoint: checkpoint.clone(),
        correlation_id: None,
    }).await
        .map_err(|e| ChainError::ActorMailbox(e.to_string()))?
        .map_err(|e| ChainError::Storage(e.to_string()))?;

    // 5. Clear pending checkpoint
    let range_start = pending.range_start;
    let range_end = pending.range_end;
    self.state.pending_checkpoint = None;

    // 6. Emit metrics
    CHECKPOINT_ACCEPTED.inc();
    CHECKPOINT_LATEST_HEIGHT.set(range_end as i64);

    tracing::info!(
        range_start = range_start,
        range_end = range_end,
        difficulty = pow_difficulty,
        "Checkpoint accepted"
    );

    Ok(ChainResponse::CheckpointAccepted { range_start, range_end })
}
```

---

## 5. New RPC Methods

### 5.1 `getcheckpointstatus`

```rust
/// Get current checkpoint status
impl GetCheckpointStatusHandler {
    pub async fn handle(
        _params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let status = chain_actor
            .send(ChainMessage::GetCheckpointStatus)
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        match status {
            ChainResponse::CheckpointStatus {
                latest_checkpoint,
                pending_checkpoint,
                blocks_since_checkpoint,
                next_checkpoint_eta,
            } => {
                Ok(json!({
                    "latest_checkpoint": latest_checkpoint.map(|cp| json!({
                        "range_start": cp.range_start_height,
                        "range_end": cp.range_end_height,
                        "commitment": hex::encode(cp.commitment),
                        "timestamp": cp.timestamp,
                    })),
                    "pending_checkpoint": pending_checkpoint.map(|pc| json!({
                        "range_start": pc.range_start,
                        "range_end": pc.range_end,
                        "commitment": hex::encode(pc.commitment),
                    })),
                    "blocks_since_checkpoint": blocks_since_checkpoint,
                    "next_checkpoint_eta_blocks": next_checkpoint_eta,
                }))
            }
            _ => Err(RpcError::Internal("Unexpected response".to_string())),
        }
    }
}
```

### 5.2 `getvalidatorstatus`

```rust
/// Get Tendermint validator status
impl GetValidatorStatusHandler {
    pub async fn handle(
        _params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let status = chain_actor
            .send(ChainMessage::GetValidatorStatus)
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        match status {
            ChainResponse::ValidatorStatus {
                is_validator,
                validator_id,
                voting_power,
                validator_set_size,
                is_proposer_this_height,
            } => {
                Ok(json!({
                    "is_validator": is_validator,
                    "validator_id": validator_id.map(|id| id.0),
                    "voting_power": voting_power,
                    "validator_set_size": validator_set_size,
                    "is_proposer_this_height": is_proposer_this_height,
                }))
            }
            _ => Err(RpcError::Internal("Unexpected response".to_string())),
        }
    }
}
```

### 5.3 `getconsensusstate`

```rust
/// Get current Tendermint consensus state
impl GetConsensusStateHandler {
    pub async fn handle(
        _params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let state = chain_actor
            .send(ChainMessage::GetConsensusState)
            .await
            .map_err(|e| RpcError::Internal(e.to_string()))?
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        match state {
            ChainResponse::ConsensusState {
                height,
                round,
                step,
                locked_round,
                locked_block,
                prevote_count,
                precommit_count,
                proposal_received,
            } => {
                Ok(json!({
                    "height": height,
                    "round": round,
                    "step": step.to_string(),
                    "locked_round": locked_round,
                    "locked_block": locked_block.map(|h| hex::encode(h)),
                    "prevotes": prevote_count,
                    "precommits": precommit_count,
                    "proposal_received": proposal_received,
                }))
            }
            _ => Err(RpcError::Internal("Unexpected response".to_string())),
        }
    }
}
```

---

## 6. Updated Route Handler

```rust
// In rpc/actor.rs

impl RpcActor {
    async fn route_request(req: JsonRpcRequest, state: RpcServerState) -> Result<Value, RpcError> {
        match req.method.as_str() {
            // === Checkpoint/Mining RPCs (adapted from AuxPoW) ===
            "createauxblock" => CreateAuxBlockHandler::handle(req.params, state.chain_actor).await,
            "submitauxblock" => SubmitAuxBlockHandler::handle(req.params, state.chain_actor).await,
            "getcheckpointstatus" => GetCheckpointStatusHandler::handle(req.params, state.chain_actor).await,

            // === Tendermint Validator RPCs (new) ===
            "getvalidatorstatus" => GetValidatorStatusHandler::handle(req.params, state.chain_actor).await,
            "getconsensusstate" => GetConsensusStateHandler::handle(req.params, state.chain_actor).await,

            // === Chain Info RPCs (existing, keep) ===
            "getblockcount" => GetBlockCountHandler::handle(req.params, state.chain_actor).await,
            "getblockhash" => GetBlockHashHandler::handle(req.params, state.chain_actor).await,
            "getblock" => GetBlockHandler::handle(req.params, state.chain_actor).await,

            // === Unknown method ===
            _ => Err(RpcError::MethodNotFound(req.method)),
        }
    }
}
```

---

## 7. Error Types

```rust
// In rpc/error.rs

#[derive(Debug, Clone)]
pub enum RpcError {
    // Existing errors
    InvalidRequest(String),
    InvalidParams(String),
    MethodNotFound(String),
    Internal(String),
    ServerNotRunning,

    // New checkpoint-related errors
    NoWorkAvailable(String),
    CheckpointRejected(String),
    InsufficientDifficulty { have: u128, need: u128 },

    // New Tendermint errors
    NotValidator,
    ConsensusNotReady,
}

impl RpcError {
    pub fn to_json_rpc_error(&self) -> JsonRpcError {
        match self {
            RpcError::NoWorkAvailable(msg) => JsonRpcError {
                code: -1,  // Bitcoin-compatible "no work available"
                message: msg.clone(),
            },
            RpcError::CheckpointRejected(msg) => JsonRpcError {
                code: -2,
                message: format!("Checkpoint rejected: {}", msg),
            },
            RpcError::InsufficientDifficulty { have, need } => JsonRpcError {
                code: -3,
                message: format!("Insufficient difficulty: {} < {}", have, need),
            },
            RpcError::NotValidator => JsonRpcError {
                code: -10,
                message: "This node is not a validator".to_string(),
            },
            // ... existing error handling ...
            _ => JsonRpcError {
                code: -32603,
                message: format!("{:?}", self),
            },
        }
    }
}
```

---

## 8. Configuration Changes

```rust
// In rpc/config.rs

#[derive(Debug, Clone)]
pub struct RpcConfig {
    /// Bind address for RPC server
    pub bind_address: SocketAddr,

    /// Allowed RPC methods (empty = all allowed)
    pub allowed_methods: Vec<String>,

    /// Enable checkpoint/mining RPCs
    pub enable_checkpoint_rpc: bool,

    /// Enable validator status RPCs
    pub enable_validator_rpc: bool,

    /// Maximum request body size
    pub max_request_size: usize,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9432".parse().unwrap(),
            allowed_methods: vec![],  // All allowed
            enable_checkpoint_rpc: true,
            enable_validator_rpc: true,
            max_request_size: 1024 * 1024,  // 1MB
        }
    }
}
```

---

## 9. Metrics

```rust
lazy_static! {
    /// Checkpoint templates created
    pub static ref RPC_CHECKPOINT_TEMPLATES: IntCounter = IntCounter::new(
        "rpc_checkpoint_templates_total",
        "Checkpoint templates created via createauxblock"
    ).unwrap();

    /// Checkpoints submitted
    pub static ref RPC_CHECKPOINTS_SUBMITTED: IntCounter = IntCounter::new(
        "rpc_checkpoints_submitted_total",
        "Checkpoints submitted via submitauxblock"
    ).unwrap();

    /// Checkpoint submissions rejected
    pub static ref RPC_CHECKPOINTS_REJECTED: IntCounter = IntCounter::new(
        "rpc_checkpoints_rejected_total",
        "Checkpoint submissions rejected"
    ).unwrap();

    /// "No work available" responses
    pub static ref RPC_NO_WORK_RESPONSES: IntCounter = IntCounter::new(
        "rpc_no_work_responses_total",
        "createauxblock responses with no work available"
    ).unwrap();
}
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_createauxblock_returns_checkpoint_template() {
        let (actor, chain_actor) = setup_test_actors().await;

        // Finalize 100 blocks
        finalize_blocks(&chain_actor, 100).await;

        // Call createauxblock
        let response = actor.send(RpcRequest {
            method: "createauxblock".to_string(),
            params: vec![],
            id: Some(json!(1)),
        }).await.unwrap();

        // Verify response structure
        let result = response.result.unwrap();
        assert!(result.get("hash").is_some());
        assert!(result.get("range_start").is_some());
        assert!(result.get("range_end").is_some());
        assert_eq!(result["range_start"], 1);
        assert_eq!(result["range_end"], 100);
    }

    #[tokio::test]
    async fn test_createauxblock_no_work_available() {
        let (actor, chain_actor) = setup_test_actors().await;

        // Only finalize 10 blocks (below min_checkpoint_interval)
        finalize_blocks(&chain_actor, 10).await;

        // Call createauxblock
        let response = actor.send(RpcRequest {
            method: "createauxblock".to_string(),
            params: vec![],
            id: Some(json!(1)),
        }).await.unwrap();

        // Should return error
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -1);
    }

    #[tokio::test]
    async fn test_submitauxblock_accepts_valid_checkpoint() {
        let (actor, chain_actor) = setup_test_actors().await;

        // Create checkpoint template
        finalize_blocks(&chain_actor, 100).await;
        let template = get_checkpoint_template(&actor).await;

        // Create valid AuxPoW
        let auxpow = create_valid_auxpow(&template.hash);

        // Submit
        let response = actor.send(RpcRequest {
            method: "submitauxblock".to_string(),
            params: vec![json!(template.hash), json!(auxpow.to_hex())],
            id: Some(json!(1)),
        }).await.unwrap();

        // Should succeed
        assert!(response.result.is_some());
        assert_eq!(response.result.unwrap()["accepted"], true);
    }

    #[tokio::test]
    async fn test_getconsensusstate() {
        let (actor, chain_actor) = setup_test_actors().await;

        // Start consensus
        start_tendermint(&chain_actor).await;

        // Query consensus state
        let response = actor.send(RpcRequest {
            method: "getconsensusstate".to_string(),
            params: vec![],
            id: Some(json!(1)),
        }).await.unwrap();

        // Verify structure
        let result = response.result.unwrap();
        assert!(result.get("height").is_some());
        assert!(result.get("round").is_some());
        assert!(result.get("step").is_some());
    }
}
```

### 10.2 Integration Tests

```rust
#[tokio::test]
async fn test_mining_pool_flow_with_checkpoint() {
    // 1. Start Alys node with RPC enabled
    let node = start_test_node_with_rpc().await;

    // 2. Finalize blocks via Tendermint
    for _ in 0..500 {
        node.produce_and_commit_block().await;
    }

    // 3. Simulate mining pool calling createauxblock
    let client = reqwest::Client::new();
    let response: JsonRpcResponse = client
        .post("http://127.0.0.1:9432")
        .json(&json!({
            "method": "createauxblock",
            "params": [],
            "id": 1
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let template = response.result.unwrap();
    assert_eq!(template["range_start"], 1);
    assert_eq!(template["range_end"], 500);

    // 4. Submit checkpoint (simulated mining)
    let auxpow = mine_checkpoint(&template["hash"].as_str().unwrap());

    let submit_response: JsonRpcResponse = client
        .post("http://127.0.0.1:9432")
        .json(&json!({
            "method": "submitauxblock",
            "params": [template["hash"], auxpow.to_hex()],
            "id": 2
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(submit_response.result.unwrap()["accepted"].as_bool().unwrap());

    // 5. Verify checkpoint stored
    let status: JsonRpcResponse = client
        .post("http://127.0.0.1:9432")
        .json(&json!({
            "method": "getcheckpointstatus",
            "params": [],
            "id": 3
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let checkpoint = status.result.unwrap()["latest_checkpoint"].clone();
    assert_eq!(checkpoint["range_end"], 500);
}
```

---

## 11. Checklist

- [ ] Modify `createauxblock` handler for checkpoint templates
- [ ] Modify `submitauxblock` handler for checkpoint submission
- [ ] Add `GetCheckpointTemplate` message to ChainActor
- [ ] Add `SubmitCheckpoint` message to ChainActor
- [ ] Add `GetCheckpointStatus` message and handler
- [ ] Add `getcheckpointstatus` RPC method
- [ ] Add `getvalidatorstatus` RPC method
- [ ] Add `getconsensusstate` RPC method
- [ ] Update error types for checkpoint errors
- [ ] Update RpcConfig with new options
- [ ] Add checkpoint RPC metrics
- [ ] Update route handler with new methods
- [ ] Write unit tests for checkpoint RPCs
- [ ] Write unit tests for validator RPCs
- [ ] Write integration test for mining pool flow

---

*Implementation Plan Version: 1.0*
*Last Updated: January 2026*
