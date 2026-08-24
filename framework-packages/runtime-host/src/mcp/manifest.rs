// Runtime manifest loading and declared MCP dependency validation.
const MAX_ART_MANIFEST_BYTES: usize = 1024 * 1024;

fn load_config(art_dir: &Path) -> Result<McpArtConfig, String> {
    let manifest_path = art_dir.join("manifest.json");
    let manifest_bytes = read_art_manifest(&manifest_path)?;
    let manifest: ArtManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let mut config = manifest
        .metadata
        .mcp
        .ok_or_else(|| "MCP Art metadata.mcp is required".to_owned())?;
    normalize_config(&mut config)?;
    if config.server_id.trim().is_empty() {
        return Err("MCP Art metadata.mcp.serverId is required".to_owned());
    }
    if config.package_id.trim().is_empty() {
        return Err("MCP Art metadata.mcp.packageId is required".to_owned());
    }
    if config.version.trim().is_empty() {
        return Err("MCP Art metadata.mcp.version is required".to_owned());
    }
    validate_declared_dependency(&config, &manifest.metadata.dependencies)?;
    validate_argument_object(&config.arguments, "metadata.mcp.arguments")?;
    validate_call_config(&config)?;
    validate_surface_actions(&config)?;
    validate_argument_aliases(&config.argument_aliases)?;
    Ok(config)
}

fn read_art_manifest(manifest_path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let mut reader = file.take((MAX_ART_MANIFEST_BYTES + 1) as u64);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    if bytes.len() > MAX_ART_MANIFEST_BYTES {
        return Err(format!(
            "MCP Art manifest {} exceeds the {MAX_ART_MANIFEST_BYTES} byte limit",
            manifest_path.display()
        ));
    }
    Ok(bytes)
}

/// Re-check, at execution time, the tie between `metadata.mcp` and the dependency the Art
/// declares. The installer already enforces this (`crates/loom_tool_registry/src/install.rs`,
/// `validate_mcp_execution_dependency`), but the installer only sees the package it installs;
/// this sees the manifest that is about to run, so a manifest edited in place after installation
/// fails closed here instead of running against a server nobody declared.
fn validate_declared_dependency(
    config: &McpArtConfig,
    dependencies: &ArtDependencies,
) -> Result<(), String> {
    let package_id = config.package_id.trim();
    let declared = dependencies
        .mcp_servers
        .iter()
        .filter(|dependency| dependency.id.trim() == package_id)
        .collect::<Vec<_>>();
    if declared.len() != 1 {
        return Err(format!(
            "MCP Art metadata.mcp.packageId `{package_id}` needs exactly one matching metadata.dependencies.mcpServers entry, found {}",
            declared.len()
        ));
    }
    if declared[0].version.trim() != config.version.trim() {
        return Err(format!(
            "MCP Art metadata.mcp.version `{}` disagrees with the declared dependency version `{}` for `{package_id}`",
            config.version.trim(),
            declared[0].version.trim()
        ));
    }
    Ok(())
}
