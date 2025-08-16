//! Consensus-related types and structures

use crate::types::*;
use serde::{Deserialize, Serialize};

/// Synchronization status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    Idle,
    Syncing {
        current_block: u64,
        target_block: u64,
        progress: f64,
        syncing_peers: Vec<PeerId>,
    },
    UpToDate,
    Stalled {
        reason: String,
        last_progress: std::time::SystemTime,
    },
}

/// Consensus state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusState {
    pub current_epoch: u64,
    pub current_slot: u64,
    pub finalized_epoch: u64,
    pub finalized_block: BlockRef,
    pub justified_epoch: u64,
    pub justified_block: BlockRef,
}

/// Validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: Address,
    pub public_key: PublicKey,
    pub stake: U256,
    pub is_active: bool,
    pub activation_epoch: u64,
    pub exit_epoch: Option<u64>,
}

/// Validator set for consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSet {
    pub validators: Vec<ValidatorInfo>,
    pub total_stake: U256,
    pub epoch: u64,
}

/// Attestation from validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub validator_index: u64,
    pub slot: u64,
    pub beacon_block_root: BlockHash,
    pub source_epoch: u64,
    pub target_epoch: u64,
    pub signature: Signature,
}

/// Aggregated attestations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateAttestation {
    pub attestation: Attestation,
    pub aggregation_bits: Vec<bool>,
    pub signature: Signature,
}

/// Slashing evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlashingEvidence {
    DoubleVote {
        validator_index: u64,
        vote1: Attestation,
        vote2: Attestation,
    },
    SurroundVote {
        validator_index: u64,
        surrounding: Attestation,
        surrounded: Attestation,
    },
}

/// Fork choice rule implementation
#[derive(Debug, Clone)]
pub struct ForkChoice {
    pub justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,
    pub block_scores: std::collections::HashMap<BlockHash, Score>,
    pub block_tree: BlockTree,
}

/// Checkpoint in consensus
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    pub epoch: u64,
    pub root: BlockHash,
}

/// Block score for fork choice
#[derive(Debug, Clone)]
pub struct Score {
    pub vote_weight: U256,
    pub block_hash: BlockHash,
    pub parent_score: U256,
}

/// Block tree for fork choice
#[derive(Debug, Clone)]
pub struct BlockTree {
    pub blocks: std::collections::HashMap<BlockHash, BlockNode>,
    pub genesis_hash: BlockHash,
}

/// Node in the block tree
#[derive(Debug, Clone)]
pub struct BlockNode {
    pub block_ref: BlockRef,
    pub parent_hash: BlockHash,
    pub children: Vec<BlockHash>,
    pub weight: U256,
    pub justified: bool,
    pub finalized: bool,
}

/// Consensus message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    Block(ConsensusBlock),
    Attestation(Attestation),
    AggregateAttestation(AggregateAttestation),
    SlashingProof(SlashingEvidence),
    SyncCommitteeContribution(SyncCommitteeContribution),
}

/// Sync committee contribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCommitteeContribution {
    pub slot: u64,
    pub beacon_block_root: BlockHash,
    pub subcommittee_index: u64,
    pub aggregation_bits: Vec<bool>,
    pub signature: Signature,
}

/// Consensus error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusError {
    InvalidBlock { reason: String },
    InvalidAttestation { reason: String },
    SlashableOffense { evidence: SlashingEvidence },
    ForkChoiceError { reason: String },
    InvalidSignature,
    UnknownValidator { validator_index: u64 },
    InsufficientStake,
    EpochTooOld { epoch: u64 },
    DuplicateAttestation,
}

/// Finalization status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinalizationStatus {
    Unfinalized,
    Justified {
        epoch: u64,
        checkpoint: Checkpoint,
    },
    Finalized {
        epoch: u64,
        checkpoint: Checkpoint,
        finalized_at: std::time::SystemTime,
    },
}

/// Consensus metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusMetrics {
    pub current_epoch: u64,
    pub finalized_epoch: u64,
    pub participation_rate: f64,
    pub attestation_inclusion_distance: f64,
    pub validator_count: u64,
    pub active_validator_count: u64,
    pub total_stake: U256,
    pub average_block_time: std::time::Duration,
}

/// Proof of Work related types (for auxiliary PoW)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryProofOfWork {
    pub parent_block: BlockHash,
    pub coinbase_tx: Vec<u8>,
    pub merkle_branch: Vec<Hash256>,
    pub merkle_index: u32,
    pub parent_block_header: Vec<u8>,
}

/// PoW validation result
#[derive(Debug, Clone)]
pub struct PoWValidationResult {
    pub valid: bool,
    pub target: U256,
    pub hash: Hash256,
    pub difficulty: U256,
}

impl SyncStatus {
    /// Check if currently syncing
    pub fn is_syncing(&self) -> bool {
        matches!(self, SyncStatus::Syncing { .. })
    }
    
    /// Get sync progress (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        match self {
            SyncStatus::Syncing { progress, .. } => *progress,
            SyncStatus::UpToDate => 1.0,
            _ => 0.0,
        }
    }
    
    /// Get estimated blocks remaining
    pub fn blocks_remaining(&self) -> Option<u64> {
        match self {
            SyncStatus::Syncing { current_block, target_block, .. } => {
                Some(target_block.saturating_sub(*current_block))
            }
            _ => None,
        }
    }
}

impl ValidatorSet {
    /// Create new validator set
    pub fn new(validators: Vec<ValidatorInfo>, epoch: u64) -> Self {
        let total_stake = validators
            .iter()
            .filter(|v| v.is_active)
            .map(|v| v.stake)
            .sum();
            
        Self {
            validators,
            total_stake,
            epoch,
        }
    }
    
    /// Get active validators
    pub fn active_validators(&self) -> Vec<&ValidatorInfo> {
        self.validators
            .iter()
            .filter(|v| v.is_active)
            .collect()
    }
    
    /// Get validator by index
    pub fn get_validator(&self, index: u64) -> Option<&ValidatorInfo> {
        self.validators.get(index as usize)
    }
    
    /// Check if validator exists and is active
    pub fn is_active_validator(&self, address: &Address) -> bool {
        self.validators
            .iter()
            .any(|v| v.address == *address && v.is_active)
    }
    
    /// Get validator count
    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }
    
    /// Get active validator count
    pub fn active_validator_count(&self) -> usize {
        self.validators.iter().filter(|v| v.is_active).count()
    }
}

impl ValidatorInfo {
    /// Create new validator info
    pub fn new(
        address: Address,
        public_key: PublicKey,
        stake: U256,
        activation_epoch: u64,
    ) -> Self {
        Self {
            address,
            public_key,
            stake,
            is_active: true,
            activation_epoch,
            exit_epoch: None,
        }
    }
    
    /// Check if validator is active at given epoch
    pub fn is_active_at_epoch(&self, epoch: u64) -> bool {
        self.is_active 
            && epoch >= self.activation_epoch
            && self.exit_epoch.map_or(true, |exit| epoch < exit)
    }
    
    /// Get effective balance (may be different from stake)
    pub fn effective_balance(&self) -> U256 {
        // For now, effective balance equals stake
        // In practice, this might be capped or adjusted
        self.stake
    }
}

impl Attestation {
    /// Create new attestation
    pub fn new(
        validator_index: u64,
        slot: u64,
        beacon_block_root: BlockHash,
        source_epoch: u64,
        target_epoch: u64,
    ) -> Self {
        Self {
            validator_index,
            slot,
            beacon_block_root,
            source_epoch,
            target_epoch,
            signature: [0u8; 64], // Will be filled during signing
        }
    }
    
    /// Check if attestation is slashable with another
    pub fn is_slashable_with(&self, other: &Attestation) -> bool {
        // Double vote: same target epoch, different beacon block roots
        if self.target_epoch == other.target_epoch 
            && self.beacon_block_root != other.beacon_block_root {
            return true;
        }
        
        // Surround vote: one attestation surrounds the other
        if (self.source_epoch < other.source_epoch && self.target_epoch > other.target_epoch)
            || (other.source_epoch < self.source_epoch && other.target_epoch > self.target_epoch) {
            return true;
        }
        
        false
    }
}

impl ForkChoice {
    /// Create new fork choice instance
    pub fn new(genesis_hash: BlockHash) -> Self {
        let genesis_checkpoint = Checkpoint {
            epoch: 0,
            root: genesis_hash,
        };
        
        let mut block_tree = BlockTree {
            blocks: std::collections::HashMap::new(),
            genesis_hash,
        };
        
        // Add genesis block
        block_tree.blocks.insert(genesis_hash, BlockNode {
            block_ref: BlockRef::genesis(genesis_hash),
            parent_hash: BlockHash::zero(),
            children: Vec::new(),
            weight: U256::zero(),
            justified: true,
            finalized: true,
        });
        
        Self {
            justified_checkpoint: genesis_checkpoint.clone(),
            finalized_checkpoint: genesis_checkpoint,
            block_scores: std::collections::HashMap::new(),
            block_tree,
        }
    }
    
    /// Get head block according to fork choice rule
    pub fn get_head(&self) -> BlockHash {
        // Simplified GHOST rule: choose the block with highest weight
        // among children of finalized block
        self.find_head_recursive(self.finalized_checkpoint.root)
    }
    
    /// Apply attestation to fork choice
    pub fn apply_attestation(&mut self, attestation: &Attestation) {
        // Update block weights based on attestation
        if let Some(node) = self.block_tree.blocks.get_mut(&attestation.beacon_block_root) {
            node.weight += U256::one(); // Simplified: each attestation adds 1 weight
        }
    }
    
    /// Add block to fork choice
    pub fn add_block(&mut self, block_ref: BlockRef) {
        let node = BlockNode {
            block_ref: block_ref.clone(),
            parent_hash: block_ref.parent_hash,
            children: Vec::new(),
            weight: U256::zero(),
            justified: false,
            finalized: false,
        };
        
        // Add as child to parent
        if let Some(parent) = self.block_tree.blocks.get_mut(&block_ref.parent_hash) {
            parent.children.push(block_ref.hash);
        }
        
        self.block_tree.blocks.insert(block_ref.hash, node);
    }
    
    /// Recursive head finding using GHOST rule
    fn find_head_recursive(&self, block_hash: BlockHash) -> BlockHash {
        if let Some(node) = self.block_tree.blocks.get(&block_hash) {
            if node.children.is_empty() {
                return block_hash;
            }
            
            // Find child with highest weight
            let best_child = node.children
                .iter()
                .max_by_key(|&child_hash| {
                    self.block_tree.blocks
                        .get(child_hash)
                        .map(|child| child.weight)
                        .unwrap_or(U256::zero())
                })
                .copied()
                .unwrap_or(block_hash);
                
            return self.find_head_recursive(best_child);
        }
        
        block_hash
    }
}

impl ConsensusMetrics {
    /// Create new consensus metrics
    pub fn new() -> Self {
        Self {
            current_epoch: 0,
            finalized_epoch: 0,
            participation_rate: 0.0,
            attestation_inclusion_distance: 0.0,
            validator_count: 0,
            active_validator_count: 0,
            total_stake: U256::zero(),
            average_block_time: std::time::Duration::from_secs(12),
        }
    }
    
    /// Update participation rate
    pub fn update_participation_rate(&mut self, expected: u64, actual: u64) {
        if expected > 0 {
            self.participation_rate = (actual as f64) / (expected as f64);
        }
    }
    
    /// Check if consensus is healthy
    pub fn is_healthy(&self) -> bool {
        self.participation_rate > 0.67 && // More than 2/3 participation
        self.current_epoch - self.finalized_epoch < 3 // Finality not too far behind
    }
}

impl Default for ConsensusMetrics {
    fn default() -> Self {
        Self::new()
    }
}