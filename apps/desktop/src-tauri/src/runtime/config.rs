//! Active endpoint selection, packaged daemon paths, and settings links.

use super::*;

pub(super) fn active_loom_daemon_url() -> Option<String> {
    if let Ok(active_url) = ACTIVE_DAEMON_URL.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(active_url) = active_url.as_ref() {
            return Some(active_url.clone());
        }
    }
    None
}

pub(super) fn configured_loom_daemon_url() -> String {
    if let Some(active_url) = active_loom_daemon_url() {
        return active_url;
    }
    std::env::var("LOOM_DAEMON_URL").unwrap_or_else(|_| DEFAULT_LOOM_DAEMON_URL.to_string())
}

pub(super) fn configured_hook_bridge_url() -> String {
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

pub(super) fn resolve_command_base_url(base_url: String) -> String {
    resolve_command_base_url_with_active(base_url, active_loom_daemon_url())
}

pub(super) fn resolve_command_base_url_with_active(
    base_url: String,
    active_url: Option<String>,
) -> String {
    if let Some(active_url) = active_url {
        return normalize_base_url(active_url);
    }
    normalize_base_url(if base_url.trim().is_empty() {
        configured_loom_daemon_url()
    } else {
        base_url
    })
}

pub(super) fn normalize_base_url(base_url: String) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

pub(super) fn settings_links(base_url: &str) -> SettingsLinks {
    let root_path = format!("{base_url}/settings");
    SettingsLinks {
        root: settings_url_with_daemon_token(&root_path),
        tea: settings_url_with_daemon_token(&format!("{root_path}/tea")),
        hook: settings_url_with_daemon_token(&format!("{root_path}/hook")),
        talk: settings_url_with_daemon_token(&format!("{root_path}/talk")),
    }
}

pub(super) fn configured_daemon_executable(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

pub(super) fn preferred_daemon_candidate(current_exe: &Path) -> PathBuf {
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

pub(super) fn daemon_executable_path_from_status(status: &Value) -> Option<PathBuf> {
    status
        .get("executablePath")
        .or_else(|| status.get("executable_path"))
        .or_else(|| status.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub(super) fn paths_match(left: &Path, right: &Path) -> bool {
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

pub(super) fn daemon_path_mismatch_warning(current_exe: &Path, status: &Value) -> Option<String> {
    let actual = daemon_executable_path_from_status(status)?;
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

pub(super) fn daemon_executable_candidates(
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

pub(super) fn daemon_sidecar_path_for_exe(current_exe: &Path) -> PathBuf {
    current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("runtime")
        .join("loom-daemon.exe")
}

pub(super) fn daemon_root_sibling_path_for_exe(current_exe: &Path) -> PathBuf {
    current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("loom-daemon.exe")
}

#[cfg(debug_assertions)]
pub(super) fn development_repo_root() -> Option<PathBuf> {
    Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".."),
    )
}

#[cfg(not(debug_assertions))]
pub(super) fn development_repo_root() -> Option<PathBuf> {
    None
}
