mod bitcoin_signing;
mod bitcoin_stream;
use thiserror::Error;
use bdk::bitcoin::hashes::hex::FromHex;
pub use bitcoin_stream::bitcoin;

use bitcoin::{Address as BitcoinAddress, BlockHash, Transaction, TxOut, Txid};
use bitcoin_stream::stream_blocks;
use bitcoincore_rpc::{Error as RpcError, RpcApi};
use ethers::prelude::*;
use futures::prelude::*;
use std::str::FromStr;
use tracing::warn;

pub use bitcoin_signing::{
    BitcoinSignatureCollector, BitcoinSigner, Federation, FeeRate,
    PartiallySignedTaprootTransaction, PublicKey as BitcoinPublicKey,
    SecretKey as BitcoinSecretKey, SingleMemberTransactionSignatures, Tree, UtxoManager,
};
pub use bitcoin_stream::BitcoinCore;

pub const REQUIRED_CONFIRMATIONS: u8 = 6;

pub fn wei_to_sats(wei: U256) -> u64 {
    // eth has 18 decimals, bitcoin 8 --> div by 10^10
    (wei / U256::from(10_000_000_000u64)).as_u64()
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Parsing failre")]
    ParsingError,
    #[error("DB access error")]
    DbError,
    #[error("Unknown or spent input")]
    UnknownOrSpentInput,
    #[error("Invalid number of signatures")]
    InvalidNumberOfSignatures,
    #[error("Missing signature")]
    MissingSignature,
    #[error("Txid was not found")]
    TxidNotFound,
    #[error("Bitcoin Error")]
    BitcoinError,
    #[error("Given signature does not match the given public key")]
    IncorrectSignature,
    #[error("Invalid witness length")]
    InvalidWitnessLength,
    #[error("Invalid witness script")]
    InvalidWitnessScript,
    #[error("Invalid witness script")]
    MissingPegoutProposal,
    #[error("Invalid pegout output")]
    InvalidPegoutOutput,
    #[error("Invalid pegout output count")]
    InvalidPegoutOutputCount,
    #[error("Invalid change output")]
    InvalidChangeOutput,
    #[error("Unspendable input")]
    UnspendableInput,
    #[error("Invalid transaction header")]
    InvalidTransactionHeader,
    #[error("Insufficient bitcoin confirmations ({0})")]
    InsufficientConfirmations(i32),
    #[error("Transaction is not a valid peg-in transaction")]
    NotAPegin,
    #[error("Rpc error: {0}")]
    RpcError(#[from] RpcError),
}

#[derive(Debug, Clone)]
pub struct PegInInfo {
    pub txid: Txid,
    pub block_hash: BlockHash,
    pub amount: u64,
    pub evm_account: H160,
    pub block_height: u32,
}

pub struct Bridge {
    pegin_address: BitcoinAddress,
    bitcoin_core: BitcoinCore,
}

impl Bridge {
    const BRIDGE_CONTRACT_ADDRESS: &'static str = "0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB";

    pub fn new(bitcoin_core: BitcoinCore, pegin_address: BitcoinAddress) -> Self {
        Self {
            pegin_address,
            bitcoin_core,
        }
    }

    pub async fn stream_blocks_for_pegins<F, R>(&self, start_height: u32, cb: F)
    where
        F: Fn(Vec<PegInInfo>, u32) -> R,
        R: Future<Output = ()>,
    {
        let mut stream = stream_blocks(
            self.bitcoin_core.clone(),
            start_height,
            REQUIRED_CONFIRMATIONS.into(),
        )
        .await;
        while let Some(x) = stream.next().await {
            let (block, height) = x.unwrap();
            let block_hash = block.block_hash();
            let pegins = block
                .txdata
                .iter()
                .filter_map(|tx| self.pegin_info(tx, block_hash, height))
                .collect();
            cb(pegins, height).await;
        }
        panic!("Unexpected end of stream");
    }

    pub fn get_confirmed_pegin_from_txid(
        &self,
        txid: &Txid,
        block_hash: &BlockHash,
    ) -> Result<PegInInfo, Error> {
        let block_info = self.bitcoin_core.rpc.get_block_header_info(block_hash)?;
        if block_info.confirmations < REQUIRED_CONFIRMATIONS.into() {
            return Err(Error::InsufficientConfirmations(block_info.confirmations));
        }

        let tx = self
            .bitcoin_core
            .rpc
            .get_raw_transaction(txid, Some(block_hash))?;

        self.pegin_info(&tx, *block_hash, block_info.height as u32)
            .ok_or(Error::NotAPegin)
    }

    pub fn fetch_transaction(&self, txid: &Txid, block_hash: &BlockHash) -> Option<Transaction> {
        let tx = self
            .bitcoin_core
            .rpc
            .get_raw_transaction(txid, Some(block_hash))
            .ok()?;

        Some(tx)
    }

    pub fn broadcast_signed_tx(&self, transaction: &Transaction) -> Result<Txid, Error> {
        self.bitcoin_core
            .rpc
            .send_raw_transaction(transaction)
            .map_err(|err| {
                warn!("send_raw_transaction error {err}");
                Error::BitcoinError
            })
    }

    fn pegin_info(
        &self,
        tx: &Transaction,
        block_hash: BlockHash,
        block_height: u32,
    ) -> Option<PegInInfo> {
        fn extract_evm_address(tx_out: &TxOut) -> Option<H160> {
            if !tx_out.script_pubkey.is_provably_unspendable() || !tx_out.script_pubkey.is_op_return() {
                return None;
            }
            let opreturn = tx_out.script_pubkey.to_asm_string();
            let parts = opreturn.split(' ');
            let op_return_parts = parts.collect::<Vec<&str>>();
            let op_return_hex_string = op_return_parts[op_return_parts.len() - 1].to_string();
            let data = Vec::from_hex(&op_return_hex_string);
            if let Err(_e) = data {
                return None;
            } 
            let opreturn_data = String::from_utf8(data.clone().unwrap());
            if let Err(_e) = opreturn_data.clone() {
                let address = H160::from_str(&op_return_hex_string);
                if let Err(_e) = address {
                    return None;
                }
                return Some(address.unwrap());
            } 
            let address_str = opreturn_data.unwrap();
            let address = H160::from_str(&address_str);
            if let Err(_e) = address {
                return None;
            }
            Some(address.unwrap())
        }

        let amount = tx
            .output
            .iter()
            .find(|output| {
                self.pegin_address
                    .matches_script_pubkey(&output.script_pubkey)
            })
            .map(|x| x.value)?;

        let evm_account = tx.output.iter().find_map(extract_evm_address)?;

        Some(PegInInfo {
            txid: tx.txid(),
            block_hash,
            block_height,
            amount,
            evm_account,
        })
    }

    pub fn filter_pegouts(receipts: Vec<TransactionReceipt>) -> Vec<TxOut> {
        // same as defined in `Bridge.sol`
        #[derive(Clone, Debug, EthEvent)]
        pub struct RequestPegOut {
            #[ethevent(indexed)]
            pub evm_address: Address,
            pub bitcoin_address: Bytes,
            pub value: U256,
        }

        let contract_address = Self::BRIDGE_CONTRACT_ADDRESS
            .parse::<Address>()
            .expect("Bridge address is valid");

        let mut pegouts = Vec::new();

        for receipt in receipts {
            if let Some(address) = receipt.to {
                // only check for pegouts to the bridge contract
                if address != contract_address {
                    continue;
                }
            }

            for log in receipt.logs {
                if let Ok(event) = parse_log::<RequestPegOut>(log) {
                    if let Some(address) = parse_bitcoin_address(event.bitcoin_address) {
                        let txout = TxOut {
                            script_pubkey: address.script_pubkey(),
                            value: wei_to_sats(event.value),
                        };

                        pegouts.push(txout);
                    }
                }
            }
        }

        pegouts
    }

    pub fn fee_rate(&self) -> FeeRate {
        self.bitcoin_core
            .rpc
            .estimate_smart_fee(1, None)
            .ok()
            .map(|x| x.fee_rate)
            .flatten()
            .map(|x| FeeRate::from_btc_per_kvb(x.to_btc() as f32))
            .unwrap_or(FeeRate::from_sat_per_vb(2.0))
    }
}

fn parse_bitcoin_address(data: Bytes) -> Option<BitcoinAddress> {
    let address_str = std::str::from_utf8(&data).ok()?;
    let address = BitcoinAddress::from_str(address_str).ok()?;
    Some(address.assume_checked())
}

#[cfg(test)]
mod tests {
    use self::bitcoin_signing::UtxoManager;
    use super::*;
    use bdk::bitcoin::address::NetworkUnchecked;
    use bdk::bitcoin::key::TapTweak;
    use bdk::bitcoin::key::UntweakedKeyPair;
    use bdk::bitcoin::secp256k1::Message;
    use bdk::bitcoin::sighash::Prevouts;
    use bdk::bitcoin::sighash::SighashCache;
    use bdk::bitcoin::sighash::TapSighashType;
    use bdk::bitcoin::TxIn;
    use bdk::bitcoin::Witness;
    use bdk::database::Database;
    use bitcoin::consensus::encode::deserialize;
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::secp256k1::SecretKey;
    use bitcoin::taproot::Signature as SchnorrSig;
    use bitcoin::{Network, ScriptBuf, Transaction};
    use bitcoincore_rpc::bitcoin::hashes::Hash;
    use bitcoincore_rpc::{bitcoin::Address, RpcApi};

    #[tokio::test]
    #[ignore]
    async fn test_stream_e2e() {
        let federation = Bridge::new(
            BitcoinCore::new("http://localhost:18443", "rpcuser", "rpcpassword"),
            "bcrt1pnv0qv2q86ny0my4tycezez7e72jnjns2ays3l4w98v6l383k2h7q0lwmyh"
                .parse::<BitcoinAddress<NetworkUnchecked>>()
                .unwrap()
                .assume_checked(),
        );

        federation
            .stream_blocks_for_pegins(0, |_, _| async {})
            .await;
    }

    #[test]
    fn test_pegin_info() {
        let raw_tx = hex::decode("02000000000101d590828406d3a14f06e41565d1ced296100350c2a04f11f1431f915b240ac48b0100000000fdffffff0310270000000000002251209b1e062807d4c8fd92ab26322c8bd9f2a5394e0ae9211fd5c53b35f89e3655fc0000000000000000166a14f9a9b63f5b7f9336da0ce520c6bec64627027f5b981f042a010000002251209a6580d80f882470a4d5cb994d057d457e0b5945e10ab4e8c0d64768202fe93b014053d2cf18b07206df067198db735562f9e6410be180771e29dbaf9cf5499a7ff179fb70405d05fc5c6def8efce3c3715af9a186ed1bc1bcb6649902b689026a3200000000").unwrap();
        let tx: Transaction = deserialize(&raw_tx).unwrap();

        let federation = Bridge::new(
            BitcoinCore::new("http://localhost:18443", "rpcuser", "rpcpassword"),
            "bcrt1pnv0qv2q86ny0my4tycezez7e72jnjns2ays3l4w98v6l383k2h7q0lwmyh"
                .parse::<BitcoinAddress<NetworkUnchecked>>()
                .unwrap()
                .assume_checked(),
        );
        let info = federation
            .pegin_info(&tx, BlockHash::all_zeros(), 0)
            .unwrap();
        println!("{info:?}");
    }

    #[tokio::test]
    #[ignore]
    async fn test_pegin() {
        let raw_tx = hex::decode("02000000000101d590828406d3a14f06e41565d1ced296100350c2a04f11f1431f915b240ac48b0100000000fdffffff0310270000000000002251209b1e062807d4c8fd92ab26322c8bd9f2a5394e0ae9211fd5c53b35f89e3655fc0000000000000000166a14f9a9b63f5b7f9336da0ce520c6bec64627027f5b981f042a010000002251209a6580d80f882470a4d5cb994d057d457e0b5945e10ab4e8c0d64768202fe93b014053d2cf18b07206df067198db735562f9e6410be180771e29dbaf9cf5499a7ff179fb70405d05fc5c6def8efce3c3715af9a186ed1bc1bcb6649902b689026a3200000000").unwrap();
        let tx: Transaction = deserialize(&raw_tx).unwrap();

        let federation = Bridge::new(
            BitcoinCore::new("http://localhost:18443", "rpcuser", "rpcpassword"),
            "bcrt1pnv0qv2q86ny0my4tycezez7e72jnjns2ays3l4w98v6l383k2h7q0lwmyh"
                .parse::<BitcoinAddress<NetworkUnchecked>>()
                .unwrap()
                .assume_checked(),
        );
        let info = federation
            .pegin_info(&tx, BlockHash::all_zeros(), 0)
            .unwrap();
        println!("{info:?}");
    }

    fn get_bitcoin_rpc() -> (bitcoincore_rpc::Client, String) {
        use bitcoincore_rpc::Auth;

        let walletname = "federation-test";
        let rpc = bitcoincore_rpc::Client::new(
            &format!("http://localhost:18443/wallet/{walletname}"),
            Auth::UserPass("rpcuser".into(), "rpcpassword".into()),
        )
        .unwrap();
        (rpc, walletname.to_string())
    }

    fn send_to_address(address: &Address, amount: u64) -> Transaction {
        let (rpc, walletname) = get_bitcoin_rpc();

        let _ = rpc.create_wallet(&walletname, None, None, None, None);
        let _ = rpc.load_wallet(&walletname);
        let funding_address = rpc.get_new_address(None, None).unwrap().assume_checked();
        rpc.generate_to_address(101, &funding_address).unwrap(); // fund the wallet
        let input_txid = rpc
            .send_to_address(
                &address,
                bitcoin::Amount::from_sat(amount),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        rpc.get_transaction(&input_txid, None)
            .unwrap()
            .transaction()
            .unwrap()
    }

    fn get_arbitrary_output() -> ScriptBuf {
        Address::from_str("tb1p5kaqsuted66fldx256lh3en4h9z4uttxuagkwepqlqup6hw639gskndd0z")
            .unwrap()
            .assume_checked()
            .script_pubkey()
    }

    #[test]
    fn test_bitcoin_signer() {
        let secp = Secp256k1::new();

        // some arbitrary keys..
        let secret_keys = [
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000003",
        ]
        .into_iter()
        .map(|x| SecretKey::from_str(x).unwrap())
        .collect::<Vec<_>>();
        let pubkeys = secret_keys
            .iter()
            .map(|x| x.public_key(&secp))
            .collect::<Vec<_>>();

        // generate the taproot info
        let federation = bitcoin_signing::Federation::new(pubkeys.clone(), 2, Network::Regtest);

        // use bitcoin-core to spend to our new address s.t. we have an utxo to spend
        let funding_tx = send_to_address(&federation.taproot_address, 10000000);

        // setup a wallet. For testing we use memorydb, in the product we'd use sled
        let mut wallet =
            UtxoManager::new_with_db(bdk::database::MemoryDatabase::new(), federation.clone());
        wallet.register_pegin(&funding_tx).unwrap();

        // helper struct to collect signatures from different authorities
        let mut signature_collector = BitcoinSignatureCollector::new(federation.clone());

        // generate some transaction that we want to send - unsigned
        let unsigned_tx = wallet
            .create_payment(
                vec![
                    TxOut {
                        script_pubkey: get_arbitrary_output(),
                        value: 5000000,
                    },
                    TxOut {
                        script_pubkey: get_arbitrary_output(),
                        value: 400000,
                    },
                ],
                FeeRate::from_sat_per_vb(2.0),
            )
            .unwrap();
        wallet.register_pegin(&unsigned_tx).unwrap();

        // sign with 1nd authority
        {
            let signer = bitcoin_signing::BitcoinSigner::new(secret_keys[1].clone());
            let sigs = signer.get_input_signatures(&wallet, &unsigned_tx).unwrap();
            signature_collector
                .add_signature(&wallet, unsigned_tx.txid(), sigs)
                .unwrap();
        }

        // sign with 2nd authority
        {
            let signer = bitcoin_signing::BitcoinSigner::new(secret_keys[2].clone());
            let sigs = signer.get_input_signatures(&wallet, &unsigned_tx).unwrap();
            signature_collector
                .add_signature(&wallet, unsigned_tx.txid(), sigs)
                .unwrap();
        }

        // add all collected signatures into the tx
        let signed_tx = signature_collector
            .get_finalized(unsigned_tx.txid())
            .unwrap();

        // Check the tx
        {
            wallet.check_transaction_signatures(&signed_tx).unwrap();
        }

        // Use bitcoin-core to send the tx, to see if it works
        get_bitcoin_rpc()
            .0
            .send_raw_transaction(&signed_tx)
            .unwrap();

        // see that getbalance works
        wallet.register_pegout(&signed_tx).unwrap();
        println!("Txid: {}", signed_tx.txid());
        println!("remaining balance: {}", wallet.get_balance().unwrap());
    }

    fn get_keypath_signing_inputs<T: Database>(
        wallet: &UtxoManager<T>,
        transaction: &Transaction,
    ) -> Result<Vec<Message>, Error> {
        let prevouts = transaction
            .input
            .iter()
            .map(|x| {
                wallet
                    .tree
                    .get_utxo(&x.previous_output)
                    .map_err(|_| Error::DbError)?
                    .ok_or(Error::UnknownOrSpentInput)
                    .map(|x| x.txout)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let prevouts = Prevouts::All(&prevouts);

        let mut sighash_cache = SighashCache::new(transaction);

        let messages = transaction
            .input
            .iter()
            .enumerate()
            .map(|(idx, _input)| {
                // get the data that we sign over
                let sighash_sig = sighash_cache
                    .taproot_key_spend_signature_hash(idx, &prevouts, TapSighashType::Default)
                    .unwrap();

                // get the hash of what we need to sign
                Message::from_slice(&sighash_sig.as_byte_array()[..]).unwrap()
            })
            .collect();

        Ok(messages)
    }

    /// A test that we keep around as documentation, showing how it's possible to
    /// spend from the multisig without using multiple signatures, by using the
    /// keypath spending using the internal key
    #[test]
    fn test_keypath_spending() {
        let secp = Secp256k1::new();

        let internal_keypair =
            SecretKey::from_str("1229101a0fcf2104e8808dab35661134aa5903867d44deb73ce1c7e4eb925be8")
                .unwrap()
                .keypair(&secp);

        // some arbitrary keys..
        let secret_keys = [
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000003",
        ]
        .into_iter()
        .map(|x| SecretKey::from_str(x).unwrap())
        .collect::<Vec<_>>();
        let pubkeys = secret_keys
            .iter()
            .map(|x| x.public_key(&secp))
            .collect::<Vec<_>>();

        // generate the taproot info
        let federation = bitcoin_signing::Federation::new_with_internal_pubkey(
            internal_keypair.public_key().into(),
            pubkeys.clone(),
            2,
            Network::Regtest,
        );

        // use bitcoin-core to spend to our new address s.t. we have an utxo to spend
        let funding_tx = send_to_address(&federation.taproot_address, 10000000);

        // setup a wallet. For testing we use memorydb, in the product we'd use sled
        let mut wallet =
            UtxoManager::new_with_db(bdk::database::MemoryDatabase::new(), federation.clone());
        wallet.register_pegin(&funding_tx).unwrap();

        // generate some transaction that we want to send - unsigned
        let unsigned_tx = wallet
            .create_payment(
                vec![
                    TxOut {
                        script_pubkey: get_arbitrary_output(),
                        value: 5000000,
                    },
                    TxOut {
                        script_pubkey: get_arbitrary_output(),
                        value: 400000,
                    },
                ],
                FeeRate::from_sat_per_vb(2.0),
            )
            .unwrap();
        wallet.register_pegin(&unsigned_tx).unwrap();

        let signing_inputs = get_keypath_signing_inputs(&wallet, &unsigned_tx).unwrap();

        let tweaked_keypair =
            UntweakedKeyPair::from_secret_key(&secp, &internal_keypair.secret_key())
                .tap_tweak(&secp, federation.spend_info.merkle_root());

        let signed_inputs = signing_inputs
            .into_iter()
            .zip(unsigned_tx.input.iter())
            .map(|(msg, txin)| {
                let sig = SchnorrSig {
                    sig: secp.sign_schnorr(&msg, &tweaked_keypair.to_inner()),
                    hash_ty: TapSighashType::Default,
                };
                TxIn {
                    witness: Witness::from_slice(&vec![sig.to_vec()]),
                    ..txin.clone()
                }
            })
            .collect();

        let signed_tx = Transaction {
            input: signed_inputs,
            ..unsigned_tx
        };

        // Use bitcoin-core to send the tx, to see if it works
        get_bitcoin_rpc()
            .0
            .send_raw_transaction(&signed_tx)
            .unwrap();
    }
}
