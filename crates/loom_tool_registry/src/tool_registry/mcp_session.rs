//! MCP session reuse, schema normalization, and error mapping.

use super::*;

pub(super) const MAX_CACHED_MCP_SESSIONS: usize = 8;
pub(super) const MCP_SESSION_IDLE_LIFETIME: Duration = Duration::from_secs(60);

pub(super) struct CachedMcpSession {
    pub(super) key: String,
    pub(super) client: loom_mcp::McpClient,
    pub(super) tools: Option<serde_json::Value>,
    pub(super) listing_failure: Option<String>,
    pub(super) reusable: bool,
    pub(super) last_used: Instant,
}

#[derive(Default)]
pub(super) struct McpSessionPool {
    sessions: Vec<CachedMcpSession>,
}

thread_local! {
    static MCP_SESSION_POOL: RefCell<McpSessionPool> = RefCell::new(McpSessionPool::default());
}

pub(super) fn mcp_session_key(
    server: &loom_mcp::McpServerConfig,
    timeout: Option<Duration>,
) -> ToolRegistryResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(server)?);
    hasher.update(format!("{:?}", crate::network_policy::runtime_proxy()));
    hasher.update(
        timeout
            .map(|value| value.as_millis())
            .unwrap_or_default()
            .to_le_bytes(),
    );
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn take_cached_mcp_session(key: &str) -> Option<CachedMcpSession> {
    let now = Instant::now();
    let (session, expired) = MCP_SESSION_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let mut expired = Vec::new();
        let mut index = 0;
        while index < pool.sessions.len() {
            if now.saturating_duration_since(pool.sessions[index].last_used)
                >= MCP_SESSION_IDLE_LIFETIME
            {
                expired.push(pool.sessions.remove(index));
            } else {
                index += 1;
            }
        }
        let session = pool
            .sessions
            .iter()
            .position(|session| session.key == key)
            .map(|index| pool.sessions.remove(index));
        (session, expired)
    });
    for mut session in expired {
        let _ = session.client.close();
    }
    session
}

pub(super) fn return_cached_mcp_session(mut session: CachedMcpSession) {
    session.last_used = Instant::now();
    let evicted = MCP_SESSION_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let duplicate = pool.sessions.iter().any(|cached| cached.key == session.key);
        if duplicate {
            Some(session)
        } else {
            pool.sessions.push(session);
            if pool.sessions.len() > MAX_CACHED_MCP_SESSIONS {
                let oldest = pool
                    .sessions
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, session)| session.last_used)
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                Some(pool.sessions.remove(oldest))
            } else {
                None
            }
        }
    });
    if let Some(mut evicted) = evicted {
        let _ = evicted.client.close();
    }
}

#[cfg(test)]
pub(super) fn clear_cached_mcp_sessions_for_current_thread() {
    let sessions = MCP_SESSION_POOL.with(|pool| std::mem::take(&mut pool.borrow_mut().sessions));
    for mut session in sessions {
        let _ = session.client.close();
    }
}

pub(super) fn acquire_mcp_session(
    tool: &ToolDefinition,
    server: &loom_mcp::McpServerConfig,
    timeout: Option<Duration>,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<CachedMcpSession> {
    let key = mcp_session_key(server, timeout)?;
    if let Some(session) = take_cached_mcp_session(&key) {
        return Ok(session);
    }
    let mut client = match timeout {
        Some(timeout) => loom_mcp::McpClient::connect_with_timeout(server, timeout)?,
        None => loom_mcp::McpClient::connect(server)?,
    };
    let initialize = match cancellation {
        Some(cancellation) => client.initialize_cancellable(cancellation),
        None => client.initialize(),
    };
    if let Err(error) = initialize {
        client.cancel();
        return Err(mcp_execution_error(tool, error));
    }
    let listing = match cancellation {
        Some(cancellation) => client.list_tools_cancellable(cancellation),
        None => client.list_tools(),
    };
    let (tools, listing_failure, reusable) = match listing {
        Ok(tools) => (Some(tools), None, true),
        Err(loom_mcp::McpError::Cancelled) => {
            client.cancel();
            return Err(ToolRegistryError::ExecutionCancelled {
                id: tool.id.clone(),
            });
        }
        Err(error) => (None, Some(error.to_string()), false),
    };
    Ok(CachedMcpSession {
        key,
        client,
        tools,
        listing_failure,
        reusable,
        last_used: Instant::now(),
    })
}
pub(super) fn mcp_execution_error(
    tool: &ToolDefinition,
    error: loom_mcp::McpError,
) -> ToolRegistryError {
    match error {
        loom_mcp::McpError::Cancelled => ToolRegistryError::ExecutionCancelled {
            id: tool.id.clone(),
        },
        error => ToolRegistryError::Mcp(error),
    }
}

/// Turn a failed MCP call into the error that leaves this crate, folding in an earlier listing failure.
///
/// When the server listed its tools normally, the call error is reported as it arrived. When the
/// listing failed first, the two are reported together: the arguments were sent without the schema
/// that would have shaped them, which is a plausible cause of the rejection and is otherwise invisible
/// to whoever reads the error. Both texts are bounded, since either can carry a server's response body.
pub(super) fn mcp_call_error(
    error: loom_mcp::McpError,
    listing_failure: Option<&str>,
) -> ToolRegistryError {
    match listing_failure {
        Some(reason) => ToolRegistryError::Mcp(loom_mcp::McpError::Protocol(format!(
            "{}; the server's tool listing failed first, so the arguments were sent without schema \
             guidance: {}",
            bounded_error_text(&error.to_string()),
            bounded_error_text(reason)
        ))),
        None => ToolRegistryError::Mcp(bounded_mcp_error(error)),
    }
}

/// Bound server- and process-controlled strings while preserving the useful MCP error category.
fn bounded_mcp_error(error: loom_mcp::McpError) -> loom_mcp::McpError {
    use loom_mcp::McpError;

    match error {
        McpError::ProcessStart { command, source } => McpError::ProcessStart {
            command: bounded_error_text(&command),
            source,
        },
        McpError::MissingPipe { pipe } => McpError::MissingPipe { pipe },
        McpError::Io(source) => McpError::Io(source),
        McpError::Json(source) => McpError::Json(source),
        McpError::JsonRpc(value) => McpError::Protocol(bounded_error_text(&format!(
            "server returned JSON-RPC error: {value}"
        ))),
        McpError::Protocol(message) => McpError::Protocol(bounded_error_text(&message)),
        McpError::ProcessSupervision { command, reason } => McpError::ProcessSupervision {
            command: bounded_error_text(&command),
            reason: bounded_error_text(&reason),
        },
        McpError::Timeout { timeout_ms, stderr } => McpError::Timeout {
            timeout_ms,
            stderr: bounded_error_text(&stderr),
        },
        McpError::OutputLimit { limit } => McpError::OutputLimit { limit },
        McpError::ProcessExited { code, stderr } => McpError::ProcessExited {
            code,
            stderr: bounded_error_text(&stderr),
        },
        McpError::Disabled { server_id } => McpError::Disabled {
            server_id: bounded_error_text(&server_id),
        },
        McpError::InvalidConfig(message) => McpError::InvalidConfig(bounded_error_text(&message)),
        McpError::PackageIntegrity(message) => {
            McpError::PackageIntegrity(bounded_error_text(&message))
        }
        McpError::UnsupportedTransport(transport) => {
            McpError::UnsupportedTransport(bounded_error_text(&transport))
        }
        McpError::Http(message) => McpError::Http(bounded_error_text(&message)),
        McpError::HttpStatus { status, body } => McpError::HttpStatus {
            status,
            body: bounded_error_text(&body),
        },
        McpError::Cancelled => McpError::Cancelled,
    }
}

pub(super) fn find_mcp_tool_input_schema<'a>(
    listed_tools: &'a serde_json::Value,
    tool_name: &str,
) -> Option<&'a serde_json::Value> {
    listed_tools
        .get("tools")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|tool| tool.get("name").and_then(serde_json::Value::as_str) == Some(tool_name))
        .and_then(|tool| tool.get("inputSchema"))
}

pub(super) fn normalize_mcp_call_arguments(
    arguments: &serde_json::Value,
    input_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let Some(argument_object) = arguments.as_object() else {
        return arguments.clone();
    };
    let property_schemas = input_schema
        .and_then(|schema| schema.get("properties"))
        .and_then(serde_json::Value::as_object);
    let mut normalized = serde_json::Map::with_capacity(argument_object.len());
    for (key, value) in argument_object {
        let schema = property_schemas.and_then(|properties| properties.get(key));
        normalized.insert(key.clone(), normalize_mcp_argument_value(value, schema));
    }
    serde_json::Value::Object(normalized)
}

pub(super) fn normalize_mcp_argument_value(
    value: &serde_json::Value,
    schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    if let Some(schema) = schema {
        if schema_type_matches(schema, "integer") {
            if let Some(parsed) = value.as_i64() {
                return serde_json::Value::from(parsed);
            }
            if let Some(raw) = value.as_str().map(str::trim) {
                if let Ok(parsed) = raw.parse::<i64>() {
                    return serde_json::Value::from(parsed);
                }
            }
        }
        if schema_type_matches(schema, "number") {
            if let Some(parsed) = value.as_f64() {
                return serde_json::Value::from(parsed);
            }
            if let Some(raw) = value.as_str().map(str::trim) {
                if let Ok(parsed) = raw.parse::<f64>() {
                    return serde_json::Value::from(parsed);
                }
            }
        }
        if schema_type_matches(schema, "boolean") {
            if let Some(parsed) = value.as_bool() {
                return serde_json::Value::from(parsed);
            }
            if let Some(raw) = value.as_str().map(str::trim) {
                if let Some(parsed) = parse_bool_string(raw) {
                    return serde_json::Value::from(parsed);
                }
            }
        }
        if let (Some(raw), Some(enum_values)) = (
            value.as_str(),
            schema.get("enum").and_then(serde_json::Value::as_array),
        ) {
            if let Some(canonical) = enum_values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .find(|candidate| candidate.eq_ignore_ascii_case(raw))
            {
                return serde_json::Value::String(canonical.to_owned());
            }
        }
    }
    value.clone()
}

pub(super) fn schema_type_matches(schema: &serde_json::Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(serde_json::Value::String(actual)) => actual == expected,
        Some(serde_json::Value::Array(actual)) => actual
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|candidate| candidate == expected),
        _ => false,
    }
}

pub(super) fn parse_bool_string(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
