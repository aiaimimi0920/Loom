//! Registry URL and MCP JSON-RPC protocol helpers.

use super::*;

/// Build the official MCP Registry URL using bounded pagination.
pub fn build_registry_url(
    search: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> McpResult<String> {
    let safe_limit = limit.unwrap_or(60).clamp(1, 100);
    let mut pairs = vec![format!("limit={safe_limit}")];

    if let Some(search_text) = search.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("search={}", percent_encode(search_text)));
    }

    if let Some(cursor_text) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("cursor={}", percent_encode(cursor_text)));
    }

    pairs.push("version=latest".to_owned());
    Ok(format!("{MCP_REGISTRY_ENDPOINT}?{}", pairs.join("&")))
}

#[must_use]
pub fn initialize_request(id: u64) -> serde_json::Value {
    initialize_request_for_version(id, MCP_PREFERRED_PROTOCOL_VERSION)
}

#[must_use]
pub fn initialize_request_for_version(id: u64, protocol_version: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {
                "name": "Loom",
                "version": LOOM_MCP_VERSION
            }
        }
    })
}

pub(super) fn validate_initialize_result(result: &JsonValue) -> McpResult<String> {
    let object = result.as_object().ok_or_else(|| {
        McpError::Protocol("MCP initialize result must be a JSON object".to_owned())
    })?;
    let protocol_version = object
        .get("protocolVersion")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or_else(|| {
            McpError::Protocol(
                "MCP initialize result requires a protocolVersion of at most 64 bytes".to_owned(),
            )
        })?;
    if !MCP_SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version) {
        return Err(McpError::Protocol(format!(
            "MCP server selected unsupported protocolVersion `{protocol_version}`; Loom supports {}",
            MCP_SUPPORTED_PROTOCOL_VERSIONS.join(", ")
        )));
    }
    if !object.get("capabilities").is_some_and(JsonValue::is_object) {
        return Err(McpError::Protocol(
            "MCP initialize result requires an object capabilities field".to_owned(),
        ));
    }
    let server_info = object
        .get("serverInfo")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            McpError::Protocol("MCP initialize result requires serverInfo".to_owned())
        })?;
    for field in ["name", "version"] {
        let value = server_info
            .get(field)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 256)
            .ok_or_else(|| {
                McpError::Protocol(format!(
                    "MCP initialize serverInfo.{field} must be non-empty and at most 256 bytes"
                ))
            })?;
        if value.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(McpError::Protocol(format!(
                "MCP initialize serverInfo.{field} contains control characters"
            )));
        }
    }
    Ok(protocol_version.to_owned())
}

pub(super) fn is_protocol_compatibility_rejection(error: &McpError) -> bool {
    let text = match error {
        McpError::JsonRpc(value) => value.to_string(),
        McpError::HttpStatus { body, .. } => body.clone(),
        _ => return false,
    }
    .to_ascii_lowercase();
    let names_protocol = text.contains("protocol") || text.contains("version");
    let rejects_version = text.contains("unsupported")
        || text.contains("not supported")
        || text.contains("incompatible");
    names_protocol && rejects_version
}

pub(super) fn no_common_protocol_error(last_error: &McpError) -> McpError {
    McpError::Protocol(format!(
        "MCP server rejected every supported protocol revision ({}); last rejection: {last_error}",
        MCP_SUPPORTED_PROTOCOL_VERSIONS.join(", ")
    ))
}
#[must_use]
pub fn initialized_notification() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

#[must_use]
pub fn tools_list_request(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list",
        "params": {}
    })
}

#[must_use]
pub fn tools_call_request(id: u64, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments
        }
    })
}

pub(super) fn validate_tool_call_payload(name: &str, arguments: &JsonValue) -> McpResult<()> {
    validate_mcp_tool_identifier("tool name", name)?;
    let serialized_bytes = serde_json::to_vec(arguments)?.len();
    if serialized_bytes > MCP_MAX_MESSAGE_BYTES {
        return Err(McpError::OutputLimit {
            limit: MCP_MAX_MESSAGE_BYTES,
        });
    }
    Ok(())
}

pub(super) fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
