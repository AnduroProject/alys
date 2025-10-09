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
use tokio::task::LocalSet;

// V2 RPC imports
use actix::Actor;
use crate::actors_v2::rpc::{RpcActor, RpcConfig, StartRpcServer};

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
        // Clone values needed for V2 actor system BEFORE V0 takes ownership
        let v2_db_path = self.db_path.clone();
        let v2_geth_url = self.geth_url.clone();
        let v2_geth_execution_url = self.geth_execution_url.clone();
        let v2_jwt_secret = self.jwt_secret;
        let v2_p2p_listen_addr = self.p2p_listen_addr.clone();
        let v2_p2p_port = self.p2p_port;
        let v2_remote_bootnode = self.remote_bootnode.clone();

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
            .clone()
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

        // Clone values for V2 RPC before V0 Chain takes ownership
        let v2_bitcoin_rpc_url = self.bitcoin_rpc_url.clone();
        let v2_bitcoin_rpc_user = self.bitcoin_rpc_user.clone();
        let v2_bitcoin_rpc_pass = self.bitcoin_rpc_pass.clone();
        let v2_bitcoin_addresses = bitcoin_addresses.clone();
        let v2_bitcoin_federation = bitcoin_federation.clone();
        let v2_authorities = authorities.clone();
        let v2_maybe_aura_signer = maybe_aura_signer.clone();
        let v2_maybe_bitcoin_sk = self.bitcoin_secret_key;
        let v2_retarget_params = chain_spec.retarget_params.clone();
        let v2_not_validator = self.not_validator;
        let v2_is_validator = chain_spec.is_validator;
        let v2_federation = chain_spec.federation.clone();
        let v2_max_blocks_without_pow = chain_spec.max_blocks_without_pow;
        let v2_required_confirmations = chain_spec.required_btc_txn_confirmations;
        let v2_slot_duration = slot_duration;
        let v2_wallet_path = format!("{DEFAULT_ROOT_DIR}/wallet_v2"); // wallet_path.clone();

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

        // start json-rpc v0 server
        crate::rpc::run_server(
            chain.clone(),
            bitcoin_federation.taproot_address,
            chain_spec.retarget_params,
            self.rpc_port,
        )
        .await;

        // Start V2 JSON-RPC server on port 3001
        info!("Starting V2 RPC server on port 3001 (sharing state with V0 Chain)...");

        // Spawn V2 RPC initialization in LocalSet context (required for Actix actors)
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move {
                let local = tokio::task::LocalSet::new();
                local.run_until(async move {
                    info!("🚀 Starting V2 Actor System initialization...");

                    // Clone values for slot worker before Aura consumes them
                    let v2_authorities_for_slot_worker = v2_authorities.clone();
                    let v2_maybe_aura_signer_for_slot_worker = v2_maybe_aura_signer.clone();

                    // Create V2 Aura (separate instance for V2 consensus)
                    let v2_aura = Aura::new(v2_authorities, v2_slot_duration, v2_maybe_aura_signer);

            // STATE SHARING STRATEGY:
            // V0 Chain owns Bridge/Wallet directly (not Arc-wrapped)
            // V2 ChainState expects Arc<RwLock<>> wrappers for async access
            //
            // Current approach: Create separate instances but share filesystem state
            // - Bridge: Separate instances, synced via Bitcoin blockchain state
            // - Wallet: SAME wallet file (disk-level sharing)
            // - SignatureCollector: Separate instances (stateless, deterministic)
            //
            // TODO: Future optimization - wrap V0 Chain's components in Arc<RwLock<>>
            // to enable true in-memory state sharing (requires V0 Chain refactor)

            // Create V2 Bridge (separate instance, eventually consistent via Bitcoin)
            let shared_bridge = Bridge::new(
                BitcoinCore::new(&v2_bitcoin_rpc_url.expect("RPC URL"),
                               v2_bitcoin_rpc_user.expect("RPC user"),
                               v2_bitcoin_rpc_pass.expect("RPC pass")),
                v2_bitcoin_addresses,
                v2_required_confirmations,
            );

            // Create V2 Wallet using SAME filesystem path as V0 (disk-level sharing)
            let shared_wallet = BitcoinWallet::new(
                &v2_wallet_path,  // SAME path as V0 - disk-level state sharing
                v2_bitcoin_federation.clone(),
            )
            .expect("V2 wallet creation");

            let shared_sig_collector = BitcoinSignatureCollector::new(v2_bitcoin_federation);
            let shared_signer = v2_maybe_bitcoin_sk.map(BitcoinSigner::new);

            let v2_state = crate::actors_v2::chain::state::ChainState::new(
                v2_aura,
                v2_federation.clone(),
                shared_bridge,
                shared_wallet,
                shared_sig_collector,
                shared_signer,
                v2_retarget_params,
                v2_is_validator && !v2_not_validator,
                v2_max_blocks_without_pow,
                None,
            );

            let v2_config = crate::actors_v2::chain::ChainConfig {
                is_validator: v2_is_validator && !v2_not_validator,
                validator_address: None,
                federation: v2_federation,
                max_blocks_without_pow: v2_max_blocks_without_pow,
                block_production_timeout: Duration::from_secs(30),
                block_validation_timeout: Duration::from_secs(10),
                enable_auxpow: true,
                enable_peg_operations: true,
                retarget_params: Some(crate::actors_v2::chain::config::BitcoinConsensusParams {
                    target_spacing: Duration::from_secs(600),
                    target_timespan: Duration::from_secs(1209600),
                    retarget_interval: 2016,
                    max_target: 0x1d00ffff,
                }),
                block_hash_cache_size: Some(1000),
                chain_id: 1337,
            };

            // 1. Initialize StorageActor V2
            info!("📦 Initializing StorageActor V2...");
            let storage_config = crate::actors_v2::storage::StorageConfig {
                database: crate::actors_v2::storage::database::DatabaseConfig {
                    main_path: v2_db_path.unwrap_or_else(|| format!("{}/v2", crate::store::DEFAULT_ROOT_DIR)),
                    archive_path: None,
                    cache_size_mb: 256,
                    write_buffer_size_mb: 64,
                    max_open_files: 1000,
                    compression_enabled: true,
                },
                cache: crate::actors_v2::storage::cache::CacheConfig {
                    max_blocks: 1000,
                    max_state_entries: 10000,
                    max_receipts: 5000,
                    state_ttl: Duration::from_secs(300),
                    receipt_ttl: Duration::from_secs(300),
                    enable_warming: false,
                },
                write_batch_size: 100,
                sync_interval: Duration::from_millis(100),
                maintenance_interval: Duration::from_secs(300),
                enable_auto_compaction: true,
                metrics_reporting_interval: Duration::from_secs(60),
            };
            let storage_actor = crate::actors_v2::storage::StorageActor::new(storage_config)
                .await
                .expect("Failed to create StorageActor V2")
                .start();
            info!("✓ StorageActor V2 started");

            // 2. Initialize EngineActor V2
            info!("⚙️  Initializing EngineActor V2...");
            let v2_http_engine_json_rpc = new_http_engine_json_rpc(
                v2_geth_url,
                JwtKey::from_slice(&v2_jwt_secret).unwrap()
            );
            let v2_public_execution_json_rpc = new_http_public_execution_json_rpc(v2_geth_execution_url);
            let v2_engine = Engine::new(v2_http_engine_json_rpc, v2_public_execution_json_rpc);
            let engine_actor = crate::actors_v2::engine::EngineActor::new(v2_engine).start();
            info!("✓ EngineActor V2 started");

            // 3. Initialize NetworkActor V2
            info!("🌐 Initializing NetworkActor V2...");
            let network_config = crate::actors_v2::network::NetworkConfig {
                listen_addresses: vec![
                    format!("/ip4/{}/tcp/{}", v2_p2p_listen_addr, if v2_p2p_port == 0 { 0 } else { v2_p2p_port + 1000 })
                ],
                bootstrap_peers: v2_remote_bootnode.map(|b| vec![b]).unwrap_or_default(),
                max_connections: 100,
                connection_timeout: Duration::from_secs(30),
                gossip_topics: vec![
                    "alys-v2-blocks".to_string(),
                    "alys-v2-transactions".to_string(),
                    "alys-v2-auxpow".to_string(),
                ],
                message_size_limit: 4 * 1024 * 1024, // 4MB
                discovery_interval: Duration::from_secs(60),
            };
            let network_actor = crate::actors_v2::network::NetworkActor::new(network_config)
                .expect("Failed to create NetworkActor V2")
                .start();
            info!("✓ NetworkActor V2 started");

            // 4. Initialize SyncActor V2
            info!("🔄 Initializing SyncActor V2...");
            let sync_config = crate::actors_v2::network::SyncConfig {
                max_blocks_per_request: 128,
                sync_timeout: Duration::from_secs(30),
                max_concurrent_requests: 4,
                block_validation_timeout: Duration::from_secs(10),
                max_sync_peers: 8,
            };
            let sync_actor = crate::actors_v2::network::SyncActor::new(sync_config)
                .expect("Failed to create SyncActor V2")
                .start();
            info!("✓ SyncActor V2 started");

            // 5. Initialize ChainActor V2 and wire up dependencies
            info!("⛓️  Initializing ChainActor V2...");
            let mut chain_actor = crate::actors_v2::chain::ChainActor::new(v2_config, v2_state);

            // Wire actor dependencies
            chain_actor.set_storage_actor(storage_actor.clone());
            chain_actor.set_network_actors(network_actor.clone(), sync_actor.clone());
            chain_actor.set_engine_actor(engine_actor.clone());

            let chain_actor_addr = chain_actor.start();
            info!("✓ ChainActor V2 started with all dependencies wired");

            // Clone chain_actor_addr for slot worker (before RPC consumes it)
            let chain_actor_addr_for_slot_worker = chain_actor_addr.clone();

            // 6. Initialize RPC server
            info!("🔌 Starting V2 RPC server on port 3001...");
            let rpc_config = RpcConfig {
                bind_address: "127.0.0.1:3001".parse().expect("Valid address"),
                request_timeout: Duration::from_secs(30),
                enable_logging: true,
                enable_metrics: true,
            };

            let rpc_actor = RpcActor::new(rpc_config, chain_actor_addr).start();

            match rpc_actor.send(StartRpcServer).await {
                Ok(Ok(())) => info!("✓ V2 RPC server started successfully on port 3001"),
                Ok(Err(e)) => error!("✗ V2 RPC server failed to start: {:?}", e),
                Err(e) => error!("✗ V2 RPC actor mailbox error: {:?}", e),
            }

            info!("🎉 V2 Actor System fully initialized and operational!");

            // 7. Start V2 Aura slot worker (if validator)
            if v2_is_validator && !v2_not_validator {
                info!("⏰ Starting V2 Aura slot worker...");

                tokio::spawn(async move {
                    crate::actors_v2::slot_worker::AuraSlotWorkerV2::new(
                        Duration::from_millis(v2_slot_duration),
                        v2_authorities_for_slot_worker,
                        v2_maybe_aura_signer_for_slot_worker,
                        chain_actor_addr_for_slot_worker,
                    )
                    .start_slot_worker()
                    .await;
                });

                info!("✓ V2 Aura slot worker started successfully");
            } else {
                info!("ℹ️  V2 Aura slot worker not started (not configured as validator)");
            }

                    // Keep actors alive - this task runs indefinitely
                    loop {
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    }
                }).await;
            });
        });

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
