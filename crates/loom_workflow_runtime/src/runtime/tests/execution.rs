//! Native/sticker execution, cancellation, and dependency-cycle regressions.

use super::*;

#[test]
fn workflow_runtime_executes_native_image_child() {
    let root = temp_root("native-image");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "native-flow",
            r#"name: Native Flow
nodes:
  - id: invert
    uses: core.image.invert
"#,
        )
        .expect("save workflow");

    let result = execute_tool_with_workflows(
        &workflow_tool("native-flow"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({ "input_base64": TEST_IMAGE }),
    )
    .expect("execute workflow tool");

    assert_eq!(result["content"][0]["type"], "image");
    let output = result["content"][0]["data"].as_str().expect("image data");
    assert!(output.starts_with("data:image/png;base64,"));
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}

#[test]
fn workflow_runtime_forwards_images_across_needs_edges() {
    let root = temp_root("implicit-image-edges");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "sticker-chain",
            r#"name: Sticker Chain
nodes:
  - id: a
    uses: __sticker__
  - id: b
    uses: __sticker__
    needs: [a]
  - id: c
    uses: __sticker__
    needs: [b]
"#,
        )
        .expect("save workflow");

    let result = execute_tool_with_workflows(
        &workflow_tool("sticker-chain"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({ "input_base64": TEST_IMAGE }),
    )
    .expect("execute sticker chain");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["data"], TEST_IMAGE);
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}

#[test]
fn workflow_runtime_rejects_an_already_cancelled_request() {
    let root = temp_root("already-cancelled");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    workflow_store
        .save_workflow("cancelled-flow", "name: Cancelled\nnodes: []\n")
        .expect("save cancelled workflow");
    let tool_registry = ToolRegistry::new(root.join("tools"));
    let cancellation = AtomicBool::new(true);

    let error = execute_tool_with_workflows_and_preview_timeout_and_cancellation(
        &workflow_tool("cancelled-flow"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({}),
        Duration::from_secs(1),
        &cancellation,
        |_| {},
    )
    .expect_err("cancelled workflow must not execute");

    assert!(matches!(error, WorkflowRuntimeError::Cancelled));
    fs::remove_dir_all(root).expect("cleanup cancelled workflow root");
}

#[test]
fn workflow_runtime_reports_unresolved_dependencies() {
    let root = temp_root("cycle");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "cycle-flow",
            r#"name: Cycle Flow
nodes:
  - id: a
    uses: test.publisher/fixture-script
    needs: [b]
  - id: b
    uses: test.publisher/fixture-script
    needs: [a]
"#,
        )
        .expect("save workflow");

    let error = execute_tool_with_workflows(
        &workflow_tool("cycle-flow"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({}),
    )
    .expect_err("cycle fails");

    assert!(error.to_string().contains("unresolved dependencies"));
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}
