// Capability Plugin lifecycle API coverage.
#[test]
fn capability_plugin_api_installs_configures_enables_and_uninstalls() {
    let root = unique_temp_dir("capability-api");
    fs::create_dir_all(&root).expect("control root");
    let key = loom_plugin_security::generate_signing_key("release-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("trust store");
    let archive = capability_api_fixture(&root, &key);
    let daemon_runtime = test_daemon_runtime(&root, None);
    let runtime = Arc::clone(&daemon_runtime.capability_runtime);
    let resources = Arc::clone(&daemon_runtime.capability_resources);
    let body = json!({
        "zipBase64": format!(
            "data:application/zip;base64,{}",
            BASE64.encode(&archive)
        )
    })
    .to_string();

    let (status, installed) =
        install_capability_plugin(&body, &root).expect("install response");
    assert_eq!(status, 200, "{installed}");
    let installed: Value = serde_json::from_str(&installed).expect("install JSON");
    let digest = installed["package"]["digest"]
        .as_str()
        .expect("package digest");
    assert_eq!(
        installed["package"]["qualifiedId"],
        "publisher.example/api-fixture"
    );
    loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .verify_installed_version("publisher.example/api-fixture", digest)
        .expect("installed package remains verifiable");
    let (status, approval) = approve_capability_plugin(
        "publisher.example/api-fixture",
        &json!({
            "digest": digest,
            "permissions": ["hook.unit.attachments.write", "hook.notice.show"]
        })
        .to_string(),
        &root,
    )
    .expect("approve response");
    assert_eq!(status, 200, "{approval}");

    let (extension_events, _extension_subscription) = enable_api_fixture_through_extension_route(
        &daemon_runtime,
        &root,
        &runtime,
        &resources,
        digest,
    );
    let (status, snapshot) = capability_extension_snapshot(&runtime).expect("extension snapshot");
    assert_eq!(status, 200);
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["snapshot"]["generation"], 1);
    assert_eq!(
        snapshot["snapshot"]["plugins"][0]["id"],
        "publisher.example/api-fixture"
    );
    assert_eq!(
        snapshot["snapshot"]["contributions"]["commands"][0]["id"],
        "publisher.example/api-fixture.run"
    );
    assert_eq!(
        snapshot["snapshot"]["contributions"]["shortcuts"][0]["commandId"],
        "publisher.example/api-fixture.run"
    );
    assert_eq!(
        snapshot["snapshot"]["contributions"]["menus"][0]["placement"],
        "hook.unit.toolbar"
    );
    assert_eq!(
        snapshot["snapshot"]["contributions"]["settings"][0]["payload"]["payload"]["type"],
        "enum"
    );

    assert_api_fixture_extension_invocation(
        &runtime,
        &resources,
        &daemon_runtime.surface_resources,
    );

    let invoked = expect_json_text_route_response(
        route_request(
            &daemon_runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[],
                Some(
                    &json!({
                        "requestId": "api-fixture-success",
                        "caller": "hook",
                        "capability": "publisher.example/api-fixture.run",
                        "input": { "text": "hello" }
                    })
                    .to_string(),
                ),
            ),
        ),
        200,
    );
    assert_eq!(invoked["status"], "succeeded");
    assert_eq!(invoked["pluginId"], "publisher.example/api-fixture");
    assert_eq!(invoked["output"]["output"]["ok"], true);
    assert!(invoked["output"]["effects"].is_array());

    let timed_out = expect_json_text_route_response(
        route_request(
            &daemon_runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[],
                Some(
                    &json!({
                        "requestId": "api-fixture-timeout",
                        "caller": "hook",
                        "capability": "publisher.example/api-fixture.run",
                        "input": { "hang": true },
                        "timeoutMs": 100
                    })
                    .to_string(),
                ),
            ),
        ),
        504,
    );
    assert_eq!(timed_out["error"]["code"], "capability_timeout");
    assert!(runtime.process_ids().is_empty());
    assert_eq!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .get("publisher.example/api-fixture")
            .unwrap()
            .unwrap()
            .runtime_failures
            .count,
        1
    );

    let crashed = expect_json_text_route_response(
        route_request(
            &daemon_runtime,
            &parsed_request(
                "POST",
                "/v1/invoke",
                &[],
                Some(
                    &json!({
                        "requestId": "api-fixture-crash",
                        "caller": "hook",
                        "capability": "publisher.example/api-fixture.run",
                        "input": { "crash": true }
                    })
                    .to_string(),
                ),
            ),
        ),
        503,
    );
    assert_eq!(
        crashed["error"]["code"],
        "capability_runtime_unavailable"
    );
    assert!(runtime.process_ids().is_empty());
    assert_eq!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .get("publisher.example/api-fixture")
            .unwrap()
            .unwrap()
            .runtime_failures
            .count,
        2
    );

    let (status, settings) = get_capability_settings(
        "publisher.example/api-fixture",
        &root,
    )
    .expect("settings response");
    assert_eq!(status, 200);
    let settings: Value = serde_json::from_str(&settings).expect("settings JSON");
    assert_eq!(settings["settings"]["revision"], 0);
    assert_eq!(settings["fields"][0]["payload"]["default"], "balanced");
    let (status, stale) = update_capability_settings(
        "publisher.example/api-fixture",
        &json!({
            "expectedRevision": 0,
            "packageDigest": "0".repeat(64),
            "values": { "publisher.example/api-fixture.density": "compact" }
        })
        .to_string(),
        &root,
    )
    .expect("stale settings response");
    assert_eq!(status, 409, "{stale}");

    let (status, settings) = update_capability_settings(
        "publisher.example/api-fixture",
        &json!({
            "expectedRevision": 0,
            "packageDigest": digest,
            "values": { "publisher.example/api-fixture.density": "compact" }
        })
        .to_string(),
        &root,
    )
    .expect("settings update response");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&settings).unwrap()["settings"]["revision"],
        1
    );
    let (status, rejected) = update_capability_settings(
        "publisher.example/api-fixture",
        &json!({
            "expectedRevision": 1,
            "packageDigest": digest,
            "values": { "publisher.example/api-fixture.undeclared": true }
        })
        .to_string(),
        &root,
    )
    .expect("invalid settings response");
    assert_eq!(status, 400, "{rejected}");

    let (status, listed) = list_capability_plugins(&root).expect("list response");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap()["plugins"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for _ in 2..loom_tool_registry::capability::CAPABILITY_MAX_RUNTIME_FAILURES {
        record_capability_runtime_failure(
            &root,
            "publisher.example/api-fixture",
            &runtime,
            &resources,
            &loom_capability_runtime::CapabilityHostError::Timeout,
        );
    }
    let faulted = loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
        .get("publisher.example/api-fixture")
        .unwrap()
        .unwrap();
    assert_eq!(
        faulted.status,
        loom_tool_registry::capability::CapabilityLifecycleStatus::Faulted
    );
    assert!(runtime.contribution_snapshot().unwrap().plugins.is_empty());
    let (_, retried) = enable_capability_plugin(
        "publisher.example/api-fixture",
        &json!({ "digest": digest }).to_string(),
        &root,
        &runtime,
    )
    .expect("retry faulted plugin");
    assert_eq!(
        serde_json::from_str::<Value>(&retried).unwrap()["plugin"]["runtimeFailures"]["count"],
        0
    );
    disable_api_fixture_through_extension_route(
        &daemon_runtime,
        &root,
        &runtime,
        &resources,
        &extension_events,
    );
    let (status, _) = uninstall_capability_plugin(
        "publisher.example/api-fixture",
        &root,
        &runtime,
        &resources,
    )
    .expect("uninstall");
    assert_eq!(status, 200);
    assert!(runtime.process_ids().is_empty());
    assert!(
        loom_tool_registry::capability::CapabilityPluginRegistry::new(&root)
            .list()
            .expect("registry")
            .is_empty()
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn capability_plugin_api_rejects_unknown_fields_and_routes_only_its_namespace() {
    let root = unique_temp_dir("capability-api-invalid");
    let runtime = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    let resources = CapabilityResourceBroker::open(root.join("capability-resources"))
        .expect("Capability resource broker");
    let (status, body) = install_capability_plugin(
        r#"{"zipBase64":"bad","unexpected":true}"#,
        &root,
    )
    .expect("error response");
    assert_eq!(status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["code"],
        "invalid_capability_install"
    );
    let request = ParsedHttpRequest {
        method: "GET".to_owned(),
        path: "/v1/unrelated".to_owned(),
        headers: Vec::new(),
        body: String::new(),
    };
    assert!(
        route_capability_plugins(
            &request,
            "/v1/unrelated",
            &root,
            &runtime,
            &resources,
            &Arc::new(Mutex::new(HookBridgeRuntime::new(root.clone()))),
        )
        .is_none()
    );
}
