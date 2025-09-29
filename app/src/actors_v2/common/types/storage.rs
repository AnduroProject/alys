//! Common storage types shared across V2 actors
//!
//! This module defines shared storage-related types.

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