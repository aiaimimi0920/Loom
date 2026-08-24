//! Tauri command adapters around daemon, package, and preview operations.

use super::*;

#[tauri::command]
pub(super) async fn start_loom_daemon() -> Result<LoomDaemonStartResult, String> {
    run_blocking_command(start_loom_daemon_blocking).await
}

#[tauri::command]
pub(super) async fn post_loom_daemon_json(
    base_url: String,
    path: String,
    body: Value,
) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        http_post_json(&resolved_base_url, &path, &body)
    })
    .await
}

#[tauri::command]
pub(super) async fn install_packaged_framework(
    base_url: String,
    id: String,
) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let current_exe =
            std::env::current_exe().map_err(|error| format!("无法定位 Loom.exe：{error}"))?;
        install_packaged_framework_from_exe(&resolved_base_url, &id, &current_exe)
    })
    .await
}

#[tauri::command]
pub(super) async fn bootstrap_packaged_arts(
    base_url: String,
) -> Result<PackagedArtBootstrapResult, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let current_exe =
            std::env::current_exe().map_err(|error| format!("无法定位 Loom.exe：{error}"))?;
        bootstrap_packaged_arts_from_exe(
            &resolved_base_url,
            &current_exe,
            &desktop_control_plane_root(),
        )
    })
    .await
}

#[tauri::command]
pub(super) async fn get_loom_daemon_json(base_url: String, path: String) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        http_get_json(&resolved_base_url, &path)
    })
    .await
}

#[tauri::command]
pub(super) async fn put_loom_daemon_json(
    base_url: String,
    path: String,
    body: Value,
) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        http_put_json(&resolved_base_url, &path, &body)
    })
    .await
}

#[tauri::command]
pub(super) async fn delete_loom_daemon_json(
    base_url: String,
    path: String,
) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        http_delete_json(&resolved_base_url, &path)
    })
    .await
}

// Fetch a Hook canvas preview image through the native HTTP client and return it
// as a base64 `data:` URL. The WebView cannot reliably load `http://127.0.0.1`
// daemon images with an `<img src>` tag, so the frontend renders previews from
// the data URL this command returns instead of a direct daemon URL.
#[tauri::command]
pub(super) async fn read_hook_canvas_preview(
    base_url: String,
    path: String,
) -> Result<String, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        let (content_type, bytes) = http_get_binary(&resolved_base_url, &path)?;
        let encoded = base64_encode(&bytes);
        Ok(format!("data:{content_type};base64,{encoded}"))
    })
    .await
}
