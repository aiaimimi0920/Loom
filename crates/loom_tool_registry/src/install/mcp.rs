use super::*;

pub(super) fn validate_mcp_execution_dependency(
    tool: &ToolDefinition,
    declared: &[ArtMcpServerDependency],
) -> Result<(), ArtInstallError> {
    let framework = crate::framework::framework_id_for_execution(&tool.execution);
    if framework.rsplit_once('/').map_or(framework, |(_, id)| id) != "mcp" {
        return Ok(());
    }
    let execution: ArtMcpExecutionMetadata = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("mcp"))
        .cloned()
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(
                "MCP Art metadata.mcp is required for the mcp framework".to_owned(),
            )
        })
        .and_then(|value| {
            serde_json::from_value(value).map_err(|error| {
                ArtInstallError::InvalidPackage(format!("invalid MCP Art metadata: {error}"))
            })
        })?;
    if !loom_protocol::is_safe_package_id(execution.server_id.trim()) {
        return Err(ArtInstallError::InvalidPackage(
            "metadata.mcp.serverId must be a safe package id".to_owned(),
        ));
    }
    split_mcp_package_identity(&execution.package_id).ok_or_else(|| {
        ArtInstallError::InvalidPackage(
            "metadata.mcp.packageId must be publisher-qualified".to_owned(),
        )
    })?;
    semver::VersionReq::parse(&execution.version).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "metadata.mcp.version `{}` is invalid: {error}",
            execution.version
        ))
    })?;
    let matches = declared
        .iter()
        .filter(|dependency| dependency.id == execution.package_id)
        .collect::<Vec<_>>();
    if matches.len() != 1 || matches[0].version != execution.version {
        return Err(ArtInstallError::InvalidPackage(format!(
            "metadata.mcp package `{}` version `{}` must have one identical metadata.dependencies.mcpServers declaration",
            execution.package_id, execution.version
        )));
    }
    Ok(())
}

pub(super) fn split_mcp_package_identity(value: &str) -> Option<(&str, &str)> {
    let (publisher, id) = value.split_once('/')?;
    (!publisher.contains('/')
        && loom_protocol::is_safe_publisher_id(publisher)
        && loom_protocol::is_safe_package_id(id))
    .then_some((publisher, id))
}

pub(super) fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn read_installed_mcp_servers(
    control_plane_root: &Path,
) -> Result<Vec<loom_mcp::McpServerConfig>, ArtInstallError> {
    let path = control_plane_root.join("mcp").join("servers.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    serde_json::from_slice(&bytes).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "installed MCP server store `{}` is invalid: {error}",
            path.display()
        ))
    })
}

pub(super) fn verify_active_mcp_package(
    control_plane_root: &Path,
    package: &loom_mcp::McpServerPackageState,
) -> Result<PathBuf, ArtInstallError> {
    let (publisher, id) = split_mcp_package_identity(&package.qualified_id).ok_or_else(|| {
        ArtInstallError::InvalidPackage(format!(
            "installed MCP package identity `{}` is invalid",
            package.qualified_id
        ))
    })?;
    if package.publisher_id != publisher || !is_sha256_hex(&package.digest) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` has invalid publisher or digest state",
            package.qualified_id
        )));
    }
    semver::Version::parse(&package.version).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` version is invalid: {error}",
            package.qualified_id
        ))
    })?;

    let package_root = control_plane_root
        .join("mcp")
        .join("packages")
        .join(publisher)
        .join(id);
    let active = loom_mcp::package::read_active_state(control_plane_root, publisher, id).map_err(
        |error| {
            ArtInstallError::InvalidPackage(format!(
                "installed MCP package `{}` has no readable active state: {error}",
                package.qualified_id
            ))
        },
    )?;
    if active.qualified_id != package.qualified_id
        || active.version != package.version
        || !active.digest.eq_ignore_ascii_case(&package.digest)
    {
        return Err(ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` active state does not match the server registry",
            package.qualified_id
        )));
    }

    let versions_root = std::fs::canonicalize(package_root.join("versions")).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` versions directory is unavailable: {error}",
            package.qualified_id
        ))
    })?;
    let package_dir = std::fs::canonicalize(&package.package_dir).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` directory is unavailable: {error}",
            package.qualified_id
        ))
    })?;
    let active_dir = std::fs::canonicalize(&active.package_dir).map_err(|error| {
        ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` active directory is unavailable: {error}",
            package.qualified_id
        ))
    })?;
    let expected_name = format!(
        "{}-{}",
        package.version,
        &package.digest[..loom_mcp::package::PACKAGE_DIRECTORY_DIGEST_CHARS]
    );
    if package_dir.parent() != Some(versions_root.as_path())
        || package_dir.file_name() != Some(OsStr::new(&expected_name))
        || active_dir != package_dir
    {
        return Err(ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` active directory escapes its immutable package root",
            package.qualified_id
        )));
    }

    let manifest_path = package_dir.join(loom_mcp::package::MCP_SERVER_PACKAGE_MANIFEST);
    let manifest: loom_mcp::package::McpServerPackageManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path)?).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "installed MCP package manifest `{}` is invalid: {error}",
                manifest_path.display()
            ))
        })?;
    if manifest.qualified_id() != package.qualified_id || manifest.version != package.version {
        return Err(ArtInstallError::InvalidPackage(format!(
            "installed MCP package `{}` manifest identity or version changed",
            package.qualified_id
        )));
    }
    Ok(package_dir)
}

pub(super) fn resolve_mcp_dependency_locks(
    control_plane_root: &Path,
    dependencies: &[ArtMcpServerDependency],
) -> Result<Vec<ResolvedDependency>, ArtInstallError> {
    if dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let servers = read_installed_mcp_servers(control_plane_root)?;
    let mut resolved = Vec::with_capacity(dependencies.len());
    let mut identities = std::collections::BTreeSet::new();
    for dependency in dependencies {
        split_mcp_package_identity(&dependency.id).ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` must be a safe publisher-qualified package id",
                dependency.id
            ))
        })?;
        if !identities.insert(dependency.id.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` is declared more than once",
                dependency.id
            )));
        }
        let requirement = semver::VersionReq::parse(&dependency.version).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` has invalid version requirement `{}`: {error}",
                dependency.id, dependency.version
            ))
        })?;
        let matches = servers
            .iter()
            .filter(|server| {
                server
                    .package
                    .as_ref()
                    .is_some_and(|package| package.qualified_id == dependency.id)
            })
            .collect::<Vec<_>>();
        let server = match matches.as_slice() {
            [] => {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "MCP dependency `{}` is not installed",
                    dependency.id
                )))
            }
            [server] => *server,
            _ => {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "MCP dependency `{}` resolves to multiple installed servers",
                    dependency.id
                )))
            }
        };
        if !server.enabled {
            return Err(ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` is disabled",
                dependency.id
            )));
        }
        let package = server.package.as_ref().expect("matching server package");
        let version = semver::Version::parse(&package.version).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "installed MCP package `{}` version is invalid: {error}",
                dependency.id
            ))
        })?;
        if !requirement.matches(&version) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` requires `{}`, but `{}` is installed",
                dependency.id, dependency.version, package.version
            )));
        }
        let package_dir = verify_active_mcp_package(control_plane_root, package)?;
        let digest = canonical_package_digest(&package_dir, None)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        resolved.push(ResolvedDependency {
            kind: "mcp".to_owned(),
            id: dependency.id.clone(),
            version: package.version.clone(),
            sha256: digest,
        });
    }
    Ok(resolved)
}

pub(super) fn art_reference_matches_qualified(reference: &str, qualified_id: &str) -> bool {
    if reference.contains('/') {
        reference == qualified_id
    } else {
        qualified_id == reference
            || qualified_id
                .rsplit_once('/')
                .is_some_and(|(_, id)| id == reference)
    }
}

pub(super) fn validate_art_dependency_lock_set(
    declared: &[String],
    resolved: &[ResolvedDependency],
) -> Result<(), ArtInstallError> {
    let locked = resolved
        .iter()
        .filter(|dependency| dependency.kind == "art")
        .collect::<Vec<_>>();
    let mut matched = std::collections::BTreeSet::new();
    for reference in declared {
        if !is_safe_art_reference(reference) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency reference `{reference}` is invalid"
            )));
        }
        let matches = locked
            .iter()
            .filter(|dependency| art_reference_matches_qualified(reference, &dependency.id))
            .collect::<Vec<_>>();
        if matches.len() != 1 || !matched.insert(matches[0].id.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency `{reference}` is not represented by one exact lock"
            )));
        }
    }
    if matched.len() != locked.len() {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile contains an undeclared or duplicate Art dependency".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn validate_mcp_dependency_lock_set(
    declared: &[ArtMcpServerDependency],
    resolved: &[ResolvedDependency],
) -> Result<(), ArtInstallError> {
    let locked = resolved
        .iter()
        .filter(|dependency| dependency.kind == "mcp")
        .collect::<Vec<_>>();
    let mut matched = std::collections::BTreeSet::new();
    for dependency in declared {
        split_mcp_package_identity(&dependency.id).ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` must be a safe publisher-qualified package id",
                dependency.id
            ))
        })?;
        let requirement = semver::VersionReq::parse(&dependency.version).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` has invalid version requirement `{}`: {error}",
                dependency.id, dependency.version
            ))
        })?;
        let matches = locked
            .iter()
            .filter(|locked| locked.id == dependency.id)
            .collect::<Vec<_>>();
        if matches.len() != 1 || !matched.insert(matches[0].id.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "MCP dependency `{}` is not represented by one exact lock",
                dependency.id
            )));
        }
        let locked_version = semver::Version::parse(&matches[0].version).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "locked MCP dependency `{}` has invalid version: {error}",
                dependency.id
            ))
        })?;
        if !requirement.matches(&locked_version) || !is_sha256_hex(&matches[0].sha256) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "locked MCP dependency `{}` no longer satisfies the Art manifest",
                dependency.id
            )));
        }
    }
    if matched.len() != locked.len() {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile contains an undeclared or duplicate MCP dependency".to_owned(),
        ));
    }
    Ok(())
}
