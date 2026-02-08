# Implementation Plan: Bridge Integration with Tendermint

## Overview

This document provides a comprehensive implementation guide for adapting the Bitcoin bridge to work with Tendermint's instant finality. The key changes involve faster peg-in confirmations (immediate with consensus) and checkpoint-based peg-out security (replacing per-block AuxPoW).

**Estimated Effort**: 3-5 days
**Dependencies**:
- `07_EL_COORDINATION.md` (Block finalization)
- `11_STORAGE_SCHEMA_MIGRATION.md` (Embedded LastCommit architecture)
- Section 4.4.2 of TENDERMINT_MIGRATION_ASSESSMENT.md (Bridge Security Integration)
- `12_RPC_ACTOR_MIGRATION.md` (Checkpoint RPC)
**Files to Modify**:
- `app/src/actors_v2/chain/withdrawals.rs`
- `app/src/bridge/` (various files)
- `app/src/actors_v2/chain/handlers.rs`

### Embedded LastCommit Architecture

Following standard Tendermint/CometBFT design, commits are embedded in blocks:

```
Block N:                           Block N+1:
├── last_commit: Commit for N-1    ├── last_commit: Commit for N  ← Proves Block N is final
├── execution_payload              │   ├── height: N
└── ...                            │   ├── signatures: [CommitSig, ...]
                                   ├── execution_payload
                                   └── ...
```

**Key Insight for Bridge**: A block at height H is finalized when Block H+1 exists, because `Block[H+1].last_commit` contains the commit proof for Block H.

---

## 1. Conceptual Change

### 1.1 Current Bridge Flow (Aura + AuxPoW)

```
PEG-IN FLOW (Bitcoin → Alys):

  Bitcoin                          Alys
     │                               │
  [BTC locked in federation]         │
     │                               │
     │─────── deposit tx ───────────►│
     │                               │
     │     [Wait for Alys block]     │
     │     [Wait for AuxPoW]         │  ← SLOW: Need AuxPoW
     │     [Wait for BTC confs]      │
     │                               │
     │◄──── EVM tokens minted ───────│


PEG-OUT FLOW (Alys → Bitcoin):

  Alys                             Bitcoin
     │                               │
  [Burn EVM tokens]                  │
     │                               │
     │     [Wait for Alys block]     │
     │     [Wait for AuxPoW]         │  ← SLOW: Need AuxPoW
     │     [Wait for BTC confs]      │
     │                               │
     │─────── release BTC ──────────►│
```

### 1.2 New Bridge Flow (Tendermint + Checkpoints)

```
PEG-IN FLOW (Bitcoin → Alys):

  Bitcoin                          Alys
     │                               │
  [BTC locked in federation]         │
     │                               │
     │─────── deposit tx ───────────►│
     │                               │
     │  [Wait for Tendermint commit] │  ← FAST: Instant finality
     │  [Wait for BTC confs]         │
     │                               │
     │◄──── EVM tokens minted ───────│  (immediate after commit)


PEG-OUT FLOW (Alys → Bitcoin):

  Alys                             Bitcoin
     │                               │
  [Burn EVM tokens]                  │
     │                               │
     │  [Tendermint commit]          │  ← FAST: Instant finality
     │  [Wait for checkpoint]        │  ← NEW: Checkpoint confirmation
     │  [Wait for BTC confs]         │
     │                               │
     │─────── release BTC ──────────►│
```

---

## 2. Peg-In Changes

### 2.1 Current Peg-In Confirmation

```rust
// CURRENT: Waits for AuxPoW before processing peg-in
impl Bridge {
    pub async fn process_pegin(&self, deposit: PegInDeposit) -> Result<(), BridgeError> {
        // 1. Verify Bitcoin transaction
        self.verify_btc_deposit(&deposit)?;

        // 2. Wait for Alys block containing deposit
        let block = self.wait_for_block_with_deposit(&deposit).await?;

        // 3. Wait for AuxPoW finalization (SLOW)
        self.wait_for_auxpow(&block).await?;

        // 4. Process peg-in
        self.mint_tokens(deposit.recipient, deposit.amount).await
    }
}
```

### 2.2 New Peg-In Confirmation (Tendermint)

**Finality Detection with Embedded LastCommit**

A block is finalized when the next block exists. This is because:
- `Block[H+1].last_commit` contains the commit proof for Block H
- Once Block H+1 is stored, Block H is irreversibly finalized

```rust
// NEW: Uses Tendermint instant finality (embedded LastCommit)
impl Bridge {
    pub async fn process_pegin(&self, deposit: PegInDeposit) -> Result<(), BridgeError> {
        // 1. Verify Bitcoin transaction
        self.verify_btc_deposit(&deposit)?;

        // 2. Wait for block containing deposit to be finalized
        // A block is finalized when the next block exists (contains commit proof)
        let (block, block_height) = self.wait_for_finalized_block(&deposit).await?;

        // 3. Process peg-in immediately (no AuxPoW wait)
        self.mint_tokens(deposit.recipient, deposit.amount).await?;

        tracing::info!(
            btc_txid = %deposit.btc_txid,
            recipient = %deposit.recipient,
            amount = deposit.amount,
            height = block_height,
            "Peg-in processed with instant finality"
        );

        Ok(())
    }

    /// Wait for block with deposit to be finalized
    ///
    /// With embedded LastCommit architecture, a block at height H is finalized
    /// when Block H+1 exists, because Block[H+1].last_commit proves Block H.
    async fn wait_for_finalized_block(
        &self,
        deposit: &PegInDeposit,
    ) -> Result<(ConsensusBlock, u64), BridgeError> {
        let storage = self.storage_actor.as_ref()
            .ok_or(BridgeError::StorageActorNotSet)?;

        loop {
            // Find block containing the deposit
            let block_result = storage.send(GetBlockForDepositMessage {
                deposit_id: deposit.id.clone(),
                correlation_id: None,
            }).await??;

            if let Some((block, height)) = block_result {
                // Check if next block exists (proves this block is finalized)
                let next_block = storage.send(GetBlockByHeightMessage {
                    height: height + 1,
                    correlation_id: None,
                }).await??;

                if let Some(next) = next_block {
                    // Verify the next block contains valid commit for this block
                    if let Some(last_commit) = &next.last_commit {
                        if last_commit.height == height && last_commit.block_hash == block.hash() {
                            return Ok((block, height));
                        }
                    }
                }
            }

            // Wait and retry
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}
```

### 2.3 Withdrawals (Peg-In Tokens)

```rust
// In chain/withdrawals.rs

impl WithdrawalCollector {
    /// Collect peg-in operations for next block
    ///
    /// With Tendermint, peg-ins are included in the next proposed block
    /// and become final when that block is committed.
    pub async fn collect_pegigns(&self) -> Result<Vec<Withdrawal>, WithdrawalError> {
        let bridge = self.bridge.read().await;

        // Get pending peg-ins that have sufficient Bitcoin confirmations
        let pending = bridge.get_confirmed_pegins().await?;

        let withdrawals: Vec<Withdrawal> = pending
            .into_iter()
            .map(|pegin| Withdrawal {
                index: pegin.index,
                validator_index: 0,  // Not used for peg-ins
                address: pegin.recipient,
                amount: pegin.amount,
            })
            .collect();

        tracing::debug!(
            count = withdrawals.len(),
            "Collected peg-in withdrawals for block"
        );

        Ok(withdrawals)
    }
}
```

---

## 3. Peg-Out Changes

### 3.1 Current Peg-Out Flow

```rust
// CURRENT: Waits for per-block AuxPoW
impl Bridge {
    pub async fn process_pegout(&self, request: PegOutRequest) -> Result<(), BridgeError> {
        // 1. Verify burn transaction on Alys
        let burn_block = self.verify_burn(&request)?;

        // 2. Wait for AuxPoW on burn block
        self.wait_for_auxpow(&burn_block).await?;

        // 3. Wait for Bitcoin confirmations on AuxPoW
        self.wait_for_btc_confirmations(&burn_block.auxpow).await?;

        // 4. Sign and broadcast Bitcoin release
        self.release_btc(request.btc_address, request.amount).await
    }
}
```

### 3.2 New Peg-Out Flow (Checkpoint-Based)

```rust
// NEW: Uses checkpoint confirmation instead of per-block AuxPoW
impl Bridge {
    pub async fn process_pegout(&self, request: PegOutRequest) -> Result<(), BridgeError> {
        // 1. Verify burn transaction is in a finalized block
        // (finalized = next block exists with commit proof in last_commit)
        let (burn_block, burn_height) = self.wait_for_finalized_burn(&request).await?;

        tracing::info!(
            burn_height = burn_height,
            btc_address = %request.btc_address,
            amount = request.amount,
            "Peg-out burn confirmed with Tendermint commit"
        );

        // 2. Wait for checkpoint covering the burn block
        // This is the NEW security requirement
        let checkpoint = self.wait_for_checkpoint(burn_height).await?;

        tracing::info!(
            burn_height = burn_height,
            checkpoint_range = format!("{}-{}", checkpoint.range_start_height, checkpoint.range_end_height),
            "Burn block covered by checkpoint"
        );

        // 3. Wait for Bitcoin confirmations on checkpoint
        self.wait_for_btc_confirmations(&checkpoint.auxpow).await?;

        // 4. Sign and broadcast Bitcoin release
        self.release_btc(request.btc_address, request.amount).await?;

        tracing::info!(
            btc_address = %request.btc_address,
            amount = request.amount,
            "Peg-out completed"
        );

        Ok(())
    }

    /// Wait for a checkpoint that covers the given height
    async fn wait_for_checkpoint(&self, height: u64) -> Result<AuxPowCheckpoint, BridgeError> {
        let storage = self.storage_actor.as_ref()
            .ok_or(BridgeError::StorageActorNotSet)?;

        let start_time = Instant::now();
        let max_wait = self.config.max_checkpoint_wait;

        loop {
            // Check for covering checkpoint
            let result = storage.send(GetCheckpointForHeightMessage {
                height,
                correlation_id: None,
            }).await??;

            match result {
                Some(checkpoint) => return Ok(checkpoint),
                None => {
                    // Check timeout
                    if start_time.elapsed() > max_wait {
                        return Err(BridgeError::CheckpointTimeout {
                            height,
                            waited: start_time.elapsed(),
                        });
                    }

                    // Log progress
                    if start_time.elapsed().as_secs() % 60 == 0 {
                        tracing::debug!(
                            height = height,
                            waited_secs = start_time.elapsed().as_secs(),
                            "Waiting for checkpoint..."
                        );
                    }

                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        }
    }
}
```

### 3.3 Latency Impact

| Scenario | Current (AuxPoW) | Tendermint + Checkpoint |
|----------|------------------|------------------------|
| Peg-in (best case) | ~6s (block) + AuxPoW | ~6s (Tendermint commit) |
| Peg-in (typical) | ~1 min | ~6s |
| Peg-out (best case) | ~6s + AuxPoW + 6 BTC blocks | ~6s + checkpoint wait + 6 BTC blocks |
| Peg-out (worst case) | Same | ~50 min checkpoint + 60 min BTC = ~2 hours |

**Important**: Peg-out latency increases because:
1. Checkpoint may cover future blocks (wait for checkpoint creation)
2. Checkpoint interval is typically 100-500 blocks

**Mitigation strategies** (configurable):
- Shorter checkpoint intervals (higher Bitcoin tx costs)
- Optimistic peg-outs for small amounts with bonds
- User notification of expected wait time

---

## 4. Configuration Changes

### 4.1 New Bridge Configuration

```rust
// In bridge/config.rs

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    // === Existing ===
    pub btc_confirmations: u32,
    pub federation_pubkeys: Vec<PublicKey>,
    pub min_peg_amount: u64,
    pub max_peg_amount: u64,

    // === Tendermint-specific (NEW) ===

    /// Maximum time to wait for checkpoint (peg-out)
    pub max_checkpoint_wait: Duration,

    /// Enable optimistic peg-outs (skip checkpoint wait for small amounts)
    pub optimistic_pegout_enabled: bool,

    /// Maximum amount for optimistic peg-out (satoshis)
    pub optimistic_pegout_max_amount: u64,

    /// Bond required for optimistic peg-out (percentage)
    pub optimistic_pegout_bond_pct: u8,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            btc_confirmations: 6,
            federation_pubkeys: vec![],
            min_peg_amount: 10_000,       // 0.0001 BTC
            max_peg_amount: 100_000_000,  // 1 BTC

            // Tendermint defaults
            max_checkpoint_wait: Duration::from_secs(2 * 60 * 60),  // 2 hours
            optimistic_pegout_enabled: false,
            optimistic_pegout_max_amount: 1_000_000,  // 0.01 BTC
            optimistic_pegout_bond_pct: 10,           // 10% bond
        }
    }
}
```

### 4.2 Checkpoint Configuration for Bridge

```rust
// In chain/config.rs

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Minimum blocks between checkpoints
    pub min_checkpoint_interval: u64,

    /// Target checkpoint interval (triggers createauxblock availability)
    pub target_checkpoint_interval: u64,

    /// Maximum blocks without checkpoint (triggers alert)
    pub max_blocks_without_checkpoint: u64,

    /// Minimum difficulty for checkpoint AuxPoW
    pub min_checkpoint_difficulty: u128,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            min_checkpoint_interval: 100,        // ~10 minutes at 6s blocks
            target_checkpoint_interval: 500,     // ~50 minutes
            max_blocks_without_checkpoint: 1000, // ~100 minutes (alert)
            min_checkpoint_difficulty: 1_000_000_000,
        }
    }
}
```

---

## 5. Fee Distribution Changes

### 5.1 Current Fee Distribution

```rust
// CURRENT: Fees distributed when block gets AuxPoW
impl FeeDistributor {
    pub fn distribute_on_auxpow(&self, block: &SignedConsensusBlock) {
        // Distribute accumulated fees when block is finalized
        let fees = self.calculate_block_fees(block);
        self.distribute_to_validators(fees);
    }
}
```

### 5.2 New Fee Distribution (Per-Commit)

```rust
// NEW: Fees distributed immediately on Tendermint commit
impl FeeDistributor {
    /// Distribute fees when block is committed
    ///
    /// With Tendermint, fees are distributed immediately when the block
    /// is committed (2/3+ precommits). No waiting for AuxPoW.
    ///
    /// Note: The commit for block N is passed separately here (before
    /// being embedded in Block N+1's last_commit field).
    pub fn distribute_on_commit(
        &self,
        block: &SignedConsensusBlock,
        commit: &Commit,  // Commit just collected, before embedding in next block
    ) -> Result<(), FeeError> {
        // 1. Calculate total fees in block
        let total_fees = self.calculate_block_fees(block)?;

        // 2. Determine proposer from commit
        let proposer = self.get_block_proposer(block)?;

        // 3. Split fees: proposer bonus + validator distribution
        let proposer_bonus = total_fees * self.config.proposer_bonus_pct / 100;
        let validator_share = total_fees - proposer_bonus;

        // 4. Distribute to proposer
        self.credit_validator(&proposer, proposer_bonus)?;

        // 5. Distribute remaining to all validators by voting power
        self.distribute_by_voting_power(validator_share)?;

        tracing::debug!(
            height = block.message.slot,
            total_fees = total_fees,
            proposer = ?proposer,
            proposer_bonus = proposer_bonus,
            "Fees distributed on commit"
        );

        Ok(())
    }

    /// Distribute fees proportionally to voting power
    fn distribute_by_voting_power(&self, amount: u64) -> Result<(), FeeError> {
        let validator_set = self.get_current_validator_set()?;
        let total_power = validator_set.total_power();

        for validator in validator_set.validators() {
            let share = amount * validator.power / total_power;
            self.credit_validator(&validator.id, share)?;
        }

        Ok(())
    }
}
```

---

## 6. Bridge State Updates

### 6.1 New Bridge State Fields

```rust
// In chain/state.rs

pub struct BridgeState {
    // === Existing ===
    pub pending_pegins: Vec<PegInDeposit>,
    pub pending_pegouts: Vec<PegOutRequest>,
    pub processed_btc_txids: HashSet<Txid>,

    // === Tendermint-specific (NEW) ===

    /// Peg-outs waiting for checkpoint confirmation
    pub pegouts_awaiting_checkpoint: HashMap<u64, Vec<PegOutRequest>>,

    /// Height of last processed checkpoint (for peg-outs)
    pub last_processed_checkpoint_height: u64,
}

impl BridgeState {
    /// Queue peg-out to wait for checkpoint
    pub fn queue_pegout_for_checkpoint(&mut self, height: u64, request: PegOutRequest) {
        self.pegouts_awaiting_checkpoint
            .entry(height)
            .or_default()
            .push(request);
    }

    /// Process peg-outs covered by new checkpoint
    pub fn process_checkpoint(&mut self, checkpoint: &AuxPowCheckpoint) -> Vec<PegOutRequest> {
        let mut ready_pegouts = Vec::new();

        // Get all peg-outs with heights covered by this checkpoint
        let heights_to_process: Vec<u64> = self.pegouts_awaiting_checkpoint
            .keys()
            .filter(|&&h| h >= checkpoint.range_start_height && h <= checkpoint.range_end_height)
            .copied()
            .collect();

        for height in heights_to_process {
            if let Some(pegouts) = self.pegouts_awaiting_checkpoint.remove(&height) {
                ready_pegouts.extend(pegouts);
            }
        }

        self.last_processed_checkpoint_height = checkpoint.range_end_height;

        ready_pegouts
    }
}
```

---

## 7. Handler Updates

### 7.1 ChainActor Handler for Checkpoint Events

```rust
// In chain/handlers.rs

impl ChainActor {
    /// Called when a new checkpoint is stored
    pub async fn on_checkpoint_stored(
        &self,
        checkpoint: AuxPowCheckpoint,
    ) -> Result<(), ChainError> {
        // 1. Notify bridge to process covered peg-outs
        let bridge = self.bridge.write().await;
        let ready_pegouts = bridge.state.process_checkpoint(&checkpoint);

        tracing::info!(
            range = format!("{}-{}", checkpoint.range_start_height, checkpoint.range_end_height),
            pegouts_ready = ready_pegouts.len(),
            "Processing peg-outs covered by checkpoint"
        );

        // 2. Queue Bitcoin releases for ready peg-outs
        for pegout in ready_pegouts {
            bridge.queue_btc_release(pegout).await?;
        }

        // 3. Update metrics
        BRIDGE_PEGOUTS_CHECKPOINT_RELEASED.add(ready_pegouts.len() as u64);

        Ok(())
    }
}
```

---

## 8. UI/API Changes for Users

### 8.1 New Status Endpoint

```rust
// RPC endpoint for peg-out status
impl GetPegoutStatusHandler {
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        let txid = params.get(0)
            .and_then(|v| v.as_str())
            .ok_or(RpcError::InvalidParams("Missing txid".to_string()))?;

        let status = chain_actor
            .send(ChainMessage::GetPegoutStatus { txid: txid.to_string() })
            .await??;

        Ok(json!({
            "txid": txid,
            "status": status.status.to_string(),
            "burn_height": status.burn_height,
            "burn_committed": status.burn_committed,
            "checkpoint_covering": status.checkpoint.map(|cp| json!({
                "range_start": cp.range_start_height,
                "range_end": cp.range_end_height,
            })),
            "btc_confirmations": status.btc_confirmations,
            "btc_release_txid": status.btc_release_txid,
            "estimated_completion": status.estimated_completion_time,
        }))
    }
}

#[derive(Debug)]
pub enum PegoutStatus {
    /// Burn transaction pending in mempool
    Pending,
    /// Burn in committed block, waiting for checkpoint
    AwaitingCheckpoint { burn_height: u64, blocks_until_checkpoint: u64 },
    /// Checkpoint exists, waiting for Bitcoin confirmations
    AwaitingBtcConfirmations { confirmations: u32, required: u32 },
    /// Bitcoin release transaction broadcast
    BtcReleasePending { btc_txid: Txid },
    /// Complete
    Complete { btc_txid: Txid },
}
```

### 8.2 User-Facing Latency Estimates

```rust
impl PegoutEstimator {
    /// Estimate time until peg-out completes
    pub fn estimate_completion(&self, burn_height: u64) -> Duration {
        let current_height = self.get_current_height();
        let checkpoint_interval = self.config.target_checkpoint_interval;

        // 1. Estimate blocks until checkpoint
        let last_checkpoint = self.get_last_checkpoint_height();
        let blocks_since_checkpoint = current_height - last_checkpoint;
        let blocks_until_checkpoint = checkpoint_interval.saturating_sub(blocks_since_checkpoint);

        // 2. Estimate time for checkpoint
        let checkpoint_wait = Duration::from_secs(blocks_until_checkpoint * 6);

        // 3. Bitcoin confirmation time (~60 minutes for 6 blocks)
        let btc_confirm_time = Duration::from_secs(60 * 60);

        // 4. Total estimate
        checkpoint_wait + btc_confirm_time
    }
}
```

---

## 9. Metrics

```rust
lazy_static! {
    /// Peg-ins processed with instant finality
    pub static ref BRIDGE_PEGINS_INSTANT: IntCounter = IntCounter::new(
        "bridge_pegins_instant_total",
        "Peg-ins processed with Tendermint instant finality"
    ).unwrap();

    /// Peg-outs waiting for checkpoint
    pub static ref BRIDGE_PEGOUTS_AWAITING_CHECKPOINT: IntGauge = IntGauge::new(
        "bridge_pegouts_awaiting_checkpoint",
        "Peg-outs waiting for checkpoint confirmation"
    ).unwrap();

    /// Peg-outs released by checkpoint
    pub static ref BRIDGE_PEGOUTS_CHECKPOINT_RELEASED: IntCounter = IntCounter::new(
        "bridge_pegouts_checkpoint_released_total",
        "Peg-outs released after checkpoint confirmation"
    ).unwrap();

    /// Average peg-out wait time
    pub static ref BRIDGE_PEGOUT_WAIT_SECONDS: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "bridge_pegout_wait_seconds",
            "Time from burn commit to BTC release"
        )
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
    async fn test_pegin_instant_finality() {
        let bridge = setup_test_bridge().await;

        // Create deposit
        let deposit = create_test_deposit(1_000_000);

        // Create and store block containing deposit at height 10
        let block_10 = create_block_with_deposit(&deposit, 10);
        bridge.on_block_stored(block_10.clone(), 10).await.unwrap();

        // Block is NOT finalized yet (no next block)
        let status = bridge.get_pegin_status(&deposit.id).await.unwrap();
        assert!(matches!(status, PeginStatus::Pending { .. }));

        // Create and store block 11 with last_commit for block 10
        let commit_for_10 = create_valid_commit(&block_10, 11);
        let block_11 = create_block_with_last_commit(11, commit_for_10);
        bridge.on_block_stored(block_11, 11).await.unwrap();

        // Now block 10 is finalized - peg-in should be processed
        let status = bridge.get_pegin_status(&deposit.id).await.unwrap();
        assert!(matches!(status, PeginStatus::Minted { .. }));
    }

    #[tokio::test]
    async fn test_pegout_waits_for_checkpoint() {
        let bridge = setup_test_bridge().await;

        // Create and commit burn
        let request = create_test_pegout(500_000);
        let burn_height = 100;
        bridge.on_burn_committed(burn_height, &request).await.unwrap();

        // Should be awaiting checkpoint
        let status = bridge.get_pegout_status(&request.id).await.unwrap();
        assert!(matches!(status, PegoutStatus::AwaitingCheckpoint { .. }));

        // Create checkpoint covering burn
        let checkpoint = create_checkpoint(1, 150);
        bridge.on_checkpoint_stored(checkpoint).await.unwrap();

        // Should now be awaiting BTC confirmations
        let status = bridge.get_pegout_status(&request.id).await.unwrap();
        assert!(matches!(status, PegoutStatus::AwaitingBtcConfirmations { .. }));
    }
}
```

---

## 11. Checklist

- [ ] Modify peg-in flow to use embedded LastCommit finality detection
- [ ] Implement `wait_for_finalized_block` (checks next block exists with valid last_commit)
- [ ] Remove AuxPoW wait from peg-in processing
- [ ] Add checkpoint wait to peg-out flow
- [ ] Implement `wait_for_checkpoint` method
- [ ] Add `pegouts_awaiting_checkpoint` to BridgeState
- [ ] Implement `process_checkpoint` for releasing peg-outs
- [ ] Update fee distribution to per-commit
- [ ] Add `GetPegoutStatus` RPC endpoint
- [ ] Add latency estimation for users
- [ ] Update BridgeConfig with checkpoint settings
- [ ] Add optimistic peg-out option (optional)
- [ ] Update bridge metrics
- [ ] Write unit tests for instant peg-in (verify finality via next block's last_commit)
- [ ] Write unit tests for checkpoint peg-out
- [ ] Update user documentation with new latency expectations

---

*Implementation Plan Version: 1.0*
*Last Updated: January 2026*
