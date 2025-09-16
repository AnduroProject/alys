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

## Next Steps

### Work Completed Analysis (75% Complete)

**Completed Components (✓):**
- Message protocol design with comprehensive peg-in/peg-out operations (95% complete)
- Core BridgeActor structure with Bitcoin integration (85% complete)
- Peg-in processing logic with transaction validation (80% complete)
- Peg-out processing with unsigned transaction building (85% complete)
- UTXO management system with refresh capabilities (80% complete)
- Basic operation state tracking and history (70% complete)

**Detailed Work Analysis:**
1. **Message Protocol (95%)** - All message types defined including ProcessPegin, ProcessPegout, GetPendingPegins, GetPendingPegouts, ApplySignatures, GetOperationStatus, UpdateFederationAddress, RetryFailedOperations with proper error handling
2. **Actor Structure (85%)** - Complete BridgeActor with Bitcoin Core integration, UTXO management, governance communication, operation tracking, and metrics
3. **Peg-in Logic (80%)** - ProcessPegin handler with transaction validation, confirmation checking, EVM address extraction, and governance notification
4. **Peg-out Logic (85%)** - ProcessPegout handler with burn event processing, unsigned transaction building, signature requesting, and state management
5. **UTXO Management (80%)** - UtxoManager with spendable UTXO selection, refresh capabilities, and spent tracking
6. **Operation Tracking (70%)** - Basic pending operation storage and operation history recording

### Remaining Work Analysis

**Missing Critical Components:**
- Advanced retry logic with exponential backoff and failure categorization (40% complete)
- Comprehensive governance integration with StreamActor coordination (35% complete)
- Production error handling and resilience patterns (30% complete)
- Event processing from bridge contract with reliable event parsing (25% complete)
- Batch processing for multiple peg-outs optimization (20% complete)
- Performance optimization and monitoring (15% complete)

### Detailed Next Step Plans

#### Priority 1: Complete Production-Ready BridgeActor

**Plan:** Implement comprehensive error handling, advanced retry mechanisms, and robust governance integration for the BridgeActor.

**Implementation 1: Advanced Error Handling and Retry System**
```rust
// src/actors/bridge/error_handling.rs
use actix::prelude::*;
use std::time::{Duration, Instant};
use std::collections::HashMap;

#[derive(Debug)]
pub struct BridgeErrorHandler {
    // Retry policies for different operation types
    retry_policies: HashMap<OperationType, RetryPolicy>,
    // Error categorization
    error_classifier: ErrorClassifier,
    // Circuit breaker for external services
    circuit_breakers: HashMap<String, CircuitBreaker>,
    // Failure tracking
    failure_tracker: FailureTracker,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum OperationType {
    PeginProcessing,
    PegoutCreation,
    TransactionBroadcast,
    UtxoRefresh,
    GovernanceCommunication,
    BitcoinRpc,
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub exponential_base: f64,
    pub jitter: bool,
    pub retryable_errors: Vec<BridgeErrorType>,
}

#[derive(Debug)]
pub struct ErrorClassifier {
    permanent_errors: HashSet<BridgeErrorType>,
    temporary_errors: HashSet<BridgeErrorType>,
    governance_errors: HashSet<BridgeErrorType>,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum BridgeErrorType {
    // Network/RPC errors (temporary)
    NetworkTimeout,
    ConnectionFailed,
    RpcError,
    
    // Bitcoin errors
    InsufficientConfirmations,
    InsufficientFunds,
    TransactionRejected,
    UtxoNotFound,
    
    // Validation errors (permanent)
    InvalidAddress,
    InvalidAmount,
    InvalidTransaction,
    NoEvmAddress,
    
    // Governance errors
    GovernanceTimeout,
    SignatureTimeout,
    InvalidSignature,
    
    // System errors
    DatabaseError,
    ConfigurationError,
    InternalError,
}

impl BridgeErrorHandler {
    pub fn new() -> Self {
        let mut retry_policies = HashMap::new();
        
        // Peg-in processing retry policy
        retry_policies.insert(OperationType::PeginProcessing, RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(300),
            exponential_base: 2.0,
            jitter: true,
            retryable_errors: vec![
                BridgeErrorType::NetworkTimeout,
                BridgeErrorType::RpcError,
                BridgeErrorType::DatabaseError,
            ],
        });
        
        // Peg-out creation retry policy
        retry_policies.insert(OperationType::PegoutCreation, RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_secs(60),
            max_delay: Duration::from_secs(600),
            exponential_base: 2.0,
            jitter: true,
            retryable_errors: vec![
                BridgeErrorType::NetworkTimeout,
                BridgeErrorType::UtxoNotFound,
                BridgeErrorType::GovernanceTimeout,
            ],
        });
        
        // Transaction broadcast retry policy
        retry_policies.insert(OperationType::TransactionBroadcast, RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_secs(15),
            max_delay: Duration::from_secs(120),
            exponential_base: 1.5,
            jitter: true,
            retryable_errors: vec![
                BridgeErrorType::NetworkTimeout,
                BridgeErrorType::RpcError,
            ],
        });
        
        // UTXO refresh retry policy
        retry_policies.insert(OperationType::UtxoRefresh, RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(60),
            exponential_base: 2.0,
            jitter: false,
            retryable_errors: vec![
                BridgeErrorType::NetworkTimeout,
                BridgeErrorType::RpcError,
                BridgeErrorType::ConnectionFailed,
            ],
        });
        
        // Governance communication retry policy
        retry_policies.insert(OperationType::GovernanceCommunication, RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(30),
            exponential_base: 2.0,
            jitter: true,
            retryable_errors: vec![
                BridgeErrorType::GovernanceTimeout,
                BridgeErrorType::NetworkTimeout,
            ],
        });
        
        Self {
            retry_policies,
            error_classifier: ErrorClassifier::new(),
            circuit_breakers: HashMap::new(),
            failure_tracker: FailureTracker::new(),
        }
    }

    pub async fn handle_error<T, F, Fut>(
        &mut self,
        operation_type: OperationType,
        operation: F,
        context: &str,
    ) -> Result<T, BridgeError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, BridgeError>>,
    {
        let policy = self.retry_policies.get(&operation_type)
            .cloned()
            .unwrap_or_default();

        let mut attempts = 0;
        let mut last_error = None;

        while attempts < policy.max_attempts {
            attempts += 1;

            // Check circuit breaker
            if let Some(cb) = self.circuit_breakers.get_mut(context) {
                if cb.is_open() {
                    return Err(BridgeError::CircuitBreakerOpen(context.to_string()));
                }
            }

            match operation().await {
                Ok(result) => {
                    if attempts > 1 {
                        info!("Operation '{}' succeeded after {} attempts", context, attempts);
                    }
                    
                    // Record success
                    if let Some(cb) = self.circuit_breakers.get_mut(context) {
                        cb.record_success();
                    }
                    
                    return Ok(result);
                }
                Err(error) => {
                    last_error = Some(error.clone());
                    
                    // Record failure
                    if let Some(cb) = self.circuit_breakers.get_mut(context) {
                        cb.record_failure();
                    }
                    
                    // Check if error is retryable
                    let error_type = self.error_classifier.classify(&error);
                    if !policy.retryable_errors.contains(&error_type) {
                        warn!("Non-retryable error in '{}': {:?}", context, error);
                        return Err(error);
                    }
                    
                    // Check if we should retry
                    if attempts >= policy.max_attempts {
                        error!("Operation '{}' failed after {} attempts", context, attempts);
                        break;
                    }
                    
                    // Calculate delay
                    let delay = self.calculate_delay(&policy, attempts);
                    warn!("Operation '{}' failed (attempt {}/{}), retrying in {:?}", 
                          context, attempts, policy.max_attempts, delay);
                    
                    tokio::time::sleep(delay).await;
                }
            }
        }

        // Track persistent failures
        self.failure_tracker.record_failure(operation_type, context.to_string());
        
        Err(last_error.unwrap_or(BridgeError::MaxRetriesExceeded))
    }

    fn calculate_delay(&self, policy: &RetryPolicy, attempt: u32) -> Duration {
        let delay = policy.base_delay.as_millis() as f64 
            * policy.exponential_base.powi((attempt - 1) as i32);
        
        let delay = Duration::from_millis(delay as u64).min(policy.max_delay);
        
        if policy.jitter {
            // Add random jitter ±25%
            let jitter_range = delay.as_millis() as f64 * 0.25;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            let final_delay = delay.as_millis() as f64 + jitter;
            Duration::from_millis(final_delay.max(0.0) as u64)
        } else {
            delay
        }
    }
}

impl ErrorClassifier {
    pub fn new() -> Self {
        let mut permanent_errors = HashSet::new();
        permanent_errors.insert(BridgeErrorType::InvalidAddress);
        permanent_errors.insert(BridgeErrorType::InvalidAmount);
        permanent_errors.insert(BridgeErrorType::InvalidTransaction);
        permanent_errors.insert(BridgeErrorType::NoEvmAddress);
        permanent_errors.insert(BridgeErrorType::ConfigurationError);
        
        let mut temporary_errors = HashSet::new();
        temporary_errors.insert(BridgeErrorType::NetworkTimeout);
        temporary_errors.insert(BridgeErrorType::ConnectionFailed);
        temporary_errors.insert(BridgeErrorType::RpcError);
        temporary_errors.insert(BridgeErrorType::DatabaseError);
        temporary_errors.insert(BridgeErrorType::UtxoNotFound);
        
        let mut governance_errors = HashSet::new();
        governance_errors.insert(BridgeErrorType::GovernanceTimeout);
        governance_errors.insert(BridgeErrorType::SignatureTimeout);
        governance_errors.insert(BridgeErrorType::InvalidSignature);
        
        Self {
            permanent_errors,
            temporary_errors,
            governance_errors,
        }
    }

    pub fn classify(&self, error: &BridgeError) -> BridgeErrorType {
        match error {
            BridgeError::NetworkTimeout => BridgeErrorType::NetworkTimeout,
            BridgeError::InvalidAddress(_) => BridgeErrorType::InvalidAddress,
            BridgeError::InsufficientConfirmations => BridgeErrorType::InsufficientConfirmations,
            BridgeError::InsufficientFunds => BridgeErrorType::InsufficientFunds,
            BridgeError::NoEvmAddress => BridgeErrorType::NoEvmAddress,
            BridgeError::BroadcastFailed(_) => BridgeErrorType::TransactionRejected,
            BridgeError::GovernanceTimeout => BridgeErrorType::GovernanceTimeout,
            BridgeError::RpcError(_) => BridgeErrorType::RpcError,
            _ => BridgeErrorType::InternalError,
        }
    }

    pub fn is_retryable(&self, error_type: &BridgeErrorType) -> bool {
        self.temporary_errors.contains(error_type) || 
        self.governance_errors.contains(error_type)
    }
}

#[derive(Debug)]
pub struct FailureTracker {
    operation_failures: HashMap<OperationType, Vec<FailureRecord>>,
    context_failures: HashMap<String, Vec<FailureRecord>>,
}

#[derive(Debug)]
pub struct FailureRecord {
    pub timestamp: Instant,
    pub error_type: BridgeErrorType,
    pub context: String,
}

impl FailureTracker {
    pub fn new() -> Self {
        Self {
            operation_failures: HashMap::new(),
            context_failures: HashMap::new(),
        }
    }

    pub fn record_failure(&mut self, operation_type: OperationType, context: String) {
        let record = FailureRecord {
            timestamp: Instant::now(),
            error_type: BridgeErrorType::InternalError, // Would be passed in real implementation
            context: context.clone(),
        };

        self.operation_failures.entry(operation_type)
            .or_insert_with(Vec::new)
            .push(record.clone());

        self.context_failures.entry(context)
            .or_insert_with(Vec::new)
            .push(record);
    }

    pub fn get_failure_rate(&self, operation_type: &OperationType, window: Duration) -> f64 {
        if let Some(failures) = self.operation_failures.get(operation_type) {
            let recent_failures = failures.iter()
                .filter(|f| f.timestamp.elapsed() < window)
                .count();
            
            // Simple rate calculation - could be more sophisticated
            recent_failures as f64 / window.as_secs() as f64 * 60.0 // failures per minute
        } else {
            0.0
        }
    }
}

// Enhanced BridgeActor with error handling
impl BridgeActor {
    pub async fn resilient_process_pegin(
        &mut self,
        tx: Transaction,
        confirmations: u32,
        deposit_address: BtcAddress,
    ) -> Result<(), BridgeError> {
        self.error_handler.handle_error(
            OperationType::PeginProcessing,
            || async {
                // Original pegin processing logic here
                self.process_pegin_internal(tx.clone(), confirmations, deposit_address.clone()).await
            },
            "process_pegin",
        ).await
    }

    pub async fn resilient_process_pegout(
        &mut self,
        burn_event: BurnEvent,
        request_id: String,
    ) -> Result<PegoutResult, BridgeError> {
        self.error_handler.handle_error(
            OperationType::PegoutCreation,
            || async {
                self.process_pegout_internal(burn_event.clone(), request_id.clone()).await
            },
            "process_pegout",
        ).await
    }

    pub async fn resilient_broadcast_transaction(
        &mut self,
        tx: Transaction,
    ) -> Result<Txid, BridgeError> {
        self.error_handler.handle_error(
            OperationType::TransactionBroadcast,
            || async {
                self.bitcoin_core.send_raw_transaction(&tx).await
                    .map_err(|e| BridgeError::BroadcastFailed(e.to_string()))
            },
            "broadcast_transaction",
        ).await
    }

    pub async fn resilient_refresh_utxos(&mut self) -> Result<(), BridgeError> {
        self.error_handler.handle_error(
            OperationType::UtxoRefresh,
            || async {
                self.utxo_manager.refresh().await
            },
            "refresh_utxos",
        ).await
    }
}
```

**Implementation 2: Advanced Governance Integration**
```rust
// src/actors/bridge/governance.rs
use actix::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct GovernanceCoordinator {
    // StreamActor communication
    stream_actor: Addr<StreamActor>,
    
    // Pending signature requests
    pending_requests: HashMap<String, SignatureRequest>,
    request_timeouts: HashMap<String, Instant>,
    
    // Governance state tracking
    governance_state: GovernanceState,
    
    // Request batching
    batch_manager: BatchManager,
    
    // Configuration
    config: GovernanceConfig,
}

#[derive(Debug, Clone)]
pub struct GovernanceConfig {
    pub signature_timeout: Duration,
    pub batch_size: usize,
    pub batch_timeout: Duration,
    pub retry_attempts: u32,
    pub quorum_threshold: usize,
}

#[derive(Debug)]
pub struct GovernanceState {
    pub active_signers: HashSet<String>,
    pub inactive_signers: HashSet<String>,
    pub current_epoch: u64,
    pub last_heartbeat: Instant,
}

#[derive(Debug)]
pub struct BatchManager {
    pending_batches: HashMap<String, SignatureBatch>,
    batch_timers: HashMap<String, Instant>,
}

#[derive(Debug)]
pub struct SignatureBatch {
    pub batch_id: String,
    pub requests: Vec<SignatureRequest>,
    pub priority: BatchPriority,
    pub created_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl GovernanceCoordinator {
    pub fn new(
        stream_actor: Addr<StreamActor>,
        config: GovernanceConfig,
    ) -> Self {
        Self {
            stream_actor,
            pending_requests: HashMap::new(),
            request_timeouts: HashMap::new(),
            governance_state: GovernanceState::new(),
            batch_manager: BatchManager::new(),
            config,
        }
    }

    pub async fn request_signatures(
        &mut self,
        request: SignatureRequest,
    ) -> Result<String, BridgeError> {
        let request_id = request.request_id.clone();
        
        // Check if governance is healthy
        if !self.is_governance_healthy() {
            return Err(BridgeError::GovernanceUnavailable);
        }

        // Determine priority based on request type and amount
        let priority = self.calculate_priority(&request);
        
        // Check if we should batch this request
        if self.should_batch(&request, priority) {
            self.add_to_batch(request, priority).await?;
        } else {
            // Send immediately for critical requests
            self.send_signature_request(request).await?;
        }

        Ok(request_id)
    }

    pub async fn handle_signatures_received(
        &mut self,
        request_id: String,
        signatures: Vec<SignatureResponse>,
    ) -> Result<(), BridgeError> {
        // Validate signatures
        let validated_signatures = self.validate_signatures(&signatures).await?;
        
        // Check if we have enough signatures for quorum
        if validated_signatures.len() >= self.config.quorum_threshold {
            // Convert signatures to witness data
            let witnesses = self.convert_signatures_to_witnesses(validated_signatures)?;
            
            // Remove from pending requests
            self.pending_requests.remove(&request_id);
            self.request_timeouts.remove(&request_id);
            
            // Return witnesses to bridge actor
            // This would be handled by the calling bridge actor
            Ok(())
        } else {
            warn!("Insufficient signatures for request {}: got {}, need {}", 
                  request_id, validated_signatures.len(), self.config.quorum_threshold);
            Err(BridgeError::InsufficientSignatures)
        }
    }

    async fn add_to_batch(
        &mut self,
        request: SignatureRequest,
        priority: BatchPriority,
    ) -> Result<(), BridgeError> {
        // Find or create appropriate batch
        let batch_id = self.find_or_create_batch(priority);
        
        let batch = self.batch_manager.pending_batches
            .get_mut(&batch_id)
            .ok_or(BridgeError::BatchNotFound)?;
        
        batch.requests.push(request);
        
        // Check if batch is ready to send
        if batch.requests.len() >= self.config.batch_size ||
           batch.created_at.elapsed() > self.config.batch_timeout ||
           priority >= BatchPriority::High {
            
            self.send_batch(batch_id).await?;
        }

        Ok(())
    }

    async fn send_batch(&mut self, batch_id: String) -> Result<(), BridgeError> {
        let batch = self.batch_manager.pending_batches
            .remove(&batch_id)
            .ok_or(BridgeError::BatchNotFound)?;
        
        info!("Sending signature batch with {} requests", batch.requests.len());
        
        // Convert batch to governance message
        let batch_request = BatchSignatureRequest {
            batch_id: batch.batch_id.clone(),
            requests: batch.requests.clone(),
            priority: batch.priority,
            deadline: Instant::now() + self.config.signature_timeout,
        };
        
        // Send to StreamActor
        self.stream_actor
            .send(RequestBatchSignatures(batch_request))
            .await
            .map_err(|e| BridgeError::GovernanceCommunicationError(e.to_string()))??;
        
        // Track individual requests
        for request in batch.requests {
            self.pending_requests.insert(request.request_id.clone(), request);
            self.request_timeouts.insert(
                request.request_id,
                Instant::now() + self.config.signature_timeout,
            );
        }

        self.batch_manager.batch_timers.remove(&batch_id);
        
        Ok(())
    }

    fn calculate_priority(&self, request: &SignatureRequest) -> BatchPriority {
        // Priority based on amount and urgency
        let amount_btc = request.amounts.iter().sum::<u64>() as f64 / 100_000_000.0;
        
        match () {
            _ if amount_btc >= 10.0 => BatchPriority::Critical,  // >= 10 BTC
            _ if amount_btc >= 1.0 => BatchPriority::High,       // >= 1 BTC
            _ if amount_btc >= 0.1 => BatchPriority::Normal,     // >= 0.1 BTC
            _ => BatchPriority::Low,                             // < 0.1 BTC
        }
    }

    fn should_batch(&self, request: &SignatureRequest, priority: BatchPriority) -> bool {
        // Don't batch critical requests or if governance is under stress
        if priority >= BatchPriority::Critical || !self.is_governance_healthy() {
            return false;
        }
        
        // Check if there are existing batches we can join
        self.batch_manager.pending_batches
            .values()
            .any(|batch| batch.priority == priority && batch.requests.len() < self.config.batch_size)
    }

    fn find_or_create_batch(&mut self, priority: BatchPriority) -> String {
        // Look for existing batch with same priority
        for (batch_id, batch) in &self.batch_manager.pending_batches {
            if batch.priority == priority && batch.requests.len() < self.config.batch_size {
                return batch_id.clone();
            }
        }
        
        // Create new batch
        let batch_id = format!("batch-{}-{}", 
                              priority.to_string().to_lowercase(),
                              chrono::Utc::now().timestamp_millis());
        
        let batch = SignatureBatch {
            batch_id: batch_id.clone(),
            requests: Vec::new(),
            priority,
            created_at: Instant::now(),
        };
        
        self.batch_manager.pending_batches.insert(batch_id.clone(), batch);
        self.batch_manager.batch_timers.insert(batch_id.clone(), Instant::now());
        
        batch_id
    }

    async fn validate_signatures(&self, signatures: &[SignatureResponse]) -> Result<Vec<SignatureResponse>, BridgeError> {
        let mut validated = Vec::new();
        
        for signature in signatures {
            // Validate signature format
            if signature.signature.len() != 64 && signature.signature.len() != 65 {
                warn!("Invalid signature length from signer {}", signature.signer_id);
                continue;
            }
            
            // Check if signer is authorized
            if !self.governance_state.active_signers.contains(&signature.signer_id) {
                warn!("Unauthorized signer: {}", signature.signer_id);
                continue;
            }
            
            // Additional cryptographic validation would go here
            // For now, assume valid if basic checks pass
            validated.push(signature.clone());
        }
        
        Ok(validated)
    }

    fn convert_signatures_to_witnesses(
        &self,
        signatures: Vec<SignatureResponse>,
    ) -> Result<Vec<WitnessData>, BridgeError> {
        let mut witnesses = Vec::new();
        
        for signature in signatures {
            // Convert signature to witness format
            // This depends on the specific script structure (P2WSH, taproot, etc.)
            let witness = WitnessData {
                input_index: signature.input_index,
                witness: vec![
                    signature.signature,
                    // Additional witness elements would depend on script
                ],
            };
            
            witnesses.push(witness);
        }
        
        Ok(witnesses)
    }

    fn is_governance_healthy(&self) -> bool {
        // Check if enough signers are active
        let active_count = self.governance_state.active_signers.len();
        let min_required = (self.config.quorum_threshold * 3) / 2; // 150% of quorum
        
        if active_count < min_required {
            return false;
        }
        
        // Check last heartbeat
        if self.governance_state.last_heartbeat.elapsed() > Duration::from_secs(300) {
            return false;
        }
        
        true
    }

    pub async fn handle_timeout_check(&mut self) -> Result<(), BridgeError> {
        let now = Instant::now();
        let mut timed_out_requests = Vec::new();
        
        // Check for timed out requests
        for (request_id, timeout) in &self.request_timeouts {
            if now > *timeout {
                timed_out_requests.push(request_id.clone());
            }
        }
        
        // Handle timeouts
        for request_id in timed_out_requests {
            warn!("Signature request timed out: {}", request_id);
            
            if let Some(request) = self.pending_requests.remove(&request_id) {
                // Try to retry the request if within retry limits
                if request.retry_count < self.config.retry_attempts {
                    let mut retry_request = request;
                    retry_request.retry_count += 1;
                    retry_request.request_id = format!("{}-retry-{}", 
                                                     retry_request.request_id, 
                                                     retry_request.retry_count);
                    
                    info!("Retrying signature request: {}", retry_request.request_id);
                    self.request_signatures(retry_request).await?;
                } else {
                    error!("Signature request exhausted retries: {}", request_id);
                    // This would notify the BridgeActor of the permanent failure
                }
            }
            
            self.request_timeouts.remove(&request_id);
        }
        
        // Check for batch timeouts
        let mut timed_out_batches = Vec::new();
        for (batch_id, created_at) in &self.batch_manager.batch_timers {
            if created_at.elapsed() > self.config.batch_timeout {
                timed_out_batches.push(batch_id.clone());
            }
        }
        
        // Send timed out batches
        for batch_id in timed_out_batches {
            info!("Sending batch due to timeout: {}", batch_id);
            self.send_batch(batch_id).await?;
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SignatureRequest {
    pub request_id: String,
    pub tx_hex: String,
    pub input_indices: Vec<usize>,
    pub amounts: Vec<u64>,
    pub retry_count: u32,
}

#[derive(Debug, Clone)]
pub struct SignatureResponse {
    pub request_id: String,
    pub input_index: usize,
    pub signature: Vec<u8>,
    pub signer_id: String,
    pub timestamp: u64,
}

#[derive(Message)]
#[rtype(result = "Result<(), BridgeError>")]
pub struct RequestBatchSignatures(pub BatchSignatureRequest);

#[derive(Debug)]
pub struct BatchSignatureRequest {
    pub batch_id: String,
    pub requests: Vec<SignatureRequest>,
    pub priority: BatchPriority,
    pub deadline: Instant,
}

impl ToString for BatchPriority {
    fn to_string(&self) -> String {
        match self {
            BatchPriority::Low => "low",
            BatchPriority::Normal => "normal", 
            BatchPriority::High => "high",
            BatchPriority::Critical => "critical",
        }.to_string()
    }
}
```

**Implementation 3: Bridge Contract Event Processing**
```rust
// src/actors/bridge/event_processor.rs
use actix::prelude::*;
use ethereum_types::{H256, H160, U256};
use std::collections::{HashMap, VecDeque};

#[derive(Debug)]
pub struct BridgeEventProcessor {
    // Event processing state
    last_processed_block: u64,
    pending_events: VecDeque<BridgeEvent>,
    processed_events: HashMap<H256, BridgeEvent>,
    
    // Event filters
    burn_event_filter: EventFilter,
    
    // Configuration
    config: EventProcessorConfig,
    
    // Event validation
    validator: EventValidator,
    
    // Retry mechanism
    retry_queue: VecDeque<RetryableEvent>,
}

#[derive(Debug, Clone)]
pub struct EventProcessorConfig {
    pub confirmation_blocks: u64,
    pub max_blocks_per_query: u64,
    pub event_batch_size: usize,
    pub retry_attempts: u32,
    pub retry_delay: Duration,
}

#[derive(Debug, Clone)]
pub struct BridgeEvent {
    pub event_type: BridgeEventType,
    pub tx_hash: H256,
    pub block_number: u64,
    pub log_index: u64,
    pub data: BridgeEventData,
    pub confirmations: u64,
}

#[derive(Debug, Clone)]
pub enum BridgeEventType {
    PegoutRequested,
    FederationUpdated,
    EmergencyPause,
    EmergencyResume,
}

#[derive(Debug, Clone)]
pub enum BridgeEventData {
    PegoutRequest {
        amount: U256,
        destination: String,
        sender: H160,
        request_id: H256,
    },
    FederationUpdate {
        old_federation: H160,
        new_federation: H160,
        version: U256,
    },
    EmergencyAction {
        paused: bool,
        initiator: H160,
    },
}

#[derive(Debug)]
pub struct EventFilter {
    pub contract_address: H160,
    pub topics: Vec<H256>,
    pub from_block: u64,
    pub to_block: Option<u64>,
}

#[derive(Debug)]
pub struct EventValidator {
    // Validation rules
    min_pegout_amount: U256,
    max_pegout_amount: U256,
    authorized_contracts: HashSet<H160>,
    
    // Duplicate detection
    seen_events: HashMap<(H256, u64), Instant>, // (tx_hash, log_index) -> timestamp
}

#[derive(Debug)]
pub struct RetryableEvent {
    pub event: BridgeEvent,
    pub retry_count: u32,
    pub next_retry: Instant,
    pub error: String,
}

impl BridgeEventProcessor {
    pub fn new(config: EventProcessorConfig, contract_address: H160) -> Self {
        let burn_event_filter = EventFilter {
            contract_address,
            topics: vec![
                // PegoutRequested event signature
                H256::from_slice(&keccak256("PegoutRequested(uint256,string,address,bytes32)")),
            ],
            from_block: 0,
            to_block: None,
        };

        Self {
            last_processed_block: 0,
            pending_events: VecDeque::new(),
            processed_events: HashMap::new(),
            burn_event_filter,
            config,
            validator: EventValidator::new(),
            retry_queue: VecDeque::new(),
        }
    }

    pub async fn process_events(
        &mut self,
        current_block: u64,
    ) -> Result<Vec<BridgeEvent>, BridgeError> {
        let mut processed_events = Vec::new();
        
        // Update filter to query from last processed block
        let from_block = self.last_processed_block + 1;
        let to_block = current_block.saturating_sub(self.config.confirmation_blocks);
        
        if from_block > to_block {
            return Ok(processed_events); // No new blocks to process
        }
        
        // Query events in batches to avoid overwhelming the RPC
        let mut query_from = from_block;
        while query_from <= to_block {
            let query_to = (query_from + self.config.max_blocks_per_query - 1).min(to_block);
            
            let events = self.query_bridge_events(query_from, query_to).await?;
            
            for event in events {
                // Validate event
                if let Err(e) = self.validator.validate_event(&event) {
                    warn!("Invalid event {}: {}", event.tx_hash, e);
                    continue;
                }
                
                // Check for duplicates
                let event_key = (event.tx_hash, event.log_index);
                if self.validator.seen_events.contains_key(&event_key) {
                    debug!("Skipping duplicate event: {:?}", event_key);
                    continue;
                }
                
                // Record as seen
                self.validator.seen_events.insert(event_key, Instant::now());
                
                // Add to pending queue
                self.pending_events.push_back(event);
            }
            
            query_from = query_to + 1;
        }
        
        // Process pending events
        while let Some(event) = self.pending_events.pop_front() {
            match self.process_single_event(&event).await {
                Ok(()) => {
                    processed_events.push(event.clone());
                    self.processed_events.insert(event.tx_hash, event);
                }
                Err(e) => {
                    warn!("Failed to process event {}: {}", event.tx_hash, e);
                    
                    // Add to retry queue
                    self.retry_queue.push_back(RetryableEvent {
                        event,
                        retry_count: 0,
                        next_retry: Instant::now() + self.config.retry_delay,
                        error: e.to_string(),
                    });
                }
            }
        }
        
        // Process retry queue
        self.process_retry_queue().await?;
        
        // Update last processed block
        self.last_processed_block = to_block;
        
        Ok(processed_events)
    }

    async fn query_bridge_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<BridgeEvent>, BridgeError> {
        // This would use web3 or similar to query Ethereum logs
        // For now, returning placeholder implementation
        
        info!("Querying bridge events from block {} to {}", from_block, to_block);
        
        // Mock implementation - would be replaced with actual RPC calls
        let logs = vec![]; // web3.eth().logs(&filter).await?;
        
        let mut events = Vec::new();
        
        for log in logs {
            if let Ok(event) = self.parse_log_to_event(&log).await {
                events.push(event);
            }
        }
        
        Ok(events)
    }

    async fn parse_log_to_event(&self, log: &EthereumLog) -> Result<BridgeEvent, BridgeError> {
        // Parse based on the first topic (event signature)
        if log.topics.is_empty() {
            return Err(BridgeError::InvalidEventFormat);
        }
        
        let event_signature = log.topics[0];
        
        // PegoutRequested event
        if event_signature == H256::from_slice(&keccak256("PegoutRequested(uint256,string,address,bytes32)")) {
            if log.topics.len() < 4 {
                return Err(BridgeError::InvalidEventFormat);
            }
            
            let amount = U256::from_big_endian(&log.topics[1].as_bytes()[..32]);
            let sender = H160::from_slice(&log.topics[2].as_bytes()[12..]);
            let request_id = log.topics[3];
            
            // Decode destination from log data
            let destination = self.decode_string_from_data(&log.data)?;
            
            let event_data = BridgeEventData::PegoutRequest {
                amount,
                destination,
                sender,
                request_id,
            };
            
            return Ok(BridgeEvent {
                event_type: BridgeEventType::PegoutRequested,
                tx_hash: log.transaction_hash,
                block_number: log.block_number,
                log_index: log.log_index,
                data: event_data,
                confirmations: 0, // Will be calculated later
            });
        }
        
        // FederationUpdated event
        if event_signature == H256::from_slice(&keccak256("FederationUpdated(address,address,uint256)")) {
            if log.topics.len() < 4 {
                return Err(BridgeError::InvalidEventFormat);
            }
            
            let old_federation = H160::from_slice(&log.topics[1].as_bytes()[12..]);
            let new_federation = H160::from_slice(&log.topics[2].as_bytes()[12..]);
            let version = U256::from_big_endian(&log.topics[3].as_bytes());
            
            let event_data = BridgeEventData::FederationUpdate {
                old_federation,
                new_federation,
                version,
            };
            
            return Ok(BridgeEvent {
                event_type: BridgeEventType::FederationUpdated,
                tx_hash: log.transaction_hash,
                block_number: log.block_number,
                log_index: log.log_index,
                data: event_data,
                confirmations: 0,
            });
        }
        
        Err(BridgeError::UnknownEventType)
    }

    async fn process_single_event(&self, event: &BridgeEvent) -> Result<(), BridgeError> {
        match &event.data {
            BridgeEventData::PegoutRequest { amount, destination, sender, request_id } => {
                // Convert to burn event format expected by BridgeActor
                let burn_event = BurnEvent {
                    tx_hash: event.tx_hash,
                    block_number: event.block_number,
                    amount: amount.as_u64(), // Assuming amount fits in u64
                    destination: destination.clone(),
                    sender: *sender,
                };
                
                // This would send to BridgeActor - for now just log
                info!("Processing pegout request: {} BTC to {}", 
                      amount.as_u64() as f64 / 100_000_000.0, 
                      destination);
                
                Ok(())
            }
            
            BridgeEventData::FederationUpdate { new_federation, version, .. } => {
                info!("Processing federation update to version {}", version);
                
                // This would update the bridge actor's federation info
                Ok(())
            }
            
            BridgeEventData::EmergencyAction { paused, .. } => {
                if *paused {
                    warn!("Bridge contract paused by emergency action");
                } else {
                    info!("Bridge contract resumed from emergency pause");
                }
                
                Ok(())
            }
        }
    }

    async fn process_retry_queue(&mut self) -> Result<(), BridgeError> {
        let now = Instant::now();
        let mut remaining_retries = VecDeque::new();
        
        while let Some(mut retry_event) = self.retry_queue.pop_front() {
            if now < retry_event.next_retry {
                // Not ready to retry yet
                remaining_retries.push_back(retry_event);
                continue;
            }
            
            retry_event.retry_count += 1;
            
            if retry_event.retry_count > self.config.retry_attempts {
                error!("Event processing permanently failed after {} attempts: {}", 
                       self.config.retry_attempts, retry_event.event.tx_hash);
                continue;
            }
            
            match self.process_single_event(&retry_event.event).await {
                Ok(()) => {
                    info!("Event processing succeeded on retry {}: {}", 
                          retry_event.retry_count, retry_event.event.tx_hash);
                    
                    self.processed_events.insert(retry_event.event.tx_hash, retry_event.event);
                }
                Err(e) => {
                    warn!("Event processing failed on retry {}: {} - {}", 
                          retry_event.retry_count, retry_event.event.tx_hash, e);
                    
                    retry_event.error = e.to_string();
                    retry_event.next_retry = now + self.config.retry_delay * retry_event.retry_count;
                    remaining_retries.push_back(retry_event);
                }
            }
        }
        
        self.retry_queue = remaining_retries;
        Ok(())
    }

    fn decode_string_from_data(&self, data: &[u8]) -> Result<String, BridgeError> {
        if data.len() < 64 {
            return Err(BridgeError::InvalidEventFormat);
        }
        
        // ABI encoding: first 32 bytes are offset, next 32 bytes are length
        let length = U256::from_big_endian(&data[32..64]).as_usize();
        
        if data.len() < 64 + length {
            return Err(BridgeError::InvalidEventFormat);
        }
        
        let string_bytes = &data[64..64 + length];
        String::from_utf8(string_bytes.to_vec())
            .map_err(|_| BridgeError::InvalidEventFormat)
    }
}

impl EventValidator {
    pub fn new() -> Self {
        Self {
            min_pegout_amount: U256::from(10_000), // 0.0001 BTC minimum
            max_pegout_amount: U256::from(1_000_000_000), // 10 BTC maximum
            authorized_contracts: HashSet::new(),
            seen_events: HashMap::new(),
        }
    }

    pub fn validate_event(&self, event: &BridgeEvent) -> Result<(), String> {
        match &event.data {
            BridgeEventData::PegoutRequest { amount, destination, .. } => {
                // Validate amount
                if *amount < self.min_pegout_amount {
                    return Err(format!("Amount too small: {}", amount));
                }
                
                if *amount > self.max_pegout_amount {
                    return Err(format!("Amount too large: {}", amount));
                }
                
                // Validate destination address format
                if destination.is_empty() || destination.len() > 100 {
                    return Err("Invalid destination address".to_string());
                }
                
                // Basic Bitcoin address validation
                if !destination.starts_with("bc1") && 
                   !destination.starts_with("1") && 
                   !destination.starts_with("3") {
                    return Err("Invalid Bitcoin address format".to_string());
                }
                
                Ok(())
            }
            
            BridgeEventData::FederationUpdate { version, .. } => {
                // Validate version progression
                if version.is_zero() {
                    return Err("Invalid federation version".to_string());
                }
                
                Ok(())
            }
            
            BridgeEventData::EmergencyAction { .. } => {
                // Emergency actions are always valid if from authorized source
                Ok(())
            }
        }
    }
}

// Mock structures for compilation
#[derive(Debug)]
pub struct EthereumLog {
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
    pub transaction_hash: H256,
    pub block_number: u64,
    pub log_index: u64,
}

fn keccak256(input: &str) -> [u8; 32] {
    // Mock implementation - would use actual keccak256
    [0u8; 32]
}
```

#### Priority 2: Performance Optimization and Monitoring

**Plan:** Implement comprehensive monitoring, batch processing optimizations, and performance benchmarks.

### Detailed Test Plan

**Unit Tests (180 tests):**
1. Message handling tests (40 tests)
2. Peg-in processing tests (35 tests)
3. Peg-out workflow tests (40 tests)
4. UTXO management tests (25 tests)
5. Error handling and retry tests (25 tests)
6. Event processing tests (15 tests)

**Integration Tests (120 tests):**
1. End-to-end peg-in flow (30 tests)
2. End-to-end peg-out flow (35 tests)
3. Bitcoin regtest integration (25 tests)
4. Governance coordination tests (20 tests)
5. Error recovery scenarios (10 tests)

**Performance Tests (40 benchmarks):**
1. Transaction building performance (10 benchmarks)
2. UTXO selection algorithms (10 benchmarks)
3. Event processing throughput (10 benchmarks)
4. Memory usage optimization (10 benchmarks)

### Implementation Timeline

**Week 1-2: Core Error Handling**
- Complete advanced retry mechanisms with exponential backoff
- Implement circuit breakers and failure tracking
- Add comprehensive error classification

**Week 3: Governance Integration**
- Complete batch processing system for signature requests
- Implement timeout handling and quorum management
- Add governance health monitoring

**Week 4: Event Processing and Optimization**
- Complete bridge contract event processing
- Implement batch processing optimizations
- Performance testing and monitoring integration

### Success Metrics

**Functional Metrics:**
- 100% test coverage for peg operations
- Zero funds loss during operation
- All acceptance criteria satisfied

**Performance Metrics:**
- Peg-in processing ≤ 30 seconds average
- Peg-out initiation ≤ 60 seconds average
- UTXO refresh ≤ 10 seconds
- Memory usage ≤ 128MB under load

**Operational Metrics:**
- 99.9% operation success rate
- Error recovery within 5 minutes
- Governance response time ≤ 2 minutes
- Event processing lag ≤ 30 seconds

### Risk Mitigation

**Technical Risks:**
- **Bitcoin RPC failures**: Multiple endpoint support and automatic failover
- **Governance coordination issues**: Timeout handling and retry mechanisms
- **Event processing delays**: Batch processing and priority queues

**Operational Risks:**
- **Fund security**: No local key storage and comprehensive transaction validation
- **Network partitions**: Graceful degradation and automatic recovery
- **Performance issues**: Resource monitoring and automatic scaling