use serde::Serialize;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_LOOM_DAEMON_URL: &str = "http://127.0.0.1:8765";
const DEFAULT_HOOK_BRIDGE_URL: &str = "ws://127.0.0.1:19820";
const LOOM_DAEMON_EXECUTABLE_ENV: &str = "LOOM_DAEMON_EXECUTABLE";
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
pub struct LoomDaemonStartResult {
    pub started: bool,
    pub base_url: String,
    pub path: String,
    pub message: String,
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

#[tauri::command]
fn read_loom_snapshot(base_url: Option<String>) -> LoomSnapshot {
    let resolved_base_url = normalize_base_url(
        base_url
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(configured_loom_daemon_url),
    );
    let settings = settings_links(&resolved_base_url);
    let checked_at = chrono::Utc::now().to_rfc3339();

    match read_daemon_snapshot(&resolved_base_url) {
        Ok(snapshot) => {
            let daemon_mismatch = std::env::current_exe()
                .ok()
                .and_then(|current_exe| daemon_path_mismatch_warning(&current_exe, &snapshot.health));
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
fn start_loom_daemon() -> Result<LoomDaemonStartResult, String> {
    let base_url = normalize_base_url(configured_loom_daemon_url());
    let current_exe =
        std::env::current_exe().map_err(|error| format!("无法定位 Loom.exe：{error}"))?;
    if let Ok(health) = http_get_json(&base_url, "/health") {
        let message = daemon_path_mismatch_warning(&current_exe, &health)
            .unwrap_or_else(|| "Loom 本地服务已运行。".to_string());
        return Ok(LoomDaemonStartResult {
            started: false,
            base_url,
            path: String::new(),
            message,
        });
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
    let mut command = std::process::Command::new(&daemon_path);
    command
        .env("LOOM_DAEMON_HOST", host)
        .env("LOOM_DAEMON_PORT", port.to_string());

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

    command
        .spawn()
        .map_err(|error| format!("启动 Loom 本地服务失败：{error}"))?;

    Ok(LoomDaemonStartResult {
        started: true,
        base_url,
        path: daemon_path.display().to_string(),
        message: "已启动 Loom 本地服务。".to_string(),
    })
}

#[tauri::command]
fn post_loom_daemon_json(base_url: String, path: String, body: Value) -> Result<Value, String> {
    let resolved_base_url = resolve_command_base_url(base_url);
    let path = normalize_daemon_path(path)?;
    http_post_json(&resolved_base_url, &path, &body)
}

#[tauri::command]
fn get_loom_daemon_json(base_url: String, path: String) -> Result<Value, String> {
    let resolved_base_url = resolve_command_base_url(base_url);
    let path = normalize_daemon_path(path)?;
    http_get_json(&resolved_base_url, &path)
}

#[tauri::command]
fn put_loom_daemon_json(base_url: String, path: String, body: Value) -> Result<Value, String> {
    let resolved_base_url = resolve_command_base_url(base_url);
    let path = normalize_daemon_path(path)?;
    http_put_json(&resolved_base_url, &path, &body)
}

#[tauri::command]
fn delete_loom_daemon_json(base_url: String, path: String) -> Result<Value, String> {
    let resolved_base_url = resolve_command_base_url(base_url);
    let path = normalize_daemon_path(path)?;
    http_delete_json(&resolved_base_url, &path)
}

// Fetch a Hook canvas preview image through the native HTTP client and return it
// as a base64 `data:` URL. The WebView cannot reliably load `http://127.0.0.1`
// daemon images with an `<img src>` tag, so the frontend renders previews from
// the data URL this command returns instead of a direct daemon URL.
#[tauri::command]
fn read_hook_canvas_preview(base_url: String, path: String) -> Result<String, String> {
    let resolved_base_url = resolve_command_base_url(base_url);
    let path = normalize_daemon_path(path)?;
    let (content_type, bytes) = http_get_binary(&resolved_base_url, &path)?;
    let encoded = base64_encode(&bytes);
    Ok(format!("data:{content_type};base64,{encoded}"))
}

fn configured_loom_daemon_url() -> String {
    std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_string())
}

fn configured_hook_bridge_url() -> String {
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
    http_request_json(base_url, "GET", path, None)
}

fn http_post_json(base_url: &str, path: &str, body: &Value) -> Result<Value, String> {
    http_request_json(base_url, "POST", path, Some(body))
}

fn http_put_json(base_url: &str, path: &str, body: &Value) -> Result<Value, String> {
    http_request_json(base_url, "PUT", path, Some(body))
}

fn http_delete_json(base_url: &str, path: &str) -> Result<Value, String> {
    http_request_json(base_url, "DELETE", path, None)
}

fn http_request_json(
    base_url: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let (host, port) = parse_loopback_http_url(base_url)?;
    let mut stream = TcpStream::connect((host.as_str(), port))
        .map_err(|error| format!("无法连接 Loom 本地服务 {base_url}：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| format!("无法设置 Loom 本地服务读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
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
    if !headers.starts_with("HTTP/1.1 200") {
        let status_line = headers.lines().next().unwrap_or("unknown status");
        return Err(format!("{path} returned {status_line}"));
    }

    serde_json::from_str(body)
        .map_err(|error| format!("无法解析 Loom 本地服务响应 {path}：{error}"))
}

// Fetch a binary daemon response (e.g. a preview image) over the native HTTP
// client. Unlike `http_request_json`, this reads raw bytes so image payloads are
// not corrupted by UTF-8 decoding, and it returns the Content-Type so the caller
// can build a correct `data:` URL.
fn http_get_binary(base_url: &str, path: &str) -> Result<(String, Vec<u8>), String> {
    let (host, port) = parse_loopback_http_url(base_url)?;
    let mut stream = TcpStream::connect((host.as_str(), port))
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
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
    let mut context = tauri::generate_context!();
    #[cfg(target_os = "windows")]
    configure_webview2_browser_args(&mut context);

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            resolve_loom_daemon_url,
            read_loom_snapshot,
            start_loom_daemon,
            get_loom_daemon_json,
            put_loom_daemon_json,
            delete_loom_daemon_json,
            post_loom_daemon_json,
            read_hook_canvas_preview
        ])
        .run(context)
        .expect("error while running Loom desktop");
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

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-desktop-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp dir");
        root
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
    fn offline_snapshot_preserves_settings_links() {
        let snapshot = read_loom_snapshot(Some("http://127.0.0.1:9".to_string()));

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

        let snapshot = read_loom_snapshot(Some(format!("http://127.0.0.1:{}", address.port())));
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

        let snapshot = read_loom_snapshot(Some(format!("http://127.0.0.1:{}", address.port())));
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
        let snapshot = read_loom_snapshot(Some("http://example.com:8765".to_string()));

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
        assert!(warning.contains(&packaged_daemon.display().to_string()), "{warning}");

        fs::remove_dir_all(root).expect("cleanup temp dir");
        restore_env(LOOM_DAEMON_EXECUTABLE_ENV, previous_override);
    }
}
