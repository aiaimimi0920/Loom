// Loom daemon tests fragment 14; included into the shared crate test module.
#[test]
fn daemon_exposes_python_art_source_import_helpers() {
    let root = unique_temp_dir("python-art-source");
    let art_dir = root.join("Art_SourceFixture");
    fs::create_dir_all(&art_dir).expect("create source art dir");
    let python_path = art_dir.join("main.py");
    let art_json_path = art_dir.join("art.json");
    fs::write(
        &python_path,
        r#"
def run(args):
    input_image = args.get("input_image")
    strength = args["strength"]
    return {"result_path": input_image, "confidence": strength}
"#,
    )
    .expect("write python source fixture");
    fs::write(
        &art_json_path,
        r#"{
  "art_id": "source_fixture",
  "label": "Source Fixture",
  "description": "Nearby art.json fixture",
  "signature": {
    "inputs": [{"id": "input_image", "label": "Input image", "type": "Image"}],
    "outputs": [{"id": "result_path", "label": "Result path", "type": "Image"}]
  },
  "variables": [{"id": "strength", "label": "Strength", "widget": "slider", "default": 0.75}]
}"#,
    )
    .expect("write art json fixture");

    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let source = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/art-authoring/source/read",
                &[],
                Some(&serde_json::json!({ "path": python_path }).to_string()),
            ),
        ),
        200,
    );
    assert_eq!(
        canonical_test_path(source["path"].as_str().expect("source path")),
        canonical_test_path(&python_path)
    );
    assert!(source["content"]
        .as_str()
        .expect("source content")
        .contains("args.get(\"input_image\")"));

    let nearby = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/art-authoring/source/check-art-json",
                &[],
                Some(&serde_json::json!({ "pythonPath": python_path }).to_string()),
            ),
        ),
        200,
    );
    assert_eq!(nearby["found"], true);
    assert_eq!(nearby["artJson"]["label"], "Source Fixture");
    assert_eq!(
        canonical_test_path(
            nearby["artJsonPath"]
                .as_str()
                .expect("nearby art json path")
        ),
        canonical_test_path(&art_json_path)
    );

    let art_json = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/art-authoring/source/read-art-json",
                &[],
                Some(&serde_json::json!({ "artPath": art_dir }).to_string()),
            ),
        ),
        200,
    );
    assert_eq!(art_json["artJson"]["art_id"], "source_fixture");

    let inferred = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/art-authoring/source/infer-ports",
                &[],
                Some(&serde_json::json!({ "path": python_path }).to_string()),
            ),
        ),
        200,
    );
    assert_eq!(inferred["inputs"][0]["name"], "input_image");
    assert_eq!(inferred["inputs"][0]["execution_type"], "image_path");
    assert_eq!(inferred["inputs"][1]["name"], "strength");
    assert_eq!(inferred["inputs"][1]["execution_type"], "number");
    assert_eq!(inferred["outputs"][0]["name"], "result_path");
    assert_eq!(inferred["outputs"][0]["execution_type"], "image_path");
    assert_eq!(inferred["outputs"][1]["name"], "confidence");
    assert_eq!(inferred["outputs"][1]["execution_type"], "string");

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup source fixture root");
}

#[test]
fn daemon_recovers_trailing_tool_registry_data_before_listing_tools() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("control-plane-trailing-tools");
    let tools_root = root.join("tools");
    fs::create_dir_all(&tools_root).expect("create tool registry root");
    fs::write(
        tools_root.join("tools.json"),
        r#"[
              {
                "id": "recovered-tool",
                "name": "Recovered Tool",
                "description": "Recover a valid tool array with trailing delimiters",
                "enabled": true,
                "execution": {
                  "type": "framework_art",
                  "framework": "process"
                }
              }
            ]  }
              }
            ]"#,
    )
    .expect("write corrupted tool registry");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let body = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
        200,
    );
    assert_eq!(body["tools"][0]["id"], "recovered-tool");

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup recovered registry root");
}

#[test]
fn daemon_reads_and_writes_tool_and_workflow_contracts() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("control-plane-tools-workflows");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let saved_tool = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/tools/paint-flow",
                &[],
                Some(
                    r#"{
              "id": "paint-flow",
              "name": "Paint Flow",
              "description": "Run a saved workflow",
              "enabled": true,
              "execution": { "type": "workflow", "workflowId": "wf-1" }
            }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved_tool["tool"]["id"], "paint-flow");
    assert_eq!(saved_tool["tool"]["execution"]["type"], "workflow");

    let tools = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
        200,
    );
    assert_eq!(tools["tools"].as_array().expect("tools").len(), 1);
    assert_eq!(tools["tools"][0]["id"], "paint-flow");

    let saved_workflow = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/workflows/wf-1",
                &[],
                Some(
                    r#"{"data":"name: Paint Flow\nnodes:\n  - id: prompt\n    uses: neuro.official/text-prompt\n"}"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved_workflow["workflow"]["id"], "wf-1");
    assert_eq!(saved_workflow["workflow"]["name"], "Paint Flow");
    assert_eq!(saved_workflow["workflow"]["nodeCount"], 1);

    let workflows = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/v1/workflows", &[], None)),
        200,
    );
    assert_eq!(
        workflows["workflows"].as_array().expect("workflows").len(),
        1
    );
    assert_eq!(workflows["workflows"][0]["id"], "wf-1");

    let loaded_workflow = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/workflows/wf-1", &[], None),
        ),
        200,
    );
    assert_eq!(loaded_workflow["workflow"]["id"], "wf-1");
    assert!(loaded_workflow["workflow"]["data"]
        .as_str()
        .expect("workflow data")
        .contains("name: Paint Flow"));

    let deleted_tool = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("DELETE", "/v1/tools/paint-flow", &[], None),
        ),
        200,
    );
    assert_eq!(deleted_tool["deleted"], true);
    let tools_after_delete = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/v1/tools", &[], None)),
        200,
    );
    assert_eq!(
        tools_after_delete["tools"].as_array().expect("tools").len(),
        0
    );

    let deleted_workflow = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("DELETE", "/v1/workflows/wf-1", &[], None),
        ),
        200,
    );
    assert_eq!(deleted_workflow["deleted"], true);
    let workflows_after_delete = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/v1/workflows", &[], None)),
        200,
    );
    assert_eq!(
        workflows_after_delete["workflows"]
            .as_array()
            .expect("workflows")
            .len(),
        0
    );

    drop(runtime);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn daemon_executes_mcp_backed_tool_contract() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("mcp-backed-tool");
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root))
        .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));
    let fixture = current_test_binary_mcp_fixture_config();

    let saved_server = http_json_put(
        address.port(),
        "/v1/mcp/servers/fixture",
        &fixture.to_string(),
    );
    assert_eq!(saved_server["server"]["id"], "fixture");

    let saved_tool = http_json_put(
        address.port(),
        "/v1/tools/fixture-echo",
        r#"{
              "id": "fixture-echo",
              "name": "Fixture Echo",
              "description": "Execute fixture MCP echo",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "echo"
              }
            }"#,
    );
    assert_eq!(saved_tool["tool"]["id"], "fixture-echo");
    assert_eq!(saved_tool["tool"]["execution"]["type"], "mcp");

    let executed = http_json_post(
        address.port(),
        "/v1/tools/fixture-echo/execute",
        r#"{"arguments":{"text":"hello daemon"}}"#,
    );
    assert_eq!(executed["toolId"], "fixture-echo");
    assert_eq!(executed["status"], "succeeded");
    assert_eq!(executed["result"]["content"][0]["type"], "text");
    assert_eq!(executed["result"]["content"][0]["text"], "hello daemon");

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
    fs::remove_dir_all(root).expect("cleanup mcp-backed tool root");
}

#[test]
fn daemon_executes_cloud_api_backed_tool_contract() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("cloud-tool");
    let fixture = CloudApiFixture::start(CloudApiFixtureMode::Text);
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root))
        .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let saved_tool = http_json_put(
        address.port(),
        "/v1/tools/fixture-cloud",
        &serde_json::json!({
            "id": "fixture-cloud",
            "name": "Fixture Cloud",
            "description": "Execute fixture cloud API",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/text"),
                "method": "POST"
            },
            // A cloud Art only reaches a loopback endpoint when it declares that it wants to,
            // so this fixture-backed tool declares it the way a local-service Art would.
            "metadata": {
                "permissionPolicy": { "network": { "allowLocalhost": true } }
            }
        })
        .to_string(),
    );
    assert_eq!(saved_tool["tool"]["id"], "fixture-cloud");
    assert_eq!(saved_tool["tool"]["execution"]["type"], "cloud_api");

    let executed = http_json_post(
        address.port(),
        "/v1/tools/fixture-cloud/execute",
        r#"{"arguments":{"prompt":"hello daemon cloud"}}"#,
    );
    assert_eq!(executed["toolId"], "fixture-cloud");
    assert_eq!(executed["status"], "succeeded");
    assert_eq!(executed["result"]["content"][0]["type"], "text");
    assert_eq!(
        executed["result"]["content"][0]["text"],
        "cloud saw hello daemon cloud"
    );

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
    fs::remove_dir_all(root).expect("cleanup cloud tool root");
}

#[test]
fn daemon_reports_hook_bridge_status_contract() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let appdata_root = unique_temp_dir("empty-hook-appdata");
    let control_plane_root = unique_temp_dir("hook-bridge-status-control-plane");
    let previous_appdata = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata_root);

    let runtime = test_daemon_runtime_from_config(&control_plane_root, DaemonConfig::localhost(0));

    let status = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/hook-bridge/status", &[], None),
        ),
        200,
    );
    assert_eq!(status["running"], false);
    assert_eq!(status["port"], 19820);
    assert_eq!(status["connectedClients"], 0);
    assert_eq!(status["protocol"], loom_protocol::HOOK_PROTOCOL_VERSION);
    assert!(status["methods"]
        .as_array()
        .expect("methods")
        .contains(&serde_json::Value::String(
            loom_protocol::HOOK_METHOD_WORKFLOW_NODE_UPDATE.to_owned()
        )));
    assert!(status["methods"]
        .as_array()
        .expect("methods")
        .contains(&serde_json::Value::String(
            loom_protocol::HOOK_METHOD_WORKFLOW_INSTANTIATE.to_owned()
        )));
    assert!(status.get("sessionMethod").is_none());

    let session = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/hook-bridge/session", &[], None),
        ),
        200,
    );
    assert_eq!(
        session["protocolVersion"],
        loom_protocol::HOOK_PROTOCOL_VERSION
    );
    assert_eq!(
        session["session"]["stickers"]
            .as_array()
            .expect("stickers")
            .len(),
        0
    );
    assert_eq!(
        session["session"]["links"].as_array().expect("links").len(),
        0
    );

    restore_env("APPDATA", previous_appdata);
    fs::remove_dir_all(appdata_root).expect("cleanup appdata root");
    fs::remove_dir_all(control_plane_root).expect("cleanup control plane");
}
