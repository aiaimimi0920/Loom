//! Internal implementation behind the crate-root MCP facade.

use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use loom_protocol::PackageTrustStatus;
use loom_security::network::{
    apply_runtime_proxy_async, host_is_loopback_literal, validate_outbound_url, OutboundPolicy,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::redirect::Policy as RedirectPolicy;
use reqwest::Client as HttpClient;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

mod client;
mod config;
mod diagnostics;
mod error;
mod http_client;
mod http_response;
mod protocol;
mod runtime;
mod spawn_windows;
mod stdio;
mod validation;

pub use client::*;
pub use config::*;
pub use error::*;
pub use http_client::*;
pub use protocol::*;
pub use runtime::*;
pub use stdio::*;
pub use validation::*;

use diagnostics::*;
use http_response::*;
use spawn_windows::*;

const MCP_REGISTRY_ENDPOINT: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";
pub const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2026-07-28", "2025-06-18", "2024-11-05"];
pub const MCP_PREFERRED_PROTOCOL_VERSION: &str = MCP_SUPPORTED_PROTOCOL_VERSIONS[0];

pub const MAX_MCP_SERVER_NAME_BYTES: usize = 256;
pub const MAX_MCP_SERVER_DESCRIPTION_BYTES: usize = 4 * 1024;
pub const MAX_MCP_ARGUMENTS: usize = 128;
pub const MAX_MCP_ARGUMENT_BYTES: usize = 8 * 1024;
pub const MAX_MCP_TOOLS: usize = 256;
pub const MAX_MCP_TOOL_ID_BYTES: usize = 128;
pub const MAX_MCP_CREDENTIALS: usize = 64;
pub const MAX_MCP_CREDENTIAL_LABEL_BYTES: usize = 256;
pub const MAX_MCP_ENVIRONMENT_ENTRIES: usize = 128;
pub const MAX_MCP_ENVIRONMENT_NAME_BYTES: usize = 128;
pub const MAX_MCP_ENVIRONMENT_VALUE_BYTES: usize = 32 * 1024;
pub const MAX_MCP_ENVIRONMENT_TOTAL_BYTES: usize = 24 * 1024;
pub const MAX_MCP_HEADERS: usize = 64;
pub const MAX_MCP_HEADER_NAME_BYTES: usize = 128;
pub const MAX_MCP_HEADER_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_MCP_HEADER_TOTAL_BYTES: usize = 128 * 1024;

/// Version of the MCP crate.
pub const LOOM_MCP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
