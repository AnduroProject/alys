use bdk::bitcoin::key::UntweakedPublicKey;
use bdk::miniscript::ToPublicKey;
pub use bdk::sled::{self, Tree};
pub use bdk::FeeRate;
pub use bitcoin::secp256k1::{PublicKey, SecretKey};

use crate::bitcoin;
use crate::Error;
use bdk::database::Database;
use bdk::wallet::coin_selection::{BranchAndBoundCoinSelection, CoinSelectionAlgorithm, Excess};
use bdk::{KeychainKind, LocalUtxo, WeightedUtxo};
use bitcoin::absolute::LockTime;
use bitcoin::key::KeyPair;
use bitcoin::opcodes::all;
use bitcoin::script::Builder;
use bitcoin::secp256k1::{
    schnorr::Signature as SchnorrSignature, All, Message, Secp256k1, XOnlyPublicKey,
};
use bitcoin::sighash::{Prevouts, ScriptPath, SighashCache, TapSighashType};
use bitcoin::taproot::{LeafVersion, Signature as SchnorrSig, TaprootBuilder, TaprootSpendInfo};
use bitcoin::{Address, Network, OutPoint, ScriptBuf, Transaction, TxIn, TxOut, Txid, Witness};
use bitcoincore_rpc::bitcoin::hashes::Hash;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::str::FromStr;

pub struct UtxoManager<T: Database> {
    pub(crate) tree: T,
    federation: Federation,
    secp: Secp256k1<All>,
}

impl UtxoManager<Tree> {
    pub fn new(db_name: &str, federation: Federation) -> Result<Self, Error> {
        let db = sled::open(db_name).map_err(|_| Error::DbError)?;
        let tree = db.open_tree("wallet").map_err(|_| Error::DbError)?;
        Ok(Self {
            tree,
            federation,
            secp: Secp256k1::new(),
        })
    }
}

impl<T: Database> UtxoManager<T> {
    const TRANSACTION_VERSION: i32 = 2;
    const LOCK_TIME: LockTime = LockTime::ZERO;

    pub fn new_with_db(db: T, federation: Federation) -> Self {
        Self {
            tree: db,
            federation,
            secp: Secp256k1::new(),
        }
    }

    /// Registers outputs to the federation from the given signed or unsigned transaction
    fn register_outputs_from(&mut self, transaction: &Transaction) -> Result<(), Error> {
        let outputs_to_federation = transaction.output.iter().enumerate().filter(|(_, txout)| {
            self.federation
                .taproot_address
                .matches_script_pubkey(&txout.script_pubkey)
        });

        for (vout, txout) in outputs_to_federation {
            // Note: even though the transaction may be unsigned, the signing does not change
            // the txid (it only adds witnesses). As such, we can already record the final
            // outpoint.
            let outpoint = OutPoint {
                txid: transaction.txid(),
                vout: vout as u32,
            };
            // technically the utxo may be `internal`` but idk if get_utxo would return
            // this utxo if we try fetching it with the wrong keychain kind, so for
            // now we set everything the external
            let keychain = KeychainKind::External;
            let utxo = LocalUtxo {
                txout: txout.clone(),
                outpoint,
                is_spent: false,
                keychain,
            };
            self.tree.set_utxo(&utxo).map_err(|_| Error::DbError)?;
        }

        Ok(())
    }

    pub fn register_pegin(&mut self, transaction: &Transaction) -> Result<(), Error> {
        // store tx
        self.tree
            .set_raw_tx(transaction)
            .map_err(|_| Error::DbError)?;
        // store outputs
        self.register_outputs_from(transaction)
    }

    /// note: transaction is expected to be unsigned, but signed transactions will work as well
    pub fn register_pegout(&mut self, transaction: &Transaction) -> Result<(), Error> {
        // store tx (even though we don't currently use stored pegout txs, only the utxos)
        self.tree
            .set_raw_tx(transaction)
            .map_err(|_| Error::DbError)?;

        // Mark the spent inputs as spent so we don't try to double spend
        for input in transaction.input.iter() {
            let mut utxo = self
                .tree
                .get_utxo(&input.previous_output)
                .map_err(|_| Error::DbError)?
                .ok_or(Error::DbError)?;
            utxo.is_spent = true;
            self.tree.set_utxo(&utxo).map_err(|_| Error::DbError)?;
        }

        self.register_outputs_from(transaction)
    }

    pub fn get_tx(&self, txid: &Txid) -> Result<Option<Transaction>, Error> {
        self.tree.get_raw_tx(txid).map_err(|_| Error::DbError)
    }

    pub fn check_payment_proposal(
        &self,
        required_outputs: Vec<TxOut>,
        pegout_proposal: Option<&Transaction>,
    ) -> Result<(), Error> {
        let tx = match pegout_proposal {
            None if required_outputs.is_empty() => return Ok(()),
            None => return Err(Error::MissingPegoutProposal),
            Some(ref proposal) => proposal,
        };

        let actual_outputs = &tx.output;

        if actual_outputs.len() == required_outputs.len() + 1 {
            // one more output: this has to be a change output
            if actual_outputs.last().unwrap().script_pubkey
                != self.federation.taproot_address.script_pubkey()
            {
                return Err(Error::InvalidChangeOutput);
            }
        } else if actual_outputs.len() != required_outputs.len() {
            return Err(Error::InvalidPegoutOutputCount);
        }

        // check that all outputs other than the change output are as expected
        // Note: we set an upper limit on the value but no lower limit, since
        // we don't know the amount of fees that have been subtracted
        if required_outputs
            .into_iter()
            .zip(actual_outputs.iter())
            .any(|(ref required, actual)| {
                required.script_pubkey != actual.script_pubkey || actual.value > required.value
            })
        {
            return Err(Error::InvalidPegoutOutput);
        }

        // check the inputs
        for input in tx.input.iter() {
            if !self.has_spendable_utxo(input.previous_output)? {
                return Err(Error::UnspendableInput);
            }
        }

        if tx.lock_time != Self::LOCK_TIME || tx.version != Self::TRANSACTION_VERSION {
            return Err(Error::InvalidTransactionHeader);
        }

        // note: we currently don't reject proposal based on fee amount

        Ok(())
    }

    /// Create a payment containing the given outputs. The set of utxos is not updated here,
    /// so this should be called at most once per block proposal. The utxo set should be
    /// updated from the import_block function through `UtxoManager::register_pegout`
    pub fn create_payment(
        &mut self,
        output: Vec<TxOut>,
        fee_rate: FeeRate,
    ) -> Result<Transaction, Error> {
        let num_pegouts = output.len() as u64;

        let utxos = self
            .tree
            .iter_utxos()
            .map_err(|_| Error::DbError)?
            .into_iter()
            .filter(|utxo| !utxo.is_spent)
            .map(|utxo| WeightedUtxo {
                satisfaction_weight: self.federation.satisfaction_weight,
                utxo: bdk::Utxo::Local(utxo),
            })
            .collect();

        let mut tx = Transaction {
            version: Self::TRANSACTION_VERSION,
            lock_time: Self::LOCK_TIME,
            input: vec![],
            output,
        };

        let total_out_value: u64 = tx.output.iter().map(|x| x.value).sum();

        let selected = BranchAndBoundCoinSelection::default()
            .coin_select(
                &self.tree, // note: this is not really used
                vec![],
                utxos,
                fee_rate,
                total_out_value,
                &self.federation.taproot_address.script_pubkey(),
            )
            .unwrap();

        // set the inputs
        tx.input = selected
            .selected
            .into_iter()
            .map(|x| TxIn {
                previous_output: x.outpoint(),
                script_sig: ScriptBuf::new(),
                sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::default(),
            })
            .collect();

        // set the change output, if any
        if let Excess::Change { amount, fee: _ } = selected.excess {
            tx.output.push(TxOut {
                script_pubkey: self.federation.taproot_address.script_pubkey(),
                value: amount,
            });
        }

        // deduct fees from the pegout outputs
        let total_weight = tx.weight();
        let total_fee = fee_rate.fee_wu(total_weight);
        // note: adding `num_pegouts - 1` to get a rounded up division
        let fee_per_output = (total_fee + num_pegouts - 1) / num_pegouts;
        for output in tx.output.iter_mut().take(num_pegouts as usize) {
            // todo: this will panic if fee > value
            output.value -= fee_per_output;
        }

        Ok(tx)
    }

    pub fn get_balance(&self) -> Result<u64, Error> {
        Ok(self
            .tree
            .iter_utxos()
            .map_err(|_| Error::DbError)?
            .into_iter()
            .filter(|utxo| !utxo.is_spent)
            .map(|utxo| utxo.txout.value)
            .sum())
    }

    pub fn get_transaction(&self, txid: &Txid) -> Result<Transaction, Error> {
        self.tree
            .get_raw_tx(txid)
            .map_err(|_| Error::DbError)?
            .ok_or(Error::DbError)
    }

    pub fn has_spendable_utxo(&self, outpoint: OutPoint) -> Result<bool, Error> {
        Ok(self
            .tree
            .get_utxo(&outpoint)
            .map_err(|_| Error::DbError)?
            .map(|x| !x.is_spent)
            .unwrap_or(false))
    }

    pub fn get_signing_inputs(&self, transaction: &Transaction) -> Result<Vec<Message>, Error> {
        let prevouts = transaction
            .input
            .iter()
            .map(|x| {
                self.tree
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
                    .taproot_script_spend_signature_hash(
                        idx,
                        &prevouts,
                        ScriptPath::with_defaults(&self.federation.redeem_script),
                        TapSighashType::Default,
                    )
                    .unwrap();

                // get the hash of what we need to sign
                Message::from_slice(&sighash_sig.as_byte_array()[..]).unwrap()
            })
            .collect();

        Ok(messages)
    }

    pub fn check_transaction_signatures(&self, transaction: &Transaction) -> Result<(), Error> {
        let signing_messages = self.get_signing_inputs(transaction)?;
        for (msg, input) in signing_messages.iter().zip(transaction.input.iter()) {
            let witnesses = input.witness.to_vec();

            let sigs = witnesses
                .iter()
                .zip(self.federation.pubkeys.iter().rev())
                .filter(|(witness, _)| !witness.is_empty())
                .collect::<Vec<_>>();

            // check that the lengths and number of signatures are as expected
            if witnesses.len() != self.federation.pubkeys.len() + 2
                || sigs.len() != self.federation.threshold
            {
                return Err(Error::InvalidWitnessLength);
            }

            // check that the actual signatures are correct
            for (witness, pubkey) in sigs {
                let sig =
                    SchnorrSignature::from_slice(witness).map_err(|_| Error::IncorrectSignature)?;
                self.secp
                    .verify_schnorr(&sig, msg, &pubkey.to_x_only_pubkey())
                    .map_err(|_| Error::IncorrectSignature)?;
            }

            let expected_tail = vec![
                self.federation.redeem_script.to_bytes(),
                self.federation.control_block_witness(),
            ];
            let actual_tail = witnesses
                .into_iter()
                .skip(self.federation.pubkeys.len())
                .collect::<Vec<_>>();
            if expected_tail != actual_tail {
                return Err(Error::InvalidWitnessScript);
            }
        }
        Ok(())
    }

    pub fn check_input_signatures(
        &self,
        transaction: &Transaction,
        signatures: &SingleMemberTransactionSignatures,
    ) -> Result<(), Error> {
        let signing_messages = self.get_signing_inputs(transaction)?;

        if signing_messages.len() != signatures.1.len() {
            return Err(Error::InvalidNumberOfSignatures);
        }

        let pubkey = &signatures.0.to_x_only_pubkey();

        let is_ok = signing_messages
            .iter()
            .zip(signatures.1.iter())
            .all(|(msg, sig)| self.secp.verify_schnorr(sig, msg, pubkey).is_ok());

        if is_ok {
            Ok(())
        } else {
            Err(Error::IncorrectSignature)
        }
    }
}

pub struct PartiallySignedTaprootTransaction {
    unsigned_transaction: Transaction,
    verified_signatures: HashMap<PublicKey, Vec<SchnorrSignature>>,
}

impl PartiallySignedTaprootTransaction {
    pub fn new(unsigned_transaction: Transaction) -> Self {
        Self {
            unsigned_transaction,
            verified_signatures: HashMap::new(),
        }
    }

    fn add_verified_signature(
        &mut self,
        pubkey: PublicKey,
        input_signatures: Vec<SchnorrSignature>,
    ) {
        self.verified_signatures.insert(pubkey, input_signatures);
    }

    fn get_sigs_for_input(
        &self,
        input_idx: usize,
        pubkeys: &[PublicKey],
    ) -> Result<Vec<Vec<u8>>, Error> {
        pubkeys
            .iter()
            .map(|pubkey| {
                match self.verified_signatures.get(pubkey) {
                    None => Ok(vec![]), // missing authority is ok
                    Some(sigs) => sigs
                        .get(input_idx)
                        .ok_or(Error::MissingSignature) // missing input is not ok
                        .map(|sig| {
                            SchnorrSig {
                                sig: *sig,
                                hash_ty: TapSighashType::Default,
                            }
                            .to_vec()
                        }),
                }
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn finalize_transaction(&self, federation: &Federation) -> Result<Transaction, Error> {
        if self.verified_signatures.len() != federation.threshold {
            return Err(Error::InvalidNumberOfSignatures);
        }

        let signed_inputs = self
            .unsigned_transaction
            .input
            .iter()
            .enumerate()
            .map(|(input_idx, tx_in)| -> Result<TxIn, Error> {
                let sigs = self.get_sigs_for_input(input_idx, &federation.pubkeys)?;
                let control = federation.control_block_witness();
                let redeem_script = federation.redeem_script.to_bytes();
                let witnesses = sigs
                    .into_iter()
                    .rev()
                    .chain([redeem_script, control].into_iter())
                    .collect::<Vec<_>>();

                Ok(TxIn {
                    witness: Witness::from_slice(&witnesses),
                    ..tx_in.clone()
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Transaction {
            input: signed_inputs,
            ..self.unsigned_transaction.clone()
        })
    }
}

#[derive(Clone, Debug)]
pub struct Federation {
    pub taproot_address: Address,
    pub(crate) spend_info: TaprootSpendInfo,
    redeem_script: ScriptBuf,
    threshold: usize,
    pubkeys: Vec<PublicKey>,
    pub satisfaction_weight: usize,
}

impl Federation {
    fn unspendable_pubkey() -> UntweakedPublicKey {
        // To disable keypath spending, we use an "unspendable" pubkey, or, more accurately,
        // we use a pubkey from a nothing-up-my-sleeve number as suggested by bip341:
        // https://en.bitcoin.it/wiki/BIP_0341
        // This BIP gives the following example:
        // - lift_x(0x50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0)
        // In order to avoid leaking the information that key path spending is not possible,
        // they recommend doing an additional operation on that point, but for now we don't
        // do that.
        // It wasn't immediately clear to me what `lift_x` is, but it turns out it is just
        // the (even) `(x,y)`` coordinate given `x`, see
        // https://bitcoin.stackexchange.com/questions/115611/how-does-the-bip340-lift-x-algorithm-work

        // IMPORTANT: someone with the private key corresponding to this public key will
        // be able to steal all funds.
        let x_coord = "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";
        XOnlyPublicKey::from_str(x_coord).unwrap()
    }

    pub fn new(pubkeys: Vec<PublicKey>, required_sigs: usize, network: Network) -> Self {
        Self::new_with_internal_pubkey(Self::unspendable_pubkey(), pubkeys, required_sigs, network)
    }

    pub(crate) fn new_with_internal_pubkey(
        internal_pubkey: UntweakedPublicKey,
        pubkeys: Vec<PublicKey>,
        required_sigs: usize,
        network: Network,
    ) -> Self {
        let secp = Secp256k1::new();

        let redeem_script = pubkeys
            .iter()
            .enumerate()
            .fold(Builder::new(), |builder, (idx, pubkey)| {
                builder
                    .push_x_only_key(&(*pubkey).into())
                    .push_opcode(if idx == 0 {
                        all::OP_CHECKSIG
                    } else {
                        all::OP_CHECKSIGADD
                    })
            })
            .push_int(required_sigs as i64)
            .push_opcode(all::OP_GREATERTHANOREQUAL)
            .into_script();

        let spend_info = TaprootBuilder::with_huffman_tree(vec![(1, redeem_script.clone())])
            .unwrap()
            .finalize(&secp, internal_pubkey)
            .unwrap();

        let taproot_address = Address::p2tr(
            &secp,
            spend_info.internal_key(),
            spend_info.merkle_root(),
            network,
        );

        let satisfaction_weight = {
            let num_omitted_sigs = pubkeys.len() - required_sigs;

            let control = spend_info
                .control_block(&(redeem_script.clone(), LeafVersion::TapScript))
                .unwrap()
                .serialize();
            let redeem_script = redeem_script.to_bytes();

            let non_empty_sigs = (0..required_sigs).map(|i| {
                SchnorrSig {
                    sig: SchnorrSignature::from_slice(&[i as u8; 64]).unwrap(),
                    hash_ty: TapSighashType::Default,
                }
                .to_vec()
            });

            let empty_sigs = (0..num_omitted_sigs).map(|_| vec![]);
            let all_witnesses = non_empty_sigs
                .chain(empty_sigs)
                .chain(vec![control, redeem_script])
                .collect::<Vec<_>>();

            let txin = TxIn {
                previous_output: OutPoint::default(),
                script_sig: ScriptBuf::new(),
                sequence: bitcoin::Sequence(0xFFFFFFFF),
                witness: Witness::from_slice(&all_witnesses),
            };
            txin.segwit_weight()
        };

        Self {
            taproot_address,
            spend_info,
            redeem_script,
            pubkeys,
            threshold: required_sigs,
            satisfaction_weight,
        }
    }
    fn control_block_witness(&self) -> Vec<u8> {
        self.spend_info
            .control_block(&(self.redeem_script.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize()
    }
}
pub struct BitcoinSigner {
    pub keypair: KeyPair,
    secp: Secp256k1<All>,
}

impl BitcoinSigner {
    pub fn new(private_key: SecretKey) -> Self {
        let secp = Secp256k1::new();
        Self {
            keypair: KeyPair::from_secret_key(&Secp256k1::new(), &private_key),
            secp,
        }
    }

    pub fn get_input_signatures<T: Database>(
        &self,
        wallet: &UtxoManager<T>,
        transaction: &Transaction,
    ) -> Result<SingleMemberTransactionSignatures, Error> {
        let signatures = wallet
            .get_signing_inputs(transaction)?
            .into_iter()
            .map(|msg| self.secp.sign_schnorr(&msg, &self.keypair))
            .collect();

        Ok(SingleMemberTransactionSignatures(
            self.keypair.public_key(),
            signatures,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SingleMemberTransactionSignatures(pub PublicKey, pub Vec<SchnorrSignature>);

pub struct BitcoinSignatureCollector {
    transactions: HashMap<Txid, PartiallySignedTaprootTransaction>,
    federation: Federation,
}

impl BitcoinSignatureCollector {
    pub fn new(federation: Federation) -> Self {
        Self {
            transactions: HashMap::new(),
            federation,
        }
    }

    pub fn cleanup_signatures_for(&mut self, txid: &Txid) {
        self.transactions.remove(txid);
    }

    /// Adds a set signatures to a psbt. It first checks the correctness of the signatures.
    pub fn add_signature<T: Database>(
        &mut self,
        wallet: &UtxoManager<T>,
        txid: Txid,
        signature: SingleMemberTransactionSignatures,
    ) -> Result<(), Error> {
        let psbt = match self.transactions.entry(txid) {
            Entry::Vacant(entry) => {
                let tx = wallet.get_transaction(&txid)?;
                let psbt = PartiallySignedTaprootTransaction::new(tx);
                entry.insert(psbt)
            }
            Entry::Occupied(entry) => entry.into_mut(),
        };

        if psbt.verified_signatures.len() >= self.federation.threshold {
            return Ok(());
        }

        wallet.check_input_signatures(&psbt.unsigned_transaction, &signature)?;
        psbt.add_verified_signature(signature.0, signature.1);

        Ok(())
    }

    pub fn get_finalized(&self, txid: Txid) -> Result<Transaction, Error> {
        let psbt = self.transactions.get(&txid).ok_or(Error::TxidNotFound)?;
        let tx = psbt.finalize_transaction(&self.federation)?;
        Ok(tx)
    }

    pub fn clear(&mut self) {
        self.transactions.clear();
    }
}
