//! Loom cache policy, inventory, pruning, and command adapters.

use super::*;

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
    pub art_cache_max_bytes: u64,
    pub art_cache_retention_days: u32,
    pub framework_temp_retention_days: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomGeneralRuntimeSettings {
    pub minimize_to_tray: bool,
}

pub(super) fn read_loom_persisted_general_settings() -> Option<LoomGeneralRuntimeSettings> {
    let path = desktop_control_plane_root()
        .join("settings")
        .join("settings.json");
    let bytes = read_bounded_regular_file(&path, MAX_SETTINGS_FILE_BYTES, "Loom 通用设置").ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    serde_json::from_value(value.get("general")?.clone()).ok()
}

#[tauri::command]
pub(super) fn apply_loom_general_settings(settings: LoomGeneralRuntimeSettings) {
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

pub(super) fn validate_loom_cache_preferences(
    settings: &LoomCachePreferences,
) -> Result<(), String> {
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

pub(super) fn read_loom_persisted_cache_settings() -> Option<LoomCachePreferences> {
    let path = desktop_control_plane_root()
        .join("settings")
        .join("settings.json");
    let bytes = read_bounded_regular_file(&path, MAX_SETTINGS_FILE_BYTES, "Loom 缓存设置").ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    serde_json::from_value(value.get("loom_cache")?.clone()).ok()
}

pub(super) fn loom_framework_temporary_dir() -> PathBuf {
    std::env::var_os("LOOM_FRAMEWORK_TEMP_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("loom-framework"))
}

pub(super) fn collect_art_cache_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
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

pub(super) fn loom_art_cache_dirs() -> Result<Vec<PathBuf>, String> {
    collect_art_cache_dirs(&desktop_control_plane_root().join("arts"))
}

pub(super) fn loom_cache_entry(
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

pub(super) fn loom_cache_snapshot() -> Result<LoomCacheSnapshot, String> {
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
pub(super) struct CacheFileInfo {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

pub(super) fn collect_cache_files(roots: &[PathBuf]) -> Result<Vec<CacheFileInfo>, String> {
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

pub(super) fn remove_cache_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法删除缓存文件 `{}`：{error}", path.display())),
    }
}

pub(super) fn prune_cache_roots(
    roots: &[PathBuf],
    max_bytes: u64,
    retention_days: u32,
) -> Result<(), String> {
    for root in roots {
        validate_destructive_cache_root(root)?;
    }
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

pub(super) fn apply_loom_cache_preferences(settings: &LoomCachePreferences) -> Result<(), String> {
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
pub(super) async fn get_loom_cache_snapshot() -> Result<LoomCacheSnapshot, String> {
    run_blocking_command(loom_cache_snapshot).await
}

#[tauri::command]
pub(super) async fn apply_loom_cache_settings(
    settings: LoomCachePreferences,
) -> Result<LoomCacheSnapshot, String> {
    run_blocking_command(move || {
        apply_loom_cache_preferences(&settings)?;
        loom_cache_snapshot()
    })
    .await
}

#[tauri::command]
pub(super) async fn clear_loom_cache(kind: String) -> Result<LoomCacheClearResult, String> {
    run_blocking_command(move || clear_loom_cache_blocking(&kind)).await
}

pub(super) fn clear_loom_cache_blocking(kind: &str) -> Result<LoomCacheClearResult, String> {
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
