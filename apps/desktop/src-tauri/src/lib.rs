use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};

const DEFAULT_LOOM_DAEMON_URL: &str = "http://127.0.0.1:8765";
const DEFAULT_HOOK_BRIDGE_URL: &str = "ws://127.0.0.1:19820";
const HOOK_COMPANION_VERSION: &str = "0.1.7";
const LOOM_DAEMON_EXECUTABLE_ENV: &str = "LOOM_DAEMON_EXECUTABLE";
const FRAMEWORK_PACKAGE_CATALOG_ENV: &str = "LOOM_FRAMEWORK_PACKAGE_CATALOG_DIR";
const FRAMEWORK_PACKAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const ART_PACKAGE_CATALOG_ENV: &str = "LOOM_ART_PACKAGE_CATALOG_DIR";
const BUNDLED_ART_SHA256_ALLOWLIST_ENV: &str = "LOOM_BUNDLED_ART_SHA256_ALLOWLIST";
const ART_PACKAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const LOOM_DAEMON_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const LOOM_MCP_REGISTRY_REQUEST_TIMEOUT: Duration = Duration::from_secs(50);
const OFFICIAL_FRAMEWORK_IDS: [&str; 4] = ["process", "cloud_api", "mcp", "workflow"];
static PACKAGED_ART_BOOTSTRAP_LOCK: Mutex<()> = Mutex::new(());
static DAEMON_START_LOCK: Mutex<()> = Mutex::new(());
static ACTIVE_DAEMON_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static ACTIVE_HOOK_BRIDGE_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static OWNED_DAEMON_PROCESS: OnceLock<Mutex<Option<std::process::Child>>> = OnceLock::new();
static LOOM_CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(true);
static LOOM_EXITING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
const LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV: &str = "LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT";
#[cfg(target_os = "windows")]
const DEFAULT_WEBVIEW2_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRuntimeConfig {
    pub loom_daemon_url: String,
    pub settings_url: String,
    pub hook_bridge_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsLinks {
    pub root: String,
    pub tea: String,
    pub hook: String,
    pub talk: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationDiagnostics {
    pub app: String,
    pub app_name: String,
    pub version: String,
    pub repository_url: Option<String>,
    pub commit_short: Option<String>,
    pub log_dir: String,
    pub log_file: Option<String>,
    pub log_file_exists: bool,
}

fn application_log_dir(app: &str) -> Result<PathBuf, String> {
    match app {
        "loom" => Ok(std::env::var_os("LOOM_LOG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| desktop_control_plane_root().join("logs"))),
        "hook" => Ok(std::env::var_os("HOOK_LOG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .map(|root| root.join("Hook").join("logs"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("Hook").join("logs"))),
        _ => Err("不支持的应用诊断目标。".to_owned()),
    }
}

fn newest_log_file(log_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(log_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let is_log = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("log"));
            if !is_log || !path.is_file() {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn application_diagnostics(app: &str) -> Result<ApplicationDiagnostics, String> {
    let log_dir = application_log_dir(app)?;
    let (app_name, version, repository_url, commit_short, log_file) = match app {
        "loom" => (
            "Loom",
            env!("CARGO_PKG_VERSION").to_owned(),
            Some(env!("LOOM_BUILD_REPOSITORY").to_owned()),
            Some(env!("LOOM_BUILD_COMMIT").to_owned()),
            newest_log_file(&log_dir),
        ),
        "hook" => {
            let log_file = log_dir.join("hook-runtime.log");
            (
                "Hook",
                std::env::var("LOOM_HOOK_APP_VERSION")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| HOOK_COMPANION_VERSION.to_owned()),
                Some(env!("HOOK_BUILD_REPOSITORY").to_owned()),
                Some(env!("HOOK_BUILD_COMMIT").to_owned()),
                log_file.is_file().then_some(log_file),
            )
        }
        _ => return Err("不支持的应用诊断目标。".to_owned()),
    };
    Ok(ApplicationDiagnostics {
        app: app.to_owned(),
        app_name: app_name.to_owned(),
        version,
        repository_url,
        commit_short,
        log_dir: log_dir.to_string_lossy().into_owned(),
        log_file_exists: log_file.is_some(),
        log_file: log_file.map(|path| path.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
fn resolve_application_diagnostics(app: String) -> Result<ApplicationDiagnostics, String> {
    application_diagnostics(app.trim())
}

#[cfg(target_os = "windows")]
fn open_local_path(path: &Path, file: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut command = std::process::Command::new(if file { "notepad.exe" } else { "explorer.exe" });
    command.arg(path).creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 `{}`：{error}", path.display()))
}

#[cfg(target_os = "macos")]
fn open_local_path(path: &Path, _file: bool) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 `{}`：{error}", path.display()))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_local_path(path: &Path, _file: bool) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 `{}`：{error}", path.display()))
}

#[tauri::command]
fn open_application_log_location(app: String, target: String) -> Result<(), String> {
    let diagnostics = application_diagnostics(app.trim())?;
    match target.trim() {
        "directory" => {
            let path = PathBuf::from(diagnostics.log_dir);
            fs::create_dir_all(&path)
                .map_err(|error| format!("无法创建日志目录 `{}`：{error}", path.display()))?;
            open_local_path(&path, false)
        }
        "file" => {
            let path = diagnostics
                .log_file
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .ok_or_else(|| format!("{} 暂无可查看的日志。", diagnostics.app_name))?;
            open_local_path(&path, true)
        }
        _ => Err("不支持的日志打开方式。".to_owned()),
    }
}

fn is_allowed_repository_url(url: &str) -> bool {
    [env!("LOOM_BUILD_REPOSITORY"), env!("HOOK_BUILD_REPOSITORY")].contains(&url)
}

fn is_safe_external_https_url(url: &str) -> bool {
    tauri::Url::parse(url).is_ok_and(|parsed| {
        parsed.scheme() == "https"
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
    })
}

#[cfg(target_os = "windows")]
fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let target = url.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    let status = result as isize;
    if status > 32 {
        Ok(())
    } else {
        Err(format!("系统浏览器无法打开地址（ShellExecuteW={status}）。"))
    }
}

#[cfg(target_os = "macos")]
fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开仓库地址：{error}"))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开仓库地址：{error}"))
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !url.starts_with("https://") || !is_allowed_repository_url(url) {
        return Err("只允许打开 Loom 或 Hook 的官方仓库地址。".to_owned());
    }
    open_url_in_default_browser(url)
}

#[tauri::command]
fn open_mcp_source_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !is_safe_external_https_url(url) {
        return Err("只允许打开不包含账号信息的 HTTPS MCP 来源地址。".to_owned());
    }
    open_url_in_default_browser(url)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCacheEntry {
    pub key: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCacheSnapshot {
    pub temporary: HookCacheEntry,
    pub recycle_bin_entries: u64,
    pub reference_entries: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCacheClearResult {
    pub kind: String,
    pub freed_bytes: u64,
    pub snapshot: HookCacheSnapshot,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCachePreferences {
    pub recycle_bin_max_entries: u32,
    pub recycle_bin_retention_days: u32,
    pub temp_cache_max_bytes: u64,
    pub temp_cache_retention_days: u32,
}

fn read_hook_persisted_cache_settings() -> Option<HookCachePreferences> {
    let value: Value = serde_json::from_slice(
        &fs::read(hook_effective_app_data_dir().join("app-settings.json")).ok()?,
    )
    .ok()?;
    serde_json::from_value(value.get("cache")?.clone()).ok()
}

#[tauri::command]
async fn wait_for_hook_cache_settings(settings: HookCachePreferences) -> Result<bool, String> {
    run_blocking_command(move || {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if read_hook_persisted_cache_settings().as_ref() == Some(&settings) {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Err(
                    "缓存设置已保存，但 Hook 尚未确认应用；将在 Hook 下次连接时同步。".to_owned(),
                );
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    })
    .await
}

fn hook_app_data_contains_user_state(dir: &Path) -> bool {
    [
        "session.json",
        "history.json",
        "tool-settings.json",
        "app-settings.json",
        "images",
        "saved",
    ]
    .iter()
    .any(|entry| dir.join(entry).exists())
}

fn hook_effective_app_data_dir() -> PathBuf {
    let current = std::env::var_os("HOOK_APPDATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join("com.yamiyu.hook"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("com.yamiyu.hook"));
    for identifier in ["io.github.aiaimimi0920.hook", "com.vmjcv.hook"] {
        let legacy = current.with_file_name(identifier);
        if legacy.exists()
            && (!current.exists()
                || (!hook_app_data_contains_user_state(&current)
                    && hook_app_data_contains_user_state(&legacy)))
        {
            return legacy;
        }
    }
    current
}

fn hook_clipboard_cache_dir() -> PathBuf {
    std::env::var_os("HOOK_CLIPBOARD_CACHE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join("Hook").join("clipboard_cache"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("Hook").join("clipboard_cache"))
}

fn directory_usage(path: &Path) -> Result<(u64, u64), String> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let mut bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("无法读取缓存目录 `{}`：{error}", directory.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("无法检查缓存目录 `{}`：{error}", directory.display()))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("无法检查缓存文件类型：{error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let len = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                bytes = bytes.saturating_add(len);
                file_count = file_count.saturating_add(1);
            }
        }
    }
    Ok((bytes, file_count))
}

fn hook_cache_entry(key: &str, label: &str, path: PathBuf) -> Result<HookCacheEntry, String> {
    let (bytes, file_count) = directory_usage(&path)?;
    Ok(HookCacheEntry {
        key: key.to_owned(),
        label: label.to_owned(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        file_count,
    })
}

fn hook_cache_snapshot() -> Result<HookCacheSnapshot, String> {
    let temporary = hook_cache_entry("temporary", "临时缓存", hook_clipboard_cache_dir())?;
    let session_path = hook_effective_app_data_dir().join("session.json");
    let session = fs::read(&session_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let collection_count = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_array)
            .map(|entries| entries.len() as u64)
            .unwrap_or(0)
    };
    Ok(HookCacheSnapshot {
        temporary,
        recycle_bin_entries: collection_count("recycleBin"),
        reference_entries: collection_count("referenceLibrary"),
    })
}

fn clear_directory_contents(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|error| format!("无法创建缓存目录 `{}`：{error}", path.display()))?;
        return Ok(());
    }
    for entry in fs::read_dir(path)
        .map_err(|error| format!("无法读取缓存目录 `{}`：{error}", path.display()))?
    {
        let entry =
            entry.map_err(|error| format!("无法检查缓存目录 `{}`：{error}", path.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("无法检查缓存文件类型：{error}"))?;
        let entry_path = entry.path();
        let result = if file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(&entry_path)
        } else {
            fs::remove_file(&entry_path)
        };
        result.map_err(|error| format!("无法删除缓存 `{}`：{error}", entry_path.display()))?;
    }
    Ok(())
}

#[tauri::command]
fn get_hook_cache_snapshot() -> Result<HookCacheSnapshot, String> {
    hook_cache_snapshot()
}

#[tauri::command]
fn clear_hook_cache(kind: String) -> Result<HookCacheClearResult, String> {
    let kind = kind.trim();
    let before = hook_cache_snapshot()?;
    match kind {
        "temporary" => clear_directory_contents(&hook_clipboard_cache_dir())?,
        "recycleBin" => {
            http_post_json(
                &configured_loom_daemon_url(),
                "/v1/artloom-compat/hook/cache-control",
                &serde_json::json!({ "action": "clearRecycleBin" }),
            )?;
        }
        "referenceLibrary" => {
            http_post_json(
                &configured_loom_daemon_url(),
                "/v1/artloom-compat/hook/cache-control",
                &serde_json::json!({ "action": "clearReferenceLibrary" }),
            )?;
        }
        _ => return Err("不支持的 Hook 缓存清理目标。".to_owned()),
    }
    let deadline = Instant::now() + Duration::from_secs(4);
    let snapshot = loop {
        let snapshot = hook_cache_snapshot()?;
        let cleared = match kind {
            "temporary" => snapshot.temporary.bytes == 0 && snapshot.temporary.file_count == 0,
            "recycleBin" => snapshot.recycle_bin_entries == 0,
            "referenceLibrary" => snapshot.reference_entries == 0,
            _ => false,
        };
        if cleared {
            break snapshot;
        }
        if Instant::now() >= deadline {
            return Err(format!("Hook 未在规定时间内完成 `{kind}` 清理。"));
        }
        std::thread::sleep(Duration::from_millis(80));
    };
    Ok(HookCacheClearResult {
        kind: kind.to_owned(),
        freed_bytes: before
            .temporary
            .bytes
            .saturating_sub(snapshot.temporary.bytes),
        snapshot,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomCacheEntry {
    pub key: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomCacheSnapshot {
    pub art_runtime: LoomCacheEntry,
    pub framework_temporary: LoomCacheEntry,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomCacheClearResult {
    pub kind: String,
    pub freed_bytes: u64,
    pub snapshot: LoomCacheSnapshot,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomCachePreferences {
    #[serde(alias = "art_cache_max_bytes")]
    pub art_cache_max_bytes: u64,
    #[serde(alias = "art_cache_retention_days")]
    pub art_cache_retention_days: u32,
    #[serde(alias = "framework_temp_retention_days")]
    pub framework_temp_retention_days: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomGeneralRuntimeSettings {
    #[serde(alias = "minimize_to_tray")]
    pub minimize_to_tray: bool,
}

fn read_loom_persisted_general_settings() -> Option<LoomGeneralRuntimeSettings> {
    let value: Value = serde_json::from_slice(
        &fs::read(
            desktop_control_plane_root()
                .join("settings")
                .join("artloom-compat-settings.json"),
        )
        .ok()?,
    )
    .ok()?;
    serde_json::from_value(value.get("general")?.clone()).ok()
}

#[tauri::command]
fn apply_loom_general_settings(settings: LoomGeneralRuntimeSettings) {
    LOOM_CLOSE_TO_TRAY.store(settings.minimize_to_tray, Ordering::Relaxed);
}

impl Default for LoomCachePreferences {
    fn default() -> Self {
        Self {
            art_cache_max_bytes: 1024 * 1024 * 1024,
            art_cache_retention_days: 30,
            framework_temp_retention_days: 3,
        }
    }
}

fn validate_loom_cache_preferences(settings: &LoomCachePreferences) -> Result<(), String> {
    if settings.art_cache_max_bytes != 0
        && !(64 * 1024 * 1024..=64 * 1024 * 1024 * 1024).contains(&settings.art_cache_max_bytes)
    {
        return Err("Art 运行缓存上限必须为无限制或介于 64 MB 到 64 GB 之间".to_owned());
    }
    if settings.art_cache_retention_days > 3650 {
        return Err("Art 运行缓存自动清理周期不能超过 3650 天".to_owned());
    }
    if settings.framework_temp_retention_days > 3650 {
        return Err("框架临时文件自动清理周期不能超过 3650 天".to_owned());
    }
    Ok(())
}

fn read_loom_persisted_cache_settings() -> Option<LoomCachePreferences> {
    let value: Value = serde_json::from_slice(
        &fs::read(
            desktop_control_plane_root()
                .join("settings")
                .join("artloom-compat-settings.json"),
        )
        .ok()?,
    )
    .ok()?;
    serde_json::from_value(value.get("loom_cache")?.clone()).ok()
}

fn loom_framework_temporary_dir() -> PathBuf {
    std::env::var_os("LOOM_FRAMEWORK_TEMP_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("loom-framework"))
}

fn collect_art_cache_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut cache_dirs = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("无法读取 Art 目录 `{}`：{error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("无法检查 Art 目录：{error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("无法检查 Art 缓存类型：{error}"))?;
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            if entry.file_name() == ".loom-cache" {
                cache_dirs.push(path);
            } else {
                pending.push(path);
            }
        }
    }
    Ok(cache_dirs)
}

fn loom_art_cache_dirs() -> Result<Vec<PathBuf>, String> {
    collect_art_cache_dirs(&desktop_control_plane_root().join("arts"))
}

fn loom_cache_entry(
    key: &str,
    label: &str,
    display_path: PathBuf,
    roots: &[PathBuf],
) -> Result<LoomCacheEntry, String> {
    let mut bytes = 0_u64;
    let mut file_count = 0_u64;
    for root in roots {
        let (root_bytes, root_files) = directory_usage(root)?;
        bytes = bytes.saturating_add(root_bytes);
        file_count = file_count.saturating_add(root_files);
    }
    Ok(LoomCacheEntry {
        key: key.to_owned(),
        label: label.to_owned(),
        path: display_path.to_string_lossy().into_owned(),
        bytes,
        file_count,
    })
}

fn loom_cache_snapshot() -> Result<LoomCacheSnapshot, String> {
    let art_root = desktop_control_plane_root().join("arts");
    let art_cache_dirs = loom_art_cache_dirs()?;
    let framework_root = loom_framework_temporary_dir();
    Ok(LoomCacheSnapshot {
        art_runtime: loom_cache_entry("artRuntime", "Art 运行缓存", art_root, &art_cache_dirs)?,
        framework_temporary: loom_cache_entry(
            "frameworkTemporary",
            "框架临时文件",
            framework_root.clone(),
            &[framework_root],
        )?,
    })
}

#[derive(Debug)]
struct CacheFileInfo {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

fn collect_cache_files(roots: &[PathBuf]) -> Result<Vec<CacheFileInfo>, String> {
    let mut files = Vec::new();
    let mut pending = roots.to_vec();
    while let Some(directory) = pending.pop() {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("无法读取缓存目录 `{}`：{error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("无法检查缓存目录：{error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("无法检查缓存文件类型：{error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let metadata = entry
                    .metadata()
                    .map_err(|error| format!("无法读取缓存文件信息：{error}"))?;
                files.push(CacheFileInfo {
                    path: entry.path(),
                    bytes: metadata.len(),
                    modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }
    }
    Ok(files)
}

fn remove_cache_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法删除缓存文件 `{}`：{error}", path.display())),
    }
}

fn prune_cache_roots(roots: &[PathBuf], max_bytes: u64, retention_days: u32) -> Result<(), String> {
    let now = SystemTime::now();
    let retention = Duration::from_secs(u64::from(retention_days).saturating_mul(86_400));
    let mut files = collect_cache_files(roots)?;
    if retention_days > 0 {
        for file in &files {
            if now.duration_since(file.modified).unwrap_or_default() >= retention {
                remove_cache_file(&file.path)?;
            }
        }
        files = collect_cache_files(roots)?;
    }
    if max_bytes == 0 {
        return Ok(());
    }
    files.sort_by_key(|file| file.modified);
    let mut total = files.iter().map(|file| file.bytes).sum::<u64>();
    for file in files {
        if total <= max_bytes {
            break;
        }
        remove_cache_file(&file.path)?;
        total = total.saturating_sub(file.bytes);
    }
    Ok(())
}

fn apply_loom_cache_preferences(settings: &LoomCachePreferences) -> Result<(), String> {
    validate_loom_cache_preferences(settings)?;
    prune_cache_roots(
        &loom_art_cache_dirs()?,
        settings.art_cache_max_bytes,
        settings.art_cache_retention_days,
    )?;
    prune_cache_roots(
        &[loom_framework_temporary_dir()],
        0,
        settings.framework_temp_retention_days,
    )
}

#[tauri::command]
fn get_loom_cache_snapshot() -> Result<LoomCacheSnapshot, String> {
    loom_cache_snapshot()
}

#[tauri::command]
async fn apply_loom_cache_settings(
    settings: LoomCachePreferences,
) -> Result<LoomCacheSnapshot, String> {
    run_blocking_command(move || {
        apply_loom_cache_preferences(&settings)?;
        loom_cache_snapshot()
    })
    .await
}

#[tauri::command]
async fn clear_loom_cache(kind: String) -> Result<LoomCacheClearResult, String> {
    run_blocking_command(move || clear_loom_cache_blocking(&kind)).await
}

fn clear_loom_cache_blocking(kind: &str) -> Result<LoomCacheClearResult, String> {
    let kind = kind.trim();
    let before = loom_cache_snapshot()?;
    match kind {
        "artRuntime" => {
            for cache_dir in loom_art_cache_dirs()? {
                clear_directory_contents(&cache_dir)?;
            }
        }
        "frameworkTemporary" => {
            clear_directory_contents(&loom_framework_temporary_dir())?;
        }
        _ => return Err("不支持的 Loom 缓存清理目标。".to_owned()),
    }
    let snapshot = loom_cache_snapshot()?;
    let freed_bytes = match kind {
        "artRuntime" => before
            .art_runtime
            .bytes
            .saturating_sub(snapshot.art_runtime.bytes),
        "frameworkTemporary" => before
            .framework_temporary
            .bytes
            .saturating_sub(snapshot.framework_temporary.bytes),
        _ => 0,
    };
    Ok(LoomCacheClearResult {
        kind: kind.to_owned(),
        freed_bytes,
        snapshot,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomDaemonStartResult {
    pub started: bool,
    pub base_url: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagedArtBootstrapResult {
    pub available: bool,
    pub applied: bool,
    pub catalog_hash: Option<String>,
    pub framework_ids: Vec<String>,
    pub art_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PackagedArtCatalog {
    #[serde(default)]
    packages: Vec<PackagedArtCatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct PackagedFrameworkCatalog {
    #[serde(default)]
    frameworks: Vec<PackagedFrameworkCatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct PackagedFrameworkCatalogEntry {
    id: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct PackagedArtCatalogEntry {
    id: String,
    framework: String,
    zip: String,
    sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomSnapshot {
    pub base_url: String,
    pub connection_state: String,
    pub checked_at: String,
    pub health: Option<Value>,
    pub status: Option<Value>,
    pub capabilities: Vec<Value>,
    pub mcp_servers: Vec<Value>,
    pub tools: Vec<Value>,
    pub python_arts: Vec<Value>,
    pub workflows: Vec<Value>,
    pub hook_bridge: Option<Value>,
    pub settings: SettingsLinks,
    pub error: Option<String>,
}

struct DaemonSnapshot {
    health: Value,
    status: Value,
    capabilities: Vec<Value>,
    mcp_servers: Vec<Value>,
    tools: Vec<Value>,
    python_arts: Vec<Value>,
    workflows: Vec<Value>,
    hook_bridge: Option<Value>,
    degraded_errors: Vec<String>,
}

#[tauri::command]
fn resolve_loom_daemon_url() -> DesktopRuntimeConfig {
    let loom_daemon_url = configured_loom_daemon_url();
    let settings_url = format!("{}/settings", loom_daemon_url.trim_end_matches('/'));

    DesktopRuntimeConfig {
        loom_daemon_url,
        settings_url,
        hook_bridge_url: configured_hook_bridge_url(),
    }
}

async fn run_blocking_command<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("Loom 桌面后台任务异常结束：{error}"))?
}

fn read_loom_snapshot_blocking(base_url: Option<String>) -> LoomSnapshot {
    let resolved_base_url = resolve_command_base_url(base_url.unwrap_or_default());
    let settings = settings_links(&resolved_base_url);
    let checked_at = chrono::Utc::now().to_rfc3339();

    match read_daemon_snapshot(&resolved_base_url) {
        Ok(snapshot) => {
            let daemon_mismatch = std::env::current_exe().ok().and_then(|current_exe| {
                daemon_path_mismatch_warning(&current_exe, &snapshot.health)
            });
            LoomSnapshot {
                base_url: resolved_base_url,
                connection_state: "online".to_string(),
                checked_at,
                health: Some(snapshot.health),
                status: Some(snapshot.status),
                capabilities: snapshot.capabilities,
                mcp_servers: snapshot.mcp_servers,
                tools: snapshot.tools,
                python_arts: snapshot.python_arts,
                workflows: snapshot.workflows,
                hook_bridge: snapshot.hook_bridge,
                settings,
                error: snapshot_error(&snapshot.degraded_errors, daemon_mismatch.as_deref()),
            }
        }
        Err(error) => LoomSnapshot {
            base_url: resolved_base_url,
            connection_state: "offline".to_string(),
            checked_at,
            health: None,
            status: None,
            capabilities: Vec::new(),
            mcp_servers: Vec::new(),
            tools: Vec::new(),
            python_arts: Vec::new(),
            workflows: Vec::new(),
            hook_bridge: None,
            settings,
            error: Some(error),
        },
    }
}

#[tauri::command]
async fn read_loom_snapshot(base_url: Option<String>) -> Result<LoomSnapshot, String> {
    run_blocking_command(move || Ok(read_loom_snapshot_blocking(base_url))).await
}

fn start_loom_daemon_blocking() -> Result<LoomDaemonStartResult, String> {
    let _start_guard = DAEMON_START_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if LOOM_EXITING.load(Ordering::Acquire) {
        return Err("Loom 正在退出，已取消启动本地服务。".to_owned());
    }
    let mut base_url = normalize_base_url(configured_loom_daemon_url());
    let mut isolated_hook_bridge_url = None;
    let current_exe =
        std::env::current_exe().map_err(|error| format!("无法定位 Loom.exe：{error}"))?;
    if let Ok(health) = http_get_json(&base_url, "/health") {
        if daemon_path_mismatch_warning(&current_exe, &health).is_none() {
            return Ok(LoomDaemonStartResult {
                started: false,
                base_url,
                path: String::new(),
                message: "Loom 本地服务已运行。".to_string(),
            });
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("无法为当前 Loom 分配本地服务端口：{error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("无法读取 Loom 本地服务端口：{error}"))?
            .port();
        drop(listener);
        base_url = format!("http://127.0.0.1:{port}");
        let hook_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("无法为当前 Loom 分配 Hook Bridge 端口：{error}"))?;
        let hook_port = hook_listener
            .local_addr()
            .map_err(|error| format!("无法读取 Loom Hook Bridge 端口：{error}"))?
            .port();
        drop(hook_listener);
        isolated_hook_bridge_url = Some(format!("ws://127.0.0.1:{hook_port}"));
    }

    let explicit_daemon_path = std::env::var(LOOM_DAEMON_EXECUTABLE_ENV)
        .ok()
        .and_then(|value| configured_daemon_executable(&value));
    let candidates = daemon_executable_candidates(
        &current_exe,
        explicit_daemon_path,
        development_repo_root().as_deref(),
    );
    let daemon_path = candidates.iter().find(|path| path.is_file()).ok_or_else(|| {
        let searched = candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "未找到 loom-daemon.exe，请检查 LOOM_DAEMON_EXECUTABLE、桌面程序 runtime 目录或开发构建路径：{searched}"
        )
    })?;
    let (host, port) = parse_loopback_http_url(&base_url)?;
    let bundled_art_sha256_allowlist = packaged_art_sha256_allowlist(&current_exe)?;
    let mut command = std::process::Command::new(&daemon_path);
    command
        .env("LOOM_DAEMON_HOST", host)
        .env("LOOM_DAEMON_PORT", port.to_string())
        .env_remove(BUNDLED_ART_SHA256_ALLOWLIST_ENV);
    if !bundled_art_sha256_allowlist.is_empty() {
        command.env(
            BUNDLED_ART_SHA256_ALLOWLIST_ENV,
            bundled_art_sha256_allowlist.join(","),
        );
    }

    // Write the discovery manifest to the shared Neuro capabilities dir so peer
    // apps (e.g. Hook) can find this daemon via %APPDATA%\Neuro\capabilities\loom.json.
    if let Some(appdata) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
        let manifest_dir = std::path::PathBuf::from(appdata)
            .join("Neuro")
            .join("capabilities");
        command.env("LOOM_CAPABILITY_MANIFEST_DIR", manifest_dir);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command
        .spawn()
        .map_err(|error| format!("启动 Loom 本地服务失败：{error}"))?;
    register_owned_daemon_process(child)?;

    if let Ok(mut active_url) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
        *active_url = Some(base_url.clone());
    }
    if let Some(hook_bridge_url) = isolated_hook_bridge_url {
        if let Ok(mut active_url) = ACTIVE_HOOK_BRIDGE_URL
            .get_or_init(|| Mutex::new(None))
            .lock()
        {
            *active_url = Some(hook_bridge_url);
        }
    }

    Ok(LoomDaemonStartResult {
        started: true,
        base_url,
        path: daemon_path.display().to_string(),
        message: "已启动 Loom 本地服务。".to_string(),
    })
}

fn register_owned_daemon_process(mut child: std::process::Child) -> Result<(), String> {
    if LOOM_EXITING.load(Ordering::Acquire) {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Loom 正在退出，已取消启动本地服务。".to_owned());
    }
    let processes = OWNED_DAEMON_PROCESS.get_or_init(|| Mutex::new(None));
    let mut owned = processes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Exit may begin after the fast check above but before this lock is acquired.
    // Rechecking while holding the same lock used by cleanup closes that window:
    // either registration wins and cleanup takes the child, or registration reaps it.
    if LOOM_EXITING.load(Ordering::Acquire) {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Loom 正在退出，已取消启动本地服务。".to_owned());
    }

    if let Some(mut previous) = owned.take() {
        match previous.try_wait() {
            Ok(Some(_)) => {
                let _ = previous.wait();
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                *owned = Some(previous);
                return Err("当前 Loom 已经拥有一个本地服务进程。".to_owned());
            }
        }
    }
    *owned = Some(child);
    Ok(())
}

fn stop_owned_daemon_process() -> Option<u32> {
    let process = OWNED_DAEMON_PROCESS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let mut process = process?;
    let pid = process.id();
    if process.try_wait().ok().flatten().is_none() {
        let _ = process.kill();
    }
    let _ = process.wait();
    if let Ok(mut active_url) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
        *active_url = None;
    }
    if let Ok(mut active_url) = ACTIVE_HOOK_BRIDGE_URL
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *active_url = None;
    }
    Some(pid)
}

fn begin_desktop_exit() {
    LOOM_EXITING.store(true, Ordering::Release);
    stop_owned_daemon_process();
}

#[cfg(test)]
fn owned_daemon_process_id() -> Option<u32> {
    OWNED_DAEMON_PROCESS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|owned| owned.as_ref().map(std::process::Child::id))
}

#[tauri::command]
async fn start_loom_daemon() -> Result<LoomDaemonStartResult, String> {
    run_blocking_command(start_loom_daemon_blocking).await
}

#[tauri::command]
async fn post_loom_daemon_json(
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
async fn install_packaged_framework(base_url: String, id: String) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let current_exe =
            std::env::current_exe().map_err(|error| format!("无法定位 Loom.exe：{error}"))?;
        install_packaged_framework_from_exe(&resolved_base_url, &id, &current_exe)
    })
    .await
}

#[tauri::command]
async fn bootstrap_packaged_arts(base_url: String) -> Result<PackagedArtBootstrapResult, String> {
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
async fn get_loom_daemon_json(base_url: String, path: String) -> Result<Value, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        http_get_json(&resolved_base_url, &path)
    })
    .await
}

#[tauri::command]
async fn put_loom_daemon_json(
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
async fn delete_loom_daemon_json(base_url: String, path: String) -> Result<Value, String> {
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
async fn read_hook_canvas_preview(base_url: String, path: String) -> Result<String, String> {
    run_blocking_command(move || {
        let resolved_base_url = resolve_command_base_url(base_url);
        let path = normalize_daemon_path(path)?;
        let (content_type, bytes) = http_get_binary(&resolved_base_url, &path)?;
        let encoded = base64_encode(&bytes);
        Ok(format!("data:{content_type};base64,{encoded}"))
    })
    .await
}

fn active_loom_daemon_url() -> Option<String> {
    if let Ok(active_url) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(active_url) = active_url.as_ref() {
            return Some(active_url.clone());
        }
    }
    None
}

fn configured_loom_daemon_url() -> String {
    if let Some(active_url) = active_loom_daemon_url() {
        return active_url;
    }
    std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_string())
}

fn configured_hook_bridge_url() -> String {
    if let Ok(active_url) = ACTIVE_HOOK_BRIDGE_URL
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        if let Some(active_url) = active_url.as_ref() {
            return active_url.clone();
        }
    }
    if let Ok(url) = std::env::var("LOOM_HOOK_BRIDGE_URL") {
        if !url.trim().is_empty() {
            return url.trim().to_owned();
        }
    }
    if let Ok(port) = std::env::var("LOOM_HOOK_BRIDGE_PORT") {
        if let Ok(port) = port.trim().parse::<u16>() {
            if port > 0 {
                return format!("ws://127.0.0.1:{port}");
            }
        }
    }
    DEFAULT_HOOK_BRIDGE_URL.to_owned()
}

fn resolve_command_base_url(base_url: String) -> String {
    resolve_command_base_url_with_active(base_url, active_loom_daemon_url())
}

fn resolve_command_base_url_with_active(base_url: String, active_url: Option<String>) -> String {
    if let Some(active_url) = active_url {
        return normalize_base_url(active_url);
    }
    normalize_base_url(if base_url.trim().is_empty() {
        configured_loom_daemon_url()
    } else {
        base_url
    })
}

fn normalize_base_url(base_url: String) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

fn settings_links(base_url: &str) -> SettingsLinks {
    let root = format!("{base_url}/settings");
    SettingsLinks {
        root: root.clone(),
        tea: format!("{root}/tea"),
        hook: format!("{root}/hook"),
        talk: format!("{root}/talk"),
    }
}

fn configured_daemon_executable(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn preferred_daemon_candidate(current_exe: &Path) -> PathBuf {
    let explicit_daemon_path = std::env::var(LOOM_DAEMON_EXECUTABLE_ENV)
        .ok()
        .and_then(|value| configured_daemon_executable(&value));
    let candidates = daemon_executable_candidates(
        current_exe,
        explicit_daemon_path,
        development_repo_root().as_deref(),
    );
    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .unwrap_or_else(|| {
            candidates
                .first()
                .cloned()
                .unwrap_or_else(|| daemon_sidecar_path_for_exe(current_exe))
        })
}

fn daemon_executable_path_from_health(health: &Value) -> Option<PathBuf> {
    health
        .get("executablePath")
        .or_else(|| health.get("executable_path"))
        .or_else(|| health.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        left.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
    }
    #[cfg(not(target_os = "windows"))]
    {
        left == right
    }
}

fn daemon_path_mismatch_warning(current_exe: &Path, health: &Value) -> Option<String> {
    let actual = daemon_executable_path_from_health(health)?;
    let expected = preferred_daemon_candidate(current_exe);
    if paths_match(&expected, &actual) {
        return None;
    }
    Some(format!(
        "检测到 127.0.0.1:8765 上运行的 Loom daemon 不是当前包自带的版本：当前 Loom 期望 `{}`, 但实际连接到 `{}`。这会让执行请求仍然落到旧 daemon 上，请先关闭旧 daemon 再重启当前包。",
        expected.display(),
        actual.display()
    ))
}

fn daemon_executable_candidates(
    current_exe: &Path,
    explicit_path: Option<PathBuf>,
    development_repo_root: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit_path {
        candidates.push(path);
    }
    candidates.push(daemon_sidecar_path_for_exe(current_exe));
    candidates.push(daemon_root_sibling_path_for_exe(current_exe));
    if let Some(repo_root) = development_repo_root {
        candidates.push(
            repo_root
                .join("target")
                .join("debug")
                .join("loom-daemon.exe"),
        );
    }
    candidates
}

fn daemon_sidecar_path_for_exe(current_exe: &Path) -> PathBuf {
    current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("runtime")
        .join("loom-daemon.exe")
}

fn daemon_root_sibling_path_for_exe(current_exe: &Path) -> PathBuf {
    current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("loom-daemon.exe")
}

#[cfg(debug_assertions)]
fn development_repo_root() -> Option<PathBuf> {
    Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".."),
    )
}

#[cfg(not(debug_assertions))]
fn development_repo_root() -> Option<PathBuf> {
    None
}

fn read_daemon_snapshot(base_url: &str) -> Result<DaemonSnapshot, String> {
    let health = http_get_json(base_url, "/health")?;
    let status = http_get_json(base_url, "/status")?;
    let mut degraded_errors = Vec::new();
    let capabilities = read_optional_daemon_array(
        base_url,
        "/v1/capabilities",
        "capabilities",
        &mut degraded_errors,
    );
    let mcp_servers =
        read_optional_daemon_array(base_url, "/v1/mcp/servers", "servers", &mut degraded_errors);
    let tools = read_optional_daemon_array(base_url, "/v1/tools", "tools", &mut degraded_errors);
    let python_arts =
        read_optional_daemon_array(base_url, "/v1/python-arts", "arts", &mut degraded_errors);
    let workflows =
        read_optional_daemon_array(base_url, "/v1/workflows", "workflows", &mut degraded_errors);
    let hook_bridge =
        read_optional_daemon_json(base_url, "/v1/hook-bridge/status", &mut degraded_errors);

    if !degraded_errors.is_empty() {
        http_get_json(base_url, "/health")
            .map_err(|error| format!("Loom 本地服务在读取模块状态期间离线：{error}"))?;
    }

    Ok(DaemonSnapshot {
        health,
        status,
        capabilities,
        mcp_servers,
        tools,
        python_arts,
        workflows,
        hook_bridge,
        degraded_errors,
    })
}

fn read_optional_daemon_array(
    base_url: &str,
    path: &str,
    key: &str,
    degraded_errors: &mut Vec<String>,
) -> Vec<Value> {
    let Some(response) = read_optional_daemon_json(base_url, path, degraded_errors) else {
        return Vec::new();
    };
    let Some(values) = response.get(key).and_then(Value::as_array) else {
        degraded_errors.push(format!("{path} 返回的模块数据无效：`{key}` 必须是数组"));
        return Vec::new();
    };
    values.clone()
}

fn read_optional_daemon_json(
    base_url: &str,
    path: &str,
    degraded_errors: &mut Vec<String>,
) -> Option<Value> {
    match http_get_json(base_url, path) {
        Ok(response) => Some(response),
        Err(error) => {
            degraded_errors.push(error);
            None
        }
    }
}

fn snapshot_error(errors: &[String], warning: Option<&str>) -> Option<String> {
    let mut messages = Vec::new();
    if !errors.is_empty() {
        messages.push(format!(
            "Loom 本地服务在线，但部分模块暂不可用：{}",
            errors.join("；")
        ));
    }
    if let Some(warning) = warning.filter(|value| !value.trim().is_empty()) {
        messages.push(warning.to_owned());
    }
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("；"))
    }
}

fn http_get_json(base_url: &str, path: &str) -> Result<Value, String> {
    http_request_json_with_timeout(base_url, "GET", path, None, daemon_get_timeout(path))
}

fn daemon_get_timeout(path: &str) -> Duration {
    if path == "/v1/mcp/registry"
        || path.starts_with("/v1/mcp/registry?")
        || path == "/v1/artloom-compat/mcp/registry"
        || path.starts_with("/v1/artloom-compat/mcp/registry?")
    {
        LOOM_MCP_REGISTRY_REQUEST_TIMEOUT
    } else {
        LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT
    }
}

fn http_post_json(base_url: &str, path: &str, body: &Value) -> Result<Value, String> {
    http_request_json(base_url, "POST", path, Some(body))
}

fn http_post_json_with_timeout(
    base_url: &str,
    path: &str,
    body: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    http_request_json_with_timeout(base_url, "POST", path, Some(body), timeout)
}

fn http_put_json(base_url: &str, path: &str, body: &Value) -> Result<Value, String> {
    http_request_json(base_url, "PUT", path, Some(body))
}

fn http_delete_json(base_url: &str, path: &str) -> Result<Value, String> {
    http_request_json(base_url, "DELETE", path, None)
}

fn daemon_error_message(body: &str) -> Option<String> {
    let payload = serde_json::from_str::<Value>(body).ok()?;
    payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| payload.get("message").and_then(Value::as_str))
        .or_else(|| payload.get("detail").and_then(Value::as_str))
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToOwned::to_owned)
}

fn http_request_json(
    base_url: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    http_request_json_with_timeout(
        base_url,
        method,
        path,
        body,
        LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT,
    )
}

fn http_request_json_with_timeout(
    base_url: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, String> {
    let (host, port) = parse_loopback_http_url(base_url)?;
    let mut stream = TcpStream::connect_timeout(
        &loopback_socket_addr(&host, port)?,
        LOOM_DAEMON_CONNECT_TIMEOUT,
    )
    .map_err(|error| format!("无法连接 Loom 本地服务 {base_url}：{error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("无法设置 Loom 本地服务读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("无法设置 Loom 本地服务写入超时：{error}"))?;

    let request = if let Some(body) = body {
        let body = serde_json::to_string(body)
            .map_err(|error| format!("无法序列化 Loom 本地服务请求 {path}：{error}"))?;
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        )
    };
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("无法写入 Loom 本地服务请求 {path}：{error}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("无法读取 Loom 本地服务响应 {path}：{error}"))?;

    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("Loom 本地服务响应格式异常：{path}"))?;
    let status_line = headers.lines().next().unwrap_or("unknown status");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| format!("Loom 本地服务响应状态异常：{path} returned {status_line}"))?;
    if !(200..=299).contains(&status_code) {
        if let Some(message) = daemon_error_message(body) {
            return Err(format!("{path} returned {status_line}: {message}"));
        }
        return Err(format!("{path} returned {status_line}"));
    }

    if body.trim().is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_str(body)
        .map_err(|error| format!("无法解析 Loom 本地服务响应 {path}：{error}"))
}

fn install_packaged_framework_from_exe(
    base_url: &str,
    id: &str,
    current_exe: &Path,
) -> Result<Value, String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 256 {
        return Err("框架 ID 无效。".to_string());
    }

    let install_path = format!("/v1/frameworks/{}/install", percent_encode_path_segment(id));
    let install_timeout = Duration::from_secs(60);
    let original_error = match http_post_json_with_timeout(
        base_url,
        &install_path,
        &serde_json::json!({}),
        install_timeout,
    ) {
        Ok(response) => return Ok(response),
        Err(error) => error,
    };

    if !framework_source_is_missing(&original_error) || !OFFICIAL_FRAMEWORK_IDS.contains(&id) {
        return Err(original_error);
    }

    let candidates = packaged_framework_package_candidates(current_exe, id);
    let package_path = candidates
        .iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            let searched = candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("; ");
            format!("{original_error}；当前 Loom 包内未找到框架安装包：{searched}")
        })?;
    let package = read_verified_framework_package(id, package_path)?;
    let response = http_post_json_with_timeout(
        base_url,
        "/v1/frameworks/install",
        &serde_json::json!({ "zipBase64": base64_encode(&package) }),
        install_timeout,
    )?;
    Ok(response)
}

fn upgrade_packaged_framework_from_exe(
    base_url: &str,
    id: &str,
    current_exe: &Path,
) -> Result<Value, String> {
    let package_path = packaged_framework_package_candidates(current_exe, id)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("当前 Loom 包内未找到框架升级包：{id}"))?;
    let package = read_verified_framework_package(id, &package_path)?;
    http_post_json_with_timeout(
        base_url,
        &format!("/v1/frameworks/{}/upgrade", percent_encode_path_segment(id)),
        &serde_json::json!({ "zipBase64": base64_encode(&package) }),
        Duration::from_secs(120),
    )
}

fn packaged_framework_version(current_exe: &Path, id: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(FRAMEWORK_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join("summary.json"));
    }
    if let Some(parent) = current_exe.parent() {
        candidates.push(
            parent
                .join("packages")
                .join("frameworks")
                .join("summary.json"),
        );
    }
    candidates.into_iter().find_map(|path| {
        let bytes = fs::read(path).ok()?;
        let catalog: PackagedFrameworkCatalog = serde_json::from_slice(&bytes).ok()?;
        catalog
            .frameworks
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.version)
    })
}

fn framework_source_is_missing(error: &str) -> bool {
    error.contains("no configured runtime download source")
        || error.contains("no available package source")
}

fn packaged_framework_package_candidates(current_exe: &Path, id: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(FRAMEWORK_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join(format!("{id}.zip")));
    }
    let packaged = packaged_framework_package_path(current_exe, id);
    if !candidates.iter().any(|candidate| candidate == &packaged) {
        candidates.push(packaged);
    }
    candidates
}

fn packaged_framework_package_path(current_exe: &Path, id: &str) -> PathBuf {
    current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("packages")
        .join("frameworks")
        .join(format!("{id}.zip"))
}

fn read_verified_framework_package(id: &str, package_path: &Path) -> Result<Vec<u8>, String> {
    read_verified_package("框架", id, package_path, FRAMEWORK_PACKAGE_MAX_BYTES)
}

fn read_verified_art_package(id: &str, package_path: &Path) -> Result<Vec<u8>, String> {
    read_verified_package("Art", id, package_path, ART_PACKAGE_MAX_BYTES)
}

fn read_verified_package(
    kind: &str,
    id: &str,
    package_path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(package_path).map_err(|error| {
        format!(
            "无法读取{kind} `{id}` 安装包 `{}`：{error}",
            package_path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "{kind} `{id}` 安装包不是文件：{}",
            package_path.display()
        ));
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "{kind} `{id}` 安装包超过 {max_bytes} 字节限制：{}",
            package_path.display()
        ));
    }

    let package = fs::read(package_path).map_err(|error| {
        format!(
            "无法读取{kind} `{id}` 安装包 `{}`：{error}",
            package_path.display()
        )
    })?;
    let checksum_path = package_path.with_extension("zip.sha256");
    let checksum = fs::read_to_string(&checksum_path).map_err(|error| {
        format!(
            "无法读取{kind} `{id}` 校验文件 `{}`：{error}",
            checksum_path.display()
        )
    })?;
    let mut fields = checksum.split_whitespace();
    let expected_hash = fields
        .next()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let expected_name = fields.next();
    let package_name = package_path.file_name().and_then(|name| name.to_str());
    if expected_hash.is_none() || expected_name != package_name || fields.next().is_some() {
        return Err(format!(
            "{kind} `{id}` 校验文件格式无效：{}",
            checksum_path.display()
        ));
    }
    let actual_hash = format!("{:x}", Sha256::digest(&package));
    if !actual_hash.eq_ignore_ascii_case(expected_hash.expect("validated checksum hash")) {
        return Err(format!(
            "{kind} `{id}` 安装包 SHA-256 不匹配：{}",
            package_path.display()
        ));
    }
    Ok(package)
}

fn packaged_art_catalog_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(ART_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join("summary.json"));
    }
    let packaged = current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("packages")
        .join("arts")
        .join("summary.json");
    if !candidates.iter().any(|candidate| candidate == &packaged) {
        candidates.push(packaged);
    }
    candidates
}

fn packaged_art_sha256_allowlist(current_exe: &Path) -> Result<Vec<String>, String> {
    let Some(catalog_path) = packaged_art_catalog_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
    else {
        return Ok(Vec::new());
    };
    let catalog_bytes = fs::read(&catalog_path).map_err(|error| {
        format!(
            "无法读取打包 Art 目录 `{}`：{error}",
            catalog_path.display()
        )
    })?;
    let catalog: PackagedArtCatalog = serde_json::from_slice(&catalog_bytes).map_err(|error| {
        format!(
            "无法解析打包 Art 目录 `{}`：{error}",
            catalog_path.display()
        )
    })?;
    if catalog.packages.is_empty() {
        return Err(format!("打包 Art 目录为空：{}", catalog_path.display()));
    }

    let mut art_ids = BTreeSet::new();
    let mut hashes = Vec::with_capacity(catalog.packages.len());
    for entry in &catalog.packages {
        validate_packaged_art_entry(entry)?;
        if !art_ids.insert(entry.id.as_str()) {
            return Err(format!("打包 Art 目录包含重复 ID：{}", entry.id));
        }
        hashes.push(entry.sha256.to_ascii_lowercase());
    }
    Ok(hashes)
}

fn desktop_control_plane_root() -> PathBuf {
    std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join("Loom").join("control-plane"))
        })
        .unwrap_or_else(|| PathBuf::from(".runtime").join("loom").join("control-plane"))
}

fn validate_packaged_art_entry(entry: &PackagedArtCatalogEntry) -> Result<(), String> {
    for (kind, value) in [
        ("Art", entry.id.as_str()),
        ("框架", entry.framework.as_str()),
    ] {
        if value.is_empty()
            || value.len() > 256
            || value.contains("..")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(format!("打包{kind} ID 无效：{value}"));
        }
    }
    if entry.zip != format!("{}.zip", entry.id) {
        return Err(format!("打包 Art `{}` 的 ZIP 文件名无效。", entry.id));
    }
    if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("打包 Art `{}` 的 SHA-256 无效。", entry.id));
    }
    Ok(())
}

fn bootstrap_packaged_arts_from_exe(
    base_url: &str,
    current_exe: &Path,
    control_plane_root: &Path,
) -> Result<PackagedArtBootstrapResult, String> {
    let _bootstrap_guard = PACKAGED_ART_BOOTSTRAP_LOCK
        .lock()
        .map_err(|_| "打包 Art 初始化锁已损坏。".to_string())?;
    let Some(catalog_path) = packaged_art_catalog_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
    else {
        return Ok(PackagedArtBootstrapResult {
            available: false,
            applied: false,
            catalog_hash: None,
            framework_ids: Vec::new(),
            art_ids: Vec::new(),
        });
    };
    let catalog_bytes = fs::read(&catalog_path).map_err(|error| {
        format!(
            "无法读取打包 Art 目录 `{}`：{error}",
            catalog_path.display()
        )
    })?;
    let catalog_hash = format!("{:x}", Sha256::digest(&catalog_bytes));
    let catalog: PackagedArtCatalog = serde_json::from_slice(&catalog_bytes).map_err(|error| {
        format!(
            "无法解析打包 Art 目录 `{}`：{error}",
            catalog_path.display()
        )
    })?;
    if catalog.packages.is_empty() {
        return Err(format!("打包 Art 目录为空：{}", catalog_path.display()));
    }

    let mut art_ids = Vec::new();
    let mut framework_ids = Vec::new();
    for entry in &catalog.packages {
        validate_packaged_art_entry(entry)?;
        if art_ids.contains(&entry.id) {
            return Err(format!("打包 Art 目录包含重复 ID：{}", entry.id));
        }
        art_ids.push(entry.id.clone());
        if !framework_ids.contains(&entry.framework) {
            framework_ids.push(entry.framework.clone());
        }
    }

    let marker_path = control_plane_root
        .join("migrations")
        .join("packaged-arts.sha256");
    let catalog_already_applied = fs::read_to_string(&marker_path)
        .ok()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(&catalog_hash));

    let framework_response = http_get_json(base_url, "/v1/frameworks")?;
    let framework_statuses = framework_response
        .get("frameworks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Loom 本地服务没有返回框架数组。".to_string())?;
    let mut framework_changed = false;
    for framework_id in &framework_ids {
        let official_qualified_id = format!("neuro.official/{framework_id}");
        let status = framework_statuses.iter().find(|status| {
            status.get("id").and_then(Value::as_str) == Some(framework_id.as_str())
                || status.get("qualifiedId").and_then(Value::as_str)
                    == Some(official_qualified_id.as_str())
        });
        let installed = status
            .and_then(|value| value.get("installed"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let enabled = status
            .and_then(|value| value.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ready = status
            .and_then(|value| value.get("ready"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let installed_version = status
            .and_then(|value| value.get("version"))
            .and_then(Value::as_str);
        let bundled_version = packaged_framework_version(current_exe, framework_id);
        let needs_upgrade = installed
            && ready
            && bundled_version
                .as_deref()
                .is_some_and(|version| installed_version != Some(version));
        if needs_upgrade {
            upgrade_packaged_framework_from_exe(base_url, framework_id, current_exe)?;
            framework_changed = true;
        } else if !installed || (enabled && !ready) {
            install_packaged_framework_from_exe(base_url, framework_id, current_exe)?;
            framework_changed = true;
        } else if !enabled {
            http_post_json_with_timeout(
                base_url,
                &format!(
                    "/v1/frameworks/{}/enable",
                    percent_encode_path_segment(framework_id)
                ),
                &serde_json::json!({}),
                Duration::from_secs(60),
            )?;
        }
    }

    // A framework upgrade can invalidate the version lock captured by an
    // already-installed Art package. Reinstall the bundled Art packages in
    // that case so their dependency lock points at the active framework.
    let should_install_arts = !catalog_already_applied || framework_changed;
    if !should_install_arts {
        return Ok(PackagedArtBootstrapResult {
            available: true,
            applied: false,
            catalog_hash: Some(catalog_hash),
            framework_ids,
            art_ids,
        });
    }

    let catalog_root = catalog_path.parent().unwrap_or_else(|| Path::new("."));
    for entry in &catalog.packages {
        let package_path = catalog_root.join(&entry.zip);
        let package = read_verified_art_package(&entry.id, &package_path)?;
        let actual_hash = format!("{:x}", Sha256::digest(&package));
        if !actual_hash.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!("打包 Art `{}` 的目录哈希不匹配。", entry.id));
        }
        http_post_json_with_timeout(
            base_url,
            "/v1/arts/install",
            &serde_json::json!({
                "zipBase64": base64_encode(&package),
                "bundledCatalog": true,
            }),
            Duration::from_secs(120),
        )?;
    }

    if let Some(parent) = marker_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("无法创建打包 Art 迁移目录 `{}`：{error}", parent.display())
        })?;
    }
    fs::write(&marker_path, format!("{catalog_hash}\n")).map_err(|error| {
        format!(
            "无法写入打包 Art 迁移标记 `{}`：{error}",
            marker_path.display()
        )
    })?;

    Ok(PackagedArtBootstrapResult {
        available: true,
        applied: true,
        catalog_hash: Some(catalog_hash),
        framework_ids,
        art_ids,
    })
}

fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

// Fetch a binary daemon response (e.g. a preview image) over the native HTTP
// client. Unlike `http_request_json`, this reads raw bytes so image payloads are
// not corrupted by UTF-8 decoding, and it returns the Content-Type so the caller
// can build a correct `data:` URL.
fn http_get_binary(base_url: &str, path: &str) -> Result<(String, Vec<u8>), String> {
    let (host, port) = parse_loopback_http_url(base_url)?;
    let mut stream = TcpStream::connect_timeout(
        &loopback_socket_addr(&host, port)?,
        LOOM_DAEMON_CONNECT_TIMEOUT,
    )
    .map_err(|error| format!("无法连接 Loom 本地服务 {base_url}：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("无法设置 Loom 本地服务读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("无法设置 Loom 本地服务写入超时：{error}"))?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: image/*\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("无法写入 Loom 本地服务请求 {path}：{error}"))?;

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|error| format!("无法读取 Loom 本地服务响应 {path}：{error}"))?;

    let separator = b"\r\n\r\n";
    let header_end = raw
        .windows(separator.len())
        .position(|window| window == separator)
        .ok_or_else(|| format!("Loom 本地服务响应格式异常：{path}"))?;
    let headers = String::from_utf8_lossy(&raw[..header_end]).into_owned();
    let body = raw[header_end + separator.len()..].to_vec();

    if !headers.starts_with("HTTP/1.1 200") {
        let status_line = headers.lines().next().unwrap_or("unknown status");
        return Err(format!("{path} returned {status_line}"));
    }

    let content_type = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-type") {
                Some(value.trim().to_owned())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "application/octet-stream".to_owned());

    Ok((content_type, body))
}

// Minimal standard base64 encoder so the desktop wrapper can return `data:` URLs
// without adding a dependency.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn normalize_daemon_path(path: String) -> Result<String, String> {
    let path = path.trim();
    if !path.starts_with('/') || path.contains('\r') || path.contains('\n') || path.contains("..") {
        return Err("Loom 本地服务 API 路径必须是绝对本地路径。".to_string());
    }
    Ok(path.to_string())
}

fn parse_loopback_http_url(base_url: &str) -> Result<(String, u16), String> {
    let without_scheme = base_url
        .strip_prefix("http://")
        .ok_or_else(|| "Loom 本地服务地址必须使用 http:// 回环地址。".to_string())?;
    let authority = without_scheme
        .split('/')
        .next()
        .ok_or_else(|| "Loom 本地服务地址缺少主机名。".to_string())?;
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| "Loom 本地服务地址必须包含端口。".to_string())?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err("Loom 桌面端只连接回环地址上的本地服务。".to_string());
    }
    let port = port
        .parse::<u16>()
        .map_err(|error| format!("Loom 本地服务端口无效：{error}"))?;
    Ok((host.trim_matches(&['[', ']'][..]).to_string(), port))
}

fn loopback_socket_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let ip = match host {
        "127.0.0.1" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::1" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => return Err("Loom 桌面端只连接回环地址上的本地服务。".to_string()),
    };
    Ok(SocketAddr::new(ip, port))
}

#[cfg(target_os = "windows")]
fn configured_webview2_browser_args() -> Option<String> {
    let port = std::env::var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV)
        .ok()?
        .trim()
        .parse::<u16>()
        .ok()?;
    if port == 0 {
        return None;
    }
    Some(format!(
        "{DEFAULT_WEBVIEW2_BROWSER_ARGS} --remote-debugging-port={port} --remote-debugging-address=127.0.0.1"
    ))
}

#[cfg(target_os = "windows")]
fn configure_webview2_browser_args(context: &mut tauri::Context<tauri::Wry>) {
    let Some(arguments) = configured_webview2_browser_args() else {
        return;
    };
    for window in &mut context.config_mut().app.windows {
        window.additional_browser_args = Some(arguments.clone());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    LOOM_EXITING.store(false, Ordering::Release);
    let mut context = tauri::generate_context!();
    #[cfg(target_os = "windows")]
    configure_webview2_browser_args(&mut context);

    let run_result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let general =
                read_loom_persisted_general_settings().unwrap_or(LoomGeneralRuntimeSettings {
                    minimize_to_tray: true,
                });
            LOOM_CLOSE_TO_TRAY.store(general.minimize_to_tray, Ordering::Relaxed);
            let settings = read_loom_persisted_cache_settings().unwrap_or_default();
            std::thread::spawn(move || {
                let _ = apply_loom_cache_preferences(&settings);
            });
            std::thread::spawn(|| {
                if let Err(error) = start_loom_daemon_blocking() {
                    eprintln!("[ERROR] Loom 本地服务启动失败：{error}");
                }
            });
            if let Some(window) = app.get_webview_window("main") {
                let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
                window.set_icon(icon)?;
            }
            let show_item = MenuItem::with_id(app, "show", "显示 Loom", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray_builder = TrayIconBuilder::with_id("loom")
                .menu(&tray_menu)
                .tooltip("Loom")
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        begin_desktop_exit();
                        app.exit(0);
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            app.manage(tray_builder.build(app)?);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if LOOM_CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    begin_desktop_exit();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            resolve_loom_daemon_url,
            resolve_application_diagnostics,
            open_application_log_location,
            open_external_url,
            open_mcp_source_url,
            apply_loom_general_settings,
            get_loom_cache_snapshot,
            apply_loom_cache_settings,
            clear_loom_cache,
            get_hook_cache_snapshot,
            wait_for_hook_cache_settings,
            clear_hook_cache,
            read_loom_snapshot,
            start_loom_daemon,
            get_loom_daemon_json,
            put_loom_daemon_json,
            delete_loom_daemon_json,
            post_loom_daemon_json,
            install_packaged_framework,
            bootstrap_packaged_arts,
            read_hook_canvas_preview
        ])
        .run(context);
    begin_desktop_exit();
    run_result.expect("error while running Loom desktop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn owned_daemon_sleep_fixture() {
        if std::env::var("LOOM_DESKTOP_OWNED_DAEMON_FIXTURE")
            .ok()
            .as_deref()
            == Some("1")
        {
            std::thread::sleep(Duration::from_secs(30));
        }
    }

    #[test]
    fn desktop_exit_terminates_and_reaps_the_owned_daemon_process() {
        let _guard = ENV_LOCK.lock().expect("environment lock");
        LOOM_EXITING.store(false, Ordering::Release);
        stop_owned_daemon_process();
        let child =
            std::process::Command::new(std::env::current_exe().expect("desktop test executable"))
                .args([
                    "tests::owned_daemon_sleep_fixture",
                    "--exact",
                    "--nocapture",
                ])
                .env("LOOM_DESKTOP_OWNED_DAEMON_FIXTURE", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn owned daemon fixture");
        let pid = child.id();

        register_owned_daemon_process(child).expect("register owned daemon fixture");
        assert_eq!(owned_daemon_process_id(), Some(pid));
        assert_eq!(stop_owned_daemon_process(), Some(pid));
        assert_eq!(owned_daemon_process_id(), None);
    }

    #[test]
    fn daemon_start_finishing_after_exit_is_reaped_instead_of_registered() {
        let _guard = ENV_LOCK.lock().expect("environment lock");
        LOOM_EXITING.store(false, Ordering::Release);
        stop_owned_daemon_process();
        let child =
            std::process::Command::new(std::env::current_exe().expect("desktop test executable"))
                .args([
                    "tests::owned_daemon_sleep_fixture",
                    "--exact",
                    "--nocapture",
                ])
                .env("LOOM_DESKTOP_OWNED_DAEMON_FIXTURE", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn late daemon fixture");

        LOOM_EXITING.store(true, Ordering::Release);
        let error = register_owned_daemon_process(child).expect_err("exit rejects late daemon");

        assert!(error.contains("正在退出"));
        assert_eq!(owned_daemon_process_id(), None);
        LOOM_EXITING.store(false, Ordering::Release);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-desktop-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn read_test_http_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("set test request timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let bytes = stream.read(&mut buffer).expect("read test request");
            if bytes == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8(request).expect("test request is utf8")
    }

    fn write_test_json_response(stream: &mut TcpStream, status: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write test response");
    }

    #[test]
    fn default_runtime_config_points_at_loopback_loom_daemon() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_daemon_url = std::env::var("LOOM_DAEMON_URL").ok();
        let previous_bridge_url = std::env::var("LOOM_HOOK_BRIDGE_URL").ok();
        let previous_bridge_port = std::env::var("LOOM_HOOK_BRIDGE_PORT").ok();
        std::env::remove_var("LOOM_DAEMON_URL");
        std::env::remove_var("LOOM_HOOK_BRIDGE_URL");
        std::env::remove_var("LOOM_HOOK_BRIDGE_PORT");

        let config = resolve_loom_daemon_url();

        assert_eq!(config.loom_daemon_url, DEFAULT_LOOM_DAEMON_URL);
        assert_eq!(config.settings_url, "http://127.0.0.1:8765/settings");
        assert_eq!(config.hook_bridge_url, DEFAULT_HOOK_BRIDGE_URL);
        restore_env("LOOM_DAEMON_URL", previous_daemon_url);
        restore_env("LOOM_HOOK_BRIDGE_URL", previous_bridge_url);
        restore_env("LOOM_HOOK_BRIDGE_PORT", previous_bridge_port);
    }

    #[test]
    fn runtime_config_accepts_an_isolated_hook_bridge_port() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_bridge_url = std::env::var("LOOM_HOOK_BRIDGE_URL").ok();
        let previous_bridge_port = std::env::var("LOOM_HOOK_BRIDGE_PORT").ok();
        std::env::remove_var("LOOM_HOOK_BRIDGE_URL");
        std::env::set_var("LOOM_HOOK_BRIDGE_PORT", "43127");

        let config = resolve_loom_daemon_url();

        assert_eq!(config.hook_bridge_url, "ws://127.0.0.1:43127");
        restore_env("LOOM_HOOK_BRIDGE_URL", previous_bridge_url);
        restore_env("LOOM_HOOK_BRIDGE_PORT", previous_bridge_port);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn webview2_remote_debugging_port_builds_explicit_browser_arguments() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_port = std::env::var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV).ok();
        std::env::set_var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, "43129");

        let arguments = configured_webview2_browser_args().expect("browser arguments");

        assert!(arguments.contains("--remote-debugging-port=43129"));
        assert!(arguments.contains("--remote-debugging-address=127.0.0.1"));
        assert!(arguments.contains("msSmartScreenProtection"));
        assert!(arguments.contains("--autoplay-policy=no-user-gesture-required"));
        restore_env(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, previous_port);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn webview2_remote_debugging_ignores_invalid_ports() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_port = std::env::var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV).ok();
        std::env::set_var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, "invalid");

        assert_eq!(configured_webview2_browser_args(), None);
        restore_env(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, previous_port);
    }

    fn restore_env(name: &str, value: Option<String>) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    #[test]
    fn application_diagnostics_expose_build_repositories_and_six_character_commits() {
        for (app, repository) in [
            ("loom", "https://github.com/aiaimimi0920/Loom"),
            ("hook", "https://github.com/aiaimimi0920/Hook"),
        ] {
            let diagnostics = application_diagnostics(app).expect("application diagnostics");
            assert_eq!(diagnostics.repository_url.as_deref(), Some(repository));
            let commit = diagnostics.commit_short.expect("embedded commit");
            assert_eq!(commit.len(), 6);
            assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(is_allowed_repository_url(repository));
        }
        assert!(!is_allowed_repository_url("https://example.com/untrusted"));
    }

    #[test]
    fn mcp_source_urls_allow_safe_https_and_reject_unsafe_targets() {
        assert!(is_safe_external_https_url(
            "https://github.com/modelcontextprotocol/servers"
        ));
        assert!(is_safe_external_https_url(
            "https://registry.modelcontextprotocol.io/v0.1/servers"
        ));
        assert!(!is_safe_external_https_url("http://example.com/mcp"));
        assert!(!is_safe_external_https_url(
            "https://user:secret@example.com/mcp"
        ));
        assert!(!is_safe_external_https_url("javascript:alert(1)"));
        assert!(!is_safe_external_https_url("not-a-url"));
    }

    #[test]
    fn mcp_registry_gets_a_timeout_longer_than_the_daemon_outbound_fetch() {
        assert_eq!(
            daemon_get_timeout("/v1/mcp/registry?limit=100&cursor=opaque"),
            LOOM_MCP_REGISTRY_REQUEST_TIMEOUT
        );
        assert_eq!(
            daemon_get_timeout("/v1/artloom-compat/mcp/registry"),
            LOOM_MCP_REGISTRY_REQUEST_TIMEOUT
        );
        assert_eq!(daemon_get_timeout("/health"), LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT);
        assert!(LOOM_MCP_REGISTRY_REQUEST_TIMEOUT > Duration::from_secs(40));
    }

    #[test]
    fn daemon_commands_prefer_the_active_owned_daemon_over_a_stale_frontend_url() {
        assert_eq!(
            resolve_command_base_url_with_active(
                DEFAULT_LOOM_DAEMON_URL.to_owned(),
                Some("http://127.0.0.1:49321/".to_owned()),
            ),
            "http://127.0.0.1:49321"
        );
        assert_eq!(
            resolve_command_base_url_with_active(
                "http://127.0.0.1:18765/".to_owned(),
                None,
            ),
            "http://127.0.0.1:18765"
        );
    }

    #[test]
    fn loom_general_settings_restore_and_apply_close_to_tray() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("general-settings");
        let settings_dir = root.join("settings");
        fs::create_dir_all(&settings_dir).expect("settings dir");
        fs::write(
            settings_dir.join("artloom-compat-settings.json"),
            br#"{"general":{"minimize_to_tray":false}}"#,
        )
        .expect("settings file");
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);

        assert_eq!(
            read_loom_persisted_general_settings(),
            Some(LoomGeneralRuntimeSettings {
                minimize_to_tray: false,
            })
        );
        apply_loom_general_settings(LoomGeneralRuntimeSettings {
            minimize_to_tray: false,
        });
        assert!(!LOOM_CLOSE_TO_TRAY.load(Ordering::Relaxed));

        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
        LOOM_CLOSE_TO_TRAY.store(true, Ordering::Relaxed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hook_cache_snapshot_and_clear_only_manage_hook_temporary_files() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("hook-cache");
        let clipboard = root.join("clipboard");
        let app_data = root.join("app-data");
        let image_search = app_data.join("image-search-cache");
        fs::create_dir_all(&clipboard).expect("create clipboard cache");
        fs::create_dir_all(&image_search).expect("create image search cache");
        fs::write(clipboard.join("capture.png"), vec![1_u8; 12]).expect("write clipboard cache");
        fs::write(image_search.join("remote_test.png"), vec![2_u8; 20])
            .expect("write image search cache");
        fs::write(
            app_data.join("session.json"),
            br#"{"recycleBin":[{},{}],"referenceLibrary":[{}]}"#,
        )
        .expect("write session");
        fs::write(
            app_data.join("app-settings.json"),
            br#"{"cache":{"recycleBinMaxEntries":50,"recycleBinRetentionDays":30,"tempCacheMaxBytes":0,"tempCacheRetentionDays":0}}"#,
        )
        .expect("write app settings");
        let previous_clipboard = std::env::var("HOOK_CLIPBOARD_CACHE_DIR").ok();
        let previous_app_data = std::env::var("HOOK_APPDATA_DIR").ok();
        std::env::set_var("HOOK_CLIPBOARD_CACHE_DIR", &clipboard);
        std::env::set_var("HOOK_APPDATA_DIR", &app_data);

        let before = hook_cache_snapshot().expect("cache snapshot");
        assert_eq!(
            read_hook_persisted_cache_settings(),
            Some(HookCachePreferences {
                recycle_bin_max_entries: 50,
                recycle_bin_retention_days: 30,
                temp_cache_max_bytes: 0,
                temp_cache_retention_days: 0,
            })
        );
        assert_eq!(before.temporary.bytes, 12);
        assert_eq!(before.recycle_bin_entries, 2);
        assert_eq!(before.reference_entries, 1);
        let cleared = clear_hook_cache("temporary".to_owned()).expect("clear temporary cache");
        assert_eq!(cleared.freed_bytes, 12);
        assert_eq!(cleared.snapshot.temporary.bytes, 0);
        assert!(image_search.join("remote_test.png").is_file());

        restore_env("HOOK_CLIPBOARD_CACHE_DIR", previous_clipboard);
        restore_env("HOOK_APPDATA_DIR", previous_app_data);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn loom_cache_snapshot_and_clear_preserve_installed_art_and_workflow_data() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = unique_temp_dir("loom-cache");
        let control_plane = root.join("control-plane");
        let art_version = control_plane
            .join("arts")
            .join("sample-art")
            .join("versions")
            .join("0.1.0");
        let art_cache = art_version.join(".loom-cache");
        let workflow = control_plane.join("workflows").join("saved.yaml");
        let framework_temp = root.join("framework-temporary");
        fs::create_dir_all(&art_cache).expect("create Art cache");
        fs::create_dir_all(workflow.parent().expect("workflow parent"))
            .expect("create workflow directory");
        fs::create_dir_all(&framework_temp).expect("create framework temporary directory");
        fs::write(art_version.join("manifest.json"), b"installed-art").expect("write Art manifest");
        fs::write(art_cache.join("runtime.bin"), vec![1_u8; 12]).expect("write Art cache");
        fs::write(&workflow, b"workflow").expect("write workflow");
        fs::write(framework_temp.join("request.bin"), vec![2_u8; 20])
            .expect("write framework temporary file");
        let settings_path = control_plane
            .join("settings")
            .join("artloom-compat-settings.json");
        fs::create_dir_all(settings_path.parent().expect("settings parent"))
            .expect("create settings directory");
        fs::write(
            &settings_path,
            br#"{"loom_cache":{"art_cache_max_bytes":0,"art_cache_retention_days":0,"framework_temp_retention_days":0}}"#,
        )
        .expect("write Loom cache settings");

        let previous_control_plane = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let previous_framework_temp = std::env::var("LOOM_FRAMEWORK_TEMP_DIR").ok();
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &control_plane);
        std::env::set_var("LOOM_FRAMEWORK_TEMP_DIR", &framework_temp);

        assert_eq!(
            read_loom_persisted_cache_settings(),
            Some(LoomCachePreferences {
                art_cache_max_bytes: 0,
                art_cache_retention_days: 0,
                framework_temp_retention_days: 0,
            })
        );
        let before = loom_cache_snapshot().expect("read Loom cache snapshot");
        assert_eq!(before.art_runtime.bytes, 12);
        assert_eq!(before.framework_temporary.bytes, 20);

        let cleared_art = clear_loom_cache_blocking("artRuntime").expect("clear Art cache");
        assert_eq!(cleared_art.freed_bytes, 12);
        assert!(art_version.join("manifest.json").is_file());
        assert!(workflow.is_file());
        assert!(framework_temp.join("request.bin").is_file());

        let cleared_temp = clear_loom_cache_blocking("frameworkTemporary")
            .expect("clear framework temporary files");
        assert_eq!(cleared_temp.freed_bytes, 20);
        assert!(art_version.join("manifest.json").is_file());
        assert!(workflow.is_file());

        fs::write(art_cache.join("old.bin"), vec![3_u8; 4]).expect("write first cache file");
        fs::write(art_cache.join("new.bin"), vec![4_u8; 4]).expect("write second cache file");
        prune_cache_roots(std::slice::from_ref(&art_cache), 5, 0).expect("enforce Art cache limit");
        assert!(directory_usage(&art_cache).expect("read pruned cache").0 <= 5);
        assert!(art_version.join("manifest.json").is_file());

        restore_env("LOOM_CONTROL_PLANE_ROOT", previous_control_plane);
        restore_env("LOOM_FRAMEWORK_TEMP_DIR", previous_framework_temp);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn offline_snapshot_preserves_settings_links() {
        let snapshot = read_loom_snapshot_blocking(Some("http://127.0.0.1:9".to_string()));

        assert_eq!(snapshot.base_url, "http://127.0.0.1:9");
        assert_eq!(snapshot.connection_state, "offline");
        assert_eq!(snapshot.settings.root, "http://127.0.0.1:9/settings");
        assert_eq!(snapshot.settings.tea, "http://127.0.0.1:9/settings/tea");
        assert!(snapshot.mcp_servers.is_empty());
        assert!(snapshot.tools.is_empty());
        assert!(snapshot.workflows.is_empty());
        assert!(snapshot.hook_bridge.is_none());
        assert!(snapshot.error.is_some());
    }

    #[test]
    fn optional_daemon_module_failure_keeps_snapshot_online() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind snapshot fixture");
        let address = listener
            .local_addr()
            .expect("read snapshot fixture address");
        listener
            .set_nonblocking(true)
            .expect("set snapshot fixture nonblocking");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("accept snapshot request: {error}"),
            };
            let mut request = Vec::new();
            let mut buffer = [0_u8; 512];
            loop {
                let bytes = stream.read(&mut buffer).expect("read snapshot request");
                if bytes == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..bytes]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request).expect("snapshot request is utf8");
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .expect("snapshot request path");
            let (status, body) = match path {
                "/health" => (200, r#"{"status":"ok"}"#),
                "/status" => (200, r#"{"status":"ready"}"#),
                "/v1/capabilities" => (200, r#"{"capabilities":[]}"#),
                "/v1/mcp/servers" => (200, r#"{"servers":[]}"#),
                "/v1/tools" => (500, r#"{"error":{"code":"tool_registry_error"}}"#),
                "/v1/python-arts" => (200, r#"{"arts":[]}"#),
                "/v1/workflows" => (200, r#"{"workflows":[]}"#),
                "/v1/hook-bridge/status" => (200, r#"{"running":false}"#),
                other => panic!("unexpected snapshot path: {other}"),
            };
            let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            stream
                .write_all(response.as_bytes())
                .expect("write snapshot response");
        });

        let snapshot =
            read_loom_snapshot_blocking(Some(format!("http://127.0.0.1:{}", address.port())));
        shutdown_tx.send(()).expect("stop snapshot fixture");
        server.join().expect("join snapshot fixture");

        assert_eq!(snapshot.connection_state, "online");
        assert_eq!(snapshot.health, Some(serde_json::json!({"status": "ok"})));
        assert_eq!(
            snapshot.status,
            Some(serde_json::json!({"status": "ready"}))
        );
        assert!(snapshot.tools.is_empty());
        assert!(snapshot
            .error
            .as_deref()
            .is_some_and(|error| error.contains("/v1/tools")));
    }

    #[test]
    fn daemon_http_error_preserves_structured_message() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind error fixture");
        let address = listener.local_addr().expect("read error fixture address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept error request");
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).expect("read error request");
            let body = r#"{"error":{"code":"framework_install_failed","message":"framework `process` has no available package source"}}"#;
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write error response");
        });

        let error = http_post_json(
            &format!("http://127.0.0.1:{}", address.port()),
            "/v1/frameworks/process/install",
            &serde_json::json!({}),
        )
        .expect_err("framework install must fail");
        server.join().expect("join error fixture");

        assert!(error.contains("HTTP/1.1 500 Internal Server Error"));
        assert!(error.contains("no available package source"));
    }

    #[test]
    fn daemon_http_client_accepts_successful_non_200_status() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind success fixture");
        let address = listener.local_addr().expect("read success fixture address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept success request");
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).expect("read success request");
            let body = r#"{"created":true}"#;
            let response = format!(
                "HTTP/1.0 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write success response");
        });

        let response = http_get_json(
            &format!("http://127.0.0.1:{}", address.port()),
            "/v1/frameworks",
        )
        .expect("201 response should be accepted");
        server.join().expect("join success fixture");

        assert_eq!(response, serde_json::json!({"created": true}));
    }

    #[test]
    fn packaged_framework_path_uses_release_catalog() {
        let desktop_exe = Path::new(r"C:\Release\Loom.exe");

        assert_eq!(
            packaged_framework_package_path(desktop_exe, "cloud_api"),
            PathBuf::from(r"C:\Release\packages\frameworks\cloud_api.zip")
        );
    }

    #[test]
    fn packaged_framework_checksum_mismatch_is_rejected() {
        let root = unique_temp_dir("framework-checksum");
        let package_path = root.join("cloud_api.zip");
        fs::write(&package_path, b"framework-package").expect("write package");
        fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{}  cloud_api.zip\n", "0".repeat(64)),
        )
        .expect("write checksum");

        let error = read_verified_framework_package("cloud_api", &package_path)
            .expect_err("mismatched checksum must fail");

        assert!(error.contains("SHA-256 不匹配"), "{error}");
        fs::remove_dir_all(root).expect("cleanup checksum fixture");
    }

    #[test]
    fn packaged_art_checksum_mismatch_is_rejected() {
        let root = unique_temp_dir("art-checksum");
        let package_path = root.join("sample-art.zip");
        fs::write(&package_path, b"art-package").expect("write package");
        fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{}  sample-art.zip\n", "0".repeat(64)),
        )
        .expect("write checksum");

        let error = read_verified_art_package("sample-art", &package_path)
            .expect_err("mismatched checksum must fail");

        assert!(error.contains("SHA-256 不匹配"), "{error}");
        fs::remove_dir_all(root).expect("cleanup checksum fixture");
    }

    #[test]
    fn packaged_art_bootstrap_installs_catalog_once() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_catalog = std::env::var(ART_PACKAGE_CATALOG_ENV).ok();
        std::env::remove_var(ART_PACKAGE_CATALOG_ENV);

        let root = unique_temp_dir("art-bootstrap");
        let desktop_exe = root.join("Loom.exe");
        let catalog_root = root.join("packages").join("arts");
        let control_plane_root = root.join("control-plane");
        fs::create_dir_all(&catalog_root).expect("create Art catalog");
        fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");

        let mut packages = Vec::new();
        let mut expected_hashes = Vec::new();
        for id in ["sample-a", "sample-b"] {
            let package = format!("independent-{id}").into_bytes();
            let hash = format!("{:x}", Sha256::digest(&package));
            expected_hashes.push(hash.clone());
            let package_path = catalog_root.join(format!("{id}.zip"));
            fs::write(&package_path, &package).expect("write Art package");
            fs::write(
                package_path.with_extension("zip.sha256"),
                format!("{hash}  {id}.zip\n"),
            )
            .expect("write Art checksum");
            packages.push(serde_json::json!({
                "id": id,
                "framework": "process",
                "zip": format!("{id}.zip"),
                "sha256": hash,
            }));
        }
        fs::write(
            catalog_root.join("summary.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "configuration": "Release",
                "packages": packages,
            }))
            .expect("serialize Art catalog"),
        )
        .expect("write Art catalog");

        assert_eq!(
            packaged_art_sha256_allowlist(&desktop_exe).expect("read packaged Art allowlist"),
            expected_hashes,
        );

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind Art bootstrap fixture");
        let address = listener
            .local_addr()
            .expect("read Art bootstrap fixture address");
        let (request_tx, request_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for (status, body) in [
                (
                    "200 OK",
                    r#"{"frameworks":[{"id":"process","qualifiedId":"neuro.official/process","installed":true,"enabled":true,"ready":true}]}"#,
                ),
                ("200 OK", r#"{"tool":{"id":"sample-a"}}"#),
                ("200 OK", r#"{"tool":{"id":"sample-b"}}"#),
                (
                    "200 OK",
                    r#"{"frameworks":[{"id":"process","qualifiedId":"neuro.official/process","installed":true,"enabled":true,"ready":true}]}"#,
                ),
            ] {
                let (mut stream, _) = listener.accept().expect("accept Art bootstrap request");
                request_tx
                    .send(read_test_http_request(&mut stream))
                    .expect("record Art bootstrap request");
                write_test_json_response(&mut stream, status, body);
            }
        });

        let base_url = format!("http://127.0.0.1:{}", address.port());
        let first = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
            .expect("bootstrap packaged Arts");

        assert!(first.available);
        assert!(first.applied);
        assert_eq!(first.framework_ids, vec!["process"]);
        assert_eq!(first.art_ids, vec!["sample-a", "sample-b"]);
        let requests = [
            request_rx.recv().expect("framework listing request"),
            request_rx.recv().expect("first Art install request"),
            request_rx.recv().expect("second Art install request"),
        ];
        assert!(requests[0].starts_with("GET /v1/frameworks HTTP/1.1"));
        assert!(requests[1].starts_with("POST /v1/arts/install HTTP/1.1"));
        assert!(requests[2].starts_with("POST /v1/arts/install HTTP/1.1"));
        assert!(requests[1].contains("\"bundledCatalog\":true"));
        assert!(requests[2].contains("\"bundledCatalog\":true"));

        let second = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
            .expect("skip previously applied catalog");
        server.join().expect("join Art bootstrap fixture");
        assert!(second.available);
        assert!(!second.applied);
        assert_eq!(second.catalog_hash, first.catalog_hash);
        assert!(request_rx
            .recv()
            .expect("second framework listing request")
            .starts_with("GET /v1/frameworks HTTP/1.1"));

        fs::remove_dir_all(root).expect("cleanup Art bootstrap fixture");
        restore_env(ART_PACKAGE_CATALOG_ENV, previous_catalog);
    }

    #[test]
    fn packaged_framework_install_falls_back_for_an_old_daemon() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_catalog = std::env::var(FRAMEWORK_PACKAGE_CATALOG_ENV).ok();
        std::env::remove_var(FRAMEWORK_PACKAGE_CATALOG_ENV);

        let root = unique_temp_dir("framework-old-daemon");
        let desktop_exe = root.join("Loom.exe");
        let catalog = root.join("packages").join("frameworks");
        fs::create_dir_all(&catalog).expect("create package catalog");
        fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");
        let package_path = catalog.join("cloud_api.zip");
        let package = b"independent-cloud-api-framework";
        fs::write(&package_path, package).expect("write framework package");
        let hash = format!("{:x}", Sha256::digest(package));
        fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{hash}  cloud_api.zip\n"),
        )
        .expect("write framework checksum");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind old daemon fixture");
        let address = listener.local_addr().expect("read old daemon address");
        let (request_tx, request_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for (status, body) in [
                (
                    "500 Internal Server Error",
                    r#"{"error":{"code":"framework_install_failed","message":"framework `cloud_api` has no configured runtime download source (set LOOM_ART_STORE_URL or LOOM_FRAMEWORK_RUNTIME_URL)"}}"#,
                ),
                (
                    "200 OK",
                    r#"{"framework":{"id":"cloud_api","installed":true,"ready":true}}"#,
                ),
            ] {
                let (mut stream, _) = listener.accept().expect("accept install request");
                request_tx
                    .send(read_test_http_request(&mut stream))
                    .expect("record install request");
                write_test_json_response(&mut stream, status, body);
            }
        });

        let response = install_packaged_framework_from_exe(
            &format!("http://127.0.0.1:{}", address.port()),
            "cloud_api",
            &desktop_exe,
        )
        .expect("packaged fallback install");
        server.join().expect("join old daemon fixture");
        let first_request = request_rx.recv().expect("first install request");
        let second_request = request_rx.recv().expect("fallback install request");

        assert!(first_request.starts_with("POST /v1/frameworks/cloud_api/install HTTP/1.1"));
        assert!(second_request.starts_with("POST /v1/frameworks/install HTTP/1.1"));
        let fallback_body: Value = serde_json::from_str(
            second_request
                .split_once("\r\n\r\n")
                .expect("fallback request body")
                .1,
        )
        .expect("fallback request json");
        assert_eq!(fallback_body["zipBase64"], base64_encode(package));
        assert_eq!(response["framework"]["installed"], true);
        assert_eq!(response["framework"]["ready"], true);

        fs::remove_dir_all(root).expect("cleanup old daemon fixture");
        restore_env(FRAMEWORK_PACKAGE_CATALOG_ENV, previous_catalog);
    }

    #[test]
    fn packaged_framework_install_does_not_mask_other_daemon_errors() {
        let root = unique_temp_dir("framework-daemon-error");
        let desktop_exe = root.join("Loom.exe");
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind daemon error fixture");
        let address = listener.local_addr().expect("read daemon error address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept failed install request");
            let _ = read_test_http_request(&mut stream);
            write_test_json_response(
                &mut stream,
                "500 Internal Server Error",
                r#"{"error":{"code":"framework_install_failed","message":"framework runtime self-test failed"}}"#,
            );
        });

        let error = install_packaged_framework_from_exe(
            &format!("http://127.0.0.1:{}", address.port()),
            "cloud_api",
            &desktop_exe,
        )
        .expect_err("non-source error must remain visible");
        server.join().expect("join daemon error fixture");

        assert!(error.contains("runtime self-test failed"), "{error}");
        fs::remove_dir_all(root).expect("cleanup daemon error fixture");
    }

    #[test]
    fn daemon_that_disappears_after_core_probes_is_offline() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind disappearing fixture");
        let address = listener
            .local_addr()
            .expect("read disappearing fixture address");
        let server = thread::spawn(move || {
            for body in [r#"{"status":"ok"}"#, r#"{"status":"ready"}"#] {
                let (mut stream, _) = listener.accept().expect("accept core probe");
                let mut request = [0_u8; 512];
                let _ = stream.read(&mut request).expect("read core probe");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write core probe response");
            }
        });

        let snapshot =
            read_loom_snapshot_blocking(Some(format!("http://127.0.0.1:{}", address.port())));
        server.join().expect("join disappearing fixture");

        assert_eq!(snapshot.connection_state, "offline");
        assert!(snapshot.error.is_some());
    }

    #[test]
    fn malformed_optional_module_contract_is_reported_as_degraded() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind malformed fixture");
        let address = listener
            .local_addr()
            .expect("read malformed fixture address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept malformed request");
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).expect("read malformed request");
            let body = "{}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write malformed response");
        });

        let mut degraded_errors = Vec::new();
        let values = read_optional_daemon_array(
            &format!("http://127.0.0.1:{}", address.port()),
            "/v1/tools",
            "tools",
            &mut degraded_errors,
        );
        server.join().expect("join malformed fixture");

        assert!(values.is_empty());
        assert!(degraded_errors
            .iter()
            .any(|error| error.contains("/v1/tools") && error.contains("tools")));
    }

    #[test]
    fn rejects_non_loopback_daemon_url() {
        let snapshot = read_loom_snapshot_blocking(Some("http://example.com:8765".to_string()));

        assert_eq!(snapshot.connection_state, "offline");
        assert_eq!(
            snapshot.error,
            Some("Loom 桌面端只连接回环地址上的本地服务。".to_string())
        );
    }

    #[test]
    fn loopback_url_parser_accepts_localhost() {
        assert_eq!(
            parse_loopback_http_url("http://localhost:8765"),
            Ok(("localhost".to_string(), 8765))
        );
    }

    #[test]
    fn daemon_sidecar_path_uses_packaged_runtime_directory() {
        let desktop_exe = std::path::Path::new(r"C:\apps\Loom\Loom.exe");

        let daemon_path = daemon_sidecar_path_for_exe(desktop_exe);

        assert_eq!(
            daemon_path,
            std::path::PathBuf::from(r"C:\apps\Loom\runtime\loom-daemon.exe")
        );
    }

    #[test]
    fn daemon_candidates_prefer_explicit_override_then_runtime_then_development_target() {
        let desktop_exe = std::path::Path::new(r"C:\apps\Loom\Loom.exe");
        let repo_root = std::path::Path::new(r"C:\src\Loom");

        let candidates = daemon_executable_candidates(
            desktop_exe,
            Some(std::path::PathBuf::from(r"D:\loom\custom-daemon.exe")),
            Some(repo_root),
        );

        assert_eq!(
            candidates,
            vec![
                std::path::PathBuf::from(r"D:\loom\custom-daemon.exe"),
                std::path::PathBuf::from(r"C:\apps\Loom\runtime\loom-daemon.exe"),
                std::path::PathBuf::from(r"C:\apps\Loom\loom-daemon.exe"),
                std::path::PathBuf::from(r"C:\src\Loom\target\debug\loom-daemon.exe"),
            ]
        );
    }

    #[test]
    fn daemon_candidates_include_root_sibling_fallback_before_development_target() {
        let desktop_exe = std::path::Path::new(r"C:\apps\Loom\loom-desktop.exe");
        let repo_root = std::path::Path::new(r"C:\src\Loom");

        let candidates = daemon_executable_candidates(desktop_exe, None, Some(repo_root));

        assert_eq!(
            candidates,
            vec![
                std::path::PathBuf::from(r"C:\apps\Loom\runtime\loom-daemon.exe"),
                std::path::PathBuf::from(r"C:\apps\Loom\loom-daemon.exe"),
                std::path::PathBuf::from(r"C:\src\Loom\target\debug\loom-daemon.exe"),
            ]
        );
    }

    #[test]
    fn blank_daemon_override_is_ignored() {
        assert_eq!(configured_daemon_executable("  "), None);
    }

    #[test]
    fn daemon_path_mismatch_warning_reports_old_running_daemon() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_override = std::env::var(LOOM_DAEMON_EXECUTABLE_ENV).ok();
        std::env::remove_var(LOOM_DAEMON_EXECUTABLE_ENV);

        let root = unique_temp_dir("daemon-path-mismatch");
        let desktop_dir = root.join("Loom");
        let runtime_dir = desktop_dir.join("runtime");
        fs::create_dir_all(&runtime_dir).expect("create runtime dir");
        let desktop_exe = desktop_dir.join("Loom.exe");
        let packaged_daemon = runtime_dir.join("loom-daemon.exe");
        fs::write(&desktop_exe, b"desktop").expect("write desktop exe placeholder");
        fs::write(&packaged_daemon, b"daemon").expect("write daemon exe placeholder");

        let warning = daemon_path_mismatch_warning(
            &desktop_exe,
            &serde_json::json!({
                "status": "ok",
                "executablePath": r"C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\older\runtime\loom-daemon.exe"
            }),
        )
        .expect("mismatch warning");

        assert!(warning.contains("旧 daemon"), "{warning}");
        assert!(warning.contains("127.0.0.1:8765"), "{warning}");
        assert!(
            warning.contains(&packaged_daemon.display().to_string()),
            "{warning}"
        );

        fs::remove_dir_all(root).expect("cleanup temp dir");
        restore_env(LOOM_DAEMON_EXECUTABLE_ENV, previous_override);
    }
}
