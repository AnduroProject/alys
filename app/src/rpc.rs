use crate::auxpow::AuxPow;
use crate::auxpow_miner::{AuxPowMiner, BitcoinConsensusParams, BlockIndex, ChainManager};
use crate::chain::Chain;
use crate::error::Error;
use bitcoin::address::NetworkChecked;
use bitcoin::consensus::Decodable;
use bitcoin::hashes::Hash;
use bitcoin::{Address, BlockHash, CompactTarget};
use ethereum_types::Address as EvmAddress;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::RawValue;
use std::net::SocketAddr;
use std::sync::Arc;
use store::ItemStore;
use tokio::sync::Mutex;
use types::MainnetEthSpec;

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequestV1<'a> {
    pub method: &'a str,
    pub params: Option<&'a RawValue>,
    pub id: serde_json::Value,
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
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcErrorV1>,
    pub id: serde_json::Value,
}

impl From<JsonRpcResponseV1> for Body {
    fn from(value: JsonRpcResponseV1) -> Self {
        serde_json::to_string(&value).unwrap().into()
    }
}

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;

// async fn http_req_json_rpc<BI: BlockIndex, CM: ChainManager<BI>, DB: ItemStore<MainnetEthSpec>>(
async fn http_req_json_rpc<BI: BlockIndex, CM: ChainManager<BI>>(
    req: Request<Body>,
    miner: Arc<Mutex<AuxPowMiner<BI, CM>>>,
    federation_address: Address<NetworkChecked>,
    // chain: Arc<Chain<DB>>,
) -> Result<Response<Body>> {
    let mut miner = miner.lock().await;

    if req.method() != Method::POST {
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
        return Ok(new_json_rpc_error!(
            id,
            hyper::StatusCode::OK,
            JsonRpcErrorV1::invalid_request()
        )?);
    };

    Ok(match json_req.method {
        "createauxblock" => {
            let [script_pub_key] =
                if let Ok(value) = serde_json::from_str::<[EvmAddress; 1]>(params.get()) {
                    value
                } else {
                    return Ok(new_json_rpc_error!(
                        id,
                        hyper::StatusCode::BAD_REQUEST,
                        JsonRpcErrorV1::invalid_params()
                    )?);
                };

            if let Some(aux_block) = miner.create_aux_block(script_pub_key).await {
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: Some(json!(aux_block)),
                        error: None,
                        id,
                    }
                    .into(),
                )
            } else {
                new_json_rpc_error!(
                    id,
                    hyper::StatusCode::NOT_FOUND,
                    JsonRpcErrorV1::internal_error()
                )
            }
        }
        "submitauxblock" => {
            // let (hash, auxpow) = if let Ok(value) = decode_submitauxblock_args(params.get()) {
            let mut hash;
            let mut auxpow;
            match decode_submitauxblock_args(params.get()) {
                Ok(value) => {
                    hash = value.0;
                    auxpow = value.1;
                }
                Err(e) => {
                    return Ok(new_json_rpc_error!(
                        id,
                        hyper::StatusCode::BAD_REQUEST,
                        JsonRpcErrorV1::debug_error(e.to_string())
                    )?)
                }
            }
            //     value
            // } else {
            //     return Ok(new_json_rpc_error!(
            //         id,
            //         hyper::StatusCode::BAD_REQUEST,
            //         JsonRpcErrorV1::debug_error()
            //     )?);
            // };

            let value = miner.submit_aux_block(hash, auxpow).await;

            Response::builder().status(hyper::StatusCode::OK).body(
                JsonRpcResponseV1 {
                    result: Some(json!(value)),
                    error: None,
                    id,
                }
                .into(),
            )
        }
        "getdepositaddress" => Response::builder().status(hyper::StatusCode::OK).body(
            JsonRpcResponseV1 {
                result: Some(json!(federation_address.to_string())),
                error: None,
                id,
            }
            .into(),
        ),
        "adjust_target" => {
            let target_override_cons_rep = serde_json::from_str::<u32>(params.get());
            if let Ok(target) = target_override_cons_rep {
                miner.set_target_override(CompactTarget::from_consensus(target));

                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: None,
                        id,
                    }
                    .into(),
                )
            } else {
                Response::builder().status(hyper::StatusCode::OK).body(
                    JsonRpcResponseV1 {
                        result: None,
                        error: Some(JsonRpcErrorV1::debug_error("Invalid target".to_string())),
                        id,
                    }
                    .into(),
                )
            }
        }
        // "getaccumulatedfees" => {
        //     let args = params.get();
        //     let block_hash;
        //     // if args.is_empty() {
        //     block_hash = None;
        //     // } else {
        //     //     block_hash = Some(args.into())
        //     // };
        //
        //     Response::builder().status(hyper::StatusCode::OK).body(
        //         match chain.get_accumulated_fees(block_hash).await {
        //             Ok(value) => JsonRpcResponseV1 {
        //                 result: Some(json!(value)),
        //                 error: None,
        //                 id,
        //             }
        //             .into(),
        //             Err(_e) => JsonRpcResponseV1 {
        //                 result: None,
        //                 error: Some(JsonRpcErrorV1::block_not_found()),
        //                 id,
        //             }
        //             .into(),
        //         },
        //     )
        // }
        _ => new_json_rpc_error!(
            id,
            hyper::StatusCode::NOT_FOUND,
            JsonRpcErrorV1::method_not_found()
        ),
    }?)
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
        // let chain = chain.clone();

        async move {
            Ok::<_, GenericError>(service_fn(move |req| {
                let miner = miner.clone();
                let federation_address = federation_address.clone();

                http_req_json_rpc(req, miner, federation_address)
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
