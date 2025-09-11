//! Bridge Validation Utilities
//! 
//! Common validation logic for bridge operations

use bitcoin::{Transaction, TxOut, Address as BtcAddress, Network, Script};
use ethereum_types::{H160, H256};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::collections::HashMap;
use crate::types::*;
use super::constants::*;

/// Validation error placeholder (since validator crate is not available)
#[derive(Debug, Clone)]
pub struct ValidationFieldError {
    pub code: String,
    pub message: String,
}

/// Validation errors collection
#[derive(Debug, Clone)]
pub struct ValidationErrors {
    pub errors: HashMap<String, Vec<ValidationFieldError>>,
}

impl ValidationErrors {
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validation trait placeholder
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationErrors>;
}

/// Validation result with detailed error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult<T> {
    pub valid: bool,
    pub result: Option<T>,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

/// Validation error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationError {
    /// Invalid Bitcoin transaction structure
    InvalidTransaction(String),
    
    /// Invalid Bitcoin address
    InvalidBitcoinAddress(String),
    
    /// Invalid Ethereum address
    InvalidEthereumAddress(String),
    
    /// Amount validation errors
    AmountTooSmall { amount: u64, minimum: u64 },
    AmountTooLarge { amount: u64, maximum: u64 },
    
    /// Federation validation errors
    InvalidFederationOutput,
    UnknownFederationAddress,
    
    /// OP_RETURN validation errors
    InvalidOpReturn(String),
    MissingEthereumAddress,
    
    /// Network mismatch
    NetworkMismatch { expected: Network, found: Network },
    
    /// Duplicate transaction
    DuplicateTransaction { txid: bitcoin::Txid },
    
    /// Generic validation error
    Other(String),
}

/// Validation warnings (non-fatal issues)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationWarning {
    /// Low fee warning
    LowFee { current: u64, recommended: u64 },
    
    /// High fee warning
    HighFee { current: u64, maximum: u64 },
    
    /// Dust output warning
    DustOutput { amount: u64, dust_limit: u64 },
    
    /// Generic warning
    Other(String),
}

/// Bitcoin transaction validator
pub struct BitcoinTransactionValidator {
    network: Network,
    federation_addresses: Vec<BtcAddress>,
    federation_scripts: Vec<Script>,
}

impl BitcoinTransactionValidator {
    pub fn new(
        network: Network,
        federation_addresses: Vec<BtcAddress>,
        federation_scripts: Vec<Script>,
    ) -> Self {
        Self {
            network,
            federation_addresses,
            federation_scripts,
        }
    }

    /// Validate a peg-in transaction
    pub fn validate_pegin_transaction(&self, tx: &Transaction) -> ValidationResult<PegInValidation> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Basic transaction validation
        if tx.input.is_empty() {
            errors.push(ValidationError::InvalidTransaction("No inputs".to_string()));
        }

        if tx.output.is_empty() {
            errors.push(ValidationError::InvalidTransaction("No outputs".to_string()));
        }

        // Find federation output
        let federation_output = self.find_federation_output(tx);
        if federation_output.is_none() {
            errors.push(ValidationError::InvalidFederationOutput);
        }

        // Extract Ethereum address from OP_RETURN
        let ethereum_address = match self.extract_ethereum_address(tx) {
            Ok(addr) => Some(addr),
            Err(e) => {
                errors.push(e);
                None
            }
        };

        // Validate amount
        let amount = federation_output.as_ref().map(|output| output.value).unwrap_or(0);
        if amount < MIN_PEGIN_AMOUNT {
            errors.push(ValidationError::AmountTooSmall {
                amount,
                minimum: MIN_PEGIN_AMOUNT,
            });
        }

        // Check for dust outputs
        for output in &tx.output {
            if output.value < DUST_LIMIT && output.value > 0 {
                warnings.push(ValidationWarning::DustOutput {
                    amount: output.value,
                    dust_limit: DUST_LIMIT,
                });
            }
        }

        let result = if errors.is_empty() {
            Some(PegInValidation {
                federation_output: federation_output.cloned(),
                ethereum_address,
                amount,
                is_valid: true,
            })
        } else {
            None
        };

        ValidationResult {
            valid: errors.is_empty(),
            result,
            errors,
            warnings,
        }
    }

    /// Validate a peg-out burn event
    pub fn validate_pegout_burn(&self, burn_event: &BurnEvent) -> ValidationResult<PegOutValidation> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate destination address
        if let Err(e) = self.validate_bitcoin_address(&burn_event.destination_address) {
            errors.push(e);
        }

        // Validate amount
        if burn_event.amount < MIN_PEGOUT_AMOUNT {
            errors.push(ValidationError::AmountTooSmall {
                amount: burn_event.amount,
                minimum: MIN_PEGOUT_AMOUNT,
            });
        }

        if burn_event.amount > MAX_PEGOUT_AMOUNT {
            errors.push(ValidationError::AmountTooLarge {
                amount: burn_event.amount,
                maximum: MAX_PEGOUT_AMOUNT,
            });
        }

        // Validate Ethereum address format
        if !self.is_valid_ethereum_address(&burn_event.requester) {
            errors.push(ValidationError::InvalidEthereumAddress(
                format!("{:?}", burn_event.requester)
            ));
        }

        let result = if errors.is_empty() {
            Some(PegOutValidation {
                destination_valid: true,
                amount_valid: true,
                requester_valid: true,
            })
        } else {
            None
        };

        ValidationResult {
            valid: errors.is_empty(),
            result,
            errors,
            warnings,
        }
    }

    /// Find federation output in transaction
    fn find_federation_output(&self, tx: &Transaction) -> Option<&TxOut> {
        tx.output.iter().find(|output| {
            self.federation_scripts.iter().any(|script| {
                output.script_pubkey == *script
            })
        })
    }

    /// Extract Ethereum address from OP_RETURN output
    fn extract_ethereum_address(&self, tx: &Transaction) -> Result<H160, ValidationError> {
        // Find OP_RETURN output
        let op_return_output = tx.output.iter()
            .find(|output| output.script_pubkey.is_op_return());

        let op_return_output = op_return_output
            .ok_or(ValidationError::InvalidOpReturn("No OP_RETURN output found".to_string()))?;

        // Extract data from OP_RETURN
        let script = &op_return_output.script_pubkey;
        let data = script.as_bytes();
        
        if data.len() < 22 { // OP_RETURN + length + 20 bytes address
            return Err(ValidationError::InvalidOpReturn("OP_RETURN data too short".to_string()));
        }

        // Skip OP_RETURN opcode and length byte
        let addr_bytes = &data[2..22];
        if addr_bytes.len() != 20 {
            return Err(ValidationError::InvalidOpReturn("Invalid address length".to_string()));
        }

        Ok(H160::from_slice(addr_bytes))
    }

    /// Validate Bitcoin address for the current network
    fn validate_bitcoin_address(&self, address: &BtcAddress) -> Result<(), ValidationError> {
        if address.network != self.network {
            return Err(ValidationError::NetworkMismatch {
                expected: self.network,
                found: address.network,
            });
        }
        Ok(())
    }

    /// Check if Ethereum address is valid format
    fn is_valid_ethereum_address(&self, address: &H160) -> bool {
        // Basic format validation - non-zero address
        !address.is_zero()
    }
}

/// Peg-in validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegInValidation {
    pub federation_output: Option<TxOut>,
    pub ethereum_address: Option<H160>,
    pub amount: u64,
    pub is_valid: bool,
}

/// Peg-out validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PegOutValidation {
    pub destination_valid: bool,
    pub amount_valid: bool,
    pub requester_valid: bool,
}

/// Burn event structure for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnEvent {
    pub burn_tx_hash: H256,
    pub block_number: u64,
    pub log_index: u32,
    pub destination_address: BtcAddress,
    pub amount: u64,
    pub requester: H160,
    pub detected_at: std::time::SystemTime,
}

/// Address validation utilities
pub mod address_validation {
    use super::*;

    /// Validate Bitcoin address string
    pub fn validate_bitcoin_address_string(
        address_str: &str,
        network: Network,
    ) -> Result<BtcAddress, ValidationError> {
        let address = BtcAddress::from_str(address_str)
            .map_err(|e| ValidationError::InvalidBitcoinAddress(e.to_string()))?;

        if address.network != network {
            return Err(ValidationError::NetworkMismatch {
                expected: network,
                found: address.network,
            });
        }

        Ok(address)
    }

    /// Validate Ethereum address string
    pub fn validate_ethereum_address_string(address_str: &str) -> Result<H160, ValidationError> {
        if !address_str.starts_with("0x") || address_str.len() != 42 {
            return Err(ValidationError::InvalidEthereumAddress(
                "Invalid format, expected 0x followed by 40 hex characters".to_string()
            ));
        }

        let address_hex = &address_str[2..];
        let bytes = hex::decode(address_hex)
            .map_err(|_| ValidationError::InvalidEthereumAddress("Invalid hex encoding".to_string()))?;

        if bytes.len() != 20 {
            return Err(ValidationError::InvalidEthereumAddress("Invalid length".to_string()));
        }

        Ok(H160::from_slice(&bytes))
    }
}

/// Amount validation utilities
pub mod amount_validation {
    use super::*;

    /// Validate peg-in amount
    pub fn validate_pegin_amount(amount: u64) -> Result<(), ValidationError> {
        if amount < MIN_PEGIN_AMOUNT {
            return Err(ValidationError::AmountTooSmall {
                amount,
                minimum: MIN_PEGIN_AMOUNT,
            });
        }
        Ok(())
    }

    /// Validate peg-out amount
    pub fn validate_pegout_amount(amount: u64) -> Result<(), ValidationError> {
        if amount < MIN_PEGOUT_AMOUNT {
            return Err(ValidationError::AmountTooSmall {
                amount,
                minimum: MIN_PEGOUT_AMOUNT,
            });
        }

        if amount > MAX_PEGOUT_AMOUNT {
            return Err(ValidationError::AmountTooLarge {
                amount,
                maximum: MAX_PEGOUT_AMOUNT,
            });
        }

        Ok(())
    }

    /// Check if amount is dust
    pub fn is_dust(amount: u64) -> bool {
        amount < DUST_LIMIT
    }
}