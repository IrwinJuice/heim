use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::runtime::Runtime;
use tracing::error;
use windows_service::{define_windows_service, service_control_handler};
use windows_service::service::{ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_control_handler::ServiceControlHandlerResult;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use crate::{init_tracing, run_http_server};

pub const SERVICE_NAME: &str = "Heim";
pub const SERVICE_DISPLAY_NAME: &str = "Heim REST API (Windows Service)";

// Wire Windows service entry point to our Rust function
define_windows_service!(ffi_service_main, windows_service_main);

fn windows_service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        // Service stdout/stderr are not visible; write errors to a temp file.
        let _ = std::fs::write(
            "C:\\Windows\\Temp\\axum_windows_service_error.log",
            format!("{e:?}"),
        );
    }
}

pub fn install_service() -> anyhow::Result<()> {
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

    let _ = service.set_description("Heim running as a Windows Service");
    Ok(())
}

pub fn uninstall_service() -> anyhow::Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(SERVICE_NAME, ServiceAccess::DELETE)?;
    service.delete()?;
    Ok(())
}

pub fn start_service() -> anyhow::Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?;
    service.start(&[] as &[OsString])?; // No args passed
    Ok(())
}

pub fn stop_service() -> anyhow::Result<()> {
    let manager = service_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
    )?;
    service.stop()?;
    Ok(())
}

fn current_exe_path() -> anyhow::Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

fn service_manager(connect_flags: ServiceManagerAccess) -> anyhow::Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, connect_flags)?)
}

// -----------------------
// Service lifecycle glue
// -----------------------
fn run_service() -> anyhow::Result<()> {
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
