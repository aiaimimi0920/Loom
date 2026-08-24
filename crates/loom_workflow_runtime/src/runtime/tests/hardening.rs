//! Malformed workflow, recursion, capability, and result-budget regressions.

use super::*;

#[test]
fn workflow_runtime_rejects_duplicate_and_missing_node_references() {
    let root = temp_root("invalid-node-graph");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    for (workflow_id, yaml, expected) in [
        (
            "duplicate-node",
            "nodes:\n  - id: duplicate\n    uses: __sticker__\n  - id: duplicate\n    uses: __sticker__\n",
            "duplicate node id",
        ),
        (
            "missing-dependency",
            "nodes:\n  - id: child\n    uses: __sticker__\n    needs: [absent]\n",
            "depends on missing node",
        ),
    ] {
        workflow_store
            .save_workflow(workflow_id, yaml)
            .expect("save malformed workflow fixture");
        let error = execute_tool_with_workflows(
            &workflow_tool(workflow_id),
            &[],
            &workflow_store,
            &tool_registry,
            json!({}),
        )
        .expect_err("malformed workflow must be rejected");
        assert!(
            matches!(&error, WorkflowRuntimeError::InvalidWorkflow { reason, .. } if reason.contains(expected)),
            "unexpected validation error: {error}"
        );
    }
    fs::remove_dir_all(root).expect("cleanup invalid graph root");
}

#[test]
fn workflow_runtime_rejects_invalid_binding_shape_before_execution() {
    let root = temp_root("invalid-binding");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "invalid-binding",
            "nodes:\n  - id: sticker\n    uses: __sticker__\n",
        )
        .expect("save workflow");
    for (inputs, expected) in [
        (
            vec![WorkflowInputBinding {
                workflow_param: "input".to_owned(),
                node_id: "sticker".to_owned(),
                target: "image".to_owned(),
                kind: "ambient_file".to_owned(),
            }],
            "unsupported input binding kind",
        ),
        (
            vec![WorkflowInputBinding {
                workflow_param: "input".to_owned(),
                node_id: "missing".to_owned(),
                target: "image".to_owned(),
                kind: "input_image".to_owned(),
            }],
            "references missing node",
        ),
        (
            vec![
                WorkflowInputBinding {
                    workflow_param: "first".to_owned(),
                    node_id: "sticker".to_owned(),
                    target: "image".to_owned(),
                    kind: "input_image".to_owned(),
                },
                WorkflowInputBinding {
                    workflow_param: "second".to_owned(),
                    node_id: "sticker".to_owned(),
                    target: "image".to_owned(),
                    kind: "input_image".to_owned(),
                },
            ],
            "duplicate input target",
        ),
    ] {
        let bindings = WorkflowExecutionBindings {
            inputs,
            ..WorkflowExecutionBindings::default()
        };
        let error = execute_tool_with_workflows(
            &workflow_tool_with_bindings("invalid-binding", bindings),
            &[],
            &workflow_store,
            &tool_registry,
            json!({ "input": TEST_IMAGE }),
        )
        .expect_err("invalid binding must be rejected");
        assert!(
            matches!(&error, WorkflowRuntimeError::InvalidWorkflow { reason, .. } if reason.contains(expected)),
            "unexpected binding error: {error}"
        );
    }
    fs::remove_dir_all(root).expect("cleanup invalid binding root");
}

#[test]
fn workflow_runtime_rejects_recursive_child_workflows() {
    let root = temp_root("recursive-child");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    workflow_store
        .save_workflow(
            "recursive-flow",
            "nodes:\n  - id: child\n    uses: test.publisher/recursive-child\n",
        )
        .expect("save recursive workflow");
    let mut child = workflow_tool("recursive-flow");
    child.id = "recursive-child".to_owned();
    tool_registry
        .save_tool(child)
        .expect("save recursive workflow child");

    let error = execute_tool_with_workflows(
        &workflow_tool("recursive-flow"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({}),
    )
    .expect_err("recursive workflow must be rejected");
    assert!(matches!(
        error,
        WorkflowRuntimeError::InvalidWorkflow { reason, .. }
            if reason.contains("recursive workflow dependency")
    ));
    fs::remove_dir_all(root).expect("cleanup recursive workflow root");
}

#[test]
fn stored_metadata_cannot_read_an_ambient_local_file() {
    let root = temp_root("stored-local-path");
    let local_image = root.join("ambient.png");
    fs::write(
        &local_image,
        loom_image_io::decode_data_url_bytes(TEST_IMAGE).expect("decode fixture"),
    )
    .expect("write ambient image");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    let yaml = format!(
        "nodes:\n  - id: sticker\n    uses: __sticker__\n    meta:\n      src: '{}'\n",
        local_image.display()
    );
    workflow_store
        .save_workflow("stored-local-path", &yaml)
        .expect("save stored path workflow");

    let error = execute_tool_with_workflows(
        &workflow_tool("stored-local-path"),
        &[],
        &workflow_store,
        &tool_registry,
        json!({}),
    )
    .expect_err("stored metadata must not gain filesystem read capability");
    assert!(matches!(
        error,
        WorkflowRuntimeError::MissingImageInput { .. }
    ));
    fs::remove_dir_all(root).expect("cleanup stored path root");
}

#[test]
fn workflow_result_budget_rejects_excessive_json_depth() {
    let mut result = JsonValue::Null;
    for _ in 0..=MAX_RESULT_VALUE_DEPTH {
        result = JsonValue::Array(vec![result]);
    }

    let error = reserve_workflow_result("deep-result", "child", &result, 0)
        .expect_err("deep child output must be rejected");
    assert!(matches!(error, WorkflowRuntimeError::ResourceLimit { .. }));
}
