//! Package identity, batch-entry, and unique temporary-name helpers.

use super::*;

static PACKAGE_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TEMPORARY_CREATE_ATTEMPTS: usize = 64;

/// Package activation is rare and mutates shared paths, so serialize it process-wide.
pub(super) fn lock_package_lifecycle() -> std::sync::MutexGuard<'static, ()> {
    PACKAGE_LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether a command names a Windows batch file, which is executed by `cmd.exe` rather than directly.
///
/// The extension is compared case-insensitively because Windows treats `SERVER.CMD` and `server.cmd`
/// as the same file, and the check is not `cfg(windows)`-gated: a package installs on one platform and
/// may be inspected on another, and a batch entry is wrong for the package either way.
pub(super) fn names_a_batch_file(command: &Path) -> bool {
    command
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("bat") || value.eq_ignore_ascii_case("cmd"))
}

pub(super) fn is_safe_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(super) fn staging_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos}-{sequence}", std::process::id())
}

pub(super) fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
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

/// Reject linked directory components before package code writes or recursively removes them.
pub(super) fn ensure_plain_directory(path: &Path) -> Result<(), McpPackageError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(McpPackageError::UnsafePath(path.display().to_string()));
    }
    Ok(())
}

/// Create control-plane-owned components one at a time and validate every existing component.
pub(super) fn ensure_directory_chain(
    root: &Path,
    components: &[&str],
) -> Result<PathBuf, McpPackageError> {
    fs::create_dir_all(root)?;
    ensure_plain_directory(root)?;
    let mut current = root.to_path_buf();
    for component in components {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        ensure_plain_directory(&current)?;
    }
    Ok(current)
}

/// Atomically claim a fresh staging leaf instead of adopting a pre-existing directory.
pub(super) fn create_unique_directory(
    parent: &Path,
    prefix: &str,
) -> Result<PathBuf, McpPackageError> {
    ensure_plain_directory(parent)?;
    for _ in 0..TEMPORARY_CREATE_ATTEMPTS {
        let path = parent.join(format!("{prefix}-{}", staging_name()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(McpPackageError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not claim a unique MCP package staging directory",
    )))
}
