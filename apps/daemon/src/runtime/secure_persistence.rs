// Structured errors, sensitive atomic files, daemon tokens, paths, and legacy ACL repair.
fn handle_capability_manifest_error(address: SocketAddr, error: anyhow::Error) -> Result<()> {
    if address.ip().is_loopback() {
        eprintln!("[WARN] Loom 本地服务已启动，但无法更新客户端发现清单：{error:#}");
        return Ok(());
    }
    Err(error).context("publish Loom capability manifest for non-loopback daemon")
}

fn create_sensitive_temporary(path: &Path) -> std::io::Result<(PathBuf, fs::File)> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "sensitive file path has no parent",
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "sensitive file path has no UTF-8 file name",
            )
        })?;
    for attempt in 0..100u32 {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{nonce}-{attempt}",
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
        "could not allocate a unique sensitive temporary file",
    ))
}

#[cfg(not(windows))]
fn replace_sensitive_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_sensitive_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = extended_windows_path(source)?;
    let destination = extended_windows_path(destination)?;
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

fn restrict_sensitive_path_permissions(_path: &Path, _directory: bool) -> std::io::Result<()> {
    loom_plugin_security::restrict_private_path_permissions(_path, _directory)
}

#[cfg(windows)]
fn extended_windows_path(path: &Path) -> std::io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    let absolute = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                std::io::Error::new(ErrorKind::InvalidInput, "Windows path has no parent")
            })?;
            let file_name = path.file_name().ok_or_else(|| {
                std::io::Error::new(ErrorKind::InvalidInput, "Windows path has no file name")
            })?;
            fs::canonicalize(parent)?.join(file_name)
        }
        Err(error) => return Err(error),
    };
    let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut extended = if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
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

#[cfg(unix)]
fn sync_sensitive_parent(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "sensitive path has no parent")
    })?;
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_sensitive_parent(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Whether an atomic write should narrow the replaced file's permissions or carry the previous
/// file's permissions forward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicWritePermissions {
    /// The daemon owns the file. Both the temporary and the replacement are restricted to this
    /// user, the same sequence `write_local_capability_manifest` uses.
    Restrict,
    /// Another process owns the file — Hook's canvas is the only such case. The write is still
    /// atomic, but an existing file's permissions are carried onto the replacement instead of
    /// being narrowed: tightening the ACL on a file this daemon does not own is a policy change,
    /// not part of a crash-safety fix.
    Preserve,
}

/// Serialize `value` as pretty JSON and replace `path` with it atomically.
///
/// Every persistence site in this file that owns its file should go through here rather than
/// calling `fs::write`, which truncates the destination in place: a crash, power loss or full
/// disk during a bare `fs::write` leaves a half-written file, and every loader downstream then
/// has to decide what to do with unparsable bytes. The sequence is temporary file, `sync_all`,
/// restrict, atomic replace, restrict, flush the parent — so a reader either sees the previous
/// complete file or the new complete file, never a partial one.
fn write_json_atomically<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .with_context(|| format!("serialize JSON for `{}`", path.display()))?;
    bytes.push(b'\n');
    write_bytes_atomically(path, &bytes, AtomicWritePermissions::Restrict)
}

/// The byte-level half of `write_json_atomically`, for callers that own their serialization —
/// Hook's canvas is written compactly and its exact bytes are returned to the caller.
fn write_bytes_atomically(
    path: &Path,
    bytes: &[u8],
    permissions: AtomicWritePermissions,
) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("atomic write target `{}` has no parent", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create directory `{}` for atomic write", parent.display()))?;
    let (temporary, mut file) = create_sensitive_temporary(path).with_context(|| {
        format!(
            "create temporary for `{}` in `{}`",
            path.display(),
            parent.display()
        )
    })?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)
            .with_context(|| format!("write temporary `{}`", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flush temporary `{}`", temporary.display()))?;
        drop(file);
        match permissions {
            AtomicWritePermissions::Restrict => {
                restrict_sensitive_path_permissions(&temporary, false).with_context(|| {
                    format!("restrict temporary permissions `{}`", temporary.display())
                })?;
                if path.is_file() {
                    restrict_sensitive_path_permissions(path, false).with_context(|| {
                        format!(
                            "refresh permissions before replacement `{}`",
                            path.display()
                        )
                    })?;
                }
            }
            AtomicWritePermissions::Preserve => {
                // `create_sensitive_temporary` opens the temporary as 0o600 on unix, so without
                // this the replacement would silently narrow a file the daemon does not own. A
                // failure here is not worth losing the write over: the bytes still land, with the
                // permissions a new file in this directory would have.
                if let Ok(metadata) = fs::metadata(path) {
                    let _ = fs::set_permissions(&temporary, metadata.permissions());
                }
            }
        }
        replace_sensitive_file_with_retry(&temporary, path)
            .with_context(|| format!("atomically replace `{}`", path.display()))?;
        if permissions == AtomicWritePermissions::Restrict {
            restrict_sensitive_path_permissions(path, false)
                .with_context(|| format!("restrict permissions `{}`", path.display()))?;
        }
        sync_sensitive_parent(path)
            .with_context(|| format!("flush directory `{}`", parent.display()))?;
        Ok(())
    })();
    if result.is_err() {
        // The destination still holds its previous contents; drop the partial temporary so a
        // failed write leaves nothing behind for the next reader or the next `create_dir_all`.
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// `replace_sensitive_file`, retried briefly before giving up.
///
/// On Windows a rename-with-replace fails with `ERROR_ACCESS_DENIED` while another handle holds the
/// destination open, which happens whenever a reader, an editor or a virus scanner touches the file
/// at the wrong moment. The replacement is the one step of an atomic write that cannot be skipped
/// and the conflict window is milliseconds wide, so a short bounded retry turns a spurious failure
/// into a success without weakening the guarantee — each attempt is still a single atomic
/// replacement. A destination that genuinely cannot be replaced still fails, one backoff later.
fn replace_sensitive_file_with_retry(source: &Path, destination: &Path) -> std::io::Result<()> {
    const ATTEMPTS: u32 = 20;
    let mut outcome = replace_sensitive_file(source, destination);
    let mut attempt = 1;
    while outcome.is_err() && attempt < ATTEMPTS {
        std::thread::sleep(Duration::from_millis(5));
        outcome = replace_sensitive_file(source, destination);
        attempt += 1;
    }
    outcome
}

/// Move an unparsable file aside so its bytes survive for recovery, and return the path it went
/// to. Used where refusing to start would be worse than continuing with defaults; anything that
/// carries authorization state should refuse instead — see `DeviceRegistryStore::new`.
fn quarantine_unreadable_file(path: &Path, reason: &str) -> Option<PathBuf> {
    let file_name = path.file_name().and_then(|name| name.to_str())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let quarantined = path.with_file_name(format!("{file_name}.corrupt-{stamp}"));
    match fs::rename(path, &quarantined) {
        Ok(()) => {
            eprintln!(
                "[WARN] loom moved an unreadable file aside: `{}` -> `{}` ({reason})",
                path.display(),
                quarantined.display()
            );
            Some(quarantined)
        }
        Err(error) => {
            eprintln!(
                "[WARN] loom could not move the unreadable file `{}` aside ({reason}): {error}",
                path.display()
            );
            None
        }
    }
}

fn is_loopback_bind_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn normalize_daemon_auth_token(value: &str, source: &str) -> Result<String> {
    let token = value.trim();
    if token.is_empty() {
        anyhow::bail!("Loom daemon auth token from {source} is empty");
    }
    if token.len() > 4096 || token.bytes().any(|byte| byte <= b' ' || byte == 0x7f) {
        anyhow::bail!(
            "Loom daemon auth token from {source} contains whitespace, control bytes, or is too long"
        );
    }
    Ok(token.to_owned())
}

fn daemon_auth_token_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join(DAEMON_AUTH_TOKEN_FILE)
}

fn resolve_daemon_auth_token(
    configured_token: Option<String>,
    control_plane_root: &Path,
) -> Result<String> {
    if let Some(token) = configured_token {
        return normalize_daemon_auth_token(&token, "DaemonConfig");
    }
    if let Some(token) = std::env::var_os("LOOM_DAEMON_TOKEN").filter(|token| !token.is_empty()) {
        return normalize_daemon_auth_token(&token.to_string_lossy(), "LOOM_DAEMON_TOKEN");
    }

    let path = daemon_auth_token_path(control_plane_root);
    match fs::read_to_string(&path) {
        Ok(token) => {
            let token = normalize_daemon_auth_token(
                &token,
                &format!("persisted token file `{}`", path.display()),
            )?;
            restrict_sensitive_path_permissions(&path, false).with_context(|| {
                format!("restrict Loom daemon auth token file `{}`", path.display())
            })?;
            Ok(token)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut bytes = [0_u8; 32];
            OsRng.fill_bytes(&mut bytes);
            let token = BASE64_URL.encode(bytes);
            write_bytes_atomically(&path, token.as_bytes(), AtomicWritePermissions::Restrict)
                .with_context(|| {
                    format!(
                        "generate and persist Loom daemon auth token `{}`",
                        path.display()
                    )
                })?;
            Ok(token)
        }
        Err(error) => Err(error)
            .with_context(|| format!("read Loom daemon auth token file `{}`", path.display())),
    }
}

fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn settings_url_with_token(url: &str, token: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!(
        "{}{separator}token={}",
        url.trim(),
        percent_encode_query_value(token)
    )
}

fn default_control_plane_root() -> PathBuf {
    if let Some(path) = std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return path;
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.join("Loom").join("control-plane"))
        .unwrap_or_else(|| PathBuf::from(".runtime").join("loom").join("control-plane"))
}

#[cfg(windows)]
fn repair_legacy_control_plane_permissions(root: &Path) -> std::io::Result<()> {
    static REPAIR_LOCK: Mutex<()> = Mutex::new(());

    let _repair_guard = REPAIR_LOCK.lock().map_err(|_| {
        std::io::Error::other("Loom control-plane permission repair lock was poisoned")
    })?;
    repair_legacy_control_plane_permissions_with(
        root,
        loom_plugin_security::restrict_private_path_permissions,
        loom_plugin_security::repair_private_tree_permissions,
    )
}

#[cfg(windows)]
const ACL_MIGRATION_MARKER: &str = "windows-private-acl-v2";

#[cfg(windows)]
fn acl_migration_marker_is_complete(body: &str) -> bool {
    let mut lines = body.lines();
    lines.next() == Some("2 skipped=0") && lines.all(|line| line.trim().is_empty())
}

#[cfg(windows)]
fn repair_legacy_control_plane_permissions_with<Restrict, RepairTree>(
    root: &Path,
    mut restrict: Restrict,
    mut repair_tree: RepairTree,
) -> std::io::Result<()>
where
    Restrict: FnMut(&Path, bool) -> std::io::Result<()>,
    RepairTree: FnMut(&Path) -> std::io::Result<Vec<PathBuf>>,
{
    match restrict(root, true) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }

    let migration_dir = root.join("migrations");
    match restrict(&migration_dir, true) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let marker = migration_dir.join(ACL_MIGRATION_MARKER);
    let marker_requires_retry = match fs::read_to_string(&marker) {
        Ok(body) if acl_migration_marker_is_complete(&body) => return Ok(()),
        Ok(_) => true,
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };

    // These stores were the only writers that applied the legacy private ACL
    // to the control-plane root. Repair their files before testing existence,
    // because ordinary metadata calls can themselves fail under that ACL.
    let mut legacy_private_store_found = false;
    let mut skipped = BTreeSet::new();
    for file_name in ["plugin-trust.json", "plugin-credentials.json"] {
        let path = root.join(file_name);
        match restrict(&path, false) {
            Ok(()) => legacy_private_store_found = true,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                legacy_private_store_found = true;
                runtime_log_warn(format!(
                    "legacy control-plane ACL repair deferred unreadable entry `{}`: {error}",
                    path.display()
                ));
                skipped.insert(path);
            }
            Err(error) => return Err(error),
        }
    }
    if !legacy_private_store_found && !marker_requires_retry {
        return Ok(());
    }

    skipped.extend(repair_tree(root)?);
    if !skipped.is_empty() {
        runtime_log_info(format!(
            "repaired legacy control-plane ACL and left {} unreadable legacy entries untouched",
            skipped.len()
        ));
    }
    fs::create_dir_all(&migration_dir)?;
    // The marker's existence short-circuits every later run, so record *which* entries were
    // skipped and not just how many — a count alone leaves a future version nothing to re-attempt.
    // The write itself is atomic for the same reason every other persist site here is: a truncated
    // marker would either be read as "migration done" or leave the migration wedged.
    let mut body = format!("2 skipped={}\n", skipped.len());
    for entry in &skipped {
        body.push_str(&format!("skipped-path={}\n", entry.display()));
    }
    write_bytes_atomically(&marker, body.as_bytes(), AtomicWritePermissions::Restrict)
        .map_err(std::io::Error::other)
}

#[cfg(not(windows))]
fn repair_legacy_control_plane_permissions(_root: &Path) -> std::io::Result<()> {
    Ok(())
}
