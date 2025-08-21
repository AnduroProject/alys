// Comprehensive test suite for BridgeActor
//
// This module implements extensive testing following Alys V2 testing patterns
// with unit tests, integration tests, and property-based tests.

pub mod unit_tests;
pub mod integration_tests;
pub mod property_tests;
pub mod performance_tests;
pub mod chaos_tests;

use actix::prelude::*;
use bitcoin::{Transaction, TxIn, TxOut, Script, OutPoint, Txid, Address as BtcAddress};
use ethereum_types::{H256, H160};
use std::sync::Arc;
use std::time::Duration;

use super::{
    actor::{BridgeActor, BridgeConfig},
    messages::*,
    errors::BridgeError,
};

// Test utilities and fixtures
pub struct TestFixture {
    pub bridge_actor: Addr<BridgeActor>,
    pub config: BridgeConfig,
    pub federation_address: BtcAddress,
    pub test_bitcoin_rpc: Arc<MockBitcoinRpc>,
}

impl TestFixture {
    pub async fn new() -> Self {
        let config = BridgeConfig::test_config();
        let federation_address = create_test_federation_address();
        let federation_script = create_test_federation_script();
        let bitcoin_rpc = Arc::new(MockBitcoinRpc::new());
        
        let bridge_actor = BridgeActor::new(
            config.clone(),
            federation_address.clone(),
            federation_script,
            bitcoin_rpc.clone(),
        )
        .unwrap()
        .start();

        TestFixture {
            bridge_actor,
            config,
            federation_address,
            test_bitcoin_rpc: bitcoin_rpc,
        }
    }

    pub fn create_test_pegin_tx(&self, amount: u64, evm_address: H160) -> Transaction {
        create_deposit_transaction(amount, evm_address, &self.federation_address)
    }

    pub fn create_test_burn_event(&self, amount: u64, btc_destination: &str) -> BurnEvent {
        BurnEvent {
            tx_hash: H256::random(),
            block_number: 1000,
            amount,
            destination: btc_destination.to_string(),
            sender: H160::random(),
        }
    }
}

impl BridgeConfig {
    pub fn test_config() -> Self {
        Self {
            bitcoin_rpc_url: "http://localhost:18443".to_string(),
            bitcoin_network: bitcoin::Network::Regtest,
            min_confirmations: 1, // Reduced for testing
            max_pegout_amount: 1_000_000_000, // 10 BTC
            batch_pegouts: false,
            batch_threshold: 3,
            retry_delay: Duration::from_secs(1), // Fast retry for tests
            max_retries: 2, // Reduced for testing
            utxo_refresh_interval: Duration::from_secs(10),
            operation_timeout: Duration::from_secs(60),
            dust_limit: 546,
        }
    }
}

// Mock Bitcoin RPC for testing
pub struct MockBitcoinRpc {
    unspent_outputs: std::sync::RwLock<Vec<super::actor::UnspentOutput>>,
    transactions: std::sync::RwLock<Vec<super::actor::TransactionInfo>>,
    transaction_data: std::sync::RwLock<std::collections::HashMap<Txid, Transaction>>,
}

impl MockBitcoinRpc {
    pub fn new() -> Self {
        Self {
            unspent_outputs: std::sync::RwLock::new(Vec::new()),
            transactions: std::sync::RwLock::new(Vec::new()),
            transaction_data: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn add_unspent(&self, txid: Txid, vout: u32, amount: u64, confirmations: u32, script: Script) {
        let unspent = super::actor::UnspentOutput {
            txid,
            vout,
            amount: bitcoin::Amount::from_sat(amount),
            confirmations,
            spendable: true,
            script_pubkey: script,
        };
        
        self.unspent_outputs.write().unwrap().push(unspent);
    }

    pub fn add_transaction(&self, txid: Txid, tx: Transaction, confirmations: u32) {
        let tx_info = super::actor::TransactionInfo {
            txid,
            confirmations,
            amount: bitcoin::Amount::from_sat(100_000_000), // 1 BTC default
        };
        
        self.transactions.write().unwrap().push(tx_info);
        self.transaction_data.write().unwrap().insert(txid, tx);
    }

    pub fn clear(&self) {
        self.unspent_outputs.write().unwrap().clear();
        self.transactions.write().unwrap().clear();
        self.transaction_data.write().unwrap().clear();
    }
}

// Test utility functions
pub fn create_test_federation_address() -> BtcAddress {
    // Use a known test address for regtest
    BtcAddress::from_str("bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh")
        .unwrap()
}

pub fn create_test_federation_script() -> Script {
    // Simple P2WPKH script for testing
    Script::new_p2wpkh(&bitcoin::PubkeyHash::from_slice(&[0u8; 20]).unwrap())
}

pub fn create_deposit_transaction(
    amount: u64,
    evm_address: H160,
    federation_address: &BtcAddress,
) -> Transaction {
    let mut tx = Transaction {
        version: 2,
        lock_time: 0,
        input: vec![
            TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_slice(&[1u8; 32]).unwrap(),
                    vout: 0,
                },
                script_sig: Script::new(),
                sequence: 0xffffffff,
                witness: bitcoin::Witness::new(),
            }
        ],
        output: vec![],
    };

    // Add deposit output to federation
    tx.output.push(TxOut {
        value: amount,
        script_pubkey: federation_address.script_pubkey(),
    });

    // Add OP_RETURN output with EVM address
    let mut op_return_data = vec![0x6a, 0x14]; // OP_RETURN + 20 bytes
    op_return_data.extend_from_slice(evm_address.as_bytes());
    
    tx.output.push(TxOut {
        value: 0,
        script_pubkey: Script::from(op_return_data),
    });

    tx
}

pub fn create_random_txid() -> Txid {
    let mut bytes = [0u8; 32];
    bytes[0] = rand::random();
    bytes[31] = rand::random();
    Txid::from_slice(&bytes).unwrap()
}

pub fn create_random_h160() -> H160 {
    let mut bytes = [0u8; 20];
    for i in 0..20 {
        bytes[i] = rand::random();
    }
    H160::from(bytes)
}

pub fn create_random_h256() -> H256 {
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = rand::random();
    }
    H256::from(bytes)
}

// Actor test harness for integration testing
pub struct ActorTestHarness {
    system: actix::System,
    fixture: TestFixture,
}

impl ActorTestHarness {
    pub async fn new() -> Self {
        let system = actix::System::new();
        let fixture = TestFixture::new().await;
        
        Self { system, fixture }
    }

    pub async fn run_test<F, Fut>(&self, test: F) -> Result<(), BridgeError>
    where
        F: FnOnce(&TestFixture) -> Fut,
        Fut: std::future::Future<Output = Result<(), BridgeError>>,
    {
        test(&self.fixture).await
    }

    pub async fn shutdown(self) {
        self.system.stop();
    }
}

// Test macros for common patterns
#[macro_export]
macro_rules! assert_pegin_processed {
    ($actor:expr, $txid:expr) => {
        {
            let pending = $actor.send(GetPendingPegins).await.unwrap().unwrap();
            assert!(pending.iter().any(|p| p.txid == $txid), "Peg-in not found in pending list");
        }
    };
}

#[macro_export]
macro_rules! assert_pegout_state {
    ($actor:expr, $request_id:expr, $expected_state:pat) => {
        {
            let status = $actor.send(GetOperationStatus {
                operation_id: $request_id.to_string(),
            }).await.unwrap().unwrap();
            
            match status.state {
                $expected_state => {},
                actual => panic!("Expected state {}, got {:?}", stringify!($expected_state), actual),
            }
        }
    };
}

#[macro_export]
macro_rules! assert_bridge_error {
    ($result:expr, $expected_error:pat) => {
        match $result {
            Err($expected_error) => {},
            Err(actual) => panic!("Expected error {}, got {:?}", stringify!($expected_error), actual),
            Ok(val) => panic!("Expected error {}, got Ok({:?})", stringify!($expected_error), val),
        }
    };
}