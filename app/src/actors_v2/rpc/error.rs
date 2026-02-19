use actix::MailboxError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// RPC error types
#[derive(Debug)]
pub enum RpcError {
    /// Invalid request format
    InvalidRequest(String),

    /// Method not found
    MethodNotFound(String),

    /// Invalid parameters
    InvalidParams(String),

    /// Internal error
    Internal(String),

    /// Chain actor error
    ChainError(crate::actors_v2::chain::ChainError),

    /// Actor mailbox error
    MailboxError(String),

    /// Server not running
    ServerNotRunning,

    /// Deprecated method (Phase 4: Document 12)
    Deprecated {
        method: String,
        replacement: Option<String>,
        message: String,
    },
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
            RpcError::MethodNotFound(method) => write!(f, "Method not found: {}", method),
            RpcError::InvalidParams(msg) => write!(f, "Invalid parameters: {}", msg),
            RpcError::Internal(msg) => write!(f, "Internal error: {}", msg),
            RpcError::ChainError(err) => write!(f, "Chain error: {:?}", err),
            RpcError::MailboxError(msg) => write!(f, "Mailbox error: {}", msg),
            RpcError::ServerNotRunning => write!(f, "RPC server not running"),
            RpcError::Deprecated { method, replacement, message } => {
                if let Some(repl) = replacement {
                    write!(f, "Method '{}' is deprecated. Use '{}' instead. {}", method, repl, message)
                } else {
                    write!(f, "Method '{}' is deprecated. {}", method, message)
                }
            }
        }
    }
}

impl std::error::Error for RpcError {}

impl From<MailboxError> for RpcError {
    fn from(err: MailboxError) -> Self {
        RpcError::MailboxError(err.to_string())
    }
}

impl From<crate::actors_v2::chain::ChainError> for RpcError {
    fn from(err: crate::actors_v2::chain::ChainError) -> Self {
        RpcError::ChainError(err)
    }
}

/// JSON-RPC 1.0 error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    /// Convert to JSON-RPC error code (Bitcoin-compatible)
    pub fn to_json_rpc_error(&self) -> JsonRpcError {
        match self {
            RpcError::InvalidRequest(_) => JsonRpcError {
                code: -32600,
                message: self.to_string(),
            },
            RpcError::MethodNotFound(_) => JsonRpcError {
                code: -32601,
                message: self.to_string(),
            },
            RpcError::InvalidParams(_) => JsonRpcError {
                code: -32602,
                message: self.to_string(),
            },
            RpcError::Internal(_) | RpcError::ChainError(_) | RpcError::MailboxError(_) => {
                JsonRpcError {
                    code: -32603,
                    message: self.to_string(),
                }
            }
            RpcError::ServerNotRunning => JsonRpcError {
                code: -32000,
                message: "RPC server not running".to_string(),
            },
            // Deprecated methods use code -32000 (server error) per Document 12
            RpcError::Deprecated { .. } => JsonRpcError {
                code: -32000,
                message: self.to_string(),
            },
        }
    }
}
