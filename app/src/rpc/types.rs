//! Shared RPC types and structures

use hyper::Body;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

/// JSON-RPC V1 request structure
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub method: String,
    pub params: Option<Value>,
    pub id: Value,
}

/// JSON-RPC V1 response structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcResponse {
    pub result: Option<Value>,
    pub error: Option<RpcError>,
    pub id: Value,
}

impl From<JsonRpcResponse> for Body {
    fn from(value: JsonRpcResponse) -> Self {
        serde_json::to_string(&value).unwrap().into()
    }
}

/// RPC error structure compatible with JSON-RPC V1
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: "Invalid params".to_string(),
            data: None,
        }
    }

    pub fn internal_error() -> Self {
        Self {
            code: -32603,
            message: "Internal error".to_string(),
            data: None,
        }
    }

    pub fn block_not_found() -> Self {
        Self {
            code: -32604,
            message: "Block not found".to_string(),
            data: None,
        }
    }

    pub fn debug_error(error_msg: String) -> Self {
        Self {
            code: -32605,
            message: error_msg,
            data: None,
        }
    }

    pub fn service_unavailable(service: &str) -> Self {
        Self {
            code: -32606,
            message: format!("{} service unavailable", service),
            data: None,
        }
    }

    pub fn mining_disabled() -> Self {
        Self {
            code: -32607,
            message: "Mining is disabled".to_string(),
            data: None,
        }
    }

    pub fn chain_syncing() -> Self {
        Self {
            code: -32608,
            message: "Chain is syncing".to_string(),
            data: None,
        }
    }
}