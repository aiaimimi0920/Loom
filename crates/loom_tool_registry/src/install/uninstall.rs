use super::*;

pub(super) fn resolve_art_root_for_uninstall(
    control_plane_root: &Path,
    art_id: &str,
    tool: Option<&ToolDefinition>,
) -> Result<PathBuf, ArtInstallError> {
    if let Some(tool) = tool {
        return art_root_for_tool(control_plane_root, tool);
    }

    if art_id.contains('/') {
        return art_root_for_reference(control_plane_root, art_id)
            .ok_or_else(|| ArtInstallError::InvalidArtId(art_id.to_owned()));
    }

    let arts_root = control_plane_root.join("arts");
    if !arts_root.is_dir() {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art `{art_id}` is not installed"
        )));
    }
    let mut publisher_matches = Vec::new();
    for entry in std::fs::read_dir(&arts_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(publisher) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !loom_protocol::is_safe_publisher_id(&publisher) {
            continue;
        }
        let candidate = entry.path().join(art_id);
        if candidate.is_dir() {
            publisher_matches.push(candidate);
        }
    }
    match publisher_matches.len() {
        0 => Err(ArtInstallError::InvalidPackage(format!(
            "Art `{art_id}` is not installed"
        ))),
        1 => Ok(publisher_matches.remove(0)),
        _ => Err(ArtInstallError::InvalidPackage(format!(
            "Art id `{art_id}` is installed by multiple publishers; use a publisher-qualified id"
        ))),
    }
}

pub fn uninstall_art_package(
    control_plane_root: &Path,
    art_id: &str,
    tool_registry: &ToolRegistry,
) -> Result<(), ArtInstallError> {
    if !is_safe_art_reference(art_id) {
        return Err(ArtInstallError::InvalidArtId(art_id.to_owned()));
    }
    let tool = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?;
    let art_root = resolve_art_root_for_uninstall(control_plane_root, art_id, tool.as_ref())?;
    let tombstone = if art_root.exists() {
        let tombstone = uninstall_tombstone_path(&art_root, ART_UNINSTALL_TOMBSTONE_PREFIX)?;
        std::fs::rename(&art_root, &tombstone)?;
        Some(tombstone)
    } else {
        None
    };
    if let Err(error) = tool_registry.delete_tool(art_id) {
        if let Some(tombstone) = &tombstone {
            let _ = std::fs::rename(tombstone, &art_root);
        }
        return Err(ArtInstallError::Registry(error.to_string()));
    }
    if let Some(tombstone) = tombstone {
        remove_tree(&tombstone)?;
    }
    Ok(())
}
