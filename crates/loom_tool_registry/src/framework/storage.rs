//! Framework package storage paths, tombstones, and filesystem cleanup.
use super::*;

use std::io::Read;

pub(super) fn metadata_has_link_semantics(metadata: &fs::Metadata) -> bool {
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

pub(super) fn is_directory_without_links(path: &Path) -> Result<bool, FrameworkError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir() && !metadata_has_link_semantics(&metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(FrameworkError::Io(error)),
    }
}

pub(super) fn is_file_without_links(path: &Path) -> Result<bool, FrameworkError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && !metadata_has_link_semantics(&metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(FrameworkError::Io(error)),
    }
}

/// Reads a regular, non-linked file without ever allocating beyond the
/// configured limit. The `take` guard preserves the bound even if the file
/// grows after its metadata is inspected.
pub(super) fn read_bounded_file(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing to read linked or non-file path {}",
                path.display()
            ),
        ));
    }
    if metadata.len() > max_bytes {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("file exceeds {max_bytes} bytes: {}", path.display()),
        ));
    }
    let mut bytes =
        Vec::with_capacity(usize::try_from(metadata.len().min(max_bytes)).unwrap_or_default());
    fs::File::open(path)?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("file grew beyond {max_bytes} bytes: {}", path.display()),
        ));
    }
    Ok(bytes)
}

pub(super) fn uninstall_tombstone_path(
    live: &Path,
    prefix: &str,
) -> Result<PathBuf, FrameworkError> {
    let parent = live
        .parent()
        .ok_or_else(|| FrameworkError::InvalidPackage {
            id: live.display().to_string(),
            reason: "package root has no parent".to_owned(),
        })?;
    let name =
        live.file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| FrameworkError::InvalidPackage {
                id: live.display().to_string(),
                reason: "package root has no UTF-8 name".to_owned(),
            })?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(parent.join(format!("{prefix}{name}--{nonce}")))
}

pub(super) fn uninstall_tombstone_original_name(path: &Path, prefix: &str) -> Option<String> {
    let name = path.file_name()?.to_str()?.strip_prefix(prefix)?;
    let (original, nonce) = name.rsplit_once("--")?;
    (!original.is_empty()
        && nonce.bytes().all(|byte| byte.is_ascii_digit())
        && is_valid_framework(original))
    .then(|| original.to_owned())
}

pub(super) fn set_framework_tree_readonly(
    path: &Path,
    readonly: bool,
) -> Result<(), FrameworkError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(FrameworkError::Io(error)),
    };
    if metadata_has_link_semantics(&metadata) {
        return Err(FrameworkError::InvalidPackage {
            id: path.display().to_string(),
            reason: "refusing to change permissions through a filesystem link".to_owned(),
        });
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            set_framework_tree_readonly(&entry?.path(), readonly)?;
        }
    }
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        permissions.set_mode(if readonly {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

pub(super) fn move_framework_tree_with_retry(source: &Path, target: &Path) -> std::io::Result<()> {
    const ATTEMPTS: usize = 40;
    for attempt in 0..ATTEMPTS {
        match fs::rename(source, target) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && error.kind() == std::io::ErrorKind::PermissionDenied
                    && attempt + 1 < ATTEMPTS =>
            {
                // A scanner can briefly hold a freshly extracted executable open. Retrying the
                // same directory rename preserves the atomic install boundary.
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the final rename attempt always returns")
}

pub(super) fn remove_framework_tree(path: &Path) -> Result<(), FrameworkError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(FrameworkError::Io(error)),
    };
    if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
        return Err(FrameworkError::InvalidPackage {
            id: path.display().to_string(),
            reason: "refusing to remove a tree through a filesystem link or non-directory"
                .to_owned(),
        });
    }
    set_framework_tree_readonly(path, false)?;
    fs::remove_dir_all(path)?;
    Ok(())
}

pub(super) fn is_safe_framework_version_path(value: &str) -> bool {
    let path = Path::new(value);
    if path.is_absolute() {
        return false;
    }
    let mut components = path.components();
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(Component::Normal(root)), Some(Component::Normal(_)), None)
            if root == OsStr::new(FRAMEWORK_VERSIONS_DIR)
    )
}

pub(super) fn framework_activation_is_safe(activation: &FrameworkActivationState) -> bool {
    is_safe_framework_version_path(&activation.active)
        && activation
            .previous
            .as_deref()
            .is_none_or(is_safe_framework_version_path)
}

pub(super) fn framework_lifecycle_journal_is_safe(journal: &FrameworkLifecycleJournal) -> bool {
    is_safe_framework_version_path(&journal.target)
        && framework_activation_is_safe(&journal.next_activation)
        && journal
            .old_activation
            .as_ref()
            .is_none_or(framework_activation_is_safe)
}

pub(super) fn framework_history_limit() -> usize {
    std::env::var("LOOM_PLUGIN_VERSION_HISTORY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(2)
}

pub(super) fn prune_framework_versions(
    package_root: &Path,
    activation: &FrameworkActivationState,
) -> Result<(), FrameworkError> {
    let versions_root = package_root.join(FRAMEWORK_VERSIONS_DIR);
    if !is_directory_without_links(&versions_root)? {
        return Ok(());
    }
    let keep_limit = framework_history_limit();
    let mut entries = fs::read_dir(&versions_root)?
        .filter_map(Result::ok)
        .filter(|entry| {
            fs::symlink_metadata(entry.path())
                .map(|metadata| metadata.is_dir() && !metadata_has_link_semantics(&metadata))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    let active = package_root.join(&activation.active);
    let previous = activation
        .previous
        .as_ref()
        .map(|path| package_root.join(path));
    let mut retained = 0usize;
    for entry in entries {
        let path = entry.path();
        let pinned = path == active || previous.as_ref().is_some_and(|previous| *previous == path);
        if pinned || retained < keep_limit.saturating_sub(2) {
            if !pinned {
                retained += 1;
            }
            continue;
        }
        remove_framework_tree(&path)?;
    }
    Ok(())
}

pub(super) fn sanitize_version_for_path(version: &str) -> String {
    let sanitized = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_owned()
    } else {
        sanitized
    }
}
