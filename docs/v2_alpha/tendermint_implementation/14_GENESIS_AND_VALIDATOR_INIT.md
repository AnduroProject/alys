# Implementation Plan: Genesis Format and Validator Initialization

## Overview

This document provides a comprehensive implementation guide for updating the genesis block format and validator set initialization for Tendermint consensus. The key changes involve embedding the initial validator set (with voting power) into genesis and removing Aura-specific authority configuration.

**Estimated Effort**: 1-2 days
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (ValidatorSet type, CommitSig)
- `02_STATE_MACHINE.md` (TendermintState initialization)
- `11_STORAGE_SCHEMA_MIGRATION.md` (Embedded LastCommit architecture)
**Files to Modify**:
- `app/src/actors_v2/chain/genesis.rs`
- `app/src/config.rs` or genesis configuration files
- `app/src/actors_v2/chain/actor.rs`

### Embedded LastCommit Design

Following standard Tendermint/CometBFT architecture, commits are embedded in blocks:

```
Genesis Block (Height 0):      Block 1:
├── last_commit: None    →     ├── last_commit: Commit for Genesis
├── execution_payload          │   ├── height: 0
└── ...                        │   ├── round: 0
                               │   ├── block_hash: hash(Genesis)
                               │   └── signatures: [CommitSig, ...]
                               ├── execution_payload
                               └── ...
```

**Key Insight**: `get_commit(height)` returns `Block[height+1].last_commit`

---

## 1. Current vs New Genesis

### 1.1 Current Genesis (Aura)

```rust
// CURRENT: Aura-style authority list
pub struct GenesisConfig {
    /// Chain ID
    pub chain_id: u64,

    /// Genesis timestamp
    pub timestamp: u64,

    /// Ordered list of authority public keys (round-robin)
    pub authorities: Vec<PublicKey>,

    /// Slot duration in seconds
    pub slot_duration: u64,

    /// Execution layer genesis
    pub execution_genesis: ExecutionGenesis,
}
```

### 1.2 New Genesis (Tendermint)

```rust
// NEW: Tendermint-style validator set with voting power
pub struct GenesisConfig {
    /// Chain ID
    pub chain_id: u64,

    /// Genesis timestamp
    pub timestamp: u64,

    /// Initial validator set with voting power
    pub validators: Vec<GenesisValidator>,

    /// Tendermint consensus parameters
    pub consensus_params: TendermintConsensusParams,

    /// Execution layer genesis
    pub execution_genesis: ExecutionGenesis,

    /// AuxPoW checkpoint configuration
    pub checkpoint_config: CheckpointConfig,

    /// Bridge configuration
    pub bridge_config: BridgeGenesisConfig,

    /// Peg-in compensation for miners (NEW)
    pub pegin_compensation: PegInCompensation,
}

/// Validator entry in genesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisValidator {
    /// Validator public key (BLS)
    pub public_key: PublicKey,

    /// Voting power (determines influence in consensus)
    /// Total voting power of all validators should sum to a known value
    pub voting_power: u64,

    /// Human-readable name (optional, for display)
    pub name: Option<String>,

    /// Validator's network address (for peer discovery)
    pub network_address: Option<String>,
}

/// Tendermint consensus parameters in genesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendermintConsensusParams {
    /// Base timeout for propose step (milliseconds)
    pub propose_timeout_ms: u64,

    /// Base timeout for prevote step (milliseconds)
    pub prevote_timeout_ms: u64,

    /// Base timeout for precommit step (milliseconds)
    pub precommit_timeout_ms: u64,

    /// Timeout increase per round (milliseconds)
    pub timeout_delta_ms: u64,

    /// Maximum validators (for voting power calculation)
    pub max_validators: u32,

    /// Maximum total voting power (prevents overflow)
    /// Default: MaxInt64 / 8 per Tendermint spec
    pub max_total_voting_power: u64,
}

impl Default for TendermintConsensusParams {
    fn default() -> Self {
        Self {
            propose_timeout_ms: 3000,    // 3 seconds
            prevote_timeout_ms: 1000,    // 1 second
            precommit_timeout_ms: 1000,  // 1 second
            timeout_delta_ms: 500,       // 500ms increase per round
            max_validators: 15,
            max_total_voting_power: i64::MAX as u64 / 8,  // ~1.15e18
        }
    }
}

/// Peg-in compensation parameters for miners
/// Miners receive a percentage of each peg-in amount as incentive
/// for monitoring Bitcoin and including peg-ins in their AuxPoW submissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegInCompensation {
    /// Percentage of peg-in amount paid to miner (basis points)
    /// e.g., 50 = 0.5%, 100 = 1%
    pub miner_fee_bps: u64,

    /// Minimum fee in satoshis (floor for small peg-ins)
    pub min_fee_satoshi: u64,

    /// Maximum fee in satoshis (cap for large peg-ins)
    pub max_fee_satoshi: u64,
}

impl Default for PegInCompensation {
    fn default() -> Self {
        Self {
            miner_fee_bps: 50,           // 0.5% default
            min_fee_satoshi: 1_000,      // 0.00001 BTC minimum (~$1 at $100k/BTC)
            max_fee_satoshi: 10_000_000, // 0.1 BTC maximum (~$10k at $100k/BTC)
        }
    }
}

impl PegInCompensation {
    /// Calculate miner compensation for a peg-in amount
    pub fn calculate_fee(&self, amount_satoshi: u64) -> u64 {
        let fee = (amount_satoshi * self.miner_fee_bps) / 10_000;
        fee.clamp(self.min_fee_satoshi, self.max_fee_satoshi)
    }
}
```

---

## 2. Genesis File Format

### 2.1 JSON Genesis File

```json
{
  "chain_id": 2121,
  "timestamp": 1704067200,

  "validators": [
    {
      "public_key": "0x8a1b2c3d4e5f...",
      "voting_power": 100,
      "name": "Validator-1",
      "network_address": "/ip4/10.0.0.1/tcp/9000/p2p/12D3Koo..."
    },
    {
      "public_key": "0x9b2c3d4e5f6a...",
      "voting_power": 100,
      "name": "Validator-2",
      "network_address": "/ip4/10.0.0.2/tcp/9000/p2p/12D3Koo..."
    }
  ],

  "consensus_params": {
    "propose_timeout_ms": 3000,
    "prevote_timeout_ms": 1000,
    "precommit_timeout_ms": 1000,
    "timeout_delta_ms": 500,
    "max_validators": 15
  },

  "checkpoint_config": {
    "min_checkpoint_interval": 100,
    "target_checkpoint_interval": 500,
    "min_checkpoint_difficulty": "0x3b9aca00"
  },

  "bridge_config": {
    "btc_confirmations": 6,
    "federation_threshold": 11,
    "min_peg_amount": 10000,
    "max_peg_amount": 100000000
  },

  "pegin_compensation": {
    "miner_fee_bps": 50,
    "min_fee_satoshi": 1000,
    "max_fee_satoshi": 10000000
  },

  "execution_genesis": {
    "state_root": "0x...",
    "alloc": {}
  }
}
```

### 2.2 Genesis Parsing

```rust
// In genesis.rs

impl GenesisConfig {
    /// Load genesis from JSON file
    pub fn from_file(path: &Path) -> Result<Self, GenesisError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| GenesisError::FileRead(e.to_string()))?;

        let config: GenesisConfig = serde_json::from_str(&contents)
            .map_err(|e| GenesisError::Parse(e.to_string()))?;

        config.validate()?;

        Ok(config)
    }

    /// Validate genesis configuration
    pub fn validate(&self) -> Result<(), GenesisError> {
        // 1. Must have at least 4 validators for BFT
        if self.validators.len() < 4 {
            return Err(GenesisError::InsufficientValidators {
                have: self.validators.len(),
                need: 4,
            });
        }

        // 2. Voting power must be positive for all validators
        for (i, v) in self.validators.iter().enumerate() {
            if v.voting_power == 0 {
                return Err(GenesisError::ZeroVotingPower { index: i });
            }
        }

        // 3. Total voting power must not overflow
        let total_power: u64 = self.validators.iter()
            .map(|v| v.voting_power)
            .try_fold(0u64, |acc, p| acc.checked_add(p))
            .ok_or(GenesisError::VotingPowerOverflow)?;

        // 4. 2/3 threshold must be calculable
        let threshold = (total_power * 2 / 3) + 1;
        if threshold > total_power {
            return Err(GenesisError::InvalidThreshold);
        }

        // 5. Max validators check
        if self.validators.len() > self.consensus_params.max_validators as usize {
            return Err(GenesisError::TooManyValidators {
                have: self.validators.len(),
                max: self.consensus_params.max_validators as usize,
            });
        }

        Ok(())
    }

    /// Convert to ValidatorSet for runtime use
    pub fn to_validator_set(&self) -> ValidatorSet {
        let validators: Vec<Validator> = self.validators.iter()
            .enumerate()
            .map(|(i, gv)| Validator {
                id: ValidatorId(i as u8),
                public_key: gv.public_key.clone(),
                power: gv.voting_power,
            })
            .collect();

        ValidatorSet::new(validators)
    }

    /// Get proposer for height 0, round 0 (genesis block)
    pub fn genesis_proposer(&self) -> &GenesisValidator {
        // Proposer = (height + round) % num_validators = 0 % n = 0
        &self.validators[0]
    }
}
```

---

## 3. Genesis Block Creation

### 3.1 Current Genesis Block

```rust
// CURRENT: Simple block without consensus data
impl ChainActor {
    async fn create_genesis_block(&self) -> Result<SignedConsensusBlock, ChainError> {
        let engine = self.engine_actor.as_ref()?;

        // Get execution payload from EL
        let payload = engine.send(GetGenesisPayloadMessage {}).await??;

        // Create consensus block
        let block = ConsensusBlock {
            slot: 0,
            proposer_index: 0,
            parent_root: Hash256::zero(),
            state_root: payload.state_root,
            execution_payload: payload,
            auxpow_header: None,
        };

        // Sign (empty signature for genesis)
        Ok(SignedConsensusBlock {
            message: block,
            signature: Signature::empty(),
        })
    }
}
```

### 3.2 New Genesis Block (Tendermint)

**Key Insight: Embedded LastCommit Architecture**

Following standard Tendermint/CometBFT design:
- **Genesis block (height 0)** has `last_commit: None` - there's no previous block
- **Block 1** contains `last_commit` with the commit proof for genesis
- The commit for any height H is retrieved from `Block[H+1].last_commit`

```rust
// NEW: Genesis block with validator set commitment
impl ChainActor {
    /// Create genesis block with Tendermint validator set
    ///
    /// Genesis is special:
    /// - `last_commit` is None (no previous block to commit)
    /// - The commit for genesis will be embedded in Block 1's last_commit
    async fn create_genesis_block(&self) -> Result<(SignedConsensusBlock, Commit), ChainError> {
        let config = &self.genesis_config;
        let engine = self.engine_actor.as_ref()?;

        // 1. Get execution payload from EL
        let payload = engine.send(GetGenesisPayloadMessage {}).await??;

        // 2. Create consensus block with validator set info
        //    Genesis has last_commit: None (no previous block)
        let block = ConsensusBlock {
            slot: 0,  // Height 0
            proposer_index: 0,  // First validator proposes genesis
            parent_root: Hash256::zero(),
            state_root: payload.state_root,
            execution_payload: payload,
            auxpow_header: None,
            last_commit: None,  // Genesis has no previous block

            // NEW: Tendermint-specific fields
            validator_set_hash: config.to_validator_set().hash(),
            next_validator_set_hash: config.to_validator_set().hash(),  // Same for genesis
        };

        // 3. Create genesis commit (special: signed by all validators offline)
        // In practice, genesis commit is created during network bootstrap ceremony.
        // This commit will be embedded in Block 1's last_commit field.
        let commit = self.create_genesis_commit(&block, config)?;

        // 4. Sign block (proposer signature)
        let signed_block = SignedConsensusBlock {
            message: block,
            signature: Signature::empty(),  // Genesis has no proposer signature
        };

        Ok((signed_block, commit))
    }

    /// Create commit for genesis block
    ///
    /// This is done offline during network bootstrap ceremony:
    /// Each validator signs the genesis block hash, creating CommitSig entries.
    /// This commit is NOT stored separately - it will be embedded in Block 1's last_commit.
    fn create_genesis_commit(
        &self,
        block: &ConsensusBlock,
        config: &GenesisConfig,
    ) -> Result<Commit, ChainError> {
        let validator_set = config.to_validator_set();
        let block_hash = block.hash();

        // Build CommitSig for each validator
        // In production, each validator signs during genesis ceremony
        let signatures: Vec<CommitSig> = validator_set.validators.iter()
            .map(|validator| {
                // For testing: sign with our key if we're this validator
                // In production: signatures collected during genesis ceremony
                let signature = if let Some(signer) = self.state.signer.as_ref() {
                    if signer.public_key() == &validator.public_key {
                        let signing_root = compute_precommit_signing_root(0, 0, block_hash);
                        Some(signer.sign(&signing_root))
                    } else {
                        // Placeholder for other validators' signatures
                        // In production, collected during ceremony
                        None
                    }
                } else {
                    None
                };

                match signature {
                    Some(sig) => CommitSig {
                        block_id_flag: BlockIDFlag::Commit,
                        validator_address: Some(validator.id),
                        timestamp: block.slot,  // Use genesis timestamp
                        signature: Some(sig),
                    },
                    None => CommitSig::absent(),
                }
            })
            .collect();

        Ok(Commit {
            height: 0,
            round: 0,
            block_hash,
            signatures,
        })
    }
}
```

---

## 4. ChainActor Initialization

### 4.1 Current Initialization

```rust
// CURRENT: Initialize with Aura authorities
impl ChainActor {
    pub async fn initialize(&mut self) -> Result<(), ChainError> {
        // Load authorities from config
        self.state.authorities = self.config.authorities.clone();

        // Create genesis if needed
        if !self.has_genesis().await? {
            let genesis = self.create_genesis_block().await?;
            self.store_genesis(genesis).await?;
        }

        Ok(())
    }
}
```

### 4.2 New Initialization (Tendermint)

**Key Design: Embedded LastCommit**

With embedded LastCommit architecture:
- Genesis block is stored with `last_commit: None`
- Genesis commit is cached as `pending_commit`
- When Block 1 is created, genesis commit is embedded in `Block1.last_commit`
- No separate commit storage (CF_COMMITS) needed

```rust
// NEW: Initialize with Tendermint validator set
impl ChainActor {
    pub async fn initialize(&mut self) -> Result<(), ChainError> {
        // 1. Load genesis configuration
        let genesis_config = GenesisConfig::from_file(&self.config.genesis_path)?;

        // 2. Create validator set
        let validator_set = Arc::new(genesis_config.to_validator_set());

        // 3. Initialize Tendermint state
        self.state.tendermint = TendermintState::new(validator_set.clone());

        // 4. Store consensus parameters
        self.state.consensus_params = genesis_config.consensus_params.clone();

        // 5. Create genesis if needed
        if !self.has_genesis().await? {
            let (genesis_block, genesis_commit) = self.create_genesis_block().await?;

            // Store genesis block (with last_commit: None)
            self.store_genesis(genesis_block.clone()).await?;

            // Cache genesis commit for embedding in Block 1's last_commit
            // This follows standard Tendermint pattern: commits are NOT stored
            // separately, they're embedded in the next block's last_commit field
            self.state.tendermint.pending_commit = Some(genesis_commit);

            // Store initial validator set (effective from height 0)
            self.storage_actor.as_ref()
                .ok_or(ChainError::StorageActorNotSet)?
                .send(StoreValidatorSetMessage {
                    effective_height: 0,
                    validator_set: (*validator_set).clone(),
                    correlation_id: None,
                })
                .await??;

            tracing::info!(
                validators = validator_set.len(),
                total_power = validator_set.total_power(),
                "Genesis block created with validator set"
            );
        } else {
            // Load existing state
            self.load_tendermint_state().await?;
        }

        // 6. Initialize Tendermint driver
        self.start_tendermint_driver().await?;

        Ok(())
    }

    /// Load Tendermint state from storage (after restart)
    async fn load_tendermint_state(&mut self) -> Result<(), ChainError> {
        let storage = self.storage_actor.as_ref()
            .ok_or(ChainError::StorageActorNotSet)?;

        // Get current height
        let head = storage.send(GetChainHeadMessage { correlation_id: None })
            .await??
            .ok_or(ChainError::NoChainHead)?;

        // Get current validator set
        let validator_set = storage
            .send(GetValidatorSetForHeightMessage {
                height: head.number,
                correlation_id: None,
            })
            .await??
            .ok_or(ChainError::NoValidatorSet)?;

        // Initialize Tendermint state at next height
        self.state.tendermint = TendermintState::new(Arc::new(validator_set));
        self.state.tendermint.height = head.number + 1;
        self.state.tendermint.round = 0;

        tracing::info!(
            height = self.state.tendermint.height,
            validators = self.state.tendermint.validator_set.len(),
            "Loaded Tendermint state from storage"
        );

        Ok(())
    }
}
```

---

## 5. Validator Set Updates (Governance Client Pattern)

Validator set updates are received from an external **Governance Client** service via a gRPC bi-directional stream. This is similar to how AuxPoW submissions are handled — updates are queued and included in blocks by the proposer.

> **Note**: Validator updates are part of the unified `GovernanceUpdate` enum which also includes parameter changes and emergency actions. See `17_GOVERNANCE_PARAMETERS.md` for the complete governance framework. This section focuses on the validator-specific aspects.

### 5.1 Architecture Overview

```
┌─────────────────────┐      gRPC Stream      ┌─────────────────────┐
│  Governance Client  │ ◄──────────────────► │     ChainActor      │
│  (External Service) │   ValidatorUpdates    │     (Validator)     │
└─────────────────────┘                       └─────────────────────┘
                                                       │
                                                       ▼
                                              ┌─────────────────┐
                                              │ queued_validator│
                                              │ _updates        │
                                              └─────────────────┘
                                                       │
                                                       ▼
                                              ┌─────────────────┐
                                              │   Proposer      │
                                              │ includes in     │
                                              │ block           │
                                              └─────────────────┘
```

**Flow** (similar to AuxPoW):
1. Governance Client sends `ValidatorUpdate` messages via gRPC stream
2. ChainActor validates and queues the updates
3. When proposing a block, proposer includes queued updates
4. All validators verify the updates when validating the proposal
5. Updates included at block H take effect at block **H+2**

### 5.2 The H+2 Delay Rule

Following standard Tendermint, validator updates included in block H take effect at block H+2:

```
Block H:     Proposer includes ValidatorUpdate in block
             All validators verify and commit
Block H+1:   Update is "pending", network prepares
             Header.next_validators_hash = hash(new_validator_set)
Block H+2:   New validator set is ACTIVE
             Header.validators_hash = hash(new_validator_set)
```

**Why H+2?**
- Gives all nodes time to receive and process the change
- Enables light client verification via `next_validators_hash`
- Prevents race conditions in consensus

### 5.3 gRPC Service Definition

```protobuf
syntax = "proto3";

package alys.governance.v1;

// Bi-directional stream between Governance Client and Validator
service GovernanceService {
    // Stream for validator set updates
    rpc ValidatorUpdateStream(stream ValidatorUpdateRequest)
        returns (stream ValidatorUpdateResponse);
}

message ValidatorUpdateRequest {
    // Validator's BLS public key
    bytes public_key = 1;
    // New voting power (0 = remove validator)
    uint64 power = 2;
    // Signature from governance authority
    bytes governance_signature = 3;
}

message ValidatorUpdateResponse {
    // Status of the update
    UpdateStatus status = 1;
    // Height at which update was included (if accepted)
    uint64 included_height = 2;
    // Height at which update takes effect (included_height + 2)
    uint64 effective_height = 3;
    // Error message if rejected
    string error = 4;
}

enum UpdateStatus {
    UPDATE_STATUS_UNSPECIFIED = 0;
    UPDATE_STATUS_QUEUED = 1;
    UPDATE_STATUS_INCLUDED = 2;
    UPDATE_STATUS_REJECTED = 3;
}
```

### 5.4 ValidatorUpdate Type

```rust
/// A single validator update (add, modify power, or remove)
///
/// Updates are **idempotent**: applying the same update multiple times
/// has the same effect as applying it once. No deduplication needed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ValidatorUpdate {
    /// Validator's BLS public key (also serves as unique identifier)
    pub public_key: PublicKey,

    /// New voting power (0 = remove validator)
    pub power: u64,

    /// Signature from governance authority
    pub governance_signature: Signature,
}

impl ValidatorUpdate {
    /// Create an update to add/modify a validator
    pub fn set_power(public_key: PublicKey, power: u64, sig: Signature) -> Self {
        Self { public_key, power, governance_signature: sig }
    }

    /// Create an update to remove a validator
    pub fn remove(public_key: PublicKey, sig: Signature) -> Self {
        Self { public_key, power: 0, governance_signature: sig }
    }
}
```

### 5.5 Governance Stream Handler

```rust
impl ChainActor {
    /// Handle incoming validator update from governance client stream
    ///
    /// Updates are **idempotent**: the same update can be received multiple times
    /// without issues. Later updates for the same validator replace earlier ones
    /// in the queue. If the same (public_key, power) is already in the active
    /// validator set, applying it again is a no-op.
    pub async fn handle_governance_update(
        &mut self,
        update: ValidatorUpdate,
    ) -> Result<ValidatorUpdateResponse, ChainError> {
        // 1. Verify governance signature
        self.verify_governance_signature(&update)?;

        // 2. Validate the update against current state
        self.validate_validator_update(&update)?;

        // 3. Queue the update for inclusion in next proposed block
        //    Keyed by public_key: later updates for same validator replace earlier ones
        let public_key = update.public_key.clone();
        self.state.queued_validator_updates.insert(public_key.clone(), update);

        tracing::info!(
            validator = %public_key,
            "Validator update queued from governance client"
        );

        Ok(ValidatorUpdateResponse {
            status: UpdateStatus::Queued,
            ..Default::default()
        })
    }

    /// Verify the governance authority signature on an update
    fn verify_governance_signature(&self, update: &ValidatorUpdate) -> Result<(), ChainError> {
        let signing_data = self.compute_update_signing_root(update);

        if !self.governance_public_key.verify(&signing_data, &update.governance_signature) {
            return Err(ChainError::InvalidGovernanceSignature);
        }

        Ok(())
    }
}
```

### 5.6 Block Structure with Governance Updates

> **Unified Approach**: In the final implementation (see `17_GOVERNANCE_PARAMETERS.md`), validator updates are included via a unified `governance_updates` field that also handles parameter changes and emergency actions. The structure below shows this unified approach:

```rust
pub struct ConsensusBlock<T: EthSpec> {
    pub parent_hash: Hash256,
    pub slot: u64,
    pub last_commit: Option<Commit>,
    pub execution_payload: ExecutionPayloadCapella<T>,

    /// AuxPoW checkpoint (optional, at checkpoint heights)
    pub auxpow_checkpoint: Option<AuxPowCheckpoint>,

    /// All governance updates: validators, parameters, emergencies (optional)
    /// None for most blocks, Some when governance updates are included
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governance_updates: Option<Vec<GovernanceUpdate>>,

    /// Hash of current chain parameters (for light client verification)
    pub params_hash: Hash256,

    // ... other fields
}

/// Unified governance update type
pub enum GovernanceUpdate {
    Validator(ValidatorUpdate),   // H+2 activation
    Parameter(ParameterUpdate),   // H+1 activation
    Emergency(EmergencyAction),   // Immediate (H+0)
}
```

### 5.7 Proposer: Including Queued Updates

> **Unified Approach**: Using the `GovernanceQueue` from Document 17 which handles validators, parameters, and emergencies together:

```rust
impl ChainActor {
    /// Build a block proposal, including any queued governance updates
    async fn build_proposal(&mut self, height: u64) -> Result<ConsensusBlock, ChainError> {
        // ... build execution payload ...

        // Collect all queued governance updates (validators, params, emergencies)
        let governance_updates: Option<Vec<GovernanceUpdate>> = {
            if self.state.tendermint.queued_governance_updates.is_empty() {
                None
            } else {
                let updates = self.state.tendermint.queued_governance_updates.drain();

                tracing::info!(
                    height = height,
                    count = updates.len(),
                    "Including governance updates in proposal"
                );

                Some(updates)
            }
        };

        Ok(ConsensusBlock {
            parent_hash,
            slot: height,
            last_commit: self.state.tendermint.pending_commit.take(),
            execution_payload,
            auxpow_checkpoint: self.state.queued_pow.take(),
            governance_updates,  // None for most blocks
            params_hash: self.state.tendermint.chain_params.hash(),
            // ...
        })
    }
}
```

### 5.8 Validator: Verifying Updates in Proposal

> **Unified Approach**: Verifies all governance update types. See Document 17 for complete implementation.

```rust
impl ChainActor {
    /// Verify a proposal includes valid governance updates (if any)
    fn verify_proposal_governance_updates(
        &self,
        proposal: &ConsensusBlock,
    ) -> Result<(), ChainError> {
        // Most blocks have no governance updates
        let Some(updates) = &proposal.governance_updates else {
            return Ok(());
        };

        for update in updates {
            match update {
                GovernanceUpdate::Validator(vu) => {
                    self.verify_governance_signature_validator(vu)?;
                    self.validate_validator_update(vu)?;
                }
                GovernanceUpdate::Parameter(pu) => {
                    self.verify_governance_signature_param(pu)?;
                    self.validate_parameter_update(pu)?;
                }
                GovernanceUpdate::Emergency(ea) => {
                    self.verify_governance_signature_emergency(ea)?;
                }
            }
        }

        Ok(())
    }
}
```

### 5.9 Committing Updates (H+2 Activation)

```rust
impl ChainActor {
    /// Called after block is committed, schedules validator updates for H+2
    /// Most blocks have no updates (validator_updates is None)
    async fn process_committed_validator_updates(
        &mut self,
        height: u64,
        validator_updates: &Option<Vec<ValidatorUpdate>>,
    ) -> Result<(), ChainError> {
        // Most blocks have no validator updates
        let Some(updates) = validator_updates else {
            return Ok(());
        };

        // Compute new validator set
        let current_set = &self.state.tendermint.validator_set;
        let new_set = current_set.apply_updates(updates)?;

        // Schedule for H+2 activation
        let effective_height = height + 2;

        // Store the new validator set
        self.storage_actor.as_ref()
            .ok_or(ChainError::StorageActorNotSet)?
            .send(StoreValidatorSetMessage {
                effective_height,
                validator_set: new_set.clone(),
                correlation_id: None,
            })
            .await??;

        // Track pending update in state
        self.state.tendermint.pending_validator_updates
            .insert(effective_height, new_set);

        // Note: No deduplication tracking needed - updates are idempotent

        tracing::info!(
            height = height,
            effective_height = effective_height,
            updates = updates.len(),
            "Validator set update scheduled"
        );

        Ok(())
    }
}
```

### 5.10 Validation Constraints

```rust
impl ChainActor {
    /// Validate a single validator update
    fn validate_validator_update(&self, update: &ValidatorUpdate) -> Result<(), ChainError> {
        let params = &self.state.consensus_params;
        let current_set = &self.state.tendermint.validator_set;

        // Simulate applying this update
        let new_set = current_set.apply_updates(&[update.clone()])?;

        // Check max validators
        if new_set.len() > params.max_validators as usize {
            return Err(ChainError::TooManyValidators {
                have: new_set.len(),
                max: params.max_validators as usize,
            });
        }

        // Check max total voting power
        if new_set.total_power() > params.max_total_voting_power {
            return Err(ChainError::VotingPowerOverflow {
                total: new_set.total_power(),
                max: params.max_total_voting_power,
            });
        }

        // Ensure at least 4 validators remain (BFT requirement)
        if new_set.len() < 4 {
            return Err(ChainError::InsufficientValidators {
                have: new_set.len(),
                need: 4,
            });
        }

        // Check power is non-negative (0 = remove, >0 = add/modify)
        // (power is u64 so always non-negative)

        Ok(())
    }
}
```

### 5.11 Applying Updates to ValidatorSet (Idempotent)

```rust
impl ValidatorSet {
    /// Apply a batch of updates to create a new validator set
    pub fn apply_updates(&self, updates: &[ValidatorUpdate]) -> Result<ValidatorSet, ValidatorError> {
        let mut validators: HashMap<PublicKey, Validator> = self.validators
            .iter()
            .map(|v| (v.public_key.clone(), v.clone()))
            .collect();

        for update in updates {
            if update.power == 0 {
                // Remove validator
                if validators.remove(&update.public_key).is_none() {
                    return Err(ValidatorError::RemoveNonexistent {
                        public_key: update.public_key.clone(),
                    });
                }
            } else {
                // Add or update validator
                match validators.get_mut(&update.public_key) {
                    Some(existing) => {
                        // Update power
                        existing.power = update.power;
                    }
                    None => {
                        // Add new validator
                        let id = ValidatorId(self.next_validator_id());
                        validators.insert(update.public_key.clone(), Validator {
                            id,
                            public_key: update.public_key.clone(),
                            power: update.power,
                        });
                    }
                }
            }
        }

        // Rebuild sorted validator list
        let mut sorted_validators: Vec<Validator> = validators.into_values().collect();
        sorted_validators.sort_by(|a, b| b.power.cmp(&a.power));  // Sort by power descending

        // Reassign IDs based on new order
        for (i, v) in sorted_validators.iter_mut().enumerate() {
            v.id = ValidatorId(i as u8);
        }

        Ok(ValidatorSet::new(sorted_validators))
    }
}
```

### 5.12 Activating Pending Updates

```rust
impl ChainActor {
    /// Called at the start of each height to activate any pending validator set
    fn activate_pending_validator_set(&mut self, height: u64) {
        if let Some(new_set) = self.state.tendermint.pending_validator_updates.remove(&height) {
            let old_set = std::mem::replace(
                &mut self.state.tendermint.validator_set,
                Arc::new(new_set.clone())
            );

            tracing::info!(
                height = height,
                old_validators = old_set.len(),
                new_validators = new_set.len(),
                old_total_power = old_set.total_power(),
                new_total_power = new_set.total_power(),
                "Validator set activated"
            );
        }
    }
}
```

### 5.13 Block Header Fields

Each block header includes hashes for light client verification:

```rust
pub struct ConsensusBlockHeader {
    // ... other fields ...

    /// Hash of validator set for THIS block
    pub validators_hash: Hash256,

    /// Hash of validator set for NEXT block (H+1)
    /// Allows light clients to verify validator transitions
    pub next_validators_hash: Hash256,
}

impl ChainActor {
    fn compute_header_validator_hashes(&self, height: u64) -> (Hash256, Hash256) {
        let current_set = &self.state.tendermint.validator_set;
        let validators_hash = current_set.hash();

        // Check if there's a pending update for H+1
        let next_validators_hash = self.state.tendermint
            .pending_validator_updates
            .get(&(height + 1))
            .map(|set| set.hash())
            .unwrap_or(validators_hash);

        (validators_hash, next_validators_hash)
    }
}
```

### 5.14 TendermintState Updates

> **Unified Approach**: Using the full `TendermintState` from Document 17 with `GovernanceQueue` and `ChainParameters`:

```rust
pub struct TendermintState {
    // Consensus state
    pub height: u64,
    pub round: u32,
    pub step: TendermintStep,

    /// Current active validator set
    pub validator_set: Arc<ValidatorSet>,

    /// Current chain parameters (mutable via governance)
    pub chain_params: ChainParameters,

    /// Pending validator set updates: effective_height -> new_set
    /// Standard Tendermint: updates at H take effect at H+2
    pub pending_validator_updates: HashMap<Height, ValidatorSet>,

    /// Pending parameter updates: effective_height -> updates
    /// Parameters take effect at H+1
    pub pending_param_updates: HashMap<Height, Vec<ParameterUpdate>>,

    /// Queued governance updates (not yet in a block)
    /// Uses GovernanceQueue for unified handling
    pub queued_governance_updates: GovernanceQueue,

    /// Pending commit for next block's last_commit
    pub pending_commit: Option<Commit>,
}

/// Queue for all governance updates awaiting block inclusion
/// See Document 17 for full implementation
pub struct GovernanceQueue {
    pub validators: HashMap<PublicKey, ValidatorUpdate>,
    pub parameters: HashMap<GovernableParam, ParameterUpdate>,
    pub emergencies: Vec<SignedEmergencyAction>,
}
```

---

## 6. Migration from Aura

### 6.1 One-Time Migration Script

```rust
// In migration.rs

/// Migrate from Aura genesis to Tendermint genesis
pub fn migrate_genesis(
    aura_genesis: &AuraGenesisConfig,
    output_path: &Path,
) -> Result<(), MigrationError> {
    // Convert authorities to validators with equal power
    let validators: Vec<GenesisValidator> = aura_genesis.authorities.iter()
        .enumerate()
        .map(|(i, pubkey)| GenesisValidator {
            public_key: pubkey.clone(),
            voting_power: 100,  // Equal power
            name: Some(format!("Validator-{}", i + 1)),
            network_address: None,
        })
        .collect();

    let tendermint_genesis = GenesisConfig {
        chain_id: aura_genesis.chain_id,
        timestamp: aura_genesis.timestamp,
        validators,
        consensus_params: TendermintConsensusParams::default(),
        checkpoint_config: CheckpointConfig::default(),
        bridge_config: BridgeGenesisConfig::default(),
        execution_genesis: aura_genesis.execution_genesis.clone(),
    };

    let json = serde_json::to_string_pretty(&tendermint_genesis)?;
    std::fs::write(output_path, json)?;

    tracing::info!(
        validators = tendermint_genesis.validators.len(),
        output = ?output_path,
        "Migrated genesis from Aura to Tendermint format"
    );

    Ok(())
}
```

---

## 7. Testing Strategy

### 7.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_validation_minimum_validators() {
        let config = GenesisConfig {
            validators: vec![create_test_validator(100)],  // Only 1
            ..Default::default()
        };

        let result = config.validate();
        assert!(matches!(result, Err(GenesisError::InsufficientValidators { .. })));
    }

    #[test]
    fn test_genesis_validation_zero_power() {
        let config = GenesisConfig {
            validators: vec![
                create_test_validator(100),
                create_test_validator(100),
                create_test_validator(100),
                create_test_validator(0),  // Zero power!
            ],
            ..Default::default()
        };

        let result = config.validate();
        assert!(matches!(result, Err(GenesisError::ZeroVotingPower { .. })));
    }

    #[test]
    fn test_genesis_to_validator_set() {
        let config = GenesisConfig {
            validators: vec![
                create_test_validator(100),
                create_test_validator(200),
                create_test_validator(100),
                create_test_validator(100),
            ],
            ..Default::default()
        };

        let set = config.to_validator_set();

        assert_eq!(set.len(), 4);
        assert_eq!(set.total_power(), 500);
        assert_eq!(set.two_thirds_threshold(), 334);  // 500 * 2 / 3 + 1
    }

    #[test]
    fn test_genesis_proposer_selection() {
        let config = GenesisConfig {
            validators: vec![
                create_test_validator_named("Alice", 100),
                create_test_validator_named("Bob", 100),
                create_test_validator_named("Carol", 100),
                create_test_validator_named("Dave", 100),
            ],
            ..Default::default()
        };

        // Genesis proposer is validator 0
        let proposer = config.genesis_proposer();
        assert_eq!(proposer.name, Some("Alice".to_string()));
    }
}
```

---

## 8. Checklist

### Genesis
- [ ] Define `GenesisValidator` struct
- [ ] Define `TendermintConsensusParams` struct (no epoch_length)
- [ ] Define `PegInCompensation` struct (miner_fee_bps, min/max fee)
- [ ] Update `GenesisConfig` with new fields
- [ ] Implement `GenesisConfig::validate()`
- [ ] Implement `PegInCompensation::calculate_fee()`
- [ ] Implement `GenesisConfig::to_validator_set()`
- [ ] Implement `GenesisConfig::from_file()`
- [ ] Update genesis block creation with `last_commit: None`
- [ ] Create genesis commit with CommitSig array (for embedding in Block 1)
- [ ] Cache genesis commit as `pending_commit` for Block 1's last_commit
- [ ] Update `ChainActor::initialize()` for Tendermint
- [ ] Implement `load_tendermint_state()` for restart
- [ ] Store initial validator set in storage (effective_height: 0)
- [ ] Create migration script from Aura genesis
- [ ] Update genesis JSON schema documentation
- [ ] Write unit tests for genesis validation
- [ ] Write unit tests for validator set conversion
- [ ] Verify Block 1 proposal correctly embeds genesis commit in last_commit

### Validator Set Updates (Governance Client Pattern + H+2)
- [ ] Define `ValidatorUpdate` struct (public_key, power, governance_signature)
- [ ] Implement `ValidatorSet::apply_updates()` method (idempotent)
- [ ] Add `pending_validator_updates: HashMap<Height, ValidatorSet>` to state
- [ ] Implement `process_committed_validator_updates()` for H+2 scheduling
- [ ] Implement `activate_pending_validator_set()` at height start
- [ ] Add `validators_hash` and `next_validators_hash` to block header
- [ ] Update storage to use `effective_height` instead of epoch
- [ ] Write unit tests for H+2 delay activation
- [ ] Write unit tests for validator addition/removal
- [ ] Write unit tests for idempotent updates (same update applied twice)

> **Note**: The unified `GovernanceUpdate` enum, `GovernanceQueue`, gRPC service, and handler implementations are defined in `17_GOVERNANCE_PARAMETERS.md`. See that document for complete governance framework including parameter changes and emergency actions.

---

*Implementation Plan Version: 1.0*
*Last Updated: January 2026*
