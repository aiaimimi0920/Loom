//! Application diagnostics and guarded operating-system open actions.

use super::*;

pub(super) fn application_log_dir(app: &str) -> Result<PathBuf, String> {
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

pub(super) fn newest_log_file(log_dir: &Path) -> Option<PathBuf> {
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

pub(super) fn application_diagnostics(app: &str) -> Result<ApplicationDiagnostics, String> {
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
pub(super) fn resolve_application_diagnostics(
    app: String,
) -> Result<ApplicationDiagnostics, String> {
    application_diagnostics(app.trim())
}

#[cfg(target_os = "windows")]
pub(super) fn open_local_path(path: &Path, file: bool) -> Result<(), String> {
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
pub(super) fn open_local_path(path: &Path, _file: bool) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 `{}`：{error}", path.display()))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub(super) fn open_local_path(path: &Path, _file: bool) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开 `{}`：{error}", path.display()))
}

#[tauri::command]
pub(super) fn open_application_log_location(app: String, target: String) -> Result<(), String> {
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

pub(super) fn is_allowed_repository_url(url: &str) -> bool {
    [env!("LOOM_BUILD_REPOSITORY"), env!("HOOK_BUILD_REPOSITORY")].contains(&url)
}

pub(super) fn is_safe_external_https_url(url: &str) -> bool {
    tauri::Url::parse(url).is_ok_and(|parsed| {
        parsed.scheme() == "https"
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
    })
}

#[cfg(target_os = "windows")]
pub(super) fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let target = url
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
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
        Err(format!(
            "系统浏览器无法打开地址（ShellExecuteW={status}）。"
        ))
    }
}

#[cfg(target_os = "macos")]
pub(super) fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开仓库地址：{error}"))
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub(super) fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开仓库地址：{error}"))
}

#[tauri::command]
pub(super) fn open_external_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !url.starts_with("https://") || !is_allowed_repository_url(url) {
        return Err("只允许打开 Loom 或 Hook 的官方仓库地址。".to_owned());
    }
    open_url_in_default_browser(url)
}

#[tauri::command]
pub(super) fn open_mcp_source_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !is_safe_external_https_url(url) {
        return Err("只允许打开不包含账号信息的 HTTPS MCP 来源地址。".to_owned());
    }
    open_url_in_default_browser(url)
}
