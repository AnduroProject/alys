//! Request-Response Protocol V2
//!
//! Simplified request-response implementation for NetworkActor V2.
//! TCP transport only, essential requests for block sync coordination.

use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};

/// Request types for V2 system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestV2 {
    /// Request blocks by height range
    GetBlocks {
        start_height: u64,
        count: u32,
    },
    /// Request current chain status
    GetChainStatus,
    /// Request peer information
    GetPeers,
}

/// Response types for V2 system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseV2 {
    /// Block data response
    Blocks {
        blocks: Vec<BlockData>,
        start_height: u64,
    },
    /// Chain status response
    ChainStatus {
        current_height: u64,
        best_hash: String,
    },
    /// Peer information response
    Peers {
        peers: Vec<PeerData>,
    },
    /// Error response
    Error {
        message: String,
    },
}

/// Simplified block data for network transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData {
    pub height: u64,
    pub hash: String,
    pub parent_hash: String,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

/// Peer data for network transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerData {
    pub peer_id: String,
    pub address: String,
    pub reputation: f64,
    pub last_seen: u64,
}

/// Request-response protocol handler
pub struct RequestResponseHandler {
    /// Maximum request size in bytes
    max_request_size: usize,
    /// Maximum response size in bytes
    max_response_size: usize,
    /// Request timeout in seconds
    request_timeout_secs: u64,
}

impl RequestResponseHandler {
    pub fn new() -> Self {
        Self {
            max_request_size: 1024 * 1024,     // 1MB
            max_response_size: 50 * 1024 * 1024, // 50MB for block responses
            request_timeout_secs: 30,
        }
    }

    /// Serialize request for network transmission
    pub fn serialize_request(&self, request: &RequestV2) -> Result<Vec<u8>> {
        let data = serde_json::to_vec(request)
            .map_err(|e| anyhow!("Failed to serialize request: {}", e))?;

        if data.len() > self.max_request_size {
            return Err(anyhow!("Request too large: {} bytes", data.len()));
        }

        Ok(data)
    }

    /// Deserialize request from network data
    pub fn deserialize_request(&self, data: &[u8]) -> Result<RequestV2> {
        if data.len() > self.max_request_size {
            return Err(anyhow!("Request too large: {} bytes", data.len()));
        }

        serde_json::from_slice(data)
            .map_err(|e| anyhow!("Failed to deserialize request: {}", e))
    }

    /// Serialize response for network transmission
    pub fn serialize_response(&self, response: &ResponseV2) -> Result<Vec<u8>> {
        let data = serde_json::to_vec(response)
            .map_err(|e| anyhow!("Failed to serialize response: {}", e))?;

        if data.len() > self.max_response_size {
            return Err(anyhow!("Response too large: {} bytes", data.len()));
        }

        Ok(data)
    }

    /// Deserialize response from network data
    pub fn deserialize_response(&self, data: &[u8]) -> Result<ResponseV2> {
        if data.len() > self.max_response_size {
            return Err(anyhow!("Response too large: {} bytes", data.len()));
        }

        serde_json::from_slice(data)
            .map_err(|e| anyhow!("Failed to deserialize response: {}", e))
    }

    /// Validate request
    pub fn validate_request(&self, request: &RequestV2) -> Result<()> {
        match request {
            RequestV2::GetBlocks { start_height: _, count } => {
                if *count == 0 {
                    return Err(anyhow!("Block count must be greater than 0"));
                }
                if *count > 1000 {
                    return Err(anyhow!("Block count too large: {}", count));
                }
            }
            RequestV2::GetChainStatus | RequestV2::GetPeers => {
                // Always valid
            }
        }
        Ok(())
    }

    /// Create error response
    pub fn error_response(message: String) -> ResponseV2 {
        ResponseV2::Error { message }
    }

    /// Get request timeout
    pub fn get_timeout_secs(&self) -> u64 {
        self.request_timeout_secs
    }
}

impl Default for RequestResponseHandler {
    fn default() -> Self {
        Self::new()
    }
}