use super::*;

pub fn list_installed_art_versions(
    control_plane_root: &Path,
    art_id: &str,
    tool_registry: &ToolRegistry,
) -> Result<Vec<ArtInstalledVersion>, ArtInstallError> {
    if !is_safe_art_reference(art_id) {
        return Err(ArtInstallError::InvalidArtId(art_id.to_owned()));
    }
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
    let versions_root = art_root.join("versions");
    if !versions_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&versions_root)? {
        let path = entry?.path();
        if !path.is_dir() || !path.join(MANIFEST_NAME).is_file() {
            continue;
        }
        let tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(path.join(MANIFEST_NAME))?)?;
        if tool.qualified_id() != identity {
            continue;
        }
        let security = read_art_package_security(&tool);
        let digest = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let relative = path
            .strip_prefix(&art_root)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.ends_with(&digest[..12]) {
            continue;
        }
        versions.push(ArtInstalledVersion {
            version: security.version.unwrap_or_else(|| "0.0.0".to_owned()),
            digest,
            active: activation.active.path == relative,
        });
    }
    versions.sort_by(|left, right| {
        match (
            semver::Version::parse(&left.version),
            semver::Version::parse(&right.version),
        ) {
            (Ok(left), Ok(right)) => right.cmp(&left),
            _ => right.version.cmp(&left.version),
        }
        .then_with(|| right.active.cmp(&left.active))
        .then_with(|| left.digest.cmp(&right.digest))
    });
    Ok(versions)
}

pub fn activate_art_version(
    control_plane_root: &Path,
    art_id: &str,
    target_version: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    if semver::Version::parse(target_version).is_err() {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art target version `{target_version}` is not valid SemVer"
        )));
    }
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let art_root = art_root_for_tool(control_plane_root, &current)?;
    let active_path = art_root.join("active.json");
    let activation = read_art_activation(&active_path)
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if activation.active.version == target_version {
        return Ok(current);
    }
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }
    let identity = current.qualified_id();
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
            || security.version.as_deref().unwrap_or("0.0.0") != target_version
        {
            continue;
        }
        let digest = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let relative = path
            .strip_prefix(&art_root)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.ends_with(&digest[..12]) {
            return Err(ArtInstallError::InvalidPackage(
                "Art version directory does not match its digest".to_owned(),
            ));
        }
        matches.push(ArtVersionPointer {
            path: relative,
            version: target_version.to_owned(),
            digest: digest.clone(),
            lockfile: art_root
                .join("locks")
                .join(format!("{digest}.json"))
                .to_string_lossy()
                .to_string(),
        });
    }
    if matches.len() != 1 {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art `{art_id}` target version `{target_version}` is {}",
            if matches.is_empty() {
                "not installed"
            } else {
                "ambiguous because multiple package digests are installed"
            }
        )));
    }
    activate_art_pointer(
        control_plane_root,
        &art_root,
        &active_path,
        activation,
        matches.remove(0),
        tool_registry,
        framework_registry,
    )
}

pub fn rollback_art_package(
    control_plane_root: &Path,
    art_id: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let art_root = art_root_for_tool(control_plane_root, &current)?;
    let active_path = art_root.join("active.json");
    let activation = read_art_activation(&active_path)
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }
    let previous = activation.previous.clone().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art has no previous version to roll back".to_owned())
    })?;
    activate_art_pointer(
        control_plane_root,
        &art_root,
        &active_path,
        activation,
        previous,
        tool_registry,
        framework_registry,
    )
}

pub(super) fn activate_art_pointer(
    control_plane_root: &Path,
    art_root: &Path,
    active_path: &Path,
    activation: ArtActivationState,
    target: ArtVersionPointer,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    if !is_safe_art_version_path(&target.path) {
        return Err(ArtInstallError::InvalidPackage(
            "target Art package path is unsafe".to_owned(),
        ));
    }
    let target_dir = art_root.join(&target.path);
    if !target_dir.join(MANIFEST_NAME).is_file() {
        return Err(ArtInstallError::InvalidPackage(
            "target Art package is missing".to_owned(),
        ));
    }
    let mut tool: ToolDefinition =
        serde_json::from_slice(&std::fs::read(target_dir.join(MANIFEST_NAME))?)?;
    let security = read_art_package_security(&tool);
    let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let trust_status = verify_package_signature(
        &target_dir,
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
    let digest = canonical_package_digest(
        &target_dir,
        security
            .signature
            .as_ref()
            .map(|signature| signature.file.as_str()),
    )
    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    if digest != target.digest || !target.path.ends_with(&digest[..12]) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "target Art package digest does not match its immutable version pointer: expected {}, got {digest}",
            target.digest,
        )));
    }
    let mut verifying = std::collections::BTreeSet::from([tool.qualified_id()]);
    verify_art_lockfile(
        control_plane_root,
        Path::new(&target.lockfile),
        art_root,
        &target_dir,
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
            art_dir: &target_dir,
            state_dir: &state_dir,
            cache_dir: &cache_dir,
            output_dir: &output_dir,
            lockfile: Path::new(&target.lockfile),
            version: &target.version,
            digest: &target.digest,
            trust_status: &trust_status,
        },
    );
    let next = ArtActivationState {
        active: target,
        previous: Some(activation.active.clone()),
        local_authoring: activation.local_authoring,
        bundled_catalog: activation.bundled_catalog,
    };
    write_art_lifecycle(
        &art_root,
        &ArtLifecycleJournal {
            old_activation: Some(activation.clone()),
            next_activation: next.clone(),
            target: next.active.path.clone(),
            // This activation points at a version that is already on disk; recovery must never
            // delete it.
            created_target: false,
        },
    )?;
    if let Err(error) = write_art_activation(active_path, &next) {
        // The journal describes an activation that was never written. Leaving it behind would make
        // the next startup restore `old_activation` over an activation that already holds exactly
        // that value, so drop it instead.
        clear_art_lifecycle(art_root);
        return Err(error);
    }
    let tool = tool_registry.save_tool(tool).map_err(|error| {
        let _ = write_art_activation(active_path, &activation);
        clear_art_lifecycle(art_root);
        ArtInstallError::Registry(error.to_string())
    })?;
    clear_art_lifecycle(art_root);
    Ok(tool)
}
