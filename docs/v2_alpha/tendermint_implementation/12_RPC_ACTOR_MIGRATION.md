# Document 12: RPC Actor Migration

## Overview

This document specifies the RPC endpoint changes required for the Tendermint consensus migration. The RPC layer must expose new Tendermint-specific queries while deprecating Aura-specific endpoints.

**Effort Estimate**: 2-3 days
**Dependencies**: 01 (Message Types), 03 (VoteSet), 04 (ChainActor Handlers), 11 (Storage Schema)

## New RPC Endpoints

### Consensus State Queries

#### `tendermint_consensusState`

Returns the current consensus state of the node.

```rust
pub struct ConsensusStateResponse {
    pub height: u64,
    pub round: u32,
    pub step: String,  // "Propose" | "Prevote" | "Precommit" | "Commit"
    pub start_time: DateTime<Utc>,
    pub proposal_block_hash: Option<BlockHash>,
    pub locked_block_hash: Option<BlockHash>,
    pub locked_round: Option<u32>,
    pub valid_block_hash: Option<BlockHash>,
    pub valid_round: Option<u32>,
    pub votes: VotesInfo,
}

pub struct VotesInfo {
    pub prevotes: Vec<VoteInfo>,
    pub precommits: Vec<VoteInfo>,
    pub prevotes_bit_array: String,    // e.g., "BA{4:xx__}"
    pub precommits_bit_array: String,
}

pub struct VoteInfo {
    pub validator_index: u32,
    pub validator_address: String,
    pub block_hash: Option<BlockHash>,  // None = NIL vote
    pub timestamp: DateTime<Utc>,
    pub signature: String,
}
```

#### `tendermint_validators`

Returns the current validator set.

```rust
// Request
pub struct ValidatorsRequest {
    pub height: Option<u64>,  // None = current height
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

// Response
pub struct ValidatorsResponse {
    pub block_height: u64,
    pub validators: Vec<ValidatorInfo>,
    pub count: u32,
    pub total: u32,
}

pub struct ValidatorInfo {
    pub address: String,
    pub pub_key: PubKeyInfo,
    pub voting_power: u64,
    pub proposer_priority: i64,
}

pub struct PubKeyInfo {
    pub type_: String,  // "ed25519" | "secp256k1"
    pub value: String,  // base64-encoded
}
```

#### `tendermint_commit`

Returns the commit (signed block header) for a given height.

```rust
// Request
pub struct CommitRequest {
    pub height: Option<u64>,  // None = latest
}

// Response
pub struct CommitResponse {
    pub signed_header: SignedHeader,
    pub canonical: bool,
}

pub struct SignedHeader {
    pub header: BlockHeader,
    pub commit: CommitInfo,
}

pub struct CommitInfo {
    pub height: u64,
    pub round: u32,
    pub block_id: BlockIdInfo,
    pub signatures: Vec<CommitSigInfo>,
}

pub struct CommitSigInfo {
    pub block_id_flag: String,  // "Commit" | "Nil" | "Absent"
    pub validator_address: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub signature: Option<String>,
}
```

### Block Queries (Modified)

#### `eth_getBlockByHash` / `eth_getBlockByNumber`

Extended response to include Tendermint fields:

```rust
pub struct BlockResponse {
    // Existing fields...
    pub hash: BlockHash,
    pub parent_hash: BlockHash,
    pub number: u64,
    pub timestamp: u64,
    pub transactions: Vec<Transaction>,

    // New Tendermint fields
    pub proposer_address: Option<String>,
    pub last_commit_hash: Option<String>,
    pub validators_hash: Option<String>,
    pub next_validators_hash: Option<String>,
    pub consensus_hash: Option<String>,
    pub evidence_hash: Option<String>,
}
```

### Governance Queries

#### `tendermint_params`

Returns current chain parameters.

```rust
pub struct ParamsResponse {
    pub block_height: u64,
    pub consensus_params: ConsensusParams,
    pub governance_params: GovernanceParams,
}

pub struct ConsensusParams {
    pub block: BlockParams,
    pub evidence: EvidenceParams,
    pub validator: ValidatorParams,
}

pub struct BlockParams {
    pub max_bytes: u64,
    pub max_gas: i64,
}

pub struct EvidenceParams {
    pub max_age_num_blocks: u64,
    pub max_age_duration_ms: u64,
    pub max_bytes: u64,
}

pub struct ValidatorParams {
    pub pub_key_types: Vec<String>,
}

pub struct GovernanceParams {
    pub pegin_minimum_satoshis: u64,
    pub pegin_confirmation_depth: u32,
    pub bridge_fee_rate_bps: u32,
    pub emergency_pause_enabled: bool,
}
```

#### `tendermint_pendingGovernanceUpdates`

Returns governance updates scheduled for future activation.

```rust
pub struct PendingUpdatesResponse {
    pub updates: Vec<PendingUpdate>,
}

pub struct PendingUpdate {
    pub update_type: String,  // "Validator" | "Parameter" | "Emergency"
    pub activation_height: u64,
    pub details: serde_json::Value,
    pub proposed_at_height: u64,
    pub proposed_by: String,
}
```

## Deprecated Endpoints

The following Aura-specific endpoints will be deprecated:

| Endpoint | Replacement | Notes |
|----------|-------------|-------|
| `aura_currentAuthorities` | `tendermint_validators` | Validators replace authorities |
| `aura_nextSlotTime` | N/A | Event-driven, no slots |
| `aura_slotDuration` | N/A | No fixed slot duration |
| `aura_currentSlot` | `tendermint_consensusState` | Use height/round instead |
| `engine_getPayloadV1` | N/A | Integrated into consensus |
| `engine_forkchoiceUpdatedV1` | N/A | No fork choice needed |

### Deprecation Strategy

1. **Phase 1 (Initial)**: All deprecated endpoints return warnings but still function
2. **Phase 2 (After 1 month)**: Deprecated endpoints return errors with migration guidance
3. **Phase 3 (After 3 months)**: Deprecated endpoints removed entirely

## Implementation

### RpcActor Message Changes

```rust
// New messages for RpcActor
pub enum RpcMessage {
    // Existing...
    GetBlockByHash { hash: BlockHash, reply: oneshot::Sender<...> },
    GetBlockByNumber { number: u64, reply: oneshot::Sender<...> },

    // New Tendermint queries
    GetConsensusState { reply: oneshot::Sender<ConsensusStateResponse> },
    GetValidators {
        height: Option<u64>,
        page: Option<u32>,
        per_page: Option<u32>,
        reply: oneshot::Sender<ValidatorsResponse>
    },
    GetCommit {
        height: Option<u64>,
        reply: oneshot::Sender<CommitResponse>
    },
    GetParams { reply: oneshot::Sender<ParamsResponse> },
    GetPendingGovernanceUpdates { reply: oneshot::Sender<PendingUpdatesResponse> },
}
```

### Handler Routing

```rust
impl RpcActor {
    async fn handle_rpc_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            // New Tendermint methods
            "tendermint_consensusState" => self.handle_consensus_state().await,
            "tendermint_validators" => self.handle_validators(request.params).await,
            "tendermint_commit" => self.handle_commit(request.params).await,
            "tendermint_params" => self.handle_params().await,
            "tendermint_pendingGovernanceUpdates" => self.handle_pending_updates().await,

            // Deprecated Aura methods
            "aura_currentAuthorities" => self.handle_deprecated("tendermint_validators"),
            "aura_currentSlot" => self.handle_deprecated("tendermint_consensusState"),
            "aura_nextSlotTime" => self.handle_deprecated_no_replacement(),
            "aura_slotDuration" => self.handle_deprecated_no_replacement(),

            // Existing methods (unchanged)
            "eth_getBlockByHash" => self.handle_get_block_by_hash(request.params).await,
            "eth_getBlockByNumber" => self.handle_get_block_by_number(request.params).await,
            // ...

            _ => JsonRpcResponse::error(-32601, "Method not found"),
        }
    }

    fn handle_deprecated(&self, replacement: &str) -> JsonRpcResponse {
        JsonRpcResponse::error(
            -32000,
            &format!("Method deprecated. Use '{}' instead.", replacement)
        )
    }
}
```

### Cross-Actor Communication

The RpcActor queries ChainActor for consensus state:

```rust
impl RpcActor {
    async fn handle_consensus_state(&self) -> JsonRpcResponse {
        let (tx, rx) = oneshot::channel();

        self.chain_actor
            .send(ChainMessage::GetTendermintState { reply: tx })
            .await
            .map_err(|e| ...)?;

        let state = rx.await.map_err(|e| ...)?;

        JsonRpcResponse::success(ConsensusStateResponse::from(state))
    }

    async fn handle_validators(&self, params: ValidatorsRequest) -> JsonRpcResponse {
        let (tx, rx) = oneshot::channel();

        self.chain_actor
            .send(ChainMessage::GetValidatorSet {
                height: params.height,
                reply: tx
            })
            .await
            .map_err(|e| ...)?;

        let validator_set = rx.await.map_err(|e| ...)?;

        // Paginate response
        let paginated = paginate(
            validator_set.validators,
            params.page.unwrap_or(1),
            params.per_page.unwrap_or(30)
        );

        JsonRpcResponse::success(ValidatorsResponse {
            block_height: validator_set.height,
            validators: paginated,
            count: paginated.len() as u32,
            total: validator_set.validators.len() as u32,
        })
    }
}
```

## WebSocket Subscriptions

### New Subscription Types

```rust
pub enum SubscriptionType {
    // Existing
    NewHeads,
    Logs { filter: LogFilter },
    PendingTransactions,

    // New Tendermint subscriptions
    NewRound,           // Emits on each new consensus round
    Vote,               // Emits on each vote received
    ValidatorSetUpdates, // Emits when validator set changes
    GovernanceUpdates,  // Emits on governance parameter changes
}
```

### Subscription Messages

```rust
// NewRound event
pub struct NewRoundEvent {
    pub height: u64,
    pub round: u32,
    pub proposer: String,
    pub timestamp: DateTime<Utc>,
}

// Vote event
pub struct VoteEvent {
    pub vote_type: String,  // "Prevote" | "Precommit"
    pub height: u64,
    pub round: u32,
    pub validator: String,
    pub block_hash: Option<BlockHash>,
    pub timestamp: DateTime<Utc>,
}
```

## Error Codes

New Tendermint-specific error codes:

| Code | Message | Description |
|------|---------|-------------|
| -32050 | Consensus not ready | Node still syncing or consensus not initialized |
| -32051 | Height not found | Requested height doesn't exist |
| -32052 | Commit not available | Commit for height not yet produced |
| -32053 | Validator not found | Requested validator address not in set |
| -32054 | Governance update pending | Cannot query during governance transition |

## Testing

### Unit Tests

```rust
#[tokio::test]
async fn test_get_consensus_state() {
    let rpc = setup_rpc_actor().await;

    let response = rpc.handle_request(json!({
        "jsonrpc": "2.0",
        "method": "tendermint_consensusState",
        "params": [],
        "id": 1
    })).await;

    assert!(response["result"]["height"].as_u64().is_some());
    assert!(response["result"]["step"].as_str().is_some());
}

#[tokio::test]
async fn test_get_validators_pagination() {
    let rpc = setup_rpc_actor_with_validators(100).await;

    let response = rpc.handle_request(json!({
        "jsonrpc": "2.0",
        "method": "tendermint_validators",
        "params": { "page": 1, "per_page": 10 },
        "id": 1
    })).await;

    let validators = response["result"]["validators"].as_array().unwrap();
    assert_eq!(validators.len(), 10);
    assert_eq!(response["result"]["total"].as_u64().unwrap(), 100);
}

#[tokio::test]
async fn test_deprecated_endpoint_warning() {
    let rpc = setup_rpc_actor().await;

    let response = rpc.handle_request(json!({
        "jsonrpc": "2.0",
        "method": "aura_currentAuthorities",
        "params": [],
        "id": 1
    })).await;

    assert_eq!(response["error"]["code"].as_i64().unwrap(), -32000);
    assert!(response["error"]["message"].as_str().unwrap()
        .contains("tendermint_validators"));
}
```

### Integration Tests

1. **Full consensus query flow**: Start network, produce blocks, query all endpoints
2. **Validator set changes**: Test queries during validator transitions
3. **WebSocket subscriptions**: Verify events emitted correctly
4. **Deprecation warnings**: Ensure old clients get helpful migration messages

## Migration Checklist

- [ ] Implement new `tendermint_*` RPC methods
- [ ] Add ChainMessage variants for state queries
- [ ] Modify block responses to include Tendermint fields
- [ ] Implement WebSocket subscription types
- [ ] Add deprecation handlers for Aura methods
- [ ] Update RPC documentation
- [ ] Add integration tests for all new endpoints
- [ ] Update client libraries with new types
