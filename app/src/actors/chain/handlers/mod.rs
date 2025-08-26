//! Chain Actor Message Handlers
//!
//! This module organizes all message handlers for the ChainActor by functional area:
//! - Block operations (import, production, validation)
//! - Consensus operations (Aura PoA, slot management)  
//! - Auxiliary Proof-of-Work (Bitcoin merged mining)
//! - Peg operations (two-way peg between Bitcoin and Alys)

pub mod block_handlers;
pub mod consensus_handlers;
pub mod auxpow_handlers;
pub mod peg_handlers;

// Re-export configuration types and managers
pub use block_handlers::{BlockProcessingConfig, BlockProcessingQueue, BlockProcessingPriority, PendingBlockInfo};
pub use consensus_handlers::{AuraConfig, AuraConsensusManager, SlotSchedule, ValidatorMetrics};
pub use auxpow_handlers::{FinalizationConfig, FinalizationManager, FinalizationEntry};
pub use peg_handlers::{PegConfig, PegOperationManager, PegInState, PegOutState, PegInStatus, PegOutStatus};