//! Streamable HTTP MCP client and configuration.

use super::*;

/// Synchronous MCP client for the standard Streamable HTTP transport.
pub struct StreamableHttpMcpClient {
    client: HttpClient,
    url: String,
    headers: HeaderMap,
    sensitive_values: Vec<String>,
    pub(super) session_id: Option<String>,
    pub(super) protocol_version: String,
    next_id: u64,
}

impl StreamableHttpMcpClient {
    pub fn connect(config: &McpServerConfig) -> McpResult<Self> {
        Self::connect_with_timeout(
            config,
            Duration::from_secs(MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed)),
        )
    }

    pub fn connect_with_timeout(
        config: &McpServerConfig,
        request_timeout: Duration,
    ) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        if config.transport != McpTransport::StreamableHttp {
            return Err(McpError::UnsupportedTransport(
                config.transport.label().to_owned(),
            ));
        }

        let request_timeout = request_timeout.max(Duration::from_millis(1));
        let url = Url::parse(config.url.trim())
            .map_err(|error| McpError::InvalidConfig(format!("invalid remote MCP URL: {error}")))?;
        // `config.validate()` above rejected the schemes this policy would also reject, without
        // touching the network. The check here is the one that needs a lookup: it resolves the
        // host and refuses loopback, private, link-local and metadata addresses unless the
        // operator opted in. A hostile DNS answer can still change between this check and the
        // request, which is why redirects are refused as well.
        let policy = remote_outbound_policy(local_servers_allowed());
        validate_outbound_url(&url, &policy).map_err(|error| {
            McpError::InvalidConfig(format!(
                "remote MCP URL `{}` is not allowed: {error}",
                remote_endpoint_label(&url)
            ))
        })?;

        let builder = HttpClient::builder()
            .connect_timeout(request_timeout.min(Duration::from_secs(15)))
            .timeout(request_timeout)
            .redirect(RedirectPolicy::none());
        let client = apply_runtime_proxy_async(builder)
            .and_then(|builder| builder.build().map_err(|error| error.to_string()))
            .map_err(McpError::Http)?;
        let headers = build_remote_headers(&config.headers)?;

        Ok(Self {
            client,
            url: config.url.trim().to_owned(),
            headers,
            sensitive_values: collect_sensitive_values(config.headers.values()),
            session_id: None,
            protocol_version: MCP_PREFERRED_PROTOCOL_VERSION.to_owned(),
            next_id: 1,
        })
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(None)
    }

    pub fn initialize_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(Some(cancellation))
    }

    fn initialize_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        for (version_index, protocol_version) in
            MCP_SUPPORTED_PROTOCOL_VERSIONS.iter().copied().enumerate()
        {
            self.protocol_version = protocol_version.to_owned();
            let id = self.next_request_id();
            let request = initialize_request_for_version(id, protocol_version);
            match self.send_message_with_cancellation(&request, Some(id), cancellation) {
                Ok(result) => {
                    self.protocol_version = validate_initialize_result(&result)?;
                    self.send_message_with_cancellation(
                        &initialized_notification(),
                        None,
                        cancellation,
                    )?;
                    return Ok(result);
                }
                Err(error)
                    if is_protocol_compatibility_rejection(&error)
                        && version_index + 1 < MCP_SUPPORTED_PROTOCOL_VERSIONS.len() =>
                {
                    // A rejected initialize request must not create a session. Refuse to carry a
                    // non-conforming response header into the next revision attempt regardless.
                    self.session_id = None;
                }
                Err(error) if is_protocol_compatibility_rejection(&error) => {
                    self.session_id = None;
                    return Err(no_common_protocol_error(&error));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("MCP supported protocol revision table is non-empty")
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(None)
    }

    pub fn list_tools_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(Some(cancellation))
    }

    fn list_tools_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.send_message_with_cancellation(&tools_list_request(id), Some(id), cancellation)
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, None)
    }

    pub fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: &AtomicBool,
    ) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, Some(cancellation))
    }

    fn call_tool_with_cancellation(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        validate_tool_call_payload(name, &arguments)?;
        let id = self.next_request_id();
        self.send_message_with_cancellation(
            &tools_call_request(id, name, arguments),
            Some(id),
            cancellation,
        )
    }

    pub fn cancel(&mut self) {
        // Streamable HTTP cancellation is request-scoped; the cancellable methods drop the
        // in-flight async request when their token fires. There is no child process to terminate.
    }

    pub fn close(&mut self) -> McpResult<()> {
        self.close_with_cancellation(None)
    }

    pub fn close_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<()> {
        self.close_with_cancellation(Some(cancellation))
    }

    fn close_with_cancellation(&mut self, cancellation: Option<&AtomicBool>) -> McpResult<()> {
        let Some(session_id) = self.session_id.as_deref() else {
            return Ok(());
        };
        let request = self
            .client
            .delete(&self.url)
            .header(ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.protocol_version)
            .header("MCP-Session-Id", session_id)
            .headers(self.headers.clone());
        let response = run_http_future(execute_http_request(request, cancellation))
            .map_err(|error| McpError::Http(format!("run MCP HTTP close request: {error}")))??;
        let status = response.status;
        if status.is_success()
            || status == reqwest::StatusCode::NOT_FOUND
            || status == reqwest::StatusCode::METHOD_NOT_ALLOWED
        {
            self.session_id = None;
            return Ok(());
        }
        Err(McpError::HttpStatus {
            status: status.as_u16(),
            body: bounded_error_body(&response.body, &self.sensitive_values),
        })
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_message_with_cancellation(
        &mut self,
        message: &JsonValue,
        expected_id: Option<u64>,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let mut request = self
            .client
            .post(&self.url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.protocol_version)
            .headers(self.headers.clone())
            .json(message);
        if let Some(session_id) = self.session_id.as_deref() {
            request = request.header("MCP-Session-Id", session_id);
        }

        let response = run_http_future(execute_http_request(request, cancellation))
            .map_err(|error| McpError::Http(format!("run MCP HTTP request: {error}")))??;
        if let Some(session_id) = response.session_id {
            self.session_id = Some(session_id);
        }
        let status = response.status;
        let content_type = response.content_type;
        let body = response.body;
        if !status.is_success() {
            return Err(McpError::HttpStatus {
                status: status.as_u16(),
                body: bounded_error_body(&body, &self.sensitive_values),
            });
        }
        if body.is_empty() {
            return expected_id.map_or_else(
                || Ok(JsonValue::Null),
                |id| {
                    Err(McpError::Protocol(format!(
                        "MCP HTTP response id {id} had an empty body"
                    )))
                },
            );
        }

        let messages = if content_type.contains("text/event-stream") {
            parse_sse_messages(&body)?
        } else {
            parse_json_messages(&body)?
        };
        match expected_id {
            Some(id) => result_from_messages(messages, id),
            None => Ok(JsonValue::Null),
        }
    }
}

pub(super) fn run_http_future<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = T> + Send,
    T: Send,
{
    thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| format!("create runtime: {error}"))?;
                Ok(runtime.block_on(future))
            })
            .join()
            .map_err(|_| "HTTP runtime thread panicked".to_owned())?
    })
}

pub(super) fn validate_remote_config(config: &McpServerConfig) -> McpResult<()> {
    let url = Url::parse(config.url.trim())
        .map_err(|error| McpError::InvalidConfig(format!("invalid remote MCP URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(McpError::InvalidConfig(
            "remote MCP URL must use http or https".to_owned(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(McpError::InvalidConfig(
            "remote MCP URL must not contain embedded credentials".to_owned(),
        ));
    }
    if url.host_str().is_none() || url.fragment().is_some() {
        return Err(McpError::InvalidConfig(
            "remote MCP URL must contain a host and no fragment".to_owned(),
        ));
    }
    if config.url.contains('{') || config.url.contains('}') {
        return Err(McpError::InvalidConfig(
            "remote MCP URL still contains unresolved template variables".to_owned(),
        ));
    }
    // Both maps mean secrets end up on the wire: `headers` holds the values that are sent, and
    // `credential_headers` names the vault entries the daemon resolves into them before a call.
    let credentialed = !config.headers.is_empty() || !config.credential_headers.is_empty();
    ensure_remote_scheme_allowed(&url, credentialed, local_servers_allowed())?;
    build_remote_headers(&config.headers).map(|_| ())
}

pub(super) fn build_remote_headers(headers: &BTreeMap<String, String>) -> McpResult<HeaderMap> {
    validate_mcp_headers(headers)?;
    let mut result = HeaderMap::new();
    for (name, value) in headers {
        let normalized = name.trim().to_ascii_lowercase();
        let header_name = HeaderName::from_bytes(normalized.as_bytes())
            .expect("validated remote MCP header name");
        let header_value = HeaderValue::from_str(value).expect("validated remote MCP header value");
        result.insert(header_name, header_value);
    }
    Ok(result)
}
