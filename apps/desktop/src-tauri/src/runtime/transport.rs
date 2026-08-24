//! Loopback-only daemon JSON transport and degraded snapshot collection.

use super::*;

pub(super) fn read_daemon_snapshot(base_url: &str) -> Result<DaemonSnapshot, String> {
    let health = http_get_json(base_url, "/health")?;
    let status = http_get_json(base_url, "/status")?;
    let mut degraded_errors = Vec::new();
    let capabilities = read_optional_daemon_array(
        base_url,
        "/v1/capabilities",
        "capabilities",
        &mut degraded_errors,
    );
    let mcp_servers =
        read_optional_daemon_array(base_url, "/v1/mcp/servers", "servers", &mut degraded_errors);
    let tools = read_optional_daemon_array(base_url, "/v1/tools", "tools", &mut degraded_errors);
    let python_arts = read_optional_daemon_array(
        base_url,
        "/v1/art-authoring/python/arts",
        "arts",
        &mut degraded_errors,
    );
    let workflows =
        read_optional_daemon_array(base_url, "/v1/workflows", "workflows", &mut degraded_errors);
    let hook_bridge =
        read_optional_daemon_json(base_url, "/v1/hook-bridge/status", &mut degraded_errors);

    if !degraded_errors.is_empty() {
        http_get_json(base_url, "/health")
            .map_err(|error| format!("Loom 本地服务在读取模块状态期间离线：{error}"))?;
    }

    Ok(DaemonSnapshot {
        health,
        status,
        capabilities,
        mcp_servers,
        tools,
        python_arts,
        workflows,
        hook_bridge,
        degraded_errors,
    })
}

pub(super) fn read_optional_daemon_array(
    base_url: &str,
    path: &str,
    key: &str,
    degraded_errors: &mut Vec<String>,
) -> Vec<Value> {
    let Some(response) = read_optional_daemon_json(base_url, path, degraded_errors) else {
        return Vec::new();
    };
    let Some(values) = response.get(key).and_then(Value::as_array) else {
        degraded_errors.push(format!("{path} 返回的模块数据无效：`{key}` 必须是数组"));
        return Vec::new();
    };
    values.clone()
}

pub(super) fn read_optional_daemon_json(
    base_url: &str,
    path: &str,
    degraded_errors: &mut Vec<String>,
) -> Option<Value> {
    match http_get_json(base_url, path) {
        Ok(response) => Some(response),
        Err(error) => {
            degraded_errors.push(error);
            None
        }
    }
}

pub(super) fn snapshot_error(errors: &[String], warning: Option<&str>) -> Option<String> {
    let mut messages = Vec::new();
    if !errors.is_empty() {
        messages.push(format!(
            "Loom 本地服务在线，但部分模块暂不可用：{}",
            errors.join("；")
        ));
    }
    if let Some(warning) = warning.filter(|value| !value.trim().is_empty()) {
        messages.push(warning.to_owned());
    }
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("；"))
    }
}

pub(super) fn http_get_json(base_url: &str, path: &str) -> Result<Value, String> {
    http_request_json_with_timeout(base_url, "GET", path, None, daemon_get_timeout(path))
}

pub(super) fn daemon_get_timeout(path: &str) -> Duration {
    if path == "/v1/mcp/registry" || path.starts_with("/v1/mcp/registry?") {
        LOOM_MCP_REGISTRY_REQUEST_TIMEOUT
    } else {
        LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT
    }
}

pub(super) fn http_post_json(base_url: &str, path: &str, body: &Value) -> Result<Value, String> {
    http_request_json(base_url, "POST", path, Some(body))
}

pub(super) fn http_post_json_with_timeout(
    base_url: &str,
    path: &str,
    body: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    http_request_json_with_timeout(base_url, "POST", path, Some(body), timeout)
}

pub(super) fn http_put_json(base_url: &str, path: &str, body: &Value) -> Result<Value, String> {
    http_request_json(base_url, "PUT", path, Some(body))
}

pub(super) fn http_delete_json(base_url: &str, path: &str) -> Result<Value, String> {
    http_request_json(base_url, "DELETE", path, None)
}

pub(super) fn daemon_error_message(body: &str) -> Option<String> {
    let payload = serde_json::from_str::<Value>(body).ok()?;
    payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| payload.get("message").and_then(Value::as_str))
        .or_else(|| payload.get("detail").and_then(Value::as_str))
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn http_request_json(
    base_url: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    http_request_json_with_timeout(
        base_url,
        method,
        path,
        body,
        LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT,
    )
}

pub(super) fn http_request_json_with_timeout(
    base_url: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, String> {
    let (host, port) = parse_loopback_http_url(base_url)?;
    let method = match method {
        "GET" | "POST" | "PUT" | "DELETE" => method,
        _ => return Err("Loom 本地服务 HTTP 方法无效。".to_owned()),
    };
    let path = normalize_daemon_path(path.to_owned())?;
    let authorization = daemon_auth_token()?
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let mut stream = TcpStream::connect_timeout(
        &loopback_socket_addr(&host, port)?,
        LOOM_DAEMON_CONNECT_TIMEOUT,
    )
    .map_err(|error| format!("无法连接 Loom 本地服务 {base_url}：{error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("无法设置 Loom 本地服务读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("无法设置 Loom 本地服务写入超时：{error}"))?;

    let request = if let Some(body) = body {
        let body = serde_json::to_string(body)
            .map_err(|error| format!("无法序列化 Loom 本地服务请求 {path}：{error}"))?;
        if body.len() > MAX_DAEMON_JSON_REQUEST_BYTES {
            return Err(format!(
                "Loom 本地服务请求超过 {} 字节限制：{path}",
                MAX_DAEMON_JSON_REQUEST_BYTES
            ));
        }
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{authorization}Connection: close\r\n\r\n{body}",
            body.len()
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\n{authorization}Connection: close\r\n\r\n"
        )
    };
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("无法写入 Loom 本地服务请求 {path}：{error}"))?;

    let response = read_bounded_http_response(&mut stream, &path, MAX_DAEMON_JSON_RESPONSE_BYTES)?;
    let parsed = parse_http_response(&response, &path, MAX_DAEMON_JSON_RESPONSE_BYTES)?;
    let body = &response[parsed.body_offset..];
    if !(200..=299).contains(&parsed.status_code) {
        let body = String::from_utf8_lossy(body);
        if let Some(message) = daemon_error_message(&body) {
            return Err(format!("{path} returned {}: {message}", parsed.status_line));
        }
        return Err(format!("{path} returned {}", parsed.status_line));
    }
    if parsed
        .content_type
        .as_deref()
        .is_some_and(|value| !is_json_content_type(value))
    {
        return Err(format!("Loom 本地服务响应类型不是 JSON：{path}"));
    }
    let body = std::str::from_utf8(body)
        .map_err(|error| format!("Loom 本地服务响应不是 UTF-8：{path}: {error}"))?;
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_str(body)
        .map_err(|error| format!("无法解析 Loom 本地服务响应 {path}：{error}"))
}
