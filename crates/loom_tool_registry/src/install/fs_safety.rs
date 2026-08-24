use super::*;

pub(super) fn metadata_has_link_semantics(metadata: &std::fs::Metadata) -> bool {
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

fn is_directory_without_links(path: &Path) -> Result<bool, ArtInstallError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir() && !metadata_has_link_semantics(&metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ArtInstallError::Io(error)),
    }
}

pub(super) fn set_tree_readonly(path: &Path, readonly: bool) -> Result<(), ArtInstallError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ArtInstallError::Io(error)),
    };
    if metadata_has_link_semantics(&metadata) {
        return Err(ArtInstallError::InvalidPackage(
            "refusing to change permissions through a filesystem link".to_owned(),
        ));
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            set_tree_readonly(&entry?.path(), readonly)?;
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
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

pub(super) fn uninstall_tombstone_path(
    live: &Path,
    prefix: &str,
) -> Result<PathBuf, ArtInstallError> {
    let parent = live.parent().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package root has no parent".to_owned())
    })?;
    let name = live.file_name().and_then(OsStr::to_str).ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package root has no UTF-8 name".to_owned())
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
        && is_safe_art_id(original))
    .then(|| original.to_owned())
}

pub(super) fn remove_tree(path: &Path) -> Result<(), ArtInstallError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ArtInstallError::Io(error)),
    };
    if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
        return Err(ArtInstallError::InvalidPackage(
            "refusing to remove a tree through a filesystem link or non-directory".to_owned(),
        ));
    }
    set_tree_readonly(path, false)?;
    std::fs::remove_dir_all(path)?;
    Ok(())
}

pub(super) fn art_history_limit() -> usize {
    std::env::var("LOOM_PLUGIN_VERSION_HISTORY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(2)
}

pub(super) fn prune_art_versions(
    art_root: &Path,
    activation: &ArtActivationState,
) -> Result<(), ArtInstallError> {
    let versions_root = art_root.join("versions");
    if !is_directory_without_links(&versions_root)? {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(&versions_root)?
        .filter_map(Result::ok)
        .filter(|entry| {
            std::fs::symlink_metadata(entry.path())
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
    let active = art_root.join(&activation.active.path);
    let previous = activation
        .previous
        .as_ref()
        .map(|pointer| art_root.join(&pointer.path));
    let mut extra_retained = 0usize;
    for entry in entries {
        let path = entry.path();
        let pinned = path == active || previous.as_ref().is_some_and(|previous| *previous == path);
        if pinned || extra_retained < art_history_limit().saturating_sub(2) {
            if !pinned {
                extra_retained += 1;
            }
            continue;
        }
        remove_tree(&path)?;
    }
    Ok(())
}

pub(super) fn read_art_activation(path: &Path) -> Option<ArtActivationState> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub(super) fn write_art_activation(
    path: &Path,
    activation: &ArtActivationState,
) -> Result<(), ArtInstallError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(activation)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    crate::replace_registry_file(&temporary, path)?;
    Ok(())
}

pub(super) fn write_art_lifecycle(
    art_root: &Path,
    journal: &ArtLifecycleJournal,
) -> Result<(), ArtInstallError> {
    let path = art_root.join(ART_LIFECYCLE_FILE);
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(journal)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    crate::replace_registry_file(&temporary, &path)?;
    Ok(())
}

pub(super) fn clear_art_lifecycle(art_root: &Path) {
    let _ = std::fs::remove_file(art_root.join(ART_LIFECYCLE_FILE));
}

pub(super) fn is_safe_art_version_path(value: &str) -> bool {
    let path = Path::new(value);
    if path.is_absolute() {
        return false;
    }
    let mut components = path.components();
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(Component::Normal(root)), Some(Component::Normal(_)), None)
            if root == OsStr::new("versions")
    )
}

pub(super) fn art_activation_is_safe(activation: &ArtActivationState) -> bool {
    is_safe_art_version_path(&activation.active.path)
        && activation
            .previous
            .as_ref()
            .map(|pointer| is_safe_art_version_path(&pointer.path))
            .unwrap_or(true)
}

pub(super) fn art_lifecycle_journal_is_safe(journal: &ArtLifecycleJournal) -> bool {
    is_safe_art_version_path(&journal.target)
        && art_activation_is_safe(&journal.next_activation)
        && journal
            .old_activation
            .as_ref()
            .map(art_activation_is_safe)
            .unwrap_or(true)
}

pub fn recover_art_lifecycle(control_plane_root: &Path) -> Result<(), ArtInstallError> {
    let arts_root = control_plane_root.join("arts");
    if !is_directory_without_links(&arts_root)? {
        return Ok(());
    }
    let mut roots = Vec::new();
    for first in std::fs::read_dir(&arts_root)? {
        let first = first?.path();
        if !is_directory_without_links(&first)?
            || !first
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(loom_protocol::is_safe_publisher_id)
        {
            continue;
        }
        for second in std::fs::read_dir(&first).into_iter().flatten().flatten() {
            let second = second.path();
            if is_directory_without_links(&second)? && second.join(ART_LIFECYCLE_FILE).is_file() {
                roots.push(second);
            }
        }
    }
    for art_root in roots {
        let journal_path = art_root.join(ART_LIFECYCLE_FILE);
        let journal: ArtLifecycleJournal =
            match serde_json::from_slice(&std::fs::read(&journal_path)?) {
                Ok(journal) => journal,
                Err(_) => {
                    let _ = std::fs::rename(&journal_path, journal_path.with_extension("corrupt"));
                    continue;
                }
            };
        if !art_lifecycle_journal_is_safe(&journal) {
            let _ = std::fs::rename(&journal_path, journal_path.with_extension("corrupt"));
            continue;
        }
        let activation_path = art_root.join("active.json");
        let current = read_art_activation(&activation_path);
        if current.as_ref() != Some(&journal.next_activation) {
            if let Some(old) = &journal.old_activation {
                write_art_activation(&activation_path, old)?;
            } else {
                let _ = std::fs::remove_file(&activation_path);
            }
            // Only a directory this operation created may be removed. Reused and older directories
            // hold versions that existed before the interrupted operation, and the activation just
            // restored above may well point at one of them.
            if journal.created_target {
                let target = art_root.join(&journal.target);
                let _ = remove_tree(&target);
            }
        }
        let _ = std::fs::remove_file(journal_path);
    }
    Ok(())
}

pub fn recover_art_uninstall_tombstones(control_plane_root: &Path) -> Result<(), ArtInstallError> {
    let arts_root = control_plane_root.join("arts");
    if !is_directory_without_links(&arts_root)? {
        return Ok(());
    }
    let mut parents = Vec::new();
    for entry in std::fs::read_dir(&arts_root)? {
        let path = entry?.path();
        if is_directory_without_links(&path)?
            && path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(loom_protocol::is_safe_publisher_id)
        {
            parents.push(path);
        }
    }
    let registry = ToolRegistry::new(control_plane_root.join("tools"));
    for parent in parents {
        for entry in std::fs::read_dir(&parent)? {
            let tombstone = entry?.path();
            if !is_directory_without_links(&tombstone)? {
                continue;
            }
            let Some(original_name) =
                uninstall_tombstone_original_name(&tombstone, ART_UNINSTALL_TOMBSTONE_PREFIX)
            else {
                continue;
            };
            let Some(publisher) = parent.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            let reference = format!("{publisher}/{original_name}");
            if !is_safe_art_reference(&reference) {
                continue;
            }
            let installed = registry
                .get_tool(&reference)
                .map_err(|error| ArtInstallError::Registry(error.to_string()))?
                .is_some();
            let live = parent.join(&original_name);
            if installed && !live.exists() {
                std::fs::rename(&tombstone, &live)?;
            } else {
                remove_tree(&tombstone)?;
            }
        }
    }
    Ok(())
}
