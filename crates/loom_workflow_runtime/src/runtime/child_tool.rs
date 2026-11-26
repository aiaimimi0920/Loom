//! Immutable locked-child resolution with optional SemVer-compatible child Art rebinding.

use super::*;

fn tool_semver(tool: &ToolDefinition) -> Option<Version> {
    let raw = tool
        .metadata
        .as_ref()
        .and_then(|m| {
            m.pointer("/packageSecurity/version")
                .or_else(|| m.pointer("/artPackage/version"))
        })
        .and_then(JsonValue::as_str)?;
    Version::parse(raw.trim()).ok()
}

fn configured_child_version_range(
    parent_tool: &ToolDefinition,
    child_id: &str,
    locked_candidate: Option<&ToolDefinition>,
) -> WorkflowRuntimeResult<Option<VersionReq>> {
    if let Some(ranges) = parent_tool
        .metadata
        .as_ref()
        .and_then(|m| m.pointer("/dependencies/artVersionRanges"))
        .and_then(JsonValue::as_object)
    {
        let raw_range = ranges.get(child_id).or_else(|| {
            locked_candidate.and_then(|locked| {
                ranges
                    .get(&locked.qualified_id())
                    .or_else(|| ranges.get(&locked.id))
            })
        });
        if let Some(raw) = raw_range {
            let raw_str = raw
                .as_str()
                .ok_or_else(|| ToolRegistryError::InvalidToolDefinition {
                    id: child_id.to_owned(),
                    reason: "artVersionRanges requirement must be a SemVer string".to_owned(),
                })?;
            let parsed = VersionReq::parse(raw_str.trim()).map_err(|error| {
                ToolRegistryError::InvalidToolDefinition {
                    id: child_id.to_owned(),
                    reason: format!(
                        "invalid artVersionRanges SemVer requirement `{raw_str}`: {error}"
                    ),
                }
            })?;
            return Ok(Some(parsed));
        }
    }

    let auto_rebind = parent_tool
        .metadata
        .as_ref()
        .and_then(|m| m.pointer("/artSettings/rebindCompatibleChildren"))
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    if auto_rebind {
        if let Some(locked_ver) = locked_candidate.and_then(tool_semver) {
            if let Ok(req) = VersionReq::parse(&format!("^{locked_ver}")) {
                return Ok(Some(req));
            }
        }
    }
    Ok(None)
}

fn find_registry_semver_match(
    child_id: &str,
    locked_candidate: Option<&ToolDefinition>,
    version_req: &VersionReq,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<Option<ToolDefinition>> {
    let expected_qualified = locked_candidate
        .map(ToolDefinition::qualified_id)
        .filter(|q| q.contains('/'));
    let locked_version = locked_candidate.and_then(tool_semver);

    let mut best: Option<(Version, ToolDefinition)> = None;
    for candidate in tool_registry.list_tools()? {
        if !candidate.enabled {
            continue;
        }
        let id_matches = candidate.id == child_id
            || candidate.qualified_id() == child_id
            || expected_qualified
                .as_deref()
                .is_some_and(|expected| candidate.qualified_id() == expected);
        if !id_matches {
            continue;
        }
        if let Some(expected) = expected_qualified.as_deref() {
            if candidate.qualified_id() != expected {
                continue;
            }
        }
        let Some(ver) = tool_semver(&candidate) else {
            continue;
        };
        if !version_req.matches(&ver) {
            continue;
        }
        if locked_version.as_ref().is_some_and(|locked| &ver <= locked) {
            continue;
        }
        if best.as_ref().is_none_or(|(best_ver, _)| &ver > best_ver) {
            best = Some((ver, candidate));
        }
    }
    Ok(best.map(|(_, tool)| tool))
}

pub(super) fn resolve_workflow_child_tool(
    parent_tool: &ToolDefinition,
    child_id: &str,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<Option<ToolDefinition>> {
    let mut locked_candidate: Option<ToolDefinition> = None;
    let locked = parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("lockedArts"))
        .and_then(JsonValue::as_object);
    if let Some(locked) = locked {
        if let Some(tool) = locked.get(child_id) {
            locked_candidate = Some(
                serde_json::from_value::<ToolDefinition>(tool.clone()).map_err(|error| {
                    ToolRegistryError::InvalidToolDefinition {
                        id: child_id.to_owned(),
                        reason: format!("locked workflow child is invalid: {error}"),
                    }
                })?,
            );
        } else {
            for tool in locked.values() {
                let candidate =
                    serde_json::from_value::<ToolDefinition>(tool.clone()).map_err(|error| {
                        ToolRegistryError::InvalidToolDefinition {
                            id: child_id.to_owned(),
                            reason: format!("locked workflow child is invalid: {error}"),
                        }
                    })?;
                if candidate.qualified_id() == child_id {
                    locked_candidate = Some(candidate);
                    break;
                }
            }
        }
    }

    if let Some(version_req) =
        configured_child_version_range(parent_tool, child_id, locked_candidate.as_ref())?
    {
        if let Some(rebound) = find_registry_semver_match(
            child_id,
            locked_candidate.as_ref(),
            &version_req,
            tool_registry,
        )? {
            return Ok(Some(rebound));
        }
        if let Some(locked_tool) = locked_candidate {
            if tool_semver(&locked_tool).is_some_and(|ver| version_req.matches(&ver)) {
                return Ok(Some(locked_tool));
            }
            return Ok(None);
        }
        return Ok(None);
    }

    if let Some(locked_tool) = locked_candidate {
        return Ok(Some(locked_tool));
    }

    let declared_locked_dependency = parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("dependencies"))
        .and_then(|dependencies| dependencies.get("arts"))
        .and_then(JsonValue::as_array)
        .is_some_and(|arts| {
            arts.iter().any(|dependency| {
                dependency
                    .as_str()
                    .is_some_and(|dependency| dependency == child_id)
            })
        });
    if parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .is_some()
        && declared_locked_dependency
    {
        return Ok(None);
    }

    tool_registry
        .get_tool(child_id)
        .map_err(WorkflowRuntimeError::from)
}
