//! Request-Response protocol for block synchronization
//!
//! Protocol: /alys/block/1.0.0
//! Encoding: SSZ (Simple Serialize)
//!
//! Phase 1 Task 1.5: Protocol type definitions with SSZ serialization
//! Phase 2 Task 2.2: Full codec implementation (deferred)

use anyhow::Result;
use futures::prelude::*;
use libp2p::request_response::Codec;
use libp2p::StreamProtocol;
use ssz::{Decode, Encode};
use ssz_derive::{Decode as DecodeDeriv, Encode as EncodeDeriv};
use std::io;

/// Block request-response protocol identifier
#[derive(Debug, Clone)]
pub struct BlockProtocol();

impl BlockProtocol {
    /// Get the protocol name as a stream protocol
    pub fn protocol() -> StreamProtocol {
        StreamProtocol::new("/alys/block/1.0.0")
    }

    /// Get the protocol name as bytes
    pub fn protocol_name(&self) -> &[u8] {
        b"/alys/block/1.0.0"
    }
}

/// Block request message types
///
/// SSZ serialization enables efficient encoding for network transmission.
#[derive(Debug, Clone, PartialEq, Eq, EncodeDeriv, DecodeDeriv)]
#[ssz(enum_behaviour = "union")]
pub enum BlockRequest {
    /// Request blocks by height range (start_height, count)
    GetBlocks(BlockRangeRequest),
    /// Request current chain status
    GetChainStatus(EmptyRequest),
}

/// Request for a range of blocks
#[derive(Debug, Clone, PartialEq, Eq, EncodeDeriv, DecodeDeriv)]
pub struct BlockRangeRequest {
    pub start_height: u64,
    pub count: u32,
}

/// Empty request marker
#[derive(Debug, Clone, PartialEq, Eq, EncodeDeriv, DecodeDeriv)]
pub struct EmptyRequest;

/// Block response message types
///
/// SSZ serialization for consistent encoding across the network.
#[derive(Debug, Clone, EncodeDeriv, DecodeDeriv)]
#[ssz(enum_behaviour = "union")]
pub enum BlockResponse {
    /// Block data response
    Blocks(BlocksResponse),
    /// Chain status response
    ChainStatus(ChainStatusResponse),
    /// Error response
    Error(ErrorResponse),
}

/// Response containing multiple blocks
#[derive(Debug, Clone, EncodeDeriv, DecodeDeriv)]
pub struct BlocksResponse {
    pub blocks: Vec<BlockData>,
}

/// Response with chain status information
#[derive(Debug, Clone, EncodeDeriv, DecodeDeriv)]
pub struct ChainStatusResponse {
    pub height: u64,
    pub head_hash: [u8; 32],
}

/// Error response with message
#[derive(Debug, Clone, EncodeDeriv, DecodeDeriv)]
pub struct ErrorResponse {
    /// Error message as UTF-8 bytes (SSZ-compatible)
    pub message: Vec<u8>,
}

/// Simplified block data for network transmission
///
/// Contains essential block metadata and transaction data.
/// Full block reconstruction happens after receiving this data.
#[derive(Debug, Clone, PartialEq, Eq, EncodeDeriv, DecodeDeriv)]
pub struct BlockData {
    /// Block height in the chain
    pub height: u64,
    /// Block hash (32 bytes)
    pub hash: [u8; 32],
    /// Parent block hash (32 bytes)
    pub parent_hash: [u8; 32],
    /// Unix timestamp (seconds since epoch)
    pub timestamp: u64,
    /// Raw transaction data (SSZ-encoded transactions)
    pub transactions: Vec<Vec<u8>>,
}

/// Codec for BlockProtocol with size limits
///
/// Phase 1 Task 1.5: Structure definition
/// Phase 2 Task 2.2: Full RequestResponseCodec trait implementation
#[derive(Debug, Clone)]
pub struct BlockCodec {
    /// Maximum request message size (1 MB)
    max_request_size: usize,
    /// Maximum response message size (10 MB for multiple blocks)
    max_response_size: usize,
}

impl BlockCodec {
    /// Create new codec with default size limits
    pub fn new() -> Self {
        Self {
            max_request_size: 1024 * 1024,       // 1 MB
            max_response_size: 10 * 1024 * 1024, // 10 MB
        }
    }

    /// Create codec with custom size limits
    pub fn with_limits(max_request_size: usize, max_response_size: usize) -> Self {
        Self {
            max_request_size,
            max_response_size,
        }
    }

    /// Get maximum request size
    pub fn max_request_size(&self) -> usize {
        self.max_request_size
    }

    /// Get maximum response size
    pub fn max_response_size(&self) -> usize {
        self.max_response_size
    }
}

impl Default for BlockCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement libp2p RequestResponseCodec for BlockCodec
#[async_trait::async_trait]
impl Codec for BlockCodec {
    type Protocol = &'static str;
    type Request = BlockRequest;
    type Response = BlockResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read length prefix (4 bytes, big-endian)
        let mut len_bytes = [0u8; 4];
        io.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Validate size
        if len > self.max_request_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Request too large: {} bytes (max: {})",
                    len, self.max_request_size
                ),
            ));
        }

        // Read SSZ-encoded request
        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;

        // Decode SSZ
        BlockRequest::from_ssz_bytes(&buf).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("SSZ decode error: {:?}", e),
            )
        })
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read length prefix (4 bytes, big-endian)
        let mut len_bytes = [0u8; 4];
        io.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Validate size
        if len > self.max_response_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Response too large: {} bytes (max: {})",
                    len, self.max_response_size
                ),
            ));
        }

        // Read SSZ-encoded response
        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;

        // Decode SSZ
        BlockResponse::from_ssz_bytes(&buf).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("SSZ decode error: {:?}", e),
            )
        })
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Encode to SSZ
        let encoded = req.as_ssz_bytes();

        // Validate size
        if encoded.len() > self.max_request_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Request too large: {} bytes", encoded.len()),
            ));
        }

        // Write length prefix (4 bytes, big-endian)
        let len = (encoded.len() as u32).to_be_bytes();
        io.write_all(&len).await?;

        // Write SSZ-encoded request
        io.write_all(&encoded).await?;

        // Flush to ensure data is sent
        io.flush().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Encode to SSZ
        let encoded = res.as_ssz_bytes();

        // Validate size
        if encoded.len() > self.max_response_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Response too large: {} bytes", encoded.len()),
            ));
        }

        // Write length prefix (4 bytes, big-endian)
        let len = (encoded.len() as u32).to_be_bytes();
        io.write_all(&len).await?;

        // Write SSZ-encoded response
        io.write_all(&encoded).await?;

        // Flush to ensure data is sent
        io.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_protocol_name() {
        let protocol = BlockProtocol();
        assert_eq!(protocol.protocol_name(), b"/alys/block/1.0.0");
    }

    #[test]
    fn test_block_request_get_blocks_ssz_roundtrip() {
        let request = BlockRequest::GetBlocks(BlockRangeRequest {
            start_height: 100,
            count: 50,
        });

        let encoded = request.as_ssz_bytes();
        let decoded = BlockRequest::from_ssz_bytes(&encoded).unwrap();

        assert_eq!(request, decoded);
    }

    #[test]
    fn test_block_request_get_chain_status_ssz_roundtrip() {
        let request = BlockRequest::GetChainStatus(EmptyRequest);

        let encoded = request.as_ssz_bytes();
        let decoded = BlockRequest::from_ssz_bytes(&encoded).unwrap();

        assert_eq!(request, decoded);
    }

    #[test]
    fn test_block_response_blocks_ssz_roundtrip() {
        let block_data = BlockData {
            height: 42,
            hash: [0u8; 32],
            parent_hash: [1u8; 32],
            timestamp: 1234567890,
            transactions: vec![vec![0xaa, 0xbb], vec![0xcc, 0xdd]],
        };

        let response = BlockResponse::Blocks(BlocksResponse {
            blocks: vec![block_data],
        });

        let encoded = response.as_ssz_bytes();
        let decoded = BlockResponse::from_ssz_bytes(&encoded).unwrap();

        // Compare the encoded/decoded values
        match (response, decoded) {
            (BlockResponse::Blocks(orig), BlockResponse::Blocks(dec)) => {
                assert_eq!(orig.blocks.len(), dec.blocks.len());
                assert_eq!(orig.blocks[0].height, dec.blocks[0].height);
                assert_eq!(orig.blocks[0].hash, dec.blocks[0].hash);
                assert_eq!(orig.blocks[0].parent_hash, dec.blocks[0].parent_hash);
                assert_eq!(orig.blocks[0].timestamp, dec.blocks[0].timestamp);
                assert_eq!(orig.blocks[0].transactions, dec.blocks[0].transactions);
            }
            _ => panic!("Decoded response type mismatch"),
        }
    }

    #[test]
    fn test_block_response_chain_status_ssz_roundtrip() {
        let response = BlockResponse::ChainStatus(ChainStatusResponse {
            height: 1000,
            head_hash: [0x42; 32],
        });

        let encoded = response.as_ssz_bytes();
        let decoded = BlockResponse::from_ssz_bytes(&encoded).unwrap();

        match (response, decoded) {
            (BlockResponse::ChainStatus(orig), BlockResponse::ChainStatus(dec)) => {
                assert_eq!(orig.height, dec.height);
                assert_eq!(orig.head_hash, dec.head_hash);
            }
            _ => panic!("Decoded response type mismatch"),
        }
    }

    #[test]
    fn test_block_response_error_ssz_roundtrip() {
        let response = BlockResponse::Error(ErrorResponse {
            message: b"Block not found".to_vec(),
        });

        let encoded = response.as_ssz_bytes();
        let decoded = BlockResponse::from_ssz_bytes(&encoded).unwrap();

        match (response, decoded) {
            (BlockResponse::Error(orig), BlockResponse::Error(dec)) => {
                assert_eq!(orig.message, dec.message);
            }
            _ => panic!("Decoded response type mismatch"),
        }
    }

    #[test]
    fn test_block_data_ssz_roundtrip() {
        let block_data = BlockData {
            height: 12345,
            hash: [0xaa; 32],
            parent_hash: [0xbb; 32],
            timestamp: 9876543210,
            transactions: vec![vec![0x01, 0x02, 0x03], vec![0x04, 0x05], vec![]],
        };

        let encoded = block_data.as_ssz_bytes();
        let decoded = BlockData::from_ssz_bytes(&encoded).unwrap();

        assert_eq!(block_data, decoded);
    }

    #[test]
    fn test_block_codec_defaults() {
        let codec = BlockCodec::new();
        assert_eq!(codec.max_request_size(), 1024 * 1024);
        assert_eq!(codec.max_response_size(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_block_codec_custom_limits() {
        let codec = BlockCodec::with_limits(512 * 1024, 5 * 1024 * 1024);
        assert_eq!(codec.max_request_size(), 512 * 1024);
        assert_eq!(codec.max_response_size(), 5 * 1024 * 1024);
    }

    #[test]
    fn test_block_codec_default_trait() {
        let codec = BlockCodec::default();
        assert_eq!(codec.max_request_size(), 1024 * 1024);
        assert_eq!(codec.max_response_size(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_ssz_encoding_size_efficiency() {
        // Verify SSZ encoding is reasonably compact
        let request = BlockRequest::GetBlocks(BlockRangeRequest {
            start_height: 100,
            count: 50,
        });

        let encoded = request.as_ssz_bytes();

        // SSZ should encode this efficiently (enum tag + two integers)
        // Should be reasonably compact
        assert!(encoded.len() < 100, "SSZ encoding should be compact");
    }

    #[test]
    fn test_multiple_blocks_encoding() {
        // Test encoding multiple blocks efficiently
        let blocks: Vec<BlockData> = (0..10)
            .map(|i| BlockData {
                height: i,
                hash: [i as u8; 32],
                parent_hash: [(i.wrapping_sub(1)) as u8; 32],
                timestamp: 1000000 + i,
                transactions: vec![],
            })
            .collect();

        let response = BlockResponse::Blocks(BlocksResponse { blocks });
        let encoded = response.as_ssz_bytes();

        // Decode and verify
        let decoded = BlockResponse::from_ssz_bytes(&encoded).unwrap();

        match decoded {
            BlockResponse::Blocks(response) => {
                assert_eq!(response.blocks.len(), 10);
                for (i, block) in response.blocks.iter().enumerate() {
                    assert_eq!(block.height, i as u64);
                }
            }
            _ => panic!("Expected Blocks response"),
        }
    }
}
