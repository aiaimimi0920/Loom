//! Hook cache discovery, settings synchronization, snapshots, and clearing.

use super::*;

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

pub(super) fn read_hook_persisted_cache_settings() -> Option<HookCachePreferences> {
    let path = hook_effective_app_data_dir().join("app-settings.json");
    let bytes = read_bounded_regular_file(&path, MAX_SETTINGS_FILE_BYTES, "Hook 缓存设置").ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    serde_json::from_value(value.get("cache")?.clone()).ok()
}

#[tauri::command]
pub(super) async fn wait_for_hook_cache_settings(
    settings: HookCachePreferences,
) -> Result<bool, String> {
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

pub(super) fn hook_effective_app_data_dir() -> PathBuf {
    std::env::var_os("HOOK_APPDATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join("com.yamiyu.hook"))
        })
        .unwrap_or_else(|| std::env::temp_dir().join("com.yamiyu.hook"))
}

pub(super) fn hook_clipboard_cache_dir() -> PathBuf {
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

pub(super) fn directory_usage(path: &Path) -> Result<(u64, u64), String> {
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

pub(super) fn hook_cache_entry(
    key: &str,
    label: &str,
    path: PathBuf,
) -> Result<HookCacheEntry, String> {
    let (bytes, file_count) = directory_usage(&path)?;
    Ok(HookCacheEntry {
        key: key.to_owned(),
        label: label.to_owned(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        file_count,
    })
}

pub(super) fn hook_cache_snapshot() -> Result<HookCacheSnapshot, String> {
    let temporary = hook_cache_entry("temporary", "临时缓存", hook_clipboard_cache_dir())?;
    let session_path = hook_effective_app_data_dir().join("session.json");
    let session =
        read_bounded_regular_file(&session_path, MAX_SETTINGS_FILE_BYTES, "Hook 会话缓存索引")
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

pub(super) fn clear_directory_contents(path: &Path) -> Result<(), String> {
    validate_destructive_cache_root(path)?;
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
pub(super) async fn get_hook_cache_snapshot() -> Result<HookCacheSnapshot, String> {
    run_blocking_command(hook_cache_snapshot).await
}

#[tauri::command]
pub(super) async fn clear_hook_cache(kind: String) -> Result<HookCacheClearResult, String> {
    run_blocking_command(move || clear_hook_cache_blocking(&kind)).await
}

pub(super) fn clear_hook_cache_blocking(kind: &str) -> Result<HookCacheClearResult, String> {
    let kind = kind.trim();
    let before = hook_cache_snapshot()?;
    match kind {
        "temporary" => clear_directory_contents(&hook_clipboard_cache_dir())?,
        "recycleBin" => {
            http_post_json(
                &configured_loom_daemon_url(),
                "/v1/hook-bridge/cache-control",
                &serde_json::json!({ "action": "clearRecycleBin" }),
            )?;
        }
        "referenceLibrary" => {
            http_post_json(
                &configured_loom_daemon_url(),
                "/v1/hook-bridge/cache-control",
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
