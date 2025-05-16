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
use lighthouse_wrapper::store::ItemStore;
use lighthouse_wrapper::types::{MainnetEthSpec, Uint256};
use rust_decimal::prelude::*; // Includes the `dec` macro when feature specified
use serde::{de::Error as _, ser::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, marker::PhantomData, sync::Arc, thread, time::Duration};
use tokio::runtime::Handle;
use tokio::time::sleep;
use tracing::*;

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
    /// The proof of work lower limit
    pub pow_lower_limit: u32,
    /// The targeted timespan between difficulty adjustments
    pub pow_target_timespan: u64,
    /// The targeted interval between blocks
    pub pow_target_spacing: u64,
    /// Whether this chain supports proof of work retargeting or not
    pub pow_no_retargeting: bool,
    /// The maximum range of adjustment for the proof of work represented as a whole number percentage (e.g. 20 == 20%)
    pub max_pow_adjustment: u8,
}

impl BitcoinConsensusParams {
    #[allow(unused)]
    const BITCOIN_MAINNET: Self = Self {
        // https://github.com/rust-bitcoin/rust-bitcoin/blob/67793d04c302bd494519b20b44b260ec3ff8a2f1/bitcoin/src/pow.rs#L124C9-L124C90
        pow_limit: 486604799,
        pow_lower_limit: 439495319,
        pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
        pow_target_spacing: 10 * 60,            // ten minutes
        pow_no_retargeting: false,
        max_pow_adjustment: 20,
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

// TODO: Might be better to rename last bits so that it doesn't conflate with the last block
/// Calculate the next work required based on the timespan between the last block containing an `AuxPowHeader` and the head block vs the target spacing
/// It returns the new target as a `CompactTarget`
fn calculate_next_work_required(
    // The difference between the head block + 1 and the last block containing an auxpow header
    mut auxpow_height_difference: u32,
    // The compact target of the head block
    last_bits: u32,
    // The consensus parameters defined in the chain.json or via an update in the historical context
    params: &BitcoinConsensusParams,
) -> CompactTarget {
    // Guarantee that the auxpow height difference is not 0
    if auxpow_height_difference == 0 {
        error!("Auxpow height difference is 0");
        auxpow_height_difference = 1;
    }
    // Grab the ratio between actual timespan & target spacing
    let mut ratio: Decimal =
        Decimal::from(auxpow_height_difference) / Decimal::from(params.pow_target_spacing);

    // Round to 2 decimal places
    ratio = ratio.round_dp(2);
    trace!(
        "Unclamped ratio between actual timespan and target timespan: {}",
        ratio
    );

    // Calculate the max & min for the adjustment from the defined parameter
    // TODO: potential to optimize by caching these values
    let max_adjustment = Decimal::from(params.max_pow_adjustment);

    // Decimal representation of `max_pow_adjustment`
    let max_lower_bound = max_adjustment / dec!(100);

    // Decimal representation of `max_pow_adjustment` + 1.0
    let max_upper_bound = max_lower_bound + dec!(1);

    // Apply the ratio bounds based on whether it is >, <, or = 1
    if ratio < dec!(1) {
        // If the ratio is < 1, make sure it's below
        ratio = ratio.min(max_lower_bound)
    } else if ratio > dec!(1) {
        ratio = ratio.min(max_upper_bound)
    } else {
        // If the ratio is 1 then we don't need to adjust the target
    }

    trace!(
        "Clamped ratio between actual timespan and target timespan: {}",
        ratio
    );

    // Multiply the adjustment ratio by 100 to get the percentage in whole numbers and cast to u8
    // TODO: handle unwrap
    let adjustment_percentage = (ratio * dec!(100)).to_u8().unwrap();

    let target = uint256_target_from_compact(last_bits);
    let single_percentage = target.checked_div(Uint256::from(100));

    match single_percentage {
        Some(single_percentage) => {
            let adjustment_percentage = Uint256::from(adjustment_percentage);

            trace!(
                "Adjustment percentage: {}\nSingle Percentage: {}",
                adjustment_percentage,
                single_percentage
            );

            let adjusted_target = single_percentage.saturating_mul(adjustment_percentage);

            trace!(
                "Original target: {}, adjusted target: {}",
                target,
                adjusted_target
            );

            target_to_compact_lossy(adjusted_target)
        }
        None => {
            error!("Target is too small to calculate adjustment percentage");
            target_to_compact_lossy(uint256_target_from_compact(last_bits))
        }
    }
}

fn is_retarget_height(
    chain_head_height: u64,
    height_difference: &u32,
    params: &BitcoinConsensusParams,
) -> bool {
    let adjustment_interval = params.difficulty_adjustment_interval();
    if chain_head_height % adjustment_interval == 0
        || height_difference > &(adjustment_interval as u32)
    {
        return true;
    }
    false
}

pub fn get_next_work_required<BI: BlockIndex>(
    get_block_at_height: impl Fn(u64) -> Result<BI>,
    index_last: &BI,
    params: &BitcoinConsensusParams,
    target_override: Option<CompactTarget>,
    chain_head_height: u64,
) -> Result<CompactTarget> {
    if let Some(target) = target_override {
        return Ok(target);
    }

    // Calculate the difference between the current head + 1 and the last block that contains a auxpow header
    let auxpow_height_difference = (chain_head_height + 1 - index_last.height()) as u32;

    if params.pow_no_retargeting
        || !is_retarget_height(chain_head_height, &auxpow_height_difference, params)
    {
        trace!(
            "No retargeting, using last bits: {:?}",
            params.pow_no_retargeting
        );
        trace!("Last bits: {:?}", index_last.bits());
        return Ok(CompactTarget::from_consensus(index_last.bits()));
    } else {
        trace!(
            "Retargeting, using new bits at height {}",
            chain_head_height + 1
        );
        trace!("Last bits: {:?}", index_last.bits());
    }

    // let blocks_back = params.difficulty_adjustment_interval() - 1;
    // let height_first = index_last.height() - blocks_back;
    let index_first = get_block_at_height(chain_head_height)?;

    let next_work =
        calculate_next_work_required(auxpow_height_difference, index_last.bits(), params);

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
        let head_height = self.chain.get_head()?.message.height();
        get_next_work_required(
            |height| self.chain.get_block_at_height(height),
            index_last,
            &self.retarget_params,
            self.get_target_override(),
            head_height,
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

        trace!(
            "Index last hash={} height={}",
            index_last.block_hash(),
            index_last.height()
        );

        let hashes = self.chain.get_aggregate_hashes().await?;
        // trace!("Found {} hashes", hashes.len());

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

    const PREV_BITS: u32 = 505544640;

    fn init_tracing() {
        tracing_subscriber::fmt()
            .with_test_writer() // Important: use test writer!
            .with_env_filter("trace") // Set desired level
            .try_init()
            .ok();
    }

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
    fn should_increase_target_to_make_it_easier_when_timespan_is_larger_then_target() {
        init_tracing();

        let actual_timespan = 150_000_u32;
        let target_timespan = 100_000_u64;

        let test_consensus_params = BitcoinConsensusParams {
            pow_target_spacing: target_timespan,
            max_pow_adjustment: 20,
            ..Default::default()
        };
        let previous_target =
            target_to_compact_lossy(uint256_target_from_compact(PREV_BITS)).to_consensus();

        let target =
            calculate_next_work_required(actual_timespan, PREV_BITS, &test_consensus_params);

        let target = target.to_consensus();

        println!("New Target: {}", target);
        println!("Previous Target: {}", previous_target);

        assert!(target > previous_target);
    }

    #[test]
    fn should_decrease_target_to_make_it_harder_when_timespan_is_shorter_then_target() {
        init_tracing();

        let actual_timespan = 50_000_u32;
        let target_timespan = 100_000_u64;

        let test_consensus_params = BitcoinConsensusParams {
            pow_target_spacing: target_timespan,
            max_pow_adjustment: 20,
            ..Default::default()
        };

        let previous_target = target_to_compact_lossy(uint256_target_from_compact(PREV_BITS));

        let target =
            calculate_next_work_required(actual_timespan, PREV_BITS, &test_consensus_params);

        // let target = target;

        println!("New Target: {:?}", target);
        println!("Previous Target: {:?}", previous_target);

        assert!(target < previous_target);
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
