// use the jsonrpc that is reexported here to save one dependency
use app::{AuxBlock, AuxPow};
use bitcoincore_rpc::bitcoin::consensus::Encodable;
use bitcoincore_rpc::jsonrpc;
use clap::Parser;
use eyre::Error;
use jsonrpc::serde_json;
use jsonrpc::Client;
use rand::Rng;
use serde_json::json;
use std::time::{Duration, Instant};

#[derive(Parser)]
pub struct Args {
    /// Alys RPC endpoint URL
    #[clap(long, default_value = "http://localhost:3000")]
    url: String,

    /// Miner address for block rewards and peg-in fees
    #[clap(long, default_value = "0xb95f80EC665a534b1e309a2a24F8849d27B70FDE")]
    miner_address: String,

    /// Enable synthetic peg-in generation for testing
    #[clap(long)]
    test_pegins: bool,

    /// Interval between test peg-ins in seconds
    #[clap(long, default_value = "30")]
    pegin_interval: u64,

    /// Test peg-in amount in satoshis (default: 0.01 BTC)
    #[clap(long, default_value = "1000000")]
    pegin_amount: u64,

    /// EVM address to receive test peg-ins (random if not specified)
    #[clap(long)]
    pegin_recipient: Option<String>,

    /// Use verbose submitauxblock response format
    #[clap(long)]
    verbose: bool,
}

/// Generate a synthetic peg-in for testing
///
/// Creates fake-but-structurally-valid Bitcoin txid and block_hash.
/// The chain will accept these since we're in test mode and don't
/// verify against real Bitcoin.
fn generate_test_pegin(amount: u64, recipient: &str, block_height: u64) -> serde_json::Value {
    let mut rng = rand::thread_rng();

    // Generate random 32-byte values for txid and block_hash
    let mut txid_bytes = [0u8; 32];
    let mut block_hash_bytes = [0u8; 32];
    rng.fill(&mut txid_bytes);
    rng.fill(&mut block_hash_bytes);

    // Bitcoin hashes are displayed in reverse byte order
    txid_bytes.reverse();
    block_hash_bytes.reverse();

    json!({
        "txid": hex::encode(txid_bytes),
        "block_hash": hex::encode(block_hash_bytes),
        "block_height": block_height,
        "amount": amount,
        "evm_account": recipient
    })
}

/// Generate random EVM address if none specified
fn random_evm_address() -> String {
    let mut rng = rand::thread_rng();
    let mut addr_bytes = [0u8; 20];
    rng.fill(&mut addr_bytes);
    format!("0x{}", hex::encode(addr_bytes))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("Starting miner...");
    println!("  RPC URL: {}", args.url);
    println!("  Miner address: {}", args.miner_address);
    if args.test_pegins {
        println!(
            "  Test peg-ins: enabled (every {}s, {} sats)",
            args.pegin_interval, args.pegin_amount
        );
    }

    let mut last_pegin = Instant::now() - Duration::from_secs(args.pegin_interval);
    let mut pegin_count = 0u64;

    loop {
        match try_mine(&args, &mut last_pegin, &mut pegin_count).await {
            Ok(()) => {}
            Err(err) => {
                println!("Mining error: {err}");
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

async fn try_mine(
    args: &Args,
    last_pegin: &mut Instant,
    pegin_count: &mut u64,
) -> Result<(), Error> {
    let client = Client::simple_http(&args.url, None, None)?;

    // Step 1: Get work from createauxblock
    let aux_block = call::<AuxBlock>(
        &client,
        "createauxblock",
        &[json!(args.miner_address)],
    )?;

    println!(
        "Mining block {} with target {:?}",
        aux_block.height(), aux_block.bits
    );

    // Step 2: Mine AuxPoW
    let auxpow = AuxPow::mine(aux_block.hash, aux_block.bits, aux_block.chain_id).await;

    // Encode for RPC
    let mut encoded_auxpow = Vec::new();
    auxpow.consensus_encode(&mut encoded_auxpow)?;
    let auxpow_hex = hex::encode(&encoded_auxpow);

    let mut encoded_hash = Vec::new();
    aux_block.hash.consensus_encode(&mut encoded_hash)?;
    let hash_hex = hex::encode(&encoded_hash);

    // Step 3: Generate test peg-in if enabled and interval elapsed
    let pegins: Vec<serde_json::Value> = if args.test_pegins
        && last_pegin.elapsed() >= Duration::from_secs(args.pegin_interval)
    {
        *last_pegin = Instant::now();
        *pegin_count += 1;

        let recipient = args
            .pegin_recipient
            .clone()
            .unwrap_or_else(random_evm_address);

        let pegin = generate_test_pegin(args.pegin_amount, &recipient, aux_block.height());

        println!(
            "Including test peg-in #{}: {} sats to {}",
            pegin_count, args.pegin_amount, recipient
        );

        vec![pegin]
    } else {
        vec![]
    };

    // Step 4: Submit with extended parameters
    let params = vec![
        json!(hash_hex),           // param[0]: aggregate hash
        json!(auxpow_hex),         // param[1]: auxpow
        json!(pegins),             // param[2]: peg-ins array
        json!(args.miner_address), // param[3]: fee recipient
        json!(args.verbose),       // param[4]: verbose response
    ];

    let result = call::<serde_json::Value>(&client, "submitauxblock", &params)?;

    if args.verbose {
        println!(
            "submitauxblock: {}",
            serde_json::to_string_pretty(&result)?
        );
    } else {
        println!("submitauxblock: {}", result);
    }

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
