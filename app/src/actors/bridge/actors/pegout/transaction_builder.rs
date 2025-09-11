//! Bitcoin Transaction Builder
//! 
//! Transaction construction utilities for peg-out operations

use bitcoin::{Transaction, TxIn, TxOut, OutPoint, Sequence, Witness, Address as BtcAddress, ScriptBuf};
use std::sync::Arc;
use tracing::{info, debug};

use crate::actors::bridge::shared::*;
use crate::actors::bridge::shared::constants::DUST_LIMIT;
use super::actor::PegOutError;

/// Bitcoin transaction builder for peg-out operations
#[derive(Debug)]
pub struct TransactionBuilder {
    bitcoin_client: Arc<dyn BitcoinRpc>,
    federation_config: FederationConfig,
}

impl TransactionBuilder {
    /// Create new transaction builder
    pub fn new(
        bitcoin_client: Arc<dyn BitcoinRpc>,
        federation_config: FederationConfig,
    ) -> Result<Self, PegOutError> {
        Ok(Self {
            bitcoin_client,
            federation_config,
        })
    }

    /// Build withdrawal transaction
    pub async fn build_withdrawal_transaction(
        &self,
        destination: BtcAddress,
        amount: u64,
        utxo_manager: &mut UtxoManager,
    ) -> Result<Transaction, PegOutError> {
        info!("Building withdrawal transaction for {} sats to {}", amount, destination);

        // Select UTXOs
        let selection_criteria = SelectionCriteria {
            target_amount: amount,
            fee_rate: 10, // 10 sat/vB
            strategy: SelectionStrategy::MinimizeFees,
            max_utxos: Some(10),
            exclude_dust: true,
            prefer_confirmed: true,
        };

        let utxo_selection = utxo_manager.select_utxos(selection_criteria)
            .map_err(|e| PegOutError::UtxoError(e.to_string()))?;

        // Build inputs
        let mut inputs = Vec::new();
        for utxo in &utxo_selection.selected_utxos {
            let input = TxIn {
                previous_output: utxo.outpoint,
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            };
            inputs.push(input);
        }

        // Build outputs
        let mut outputs = Vec::new();

        // Destination output
        let destination_output = TxOut {
            value: amount,
            script_pubkey: destination.script_pubkey(),
        };
        outputs.push(destination_output);

        // Change output (if needed)
        if utxo_selection.change_amount > DUST_LIMIT {
            let change_address = self.federation_config.addresses.taproot.clone();
            let change_output = TxOut {
                value: utxo_selection.change_amount,
                script_pubkey: change_address.script_pubkey(),
            };
            outputs.push(change_output);
        }

        // Create transaction
        let transaction = Transaction {
            version: 2,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: inputs,
            output: outputs,
        };

        debug!("Built transaction with {} inputs and {} outputs", 
               transaction.input.len(), transaction.output.len());

        Ok(transaction)
    }
}

/// Fee estimator for Bitcoin transactions
#[derive(Debug)]
pub struct FeeEstimator {
    bitcoin_client: Arc<dyn BitcoinRpc>,
    default_fee_rate: u64,
}

impl FeeEstimator {
    /// Create new fee estimator
    pub fn new(bitcoin_client: Arc<dyn BitcoinRpc>, default_fee_rate: u64) -> Self {
        Self {
            bitcoin_client,
            default_fee_rate,
        }
    }

    /// Estimate fee for transaction
    pub async fn estimate_fee(&self, tx_vsize: usize) -> Result<u64, PegOutError> {
        // Try to get dynamic fee estimate
        match self.bitcoin_client.estimate_smart_fee(6).await {
            Ok(fee_estimate) => {
                let sat_per_vb = (fee_estimate.feerate * 100_000.0) as u64;
                Ok(sat_per_vb * tx_vsize as u64)
            }
            Err(_) => {
                // Fall back to default rate
                Ok(self.default_fee_rate * tx_vsize as u64)
            }
        }
    }
}