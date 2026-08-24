//! Shared workflow tools, bindings, image constants, and temporary stores.

use super::*;

pub(super) const TEST_IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPgEpH7DwABpAE8k4sOtwAAAABJRU5ErkJggg==";
pub(super) const TEST_REFERENCE_IMAGE: &str = "data:image/png;base64,cmVmZXJlbmNlLWltYWdl";

pub(super) fn temp_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("loom-workflow-runtime-{name}-{nonce}"));
    fs::create_dir_all(&root).expect("create temp workflow runtime root");
    root
}

pub(super) fn workflow_tool(workflow_id: &str) -> ToolDefinition {
    let mut tool = ToolDefinition::new(
        "fixture-workflow",
        "Fixture Workflow",
        "Workflow child runner",
        ToolExecution::Workflow {
            workflow_id: workflow_id.to_owned(),
            workflow_bindings: None,
        },
    );
    tool.metadata = Some(json!({
        "packageSecurity": {
            "publisher": { "id": "test.publisher", "name": "Test Publisher" }
        }
    }));
    tool
}

pub(super) fn workflow_tool_with_bindings(
    workflow_id: &str,
    bindings: WorkflowExecutionBindings,
) -> ToolDefinition {
    ToolDefinition::new(
        "fixture-workflow",
        "Fixture Workflow",
        "Workflow child runner",
        ToolExecution::Workflow {
            workflow_id: workflow_id.to_owned(),
            workflow_bindings: Some(bindings),
        },
    )
}

pub(super) fn output_binding(node_id: &str, output: &str) -> WorkflowOutputBinding {
    WorkflowOutputBinding {
        node_id: node_id.to_owned(),
        output: output.to_owned(),
        kind: "node_result".to_owned(),
    }
}
