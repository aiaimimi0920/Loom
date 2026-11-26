//! Transfers archive payloads via in-process libaria2; the host retains authorization and final integrity verification.
use super::{error, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

// Reclaim only the two engine files created by this task without recursively deleting anything else in the directory.
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
    // Keep the bounded relay: credentials and remote redirect rules remain owned by the host and are never relaxed by swapping engines.
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
    // On failure or timeout, the worker thread cleans up after closing the session so the caller never deletes files still being written.
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
