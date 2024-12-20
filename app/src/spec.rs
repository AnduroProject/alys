use bls::PublicKey;
use bridge::BitcoinPublicKey;
use ethereum_types::Address;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};

use crate::auxpow_miner::BitcoinConsensusParams;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChainSpec {
    /// Block duration, milliseconds
    pub slot_duration: u64,
    /// Valid authorities
    pub authorities: Vec<PublicKey>,
    /// Federation accounts
    pub federation: Vec<Address>,
    /// Federation pubkeys used for the bitcoin handling
    pub federation_bitcoin_pubkeys: Vec<BitcoinPublicKey>,
    /// Bitcoin difficulty target in compact form
    pub bits: u32,
    /// Chain ID
    pub chain_id: u32,
    /// Stalls block production without AuxPow
    pub max_blocks_without_pow: u64,
    /// Starts processing from this height
    pub bitcoin_start_height: u32,
    /// Configuration of the retargeting algorithm
    pub retarget_params: BitcoinConsensusParams,
    /// Variable to identify node type 0 - full node, 1 - validator node
    pub is_validator: bool,
    /// The multiplier that determines how long the consensus engine will wait on the execution layer
    pub execution_timeout_length: u16,
}

pub const DEV_SECRET_KEY: &str = "0000000000000000000000000000000000000000000000000000000000000001";

pub const DEV_BITCOIN_SECRET_KEY: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";

pub static DEV: Lazy<ChainSpec> = Lazy::new(|| {
    ChainSpec {
        slot_duration:4000,
        authorities: vec![
            PublicKey::from_str(
                "0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
            ).unwrap()
        ],
        federation: vec![
            "2e80ab37dfb510a64526296fd1f295c42ef19c29".parse().unwrap(),
        ],
        federation_bitcoin_pubkeys: vec![
            BitcoinPublicKey::from_str("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798").unwrap()
        ],
        bits: 505794034,
        chain_id: 212121,
        max_blocks_without_pow: 20000,
        bitcoin_start_height: 0,
        retarget_params: BitcoinConsensusParams {
            pow_limit: 553713663,
            pow_target_timespan: 120000,
            pow_target_spacing: 10000,
            pow_no_retargeting: true,
        },
        is_validator: true,
        execution_timeout_length: 3,
    }
});

impl Default for ChainSpec {
    fn default() -> Self {
        DEV.clone()
    }
}
pub fn genesis_value_parser(s: &str) -> eyre::Result<ChainSpec, eyre::Error> {
    Ok(match s {
        "dev" => DEV.clone(),
        _ => {
            let raw = std::fs::read_to_string(PathBuf::from(s))?;
            serde_json::from_str(&raw)?
        }
    })
}
