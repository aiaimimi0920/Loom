//! Runtime limits and JSON-RPC protocol coverage.

use super::*;

#[test]
pub(super) fn runtime_limits_can_be_updated_for_new_clients() {
    let _guard = ProcessConfigTestGuard::capture();
    configure_runtime_limits(30, 1024 * 1024 * 1024);
    assert_eq!(runtime_limits(), (30, 1024 * 1024 * 1024));
}

#[test]
pub(super) fn tools_list_request_uses_mcp_json_rpc_shape() {
    let request = tools_list_request(2);

    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], 2);
    assert_eq!(request["method"], "tools/list");
    assert_eq!(request["params"], serde_json::json!({}));
}

#[test]
pub(super) fn tools_call_request_embeds_tool_name_and_arguments() {
    let request = tools_call_request(
        3,
        "brave_web_search",
        serde_json::json!({
            "query": "loom mcp",
            "count": 3
        }),
    );

    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], 3);
    assert_eq!(request["method"], "tools/call");
    assert_eq!(request["params"]["name"], "brave_web_search");
    assert_eq!(request["params"]["arguments"]["query"], "loom mcp");
    assert_eq!(request["params"]["arguments"]["count"], 3);
}

#[test]
pub(super) fn initialize_request_identifies_loom_client() {
    let request = initialize_request(1);

    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], 1);
    assert_eq!(request["method"], "initialize");
    assert_eq!(
        request["params"]["protocolVersion"],
        MCP_PREFERRED_PROTOCOL_VERSION
    );
    assert_eq!(request["params"]["clientInfo"]["name"], "Loom");
}

#[test]
pub(super) fn initialize_response_conformance_table_is_shared_by_both_transports() {
    for version in MCP_SUPPORTED_PROTOCOL_VERSIONS {
        let result = serde_json::json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "fixture", "version": "1.0.0" }
        });
        assert_eq!(
            validate_initialize_result(&result).unwrap(),
            *version,
            "supported revision {version}"
        );
    }

    let incompatible = serde_json::json!({
        "protocolVersion": "2099-01-01",
        "capabilities": {},
        "serverInfo": { "name": "fixture", "version": "1.0.0" }
    });
    let error = validate_initialize_result(&incompatible).unwrap_err();
    assert!(error.to_string().contains("unsupported protocolVersion"));
    assert!(error
        .to_string()
        .contains(MCP_SUPPORTED_PROTOCOL_VERSIONS[0]));

    for malformed in [
        serde_json::json!({
            "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
            "serverInfo": { "name": "fixture", "version": "1.0.0" }
        }),
        serde_json::json!({
            "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
            "capabilities": {},
            "serverInfo": { "name": "", "version": "1.0.0" }
        }),
    ] {
        assert!(validate_initialize_result(&malformed).is_err());
    }
}
