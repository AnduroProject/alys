//! Peg-in types for Bitcoin deposit processing.
//!
//! Miners monitor Bitcoin for deposits and submit them via submitauxblock.
//! These types define the peg-in data structures and compensation parameters.

use ethereum_types::Address;
use serde::{Deserialize, Serialize};

/// Peg-in information extracted from Bitcoin transaction
///
/// This data travels FROM the miner TO the chain via submitauxblock.
/// ChainActor validates and queues them, then the proposer converts
/// them to EVM Withdrawals in the next block's execution_payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PegInInfo {
    /// Bitcoin transaction ID
    pub txid: bitcoin::Txid,

    /// Bitcoin block containing the deposit
    pub block_hash: bitcoin::BlockHash,

    /// Bitcoin block height
    pub block_height: u32,

    /// Amount deposited in satoshis
    pub amount: u64,

    /// Target EVM address (extracted from OP_RETURN)
    pub evm_account: Address,
}

/// Queued peg-in with miner fee recipient
///
/// When a miner submits a peg-in, we track who should receive
/// the compensation when the peg-in is included in a block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedPegIn {
    /// The peg-in information
    pub info: PegInInfo,

    /// Miner address to receive compensation
    pub fee_recipient: Address,

    /// Height at which this peg-in was queued
    pub queued_at_height: u64,
}

/// Peg-in compensation parameters
///
/// Configures how miners are compensated for including peg-ins.
/// These are governable parameters that can be changed by the federation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PegInCompensation {
    /// Percentage of peg-in amount paid to miner (basis points)
    /// e.g., 50 = 0.5%
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
            min_fee_satoshi: 1_000,      // 0.00001 BTC
            max_fee_satoshi: 10_000_000, // 0.1 BTC
        }
    }
}

impl PegInCompensation {
    /// Calculate miner fee for a given peg-in amount
    ///
    /// Fee = (amount * miner_fee_bps) / 10000, clamped to [min, max]
    pub fn calculate_fee(&self, amount: u64) -> u64 {
        let fee = (amount * self.miner_fee_bps) / 10_000;
        fee.clamp(self.min_fee_satoshi, self.max_fee_satoshi)
    }

    /// Calculate the net amount received by user after miner fee
    pub fn net_amount(&self, amount: u64) -> u64 {
        amount.saturating_sub(self.calculate_fee(amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_calculation() {
        let params = PegInCompensation::default();

        // Normal case: 0.5% of 1 BTC = 500,000 sats
        assert_eq!(params.calculate_fee(100_000_000), 500_000);

        // Min floor: 0.5% of 10,000 sats = 50 sats, but min is 1000
        assert_eq!(params.calculate_fee(10_000), 1_000);

        // Max cap: 0.5% of 100 BTC = 50M sats, but max is 10M
        assert_eq!(params.calculate_fee(10_000_000_000), 10_000_000);
    }

    #[test]
    fn test_net_amount() {
        let params = PegInCompensation::default();

        // 1 BTC deposit: user gets 0.995 BTC
        let deposit = 100_000_000u64;
        let fee = params.calculate_fee(deposit);
        let net = params.net_amount(deposit);
        assert_eq!(net, deposit - fee);
        assert_eq!(net, 99_500_000);
    }
}
