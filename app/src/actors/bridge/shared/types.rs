//! Shared Bridge Types
//!
//! Common types used across bridge actors

use serde::{Deserialize, Serialize};

/// Unified operation event types for both PegIn and PegOut operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationEventType {
    // PegIn specific events
    DepositDetected,
    ValidationStarted,
    ValidationCompleted,
    ConfirmationStarted,
    ConfirmationUpdated,
    ConfirmationCompleted,
    MintingInitiated,
    MintingCompleted,

    // PegOut specific events
    BurnEventProcessed,
    TransactionBuilt,
    SignaturesRequested,
    SignaturesReceived,
    SignaturesApplied,
    TransactionBroadcast,
    TransactionConfirmed,
    PegOutCompleted,

    // Common events
    OperationFailed,
    OperationRetried,
}

impl OperationEventType {
    /// Check if this event type is PegIn specific
    pub fn is_pegin_event(&self) -> bool {
        matches!(self,
            OperationEventType::DepositDetected |
            OperationEventType::ValidationStarted |
            OperationEventType::ValidationCompleted |
            OperationEventType::ConfirmationStarted |
            OperationEventType::ConfirmationUpdated |
            OperationEventType::ConfirmationCompleted |
            OperationEventType::MintingInitiated |
            OperationEventType::MintingCompleted
        )
    }

    /// Check if this event type is PegOut specific
    pub fn is_pegout_event(&self) -> bool {
        matches!(self,
            OperationEventType::BurnEventProcessed |
            OperationEventType::TransactionBuilt |
            OperationEventType::SignaturesRequested |
            OperationEventType::SignaturesReceived |
            OperationEventType::SignaturesApplied |
            OperationEventType::TransactionBroadcast |
            OperationEventType::TransactionConfirmed |
            OperationEventType::PegOutCompleted
        )
    }

    /// Check if this event type is common to both operations
    pub fn is_common_event(&self) -> bool {
        matches!(self,
            OperationEventType::OperationFailed |
            OperationEventType::OperationRetried
        )
    }

    /// Get human-readable description of the event
    pub fn description(&self) -> &'static str {
        match self {
            OperationEventType::DepositDetected => "Bitcoin deposit detected",
            OperationEventType::ValidationStarted => "Deposit validation started",
            OperationEventType::ValidationCompleted => "Deposit validation completed",
            OperationEventType::ConfirmationStarted => "Confirmation monitoring started",
            OperationEventType::ConfirmationUpdated => "Confirmation count updated",
            OperationEventType::ConfirmationCompleted => "Required confirmations reached",
            OperationEventType::MintingInitiated => "Alys token minting initiated",
            OperationEventType::MintingCompleted => "Alys token minting completed",
            OperationEventType::BurnEventProcessed => "Burn event processed",
            OperationEventType::TransactionBuilt => "Bitcoin transaction built",
            OperationEventType::SignaturesRequested => "Signatures requested from federation",
            OperationEventType::SignaturesReceived => "Signatures received from federation",
            OperationEventType::SignaturesApplied => "Signatures applied to transaction",
            OperationEventType::TransactionBroadcast => "Transaction broadcast to Bitcoin network",
            OperationEventType::TransactionConfirmed => "Transaction confirmed on Bitcoin",
            OperationEventType::PegOutCompleted => "PegOut operation completed",
            OperationEventType::OperationFailed => "Operation failed",
            OperationEventType::OperationRetried => "Operation retried",
        }
    }
}