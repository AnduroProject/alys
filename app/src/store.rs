use crate::{
    block::*,
    error::{BlockErrorBlockTypes, Error},
};
use bitcoin::CompactTarget;
use ethers_core::types::U256;
use serde_derive::{Deserialize, Serialize};
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};
use std::{fs, marker::PhantomData, path::PathBuf};
use lighthouse_wrapper::store::{get_key_for_col, ItemStore, KeyValueStoreOp, LevelDB, MemoryStore};
use strum::{EnumString, IntoStaticStr};
use tracing::*;
use lighthouse_wrapper::types::{EthSpec, Hash256, MainnetEthSpec};

pub const DEFAULT_ROOT_DIR: &str = "etc/data/consensus/node_0";

pub const HEAD_KEY: Hash256 = Hash256::repeat_byte(5);
pub const LATEST_POW_BLOCK_KEY: Hash256 = Hash256::repeat_byte(6);
pub const DEFAULT_KEY: Hash256 = Hash256::repeat_byte(7);
pub const TARGET_OVERRIDE_KEY: Hash256 = Hash256::repeat_byte(8);
// TODO: should we keep this or use `DBColumn`
// it might make more sense to rewrite the db stuff entirely
#[derive(Debug, Clone, Copy, PartialEq, IntoStaticStr, EnumString)]
pub enum DbColumn {
    #[strum(serialize = "chi")]
    ChainInfo,
    #[strum(serialize = "blk")]
    Block,
    #[strum(serialize = "axh")]
    AuxPowBlockHeight,
    #[strum(serialize = "fee")]
    BlockFees,
    #[strum(serialize = "scn")]
    BitcoinScanStartHeight,
    #[strum(serialize = "bbh")]
    BlockByHeight,
}

#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct BlockRef {
    pub hash: Hash256,
    pub height: u64,
}

pub struct Storage<E: EthSpec, DB> {
    db: DB,
    _phantom: PhantomData<E>,
}

pub trait BlockByHeight {
    fn put_block_by_height(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error>;
    fn get_block_by_height(
        &self,
        height: u64,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>, Error>;
}

impl Storage<MainnetEthSpec, MemoryStore<MainnetEthSpec>> {
    #[allow(unused)]
    pub fn new_memory() -> Self {
        let memory_store = MemoryStore::<MainnetEthSpec>::open();
        Self {
            db: memory_store,
            _phantom: PhantomData,
        }
    }
}

impl Storage<MainnetEthSpec, LevelDB<MainnetEthSpec>> {
    pub fn new_disk(path_override: Option<String>) -> Self {
        let db_path = if let Some(path) = path_override {
            PathBuf::from(path)
        } else {
            PathBuf::from(DEFAULT_ROOT_DIR).join("chain_db")
        };

        info!("Using db path {}", db_path.display());
        let db_path = ensure_dir_exists(db_path).unwrap();
        let level_db = LevelDB::<MainnetEthSpec>::open(&db_path).unwrap();
        Self {
            db: level_db,
            _phantom: PhantomData,
        }
    }
}

impl<DB: ItemStore<MainnetEthSpec>> BlockByHeight for Storage<MainnetEthSpec, DB> {
    fn put_block_by_height(
        &self,
        block: &SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        let block_root = block.canonical_root();
        let height = block.message.execution_payload.block_number;

        self.commit_ops(vec![KeyValueStoreOp::PutKeyValue(
            get_key_for_col(DbColumn::BlockByHeight.into(), &height.to_be_bytes()),
            block_root.as_bytes().to_vec(),
        )])
    }

    fn get_block_by_height(
        &self,
        height: u64,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>, Error> {
        trace!("Getting block by height {}", height);

        match self
            .db
            .get_bytes(DbColumn::BlockByHeight.into(), &height.to_be_bytes())
            .map_err(|_| Error::DbReadError)?
        {
            // Get the block hash from the block by height index
            Some(block_hash) => {
                // Use the hash to retrieve the block
                self.get_block(&Hash256::from_slice(&block_hash)) // return the block
            }
            None => Ok(None),
        }
    }
}

impl<DB: ItemStore<MainnetEthSpec>> Storage<MainnetEthSpec, DB> {
    pub fn set_bitcoin_scan_start_height(&self, height: u32) -> Result<(), Error> {
        let db_key = get_key_for_col(
            DbColumn::BitcoinScanStartHeight.into(),
            DEFAULT_KEY.as_bytes(),
        );
        self.commit_ops(vec![KeyValueStoreOp::PutKeyValue(
            db_key,
            height.as_ssz_bytes(),
        )])
    }

    #[must_use]
    pub fn set_head(&self, sync_status: &BlockRef) -> Vec<KeyValueStoreOp> {
        self.set_ref(sync_status, HEAD_KEY.as_bytes())
    }

    #[must_use]
    pub fn set_latest_pow_block(&self, block_ref: &BlockRef) -> Vec<KeyValueStoreOp> {
        self.set_ref(block_ref, LATEST_POW_BLOCK_KEY.as_bytes())
    }

    #[must_use]
    fn set_ref(&self, block_ref: &BlockRef, key: &[u8]) -> Vec<KeyValueStoreOp> {
        let db_key = get_key_for_col(DbColumn::ChainInfo.into(), key);
        vec![KeyValueStoreOp::PutKeyValue(
            db_key,
            block_ref.as_ssz_bytes(),
        )]
    }

    pub fn get_head(&self) -> Result<Option<BlockRef>, Error> {
        self.get_ref(HEAD_KEY.as_bytes())
            .map_err(|_| Error::ChainError(BlockErrorBlockTypes::Head.into()))
    }

    fn get_previous_head(&self, head_ref: &BlockRef) -> Result<Option<BlockRef>, Error> {
        let previous_head = self.get_block(&head_ref.hash)?.unwrap().message.parent_hash;
        Ok(Some(BlockRef {
            hash: previous_head,
            height: head_ref.height - 1,
        }))
    }

    pub fn get_latest_pow_block(&self) -> Result<Option<BlockRef>, Error> {
        self.get_ref(LATEST_POW_BLOCK_KEY.as_bytes())
    }

    fn get_ref(&self, key: &[u8]) -> Result<Option<BlockRef>, Error> {
        self.db
            .get_bytes(DbColumn::ChainInfo.into(), key)
            .unwrap()
            .map(|bytes| BlockRef::from_ssz_bytes(&bytes))
            .transpose()
            .map_err(|_| Error::DbReadError)
    }

    pub fn set_target_override(&self, target: CompactTarget) -> Result<(), Error> {
        let db_key = get_key_for_col(DbColumn::ChainInfo.into(), TARGET_OVERRIDE_KEY.as_bytes());
        self.commit_ops(vec![KeyValueStoreOp::PutKeyValue(
            db_key,
            target.to_consensus().as_ssz_bytes(),
        )])
    }

    pub fn get_target_override(&self) -> Result<Option<CompactTarget>, Error> {
        let consensus_rep_bytes = self
            .db
            .get_bytes(DbColumn::ChainInfo.into(), TARGET_OVERRIDE_KEY.as_bytes())
            .unwrap()
            .map(|bytes| u32::from_ssz_bytes(&bytes))
            .transpose()
            .map_err(|_| Error::DbReadError)?;

        // let consensus_rep = if let Some(consensus_rep_bytes) = consensus_rep_bytes {
        //     consensus_rep_bytes
        //         .map(|bytes| u32::from_ssz_bytes(&bytes))
        //         .transpose()
        //         .map_err(|_| Error::DbReadError)?
        // } else {
        //     Ok(None)
        // };

        if let Some(consensus_rep) = consensus_rep_bytes {
            if consensus_rep != 0 {
                Ok(Some(CompactTarget::from_consensus(consensus_rep)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    #[must_use]
    pub fn put_block(
        &self,
        block_root: &Hash256,
        block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Vec<KeyValueStoreOp> {
        let mut ops = vec![KeyValueStoreOp::PutKeyValue(
            get_key_for_col(DbColumn::Block.into(), block_root.as_bytes()),
            rmp_serde::to_vec(&block).unwrap(),
        )];

        ops.push(KeyValueStoreOp::PutKeyValue(
            get_key_for_col(
                DbColumn::BlockByHeight.into(),
                &block.message.execution_payload.block_number.to_be_bytes(),
            ),
            Vec::from(block_root.as_bytes()),
        ));

        if let Some(auxpow_header) = block.message.auxpow_header {
            ops.push(KeyValueStoreOp::PutKeyValue(
                get_key_for_col(
                    DbColumn::AuxPowBlockHeight.into(),
                    &auxpow_header.height.to_be_bytes(),
                ),
                block_root.as_bytes().to_vec(),
            ));
        }

        ops
    }

    pub fn get_bitcoin_scan_start_height(&self) -> Result<Option<u32>, Error> {
        Ok(self
            .db
            .get_bytes(
                DbColumn::BitcoinScanStartHeight.into(),
                DEFAULT_KEY.as_bytes(),
            )
            .map_err(|_| Error::DbReadError)?
            .map(|bytes| u32::from_ssz_bytes(&bytes).unwrap()))
    }

    /// Fetch the hash of the auxpow block at this height
    pub fn get_auxpow_block_hash(&self, height: u64) -> Result<Option<Hash256>, Error> {
        Ok(self
            .db
            .get_bytes(DbColumn::AuxPowBlockHeight.into(), &height.to_be_bytes())
            .map_err(|_| Error::DbReadError)?
            .map(|bytes| Hash256::from_slice(&bytes)))
    }

    pub fn get_block(
        &self,
        block_root: &Hash256,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>, Error> {
        self.get_block_with(block_root, |bytes| {
            rmp_serde::from_slice(bytes).map_err(|_| Error::CodecError)
        })
    }

    pub fn get_block_with(
        &self,
        block_root: &Hash256,
        decoder: impl FnOnce(&[u8]) -> Result<SignedConsensusBlock<MainnetEthSpec>, Error>,
    ) -> Result<Option<SignedConsensusBlock<MainnetEthSpec>>, Error> {
        self.db
            .get_bytes(DbColumn::Block.into(), block_root.as_bytes())
            .unwrap()
            .map(|block_bytes| decoder(&block_bytes))
            .transpose()
            .map_err(|_| Error::DbReadError)
    }

    pub fn set_accumulated_block_fees(
        &self,
        block_root: &Hash256,
        fees: U256,
    ) -> Vec<KeyValueStoreOp> {
        vec![KeyValueStoreOp::PutKeyValue(
            get_key_for_col(DbColumn::BlockFees.into(), block_root.as_bytes()),
            Into::<[u8; 32]>::into(fees).into(),
        )]
    }

    pub fn get_accumulated_block_fees(&self, block_root: &Hash256) -> Result<Option<U256>, Error> {
        Ok(self
            .db
            .get_bytes(DbColumn::BlockFees.into(), block_root.as_bytes())
            .map_err(|_| Error::DbReadError)?
            .map(|bytes| U256::from(&bytes[..])))
    }

    pub fn commit_ops(&self, ops: Vec<KeyValueStoreOp>) -> Result<(), Error> {
        self.db.do_atomically(ops).map_err(|_| Error::StorageError)
    }
}

fn ensure_dir_exists(path: PathBuf) -> Result<PathBuf, String> {
    fs::create_dir_all(&path).map_err(|e| format!("Unable to create {}: {}", path.display(), e))?;
    Ok(path)
}
