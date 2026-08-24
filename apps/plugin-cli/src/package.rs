// Deterministic package collection, ZIP creation and digest sidecars.
fn validate_relative_package_path(
    directory: &Path,
    value: &str,
    require_payload: bool,
) -> Result<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("path must remain inside the package: {value}");
    }
    if require_payload {
        validate_contained_regular_file(directory, path)
            .with_context(|| format!("validate package path {value}"))?;
    }
    Ok(())
}

fn validate_art_runtime_command(
    directory: &Path,
    value: &str,
    require_payload: bool,
) -> Result<()> {
    let path = Path::new(value);
    validate_relative_package_path(directory, value, false)?;
    if require_payload && path.components().count() > 1 {
        validate_contained_regular_file(directory, path)
            .with_context(|| format!("validate package path {value}"))?;
    }
    Ok(())
}

fn is_safe_package_reference(reference: &str) -> bool {
    reference
        .split_once('/')
        .map(|(publisher, id)| {
            is_safe_publisher_id(publisher) && !id.contains('/') && is_safe_package_id(id)
        })
        .unwrap_or_else(|| is_safe_package_id(reference))
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = read_bounded_regular_file(path, MAX_MANIFEST_BYTES)
        .with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn pack_directory(source: &Path, output: &Path) -> Result<String> {
    ensure_output_outside_source(source, output)?;
    let digest_path = package_digest_path(output);
    ensure_safe_destination(output)?;
    ensure_safe_destination(&digest_path)?;
    let files = collect_package_files(source)?;
    validate_path_with_payload_after_tree_inspection(
        source,
        true,
        &TrustStore::default(),
        true,
    )?;
    let (temporary, file) = create_atomic_temporary(output)?;
    let result = (|| -> Result<(String, u64)> {
        let mut archive = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        let mut streamed_total = 0u64;
        for relative in &files {
            let name = relative.to_string_lossy().replace('\\', "/");
            archive.start_file(&name, options)?;
            let mut input = open_contained_regular_file(source, relative)?;
            let remaining = MAX_PACKAGE_BYTES.saturating_sub(streamed_total);
            let copied = std::io::copy(
                &mut Read::by_ref(&mut input).take(remaining + 1),
                &mut archive,
            )?;
            if copied > remaining {
                bail!("package exceeds {MAX_PACKAGE_BYTES} uncompressed bytes while packing");
            }
            streamed_total += copied;
        }
        archive.finish()?.sync_all()?;
        digest_regular_file(&temporary)
    })();
    let (digest, archive_bytes) = match result {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("build package archive");
        }
    };
    if let Err(error) = replace_file_atomic(&temporary, output) {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("replace {}", output.display()));
    }
    sync_parent_directory(output)?;
    write_bytes_atomic(
        &digest_path,
        format!(
            "{digest}  {}\n",
            output.file_name().unwrap_or_default().to_string_lossy()
        )
        .as_bytes(),
    )?;
    Ok(format!(
        "package built: {} files, {} bytes, sha256={digest}",
        files.len(),
        archive_bytes
    ))
}

fn collect_package_files(root: &Path) -> Result<Vec<PathBuf>> {
    let canonical_root = ensure_real_directory(root, "package root")?;
    let mut pending = vec![canonical_root.clone()];
    let mut files = Vec::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("read package directory {}", directory.display()))?
        {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if is_reparse_or_symlink(&metadata) {
                bail!(
                    "links are not allowed in plugin packages: {}",
                    entry.path().display()
                );
            }
            if metadata.file_type().is_dir() {
                pending.push(entry.path());
            } else if metadata.file_type().is_file() {
                total = total.saturating_add(metadata.len());
                if total > MAX_PACKAGE_BYTES {
                    bail!("package exceeds {MAX_PACKAGE_BYTES} uncompressed bytes");
                }
                files.push(entry.path().strip_prefix(&canonical_root)?.to_path_buf());
                if files.len() > MAX_PACKAGE_FILES {
                    bail!("package exceeds {MAX_PACKAGE_FILES} files");
                }
            } else {
                bail!("special files are not allowed in plugin packages: {}", entry.path().display());
            }
        }
    }
    files.sort_by(|left, right| {
        left.to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.to_string_lossy().to_ascii_lowercase())
    });
    for pair in files.windows(2) {
        let left = pair[0]
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        let right = pair[1]
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if left == right {
            bail!("case-insensitive package path collision: {left}");
        }
    }
    Ok(files)
}

fn package_digest_path(output: &Path) -> PathBuf {
    output.with_extension(format!(
        "{}sha256",
        output
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{value}."))
            .unwrap_or_default()
    ))
}

fn digest_regular_file(path: &Path) -> Result<(String, u64)> {
    let mut file = open_regular_file(path, "package archive")?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        total += read as u64;
    }
    let digest = digest.finalize();
    Ok((
        digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect(),
        total,
    ))
}
