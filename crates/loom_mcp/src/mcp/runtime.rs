//! Process limits and outbound policy configuration.

use super::*;

pub(super) const DEFAULT_MCP_REQUEST_TIMEOUT_SECONDS: u64 = 60;
pub(super) const DEFAULT_MCP_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
pub(super) const MCP_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MCP_MAX_STDERR_BYTES: usize = 1024 * 1024;
pub(super) static MCP_REQUEST_TIMEOUT_SECONDS: AtomicU64 =
    AtomicU64::new(DEFAULT_MCP_REQUEST_TIMEOUT_SECONDS);
pub(super) static MCP_MEMORY_LIMIT_BYTES: AtomicU64 =
    AtomicU64::new(DEFAULT_MCP_MEMORY_LIMIT_BYTES);

/// Environment variable that lets an operator point a remote MCP server at their own machine.
pub(super) const MCP_LOCAL_SERVERS_ENV: &str = "LOOM_MCP_ALLOW_LOCAL_SERVERS";
pub(super) static MCP_ALLOW_LOCAL_SERVERS: AtomicBool = AtomicBool::new(false);

/// Applies process-wide defaults used by newly spawned MCP stdio clients.
pub fn configure_runtime_limits(request_timeout_seconds: u64, memory_limit_bytes: u64) {
    MCP_REQUEST_TIMEOUT_SECONDS.store(request_timeout_seconds.max(1), Ordering::Relaxed);
    MCP_MEMORY_LIMIT_BYTES.store(memory_limit_bytes.max(1), Ordering::Relaxed);
}

/// Allows remote MCP servers to address loopback and private networks.
///
/// This is off by default: a remote server URL can arrive from a package manifest that nobody
/// signed, and the credential headers configured for that server are attached to every request.
/// Pointing such a URL at `127.0.0.1`, at a LAN device, or at a cloud metadata endpoint turns
/// the daemon into a confused deputy, so those destinations require an explicit decision by the
/// operator, either through this call or by setting `LOOM_MCP_ALLOW_LOCAL_SERVERS=1`.
pub fn configure_local_servers(allowed: bool) {
    MCP_ALLOW_LOCAL_SERVERS.store(allowed, Ordering::Relaxed);
}

/// Report whether local and private destinations are currently allowed for remote MCP servers.
#[must_use]
pub fn local_servers_allowed() -> bool {
    MCP_ALLOW_LOCAL_SERVERS.load(Ordering::Relaxed) || environment_allows_local_servers()
}

pub(super) fn environment_allows_local_servers() -> bool {
    std::env::var(MCP_LOCAL_SERVERS_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// The outbound policy applied to every remote MCP request.
///
/// Remote MCP servers get the same policy the Art and cloud paths use, with redirects refused
/// outright: Streamable HTTP has no use for them, and a redirect is the cheapest way to move a
/// request holding operator credentials from an allowed host to a forbidden one.
pub(super) fn remote_outbound_policy(allow_local: bool) -> OutboundPolicy {
    OutboundPolicy {
        allow_http_loopback: allow_local,
        allow_private_networks: allow_local,
        allowed_domains: Vec::new(),
        max_redirects: 0,
    }
}

/// Reject a remote MCP URL whose scheme cannot protect what Loom is about to send.
///
/// This runs during configuration validation, so it must not perform a DNS lookup; the address
/// classes are checked in [`StreamableHttpMcpClient::connect_with_timeout`] instead, where a
/// lookup is already unavoidable. Keeping the two apart means saving a server never depends on
/// name resolution while connecting to one still refuses loopback, private and link-local
/// destinations.
pub(super) fn ensure_remote_scheme_allowed(
    url: &Url,
    credentialed: bool,
    allow_local: bool,
) -> McpResult<()> {
    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_local && host_is_loopback_literal(host) => Ok(()),
        "http" if credentialed => Err(McpError::InvalidConfig(format!(
            "remote MCP URL must use https because credential headers are attached; plain http \
             would send them in cleartext. Plain http is only accepted for a loopback \
             development endpoint, and only with {MCP_LOCAL_SERVERS_ENV}=1"
        ))),
        "http" => Err(McpError::InvalidConfig(format!(
            "remote MCP URL must use https; plain http is only accepted for a loopback \
             development endpoint, and only with {MCP_LOCAL_SERVERS_ENV}=1"
        ))),
        scheme => Err(McpError::InvalidConfig(format!(
            "remote MCP URL scheme `{scheme}` is not supported"
        ))),
    }
}

#[must_use]
pub fn runtime_limits() -> (u64, u64) {
    (
        MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed),
        MCP_MEMORY_LIMIT_BYTES.load(Ordering::Relaxed),
    )
}
