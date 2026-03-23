//! Mock governance server for Alys testnet.
//!
//! Provides a gRPC server implementing the GovernanceService for testing
//! validator integration. Supports:
//! - Auto-ACK for peg-in verification requests
//! - Periodic validator set update pushes
//! - Chaos injection modes (latency, failures, disconnects)

mod chaos;
mod config;
mod server;

use clap::Parser;
use config::Config;
use eyre::Result;
use governance_proto::GovernanceServiceServer;
use server::MockGovernanceService;
use tonic::transport::Server;
use tracing::{info, Level};
use tracing_subscriber::{prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::parse();

    // Initialize logging
    let log_level = config.log_level.parse::<Level>().unwrap_or(Level::INFO);
    let filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(filter)
        .init();

    // Log configuration
    info!("Starting mock governance server");
    info!("  Listen address: {}", config.listen_addr);
    info!("  Chain ID: {}", config.chain_id);
    info!("  Auto-ACK peg-ins: {}", config.auto_ack_pegins);
    if config.chaos_mode {
        info!("  Chaos mode: ENABLED");
        info!("    Peg-in reject rate: {:.1}%", config.pegin_reject_rate * 100.0);
        info!("    Response delay: {}ms", config.response_delay_ms);
        if config.disconnect_after > 0 {
            info!("    Disconnect after: {} requests", config.disconnect_after);
        }
    }
    if config.push_validator_update_interval > 0 {
        info!(
            "  Push validator updates: every {}s",
            config.push_validator_update_interval
        );
    }

    // Create gRPC service
    let service = MockGovernanceService::new(config.clone());

    // Parse listen address
    let addr = config
        .listen_addr
        .parse()
        .map_err(|e| eyre::eyre!("Invalid listen address: {}", e))?;

    info!("Mock governance server listening on {}", addr);

    // Start server with graceful shutdown
    Server::builder()
        .add_service(GovernanceServiceServer::new(service))
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    info!("Mock governance server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating shutdown");
        }
        _ = terminate => {
            info!("Received SIGTERM, initiating shutdown");
        }
    }
}
