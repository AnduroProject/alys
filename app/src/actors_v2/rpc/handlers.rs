use actix::Addr;
use ethereum_types::Address;
use serde_json::{json, Value};
use uuid::Uuid;
use bitcoin::hashes::hex::FromHex;
use bitcoin::BlockHash;
use bitcoin::consensus::Decodable;
use std::str::FromStr;

use crate::actors_v2::chain::ChainActor;
use crate::actors_v2::chain::messages::{CreateAuxBlock, SubmitAuxBlock};
use crate::auxpow::AuxPow;
use super::error::RpcError;

/// createauxblock RPC handler
pub struct CreateAuxBlockHandler;

impl CreateAuxBlockHandler {
    /// Handle createauxblock request
    ///
    /// # Parameters
    /// - params[0]: miner_address (hex string, optional - uses zero address if not provided)
    ///
    /// # Returns
    /// JSON object containing:
    /// - hash: aggregate hash for mining (hex string)
    /// - chainid: chain ID (integer)
    /// - previousblockhash: previous Bitcoin block hash (hex string)
    /// - coinbasevalue: coinbase reward value (integer)
    /// - bits: difficulty target (hex string)
    /// - height: target height after mining (integer)
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        // Parse miner address (optional parameter)
        let miner_address = if params.is_empty() {
            // Use zero address if not provided
            Address::zero()
        } else {
            let addr_str = params[0]
                .as_str()
                .ok_or_else(|| RpcError::InvalidParams("Expected string address".to_string()))?;

            // Remove "0x" prefix if present
            let addr_str = addr_str.trim_start_matches("0x");

            Address::from_slice(
                &hex::decode(addr_str)
                    .map_err(|e| RpcError::InvalidParams(format!("Invalid address hex: {}", e)))?,
            )
        };

        // Create correlation ID
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            miner_address = %miner_address,
            "createauxblock request received"
        );

        // Send message to ChainActor
        let message = CreateAuxBlock {
            miner_address,
            correlation_id,
        };

        let aux_block = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?
            .map_err(RpcError::ChainError)?;

        tracing::info!(
            correlation_id = %correlation_id,
            hash = %aux_block.hash,
            "createauxblock completed successfully"
        );

        // Convert AuxBlock to JSON (serde handles the field serialization)
        let response = serde_json::to_value(&aux_block)
            .map_err(|e| RpcError::Internal(format!("Failed to serialize AuxBlock: {}", e)))?;

        Ok(response)
    }
}

/// submitauxblock RPC handler
pub struct SubmitAuxBlockHandler;

impl SubmitAuxBlockHandler {
    /// Handle submitauxblock request
    ///
    /// # Parameters
    /// - params[0]: hash (aggregate hash from createauxblock, hex string)
    /// - params[1]: auxpow (serialized AuxPoW hex string)
    ///
    /// # Returns
    /// Boolean: true if submission accepted, false otherwise
    pub async fn handle(
        params: Vec<Value>,
        chain_actor: Addr<ChainActor>,
    ) -> Result<Value, RpcError> {
        // Validate parameter count
        if params.len() != 2 {
            return Err(RpcError::InvalidParams(
                "Expected 2 parameters: hash and auxpow".to_string(),
            ));
        }

        // Parse aggregate hash
        let hash_str = params[0]
            .as_str()
            .ok_or_else(|| RpcError::InvalidParams("Expected string hash".to_string()))?;

        let aggregate_hash = BlockHash::from_str(hash_str)
            .map_err(|e| RpcError::InvalidParams(format!("Invalid hash: {}", e)))?;

        // Parse AuxPoW hex
        let auxpow_hex = params[1]
            .as_str()
            .ok_or_else(|| RpcError::InvalidParams("Expected string auxpow".to_string()))?;

        let auxpow_bytes = Vec::<u8>::from_hex(auxpow_hex)
            .map_err(|e| RpcError::InvalidParams(format!("Invalid auxpow hex: {}", e)))?;

        // Deserialize AuxPoW using Bitcoin's Decodable trait
        let auxpow = AuxPow::consensus_decode(&mut &auxpow_bytes[..])
            .map_err(|e| RpcError::InvalidParams(format!("Invalid auxpow structure: {:?}", e)))?;

        // Create correlation ID
        let correlation_id = Uuid::new_v4();

        tracing::debug!(
            correlation_id = %correlation_id,
            hash = %aggregate_hash,
            auxpow_size = auxpow_bytes.len(),
            "submitauxblock request received"
        );

        // Send message to ChainActor
        let message = SubmitAuxBlock {
            aggregate_hash,
            auxpow,
            correlation_id,
        };

        // Attempt submission
        let result = chain_actor
            .send(message)
            .await
            .map_err(|e| RpcError::MailboxError(e.to_string()))?;

        match result {
            Ok(auxpow_header) => {
                tracing::info!(
                    correlation_id = %correlation_id,
                    hash = %aggregate_hash,
                    height = auxpow_header.height,
                    "submitauxblock accepted successfully"
                );
                Ok(json!(true))
            }
            Err(e) => {
                tracing::warn!(
                    correlation_id = %correlation_id,
                    hash = %aggregate_hash,
                    error = ?e,
                    "submitauxblock rejected"
                );
                // Return false (not an error) - Bitcoin convention
                Ok(json!(false))
            }
        }
    }
}
