mod cli;
mod config;
#[cfg(feature = "win-service")]
mod win_service;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, time::sleep};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "win-service")]
use crate::cli::args::{Cli, Commands};
use crate::config::{Config, load_config};
#[cfg(feature = "win-service")]
use crate::win_service::{
    SERVICE_DISPLAY_NAME, SERVICE_NAME, install_service, start_service, stop_service,
    uninstall_service,
};
#[cfg(feature = "win-service")]
use clap::Parser;

fn init_tracing(default_level: &str) {
    // Initialize tracing once. Safe to call multiple times; subsequent calls are no-ops.
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(false))
        .try_init();
}

#[derive(Clone)]
struct AppState {
    started_at: std::time::Instant,
}

async fn root() -> &'static str {
    "Axum + Tokio running as a Windows Service. Try GET /health or POST /echo"
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    uptime_secs: u64,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        status: "ok",
        uptime_secs: state.started_at.elapsed().as_secs(),
    })
}

#[derive(Deserialize, Serialize)]
struct EchoPayload {
    message: String,
}

async fn echo(Json(payload): Json<EchoPayload>) -> Json<EchoPayload> {
    Json(payload)
}

async fn run_http_server(addr: SocketAddr, stop_flag: Arc<AtomicBool>) -> Result<()> {
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .with_state(AppState {
            started_at: std::time::Instant::now(),
        });
    let listener = TcpListener::bind(addr).await?;
    info!("HTTP server listening on http://{addr}");

    let shutdown = async move {
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                info!("Shutdown signal received (service stop).");
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    info!("HTTP server exited.");
    Ok(())
}

#[cfg(not(feature = "win-service"))]
fn main() -> Result<()> {
    print!("main");
    Ok(())
}

#[cfg(feature = "win-service")]
fn main() -> Result<()> {
    let args = Cli::parse();
    let config = load_config()?;


    match args.command {
        Commands::Install => {
            install_service()?;
            println!("Service installed: {}", SERVICE_NAME);
        }
        Commands::Uninstall => {
            uninstall_service()?;
            println!("Service uninstalled: {}", SERVICE_NAME);
        }
        Commands::Start => {
            start_service(config)?;
            println!("Service started: {}", SERVICE_NAME);
        }
        Commands::Stop => {
            stop_service()?;
            println!("Service stopped: {}", SERVICE_NAME);
        }
    }

    Ok(())
}
