//! Bounded reads and crash-resistant same-directory replacement.

use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use crate::error::invalid_data;
use crate::{restrict_private_path_permissions, PluginSecurityError};

pub(crate) fn read_bounded(
    path: &Path,
    maximum_bytes: u64,
    document_name: &str,
) -> Result<Vec<u8>, PluginSecurityError> {
    let mut file = File::open(path)?;
    let initial_length = file.metadata()?.len();
    if initial_length > maximum_bytes {
        return Err(invalid_data(format!(
            "{document_name} exceeds the {maximum_bytes}-byte limit"
        )));
    }
    let mut bytes = Vec::with_capacity(initial_length.min(maximum_bytes) as usize);
    std::io::Read::by_ref(&mut file)
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(invalid_data(format!(
            "{document_name} exceeds the {maximum_bytes}-byte limit"
        )));
    }
    Ok(bytes)
}

pub(crate) fn write_bytes_atomic(
    path: &Path,
    bytes: &[u8],
    private_parent: bool,
    private_file: bool,
) -> Result<(), PluginSecurityError> {
    let parent = path.parent().ok_or_else(|| {
        invalid_data(format!(
            "atomic output path has no parent: {}",
            path.display()
        ))
    })?;
    if private_parent {
        match restrict_private_path_permissions(parent, true) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(parent)?;
                restrict_private_path_permissions(parent, true)?;
            }
            Err(error) => return Err(PluginSecurityError::Io(error)),
        }
    } else {
        fs::create_dir_all(parent)?;
    }

    let (temporary, mut file) = create_atomic_temporary(path)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if private_file {
            restrict_private_path_permissions(&temporary, false)?;
        }
        replace_file_atomic(&temporary, path)?;
        if private_file {
            restrict_private_path_permissions(path, false)?;
        }
        sync_parent_directory(path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_atomic_temporary(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "atomic output path has no parent",
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "atomic output path has no UTF-8 file name",
            )
        })?;
    for attempt in 0..100u32 {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique atomic temporary file",
    ))
}

#[cfg(not(windows))]
fn replace_file_atomic(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file_atomic(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn extended_length_path(path: &Path) -> std::io::Result<Vec<u16>> {
        let absolute = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "atomic replacement path has no parent",
                    )
                })?;
                let file_name = path.file_name().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "atomic replacement path has no file name",
                    )
                })?;
                fs::canonicalize(parent)?.join(file_name)
            }
            Err(error) => return Err(error),
        };
        let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut extended =
            if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
                || wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
            {
                wide
            } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
                let mut path = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide[2..]);
                path
            } else {
                let mut path = r"\\?\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide);
                path
            };
        extended.push(0);
        Ok(extended)
    }

    let source = extended_length_path(source)?;
    let destination = extended_length_path(destination)?;
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
