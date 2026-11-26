//! Tests for dynamic placement, intra-workflow cross-end Loom <-> Otter relay, and SemVer child rebinding.

use super::*;
use loom_tool_registry::ToolExecution;

struct MockTerminalRelay {
    connected: bool,
    device_id: String,
    calls: Vec<(String, String, JsonValue)>,
}

impl WorkflowTerminalRelay for MockTerminalRelay {
    fn has_connected_terminal(&self, _preferred_device_id: Option<&str>) -> bool {
        self.connected
    }

    fn default_device_id(&self) -> Option<String> {
        Some(self.device_id.clone())
    }

    fn execute_on_terminal(
        &mut self,
        request: TerminalNodeRelayRequest<'_>,
    ) -> WorkflowRuntimeResult<JsonValue> {
        self.calls.push((
            request.node_id.to_owned(),
            request.uses.to_owned(),
            request.arguments.clone(),
        ));
        let upstream_text = request
            .arguments
            .get("prompt")
            .and_then(JsonValue::as_str)
            .unwrap_or_default();
        Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("shader_gpu({upstream_text})@{}", self.device_id)
            }]
        }))
    }
}

fn temp_workflow_env(label: &str) -> (std::path::PathBuf, ToolRegistry, WorkflowStore) {
    let root = std::env::temp_dir().join(format!(
        "loom-hybrid-wf-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp dir");
    let registry = ToolRegistry::new(root.join("tools"));
    let store = WorkflowStore::new(root.join("workflows"));
    (root, registry, store)
}

#[test]
fn composite_workflow_relays_subgraph_from_loom_to_otter_terminal_and_back_to_loom() {
    let (root, registry, store) = temp_workflow_env("relay");

    store
        .save_workflow("prep_wf", "nodes: []\n")
        .expect("save prep");
    store
        .save_workflow(
            "post_finalize_wf",
            "nodes:\n  - id: echo_back\n    uses: neuro.official/loom-prep\n",
        )
        .expect("save post_finalize_wf");

    let mut prep_tool = ToolDefinition::new(
        "loom-prep",
        "Loom Prep",
        "Runs centrally on Loom",
        ToolExecution::Workflow {
            workflow_id: "prep_wf".to_owned(),
            workflow_bindings: None,
        },
    );
    prep_tool.metadata = Some(json!({
        "packageSecurity": { "publisher": { "id": "neuro.official" } }
    }));
    registry.save_tool(prep_tool).expect("save prep tool");

    let mut shader_tool = ToolDefinition::new(
        "gpu-bloom",
        "GPU Bloom Shader",
        "Terminal GPU shader",
        ToolExecution::FrameworkArt {
            framework: "shader".to_owned(),
        },
    );
    shader_tool.metadata = Some(json!({
        "packageSecurity": { "publisher": { "id": "neuro.official" } },
        "capabilities": { "preview": "shader", "requiresTerminalGpu": true }
    }));
    registry.save_tool(shader_tool).expect("save shader tool");

    let mut post_tool = ToolDefinition::new(
        "post-finalize",
        "Post Finalize",
        "Runs back on Loom",
        ToolExecution::Workflow {
            workflow_id: "post_finalize_wf".to_owned(),
            workflow_bindings: None,
        },
    );
    post_tool.metadata = Some(json!({
        "packageSecurity": { "publisher": { "id": "neuro.official" } }
    }));
    registry.save_tool(post_tool).expect("save post tool");

    store
        .save_workflow(
            "composite_hybrid",
            concat!(
                "nodes:\n",
                "  - id: step1_loom\n",
                "    uses: neuro.official/loom-prep\n",
                "  - id: step2_terminal_gpu\n",
                "    uses: neuro.official/gpu-bloom\n",
                "    needs: [step1_loom]\n",
                "    with:\n",
                "      prompt: ${{ nodes.step1_loom.outputs.text }}\n",
                "  - id: step3_loom_finalize\n",
                "    uses: neuro.official/post-finalize\n",
                "    placement: loom_central\n",
                "    needs: [step2_terminal_gpu]\n",
            ),
        )
        .expect("save composite workflow");

    let root_workflow_art = ToolDefinition::new(
        "hybrid-pipeline",
        "Hybrid Pipeline Art",
        "Loom -> Otter GPU -> Loom",
        ToolExecution::Workflow {
            workflow_id: "composite_hybrid".to_owned(),
            workflow_bindings: Some(WorkflowExecutionBindings {
                inputs: Vec::new(),
                primary_output: Some(WorkflowOutputBinding {
                    node_id: "step2_terminal_gpu".to_owned(),
                    kind: "node_result".to_owned(),
                    output: String::new(),
                }),
                preview_output: None,
                ..WorkflowExecutionBindings::default()
            }),
        },
    );

    let mut relay = MockTerminalRelay {
        connected: true,
        device_id: "device-livingroom-pc".to_owned(),
        calls: Vec::new(),
    };

    let output = execute_tool_with_workflows_and_relay(
        &root_workflow_art,
        &[],
        &store,
        &registry,
        json!({}),
        Some(Duration::from_secs(5)),
        None,
        Some(&mut relay),
        |_| {},
    )
    .expect("execute hybrid workflow");

    assert_eq!(relay.calls.len(), 1);
    assert_eq!(relay.calls[0].0, "step2_terminal_gpu");
    assert!(output["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .contains("@device-livingroom-pc"));
    let trace = output["loomMetadata"]["placementTrace"]
        .as_array()
        .expect("placement trace recorded");
    assert_eq!(trace[0]["nodeId"], "step1_loom");
    assert_eq!(trace[0]["target"], "loom_central");
    assert_eq!(trace[1]["nodeId"], "step2_terminal_gpu");
    assert_eq!(trace[1]["target"], "otter_terminal");
    assert_eq!(trace[1]["deviceId"], "device-livingroom-pc");
    assert!(trace
        .iter()
        .any(|step| step["nodeId"] == "step3_loom_finalize" && step["target"] == "loom_central"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn semver_child_art_rebinding_upgrades_compatible_minor_and_ignores_major_breaking() {
    let (root, registry, _store) = temp_workflow_env("semver");

    let mut locked_v1 = ToolDefinition::new(
        "upscale",
        "Upscale 1.0.0",
        "locked v1.0.0",
        ToolExecution::Workflow {
            workflow_id: "v1_wf".to_owned(),
            workflow_bindings: None,
        },
    );
    locked_v1.metadata = Some(json!({
        "packageSecurity": {
            "version": "1.0.0",
            "publisher": { "id": "neuro.official" }
        }
    }));

    let mut installed_v1_2 = locked_v1.clone();
    installed_v1_2.description = "compatible v1.2.0".to_owned();
    installed_v1_2.metadata = Some(json!({
        "packageSecurity": {
            "version": "1.2.0",
            "publisher": { "id": "neuro.official" }
        }
    }));
    registry
        .save_tool(installed_v1_2)
        .expect("save v1.2.0 in registry");

    let mut parent_with_range = ToolDefinition::new(
        "parent-wf",
        "Parent",
        "Parent workflow",
        ToolExecution::Workflow {
            workflow_id: "parent".to_owned(),
            workflow_bindings: None,
        },
    );
    parent_with_range.metadata = Some(json!({
        "dependencies": {
            "arts": ["neuro.official/upscale"],
            "artVersionRanges": {
                "neuro.official/upscale": "^1.0.0"
            }
        },
        "artPackage": {
            "lockedArts": {
                "neuro.official/upscale": locked_v1
            }
        }
    }));

    let resolved =
        resolve_workflow_child_tool(&parent_with_range, "neuro.official/upscale", &registry)
            .expect("resolve compatible")
            .expect("child found");
    assert_eq!(resolved.description, "compatible v1.2.0");

    let mut installed_v2 = resolved;
    installed_v2.description = "breaking v2.0.0".to_owned();
    installed_v2.metadata = Some(json!({
        "packageSecurity": {
            "version": "2.0.0",
            "publisher": { "id": "neuro.official" }
        }
    }));
    registry.save_tool(installed_v2).expect("save v2.0.0");

    let fallback =
        resolve_workflow_child_tool(&parent_with_range, "neuro.official/upscale", &registry)
            .expect("resolve fallback")
            .expect("locked child returned");
    assert_eq!(fallback.description, "locked v1.0.0");

    let _ = std::fs::remove_dir_all(root);
}
