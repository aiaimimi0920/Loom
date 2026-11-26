//! Desktop client/server connection profile commands (`local` bundled daemon vs `remote` LAN server).

use super::*;
use loom_local_data::{
    ConnectionMode, LoomConnectionStatus, LoomConnectionStore, LoomConnectionUpdate,
};

pub(super) fn connection_store() -> LoomConnectionStore {
    LoomConnectionStore::new(desktop_control_plane_root())
}

pub(super) fn is_remote_connection_mode() -> bool {
    connection_store()
        .load_profile()
        .map(|profile| profile.mode == ConnectionMode::Remote && profile.remote_url.is_some())
        .unwrap_or(false)
}

#[tauri::command]
pub(super) fn get_loom_connection_profile() -> Result<LoomConnectionStatus, String> {
    let fallback = configured_local_daemon_url();
    connection_store().status(&fallback)
}

#[tauri::command]
pub(super) async fn save_loom_connection_profile(
    update: LoomConnectionUpdate,
) -> Result<LoomConnectionStatus, String> {
    run_blocking_command(move || save_loom_connection_profile_blocking(update)).await
}

pub(super) fn save_loom_connection_profile_blocking(
    update: LoomConnectionUpdate,
) -> Result<LoomConnectionStatus, String> {
    let start_guard = DAEMON_START_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if update.mode == ConnectionMode::Remote {
        let endpoint = loom_local_data::parse_loom_endpoint_url(
            update.remote_url.as_deref().unwrap_or_default(),
            true,
        )?;
        if endpoint.scheme != "http" {
            return Err("Native HTTPS connections are not supported yet. Open the server's HTTPS /ui/ console in your browser; the current connection has not been changed.".to_owned());
        }
    }
    let fallback =
        std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_owned());
    let mut status = connection_store().save_update(update, &fallback)?;
    match status.mode {
        ConnectionMode::Remote => {
            stop_owned_daemon_process();
            if let Ok(mut active) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
                *active = None;
            }
            if let Ok(mut bridge) = ACTIVE_OTTER_BRIDGE_URL
                .get_or_init(|| Mutex::new(None))
                .lock()
            {
                *bridge = None;
            }
        }
        ConnectionMode::Local => {
            // A repeated Local save must retain the ports of an already owned isolated daemon.
            status.active_url = start_loom_daemon_locked(&start_guard)
                .map_err(|error| {
                    format!("Local profile saved, but daemon startup failed: {error}")
                })?
                .base_url;
        }
    }
    Ok(status)
}
