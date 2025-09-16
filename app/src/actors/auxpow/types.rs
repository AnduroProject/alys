//! Core AuxPow types for V2 actor system
//!
//! Bitcoin merged mining data structures and algorithms migrated from legacy auxpow.rs
//! These are fundamental types used throughout the V2 AuxPow actor system.

use bitcoin::absolute::Height;
use bitcoin::block::Version;
use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::hashes::Hash;
use bitcoin::script::PushBytesBuf;
use bitcoin::{blockdata::block::Header, hash_types::TxMerkleNode, BlockHash, Transaction};
use bitcoin::{CompactTarget, ScriptBuf, Target, TxOut, VarInt};
use serde_derive::{Deserialize, Serialize};
use std::array::TryFromSliceError;
use tokio::time::Instant;
use tracing::*;

use crate::actors::auxpow::error::AuxPowError;

const MERGED_MINING_HEADER: [u8; 4] = [0xfa, 0xbe, b'm', b'm'];
const MERGED_MINING_HEADER_LENGTH: usize = 44;

/// Bitcoin merkle branch for proof validation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MerkleBranch {
    /// Individual hash in the branch
    pub branch_hash: Vec<TxMerkleNode>,
    /// Bitmask of which side of the merkle hash function the branch_hash element should go on.
    /// Zero means it goes on the right, One means on the left. It is equal to the index of the
    /// starting hash within the widest level of the merkle tree for this merkle branch.
    pub branch_side_mask: u32,
}

impl Encodable for MerkleBranch {
    fn consensus_encode<W: std::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, std::io::Error> {
        let mut len = 0;
        len += VarInt(self.branch_hash.len() as u64).consensus_encode(writer)?;
        for hash in self.branch_hash.iter() {
            len += hash.consensus_encode(writer)?;
        }
        len += self.branch_side_mask.consensus_encode(writer)?;
        Ok(len)
    }
}

impl Decodable for MerkleBranch {
    fn consensus_decode_from_finite_reader<R: std::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        Ok(Self {
            branch_hash: Decodable::consensus_decode_from_finite_reader(reader)?,
            branch_side_mask: Decodable::consensus_decode_from_finite_reader(reader)?,
        })
    }
}

impl MerkleBranch {
    /// Helper method to produce SHA256D(left + right)
    fn parent_hash(left: &TxMerkleNode, right: &TxMerkleNode) -> TxMerkleNode {
        let mut encoder = TxMerkleNode::engine();
        left.consensus_encode(&mut encoder)
            .expect("engines don't error");
        right
            .consensus_encode(&mut encoder)
            .expect("engines don't error");
        TxMerkleNode::from_engine(encoder)
    }

    fn check_merkle_branch(&self, mut hash: TxMerkleNode) -> TxMerkleNode {
        let b = self.branch_side_mask;
        let _length = self.branch_hash.len();
        for (n, branch_hash) in self.branch_hash.iter().enumerate() {
            let parent_hash = if b >> n & 1 == 1 {
                // left
                MerkleBranch::parent_hash(branch_hash, &hash)
            } else {
                // right
                MerkleBranch::parent_hash(&hash, branch_hash)
            };

            hash = parent_hash;
        }
        hash
    }
}

/// Merged mining header embedded in Bitcoin coinbase transactions
#[derive(Debug)]
struct MergedMiningHeader {
    magic: [u8; 4],
    block_hash: BlockHash,
    merkle_size: u32,
    merkle_nonce: u32,
}

impl MergedMiningHeader {
    #[allow(dead_code)]
    fn from_script_sig(script_sig: &ScriptBuf) -> Result<Self, AuxPowError> {
        let mut header = None;
        let mut instructions = script_sig.instructions();
        while let Some(Ok(instruction)) = instructions.next() {
            if let Some(bytes) = instruction.push_bytes() {
                if bytes.len() == MERGED_MINING_HEADER_LENGTH
                    && bytes[0..4].as_bytes().eq(&MERGED_MINING_HEADER)
                {
                    if header.is_some() {
                        return Err(AuxPowError::InvalidAuxpow);
                    }
                    header = Some(bytes.as_bytes().to_vec());
                }
            }
        }

        let header = header.ok_or(AuxPowError::InvalidAuxpow)?;

        // convert to big endian
        let mut raw_block_hash: Vec<u8> = header[4..36].to_vec();
        raw_block_hash.reverse();

        Ok(Self {
            magic: header[0..4].try_into().map_err(|_| AuxPowError::InvalidAuxpow)?,
            block_hash: BlockHash::from_slice(&raw_block_hash).map_err(|_| AuxPowError::InvalidAuxpow)?,
            merkle_size: u32::from_le_bytes(header[36..40].try_into().map_err(|_| AuxPowError::InvalidAuxpow)?),
            merkle_nonce: u32::from_le_bytes(header[40..44].try_into().map_err(|_| AuxPowError::InvalidAuxpow)?),
        })
    }

    fn from_script_pub_key(
        script_pub_key: &ScriptBuf,
        root_hash: &BlockHash,
    ) -> Result<Self, AuxPowError> {
        let header = script_pub_key.as_bytes().to_vec();
        if header.len() != MERGED_MINING_HEADER_LENGTH {
            return Err(AuxPowError::InvalidAuxpow);
        }

        let magic = header[0..4].try_into().map_err(|_| AuxPowError::InvalidAuxpow)?;
        if magic != MERGED_MINING_HEADER {
            return Err(AuxPowError::InvalidAuxpow);
        }

        // convert to big endian
        let mut raw_block_hash: Vec<u8> = header[4..36].to_vec();
        raw_block_hash.reverse();
        let block_hash = BlockHash::from_slice(&raw_block_hash).map_err(|_| AuxPowError::InvalidAuxpow)?;

        if block_hash != *root_hash {
            return Err(AuxPowError::InvalidAuxpow);
        }

        Ok(Self {
            magic,
            block_hash,
            merkle_size: u32::from_le_bytes(header[36..40].try_into().map_err(|_| AuxPowError::InvalidAuxpow)?),
            merkle_nonce: u32::from_le_bytes(header[40..44].try_into().map_err(|_| AuxPowError::InvalidAuxpow)?),
        })
    }

    fn to_script_pub_key(&self) -> ScriptBuf {
        let mut header = PushBytesBuf::new();
        header.extend_from_slice(&self.magic).unwrap();
        header.extend_from_slice(&self.block_hash[..]).unwrap();
        header
            .extend_from_slice(&self.merkle_size.to_le_bytes())
            .unwrap();
        header
            .extend_from_slice(&self.merkle_nonce.to_le_bytes())
            .unwrap();

        ScriptBuf::from_bytes(header.into())
    }
}

trait CoinbaseParser {
    fn parse_coinbase(
        tx: &Transaction,
        vch_root_hash: &BlockHash,
    ) -> Result<MergedMiningHeader, AuxPowError>;
}

pub enum CoinbaseVin {}
pub enum CoinbaseVout {}

impl CoinbaseParser for CoinbaseVin {
    fn parse_coinbase(
        tx: &Transaction,
        _root_hash: &BlockHash,
    ) -> Result<MergedMiningHeader, AuxPowError> {
        if tx.input.is_empty() {
            return Err(AuxPowError::InvalidAuxpow);
        }
        MergedMiningHeader::from_script_sig(&tx.input[0].script_sig)
    }
}

impl CoinbaseParser for CoinbaseVout {
    fn parse_coinbase(
        tx: &Transaction,
        root_hash: &BlockHash,
    ) -> Result<MergedMiningHeader, AuxPowError> {
        match MergedMiningHeader::from_script_pub_key(
            &tx.output
                .get(2)
                .ok_or(AuxPowError::InvalidAuxpow)?
                .script_pubkey,
            root_hash,
        ) {
            Ok(header) => Ok(header),
            Err(_) => MergedMiningHeader::from_script_pub_key(
                &tx.output
                    .get(3)
                    .ok_or(AuxPowError::InvalidAuxpow)?
                    .script_pubkey,
                root_hash,
            ),
        }
    }
}

// https://en.bitcoin.it/wiki/Merged_mining_specification
/// Bitcoin auxiliary proof-of-work for merged mining
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AuxPow {
    /// Coinbase transaction that is in the parent block, linking the AuxPOW block to its parent block
    pub coinbase_txn: Transaction,
    /// Hash of the parent_block header
    pub block_hash: BlockHash,
    /// The merkle branch linking the coinbase_txn to the parent block's merkle_root
    pub coinbase_branch: MerkleBranch,
    /// The merkle branch linking this auxiliary blockchain to the others, when used in a merged mining setup with multiple auxiliary chains
    pub blockchain_branch: MerkleBranch,
    /// Parent block header
    pub parent_block: Header,
}

impl Encodable for AuxPow {
    fn consensus_encode<W: std::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.coinbase_txn.consensus_encode(writer)?;
        len += self.block_hash.consensus_encode(writer)?;
        len += self.coinbase_branch.consensus_encode(writer)?;
        len += self.blockchain_branch.consensus_encode(writer)?;
        len += self.parent_block.consensus_encode(writer)?;
        Ok(len)
    }
}

impl Decodable for AuxPow {
    fn consensus_decode_from_finite_reader<R: std::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        Ok(Self {
            coinbase_txn: Decodable::consensus_decode_from_finite_reader(reader)?,
            block_hash: Decodable::consensus_decode_from_finite_reader(reader)?,
            coinbase_branch: Decodable::consensus_decode_from_finite_reader(reader)?,
            blockchain_branch: Decodable::consensus_decode_from_finite_reader(reader)?,
            parent_block: Decodable::consensus_decode_from_finite_reader(reader)?,
        })
    }
}

impl AuxPow {
    /// Extract parent chain ID from block version
    pub fn get_parent_chain_id(&self) -> u32 {
        self.parent_block.version.to_consensus() as u32 / (1 << 16)
    }

    /// Aggregate multiple block hashes into a single hash
    pub(crate) fn aggregate_hash(hashes: &[BlockHash]) -> BlockHash {
        let mut engine = BlockHash::engine();
        hashes
            .iter()
            .map(|hash| BlockHash::consensus_encode(hash, &mut engine))
            .collect::<Result<Vec<_>, _>>()
            .expect("engines don't error");
        BlockHash::from_engine(engine)
    }

    /// Validate AuxPow structure and merkle proofs
    pub fn check(&self, hash_aux_block: BlockHash, chain_id: u32) -> Result<(), AuxPowError> {
        self._check::<CoinbaseVout>(hash_aux_block, chain_id)
    }

    fn _check<C: CoinbaseParser>(
        &self,
        hash_aux_block: BlockHash,
        chain_id: u32,
    ) -> Result<(), AuxPowError> {
        if self.get_parent_chain_id() == chain_id {
            return Err(AuxPowError::InvalidAuxpow);
        }

        if self.blockchain_branch.branch_hash.len() > 30 {
            return Err(AuxPowError::InvalidAuxpow);
        }

        let n_root_hash = self
            .blockchain_branch
            .check_merkle_branch(TxMerkleNode::from_raw_hash(hash_aux_block.to_raw_hash()));

        let mut vch_root_hash = *n_root_hash.as_byte_array();
        vch_root_hash.reverse();
        let vch_root_hash = BlockHash::from_byte_array(vch_root_hash);

        if self
            .coinbase_branch
            .check_merkle_branch(TxMerkleNode::from_raw_hash(
                self.coinbase_txn.txid().to_raw_hash(),
            ))
            != self.parent_block.merkle_root
        {
            return Err(AuxPowError::InvalidAuxpow);
        }

        let header = C::parse_coinbase(&self.coinbase_txn, &vch_root_hash)?;

        if header.block_hash != vch_root_hash {
            return Err(AuxPowError::InvalidAuxpow);
        }

        let merkle_height = self.blockchain_branch.branch_hash.len();
        if header.merkle_size != (1 << merkle_height) {
            return Err(AuxPowError::InvalidAuxpow);
        }

        if self.blockchain_branch.branch_side_mask as u64
            != Self::get_expected_index(header.merkle_nonce, chain_id, merkle_height)
        {
            return Err(AuxPowError::InvalidAuxpow);
        }

        Ok(())
    }

    fn get_expected_index(nonce: u32, chain_id: u32, h: usize) -> u64 {
        let m = 1 << h;

        let mut rand = nonce as u64;
        rand = rand * 1103515245 + 12345;
        rand %= m;
        rand += chain_id as u64;
        rand = rand * 1103515245 + 12345;
        rand %= m;

        rand
    }

    /// Check if the parent block meets the difficulty target
    pub fn check_proof_of_work(&self, bits: CompactTarget) -> bool {
        let diff_target = Target::from_compact(bits);
        trace!("Checking PoW target with target of: {:?}", diff_target);
        diff_target.is_met_by(self.parent_block.block_hash())
    }

    /// Mine a new AuxPow for the given sidechain hash and target
    pub async fn mine(sidechain_hash: BlockHash, target: CompactTarget, chain_id: u32) -> Self {
        trace!("Mining AuxPow with target: {}", target.to_consensus());
        let parent_chainid = 1u32;

        let transaction = Transaction {
            lock_time: bitcoin::absolute::LockTime::Blocks(Height::MIN),
            version: 0,
            input: vec![],
            output: vec![
                TxOut::default(),
                TxOut::default(),
                TxOut {
                    value: 0,
                    script_pubkey: MergedMiningHeader {
                        magic: MERGED_MINING_HEADER,
                        block_hash: sidechain_hash,
                        merkle_nonce: 0,
                        merkle_size: 1,
                    }
                    .to_script_pub_key(),
                },
            ],
        };

        let parent_block = Header {
            version: Version::from_consensus((parent_chainid as i32) * (1 << 16)),
            bits: CompactTarget::from_consensus(0),
            merkle_root: TxMerkleNode::from_raw_hash(transaction.txid().to_raw_hash()),
            nonce: 0,
            prev_blockhash: BlockHash::all_zeros(),
            time: 0,
        };

        let mut aux_pow = AuxPow {
            parent_block,
            coinbase_txn: transaction,
            block_hash: BlockHash::all_zeros(),
            coinbase_branch: MerkleBranch {
                branch_hash: vec![],
                branch_side_mask: 0,
            },
            blockchain_branch: MerkleBranch {
                branch_hash: vec![],
                branch_side_mask: 0,
            },
        };

        let start = Instant::now();
        for nonce in 0..u32::MAX {
            tokio::task::yield_now().await;

            aux_pow.parent_block.nonce = nonce;
            if aux_pow.check_proof_of_work(target) {
                aux_pow.check(sidechain_hash, chain_id).unwrap();
                info!("Mining took {}ms", start.elapsed().as_millis());
                return aux_pow;
            }
        }

        panic!("No valid PoW found after trying all nonces")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::{
        consensus::encode::{deserialize, serialize},
        hashes::{sha256d, Hash},
    };
    use lighthouse_facade::types::{Hash256, Uint256};

    #[tokio::test]
    async fn test_miner() {
        let sidechain_blockhash = Hash256::random();

        let sidechain_hash =
            sha256d::Hash::from_byte_array(sidechain_blockhash.to_fixed_bytes()).into();
        let chain_id = 0;
        let target = bitcoin::CompactTarget::from_consensus(0x207fffff); // Easy target

        let aux_pow = AuxPow::mine(sidechain_hash, target, chain_id).await;

        aux_pow.check(sidechain_hash, chain_id).unwrap();
        assert!(aux_pow.check_proof_of_work(target));
    }

    #[test]
    fn check_nmc_auxpow() {
        // namecoin block height #19200
        let nmc_auxpow = AuxPow {
            coinbase_txn: deserialize(&hex::decode("01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff35045dee091a014d522cfabe6d6dd8a7c3e01e1e95bcee015e6fcc7583a2ca60b79e5a3aa0a171eddd344ada903d0100000000000000ffffffff0160a0102a01000000434104f8bbe97ed2acbc5bba11c68f6f1a0313f918f3d3c0e8475055e351e3bf442f8c8dcee682d2457bdc5351b70dd9e34026766eba18b06eaee2e102efd1ab634667ac00000000").unwrap()).unwrap(),
            block_hash: BlockHash::all_zeros(),
            coinbase_branch: MerkleBranch {
                branch_hash: vec![
                    "cbc33f5a42a09c72249b37d6728c968152939f1a5b558ec4e0bce1a1a1c40a05",
                    "34300f062161ab7323adb51e558969ef9a4846b1c721cf0628a25eb348d33c43",
                    "06850c5e3a186519743bf357d165ec2018fc64edea576ce6437d21c05787641d",
                    "56f309bdf576f2c985f20f93d96ba4ab978fd40450c71c2d0147f5e2df0226ac",
                    "5bd32251e560db7cb3da7f9b83b2e3b06ffc6d9497bf62cbc75ed679457219df"
                ].into_iter().map(|data| {
                    let mut bytes = hex::decode(data).unwrap();
                    bytes.reverse();
                    deserialize(&bytes).unwrap()
                }).collect(),
                branch_side_mask: 0,
            },
            blockchain_branch: MerkleBranch { branch_hash: vec![], branch_side_mask: 0 },
            parent_block: deserialize(&hex::decode("0100000008be13295c03e67cb70d00dae81ea06e78b9014e5ceb7d9ba504000000000000e0fd42db8ef6d783f079d126bea12e2d10c104c0927cd68f954d856f9e8111e59a23904e5dee091a1c655086").unwrap()).unwrap(),
        };

        let nmc_block_hash = deserialize(
            &hex::decode("d8a7c3e01e1e95bcee015e6fcc7583a2ca60b79e5a3aa0a171eddd344ada903d")
                .unwrap(),
        )
        .unwrap();

        nmc_auxpow
            ._check::<CoinbaseVin>(nmc_block_hash, 1)
            .expect("AuxPow is valid");

        let nmc_diff_target = Target::from_be_bytes(
            hex::decode("000000000000b269000000000000000000000000000000000000000000000000")
                .unwrap()
                .try_into()
                .unwrap(),
        );
        assert_eq!(nmc_diff_target.difficulty_float(), 94035.90217415104);

        assert!(
            nmc_auxpow.check_proof_of_work(nmc_diff_target.to_compact_lossy()),
            "Namecoin block is merge mined by Bitcoin"
        );
    }

    #[test] 
    fn should_decode_multiple_headers() {
        let expected_block_hash = BlockHash::from_byte_array([1; 32]);
        let transaction = Transaction {
            lock_time: bitcoin::absolute::LockTime::Blocks(Height::MIN),
            version: 0,
            input: vec![],
            output: vec![
                TxOut::default(),
                TxOut::default(),
                TxOut {
                    value: 0,
                    script_pubkey: MergedMiningHeader {
                        magic: MERGED_MINING_HEADER,
                        block_hash: BlockHash::from_byte_array([0; 32]),
                        merkle_nonce: 0,
                        merkle_size: 1,
                    }
                    .to_script_pub_key(),
                },
                TxOut {
                    value: 0,
                    script_pubkey: MergedMiningHeader {
                        magic: MERGED_MINING_HEADER,
                        block_hash: expected_block_hash,
                        merkle_nonce: 0,
                        merkle_size: 1,
                    }
                    .to_script_pub_key(),
                },
            ],
        };
        let header = CoinbaseVout::parse_coinbase(&transaction, &expected_block_hash).unwrap();
        assert_eq!(header.block_hash, expected_block_hash);
    }
}