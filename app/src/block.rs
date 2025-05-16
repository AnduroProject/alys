use crate::{
    aura::Authority,
    auxpow::AuxPow,
    auxpow_miner::BlockIndex,
    error::Error,
    signatures::{AggregateApproval, CheckedIndividualApproval, IndividualApproval},
    spec::ChainSpec,
    store::BlockRef,
};
use bitcoin::{hashes::Hash, BlockHash, Transaction as BitcoinTransaction, Txid};
use lighthouse_wrapper::bls::PublicKey;
use lighthouse_wrapper::types::{
    Address, EthSpec, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadCapella, FixedVector,
    Hash256, MainnetEthSpec, Transactions, Uint256, VariableList, Withdrawals,
};
use serde_derive::{Deserialize, Serialize};

pub trait ConvertBlockHash<H> {
    fn to_block_hash(&self) -> H;
}

impl ConvertBlockHash<BlockHash> for Hash256 {
    fn to_block_hash(&self) -> BlockHash {
        BlockHash::from_slice(self.as_bytes()).expect("Should have same length hash")
    }
}

impl ConvertBlockHash<Hash256> for BlockHash {
    fn to_block_hash(&self) -> Hash256 {
        Hash256::from_slice(self.as_byte_array())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AuxPowHeader {
    /// The oldest block covered by this AuxPoW
    pub range_start: Hash256,
    /// The newest block covered by this AuxPoW (inclusive)
    pub range_end: Hash256,
    /// The difficulty target in compact form
    pub bits: u32,
    /// The ID of the chain used to isolate the AuxPow merkle branch
    pub chain_id: u32,
    /// The height of the AuxPow, used for difficulty adjustment
    pub height: u64,
    /// The AuxPow itself, only None at genesis
    pub auxpow: Option<AuxPow>,
    /// The miner's EVM address
    pub fee_recipient: Address,
}

// this is the sidechain block (pre-signing) that contains
// the embedded payload from the execution layer
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ConsensusBlock<T: EthSpec> {
    /// The block hash of the parent
    pub parent_hash: Hash256,
    /// Aura slot the block was produced in
    pub slot: u64,
    /// Proof of work, used for finalization. Not every block is expected to have this.
    pub auxpow_header: Option<AuxPowHeader>,
    // we always assume the geth node is configured
    // to start after the capella hard fork
    pub execution_payload: ExecutionPayloadCapella<T>,
    /// Transactions that are sending funds to the bridge
    pub pegins: Vec<(Txid, BlockHash)>,
    /// Bitcoin payments for pegouts
    pub pegout_payment_proposal: Option<BitcoinTransaction>,
    /// Finalized bitcoin payments. Only non-empty if there is an auxpow.
    /// Note: technically we only need the signatures rather than the whole
    /// tx, but that's left as a future optimization. We could even completely
    /// omit the field but for now it's nice to have a public record
    pub finalized_pegouts: Vec<BitcoinTransaction>,
}

// NOTE: implementation assumes ConsensusBlock contains auxpow_header
// i.e. it is only called for those blocks retrieved from storage
impl BlockIndex for ConsensusBlock<MainnetEthSpec> {
    fn block_hash(&self) -> BlockHash {
        self.signing_root().to_block_hash()
    }

    fn block_time(&self) -> u64 {
        self.execution_payload.timestamp
    }

    fn bits(&self) -> u32 {
        self.auxpow_header
            .as_ref()
            .map(|header| header.bits)
            .expect("Should contain AuxPow")
    }

    fn chain_id(&self) -> u32 {
        self.auxpow_header
            .as_ref()
            .map(|header| header.chain_id)
            .expect("Should contain AuxPow")
    }

    fn height(&self) -> u64 {
        self.execution_payload.block_number
    }
}

impl Default for ConsensusBlock<MainnetEthSpec> {
    fn default() -> Self {
        Self {
            parent_hash: Hash256::zero(),
            slot: 0,
            auxpow_header: None,
            execution_payload: ExecutionPayloadCapella {
                parent_hash: ExecutionBlockHash::zero(),
                fee_recipient: Address::zero(),
                state_root: Hash256::zero(),
                receipts_root: Hash256::zero(),
                logs_bloom: FixedVector::default(),
                prev_randao: Hash256::zero(),
                block_number: 0,
                gas_limit: 0,
                gas_used: 0,
                timestamp: 0,
                extra_data: VariableList::default(),
                base_fee_per_gas: Uint256::zero(),
                block_hash: ExecutionBlockHash::zero(),
                transactions: Transactions::<MainnetEthSpec>::default(),
                withdrawals: Withdrawals::<MainnetEthSpec>::default(),
            },
            pegins: vec![],
            pegout_payment_proposal: None,
            finalized_pegouts: vec![],
        }
    }
}

impl ConsensusBlock<MainnetEthSpec> {
    pub fn new(
        slot: u64,
        payload: ExecutionPayload<MainnetEthSpec>,
        prev: Hash256,
        auxpow_header: Option<AuxPowHeader>,
        pegins: Vec<(Txid, BlockHash)>,
        pegout_payment_proposal: Option<BitcoinTransaction>,
        finalized_pegouts: Vec<BitcoinTransaction>,
    ) -> Self {
        Self {
            slot,
            parent_hash: prev,
            execution_payload: payload.as_capella().unwrap().clone(),
            auxpow_header,
            pegins,
            pegout_payment_proposal,
            finalized_pegouts,
        }
    }

    fn signing_root(&self) -> Hash256 {
        tree_hash::merkle_root(&rmp_serde::to_vec(&self).unwrap(), 0)
    }

    pub fn sign(&self, authority: &Authority) -> CheckedIndividualApproval {
        let signing_root = self.signing_root();
        // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/validator_client/src/signing_method.rs#L163
        let signature = authority.signer.sk.sign(signing_root);

        IndividualApproval {
            signature,
            authority_index: authority.index,
        }
        .assume_checked()
    }

    pub fn sign_block(self, authority: &Authority) -> SignedConsensusBlock<MainnetEthSpec> {
        let approval = self.sign(authority).into_aggregate();

        SignedConsensusBlock {
            message: self,
            signature: approval,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SignedConsensusBlock<T: EthSpec> {
    pub message: ConsensusBlock<T>,
    // signed by the authority for that slot, plus the approvals of other authorities
    pub signature: AggregateApproval,
}

impl SignedConsensusBlock<MainnetEthSpec> {
    // https://github.com/sigp/lighthouse/blob/441fc1691b69f9edc4bbdc6665f3efab16265c9b/beacon_node/beacon_chain/src/block_verification.rs#L1893
    pub fn verify_signature(&self, public_keys: &[PublicKey]) -> bool {
        let message = self.message.signing_root();
        self.signature.verify(public_keys, message)
    }

    #[allow(dead_code)]
    pub fn is_signed_by(&self, authority_index: u8) -> bool {
        self.signature.is_signed_by(authority_index)
    }

    pub fn num_approvals(&self) -> usize {
        self.signature.num_approvals()
    }

    pub fn canonical_root(&self) -> Hash256 {
        self.message.signing_root()
    }

    pub fn add_approval(&mut self, approval: CheckedIndividualApproval) -> Result<(), Error> {
        self.signature.add_approval(approval)
    }

    pub fn block_ref(&self) -> BlockRef {
        BlockRef {
            hash: self.canonical_root(),
            height: self.message.execution_payload.block_number,
        }
    }

    pub fn genesis(
        chain_spec: ChainSpec,
        execution_payload: ExecutionPayloadCapella<MainnetEthSpec>,
    ) -> Self {
        // sanity checks
        if execution_payload.block_number != 0 {
            panic!("Execution payload should start at zero");
        }
        // TODO: https://github.com/bitcoin/bitcoin/blob/aa9231fafe45513134ec8953a217cda07446fae8/src/test/pow_tests.cpp#L176C1-L176C68
        Self {
            message: ConsensusBlock {
                parent_hash: Hash256::zero(),
                slot: 0, // TODO: calculate slot
                auxpow_header: Some(AuxPowHeader {
                    range_start: Hash256::zero(),
                    range_end: Hash256::zero(),
                    bits: chain_spec.bits,
                    chain_id: chain_spec.chain_id,
                    height: 0,
                    auxpow: None,
                    fee_recipient: Address::zero(),
                }),
                execution_payload,
                pegins: vec![],
                pegout_payment_proposal: None,
                finalized_pegouts: vec![],
            },
            signature: AggregateApproval::new(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lighthouse_wrapper::bls::Keypair;

    #[test]
    fn should_sign_block() {
        let block = ConsensusBlock::default();
        let key_pair = Keypair::random();

        let authority = Authority {
            signer: key_pair.clone(),
            index: 0,
        };

        let signed_block = block.sign_block(&authority);
        assert!(signed_block.verify_signature(&[key_pair.pk]));
    }
}
