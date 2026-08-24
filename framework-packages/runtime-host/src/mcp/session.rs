// Thread-local MCP session reuse, eviction, and close ownership.
enum McpCallOutcome {
    Success(Value),
    Failure(String),
}

struct McpBatchExecution {
    outcomes: Vec<McpCallOutcome>,
    close_error: Option<String>,
}

// The MCP framework host itself and every stdio server descendant share the
// framework manifest's four-process Windows Job. A server commonly needs an
// interpreter plus its runtime, so caching more than one server can exhaust
// the Job before the replacement process starts. Repeated calls still reuse
// the matching session; a different immutable config replaces it first.
const MAX_CACHED_MCP_SESSIONS: usize = 1;
const MCP_SESSION_IDLE_LIFETIME: Duration = Duration::from_secs(60);

struct CachedMcpSession {
    key: String,
    client: McpClient,
    tools: Value,
    last_used: Instant,
}

impl Drop for CachedMcpSession {
    fn drop(&mut self) {
        let _ = self.client.close();
    }
}

// The persistent runtime host's `--serve` loop processes requests serially on
// one OS thread. Keeping the pool thread-local therefore makes this a
// process-host limit without requiring the transport client to be `Send`.
thread_local! {
    static MCP_SESSION_POOL: RefCell<Vec<CachedMcpSession>> = const { RefCell::new(Vec::new()) };
}

fn mcp_session_key(server: &McpServerConfig) -> Result<String, String> {
    let encoded = serde_json::to_vec(server)
        .map_err(|error| format!("cannot serialize MCP session identity: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

fn take_cached_mcp_session(key: &str) -> Option<CachedMcpSession> {
    let now = Instant::now();
    MCP_SESSION_POOL.with(|pool| {
        let mut sessions = pool.borrow_mut();
        sessions.retain(|session| {
            now.saturating_duration_since(session.last_used) < MCP_SESSION_IDLE_LIFETIME
        });
        sessions
            .iter()
            .position(|session| session.key == key)
            .map(|index| sessions.remove(index))
    })
}

fn return_cached_mcp_session(mut session: CachedMcpSession) {
    session.last_used = Instant::now();
    MCP_SESSION_POOL.with(|pool| {
        let mut sessions = pool.borrow_mut();
        if let Some(index) = sessions
            .iter()
            .position(|existing| existing.key == session.key)
        {
            sessions.remove(index);
        }
        sessions.push(session);
        if sessions.len() > MAX_CACHED_MCP_SESSIONS {
            let oldest = sessions
                .iter()
                .enumerate()
                .min_by_key(|(_, session)| session.last_used)
                .map(|(index, _)| index)
                .unwrap_or(0);
            sessions.remove(oldest);
        }
    });
}

fn evict_cached_mcp_sessions_before_connect() {
    MCP_SESSION_POOL.with(|pool| {
        let mut sessions = pool.borrow_mut();
        while sessions.len() >= MAX_CACHED_MCP_SESSIONS {
            let oldest = sessions
                .iter()
                .enumerate()
                .min_by_key(|(_, session)| session.last_used)
                .map(|(index, _)| index)
                .unwrap_or(0);
            sessions.remove(oldest);
        }
    });
}

fn acquire_mcp_session(server: &McpServerConfig) -> Result<CachedMcpSession, String> {
    let key = mcp_session_key(server)?;
    if let Some(session) = take_cached_mcp_session(&key) {
        return Ok(session);
    }
    // Close an idle session before spawning its replacement. Evicting only
    // after the new connection succeeds creates a transient process spike and
    // can make CreateProcess fail with ERROR_NOT_ENOUGH_QUOTA inside the Job.
    evict_cached_mcp_sessions_before_connect();
    let mut client = McpClient::connect(server)
        .map_err(|error| format!("failed to connect MCP server: {error}"))?;
    if let Err(error) = client.initialize() {
        let _ = client.close();
        return Err(format!("MCP initialize failed: {error}"));
    }
    let tools = match client.list_tools() {
        Ok(tools) => {
            if let Err(error) = validate_mcp_response_value(&tools, "MCP tools/list response") {
                let _ = client.close();
                return Err(error);
            }
            tools
        }
        Err(error) => {
            let _ = client.close();
            return Err(format!("MCP tools/list failed: {error}"));
        }
    };
    Ok(CachedMcpSession {
        key,
        client,
        tools,
        last_used: Instant::now(),
    })
}

#[cfg(test)]
fn clear_mcp_session_pool() {
    MCP_SESSION_POOL.with(|pool| pool.borrow_mut().clear());
}
