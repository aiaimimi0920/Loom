// Link-safe bounded reads and crash-safe output replacement for CLI-owned files.
fn is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
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

fn ensure_real_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} {}", path.display()))?;
    if !metadata.file_type().is_dir() || is_reparse_or_symlink(&metadata) {
        bail!("{label} must be a real directory, not a link: {}", path.display());
    }
    fs::canonicalize(path).with_context(|| format!("canonicalize {label} {}", path.display()))
}

fn open_regular_file(path: &Path, label: &str) -> Result<File> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} {}", path.display()))?;
    if !metadata.file_type().is_file() || is_reparse_or_symlink(&metadata) {
        bail!("{label} must be a regular file, not a link: {}", path.display());
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        };
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .with_context(|| format!("open {label} {}", path.display()))?;
    let opened_metadata = file
        .metadata()
        .with_context(|| format!("inspect opened {label} {}", path.display()))?;
    if !opened_metadata.file_type().is_file() || is_reparse_or_symlink(&opened_metadata) {
        bail!("{label} changed while it was being opened: {}", path.display());
    }
    Ok(file)
}

fn validate_contained_regular_file(root: &Path, relative: &Path) -> Result<PathBuf> {
    let canonical_root = ensure_real_directory(root, "package root")?;
    let mut current = canonical_root.clone();
    let components = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_owned()),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        bail!("package path has no file name: {}", relative.display());
    }
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("inspect package path {}", current.display()))?;
        if is_reparse_or_symlink(&metadata) {
            bail!("links are not allowed in plugin packages: {}", current.display());
        }
        let is_last = index + 1 == components.len();
        if is_last && !metadata.file_type().is_file() {
            bail!("package path is not a regular file: {}", current.display());
        }
        if !is_last && !metadata.file_type().is_dir() {
            bail!("package path parent is not a directory: {}", current.display());
        }
    }
    let canonical_path = fs::canonicalize(&current)
        .with_context(|| format!("canonicalize package path {}", current.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("package path escapes its root: {}", relative.display());
    }
    Ok(canonical_path)
}

fn open_contained_regular_file(root: &Path, relative: &Path) -> Result<File> {
    let path = validate_contained_regular_file(root, relative)?;
    open_regular_file(&path, "package file")
}

fn contained_regular_file_exists(root: &Path, relative: &Path) -> Result<bool> {
    let path = root.join(relative);
    match fs::symlink_metadata(&path) {
        Ok(_) => validate_contained_regular_file(root, relative).map(|_| true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn read_bounded_regular_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut file = open_regular_file(path, "JSON document")?;
    let declared = file.metadata()?.len();
    if declared > limit {
        bail!("{} exceeds the {limit}-byte limit", path.display());
    }
    let mut bytes = Vec::with_capacity(declared.min(limit) as usize);
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("{} exceeds the {limit}-byte limit", path.display());
    }
    Ok(bytes)
}

fn ensure_safe_destination(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || is_reparse_or_symlink(&metadata) {
                bail!("output must not replace a link or non-file: {}", path.display());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    }
    Ok(())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    write_bytes_atomic_with_privacy(path, bytes, false)
}

fn write_private_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    write_bytes_atomic_with_privacy(path, bytes, true)
}

fn write_bytes_atomic_with_privacy(path: &Path, bytes: &[u8], private: bool) -> Result<()> {
    ensure_safe_destination(path)?;
    let (temporary, mut file) = create_atomic_temporary(path)?;
    let result = (|| -> Result<()> {
        if private {
            restrict_private_path_permissions(&temporary, false)?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file_atomic(&temporary, path)?;
        if private {
            restrict_private_path_permissions(path, false)?;
        }
        sync_parent_directory(path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("atomically write {}", path.display()))
}

fn path_parent_or_current(path: &Path) -> Result<&Path> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent),
        Some(_) => Ok(Path::new(".")),
        None => bail!("path has no parent: {}", path.display()),
    }
}

fn create_atomic_temporary(path: &Path) -> Result<(PathBuf, File)> {
    let parent = path_parent_or_current(path)?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    ensure_real_directory(parent, "output parent")?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("output path has no file name: {}", path.display()))?
        .to_string_lossy();
    for attempt in 0..100u32 {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{nonce}-{sequence}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("could not allocate a unique temporary file for {}", path.display())
}

fn lexical_absolute(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    bail!("path escapes its filesystem root: {}", path.display());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn canonicalize_for_creation(path: &Path) -> Result<PathBuf> {
    let mut probe = lexical_absolute(path)?;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&probe) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(&probe)?;
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = probe
                    .file_name()
                    .ok_or_else(|| anyhow!("path has no existing ancestor: {}", path.display()))?
                    .to_owned();
                missing.push(name);
                if !probe.pop() {
                    bail!("path has no existing ancestor: {}", path.display());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn ensure_output_outside_source(source: &Path, output: &Path) -> Result<()> {
    let source_lexical = lexical_absolute(source)?;
    let source_canonical = ensure_real_directory(source, "package root")?;
    let output_lexical = lexical_absolute(output)?;
    let output_resolved = canonicalize_for_creation(output)?;
    if output_lexical.starts_with(&source_lexical)
        || output_resolved.starts_with(&source_canonical)
    {
        bail!("package output must be outside the source directory: {}", output.display());
    }
    Ok(())
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
                let parent = path.parent().filter(|value| !value.as_os_str().is_empty()).unwrap_or(Path::new("."));
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
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty()).unwrap_or(Path::new("."));
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
