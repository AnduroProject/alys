// use the jsonrpc that is reexported here to save one dependency
use app::{AuxBlock, AuxPow};
use bitcoincore_rpc::bitcoin::consensus::Encodable;
use bitcoincore_rpc::jsonrpc;
use clap::Parser;
use eyre::Error;
use jsonrpc::serde_json;
use jsonrpc::Client;
use serde_json::json;
use std::time::Duration;

#[derive(Parser)]
pub struct Args {
    #[clap(long, default_value("http://localhost:3000"))]
    url: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    loop {
        if let Err(err) = try_mine(&args).await {
            println!("Failed to mine: {err}");
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

async fn try_mine(args: &Args) -> Result<(), Error> {
    let client = Client::simple_http(&args.url, None, None)?;
    let aux_block = call::<AuxBlock>(
        &client,
        "createauxblock",
        &[serde_json::Value::String(
            "0x0000000000000000000000000000000000000000".to_string(),
        )],
    )?;

    println!("Mining with target = {:?}", aux_block.bits);
    let auxpow = AuxPow::mine(aux_block.hash, aux_block.bits, aux_block.chain_id).await;

    let mut encoded_auxpow = Vec::new();
    auxpow.consensus_encode(&mut encoded_auxpow)?;
    let stringified_auxpow = hex::encode(encoded_auxpow);

    let mut encoded_auxpow_hash = Vec::new();
    aux_block.hash.consensus_encode(&mut encoded_auxpow_hash)?;
    let stringified_aux_hash = hex::encode(encoded_auxpow_hash);

    let result = call::<bool>(
        &client,
        "submitauxblock",
        &[json!(stringified_aux_hash), json!(stringified_auxpow)],
    )?;

    println!("submitauxblock result: {result}");
    Ok(())
}

fn call<T: for<'a> serde::de::Deserialize<'a>>(
    client: &Client,
    cmd: &str,
    args: &[serde_json::Value],
) -> Result<T, Error> {
    let raw_args: Vec<_> = args
        .iter()
        .map(|a| {
            let json_string = serde_json::to_string(a)?;
            serde_json::value::RawValue::from_string(json_string) // we can't use to_raw_value here due to compat with Rust 1.29
        })
        .collect::<Result<Vec<_>, _>>()?;
    let req = client.build_request(cmd, &raw_args);

    let resp = client.send_request(req)?;

    Ok(resp.result::<T>()?)
}
