use bitcoin::absolute::Height;
use bitcoin::block::Version;
use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::hashes::{Error as BitcoinHashesError, Hash};
use bitcoin::script::PushBytesBuf;
use bitcoin::{blockdata::block::Header, hash_types::TxMerkleNode, BlockHash, Transaction};
use bitcoin::{CompactTarget, ScriptBuf, Target, TxOut, VarInt};
use serde_derive::{Deserialize, Serialize};
use std::array::TryFromSliceError;
use tokio::time::Instant;
use tracing::*;

const MERGED_MINING_HEADER: [u8; 4] = [0xfa, 0xbe, b'm', b'm'];
const MERGED_MINING_HEADER_LENGTH: usize = 44;

#[derive(Debug)]
pub enum AuxPowError {
    MultipleHeaders,
    MissingHeader,
    InvalidBlockHash,
    InvalidSlice,
    ParentHasChainId,
    MerkleBranchTooLong,
    MerkleRootIncorrect,
    CoinbaseNoInputs,
    CoinbaseInvalidOutputs,
    MissingMerkleRoot,
    InvalidMerkleSize,
    WrongIndex,
}

impl From<BitcoinHashesError> for AuxPowError {
    fn from(_: BitcoinHashesError) -> Self {
        Self::InvalidBlockHash
    }
}

impl From<TryFromSliceError> for AuxPowError {
    fn from(_: TryFromSliceError) -> Self {
        Self::InvalidSlice
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MerkleBranch {
    // pub branch_length: VarInt,
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
                        return Err(AuxPowError::MultipleHeaders);
                    }
                    // TODO: check that it starts early in the coinbase
                    header = Some(bytes.as_bytes().to_vec());
                }
            }
        }

        let header = header.ok_or(AuxPowError::MissingHeader)?;

        // convert to big endian
        let mut raw_block_hash: Vec<u8> = header[4..36].to_vec();
        raw_block_hash.reverse();

        Ok(Self {
            magic: header[0..4].try_into()?, // fabe6d6d
            block_hash: BlockHash::from_slice(&raw_block_hash)?,
            merkle_size: u32::from_le_bytes(header[36..40].try_into()?),
            merkle_nonce: u32::from_le_bytes(header[40..44].try_into()?),
        })
    }

    fn from_script_pub_key(
        script_pub_key: &ScriptBuf,
        root_hash: &BlockHash,
    ) -> Result<Self, AuxPowError> {
        let header = script_pub_key.as_bytes().to_vec();
        if header.len() != MERGED_MINING_HEADER_LENGTH {
            return Err(AuxPowError::MissingHeader);
        }

        let magic = header[0..4].try_into()?;
        if magic != MERGED_MINING_HEADER {
            return Err(AuxPowError::MissingHeader);
        }

        // convert to big endian
        let mut raw_block_hash: Vec<u8> = header[4..36].to_vec();
        raw_block_hash.reverse();
        let block_hash = BlockHash::from_slice(&raw_block_hash)?;

        if block_hash != *root_hash {
            return Err(AuxPowError::MissingMerkleRoot);
        }

        Ok(Self {
            magic, // fabe6d6d
            block_hash,
            merkle_size: u32::from_le_bytes(header[36..40].try_into()?),
            merkle_nonce: u32::from_le_bytes(header[40..44].try_into()?),
        })
    }

    // https://github.com/bitcoin/bitcoin/blob/9e1306fc886bcf8024ec37687bbfb8ae364286d6/src/node/miner.cpp#L158
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
            // Aux POW coinbase has no inputs
            return Err(AuxPowError::CoinbaseNoInputs);
        }
        MergedMiningHeader::from_script_sig(&tx.input[0].script_sig)
    }
}

impl CoinbaseParser for CoinbaseVout {
    fn parse_coinbase(
        tx: &Transaction,
        root_hash: &BlockHash,
    ) -> Result<MergedMiningHeader, AuxPowError> {
        // since Marathon does not yet support the merkle construction and the
        // header may be in out[2] or out[3] we need to check the root hash
        match MergedMiningHeader::from_script_pub_key(
            &tx.output
                .get(2)
                .ok_or(AuxPowError::CoinbaseInvalidOutputs)?
                .script_pubkey,
            root_hash,
        ) {
            Ok(header) => Ok(header),
            Err(_) => MergedMiningHeader::from_script_pub_key(
                &tx.output
                    .get(3)
                    .ok_or(AuxPowError::CoinbaseInvalidOutputs)?
                    .script_pubkey,
                root_hash,
            ),
        }
    }
}

// https://en.bitcoin.it/wiki/Merged_mining_specification
/// This is used to prove work on the auxiliary blockchain
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AuxPow {
    /// Coinbase transaction that is in the parent block, linking the AuxPOW block to its parent block
    pub coinbase_txn: Transaction,
    /// Hash of the parent_block header
    // NOTE: not needed
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
    // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/primitives/pureheader.h#L105-L108
    pub fn get_parent_chain_id(&self) -> u32 {
        self.parent_block.version.to_consensus() as u32 / (1 << 16)
    }

    pub(crate) fn aggregate_hash(hashes: &[BlockHash]) -> BlockHash {
        let mut engine = BlockHash::engine();
        hashes
            .iter()
            .map(|hash| BlockHash::consensus_encode(hash, &mut engine))
            .collect::<Result<Vec<_>, _>>()
            .expect("engines don't error");
        BlockHash::from_engine(engine)
    }

    pub fn check(&self, hash_aux_block: BlockHash, chain_id: u32) -> Result<(), AuxPowError> {
        self._check::<CoinbaseVout>(hash_aux_block, chain_id)
    }

    // https://github.com/dogecoin/dogecoin/blob/3a29ba6d497cd1d0a32ecb039da0d35ea43c9c85/src/auxpow.cpp#L81
    // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/auxpow.cpp#L41
    fn _check<C: CoinbaseParser>(
        &self,
        hash_aux_block: BlockHash,
        chain_id: u32,
    ) -> Result<(), AuxPowError> {
        if self.get_parent_chain_id() == chain_id {
            // Aux POW parent has our chain ID
            return Err(AuxPowError::ParentHasChainId);
        }

        if self.blockchain_branch.branch_hash.len() > 30 {
            // Aux POW chain merkle branch too long
            return Err(AuxPowError::MerkleBranchTooLong);
        }

        let n_root_hash = self
            .blockchain_branch
            .check_merkle_branch(TxMerkleNode::from_raw_hash(hash_aux_block.to_raw_hash()));

        let mut vch_root_hash = *n_root_hash.as_byte_array();
        vch_root_hash.reverse(); // correct endian
        let vch_root_hash = BlockHash::from_byte_array(vch_root_hash);

        if self
            .coinbase_branch
            .check_merkle_branch(TxMerkleNode::from_raw_hash(
                self.coinbase_txn.txid().to_raw_hash(),
            ))
            != self.parent_block.merkle_root
        {
            // Aux POW merkle root incorrect
            return Err(AuxPowError::MerkleRootIncorrect);
        }

        let header = C::parse_coinbase(&self.coinbase_txn, &vch_root_hash)?;

        if header.block_hash != vch_root_hash {
            return Err(AuxPowError::MissingMerkleRoot);
        }

        let merkle_height = self.blockchain_branch.branch_hash.len();
        if header.merkle_size != (1 << merkle_height) {
            // Aux POW merkle branch size does not match parent coinbase
            return Err(AuxPowError::InvalidMerkleSize);
        }

        if self.blockchain_branch.branch_side_mask as u64
            != Self::get_expected_index(header.merkle_nonce, chain_id, merkle_height)
        {
            // Aux POW wrong index
            return Err(AuxPowError::WrongIndex);
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

    // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/validation.cpp#L1744
    pub fn check_proof_of_work(&self, bits: CompactTarget) -> bool {
        let diff_target = Target::from_compact(bits);

        trace!("Checking PoW target with target of: {:?}", diff_target);
        diff_target.is_met_by(self.parent_block.block_hash())
    }

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
                        merkle_nonce: 0, // todo
                        merkle_size: 1,  // todo
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
            // trace!("Trying nonce: {}", nonce);
            if aux_pow.check_proof_of_work(target) {
                // This unwrap should always succeed, just a sanity check to catch any bugs asap
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
    use crate::auxpow_miner;
    use bitcoin::{
        consensus::encode::{deserialize, serialize},
        hashes::{sha256d, Hash},
    };
    use lighthouse_wrapper::types::{Hash256, Uint256};

    #[tokio::test]
    async fn test_miner() {
        let sidechain_blockhash = Hash256::random();

        let sidechain_hash =
            sha256d::Hash::from_byte_array(sidechain_blockhash.to_fixed_bytes()).into();
        let chain_id = 0;
        let target = auxpow_miner::target_to_compact_lossy(Uint256::max_value() / 16);

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

    // test that the auxpow encoding format is correct
    // according to the namecoin implementation
    #[test]
    fn should_decode_nmc_auxpow() {
        // it was easier to test the decoding of the whole namecoin block header
        // (including the auxpow) since we can fetch that using namecoin-cli
        #[derive(Debug)]
        struct NamecoinBlockHeader {
            // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/primitives/pureheader.h#L20
            version: i32,
            hash_prev_block: BlockHash,
            hash_merkle_root: TxMerkleNode,
            time: u32,
            bits: CompactTarget,
            nonce: u32,
            // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/primitives/block.h#L30
            auxpow: AuxPow,
        }

        impl Encodable for NamecoinBlockHeader {
            fn consensus_encode<W: std::io::Write + ?Sized>(
                &self,
                writer: &mut W,
            ) -> Result<usize, std::io::Error> {
                let mut len = 0;
                len += self.version.consensus_encode(writer)?;
                len += self.hash_prev_block.consensus_encode(writer)?;
                len += self.hash_merkle_root.consensus_encode(writer)?;
                len += self.time.consensus_encode(writer)?;
                len += self.bits.consensus_encode(writer)?;
                len += self.nonce.consensus_encode(writer)?;
                len += self.auxpow.consensus_encode(writer)?;
                Ok(len)
            }
        }

        impl Decodable for NamecoinBlockHeader {
            fn consensus_decode_from_finite_reader<R: std::io::Read + ?Sized>(
                reader: &mut R,
            ) -> Result<Self, bitcoin::consensus::encode::Error> {
                Ok(Self {
                    version: Decodable::consensus_decode_from_finite_reader(reader)?,
                    hash_prev_block: Decodable::consensus_decode_from_finite_reader(reader)?,
                    hash_merkle_root: Decodable::consensus_decode_from_finite_reader(reader)?,
                    time: Decodable::consensus_decode_from_finite_reader(reader)?,
                    bits: Decodable::consensus_decode_from_finite_reader(reader)?,
                    nonce: Decodable::consensus_decode_from_finite_reader(reader)?,
                    auxpow: Decodable::consensus_decode_from_finite_reader(reader)?,
                })
            }
        }

        // namecoin-cli getblockheader d8a7c3e01e1e95bcee015e6fcc7583a2ca60b79e5a3aa0a171eddd344ada903d false
        let block_header_hex = "0101010036909ac07a1673daf65fa7d828882e66c9e89f8546cdd50a9fb10000000000000f5c6549bcd608ab7c4eac593e5bd5a73b2d432eb63518708f778fc7dcdfaf888d1a904e69b2001b0000000001000000010000000000000000000000000000000000000000000000000000000000000000ffffffff35045dee091a014d522cfabe6d6dd8a7c3e01e1e95bcee015e6fcc7583a2ca60b79e5a3aa0a171eddd344ada903d0100000000000000ffffffff0160a0102a01000000434104f8bbe97ed2acbc5bba11c68f6f1a0313f918f3d3c0e8475055e351e3bf442f8c8dcee682d2457bdc5351b70dd9e34026766eba18b06eaee2e102efd1ab634667ac00000000000000000000000000000000000000000000000000000000000000000000000005050ac4a1a1e1bce0c48e555b1a9f935281968c72d6379b24729ca0425a3fc3cb433cd348b35ea22806cf21c7b146489aef6989551eb5ad2373ab6121060f30341d648757c0217d43e66c57eaed64fc1820ec65d157f33b741965183a5e0c8506ac2602dfe2f547012d1cc75004d48f97aba46bd9930ff285c9f276f5bd09f356df19724579d65ec7cb62bf97946dfc6fb0e3b2839b7fdab37cdb60e55122d35b0000000000000000000100000008be13295c03e67cb70d00dae81ea06e78b9014e5ceb7d9ba504000000000000e0fd42db8ef6d783f079d126bea12e2d10c104c0927cd68f954d856f9e8111e59a23904e5dee091a1c655086";
        let block_header_raw = hex::decode(block_header_hex).unwrap();
        let block_header: NamecoinBlockHeader =
            deserialize(&block_header_raw).expect("Block header decoding is wrong");
        assert_eq!(
            block_header_raw,
            serialize(&block_header),
            "Block header encoding is wrong"
        );
    }

    // https://blockstream.info/block/00000000000000000002f3cbfe48faaee0851268fa232a414bf3e71b8b19bc1a
    #[test]
    fn should_decode_marathon_coinbase() {
        let marathon_coinbase_tx = deserialize::<Transaction>(&hex::decode("010000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff2f03bd7e0c04721566652f7a7a616d78632f76649b3c094f135bf4b83108c14ea85f12edd2045a0075000000ffffffffffffffff039df4662700000000160014b6f3cfc20084e3b9f0d12b0e6f9da8fcbcf5a2d90000000000000000266a24aa21a9edb211480d24c30a0d4df77d79618af9f03c2bb0dced634e7b152af2247a9ca99c00000000000000002cfabe6d6da62edaca27060f885a1935a8f9f4401e65a6c9d936a5a9fb384a0b9fae07a983010000000000000001200000000000000000000000000000000000000000000000000000000000000000173d850d").unwrap()).unwrap();
        let expected_block_hash = deserialize(
            &hex::decode("83a907ae9f0b4a38fba9a536d9c9a6651e40f4f9a835195a880f0627cada2ea6")
                .unwrap(),
        )
        .unwrap();
        let header =
            CoinbaseVout::parse_coinbase(&marathon_coinbase_tx, &expected_block_hash).unwrap();
        assert_eq!(header.block_hash, expected_block_hash);
    }

    // https://coordiscan.io/block-height/16908#JSON
    #[test]
    fn should_decode_coordinate_coinbase() {
        let marathon_coinbase_tx = deserialize::<Transaction>(&hex::decode("01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff3c0349a00c04a29fb3652f4d41524120506f6f6c202876313232373233292f76649b3c094f135bf4b83108c14ea85f12e9a6e4aa00dd000000ffffffffffffffff037e27f826000000001976a9142fc701e2049ee4957b07134b6c1d771dd5a96b2188ac0000000000000000266a24aa21a9edd13261ee69d05e4d79610862cd3a6feb09b43a39c5d26cae63ca0a6501d87d4900000000000000002cfabe6d6daf39e487e7b11bf65932284dcb85290d874a76f00b3619cab549c551fed68f7a01000000000000006dc56f97").unwrap()).unwrap();
        let expected_block_hash = deserialize(
            &hex::decode("7a8fd6fe51c549b5ca19360bf0764a870d2985cb4d283259f61bb1e787e439af")
                .unwrap(),
        )
        .unwrap();
        let header =
            CoinbaseVout::parse_coinbase(&marathon_coinbase_tx, &expected_block_hash).unwrap();
        assert_eq!(header.block_hash, expected_block_hash);
    }

    #[test]
    fn empty_merkle_branch() {
        let hash_aux_block = BlockHash::from_byte_array([1; 32]);
        let blockchain_branch = MerkleBranch {
            branch_hash: vec![],
            branch_side_mask: 0,
        };
        let root_hash = blockchain_branch
            .check_merkle_branch(TxMerkleNode::from_raw_hash(hash_aux_block.to_raw_hash()));
        // tests that if the mining pool is only including a single hash
        // we get that as the merkle root if the branch is empty
        assert_eq!(hash_aux_block.to_raw_hash(), root_hash.to_raw_hash());
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
