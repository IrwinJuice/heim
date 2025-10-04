mod cli;
mod config;
mod error;
mod heim;
#[cfg(feature = "win-service")]
mod win_service;

use heim::load_heim;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Mutex,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    routing::{get, post},
};
use notify::{RecommendedWatcher, Watcher};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::{net::TcpListener, time::sleep};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{Config, load_config};

#[cfg(feature = "win-service")]
use crate::cli::args::{Cli, Commands};
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

// #[derive(Serialize)]
// struct Health {
//     status: &'static str,
//     uptime_secs: u64,
// }

// async fn health(State(state): State<AppState>) -> Json<Health> {
//     Json(Health {
//         status: "ok",
//         uptime_secs: state.started_at.elapsed().as_secs(),
//     })
// }

// async fn copy(State(state): State<AppState>) -> Json<Health> {
//     Json(Health {
//         status: "ok",
//         uptime_secs: state.started_at.elapsed().as_secs(),
//     })
// }

async fn run_http_server(addr: SocketAddr, stop_flag: Arc<AtomicBool>) -> Result<()> {
    let app = Router::new()
        // .route("/health", get(health))
        // .route("/copy", get(copy)).layer(DefaultBodyLimit::disable())
        .route("/deploy", get(deploy))
        .layer(DefaultBodyLimit::disable())
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
#[tokio::main]
async fn main() -> Result<()> {
    let path_to_heim_file = "Heim.json";
    let path: PathBuf = path_to_heim_file.into();
    let config = load_config().unwrap();
    let heim = Arc::new(Mutex::new(load_heim(&path_to_heim_file).await.unwrap()));

    
    let (tx, mut rx) = mpsc::channel(1);

    // watch on Heim.json file
    let mut watcher: RecommendedWatcher = Watcher::new(
        move |res: notify::Result<notify::Event>| {
            let tx = tx.clone();
            tokio::spawn(async move {
                if let Ok(event) = res {
                    let _ = tx.send(event).await;
                }
            });
        },
        notify::Config::default(),
    )?;
    watcher.watch(&path, notify::RecursiveMode::NonRecursive)?;

    let heim_clone = Arc::clone(&heim);
    tokio::spawn(async move {
        loop {
            if let Some(event) = rx.recv().await {
                if event.kind.is_modify() {
                    match load_heim(&path_to_heim_file).await {
                        Ok(heim) => {
                            *heim_clone.lock().unwrap() = heim;
                            info!("Heim file has been updated!")
                        },
                        Err(e) => {
                            error!("{}", e)
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

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

// pub struct Artifact {
//     pub id: String,
//     pub file: File,
// }

async fn deploy(mut multipart: Multipart) {
    // let file;
    // let id;

    todo!();
    // while let Some(mut field) = multipart.next_field().await.unwrap() {
    //     let name = field.name().unwrap().to_string();
    //
    //     if name == "file" {
    //         file = field
    //             .bytes()
    //             .await
    //             .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    //     } else if name == "artifact_id" {
    //         id = field.text().await;
    //     } else {
    //         // Handle unknown fields
    //         warn!("Unknown field: {}", name);
    //         Err(StatusCode::BAD_REQUEST)
    //     }
    // }

    // file

    // Ok(Artifact { id, file })
}

// impl<S> FromRequestParts<S> for Token
// where
//     S: Send + Sync,
// {
//     type Rejection = AuthError;

//     async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
//         // Extract the token from the authorization header
//         let TypedHeader(Authorization(bearer)) = parts
//             .extract::<TypedHeader<Authorization<Bearer>>>()
//             .await
//             .map_err(|_| AuthError::InvalidToken)?;
//         // Decode the user data
//         let token_data = decode::<Claims>(bearer.token(), &KEYS.decoding, &Validation::default())
//             .map_err(|_| AuthError::InvalidToken)?;

//         Ok(token_data.claims)
//     }
// }

// pub struct Token {
//     token: String,
//     kind: String
// }
