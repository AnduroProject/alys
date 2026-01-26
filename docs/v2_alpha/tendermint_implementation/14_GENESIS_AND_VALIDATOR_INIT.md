# Implementation Plan: Genesis Format and Validator Initialization

## Overview

This document provides a comprehensive implementation guide for updating the genesis block format and validator set initialization for Tendermint consensus. The key changes involve embedding the initial validator set (with voting power) into genesis and removing Aura-specific authority configuration.

**Estimated Effort**: 1-2 days
**Dependencies**:
- `01_MESSAGE_TYPES_AND_PROTOCOL_FOUNDATION.md` (ValidatorSet type)
- `02_STATE_MACHINE.md` (TendermintState initialization)
**Files to Modify**:
- `app/src/actors_v2/chain/genesis.rs`
- `app/src/config.rs` or genesis configuration files
- `app/src/actors_v2/chain/actor.rs`

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

    /// Epoch length (blocks per epoch for validator set updates)
    pub epoch_length: u64,
}

impl Default for TendermintConsensusParams {
    fn default() -> Self {
        Self {
            propose_timeout_ms: 3000,    // 3 seconds
            prevote_timeout_ms: 1000,    // 1 second
            precommit_timeout_ms: 1000,  // 1 second
            timeout_delta_ms: 500,       // 500ms increase per round
            max_validators: 15,
            epoch_length: 1000,          // Validator set can change every 1000 blocks
        }
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
    "max_validators": 15,
    "epoch_length": 1000
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

```rust
// NEW: Genesis block with validator set commitment
impl ChainActor {
    /// Create genesis block with Tendermint validator set
    async fn create_genesis_block(&self) -> Result<(SignedConsensusBlock, Commit), ChainError> {
        let config = &self.genesis_config;
        let engine = self.engine_actor.as_ref()?;

        // 1. Get execution payload from EL
        let payload = engine.send(GetGenesisPayloadMessage {}).await??;

        // 2. Create consensus block with validator set info
        let block = ConsensusBlock {
            slot: 0,  // Height 0
            proposer_index: 0,  // First validator proposes genesis
            parent_root: Hash256::zero(),
            state_root: payload.state_root,
            execution_payload: payload,
            auxpow_header: None,

            // NEW: Tendermint-specific fields
            validator_set_hash: config.to_validator_set().hash(),
            next_validator_set_hash: config.to_validator_set().hash(),  // Same for genesis
        };

        // 3. Create genesis commit (special: signed by all validators offline)
        // In practice, genesis commit is created during network bootstrap
        let commit = Commit {
            height: 0,
            round: 0,
            block_hash: block.hash(),
            signers: vec![true; config.validators.len()],  // All validators
            aggregate_signature: self.create_genesis_aggregate_signature(&block)?,
        };

        // 4. Sign block (proposer signature)
        let signed_block = SignedConsensusBlock {
            message: block,
            signature: Signature::empty(),  // Genesis has no proposer signature
        };

        Ok((signed_block, commit))
    }

    /// Create aggregate signature for genesis commit
    ///
    /// This is done offline during network bootstrap:
    /// Each validator signs the genesis block hash and signatures are aggregated.
    fn create_genesis_aggregate_signature(
        &self,
        block: &ConsensusBlock,
    ) -> Result<AggregateSignature, ChainError> {
        // For automated testing, sign with our key
        // In production, this would be done during ceremony
        let signer = self.state.signer.as_ref()
            .ok_or(ChainError::NoSigner)?;

        let signing_root = compute_precommit_signing_root(0, 0, block.hash());
        let signature = signer.sign(&signing_root);

        // In production, aggregate all validator signatures
        Ok(AggregateSignature::from_single(signature))
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

            // Store block
            self.store_genesis(genesis_block.clone()).await?;

            // Store commit
            self.storage_actor.as_ref()
                .ok_or(ChainError::StorageActorNotSet)?
                .send(StoreCommitMessage {
                    height: 0,
                    commit: genesis_commit.clone(),
                    correlation_id: None,
                })
                .await??;

            // Store initial validator set
            self.storage_actor.as_ref()
                .ok_or(ChainError::StorageActorNotSet)?
                .send(StoreValidatorSetMessage {
                    epoch: 0,
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

## 5. Validator Set Updates (Future Epochs)

### 5.1 Epoch Transition

```rust
impl ChainActor {
    /// Check if we need to update validator set for new epoch
    async fn check_epoch_transition(&self, height: u64) -> Result<Option<ValidatorSet>, ChainError> {
        let epoch_length = self.state.consensus_params.epoch_length;

        // Check if height is epoch boundary
        if height % epoch_length != 0 {
            return Ok(None);
        }

        let new_epoch = height / epoch_length;

        // Get pending validator set updates (from governance/staking)
        let updates = self.get_pending_validator_updates(new_epoch).await?;

        if updates.is_empty() {
            return Ok(None);
        }

        // Apply updates to current set
        let current_set = &self.state.tendermint.validator_set;
        let new_set = current_set.apply_updates(updates)?;

        tracing::info!(
            epoch = new_epoch,
            height = height,
            validator_changes = updates.len(),
            "Epoch transition with validator set update"
        );

        Ok(Some(new_set))
    }

    /// Commit validator set for new epoch
    async fn commit_epoch_transition(
        &self,
        new_epoch: u64,
        new_validator_set: ValidatorSet,
    ) -> Result<(), ChainError> {
        // Store new validator set
        self.storage_actor.as_ref()
            .ok_or(ChainError::StorageActorNotSet)?
            .send(StoreValidatorSetMessage {
                epoch: new_epoch,
                validator_set: new_validator_set.clone(),
                correlation_id: None,
            })
            .await??;

        // Update runtime state (takes effect after delay)
        // Tendermint requires N+2 delay for validator set changes
        self.state.pending_validator_set = Some((new_epoch + 2, new_validator_set));

        Ok(())
    }
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

- [ ] Define `GenesisValidator` struct
- [ ] Define `TendermintConsensusParams` struct
- [ ] Update `GenesisConfig` with new fields
- [ ] Implement `GenesisConfig::validate()`
- [ ] Implement `GenesisConfig::to_validator_set()`
- [ ] Implement `GenesisConfig::from_file()`
- [ ] Update genesis block creation with validator set hash
- [ ] Create genesis commit with aggregate signature
- [ ] Update `ChainActor::initialize()` for Tendermint
- [ ] Implement `load_tendermint_state()` for restart
- [ ] Store validator set in storage on genesis
- [ ] Add epoch transition logic (optional for Phase 1)
- [ ] Create migration script from Aura genesis
- [ ] Update genesis JSON schema documentation
- [ ] Write unit tests for genesis validation
- [ ] Write unit tests for validator set conversion

---

*Implementation Plan Version: 1.0*
*Last Updated: January 2026*
