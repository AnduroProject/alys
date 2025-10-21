use bridge::BitcoinPublicKey;
use ethereum_types::Address;
use lighthouse_wrapper::bls::PublicKey;
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
    /// Number of confirmations required for a transaction to be considered final
    pub required_btc_txn_confirmations: u16,
}

pub const DEV_SECRET_KEY: &str = "0000000000000000000000000000000000000000000000000000000000000001";

pub const DEV_BITCOIN_SECRET_KEY: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";

// Dev-Regtest keys for two-validator federation
pub const DEV_REGTEST_AURA_SECRET_KEY_NODE1: &str = "1eb37c7780cae17cf6dfb2fd8b93595e4c2810d8632277f70336e14c8b9446e5";
pub const DEV_REGTEST_AURA_SECRET_KEY_NODE2: &str = "5d4d847ef298b175f2f4c8df9d7e2581edc7529633b1851cc06f2c76af902ed8";
pub const DEV_REGTEST_BITCOIN_SECRET_KEY_NODE1: &str = "8e06f3b7bd261c8dba364b5bd03307d6d2f3dfe567f94f5b190f779dc85081f7";
pub const DEV_REGTEST_BITCOIN_SECRET_KEY_NODE2: &str = "0e84e14debf28b663c7f7a29c203649a846fcd57f2989dd429415496e5897b73";

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
        chain_id: 121212,
        max_blocks_without_pow: 50000,
        required_btc_txn_confirmations: 144,
        bitcoin_start_height: 0, // 95800, // TODO: change when deploying new testnet4
        retarget_params: BitcoinConsensusParams {
            pow_no_retargeting: false,
            pow_limit: 553713663,
            pow_lower_limit: 439495319,
            max_pow_adjustment: 20,
            pow_target_timespan: 60,
            pow_target_spacing: 5
        },
        is_validator: true,
        execution_timeout_length: 3,
    }
});

pub static DEV_REGTEST: Lazy<ChainSpec> = Lazy::new(|| {
    ChainSpec {
        slot_duration: 4000,
        authorities: vec![
            PublicKey::from_str("0xb3c0fc40650f68a271c7c2c164d82c56e20ec371fedcc4576efaa3442f6461ee8b4b86100469eeacd4ea2b383bbaddf9").unwrap(),
            PublicKey::from_str("0xb25acafd0128182e6f87fa46334c8239e7f265d75b8c82ed0fdb2ca531a3761ae1b3eacae74761592aff2ed58827d06f").unwrap()
        ],
        federation: vec![
            "083bf0043a3f5dfb90517bbee09ee1c9c15f29f0".parse().unwrap(),
            "4d2e64af4ec6cb4a091849bc400d8bf85faef25a".parse().unwrap()
        ],
        federation_bitcoin_pubkeys: vec![
            BitcoinPublicKey::from_str("02cf74e0950ef067911150603b1a6633c4b850719cb381ae5e58b020aef94d5bac").unwrap(),
            BitcoinPublicKey::from_str("03dc1bc27968947f06cb6dbb9c3107a3a349810f5d563b302acff6a8e3d423b4c5").unwrap()
        ],
        bits: 505794034,
        chain_id: 121212,
        max_blocks_without_pow: 50000,
        required_btc_txn_confirmations: 144,
        bitcoin_start_height: 0,
        retarget_params: BitcoinConsensusParams {
            pow_no_retargeting: false,
            pow_limit: 553713663,
            pow_lower_limit: 439495319,
            max_pow_adjustment: 20,
            pow_target_timespan: 60,
            pow_target_spacing: 5
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
        "dev-regtest" => DEV_REGTEST.clone(),
        _ => {
            let raw = std::fs::read_to_string(PathBuf::from(s))?;
            serde_json::from_str(&raw)?
        }
    })
}

pub fn hex_file_parser(path: &str) -> eyre::Result<[u8; 32], eyre::Error> {
    Ok(hex::decode(&std::fs::read_to_string(PathBuf::from(path))?)?
        .try_into()
        .expect("Expected 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn should_successfully_decode_hex_file() {
        const HEX_STRING: &str = "This is a 32-byte long string!!!";
        let start_hex_bytes = HEX_STRING.as_bytes();

        println!("*****A) {:?}", start_hex_bytes.len());
        // debug!(start_hex_bytes.len());

        let dir = tempdir().unwrap();

        let file_path = dir.path().join("test_hex.hex");
        let mut file = File::create(file_path.clone()).unwrap();
        write!(file, "{}", hex::encode(HEX_STRING)).unwrap();

        let hex_bytes = hex_file_parser(file_path.to_str().unwrap()).unwrap();

        assert_eq!(start_hex_bytes.len(), hex_bytes.len());
        for i in 0..start_hex_bytes.len() {
            assert_eq!(start_hex_bytes[i], hex_bytes[i]);
        }

        drop(file);
        dir.close().unwrap();
    }
}
