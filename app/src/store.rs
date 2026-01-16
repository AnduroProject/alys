use crate::{
    block::*,
    error::{BlockErrorBlockTypes, Error},
    metrics::CHAIN_LAST_FINALIZED_BLOCK,
};
use ethers_core::types::U256;
use leveldb::database::management::repair;
use leveldb::options::Options as LevelDbOptions;
use lighthouse_wrapper::store::{
    get_key_for_col, ItemStore, KeyValueStore, KeyValueStoreOp, LevelDB, MemoryStore,
};
use lighthouse_wrapper::types::{EthSpec, Hash256, MainnetEthSpec};
use serde_derive::{Deserialize, Serialize};
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};
use std::{fs, marker::PhantomData, path::PathBuf};
use strum::{EnumString, IntoStaticStr};
use tracing::*;

pub const DEFAULT_ROOT_DIR: &str = "etc/data/consensus/node_0";

pub const HEAD_KEY: Hash256 = Hash256::repeat_byte(5);
pub const LATEST_POW_BLOCK_KEY: Hash256 = Hash256::repeat_byte(6);
pub const DEFAULT_KEY: Hash256 = Hash256::repeat_byte(7);
// TODO: Can be removed on later version, kept to maintain key ordering
#[allow(dead_code)]
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

        // Try to open the database, with automatic recovery on corruption
        let level_db = match LevelDB::<MainnetEthSpec>::open(&db_path) {
            Ok(db) => {
                info!("Database opened successfully");
                db
            }
            Err(e) => {
                let error_msg = format!("{:?}", e);
                warn!(
                    "Failed to open database: {}. Attempting recovery...",
                    error_msg
                );

                // Check if this is a corruption error
                if error_msg.contains("Corruption")
                    || error_msg.contains("unknown tag")
                    || error_msg.contains("VersionEdit")
                {
                    info!("Detected database corruption, running LevelDB repair...");

                    // Attempt to repair the database
                    let mut repair_options = LevelDbOptions::new();
                    repair_options.create_if_missing = false;

                    match repair(&db_path, repair_options) {
                        Ok(()) => {
                            info!("Database repair completed successfully");

                            // Try to open again after repair
                            match LevelDB::<MainnetEthSpec>::open(&db_path) {
                                Ok(db) => {
                                    info!("Database opened successfully after repair");
                                    db
                                }
                                Err(e2) => {
                                    error!(
                                        "Failed to open database after repair: {:?}. \
                                         Database may need manual recovery or deletion.",
                                        e2
                                    );
                                    panic!(
                                        "Unable to recover database at {}. \
                                         Consider deleting the database directory and resyncing. \
                                         Original error: {:?}, Post-repair error: {:?}",
                                        db_path.display(),
                                        e,
                                        e2
                                    );
                                }
                            }
                        }
                        Err(repair_err) => {
                            error!("Database repair failed: {:?}", repair_err);
                            panic!(
                                "Unable to repair corrupted database at {}. \
                                 Consider deleting the database directory and resyncing. \
                                 Original error: {:?}, Repair error: {:?}",
                                db_path.display(),
                                e,
                                repair_err
                            );
                        }
                    }
                } else {
                    // Non-corruption error, just propagate it
                    panic!("Failed to open database at {}: {:?}", db_path.display(), e);
                }
            }
        };

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
        CHAIN_LAST_FINALIZED_BLOCK.set(block_ref.height as i64);
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

    /// Sync all pending writes to disk.
    /// Should be called before graceful shutdown to prevent data loss.
    pub fn sync(&self) -> Result<(), Error> {
        info!("Syncing database to disk...");
        self.db.sync().map_err(|e| {
            error!("Failed to sync database: {:?}", e);
            Error::StorageError
        })?;
        info!("Database sync completed");
        Ok(())
    }
}

fn ensure_dir_exists(path: PathBuf) -> Result<PathBuf, String> {
    fs::create_dir_all(&path).map_err(|e| format!("Unable to create {}: {}", path.display(), e))?;
    Ok(path)
}
