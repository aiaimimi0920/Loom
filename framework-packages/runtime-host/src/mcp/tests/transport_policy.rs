// Credential mapping, shared transport policy, and path-expansion contracts.
#[test]
fn credential_alias_maps_to_server_environment() {
    let config = resolved_server();
    let environment = build_environment(&request(), &config).unwrap();
    assert_eq!(
        environment.get("BRAVE_API_KEY"),
        Some(&"secret-value".to_owned())
    );
}

#[test]
fn credential_mapping_declared_required_and_optional_is_rejected() {
    let mut config = resolved_server();
    config
        .optional_credential_env
        .insert("BRAVE_API_KEY".to_owned(), "weaker_alias".to_owned());
    assert!(build_environment(&request(), &config)
        .unwrap_err()
        .contains("both required and optional"));

    let mut config = resolved_server();
    config
        .credential_headers
        .insert("X-Api-Key".to_owned(), "api_key".to_owned());
    config
        .optional_credential_headers
        .insert("X-Api-Key".to_owned(), "weaker_alias".to_owned());
    assert!(build_headers(&request(), &config)
        .unwrap_err()
        .contains("both required and optional"));
}

#[test]
fn runtime_host_uses_shared_environment_and_header_security_policy() {
    let mut config = resolved_server();
    config.env.insert("PATH".to_owned(), "attacker".to_owned());
    assert!(build_environment(&request(), &config)
        .unwrap_err()
        .contains("process-influencing"));

    let mut config = resolved_server();
    config.credential_env.clear();
    config
        .headers
        .insert("Host".to_owned(), "attacker".to_owned());
    assert!(build_headers(&request(), &config)
        .unwrap_err()
        .contains("managed by Loom"));

    let mut oversized_request = request();
    oversized_request.context.credentials[0].value =
        "x".repeat(loom_mcp::MAX_MCP_HEADER_VALUE_BYTES + 1);
    let mut config = resolved_server();
    config.credential_env.clear();
    config
        .credential_headers
        .insert("X-Api-Key".to_owned(), "api_key".to_owned());
    assert!(build_headers(&oversized_request, &config)
        .unwrap_err()
        .contains("value"));
}

#[test]
fn stdio_command_art_dir_expansion_is_absolute_and_anchored() {
    let mut request = request();
    let art_dir = std::env::temp_dir().join("loom-runtime-host-art");
    request.art_dir = art_dir.clone();
    let expanded = expand_stdio_command("{artDir}/runtime/server", &request, &art_dir)
        .expect("expand anchored Art command");
    assert!(Path::new(&expanded).is_absolute());
    assert!(Path::new(&expanded).starts_with(&art_dir));

    assert!(
        expand_stdio_command("{artDir}/../server", &request, &art_dir)
            .unwrap_err()
            .contains("escapes")
    );
    assert!(expand_stdio_command("{tempDir}/server", &request, &art_dir)
        .unwrap_err()
        .contains("only use"));
}

#[test]
fn remote_destinations_do_not_expand_local_path_placeholders() {
    let mut request = request();
    request.art_dir = if cfg!(windows) {
        PathBuf::from(r"C:\private\art")
    } else {
        PathBuf::from("/private/art")
    };
    let mut config = resolved_server();
    config.transport = "streamable-http".to_owned();
    config.command.clear();
    config.url = "https://example.test/{artDir}/mcp".to_owned();
    config.args = vec!["{tempDir}/secret".to_owned()];
    request.context.mcp_server = Some(config);

    let resolved = request.context.mcp_server.as_ref().unwrap();
    assert_eq!(resolved.url, "https://example.test/{artDir}/mcp");
    assert!(
        McpServerConfig::remote("remote", "Remote", resolved.url.clone())
            .validate()
            .unwrap_err()
            .to_string()
            .contains("template")
    );
    assert!(!resolved
        .url
        .contains(request.art_dir.to_string_lossy().as_ref()));
    assert_eq!(resolved.args, vec!["{tempDir}/secret"]);
}
