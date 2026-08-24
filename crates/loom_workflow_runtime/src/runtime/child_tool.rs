//! Immutable locked-child resolution with registry fallback policy.

use super::*;

pub(super) fn resolve_workflow_child_tool(
    parent_tool: &ToolDefinition,
    child_id: &str,
    tool_registry: &ToolRegistry,
) -> WorkflowRuntimeResult<Option<ToolDefinition>> {
    let locked = parent_tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("lockedArts"))
        .and_then(JsonValue::as_object);
    if let Some(locked) = locked {
        if let Some(tool) = locked.get(child_id) {
            return serde_json::from_value::<ToolDefinition>(tool.clone())
                .map(Some)
                .map_err(|error| ToolRegistryError::InvalidToolDefinition {
                    id: child_id.to_owned(),
                    reason: format!("locked workflow child is invalid: {error}"),
                })
                .map_err(WorkflowRuntimeError::from);
        }
        for tool in locked.values() {
            let candidate =
                serde_json::from_value::<ToolDefinition>(tool.clone()).map_err(|error| {
                    ToolRegistryError::InvalidToolDefinition {
                        id: child_id.to_owned(),
                        reason: format!("locked workflow child is invalid: {error}"),
                    }
                })?;
            if candidate.qualified_id() == child_id {
                return Ok(Some(candidate));
            }
        }
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
