# Tendermint Consensus Implementation Guide

## Overview

This directory contains comprehensive implementation plans for migrating Alys V2 from Aura-based consensus to Tendermint-style two-phase BFT consensus. Each document provides detailed code examples, mermaid diagrams, and step-by-step integration guidance.

**Total Estimated Effort**: 18-24 weeks

---

## Document Index

### Core Consensus (Documents 01-09)

| # | Document | Effort | Dependencies |
|---|----------|--------|--------------|
| 01 | [Message Types & Protocol Foundation](01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md) | 3-5 days | None |
| 02 | [State Machine](02_STATE_MACHINE.md) | 2 weeks | 01 |
| 03 | [Vote Set Management](03_VOTE_SET_MANAGEMENT.md) | 1 week | 01 |
| 04 | [ChainActor Handler Modifications](04_CHAINACTOR_HANDLERS.md) | 3 weeks | 01, 02, 03 |
| 05 | [Network Layer Integration](05_NETWORK_LAYER.md) | 1-2 weeks | 01, 04 |
| 06 | [Write-Ahead Log (WAL)](06_WAL.md) | 1 week | 01 |
| 07 | [Execution Layer Coordination](07_EL_COORDINATION.md) | 1 week | 02, 04 |
| 08 | [Timeout Management](08_TIMEOUT_MANAGEMENT.md) | 2-3 days | 02, 04 |
| 09 | [SyncActor Modifications](09_SYNC_ACTOR.md) | 1-2 weeks | 01, 04, 05 |

### System Integration (Documents 10-17)

| # | Document | Effort | Dependencies |
|---|----------|--------|--------------|
| 10 | [Slot Worker to Tendermint Timing](10_SLOT_WORKER_TO_TENDERMINT_TIMING.md) | 3-5 days | 02, 08 |
| 11 | [Storage Schema Migration](11_STORAGE_SCHEMA_MIGRATION.md) | 1 week | 01 |
| 12 | [RPC Actor Migration](12_RPC_ACTOR_MIGRATION.md) | 2-3 days | 11, 13 |
| 13 | [Bridge Integration](13_BRIDGE_INTEGRATION.md) | 3-5 days | 07 |
| 14 | [Genesis & Validator Init](14_GENESIS_AND_VALIDATOR_INIT.md) | 1-2 days | 01 |
| 15 | [Validation Module](15_VALIDATION_MODULE.md) | 3-5 days | 01, 02, 03 |
| 16 | [AuxPoW-Tendermint Integration](16_AUXPOW_TENDERMINT_INTEGRATION.md) | 2-3 weeks | 07, 13 |
| 17 | [Governance Parameters](17_GOVERNANCE_PARAMETERS.md) | 1-2 weeks | 14, 11 |

---

## Implementation Order

### Phase 1: Foundation (4-5 weeks)

```mermaid
graph LR
    A[01 Message Types] --> B[02 State Machine]
    A --> C[03 Vote Set]
    A --> F[06 WAL]
    B --> D[04 Handlers]
    C --> D
```

**Order of implementation:**
1. **01 Message Types** - Define all data structures first
2. **03 Vote Set** - Can be done in parallel with 02
3. **02 State Machine** - Core consensus logic
4. **06 WAL** - Critical for safety, can start early
5. **04 Handlers** - Integrate everything

### Phase 2: Integration (4-5 weeks)

```mermaid
graph LR
    D[04 Handlers] --> E[05 Network]
    D --> G[07 EL Coordination]
    D --> H[08 Timeouts]
    E --> I[09 SyncActor]
    D --> I
```

**Order of implementation:**
1. **05 Network Layer** - Message routing
2. **07 EL Coordination** - Block finalization
3. **08 Timeout Management** - Liveness
4. **09 SyncActor** - Commit-proof based sync (replaces fork-choice sync)

### Phase 3: System Integration (3-4 weeks)

```mermaid
graph LR
    B[02 State Machine] --> J[10 Timing]
    H[08 Timeouts] --> J
    A[01 Message Types] --> K[11 Storage]
    K --> L[12 RPC]
    M[13 Bridge] --> L
    G[07 EL Coordination] --> M
    A --> N[14 Genesis]
    A --> O[15 Validation]
    B --> O
    C[03 Vote Set] --> O
```

**Order of implementation:**
1. **11 Storage Schema** - Foundation for new data types
2. **14 Genesis & Validator Init** - Required for bootstrap
3. **15 Validation Module** - Replace Aura validation
4. **10 Timing** - Replace slot worker with TendermintDriver
5. **13 Bridge Integration** - Instant finality for peg-ins
6. **12 RPC Migration** - Checkpoint-based mining API

### Phase 4: Testing & Hardening (3-4 weeks)

1. Unit tests for all components (01-15)
2. Integration tests (4-node testnet)
3. Adversarial testing (Byzantine scenarios)
4. Performance benchmarking
5. Bridge peg-in/peg-out end-to-end testing
6. Checkpoint anchoring verification

---

## File Structure

After implementation, the Tendermint module will have this structure:

```
app/src/actors_v2/chain/tendermint/
├── mod.rs                    # Module root and re-exports
├── types.rs                  # Core types (ValidatorId, TendermintStep, etc.)
├── messages.rs               # Protocol messages (Proposal, Vote, etc.)
├── state_machine.rs          # TendermintState and transitions
├── vote_set.rs               # Vote collection and thresholds
├── timeout.rs                # Timeout scheduling
├── wal.rs                    # Write-ahead log
├── evidence.rs               # Equivocation detection
├── proposer.rs               # Proposer selection
├── validation.rs             # Tendermint signature validation (15)
├── driver.rs                 # TendermintDriver (replaces slot worker) (10)
└── handlers.rs               # Handler implementations

app/src/actors_v2/storage/
├── schema.rs                 # Updated with CF_VALIDATOR_SETS, CF_CHECKPOINTS (11)
│                             # Note: NO CF_COMMITS - commits are embedded in blocks
└── messages.rs               # New: GetCommitForHeightMessage, validator set messages, etc.

app/src/actors_v2/rpc/
└── actor.rs                  # Updated for checkpoint mining API (12)

app/src/bridge/
└── mod.rs                    # Updated for instant finality (13)

app/src/genesis/
└── mod.rs                    # Updated with ValidatorSet config (14)
```

---

## Key Concepts

### Block Structure with LastCommit

Following standard Tendermint/CometBFT architecture, **LastCommit is embedded in the block structure**:

```
Block N:
├── Header
│   └── parent_hash: hash(Block N-1)
├── last_commit: Commit for Block N-1  ← +2/3 precommit signatures
│   ├── height: N-1
│   ├── round: R
│   ├── block_hash: hash(Block N-1)
│   └── signatures: [CommitSig, CommitSig, ...]
├── execution_payload: EVM state transition
└── ... other fields
```

**Key Insight**: `LoadBlockCommit(height)` returns `Block[height+1].last_commit`

This design ensures:
- Atomic persistence of block + previous finality proof
- Light client efficiency (single fetch proves finality)
- No separate commit storage needed

### Consensus Flow

```
Height H, Round R:

  ┌──────────┐     ┌──────────┐     ┌────────────┐     ┌────────┐
  │ PROPOSE  │ ──► │ PREVOTE  │ ──► │ PRECOMMIT  │ ──► │ COMMIT │
  └──────────┘     └──────────┘     └────────────┘     └────────┘
       │               │                 │                │
       ▼               ▼                 ▼                ▼
    Proposer       All vote          All vote          FINAL
    broadcasts     (2/3+)            (2/3+)           (instant)
```

### Safety Properties

1. **No Double Voting**: WAL ensures votes are recorded before broadcast
2. **Locking Rules**: Once locked, only vote for locked block or NIL
3. **Instant Finality**: 2/3+ precommits = irreversible commit

### Liveness Properties

1. **Timeouts**: Ensure progress even with failed proposers
2. **Round Advancement**: Move to new round if no majority
3. **Exponential Backoff**: Handle network delays gracefully

### Miner-Effectuated Peg-Ins (AuxPoW)

**Critical Requirement**: For AML/KYC legal compliance, **peg-ins must be effectuated by miners**, not the bridge or federation.

```
┌─────────────────┐                              ┌─────────────────┐
│  Bitcoin Chain  │                              │   Alys Chain    │
│                 │                              │                 │
│  Deposit to     │      Miner monitors          │                 │
│  Federation     │ ─────────────────────────►   │                 │
│  Address        │                              │                 │
└─────────────────┘                              │                 │
        │                                        │                 │
        │ Miner detects peg-in tx                │                 │
        │ (6+ confirmations)                     │                 │
        ▼                                        │                 │
┌─────────────────┐                              │                 │
│     Miner       │    submitauxblock(           │                 │
│                 │      block_hash,             │   Block N       │
│  - Monitors BTC │      auxpow_header,  ──────► │   ├─ AuxPoW     │
│  - Detects pegins│     pegins[]        )       │   │  └─ pegins  │
│  - Earns fee    │                              │   └─ EVM tx     │
└─────────────────┘                              │      (Withdraw) │
                                                 └─────────────────┘
```

**Miner Responsibilities:**
1. Monitor the Bitcoin federation deposit address for incoming transactions
2. Wait for sufficient confirmations (e.g., 6 blocks)
3. Include valid peg-in proofs in the `AuxPowHeader.pegins` field
4. Submit via `submitauxblock(block_hash, auxpow_header, pegins)`

**Miner Compensation:**
- Miners receive a **percentage of each peg-in amount** as compensation
- This incentivizes miners to actively monitor and include peg-ins
- Fee percentage configured in genesis/chain parameters

**Peg-In Flow:**
1. User deposits BTC to federation address
2. Miner detects deposit, waits for confirmations
3. Miner includes peg-in proof in `submitauxblock`
4. Tendermint proposer includes AuxPoW in block (if valid)
5. Peg-in becomes EVM `Withdrawal` in `execution_payload`
6. User receives wrapped BTC on Alys chain

- **See**: `MINER_PEGIN_IMPACT_ANALYSIS.md` for detailed analysis
- **See**: `16_AUXPOW_TENDERMINT_INTEGRATION.md` for AuxPoW integration

### Validator Set Updates (Governance Client + H+2)

Validator set changes are received from an external **Governance Client** service via gRPC bi-directional stream (similar to AuxPoW submission pattern):

```
┌─────────────────────┐      gRPC Stream      ┌─────────────────────┐
│  Governance Client  │ ◄──────────────────► │     ChainActor      │
│  (External Service) │   ValidatorUpdates    │     (Validator)     │
└─────────────────────┘                       └─────────────────────┘
                                                      │
                                                      ▼
                                             ┌─────────────────┐
                                             │ Proposer        │
                                             │ includes in     │
                                             │ block at H      │
                                             └─────────────────┘
                                                      │
                                                      ▼
                                             Activates at H+2
```

**Flow:**
1. Governance Client sends `ValidatorUpdate` messages via gRPC stream
2. ChainActor validates (governance signature) and queues updates
3. Proposer includes queued updates in block at height H
4. All validators verify updates when validating the proposal
5. Updates take effect at block **H+2** (standard Tendermint delay)

- **No epoch-based updates**: Changes take effect at H+2, not at fixed intervals
- **Power = 0**: Removes a validator from the set
- **Constraints**: Max validators, max total power, min 4 validators for BFT
- **See**: `14_GENESIS_AND_VALIDATOR_INIT.md` Section 5 for full implementation details

### Governable Parameters (Governance Client)

Chain parameters can be modified by the federation via the Governance Client gRPC stream. All changes are included in blocks for auditability and verification by late-joining validators and light clients.

**Governable Parameter Categories:**

| Category | Examples | Activation |
|----------|----------|------------|
| Peg-In Compensation | `miner_fee_bps`, `min/max_fee_satoshi` | H+1 |
| Bridge Config | `btc_confirmations`, `min/max_peg_amount`, `federation_members` | H+1 |
| Checkpoint Config | `attestation_difficulty`, `checkpoint_difficulty`, `max_blocks_without_pow` | H+1 |
| Consensus Params | `propose_timeout_ms`, `max_validators` | H+1 |
| Emergency Controls | `chain_paused`, `pegins_paused`, `pegouts_paused` | Immediate (H+0) |

**Unified GovernanceUpdate Type:**

```rust
pub enum GovernanceUpdate {
    Validator(ValidatorUpdate),   // H+2 activation
    Parameter(ParameterUpdate),   // H+1 activation
    Emergency(EmergencyAction),   // Immediate activation
}
```

**Block Structure:**

```
Block N:
├── last_commit: Commit for Block N-1
├── execution_payload
├── auxpow_checkpoint: Option<AuxPowCheckpoint>
├── governance_updates: Option<Vec<GovernanceUpdate>>  ← All governance changes
├── validators_hash
├── next_validators_hash
└── params_hash  ← Hash of current chain parameters
```

**Late-Joiner Support:**
- Parameter history stored in `CF_PARAMETER_HISTORY` column family
- Late-joining validators reconstruct parameter state from storage
- Light clients verify via `params_hash` in block headers

- **See**: `17_GOVERNANCE_PARAMETERS.md` for full implementation details

---

## Integration Points

### With Existing V2 Actors

| Actor | Integration | Document |
|-------|-------------|----------|
| **ChainActor** | New Tendermint handlers replace block import | 04, 15 |
| **StorageActor** | Stores blocks, commits, validator sets, checkpoints | 11 |
| **NetworkActor** | New Gossipsub topics for Tendermint messages | 05 |
| **EngineActor** | Direct execution instead of fork_choice_updated | 07 |
| **SyncActor** | Commit-proof verification, simplified state machine | 09 |
| **RpcActor** | Checkpoint-based mining, validator status RPCs | 12 |
| **SlotWorker** | Replaced by TendermintDriver (event-driven timing) | 10 |
| **Bridge** | Instant finality peg-ins, checkpoint-based peg-outs | 13 |

### Code Removal / Replacement

| File | Action | Replacement |
|------|--------|-------------|
| `fork_choice.rs` | Remove entirely | Tendermint instant finality |
| `reorganization.rs` | Remove entirely | No reorgs with BFT consensus |
| `orphan_cache.rs` | Remove entirely | No orphan blocks possible |
| `handlers.rs` | Remove reorg logic | Tendermint handlers |
| `state.rs` | Remove difficulty tracking | Validator set tracking |
| `slot_worker.rs` | Replace with TendermintDriver | Event-driven timing (10) |
| `common/validation.rs` | Replace Aura validation | Tendermint validation (15) |
| `auxpow.rs` | Repurpose for checkpointing | Checkpoint layer (not fork choice) |

---

## Testing Strategy

### Unit Tests (per document)

Each document includes specific test cases for its components.

### Integration Tests

```rust
#[tokio::test]
async fn test_4_validator_consensus() {
    // 1. Setup 4 validators with Tendermint
    // 2. Produce block at height 1
    // 3. Verify all validators commit same block
    // 4. Verify instant finality
}

#[tokio::test]
async fn test_proposer_failure() {
    // 1. Setup 4 validators
    // 2. Kill proposer for round 0
    // 3. Verify timeout advances to round 1
    // 4. Verify new proposer completes consensus
}

#[tokio::test]
async fn test_wal_recovery() {
    // 1. Start consensus
    // 2. Vote prevote
    // 3. Crash validator
    // 4. Restart and verify no double vote
}
```

### Byzantine Tests

| Scenario | Expected Behavior |
|----------|-------------------|
| Double vote | Evidence created, vote rejected |
| Double propose | Evidence created, proposal rejected |
| 4/15 validators offline | Consensus continues (have 11/15) |
| 5/15 validators offline | Network halts until recovery |
| Network partition 8/7 | Both halves halt |

---

## Metrics

Key metrics to monitor:

```yaml
# Consensus progress
tendermint_height: Current consensus height
tendermint_round: Current round within height
tendermint_step: Current step (0-3)

# Vote collection
tendermint_prevotes_received: Total prevotes
tendermint_precommits_received: Total precommits
tendermint_prevote_time_seconds: Time to collect 2/3+ prevotes

# Liveness
tendermint_timeouts_total: Timeout events by step
tendermint_rounds_per_height: Rounds needed per block (>1 = issues)

# Performance
tendermint_block_time_seconds: Time from propose to commit
tendermint_commit_latency_seconds: End-to-end latency
```

---

## Getting Started

1. Read through all documents to understand the full scope
2. Start with `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md`
3. Create the `tendermint/` directory structure
4. Implement types and messages first
5. Follow dependency order for remaining components
6. Write tests as you implement each component

---

## Related Documents

- [Tendermint Migration Assessment](../TENDERMINT_MIGRATION_ASSESSMENT.md) - High-level analysis
- [Tendermint Consensus Guide](../TENDERMINT_CONSENSUS_GUIDE.md) - Protocol explanation

---

*Implementation Guide Version: 1.0*
*Last Updated: January 2026*
