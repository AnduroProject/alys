use crate::aura::{Aura, AuraSlotWorker};
use crate::auxpow_miner::spawn_background_miner;
use crate::chain::{BitcoinWallet, Chain};
use crate::engine::*;
use crate::spec::{genesis_value_parser, ChainSpec, DEV_BITCOIN_SECRET_KEY, DEV_SECRET_KEY};
use crate::store::{Storage, DEFAULT_ROOT_DIR};
use bls::{Keypair, SecretKey};
use bridge::{
    bitcoin::Network, BitcoinCore, BitcoinSecretKey, BitcoinSignatureCollector, BitcoinSigner,
    Bridge, Federation,
};
use clap::builder::ArgPredicate;
use clap::Parser;
use futures::pin_mut;
use std::str::FromStr;
use std::time::Duration;
use std::{future::Future, sync::Arc};
use tracing::log::Level::{Debug, Trace};
use tracing::*;
use tracing_subscriber::{prelude::*, EnvFilter};
use types::chain_spec;

#[inline]
pub fn run() -> eyre::Result<()> {
    App::parse().run()
}

pub fn parse_secret_key(s: &str) -> eyre::Result<SecretKey, eyre::Error> {
    let secret_key = SecretKey::deserialize(&hex::decode(s)?[..])
        .map_err(|_err| eyre::Error::msg("Failed to deserialize key"))?;
    Ok(secret_key)
}

pub fn parse_bitcoin_secret_key(
    s: &str,
) -> eyre::Result<bitcoin::key::secp256k1::SecretKey, eyre::Error> {
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

    #[arg(long = "mine")]
    pub mine: bool,

    #[arg(long, default_value_t = 3000)]
    pub rpc_port: u16,

    #[arg(long, default_value_t = 0)]
    pub p2p_port: u16,

    #[arg(long)]
    pub remote_bootnode: Option<String>,

    #[arg(long)]
    pub dev: bool,

    #[clap(
        long,
        env = "BITCOIN_RPC_URL",
        default_value_if("dev", ArgPredicate::IsPresent, Some("http://0.0.0.0:18443")),
        required_unless_present = "dev"
    )]
    pub bitcoin_rpc_url: Option<String>,

    #[clap(
        long,
        env = "BITCOIN_RPC_USER",
        default_value_if("dev", ArgPredicate::IsPresent, Some("rpcuser")),
        required_unless_present = "dev"
    )]
    pub bitcoin_rpc_user: Option<String>,

    #[clap(
        long,
        env = "BITCOIN_RPC_PASS",
        default_value_if("dev", ArgPredicate::IsPresent, Some("rpcpassword")),
        required_unless_present = "dev"
    )]
    pub bitcoin_rpc_pass: Option<String>,

    #[clap(long, default_value("regtest"))]
    pub bitcoin_network: Network,
}

impl App {
    pub fn run(self) -> eyre::Result<()> {
        self.init_tracing();
        let tokio_runtime = tokio_runtime()?;
        tokio_runtime.block_on(run_until_ctrl_c(self.execute()))?;
        Ok(())
    }

    fn init_tracing(&self) {
        // let rust_log_level = Level::from_str(
        //     std::env::var("RUST_LOG")
        //         .unwrap_or("info".to_string())
        //         .as_str(),
        // )
        // .unwrap();

        let filter = EnvFilter::builder().from_env_lossy();

        let main_layer = tracing_subscriber::fmt::layer().with_target(true);

        // let layers = if rust_log_level == Level::DEBUG || rust_log_level == Level::TRACE {
        //     vec![main_layer
        //         .with_file(true)
        //         .with_line_number(true)
        //         .with_filter(filter)
        //         .boxed()]
        // } else {
        //     vec![main_layer.with_filter(filter).boxed()]
        // };

        tracing_subscriber::registry()
            .with(vec![main_layer.with_filter(filter).boxed()])
            .init();
    }

    async fn execute(self) -> eyre::Result<()> {
        let disk_store = Storage::new_disk(self.db_path);

        info!("Head: {:?}", disk_store.get_head());
        info!("Finalized: {:?}", disk_store.get_latest_pow_block());

        let http_engine_json_rpc = new_http_engine_json_rpc(self.geth_url);
        let public_execution_json_rpc = new_http_public_execution_json_rpc(self.geth_execution_url);
        let engine = Engine::new(http_engine_json_rpc, public_execution_json_rpc);

        let network =
            crate::network::spawn_network_handler(self.p2p_port, self.remote_bootnode).await?;

        let chain_spec = self.chain_spec.expect("Chain spec is configured");
        let authorities = chain_spec.authorities.clone();
        let slot_duration = chain_spec.slot_duration;
        let bitcoin_start_height = disk_store
            .get_bitcoin_scan_start_height()
            .unwrap()
            .unwrap_or(chain_spec.bitcoin_start_height);

        let threshold = ((chain_spec.federation_bitcoin_pubkeys.len() * 2) + 2) / 3; // 2rds majority, rounded up
        let bitcoin_federation = Federation::new(
            chain_spec.federation_bitcoin_pubkeys.clone(),
            threshold,
            self.bitcoin_network,
        );
        info!(
            "Using bitcoin deposit address {}",
            bitcoin_federation.taproot_address
        );

        let wallet_path = self
            .wallet_path
            .unwrap_or(format!("{DEFAULT_ROOT_DIR}/wallet"));
        let bitcoin_wallet = BitcoinWallet::new(&wallet_path, bitcoin_federation.clone())?;
        let bitcoin_signature_collector =
            BitcoinSignatureCollector::new(bitcoin_federation.clone());

        let (maybe_aura_signer, maybe_bitcoin_signer);
        if chain_spec.is_validator {
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
                bitcoin_federation.taproot_address.clone(),
            ),
            bitcoin_wallet,
            bitcoin_signature_collector,
            maybe_bitcoin_signer,
            chain_spec.retarget_params.clone(),
        ));

        // import genesis block without signatures or verification
        chain
            .store_genesis(chain_spec.clone())
            .await
            .expect("Should store genesis");

        // start json-rpc v1 server
        crate::rpc::run_server(
            chain.clone(),
            bitcoin_federation.taproot_address,
            chain_spec.retarget_params,
            self.rpc_port,
        )
        .await;

        if self.mine || self.dev {
            info!("Spawning miner");
            spawn_background_miner(chain.clone());
        }

        chain.clone().monitor_gossip().await;
        chain.clone().listen_for_peer_discovery().await;
        chain.clone().listen_for_rpc_requests().await;

        chain
            .clone()
            .monitor_bitcoin_blocks(bitcoin_start_height)
            .await;

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
