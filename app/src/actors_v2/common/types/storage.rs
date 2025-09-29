//! Common storage types shared across V2 actors
//!
//! This module defines shared storage-related types.

use crate::actors_v2::storage::messages::{
    StoreBlockMessage,
    GetBlockMessage,
    GetBlockByHeightMessage,
    BlockExistsMessage,
    UpdateStateMessage,
    GetStateMessage,
    GetChainHeadMessage,
};

#[derive(Debug)]
pub enum StorageMessage {
    StoreBlock(StoreBlockMessage),
    GetBlock(GetBlockMessage),
    GetBlockByHeight(GetBlockByHeightMessage),
    BlockExists(BlockExistsMessage),
    UpdateState(UpdateStateMessage),
    GetState(GetStateMessage),
    GetChainHead(GetChainHeadMessage),
    // Add more message types as needed
}