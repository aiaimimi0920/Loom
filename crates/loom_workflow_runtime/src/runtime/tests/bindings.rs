//! Node references, binding precedence, and missing-input regressions.

use super::*;

#[test]
fn workflow_runtime_preserves_named_secondary_image_references() {
    let node = StoredWorkflowNode {
        id: "color".to_owned(),
        uses: "color-transfer".to_owned(),
        needs: vec!["input-source".to_owned(), "reference-source".to_owned()],
        params: BTreeMap::from([
            (
                "input".to_owned(),
                serde_yaml::Value::String(
                    "${{ nodes.input-source.outputs.output_image }}".to_owned(),
                ),
            ),
            (
                "reference".to_owned(),
                serde_yaml::Value::String(
                    "${{ nodes.reference-source.outputs.output_image }}".to_owned(),
                ),
            ),
        ]),
        meta: None,
    };
    let results = HashMap::from([
        (
            "input-source".to_owned(),
            image_content_response(TEST_IMAGE, "image/png"),
        ),
        (
            "reference-source".to_owned(),
            image_content_response(TEST_REFERENCE_IMAGE, "image/png"),
        ),
    ]);

    let (mut child_args, child_input) = resolve_node_params(&node, &results);
    assert_eq!(child_input.as_deref(), Some(TEST_IMAGE));
    assert_eq!(
        child_args.get("reference"),
        Some(&JsonValue::String(TEST_REFERENCE_IMAGE.to_owned()))
    );
    insert_child_input(
        &mut child_args,
        child_input.as_deref().expect("primary image input"),
    );
    assert_eq!(
        child_args.get("input"),
        Some(&JsonValue::String(TEST_IMAGE.to_owned()))
    );
}

#[test]
fn workflow_param_binding_overrides_baked_value_from_nested_params() {
    let node = StoredWorkflowNode {
        id: "transfer".to_owned(),
        uses: "color-transfer".to_owned(),
        needs: vec![],
        params: BTreeMap::from([("strength".to_owned(), serde_yaml::Value::Number(20.into()))]),
        meta: None,
    };
    let bindings = WorkflowExecutionBindings {
        inputs: vec![WorkflowInputBinding {
            workflow_param: "strength".to_owned(),
            node_id: node.id.clone(),
            target: "strength".to_owned(),
            kind: "param".to_owned(),
        }],
        primary_output: None,
        ..WorkflowExecutionBindings::default()
    };
    let (mut child_args, mut child_input) = resolve_node_params(&node, &HashMap::new());

    apply_input_bindings(
        "binding-flow",
        &bindings,
        &node,
        &json!({ "params": { "strength": 87 } }),
        &None,
        &mut child_input,
        &mut child_args,
    )
    .expect("apply workflow parameter binding");

    assert_eq!(child_args.get("strength"), Some(&json!(87)));
}

#[test]
fn workflow_param_binding_keeps_baked_value_when_argument_is_missing() {
    let node = StoredWorkflowNode {
        id: "transfer".to_owned(),
        uses: "color-transfer".to_owned(),
        needs: vec![],
        params: BTreeMap::from([("strength".to_owned(), serde_yaml::Value::Number(20.into()))]),
        meta: None,
    };
    let bindings = WorkflowExecutionBindings {
        inputs: vec![WorkflowInputBinding {
            workflow_param: "strength".to_owned(),
            node_id: node.id.clone(),
            target: "strength".to_owned(),
            kind: "param".to_owned(),
        }],
        primary_output: None,
        ..WorkflowExecutionBindings::default()
    };
    let (mut child_args, mut child_input) = resolve_node_params(&node, &HashMap::new());

    apply_input_bindings(
        "binding-flow",
        &bindings,
        &node,
        &json!({ "params": {} }),
        &None,
        &mut child_input,
        &mut child_args,
    )
    .expect("apply workflow parameter binding");

    assert_eq!(child_args.get("strength"), Some(&json!(20)));
}

#[test]
fn bound_workflow_argument_prefers_top_level_then_params_then_inputs() {
    let binding = WorkflowInputBinding {
        workflow_param: "strength".to_owned(),
        node_id: "transfer".to_owned(),
        target: "strength".to_owned(),
        kind: "param".to_owned(),
    };

    assert_eq!(
        bound_argument_value(
            &binding,
            &json!({
                "strength": 90,
                "params": { "strength": 80 },
                "inputs": { "strength": 70 }
            }),
        ),
        Some(json!(90)),
    );
    assert_eq!(
        bound_argument_value(
            &binding,
            &json!({
                "params": { "strength": 80 },
                "inputs": { "strength": 70 }
            }),
        ),
        Some(json!(80)),
    );
    assert_eq!(
        bound_argument_value(&binding, &json!({ "inputs": { "strength": 70 } })),
        Some(json!(70)),
    );
}

#[test]
fn workflow_runtime_rejects_missing_explicit_image_binding() {
    let root = temp_root("missing-explicit-image-binding");
    let workflow_store = WorkflowStore::new(root.join("workflows"));
    let tool_registry = ToolRegistry::new(root.join("tools"));
    let node = StoredWorkflowNode {
        id: "reference-source".to_owned(),
        uses: "__sticker__".to_owned(),
        needs: vec![],
        params: BTreeMap::new(),
        meta: None,
    };
    let bindings = WorkflowExecutionBindings {
        inputs: vec![WorkflowInputBinding {
            workflow_param: "input_2".to_owned(),
            node_id: node.id.clone(),
            target: "image".to_owned(),
            kind: "input_image".to_owned(),
        }],
        primary_output: None,
        ..WorkflowExecutionBindings::default()
    };
    let root_input = Some(TEST_IMAGE.to_owned());

    let error = execute_workflow_node(
        "missing-binding-flow",
        &workflow_tool("missing-binding-flow"),
        &node,
        Some(&bindings),
        &root_input,
        &json!({ "input_base64": TEST_IMAGE }),
        &HashMap::new(),
        &[],
        &workflow_store,
        &tool_registry,
        None,
        None,
        &mut ExecutionContext::default(),
    )
    .expect_err("missing input_2 must not reuse the root input");

    assert!(matches!(
        error,
        WorkflowRuntimeError::MissingImageInput {
            workflow_id,
            node_id
        } if workflow_id == "missing-binding-flow" && node_id == "reference-source"
    ));
    fs::remove_dir_all(root).expect("cleanup missing binding root");
}
