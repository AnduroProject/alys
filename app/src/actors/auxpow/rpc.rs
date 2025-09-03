//! RPC endpoints for external miners
//!
//! Provides Bitcoin-compatible RPC interface for mining pools and external
//! miners to interact with Alys merged mining system.

use std::str::FromStr;
use actix::prelude::*;
use tracing::*;

use bitcoin::BlockHash;
use ethereum_types::Address as EvmAddress;
use serde::{Deserialize, Serialize};

use crate::{
    auxpow::AuxPow,
    auxpow_miner::AuxBlock,
};

use super::{
    AuxPowActor,
    messages::{CreateAuxBlock, SubmitAuxBlock, GetMiningStatus, MiningStatus},
    error::AuxPowError,
};

/// RPC context for AuxPow operations
#[derive(Clone)]
pub struct AuxPowRpcContext {
    /// Reference to the AuxPow actor
    pub auxpow_actor: Addr<AuxPowActor>,
}

impl AuxPowRpcContext {
    /// Create new RPC context
    pub fn new(auxpow_actor: Addr<AuxPowActor>) -> Self {
        Self { auxpow_actor }
    }
}

/// RPC error types for mining operations
#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl From<AuxPowError> for RpcError {
    fn from(error: AuxPowError) -> Self {
        match error {
            AuxPowError::ChainSyncing => RpcError {
                code: -1,
                message: "Chain is currently syncing".to_string(),
                data: None,
            },
            AuxPowError::UnknownBlock => RpcError {
                code: -2,
                message: "Unknown block hash".to_string(),
                data: None,
            },
            AuxPowError::InvalidPow => RpcError {
                code: -3,
                message: "Invalid proof of work".to_string(),
                data: None,
            },
            AuxPowError::InvalidAuxpow => RpcError {
                code: -4,
                message: "Invalid auxiliary proof of work".to_string(),
                data: None,
            },
            AuxPowError::MiningDisabled => RpcError {
                code: -5,
                message: "Mining is disabled".to_string(),
                data: None,
            },
            _ => RpcError {
                code: -32603,
                message: "Internal error".to_string(),
                data: Some(serde_json::json!({ "error": format!("{:?}", error) })),
            },
        }
    }
}

impl AuxPowRpcContext {
    /// RPC endpoint: createauxblock <address>
    /// 
    /// Creates a new auxiliary block for mining. Returns work package
    /// that external miners can use to perform merged mining.
    /// 
    /// This is the exact equivalent of Bitcoin's createauxblock RPC call
    /// used by mining pools for merged mining operations.
    pub async fn create_aux_block_rpc(
        &self,
        address: String,
    ) -> Result<AuxBlock, RpcError> {
        debug!("RPC createauxblock called with address: {}", address);
        
        // Parse and validate mining address
        let evm_address = address.parse::<EvmAddress>()
            .map_err(|_| RpcError {
                code: -8,
                message: format!("Invalid address format: {}", address),
                data: None,
            })?;

        // Send create aux block request to actor
        let aux_block = self.auxpow_actor
            .send(CreateAuxBlock { address: evm_address })
            .await
            .map_err(|e| RpcError {
                code: -32603,
                message: "Actor communication failed".to_string(),
                data: Some(serde_json::json!({ "actor_error": e.to_string() })),
            })?
            .map_err(RpcError::from)?;

        info!(
            block_hash = %aux_block.hash,
            chain_id = aux_block.chain_id,
            height = aux_block.height,
            difficulty = %aux_block.bits.to_consensus(),
            "Created aux block for mining"
        );

        Ok(aux_block)
    }

    /// RPC endpoint: submitauxblock <hash> <auxpow>
    /// 
    /// Submits a completed auxiliary proof of work for validation and
    /// chain finalization. Returns true if accepted, false if rejected.
    /// 
    /// This is the exact equivalent of Bitcoin's submitauxblock RPC call
    /// used by mining pools to submit completed work.
    pub async fn submit_aux_block_rpc(
        &self,
        hash_hex: String,
        auxpow_hex: String,
    ) -> Result<bool, RpcError> {
        debug!("RPC submitauxblock called with hash: {}, auxpow length: {}", 
               hash_hex, auxpow_hex.len());
        
        // Parse block hash
        let hash = BlockHash::from_str(&hash_hex)
            .map_err(|_| RpcError {
                code: -8,
                message: format!("Invalid hash format: {}", hash_hex),
                data: None,
            })?;
        
        // Parse auxpow hex data
        let auxpow_bytes = hex::decode(&auxpow_hex)
            .map_err(|_| RpcError {
                code: -8,
                message: format!("Invalid auxpow hex: {}", auxpow_hex),
                data: None,
            })?;
        
        // Deserialize auxpow structure
        let auxpow = AuxPow::consensus_decode(&mut auxpow_bytes.as_slice())
            .map_err(|e| RpcError {
                code: -8,
                message: format!("Invalid auxpow structure: {:?}", e),
                data: None,
            })?;

        // Submit to actor for validation and processing
        let result = self.auxpow_actor
            .send(SubmitAuxBlock { hash, auxpow })
            .await
            .map_err(|e| RpcError {
                code: -32603,
                message: "Actor communication failed".to_string(),
                data: Some(serde_json::json!({ "actor_error": e.to_string() })),
            })?;

        match result {
            Ok(_) => {
                info!(
                    block_hash = %hash,
                    "AuxPow submission accepted"
                );
                Ok(true)
            }
            Err(e) => {
                warn!(
                    block_hash = %hash,
                    error = ?e,
                    "AuxPow submission rejected"
                );
                // Bitcoin RPC returns false on failure, not error
                Ok(false)
            }
        }
    }

    /// RPC endpoint: getauxblock
    /// 
    /// Gets the current auxiliary block template (alternative to createauxblock).
    /// Some mining software uses this variant of the create call.
    pub async fn get_aux_block_rpc(&self) -> Result<Option<AuxBlock>, RpcError> {
        debug!("RPC getauxblock called");
        
        // Use zero address as default for template requests
        let aux_block = self.auxpow_actor
            .send(CreateAuxBlock { address: EvmAddress::zero() })
            .await
            .map_err(|e| RpcError {
                code: -32603,
                message: "Actor communication failed".to_string(),
                data: Some(serde_json::json!({ "actor_error": e.to_string() })),
            })?;

        match aux_block {
            Ok(block) => {
                debug!("Generated aux block template");
                Ok(Some(block))
            }
            Err(AuxPowError::ChainSyncing) => {
                debug!("No aux block available - chain syncing");
                Ok(None)
            }
            Err(e) => Err(RpcError::from(e)),
        }
    }

    /// RPC endpoint: getmininginfo
    /// 
    /// Returns current mining information and statistics.
    /// Provides compatibility with Bitcoin's getmininginfo call.
    pub async fn get_mining_info_rpc(&self) -> Result<MiningInfo, RpcError> {
        debug!("RPC getmininginfo called");
        
        let status = self.auxpow_actor
            .send(GetMiningStatus)
            .await
            .map_err(|e| RpcError {
                code: -32603,
                message: "Actor communication failed".to_string(),
                data: Some(serde_json::json!({ "actor_error": e.to_string() })),
            })?;

        let mining_info = MiningInfo {
            mining: status.mining_enabled,
            blocks: status.total_blocks_mined,
            currentblocksize: 0, // Not applicable to auxiliary mining
            currentblocktx: 0,   // Not applicable to auxiliary mining
            difficulty: 1.0,     // Would need difficulty manager integration
            errors: "".to_string(),
            pooledtx: status.current_work_count,
            testnet: false, // Would be determined from chain config
            chain: "alys".to_string(),
            generate: status.mining_enabled,
            genproclimit: 1,
            hashespersec: 0.0, // Would need hash rate calculation
        };

        Ok(mining_info)
    }

    /// RPC endpoint: setgenerate <generate> [genproclimit]
    /// 
    /// Enables or disables mining (generate=true/false).
    /// Compatible with Bitcoin's setgenerate call.
    pub async fn set_generate_rpc(
        &self,
        generate: bool,
        _genproclimit: Option<u32>,
    ) -> Result<bool, RpcError> {
        info!("RPC setgenerate called: generate={}", generate);
        
        use super::messages::SetMiningEnabled;
        
        self.auxpow_actor
            .send(SetMiningEnabled {
                enabled: generate,
                mining_address: None, // Keep current address
            })
            .await
            .map_err(|e| RpcError {
                code: -32603,
                message: "Actor communication failed".to_string(),
                data: Some(serde_json::json!({ "actor_error": e.to_string() })),
            })?
            .map_err(RpcError::from)?;

        Ok(generate)
    }
}

/// Mining information response structure
/// 
/// Compatible with Bitcoin's getmininginfo RPC response format
/// for mining pool and external miner compatibility.
#[derive(Debug, Serialize, Deserialize)]
pub struct MiningInfo {
    /// Whether mining is currently enabled
    pub mining: bool,
    /// Number of blocks mined
    pub blocks: u64,
    /// Size of current block template (not applicable for aux mining)
    pub currentblocksize: u64,
    /// Number of transactions in current block (not applicable for aux mining)
    pub currentblocktx: u64,
    /// Current difficulty
    pub difficulty: f64,
    /// Error messages
    pub errors: String,
    /// Number of transactions in mempool (work queue size)
    pub pooledtx: usize,
    /// Whether this is testnet
    pub testnet: bool,
    /// Chain name
    pub chain: String,
    /// Whether generation is enabled (same as mining)
    pub generate: bool,
    /// Generation processor limit
    pub genproclimit: u32,
    /// Hash rate in hashes per second
    pub hashespersec: f64,
}

/// RPC method registration helper
/// 
/// Registers all AuxPow RPC methods with the RPC server.
/// This provides the standard Bitcoin mining RPC interface.
pub fn register_auxpow_rpc_methods(
    rpc_module: &mut jsonrpsee::RpcModule<AuxPowRpcContext>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Register createauxblock method
    rpc_module.register_async_method("createauxblock", |params, ctx| async move {
        let address = params.one::<String>()?;
        ctx.create_aux_block_rpc(address).await
    })?;

    // Register submitauxblock method  
    rpc_module.register_async_method("submitauxblock", |params, ctx| async move {
        let (hash, auxpow) = params.parse::<(String, String)>()?;
        ctx.submit_aux_block_rpc(hash, auxpow).await
    })?;

    // Register getauxblock method
    rpc_module.register_async_method("getauxblock", |_params, ctx| async move {
        ctx.get_aux_block_rpc().await
    })?;

    // Register getmininginfo method
    rpc_module.register_async_method("getmininginfo", |_params, ctx| async move {
        ctx.get_mining_info_rpc().await
    })?;

    // Register setgenerate method
    rpc_module.register_async_method("setgenerate", |params, ctx| async move {
        let generate = params.one::<bool>()?;
        let genproclimit = params.opt_at::<u32>(1)?;
        ctx.set_generate_rpc(generate, genproclimit).await
    })?;

    info!("Registered AuxPow RPC methods: createauxblock, submitauxblock, getauxblock, getmininginfo, setgenerate");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::System;

    #[actix_rt::test]
    async fn test_parse_mining_address() {
        let valid_address = "0x742d35Cc6634C0532925a3b8D2C7BFcb39db4D8e";
        let parsed = valid_address.parse::<EvmAddress>();
        assert!(parsed.is_ok());
    }

    #[actix_rt::test]
    async fn test_invalid_mining_address() {
        let invalid_address = "invalid_address";
        let parsed = invalid_address.parse::<EvmAddress>();
        assert!(parsed.is_err());
    }

    #[test]
    fn test_rpc_error_conversion() {
        let auxpow_error = AuxPowError::ChainSyncing;
        let rpc_error = RpcError::from(auxpow_error);
        assert_eq!(rpc_error.code, -1);
        assert_eq!(rpc_error.message, "Chain is currently syncing");
    }
}