// Loom daemon tests fragment 13; included into the shared crate test module.
#[test]
fn daemon_manages_independent_mcp_server_package_lifecycle() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("control-plane-independent-mcp");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let package = mcp_server_package_zip();
    let package_base64 = BASE64.encode(&package);
    let installed = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/mcp/servers/install",
                &[],
                Some(&json!({ "zipBase64": &package_base64 }).to_string()),
            ),
        ),
        200,
    );
    assert_eq!(installed["server"]["id"], "fixture-search");
    assert_eq!(installed["server"]["source"], "package");
    assert_eq!(
        installed["server"]["package"]["qualifiedId"],
        "publisher.test/fixture-search"
    );
    assert_eq!(installed["server"]["credentialRequired"], true);
    assert_eq!(installed["server"]["credentialBound"], false);
    assert!(installed["server"].get("credentialBindings").is_none());

    let mut art = ToolDefinition::new(
        "fixture-art",
        "Fixture Art",
        "Uses an independently installed MCP server.",
        ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    art.metadata = Some(json!({
        "packageSecurity": {
            "version": "1.0.0",
            "publisher": { "id": "publisher.test", "name": "Publisher Test" }
        },
        "dependencies": {
            "framework": "mcp",
            "mcpServers": [{ "id": "publisher.test/fixture-search", "version": "^1.2" }]
        },
        "mcp": {
            "serverId": "fixture-search",
            "packageId": "publisher.test/fixture-search",
            "version": "^1.2",
            "toolName": "search"
        }
    }));
    runtime
        .tool_registry
        .save_tool(art)
        .expect("save MCP consumer Art");

    let configured = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/fixture-search/credentials",
                &[],
                Some(r#"{"values":{"api_key":"fixture-secret"}}"#),
            ),
        ),
        200,
    );
    assert_eq!(configured["server"]["credentialBound"], true);
    assert!(!fs::read_to_string(mcp_server_store_path(&root))
        .expect("MCP server store")
        .contains("fixture-secret"));

    let reinstalled = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/mcp/servers/install",
                &[],
                Some(&json!({ "zipBase64": &package_base64 }).to_string()),
            ),
        ),
        200,
    );
    assert_eq!(reinstalled["server"]["credentialBound"], true);
    assert!(reinstalled["server"].get("credentialBindings").is_none());
    let persisted_servers: Value = serde_json::from_str(
        &fs::read_to_string(mcp_server_store_path(&root)).expect("MCP server store"),
    )
    .expect("parse MCP server store");
    assert!(persisted_servers[0]["credentialBindings"]["api_key"]
        .as_str()
        .is_some_and(|binding| !binding.is_empty()));

    let listed = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/mcp/servers", &[], None),
        ),
        200,
    );
    assert_eq!(listed["servers"][0]["usageCount"], 1);
    assert_eq!(
        listed["servers"][0]["usedByArtIds"][0],
        "publisher.test/fixture-art"
    );
    assert_eq!(listed["servers"][0]["credentialBound"], true);

    let disabled = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/fixture-search/enabled",
                &[],
                Some(r#"{"enabled":false}"#),
            ),
        ),
        200,
    );
    assert_eq!(disabled["server"]["enabled"], false);

    let deleted = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("DELETE", "/v1/mcp/servers/fixture-search", &[], None),
        ),
        200,
    );
    assert_eq!(deleted["deleted"], true);
    assert!(!root
        .join("mcp/packages/publisher.test/fixture-search")
        .exists());

    drop(runtime);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_package_base64_decode_enforces_the_decoded_size_limit() {
    const TEST_LIMIT: usize = 4;
    let below_limit = BASE64.encode([1_u8, 2, 3]);
    let at_limit = BASE64.encode([1_u8, 2, 3, 4]);
    let above_limit = BASE64.encode([1_u8, 2, 3, 4, 5]);

    assert_eq!(
        decode_mcp_server_package_base64(&below_limit, TEST_LIMIT).expect("below limit"),
        vec![1, 2, 3]
    );
    assert_eq!(
        decode_mcp_server_package_base64(&at_limit, TEST_LIMIT).expect("at limit"),
        vec![1, 2, 3, 4]
    );
    assert!(matches!(
        decode_mcp_server_package_base64(&above_limit, TEST_LIMIT),
        Err(McpServerPackageBase64Error::TooLarge)
    ));
    assert!(matches!(
        decode_mcp_server_package_base64("@@@@", TEST_LIMIT),
        Err(McpServerPackageBase64Error::Invalid(_))
    ));
    assert!(matches!(
        decode_mcp_server_package_base64("AAAAAAAAA", TEST_LIMIT),
        Err(McpServerPackageBase64Error::TooLarge)
    ));
}

#[test]
fn daemon_persists_mcp_servers_across_runtime_reloads() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("control-plane-mcp-persist");
    let fixture = current_test_binary_mcp_fixture_config();

    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let saved = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/fixture",
                &[],
                Some(&fixture.to_string()),
            ),
        ),
        200,
    );
    assert_eq!(saved["server"]["id"], "fixture");
    let saved_remote = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/remote-fixture",
                &[],
                Some(
                    r#"{
                          "id": "remote-fixture",
                          "name": "Remote Fixture",
                          "description": "Persist every Streamable HTTP field",
                          "transport": "streamable-http",
                          "command": "",
                          "args": [],
                          "env": {},
                          "url": "https://example.test/mcp",
                          "headers": { "Authorization": "Bearer persisted-test-token" },
                          "enabled": false
                        }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved_remote["server"]["transport"], "streamable-http");
    assert_eq!(saved_remote["server"]["url"], "https://example.test/mcp");
    drop(runtime);

    let reloaded_runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let reloaded = expect_json_text_route_response(
        route_request(
            &reloaded_runtime,
            &parsed_request("GET", "/v1/mcp/servers", &[], None),
        ),
        200,
    );
    assert_eq!(
        reloaded["servers"].as_array().expect("servers").len(),
        2,
        "persisted local and remote MCP servers should reload from disk"
    );
    let reloaded_servers = reloaded["servers"].as_array().expect("servers");
    assert!(reloaded_servers
        .iter()
        .any(|server| server["id"] == "fixture"));
    let reloaded_remote = reloaded_servers
        .iter()
        .find(|server| server["id"] == "remote-fixture")
        .expect("remote server");
    assert_eq!(reloaded_remote["transport"], "streamable-http");
    assert_eq!(reloaded_remote["url"], "https://example.test/mcp");
    assert_eq!(
        reloaded_remote["headers"]["Authorization"],
        "Bearer persisted-test-token"
    );
    assert_eq!(reloaded_remote["enabled"], false);

    let deleted = expect_json_text_route_response(
        route_request(
            &reloaded_runtime,
            &parsed_request("DELETE", "/v1/mcp/servers/fixture", &[], None),
        ),
        200,
    );
    assert_eq!(deleted["deleted"], true);
    let deleted_remote = expect_json_text_route_response(
        route_request(
            &reloaded_runtime,
            &parsed_request("DELETE", "/v1/mcp/servers/remote-fixture", &[], None),
        ),
        200,
    );
    assert_eq!(deleted_remote["deleted"], true);
    drop(reloaded_runtime);

    let deleted_runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let empty = expect_json_text_route_response(
        route_request(
            &deleted_runtime,
            &parsed_request("GET", "/v1/mcp/servers", &[], None),
        ),
        200,
    );
    assert_eq!(empty["servers"], serde_json::json!([]));

    drop(deleted_runtime);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn daemon_exposes_mcp_registry_and_connection_test_contracts() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let registry_fixture = McpRegistryFixture::start();
    let previous_registry_endpoint = std::env::var("LOOM_MCP_REGISTRY_ENDPOINT").ok();
    std::env::set_var(
        "LOOM_MCP_REGISTRY_ENDPOINT",
        registry_fixture.url("/v0.1/servers"),
    );
    let root = unique_temp_dir("mcp-registry-contracts");
    let config = DaemonConfig::localhost(0);
    restore_env("LOOM_MCP_REGISTRY_ENDPOINT", previous_registry_endpoint);
    let runtime = test_daemon_runtime_from_config(&root, config);

    let registry = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "GET",
                "/v1/mcp/registry?search=fixture&limit=250&cursor=cursor-1",
                &[],
                None,
            ),
        ),
        200,
    );
    assert_eq!(
        registry["servers"][0]["server"]["name"],
        "io.modelcontextprotocol/fixture"
    );
    assert!(registry_fixture.request_path().contains("limit=100"));
    assert!(registry_fixture.request_path().contains("search=fixture"));
    assert!(registry_fixture.request_path().contains("cursor=cursor-1"));
    assert!(registry_fixture.request_path().contains("version=latest"));

    let request_body = current_test_binary_mcp_fixture_config().to_string();
    let test_result = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("POST", "/v1/mcp/test", &[], Some(&request_body)),
        ),
        200,
    );
    assert_eq!(test_result["success"], true);
    assert_eq!(test_result["tools"][0]["name"], "echo");
    assert_eq!(
        test_result["server_info"]["serverInfo"]["name"],
        "daemon-fixture"
    );

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup mcp registry root");
}

#[test]
fn mcp_registry_uses_stale_disk_cache_when_the_official_registry_is_unavailable() {
    let fixture = McpRegistryFixture::start();
    let endpoint = fixture.url("/v0.1/servers");
    let root = unique_temp_dir("mcp-registry-cache");
    let cache_path = mcp_registry_cache_path(&root);
    let path = "/v1/mcp/registry?limit=20&refresh=true";

    let (status, body) =
        fetch_mcp_registry(path, &endpoint, &cache_path).expect("initial registry fetch");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).expect("initial registry response");
    assert_eq!(body["loomRegistry"]["source"], "network");
    assert_eq!(body["loomRegistry"]["stale"], false);
    assert!(cache_path.is_file());
    drop(fixture);

    let (status, body) =
        fetch_mcp_registry(path, &endpoint, &cache_path).expect("cached registry fallback");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).expect("cached registry response");
    assert_eq!(body["loomRegistry"]["source"], "cache");
    assert_eq!(body["loomRegistry"]["stale"], true);
    assert_eq!(
        body["servers"][0]["server"]["name"],
        "io.modelcontextprotocol/fixture"
    );

    fs::remove_dir_all(root).expect("cleanup MCP registry cache");
}

#[test]
fn mcp_registry_retries_the_same_cursor_after_a_transient_connection_failure() {
    let fixture = McpRegistryFixture::start_flaky();
    let endpoint = fixture.url("/v0.1/servers");
    let root = unique_temp_dir("mcp-registry-retry");
    let cache_path = mcp_registry_cache_path(&root);

    let (status, body) = fetch_mcp_registry(
        "/v1/mcp/registry?limit=100&cursor=retry-cursor",
        &endpoint,
        &cache_path,
    )
    .expect("retried registry fetch");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).expect("retried registry response");
    assert_eq!(body["loomRegistry"]["source"], "network");
    assert_eq!(
        body["servers"][0]["server"]["name"],
        "io.modelcontextprotocol/fixture"
    );
    assert!(fixture.request_path().contains("cursor=retry-cursor"));

    fs::remove_dir_all(root).expect("cleanup MCP registry retry cache");
}

#[test]
fn daemon_exposes_safe_mcp_package_contracts() {
    let install_plan = build_mcp_package_install_plan(r#"{"packageName":"mcp-server-demo"}"#)
        .expect("install plan");
    assert_eq!(install_plan.0, 200);
    let install_plan = serde_json::from_str::<Value>(&install_plan.1).expect("install plan json");
    assert_eq!(install_plan["package"], "mcp-server-demo");
    assert_eq!(install_plan["sideEffect"], false);
    assert_eq!(install_plan["command"][1], "-m");
    assert_eq!(install_plan["command"][2], "pip");

    let rejected = build_mcp_package_install_plan(r#"{"packageName":"demo;rm"}"#)
        .expect("invalid plan response");
    assert_eq!(rejected.0, 400);

    let check = check_mcp_package_installed(r#"{"moduleName":"json"}"#).expect("check module");
    assert_eq!(check.0, 200);
    let check = serde_json::from_str::<Value>(&check.1).expect("check json");
    assert_eq!(check["module"], "json");
}
