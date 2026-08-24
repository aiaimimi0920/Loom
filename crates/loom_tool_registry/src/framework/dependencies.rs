//! Art dependency registration and framework dependency-lock verification.
use super::*;

pub(super) fn resolve_framework_dependencies(
    control_plane_root: &Path,
    manifest: &FrameworkPackageManifest,
    staging: &Path,
) -> Result<Vec<loom_protocol::ResolvedDependency>, FrameworkError> {
    if manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let registry = crate::dependency::RuntimeRegistry::new(control_plane_root);
    let mut candidates = registry
        .list()
        .map_err(|reason| FrameworkError::InvalidPackage {
            id: manifest.id.clone(),
            reason,
        })?;
    let python = staging.join("python-embed");
    if is_directory_without_links(&python)? {
        let version = std::env::var("LOOM_PYTHON_RUNTIME_VERSION")
            .ok()
            .filter(|version| semver::Version::parse(version).is_ok())
            .unwrap_or_else(|| "3.12.0".to_owned());
        let sha256 = canonical_package_digest(&python, None)?;
        candidates.push(crate::dependency::PackageCandidate {
            kind: "runtime".to_owned(),
            id: "loom.runtime.python".to_owned(),
            version,
            sha256,
            path: python,
        });
    }
    crate::dependency::resolve_dependencies(&manifest.dependencies, &candidates).map_err(|reason| {
        FrameworkError::InvalidPackage {
            id: manifest.id.clone(),
            reason,
        }
    })
}

pub(super) fn register_framework_runtimes(
    control_plane_root: &Path,
    manifest: &FrameworkPackageManifest,
    package_dir: &Path,
) -> Result<(), FrameworkError> {
    let python = package_dir.join("python-embed");
    if !is_directory_without_links(&python)? {
        return Ok(());
    }
    let version = std::env::var("LOOM_PYTHON_RUNTIME_VERSION")
        .ok()
        .filter(|version| semver::Version::parse(version).is_ok())
        .unwrap_or_else(|| "3.12.0".to_owned());
    let sha256 = canonical_package_digest(&python, None)?;
    crate::dependency::RuntimeRegistry::new(control_plane_root)
        .register(crate::dependency::PackageCandidate {
            kind: "runtime".to_owned(),
            id: "loom.runtime.python".to_owned(),
            version,
            sha256,
            path: python,
        })
        .map_err(|reason| FrameworkError::InvalidPackage {
            id: manifest.id.clone(),
            reason,
        })
}

pub(super) fn write_framework_lockfile(
    package_root: &Path,
    qualified_id: &str,
    version: &str,
    package_digest: &str,
    resolved: Vec<loom_protocol::ResolvedDependency>,
) -> Result<(), FrameworkError> {
    let lockfile = loom_protocol::PluginLockfile {
        schema_version: loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION,
        package_id: qualified_id.to_owned(),
        package_version: version.to_owned(),
        resolved,
    };
    let locks = package_root.join("locks");
    fs::create_dir_all(&locks)?;
    let path = locks.join(format!("{package_digest}.json"));
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(&lockfile)?;
    bytes.push(b'\n');
    fs::write(&temporary, bytes)?;
    crate::replace_registry_file(&temporary, &path)?;
    Ok(())
}

pub(super) fn verify_framework_lockfile(
    control_plane_root: &Path,
    package_dir: &Path,
    manifest: &FrameworkPackageManifest,
) -> Result<(), String> {
    let versions_root = package_dir
        .parent()
        .ok_or_else(|| "framework package has no versions directory".to_owned())?;
    if versions_root.file_name() != Some(OsStr::new(FRAMEWORK_VERSIONS_DIR)) {
        return Err("framework package is not an immutable versioned install".to_owned());
    }
    let package_root = versions_root
        .parent()
        .ok_or_else(|| "version directory has no package root".to_owned())?;
    let digest = canonical_package_digest(
        package_dir,
        manifest
            .signature
            .as_ref()
            .map(|signature| signature.file.as_str()),
    )
    .map_err(|error| error.to_string())?;
    let lockfile_path = package_root.join("locks").join(format!("{digest}.json"));
    let lockfile: loom_protocol::PluginLockfile = serde_json::from_slice(
        &read_bounded_file(&lockfile_path, FRAMEWORK_METADATA_MAX_BYTES)
            .map_err(|error| format!("cannot read {}: {error}", lockfile_path.display()))?,
    )
    .map_err(|error| format!("invalid lockfile JSON: {error}"))?;
    if lockfile.schema_version != loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION
        || lockfile.package_id != manifest.qualified_id()
        || lockfile.package_version != manifest.version
    {
        return Err("lockfile identity, version, or schema is invalid".to_owned());
    }

    let candidates = crate::dependency::RuntimeRegistry::new(control_plane_root)
        .list()?
        .into_iter()
        .filter(|candidate| is_directory_without_links(&candidate.path).unwrap_or(false))
        .collect::<Vec<_>>();
    let mut locked = BTreeSet::new();
    for resolved in &lockfile.resolved {
        let key = (resolved.kind.clone(), resolved.id.clone());
        if !locked.insert(key.clone()) {
            return Err(format!(
                "dependency `{}/{}` appears more than once in the lockfile",
                key.0, key.1
            ));
        }
        let declared = manifest
            .dependencies
            .iter()
            .find(|dependency| dependency.kind == resolved.kind && dependency.id == resolved.id)
            .ok_or_else(|| {
                format!(
                    "lockfile contains undeclared dependency `{}/{}`",
                    resolved.kind, resolved.id
                )
            })?;
        let requirement = semver::VersionReq::parse(&declared.version)
            .map_err(|error| format!("invalid dependency requirement: {error}"))?;
        let version = semver::Version::parse(&resolved.version)
            .map_err(|error| format!("invalid locked dependency version: {error}"))?;
        if !requirement.matches(&version)
            || declared
                .sha256
                .as_deref()
                .is_some_and(|expected| !expected.eq_ignore_ascii_case(&resolved.sha256))
        {
            return Err(format!(
                "locked dependency `{}` no longer satisfies the manifest",
                resolved.id
            ));
        }
        if !candidates.iter().any(|candidate| {
            candidate.kind == resolved.kind
                && candidate.id == resolved.id
                && candidate.version == resolved.version
                && candidate.sha256.eq_ignore_ascii_case(&resolved.sha256)
        }) {
            return Err(format!(
                "locked dependency `{}` is unavailable or has changed",
                resolved.id
            ));
        }
    }
    for dependency in &manifest.dependencies {
        if !dependency.optional
            && !locked.contains(&(dependency.kind.clone(), dependency.id.clone()))
        {
            return Err(format!(
                "required dependency `{}` is missing from the lockfile",
                dependency.id
            ));
        }
    }
    Ok(())
}
