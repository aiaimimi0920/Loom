//! Symlink-resistant, bounded filesystem operations for workflow documents.

use std::fs::{self, File};
use std::io::{ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};

use fs2::FileExt as _;

const STORE_LOCK_FILE: &str = ".workflow-store.lock";

pub(super) struct StoreLock {
    file: File,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub(super) fn lock_store(root: &Path) -> std::io::Result<StoreLock> {
    ensure_private_root(root)?;
    let path = root.join(STORE_LOCK_FILE);
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    apply_no_follow(&mut options);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    ensure_regular_handle(&file, &path)?;
    loom_plugin_security::restrict_private_path_permissions(&path, false)?;
    file.lock_exclusive()?;
    Ok(StoreLock { file })
}

pub(super) fn ensure_private_root(root: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => ensure_directory_metadata(&metadata, root)?,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(root)?;
            ensure_directory_metadata(&fs::symlink_metadata(root)?, root)?;
        }
        Err(error) => return Err(error),
    }
    loom_plugin_security::restrict_private_path_permissions(root, true)
}

pub(super) fn read_bounded_utf8(path: &Path, max_bytes: usize) -> std::io::Result<String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    apply_no_follow(&mut options);
    let file = options.open(path)?;
    ensure_regular_handle(&file, path)?;
    let metadata = file.metadata()?;
    if metadata.len() > max_bytes as u64 {
        return Err(invalid_data(format!(
            "workflow file exceeds {max_bytes} bytes: {}",
            path.display()
        )));
    }
    loom_plugin_security::restrict_private_path_permissions(path, false)?;
    let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes as u64) as usize);
    file.take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(invalid_data(format!(
            "workflow file grew beyond {max_bytes} bytes: {}",
            path.display()
        )));
    }
    String::from_utf8(bytes).map_err(|error| invalid_data(error.to_string()))
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data("workflow path has no parent"))?;
    ensure_private_root(parent)?;
    reject_linked_destination(path)?;
    let (temporary, mut output) = create_temporary(path)?;
    let result = (|| {
        output.write_all(bytes)?;
        output.sync_all()?;
        drop(output);
        loom_plugin_security::restrict_private_path_permissions(&temporary, false)?;
        replace_file(&temporary, path)?;
        loom_plugin_security::restrict_private_path_permissions(path, false)?;
        sync_parent_directory(path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn remove_regular_file(path: &Path) -> std::io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
                return Err(invalid_data(format!(
                    "refusing to remove linked or non-file workflow path {}",
                    path.display()
                )));
            }
            fs::remove_file(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn ensure_regular_directory_entry(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
        return Err(invalid_data(format!(
            "workflow directory entry is linked or not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn ensure_directory_metadata(metadata: &fs::Metadata, path: &Path) -> std::io::Result<()> {
    if metadata_has_link_semantics(metadata) || !metadata.is_dir() {
        return Err(invalid_data(format!(
            "workflow root is linked or not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn ensure_regular_handle(file: &File, path: &Path) -> std::io::Result<()> {
    let metadata = file.metadata()?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
        return Err(invalid_data(format!(
            "workflow path is linked or not a regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn reject_linked_destination(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_has_link_semantics(&metadata) || !metadata.is_file() => {
            Err(invalid_data(format!(
                "refusing to replace linked or non-file workflow path {}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn create_temporary(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data("workflow path has no parent"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_data("workflow path has no UTF-8 file name"))?;
    for attempt in 0..100_u32 {
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
        "could not allocate a unique workflow temporary file",
    ))
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data("workflow path has no parent"))?;
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
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

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn extended_length_path(path: &Path) -> std::io::Result<Vec<u16>> {
        let parent = path
            .parent()
            .ok_or_else(|| invalid_data("workflow path has no parent"))?;
        let file_name = path
            .file_name()
            .ok_or_else(|| invalid_data("workflow path has no file name"))?;
        let absolute = fs::canonicalize(parent)?.join(file_name);
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

fn invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(ErrorKind::InvalidData, message.into())
}
