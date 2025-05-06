use crate::block::{AuxPowHeader, SignedConsensusBlock};
use crate::error::AuxPowMiningError::HashRetrievalError;
use crate::error::{BlockErrorBlockTypes, Error};
use crate::metrics::{
    AUXPOW_CREATE_BLOCK_CALLS, AUXPOW_HASHES_PROCESSED, AUXPOW_SUBMIT_BLOCK_CALLS,
};
use crate::{auxpow::AuxPow, chain::Chain};
use bitcoin::consensus::Encodable;
use bitcoin::{consensus::Decodable, string::FromHexStr, BlockHash, CompactTarget, Target};
use ethereum_types::Address as EvmAddress;
use eyre::{eyre, Result};
use serde::{de::Error as _, ser::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, marker::PhantomData, sync::Arc, thread, time::Duration};
use store::ItemStore;
use tokio::runtime::Handle;
use tokio::time::sleep;
use tracing::*;
use types::{MainnetEthSpec, Uint256};

fn compact_target_to_hex<S>(bits: &CompactTarget, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&format!("{:x}", bits.to_consensus()))
}

fn compact_target_from_hex<'de, D>(deserializer: D) -> Result<CompactTarget, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    CompactTarget::from_hex_str_no_prefix(s).map_err(D::Error::custom)
}

fn block_hash_to_consensus_hex<S>(block_hash: &BlockHash, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut encoded_block_hash = Vec::new();
    block_hash
        .consensus_encode(&mut encoded_block_hash)
        .map_err(S::Error::custom)?;
    let stringified_auxpow = hex::encode(encoded_block_hash);

    s.serialize_str(&stringified_auxpow)
}

fn block_hash_from_consensus_hex<'de, D>(deserializer: D) -> Result<BlockHash, D::Error>
where
    D: Deserializer<'de>,
{
    let blockhash_str: &str = Deserialize::deserialize(deserializer)?;
    // Note: BlockHash::from_slice results in opposite endianness from BlockHash::from_str
    let blockhash_bytes = hex::decode(blockhash_str).map_err(D::Error::custom)?;
    BlockHash::consensus_decode(&mut blockhash_bytes.as_slice()).map_err(D::Error::custom)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuxBlock {
    #[serde(serialize_with = "block_hash_to_consensus_hex")]
    #[serde(deserialize_with = "block_hash_from_consensus_hex")]
    pub hash: BlockHash,
    #[serde(rename = "chainid")]
    pub chain_id: u32,
    #[serde(rename = "previousblockhash")]
    #[serde(serialize_with = "block_hash_to_consensus_hex")]
    #[serde(deserialize_with = "block_hash_from_consensus_hex")]
    previous_block_hash: BlockHash,
    #[serde(rename = "coinbasevalue")]
    coinbase_value: u64,
    #[serde(serialize_with = "compact_target_to_hex")]
    #[serde(deserialize_with = "compact_target_from_hex")]
    pub bits: CompactTarget,
    height: u64,
    _target: Target,
}

// TODO: Either move this struct out of auxpow__miner or modularize between mining related functionalities, and basic chain functionality
#[async_trait::async_trait]
pub trait ChainManager<BI> {
    async fn get_aggregate_hashes(&self) -> Result<Vec<BlockHash>>;
    fn get_last_finalized_block(&self) -> BI;
    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<BI>;
    async fn get_queued_auxpow(&self) -> Option<AuxPowHeader>;
    fn get_block_at_height(&self, height: u64) -> Result<BI>;
    #[allow(clippy::too_many_arguments)]
    async fn push_auxpow(
        &self,
        start_hash: BlockHash,
        end_hash: BlockHash,
        bits: u32,
        chain_id: u32,
        height: u64,
        auxpow: AuxPow,
        address: EvmAddress,
    ) -> bool;
    fn set_target_override(&self, target: CompactTarget);
    fn get_target_override(&self) -> Option<CompactTarget>;
    async fn is_synced(&self) -> bool;
    fn get_head(&self) -> Result<SignedConsensusBlock<MainnetEthSpec>, Error>;
}

pub trait BlockIndex {
    fn block_hash(&self) -> BlockHash;
    fn block_time(&self) -> u64;
    fn bits(&self) -> u32;
    fn chain_id(&self) -> u32;
    fn height(&self) -> u64;
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BitcoinConsensusParams {
    /// The proof of work limit of the bitcoin network
    pub pow_limit: u32,
    /// The targeted timespan between difficulty adjustments
    pub pow_target_timespan: u64,
    /// The targeted interval between blocks
    pub pow_target_spacing: u64,
    /// Whether this chain supports proof of work retargeting or not
    pub pow_no_retargeting: bool,
}

impl BitcoinConsensusParams {
    #[allow(unused)]
    const BITCOIN_MAINNET: Self = Self {
        // https://github.com/rust-bitcoin/rust-bitcoin/blob/67793d04c302bd494519b20b44b260ec3ff8a2f1/bitcoin/src/pow.rs#L124C9-L124C90
        pow_limit: 486604799,
        pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
        pow_target_spacing: 10 * 60,            // ten minutes
        pow_no_retargeting: false,
    };

    fn difficulty_adjustment_interval(&self) -> u64 {
        self.pow_target_timespan / self.pow_target_spacing
    }
}

// TODO: remove once this is merged
// https://github.com/rust-bitcoin/rust-bitcoin/pull/2180
fn uint256_target_from_compact(bits: u32) -> Uint256 {
    let (mant, expt) = {
        let unshifted_expt = bits >> 24;
        if unshifted_expt <= 3 {
            ((bits & 0xFFFFFF) >> (8 * (3 - unshifted_expt as usize)), 0)
        } else {
            (bits & 0xFFFFFF, 8 * ((bits >> 24) - 3))
        }
    };

    // The mantissa is signed but may not be negative.
    if mant > 0x7F_FFFF {
        Uint256::zero()
    } else {
        Uint256::from(mant) << expt
    }
}

// TODO: remove once this is merged
// https://github.com/rust-bitcoin/rust-bitcoin/pull/2180
pub fn target_to_compact_lossy(target: Uint256) -> CompactTarget {
    #[allow(clippy::manual_div_ceil)]
    let mut size = (target.bits() + 7) / 8;
    let mut compact = if size <= 3 {
        (target.low_u64() << (8 * (3 - size))) as u32
    } else {
        let bn = target >> (8 * (size - 3));
        bn.low_u32()
    };

    if (compact & 0x0080_0000) != 0 {
        compact >>= 8;
        size += 1;
    }

    CompactTarget::from_consensus(compact | ((size as u32) << 24))
}

fn calculate_next_work_required(
    first_block_time: u64,
    last_block_time: u64,
    last_bits: u32,
    params: &BitcoinConsensusParams,
) -> CompactTarget {
    let min_timespan = params.pow_target_timespan >> 2;
    let max_timespan = params.pow_target_timespan << 2;

    let timespan = last_block_time - first_block_time;
    let timespan = timespan.clamp(min_timespan, max_timespan);

    let target = uint256_target_from_compact(last_bits);
    // TODO: figure out why this was overflowing with new consensus params
    let target = target.saturating_mul(Uint256::from(timespan));
    let target = target / Uint256::from(params.pow_target_timespan);
    let target = target.min(uint256_target_from_compact(params.pow_limit));

    trace!(
        "First block time: {}, last block time: {}, last bits: {}, timespan: {}, target: {}",
        first_block_time,
        last_block_time,
        last_bits,
        timespan,
        target
    );

    target_to_compact_lossy(target)
}

fn is_retarget_height(height: u64, params: &BitcoinConsensusParams) -> bool {
    let adjustment_interval = params.difficulty_adjustment_interval() as u32;
    trace!(
        "Height: {}, interval: {}, is_time: {}",
        height,
        adjustment_interval,
        height % adjustment_interval as u64 == 0
    );
    height % adjustment_interval as u64 == 0
}

pub fn get_next_work_required<BI: BlockIndex>(
    get_block_at_height: impl Fn(u64) -> Result<BI>,
    index_last: &BI,
    params: &BitcoinConsensusParams,
    target_override: Option<CompactTarget>,
) -> Result<CompactTarget> {
    if let Some(target) = target_override {
        return Ok(target);
    }
    if is_retarget_height(index_last.height() + 1, params) {
        info!("Retargeting, using new bits at height {}", index_last.height() + 1);
        info!("Last bits: {:?}", index_last.bits());
    }

    if params.pow_no_retargeting || !is_retarget_height(index_last.height() + 1, params) {
        info!("No retargeting, using last bits: {:?}", params.pow_no_retargeting);
        info!("Last bits: {:?}", index_last.bits());
        return Ok(CompactTarget::from_consensus(index_last.bits()));
    }

    let blocks_back = params.difficulty_adjustment_interval() - 1;
    let height_first = index_last.height() - blocks_back;
    let index_first = get_block_at_height(height_first)?;

    let next_work = calculate_next_work_required(
        index_first.block_time(),
        index_last.block_time(),
        index_last.bits(),
        params,
    );

    info!(
        "Difficulty adjustment from {} to {}",
        index_last.bits(),
        next_work.to_consensus()
    );

    Ok(next_work)
}

struct AuxInfo {
    last_hash: BlockHash,
    start_hash: BlockHash,
    end_hash: BlockHash,
    address: EvmAddress,
}

pub struct AuxPowMiner<BI: BlockIndex, CM: ChainManager<BI>> {
    state: BTreeMap<BlockHash, AuxInfo>,
    chain: Arc<CM>,
    retarget_params: BitcoinConsensusParams,
    _phantom: PhantomData<BI>,
}

impl<BI: BlockIndex, CM: ChainManager<BI>> AuxPowMiner<BI, CM> {
    pub fn new(chain: Arc<CM>, retarget_params: BitcoinConsensusParams) -> Self {
        Self {
            state: BTreeMap::new(),
            chain,
            retarget_params,
            _phantom: Default::default(),
        }
    }

    fn get_next_work_required(&self, index_last: &BI) -> Result<CompactTarget> {
        get_next_work_required(
            |height| self.chain.get_block_at_height(height),
            index_last,
            &self.retarget_params,
            self.get_target_override(),
        )
    }

    pub fn set_target_override(&mut self, target: CompactTarget) {
        self.chain.set_target_override(target);
    }

    pub fn get_target_override(&self) -> Option<CompactTarget> {
        self.chain.get_target_override()
    }

    /// Creates a new block and returns information required to merge-mine it.
    // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/rpc/auxpow_miner.cpp#L139
    pub async fn create_aux_block(&mut self, address: EvmAddress) -> Result<AuxBlock> {
        AUXPOW_CREATE_BLOCK_CALLS
            .with_label_values(&["called"])
            .inc();

        if !self.chain.is_synced().await {
            AUXPOW_CREATE_BLOCK_CALLS
                .with_label_values(&["chain_syncing"])
                .inc();
            return Err(Error::ChainSyncing.into());
        }

        let index_last = self.chain.get_last_finalized_block();

        let hashes = self.chain.get_aggregate_hashes().await?;
        trace!("Found {} hashes", hashes.len());

        AUXPOW_HASHES_PROCESSED.observe(hashes.len() as f64);

        // calculates the "vector commitment" for previous blocks without PoW.
        let hash = AuxPow::aggregate_hash(&hashes);

        trace!("Creating AuxBlock for hash {}", hash);

        // store the height for this hash so we can retrieve the
        // same unverified hashes on submit
        self.state.insert(
            hash,
            AuxInfo {
                last_hash: index_last.block_hash(),
                start_hash: *hashes.first().ok_or(Error::from(HashRetrievalError(
                    BlockErrorBlockTypes::AuxPowFirst,
                )))?,
                end_hash: *hashes.last().ok_or(Error::from(HashRetrievalError(
                    BlockErrorBlockTypes::AuxPowLast,
                )))?,
                address,
            },
        );

        // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/node/miner.cpp#L174
        let bits = self.get_next_work_required(&index_last)?;

        AUXPOW_CREATE_BLOCK_CALLS
            .with_label_values(&["success"])
            .inc();

        Ok(AuxBlock {
            hash,
            chain_id: index_last.chain_id(),
            previous_block_hash: index_last.block_hash(),
            coinbase_value: 0,
            bits,
            height: index_last.height() + 1,
            _target: bits.into(),
        })
    }

    /// Submits a solved auxpow for a block that was previously created by 'createauxblock'.
    ///
    /// # Arguments
    ///
    /// * `hash` - Hash of the block to submit
    /// * `auxpow` - Serialised auxpow found
    // https://github.com/namecoin/namecoin-core/blob/1e19d9f53a403d627d7a53a27c835561500c76f5/src/rpc/auxpow_miner.cpp#L166
    pub async fn submit_aux_block(&mut self, hash: BlockHash, auxpow: AuxPow) -> Result<()> {
        AUXPOW_SUBMIT_BLOCK_CALLS
            .with_label_values(&["called"])
            .inc();

        trace!("Submitting AuxPow for hash {}", hash);
        let AuxInfo {
            last_hash,
            start_hash,
            end_hash,
            address,
        } = if let Some(aux_info) = self.state.remove(&hash) {
            // TODO: should we only remove on error?
            aux_info
        } else {
            error!("Submitted AuxPow for unknown block");
            AUXPOW_SUBMIT_BLOCK_CALLS
                .with_label_values(&["unknown_block"])
                .inc();
            return Err(eyre!("Submitted AuxPow for unknown block"));
        };

        let index_last = if let Ok(block) = self.chain.get_block_by_hash(&last_hash) {
            block
        } else {
            error!("Last block not found");
            return Err(eyre!("Last block not found"));
        };

        trace!("Last block hash: {}", index_last.block_hash());
        let bits = self.get_next_work_required(&index_last)?;
        trace!("Next work required: {}", bits.to_consensus());
        let chain_id = index_last.chain_id();
        trace!("Chain ID: {}", chain_id);

        // NOTE: we also check this in `check_pow`
        // process block
        if !auxpow.check_proof_of_work(bits) {
            // AUX proof of work failed
            error!("POW is not valid");
            AUXPOW_SUBMIT_BLOCK_CALLS
                .with_label_values(&["invalid_pow"])
                .inc();
            return Err(eyre!("POW is not valid"));
        }
        if auxpow.check(hash, chain_id).is_err() {
            // AUX POW is not valid
            error!("AuxPow is not valid");
            AUXPOW_SUBMIT_BLOCK_CALLS
                .with_label_values(&["invalid_auxpow"])
                .inc();
            return Err(eyre!("AuxPow is not valid"));
        }

        // should check if newer block is finalized
        self.chain
            .push_auxpow(
                start_hash,
                end_hash,
                bits.to_consensus(),
                chain_id,
                index_last.height() + 1,
                auxpow,
                address,
            )
            .await;
        Ok(())
    }

    pub fn get_head(&self) -> Result<SignedConsensusBlock<MainnetEthSpec>, Error> {
        self.chain.get_head()
    }

    pub async fn get_queued_auxpow(&self) -> Option<AuxPowHeader> {
        self.chain.get_queued_auxpow().await
    }
}

pub fn spawn_background_miner<DB: ItemStore<MainnetEthSpec>>(chain: Arc<Chain<DB>>) {
    let task = async move {
        let mut miner = AuxPowMiner::new(chain.clone(), chain.retarget_params.clone());
        loop {
            trace!("Calling create_aux_block");
            // TODO: set miner address
            if let Ok(aux_block) = miner.create_aux_block(EvmAddress::zero()).await {
                trace!("Created AuxBlock for hash {}", aux_block.hash);
                let auxpow = AuxPow::mine(aux_block.hash, aux_block.bits, aux_block.chain_id).await;
                trace!("Calling submit_aux_block");
                miner
                    .submit_aux_block(aux_block.hash, auxpow)
                    .await
                    .unwrap();
            } else {
                trace!("No aux block created");
                sleep(Duration::from_millis(250)).await;
                continue;
            }
        }
    };

    let handle = Handle::current();
    thread::spawn(move || handle.spawn(task));
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::hashes::Hash as HashExt;
    use tokio::time::Instant;

    #[test]
    fn parse_aux_block_rpc() {
        // namecoin-cli -regtest createauxblock n4cXYAUygg8jRypEamNcwgVwGnRdwBJb3S
        let data = r#"{
            "hash": "df8be27164c84d325c77ef9383abf47c0c7ff06c66ccda3447b585c50872d010",
            "chainid": 1,
            "previousblockhash": "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206",
            "coinbasevalue": 5000000000,
            "bits": "207fffff",
            "height": 1,
            "_target": "0000000000000000000000000000000000000000000000000000000000ffff7f"
        }"#
        .replace(" ", "")
        .replace("\n", "");

        let aux_block: AuxBlock = serde_json::from_str(&data).unwrap();
        assert_eq!(data, serde_json::to_string(&aux_block).unwrap());
    }

    #[test]
    fn should_calculate_target() {
        assert_eq!(
            0x1704e90f,
            calculate_next_work_required(
                0x650964b5,
                0x651bc919,
                0x1704ed7f,
                &BitcoinConsensusParams::BITCOIN_MAINNET
            )
            .to_consensus()
        );

        assert_eq!(
            453150034, // 106848
            calculate_next_work_required(
                1296116171, // 104832
                1297140342, // 106847
                453179945,
                &BitcoinConsensusParams::BITCOIN_MAINNET
            )
            .to_consensus()
        );
    }

    #[ignore]
    #[tokio::test]
    async fn benchmark_pow() {
        fn calculate_work(
            first_block_time: u64,
            last_block_time: u64,
            last_bits: u32,
            params: BitcoinConsensusParams,
        ) -> CompactTarget {
            let timespan = last_block_time - first_block_time;
            let target = uint256_target_from_compact(last_bits);
            let target = target.saturating_mul(Uint256::from(timespan));
            let target = target / Uint256::from(params.pow_target_timespan);
            target_to_compact_lossy(target)
        }

        let mut bits = target_to_compact_lossy(Uint256::MAX).to_consensus();
        loop {
            let params = BitcoinConsensusParams::BITCOIN_MAINNET;

            let start = Instant::now();
            AuxPow::mine(
                BlockHash::all_zeros(),
                CompactTarget::from_consensus(bits),
                0,
            )
            .await;
            println!("Took {}s for {}", start.elapsed().as_secs(), bits);

            let start_time = 1706557326;
            // simulate 2s aura block production
            let end_time = start_time
                + params.difficulty_adjustment_interval() * start.elapsed().as_secs().max(2);

            bits = calculate_work(start_time, end_time, bits, params).to_consensus();
        }
    }
}
