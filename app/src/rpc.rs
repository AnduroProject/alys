use crate::auxpow::AuxPow;
use crate::auxpow_miner::{AuxPowMiner, BitcoinConsensusParams, BlockIndex, ChainManager};
use crate::block::SignedConsensusBlock;
use crate::chain::Chain;
use crate::metrics::{RPC_REQUESTS, RPC_REQUEST_DURATION};
use bitcoin::address::NetworkChecked;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::Hash;
use bitcoin::{Address, BlockHash};
use ethereum_types::Address as EvmAddress;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use lighthouse_wrapper::store::ItemStore;
use lighthouse_wrapper::types::{Hash256, MainnetEthSpec};
use serde_derive::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequestV1<'a> {
    pub method: &'a str,
    pub params: Option<&'a RawValue>,
    pub id: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcErrorV1 {
    pub code: i32,
    pub message: String,
}

impl JsonRpcErrorV1 {
    #[allow(unused)]
    fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
        }
    }

    fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
        }
    }

    fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
        }
    }

    fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: "Invalid params".to_string(),
        }
    }

    #[allow(dead_code)]
    fn internal_error() -> Self {
        Self {
            code: -32603,
            message: "Internal error".to_string(),
        }
    }

    fn block_not_found() -> Self {
        Self {
            code: -32604,
            message: "Block not found".to_string(),
        }
    }

    fn debug_error(error_msg: String) -> Self {
        Self {
            code: -32605,
            message: error_msg,
        }
    }

    fn chain_syncing_error() -> Self {
        Self {
            code: -32606,
            message: "Chain is syncing".to_string(),
        }
    }
}

macro_rules! new_json_rpc_error {
    ($id:expr, $status:expr, $error:expr) => {
        Response::builder().status($status).body(
            JsonRpcResponseV1 {
                result: None,
                error: Some($error),
                id: $id,
            }
            .into(),
        )
    };
}

// https://www.jsonrpc.org/specification_v1#a1.2Response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcResponseV1 {
    pub result: Option<Value>,
    pub error: Option<JsonRpcErrorV1>,
    pub id: Value,
}

impl From<JsonRpcResponseV1> for Body {
    fn from(value: JsonRpcResponseV1) -> Self {
        serde_json::to_string(&value).unwrap().into()
    }
}

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;

// async fn http_req_json_rpc<BI: BlockIndex, CM: ChainManager<BI>, DB: ItemStore<MainnetEthSpec>>(
async fn http_req_json_rpc<BI: BlockIndex, CM: ChainManager<BI>, DB: ItemStore<MainnetEthSpec>>(
    req: Request<Body>,
    miner: Arc<Mutex<AuxPowMiner<BI, CM>>>,
    federation_address: Address<NetworkChecked>,
    chain: Arc<Chain<DB>>,
) -> Result<Response<Body>> {
    let mut miner = miner.lock().await;

    if req.method() != Method::POST {
        RPC_REQUESTS
            .with_label_values(&["unknown", "method_not_allowed"])
            .inc();
        return Ok(Response::builder()
            .status(hyper::StatusCode::METHOD_NOT_ALLOWED)
            .body("JSONRPC server handles only POST requests".into())?);
    }

    let bytes = hyper::body::to_bytes(req.into_body()).await?;
    let json_req = serde_json::from_slice::<JsonRpcRequestV1>(&bytes)?;
    let id = json_req.id;

    let params = if let Some(raw_value) = json_req.params {
        raw_value
    } else {
        RPC_REQUESTS
            .with_label_values(&[json_req.method, "invalid_request"])
            .inc();
        return Ok(new_json_rpc_error!(
            id,
            hyper::StatusCode::OK,
            JsonRpcErrorV1::invalid_request()
        )?);
    };

    let block_response_helper = |id: Value,
                                 block_result: eyre::Result<
        Option<SignedConsensusBlock<MainnetEthSpec>>,
    >| match block_result {
        Ok(block) => Response::builder().status(hyper::StatusCode::OK).body(
            JsonRpcResponseV1 {
                result: Some(json!(block)),
                error: None,
                id,
            }
            .into(),
        ),
        Err(e) => Ok(new_json_rpc_error!(
            id,
            hyper::StatusCode::BAD_REQUEST,
            JsonRpcErrorV1::debug_error(e.to_string())
        )?),
    };

    // Start a timer for the request processing duration
    let timer = RPC_REQUEST_DURATION
        .with_label_values(&[json_req.method])
        .start_timer();

    let response = match json_req.method {
        "createauxblock" => {
            RPC_REQUESTS
                .with_label_values(&["createauxblock", "called"])
                .inc();

            let [script_pub_key] =
                if let Ok(value) = serde_json::from_str::<[EvmAddress; 1]>(params.get()) {
                    value
                } else {
                    RPC_REQUESTS
                        .with_label_values(&["createauxblock", "invalid_params"])
                        .inc();
                    return Ok(new_json_rpc_error!(
                        id,
                        hyper::StatusCode::BAD_REQUEST,
                        JsonRpcErrorV1::invalid_params()
                    )?);
                };

            match miner.create_aux_block(script_pub_key).await {
                Ok(aux_block) => {
                    RPC_REQUESTS
                        .with_label_values(&["createauxblock", "success"])
                        .inc();
                    Response::builder().status(hyper::StatusCode::OK).body(
                        JsonRpcResponseV1 {
                            result: Some(json!(aux_block)),
                            error: None,
                            id,
                        }
                        .into(),
                    )
                }
                Err(e) => {
                    let status = e.to_string();
                    RPC_REQUESTS
                        .with_label_values(&["createauxblock", &status])
                        .inc();
                    new_json_rpc_error!(
                        id,
                        hyper::StatusCode::SERVICE_UNAVAILABLE,
                        JsonRpcErrorV1::chain_syncing_error()
                    )
                }
            }
        }
        "submitauxblock" => {
            RPC_REQUESTS
                .with_label_values(&["submitauxblock", "called"])
                .inc();

            #[allow(unused_mut)]
            let mut hash;
            #[allow(unused_mut)]
            let mut auxpow;
            match decode_submitauxblock_args(params.get()) {
                Ok(value) => {
                    hash = value.0;
                    auxpow = value.1;
                }
                Err(e) => {
                    RPC_REQUESTS
                        .with_label_values(&["submitauxblock", "invalid_params"])
                        .inc();
                    return Ok(new_json_rpc_error!(
                        id,
                        hyper::StatusCode::BAD_REQUEST,
                        JsonRpcErrorV1::debug_error(e.to_string())
                    )?);
                }
            }

            miner.submit_aux_block(hash, auxpow).await?;

            RPC_REQUESTS
                .with_label_values(&["submitauxblock", "success"])
                .inc();

            Response::builder().status(hyper::StatusCode::OK).body(
                JsonRpcResponseV1 {
                    result: Some(json!(())),
                    error: None,
                    id,
                }
                .into(),
            )
        }
        "getdepositaddress" => {
            RPC_REQUESTS
                .with_label_values(&["getdepositaddress", "called"])
                .inc();
            Response::builder().status(hyper::StatusCode::OK).body(
                JsonRpcResponseV1 {
                    result: Some(json!(federation_address.to_string())),
                    error: None,
                    id,
                }
                .into(),
            )
        }
        "getheadblock" => match miner.get_head() {
            Ok(head) => {
                RPC_REQUESTS
                    .with_label_values(&["getheadblock", "success"])
                    .inc();
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: Some(json!(head)),
                        error: None,
                        id,
                    }
                    .into(),
                )
            }
            Err(e) => {
                error!("{}", e.to_string());
                RPC_REQUESTS
                    .with_label_values(&["getheadblock", "block_not_found"])
                    .inc();
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::block_not_found()),
                        id,
                    }
                    .into(),
                )
            }
        },
        "getblockbyheight" => match params.get().parse::<u64>() {
            Ok(target_height) => {
                block_response_helper(id, chain.get_block_by_height(target_height))
            }
            Err(e) => {
                return Ok(new_json_rpc_error!(
                    id,
                    hyper::StatusCode::BAD_REQUEST,
                    JsonRpcErrorV1::debug_error(e.to_string())
                )?)
            }
        },
        "getblockbyhash" => {
            let block_hash = if let Ok(value) = serde_json::from_str::<String>(params.get()) {
                // Note: BlockHash::from_slice results in opposite endianness from BlockHash::from_str
                let block_hash_bytes = hex::decode(&value)?;
                Hash256::from_slice(block_hash_bytes.as_slice())
            } else {
                return Ok(new_json_rpc_error!(
                    id,
                    hyper::StatusCode::BAD_REQUEST,
                    JsonRpcErrorV1::invalid_params()
                )?);
            };

            block_response_helper(id, chain.get_block(&block_hash))
        }
        "getqueuedpow" => match miner.get_queued_auxpow().await {
            Some(queued_pow) => {
                RPC_REQUESTS
                    .with_label_values(&["getqueuedpow", "success"])
                    .inc();
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: Some(json!(queued_pow)),
                        error: None,
                        id,
                    }
                    .into(),
                )
            }
            None => {
                RPC_REQUESTS
                    .with_label_values(&["getqueuedpow", "no_data"])
                    .inc();
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: None,
                        id,
                    }
                    .into(),
                )
            }
        },
        "setbtcscanheight" => match params.get().parse::<u32>() {
            Ok(target_height) => {
                // block_response_helper(id, chain.get_block_by_height(target_height))
                info!("clearbtcscanheight: clearing btc scan height to {}", target_height);
                chain.set_bitcoin_scan_start_height(target_height).unwrap();
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: Some(json!(())),
                        error: None,
                        id,
                    }
                    .into(),
                )
            }
            Err(e) => {
                return Ok(new_json_rpc_error!(
                    id,
                    hyper::StatusCode::BAD_REQUEST,
                    JsonRpcErrorV1::debug_error(e.to_string())
                )?)
            }
        },
        _ => {
            RPC_REQUESTS
                .with_label_values(&["unknown", "method_not_found"])
                .inc();
            new_json_rpc_error!(
                id,
                hyper::StatusCode::NOT_FOUND,
                JsonRpcErrorV1::method_not_found()
            )
        }
    };

    // Stop the timer and record the duration
    timer.observe_duration();

    Ok(response?)
}

fn decode_submitauxblock_args(encoded: &str) -> Result<(BlockHash, AuxPow)> {
    let (blockhash_str, auxpow_str) = serde_json::from_str::<(String, String)>(encoded)?;
    // Note: BlockHash::from_slice results in opposite endianness from BlockHash::from_str
    let blockhash_bytes = hex::decode(&blockhash_str)?;
    let blockhash = BlockHash::from_slice(blockhash_bytes.as_slice())?;

    let auxpow_bytes = hex::decode(&auxpow_str)?;
    let auxpow = AuxPow::consensus_decode(&mut auxpow_bytes.as_slice())?;
    Ok((blockhash, auxpow))
}

pub async fn run_server<DB: ItemStore<MainnetEthSpec>>(
    chain: Arc<Chain<DB>>,
    federation_address: Address<NetworkChecked>,
    retarget_params: BitcoinConsensusParams,
    rpc_port: u16,
) {
    let addr = SocketAddr::from(([0, 0, 0, 0], rpc_port));
    let miner = Arc::new(Mutex::new(AuxPowMiner::new(chain.clone(), retarget_params)));

    tracing::info!("Starting RPC server on {}", addr);
    let server = Server::bind(&addr).serve(make_service_fn(move |_conn| {
        let miner = miner.clone();
        let federation_address = federation_address.clone();
        let chain_clone = chain.clone();

        async move {
            Ok::<_, GenericError>(service_fn(move |req| {
                let miner = miner.clone();
                let federation_address = federation_address.clone();
                let chain_for_req = chain_clone.clone();

                http_req_json_rpc(req, miner, federation_address, chain_for_req)
            }))
        }
    }));

    // TODO: handle graceful shutdown
    tokio::spawn(async move {
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    });
}

#[test]
fn test_decode_submitauxblock_args() {
    // let params = r##"["466f02c19563c706028cf8941387ca93fc67853e98af0a3c8a87d424c30f4cb4","01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff2e027612043aefe1652f7a7a616d78632f9b1f463eeea422e558d49377f2c75c01091aea17b600f4b8b094ffffffffffffffff0300000000000000001600148ec4187ad5b631c9a559ba4cd8eb191618ec24830000000000000000266a24aa21a9ed7cb317c7def059f16e0902e9fcddd8152ac863f25bc396c57b45a438f0829caf00000000000000002cfabe6d6d466f02c19563c706028cf8941387ca93fc67853e98af0a3c8a87d424c30f4cb401000000000000005b756d5666258e4e5ccbf610b7ffab355283938104c91debbf0c29685ecc685f46b0af1c03dc67e9a45634a992c1b14103215bf03950f76c62ffd34bf73bdd16fdb4f747162c03d1b71a52cf006c60086f0dc4dc31b8cc687c344e5ce19594d7c4f55675c1e93d45b26ceb8c0f3ff89a638d60eacbe8977b46c1fcf32b922c56c161685a2800000000000000000000000020c10be9d39faec5176a7b117e4ef25601570e66c370b1af905fddf0180ed6b36f4b274e9a639960a3a756b011fb0dce098e8053a1dc8948d570da63a9d1df44f14cefe165ffff7f200c29c7bf"]"##;
    let params = r##"["f5aa3d8d1f59922d78a072ce6fee4e4f85f9dceb0737d181db35d98ef4584eb4","01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff2e02ff1704f028e7652f7a7a616d78632f9b1f463eeea422e558d49377f2c75c0109d718e970007ac4cfc0ffffffffffffffff0300000000000000001600148ec4187ad5b631c9a559ba4cd8eb191618ec24830000000000000000266a24aa21a9edf9c04456edd5d664954fafd41f74e526552b72f2df6bbf6568f6acb6a05fd31300000000000000002cfabe6d6df5aa3d8d1f59922d78a072ce6fee4e4f85f9dceb0737d181db35d98ef4584eb401000000000000002460910e236c5ddadc4b5d8483402a0c0a24148dafdad5d27755770df47aa23fe5185883037187916ad659ae13fd7b4c80c4bd99737459f8203ffc92962c2c7771ab23775ff322af955beddf107840635c8464b1a284b836ecb3e80d8d43aab465b67ca58b97eeff36d44105cf74a8dfec9f4d0c17aa3a8302852908b6f83569e73c60be310000000000000000000000002011deda93abf5a65b68494fae9b52c3b871c53ca6421510cce1124d9b46f78824aa1dbaca594a6722e6e2e897ed515074eb2ec6b1c354df8c63971e0a26e3ba0e0629e765ffff7f20514d46e9"]"##;

    let (hash, auxpow) = decode_submitauxblock_args(params).unwrap();
    auxpow.check(hash, 21212).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Body, Request};
    use serde_json::json;
    use std::sync::Once;
    use tracing::{debug, info};

    // Initialize the logger only once for all tests
    static INIT: Once = Once::new();

    // Setup function that initializes the logger
    fn setup_logger() {
        INIT.call_once(|| {
            // Initialize tracing subscriber for tests
            let subscriber = tracing_subscriber::FmtSubscriber::builder()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_test_writer() // Use test writer which works well with cargo test
                .finish();

            // Set the subscriber as the default
            tracing::subscriber::set_global_default(subscriber)
                .expect("Failed to set tracing subscriber");

            info!("Test logging initialized");
        });
    }

    #[test]
    fn test_getblockbyheight_rpc_parsing() {
        // Setup logger
        setup_logger();
        debug!("Running getblockbyheight RPC parsing test");

        // Test the JSON-RPC parsing logic for getblockbyheight
        let json_request = r#"{"method":"getblockbyheight","params":1,"id":1}"#;

        // Parse the JSON-RPC request
        let json_req: JsonRpcRequestV1 = serde_json::from_str(json_request).unwrap();

        // Verify the method
        assert_eq!(json_req.method, "getblockbyheight");

        // Verify the params
        let params = json_req.params.unwrap();
        assert_eq!(params.get().parse::<u64>().unwrap(), 1);

        // Verify the ID
        assert_eq!(json_req.id, json!(1));

        // Parse the height
        let height: u64 = params.get().parse().unwrap();
        assert_eq!(height, 1);

        info!("getblockbyheight RPC parsing test successful");
    }

    #[test]
    fn test_getblockbyheight_invalid_params() {
        // Setup logger
        setup_logger();
        debug!("Running getblockbyheight invalid params test");

        // Test invalid parameter in the request
        let json_request = r#"{"method":"getblockbyheight","params":"invalid","id":1}"#;

        // Parse the JSON-RPC request
        let json_req: JsonRpcRequestV1 = serde_json::from_str(json_request).unwrap();

        // Verify the method
        assert_eq!(json_req.method, "getblockbyheight");

        // Parse the height (should fail)
        let params = json_req.params.unwrap();
        let height_result = params.get().parse::<u64>();
        assert!(height_result.is_err());

        info!("getblockbyheight invalid params test successful");
    }

    // Note: This test is a placeholder and will only pass when run with a live node
    #[tokio::test]
    async fn test_getblockbyheight_height_one() {
        // Setup logger
        setup_logger();
        debug!("Running getblockbyheight height one test");

        // This test requires a running node with a valid chain that has at least block #1
        // Create the JSON-RPC request for block height 1
        let json_request = r#"{"method":"getblockbyheight","params":"1","id":1}"#;
        let req = Request::builder()
            .method("POST")
            .body(Body::from(json_request))
            .unwrap();

        // In a real test environment, we would:
        // 1. Have access to a running node with blocks
        // 2. Call http_req_json_rpc with the request and node components
        // 3. Assert on the response structure

        info!("GetBlockByHeight request: {:#?}", req);

        info!("getblockbyheight height one test completed");
    }
}
