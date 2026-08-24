// Framework, Art and Surface package contract validation.
fn validate_path_with_trust_store(path: &Path, trust_store_path: Option<&Path>) -> Result<String> {
    let trust_store = trust_store_path
        .map(TrustStore::load)
        .transpose()?
        .unwrap_or_default();
    validate_path_with_payload(path, false, &trust_store)
}

fn validate_path_with_payload(
    path: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
) -> Result<String> {
    validate_path_with_payload_after_tree_inspection(path, require_payload, trust_store, false)
}

fn validate_path_with_payload_after_tree_inspection(
    path: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
    package_tree_inspected: bool,
) -> Result<String> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect package path {}", path.display()))?;
    if is_reparse_or_symlink(&metadata) {
        bail!("package path must not be a link: {}", path.display());
    }
    if metadata.file_type().is_dir() {
        ensure_real_directory(path, "package root")?;
        if contained_regular_file_exists(path, Path::new("framework.manifest.json"))? {
            return validate_framework_package(
                path,
                require_payload,
                trust_store,
                package_tree_inspected,
            );
        }
        if contained_regular_file_exists(path, Path::new("manifest.json"))?
            || contained_regular_file_exists(path, Path::new("art.runtime.json"))?
        {
            return validate_art_package(
                path,
                require_payload,
                trust_store,
                package_tree_inspected,
            );
        }
        bail!("directory contains neither a framework nor an Art package manifest");
    }
    if !metadata.file_type().is_file() {
        bail!("package path must be a manifest or package directory: {}", path.display());
    }
    match path.file_name().and_then(|value| value.to_str()) {
        Some("framework.manifest.json") => validate_framework_package(
            path_parent_or_current(path).context("framework manifest has no parent directory")?,
            require_payload,
            trust_store,
            package_tree_inspected,
        ),
        Some("art.runtime.json") | Some("manifest.json") => validate_art_package(
            path_parent_or_current(path).context("Art manifest has no parent directory")?,
            require_payload,
            trust_store,
            package_tree_inspected,
        ),
        _ => bail!("unsupported manifest path: {}", path.display()),
    }
}

fn validate_framework_package(
    directory: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
    package_tree_inspected: bool,
) -> Result<String> {
    ensure_real_directory(directory, "framework package root")?;
    if !package_tree_inspected {
        collect_package_files(directory).context("inspect framework package tree")?;
    }
    let path = directory.join("framework.manifest.json");
    let manifest: FrameworkPackageManifest = read_json(&path)?;
    validate_framework_manifest_contract(&manifest).map_err(|error| anyhow!(error))?;
    validate_relative_package_path(directory, &manifest.entry.command, require_payload)
        .context("validate framework entry")?;
    if manifest.entry.kind != "process" {
        bail!("framework entry.kind must be `process`");
    }
    if let Some(signature) = &manifest.signature {
        validate_relative_package_path(directory, &signature.file, true)
            .context("validate signature file")?;
        let publisher_key = manifest.publisher.key_id.as_deref();
        if publisher_key != Some(signature.key_id.as_str()) {
            bail!("signature keyId must match publisher.keyId");
        }
    }
    let trust = verify_package_signature(
        directory,
        Some(&manifest.publisher),
        manifest.signature.as_ref(),
        trust_store,
    )?;
    reject_revoked_package(&trust)?;
    Ok(format!(
        "framework package valid: {} {} ({}, trust={trust:?})",
        manifest.qualified_id(),
        manifest.version,
        FRAMEWORK_PROTOCOL_VERSION
    ))
}

fn validate_art_package(
    directory: &Path,
    require_payload: bool,
    trust_store: &TrustStore,
    package_tree_inspected: bool,
) -> Result<String> {
    ensure_real_directory(directory, "Art package root")?;
    if !package_tree_inspected {
        collect_package_files(directory).context("inspect Art package tree")?;
    }
    let runtime_path = directory.join("art.runtime.json");
    let runtime: ArtRuntimeManifest = read_json(&runtime_path)?;
    if runtime.protocol_version != ART_RUNTIME_PROTOCOL_VERSION {
        bail!(
            "unsupported Art runtime protocol: {}",
            runtime.protocol_version
        );
    }
    validate_art_runtime_command(directory, &runtime.entry.command, require_payload)
        .context("validate Art runtime entry")?;

    let manifest_path = directory.join("manifest.json");
    let manifest: Value = read_json(&manifest_path)?;
    let id = manifest
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Art manifest id is required"))?;
    if !is_safe_package_id(id) {
        bail!("Art manifest id is not a safe package id: {id}");
    }
    let execution = manifest
        .get("execution")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Art manifest execution is required"))?;
    if execution.get("type").and_then(Value::as_str) != Some("framework_art") {
        bail!("Art package execution.type must be `framework_art`");
    }
    let framework = execution
        .get("framework")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Art package execution.framework is required"))?;
    if !is_safe_package_reference(framework) {
        bail!("Art framework id is not safe: {framework}");
    }
    if let Some(surface) = manifest
        .get("metadata")
        .and_then(|value| value.get("capabilities"))
        .and_then(|value| value.get("surface"))
    {
        validate_surface_package(directory, surface, require_payload)?;
    }
    let (publisher, signature) = art_security_metadata(&manifest)?;
    let expected_qualified = format!("{}/{}", publisher.id, id);
    match manifest
        .pointer("/metadata/art/qualifiedId")
        .and_then(Value::as_str)
    {
        Some(declared) if declared == expected_qualified => {}
        Some(declared) => {
            bail!("metadata.art.qualifiedId `{declared}` does not match `{expected_qualified}`")
        }
        None => bail!("metadata.art.qualifiedId is required"),
    }
    let trust =
        verify_package_signature(directory, Some(&publisher), signature.as_ref(), trust_store)?;
    reject_revoked_package(&trust)?;
    Ok(format!(
        "Art package valid: {id} -> {framework} (trust={trust:?})"
    ))
}

fn validate_surface_package(directory: &Path, value: &Value, require_payload: bool) -> Result<()> {
    let manifest: SurfacePackageManifest =
        serde_json::from_value(value.clone()).context("parse Art Surface manifest")?;
    validate_surface_protocol(&manifest.protocol_version)
        .map_err(|error| anyhow!(error))
        .context("validate Surface protocol")?;
    if manifest.api_version != SURFACE_API_VERSION {
        bail!("unsupported Surface API version: {}", manifest.api_version);
    }
    if manifest.variants.is_empty() {
        bail!("Surface manifest must declare at least one variant");
    }

    let mut declared_actions = BTreeSet::new();
    for action in &manifest.actions {
        if !is_safe_surface_identifier(&action.id) {
            bail!("Surface action id is not safe: {}", action.id);
        }
        if !declared_actions.insert(action.id.as_str()) {
            bail!("duplicate Surface action id: {}", action.id);
        }
        if matches!(action.risk, loom_protocol::SurfaceActionRisk::High) && !action.confirmation {
            bail!(
                "high-risk Surface action must require host confirmation: {}",
                action.id
            );
        }
        if action.timeout_ms == Some(0) {
            bail!(
                "Surface action timeoutMs must be greater than zero: {}",
                action.id
            );
        }
    }

    for node_type in &manifest.required_nodes {
        if !DECLARATIVE_SURFACE_NODE_TYPES.contains(&node_type.as_str()) {
            bail!("Surface manifest requires unknown declarative node type: {node_type}");
        }
    }
    for variant in &manifest.variants {
        validate_relative_package_path(directory, &variant.entry, require_payload)
            .context("validate Surface variant entry")?;
        if variant.runtime == SurfaceRuntimeKind::Declarative && require_payload {
            validate_declarative_surface_scene(directory, &variant.entry, &declared_actions)?;
        }
    }
    if let Some(fallback) = &manifest.fallback_scene {
        validate_relative_package_path(directory, fallback, require_payload)
            .context("validate Surface fallback scene")?;
        if require_payload {
            validate_declarative_surface_scene(directory, fallback, &declared_actions)?;
        }
    }
    for migration in &manifest.migrations {
        if migration.to != migration.from.saturating_add(1) {
            bail!(
                "Surface state migration must advance exactly one version: {} -> {}",
                migration.from,
                migration.to
            );
        }
        validate_relative_package_path(directory, &migration.entry, require_payload)
            .context("validate Surface migration entry")?;
    }
    Ok(())
}

fn validate_declarative_surface_scene(
    directory: &Path,
    entry: &str,
    declared_actions: &BTreeSet<&str>,
) -> Result<()> {
    let document: Value = read_json(&directory.join(entry))?;
    let protocol = document
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("declarative Surface scene protocolVersion is required"))?;
    validate_surface_protocol(protocol)
        .map_err(|error| anyhow!(error))
        .context("validate declarative Surface scene protocol")?;
    let scene: SurfaceNode = serde_json::from_value(
        document
            .get("scene")
            .cloned()
            .ok_or_else(|| anyhow!("declarative Surface scene root is required"))?,
    )
    .context("parse declarative Surface scene root")?;
    validate_surface_node_tree(&scene)
        .map_err(|error| anyhow!(error))
        .context("validate declarative Surface scene root")?;

    fn visit(node: &SurfaceNode, declared_actions: &BTreeSet<&str>) -> Result<()> {
        if !DECLARATIVE_SURFACE_NODE_TYPES.contains(&node.node_type.as_str()) {
            bail!(
                "declarative Surface scene uses unknown node type: {}",
                node.node_type
            );
        }
        for action in node.events.values() {
            if !declared_actions.contains(action.as_str()) {
                bail!(
                    "declarative Surface node `{}` references undeclared action `{action}`",
                    node.id
                );
            }
        }
        for child in &node.children {
            visit(child, declared_actions)?;
        }
        Ok(())
    }
    visit(&scene, declared_actions)
}

fn art_security_metadata(
    manifest: &Value,
) -> Result<(PublisherIdentity, Option<PackageSignature>)> {
    let security = manifest
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("packageSecurity"));
    let publisher: PublisherIdentity = security
        .and_then(|security| security.get("publisher"))
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .ok_or_else(|| anyhow!("Art package publisher metadata is required"))?;
    if !is_safe_publisher_id(&publisher.id) {
        bail!("Art publisher id is not safe: {}", publisher.id);
    }
    let signature = security
        .and_then(|security| security.get("signature"))
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?;
    Ok((publisher, signature))
}

fn reject_revoked_package(trust: &PackageTrustStatus) -> Result<()> {
    if *trust == PackageTrustStatus::Revoked {
        bail!("package signature belongs to a revoked publisher key");
    }
    Ok(())
}
