use bitcoincore_rpc::Auth;
pub use bitcoincore_rpc::{
    bitcoin::Block,
    jsonrpc::{error::RpcError, Error as JsonRpcError},
    Client, Error as BitcoinError, RpcApi,
};
use futures::prelude::*;
use num_derive::FromPrimitive;
use std::sync::Arc;
use tracing::*;

pub use bitcoincore_rpc::bitcoin;
use std::time::Duration;

const RETRY_DURATION: Duration = Duration::from_secs(1);

// https://github.com/bitcoin/bitcoin/blob/be3af4f31089726267ce2dbdd6c9c153bb5aeae1/src/rpc/protocol.h#L43
#[derive(Debug, FromPrimitive, PartialEq, Eq)]
pub enum BitcoinRpcError {
    /// Standard JSON-RPC 2.0 errors
    RpcInvalidRequest = -32600,
    RpcMethodNotFound = -32601,
    RpcInvalidParams = -32602,
    RpcInternalError = -32603,
    RpcParseError = -32700,

    /// General application defined errors
    RpcMiscError = -1,
    RpcTypeError = -3,
    RpcInvalidAddressOrKey = -5,
    RpcOutOfMemory = -7,
    RpcInvalidParameter = -8,
    RpcDatabaseError = -20,
    RpcDeserializationErrr = -22,
    RpcVerifyError = -25,
    RpcVerifyRejected = -26,
    RpcVerifyAlreadyInChain = -27,
    RpcInWarmup = -28,
    RpcMethodDeprecated = -32,

    /// Aliases for backward compatibility
    // RpcTransactionError           = RpcVerifyError,
    // RpcTransactionRejected        = RpcVerifyRejected,
    // RpcTransactionAlreadyInChain  = RpcVerifyAlreadyInChain,

    /// P2P client errors
    RpcClientNotConnected = -9,
    RpcClientInInitialDownload = -10,
    RpcClientNodeAlreadyAdded = -23,
    RpcClientNodeNotAdded = -24,
    RpcClientNodeNotConnected = -29,
    RpcClientInvalidIpOrSubnet = -30,
    RpcClientP2PDisabled = -31,

    /// Chain errors
    RpcClientMempoolDisabled = -33,

    /// Wallet errors
    RpcWalletError = -4,
    RpcWalletInsufficientFunds = -6,
    RpcWalletInvalidLabelName = -11,
    RpcWalletKeypoolRanOut = -12,
    RpcWalletUnlockNeeded = -13,
    RpcWalletPassphraseIncorrect = -14,
    RpcWalletWrongEncState = -15,
    RpcWalletEncryptionFailed = -16,
    RpcWalletAlreadyUnlocked = -17,
    RpcWalletNotFound = -18,
    RpcWalletNotSpecified = -19,

    /// Backwards compatible aliases
    // RpcWalletInvalidAccountName = RpcWalletInvalidLabelName,

    /// Unused reserved codes.
    RpcForbiddenBySafeMode = -2,

    /// Unknown error code (not in spec).
    RpcUnknownError = 0,
}

impl From<RpcError> for BitcoinRpcError {
    fn from(err: RpcError) -> Self {
        match num::FromPrimitive::from_i32(err.code) {
            Some(err) => err,
            None => Self::RpcUnknownError,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    BitcoinRpcError,
}
impl From<bitcoincore_rpc::Error> for Error {
    fn from(_value: bitcoincore_rpc::Error) -> Self {
        Self::BitcoinRpcError
    }
}

#[derive(Clone)]
pub struct BitcoinCore {
    pub rpc: Arc<Client>,
}

impl BitcoinCore {
    pub fn new(url: &str, rpc_user: impl Into<String>, rpc_pass: impl Into<String>) -> Self {
        Self {
            rpc: Client::new(url, Auth::UserPass(rpc_user.into(), rpc_pass.into()))
                .unwrap()
                .into(),
        }
    }

    /// Wait for a specified height to return a `BlockHash` or
    /// exit on error.
    ///
    /// # Arguments
    /// * `height` - block height to fetch
    /// * `num_confirmations` - minimum for a block to be accepted
    async fn wait_for_block(&self, height: u32, num_confirmations: u32) -> Result<Block, Error> {
        loop {
            match self.rpc.get_block_hash(height.into()) {
                Ok(hash) => {
                    let info = self.rpc.get_block_info(&hash)?;
                    if info.confirmations >= num_confirmations as i32 {
                        return Ok(self.rpc.get_block(&hash)?);
                    } else {
                        tokio::time::sleep(RETRY_DURATION).await;
                        continue;
                    }
                }
                Err(BitcoinError::JsonRpc(JsonRpcError::Rpc(err)))
                    if BitcoinRpcError::from(err.clone())
                        == BitcoinRpcError::RpcInvalidParameter =>
                {
                    // block does not exist yet
                    tokio::time::sleep(RETRY_DURATION).await;
                    continue;
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }
    }
}

/// Stream blocks continuously `from_height` awaiting the production of
/// new blocks as reported by Bitcoin core. The stream never ends.
///
/// # Arguments:
///
/// * `rpc` - bitcoin rpc
/// * `from_height` - height of the first block of the stream
/// * `num_confirmations` - minimum for a block to be accepted
pub async fn stream_blocks(
    rpc: BitcoinCore,
    from_height: u32,
    num_confirmations: u32,
) -> impl Stream<Item = Result<(Block, u32), Error>> + Unpin {
    struct StreamState<B> {
        rpc: B,
        next_height: u32,
    }

    let state = StreamState {
        rpc,
        next_height: from_height,
    };

    Box::pin(
        stream::unfold(state, move |mut state| async move {
            // FIXME: if Bitcoin Core forks, this may skip a block
            let height = state.next_height;
            match state.rpc.wait_for_block(height, num_confirmations).await {
                Ok(block) => {
                    debug!("found block {} at height {}", block.block_hash(), height);
                    state.next_height += 1;
                    Some((Ok((block, height)), state))
                }
                Err(e) => Some((Err(e), state)),
            }
        })
        .fuse(),
    )
}
