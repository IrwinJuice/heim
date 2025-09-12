use std::ffi::OsString;
use windows_service::define_windows_service;
use crate::run_service;

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

