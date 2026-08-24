use super::*;

pub fn resolve_active_art_package(control_plane_root: &Path, art_id: &str) -> Option<PathBuf> {
    let art_root = art_root_for_reference(control_plane_root, art_id)?;
    let activation = read_art_activation(&art_root.join("active.json"))?;
    if !is_safe_art_version_path(&activation.active.path) {
        return None;
    }
    let relative = Path::new(&activation.active.path);
    let active = art_root.join(relative);
    active.join(MANIFEST_NAME).is_file().then_some(active)
}

/// Resolves and verifies one immutable installed Art package without changing
/// the user's active version. Long-lived Surface instances use this path so an
/// unrelated store update cannot silently move their code or break execution.
pub fn resolve_installed_art_package(
    control_plane_root: &Path,
    art_id: &str,
    version: &str,
    digest: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    if !is_safe_art_reference(art_id) {
        return Err(ArtInstallError::InvalidArtId(art_id.to_owned()));
    }
    if semver::Version::parse(version).is_err() {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art version `{version}` is not valid SemVer"
        )));
    }
    let digest = digest
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(digest.trim());
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ArtInstallError::InvalidPackage(
            "Art package digest must be a SHA-256 hex value".to_owned(),
        ));
    }
    let digest = digest.to_ascii_lowercase();
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let identity = current.qualified_id();
    let art_root = art_root_for_tool(control_plane_root, &current)?;
    let activation = read_art_activation(&art_root.join("active.json"))
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }

    let mut matches = Vec::new();
    for entry in std::fs::read_dir(art_root.join("versions"))? {
        let path = entry?.path();
        if !path.is_dir() || !path.join(MANIFEST_NAME).is_file() {
            continue;
        }
        let tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(path.join(MANIFEST_NAME))?)?;
        let security = read_art_package_security(&tool);
        if tool.qualified_id() != identity
            || security.version.as_deref().unwrap_or("0.0.0") != version
        {
            continue;
        }
        let actual_digest = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if actual_digest != digest {
            continue;
        }
        let relative = path
            .strip_prefix(&art_root)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.ends_with(&actual_digest[..12]) {
            return Err(ArtInstallError::InvalidPackage(
                "Art version directory does not match its digest".to_owned(),
            ));
        }
        matches.push((path, relative, tool, security));
    }
    if matches.len() != 1 {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art `{art_id}` package `{version}` with digest `{digest}` is {}",
            if matches.is_empty() {
                "not installed"
            } else {
                "ambiguous"
            }
        )));
    }

    let (art_dir, relative, mut tool, security) = matches.remove(0);
    let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let trust_status = verify_package_signature(
        &art_dir,
        security.publisher.as_ref(),
        security.signature.as_ref(),
        &trust_store,
    )
    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    if !activation.local_authoring && !activation.bundled_catalog {
        trust_store
            .effective_policy()
            .enforce(trust_status.clone())
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    }
    let lockfile = art_root.join("locks").join(format!("{digest}.json"));
    let mut verifying = std::collections::BTreeSet::from([identity]);
    verify_art_lockfile(
        control_plane_root,
        &lockfile,
        &art_root,
        &art_dir,
        &tool,
        framework_registry,
        &mut verifying,
    )?;
    let state_dir = art_root.join("state");
    let cache_dir = art_root.join("cache");
    let output_dir = art_root.join("outputs");
    let qualified_id = qualified_art_id(&tool)?;
    record_art_package_directory(
        &mut tool.metadata,
        ArtPackagePaths {
            qualified_id: &qualified_id,
            art_dir: &art_dir,
            state_dir: &state_dir,
            cache_dir: &cache_dir,
            output_dir: &output_dir,
            lockfile: &lockfile,
            version,
            digest: &digest,
            trust_status: &trust_status,
        },
    );
    let lockfile_document: PluginLockfile = serde_json::from_slice(&std::fs::read(&lockfile)?)?;
    let mut locked_arts = serde_json::Map::new();
    for dependency in lockfile_document
        .resolved
        .iter()
        .filter(|dependency| dependency.kind == "art")
    {
        let child = resolve_installed_art_package(
            control_plane_root,
            &dependency.id,
            &dependency.version,
            &dependency.sha256,
            tool_registry,
            framework_registry,
        )?;
        locked_arts.insert(dependency.id.clone(), serde_json::to_value(child)?);
    }
    if !locked_arts.is_empty() {
        if let Some(package) = tool
            .metadata
            .as_mut()
            .and_then(|metadata| metadata.get_mut("artPackage"))
            .and_then(serde_json::Value::as_object_mut)
        {
            package.insert(
                "lockedArts".to_owned(),
                serde_json::Value::Object(locked_arts),
            );
        }
    }
    debug_assert!(relative.ends_with(&digest[..12]));
    Ok(tool)
}
