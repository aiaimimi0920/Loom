use super::*;

pub(super) fn resolve_art_dependency_locks(
    control_plane_root: &Path,
    art_references: &[String],
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<Vec<ResolvedDependency>, ArtInstallError> {
    let mut resolved = Vec::with_capacity(art_references.len());
    let mut identities = std::collections::BTreeSet::new();
    for reference in art_references {
        if !is_safe_art_reference(reference) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency reference `{reference}` is invalid"
            )));
        }
        let child = tool_registry
            .get_tool(reference)
            .map_err(|error| ArtInstallError::Registry(error.to_string()))?
            .ok_or_else(|| {
                ArtInstallError::InvalidPackage(format!(
                    "Art dependency `{reference}` is not installed"
                ))
            })?;
        let identity = child.qualified_id();
        if !identities.insert(identity.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency `{reference}` resolves to duplicate `{identity}`"
            )));
        }
        let pointer = {
            let mut verifying = std::collections::BTreeSet::new();
            verify_art_package_integrity_inner(
                control_plane_root,
                &child,
                framework_registry,
                &mut verifying,
            )
        }
        .map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "Art dependency `{identity}` integrity verification failed: {error}"
            ))
        })?;
        resolved.push(ResolvedDependency {
            kind: "art".to_owned(),
            id: identity,
            version: pointer.version,
            sha256: pointer.digest,
        });
    }
    Ok(resolved)
}

pub(super) fn write_art_lockfile(
    path: &Path,
    art_id: &str,
    art_version: &str,
    framework_id: &str,
    framework_registry: &FrameworkRegistry,
    binaries: &[ArtBinary],
    art_dir: &Path,
    art_dependencies: &[ResolvedDependency],
) -> Result<(), ArtInstallError> {
    let framework = framework_registry
        .statuses()
        .into_iter()
        .find(|status| status.qualified_id == framework_id || status.id == framework_id)
        .ok_or_else(|| ArtInstallError::FrameworkNotReady {
            art_id: art_id.to_owned(),
            framework: framework_id.to_owned(),
            reason: "missing status".to_owned(),
        })?;
    let framework_dir =
        framework
            .runtime_dir
            .ok_or_else(|| ArtInstallError::FrameworkNotReady {
                art_id: art_id.to_owned(),
                framework: framework_id.to_owned(),
                reason: "missing runtime directory".to_owned(),
            })?;
    let framework_digest = canonical_package_digest(&framework_dir, None)
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let mut resolved = vec![ResolvedDependency {
        kind: "framework".to_owned(),
        id: framework.qualified_id,
        version: framework.version.unwrap_or_else(|| "0.0.0".to_owned()),
        sha256: framework_digest,
    }];
    for binary in binaries {
        let bytes = std::fs::read(art_dir.join(binary.name.replace('\\', "/")))?;
        resolved.push(ResolvedDependency {
            kind: "binary".to_owned(),
            id: binary.name.clone(),
            version: "pinned".to_owned(),
            sha256: sha256_hex(&bytes),
        });
    }
    resolved.extend_from_slice(art_dependencies);
    let lockfile = PluginLockfile {
        schema_version: loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION,
        package_id: art_id.to_owned(),
        package_version: art_version.to_owned(),
        resolved,
    };
    let mut bytes = serde_json::to_vec_pretty(&lockfile)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}
