//! Daemon snapshot models, startup, process ownership, and shutdown.

use super::*;

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
pub(super) struct PackagedArtCatalog {
    #[serde(default)]
    pub(super) packages: Vec<PackagedArtCatalogEntry>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PackagedFrameworkCatalog {
    #[serde(default)]
    pub(super) frameworks: Vec<PackagedFrameworkCatalogEntry>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PackagedFrameworkCatalogEntry {
    pub(super) id: String,
    pub(super) version: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct PackagedMcpServerCatalog {
    #[serde(default)]
    pub(super) servers: Vec<PackagedMcpServerCatalogEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackagedMcpServerCatalogEntry {
    pub(super) id: String,
    pub(super) qualified_id: String,
    pub(super) version: String,
    pub(super) zip: String,
    pub(super) sha256: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct PackagedArtCatalogEntry {
    pub(super) id: String,
    pub(super) framework: String,
    pub(super) zip: String,
    pub(super) sha256: String,
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

pub(super) struct DaemonSnapshot {
    pub(super) health: Value,
    pub(super) status: Value,
    pub(super) capabilities: Vec<Value>,
    pub(super) mcp_servers: Vec<Value>,
    pub(super) tools: Vec<Value>,
    pub(super) python_arts: Vec<Value>,
    pub(super) workflows: Vec<Value>,
    pub(super) hook_bridge: Option<Value>,
    pub(super) degraded_errors: Vec<String>,
}

#[tauri::command]
pub(super) fn resolve_loom_daemon_url() -> DesktopRuntimeConfig {
    let loom_daemon_url = configured_loom_daemon_url();
    let settings_url = settings_url_with_daemon_token(&format!(
        "{}/settings",
        loom_daemon_url.trim_end_matches('/')
    ));

    DesktopRuntimeConfig {
        loom_daemon_url,
        settings_url,
        hook_bridge_url: configured_hook_bridge_url(),
    }
}

pub(super) async fn run_blocking_command<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("Loom 桌面后台任务异常结束：{error}"))?
}

pub(super) fn read_loom_snapshot_blocking(base_url: Option<String>) -> LoomSnapshot {
    let resolved_base_url = resolve_command_base_url(base_url.unwrap_or_default());
    let settings = settings_links(&resolved_base_url);
    let checked_at = chrono::Utc::now().to_rfc3339();

    match read_daemon_snapshot(&resolved_base_url) {
        Ok(snapshot) => {
            let daemon_mismatch = std::env::current_exe().ok().and_then(|current_exe| {
                daemon_path_mismatch_warning(&current_exe, &snapshot.status)
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
pub(super) async fn read_loom_snapshot(base_url: Option<String>) -> Result<LoomSnapshot, String> {
    run_blocking_command(move || Ok(read_loom_snapshot_blocking(base_url))).await
}

pub(super) fn start_loom_daemon_blocking() -> Result<LoomDaemonStartResult, String> {
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
    if http_get_json(&base_url, "/health").is_ok() {
        let status = http_get_json(&base_url, "/status")?;
        if daemon_path_mismatch_warning(&current_exe, &status).is_none() {
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

pub(super) fn register_owned_daemon_process(mut child: std::process::Child) -> Result<(), String> {
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

pub(super) fn stop_owned_daemon_process() -> Option<u32> {
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

pub(super) fn begin_desktop_exit() {
    LOOM_EXITING.store(true, Ordering::Release);
    stop_owned_daemon_process();
}

#[cfg(test)]
pub(super) fn owned_daemon_process_id() -> Option<u32> {
    OWNED_DAEMON_PROCESS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|owned| owned.as_ref().map(std::process::Child::id))
}
