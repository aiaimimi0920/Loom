//! Preview ordering, policy, and formal-output separation regressions.

use super::*;

#[test]
fn workflow_runtime_emits_preview_before_non_required_formal_failure() {
    let root = temp_root("preview-before-final");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "preview-before-final",
            r#"name: Preview Before Final
nodes:
  - id: formal
    uses: test.publisher/missing-formal-tool
  - id: preview
    uses: __sticker__
"#,
        )
        .expect("save workflow");
    let bindings = WorkflowExecutionBindings {
        preview_output: Some(output_binding("preview", "output_image")),
        preview_required_nodes: vec!["preview".to_owned()],
        primary_output: Some(output_binding("formal", "result")),
        ..WorkflowExecutionBindings::default()
    };
    let mut previews = Vec::new();

    let error = execute_tool_with_workflows_and_preview(
        &workflow_tool_with_bindings("preview-before-final", bindings),
        &[],
        &workflow_store,
        &tool_registry,
        json!({ "input_base64": TEST_IMAGE }),
        |preview| previews.push(preview),
    )
    .expect_err("formal node should still execute and fail");

    assert!(matches!(
        error,
        WorkflowRuntimeError::ChildToolNotFound { .. }
    ));
    assert_eq!(previews.len(), 1);
    assert_eq!(previews[0]["content"][0]["data"], TEST_IMAGE);
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}

#[test]
fn workflow_runtime_required_node_blocks_preview() {
    let root = temp_root("required-blocks-preview");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "required-blocks-preview",
            r#"name: Required Blocks Preview
nodes:
  - id: required
    uses: test.publisher/missing-required-tool
  - id: preview
    uses: __sticker__
"#,
        )
        .expect("save workflow");
    let bindings = WorkflowExecutionBindings {
        preview_output: Some(output_binding("preview", "output_image")),
        preview_required_nodes: vec!["required".to_owned(), "preview".to_owned()],
        ..WorkflowExecutionBindings::default()
    };
    let mut previews = Vec::new();

    let error = execute_tool_with_workflows_and_preview(
        &workflow_tool_with_bindings("required-blocks-preview", bindings),
        &[],
        &workflow_store,
        &tool_registry,
        json!({ "input_base64": TEST_IMAGE }),
        |preview| previews.push(preview),
    )
    .expect_err("required node should fail before preview publication");

    assert!(matches!(
        error,
        WorkflowRuntimeError::ChildToolNotFound { .. }
    ));
    assert!(previews.is_empty());
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}

#[test]
fn workflow_runtime_keeps_preview_and_formal_outputs_separate() {
    let root = temp_root("separate-preview-formal");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "separate-preview-formal",
            r#"name: Separate Preview And Formal
nodes:
  - id: preview
    uses: __sticker__
  - id: formal
    uses: __sticker__
"#,
        )
        .expect("save workflow");
    let bindings = WorkflowExecutionBindings {
        inputs: vec![
            WorkflowInputBinding {
                workflow_param: "input".to_owned(),
                node_id: "preview".to_owned(),
                target: "image".to_owned(),
                kind: "input_image".to_owned(),
            },
            WorkflowInputBinding {
                workflow_param: "input_2".to_owned(),
                node_id: "formal".to_owned(),
                target: "image".to_owned(),
                kind: "input_image".to_owned(),
            },
        ],
        primary_output: Some(output_binding("formal", "output_image")),
        preview_output: Some(output_binding("preview", "output_image")),
        preview_required_nodes: vec!["preview".to_owned()],
    };
    let mut previews = Vec::new();

    let result = execute_tool_with_workflows_and_preview(
        &workflow_tool_with_bindings("separate-preview-formal", bindings),
        &[],
        &workflow_store,
        &tool_registry,
        json!({
            "input": TEST_IMAGE,
            "input_2": TEST_REFERENCE_IMAGE,
        }),
        |preview| previews.push(preview),
    )
    .expect("execute workflow");

    assert_eq!(previews.len(), 1);
    assert_eq!(previews[0]["content"][0]["data"], TEST_IMAGE);
    assert_eq!(result["content"][0]["data"], TEST_REFERENCE_IMAGE);
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}

#[test]
fn workflow_runtime_without_preview_binding_keeps_single_result_behavior() {
    let root = temp_root("no-preview-binding");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "no-preview-binding",
            "name: No Preview\nnodes:\n  - id: only\n    uses: __sticker__\n",
        )
        .expect("save workflow");
    let mut preview_count = 0;

    let result = execute_tool_with_workflows_and_preview(
        &workflow_tool("no-preview-binding"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({ "input_base64": TEST_IMAGE }),
        |_| preview_count += 1,
    )
    .expect("execute workflow");

    assert_eq!(preview_count, 0);
    assert_eq!(result["content"][0]["data"], TEST_IMAGE);
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}

#[test]
fn workflow_runtime_rejects_invalid_preview_node_and_port() {
    let root = temp_root("invalid-preview-policy");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "invalid-preview-policy",
            "name: Invalid Preview\nnodes:\n  - id: child\n    uses: test.publisher/child-tool\n",
        )
        .expect("save workflow");
    let mut child = ToolDefinition::new(
        "child-tool",
        "Child Tool",
        "Preview output fixture",
        ToolExecution::CloudApi {
            endpoint: "https://example.invalid".to_owned(),
            method: "GET".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    child.metadata = Some(json!({
        "packageSecurity": {
            "publisher": { "id": "test.publisher", "name": "Test Publisher" }
        }
    }));
    child.outputs = vec![json!({ "name": "image", "type": "image" })];
    tool_registry.save_tool(child).expect("save child tool");

    for (node_id, output) in [("missing", "image"), ("child", "missing-output")] {
        let bindings = WorkflowExecutionBindings {
            preview_output: Some(output_binding(node_id, output)),
            ..WorkflowExecutionBindings::default()
        };
        let error = execute_tool_with_workflows_and_preview(
            &workflow_tool_with_bindings("invalid-preview-policy", bindings),
            &[],
            &workflow_store,
            &tool_registry,
            json!({}),
            |_| panic!("invalid preview policy must not emit"),
        )
        .expect_err("invalid preview policy");
        assert!(matches!(
            error,
            WorkflowRuntimeError::InvalidPreviewPolicy { .. }
        ));
    }
    fs::remove_dir_all(root).expect("cleanup temp workflow runtime root");
}
