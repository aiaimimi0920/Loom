// Loom daemon tests fragment 4; included into the shared crate test module.
#[test]
fn hook_art_lists_restore_workflow_param_widget_schema_from_child_art() {
    let root = unique_temp_dir("workflow-param-widget-schema");
    let tool_registry = ToolRegistry::new(root.join("tools"));
    let workflow_store = WorkflowStore::new(root.join("workflows"));

    let mut child = ToolDefinition::new(
        "color-transfer",
        "颜色迁移",
        "颜色迁移子节点",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    child.params = vec![json!({
        "id": "strength",
        "label": "迁移强度",
        "widget": "slider",
        "data_type": "number",
        "default": 100,
        "min": 0,
        "max": 100,
        "step": 1,
        "group": "基础"
    })];
    child.outputs = vec![json!({ "name": "output", "type": "image" })];
    child.metadata = Some(json!({
        "packageSecurity": {
            "publisher": { "id": "neuro.official", "name": "Neuro Official" }
        },
        "capabilities": { "preview": "shader" }
    }));
    tool_registry.save_tool(child).expect("save child Art");

    workflow_store
        .save_workflow(
            "transfer-composite",
            r#"name: Transfer Composite
nodes:
  - id: transfer
    uses: neuro.official/color-transfer
    with:
      strength: 100
"#,
        )
        .expect("save workflow");

    let mut composite = ToolDefinition::new(
        "transfer-composite-art",
        "颜色迁移复合 Art",
        "旧版复合 Art 参数元数据不完整",
        ToolExecution::Workflow {
            workflow_id: "transfer-composite".to_owned(),
            workflow_bindings: Some(WorkflowExecutionBindings {
                inputs: vec![loom_tool_registry::WorkflowInputBinding {
                    workflow_param: "strength".to_owned(),
                    node_id: "transfer".to_owned(),
                    target: "strength".to_owned(),
                    kind: "param".to_owned(),
                }],
                primary_output: None,
                preview_output: Some(loom_tool_registry::WorkflowOutputBinding {
                    node_id: "transfer".to_owned(),
                    output: "output".to_owned(),
                    kind: "node_result".to_owned(),
                }),
                preview_required_nodes: vec!["transfer".to_owned()],
                ..WorkflowExecutionBindings::default()
            }),
        },
    );
    composite.params = vec![json!({
        "id": "strength",
        "name": "strength",
        "label": "迁移强度",
        "widget": "number",
        "type": "number",
        "default": 35
    })];
    composite.inputs = vec![
        json!({
            "name": "input",
            "label": "源图",
            "type": "image",
            "executionType": "image_buffer"
        }),
        json!({
            "name": "input_2",
            "label": "参考图",
            "type": "image",
            "executionType": "image_buffer"
        }),
    ];
    composite.metadata = Some(json!({ "autoProcess": true }));
    tool_registry
        .save_tool(composite)
        .expect("save composite Art");

    let tools = tool_registry.list_tools().expect("list Hook Arts");
    let composite = tools
        .iter()
        .find(|tool| tool.id == "transfer-composite-art")
        .expect("composite Art");
    let capability = hook_protocol_art_capability(composite, &tools, &workflow_store);
    let capability = serde_json::to_value(capability).expect("serialize capability");
    let param = &capability["parameters"][0];
    assert_eq!(param["id"], "strength");
    assert_eq!(param["label"], "迁移强度");
    assert_eq!(param["default"], 35);
    assert_eq!(param["widget"], "slider");
    assert_eq!(param["data_type"], "number");
    assert_eq!(param["min"], 0);
    assert_eq!(param["max"], 100);
    assert_eq!(param["step"], 1);
    assert_eq!(param["group"], "基础");
    assert_eq!(capability["metadata"]["capabilities"]["preview"], "shader");
    assert_eq!(
        capability["metadata"]["capabilities"]["requiresFormalExecution"],
        true
    );
    assert_eq!(
        capability["metadata"]["capabilities"]["shaderInput"],
        "input"
    );
    assert_eq!(
        capability["metadata"]["capabilities"]["shaderReferenceInput"],
        "input_2"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn local_publisher_identity_is_created_and_reset_without_a_store() {
    let root = unique_temp_dir("default-publisher-identity");
    let (identity, key) = ensure_local_publisher_identity(&root).expect("create identity");
    assert_eq!(identity.user_id, DEFAULT_TEST_PUBLISHER_ID);
    assert_eq!(identity.current_key_id, key.key_id);
    assert_eq!(identity.public_key, key.public_key);

    let (reset_identity, reset_key) =
        reset_local_publisher_identity(&root, &identity).expect("reset identity");
    assert_eq!(reset_identity.user_id, DEFAULT_TEST_PUBLISHER_ID);
    assert_ne!(reset_identity.current_key_id, identity.current_key_id);
    assert_ne!(reset_identity.public_key, identity.public_key);
    assert_eq!(reset_identity.current_key_id, reset_key.key_id);

    let (reloaded_identity, reloaded_key) =
        ensure_local_publisher_identity(&root).expect("reload identity");
    assert_eq!(
        reloaded_identity.current_key_id,
        reset_identity.current_key_id
    );
    assert_eq!(reloaded_key.private_key, reset_key.private_key);
    fs::remove_dir_all(root).ok();
}

#[test]
fn remote_art_store_catalog_preserves_platform_global_ids() {
    let catalog: RemoteArtStoreCatalog = serde_json::from_str(
            r#"{"arts":[{"id":"sample-art","qualifiedId":"neuro.official/sample-art","globalId":"NA40000000000","official":true}]}"#,
        )
        .expect("catalog should deserialize");
    assert_eq!(catalog.arts[0].global_id.as_deref(), Some("NA40000000000"));
    assert!(catalog.arts[0].official);

    let serialized = serde_json::to_value(catalog).expect("catalog should serialize");
    assert_eq!(serialized["arts"][0]["globalId"], "NA40000000000");
    assert_eq!(serialized["arts"][0]["official"], true);
}

#[test]
fn publish_route_rejects_arts_that_are_not_locally_authored() {
    let root = unique_temp_dir("publish-ownership");
    let tools = ToolRegistry::new(root.join("tools"));
    let execution = || ToolExecution::CloudApi {
        endpoint: "https://example.com".to_owned(),
        method: "GET".to_owned(),
        content_type: None,
        headers: None,
        body: None,
    };

    let mut third_party = ToolDefinition::new(
        "third-party-art",
        "Third Party",
        "Published elsewhere",
        execution(),
    );
    third_party.metadata = Some(json!({
        "packageSecurity": {
            "version": "1.0.0",
            "publisher": { "id": "publisher.example", "name": "Publisher" }
        }
    }));
    tools.save_tool(third_party).unwrap();

    let (status, body) =
        publish_art_to_store(r#"{"artId":"third-party-art"}"#, &tools, &root).unwrap();
    assert_eq!(status, 403);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["code"],
        "art_publish_not_owned"
    );

    let unowned = ToolDefinition::new(
        "unowned-art",
        "Unowned",
        "No local authoring metadata",
        execution(),
    );
    tools.save_tool(unowned).unwrap();
    let (status, body) = publish_art_to_store(r#"{"artId":"unowned-art"}"#, &tools, &root).unwrap();
    assert_eq!(status, 403);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["code"],
        "art_publish_not_owned"
    );

    let mut local = ToolDefinition::new("local-art", "Local", "Locally authored", execution());
    local.metadata = Some(json!({
        "packageSecurity": { "version": "0.1.0" },
        "authoring": { "origin": "local", "owner": "local-user" }
    }));
    tools.save_tool(local).unwrap();
    let (status, body) = publish_art_to_store(r#"{"artId":"local-art"}"#, &tools, &root).unwrap();
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["code"],
        "art_store_not_configured"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn platform_global_art_ids_are_applied_as_daemon_managed_metadata() {
    let mut tool = ToolDefinition::new(
        "local-art",
        "Local Art",
        "Locally authored",
        ToolExecution::CloudApi {
            endpoint: "https://example.com".to_owned(),
            method: "GET".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    apply_platform_global_art_id(&mut tool, "user-supplied");
    assert!(tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("art"))
        .is_none());

    apply_platform_global_art_id(&mut tool, "NA40000000000");
    assert_eq!(
        tool.metadata.as_ref().and_then(|metadata| metadata
            .get("art")
            .and_then(|art| art.get("globalId"))
            .and_then(Value::as_str)),
        Some("NA40000000000")
    );
}

fn framework_package_zip(id: &str, version: &str) -> Vec<u8> {
    let command = match id {
        "process" => "runtime/loom-framework-process.exe",
        "cloud_api" => "runtime/loom-framework-cloud-api.exe",
        "mcp" => "runtime/loom-framework-mcp.exe",
        "workflow" => "runtime/loom-framework-workflow.exe",
        other => panic!("unsupported test framework: {other}"),
    };
    let manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} daemon test framework"),
        "description": "daemon framework package test",
        "version": version,
        "publisher": { "id": "publisher.test", "name": "Publisher Test" },
        "protocolVersion": "loom.framework.v1",
        "platforms": ["windows-x64"],
        "entry": {
            "kind": "process",
            "command": command,
            "args": ["--stdio"],
            "processModel": "per_execution"
        },
        "permissions": ["process.spawn"],
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer
            .start_file("framework.manifest.json", options)
            .expect("manifest entry");
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .expect("manifest bytes");
        writer.start_file(command, options).expect("runtime entry");
        writer
            .write_all(b"MZ-test-framework")
            .expect("runtime bytes");
        writer.finish().expect("finish package");
    }
    bytes
}

/// The same package as `framework_package_zip`, signed with `key`.
///
/// A framework package is checked against the trust policy on every readiness probe, not only
/// when it is installed, so a test that persists a strict policy needs a framework whose
/// signature satisfies it — otherwise the framework install itself is refused and no Art can
/// run.
fn signed_framework_package_zip(
    id: &str,
    version: &str,
    key: &loom_plugin_security::SigningKeyDocument,
) -> Vec<u8> {
    let command = match id {
        "process" => "runtime/loom-framework-process.exe",
        "cloud_api" => "runtime/loom-framework-cloud-api.exe",
        "mcp" => "runtime/loom-framework-mcp.exe",
        "workflow" => "runtime/loom-framework-workflow.exe",
        other => panic!("unsupported test framework: {other}"),
    };
    let package = unique_temp_dir("signed-framework-package");
    fs::create_dir_all(package.join("runtime")).expect("package directory");
    let manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} daemon test framework"),
        "description": "daemon framework package test",
        "version": version,
        "publisher": {
            "id": "publisher.test",
            "name": "Publisher Test",
            "keyId": key.key_id.clone()
        },
        "signature": {
            "algorithm": "ed25519",
            "keyId": key.key_id.clone(),
            "file": "signature.json"
        },
        "protocolVersion": "loom.framework.v1",
        "platforms": ["windows-x64"],
        "entry": {
            "kind": "process",
            "command": command,
            "args": ["--stdio"],
            "processModel": "per_execution"
        },
        "permissions": ["process.spawn"],
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    });
    fs::write(
        package.join("framework.manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest bytes"),
    )
    .expect("write manifest");
    fs::write(package.join(command), b"MZ-test-framework").expect("write runtime entry");
    loom_plugin_security::sign_package(&package, "signature.json", key)
        .expect("sign framework package");

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for relative in ["framework.manifest.json", command, "signature.json"] {
            writer.start_file(relative, options).expect("package entry");
            writer
                .write_all(&fs::read(package.join(relative)).expect("package file"))
                .expect("package bytes");
        }
        writer.finish().expect("finish package");
    }
    fs::remove_dir_all(&package).ok();
    bytes
}

fn mcp_server_package_zip() -> Vec<u8> {
    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "id": "fixture-search",
        "name": "Fixture Search",
        "version": "1.2.3",
        "publisher": { "id": "publisher.test", "name": "Publisher Test" },
        "transport": "stdio",
        "entry": { "command": "runtime/server.ps1", "args": [] },
        "tools": ["search"],
        "credentials": [{
            "id": "api_key",
            "label": "API Key",
            "required": true,
            "target": { "kind": "env", "name": "API_KEY" }
        }]
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer
            .start_file("mcp.server.json", options)
            .expect("manifest entry");
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .expect("manifest bytes");
        writer
            .start_file("runtime/server.ps1", options)
            .expect("runtime entry");
        writer
            .write_all(b"Write-Output fixture")
            .expect("runtime bytes");
        writer.finish().expect("finish package");
    }
    bytes
}
