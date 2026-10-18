//! 包体由进程内 libaria2 传输；宿主负责授权和最终完整性复验。
use super::{error, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

// 只回收本次生成的两个引擎文件，不递归删除目录中的其他内容。
pub(super) fn cleanup(directory: &Path) {
    let Ok(metadata) = fs::symlink_metadata(directory) else {
        return;
    };
    if !metadata.is_dir() || loom_security::metadata_has_link_semantics(&metadata) {
        return;
    }
    for name in ["payload.zip", "payload.zip.aria2"] {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_dir(directory);
}

pub(super) fn download(
    client: reqwest::blocking::Client,
    dir: &Path,
    url: &str,
    token: &str,
    ticket: &str,
    size: u64,
    sha: &str,
    state: Option<std::sync::Arc<super::aria2_native::State>>,
) -> Result<PathBuf> {
    if [url, token, ticket]
        .iter()
        .any(|s| s.is_empty() || s.chars().any(char::is_control))
    {
        return Err(error(400, "asset_library_request_invalid"));
    }
    // 沿用有界 relay：凭据、远端重定向规则由宿主掌握，不因替换引擎而放宽。
    let relay =
        super::aria2_relay::Relay::start(client, url.into(), token.into(), ticket.into(), size)?;
    let directory = dir.join(format!("libaria2-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir(&directory).map_err(|_| error(500, "asset_library_save_failed"))?;
    let control = state.clone();
    let task = match state {
        Some(state) => {
            super::aria2_native::start_controlled(relay.url.clone(), directory.clone(), size, state)
        }
        None => super::aria2_native::start(relay.url.clone(), directory.clone(), size),
    };
    let task = task.map_err(|error| {
        cleanup(&directory);
        error
    })?;
    // 失败或超时由工作线程在关闭会话后清理，调用线程不删除仍在写入的文件。
    task.wait().map_err(|failure| {
        if control
            .as_ref()
            .is_some_and(|state| state.control.load(std::sync::atomic::Ordering::Acquire) == 1)
        {
            error(409, "asset_library_download_cancelled")
        } else {
            failure
        }
    })?;
    let result = fs::File::open(directory.join("payload.zip"))
        .map_err(|_| error(503, "asset_library_download_interrupted"))
        .and_then(|payload| super::download::save(payload, dir, size, sha));
    cleanup(&directory);
    result
}
