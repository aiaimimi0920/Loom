// Loom daemon tests fragment 7; included into the shared crate test module.
#[test]
fn art_management_enforces_ownership_defaults_and_global_secret_references() {
    let root = unique_temp_dir("art-management-settings");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    CredentialStore::new(&root)
        .upsert(CredentialInput {
            name: "cloudflare_key".to_owned(),
            value: "never-persist-this-plaintext".to_owned(),
            value_type: CredentialValueType::String,
            scope: CredentialScope::default(),
            expires_at: None,
        })
        .expect("save global credential");
    for (name, value, value_type) in [
        ("default_query", "ready", CredentialValueType::String),
        ("image_quality", "80", CredentialValueType::Integer),
        ("feature_enabled", "true", CredentialValueType::Boolean),
        (
            "request_payload",
            "{\"safe\":true}",
            CredentialValueType::Json,
        ),
    ] {
        CredentialStore::new(&root)
            .upsert(CredentialInput {
                name: name.to_owned(),
                value: value.to_owned(),
                value_type,
                scope: CredentialScope::default(),
                expires_at: None,
            })
            .expect("save typed global value");
    }

    let mut local = ToolDefinition::new(
        "local-art",
        "Local Art",
        "local description",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    local.params = vec![
        json!({ "id": "required_text", "label": "Required", "type": "string", "required": true }),
        json!({ "id": "quality", "label": "Quality", "data_type": "number", "default": 90 }),
        json!({ "id": "enabled", "label": "Enabled", "type": "boolean" }),
        json!({ "id": "payload", "label": "Payload", "type": "json" }),
        json!({ "id": "api_token", "label": "API Token", "type": "secret", "required": true, "default": "manifest-secret" }),
    ];
    let art_dir = root
        .join("arts")
        .join("local-art")
        .join("versions")
        .join("0.1.0");
    fs::create_dir_all(&art_dir).unwrap();
    local.metadata = Some(json!({
        "authoring": { "origin": "local", "owner": "local-user" },
        "packageSecurity": { "version": "0.1.0" },
        "artPackage": { "dir": art_dir, "version": "0.1.0" }
    }));
    runtime.tool_registry.save_tool(local).unwrap();

    let (status, initial) = get_art_management("local-art", &runtime.tool_registry, &root)
        .expect("read Art management");
    assert_eq!(status, 200);
    let initial: Value = serde_json::from_str(&initial).unwrap();
    assert_eq!(initial["canEditIdentity"], true);
    assert_eq!(initial["autoUpdate"], true);
    assert_eq!(initial["currentVersion"], "0.1.0");
    assert!(initial["availableCredentials"]
        .as_array()
        .unwrap()
        .iter()
        .any(|credential| credential["name"] == "image_quality"
            && credential["valueType"] == "integer"));
    let secret_parameter = initial["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["id"] == "api_token")
        .unwrap();
    assert!(secret_parameter.get("default").is_none());

    let (status, secret_default) = put_art_management_settings(
            "local-art",
            r#"{"autoUpdate":false,"defaults":{"api_token":"plaintext"},"credentialBindings":{"api_token":"cloudflare_key"}}"#,
            &runtime.tool_registry,
            &root,
            &runtime.hook_bridge,
        )
        .expect("reject secret default");
    assert_eq!(status, 400, "body={secret_default}");

    let (status, missing_required) = put_art_management_settings(
        "local-art",
        r#"{"autoUpdate":false,"defaults":{},"credentialBindings":{}}"#,
        &runtime.tool_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("reject missing required defaults");
    assert_eq!(status, 400, "body={missing_required}");

    let (status, wrong_type) = put_art_management_settings(
            "local-art",
            r#"{"autoUpdate":false,"defaults":{"required_text":"ready"},"valueBindings":{"quality":"cloudflare_key"},"credentialBindings":{"api_token":"cloudflare_key"}}"#,
            &runtime.tool_registry,
            &root,
            &runtime.hook_bridge,
        )
        .expect("reject wrong typed global value");
    assert_eq!(status, 400, "body={wrong_type}");

    let body = serde_json::to_string(&json!({
        "name": "Renamed Local Art",
        "description": "updated",
        "autoUpdate": false,
        "defaults": {},
        "valueBindings": {
            "required_text": "default_query",
            "quality": "image_quality",
            "enabled": "feature_enabled",
            "payload": "request_payload"
        },
        "credentialBindings": { "api_token": "cloudflare_key" }
    }))
    .unwrap();
    let (status, saved) = put_art_management_settings(
        "local-art",
        &body,
        &runtime.tool_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("save Art management");
    assert_eq!(status, 200, "body={saved}");
    let saved: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved["name"], "Renamed Local Art");
    assert_eq!(saved["autoUpdate"], false);
    assert!(saved["defaults"].as_object().unwrap().is_empty());
    assert_eq!(saved["valueBindings"]["quality"], "image_quality");
    assert_eq!(saved["credentialBindings"]["api_token"], "cloudflare_key");
    let settings_text = fs::read_to_string(root.join("art-user-settings.json")).unwrap();
    assert!(!settings_text.contains("never-persist-this-plaintext"));
    assert!(!settings_text.contains("{\"safe\":true}"));
    let prepared = loom_tool_registry::prepare_tool_arguments(
        &runtime
            .tool_registry
            .get_tool("local-art")
            .unwrap()
            .unwrap(),
        json!({ "params": {} }),
    )
    .unwrap();
    assert_eq!(prepared["params"]["required_text"], "ready");
    assert_eq!(prepared["params"]["quality"], 80);
    assert_eq!(prepared["params"]["enabled"], true);
    assert_eq!(prepared["params"]["payload"], json!({ "safe": true }));
    let explicit = loom_tool_registry::prepare_tool_arguments(
        &runtime
            .tool_registry
            .get_tool("local-art")
            .unwrap()
            .unwrap(),
        json!({ "params": { "quality": 70 } }),
    )
    .unwrap();
    assert_eq!(explicit["params"]["quality"], 70);

    let manual_body = serde_json::to_string(&json!({
        "autoUpdate": false,
        "defaults": { "required_text": "ready", "quality": 75 },
        "valueBindings": {},
        "secretValues": { "api_token": "art-only-plaintext" }
    }))
    .unwrap();
    let (status, manual_saved) = put_art_management_settings(
        "local-art",
        &manual_body,
        &runtime.tool_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("save Art-specific secret");
    assert_eq!(status, 200, "body={manual_saved}");
    let manual_saved: Value = serde_json::from_str(&manual_saved).unwrap();
    let manual_name = manual_saved["credentialBindings"]["api_token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(manual_name.starts_with("loom-art-secret-"));
    assert!(manual_saved["availableCredentials"]
        .as_array()
        .unwrap()
        .iter()
        .any(|credential| credential["name"] == manual_name
            && credential["scope"]["artId"] == "local-art"));
    let settings_text = fs::read_to_string(root.join("art-user-settings.json")).unwrap();
    let credentials_text = fs::read_to_string(root.join("plugin-credentials.json")).unwrap();
    assert!(!settings_text.contains("art-only-plaintext"));
    assert!(!credentials_text.contains("art-only-plaintext"));
    let grants = CredentialStore::new(&root)
        .grants_for_bindings(
            "process",
            "local-art",
            &BTreeMap::from([("api_token".to_owned(), manual_name.clone())]),
        )
        .unwrap();
    assert_eq!(grants[0].name, "api_token");
    assert_eq!(grants[0].value, "art-only-plaintext");

    let switch_to_global = serde_json::to_string(&json!({
        "autoUpdate": false,
        "defaults": { "required_text": "ready" },
        "valueBindings": {},
        "credentialBindings": { "api_token": "cloudflare_key" }
    }))
    .unwrap();
    let (status, switched) = put_art_management_settings(
        "local-art",
        &switch_to_global,
        &runtime.tool_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("switch Art secret back to global reference");
    assert_eq!(status, 200, "body={switched}");
    assert!(!CredentialStore::new(&root)
        .summaries()
        .unwrap()
        .iter()
        .any(|credential| credential.name == manual_name));

    let mut external = ToolDefinition::new(
        "external-art",
        "External Art",
        "publisher owned",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    external.metadata = Some(json!({
        "packageSecurity": {
            "version": "1.0.0",
            "publisher": { "id": "publisher.test", "name": "Publisher" }
        }
    }));
    runtime.tool_registry.save_tool(external).unwrap();
    let (status, denied) = put_art_management_settings(
        "publisher.test/external-art",
        r#"{"name":"Takeover","autoUpdate":true,"defaults":{},"credentialBindings":{}}"#,
        &runtime.tool_registry,
        &root,
        &runtime.hook_bridge,
    )
    .expect("deny publisher-owned identity edit");
    assert_eq!(status, 403, "body={denied}");

    drop(runtime);
    fs::remove_dir_all(root).ok();
}

#[test]
fn package_routes_decode_publisher_qualified_ids_without_accepting_raw_slashes() {
    assert_eq!(
        decoded_package_path_id_with_suffix(
            "/v1/frameworks/publisher.alpha%2Fshared-framework/rollback",
            "/v1/frameworks/",
            "/rollback",
        )
        .as_deref(),
        Some("publisher.alpha/shared-framework")
    );
    assert_eq!(
        decoded_package_path_id_with_suffix(
            "/v1/arts/publisher.alpha%2Fshared-art/uninstall",
            "/v1/arts/",
            "/uninstall",
        )
        .as_deref(),
        Some("publisher.alpha/shared-art")
    );
    assert_eq!(
        tool_execute_path_id("/v1/tools/publisher.alpha%2Fshared-art/execute").as_deref(),
        Some("publisher.alpha/shared-art")
    );
    assert!(decoded_package_path_id_with_suffix(
        "/v1/arts/publisher.alpha/shared-art/uninstall",
        "/v1/arts/",
        "/uninstall",
    )
    .is_none());
    assert!(decoded_package_path_id_with_suffix(
        "/v1/arts/%2e%2e%2Fescape/uninstall",
        "/v1/arts/",
        "/uninstall",
    )
    .is_none());
}

#[test]
fn diagnostics_and_support_bundle_redact_secrets_and_require_existing_runs() {
    let root = unique_temp_dir("support-bundle-redaction");
    let mut store = InMemoryRunEvidenceStore::default();
    store
            .insert_run(
                json!({
                    "id": "run-support",
                    "capability": "art.execute",
                    "status": "succeeded",
                    "token": "top-secret-token",
                    "privateKey": "private-key-value",
                    "headers": { "Authorization": "Bearer header-secret" },
                    "message": "Bearer free-form-secret",
                    "url": "https://user:password@example.test/path?api_key=query-secret#fragment-secret"
                }),
                vec![RunEventDraft::new(
                    "external_tool_completed",
                    json!({
                        "status": "succeeded",
                        "refresh_token": "event-secret"
                    }),
                )
                .unwrap()],
            )
            .unwrap();
    let run_store: SharedRunStore = Arc::new(Mutex::new(Box::new(store)));
    let run_store_status = run_store.lock().unwrap().status();
    let tools = ToolRegistry::new(root.join("tools"));
    let frameworks = FrameworkRegistry::new(&root);

    let (status, body) = support_bundle(
        "/v1/support-bundle?runId=run-support",
        &HookSettings::default(),
        &run_store,
        run_store_status,
        &tools,
        &frameworks,
        &root,
    )
    .expect("support bundle");
    assert_eq!(status, 200);
    for secret in [
        "top-secret-token",
        "private-key-value",
        "header-secret",
        "free-form-secret",
        "query-secret",
        "fragment-secret",
        "event-secret",
        "password",
    ] {
        assert!(
            !body.contains(secret),
            "support bundle leaked {secret}: {body}"
        );
    }
    assert!(body.contains("[REDACTED]"));

    let (status, diagnostics) =
        execution_diagnostics("run-support", &run_store).expect("diagnostics");
    assert_eq!(status, 200);
    assert!(!diagnostics.contains("top-secret-token"));
    assert!(!diagnostics.contains("event-secret"));

    let (status, missing) = support_bundle(
        "/v1/support-bundle?runId=missing-run",
        &HookSettings::default(),
        &run_store,
        run_store_status,
        &tools,
        &frameworks,
        &root,
    )
    .expect("missing support run response");
    assert_eq!(status, 404);
    assert!(missing.contains("run_not_found"));
    fs::remove_dir_all(&root).ok();
}

#[test]
fn hook_art_execution_creates_durable_run_evidence() {
    let root = unique_temp_dir("hook-execution-evidence");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let hook_response = run_hook_bridge_text(
        &runtime,
        &formal_art_execute_request(
            "missing-request",
            "missing-node",
            "missing-art",
            None,
            json!({}),
        ),
    );
    assert_eq!(hook_response["status"], "failed");
    let hook_run_id = hook_response["data"]["executionId"]
        .as_str()
        .expect("Hook execution id");
    let store = runtime.run_store.lock().expect("run store");
    let hook_run = store
        .get_run(hook_run_id)
        .expect("read Hook run")
        .expect("Hook run");
    assert_eq!(hook_run["capability"], "art.execute");
    assert_eq!(hook_run["surface"], "loom_hook_v1");
    assert_eq!(hook_run["externalRequestId"], "missing-request");
    assert_eq!(hook_run["status"], "failed");
    let hook_events = store
        .get_events(hook_run_id)
        .expect("Hook events")
        .expect("stored Hook events");
    assert_eq!(hook_events[0]["kind"], "external_tool_started");
    assert_eq!(hook_events[1]["kind"], "external_tool_failed");
    drop(store);

    drop(runtime);
    fs::remove_dir_all(&root).ok();
}
