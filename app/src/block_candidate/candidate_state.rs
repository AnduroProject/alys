use crate::block::SignedConsensusBlock;
use crate::error::Error;
use crate::network::ApproveBlock;
use crate::signatures::CheckedIndividualApproval;
use lighthouse_wrapper::bls::PublicKey;
use lighthouse_wrapper::store::MainnetEthSpec;

/// CandidateState enum represents the state of a block candidate.
#[allow(clippy::large_enum_variant)]
pub enum CandidateState {
    /// We received the block and approved of it
    CheckedBlock(SignedConsensusBlock<MainnetEthSpec>),
    /// We received approvals before we received the block - store them until we receive the block
    QueuedApprovals(Vec<CheckedIndividualApproval>),
}

impl CandidateState {
    pub fn add_unchecked_approval(
        &mut self,
        approval: ApproveBlock,
        authorities: &[PublicKey],
    ) -> Result<(), Error> {
        let checked_approval = approval.signature.check(approval.block_hash, authorities)?;
        self.add_checked_approval(checked_approval)
    }

    pub fn add_checked_approval(
        &mut self,
        approval: CheckedIndividualApproval,
    ) -> Result<(), Error> {
        match self {
            CandidateState::CheckedBlock(x) => {
                x.add_approval(approval)?;
            }
            CandidateState::QueuedApprovals(v) => {
                v.push(approval);
            }
        }
        Ok(())
    }

    pub fn add_checked_block(
        &mut self,
        block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        match self {
            CandidateState::QueuedApprovals(queued_approvals) => {
                let mut new_state = CandidateState::CheckedBlock(block);
                for approval in queued_approvals.drain(..) {
                    new_state.add_checked_approval(approval)?;
                }

                *self = new_state;
            }
            CandidateState::CheckedBlock(_) => {
                // Replace the existing block with the new one
                *self = CandidateState::CheckedBlock(block);
            }
        }
        Ok(())
    }

    /// Get the block contained in this CandidateState if it exists
    pub fn get_block(&self) -> Option<&SignedConsensusBlock<MainnetEthSpec>> {
        match self {
            CandidateState::CheckedBlock(block) => Some(block),
            CandidateState::QueuedApprovals(_) => None,
        }
    }
}

impl Default for CandidateState {
    fn default() -> Self {
        Self::QueuedApprovals(vec![])
    }
}
