//! Transport-neutral MCP client facade.

use super::*;

/// Transport-neutral MCP client used by daemon and tool registry callers.
pub enum McpClient {
    Stdio(StdioMcpClient),
    StreamableHttp(StreamableHttpMcpClient),
}

impl McpClient {
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
        match config.transport {
            McpTransport::Stdio => {
                StdioMcpClient::spawn_with_timeout(config, request_timeout).map(Self::Stdio)
            }
            McpTransport::StreamableHttp => {
                StreamableHttpMcpClient::connect_with_timeout(config, request_timeout)
                    .map(Self::StreamableHttp)
            }
        }
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.initialize(),
            Self::StreamableHttp(client) => client.initialize(),
        }
    }

    pub fn initialize_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.initialize_cancellable(cancellation),
            Self::StreamableHttp(client) => client.initialize_cancellable(cancellation),
        }
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.list_tools(),
            Self::StreamableHttp(client) => client.list_tools(),
        }
    }

    pub fn list_tools_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.list_tools_cancellable(cancellation),
            Self::StreamableHttp(client) => client.list_tools_cancellable(cancellation),
        }
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.call_tool(name, arguments),
            Self::StreamableHttp(client) => client.call_tool(name, arguments),
        }
    }

    pub fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: &AtomicBool,
    ) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.call_tool_cancellable(name, arguments, cancellation),
            Self::StreamableHttp(client) => {
                client.call_tool_cancellable(name, arguments, cancellation)
            }
        }
    }

    pub fn close(&mut self) -> McpResult<()> {
        match self {
            Self::Stdio(client) => client.close(),
            Self::StreamableHttp(client) => client.close(),
        }
    }

    pub fn close_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<()> {
        match self {
            Self::Stdio(client) => client.close(),
            Self::StreamableHttp(client) => client.close_cancellable(cancellation),
        }
    }

    pub fn cancel(&mut self) {
        match self {
            Self::Stdio(client) => client.cancel(),
            Self::StreamableHttp(client) => client.cancel(),
        }
    }
}
