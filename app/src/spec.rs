use bls::PublicKey;
use bridge::BitcoinPublicKey;
use ethereum_types::Address;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};

use crate::auxpow_miner::BitcoinConsensusParams;

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
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
}

pub const DEV_SECRET_KEY: &str = "2dcc87facf0963070b0422ab170e9cf219d885206d9d8fd06eaa643848cc7c63";

pub const DEV_BITCOIN_SECRET_KEY: &str = "037d0b6371526997dca0bb59ae44be0b0d6ee812393d996d749c0ef85d8adaea";

pub static DEV: Lazy<ChainSpec> = Lazy::new(|| {
    ChainSpec {
        slot_duration: 10000,
        authorities: vec![
            PublicKey::from_str(
                "0xa7f3f50888a4548114709476738555a01ed83eef8b0d0b45e50c4c224ee86d98d6ea69a2d3e619aed8693d59411d1ae2"
            ).unwrap()
        ],
        federation: vec![
            "2e80ab37dfb510a64526296fd1f295c42ef19c29".parse().unwrap(),
        ],
        federation_bitcoin_pubkeys: vec![
            BitcoinPublicKey::from_str("02767b3ebfdee7190772742cbeacf678e21f1aa043b66e8bfd6d07ac9e50b0049a").unwrap()
        ],
        bits: 505794034,
        chain_id: 212121,
        max_blocks_without_pow: 20,
        bitcoin_start_height: 0,
        retarget_params: BitcoinConsensusParams {
            pow_limit: 553713663,
            pow_target_timespan: 120000,
            pow_target_spacing: 10000,
            pow_no_retargeting: true,
        },
        is_validator: true
    }
});

pub fn genesis_value_parser(s: &str) -> eyre::Result<ChainSpec, eyre::Error> {
    Ok(match s {
        "dev" => DEV.clone(),
        _ => {
            let raw = std::fs::read_to_string(PathBuf::from(s))?;
            serde_json::from_str(&raw)?
        }
    })
}
