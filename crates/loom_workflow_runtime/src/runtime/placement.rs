//! Dynamic execution placement (`LoomCentral` vs `OtterTerminal`) and cross-end subgraph relay.

use super::*;

/// Execution placement policy for an individual workflow node.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPlacement {
    #[default]
    Auto,
    #[serde(alias = "loom", alias = "central")]
    LoomCentral,
    #[serde(alias = "otter", alias = "otter_local", alias = "terminal")]
    OtterTerminal,
}

impl ExecutionPlacement {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::LoomCentral => "loom_central",
            Self::OtterTerminal => "otter_terminal",
        }
    }
}

/// Recorded placement decision for a single executed workflow node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPlacementStep {
    pub workflow_id: String,
    pub node_id: String,
    pub uses: String,
    pub target: ExecutionPlacement,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub reason: String,
}

/// Request handed to a connected Otter terminal when a workflow node is placed on the terminal.
pub struct TerminalNodeRelayRequest<'a> {
    pub workflow_id: &'a str,
    pub node_id: &'a str,
    pub uses: &'a str,
    pub device_id: Option<&'a str>,
    pub tool: Option<&'a ToolDefinition>,
    pub arguments: JsonValue,
    pub timeout: Option<Duration>,
    pub cancellation: Option<&'a AtomicBool>,
}

/// Callback interface implemented by `loom-daemon` to relay terminal-bound subgraph nodes to Otter.
pub trait WorkflowTerminalRelay {
    fn has_connected_terminal(&self, preferred_device_id: Option<&str>) -> bool;

    fn default_device_id(&self) -> Option<String> {
        None
    }

    fn execute_on_terminal(
        &mut self,
        request: TerminalNodeRelayRequest<'_>,
    ) -> WorkflowRuntimeResult<JsonValue>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedNodePlacement {
    pub(super) target: ExecutionPlacement,
    pub(super) device_id: Option<String>,
    pub(super) reason: String,
    pub(super) explicit_terminal: bool,
}

pub(super) fn resolve_node_placement(
    node: &StoredWorkflowNode,
    child_tool: Option<&ToolDefinition>,
    relay: Option<&dyn WorkflowTerminalRelay>,
) -> ResolvedNodePlacement {
    let preferred_device = node
        .device_id
        .as_deref()
        .or_else(|| {
            node.meta
                .as_ref()
                .and_then(|meta| meta.device_id.as_deref())
        })
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .or_else(|| relay.and_then(WorkflowTerminalRelay::default_device_id));

    let declared = node
        .placement
        .or_else(|| node.meta.as_ref().and_then(|meta| meta.placement))
        .unwrap_or(ExecutionPlacement::Auto);

    match declared {
        ExecutionPlacement::LoomCentral => ResolvedNodePlacement {
            target: ExecutionPlacement::LoomCentral,
            device_id: None,
            reason: "explicit_loom_central".to_owned(),
            explicit_terminal: false,
        },
        ExecutionPlacement::OtterTerminal => ResolvedNodePlacement {
            target: ExecutionPlacement::OtterTerminal,
            device_id: preferred_device,
            reason: "explicit_otter_terminal".to_owned(),
            explicit_terminal: true,
        },
        ExecutionPlacement::Auto => {
            let (wants_terminal, requires_terminal, reason) =
                tool_prefers_terminal(node, child_tool);
            if wants_terminal {
                let terminal_ready =
                    relay.is_some_and(|r| r.has_connected_terminal(preferred_device.as_deref()));
                if terminal_ready || requires_terminal {
                    return ResolvedNodePlacement {
                        target: ExecutionPlacement::OtterTerminal,
                        device_id: preferred_device,
                        reason,
                        explicit_terminal: requires_terminal,
                    };
                }
            }
            ResolvedNodePlacement {
                target: ExecutionPlacement::LoomCentral,
                device_id: None,
                reason: "auto_loom_central".to_owned(),
                explicit_terminal: false,
            }
        }
    }
}

fn tool_prefers_terminal(
    node: &StoredWorkflowNode,
    child_tool: Option<&ToolDefinition>,
) -> (bool, bool, String) {
    if node.uses.starts_with("otter.shader.") || node.uses.starts_with("shader.") {
        return (true, true, "terminal_gpu_shader_node".to_owned());
    }
    let Some(tool) = child_tool else {
        return (false, false, String::new());
    };
    if let ToolExecution::FrameworkArt { framework } = &tool.execution {
        if matches!(
            framework.as_str(),
            "shader" | "hlsl" | "wgsl" | "glsl" | "otter_local"
        ) {
            return (true, true, format!("framework_{framework}_terminal_gpu"));
        }
    }
    if let Some(metadata) = &tool.metadata {
        if let Some(target) = metadata
            .get("executionTarget")
            .or_else(|| metadata.get("placement"))
            .and_then(JsonValue::as_str)
        {
            if matches!(
                target,
                "otter" | "otter_local" | "otter_terminal" | "terminal"
            ) {
                let required = metadata
                    .get("requiresTerminal")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(true);
                return (true, required, "tool_metadata_terminal_target".to_owned());
            }
        }
        if metadata
            .pointer("/capabilities/requiresTerminalGpu")
            .and_then(JsonValue::as_bool)
            == Some(true)
        {
            return (true, true, "requires_terminal_gpu".to_owned());
        }
        if metadata
            .pointer("/capabilities/preview")
            .and_then(JsonValue::as_str)
            == Some("shader")
            && metadata
                .pointer("/capabilities/preferTerminalExecution")
                .and_then(JsonValue::as_bool)
                == Some(true)
        {
            return (true, false, "prefer_terminal_shader".to_owned());
        }
    }
    (false, false, String::new())
}

pub(super) fn attach_placement_trace(
    mut output: JsonValue,
    trace: &[WorkflowPlacementStep],
) -> JsonValue {
    if trace.is_empty() {
        return output;
    }
    if let Some(obj) = output.as_object_mut() {
        let meta = obj
            .entry("loomMetadata".to_owned())
            .or_insert_with(|| json!({}));
        if !meta.is_object() {
            *meta = json!({});
        }
        if let Some(meta_obj) = meta.as_object_mut() {
            meta_obj.insert(
                "placementTrace".to_owned(),
                serde_json::to_value(trace).unwrap_or_else(|_| json!([])),
            );
        }
    }
    output
}
