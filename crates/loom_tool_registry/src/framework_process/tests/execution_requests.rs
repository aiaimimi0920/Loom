use super::super::*;
use super::execution_support::*;
use std::fs;

#[test]
fn process_request_contains_art_inputs_params_and_context() {
    let root = temp_root("success");
    let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
    let result = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({
            "inputs": { "image": "input.png" },
            "params": { "strength": 0.5 },
            "disabledParams": ["unused"]
        }),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect("framework process success");
    assert_eq!(
        result["request"]["protocolVersion"],
        FRAMEWORK_PROTOCOL_VERSION
    );
    assert_eq!(result["request"]["frameworkId"], "script");
    assert_eq!(result["request"]["artId"], "fixture-art");
    assert_eq!(result["request"]["inputs"]["image"], "input.png");
    assert_eq!(result["request"]["params"]["strength"], 0.5);
    assert_eq!(result["request"]["disabledParams"][0], "unused");
    assert_eq!(
        result["request"]["artDir"],
        art_dir.to_string_lossy().to_string()
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn process_request_contains_art_scoped_credential_bindings() {
    let root = temp_root("credentials");
    let packages_root = root.join("frameworks");
    fs::create_dir_all(&packages_root).expect("create framework packages root");
    let art_dir = write_fixture_package(&packages_root, SUCCESS_SCRIPT);
    let mut tool = fixture_tool(&art_dir);
    tool.metadata
        .as_mut()
        .and_then(Value::as_object_mut)
        .expect("fixture metadata")
        .insert(
            "packageSecurity".to_owned(),
            json!({
                "version": "1.0.0",
                "publisher": { "id": "publisher.test", "name": "Publisher" }
            }),
        );
    let art_identity = tool.qualified_id();
    crate::art_settings::ArtSettingsStore::new(&root)
        .save(
            &art_identity,
            crate::art_settings::ArtUserSettings {
                credential_bindings: BTreeMap::from([(
                    "api_key".to_owned(),
                    "stored-secret".to_owned(),
                )]),
                ..crate::art_settings::ArtUserSettings::default()
            },
        )
        .expect("persist fixture Art settings");
    crate::credentials::CredentialStore::new(&root)
        .upsert(crate::credentials::CredentialInput {
            name: "stored-secret".to_owned(),
            value: "fixture-value".to_owned(),
            value_type: crate::credentials::CredentialValueType::String,
            scope: crate::credentials::CredentialScope {
                framework_id: None,
                art_id: Some(art_identity.clone()),
                mcp_server_id: None,
            },
            expires_at: None,
        })
        .expect("store fixture credential");
    let tool = crate::ToolRegistry::new(root.join("tools"))
        .save_tool(tool)
        .expect("save fixture tool with persisted settings");
    assert_eq!(
        tool.metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artUserSettings/credentialBindings/api_key"))
            .and_then(Value::as_str),
        Some("stored-secret")
    );

    let result = execute_framework_art_in_root_with_timeout(
        &tool,
        "publisher.test/script",
        json!({}),
        &packages_root,
        Duration::from_secs(10),
        None,
    )
    .expect("framework process credential binding");
    assert_eq!(
        result["request"]["context"]["credentials"][0]["name"],
        "api_key"
    );
    assert_eq!(
        result["request"]["context"]["credentials"][0]["value"],
        "fixture-value"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn mcp_framework_resolves_independent_package_and_server_scoped_credentials() {
    let root = temp_root("independent-mcp");
    fs::create_dir_all(root.join("mcp")).expect("create MCP store root");
    let mut server = loom_mcp::McpServerConfig::new(
        "neuro-image-search",
        "Image Search",
        root.join("mcp/server.ps1").display().to_string(),
    );
    server
        .credential_env
        .insert("BRAVE_API_KEY".to_owned(), "brave_api_key".to_owned());
    server
        .credential_bindings
        .insert("brave_api_key".to_owned(), "stored-image-key".to_owned());
    server
        .credential_requirements
        .push(loom_mcp::McpCredentialRequirement {
            id: "brave_api_key".to_owned(),
            label: "Brave API Key".to_owned(),
            required: true,
        });
    let package_digest = "ab".repeat(32);
    server.package = Some(loom_mcp::McpServerPackageState {
        qualified_id: "neuro.official/neuro-image-search".to_owned(),
        publisher_id: "neuro.official".to_owned(),
        version: "0.1.0".to_owned(),
        digest: package_digest.clone(),
        package_dir: root
            .join("mcp/packages/neuro.official/neuro-image-search/versions")
            .join(format!("0.1.0-{package_digest}")),
        files: std::collections::BTreeMap::new(),
        trust_status: loom_protocol::PackageTrustStatus::Unsigned,
    });
    fs::write(
        root.join("mcp/servers.json"),
        serde_json::to_vec(&vec![server]).expect("serialize MCP store"),
    )
    .expect("write MCP store");
    crate::credentials::CredentialStore::new(&root)
        .upsert(crate::credentials::CredentialInput {
            name: "stored-image-key".to_owned(),
            value: "fixture-value".to_owned(),
            value_type: crate::credentials::CredentialValueType::String,
            scope: crate::credentials::CredentialScope {
                framework_id: None,
                art_id: None,
                mcp_server_id: Some("neuro-image-search".to_owned()),
            },
            expires_at: None,
        })
        .expect("store MCP credential");
    let mut tool = ToolDefinition::new(
        "custom-image-search",
        "Image Search",
        "MCP consumer",
        crate::ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.metadata = Some(json!({
        "packageSecurity": {
            "version": "0.4.0",
            "publisher": { "id": "neuro.official", "name": "Neuro" }
        },
        "mcp": {
            "serverId": "neuro-image-search",
            "packageId": "neuro.official/neuro-image-search",
            "version": "^0.1",
            "toolName": "brave_image_search"
        }
    }));
    let store = crate::credentials::CredentialStore::new(&root);
    let (resolved, credentials) =
        resolve_mcp_server(&tool, &root, Some(&store)).expect("resolve MCP dependency");
    assert_eq!(resolved.package_id, "neuro.official/neuro-image-search");
    assert_eq!(resolved.version, "0.1.0");
    assert_eq!(resolved.credential_env["BRAVE_API_KEY"], "brave_api_key");
    assert_eq!(credentials[0].name, "brave_api_key");
    assert_eq!(credentials[0].value, "fixture-value");
    fs::remove_dir_all(root).ok();
}

#[test]
fn flat_art_arguments_are_partitioned_by_manifest_schema() {
    let root = temp_root("flat-schema");
    let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
    let result = execute_framework_art_in_root_with_timeout(
        &fixture_tool_with_schema(&art_dir),
        "publisher.test/script",
        json!({
            "input": "source.png",
            "reference": "reference.png",
            "strength": 25
        }),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect("framework process success");

    assert_eq!(result["request"]["inputs"]["input"], "source.png");
    assert_eq!(result["request"]["inputs"]["reference"], "reference.png");
    assert_eq!(result["request"]["params"]["strength"], 25);
    assert!(result["request"]["inputs"].get("strength").is_none());
    fs::remove_dir_all(root).ok();
}

#[test]
fn execute_tool_routes_framework_art_to_the_external_process() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = temp_root("execute-tool");
    let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
    let _environment = EnvVarGuard::set("LOOM_FRAMEWORK_PACKAGES_DIR", &root);
    let result = crate::execute_tool(
        &fixture_tool_with_schema(&art_dir),
        &[],
        json!({
            "input": "source.png",
            "reference": "reference.png",
            "strength": 40
        }),
    )
    .expect("execute_tool external framework route");
    assert_eq!(result["request"]["inputs"]["input"], "source.png");
    assert_eq!(result["request"]["inputs"]["reference"], "reference.png");
    assert_eq!(result["request"]["params"]["strength"], 40);
    fs::remove_dir_all(root).ok();
}
