# Implementation Plan: Governance Parameters

## Overview

This document provides a comprehensive implementation guide for governance-controlled parameter changes in Alys V2. Chain parameters (peg-in fees, bridge config, consensus timeouts, etc.) can be modified by the federation via the Governance Client gRPC stream. All changes are included in blocks for auditability and verification by late-joining validators and light clients.

**Estimated Effort**: 1-2 weeks
**Dependencies**:
- `14_GENESIS_AND_VALIDATOR_INIT.md` (Governance Client stream, ValidatorUpdate)
- `11_STORAGE_SCHEMA_MIGRATION.md` (Parameter history storage)
- `04_CHAINACTOR_HANDLERS.md` (Handler integration)
- `15_VALIDATION_MODULE.md` (Governance validation)
- `16_AUXPOW_TENDERMINT_INTEGRATION.md` (Block structure, AuxPowHeader)
**Files to Modify**:
- `app/src/actors_v2/chain/tendermint/governance.rs` (NEW)
- `app/src/actors_v2/chain/tendermint/types.rs`
- `app/src/actors_v2/chain/actor.rs`
- `app/src/actors_v2/storage/schema.rs`
- `app/src/block.rs`

---

## 1. Motivation

### Why Governable Parameters?

Chain parameters set at genesis may need adjustment over time:

| Scenario | Parameter Change |
|----------|------------------|
| Market conditions change | Adjust `miner_fee_bps` for peg-in compensation |
| Security posture update | Increase `btc_confirmations` requirement |
| Network growth | Adjust `max_validators` limit |
| Emergency response | Pause peg-ins during exploit |
| Performance tuning | Adjust consensus timeouts |

### Requirements

1. **Auditability**: All changes recorded in blocks
2. **Verifiability**: Late-joiners can reconstruct parameter history
3. **Determinism**: All nodes apply changes at same height
4. **Safety**: Validation prevents invalid parameter values
5. **Flexibility**: Support different activation delays per update type

---

## 2. Governable Parameters (Exhaustive List)

### 2.1 Peg-In Compensation

| Parameter | Type | Default | Constraints | Description |
|-----------|------|---------|-------------|-------------|
| `miner_fee_bps` | u64 | 50 | 0-10000 | Miner fee (basis points, 50 = 0.5%) |
| `min_fee_satoshi` | u64 | 1000 | > 0 | Minimum fee floor |
| `max_fee_satoshi` | u64 | 10000000 | > min_fee | Maximum fee cap |

### 2.2 Bridge Configuration

| Parameter | Type | Default | Constraints | Description |
|-----------|------|---------|-------------|-------------|
| `btc_confirmations` | u32 | 6 | 1-100 | Required Bitcoin confirmations |
| `min_peg_amount` | u64 | 10000 | > 0 | Minimum peg-in/out (satoshis) |
| `max_peg_amount` | u64 | 100000000 | > min | Maximum peg-in/out (satoshis) |
| `federation_threshold` | u32 | 11 | > n/2 | Required federation signatures |
| `federation_members` | Vec<PubKey> | genesis | len >= threshold | Federation multisig participants |

### 2.3 AuxPoW Configuration

**Note**: With the simplified AuxPoW model (see Document 16), checkpoint intervals, difficulty thresholds, and liveness gates have been removed. AuxPoW is optional per-block with no governance parameters.

### 2.4 Consensus Parameters

| Parameter | Type | Default | Constraints | Description |
|-----------|------|---------|-------------|-------------|
| `propose_timeout_ms` | u64 | 3000 | 100-60000 | Proposal timeout |
| `prevote_timeout_ms` | u64 | 1000 | 100-60000 | Prevote timeout |
| `precommit_timeout_ms` | u64 | 1000 | 100-60000 | Precommit timeout |
| `timeout_delta_ms` | u64 | 500 | 0-10000 | Timeout increase per round |
| `max_validators` | u32 | 15 | 4-100 | Maximum validator count |

### 2.5 Emergency Controls

| Parameter | Type | Default | Constraints | Description |
|-----------|------|---------|-------------|-------------|
| `chain_paused` | bool | false | - | Emergency chain halt |
| `pegins_paused` | bool | false | - | Pause peg-in processing |
| `pegouts_paused` | bool | false | - | Pause peg-out processing |

### 2.6 Fee Schedule

| Parameter | Type | Default | Constraints | Description |
|-----------|------|---------|-------------|-------------|
| `base_fee_floor` | u64 | 1 gwei | > 0 | Minimum EVM base fee |
| `base_fee_ceiling` | u64 | 1000 gwei | > floor | Maximum EVM base fee |

---

## 3. Core Types

### 3.1 GovernanceUpdate Enum

```rust
/// Unified type for all governance-controlled changes
/// Included in blocks for auditability and late-joiner verification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GovernanceUpdate {
    /// Validator set changes (add/remove/change power)
    /// Activation: H+2
    Validator(ValidatorUpdate),

    /// Chain parameter changes
    /// Activation: H+1
    Parameter(ParameterUpdate),

    /// Emergency actions (pause/resume)
    /// Activation: Immediate (H+0)
    /// Note: Uses SignedEmergencyAction to preserve signature for auditability
    Emergency(SignedEmergencyAction),
}

impl GovernanceUpdate {
    /// Get the activation delay (in blocks) for this update type
    pub fn activation_delay(&self) -> u64 {
        match self {
            GovernanceUpdate::Validator(_) => 2,   // H+2 (standard Tendermint)
            GovernanceUpdate::Parameter(_) => 1,   // H+1 (propagation delay)
            GovernanceUpdate::Emergency(_) => 0,   // Immediate
        }
    }

    /// Get the effective height for this update when included at `inclusion_height`
    pub fn effective_height(&self, inclusion_height: u64) -> u64 {
        inclusion_height + self.activation_delay()
    }
}
```

### 3.2 GovernableParam Enum

```rust
/// Enumeration of all governable parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum GovernableParam {
    // Peg-in compensation (100-199)
    MinerFeeBps = 100,
    MinFeeSatoshi = 101,
    MaxFeeSatoshi = 102,

    // Bridge config (200-299)
    BtcConfirmations = 200,
    MinPegAmount = 201,
    MaxPegAmount = 202,
    FederationThreshold = 203,
    FederationMembers = 204,

    // NOTE: Checkpoint config (300-399) removed - see Document 16 for simplified AuxPoW model

    // Consensus params (400-499)
    ProposeTimeoutMs = 400,
    PrevoteTimeoutMs = 401,
    PrecommitTimeoutMs = 402,
    TimeoutDeltaMs = 403,
    MaxValidators = 404,

    // Fee schedule (500-599)
    BaseFeeFloor = 500,
    BaseFeeCeiling = 501,
}

impl GovernableParam {
    /// Convert to bytes for storage key
    pub fn to_bytes(&self) -> [u8; 2] {
        (*self as u16).to_be_bytes()
    }

    /// Parse from bytes
    pub fn from_bytes(bytes: [u8; 2]) -> Option<Self> {
        let value = u16::from_be_bytes(bytes);
        Self::try_from(value).ok()
    }

    /// Iterate over all governable parameters
    /// Used for parameter reconstruction during sync
    pub fn all() -> impl Iterator<Item = Self> {
        [
            // Peg-in compensation
            Self::MinerFeeBps,
            Self::MinFeeSatoshi,
            Self::MaxFeeSatoshi,
            // Bridge config
            Self::BtcConfirmations,
            Self::MinPegAmount,
            Self::MaxPegAmount,
            Self::FederationThreshold,
            Self::FederationMembers,
            // Consensus params
            Self::ProposeTimeoutMs,
            Self::PrevoteTimeoutMs,
            Self::PrecommitTimeoutMs,
            Self::TimeoutDeltaMs,
            Self::MaxValidators,
            // Fee schedule
            Self::BaseFeeFloor,
            Self::BaseFeeCeiling,
        ].into_iter()
    }
}
```

### 3.3 ParameterUpdate Struct

```rust
/// A parameter update from the Governance Client
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParameterUpdate {
    /// Which parameter to update
    pub param: GovernableParam,

    /// New value (serialized as bytes)
    pub value: Vec<u8>,

    /// Signature from governance authority
    pub governance_signature: Signature,
}

impl ParameterUpdate {
    /// Create a new parameter update
    pub fn new<T: Serialize>(
        param: GovernableParam,
        value: &T,
        signature: Signature,
    ) -> Result<Self, SerializeError> {
        Ok(Self {
            param,
            value: bincode::serialize(value)?,
            governance_signature: signature,
        })
    }

    /// Decode the value as a specific type
    pub fn decode_value<T: DeserializeOwned>(&self) -> Result<T, DeserializeError> {
        bincode::deserialize(&self.value)
    }
}
```

### 3.4 EmergencyAction Enum

```rust
/// Emergency actions that take effect immediately
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmergencyAction {
    /// Halt the chain (no new blocks)
    PauseChain,
    /// Resume chain operation
    ResumeChain,
    /// Pause peg-in processing
    PausePegIns,
    /// Resume peg-in processing
    ResumePegIns,
    /// Pause peg-out processing
    PausePegOuts,
    /// Resume peg-out processing
    ResumePegOuts,
}

/// Emergency action with governance signature
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedEmergencyAction {
    pub action: EmergencyAction,
    pub governance_signature: Signature,
}
```

### 3.5 ValidatorUpdate (Updated)

```rust
/// Validator set update (unchanged from Document 14)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ValidatorUpdate {
    /// Validator's BLS public key
    pub public_key: PublicKey,
    /// New voting power (0 = remove)
    pub power: u64,
    /// Signature from governance authority
    pub governance_signature: Signature,
}
```

### 3.6 Supporting Structs

```rust
/// Tendermint consensus timing parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TendermintConsensusParams {
    /// Proposal timeout in milliseconds (100-60000)
    pub propose_timeout_ms: u64,
    /// Prevote timeout in milliseconds (100-60000)
    pub prevote_timeout_ms: u64,
    /// Precommit timeout in milliseconds (100-60000)
    pub precommit_timeout_ms: u64,
    /// Timeout increase per round in milliseconds (0-10000)
    pub timeout_delta_ms: u64,
    /// Maximum number of validators (4-100)
    pub max_validators: u32,
}

impl Default for TendermintConsensusParams {
    fn default() -> Self {
        Self {
            propose_timeout_ms: 3000,
            prevote_timeout_ms: 1000,
            precommit_timeout_ms: 1000,
            timeout_delta_ms: 500,
            max_validators: 15,
        }
    }
}

/// EVM fee schedule parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeeSchedule {
    /// Minimum EVM base fee (> 0)
    pub base_fee_floor: u64,
    /// Maximum EVM base fee (> floor)
    pub base_fee_ceiling: u64,
}

impl Default for FeeSchedule {
    fn default() -> Self {
        Self {
            base_fee_floor: 1_000_000_000,      // 1 gwei
            base_fee_ceiling: 1_000_000_000_000, // 1000 gwei
        }
    }
}

/// Bridge configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeConfig {
    /// Required Bitcoin confirmations (1-100)
    pub btc_confirmations: u32,
    /// Minimum peg-in/out amount in satoshis
    pub min_peg_amount: u64,
    /// Maximum peg-in/out amount in satoshis
    pub max_peg_amount: u64,
    /// Required federation signatures (> n/2)
    pub federation_threshold: u32,
    /// Federation multisig participants
    pub federation_members: Vec<PublicKey>,
}

/// Peg-in compensation for miners (from Document 16)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PegInCompensation {
    /// Percentage of peg-in amount paid to miner (basis points, 0-10000)
    pub miner_fee_bps: u64,
    /// Minimum fee in satoshis (floor for small peg-ins)
    pub min_fee_satoshi: u64,
    /// Maximum fee in satoshis (cap for large peg-ins)
    pub max_fee_satoshi: u64,
}

impl Default for PegInCompensation {
    fn default() -> Self {
        Self {
            miner_fee_bps: 50,           // 0.5%
            min_fee_satoshi: 1000,       // 0.00001 BTC
            max_fee_satoshi: 10_000_000, // 0.1 BTC
        }
    }
}
```

---

## 4. Block Structure

### 4.1 Updated ConsensusBlock

```rust
pub struct ConsensusBlock<T: EthSpec> {
    pub parent_hash: Hash256,
    pub slot: u64,
    pub proposer_index: u64,
    pub state_root: Hash256,

    /// Commit proof for previous block (None for genesis)
    pub last_commit: Option<Commit>,

    /// EVM execution payload
    pub execution_payload: ExecutionPayloadCapella<T>,

    /// Optional AuxPoW with peg-ins (see Document 16)
    pub auxpow: Option<AuxPowHeader>,

    /// Governance updates: validator changes, parameter changes, emergency actions
    /// Optional - most blocks have None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governance_updates: Option<Vec<GovernanceUpdate>>,

    /// Hash of current validator set
    pub validators_hash: Hash256,

    /// Hash of next block's validator set (for light clients)
    pub next_validators_hash: Hash256,

    /// Hash of current chain parameters (for light clients)
    pub params_hash: Hash256,
}
```

### 4.2 Params Hash Computation

```rust
impl ChainParameters {
    /// Compute deterministic hash of all parameters
    /// Used in block headers for light client verification
    pub fn hash(&self) -> Hash256 {
        let mut hasher = Sha256::new();

        // Peg-in compensation
        hasher.update(self.pegin_compensation.miner_fee_bps.to_be_bytes());
        hasher.update(self.pegin_compensation.min_fee_satoshi.to_be_bytes());
        hasher.update(self.pegin_compensation.max_fee_satoshi.to_be_bytes());

        // Bridge config
        hasher.update(self.bridge_config.btc_confirmations.to_be_bytes());
        hasher.update(self.bridge_config.min_peg_amount.to_be_bytes());
        hasher.update(self.bridge_config.max_peg_amount.to_be_bytes());
        hasher.update(self.bridge_config.federation_threshold.to_be_bytes());
        // Federation members: hash the count and each member's bytes
        hasher.update((self.bridge_config.federation_members.len() as u32).to_be_bytes());
        for member in &self.bridge_config.federation_members {
            hasher.update(member.as_bytes());
        }

        // Consensus params
        hasher.update(self.consensus_params.propose_timeout_ms.to_be_bytes());
        hasher.update(self.consensus_params.prevote_timeout_ms.to_be_bytes());
        hasher.update(self.consensus_params.precommit_timeout_ms.to_be_bytes());
        hasher.update(self.consensus_params.timeout_delta_ms.to_be_bytes());
        hasher.update(self.consensus_params.max_validators.to_be_bytes());

        // Fee schedule
        hasher.update(self.fee_schedule.base_fee_floor.to_be_bytes());
        hasher.update(self.fee_schedule.base_fee_ceiling.to_be_bytes());

        // Emergency flags
        hasher.update([self.chain_paused as u8]);
        hasher.update([self.pegins_paused as u8]);
        hasher.update([self.pegouts_paused as u8]);

        Hash256::from_slice(&hasher.finalize())
    }
}
```

---

## 5. gRPC Service Definition

### 5.1 Updated Proto

```protobuf
syntax = "proto3";

package alys.governance.v1;

// Unified governance stream for all update types
service GovernanceService {
    // Bi-directional stream for governance updates
    rpc GovernanceStream(stream GovernanceRequest)
        returns (stream GovernanceResponse);
}

// Request wrapper for all governance update types
message GovernanceRequest {
    oneof update {
        ValidatorUpdateRequest validator = 1;
        ParameterUpdateRequest parameter = 2;
        EmergencyActionRequest emergency = 3;
    }
}

// Validator update (existing)
message ValidatorUpdateRequest {
    bytes public_key = 1;
    uint64 power = 2;
    bytes governance_signature = 3;
}

// Parameter update (new)
message ParameterUpdateRequest {
    GovernableParam param = 1;
    bytes value = 2;
    bytes governance_signature = 3;
}

// Emergency action (new)
message EmergencyActionRequest {
    EmergencyAction action = 1;
    bytes governance_signature = 2;
}

// Response for any governance update
message GovernanceResponse {
    UpdateStatus status = 1;
    uint64 included_height = 2;    // Height where update was included
    uint64 effective_height = 3;   // Height where update takes effect
    string error = 4;              // Error message if rejected
}

// All governable parameters
enum GovernableParam {
    PARAM_UNSPECIFIED = 0;

    // Peg-in compensation
    PARAM_MINER_FEE_BPS = 100;
    PARAM_MIN_FEE_SATOSHI = 101;
    PARAM_MAX_FEE_SATOSHI = 102;

    // Bridge config
    PARAM_BTC_CONFIRMATIONS = 200;
    PARAM_MIN_PEG_AMOUNT = 201;
    PARAM_MAX_PEG_AMOUNT = 202;
    PARAM_FEDERATION_THRESHOLD = 203;
    PARAM_FEDERATION_MEMBERS = 204;

    // NOTE: Checkpoint config (300-399) removed - simplified AuxPoW model

    // Consensus params
    PARAM_PROPOSE_TIMEOUT_MS = 400;
    PARAM_PREVOTE_TIMEOUT_MS = 401;
    PARAM_PRECOMMIT_TIMEOUT_MS = 402;
    PARAM_TIMEOUT_DELTA_MS = 403;
    PARAM_MAX_VALIDATORS = 404;

    // Fee schedule
    PARAM_BASE_FEE_FLOOR = 500;
    PARAM_BASE_FEE_CEILING = 501;
}

// Emergency actions
enum EmergencyAction {
    ACTION_UNSPECIFIED = 0;
    ACTION_PAUSE_CHAIN = 1;
    ACTION_RESUME_CHAIN = 2;
    ACTION_PAUSE_PEGINS = 3;
    ACTION_RESUME_PEGINS = 4;
    ACTION_PAUSE_PEGOUTS = 5;
    ACTION_RESUME_PEGOUTS = 6;
}

enum UpdateStatus {
    STATUS_UNSPECIFIED = 0;
    STATUS_QUEUED = 1;
    STATUS_INCLUDED = 2;
    STATUS_ACTIVATED = 3;
    STATUS_REJECTED = 4;
}
```

---

## 6. State Management

### 6.1 ChainParameters Struct

```rust
/// All mutable chain parameters (initialized from genesis, updated via governance)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainParameters {
    /// Peg-in compensation for miners
    pub pegin_compensation: PegInCompensation,

    /// Bridge configuration
    pub bridge_config: BridgeConfig,

    // NOTE: checkpoint_config removed - see Document 16 for simplified AuxPoW model

    /// Consensus parameters
    pub consensus_params: TendermintConsensusParams,

    /// Fee schedule
    pub fee_schedule: FeeSchedule,

    /// Emergency flags
    pub chain_paused: bool,
    pub pegins_paused: bool,
    pub pegouts_paused: bool,
}

impl ChainParameters {
    /// Initialize from genesis config
    pub fn from_genesis(genesis: &GenesisConfig) -> Self {
        Self {
            pegin_compensation: genesis.pegin_compensation.clone(),
            bridge_config: genesis.bridge_config.clone(),
            // NOTE: checkpoint_config removed - see Document 16
            consensus_params: genesis.consensus_params.clone(),
            fee_schedule: FeeSchedule::default(),
            chain_paused: false,
            pegins_paused: false,
            pegouts_paused: false,
        }
    }

    /// Apply a parameter update
    pub fn apply_update(&mut self, update: &ParameterUpdate) -> Result<(), ChainError> {
        match update.param {
            // Peg-in compensation
            GovernableParam::MinerFeeBps => {
                self.pegin_compensation.miner_fee_bps = update.decode_value()?;
            }
            GovernableParam::MinFeeSatoshi => {
                self.pegin_compensation.min_fee_satoshi = update.decode_value()?;
            }
            GovernableParam::MaxFeeSatoshi => {
                self.pegin_compensation.max_fee_satoshi = update.decode_value()?;
            }

            // Bridge config
            GovernableParam::BtcConfirmations => {
                self.bridge_config.btc_confirmations = update.decode_value()?;
            }
            GovernableParam::MinPegAmount => {
                self.bridge_config.min_peg_amount = update.decode_value()?;
            }
            GovernableParam::MaxPegAmount => {
                self.bridge_config.max_peg_amount = update.decode_value()?;
            }
            GovernableParam::FederationThreshold => {
                self.bridge_config.federation_threshold = update.decode_value()?;
            }
            GovernableParam::FederationMembers => {
                self.bridge_config.federation_members = update.decode_value()?;
            }

            // NOTE: Checkpoint config cases removed - see Document 16

            // Consensus params
            GovernableParam::ProposeTimeoutMs => {
                self.consensus_params.propose_timeout_ms = update.decode_value()?;
            }
            GovernableParam::PrevoteTimeoutMs => {
                self.consensus_params.prevote_timeout_ms = update.decode_value()?;
            }
            GovernableParam::PrecommitTimeoutMs => {
                self.consensus_params.precommit_timeout_ms = update.decode_value()?;
            }
            GovernableParam::TimeoutDeltaMs => {
                self.consensus_params.timeout_delta_ms = update.decode_value()?;
            }
            GovernableParam::MaxValidators => {
                self.consensus_params.max_validators = update.decode_value()?;
            }

            // Fee schedule
            GovernableParam::BaseFeeFloor => {
                self.fee_schedule.base_fee_floor = update.decode_value()?;
            }
            GovernableParam::BaseFeeCeiling => {
                self.fee_schedule.base_fee_ceiling = update.decode_value()?;
            }
        }

        Ok(())
    }

    /// Apply an emergency action (immediate effect)
    pub fn apply_emergency(&mut self, action: &EmergencyAction) {
        match action {
            EmergencyAction::PauseChain => self.chain_paused = true,
            EmergencyAction::ResumeChain => self.chain_paused = false,
            EmergencyAction::PausePegIns => self.pegins_paused = true,
            EmergencyAction::ResumePegIns => self.pegins_paused = false,
            EmergencyAction::PausePegOuts => self.pegouts_paused = true,
            EmergencyAction::ResumePegOuts => self.pegouts_paused = false,
        }
    }
}
```

### 6.2 Updated TendermintState

```rust
pub struct TendermintState {
    // Consensus state
    pub height: u64,
    pub round: u32,
    pub step: TendermintStep,

    /// Current active validator set
    pub validator_set: Arc<ValidatorSet>,

    /// Current chain parameters
    pub chain_params: ChainParameters,

    /// Pending validator updates: effective_height -> new_set
    pub pending_validator_updates: HashMap<Height, ValidatorSet>,

    /// Pending parameter updates: effective_height -> updates
    pub pending_param_updates: HashMap<Height, Vec<ParameterUpdate>>,

    /// Queued governance updates (not yet in a block)
    /// Keyed by update type for easy lookup
    pub queued_governance_updates: GovernanceQueue,

    /// Pending commit for next block's last_commit
    pub pending_commit: Option<Commit>,

    // ... other fields
}

/// Queue for governance updates awaiting inclusion in a block
#[derive(Debug, Default)]
pub struct GovernanceQueue {
    /// Validator updates keyed by public key (idempotent)
    pub validators: HashMap<PublicKey, ValidatorUpdate>,

    /// Parameter updates keyed by param (latest wins)
    pub parameters: HashMap<GovernableParam, ParameterUpdate>,

    /// Emergency actions (processed immediately, but still included in block)
    pub emergencies: Vec<SignedEmergencyAction>,
}

impl GovernanceQueue {
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
            && self.parameters.is_empty()
            && self.emergencies.is_empty()
    }

    /// Drain all queued updates for block inclusion
    pub fn drain(&mut self) -> Vec<GovernanceUpdate> {
        let mut updates = Vec::new();

        // Validator updates
        updates.extend(
            self.validators.drain().map(|(_, v)| GovernanceUpdate::Validator(v))
        );

        // Parameter updates
        updates.extend(
            self.parameters.drain().map(|(_, p)| GovernanceUpdate::Parameter(p))
        );

        // Emergency actions (preserve full SignedEmergencyAction for auditability)
        updates.extend(
            self.emergencies.drain(..).map(|e| GovernanceUpdate::Emergency(e))
        );

        updates
    }
}
```

---

## 7. Handler Implementation

### 7.1 Governance Update Handler

```rust
impl ChainActor {
    /// Handle incoming governance update from gRPC stream
    pub async fn handle_governance_update(
        &mut self,
        update: GovernanceUpdate,
    ) -> Result<GovernanceResponse, ChainError> {
        match update {
            GovernanceUpdate::Validator(vu) => {
                self.handle_validator_update(vu).await
            }
            GovernanceUpdate::Parameter(pu) => {
                self.handle_parameter_update(pu).await
            }
            GovernanceUpdate::Emergency(ea) => {
                self.handle_emergency_action(ea).await
            }
        }
    }

    /// Handle validator update (existing, from Document 14)
    async fn handle_validator_update(
        &mut self,
        update: ValidatorUpdate,
    ) -> Result<GovernanceResponse, ChainError> {
        // Verify governance signature
        self.verify_governance_signature_validator(&update)?;

        // Validate the update
        self.validate_validator_update(&update)?;

        // Queue for inclusion (keyed by public_key, idempotent)
        let public_key = update.public_key.clone();
        self.state.tendermint.queued_governance_updates.validators
            .insert(public_key, update);

        Ok(GovernanceResponse {
            status: UpdateStatus::Queued,
            ..Default::default()
        })
    }

    /// Handle parameter update
    async fn handle_parameter_update(
        &mut self,
        update: ParameterUpdate,
    ) -> Result<GovernanceResponse, ChainError> {
        // Verify governance signature
        self.verify_governance_signature_param(&update)?;

        // Validate the parameter value
        self.validate_parameter_update(&update)?;

        // Queue for inclusion (keyed by param, latest wins)
        let param = update.param;
        self.state.tendermint.queued_governance_updates.parameters
            .insert(param, update);

        tracing::info!(
            param = ?param,
            "Parameter update queued from governance client"
        );

        Ok(GovernanceResponse {
            status: UpdateStatus::Queued,
            ..Default::default()
        })
    }

    /// Handle emergency action (immediate effect + block inclusion)
    async fn handle_emergency_action(
        &mut self,
        action: SignedEmergencyAction,
    ) -> Result<GovernanceResponse, ChainError> {
        // Verify governance signature
        self.verify_governance_signature_emergency(&action)?;

        // Apply immediately (H+0)
        self.state.tendermint.chain_params.apply_emergency(&action.action);

        // Also queue for block inclusion (auditability)
        self.state.tendermint.queued_governance_updates.emergencies
            .push(action.clone());

        tracing::warn!(
            action = ?action.action,
            "Emergency action applied immediately"
        );

        Ok(GovernanceResponse {
            status: UpdateStatus::Activated,
            included_height: 0,  // Will be set when included in block
            effective_height: self.state.tendermint.height,  // Immediate
            ..Default::default()
        })
    }
}
```

### 7.2 Parameter Validation

```rust
impl ChainActor {
    /// Validate a parameter update value
    fn validate_parameter_update(&self, update: &ParameterUpdate) -> Result<(), ChainError> {
        match update.param {
            // Peg-in compensation
            GovernableParam::MinerFeeBps => {
                let value: u64 = update.decode_value()?;
                if value > 10_000 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "miner_fee_bps cannot exceed 10000 (100%)".to_string(),
                    });
                }
            }
            GovernableParam::MinFeeSatoshi => {
                let value: u64 = update.decode_value()?;
                if value == 0 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "min_fee_satoshi must be > 0".to_string(),
                    });
                }
            }
            GovernableParam::MaxFeeSatoshi => {
                let value: u64 = update.decode_value()?;
                let min = self.state.tendermint.chain_params.pegin_compensation.min_fee_satoshi;
                if value <= min {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: format!("max_fee_satoshi must be > min_fee_satoshi ({})", min),
                    });
                }
            }

            // Bridge config
            GovernableParam::BtcConfirmations => {
                let value: u32 = update.decode_value()?;
                if value < 1 || value > 100 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "btc_confirmations must be 1-100".to_string(),
                    });
                }
            }
            GovernableParam::MinPegAmount => {
                let value: u64 = update.decode_value()?;
                if value == 0 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "min_peg_amount must be > 0".to_string(),
                    });
                }
            }
            GovernableParam::MaxPegAmount => {
                let value: u64 = update.decode_value()?;
                let min = self.state.tendermint.chain_params.bridge_config.min_peg_amount;
                if value <= min {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: format!("max_peg_amount must be > min_peg_amount ({})", min),
                    });
                }
            }
            GovernableParam::FederationThreshold => {
                let value: u32 = update.decode_value()?;
                let members = self.state.tendermint.chain_params.bridge_config.federation_members.len() as u32;
                if value > members || value <= members / 2 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: format!("federation_threshold must be > n/2 and <= n ({})", members),
                    });
                }
            }
            GovernableParam::FederationMembers => {
                let value: Vec<PublicKey> = update.decode_value()?;
                let threshold = self.state.tendermint.chain_params.bridge_config.federation_threshold;
                if value.len() < threshold as usize {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: format!("federation_members count ({}) must be >= threshold ({})", value.len(), threshold),
                    });
                }
                if value.is_empty() {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "federation_members cannot be empty".to_string(),
                    });
                }
            }

            // Consensus params
            GovernableParam::ProposeTimeoutMs |
            GovernableParam::PrevoteTimeoutMs |
            GovernableParam::PrecommitTimeoutMs => {
                let value: u64 = update.decode_value()?;
                if value < 100 || value > 60_000 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "timeout must be 100-60000 ms".to_string(),
                    });
                }
            }
            GovernableParam::MaxValidators => {
                let value: u32 = update.decode_value()?;
                if value < 4 || value > 100 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "max_validators must be 4-100".to_string(),
                    });
                }
                // Also check current validator count
                let current = self.state.tendermint.validator_set.len() as u32;
                if value < current {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: format!("max_validators ({}) cannot be less than current count ({})", value, current),
                    });
                }
            }
            GovernableParam::TimeoutDeltaMs => {
                let value: u64 = update.decode_value()?;
                if value > 10_000 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "timeout_delta_ms must be 0-10000".to_string(),
                    });
                }
            }

            // Fee schedule
            GovernableParam::BaseFeeFloor => {
                let value: u64 = update.decode_value()?;
                if value == 0 {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: "base_fee_floor must be > 0".to_string(),
                    });
                }
            }
            GovernableParam::BaseFeeCeiling => {
                let value: u64 = update.decode_value()?;
                let floor = self.state.tendermint.chain_params.fee_schedule.base_fee_floor;
                if value <= floor {
                    return Err(ChainError::InvalidParameterValue {
                        param: update.param,
                        reason: format!("base_fee_ceiling must be > base_fee_floor ({})", floor),
                    });
                }
            }
        }

        Ok(())
    }
}
```

### 7.3 Proposal Building

```rust
impl ChainActor {
    /// Build a block proposal including governance updates
    async fn build_proposal(&mut self, height: u64) -> Result<ConsensusBlock, ChainError> {
        // ... build execution payload ...

        // Collect governance updates (if any)
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

        // Compute hashes
        let validators_hash = self.state.tendermint.validator_set.hash();
        let next_validators_hash = self.compute_next_validators_hash(height);
        let params_hash = self.state.tendermint.chain_params.hash();

        Ok(ConsensusBlock {
            parent_hash,
            slot: height,
            last_commit: self.state.tendermint.pending_commit.take(),
            execution_payload,
            auxpow: self.state.queued_pow.take(),  // Optional AuxPowHeader
            governance_updates,
            validators_hash,
            next_validators_hash,
            params_hash,
            // ...
        })
    }
}
```

### 7.4 Proposal Validation

```rust
impl ChainActor {
    /// Validate governance updates in a received proposal
    fn verify_proposal_governance_updates(
        &self,
        proposal: &ConsensusBlock,
    ) -> Result<(), ChainError> {
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
                    // Emergency actions already applied by proposer
                    // Just verify signature
                    self.verify_governance_signature_emergency_action(ea)?;
                }
            }
        }

        Ok(())
    }
}
```

### 7.5 Commit Processing

```rust
impl ChainActor {
    /// Process governance updates after block commit
    async fn process_committed_governance_updates(
        &mut self,
        height: u64,
        updates: &Option<Vec<GovernanceUpdate>>,
    ) -> Result<(), ChainError> {
        let Some(updates) = updates else {
            return Ok(());
        };

        for update in updates {
            match update {
                GovernanceUpdate::Validator(vu) => {
                    // Schedule for H+2 (existing logic)
                    self.schedule_validator_update(height, vu).await?;
                }
                GovernanceUpdate::Parameter(pu) => {
                    // Schedule for H+1
                    self.schedule_parameter_update(height, pu).await?;
                }
                GovernanceUpdate::Emergency(sea) => {
                    // Already applied immediately, log for auditability
                    // Signature preserved in block for verification
                    tracing::info!(
                        height = height,
                        action = ?sea.action,
                        "Emergency action recorded in block (signature preserved)"
                    );
                }
            }
        }

        Ok(())
    }

    /// Schedule a parameter update for activation
    async fn schedule_parameter_update(
        &mut self,
        height: u64,
        update: &ParameterUpdate,
    ) -> Result<(), ChainError> {
        let effective_height = height + 1;  // H+1

        // Store in pending updates
        self.state.tendermint.pending_param_updates
            .entry(effective_height)
            .or_default()
            .push(update.clone());

        // Store in persistent storage for late-joiners
        self.storage_actor.as_ref()
            .ok_or(ChainError::StorageActorNotSet)?
            .send(StoreParameterUpdateMessage {
                param: update.param,
                effective_height,
                value: update.value.clone(),
                correlation_id: None,
            })
            .await??;

        tracing::info!(
            param = ?update.param,
            height = height,
            effective_height = effective_height,
            "Parameter update scheduled"
        );

        Ok(())
    }
}
```

### 7.6 Activation at Height Start

```rust
impl ChainActor {
    /// Called at the start of each height to activate pending updates
    fn activate_pending_updates(&mut self, height: u64) {
        // Activate validator updates (H+2)
        self.activate_pending_validator_set(height);

        // Activate parameter updates (H+1)
        self.activate_pending_param_updates(height);
    }

    /// Activate pending parameter updates for this height
    fn activate_pending_param_updates(&mut self, height: u64) {
        if let Some(updates) = self.state.tendermint.pending_param_updates.remove(&height) {
            for update in updates {
                if let Err(e) = self.state.tendermint.chain_params.apply_update(&update) {
                    // This shouldn't happen if validation was correct
                    tracing::error!(
                        param = ?update.param,
                        error = %e,
                        "Failed to apply parameter update"
                    );
                } else {
                    tracing::info!(
                        param = ?update.param,
                        height = height,
                        "Parameter update activated"
                    );
                }
            }
        }
    }
}
```

---

## 8. Storage Schema

### 8.1 New Column Family

```rust
/// Column family for parameter change history
/// Enables late-joiners to reconstruct parameter state at any height
pub const CF_PARAMETER_HISTORY: &str = "parameter_history";

// Key format: [param_id (2 bytes)][effective_height (8 bytes BE)]
// Value: serialized parameter value
```

### 8.2 Storage Messages

```rust
/// Store a parameter update
pub struct StoreParameterUpdateMessage {
    pub param: GovernableParam,
    pub effective_height: u64,
    pub value: Vec<u8>,
    pub correlation_id: Option<CorrelationId>,
}

/// Get parameter value at a specific height
pub struct GetParameterAtHeightMessage {
    pub param: GovernableParam,
    pub height: u64,
    pub correlation_id: Option<CorrelationId>,
}

/// Get all parameter changes in a height range
pub struct GetParameterHistoryMessage {
    pub param: GovernableParam,
    pub start_height: u64,
    pub end_height: u64,
    pub correlation_id: Option<CorrelationId>,
}
```

### 8.3 Storage Implementation

```rust
impl StorageActor {
    /// Store a parameter update
    fn handle_store_parameter_update(
        &self,
        msg: StoreParameterUpdateMessage,
    ) -> Result<(), StorageError> {
        let cf = self.db.cf_handle(CF_PARAMETER_HISTORY)?;

        // Key: [param (2 bytes)][height (8 bytes BE)]
        let mut key = Vec::with_capacity(10);
        key.extend_from_slice(&msg.param.to_bytes());
        key.extend_from_slice(&msg.effective_height.to_be_bytes());

        self.db.put_cf(cf, &key, &msg.value)?;

        Ok(())
    }

    /// Get parameter value at a specific height
    /// Returns the most recent value with effective_height <= requested height
    fn handle_get_parameter_at_height(
        &self,
        msg: GetParameterAtHeightMessage,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let cf = self.db.cf_handle(CF_PARAMETER_HISTORY)?;

        // Prefix for this parameter
        let prefix = msg.param.to_bytes();

        // Iterate from requested height backwards
        let start_key = {
            let mut k = Vec::with_capacity(10);
            k.extend_from_slice(&prefix);
            k.extend_from_slice(&msg.height.to_be_bytes());
            k
        };

        let mut iter = self.db.iterator_cf(
            cf,
            IteratorMode::From(&start_key, Direction::Reverse)
        );

        while let Some(Ok((key, value))) = iter.next() {
            // Check prefix matches
            if key.len() < 2 || key[..2] != prefix {
                break;
            }

            // Extract height from key
            if key.len() >= 10 {
                let height_bytes: [u8; 8] = key[2..10].try_into().unwrap();
                let effective_height = u64::from_be_bytes(height_bytes);

                if effective_height <= msg.height {
                    return Ok(Some(value.to_vec()));
                }
            }
        }

        // No update found - use genesis default
        Ok(None)
    }
}
```

---

## 9. Late-Joiner Verification

### 9.1 Parameter State Reconstruction

```rust
impl ChainActor {
    /// Reconstruct chain parameters at a specific height
    /// Used by late-joining validators during sync
    async fn reconstruct_params_at_height(
        &self,
        height: u64,
    ) -> Result<ChainParameters, ChainError> {
        let storage = self.storage_actor.as_ref()
            .ok_or(ChainError::StorageActorNotSet)?;

        // Start with genesis parameters
        let mut params = ChainParameters::from_genesis(&self.genesis_config);

        // Apply all parameter updates up to this height
        for param in GovernableParam::all() {
            let response = storage.send(GetParameterAtHeightMessage {
                param,
                height,
                correlation_id: None,
            }).await??;

            if let Some(value) = response {
                let update = ParameterUpdate {
                    param,
                    value,
                    governance_signature: Signature::empty(),  // Not needed for reconstruction
                };
                params.apply_update(&update)?;
            }
        }

        Ok(params)
    }
}
```

### 9.2 Light Client Verification

```rust
/// Light client can verify parameter changes by checking:
/// 1. Block includes the governance update
/// 2. Update has valid governance signature
/// 3. params_hash in subsequent blocks reflects the change
pub fn verify_parameter_change(
    blocks: &[BlockHeader],
    update: &ParameterUpdate,
    governance_pubkey: &PublicKey,
) -> Result<(), VerifyError> {
    // 1. Verify governance signature
    let signing_root = compute_param_update_signing_root(update);
    if !governance_pubkey.verify(&signing_root, &update.governance_signature) {
        return Err(VerifyError::InvalidSignature);
    }

    // 2. Find the block containing this update
    let inclusion_block = blocks.iter()
        .find(|b| b.governance_updates.as_ref()
            .map(|updates| updates.contains(&GovernanceUpdate::Parameter(update.clone())))
            .unwrap_or(false))
        .ok_or(VerifyError::UpdateNotInChain)?;

    // 3. Verify params_hash changed in H+1
    let effective_height = inclusion_block.slot + 1;
    let effective_block = blocks.iter()
        .find(|b| b.slot == effective_height)
        .ok_or(VerifyError::MissingBlock)?;

    // The params_hash should reflect the new parameter value
    // (Light client would need to compute expected hash)

    Ok(())
}
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_update_validation() {
        let actor = create_test_chain_actor();

        // Valid miner_fee_bps
        let update = ParameterUpdate::new(
            GovernableParam::MinerFeeBps,
            &50u64,
            test_signature(),
        ).unwrap();
        assert!(actor.validate_parameter_update(&update).is_ok());

        // Invalid: > 100%
        let update = ParameterUpdate::new(
            GovernableParam::MinerFeeBps,
            &15000u64,
            test_signature(),
        ).unwrap();
        assert!(actor.validate_parameter_update(&update).is_err());
    }

    #[test]
    fn test_parameter_activation_h_plus_1() {
        let mut actor = create_test_chain_actor();
        let height = 100;

        // Queue parameter update
        let update = ParameterUpdate::new(
            GovernableParam::BtcConfirmations,
            &12u32,
            test_signature(),
        ).unwrap();

        // Include in block at height 100
        actor.schedule_parameter_update(height, &update).unwrap();

        // Not yet active at height 100
        assert_eq!(actor.state.tendermint.chain_params.bridge_config.btc_confirmations, 6);

        // Activate at height 101
        actor.activate_pending_param_updates(height + 1);
        assert_eq!(actor.state.tendermint.chain_params.bridge_config.btc_confirmations, 12);
    }

    #[test]
    fn test_emergency_action_immediate() {
        let mut actor = create_test_chain_actor();

        assert!(!actor.state.tendermint.chain_params.pegins_paused);

        // Emergency action takes effect immediately
        let action = SignedEmergencyAction {
            action: EmergencyAction::PausePegIns,
            governance_signature: test_signature(),
        };
        actor.handle_emergency_action(action).unwrap();

        assert!(actor.state.tendermint.chain_params.pegins_paused);
    }

    #[test]
    fn test_governance_queue_latest_wins() {
        let mut queue = GovernanceQueue::default();

        // First update
        let update1 = ParameterUpdate::new(
            GovernableParam::MinerFeeBps,
            &50u64,
            test_signature(),
        ).unwrap();
        queue.parameters.insert(GovernableParam::MinerFeeBps, update1);

        // Second update for same param
        let update2 = ParameterUpdate::new(
            GovernableParam::MinerFeeBps,
            &100u64,
            test_signature(),
        ).unwrap();
        queue.parameters.insert(GovernableParam::MinerFeeBps, update2);

        // Drain should return only the latest
        let updates = queue.drain();
        assert_eq!(updates.len(), 1);

        if let GovernanceUpdate::Parameter(pu) = &updates[0] {
            let value: u64 = pu.decode_value().unwrap();
            assert_eq!(value, 100);  // Latest wins
        } else {
            panic!("Expected parameter update");
        }
    }
}
```

### 10.2 Integration Tests

```rust
#[tokio::test]
async fn test_parameter_change_e2e() {
    let mut testnet = create_4_validator_testnet().await;

    // 1. Send parameter update via governance stream
    testnet.governance_client.send(ParameterUpdateRequest {
        param: GovernableParam::MinerFeeBps,
        value: encode(&100u64),
        governance_signature: sign_with_governance_key(&100u64),
    }).await.unwrap();

    // 2. Wait for block to be produced
    let block = testnet.wait_for_block().await;

    // 3. Verify update is in block
    assert!(block.governance_updates.is_some());

    // 4. Wait for H+1
    let next_block = testnet.wait_for_block().await;

    // 5. Verify parameter is now active
    let params = testnet.validators[0].get_chain_params().await;
    assert_eq!(params.pegin_compensation.miner_fee_bps, 100);
}

#[tokio::test]
async fn test_late_joiner_param_reconstruction() {
    let mut testnet = create_4_validator_testnet().await;

    // 1. Make some parameter changes
    for bps in [50, 100, 150] {
        testnet.governance_client.send_param_update(
            GovernableParam::MinerFeeBps,
            &bps,
        ).await.unwrap();
        testnet.wait_for_blocks(2).await;  // Wait for activation
    }

    // 2. Start a new validator (late joiner)
    let late_joiner = testnet.add_syncing_validator().await;

    // 3. Wait for sync to complete
    late_joiner.wait_for_sync().await;

    // 4. Verify parameters match
    let expected = testnet.validators[0].get_chain_params().await;
    let actual = late_joiner.get_chain_params().await;
    assert_eq!(expected, actual);
}
```

---

## 11. Checklist

### Types & Messages
- [ ] Define `GovernanceUpdate` enum (uses `SignedEmergencyAction` for auditability)
- [ ] Define `GovernableParam` enum with all parameters
- [ ] Implement `GovernableParam::all()` iterator
- [ ] Define `ParameterUpdate` struct
- [ ] Define `EmergencyAction` enum
- [ ] Define `SignedEmergencyAction` struct
- [ ] Update gRPC proto with new message types
- [ ] Implement `GovernanceUpdate::activation_delay()`

### Supporting Structs
- [ ] Define `TendermintConsensusParams` struct with defaults
- [ ] Define `FeeSchedule` struct with defaults
- [ ] Define `BridgeConfig` struct
- [ ] Define `PegInCompensation` struct with defaults

### State Management
- [ ] Define `ChainParameters` struct
- [ ] Implement `ChainParameters::from_genesis()`
- [ ] Implement `ChainParameters::apply_update()` for all params
- [ ] Implement `ChainParameters::apply_emergency()`
- [ ] Implement `ChainParameters::hash()` (include all param categories)
- [ ] Define `GovernanceQueue` struct
- [ ] Update `TendermintState` with new fields

### Handlers
- [ ] Implement `handle_governance_update()` dispatcher
- [ ] Implement `handle_parameter_update()`
- [ ] Implement `handle_emergency_action()`
- [ ] Implement `validate_parameter_update()` for ALL params:
  - [ ] MinerFeeBps (0-10000)
  - [ ] MinFeeSatoshi (> 0)
  - [ ] MaxFeeSatoshi (> min)
  - [ ] BtcConfirmations (1-100)
  - [ ] MinPegAmount (> 0)
  - [ ] MaxPegAmount (> min)
  - [ ] FederationThreshold (> n/2, <= n)
  - [ ] FederationMembers (len >= threshold)
  - [ ] ProposeTimeoutMs (100-60000)
  - [ ] PrevoteTimeoutMs (100-60000)
  - [ ] PrecommitTimeoutMs (100-60000)
  - [ ] TimeoutDeltaMs (0-10000)
  - [ ] MaxValidators (4-100, >= current)
  - [ ] BaseFeeFloor (> 0)
  - [ ] BaseFeeCeiling (> floor)
- [ ] Implement `verify_governance_signature_param()`
- [ ] Implement `verify_governance_signature_emergency()`

### Block Processing
- [ ] Update `ConsensusBlock` with `governance_updates` field
- [ ] Add `params_hash` to block header
- [ ] Update `build_proposal()` to include governance updates
- [ ] Implement `verify_proposal_governance_updates()`
- [ ] Implement `process_committed_governance_updates()`
- [ ] Implement `schedule_parameter_update()`
- [ ] Implement `activate_pending_param_updates()`

### Storage
- [ ] Add `CF_PARAMETER_HISTORY` column family
- [ ] Implement `StoreParameterUpdateMessage` handler
- [ ] Implement `GetParameterAtHeightMessage` handler
- [ ] Implement `GetParameterHistoryMessage` handler

### Late-Joiner Support
- [ ] Implement `reconstruct_params_at_height()`
- [ ] Add parameter reconstruction to sync flow

### Testing
- [ ] Unit tests for parameter validation
- [ ] Unit tests for activation timing (H+1)
- [ ] Unit tests for emergency immediate activation
- [ ] Unit tests for governance queue (latest wins)
- [ ] Integration tests for parameter change E2E
- [ ] Integration tests for late-joiner reconstruction

---

*Implementation Plan Version: 1.2*
*Last Updated: February 2026*

---

### Changelog

**v1.2** (February 2026):
- Added dependencies on Document 15 (Validation) and Document 16 (AuxPoW)
- Fixed block structure: `auxpow_checkpoint` → `auxpow: Option<AuxPowHeader>` (per Doc 16)
- Added `GovernableParam::all()` iterator for parameter reconstruction
- Added supporting struct definitions: `TendermintConsensusParams`, `FeeSchedule`, `BridgeConfig`, `PegInCompensation`
- Completed `params_hash` computation (added consensus_params, fee_schedule, federation_members)
- Added missing parameter validation: MinPegAmount, MaxPegAmount, FederationMembers, TimeoutDeltaMs, BaseFeeFloor, BaseFeeCeiling
- Fixed `GovernanceUpdate::Emergency` to use `SignedEmergencyAction` (preserves signature for auditability)
- Updated `GovernanceQueue::drain()` to preserve emergency signatures
- Expanded checklist with supporting structs and detailed validation requirements

**v1.1** (February 2026):
- Removed checkpoint configuration from governable parameters (see Document 16 for simplified AuxPoW model)
- Removed `CheckpointConfig` from `ChainParameters` struct
- Removed checkpoint-related `GovernableParam` enum variants
- Removed checkpoint cases from `apply_update()` function
- Updated protobuf enum to remove checkpoint params
