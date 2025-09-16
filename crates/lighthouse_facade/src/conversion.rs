//! Type conversion utilities for Lighthouse facade
//!
//! This module provides simple type conversion utilities between different representations.

use crate::{
    error::{FacadeError, FacadeResult},
    types::*,
};
use ethereum_types::{Address, H256, U256};

pub mod v7_to_v4;
pub mod responses;

/// Convert ExecutionBlockHash to H256 for compatibility
pub fn execution_block_hash_to_h256(hash: &ExecutionBlockHash) -> H256 {
    #[cfg(any(feature = "v4", feature = "v7"))]
    {
        // For real Lighthouse types, ExecutionBlockHash wraps Hash256
        H256::from_slice(&hash.into_root().0[..])
    }
    
    #[cfg(not(any(feature = "v4", feature = "v7")))]
    {
        // For mock types, ExecutionBlockHash is already H256
        *hash
    }
}

/// Convert H256 to ExecutionBlockHash for compatibility
pub fn h256_to_execution_block_hash(hash: H256) -> ExecutionBlockHash {
    #[cfg(any(feature = "v4", feature = "v7"))]
    {
        // For real Lighthouse types, convert from H256 via Hash256
        use crate::types::Hash256;
        use lighthouse_v7_types::Hash256 as LighthouseHash256;
        ExecutionBlockHash::from_root(LighthouseHash256::from_slice(hash.as_bytes()))
    }
    
    #[cfg(not(any(feature = "v4", feature = "v7")))]
    {
        // For mock types, ExecutionBlockHash is already H256
        hash
    }
}

/// Convert Lighthouse Address to ethereum_types::Address
pub fn lighthouse_address_to_address(addr: &LighthouseAddress) -> Address {
    #[cfg(any(feature = "v4", feature = "v7"))]
    {
        // For real Lighthouse types, convert to Address
        Address::from_slice(&addr.0[..])
    }
    
    #[cfg(not(any(feature = "v4", feature = "v7")))]
    {
        // For mock types, LighthouseAddress is already Address
        *addr
    }
}

/// Convert ethereum_types::Address to Lighthouse Address
pub fn address_to_lighthouse_address(addr: Address) -> LighthouseAddress {
    #[cfg(any(feature = "v4", feature = "v7"))]
    {
        // For real Lighthouse types, convert from Address bytes
        LighthouseAddress::from(<[u8; 20]>::try_from(addr.as_bytes()).unwrap())
    }
    
    #[cfg(not(any(feature = "v4", feature = "v7")))]
    {
        // For mock types, LighthouseAddress is already Address
        addr
    }
}