//! Filesystem validation for SQLite's primary database path.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{RunStoreError, RunStoreResult};

pub(super) fn prepare_database_path(path: &Path) -> RunStoreResult<PathBuf> {
    let file_name = path.file_name().ok_or_else(|| {
        RunStoreError::Io(std::io::Error::new(
            ErrorKind::InvalidInput,
            "SQLite run store path has no file name",
        ))
    })?;
    let requested_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_dir()?);

    let created_parent = match fs::symlink_metadata(&requested_parent) {
        Ok(metadata) => {
            ensure_directory(&metadata, &requested_parent)?;
            false
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(&requested_parent)?;
            ensure_directory(&fs::symlink_metadata(&requested_parent)?, &requested_parent)?;
            true
        }
        Err(error) => return Err(error.into()),
    };
    if created_parent {
        loom_plugin_security::restrict_private_path_permissions(&requested_parent, true)?;
    }

    // Resolve the selected parent once so later changes to a symlink in the
    // caller-provided prefix cannot redirect SQLite's primary file.
    let canonical_parent = fs::canonicalize(&requested_parent)?;
    let database_path = canonical_parent.join(file_name);
    match fs::symlink_metadata(&database_path) {
        Ok(metadata) if metadata_has_link_semantics(&metadata) || !metadata.is_file() => {
            return Err(RunStoreError::Io(std::io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "SQLite run store is linked or not a regular file: {}",
                    database_path.display()
                ),
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(database_path)
}

pub(super) fn restrict_database_file(path: &Path) -> RunStoreResult<()> {
    loom_plugin_security::restrict_private_path_permissions(path, false)?;
    Ok(())
}

fn ensure_directory(metadata: &fs::Metadata, path: &Path) -> RunStoreResult<()> {
    if metadata_has_link_semantics(metadata) || !metadata.is_dir() {
        return Err(RunStoreError::Io(std::io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "SQLite run store parent is linked or not a directory: {}",
                path.display()
            ),
        )));
    }
    Ok(())
}

fn metadata_has_link_semantics(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}
