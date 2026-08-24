use super::*;

pub(super) fn read_art_package_security(tool: &ToolDefinition) -> ArtPackageSecurityMetadata {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

pub(super) fn required_art_package_version(
    security: &ArtPackageSecurityMetadata,
) -> Result<&str, ArtInstallError> {
    let version = security
        .version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(
                "metadata.packageSecurity.version is required".to_owned(),
            )
        })?;
    semver::Version::parse(version).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "metadata.packageSecurity.version `{version}` is not valid SemVer: {error}"
        ))
    })?;
    Ok(version)
}

pub(super) fn is_safe_art_id(id: &str) -> bool {
    loom_protocol::is_safe_package_id(id)
}

pub(super) fn is_safe_art_reference(reference: &str) -> bool {
    if let Some((publisher, id)) = reference.split_once('/') {
        !publisher.contains('/')
            && loom_protocol::is_safe_publisher_id(publisher)
            && is_safe_art_id(id)
    } else {
        is_safe_art_id(reference)
    }
}

pub(super) fn art_root_for_reference(
    control_plane_root: &Path,
    reference: &str,
) -> Option<PathBuf> {
    if !is_safe_art_reference(reference) {
        return None;
    }
    let arts = control_plane_root.join("arts");
    reference
        .split_once('/')
        .map(|(publisher, id)| arts.join(publisher).join(id))
}

/// Read the `manifest.json` (a `ToolDefinition`) from an art package zip without
/// extracting anything. Testable in isolation.
pub fn read_manifest_from_zip(zip_bytes: &[u8]) -> Result<ToolDefinition, ArtInstallError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut file = archive
        .by_name(MANIFEST_NAME)
        .map_err(|_| ArtInstallError::MissingManifest)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let tool: ToolDefinition = serde_json::from_str(&text)?;
    Ok(tool)
}

pub(super) struct ArtPackagePaths<'a> {
    pub(super) qualified_id: &'a str,
    pub(super) art_dir: &'a Path,
    pub(super) state_dir: &'a Path,
    pub(super) cache_dir: &'a Path,
    pub(super) output_dir: &'a Path,
    pub(super) lockfile: &'a Path,
    pub(super) version: &'a str,
    pub(super) digest: &'a str,
    pub(super) trust_status: &'a PackageTrustStatus,
}

pub(super) fn record_art_package_directory(
    metadata: &mut Option<serde_json::Value>,
    paths: ArtPackagePaths<'_>,
) {
    let root = metadata.get_or_insert_with(|| serde_json::json!({}));
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    if let Some(object) = root.as_object_mut() {
        object.insert(
            "artPackage".to_owned(),
            serde_json::json!({
                "qualifiedId": paths.qualified_id,
                "dir": paths.art_dir.to_string_lossy().to_string(),
                "stateDir": paths.state_dir.to_string_lossy().to_string(),
                "cacheDir": paths.cache_dir.to_string_lossy().to_string(),
                "outputDir": paths.output_dir.to_string_lossy().to_string(),
                "lockfile": paths.lockfile.to_string_lossy().to_string(),
                "version": paths.version,
                "digest": paths.digest,
                "trustStatus": paths.trust_status
            }),
        );
    }
}

pub(super) fn inject_declared_art_qualified_id(
    tool: &mut ToolDefinition,
) -> Result<String, ArtInstallError> {
    let publisher = tool.publisher_identity().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package publisher is required".to_owned())
    })?;
    let expected = format!("{}/{}", publisher.id, tool.id);
    let root = tool.metadata.get_or_insert_with(|| serde_json::json!({}));
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    let metadata = root
        .as_object_mut()
        .expect("Art metadata was normalized to an object");
    let art = metadata
        .entry("art".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    if !art.is_object() {
        *art = serde_json::json!({});
    }
    let art = art
        .as_object_mut()
        .expect("Art identity metadata was normalized to an object");
    match art.get("qualifiedId").and_then(serde_json::Value::as_str) {
        None => {
            art.insert(
                "qualifiedId".to_owned(),
                serde_json::json!(expected.clone()),
            );
        }
        Some(declared) if declared != expected => {
            return Err(ArtInstallError::InvalidPackage(format!(
                "metadata.art.qualifiedId `{declared}` does not match `{expected}`"
            )));
        }
        Some(_) => {}
    }
    Ok(expected)
}

pub(super) fn declared_art_qualified_id(tool: &ToolDefinition) -> Option<&str> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("art"))
        .and_then(|art| art.get("qualifiedId"))
        .and_then(serde_json::Value::as_str)
}

pub(super) fn qualified_art_id(tool: &ToolDefinition) -> Result<String, ArtInstallError> {
    let publisher = tool.publisher_identity().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package publisher is required".to_owned())
    })?;
    let expected = format!("{}/{}", publisher.id, tool.id);
    match declared_art_qualified_id(tool) {
        None => Err(ArtInstallError::InvalidPackage(
            "metadata.art.qualifiedId is required".to_owned(),
        )),
        Some(declared) if declared != expected => Err(ArtInstallError::InvalidPackage(format!(
            "metadata.art.qualifiedId `{declared}` does not match `{expected}`"
        ))),
        Some(_) => Ok(expected),
    }
}

pub(super) fn art_root_for_tool(
    control_plane_root: &Path,
    tool: &ToolDefinition,
) -> Result<PathBuf, ArtInstallError> {
    let arts_root = control_plane_root.join("arts");
    let publisher = tool.publisher_identity().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package publisher is required".to_owned())
    })?;
    Ok(arts_root.join(publisher.id).join(&tool.id))
}
