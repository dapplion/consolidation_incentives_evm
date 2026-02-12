//! Consolidation Incentives Service
//!
//! REST API and auto-submitter for consolidation reward claims.

mod api;
mod scanner;
mod state;
mod submitter;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "consolidation-service")]
#[command(about = "Auto-submitter service for Gnosis consolidation incentives")]
struct Args {
    /// Beacon node URL
    #[arg(long, env = "BEACON_URL", default_value = "http://localhost:5052")]
    beacon_url: String,

    /// Gnosis RPC URL
    #[arg(long, env = "RPC_URL", default_value = "https://rpc.gnosis.gateway.fm")]
    rpc_url: String,

    /// Contract address
    #[arg(long, env = "CONTRACT_ADDRESS")]
    contract_address: Option<String>,

    /// Private key for transaction signing (hex, without 0x prefix)
    #[arg(long, env = "PRIVATE_KEY")]
    private_key: Option<String>,

    /// API listen address
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,

    /// Metrics listen address
    #[arg(long, default_value = "0.0.0.0:9090")]
    metrics_listen: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment from .env if present
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    tracing::info!("Starting consolidation incentives service");
    tracing::info!(beacon_url = %args.beacon_url, "Beacon node");
    tracing::info!(listen = %args.listen, "API server");

    // Initialize application state
    let app_state = state::AppState::new();

    // Start API server
    let api_handle = tokio::spawn(api::run_server(args.listen.clone(), app_state.clone()));

    // TODO: Start scanner and submitter when contract is deployed

    // Wait for shutdown
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal");
        }
        result = api_handle => {
            if let Err(e) = result {
                tracing::error!(error = %e, "API server error");
            }
        }
    }

    Ok(())
}
