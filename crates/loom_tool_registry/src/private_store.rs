//! Shared hardening for small private control-plane documents.

use std::fs::{self, File};
use std::io::{ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};

use fs2::FileExt as _;

pub(crate) struct PrivateFileLock {
    file: File,
}

impl Drop for PrivateFileLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub(crate) fn ensure_private_parent(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "private file path has no parent")
    })?;
    match loom_plugin_security::restrict_private_path_permissions(parent, true) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(parent)?;
            loom_plugin_security::restrict_private_path_permissions(parent, true)
        }
        Err(error) => Err(error),
    }
}

/// Serializes read-modify-write operations across processes that honor the
/// same private store contract. Atomic replacement still protects readers.
pub(crate) fn lock_private_file(path: &Path) -> std::io::Result<PrivateFileLock> {
    ensure_private_parent(path)?;
    let lock_path = private_lock_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    apply_no_follow(&mut options);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&lock_path)?;
    let metadata = file.metadata()?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "private lock is linked or not a file: {}",
                lock_path.display()
            ),
        ));
    }
    loom_plugin_security::restrict_private_path_permissions(&lock_path, false)?;
    file.lock_exclusive()?;
    Ok(PrivateFileLock { file })
}

/// Reads a regular, non-linked private file without allocating past `max_bytes`.
pub(crate) fn read_bounded_private_file(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    apply_no_follow(&mut options);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing to read linked or non-file private path {}",
                path.display()
            ),
        ));
    }
    if metadata.len() > max_bytes {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("private file exceeds {max_bytes} bytes: {}", path.display()),
        ));
    }
    loom_plugin_security::restrict_private_path_permissions(path, false)?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(metadata.len().min(max_bytes)).unwrap_or_default());
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "private file grew beyond {max_bytes} bytes: {}",
                path.display()
            ),
        ));
    }
    Ok(bytes)
}

/// Replaces a private document through a unique, permission-restricted file.
pub(crate) fn write_private_file_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    ensure_private_parent(path)?;
    let (temporary, mut output) = create_private_temporary(path)?;
    let result = (|| {
        output.write_all(bytes)?;
        output.sync_all()?;
        drop(output);
        loom_plugin_security::restrict_private_path_permissions(&temporary, false)?;
        crate::replace_registry_file(&temporary, path)?;
        loom_plugin_security::restrict_private_path_permissions(path, false)?;
        sync_parent_directory(path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_private_temporary(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "private file path has no parent")
    })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "private file path has no UTF-8 file name",
            )
        })?;
    for attempt in 0..100u32 {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".{name}.tmp-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "could not allocate a unique private temporary file",
    ))
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "private file path has no parent")
    })?;
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn private_lock_path(path: &Path) -> std::io::Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "private file path has no parent")
    })?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                "private file path has no UTF-8 file name",
            )
        })?;
    Ok(parent.join(format!(".{name}.lock")))
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

fn apply_no_follow(options: &mut fs::OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(not(any(unix, windows)))]
    let _ = options;
}
