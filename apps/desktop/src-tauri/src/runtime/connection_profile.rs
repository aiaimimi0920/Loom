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
    let fallback =
        std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_owned());
    connection_store().status(&fallback)
}

#[tauri::command]
pub(super) async fn save_loom_connection_profile(
    update: LoomConnectionUpdate,
) -> Result<LoomConnectionStatus, String> {
    run_blocking_command(move || {
        let fallback =
            std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_owned());
        let status = connection_store().save_update(update, &fallback)?;
        match status.mode {
            ConnectionMode::Remote => {
                stop_owned_daemon_process();
                if let Ok(mut active) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
                    *active = Some(status.active_url.clone());
                }
                if let Ok(mut bridge) = ACTIVE_OTTER_BRIDGE_URL
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                {
                    *bridge = status.otter_bridge_url.clone();
                }
            }
            ConnectionMode::Local => {
                if let Ok(mut active) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
                    *active = None;
                }
                if let Ok(mut bridge) = ACTIVE_OTTER_BRIDGE_URL
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                {
                    *bridge = None;
                }
                let _ = start_loom_daemon_blocking();
            }
        }
        Ok(status)
    })
    .await
}
