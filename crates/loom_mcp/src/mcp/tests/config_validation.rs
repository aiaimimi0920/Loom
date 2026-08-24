//! Configuration, validation, and outbound policy coverage.

use super::*;

#[test]
pub(super) fn registry_url_encodes_search_limit_and_cursor() {
    let url = build_registry_url(
        Some("brave search"),
        Some(250),
        Some("ai.example/server:1.0.0"),
    )
    .expect("registry url");

    assert_eq!(
        url,
        "https://registry.modelcontextprotocol.io/v0.1/servers?limit=100&search=brave%20search&cursor=ai.example%2Fserver%3A1.0.0&version=latest"
    );
}

#[test]
pub(super) fn registry_url_omits_blank_search_and_cursor() {
    let url = build_registry_url(Some("   "), Some(0), Some(" "))
        .expect("registry url without optional terms");

    assert_eq!(
        url,
        "https://registry.modelcontextprotocol.io/v0.1/servers?limit=1&version=latest"
    );
}

#[test]
pub(super) fn server_config_defaults_enabled() {
    let config = McpServerConfig::new("brave", "Brave Search", "npx")
        .arg("-y")
        .arg("@brave/brave-search-mcp-server")
        .env("BRAVE_API_KEY", "test-key");

    assert_eq!(config.id, "brave");
    assert_eq!(config.name, "Brave Search");
    assert_eq!(config.command, "npx");
    assert_eq!(config.args, vec!["-y", "@brave/brave-search-mcp-server"]);
    assert_eq!(
        config.env.get("BRAVE_API_KEY").map(String::as_str),
        Some("test-key")
    );
    assert!(config.enabled);
}

#[test]
pub(super) fn stdio_server_config_requires_explicit_transport() {
    assert!(
        serde_json::from_value::<McpServerConfig>(serde_json::json!({
            "id": "local",
            "name": "Local",
            "command": "npx",
            "args": ["-y", "local-mcp"]
        }))
        .is_err()
    );

    let config: McpServerConfig = serde_json::from_value(serde_json::json!({
        "id": "local",
        "name": "Local",
        "command": "npx",
        "args": ["-y", "local-mcp"],
        "transport": "stdio"
    }))
    .expect("explicit stdio MCP config");

    assert_eq!(config.transport, McpTransport::Stdio);
    assert!(config.url.is_empty());
    assert!(config.headers.is_empty());
    config.validate().expect("valid explicit stdio config");
}

#[test]
pub(super) fn a_relative_stdio_command_is_refused() {
    // A relative path is completed by the daemon's working directory, so the same configuration
    // starts different files depending on where the daemon was launched from.
    let relative = McpServerConfig::new("local", "Local", "runtime/server.exe");
    assert!(
        matches!(relative.validate(), Err(McpError::InvalidConfig(message))
            if message.contains("relative path")),
        "a relative stdio command must be refused"
    );
    assert!(
        McpServerConfig::new("local", "Local", "./server")
            .validate()
            .is_err(),
        "an explicitly current-directory command must be refused too"
    );

    // A bare name is a `PATH` lookup, which is how servers are normally launched, and an absolute
    // path says exactly what it means. Both stay valid.
    McpServerConfig::new("local", "Local", "npx")
        .validate()
        .expect("a bare program name is a PATH lookup");
    let absolute = if cfg!(windows) {
        r"C:\tools\server.exe"
    } else {
        "/usr/bin/server"
    };
    McpServerConfig::new("local", "Local", absolute)
        .validate()
        .expect("an absolute command is unambiguous");
}

#[test]
pub(super) fn remote_server_config_rejects_embedded_credentials_and_templates() {
    let embedded =
        McpServerConfig::remote("remote", "Remote", "https://user:secret@example.test/mcp");
    assert!(matches!(
        embedded.validate(),
        Err(McpError::InvalidConfig(_))
    ));

    let templated =
        McpServerConfig::remote("remote", "Remote", "https://{tenant}.example.test/mcp");
    assert!(matches!(
        templated.validate(),
        Err(McpError::InvalidConfig(_))
    ));
}

#[test]
pub(super) fn server_config_enforces_manifest_sized_fields_and_tool_identifiers() {
    let mut config = McpServerConfig::new("local", "Local", "npx");
    config.args = vec!["argument".to_owned(); MAX_MCP_ARGUMENTS + 1];
    assert!(matches!(
        config.validate(),
        Err(McpError::InvalidConfig(message)) if message.contains("entry.args") && message.contains("limit")
    ));

    let mut config = McpServerConfig::new("local", "Local", "npx");
    config.tools = vec!["invalid tool name".to_owned()];
    assert!(matches!(
        config.validate(),
        Err(McpError::InvalidConfig(message)) if message.contains("tools[0]")
    ));
}

#[test]
pub(super) fn process_environment_and_remote_headers_share_security_bounds() {
    let mut config = McpServerConfig::new("local", "Local", "npx");
    config.env.insert("PATH".to_owned(), "attacker".to_owned());
    assert!(matches!(
        config.validate(),
        Err(McpError::InvalidConfig(message)) if message.contains("process-influencing")
    ));

    let mut config = McpServerConfig::remote("remote", "Remote", "https://example.test/mcp");
    config
        .headers
        .insert("Host".to_owned(), "attacker".to_owned());
    assert!(matches!(
        config.validate(),
        Err(McpError::InvalidConfig(message)) if message.contains("managed by Loom")
    ));

    let oversized_environment = BTreeMap::from([(
        "SAFE_VALUE".to_owned(),
        "x".repeat(MAX_MCP_ENVIRONMENT_TOTAL_BYTES),
    )]);
    assert!(validate_mcp_environment(&oversized_environment)
        .unwrap_err()
        .to_string()
        .contains("aggregate bytes"));

    let oversized_credential_header = BTreeMap::from([(
        "X-Api-Key".to_owned(),
        "x".repeat(MAX_MCP_HEADER_VALUE_BYTES + 1),
    )]);
    assert!(validate_mcp_headers(&oversized_credential_header)
        .unwrap_err()
        .to_string()
        .contains("value"));
}

#[test]
pub(super) fn installed_package_state_is_revalidated_with_user_server_config() {
    let mut config = McpServerConfig::new("fixture", "Fixture", "npx");
    config.package = Some(McpServerPackageState {
        qualified_id: "publisher.test/other".to_owned(),
        publisher_id: "publisher.test".to_owned(),
        version: "1.0.0".to_owned(),
        digest: "a".repeat(64),
        package_dir: std::env::temp_dir().join("loom-mcp-fixture-package"),
        files: BTreeMap::new(),
        trust_status: PackageTrustStatus::Unsigned,
    });

    assert!(matches!(
        config.validate(),
        Err(McpError::InvalidConfig(message)) if message.contains("package.qualifiedId")
    ));
}

#[cfg(windows)]
#[test]
pub(super) fn windows_path_extensions_do_not_implicitly_enable_powershell() {
    let extensions = windows_path_extensions_from(Some(std::ffi::OsStr::new(".EXE;.CMD")));
    assert_eq!(extensions, vec![".exe", ".cmd"]);
    assert!(!extensions.iter().any(|extension| extension == ".ps1"));

    let explicit = windows_path_extensions_from(Some(std::ffi::OsStr::new(".EXE;.PS1")));
    assert_eq!(explicit, vec![".exe", ".ps1"]);
}

#[test]
pub(super) fn remote_config_requires_https_unless_the_operator_allows_a_loopback_endpoint() {
    let public = Url::parse("http://mcp.example.test/mcp").expect("public URL");
    let error = ensure_remote_scheme_allowed(&public, true, false)
        .expect_err("credentialed plain http must be refused");
    assert!(
        error.to_string().contains("cleartext"),
        "unexpected error: {error}"
    );
    assert!(ensure_remote_scheme_allowed(&public, false, false).is_err());
    // Opting in covers the local machine only; a name that merely resolves to loopback is
    // controlled by whoever answers DNS, so it stays refused.
    assert!(ensure_remote_scheme_allowed(&public, false, true).is_err());

    let loopback = Url::parse("http://127.0.0.1:9000/mcp").expect("loopback URL");
    assert!(ensure_remote_scheme_allowed(&loopback, true, false).is_err());
    assert!(ensure_remote_scheme_allowed(&loopback, true, true).is_ok());

    let secure = Url::parse("https://mcp.example.test/mcp").expect("https URL");
    assert!(ensure_remote_scheme_allowed(&secure, true, false).is_ok());
}

#[test]
pub(super) fn remote_outbound_policy_refuses_local_private_and_metadata_addresses() {
    let policy = remote_outbound_policy(false);
    assert_eq!(
        policy.max_redirects, 0,
        "a redirect can move a credentialed request to a forbidden host"
    );
    for address in [
        "http://127.0.0.1:9200/",
        "https://127.0.0.1:9200/",
        "https://192.168.1.1/admin",
        "http://169.254.169.254/latest/meta-data/",
        "https://[fd00::1]/mcp",
    ] {
        let url = Url::parse(address).expect("test URL");
        assert!(
            validate_outbound_url(&url, &policy).is_err(),
            "{address} must be refused by default"
        );
    }

    let opted_in = remote_outbound_policy(true);
    for address in ["http://127.0.0.1:9200/", "https://192.168.1.1/admin"] {
        let url = Url::parse(address).expect("test URL");
        assert!(
            validate_outbound_url(&url, &opted_in).is_ok(),
            "{address} must be reachable once the operator opts in"
        );
    }
}
