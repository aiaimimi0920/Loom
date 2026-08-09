use anyhow::{Context, Result};
use loom_daemon::{
    daemon_help_text, daemon_version_text, default_run_store_path, DaemonConfig, LoomDaemon,
};
use std::sync::mpsc;

#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{FALSE, TRUE};
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_LOGOFF_EVENT,
    CTRL_SHUTDOWN_EVENT,
};

#[cfg(windows)]
static SHUTDOWN_SENDER: OnceLock<mpsc::Sender<()>> = OnceLock::new();

#[cfg(windows)]
unsafe extern "system" fn console_control_handler(control_type: u32) -> i32 {
    if !matches!(
        control_type,
        CTRL_C_EVENT
            | CTRL_BREAK_EVENT
            | CTRL_CLOSE_EVENT
            | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT
    ) {
        return FALSE;
    }
    if let Some(sender) = SHUTDOWN_SENDER.get() {
        let _ = sender.send(());
    }
    TRUE
}

#[cfg(windows)]
fn install_shutdown_handler(sender: mpsc::Sender<()>) -> Result<()> {
    SHUTDOWN_SENDER
        .set(sender)
        .map_err(|_| anyhow::anyhow!("Loom daemon shutdown handler was already installed"))?;
    let registered = unsafe { SetConsoleCtrlHandler(Some(console_control_handler), TRUE) };
    if registered == 0 {
        anyhow::bail!(
            "register Loom daemon console shutdown handler: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_shutdown_handler(_sender: mpsc::Sender<()>) -> Result<()> {
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{}", daemon_help_text());
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("{}", daemon_version_text());
        return Ok(());
    }

    let port = std::env::var("LOOM_DAEMON_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8765);
    let host = std::env::var("LOOM_DAEMON_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let mut config = DaemonConfig::bind_host(host, port)
        .with_brain_planner_from_env()?
        .with_request_executor_from_env()?
        .with_sqlite_run_store(default_run_store_path());
    if let Ok(value) = std::env::var("LOOM_BUNDLED_ART_SHA256_ALLOWLIST") {
        config = config.with_bundled_art_sha256_allowlist(value.split(','))?;
    }
    if let Ok(token) = std::env::var("LOOM_DAEMON_TOKEN") {
        config = config.with_bearer_token(token);
    }
    if let Some(manifest_dir) = manifest_dir_from_args(&args)? {
        config = config.with_manifest_dir(manifest_dir);
    } else if let Ok(manifest_dir) = std::env::var("LOOM_CAPABILITY_MANIFEST_DIR") {
        if !manifest_dir.trim().is_empty() {
            config = config.with_manifest_dir(manifest_dir);
        }
    }
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    install_shutdown_handler(shutdown_tx)?;
    let daemon = LoomDaemon::bind(config)?;
    let address = daemon.local_addr()?;
    println!(
        "loom-daemon {} listening on http://{}",
        loom_core::LOOM_VERSION,
        address
    );

    daemon.serve_until(shutdown_rx)?;
    Ok(())
}

fn manifest_dir_from_args(args: &[String]) -> Result<Option<String>> {
    for (index, arg) in args.iter().enumerate() {
        if let Some(value) = arg.strip_prefix("--manifest-dir=") {
            return Ok((!value.trim().is_empty()).then(|| value.to_owned()));
        }
        if arg == "--manifest-dir" {
            let value = args
                .get(index + 1)
                .with_context(|| "--manifest-dir requires a directory path")?;
            return Ok((!value.trim().is_empty()).then(|| value.to_owned()));
        }
    }
    Ok(None)
}
