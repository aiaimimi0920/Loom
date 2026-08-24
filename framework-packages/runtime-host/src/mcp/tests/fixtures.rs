// Shared execution requests, resolved servers, and multi-call fixtures.
fn request() -> FrameworkExecuteRequest {
    FrameworkExecuteRequest {
        protocol_version: "loom.framework.v1".to_owned(),
        supported_protocol_versions: vec!["loom.framework.v1".to_owned()],
        framework_id: "mcp".to_owned(),
        art_id: "fixture".to_owned(),
        art_dir: PathBuf::from("art"),
        inputs: json!({ "input": "from-input", "disabled": true }),
        params: json!({ "query": "loom", "count": "2" }),
        disabled_params: vec!["disabled".to_owned()],
        context: FrameworkExecutionContext {
            mcp_server: Some(resolved_server()),
            credentials: vec![CredentialGrant {
                name: "api_key".to_owned(),
                value: "secret-value".to_owned(),
                expires_at: None,
            }],
            ..FrameworkExecutionContext::default()
        },
    }
}

fn resolved_server() -> FrameworkMcpServer {
    FrameworkMcpServer {
        id: "fixture".to_owned(),
        package_id: "publisher.test/fixture".to_owned(),
        version: "1.0.0".to_owned(),
        transport: "stdio".to_owned(),
        command: "fixture".to_owned(),
        credential_env: BTreeMap::from([("BRAVE_API_KEY".to_owned(), "api_key".to_owned())]),
        ..FrameworkMcpServer::default()
    }
}
