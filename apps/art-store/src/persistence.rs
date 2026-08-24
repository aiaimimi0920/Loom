// Cross-process locking and crash-safe replacement for shared Art Store indexes.
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::Serialize;

use crate::model::StoreError;

const STORE_LOCK_FILE: &str = ".art-store.lock";
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct StoreLock {
    file: File,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(crate) fn lock_store(root: &Path) -> Result<StoreLock, StoreError> {
    fs::create_dir_all(root)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(root.join(STORE_LOCK_FILE))?;
    let started = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(StoreLock { file }),
            Err(error) if lock_is_contended(&error) => {
                if started.elapsed() >= LOCK_TIMEOUT {
                    return Err(StoreError::PersistenceLockTimeout);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn lock_is_contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock
        || cfg!(windows) && error.raw_os_error() == Some(33)
}

pub(crate) fn write_json_atomic<T: Serialize>(
    root: &Path,
    file_name: &str,
    value: &T,
) -> Result<(), StoreError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes_atomic(&root.join(file_name), &bytes)
}

pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let (temporary, mut file) = create_atomic_temporary(path)?;
    let result = (|| -> Result<(), StoreError> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file_atomic(&temporary, path)?;
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
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "store path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "store path has no UTF-8 file name",
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
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique Art Store temporary file",
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
    File::open(path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "store path has no parent")
    })?)?
    .sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
