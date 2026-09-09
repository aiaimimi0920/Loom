//! Deterministic, size-bounded package enumeration and streaming SHA-256.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use loom_security::metadata_has_link_semantics;

use crate::error::invalid_data;
use crate::{PluginSecurityError, MAX_SIGNED_PACKAGE_BYTES, MAX_SIGNED_PACKAGE_FILES};

pub fn canonical_package_digest(
    package_dir: &Path,
    excluded_relative_path: Option<&str>,
) -> Result<String, PluginSecurityError> {
    let excluded = excluded_relative_path.map(|path| path.replace('\\', "/").to_ascii_lowercase());
    let files = collect_files(package_dir, excluded.as_deref())?;
    let mut hasher = Sha256::new();
    for (relative, path, expected_length) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(expected_length.to_le_bytes());
        hasher.update([0]);
        hash_file(&mut hasher, &path, expected_length)?;
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hash_file(
    hasher: &mut Sha256,
    path: &Path,
    expected_length: u64,
) -> Result<(), PluginSecurityError> {
    let link_metadata = fs::symlink_metadata(path)?;
    if metadata_has_link_semantics(&link_metadata) {
        return Err(PluginSecurityError::SymbolicLink(
            path.display().to_string(),
        ));
    }
    let mut file = File::open(path)?;
    let actual_length = file.metadata()?.len();
    if actual_length != expected_length {
        return Err(package_changed(path));
    }
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = expected_length;
    while remaining > 0 {
        let requested = remaining.min(buffer.len() as u64) as usize;
        let count = file.read(&mut buffer[..requested])?;
        if count == 0 {
            return Err(package_changed(path));
        }
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if file.read(&mut buffer[..1])? != 0 {
        return Err(package_changed(path));
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    excluded: Option<&str>,
) -> Result<Vec<(String, PathBuf, u64)>, PluginSecurityError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        let directory_metadata = fs::symlink_metadata(&directory)?;
        if metadata_has_link_semantics(&directory_metadata) || !directory_metadata.is_dir() {
            return Err(PluginSecurityError::SymbolicLink(
                directory.display().to_string(),
            ));
        }
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata_has_link_semantics(&metadata) {
                return Err(PluginSecurityError::SymbolicLink(
                    path.display().to_string(),
                ));
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| PluginSecurityError::UnsafePath(path.display().to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            validate_relative_path(&relative)?;
            let folded = relative.to_ascii_lowercase();
            if excluded == Some(folded.as_str()) {
                continue;
            }
            if !seen.insert(folded) {
                return Err(PluginSecurityError::DuplicatePath(relative));
            }
            let length = metadata.len();
            total = total.saturating_add(length);
            if total > MAX_SIGNED_PACKAGE_BYTES {
                return Err(PluginSecurityError::PackageSize);
            }
            files.push((relative, path, length));
            if files.len() > MAX_SIGNED_PACKAGE_FILES {
                return Err(PluginSecurityError::FileCount);
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

pub(crate) fn validate_relative_path(value: &str) -> Result<(), PluginSecurityError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || value.contains('\\')
        || value.contains(':')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PluginSecurityError::UnsafePath(value.to_owned()));
    }
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(PluginSecurityError::UnsafePath(value.to_owned()));
        };
        let component = component.to_string_lossy();
        if component.ends_with('.')
            || component.ends_with(' ')
            || loom_protocol::is_windows_reserved_device_name(&component)
        {
            return Err(PluginSecurityError::UnsafePath(value.to_owned()));
        }
    }
    Ok(())
}

pub(crate) fn checked_package_output_path(
    package_dir: &Path,
    relative: &str,
) -> Result<PathBuf, PluginSecurityError> {
    validate_relative_path(relative)?;
    let output = package_dir.join(relative);
    let components = Path::new(relative).components().collect::<Vec<_>>();
    let mut current = package_dir.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata_has_link_semantics(&metadata) => {
                return Err(PluginSecurityError::SymbolicLink(
                    current.display().to_string(),
                ));
            }
            Ok(metadata) if index + 1 < components.len() && !metadata.is_dir() => {
                return Err(PluginSecurityError::UnsafePath(
                    current.display().to_string(),
                ));
            }
            Ok(metadata) if index + 1 == components.len() && !metadata.is_file() => {
                return Err(PluginSecurityError::UnsafePath(
                    current.display().to_string(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(PluginSecurityError::Io(error)),
        }
    }
    Ok(output)
}

fn package_changed(path: &Path) -> PluginSecurityError {
    invalid_data(format!(
        "package file changed while computing its digest: {}",
        path.display()
    ))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
