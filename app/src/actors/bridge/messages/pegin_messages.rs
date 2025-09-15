//! Peg-In Actor Messages
//! 
//! Messages for Bitcoin deposit processing and validation

use actix::prelude::*;
use bitcoin::{Transaction, Txid, TxOut};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use crate::types::*;
use crate::actors::bridge::shared::errors::BridgeError;

// Import the actual actor instead of forward declaration
pub use super::super::actors::pegin::actor::{PegInActor, PegInActorStatus};

/// Peg-in workflow messages
#[derive(Debug, Clone, Message, Serialize, Deserialize)]
#[rtype(result = "Result<PegInResponse, BridgeError>")]
pub enum PegInMessage {
    /// Process new deposit detection
    ProcessDeposit {
        txid: Txid,
        bitcoin_tx: Transaction,
        block_height: u32,
    },
    
    /// Validate deposit transaction
    ValidateDeposit {
        pegin_id: String,
        deposit: DepositTransaction,
    },
    
    /// Update confirmation count
    UpdateConfirmations {
        pegin_id: String,
        confirmations: u32,
    },
    
    /// Confirm deposit is ready for minting
    ConfirmDeposit {
        pegin_id: String,
    },
    
    /// Notify minting completion
    NotifyMinting {
        pegin_id: String,
        alys_tx_hash: H256,
        amount: u64,
    },
    
    /// Get deposit status
    GetDepositStatus {
        pegin_id: String,
    },
    
    /// List pending deposits
    ListPendingDeposits,
    
    /// Force retry failed deposit
    RetryDeposit {
        pegin_id: String,
    },
    
    /// Cancel deposit processing
    CancelDeposit {
        pegin_id: String,
        reason: String,
    },
    
    /// Initialize the peg-in actor
    Initialize,
    
    /// Get actor status
    GetStatus,
    
    /// Shutdown the actor
    Shutdown,
}

/// Peg-in response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PegInResponse {
    DepositProcessed { pegin_id: String },
    DepositValidated { pegin_id: String, valid: bool },
    ConfirmationsUpdated { pegin_id: String, confirmations: u32 },
    DepositConfirmed { pegin_id: String },
    MintingNotified { pegin_id: String },
    DepositStatus(DepositStatus),
    PendingDeposits(Vec<PendingDeposit>),
    DepositRetried { pegin_id: String },
    DepositCancelled { pegin_id: String },
    Initialized,
    StatusReported(PegInActorStatus),
    Shutdown,
}

/// Deposit transaction details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositTransaction {
    pub txid: Txid,
    pub bitcoin_tx: Transaction,
    pub federation_output: TxOut,
    pub op_return_data: Option<Vec<u8>>,
    pub evm_address: Option<H160>,
    pub amount: u64,
    pub block_height: u32,
    pub detected_at: SystemTime,
}

/// Pending deposit state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDeposit {
    pub pegin_id: String,
    pub txid: Txid,
    pub bitcoin_tx: Transaction,
    pub federation_output: TxOut,
    pub evm_address: H160,
    pub amount: u64,
    pub confirmations: u32,
    pub status: DepositStatus,
    pub created_at: SystemTime,
    pub last_updated: SystemTime,
    pub retry_count: u32,
}

/// Deposit processing status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DepositStatus {
    Detected,
    Validating,
    ValidationFailed { reason: String },
    ConfirmationPending { 
        current: u32, 
        required: u32 
    },
    Confirmed,
    Minting,
    Completed {
        alys_tx_hash: H256,
        minted_amount: u64,
    },
    Failed { 
        reason: String,
        recoverable: bool,
    },
    Cancelled { reason: String },
}

/// Deposit validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositValidationResult {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub extracted_address: Option<H160>,
    pub validated_amount: Option<u64>,
}

/// Validation issue types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationIssue {
    InvalidFederationOutput,
    InvalidOpReturn,
    InvalidEvmAddress,
    InsufficientAmount,
    DuplicateDeposit,
    NetworkMismatch,
    Other(String),
}

/// Confirmation tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationTracker {
    pub required_confirmations: u32,
    pub current_confirmations: u32,
    pub last_check: SystemTime,
    pub confirmation_history: Vec<ConfirmationUpdate>,
}

/// Confirmation update record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationUpdate {
    pub confirmations: u32,
    pub block_height: u32,
    pub timestamp: SystemTime,
}

// AlysMessage trait implementation
use actor_system::message::{AlysMessage, MessagePriority};
use std::time::Duration;

impl AlysMessage for PegInMessage {
    fn message_type(&self) -> &'static str {
        match self {
            PegInMessage::ProcessDeposit { .. } => "ProcessDeposit",
            PegInMessage::ValidateDeposit { .. } => "ValidateDeposit",
            PegInMessage::UpdateConfirmations { .. } => "UpdateConfirmations",
            PegInMessage::ConfirmDeposit { .. } => "ConfirmDeposit",
            PegInMessage::NotifyMinting { .. } => "NotifyMinting",
            PegInMessage::GetDepositStatus { .. } => "GetDepositStatus",
            PegInMessage::ListPendingDeposits => "ListPendingDeposits",
            PegInMessage::RetryDeposit { .. } => "RetryDeposit",
            PegInMessage::CancelDeposit { .. } => "CancelDeposit",
            PegInMessage::Initialize => "Initialize",
            PegInMessage::GetStatus => "GetStatus",
            PegInMessage::Shutdown => "Shutdown",
        }
    }

    fn priority(&self) -> MessagePriority {
        match self {
            PegInMessage::Shutdown => MessagePriority::Critical,
            PegInMessage::Initialize => MessagePriority::High,
            PegInMessage::ProcessDeposit { .. } => MessagePriority::High,
            PegInMessage::ValidateDeposit { .. } => MessagePriority::High,
            PegInMessage::ConfirmDeposit { .. } => MessagePriority::High,
            PegInMessage::NotifyMinting { .. } => MessagePriority::High,
            PegInMessage::RetryDeposit { .. } => MessagePriority::High,
            PegInMessage::UpdateConfirmations { .. } => MessagePriority::Normal,
            PegInMessage::CancelDeposit { .. } => MessagePriority::Normal,
            PegInMessage::GetDepositStatus { .. } => MessagePriority::Low,
            PegInMessage::ListPendingDeposits => MessagePriority::Low,
            PegInMessage::GetStatus => MessagePriority::Low,
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            PegInMessage::ProcessDeposit { .. } => Duration::from_secs(120),
            PegInMessage::ValidateDeposit { .. } => Duration::from_secs(60),
            PegInMessage::ConfirmDeposit { .. } => Duration::from_secs(60),
            PegInMessage::Initialize => Duration::from_secs(60),
            PegInMessage::Shutdown => Duration::from_secs(30),
            _ => Duration::from_secs(30),
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            PegInMessage::ProcessDeposit { .. } => true,
            PegInMessage::ValidateDeposit { .. } => true,
            PegInMessage::ConfirmDeposit { .. } => true,
            PegInMessage::RetryDeposit { .. } => false, // Already a retry
            PegInMessage::CancelDeposit { .. } => false, // Cancellation is final
            PegInMessage::Shutdown => false, // Shutdown is final
            _ => true,
        }
    }

    fn max_retries(&self) -> u32 {
        match self {
            PegInMessage::ProcessDeposit { .. } => 5,
            PegInMessage::ValidateDeposit { .. } => 3,
            PegInMessage::ConfirmDeposit { .. } => 3,
            _ => 3,
        }
    }
}