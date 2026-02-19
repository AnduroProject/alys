# Tendermint Migration Implementation Prompt

You are implementing a migration of Alys V2 from Aura-based consensus to Tendermint-style two-phase BFT consensus. This is a comprehensive refactoring of the consensus layer while maintaining the existing execution layer and bridge infrastructure.

## Project Context

- **Codebase Location**: `/Users/michael/zDevelopment/Mara/alys-v2/`
- **V2 Actors Location**: `app/src/actors_v2/`
- **Implementation Plan**: `docs/v2_alpha/tendermint_implementation/`
- **Current Consensus**: Aura (round-robin slot-based)
- **Target Consensus**: Tendermint BFT (Propose → Prevote → Precommit → Commit)

## Core Principles

1. **Instant Finality**: 2/3+ precommits = irreversible commit. No fork choice, no reorgs, no orphan blocks.
2. **Safety First**: WAL writes MUST happen before broadcasting votes. Never double-vote.
3. **LastCommit in Blocks**: Block N contains the commit proof for Block N-1 (standard CometBFT pattern).
4. **Simplicity**: Learn from V1's failure - avoid over-engineering. Clear separation of concerns.
5. **Co-existence**: V2 must initially run alongside V0 without breaking existing functionality.

## Implementation Order (Critical)

Follow this exact order to manage dependencies correctly:

### Phase 1: Foundation (Weeks 1-5)
1. **01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md** - Create all base types first
   - Files: `tendermint/mod.rs`, `messages.rs`, `types.rs`, `governance.rs`, `params.rs`, `pegin.rs`, `block.rs`
   - Types: ValidatorId, TendermintStep, VoteType, Commit, CommitSig, BlockIDFlag, ValidatorSet, Vote, Proposal

2. **02_STATE_MACHINE.md** - Core consensus state machine
   - TendermintState struct with height, round, step, locked_value, locked_round, valid_value, valid_round
   - ConsensusEvent and ConsensusAction enums
   - Locking rules: once locked, only vote for locked block or NIL unless valid POL exists

3. **03_VOTE_SET_MANAGEMENT.md** - Vote tracking and threshold detection
   - VoteSet for +2/3 detection
   - build_commit_sigs() to create Commit proofs from collected votes

4. **06_WAL.md** - Write-ahead log for safety
   - WALEntry: NewRound, SentProposal, SentPrevote, SentPrecommit, Locked, Commit
   - CRITICAL: Write to WAL BEFORE broadcasting any vote

5. **08_TIMEOUT_MANAGEMENT.md** - Timeout configuration
   - Base timeouts: Propose 3000ms, Prevote 1000ms, Precommit 1000ms
   - Exponential backoff: timeout(r) = base + delta * r

### Phase 2: Integration (Weeks 5-10)
6. **05_NETWORK_LAYER.md** - Gossipsub topics and message routing
   - Topics: /alys/tendermint/proposals, /alys/tendermint/votes, /alys/tendermint/timeouts

7. **07_EL_COORDINATION.md** - Execution layer integration
   - Replace fork_choice_updated with direct block execution
   - ExecuteBlockMessage for EL coordination

8. **04_CHAINACTOR_HANDLERS.md** - Main integration point (LARGEST)
   - New ChainMessage variants: TendermintNewHeight, TendermintPropose, TendermintProposal, TendermintVote, TendermintTimeout, TendermintGovernanceUpdate, TendermintSubmitAuxBlock
   - Handler implementations for all consensus phases

9. **15_VALIDATION_MODULE.md** - Block and vote validation
   - verify_proposal(), verify_vote(), verify_commit()
   - Proposer selection: proposer_index = (height + round) % validator_count

10. **11_STORAGE_SCHEMA_MIGRATION.md** - Database schema updates
    - Updated ConsensusBlock with last_commit field
    - New column families: CF_VALIDATOR_SETS, CF_PARAMETER_HISTORY

### Phase 3: System Integration (Weeks 10-14)
11. **10_SLOT_WORKER_TO_TENDERMINT_TIMING.md** - Replace slot worker
    - TendermintDriver replaces slot_worker.rs
    - Event-driven vs time-based block production

12. **09_SYNC_ACTOR.md** - Simplified sync state machine
    - 6 states vs 8 (no fork choice complexity)
    - DELETE: fork_choice.rs, reorganization.rs, orphan_cache.rs

13. **14_GENESIS_AND_VALIDATOR_INIT.md** - Genesis configuration
    - New GenesisConfig with validators, consensus_params, governance_authority
    - GenesisValidator with public_key and voting_power

14. **17_GOVERNANCE_PARAMETERS.md** - Runtime parameter management
    - GovernanceUpdate: Validator, Parameter, Emergency
    - Activation timing: Emergency H+0, Parameter H+1, Validator H+2

15. **16_AUXPOW_TENDERMINT_INTEGRATION.md** - Merge-mining integration
    - Simplified model: AuxPoW optional per-block, no checkpoints, no liveness gate
    - Extended AuxPowHeader with pegins field
    - Four-layer duplicate peg-in prevention

16. **13_BRIDGE_INTEGRATION.md** - Bridge finality handling
    - Instant finality for peg-ins (no AuxPoW wait)
    - Finality detection via embedded LastCommit

### Phase 4: Testing & Polish (Weeks 14-18)
17. **12_RPC_ACTOR_MIGRATION.md** - RPC endpoint updates
    - New endpoints for Tendermint consensus queries
    - Deprecate Aura-specific methods

## Key Data Structures

```rust
// Core consensus state
pub struct TendermintState {
    pub height: u64,
    pub round: u32,
    pub step: TendermintStep,
    pub locked_value: Option<BlockHash>,
    pub locked_round: Option<u32>,
    pub valid_value: Option<BlockHash>,
    pub valid_round: Option<u32>,
    pub proposal: Option<Proposal>,
    pub prevotes: VoteSet,
    pub precommits: VoteSet,
}

// Commit proof embedded in next block
pub struct Commit {
    pub height: u64,
    pub round: u32,
    pub block_id: BlockHash,
    pub signatures: Vec<CommitSig>,
}

// Validator with voting power
pub struct Validator {
    pub id: ValidatorId,
    pub public_key: PublicKey,
    pub voting_power: u64,
}
```

## Critical Safety Rules

1. **WAL Before Broadcast**: ALWAYS write vote to WAL before sending to network
2. **No Double Voting**: Check WAL on startup; if already voted for (H,R), use same vote
3. **Locking Discipline**:
   - Lock on block when seeing +2/3 prevotes for it
   - Only unlock if valid POL (Proof of Lock) exists for different block at higher round
4. **Commit Threshold**: Exactly 2/3+ of total voting power required for commit
5. **LastCommit Validation**: Block N's last_commit must be valid commit for block N-1

## Files to Create (New)

```
app/src/actors_v2/tendermint/
├── mod.rs           # Module exports
├── messages.rs      # Protocol messages (Vote, Proposal, etc.)
├── types.rs         # Core types (ValidatorId, TendermintStep, etc.)
├── state.rs         # TendermintState and state machine
├── vote_set.rs      # VoteSet for threshold detection
├── wal.rs           # Write-ahead log
├── driver.rs        # TendermintDriver (replaces slot_worker)
├── validation.rs    # verify_proposal, verify_vote, verify_commit
├── governance.rs    # GovernanceUpdate handling
├── params.rs        # ChainParams and parameter management
├── pegin.rs         # PegInInfo and peg-in handling
└── block.rs         # ConsensusBlockHeader with Tendermint fields
```

## Files to Modify

- `app/src/actors_v2/chain/mod.rs` - Add Tendermint message handlers
- `app/src/actors_v2/chain/messages.rs` - New ChainMessage variants
- `app/src/actors_v2/network/mod.rs` - Gossipsub topics for consensus
- `app/src/actors_v2/storage/mod.rs` - New column families
- `app/src/actors_v2/sync/mod.rs` - Simplified state machine

## Files to DELETE

- `fork_choice.rs` - No fork choice with instant finality
- `reorganization.rs` - No reorgs with instant finality
- `orphan_cache.rs` - No orphan blocks with instant finality
- `slot_worker.rs` - Replaced by TendermintDriver

## Testing Requirements

1. **Unit Tests**: Each module should have comprehensive unit tests
2. **Integration Tests**: Multi-validator consensus scenarios
3. **Safety Tests**: Double-voting prevention, WAL recovery, crash recovery
4. **Liveness Tests**: Timeout escalation, round advancement, view change

## Success Criteria

- [ ] All 17 implementation documents completed (including creating missing doc 12)
- [ ] Tendermint consensus produces blocks with 2/3+ validator agreement
- [ ] WAL correctly prevents double-voting after crash recovery
- [ ] LastCommit embedded in blocks and validated correctly
- [ ] Bridge recognizes instant finality for peg-in processing
- [ ] AuxPoW optional enhancement working with miner peg-in submission
- [ ] Governance parameter updates activate at correct heights
- [ ] All existing V0 functionality preserved during co-existence period
- [ ] Comprehensive test coverage for safety-critical paths

## Reference Documents

Read these documents in order as you implement:
1. `docs/v2_alpha/tendermint_implementation/00_INDEX.md` - Master index
2. Each numbered document (01-17) in the order specified above
3. `docs/v2_alpha/tendermint_implementation/MINER_PEGIN_IMPACT_ANALYSIS.md` - Supplementary merge-mining details

Prioritize safety over speed - consensus bugs are extremely difficult to debug in production.
