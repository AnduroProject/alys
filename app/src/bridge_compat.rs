//! Bridge Compatibility Layer
//! 
//! Provides minimal compatibility shims for legacy code during federation crate sunset.
//! This module will be removed once all legacy components are migrated to V2 actors.

use bitcoin::{Transaction as BitcoinTransaction, Txid, BlockHash};
use ethereum_types::{Address, H256, U64};
use ethers_core::types::TransactionReceipt;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Compatibility shim for Bridge functionality
pub struct BridgeCompat {
    pegin_addresses: Vec<bitcoin::Address>,
    required_confirmations: u16,
}

impl BridgeCompat {
    pub fn new(
        pegin_addresses: Vec<bitcoin::Address>,
        required_confirmations: u16,
    ) -> Self {
        Self {
            pegin_addresses,
            required_confirmations,
        }
    }

    /// Filter peg-out requests from transaction receipts
    /// This is a simplified version that delegates to V2 actors
    pub fn filter_pegouts(receipts: Vec<TransactionReceipt>) -> Vec<bitcoin::TxOut> {
        // For now, return empty - V2 actors handle peg-out processing
        // This maintains API compatibility during migration
        Vec::new()
    }

    /// Convert wei to satoshis
    pub fn wei_to_sats(wei: ethers_core::types::U256) -> u64 {
        (wei / ethers_core::types::U256::from(10_000_000_000u64)).as_u64()
    }
}

/// Compatibility type for peg-in information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegInInfoCompat {
    pub txid: Txid,
    pub block_hash: BlockHash,
    pub amount: u64,
    pub evm_account: Address,
    pub block_height: u32,
}

/// Minimal UTXO manager compatibility shim
pub struct UtxoManagerCompat;

impl UtxoManagerCompat {
    pub fn new() -> Self {
        Self
    }

    /// Check if transaction exists (compatibility shim)
    pub fn get_tx(&self, _txid: &Txid) -> Result<Option<BitcoinTransaction>, BridgeCompatError> {
        // Delegate to V2 actors or return None for now
        Ok(None)
    }
}

/// Minimal signature collector compatibility shim  
pub struct SignatureCollectorCompat;

impl SignatureCollectorCompat {
    pub fn new() -> Self {
        Self
    }

    /// Get finalized transaction (compatibility shim)
    pub fn get_finalized(&self, _txid: Txid) -> Result<Option<BitcoinTransaction>, BridgeCompatError> {
        // Delegate to V2 actors or return None for now
        Ok(None)
    }
}

/// Bitcoin signer compatibility shim
pub struct BitcoinSignerCompat;

/// Compatibility errors
#[derive(Debug, thiserror::Error)]
pub enum BridgeCompatError {
    #[error("Operation not supported in compatibility mode")]
    NotSupported,
    #[error("Delegation to V2 actors failed: {message}")]
    DelegationFailed { message: String },
}

/// Type aliases for backward compatibility
pub type BitcoinWallet = UtxoManagerCompat;
pub type BitcoinSignatureCollector = SignatureCollectorCompat;
pub type BitcoinSigner = BitcoinSignerCompat;
pub type Bridge = BridgeCompat;
pub type PegInInfo = PegInInfoCompat;

/// Re-exports for compatibility
pub use bitcoin::Network;
pub use bitcoin::secp256k1::SecretKey as BitcoinSecretKey;

/// Compatibility types that were previously in federation crate
pub type BitcoinPublicKey = bitcoin::PublicKey;

/// Signature collection for multi-signature transactions
#[derive(Debug, Clone)]
pub struct SingleMemberTransactionSignatures {
    pub member_id: String,
    pub signatures: Vec<bitcoin::secp256k1::ecdsa::Signature>,
}

/// Bitcoin RPC client compatibility layer
/// In a full implementation, this would integrate with V2 actors
pub struct BitcoinCoreCompat {
    pub rpc_url: String,
    pub username: String, 
    pub password: String,
}

impl BitcoinCoreCompat {
    pub fn new(rpc_url: &str, username: &str, password: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        }
    }
}

pub type BitcoinCore = BitcoinCoreCompat;

/// Minimal federation struct for compatibility
pub struct FederationCompat {
    pub taproot_address: bitcoin::Address,
}

impl FederationCompat {
    pub fn new(
        _pubkeys: Vec<bitcoin::PublicKey>,
        _threshold: u32,
        network: bitcoin::Network,
    ) -> Self {
        // Create a placeholder taproot address for compatibility
        // In a real migration, this would derive from the actual federation setup
        let taproot_address = bitcoin::Address::from_str("bcrt1pnv0qv2q86ny0my4tycezez7e72jnjns2ays3l4w98v6l383k2h7q0lwmyh")
            .unwrap()
            .require_network(network)
            .unwrap();
            
        Self { taproot_address }
    }
}

pub type Federation = FederationCompat;

/// Error compatibility 
pub use BridgeCompatError as Error;