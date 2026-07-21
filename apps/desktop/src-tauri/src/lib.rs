use serde::Serialize;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_LOOM_DAEMON_URL: &str = "http://127.0.0.1:8765";
const LOOM_DAEMON_EXECUTABLE_ENV: &str = "LOOM_DAEMON_EXECUTABLE";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRuntimeConfig {
    pub loom_daemon_url: String,
    pub settings_url: String,
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
        Ok(snapshot) => LoomSnapshot {
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
            error: degraded_snapshot_error(&snapshot.degraded_errors),
        },
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
    if http_get_json(&base_url, "/health").is_ok() {
        return Ok(LoomDaemonStartResult {
            started: false,
            base_url,
            path: String::new(),
            message: "Loom 本地服务已运行。".to_string(),
        });
    }

    let current_exe =
        std::env::current_exe().map_err(|error| format!("无法定位 Loom.exe：{error}"))?;
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

fn configured_loom_daemon_url() -> String {
    std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_string())
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
    let mcp_servers = read_optional_daemon_array(
        base_url,
        "/v1/mcp/servers",
        "servers",
        &mut degraded_errors,
    );
    let tools =
        read_optional_daemon_array(base_url, "/v1/tools", "tools", &mut degraded_errors);
    let python_arts = read_optional_daemon_array(
        base_url,
        "/v1/python-arts",
        "arts",
        &mut degraded_errors,
    );
    let workflows = read_optional_daemon_array(
        base_url,
        "/v1/workflows",
        "workflows",
        &mut degraded_errors,
    );
    let hook_bridge =
        read_optional_daemon_json(base_url, "/v1/hook-bridge/status", &mut degraded_errors);

    if !degraded_errors.is_empty() {
        http_get_json(base_url, "/health").map_err(|error| {
            format!("Loom 本地服务在读取模块状态期间离线：{error}")
        })?;
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
        degraded_errors.push(format!(
            "{path} 返回的模块数据无效：`{key}` 必须是数组"
        ));
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

fn degraded_snapshot_error(errors: &[String]) -> Option<String> {
    if errors.is_empty() {
        None
    } else {
        Some(format!(
            "Loom 本地服务在线，但部分模块暂不可用：{}",
            errors.join("；")
        ))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            resolve_loom_daemon_url,
            read_loom_snapshot,
            start_loom_daemon,
            get_loom_daemon_json,
            put_loom_daemon_json,
            delete_loom_daemon_json,
            post_loom_daemon_json
        ])
        .run(tauri::generate_context!())
        .expect("error while running Loom desktop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn default_runtime_config_points_at_loopback_loom_daemon() {
        std::env::remove_var("LOOM_DAEMON_URL");

        let config = resolve_loom_daemon_url();

        assert_eq!(config.loom_daemon_url, DEFAULT_LOOM_DAEMON_URL);
        assert_eq!(config.settings_url, "http://127.0.0.1:8765/settings");
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
        let address = listener.local_addr().expect("read snapshot fixture address");
        listener
            .set_nonblocking(true)
            .expect("set snapshot fixture nonblocking");
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            loop {
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
            }
        });

        let snapshot = read_loom_snapshot(Some(format!("http://127.0.0.1:{}", address.port())));
        shutdown_tx.send(()).expect("stop snapshot fixture");
        server.join().expect("join snapshot fixture");

        assert_eq!(snapshot.connection_state, "online");
        assert_eq!(snapshot.health, Some(serde_json::json!({"status": "ok"})));
        assert_eq!(snapshot.status, Some(serde_json::json!({"status": "ready"})));
        assert!(snapshot.tools.is_empty());
        assert!(snapshot.error.as_deref().is_some_and(|error| error.contains("/v1/tools")));
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
        let address = listener.local_addr().expect("read malformed fixture address");
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
                std::path::PathBuf::from(r"C:\src\Loom\target\debug\loom-daemon.exe"),
            ]
        );
    }

    #[test]
    fn blank_daemon_override_is_ignored() {
        assert_eq!(configured_daemon_executable("  "), None);
    }
}
