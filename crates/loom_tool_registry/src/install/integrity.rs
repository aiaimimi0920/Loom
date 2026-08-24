use super::*;

pub fn verify_art_package_integrity(
    control_plane_root: &Path,
    tool: &ToolDefinition,
    framework_registry: &FrameworkRegistry,
) -> Result<(), ArtInstallError> {
    let mut verifying = std::collections::BTreeSet::new();
    verify_art_package_integrity_inner(control_plane_root, tool, framework_registry, &mut verifying)
        .map(|_| ())
}

pub(super) fn verify_art_package_integrity_inner(
    control_plane_root: &Path,
    tool: &ToolDefinition,
    framework_registry: &FrameworkRegistry,
    verifying: &mut std::collections::BTreeSet<String>,
) -> Result<ArtVersionPointer, ArtInstallError> {
    let identity = tool.qualified_id();
    if !verifying.insert(identity.clone()) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art dependency cycle detected at `{identity}`"
        )));
    }
    let result = (|| {
        let art_root = art_root_for_tool(control_plane_root, tool)?;
        let activation = read_art_activation(&art_root.join("active.json")).ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{}` has no activation state", tool.id))
        })?;
        if !art_activation_is_safe(&activation) {
            return Err(ArtInstallError::InvalidPackage(
                "Art activation state contains an unsafe version path".to_owned(),
            ));
        }
        let active_dir = art_root.join(&activation.active.path);
        let package_tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(active_dir.join(MANIFEST_NAME))?)?;
        if package_tool.qualified_id() != identity {
            return Err(ArtInstallError::InvalidPackage(
                "active Art manifest identity does not match the registry".to_owned(),
            ));
        }
        let security = read_art_package_security(&package_tool);
        let expected_version = security.version.as_deref().unwrap_or("0.0.0");
        if activation.active.version != expected_version {
            return Err(ArtInstallError::InvalidPackage(
                "active Art version does not match its manifest".to_owned(),
            ));
        }
        let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let trust_status = verify_package_signature(
            &active_dir,
            security.publisher.as_ref(),
            security.signature.as_ref(),
            &trust_store,
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if !activation.local_authoring && !activation.bundled_catalog {
            trust_store
                .effective_policy()
                .enforce(trust_status)
                .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        }
        let digest = canonical_package_digest(
            &active_dir,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if digest != activation.active.digest || !activation.active.path.ends_with(&digest[..12]) {
            return Err(ArtInstallError::InvalidPackage(
                "active Art digest does not match its immutable version pointer".to_owned(),
            ));
        }
        verify_art_lockfile(
            control_plane_root,
            Path::new(&activation.active.lockfile),
            &art_root,
            &active_dir,
            &package_tool,
            framework_registry,
            verifying,
        )?;
        Ok(activation.active)
    })();
    verifying.remove(&identity);
    result
}

pub(super) fn verify_art_lockfile(
    control_plane_root: &Path,
    lockfile_path: &Path,
    art_root: &Path,
    art_dir: &Path,
    tool: &ToolDefinition,
    framework_registry: &FrameworkRegistry,
    verifying: &mut std::collections::BTreeSet<String>,
) -> Result<(), ArtInstallError> {
    let canonical_root = std::fs::canonicalize(art_root)?;
    let canonical_lockfile = std::fs::canonicalize(lockfile_path)?;
    if !canonical_lockfile.starts_with(&canonical_root) {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile escapes the Art package root".to_owned(),
        ));
    }
    let lockfile: PluginLockfile = serde_json::from_slice(&std::fs::read(&canonical_lockfile)?)?;
    let security = read_art_package_security(tool);
    let expected_version = security.version.as_deref().unwrap_or("0.0.0");
    let expected_identity = tool.qualified_id();
    if lockfile.schema_version != loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION
        || lockfile.package_id != expected_identity
        || lockfile.package_version != expected_version
    {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile identity, version, or schema version is invalid".to_owned(),
        ));
    }
    let declared = read_dependencies(tool);
    validate_art_dependency_lock_set(&declared.arts, &lockfile.resolved)?;
    validate_mcp_dependency_lock_set(&declared.mcp_servers, &lockfile.resolved)?;
    for dependency in &lockfile.resolved {
        match dependency.kind.as_str() {
            "framework" => {
                let status = framework_registry
                    .statuses()
                    .into_iter()
                    .find(|status| status.qualified_id == dependency.id)
                    .ok_or_else(|| ArtInstallError::FrameworkNotReady {
                        art_id: tool.id.clone(),
                        framework: dependency.id.clone(),
                        reason: "locked".to_owned(),
                    })?;
                if status.version.as_deref() != Some(dependency.version.as_str()) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked framework `{}` version is no longer active",
                        dependency.id
                    )));
                }
                let runtime_dir = status.runtime_dir.ok_or_else(|| {
                    ArtInstallError::InvalidPackage(format!(
                        "locked framework `{}` has no runtime directory",
                        dependency.id
                    ))
                })?;
                let actual = canonical_package_digest(&runtime_dir, None)
                    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
                if actual != dependency.sha256 {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked framework `{}` digest mismatch",
                        dependency.id
                    )));
                }
            }
            "binary" => {
                let relative = Path::new(&dependency.id);
                if relative.is_absolute()
                    || relative.components().any(|component| {
                        matches!(
                            component,
                            Component::ParentDir | Component::RootDir | Component::Prefix(_)
                        )
                    })
                {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked binary path `{}` is invalid",
                        dependency.id
                    )));
                }
                let actual = sha256_hex(&std::fs::read(art_dir.join(relative))?);
                if actual != dependency.sha256 {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked binary `{}` digest mismatch",
                        dependency.id
                    )));
                }
            }
            "art" => {
                let locked_digest_is_valid = dependency.sha256.len() == 64
                    && dependency
                        .sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit());
                if !locked_digest_is_valid {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked Art dependency `{}` has an invalid digest",
                        dependency.id
                    )));
                }
                let (child_root, child_dir, child_tool) = locate_exact_installed_art_package(
                    control_plane_root,
                    &dependency.id,
                    &dependency.version,
                    &dependency.sha256,
                )?;
                if !verifying.insert(dependency.id.clone()) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "Art dependency cycle detected at `{}`",
                        dependency.id
                    )));
                }
                let child_lockfile = child_root
                    .join("locks")
                    .join(format!("{}.json", dependency.sha256));
                let verified = verify_art_lockfile(
                    control_plane_root,
                    &child_lockfile,
                    &child_root,
                    &child_dir,
                    &child_tool,
                    framework_registry,
                    verifying,
                );
                verifying.remove(&dependency.id);
                verified?;
            }
            "mcp" => {
                let exact = ArtMcpServerDependency {
                    id: dependency.id.clone(),
                    version: format!("={}", dependency.version),
                };
                let active =
                    resolve_mcp_dependency_locks(control_plane_root, std::slice::from_ref(&exact))?;
                if active.len() != 1 || !active[0].sha256.eq_ignore_ascii_case(&dependency.sha256) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked MCP dependency `{}` is unavailable or has changed",
                        dependency.id
                    )));
                }
            }
            kind => {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "Art lockfile contains unsupported dependency kind `{kind}`"
                )))
            }
        }
    }
    Ok(())
}

pub(super) fn locate_exact_installed_art_package(
    control_plane_root: &Path,
    art_id: &str,
    version: &str,
    digest: &str,
) -> Result<(PathBuf, PathBuf, ToolDefinition), ArtInstallError> {
    let art_root = art_root_for_reference(control_plane_root, art_id).ok_or_else(|| {
        ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` has an invalid identity"
        ))
    })?;
    let activation = read_art_activation(&art_root.join("active.json")).ok_or_else(|| {
        ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` is not installed"
        ))
    })?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` has unsafe activation state"
        )));
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
        if tool.qualified_id() != art_id
            || security.version.as_deref().unwrap_or("0.0.0") != version
        {
            continue;
        }
        let actual = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if actual.eq_ignore_ascii_case(digest)
            && path.ends_with(format!("{version}-{}", &actual[..12]))
        {
            let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
                .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
            let trust_status = verify_package_signature(
                &path,
                security.publisher.as_ref(),
                security.signature.as_ref(),
                &trust_store,
            )
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
            if !activation.local_authoring && !activation.bundled_catalog {
                trust_store
                    .effective_policy()
                    .enforce(trust_status)
                    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
            }
            matches.push((path, tool));
        }
    }
    if matches.len() != 1 {
        return Err(ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` version `{version}` and digest `{digest}` is {}",
            if matches.is_empty() {
                "unavailable"
            } else {
                "ambiguous"
            }
        )));
    }
    let (art_dir, tool) = matches.remove(0);
    Ok((art_root, art_dir, tool))
}
