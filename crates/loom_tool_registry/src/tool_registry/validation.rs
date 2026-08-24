//! Tool surface and identifier validation.

use super::*;

pub(super) fn validate_surface_package_manifest(
    tool_id: &str,
    surface: &SurfacePackageManifest,
) -> ToolRegistryResult<()> {
    let invalid = |reason: String| ToolRegistryError::InvalidToolDefinition {
        id: tool_id.to_owned(),
        reason,
    };
    if surface.protocol_version != SURFACE_PROTOCOL_VERSION {
        return Err(invalid(format!(
            "unsupported Surface protocol {}",
            surface.protocol_version
        )));
    }
    if surface.api_version != SURFACE_API_VERSION {
        return Err(invalid(format!(
            "unsupported Surface API {}",
            surface.api_version
        )));
    }
    if surface.variants.is_empty() && surface.fallback_scene.is_none() {
        return Err(invalid(
            "Surface manifest must declare a runtime variant or fallback scene".to_owned(),
        ));
    }
    if surface.state_schema_version == 0 {
        return Err(invalid(
            "Surface state schema version must be at least 1".to_owned(),
        ));
    }
    for variant in &surface.variants {
        validate_surface_entry_path(tool_id, &variant.entry)?;
        let expected_extension = match variant.runtime {
            SurfaceRuntimeKind::Declarative => "json",
            SurfaceRuntimeKind::Javascript => "js",
            SurfaceRuntimeKind::Shader => "json",
            SurfaceRuntimeKind::LoomRemote => "json",
        };
        if Path::new(&variant.entry)
            .extension()
            .and_then(|value| value.to_str())
            != Some(expected_extension)
        {
            return Err(invalid(format!(
                "Surface {:?} entry must use .{expected_extension}",
                variant.runtime
            )));
        }
        for capability in &variant.required_capabilities {
            if !is_safe_surface_identifier(capability) {
                return Err(invalid(format!(
                    "unsafe Surface capability id {capability}"
                )));
            }
        }
    }
    if let Some(fallback) = &surface.fallback_scene {
        validate_surface_entry_path(tool_id, fallback)?;
        if Path::new(fallback)
            .extension()
            .and_then(|value| value.to_str())
            != Some("json")
        {
            return Err(invalid("Surface fallback scene must use .json".to_owned()));
        }
    }
    let mut migration_sources = HashSet::new();
    for migration in &surface.migrations {
        if migration.from == 0
            || migration.to == 0
            || migration.from >= migration.to
            || migration.to > surface.state_schema_version
        {
            return Err(invalid(format!(
                "Surface migration {} -> {} is invalid for state schema {}",
                migration.from, migration.to, surface.state_schema_version
            )));
        }
        if !migration_sources.insert(migration.from) {
            return Err(invalid(format!(
                "Surface state schema {} has more than one migration",
                migration.from
            )));
        }
        validate_surface_entry_path(tool_id, &migration.entry)?;
        if Path::new(&migration.entry)
            .extension()
            .and_then(|value| value.to_str())
            != Some("json")
        {
            return Err(invalid(
                "Surface state migration entries must use .json".to_owned(),
            ));
        }
    }
    for node in &surface.required_nodes {
        if !is_safe_surface_identifier(node) {
            return Err(invalid(format!("unsafe Surface node type {node}")));
        }
    }
    for capability in &surface.required_capabilities {
        if !is_safe_surface_identifier(capability) {
            return Err(invalid(format!(
                "unsafe Surface capability id {capability}"
            )));
        }
    }
    let mut view_ids = HashSet::new();
    for view in &surface.views {
        if !is_safe_surface_identifier(&view.id) {
            return Err(invalid(format!("unsafe Surface view id {}", view.id)));
        }
        if !view_ids.insert(view.id.as_str()) {
            return Err(invalid(format!("duplicate Surface view id {}", view.id)));
        }
        if view.label.trim().is_empty() || view.label.chars().count() > 80 {
            return Err(invalid(format!(
                "Surface view {} must declare a non-empty label of at most 80 characters",
                view.id
            )));
        }
        if view.full_size.width == 0
            || view.full_size.height == 0
            || view.full_size.width > 16_384
            || view.full_size.height > 16_384
        {
            return Err(invalid(format!(
                "Surface view {} full size must be between 1 and 16384 pixels",
                view.id
            )));
        }
    }
    if let Some(default_view_id) = surface.default_view_id.as_deref() {
        if !view_ids.contains(default_view_id) {
            return Err(invalid(format!(
                "Surface default view id {default_view_id} is not declared"
            )));
        }
    } else if !surface.views.is_empty() {
        return Err(invalid(
            "Surface manifests with views must declare defaultViewId".to_owned(),
        ));
    }
    let mut action_ids = HashSet::new();
    for action in &surface.actions {
        if !is_safe_surface_identifier(&action.id) {
            return Err(invalid(format!("unsafe Surface action id {}", action.id)));
        }
        if !action_ids.insert(action.id.as_str()) {
            return Err(invalid(format!(
                "duplicate Surface action id {}",
                action.id
            )));
        }
        if action.risk == SurfaceActionRisk::High && !action.confirmation {
            return Err(invalid(format!(
                "high-risk Surface action {} must require Host confirmation",
                action.id
            )));
        }
        if action
            .timeout_ms
            .is_some_and(|timeout| timeout == 0 || timeout > 300_000)
        {
            return Err(invalid(format!(
                "Surface action {} timeout must be between 1 and 300000 ms",
                action.id
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_surface_entry_path(tool_id: &str, entry: &str) -> ToolRegistryResult<()> {
    let path = Path::new(entry);
    let safe = !entry.trim().is_empty()
        && !entry.contains('\\')
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        });
    if safe {
        Ok(())
    } else {
        Err(ToolRegistryError::InvalidToolDefinition {
            id: tool_id.to_owned(),
            reason: format!("Surface entry path is unsafe: {entry}"),
        })
    }
}
pub(super) fn execution_type_name(execution: &ToolExecution) -> &'static str {
    match execution {
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Mcp { .. } => "mcp",
        ToolExecution::Workflow { .. } => "workflow",
        ToolExecution::FrameworkArt { .. } => "framework_art",
    }
}

pub(super) fn require_non_empty(tool_id: &str, value: &str, field: &str) -> ToolRegistryResult<()> {
    if value.trim().is_empty() {
        return Err(ToolRegistryError::InvalidToolDefinition {
            id: tool_id.to_owned(),
            reason: format!("{field} is required"),
        });
    }
    Ok(())
}

pub(super) fn require_no_path_separator(tool_id: &str, value: &str) -> ToolRegistryResult<()> {
    if value.contains("..") || value.contains('/') || value.contains('\\') || value.contains(':') {
        return Err(ToolRegistryError::InvalidToolDefinition {
            id: tool_id.to_owned(),
            reason: "id cannot contain path separators".to_owned(),
        });
    }
    Ok(())
}
