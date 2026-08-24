use super::*;

/// Package an art into a publishable zip: a `manifest.json` (the ToolDefinition)
/// plus every file in the active immutable Art package directory.
/// Inverse of `install_art_from_zip`. Returns the zip bytes.
pub fn package_art_to_zip(
    tool: &ToolDefinition,
    art_dir: &Path,
) -> Result<Vec<u8>, ArtInstallError> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        // manifest.json — the ToolDefinition.
        writer
            .start_file(MANIFEST_NAME, opts)
            .map_err(ArtInstallError::Zip)?;
        let manifest = serde_json::to_vec_pretty(tool)?;
        writer.write_all(&manifest)?;
        // Bundle the art resource dir, if present.
        if art_dir.is_dir() {
            add_dir_to_zip(&mut writer, art_dir, art_dir, opts)?;
        }
        writer.finish().map_err(ArtInstallError::Zip)?;
    }
    Ok(buf)
}

pub fn package_signed_art_to_zip(
    tool: &ToolDefinition,
    art_dir: &Path,
    publisher_id: &str,
    key: &SigningKeyDocument,
) -> Result<Vec<u8>, ArtInstallError> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging =
        std::env::temp_dir().join(format!("loom-art-sign-{}-{nonce}", std::process::id()));
    let result = (|| {
        std::fs::create_dir_all(&staging)?;
        if art_dir.is_dir() {
            copy_art_resources_for_signing(art_dir, art_dir, &staging)?;
        }
        let mut signed_tool = tool.clone();
        let metadata = signed_tool
            .metadata
            .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !metadata.is_object() {
            *metadata = serde_json::Value::Object(serde_json::Map::new());
        }
        let metadata = metadata
            .as_object_mut()
            .expect("Art metadata was normalized to an object");
        let security = metadata
            .entry("packageSecurity".to_owned())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !security.is_object() {
            *security = serde_json::Value::Object(serde_json::Map::new());
        }
        let security = security
            .as_object_mut()
            .expect("Art package security was normalized to an object");
        security.insert(
            "publisher".to_owned(),
            serde_json::json!({ "id": publisher_id, "keyId": key.key_id }),
        );
        security.insert(
            "signature".to_owned(),
            serde_json::json!({
                "algorithm": "ed25519",
                "keyId": key.key_id,
                "file": "signature.json"
            }),
        );
        std::fs::write(
            staging.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&signed_tool)?,
        )?;
        sign_package(&staging, "signature.json", key)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        package_art_to_zip(&signed_tool, &staging)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

pub(super) fn copy_art_resources_for_signing(
    base: &Path,
    directory: &Path,
    staging: &Path,
) -> Result<(), ArtInstallError> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(ArtInstallError::InvalidPackage(format!(
                "signed Art resources cannot contain symbolic links: {}",
                entry.path().display()
            )));
        }
        let path = entry.path();
        let relative = path.strip_prefix(base).map_err(|_| {
            ArtInstallError::InvalidPackage("Art resource path escaped its package root".to_owned())
        })?;
        if relative == Path::new(MANIFEST_NAME) || relative == Path::new("signature.json") {
            continue;
        }
        let target = staging.join(relative);
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_art_resources_for_signing(base, &path, staging)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, target)?;
        }
    }
    Ok(())
}

/// Build a newly-authored Art package without requiring a pre-existing package
/// directory. The resulting ZIP is consumed by the same secure installer as
/// imported packages, so authoring cannot bypass validation or activation.
pub fn build_authored_art_package_zip(
    tool: &ToolDefinition,
    runtime: Option<&ArtRuntimeManifest>,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, ArtInstallError> {
    use std::io::Write;
    let mut tool = tool.clone();
    inject_declared_art_qualified_id(&mut tool)?;
    tool.validate()
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file(MANIFEST_NAME, options)?;
        writer.write_all(&serde_json::to_vec_pretty(&tool)?)?;
        if let Some(runtime) = runtime {
            writer.start_file("art.runtime.json", options)?;
            writer.write_all(&serde_json::to_vec_pretty(runtime)?)?;
        }
        let mut written = std::collections::BTreeSet::new();
        for (path, content) in files {
            let normalized = path.replace('\\', "/");
            let candidate = Path::new(&normalized);
            if normalized.is_empty()
                || normalized == MANIFEST_NAME
                || normalized == "art.runtime.json"
                || candidate.is_absolute()
                || candidate.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
                || !written.insert(normalized.clone())
            {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "invalid authored Art file path: {path}"
                )));
            }
            writer.start_file(normalized, options)?;
            writer.write_all(content)?;
        }
        writer.finish()?;
    }
    Ok(bytes)
}

pub(super) fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    base: &Path,
    dir: &Path,
    opts: zip::write::FileOptions<'_, ()>,
) -> Result<(), ArtInstallError> {
    use std::io::Write;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(writer, base, &path, opts)?;
        } else {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let name = rel.to_string_lossy().replace('\\', "/");
            // manifest.json is written explicitly from the ToolDefinition; skip
            // any copy left in the art dir to avoid a duplicate zip entry.
            if name == MANIFEST_NAME {
                continue;
            }
            writer
                .start_file(name, opts)
                .map_err(ArtInstallError::Zip)?;
            let bytes = std::fs::read(&path)?;
            writer.write_all(&bytes)?;
        }
    }
    Ok(())
}
