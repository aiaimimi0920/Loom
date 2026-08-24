//! Bounded HTTP execution and JSON/SSE response parsing.

use super::*;

pub(super) const MCP_MAX_HTTP_MESSAGES: usize = 256;

pub(super) struct HttpWireResponse {
    pub(super) status: reqwest::StatusCode,
    pub(super) content_type: String,
    pub(super) session_id: Option<String>,
    pub(super) body: Vec<u8>,
}

pub(super) async fn wait_for_cancellation(cancellation: &AtomicBool) {
    while !cancellation.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub(super) async fn execute_http_request(
    request: reqwest::RequestBuilder,
    cancellation: Option<&AtomicBool>,
) -> McpResult<HttpWireResponse> {
    let mut response = if let Some(cancellation) = cancellation {
        tokio::select! {
            response = request.send() => response,
            () = wait_for_cancellation(cancellation) => return Err(McpError::Cancelled),
        }
    } else {
        request.send().await
    }
    .map_err(|error| McpError::Http(error.without_url().to_string()))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let session_id = response
        .headers()
        .get("MCP-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let body = read_bounded_http_body(&mut response, cancellation).await?;
    Ok(HttpWireResponse {
        status,
        content_type,
        session_id,
        body,
    })
}

pub(super) async fn read_bounded_http_body(
    response: &mut reqwest::Response,
    cancellation: Option<&AtomicBool>,
) -> McpResult<Vec<u8>> {
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MCP_MAX_MESSAGE_BYTES as u64) {
        return Err(McpError::OutputLimit {
            limit: MCP_MAX_MESSAGE_BYTES,
        });
    }
    let mut body = Vec::with_capacity(
        content_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or_default(),
    );
    loop {
        let chunk = if let Some(cancellation) = cancellation {
            tokio::select! {
                chunk = response.chunk() => chunk,
                () = wait_for_cancellation(cancellation) => return Err(McpError::Cancelled),
            }
        } else {
            response.chunk().await
        }
        .map_err(|error| McpError::Http(error.without_url().to_string()))?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len().saturating_add(chunk.len()) > MCP_MAX_MESSAGE_BYTES {
            return Err(McpError::OutputLimit {
                limit: MCP_MAX_MESSAGE_BYTES,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(super) fn bounded_error_body(body: &[u8], sensitive_values: &[String]) -> String {
    const ERROR_BODY_LIMIT: usize = 2048;
    let text = String::from_utf8_lossy(body);
    let redacted = redact_sensitive_text(text.trim(), sensitive_values);
    if redacted.len() <= ERROR_BODY_LIMIT {
        return redacted;
    }
    let mut end = ERROR_BODY_LIMIT;
    while end > 0 && !redacted.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [truncated]", &redacted[..end])
}

pub(super) fn parse_json_messages(body: &[u8]) -> McpResult<Vec<JsonValue>> {
    let value = serde_json::from_slice::<JsonValue>(body)?;
    let messages = match value {
        JsonValue::Array(messages) => messages,
        message => vec![message],
    };
    enforce_http_message_count(messages.len())?;
    Ok(messages)
}

pub(super) fn parse_sse_messages(body: &[u8]) -> McpResult<Vec<JsonValue>> {
    let text = String::from_utf8_lossy(body);
    let mut messages = Vec::new();
    let mut data_lines = Vec::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            data_lines.clear();
            let value = serde_json::from_str::<JsonValue>(&data)?;
            match value {
                JsonValue::Array(values) => {
                    enforce_http_message_count(messages.len().saturating_add(values.len()))?;
                    messages.extend(values);
                }
                value => {
                    enforce_http_message_count(messages.len().saturating_add(1))?;
                    messages.push(value);
                }
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.strip_prefix(' ').unwrap_or(data));
        }
    }
    if messages.is_empty() {
        return Err(McpError::Protocol(
            "MCP SSE response did not contain a JSON data event".to_owned(),
        ));
    }
    Ok(messages)
}

fn enforce_http_message_count(count: usize) -> McpResult<()> {
    if count > MCP_MAX_HTTP_MESSAGES {
        return Err(McpError::Protocol(format!(
            "MCP HTTP response contains more than {MCP_MAX_HTTP_MESSAGES} messages"
        )));
    }
    Ok(())
}

pub(super) fn result_from_messages(
    messages: Vec<JsonValue>,
    expected_id: u64,
) -> McpResult<JsonValue> {
    let expected = serde_json::json!(expected_id);
    let response = messages
        .into_iter()
        .find(|message| message.get("id") == Some(&expected))
        .ok_or_else(|| {
            McpError::Protocol(format!(
                "MCP HTTP response did not contain id {expected_id}"
            ))
        })?;
    if let Some(error) = response.get("error") {
        return Err(McpError::JsonRpc(error.clone()));
    }
    response.get("result").cloned().ok_or_else(|| {
        McpError::Protocol(format!("MCP HTTP response id {expected_id} missing result"))
    })
}
