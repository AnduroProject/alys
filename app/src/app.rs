#![allow(clippy::manual_div_ceil)]

use crate::aura::{Aura, AuraSlotWorker};
use crate::auxpow_miner::spawn_background_miner;
use crate::block_hash_cache::BlockHashCacheInit;
use crate::chain::{BitcoinWallet, Chain};
use crate::engine::*;
use crate::spec::{
    genesis_value_parser, hex_file_parser, ChainSpec, DEV_BITCOIN_SECRET_KEY, DEV_SECRET_KEY,
};
use crate::store::{Storage, DEFAULT_ROOT_DIR};
use bridge::{
    bitcoin::Network, BitcoinCore, BitcoinSecretKey, BitcoinSignatureCollector, BitcoinSigner,
    Bridge, Federation,
};
use clap::builder::ArgPredicate;
use clap::Parser;
use eyre::Result;
use futures::pin_mut;
use lighthouse_wrapper::bls::{Keypair, SecretKey};
use lighthouse_wrapper::execution_layer::auth::JwtKey;
use std::str::FromStr;
use std::time::Duration;
use std::{future::Future, sync::Arc};
use tracing::*;
use tracing_subscriber::{prelude::*, EnvFilter};

#[inline]
pub fn run() -> Result<()> {
    App::parse().run()
}

pub fn parse_secret_key(s: &str) -> Result<SecretKey, eyre::Error> {
    let secret_key = SecretKey::deserialize(&hex::decode(s)?[..])
        .map_err(|_err| eyre::Error::msg("Failed to deserialize key"))?;
    Ok(secret_key)
}

pub fn parse_bitcoin_secret_key(
    s: &str,
) -> Result<bitcoin::key::secp256k1::SecretKey, eyre::Error> {
    let secret_key = bitcoin::key::secp256k1::SecretKey::from_str(s)
        .map_err(|_err| eyre::Error::msg("Failed to deserialize key"))?;
    Ok(secret_key)
}

#[derive(Parser)]
#[command(author, about = "ALYS", long_about = None)]
pub struct App {
    #[arg(
        long = "chain",
        value_name = "CHAIN_OR_PATH",
        value_parser = genesis_value_parser,
        default_value_if("dev", ArgPredicate::IsPresent, Some("dev")),
        required_unless_present = "dev"
    )]
    chain_spec: Option<ChainSpec>,

    #[arg(
        long = "aura-secret-key",
        value_parser = parse_secret_key,
        default_value_if("dev", ArgPredicate::IsPresent, Some(DEV_SECRET_KEY)),
    )]
    pub aura_secret_key: Option<SecretKey>,

    #[arg(
        long = "bitcoin-secret-key",
        value_parser = parse_bitcoin_secret_key,
        default_value_if("dev", ArgPredicate::IsPresent, Some(DEV_BITCOIN_SECRET_KEY))
    )]
    pub bitcoin_secret_key: Option<BitcoinSecretKey>,

    #[arg(long)]
    pub wallet_path: Option<String>,

    #[arg(long = "geth-url")]
    pub geth_url: Option<String>,

    #[arg(long = "geth-execution-url")]
    pub geth_execution_url: Option<String>,

    #[arg(long = "db-path")]
    pub db_path: Option<String>,

    /// Flag to enable mining
    #[arg(long = "mine")]
    pub mine: bool,

    /// Flag to disable mining regardless of the `--dev` flags
    #[arg(long = "no-mine", default_value_t = false)]
    pub no_mine: bool,

    #[arg(long = "not-validator", default_value_t = false)]
    pub not_validator: bool,

    #[arg(
        long = "full-log-context",
        env = "FULL_LOG_CONTEXT",
        default_value_t = false
    )]
    pub full_log_context: bool,

    #[arg(long, default_value_t = 3000)]
    pub rpc_port: u16,

    #[arg(long, default_value_t = 0)]
    pub p2p_port: u16,

    #[arg(long, default_value = "0.0.0.0")]
    pub p2p_listen_addr: String,

    #[arg(long)]
    pub remote_bootnode: Option<String>,

    #[arg(long)]
    pub dev: bool,

    #[clap(
        long,
        env = "BITCOIN_RPC_URL",
        default_value_if("dev", ArgPredicate::IsPresent, Some("http://0.0.0.0:18443")),
        // required_unless_present = "dev"
    )]
    pub bitcoin_rpc_url: Option<String>,

    #[clap(
        long,
        env = "BITCOIN_RPC_USER",
        default_value_if("dev", ArgPredicate::IsPresent, Some("rpcuser")),
        // required_unless_present = "dev"
    )]
    pub bitcoin_rpc_user: Option<String>,

    #[clap(
        long,
        env = "BITCOIN_RPC_PASS",
        default_value_if("dev", ArgPredicate::IsPresent, Some("rpcpassword")),
        // required_unless_present = "dev"
    )]
    pub bitcoin_rpc_pass: Option<String>,

    #[clap(long, default_value("regtest"))]
    pub bitcoin_network: Network,

    #[clap(long, required = true, value_parser = hex_file_parser)]
    pub jwt_secret: [u8; 32],

    #[clap(long, help = "Port for the metrics server")]
    pub metrics_port: Option<u16>,
}

impl App {
    pub fn run(self) -> Result<()> {
        self.init_tracing();
        let tokio_runtime = tokio_runtime()?;
        tokio_runtime.block_on(run_until_ctrl_c(self.execute()))?;
        Ok(())
    }

    fn init_tracing(&self) {
        let rust_log_level = Level::from_str(
            std::env::var("RUST_LOG")
                .unwrap_or("info".to_string())
                .as_str(),
        )
        .unwrap();

        let filter = if self.full_log_context {
            EnvFilter::builder().parse_lossy(rust_log_level.as_str())
        } else {
            let filter_tag =
                format!("app={rust_log_level},federation={rust_log_level},miner={rust_log_level}");
            EnvFilter::builder().parse_lossy(filter_tag.as_str())
        };

        let main_layer = tracing_subscriber::fmt::layer().with_target(true);

        let layers = if rust_log_level == Level::DEBUG || rust_log_level == Level::TRACE {
            vec![main_layer
                .with_file(true)
                .with_line_number(true)
                .with_filter(filter)
                .boxed()]
        } else {
            vec![main_layer.with_filter(filter).boxed()]
        };

        tracing_subscriber::registry().with(layers).init();
    }

    async fn execute(self) -> Result<()> {
        let disk_store = Storage::new_disk(self.db_path);

        info!("Head: {:?}", disk_store.get_head());
        info!("Finalized: {:?}", disk_store.get_latest_pow_block());

        // TODO: Combine instantiation of engine & execution apis into Engine::new
        let http_engine_json_rpc =
            new_http_engine_json_rpc(self.geth_url, JwtKey::from_slice(&self.jwt_secret).unwrap());
        let public_execution_json_rpc = new_http_public_execution_json_rpc(self.geth_execution_url);
        let engine = Engine::new(http_engine_json_rpc, public_execution_json_rpc);

        let network = crate::network::spawn_network_handler(
            self.p2p_listen_addr,
            self.p2p_port,
            self.remote_bootnode,
        )
        .await?;

        let chain_spec = self.chain_spec.expect("Chain spec is configured");
        let authorities = chain_spec.authorities.clone();
        let slot_duration = chain_spec.slot_duration;
        let bitcoin_start_height = disk_store
            .get_bitcoin_scan_start_height()
            .unwrap()
            .unwrap_or(chain_spec.bitcoin_start_height);

        let mut bitcoin_addresses = Vec::new();

        fn calculate_threshold(federation_bitcoin_pubkeys_len: usize) -> usize {
            ((federation_bitcoin_pubkeys_len * 2) + 2) / 3
        }

        let threshold = calculate_threshold(chain_spec.federation_bitcoin_pubkeys.len()); // 2rds majority, rounded up
        let bitcoin_federation = Federation::new(
            chain_spec.federation_bitcoin_pubkeys.clone(),
            threshold,
            self.bitcoin_network,
        );
        info!(
            "Using bitcoin deposit address {}",
            bitcoin_federation.taproot_address
        );

        bitcoin_addresses.push(bitcoin_federation.taproot_address.clone());

        let wallet_path = self
            .wallet_path
            .unwrap_or(format!("{DEFAULT_ROOT_DIR}/wallet"));
        let bitcoin_wallet = BitcoinWallet::new(&wallet_path, bitcoin_federation.clone())?;
        let bitcoin_signature_collector =
            BitcoinSignatureCollector::new(bitcoin_federation.clone());

        let (maybe_aura_signer, maybe_bitcoin_signer);
        if chain_spec.is_validator && !self.not_validator {
            (maybe_aura_signer, maybe_bitcoin_signer) =
                match (self.aura_secret_key, self.bitcoin_secret_key) {
                    (Some(aura_sk), Some(bitcoin_sk)) => {
                        let aura_pk = aura_sk.public_key();
                        info!("Using aura public key {aura_pk}");
                        let aura_signer = Keypair::from_components(aura_pk, aura_sk);

                        let bitcoin_pk = bitcoin_sk.public_key(&bitcoin::key::Secp256k1::new());
                        info!("Using bitcoin public key {bitcoin_pk}");
                        let bitcoin_signer = BitcoinSigner::new(bitcoin_sk);

                        info!("Running authority");
                        (Some(aura_signer), Some(bitcoin_signer))
                    }
                    (None, Some(_)) => panic!("Aura secret not configured"),
                    (Some(_), None) => panic!("Bitcoin secret not configured"),
                    (None, None) => {
                        info!("Running full node");
                        (None, None)
                    }
                };
        } else {
            (maybe_aura_signer, maybe_bitcoin_signer) = (None, None);
        }

        let aura = Aura::new(
            authorities.clone(),
            slot_duration,
            maybe_aura_signer.clone(),
        );

        // TODO: We probably just want to persist the chain_spec struct
        let chain = Arc::new(Chain::new(
            engine,
            network,
            disk_store,
            aura,
            chain_spec.max_blocks_without_pow,
            chain_spec.federation.clone(),
            Bridge::new(
                BitcoinCore::new(
                    &self.bitcoin_rpc_url.expect("RPC URL is configured"),
                    self.bitcoin_rpc_user.expect("RPC user is configured"),
                    self.bitcoin_rpc_pass.expect("RPC password is configured"),
                ),
                bitcoin_addresses,
                chain_spec.required_btc_txn_confirmations,
            ),
            bitcoin_wallet,
            bitcoin_signature_collector,
            maybe_bitcoin_signer,
            chain_spec.retarget_params.clone(),
            chain_spec.is_validator && !self.not_validator,
        ));

        // import genesis block without signatures or verification
        chain
            .store_genesis(chain_spec.clone())
            .await
            .expect("Should store genesis");

        // Initialize the block hash cache
        chain.init_block_hash_cache().await?;

        // start json-rpc v1 server
        crate::rpc::run_server(
            chain.clone(),
            bitcoin_federation.taproot_address,
            chain_spec.retarget_params,
            self.rpc_port,
        )
        .await;

        crate::metrics::start_server(self.metrics_port).await;

        if (self.mine || self.dev) && !self.no_mine {
            info!("Spawning miner");
            spawn_background_miner(chain.clone());
        }

        chain.clone().monitor_gossip().await;
        chain.clone().listen_for_peer_discovery().await;
        chain.clone().listen_for_rpc_requests().await;

        info!("Triggering initial sync...");
        let chain_clone = chain.clone();
        tokio::spawn(async move {
            chain_clone.sync().await;
        });

        if chain_spec.is_validator && !self.not_validator {
            chain
                .clone()
                .monitor_bitcoin_blocks(bitcoin_start_height)
                .await;
        }

        AuraSlotWorker::new(
            Duration::from_millis(slot_duration),
            authorities,
            maybe_aura_signer,
            chain,
        )
        .start_slot_worker()
        .await;

        Ok(())
    }
}

// async code taken from reth, when we add more complexity we should adopt
// the task manager logic to handle thread spawning and graceful shutdown
pub fn tokio_runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

async fn run_until_ctrl_c<F, E>(fut: F) -> Result<(), E>
where
    F: Future<Output = Result<(), E>>,
    E: Send + Sync + 'static + From<std::io::Error>,
{
    let ctrl_c = tokio::signal::ctrl_c();

    let mut stream = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let sigterm = stream.recv();
    pin_mut!(sigterm, ctrl_c, fut);

    tokio::select! {
        _ = ctrl_c => {
            info!("Received ctrl-c");
        },
        _ = sigterm => {
            info!("Received SIGTERM");
        },
        res = fut => res?,
    }

    Ok(())
}
