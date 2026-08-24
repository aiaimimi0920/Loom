//! Atomic active-state persistence and bounded state loading.

use super::*;

pub(super) fn write_active_state(
    package_root: &Path,
    manifest: &McpServerPackageManifest,
    digest: &str,
    target_dir: &Path,
    files: &BTreeMap<String, String>,
    trust_status: &PackageTrustStatus,
) -> Result<(), McpPackageError> {
    ensure_plain_directory(package_root)?;
    let path = package_root.join("active.json");
    // The temporary name carries a nonce. A constant one is shared by every concurrent install of the
    // same package — a retry racing its own first attempt is enough to have two — and then both write
    // the same file and whichever rename lands second publishes a mix of the two payloads. This is the
    // nonce the staging directory already uses.
    let (temporary, mut file) = create_state_temporary_file(package_root)?;
    let payload = serde_json::to_vec_pretty(&McpPackageActiveState {
        qualified_id: manifest.qualified_id(),
        version: manifest.version.clone(),
        digest: digest.to_owned(),
        package_dir: target_dir.to_path_buf(),
        files: files.clone(),
        trust_status: trust_status.clone(),
    })?;
    // Synced before the rename, the way `write_tools` and the zip extractor both do it:
    // `MOVEFILE_WRITE_THROUGH` flushes the rename, not the bytes of the file being renamed, so a crash
    // in between could otherwise leave an `active.json` that is present and empty. That reads back as a
    // package with no recorded digests, which is the one state a spawn refuses outright.
    let written = (|| {
        file.write_all(&payload)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = replace_file(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

pub(super) fn read_active_state_file(
    path: &Path,
) -> Result<McpPackageActiveState, McpPackageError> {
    let bytes = read_bounded_file(path, MAX_ACTIVE_STATE_BYTES, "MCP package active state")?;
    serde_json::from_slice(&bytes).map_err(|error| {
        McpPackageError::InvalidManifest(format!("cannot parse {}: {error}", path.display()))
    })
}

pub(super) fn read_bounded_file(
    path: &Path,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, McpPackageError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(McpPackageError::UnsafePath(path.display().to_string()));
    }
    if metadata.len() > limit as u64 {
        return Err(McpPackageError::InvalidManifest(format!(
            "{label} exceeds {limit} bytes"
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(limit).min(limit));
    fs::File::open(path)?
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(McpPackageError::InvalidManifest(format!(
            "{label} exceeds {limit} bytes"
        )));
    }
    Ok(bytes)
}

fn create_state_temporary_file(package_root: &Path) -> std::io::Result<(PathBuf, fs::File)> {
    for _ in 0..64 {
        let path = package_root.join(format!("active.json.{}.tmp", staging_name()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not claim a unique MCP package state file",
    ))
}

#[cfg(not(windows))]
pub(super) fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub(super) fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = fs::canonicalize(source)?;
    let destination_parent = destination.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "MCP package state path has no parent",
        )
    })?;
    let destination =
        fs::canonicalize(destination_parent)?.join(destination.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "MCP package state path has no file name",
            )
        })?);
    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
