# ALYS-009: Implement BridgeActor

## Issue Type
Task

## Priority
Critical

## Story Points
10

## Sprint
Migration Sprint 2-3

## Component
Core Architecture

## Labels
`migration`, `phase-1`, `actor-system`, `bridge`, `peg-operations`

## Description

Implement the BridgeActor to handle all peg-in and peg-out operations using the actor model. This actor manages Bitcoin transaction building, coordinates with governance for signatures, processes bridge contract events, and tracks peg operation state without shared mutable state.

## Acceptance Criteria

- [ ] BridgeActor handles all peg operations
- [ ] Message protocol for peg-in/peg-out flows
- [ ] Bitcoin transaction building (unsigned)
- [ ] Integration with StreamActor for governance
- [ ] Event processing from bridge contract
- [ ] UTXO management implemented
- [ ] Operation state tracking with persistence
- [ ] Retry logic for failed operations
- [ ] No key material stored locally

## Technical Details

### Implementation Steps

1. **Define BridgeActor Messages**
```rust
// src/actors/bridge/messages.rs

use actix::prelude::*;
use bitcoin::{Transaction, Txid, Address as BtcAddress};
use ethereum_types::{H256, H160};

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ProcessPegin {
    pub tx: Transaction,
    pub confirmations: u32,
    pub deposit_address: BtcAddress,
}

#[derive(Message)]
#[rtype(result = "Result<PegoutResult, BridgeError>")]
pub struct ProcessPegout {
    pub burn_event: BurnEvent,
    pub request_id: String,
}

#[derive(Message)]
#[rtype(result = "Result<Vec<PendingPegin>, BridgeError>")]
pub struct GetPendingPegins;

#[derive(Message)]
#[rtype(result = "Result<Vec<PendingPegout>, BridgeError>")]
pub struct GetPendingPegouts;

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct ApplySignatures {
    pub request_id: String,
    pub witnesses: Vec<WitnessData>,
}

#[derive(Message)]
#[rtype(result = "Result<OperationStatus, BridgeError>")]
pub struct GetOperationStatus {
    pub operation_id: String,
}

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct UpdateFederationAddress {
    pub version: u32,
    pub address: BtcAddress,
    pub script_pubkey: Script,
}

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct RetryFailedOperations;

#[derive(Debug, Clone)]
pub struct BurnEvent {
    pub tx_hash: H256,
    pub block_number: u64,
    pub amount: u64,
    pub destination: String,  // Bitcoin address
    pub sender: H160,
}

#[derive(Debug, Clone)]
pub struct PendingPegin {
    pub txid: Txid,
    pub amount: u64,
    pub evm_address: H160,
    pub confirmations: u32,
    pub index: u64,
}

#[derive(Debug, Clone)]
pub struct PendingPegout {
    pub request_id: String,
    pub amount: u64,
    pub destination: BtcAddress,
    pub burn_tx_hash: H256,
    pub state: PegoutState,
}

#[derive(Debug, Clone)]
pub enum PegoutState {
    Pending,
    BuildingTransaction,
    SignatureRequested,
    SignaturesReceived { count: usize },
    Broadcasting,
    Broadcast { txid: Txid },
    Confirmed { confirmations: u32 },
    Failed { reason: String, retry_count: u32 },
}

#[derive(Debug, Clone)]
pub enum PegoutResult {
    Pending(String),  // Request ID
    InProgress(PegoutState),
    Completed(Txid),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct WitnessData {
    pub input_index: usize,
    pub witness: Vec<Vec<u8>>,
}
```

2. **Implement BridgeActor Core**
```rust
// src/actors/bridge/mod.rs

use actix::prelude::*;
use bitcoin::{
    Transaction, TxIn, TxOut, Script, Witness,
    util::psbt::serialize::Serialize,
};
use std::collections::HashMap;

pub struct BridgeActor {
    // Bitcoin operations
    bitcoin_core: Arc<BitcoinCore>,
    utxo_manager: UtxoManager,
    tx_builder: TransactionBuilder,
    
    // Governance communication
    stream_actor: Addr<StreamActor>,
    
    // Operation tracking
    pending_pegins: HashMap<Txid, PendingPegin>,
    pending_pegouts: HashMap<String, PendingPegout>,
    operation_history: OperationHistory,
    
    // Federation info
    federation_address: BtcAddress,
    federation_script: Script,
    federation_version: u32,
    
    // Configuration
    config: BridgeConfig,
    
    // Metrics
    metrics: BridgeMetrics,
}

#[derive(Clone)]
pub struct BridgeConfig {
    pub bitcoin_rpc: String,
    pub min_confirmations: u32,
    pub max_pegout_amount: u64,
    pub batch_pegouts: bool,
    pub batch_threshold: usize,
    pub retry_delay: Duration,
    pub max_retries: u32,
}

impl BridgeActor {
    pub fn new(
        config: BridgeConfig,
        stream_actor: Addr<StreamActor>,
        bitcoin_core: Arc<BitcoinCore>,
    ) -> Result<Self> {
        let utxo_manager = UtxoManager::new(bitcoin_core.clone());
        let tx_builder = TransactionBuilder::new();
        
        Ok(Self {
            bitcoin_core,
            utxo_manager,
            tx_builder,
            stream_actor,
            pending_pegins: HashMap::new(),
            pending_pegouts: HashMap::new(),
            operation_history: OperationHistory::new(),
            federation_address: config.initial_federation_address.clone(),
            federation_script: config.initial_federation_script.clone(),
            federation_version: 1,
            config,
            metrics: BridgeMetrics::new(),
        })
    }
}

impl Actor for BridgeActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("BridgeActor started");
        
        // Start Bitcoin monitoring
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            ctx.spawn(
                async move {
                    act.scan_for_pegins().await
                }
                .into_actor(act)
            );
        });
        
        // Start retry timer for failed operations
        ctx.run_interval(Duration::from_secs(60), |act, ctx| {
            ctx.spawn(
                async move {
                    act.retry_failed_operations().await
                }
                .into_actor(act)
            );
        });
        
        // Start UTXO refresh
        ctx.run_interval(Duration::from_secs(120), |act, ctx| {
            ctx.spawn(
                async move {
                    act.refresh_utxos().await
                }
                .into_actor(act)
            );
        });
    }
}

impl Handler<ProcessPegin> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<(), BridgeError>>;
    
    fn handle(&mut self, msg: ProcessPegin, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let start = Instant::now();
            self.metrics.pegin_attempts.inc();
            
            // Validate transaction
            if msg.confirmations < self.config.min_confirmations {
                return Err(BridgeError::InsufficientConfirmations);
            }
            
            // Check if already processed
            if self.operation_history.contains_pegin(&msg.tx.txid()) {
                return Ok(()); // Already processed
            }
            
            // Extract deposit details
            let deposit_details = self.extract_deposit_details(&msg.tx)?;
            
            // Validate deposit address matches federation
            if deposit_details.address != self.federation_address {
                return Err(BridgeError::InvalidDepositAddress);
            }
            
            // Extract EVM address from OP_RETURN
            let evm_address = self.extract_evm_address(&msg.tx)?;
            
            // Create pending peg-in
            let pending = PendingPegin {
                txid: msg.tx.txid(),
                amount: deposit_details.amount,
                evm_address,
                confirmations: msg.confirmations,
                index: self.pending_pegins.len() as u64,
            };
            
            // Store pending peg-in
            self.pending_pegins.insert(msg.tx.txid(), pending.clone());
            
            // Notify governance (informational)
            self.stream_actor.send(NotifyPegin {
                txid: msg.tx.txid(),
                amount: deposit_details.amount,
                evm_address,
            }).await?;
            
            // Record in history
            self.operation_history.record_pegin(
                msg.tx.txid(),
                deposit_details.amount,
                evm_address,
            );
            
            self.metrics.pegins_processed.inc();
            self.metrics.pegin_processing_time.observe(start.elapsed().as_secs_f64());
            
            info!("Processed peg-in: {} BTC to {}", 
                deposit_details.amount as f64 / 100_000_000.0,
                evm_address
            );
            
            Ok(())
        }.into_actor(self))
    }
}

impl Handler<ProcessPegout> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<PegoutResult, BridgeError>>;
    
    fn handle(&mut self, msg: ProcessPegout, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            let start = Instant::now();
            self.metrics.pegout_attempts.inc();
            
            // Validate amount
            if msg.burn_event.amount > self.config.max_pegout_amount {
                return Err(BridgeError::AmountTooLarge);
            }
            
            // Check if already processing
            if self.pending_pegouts.contains_key(&msg.request_id) {
                let state = self.pending_pegouts[&msg.request_id].state.clone();
                return Ok(PegoutResult::InProgress(state));
            }
            
            // Parse Bitcoin address
            let btc_address = BtcAddress::from_str(&msg.burn_event.destination)
                .map_err(|e| BridgeError::InvalidAddress(e.to_string()))?;
            
            // Create pending peg-out
            let mut pending = PendingPegout {
                request_id: msg.request_id.clone(),
                amount: msg.burn_event.amount,
                destination: btc_address.clone(),
                burn_tx_hash: msg.burn_event.tx_hash,
                state: PegoutState::BuildingTransaction,
            };
            
            // Build unsigned transaction
            let unsigned_tx = self.build_pegout_transaction(
                btc_address,
                msg.burn_event.amount,
            ).await?;
            
            // Get input amounts for signing
            let input_amounts = self.get_input_amounts(&unsigned_tx).await?;
            
            // Request signatures from governance
            let sig_request = SignatureRequest {
                request_id: msg.request_id.clone(),
                tx_hex: hex::encode(serialize(&unsigned_tx)),
                input_indices: (0..unsigned_tx.input.len()).collect(),
                amounts: input_amounts,
            };
            
            self.stream_actor.send(RequestSignatures(sig_request)).await??;
            
            pending.state = PegoutState::SignatureRequested;
            self.pending_pegouts.insert(msg.request_id.clone(), pending);
            
            self.metrics.pegout_processing_time.observe(start.elapsed().as_secs_f64());
            
            info!("Initiated peg-out: {} BTC to {}", 
                msg.burn_event.amount as f64 / 100_000_000.0,
                msg.burn_event.destination
            );
            
            Ok(PegoutResult::Pending(msg.request_id))
        }.into_actor(self))
    }
}

impl Handler<ApplySignatures> for BridgeActor {
    type Result = ResponseActFuture<Self, Result<(), BridgeError>>;
    
    fn handle(&mut self, msg: ApplySignatures, _: &mut Context<Self>) -> Self::Result {
        Box::pin(async move {
            // Get pending peg-out
            let pending = self.pending_pegouts.get_mut(&msg.request_id)
                .ok_or(BridgeError::OperationNotFound)?;
            
            // Get the unsigned transaction
            let mut tx = self.get_unsigned_transaction(&msg.request_id).await?;
            
            // Apply witness data
            for witness_data in msg.witnesses {
                if witness_data.input_index >= tx.input.len() {
                    return Err(BridgeError::InvalidWitnessIndex);
                }
                
                tx.input[witness_data.input_index].witness = Witness::from_vec(
                    witness_data.witness
                );
            }
            
            // Update state
            pending.state = PegoutState::Broadcasting;
            
            // Broadcast transaction
            let txid = self.bitcoin_core.send_raw_transaction(&tx).await
                .map_err(|e| {
                    pending.state = PegoutState::Failed {
                        reason: e.to_string(),
                        retry_count: 0,
                    };
                    BridgeError::BroadcastFailed(e.to_string())
                })?;
            
            pending.state = PegoutState::Broadcast { txid };
            
            // Record in history
            self.operation_history.record_pegout(
                msg.request_id.clone(),
                pending.amount,
                pending.destination.clone(),
                txid,
            );
            
            self.metrics.pegouts_broadcast.inc();
            
            info!("Broadcast peg-out transaction: {}", txid);
            
            Ok(())
        }.into_actor(self))
    }
}

impl BridgeActor {
    async fn build_pegout_transaction(
        &mut self,
        destination: BtcAddress,
        amount: u64,
    ) -> Result<Transaction, BridgeError> {
        // Get available UTXOs
        let utxos = self.utxo_manager.get_spendable_utxos().await?;
        
        // Select UTXOs for transaction
        let (selected_utxos, total_input) = self.select_utxos(&utxos, amount)?;
        
        // Calculate fee
        let fee = self.calculate_fee(selected_utxos.len(), 2); // 2 outputs typically
        
        if total_input < amount + fee {
            return Err(BridgeError::InsufficientFunds);
        }
        
        // Build transaction
        let mut tx = Transaction {
            version: 2,
            lock_time: 0,
            input: vec![],
            output: vec![],
        };
        
        // Add inputs
        for utxo in selected_utxos {
            tx.input.push(TxIn {
                previous_output: utxo.outpoint,
                script_sig: Script::new(), // Will be signed by governance
                sequence: 0xfffffffd, // Enable RBF
                witness: Witness::new(), // Will be filled by governance
            });
        }
        
        // Add peg-out output
        tx.output.push(TxOut {
            value: amount,
            script_pubkey: destination.script_pubkey(),
        });
        
        // Add change output if needed
        let change = total_input - amount - fee;
        if change > DUST_LIMIT {
            tx.output.push(TxOut {
                value: change,
                script_pubkey: self.federation_script.clone(),
            });
        }
        
        Ok(tx)
    }
    
    async fn scan_for_pegins(&mut self) -> Result<(), BridgeError> {
        // Get recent transactions to federation address
        let transactions = self.bitcoin_core
            .list_transactions(&self.federation_address, 100)
            .await?;
        
        for tx_info in transactions {
            if tx_info.confirmations >= self.config.min_confirmations {
                // Process as peg-in
                let tx = self.bitcoin_core.get_transaction(&tx_info.txid).await?;
                
                self.handle(ProcessPegin {
                    tx,
                    confirmations: tx_info.confirmations,
                    deposit_address: self.federation_address.clone(),
                }, ctx).await?;
            }
        }
        
        Ok(())
    }
    
    async fn retry_failed_operations(&mut self) -> Result<(), BridgeError> {
        let failed_ops: Vec<_> = self.pending_pegouts
            .iter()
            .filter_map(|(id, op)| {
                if let PegoutState::Failed { retry_count, .. } = &op.state {
                    if *retry_count < self.config.max_retries {
                        Some(id.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        
        for request_id in failed_ops {
            info!("Retrying failed peg-out: {}", request_id);
            
            if let Some(pending) = self.pending_pegouts.get_mut(&request_id) {
                if let PegoutState::Failed { retry_count, .. } = &mut pending.state {
                    *retry_count += 1;
                    
                    // Rebuild and resubmit
                    let burn_event = self.operation_history
                        .get_burn_event(&pending.burn_tx_hash)?;
                    
                    self.handle(ProcessPegout {
                        burn_event,
                        request_id: request_id.clone(),
                    }, ctx).await?;
                }
            }
        }
        
        Ok(())
    }
    
    fn extract_evm_address(&self, tx: &Transaction) -> Result<H160, BridgeError> {
        // Look for OP_RETURN output with EVM address
        for output in &tx.output {
            if output.script_pubkey.is_op_return() {
                let data = output.script_pubkey.as_bytes();
                if data.len() >= 22 && data[0] == 0x6a && data[1] == 0x14 {
                    // OP_RETURN with 20 bytes (EVM address)
                    let address_bytes = &data[2..22];
                    return Ok(H160::from_slice(address_bytes));
                }
            }
        }
        
        Err(BridgeError::NoEvmAddress)
    }
}
```

3. **Implement UTXO Management**
```rust
// src/actors/bridge/utxo.rs

use bitcoin::{OutPoint, TxOut};

pub struct UtxoManager {
    bitcoin_core: Arc<BitcoinCore>,
    utxo_set: HashMap<OutPoint, Utxo>,
    spent_utxos: HashSet<OutPoint>,
    last_refresh: Instant,
}

#[derive(Debug, Clone)]
pub struct Utxo {
    pub outpoint: OutPoint,
    pub output: TxOut,
    pub confirmations: u32,
    pub spendable: bool,
}

impl UtxoManager {
    pub async fn get_spendable_utxos(&mut self) -> Result<Vec<Utxo>, BridgeError> {
        // Refresh if stale
        if self.last_refresh.elapsed() > Duration::from_secs(60) {
            self.refresh().await?;
        }
        
        Ok(self.utxo_set
            .values()
            .filter(|utxo| utxo.spendable && !self.spent_utxos.contains(&utxo.outpoint))
            .cloned()
            .collect())
    }
    
    pub async fn refresh(&mut self) -> Result<(), BridgeError> {
        let unspent = self.bitcoin_core.list_unspent(
            Some(6), // Min confirmations
            None,    // Max confirmations
            Some(&[self.federation_address.clone()]),
        ).await?;
        
        self.utxo_set.clear();
        
        for unspent_output in unspent {
            let outpoint = OutPoint {
                txid: unspent_output.txid,
                vout: unspent_output.vout,
            };
            
            let utxo = Utxo {
                outpoint,
                output: TxOut {
                    value: unspent_output.amount.as_sat(),
                    script_pubkey: unspent_output.script_pub_key,
                },
                confirmations: unspent_output.confirmations,
                spendable: unspent_output.spendable,
            };
            
            self.utxo_set.insert(outpoint, utxo);
        }
        
        self.last_refresh = Instant::now();
        Ok(())
    }
    
    pub fn mark_spent(&mut self, outpoint: OutPoint) {
        self.spent_utxos.insert(outpoint);
    }
}
```

## Testing Plan

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[actix::test]
    async fn test_pegin_processing() {
        let bridge = create_test_bridge_actor().await;
        
        let tx = create_deposit_transaction(
            100_000_000, // 1 BTC
            "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb7", // EVM address
        );
        
        bridge.send(ProcessPegin {
            tx,
            confirmations: 6,
            deposit_address: test_federation_address(),
        }).await.unwrap().unwrap();
        
        let pending = bridge.send(GetPendingPegins).await.unwrap().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].amount, 100_000_000);
    }
    
    #[actix::test]
    async fn test_pegout_flow() {
        let bridge = create_test_bridge_actor().await;
        
        let burn_event = BurnEvent {
            tx_hash: H256::random(),
            block_number: 1000,
            amount: 50_000_000, // 0.5 BTC
            destination: "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string(),
            sender: H160::random(),
        };
        
        let result = bridge.send(ProcessPegout {
            burn_event,
            request_id: "test-pegout-1".to_string(),
        }).await.unwrap().unwrap();
        
        assert!(matches!(result, PegoutResult::Pending(_)));
    }
    
    #[actix::test]
    async fn test_signature_application() {
        let bridge = create_test_bridge_actor().await;
        
        // Setup pending pegout
        setup_pending_pegout(&bridge, "test-1").await;
        
        // Apply signatures
        let witnesses = vec![
            WitnessData {
                input_index: 0,
                witness: vec![/* witness data */],
            }
        ];
        
        bridge.send(ApplySignatures {
            request_id: "test-1".to_string(),
            witnesses,
        }).await.unwrap().unwrap();
        
        // Check state
        let status = bridge.send(GetOperationStatus {
            operation_id: "test-1".to_string(),
        }).await.unwrap().unwrap();
        
        assert!(matches!(status.state, PegoutState::Broadcast { .. }));
    }
}
```

### Integration Tests
1. Test with Bitcoin regtest
2. Test UTXO selection algorithms
3. Test federation address updates
4. Test batch peg-out processing
5. Test failure recovery

### Performance Tests
```rust
#[bench]
fn bench_transaction_building(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bridge = runtime.block_on(create_test_bridge_actor());
    
    b.iter(|| {
        runtime.block_on(async {
            bridge.build_pegout_transaction(
                test_btc_address(),
                black_box(100_000_000),
            ).await.unwrap()
        })
    });
}
```

## Dependencies

### Blockers
- ALYS-006: Actor supervisor
- ALYS-010: StreamActor for governance communication

### Blocked By
None

### Related Issues
- ALYS-007: ChainActor (block production)
- ALYS-016: Governance integration
- ALYS-017: P2WSH implementation

## Definition of Done

- [ ] BridgeActor fully implemented
- [ ] Peg-in flow working end-to-end
- [ ] Peg-out flow working end-to-end
- [ ] UTXO management operational
- [ ] Retry logic tested
- [ ] No local key storage
- [ ] Integration tests pass
- [ ] Documentation complete
- [ ] Code review completed

## Time Tracking

- Estimated: 6 days
- Actual: _To be filled_