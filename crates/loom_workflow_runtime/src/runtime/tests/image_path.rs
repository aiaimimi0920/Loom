//! Local image-path binding compatibility regression.

use super::*;

#[test]
fn workflow_runtime_resolves_local_path_image_bindings() {
    let root = temp_root("local-path-binding");
    let reference_path = root.join("reference.png");
    let reference_data = loom_image_io::rgba8_to_png_data_url(1, 1, &[10, 20, 30, 255])
        .expect("encode reference fixture");
    fs::write(
        &reference_path,
        loom_image_io::decode_data_url_bytes(&reference_data).expect("decode reference fixture"),
    )
    .expect("write reference fixture");

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
    let result = execute_workflow_node(
        "local-path-flow",
        &workflow_tool("local-path-flow"),
        &node,
        Some(&bindings),
        &root_input,
        &json!({
            "input_base64": TEST_IMAGE,
            "input_2": reference_path.to_string_lossy()
        }),
        &HashMap::new(),
        &[],
        &workflow_store,
        &tool_registry,
        None,
        None,
        &mut ExecutionContext::default(),
    )
    .expect("resolve local path binding");

    let output = result["content"][0]["data"]
        .as_str()
        .expect("sticker image output");
    let decoded =
        loom_image_io::decode_image_base64_to_rgba8(output).expect("decode sticker output");
    assert_eq!(decoded.data, vec![10, 20, 30, 255]);
    fs::remove_dir_all(root).expect("cleanup local path binding root");
}

#[test]
fn workflow_runtime_rejects_oversized_local_image_files_before_reading() {
    let root = temp_root("oversized-local-image");
    let image_path = root.join("oversized.png");
    let file = fs::File::create(&image_path).expect("create sparse oversized image");
    file.set_len(MAX_IMAGE_FILE_BYTES + 1)
        .expect("size sparse oversized image");

    assert_eq!(
        normalize_image_reference(&image_path.to_string_lossy()),
        None
    );
    fs::remove_dir_all(root).expect("cleanup oversized image root");
}
