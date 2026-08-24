// MCP framework facade; implementation fragments share this private lexical boundary.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD as BASE64_URL_SAFE};
use base64::Engine as _;
use loom_mcp::{
    validate_mcp_environment, validate_mcp_environment_name, validate_mcp_header_name,
    validate_mcp_headers, McpClient, McpServerConfig, McpTransport, MAX_MCP_ENVIRONMENT_ENTRIES,
    MAX_MCP_HEADERS,
};
use loom_protocol::{CredentialGrant, FrameworkExecuteRequest, FrameworkMcpServer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

include!("mcp/model.rs");
include!("mcp/execute.rs");
include!("mcp/manifest.rs");
include!("mcp/version.rs");
include!("mcp/call_config.rs");
include!("mcp/surface_config.rs");
include!("mcp/transport_config.rs");
include!("mcp/call_resolution.rs");
include!("mcp/bindings.rs");
include!("mcp/argument_merge.rs");
include!("mcp/session.rs");
include!("mcp/tool_execution.rs");
include!("mcp/response_limits.rs");
include!("mcp/schema.rs");
include!("mcp/redaction.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use loom_protocol::FrameworkExecutionContext;
    use serde_json::json;
    #[cfg(windows)]
    use std::{
        io::BufReader,
        net::TcpListener,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };
    use std::{
        io::{BufRead, Write},
        path::PathBuf,
    };

    include!("mcp/tests/fixtures.rs");
    include!("mcp/tests/session.rs");
    include!("mcp/tests/arguments.rs");
    include!("mcp/tests/surface.rs");
    include!("mcp/tests/transport_policy.rs");
    include!("mcp/tests/dependencies.rs");
    include!("mcp/tests/allowlist.rs");
    include!("mcp/tests/schema.rs");
    include!("mcp/tests/redaction.rs");
    include!("mcp/tests/outcomes.rs");
    include!("mcp/tests/hardening.rs");
    include!("mcp/tests/image_search_windows.rs");
}
