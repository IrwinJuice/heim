mod win_service;
mod config;

use std::{
    ffi::OsString,
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
use tokio::{net::TcpListener, runtime::Runtime, time::sleep};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use windows_service::service::ServiceStartType;
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use crate::win_service::{SERVICE_DISPLAY_NAME, SERVICE_NAME};

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
    let app =
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/echo", post(echo))
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

// -----------------------
// Service lifecycle glue
// -----------------------
fn run_service() -> Result<()> {
    init_tracing("info");

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_for_handler = stop_flag.clone();

    // Register control handler
    let handler = move |control| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                stop_for_handler.store(true, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, handler)?;

    // Notify SCM: starting
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 1,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;

    // Build a Tokio runtime and start server
    let rt = Runtime::new()?;

    // Notify SCM: running and accepting stop
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    // Bind address; keep it on localhost by default to avoid firewall prompts.
    let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();

    let result = rt.block_on(run_http_server(addr, stop_flag));

    if let Err(err) = &result {
        error!("Server error: {err:?}");
    }

    // Notify SCM: stopping
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(3),
        process_id: None,
    })?;

    // Final transition: stopped
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    result.map(|_| ())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("install") => {
            install_service()?;
            println!("Service installed: {}", SERVICE_NAME);
        }
        Some("uninstall") => {
            uninstall_service()?;
            println!("Service uninstalled: {}", SERVICE_NAME);
        }
        Some("start") => {
            start_service()?;
            println!("Service started: {}", SERVICE_NAME);
        }
        Some("stop") => {
            stop_service()?;
            println!("Service stopped: {}", SERVICE_NAME);
        }
        Some("run") => {
            // Console/dev mode with Ctrl+C to stop
            init_tracing("debug");
            let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_c = stop_flag.clone();

            // Watch for Ctrl+C to trigger graceful shutdown
            thread::spawn(move || {
                // Use a blocking thread to listen for Ctrl+C from tokio
                let rt = Runtime::new().expect("create rt");
                rt.block_on(async {
                    let _ = tokio::signal::ctrl_c().await;
                    stop_flag_c.store(true, Ordering::SeqCst);
                });
            });

            let rt = Runtime::new()?;
            rt.block_on(async move {
                if let Err(e) = run_http_server(addr, stop_flag).await {
                    error!("Server error (console): {e:?}");
                }
            });
        }
        Some(_) => {
            print_usage();
        }
        None => {
            // No args: launched by Service Control Manager
            service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!(
        "Usage:
  {} install     # Install the service (admin required)
  {} uninstall   # Uninstall the service (admin required)
  {} start       # Start the service
  {} stop        # Stop the service
  {} run         # Run in console (debug)",
        exe_name(),
        exe_name(),
        exe_name(),
        exe_name(),
        exe_name(),
    );
}

fn exe_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "service.exe".to_string())
}

fn current_exe_path() -> Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

fn service_manager(connect_flags: ServiceManagerAccess) -> Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, connect_flags)?)
}

fn install_service() -> Result<()> {
    let exe_path = current_exe_path()?;
    let manager =
        service_manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;

    let service_info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: SERVICE_DISPLAY_NAME.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::OnDemand, // Change to Auto or AutoDelayed for automatic start
        error_control: ServiceErrorControl::Normal,
        executable_path: exe_path,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let service = manager.create_service(
        &service_info,
        ServiceAccess::QUERY_STATUS
            | ServiceAccess::START
            | ServiceAccess::STOP
            | ServiceAccess::DELETE,
    )?;

    let _ = service.set_description("Axum + Tokio REST API running as a Windows Service (Rust)");
    Ok(())
}

fn uninstall_service() -> Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(SERVICE_NAME, ServiceAccess::DELETE)?;
    service.delete()?;
    Ok(())
}

fn start_service() -> Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?;
    service.start(&[] as &[OsString])?; // No args passed
    Ok(())
}

fn stop_service() -> Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
    )?;
    service.stop()?;
    Ok(())
}
